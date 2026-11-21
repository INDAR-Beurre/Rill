// What a page may ask of the browser through script and files: alerts,
// confirmations, prompts, the leave-page question, and the file chooser. A
// dialog is answered with the browser's own card over the tab that asked,
// built with the same helper as the permission cards. Nothing is a stock dialog.
use crate::app::{App, TabId};
use crate::permissions;
use gtk::gdk::keys::constants::{Escape, KP_Enter, Return};
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::BTreeMap;
use webkit2gtk::{
    FileChooserRequest, FileChooserRequestExt, ScriptDialog, ScriptDialogType, WebView, WebViewExt,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Alert,
    Confirm,
    Prompt,
    LeavePage,
}

pub fn kind_of(kind: ScriptDialogType) -> Option<Kind> {
    match kind {
        ScriptDialogType::Alert => Some(Kind::Alert),
        ScriptDialogType::Confirm => Some(Kind::Confirm),
        ScriptDialogType::Prompt => Some(Kind::Prompt),
        ScriptDialogType::BeforeUnloadConfirm => Some(Kind::LeavePage),
        _ => None,
    }
}

/// The buttons a dialog shows, in order, each with the answer it gives.
pub fn choices(kind: Kind) -> &'static [(&'static str, bool)] {
    match kind {
        Kind::Alert => &[("OK", true)],
        Kind::Confirm | Kind::Prompt | Kind::LeavePage => &[("Cancel", false), ("OK", true)],
    }
}

pub fn has_field(kind: Kind) -> bool {
    kind == Kind::Prompt
}

/// Pages that keep asking are cut off: a third dialog inside this window,
/// counting the one being shown, offers "Stop this page asking".
pub const REPEAT_LIMIT: usize = 3;
pub const REPEAT_WINDOW_MS: i64 = 10_000;

pub fn shows_stop(earlier: &[i64], now: i64) -> bool {
    let recent = earlier.iter().filter(|t| now - **t <= REPEAT_WINDOW_MS).count();
    recent + 1 >= REPEAT_LIMIT
}

#[derive(Default)]
struct History {
    shown: Vec<i64>,
    stopped: bool,
}

struct Prompt {
    widget: gtk::Box,
    // WebKit holds the page's script until close() is called on this dialog.
    // ScriptDialog is a shared GObject wrapper, so this clone is a reference:
    // the dialog stays alive here until answer() closes it.
    dialog: ScriptDialog,
    entry: Option<gtk::Entry>,
    default: Option<gtk::Button>,
}

thread_local! {
    // One dialog per tab. WebKit waits on the page's script, so a second one
    // while the first is open is answered Cancel instead of queued.
    static PENDING: RefCell<BTreeMap<TabId, Prompt>> = const { RefCell::new(BTreeMap::new()) };
    static HISTORY: RefCell<BTreeMap<TabId, History>> = const { RefCell::new(BTreeMap::new()) };
    static CHOOSERS: RefCell<Vec<gtk::FileChooserNative>> = const { RefCell::new(Vec::new()) };
}

fn pending(tab: TabId) -> bool {
    PENDING.with(|p| p.borrow().contains_key(&tab))
}

/// When dialogs from this page are stopped, a leave-page check must answer
/// true (allow leaving) so the user is not trapped on the page. Any other
/// suppressed or shadowed dialog answers false (Cancel).
pub fn suppressed_answer(kind: Kind, suppressed: bool) -> bool {
    suppressed && kind == Kind::LeavePage
}

fn finish(dialog: &ScriptDialog, confirmed: bool, text: Option<&str>) {
    if let Some(text) = text {
        dialog.prompt_set_text(text);
    }
    dialog.confirm_set_confirmed(confirmed);
    dialog.close();
}

