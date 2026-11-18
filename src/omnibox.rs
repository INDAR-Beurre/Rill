// The one field: an address, a few words, or the name of a tab. It rises over
// the page when asked for and never sends what was typed until Return.
use crate::app::{App, AppRef};
use crate::layer::Layer;
use crate::palette::Palette;
use gtk::prelude::*;

pub const FIELD_WIDTH: i32 = 560;
const FIELD_HEIGHT: i32 = 50;

#[derive(Clone)]
pub struct Offer {
    pub title: String,
    pub url: String,
    pub kind: String,
}

pub struct Omnibox {
    pub offers: std::cell::RefCell<Vec<Offer>>,
    column: std::cell::Cell<i32>,
    pub picked: std::cell::Cell<i32>,
    pub holder: gtk::Fixed,
    layer: Layer,
    pub scrim: gtk::DrawingArea,
    pub block: gtk::Box,
    pub field: gtk::Box,
    pub entry: gtk::Entry,
    pub site: gtk::Label,
    pub list: gtk::Box,
    pub list_scroll: gtk::ScrolledWindow,
    pub visible: std::cell::Cell<bool>,
}

impl Omnibox {
    pub fn new(palette: &Palette) -> Omnibox {
        let layer = Layer::new();

        let scrim = crate::ui::scrim(palette);
        layer.add(&scrim);

        // The list scrolls inside a cap instead of being cropped by one: eight
        // kinds of suggestion must not run off the bottom of the screen.
        let list_scroll = gtk::ScrolledWindow::new(None::<&gtk::Adjustment>, None::<&gtk::Adjustment>);
        list_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        crate::ui::class(&list_scroll, "suggestscroll");
        list_scroll.set_no_show_all(true);

        let block = gtk::Box::new(gtk::Orientation::Vertical, 8);
        block.set_halign(gtk::Align::Start);
        block.set_valign(gtk::Align::Start);

        let field = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        crate::ui::class(&field, "omni");
        field.set_size_request(FIELD_WIDTH, FIELD_HEIGHT);

        let site = gtk::Label::new(None);
        crate::ui::class(&site, "omnisite");
        site.set_margin_start(18);
        site.set_ellipsize(gtk::pango::EllipsizeMode::End);
        site.set_max_width_chars(14);
        site.set_no_show_all(true);
        crate::ui::pack(&field, &site, false);

        let entry = gtk::Entry::new();
        crate::ui::class(&entry, "omnitext");
        entry.set_placeholder_text(Some("Search or enter address"));
        entry.set_hexpand(true);
        entry.set_margin_end(18);
        entry.set_valign(gtk::Align::Center);
        entry.set_has_frame(false);
        crate::ui::pack(&field, &entry, true);
        block.add(&field);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&list, "suggest");
        list.set_size_request(FIELD_WIDTH, -1);
        list.set_halign(gtk::Align::Start);
        list.set_no_show_all(true);
        list_scroll.add(&list);
        block.add(&list_scroll);

        layer.add(&block);
        let holder = layer.root.clone();

