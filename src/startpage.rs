use crate::icons::{self, Icon};
use crate::palette::{Palette, Rgb};
use crate::store::{Look, StartBookmarksMode, StartDitherMode};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

const CONTENT_WIDTH: i32 = 640;
const WIDE_WIDTH: i32 = 1120;
const COMPACT_WIDTH: i32 = 820;
const NARROW_WIDTH: i32 = 460;
const SHORT_HEIGHT: i32 = 620;
const DITHER_CELL: i32 = 2;
const BOOKMARK_GAP: i32 = 14;
const MAX_START_BOOKMARKS: usize = 8;

/// Classic 8x8 Bayer ordered-dither threshold matrix (0..64).
/// Produces crisp, deterministic architectural halftone patterns instead of
/// smooth gradients or random static.
pub(crate) const BAYER_8X8: [[u8; 8]; 8] = [
    [0, 32, 8, 40, 2, 34, 10, 42],
    [48, 16, 56, 24, 50, 18, 58, 26],
    [12, 44, 4, 36, 14, 46, 6, 38],
    [60, 28, 52, 20, 62, 30, 54, 22],
    [3, 35, 11, 43, 1, 33, 9, 41],
    [51, 19, 59, 27, 49, 17, 57, 25],
    [15, 47, 7, 39, 13, 45, 5, 37],
    [63, 31, 55, 23, 61, 29, 53, 21],
];

struct DitherCache {
    key: (i32, i32, bool, bool, StartDitherMode),
    surface: cairo::ImageSurface,
}

pub struct StartPage {
    pub root: gtk::ScrolledWindow,
    pub search: gtk::Entry,
    dither: gtk::DrawingArea,
    private_pill: gtk::Box,
    private_note: gtk::Box,
    links: gtk::Box,
    pub suggest_box: gtk::Box,
    #[allow(dead_code)]
    suggest_list: gtk::Box,
    pub offers: Rc<RefCell<Vec<crate::omnibox::Offer>>>,
    #[allow(dead_code)]
    rows: Rc<RefCell<Vec<gtk::EventBox>>>,
    pub picked: Rc<Cell<i32>>,
    compact: Rc<Cell<bool>>,
    narrow: Rc<Cell<bool>>,
    private: Rc<Cell<bool>>,
    bookmarks_mode: Rc<Cell<StartBookmarksMode>>,
    dither_mode: Rc<Cell<StartDitherMode>>,
    icon_gen: Cell<u64>,
    shown: RefCell<Option<VisibleContent>>,
}

#[derive(PartialEq, Eq, Debug)]
struct Link {
    title: String,
    url: String,
    description: String,
}

#[derive(PartialEq, Eq)]
struct VisibleContent {
    private: bool,
    show_shortcuts: bool,
    max_shortcuts: usize,
    mode: StartBookmarksMode,
    links: Vec<Link>,
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

        let private = Rc::new(Cell::new(false));
        let bookmarks_mode = Rc::new(Cell::new(StartBookmarksMode::default()));
        let dither_mode = Rc::new(Cell::new(StartDitherMode::Full));
        let initial_dark = palette.is_dark();

        let canvas = gtk::Overlay::new();
        style(&canvas, "start-canvas");
        canvas.set_hexpand(true);
        canvas.set_vexpand(true);

        let dither = dither_drawing_area_with_mode(initial_dark, private.clone(), dither_mode.clone());
        style(&dither, "start-dither");
        dither.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
        {
            let private_ctx = private.clone();
            let mode_ctx = bookmarks_mode.clone();
            dither.connect_button_press_event(move |_, event| {
                if event.button() == 3 && !private_ctx.get() {
                    open_bookmark_mode_menu(mode_ctx.get(), event);
                    return crate::gtkx::inh(true);
                }
                crate::gtkx::inh(false)
            });
        }
        canvas.add(&dither);

        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        style(&content, "start-content");
        content.set_margin_start(24);
        content.set_margin_end(24);
        content.set_margin_top(160);
        content.set_margin_bottom(32);
        content.set_hexpand(true);
        content.set_halign(gtk::Align::Fill);
        content.set_valign(gtk::Align::Start);

        let private_pill = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        style(&private_pill, "start-private-pill");
        private_pill.set_halign(gtk::Align::Center);
        let private_icon = mark(Icon::Mask, 14, palette.muted, "start-private-icon");
        private_pill.pack_start(&private_icon, false, false, 0);
        let private_heading = heading("Private tab", "start-private-badge");
        private_pill.pack_start(&private_heading, false, false, 0);
        visible(&private_pill, false);
        content.pack_start(&private_pill, false, false, 0);

        let search_field = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        style(&search_field, "start-search");
        search_field.set_hexpand(true);
        let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let search_icon = mark(Icon::Search, 14, palette.muted, "start-search-icon");
        search_row.pack_start(&search_icon, false, false, 0);
        let search = gtk::Entry::new();
        style(&search, "start-search-entry");
        search.set_width_chars(1);
        search.set_hexpand(true);
        search.set_has_frame(false);
        search.set_placeholder_text(Some("Search with Startpage or enter an address"));
        name(&search, "Search the web or enter an address");
        search_row.pack_start(&search, true, true, 0);
        search_field.pack_start(&search_row, true, true, 0);
        let search_submit = gtk::Button::new();
        style(&search_submit, "start-search-submit");
        search_submit.set_valign(gtk::Align::Center);
        let search_arrow = mark(Icon::ArrowRight, 13, palette.muted, "start-arrow");
        search_submit.add(&search_arrow);
        name(&search_submit, "Open search or address");
        search_field.pack_end(&search_submit, false, false, 0);
        let suggest_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        style(&suggest_box, "start-suggest");
        suggest_box.set_hexpand(true);
        let suggest_list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        style(&suggest_list, "suggest");
        suggest_list.set_hexpand(true);
        suggest_box.pack_start(&suggest_list, false, true, 0);
        visible(&suggest_box, false);

        let offers: Rc<RefCell<Vec<crate::omnibox::Offer>>> = Rc::new(RefCell::new(Vec::new()));
        let rows: Rc<RefCell<Vec<gtk::EventBox>>> = Rc::new(RefCell::new(Vec::new()));
        let picked = Rc::new(Cell::new(-1i32));

