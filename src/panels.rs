// The four panels that hold everything else: what you kept, where you went,
// what you brought down, and how the browser is set up.
use crate::app::AppRef;
use crate::palette::Palette;
use crate::store::{ago, day_of, now_secs, Records};
use gtk::prelude::*;

const INSET: i32 = 20;
const COMPACT_INSET: i32 = 16;
const RAIL_WIDTH: i32 = 164;
const MAX_HEIGHT: i32 = 720;

fn widget_insets(widget: &impl IsA<gtk::Widget>) -> (i32, i32, i32, i32) {
    let context = widget.style_context();
    let state = widget.state_flags();
    let padding = context.padding(state);
    let border = context.border(state);
    let margin = context.margin(state);
    (
        widget.margin_start() + i32::from(padding.left) + i32::from(border.left) + i32::from(margin.left),
        widget.margin_end() + i32::from(padding.right) + i32::from(border.right) + i32::from(margin.right),
        widget.margin_top() + i32::from(padding.top) + i32::from(border.top) + i32::from(margin.top),
        widget.margin_bottom() + i32::from(padding.bottom) + i32::from(border.bottom) + i32::from(margin.bottom),
    )
}

/// Chooses fast word-boundary wrapping for ordinary prose labels so GTK3
/// never lays out every single character onto its own line at `width = 0`
/// during size negotiation. Labels containing long unbroken tokens (such as
/// URLs or long hostnames) retain `WordChar` with a minimum character floor so
/// narrow windows still wrap cleanly without triggering the `width = 0` storm.
fn wrap_policy(text: &str) -> (gtk::pango::WrapMode, Option<i32>) {
    if text.split_whitespace().any(|word| word.chars().count() > 24) {
        (gtk::pango::WrapMode::WordChar, Some(12))
    } else {
        (gtk::pango::WrapMode::Word, None)
    }
}

fn configure_label_wrap(label: &gtk::Label) {
    let text = label.text();
    let (mode, min_chars) = wrap_policy(&text);
    if !label.wraps() {
        label.set_line_wrap(true);
    }
    if label.line_wrap_mode() != mode {
        label.set_line_wrap_mode(mode);
    }
    if let Some(chars) = min_chars {
        if label.width_chars() < chars {
            label.set_width_chars(chars);
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PanelKind {
    Bookmarks,
    History,
    Downloads,
    Extensions,
    Settings,
    Shortcuts,
}

impl PanelKind {
    fn title(&self) -> &'static str {
        match self {
            PanelKind::Bookmarks => "Bookmarks",
            PanelKind::History => "History",
            PanelKind::Downloads => "Downloads",
            PanelKind::Extensions => "Extensions",
            PanelKind::Settings => "Settings",
            PanelKind::Shortcuts => "Shortcuts",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            PanelKind::Bookmarks => "The pages you kept. All in one place.",
            PanelKind::History => "Find a page you visited. Pick up where you left off.",
            PanelKind::Downloads => "Follow the transfer. Find the file.",
            PanelKind::Extensions => "See what is installed, and what can actually run.",
            PanelKind::Settings => "Appearance, browsing, and the choices you control.",
            PanelKind::Shortcuts => "The keyboard guide. Every shortcut the browser supports.",
        }
    }

    fn icon(&self) -> crate::icons::Icon {
        match self {
            PanelKind::Bookmarks => crate::icons::Icon::Star,
            PanelKind::History => crate::icons::Icon::Clock,
            PanelKind::Downloads => crate::icons::Icon::Download,
            PanelKind::Extensions => crate::icons::Icon::Spark,
            PanelKind::Settings => crate::icons::Icon::Settings,
            PanelKind::Shortcuts => crate::icons::Icon::Command,
        }
    }
    pub fn all() -> [PanelKind; 6] {
        [
            PanelKind::Bookmarks,
            PanelKind::History,
            PanelKind::Downloads,
            PanelKind::Extensions,
            PanelKind::Settings,
            PanelKind::Shortcuts,
        ]
    }
}

thread_local! {
    static EXTENSION_CHOOSERS: std::cell::RefCell<Vec<gtk::FileChooserNative>> =
        const { std::cell::RefCell::new(Vec::new()) };
    static PROFILE_CHOOSERS: std::cell::RefCell<Vec<gtk::FileChooserNative>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Card width clamped to the page viewport width so narrow windows never overflow.
pub fn panel_card_width(page_width: i32) -> i32 {
    880.min(page_width.saturating_sub(24).max(1))
}

/// Map a shortcut description from `crate::keys::SHEET` to its section heading.
pub fn shortcut_section(what: &str) -> &'static str {
    match what {
        "New tab"
        | "Private tab"
        | "Close tab"
        | "Reopen the last closed tab"
        | "Next tab"
        | "Previous tab"
        | "Jump to a tab"
        | "Switch to a tab by typing its name"
        | "Duplicate this tab"
        | "Move tab left"
        | "Move tab right"
        | "Show or hide the tab sidebar"
        | "Fold the tab column to a strip, or open it" => "Tabs",
        "Back"
        | "Forward"
        | "Reload"
        | "Reload past the cache"
        | "Reading mode (again to leave)"
        | "Keep this page"
        | "Find in the page"
        | "Next match"
        | "Previous match"
        | "Copy the address"
        | "The inspector"
        | "About this site: connection, permissions, zoom, data"
        | "The page's own source"
        | "Print this page"
        | "Full screen"
        | "Hide something on this site"
        | "Show what is hidden here"
        | "Zoom in"
        | "Zoom out"
        | "Zoom reset" => "Page",
        "History"
        | "Downloads"
        | "Bookmarks"
        | "Settings"
        | "This list"
        | "The field"
        | "Close the field, find, a panel or reader, else stop the page"
        | "Previous panel"
        | "Next panel"
        | "Focus the panel" => "Browser",
        _ => "Browser",
    }
}

pub struct Panels {
    pub wanted_height: std::cell::Cell<i32>,

    column: std::cell::Cell<i32>,
    band: std::cell::Cell<i32>,
    pub filter: std::cell::RefCell<String>,
    pub holder: gtk::Fixed,
    layer: crate::layer::Layer,
    pub scrim: gtk::DrawingArea,
    scrim_alpha: std::rc::Rc<std::cell::Cell<f64>>,
    pub card: gtk::Box,
    pub title: gtk::Label,
    pub nav: gtk::Box,
    pub body: gtk::Box,
    pub scroll: gtk::ScrolledWindow,
    pub close_button: gtk::Button,
    pub open: Option<PanelKind>,
    rail: gtk::Box,
    rail_brand: gtk::Box,
    rail_hint: gtk::Box,
    content: gtk::Box,
    header: gtk::Box,
    description: gtk::Label,
    compact: std::cell::Cell<bool>,
    refit_queued: std::rc::Rc<std::cell::Cell<bool>>,
    /// True while a freshly opened panel is awaiting or running its entrance
    /// animation so immediate/idle layout calls cannot snap its animated `y`.
    entering: std::rc::Rc<std::cell::Cell<bool>>,
    /// Incremented on every open/close so deferred entrance callbacks from a
    /// superseded open become no-ops.
    open_gen: std::rc::Rc<std::cell::Cell<u64>>,
    /// The card width the body was last measured at. A resize changes the width
    /// a wrapping line needs, so the height has to be asked again.
    fitted_width: std::cell::Cell<i32>,
    /// Last content width adapted via `adapt_content`, reset on `clear_body`.
    adapted_width: std::cell::Cell<i32>,
    /// Last compact layout mode applied to chrome/nav, so repeat `layout_card`
    /// calls do not mutate unchanged GTK properties and dirty Pango caches.
    configured_compact: std::cell::Cell<Option<bool>>,
    /// Persistent rail navigation buttons, rebuilt only when theme polarity changes.
    nav_buttons: std::cell::RefCell<Vec<(PanelKind, gtk::ToggleButton)>>,
    nav_dark: std::cell::Cell<Option<bool>>,
    updating_nav: std::rc::Rc<std::cell::Cell<bool>>,
    /// A permanent home for the Settings body. Other panels clear `body` on
    /// open; keeping Settings here avoids jumping the scroller while typing.
    pub(crate) settings_holder: gtk::Box,
    /// Open popover owned by a panel control (e.g., the Settings search engine
    /// picker) so Escape dismisses it before closing the panel and Tab stays
    /// inside its menu instead of jumping to the panel card.
    active_popover: std::cell::RefCell<Option<gtk::Popover>>,
}

/// Recursive lookup for a filter `gtk::Entry` inside a container (supports both
/// direct entries and entries wrapped inside `.panel-filter-box`).
fn find_entry(widget: &gtk::Widget) -> Option<gtk::Entry> {
    if let Ok(entry) = widget.clone().downcast::<gtk::Entry>() {
        return Some(entry);
    }
    if let Ok(container) = widget.clone().downcast::<gtk::Container>() {
        for child in container.children() {
            if let Some(entry) = find_entry(&child) {
                return Some(entry);
            }
        }
    }
    None
}

fn populate_catalog_rows(
    app: &AppRef,
    container: &gtk::Box,
    entries: &[crate::addons_catalog::CatalogEntry],
) {
    for child in container.children() {
        container.remove(&child);
    }
    if entries.is_empty() {
        let empty = gtk::Label::new(Some("No matching add-ons."));
        crate::ui::class(&empty, "settingdesc");
        empty.set_xalign(0.0);
        empty.set_margin_top(6);
        empty.set_margin_bottom(6);
        container.add(&empty);
        container.show_all();
        return;
    }
    for entry in entries {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        crate::ui::class(&row, "ptrow");
        crate::ui::class(&row, "panel-list-row");

        let copy = gtk::Box::new(gtk::Orientation::Vertical, 4);
        copy.set_hexpand(true);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some(&entry.name));
        crate::ui::class(&title, "ptitle");
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let badge = gtk::Label::new(Some(entry.compat.badge()));
        crate::ui::class(&badge, "compat-badge");
        match entry.compat {
            crate::addons_catalog::CompatTier::Unverified => crate::ui::class(&badge, "content"),
            crate::addons_catalog::CompatTier::Partial => crate::ui::class(&badge, "partial"),
            crate::addons_catalog::CompatTier::Unsupported => crate::ui::class(&badge, "unsupported"),
        };
        badge.set_valign(gtk::Align::Center);
        head.pack_start(&title, false, false, 0);
        head.pack_start(&badge, false, false, 0);
        copy.add(&head);

        let desc_text = if entry.summary.is_empty() {
            entry.compat_note.clone()
        } else {
            format!("{} {}", entry.summary, entry.compat_note)
        };
        let desc = gtk::Label::new(Some(&desc_text));
        crate::ui::class(&desc, "purl");
        desc.set_xalign(0.0);
        desc.set_line_wrap(true);
        desc.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        copy.add(&desc);
        row.pack_start(&copy, true, true, 0);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions.set_valign(gtk::Align::Center);
        if let Some(xpi_url) = entry.xpi_url.clone() {
            let install_btn = gtk::Button::with_label("Install\u{2026}");
            crate::ui::class(&install_btn, "act");
            if entry.compat != crate::addons_catalog::CompatTier::Unsupported {
                crate::ui::class(&install_btn, "primary");
            }
            let app_weak = app.weak();
            let slug = entry.slug.clone();
            let listing = entry.listing_url.clone();
            install_btn.connect_clicked(move |btn| {
                btn.set_sensitive(false);
                btn.set_label("Fetching\u{2026}");
                let app_done = app_weak.clone();
                let btn_done = btn.downgrade();
                let came_from = listing.clone();
                crate::addons_catalog::download_amo_xpi(&slug, &xpi_url, move |res| {
                    if let Some(b) = btn_done.upgrade() {
                        b.set_sensitive(true);
                        b.set_label("Install\u{2026}");
                    }
                    let Some(app) = app_done.upgrade() else { return };
                    match res {
                        Ok(path) => match crate::extensions::inspect(&path) {
                            Ok(offer) => {
                                app.close_panels();
                                crate::permissions::offer_addon(&app, app.active_id(), path, came_from, offer);
                            }
                            Err(why) => app.flash_toast(&format!("Could not inspect add-on: {why}")),
                        },
                        Err(why) => app.flash_toast(&why),
                    }
                });
            });
            actions.pack_start(&install_btn, false, false, 0);
        }
        let open_btn = gtk::Button::with_label("Open on AMO");
        crate::ui::class(&open_btn, "act");
        let app_weak = app.weak();
        let listing_url = entry.listing_url.clone();
        open_btn.connect_clicked(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.close_panels();
                app.new_tab(Some(&listing_url), false, true);
            }
        });
        actions.pack_start(&open_btn, false, false, 0);
        row.pack_end(&actions, false, false, 0);
        container.add(&row);
    }
    container.show_all();
}

/// Pure switch state `(active, sensitive)` for an installed extension row.
/// An extension that cannot run (`runs == false`) must never appear active or
/// toggleable.
fn extension_switch_state(runs: bool, enabled: bool) -> (bool, bool) {
    if runs {
        (enabled, true)
    } else {
        (false, false)
    }
}

/// The Settings query hides widgets in place. Rebuilding the panel on each
/// keystroke destroyed the entry while its changed callback was still running.
/// Settings owns a search entry as the first child of its list. Filtering
/// must leave that entry visible and keep a section header with any row that
/// matches, or the result reads as an orphaned subtitle.
fn is_section_chrome(widget: &gtk::Widget) -> bool {
    let style = widget.style_context();
    style.has_class("panel-section-header")
        || style.has_class("group")
        || style.has_class("panel-section-description")
}

fn filter_settings_list(list: &gtk::Box, query: &str) {
    for section in list.children() {
        section.show();
        let children = child_widgets(&section);
        // If the section title itself matches, keep the whole section so
        // searching "Appearance" or "Browsing" reveals its controls. Otherwise
        // require an actual control row to match — never an orphaned subtitle.
        let title_matches = children.iter().any(|child| {
            let style = child.style_context();
            (style.has_class("panel-section-header") || style.has_class("group"))
                && widget_text(child).to_lowercase().contains(query)
        });
        let row_matches = children.iter().any(|child| {
            !is_section_chrome(child) && widget_text(child).to_lowercase().contains(query)
        });
        if !title_matches && !row_matches {
            section.hide();
            continue;
        }
        for child in children {
            let visible = title_matches
                || is_section_chrome(&child)
                || widget_text(&child).to_lowercase().contains(query);
            child.set_visible(visible);
        }
    }
}

fn child_widgets(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    widget
        .clone()
        .downcast::<gtk::Container>()
        .map(|container| container.children())
        .unwrap_or_default()
}

fn widget_text(widget: &gtk::Widget) -> String {
    use gtk::prelude::*;
    fn text(value: glib::GString) -> String {
        value.as_str().to_owned()
    }
    if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
        return text(label.text());
    }
    if let Ok(button) = widget.clone().downcast::<gtk::Button>() {
        // Custom-child buttons (the Light/Dark/System theme cards) have no
        // `label`. Falling through to Container traversal lets their words
        // take part in filtering instead of orphaning the section heading.
        let label = text(button.label().unwrap_or_default());
        if !label.is_empty() {
            return label;
        }
    }
    if let Ok(entry) = widget.clone().downcast::<gtk::Entry>() {
        return text(entry.text());
    }
    if let Ok(combo) = widget.clone().downcast::<gtk::ComboBoxText>() {
        return text(combo.active_text().unwrap_or_default());
    }
    if let Ok(container) = widget.clone().downcast::<gtk::Container>() {
        let mut parts = Vec::new();
        container.foreach(|child| parts.push(widget_text(child.upcast_ref::<gtk::Widget>())));
        return parts.join(" ");
    }
    String::new()
}

