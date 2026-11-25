// The window as it is drawn: a column of tabs with the page beside it, and
// the few things that appear over the page — the field, find, panels.
//
// Nothing here knows what a tab is; it makes widgets, and the browser keeps
// them true.
use crate::icons::{self, Icon};
use crate::palette::Palette;
use gtk::prelude::*;

pub struct Ui {
    pub window: gtk::Window,
    pub overlay: gtk::Overlay,
    pub shell: gtk::Box,
    pub paned: gtk::Paned,
    /// The column: the tab rows and their foot, with the hairline on its
    /// right edge.
    pub side: gtk::Box,
    /// What is left of the column when it is folded (Ctrl S): a thin strip,
    /// clicked to unfold. `body` holds everything else in the column.
    pub strip: gtk::EventBox,
    pub body: gtk::Box,
    /// The whole band at the top of the column, which is the window's handle:
    /// a titlebar that is not drawn still has to be somewhere to grab.
    pub top: gtk::EventBox,
    pub toolbar: gtk::EventBox,
    pub fold: gtk::Button,
    pub address: gtk::Button,
    pub address_label: gtk::Label,
    /// The path after the trusted origin chip. Kept as a separate, quiet
    /// label so a path cannot be mistaken for the site's authority.
    pub address_path: gtk::Label,
    pub address_hint: gtk::Label,
    pub site_info: gtk::Button,
    pub reader: gtk::Button,
    pub bookmark: gtk::Button,
    pub more: gtk::Button,
    pub newtab: gtk::Button,
    pub private_tab: gtk::Button,
    pub library: gtk::Button,
    pub history: gtk::Button,
    pub downloads: gtk::Button,
    pub tab_count: gtk::Label,
    pub start: crate::startpage::StartPage,
    pub light_red: gtk::EventBox,
    pub light_amber: gtk::EventBox,
    pub light_green: gtk::EventBox,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub reload: gtk::Button,
    pub pins: gtk::Grid,
    pub pins_holder: gtk::Box,
    pub rows: gtk::Box,
    /// The scrolled window the rows live in, so a tab switch can bring the row
    /// it switched to into view. Without it, Ctrl Tab past the bottom of the
    /// list changes the page and leaves the highlight somewhere off-screen.
    pub rows_scroll: gtk::ScrolledWindow,
    pub menu: gtk::Button,
    pub stack: gtk::Stack,
    pub progress: gtk::DrawingArea,
    pub progress_frac: std::rc::Rc<std::cell::Cell<f64>>,
    /// The row of kept pages under the top band. A real browser keeps them one
    /// click away; this one does too, and the row is only visible when there is
    /// something in it or when it was asked for.
    pub bmarkbar: gtk::EventBox,
    pub bmarks: gtk::Box,
    /// The error page's wrapper. A Box has no window of its own, so a press on
    /// the apology went to whatever was behind it; this EventBox is the widget
    /// that can receive the click and is the child the stack holds as "error".
    pub error_holder: gtk::EventBox,
    pub errorpage: gtk::Box,
    pub error_title: gtk::Label,
    pub error_reason: gtk::Label,
    pub error_address: gtk::Label,
    pub error_retry: gtk::Button,
    pub error_back: gtk::Button,
}

fn widget_box(orientation: gtk::Orientation, spacing: i32) -> gtk::Box {
    gtk::Box::new(orientation, spacing)
}

/// Give a widget one of the browser's classes. Every style in this browser is
/// a class in one stylesheet, so this is the only door between code and look.
pub fn class<W: IsA<gtk::Widget>>(widget: &W, name: &str) -> gtk::StyleContext {
    let context = widget.style_context();
    context.add_class(name);
    context
}

/// Add a child to a box with the expansion it should have. GtkBox's own
/// container-add stretches everything it is given, which is never what a
/// column of fixed rows and one page wants.
pub fn pack<B: IsA<gtk::Box>, C: IsA<gtk::Widget>>(container: &B, child: &C, expand: bool) {
    container.pack_start(child, expand, true, 0);
}


/// Width of the column strip Ctrl S leaves behind. Clicking it unfolds the column.
pub const STRIP_WIDTH: i32 = 10;

/// Minimum width of an unfolded tab column so users can drag it narrower without
/// being locked to the saved width.
pub const MIN_COL: i32 = 200;

fn label_a11y<W: IsA<gtk::Widget>>(widget: &W, name: &str, role: gtk::atk::Role) {
    widget.set_tooltip_text(Some(name));
    if let Some(access) = widget.accessible() {
        // Fully qualified: gtk's prelude brings several traits with a
        // `set_name`, and this one is atk's.
        gtk::atk::prelude::AtkObjectExt::set_name(&access, name);
        gtk::atk::prelude::AtkObjectExt::set_role(&access, role);
    }
}

/// What the column looks like for a fold state and a page full-screen state.
/// Fold and full screen are independent: a page leaving full screen restores
/// whatever the fold state is, and never opens a folded column.
#[derive(Debug, PartialEq, Eq)]
pub struct ColumnPlan {
    /// False while a page is full screen: the column is hidden, not folded.
    pub side_shown: bool,
    /// Width request of the whole column.
    pub width: i32,
    /// Where the paned divider sits.
    pub paned_at: i32,
    pub strip: bool,
    pub body: bool,
}

pub fn column_plan(folded: bool, page_full: bool, sidebar: bool, side_width: i32) -> ColumnPlan {
    let width = match (sidebar, folded) {
        (false, _) => 0,
        (true, true) => STRIP_WIDTH,
        (true, false) => side_width,
    };
    ColumnPlan {
        side_shown: !page_full,
        width,
        paned_at: if folded { STRIP_WIDTH } else { side_width },
        strip: folded && sidebar,
        body: !folded,
    }
}