        search_submit.connect_clicked({
            let entry = search.downgrade();
            move |_| {
                if let Some(entry) = entry.upgrade() {
                    entry.emit_activate();
                }
            }
        });
        {
            let offers_act = offers.clone();
            let picked_act = picked.clone();
            let suggest_act = suggest_box.downgrade();
            let private_act = private.clone();
            search.connect_activate(move |entry| {
                let idx = picked_act.get();
                if idx >= 0 {
                    let chosen = offers_act.borrow().get(idx as usize).cloned();
                    if let Some(offer) = chosen {
                        if let Some(sb) = suggest_act.upgrade() {
                            visible(&sb, false);
                        }
                        picked_act.set(-1);
                        if let Some(app) = crate::app::current() {
                            activate_start_offer(&app, &offer, private_act.get());
                        }
                        return;
                    }
                }
                if entry.text().trim().is_empty() {
                    return;
                }
                if let Some(sb) = suggest_act.upgrade() {
                    visible(&sb, false);
                }
                picked_act.set(-1);
                if let Some(app) = crate::app::current() {
                    let target = crate::app::resolve(&app, &entry.text());
                    if !target.is_empty() {
                        app.navigate_active(&target);
                    }
                }
            });
        }
        {
            let offers_key = offers.clone();
            let rows_key = rows.clone();
            let picked_key = picked.clone();
            let suggest_key = suggest_box.downgrade();
            search.connect_key_press_event(move |entry, event| {
                let ctrl = event.state().contains(gtk::gdk::ModifierType::CONTROL_MASK);
                let alt = event.state().contains(gtk::gdk::ModifierType::MOD1_MASK);
                if ctrl || alt {
                    return crate::gtkx::inh(false);
                }
                let shown = suggest_key.upgrade().is_some_and(|w| w.is_visible());
                match event.keyval().name().as_deref() {
                    Some("Down") if shown => {
                        step_rows(&rows_key, &picked_key, 1);
                        crate::gtkx::inh(true)
                    }
                    Some("Up") if shown => {
                        step_rows(&rows_key, &picked_key, -1);
                        crate::gtkx::inh(true)
                    }
                    Some("Tab") if shown => {
                        let list = offers_key.borrow();
                        if list.is_empty() {
                            return crate::gtkx::inh(false);
                        }
                        let idx = if picked_key.get() >= 0 {
                            (picked_key.get() as usize).min(list.len() - 1)
                        } else {
                            0
                        };
                        let completion = offer_completion(&list[idx]);
                        drop(list);
                        entry.set_text(&completion);
                        entry.set_position(-1);
                        crate::gtkx::inh(true)
                    }
                    Some("Escape") if shown => {
                        if let Some(sb) = suggest_key.upgrade() {
                            visible(&sb, false);
                        }
                        picked_key.set(-1);
                        crate::suggest::stop();
                        crate::gtkx::inh(true)
                    }
                    _ => crate::gtkx::inh(false),
                }
            });
        }
        search.connect_has_focus_notify({
            let field = search_field.downgrade();
            move |entry| {
                if let Some(field) = field.upgrade() {
                    state_class(&field, "focused", entry.has_focus());
                }
            }
        });
        {
            let field = search_field.downgrade();
            let suggest_w = suggest_box.downgrade();
            let list_w = suggest_list.downgrade();
            let offers_c = offers.clone();
            let rows_c = rows.clone();
            let picked_c = picked.clone();
            let private_c = private.clone();
            search.connect_changed(move |entry| {
                let raw = entry.text().to_string();
                let has_text = !raw.trim().is_empty();
                if let Some(field) = field.upgrade() {
                    state_class(&field, "has-text", has_text);
                }
                let (Some(sb), Some(list_box)) = (suggest_w.upgrade(), list_w.upgrade()) else {
                    return;
                };
                populate_start_suggestions(
                    &sb,
                    &list_box,
                    &offers_c,
                    &rows_c,
                    &picked_c,
                    &raw,
                    private_c.get(),
                    false,
                );
            });
        }
        let search_stack = gtk::Box::new(gtk::Orientation::Vertical, 4);
        search_stack.set_hexpand(true);
        search_stack.pack_start(&search_field, false, true, 0);
        search_stack.pack_start(&suggest_box, false, true, 0);
        content.pack_start(&search_stack, false, true, 0);

        // Centered horizontal row(s) of spacious pill bookmark chips, shown ONLY
        // when the user has saved bookmarks in regular browsing. Uses explicit
        // centered horizontal rows with generous consistent 14px gaps rather
        // than FlowBox+halign=Center (which collapses to 1 column in GTK3).
        let links = gtk::Box::new(gtk::Orientation::Vertical, BOOKMARK_GAP);
        style(&links, "start-links");
        links.set_halign(gtk::Align::Center);
        links.set_margin_top(6);
        visible(&links, false);
        content.pack_start(&links, false, false, 0);

        let private_note = gtk::Box::new(gtk::Orientation::Vertical, 0);
        style(&private_note, "start-private-note");
        private_note.set_halign(gtk::Align::Center);
        let private_copy = paragraph(
            "History isn\u{2019}t saved and private cookies clear when private tabs close. Private does not mean anonymous \u{2014} sites and your network can still see activity.",
            "start-private-copy",
            58,
        );
        private_copy.set_justify(gtk::Justification::Center);
        private_copy.set_xalign(0.5);
        private_note.pack_start(&private_copy, false, false, 0);
        visible(&private_note, false);
        content.pack_start(&private_note, false, false, 0);

        canvas.add_overlay(&content);
        root.add(&canvas);