impl Panels {
    pub fn new(palette: &Palette) -> Panels {
        let layer = crate::layer::Layer::new();
        let holder = layer.root.clone();

        let (scrim, scrim_alpha) = crate::ui::scrim_with_alpha(palette);

        let card = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        crate::ui::class(&card, "panel");
        crate::ui::class(&card, "panel-workspace");
        card.set_size_request(880, 420);
        if let Some(access) = card.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Dialog);
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Browser panels");
        }

        let rail = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&rail, "panel-rail");
        rail.set_hexpand(false);
        rail.set_size_request(RAIL_WIDTH, -1);
        let rail_brand = gtk::Box::new(gtk::Orientation::Vertical, 4);
        rail_brand.set_margin_start(16);
        rail_brand.set_margin_end(12);
        rail_brand.set_margin_top(20);
        rail_brand.set_margin_bottom(16);
        let wordmark = gtk::Label::new(Some(crate::app::APP_NAME));
        crate::ui::class(&wordmark, "panel-rail-brand");
        wordmark.set_xalign(0.0);
        rail_brand.pack_start(&wordmark, false, true, 0);
        rail.pack_start(&rail_brand, false, true, 0);

        let nav = gtk::Box::new(gtk::Orientation::Vertical, 4);
        nav.set_margin_start(8);
        nav.set_margin_end(8);
        nav.set_margin_bottom(8);
        if let Some(access) = nav.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::PageTabList);
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Browser panel navigation");
        }
        rail.pack_start(&nav, false, true, 0);

        let rail_hint = gtk::Box::new(gtk::Orientation::Vertical, 4);
        crate::ui::class(&rail_hint, "panel-rail-hint");
        rail_hint.set_margin_start(16);
        rail_hint.set_margin_end(12);
        rail_hint.set_margin_top(12);
        rail_hint.set_margin_bottom(16);
        let hint_keys = gtk::Label::new(Some("Ctrl ← / →"));
        crate::ui::class(&hint_keys, "keyhint");
        hint_keys.set_halign(gtk::Align::Start);
        let hint_text = gtk::Label::new(Some("Move between panels"));
        crate::ui::class(&hint_text, "settingdesc");
        hint_text.set_xalign(0.0);
        hint_text.set_line_wrap(true);
        hint_text.set_width_chars(1);
        hint_text.set_max_width_chars(18);
        rail_hint.pack_start(&hint_keys, false, true, 0);
        rail_hint.pack_start(&hint_text, false, true, 0);
        rail.pack_end(&rail_hint, false, true, 0);
        card.pack_start(&rail, false, true, 0);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&content, "panel-content");
        content.set_hexpand(true);
        content.set_vexpand(true);
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        crate::ui::class(&head, "panel-header");
        head.set_margin_start(INSET);
        head.set_margin_end(INSET);
        head.set_margin_top(20);
        head.set_margin_bottom(12);
        let heading = gtk::Box::new(gtk::Orientation::Vertical, 4);
        heading.set_hexpand(true);
        let title = gtk::Label::new(Some("Bookmarks"));
        crate::ui::class(&title, "panel-heading");
        title.set_xalign(0.0);
        title.set_line_wrap(true);
        title.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        if let Some(access) = title.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
        }
        let description = gtk::Label::new(Some(PanelKind::Bookmarks.description()));
        crate::ui::class(&description, "panel-description");
        description.set_xalign(0.0);
        description.set_line_wrap(true);
        description.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        heading.pack_start(&title, false, true, 0);
        heading.pack_start(&description, false, true, 0);
        head.pack_start(&heading, true, true, 0);
        let close = crate::icons::button(crate::icons::Icon::Close, 13, palette.muted, "door");
        close.set_can_focus(true);
        close.set_focus_on_click(false);
        close.set_valign(gtk::Align::Start);
        close.set_tooltip_text(Some("Close panel · Esc"));
        if let Some(access) = close.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Close panel");
        }
        crate::ui::class(&close, "panelclose");
        head.pack_start(&close, false, false, 0);
        content.pack_start(&head, false, true, 0);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let settings_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(false);
        scroll.set_min_content_height(1);
        scroll.set_max_content_height(MAX_HEIGHT);
        scroll.set_vexpand(true);
        scroll.set_overlay_scrolling(false);
        // 20px of fixed space under the scroller, outside the scrolling body.
        // A capped panel snaps to a whole row, so without this the last row
        // (the Extensions chooser in round 54) sat right on the card's bottom
        // edge. Round 76 raised this from 12px after the critic called the
        // last Settings and Extensions rows crowded. chrome_height still
        // counts the scroller's own margins, so snap_to_row ends on a whole
        // row. The body's bottom margin still scrolls with the rows and only
        // shows at the end of the list.
        scroll.set_margin_bottom(20);
        scroll.add(&body);
        // The scroller clips to its allocation; a box does not. Let the
        // viewport own scrolling and let this wrapper shrink with the card.
        body.set_hexpand(true);
        body.set_margin_start(INSET);
        body.set_margin_end(INSET);
        body.set_margin_bottom(INSET);
        body.set_vexpand(true);
        // The ScrolledWindow owns clipping. Giving every direct child the
        // entire body height blanks filter fields and leaves rows half-clipped.
        content.pack_start(&scroll, true, true, 0);
        card.pack_start(&content, true, true, 0);
        // Panel arrow keys and focus go through keys::apply, like every other
        // browser action. No second event router belongs on this card.

        layer.add(&scrim);
        layer.add(&card);

        let panels = Panels {
            wanted_height: std::cell::Cell::new(420),
            column: std::cell::Cell::new(0),
            band: std::cell::Cell::new(23),
            filter: std::cell::RefCell::new(String::new()),
            holder,
            layer,
            scrim,
            scrim_alpha,
            card,
            title,
            nav,
            body,
            scroll,
            close_button: close.clone(),
            open: None,
            rail,
            rail_brand,
            rail_hint,
            content,
            header: head,
            description,
            compact: std::cell::Cell::new(false),
            refit_queued: std::rc::Rc::new(std::cell::Cell::new(false)),
            entering: std::rc::Rc::new(std::cell::Cell::new(false)),
            open_gen: std::rc::Rc::new(std::cell::Cell::new(0)),
            fitted_width: std::cell::Cell::new(0),
            adapted_width: std::cell::Cell::new(0),
            configured_compact: std::cell::Cell::new(None),
            nav_buttons: std::cell::RefCell::new(Vec::new()),
            nav_dark: std::cell::Cell::new(None),
            updating_nav: std::rc::Rc::new(std::cell::Cell::new(false)),
            settings_holder,
            active_popover: std::cell::RefCell::new(None),
        };

        close.connect_clicked(move |_| {
            if let Some(app) = crate::app::current() {
                app.close_panels();
            }
        });
        {
            let scrim = panels.scrim.clone();
            scrim.connect_button_press_event(move |_, _| {
                if let Some(app) = crate::app::current() {
                    app.close_panels();
                }
                crate::gtkx::inh(true)
            });
        }
        panels.body.add(&panels.settings_holder);
        panels
    }

    /// A filter typed into Bookmarks, History or Downloads rebuilds that list.
    /// Settings hides rows in place: rebuilding its form destroys the entry
    /// while its changed signal is still running.
    pub fn set_filter(&mut self, app: &AppRef, kind: PanelKind, text: &str) {
        if kind == PanelKind::Settings {
            let query = text.to_lowercase();
            *self.filter.borrow_mut() = query.clone();
            let children = self.settings_holder.children();
            let entry = children.iter().find_map(find_entry);
            let list = children
                .iter()
                .find_map(|child| child.clone().downcast::<gtk::ScrolledWindow>().ok())
                .and_then(|scroll| scroll.child())
                .and_then(|viewport| viewport.downcast::<gtk::Viewport>().ok())
                .and_then(|viewport| viewport.child())
                .and_then(|child| child.downcast::<gtk::Box>().ok());
            if let Some(entry) = entry {
                if entry.text() != text {
                    entry.set_text(text);
                }
                entry.set_position(text.chars().count() as i32);
                entry.grab_focus();
            }
            if let Some(list) = list {
                // show_all restores rows hidden by the previous query,
                // then the new query is applied once. The field lives
                // outside this list and is never affected by either.
                list.show_all();
                if query.is_empty() {
                    for section in list.children() {
                        for child in child_widgets(&section) {
                            child.show();
                        }
                        section.show();
                    }
                } else {
                    filter_settings_list(&list, &query);
                }
                list.queue_resize();
                let scroll = self.scroll_for_open();
                scroll.queue_resize();
                scroll.check_resize();
                self.fitted_width.set(0);
                self.queue_refit();
            }
            return;
        }
        *self.filter.borrow_mut() = text.to_lowercase();
        let caret = text.chars().count() as i32;
        let entry = self.open_panel_with(app, kind);
        if let Some(entry) = entry {
            entry.set_text(text);
            entry.grab_focus();
            entry.set_position(caret);
        }
    }

    pub fn has_open_popover(&self) -> bool {
        self.active_popover
            .borrow()
            .as_ref()
            .is_some_and(|pop| pop.is_visible())
    }

    pub fn dismiss_popover(&self) -> bool {
        let pop = self.active_popover.borrow().clone();
        if let Some(pop) = pop {
            if pop.is_visible() {
                pop.popdown();
                if let Some(rel) = pop.relative_to() {
                    if rel.is_mapped() {
                        rel.grab_focus();
                    }
                }
                return true;
            }
        }
        false
    }

    fn open_panel_with(&mut self, app: &AppRef, kind: PanelKind) -> Option<gtk::Entry> {
        self.dismiss_popover();
        let prev = self.open.clone();
        let was_open = prev.is_some();
        let switched = prev.as_ref().is_some_and(|p| *p != kind);
        let gen = self.open_gen.get().wrapping_add(1);
        self.open_gen.set(gen);
        self.entering.set(false);
        crate::motion::cancel(&self.layer.root);
        crate::motion::cancel(&self.scrim);
        crate::motion::cancel(&self.card);
        crate::motion::cancel(&self.scroll);
        self.layer.root.set_opacity(1.0);
        self.scrim.set_opacity(1.0);
        self.scrim_alpha.set(1.0);
        self.card.set_opacity(1.0);
        self.scroll.set_opacity(1.0);

        self.open = Some(kind.clone());
        self.title.set_text(kind.title());
        self.description.set_text(kind.description());
        if let Some(access) = self.card.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, kind.title());
        }
        self.build_nav(app, &kind);
        // Settings owns its own inner scroller below the pinned search field;
        // turning off the outer scroller prevents a double scrollbar at narrow
        // widths while keeping every other panel on the shared scroller.
        if kind == PanelKind::Settings {
            self.scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Never);
        } else {
            self.scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        }
        let field = match kind {
            PanelKind::Bookmarks => self.build_bookmarks(app),
            PanelKind::History => self.build_history(app),
            PanelKind::Downloads => self.build_downloads(app),
            PanelKind::Extensions => {
                self.build_extensions(app);
                None
            }
            PanelKind::Settings => self.build_settings(app).map(|field| {
                field.set_text(&self.filter.borrow());
                field
            }),
            PanelKind::Shortcuts => {
                self.build_shortcuts();
                None
            }
        };
        let column = if app.ui.side.is_visible() && app.ui.side.is_child_visible() {
            app.ui.side.allocated_width().max(0)
        } else {
            0
        };
        let band = if app.ui.topbar.is_visible() && app.ui.topbar.is_mapped() {
            app.ui.topbar.allocated_height().max(38)
        } else {
            0
        };
        self.column.set(column);
        self.band.set(band);
        let (width, height) = (
            app.ui.overlay.allocated_width().max(app.ui.window.allocated_width()),
            app.ui.overlay.allocated_height().max(app.ui.window.allocated_height()),
        );
        let page = (width - column).max(0);
        let card_width = panel_card_width(page);
        self.configure_layout(card_width, height);
        // Shown before measured. A widget that is not shown yet reports a
        // preferred height of 0, so measuring the title row and the nav before
        // this point counted them as nothing: the card came out 45px shorter
        // than its own header and nav, and the last of the extensions note was
        // cut off below the edge. This is the order the two have to be in.
        self.card.show_all();
        // An empty panel should be a note, not a slab: the floor used to be
        // 300px of nothing under one line of text.
        self.wanted_height.set(self.fit_height(card_width));
        self.card.set_can_focus(true);
        self.layer.show();
        self.layout(width, height);
        if !was_open || switched {
            self.scroll_for_open().vadjustment().set_value(0.0);
        }
        if !was_open && crate::motion::enabled() {
            // Hold the frame-0 starting pose immediately while the freshly
            // built panel settles its initial allocation and app-side
            // layout_with_column completes, then launch the VSync entrance on
            // idle so build/layout work cannot snap y or eat the frame budget.
            self.entering.set(true);
            self.scrim_alpha.set(0.0);
            self.scrim.queue_draw();
            self.card.set_opacity(0.0);
            if width > 1 && height > 1 {
                let tallest = MAX_HEIGHT.min((height - 80).max(1));
                let x = column + ((page - card_width) / 2).max(12);
                let y = ((height - tallest) / 2).max(band + 12);
                self.layer.root.move_(&self.card, x, y + crate::motion::dur::TRAVEL);
            }
            let open_gen = self.open_gen.clone();
            let entering = self.entering.clone();
            glib::idle_add_local_once(move || {
                if open_gen.get() != gen {
                    return;
                }
                let Some(app) = crate::app::current() else { return };
                let Ok(panels) = app.panels.try_borrow() else { return };
                if panels.open.is_none() || panels.open_gen.get() != gen {
                    return;
                }
                let width = app.ui.overlay.allocated_width().max(app.ui.window.allocated_width());
                let height = app.ui.overlay.allocated_height().max(app.ui.window.allocated_height());
                let column = panels.column.get();
                let band = panels.band.get();
                let page = (width - column).max(0);
                let card_width = panel_card_width(page);
                if panels.open.as_ref() != Some(&PanelKind::Settings)
                    || panels.fitted_width.get() != card_width
                {
                    panels.wanted_height.set(panels.fit_height(card_width));
                    let card_height = panels.card_height(card_width, height);
                    panels.card.set_size_request(card_width, card_height);
                    panels.scroll_for_open().check_resize();
                }
                panels.layer.place(&panels.scrim, 0, 0, width, height);
                crate::motion::fade_alpha(&panels.scrim, &panels.scrim_alpha, crate::motion::dur::ENTER);
                if width > 1 && height > 1 {
                    let tallest = MAX_HEIGHT.min((height - 80).max(1));
                    let x = column + ((page - card_width) / 2).max(12);
                    let y = ((height - tallest) / 2).max(band + 12);
                    let done_entering = entering.clone();
                    let done_gen = open_gen.clone();
                    crate::motion::slide_in_fixed_then(
                        &panels.layer.root,
                        &panels.card,
                        x,
                        y,
                        crate::motion::dur::TRAVEL,
                        crate::motion::dur::ENTER,
                        move || {
                            if done_gen.get() == gen {
                                done_entering.set(false);
                            }
                        },
                    );
                } else {
                    entering.set(false);
                    crate::motion::fade_in(&panels.card, crate::motion::dur::ENTER);
                }
            });
        } else {
            self.queue_refit();
            if !was_open {
                // Reduced motion / SEARCH_BENCH: finish synchronously in one step.
                crate::motion::fade_alpha(&self.scrim, &self.scrim_alpha, crate::motion::dur::ENTER);
                crate::motion::fade_in(&self.card, crate::motion::dur::ENTER);
            } else if switched {
                // Switching panels only swaps the content inside the open card, so
                // the card stays put and the content uses the short MICRO role.
                crate::motion::fade_from(&self.scroll, 0.35, 1.0, crate::motion::dur::MICRO);
            }
        }
        // Each showing of a panel gets its own generation number. A deferred
        // focus from an earlier showing checks it before it runs. Checking
        // only the panel kind let a close and reopen of the same panel inside
        // 30ms run the old timeout. That timeout took focus off the new
        // showing's filter field and moved it to the card.
        static FOCUS_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let generation = FOCUS_GEN.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if let Some(ref entry) = field {
            entry.grab_focus();
        } else {
            self.card.grab_focus();
            // Refocus only when all of these hold: this exact showing is still
            // current, the same panel is open, and no layer above it has the
            // keyboard. Those layers are the omnibox, the find bar, and any card
            // that can open inside the 30ms: a dialog or permission prompt for
            // the active tab, Peek, the site card or the first-run card. The
            // card must also still be on screen (Gemini R59 craft fix 4).
            glib::timeout_add_local_once(std::time::Duration::from_millis(30), {
                let card = self.card.clone();
                let opened = self.open.clone();
                move || {
                    if FOCUS_GEN.load(std::sync::atomic::Ordering::Relaxed) != generation {
                        return;
                    }
                    let Some(app) = crate::app::current() else { return };
                    let same_panel = app
                        .panels
                        .try_borrow()
                        .ok()
                        .is_some_and(|panels| panels.open.is_some() && panels.open == opened);
                    let find_open = app.find.try_borrow().ok().is_some_and(|find| find.visible);
                    let tab = app.active_id();
                    let card_above = crate::dialogs::showing(tab)
                        || crate::permissions::showing(tab)
                        || crate::peek::showing()
                        || crate::sitecard::is_open()
                        || crate::welcome::is_open(&app);
                    if same_panel && !app.omni.visible.get() && !find_open && !card_above && card.is_mapped() {
                        card.grab_focus();
                    }
                }
            });
        }
        field
    }

    fn matches(&self, text: &str) -> bool {
        let filter = self.filter.borrow();
        filter.is_empty() || text.to_lowercase().contains(filter.as_str())
    }

    pub fn open_panel(&mut self, app: &AppRef, kind: PanelKind) {
        *self.filter.borrow_mut() = String::new();
        self.open_panel_with(app, kind);
    }

    pub fn open_panel_filtered(&mut self, app: &AppRef, kind: PanelKind, filter: &str) {
        let query = filter.trim();
        *self.filter.borrow_mut() = query.to_lowercase();
        let entry = self.open_panel_with(app, kind.clone());
        if let Some(entry) = entry {
            if entry.text() != query {
                entry.set_text(query);
            }
            entry.set_position(query.chars().count() as i32);
        }
        if kind == PanelKind::Settings && !query.is_empty() {
            self.set_filter(app, PanelKind::Settings, query);
        }
    }

    /// Rebuild an open Downloads panel in-place without dropping filter text, cursor, or focus.
    pub fn refresh_downloads_open(&mut self, app: &AppRef) {
        if self.open.as_ref() != Some(&PanelKind::Downloads) {
            return;
        }
        let existing = self.body.children().iter().find_map(find_entry);
        let was_focused = existing.as_ref().is_some_and(|entry| entry.has_focus());
        let caret = existing.as_ref().map(|entry| entry.position()).unwrap_or(0);
        let typed = existing
            .as_ref()
            .map(|entry| entry.text().to_string())
            .unwrap_or_default();
        let scroll_val = self.scroll.vadjustment().value();

        let new_entry = self.build_downloads(app);
        let page = (app.ui.window.allocated_width() - self.column.get()).max(0);
        let card_width = panel_card_width(page);
        self.configure_layout(card_width, app.ui.window.allocated_height());
        self.wanted_height.set(self.fit_height(card_width));
        self.layout_card(
            app.ui.window.allocated_width(),
            app.ui.window.allocated_height(),
        );

        let adjust = self.scroll.vadjustment();
        adjust.set_value(scroll_val.clamp(
            adjust.lower(),
            (adjust.upper() - adjust.page_size()).max(adjust.lower()),
        ));

        if let Some(entry) = new_entry {
            if !typed.is_empty() {
                entry.set_text(&typed);
            }
            if was_focused {
                entry.grab_focus();
                entry.set_position(caret);
            }
        }
    }

    /// The card stands in the middle of the page it covers.
    pub fn layout(&self, width: i32, height: i32) {
        self.layout_card(width, height);
    }

    pub fn layout_with_column(&self, width: i32, height: i32, column: i32) {
        // A modal veil covers the whole window. Leaving the sidebar bright
        // split the frame in two and made the panel read as a page dialog.
        self.column.set(column);
        self.band.set(0);
        self.layout_card(width, height);
    }

    fn layout_card(&self, width: i32, height: i32) {
        let column = self.column.get();
        let band = self.band.get();
        let page = (width - column).max(0);
        self.layer.place(&self.scrim, 0, 0, width, height);
        let card_width = panel_card_width(page);
        let changed = self.open.is_some() && self.configure_layout(card_width, height);
        // A narrower window wraps the body's sentences onto more lines, so the
        // height the card asked for on the way in is no longer the height it
        // needs. Re-ask whenever the width moved.
        if self.open.is_some() && (changed || self.fitted_width.get() != card_width) {
            self.wanted_height.set(self.fit_height(card_width));
            self.queue_refit();
        }
        // The card is as tall as what it holds, within reason: a panel with
        // one line in it should not be a window with one line in it.
        let card_height = self.card_height(card_width, height);
        // The top edge comes from the window, not from what the card holds.
        // Centring on the card's own height made the rail and heading jump
        // about 89px when you switched from a tall panel (Settings) to a
        // short one (an empty Bookmarks). Anchor on the tallest card this
        // window allows instead, so switching panels only moves the bottom edge.
        let tallest = MAX_HEIGHT.min((height - 80).max(1));
        let top = ((height - tallest) / 2).max(band + 12);
        if self.entering.get() || crate::motion::is_animating(&self.card) {
            // Keep size up to date without snapping y mid-entrance.
            self.card.set_size_request(card_width, card_height);
        } else {
            self.layer.place(
                &self.card,
                column + ((page - card_width) / 2).max(12),
                top,
                card_width,
                card_height,
            );
        }
    }

    pub fn is_refit_pending(&self) -> bool {
        self.refit_queued.get()
    }

    pub fn close(&mut self, _ui: &crate::ui::Ui) {
        self.dismiss_popover();
        if self.open.is_none() {
            return;
        }
        self.open = None;
        self.open_gen.set(self.open_gen.get().wrapping_add(1));
        self.entering.set(false);
        crate::motion::cancel(&self.layer.root);
        crate::motion::cancel(&self.scrim);
        crate::motion::cancel(&self.card);
        crate::motion::cancel(&self.scroll);
        let root = self.layer.root.clone();
        crate::motion::fade_out_then(&self.layer.root, crate::motion::dur::EXIT, move || {
            let app = crate::app::current();
            let still_closed = app
                .as_ref()
                .and_then(|app| app.panels.try_borrow().ok().map(|p| p.open.is_none()))
                .unwrap_or(true);
            if !still_closed {
                return;
            }
            root.hide();
            root.set_no_show_all(true);
            root.set_opacity(1.0);
            // A closed panel lets go of what it was showing. Every open fills
            // the body again from scratch, so keeping the old rows (a whole
            // history or downloads list) only holds widgets and memory while
            // nothing is on screen. Clone the body out first, so no borrow of
            // the panels is held while GTK runs destroy handlers that may
            // borrow them again.
            let body = app.as_ref().and_then(|app| {
                let panels = app.panels.try_borrow().ok()?;
                if panels.open.is_some() {
                    return None;
                }
                panels.fitted_width.set(0);
                panels.adapted_width.set(0);
                Some(panels.body.clone())
            });
            if let Some(body) = body {
                for child in body.children() {
                    body.remove(&child);
                }
            }
        });
    }

    fn build_nav(&self, app: &AppRef, current: &PanelKind) {
        let dark = app.palette.borrow().is_dark();
        let needs_rebuild = self.nav_buttons.borrow().is_empty() || self.nav_dark.get() != Some(dark);
        if needs_rebuild {
            for child in self.nav.children() {
                self.nav.remove(&child);
            }
            let mut buttons = Vec::with_capacity(PanelKind::all().len());
            let muted = app.palette.borrow().muted;
            for kind in PanelKind::all() {
                let button = gtk::ToggleButton::new();
                crate::ui::class(&button, "tab");
                crate::ui::class(&button, "panel-nav-item");
                button.set_relief(gtk::ReliefStyle::None);
                button.set_can_focus(true);
                button.set_focus_on_click(false);
                button.set_tooltip_text(Some(kind.title()));
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                crate::ui::class(&content, "panel-nav-content");
                let icon = crate::icons::area(kind.icon(), 15, muted);
                crate::ui::class(&icon, "panel-nav-icon");
                icon.set_valign(gtk::Align::Center);
                content.pack_start(&icon, false, false, 0);
                let label = gtk::Label::new(Some(kind.title()));
                crate::ui::class(&label, "panel-nav-label");
                label.set_xalign(0.0);
                label.set_hexpand(true);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                content.pack_start(&label, true, true, 0);
                button.add(&content);
                if let Some(access) = button.accessible() {
                    gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::PageTab);
                    gtk::atk::prelude::AtkObjectExt::set_name(&access, kind.title());
                }
                let app_weak = app.weak();
                let target = kind.clone();
                let updating = self.updating_nav.clone();
                button.connect_clicked(move |_| {
                    if updating.get() {
                        return;
                    }
                    if let Some(app) = app_weak.upgrade() {
                        app.toggle_panel_switch(target.clone());
                    }
                });
                self.nav.pack_start(&button, false, true, 0);
                buttons.push((kind, button));
            }
            *self.nav_buttons.borrow_mut() = buttons;
            self.nav_dark.set(Some(dark));
            // Force compact styling to apply to freshly built nav buttons.
            self.configured_compact.set(None);
            self.nav.show_all();
        }
        self.updating_nav.set(true);
        for (kind, button) in self.nav_buttons.borrow().iter() {
            let active = kind == current;
            if button.is_active() != active {
                button.set_active(active);
            }
            let ctx = button.style_context();
            if active {
                if !ctx.has_class("on") {
                    ctx.add_class("on");
                }
                button.set_state_flags(gtk::StateFlags::CHECKED, false);
            } else {
                if ctx.has_class("on") {
                    ctx.remove_class("on");
                }
                button.unset_state_flags(gtk::StateFlags::CHECKED);
            }
        }
        self.updating_nav.set(false);
    }

    fn configure_layout(&self, card_width: i32, window_height: i32) -> bool {
        let compact = card_width < 700 || (window_height > 1 && window_height < 520);
        let changed = self.compact.replace(compact) != compact;
        let chrome_dirty = self.configured_compact.get() != Some(compact);
        if !changed && !chrome_dirty && self.fitted_width.get() == card_width && self.adapted_width.get() == self.content_width(card_width) {
            return false;
        }
        if chrome_dirty {
            let orientation = if compact { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal };
            self.card.set_orientation(orientation);
            self.rail.set_orientation(if compact { gtk::Orientation::Horizontal } else { gtk::Orientation::Vertical });
            self.rail.set_hexpand(compact);
            self.rail.set_size_request(if compact { -1 } else { RAIL_WIDTH }, -1);
            self.card.set_size_request(if compact { 560 } else { 880 }, 420);
            self.rail.set_child_packing(&self.nav, compact, true, 0, gtk::PackType::Start);
            self.nav.set_orientation(if compact { gtk::Orientation::Horizontal } else { gtk::Orientation::Vertical });
            self.nav.set_homogeneous(compact);
            self.nav.set_margin_top(if compact { 12 } else { 0 });
            self.nav.set_margin_bottom(12);
            self.rail_brand.set_no_show_all(compact);
            self.rail_brand.set_visible(!compact);
            self.rail_hint.set_no_show_all(compact);
            self.rail_hint.set_visible(!compact);
            if !compact {
                self.rail_brand.show_all();
                self.rail_hint.show_all();
            }
            let context = self.card.style_context();
            if compact {
                context.add_class("panel-compact");
            } else {
                context.remove_class("panel-compact");
            }
            for child in self.nav.children() {
                self.nav.set_child_packing(&child, compact, true, 0, gtk::PackType::Start);
                if let Some(button) = child.downcast_ref::<gtk::Button>() {
                    if let Some(content) = button.child().and_then(|child| child.downcast::<gtk::Box>().ok()) {
                        content.set_halign(if compact { gtk::Align::Center } else { gtk::Align::Fill });
                        for label in content.children().into_iter().filter(|child| child.is::<gtk::Label>()) {
                            label.set_no_show_all(compact);
                            label.set_visible(!compact);
                        }
                    }
                }
            }
            let inset = if compact { COMPACT_INSET } else { INSET };
            self.header.set_margin_start(inset);
            self.header.set_margin_end(inset);
            self.header.set_margin_top(if compact { 16 } else { 20 });
            self.header.set_margin_bottom(12);
            self.body.set_margin_start(inset);
            self.body.set_margin_end(inset);
            self.body.set_margin_bottom(inset);
            self.configured_compact.set(Some(compact));
        }
        self.adapt_body_if_needed(self.content_width(card_width));
        changed
    }

    fn adapt_body_if_needed(&self, target_width: i32) {
        if self.adapted_width.get() == target_width {
            return;
        }
        Self::adapt_content(self.body.upcast_ref(), target_width);
        self.adapted_width.set(target_width);
    }

    fn adapt_content(widget: &gtk::Widget, width: i32) {
        let context = widget.style_context();
        let (start, end, _, _) = widget_insets(widget);
        let inner = (width - start - end).max(1);
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if context.has_class("setting") || context.has_class("settingdesc") {
                configure_label_wrap(label);
            }
        }
        if let Some(container) = widget.downcast_ref::<gtk::Box>() {
            let threshold = if context.has_class("panel-row-layout") {
                460
            } else if context.has_class("panel-appearance") {
                300
            } else if context.has_class("panel-shortcut-row") {
                340
            } else {
                380
            };
            if context.has_class("panel-row-layout")
                || context.has_class("panel-setting-select")
                || context.has_class("panel-toolbar")
                || context.has_class("panel-keyword-form")
                || context.has_class("panel-shortcut-row")
                || context.has_class("panel-appearance")
            {
                let stacked = inner < threshold;
                let want_orient = if stacked { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal };
                if container.orientation() != want_orient {
                    container.set_orientation(want_orient);
                }
                for child in container.children() {
                    if !context.has_class("panel-appearance") && (child.is::<gtk::Button>() || child.is::<gtk::ComboBoxText>()) {
                        let want_align = if stacked { gtk::Align::Start } else { gtk::Align::Fill };
                        if child.halign() != want_align {
                            child.set_halign(want_align);
                        }
                    }
                }
            }
        }
        if let Some(container) = widget.downcast_ref::<gtk::Container>() {
            for child in container.children() {
                Self::adapt_content(&child, inner);
            }
        }
    }

    fn content_width(&self, card_width: i32) -> i32 {
        let (start, end, _, _) = widget_insets(&self.card);
        let rail = if self.compact.get() { 0 } else { self.rail.preferred_width().0 };
        (card_width - start - end - rail).max(1)
    }

    fn body_width(&self, card_width: i32) -> i32 {
        let (start, end, _, _) = widget_insets(&self.body);
        let (content_start, content_end, _, _) = widget_insets(&self.content);
        let (scroll_start, scroll_end, _, _) = widget_insets(&self.scroll);
        (self.content_width(card_width) - start - end - content_start - content_end - scroll_start - scroll_end).max(1)
    }

    fn chrome_height(&self, card_width: i32) -> i32 {
        let (_, _, card_top, card_bottom) = widget_insets(&self.card);
        let (content_start, content_end, content_top, content_bottom) = widget_insets(&self.content);
        let (_, _, scroll_top, scroll_bottom) = widget_insets(&self.scroll);
        let (_, _, body_top, body_bottom) = widget_insets(&self.body);
        let content_width = (self.content_width(card_width) - content_start - content_end).max(1);
        let navigation = if self.compact.get() {
            self.rail.preferred_height_for_width(content_width).1
        } else {
            0
        };
        card_top + card_bottom + content_top + content_bottom + scroll_top + scroll_bottom
            + body_top + body_bottom + self.header.preferred_height_for_width(content_width).1 + navigation
    }

    fn minimum_height(&self) -> i32 {
        if self.compact.get() {
            return 420;
        }
        let (_, _, top, bottom) = widget_insets(&self.card);
        (self.rail.preferred_height_for_width(self.rail.preferred_width().0).0 + top + bottom).max(240)
    }

    fn card_height(&self, _card_width: i32, window_height: i32) -> i32 {
        let available = (window_height - self.band.get() - 24).max(1);
        let minimum = self.minimum_height().min(available);
        self.wanted_height.get().min((window_height - 80).max(240)).max(minimum).min(available)
    }

    /// The body's visible widgets, with the permanent Settings holder expanded
    /// so measuring and row snapping see the same children as before.
    fn body_children(&self) -> Vec<gtk::Widget> {
        let mut widgets = Vec::new();
        for child in self.body.children() {
            if child == self.settings_holder {
                widgets.extend(self.settings_holder.children());
            } else {
                widgets.push(child);
            }
        }
        widgets
    }

    fn clear_body(&self) {
        self.fitted_width.set(0);
        self.adapted_width.set(0);
        for child in self.body.children() {
            if child != self.settings_holder {
                self.body.remove(&child);
            }
        }
        if self.settings_holder.parent().is_none() {
            self.body.add(&self.settings_holder);
        }
        for child in self.settings_holder.children() {
            self.settings_holder.remove(&child);
        }
    }

    fn queue_refit(&self) {
        if self.refit_queued.replace(true) {
            return;
        }
        let queued = self.refit_queued.clone();
        let card = self.card.downgrade();
        glib::idle_add_local_once(move || {
            queued.set(false);
            let Some(app) = crate::app::current() else { return };
            let Some(card) = card.upgrade() else { return };
            let Ok(panels) = app.panels.try_borrow() else { return };
            if panels.open.is_none() || panels.card != card {
                return;
            }
            let width = app.ui.overlay.allocated_width();
            let height = app.ui.overlay.allocated_height();
            let card_width = panel_card_width((width - panels.column.get()).max(1));
            if panels.open.as_ref() == Some(&PanelKind::Settings)
                && panels.fitted_width.get() == card_width
            {
                return;
            }
            panels.wanted_height.set(panels.fit_height(card_width));
            panels.layout_card(width, height);
            panels.scroll_for_open().check_resize();
        });
    }

    /// The height the card has to be at this width, from the measure below.
    ///
    /// Ask the widgets above the list how tall they are instead of adding a
    /// round 140 and hoping: the guess was 34px short, which cut the last row of
    /// a list in half and made a scrollable card look broken.
    fn fit_height(&self, card_width: i32) -> i32 {
        self.adapt_body_if_needed(self.content_width(card_width));
        let inner = self.body_width(card_width);
        let above = self.chrome_height(card_width);
        let minimum = self.minimum_height().min(MAX_HEIGHT);
        let wanted = self.measure_content(inner) + above;
        if wanted > MAX_HEIGHT {
            let inner_scrolled = (inner - 18).max(1);
            self.adapt_body_if_needed((self.content_width(card_width) - 18).max(1));
            self.fitted_width.set(card_width);
            let capped = (self.measure_content(inner_scrolled) + above).clamp(minimum, MAX_HEIGHT);
            // A snapped card can be shorter than the scrollbar's preferred
            // height, which clips a partially visible row. Keep the full cap:
            // the scrollbar already ends the viewport on whole rows.
            return capped;
        }
        self.fitted_width.set(card_width);
        let height = wanted.clamp(minimum, MAX_HEIGHT);
        height
    }

    /// How tall the body wants to be at a given width.
    ///
    /// Every body child counts, not just the scroller. History and bookmarks
    /// put their count line and their filter field above the list, and
    /// measuring only the list asked for a card 80px shorter than the body
    /// holds, which squeezed the field to nothing.
    fn measure_content(&self, width: i32) -> i32 {
        let mut total = 0;
        for child in self.body_children().into_iter().filter(|child| child.is_visible()) {
            child.set_size_request(-1, -1);
            let needed = child.preferred_height_for_width(width).1;
            self.body.set_child_packing(&child, false, true, 0, gtk::PackType::Start);
            total += needed;
        }
        total
    }

    #[allow(dead_code)]
    fn row_edges(widget: &gtk::Widget, width: i32, origin: i32, edges: &mut Vec<i32>) {
        let total = widget.preferred_height_for_width(width).1;
        let context = widget.style_context();
        let list = widget.downcast_ref::<gtk::Box>().filter(|container| {
            container.orientation() == gtk::Orientation::Vertical
                && !context.has_class("panel-empty")
                && !context.has_class("panel-setting-row")
                && !context.has_class("panel-row-layout")
                && !context.has_class("panel-toolbar")
                && !context.has_class("panel-shortcut-row")
                && !context.has_class("panel-appearance-option")
                && !context.has_class("panel-keyword-form")
        });
        if let Some(list) = list {
            let (start, end, top, _) = widget_insets(widget);
            let inner = (width - start - end).max(1);
            let mut position = origin + top;
            let children: Vec<_> = list.children().into_iter().filter(|child| child.is_visible()).collect();
            for child in &children {
                Self::row_edges(child, inner, position, edges);
                position += child.preferred_height_for_width(inner).1 + list.spacing();
            }
            edges.push(origin + total);
        } else {
            edges.push(origin + total);
        }
    }

    /// The scroller actually in use for the open panel. Settings moved out of
    /// the common scroller so its search field stays pinned; bench commands
    /// must follow it there instead of inspecting the empty common scroller.
    pub fn scroll_for_open(&self) -> gtk::ScrolledWindow {
        if self.open.as_ref() == Some(&PanelKind::Settings) {
            self.settings_holder
                .children()
                .into_iter()
                .find_map(|child| child.downcast::<gtk::ScrolledWindow>().ok())
                .unwrap_or_else(|| self.scroll.clone())
        } else {
            self.scroll.clone()
        }
    }

    fn body_scroll(&self, content: &gtk::Box) -> gtk::Box {
        // One scroller owns every panel. Returning the content here avoids
        // nesting a second GtkViewport, which breaks measurement and clipping.
        content.clone()
    }

    /// The Settings body is pinned outside the common scroller so typing in
    /// its filter does not rebuild the form. It still needs its own scroller;
    /// without one, `panelscroll` can only move the empty common viewport.
    fn settings_scroll(&self, content: &gtk::Box) -> gtk::ScrolledWindow {
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_overlay_scrolling(false);
        scroll.set_min_content_height(340);
        scroll.set_max_content_height(480);
        scroll.set_vexpand(true);
        scroll.add(content);
        scroll
    }

    /// Native search field backed by the browser's own Cairo `Icon::Search`
    /// instead of a named GTK theme icon (`edit-find-symbolic`), avoiding the
    /// external `glycin-svg` + `bwrap` SVG loader processes while preserving
    /// full entry focus, click, and accessibility UX.
    fn filter_field(
        &self,
        app: &AppRef,
        placeholder: &str,
        on_type: impl Fn(&str) + 'static,
    ) -> (gtk::Box, gtk::Entry) {
        let shell = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        crate::ui::class(&shell, "panel-filter-box");
        shell.set_hexpand(true);
        shell.set_margin_bottom(12);
        let icon = crate::icons::area(crate::icons::Icon::Search, 14, app.palette.borrow().muted);
        crate::ui::class(&icon, "panel-filter-icon");
        icon.set_valign(gtk::Align::Center);
        shell.pack_start(&icon, false, false, 0);

        let entry = gtk::Entry::new();
        crate::ui::class(&entry, "panel-filter");
        entry.set_has_frame(false);
        entry.set_placeholder_text(Some(placeholder));
        entry.set_width_chars(1);
        entry.set_hexpand(true);
        if let Some(access) = entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, placeholder);
        }
        let shell_focus = shell.downgrade();
        entry.connect_focus_in_event(move |_, _| {
            if let Some(s) = shell_focus.upgrade() {
                crate::ui::class(&s, "focused");
            }
            crate::gtkx::inh(false)
        });
        let shell_blur = shell.downgrade();
        entry.connect_focus_out_event(move |_, _| {
            if let Some(s) = shell_blur.upgrade() {
                s.style_context().remove_class("focused");
            }
            crate::gtkx::inh(false)
        });
        let on_type = std::rc::Rc::new(on_type);
        entry.connect_changed(move |entry| on_type(&entry.text()));
        shell.pack_start(&entry, true, true, 0);
        (shell, entry)
    }

    fn build_bookmarks(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_body();
        let bookmarks = app.state.borrow().bookmarks.clone();
        if bookmarks.is_empty() {
            let empty = self.empty_state(
                app,
                crate::icons::Icon::Star,
                "No bookmarks yet",
                "Bookmark a page to find it here later.",
                None,
            );
            // The next step is a button, not a sentence. It used to be a label
            // reading "Keep the current page", so the user had to turn the
            // words into a shortcut themselves. Now it is a focusable button
            // that bookmarks the page, and the Ctrl D keycap beside it still
            // teaches the shortcut. It uses the same row class and spacing as
            // the tip it replaces, so the empty state keeps its layout.
            let tip = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            crate::ui::class(&tip, "panel-empty-tip");
            tip.set_margin_top(8);
            let keep = gtk::Button::with_label("Keep the current page");
            crate::ui::class(&keep, "act");
            keep.set_can_focus(true);
            keep.set_halign(gtk::Align::Start);
            keep.set_valign(gtk::Align::Center);
            // A blank tab has no address, so add_bookmark would do nothing.
            // Turn the button off and say why, instead of letting a click fail
            // without a word.
            let has_page = !app
                .state
                .borrow()
                .tab(app.active_id())
                .map(|tab| tab.url.is_empty())
                .unwrap_or(true);
            keep.set_sensitive(has_page);
            keep.set_tooltip_text(Some(if has_page {
                "Bookmark the current page · Ctrl D"
            } else {
                "Open a page first, then bookmark it · Ctrl D"
            }));
            {
                let app_weak = app.weak();
                keep.connect_clicked(move |_| {
                    if let Some(app) = app_weak.upgrade() {
                        app.add_bookmark();
                    }
                });
            }
            tip.pack_start(&keep, false, false, 0);
            let keycap = gtk::Label::new(Some("Ctrl D"));
            crate::ui::class(&keycap, "keycap");
            keycap.set_valign(gtk::Align::Center);
            tip.pack_start(&keycap, false, false, 0);
            empty.pack_start(&tip, false, true, 0);
            self.body.add(&empty);
            self.body.show_all();
            return None;
        }
        let toolbar = self.toolbar(&format!("{} saved {}", bookmarks.len(), if bookmarks.len() == 1 { "page" } else { "pages" }));
        self.body.add(&toolbar);
        let (filter_box, filter) = self.filter_field(app, "Search bookmarks", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::Bookmarks, text);
            }
        });
        self.body.add(&filter_box);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        list.set_margin_end(10);
        let mut shown = 0;
        for bookmark in bookmarks.into_iter().filter(|bookmark| self.matches(&format!("{} {}", bookmark.title, bookmark.url))) {
            shown += 1;
            let remove_url = bookmark.url.clone();
            let row = self.row_with(app, &bookmark.title, &bookmark.url, "Open", {
                let url = bookmark.url.clone();
                move |app: &AppRef| app.navigate_active(&url)
            }, Some(("Remove", Box::new(move |app: &AppRef| {
                {
                    let mut state = app.state.borrow_mut();
                    state.bookmarks.retain(|kept| kept.url != remove_url);
                    Records::save_bookmarks(&state.bookmarks);
                }
                app.toggle_panel_switch(PanelKind::Bookmarks);
            }))));
            if let Some(surface) = app.favicon_for_url(&bookmark.url) {
                if let Some(tile) = row
                    .child()
                    .and_then(|c| c.downcast::<gtk::Box>().ok())
                    .and_then(|hbox| hbox.children().into_iter().next())
                    .and_then(|id| id.downcast::<gtk::Box>().ok())
                    .and_then(|id| id.children().into_iter().next())
                    .and_then(|t| t.downcast::<gtk::EventBox>().ok())
                {
                    if let Some(child) = tile.child() {
                        tile.remove(&child);
                    }
                    let img = crate::ui::favicon(&surface, 16);
                    crate::ui::class(&img, "favicon");
                    img.set_halign(gtk::Align::Center);
                    img.set_valign(gtk::Align::Center);
                    tile.add(&img);
                }
            }
            list.add(&row);
        }
        if shown == 0 {
            list.add(&self.empty_state(
                app,
                crate::icons::Icon::Search,
                "No saved pages match.",
                "Try a different title or address, or clear the search to see everything.",
                None,
            ));
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
        Some(filter)
    }

    fn build_history(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_body();
        let history = app.state.borrow().history.clone();
        if history.is_empty() {
            self.body.add(&self.empty_state(
                app,
                crate::icons::Icon::Clock,
                "No pages in your history.",
                "Pages you visit in regular tabs appear here. Private tabs are not added to this history.",
                Some(("Ctrl L", "Find your next page")),
            ));
            self.body.show_all();
            return None;
        }
        let header = self.toolbar(&if history.len() == 1 {
            "1 page".to_string()
        } else {
            format!("{} pages", history.len())
        });
        let clear = gtk::Button::with_label("Clear History");
        crate::ui::class(&clear, "act");
        clear.set_valign(gtk::Align::Center);
        clear.set_vexpand(false);
        // The one irreversible control in the panel says so before it is
        // pressed. The stylesheet owns the colour: `dangerink` (#C0392B light,
        // #EE8B76 dark) is 5.44:1 on the white chip and 5.87:1 on the dark one.
        // This only names the class; the rule belongs to css.rs.
        crate::ui::class(&clear, "danger");
        {
            let app_weak = app.weak();
            let armed = std::rc::Rc::new(std::cell::Cell::new(false));
            // Bumped on every arm and on confirm. A five-second tick only
            // disarms the press it was started for, so a stale timer cannot
            // reset the label of a newer arm, and a confirm leaves no pending
            // tick that touches the chip afterwards.
            let generation = std::rc::Rc::new(std::cell::Cell::new(0u32));
            let for_click = armed.clone();
            let gen_click = generation.clone();
            let button_weak = clear.downgrade();
            clear.connect_clicked(move |button| {
                if for_click.replace(false) {
                    gen_click.set(gen_click.get().wrapping_add(1));
                    if let Some(app) = app_weak.upgrade() {
                        app.state.borrow_mut().history.clear();
                        Records::save_history(&[]);
                        app.toggle_panel_switch(PanelKind::History);
                    }
                    return;
                }
                for_click.set(true);
                button.set_label("Clear history? Click again");
                let tick = gen_click.get().wrapping_add(1);
                gen_click.set(tick);
                let later_armed = armed.clone();
                let later_gen = gen_click.clone();
                let weak = button_weak.clone();
                gtk::glib::timeout_add_seconds_local(5, move || {
                    if later_gen.get() == tick {
                        later_armed.set(false);
                        if let Some(button) = weak.upgrade() {
                            button.set_label("Clear History");
                        }
                    }
                    gtk::glib::ControlFlow::Break
                });
            });
        }
        header.pack_start(&clear, false, false, 0);
        self.body.add(&header);

        let (filter_box, filter) = self.filter_field(app, "Search history", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::History, text);
            }
        });
        self.body.add(&filter_box);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        list.set_margin_end(10);
        let now = now_secs();
        // The filter runs before the cap, not after: taking the newest 300 and
        // then searching them made every older page unfindable while the
        // header still counted it.
        let mut shown = 0;
        let mut day = String::new();
        for entry in history
            .iter()
            .filter(|entry| self.matches(&format!("{} {}", entry.title, entry.url)))
            .take(300)
        {
            // A flat list of "43 days ago" cannot answer "what did I read on
            // Tuesday". A heading per day costs one label and makes the list
            // scannable.
            let which = day_of(entry.at, now);
            if which != day {
                day = which.clone();
                let head = gtk::Label::new(Some(&day));
                crate::ui::class(&head, "group");
                crate::ui::class(&head, "panel-list-heading");
                head.set_xalign(0.0);
                head.set_margin_top(if shown == 0 { 0 } else { 18 });
                head.set_margin_bottom(8);
                list.add(&head);
            }
            shown += 1;
            let title = if entry.title.is_empty() {
                crate::shield::host_of(&entry.url)
            } else {
                entry.title.clone()
            };
            let when = ago(entry.at, now);
            let remove_url = entry.url.clone();
            let row = self.row_with(app, &title, &format!("{} · {}", entry.url, when), "Open", {
                let url = entry.url.clone();
                move |app: &AppRef| app.navigate_active(&url)
            }, Some(("Remove", Box::new(move |app: &AppRef| {
                {
                    let mut state = app.state.borrow_mut();
                    state.history.retain(|kept| kept.url != remove_url);
                    Records::save_history(&state.history);
                }
                app.toggle_panel_switch(PanelKind::History);
            }))));
            list.add(&row);
        }
        if shown == 0 {
            list.add(&self.empty_state(
                app,
                crate::icons::Icon::Search,
                "No pages match this search.",
                "Try another word or part of an address. Clearing the search brings your history back.",
                None,
            ));
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
        Some(filter)
    }

    fn build_downloads(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_empty_or_list(app)
    }

    fn clear_empty_or_list(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_body();
        let downloads = app.state.borrow().downloads.clone();
        if downloads.is_empty() {
            self.body.add(&self.empty_state(
                app,
                crate::icons::Icon::Download,
                "No downloads yet",
                "Follow file progress here, then open a finished download or find it in your Downloads folder.",
                None,
            ));
            let open = gtk::Button::with_label("Open Downloads folder");
            crate::ui::class(&open, "act");
            crate::ui::class(&open, "primary");
            open.set_halign(gtk::Align::Start);
            open.set_can_focus(true);
            open.connect_clicked(|_| {
                let _ = std::process::Command::new("xdg-open").arg(crate::web::downloads_dir()).spawn();
            });
            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            crate::ui::class(&actions, "panel-action-bar");
            actions.set_halign(gtk::Align::Start);
            actions.set_margin_bottom(24);
            actions.pack_start(&open, false, false, 0);
            self.body.add(&actions);
            self.body.show_all();
            return None;
        }
        let toolbar = self.toolbar(&format!("{} {}", downloads.len(), if downloads.len() == 1 { "file" } else { "files" }));
        let open_folder = gtk::Button::with_label("Open folder");
        crate::ui::class(&open_folder, "act");
        open_folder.set_valign(gtk::Align::Center);
        open_folder.set_tooltip_text(Some("Open Downloads folder"));
        open_folder.connect_clicked(|_| {
            let _ = std::process::Command::new("xdg-open").arg(crate::web::downloads_dir()).spawn();
        });
        toolbar.pack_start(&open_folder, false, false, 0);
        self.body.add(&toolbar);
        let (filter_box, filter) = self.filter_field(app, "Search downloads", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::Downloads, text);
            }
        });
        self.body.add(&filter_box);
        let where_to = gtk::Label::new(Some(&format!(
            "Saved in {}",
            crate::web::downloads_dir().to_string_lossy()
        )));
        crate::ui::class(&where_to, "sub");
        crate::ui::class(&where_to, "panel-location");
        where_to.set_xalign(0.0);
        where_to.set_tooltip_text(Some(&crate::web::downloads_dir().to_string_lossy()));
        where_to.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        where_to.set_margin_bottom(16);
        self.body.add(&where_to);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        list.set_margin_end(10);
        let mut shown = 0;
        for download in downloads.into_iter().filter(|download| self.matches(&format!("{} {}", download.name, download.path))) {
            shown += 1;
            let sub = download_subtitle(&download);
            let remove_path = download.path.clone();
            // "Open" only when there is a file to open: a button that does
            // nothing on a half-written download is worse than no button.
            let action = if download.done { "Open" } else { "Reveal" };
            let second: (&str, Box<dyn Fn(&AppRef)>) = if !download.done && !download.failed {
                let id = download.id;
                ("Cancel", Box::new(move |_app: &AppRef| {
                    crate::web::cancel_download(id);
                }))
            } else {
                ("Remove", Box::new(move |app: &AppRef| {
                    // Removes the row, not the file. Deleting someone's file from
                    // a list of records is not what "Remove" means here.
                    {
                        let mut state = app.state.borrow_mut();
                        state.downloads.retain(|kept| kept.path != remove_path);
                        Records::save_downloads(&state.downloads);
                    }
                    app.toggle_panel_switch(PanelKind::Downloads);
                }))
            };
            let row = self.row_with(app, &download.name, &sub, action, {
                let path = download.path.clone();
                let finished = download.done;
                move |_app: &AppRef| {
                    let target = std::path::Path::new(&path);
                    let target = if finished {
                        target.to_path_buf()
                    } else {
                        target.parent().unwrap_or(std::path::Path::new("/")).to_path_buf()
                    };
                    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
                }
            }, Some(second));
            if let Some(texts) = row
                .child()
                .and_then(|c| c.downcast::<gtk::Box>().ok())
                .and_then(|hbox| hbox.children().into_iter().next())
                .and_then(|id| id.downcast::<gtk::Box>().ok())
                .and_then(|id| id.children().into_iter().nth(1))
                .and_then(|t| t.downcast::<gtk::Box>().ok())
            {
                if download.failed {
                    if let Some(sub_label) = texts.children().into_iter().nth(1) {
                        crate::ui::class(&sub_label, "dl-failed");
                    }
                } else if !download.done && download.total > 0 {
                    let bar = gtk::ProgressBar::new();
                    crate::ui::class(&bar, "dl-progress");
                    bar.set_fraction((download.got as f64 / download.total as f64).clamp(0.0, 1.0));
                    texts.add(&bar);
                }
            }
            list.add(&row);
        }
        if shown == 0 {
            list.add(&self.empty_state(
                app,
                crate::icons::Icon::Search,
                "No files match this search.",
                "Search by file name or location, or clear the field to see all your downloads.",
                None,
            ));
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
        Some(filter)
    }

    /// Extensions, and the truth about them.
    ///
    /// A browser without a WebExtensions API cannot run most of what the stores
    /// list, so this panel shows every extension it has, which ones actually do
    /// something here, and the sentence saying why not for the others. Anything
    /// else would be a lie a person only discovers when their passwords do not
    /// fill in.
    fn build_extensions(&self, app: &AppRef) {
        self.clear_body();
        let installed = crate::extensions::registry().extensions;
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        list.set_margin_end(10);

        let disclosure = gtk::Label::new(Some(
            "Content scripts and CSS run on the sites an add-on requests. WebExtensions background workers, chrome.*/browser.* APIs, extension storage, and nativeMessaging are not supported by WebKitGTK — password managers (such as Bitwarden) and built-in autofill do not work here.",
        ));
        crate::ui::class(&disclosure, "settingdesc");
        crate::ui::class(&disclosure, "panel-note");
        disclosure.set_xalign(0.0);
        disclosure.set_line_wrap(true);
        disclosure.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        disclosure.set_margin_bottom(14);
        list.add(&disclosure);

        let installed_section = self.section(app, crate::icons::Icon::Spark, "Installed", "Enable, inspect, or remove installed add-ons.");

        if installed.is_empty() {
            installed_section.add(&self.empty_state(
                app,
                crate::icons::Icon::Spark,
                "No add-ons installed",
                "Browse Firefox Add-ons below or install a .xpi, .crx, or unpacked folder from this computer.",
                None,
            ));
        }
        for entry in installed {
            let mut notes = vec![entry.why_not.replace('\n', " ")];
            notes.extend(crate::extensions::not_drawn(&entry));
            notes.retain(|note| !note.trim().is_empty());
            let subtitle = if entry.partial {
                format!("{}  ·  loads on the sites it asks for  ·  {}", entry.version, notes.join("; "))
            } else if entry.runs {
                format!("{}  ·  runs on the sites it asks for  ·  {}", entry.version, notes.join("; "))
            } else {
                format!("{}  ·  {}", entry.version, notes.join("; "))
            };
            let id = entry.id.clone();
            let runs = entry.runs;
            let (switch_active, switch_sensitive) = extension_switch_state(runs, entry.enabled);
            let row = self.row_with(
                app,
                &entry.name,
                &subtitle,
                if switch_active { "Disable" } else { "Enable" },
                {
                    let id = id.clone();
                    move |app: &AppRef| {
                        if !runs {
                            return;
                        }
                        crate::extensions::set_enabled(&id, !switch_active);
                        app.rebuild_every_view();
                        app.open_panel(PanelKind::Extensions);
                    }
                },
                Some(("Remove", Box::new({
                    let id = id.clone();
                    move |app: &AppRef| {
                        crate::extensions::remove(&id);
                        app.rebuild_every_view();
                        app.open_panel(PanelKind::Extensions);
                    }
                }))),
            );
            if let Some(actions) = row
                .child()
                .and_then(|c| c.downcast::<gtk::Box>().ok())
                .and_then(|hbox| hbox.children().into_iter().nth(1))
                .and_then(|a| a.downcast::<gtk::Box>().ok())
            {
                if let Some(first_btn) = actions.children().into_iter().next() {
                    actions.remove(&first_btn);
                }
                let toggle = Self::pill_switch(app, switch_active, switch_sensitive);
                toggle.set_tooltip_text(Some(if !switch_sensitive {
                    "Not supported here — nothing to run"
                } else if switch_active {
                    "Disable add-on"
                } else {
                    "Enable add-on"
                }));
                if let Some(access) = toggle.accessible() {
                    gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::ToggleButton);
                    gtk::atk::prelude::AtkObjectExt::set_name(
                        &access,
                        &if switch_sensitive {
                            format!("Enable {}", entry.name)
                        } else {
                            format!("{} (cannot run here)", entry.name)
                        },
                    );
                }
                if switch_sensitive {
                    let app_weak = app.weak();
                    let id_toggle = id.clone();
                    toggle.connect_toggled(move |sw| {
                        if let Some(app) = app_weak.upgrade() {
                            let next = sw.is_active();
                            if next != switch_active {
                                crate::extensions::set_enabled(&id_toggle, next);
                                app.rebuild_every_view();
                                app.open_panel(PanelKind::Extensions);
                            }
                        }
                    });
                }
                actions.pack_start(&toggle, false, false, 0);
                actions.reorder_child(&toggle, 0);
            }
            installed_section.add(&row);
            if let Some(page) = crate::extensions::options_page(&entry) {
                let row = self.link_row(app, &format!("{} options", entry.name), "Opens the page the add-on ships", "Options", {
                    let url = format!("file://{page}");
                    move |app: &AppRef| {
                        app.open_panel(PanelKind::Extensions);
                        app.new_tab(Some(&url), false, false);
                    }
                });
                installed_section.add(&row);
            }
        }
        list.add(&installed_section);

        let current = app.state.borrow().tab(app.active_id()).map(|tab| tab.url.clone()).unwrap_or_default();
        let acting = if current.is_empty() { Vec::new() } else { crate::extensions::here(&current) };
        if !acting.is_empty() {
            let active_section = self.section(app, crate::icons::Icon::Globe, "On this page", "Add-ons acting on the page in front of you.");
            let line = gtk::Label::new(Some(&acting.join(", ")));
            crate::ui::class(&line, "settingdesc");
            line.set_xalign(0.0);
            line.set_line_wrap(true);
            line.set_line_wrap_mode(gtk::pango::WrapMode::Word);
            active_section.add(&line);
            list.add(&active_section);
        }

        // ---- Discover Firefox Add-ons (addons.mozilla.org) ----
        let discover = self.section(
            app,
            crate::icons::Icon::Search,
            "Discover Firefox Add-ons",
            "Official addons.mozilla.org listings with honest compatibility badges. Searches connect to addons.mozilla.org only when you press Search.",
        );
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        search_row.set_margin_bottom(8);
        let amo_entry = gtk::Entry::new();
        amo_entry.set_placeholder_text(Some("Search Firefox Add-ons (addons.mozilla.org)…"));
        amo_entry.set_hexpand(true);
        amo_entry.set_width_chars(1);
        if let Some(access) = amo_entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Search Firefox Add-ons");
        }
        let amo_search_btn = gtk::Button::with_label("Search");
        crate::ui::class(&amo_search_btn, "act");
        amo_search_btn.set_valign(gtk::Align::Center);
        search_row.pack_start(&amo_entry, true, true, 0);
        search_row.pack_start(&amo_search_btn, false, false, 0);
        discover.add(&search_row);

        let status_label = gtk::Label::new(Some("Featured official Firefox Add-ons listings:"));
        crate::ui::class(&status_label, "panel-count");
        status_label.set_xalign(0.0);
        status_label.set_margin_bottom(6);
        discover.add(&status_label);

        let catalog_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        populate_catalog_rows(app, &catalog_box, &crate::addons_catalog::curated_entries());
        discover.add(&catalog_box);

        {
            let run_search = {
                let app_weak = app.weak();
                let entry_w = amo_entry.downgrade();
                let status_w = status_label.downgrade();
                let box_w = catalog_box.downgrade();
                std::rc::Rc::new(move || {
                    let (Some(app), Some(entry), Some(status), Some(cbox)) = (
                        app_weak.upgrade(),
                        entry_w.upgrade(),
                        status_w.upgrade(),
                        box_w.upgrade(),
                    ) else {
                        return;
                    };
                    let q = entry.text().trim().to_string();
                    if q.is_empty() {
                        crate::addons_catalog::cancel_search();
                        status.set_text("Featured official Firefox Add-ons listings:");
                        populate_catalog_rows(&app, &cbox, &crate::addons_catalog::curated_entries());
                        return;
                    }
                    status.set_text(&format!("Searching addons.mozilla.org for \"{q}\"…"));
                    let local_hits = crate::addons_catalog::filter_curated(&q);
                    if !local_hits.is_empty() {
                        populate_catalog_rows(&app, &cbox, &local_hits);
                    }
                    let app_done = app.weak();
                    let status_done = status.downgrade();
                    let box_done = cbox.downgrade();
                    let q_done = q.clone();
                    crate::addons_catalog::search_amo(&q, move |result| {
                        let (Some(app), Some(status), Some(cbox)) = (
                            app_done.upgrade(),
                            status_done.upgrade(),
                            box_done.upgrade(),
                        ) else {
                            return;
                        };
                        match result {
                            Ok(entries) if !entries.is_empty() => {
                                status.set_text(&format!(
                                    "{} official addons.mozilla.org results for \"{q_done}\":",
                                    entries.len()
                                ));
                                populate_catalog_rows(&app, &cbox, &entries);
                            }
                            Ok(_) => {
                                status.set_text(&format!("No add-ons found on addons.mozilla.org for \"{q_done}\"."));
                                populate_catalog_rows(&app, &cbox, &[]);
                            }
                            Err(_) => {
                                let fallback = crate::addons_catalog::filter_curated(&q_done);
                                status.set_text("Could not reach addons.mozilla.org — showing offline catalog matches.");
                                populate_catalog_rows(&app, &cbox, &fallback);
                            }
                        }
                    });
                })
            };
            let run_click = run_search.clone();
            amo_search_btn.connect_clicked(move |_| run_click());
            let run_act = run_search.clone();
            amo_entry.connect_activate(move |_| run_act());
        }

        let firefox = self.link_row(
            app,
            "Browse all Firefox Add-ons",
            "Opens addons.mozilla.org in a new tab. Clicking Add to Firefox triggers the install confirmation card.",
            "Open store",
            move |app: &AppRef| {
                app.toggle_panel_switch(PanelKind::Extensions);
                app.new_tab(Some("https://addons.mozilla.org/firefox/extensions/"), false, true);
            },
        );
        discover.add(&firefox);
        list.add(&discover);

        // ---- Install from disk ----
        let installation = self.section(app, crate::icons::Icon::Plus, "Install from this computer", "Install a local .xpi, .crx, or unpacked extension folder.");

        let field = gtk::Entry::new();
        field.set_placeholder_text(Some("A .crx, an .xpi, or a folder with a manifest.json"));
        field.set_width_chars(1);
        field.set_hexpand(true);
        if let Some(access) = field.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Extension path or archive");
        }
        let app_field = app.weak();
        field.connect_activate(move |field| {
            let Some(app) = app_field.upgrade() else { return };
            install_extension_path(&app, &std::path::PathBuf::from(field.text().to_string()));
        });
        let choose = gtk::Button::with_label("Choose…");
        crate::ui::class(&choose, "act");
        choose.set_valign(gtk::Align::Center);
        if let Some(access) = choose.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Choose a file or folder to install");
        }
        choose.set_tooltip_text(Some("Install a .crx, .xpi or an unpacked extension folder"));
        let app_choose = app.weak();
        choose.connect_clicked(move |_| {
            let Some(app) = app_choose.upgrade() else { return };
            let dialog = gtk::FileChooserNative::new(
                Some("Install an add-on"),
                Some(&app.ui.window),
                gtk::FileChooserAction::Open,
                Some("Install"),
                Some("Cancel"),
            );
            dialog.set_select_multiple(false);
            let app_native = app.weak();
            dialog.connect_response(move |dialog, response| {
                if response == gtk::ResponseType::Accept {
                    if let Some(app) = app_native.upgrade() {
                        if let Some(path) = dialog.filename() {
                            install_extension_path(&app, &path);
                        }
                    }
                }
                dialog.destroy();
                EXTENSION_CHOOSERS.with(|c| c.borrow_mut().retain(|kept| kept != dialog));
            });
            EXTENSION_CHOOSERS.with(|c| c.borrow_mut().push(dialog.clone()));
            dialog.show();
        });
        let install_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        install_row.set_margin_top(8);
        install_row.set_margin_bottom(8);
        install_row.pack_start(&field, true, true, 0);
        install_row.pack_start(&choose, false, false, 0);
        installation.add(&install_row);
        list.add(&installation);

        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    fn build_settings(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_body();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        list.set_margin_end(10);
        let prefs = app.prefs.borrow().clone();
        let query = self.filter.borrow().clone();
        let app_weak = std::rc::Rc::<crate::app::App>::downgrade(app);
        let (field_box, field) = self.filter_field(app, "Search settings", move |text| {
            if let Some(app) = app_weak.upgrade() {
                app.filter_panel(crate::panels::PanelKind::Settings, text);
            }
        });
        field.set_text(&query);
        field.set_position(query.chars().count() as i32);
        // The filter is chrome, not a setting: keeping it outside the
        // scroller stops the bottom-of-panel proof from clipping it in half.
        self.settings_holder.add(&field_box);

        let appearance = self.section(app, crate::icons::Icon::Spark, "Appearance", "Light, dark, homepage shortcuts, dither canvas, and window controls.");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        crate::ui::class(&buttons, "panel-appearance");
        buttons.set_homogeneous(true);
        for look in [crate::store::Look::Light, crate::store::Look::Dark, crate::store::Look::System] {
            buttons.pack_start(&self.appearance_option(app, look.clone(), look == prefs.appearance), true, true, 0);
        }
        appearance.add(&buttons);
        appearance.add(&self.setting_switch(
            app,
            "Show homepage shortcuts",
            "Display saved bookmark chips on the regular start page. Never shown on private tabs.",
            prefs.show_start_shortcuts,
            |app, on| app.set_start_shortcuts_visible(on),
        ));
        appearance.add(&self.option_picker_row(
            app,
            "Homepage shortcuts",
            "Choose how saved bookmarks appear on the start page. Right-clicking the start page also switches modes.",
            crate::store::StartBookmarksMode::all()
                .iter()
                .map(|m| m.title())
                .collect(),
            prefs.start_bookmarks_mode.title(),
            |app, chosen| {
                if let Some(mode) = crate::store::StartBookmarksMode::all()
                    .into_iter()
                    .find(|m| m.title() == chosen)
                {
                    app.set_start_bookmarks_mode(mode);
                }
            },
        ));
        let active_shortcut_count = match prefs.start_shortcuts_count {
            0..=2 => "2",
            3..=4 => "4",
            5..=6 => "6",
            7..=8 => "8",
            9..=10 => "10",
            _ => "12",
        };
        appearance.add(&self.option_picker_row(
            app,
            "Homepage shortcut count",
            "Maximum number of saved bookmark chips displayed on the regular start page.",
            vec!["2", "4", "6", "8", "10", "12"],
            active_shortcut_count,
            |app, chosen| {
                if let Ok(count) = chosen.parse::<usize>() {
                    app.set_start_shortcuts_count(count);
                }
            },
        ));
        appearance.add(&self.option_picker_row(
            app,
            "Homepage background dither",
            "Control the ordered halftone horizon behind the start page (Off, Subtle, or Full).",
            crate::store::StartDitherMode::all()
                .iter()
                .map(|m| m.title())
                .collect(),
            prefs.start_dither.title(),
            |app, chosen| {
                if let Some(mode) = crate::store::StartDitherMode::all()
                    .into_iter()
                    .find(|m| m.title() == chosen)
                {
                    app.set_start_dither_mode(mode);
                }
            },
        ));
        appearance.add(&self.option_picker_row(
            app,
            "Window controls",
            "System uses your desktop titlebar when available; Browser draws compact controls in the tab strip; Hidden removes both (use menu Close Window or Alt+F4).",
            crate::store::WindowControlsMode::all()
                .iter()
                .map(|m| m.title())
                .collect(),
            prefs.window_controls.title(),
            |app, chosen| {
                if let Some(mode) = crate::store::WindowControlsMode::all()
                    .into_iter()
                    .find(|m| m.title() == chosen)
                {
                    app.set_window_controls_mode(mode);
                }
            },
        ));
        list.add(&appearance);

        let layout = self.section(
            app,
            crate::icons::Icon::Sidebar,
            "Toolbar & layout",
            "Presets, toolbar button visibility and order, chrome density, tab sizing, and portable appearance profiles. Hidden actions stay in the browser menu and shortcuts.",
        );
        let active_preset = crate::customize::LayoutPreset::detect(&prefs);
        let preset_row = gtk::Box::new(gtk::Orientation::Vertical, 8);
        crate::ui::class(&preset_row, "panel-setting-row");
        crate::ui::class(&preset_row, "customize-preset-row");
        let preset_head = gtk::Label::new(Some("Layout presets"));
        crate::ui::class(&preset_head, "setting");
        preset_head.set_xalign(0.0);
        let preset_sub = gtk::Label::new(Some(match active_preset {
            Some(p) => p.description(),
            None => "Custom layout active. Choose Minimal, Standard, or Full to switch presets.",
        }));
        crate::ui::class(&preset_sub, "settingdesc");
        preset_sub.set_xalign(0.0);
        preset_sub.set_line_wrap(true);
        preset_sub.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        let preset_btns = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        crate::ui::class(&preset_btns, "customize-preset-group");
        preset_btns.set_homogeneous(true);
        for preset in crate::customize::LayoutPreset::ALL {
            let btn = gtk::Button::with_label(preset.title());
            crate::ui::class(&btn, "act");
            crate::ui::class(&btn, "customize-preset-btn");
            if active_preset == Some(preset) {
                crate::ui::class(&btn, "chosen");
            }
            btn.set_tooltip_text(Some(preset.description()));
            if let Some(access) = btn.accessible() {
                gtk::atk::prelude::AtkObjectExt::set_name(&access, &format!("Apply {} layout preset", preset.title()));
            }
            let weak = app.weak();
            btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.apply_layout_preset(preset);
                    app.toggle_panel_switch(PanelKind::Settings);
                }
            });
            preset_btns.pack_start(&btn, true, true, 0);
        }
        preset_row.pack_start(&preset_head, false, true, 0);
        preset_row.pack_start(&preset_sub, false, true, 0);
        preset_row.pack_start(&preset_btns, false, true, 0);
        layout.add(&preset_row);

        layout.add(&self.setting_switch(
            app,
            "Show navigation bar",
            "Keeps navigation and the address row below the top tabs. Ctrl Shift S also toggles it.",
            prefs.show_toolbar,
            |app, on| app.set_toolbar_visible(on),
        ));
        layout.add(&self.setting_switch(
            app,
            "Back button",
            "Show the Back navigation button independently in the toolbar.",
            prefs.show_back_button,
            |app, on| {
                app.set_back_button_visible(on);
                app.toggle_panel_switch(PanelKind::Settings);
            },
        ));
        layout.add(&self.setting_switch(
            app,
            "Forward button",
            "Show the Forward navigation button independently in the toolbar.",
            prefs.show_forward_button,
            |app, on| {
                app.set_forward_button_visible(on);
                app.toggle_panel_switch(PanelKind::Settings);
            },
        ));
        layout.add(&self.setting_switch(
            app,
            "Reload / Stop button",
            "Show the Reload and Stop loading button in the toolbar.",
            prefs.show_reload_button,
            |app, on| {
                app.set_reload_button_visible(on);
                app.toggle_panel_switch(PanelKind::Settings);
            },
        ));
        layout.add(&self.setting_switch(
            app,
            "Home button",
            "Show a Home button that returns the active tab to the start page.",
            prefs.show_home_button,
            |app, on| {
                app.set_home_button_visible(on);
                app.toggle_panel_switch(PanelKind::Settings);
            },
        ));
        layout.add(&self.setting_switch(
            app,
            "Reader mode button",
            "Show the Reading mode button in the toolbar. Always available from the browser menu and Ctrl Shift R.",
            prefs.show_reader_button,
            |app, on| {
                app.set_reader_button_visible(on);
                app.toggle_panel_switch(PanelKind::Settings);
            },
        ));
        layout.add(&self.setting_switch(
            app,
            "Bookmark button",
            "Show the Bookmark button in the toolbar. Always available from the browser menu and Ctrl D.",
            prefs.show_bookmark_button,
            |app, on| {
                app.set_bookmark_button_visible(on);
                app.toggle_panel_switch(PanelKind::Settings);
            },
        ));

        let order_block = gtk::Box::new(gtk::Orientation::Vertical, 6);
        crate::ui::class(&order_block, "panel-setting-row");
        crate::ui::class(&order_block, "customize-order-block");
        let order_title = gtk::Label::new(Some("Toolbar action order"));
        crate::ui::class(&order_title, "setting");
        order_title.set_xalign(0.0);
        let order_desc = gtk::Label::new(Some(
            "Arrange toolbar actions left-to-right using Up and Down. The address bar stays visible as a required anchor.",
        ));
        crate::ui::class(&order_desc, "settingdesc");
        order_desc.set_xalign(0.0);
        order_desc.set_line_wrap(true);
        order_desc.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        order_block.pack_start(&order_title, false, true, 0);
        order_block.pack_start(&order_desc, false, true, 0);

        let normalized_order = crate::customize::normalize_toolbar_order(&prefs.toolbar_order);
        let total_actions = normalized_order.len();
        for (idx, action) in normalized_order.into_iter().enumerate() {
            let item_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            crate::ui::class(&item_row, "customize-order-row");
            if action.is_mandatory() {
                crate::ui::class(&item_row, "customize-order-required");
            }
            let name = gtk::Label::new(Some(action.title()));
            crate::ui::class(&name, "customize-order-label");
            name.set_xalign(0.0);
            name.set_hexpand(true);
            let status_text = if action.is_mandatory() {
                "Required"
            } else if action.is_visible_in(&prefs) {
                "Shown"
            } else {
                "Hidden"
            };
            let status = gtk::Label::new(Some(status_text));
            crate::ui::class(&status, "panel-count");
            status.set_valign(gtk::Align::Center);

            let up_btn = gtk::Button::with_label("Up");
            crate::ui::class(&up_btn, "act");
            crate::ui::class(&up_btn, "customize-order-btn");
            up_btn.set_sensitive(idx > 0);
            up_btn.set_valign(gtk::Align::Center);
            if let Some(a) = up_btn.accessible() {
                gtk::atk::prelude::AtkObjectExt::set_name(&a, &format!("Move {} earlier", action.title()));
            }
            {
                let weak = app.weak();
                up_btn.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.move_toolbar_action(action, -1);
                        app.toggle_panel_switch(PanelKind::Settings);
                    }
                });
            }

            let down_btn = gtk::Button::with_label("Down");
            crate::ui::class(&down_btn, "act");
            crate::ui::class(&down_btn, "customize-order-btn");
            down_btn.set_sensitive(idx + 1 < total_actions);
            down_btn.set_valign(gtk::Align::Center);
            if let Some(a) = down_btn.accessible() {
                gtk::atk::prelude::AtkObjectExt::set_name(&a, &format!("Move {} later", action.title()));
            }
            {
                let weak = app.weak();
                down_btn.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.move_toolbar_action(action, 1);
                        app.toggle_panel_switch(PanelKind::Settings);
                    }
                });
            }

            item_row.pack_start(&name, true, true, 0);
            item_row.pack_start(&status, false, false, 0);
            item_row.pack_start(&up_btn, false, false, 0);
            item_row.pack_start(&down_btn, false, false, 0);
            order_block.pack_start(&item_row, false, true, 0);
        }
        layout.add(&order_block);

        layout.add(&self.setting_switch(
            app,
            "Vertical tabs (sidebar)",
            "Switch exclusively between top tabs and the vertical tab sidebar. Ctrl S also toggles it.",
            prefs.sidebar && !prefs.column_folded,
            |app, on| app.set_sidebar_visible(on),
        ));

        let tab_width_row = self.stepper_row(
            app,
            "Top tab width",
            "Width of each tab pill in the top tab strip (96 to 280 px).",
            prefs.top_tab_width,
            crate::store::MIN_TOP_TAB_WIDTH,
            crate::store::MAX_TOP_TAB_WIDTH,
            16,
            |app, next| app.set_top_tab_width(next),
        );
        layout.add(&tab_width_row);

        let side_width_row = self.stepper_row(
            app,
            "Sidebar width",
            "Width of the vertical tab sidebar (176 to 440 px). Dragging the divider also updates this.",
            prefs.side_width.round() as i32,
            crate::store::MIN_SIDE_WIDTH as i32,
            crate::store::MAX_SIDE_WIDTH as i32,
            16,
            |app, next| app.set_sidebar_width(next as f64),
        );
        layout.add(&side_width_row);

        let profile_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&profile_row, "panel-setting-row");
        crate::ui::class(&profile_row, "customize-io-row");
        let profile_copy = gtk::Box::new(gtk::Orientation::Vertical, 4);
        crate::ui::class(&profile_copy, "panel-setting-copy");
        profile_copy.set_hexpand(true);
        let profile_title = gtk::Label::new(Some("Appearance & layout profile"));
        crate::ui::class(&profile_title, "setting");
        profile_title.set_xalign(0.0);
        let profile_desc = gtk::Label::new(Some(
            "Export or import only appearance, toolbar, density, and homepage layout preferences as JSON. Cookies, history, bookmarks, passwords, and privacy settings are never included.",
        ));
        crate::ui::class(&profile_desc, "settingdesc");
        profile_desc.set_xalign(0.0);
        profile_desc.set_line_wrap(true);
        profile_desc.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        profile_copy.add(&profile_title);
        profile_copy.add(&profile_desc);
        let profile_actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        profile_actions.set_valign(gtk::Align::Center);
        let export_btn = gtk::Button::with_label("Export…");
        crate::ui::class(&export_btn, "act");
        if let Some(a) = export_btn.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&a, "Export appearance and layout profile");
        }
        {
            let weak = app.weak();
            export_btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    open_export_profile_chooser(&app);
                }
            });
        }
        let import_btn = gtk::Button::with_label("Import…");
        crate::ui::class(&import_btn, "act");
        if let Some(a) = import_btn.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&a, "Import appearance and layout profile");
        }
        {
            let weak = app.weak();
            import_btn.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    open_import_profile_chooser(&app);
                }
            });
        }
        profile_actions.pack_start(&export_btn, false, false, 0);
        profile_actions.pack_start(&import_btn, false, false, 0);
        profile_row.pack_start(&profile_copy, true, true, 0);
        profile_row.pack_start(&profile_actions, false, false, 0);
        layout.add(&profile_row);

        let reset_row = self.link_row(
            app,
            "Reset to minimal layout",
            "Restore minimal defaults: navigation bar, Back, Forward, and Reload on; default order; Home, Reader, Bookmark, Bookmarks bar, sidebar, and roomy density off.",
            "Reset to minimal",
            |app: &AppRef| {
                app.reset_toolbar_layout();
                app.toggle_panel_switch(PanelKind::Settings);
            },
        );
        crate::ui::class(&reset_row, "panel-setting-row");
        layout.add(&reset_row);
        list.add(&layout);

        let spaces_sec = self.section(
            app,
            crate::icons::Icon::Tab,
            "Spaces",
            "Organize tabs into persistent groups inside your shared browser profile (cookies, logins, history, and bookmarks are shared — not security containers; use Private Tabs for isolated sessions).",
        );
        let create_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        crate::ui::class(&create_row, "panel-setting-row");
        crate::ui::class(&create_row, "space-create-row");
        let new_space_entry = gtk::Entry::new();
        crate::ui::class(&new_space_entry, "panel-input");
        new_space_entry.set_hexpand(true);
        new_space_entry.set_max_length(crate::spaces::MAX_SPACE_NAME_LEN as i32);
        new_space_entry.set_placeholder_text(Some("New space name (e.g. Work, Reading)"));
        if let Some(a) = new_space_entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&a, "New space name");
        }
        let add_space_btn = gtk::Button::with_label("Create space");
        crate::ui::class(&add_space_btn, "act");
        crate::ui::class(&add_space_btn, "primary");
        add_space_btn.set_valign(gtk::Align::Center);
        {
            let weak = app.weak();
            let entry_ref = new_space_entry.clone();
            let do_create = std::rc::Rc::new(move || {
                let Some(app) = weak.upgrade() else { return };
                let raw = entry_ref.text().to_string();
                match app.create_space(&raw) {
                    Ok(created) => {
                        entry_ref.set_text("");
                        app.flash_toast(&format!("Created space \"{}\"", created.name));
                        app.toggle_panel_switch(PanelKind::Settings);
                    }
                    Err(msg) => app.flash_toast(msg),
                }
            });
            let c1 = do_create.clone();
            add_space_btn.connect_clicked(move |_| c1());
            let c2 = do_create.clone();
            new_space_entry.connect_activate(move |_| c2());
        }
        create_row.pack_start(&new_space_entry, true, true, 0);
        create_row.pack_start(&add_space_btn, false, false, 0);
        spaces_sec.add(&create_row);

        let (all_spaces, active_space_id, space_counts, default_space_name) = {
            let state = app.state.borrow();
            let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
            for tab in &state.tabs {
                *counts.entry(tab.space_id.clone()).or_insert(0) += 1;
            }
            (
                state.spaces.spaces.clone(),
                state.spaces.active_space.clone(),
                counts,
                state.spaces.default_name().to_string(),
            )
        };
        for space in all_spaces {
            let tab_count = space_counts.get(&space.id).copied().unwrap_or(0);
            let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
            crate::ui::class(&row, "panel-setting-row");
            crate::ui::class(&row, "space-row");

            let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let name_entry = gtk::Entry::new();
            crate::ui::class(&name_entry, "panel-input");
            name_entry.set_hexpand(true);
            name_entry.set_max_length(crate::spaces::MAX_SPACE_NAME_LEN as i32);
            name_entry.set_text(&space.name);
            if let Some(a) = name_entry.accessible() {
                gtk::atk::prelude::AtkObjectExt::set_name(&a, &format!("Rename space {}", space.name));
            }

            let rename_btn = gtk::Button::with_label("Rename");
            crate::ui::class(&rename_btn, "act");
            rename_btn.set_valign(gtk::Align::Center);
            {
                let weak = app.weak();
                let sid = space.id.clone();
                let entry_ref = name_entry.clone();
                let do_rename = std::rc::Rc::new(move || {
                    let Some(app) = weak.upgrade() else { return };
                    match app.rename_space(&sid, &entry_ref.text()) {
                        Ok(name) => {
                            app.flash_toast(&format!("Renamed space to \"{name}\""));
                            app.toggle_panel_switch(PanelKind::Settings);
                        }
                        Err(msg) => app.flash_toast(msg),
                    }
                });
                let r1 = do_rename.clone();
                rename_btn.connect_clicked(move |_| r1());
                let r2 = do_rename.clone();
                name_entry.connect_activate(move |_| r2());
            }

            let switch_btn = gtk::Button::with_label(if space.id == active_space_id { "Active" } else { "Switch" });
            crate::ui::class(&switch_btn, "act");
            switch_btn.set_sensitive(space.id != active_space_id);
            switch_btn.set_valign(gtk::Align::Center);
            {
                let weak = app.weak();
                let sid = space.id.clone();
                switch_btn.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        if app.switch_space(&sid) {
                            app.toggle_panel_switch(PanelKind::Settings);
                        }
                    }
                });
            }

            top.pack_start(&name_entry, true, true, 0);
            top.pack_start(&rename_btn, false, false, 0);
            top.pack_start(&switch_btn, false, false, 0);

            if !space.is_default() {
                let delete_btn = gtk::Button::with_label("Delete");
                crate::ui::class(&delete_btn, "act");
                crate::ui::class(&delete_btn, "danger");
                delete_btn.set_valign(gtk::Align::Center);
                let consequence = if tab_count == 1 {
                    format!("Delete \"{}\" and move its 1 tab to \"{}\"", space.name, default_space_name)
                } else {
                    format!("Delete \"{}\" and move its {} tabs to \"{}\"", space.name, tab_count, default_space_name)
                };
                delete_btn.set_tooltip_text(Some(&consequence));
                if let Some(a) = delete_btn.accessible() {
                    gtk::atk::prelude::AtkObjectExt::set_name(&a, &consequence);
                }
                let weak = app.weak();
                let sid = space.id.clone();
                let sname = space.name.clone();
                let dname = default_space_name.clone();
                delete_btn.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        match app.delete_space(&sid) {
                            Ok(moved) => {
                                let msg = if moved == 0 {
                                    format!("Deleted space \"{sname}\"")
                                } else if moved == 1 {
                                    format!("Deleted \"{sname}\" — moved 1 tab to \"{dname}\"")
                                } else {
                                    format!("Deleted \"{sname}\" — moved {moved} tabs to \"{dname}\"")
                                };
                                app.flash_toast(&msg);
                                app.toggle_panel_switch(PanelKind::Settings);
                            }
                            Err(msg) => app.flash_toast(msg),
                        }
                    }
                });
                top.pack_start(&delete_btn, false, false, 0);
            }
            row.pack_start(&top, false, true, 0);

            let meta_text = if space.is_default() {
                format!(
                    "Default space (cannot be deleted) · {} {}",
                    tab_count,
                    if tab_count == 1 { "tab" } else { "tabs" }
                )
            } else {
                format!(
                    "{} {} · Deleting moves all tabs here into \"{}\"",
                    tab_count,
                    if tab_count == 1 { "tab" } else { "tabs" },
                    default_space_name
                )
            };
            let meta = gtk::Label::new(Some(&meta_text));
            crate::ui::class(&meta, "settingdesc");
            crate::ui::class(&meta, "space-meta");
            meta.set_xalign(0.0);
            meta.set_line_wrap(true);
            row.pack_start(&meta, false, true, 0);
            spaces_sec.add(&row);
        }
        list.add(&spaces_sec);

        let browsing = self.section(app, crate::icons::Icon::Shield, "Browsing", "Blocking, memory use, and the bookmarks bar.");
        browsing.add(&self.setting_switch(app, "Block ads and trackers", "Rules enforced inside the engine, before a request is made.", prefs.shield, |app, on| {
            app.prefs.borrow_mut().shield = on;
            app.prefs.borrow().save();
            app.flash_toast("The shield applies to new pages");
        }));

        browsing.add(&self.setting_switch(app, "Keep hidden things hidden", "What you hide on a site stays hidden there.", prefs.curtain, |app, on| {
            app.prefs.borrow_mut().curtain = on;
            app.prefs.borrow().save();
            crate::curtain::apply_curtain_pref(app);
            app.reload();
        }));

        browsing.add(&self.setting_switch(
            app,
            "Let tabs sleep",
            &format!(
                "After {} minutes away, a tab releases its page and reloads when you return.",
                prefs.sleep_minutes
            ),
            prefs.memory_saver,
            |app, on| {
                app.prefs.borrow_mut().memory_saver = on;
                app.prefs.borrow().save();
                // Turning it off removes the armed timer at once, so the
                // process gets no further wakeups. Turning it on arms it again.
                if on {
                    app.start_memory_timer();
                } else {
                    app.stop_memory_timer();
                }
            },
        ));

        browsing.add(&self.setting_switch(
            app,
            "Show the bookmarks bar",
            "A row of kept pages above the page. It also appears automatically when you save a page.",
            prefs.show_bookmarks_bar,
            |app, on| {
                app.prefs.borrow_mut().show_bookmarks_bar = on;
                app.prefs.borrow().save();
                app.render_bmarks();
            },
        ));

        browsing.add(&self.setting_switch(
            app,
            "Use less memory",
            "Disables GPU compositing on the next launch. Makes WebGL, video, and animated pages slower.",
            prefs.low_memory,
            |app, on| {
                app.prefs.borrow_mut().low_memory = on;
                app.prefs.borrow().save();
                app.flash_toast(if on { "Less memory from the next launch" } else { "Full rendering from the next launch" });
            },
        ));
        let autofill_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&autofill_row, "panel-setting-row");
        let autofill_copy = gtk::Box::new(gtk::Orientation::Vertical, 4);
        crate::ui::class(&autofill_copy, "panel-setting-copy");
        autofill_copy.set_hexpand(true);
        let autofill_title = gtk::Label::new(Some("Passwords and autofill"));
        crate::ui::class(&autofill_title, "setting");
        autofill_title.set_xalign(0.0);
        let autofill_desc = gtk::Label::new(Some(
            "Disabled by design. Rill never saves passwords or autofills forms; use a standalone Linux password manager or paste credentials manually.",
        ));
        crate::ui::class(&autofill_desc, "settingdesc");
        autofill_desc.set_xalign(0.0);
        autofill_desc.set_line_wrap(true);
        autofill_desc.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        autofill_copy.add(&autofill_title);
        autofill_copy.add(&autofill_desc);
        crate::ui::pack(&autofill_row, &autofill_copy, true);
        let autofill_state = gtk::Label::new(Some("Off"));
        crate::ui::class(&autofill_state, "panel-count");
        autofill_state.set_valign(gtk::Align::Center);
        crate::ui::pack(&autofill_row, &autofill_state, false);
        browsing.add(&autofill_row);
        list.add(&browsing);

        let tabs = self.section(app, crate::icons::Icon::Sidebar, "Tabs & pages", "Page colours, tab spacing, and session restore.");
        tabs.add(&self.setting_switch(
            app,
            "Darken pages that stay white",
            "Applies a dark background and light text to all pages, even already-dark sites. Independent of the browser frame; off by default.",
            prefs.darken_pages,
            |app, on| {
                app.prefs.borrow_mut().darken_pages = on;
                app.prefs.borrow().save();
                app.apply_page_tone();
                app.reload();
            },
        ));

        tabs.add(&self.setting_switch(
            app,
            "Roomy tabs & density",
            "Uses taller tab rows, top tabs, and toolbar spacing for easier mouse targets.",
            prefs.roomy_tabs,
            |app, on| {
                app.set_roomy_tabs(on);
                app.toggle_panel_switch(PanelKind::Settings);
            },
        ));

        tabs.add(&self.setting_switch(app, "Open where you left off", "Tabs come back when the browser starts again.", prefs.restore_session, |app, on| {
            app.prefs.borrow_mut().restore_session = on;
            app.prefs.borrow().save();
        }));
        list.add(&tabs);

        let search = self.section(app, crate::icons::Icon::Search, "Search", "Where your words go, and what is sent while you type.");

        let engine_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&engine_row, "panel-setting-row");
        crate::ui::class(&engine_row, "panel-setting-select");
        // No extra margins here: .panel-setting-row already sets 10px of
        // padding above and below. Adding 8px more made this the only row
        // in Settings with doubled spacing.
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 5);
        crate::ui::class(&texts, "panel-setting-copy");
        texts.set_hexpand(true);
        let title = gtk::Label::new(Some("Search engine"));
        crate::ui::class(&title, "setting");
        title.set_xalign(0.0);
        let sub = gtk::Label::new(Some("Used for searches from the address field and start page."));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        texts.add(&title);
        texts.add(&sub);
        engine_row.pack_start(&texts, true, true, 0);
        // Native Cairo-arrow engine picker instead of GtkComboBoxText:
        // GtkComboBoxText loads a themed `pan-down-symbolic` SVG on first paint,
        // spawning external `glycin-image-rs` + `bwrap` processes synchronously
        // (~100ms main-loop stall on cold Settings open + 3 extra processes).
        let engines = crate::store::Prefs::engines();
        let active_name = engines
            .iter()
            .copied()
            .find(|engine| *engine == prefs.search_engine)
            .unwrap_or_else(|| engines.first().copied().unwrap_or("Startpage"));
        let picker = gtk::MenuButton::new();
        crate::ui::class(&picker, "act");
        crate::ui::class(&picker, "engine-picker");
        picker.set_valign(gtk::Align::Center);
        picker.set_can_focus(true);
        if let Some(access) = picker.accessible() {
            use gtk::atk::prelude::AtkObjectExt;
            access.set_role(gtk::atk::Role::ComboBox);
            access.set_name("Search engine");
            if let Some(label_access) = title.accessible() {
                access.add_relationship(gtk::atk::RelationType::LabelledBy, &label_access);
            }
        }
        let picker_inner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let picker_label = gtk::Label::new(Some(active_name));
        picker_label.set_xalign(0.0);
        let picker_arrow = crate::icons::area(crate::icons::Icon::Down, 12, app.palette.borrow().muted);
        picker_arrow.set_valign(gtk::Align::Center);
        picker_inner.pack_start(&picker_label, true, true, 0);
        picker_inner.pack_start(&picker_arrow, false, false, 0);
        picker.add(&picker_inner);

        let popover = gtk::Popover::new(Some(&picker));
        crate::ui::class(&popover, "engine-popover");
        popover.set_modal(true);
        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        menu_box.set_margin_top(4);
        menu_box.set_margin_bottom(4);
        menu_box.set_margin_start(4);
        menu_box.set_margin_end(4);
        let option_buttons: std::rc::Rc<std::cell::RefCell<Vec<(String, gtk::Button)>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        for engine in engines {
            let item = gtk::Button::with_label(engine);
            crate::ui::class(&item, "act");
            if *engine == *active_name {
                crate::ui::class(&item, "chosen");
            }
            item.set_relief(gtk::ReliefStyle::None);
            if let Some(lbl) = item.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                lbl.set_xalign(0.0);
            }
            option_buttons.borrow_mut().push((engine.to_string(), item.clone()));
            let app_weak = app.weak();
            let chosen = engine.to_string();
            let label_ref = picker_label.clone();
            let pop_weak = popover.downgrade();
            let picker_weak = picker.downgrade();
            let options_ref = option_buttons.clone();
            item.connect_clicked(move |_| {
                label_ref.set_text(&chosen);
                for (name, btn) in options_ref.borrow().iter() {
                    let ctx = btn.style_context();
                    if name == &chosen {
                        ctx.add_class("chosen");
                    } else {
                        ctx.remove_class("chosen");
                    }
                }
                if let Some(app) = app_weak.upgrade() {
                    app.prefs.borrow_mut().search_engine = chosen.clone();
                    app.prefs.borrow().save();
                    crate::suggest::stop();
                    app.refresh_open_search_offers();
                    app.render();
                }
                if let Some(pop) = pop_weak.upgrade() {
                    pop.popdown();
                }
                if let Some(btn) = picker_weak.upgrade() {
                    if btn.is_mapped() {
                        btn.grab_focus();
                    }
                }
            });
            menu_box.pack_start(&item, false, true, 0);
        }
        menu_box.show_all();
        popover.add(&menu_box);
        {
            let picker_weak = picker.downgrade();
            popover.connect_closed(move |_| {
                if let Some(btn) = picker_weak.upgrade() {
                    if btn.is_mapped() {
                        btn.grab_focus();
                    }
                }
            });
        }
        *self.active_popover.borrow_mut() = Some(popover.clone());
        picker.set_popover(Some(&popover));
        engine_row.pack_start(&picker, false, false, 0);
        search.add(&engine_row);

        search.add(&self.setting_switch(
            app,
            "Suggestions from your search engine",
            "Sends search queries as you type to your chosen engine from the address field and start page. Never sent in private tabs, or for addresses, emails, paths, secrets, or shortcuts.",
            prefs.engine_suggestions,
            |app, on| {
                app.prefs.borrow_mut().engine_suggestions = on;
                app.prefs.borrow().save();
                if !on {
                    crate::suggest::stop();
                }
                app.refresh_open_search_offers();
            },
        ));
        list.add(&search);

        // Keyword shortcuts belong with the engine: both decide where typed
        // words go. See src/keyword.rs.
        list.add(&self.decorate_section(app, crate::keyword::settings_section(app), crate::icons::Icon::Command, "Keyword shortcuts"));

        // This row is about the page in front of you, not about a tab
        // setting, so it gets a head of its own rather than trailing TABS.
        let page = self.section(app, crate::icons::Icon::Globe, "This page", "Show elements you previously hid on this site.");

        // The same helper the tab column and the curtain use, so the name here
        // is the name the sidebar would show for this tab.
        let hidden = app
            .state
            .borrow()
            .active_tab()
            .map(|tab| crate::shield::host_of(&tab.url))
            .unwrap_or_default();
        let count = if hidden.is_empty() {
            0
        } else {
            app.state.borrow().hidden.get(&hidden).map(|items| items.len()).unwrap_or(0)
        };
        let unhide_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&unhide_row, "panel-setting-row");
        unhide_row.set_margin_top(8);
        unhide_row.set_margin_bottom(8);
        let unhide_text = gtk::Box::new(gtk::Orientation::Vertical, 5);
        crate::ui::class(&unhide_text, "panel-setting-copy");
        unhide_text.set_hexpand(true);
        let unhide_title = gtk::Label::new(Some("Hidden here"));
        crate::ui::class(&unhide_title, "setting");
        unhide_title.set_xalign(0.0);
        let unhide_sub = gtk::Label::new(Some(&hidden_sentence(&hidden, count)));
        crate::ui::class(&unhide_sub, "settingdesc");
        unhide_sub.set_xalign(0.0);
        unhide_sub.set_line_wrap(true);
        unhide_text.add(&unhide_title);
        unhide_text.add(&unhide_sub);
        crate::ui::pack(&unhide_row, &unhide_text, true);
        let show_all = gtk::Button::with_label("Show them");
        crate::ui::class(&show_all, "act");
        show_all.set_valign(gtk::Align::Center);
        // A button that cannot do anything is not offered as if it could.
        show_all.set_sensitive(count > 0);
        {
            let weak = app.weak();
            show_all.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.unhide_all();
                    app.toggle_panel_switch(PanelKind::Settings);
                }
            });
        }
        crate::ui::pack(&unhide_row, &show_all, false);
        page.add(&unhide_row);
        list.add(&page);

        list.add(&self.decorate_section(app, crate::permissions::settings_section(app), crate::icons::Icon::Lock, "Site permissions"));
        let importing = self.section(app, crate::icons::Icon::Download, "Bring your browser with you", "Add bookmarks and history from another browser on this computer.");
        let import_row = crate::import::settings_row(app);
        crate::ui::class(&import_row, "panel-setting-row");
        crate::ui::class(&import_row, "panel-setting-select");
        importing.add(&import_row);
        list.add(&importing);

        let about = gtk::Label::new(Some(&about_label_text()));
        crate::ui::class(&about, "settingdesc");
        crate::ui::class(&about, "panel-about");
        about.set_xalign(0.0);
        about.set_line_wrap(true);
        about.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        about.set_margin_top(2);
        list.add(&about);

        // Settings is intentionally compact in a small window. Its scroller
        // owns the overflow; the capture clips the viewport to the card.
        // A search narrows the form to matching sections and rows. Sections
        // stay visible when a row inside them matches, so the result still
        // reads as Settings rather than a bare list of controls.
        self.settings_holder.add(&self.settings_scroll(&list));
        self.settings_holder.show_all();
        if !query.is_empty() {
            filter_settings_list(&list, &query);
        }
        Some(field)
    }

    /// Every key the browser keeps, in one table. A browser that hides its
    /// keys is a browser whose keys nobody learns.
    fn build_shortcuts(&self) {
        self.clear_body();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        list.set_margin_end(10);
        // The sheet is `keys::SHEET`: the same table the bench presses and the
        // tests check, so what is drawn here cannot advertise a dead key.
        // Sentence case, like every other panel and Settings heading. The
        // section name is drawn as written, so the capitals were in the text.
        for section in ["Browser", "Tabs", "Page"] {
            let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
            crate::ui::class(&group, "panel-section");
            group.set_margin_bottom(16);
            let mut headed = false;
            // One row per action, not one per key. SHEET lists a second
            // spelling as its own row (Ctrl Y and Ctrl H are both History,
            // Ctrl T and Ctrl N are both New tab), so the panel showed the same
            // label twice (Gemini R59 fix 9). Every key still works and is still
            // listed: the keys for one action share a row, joined with "or", in
            // SHEET order. The zoom row already says "Ctrl + or Ctrl =" this way.
            let mut shown: Vec<&'static str> = Vec::new();
            for (_, _, what, ..) in crate::keys::SHEET {
                if shortcut_section(what) != section || shown.contains(what) {
                    continue;
                }
                shown.push(*what);
                let key = crate::keys::SHEET
                    .iter()
                    .filter(|row| row.2 == *what)
                    .map(|row| row.1)
                    .collect::<Vec<_>>()
                    .join(" or ");
                if !headed {
                    headed = true;
                    let head = gtk::Label::new(Some(section));
                    crate::ui::class(&head, "group");
                    crate::ui::class(&head, "panel-section-heading");
                    head.set_xalign(0.0);
                    head.set_margin_top(0);
                    head.set_margin_bottom(8);
                    if let Some(access) = head.accessible() {
                        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
                    }
                    group.add(&head);
                }
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
                crate::ui::class(&row, "panel-shortcut-row");
                let key_col = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                key_col.set_size_request(168, -1);
                key_col.set_valign(gtk::Align::Center);
                let keycap = gtk::Label::new(Some(key.as_str()));
                crate::ui::class(&keycap, "keycap");
                crate::ui::class(&keycap, "panel-shortcut-key");
                keycap.set_xalign(0.0);
                keycap.set_halign(gtk::Align::Start);
                keycap.set_valign(gtk::Align::Center);
                keycap.set_line_wrap(true);
                keycap.set_line_wrap_mode(gtk::pango::WrapMode::Word);
                key_col.pack_start(&keycap, false, false, 0);
                crate::ui::pack(&row, &key_col, false);
                let label = gtk::Label::new(Some(what));
                crate::ui::class(&label, "setting");
                label.set_xalign(0.0);
                label.set_valign(gtk::Align::Center);
                label.set_line_wrap(true);
                label.set_line_wrap_mode(gtk::pango::WrapMode::Word);
                crate::ui::pack(&row, &label, true);
                group.add(&row);
            }
            if headed {
                list.add(&group);
            }
        }
        // 28 rows are taller than the card's tallest allowed height, so the
        // sheet scrolls like every other panel instead of losing its last rows.
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    fn option_picker_row(
        &self,
        app: &AppRef,
        title_text: &str,
        description: &str,
        options: Vec<&'static str>,
        active_name: &str,
        on_choose: impl Fn(&AppRef, &'static str) + 'static,
    ) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&row, "panel-setting-row");
        crate::ui::class(&row, "panel-setting-select");
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 4);
        crate::ui::class(&texts, "panel-setting-copy");
        texts.set_hexpand(true);
        let title = gtk::Label::new(Some(title_text));
        crate::ui::class(&title, "setting");
        title.set_xalign(0.0);
        let sub = gtk::Label::new(Some(description));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        sub.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        texts.add(&title);
        texts.add(&sub);
        row.pack_start(&texts, true, true, 0);

        let picker = gtk::MenuButton::new();
        crate::ui::class(&picker, "act");
        crate::ui::class(&picker, "setting-picker");
        picker.set_valign(gtk::Align::Center);
        picker.set_can_focus(true);
        if let Some(access) = picker.accessible() {
            use gtk::atk::prelude::AtkObjectExt;
            access.set_role(gtk::atk::Role::ComboBox);
            access.set_name(title_text);
        }
        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let lbl = gtk::Label::new(Some(active_name));
        lbl.set_xalign(0.0);
        let arrow = crate::icons::area(crate::icons::Icon::Down, 12, app.palette.borrow().muted);
        arrow.set_valign(gtk::Align::Center);
        inner.pack_start(&lbl, true, true, 0);
        inner.pack_start(&arrow, false, false, 0);
        picker.add(&inner);

        let popover = gtk::Popover::new(Some(&picker));
        crate::ui::class(&popover, "engine-popover");
        popover.set_modal(true);
        let menu_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        menu_box.set_margin_top(4);
        menu_box.set_margin_bottom(4);
        menu_box.set_margin_start(4);
        menu_box.set_margin_end(4);
        let buttons_ref: std::rc::Rc<std::cell::RefCell<Vec<(&'static str, gtk::Button)>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let on_choose = std::rc::Rc::new(on_choose);
        for opt in options {
            let btn = gtk::Button::with_label(opt);
            crate::ui::class(&btn, "act");
            if opt == active_name {
                crate::ui::class(&btn, "chosen");
            }
            btn.set_relief(gtk::ReliefStyle::None);
            if let Some(bl) = btn.child().and_then(|c| c.downcast::<gtk::Label>().ok()) {
                bl.set_xalign(0.0);
            }
            buttons_ref.borrow_mut().push((opt, btn.clone()));
            let app_weak = app.weak();
            let lbl_ref = lbl.clone();
            let pop_weak = popover.downgrade();
            let buttons = buttons_ref.clone();
            let cb = on_choose.clone();
            btn.connect_clicked(move |_| {
                lbl_ref.set_text(opt);
                for (name, b) in buttons.borrow().iter() {
                    let ctx = b.style_context();
                    if *name == opt {
                        ctx.add_class("chosen");
                    } else {
                        ctx.remove_class("chosen");
                    }
                }
                if let Some(app) = app_weak.upgrade() {
                    cb(&app, opt);
                }
                if let Some(pop) = pop_weak.upgrade() {
                    pop.popdown();
                }
            });
            menu_box.pack_start(&btn, false, true, 0);
        }
        menu_box.show_all();
        popover.add(&menu_box);
        {
            let slot = self.active_popover.clone();
            let pop_clone = popover.clone();
            popover.connect_show(move |_| {
                *slot.borrow_mut() = Some(pop_clone.clone());
            });
            let slot_hide = self.active_popover.clone();
            let picker_weak = picker.downgrade();
            popover.connect_closed(move |pop| {
                let mut active = slot_hide.borrow_mut();
                if active.as_ref().is_some_and(|p| p == pop) {
                    *active = None;
                }
                if let Some(btn) = picker_weak.upgrade() {
                    if btn.is_mapped() {
                        btn.grab_focus();
                    }
                }
            });
        }
        picker.set_popover(Some(&popover));
        row.pack_start(&picker, false, false, 0);
        row
    }

    #[allow(clippy::too_many_arguments)]
    fn stepper_row(
        &self,
        app: &AppRef,
        title_text: &str,
        description: &str,
        value: i32,
        min: i32,
        max: i32,
        step: i32,
        on_change: impl Fn(&AppRef, i32) + 'static,
    ) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&row, "panel-setting-row");
        crate::ui::class(&row, "customize-stepper-row");
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 4);
        crate::ui::class(&texts, "panel-setting-copy");
        texts.set_hexpand(true);
        let title = gtk::Label::new(Some(title_text));
        crate::ui::class(&title, "setting");
        title.set_xalign(0.0);
        let sub = gtk::Label::new(Some(description));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        sub.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        texts.add(&title);
        texts.add(&sub);
        row.pack_start(&texts, true, true, 0);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        controls.set_valign(gtk::Align::Center);
        let current = std::rc::Rc::new(std::cell::Cell::new(value.clamp(min, max)));
        let value_lbl = gtk::Label::new(Some(&format!("{} px", current.get())));
        crate::ui::class(&value_lbl, "customize-stepper-value");
        value_lbl.set_width_chars(6);
        value_lbl.set_xalign(0.5);

        let minus = gtk::Button::with_label("\u{2212}");
        crate::ui::class(&minus, "act");
        crate::ui::class(&minus, "customize-stepper-btn");
        minus.set_sensitive(current.get() > min);
        if let Some(a) = minus.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&a, &format!("Decrease {title_text}"));
        }

        let plus = gtk::Button::with_label("+");
        crate::ui::class(&plus, "act");
        crate::ui::class(&plus, "customize-stepper-btn");
        plus.set_sensitive(current.get() < max);
        if let Some(a) = plus.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&a, &format!("Increase {title_text}"));
        }

        let cb = std::rc::Rc::new(on_change);
        {
            let weak = app.weak();
            let cur = current.clone();
            let lbl = value_lbl.clone();
            let minus_ref = minus.clone();
            let plus_ref = plus.clone();
            let cb_ref = cb.clone();
            minus.connect_clicked(move |_| {
                let next = (cur.get() - step).clamp(min, max);
                cur.set(next);
                lbl.set_text(&format!("{next} px"));
                minus_ref.set_sensitive(next > min);
                plus_ref.set_sensitive(next < max);
                if let Some(app) = weak.upgrade() {
                    cb_ref(&app, next);
                }
            });
        }
        {
            let weak = app.weak();
            let cur = current.clone();
            let lbl = value_lbl.clone();
            let minus_ref = minus.clone();
            let plus_ref = plus.clone();
            let cb_ref = cb.clone();
            plus.connect_clicked(move |_| {
                let next = (cur.get() + step).clamp(min, max);
                cur.set(next);
                lbl.set_text(&format!("{next} px"));
                minus_ref.set_sensitive(next > min);
                plus_ref.set_sensitive(next < max);
                if let Some(app) = weak.upgrade() {
                    cb_ref(&app, next);
                }
            });
        }

        controls.pack_start(&minus, false, false, 0);
        controls.pack_start(&value_lbl, false, false, 0);
        controls.pack_start(&plus, false, false, 0);
        row.pack_start(&controls, false, false, 0);
        row
    }

    fn toolbar(&self, text: &str) -> gtk::Box {
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        crate::ui::class(&toolbar, "panel-toolbar");
        toolbar.set_margin_bottom(12);
        let count = gtk::Label::new(Some(text));
        crate::ui::class(&count, "sub");
        crate::ui::class(&count, "panel-count");
        count.set_xalign(0.0);
        count.set_hexpand(true);
        count.set_ellipsize(gtk::pango::EllipsizeMode::End);
        toolbar.pack_start(&count, true, true, 0);
        toolbar
    }

    fn section_header(&self, _app: &AppRef, _icon: crate::icons::Icon, label: &gtk::Label) -> gtk::Box {
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        crate::ui::class(&header, "panel-section-header");
        crate::ui::class(label, "group");
        crate::ui::class(label, "panel-section-heading");
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_line_wrap(true);
        label.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        label.set_margin_top(0);
        label.set_margin_bottom(0);
        if let Some(access) = label.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
        }
        header.pack_start(label, true, true, 0);
        header
    }

    fn section(&self, app: &AppRef, icon: crate::icons::Icon, title: &str, description: &str) -> gtk::Box {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&section, "panel-section");
        section.set_margin_bottom(16);
        if let Some(access) = section.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Grouping);
            gtk::atk::prelude::AtkObjectExt::set_name(&access, title);
        }
        // Sentence case, like every other heading in the browser.
        let label = gtk::Label::new(Some(title));
        section.pack_start(&self.section_header(app, icon, &label), false, true, 0);
        if !description.is_empty() {
            let detail = gtk::Label::new(Some(description));
            crate::ui::class(&detail, "settingdesc");
            crate::ui::class(&detail, "panel-section-description");
            detail.set_xalign(0.0);
            detail.set_line_wrap(true);
            detail.set_line_wrap_mode(gtk::pango::WrapMode::Word);
            detail.set_margin_top(4);
            detail.set_margin_bottom(8);
            section.pack_start(&detail, false, true, 0);
        }
        section
    }

    fn decorate_section(&self, app: &AppRef, section: gtk::Box, icon: crate::icons::Icon, title: &str) -> gtk::Box {
        crate::ui::class(&section, "panel-section");
        section.set_spacing(0);
        section.set_margin_top(0);
        section.set_margin_bottom(16);
        if let Some(access) = section.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Grouping);
            gtk::atk::prelude::AtkObjectExt::set_name(&access, title);
        }
        if let Some(label) = section.children().into_iter().find_map(|child| {
            child.downcast::<gtk::Label>().ok().filter(|label| label.style_context().has_class("group"))
        }) {
            section.remove(&label);
            // Sentence case, the same as the sections built in section() above.
            label.set_text(title);
            let header = self.section_header(app, icon, &label);
            section.pack_start(&header, false, true, 0);
            section.reorder_child(&header, 0);
        }
        let mut description_seen = false;
        for child in section.children() {
            if let Some(label) = child.downcast_ref::<gtk::Label>() {
                if !description_seen && label.style_context().has_class("settingdesc") && !label.is_no_show_all() {
                    crate::ui::class(label, "panel-section-description");
                    label.set_margin_top(4);
                    label.set_margin_bottom(8);
                    description_seen = true;
                }
            }
            if let Some(row) = child.downcast_ref::<gtk::Box>() {
                if row.orientation() == gtk::Orientation::Horizontal && !row.style_context().has_class("panel-section-header") {
                    crate::ui::class(row, "panel-setting-row");
                    for button in row.children().into_iter().filter_map(|widget| widget.downcast::<gtk::Button>().ok()) {
                        button.set_valign(gtk::Align::Center);
                    }
                    let entries = row.children().iter().filter(|widget| widget.is::<gtk::Entry>()).count();
                    if entries > 1 {
                        crate::ui::class(row, "panel-keyword-form");
                        for entry in row.children().into_iter().filter_map(|widget| widget.downcast::<gtk::Entry>().ok()) {
                            entry.set_width_chars(entry.width_chars().min(8));
                        }
                    }
                }
            }
        }
        section
    }

    fn appearance_option(&self, app: &AppRef, look: crate::store::Look, selected: bool) -> gtk::Button {
        let button = gtk::ToggleButton::new();
        crate::ui::class(&button, "act");
        crate::ui::class(&button, "panel-appearance-option");
        button.set_hexpand(true);
        button.set_tooltip_text(Some(match look {
            crate::store::Look::Light => "Use the light appearance",
            crate::store::Look::Dark => "Use the dark appearance",
            crate::store::Look::System => "Follow your desktop's light or dark appearance",
        }));
        if let Some(access) = button.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, look.title());
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::RadioButton);
        }
        if selected {
            button.set_active(true);
            crate::ui::class(&button, "chosen");
            button.set_state_flags(gtk::StateFlags::CHECKED, false);
        }
        let contents = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let preview = gtk::DrawingArea::new();
        crate::ui::class(&preview, "panel-appearance-preview");
        // 36px is enough to tell light from dark at a glance. At 54px the
        // three previews took up the first screen of Settings.
        preview.set_size_request(64, 36);
        preview.set_hexpand(true);
        let frame_look = look.clone();
        preview.connect_draw(move |widget, context| {
            let width = f64::from(widget.allocated_width());
            let height = f64::from(widget.allocated_height());
            if width <= 2.0 || height <= 2.0 {
                return crate::gtkx::inh(false);
            }
            let source = |color: crate::palette::Rgb| {
                context.set_source_rgb(f64::from(color.0) / 255.0, f64::from(color.1) / 255.0, f64::from(color.2) / 255.0);
            };
            let rounded_rect = |cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64| {
                let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
                cr.new_sub_path();
                cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
                cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
                cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
                cr.arc(x + r, y + r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
                cr.close_path();
            };
            let halves = if frame_look == crate::store::Look::System { 2 } else { 1 };
            for index in 0..halves {
                let dark = frame_look == crate::store::Look::Dark || (halves == 2 && index == 1);
                let palette = Palette::for_look(if dark { crate::store::Look::Dark } else { crate::store::Look::Light }, false);
                let left = f64::from(index) * width / f64::from(halves);
                let span = width / f64::from(halves);
                let _ = context.save();
                // Clip to the concentric 4px rounded mini-window frame (12px card - 8px padding).
                rounded_rect(context, 0.5, 0.5, (width - 1.0).max(0.0), (height - 1.0).max(0.0), 4.0);
                context.clip();
                context.rectangle(left, 0.0, span, height);
                context.clip();
                source(palette.ground);
                let _ = context.paint();
                // Mini top tab strip across the top of the preview.
                let bar_h = 8.5;
                source(palette.surface());
                rounded_rect(context, 3.0, 2.0, 22.0, 5.0, 2.0);
                let _ = context.fill();
                source(palette.hover);
                rounded_rect(context, 27.0, 2.0, 16.0, 5.0, 2.0);
                let _ = context.fill();
                // Mini rounded viewport frame inset inside the ground shell.
                let vp_x = 3.0;
                let vp_y = bar_h;
                let vp_w = (width - 6.0).max(8.0);
                let vp_h = (height - bar_h - 3.0).max(8.0);
                let _ = context.save();
                rounded_rect(context, vp_x, vp_y, vp_w, vp_h, 3.0);
                source(palette.card);
                let _ = context.fill_preserve();
                context.clip();
                // Mini Bayer-dithered rising horizon in the rounded viewport.
                let (mid, core) = crate::startpage::dither_stops(&palette, false);
                let mut py = (vp_y + vp_h * 0.28).floor() as i32;
                while f64::from(py) < vp_y + vp_h {
                    let ny = (f64::from(py) - vp_y + 1.0) / vp_h;
                    let mut px = vp_x.floor() as i32;
                    while f64::from(px) < vp_x + vp_w {
                        let nx = (f64::from(px) - vp_x) / vp_w;
                        let level = crate::startpage::bayer_level(
                            (px / 2) as usize,
                            (py / 2) as usize,
                            crate::startpage::horizon_density(nx as f32, ny as f32),
                        );
                        if level != 0 {
                            source(if level == 2 { core } else { mid });
                            context.rectangle(f64::from(px), f64::from(py), 2.0, 2.0);
                            let _ = context.fill();
                        }
                        px += 2;
                    }
                    py += 2;
                }
                // Mini rounded search bar on the clean upper canvas.
                let pill_w = (vp_w * 0.58).max(18.0);
                let pill_x = vp_x + (vp_w - pill_w) * 0.5;
                let pill_y = vp_y + 5.5;
                source(palette.surface());
                rounded_rect(context, pill_x, pill_y, pill_w, 6.0, 2.5);
                let _ = context.fill_preserve();
                source(palette.hairline);
                context.set_line_width(0.8);
                let _ = context.stroke();
                source(palette.accent());
                rounded_rect(context, pill_x + pill_w - 5.2, pill_y + 1.1, 3.8, 3.8, 1.4);
                let _ = context.fill();
                let _ = context.restore();
                // Viewport hairline border.
                source(palette.hairline);
                context.set_line_width(0.8);
                rounded_rect(context, vp_x, vp_y, vp_w, vp_h, 3.0);
                let _ = context.stroke();
                // Outer preview frame border.
                source(palette.hairline);
                context.set_line_width(1.0);
                rounded_rect(context, 0.5, 0.5, (width - 1.0).max(0.0), (height - 1.0).max(0.0), 4.0);
                let _ = context.stroke();
                let _ = context.restore();
            }
            crate::gtkx::inh(false)
        });
        contents.pack_start(&preview, false, true, 0);
        let caption = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = gtk::Label::new(Some(look.title()));
        crate::ui::class(&label, "panel-appearance-label");
        label.set_xalign(0.0);
        caption.pack_start(&label, true, true, 0);
        // The chosen look's check uses the accent, the same colour as the chosen
        // card's border and underline (src/css.rs:273). In muted grey it read
        // as a disabled glyph next to an accent frame (Gemini R60 polish).
        let check = crate::icons::area(crate::icons::Icon::Check, 14, app.palette.borrow().accent());
        check.set_opacity(if selected { 1.0 } else { 0.0 });
        check.set_valign(gtk::Align::Center);
        caption.pack_start(&check, false, false, 0);
        contents.pack_start(&caption, false, true, 0);
        button.add(&contents);
        let app_weak = app.weak();
        button.connect_clicked(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.switch_look(look.clone());
            }
        });
        button.upcast()
    }

    /// Native Cairo-drawn 40x24 pill toggle switch backed by a `ToggleButton`.
    ///
    /// Replaces `gtk::Switch`, whose Adwaita theme loads symbolic SVG state
    /// glyphs via `gdk-pixbuf` -> external `glycin-image-rs` + `bwrap`
    /// subprocesses synchronously on first paint (~80-100ms main-loop stall on
    /// cold Settings open plus 3 extra persistent processes).
    pub(crate) fn pill_switch(app: &AppRef, value: bool, sensitive: bool) -> gtk::ToggleButton {
        let button = gtk::ToggleButton::new();
        crate::ui::class(&button, "mark");
        crate::ui::class(&button, "pill-switch");
        button.set_relief(gtk::ReliefStyle::None);
        button.set_can_focus(true);
        button.set_valign(gtk::Align::Center);
        button.set_active(value);
        button.set_sensitive(sensitive);

        let palette = app.palette.borrow();
        let dark = palette.is_dark();
        let off_track = crate::css::quiet::pick(crate::css::quiet::SWITCH_OFF, dark);
        let on_track = palette.accent();
        let off_knob = palette.surface();
        let on_knob = palette.accent_ink();
        drop(palette);

        let canvas = gtk::DrawingArea::new();
        canvas.set_size_request(40, 24);
        let btn_weak = button.downgrade();
        canvas.connect_draw(move |widget, cr| {
            let active = btn_weak.upgrade().is_some_and(|b| b.is_active());
            let w = f64::from(widget.allocated_width()).max(40.0);
            let h = f64::from(widget.allocated_height()).max(24.0);
            let x0 = ((w - 40.0) / 2.0).max(0.0);
            let y0 = ((h - 24.0) / 2.0).max(0.0);
            let r = 12.0;

            let track = if active { on_track } else { off_track };
            cr.new_sub_path();
            cr.arc(x0 + 40.0 - r, y0 + r, r, -std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2);
            cr.arc(x0 + r, y0 + r, r, std::f64::consts::FRAC_PI_2, 3.0 * std::f64::consts::FRAC_PI_2);
            cr.close_path();
            cr.set_source_rgb(
                f64::from(track.0) / 255.0,
                f64::from(track.1) / 255.0,
                f64::from(track.2) / 255.0,
            );
            let _ = cr.fill();

            let knob_x = if active { x0 + 28.0 } else { x0 + 12.0 };
            let knob_y = y0 + 12.0;
            // Subtle shadow under the circular knob matching `.panel switch slider`.
            cr.arc(knob_x, knob_y + 1.0, 9.0, 0.0, 2.0 * std::f64::consts::PI);
            cr.set_source_rgba(0.0, 0.0, 0.0, 0.18);
            let _ = cr.fill();

            let knob = if active { on_knob } else { off_knob };
            cr.arc(knob_x, knob_y, 9.0, 0.0, 2.0 * std::f64::consts::PI);
            cr.set_source_rgb(
                f64::from(knob.0) / 255.0,
                f64::from(knob.1) / 255.0,
                f64::from(knob.2) / 255.0,
            );
            let _ = cr.fill();
            crate::gtkx::inh(false)
        });
        let canvas_for_toggle = canvas.clone();
        button.connect_toggled(move |_| {
            canvas_for_toggle.queue_draw();
        });
        button.add(&canvas);
        button
    }

    fn setting_switch(
        &self,
        app: &AppRef,
        title: &str,
        subtitle: &str,
        value: bool,
        changed: impl Fn(&AppRef, bool) + 'static,
    ) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&row, "panel-setting-row");
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 5);
        crate::ui::class(&texts, "panel-setting-copy");
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(title));
        crate::ui::class(&name, "setting");
        name.set_xalign(0.0);
        name.set_line_wrap(true);
        name.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        let sub = gtk::Label::new(Some(subtitle));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        sub.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        texts.add(&name);
        texts.add(&sub);
        crate::ui::pack(&row, &texts, true);
        let switch = Self::pill_switch(app, value, true);
        switch.set_tooltip_text(Some(title));
        if let Some(access) = switch.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::ToggleButton);
            gtk::atk::prelude::AtkObjectExt::set_name(&access, title);
            gtk::atk::prelude::AtkObjectExt::set_description(&access, subtitle);
        }
        let app_weak = app.weak();
        let changed = std::rc::Rc::new(changed);
        switch.connect_toggled(move |switch| {
            if let Some(app) = app_weak.upgrade() {
                changed(&app, switch.is_active());
            }
        });
        crate::ui::pack(&row, &switch, false);
        row
    }

    fn empty_state(
        &self,
        app: &AppRef,
        icon: crate::icons::Icon,
        title: &str,
        description: &str,
        tip: Option<(&str, &str)>,
    ) -> gtk::Box {
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 8);
        crate::ui::class(&empty, "panel-empty");
        empty.set_margin_top(20);
        empty.set_margin_bottom(20);
        let mark = crate::icons::area(icon, 24, app.palette.borrow().muted);
        crate::ui::class(&mark, "panel-empty-icon");
        mark.set_halign(gtk::Align::Start);
        mark.set_margin_bottom(4);
        empty.pack_start(&mark, false, false, 0);
        let heading = gtk::Label::new(Some(title));
        crate::ui::class(&heading, "panel-empty-title");
        heading.set_xalign(0.0);
        heading.set_line_wrap(true);
        heading.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        empty.pack_start(&heading, false, true, 0);
        let detail = gtk::Label::new(Some(description));
        crate::ui::class(&detail, "settingdesc");
        crate::ui::class(&detail, "panel-empty-description");
        detail.set_xalign(0.0);
        detail.set_line_wrap(true);
        detail.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        empty.pack_start(&detail, false, true, 0);
        if let Some((key, text)) = tip {
            let hint = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            crate::ui::class(&hint, "panel-empty-tip");
            hint.set_margin_top(8);
            let keycap = gtk::Label::new(Some(key));
            crate::ui::class(&keycap, "keycap");
            keycap.set_valign(gtk::Align::Start);
            hint.pack_start(&keycap, false, false, 0);
            let action = gtk::Label::new(Some(text));
            crate::ui::class(&action, "settingdesc");
            action.set_xalign(0.0);
            action.set_line_wrap(true);
            action.set_line_wrap_mode(gtk::pango::WrapMode::Word);
            hint.pack_start(&action, true, true, 0);
            empty.pack_start(&hint, false, true, 0);
        }
        empty
    }

    fn link_row(
        &self,
        app: &AppRef,
        title: &str,
        subtitle: &str,
        action: &str,
        run: impl Fn(&AppRef) + 'static,
    ) -> gtk::EventBox {
        self.row_with(app, title, subtitle, action, run, None)
    }

    /// A row with a second, quieter action at the end — "Remove" beside
    /// "Open". A list you can only add to is not a list anyone keeps.
    #[allow(clippy::type_complexity)]
    fn row_with(
        &self,
        app: &AppRef,
        title: &str,
        subtitle: &str,
        action: &str,
        run: impl Fn(&AppRef) + 'static,
        second: Option<(&str, Box<dyn Fn(&AppRef)>)>,
    ) -> gtk::EventBox {
        let row = gtk::EventBox::new();
        crate::ui::class(&row, "ptrow");
        crate::ui::class(&row, "panel-list-row");
        row.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
        row.set_can_focus(true);
        row.set_tooltip_text(Some(&if subtitle.is_empty() { title.to_string() } else { format!("{title} — {subtitle}") }));
        if let Some(access) = row.accessible() {
            let name = if subtitle.is_empty() {
                title.to_string()
            } else {
                format!("{title} — {subtitle}")
            };
            gtk::atk::prelude::AtkObjectExt::set_name(&access, &name);
        }
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&hbox, "panel-row-layout");
        hbox.set_margin_top(2);
        hbox.set_margin_bottom(2);
        let identity = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        crate::ui::class(&identity, "panel-row-identity");
        identity.set_hexpand(true);

        let letter = gtk::Label::new(Some(
            &title.chars().next().unwrap_or('\u{2022}').to_uppercase().to_string(),
        ));
        crate::ui::class(&letter, "tileletter");
        let tile = gtk::EventBox::new();
        crate::ui::class(&tile, "tile");
        crate::ui::class(&tile, "panel-row-icon");
        tile.set_size_request(32, 32);
        tile.set_valign(gtk::Align::Center);
        tile.add(&letter);
        crate::ui::pack(&identity, &tile, false);

        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(title));
        crate::ui::class(&name, "ptitle");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let url = gtk::Label::new(Some(subtitle));
        crate::ui::class(&url, "purl");
        url.set_xalign(0.0);
        if self.open.as_ref() == Some(&PanelKind::Extensions) {
            url.set_line_wrap(true);
            url.set_line_wrap_mode(gtk::pango::WrapMode::Word);
        } else {
            url.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        }
        url.set_tooltip_text(Some(subtitle));
        texts.add(&name);
        texts.add(&url);
        crate::ui::pack(&identity, &texts, true);
        crate::ui::pack(&hbox, &identity, true);

        let button = gtk::Button::with_label(action);
        crate::ui::class(&button, "act");
        button.set_valign(gtk::Align::Center);
        // One Tab stop per row (Gemini R60 polish): the row itself takes focus
        // and Return runs this action, so the button stays clickable but is
        // left out of the Tab order. A list of 20 rows used to take 40-60 Tabs.
        button.set_can_focus(false);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        crate::ui::class(&actions, "panel-row-actions");
        actions.set_halign(gtk::Align::End);
        crate::ui::pack(&actions, &button, false);
        // The second action is shared by its button and by Delete on the focused
        // row, so the keyboard keeps a way to reach it now that the button is
        // out of the Tab order.
        let mut second_run: Option<std::rc::Rc<Box<dyn Fn(&AppRef)>>> = None;
        if let Some((label, run_second)) = second {
            let run_second = std::rc::Rc::new(run_second);
            second_run = Some(run_second.clone());
            let other = gtk::Button::with_label(label);
            crate::ui::class(&other, "act");
            crate::ui::class(&other, "quiet");
            other.set_valign(gtk::Align::Center);
            other.set_can_focus(false);
            other.set_tooltip_text(Some(&format!("{label} · Delete")));
            crate::ui::pack(&actions, &other, false);
            let app_weak = app.weak();
            other.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    run_second(&app);
                }
            });
        }
        crate::ui::pack(&hbox, &actions, false);

        row.add(&hbox);

        // The whole row runs the row's action: a row that lights up under the
        // cursor and ignores the click is worse than a plain row.
        {
            let app_weak = app.weak();
            let run = std::rc::Rc::new(run);
            let run_row = run.clone();
            let app_key = app.weak();
            let second_key = second_run.clone();
            row.connect_key_press_event(move |_, event| {
                let name = event.keyval().name().unwrap_or_default();
                if name == "Return" || name == "KP_Enter" {
                    if let Some(app) = app_key.upgrade() { run_row(&app); }
                    return crate::gtkx::inh(true);
                }
                // Delete runs the quieter second action (Remove), the keyboard
                // path to the button that Tab no longer reaches.
                if name == "Delete" || name == "KP_Delete" {
                    if let (Some(run_second), Some(app)) = (second_key.as_ref(), app_key.upgrade()) {
                        run_second(&app);
                        return crate::gtkx::inh(true);
                    }
                }
                crate::gtkx::inh(false)
            });
            let run_row = run.clone();
            row.connect_button_press_event(move |_, event| {
                if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
                    if button.button() == 1 {
                        if let Some(app) = app_weak.upgrade() {
                            run_row(&app);
                        }
                        return crate::gtkx::inh(true);
                    }
                }
                crate::gtkx::inh(false)
            });
            let app_weak = app.weak();
            button.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    run(&app);
                }
            });
        }
        row
    }
}

