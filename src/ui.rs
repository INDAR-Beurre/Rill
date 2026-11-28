// The window as it is drawn: a column of tabs with the page beside it, and
// the few things that appear over the page — the field, find, panels.
//
// Nothing here knows what a tab is; it makes widgets, and the browser keeps
// them true.
use crate::icons::{self, Icon};
use crate::palette::{Palette, Rgb};
use gtk::prelude::*;

pub struct Ui {
    pub window: gtk::Window,
    pub overlay: gtk::Overlay,
    pub shell: gtk::Box,
    /// Full-width top tab strip (with pinned tabs, reorderable tabs, `+` new tab,
    /// single browser menu button, and right-aligned Linux window controls).
    pub topbar: gtk::EventBox,
    pub top_lights_left: gtk::Box,
    pub top_lights_right: gtk::Box,
    pub top_more_slot: gtk::Box,
    pub top_space_btn: gtk::Button,
    pub top_space_label: gtk::Label,
    pub top_pins_holder: gtk::Box,
    pub top_pins: gtk::Box,
    pub top_tabs_scroll: gtk::ScrolledWindow,
    pub top_tabs: gtk::Box,
    pub top_drag: gtk::EventBox,
    pub paned: gtk::Paned,
    /// Exclusive vertical tab sidebar (toggled with Ctrl S; hides topbar when active).
    pub side: gtk::Box,
    pub strip: gtk::EventBox,
    pub body: gtk::Box,
    pub top: gtk::EventBox,
    pub side_lights_left: gtk::Box,
    pub side_lights_right: gtk::Box,
    pub side_more_slot: gtk::Box,
    pub side_space_btn: gtk::Button,
    pub side_space_label: gtk::Label,
    pub side_spaces_footer: gtk::Box,
    pub side_spaces: gtk::Box,
    pub side_tabs_heading: gtk::Label,
    pub toolbar: gtk::EventBox,
    pub tools: gtk::Box,
    pub location: gtk::Box,
    pub viewport_frame: gtk::Box,
    pub viewport_corners: [gtk::DrawingArea; 4],
    pub viewport_corner_colors: std::rc::Rc<std::cell::Cell<(Rgb, Rgb)>>,
    pub side_fold: gtk::Button,
    pub side_newtab: gtk::Button,
    pub address: gtk::Button,
    pub address_label: gtk::Label,
    /// The path after the trusted origin chip. Kept as a separate, quiet
    /// label so a path cannot be mistaken for the site's authority.
    pub address_path: gtk::Label,
    pub site_info: gtk::Button,
    pub reader: gtk::Button,
    pub bookmark: gtk::Button,
    pub more: gtk::Button,
    pub newtab: gtk::Button,
    pub tab_count: gtk::Label,
    pub start: crate::startpage::StartPage,
    pub light_red: gtk::EventBox,
    pub light_amber: gtk::EventBox,
    pub light_green: gtk::EventBox,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub reload: gtk::Button,
    pub home: gtk::Button,
    pub pins: gtk::Grid,
    pub pins_holder: gtk::Box,
    pub rows: gtk::Box,
    pub rows_scroll: gtk::ScrolledWindow,
    pub stack: gtk::Stack,
    pub progress: gtk::DrawingArea,
    pub progress_frac: std::rc::Rc<std::cell::Cell<f64>>,
    pub bmarkbar: gtk::EventBox,
    pub bmarks: gtk::Box,
    pub bmark_search_box: gtk::Box,
    pub bmark_search: gtk::Entry,
    pub bmark_layer: crate::layer::Layer,
    pub bmark_card: gtk::Box,
    pub bmark_suggest: gtk::Box,
    pub error_holder: gtk::EventBox,
    pub errorpage: gtk::Box,
    pub error_glyph: gtk::EventBox,
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
pub const STRIP_WIDTH: i32 = 18;

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

/// Exclusive visibility plan for the two tab chrome layouts.
/// Outside fullscreen, topbar and vertical sidebar are strictly mutually
/// exclusive (never both visible, never both hidden). In fullscreen, both hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TabLayoutPlan {
    pub show_topbar: bool,
    pub show_sidebar: bool,
}

pub fn tab_layout_plan(sidebar: bool, column_folded: bool, fullscreen: bool) -> TabLayoutPlan {
    if fullscreen {
        return TabLayoutPlan {
            show_topbar: false,
            show_sidebar: false,
        };
    }
    let vertical = sidebar && !column_folded;
    TabLayoutPlan {
        show_topbar: !vertical,
        show_sidebar: vertical,
    }
}

/// Build an icon `DrawingArea` whose native GDK window passes pointer events
/// through to its enclosing `GtkButton`. Prevents post-realize icon swaps from
/// stacking above `GtkButton`'s input-only event window and eating real clicks.
pub fn passthrough_icon(kind: Icon, size: i32, color: Rgb) -> gtk::DrawingArea {
    let area = icons::area(kind, size, color);
    area.connect_realize(|widget| {
        if let Some(win) = widget.window() {
            win.set_pass_through(true);
        }
    });
    area
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
        paned_at: width,
        strip: folded && sidebar,
        body: !folded && sidebar,
    }
}