        let compact_state = Rc::new(Cell::new(false));
        let narrow_state = Rc::new(Cell::new(false));
        let content_weak = content.downgrade();
        let links_weak = links.downgrade();
        let dither_weak = dither.downgrade();
        let root_weak = root.downgrade();
        let compact_for_alloc = compact_state.clone();
        let narrow_for_alloc = narrow_state.clone();
        let mode_for_alloc = bookmarks_mode.clone();
        let fitted = Rc::new(Cell::new(None::<(i32, i32, i32, bool, bool, bool, bool)>));
        root.connect_size_allocate(move |_, allocation| {
            let width = allocation.width().max(1);
            let height = allocation.height().max(1);
            let wide = width >= WIDE_WIDTH;
            let compact = width < COMPACT_WIDTH;
            let narrow = width < NARROW_WIDTH;
            let short = height < SHORT_HEIGHT;
            let base_inset = if narrow { 16 } else { 32 };
            let inset = base_inset.max((width - CONTENT_WIDTH) / 2);
            // Sit cleanly near the upper clear-to-dither horizon so the enlarged
            // ~65% Bayer dither field breathes generously beneath and around
            // the solid pill controls.
            let top_margin = if short {
                ((height as f64) * 0.19).round() as i32
            } else {
                ((height as f64) * 0.26).round() as i32
            }
            .clamp(56, 230);
            let min_canvas_h = height.max(top_margin + 220);
            let fit = (inset, top_margin, min_canvas_h, wide, compact, narrow, short);
            if fitted.replace(Some(fit)) == Some(fit) {
                return;
            }
            let fitted = fitted.clone();
            let root = root_weak.clone();
            let content = content_weak.clone();
            let links = links_weak.clone();
            let dither = dither_weak.clone();
            let compact_cell = compact_for_alloc.clone();
            let narrow_cell = narrow_for_alloc.clone();
            let mode_cell = mode_for_alloc.clone();
            glib::idle_add_local_once(move || {
                if fitted.get() != Some(fit) {
                    return;
                }
                let Some(widget) = root.upgrade() else { return };
                state_class(&widget, "start-wide", wide);
                state_class(&widget, "start-compact", compact);
                state_class(&widget, "start-narrow", narrow);
                state_class(&widget, "start-short", short);
                if let Some(dither) = dither.upgrade() {
                    dither.set_size_request(-1, min_canvas_h);
                }
                if let Some(content) = content.upgrade() {
                    content.set_margin_start(inset);
                    content.set_margin_end(inset);
                    content.set_margin_top(top_margin);
                    content.set_margin_bottom(if short { 24 } else { 40 });
                    content.set_spacing(if short { 16 } else { 20 });
                }
                let prev_compact = compact_cell.replace(compact);
                let prev_narrow = narrow_cell.replace(narrow);
                if prev_compact != compact || prev_narrow != narrow {
                    if let Some(links) = links.upgrade() {
                        reflow_link_rows(&links, chips_per_row(mode_cell.get(), compact, narrow));
                    }
                }
                widget.queue_resize();
                widget.check_resize();
                widget.queue_draw();
            });
        });

