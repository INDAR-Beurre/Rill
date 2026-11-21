// What a page may ask for, and what HTTP sign-in asks for. Both are answered
// in the browser's own style over the tab that asked, never in a stock dialog.
//
// A decision is remembered per host and kind, in permissions.json. A private
// tab remembers nothing and asks every time.
use crate::app::{App, TabId};
use crate::find::{PAGE_INSET, PAGE_TOP};
use crate::store::Records;
use glib::prelude::*;
use gtk::gdk::keys::constants::Escape;
use gtk::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::CString;
use webkit2gtk::{
    AuthenticationRequest, AuthenticationRequestExt, PermissionRequest, PermissionRequestExt,
    UserMediaPermissionRequest, UserMediaPermissionRequestExt, WebView, WebViewExt,
};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Notifications,
    Location,
    Camera,
    Microphone,
    CameraAndMicrophone,
    Clipboard,
}

impl Kind {
    pub(crate) fn title(self) -> &'static str {
        match self {
            Kind::Notifications => "Notifications",
            Kind::Location => "Location",
            Kind::Camera => "Camera",
            Kind::Microphone => "Microphone",
            Kind::CameraAndMicrophone => "Camera and microphone",
            Kind::Clipboard => "Clipboard",
        }
    }

    /// What follows "{host} " in the permission prompt.
    fn asking(self) -> &'static str {
        match self {
            Kind::Notifications => "wants to use notifications",
            Kind::Location => "wants to use your location",
            Kind::Camera => "wants to use your camera",
            Kind::Microphone => "wants to use your microphone",
            Kind::CameraAndMicrophone => "wants to use your camera and microphone",
            Kind::Clipboard => "wants to read your clipboard",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Grant {
    pub host: String,
    pub kind: Kind,
    pub allowed: bool,
}

/// What a request asks for, by the engine's own type name. `None` means the
/// request is refused without a prompt and without a record.
pub fn kind_for(type_name: &str, audio: bool, video: bool) -> Option<Kind> {
    match type_name {
        "WebKitNotificationPermissionRequest" => Some(Kind::Notifications),
        "WebKitGeolocationPermissionRequest" => Some(Kind::Location),
        "WebKitUserMediaPermissionRequest" => match (audio, video) {
            (true, true) => Some(Kind::CameraAndMicrophone),
            (true, false) => Some(Kind::Microphone),
            (false, true) => Some(Kind::Camera),
            (false, false) => None,
        },
        "WebKitClipboardPermissionRequest" => Some(Kind::Clipboard),
        // Refused silently, on purpose. Device info lists the machine's
        // hardware, which a page has no business reading. Media key systems
        // are DRM, which this browser does not offer. Pointer lock,
        // website data access, missing media plugins and XR are not things a
        // person is asked about in any other browser, and a refusal leaves
        // the page and the reading flow intact.
        _ => None,
    }
}

/// The decision already recorded for this host and kind, if there is one.
pub fn decide(grants: &[Grant], host: &str, kind: Kind) -> Option<bool> {
    grants.iter().find(|g| g.host == host && g.kind == kind).map(|g| g.allowed)
}

/// Writes one decision in place. Returns whether it should be saved: a private
/// tab returns false and leaves the list alone, which is the whole rule.
pub fn record(grants: &mut Vec<Grant>, private: bool, host: &str, kind: Kind, allowed: bool) -> bool {
    if private {
        return false;
    }
    grants.retain(|g| !(g.host == host && g.kind == kind));
    grants.push(Grant { host: host.to_string(), kind, allowed });
    true
}

pub fn forget(grants: &mut Vec<Grant>, host: &str, kind: Kind) {
    grants.retain(|g| !(g.host == host && g.kind == kind));
}

enum Answer {
    Permission { request: PermissionRequest, kind: Kind, host: String, private: bool },
    Auth { request: AuthenticationRequest },
    /// An add-on from addons.mozilla.org waiting for Install or Cancel. The
    /// engine is not waiting on anything, so there is nothing to refuse: an
    /// unanswered card just leaves the .xpi in Downloads.
    AddOn { path: std::path::PathBuf },
}

struct Prompt {
    widget: gtk::Box,
    focus: Option<gtk::Widget>,
    answer: Answer,
}

thread_local! {
    // One prompt per tab. A second request while one is open is refused, so
    // the first is never left unanswered and the engine never waits forever.
    static PENDING: RefCell<BTreeMap<TabId, Prompt>> = const { RefCell::new(BTreeMap::new()) };
}

fn take(tab: TabId) -> Option<Prompt> {
    PENDING.with(|p| p.borrow_mut().remove(&tab))
}

fn pending(tab: TabId) -> bool {
    PENDING.with(|p| p.borrow().contains_key(&tab))
}

/// Answers a permission request from the page's view.
pub fn on_request(app: &App, tab: TabId, view: &WebView, request: &PermissionRequest) -> bool {
    let (audio, video) = match request.downcast_ref::<UserMediaPermissionRequest>() {
        Some(media) => (media.is_for_audio_device(), media.is_for_video_device()),
        None => (false, false),
    };
    let Some(kind) = kind_for(request.type_().name(), audio, video) else {
        request.deny();
        return true;
    };
    let host = view.uri().map(|uri| crate::shield::host_of(&uri)).unwrap_or_default();
    let private = app.state.borrow().tab(tab).map(|t| t.private).unwrap_or(true);
    if host.is_empty() || pending(tab) {
        request.deny();
        return true;
    }
    if !private {
        if let Some(allowed) = decide(&Records::load_permissions(), &host, kind) {
            if allowed {
                request.allow();
            } else {
                request.deny();
            }
            return true;
        }
    }
    // The page asking from a tab that is not on screen has nobody to ask.
    if app.active_id() != tab {
        request.deny();
        return true;
    }
    let weak = app.weak();
    let message = format!("{host} {}.", kind.asking());
    let card = card(app, &message);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    for (label, allowed) in [("Block", false), ("Allow", true)] {
        let button = gtk::Button::with_label(label);
        crate::ui::class(&button, "act");
        if allowed {
            crate::ui::class(&button, "primary");
        }
        let weak = weak.clone();
        button.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                answer_permission(&app, tab, allowed);
            }
        });
        crate::ui::pack(&buttons, &button, false);
    }
    card.add(&buttons);
    let focus = first_button(&card).map(|b| b.upcast::<gtk::Widget>());
    show(
        app,
        tab,
        card.clone(),
        focus.clone(),
        Answer::Permission { request: request.clone(), kind, host, private },
    );
    if let Some(block) = focus {
        block.grab_focus();
    }
    true
}

