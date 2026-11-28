// The one field: an address, a few words, or the name of a tab. It rises over
// the page when asked for and never sends what was typed until Return.
use crate::app::{App, AppRef, TabId};
use crate::layer::Layer;
use crate::palette::Palette;
use gtk::prelude::*;
use webkit2gtk::WebViewExt;

pub const FIELD_WIDTH: i32 = 640;
const FIELD_HEIGHT: i32 = 52;
const FOOTER_HEIGHT: i32 = 32;

/// Field width clamped to the available page width on narrow windows.
fn field_width_for(page: i32) -> i32 {
    FIELD_WIDTH.min((page - 24).max(260))
}
/// The most of the window the list may take. A cap, not a crop: past it the
/// list scrolls.
const LIST_CAP: i32 = 420;
/// The list height the field was measured against under its tallest list: the
/// 1500x917 capture put the field's top edge at y=190 with 359px of list under
/// it. The anchor keeps using that sum, so after this change the field stands
/// in exactly the same place as before and no list can move it.
const LIST_RESERVE: i32 = 359;
/// The highest the field may stand, so its top edge never reaches the lights.
const TOP_MIN: i32 = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ChipKind {
    /// Active page over verified HTTPS (`!failed && Security::Secure`): accent Lock.
    VerifiedHttps,
    /// Active page with failure / plain HTTP / bad or missing TLS, or explicit `http://`:
    /// danger ShieldAlert with `.insecure`.
    Insecure,
    /// Explicit typed/unvisited destination (bare host or `https://` not verified on
    /// this tab): neutral Globe, never a false verified Lock.
    Destination,
}