        let page = StartPage {
            root,
            search,
            dither,
            private_pill,
            private_note,
            links,
            suggest_box,
            suggest_list,
            offers,
            rows,
            picked,
            compact: compact_state,
            narrow: narrow_state,
            private,
            bookmarks_mode,
            dither_mode,
            icon_gen: Cell::new(0),
            shown: RefCell::new(None),
        };
        page.root.show_all();
        page
    }

    #[allow(dead_code)]
    pub fn step_suggest(&self, delta: i32) {
        if self.suggest_box.is_visible() {
            step_rows(&self.rows, &self.picked, delta);
        }
    }

    #[allow(dead_code)]
    pub fn complete_suggest(&self) -> bool {
        if !self.suggest_box.is_visible() {
            return false;
        }
        let list = self.offers.borrow();
        if list.is_empty() {
            return false;
        }
        let idx = if self.picked.get() >= 0 {
            (self.picked.get() as usize).min(list.len() - 1)
        } else {
            0
        };
        let completion = offer_completion(&list[idx]);
        drop(list);
        self.search.set_text(&completion);
        self.search.set_position(-1);
        true
    }

    pub fn hide_suggest(&self) {
        visible(&self.suggest_box, false);
        self.picked.set(-1);
    }

    pub fn refresh_suggestions_async(&self) {
        let raw = self.search.text().to_string();
        populate_start_suggestions(
            &self.suggest_box,
            &self.suggest_list,
            &self.offers,
            &self.rows,
            &self.picked,
            &raw,
            self.private.get(),
            true,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub fn refresh(
        &self,
        bookmarks: &[crate::store::Bookmark],
        _history: &[crate::store::HistoryEntry],
        private: bool,
        shield: bool,
        engine: &str,
        show_shortcuts: bool,
        shortcuts_count: usize,
        bookmarks_mode: StartBookmarksMode,
        dither_mode: StartDitherMode,
    ) {
        let changed_mode = self.private.replace(private) != private;
        self.bookmarks_mode.set(bookmarks_mode);
        self.dither_mode.set(dither_mode);
        state_class(&self.root, "start-private", private);
        state_class(&self.root, "start-shield-off", !shield);
        visible(&self.private_pill, private);
        visible(&self.private_note, private);

        let engine_name = match engine {
            "Startpage" | "Ecosia" | "DuckDuckGo" | "Google" | "Bing" | "Brave" => engine,
            _ => "Startpage",
        };
        self.search.set_placeholder_text(Some(&format!(
            "Search with {engine_name} or enter an address"
        )));

        let max_shortcuts = shortcuts_count.clamp(
            crate::store::MIN_START_SHORTCUTS_COUNT,
            crate::store::MAX_START_SHORTCUTS_COUNT,
        );
        let next = if private || !show_shortcuts {
            VisibleContent {
                private,
                show_shortcuts,
                max_shortcuts,
                mode: bookmarks_mode,
                links: Vec::new(),
            }
        } else {
            VisibleContent {
                private: false,
                show_shortcuts: true,
                max_shortcuts,
                mode: bookmarks_mode,
                links: bookmark_links_capped(bookmarks, max_shortcuts),
            }
        };
        let cur_gen = crate::app::current().map(|a| a.icon_gen.get()).unwrap_or(0);
        let icons_changed = self.icon_gen.replace(cur_gen) != cur_gen;
        if self.shown.borrow().as_ref() != Some(&next)
            || (icons_changed && !private && show_shortcuts && !next.links.is_empty())
        {
            if private || !show_shortcuts || next.links.is_empty() {
                clear(&self.links);
                visible(&self.links, false);
            } else {
                self.render_links(&next.links, bookmarks_mode);
                visible(&self.links, true);
            }
            *self.shown.borrow_mut() = Some(next);
        }
        if changed_mode {
            crate::suggest::stop();
            self.search.set_text("");
            self.hide_suggest();
            self.root.vadjustment().set_value(0.0);
        }
        self.dither.queue_draw();
    }

    pub fn queue_dither_redraw(&self) {
        self.dither.queue_draw();
    }

    fn render_links(&self, links: &[Link], mode: StartBookmarksMode) {
        clear(&self.links);
        let app = crate::app::current();
        let per_row = chips_per_row(mode, self.compact.get(), self.narrow.get());
        for chunk in links.chunks(per_row.max(1)) {
            let row_box = gtk::Box::new(gtk::Orientation::Horizontal, BOOKMARK_GAP);
            style(&row_box, "start-link-row");
            row_box.set_halign(gtk::Align::Center);
            for link in chunk {
                let button = gtk::Button::new();
                style(&button, "start-chip");
                style(&button, "start-bookmark");
                if mode == StartBookmarksMode::IconsOnly {
                    style(&button, "start-chip-icon-only");
                    button.set_size_request(44, 44);
                } else if mode == StartBookmarksMode::IconsNamesUrl {
                    style(&button, "start-chip-detailed");
                }
                button.set_relief(gtk::ReliefStyle::None);
                button.set_can_focus(true);
                button.set_tooltip_text(Some(&format!("{}\n{}", link.title, link.url)));
                name(&button, &format!("Open {}. {}", link.title, link.description));
                let icon_px = match mode {
                    StartBookmarksMode::IconsOnly => 18,
                    StartBookmarksMode::IconsAndNames => 16,
                    StartBookmarksMode::IconsNamesUrl => 18,
                };
                let tile_px = icon_px + 4;
                let row = gtk::Box::new(gtk::Orientation::Horizontal, if mode == StartBookmarksMode::IconsOnly { 0 } else { 8 });
                row.set_halign(gtk::Align::Center);
                row.set_valign(gtk::Align::Center);
                if let Some(surface) = app.as_ref().and_then(|a| a.favicon_for_url(&link.url)) {
                    let image = crate::ui::favicon(&surface, icon_px);
                    style(&image, "favicon");
                    image.set_halign(gtk::Align::Center);
                    image.set_valign(gtk::Align::Center);
                    row.pack_start(&image, false, false, 0);
                } else {
                    let tile = monogram_tile(&link.title, &link.url, tile_px);
                    tile.set_halign(gtk::Align::Center);
                    row.pack_start(&tile, false, false, 0);
                }
                match mode {
                    StartBookmarksMode::IconsOnly => {}
                    StartBookmarksMode::IconsAndNames => {
                        let title = label(&link.title, "start-chip-title");
                        title.set_max_width_chars(20);
                        row.pack_start(&title, false, false, 0);
                    }
                    StartBookmarksMode::IconsNamesUrl => {
                        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
                        texts.set_valign(gtk::Align::Center);
                        let title = label(&link.title, "start-chip-title");
                        title.set_max_width_chars(20);
                        let url_lbl = label(&link.description, "start-chip-url");
                        url_lbl.set_max_width_chars(22);
                        texts.pack_start(&title, false, false, 0);
                        texts.pack_start(&url_lbl, false, false, 0);
                        row.pack_start(&texts, false, false, 0);
                    }
                }
                button.add(&row);
                let target = link.url.clone();
                let private = self.private.clone();
                button.connect_clicked(move |_| {
                    if private.get() {
                        return;
                    }
                    if let Some(app) = crate::app::current() {
                        app.navigate_active(&target);
                    }
                });
                let private_ctx = self.private.clone();
                let mode_ctx = self.bookmarks_mode.clone();
                button.connect_button_press_event(move |_, event| {
                    if event.button() == 3 && !private_ctx.get() {
                        open_bookmark_mode_menu(mode_ctx.get(), event);
                        return crate::gtkx::inh(true);
                    }
                    crate::gtkx::inh(false)
                });
                row_box.pack_start(&button, false, false, 0);
            }
            self.links.pack_start(&row_box, false, false, 0);
        }
        self.links.show_all();
    }
}

fn chips_per_row(mode: StartBookmarksMode, compact: bool, narrow: bool) -> usize {
    match mode {
        StartBookmarksMode::IconsOnly => {
            if narrow { 3 } else if compact { 4 } else { 6 }
        }
        StartBookmarksMode::IconsAndNames => {
            if narrow { 2 } else if compact { 3 } else { 4 }
        }
        StartBookmarksMode::IconsNamesUrl => {
            if narrow || compact { 2 } else { 3 }
        }
    }
}

fn open_bookmark_mode_menu(current: StartBookmarksMode, event: &gtk::gdk::EventButton) {
    let menu = gtk::Menu::new();
    crate::app::prepare_popup_menu_pub(&menu);
    let show_shortcuts = crate::app::current()
        .map(|app| app.prefs.borrow().show_start_shortcuts)
        .unwrap_or(true);
    let toggle_label = if show_shortcuts {
        "\u{2713}  Show Homepage Shortcuts"
    } else {
        "    Show Homepage Shortcuts"
    };
    let toggle_item = gtk::MenuItem::with_label(toggle_label);
    toggle_item.connect_activate(move |_| {
        if let Some(app) = crate::app::current() {
            app.set_start_shortcuts_visible(!show_shortcuts);
        }
    });
    menu.append(&toggle_item);
    menu.append(&gtk::SeparatorMenuItem::new());
    for mode in StartBookmarksMode::all() {
        let caption = if mode == current {
            format!("\u{2713}  {}", mode.title())
        } else {
            format!("    {}", mode.title())
        };
        let item = gtk::MenuItem::with_label(&caption);
        item.connect_activate(move |_| {
            if let Some(app) = crate::app::current() {
                app.set_start_bookmarks_mode(mode);
            }
        });
        menu.append(&item);
    }
    menu.append(&gtk::SeparatorMenuItem::new());
    let customize = gtk::MenuItem::with_label("Customize Start Page\u{2026}");
    customize.connect_activate(|_| {
        if let Some(app) = crate::app::current() {
            app.open_panel_filtered(crate::panels::PanelKind::Settings, "Homepage");
        }
    });
    menu.append(&customize);
    let manage = gtk::MenuItem::with_label("Manage Bookmarks\u{2026}");
    manage.connect_activate(|_| {
        if let Some(app) = crate::app::current() {
            app.open_panel(crate::panels::PanelKind::Bookmarks);
        }
    });
    menu.append(&manage);
    menu.show_all();
    menu.popup_easy(event.button(), event.time());
}

fn offer_completion(offer: &crate::omnibox::Offer) -> String {
    if matches!(offer.kind.as_str(), "Search" | "Suggest") {
        offer
            .title
            .strip_prefix("Search for \"")
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(&offer.title)
            .to_string()
    } else {
        offer.url.clone()
    }
}

fn activate_start_offer(app: &crate::app::App, offer: &crate::omnibox::Offer, private: bool) {
    if offer.kind == "Open" {
        let target_tab = {
            let state = app.state.borrow();
            state
                .tabs
                .iter()
                .find(|t| t.url == offer.url && t.private == private)
                .map(|t| t.id)
        };
        if let Some(id) = target_tab {
            app.select(id);
            return;
        }
    }
    if !offer.url.is_empty() {
        app.navigate_active(&offer.url);
    }
}

fn step_rows(rows: &Rc<RefCell<Vec<gtk::EventBox>>>, picked: &Rc<Cell<i32>>, delta: i32) {
    let borrowed = rows.borrow();
    let count = borrowed.len() as i32;
    if count == 0 {
        picked.set(-1);
        return;
    }
    let next = (picked.get() + delta).clamp(-1, count - 1);
    picked.set(next);
    for (idx, row) in borrowed.iter().enumerate() {
        state_class(row, "picked", idx as i32 == next);
    }
}

fn populate_start_suggestions(
    suggest_box: &gtk::Box,
    suggest_list: &gtk::Box,
    offers_slot: &Rc<RefCell<Vec<crate::omnibox::Offer>>>,
    rows_slot: &Rc<RefCell<Vec<gtk::EventBox>>>,
    picked_slot: &Rc<Cell<i32>>,
    raw: &str,
    private: bool,
    preserve_pick: bool,
) {
    let previous_pick = if preserve_pick {
        let idx = picked_slot.get();
        if idx >= 0 {
            offers_slot
                .borrow()
                .get(idx as usize)
                .map(|o| (o.kind.clone(), o.url.clone()))
        } else {
            None
        }
    } else {
        None
    };
    clear(suggest_list);
    rows_slot.borrow_mut().clear();
    picked_slot.set(-1);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        crate::suggest::stop();
        offers_slot.borrow_mut().clear();
        visible(suggest_box, false);
        return;
    }
    let Some(app) = crate::app::current() else {
        offers_slot.borrow_mut().clear();
        visible(suggest_box, false);
        return;
    };
    let items = app.start_offers(trimmed, private);
    if items.is_empty() {
        offers_slot.borrow_mut().clear();
        visible(suggest_box, false);
        return;
    }
    let restored_pick = previous_pick
        .and_then(|(kind, url)| items.iter().position(|o| o.kind == kind && o.url == url))
        .map(|i| i as i32)
        .unwrap_or(-1);
    picked_slot.set(restored_pick);
    *offers_slot.borrow_mut() = items.clone();
    let tint = app.palette.borrow().faint;
    for (idx, offer) in items.into_iter().enumerate() {
        let row = gtk::EventBox::new();
        style(&row, "sgrow");
        style(&row, "start-sgrow");
        if idx as i32 == restored_pick {
            style(&row, "picked");
        }
        row.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        hbox.set_margin_start(10);
        hbox.set_margin_end(10);
        hbox.set_margin_top(6);
        hbox.set_margin_bottom(6);

        let icon = crate::omnibox::build_offer_icon(&app, &offer.url, &offer.kind, tint);
        hbox.pack_start(&icon, false, false, 0);

        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        let title = label(&offer.title, "sgtitle");
        let url_line = crate::omnibox::build_offer_url_box(&offer.url);
        texts.pack_start(&title, false, false, 0);
        texts.pack_start(&url_line, false, false, 0);
        hbox.pack_start(&texts, true, true, 0);

        let kind_text = match offer.kind.as_str() {
            "Open" => "Switch to tab",
            "Bookmark" => "Bookmark",
            "History" => "History",
            "Search" | "Suggest" => "Search",
            other => other,
        };
        let kind = label(kind_text, "sgkind");
        kind.set_valign(gtk::Align::Center);
        hbox.pack_start(&kind, false, false, 0);

        row.add(&hbox);
        let sb_weak = suggest_box.downgrade();
        let picked_click = picked_slot.clone();
        let chosen = offer.clone();
        row.connect_button_press_event(move |_, _| {
            if let Some(sb) = sb_weak.upgrade() {
                visible(&sb, false);
            }
            picked_click.set(-1);
            if let Some(app) = crate::app::current() {
                activate_start_offer(&app, &chosen, private);
            }
            crate::gtkx::inh(true)
        });
        rows_slot.borrow_mut().push(row.clone());
        suggest_list.pack_start(&row, false, true, 0);
    }
    visible(suggest_box, true);
}

