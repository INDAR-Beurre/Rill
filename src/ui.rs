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
    pub menu: gtk::Button,
    pub stack: gtk::Stack,
    pub blank_holder: gtk::EventBox,
    pub errorpage: gtk::Box,
    pub error_title: gtk::Label,
    pub error_reason: gtk::Label,
    pub error_address: gtk::Label,
    pub error_retry: gtk::Button,
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
    side.set_size_request(if prefs.sidebar { side_width } else { 0 }, -1);

    // The lights and the page's own doors, in the band they share.
    let top = widget_box(gtk::Orientation::Horizontal, 0);
    top.set_size_request(-1, 40);
    let lights = widget_box(gtk::Orientation::Horizontal, 2);
    lights.set_margin_start(12);
    lights.set_margin_top(14);
    lights.set_valign(gtk::Align::Start);
    let (light_red, light_amber, light_green) = window_controls(palette);
    pack(&lights, &light_red, false);
    pack(&lights, &light_amber, false);
    pack(&lights, &light_green, false);
    pack(&top, &lights, false);

    let helm = widget_box(gtk::Orientation::Horizontal, 0);
    helm.set_margin_start(16);
    helm.set_margin_top(6);
    helm.set_valign(gtk::Align::Start);
    crate::ui::class(&helm, "helm");
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
    pins_holder.set_margin_top(24);
    let pins = gtk::Grid::new();
    pins.set_column_spacing(3);
    pins.set_row_spacing(4);
    pins.set_halign(gtk::Align::Start);
    pack(&pins_holder, &pins, false);
    pins_holder.set_no_show_all(true);
    pack(&side, &pins_holder, false);

    // The rows.
    let rows = widget_box(gtk::Orientation::Vertical, 2);
    rows.set_margin_start(10);
    rows.set_margin_end(9);
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
    crate::ui::class(&card, "card");
    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_vexpand_set(true);
    let blank_holder = gtk::EventBox::new();
    crate::ui::class(&blank_holder, "blank");
    blank_holder.set_visible_window(false);
    blank_holder.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
    let blank = widget_box(gtk::Orientation::Vertical, 6);
    crate::ui::class(&blank, "blank");
    blank.set_valign(gtk::Align::Center);
    blank.set_halign(gtk::Align::Center);
    let blank_title = gtk::Label::new(Some("Search"));
    crate::ui::class(&blank_title, "blanktitle");
    // A field to click, not just a sentence telling you a key exists: the
    // whole page opens the address field already, but a stranger looks for a
    // box. It is a label in a frame, so it costs no entry and no caret.
    let fake_field = gtk::EventBox::new();
    crate::ui::class(&fake_field, "fakefield");
    fake_field.set_size_request(340, -1);
    let fake_row = widget_box(gtk::Orientation::Horizontal, 8);
    fake_row.set_margin_start(10);
    fake_row.set_margin_end(10);
    fake_row.set_margin_top(7);
    fake_row.set_margin_bottom(7);
    let magnifier = icons::area(Icon::Search, 13, palette.muted);
    magnifier.set_valign(gtk::Align::Center);
    pack(&fake_row, &magnifier, false);
    let placeholder = gtk::Label::new(Some("Search or enter an address"));
    crate::ui::class(&placeholder, "fakefieldtext");
    placeholder.set_xalign(0.0);
    placeholder.set_hexpand(true);
    pack(&fake_row, &placeholder, true);
    let keycap = gtk::Label::new(Some("Ctrl L"));
    crate::ui::class(&keycap, "keyhint");
    keycap.set_valign(gtk::Align::Center);
    pack(&fake_row, &keycap, false);
    fake_field.add(&fake_row);
    pack(&blank, &blank_title, false);
    pack(&blank, &fake_field, false);
    blank_holder.add(&blank);
    stack.add_named(&blank_holder, "blank");

    // A page that will not open is the browser's to explain, not the engine's
    // to dump: the same quiet type, the reason, the address it tried, and one
    // thing to do about it.
    let errorpage = widget_box(gtk::Orientation::Vertical, 8);
    crate::ui::class(&errorpage, "blank");
    errorpage.set_valign(gtk::Align::Center);
    errorpage.set_halign(gtk::Align::Center);
    let error_title = gtk::Label::new(Some("That page did not open"));
    crate::ui::class(&error_title, "blanktitle");
    let error_reason = gtk::Label::new(None);
    crate::ui::class(&error_reason, "blankhint");
    let error_address = gtk::Label::new(None);
    crate::ui::class(&error_address, "erraddress");
    error_address.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    error_address.set_max_width_chars(52);
    let error_retry = gtk::Button::with_label("Try again");
    crate::ui::class(&error_retry, "act");
    error_retry.set_halign(gtk::Align::Center);
    error_retry.set_margin_top(10);
    pack(&errorpage, &error_title, false);
    pack(&errorpage, &error_reason, false);
    pack(&errorpage, &error_address, false);
    pack(&errorpage, &error_retry, false);
    stack.add_named(&errorpage, "error");
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
        menu,
        stack,
        blank_holder,
        errorpage,
        error_title,
        error_reason,
        error_address,
        error_retry,
    };
      ui
}