impl ChipKind {
    fn as_str(self) -> &'static str {
        match self {
            ChipKind::VerifiedHttps => "verified",
            ChipKind::Insecure => "insecure",
            ChipKind::Destination => "destination",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ChipModel {
    host: String,
    kind: ChipKind,
}

/// Security chip for the page currently open in the active tab.
/// Derived strictly from `tab.failed` and `sitecard::security(url, tls)`, never
/// from the URL prefix alone.
fn active_page_chip(
    url: &str,
    failed: bool,
    tls: Option<gtk::gio::TlsCertificateFlags>,
) -> Option<ChipModel> {
    let host = chip_host(url);
    if host.is_empty() {
        return None;
    }
    let kind = match (failed, crate::sitecard::security(url, tls)) {
        (false, crate::sitecard::Security::Secure) => ChipKind::VerifiedHttps,
        (true, _)
        | (
            _,
            crate::sitecard::Security::Plain
            | crate::sitecard::Security::Problem(_)
            | crate::sitecard::Security::Unknown,
        ) => ChipKind::Insecure,
        (false, crate::sitecard::Security::NotWeb) => ChipKind::Destination,
    };
    Some(ChipModel { host, kind })
}

/// Whether `typed` names an explicit destination (scheme URL or bare host) rather
/// than a search query or tab-switch keyword.
fn explicit_destination(typed: &str) -> Option<String> {
    let trimmed = typed.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("about:") || lower.starts_with("file:") {
        return None;
    }
    if trimmed.contains("://") {
        let (scheme, _, _) = split_url(trimmed)?;
        if !scheme.eq_ignore_ascii_case("http://") && !scheme.eq_ignore_ascii_case("https://") {
            return None;
        }
        return Some(trimmed.to_string());
    }
    // Reject non-HTTP(S) opaque schemes such as `javascript:alert(a.b)` or
    // `data:text/html,a.b` while still allowing host:port (e.g. `localhost:8080/x`)
    // and paths containing colons (e.g. `example.com/wiki/Category:Rust`).
    if let Some((prefix, rest)) = trimmed.split_once(':') {
        if looks_like_a_scheme(prefix) {
            let port_part = rest.split(['/', '?', '#']).next().unwrap_or(rest);
            let is_port = !port_part.is_empty() && port_part.chars().all(|c| c.is_ascii_digit());
            if !is_port {
                return None;
            }
        }
    }
    let hostlike = trimmed.contains('.')
        || trimmed.starts_with("localhost")
        || trimmed.starts_with("127.0.0.1");
    if hostlike {
        Some(format!("https://{trimmed}"))
    } else {
        None
    }
}

fn same_origin_authority(a: &str, b: &str) -> bool {
    match (split_url(a), split_url(b)) {
        (Some((sa, aa, _)), Some((sb, ab, _))) => {
            sa.eq_ignore_ascii_case(sb) && aa.eq_ignore_ascii_case(ab)
        }
        _ => false,
    }
}

/// Chip shown for text currently typed in the omnibox.
/// - Plain searches / tab-switch words (`"office"`, `"two words"`, `"yt cats"`)
///   and empty text get `None` (never a search engine's host or lock).
/// - Paths starting with `/` refer to the active page's host and inherit its
///   verified/insecure chip state.
/// - Explicit destinations (`example.com`, `https://...`, `http://...`) get a
///   host intent chip: `Insecure` for explicit `http://`, active-page state if
///   the origin matches the active tab, and neutral `Destination` (no lock) for
///   any unvisited HTTPS/host destination.
fn typed_chip(
    current_url: &str,
    current_failed: bool,
    current_tls: Option<gtk::gio::TlsCertificateFlags>,
    typed: &str,
) -> Option<ChipModel> {
    let trimmed = typed.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('/') {
        split_url(current_url)?;
        return active_page_chip(current_url, current_failed, current_tls);
    }
    let dest = explicit_destination(trimmed)?;
    let host = chip_host(&dest);
    if host.is_empty() {
        return None;
    }
    if same_origin_authority(&dest, current_url) {
        return active_page_chip(current_url, current_failed, current_tls);
    }
    let kind = if dest.to_ascii_lowercase().starts_with("http://") {
        ChipKind::Insecure
    } else {
        ChipKind::Destination
    };
    Some(ChipModel { host, kind })
}

#[derive(Clone)]
pub struct Offer {
    pub title: String,
    pub url: String,
    pub kind: String,
    /// Exact open tab ID when this row represents an open tab (including in another Space, T30).
    pub tab_id: Option<TabId>,
    /// Space name shown when the open tab belongs to a different Space than the active one.
    pub space_name: Option<String>,
}

impl Offer {
    pub fn simple(title: String, url: String, kind: &str) -> Self {
        Self {
            title,
            url,
            kind: kind.to_string(),
            tab_id: None,
            space_name: None,
        }
    }

    pub fn open_tab(title: String, url: String, tab_id: TabId, space_name: Option<String>) -> Self {
        Self {
            title,
            url,
            kind: "Open".to_string(),
            tab_id: Some(tab_id),
            space_name,
        }
    }
}

pub struct Omnibox {
    pub offers: std::cell::RefCell<Vec<Offer>>,
    column: std::cell::Cell<i32>,
    band: std::cell::Cell<i32>,
    last_size: std::cell::Cell<(i32, i32)>,
    pub picked: std::cell::Cell<i32>,
    pub holder: gtk::Fixed,
    layer: Layer,
    pub scrim: gtk::DrawingArea,
    scrim_alpha: std::rc::Rc<std::cell::Cell<f64>>,
    pub block: gtk::Box,
    pub field: gtk::Box,
    pub entry: gtk::Entry,
    pub site: gtk::Label,
    site_icon: gtk::Box,
    site_pill: gtk::Box,
    pub site_box: gtk::EventBox,
    chip_kind: std::cell::Cell<Option<ChipKind>>,
    /// The footer's action label. Its verb follows the first row: an Open
    /// row switches tabs, so "Enter  open" would promise the wrong thing.
    commit: gtk::Label,
    pub list: gtk::Box,
    pub list_scroll: gtk::ScrolledWindow,
    /// The rows, in draw order, so a pick can restyle one row instead of
    /// rebuilding every one of them: a rebuild leaves the whole list needing a
    /// fresh allocation, and GTK drops a draw on a widget that needs one.
    rows: std::cell::RefCell<Vec<gtk::EventBox>>,
    pub visible: std::rc::Rc<std::cell::Cell<bool>>,
}

/// A scheme is a name, not a sentence: `data:text/html,<a href="http://x">`
/// has an "://" in it, and reading the site out of that would make a chip that
/// says `x">` over a page that is not on the web at all.
fn looks_like_a_scheme(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
}

/// Split an address into the part before the site, the site itself, and the
/// rest. `None` when there is no authority at all: `about:blank` and a bare
/// path have no site, and reading one out of them invents a host.
fn split_url(url: &str) -> Option<(&str, &str, &str)> {
    let at = url.find("://")?;
    if !looks_like_a_scheme(&url[..at]) {
        return None;
    }
    let after = &url[at + 3..];
    let end = after
        .find(['/', '?', '#'])
        .unwrap_or(after.len());
    let authority = &after[..end];
    if authority.is_empty() {
        return None;
    }
    Some((&url[..at + 3], authority, &after[end..]))
}

/// The site a chip may name: the host without a user or a port, because a
/// chip reading "me@example.com:443" says three things where one is the
/// site. A bracketed address keeps its brackets, so the chip shows the whole
/// address rather than the first colon of it.
fn site_of(authority: &str) -> &str {
    let bare = authority.rsplit('@').next().unwrap_or(authority);
    if bare.starts_with('[') {
        return match bare.find(']') {
            Some(close) => &bare[..=close],
            None => bare,
        };
    }
    bare.split(':').next().unwrap_or(bare)
}

/// The host the chip may name, or nothing when the address has no site.
fn chip_host(url: &str) -> String {
    if url.is_empty() || url.starts_with("file:") {
        return String::new();
    }
    match split_url(url) {
        Some((_, authority, _)) => site_of(authority).to_lowercase(),
        None => String::new(),
    }
}

/// What the field shows for an address whose site the chip already names: the
/// part from the path on, so the chip's host is not said twice. The chip
/// strips the port, so the site is taken out of the authority the same way;
/// when it is not the same site, the field keeps the whole address rather
/// than a tail of one page under the name of another.
fn here_for_field(url: &str, host: &str) -> String {
    if host.is_empty() || url.is_empty() {
        return url.to_string();
    }
    let Some((_, authority, rest)) = split_url(url) else { return url.to_string() };
    if !site_of(authority).eq_ignore_ascii_case(host) {
        return url.to_string();
    }
    // A bare host has an empty tail, and an empty field with a chip beside it
    // is a field that looks broken. The path it is on is "/".
    match rest {
        "" | "/" => "/".to_string(),
        _ if rest.starts_with('/') => rest.to_string(),
        _ => format!("/{rest}"),
    }
}

/// Split a URL into `(host_label, path_label)` for clear autocomplete rows.
/// Keeps `http://` visible on insecure origins while presenting verified or
/// standard `https://` hosts cleanly without protocol clutter.
pub fn host_and_path(url: &str) -> (String, String) {
    let Some((before, authority, rest)) = split_url(url) else {
        return (url.to_string(), String::new());
    };
    let bare_authority = authority.rsplit('@').next().unwrap_or(authority);
    let host_label = if before.eq_ignore_ascii_case("https://") {
        bare_authority.to_string()
    } else {
        format!("{before}{bare_authority}")
    };
    let path_label = if rest.is_empty() || rest == "/" {
        String::new()
    } else {
        rest.to_string()
    };
    (host_label, path_label)
}

/// Reusable single-line `host + path` widget for Omnibox, Start Page, and
/// Bookmarks Bar autocomplete suggestions.
pub fn build_offer_url_box(url: &str) -> gtk::Box {
    let (host_text, path_text) = host_and_path(url);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    crate::ui::class(&line, "sgurl-row");
    line.set_hexpand(true);
    let host = gtk::Label::new(Some(&host_text));
    crate::ui::class(&host, "sghost");
    host.set_xalign(0.0);
    host.set_width_chars(1);
    host.set_ellipsize(gtk::pango::EllipsizeMode::End);
    host.set_single_line_mode(true);
    if path_text.is_empty() {
        crate::ui::pack(&line, &host, true);
    } else {
        host.set_max_width_chars(24);
        crate::ui::pack(&line, &host, false);
        let path = gtk::Label::new(Some(&path_text));
        crate::ui::class(&path, "sgpath");
        path.set_xalign(0.0);
        path.set_width_chars(1);
        path.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        path.set_single_line_mode(true);
        crate::ui::pack(&line, &path, true);
    }
    line
}

/// Build the left-hand suggestion icon: real cached site favicon for URLs when
/// available (so bookmarks and visited pages keep their site icon), falling
/// back to a crisp Cairo kind glyph rather than a generic letter.
pub fn build_offer_icon(
    app: &App,
    url: &str,
    kind: &str,
    fallback_color: crate::palette::Rgb,
) -> gtk::Box {
    let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    holder.set_size_request(18, 18);
    holder.set_valign(gtk::Align::Center);
    holder.set_halign(gtk::Align::Center);
    let use_favicon = matches!(kind, "Bookmark" | "Open" | "History");
    if use_favicon {
        if let Some(surface) = app.favicon_for_url(url) {
            let image = crate::ui::favicon(&surface, 16);
            crate::ui::class(&image, "favicon");
            crate::ui::class(&image, "sgicon-favicon");
            image.set_halign(gtk::Align::Center);
            image.set_valign(gtk::Align::Center);
            crate::ui::pack(&holder, &image, true);
            return holder;
        }
    }
    let glyph = match kind {
        "Open" => crate::icons::Icon::Tab,
        "History" => crate::icons::Icon::Clock,
        "Bookmark" => crate::icons::Icon::Star,
        "Search" | "Suggest" => crate::icons::Icon::Search,
        _ => crate::icons::Icon::Globe,
    };
    let mark = crate::icons::area(glyph, 15, fallback_color);
    mark.set_size_request(18, 18);
    mark.set_valign(gtk::Align::Center);
    crate::ui::pack(&holder, &mark, true);
    holder
}

/// Where the field and its list stand for a window of this height.
#[derive(Debug, PartialEq)]
struct Plan {
    top: i32,
    /// The height the list is given, which is what it asks for until the room
    /// under the field runs out.
    list: i32,
    block: i32,
}

/// How tall the list may grow in a window this tall, under a field that
/// stands at `top`.
fn room_under(top: i32, height: i32) -> i32 {
    (height - top - FIELD_HEIGHT - FOOTER_HEIGHT - 12).clamp(0, LIST_CAP)
}

/// The anchor comes from the window height alone, never from what is in the
/// list: a field that re-centres when the first suggestion arrives moves under
/// the caret while the user is typing. The list takes the room left under the
/// anchor and scrolls past it, so the field stands still and the list gives
/// way.
fn plan(height: i32, list_height: i32) -> Plan {
    // The field, plus the room a full list has always been given, centred as
    // one block and lifted a little. What is really in the list is not part
    // of that sum, so nothing that arrives in it can move the field.
    let top = ((height - (FIELD_HEIGHT + FOOTER_HEIGHT + LIST_RESERVE)) / 2 - 24).max(TOP_MIN);
    let list = list_height.clamp(0, room_under(top, height));
    Plan {
        top,
        list,
        block: FIELD_HEIGHT + FOOTER_HEIGHT + list,
    }
}

impl Omnibox {
    pub fn new(palette: &Palette) -> Omnibox {
        let layer = Layer::new();

        let (scrim, scrim_alpha) = crate::ui::scrim_with_alpha(palette);
        layer.add(&scrim);

        // The list scrolls inside a cap instead of being cropped by one: eight
        // kinds of suggestion must not run off the bottom of the screen.
        let list_scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        list_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        list_scroll.set_propagate_natural_width(false);
        list_scroll.set_min_content_width(0);
        crate::ui::class(&list_scroll, "suggestscroll");
        // Visibility is explicit: `no_show_all` leaves later rows allocated
        // 1x1 even after the flags are cleared, which renders as a blank list.
        list_scroll.hide();

        let block = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&block, "omni-block");
        block.set_halign(gtk::Align::Start);
        block.set_valign(gtk::Align::Start);

        let field = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        crate::ui::class(&field, "omni");
        field.set_size_request(FIELD_WIDTH, FIELD_HEIGHT);

        let magnifier = crate::icons::area(crate::icons::Icon::Search, 14, palette.muted);
        crate::ui::class(&magnifier, "omni-search-icon");
        magnifier.set_margin_start(16);
        magnifier.set_valign(gtk::Align::Center);
        crate::ui::pack(&field, &magnifier, false);

        let site_pill = gtk::Box::new(gtk::Orientation::Horizontal, 5);
        crate::ui::class(&site_pill, "omnisite-pill");
        site_pill.set_valign(gtk::Align::Center);
        let site_icon = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        site_icon.set_valign(gtk::Align::Center);
        let site = gtk::Label::new(None);
        crate::ui::class(&site, "omnisite");
        site.set_ellipsize(gtk::pango::EllipsizeMode::End);
        site.set_max_width_chars(16);
        crate::ui::pack(&site_pill, &site_icon, false);
        crate::ui::pack(&site_pill, &site, false);
        // Clicking the site chip opens its card, the same as Ctrl I.
        let site_box = gtk::EventBox::new();
        site_box.set_no_show_all(true);
        site_box.set_visible_window(false);
        site_box.set_valign(gtk::Align::Center);
        site_box.set_margin_start(4);
        crate::ui::class(&site_box, "omnisite-hit");
        if let Some(access) = site_box.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Site information");
        }
        site_box.add(&site_pill);
        site_box.connect_button_press_event(|_, _| {
            if let Some(app) = crate::app::current() {
                app.close_omni();
                crate::sitecard::open(&app);
            }
            glib::Propagation::Stop
        });
        crate::ui::pack(&field, &site_box, false);

        let entry = gtk::Entry::new();
        crate::ui::class(&entry, "omnitext");
        entry.set_placeholder_text(Some("Search or enter address"));
        entry.set_width_chars(1);
        entry.set_hexpand(true);
        entry.set_margin_end(4);
        entry.set_valign(gtk::Align::Center);
        entry.set_has_frame(false);
        if let Some(access) = entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Address and search");
        }
        crate::ui::pack(&field, &entry, true);
        let escape = gtk::Label::new(Some("Esc"));
        crate::ui::class(&escape, "omni-escape");
        escape.set_valign(gtk::Align::Center);
        escape.set_margin_end(16);
        crate::ui::pack(&field, &escape, false);
        block.add(&field);

