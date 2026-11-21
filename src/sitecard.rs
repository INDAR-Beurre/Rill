// The site card: what the browser knows about the page on screen, and the few
// things a person may change about this site. Opened with Ctrl I, or by
// clicking the site name in the address field.
//
// The rules are plain functions (`build`, `security`, `change`, `clear_targets`)
// so they are tested without a window. The card only draws what they return and
// carries out the one action each button names.
use crate::app::{App, TabId};
use crate::extensions;
use crate::permissions::{self, Grant, Kind};
use crate::store::Records;
use gtk::gio::TlsCertificateFlags;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use webkit2gtk::{
    WebContextExt, WebViewExt, WebsiteData, WebsiteDataManagerExt, WebsiteDataManagerExtManual, WebsiteDataTypes,
};

thread_local! {
    /// The card on screen and the tab it describes. Only one is ever up.
    static OPEN: RefCell<Option<(TabId, gtk::Box)>> = const { RefCell::new(None) };
    /// The records each clear is removing, filed under that clear's number. They
    /// stay alive until WebKit reports back for that clear, so two clears in a
    /// row never drop each other's records.
    static CLEARING: RefCell<Vec<(u64, Vec<WebsiteData>)>> = const { RefCell::new(Vec::new()) };
    /// The number the next clear is filed under.
    static CLEAR_TICKET: Cell<u64> = const { Cell::new(0) };
}

/// The data kinds a site clear removes. Cookies and storage only: the cache is
/// shared by many sites and is not this site's to clear.
fn site_data() -> WebsiteDataTypes {
    WebsiteDataTypes::COOKIES
        | WebsiteDataTypes::LOCAL_STORAGE
        | WebsiteDataTypes::SESSION_STORAGE
        | WebsiteDataTypes::INDEXEDDB_DATABASES
        | WebsiteDataTypes::WEBSQL_DATABASES
        | WebsiteDataTypes::SERVICE_WORKER_REGISTRATIONS
}

