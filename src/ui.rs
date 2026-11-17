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
    pub shell: gtk::Paned,
    pub side: gtk::Box,
    pub lights: gtk::Box,
    pub light_red: gtk::EventBox,
    pub light_amber: gtk::EventBox,
    pub light_green: gtk::EventBox,
    pub helm: gtk::Box,
    pub back: gtk::Button,
    pub forward: gtk::Button,
    pub reload: gtk::Button,
    pub pins: gtk::Grid,
    pub pins_holder: gtk::Box,
    pub rows: gtk::Box,
    pub rows_scroll: gtk::ScrolledWindow,
    pub footer: gtk::Box,
    pub menu: gtk::Button,
    pub card: gtk::Box,
    pub stack: gtk::Stack,
    pub blank: gtk::Box,
    pub palette: Palette,
}

pub struct Chrome {
    pub row_height: i32,
}

fn widget_box(orientation: gtk::Orientation, spacing: i32) -> gtk::Box {
    gtk::Box::new(orientation, spacing)
}

/// Add a child to a box with the expansion it should have. GtkBox's own
/// container-add stretches everything it is given, which is never what a
/// column of fixed rows and one page wants.
pub fn pack<B: IsA<gtk::Box>, C: IsA<gtk::Widget>>(container: &B, child: &C, expand: bool) {
    container.pack_start(child, expand, true, 0);
}

/// The column's edge: the handle between the tabs and the page.
pub fn set_side_width(paned: &gtk::Paned, width: i32) {
    paned.set_position(width);
}

/// Add a child that keeps exactly the width it asks for.
pub fn pack_fixed<B: IsA<gtk::Box>, C: IsA<gtk::Widget>>(container: &B, child: &C) {
    container.pack_start(child, false, false, 0);
}

pub fn build(window: gtk::Window, palette: Palette, prefs: &crate::store::Prefs, composited: bool) -> Ui {
    let overlay = gtk::Overlay::new();

    // The column and the page, with the column's edge between them: a paned
    // window keeps the width the column was given, and lets it be dragged.
    let paned = gtk::Paned::new(gtk::Orientation::Horizontal);
    paned.style_context().add_class("shell");
    paned.set_wide_handle(false);
    paned.set_hexpand(true);
    paned.set_vexpand(true);
    let side_width = prefs.side_width.round() as i32;
    let side = widget_box(gtk::Orientation::Vertical, 0);
    side.style_context().add_class("side");
    side.set_size_request(if prefs.sidebar { side_width } else { 0 }, -1);

    // The lights and the page's own doors, in the band they share.
    let top = widget_box(gtk::Orientation::Horizontal, 0);
    top.set_size_request(-1, 40);
    let lights = widget_box(gtk::Orientation::Horizontal, 9);
    lights.set_margin_start(8);
    lights.set_margin_top(8);
    lights.set_valign(gtk::Align::Start);
    let (light_red, light_amber, light_green) = traffic_lights();
    pack(&lights, &light_red, false);
    pack(&lights, &light_amber, false);
    pack(&lights, &light_green, false);
    pack(&top, &lights, false);

    let helm = widget_box(gtk::Orientation::Horizontal, 0);
    helm.set_margin_start(16);
    helm.set_margin_top(6);
    helm.set_valign(gtk::Align::Start);
    helm.style_context().add_class("helm");
    helm.set_no_show_all(true);
    let ink_muted = palette.muted;
    let back = icons::button(Icon::Back, 14, ink_muted, "door");
    let forward = icons::button(Icon::Forward, 14, ink_muted, "door");
    let reload = icons::button(Icon::Reload, 14, ink_muted, "door");
    pack(&helm, &back, false);
    pack(&helm, &forward, false);
    pack(&helm, &reload, false);
    pack(&top, &helm, false);
    // Nothing here expands: the column keeps the width it is given.

    pack(&side, &top, false);

    // Pinned pages: squares that keep their place.
    let pins_holder = widget_box(gtk::Orientation::Vertical, 0);
    pins_holder.set_margin_start(10);
    pins_holder.set_margin_end(10);
    pins_holder.set_margin_top(2);
    let pins = gtk::Grid::new();
    pins.set_column_spacing(4);
    pins.set_row_spacing(4);
    pins.set_halign(gtk::Align::Start);
    pack(&pins_holder, &pins, false);
    pins_holder.set_no_show_all(true);
    pack(&side, &pins_holder, false);

    // The rows.
    let rows = widget_box(gtk::Orientation::Vertical, 2);
    rows.set_margin_start(10);
    rows.set_margin_end(10);
    rows.set_margin_top(4);
    let rows_scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    rows_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    rows_scroll.set_vexpand(true);
    rows_scroll.set_propagate_natural_height(false);
    rows_scroll.set_propagate_natural_width(false);
    rows_scroll.set_min_content_width(0);
    rows.set_size_request(0, -1);
    rows_scroll.add(&rows);
    pack(&side, &rows_scroll, true);

    // The foot: one door to everything else.
    let footer = widget_box(gtk::Orientation::Horizontal, 0);
    footer.set_size_request(-1, 46);
    footer.set_margin_start(12);
    footer.set_margin_end(12);
    let menu = icons::button(Icon::Menu, 16, palette.muted, "foot");
    menu.set_valign(gtk::Align::Center);
    pack(&footer, &menu, false);
    pack(&side, &footer, false);

    paned.pack1(&side, false, false);

    // The page, edge to edge.
    let card = widget_box(gtk::Orientation::Vertical, 0);
    card.set_hexpand(true);
    card.set_vexpand(true);
    card.style_context().add_class("card");
    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_vexpand_set(true);
    let blank = widget_box(gtk::Orientation::Vertical, 6);
    blank.style_context().add_class("blank");
    blank.set_valign(gtk::Align::Center);
    blank.set_halign(gtk::Align::Center);
    let blank_title = gtk::Label::new(Some("Search"));
    blank_title.style_context().add_class("blanktitle");
    let blank_hint = gtk::Label::new(Some("Type an address, or a few words.  Ctrl L for the field."));
    blank_hint.style_context().add_class("blankhint");
    pack(&blank, &blank_title, false);
    pack(&blank, &blank_hint, false);
    stack.add_named(&blank, "blank");
    pack(&card, &stack, true);
    paned.pack2(&card, true, false);

    overlay.add(&paned);

    let ui = Ui {
        window,
        overlay,
        shell: paned,
        side,
        lights,
        light_red,
        light_amber,
        light_green,
        helm,
        back,
        forward,
        reload,
        pins,
        pins_holder,
        rows,
        rows_scroll,
        footer,
        menu,
        card,
        stack,
        blank,
        palette,
    };
    let _ = composited;
    ui
}