/// Three window controls, drawn as marks on a wash instead of borrowed from
/// another operating system: a desktop that is not a Mac should not wear its
/// buttons. They sit on the left because the column is there, and they read
/// the way the rest of this desktop redraws them.
fn window_controls(palette: Palette) -> (gtk::EventBox, gtk::EventBox, gtk::EventBox) {
    let make = |kind: Icon, name: &str, tooltip: &str| {
        let e = gtk::EventBox::new();
        crate::ui::class(&e, "control");
        e.style_context().add_class(name);
        e.set_size_request(22, 22);
        e.set_valign(gtk::Align::Center);
        e.set_tooltip_text(Some(tooltip));
        let mark = icons::area(kind, 12, palette.muted);
        mark.set_halign(gtk::Align::Center);
        mark.set_valign(gtk::Align::Center);
        e.add(&mark);
        e
    };
    (
        make(Icon::Close, "shut", "Close"),
        make(Icon::Minimise, "smaller", "Minimise"),
        make(Icon::Maximise, "bigger", "Maximise"),
    )
}

/// One tab's line: a small mark, the title, and a cross when you are on it.
pub struct TabRow {
    pub event: gtk::EventBox,
    pub title: gtk::Label,
    /// The letter, when there is one. A page that offers an icon has no letter
    /// on screen, so this is `None` rather than a label nobody can see.
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

pub fn mark(holder: &gtk::Box, letter: char, surface: Option<&cairo::Surface>, size: i32) -> Option<gtk::Label> {
    clear(holder);
    let label = match surface {
        Some(surface) => {
            let image = favicon(surface, size);
            image.set_valign(gtk::Align::Center);
            crate::ui::class(&image, "favicon");
            pack(holder, &image, false);
            None
        }
        None => {
            let tile = gtk::EventBox::new();
            crate::ui::class(&tile, "tile");
            tile.set_size_request(size + 2, size + 2);
            tile.set_valign(gtk::Align::Center);
            let label = gtk::Label::new(Some(&letter.to_uppercase().to_string()));
            crate::ui::class(&label, "tileletter");
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            label.set_width_chars(1);
            label.set_max_width_chars(1);
            tile.add(&label);
            pack(holder, &tile, false);
            Some(label)
        }
    };
    holder.show_all();
    label
}

pub fn tab_row(title: &str, palette: Palette, row_height: i32, letter: Option<char>, live: bool) -> TabRow {
    let event = gtk::EventBox::new();
    crate::ui::class(&event, "row");
    if live {
        crate::ui::class(&event, "live");
    }
    event.set_size_request(-1, row_height);

    let hbox = widget_box(gtk::Orientation::Horizontal, 6);
    hbox.set_margin_start(8);
    hbox.set_margin_end(6);

    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_size_request(18, 18);
    icon.set_valign(gtk::Align::Center);
    let glyph = if letter.is_some() { Icon::Globe } else { Icon::Plus };
    let letter_label = mark_glyph(&icon, glyph, None, 14, palette.faint);
    pack(&hbox, &icon, false);

    let label = gtk::Label::new(Some(title));
    crate::ui::class(&label, "rowtitle");
    if live {
        // GTK's CSS wants keywords, so the weight that sits between normal and
        // bold is set as a Pango attribute instead.
        let attrs = gtk::pango::AttrList::new();
        attrs.insert(gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Semibold));
        label.set_attributes(Some(&attrs));
    }
    label.set_xalign(0.0);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_hexpand(true);
    label.set_valign(gtk::Align::Center);
    label.set_max_width_chars(1);
    pack(&hbox, &label, true);

    let close = gtk::Button::new();
    close.set_relief(gtk::ReliefStyle::None);
    close.add(&icons::area(Icon::Close, 12, palette.muted.mix(palette.faint, 0.4)));
    crate::ui::class(&close, "loupe");
    crate::ui::class(&close, "close");
    close.set_valign(gtk::Align::Center);
    close.set_size_request(18, 18);
    close.set_can_focus(false);
    pack(&hbox, &close, false);

    event.add(&hbox);
    TabRow { event, title: label, letter: letter_label, icon, close }
}

/// A pinned page: one square, one letter, or its site's mark.
pub struct PinWidget {
    pub event: gtk::EventBox,
    pub letter: gtk::Label,
    pub icon: gtk::Box,
}

pub fn pin_tile(letter: char, live: bool, palette: Palette) -> PinWidget {
    let event = gtk::EventBox::new();
    crate::ui::class(&event, "pin");
    if live {
        crate::ui::class(&event, "live");
    }
    event.set_size_request(35, 35);
    let label = gtk::Label::new(Some(&letter.to_uppercase().to_string()));
    crate::ui::class(&label, "pinletter");
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    // The mark sits over the letter: a page with an icon shows the icon, a
    // page without one shows the globe drawn here rather than a capital that
    // could belong to any of six sites.
    let stack = gtk::Overlay::new();
    label.hide();
    stack.add(&label);
    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    mark_glyph(&icon, Icon::Globe, None, 18, palette.muted.mix(palette.faint, 0.5));
    stack.add_overlay(&icon);
    event.add(&stack);
    PinWidget { event, letter: label, icon }
}

/// The dimming behind a field or a panel. A drawing area rather than an event
/// box: it needs no window of its own, so it stacks where it is put.
pub fn scrim(palette: &Palette) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    crate::ui::class(&area, "scrim");
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