pub fn on_dialog(app: &App, tab: TabId, view: &WebView, dialog: &ScriptDialog) -> bool {
    let Some(kind) = kind_of(dialog.dialog_type()) else {
        finish(dialog, false, None);
        return true;
    };
    let now = (glib::monotonic_time() / 1000) as i64;
    let (suppressed, stop_offered) = HISTORY.with(|h| {
        let mut h = h.borrow_mut();
        let slot = h.entry(tab).or_default();
        if slot.stopped {
            return (true, false);
        }
        let offered = shows_stop(&slot.shown, now);
        slot.shown.retain(|t| now - *t <= REPEAT_WINDOW_MS);
        slot.shown.push(now);
        (false, offered)
    });
    if suppressed || pending(tab) {
        finish(dialog, suppressed_answer(kind, suppressed), None);
        return true;
    }

    // The host is the browser's line, not the page's: a page cannot put its
    // own name above the card.
    let host = view.uri().map(|uri| crate::shield::host_of(&uri)).unwrap_or_default();
    let (head, body) = match kind {
        Kind::LeavePage => (host, "Leave this page?".to_string()),
        _ => (
            format!("{host} says"),
            dialog.message().map(|m| m.to_string()).unwrap_or_default(),
        ),
    };
    let card = permissions::card(app, &head);
    if let Some(access) = card.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Dialog);
    }
    let text = gtk::Label::new(Some(&body));
    crate::ui::class(&text, "settingdesc");
    text.set_xalign(0.0);
    text.set_line_wrap(true);
    text.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    text.set_max_width_chars(44);
    card.add(&text);

    let entry = has_field(kind).then(|| {
        let entry = gtk::Entry::new();
        entry.set_text(&dialog.prompt_get_default_text().map(|t| t.to_string()).unwrap_or_default());
        if let Some(access) = entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Dialog input");
        }
        card.add(&entry);
        entry
    });

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::End);
    if stop_offered {
        let stop = gtk::Button::with_label("Stop this page asking");
        crate::ui::class(&stop, "act");
        let weak = app.weak();
        stop.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                HISTORY.with(|h| h.borrow_mut().entry(tab).or_default().stopped = true);
                answer(&app, tab, false);
            }
        });
        crate::ui::pack(&row, &stop, false);
    }
    for &(label, confirmed) in choices(kind) {
        let button = gtk::Button::with_label(label);
        crate::ui::class(&button, "act");
        if confirmed {
            crate::ui::class(&button, "primary");
        }
        let weak = app.weak();
        button.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                answer(&app, tab, confirmed);
            }
        });
        crate::ui::pack(&row, &button, false);
    }
    card.add(&row);

    // The card's own show_all ran before the field and buttons were added, so
    // they would stay hidden without this.
    card.show_all();
    let default = row.children().last().and_then(|w| w.clone().downcast::<gtk::Button>().ok());
    PENDING.with(|p| {
        p.borrow_mut().insert(
            tab,
            Prompt {
                widget: card.clone().upcast(),
                dialog: dialog.clone(),
                entry: entry.clone(),
                default: default.clone(),
            },
        )
    });
    sync_visible(app);
    if app.active_id() == tab {
        crate::motion::pop_card_in(&card, crate::find::PAGE_TOP, -10, 155);
    }
    // A prompt starts in its field. Alert and confirm start on the default
    // button, the one the last choice names (OK).
    match entry {
        Some(entry) => entry.grab_focus(),
        None => {
            if let Some(default) = default {
                default.grab_focus();
            }
        }
    }
    true
}

fn answer(app: &App, tab: TabId, confirmed: bool) {
    let Some(prompt) = PENDING.with(|p| p.borrow_mut().remove(&tab)) else { return };
    let text = prompt.entry.as_ref().map(|e| e.text().to_string());
    finish(&prompt.dialog, confirmed, if confirmed { text.as_deref() } else { None });
    crate::motion::cancel(&prompt.widget);
    app.ui.overlay.remove(&prompt.widget);
    permissions::focus_page(app);
}

/// A tab that closes or leaves its page answers its open dialog Cancel, and
/// forgets how often the page has been asking.
pub fn dismiss(app: &App, tab: TabId) {
    HISTORY.with(|h| h.borrow_mut().remove(&tab));
    if pending(tab) {
        answer(app, tab, false);
    }
}

/// Only the dialog of the tab on screen is drawn.
pub fn sync_visible(app: &App) {
    let active = app.active_id();
    PENDING.with(|p| {
        for (id, prompt) in p.borrow().iter() {
            prompt.widget.set_visible(*id == active);
        }
    });
}

/// If a dialog card is visible for `tab`, focuses its prompt entry or default
/// button and returns true; otherwise returns false.
pub fn focus_pending(_app: &App, tab: TabId) -> bool {
    let target = PENDING.with(|p| {
        let map = p.borrow();
        let prompt = map.get(&tab)?;
        if !prompt.widget.is_visible() {
            return None;
        }
        if let Some(entry) = &prompt.entry {
            return Some(entry.clone().upcast::<gtk::Widget>());
        }
        prompt.default.as_ref().map(|b| b.clone().upcast::<gtk::Widget>())
    });
    let Some(widget) = target else {
        return false;
    };
    widget.grab_focus();
    true
}

