// The four panels that hold everything else: what you kept, where you went,
// what you brought down, and how the browser is set up.
use crate::app::{App, AppRef};
use crate::palette::Palette;
use crate::store::{ago, now_secs, Records};
use gtk::prelude::*;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PanelKind {
    Bookmarks,
    History,
    Downloads,
    Settings,
}

impl PanelKind {
    fn title(&self) -> &'static str {
        match self {
            PanelKind::Bookmarks => "Bookmarks",
            PanelKind::History => "History",
            PanelKind::Downloads => "Downloads",
            PanelKind::Settings => "Settings",
        }
    }
    fn all() -> [PanelKind; 4] {
        [PanelKind::Bookmarks, PanelKind::History, PanelKind::Downloads, PanelKind::Settings]
    }
}

pub struct Panels {
    pub holder: gtk::Fixed,
    layer: crate::layer::Layer,
    pub scrim: gtk::DrawingArea,
    pub card: gtk::Box,
    pub title: gtk::Label,
    pub nav: gtk::Box,
    pub body: gtk::Box,
    pub open: Option<PanelKind>,
    pub palette: Palette,
}

impl Panels {
    pub fn new(palette: &Palette) -> Panels {
        let layer = crate::layer::Layer::new();
        let holder = layer.root.clone();

        let scrim = crate::ui::scrim(palette);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.style_context().add_class("panel");
        card.set_size_request(680, 420);

        let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        head.set_margin_start(20);
        head.set_margin_end(14);
        head.set_margin_top(16);
        head.set_margin_bottom(10);
        let title = gtk::Label::new(Some("Bookmarks"));
        title.style_context().add_class("head");
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
            holder,
            layer,
            scrim,
            card,
            title,
            nav,
            body,
            open: None,
            palette: *palette,
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

    pub fn open_panel(&mut self, app: &AppRef, kind: PanelKind) {
        self.open = Some(kind.clone());
        self.title.set_text(kind.title());
        self.layer.show();
        self.build_nav(app, &kind);
        match kind {
            PanelKind::Bookmarks => self.build_bookmarks(app),
            PanelKind::History => self.build_history(app),
            PanelKind::Downloads => self.build_downloads(app),
            PanelKind::Settings => self.build_settings(app),
        }
        self.layer.show();
        self.layout(app.ui.window.allocated_width(), app.ui.window.allocated_height());
    }

    /// The card stands in the middle of the page it covers.
    pub fn layout(&self, width: i32, height: i32) {
        self.layer.place(&self.scrim, 0, 0, width, height);
        self.layout_card(width, height);
    }

    pub fn layout_with_column(&self, width: i32, height: i32) {
        // The column stays readable while a panel is up: the scrim covers the
        // page, not the tabs.
        let column = 232;
        self.layer.place(&self.scrim, column, 0, (width - column).max(0), height);
        self.layout_card(width, height);
    }

    fn layout_card(&self, width: i32, height: i32) {
        self.layer.place(&self.scrim, 0, 0, width, height);
        let card_width = 680.min(width - 48);
        // The card is as tall as what it holds, within reason: a panel with
        // one line in it should not be a window with one line in it.
        let wanted = self.body.preferred_height_for_width(card_width - 32).0 + 132;
        let card_height = wanted.clamp(300, 640).min(height - 80);
        self.layer.place(
            &self.card,
            (width - card_width) / 2,
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
            button.style_context().add_class("tab");
            let label = gtk::Label::new(Some(kind.title()));
            button.add(&label);
            if &kind == current {
                button.set_state_flags(gtk::StateFlags::CHECKED, false);
                button.style_context().add_class("on");
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

    fn body_scroll(&self, content: &gtk::Box) -> gtk::ScrolledWindow {
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_vexpand(true);
        scroll.add(content);
        scroll
    }

    fn build_bookmarks(&self, app: &AppRef) {
        self.clear_body();
        let bookmarks = app.state.borrow().bookmarks.clone();
        if bookmarks.is_empty() {
            self.body.add(&self.empty_state("Nothing kept yet. ⌘D, or Ctrl+D, keeps the page you are on."));
            self.body.show_all();
            return;
        }
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for bookmark in bookmarks {
            let row = self.link_row(app, &bookmark.title, &bookmark.url, "Open", {
                let url = bookmark.url.clone();
                move |app: &AppRef| app.navigate_active(&url)
            });
            list.add(&row);
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    fn build_history(&self, app: &AppRef) {
        self.clear_body();
        let history = app.state.borrow().history.clone();
        if history.is_empty() {
            self.body.add(&self.empty_state("The pages you visit show up here, on this machine only."));
            self.body.show_all();
            return;
        }
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_margin_bottom(8);
        let count = gtk::Label::new(Some(&format!("{} pages", history.len())));
        count.style_context().add_class("sub");
        count.set_xalign(0.0);
        count.set_hexpand(true);
        header.add(&count);
        let clear = gtk::Button::with_label("Clear History");
        clear.style_context().add_class("act");
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

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let now = now_secs();
        for entry in history.iter().take(300) {
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
        label.style_context().add_class("setting");
        label.set_xalign(0.0);
        appearance.add(&label);
        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        for look in [crate::store::Look::Light, crate::store::Look::Dark, crate::store::Look::System] {
            let button = gtk::Button::with_label(look.title());
            button.style_context().add_class("act");
            if look == prefs.appearance {
                button.style_context().add_class("on");
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

        list.add(&self.setting_switch(app, "Let tabs sleep", "Tabs you have not looked at let go of their page after half an hour.", prefs.memory_saver, |app, on| {
            app.prefs.borrow_mut().memory_saver = on;
            app.prefs.borrow().save();
        }));

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
        title.style_context().add_class("setting");
        title.set_xalign(0.0);
        let sub = gtk::Label::new(Some("What words in the field search with"));
        sub.style_context().add_class("settingdesc");
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
        list.add(&self.setting_switch(app, "Nothing hidden here", &format!("{count} things hidden on {hidden}"), false, |app, _| {
            app.unhide_all();
        }));

        let about = gtk::Label::new(Some(&format!(
            "Search {} · WebKitGTK · Rust · this machine only",
            env!("CARGO_PKG_VERSION")
        )));
        about.style_context().add_class("settingdesc");
        about.set_xalign(0.0);
        about.set_margin_top(16);
        list.add(&about);

        // Settings is a list of rows, not a document: give it to the body
        // directly so the card can measure it and be as tall as it.
        self.body.add(&list);
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
        name.style_context().add_class("setting");
        name.set_xalign(0.0);
        let sub = gtk::Label::new(Some(subtitle));
        sub.style_context().add_class("settingdesc");
        sub.set_xalign(0.0);
        sub.set_line_wrap(true);
        texts.add(&name);
        texts.add(&sub);
        row.add(&texts);
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
        row.add(&switch);
        row
    }

    fn empty_state(&self, text: &str) -> gtk::Label {
        let label = gtk::Label::new(Some(text));
        label.style_context().add_class("empty");
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
        row.style_context().add_class("ptrow");
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        hbox.set_margin_top(7);
        hbox.set_margin_bottom(7);
        hbox.set_margin_start(6);
        hbox.set_margin_end(6);
        let letter = gtk::Label::new(Some(&title.chars().next().unwrap_or('•').to_uppercase().to_string()));
        letter.style_context().add_class("tileletter");
        let tile = gtk::EventBox::new();
        tile.style_context().add_class("tile");
        tile.set_size_request(18, 18);
        tile.set_valign(gtk::Align::Center);
        tile.add(&letter);
        hbox.add(&tile);
        let texts = gtk::Box::new(gtk::Orientation::Vertical, 1);
        texts.set_hexpand(true);
        let name = gtk::Label::new(Some(title));
        name.style_context().add_class("ptitle");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let url = gtk::Label::new(Some(subtitle));
        url.style_context().add_class("purl");
        url.set_xalign(0.0);
        url.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        texts.add(&name);
        texts.add(&url);
        hbox.add(&texts);
        let button = gtk::Button::with_label(action);
        button.style_context().add_class("act");
        button.set_valign(gtk::Align::Center);
        let app_weak = app.weak();
        let run = std::rc::Rc::new(run);
        button.connect_clicked(move |_| {
            if let Some(app) = app_weak.upgrade() {
                run(&app);
            }
        });
        hbox.add(&button);
        row.add(&hbox);
        let app_weak = app.weak();
        let run = std::rc::Rc::new(()); // the row itself only highlights
        row.connect_button_press_event(move |_, _| {
            let _ = (&app_weak, &run);
            crate::gtkx::inh(false)
        });
        row
    }
}