/// The first button of a card's button row, the one focus starts on.
fn first_button(card: &gtk::Box) -> Option<gtk::Button> {
    card.children().into_iter().filter_map(|child| child.downcast::<gtk::Box>().ok()).find_map(|row| {
        row.children().into_iter().find_map(|child| child.downcast::<gtk::Button>().ok())
    })
}

/// Puts the keyboard back on the page once a card that held it is gone.
pub(crate) fn focus_page(app: &App) {
    if let Some(view) = app.view(app.active_id()) {
        view.grab_focus();
    }
}

/// Escape refuses the card on screen: a permission is denied, a sign-in is
/// cancelled, and nothing is recorded.
pub fn key(app: &App, event: &gtk::gdk::EventKey) -> bool {
    if event.keyval() != Escape {
        return false;
    }
    let tab = app.active_id();
    let showing = PENDING.with(|p| p.borrow().get(&tab).is_some_and(|p| p.widget.is_visible()));
    if !showing {
        return false;
    }
    dismiss(app, tab);
    true
}

fn answer_permission(app: &App, tab: TabId, allowed: bool) {
    let Some(prompt) = take(tab) else { return };
    if let Answer::Permission { request, kind, host, private } = &prompt.answer {
        let mut grants = Records::load_permissions();
        if record(&mut grants, *private, host, *kind, allowed) {
            Records::save_permissions(&grants);
        }
        if allowed {
            request.allow();
        } else {
            request.deny();
        }
    }
    crate::motion::cancel(&prompt.widget);
    app.ui.overlay.remove(&prompt.widget);
    focus_page(app);
}