        // The ring is the only sign the field is live, so it follows the
        // entry: a field holding focus is a field you are typing into. It is
        // also put on when the field is shown, because a window the desktop
        // has not focused yet gives its entry no focus at all, and a field
        // with no ring reads as a static label.
        {
            // A weak field: the closure lives on the entry and the entry is a
            // child of the field, so a strong handle here would be a cycle
            // that keeps both alive for as long as the app runs.
            let ring = field.downgrade();
            entry.connect_has_focus_notify(move |entry| {
                let Some(ring) = ring.upgrade() else { return };
                if entry.has_focus() {
                    crate::ui::class(&ring, "focused");
                    return;
                }
                // Focus moving inside an inactive window is the compositor's
                // doing, not the user's: the field is still the one that will
                // take the typing, so it keeps its ring until the field is put
                // away or the user really clicks elsewhere.
                let active = entry
                    .toplevel()
                    .and_then(|top| top.downcast::<gtk::Window>().ok())
                    .map(|window| window.is_active())
                    .unwrap_or(false);
                if active {
                    ring.style_context().remove_class("focused");
                }
            });
        }

        // Ctrl Return is the field's way of opening what it names beside the
        // page instead of over it. GTK owns plain Return; a modified Return is
        // not promised to reach the same `activate`, so it is taken here and
        // handed to the same `commit` the key reaches. Plain Return is left to
        // GTK, so one press is still one commit.
        entry.connect_key_press_event(|_, event| {
            let control = event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let returns = matches!(
                event.keyval().name().as_deref(),
                Some("Return") | Some("KP_Enter") | Some("ISO_Enter")
            );
            if !control || !returns {
                return crate::gtkx::inh(false);
            }
            // A field is only up with an app behind it, and with none there is
            // nothing to commit to.
            if let Some(app) = crate::app::current() {
                commit(&app);
            }
            crate::gtkx::inh(true)
        });

