use crate::icons::{self, Icon};
use crate::palette::{Palette, Rgb};
use crate::panels::PanelKind;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::f64::consts::{PI, TAU};
use std::rc::Rc;

const CONTENT_WIDTH: i32 = 1060;
const WIDE_WIDTH: i32 = 1120;
const COMPACT_WIDTH: i32 = 820;
const NARROW_WIDTH: i32 = 440;
const SHORT_HEIGHT: i32 = 620;

pub struct StartPage {
    pub root: gtk::ScrolledWindow,
    pub search: gtk::Entry,
    headline: gtk::Label,
    introduction: gtk::Label,
    session: gtk::Label,
    engine: gtk::Label,
    shortcuts_label: gtk::Label,
    shortcuts_note: gtk::Label,
    bookmarks_action: gtk::Button,
    links: gtk::FlowBox,
    recent_section: gtk::Box,
    recent_pages: gtk::Box,
    history_action: gtk::Button,
    utility_label: gtk::Label,
    protection: gtk::Button,
    protection_title: gtk::Label,
    protection_copy: gtk::Label,
    private_note: gtk::Box,
    private: Rc<Cell<bool>>,
    shown: RefCell<Option<VisibleContent>>,
    palette: Palette,
}

#[derive(PartialEq)]
struct Link {
    title: String,
    url: String,
    description: String,
    icon: Icon,
}

#[derive(PartialEq, Eq)]
struct RecentPage {
    title: String,
    url: String,
    age: String,
}

#[derive(PartialEq)]
struct VisibleContent {
    private: bool,
    bookmarked: bool,
    links: Vec<Link>,
    recent: Vec<RecentPage>,
}

