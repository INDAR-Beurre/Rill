// Every key the browser keeps for itself, in one place.
//
// The decision is separate from the doing: `Key::intent` takes a key name and
// three modifiers and answers what it means; `handle` carries it out. That
// split exists so the keyboard can be tested without a window, which it could
// not be while the two were the same function.
use crate::app::AppRef;
use crate::panels::PanelKind;
use gtk::gdk::ModifierType;
use gtk::prelude::*;
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
    FindNext,
    FindPrevious,
    History,
    Downloads,
    Bookmarks,
    Settings,
    Shortcuts,
    Print,
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
    ViewSource,
    FullScreen,
    PanelLeft,
    PanelRight,
    PanelFocus,
    /// Ctrl K: the field, showing only open tabs, so typing a few letters of
    /// a title and pressing Return switches to that tab.
    SwitchTab,
    /// Ctrl Shift K: the same page again, in a new tab beside this one.
    DuplicateTab,
    /// Ctrl I: the site card for the page on screen.
    SiteCard,
    /// Ctrl S: fold the tab column to a strip, or open it again.
    FoldColumn,
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

        if !self.ctrl && !self.alt && name == "f11" {
            return Intent::FullScreen;
        }

        // Ctrl with Alt held is not a browser shortcut. On many layouts AltGr
        // arrives as Ctrl+Alt, so AltGr+L, AltGr+T or AltGr+7 is someone typing
        // a character such as @, | or {. Matching those here opened the field,
        // a new tab or jumped tabs in the middle of typing. They now reach the
        // page or the field, and the `self.alt` check below returns Nothing for
        // any that are left over.
        if self.ctrl && !self.alt {
            let intent = match (name, self.shift) {
                ("left", false) => Intent::PanelLeft,
                ("right", false) => Intent::PanelRight,
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
                ("h", false) => Intent::History,
                ("j", _) => Intent::Downloads,
                ("b", true) => Intent::Bookmarks,
                ("slash", _) => Intent::Shortcuts,
                ("comma", _) => Intent::Settings,
                ("h", true) => Intent::HideSomething,
                ("u", true) => Intent::ShowHidden,
                ("i", true) => Intent::Inspector,
                ("i", false) => Intent::SiteCard,
                ("u", false) => Intent::ViewSource,
                ("c", true) => Intent::CopyAddress,
                // Shift Tab is a keyval of its own, and Ctrl Shift Tab is the
                // one a browser keeps for the other direction.
                ("tab", false) | ("page_down", _) => Intent::NextTab,
                ("tab", true) | ("iso_left_tab", _) | ("page_up", _) => Intent::PreviousTab,
                ("bracketleft", _) => Intent::Back,
                ("bracketright", _) => Intent::Forward,
                ("plus", _) | ("equal", _) | ("kp_add", _) => Intent::ZoomIn,
                ("minus", _) | ("kp_subtract", _) => Intent::ZoomOut,
                ("0", _) | ("kp_0", _) => Intent::ZoomReset,
                ("p", false) => Intent::Print,
                ("k", false) => Intent::SwitchTab,
                ("k", true) => Intent::DuplicateTab,
                ("s", false) => Intent::FoldColumn,
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

        // Plain Alt+arrow is back and forward. Ctrl+Alt+arrow belongs to the
        // desktop (switching workspaces on most setups), and AltGr reports as
        // Ctrl+Alt on some layouts, so neither should move the page through
        // its history.
        if self.alt && !self.ctrl {
            match name {
                "left" => return Intent::Back,
                "right" => return Intent::Forward,
                _ => {}
            }
        }

        if self.alt || (self.ctrl && name != "f5") {
            return Intent::Nothing;
        }
        if !self.ctrl && !self.alt && name == "tab" {
            return Intent::PanelFocus;
        }
        match name {
            "escape" => Intent::Escape,
            "f5" if self.ctrl => Intent::ReloadBypassCache,
            "f5" => Intent::Reload,
            "f3" if self.shift => Intent::FindPrevious,
            "f3" => Intent::FindNext,
            "f12" => Intent::Inspector,
            _ => Intent::Nothing,
        }
    }
}