/// Refresh the list without throwing away a search typed into Downloads.
pub fn refresh_downloads(app: &AppRef) {
    if app.panels.borrow().open.as_ref() != Some(&PanelKind::Downloads) {
        return;
    }
    app.panels.borrow_mut().refresh_downloads_open(app);
}

/// Install a local extension from a typed path or a native chooser.
/// The chooser must never start navigation or overwrite the current page.
fn install_extension_path(app: &AppRef, path: &std::path::Path) {
    match crate::extensions::install(path) {
        Ok(entry) => {
            let message = if entry.runs {
                format!("{} installed", entry.name)
            } else {
                format!("{} installed, but {}", entry.name, entry.why_not.replace('\n', " "))
            };
            app.rebuild_every_view();
            app.flash_toast(&message);
            app.toggle_panel_switch(PanelKind::Extensions);
        }
        Err(why) => app.flash_toast(&format!("Could not install: {why}")),
    }
}

/// What the "Hidden here" row says. A count of nothing is said plainly rather
/// than as "0 things hidden", which reads like a number nobody needs.
fn hidden_sentence(host: &str, count: usize) -> String {
    if host.is_empty() {
        return "Nothing to hide yet: this tab has no page of its own".to_string();
    }
    match count {
        0 => format!("Nothing hidden on {host}"),
        1 => format!("1 thing hidden on {host}"),
        n => format!("{n} things hidden on {host}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_is_told_in_words() {
        assert_eq!(hidden_sentence("example.com", 0), "Nothing hidden on example.com");
        assert_eq!(hidden_sentence("example.com", 1), "1 thing hidden on example.com");
        assert_eq!(hidden_sentence("example.com", 4), "4 things hidden on example.com");
    }

    #[test]
    fn a_tab_with_no_page_says_so() {
        assert_eq!(hidden_sentence("", 3), "Nothing to hide yet: this tab has no page of its own");
    }

    #[test]
    fn card_width_clamps_to_narrow_page_and_caps_at_880() {
        assert_eq!(panel_card_width(1268), 880);
        assert_eq!(panel_card_width(904), 880);
        assert_eq!(panel_card_width(704), 680);
        assert_eq!(panel_card_width(568), 544);
        assert_eq!(panel_card_width(240), 216);
        assert_eq!(panel_card_width(24), 1);
        assert_eq!(panel_card_width(0), 1);
    }

    #[test]
    fn shortcuts_group_under_expected_sections() {
        let find_what = |verb: &str| {
            crate::keys::SHEET
                .iter()
                .find(|row| row.0 == verb)
                .map(|row| row.2)
                .unwrap_or_else(|| panic!("missing verb {verb}"))
        };
        assert_eq!(shortcut_section(find_what("sitecard")), "Page");
        assert_eq!(shortcut_section(find_what("print")), "Page");
        assert_eq!(shortcut_section(find_what("inspector")), "Page");
        assert_eq!(shortcut_section(find_what("inspectorctrl")), "Page");
        assert_eq!(shortcut_section(find_what("zoomin")), "Page");
        assert_eq!(shortcut_section(find_what("escape")), "Browser");
        assert_eq!(shortcut_section(find_what("panelleft")), "Browser");
        assert_eq!(shortcut_section(find_what("panelright")), "Browser");
        assert_eq!(shortcut_section(find_what("panelfocus")), "Browser");
        assert_eq!(shortcut_section(find_what("pagedown")), "Tabs");
        assert_eq!(shortcut_section(find_what("pageup")), "Tabs");
        for (_, _, what, ..) in crate::keys::SHEET {
            assert!(
                matches!(shortcut_section(what), "Browser" | "Tabs" | "Page"),
                "unexpected section for {what}"
            );
        }
    }
}

/// What a download row says under its name.
///
/// A file name and a path is not progress. "12.4 of 40 MB" is, and when the
/// server never declared a length the honest thing is the byte count alone
/// rather than a percentage of nothing.
fn download_subtitle(download: &crate::app::DownloadRec) -> String {
    if download.failed {
        return format!("Failed · {}", download.path);
    }
    if download.done {
        let size = bytes(download.got.max(download.total));
        return if size.is_empty() {
            download.path.clone()
        } else {
            format!("{} · {}", size, download.path)
        };
    }
    let got = bytes(download.got);
    let total = bytes(download.total);
    match (got.is_empty(), total.is_empty()) {
        (true, _) => format!("Downloading to {}", download.path),
        (false, true) => format!("{got} so far"),
        (false, false) => format!("{got} of {total}"),
    }
}

/// Bytes in the units a person reads. Empty for zero, so a caller can tell
/// "no size known" from "no bytes yet".
fn bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let n = n as f64;
    if n <= 0.0 {
        String::new()
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.0} KB", n / KB)
    } else {
        format!("{n:.0} B")
    }
}