impl StartPage {
    pub fn new(palette: Palette) -> StartPage {
        let root = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        style(&root, "start-page");
        style(&root, "start-compact");
        root.set_policy(gtk::PolicyType::External, gtk::PolicyType::Automatic);
        root.set_shadow_type(gtk::ShadowType::None);
        root.set_propagate_natural_width(false);
        root.set_propagate_natural_height(false);
        root.set_min_content_width(0);
        root.set_min_content_height(0);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_kinetic_scrolling(true);
        root.set_overlay_scrolling(true);

        let canvas = gtk::Box::new(gtk::Orientation::Vertical, 0);
        style(&canvas, "start-canvas");
        canvas.set_hexpand(true);
        let content = gtk::Box::new(gtk::Orientation::Vertical, 24);
        style(&content, "start-content");
        content.set_margin_start(16);
        content.set_margin_end(16);
        content.set_margin_top(24);
        content.set_margin_bottom(24);
        content.set_hexpand(true);
        content.set_valign(gtk::Align::Start);
        canvas.pack_start(&content, false, true, 0);
        root.add(&canvas);

        let private = Rc::new(Cell::new(false));
        let masthead = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let brand_mark = mark(Icon::Globe, 14, palette.accent(), "start-brand-mark");
        masthead.pack_start(&brand_mark, false, false, 0);
        // The browser's name in plain sentence case, next to its mark. The old
        // "SEARCH / START HERE" was a decorative all-caps eyebrow. It repeated
        // what the page already says and was the last capitalised label left.
        let eyebrow = label("Search", "start-eyebrow");
        masthead.pack_start(&eyebrow, true, true, 0);
        let session = label("Regular session", "start-session");
        session.set_halign(gtk::Align::End);
        masthead.pack_end(&session, false, false, 0);
        content.pack_start(&masthead, false, true, 0);

        let hero = gtk::Box::new(gtk::Orientation::Horizontal, 28);
        style(&hero, "start-hero");
        let hero_copy = gtk::Box::new(gtk::Orientation::Vertical, 0);
        hero_copy.set_hexpand(true);
        hero_copy.set_valign(gtk::Align::Center);
        let headline = heading("Go somewhere\ninteresting.", "start-headline");
        headline.set_max_width_chars(20);
        headline.set_margin_bottom(12);
        hero_copy.pack_start(&headline, false, true, 0);
        let introduction = paragraph(
            "A question, an address, a place worth coming back to.",
            "start-intro",
            52,
        );
        introduction.set_margin_bottom(12);
        hero_copy.pack_start(&introduction, false, true, 0);

        let search_section = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let search_field = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        style(&search_field, "start-search");
        search_field.set_hexpand(true);
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
        let search_icon = mark(Icon::Search, 20, palette.muted, "start-search-icon");
        search_row.pack_start(&search_icon, false, false, 0);
        let search = gtk::Entry::new();
        style(&search, "start-search-entry");
        search.set_width_chars(1);
        search.set_hexpand(true);
        search.set_has_frame(false);
        search.set_placeholder_text(Some("Search or enter an address"));
        name(&search, "Search the web or enter an address");
        search_row.pack_start(&search, true, true, 0);
        search_field.pack_start(&search_row, true, true, 0);
        let search_submit = gtk::Button::new();
        style(&search_submit, "start-search-submit");
        let search_arrow = mark(Icon::ArrowRight, 18, palette.accent_ink(), "start-arrow");
        search_submit.add(&search_arrow);
        name(&search_submit, "Open search or address");
        search_field.pack_end(&search_submit, false, false, 0);
        search_submit.connect_clicked({
            let entry = search.downgrade();
            move |_| { if let Some(entry) = entry.upgrade() { entry.emit_activate(); } }
        });
        search.connect_activate(|entry| {
            if entry.text().trim().is_empty() { return; }
            if let Some(app) = crate::app::current() {
                let target = crate::app::resolve(&app, &entry.text());
                if !target.is_empty() { app.navigate_active(&target); }
            }
        });
        search.connect_has_focus_notify({
            let field = search_field.downgrade();
            move |entry| {
                if let Some(field) = field.upgrade() { state_class(&field, "focused", entry.has_focus()); }
            }
        });
        search_section.pack_start(&search_field, false, true, 0);
        let engine = label("One field for searches and addresses.", "start-engine");
        search_section.pack_start(&engine, false, true, 0);
        hero_copy.pack_start(&search_section, false, true, 0);
        hero.pack_start(&hero_copy, true, true, 0);

        let artwork = orbital_art(palette);
        artwork.set_halign(gtk::Align::End);
        artwork.set_valign(gtk::Align::Center);
        visible(&artwork, false);
        hero.pack_end(&artwork, false, false, 0);
        content.pack_start(&hero, false, true, 0);

        let shortcuts = gtk::Box::new(gtk::Orientation::Vertical, 14);
        style(&shortcuts, "start-section");
        let shortcuts_label = section_label("Explore");
        let shortcuts_header = section_header(&shortcuts_label);
        let shortcuts_note = label("Selected by Search", "start-section-note");
        shortcuts_header.pack_end(&shortcuts_note, false, false, 0);
        let bookmarks_action = panel_button(
            "All bookmarks",
            PanelKind::Bookmarks,
            "start-section-action",
            Some(private.clone()),
        );
        visible(&bookmarks_action, false);
        shortcuts_header.pack_end(&bookmarks_action, false, false, 0);
        shortcuts.pack_start(&shortcuts_header, false, true, 0);
        let links = gtk::FlowBox::new();
        style(&links, "start-links");
        links.set_selection_mode(gtk::SelectionMode::None);
        links.set_homogeneous(true);
        links.set_min_children_per_line(1);
        links.set_max_children_per_line(2);
        links.set_column_spacing(22);
        links.set_row_spacing(10);
        links.set_can_focus(false);
        shortcuts.pack_start(&links, false, true, 0);
        content.pack_start(&shortcuts, false, true, 0);

        let bottom = gtk::Box::new(gtk::Orientation::Vertical, 26);
        style(&bottom, "start-bottom");
        let recent_section = gtk::Box::new(gtk::Orientation::Vertical, 10);
        style(&recent_section, "start-section");
        recent_section.set_hexpand(true);
        recent_section.set_valign(gtk::Align::Start);
        let recent_label = section_label("Recently opened");
        let recent_header = section_header(&recent_label);
        let history_action = panel_button(
            "All history",
            PanelKind::History,
            "start-section-action",
            Some(private.clone()),
        );
        visible(&history_action, false);
        recent_header.pack_end(&history_action, false, false, 0);
        recent_section.pack_start(&recent_header, false, true, 0);
        let recent_pages = gtk::Box::new(gtk::Orientation::Vertical, 0);
        style(&recent_pages, "start-recents");
        recent_section.pack_start(&recent_pages, false, true, 0);
        bottom.pack_start(&recent_section, true, true, 0);

        let utility = gtk::Box::new(gtk::Orientation::Vertical, 13);
        style(&utility, "start-utility");
        utility.set_valign(gtk::Align::Start);
        let utility_label = section_label("Browsing");
        utility.pack_start(&section_header(&utility_label), false, true, 0);
        let protection = gtk::Button::new();
        style(&protection, "start-protection");
        protection.set_relief(gtk::ReliefStyle::None);
        let protection_body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let protection_heading = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let protection_icon = mark(Icon::Shield, 17, palette.accent(), "start-protection-icon");
        protection_heading.pack_start(&protection_icon, false, false, 0);
        let protection_title = paragraph("Your browsing preferences", "start-protection-title", 34);
        protection_heading.pack_start(&protection_title, true, true, 0);
        protection_body.pack_start(&protection_heading, false, true, 0);
        let protection_copy = paragraph(
            "Choose ad and tracker blocking in Settings.",
            "start-protection-copy",
            40,
        );
        protection_body.pack_start(&protection_copy, false, true, 0);
        let protection_action = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        let protection_action_label = label("Review settings", "start-protection-action");
        protection_action.pack_start(&protection_action_label, false, false, 0);
        let protection_arrow = mark(Icon::ArrowRight, 13, palette.accent(), "start-arrow");
        protection_action.pack_start(&protection_arrow, false, false, 0);
        protection_body.pack_start(&protection_action, false, true, 0);
        protection.add(&protection_body);
        name(&protection, "Open privacy settings");
        protection.connect_clicked(|_| {
            if let Some(app) = crate::app::current() {
                app.open_panel(PanelKind::Settings);
            }
        });
        utility.pack_start(&protection, false, true, 0);

        let private_note = gtk::Box::new(gtk::Orientation::Vertical, 5);
        style(&private_note, "start-private-note");
        let private_note_title = paragraph("Private does not mean anonymous.", "start-private-title", 40);
        let private_note_copy = paragraph(
            "Sites and your network provider can still see your activity. Saved bookmarks and downloaded files remain.",
            "start-private-copy",
            46,
        );
        private_note.pack_start(&private_note_title, false, true, 0);
        private_note.pack_start(&private_note_copy, false, true, 0);
        visible(&private_note, false);
        utility.pack_start(&private_note, false, true, 0);
        bottom.pack_start(&utility, false, true, 0);
        content.pack_start(&bottom, false, true, 0);

        let content_weak = content.downgrade();
        let hero_weak = hero.downgrade();
        let artwork_weak = artwork.downgrade();
        let links_weak = links.downgrade();
        let bottom_weak = bottom.downgrade();
        let utility_weak = utility.downgrade();
        let root_weak = root.downgrade();
        let fitted = Rc::new(Cell::new(None::<(i32, bool, bool, bool, bool)>));
        root.connect_size_allocate(move |_, allocation| {
            let width = allocation.width().max(1);
            let wide = width >= WIDE_WIDTH;
            let compact = width < COMPACT_WIDTH;
            let narrow = width < NARROW_WIDTH;
            let short = allocation.height() < SHORT_HEIGHT;
            let base_inset = if narrow { 16 } else { 32 };
            let inset = base_inset.max((width - CONTENT_WIDTH) / 2);
            let fit = (inset, wide, compact, narrow, short);
            if fitted.replace(Some(fit)) == Some(fit) {
                return;
            }
            let fitted = fitted.clone();
            let root = root_weak.clone();
            let content = content_weak.clone();
            let hero = hero_weak.clone();
            let artwork = artwork_weak.clone();
            let links = links_weak.clone();
            let bottom = bottom_weak.clone();
            let utility = utility_weak.clone();
            glib::idle_add_local_once(move || {
                if fitted.get() != Some(fit) {
                    return;
                }
                let Some(widget) = root.upgrade() else { return };
                state_class(&widget, "start-wide", wide);
                state_class(&widget, "start-compact", compact);
                state_class(&widget, "start-narrow", narrow);
                state_class(&widget, "start-short", short);
                if let Some(content) = content.upgrade() {
                    content.set_margin_start(inset);
                    content.set_margin_end(inset);
                    content.set_margin_top(if short { 16 } else if wide { 32 } else { 24 });
                    content.set_spacing(if short { 18 } else if wide { 28 } else { 24 });
                }
                if let Some(hero) = hero.upgrade() {
                    hero.set_spacing(if wide { 36 } else { 28 });
                }
                if let Some(artwork) = artwork.upgrade() {
                    artwork.set_size_request(if wide { 360 } else { 320 }, if wide { 308 } else { 260 });
                    visible(&artwork, false);
                }
                if let Some(links) = links.upgrade() {
                    links.set_max_children_per_line(if compact { 2 } else { 4 });
                    links.set_column_spacing(if narrow { 16 } else { 22 });
                }
                if let Some(bottom) = bottom.upgrade() {
                    bottom.set_orientation(if compact { gtk::Orientation::Vertical } else { gtk::Orientation::Horizontal });
                    bottom.set_spacing(if compact { 26 } else { 36 });
                }
                if let Some(utility) = utility.upgrade() {
                    utility.set_size_request(if compact { -1 } else { 244 }, -1);
                }
                widget.queue_resize();
                widget.check_resize();
                widget.queue_draw();
            });
        });

        let page = StartPage {
            root,
            search,
            headline,
            introduction,
            session,
            engine,
            shortcuts_label,
            shortcuts_note,
            bookmarks_action,
            links,
            recent_section,
            recent_pages,
            history_action,
            utility_label,
            protection,
            protection_title,
            protection_copy,
            private_note,
            private,
            shown: RefCell::new(None),
            palette,
        };
        page.render_links(&explore_links(), false);
        page.render_recent(&[]);
        page.root.show_all();
        page
    }