pub fn build(window: gtk::Window, palette: Palette, prefs: &crate::store::Prefs) -> Ui {
    let overlay = gtk::Overlay::new();

    // The column and the page, with the column's edge between them: a paned
    // window keeps the width the column was given, and lets it be dragged.
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    crate::ui::class(&paned, "shell");
    paned.set_wide_handle(false);
    paned.set_hexpand(true);
    paned.set_vexpand(true);

    let side_width = prefs.side_width.round() as i32;
    let side = widget_box(gtk::Orientation::Vertical, 0);
    crate::ui::class(&side, "side");
    let plan = column_plan(prefs.column_folded, false, prefs.sidebar, side_width);
    let min_width = if !prefs.sidebar {
        0
    } else if prefs.column_folded {
        STRIP_WIDTH
    } else {
        MIN_COL
    };
    side.set_size_request(min_width, -1);
    let strip = gtk::EventBox::new();
    crate::ui::class(&strip, "colstrip");
    strip.set_visible_window(true);
    strip.add_events(
        gtk::gdk::EventMask::BUTTON_PRESS_MASK
            | gtk::gdk::EventMask::ENTER_NOTIFY_MASK
            | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK,
    );
    strip.set_size_request(STRIP_WIDTH, -1);
    strip.set_no_show_all(true);
    strip.set_visible(plan.strip);
    label_a11y(&strip, "Show tab column", gtk::atk::Role::PushButton);
    pack(&side, &strip, true);
    let body = widget_box(gtk::Orientation::Vertical, 0);
    // Opted out of show_all, so the window's show_all cannot unfold a column
    // saved as folded. apply_column shows the body's own subtree when unfolded.
    body.set_no_show_all(true);
    body.set_visible(plan.body);
    pack(&side, &body, true);

    let top = gtk::EventBox::new();
    crate::ui::class(&top, "topband");
    top.set_size_request(-1, 34);
    top.add_events(gtk::gdk::EventMask::ENTER_NOTIFY_MASK | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK);
    let band = widget_box(gtk::Orientation::Horizontal, 10);
    // Brand at the 24px text edge, the same edge as the section labels and
    // the row content below it. At 20px it was the only item off that line.
    band.set_margin_start(24);
    band.set_margin_end(18);
    top.add(&band);
    let brand_mark = gtk::EventBox::new();
    class(&brand_mark, "brand-mark");
    brand_mark.set_size_request(30, 30);
    brand_mark.set_valign(gtk::Align::Center);
    let glyph = icons::area(Icon::Orbit, 20, palette.accent_ink());
    glyph.set_halign(gtk::Align::Center);
    glyph.set_valign(gtk::Align::Center);
    brand_mark.add(&glyph);
    pack(&band, &brand_mark, false);
    let brand = gtk::Label::new(Some("search"));
    class(&brand, "brand-word");
    brand.set_xalign(0.0);
    pack(&band, &brand, true);
    pack(&body, &top, false);

    let create = widget_box(gtk::Orientation::Horizontal, 6);
    // The button is a surface, so it starts on the 12px surface edge, like
    // the pin tiles and the row fills. At 16px it sat between the two guides.
    create.set_margin_start(12);
    create.set_margin_end(12);
    create.set_margin_bottom(12);
    let newtab = gtk::Button::new();
    class(&newtab, "create-tab");
    newtab.set_hexpand(true);
    let create_content = widget_box(gtk::Orientation::Horizontal, 9);
    let create_icon = icons::area(Icon::Plus, 17, palette.accent_ink());
    create_icon.set_valign(gtk::Align::Center);
    pack(&create_content, &create_icon, false);
    let create_label = gtk::Label::new(Some("New tab"));
    create_label.set_xalign(0.0);
    pack(&create_content, &create_label, true);
    let create_hint = gtk::Label::new(Some("Ctrl T"));
    class(&create_hint, "create-hint");
    create_hint.set_no_show_all(true);
    create_hint.hide();
    pack(&create_content, &create_hint, false);
    newtab.add(&create_content);
    label_a11y(&newtab, "New tab (Ctrl T)", gtk::atk::Role::PushButton);
    let private_tab = icons::button(Icon::Mask, 17, palette.muted, "private-tab");
    private_tab.set_can_focus(true);
    label_a11y(&private_tab, "New private tab (Ctrl Shift N)", gtk::atk::Role::PushButton);
    private_tab.set_no_show_all(true);
    private_tab.hide();
    pack(&create, &newtab, true);
    pack(&create, &private_tab, false);
    pack(&body, &create, false);

    // Pinned pages: squares that keep their place.
    // One gutter for the whole column. Surfaces (pin tiles, row fills) start
    // 12px from the edge. Text labels start 24px in, which lines them up with
    // the row content (12px row margin plus the row's own 12px inset). Before
    // this, pins used 16px, section labels 20px and rows 12px, so no two
    // edges in the column lined up.
    let pins_holder = widget_box(gtk::Orientation::Vertical, 0);
    pins_holder.set_margin_start(12);
    pins_holder.set_margin_end(12);
    pins_holder.set_margin_bottom(18);
    // Sentence case, like the panel and settings headings. The 12px label
    // already sets the section apart without being written in capitals.
    let pin_heading = gtk::Label::new(Some("Pinned"));
    class(&pin_heading, "sidebar-section");
    pin_heading.set_xalign(0.0);
    pin_heading.set_margin_start(12);
    pin_heading.set_margin_bottom(8);
    pin_heading.set_no_show_all(true);
    pin_heading.hide();
    pack(&pins_holder, &pin_heading, false);
    let pins = gtk::Grid::new();
    pins.set_column_spacing(3);
    pins.set_row_spacing(4);
    pins.set_column_homogeneous(false);
    pins.set_halign(gtk::Align::Start);
    pack(&pins_holder, &pins, false);
    pins_holder.set_no_show_all(true);
    pack(&body, &pins_holder, false);

    let section = widget_box(gtk::Orientation::Horizontal, 6);
    // Label at the 24px text edge, so it lines up with the row content below.
    // The count stays 20px from the right edge, in line with the row's close
    // button (12px row margin plus the 8px end inset).
    section.set_margin_start(24);
    section.set_margin_end(20);
    section.set_margin_bottom(8);
    let tabs_heading = gtk::Label::new(Some("Open tabs"));
    class(&tabs_heading, "sidebar-section");
    tabs_heading.set_xalign(0.0);
    tabs_heading.set_no_show_all(true);
    tabs_heading.hide();
    pack(&section, &tabs_heading, true);
    let tab_count = gtk::Label::new(Some("01"));
    class(&tab_count, "sidebar-count");
    tab_count.set_no_show_all(true);
    tab_count.hide();
    pack(&section, &tab_count, false);
    pack(&body, &section, false);

    // The rows.
    let rows = widget_box(gtk::Orientation::Vertical, 4);
    rows.set_margin_start(12);
    rows.set_margin_end(12);
    let rows_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    rows_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    rows_scroll.set_vexpand(true);
    rows_scroll.set_propagate_natural_height(false);
    rows_scroll.set_propagate_natural_width(false);
    rows_scroll.set_min_content_width(0);
    rows.set_size_request(0, -1);
    rows_scroll.add(&rows);
    pack(&body, &rows_scroll, true);

    let footer = widget_box(gtk::Orientation::Horizontal, 4);
    class(&footer, "sidebar-footer");
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    footer.set_margin_bottom(14);
    let library = icons::button(Icon::Book, 17, palette.muted, "sidebar-tool");
    label_a11y(&library, "Library (Ctrl Shift B)", gtk::atk::Role::PushButton);
    let history = icons::button(Icon::Clock, 17, palette.muted, "sidebar-tool");
    label_a11y(&history, "History (Ctrl H)", gtk::atk::Role::PushButton);
    let downloads = icons::button(Icon::Download, 17, palette.muted, "sidebar-tool");
    label_a11y(&downloads, "Downloads (Ctrl J)", gtk::atk::Role::PushButton);
    let menu = icons::button(Icon::Settings, 17, palette.muted, "sidebar-tool");
    label_a11y(&menu, "Settings and more", gtk::atk::Role::PushButton);
    library.set_can_focus(true);
    history.set_can_focus(true);
    downloads.set_can_focus(true);
    menu.set_can_focus(true);
    pack(&footer, &library, false);
    pack(&footer, &history, false);
    pack(&footer, &downloads, false);
    pack(&footer, &menu, false);
    pack(&body, &footer, false);

    // The column is the paned's first child itself, with no wrapper around
    // it. `.side` draws the hairline as its own right border, so the line is
    // exactly as tall as the widget; wrapped, the 23px strip above the column
    // was outside that border and the line began at y=24 instead of at the
    // band's edge. Unwrapped it runs from the band to the window's bottom.
    paned.pack1(&side, false, false);
    paned.set_position(plan.paned_at);

    // The pages you kept, in one row, above the page and below the column's
    // top band. Hidden unless it has something to show.
    let bmarkbar = gtk::EventBox::new();
    crate::ui::class(&bmarkbar, "bmarkbar");
    bmarkbar.set_visible_window(true);
    bmarkbar.set_halign(gtk::Align::Fill);
    let bmarks = widget_box(gtk::Orientation::Horizontal, 2);
    bmarks.set_margin_start(10);
    bmarks.set_margin_end(10);
    bmarks.set_margin_top(3);
    bmarks.set_margin_bottom(3);
    let bmarks_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    bmarks_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    bmarks_scroll.set_propagate_natural_height(true);
    bmarks_scroll.add(&bmarks);
    bmarkbar.add(&bmarks_scroll);
    // Hidden by default, and hidden again by hide() rather than by
    // no_show_all: a widget marked no_show_all is skipped by show_all, which
    // is how the pins disappeared once.
    bmarkbar.hide();

    // The page, edge to edge.
    let card = widget_box(gtk::Orientation::Vertical, 0);
    card.set_hexpand(true);
    card.set_vexpand(true);
    crate::ui::class(&card, "card");

    let toolbar = gtk::EventBox::new();
    class(&toolbar, "toolbar");
    toolbar.set_size_request(-1, 72);
    let tools = widget_box(gtk::Orientation::Horizontal, 6);
    tools.set_margin_start(12);
    tools.set_margin_end(14);
    toolbar.add(&tools);
    let fold = icons::button(Icon::Sidebar, 18, palette.muted, "chrome-tool");
    fold.set_valign(gtk::Align::Center);
    label_a11y(&fold, "Toggle tab column (Ctrl S)", gtk::atk::Role::PushButton);
    pack(&tools, &fold, false);
    let helm = widget_box(gtk::Orientation::Horizontal, 0);
    helm.set_valign(gtk::Align::Center);
    class(&helm, "helm");
    let back = icons::button(Icon::Back, 17, palette.muted, "chrome-tool");
    label_a11y(&back, "Back (Alt Left)", gtk::atk::Role::PushButton);
    let forward = icons::button(Icon::Forward, 17, palette.muted, "chrome-tool");
    label_a11y(&forward, "Forward (Alt Right)", gtk::atk::Role::PushButton);
    let reload = icons::button(Icon::Reload, 17, palette.muted, "chrome-tool");
    label_a11y(&reload, "Reload (Ctrl R)", gtk::atk::Role::PushButton);
    for button in [&fold, &back, &forward, &reload] {
        button.set_can_focus(true);
        button.set_focus_on_click(false);
    }
    for button in [&forward, &reload] {
        button.show_all();
        button.set_no_show_all(true);
    }
    pack(&helm, &back, false);
    pack(&helm, &forward, false);
    pack(&helm, &reload, false);
    pack(&tools, &helm, false);

    let location = widget_box(gtk::Orientation::Horizontal, 0);
    class(&location, "location");
    location.set_valign(gtk::Align::Center);
    location.set_margin_start(8);
    location.set_margin_end(8);
    let site_info = icons::button(Icon::Shield, 16, palette.accent(), "site-info");
    label_a11y(&site_info, "Site information (Ctrl I)", gtk::atk::Role::PushButton);
    site_info.set_can_focus(true);
    let address = gtk::Button::new();
    class(&address, "address-button");
    address.set_hexpand(true);
    label_a11y(&address, "Search or enter an address (Ctrl L)", gtk::atk::Role::PushButton);
    let address_content = widget_box(gtk::Orientation::Horizontal, 10);
    let address_label = gtk::Label::new(Some("New tab"));
    class(&address_label, "address-label");
    address_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    address_label.set_max_width_chars(24);
    address_label.set_xalign(0.0);
    pack(&address_content, &address_label, true);
    let address_path = gtk::Label::new(Some(""));
    class(&address_path, "address-path");
    address_path.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    address_path.set_max_width_chars(22);
    address_path.set_xalign(0.0);
    pack(&address_content, &address_path, false);
    let address_hint = gtk::Label::new(Some("Ctrl L"));
    class(&address_hint, "address-hint");
    pack(&address_content, &address_hint, false);
    address.add(&address_content);
    pack(&location, &site_info, false);
    pack(&location, &address, true);
    pack(&tools, &location, true);

    let reader = icons::button(Icon::Book, 18, palette.muted, "chrome-tool");
    label_a11y(&reader, "Reading mode (Ctrl Shift R)", gtk::atk::Role::PushButton);
    let bookmark = icons::button(Icon::Star, 18, palette.muted, "chrome-tool");
    label_a11y(&bookmark, "Keep this page (Ctrl D)", gtk::atk::Role::PushButton);
    for button in [&reader, &bookmark] {
        button.set_can_focus(true);
        button.set_focus_on_click(false);
        button.set_valign(gtk::Align::Center);
        pack(&tools, button, false);
        button.show_all();
        button.set_no_show_all(true);
    }
    address_hint.set_no_show_all(true);
    let more = icons::button(Icon::Menu, 17, palette.muted, "chrome-tool");
    more.set_valign(gtk::Align::Center);
    more.set_can_focus(true);
    label_a11y(&more, "Browser menu", gtk::atk::Role::PushButton);
    pack(&tools, &more, false);
    let lights = widget_box(gtk::Orientation::Horizontal, 2);
    lights.set_valign(gtk::Align::Center);
    lights.set_margin_start(12);
    let (light_red, light_amber, light_green) = window_controls(palette);
    pack(&lights, &light_amber, false);
    pack(&lights, &light_green, false);
    pack(&lights, &light_red, false);
    pack(&tools, &lights, false);
    pack(&card, &toolbar, false);
    pack(&card, &bmarkbar, false);
    let stack = gtk::Stack::new();
    stack.set_homogeneous(false);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_vexpand_set(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    // The page crossfade uses the shared MICRO role, the same as hover and
    // fade-in, instead of its own 90ms. motion::enabled() is false under the
    // bench and when the desktop turns animations off, so both get a cut.
    stack.set_transition_duration(if crate::motion::enabled() { crate::motion::dur::MICRO as u32 } else { 0 });
    let blank_holder = gtk::EventBox::new();
    crate::ui::class(&blank_holder, "blank");
    blank_holder.set_visible_window(false);
    let start = crate::startpage::StartPage::new(palette);
    blank_holder.add(&start.root);
    stack.add_named(&blank_holder, "blank");

    // A page that will not open is the browser's to explain, not the engine's
    // to dump: the same quiet type, the reason, the address it tried, and one
    // thing to do about it.
    // Wrapped the way the blank page is wrapped: the same shape, so the click
    // that opens the address field there can retry a load here.
    let error_holder = gtk::EventBox::new();
    crate::ui::class(&error_holder, "error-page");
    error_holder.set_visible_window(false);
    error_holder.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    let errorpage = widget_box(gtk::Orientation::Vertical, 8);
    crate::ui::class(&errorpage, "error-content");
    errorpage.set_valign(gtk::Align::Center);
    errorpage.set_halign(gtk::Align::Center);
    let error_glyph = icons::area(Icon::Globe, 48, palette.accent());
    class(&error_glyph, "error-glyph");
    error_glyph.set_halign(gtk::Align::Center);
    error_glyph.set_margin_bottom(16);
    let error_title = gtk::Label::new(Some("This page didn't open"));
    crate::ui::class(&error_title, "blanktitle");
    error_title.set_width_chars(1);
    error_title.set_max_width_chars(22);
    error_title.set_line_wrap(true);
    error_title.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    error_title.set_justify(gtk::Justification::Center);
    let error_reason = gtk::Label::new(None);
    crate::ui::class(&error_reason, "blankhint");
    error_reason.set_width_chars(1);
    error_reason.set_line_wrap(true);
    error_reason.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    error_reason.set_max_width_chars(44);
    error_reason.set_justify(gtk::Justification::Center);
    let error_address = gtk::Label::new(None);
    crate::ui::class(&error_address, "erraddress");
    error_address.set_width_chars(1);
    error_address.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    error_address.set_max_width_chars(52);
    let error_actions = widget_box(gtk::Orientation::Horizontal, 9);
    crate::ui::class(&error_actions, "error-actions");
    error_actions.set_halign(gtk::Align::Center);
    error_actions.set_margin_top(12);
    let error_retry = gtk::Button::with_label("Try again");
    crate::ui::class(&error_retry, "act");
    class(&error_retry, "primary");
    error_retry.set_halign(gtk::Align::Center);
    // A 4xx cannot be fixed by reloading: the primary button becomes Change
    // address, and Back is the quiet second way out of the same dead end.
    let error_back = gtk::Button::with_label("Go back");
    crate::ui::class(&error_back, "act");
    error_back.set_halign(gtk::Align::Center);
    error_actions.pack_start(&error_retry, false, false, 0);
    error_actions.pack_start(&error_back, false, false, 0);
    pack(&errorpage, &error_glyph, false);
    pack(&errorpage, &error_title, false);
    pack(&errorpage, &error_reason, false);
    pack(&errorpage, &error_address, false);
    pack(&errorpage, &error_actions, false);
    error_holder.add(&errorpage);
    // The name stays "error": the browser and the bench both look the page up
    // by it, and only the widget under the name changed.
    stack.add_named(&error_holder, "error");

    // Overlay the page stack so the 3px load progress bar floats at the very top
    // of the viewport without ever pushing page content down.
    let page_overlay = gtk::Overlay::new();
    class(&page_overlay, "viewport");
    page_overlay.set_hexpand(true);
    page_overlay.set_vexpand(true);
    page_overlay.add(&stack);

    let progress_frac = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));
    let progress = gtk::DrawingArea::new();
    crate::ui::class(&progress, "progress");
    progress.set_size_request(-1, 3);
    progress.set_halign(gtk::Align::Fill);
    progress.set_valign(gtk::Align::Start);
    progress.set_no_show_all(true);
    progress.set_visible(false);
    let frac_for_draw = progress_frac.clone();
    progress.connect_draw(move |widget, cr| {
        let frac = frac_for_draw.get().clamp(0.0, 1.0);
        if frac <= 0.001 {
            return crate::gtkx::inh(false);
        }
        let alloc = widget.allocation();
        let w = alloc.width() as f64;
        let h = alloc.height() as f64;
        if w <= 0.0 || h <= 0.0 {
            return crate::gtkx::inh(false);
        }
        let fill_w = (w * frac).max(2.0).min(w);
        let rgba = widget.style_context().color(gtk::StateFlags::NORMAL);
        cr.set_source_rgba(rgba.red(), rgba.green(), rgba.blue(), rgba.alpha());
        cr.rectangle(0.0, 0.0, fill_w, h);
        let _ = cr.fill();
        if fill_w > 4.0 {
            let glow_w = 18.0_f64.min(fill_w);
            let x0 = (fill_w - glow_w).max(0.0);
            let grad = cairo::LinearGradient::new(x0, 0.0, fill_w, 0.0);
            grad.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, 0.0);
            grad.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.45);
            let _ = cr.set_source(&grad);
            cr.rectangle(x0, 0.0, glow_w, h);
            let _ = cr.fill();
        }
        crate::gtkx::inh(false)
    });
    page_overlay.add_overlay(&progress);
    page_overlay.set_overlay_pass_through(&progress, true);
    pack(&card, &page_overlay, true);

    // The page is the paned's second child itself: the band it used to begin
    // with is the one strip above the paned now, and a wrapper here would buy
    // a box with a single child in it.
    paned.pack2(&card, true, true);

    let frame = widget_box(gtk::Orientation::Vertical, 0);
    crate::ui::class(&frame, "shell");
    pack(&frame, &paned, true);
    overlay.add(&frame);

    Ui {
        window,
        overlay,
        shell: frame,
        paned,
        side,
        strip,
        body,
        top,
        toolbar,
        fold,
        address,
        address_label,
        address_path,
        address_hint,
        site_info,
        reader,
        bookmark,
        more,
        newtab,
        private_tab,
        library,
        history,
        downloads,
        tab_count,
        start,
        light_red,
        light_amber,
        light_green,
        back,
        forward,
        reload,
        pins,
        pins_holder,
        rows,
        rows_scroll,
        menu,
        stack,
        progress,
        progress_frac,
        bmarkbar,
        bmarks,
        error_holder,
        errorpage,
        error_title,
        error_reason,
        error_address,
        error_retry,
        error_back,
    }
}