fn open_export_profile_chooser(app: &AppRef) {
    let dialog = gtk::FileChooserNative::new(
        Some("Export appearance & layout profile"),
        Some(&app.ui.window),
        gtk::FileChooserAction::Save,
        Some("Export"),
        Some("Cancel"),
    );
    dialog.set_do_overwrite_confirmation(true);
    dialog.set_current_name("rill-layout-profile.json");
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("JSON profiles"));
    filter.add_pattern("*.json");
    dialog.add_filter(filter);
    let weak = app.weak();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let (Some(app), Some(path)) = (weak.upgrade(), dialog.filename()) {
                let payload = {
                    let prefs = app.prefs.borrow();
                    crate::customize::export_prefs_json(&prefs)
                };
                match payload {
                    Ok(json) => match std::fs::write(&path, json.as_bytes()) {
                        Ok(()) => app.flash_toast("Exported appearance & layout profile"),
                        Err(_) => app.flash_toast("Could not write layout profile file"),
                    },
                    Err(_) => app.flash_toast("Could not encode layout profile"),
                }
            }
        }
        dialog.destroy();
        PROFILE_CHOOSERS.with(|c| c.borrow_mut().retain(|kept| kept != dialog));
    });
    PROFILE_CHOOSERS.with(|c| c.borrow_mut().push(dialog.clone()));
    dialog.show();
}

