// Every key the browser keeps for itself, in one place.
use crate::app::AppRef;
use crate::panels::PanelKind;
use gtk::gdk::ModifierType;
use webkit2gtk::WebViewExt;

pub fn handle(app: &AppRef, event: &gtk::gdk::EventKey) -> glib::Propagation {
    let state = event.state();
    let ctrl = state.contains(ModifierType::CONTROL_MASK);
    let shift = state.contains(ModifierType::SHIFT_MASK);
    let alt = state.contains(ModifierType::MOD1_MASK);
    let key = event.keyval();
    let name = key.name().map(|n| n.to_string()).unwrap_or_default();
    let name = name.as_str();

    if ctrl {
        match (name, shift) {
            ("l", _) => {
                app.open_omni(true);
                return crate::gtkx::inh(true);
            }
            ("t", false) => {
                app.new_tab(None, false, false);
                return crate::gtkx::inh(true);
            }
            ("t", true) => {
                app.reopen_closed();
                return crate::gtkx::inh(true);
            }
            ("n", false) => {
                app.new_tab(None, false, false);
                return crate::gtkx::inh(true);
            }
            ("n", true) => {
                app.new_tab(None, true, false);
                return crate::gtkx::inh(true);
            }
            ("w", _) => {
                app.close_tab(app.active_id());
                return crate::gtkx::inh(true);
            }
            ("r", false) => {
                app.reload();
                return crate::gtkx::inh(true);
            }
            ("r", true) => {
                app.reader();
                return crate::gtkx::inh(true);
            }
            ("d", _) => {
                app.add_bookmark();
                return crate::gtkx::inh(true);
            }
            ("f", _) => {
                app.toggle_find();
                return crate::gtkx::inh(true);
            }
            ("y", _) => {
                app.toggle_panel(PanelKind::History);
                return crate::gtkx::inh(true);
            }
            ("j", _) => {
                app.toggle_panel(PanelKind::Downloads);
                return crate::gtkx::inh(true);
            }
            ("b", true) => {
                app.toggle_panel(PanelKind::Bookmarks);
                return crate::gtkx::inh(true);
            }
            ("slash", _) => {
                app.toggle_panel(crate::panels::PanelKind::Shortcuts);
                return crate::gtkx::inh(true);
            }
            ("comma", _) => {
                app.toggle_panel(PanelKind::Settings);
                return crate::gtkx::inh(true);
            }
            ("h", true) => {
                app.hide_element_mode();
                return crate::gtkx::inh(true);
            }
            ("u", true) => {
                app.unhide_all();
                return crate::gtkx::inh(true);
            }
            ("i", true) => {
                app.inspector();
                return crate::gtkx::inh(true);
            }
            ("Tab", _) => {
                app.cycle(!shift);
                return crate::gtkx::inh(true);
            }
            ("Page_Down", _) => {
                app.cycle(true);
                return crate::gtkx::inh(true);
            }
            ("Page_Up", _) => {
                app.cycle(false);
                return crate::gtkx::inh(true);
            }
            ("bracketleft", _) => {
                app.go_back();
                return crate::gtkx::inh(true);
            }
            ("bracketright", _) => {
                app.go_forward();
                return crate::gtkx::inh(true);
            }
            ("plus", _) | ("equal", _) => {
                app.zoom(0.1);
                return crate::gtkx::inh(true);
            }
            ("minus", _) => {
                app.zoom(-0.1);
                return crate::gtkx::inh(true);
            }
            ("0", _) => {
                app.reset_zoom();
                return crate::gtkx::inh(true);
            }
            (digit, _) if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() && digit != "0" => {
                if let Ok(n) = digit.parse::<usize>() {
                    app.jump(n);
                }
                return crate::gtkx::inh(true);
            }
            ("c", true) => {
                app.copy_address();
                return crate::gtkx::inh(true);
            }
            _ => {}
        }
    }

    if alt {
        match name {
            "Left" => {
                app.go_back();
                return crate::gtkx::inh(true);
            }
            "Right" => {
                app.go_forward();
                return crate::gtkx::inh(true);
            }
            _ => {}
        }
    }

    match name {
        "Escape" => {
            if app.omni.visible.get() {
                app.close_omni();
                return crate::gtkx::inh(true);
            }
            if app.find.borrow().visible {
                app.toggle_find();
                return crate::gtkx::inh(true);
            }
            if app.panels.borrow().open.is_some() {
                app.close_panels();
                return crate::gtkx::inh(true);
            }
            // Reading mode is the last thing standing between Escape and the
            // page, so it is the last thing Escape takes away.
            if app.reading.get() {
                app.reader();
                return crate::gtkx::inh(true);
            }
        }
        "F5" => {
            if event.state().contains(ModifierType::CONTROL_MASK) {
                if let Some(view) = app.view(app.active_id()) {
                    view.reload_bypass_cache();
                }
            } else {
                app.reload();
            }
            return crate::gtkx::inh(true);
        }
        "F12" => {
            app.inspector();
            return crate::gtkx::inh(true);
        }
        _ => {}
    }
    crate::gtkx::inh(false)
}