fn window_controls(palette: Palette) -> (gtk::EventBox, gtk::EventBox, gtk::EventBox) {
    let make = |kind: Icon, name: &str, tooltip: &str| {
        let e = gtk::EventBox::new();
        crate::ui::class(&e, "control");
        e.style_context().add_class(name);
        e.set_size_request(28, 28);
        e.set_valign(gtk::Align::Center);
        e.add_events(gtk::gdk::EventMask::ENTER_NOTIFY_MASK | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK);
        // The mark is drawn, not typed: there is no text in this control for a
        // screen reader to read, so the words have to be given to it. A
        // button role says what it is; the tooltip and the accessible name say
        // which.
        label_a11y(&e, tooltip, gtk::atk::Role::PushButton);
        let mark = icons::area(kind, 12, palette.muted);
        mark.set_halign(gtk::Align::Center);
        mark.set_valign(gtk::Align::Center);
        e.add(&mark);
        e
    };
    (
        make(Icon::Close, "shut", "Close window"),
        make(Icon::Minimise, "smaller", "Minimise window"),
        make(Icon::Maximise, "bigger", "Maximise window"),
    )
}

/// One tab's line: a small mark, the title, and a cross when you are on it.
pub struct TabRow {
    pub event: gtk::EventBox,
    pub title: gtk::Label,
    pub subtitle: gtk::Label,
    /// The letter, when there is one. A page that offers an icon has no letter
    /// on screen, so this is `None` rather than a label nobody can see. While
    /// there is no icon it is the label inside the letter's tile, so renaming
    /// the row renames the letter too.
    pub letter: Option<gtk::Label>,
    /// Where the mark goes: a letter until the page offers an icon, then the
    /// page's own icon. Both live in this box so one can replace the other.
    pub icon: gtk::Box,
    pub close: gtk::Button,
}