        // Paste and Go: the field's own right-click menu gains one item that
        // pastes what is on the clipboard and goes there at once, the same way
        // Return would (a keyword, an address or a search). The clipboard is
        // read when the menu opens, so the item is greyed out when there is
        // nothing to paste instead of offering a press that does nothing. A
        // pasted block with line breaks is one line of words, not several.
        entry.connect_populate_popup(|_, popup| {
            let Some(menu) = popup.downcast_ref::<gtk::Menu>() else { return };
            let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
            let text = clipboard
                .wait_for_text()
                .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
                .filter(|t| !t.is_empty());
            let separator = gtk::SeparatorMenuItem::new();
            let item = gtk::MenuItem::with_label("Paste and Go");
            item.set_sensitive(text.is_some());
            item.connect_activate(move |_| {
                let (Some(app), Some(text)) = (crate::app::current(), text.as_deref()) else { return };
                let target = crate::app::resolve(&app, text);
                if target.is_empty() {
                    return;
                }
                // Never a picked row: the clipboard is what was asked for.
                app.omni.picked.set(-1);
                choose(&app, &target, false, false, None);
            });
            menu.append(&separator);
            menu.append(&item);
            separator.show();
            item.show();
        });

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "suggest");
        list.set_hexpand(true);
        list.hide();
        list_scroll.add(&list);
        block.add(&list_scroll);

        let footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        crate::ui::class(&footer, "omni-footer");
        let guidance = gtk::Label::new(Some("↑ ↓  choose   Tab  complete"));
        guidance.set_xalign(0.0);
        guidance.set_hexpand(true);
        crate::ui::pack(&footer, &guidance, true);
        let commit = gtk::Label::new(Some("Enter  open"));
        crate::ui::pack(&footer, &commit, false);
        crate::ui::pack(&block, &footer, false);

        layer.add(&block);
        let holder = layer.root.clone();

        Omnibox {
            offers: std::cell::RefCell::new(Vec::new()),
            column: std::cell::Cell::new(0),
            band: std::cell::Cell::new(23),
            last_size: std::cell::Cell::new((0, 0)),
            picked: std::cell::Cell::new(-1),
            holder,
            layer,
            scrim,
            scrim_alpha,
            block,
            field,
            entry,
            site,
            site_icon,
            site_pill,
            site_box,
            chip_kind: std::cell::Cell::new(None),
            commit,
            list,
            list_scroll,
            rows: std::cell::RefCell::new(Vec::new()),
            visible: std::rc::Rc::new(std::cell::Cell::new(false)),
        }
    }

    /// Where the field stands for a window of this size, and how much room the
    /// list under it has.
    pub fn layout(&self, width: i32, height: i32) {
        self.layout_in(width, height, self.column.get(), self.band.get());
    }

    /// Lay out the scrim and omnibox inside the page area starting at `column` and `band`.
    pub fn layout_in(&self, width: i32, height: i32, column: i32, band: i32) {
        self.last_size.set((width, height));
        self.column.set(column);
        let band = band.max(0);
        self.band.set(band);
        let page = (width - column).max(0);
        self.layer.place(&self.scrim, 0, 0, width, height);
        let field_width = field_width_for(page);
        // Centred on the page, not on the window: the column is not part of
        // the page the field is over.
        let x = column + ((page - field_width) / 2).max(12);
        // The list is capped, not cropped: it scrolls past the cap, and the
        // block is exactly as tall as the list that is really in it. The
        // anchor above it does not depend on any of that.
        let shown = self.list_scroll.get_visible();
        self.list.set_size_request((field_width - 2).max(200), -1);
        let placed = plan(height, if shown { self.list_height_for(field_width) } else { 0 });
        self.block.set_size_request(field_width, placed.block.max(FIELD_HEIGHT + FOOTER_HEIGHT));
        self.layer.place(&self.block, x, placed.top, field_width, placed.block.max(FIELD_HEIGHT + FOOTER_HEIGHT));
        if shown {
            self.list_scroll.set_max_content_width(field_width);
            self.list_scroll.set_size_request(field_width, placed.list);
        }
        self.field.set_size_request(field_width, FIELD_HEIGHT);
    }

    /// The list's own preferred height, not a guess: group headers are shorter
    /// than rows, and the cap has to be a scroll, not a crop.
    fn list_height_for(&self, width: i32) -> i32 {
        self.list.preferred_height_for_width(width).0.max(0)
    }

    /// Snapshot of the live omnibox chip for bench/UI verification.
    pub fn chip_state(&self) -> (bool, String, &'static str) {
        let visible = self.site_box.get_visible();
        let host = if visible { self.site.text().to_string() } else { String::new() };
        let kind = if visible {
            self.chip_kind.get().map(ChipKind::as_str).unwrap_or("none")
        } else {
            "none"
        };
        (visible, host, kind)
    }

    fn apply_site_chip(&self, chip: Option<&ChipModel>) {
        let ctx = self.site_pill.style_context();
        match chip {
            None => {
                self.chip_kind.set(None);
                ctx.remove_class("insecure");
                ctx.remove_class("destination");
                self.site_box.set_no_show_all(true);
                self.site_box.hide();
                self.entry.set_margin_start(4);
            }
            Some(model) => {
                self.chip_kind.set(Some(model.kind));
                for child in self.site_icon.children() {
                    self.site_icon.remove(&child);
                }
                match model.kind {
                    ChipKind::VerifiedHttps => {
                        ctx.remove_class("insecure");
                        ctx.remove_class("destination");
                    }
                    ChipKind::Insecure => {
                        ctx.add_class("insecure");
                        ctx.remove_class("destination");
                    }
                    ChipKind::Destination => {
                        ctx.remove_class("insecure");
                        ctx.add_class("destination");
                    }
                }
                if let Some(app) = crate::app::current() {
                    let palette = *app.palette.borrow();
                    let (icon, color) = match model.kind {
                        ChipKind::VerifiedHttps => (crate::icons::Icon::Lock, palette.accent()),
                        ChipKind::Insecure => (crate::icons::Icon::ShieldAlert, palette.danger),
                        ChipKind::Destination => (crate::icons::Icon::Globe, palette.muted),
                    };
                    let glyph = crate::icons::area(icon, 13, color);
                    glyph.set_valign(gtk::Align::Center);
                    self.site_icon.add(&glyph);
                }
                self.site_box.set_no_show_all(false);
                self.site.set_text(&model.host);
                self.site_box.show_all();
                self.entry.set_margin_start(0);
            }
        }
    }

    pub fn show(&self, ui: &crate::ui::Ui, over: bool) {
        crate::motion::cancel(&self.layer.root);
        crate::motion::cancel(&self.scrim);
        crate::motion::cancel(&self.block);
        crate::motion::cancel(&self.list_scroll);
        self.layer.root.set_opacity(1.0);
        self.scrim.set_opacity(1.0);
        self.scrim_alpha.set(1.0);
        self.block.set_opacity(1.0);
        self.list_scroll.set_opacity(1.0);
        self.visible.set(true);
        // A new field starts on its first row: a pick from the last time is a
        // pick the user did not make, and an unhighlighted first row made the
        // footer's "Enter" promise invisible (Gemini R68 review).
        self.picked.set(-1);
        self.scrim.set_visible(over);
        let column = if ui.side.is_visible() && ui.side.is_child_visible() {
            ui.side.allocated_width().max(0)
        } else {
            0
        };
        self.column.set(column);
        self.band.set(0);
        // The holder is an overlay child, and GTK leaves a hidden overlay child
        // unallocated: it reads 1x1 until the pass after it is shown. Giving it
        // the window's size before anything is placed in it is what lets the
        // first field raised by a keystroke stand where it belongs.
        let (width, height) = (
            ui.overlay.allocated_width().max(ui.window.allocated_width()),
            ui.overlay.allocated_height().max(ui.window.allocated_height()),
        );
        if width > 1 && height > 1 {
            self.holder.set_size_request(width, height);
            let place = gtk::Allocation::new(0, 0, width, height);
            self.holder.size_allocate(&place);
        }
        self.layer.show();
        // The chip only exists when there is something to say about the page:
        // an empty field with a label saying "Search" says it twice.
        let (current, failed, tls) = active_tab_security_context();
        let chip = active_page_chip(&current, failed, tls);
        let host = chip.as_ref().map(|c| c.host.clone()).unwrap_or_default();
        // The chip names where you are and its verified transport state via sitecard::security.
        self.apply_site_chip(chip.as_ref());
        // Typing replaces the address, and the chip is already the site, so
        // the entry shows the part the chip cannot: "127.0.0.1" beside
        // "http://127.0.0.1:8099/office" says the same thing twice.
        self.entry.set_text(&here_for_field(&current, &host));
        // GTK leaves the caret at the start of freshly set text, where it is
        // hidden under the first glyph. The caret goes at the end, so the
        // field shows where the typing will land even before it holds focus.
        self.entry.set_position(-1);
        // The ring is on from the moment the field is up. A window the desktop
        // has not focused, and a capture taken before the entry's idle focus
        // lands, would otherwise show a bare label where the field is.
        crate::ui::class(&self.field, "focused");
        self.entry.grab_focus();
        // The text above is the address's path, not the empty string the
        // caller passed in. Setting it fires `changed`, which refreshes the
        // offers; asking again here makes that the one that sticks even when
        // GTK skips `changed` because the text is identical to last time.
        // Writing the caller's stale `offers("")` back here is what used to
        // leave every same-host tab listed under Open while the field said
        // "/office" (round 67, council/round-67/04-omni.png).
        if let Some(app) = crate::app::current() {
            app.refresh_offers(&self.entry.text());
        }
        self.layout(width, height);
        // The scrim, field and list all arrive on the shared entrance role,
        // so they land on the same frame instead of at 130, 150 and 160ms.
        if over {
            crate::motion::fade_alpha(&self.scrim, &self.scrim_alpha, crate::motion::dur::ENTER);
        }
        if width > 1 && height > 1 {
            let page = (width - column).max(0);
            let field_width = field_width_for(page);
            let x = column + ((page - field_width) / 2).max(12);
            let y = plan(height, 0).top;
            crate::motion::slide_in_fixed(
                &self.layer.root,
                &self.block,
                x,
                y,
                -crate::motion::dur::TRAVEL,
                crate::motion::dur::ENTER,
            );
        }
        if self.list_scroll.get_visible() {
            crate::motion::fade_in(&self.list_scroll, crate::motion::dur::ENTER);
        }
    }

    pub fn hide(&self, _ui: &crate::ui::Ui) {
        let was_visible = self.visible.replace(false);
        // A hidden field holds no focus, and leaving the ring on would paint
        // the next field as already chosen.
        self.field.style_context().remove_class("focused");
        if !was_visible {
            return;
        }
        crate::motion::cancel(&self.layer.root);
        crate::motion::cancel(&self.scrim);
        crate::motion::cancel(&self.block);
        crate::motion::cancel(&self.list_scroll);
        let visible_flag = self.visible.clone();
        let root = self.layer.root.clone();
        // Closing mirrors opening on the shared exit role (130ms, ease_exit).
        crate::motion::fade_out_then(&self.layer.root, crate::motion::dur::EXIT, move || {
            if !visible_flag.get() {
                root.hide();
                root.set_no_show_all(true);
                root.set_opacity(1.0);
            }
        });
    }

    pub fn set_offers(&self, _ui: &crate::ui::Ui, app: &AppRef, offers: Vec<Offer>) {
        *self.offers.borrow_mut() = offers.clone();
        // An open field has a visible first pick unless the list is empty.
        // Previously picked stayed -1 until an arrow key: the first row had no
        // highlight while the footer said Enter would open it.
        let picked = if offers.is_empty() {
            -1
        } else {
            self.picked.get().clamp(0, offers.len() as i32 - 1)
        };
        self.picked.set(picked);
        let verb = if offers.first().is_some_and(|offer| offer.kind == "Open") {
            "Enter  switch"
        } else {
            "Enter  open"
        };
        self.commit.set_text(verb);
        let tint = app.palette.borrow().faint;
        self.rows.borrow_mut().clear();
        for child in self.list.children() {
            self.list.remove(&child);
        }
        if offers.is_empty() {
            self.list.hide();
            self.list_scroll.hide();
            // The block is measured from the list, so a list that just went
            // away leaves the field standing over an empty gap of its height.
            // The anchor does not move; only the room under it changes.
            self.relayout();
            return;
        }
        self.list_scroll.show();
        self.list.show();
        let mut last_kind = String::new();
        for (index, offer) in offers.into_iter().enumerate() {
            if offer.kind != last_kind {
                last_kind = offer.kind.clone();
                let head_text = match last_kind.as_str() {
                    "Suggest" => "Suggestions",
                    other => other,
                };
                let head = gtk::Label::new(Some(head_text));
                crate::ui::class(&head, "sghead");
                head.set_xalign(0.0);
                head.set_margin_start(12);
                head.set_margin_top(6);
                head.set_margin_bottom(2);
                self.list.add(&head);
            }
            let row = gtk::EventBox::new();
            crate::ui::class(&row, "sgrow");
            row.set_can_focus(true);
            let row_label = format!("{} — {}", offer.title, offer.kind);
            row.set_tooltip_text(Some(&row_label));
            if let Some(access) = row.accessible() {
                gtk::atk::prelude::AtkObjectExt::set_name(&access, &row_label);
            }
            if index as i32 == picked {
                crate::ui::class(&row, "picked");
            }
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            hbox.set_margin_start(12);
            hbox.set_margin_end(12);
            hbox.set_margin_top(7);
            hbox.set_margin_bottom(7);

            let mark = build_offer_icon(app, &offer.url, &offer.kind, tint);
            crate::ui::pack(&hbox, &mark, false);

            let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
            texts.set_hexpand(true);
            let title = gtk::Label::new(Some(&offer.title));
            crate::ui::class(&title, "sgtitle");
            title.set_xalign(0.0);
            title.set_width_chars(1);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let url_line = build_offer_url_box(&offer.url);
            texts.add(&title);
            texts.add(&url_line);
            crate::ui::pack(&hbox, &texts, true);

            let kind_hint: Option<String> = match offer.kind.as_str() {
                "Open" => Some(match offer.space_name.as_deref() {
                    Some(space) => format!("Switch · {space}"),
                    None => "Switch to tab".to_string(),
                }),
                "Bookmark" => Some("Bookmark".to_string()),
                _ => None,
            };
            if let Some(hint_text) = kind_hint.as_deref() {
                let hint = gtk::Label::new(Some(hint_text));
                crate::ui::class(&hint, "sgkind");
                hint.set_hexpand(false);
                hint.set_valign(gtk::Align::Center);
                crate::ui::pack(&hbox, &hint, false);
            }

            row.add(&hbox);
            self.rows.borrow_mut().push(row.clone());
            // A press on a row is a pick, and a pick has to leave the field
            // behind: the modifier is read from the press's own state, so a
            // Ctrl click and a Ctrl Return cannot disagree about where the
            // address goes.
            let app_weak = app.weak();
            let target = offer.url.clone();
            let from_key = target.clone();
            let open_row = offer.kind == "Open";
            let target_tab_id = offer.tab_id;
            row.connect_button_press_event(move |_, event| {
                if let Some(app) = app_weak.upgrade() {
                    choose(&app, &target, opens_beside(Some(event.state())), open_row, target_tab_id);
                }
                crate::gtkx::inh(true)
            });
            // A row can hold focus (`set_can_focus(true)` above), so Return on
            // a focused row is the same pick as a press on it. Without this the
            // row takes the keyboard's focus and answers nothing, which leaves
            // the field standing over a list the user cannot choose from. A
            // row is not the entry: other keys, Escape included, are left to
            // the handlers that already know the field, not swallowed here.
            let app_weak = app.weak();
            row.connect_key_press_event(move |_, event| {
                let returns = matches!(
                    event.keyval().name().as_deref(),
                    Some("Return") | Some("KP_Enter") | Some("ISO_Enter")
                );
                if !returns {
                    return crate::gtkx::inh(false);
                }
                if let Some(app) = app_weak.upgrade() {
                    choose(&app, &from_key, opens_beside(Some(event.state())), open_row, target_tab_id);
                }
                crate::gtkx::inh(true)
            });
            self.list.add(&row);
        }
        self.list.show_all();
        // A bench screenshot is composited in the raise turn before GTK's
        // normal size-allocate pass reaches children created since the list
        // was hidden. Give the list a parent-relative allocation; using the
        // viewport's absolute x/y here produces negative child allocations.
        if std::env::var_os("SEARCH_BENCH").is_some() {
            let context = glib::MainContext::default();
            for _ in 0..4 {
                let mut dispatched = 0;
                while dispatched < 64 && context.iteration(false) {
                    dispatched += 1;
                }
                if self.list.allocated_height() > 1 {
                    break;
                }
            }
            let overlay_w = crate::app::current()
                .map(|app| app.ui.overlay.allocated_width().max(app.ui.window.allocated_width()))
                .unwrap_or(FIELD_WIDTH);
            let target_w = field_width_for((overlay_w - self.column.get()).max(0));
            let width = if self.list_scroll.allocated_width() > 1 {
                self.list_scroll.allocated_width().min(target_w).max(200)
            } else {
                target_w.max(200)
            };
            let height = self.list_height_for(width).max(48);
            self.list.size_allocate(&gtk::Allocation::new(0, 0, width, height));
        }
        self.list_scroll.queue_resize();
        self.list.queue_resize();
        // Typing changes how tall the list is, and the cap is a scroll: the
        // scroll has to be given the new height or it keeps the old one.
        self.relayout();
    }

    /// Lay the field out for the window it is actually in. Called when the
    /// list changes size, because a size request made for a taller list
    /// would otherwise leave a shorter one cropped.
    ///
    /// The window's size, not the holder's: the holder is an overlay child,
    /// and GTK leaves a hidden overlay child at 1x1 until the pass after it
    /// is shown. Measuring the holder meant a field raised by a keystroke was
    /// laid out against 1x1 and stood in the corner until something else
    /// forced an allocation.
    fn relayout(&self) {
        let (cached_w, cached_h) = self.last_size.get();
        let (width, height) = if cached_w > 1 && cached_h > 1 {
            (cached_w, cached_h)
        } else if let Some(overlay) = self.holder.parent() {
            (overlay.allocated_width(), overlay.allocated_height())
        } else if let Some(window) = self.holder.toplevel().and_then(|w| w.downcast::<gtk::Window>().ok()) {
            (window.allocated_width(), window.allocated_height())
        } else {
            return;
        };
        if self.visible.get() && width > 1 && height > 1 {
            self.holder.set_size_request(width, height);
            self.layout(width, height);
        }
    }
}

