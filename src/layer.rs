// The layers that come over the page — the field, the panels, find — live in a
// fixed layer of their own: one place that knows where each of them stands for
// a given window size, rather than three overlays guessing at their own.
use gtk::prelude::*;

pub struct Layer {
    pub root: gtk::Fixed,
}

impl Layer {
    pub fn new() -> Layer {
        let root = gtk::Fixed::new();
        root.set_halign(gtk::Align::Fill);
        root.set_valign(gtk::Align::Fill);
        root.set_hexpand(true);
        root.set_vexpand(true);
        root.set_no_show_all(true);
        Layer { root }
    }

    pub fn add(&self, widget: &impl IsA<gtk::Widget>) {
        self.root.put(widget, 0, 0);
    }

    pub fn place(&self, widget: &impl IsA<gtk::Widget>, x: i32, y: i32, width: i32, height: i32) {
        widget.set_size_request(width, height);
        self.root.move_(widget, x, y);
    }

    pub fn show(&self) {
        self.root.set_no_show_all(false);
        self.root.show_all();
    }

    pub fn hide(&self) {
        self.root.hide();
        self.root.set_no_show_all(true);
    }
}