/// A page's own mark, at the size the row has for it. GTK scales the surface;
/// asking cairo for the pixels first would need to know the surface's type,
/// and a 16px favicon is the size it was drawn at anyway.
pub fn favicon(surface: &cairo::Surface, size: i32) -> gtk::Image {
    let image = gtk::Image::from_surface(Some(surface));
    image.set_pixel_size(size);
    image
}

/// The mark in front of a title: the page's own icon if it has one, its letter
/// in a small tile if it does not. One function for both, so a page that gains
/// a favicon mid-load does not end up with two marks.
/// The same door, for a mark that is drawn rather than a letter.
pub fn mark_glyph(holder: &gtk::Box, glyph: Icon, surface: Option<&cairo::Surface>, size: i32, color: crate::palette::Rgb) -> Option<gtk::Label> {
    clear(holder);
    match surface {
        Some(surface) => {
            let image = favicon(surface, size);
            image.set_valign(gtk::Align::Center);
            crate::ui::class(&image, "favicon");
            pack(holder, &image, false);
        }
        None => {
            let mark = icons::area(glyph, size, color);
            mark.set_valign(gtk::Align::Center);
            crate::ui::class(&mark, "rowmark");
            pack(holder, &mark, false);
        }
    }
    holder.show_all();
    None
}

