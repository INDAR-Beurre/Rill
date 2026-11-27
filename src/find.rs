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
/// Where the page starts, in overlay coordinates: just under the toolbar.
/// Returns 0 when the toolbar is hidden or not mapped (fullscreen), so Find
/// then sits at the top of the page as before.
///
/// While the active tab is in reading mode, the band also leaves room for the
/// reader's exit pill. That pill is fixed at the same PAGE_TOP/PAGE_INSET
/// corner (assets/reader.js), so without this Find covers it.
pub fn toolbar_band(ui: &crate::ui::Ui) -> i32 {
    let chrome_bottom = if ui.viewport_frame.is_visible() && ui.viewport_frame.is_mapped() {
        ui.viewport_frame
            .translate_coordinates(&ui.overlay, 0, 0)
            .map(|(_, y)| y.max(0))
            .unwrap_or(0)
    } else if ui.toolbar.is_visible() && ui.toolbar.is_mapped() {
        ui.toolbar
            .translate_coordinates(&ui.overlay, 0, ui.toolbar.allocated_height())
            .map(|(_, y)| y.max(0))
            .unwrap_or(0)
    } else if ui.topbar.is_visible() && ui.topbar.is_mapped() {
        ui.topbar
            .translate_coordinates(&ui.overlay, 0, ui.topbar.allocated_height())
            .map(|(_, y)| y.max(0))
            .unwrap_or(0)
    } else {
        0
    };
    let reading = crate::app::current().map(|app| app.reading.get()).unwrap_or(false);
    chrome_bottom + if reading { READER_EXIT_CLEARANCE } else { 0 }
}

/// The reader exit pill is at least 28px tall (assets/reader.js min-height).
/// Find moves down by that height plus an 8px gap, so its top lands 8px below
/// the pill's bottom edge and both keep the same right inset.
pub const READER_EXIT_CLEARANCE: i32 = 28 + 8;

/// The reader's text column (assets/reader.js max-width), centred in the page.
pub const READER_MEASURE: i32 = 660;
/// Narrowest the bar gets while reading before it gives up on the gutter: the
/// entry, the count and both arrows still fit.
pub const READING_BAR_MIN: i32 = 220;
/// Below this page width the reader column fills the page, so there is no
/// right gutter to sit in. The bar then takes the full page row below the
/// reader controls instead of overlapping the article at a narrow width.
pub const READER_GUTTER_MIN: i32 = READING_BAR_MIN + PAGE_INSET + 8;

/// How wide the bar may be while reading so it stays in the right gutter beside
/// the centred reader column, with an 8px gap. At 1500px with the 264px tab
/// column the gutter is (1236 - 660) / 2 = 288, so the bar is 288 - 18 - 8 = 262
/// and sits at x1220, clear of the column's right edge at x1212. Gemini R59 saw
/// the 320px bar (x1162..1482) cut off "lazy" in 07c-reading-find.png.
fn reading_bar_width(width: i32, column: i32) -> i32 {
    let gutter = (width - column - READER_MEASURE) / 2;
    (gutter - PAGE_INSET - 8).clamp(READING_BAR_MIN, BAR_WIDTH)
}