/// Every key the browser keeps, in one table. The panel draws it, the bench
/// presses it by name, and a test asserts each row still means something, so
/// the three can no longer drift apart.
///
/// (bench verb, label, what it does, keyval name, ctrl, shift, alt)
pub const SHEET: &[(&str, &str, &str, &str, bool, bool, bool)] = &[
    ("shortcuts", "Ctrl /", "This list", "slash", true, false, false),
    ("omni", "Ctrl L", "The field", "l", true, false, false),
    ("newtab", "Ctrl T", "New tab", "t", true, false, false),
    // Ctrl N is the second spelling of a new tab, and it works: the panel is
    // wrong if it lists one way and not the other.
    ("newtabn", "Ctrl N", "New tab", "n", true, false, false),
    ("privatetab", "Ctrl Shift N", "Private tab", "n", true, true, false),
    ("closetab", "Ctrl W", "Close tab", "w", true, false, false),
    // Shift rides along ignored in the ctrl `w` arm, so the key a real browser
    // spends on the window closes the tab here. It works, so it is listed.
    ("closetabshift", "Ctrl Shift W", "Close tab", "w", true, true, false),
    ("reopen", "Ctrl Shift T", "Reopen the last closed tab", "t", true, true, false),
    ("nexttab", "Ctrl Tab", "Next tab", "tab", true, false, false),
    ("previoustab", "Ctrl Shift Tab", "Previous tab", "tab", true, true, false),
    ("jump", "Ctrl 1 to 9", "Jump to a tab", "3", true, false, false),
    ("switchtab", "Ctrl K", "Switch to a tab by typing its name", "k", true, false, false),
    ("duplicatetab", "Ctrl Shift K", "Duplicate this tab", "k", true, true, false),
    ("foldcolumn", "Ctrl S", "Fold the tab column to a strip, or open it", "s", true, false, false),
    ("back", "Alt Left", "Back", "left", false, false, true),
    ("forward", "Alt Right", "Forward", "right", false, false, true),
    ("reload", "Ctrl R", "Reload", "r", true, false, false),
    ("f5", "F5", "Reload", "f5", false, false, false),
    ("reloadbypass", "Ctrl F5", "Reload past the cache", "f5", true, false, false),
    ("reader", "Ctrl Shift R", "Reading mode (again to leave)", "r", true, true, false),
    ("find", "Ctrl F", "Find in the page", "f", true, false, false),
    ("findnext", "F3", "Next match", "f3", false, false, false),
    ("findprevious", "Shift F3", "Previous match", "f3", false, true, false),
    ("bookmark", "Ctrl D", "Keep this page", "d", true, false, false),
    ("history", "Ctrl Y", "History", "y", true, false, false),
    // Ctrl H is the second spelling of History, the way Ctrl N is of a new tab.
    ("historyh", "Ctrl H", "History", "h", true, false, false),
    ("bookmarks", "Ctrl Shift B", "Bookmarks", "b", true, true, false),
    ("downloads", "Ctrl J", "Downloads", "j", true, false, false),
    ("settings", "Ctrl ,", "Settings", "comma", true, false, false),
    ("hide", "Ctrl Shift H", "Hide something on this site", "h", true, true, false),
    ("unhide", "Ctrl Shift U", "Show what is hidden here", "u", true, true, false),
    ("copy", "Ctrl Shift C", "Copy the address", "c", true, true, false),
    ("inspector", "F12", "The inspector", "f12", false, false, false),
    ("inspectorctrl", "Ctrl Shift I", "The inspector", "i", true, true, false),
    ("sitecard", "Ctrl I", "About this site: connection, permissions, zoom, data", "i", true, false, false),
    ("source", "Ctrl U", "The page's own source", "u", true, false, false),
    ("print", "Ctrl P", "Print this page", "p", true, false, false),
    ("fullscreen", "F11", "Full screen", "f11", false, false, false),
    ("pagedown", "Ctrl Page Down", "Next tab", "page_down", true, false, false),
    ("pageup", "Ctrl Page Up", "Previous tab", "page_up", true, false, false),
    // `+` needs Shift on most layouts, so the key that lands here is usually
    // `equal`. Both are taken; the label says so instead of adding a row that
    // would read as a second, different shortcut.
    ("zoomin", "Ctrl + or Ctrl =", "Zoom in", "plus", true, false, false),
    ("zoomout", "Ctrl -", "Zoom out", "minus", true, false, false),
    ("zoomreset", "Ctrl 0", "Zoom reset", "0", true, false, false),
    ("backbracket", "Ctrl [", "Back", "bracketleft", true, false, false),
    ("forwardbracket", "Ctrl ]", "Forward", "bracketright", true, false, false),
    // Escape has five jobs and the panel has to own all of them: whatever the
    // browser drew, else a page that is still arriving.
    ("escape", "Escape", "Close the field, find, a panel or reader, else stop the page", "escape", false, false, false),
    ("panelleft", "Ctrl Left", "Previous panel", "left", true, false, false),
    ("panelright", "Ctrl Right", "Next panel", "right", true, false, false),
    ("panelfocus", "Tab", "Focus the panel", "tab", false, false, false),
];

