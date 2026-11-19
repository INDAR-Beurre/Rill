// The four panels that hold everything else: what you kept, where you went,
// what you brought down, and how the browser is set up.
use crate::app::AppRef;
use crate::palette::Palette;
use crate::store::{ago, day_of, now_secs, Records};
use gtk::prelude::*;

/// The card's horizontal inset: head, tab strip and body all start and end
/// here, so nothing hangs further out than anything else.
const INSET: i32 = 16;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PanelKind {
    Bookmarks,
    History,
    Downloads,
    Extensions,
    Settings,
    Shortcuts,
}

impl PanelKind {
    fn title(&self) -> &'static str {
        match self {
            PanelKind::Bookmarks => "Bookmarks",
            PanelKind::History => "History",
            PanelKind::Downloads => "Downloads",
            PanelKind::Extensions => "Extensions",
            PanelKind::Settings => "Settings",
            PanelKind::Shortcuts => "Shortcuts",
        }
    }
    fn all() -> [PanelKind; 6] {
        [
            PanelKind::Bookmarks,
            PanelKind::History,
            PanelKind::Downloads,
            PanelKind::Extensions,
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
    /// The card width the body was last measured at. A resize changes the width
    /// a wrapping line needs, so the height has to be asked again.
    fitted_width: std::cell::Cell<i32>,
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
        // One inset for the whole card: the title's left edge and the last
        // control's right edge sit the same distance from the border.
        head.set_margin_start(INSET);
        head.set_margin_end(INSET);
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
        nav.set_margin_start(INSET);
        nav.set_margin_end(INSET);
        nav.set_margin_bottom(8);
        card.add(&nav);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // The scroller clips to its allocation; a box does not. Let the
        // viewport own scrolling and let this wrapper shrink with the card.
        body.set_hexpand(true);
        body.set_margin_start(INSET);
        body.set_margin_end(INSET);
        body.set_margin_bottom(16);
        body.set_vexpand(true);
        // The ScrolledWindow owns clipping. Giving every direct child the
        // entire body height blanks filter fields and leaves rows half-clipped.
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
            fitted_width: std::cell::Cell::new(0),
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
            PanelKind::Extensions => {
                self.build_extensions(app);
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
        // Shown before measured. A widget that is not shown yet reports a
        // preferred height of 0, so measuring the title row and the nav before
        // this point counted them as nothing: the card came out 45px shorter
        // than its own header and nav, and the last of the extensions note was
        // cut off below the edge. This is the order the two have to be in.
        self.card.show_all();
        // An empty panel should be a note, not a slab: the floor used to be
        // 300px of nothing under one line of text.
        self.wanted_height.set(self.fit_height(card_width));
        self.card.set_can_focus(true);
        self.layer.show();
        self.layout(app.ui.window.allocated_width(), app.ui.window.allocated_height());
        if field.is_none() {
            self.card.grab_focus();
        }
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
        // A narrower window wraps the body's sentences onto more lines, so the
        // height the card asked for on the way in is no longer the height it
        // needs. Re-ask whenever the width moved.
        if self.open.is_some() && self.fitted_width.get() != card_width {
            self.wanted_height.set(self.fit_height(card_width));
        }
        // The card is as tall as what it holds, within reason: a panel with
        // one line in it should not be a window with one line in it.
        let card_height = self.wanted_height.get().min(height - 80).max(240);
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

    /// The height the card has to be at this width, from the measure below.
    ///
    /// Ask the widgets above the list how tall they are instead of adding a
    /// round 140 and hoping: the guess was 34px short, which cut the last row of
    /// a list in half and made a scrollable card look broken.
    fn fit_height(&self, card_width: i32) -> i32 {
        let inner = (card_width - 2 * INSET).max(1);
        let mut above = 0;
        for child in self.card.children() {
            if child == self.body.clone().upcast::<gtk::Widget>() {
                continue;
            }
            above += child.preferred_height().0;
        }
        above += 16; // the body's bottom margin
        let wanted = self
            .measure_content(inner)
            .max(self.body.preferred_height_for_width(inner).1)
            + above;
        self.fitted_width.set(card_width);
        wanted.clamp(200, 640)
    }

    /// How tall the body wants to be at a given width, and the one place that
    /// tells a scroller how tall its list is.
    ///
    /// Every body child counts, not just the scroller. History and bookmarks
    /// put their count line and their filter field above the list, and
    /// measuring only the list asked for a card 80px shorter than the body
    /// holds, which squeezed the field to nothing.
    ///
    /// The scroller also needs telling. `propagate_natural_height` asks the
    /// content for its preferred height at the content's **natural** width, and
    /// a wrapping sentence's natural width is the whole sentence on one line:
    /// the answer was 174px while the list, once given 648px, needed 231. The
    /// viewport stayed 174 tall and the extensions note lost three of its four
    /// lines below the edge. Asking at the width the body actually has, and
    /// pinning the list to that answer, makes the two numbers agree.
    fn measure_content(&self, width: i32) -> i32 {
        let mut total = 0;
        for child in self.body.children() {
            if let Ok(scroll) = child.clone().downcast::<gtk::ScrolledWindow>() {
                if let Some(inner) = scroll.child() {
                    let needed = inner.preferred_height_for_width(width).1;
                    inner.set_size_request(-1, needed);
                }
            }
            // The natural height, not the minimum: a scroller's minimum is its
            // one-pixel floor plus its frame, which is how a 231px list was
            // measured as 46px.
            total += child.preferred_height_for_width(width).1;
        }
        total
    }

    fn body_scroll(&self, content: &gtk::Box) -> gtk::ScrolledWindow {
        let scroll = gtk::ScrolledWindow::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_propagate_natural_height(true);
        scroll.set_min_content_height(1);
        scroll.set_max_content_height(640);
        scroll.set_vexpand(false);
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
            self.body.add(&self.empty_state("Nothing kept yet. Ctrl D keeps the page you are on."));
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
        let mut shown = 0;
        for bookmark in bookmarks.into_iter().filter(|bookmark| self.matches(&format!("{} {}", bookmark.title, bookmark.url))) {
            shown += 1;
            let remove_url = bookmark.url.clone();
            let row = self.row_with(app, &bookmark.title, &bookmark.url, "Open", {
                let url = bookmark.url.clone();
                move |app: &AppRef| app.navigate_active(&url)
            }, Some(("Remove", Box::new(move |app: &AppRef| {
                {
                    let mut state = app.state.borrow_mut();
                    state.bookmarks.retain(|kept| kept.url != remove_url);
                    Records::save_bookmarks(&state.bookmarks);
                }
                app.toggle_panel_switch(PanelKind::Bookmarks);
            }))));
            list.add(&row);
        }
        if shown == 0 {
            list.add(&self.empty_state("Nothing kept matches what you typed."));
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
        let count = gtk::Label::new(Some(&if history.len() == 1 {
            "1 page".to_string()
        } else {
            format!("{} pages", history.len())
        }));
        crate::ui::class(&count, "sub");
        count.set_xalign(0.0);
        count.set_hexpand(true);
        header.add(&count);
        let clear = gtk::Button::with_label("Clear History");
        crate::ui::class(&clear, "act");
        // The one irreversible control in the panel says so before it is
        // pressed. The stylesheet owns the colour: `dangerink` (#C0392B light,
        // #EE8B76 dark) is 5.44:1 on the white chip and 5.87:1 on the dark one.
        // This only names the class; the rule belongs to css.rs.
        crate::ui::class(&clear, "danger");
        {
            let app_weak = app.weak();
            let armed = std::rc::Rc::new(std::cell::Cell::new(false));
            // Bumped on every arm and on confirm. A five-second tick only
            // disarms the press it was started for, so a stale timer cannot
            // reset the label of a newer arm, and a confirm leaves no pending
            // tick that touches the chip afterwards.
            let generation = std::rc::Rc::new(std::cell::Cell::new(0u32));
            let for_click = armed.clone();
            let gen_click = generation.clone();
            let button_weak = clear.downgrade();
            clear.connect_clicked(move |button| {
                if for_click.replace(false) {
                    gen_click.set(gen_click.get().wrapping_add(1));
                    if let Some(app) = app_weak.upgrade() {
                        app.state.borrow_mut().history.clear();
                        Records::save_history(&[]);
                        app.toggle_panel_switch(PanelKind::History);
                    }
                    return;
                }
                for_click.set(true);
                button.set_label("Clear history? Click again");
                let tick = gen_click.get().wrapping_add(1);
                gen_click.set(tick);
                let later_armed = armed.clone();
                let later_gen = gen_click.clone();
                let weak = button_weak.clone();
                gtk::glib::timeout_add_seconds_local(5, move || {
                    if later_gen.get() == tick {
                        later_armed.set(false);
                        if let Some(button) = weak.upgrade() {
                            button.set_label("Clear History");
                        }
                    }
                    gtk::glib::ControlFlow::Break
                });
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
        // The filter runs before the cap, not after: taking the newest 300 and
        // then searching them made every older page unfindable while the
        // header still counted it.
        let mut shown = 0;
        let mut day = String::new();
        for entry in history
            .iter()
            .filter(|entry| self.matches(&format!("{} {}", entry.title, entry.url)))
            .take(300)
        {
            // A flat list of "43 days ago" cannot answer "what did I read on
            // Tuesday". A heading per day costs one label and makes the list
            // scannable.
            let which = day_of(entry.at, now);
            if which != day {
                day = which.clone();
                let head = gtk::Label::new(Some(&day));
                crate::ui::class(&head, "group");
                head.set_xalign(0.0);
                head.set_margin_top(if shown == 0 { 0 } else { 10 });
                head.set_margin_bottom(4);
                list.add(&head);
            }
            shown += 1;
            let title = if entry.title.is_empty() {
                crate::shield::host_of(&entry.url)
            } else {
                entry.title.clone()
            };
            let when = ago(entry.at, now);
            let remove_url = entry.url.clone();
            let row = self.row_with(app, &title, &format!("{} · {}", entry.url, when), "Open", {
                let url = entry.url.clone();
                move |app: &AppRef| app.navigate_active(&url)
            }, Some(("Remove", Box::new(move |app: &AppRef| {
                {
                    let mut state = app.state.borrow_mut();
                    state.history.retain(|kept| kept.url != remove_url);
                    Records::save_history(&state.history);
                }
                app.toggle_panel_switch(PanelKind::History);
            }))));
            list.add(&row);
        }
        if shown == 0 {
            list.add(&self.empty_state("No page here matches what you typed."));
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
        let where_to = gtk::Label::new(Some(&format!(
            "Saved in {}",
            crate::web::downloads_dir().to_string_lossy()
        )));
        crate::ui::class(&where_to, "sub");
        where_to.set_xalign(0.0);
        where_to.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        where_to.set_margin_bottom(8);
        self.body.add(&where_to);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for download in downloads {
            let sub = download_subtitle(&download);
            let remove_path = download.path.clone();
            // "Open" only when there is a file to open: a button that does
            // nothing on a half-written download is worse than no button.
            let action = if download.done { "Open" } else { "Reveal" };
            let row = self.row_with(app, &download.name, &sub, action, {
                let path = download.path.clone();
                let finished = download.done;
                move |_app: &AppRef| {
                    let target = std::path::Path::new(&path);
                    let target = if finished {
                        target.to_path_buf()
                    } else {
                        target.parent().unwrap_or(std::path::Path::new("/")).to_path_buf()
                    };
                    let _ = std::process::Command::new("xdg-open").arg(target).spawn();
                }
            }, Some(("Remove", Box::new(move |app: &AppRef| {
                // Removes the row, not the file. Deleting someone's file from
                // a list of records is not what "Remove" means here.
                {
                    let mut state = app.state.borrow_mut();
                    state.downloads.retain(|kept| kept.path != remove_path);
                    Records::save_downloads(&state.downloads);
                }
                app.toggle_panel_switch(PanelKind::Downloads);
            }))));
            list.add(&row);
        }
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    /// Extensions, and the truth about them.
    ///
    /// A browser without a WebExtensions API cannot run most of what the stores
    /// list, so this panel shows every extension it has, which ones actually do
    /// something here, and the sentence saying why not for the others. Anything
    /// else would be a lie a person only discovers when their passwords do not
    /// fill in.
    fn build_extensions(&self, app: &AppRef) {
        self.clear_body();
        let installed = crate::extensions::registry().extensions;
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let head = gtk::Label::new(Some("INSTALLED"));
        crate::ui::class(&head, "group");
        head.set_xalign(0.0);
        list.add(&head);

        if installed.is_empty() {
            list.add(&self.empty_state(
                "Nothing installed. Put a .crx, an .xpi or an unpacked extension folder below.",
            ));
        }
        for entry in installed {
            let mut notes = vec![entry.why_not.replace('\n', " ")];
            notes.extend(crate::extensions::not_drawn(&entry));
            notes.retain(|note| !note.trim().is_empty());
            let subtitle = if entry.runs {
                format!("{}  ·  runs on the sites it asks for  ·  {}", entry.version, notes.join("; "))
            } else {
                format!("{}  ·  {}", entry.version, notes.join("; "))
            };
            let row = self.link_row(app, &entry.name, &subtitle, if entry.enabled { "Off" } else { "On" }, {
                let id = entry.id.clone();
                let on = entry.enabled;
                move |app: &AppRef| {
                    crate::extensions::set_enabled(&id, !on);
                    app.rebuild_every_view();
                    app.toggle_panel_switch(PanelKind::Extensions);
                }
            });
            list.add(&row);
            // An extension that ships an options page gets a way to open it.
            // It is a file on disk, so it opens as a page, which is what it is.
            if let Some(page) = crate::extensions::options_page(&entry) {
                let row = self.link_row(app, &format!("{} options", entry.name), "Opens the page the extension ships", "Options", {
                    let url = format!("file://{page}");
                    move |app: &AppRef| {
                        app.toggle_panel_switch(PanelKind::Extensions);
                        app.new_tab(Some(&url), false, false);
                    }
                });
                list.add(&row);
            }
            let remove = self.link_row(app, &format!("Remove {}", entry.name), &entry.source, "Remove", {
                let id = entry.id.clone();
                move |app: &AppRef| {
                    crate::extensions::remove(&id);
                    app.rebuild_every_view();
                    app.toggle_panel_switch(PanelKind::Extensions);
                }
            });
            list.add(&remove);
        }

        // What is acting on the page in front of you, which is the question a
        // person actually has when they install something.
        let current = app.state.borrow().tab(app.active_id()).map(|tab| tab.url.clone()).unwrap_or_default();
        let acting = if current.is_empty() { Vec::new() } else { crate::extensions::here(&current) };
        if !acting.is_empty() {
            let head = gtk::Label::new(Some("ON THIS PAGE"));
            crate::ui::class(&head, "group");
            head.set_xalign(0.0);
            list.add(&head);
            let line = gtk::Label::new(Some(&acting.join(", ")));
            crate::ui::class(&line, "settingdesc");
            line.set_xalign(0.0);
            line.set_line_wrap(true);
            list.add(&line);
        }

        let how = gtk::Label::new(Some("FROM A STORE OR A FOLDER"));
        crate::ui::class(&how, "group");
        how.set_xalign(0.0);
        list.add(&how);

        let field = gtk::Entry::new();
        field.set_placeholder_text(Some("A .crx, an .xpi, or a folder with a manifest.json"));
        field.set_margin_bottom(8);
        let app_field = app.weak();
        field.connect_activate(move |field| {
            let Some(app) = app_field.upgrade() else { return };
            let path = std::path::PathBuf::from(field.text().to_string());
            match crate::extensions::install(&path) {
                Ok(entry) => {
                    let message = if entry.runs {
                        format!("{} installed", entry.name)
                    } else {
                        format!("{} installed, but {}", entry.name, entry.why_not.replace('\n', " "))
                    };
                    app.rebuild_every_view();
                    app.flash_toast(&message);
                    app.toggle_panel_switch(PanelKind::Extensions);
                    app.toggle_panel_switch(PanelKind::Extensions);
                }
                Err(why) => app.flash_toast(&format!("Could not install: {why}")),
            }
        });
        list.add(&field);

        let note = gtk::Label::new(Some(
            "This engine has no WebExtensions API, so an extension runs the part of itself that is a content script: its scripts and its styles, on the sites its manifest names. Downloading a .crx from the Chrome Web Store or an .xpi from addons.mozilla.org and choosing it here installs what that part can do. An extension that lives entirely in a background page is installed and shown as not running rather than silently ignored.",
        ));
        crate::ui::class(&note, "settingdesc");
        note.set_xalign(0.0);
        note.set_line_wrap(true);
        // No width of its own: the body's width bounds it, and measure_content
        // asks for its height at exactly that width. Capping max_width_chars
        // here made GTK answer with the height at the cap instead (86px for a
        // sentence that fills 76), which only added slack to the card.
        note.set_margin_top(10);
        list.add(&note);

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

        list.add(&self.setting_switch(
            app,
            "Show the bookmarks bar",
            "The row of kept pages above every tab. Shown on its own as soon as you keep something.",
            prefs.show_bookmarks_bar,
            |app, on| {
                app.prefs.borrow_mut().show_bookmarks_bar = on;
                app.prefs.borrow().save();
                app.render_bmarks();
            },
        ));

        list.add(&self.setting_switch(
            app,
            "Use less memory",
            "Turns off the one graphics path the engine can do without. Across four runs of one idle tab, the browser's own memory went from 60-88 MB to a steadier 62-66 MB, and pages with many images paint a little slower. Applies the next time the browser opens.",
            prefs.low_memory,
            |app, on| {
                app.prefs.borrow_mut().low_memory = on;
                app.prefs.borrow().save();
                app.flash_toast(if on { "Less memory from the next launch" } else { "Full rendering from the next launch" });
            },
        ));

        let tabs_head = gtk::Label::new(Some("TABS"));
        crate::ui::class(&tabs_head, "group");
        tabs_head.set_xalign(0.0);
        list.add(&tabs_head);
        list.add(&self.setting_switch(
            app,
            "Darken pages that stay white",
            "Gives every page the browser's dark ground and light text, whatever the frame's own appearance — including pages that are already dark. Off by default: a page's colour is the site's to choose.",
            prefs.darken_pages,
            |app, on| {
                app.prefs.borrow_mut().darken_pages = on;
                app.prefs.borrow().save();
                app.apply_page_tone();
                app.reload();
            },
        ));

        list.add(&self.setting_switch(app, "Roomy tabs", "Taller rows, easier to hit with a mouse.", prefs.roomy_tabs, |app, on| {
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

        let search_head = gtk::Label::new(Some("SEARCH"));
        crate::ui::class(&search_head, "group");
        search_head.set_xalign(0.0);
        list.add(&search_head);

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

        // This row is about the page in front of you, not about a tab
        // setting, so it gets a head of its own rather than trailing TABS.
        let page_head = gtk::Label::new(Some("THIS PAGE"));
        crate::ui::class(&page_head, "group");
        page_head.set_xalign(0.0);
        list.add(&page_head);

        // The same helper the tab column and the curtain use, so the name here
        // is the name the sidebar would show for this tab.
        let hidden = app
            .state
            .borrow()
            .active_tab()
            .map(|t| crate::shield::host_of(&t.url))
            .unwrap_or_default();
        let count = if hidden.is_empty() {
            0
        } else {
            app.state.borrow().hidden.get(&hidden).map(|v| v.len()).unwrap_or(0)
        };
        let unhide_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        unhide_row.set_margin_top(7);
        unhide_row.set_margin_bottom(7);
        let unhide_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        unhide_text.set_hexpand(true);
        let unhide_title = gtk::Label::new(Some("Hidden here"));
        crate::ui::class(&unhide_title, "setting");
        unhide_title.set_xalign(0.0);
        let unhide_sub = gtk::Label::new(Some(&hidden_sentence(&hidden, count)));
        crate::ui::class(&unhide_sub, "settingdesc");
        unhide_sub.set_xalign(0.0);
        unhide_text.add(&unhide_title);
        unhide_text.add(&unhide_sub);
        crate::ui::pack(&unhide_row, &unhide_text, true);
        let show_all = gtk::Button::with_label("Show them");
        crate::ui::class(&show_all, "act");
        // A button that cannot do anything is not offered as if it could.
        show_all.set_sensitive(count > 0);
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

        let about = gtk::Label::new(Some(&format!(
            "Search {} · WebKitGTK · Rust · this machine only",
            env!("CARGO_PKG_VERSION")
        )));
        crate::ui::class(&about, "settingdesc");
        about.set_xalign(0.0);
        about.set_margin_top(16);
        list.add(&about);

        // Settings is intentionally compact in a small window. Its scroller
        // owns the overflow; the capture clips the viewport to the card.
        self.body.add(&self.body_scroll(&list));
        self.body.show_all();
    }

    /// Every key the browser keeps, in one table. A browser that hides its
    /// keys is a browser whose keys nobody learns.
    fn build_shortcuts(&self) {
        self.clear_body();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // The sheet is `keys::SHEET`: the same table the bench presses and the
        // tests check, so what is drawn here cannot advertise a dead key.
        for (_, key, what, ..) in crate::keys::SHEET {
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
        // The switch is drawn, not typed: there is no text in it for a screen
        // reader to read and nothing under the pointer to say what it sets, so
        // the row's name is given to it both ways. The subtitle is a sentence,
        // not a name, and is left out. The switch already reports that it is a
        // switch; only the name is missing.
        switch.set_tooltip_text(Some(title));
        if let Some(access) = switch.accessible() {
            // Fully qualified: gtk's prelude brings several traits with a
            // `set_name`, and this one is atk's.
            gtk::atk::prelude::AtkObjectExt::set_name(&access, title);
        }
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
        label.set_margin_top(22);
        label.set_margin_bottom(22);
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
        self.row_with(app, title, subtitle, action, run, None)
    }

    /// A row with a second, quieter action at the end — "Remove" beside
    /// "Open". A list you can only add to is not a list anyone keeps.
    #[allow(clippy::type_complexity)]
    fn row_with(
        &self,
        app: &AppRef,
        title: &str,
        subtitle: &str,
        action: &str,
        run: impl Fn(&AppRef) + 'static,
        second: Option<(&str, Box<dyn Fn(&AppRef)>)>,
    ) -> gtk::EventBox {
        let row = gtk::EventBox::new();
        crate::ui::class(&row, "ptrow");
        row.add_events(gtk::gdk::EventMask::BUTTON_PRESS_MASK);
        row.set_can_focus(true);
        row.set_tooltip_text(Some(title));
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

        if let Some((label, run_second)) = second {
            let other = gtk::Button::with_label(label);
            crate::ui::class(&other, "act quiet");
            other.set_valign(gtk::Align::Center);
            other.set_tooltip_text(Some(label));
            crate::ui::pack(&hbox, &other, false);
            let app_weak = app.weak();
            other.connect_clicked(move |_| {
                if let Some(app) = app_weak.upgrade() {
                    run_second(&app);
                }
            });
        }

        row.add(&hbox);

        // The whole row runs the row's action: a row that lights up under the
        // cursor and ignores the click is worse than a plain row.
        {
            let app_weak = app.weak();
            let run = std::rc::Rc::new(run);
            let run_row = run.clone();
            let app_key = app.weak();
            row.connect_key_press_event(move |_, event| {
                let name = event.keyval().name().unwrap_or_default();
                if name == "Return" || name == "KP_Enter" {
                    if let Some(app) = app_key.upgrade() { run_row(&app); }
                    return crate::gtkx::inh(true);
                }
                crate::gtkx::inh(false)
            });
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

/// What the "Hidden here" row says. A count of nothing is said plainly rather
/// than as "0 things hidden", which reads like a number nobody needs.
fn hidden_sentence(host: &str, count: usize) -> String {
    if host.is_empty() {
        return "Nothing to hide yet: this tab has no page of its own".to_string();
    }
    match count {
        0 => format!("Nothing hidden on {host}"),
        1 => format!("1 thing hidden on {host}"),
        n => format!("{n} things hidden on {host}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_count_is_told_in_words() {
        assert_eq!(hidden_sentence("example.com", 0), "Nothing hidden on example.com");
        assert_eq!(hidden_sentence("example.com", 1), "1 thing hidden on example.com");
        assert_eq!(hidden_sentence("example.com", 4), "4 things hidden on example.com");
    }

    #[test]
    fn a_tab_with_no_page_says_so() {
        assert_eq!(hidden_sentence("", 3), "Nothing to hide yet: this tab has no page of its own");
    }
}

/// What a download row says under its name.
///
/// A file name and a path is not progress. "12.4 of 40 MB" is, and when the
/// server never declared a length the honest thing is the byte count alone
/// rather than a percentage of nothing.
fn download_subtitle(download: &crate::app::DownloadRec) -> String {
    if download.failed {
        return format!("Failed · {}", download.path);
    }
    if download.done {
        let size = bytes(download.got.max(download.total));
        return if size.is_empty() {
            download.path.clone()
        } else {
            format!("{} · {}", size, download.path)
        };
    }
    let got = bytes(download.got);
    let total = bytes(download.total);
    match (got.is_empty(), total.is_empty()) {
        (true, _) => format!("Downloading to {}", download.path),
        (false, true) => format!("{got} so far"),
        (false, false) => format!("{got} of {total}"),
    }
}

/// Bytes in the units a person reads. Empty for zero, so a caller can tell
/// "no size known" from "no bytes yet".
fn bytes(n: u64) -> String {
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let n = n as f64;
    if n <= 0.0 {
        String::new()
    } else if n >= MB {
        format!("{:.1} MB", n / MB)
    } else if n >= KB {
        format!("{:.0} KB", n / KB)
    } else {
        format!("{n:.0} B")
    }
}

#[cfg(test)]
mod download_words {
    use super::{bytes, download_subtitle};
    use crate::app::DownloadRec;

    fn rec(got: u64, total: u64, done: bool, failed: bool) -> DownloadRec {
        DownloadRec {
            name: "file.pdf".into(),
            path: "/home/you/Downloads/file.pdf".into(),
            done,
            failed,
            got,
            total,
        }
    }

    #[test]
    fn a_running_download_says_how_far_along_it_is() {
        let sub = download_subtitle(&rec(13_000_000, 41_943_040, false, false));
        assert!(sub.contains(" of "), "{sub}");
        assert!(sub.contains("40.0 MB"), "{sub}");
    }

    /// A server that sends no Content-Length still gets a truthful line, not
    /// "0%" of a total nobody knows.
    #[test]
    fn without_a_length_it_counts_instead_of_guessing() {
        let sub = download_subtitle(&rec(2_097_152, 0, false, false));
        assert_eq!(sub, "2.0 MB so far");
    }

    #[test]
    fn a_failed_download_says_failed() {
        assert!(download_subtitle(&rec(0, 100, false, true)).starts_with("Failed"));
    }

    #[test]
    fn a_finished_download_shows_its_size_and_where_it_is() {
        let sub = download_subtitle(&rec(41_943_040, 41_943_040, true, false));
        assert!(sub.starts_with("40.0 MB · "), "{sub}");
        assert!(sub.ends_with("file.pdf"));
    }

    #[test]
    fn the_units_are_the_ones_people_read() {
        assert_eq!(bytes(0), "");
        assert_eq!(bytes(512), "512 B");
        assert_eq!(bytes(2048), "2 KB");
        assert_eq!(bytes(5 * 1024 * 1024), "5.0 MB");
    }
}
