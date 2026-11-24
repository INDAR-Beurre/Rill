// The first-run card: what this browser is, which other browsers it can bring
// things from, and the three keys worth knowing. Shown once; Start remembers.
use crate::app::{App, AppRef};
use crate::find::{PAGE_INSET, PAGE_TOP};
use crate::import::{self, Browser};
use gtk::gdk::keys::constants::Escape;
use gtk::prelude::*;

/// The name the card carries on the overlay, so a key or a later start can find it.
const NAME: &str = "welcome-card";

thread_local! {
    static FITTED_GEOMETRY: std::cell::Cell<(i32, i32)> = const { std::cell::Cell::new((0, 0)) };
}

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

/// Whether the first-run card is on screen. It only asks and never moves focus.
pub fn is_open(app: &App) -> bool {
    app.ui.overlay.children().into_iter().any(|child| child.widget_name() == NAME && child.is_visible())
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
    if app.ui.stack.visible_child_name().as_deref() == Some("blank") {
        app.ui.start.search.grab_focus();
    } else {
        crate::permissions::focus_page(app);
    }
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
    let window_width = app.ui.overlay.allocated_width().max(app.ui.window.allocated_width());
    let window_height = app.ui.overlay.allocated_height().max(app.ui.window.allocated_height());
    let width = if window_width > 1 {
        460.min((window_width - page_left(app) - 2 * PAGE_INSET).max(1))
    } else {
        460
    };
    card.set_size_request(width, -1);
    if let Some(scroll) = card.downcast_ref::<gtk::ScrolledWindow>() {
        let context = card.style_context();
        let padding = context.padding(card.state_flags());
        let border = context.border(card.state_flags());
        let margin = context.margin(card.state_flags());
        let inner = (width - i32::from(padding.left + padding.right + border.left + border.right + margin.left + margin.right)).max(1);
        let maximum = if window_height > 1 {
            (window_height - PAGE_TOP - 16 - i32::from(padding.top + padding.bottom + border.top + border.bottom + margin.top + margin.bottom)).max(1)
        } else {
            560
        };
        if FITTED_GEOMETRY.with(|geometry| geometry.get() == (inner, maximum)) {
            return;
        }
        let child = scroll.child().and_then(|child| {
            if let Some(viewport) = child.downcast_ref::<gtk::Viewport>() {
                viewport.child()
            } else {
                Some(child)
            }
        });
        if let Some(child) = child {
            child.set_size_request(-1, -1);
            let mut needed = child.preferred_height_for_width(inner).1;
            if needed > maximum {
                needed = child.preferred_height_for_width((inner - 18).max(1)).1;
            }
            child.set_size_request(-1, needed);
        }
        scroll.set_max_content_height(maximum);
        FITTED_GEOMETRY.with(|geometry| geometry.set((inner, maximum)));
    }
}

pub fn relayout(app: &App) {
    if let Some(card) = app.ui.overlay.children().into_iter().find(|child| child.widget_name() == NAME) {
        apply_margins(app, &card);
    }
}