/// The intent a bench verb stands for, through the same fold the keyboard uses.
pub fn intent_of(verb: &str) -> Option<Intent> {
    let row = SHEET.iter().find(|row| row.0 == verb)?;
    Some(Key::new(row.3, row.4, row.5, row.6).intent())
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
            } else {
                // Nothing the browser drew is open: stop a page that is still
                // arriving, otherwise the key belongs to the page.
                let active = app.state.borrow().active;
                let loading = app.state.borrow().tab(active).is_some_and(|t| t.loading);
                if loading {
                    if let Some(view) = app.view(active) {
                        view.stop_loading();
                    }
                    app.set_loading(active, false);
                }
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
        Intent::FindNext => {
            let text = app.find.borrow().entry.text();
            app.find_step(&text, true);
        }
        Intent::FindPrevious => {
            let text = app.find.borrow().entry.text();
            app.find_step(&text, false);
        }
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
        Intent::ViewSource => app.view_source(),
        Intent::Print => app.print_page(),
        Intent::FullScreen => app.toggle_fullscreen(),
        Intent::PanelLeft => app.panel_arrow(false),
        Intent::PanelRight => app.panel_arrow(true),
        Intent::PanelFocus => app.panel_focus(),
        Intent::SwitchTab => app.open_tab_switcher(),
        Intent::DuplicateTab => app.duplicate_tab(),
        Intent::FoldColumn => app.toggle_fold(),
        Intent::SiteCard => crate::sitecard::open(app),
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
                || app.state.borrow().tab(app.state.borrow().active).is_some_and(|t| t.loading)
        }
        Intent::PanelLeft | Intent::PanelRight => {
            arrow_belongs_to_panel(app.panels.borrow().open.is_some(), focus_in_entry(app))
        }
        Intent::PanelFocus => {
            tab_belongs_to_panel(app.panels.borrow().open.is_some(), focus_in_panel(app))
        }
        _ => true,
    }
}

/// Escape's first three jobs are the address field, the find bar, and any open
/// panel. Only when none of them is open may a card (permission, sign-in,
/// first-run, peek, site card) take it.
pub fn escape_reaches_cards(field_open: bool, find_open: bool, panel_open: bool) -> bool {
    !field_open && !find_open && !panel_open
}

/// Tab moves the panel's focus only when focus is outside the panel. Inside,
/// it is GTK's to move, so Tab can walk the controls instead of looping.
pub fn tab_belongs_to_panel(panel_open: bool, focus_inside: bool) -> bool {
    panel_open && !focus_inside
}

/// Ctrl+Left / Ctrl+Right switch panels only when focus is not inside a text
/// entry, so search and keyword fields keep word-by-word cursor movement.
pub fn arrow_belongs_to_panel(panel_open: bool, focus_in_entry: bool) -> bool {
    panel_open && !focus_in_entry
}

fn focus_in_panel(app: &AppRef) -> bool {
    let card = app.panels.borrow().card.clone();
    app.ui.window.focused_widget().is_some_and(|widget| widget.is_ancestor(&card))
}