    pub fn refresh(
        &self,
        bookmarks: &[crate::store::Bookmark],
        history: &[crate::store::HistoryEntry],
        private: bool,
        shield: bool,
        engine: &str,
    ) {
        let changed_mode = self.private.replace(private) != private;
        state_class(&self.root, "start-private", private);
        state_class(&self.root, "start-shield-off", !shield);
        visible(&self.recent_section, !private);
        visible(&self.private_note, private);
        if private {
            visible(&self.bookmarks_action, false);
            visible(&self.history_action, false);
            if changed_mode {
                self.shown.borrow_mut().take();
                clear(&self.links);
                clear(&self.recent_pages);
            }
            self.session.set_text("Private session");
            self.headline.set_text("A private\nsession.");
            self.introduction.set_text(
                "This tab isn’t saved to history. Private-session cookies are cleared when the last private tab closes.",
            );
            self.utility_label.set_text("This session");
        } else {
            self.session.set_text("Regular session");
            self.headline.set_text("Go somewhere\ninteresting.");
            self.introduction.set_text("A question, an address, a place worth coming back to.");
            self.utility_label.set_text("Browsing");
        }
        let engine_name = match engine {
            "Google" | "Bing" | "Brave" | "Startpage" => engine,
            _ => "DuckDuckGo",
        };
        self.engine.set_text(&format!("{engine_name} / default search engine"));
        self.protection_title.set_text(match (private, shield) {
            (true, true) => "Private session · shield on",
            (true, false) => "Private session · shield off",
            (false, true) => "Shield on",
            (false, false) => "Shield off",
        });
        self.protection_copy.set_text(if shield {
            "Ad and tracker blocking applies to new pages. Sites you allow are exempt."
        } else {
            "Ad and tracker blocking is off for new pages. You can enable it in Settings."
        });
        name(
            &self.protection,
            &format!(
                "{}. Ad and tracker blocking {}. Open privacy settings.",
                if private { "Private browsing" } else { "Regular browsing" },
                if shield { "on" } else { "off" },
            ),
        );

        let next = if private {
            VisibleContent {
                private: true,
                bookmarked: false,
                links: explore_links(),
                recent: Vec::new(),
            }
        } else {
            let saved_links = bookmark_links(bookmarks);
            let bookmarked = !saved_links.is_empty();
            VisibleContent {
                private: false,
                bookmarked,
                links: if bookmarked { saved_links } else { explore_links() },
                recent: recent_pages(history),
            }
        };
        if self.shown.borrow().as_ref() != Some(&next) {
            self.shortcuts_label.set_text(if next.bookmarked { "Your bookmarks" } else { "Explore" });
            self.shortcuts_note.set_text(if private { "Selected by Search · public links" } else { "Selected by Search" });
            visible(&self.shortcuts_note, !next.bookmarked);
            visible(&self.bookmarks_action, next.bookmarked && !private);
            visible(&self.history_action, !next.recent.is_empty() && !private);
            self.render_links(&next.links, next.bookmarked);
            if private {
                clear(&self.recent_pages);
            } else {
                self.render_recent(&next.recent);
            }
            *self.shown.borrow_mut() = Some(next);
        }
        if changed_mode {
            self.search.set_text("");
            self.root.vadjustment().set_value(0.0);
        }
    }