fn open_import_profile_chooser(app: &AppRef) {
    let dialog = gtk::FileChooserNative::new(
        Some("Import appearance & layout profile"),
        Some(&app.ui.window),
        gtk::FileChooserAction::Open,
        Some("Import"),
        Some("Cancel"),
    );
    dialog.set_select_multiple(false);
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("JSON profiles"));
    filter.add_pattern("*.json");
    dialog.add_filter(filter);
    let weak = app.weak();
    dialog.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept {
            if let (Some(app), Some(path)) = (weak.upgrade(), dialog.filename()) {
                let read_result = std::fs::File::open(&path).and_then(|file| {
                    use std::io::Read;
                    let mut buf = String::new();
                    file.take((crate::customize::MAX_IMPORT_BYTES + 1) as u64)
                        .read_to_string(&mut buf)?;
                    Ok(buf)
                });
                match read_result {
                    Ok(raw) => match app.import_appearance_profile_str(&raw) {
                        Ok(()) => {
                            app.flash_toast("Imported appearance & layout profile");
                            app.toggle_panel_switch(PanelKind::Settings);
                        }
                        Err(err) => app.flash_toast(&err.message()),
                    },
                    Err(_) => app.flash_toast("Could not read layout profile file"),
                }
            }
        }
        dialog.destroy();
        PROFILE_CHOOSERS.with(|c| c.borrow_mut().retain(|kept| kept != dialog));
    });
    PROFILE_CHOOSERS.with(|c| c.borrow_mut().push(dialog.clone()));
    dialog.show();
}