fn focus_in_entry(app: &AppRef) -> bool {
    app.ui.window.focused_widget().is_some_and(|widget| widget.is::<gtk::Entry>())
}

pub fn handle(app: &AppRef, event: &gtk::gdk::EventKey) -> glib::Propagation {
    let field_open = app.omni.visible.get();
    let find_open = app.find.borrow().visible;
    let panel_open = app.panels.borrow().open.is_some();
    let chrome_open = field_open || find_open || panel_open;
    let intent = Key::from_event(event).intent();

    if !chrome_open {
        // A dialog on screen takes Escape and Enter only when no browser
        // chrome (omnibox, find bar, panel) is open above it.
        if crate::dialogs::key(app, event) {
            return crate::gtkx::inh(true);
        }
        // Escape answers the cards only after the field, find bar, and panels.
        if intent == Intent::Escape
            && escape_reaches_cards(field_open, find_open, panel_open)
            && (crate::permissions::key(app, event)
                || crate::welcome::key(app, event)
                || crate::peek::key(app, event)
                || crate::sitecard::key(app, event))
        {
            return crate::gtkx::inh(true);
        }
    }

    // Decide before applying: stopping a load changes the state `taken`
    // reads, but the key must still be consumed.
    let consume = taken(app, intent);
    if consume {
        apply(app, intent);
    }
    crate::gtkx::inh(consume)
}

#[cfg(test)]
mod focus_tests {
    use super::*;

    #[test]
    fn escape_waits_for_the_field_find_bar_and_panels() {
        assert!(escape_reaches_cards(false, false, false));
        assert!(!escape_reaches_cards(true, false, false));
        assert!(!escape_reaches_cards(false, true, false));
        assert!(!escape_reaches_cards(false, false, true));
        assert!(!escape_reaches_cards(true, true, true));
    }

    #[test]
    fn tab_moves_the_panel_only_from_outside_it() {
        assert!(tab_belongs_to_panel(true, false));
        assert!(!tab_belongs_to_panel(true, true));
        assert!(!tab_belongs_to_panel(false, false));
    }

    #[test]
    fn ctrl_arrows_switch_panels_only_outside_entries() {
        assert!(arrow_belongs_to_panel(true, false));
        assert!(!arrow_belongs_to_panel(true, true));
        assert!(!arrow_belongs_to_panel(false, false));
    }
}

#[cfg(test)]
mod sheet_tests {
    use super::*;

    /// The panel lists SHEET. If a row stops resolving to a real intent, the
    /// browser is advertising a key that does nothing.
    #[test]
    fn every_advertised_key_means_something() {
        for row in SHEET {
            let key = Key::new(row.3, row.4, row.5, row.6);
            let intent = key.intent();
            assert_ne!(
                intent,
                Intent::Nothing,
                "the sheet advertises {} ({}) but {} with ctrl={} shift={} alt={} means nothing",
                row.1,
                row.0,
                row.3,
                row.4,
                row.5,
                row.6
            );
        }
    }

    /// One press, one row. The panel draws every row, so two rows for the
    /// same press shows the user the same key twice; and because a press
    /// resolves through one fold, two rows could also claim two meanings for
    /// one key. Neither belongs in a sheet that is meant to be the truth.
    #[test]
    fn no_press_is_claimed_twice() {
        for (index, first) in SHEET.iter().enumerate() {
            for second in &SHEET[index + 1..] {
                let same_press = first.3.to_lowercase() == second.3.to_lowercase()
                    && first.4 == second.4
                    && first.5 == second.5
                    && first.6 == second.6;
                if !same_press {
                    continue;
                }
                let a = Key::new(first.3, first.4, first.5, first.6).intent();
                let b = Key::new(second.3, second.4, second.5, second.6).intent();
                panic!(
                    "{} ({}) and {} ({}) are the same press: the panel would list it twice ({:?} / {:?})",
                    first.1, first.0, second.1, second.0, a, b
                );
            }
        }
    }