#[derive(Debug, PartialEq)]
pub enum Security {
    /// Plain http: nothing is encrypted.
    Plain,
    /// https, and the certificate checks out.
    Secure,
    /// https, but the certificate has a problem. The text is the cause.
    Problem(&'static str),
    /// https, but the engine gave no certificate to check. Not the same as secure.
    Unknown,
    /// Not a web site (about:, file:, data:).
    NotWeb,
}

impl Security {
    pub fn text(&self) -> String {
        match self {
            Security::Plain => "Not private. This site is reached without encryption, so others on the network can read what passes between you.".into(),
            Security::Secure => "Secure. The connection is encrypted and the site's certificate checks out.".into(),
            Security::Problem(why) => format!("Not secure. This site's security could not be verified: {why}."),
            Security::Unknown => "The security of this connection could not be read.".into(),
            Security::NotWeb => "This is not a web site, so there is no connection to check.".into(),
        }
    }
}

/// Whether the address is one the browser loads over the network.
fn is_web(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// The connection's state. `tls` is None when there is no certificate to check.
pub fn security(url: &str, tls: Option<TlsCertificateFlags>) -> Security {
    if !is_web(url) {
        return Security::NotWeb;
    }
    if url.to_ascii_lowercase().starts_with("http://") {
        return Security::Plain;
    }
    match tls {
        None => Security::Unknown,
        Some(flags) if flags.is_empty() => Security::Secure,
        Some(flags) => Security::Problem(crate::web::tls_problem(flags)),
    }
}

/// The decisions saved for exactly this host. A subdomain or a look-alike
/// host is another site and is not listed.
pub fn saved_for(grants: &[Grant], host: &str) -> Vec<Grant> {
    grants.iter().filter(|g| !host.is_empty() && g.host == host).cloned().collect()
}

/// The zoom as the card shows it, in whole percent.
pub fn zoom_percent(level: f64) -> u32 {
    (level * 100.0).round() as u32
}

pub struct Model {
    pub host: String,
    pub security: Security,
    pub saved: Vec<Grant>,
    /// A private tab saves nothing, so its decisions are only shown.
    pub private: bool,
    pub zoom: u32,
    pub addons: Vec<String>,
}

pub fn build(
    url: &str,
    tls: Option<TlsCertificateFlags>,
    grants: &[Grant],
    private: bool,
    zoom: f64,
    addons: Vec<String>,
) -> Model {
    let host = if is_web(url) { crate::shield::host_of(url) } else { String::new() };
    Model {
        saved: saved_for(grants, &host),
        security: security(url, tls),
        host,
        private,
        zoom: zoom_percent(zoom),
        addons,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Set(bool),
    Forget,
}

/// The saved decisions after one change to this site, or None when nothing may
/// be written: a private tab writes nothing, and an empty host names no site.
pub fn change(grants: &[Grant], private: bool, host: &str, kind: Kind, how: Change) -> Option<Vec<Grant>> {
    if private || host.is_empty() {
        return None;
    }
    let mut next = grants.to_vec();
    match how {
        Change::Set(allowed) => {
            permissions::record(&mut next, false, host, kind, allowed);
        }
        Change::Forget => permissions::forget(&mut next, host, kind),
    }
    Some(next)
}

/// The indices of stored records that belong to this site. WebKit files most
/// data under the site's main domain ("example.com" for "www.example.com"), so
/// an exact match would find nothing on a www. site. A record counts when its
/// name, without a leading dot, is the host itself or a domain the host sits
/// under. A sibling ("other.example.com"), a look-alike ("example.com.evil.test",
/// "notexample.com") or a bare name with no dot is another site and is not
/// cleared. An empty host picks nothing.
pub fn clear_targets(host: &str, names: &[String]) -> Vec<usize> {
    let want = host.trim_end_matches('.').to_ascii_lowercase();
    if want.is_empty() {
        return Vec::new();
    }
    names
        .iter()
        .enumerate()
        .filter(|(_, name)| {
            let name = name.trim_start_matches('.').to_ascii_lowercase();
            !name.is_empty()
                && name.contains('.')
                && (want == name || want.ends_with(&format!(".{name}")))
        })
        .map(|(index, _)| index)
        .collect()
}

/// Opens the card for the tab on screen, replacing one already up.
pub fn open(app: &App) {
    let tab = app.active_id();
    let was_open = OPEN.with(|o| o.borrow().as_ref().is_some_and(|(id, _)| *id == tab));
    if let Some((_, old)) = OPEN.with(|o| o.borrow_mut().take()) {
        crate::motion::cancel(&old);
        app.ui.overlay.remove(&old);
    }
    for child in app.ui.overlay.children() {
        if child.widget_name() == "site-card" {
            crate::motion::cancel(&child);
            app.ui.overlay.remove(&child);
        }
    }
    let Some((url, private)) = app.state.borrow().tab(tab).map(|t| (t.url.clone(), t.private)) else {
        return;
    };
    let tls = app.view(tab).and_then(|view| view.tls_info()).map(|(_, flags)| flags);
    let model = build(
        &url,
        tls,
        &Records::load_permissions(),
        private,
        app.zoom_for(&url),
        extensions::here(&url),
    );
    let title = if model.host.is_empty() { "This page".to_string() } else { model.host.clone() };
    let card = permissions::card(app, &title);
    card.set_widget_name("site-card");
    card.set_size_request(380, -1);
    if let Some(access) = card.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Dialog);
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Site information");
    }
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    add_rows(&content, tab, &model);
    if model.saved.len() + model.addons.len() > 6 {
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(480);
        scroll.set_overlay_scrolling(false);
        scroll.add(&content);
        card.add(&scroll);
    } else {
        card.add(&content);
    }
    card.show_all();
    if !was_open {
        crate::motion::pop_card_in(&card, crate::find::PAGE_TOP, -10, 155);
    }
    if let Some(button) = first_interactive(&content) {
        button.grab_focus();
    } else {
        card.set_can_focus(true);
        card.grab_focus();
    }
    OPEN.with(|o| *o.borrow_mut() = Some((tab, card)));
}

fn first_interactive(box_: &gtk::Box) -> Option<gtk::Button> {
    box_.children().into_iter().find_map(|child| {
        if let Ok(button) = child.clone().downcast::<gtk::Button>() {
            return button.is_sensitive().then_some(button);
        }
        let row = child.downcast::<gtk::Box>().ok()?;
        row.children()
            .into_iter()
            .filter_map(|w| w.downcast::<gtk::Button>().ok())
            .find(|b| b.is_sensitive())
    })
}

/// Takes the card down. The keyboard goes back to the page.
pub fn dismiss(app: &App) {
    let Some((_, card)) = OPEN.with(|o| o.borrow_mut().take()) else { return };
    crate::motion::cancel(&card);
    permissions::focus_page(app);
    let overlay = app.ui.overlay.clone();
    let card_for_done = card.clone();
    crate::motion::fade_out_then(&card, 95, move || {
        overlay.remove(&card_for_done);
    });
}

/// A tab switch or a close takes the card only when it describes that tab.
pub fn dismiss_tab(app: &App, tab: TabId) {
    let mine = OPEN.with(|o| o.borrow().as_ref().is_some_and(|(id, _)| *id == tab));
    if mine {
        dismiss(app);
    }
}

/// Escape takes the card down, and only when it is the thing on screen.
pub fn key(app: &App, event: &gtk::gdk::EventKey) -> bool {
    if event.keyval() != gtk::gdk::keys::constants::Escape {
        return false;
    }
    let showing = OPEN.with(|o| o.borrow().as_ref().is_some_and(|(_, card)| card.is_visible()));
    if !showing {
        return false;
    }
    dismiss(app);
    true
}

fn text(card: &gtk::Box, message: &str, class: &str) {
    let label = gtk::Label::new(Some(message));
    crate::ui::class(&label, class);
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_max_width_chars(44);
    card.add(&label);
}

fn button(label: &str, tip: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    crate::ui::class(&button, "act");
    button.set_tooltip_text(Some(tip));
    button
}

fn row(card: &gtk::Box, message: &str, buttons: Vec<gtk::Button>) {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(message));
    crate::ui::class(&label, "settingdesc");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_line_wrap(true);
    label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_max_width_chars(30);
    line.pack_start(&label, true, true, 0);
    for button in buttons {
        button.set_valign(gtk::Align::Center);
        line.pack_start(&button, false, true, 0);
    }
    card.add(&line);
}

