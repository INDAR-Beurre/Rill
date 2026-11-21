// The first-run card: what this browser is, which other browsers it can bring
// things from, and the three keys worth knowing. Shown once; Start remembers.
use crate::app::{App, AppRef};
use crate::find::{PAGE_INSET, PAGE_TOP};
use crate::import::{self, Browser};
use gtk::gdk::keys::constants::Escape;
use gtk::prelude::*;

/// The name the card carries on the overlay, so a key or a later start can find it.
const NAME: &str = "welcome-card";

/// Escape puts the card away, the same as Start: every other card on this
/// browser leaves with Escape, and a first-run card that only a mouse can
/// close traps a keyboard user on their first minute. It is remembered, so
/// the card does not come back on the next start.
pub fn key(app: &App, event: &gtk::gdk::EventKey) -> bool {
    if event.keyval() != Escape {
        return false;
    }
    let card = app.ui.overlay.children().into_iter().find(|child| child.widget_name() == NAME);
    let Some(card) = card else { return false };
    finish(app, &card);
    true
}

pub fn dismiss(app: &App) {
    if let Some(card) = app.ui.overlay.children().into_iter().find(|child| child.widget_name() == NAME) {
        finish(app, &card);
    }
}

/// Start and Escape both end here: remember the card was seen, then remove it.
fn finish(app: &App, card: &gtk::Widget) {
    app.prefs.borrow_mut().welcomed = true;
    app.prefs.borrow().save();
    crate::motion::cancel(card);
    app.ui.overlay.remove(card);
    crate::permissions::focus_page(app);
}

fn page_left(app: &App) -> i32 {
    if !app.ui.side.is_visible() || !app.ui.side.is_child_visible() {
        return 0;
    }
    let allocated = app.ui.side.allocated_width();
    if allocated > 1 {
        return allocated;
    }
    let prefs = app.prefs.borrow();
    crate::ui::column_plan(
        prefs.column_folded,
        false,
        prefs.sidebar,
        prefs.side_width.round() as i32,
    )
    .width
    .max(0)
}

fn apply_margins(app: &App, card: &gtk::Widget) {
    card.set_margin_start(page_left(app) + PAGE_INSET);
    card.set_margin_end(PAGE_INSET);
    // Below the window's band, not inside it: the overlay starts at the top
    // of the window, and the band is drawn over anything placed in it.
    card.set_margin_top(PAGE_TOP);
    card.set_margin_bottom(8);
}

pub fn relayout(app: &App) {
    if let Some(card) = app.ui.overlay.children().into_iter().find(|child| child.widget_name() == NAME) {
        apply_margins(app, &card);
    }
}

pub fn show(app: &AppRef) {
    let found = import::home().map(|h| import::installed(&h)).unwrap_or_default();

    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    crate::ui::class(&card, "dlbar");
    // The same inner room as the site card and the permission prompt.
    crate::ui::class(&card, "prompt");
    if let Some(access) = card.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Dialog);
    }
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Start);
    apply_margins(app, card.upcast_ref::<gtk::Widget>());
    card.set_size_request(340, -1);
    card.set_widget_name(NAME);

    let greeting = gtk::Label::new(Some("Welcome to Search. A browser with nothing in the way."));
    crate::ui::class(&greeting, "setting");
    greeting.set_xalign(0.0);
    greeting.set_line_wrap(true);
    greeting.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    greeting.set_max_width_chars(44);
    card.add(&greeting);

    let status = gtk::Label::new(None);
    crate::ui::class(&status, "settingdesc");
    status.set_xalign(0.0);
    status.set_line_wrap(true);
    status.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    status.set_max_width_chars(44);

    if found.is_empty() {
        let none = gtk::Label::new(Some("No other browser found here, so there is nothing to bring over."));
        crate::ui::class(&none, "settingdesc");
        none.set_xalign(0.0);
        none.set_line_wrap(true);
        none.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
        none.set_max_width_chars(44);
        card.add(&none);
    } else {
        for browser in found {
            card.add(&import_row(app, browser, &status));
        }
    }
    card.add(&status);

    let keys = gtk::Label::new(Some("Ctrl L: the address field\nCtrl K: switch to a tab by name\nCtrl /: every key"));
    crate::ui::class(&keys, "settingdesc");
    keys.set_xalign(0.0);
    card.add(&keys);

    let start = gtk::Button::with_label("Start");
    crate::ui::class(&start, "act");
    crate::ui::class(&start, "primary");
    start.set_halign(gtk::Align::End);
    {
        let weak = app.weak();
        let card_weak = card.downgrade();
        start.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            if let Some(card) = card_weak.upgrade() {
                finish(&app, card.upcast_ref::<gtk::Widget>());
            }
        });
    }
    card.add(&start);

    app.ui.overlay.add_overlay(&card);
    card.show_all();
    crate::motion::pop_card_in(&card, PAGE_TOP, -10, 155);
    start.grab_focus();
}

/// One browser's line: its name and an Import button that runs once.
fn import_row(app: &AppRef, browser: Browser, status: &gtk::Label) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let text = import::home().map(|h| import::label(&h, browser)).unwrap_or_else(|| browser.name().to_string());
    let name = gtk::Label::new(Some(&text));
    crate::ui::class(&name, "setting");
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_line_wrap(true);
    name.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    name.set_max_width_chars(30);
    name.set_lines(2);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_tooltip_text(Some(&text));
    row.add(&name);

    let button = gtk::Button::with_label("Import");
    crate::ui::class(&button, "act");
    button.set_valign(gtk::Align::Center);
    {
        let weak = app.weak();
        let status = status.downgrade();
        button.connect_clicked(move |b| {
            b.set_sensitive(false);
            let Some(app) = weak.upgrade() else { return };
            // The databases are read off the main loop, the same as the
            // Settings row, so a large Firefox history does not freeze the
            // first-run card. The line says so while it reads.
            if let Some(status) = status.upgrade() {
                status.set_text("Reading bookmarks and history…");
            }
            let status = status.clone();
            let b_weak = b.downgrade();
            import::bring_later_checked(&app, vec![browser], move |ok, message| {
                if let Some(status) = status.upgrade() {
                    status.set_text(&message);
                }
                if !ok {
                    if let Some(b) = b_weak.upgrade() {
                        b.set_sensitive(true);
                    }
                }
            });
        });
    }
    row.add(&button);
    row
}