fn reflow_link_rows(links: &gtk::Box, per_row: usize) {
    let mut buttons: Vec<gtk::Widget> = Vec::new();
    for row in links.children() {
        if let Ok(row_box) = row.clone().downcast::<gtk::Box>() {
            for child in row_box.children() {
                row_box.remove(&child);
                buttons.push(child);
            }
        }
        links.remove(&row);
    }
    if buttons.is_empty() {
        return;
    }
    for chunk in buttons.chunks(per_row.max(1)) {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, BOOKMARK_GAP);
        style(&row_box, "start-link-row");
        row_box.set_halign(gtk::Align::Center);
        for button in chunk {
            row_box.pack_start(button, false, false, 0);
        }
        links.pack_start(&row_box, false, false, 0);
    }
    links.show_all();
}

#[allow(dead_code)]
pub(crate) fn dither_drawing_area(
    initial_dark: bool,
    private: Rc<Cell<bool>>,
) -> gtk::DrawingArea {
    dither_drawing_area_with_mode(initial_dark, private, Rc::new(Cell::new(StartDitherMode::Full)))
}

fn dither_drawing_area_with_mode(
    initial_dark: bool,
    private: Rc<Cell<bool>>,
    mode: Rc<Cell<StartDitherMode>>,
) -> gtk::DrawingArea {
    let dither = gtk::DrawingArea::new();
    dither.set_hexpand(true);
    dither.set_vexpand(true);
    dither.set_can_focus(false);
    if let Some(accessible) = dither.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&accessible, gtk::atk::Role::RedundantObject);
    }
    let cache: Rc<RefCell<Option<DitherCache>>> = Rc::new(RefCell::new(None));
    dither.connect_style_updated(|widget| {
        widget.queue_draw();
    });
    dither.connect_draw(move |widget, cr| {
        let width = widget.allocated_width();
        let height = widget.allocated_height();
        if width < 8 || height < 8 {
            return crate::gtkx::inh(false);
        }
        let dither_mode = mode.get();
        if dither_mode == StartDitherMode::Off {
            return crate::gtkx::inh(false);
        }
        let dark = widget_is_dark(widget, initial_dark);
        let is_private = private.get();
        let key = (width, height, dark, is_private, dither_mode);
        let mut slot = cache.borrow_mut();
        let needs_build = slot.as_ref().is_none_or(|cached| cached.key != key);
        if needs_build {
            if let Some(surface) = build_dither_surface_mode(width, height, dark, is_private, dither_mode) {
                *slot = Some(DitherCache { key, surface });
            }
        }
        if let Some(cached) = slot.as_ref() {
            let _ = cr.save();
            let r = 15.0_f64.min(width as f64 * 0.5).min(height as f64 * 0.5);
            let wf = width as f64;
            let hf = height as f64;
            cr.new_sub_path();
            cr.arc(wf - r, r, r, -std::f64::consts::FRAC_PI_2, 0.0);
            cr.arc(wf - r, hf - r, r, 0.0, std::f64::consts::FRAC_PI_2);
            cr.arc(r, hf - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
            cr.arc(r, r, r, std::f64::consts::PI, 1.5 * std::f64::consts::PI);
            cr.close_path();
            cr.clip();
            if cr.set_source_surface(&cached.surface, 0.0, 0.0).is_ok() {
                cr.source().set_filter(cairo::Filter::Nearest);
                let _ = cr.paint();
            }
            let _ = cr.restore();
        }
        crate::gtkx::inh(false)
    });
    dither
}

