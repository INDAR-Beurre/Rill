// Native Linux window decoration negotiation and gtk-decoration-layout parsing.
//
// Avoids forcing Client-Side Decorations when a server-side decorating WM
// (Openbox, KWin, Xfwm4, etc.) already provides native titlebar controls, and
// honors GTK's `gtk-decoration-layout` button side & ordering whenever custom
// browser controls are drawn.

use crate::store::WindowControlsMode;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlButton {
    Minimize,
    Maximize,
    Close,
}

#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct ParsedLayout {
    pub left: Vec<ControlButton>,
    pub right: Vec<ControlButton>,
}

impl ParsedLayout {
    pub fn is_empty(&self) -> bool {
        self.left.is_empty() && self.right.is_empty()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct DecorationPlan {
    /// True when the OS / window manager should draw its native titlebar frame.
    pub native_decorated: bool,
    /// Browser-drawn window control buttons on the left of the top tab strip.
    pub left_controls: Vec<ControlButton>,
    /// Browser-drawn window control buttons on the right of the top tab strip.
    pub right_controls: Vec<ControlButton>,
}

impl DecorationPlan {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn has_browser_controls(&self) -> bool {
        !self.left_controls.is_empty() || !self.right_controls.is_empty()
    }
}

/// Parse GTK3's documented `gtk-decoration-layout` string (`"left:right"`).
///
/// Recognises `minimize`, `maximize`, and `close` on either side of the colon;
/// ignores non-window-control tokens (`menu`, `icon`, `appmenu`, `spacer`, etc.)
/// and duplicates. Note: this property describes button layout when drawing
/// CSD, NOT proof that server-side decorations exist.
pub fn parse_decoration_layout(raw: &str) -> ParsedLayout {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return ParsedLayout::default();
    }
    let (left_raw, right_raw) = match trimmed.split_once(':') {
        Some((l, r)) => (l, r),
        None => (trimmed, ""),
    };
    ParsedLayout {
        left: parse_side(left_raw),
        right: parse_side(right_raw),
    }
}

fn parse_side(side: &str) -> Vec<ControlButton> {
    let mut out = Vec::new();
    for token in side.split(',') {
        let button = match token.trim().to_ascii_lowercase().as_str() {
            "minimize" => Some(ControlButton::Minimize),
            "maximize" => Some(ControlButton::Maximize),
            "close" => Some(ControlButton::Close),
            _ => None,
        };
        if let Some(button) = button {
            if !out.contains(&button) {
                out.push(button);
            }
        }
    }
    out
}

fn default_right_controls() -> Vec<ControlButton> {
    vec![
        ControlButton::Minimize,
        ControlButton::Maximize,
        ControlButton::Close,
    ]
}

/// Decide native decoration flag and custom browser control placement.
///
/// - `System` on X11 with an active EWMH WM: use real native server-side
///   decorations (`native_decorated = true`) and hide custom browser controls
///   so only one set of window controls ever appears.
/// - `System` on Wayland (where GTK3 does not negotiate `xdg-decoration` SSD
///   and `set_decorated(true)` without a custom titlebar synthesises a giant
///   fallback Adwaita bar): use `native_decorated = false` and follow
///   `gtk-decoration-layout` (so empty `""` or `":"` hides buttons cleanly).
/// - `Browser`: always draw browser controls inside the tab strip, following
///   `gtk-decoration-layout` when non-empty, or right-aligned min/max/close.
/// - `Hidden`: suppress native titlebar and custom browser controls (safe
///   close remains in Browser menu, Alt+F4, and Ctrl+W).
pub fn plan_decorations(
    mode: WindowControlsMode,
    is_x11: bool,
    x11_has_wm: bool,
    gtk_layout: Option<&str>,
) -> DecorationPlan {
    match mode {
        WindowControlsMode::Hidden => DecorationPlan {
            native_decorated: false,
            left_controls: Vec::new(),
            right_controls: Vec::new(),
        },
        WindowControlsMode::Browser => {
            let parsed = gtk_layout.map(parse_decoration_layout).unwrap_or_default();
            if parsed.is_empty() {
                DecorationPlan {
                    native_decorated: false,
                    left_controls: Vec::new(),
                    right_controls: default_right_controls(),
                }
            } else {
                DecorationPlan {
                    native_decorated: false,
                    left_controls: parsed.left,
                    right_controls: parsed.right,
                }
            }
        }
        WindowControlsMode::System => {
            if is_x11 && x11_has_wm {
                DecorationPlan {
                    native_decorated: true,
                    left_controls: Vec::new(),
                    right_controls: Vec::new(),
                }
            } else if is_x11 {
                // Bare X11 (e.g., Xvfb with no WM): no server frame exists.
                let parsed = gtk_layout.map(parse_decoration_layout).unwrap_or_default();
                if parsed.is_empty() {
                    DecorationPlan {
                        native_decorated: false,
                        left_controls: Vec::new(),
                        right_controls: default_right_controls(),
                    }
                } else {
                    DecorationPlan {
                        native_decorated: false,
                        left_controls: parsed.left,
                        right_controls: parsed.right,
                    }
                }
            } else {
                // Wayland: honor gtk-decoration-layout literally (including empty).
                match gtk_layout {
                    Some(raw) => {
                        let parsed = parse_decoration_layout(raw);
                        DecorationPlan {
                            native_decorated: false,
                            left_controls: parsed.left,
                            right_controls: parsed.right,
                        }
                    }
                    None => DecorationPlan {
                        native_decorated: false,
                        left_controls: Vec::new(),
                        right_controls: default_right_controls(),
                    },
                }
            }
        }
    }
}

/// Inspect the live GDK display/screen and GTK settings to build the current
/// decoration plan.
pub fn live_plan(mode: WindowControlsMode) -> DecorationPlan {
    use gtk::prelude::*;
    let is_x11 = gtk::gdk::Display::default()
        .map(|d| d.type_().name().contains("X11"))
        .unwrap_or(false);
    let x11_has_wm = if is_x11 {
        gtk::gdk::Screen::default()
            .and_then(|screen| screen.root_window())
            .is_some_and(|root| {
                let check = gtk::gdk::Atom::intern("_NET_SUPPORTING_WM_CHECK");
                let win_atom = gtk::gdk::Atom::intern("WINDOW");
                let supported = gtk::gdk::Atom::intern("_NET_SUPPORTED");
                let atom_atom = gtk::gdk::Atom::intern("ATOM");
                gtk::gdk::property_get(&root, &check, &win_atom, 0, 16, 0).is_some()
                    || gtk::gdk::property_get(&root, &supported, &atom_atom, 0, 1024, 0).is_some()
            })
    } else {
        false
    };
    let layout = gtk::Settings::default()
        .and_then(|s| s.gtk_decoration_layout())
        .map(|s| s.to_string());
    plan_decorations(mode, is_x11, x11_has_wm, layout.as_deref())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_standard_left_right_and_empty_gtk_decoration_layouts() {
        let right = parse_decoration_layout("menu:minimize,maximize,close");
        assert!(right.left.is_empty());
        assert_eq!(
            right.right,
            vec![ControlButton::Minimize, ControlButton::Maximize, ControlButton::Close]
        );

        let left = parse_decoration_layout("close,minimize,maximize:");
        assert_eq!(
            left.left,
            vec![ControlButton::Close, ControlButton::Minimize, ControlButton::Maximize]
        );
        assert!(left.right.is_empty());

        let split = parse_decoration_layout("close:minimize,maximize");
        assert_eq!(split.left, vec![ControlButton::Close]);
        assert_eq!(split.right, vec![ControlButton::Minimize, ControlButton::Maximize]);

        assert!(parse_decoration_layout("").is_empty());
        assert!(parse_decoration_layout(":").is_empty());
        assert!(parse_decoration_layout("icon,menu:").is_empty());
    }

    #[test]
    fn x11_wm_system_mode_uses_native_frame_with_zero_duplicate_browser_controls() {
        let plan = plan_decorations(
            WindowControlsMode::System,
            true,
            true,
            Some("menu:minimize,maximize,close"),
        );
        assert!(plan.native_decorated);
        assert!(!plan.has_browser_controls());
    }

    #[test]
    fn wayland_system_mode_honors_empty_and_left_side_layouts() {
        let empty = plan_decorations(WindowControlsMode::System, false, false, Some(""));
        assert!(!empty.native_decorated);
        assert!(!empty.has_browser_controls());

        let left = plan_decorations(
            WindowControlsMode::System,
            false,
            false,
            Some("close,minimize,maximize:menu"),
        );
        assert!(!left.native_decorated);
        assert_eq!(
            left.left_controls,
            vec![ControlButton::Close, ControlButton::Minimize, ControlButton::Maximize]
        );
        assert!(left.right_controls.is_empty());
    }

    #[test]
    fn browser_and_hidden_overrides_are_deterministic() {
        let browser = plan_decorations(WindowControlsMode::Browser, true, true, Some(""));
        assert!(!browser.native_decorated);
        assert_eq!(
            browser.right_controls,
            vec![ControlButton::Minimize, ControlButton::Maximize, ControlButton::Close]
        );

        let browser_left = plan_decorations(
            WindowControlsMode::Browser,
            true,
            true,
            Some("close,maximize:"),
        );
        assert!(!browser_left.native_decorated);
        assert_eq!(browser_left.left_controls, vec![ControlButton::Close, ControlButton::Maximize]);

        let hidden = plan_decorations(
            WindowControlsMode::Hidden,
            true,
            true,
            Some("menu:minimize,maximize,close"),
        );
        assert!(!hidden.native_decorated);
        assert!(!hidden.has_browser_controls());
    }
}