fn add_rows(card: &gtk::Box, tab: TabId, model: &Model) {
    text(card, &model.security.text(), "settingdesc");

    let zoom_out = button("-", "Zoom out");
    if let Some(access) = zoom_out.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Zoom out");
    }
    zoom_out.set_sensitive(model.zoom > 40);
    zoom_out.connect_clicked(move |_| {
        if let Some(app) = crate::app::current() {
            app.zoom(-0.1);
            open(&app);
        }
    });
    let zoom_in = button("+", "Zoom in");
    if let Some(access) = zoom_in.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Zoom in");
    }
    zoom_in.set_sensitive(model.zoom < 300);
    zoom_in.connect_clicked(move |_| {
        if let Some(app) = crate::app::current() {
            app.zoom(0.1);
            open(&app);
        }
    });
    let reset = button("Reset zoom", "Back to the normal size for this site");
    reset.set_sensitive(model.zoom != 100);
    reset.connect_clicked(move |_| {
        if let Some(app) = crate::app::current() {
            app.reset_zoom();
            open(&app);
        }
    });
    row(card, &format!("Zoom {}%", model.zoom), vec![zoom_out, zoom_in, reset]);

    text(card, "Permissions", "group");
    if model.private {
        text(card, "Private tab: nothing is saved here, so these are shown read-only.", "settingdesc");
    }
    if model.saved.is_empty() {
        text(card, "Nothing is saved for this site. It asks each time it wants something.", "settingdesc");
    }
    for grant in &model.saved {
        let what = format!(
            "{}: {}",
            grant.kind.title(),
            if grant.allowed { "Allowed" } else { "Blocked" }
        );
        if model.private {
            row(card, &what, Vec::new());
            continue;
        }
        let (kind, host, allowed) = (grant.kind, model.host.clone(), grant.allowed);
        let flip = button(
            if allowed { "Block" } else { "Allow" },
            if allowed { "Block this from now on" } else { "Allow this from now on" },
        );
        let (flip_host, flip_kind) = (host.clone(), kind);
        flip.connect_clicked(move |_| apply(&flip_host, flip_kind, Change::Set(!allowed)));
        let forget = button("Forget", "Ask again next time");
        forget.connect_clicked(move |_| apply(&host, kind, Change::Forget));
        row(card, &what, vec![flip, forget]);
    }

    text(card, "Add-ons on this page", "group");
    if model.addons.is_empty() {
        text(card, "No installed add-on acts on this page.", "settingdesc");
    }
    for name in &model.addons {
        text(card, name, "settingdesc");
    }

    if !model.host.is_empty() {
        let clear = button("Clear this site's data", "Remove cookies and storage kept for this site");
        let host = model.host.clone();
        clear.connect_clicked(move |_| {
            if let Some(app) = crate::app::current() {
                clear_site_data(&app, tab, &host);
            }
        });
        row(
            card,
            &format!(
                "Removes cookies and storage kept for {} and the domain it belongs to. Sibling sites are not touched.",
                model.host
            ),
            vec![clear],
        );
    }
}