    fn render_links(&self, links: &[Link], personal: bool) {
        clear(&self.links);
        for link in links {
            let button = gtk::Button::new();
            style(&button, "start-link");
            style(&button, if personal { "start-bookmark" } else { "start-explore" });
            button.set_relief(gtk::ReliefStyle::None);
            button.set_size_request(128, -1);
            button.set_hexpand(true);
            button.set_tooltip_text(Some(&link.url));
            name(&button, &format!("Open {}. {}", link.title, host_label(&link.url)));
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 9);
            let icon = mark(link.icon, 16, self.palette.muted, "start-icon");
            icon.set_valign(gtk::Align::Start);
            icon.set_margin_top(2);
            row.pack_start(&icon, false, false, 0);
            let copy = gtk::Box::new(gtk::Orientation::Vertical, 5);
            copy.set_hexpand(true);
            let title = label(&link.title, "start-link-title");
            title.set_max_width_chars(22);
            copy.pack_start(&title, false, true, 0);
            let description = label(&link.description, "start-link-description");
            description.set_max_width_chars(24);
            copy.pack_start(&description, false, true, 0);
            row.pack_start(&copy, true, true, 0);
            button.add(&row);
            let target = link.url.clone();
            let private = self.private.clone();
            button.connect_clicked(move |_| {
                if personal && private.get() {
                    return;
                }
                if let Some(app) = crate::app::current() {
                    app.navigate_active(&target);
                }
            });
            let slot = gtk::FlowBoxChild::new();
            style(&slot, "start-link-slot");
            slot.set_can_focus(false);
            slot.add(&button);
            self.links.insert(&slot, -1);
        }
        self.links.show_all();
    }

    fn render_recent(&self, pages: &[RecentPage]) {
        clear(&self.recent_pages);
        if pages.is_empty() {
            let empty = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            style(&empty, "start-empty");
            let empty_icon = mark(Icon::Clock, 19, self.palette.muted, "start-icon");
            empty_icon.set_valign(gtk::Align::Start);
            empty.pack_start(&empty_icon, false, false, 0);
            let copy = gtk::Box::new(gtk::Orientation::Vertical, 6);
            copy.set_hexpand(true);
            let title = paragraph("No recent pages", "start-empty-title", 34);
            let detail = paragraph(
                "Visit a page and it will appear here. Private browsing never appears in this list.",
                "start-empty-copy",
                56,
            );
            copy.pack_start(&title, false, true, 0);
            copy.pack_start(&detail, false, true, 0);
            empty.pack_start(&copy, true, true, 0);
            self.recent_pages.pack_start(&empty, false, true, 0);
            self.recent_pages.pack_start(&rule(), false, true, 0);
        } else {
            // Peekable so the loop can tell which row is last: the list used to
            // end on a hairline with nothing under it (Gemini R60 polish).
            let mut pages = pages.into_iter().peekable();
            while let Some(page) = pages.next() {
                let button = gtk::Button::new();
                style(&button, "start-recent");
                button.set_relief(gtk::ReliefStyle::None);
                button.set_tooltip_text(Some(&page.url));
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 11);
                // A letter tile from the host, not a globe. Every row used to
                // show the same globe, so nothing at a glance told two sites
                // apart (Gemini R60 polish). It reuses the tab column's
                // .tile/.tileletter style (src/css.rs), so the start page and the
                // column look like the same app.
                let host_text = host_label(&page.url);
                // The tab column learned to qualify same-host tabs with their
                // path; recently opened rows need the same cue, or every page
                // from the local test server still reads as one bare host.
                let place = {
                    let location = crate::shield::location_of(&page.url);
                    if location == "/" {
                        host_text.clone()
                    } else {
                        format!("{}{}", host_text, location)
                    }
                };
                name(&button, &format!("Open {}. {}. {}", page.title, place, page.age));
                let host_bare = host_text.trim_start_matches("www.");
                // An IP address, localhost or a local file gives every page the
                // same tile ("1" for each 127.0.0.1 page, as round 62 showed),
                // which tells sites apart no better than the old globe. For those
                // hosts the tile takes the page title's first letter instead, and
                // falls back to the host when the title has no letter or digit.
                let local = host_bare
                    .trim_matches(|c| c == '[' || c == ']')
                    .parse::<std::net::IpAddr>()
                    .is_ok()
                    || host_bare.eq_ignore_ascii_case("localhost")
                    || host_bare.to_ascii_lowercase().ends_with(".localhost")
                    || host_text == "Local file";
                let first = |text: &str| {
                    text.chars()
                        .find(|c| c.is_alphanumeric())
                        .map(|c| c.to_uppercase().collect::<String>())
                };
                let initial = (if local { first(&page.title) } else { None })
                    .or_else(|| first(host_bare))
                    .unwrap_or_else(|| "·".to_string());
                let tile = gtk::Box::new(gtk::Orientation::Vertical, 0);
                style(&tile, "tile");
                tile.set_size_request(22, 22);
                tile.set_valign(gtk::Align::Center);
                let letter = gtk::Label::new(Some(&initial));
                style(&letter, "tileletter");
                letter.set_halign(gtk::Align::Center);
                letter.set_valign(gtk::Align::Center);
                letter.set_vexpand(true);
                tile.pack_start(&letter, true, true, 0);
                row.pack_start(&tile, false, false, 0);
                let copy = gtk::Box::new(gtk::Orientation::Vertical, 5);
                copy.set_hexpand(true);
                let title = label(&page.title, "start-recent-title");
                title.set_max_width_chars(52);
                copy.pack_start(&title, false, true, 0);
                let host = label(&place, "start-recent-host");
                host.set_max_width_chars(54);
                copy.pack_start(&host, false, true, 0);
                row.pack_start(&copy, true, true, 0);
                let age = gtk::Label::new(Some(&page.age));
                style(&age, "start-recent-age");
                age.set_xalign(1.0);
                row.pack_end(&age, false, false, 0);
                button.add(&row);
                let target = page.url.clone();
                let private = self.private.clone();
                button.connect_clicked(move |_| {
                    if private.get() {
                        return;
                    }
                    if let Some(app) = crate::app::current() {
                        app.navigate_active(&target);
                    }
                });
                self.recent_pages.pack_start(&button, false, true, 0);
                // Rules go only between rows. The last row ends on the
                // section's own spacing, not on an extra line.
                if pages.peek().is_some() {
                    self.recent_pages.pack_start(&rule(), false, true, 0);
                }
            }
        }
        self.recent_pages.show_all();
    }
}