/// Answers an HTTP sign-in request with the browser's own small form.
pub fn on_auth(app: &App, tab: TabId, request: &AuthenticationRequest) -> bool {
    // The engine keeps nothing: no save offer, and the credential made in
    // `authenticate` below is persisted as NONE.
    request.set_can_save_credentials(false);
    if pending(tab) {
        request.cancel();
        return true;
    }
    let host = request.host().map(|h| h.to_string()).unwrap_or_default();
    let realm = request.realm().map(|r| r.to_string()).filter(|r| !r.is_empty());
    let who = if request.is_for_proxy() { "The proxy" } else { "The site" };
    let message = match realm {
        Some(realm) => format!("{who} {host} asks for a user name and password for \"{realm}\"."),
        None => format!("{who} {host} asks for a user name and password."),
    };
    let card = card(app, &message);

    let user = gtk::Entry::new();
    user.set_placeholder_text(Some("User name"));
    named(&user, "User name");
    let password = gtk::Entry::new();
    password.set_placeholder_text(Some("Password"));
    password.set_visibility(false);
    password.set_input_purpose(gtk::InputPurpose::Password);
    named(&password, "Password");
    card.add(&user);
    card.add(&password);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    crate::ui::class(&cancel, "act");
    let weak = app.weak();
    cancel.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            answer_auth(&app, tab, None);
        }
    });
    let sign_in = gtk::Button::with_label("Sign in");
    crate::ui::class(&sign_in, "act");
    crate::ui::class(&sign_in, "primary");
    let weak = app.weak();
    let (user_for_submit, password_for_submit) = (user.clone(), password.clone());
    let submit = move || {
        if let Some(app) = weak.upgrade() {
            let credentials = (user_for_submit.text().to_string(), password_for_submit.text().to_string());
            answer_auth(&app, tab, Some(credentials));
        }
    };
    let submit_click = submit.clone();
    sign_in.connect_clicked(move |_| submit_click());
    let submit_pw = submit.clone();
    password.connect_activate(move |_| submit_pw());
    {
        let password = password.clone();
        user.connect_activate(move |_| {
            if password.text().is_empty() {
                password.grab_focus();
            } else {
                submit();
            }
        });
    }
    crate::ui::pack(&buttons, &cancel, false);
    crate::ui::pack(&buttons, &sign_in, false);
    card.add(&buttons);

    show(
        app,
        tab,
        card.clone(),
        Some(user.clone().upcast::<gtk::Widget>()),
        Answer::Auth { request: request.clone() },
    );
    user.grab_focus();
    true
}

fn answer_auth(app: &App, tab: TabId, credentials: Option<(String, String)>) {
    let Some(prompt) = take(tab) else { return };
    if let Answer::Auth { request } = &prompt.answer {
        match credentials {
            Some((user, password)) => authenticate(request, &user, &password),
            None => request.cancel(),
        }
    }
    crate::motion::cancel(&prompt.widget);
    app.ui.overlay.remove(&prompt.widget);
    focus_page(app);
}

/// Hands WebKit a credential that it is told not to keep. The credential is
/// made here and freed here; WebKit copies what it needs during the call.
fn authenticate(request: &AuthenticationRequest, user: &str, password: &str) {
    use glib::translate::ToGlibPtr;
    let (Ok(user), Ok(password)) = (CString::new(user), CString::new(password)) else {
        request.cancel();
        return;
    };
    // SAFETY: both strings live across the calls, the request is a live
    // WebKit object held by the caller, and the credential is freed here.
    unsafe {
        let credential = webkit2gtk::ffi::webkit_credential_new(
            user.as_ptr(),
            password.as_ptr(),
            webkit2gtk::ffi::WEBKIT_CREDENTIAL_PERSISTENCE_NONE,
        );
        webkit2gtk::ffi::webkit_authentication_request_authenticate(request.to_glib_none().0, credential);
        webkit2gtk::ffi::webkit_credential_free(credential);
    }
}