fn about_label_text() -> String {
    format!(
        "{} {} · WebKitGTK · Built with Rust",
        crate::app::APP_NAME,
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod download_words {
    use super::{bytes, download_subtitle};
    use crate::app::DownloadRec;

    fn rec(got: u64, total: u64, done: bool, failed: bool) -> DownloadRec {
        DownloadRec {
            id: 0,
            name: "file.pdf".into(),
            path: "/home/you/Downloads/file.pdf".into(),
            done,
            failed,
            got,
            total,
            private: false,
        }
    }

    #[test]
    fn a_running_download_says_how_far_along_it_is() {
        let sub = download_subtitle(&rec(13_000_000, 41_943_040, false, false));
        assert!(sub.contains(" of "), "{sub}");
        assert!(sub.contains("40.0 MB"), "{sub}");
    }

    /// A server that sends no Content-Length still gets a truthful line, not
    /// "0%" of a total nobody knows.
    #[test]
    fn without_a_length_it_counts_instead_of_guessing() {
        let sub = download_subtitle(&rec(2_097_152, 0, false, false));
        assert_eq!(sub, "2.0 MB so far");
    }

    #[test]
    fn a_failed_download_says_failed() {
        assert!(download_subtitle(&rec(0, 100, false, true)).starts_with("Failed"));
    }

    #[test]
    fn a_finished_download_shows_its_size_and_where_it_is() {
        let sub = download_subtitle(&rec(41_943_040, 41_943_040, true, false));
        assert!(sub.starts_with("40.0 MB · "), "{sub}");
        assert!(sub.ends_with("file.pdf"));
    }

    #[test]
    fn the_units_are_the_ones_people_read() {
        assert_eq!(bytes(0), "");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(2048), "2 KB");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn non_running_extensions_never_show_an_active_or_enabled_switch() {
        assert_eq!(super::extension_switch_state(false, true), (false, false));
        assert_eq!(super::extension_switch_state(false, false), (false, false));
        assert_eq!(super::extension_switch_state(true, true), (true, true));
        assert_eq!(super::extension_switch_state(true, false), (false, true));
    }

    #[test]
    fn wrap_policy_uses_fast_word_wrap_for_prose_and_bounded_word_char_for_long_tokens() {
        assert_eq!(
            super::wrap_policy("Applies a dark background and light text to all pages."),
            (gtk::pango::WrapMode::Word, None)
        );
        assert_eq!(
            super::wrap_policy("Hidden on very-long-unbroken-subdomain.example-host.invalid"),
            (gtk::pango::WrapMode::WordChar, Some(12))
        );
    }

    #[test]
    fn about_line_uses_rill_brand_and_package_version() {
        let line = super::about_label_text();
        assert!(line.starts_with("Rill "), "{line}");
        assert!(line.contains("WebKitGTK"), "{line}");
        assert!(!line.starts_with("Search "), "{line}");
    }
}