/// Writes one change for this site and draws the card again.
fn apply(host: &str, kind: Kind, how: Change) {
    let Some(app) = crate::app::current() else { return };
    let grants = Records::load_permissions();
    // The private flag is false here: a private tab never gets these buttons,
    // and `change` refuses a private write on its own too.
    if let Some(next) = change(&grants, false, host, kind, how) {
        Records::save_permissions(&next);
    }
    open(&app);
}

/// Removes cookies and storage for exactly this host, from this tab's context.
fn clear_site_data(app: &App, tab: TabId, host: &str) {
    let Some(manager) = app.view(tab).and_then(|view| view.context()).and_then(|c| c.website_data_manager()) else {
        app.flash_toast("There is nothing to clear for this page.");
        return;
    };
    let host = host.to_string();
    let weak = app.weak();
    let fetch_manager = manager.clone();
    fetch_manager.fetch(site_data(), None::<&gtk::gio::Cancellable>, move |result| {
        let Ok(all) = result else { return };
        let names: Vec<String> = all.iter().map(|d| d.name().map(|n| n.to_string()).unwrap_or_default()).collect();
        let picked: Vec<WebsiteData> = clear_targets(&host, &names).into_iter().map(|i| all[i].clone()).collect();
        let Some(app) = weak.upgrade() else { return };
        dismiss(&app);
        if picked.is_empty() {
            app.flash_toast(&format!("Nothing was kept for {host}."));
            return;
        }
        // WebKit's remove callback must be Send, so the records are kept alive here,
        // under this clear's own number, until it reports back. They are not
        // captured by the callback.
        let ticket = CLEAR_TICKET.with(|t| {
            let next = t.get().wrapping_add(1);
            t.set(next);
            next
        });
        let held = picked.clone();
        CLEARING.with(|c| c.borrow_mut().push((ticket, picked)));
        let refs: Vec<&WebsiteData> = held.iter().collect();
        let done_host = host.clone();
        // The toast waits for WebKit's answer, so "Cleared" is only said once it is true.
        manager.remove(site_data(), &refs, None::<&gtk::gio::Cancellable>, move |result| {
            CLEARING.with(|c| c.borrow_mut().retain(|(number, _)| *number != ticket));
            let Some(app) = crate::app::current() else { return };
            match result {
                Ok(_) => {
                    app.flash_toast(&format!("Cleared the data kept for {done_host}."));
                }
                Err(_) => {
                    app.flash_toast(&format!("The data kept for {done_host} could not be cleared."));
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk::gio::TlsCertificateFlags as F;

    fn grant(host: &str, kind: Kind, allowed: bool) -> Grant {
        Grant { host: host.to_string(), kind, allowed }
    }

    #[test]
    fn plain_http_says_it_is_not_private() {
        let text = security("http://example.com/", None).text();
        assert!(text.starts_with("Not private."), "{text}");
    }

    #[test]
    fn https_with_a_clean_certificate_is_secure() {
        assert_eq!(security("https://example.com/", Some(F::empty())), Security::Secure);
    }

    #[test]
    fn https_with_no_certificate_is_not_called_secure() {
        assert_eq!(security("https://example.com/", None), Security::Unknown);
        assert!(!security("https://example.com/", None).text().starts_with("Secure"));
    }

    #[test]
    fn a_bad_certificate_gives_the_same_cause_as_the_error_page() {
        let Security::Problem(why) = security("https://example.com/", Some(F::EXPIRED)) else {
            panic!("an expired certificate is a problem");
        };
        assert_eq!(why, crate::web::tls_problem(F::EXPIRED));
        assert!(security("https://example.com/", Some(F::EXPIRED)).text().contains("has expired"));
    }

    #[test]
    fn a_page_that_is_not_web_has_no_connection_to_check() {
        assert_eq!(security("about:blank", None), Security::NotWeb);
        assert_eq!(security("file:///tmp/x.html", None), Security::NotWeb);
        let model = build("about:blank", None, &[], false, 1.0, Vec::new());
        assert_eq!(model.host, "");
    }

    #[test]
    fn the_host_is_lower_case_and_has_no_port() {
        let model = build("https://Example.com:443/x?q=1", Some(F::empty()), &[], false, 1.0, Vec::new());
        assert_eq!(model.host, "example.com");
    }

    #[test]
    fn only_this_hosts_saved_answers_are_listed() {
        let grants = vec![
            grant("example.com", Kind::Location, true),
            grant("other.example.com", Kind::Camera, true),
            grant("example.com.evil.test", Kind::Notifications, false),
        ];
        let shown = saved_for(&grants, "example.com");
        assert_eq!(shown, vec![grant("example.com", Kind::Location, true)]);
    }

    #[test]
    fn a_private_tab_writes_nothing() {
        let grants = vec![grant("example.com", Kind::Location, true)];
        assert_eq!(change(&grants, true, "example.com", Kind::Location, Change::Set(false)), None);
        assert_eq!(change(&grants, true, "example.com", Kind::Location, Change::Forget), None);
    }

    #[test]
    fn a_normal_tab_changes_only_this_host_and_kind() {
        let grants = vec![grant("example.com", Kind::Location, true), grant("other.example.com", Kind::Location, true)];
        let blocked = change(&grants, false, "example.com", Kind::Location, Change::Set(false)).unwrap();
        assert!(blocked.contains(&grant("example.com", Kind::Location, false)));
        assert!(blocked.contains(&grant("other.example.com", Kind::Location, true)));
        let forgotten = change(&grants, false, "example.com", Kind::Location, Change::Forget).unwrap();
        assert_eq!(forgotten, vec![grant("other.example.com", Kind::Location, true)]);
    }

    #[test]
    fn zoom_is_shown_in_whole_percent_and_resets_at_one_hundred() {
        assert_eq!(zoom_percent(1.0), 100);
        assert_eq!(zoom_percent(1.1), 110);
        assert_eq!(zoom_percent(0.4), 40);
        let model = build("https://example.com/", Some(F::empty()), &[], false, 1.5, Vec::new());
        assert_eq!(model.zoom, 150);
    }

    #[test]
    fn clearing_picks_this_site_and_its_main_domain_but_no_sibling() {
        let names = vec![
            ".example.com".to_string(),
            "example.com".to_string(),
            "EXAMPLE.com".to_string(),
            "other.example.com".to_string(),
            "example.com.evil.test".to_string(),
            "notexample.com".to_string(),
            "com".to_string(),
            "www.example.com".to_string(),
        ];
        assert_eq!(clear_targets("example.com", &names), vec![0, 1, 2]);
        // A www. page finds the data WebKit filed under the main domain.
        assert_eq!(clear_targets("www.example.com", &names), vec![0, 1, 2, 7]);
        assert_eq!(clear_targets("other.example.com", &names), vec![0, 1, 2, 3]);
    }

    #[test]
    fn an_empty_host_clears_nothing() {
        let names = vec!["".to_string(), "example.com".to_string()];
        assert!(clear_targets("", &names).is_empty());
    }
}
