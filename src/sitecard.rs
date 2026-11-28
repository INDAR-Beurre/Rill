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

struct OpenCard {
    tab: TabId,
    card: gtk::Box,
    scrim: gtk::DrawingArea,
    scrim_alpha: std::rc::Rc<Cell<f64>>,
}

thread_local! {
    /// The card on screen, its backdrop scrim, and the tab it describes. Only one is ever up.
    static OPEN: RefCell<Option<OpenCard>> = const { RefCell::new(None) };
    /// The records each clear is removing, filed under that clear's number. They
    /// stay alive until WebKit reports back for that clear, so two clears in a
    /// row never drop each other's records.
    static CLEARING: RefCell<Vec<(u64, Vec<WebsiteData>)>> = const { RefCell::new(Vec::new()) };
    /// The number the next clear is filed under.
    static CLEAR_TICKET: Cell<u64> = const { Cell::new(0) };
    /// Which button in the rebuilt card should get the keyboard back.
    static FOCUS_HINT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Whether the site card is on screen. It only asks and never moves focus.
pub fn is_open() -> bool {
    OPEN.with(|o| o.borrow().as_ref().is_some_and(|open| open.card.is_visible()))
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
    if let Ok(panels) = app.panels.try_borrow() {
        panels.dismiss_popover();
    }
    let tab = app.active_id();
    let was_open = OPEN.with(|o| o.borrow().as_ref().is_some_and(|open| open.tab == tab));
    if let Some(old) = OPEN.with(|o| o.borrow_mut().take()) {
        crate::motion::cancel(&old.card);
        crate::motion::cancel(&old.scrim);
        app.ui.overlay.remove(&old.card);
        app.ui.overlay.remove(&old.scrim);
    }
    for child in app.ui.overlay.children() {
        let name = child.widget_name();
        if name == "site-card" || name == "site-card-scrim" {
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
        app.zoom_for(&url, private),
        extensions::here(&url),
    );
    let title = if model.host.is_empty() { "This page".to_string() } else { model.host.clone() };
    let (scrim, scrim_alpha) = crate::ui::scrim_with_alpha(&app.palette.borrow());
    scrim.set_widget_name("site-card-scrim");
    {
        let weak = app.weak();
        scrim.connect_button_press_event(move |_, _| {
            if let Some(app) = weak.upgrade() {
                dismiss(&app);
            }
            crate::gtkx::inh(true)
        });
    }
    app.ui.overlay.add_overlay(&scrim);
    scrim.show_all();
    let card = permissions::card(app, &title);
    card.set_widget_name("site-card");
    card.set_size_request(380, -1);
    if let Some(first) = card.children().into_iter().next() {
        card.remove(&first);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.pack_start(&first, true, true, 0);
        let close = crate::icons::button(
            crate::icons::Icon::Close,
            14,
            app.palette.borrow().muted,
            "panelclose",
        );
        close.set_can_focus(true);
        close.set_tooltip_text(Some("Close site information (Escape)"));
        if let Some(access) = close.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Close site information");
        }
        let weak = app.weak();
        close.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                dismiss(&app);
            }
        });
        head.pack_end(&close, false, false, 0);
        card.pack_start(&head, false, false, 0);
        card.reorder_child(&head, 0);
    }
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
    // The card opens under the shield button that opened it. The shared
    // prompt card sits at the far right, which put the site card about 560px
    // away from that button. Readers then saw it as a separate prompt.
    // The anchor is measured only after the rows are added and shown, because
    // the width depends on what the card holds: round 54 drew it about 458px
    // wide, not the 380px it was told to be.
    let anchor_top = anchor_under_button(app, &card);
    if !was_open {
        // Both the scrim and the card arrive on the shared entrance role so
        // clicking outside dismisses via a real alpha-animated backdrop.
        crate::motion::fade_alpha(&scrim, &scrim_alpha, crate::motion::dur::ENTER);
        // Every card under the toolbar drops in the same shared 8px on the
        // entrance role, so the site card, dialogs, welcome and permission
        // prompts all arrive the same way.
        crate::motion::pop_card_in(
            &card,
            anchor_top,
            -crate::motion::dur::TRAVEL,
            crate::motion::dur::ENTER,
        );
    } else {
        scrim_alpha.set(1.0);
    }
    // Rebuilding after a zoom or permission change used to drop the keyboard
    // on the first button again. Keep the control the person was on, matched
    // by its tip, so a second click or a press of Space lands on the same
    // action in the new card.
    let wanted = FOCUS_HINT.with(|hint| hint.borrow_mut().take());
    let next = wanted
        .as_deref()
        .and_then(|tip| interactive_buttons(&content).into_iter().find(|b| b.tooltip_text().as_deref() == Some(tip)))
        .or_else(|| first_interactive(&content));
    if let Some(button) = next {
        button.grab_focus();
    } else {
        card.set_can_focus(true);
        card.grab_focus();
    }
    OPEN.with(|o| {
        *o.borrow_mut() = Some(OpenCard {
            tab,
            card,
            scrim,
            scrim_alpha,
        });
    });
}

