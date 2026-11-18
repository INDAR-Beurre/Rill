// Every key the browser keeps for itself, in one place.
//
// The decision is separate from the doing: `Key::intent` takes a key name and
// three modifiers and answers what it means; `handle` carries it out. That
// split exists so the keyboard can be tested without a window, which it could
// not be while the two were the same function.
use crate::app::AppRef;
use crate::panels::PanelKind;
use gtk::gdk::ModifierType;
use webkit2gtk::WebViewExt;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Intent {
    Nothing,
    Omni,
    NewTab,
    NewPrivateTab,
    ReopenClosed,
    CloseTab,
    Reload,
    ReloadBypassCache,
    Reader,
    Bookmark,
    Find,
    History,
    Downloads,
    Bookmarks,
    Settings,
    Shortcuts,
    HideSomething,
    ShowHidden,
    Inspector,
    NextTab,
    PreviousTab,
    Back,
    Forward,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    Jump(usize),
    CopyAddress,
    Escape,
}

pub struct Key {
    /// The key's own name, before anything is folded.
    pub name: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Key {
    pub fn new(name: &str, ctrl: bool, shift: bool, alt: bool) -> Key {
        Key { name: name.to_string(), ctrl, shift, alt }
    }

    pub fn from_event(event: &gtk::gdk::EventKey) -> Key {
        let state = event.state();
        let name = event
            .keyval()
            .name()
            .map(|name| name.to_string())
            .unwrap_or_default();
        Key::new(
            &name,
            state.contains(ModifierType::CONTROL_MASK),
            state.contains(ModifierType::SHIFT_MASK),
            state.contains(ModifierType::MOD1_MASK),
        )
    }

    /// Shift is also spelled in the key itself: Ctrl Shift T arrives as `T`,
    /// not `t`. Everything below matches the plain name and reads `shift`
    /// separately, so the letter is folded down once, here.
    fn folded(&self) -> String {
        self.name.to_lowercase()
    }

    pub fn intent(&self) -> Intent {
        let name = self.folded();
        let name = name.as_str();

        if self.ctrl {
            let intent = match (name, self.shift) {
                ("l", _) => Intent::Omni,
                ("t", false) => Intent::NewTab,
                ("t", true) => Intent::ReopenClosed,
                ("n", false) => Intent::NewTab,
                ("n", true) => Intent::NewPrivateTab,
                ("w", _) => Intent::CloseTab,
                ("r", false) => Intent::Reload,
                ("r", true) => Intent::Reader,
                ("d", _) => Intent::Bookmark,
                ("f", _) => Intent::Find,
                ("y", _) => Intent::History,
                ("j", _) => Intent::Downloads,
                ("b", true) => Intent::Bookmarks,
                ("slash", _) => Intent::Shortcuts,
                ("comma", _) => Intent::Settings,
                ("h", true) => Intent::HideSomething,
                ("u", true) => Intent::ShowHidden,
                ("i", true) => Intent::Inspector,
                ("c", true) => Intent::CopyAddress,
                // Shift Tab is a keyval of its own, and Ctrl Shift Tab is the
                // one a browser keeps for the other direction.
                ("tab", false) | ("page_down", _) => Intent::NextTab,
                ("tab", true) | ("iso_left_tab", _) | ("page_up", _) => Intent::PreviousTab,
                ("bracketleft", _) => Intent::Back,
                ("bracketright", _) => Intent::Forward,
                ("plus", _) | ("equal", _) => Intent::ZoomIn,
                ("minus", _) => Intent::ZoomOut,
                ("0", _) => Intent::ZoomReset,
                (digit, _) if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() => {
                    match digit.parse::<usize>() {
                        Ok(number) => Intent::Jump(number),
                        Err(_) => Intent::Nothing,
                    }
                }
                _ => Intent::Nothing,
            };
            if intent != Intent::Nothing {
                return intent;
            }
        }

        if self.alt {
            match name {
                "left" => return Intent::Back,
                "right" => return Intent::Forward,
                _ => {}
            }
        }

        match name {
            "escape" => Intent::Escape,
            "f5" if self.ctrl => Intent::ReloadBypassCache,
            "f5" => Intent::Reload,
            "f12" => Intent::Inspector,
            _ => Intent::Nothing,
        }
    }
}

/// Carries an intent out. Split from `handle` so the bench can drive the same
/// table the keyboard does: a shortcut that stops working is caught by the
/// suite without a window and without a fake key event.
pub fn apply(app: &AppRef, intent: Intent) {
    match intent {
        Intent::Nothing => {}
        Intent::Escape => {
            if app.omni.visible.get() {
                app.close_omni();
            } else if app.find.borrow().visible {
                app.toggle_find();
            } else if app.panels.borrow().open.is_some() {
                app.close_panels();
            } else if app.reading.get() {
                // Reading mode is the last thing standing between Escape and
                // the page, so it is the last thing Escape takes away.
                app.reader();
            }
        }
        Intent::Omni => app.open_omni(true),
        Intent::NewTab => {
            app.new_tab(None, false, false);
        }
        Intent::NewPrivateTab => {
            app.new_tab(None, true, false);
        }
        Intent::ReopenClosed => app.reopen_closed(),
        Intent::CloseTab => app.close_tab(app.active_id()),
        Intent::Reload => app.reload(),
        Intent::ReloadBypassCache => {
            if let Some(view) = app.view(app.active_id()) {
                view.reload_bypass_cache();
            }
        }
        Intent::Reader => app.reader(),
        Intent::Bookmark => app.add_bookmark(),
        Intent::Find => app.toggle_find(),
        Intent::History => app.toggle_panel(PanelKind::History),
        Intent::Downloads => app.toggle_panel(PanelKind::Downloads),
        Intent::Bookmarks => app.toggle_panel(PanelKind::Bookmarks),
        Intent::Settings => app.toggle_panel(PanelKind::Settings),
        Intent::Shortcuts => app.toggle_panel(PanelKind::Shortcuts),
        Intent::HideSomething => app.hide_element_mode(),
        Intent::ShowHidden => app.unhide_all(),
        Intent::Inspector => app.inspector(),
        Intent::NextTab => app.cycle(true),
        Intent::PreviousTab => app.cycle(false),
        Intent::Back => app.go_back(),
        Intent::Forward => app.go_forward(),
        Intent::ZoomIn => app.zoom(0.1),
        Intent::ZoomOut => app.zoom(-0.1),
        Intent::ZoomReset => app.reset_zoom(),
        Intent::Jump(number) => app.jump(number),
        Intent::CopyAddress => app.copy_address(),
    }
}

/// True when the browser should swallow the key rather than let the page have
/// it. Escape is the one that can be nobody's: an Escape with nothing open
/// belongs to the page.
pub fn taken(app: &AppRef, intent: Intent) -> bool {
    match intent {
        Intent::Nothing => false,
        Intent::Escape => {
            app.omni.visible.get()
                || app.find.borrow().visible
                || app.panels.borrow().open.is_some()
                || app.reading.get()
        }
        _ => true,
    }
}

pub fn handle(app: &AppRef, event: &gtk::gdk::EventKey) -> glib::Propagation {
    let intent = Key::from_event(event).intent();
    apply(app, intent);
    crate::gtkx::inh(taken(app, intent))
}

#[cfg(test)]
mod tests {
    use super::{Intent, Key};