/// Read the live theme polarity from the toplevel window's `.dark` / `.light`
/// class (set by `App::apply_style`), falling back to the initial palette.
pub(crate) fn widget_is_dark(widget: &impl IsA<gtk::Widget>, fallback: bool) -> bool {
    if let Some(top) = widget.toplevel() {
        let ctx = top.style_context();
        if ctx.has_class("dark") {
            return true;
        }
        if ctx.has_class("light") {
            return false;
        }
    }
    fallback
}

/// Palette stops for the start page's 3-level ordered Bayer dither field:
/// `(mid_tone, core_tone)` painted in discrete 2x2 cells over `palette.card`.
/// Regular mode blends warm stone into a restrained terracotta whisper;
/// Private mode uses cool monochrome ash-stone with no accent.
pub(crate) fn dither_stops(palette: &Palette, private: bool) -> (Rgb, Rgb) {
    let dark = palette.is_dark();
    if private {
        let mid = palette.card.mix(palette.hairline, if dark { 0.54 } else { 0.56 });
        let core = palette.card.mix(palette.muted, if dark { 0.34 } else { 0.34 });
        (mid, core)
    } else {
        let mid = palette.card.mix(palette.hairline, if dark { 0.58 } else { 0.62 });
        let core = palette.card.mix(palette.accent(), if dark { 0.36 } else { 0.35 });
        (mid, core)
    }
}

/// Continuous rising-horizon density in `[0.0, 1.0]` for normalized coordinates
/// `nx, ny` in `[0.0, 1.0]`. Covers ~62–70% of the visible homepage height in a
/// broad architectural dome so ordered Bayer halftone clearly reads across more
/// than half the page in light, dark, wide, compact, and short windows, while
/// keeping the uppermost ~29% calm and clear.
pub(crate) fn horizon_density(nx: f32, ny: f32) -> f32 {
    let dx = (nx - 0.5) * 2.0; // -1.0 .. 1.0
    // Broad shallow dome peaking in the center at y=0.30 (70% coverage) and
    // curving gently to y=0.38 near the sides (62% coverage; ~67.3% mean area).
    let crest = 0.30 + 0.08 * dx * dx;
    if ny <= crest {
        return 0.0;
    }
    let t = ((ny - crest) / (1.0 - crest)).clamp(0.0, 1.0);
    // Short feathered onset avoids a ruler-straight seam while making visible
    // ordered Bayer pixels appear promptly across the upper-mid canvas.
    let onset = ((ny - crest) / 0.045).clamp(0.0, 1.0);
    ((0.15 + t.powf(1.12) * 0.77) * onset).clamp(0.0, 0.94)
}

/// Evaluate the 3-stop complementary-phase 8x8 Bayer ordered dither at cell
/// `(cx, cy)`. Uses a half-period phase offset for the warm core channel so
/// terracotta/accent dots weave smoothly into the halftone without ever
/// saturating into a flat horizontal mid-tone band.
/// Returns `0` (transparent / background), `1` (mid stone), or `2` (core tone).
pub(crate) fn bayer_level(cx: usize, cy: usize, density: f32) -> u8 {
    if density <= 0.0 {
        return 0;
    }
    let b_mid = (BAYER_8X8[cy & 7][cx & 7] as f32 + 0.5) / 64.0;
    let b_core = (BAYER_8X8[(cy + 4) & 7][(cx + 4) & 7] as f32 + 0.5) / 64.0;
    let stone_d = (density * 1.06).clamp(0.0, 0.86);
    let core_d = ((density - 0.26) / 0.74).clamp(0.0, 1.0).powf(1.22) * 0.64;
    if core_d > b_core {
        2
    } else if stone_d > b_mid {
        1
    } else {
        0
    }
}

fn premul_argb(c: Rgb) -> u32 {
    0xFF00_0000 | ((c.0 as u32) << 16) | ((c.1 as u32) << 8) | (c.2 as u32)
}

