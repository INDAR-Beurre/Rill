// Find, as a small bar that comes over the top of the page rather than a
// strip that moves it.
use crate::layer::Layer;
use crate::palette::Palette;
use gtk::prelude::*;
use webkit2gtk::{FindControllerExt, WebView, WebViewExt};

pub const BAR_WIDTH: i32 = 320;
// The page column has one right edge and one top edge for floating controls.
// The reading-mode exit pill in assets/reader.js sits at the same numbers, so
// a change here has to be made there too or the two insets will drift apart.
pub const PAGE_INSET: i32 = 18;
pub const PAGE_TOP: i32 = 14;

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
        // A placeholder is drawn hint text, not an accessible name, so the
        // field is unnamed to a screen reader without this.
        if let Some(access) = entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Find in the page");
        }
        crate::ui::pack(&bar, &entry, true);

        // `icons::button` turns focus off to keep the marks mouse-only. In this
        // bar they are the whole set of actions, so they take focus back the
        // way `ui::window_controls` labels its marks: tooltip for sight and
        // pointer, accessible name for speech.
        let label = |button: &gtk::Button, name: &str, tooltip: &str| {
            button.set_can_focus(true);
            button.set_tooltip_text(Some(tooltip));
            if let Some(access) = button.accessible() {
                // Fully qualified: several preludes bring a `set_name`.
                gtk::atk::prelude::AtkObjectExt::set_name(&access, name);
            }
        };

        let count = gtk::Label::new(Some(""));
        crate::ui::class(&count, "empty");
        count.set_valign(gtk::Align::Center);
        crate::ui::pack(&bar, &count, false);

        // Down the page, not back through history: the marks say "previous
        // match" and "next match", so they point that way.
        let up = crate::icons::button(crate::icons::Icon::Up, 12, palette.muted, "door");
        up.set_valign(gtk::Align::Center);
        // The arrows are 12px and carry no words, so the tooltip is the only
        // place the direction and the matching key can be stated.
        label(&up, "Previous match", "Previous match (Shift+Return)");
        crate::ui::pack(&bar, &up, false);
        let down = crate::icons::button(crate::icons::Icon::Down, 12, palette.muted, "door");
        down.set_valign(gtk::Align::Center);
        label(&down, "Next match", "Next match (Return)");
        crate::ui::pack(&bar, &down, false);
        let close = crate::icons::button(crate::icons::Icon::Close, 12, palette.muted, "door");
        close.set_valign(gtk::Align::Center);
        label(&close, "Close find", "Close find (Escape)");
        crate::ui::pack(&bar, &close, false);

        layer.add(&bar);

        {
            let entry = entry.clone();
            entry.connect_changed(move |entry| {
                if let Some(app) = crate::app::current() {
                    app.find_query(&entry.text());
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
        let x = (width - BAR_WIDTH - PAGE_INSET).max(12);
        self.layer.place(&self.bar, x, PAGE_TOP, BAR_WIDTH, 34);
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