impl Omnibox {
    /// Up and down move through the list; Return takes the picked row.
    pub fn step(&self, delta: i32) {
        // Only the rows in the list count: group headers are rows in the box
        // but not suggestions, so they are skipped when a pick walks.
        let count = self.rows.borrow().len() as i32;
        if count == 0 {
            self.picked.set(-1);
            return;
        }
        let next = (self.picked.get() + delta).clamp(-1, count - 1);
        self.picked.set(next);
        self.mark_picked();
    }

    /// Show which row is picked by restyling it. A pick used to rebuild the
    /// whole list, which left every row needing an allocation and made a
    /// screenshot of a pick document the state before it.
    fn mark_picked(&self) {
        let picked = self.picked.get();
        let verb = if picked >= 0
            && self
                .offers
                .borrow()
                .get(picked as usize)
                .is_some_and(|offer| offer.kind == "Open")
        {
            "Enter  switch"
        } else {
            "Enter  open"
        };
        self.commit.set_text(verb);
        for (index, row) in self.rows.borrow().iter().enumerate() {
            let wanted = index as i32 == picked;
            let has = row.style_context().has_class("picked");
            match (wanted, has) {
                (true, false) => {
                    crate::ui::class(row, "picked");
                }
                (false, true) => row.style_context().remove_class("picked"),
                _ => {}
            }
        }
        if picked >= 0 {
            if let Some(row) = self.rows.borrow().get(picked as usize) {
                let adjust = self.list_scroll.vadjustment();
                if let Some((_, y)) = row.translate_coordinates(&self.list, 0, 0) {
                    let height = row.allocated_height().max(1);
                    let val = adjust.value();
                    let page = adjust.page_size();
                    let wanted = if (y as f64) < val {
                        Some(y as f64)
                    } else if (y + height) as f64 > val + page {
                        Some((y + height) as f64 - page)
                    } else {
                        None
                    };
                    if let Some(wanted) = wanted {
                        adjust.set_value(
                            wanted.clamp(adjust.lower(), (adjust.upper() - page).max(adjust.lower())),
                        );
                    }
                }
            }
        }
    }