pub fn build(window: gtk::Window, palette: Palette, prefs: &crate::store::Prefs) -> Ui {
    let overlay = gtk::Overlay::new();

    let layout_plan = tab_layout_plan(prefs.sidebar, prefs.column_folded, false);

    // ---- Top tab bar (38px, full-width, Linux-native CSD header strip) ----
    let topbar = gtk::EventBox::new();
    class(&topbar, "topbar");
    topbar.set_size_request(-1, 38);
    topbar.set_no_show_all(true);
    topbar.set_visible(layout_plan.show_topbar);
    topbar.add_events(
        gtk::gdk::EventMask::BUTTON_PRESS_MASK
            | gtk::gdk::EventMask::ENTER_NOTIFY_MASK
            | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK,
    );
    let top_row = widget_box(gtk::Orientation::Horizontal, 4);
    top_row.set_margin_start(8);
    top_row.set_margin_end(8);
    top_row.set_margin_top(4);
    top_row.set_margin_bottom(4);
    topbar.add(&top_row);

    // Optional left-side browser window controls (populated only when CSD is
    // active and gtk-decoration-layout places controls on the left).
    let top_lights_left = widget_box(gtk::Orientation::Horizontal, 2);
    top_lights_left.set_valign(gtk::Align::Center);
    top_lights_left.set_margin_end(4);
    top_lights_left.set_no_show_all(true);
    top_lights_left.hide();
    pack(&top_row, &top_lights_left, false);

    // Space switcher pill in top bar (hidden while only Default space exists).
    let top_space_btn = gtk::Button::new();
    class(&top_space_btn, "space-pill");
    class(&top_space_btn, "top-space-btn");
    top_space_btn.set_relief(gtk::ReliefStyle::None);
    top_space_btn.set_valign(gtk::Align::Center);
    top_space_btn.set_can_focus(true);
    top_space_btn.set_focus_on_click(false);
    let top_space_inner = widget_box(gtk::Orientation::Horizontal, 4);
    let top_space_label = gtk::Label::new(Some(crate::spaces::DEFAULT_SPACE_NAME));
    class(&top_space_label, "space-pill-label");
    top_space_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    top_space_label.set_max_width_chars(14);
    pack(&top_space_inner, &top_space_label, false);
    let top_space_arrow = passthrough_icon(Icon::Down, 10, palette.muted);
    top_space_arrow.set_valign(gtk::Align::Center);
    pack(&top_space_inner, &top_space_arrow, false);
    top_space_btn.add(&top_space_inner);
    label_a11y(&top_space_btn, "Switch space", gtk::atk::Role::PushButton);
    top_space_btn.set_no_show_all(true);
    top_space_btn.hide();
    pack(&top_row, &top_space_btn, false);

    // Pinned top tabs + hairline separator (hidden when empty).
    let top_pins_holder = widget_box(gtk::Orientation::Horizontal, 4);
    top_pins_holder.set_valign(gtk::Align::Center);
    let top_pins = widget_box(gtk::Orientation::Horizontal, 4);
    top_pins.set_valign(gtk::Align::Center);
    pack(&top_pins_holder, &top_pins, false);
    let top_pin_sep = gtk::Separator::new(gtk::Orientation::Vertical);
    class(&top_pin_sep, "topbar-sep");
    top_pin_sep.set_margin_start(2);
    top_pin_sep.set_margin_end(2);
    top_pin_sep.set_margin_top(6);
    top_pin_sep.set_margin_bottom(6);
    pack(&top_pins_holder, &top_pin_sep, false);
    top_pins_holder.set_no_show_all(true);
    top_pins_holder.hide();
    pack(&top_row, &top_pins_holder, false);

    // Horizontally scrollable top tab strip. Propagating natural width with a
    // 0px minimum keeps `+` directly beside the last tab when tabs fit, while
    // shrinking cleanly to a scrollable strip when tabs overflow so `+` stays
    // pinned and visible at every window width.
    let top_tabs_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    class(&top_tabs_scroll, "toptabs-scroll");
    top_tabs_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    top_tabs_scroll.set_shadow_type(gtk::ShadowType::None);
    top_tabs_scroll.set_propagate_natural_width(true);
    top_tabs_scroll.set_propagate_natural_height(false);
    top_tabs_scroll.set_min_content_width(0);
    top_tabs_scroll.set_hexpand(false);
    top_tabs_scroll.set_valign(gtk::Align::Center);
    top_tabs_scroll.set_overlay_scrolling(true);

    let top_tabs = widget_box(gtk::Orientation::Horizontal, 4);
    class(&top_tabs, "toptabs");
    top_tabs.set_hexpand(false);
    top_tabs.set_halign(gtk::Align::Start);
    top_tabs.set_valign(gtk::Align::Center);
    if let Some(access) = top_tabs.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::PageTabList);
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Open tabs");
    }
    top_tabs_scroll.add(&top_tabs);
    pack(&top_row, &top_tabs_scroll, false);

    let newtab = icons::button(Icon::Plus, 14, palette.muted, "top-newtab");
    newtab.set_valign(gtk::Align::Center);
    newtab.set_can_focus(true);
    newtab.set_focus_on_click(false);
    label_a11y(&newtab, "New tab (Ctrl T)", gtk::atk::Role::PushButton);
    pack(&top_row, &newtab, false);

    let top_drag = gtk::EventBox::new();
    top_drag.set_visible_window(false);
    top_drag.set_hexpand(true);
    top_drag.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    pack(&top_row, &top_drag, true);

    // Right side of top bar: ONE compact browser menu button (reparented into
    // sidebar header in vertical-tab mode) + optional right-aligned Linux window controls.
    let top_more_slot = widget_box(gtk::Orientation::Horizontal, 0);
    top_more_slot.set_valign(gtk::Align::Center);
    let more = icons::button(Icon::Menu, 14, palette.muted, "chrome-tool");
    more.set_valign(gtk::Align::Center);
    more.set_can_focus(true);
    more.set_focus_on_click(false);
    label_a11y(&more, "Browser menu", gtk::atk::Role::PushButton);
    pack(&top_more_slot, &more, false);
    pack(&top_row, &top_more_slot, false);

    let top_lights_right = widget_box(gtk::Orientation::Horizontal, 2);
    top_lights_right.set_valign(gtk::Align::Center);
    top_lights_right.set_margin_start(4);
    top_lights_right.set_no_show_all(true);
    let (light_red, light_amber, light_green) = window_controls(palette);
    pack(&top_lights_right, &light_amber, false);
    pack(&top_lights_right, &light_green, false);
    pack(&top_lights_right, &light_red, false);
    top_lights_right.hide();
    pack(&top_row, &top_lights_right, false);

    // ---- Optional vertical sidebar (hidden by default; Ctrl S toggles) ----
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    crate::ui::class(&paned, "shell");
    paned.set_wide_handle(false);
    paned.set_hexpand(true);
    paned.set_vexpand(true);

    let side_width = prefs.side_width.round() as i32;
    let side = widget_box(gtk::Orientation::Vertical, 0);
    crate::ui::class(&side, "side");
    let plan = column_plan(prefs.column_folded, false, layout_plan.show_sidebar, side_width);
    let min_width = if !layout_plan.show_sidebar {
        0
    } else if prefs.column_folded {
        STRIP_WIDTH
    } else {
        MIN_COL
    };
    side.set_size_request(min_width, -1);
    side.set_no_show_all(true);
    side.set_visible(layout_plan.show_sidebar);
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
    label_a11y(&strip, "Show tab sidebar", gtk::atk::Role::PushButton);
    let strip_glyph = icons::area(Icon::Sidebar, 14, palette.muted);
    strip_glyph.set_halign(gtk::Align::Center);
    strip_glyph.set_valign(gtk::Align::Start);
    strip_glyph.set_margin_top(12);
    strip.add(&strip_glyph);
    strip_glyph.show();
    pack(&side, &strip, true);

    let body = widget_box(gtk::Orientation::Vertical, 0);
    body.set_no_show_all(true);
    body.set_visible(plan.body);
    pack(&side, &body, true);

    let top = gtk::EventBox::new();
    crate::ui::class(&top, "topband");
    crate::ui::class(&top, "side-header");
    top.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    let top_stack = widget_box(gtk::Orientation::Vertical, 2);
    top.add(&top_stack);

    // Row 1: always-reachable chrome controls (window controls when CSD is
    // active, Browser Menu reparented here in vertical mode, and Top Tabs toggle).
    let side_chrome_row = widget_box(gtk::Orientation::Horizontal, 4);
    side_chrome_row.set_margin_start(8);
    side_chrome_row.set_margin_end(8);
    side_chrome_row.set_margin_top(6);
    side_chrome_row.set_margin_bottom(2);
    let side_lights_left = widget_box(gtk::Orientation::Horizontal, 2);
    side_lights_left.set_valign(gtk::Align::Center);
    side_lights_left.set_no_show_all(true);
    side_lights_left.hide();
    pack(&side_chrome_row, &side_lights_left, false);

    let side_more_slot = widget_box(gtk::Orientation::Horizontal, 0);
    side_more_slot.set_valign(gtk::Align::Center);
    pack(&side_chrome_row, &side_more_slot, false);

    let side_fold = icons::button(Icon::Sidebar, 14, palette.muted, "sidebar-tool");
    side_fold.set_valign(gtk::Align::Center);
    side_fold.set_can_focus(true);
    side_fold.set_focus_on_click(false);
    label_a11y(&side_fold, "Switch to top tabs (Ctrl S)", gtk::atk::Role::PushButton);
    pack(&side_chrome_row, &side_fold, false);

    let side_spacer = widget_box(gtk::Orientation::Horizontal, 0);
    side_spacer.set_hexpand(true);
    pack(&side_chrome_row, &side_spacer, true);

    let side_lights_right = widget_box(gtk::Orientation::Horizontal, 2);
    side_lights_right.set_valign(gtk::Align::Center);
    side_lights_right.set_no_show_all(true);
    side_lights_right.hide();
    pack(&side_chrome_row, &side_lights_right, false);
    pack(&top_stack, &side_chrome_row, false);

    // Row 2: active Space pill (when multiple spaces exist) or "Tabs" heading + count.
    let band = widget_box(gtk::Orientation::Horizontal, 6);
    band.set_margin_start(12);
    band.set_margin_end(10);
    band.set_margin_top(2);
    band.set_margin_bottom(4);
    let side_space_btn = gtk::Button::new();
    class(&side_space_btn, "space-menu-button");
    class(&side_space_btn, "side-space-btn");
    side_space_btn.set_relief(gtk::ReliefStyle::None);
    side_space_btn.set_valign(gtk::Align::Center);
    side_space_btn.set_can_focus(true);
    side_space_btn.set_focus_on_click(false);
    let side_space_inner = widget_box(gtk::Orientation::Horizontal, 4);
    let side_space_label = gtk::Label::new(Some(crate::spaces::DEFAULT_SPACE_NAME));
    class(&side_space_label, "space-pill-label");
    side_space_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    side_space_label.set_max_width_chars(16);
    side_space_label.set_no_show_all(true);
    side_space_label.hide();
    pack(&side_space_inner, &side_space_label, false);
    let side_space_arrow = passthrough_icon(Icon::Down, 10, palette.muted);
    side_space_arrow.set_valign(gtk::Align::Center);
    pack(&side_space_inner, &side_space_arrow, false);
    side_space_btn.add(&side_space_inner);
    label_a11y(&side_space_btn, "Switch space", gtk::atk::Role::PushButton);
    side_space_btn.set_no_show_all(true);
    side_space_btn.hide();

    let side_tabs_heading = gtk::Label::new(Some("Tabs"));
    class(&side_tabs_heading, "sidebar-section");
    side_tabs_heading.set_xalign(0.0);
    side_tabs_heading.set_no_show_all(true);
    side_tabs_heading.show();
    pack(&band, &side_tabs_heading, false);

    let band_spacer = widget_box(gtk::Orientation::Horizontal, 0);
    band_spacer.set_hexpand(true);
    pack(&band, &band_spacer, true);

    let tab_count = gtk::Label::new(Some("1"));
    class(&tab_count, "sidebar-count");
    pack(&band, &tab_count, false);
    pack(&top_stack, &band, false);
    band.set_no_show_all(true);
    band.hide();
    pack(&body, &top, false);

    let pins_holder = widget_box(gtk::Orientation::Vertical, 0);
    pins_holder.set_margin_start(8);
    pins_holder.set_margin_end(8);
    pins_holder.set_margin_top(2);
    pins_holder.set_margin_bottom(6);
    pins_holder.set_no_show_all(true);
    let pins = gtk::Grid::new();
    class(&pins, "pin-rail");
    pins.set_column_spacing(6);
    pins.set_row_spacing(6);
    pins.set_column_homogeneous(false);
    pins.set_halign(gtk::Align::Start);
    pins.set_hexpand(false);
    pack(&pins_holder, &pins, false);
    let pin_divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    class(&pin_divider, "hairline");
    pin_divider.set_margin_top(8);
    pack(&pins_holder, &pin_divider, false);
    pins_holder.hide();
    pack(&body, &pins_holder, false);

    let rows = widget_box(gtk::Orientation::Vertical, 4);
    rows.set_margin_start(8);
    rows.set_margin_end(8);
    let rows_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    rows_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    rows_scroll.set_vexpand(true);
    rows_scroll.set_propagate_natural_height(false);
    rows_scroll.set_propagate_natural_width(false);
    rows_scroll.set_min_content_width(0);
    rows.set_size_request(0, -1);
    let rows_stack = widget_box(gtk::Orientation::Vertical, 0);
    pack(&rows_stack, &rows, false);
    rows_scroll.add(&rows_stack);
    pack(&body, &rows_scroll, true);

    let create = widget_box(gtk::Orientation::Horizontal, 4);
    create.set_margin_start(8);
    create.set_margin_end(8);
    create.set_margin_top(4);
    create.set_margin_bottom(8);
    let side_newtab = gtk::Button::new();
    class(&side_newtab, "newtab-row");
    side_newtab.set_hexpand(true);
    let create_content = widget_box(gtk::Orientation::Horizontal, 8);
    let create_icon = icons::area(Icon::Plus, 14, palette.muted);
    create_icon.set_valign(gtk::Align::Center);
    pack(&create_content, &create_icon, false);
    let create_label = gtk::Label::new(Some("New tab"));
    class(&create_label, "newtab-label");
    create_label.set_xalign(0.0);
    pack(&create_content, &create_label, true);
    side_newtab.add(&create_content);
    label_a11y(&side_newtab, "New tab (Ctrl T)", gtk::atk::Role::PushButton);
    pack(&create, &side_newtab, true);
    pack(&rows_stack, &create, false);

    let side_spaces_footer = widget_box(gtk::Orientation::Horizontal, 4);
    class(&side_spaces_footer, "space-footer");
    side_spaces_footer.set_margin_start(12);
    side_spaces_footer.set_margin_end(12);
    side_spaces_footer.set_margin_top(4);
    side_spaces_footer.set_margin_bottom(12);
    side_spaces_footer.set_no_show_all(true);
    side_spaces_footer.hide();
    let side_spaces = widget_box(gtk::Orientation::Horizontal, 2);
    class(&side_spaces, "space-dots");
    side_spaces.set_halign(gtk::Align::Center);
    let spaces_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    spaces_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    spaces_scroll.set_propagate_natural_width(false);
    spaces_scroll.set_min_content_width(0);
    spaces_scroll.set_hexpand(true);
    spaces_scroll.add(&side_spaces);
    pack(&side_spaces_footer, &spaces_scroll, true);
    pack(&side_spaces_footer, &side_space_btn, false);
    pack(&body, &side_spaces_footer, false);

    paned.pack1(&side, false, false);
    paned.set_position(plan.paned_at);

    // ---- Bookmarks bar (hidden unless populated/requested) ----
    let bmarkbar = gtk::EventBox::new();
    crate::ui::class(&bmarkbar, "bmarkbar");
    bmarkbar.set_visible_window(true);
    bmarkbar.set_halign(gtk::Align::Fill);
    let bmark_row = widget_box(gtk::Orientation::Horizontal, 8);
    bmark_row.set_margin_start(8);
    bmark_row.set_margin_end(8);
    bmark_row.set_margin_top(0);
    bmark_row.set_margin_bottom(4);
    let bmarks = widget_box(gtk::Orientation::Horizontal, 4);
    bmarks.set_hexpand(true);
    bmarks.set_halign(gtk::Align::Start);
    let bmarks_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    bmarks_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    bmarks_scroll.set_shadow_type(gtk::ShadowType::None);
    bmarks_scroll.set_propagate_natural_height(true);
    bmarks_scroll.set_propagate_natural_width(false);
    bmarks_scroll.set_min_content_width(0);
    bmarks_scroll.set_hexpand(true);
    bmarks_scroll.add(&bmarks);
    pack(&bmark_row, &bmarks_scroll, true);

    let bmark_search_box = widget_box(gtk::Orientation::Horizontal, 6);
    class(&bmark_search_box, "bmark-search");
    bmark_search_box.set_valign(gtk::Align::Center);
    bmark_search_box.set_halign(gtk::Align::End);
    bmark_search_box.set_hexpand(false);
    bmark_search_box.set_size_request(210, 24);
    let bmark_search_icon = icons::area(Icon::Search, 12, palette.muted);
    bmark_search_icon.set_valign(gtk::Align::Center);
    pack(&bmark_search_box, &bmark_search_icon, false);
    let bmark_search = gtk::Entry::new();
    class(&bmark_search, "bmark-search-entry");
    bmark_search.set_has_frame(false);
    bmark_search.set_width_chars(1);
    bmark_search.set_hexpand(true);
    bmark_search.set_placeholder_text(Some("Filter bookmarks"));
    label_a11y(&bmark_search, "Filter or autocomplete bookmarks", gtk::atk::Role::Text);
    {
        let box_focus = bmark_search_box.downgrade();
        bmark_search.connect_focus_in_event(move |_, _| {
            if let Some(b) = box_focus.upgrade() {
                class(&b, "focused");
            }
            crate::gtkx::inh(false)
        });
        let box_blur = bmark_search_box.downgrade();
        bmark_search.connect_focus_out_event(move |_, _| {
            if let Some(b) = box_blur.upgrade() {
                b.style_context().remove_class("focused");
            }
            crate::gtkx::inh(false)
        });
    }
    pack(&bmark_search_box, &bmark_search, true);
    pack(&bmark_row, &bmark_search_box, false);

    let bmark_layer = crate::layer::Layer::new();
    let bmark_card = widget_box(gtk::Orientation::Vertical, 0);
    class(&bmark_card, "bmark-suggest-card");
    bmark_card.set_size_request(360, -1);
    let bmark_pop_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    bmark_pop_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    bmark_pop_scroll.set_shadow_type(gtk::ShadowType::None);
    bmark_pop_scroll.set_propagate_natural_height(true);
    bmark_pop_scroll.set_max_content_height(260);
    bmark_pop_scroll.set_min_content_width(340);
    let bmark_suggest = widget_box(gtk::Orientation::Vertical, 2);
    class(&bmark_suggest, "suggest");
    bmark_suggest.set_hexpand(true);
    bmark_pop_scroll.add(&bmark_suggest);
    bmark_card.pack_start(&bmark_pop_scroll, true, true, 0);
    bmark_card.show_all();
    bmark_layer.add(&bmark_card);
    bmark_layer.root.set_no_show_all(true);
    bmark_layer.root.hide();

    bmarkbar.add(&bmark_row);
    bmarkbar.set_no_show_all(true);
    bmarkbar.hide();

    // ---- Main page column: optional compact 38px toolbar + rounded viewport frame ----
    let card = widget_box(gtk::Orientation::Vertical, 0);
    card.set_hexpand(true);
    card.set_vexpand(true);
    crate::ui::class(&card, "card");

    let toolbar = gtk::EventBox::new();
    class(&toolbar, "toolbar");
    toolbar.set_size_request(-1, 38);
    toolbar.set_no_show_all(true);
    toolbar.set_visible(prefs.show_toolbar);
    let tools = widget_box(gtk::Orientation::Horizontal, 4);
    tools.set_margin_start(8);
    tools.set_margin_end(8);
    tools.set_margin_top(2);
    tools.set_margin_bottom(4);
    toolbar.add(&tools);

    let back = icons::button(Icon::Back, 14, palette.muted, "chrome-tool");
    label_a11y(&back, "Back (Alt Left)", gtk::atk::Role::PushButton);
    let forward = icons::button(Icon::Forward, 14, palette.muted, "chrome-tool");
    label_a11y(&forward, "Forward (Alt Right)", gtk::atk::Role::PushButton);
    let reload = gtk::Button::new();
    reload.set_relief(gtk::ReliefStyle::None);
    class(&reload, "mark");
    class(&reload, "chrome-tool");
    reload.add(&passthrough_icon(Icon::Reload, 14, palette.muted));
    label_a11y(&reload, "Reload (Ctrl R)", gtk::atk::Role::PushButton);
    let home = icons::button(Icon::Home, 14, palette.muted, "chrome-tool");
    label_a11y(&home, "Home (Alt Home)", gtk::atk::Role::PushButton);
    for button in [&back, &forward, &reload, &home] {
        button.set_can_focus(true);
        button.set_focus_on_click(false);
        button.set_valign(gtk::Align::Center);
        pack(&tools, button, false);
        button.show_all();
        button.set_no_show_all(true);
    }

    let location = widget_box(gtk::Orientation::Horizontal, 0);
    class(&location, "location");
    location.set_valign(gtk::Align::Center);
    location.set_margin_start(4);
    location.set_margin_end(4);
    let site_info = gtk::Button::new();
    site_info.set_relief(gtk::ReliefStyle::None);
    class(&site_info, "mark");
    class(&site_info, "site-info");
    site_info.add(&passthrough_icon(Icon::Globe, 14, palette.muted));
    label_a11y(&site_info, "Site information (Ctrl I)", gtk::atk::Role::PushButton);
    site_info.set_can_focus(true);
    site_info.set_focus_on_click(false);
    let address = gtk::Button::new();
    class(&address, "address-button");
    address.set_hexpand(true);
    label_a11y(&address, "Search or enter an address (Ctrl L)", gtk::atk::Role::PushButton);
    let address_content = widget_box(gtk::Orientation::Horizontal, 0);
    let address_label = gtk::Label::new(Some("New tab"));
    class(&address_label, "address-label");
    address_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    address_label.set_max_width_chars(24);
    address_label.set_xalign(0.0);
    pack(&address_content, &address_label, false);
    let address_path = gtk::Label::new(Some(""));
    class(&address_path, "address-path");
    address_path.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    address_path.set_max_width_chars(24);
    address_path.set_xalign(0.0);
    pack(&address_content, &address_path, true);
    address.add(&address_content);
    pack(&location, &site_info, false);
    pack(&location, &address, true);
    pack(&tools, &location, true);

    let reader = icons::button(Icon::Book, 14, palette.muted, "chrome-tool");
    label_a11y(&reader, "Reading mode (Ctrl Shift R)", gtk::atk::Role::PushButton);
    let bookmark = icons::button(Icon::Star, 14, palette.muted, "chrome-tool");
    label_a11y(&bookmark, "Keep this page (Ctrl D)", gtk::atk::Role::PushButton);
    for button in [&reader, &bookmark] {
        button.set_can_focus(true);
        button.set_focus_on_click(false);
        button.set_valign(gtk::Align::Center);
        pack(&tools, button, false);
        button.show_all();
        button.set_no_show_all(true);
    }
    pack(&card, &toolbar, false);
    pack(&card, &bmarkbar, false);

    let stack = gtk::Stack::new();
    stack.set_homogeneous(false);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_vexpand_set(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(if crate::motion::enabled() { crate::motion::dur::MICRO as u32 } else { 0 });
    let blank_holder = gtk::EventBox::new();
    crate::ui::class(&blank_holder, "blank");
    blank_holder.set_visible_window(false);
    let start = crate::startpage::StartPage::new(palette);
    blank_holder.add(&start.root);
    stack.add_named(&blank_holder, "blank");

    let error_holder = gtk::EventBox::new();
    crate::ui::class(&error_holder, "error-page");
    error_holder.set_visible_window(false);
    error_holder.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    let errorpage = widget_box(gtk::Orientation::Vertical, 8);
    crate::ui::class(&errorpage, "error-content");
    errorpage.set_valign(gtk::Align::Center);
    errorpage.set_halign(gtk::Align::Center);
    let error_glyph = gtk::EventBox::new();
    class(&error_glyph, "error-glyph");
    error_glyph.set_halign(gtk::Align::Center);
    error_glyph.set_margin_bottom(16);
    error_glyph.add(&icons::area(Icon::Warning, 48, palette.accent()));
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
    let error_actions = widget_box(gtk::Orientation::Horizontal, 8);
    crate::ui::class(&error_actions, "error-actions");
    error_actions.set_halign(gtk::Align::Center);
    error_actions.set_margin_top(12);
    let error_retry = gtk::Button::with_label("Try again");
    crate::ui::class(&error_retry, "act");
    class(&error_retry, "primary");
    error_retry.set_halign(gtk::Align::Center);
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
    stack.add_named(&error_holder, "error");

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
        let fill_w = (w * frac).max(h).min(w);
        let r = h * 0.5;
        let rgba = widget.style_context().color(gtk::StateFlags::NORMAL);
        cr.set_source_rgba(rgba.red(), rgba.green(), rgba.blue(), rgba.alpha());
        // Rounded pill progress bar with a smooth liquid leading cap.
        cr.new_sub_path();
        cr.move_to(0.0, 0.0);
        cr.line_to((fill_w - r).max(0.0), 0.0);
        cr.arc(
            (fill_w - r).max(r),
            r,
            r,
            -std::f64::consts::FRAC_PI_2,
            std::f64::consts::FRAC_PI_2,
        );
        cr.line_to(0.0, h);
        cr.close_path();
        let _ = cr.fill();
        // Subtle crisp leading droplet highlight while actively loading (< 98%).
        if frac < 0.98 && fill_w > h * 2.0 {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.28);
            cr.arc((fill_w - r).max(r), r, r * 0.65, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
        crate::gtkx::inh(false)
    });
    page_overlay.add_overlay(&progress);
    page_overlay.set_overlay_pass_through(&progress, true);

    // Live anti-aliased Cairo corner masks over the WebKitGTK viewport, matching
    // `.viewport-frame`'s 16px outer radius (15px inner radius + 1px border).
    // Four tiny 16x16 pass-through overlays avoid full-viewport redraw overhead
    // while genuinely rounding the live WebView on both X11 and Wayland.
    let viewport_corner_colors = std::rc::Rc::new(std::cell::Cell::new((palette.ground, palette.hairline)));
    let viewport_corners = attach_corner_masks(
        &page_overlay,
        15.0,
        true,
        viewport_corner_colors.clone(),
        "viewport-corner",
    );

    // Continuous 12px-rounded viewport container inset cleanly inside the
    // ground shell, eliminating sharp 90° structural seams.
    let viewport_frame = widget_box(gtk::Orientation::Vertical, 0);
    class(&viewport_frame, "viewport-frame");
    viewport_frame.set_hexpand(true);
    viewport_frame.set_vexpand(true);
    viewport_frame.set_margin_start(6);
    viewport_frame.set_margin_end(6);
    viewport_frame.set_margin_top(2);
    viewport_frame.set_margin_bottom(6);
    pack(&viewport_frame, &page_overlay, true);
    pack(&card, &viewport_frame, true);

    paned.pack2(&card, true, true);

    let frame = widget_box(gtk::Orientation::Vertical, 0);
    crate::ui::class(&frame, "shell");
    pack(&frame, &topbar, false);
    pack(&frame, &paned, true);
    overlay.add(&frame);

    Ui {
        window,
        overlay,
        shell: frame,
        topbar,
        top_lights_left,
        top_lights_right,
        top_more_slot,
        top_space_btn,
        top_space_label,
        top_pins_holder,
        top_pins,
        top_tabs_scroll,
        top_tabs,
        top_drag,
        paned,
        side,
        strip,
        body,
        top,
        side_lights_left,
        side_lights_right,
        side_more_slot,
        side_space_btn,
        side_space_label,
        side_spaces_footer,
        side_spaces,
        side_tabs_heading,
        toolbar,
        tools,
        location,
        viewport_frame,
        viewport_corners,
        viewport_corner_colors,
        side_fold,
        side_newtab,
        address,
        address_label,
        address_path,
        site_info,
        reader,
        bookmark,
        more,
        newtab,
        tab_count,
        start,
        light_red,
        light_amber,
        light_green,
        back,
        forward,
        reload,
        home,
        pins,
        pins_holder,
        rows,
        rows_scroll,
        stack,
        progress,
        progress_frac,
        bmarkbar,
        bmarks,
        bmark_search_box,
        bmark_search,
        bmark_layer,
        bmark_card,
        bmark_suggest,
        error_holder,
        errorpage,
        error_glyph,
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
        e.set_size_request(26, 26);
        e.set_valign(gtk::Align::Center);
        e.add_events(gtk::gdk::EventMask::ENTER_NOTIFY_MASK | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK);
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

/// Scale a WebKit favicon `cairo::Surface` onto an exact `size × size`
/// ARGB32 `cairo::ImageSurface` so favicons render crisply at 14–16px everywhere
/// (tabs, bookmarks bar, start page chips, and autocomplete rows) and can be
/// persisted to PNG without any extra crates.
pub fn scale_favicon(surface: &cairo::Surface, size: i32) -> Option<cairo::ImageSurface> {
    let size = size.max(1);
    let out = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    let cr = cairo::Context::new(&out).ok()?;
    let (sw, sh) = match cairo::ImageSurface::try_from(surface.clone()) {
        Ok(img) if img.width() > 0 && img.height() > 0 => (img.width() as f64, img.height() as f64),
        _ => (size as f64, size as f64),
    };
    let scale = (size as f64 / sw).min(size as f64 / sh);
    let dx = (size as f64 - sw * scale) * 0.5;
    let dy = (size as f64 - sh * scale) * 0.5;
    cr.translate(dx, dy);
    cr.scale(scale, scale);
    cr.set_source_surface(surface, 0.0, 0.0).ok()?;
    cr.source().set_filter(cairo::Filter::Best);
    cr.paint().ok()?;
    drop(cr);
    out.flush();
    Some(out)
}

/// A page's own mark, scaled cleanly to the exact pixel size requested.
pub fn favicon(surface: &cairo::Surface, size: i32) -> gtk::Image {
    if let Some(scaled) = scale_favicon(surface, size) {
        let image = gtk::Image::from_surface(Some(&scaled));
        image.set_size_request(size, size);
        return image;
    }
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
            let mark = passthrough_icon(glyph, size, color);
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
    let sweep = std::rc::Rc::new(std::cell::Cell::new(std::f64::consts::TAU * 0.68));
    let for_draw = angle.clone();
    let sweep_draw = sweep.clone();
    area.connect_draw(move |widget, cr| {
        let a = for_draw.get();
        let s = sweep_draw.get();
        icons::set_source_color(widget, cr, colour);
        cr.set_line_width(1.7);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.arc(7.0, 7.0, 5.0, a, a + s);
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
    // so a spinner in a hidden or folded row costs no wakeups at all. The arc
    // breathes smoothly between 28% and 74% circumference while rotating, so
    // active page loads read as fluid rather than a mechanical constant ring.
    let start = std::rc::Rc::new(std::cell::Cell::new(None::<i64>));
    let _ = area.add_tick_callback(move |area, clock| {
        if !crate::motion::enabled() {
            sweep.set(std::f64::consts::TAU * 0.68);
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
        let elapsed_s = (now - t0).max(0) as f64 / 1_000_000.0;
        let rot = (elapsed_s / 1.25).fract() * std::f64::consts::TAU;
        let breath = 0.5 - 0.5 * (elapsed_s * std::f64::consts::TAU / 1.65).cos();
        angle.set(rot + breath * 0.45);
        sweep.set(std::f64::consts::TAU * (0.28 + 0.46 * breath));
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
    // Compact rows are one line. A second line inside a 34px row collided
    // with the next tab; only the roomy preference shows the URL.
    subtitle.set_no_show_all(true);
    subtitle.set_visible(row_height >= 44);
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

/// One compact horizontal top tab in the top tab strip.
pub fn top_tab_chip(
    title: &str,
    palette: Palette,
    letter: Option<char>,
    live: bool,
    state: TabState,
) -> TabRow {
    let event = gtk::EventBox::new();
    crate::ui::class(&event, "toptab");
    if live {
        crate::ui::class(&event, "live");
    }
    if state.asleep {
        crate::ui::class(&event, "asleep");
    }
    if state.private {
        crate::ui::class(&event, "private");
    }
    event.set_size_request(144, 28);
    event.set_hexpand(false);
    event.set_halign(gtk::Align::Start);
    event.set_valign(gtk::Align::Center);

    let hbox = widget_box(gtk::Orientation::Horizontal, 6);
    hbox.set_margin_start(8);
    hbox.set_margin_end(4);
    hbox.set_valign(gtk::Align::Center);

    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_size_request(16, 16);
    icon.set_valign(gtk::Align::Center);
    let letter_label = match letter {
        Some(letter) => mark(&icon, letter, None, 14),
        None => mark_glyph(&icon, Icon::Plus, None, 13, palette.faint),
    };
    if state.private {
        for child in icon.children() {
            icon.remove(&child);
        }
        mark_glyph(&icon, Icon::Mask, None, 13, palette.muted);
    }
    if state.loading && !state.private {
        loading_mark(&icon, palette.muted);
    }
    pack(&hbox, &icon, false);

    let label = gtk::Label::new(Some(title));
    crate::ui::class(&label, "toptab-title");
    if live {
        crate::ui::class(&label, "liveword");
    }
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_single_line_mode(true);
    label.set_hexpand(false);
    label.set_valign(gtk::Align::Center);
    label.set_width_chars(8);
    label.set_max_width_chars(15);
    pack(&hbox, &label, true);

    let subtitle = gtk::Label::new(None);
    subtitle.set_no_show_all(true);
    subtitle.hide();

    let close = gtk::Button::new();
    close.set_relief(gtk::ReliefStyle::None);
    close.add(&icons::area(Icon::Close, 10, palette.muted.mix(palette.faint, 0.4)));
    crate::ui::class(&close, "loupe");
    crate::ui::class(&close, "close");
    crate::ui::class(&close, "rowclose");
    crate::ui::class(&close, "toptab-close");
    close.set_opacity(close_opacity(live, false));
    close.set_valign(gtk::Align::Center);
    close.set_size_request(20, 20);
    close.set_can_focus(false);
    close.set_tooltip_text(Some("Close tab (Ctrl W)"));
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

/// One compact Zen-style favicon-only pinned tab chip in the top tab strip.
pub fn top_pin_chip(letter: &str, live: bool, private: bool, _palette: Palette) -> PinWidget {
    let event = gtk::EventBox::new();
    event.set_above_child(true);
    crate::ui::class(&event, "toppin");
    if live {
        crate::ui::class(&event, "live");
    }
    if private {
        crate::ui::class(&event, "private");
    }
    event.set_size_request(30, 30);
    event.set_valign(gtk::Align::Center);
    event.set_can_focus(true);
    event.set_focus_on_click(false);
    let label = gtk::Label::new(Some(&letter.to_uppercase()));
    crate::ui::class(&label, "pinletter");
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    label.set_no_show_all(true);
    label.show();
    let stack = gtk::Overlay::new();
    stack.add(&label);
    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    icon.set_no_show_all(true);
    icon.hide();
    stack.add_overlay(&icon);
    stack.set_overlay_pass_through(&icon, true);
    event.add(&stack);
    PinWidget { event, letter: label, icon }
}

/// A pinned page: one square, one letter, or its site's mark.
pub struct PinWidget {
    pub event: gtk::EventBox,
    pub letter: gtk::Label,
    pub icon: gtk::Box,
}

pub fn pin_tile(letter: &str, live: bool, private: bool, _palette: Palette) -> PinWidget {
    let event = gtk::EventBox::new();
    event.set_above_child(true);
    crate::ui::class(&event, "pin");
    if live {
        crate::ui::class(&event, "live");
    }
    if private {
        crate::ui::class(&event, "private");
    }
    event.set_size_request(36, 36);
    event.set_can_focus(true);
    event.set_focus_on_click(false);
    let label = gtk::Label::new(Some(&letter.to_uppercase()));
    crate::ui::class(&label, "pinletter");
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    // Opted out of show_all, and shown here instead. The column is redrawn with
    // show_all every time its tabs change, and that call would otherwise put
    // the letter back over the favicon the browser had just hidden it for.
    label.set_no_show_all(true);
    label.show();
    let stack = gtk::Overlay::new();
    stack.add(&label);
    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    icon.set_no_show_all(true);
    icon.hide();
    stack.add_overlay(&icon);
    stack.set_overlay_pass_through(&icon, true);
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
        (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0, 0.62)
    } else {
        // A light scrim is a black veil, not white paint over the page.
        // Gemini R60 polish: at 0.28 a busy light page competed with the
        // omnibox and panels above it. At 0.42 the page drops back behind the
        // open layer and its text still shows. As with the dark value, it is
        // set only here, and the capture in app.rs reads scrim_colour.
        (0.0, 0.0, 0.0, 0.24)
    }
}

pub fn scrim_with_alpha(palette: &Palette) -> (gtk::DrawingArea, std::rc::Rc<std::cell::Cell<f64>>) {
    let area = gtk::DrawingArea::new();
    crate::ui::class(&area, "scrim");
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    let alpha = std::rc::Rc::new(std::cell::Cell::new(1.0_f64));
    let alpha_draw = alpha.clone();
    let (r, g, b, a) = scrim_colour(palette);
    area.connect_draw(move |_, context| {
        let factor = alpha_draw.get().clamp(0.0, 1.0);
        if factor > 0.001 {
            context.set_source_rgba(r, g, b, a * factor);
            let _ = context.paint();
        }
        crate::gtkx::inh(false)
    });
    (area, alpha)
}




/// Attach four tiny pass-through Cairo corner overlays to `overlay` so a live
/// child surface (such as `WebKitWebView`) is genuinely rounded at runtime on
/// both X11 and Wayland without full-surface overdraw or blocking pointer input.
///
/// When `with_border` is true, each corner paints the exterior spandrel in
/// `colors.0` (`ground`) and strokes a continuous 1px inner border arc at
/// `inner_radius + 0.5` in `colors.1` (`hairline`) aligned with the enclosing
/// 1px CSS frame. When `with_border` is false, it paints only the anti-aliased
/// spandrel outside `inner_radius`.
pub fn attach_corner_masks(
    overlay: &gtk::Overlay,
    inner_radius: f64,
    with_border: bool,
    colors: std::rc::Rc<std::cell::Cell<(Rgb, Rgb)>>,
    class_name: &str,
) -> [gtk::DrawingArea; 4] {
    let size = inner_radius.ceil() as i32 + 1;
    let specs = [
        (gtk::Align::Start, gtk::Align::Start, false, false, "tl"),
        (gtk::Align::End, gtk::Align::Start, true, false, "tr"),
        (gtk::Align::Start, gtk::Align::End, false, true, "bl"),
        (gtk::Align::End, gtk::Align::End, true, true, "br"),
    ];
    specs.map(|(halign, valign, flip_x, flip_y, tag)| {
        let corner = gtk::DrawingArea::new();
        corner.set_widget_name(&format!("{class_name}-{tag}"));
        class(&corner, class_name);
        corner.set_size_request(size, size);
        corner.set_halign(halign);
        corner.set_valign(valign);
        corner.set_can_focus(false);
        corner.set_no_show_all(true);
        corner.show();
        if let Some(access) = corner.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::RedundantObject);
        }
        let colors = colors.clone();
        corner.connect_draw(move |widget, cr| {
            let w = widget.allocated_width() as f64;
            let h = widget.allocated_height() as f64;
            if w <= 0.0 || h <= 0.0 {
                return crate::gtkx::inh(false);
            }
            let (outer, border) = colors.get();
            let _ = cr.save();
            if flip_x {
                cr.translate(w, 0.0);
                cr.scale(-1.0, 1.0);
            }
            if flip_y {
                cr.translate(0.0, h);
                cr.scale(1.0, -1.0);
            }
            let center = inner_radius;
            let spandrel_radius = if with_border { inner_radius + 0.5 } else { inner_radius };
            cr.new_path();
            cr.move_to(-1.0, -1.0);
            cr.line_to(center, -1.0);
            cr.arc_negative(
                center,
                center,
                spandrel_radius,
                -std::f64::consts::FRAC_PI_2,
                std::f64::consts::PI,
            );
            cr.line_to(-1.0, center);
            cr.close_path();
            cr.set_source_rgb(
                outer.0 as f64 / 255.0,
                outer.1 as f64 / 255.0,
                outer.2 as f64 / 255.0,
            );
            let _ = cr.fill();
            if with_border {
                cr.new_path();
                cr.arc(
                    center,
                    center,
                    inner_radius + 0.5,
                    std::f64::consts::PI,
                    1.5 * std::f64::consts::PI,
                );
                cr.set_source_rgb(
                    border.0 as f64 / 255.0,
                    border.1 as f64 / 255.0,
                    border.2 as f64 / 255.0,
                );
                cr.set_line_width(1.0);
                let _ = cr.stroke();
            }
            let _ = cr.restore();
            crate::gtkx::inh(false)
        });
        overlay.add_overlay(&corner);
        overlay.set_overlay_pass_through(&corner, true);
        corner
    })
}

/// Build an exact pixel-center rounded-rectangle `cairo::Region` for shaping
/// non-composited X11 popup windows (such as `GtkMenu` toplevels) so rounded
/// menus never show rectangular outer backing corners when no compositor runs.
pub fn rounded_rect_region(width: i32, height: i32, radius: i32) -> cairo::Region {
    if width <= 0 || height <= 0 {
        return cairo::Region::create();
    }
    let r = radius.min(width / 2).min(height / 2).max(0);
    if r == 0 {
        return cairo::Region::create_rectangle(&cairo::RectangleInt::new(0, 0, width, height));
    }
    let region = cairo::Region::create();
    let mid_h = height - 2 * r;
    if mid_h > 0 {
        let _ = region.union_rectangle(&cairo::RectangleInt::new(0, r, width, mid_h));
    }
    let rf = r as f64;
    for y in 0..r {
        let dy = rf - (y as f64 + 0.5);
        let dx = (rf * rf - dy * dy).max(0.0).sqrt();
        let inset = (rf - dx - 0.5).ceil().max(0.0) as i32;
        let span = (width - 2 * inset).max(0);
        if span > 0 {
            let _ = region.union_rectangle(&cairo::RectangleInt::new(inset, y, span, 1));
            let _ = region.union_rectangle(&cairo::RectangleInt::new(inset, height - 1 - y, span, 1));
        }
    }
    region
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
        for folded in [false, true] {
            let plan = column_plan(folded, false, false, 260);
            assert_eq!(plan.width, 0);
            assert_eq!(plan.paned_at, 0);
            assert!(!plan.strip);
            assert!(!plan.body, "disabled sidebar must hide body (folded={folded})");
            assert!(plan.side_shown);
        }
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

    #[test]
    fn rounded_rect_region_excludes_outer_corners_and_includes_interior() {
        let region = rounded_rect_region(160, 200, 12);
        // All four extreme corners outside the 12px arc are excluded.
        for (x, y) in [(0, 0), (1, 1), (2, 2), (159, 0), (0, 199), (159, 199)] {
            assert!(!region.contains_point(x, y), "corner ({x},{y}) must be clipped out");
        }
        // Straight edges past the radius and interior remain included.
        for (x, y) in [(12, 0), (147, 0), (0, 12), (159, 12), (80, 100), (12, 199)] {
            assert!(region.contains_point(x, y), "interior/edge ({x},{y}) must stay inside");
        }
    }

    #[test]
    fn scale_favicon_produces_exact_square_image_surface() {
        let src = cairo::ImageSurface::create(cairo::Format::ARgb32, 32, 32).expect("src");
        let cr = cairo::Context::new(&src).expect("ctx");
        cr.set_source_rgb(0.2, 0.5, 0.8);
        let _ = cr.paint();
        drop(cr);
        let base: &cairo::Surface = &src;
        let scaled = scale_favicon(base, 16).expect("scaled");
        assert_eq!((scaled.width(), scaled.height()), (16, 16));
    }

    #[test]
    fn tab_layout_plan_is_strictly_exclusive_outside_fullscreen() {
        let top = tab_layout_plan(false, false, false);
        assert_eq!(top, TabLayoutPlan { show_topbar: true, show_sidebar: false });

        let vertical = tab_layout_plan(true, false, false);
        assert_eq!(vertical, TabLayoutPlan { show_topbar: false, show_sidebar: true });

        // Folded legacy sidebar state falls back to topbar so chrome is never lost.
        let folded = tab_layout_plan(true, true, false);
        assert_eq!(folded, TabLayoutPlan { show_topbar: true, show_sidebar: false });

        // Fullscreen hides BOTH layouts unconditionally.
        for (sidebar, folded) in [(false, false), (true, false), (true, true)] {
            let full = tab_layout_plan(sidebar, folded, true);
            assert_eq!(full, TabLayoutPlan { show_topbar: false, show_sidebar: false });
        }
    }
}