    /// The other direction: a key the browser handles but the panel never
    /// mentions is a key nobody learns. The match below is deliberately
    /// exhaustive with no wildcard, so adding a variant to `Intent` stops this
    /// file compiling until someone says whether the panel should list it.
    /// That pause is the whole point: the gap cannot be forgotten silently.
    fn must_be_advertised(intent: Intent) -> bool {
        match intent {
            Intent::Nothing => false,
            Intent::Jump(_) => true,
            Intent::Omni
            | Intent::NewTab
            | Intent::NewPrivateTab
            | Intent::ReopenClosed
            | Intent::CloseTab
            | Intent::Reload
            | Intent::ReloadBypassCache
            | Intent::Reader
            | Intent::Bookmark
            | Intent::Find
            | Intent::FindNext
            | Intent::FindPrevious
            | Intent::History
            | Intent::Downloads
            | Intent::Bookmarks
            | Intent::Settings
            | Intent::Shortcuts
            | Intent::HideSomething
            | Intent::ShowHidden
            | Intent::Inspector
            | Intent::NextTab
            | Intent::PreviousTab
            | Intent::Back
            | Intent::Forward
            | Intent::ZoomIn
            | Intent::ZoomOut
            | Intent::ZoomReset
            | Intent::CopyAddress
            | Intent::Escape
            | Intent::ViewSource
            | Intent::Print
            | Intent::FullScreen
            | Intent::PanelLeft
            | Intent::PanelRight
            | Intent::PanelFocus
            | Intent::SwitchTab
            | Intent::DuplicateTab
            | Intent::SiteCard
            | Intent::FoldColumn => true,
        }
    }

    /// Same meaning even when the number differs: the jump row names one tab
    /// number, the intent can carry any.
    fn same_kind(a: Intent, b: Intent) -> bool {
        match (a, b) {
            (Intent::Jump(_), Intent::Jump(_)) => true,
            _ => a == b,
        }
    }

    #[test]
    fn every_intent_the_browser_can_do_is_listed() {
        let all = [
            Intent::Nothing,
            Intent::Omni,
            Intent::NewTab,
            Intent::NewPrivateTab,
            Intent::ReopenClosed,
            Intent::CloseTab,
            Intent::Reload,
            Intent::ReloadBypassCache,
            Intent::Reader,
            Intent::Bookmark,
            Intent::Find,
            Intent::FindNext,
            Intent::FindPrevious,
            Intent::History,
            Intent::Downloads,
            Intent::Bookmarks,
            Intent::Settings,
            Intent::Shortcuts,
            Intent::HideSomething,
            Intent::ShowHidden,
            Intent::Inspector,
            Intent::NextTab,
            Intent::PreviousTab,
            Intent::Back,
            Intent::Forward,
            Intent::ZoomIn,
            Intent::ZoomOut,
            Intent::ZoomReset,
            Intent::Jump(3),
            Intent::CopyAddress,
            Intent::Escape,
            Intent::ViewSource,
            Intent::Print,
            Intent::FullScreen,
            Intent::PanelLeft,
            Intent::PanelRight,
            Intent::PanelFocus,
            Intent::SiteCard,
            Intent::FoldColumn,
        ];
        for wanted in all {
            if !must_be_advertised(wanted) {
                continue;
            }
            let listed = SHEET
                .iter()
                .map(|row| Key::new(row.3, row.4, row.5, row.6).intent())
                .any(|intent| same_kind(intent, wanted));
            assert!(
                listed,
                "{:?} can be carried out but no row of the sheet offers it",
                wanted
            );
        }
    }

    /// Some rows name a second spelling in their label because the keyval is
    /// layout dependent. That promise is checked here: if `equal` stops
    /// zooming, "Ctrl + or Ctrl =" becomes a lie.
    #[test]
    fn the_other_spellings_a_label_promises_still_work() {
        let label = SHEET.iter().find(|row| row.0 == "zoomin").map(|row| row.1).unwrap_or_default();
        assert!(label.contains("Ctrl ="), "the zoom row no longer mentions equal");
        assert_eq!(Key::new("equal", true, false, false).intent(), Intent::ZoomIn);
        assert_eq!(Key::new("plus", true, false, false).intent(), Intent::ZoomIn);
        // Shift arrives as the upper case plus on many layouts.
        assert_eq!(Key::new("plus", true, true, false).intent(), Intent::ZoomIn);
    }