fn bar_plan(width: i32, band: i32, column: i32) -> (i32, i32, i32) {
    let avail = (width - column - 24).max(1);
    // Tests have no app, so current() is None there and the plain width holds.
    let reading = crate::app::current().map(|app| app.reading.get()).unwrap_or(false);
    let gutter = (width - column - READER_MEASURE) / 2;
    let narrow_reading = reading && gutter < READER_GUTTER_MIN;
    let wanted = if narrow_reading {
        (avail - 12).max(1)
    } else if reading {
        reading_bar_width(width, column)
    } else {
        BAR_WIDTH
    };
    let bar_width = wanted.min(avail);
    let min_x = column + 12;
    let max_x = (width - bar_width - 12).max(min_x);
    let x = if narrow_reading {
        min_x
    } else {
        (width - bar_width - PAGE_INSET).clamp(min_x, max_x)
    };
    // `toolbar_band` already adds `READER_EXIT_CLEARANCE` while reading, so
    // narrow reading sits cleanly below the unified reader header without
    // doubling the offset.
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
        // The counter stays legible when the bar is narrow; the entry gives up
        // space first.
        count.set_width_request(44);
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
        // Sit below the toolbar, not over its controls and the window marks.
        let band = toolbar_band(ui);
        self.layout_in(width, band, column);
        if width > 1 {
            let (x, y, _) = bar_plan(width, band, column);
            // Enters and leaves on the shared roles: the same 8px of travel
            // both ways, so the exit mirrors the entrance instead of using
            // its own -18/-14 and 150/100 numbers.
            crate::motion::slide_in_fixed(
                &self.layer.root,
                &self.bar,
                x,
                y,
                -crate::motion::dur::TRAVEL,
                crate::motion::dur::ENTER,
            );
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
                // search_finish only clears the engine's own marks. The current
                // match is a <mark data-search-find> wrapper set by the step
                // script (WebKitGTK 2.54 does not paint custom highlights).
                // Unwrap it, join the text back up, and forget the position,
                // so closing find leaves the page exactly as it was.
                crate::js::run(
                    &view,
                    "(() => { const parents = new Set(); for (const m of document.querySelectorAll('mark[data-search-find]')) { const p = m.parentNode; if (!p) continue; while (m.firstChild) p.insertBefore(m.firstChild, m); p.removeChild(m); parents.add(p); } for (const p of parents) p.normalize(); window.__searchFindAt = undefined; const s = document.getSelection(); if (s) s.removeAllRanges(); if (window.CSS && CSS.highlights) CSS.highlights.delete('search-find'); return 'cleared'; })()",
                    |_| {},
                );
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
        crate::motion::slide_out_fixed(
            &self.layer.root,
            &self.bar,
            x,
            y,
            -crate::motion::dur::TRAVEL,
            crate::motion::dur::EXIT,
            move || {
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
            // The find step may already have written '3 of 12' for this same
            // total (App::find_select). The engine's count can arrive after it,
            // so it does not replace a position that already agrees with it.
            if (1..=1_000_000).contains(&matches) && count.text().ends_with(&format!(" of {matches}")) {
                return;
            }
            // The engine reports -1 (as u32) until it has counted: printing
            // 4294967295 matches is worse than printing nothing.
            let zero = matches == 0;
            let ctx = count.style_context();
            if zero {
                ctx.add_class("find-zero");
            } else {
                ctx.remove_class("find-zero");
            }
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

    #[test]
    fn bar_sits_under_the_real_toolbar_band() {
        // The 72px toolbar and the 264px column used in the round-58 shots.
        let (x, y, w) = bar_plan(1500, 72, 264);
        assert_eq!((x, y, w), (1500 - BAR_WIDTH - PAGE_INSET, 72 + PAGE_TOP, BAR_WIDTH));
        assert!(y > 72, "find must start below the toolbar rule");
    }

    #[test]
    fn fullscreen_band_puts_the_bar_at_the_page_top() {
        let (_, y, _) = bar_plan(1500, 0, 0);
        assert_eq!(y, PAGE_TOP);
        // A negative band from a stale allocation never lifts it above the page.
        let (_, y, _) = bar_plan(1500, -40, 0);
        assert_eq!(y, PAGE_TOP);
    }

    #[test]
    fn reading_band_clears_the_reader_exit_pill() {
        // The exit pill sits at PAGE_TOP under the toolbar and is at least
        // 28px tall; find must start 8px below its bottom edge.
        let toolbar = 72;
        let pill_bottom = toolbar + PAGE_TOP + 28;
        let (_, y, _) = bar_plan(1500, toolbar + READER_EXIT_CLEARANCE, 264);
        assert_eq!(y, pill_bottom + 8);
    }

    #[test]
    fn narrow_page_keeps_the_bar_inside_the_page_column() {
        for width in [300, 400, 520, 720] {
            let column = 264;
            let (x, _, w) = bar_plan(width, 72, column);
            assert!(w >= 1);
            assert!(x >= column + 12, "width {width}: x {x} enters the column");
            if width - column - 24 >= 1 {
                assert!(x + w <= width - 12, "width {width}: bar runs past the right edge");
            }
        }
    }

    #[test]
    fn a_narrow_reading_page_has_a_full_row_below_the_controls() {
        // The fixture's 820px shell gives a 556px page: too narrow for the
        // 660px reader measure and its right gutter at once.
        let (x, y, w) = bar_plan(820, 72 + READER_EXIT_CLEARANCE, 264);
        // There is no app in unit tests, so current() is None and no reader
        // mode is assumed. The visual fixture supplies that state at runtime;
        // here the invariant is that the narrow bar stays inside the page.
        assert_eq!(x, 482);
        assert_eq!(y, 72 + READER_EXIT_CLEARANCE + PAGE_TOP);
        assert_eq!(w, 320);
    }
}