/// Escape cancels and Enter confirms the dialog on screen. The window sees
/// the key before the page does, so the page never gets it.
pub fn key(app: &App, event: &gtk::gdk::EventKey) -> bool {
    let tab = app.active_id();
    let showing = PENDING.with(|p| p.borrow().get(&tab).is_some_and(|p| p.widget.is_visible()));
    if !showing {
        return false;
    }
    let key = event.keyval();
    if key == Escape {
        answer(app, tab, false);
    } else if key == Return || key == KP_Enter {
        // Enter presses the button that has focus. It used to answer OK every
        // time, so a person who moved to Cancel with Tab and pressed Enter
        // confirmed the dialog anyway. Only a button inside this tab's card
        // counts. With focus anywhere else (the prompt's text field, or
        // nothing), Enter still means OK.
        let focused = app
            .ui
            .window
            .focused_widget()
            .and_then(|widget| widget.downcast::<gtk::Button>().ok())
            .filter(|button| {
                PENDING.with(|p| p.borrow().get(&tab).is_some_and(|p| button.is_ancestor(&p.widget)))
            });
        match focused {
            Some(button) => button.clicked(),
            None => answer(app, tab, true),
        }
    } else {
        return false;
    }
    true
}

/// The file chooser is the desktop's own, shown over the browser window. It
/// is kept until the person answers; nothing else holds it.
pub fn on_file_chooser(app: &App, request: &FileChooserRequest) -> bool {
    let chooser = gtk::FileChooserNative::new(
        Some("Choose a file"),
        Some(&app.ui.window),
        gtk::FileChooserAction::Open,
        Some("Open"),
        Some("Cancel"),
    );
    chooser.set_select_multiple(request.selects_multiple());
    let mimes = request.mime_types();
    if !mimes.is_empty() {
        let filter = gtk::FileFilter::new();
        for mime in &mimes {
            filter.add_mime_type(mime);
        }
        chooser.add_filter(filter);
    }
    if let Some(filter) = request.mime_types_filter() {
        chooser.add_filter(filter);
    }
    let answered = request.clone();
    chooser.connect_response(move |chooser, response| {
        let files: Vec<String> = chooser
            .filenames()
            .iter()
            .filter_map(|path| path.to_str().map(String::from))
            .collect();
        let refs: Vec<&str> = files.iter().map(String::as_str).collect();
        if response == gtk::ResponseType::Accept && !refs.is_empty() {
            answered.select_files(&refs);
        } else {
            answered.cancel();
        }
        CHOOSERS.with(|c| c.borrow_mut().retain(|kept| kept != chooser));
    });
    CHOOSERS.with(|c| c.borrow_mut().push(chooser.clone()));
    chooser.show();
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_dialog_type_has_its_own_buttons() {
        assert_eq!(kind_of(ScriptDialogType::Alert), Some(Kind::Alert));
        assert_eq!(kind_of(ScriptDialogType::Confirm), Some(Kind::Confirm));
        assert_eq!(kind_of(ScriptDialogType::Prompt), Some(Kind::Prompt));
        assert_eq!(kind_of(ScriptDialogType::BeforeUnloadConfirm), Some(Kind::LeavePage));
        assert_eq!(choices(Kind::Alert), &[("OK", true)]);
        assert_eq!(choices(Kind::Confirm), &[("Cancel", false), ("OK", true)]);
        assert_eq!(choices(Kind::Prompt), &[("Cancel", false), ("OK", true)]);
        assert_eq!(choices(Kind::LeavePage), &[("Cancel", false), ("OK", true)]);
    }

    #[test]
    fn only_a_prompt_has_a_text_field() {
        assert!(has_field(Kind::Prompt));
        assert!(!has_field(Kind::Alert));
        assert!(!has_field(Kind::Confirm));
        assert!(!has_field(Kind::LeavePage));
    }

    #[test]
    fn a_third_dialog_in_quick_succession_offers_to_stop() {
        assert!(!shows_stop(&[], 1_000));
        assert!(!shows_stop(&[1_000], 2_000));
        assert!(shows_stop(&[1_000, 2_000], 3_000));
        // An old dialog does not count toward the run.
        assert!(!shows_stop(&[0, 2_000], 2_000 + REPEAT_WINDOW_MS + 1_000));
        assert!(!shows_stop(&[0, 20_000], 20_001));
    }

    #[test]
    fn stopped_pages_still_allow_leaving_on_beforeunload() {
        assert!(suppressed_answer(Kind::LeavePage, true));
        assert!(!suppressed_answer(Kind::Alert, true));
        assert!(!suppressed_answer(Kind::Confirm, true));
        assert!(!suppressed_answer(Kind::Prompt, true));
        assert!(!suppressed_answer(Kind::LeavePage, false));
    }
}