    /// And the bench verb for that row drives the same intent.
    #[test]
    fn every_bench_verb_reaches_its_intent() {
        for row in SHEET {
            let key = Key::new(row.3, row.4, row.5, row.6);
            assert_eq!(
                intent_of(row.0),
                Some(key.intent()),
                "the bench verb {} and the sheet row {} disagree",
                row.0,
                row.1
            );
        }
    }
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
    fn altgr_characters_are_not_browser_shortcuts() {
        // On many layouts AltGr arrives as Ctrl+Alt. AltGr+L, AltGr+T, AltGr+Q
        // and AltGr+7 type characters such as @, | or {, so they must reach the
        // page or the field instead of opening the field, a tab or a jump.
        assert_eq!(key("l", true, false, true).intent(), Intent::Nothing);
        assert_eq!(key("t", true, false, true).intent(), Intent::Nothing);
        assert_eq!(key("q", true, false, true).intent(), Intent::Nothing);
        assert_eq!(key("7", true, false, true).intent(), Intent::Nothing);
        assert_eq!(key("w", true, true, true).intent(), Intent::Nothing);
        // Ctrl alone still reaches the same shortcuts.
        assert_eq!(key("l", true, false, false).intent(), Intent::Omni);
        assert_eq!(key("t", true, false, false).intent(), Intent::NewTab);
        assert_eq!(key("7", true, false, false).intent(), Intent::Jump(7));
        // Alt alone keeps Back and Forward.
        assert_eq!(key("Left", false, false, true).intent(), Intent::Back);
        assert_eq!(key("Right", false, false, true).intent(), Intent::Forward);
        // Ctrl+Alt+arrow switches workspaces on most desktops, and AltGr can
        // arrive as Ctrl+Alt, so neither may move the page through history.
        assert_eq!(key("Left", true, false, true).intent(), Intent::Nothing);
        assert_eq!(key("Right", true, false, true).intent(), Intent::Nothing);
        assert_eq!(key("Left", true, true, true).intent(), Intent::Nothing);
        assert_eq!(key("Right", true, true, true).intent(), Intent::Nothing);
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
    fn f3_steps_through_the_find_bar_both_ways() {
        assert_eq!(key("F3", false, false, false).intent(), Intent::FindNext);
        assert_eq!(key("F3", false, true, false).intent(), Intent::FindPrevious);
        // The keyval can arrive folded already; the fold is not case sensitive.
        assert_eq!(key("f3", false, false, false).intent(), Intent::FindNext);
        // Ctrl F3 is not a browser key here, so it must not step the bar.
        assert_eq!(key("F3", true, false, false).intent(), Intent::Nothing);
    }

    #[test]
    fn alt_arrows_go_through_history() {
        assert_eq!(key("Left", false, false, true).intent(), Intent::Back);
        assert_eq!(key("Right", false, false, true).intent(), Intent::Forward);
        assert_eq!(key("Left", false, false, false).intent(), Intent::Nothing);
    }

    #[test]
    fn ctrl_h_is_history_and_ctrl_shift_p_is_not_print() {
        assert_eq!(key("h", true, false, false).intent(), Intent::History);
        assert_eq!(key("H", true, false, false).intent(), Intent::History);
        assert_eq!(key("p", true, false, false).intent(), Intent::Print);
        assert_eq!(key("P", true, true, false).intent(), Intent::Nothing);
    }

    #[test]
    fn numpad_zoom_keys_map_to_zoom_intents() {
        assert_eq!(key("KP_Add", true, false, false).intent(), Intent::ZoomIn);
        assert_eq!(key("KP_Subtract", true, false, false).intent(), Intent::ZoomOut);
        assert_eq!(key("KP_0", true, false, false).intent(), Intent::ZoomReset);
        assert_eq!(key("KP_Add", false, false, false).intent(), Intent::Nothing);
    }
}