/// Asks before an add-on fetched from addons.mozilla.org is installed. The
/// card names the add-on, where it came from, every site it would run on and
/// the add-on it would replace. Nothing is written to the extensions folder
/// until Install is pressed. Cancel, or leaving the page, keeps the .xpi in
/// Downloads and installs nothing.
pub fn offer_addon(
    app: &App,
    tab: TabId,
    path: std::path::PathBuf,
    came_from: String,
    offer: crate::extensions::Offer,
) {
    if pending(tab) {
        app.flash_toast(&format!(
            "{} was saved to Downloads but not installed: this tab is already asking something.",
            offer.name
        ));
        return;
    }
    let message = addon_message(&crate::shield::host_of(&came_from), &offer);
    let card = card(app, &message);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    let weak = app.weak();
    let mut install_button = None;
    for (label, install) in [("Cancel", false), ("Install", true)] {
        let button = gtk::Button::with_label(label);
        crate::ui::class(&button, "act");
        if install {
            crate::ui::class(&button, "primary");
            install_button = Some(button.clone());
        }
        let weak = weak.clone();
        button.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                answer_addon(&app, tab, install);
            }
        });
        crate::ui::pack(&buttons, &button, false);
    }
    card.add(&buttons);
    let focus = install_button.as_ref().map(|b| b.clone().upcast::<gtk::Widget>());
    show(app, tab, card.clone(), focus, Answer::AddOn { path });
    if let Some(install) = install_button {
        install.grab_focus();
    }
}

/// The card's words, kept apart from GTK so a test can read them.
pub fn addon_message(host: &str, offer: &crate::extensions::Offer) -> String {
    let host = if host.is_empty() { "an unknown address" } else { host };
    let mut lines = vec![format!("Install {} {} from {host}?", offer.name, offer.version)];
    let everywhere = offer
        .hosts
        .iter()
        .any(|h| matches!(h.as_str(), "<all_urls>" | "*://*/*" | "http://*/*" | "https://*/*"));
    if everywhere {
        lines.push("It can read and change every page you visit.".to_string());
    } else if offer.hosts.is_empty() {
        lines.push("It asks to run on no sites.".to_string());
    } else {
        let shown: Vec<&str> = offer.hosts.iter().take(5).map(String::as_str).collect();
        let more = offer.hosts.len().saturating_sub(shown.len());
        let tail = if more > 0 { format!(", and {more} more") } else { String::new() };
        lines.push(format!("It can read and change pages on {}{tail}.", shown.join(", ")));
    }
    if let Some(old) = &offer.replaces {
        lines.push(format!(
            "It replaces {} {}, already installed. It says it is the same add-on; nothing here checks a signature.",
            old.name, old.version
        ));
    }
    if let Some(gecko) = &offer.gecko_id {
        lines.push(format!("It calls itself {gecko}."));
    }
    if !offer.runs {
        lines.push("Nothing in it would run in this browser.".to_string());
    }
    if !offer.why_not.is_empty() {
        lines.push(format!("In this browser, {}", offer.why_not));
    }
    lines.join("\n\n")
}

fn answer_addon(app: &App, tab: TabId, install: bool) {
    let Some(prompt) = take(tab) else { return };
    crate::motion::cancel(&prompt.widget);
    app.ui.overlay.remove(&prompt.widget);
    focus_page(app);
    let Answer::AddOn { path } = &prompt.answer else { return };
    if !install {
        app.flash_toast("Not installed. The file is still in Downloads.");
        return;
    }
    match crate::extensions::install(path) {
        Ok(entry) => {
            app.rebuild_every_view();
            // The caveat is always said, not only when nothing runs: an add-on
            // whose content scripts run but whose background page never starts
            // is only partly working, and the toast has to say so.
            if entry.why_not.is_empty() {
                app.flash_toast(&format!("{} installed", entry.name));
            } else {
                app.flash_toast(&format!("{} installed: {}", entry.name, entry.why_not.replace('\n', " ")));
            }
        }
        Err(why) => app.flash_toast(&format!("Could not install: {why}")),
    }
}

/// Refuses whatever the tab is waiting on: a tab that closes or leaves its
/// page takes its open prompt with it.
pub fn dismiss(app: &App, tab: TabId) {
    let Some(prompt) = take(tab) else { return };
    match &prompt.answer {
        Answer::Permission { request, .. } => request.deny(),
        Answer::Auth { request } => request.cancel(),
        Answer::AddOn { .. } => app.flash_toast("The add-on was not installed. The file is still in Downloads."),
    }
    crate::motion::cancel(&prompt.widget);
    app.ui.overlay.remove(&prompt.widget);
    focus_page(app);
}