fn clear(holder: &gtk::Box) {
    for child in holder.children() {
        holder.remove(&child);
    }
}

/// The letter a mark stands in for, when the mark does not own it. In a row
/// the holder is the letter's parent and `mark` returns it; in a pin the
/// letter and the icon are two children of the same overlay, and the overlay
/// is the only thing between them. Without this a favicon lands on top of the
/// initial it replaced and the two show through each other. A plain box (the
/// row's holder) has no overlay above it, so this finds nothing there.
fn overlay_letter(holder: &gtk::Box) -> Option<gtk::Widget> {
    let overlay = holder.parent()?.downcast::<gtk::Overlay>().ok()?;
    overlay.child()
}

pub fn mark(holder: &gtk::Box, letter: char, surface: Option<&cairo::Surface>, size: i32) -> Option<gtk::Label> {
    clear(holder);
    // One mark at a time, centred in a holder that is often a little wider
    // than the mark: homogeneous gives the single child the whole cell, so
    // halign on the tile can actually centre it instead of leaving it at the
    // left edge with a gap on the right.
    holder.set_homogeneous(true);
    match surface {
        Some(surface) => {
            let image = favicon(surface, size);
            image.set_valign(gtk::Align::Center);
            crate::ui::class(&image, "favicon");
            pack(holder, &image, false);
            // Shown by name: `show_all` on the holder is what ordinarily makes
            // a new child appear, and the pin's mark box is built hidden, so the
            // one child that has to appear is shown here instead.
            image.show();
            holder.show();
            if let Some(letter) = overlay_letter(holder) {
                letter.hide();
            }
            None
        }
        None => {
            // In a pin the letter is not the mark: it is the sibling the icon
            // was put over, and it comes back rather than being replaced by a
            // second copy of itself in a tile.
            if let Some(letter) = overlay_letter(holder) {
                holder.hide();
                letter.show();
                return None;
            }
            let tile = gtk::EventBox::new();
            crate::ui::class(&tile, "tile");
            // Square, and centred in whatever holder it is put in: a tile
            // wider than it is tall, or pushed to one edge of an 18px holder,
            // reads as a smudge rather than as a letter.
            tile.set_size_request(size + 2, size + 2);
            tile.set_halign(gtk::Align::Center);
            tile.set_valign(gtk::Align::Center);
            let label = gtk::Label::new(Some(&letter.to_uppercase().to_string()));
            crate::ui::class(&label, "tileletter");
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            // No max-width-chars here: with a one-character label it only
            // gives GTK a reason to clip a wide capital in a 16px tile, and a
            // single uppercase letter at 11px is narrower than the tile is.
            tile.add(&label);
            pack(holder, &tile, false);
            holder.show_all();
            Some(label)
        }
    }
}