    /// Make the chip agree with what is in the field.
    ///
    /// The chip is written once, when the field opens, from the page you are on.
    /// Typing an address for somewhere else left the old site's name sitting over
    /// the new site's address — two different places in one line. This runs on
    /// every keystroke: the chip names the host of whatever the field now resolves
    /// to, and disappears when there is no host to name (a search, or nothing).
    pub fn chip_for_text(&self, typed: &str) {
        let (current, failed, tls) = active_tab_security_context();
        let chip = typed_chip(&current, failed, tls, typed);
        self.apply_site_chip(chip.as_ref());
    }

    pub fn picked_is_open_tab(&self) -> bool {
        let picked = self.picked.get();
        picked >= 0 && self.offers.borrow().get(picked as usize).is_some_and(|offer| offer.kind == "Open")
    }

    pub fn picked_tab_id(&self) -> Option<TabId> {
        let picked = self.picked.get();
        if picked < 0 {
            return None;
        }
        self.offers.borrow().get(picked as usize).and_then(|offer| offer.tab_id)
    }

    pub fn picked_url(&self) -> Option<String> {
        let picked = self.picked.get();
        if picked < 0 {
            return None;
        }
        self.offers.borrow().get(picked as usize).map(|offer| offer.url.clone())
    }

    /// Autocomplete the highlighted (or first) suggestion into the entry so
    /// the user can refine the URL or query with Tab before pressing Enter.
    pub fn complete_picked(&self) -> bool {
        let offers = self.offers.borrow();
        if offers.is_empty() {
            return false;
        }
        let idx = if self.picked.get() >= 0 {
            (self.picked.get() as usize).min(offers.len() - 1)
        } else {
            0
        };
        let offer = &offers[idx];
        let completion = if matches!(offer.kind.as_str(), "Search" | "Suggest") {
            offer
                .title
                .strip_prefix("Search for \"")
                .and_then(|s| s.strip_suffix('"'))
                .unwrap_or(&offer.title)
                .to_string()
        } else {
            offer.url.clone()
        };
        drop(offers);
        self.entry.set_text(&completion);
        self.entry.set_position(-1);
        self.chip_for_text(&completion);
        true
    }
}

/// Reads `(url, failed, tls_flags)` for the active tab.
fn active_tab_security_context() -> (String, bool, Option<gtk::gio::TlsCertificateFlags>) {
    let Some(app) = crate::app::current() else {
        return (String::new(), false, None);
    };
    let active = {
        let state = app.state.borrow();
        state.active_tab().map(|tab| (tab.id, tab.url.clone(), tab.failed))
    };
    let Some((id, url, failed)) = active else {
        return (String::new(), false, None);
    };
    let tls = app.view(id).and_then(|view| view.tls_info()).map(|(_, flags)| flags);
    (url, failed, tls)
}

/// The field shows a path when the chip already names the site, so Return on
/// what the field says has to mean that path on that site. Anything else is
/// passed through untouched, including a search, whose words never start with
/// a slash.
fn whole_address(current: &str, typed: &str) -> String {
    let path = typed.trim();
    if !path.starts_with('/') {
        return typed.to_string();
    }
    let Some((before, authority, _)) = split_url(current) else {
        return typed.to_string();
    };
    format!("{before}{authority}{path}")
}

/// Whether a commit opens beside the page rather than over it. Ctrl is the
/// browser's "not in this tab" modifier, and it is read from the event that
/// asked for the commit: a field that remembered the last modifier held would
/// open a tab nobody asked for on the next plain Return.
fn opens_beside(state: Option<gtk::gdk::ModifierType>) -> bool {
    match state {
        Some(state) => state.contains(gtk::gdk::ModifierType::CONTROL_MASK),
        None => false,
    }
}

/// Take the field's answer: the page beside this one when `beside`, otherwise
/// this tab. Taking it puts the field away, always — a field left standing
/// over the page it just opened holds the next keystroke somewhere the user is
/// not looking, and the page it names is no longer the page under it.
fn choose(app: &App, target: &str, beside: bool, open_row: bool, open_tab_id: Option<TabId>) {
    if beside {
        // A chosen tab closes the field from `select`, so the order here is
        // the tab first: at the tab limit `new_tab` refuses and says so, and a
        // field closed ahead of that refusal would throw away the address it
        // could not open. Nothing else can fail to close it.
        app.new_tab(Some(target), false, false);
        return;
    }
    // A typed or pasted address loads here. Only a row that is an open tab
    // goes to that tab, so picking it does not make a second copy of the page.
    let open = if open_row {
        let state = app.state.borrow();
        let active_private = state.active_tab().is_some_and(|t| t.private);
        if let Some(tid) = open_tab_id {
            // T30: Allow jumping to an exact open tab across spaces while
            // strictly preserving the private/regular boundary.
            state
                .tab(tid)
                .filter(|tab| tab.private == active_private)
                .map(|tab| tab.id)
        } else {
            let active_space = state.active_space_id();
            let tabs: Vec<(TabId, &str, bool)> = state
                .tabs
                .iter()
                .filter(|tab| tab.space_id == active_space)
                .map(|tab| (tab.id, tab.url.as_str(), tab.private))
                .collect();
            same_privacy_tab(&tabs, state.active, target)
        }
    } else {
        None
    };
    if let Some(id) = open {
        app.close_omni();
        app.select(id);
        return;
    }
    app.close_omni();
    app.navigate_active(target);
}

/// The other tab showing `target` that has the same privacy as the active one,
/// so an open-tab row never carries a normal page into a private tab or back.
/// `tabs` holds (id, url, private) for every tab.
fn same_privacy_tab(tabs: &[(TabId, &str, bool)], active: TabId, target: &str) -> Option<TabId> {
    let active_private = tabs.iter().find(|(id, _, _)| *id == active)?.2;
    tabs.iter()
        .find(|(id, url, private)| *id != active && *url == target && *private == active_private)
        .map(|(id, _, _)| *id)
}

/// Return in the field: the picked row if there is one, else the typed words.
pub fn commit(app: &App) {
    if let Some(url) = app.omni.picked_url() {
        let open_row = app.omni.picked_is_open_tab();
        let open_tab_id = app.omni.picked_tab_id();
        choose(app, &url, opens_beside(gtk::current_event_state()), open_row, open_tab_id);
        return;
    }
    let typed = app.omni.entry.text().to_string();
    // A path is a path on the page you are on, not two words to search for.
    let current = app
        .state
        .borrow()
        .active_tab()
        .map(|tab| tab.url.clone())
        .unwrap_or_default();
    let target = crate::app::resolve(app, &whole_address(&current, &typed));
    if target.is_empty() {
        app.close_omni();
        return;
    }
    choose(app, &target, opens_beside(gtk::current_event_state()), false, None);
}

#[cfg(test)]
mod tests {
    use super::*;