fn traffic_lights() -> (gtk::EventBox, gtk::EventBox, gtk::EventBox) {
    let make = |class: &str| {
        let e = gtk::EventBox::new();
        e.style_context().add_class("light");
        e.style_context().add_class(class);
        e.set_size_request(14, 14);
        e.set_valign(gtk::Align::Center);
        e
    };
    (make("red"), make("amber"), make("green"))
}

/// One tab's line: a small mark, the title, and a cross when you are on it.
pub struct TabRow {
    pub event: gtk::EventBox,
    pub title: gtk::Label,
    pub letter: gtk::Label,
    pub icon: gtk::Box,
    pub close: gtk::Button,
    pub box_: gtk::Box,
}

pub fn tab_row(title: &str, palette: Palette, row_height: i32, letter: Option<char>, live: bool) -> TabRow {
    let event = gtk::EventBox::new();
    event.style_context().add_class("row");
    if live {
        event.style_context().add_class("live");
    }
    event.set_size_request(-1, row_height);

    let hbox = widget_box(gtk::Orientation::Horizontal, 6);
    hbox.set_margin_start(8);
    hbox.set_margin_end(6);

    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_size_request(18, 18);
    icon.set_valign(gtk::Align::Center);
    let mut letter_label = gtk::Label::new(None);
    match letter {
        Some(c) => {
            let tile = gtk::EventBox::new();
            tile.style_context().add_class("tile");
            tile.set_size_request(18, 18);
            tile.set_valign(gtk::Align::Center);
            let label = gtk::Label::new(Some(&c.to_uppercase().to_string()));
            label.style_context().add_class("tileletter");
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            label.set_width_chars(1);
            label.set_max_width_chars(1);
            tile.add(&label);
            letter_label = label;
            pack(&icon, &tile, false);
        }
        None => {
            let mark = icons::area(Icon::Globe, 16, palette.muted);
            mark.set_valign(gtk::Align::Center);
            pack(&icon, &mark, false);
        }
    }
    pack(&hbox, &icon, false);

    let label = gtk::Label::new(Some(title));
    label.style_context().add_class("rowtitle");
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_hexpand(true);
    label.set_valign(gtk::Align::Center);
    label.set_max_width_chars(1);
    pack(&hbox, &label, true);

    let close = gtk::Button::new();
    close.set_relief(gtk::ReliefStyle::None);
    close.add(&icons::area(Icon::Close, 12, palette.muted.mix(palette.faint, 0.4)));
    close.style_context().add_class("loupe");
    close.style_context().add_class("close");
    close.set_valign(gtk::Align::Center);
    close.set_size_request(18, 18);
    close.set_can_focus(false);
    pack(&hbox, &close, false);

    event.add(&hbox);
    TabRow { event, title: label, letter: letter_label, icon, close, box_: hbox }
}

/// A pinned page: one square, one letter, or its site's mark.
pub struct PinWidget {
    pub event: gtk::EventBox,
    pub letter: gtk::Label,
}

pub fn pin_tile(letter: char, palette: Palette, live: bool) -> PinWidget {
    let event = gtk::EventBox::new();
    event.style_context().add_class("pin");
    if live {
        event.style_context().add_class("live");
    }
    event.set_size_request(34, 34);
    let label = gtk::Label::new(Some(&letter.to_uppercase().to_string()));
    label.style_context().add_class("pinletter");
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    let _ = palette;
    event.add(&label);
    PinWidget { event, letter: label }
}

/// The dimming behind a field or a panel. A drawing area rather than an event
/// box: it needs no window of its own, so it stacks where it is put.
pub fn scrim(palette: &Palette) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.style_context().add_class("scrim");
    area.set_hexpand(true);
    area.set_vexpand(true);
    area.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    let (r, g, b, a) = if palette.is_dark() {
        let c = palette.ground;
        (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0, 0.62)
    } else {
        let c = palette.card;
        (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0, 0.74)
    };
    area.connect_draw(move |_, context| {
        context.set_source_rgba(r, g, b, a);
        let _ = context.paint();
        crate::gtkx::inh(false)
    });
    area
}

pub fn separator(palette: Palette) -> gtk::Box {
    let line = widget_box(gtk::Orientation::Horizontal, 0);
    line.style_context().add_class("hairline");
    line.set_size_request(-1, 1);
    line.set_margin_top(6);
    line.set_margin_bottom(6);
    let _ = palette;
    line
}

pub fn ink(palette: Palette) -> Rgb {
    palette.ink
}