/// Puts the site card 8px below the site-information button, with its left
/// edge lined up with the button. Returns the top margin it set, so the
/// entrance animation drops into that same place.
///
/// The card cannot run past the right edge of the page: at a narrow width it
/// moves left until it fits inside PAGE_INSET. If the button has no position
/// yet (it is not mapped), the card falls back to the shared prompt position.
fn anchor_under_button(app: &crate::app::App, card: &gtk::Box) -> i32 {
    let Some((left, top)) = anchor_point(app, card) else {
        return crate::find::PAGE_TOP;
    };
    card.set_halign(gtk::Align::Start);
    card.set_margin_start(left);
    card.set_margin_end(0);
    card.set_margin_top(top);
    top
}

/// Keeps an open card under its button after the layout moves: a window
/// resize, a column fold or unfold, or a toolbar reflow. Called from the
/// overlay's and the toolbar's size_allocate handlers.
///
/// Only the left edge is updated. The top margin is what the entrance
/// animation moves, and these handlers run on every animation frame (each
/// margin step re-allocates the overlay), so writing the top here would cut
/// the entrance short. The button's height and the toolbar's height do not
/// change on resize or fold, so the top stays correct anyway. GTK ignores a
/// margin set to the value it already has, so repeated calls settle and do
/// not loop.
pub fn relayout(app: &App) {
    let Some(card) = OPEN.with(|o| o.borrow().as_ref().map(|open| open.card.clone())) else {
        return;
    };
    if !card.is_mapped() {
        return;
    }
    if let Some((left, _)) = anchor_point(app, &card) {
        card.set_margin_start(left);
    }
}

const MIN_CARD_WIDTH: i32 = 380;

/// Pure horizontal anchor for the hidden-toolbar fallback: keeps the card's
/// right edge inset by `PAGE_INSET` on both wide and narrow windows.
fn hidden_toolbar_left(overlay_width: i32, card_width: i32) -> i32 {
    (overlay_width - card_width - crate::find::PAGE_INSET).max(crate::find::PAGE_INSET)
}

/// Drawn card width derived from GTK's preferred width minus its own margins.
fn measured_card_width(card: &gtk::Box) -> i32 {
    let (minimum, natural) = card.preferred_width();
    let margins = card.margin_start() + card.margin_end();
    (natural.max(minimum) - margins).max(MIN_CARD_WIDTH)
}

/// Where the card goes: (left, top) in overlay coordinates, or None when the
/// site-information button has no position yet.
fn anchor_point(app: &crate::app::App, card: &gtk::Box) -> Option<(i32, i32)> {
    const GAP: i32 = 8;
    let button = &app.ui.site_info;
    let overlay = &app.ui.overlay;
    // Clamp with the width the card will actually be drawn at, not an
    // assumed 380px or an unallocated 1px width. GTK's preferred width
    // includes the widget's own margins, so subtracting both horizontal
    // margins gives the drawn width on both first open and relayout.
    let card_width = measured_card_width(card);
    if !button.is_mapped() {
        // The toolbar can be hidden by preference or fullscreen. A site card
        // still needs a stable anchor, so use the page column's top-right
        // below the active top bar with the real preferred width so PAGE_INSET
        // is preserved on wide and narrow windows alike.
        let left = hidden_toolbar_left(overlay.allocated_width(), card_width);
        let top = crate::find::toolbar_band(&app.ui).max(crate::find::PAGE_TOP) + GAP;
        return Some((left, top));
    }
    let (x, y) = button.translate_coordinates(overlay, 0, button.allocated_height())?;
    // Both edges: the right edge stays inside PAGE_INSET, and the left edge
    // never goes past the 12px surface guide. In a window too narrow for both,
    // the left guide wins.
    let max_left = (overlay.allocated_width() - card_width - crate::find::PAGE_INSET).max(12);
    let left = x.clamp(12, max_left);
    // Keep the card below the chrome. A button near the top edge can anchor
    // the card over the address row, which hides the very control it explains.
    let toolbar_bottom = app
        .ui
        .toolbar
        .translate_coordinates(overlay, 0, app.ui.toolbar.allocated_height())
        .map(|(_, bottom)| bottom + GAP)
        .unwrap_or(0);
    let top = y.max(toolbar_bottom) + GAP;
    Some((left, top))
}

