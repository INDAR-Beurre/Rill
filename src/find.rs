// Find, as a small bar that comes over the top of the page rather than a
// strip that moves it.
use crate::layer::Layer;
use crate::palette::Palette;
use gtk::prelude::*;
use webkit2gtk::{FindControllerExt, WebView, WebViewExt};

pub const BAR_WIDTH: i32 = 320;

pub struct FindBar {
    pub holder: gtk::Fixed,
    layer: Layer,
    pub bar: gtk::Box,
    pub entry: gtk::Entry,
    pub count: gtk::Label,
    pub visible: bool,
}

impl FindBar {
    pub fn new(palette: &Palette) -> FindBar {
        let layer = Layer::new();
        let holder = layer.root.clone();

        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        crate::ui::class(&bar, "findbar");
        bar.set_size_request(BAR_WIDTH, 34);

        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("Find"));
        entry.set_hexpand(true);
        entry.set_margin_start(8);
        entry.set_has_frame(false);
        entry.set_valign(gtk::Align::Center);
        crate::ui::pack(&bar, &entry, true);

        let count = gtk::Label::new(Some(""));
        crate::ui::class(&count, "empty");
        count.set_valign(gtk::Align::Center);
        crate::ui::pack(&bar, &count, false);

        let up = crate::icons::button(crate::icons::Icon::Back, 12, palette.muted, "door");
        up.set_valign(gtk::Align::Center);
        crate::ui::pack(&bar, &up, false);
        let down = crate::icons::button(crate::icons::Icon::Forward, 12, palette.muted, "door");
        down.set_valign(gtk::Align::Center);
        crate::ui::pack(&bar, &down, false);
        let close = crate::icons::button(crate::icons::Icon::Close, 12, palette.muted, "door");
        close.set_valign(gtk::Align::Center);
        crate::ui::pack(&bar, &close, false);

        layer.add(&bar);

        {
            let entry = entry.clone();
            entry.connect_changed(move |entry| {
                if let Some(app) = crate::app::current() {
                    app.find_query(&entry.text(), true);
                }
            });
        }
        {
            let entry_for_up = entry.clone();
            up.connect_clicked(move |_| {
                if let Some(app) = crate::app::current() {
                    app.find_step(&entry_for_up.text(), false);
                }
            });
        }
        {
            let entry_for_down = entry.clone();
            down.connect_clicked(move |_| {
                if let Some(app) = crate::app::current() {
                    app.find_step(&entry_for_down.text(), true);
                }
            });
        }
        {
            close.connect_clicked(move |_| {
                if let Some(app) = crate::app::current() {
                    app.toggle_find();
                }
            });
        }

        FindBar { holder, layer, bar, entry, count, visible: false }
    }

    /// The bar sits at the page's top right, over the page rather than in it.
    pub fn layout(&self, width: i32) {
        let x = (width - BAR_WIDTH - 24).max(12);
        self.layer.place(&self.bar, x, 14, BAR_WIDTH, 34);
    }

    pub fn show(&mut self, ui: &crate::ui::Ui) {
        self.visible = true;
        self.layer.show();
        self.layout(ui.window.allocated_width());
    }

    pub fn hide(&mut self, _ui: &crate::ui::Ui) {
        self.visible = false;
        self.layer.hide();
        if let Some(app) = crate::app::current() {
            if let Some(view) = app.view(app.active_id()) {
                if let Some(controller) = view.find_controller() {
                    controller.search("", 0, 0);
                }
            }
        }
    }

    pub fn focus(&self) {
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    pub fn attach(&self, view: &WebView) {
        let count = self.count.clone();
        let Some(controller) = view.find_controller() else { return };
        // The count comes from counted-matches, not from found-text: the latter
        // reports the text it found, so its number is meaningless as a total.
        controller.connect_counted_matches(move |_, matches| {
            // The engine reports -1 (as u32) until it has counted: printing
            // 4294967295 matches is worse than printing nothing.
            count.set_text(&if matches > 1_000_000 {
                String::new()
            } else {
                match matches {
                    0 => "no matches".to_string(),
                    1 => "1 match".to_string(),
                    n => format!("{n} matches"),
                }
            });
        });
    }

}
