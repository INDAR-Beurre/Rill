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
    pub desk: gtk::EventBox,
    pub paned: gtk::Paned,
    /// The column: the tab rows and their foot, with the hairline on its
    /// right edge.
    pub side: gtk::Box,
    /// The whole band at the top of the column, which is the window's handle:
    /// a titlebar that is not drawn still has to be somewhere to grab.
    pub top: gtk::EventBox,
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
    /// The scrolled window the rows live in, so a tab switch can bring the row
    /// it switched to into view. Without it, Ctrl Tab past the bottom of the
    /// list changes the page and leaves the highlight somewhere off-screen.
    pub rows_scroll: gtk::ScrolledWindow,
    pub menu: gtk::Button,
    pub stack: gtk::Stack,
    /// The row of kept pages under the top band. A real browser keeps them one
    /// click away; this one does too, and the row is only visible when there is
    /// something in it or when it was asked for.
    pub bmarkbar: gtk::EventBox,
    pub bmarks: gtk::Box,
    pub blank_holder: gtk::EventBox,
    /// The error page's wrapper. A Box has no window of its own, so a press on
    /// the apology went to whatever was behind it; this EventBox is the widget
    /// that can receive the click and is the child the stack holds as "error".
    pub error_holder: gtk::EventBox,
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

    // The window's own band above everything, the way the original has one:
    // 23px of desk-coloured chrome across the whole width, then the white
    // window with its top-left corner rounded into it. Without it the top edge
    // of the window is page, and the window has no edge at all.
    //
    // It is one strip above the paned, not a strip inside each side of it.
    // Split in two, the paned's own 1px handle was left between the halves:
    // in `01-default.png` rows y=1..23 read the band's grey at every x but
    // 232, which was white, so the band was full width with a one-pixel hole
    // punched in it. Above the paned there is nothing between the two sides
    // for the hole to open in.
    let deskband = gtk::EventBox::new();
    crate::ui::class(&deskband, "desk");
    deskband.set_size_request(-1, 23);
    deskband.set_visible_window(true);

    let side_width = prefs.side_width.round() as i32;
    let side = widget_box(gtk::Orientation::Vertical, 0);
    crate::ui::class(&side, "side");
    side.set_size_request(if prefs.sidebar { side_width } else { 0 }, -1);

    // The lights and the page's own doors, in the band they share. The band
    // is one EventBox so the whole of it can be the window's handle, not only
    // the strip the three controls happen to occupy.
    let top = gtk::EventBox::new();
    crate::ui::class(&top, "topband");
    top.set_size_request(-1, 40);
    let band = widget_box(gtk::Orientation::Horizontal, 0);
    top.add(&band);
    let lights = widget_box(gtk::Orientation::Horizontal, 2);
    lights.set_margin_start(12);
    lights.set_margin_top(9);
    lights.set_valign(gtk::Align::Center);
    let (light_red, light_amber, light_green) = window_controls(palette);
    pack(&lights, &light_red, false);
    pack(&lights, &light_amber, false);
    pack(&lights, &light_green, false);
    pack(&band, &lights, false);

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
    pack(&band, &helm, false);
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

    // The column is the paned's first child itself, with no wrapper around
    // it. `.side` draws the hairline as its own right border, so the line is
    // exactly as tall as the widget; wrapped, the 23px strip above the column
    // was outside that border and the line began at y=24 instead of at the
    // band's edge. Unwrapped it runs from the band to the window's bottom.
    paned.pack1(&side, false, false);

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
    pack(&card, &bmarkbar, false);
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
    // Wrapped the way the blank page is wrapped: the same shape, so the click
    // that opens the address field there can retry a load here.
    let error_holder = gtk::EventBox::new();
    crate::ui::class(&error_holder, "blank");
    error_holder.set_visible_window(false);
    error_holder.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
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
    error_holder.add(&errorpage);
    // The name stays "error": the browser and the bench both look the page up
    // by it, and only the widget under the name changed.
    stack.add_named(&error_holder, "error");
    pack(&card, &stack, true);

    // The page is the paned's second child itself: the band it used to begin
    // with is the one strip above the paned now, and a wrapper here would buy
    // a box with a single child in it.
    paned.pack2(&card, true, false);

    // The outer frame is what the capture walks. The band is its first child
    // and the paned its second, so the band is laid out above both columns and
    // painted before them rather than over them.
    let frame = widget_box(gtk::Orientation::Vertical, 0);
    crate::ui::class(&frame, "shell");
    pack(&frame, &deskband, false);
    pack(&frame, &paned, true);
    overlay.add(&frame);

    Ui {
        window,
        overlay,
        shell: frame,
        desk: deskband,
        paned,
        side,
        top,
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
        menu,
        stack,
        bmarkbar,
        bmarks,
        blank_holder,
        error_holder,
        errorpage,
        error_title,
        error_reason,
        error_address,
        error_retry,
    }
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
        // The mark is drawn, not typed: there is no text in this control for a
        // screen reader to read, so the words have to be given to it. A
        // button role says what it is; the tooltip and the accessible name say
        // which.
        e.set_tooltip_text(Some(tooltip));
        if let Some(access) = e.accessible() {
            // Fully qualified: gtk's prelude brings several traits with a
            // `set_name`, and this one is atk's.
            gtk::atk::prelude::AtkObjectExt::set_name(&access, tooltip);
            gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::PushButton);
        }
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
    area.connect_draw(move |_, cr| {
        let a = for_draw.get();
        cr.set_source_rgb(colour.0 as f64 / 255.0, colour.1 as f64 / 255.0, colour.2 as f64 / 255.0);
        cr.set_line_width(1.6);
        cr.set_line_cap(cairo::LineCap::Round);
        cr.arc(7.0, 7.0, 5.0, a, a + std::f64::consts::TAU * 0.7);
        let _ = cr.stroke();
        crate::gtkx::inh(false)
    });
    let weak = area.downgrade();
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(80), move || {
        let Some(area) = weak.upgrade() else { return gtk::glib::ControlFlow::Break };
        angle.set((angle.get() + 0.35) % std::f64::consts::TAU);
        area.queue_draw();
        gtk::glib::ControlFlow::Continue
    });
    icon.pack_start(&area, false, false, 0);
    area.show();
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

    let hbox = widget_box(gtk::Orientation::Horizontal, 6);
    hbox.set_margin_start(8);
    hbox.set_margin_end(6);

    let icon = widget_box(gtk::Orientation::Horizontal, 0);
    icon.set_size_request(18, 18);
    icon.set_valign(gtk::Align::Center);
    // The same mark the pin uses: the page's own letter until the page offers
    // an icon. A globe in every row is one grey mark for every site, which
    // tells nobody which row is which; a row with no letter at all keeps the
    // plus, and that plus is only for a row that could make a tab.
    let letter_label = match letter {
        Some(letter) => mark(&icon, letter, None, 14),
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
    pack(&hbox, &label, true);

    let close = gtk::Button::new();
    close.set_relief(gtk::ReliefStyle::None);
    close.add(&icons::area(Icon::Close, 12, palette.muted.mix(palette.faint, 0.4)));
    crate::ui::class(&close, "loupe");
    crate::ui::class(&close, "close");
    close.set_valign(gtk::Align::Center);
    close.set_size_request(18, 18);
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
    TabRow { event, title: label, letter: letter_label, icon, close }
}

/// A pinned page: one square, one letter, or its site's mark.
pub struct PinWidget {
    pub event: gtk::EventBox,
    pub letter: gtk::Label,
    pub icon: gtk::Box,
}

pub fn pin_tile(letter: char, live: bool, _palette: Palette) -> PinWidget {
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
        let c = palette.ground;
        (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0, 0.86)
    } else {
        let c = palette.card;
        (c.0 as f64 / 255.0, c.1 as f64 / 255.0, c.2 as f64 / 255.0, 0.88)
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