        Omnibox {
            offers: std::cell::RefCell::new(Vec::new()),
            column: std::cell::Cell::new(0),
            picked: std::cell::Cell::new(-1),
            holder,
            layer,
            scrim,
            block,
            field,
            entry,
            site,
            list,
            list_scroll,
            visible: std::cell::Cell::new(false),
        }
    }

    /// Where the field stands for a window of this size, and how much room the
    /// list under it has.
    pub fn layout(&self, width: i32, height: i32) {
        self.layout_with_column(width, height, self.column.get());
    }

    /// The scrim covers the page, not the column: the tabs stay readable while
    /// the field is up.
    pub fn layout_with_column(&self, width: i32, height: i32, column: i32) {
        self.column.set(column);
        self.layer.place(&self.scrim, column, 0, (width - column).max(0), height);
        // Centred on the page, not on the window: the column is not part of
        // the page the field is over.
        let x = (column + (width - column - FIELD_WIDTH) / 2).max(column + 12);
        let block_height = FIELD_HEIGHT + if self.list_scroll.get_visible() { self.list_height() + 8 } else { 0 };
        let y = ((height - block_height) / 2 - 60).max(72);
        self.layer.place(&self.block, x, y, FIELD_WIDTH, block_height.max(FIELD_HEIGHT));
        if self.list_scroll.get_visible() {
            self.list_scroll.set_size_request(FIELD_WIDTH, self.list_height());
        }
        self.field.set_size_request(FIELD_WIDTH, FIELD_HEIGHT);
    }

    /// The list's own preferred height, not a guess: group headers are shorter
    /// than rows, and the cap has to be a scroll, not a crop.
    fn list_height(&self) -> i32 {
        let wanted = self.list.preferred_height_for_width(FIELD_WIDTH).0;
        wanted.clamp(0, 420)
    }

    pub fn show(&self, ui: &crate::ui::Ui, over: bool, offers: Vec<Offer>) {
        self.visible.set(true);
        // A new field starts with nothing picked: a pick from the last time is
        // a pick the user did not make.
        self.picked.set(-1);
        self.scrim.set_visible(over);
        self.layer.show();
        // The chip only exists when there is something to say about the page:
        // an empty field with a label saying "Search" says it twice.
        let host = crate::app::current()
            .and_then(|app| app.state.borrow().active_tab().map(|tab| crate::shield::host_of(&tab.url)))
            .unwrap_or_default();
        // The chip names where you are, whenever there is a where.
        if host.is_empty() {
            self.site.set_no_show_all(true);
            self.site.hide();
        } else {
            self.site.set_no_show_all(false);
            self.site.set_text(&host);
            self.site.show();
        }
        let current = crate::app::current()
            .and_then(|app| app.state.borrow().active_tab().map(|tab| tab.url.clone()))
            .unwrap_or_default();
        self.entry.set_text(&current);
        if let Some(app) = crate::app::current() {
            self.set_offers(ui, &app, offers);
        }
        self.layout(ui.window.allocated_width(), ui.window.allocated_height());
    }

    pub fn hide(&self, _ui: &crate::ui::Ui) {
        self.visible.set(false);
        self.layer.hide();
    }

    pub fn set_offers(&self, _ui: &crate::ui::Ui, app: &AppRef, offers: Vec<Offer>) {
        *self.offers.borrow_mut() = offers.clone();
        let picked = self.picked.get();
        let tint = app.palette.borrow().faint;
        for child in self.list.children() {
            self.list.remove(&child);
        }
        if offers.is_empty() {
            self.list.hide();
            self.list_scroll.hide();
            return;
        }
        self.list.set_no_show_all(false);
        self.list_scroll.set_no_show_all(false);
        self.list_scroll.show();
        self.list.show_all();
        let mut last_kind = String::new();
        for (index, offer) in offers.into_iter().enumerate() {
            if offer.kind != last_kind {
                last_kind = offer.kind.clone();
                let head = gtk::Label::new(Some(&last_kind));
                crate::ui::class(&head, "sghead");
                head.set_xalign(0.0);
                head.set_margin_start(12);
                head.set_margin_top(6);
                head.set_margin_bottom(2);
                self.list.add(&head);
            }
            let row = gtk::EventBox::new();
            crate::ui::class(&row, "sgrow");
            row.set_can_focus(true);
            row.set_tooltip_text(Some(&format!("{} — {}", offer.title, offer.kind)));
            if index as i32 == picked {
                crate::ui::class(&row, "picked");
            }
            let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            hbox.set_margin_start(12);
            hbox.set_margin_end(12);
            hbox.set_margin_top(7);
            hbox.set_margin_bottom(7);

            // A mark that says where the suggestion came from, rather than the
            // first letter of its title: four history rows for one host used
            // to show four identical "1" tiles.
            let glyph = match offer.kind.as_str() {
                "Open" => crate::icons::Icon::Tab,
                "History" => crate::icons::Icon::Clock,
                "Bookmark" => crate::icons::Icon::Star,
                "Search" => crate::icons::Icon::Search,
                _ => crate::icons::Icon::Globe,
            };
            let mark = crate::icons::area(glyph, 15, tint);
            mark.set_size_request(18, 18);
            mark.set_valign(gtk::Align::Center);
            crate::ui::pack(&hbox, &mark, false);

            let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let title = gtk::Label::new(Some(&offer.title));
            crate::ui::class(&title, "sgtitle");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let url = gtk::Label::new(Some(&offer.url));
            crate::ui::class(&url, "sgurl");
            url.set_xalign(0.0);
            url.set_ellipsize(gtk::pango::EllipsizeMode::End);
            texts.add(&title);
            texts.add(&url);
            crate::ui::pack(&hbox, &texts, true);

            let kind = gtk::Label::new(Some(&offer.kind));
            crate::ui::class(&kind, "sgkind");
            kind.set_valign(gtk::Align::Center);
            crate::ui::pack(&hbox, &kind, false);

            row.add(&hbox);
            let app_weak = app.weak();
            let target = offer.url.clone();
            row.connect_button_press_event(move |_, _| {
                if let Some(app) = app_weak.upgrade() {
                    app.close_omni();
                    app.navigate_active(&target);
                }
                crate::gtkx::inh(true)
            });
            self.list.add(&row);
        }
        self.list.show_all();
    }
}

impl Omnibox {
    /// Up and down move through the list; Return takes the picked row.
    pub fn step(&self, delta: i32) {
        let count = self.offers.borrow().len() as i32;
        if count == 0 {
            self.picked.set(-1);
            return;
        }
        let next = (self.picked.get() + delta).clamp(-1, count - 1);
        self.picked.set(if next <= -1 { -1 } else { next });
        if let Some(app) = crate::app::current() {
            let offers = self.offers.borrow().clone();
            self.set_offers(&app.ui, &app, offers);
        }
    }

    pub fn picked_url(&self) -> Option<String> {
        let picked = self.picked.get();
        if picked < 0 {
            return None;
        }
        self.offers.borrow().get(picked as usize).map(|offer| offer.url.clone())
    }
}

/// Return in the field: the picked row if there is one, else the typed words.
pub fn commit(app: &App) {
    if let Some(url) = app.omni.picked_url() {
        app.close_omni();
        app.navigate_active(&url);
        return;
    }
    let typed = app.omni.entry.text().to_string();
    let target = crate::app::resolve(app, &typed);
    if target.is_empty() {
        app.close_omni();
        return;
    }
    app.close_omni();
    app.navigate_active(&target);
}
