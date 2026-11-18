// The four panels that hold everything else: what you kept, where you went,
// what you brought down, and how the browser is set up.
use crate::app::AppRef;
use crate::palette::Palette;
use crate::store::{ago, now_secs, Records};
use gtk::prelude::*;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PanelKind {
    Bookmarks,
    History,
    Downloads,
    Settings,
    Shortcuts,
}

impl PanelKind {
    fn title(&self) -> &'static str {
        match self {
            PanelKind::Bookmarks => "Bookmarks",
            PanelKind::History => "History",
            PanelKind::Downloads => "Downloads",
            PanelKind::Settings => "Settings",
            PanelKind::Shortcuts => "Shortcuts",
        }
    }
    fn all() -> [PanelKind; 5] {
        [
            PanelKind::Bookmarks,
            PanelKind::History,
            PanelKind::Downloads,
            PanelKind::Settings,
            PanelKind::Shortcuts,
        ]
    }
}

pub struct Panels {
    pub wanted_height: std::cell::Cell<i32>,

    column: std::cell::Cell<i32>,
    pub filter: std::cell::RefCell<String>,
    pub holder: gtk::Fixed,
    layer: crate::layer::Layer,
    pub scrim: gtk::DrawingArea,
    pub card: gtk::Box,
    pub title: gtk::Label,
    pub nav: gtk::Box,
    pub body: gtk::Box,
    pub open: Option<PanelKind>,
}

impl Panels {
    pub fn new(palette: &Palette) -> Panels {
        let layer = crate::layer::Layer::new();
        let holder = layer.root.clone();

        let scrim = crate::ui::scrim(palette);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        crate::ui::class(&card, "panel");
        card.set_size_request(680, 420);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.set_margin_start(20);
        head.set_margin_end(14);
        head.set_margin_top(16);
        head.set_margin_bottom(10);
        let title = gtk::Label::new(Some("Bookmarks"));
        crate::ui::class(&title, "head");
        title.set_xalign(0.0);
        title.set_hexpand(true);
        head.add(&title);
        let close = crate::icons::button(crate::icons::Icon::Close, 13, palette.muted, "door");
        close.set_valign(gtk::Align::Center);
        head.add(&close);
        card.add(&head);

        let nav = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        nav.set_margin_start(16);
        nav.set_margin_end(16);
        nav.set_margin_bottom(8);
        card.add(&nav);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        body.set_margin_start(16);
        body.set_margin_end(16);
        body.set_margin_bottom(16);
        body.set_vexpand(true);
        card.add(&body);

        layer.add(&scrim);
        layer.add(&card);

        let panels = Panels {
            wanted_height: std::cell::Cell::new(420),
            column: std::cell::Cell::new(0),
            filter: std::cell::RefCell::new(String::new()),
            holder,
            layer,
            scrim,
            card,
            title,
            nav,
            body,
            open: None,
        };

        close.connect_clicked(move |_| {
            if let Some(app) = crate::app::current() {
                app.close_panels();
            }
        });
        {
            let scrim = panels.scrim.clone();
            scrim.connect_button_press_event(move |_, _| {
                if let Some(app) = crate::app::current() {
                    app.close_panels();
                }
                crate::gtkx::inh(true)
            });
        }
        panels
    }

    /// A filter typed into a panel: the panel is rebuilt with it applied, and
    /// the field is given back what was typed so the caret stays where it was.
    pub fn set_filter(&mut self, app: &AppRef, kind: PanelKind, text: &str) {
        *self.filter.borrow_mut() = text.to_lowercase();
        let caret = text.chars().count() as i32;
        let entry = self.open_panel_with(app, kind);
        if let Some(entry) = entry {
            entry.set_text(text);
            entry.grab_focus();
            entry.set_position(caret);
        }
    }