fn interactive_buttons(box_: &gtk::Box) -> Vec<gtk::Button> {
    box_.children()
        .into_iter()
        .flat_map(|child| {
            if let Ok(button) = child.clone().downcast::<gtk::Button>() {
                return vec![button];
            }
            child
                .downcast::<gtk::Box>()
                .map(|row| {
                    row.children()
                        .into_iter()
                        .filter_map(|w| w.downcast::<gtk::Button>().ok())
                        .collect()
                })
                .unwrap_or_default()
        })
        .filter(|button| button.is_sensitive())
        .collect()
}

fn first_interactive(box_: &gtk::Box) -> Option<gtk::Button> {
    interactive_buttons(box_).into_iter().next()
}

/// Takes the card down. The keyboard goes back to the page.
pub fn dismiss(app: &App) {
    let Some(open) = OPEN.with(|o| o.borrow_mut().take()) else { return };
    crate::motion::cancel(&open.card);
    crate::motion::cancel(&open.scrim);
    permissions::focus_page(app);
    let overlay = app.ui.overlay.clone();
    let card_for_done = open.card.clone();
    let scrim_for_done = open.scrim.clone();
    crate::motion::fade_alpha_out(&open.scrim, &open.scrim_alpha, crate::motion::dur::EXIT);
    crate::motion::fade_out_then(&open.card, crate::motion::dur::EXIT, move || {
        overlay.remove(&card_for_done);
        overlay.remove(&scrim_for_done);
        for child in overlay.children() {
            if child.widget_name() == "site-card-scrim" {
                overlay.remove(&child);
            }
        }
    });
}

/// A tab switch or a close takes the card only when it describes that tab.
pub fn dismiss_tab(app: &App, tab: TabId) {
    let mine = OPEN.with(|o| o.borrow().as_ref().is_some_and(|open| open.tab == tab));
    if mine {
        dismiss(app);
    }
}

/// Escape takes the card down, and only when it is the thing on screen.
pub fn key(app: &App, event: &gtk::gdk::EventKey) -> bool {
    if event.keyval() != gtk::gdk::keys::constants::Escape {
        return false;
    }
    let showing = OPEN.with(|o| o.borrow().as_ref().is_some_and(|open| open.card.is_visible()));
    if !showing {
        return false;
    }
    dismiss(app);
    true
}

fn text(card: &gtk::Box, message: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(message));
    crate::ui::class(&label, class);
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_max_width_chars(44);
    card.add(&label);
    label
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

/// A thin rule between the card's groups. Gemini R59 fix 10 found the zoom
/// row, Permissions and Add-ons running together as one list, with only the
/// small group label between them. A 1px separator with even space above and
/// below marks where each group starts. The colour comes from the theme's
/// separator rule, the same divider the tab column uses.
fn rule(card: &gtk::Box) {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    crate::ui::class(&separator, "sitecard-rule");
    separator.set_margin_top(6);
    separator.set_margin_bottom(2);
    card.add(&separator);
}