pub fn show(app: &AppRef) {
    FITTED_GEOMETRY.with(|geometry| geometry.set((0, 0)));
    let found = import::home().map(|h| import::installed(&h)).unwrap_or_default();

    let card = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    crate::ui::class(&card, "dlbar");
    crate::ui::class(&card, "welcome-card");
    card.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    card.set_propagate_natural_width(false);
    card.set_propagate_natural_height(true);
    card.set_min_content_width(1);
    card.set_min_content_height(1);
    card.set_max_content_height(560);
    card.set_overlay_scrolling(false);
    if let Some(access) = card.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Dialog);
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Welcome to Search");
    }
    card.set_halign(gtk::Align::Center);
    card.set_valign(gtk::Align::Start);
    card.set_widget_name(NAME);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 22);
    crate::ui::class(&content, "welcome-content");
    content.set_margin_start(26);
    content.set_margin_end(26);
    content.set_margin_top(26);
    content.set_margin_bottom(24);
    let header = gtk::Box::new(gtk::Orientation::Vertical, 8);
    crate::ui::class(&header, "welcome-header");
    // Sentence case, like the sidebar, panel and Settings headings. The eyebrow
    // style already sets it apart from the title without capitals.
    header.pack_start(&welcome_label("First run", "welcome-eyebrow"), false, true, 0);
    let greeting = welcome_label("Welcome to Search.", "welcome-title");
    if let Some(access) = greeting.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Heading);
    }
    header.pack_start(&greeting, false, true, 0);
    header.pack_start(&welcome_label("A browser for the web, with your tabs and saved pages close at hand.", "welcome-description"), false, true, 0);
    content.pack_start(&header, false, true, 0);

    let shortcuts = gtk::Box::new(gtk::Orientation::Vertical, 0);
    crate::ui::class(&shortcuts, "welcome-section");
    let shortcuts_title = welcome_label("Three keys to know", "welcome-section-title");
    shortcuts_title.set_margin_bottom(10);
    shortcuts.pack_start(&shortcuts_title, false, true, 0);
    for (key, description) in [
        ("Ctrl L", "Search or enter an address"),
        ("Ctrl K", "Switch to a tab by name"),
        ("Ctrl /", "See every shortcut"),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
        crate::ui::class(&row, "welcome-shortcut-row");
        row.set_margin_top(7);
        row.set_margin_bottom(7);
        let keycap = welcome_label(key, "welcome-shortcut-key");
        keycap.set_size_request(70, -1);
        row.pack_start(&keycap, false, true, 0);
        row.pack_start(&welcome_label(description, "welcome-shortcut-label"), true, true, 0);
        shortcuts.pack_start(&row, false, true, 0);
    }
    content.pack_start(&shortcuts, false, true, 0);

    let status = gtk::Label::new(None);
    crate::ui::class(&status, "welcome-status");
    status.set_xalign(0.0);
    status.set_line_wrap(true);
    status.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    status.set_no_show_all(true);
    if let Some(access) = status.accessible() {
        gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::Statusbar);
        gtk::atk::prelude::AtkObjectExt::set_name(&access, "Import status");
    }

    if found.is_empty() {
        content.pack_start(&welcome_label("No other browser found on this computer.", "welcome-description"), false, true, 0);
    } else {
        let importing = gtk::Expander::new(None);
        crate::ui::class(&importing, "welcome-section");
        crate::ui::class(&importing, "welcome-import");
        // Sentence case, like every other heading in the app ("First run",
        // "Three keys to know" above). This expander was the last all-caps
        // heading left in the UI (Gemini R60 polish).
        importing.set_label_widget(Some(&welcome_label("Import bookmarks and history", "welcome-section-title")));
        importing.set_can_focus(true);
        importing.set_tooltip_text(Some("Add bookmarks and history from browsers on this computer. The originals are not removed."));
        let browsers = gtk::Box::new(gtk::Orientation::Vertical, 4);
        browsers.set_margin_top(12);
        browsers.pack_start(&welcome_label("Add what you already keep. The originals stay in your other browser.", "welcome-description"), false, true, 0);
        for browser in found {
            browsers.pack_start(&import_row(app, browser, &status), false, true, 0);
        }
        importing.add(&browsers);
        let weak = app.weak();
        importing.connect_expanded_notify(move |_| {
            if let Some(app) = weak.upgrade() {
                FITTED_GEOMETRY.with(|geometry| geometry.set((0, 0)));
                relayout(&app);
            }
        });
        content.pack_start(&importing, false, true, 0);
    }
    content.pack_start(&status, false, true, 0);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    crate::ui::class(&actions, "welcome-actions");
    let hint = welcome_label("Esc to skip", "welcome-hint");
    actions.pack_start(&hint, true, true, 0);
    let start = gtk::Button::with_label("Start");
    crate::ui::class(&start, "act");
    crate::ui::class(&start, "primary");
    crate::ui::class(&start, "welcome-start");
    let arrow = crate::icons::area(crate::icons::Icon::ArrowRight, 14, app.palette.borrow().muted);
    start.set_image(Some(&arrow));
    start.set_image_position(gtk::PositionType::Right);
    start.set_always_show_image(true);
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
    actions.pack_start(&start, false, false, 0);
    content.pack_start(&actions, false, true, 0);
    card.add(&content);

    app.ui.overlay.add_overlay(&card);
    card.show_all();
    apply_margins(app, card.upcast_ref::<gtk::Widget>());
    crate::motion::pop_card_in(&card, PAGE_TOP, -crate::motion::dur::TRAVEL, crate::motion::dur::ENTER);
    start.grab_focus();
}

fn welcome_label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    crate::ui::class(&label, class);
    label.set_xalign(0.0);
    label.set_line_wrap(true);
    label.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    label
}

/// One browser's line: its name and an Import button that runs once.
fn import_row(app: &AppRef, browser: Browser, status: &gtk::Label) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    crate::ui::class(&row, "welcome-import-row");
    row.set_margin_top(8);
    row.set_margin_bottom(8);
    let icon = crate::icons::area(crate::icons::Icon::Globe, 16, app.palette.borrow().muted);
    crate::ui::class(&icon, "welcome-browser-icon");
    icon.set_valign(gtk::Align::Center);
    row.pack_start(&icon, false, false, 0);
    let text = import::home().map(|h| import::label(&h, browser)).unwrap_or_else(|| browser.name().to_string());
    let name = gtk::Label::new(Some(&text));
    crate::ui::class(&name, "welcome-browser-name");
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_line_wrap(true);
    name.set_line_wrap_mode(gtk::pango::WrapMode::WordChar);
    name.set_lines(2);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_tooltip_text(Some(&text));
    row.pack_start(&name, true, true, 0);

    let button = gtk::Button::with_label("Import");
    crate::ui::class(&button, "act");
    button.set_valign(gtk::Align::Center);
    {
        let weak = app.weak();
        let status = status.downgrade();
        button.connect_clicked(move |button| {
            button.set_sensitive(false);
            let Some(app) = weak.upgrade() else { return };
            // The databases are read off the main loop, the same as the
            // Settings row, so a large Firefox history does not freeze the
            // first-run card. The line says so while it reads.
            if let Some(status) = status.upgrade() {
                status.set_text("Reading bookmarks and history…");
                status.show();
            }
            FITTED_GEOMETRY.with(|geometry| geometry.set((0, 0)));
            relayout(&app);
            let status = status.clone();
            let button_weak = button.downgrade();
            let app_weak = app.weak();
            import::bring_later_checked(&app, vec![browser], move |ok, message| {
                if let Some(status) = status.upgrade() {
                    status.set_text(&message);
                    status.show();
                }
                if let Some(button) = button_weak.upgrade() {
                    if ok {
                        button.set_label("Imported");
                    } else {
                        button.set_sensitive(true);
                    }
                }
                if let Some(app) = app_weak.upgrade() {
                    FITTED_GEOMETRY.with(|geometry| geometry.set((0, 0)));
                    relayout(&app);
                }
            });
        });
    }
    row.pack_start(&button, false, false, 0);
    row
}