#[allow(dead_code)]
fn build_dither_surface(
    width: i32,
    height: i32,
    dark: bool,
    private: bool,
) -> Option<cairo::ImageSurface> {
    build_dither_surface_mode(width, height, dark, private, StartDitherMode::Full)
}

fn build_dither_surface_mode(
    width: i32,
    height: i32,
    dark: bool,
    private: bool,
    mode: StartDitherMode,
) -> Option<cairo::ImageSurface> {
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    if mode == StartDitherMode::Off {
        surface.mark_dirty();
        return Some(surface);
    }
    let palette = Palette::for_look(if dark { Look::Dark } else { Look::Light }, false);
    let (mid, core) = dither_stops(&palette, private);
    let mid_px = premul_argb(mid);
    let core_px = premul_argb(core);

    let stride = surface.stride() as usize;
    let w = width as usize;
    let h = height as usize;
    let cell = DITHER_CELL as usize;
    let scale = match mode {
        StartDitherMode::Off => 0.0,
        StartDitherMode::Subtle => 0.45,
        StartDitherMode::Full => 1.0,
    };

    {
        let mut data = surface.data().ok()?;
        let inv_w = 1.0 / (w.max(1) as f32);
        let inv_h = 1.0 / (h.max(1) as f32);

        let mut y = 0usize;
        while y < h {
            let cy = y / cell;
            let ny = (y as f32 + 0.5 * cell as f32) * inv_h;
            if ny <= 0.29 {
                y += cell;
                continue;
            }
            let y_end = (y + cell).min(h);
            let mut x = 0usize;
            while x < w {
                let cx = x / cell;
                let nx = (x as f32 + 0.5 * cell as f32) * inv_w;
                let density = horizon_density(nx, ny) * scale;
                let level = bayer_level(cx, cy, density);
                if level != 0 {
                    let px = if level == 2 { core_px } else { mid_px };
                    let bytes = px.to_ne_bytes();
                    let x_end = (x + cell).min(w);
                    for py in y..y_end {
                        let row_off = py * stride;
                        for px_x in x..x_end {
                            let idx = row_off + px_x * 4;
                            data[idx..idx + 4].copy_from_slice(&bytes);
                        }
                    }
                }
                x += cell;
            }
            y += cell;
        }
    }

    surface.mark_dirty();
    Some(surface)
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
    let label = gtk::Label::new(Some(text));
    style(&label, class);
    label.set_xalign(0.0);
    label.set_yalign(0.5);
    label.set_single_line_mode(true);
    if let Some(accessible) = label.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&accessible, gtk::atk::Role::Heading);
    }
    label
}

fn monogram_tile(title: &str, url: &str, size: i32) -> gtk::Box {
    let initial = monogram_initial(title, url);
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 0);
    style(&tile, "tile");
    tile.set_size_request(size, size);
    tile.set_valign(gtk::Align::Center);
    let letter = gtk::Label::new(Some(&initial));
    style(&letter, "tileletter");
    letter.set_halign(gtk::Align::Center);
    letter.set_valign(gtk::Align::Center);
    letter.set_vexpand(true);
    tile.pack_start(&letter, true, true, 0);
    tile
}

fn monogram_initial(title: &str, url: &str) -> String {
    let host_text = host_label(url);
    let host_bare = host_text.trim_start_matches("www.");
    // An IP address, localhost or a local file gives every page the same tile
    // ("1" for each 127.0.0.1 page), which tells sites apart no better than a
    // generic icon. For those hosts the tile takes the page title's first
    // letter instead, and falls back to the host when the title has none.
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
    (if local { first(title) } else { None })
        .or_else(|| first(host_bare))
        .unwrap_or_else(|| "\u{00b7}".to_string())
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

fn clear(container: &impl IsA<gtk::Container>) {
    for child in container.children() {
        container.remove(&child);
    }
}

#[allow(dead_code)]
fn bookmark_links(bookmarks: &[crate::store::Bookmark]) -> Vec<Link> {
    bookmark_links_capped(bookmarks, MAX_START_BOOKMARKS)
}

fn bookmark_links_capped(bookmarks: &[crate::store::Bookmark], limit: usize) -> Vec<Link> {
    let cap = limit.clamp(
        crate::store::MIN_START_SHORTCUTS_COUNT,
        crate::store::MAX_START_SHORTCUTS_COUNT,
    );
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
            description: place_label(&bookmark.url),
        });
        if links.len() == cap {
            break;
        }
    }
    links
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

fn place_label(url: &str) -> String {
    let host = host_label(url);
    if host == "Local file" {
        return host;
    }
    let location = crate::shield::location_of(url.trim());
    if location == "/" {
        host
    } else {
        format!("{host}{location}")
    }
}