fn style(widget: &impl IsA<gtk::Widget>, class: &str) {
    widget.style_context().add_class(class);
}

fn state_class(widget: &impl IsA<gtk::Widget>, class: &str, enabled: bool) {
    if enabled {
        widget.style_context().add_class(class);
    } else {
        widget.style_context().remove_class(class);
    }
}

fn visible(widget: &impl IsA<gtk::Widget>, show: bool) {
    widget.set_no_show_all(!show);
    if show {
        widget.show_all();
    } else {
        widget.hide();
    }
}

fn name(widget: &impl IsA<gtk::Widget>, text: &str) {
    if let Some(accessible) = widget.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_name(&accessible, text);
    }
}

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    style(&label, class);
    label.set_xalign(0.0);
    label.set_yalign(0.5);
    label.set_width_chars(1);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_single_line_mode(true);
    label
}

fn paragraph(text: &str, class: &str, maximum: i32) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    style(&label, class);
    label.set_xalign(0.0);
    label.set_yalign(0.0);
    label.set_width_chars(1);
    label.set_max_width_chars(maximum);
    label.set_line_wrap(true);
    label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    label
}

fn heading(text: &str, class: &str) -> gtk::Label {
    let label = paragraph(text, class, 48);
    if let Some(accessible) = label.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&accessible, gtk::atk::Role::Heading);
    }
    label
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    style(&label, "start-section-label");
    label.set_xalign(0.0);
    label.set_single_line_mode(true);
    if let Some(accessible) = label.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&accessible, gtk::atk::Role::Heading);
    }
    label
}