/// Only the prompt of the tab on screen is drawn.
pub fn sync_visible(app: &App) {
    let active = app.active_id();
    PENDING.with(|p| {
        for (id, prompt) in p.borrow().iter() {
            prompt.widget.set_visible(*id == active);
        }
    });
}

/// If a permission, sign-in, or add-on card is visible for `tab`, focuses its
/// primary action or entry and returns true; otherwise returns false.
pub fn focus_pending(_app: &App, tab: TabId) -> bool {
    let target = PENDING.with(|p| {
        let map = p.borrow();
        let prompt = map.get(&tab)?;
        if !prompt.widget.is_visible() {
            return None;
        }
        prompt.focus.clone()
    });
    let Some(widget) = target else {
        return false;
    };
    widget.grab_focus();
    true
}

fn show(app: &App, tab: TabId, card: gtk::Box, focus: Option<gtk::Widget>, answer: Answer) {
    // The buttons were added after the card's own show_all, so show again or
    // they stay hidden.
    card.show_all();
    PENDING.with(|p| {
        p.borrow_mut().insert(
            tab,
            Prompt { widget: card.clone().upcast(), focus, answer },
        )
    });
    sync_visible(app);
    if app.active_id() == tab {
        crate::motion::pop_card_in(&card, PAGE_TOP, -10, 155);
    }
}

/// The card the browser draws over the page's top right, the same place as
/// find. Its colours are the ones the find bar and downloads already use.
pub(crate) fn card(app: &App, message: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    crate::ui::class(&card, "dlbar");
    if let Some(access) = card.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Dialog);
    }
    card.set_halign(gtk::Align::End);
    card.set_valign(gtk::Align::Start);
    card.set_margin_start(12);
    card.set_margin_end(PAGE_INSET);
    // The overlay begins at the window's top edge, band included, so a card
    // placed PAGE_TOP below it sat 9px inside the 23px band and lost the top
    // of its title (round 43's site card). The page starts under the band.
    card.set_margin_top(PAGE_TOP);
    card.set_margin_bottom(8);
    card.set_size_request(300, -1);
    // Room inside the border. `.dlbar` has none, because the downloads bar
    // and the Peek header bring their own. Without this, round 43's site card
    // had its title and rows against the left border and its last button
    // against the bottom one. It is CSS padding on a `prompt` class, not
    // set_border_width: a GtkBox lays out through its CSS gadget, which
    // ignores the container border width (the bench `cards` verb read
    // border 12 with the first label still at [1,1]). Only the cards built
    // here (the site card, the permission prompt, script dialogs) get it.
    crate::ui::class(&card, "prompt");
    let text = gtk::Label::new(Some(message));
    crate::ui::class(&text, "setting");
    text.set_xalign(0.0);
    text.set_line_wrap(true);
    text.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    text.set_max_width_chars(44);
    card.add(&text);
    app.ui.overlay.add_overlay(&card);
    card.show_all();
    card
}

fn named<W: IsA<gtk::Widget>>(widget: &W, name: &str) {
    if let Some(access) = widget.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, name);
    }
}