/// How a tab is doing, so the row can show it.
///
/// These three facts were already computed for every tab on every rebuild and
/// then dropped on the floor: a sleeping tab looked exactly like a live one, a
/// loading tab had no sign of loading, and a private tab was private only in
/// the data. A row is the only place most people will ever look.
#[derive(Clone, Copy, Default)]
pub struct TabState {
    pub asleep: bool,
    pub loading: bool,
    pub private: bool,
}

pub fn loading_mark(icon: &gtk::Box, colour: crate::palette::Rgb) {
    for child in icon.children() {
        icon.remove(&child);
    }
    let area = gtk::DrawingArea::new();
    area.set_size_request(14, 14);
    area.set_valign(gtk::Align::Center);
    let angle = std::rc::Rc::new(std::cell::Cell::new(0.0_f64));
    let for_draw = angle.clone();
    area.connect_draw(move |widget, cr| {
        let a = for_draw.get();
        icons::set_source_color(widget, cr, colour);
        cr.set_line_width(1.6);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.arc(7.0, 7.0, 5.0, a, a + std::f64::consts::TAU * 0.7);
        let _ = cr.stroke();
        crate::gtkx::inh(false)
    });
    // With animations off (gtk-enable-animations is false for reduced motion,
    // or the bench is running), the mark is drawn once as a still arc and
    // gets no tick callback, so it asks for no frames at all.
    if !crate::motion::enabled() {
        icon.pack_start(&area, false, false, 0);
        area.show();
        return;
    }
    // Driven by the frame clock, not a timer: GTK only ticks mapped widgets,
    // so a spinner in a hidden or folded row costs no wakeups at all. The
    // angle comes from elapsed time, so it keeps the old speed (one turn in
    // about 1.44s) whatever the refresh rate.
    let start = std::rc::Rc::new(std::cell::Cell::new(None::<i64>));
    let _ = area.add_tick_callback(move |area, clock| {
        // Reduced motion can be switched on while the spinner is already
        // running (gtk-enable-animations changes at run time). The arc then
        // stops where it is, gets one last draw, and its tick callback is
        // removed, so it asks for no more frames. This is the same still arc
        // a spinner built with animations off would show.
        if !crate::motion::enabled() {
            area.queue_draw();
            return gtk::glib::ControlFlow::Break;
        }
        let now = clock.frame_time();
        let t0 = match start.get() {
            Some(t0) => t0,
            None => {
                start.set(Some(now));
                now
            }
        };
        let turns = (now - t0).max(0) as f64 / 1_440_000.0;
        angle.set(turns.fract() * std::f64::consts::TAU);
        area.queue_draw();
        gtk::glib::ControlFlow::Continue
    });
    icon.pack_start(&area, false, false, 0);
    area.show();
}