    fn open_panel_with(&mut self, app: &AppRef, kind: PanelKind) -> Option<gtk::Entry> {
        self.open = Some(kind.clone());
        self.title.set_text(kind.title());
        self.build_nav(app, &kind);
        let field = match kind {
            PanelKind::Bookmarks => self.build_bookmarks(app),
            PanelKind::History => self.build_history(app),
            PanelKind::Downloads => {
                self.build_downloads(app);
                None
            }
            PanelKind::Settings => {
                self.build_settings(app);
                None
            }
            PanelKind::Shortcuts => {
                self.build_shortcuts();
                None
            }
        };
        let card_width = 680.min(app.ui.window.allocated_width() - 48).max(320);
        let wanted = self.measure_content(card_width - 40).max(self.body.preferred_height_for_width(card_width - 32).0) + 140;
        self.wanted_height.set(wanted.clamp(300, 640));
        self.layer.show();
        self.layout(app.ui.window.allocated_width(), app.ui.window.allocated_height());
        field
    }

    fn matches(&self, text: &str) -> bool {
        let filter = self.filter.borrow();
        filter.is_empty() || text.to_lowercase().contains(filter.as_str())
    }

    pub fn open_panel(&mut self, app: &AppRef, kind: PanelKind) {
        *self.filter.borrow_mut() = String::new();
        self.open_panel_with(app, kind);
    }

    /// The card stands in the middle of the page it covers.
    pub fn layout(&self, width: i32, height: i32) {
        self.layer.place(&self.scrim, 0, 0, width, height);
        self.layout_card(width, height);
    }

    pub fn layout_with_column(&self, width: i32, height: i32, column: i32) {
        // The column stays readable while a panel is up: the scrim covers the
        // page, not the tabs.
        self.column.set(column);
        self.layout_card(width, height);
    }

    fn layout_card(&self, width: i32, height: i32) {
        let column = self.column.get();
        self.layer.place(&self.scrim, column, 0, (width - column).max(0), height);
        let card_width = 680.min(width - 48);
        // The card is as tall as what it holds, within reason: a panel with
        // one line in it should not be a window with one line in it.
        let card_height = self.wanted_height.get().min(height - 80).max(240);
        // Centred on the page the panel covers. Centring on the window would
        // put the card half a column left of what it is talking about.
        let page = (width - column).max(0);
        self.layer.place(
            &self.card,
            column + ((page - card_width) / 2).max(12),
            (height - card_height) / 2,
            card_width,
            card_height,
        );
    }

    pub fn close(&mut self, _ui: &crate::ui::Ui) {
        self.open = None;
        self.layer.hide();
    }