/// The saved decisions, each with a way to clear it. Only what was saved is
/// listed; a private tab leaves nothing here.
pub fn settings_section(app: &crate::app::AppRef) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 6);
    section.set_margin_top(14);
    let head = gtk::Label::new(Some("SITE PERMISSIONS"));
    crate::ui::class(&head, "group");
    head.set_xalign(0.0);
    section.add(&head);

    let grants = Records::load_permissions();
    if grants.is_empty() {
        let note = gtk::Label::new(Some("Nothing is remembered. A site asks each time it wants something."));
        crate::ui::class(&note, "settingdesc");
        note.set_xalign(0.0);
        note.set_line_wrap(true);
        section.add(&note);
    }
    for grant in grants {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(4);
        row.set_margin_bottom(4);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        let host = gtk::Label::new(Some(&grant.host));
        crate::ui::class(&host, "setting");
        host.set_xalign(0.0);
        host.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let what = gtk::Label::new(Some(&format!(
            "{}: {}",
            grant.kind.title(),
            if grant.allowed { "Allowed" } else { "Blocked" }
        )));
        crate::ui::class(&what, "settingdesc");
        what.set_xalign(0.0);
        crate::ui::pack(&texts, &host, false);
        crate::ui::pack(&texts, &what, false);
        crate::ui::pack(&row, &texts, true);

        let clear = gtk::Button::with_label("Clear");
        crate::ui::class(&clear, "act");
        clear.set_valign(gtk::Align::Center);
        clear.set_tooltip_text(Some("Clear this decision"));
        let weak = app.weak();
        let (host_name, kind) = (grant.host.clone(), grant.kind);
        clear.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let mut grants = Records::load_permissions();
                forget(&mut grants, &host_name, kind);
                Records::save_permissions(&grants);
                app.open_panel(crate::panels::PanelKind::Settings);
            }
        });
        crate::ui::pack(&row, &clear, false);
        section.add(&row);
    }
    section
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_maps_to_the_kind_it_asks_for() {
        assert_eq!(kind_for("WebKitNotificationPermissionRequest", false, false), Some(Kind::Notifications));
        assert_eq!(kind_for("WebKitGeolocationPermissionRequest", false, false), Some(Kind::Location));
        assert_eq!(kind_for("WebKitUserMediaPermissionRequest", false, true), Some(Kind::Camera));
        assert_eq!(kind_for("WebKitUserMediaPermissionRequest", true, false), Some(Kind::Microphone));
        assert_eq!(kind_for("WebKitUserMediaPermissionRequest", true, true), Some(Kind::CameraAndMicrophone));
        assert_eq!(kind_for("WebKitClipboardPermissionRequest", false, false), Some(Kind::Clipboard));
    }

    #[test]
    fn clipboard_kind_has_title_copy_and_json_roundtrip() {
        assert_eq!(Kind::Clipboard.title(), "Clipboard");
        assert_eq!(Kind::Clipboard.asking(), "wants to read your clipboard");
        let encoded = serde_json::to_string(&Kind::Clipboard).expect("serialize");
        assert_eq!(encoded, "\"clipboard\"");
        let decoded: Kind = serde_json::from_str(&encoded).expect("deserialize");
        assert_eq!(decoded, Kind::Clipboard);
    }

    #[test]
    fn a_request_with_no_device_is_refused() {
        assert_eq!(kind_for("WebKitUserMediaPermissionRequest", false, false), None);
    }

    #[test]
    fn what_is_always_refused_never_reaches_a_prompt() {
        for name in [
            "WebKitDeviceInfoPermissionRequest",
            "WebKitMediaKeySystemPermissionRequest",
            "WebKitPointerLockPermissionRequest",
            "WebKitWebsiteDataAccessPermissionRequest",
            "WebKitInstallMissingMediaPluginsPermissionRequest",
            "WebKitXRPermissionRequest",
            "SomethingNew",
        ] {
            assert_eq!(kind_for(name, true, true), None, "{name}");
        }
    }

    #[test]
    fn a_saved_decision_is_found_for_its_host_and_kind_only() {
        let mut grants = Vec::new();
        assert!(record(&mut grants, false, "example.org", Kind::Camera, true));
        assert_eq!(decide(&grants, "example.org", Kind::Camera), Some(true));
        assert_eq!(decide(&grants, "example.org", Kind::Microphone), None);
        assert_eq!(decide(&grants, "other.org", Kind::Camera), None);
        // A new answer replaces the old one rather than sitting beside it.
        assert!(record(&mut grants, false, "example.org", Kind::Camera, false));
        assert_eq!(grants.len(), 1);
        assert_eq!(decide(&grants, "example.org", Kind::Camera), Some(false));
        forget(&mut grants, "example.org", Kind::Camera);
        assert!(grants.is_empty());
    }

    #[test]
    fn a_private_tab_writes_nothing() {
        let mut grants = Vec::new();
        assert!(!record(&mut grants, true, "example.org", Kind::Location, true));
        assert!(grants.is_empty());
    }

    #[test]
    fn decisions_survive_a_trip_through_a_file_in_a_temporary_folder() {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("search-permissions-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp folder");
        let path = dir.join("permissions.json");

        let mut grants = Vec::new();
        record(&mut grants, false, "maps.example", Kind::Location, true);
        record(&mut grants, false, "call.example", Kind::CameraAndMicrophone, false);
        crate::files::write_json(&path, &grants);

        let back: Vec<Grant> = crate::files::read_json(&path).unwrap_or_default();
        assert_eq!(back, grants);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