    /// The name a key wears when Shift is held: GDK reports the upper case
    /// letter, which is what made every Shift shortcut dead.
    fn key(name: &str, ctrl: bool, shift: bool, alt: bool) -> Key {
        Key::new(name, ctrl, shift, alt)
    }

    #[test]
    fn shift_shortcuts_survive_the_upper_case_keyval() {
        assert_eq!(key("T", true, true, false).intent(), Intent::ReopenClosed);
        assert_eq!(key("R", true, true, false).intent(), Intent::Reader);
        assert_eq!(key("B", true, true, false).intent(), Intent::Bookmarks);
        assert_eq!(key("H", true, true, false).intent(), Intent::HideSomething);
        assert_eq!(key("U", true, true, false).intent(), Intent::ShowHidden);
        assert_eq!(key("I", true, true, false).intent(), Intent::Inspector);
        assert_eq!(key("C", true, true, false).intent(), Intent::CopyAddress);
        assert_eq!(key("N", true, true, false).intent(), Intent::NewPrivateTab);
    }

    #[test]
    fn a_plain_ctrl_key_is_not_a_shift_key() {
        assert_eq!(key("t", true, false, false).intent(), Intent::NewTab);
        assert_eq!(key("n", true, false, false).intent(), Intent::NewTab);
        assert_eq!(key("r", true, false, false).intent(), Intent::Reload);
        assert_eq!(key("b", true, false, false).intent(), Intent::Nothing);
    }

    #[test]
    fn tab_steps_both_ways() {
        assert_eq!(key("Tab", true, false, false).intent(), Intent::NextTab);
        assert_eq!(key("Tab", true, true, false).intent(), Intent::PreviousTab);
        assert_eq!(key("ISO_Left_Tab", true, true, false).intent(), Intent::PreviousTab);
    }

    #[test]
    fn digits_jump_and_zero_resets_the_zoom() {
        assert_eq!(key("1", true, false, false).intent(), Intent::Jump(1));
        assert_eq!(key("9", true, false, false).intent(), Intent::Jump(9));
        // Ctrl Shift 0 resets zoom in every browser that has zoom, so it is
        // not "nothing": only a digit other than zero jumps.
        assert_eq!(key("0", true, false, false).intent(), Intent::ZoomReset);
        assert_eq!(key("0", true, true, false).intent(), Intent::ZoomReset);
    }

    #[test]
    fn f5_reloads_and_ctrl_f5_skips_the_cache() {
        assert_eq!(key("F5", false, false, false).intent(), Intent::Reload);
        assert_eq!(key("F5", true, false, false).intent(), Intent::ReloadBypassCache);
    }

    #[test]
    fn alt_arrows_go_through_history() {
        assert_eq!(key("Left", false, false, true).intent(), Intent::Back);
        assert_eq!(key("Right", false, false, true).intent(), Intent::Forward);
        assert_eq!(key("Left", false, false, false).intent(), Intent::Nothing);
    }
}