fn rule() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    style(&separator, "start-rule");
    separator
}

fn section_header(title: &gtk::Label) -> gtk::Box {
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.pack_start(title, false, false, 0);
    let separator = rule();
    separator.set_valign(gtk::Align::Center);
    header.pack_start(&separator, true, true, 0);
    header
}

fn mark(kind: Icon, size: i32, fallback: Rgb, class: &str) -> gtk::DrawingArea {
    let area = icons::area(kind, size, fallback);
    style(&area, class);
    area.set_valign(gtk::Align::Center);
    area.set_can_focus(false);
    if let Some(accessible) = area.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&accessible, gtk::atk::Role::RedundantObject);
    }
    area
}

fn panel_button(
    text: &str,
    panel: PanelKind,
    class: &str,
    private: Option<Rc<Cell<bool>>>,
) -> gtk::Button {
    let button = gtk::Button::with_label(text);
    style(&button, class);
    button.set_relief(gtk::ReliefStyle::None);
    button.connect_clicked(move |_| {
        if private.as_ref().is_some_and(|state| state.get()) {
            return;
        }
        if let Some(app) = crate::app::current() {
            app.open_panel(panel.clone());
        }
    });
    button
}

fn clear(container: &impl IsA<gtk::Container>) {
    for child in container.children() {
        container.remove(&child);
    }
}

fn explore_links() -> Vec<Link> {
    [
        ("GitHub", "https://github.com/", "Code & collaboration", Icon::Command),
        ("Wikipedia", "https://www.wikipedia.org/", "The free encyclopedia", Icon::Book),
        ("MDN Web Docs", "https://developer.mozilla.org/", "Understand the web", Icon::Book),
        ("YouTube", "https://www.youtube.com/", "Video & discovery", Icon::Globe),
    ]
    .into_iter()
    .map(|(title, url, description, icon)| Link {
        title: title.into(),
        url: url.into(),
        description: description.into(),
        icon,
    })
    .collect()
}