    // The field is filled from the address and the chip beside it, so the two
    // have to agree on one thing: the chip names the site, the field never
    // names it again and never names a site it is not on.

    #[test]
    fn an_open_row_never_crosses_into_a_tab_of_the_other_privacy() {
        // Tab 1 is the normal tab in front; tab 2 is a private copy of the same page.
        let tabs = [(1, "https://a.example/", false), (2, "https://a.example/", true), (3, "https://a.example/", false)];
        assert_eq!(same_privacy_tab(&tabs, 1, "https://a.example/"), Some(3));
        assert_eq!(same_privacy_tab(&tabs, 2, "https://a.example/"), None);
        assert_eq!(same_privacy_tab(&tabs[..2], 1, "https://a.example/"), None);
    }

    #[test]
    fn a_bare_host_leaves_the_slash_it_stands_on() {
        assert_eq!(here_for_field("https://example.com", "example.com"), "/");
        assert_eq!(here_for_field("https://example.com/", "example.com"), "/");
    }

    #[test]
    fn the_path_keeps_its_query_and_fragment() {
        assert_eq!(
            here_for_field("https://example.com/a/b?q=1#top", "example.com"),
            "/a/b?q=1#top"
        );
        assert_eq!(
            here_for_field("https://example.com/?q=1", "example.com"),
            "/?q=1"
        );
    }

    #[test]
    fn a_port_is_not_the_site_and_neither_is_a_user() {
        // The chip strips the port, so the authority has to be matched whole.
        assert_eq!(
            here_for_field("http://127.0.0.1:8099/office", "127.0.0.1"),
            "/office"
        );
        assert_eq!(
            here_for_field("https://me@example.com/x", "example.com"),
            "/x"
        );
        assert_eq!(here_for_field("https://EXAMPLE.com/x", "example.com"), "/x");
        // A bracketed address keeps its brackets, so the site is the whole
        // address and not the first colon of it.
        assert_eq!(site_of("[::1]:8099"), "[::1]");
        assert_eq!(chip_host("https://[::1]:8099/x"), "[::1]");
        assert_eq!(here_for_field("https://[::1]:8099/x", "[::1]"), "/x");
        // The chip is a name, so it is lower case whatever the address says.
        assert_eq!(chip_host("https://Example.COM:443/x"), "example.com");
    }

    #[test]
    fn a_url_without_an_authority_is_not_shortened() {
        // `about:blank` used to read as the site "about", which is not a site.
        assert_eq!(here_for_field("about:blank", &chip_host("about:blank")), "about:blank");
        assert_eq!(chip_host("about:blank"), "");
        assert_eq!(chip_host(""), "");
    }

    #[test]
    fn a_file_url_is_never_given_a_host_chip() {
        assert_eq!(chip_host("file:///home/a/notes.html"), "");
        assert_eq!(
            here_for_field("file:///home/a/notes.html", ""),
            "file:///home/a/notes.html"
        );
    }

    #[test]
    fn a_host_that_is_not_the_chip_leaves_the_whole_address() {
        // A chip saying one site over a field holding another is a lie; the
        // full address is at least true.
        assert_eq!(
            here_for_field("https://other.example/x", "example.com"),
            "https://other.example/x"
        );
        assert_eq!(
            here_for_field("https://example.com.au/x", "example.com"),
            "https://example.com.au/x"
        );
    }

    #[test]
    fn an_address_that_is_not_on_the_web_gets_no_chip() {
        // The "://" inside a data: URL is not an authority.
        assert_eq!(chip_host("data:text/html,<a href=\"http://x\">"), "");
        assert_eq!(chip_host("mailto:a@example.com"), "");
        assert_eq!(
            here_for_field("data:text/html,<a href=\"http://x\">", ""),
            "data:text/html,<a href=\"http://x\">"
        );
    }

    #[test]
    fn the_field_stands_in_the_same_place_whatever_the_list_holds() {
        // The anchor is the thing that must not move: suggestions arriving
        // used to push the field up under the caret. The list height is an
        // argument here so the test sees the real input, not a flag.
        for height in [500, 640, 917, 1400] {
            let empty = plan(height, 0);
            for rows in [44, 3 * 44, 8 * 44, 1000] {
                let full = plan(height, rows);
                assert_eq!(empty.top, full.top, "moved at {height} with {rows}");
            }
        }
        // A taller window does move it, or the rule would be "never move".
        assert!(plan(1400, 0).top > plan(500, 0).top);
    }

    #[test]
    fn the_list_is_capped_and_never_takes_the_field_with_it() {
        // A short window still shows the field and rows: the list is given the
        // room that is left, not the height it asked for.
        let short = plan(500, 8 * 44);
        assert!(short.list >= 3 * 44, "only {}px of rows at height 500", short.list);
        assert!(short.top >= TOP_MIN);
        assert!(short.top + FIELD_HEIGHT + FOOTER_HEIGHT + short.list <= 500);

        // Room to spare: the list gets exactly what it asked for, up to the
        // cap, and no more.
        let tall = plan(1400, 8 * 44);
        assert_eq!(tall.list, 8 * 44);
        assert_eq!(plan(1400, 1000).list, LIST_CAP);
        assert!(tall.top >= TOP_MIN);

        // Too short for even one row: the list is empty rather than negative.
        assert_eq!(plan(120, 8 * 44).list, 0);
        assert_eq!(plan(917, -5).list, 0);
    }

    #[test]
    fn a_path_typed_over_the_chip_is_that_path_on_that_site() {
        // The field shows "/office"; Return must not turn that into a search.
        assert_eq!(
            whole_address("http://127.0.0.1:8099/office", "/pricing"),
            "http://127.0.0.1:8099/pricing"
        );
        assert_eq!(
            whole_address("https://example.com/a?q=1", "/b"),
            "https://example.com/b"
        );
        // Everything else is left exactly as typed: a search, an address, a
        // bare host, and a path with no page to belong to.
        assert_eq!(whole_address("https://example.com/x", "two words"), "two words");
        assert_eq!(whole_address("https://example.com/x", "other.com/y"), "other.com/y");
        assert_eq!(whole_address("", "/elsewhere"), "/elsewhere");
        assert_eq!(whole_address("about:blank", "/elsewhere"), "/elsewhere");
    }

    #[test]
    fn a_held_control_is_the_only_thing_that_opens_beside() {
        use gtk::gdk::ModifierType;
        // Ctrl Return is the "not in this tab" Return. A commit with no event
        // beside it — the bench calling `commit` directly, a paste — is a
        // plain Return rather than a guess, and Shift or Alt held with it is
        // still the same tab.
        assert!(opens_beside(Some(ModifierType::CONTROL_MASK)));
        assert!(!opens_beside(Some(ModifierType::SHIFT_MASK)));
        assert!(!opens_beside(Some(ModifierType::MOD1_MASK)));
        assert!(!opens_beside(None));
        // The row press reads the same mask, so a modifier pressed in either
        // order still means "beside".
        assert!(opens_beside(Some(ModifierType::SHIFT_MASK | ModifierType::CONTROL_MASK)));
    }

    #[test]
    fn nothing_in_the_list_leaves_the_block_at_the_field() {
        let none = plan(917, 0);
        assert_eq!(none.block, FIELD_HEIGHT + FOOTER_HEIGHT);
        assert!(plan(917, 352).block > FIELD_HEIGHT);
    }

    #[test]
    fn field_width_clamps_to_narrow_page() {
        assert_eq!(field_width_for(1268), FIELD_WIDTH);
        assert_eq!(field_width_for(400), 376);
        assert_eq!(field_width_for(200), 260);
    }