/// How strongly a tab row shows its close button: full under the pointer,
/// softened on the tab you are on, hidden on every other row. This is widget
/// opacity, not CSS (round 65): GTK3 kept drawing the CSS opacity a button
/// was built with.
pub fn close_opacity(live: bool, hovered: bool) -> f64 {
    if hovered {
        1.0
    } else if live {
        0.6
    } else {
        0.0
    }
}

pub fn tab_row(
    title: &str,
    palette: Palette,
    row_height: i32,
    letter: Option<char>,
    live: bool,
    state: TabState,
) -> TabRow {
    let event = gtk::EventBox::new();
    crate::ui::class(&event, "row");
    if live {
        crate::ui::class(&event, "live");
    }
    if state.asleep {
        crate::ui::class(&event, "asleep");
    }
    if state.private {
        crate::ui::class(&event, "private");
    }
    event.set_size_request(-1, row_height);

    let hbox = widget_box(gtk::Orientation::Horizontal, 10);
    hbox.set_margin_start(12);
    hbox.set_margin_end(8);

    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_size_request(22, 22);
    icon.set_valign(gtk::Align::Center);
    // The same mark the pin uses: the page's own letter until the page offers
    // an icon. A globe in every row is one grey mark for every site, which
    // tells nobody which row is which; a row with no letter at all keeps the
    // plus, and that plus is only for a row that could make a tab.
    // 16px fills the 22px holder the way a favicon does. At 14px a letter
    // looked smaller than the real site icons in the rows around it.
    let letter_label = match letter {
        Some(letter) => mark(&icon, letter, None, 16),
        // A row with no letter is the one that makes another tab, so the plus
        // is right there and nowhere else.
        None => mark_glyph(&icon, Icon::Plus, None, 14, palette.faint),
    };
    // A private tab wears the mask instead of its letter: the promise it makes
    // is the most important thing about it, and a letter cannot say it.
    if state.private {
        for child in icon.children() {
            icon.remove(&child);
        }
        mark_glyph(&icon, Icon::Mask, None, 14, palette.muted);
    }
    // Loading is an arc that turns, in the same 18px box the letter lives in,
    // so a row does not change width when a page starts or stops loading.
    let spinner = if state.loading && !state.private {
        loading_mark(&icon, palette.muted);
        Some(())
    } else {
        None
    };
    let _ = spinner;
    pack(&hbox, &icon, false);

    let label = gtk::Label::new(Some(title));
    crate::ui::class(&label, "rowtitle");
    if live {
        // A second class rather than a descendant selector: GTK resolves
        // `.row.live .rowtitle` against the row's own style context, and the
        // label kept the resting grey. One class on the label itself always
        // lands.
        crate::ui::class(&label, "liveword");
    }
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_hexpand(true);
    label.set_valign(gtk::Align::Center);
    label.set_max_width_chars(1);
    let text = widget_box(gtk::Orientation::Vertical, 1);
    text.set_valign(gtk::Align::Center);
    pack(&text, &label, false);
    let subtitle = gtk::Label::new(Some("Start exploring"));
    class(&subtitle, "row-subtitle");
    subtitle.set_xalign(0.0);
    subtitle.set_ellipsize(gtk::pango::EllipsizeMode::End);
    subtitle.set_max_width_chars(1);
    pack(&text, &subtitle, false);
    pack(&hbox, &text, true);

    let close = gtk::Button::new();
    close.set_relief(gtk::ReliefStyle::None);
    close.add(&icons::area(Icon::Close, 12, palette.muted.mix(palette.faint, 0.4)));
    crate::ui::class(&close, "loupe");
    crate::ui::class(&close, "close");
    // The row's close button is always fully opaque in CSS (`rowclose`), and
    // whether it shows is the widget opacity, set in code. Round 65 showed
    // GTK3 drawing the CSS opacity a button had when its row was built, even
    // after the class moved and the button was redrawn, so CSS cannot decide
    // which row shows the x. render_rows and the row's enter/leave keep it
    // current.
    crate::ui::class(&close, "rowclose");
    close.set_opacity(close_opacity(live, false));
    close.set_valign(gtk::Align::Center);
    // A 24px target keeps the 12px glyph but gives the pointer room. At 18px
    // the close button was easy to miss, and a miss selected the tab instead.
    // The row is 46px tall, so the larger target does not change its height.
    close.set_size_request(24, 24);
    close.set_can_focus(false);
    close.set_tooltip_text(Some("Close this tab (Ctrl W)"));
    {
        use gtk::atk::prelude::AtkObjectExt;
        if let Some(access) = close.accessible() {
            access.set_name("Close this tab");
            access.set_role(gtk::atk::Role::PushButton);
        }
    }
    pack(&hbox, &close, false);

    event.add(&hbox);
    TabRow { event, title: label, subtitle, letter: letter_label, icon, close }
}