fn bookmark_links(bookmarks: &[crate::store::Bookmark]) -> Vec<Link> {
    let mut seen = HashSet::new();
    let mut links = Vec::new();
    for bookmark in bookmarks {
        let Some(key) = page_key(&bookmark.url) else { continue };
        if !seen.insert(key) {
            continue;
        }
        links.push(Link {
            title: page_title(&bookmark.title, &bookmark.url),
            url: bookmark.url.trim().into(),
            description: host_label(&bookmark.url),
            icon: Icon::Star,
        });
        if links.len() == 4 {
            break;
        }
    }
    links
}

fn recent_pages(history: &[crate::store::HistoryEntry]) -> Vec<RecentPage> {
    let mut ordered: Vec<_> = history.iter().collect();
    ordered.sort_by(|left_entry, right_entry| right_entry.at.cmp(&left_entry.at));
    let now = crate::store::now_secs();
    let mut seen = HashSet::new();
    let mut pages = Vec::new();
    for entry in ordered {
        let Some(key) = page_key(&entry.url) else { continue };
        if key.starts_with("file://") || !seen.insert(key) {
            continue;
        }
        pages.push(RecentPage {
            title: page_title(&entry.title, &entry.url),
            url: entry.url.trim().into(),
            age: if entry.at == 0 {
                "Earlier".into()
            } else if entry.at > now {
                "Date unavailable".into()
            } else {
                crate::store::ago(entry.at, now)
            },
        });
        if pages.len() == 3 {
            break;
        }
    }
    pages
}

fn page_key(url: &str) -> Option<String> {
    let (scheme, rest) = url.trim().split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "https" | "http" | "file") {
        return None;
    }
    let rest = rest.split('#').next()?;
    let authority_end = rest.find(['/', '?']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    if authority.is_empty() && scheme != "file" {
        return None;
    }
    let path = &rest[authority_end..];
    let path = if path == "/" { "" } else { path };
    Some(format!("{scheme}://{}{path}", authority.to_ascii_lowercase()))
}

fn host_label(url: &str) -> String {
    if url.trim().to_ascii_lowercase().starts_with("file://") {
        return "Local file".into();
    }
    let host = crate::shield::host_of(url.trim());
    host.strip_prefix("www.").unwrap_or(&host).to_string()
}

fn page_title(title: &str, url: &str) -> String {
    let title = title.trim();
    if title.is_empty() || title == url.trim() {
        host_label(url)
    } else {
        title.chars().take(160).collect()
    }
}

fn rgba(color: Rgb) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        color.0 as f64 / 255.0,
        color.1 as f64 / 255.0,
        color.2 as f64 / 255.0,
        1.0,
    )
}

fn blend(first: &gtk::gdk::RGBA, second: &gtk::gdk::RGBA, amount: f64) -> gtk::gdk::RGBA {
    gtk::gdk::RGBA::new(
        first.red() + (second.red() - first.red()) * amount,
        first.green() + (second.green() - first.green()) * amount,
        first.blue() + (second.blue() - first.blue()) * amount,
        first.alpha() + (second.alpha() - first.alpha()) * amount,
    )
}

fn paint(context: &cairo::Context, color: &gtk::gdk::RGBA, opacity: f64) {
    context.set_source_rgba(color.red(), color.green(), color.blue(), color.alpha() * opacity);
}

fn ellipse(context: &cairo::Context, radius_x: f64, radius_y: f64, start: f64, end: f64) {
    let _ = context.save();
    context.scale(radius_x, radius_y);
    context.arc(0.0, 0.0, 1.0, start, end);
    let _ = context.restore();
}