    fn build_nav(&self, app: &AppRef, current: &PanelKind) {
        for child in self.nav.children() {
            self.nav.remove(&child);
        }
        for kind in PanelKind::all() {
            let button = gtk::Button::new();
            crate::ui::class(&button, "tab");
            let label = gtk::Label::new(Some(kind.title()));
            button.add(&label);
            if &kind == current {
                button.set_state_flags(gtk::StateFlags::CHECKED, false);
                crate::ui::class(&button, "on");
            }
            let app_weak = app.weak();
            let target = kind.clone();
            button.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    app.toggle_panel_switch(target.clone());
                }
            });
            self.nav.add(&button);
        }
        self.nav.show_all();
    }

    fn clear_body(&self) {
        for child in self.body.children() {
            self.body.remove(&child);
        }
    }

    /// How tall the rows want to be. A scroller asked for its preferred height
    /// answers with what the window gives it, not with what it holds, so the
    /// height the card asks for has to come from the list inside it: otherwise
    /// a panel opens around two rows and cuts the third in half.
    fn measure_content(&self, width: i32) -> i32 {
        for child in self.body.children() {
            if let Ok(scroll) = child.downcast::<gtk::ScrolledWindow>() {
                if let Some(inner) = scroll.child() {
                    return inner.preferred_height_for_width(width).0 + 4;
                }
            }
        }
        0
    }

    fn body_scroll(&self, content: &gtk::Box) -> gtk::ScrolledWindow {
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_vexpand(true);
        scroll.add(content);
        scroll
    }

    fn filter_field(&self, placeholder: &str, on_type: impl Fn(&str) + 'static) -> gtk::Entry {
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some(placeholder));
        entry.set_margin_bottom(8);
        let on_type = std::rc::Rc::new(on_type);
        entry.connect_changed(move |entry| on_type(&entry.text()));
        entry
    }

    fn build_bookmarks(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_body();
        let bookmarks = app.state.borrow().bookmarks.clone();
        if bookmarks.is_empty() {
            self.body.add(&self.empty_state("Nothing kept yet. ⌘D, or Ctrl+D, keeps the page you are on."));
            self.body.show_all();
            return None;
        }
        let filter = self.filter_field("Search bookmarks", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::Bookmarks, text);
            }
        });
        self.body.add(&filter);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for bookmark in bookmarks.into_iter().filter(|bookmark| self.matches(&format!("{} {}", bookmark.title, bookmark.url))) {
            let row = self.link_row(app, &bookmark.title, &bookmark.url, "Open", {
                let url = bookmark.url.clone();
                move |app: &AppRef| app.navigate_active(&url)
            });
            list.add(&row);
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
        Some(filter)
    }

    fn build_history(&self, app: &AppRef) -> Option<gtk::Entry> {
        self.clear_body();
        let history = app.state.borrow().history.clone();
        if history.is_empty() {
            self.body.add(&self.empty_state("The pages you visit show up here, on this machine only."));
            self.body.show_all();
            return None;
        }
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_margin_bottom(8);
        let count = gtk::Label::new(Some(&format!("{} pages", history.len())));
        crate::ui::class(&count, "sub");
        count.set_xalign(0.0);
        count.set_hexpand(true);
        header.add(&count);
        let clear = gtk::Button::with_label("Clear History");
        crate::ui::class(&clear, "act");
        {
            let app_weak = app.weak();
            clear.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    app.state.borrow_mut().history.clear();
                    Records::save_history(&[]);
                    app.toggle_panel_switch(PanelKind::History);
                }
            });
        }
        header.add(&clear);
        self.body.add(&header);

        let filter = self.filter_field("Search history", |text| {
            if let Some(app) = crate::app::current() {
                app.filter_panel(PanelKind::History, text);
            }
        });
        self.body.add(&filter);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let now = now_secs();
        for entry in history.iter().take(300).filter(|entry| self.matches(&format!("{} {}", entry.title, entry.url))) {
            let title = if entry.title.is_empty() {
                crate::shield::host_of(&entry.url)
            } else {
                entry.title.clone()
            };
            let when = ago(entry.at, now);
            let row = self.link_row(app, &title, &format!("{} · {}", entry.url, when), "Open", {
                let url = entry.url.clone();
                move |app: &AppRef| app.navigate_active(&url)
            });
            list.add(&row);
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
        Some(filter)
    }

    fn build_downloads(&self, app: &AppRef) {
        self.clear_empty_or_list(app);
    }

    fn clear_empty_or_list(&self, app: &AppRef) {
        self.clear_body();
        let downloads = app.state.borrow().downloads.clone();
        if downloads.is_empty() {
            self.body.add(&self.empty_state("Files you download land here and in your Downloads folder."));
            self.body.show_all();
            return;
        }
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for download in downloads {
            let sub = if download.done { download.path.clone() } else { format!("Downloading to {}", download.path) };
            let row = self.link_row(app, &download.name, &sub, "Reveal", {
                let path = download.path.clone();
                move |_app: &AppRef| {
                    let _ = std::process::Command::new("xdg-open")
                        .arg(std::path::Path::new(&path).parent().unwrap_or(std::path::Path::new("/")))
                        .spawn();
                }
            });
            list.add(&row);
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    fn build_settings(&self, app: &AppRef) {
        self.clear_body();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let prefs = app.prefs.borrow().clone();

        // Appearance
        let appearance = gtk::Box::new(gtk::Orientation::Vertical, 6);
        appearance.set_margin_top(4);
        appearance.set_margin_bottom(14);
        let label = gtk::Label::new(Some("Appearance"));
        crate::ui::class(&label, "setting");
        label.set_xalign(0.0);
        appearance.add(&label);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        crate::ui::class(&buttons, "segmented");
        for look in [crate::store::Look::Light, crate::store::Look::Dark, crate::store::Look::System] {
            let button = gtk::Button::with_label(look.title());
            crate::ui::class(&button, "act");
            if look == prefs.appearance {
                crate::ui::class(&button, "chosen");
            }
            let app_weak = app.weak();
            let chosen = look.clone();
            button.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    app.switch_look(chosen.clone());
                    app.toggle_panel_switch(PanelKind::Settings);
                }
            });
            buttons.add(&button);
        }
        appearance.add(&buttons);
        list.add(&appearance);

        let browsing = gtk::Label::new(Some("BROWSING"));
        crate::ui::class(&browsing, "group");
        browsing.set_xalign(0.0);
        list.add(&browsing);
        list.add(&self.setting_switch(app, "Block ads and trackers", "Rules enforced inside the engine, before a request is made.", prefs.shield, |app, on| {
            app.prefs.borrow_mut().shield = on;
            app.prefs.borrow().save();
            app.flash_toast("The shield applies to new pages");
        }));

        list.add(&self.setting_switch(app, "Keep hidden things hidden", "What you hide on a site stays hidden there.", prefs.curtain, |app, on| {
            app.prefs.borrow_mut().curtain = on;
            app.prefs.borrow().save();
            app.reload();
        }));

        list.add(&self.setting_switch(
            app,
            "Let tabs sleep",
            &format!(
                "A tab you have not looked at for {} minutes lets go of its page, and reloads when you come back.",
                prefs.sleep_minutes
            ),
            prefs.memory_saver,
            |app, on| {
                app.prefs.borrow_mut().memory_saver = on;
                app.prefs.borrow().save();
            },
        ));

        let tabs_head = gtk::Label::new(Some("TABS"));
        crate::ui::class(&tabs_head, "group");
        tabs_head.set_xalign(0.0);
        list.add(&tabs_head);
        list.add(&self.setting_switch(
            app,
            "Darken pages that stay white",
            "Sites that do not offer a dark page get one from the browser. Off by default: a page's colour is the site's to choose.",
            prefs.darken_pages,
            |app, on| {
                app.prefs.borrow_mut().darken_pages = on;
                app.prefs.borrow().save();
                app.apply_page_tone();
                app.reload();
            },
        ));

        list.add(&self.setting_switch(app, "Roomy tabs", "Taller rows, as the Mac build offers.", prefs.roomy_tabs, |app, on| {
            app.prefs.borrow_mut().roomy_tabs = on;
            app.prefs.borrow().save();
            app.apply_style();
            app.render();
            app.toggle_panel_switch(PanelKind::Settings);
        }));

        list.add(&self.setting_switch(app, "Open where you left off", "Tabs come back when the browser starts again.", prefs.restore_session, |app, on| {
            app.prefs.borrow_mut().restore_session = on;
            app.prefs.borrow().save();
        }));

        // Search engine
        let engine_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        engine_row.set_margin_top(8);
        engine_row.set_margin_bottom(8);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        let title = gtk::Label::new(Some("Search engine"));
        crate::ui::class(&title, "setting");
        title.set_xalign(0.0);
        let sub = gtk::Label::new(Some("What words in the field search with"));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        texts.add(&title);
        texts.add(&sub);
        engine_row.add(&texts);
        let combo = gtk::ComboBoxText::new();
        for engine in crate::store::Prefs::engines() {
            combo.append_text(engine);
        }
        let engines = crate::store::Prefs::engines();
        let index = engines.iter().position(|e| *e == prefs.search_engine).unwrap_or(0);
        combo.set_active(Some(index as u32));
        {
            let app_weak = app.weak();
            combo.connect_changed(move |combo| {
                if let Some(app) = app_weak.upgrade() {
                    if let Some(text) = combo.active_text() {
                        app.prefs.borrow_mut().search_engine = text.to_string();
                        app.prefs.borrow().save();
                    }
                }
            });
        }
        engine_row.add(&combo);
        list.add(&engine_row);

        let hidden = app
            .state
            .borrow()
            .active_tab()
            .map(|t| crate::shield::host_of(&t.url))
            .unwrap_or_default();
        let count = app.state.borrow().hidden.get(&hidden).map(|v| v.len()).unwrap_or(0);
        let unhide_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        unhide_row.set_margin_top(7);
        unhide_row.set_margin_bottom(7);
        let unhide_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        unhide_text.set_hexpand(true);
        let unhide_title = gtk::Label::new(Some("Hidden here"));
        crate::ui::class(&unhide_title, "setting");
        unhide_title.set_xalign(0.0);
        let unhide_sub = gtk::Label::new(Some(&format!("{count} things hidden on {hidden}")));
        crate::ui::class(&unhide_sub, "settingdesc");
        unhide_sub.set_xalign(0.0);
        unhide_text.add(&unhide_title);
        unhide_text.add(&unhide_sub);
        crate::ui::pack(&unhide_row, &unhide_text, true);
        let show_all = gtk::Button::with_label("Show them");
        crate::ui::class(&show_all, "act");
        {
            let weak = app.weak();
            show_all.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.unhide_all();
                    app.toggle_panel_switch(PanelKind::Settings);
                }
            });
        }
        crate::ui::pack(&unhide_row, &show_all, false);
        list.add(&unhide_row);

        // Group headings, so a list of switches is not a wall of switches.
        

        let about = gtk::Label::new(Some(&format!(
            "Search {} · WebKitGTK · Rust · this machine only",
            env!("CARGO_PKG_VERSION")
        )));
        crate::ui::class(&about, "settingdesc");
        about.set_xalign(0.0);
        about.set_margin_top(16);
        list.add(&about);

        // Settings is a list of rows, not a document: give it to the body
        // directly so the card can measure it and be as tall as it.
        self.body.add(&list);
        self.body.show_all();
    }

    /// Every key the browser keeps, in one table. A browser that hides its
    /// keys is a browser whose keys nobody learns.
    fn build_shortcuts(&self) {
        self.clear_body();
        let keys: [(&str, &str); 28] = [
            ("Ctrl /", "This list"),
            ("Ctrl L", "The field"),
            ("Ctrl T", "New tab"),
            ("Ctrl W", "Close tab"),
            ("Ctrl Shift T", "Reopen the last closed tab"),
            ("Ctrl Tab", "Next tab"),
            ("Ctrl 1 to 9", "Jump to a tab"),
            ("Ctrl [ ]", "Back and forward"),
            ("Ctrl R", "Reload"),
            ("Ctrl F", "Find in the page"),
            ("Ctrl D", "Keep this page"),
            ("Ctrl Y", "History"),
            ("Ctrl Shift B", "Bookmarks"),
            ("Ctrl J", "Downloads"),
            ("Ctrl Shift R", "Reading mode (again to leave)"),
            ("Ctrl Shift H", "Hide something on this site"),
            ("Ctrl Shift U", "Show what is hidden here"),
            ("Ctrl ,", "Settings"),
            ("Ctrl Shift I", "The inspector"),
            ("Ctrl N", "New tab (Ctrl Shift N: private)"),
            ("Ctrl Shift Tab", "Previous tab"),
            ("Ctrl Page Up Down", "Move between tabs"),
            ("Ctrl Shift C", "Copy the address"),
            ("Ctrl + - 0", "Zoom in, out, reset"),
            ("F5", "Reload (Ctrl F5 skips the cache)"),
            ("F12", "The inspector"),
            ("Alt Left Right", "Back and forward"),
            ("Escape", "Close the field, find, a panel, or reading mode"),
        ];
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (key, what) in keys {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.set_margin_top(4);
            row.set_margin_bottom(4);
            let keycap = gtk::Label::new(Some(key));
            crate::ui::class(&keycap, "keycap");
            keycap.set_xalign(0.0);
            keycap.set_size_request(150, -1);
            crate::ui::pack(&row, &keycap, false);
            let label = gtk::Label::new(Some(what));
            crate::ui::class(&label, "setting");
            label.set_xalign(0.0);
            crate::ui::pack(&row, &label, true);
            list.add(&row);
        }
        // 28 rows are taller than the card's tallest allowed height, so the
        // sheet scrolls like every other panel instead of losing its last rows.
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    fn setting_switch(
        &self,
        app: &AppRef,
        title: &str,
        subtitle: &str,
        value: bool,
        changed: impl Fn(&AppRef, bool) + 'static,
    ) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_margin_top(7);
        row.set_margin_bottom(7);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(title));
        crate::ui::class(&name, "setting");
        name.set_xalign(0.0);
        let sub = gtk::Label::new(Some(subtitle));
        crate::ui::class(&sub, "settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        texts.add(&name);
        texts.add(&sub);
        crate::ui::pack(&row, &texts, true);
        let switch = gtk::Switch::new();
        switch.set_active(value);
        switch.set_valign(gtk::Align::Center);
        let app_weak = app.weak();
        let changed = std::rc::Rc::new(changed);
        switch.connect_active_notify(move |switch| {
            if let Some(app) = app_weak.upgrade() {
                changed(&app, switch.is_active());
            }
        });
        crate::ui::pack(&row, &switch, false);
        row
    }

    fn empty_state(&self, text: &str) -> gtk::Label {
        let label = gtk::Label::new(Some(text));
        crate::ui::class(&label, "empty");
        label.set_line_wrap(true);
        label.set_margin_top(40);
        label.set_justify(gtk::Justification::Center);
        label
    }

    fn link_row(
        &self,
        app: &AppRef,
        title: &str,
        subtitle: &str,
        action: &str,
        run: impl Fn(&AppRef) + 'static,
    ) -> gtk::EventBox {
        let row = gtk::EventBox::new();
        crate::ui::class(&row, "ptrow");
        row.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        hbox.set_margin_top(7);
        hbox.set_margin_bottom(7);
        hbox.set_margin_start(6);
        hbox.set_margin_end(6);

        let letter = gtk::Label::new(Some(
            &title.chars().next().unwrap_or('\u{2022}').to_uppercase().to_string(),
        ));
        crate::ui::class(&letter, "tileletter");
        let tile = gtk::EventBox::new();
        crate::ui::class(&tile, "tile");
        tile.set_size_request(18, 18);
        tile.set_valign(gtk::Align::Center);
        tile.add(&letter);
        crate::ui::pack(&hbox, &tile, false);

        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(title));
        crate::ui::class(&name, "ptitle");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let url = gtk::Label::new(Some(subtitle));
        crate::ui::class(&url, "purl");
        url.set_xalign(0.0);
        url.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        texts.add(&name);
        texts.add(&url);
        crate::ui::pack(&hbox, &texts, true);

        let button = gtk::Button::with_label(action);
        crate::ui::class(&button, "act");
        button.set_valign(gtk::Align::Center);
        crate::ui::pack(&hbox, &button, false);

        row.add(&hbox);

        // The whole row runs the row's action: a row that lights up under the
        // cursor and ignores the click is worse than a plain row.
        {
            let app_weak = app.weak();
            let run = std::rc::Rc::new(run);
            let run_row = run.clone();
            row.connect_button_press_event(move |_, event| {
                if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
                    if button.button() == 1 {
                        if let Some(app) = app_weak.upgrade() {
                            run_row(&app);
                        }
                        return crate::gtkx::inh(true);
                    }
                }
                crate::gtkx::inh(false)
            });
            let app_weak = app.weak();
            button.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    run(&app);
                }
            });
        }
        row
    }
}
