// The four panels that hold everything else: what you kept, where you went,
// what you brought down, and how the browser is set up.
use crate::app::AppRef;
use crate::palette::Palette;
use crate::store::{ago, day_of, now_secs, Records};
use gtk::prelude::*;

/// True for a section heading such as "Tabs" or "Browsing": a label with the
/// `group` style class. Every panel heading is built that way (Settings,
/// History days, keywords, site permissions). `snap_to_row` uses this so a
/// scrolling card never ends just below a heading with none of its rows.
fn is_heading(widget: &gtk::Widget) -> bool {
    let context = widget.style_context();
    context.has_class("group")
        || context.has_class("panel-section-header")
        || context.has_class("panel-section-description")
}

const INSET: i32 = 28;
const COMPACT_INSET: i32 = 18;
const RAIL_WIDTH: i32 = 176;
const MAX_HEIGHT: i32 = 640;

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
    /// The card width the body was last measured at. A resize changes the width
    /// a wrapping line needs, so the height has to be asked again.
    fitted_width: std::cell::Cell<i32>,
    /// A permanent home for the Settings body. Other panels clear `body` on
    /// open; keeping Settings here avoids jumping the scroller while typing.
    settings_holder: gtk::Box,
}

/// The Settings query hides widgets in place. Rebuilding the panel on each
/// keystroke destroyed the entry while its changed callback was still running.
/// Settings owns a search entry as the first child of its list. Filtering
/// must leave that entry visible and keep a section header with any row that
/// matches, or the result reads as an orphaned subtitle.
fn filter_settings_list(list: &gtk::Box, query: &str) {
    let mut sections = list.children().into_iter();
    sections.next(); // the search field, never filtered
    for section in sections {
        section.show_all();
        let children = child_widgets(&section);
        let found = children
            .iter()
            .any(|child| widget_text(child).to_lowercase().contains(query));
        if !found {
            section.hide();
            continue;
        }
        let mut iter = children.into_iter();
        if let Some(header) = iter.next() {
            header.show();
        }
        for child in iter {
            let visible = widget_text(&child).to_lowercase().contains(query);
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

        let scrim = crate::ui::scrim(palette);

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
        let rail_brand = gtk::Box::new(gtk::Orientation::Vertical, 5);
        rail_brand.set_margin_start(22);
        rail_brand.set_margin_end(16);
        rail_brand.set_margin_top(26);
        rail_brand.set_margin_bottom(26);
        // Only the wordmark. The "Your browser" caption above it said nothing
        // the wordmark and the panel itself don't already say.
        let wordmark = gtk::Label::new(Some("Search"));
        crate::ui::class(&wordmark, "panel-rail-brand");
        wordmark.set_xalign(0.0);
        rail_brand.pack_start(&wordmark, false, true, 0);
        rail.pack_start(&rail_brand, false, true, 0);

        let nav = gtk::Box::new(gtk::Orientation::Vertical, 4);
        nav.set_margin_start(12);
        nav.set_margin_end(12);
        nav.set_margin_bottom(12);
        if let Some(access) = nav.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::PageTabList);
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Browser panel navigation");
        }
        rail.pack_start(&nav, false, true, 0);

        let rail_hint = gtk::Box::new(gtk::Orientation::Vertical, 5);
        crate::ui::class(&rail_hint, "panel-rail-hint");
        rail_hint.set_margin_start(22);
        rail_hint.set_margin_end(12);
        rail_hint.set_margin_top(18);
        rail_hint.set_margin_bottom(22);
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
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        crate::ui::class(&head, "panel-header");
        head.set_margin_start(INSET);
        head.set_margin_end(INSET);
        head.set_margin_top(28);
        head.set_margin_bottom(24);
        let heading = gtk::Box::new(gtk::Orientation::Vertical, 8);
        heading.set_hexpand(true);
        let title = gtk::Label::new(Some("Bookmarks"));
        crate::ui::class(&title, "panel-heading");
        title.set_xalign(0.0);
        title.set_line_wrap(true);
        title.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        if let Some(access) = title.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
        }
        let description = gtk::Label::new(Some(PanelKind::Bookmarks.description()));
        crate::ui::class(&description, "panel-description");
        description.set_xalign(0.0);
        description.set_line_wrap(true);
        description.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
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
            fitted_width: std::cell::Cell::new(0),
            settings_holder,

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
            let list = self
                .settings_holder
                .children()
                .first()
                .cloned()
                .and_then(|list| list.downcast::<gtk::Box>().ok());
            if let Some(list) = list {
                let entry = list.children().first().cloned().and_then(|child| child.downcast::<gtk::Entry>().ok());
                if let Some(entry) = entry {
                    if entry.text() != text {
                        entry.set_text(text);
                    }
                    entry.set_position(text.chars().count() as i32);
                    entry.grab_focus();
                }
                if query.is_empty() {
                    for section in list.children() {
                        section.show_all();
                        for child in child_widgets(&section) {
                            child.show_all();
                        }
                    }
                } else {
                    // `show_all()` above restores rows hidden by an earlier
                    // query before the new one is applied.
                    list.show_all();
                    filter_settings_list(&list, &query);
                }
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

    fn open_panel_with(&mut self, app: &AppRef, kind: PanelKind) -> Option<gtk::Entry> {
        let prev = self.open.clone();
        let was_open = prev.is_some();
        let switched = prev.as_ref().is_some_and(|p| *p != kind);
        crate::motion::cancel(&self.layer.root);
        crate::motion::cancel(&self.scrim);
        crate::motion::cancel(&self.card);
        crate::motion::cancel(&self.scroll);
        self.layer.root.set_opacity(1.0);
        self.scrim.set_opacity(1.0);
        self.card.set_opacity(1.0);
        self.scroll.set_opacity(1.0);

        self.open = Some(kind.clone());
        self.title.set_text(kind.title());
        self.description.set_text(kind.description());
        if let Some(access) = self.card.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, kind.title());
        }
        self.build_nav(app, &kind);
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
        let band = 0;
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
        self.queue_refit();
        if !was_open || switched {
            self.scroll.vadjustment().set_value(0.0);
        }
        if !was_open {
            // The scrim fades on the shared entrance role, under the card,
            // so both land on the same frame.
            crate::motion::fade_in(&self.scrim, crate::motion::dur::ENTER);
            if width > 1 && height > 1 {
                let card_height = self.card_height(card_width, height);
                let x = column + ((page - card_width) / 2).max(12);
                let y = ((height - card_height) / 2).max(band + 4);
                // First open rises the shared 8px on the entrance role, and the
                // 130ms close fade mirrors it.
                crate::motion::slide_in_fixed(
                    &self.layer.root,
                    &self.card,
                    x,
                    y,
                    crate::motion::dur::TRAVEL,
                    crate::motion::dur::ENTER,
                );
            } else {
                crate::motion::fade_in(&self.card, crate::motion::dur::ENTER);
            }
        } else if switched {
            // Switching panels only swaps the content inside the open card, so
            // the card stays put and the content uses the short MICRO role.
            crate::motion::fade_from(&self.scroll, 0.35, 1.0, crate::motion::dur::MICRO);
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

    /// Rebuild an open Downloads panel in-place without dropping filter text, cursor, or focus.
    pub fn refresh_downloads_open(&mut self, app: &AppRef) {
        if self.open.as_ref() != Some(&PanelKind::Downloads) {
            return;
        }
        let existing = self
            .body
            .children()
            .into_iter()
            .find_map(|child| child.downcast::<gtk::Entry>().ok());
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
        self.layer.place(
            &self.card,
            column + ((page - card_width) / 2).max(12),
            top,
            card_width,
            card_height,
        );
    }

    pub fn close(&mut self, _ui: &crate::ui::Ui) {
        if self.open.is_none() {
            return;
        }
        self.open = None;
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
        for child in self.nav.children() {
            self.nav.remove(&child);
        }
        for kind in PanelKind::all() {
            let button = gtk::ToggleButton::new();
            crate::ui::class(&button, "tab");
            crate::ui::class(&button, "panel-nav-item");
            button.set_relief(gtk::ReliefStyle::None);
            button.set_can_focus(true);
            button.set_focus_on_click(false);
            button.set_tooltip_text(Some(kind.title()));
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            crate::ui::class(&content, "panel-nav-content");
            let icon = crate::icons::area(kind.icon(), 18, app.palette.borrow().muted);
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
            if &kind == current {
                button.set_active(true);
                button.set_state_flags(gtk::StateFlags::CHECKED, false);
                crate::ui::class(&button, "on");
            }
            let app_weak = app.weak();
            let target = kind.clone();
            button.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    app.toggle_panel_switch(target.clone());
                }
            });
            self.nav.pack_start(&button, false, true, 0);
        }
        self.nav.show_all();
    }

    fn configure_layout(&self, card_width: i32, window_height: i32) -> bool {
        let compact = card_width < 700 || (window_height > 1 && window_height < 520);
        let changed = self.compact.replace(compact) != compact;
        if !changed && self.fitted_width.get() == card_width {
            return false;
        }
        let orientation = if compact { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal };
        self.card.set_orientation(orientation);
        self.rail.set_orientation(if compact { gtk::Orientation::Horizontal } else { gtk::Orientation::Vertical });
        self.rail.set_hexpand(compact);
        self.rail.set_size_request(if compact { -1 } else { RAIL_WIDTH }, -1);
        self.card.set_size_request(if compact { 560 } else { 880 }, if compact { 420 } else { 420 });
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
        self.header.set_margin_top(if compact { 20 } else { 28 });
        self.body.set_margin_start(inset);
        self.body.set_margin_end(inset);
        self.body.set_margin_bottom(inset);
        Self::adapt_content(self.body.upcast_ref(), self.content_width(card_width));
        changed
    }

    fn adapt_content(widget: &gtk::Widget, width: i32) {
        let context = widget.style_context();
        let (start, end, _, _) = widget_insets(widget);
        let inner = (width - start - end).max(1);
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if context.has_class("setting") || context.has_class("settingdesc") {
                label.set_line_wrap(true);
                label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
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
                container.set_orientation(if stacked { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal });
                for child in container.children() {
                    if !context.has_class("panel-appearance") && (child.is::<gtk::Button>() || child.is::<gtk::ComboBoxText>()) {
                        child.set_halign(if stacked { gtk::Align::Start } else { gtk::Align::Fill });
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

    fn card_height(&self, card_width: i32, window_height: i32) -> i32 {
        let available = (window_height - self.band.get() - 24).max(1);
        let minimum = self.minimum_height().min(available);
        let height = self.wanted_height.get().min((window_height - 80).max(240)).max(minimum).min(available);
        if height < self.wanted_height.get() {
            self.snap_to_row((self.body_width(card_width) - 18).max(1), height)
                .max(minimum)
                .min(height)
        } else {
            height
        }
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
            panels.wanted_height.set(panels.fit_height(card_width));
            panels.layout_card(width, height);
            panels.scroll.check_resize();
        });
    }

    /// The height the card has to be at this width, from the measure below.
    ///
    /// Ask the widgets above the list how tall they are instead of adding a
    /// round 140 and hoping: the guess was 34px short, which cut the last row of
    /// a list in half and made a scrollable card look broken.
    fn fit_height(&self, card_width: i32) -> i32 {
        Self::adapt_content(self.body.upcast_ref(), self.content_width(card_width));
        let inner = self.body_width(card_width);
        let above = self.chrome_height(card_width);
        let minimum = self.minimum_height().min(MAX_HEIGHT);
        let wanted = self.measure_content(inner) + above;
        if wanted > MAX_HEIGHT {
            let inner_scrolled = (inner - 18).max(1);
            Self::adapt_content(self.body.upcast_ref(), (self.content_width(card_width) - 18).max(1));
            self.fitted_width.set(card_width);
            let capped = (self.measure_content(inner_scrolled) + above).clamp(minimum, MAX_HEIGHT);
            // A list taller than the card used to stop at MAX_HEIGHT exactly,
            // so the bottom edge cut through a row (History, Settings).
            // card_height only snaps when the window shrinks the card further,
            // so snap here too and end the card on a whole row.
            let height = self.snap_to_row(inner_scrolled, capped).max(minimum).min(capped);
            return height;
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

    /// Shortens a scrolling card so its viewport ends on the edge of a row.
    /// The edge is never just below a group heading: the cut goes above the
    /// heading instead, so a section title is shown with at least one row.
    ///
    /// The viewport is the card minus everything outside the scroller (the
    /// title row and the panel tabs). Walk the body's children from the top,
    /// and step into a vertical list box one row at a time. Keep the last edge
    /// that still fits. If no edge reaches past half the viewport (one very
    /// tall child), keep the height as it is, because cutting more would hide
    /// more than it shows.
    fn snap_to_row(&self, width: i32, card_height: i32) -> i32 {
        let chrome = self.chrome_height(self.fitted_width.get().max(1));
        // chrome_height counts the body's own top and bottom margins, because
        // the full-content measure needs them. They are not fixed chrome,
        // though: they sit inside the scroller and scroll with the rows. When
        // they were taken out of the viewport, the snapped card ended
        // body_bottom pixels lower than the row edge, so the next row showed
        // its top (History, Settings, Shortcuts, Extensions). So measure the
        // real scroller viewport here, and place row edges after body_top.
        let (_, _, body_top, body_bottom) = widget_insets(&self.body);
        let viewport = card_height - (chrome - body_top - body_bottom);
        if viewport <= 0 {
            return card_height;
        }
        let mut edges = Vec::new();
        let mut position = body_top;
        for child in self.body_children().into_iter().filter(|child| child.is_visible()) {
            Self::row_edges(&child, width, position, &mut edges);
            position += child.preferred_height_for_width(width).1;
        }
        let edge = edges.into_iter().filter(|edge| *edge <= viewport).max().unwrap_or(0);
        if edge * 2 < viewport {
            return card_height;
        }
        (card_height - (viewport - edge)).max(200)
    }

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
            if children.last().is_some_and(|child| !is_heading(child)) {
                edges.push(origin + total);
            }
        } else if !is_heading(widget) {
            edges.push(origin + total);
        }
    }

    fn body_scroll(&self, content: &gtk::Box) -> gtk::Box {
        // One scroller owns every panel. Returning the content here avoids
        // nesting a second GtkViewport, which breaks measurement and clipping.
        content.clone()
    }

    fn filter_field(&self, placeholder: &str, on_type: impl Fn(&str) + 'static) -> gtk::Entry {
        let entry = gtk::Entry::new();
        crate::ui::class(&entry, "panel-filter");
        entry.set_placeholder_text(Some(placeholder));
        entry.set_icon_from_icon_name(gtk::EntryIconPosition::Primary, Some("edit-find-symbolic"));
        entry.set_icon_activatable(gtk::EntryIconPosition::Primary, false);
        entry.set_width_chars(1);
        entry.set_hexpand(true);
        entry.set_margin_bottom(16);
        if let Some(access) = entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, placeholder);
        }
        let on_type = std::rc::Rc::new(on_type);
        entry.connect_changed(move |entry| on_type(&entry.text()));
        entry
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
        let filter = self.filter_field("Search bookmarks", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::Bookmarks, text);
            }
        });
        self.body.add(&filter);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
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

        let filter = self.filter_field("Search history", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::History, text);
            }
        });
        self.body.add(&filter);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
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
        let filter = self.filter_field("Search downloads", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::Downloads, text);
            }
        });
        self.body.add(&filter);
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
        let installed_section = self.section(app, crate::icons::Icon::Spark, "Installed", "Enable, inspect, or remove your extensions.");

        if installed.is_empty() {
            installed_section.add(&self.empty_state(
                app,
                crate::icons::Icon::Spark,
                "No extensions installed",
                "Add a .crx, an .xpi, or an unpacked extension folder below. Each extension shows its compatibility here.",
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
            let row = self.link_row(app, &entry.name, &subtitle, if entry.enabled { "Off" } else { "On" }, {
                let id = entry.id.clone();
                let on = entry.enabled;
                move |app: &AppRef| {
                    crate::extensions::set_enabled(&id, !on);
                    app.rebuild_every_view();
                    app.toggle_panel_switch(PanelKind::Extensions);
                }
            });
            installed_section.add(&row);
            // An extension that ships an options page gets a way to open it.
            // It is a file on disk, so it opens as a page, which is what it is.
            if let Some(page) = crate::extensions::options_page(&entry) {
                let row = self.link_row(app, &format!("{} options", entry.name), "Opens the page the extension ships", "Options", {
                    let url = format!("file://{page}");
                    move |app: &AppRef| {
                        app.toggle_panel_switch(PanelKind::Extensions);
                        app.new_tab(Some(&url), false, false);
                    }
                });
                installed_section.add(&row);
            }
            let remove = self.link_row(app, &format!("Remove {}", entry.name), &entry.source, "Remove", {
                let id = entry.id.clone();
                move |app: &AppRef| {
                    crate::extensions::remove(&id);
                    app.rebuild_every_view();
                    app.toggle_panel_switch(PanelKind::Extensions);
                }
            });
            installed_section.add(&remove);
        }
        list.add(&installed_section);

        // What is acting on the page in front of you, which is the question a
        // person actually has when they install something.
        let current = app.state.borrow().tab(app.active_id()).map(|tab| tab.url.clone()).unwrap_or_default();
        let acting = if current.is_empty() { Vec::new() } else { crate::extensions::here(&current) };
        if !acting.is_empty() {
            let active_section = self.section(app, crate::icons::Icon::Globe, "On this page", "Extensions acting on the page in front of you.");
            let line = gtk::Label::new(Some(&acting.join(", ")));
            crate::ui::class(&line, "settingdesc");
            line.set_xalign(0.0);
            line.set_line_wrap(true);
            line.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
            active_section.add(&line);
            list.add(&active_section);
        }

        let installation = self.section(app, crate::icons::Icon::Plus, "Add an extension", "Browse a store or install an archive or folder from this computer.");

        // Firefox's store, one click away. It opens in a tab like any other
        // page. An .xpi fetched from it lands in the download hook in web.rs,
        // which hands it to the same installer as a file chosen below, so an
        // add-on from the store and one from disk get the same checks.
        let firefox = self.link_row(
            app,
            "Firefox Add-ons",
            "addons.mozilla.org, in a new tab. Add to Firefox installs it here.",
            "Open",
            move |app: &AppRef| {
                app.toggle_panel_switch(PanelKind::Extensions);
                app.new_tab(Some("https://addons.mozilla.org/firefox/extensions/"), false, true);
            },
        );
        installation.add(&firefox);

        let field = gtk::Entry::new();
        field.set_placeholder_text(Some("A .crx, an .xpi, or a folder with a manifest.json"));
        field.set_width_chars(1);
        // The field fills the install row and the chooser sits at its end
        // (see install_row below). The row's own margins replace the 12px
        // the field used to carry by itself.
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
                Some("Install an extension"),
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
        // Both ways to install from disk share one row: type a path, or pick
        // one. When the chooser had its own row under the field, a panel
        // already at MAX_HEIGHT snapped to a whole row that ended on the
        // field, so the main action opened below the fold (Gemini R60 fix 5,
        // council/round-60/05-panel-extensions.png). One row also means one
        // row edge for snap_to_row: a horizontal box is a leaf there. The
        // note under it stays secondary text you can scroll to.
        let install_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        install_row.set_margin_top(12);
        install_row.set_margin_bottom(12);
        install_row.pack_start(&field, true, true, 0);
        install_row.pack_start(&choose, false, false, 0);
        installation.add(&install_row);
        list.add(&installation);

        let note = gtk::Label::new(Some(
            "Content scripts and styles can run on the sites an extension requests. WebExtensions background-page APIs are not supported. Each installed extension lists the parts that do not run here.",
        ));
        crate::ui::class(&note, "settingdesc");
        crate::ui::class(&note, "panel-note");
        note.set_xalign(0.0);
        note.set_line_wrap(true);
        note.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        // No width of its own: the body's width bounds it, and measure_content
        // asks for its height at exactly that width. Capping max_width_chars
        // here made GTK answer with the height at the cap instead (86px for a
        // sentence that fills 76), which only added slack to the card.
        list.add(&note);

        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    fn build_settings(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_body();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        let prefs = app.prefs.borrow().clone();
        let query = self.filter.borrow().clone();
        let app_weak = std::rc::Rc::<crate::app::App>::downgrade(app);
        let field = self.filter_field("Search settings", move |text| {
            if let Some(app) = app_weak.upgrade() {
                app.filter_panel(crate::panels::PanelKind::Settings, text);
            }
        });
        field.set_text(&query);
        field.set_position(query.chars().count() as i32);
        list.add(&field);

        let appearance = self.section(app, crate::icons::Icon::Spark, "Appearance", "Light, dark, or matched to your desktop.");
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        crate::ui::class(&buttons, "panel-appearance");
        buttons.set_homogeneous(true);
        for look in [crate::store::Look::Light, crate::store::Look::Dark, crate::store::Look::System] {
            buttons.pack_start(&self.appearance_option(app, look.clone(), look == prefs.appearance), true, true, 0);
        }
        appearance.add(&buttons);
        list.add(&appearance);

        let browsing = self.section(app, crate::icons::Icon::Shield, "Browsing", "Blocking, memory use, and the bookmarks bar.");
        browsing.add(&self.setting_switch(app, "Block ads and trackers", "Rules enforced inside the engine, before a request is made.", prefs.shield, |app, on| {
            app.prefs.borrow_mut().shield = on;
            app.prefs.borrow().save();
            app.flash_toast("The shield applies to new pages");
        }));

        browsing.add(&self.setting_switch(app, "Keep hidden things hidden", "What you hide on a site stays hidden there.", prefs.curtain, |app, on| {
            app.prefs.borrow_mut().curtain = on;
            app.prefs.borrow().save();
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

        tabs.add(&self.setting_switch(app, "Roomy tabs", "Taller rows, easier to hit with a mouse.", prefs.roomy_tabs, |app, on| {
            app.prefs.borrow_mut().roomy_tabs = on;
            app.prefs.borrow().save();
            app.apply_style();
            app.render();
            app.toggle_panel_switch(PanelKind::Settings);
        }));

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
        let sub = gtk::Label::new(Some("Used for searches from the address field."));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        texts.add(&title);
        texts.add(&sub);
        engine_row.pack_start(&texts, true, true, 0);
        let combo = gtk::ComboBoxText::new();
        combo.set_valign(gtk::Align::Center);
        if let Some(access) = combo.accessible() {
            use gtk::atk::prelude::AtkObjectExt;
            access.set_name("Search engine");
            if let Some(label_access) = title.accessible() {
                access.add_relationship(gtk::atk::RelationType::LabelledBy, &label_access);
            }
        }
        for engine in crate::store::Prefs::engines() {
            combo.append_text(engine);
        }
        let engines = crate::store::Prefs::engines();
        let index = engines.iter().position(|engine| *engine == prefs.search_engine).unwrap_or(0);
        combo.set_active(Some(index as u32));
        {
            let app_weak = app.weak();
            combo.connect_changed(move |combo| {
                if let Some(app) = app_weak.upgrade() {
                    if let Some(text) = combo.active_text() {
                        app.prefs.borrow_mut().search_engine = text.to_string();
                        app.prefs.borrow().save();
                    }
                }
            });
        }
        engine_row.pack_start(&combo, false, false, 0);
        search.add(&engine_row);

        search.add(&self.setting_switch(app, "Suggestions from your search engine", "What you type is sent to the engine as you type.", prefs.engine_suggestions, |app, on| {
            app.prefs.borrow_mut().engine_suggestions = on;
            app.prefs.borrow().save();
        }));
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

        let about = gtk::Label::new(Some(&format!(
            "Search {} · WebKitGTK · Built with Rust",
            env!("CARGO_PKG_VERSION")
        )));
        crate::ui::class(&about, "settingdesc");
        crate::ui::class(&about, "panel-about");
        about.set_xalign(0.0);
        about.set_line_wrap(true);
        about.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        about.set_margin_top(2);
        list.add(&about);

        // Settings is intentionally compact in a small window. Its scroller
        // owns the overflow; the capture clips the viewport to the card.
        // A search narrows the form to matching sections and rows. Sections
        // stay visible when a row inside them matches, so the result still
        // reads as Settings rather than a bare list of controls.
        if !query.is_empty() {
            filter_settings_list(&list, &query);
        }
        self.settings_holder.add(&self.body_scroll(&list));
        self.settings_holder.show_all();
        Some(field)
    }

    /// Every key the browser keeps, in one table. A browser that hides its
    /// keys is a browser whose keys nobody learns.
    fn build_shortcuts(&self) {
        self.clear_body();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "panel-list");
        // The sheet is `keys::SHEET`: the same table the bench presses and the
        // tests check, so what is drawn here cannot advertise a dead key.
        // Sentence case, like every other panel and Settings heading. The
        // section name is drawn as written, so the capitals were in the text.
        for section in ["Browser", "Tabs", "Page"] {
            let group = gtk::Box::new(gtk::Orientation::Vertical, 0);
            crate::ui::class(&group, "panel-section");
            group.set_margin_bottom(18);
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
                    head.set_margin_bottom(12);
                    if let Some(access) = head.accessible() {
                        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
                    }
                    group.add(&head);
                }
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
                crate::ui::class(&row, "panel-shortcut-row");
                let keycap = gtk::Label::new(Some(key.as_str()));
                crate::ui::class(&keycap, "keycap");
                crate::ui::class(&keycap, "panel-shortcut-key");
                keycap.set_xalign(0.0);
                keycap.set_size_request(142, -1);
                keycap.set_line_wrap(true);
                keycap.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
                crate::ui::pack(&row, &keycap, false);
                let label = gtk::Label::new(Some(what));
                crate::ui::class(&label, "setting");
                label.set_xalign(0.0);
                label.set_line_wrap(true);
                label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
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

    fn toolbar(&self, text: &str) -> gtk::Box {
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        crate::ui::class(&toolbar, "panel-toolbar");
        toolbar.set_margin_bottom(14);
        let count = gtk::Label::new(Some(text));
        crate::ui::class(&count, "sub");
        crate::ui::class(&count, "panel-count");
        count.set_xalign(0.0);
        count.set_hexpand(true);
        count.set_ellipsize(gtk::pango::EllipsizeMode::End);
        toolbar.pack_start(&count, true, true, 0);
        toolbar
    }

    fn section_header(&self, app: &AppRef, icon: crate::icons::Icon, label: &gtk::Label) -> gtk::Box {
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        crate::ui::class(&header, "panel-section-header");
        let mark = crate::icons::area(icon, 16, app.palette.borrow().muted);
        crate::ui::class(&mark, "panel-section-icon");
        mark.set_valign(gtk::Align::Center);
        header.pack_start(&mark, false, false, 0);
        crate::ui::class(label, "group");
        crate::ui::class(label, "panel-section-heading");
        label.set_xalign(0.0);
        label.set_hexpand(true);
        label.set_line_wrap(true);
        label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
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
        section.set_margin_bottom(18);
        if let Some(access) = section.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Grouping);
            gtk::atk::prelude::AtkObjectExt::set_name(&access, title);
        }
        // Sentence case, like every other heading in the browser. The 12px/600
        // label already sets the section apart, so it doesn't need capitals.
        let label = gtk::Label::new(Some(title));
        section.pack_start(&self.section_header(app, icon, &label), false, true, 0);
        if !description.is_empty() {
            let detail = gtk::Label::new(Some(description));
            crate::ui::class(&detail, "settingdesc");
            crate::ui::class(&detail, "panel-section-description");
            detail.set_xalign(0.0);
            detail.set_line_wrap(true);
            detail.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
            detail.set_margin_top(6);
            detail.set_margin_bottom(12);
            section.pack_start(&detail, false, true, 0);
        }
        section
    }

    fn decorate_section(&self, app: &AppRef, section: gtk::Box, icon: crate::icons::Icon, title: &str) -> gtk::Box {
        crate::ui::class(&section, "panel-section");
        section.set_spacing(0);
        section.set_margin_top(0);
        section.set_margin_bottom(18);
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
                    label.set_margin_top(6);
                    label.set_margin_bottom(12);
                    description_seen = true;
                }
            }
            if let Some(row) = child.downcast_ref::<gtk::Box>() {
                if row.orientation() == gtk::Orientation::Horizontal && !row.style_context().has_class("panel-section-header") {
                    crate::ui::class(row, "panel-setting-row");
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
        let contents = gtk::Box::new(gtk::Orientation::Vertical, 10);
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
            let source = |color: crate::palette::Rgb| {
                context.set_source_rgb(f64::from(color.0) / 255.0, f64::from(color.1) / 255.0, f64::from(color.2) / 255.0);
            };
            let halves = if frame_look == crate::store::Look::System { 2 } else { 1 };
            for index in 0..halves {
                let dark = frame_look == crate::store::Look::Dark || (halves == 2 && index == 1);
                let palette = Palette::for_look(if dark { crate::store::Look::Dark } else { crate::store::Look::Light }, false);
                let left = f64::from(index) * width / f64::from(halves);
                let span = width / f64::from(halves);
                let _ = context.save();
                context.rectangle(left, 0.0, span, height);
                context.clip();
                source(palette.card);
                let _ = context.paint();
                source(palette.ground);
                context.rectangle(0.0, 0.0, width * 0.24, height);
                let _ = context.fill();
                source(palette.hairline);
                context.set_line_width(1.0);
                context.move_to(width * 0.24 + 0.5, 0.0);
                context.line_to(width * 0.24 + 0.5, height);
                let _ = context.stroke();
                source(palette.accent());
                context.rectangle(6.0, 12.0, (width * 0.24 - 12.0).max(4.0), 3.0);
                let _ = context.fill();
                // The preview is 36px tall. The old steps drew the last sidebar
                // line at y36 and the last content line at y37, so both were
                // cut off by the frame (round-54 review, Settings). At 5px steps
                // the lines sit at 18/23/28 and 22/27/32, all inside the frame,
                // and the bottom margin (4px or more) is close to the top one.
                source(palette.muted);
                for line in 0..3 {
                    let top = 18.0 + f64::from(line) * 5.0;
                    context.rectangle(6.0, top, (width * 0.24 - 14.0).max(3.0), 1.0);
                    let _ = context.fill();
                }
                source(palette.ink);
                context.rectangle(width * 0.34, 13.0, width * 0.44, 3.0);
                let _ = context.fill();
                source(palette.hairline);
                for line in 0..3 {
                    let top = 22.0 + f64::from(line) * 5.0;
                    context.rectangle(width * 0.34, top, width * if line == 2 { 0.32 } else { 0.53 }, 1.0);
                    let _ = context.fill();
                }
                // Each preview is outlined in its own palette's hairline, drawn
                // while that half's clip is still set. The old frame always used
                // the light hairline, so the Dark preview (and the dark half of
                // System) had a pale ring that no dark window has (Gemini R60
                // polish). Under the clip, System gets a light frame on its light
                // half and a dark frame on its dark half.
                source(palette.hairline);
                context.set_line_width(1.0);
                context.rectangle(0.5, 0.5, (width - 1.0).max(0.0), (height - 1.0).max(0.0));
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
                app.toggle_panel_switch(PanelKind::Settings);
            }
        });
        button.upcast()
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
        name.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        let sub = gtk::Label::new(Some(subtitle));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        sub.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        texts.add(&name);
        texts.add(&sub);
        crate::ui::pack(&row, &texts, true);
        let switch = gtk::Switch::new();
        switch.set_active(value);
        switch.set_valign(gtk::Align::Center);
        // The switch is drawn, not typed: there is no text in it for a screen
        // reader to read and nothing under the pointer to say what it sets, so
        // the row's name is given to it both ways. The subtitle is a sentence,
        // not a name, and is left out. The switch already reports that it is a
        // switch; only the name is missing.
        switch.set_tooltip_text(Some(title));
        if let Some(access) = switch.accessible() {
            // Fully qualified: gtk's prelude brings several traits with a
            // `set_name`, and this one is atk's.
            gtk::atk::prelude::AtkObjectExt::set_name(&access, title);
            gtk::atk::prelude::AtkObjectExt::set_description(&access, subtitle);
        }
        let app_weak = app.weak();
        let changed = std::rc::Rc::new(changed);
        switch.connect_active_notify(move |switch| {
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
        let empty = gtk::Box::new(gtk::Orientation::Vertical, 12);
        crate::ui::class(&empty, "panel-empty");
        empty.set_margin_top(28);
        empty.set_margin_bottom(28);
        let mark = crate::icons::area(icon, 32, app.palette.borrow().muted);
        crate::ui::class(&mark, "panel-empty-icon");
        mark.set_halign(gtk::Align::Start);
        mark.set_margin_bottom(8);
        empty.pack_start(&mark, false, false, 0);
        let heading = gtk::Label::new(Some(title));
        crate::ui::class(&heading, "panel-empty-title");
        heading.set_xalign(0.0);
        heading.set_line_wrap(true);
        heading.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        empty.pack_start(&heading, false, true, 0);
        let detail = gtk::Label::new(Some(description));
        crate::ui::class(&detail, "settingdesc");
        crate::ui::class(&detail, "panel-empty-description");
        detail.set_xalign(0.0);
        detail.set_line_wrap(true);
        detail.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
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
            action.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
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
        hbox.set_margin_top(12);
        hbox.set_margin_bottom(12);
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

        let texts = gtk::Box::new(gtk::Orientation::Vertical, 4);
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
            url.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
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
        let arrow = crate::icons::area(crate::icons::Icon::ArrowRight, 12, app.palette.borrow().muted);
        button.set_image(Some(&arrow));
        button.set_image_position(gtk::PositionType::Right);
        button.set_always_show_image(true);
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
}