/// A pinned page: one square, one letter, or its site's mark.
pub struct PinWidget {
    pub event: gtk::EventBox,
    pub letter: gtk::Label,
    pub icon: gtk::Box,
}

pub fn pin_tile(letter: &str, live: bool, _palette: Palette) -> PinWidget {
    let event = gtk::EventBox::new();
    crate::ui::class(&event, "pin");
    if live {
        crate::ui::class(&event, "live");
    }
    event.set_size_request(35, 35);
    let label = gtk::Label::new(Some(&letter.to_uppercase()));
    crate::ui::class(&label, "pinletter");
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    // Opted out of show_all, and shown here instead. The column is redrawn with
    // show_all every time its tabs change, and that call would otherwise put
    // the letter back over the favicon the browser had just hidden it for.
    label.set_no_show_all(true);
    label.show();
    // The mark sits over the letter: a page with an icon shows the icon there,
    // a page without one shows its own initial. An empty globe for every site
    // with no favicon was four identical grey circles telling nobody which pin
    // was which, so nothing is drawn until the page offers something real.
    let stack = gtk::Overlay::new();
    stack.add(&label);
    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    // Empty and hidden while there is no favicon, so a pin draws its own
    // letter and nothing else. `mark` shows this box and the image inside it by
    // name when an icon arrives, which show_all cannot do for a box that opted
    // out of it.
    icon.set_no_show_all(true);
    icon.hide();
    stack.add_overlay(&icon);
    event.add(&stack);
    PinWidget { event, letter: label, icon }
}

/// The dimming behind a field or a panel. A drawing area rather than an event
/// box: it needs no window of its own, so it stacks where it is put.
/// The scrim's colour and alpha, in one place: the widget draws it and a
/// capture fills a rectangle with it, and the two must not drift apart.
pub fn scrim_colour(palette: &Palette) -> (f64, f64, f64, f64) {
    if palette.is_dark() {
        // Gemini R59 fix 8: at 0.78 the dark scrim let bright page content show
        // through, so the omnibox and panels competed with the page behind them.
        // At 0.88 the page drops back without going fully black. The value is set
        // only here: the widget draws it, and the capture in app.rs reads
        // scrim_colour, so the screen and the shots stay the same.
        let c = palette.ground;
        (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0, 0.88)
    } else {
        // A light scrim is a black veil, not white paint over the page.
        // Gemini R60 polish: at 0.28 a busy light page competed with the
        // omnibox and panels above it. At 0.42 the page drops back behind the
        // open layer and its text still shows. As with the dark value, it is
        // set only here, and the capture in app.rs reads scrim_colour.
        (0.0, 0.0, 0.0, 0.42)
    }
}

pub fn scrim(palette: &Palette) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    crate::ui::class(&area, "scrim");
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    let (r, g, b, a) = scrim_colour(palette);
    area.connect_draw(move |_, context| {
        context.set_source_rgba(r, g, b, a);
        let _ = context.paint();
        crate::gtkx::inh(false)
    });
    area
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folded_column_stays_folded_through_full_screen() {
        let folded = column_plan(true, false, true, 260);
        assert_eq!((folded.strip, folded.body, folded.width), (true, false, STRIP_WIDTH));
        assert_eq!(folded.paned_at, STRIP_WIDTH);

        let full = column_plan(true, true, true, 260);
        assert!(!full.side_shown, "full screen hides the column");
        assert!(full.strip, "full screen does not unfold the column");

        let back = column_plan(true, false, true, 260);
        assert_eq!(back, folded, "leaving full screen returns to the fold");
    }

    #[test]
    fn an_unfolded_column_leaves_full_screen_open_again() {
        let open = column_plan(false, false, true, 260);
        assert_eq!((open.strip, open.body, open.width, open.paned_at), (false, true, 260, 260));
        assert!(column_plan(false, true, true, 260).body == open.body, "full screen does not change the fold");
        assert!(column_plan(false, false, true, 260).side_shown);
    }

    #[test]
    fn no_sidebar_means_no_column_whatever_the_fold() {
        assert_eq!(column_plan(true, false, false, 260).width, 0);
        assert!(!column_plan(true, false, false, 260).strip);
    }

    #[test]
    fn close_button_visibility_has_three_clear_steps() {
        // A row under the pointer shows its x at full strength even when it
        // is not the live tab; an idle live row keeps a hint of it; every
        // other row has none. These are the values the capture draws too.
        assert_eq!(close_opacity(false, true), 1.0);
        assert_eq!(close_opacity(true, true), 1.0);
        assert_eq!(close_opacity(true, false), 0.6);
        assert_eq!(close_opacity(false, false), 0.0);
    }
}