    #[test]
    fn active_page_chip_requires_verified_tls_and_not_failed_for_lock() {
        use gtk::gio::TlsCertificateFlags as F;
        assert_eq!(
            active_page_chip("https://example.com/a", false, Some(F::empty())),
            Some(ChipModel { host: "example.com".into(), kind: ChipKind::VerifiedHttps })
        );
        // Failed HTTPS page must never claim a verified lock even if TLS flags were empty or absent.
        assert_eq!(
            active_page_chip("https://expired.badssl.com/account", true, None),
            Some(ChipModel { host: "expired.badssl.com".into(), kind: ChipKind::Insecure })
        );
        assert_eq!(
            active_page_chip("https://expired.badssl.com/account", true, Some(F::empty())),
            Some(ChipModel { host: "expired.badssl.com".into(), kind: ChipKind::Insecure })
        );
        // Expired certificate without failed flag is still insecure.
        assert_eq!(
            active_page_chip("https://expired.badssl.com/", false, Some(F::EXPIRED)),
            Some(ChipModel { host: "expired.badssl.com".into(), kind: ChipKind::Insecure })
        );
        // Missing TLS info on HTTPS is Unknown -> insecure, never Lock.
        assert_eq!(
            active_page_chip("https://example.com/", false, None),
            Some(ChipModel { host: "example.com".into(), kind: ChipKind::Insecure })
        );
        // Plain HTTP is insecure.
        assert_eq!(
            active_page_chip("http://127.0.0.1:8099/office", false, None),
            Some(ChipModel { host: "127.0.0.1".into(), kind: ChipKind::Insecure })
        );
        // Non-web pages get no chip.
        assert_eq!(active_page_chip("", false, None), None);
        assert_eq!(active_page_chip("about:blank", false, None), None);
        assert_eq!(active_page_chip("file:///tmp/x.html", false, None), None);
    }

    #[test]
    fn typed_chip_hides_on_plain_searches_and_never_locks_unvisited_hosts() {
        use gtk::gio::TlsCertificateFlags as F;
        let current = "http://127.0.0.1:8099/office";
        // Plain searches and open-tab keywords must NOT display duckduckgo.com or any lock!
        assert_eq!(typed_chip(current, false, None, "office"), None);
        assert_eq!(typed_chip(current, false, None, "rust browser"), None);
        assert_eq!(typed_chip(current, false, None, "yt cats"), None);
        assert_eq!(typed_chip(current, false, None, ""), None);
        assert_eq!(typed_chip(current, false, None, "   "), None);

        // Current-host path inherits active page security state.
        assert_eq!(
            typed_chip(current, false, None, "/pricing"),
            Some(ChipModel { host: "127.0.0.1".into(), kind: ChipKind::Insecure })
        );
        assert_eq!(
            typed_chip("https://example.com/a", false, Some(F::empty()), "/b"),
            Some(ChipModel { host: "example.com".into(), kind: ChipKind::VerifiedHttps })
        );
        assert_eq!(
            typed_chip("https://expired.badssl.com/a", true, Some(F::EXPIRED), "/b"),
            Some(ChipModel { host: "expired.badssl.com".into(), kind: ChipKind::Insecure })
        );

        // Explicit unvisited destinations show neutral Destination intent, NEVER VerifiedHttps Lock.
        assert_eq!(
            typed_chip(current, false, None, "example.com"),
            Some(ChipModel { host: "example.com".into(), kind: ChipKind::Destination })
        );
        assert_eq!(
            typed_chip(current, false, None, "https://example.com/docs"),
            Some(ChipModel { host: "example.com".into(), kind: ChipKind::Destination })
        );
        // Explicit plain HTTP destination is Insecure.
        assert_eq!(
            typed_chip("https://example.com/a", false, Some(F::empty()), "http://other.test/x"),
            Some(ChipModel { host: "other.test".into(), kind: ChipKind::Insecure })
        );
    }

    #[test]
    fn explicit_destination_restricts_to_http_and_https_and_matches_resolver() {
        use gtk::gio::TlsCertificateFlags as F;
        let current = "https://example.com/a";
        let engine = |q: &str| crate::store::Prefs::default().search_url(q);

        // Non-HTTP(S) schemes must never produce a destination chip.
        for non_http in [
            "ftp://example.com/",
            "FTP://example.com/file",
            "javascript://example.com/%0aalert(1)",
            "JAVASCRIPT://example.com/",
            "javascript:alert(document.domain)",
            "data://example.com/x",
            "data:text/html,example.com",
            "blob://example.com/uuid",
            "mailto:user@example.com",
            "chrome://settings",
            "custom+proto://host.test/path",
            "file:///tmp/index.html",
            "FILE:///tmp/index.html",
            "about:blank",
            "ABOUT:blank",
            "http://",
            "https://",
        ] {
            assert_eq!(
                explicit_destination(non_http),
                None,
                "{non_http:?} must not produce an explicit HTTP(S) destination"
            );
            assert_eq!(
                typed_chip(current, false, Some(F::empty()), non_http),
                None,
                "{non_http:?} must not render a chip"
            );
        }

        // Disallowed :// schemes also remain refused by the app resolver -> scheme_is_allowed path.
        for disallowed in [
            "ftp://example.com/",
            "FTP://example.com/file",
            "javascript://example.com/%0aalert(1)",
            "JAVASCRIPT://example.com/",
            "chrome://settings",
            "custom+proto://host.test/path",
        ] {
            let resolved = crate::app::plain_target(disallowed, &engine);
            assert!(
                !crate::web::scheme_is_allowed(&resolved, true),
                "resolver output {resolved:?} for {disallowed:?} must be refused by scheme_is_allowed"
            );
        }

        // Bare host:port and paths with colons still resolve as HTTPS destinations.
        assert_eq!(
            explicit_destination("localhost:8080/a"),
            Some("https://localhost:8080/a".to_string())
        );
        assert_eq!(
            explicit_destination("example.com/wiki/Category:Rust"),
            Some("https://example.com/wiki/Category:Rust".to_string())
        );

        // Uppercase and mixed-case canonical HTTP(S) schemes remain supported and
        // preserve verified/destination/insecure chip classification.
        assert_eq!(
            typed_chip(current, false, Some(F::empty()), "HTTPS://Example.COM/docs"),
            Some(ChipModel { host: "example.com".into(), kind: ChipKind::VerifiedHttps })
        );
        assert_eq!(
            typed_chip(current, false, Some(F::empty()), "HTTPS://Other.Test/docs"),
            Some(ChipModel { host: "other.test".into(), kind: ChipKind::Destination })
        );
        assert_eq!(
            typed_chip(current, false, Some(F::empty()), "HTTP://Example.COM/a"),
            Some(ChipModel { host: "example.com".into(), kind: ChipKind::Insecure })
        );
        assert_eq!(
            typed_chip(current, false, Some(F::empty()), "Http://[::1]:8080/p"),
            Some(ChipModel { host: "[::1]".into(), kind: ChipKind::Insecure })
        );
        assert_eq!(
            typed_chip(current, false, Some(F::empty()), "HtTpS://[2001:DB8::1]/p"),
            Some(ChipModel { host: "[2001:db8::1]".into(), kind: ChipKind::Destination })
        );

        for allowed in [
            "HTTPS://Example.COM/docs",
            "HTTP://Example.COM/a",
            "https://other.test/x",
            "http://[::1]:8080/p",
            "example.com/path",
            "localhost:8099",
        ] {
            let resolved = crate::app::plain_target(allowed, &engine);
            assert!(
                crate::web::scheme_is_allowed(&resolved, true),
                "supported input {allowed:?} -> {resolved:?} must remain allowed"
            );
            assert!(
                typed_chip(current, false, Some(F::empty()), allowed).is_some(),
                "supported input {allowed:?} must still produce a chip"
            );
        }
    }

    #[test]
    fn host_and_path_splits_https_http_and_root_cleanly() {
        assert_eq!(
            host_and_path("https://en.wikipedia.org/wiki/Linux_kernel"),
            ("en.wikipedia.org".into(), "/wiki/Linux_kernel".into())
        );
        assert_eq!(
            host_and_path("https://example.com/"),
            ("example.com".into(), "".into())
        );
        assert_eq!(
            host_and_path("http://127.0.0.1:8080/plain"),
            ("http://127.0.0.1:8080".into(), "/plain".into())
        );
        assert_eq!(
            host_and_path("https://user:pass@example.org/docs"),
            ("example.org".into(), "/docs".into())
        );
    }
}