fn orbital_art(palette: Palette) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    style(&area, "start-orbit");
    area.set_size_request(320, 260);
    area.set_can_focus(false);
    if let Some(accessible) = area.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&accessible, gtk::atk::Role::RedundantObject);
    }
    area.connect_style_updated(|widget| widget.queue_draw());
    area.connect_state_flags_changed(|widget, _| widget.queue_draw());
    area.connect_draw(move |widget, context| {
        let styles = widget.style_context();
        let state = widget.state_flags();
        let foreground = styles.color(state);
        let accent = if foreground.alpha() > 0.0 { foreground } else { rgba(palette.accent()) };
        let background = styles
            .style_property_for_state("background-color", state)
            .get::<gtk::gdk::RGBA>()
            .ok()
            .filter(|color| color.alpha() > 0.0)
            .unwrap_or_else(|| rgba(palette.ground));
        let ink = styles
            .style_property_for_state("border-top-color", state)
            .get::<gtk::gdk::RGBA>()
            .ok()
            .filter(|color| color.alpha() > 0.0)
            .unwrap_or_else(|| rgba(palette.ink));
        let dark = background.red() * 0.2126 + background.green() * 0.7152 + background.blue() * 0.0722 < 0.45;
        let paper = if dark { ink.clone() } else { blend(&background, &gtk::gdk::RGBA::WHITE, 0.62) };
        let eclipse = if dark { blend(&background, &ink, 0.08) } else { ink.clone() };
        let width = widget.allocated_width().max(1) as f64;
        let height = widget.allocated_height().max(1) as f64;
        let scale = (width / 340.0).min(height / 280.0);
        let _ = context.save();
        context.translate((width - 340.0 * scale) / 2.0, (height - 280.0 * scale) / 2.0);
        context.scale(scale, scale);
        context.set_line_cap(cairo::LineCap::Round);
        context.set_line_join(cairo::LineJoin::Round);

        let _ = context.save();
        context.translate(165.0, 140.0);
        context.rotate(-0.60);
        paint(context, &ink, 0.42);
        context.set_line_width(1.0);
        ellipse(context, 158.0, 70.0, 0.0, TAU);
        let _ = context.stroke();
        paint(context, &paper, 1.0);
        context.set_line_width(24.0);
        ellipse(context, 148.0, 48.0, PI, TAU);
        let _ = context.stroke();
        let _ = context.restore();

        paint(context, &accent, 1.0);
        context.arc(176.0, 134.0, 124.0, 0.0, TAU);
        let _ = context.fill();

        let _ = context.save();
        context.arc(176.0, 134.0, 124.0, 0.0, TAU);
        context.clip();
        paint(context, &eclipse, 1.0);
        context.move_to(40.0, 208.0);
        context.curve_to(145.0, 274.0, 204.0, 130.0, 310.0, 150.0);
        context.line_to(340.0, 290.0);
        context.line_to(30.0, 290.0);
        context.close_path();
        let _ = context.fill();
        let _ = context.restore();

        let _ = context.save();
        context.translate(165.0, 140.0);
        context.rotate(-0.60);
        paint(context, &paper, 1.0);
        context.set_line_width(24.0);
        ellipse(context, 148.0, 48.0, 0.0, PI);
        let _ = context.stroke();
        paint(context, &eclipse, 0.60);
        context.set_line_width(1.0);
        ellipse(context, 148.0, 60.0, 0.05, PI - 0.05);
        let _ = context.stroke();
        let _ = context.restore();

        paint(context, &paper, 1.0);
        context.arc(44.0, 42.0, 9.0, 0.0, TAU);
        let _ = context.fill();
        paint(context, &ink, 0.80);
        context.arc(44.0, 42.0, 9.0, 0.0, TAU);
        context.set_line_width(1.0);
        let _ = context.stroke();
        paint(context, &accent, 1.0);
        context.arc(300.0, 269.0, 3.5, 0.0, TAU);
        let _ = context.fill();
        let _ = context.restore();
        crate::gtkx::inh(false)
    });
    area
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{Bookmark, HistoryEntry};

    #[test]
    fn bookmarks_are_deduplicated_and_capped_without_unsafe_schemes() {
        let mut bookmarks = vec![
            Bookmark { title: "One".into(), url: "https://EXAMPLE.com/".into() },
            Bookmark { title: "Duplicate".into(), url: "https://example.com/#section".into() },
            Bookmark { title: "Script".into(), url: "javascript:alert(1)".into() },
        ];
        for index in 0..8 {
            bookmarks.push(Bookmark { title: format!("Page {index}"), url: format!("https://example.com/{index}") });
        }
        let links = bookmark_links(&bookmarks);
        assert_eq!(links.len(), 4);
        assert_eq!(links[0].title, "One");
        assert!(!links.iter().any(|link| link.title == "Duplicate" || link.title == "Script"));
    }

    #[test]
    fn recent_pages_are_ordered_deduplicated_and_exclude_local_files() {
        let entries = vec![
            HistoryEntry { title: "Old title".into(), url: "https://example.com/".into(), at: 1 },
            HistoryEntry { title: "Local document".into(), url: "file:///tmp/private.txt".into(), at: 100 },
            HistoryEntry { title: "Newest title".into(), url: "https://example.com/#section".into(), at: 8 },
            HistoryEntry { title: "Second".into(), url: "https://example.org/".into(), at: 5 },
        ];
        let pages = recent_pages(&entries);
        assert_eq!(pages.len(), 2);
        assert_eq!(pages[0].title, "Newest title");
        assert_eq!(pages[1].title, "Second");
    }

    #[test]
    fn fallback_titles_show_the_actual_host_and_explore_links_are_distinct() {
        assert_eq!(page_title("", "https://www.example.com/a"), "example.com");
        let links = explore_links();
        assert_eq!(links.len(), 4);
        assert!(links.iter().all(|link| link.url.starts_with("https://")));
        assert_eq!(links.iter().map(|link| &link.url).collect::<HashSet<_>>().len(), 4);
    }
}