fn page_title(title: &str, url: &str) -> String {
    let title = title.trim();
    if title.is_empty() || title == url.trim() {
        host_label(url)
    } else {
        title.chars().take(160).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Bookmark;

    #[test]
    fn bookmarks_are_deduplicated_and_capped_without_unsafe_schemes() {
        let mut bookmarks = vec![
            Bookmark { title: "One".into(), url: "https://EXAMPLE.com/".into(), folder: String::new() },
            Bookmark { title: "Duplicate".into(), url: "https://example.com/#section".into(), folder: String::new() },
            Bookmark { title: "Script".into(), url: "javascript:alert(1)".into(), folder: String::new() },
        ];
        for index in 0..10 {
            bookmarks.push(Bookmark {
                title: format!("Page {index}"),
                url: format!("https://example.com/{index}"),
                folder: String::new(),
            });
        }
        let links = bookmark_links(&bookmarks);
        assert_eq!(links.len(), MAX_START_BOOKMARKS);
        assert_eq!(links[0].title, "One");
        assert!(!links.iter().any(|link| link.title == "Duplicate" || link.title == "Script"));
    }

    #[test]
    fn bookmark_links_distinguish_same_host_paths() {
        let bookmarks = vec![
            Bookmark { title: "Office Commun".into(), url: "http://127.0.0.1:8080/office".into(), folder: String::new() },
            Bookmark { title: "The Long Read".into(), url: "http://127.0.0.1:8080/long".into(), folder: String::new() },
            Bookmark { title: "Root".into(), url: "https://www.example.com/".into(), folder: String::new() },
            Bookmark { title: "Local".into(), url: "file:///tmp/notes.html".into(), folder: String::new() },
        ];
        let links = bookmark_links(&bookmarks);
        assert_eq!(links.len(), 4);
        assert_eq!(links[0].description, "127.0.0.1/office");
        assert_eq!(links[1].description, "127.0.0.1/long");
        assert_eq!(links[2].description, "example.com");
        assert_eq!(links[3].description, "Local file");
    }

    #[test]
    fn fallback_titles_show_the_actual_host() {
        assert_eq!(page_title("", "https://www.example.com/a"), "example.com");
        assert_eq!(page_title("https://www.example.com/a", "https://www.example.com/a"), "example.com");
    }

    #[test]
    fn monograms_use_host_for_public_sites_and_title_for_local_pages() {
        assert_eq!(monogram_initial("GitHub", "https://github.com/"), "G");
        assert_eq!(monogram_initial("Wikipedia", "https://www.wikipedia.org/"), "W");
        assert_eq!(monogram_initial("Office Commun", "http://127.0.0.1:8080/office"), "O");
        assert_eq!(monogram_initial("The Long Read", "http://localhost:3000/long"), "T");
        assert_eq!(monogram_initial("Notes", "file:///tmp/notes.html"), "N");
    }

    #[test]
    fn bayer_matrix_contains_every_threshold_once() {
        let mut seen = [false; 64];
        for row in BAYER_8X8 {
            for val in row {
                assert!((val as usize) < 64);
                assert!(!seen[val as usize], "duplicate Bayer threshold {val}");
                seen[val as usize] = true;
            }
        }
        assert!(seen.into_iter().all(|ok| ok));
    }

    #[test]
    fn dither_horizon_covers_more_than_half_canvas_while_keeping_upper_band_clear() {
        // Upper ~28% stays strictly clear at the top edge.
        for x in [0.05_f32, 0.25, 0.5, 0.75, 0.95] {
            assert_eq!(horizon_density(x, 0.10), 0.0);
            assert_eq!(horizon_density(x, 0.25), 0.0);
        }
        // Center horizon begins well above 50% (around y=0.30..0.38 -> 62-70% height).
        assert!(horizon_density(0.5, 0.36) > 0.10);
        assert!(horizon_density(0.5, 0.50) > 0.25);
        // Triggers all 3 discrete Bayer levels (0, 1, 2).
        let mut saw_0 = false;
        let mut saw_1 = false;
        let mut saw_2 = false;
        for cy in 0..16 {
            for cx in 0..16 {
                let mid = bayer_level(cx, cy, horizon_density(0.5, 0.42));
                let deep = bayer_level(cx, cy, horizon_density(0.5, 0.94));
                if mid == 0 { saw_0 = true; }
                if mid == 1 || deep == 1 { saw_1 = true; }
                if deep == 2 { saw_2 = true; }
            }
        }
        assert!(saw_0 && saw_1 && saw_2, "ordered dither must produce 3 discrete halftone levels");
    }

    #[test]
    fn dither_surface_renders_clear_top_and_over_60_percent_visible_dither_coverage() {
        for (dark, private) in [(false, false), (true, false), (false, true), (true, true)] {
            let mut surf = build_dither_surface(320, 240, dark, private)
                .expect("dither surface builds");
            let stride = surf.stride() as usize;
            let data = surf.data().expect("surface pixel access");
            // Top band (y=24) must stay completely clear.
            let top_alpha = data[24 * stride + 8 * 4 + 3];
            assert_eq!(top_alpha, 0, "upper canvas stayed clear (dark={dark}, private={private})");
            // Measure fraction of vertical rows containing real visible dither pixels.
            let mut dithered_rows = 0usize;
            for y in 0..240 {
                let row_lit = (0..320)
                    .step_by(2)
                    .filter(|&x| data[y * stride + x * 4 + 3] > 0)
                    .count();
                if row_lit >= 6 {
                    dithered_rows += 1;
                }
            }
            let coverage = dithered_rows as f64 / 240.0;
            assert!(
                (0.58..=0.74).contains(&coverage),
                "expected 58-74% visible dither height coverage, got {:.1}% (dark={dark}, private={private})",
                coverage * 100.0
            );
            // Mid and lower bands must contain genuine multi-tone dither (not flat fill).
            let mut distinct = HashSet::new();
            for x in (0..320).step_by(2) {
                let idx = 200 * stride + x * 4;
                distinct.insert([data[idx], data[idx + 1], data[idx + 2], data[idx + 3]]);
            }
            assert!(distinct.len() >= 2, "horizon is genuine multi-tone dither, not a flat fill");
        }
    }

    #[test]
    fn dither_modes_off_subtle_and_full_differ_in_density() {
        let count_lit = |mode: StartDitherMode| -> usize {
            let mut surf = build_dither_surface_mode(240, 200, false, false, mode).unwrap();
            let stride = surf.stride() as usize;
            let data = surf.data().unwrap();
            let mut lit = 0usize;
            for y in (0..200).step_by(2) {
                for x in (0..240).step_by(2) {
                    if data[y * stride + x * 4 + 3] > 0 {
                        lit += 1;
                    }
                }
            }
            lit
        };
        let off = count_lit(StartDitherMode::Off);
        let subtle = count_lit(StartDitherMode::Subtle);
        let full = count_lit(StartDitherMode::Full);
        assert_eq!(off, 0);
        assert!(subtle > 0 && subtle < full, "expected 0 < {subtle} < {full}");
    }

    #[test]
    fn bookmark_links_cap_honors_configured_shortcut_count() {
        let bookmarks: Vec<Bookmark> = (0..16)
            .map(|i| Bookmark {
                title: format!("Site {i}"),
                url: format!("https://site{i}.example/"),
                folder: String::new(),
            })
            .collect();
        assert_eq!(bookmark_links_capped(&bookmarks, 4).len(), 4);
        assert_eq!(bookmark_links_capped(&bookmarks, 12).len(), 12);
        assert_eq!(bookmark_links_capped(&bookmarks, 99).len(), crate::store::MAX_START_SHORTCUTS_COUNT);
    }
}