fn add_rows(card: &gtk::Box, tab: TabId, model: &Model) {
    let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    crate::ui::class(&status_box, "sitecard-status");
    let warning = matches!(model.security, Security::Plain | Security::Problem(_));
    if warning {
        crate::ui::class(&status_box, "warning");
    } else if matches!(model.security, Security::Secure) {
        crate::ui::class(&status_box, "secure");
    }
    if let Some(app) = crate::app::current() {
        let palette = *app.palette.borrow();
        let (icon_kind, icon_color) = match model.security {
            Security::Secure => (crate::icons::Icon::Lock, palette.accent()),
            Security::Plain | Security::Problem(_) => (crate::icons::Icon::ShieldAlert, palette.danger),
            _ => (crate::icons::Icon::Globe, palette.muted),
        };
        let glyph = crate::icons::area(icon_kind, 18, icon_color);
        glyph.set_valign(gtk::Align::Start);
        glyph.set_margin_top(2);
        status_box.pack_start(&glyph, false, false, 0);
    }
    let security = gtk::Label::new(Some(&model.security.text()));
    crate::ui::class(&security, "settingdesc");
    security.set_xalign(0.0);
    security.set_line_wrap(true);
    security.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    security.set_max_width_chars(38);
    if warning {
        crate::ui::class(&security, "security-warning");
        if let Some(app) = crate::app::current() {
            let danger = app.palette.borrow().danger;
            let attrs = gtk::pango::AttrList::new();
            attrs.insert(gtk::pango::AttrColor::new_foreground(
                u16::from(danger.0) * 257,
                u16::from(danger.1) * 257,
                u16::from(danger.2) * 257,
            ));
            attrs.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold));
            security.set_attributes(Some(&attrs));
        }
    }
    status_box.pack_start(&security, true, true, 0);
    card.add(&status_box);

    if !model.host.is_empty() {
        let global_on = crate::app::current()
            .map(|app| app.prefs.borrow().shield)
            .unwrap_or(true);
        let site_off = crate::shield::allowed(&model.host);
        let label = if !global_on {
            "Shield off globally"
        } else if site_off {
            "Shield paused on this site"
        } else {
            "Shield blocking trackers"
        };
        let toggle = button(
            if site_off { "Resume" } else { "Pause here" },
            if site_off {
                "Turn ad and tracker blocking back on for this site"
            } else {
                "Pause ad and tracker blocking for this site"
            },
        );
        toggle.set_sensitive(global_on && !model.private);
        let tip_key = if site_off {
            "Turn ad and tracker blocking back on for this site"
        } else {
            "Pause ad and tracker blocking for this site"
        };
        toggle.connect_clicked(move |_| {
            if let Some(app) = crate::app::current() {
                FOCUS_HINT.with(|hint| *hint.borrow_mut() = Some(tip_key.into()));
                app.toggle_blocker();
                open(&app);
            }
        });
        row(card, label, vec![toggle]);
    }

    let zoom_out = button("-", "Zoom out");
    if let Some(access) = zoom_out.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Zoom out");
    }
    zoom_out.set_sensitive(model.zoom > 40);
    zoom_out.connect_clicked(move |_| {
        if let Some(app) = crate::app::current() {
            FOCUS_HINT.with(|hint| *hint.borrow_mut() = Some("Zoom out".into()));
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
            FOCUS_HINT.with(|hint| *hint.borrow_mut() = Some("Zoom in".into()));
            app.zoom(0.1);
            open(&app);
        }
    });
    let reset = button("Reset zoom", "Back to the normal size for this site");
    reset.set_sensitive(model.zoom != 100);
    reset.connect_clicked(move |_| {
        if let Some(app) = crate::app::current() {
            FOCUS_HINT.with(|hint| *hint.borrow_mut() = Some("Back to the normal size for this site".into()));
            app.reset_zoom();
            open(&app);
        }
    });
    row(card, &format!("Zoom {}%", model.zoom), vec![zoom_out, zoom_in, reset]);

    rule(card);
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
        let flip_tip = format!("{} this from now on", if allowed { "Block" } else { "Allow" });
        flip.connect_clicked(move |_| {
            FOCUS_HINT.with(|hint| *hint.borrow_mut() = Some(flip_tip.clone()));
            apply(&flip_host, flip_kind, Change::Set(!allowed));
        });
        let forget = button("Forget", "Ask again next time");
        forget.connect_clicked(move |_| {
            FOCUS_HINT.with(|hint| *hint.borrow_mut() = Some("Ask again next time".into()));
            apply(&host, kind, Change::Forget);
        });
        row(card, &what, vec![flip, forget]);
    }

    rule(card);
    text(card, "Add-ons on this page", "group");
    if model.addons.is_empty() {
        text(card, "No installed add-on acts on this page.", "settingdesc");
    }
    for name in &model.addons {
        text(card, name, "settingdesc");
    }

    if !model.host.is_empty() {
        // Clearing data cannot be undone, so it gets its own section. Without a
        // divider it read as one more line of the add-on list above it
        // (Gemini R60 polish). It uses the same rule() hairline as the
        // permission and add-on sections, so the card keeps one rhythm.
        rule(card);
        let clear = button("Clear this site's data", "Remove cookies and storage kept for this site");
        crate::ui::class(&clear, "sitecard-clear");
        let host = model.host.clone();
        clear.connect_clicked(move |_| {
            if let Some(app) = crate::app::current() {
                FOCUS_HINT.with(|hint| *hint.borrow_mut() = None);
                clear_site_data(&app, tab, &host);
            }
        });
        // The explanation and the destructive button share a column: beside
        // a three-line paragraph the button crowded the second line (Gemini
        // R73 review). Give the words their own lines, then the action.
        let clear_block = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let clear_note = text(
            &clear_block,
            &format!(
                "Removes cookies and storage kept for {} and the domain it belongs to. Sibling sites are not touched.",
                model.host
            ),
            "settingdesc",
        );
        clear_note.set_line_wrap(true);
        clear_note.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        clear_note.set_max_width_chars(48);
        clear.set_halign(gtk::Align::Start);
        clear_block.add(&clear);
        card.add(&clear_block);
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
    fn hidden_toolbar_anchor_preserves_right_inset_on_wide_and_narrow_windows() {
        let inset = crate::find::PAGE_INSET;
        for overlay_width in [1500, 1180, 900] {
            for card_width in [380, 416, 458] {
                let left = hidden_toolbar_left(overlay_width, card_width);
                assert_eq!(
                    overlay_width - (left + card_width),
                    inset,
                    "right gap should equal PAGE_INSET ({inset}) at overlay={overlay_width}, card={card_width}"
                );
            }
        }
        // On a very tiny window, never push past the left inset.
        assert_eq!(hidden_toolbar_left(300, 416), inset);
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
