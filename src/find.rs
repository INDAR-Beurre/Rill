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
    last_pos: std::cell::Cell<(i32, i32)>,
    attached: std::cell::RefCell<Option<(glib::WeakRef<webkit2gtk::FindController>, glib::SignalHandlerId)>>,
}

/// Position and width of the find bar inside the page viewport (`[column + 12, width - 12]`),
/// sitting `PAGE_TOP` below `band`.
fn bar_plan(width: i32, band: i32, column: i32) -> (i32, i32, i32) {
    let avail = (width - column - 24).max(1);
    let bar_width = BAR_WIDTH.min(avail);
    let min_x = column + 12;
    let max_x = (width - bar_width - 12).max(min_x);
    let x = (width - bar_width - PAGE_INSET).clamp(min_x, max_x);
    let y = band.max(0) + PAGE_TOP;
    (x, y, bar_width)
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
        entry.set_width_chars(1);
        entry.set_hexpand(true);
        entry.set_margin_start(8);
        entry.set_has_frame(false);
        entry.set_valign(gtk::Align::Center);
        // A placeholder is drawn hint text, not an accessible name, so the
        // field is unnamed to a screen reader without this.
        if let Some(access) = entry.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, "Find in page");
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
        label(&close, "Close find bar", "Close find (Escape)");
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

        FindBar {
            holder,
            layer,
            bar,
            entry,
            count,
            visible: false,
            last_pos: std::cell::Cell::new((0, 0)),
            attached: std::cell::RefCell::new(None),
        }
    }

    /// Place the bar inside the page viewport below `band` and right of `column`.
    pub fn layout_in(&self, width: i32, band: i32, column: i32) {
        let (x, y, bar_width) = bar_plan(width, band, column);
        self.last_pos.set((x, y));
        self.bar.set_size_request(bar_width, 34);
        self.layer.place(&self.bar, x, y, bar_width, 34);
    }

    pub fn show(&mut self, ui: &crate::ui::Ui) {
        crate::motion::cancel(&self.bar);
        crate::motion::cancel(&self.layer.root);
        self.bar.set_opacity(1.0);
        self.layer.root.set_opacity(1.0);
        self.visible = true;
        self.layer.show();
        let column = if ui.side.is_visible() && ui.side.is_child_visible() {
            ui.side.allocated_width()
        } else {
            0
        };
        let width = ui.overlay.allocated_width().max(ui.window.allocated_width());
        self.layout_in(width, 0, column);
        if width > 1 {
            let (x, y, _) = bar_plan(width, 0, column);
            crate::motion::slide_in_fixed(&self.layer.root, &self.bar, x, y, -18, 150);
        }
    }

    pub fn hide(&mut self, _ui: &crate::ui::Ui) {
        let was_visible = self.visible;
        self.visible = false;
        self.detach();
        if let Some(app) = crate::app::current() {
            if let Some(view) = app.view(app.active_id()) {
                if let Some(controller) = view.find_controller() {
                    controller.search_finish();
                }
            }
        }
        if !was_visible {
            return;
        }
        crate::motion::cancel(&self.bar);
        crate::motion::cancel(&self.layer.root);
        let (x, y) = self.last_pos.get();
        let root = self.layer.root.clone();
        let bar = self.bar.clone();
        crate::motion::slide_out_fixed(&self.layer.root, &self.bar, x, y, -14, 100, move || {
            let still_hidden = !crate::app::current()
                .map(|app| app.find.try_borrow().map(|f| f.visible).unwrap_or(false))
                .unwrap_or(false);
            if still_hidden {
                root.hide();
                root.set_no_show_all(true);
                root.set_opacity(1.0);
                bar.set_opacity(1.0);
            }
        });
    }

    pub fn focus(&self) {
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    /// Disconnect the previously attached FindController handler and clear its highlights.
    pub fn detach(&self) {
        if let Some((weak, handler_id)) = self.attached.borrow_mut().take() {
            if let Some(controller) = weak.upgrade() {
                controller.search_finish();
                controller.disconnect(handler_id);
            }
        }
    }

    pub fn attach(&self, view: &WebView) {
        self.detach();
        let count = self.count.clone();
        let Some(controller) = view.find_controller() else { return };
        // The count comes from counted-matches, not from found-text: the latter
        // reports the text it found, so its number is meaningless as a total.
        let handler_id = controller.connect_counted_matches(move |_, matches| {
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
        *self.attached.borrow_mut() = Some((controller.downgrade(), handler_id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bar_sits_below_desk_band_and_stays_inside_page() {
        assert_eq!(bar_plan(1500, 23, 232), (1500 - BAR_WIDTH - PAGE_INSET, 23 + PAGE_TOP, BAR_WIDTH));
        // Narrow window clamps bar width and keeps x >= column + 12.
        let (x, y, w) = bar_plan(420, 23, 232);
        assert_eq!(y, 37);
        assert_eq!(x, 244);
        assert_eq!(w, 420 - 232 - 24);
        assert!(x + w <= 420 - 12);
    }
}
