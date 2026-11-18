// The browser itself: the tabs, what is on screen, and every action a key, a
// click or a page can ask for.
use crate::curtain;
use crate::find::FindBar;
use crate::omnibox::{Offer, Omnibox};
use crate::palette::Palette;
use crate::panels::{PanelKind, Panels};
use crate::store::{Records, TabRecord};
use crate::ui::{self, TabRow, Ui};
use crate::web;
use gtk::gdk::prelude::*;
use gtk::prelude::*;
use webkit2gtk::{FindControllerExt, UserContentManagerExt, WebInspectorExt, WebViewExt};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

pub type TabId = u64;
pub type AppRef = Rc<App>;

thread_local! {
    static CURRENT: RefCell<Option<AppRef>> = const { RefCell::new(None) };
    static FLUSH_PENDING: Cell<bool> = const { Cell::new(false) };
}

static HISTORY_PENDING: AtomicBool = AtomicBool::new(false);
static SESSION_PENDING: AtomicBool = AtomicBool::new(false);

pub fn current() -> Option<AppRef> {
    CURRENT.with(|c| c.borrow().clone())
}

pub struct Tab {
    pub id: TabId,
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub private: bool,
    pub loading: bool,
    pub can_back: bool,
    pub can_forward: bool,
    pub failed: bool,
    pub asleep: bool,
    pub row: Option<TabRow>,
    pub pin: Option<ui::PinWidget>,
    pub last_seen: u64,
}

impl Tab {
    pub fn letter(&self) -> char {
        if let Some(first) = self.title.chars().find(|c| c.is_alphanumeric()) {
            return first;
        }
        let host = crate::shield::host_of(&self.url);
        let text = if host.is_empty() { "New tab" } else { host.as_str() };
        text.chars().find(|c| c.is_alphanumeric()).unwrap_or('N')
    }

    pub fn new(id: TabId) -> Tab {
        Tab {
            id,
            url: String::new(),
            title: String::new(),
            pinned: false,
            private: false,
            loading: false,
            can_back: false,
            can_forward: false,
            failed: false,
            asleep: false,
            row: None,
            pin: None,
            last_seen: crate::store::now_secs(),
        }
    }
}

#[derive(Clone)]
pub struct DownloadRec {
    pub name: String,
    pub path: String,
    pub done: bool,
}

pub struct ErrorPage {
    pub reason: String,
    pub url: String,
}

pub struct State {
    pub tabs: Vec<Tab>,
    pub errors: BTreeMap<TabId, ErrorPage>,
    pub active: TabId,
    pub next_id: TabId,
    pub history: Vec<crate::store::HistoryEntry>,
    pub bookmarks: Vec<crate::store::Bookmark>,
    pub hidden: BTreeMap<String, Vec<String>>,
    pub downloads: Vec<DownloadRec>,
    pub hide_mode: bool,
}

impl State {
    pub fn position(&self, id: TabId) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }
    pub fn tab(&self, id: TabId) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.id == id)
    }
    pub fn tab_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.id == id)
    }
    pub fn active_tab(&self) -> Option<&Tab> {
        self.tab(self.active)
    }
}

pub struct App {
    pub ui: Ui,
    pub prefs: RefCell<crate::store::Prefs>,
    pub state: RefCell<State>,
    pub omni: Omnibox,
    pub panels: RefCell<Panels>,
    pub find: RefCell<FindBar>,
    pub provider: gtk::CssProvider,
    pub private_ctx: RefCell<Option<webkit2gtk::WebContext>>,
    pub palette: RefCell<Palette>,
    pub last_layout: RefCell<(i32, i32)>,
    /// Reading mode is a state of the tab, not of the page, so the browser has
    /// to remember it to be able to leave it.
    pub reading: std::cell::Cell<bool>,
}

pub fn start(mut prefs: crate::store::Prefs) -> AppRef {
    let window = gtk::Window::new(gtk::WindowType::Toplevel);
    window.set_title("Search");
    window.set_decorated(true);
    // Client-side decorations, with a title bar of no height at all: the
    // window draws itself, and the desktop's own bar never appears.
    let bar = gtk::HeaderBar::new();
    bar.set_show_close_button(false);
    bar.set_has_subtitle(false);
    crate::ui::class(&bar, "nobar");
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_size_request(-1, 0);
    bar.set_custom_title(Some(&spacer));
    bar.set_size_request(-1, 0);
    window.set_titlebar(Some(&bar));
    window.set_default_size(1180, 760);
    window.set_position(gtk::WindowPosition::Center);

    prefs.desktop_dark = gtk::Settings::default()
        .map(|s| s.is_gtk_application_prefer_dark_theme())
        .unwrap_or(false);

    let palette = Palette::for_look(prefs.appearance.clone(), prefs.desktop_dark);
    let composited = gtk::prelude::GtkWindowExt::screen(&window).map(|s| s.is_composited()).unwrap_or(true);
    // One opaque window colour, so a strip the shell does not cover is the
    // window's own ground rather than whatever the compositor shows through.
    window.set_app_paintable(true);

    let ui = ui::build(window.clone(), palette, &prefs, composited);
    window.add(&ui.overlay);

    let provider = gtk::CssProvider::new();
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let mut tabs: Vec<Tab> = Vec::new();
    let mut next_id: TabId = 1;
    if prefs.restore_session {
        for record in Records::load_session() {
            let mut tab = Tab::new(next_id);
            tab.url = record.url;
            tab.title = record.title;
            tab.pinned = record.pinned;
            tab.asleep = true;
            tabs.push(tab);
            next_id += 1;
        }
    }
    if tabs.is_empty() {
        tabs.push(Tab::new(next_id));
        next_id += 1;
    }
    let active = tabs[0].id;

    let state = State {
        errors: BTreeMap::new(),
        tabs,
        active,
        next_id,
        history: Records::load_history(),
        bookmarks: Records::load_bookmarks(),
        hidden: Records::load_hidden(),
        downloads: Vec::new(),
        hide_mode: false,
    };

    let prefs_side_width = prefs.side_width.round() as i32;
    let omni = Omnibox::new(&palette);
    let panels = Panels::new(&palette);
    let find = FindBar::new(&palette);
    ui.overlay.add_overlay(&omni.holder);
    ui.overlay.add_overlay(&find.holder);
    ui.overlay.add_overlay(&panels.holder);
    omni.holder.hide();
    find.holder.hide();
    panels.holder.hide();

    let app = Rc::new(App {
        ui,
        prefs: RefCell::new(prefs),
        state: RefCell::new(state),
        omni,
        panels: RefCell::new(panels),
        find: RefCell::new(find),
        provider,
        private_ctx: RefCell::new(None),
        palette: RefCell::new(palette),
        last_layout: RefCell::new((0, 0)),
        reading: std::cell::Cell::new(false),
    });

    app.ui.shell.set_position(prefs_side_width);
    app.wire();
    app.apply_style();
    app.render();
    app.start_memory_timer();
    app
}

impl App {
    pub fn weak(&self) -> Weak<App> {
        CURRENT.with(|c| c.borrow().as_ref().map(Rc::downgrade).unwrap_or_default())
    }

    pub fn dark(&self) -> bool {
        self.palette.borrow().is_dark()
    }

    pub fn apply_style(&self) {
        let css = crate::css::build(
            &self.palette.borrow(),
            self.composited(),
            self.prefs.borrow().row_height(),
        );
        if let Err(error) = self.provider.load_from_data(css.as_bytes()) {
            eprintln!("search: the stylesheet did not load: {error}");
        }
        // Pages follow the same appearance the frame does, which WebKit takes
        // from the toolkit's own preference.
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(self.dark());
        }
        for widget in [self.ui.shell.clone().upcast::<gtk::Widget>(), self.ui.window.clone().upcast::<gtk::Widget>()] {
            widget.style_context().remove_class("dark");
            widget.style_context().remove_class("light");
            widget.style_context().add_class(if self.dark() { "dark" } else { "light" });
        }
    }

    fn composited(&self) -> bool {
        gtk::prelude::GtkWindowExt::screen(&self.ui.window).map(|s| s.is_composited()).unwrap_or(false)
    }

    pub fn switch_look(&self, look: crate::store::Look) {
        {
            let mut prefs = self.prefs.borrow_mut();
            prefs.appearance = look.clone();
            prefs.save();
        }
        let palette = Palette::for_look(look, self.prefs.borrow().desktop_dark);
        *self.palette.borrow_mut() = palette;
        self.apply_style();
        self.apply_page_tone();
        // A page already open keeps the background it was made with unless it
        // is told the appearance changed.
        let page = if palette.is_dark() { (0.043, 0.043, 0.043) } else { (1.0, 1.0, 1.0) };
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            if let Some(view) = self.view(id) {
                view.set_background_color(&gtk::gdk::RGBA::new(page.0, page.1, page.2, 1.0));
            }
        }
        self.render();
    }

    pub fn private_context(&self) -> webkit2gtk::WebContext {
        if let Some(context) = self.private_ctx.borrow().as_ref() {
            return context.clone();
        }
        let context = web::ephemeral_context();
        *self.private_ctx.borrow_mut() = Some(context.clone());
        context
    }

    // ---- tabs -----------------------------------------------------------

    pub fn active_id(&self) -> TabId {
        self.state.borrow().active
    }

    pub fn new_tab(&self, url: Option<&str>, private: bool, background: bool) -> TabId {
        let id = {
            let mut state = self.state.borrow_mut();
            let id = state.next_id;
            state.next_id += 1;
            let mut tab = Tab::new(id);
            tab.private = private;
            tab.url = url.unwrap_or("").to_string();
            tab.asleep = url.is_none();
            let at = state.position(state.active).map(|p| p + 1).unwrap_or(state.tabs.len());
            state.tabs.insert(at, tab);
            id
        };
        if background {
            self.render();
        } else {
            self.select(id);
        }
        match url {
            Some(url) => self.navigate(id, url),
            None => self.open_omni(self.state.borrow().active_tab().map(|t| t.url.is_empty()).unwrap_or(true)),
        }
        id
    }

    pub fn select(&self, id: TabId) {
        if self.state.borrow().position(id).is_none() {
            return;
        }
        self.state.borrow_mut().active = id;
        self.ensure_view(id);
        self.ui.stack.set_visible_child_name(&format!("tab-{id}"));
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.asleep = false;
            tab.last_seen = crate::store::now_secs();
        }
        if let Some(view) = self.view(id) {
            view.grab_focus();
        }
        self.render();
        self.schedule_session();
    }

    pub fn close_tab(&self, id: TabId) {
        let (index, was_active) = {
            let state = self.state.borrow();
            match state.position(id) {
                Some(index) => (index, state.active == id),
                None => return,
            }
        };
        if let Some(view) = self.view(id) {
            self.ui.stack.remove(&view);
        }
        let next_active = {
            let mut state = self.state.borrow_mut();
            state.tabs.remove(index);
            if state.tabs.is_empty() {
                let next = state.next_id;
                state.next_id += 1;
                state.tabs.push(Tab::new(next));
            }
            if was_active {
                state.active = state.tabs[index.min(state.tabs.len() - 1)].id;
            }
            state.active
        };
        self.select(next_active);
    }

    pub fn close_tab_of_view(&self, view: &webkit2gtk::WebView) -> bool {
        if let Some(id) = self.id_of_page(view) {
            self.close_tab(id);
        }
        true
    }

    pub fn view(&self, id: TabId) -> Option<webkit2gtk::WebView> {
        let name = format!("page-{id}");
        for child in self.ui.stack.children() {
            if child.widget_name() == name.as_str() {
                if let Ok(view) = child.downcast::<webkit2gtk::WebView>() {
                    return Some(view);
                }
            }
        }
        None
    }

    pub fn id_of_page(&self, view: &webkit2gtk::WebView) -> Option<TabId> {
        view.widget_name().strip_prefix("page-")?.parse().ok()
    }

    fn ensure_view(&self, id: TabId) {
        if self.view(id).is_some() {
            return;
        }
        let Some(app) = current() else { return };
        let Some(view) = web::view_for(&app, id) else { return };
        view.set_widget_name(&format!("page-{id}"));
        let url = self.state.borrow().tab(id).map(|t| t.url.clone()).unwrap_or_default();
        self.ui.stack.add_named(&view, &format!("tab-{id}"));
        view.show_all();
        // A view made for a tab in the background is made off screen: adding it
        // to the stack would otherwise put it on top of the page being read.
        if id == self.active_id() {
            self.ui.stack.set_visible_child(&view);
        }
        if !url.is_empty() {
            web::load(&view, &url);
        }
    }

    pub fn render(&self) {
        self.render_pins();
        self.render_rows();
        self.refresh_helm();
        let (blank, has_error) = {
            let state = self.state.borrow();
            (
                state.active_tab().map(|t| t.url.is_empty()).unwrap_or(true),
                state.errors.contains_key(&state.active),
            )
        };
        if has_error {
            let active = self.state.borrow().active;
            if let Some(error) = self.state.borrow().errors.get(&active) {
                self.ui.error_reason.set_text(&error.reason);
                self.ui.error_address.set_text(&error.url);
            }
            self.ui.errorpage.show_all();
            self.ui.stack.set_visible_child_name("error");
        } else if blank {
            self.ui.stack.set_visible_child_name("blank");
        }
    }

    fn render_rows(&self) {
        for child in self.ui.rows.children() {
            self.ui.rows.remove(&child);
        }
        let (tabs, active) = {
            let state = self.state.borrow();
            let tabs: Vec<Tab> = state
                .tabs
                .iter()
                .map(|t| Tab {
                    id: t.id,
                    url: t.url.clone(),
                    title: t.title.clone(),
                    pinned: t.pinned,
                    private: t.private,
                    loading: t.loading,
                    can_back: t.can_back,
                    can_forward: t.can_forward,
                    failed: t.failed,
                    asleep: t.asleep,
                    row: None,
                    pin: None,
                    last_seen: t.last_seen,
                })
                .collect();
            (tabs, state.active)
        };
        let palette = *self.palette.borrow();
        let height = self.prefs.borrow().row_height();
        for tab in tabs.iter().filter(|t| !t.pinned) {
            let title = if tab.title.is_empty() {
                if tab.url.is_empty() {
                    "New tab".to_string()
                } else {
                    crate::shield::host_of(&tab.url)
                }
            } else {
                tab.title.clone()
            };
            let row = ui::tab_row(&title, palette, height, Some(tab.letter()), tab.id == active);
            row.event.set_tooltip_text(Some(&if tab.url.is_empty() {
                "New tab".to_string()
            } else {
                format!("{}\n{}", if title.is_empty() { "Untitled" } else { title.as_str() }, tab.url)
            }));
            {
                let weak = self.weak();
                let id = tab.id;
                row.event.connect_button_press_event(move |_, event| {
                    let Some(app) = weak.upgrade() else { return crate::gtkx::inh(false) };
                    if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
                        match button.button() {
                            1 => app.select(id),
                            2 => app.close_tab(id),
                            3 => app.tab_menu(id, event),
                            _ => {}
                        }
                    }
                    crate::gtkx::inh(false)
                });
            }
            {
                let weak = self.weak();
                let id = tab.id;
                row.close.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        app.close_tab(id);
                    }
                });
            }
            ui::pack(&self.ui.rows, &row.event, false);
            if let Some(slot) = self.state.borrow_mut().tab_mut(tab.id) {
                slot.row = Some(row);
            }
        }
        // The row that makes another.
        let new_tab = gtk::EventBox::new();
        crate::ui::class(&new_tab, "newtab");
        new_tab.set_size_request(-1, height);
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        hbox.set_margin_start(8);
        let plus = crate::icons::area(crate::icons::Icon::Plus, 16, palette.faint);
        plus.set_valign(gtk::Align::Center);
        ui::pack(&hbox, &plus, false);
        let label = gtk::Label::new(Some("New tab"));
        crate::ui::class(&label, "newtablabel");
        label.set_xalign(0.0);
        label.set_valign(gtk::Align::Center);
        ui::pack(&hbox, &label, true);
        new_tab.add(&hbox);
        {
            let weak = self.weak();
            new_tab.connect_button_press_event(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    app.new_tab(None, false, false);
                }
                crate::gtkx::inh(true)
            });
        }
        ui::pack(&self.ui.rows, &new_tab, false);
        self.ui.rows.show_all();
    }

    fn render_pins(&self) {
        for child in self.ui.pins.children() {
            self.ui.pins.remove(&child);
        }
        let (pinned, active): (Vec<(TabId, char, String)>, TabId) = {
            let state = self.state.borrow();
            let pinned = state
                .tabs
                .iter()
                .filter(|t| t.pinned)
                .map(|t| (t.id, t.letter(), if t.title.is_empty() { t.url.clone() } else { t.title.clone() }))
                .collect();
            (pinned, state.active)
        };
        if pinned.is_empty() {
            self.ui.pins_holder.hide();
            self.ui.pins_holder.set_no_show_all(true);
            return;
        }
        let palette = *self.palette.borrow();
        // A widget that opts out of show_all is not shown by it, even when it
        // is the one being asked: the flag comes off first.
        self.ui.pins_holder.set_no_show_all(false);
        self.ui.pins_holder.show_all();
        for (index, (id, letter, title)) in pinned.iter().enumerate() {
            let tile = ui::pin_tile(*letter, palette, *id == active);
            tile.event.set_tooltip_text(Some(title));
            {
                let weak = self.weak();
                let id = *id;
                tile.event.connect_button_press_event(move |_, event| {
                    let Some(app) = weak.upgrade() else { return crate::gtkx::inh(false) };
                    if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
                        match button.button() {
                            1 => app.select(id),
                            2 => app.close_tab(id),
                            _ => {}
                        }
                    }
                    crate::gtkx::inh(false)
                });
            }
            self.ui.pins.attach(&tile.event, (index % 6) as i32, (index / 6) as i32, 1, 1);
            if let Some(slot) = self.state.borrow_mut().tab_mut(*id) {
                slot.pin = Some(tile);
            }
        }
        self.ui.pins.show_all();
    }

    pub fn refresh_helm(&self) {
        let (back, forward, url) = self
            .state
            .borrow()
            .active_tab()
            .map(|t| (t.can_back, t.can_forward, t.url.clone()))
            .unwrap_or((false, false, String::new()));
        self.ui.back.set_sensitive(back);
        self.ui.forward.set_sensitive(forward);
        // The three doors only show when there is somewhere to go: an empty
        // tab has no back, and the row is quieter without them.
        let show = back || forward || !url.is_empty();
        self.ui.helm.set_sensitive(show);
    }

    // ---- pages ----------------------------------------------------------

    pub fn set_title(&self, id: TabId, title: &str) {
        let changed = {
            let mut state = self.state.borrow_mut();
            match state.tab_mut(id) {
                Some(tab) if tab.title != title => {
                    tab.title = title.to_string();
                    true
                }
                _ => false,
            }
        };
        if changed {
            self.update_row_title(id);
            self.save_session();
        }
    }

    fn update_row_title(&self, id: TabId) {
        let mut state = self.state.borrow_mut();
        let Some(tab) = state.tab_mut(id) else { return };
        let title = if tab.title.is_empty() {
            if tab.url.is_empty() {
                "New tab".to_string()
            } else {
                crate::shield::host_of(&tab.url)
            }
        } else {
            tab.title.clone()
        };
        let letter = tab.letter().to_uppercase().to_string();
        if let Some(row) = tab.row.as_ref() {
            row.title.set_text(&title);
            row.letter.set_text(&letter);
        }
        if let Some(pin) = tab.pin.as_ref() {
            pin.letter.set_text(&letter);
        }
    }

    pub fn set_uri(&self, id: TabId, uri: &str) {
        let changed = {
            let mut state = self.state.borrow_mut();
            match state.tab_mut(id) {
                Some(tab) if tab.url != uri => {
                    tab.url = uri.to_string();
                    true
                }
                _ => false,
            }
        };
        if changed {
            curtain::apply_for_tab(self, id);
            self.update_row_title(id);
        }
    }

    pub fn set_loading(&self, id: TabId, loading: bool) {
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.loading = loading;
        }
    }

    pub fn sync_nav(&self, id: TabId, view: &webkit2gtk::WebView) {
        let (back, forward, loading) = (view.can_go_back(), view.can_go_forward(), view.is_loading());
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.can_back = back;
            tab.can_forward = forward;
            tab.loading = loading;
        }
        self.refresh_helm();
    }

    pub fn record_visit(&self, id: TabId) {
        let (url, title) = {
            let state = self.state.borrow();
            match state.tab(id) {
                Some(tab) => (tab.url.clone(), tab.title.clone()),
                None => return,
            }
        };
        if url.is_empty() {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            Records::record_visit(&mut state.history, &url, &title);
        }
        self.schedule_flush();
    }

    fn schedule_flush(&self) {
        if HISTORY_PENDING.swap(true, Ordering::Relaxed) {
            return;
        }
        let weak = self.weak();
        glib::timeout_add_seconds_local(2, move || {
            HISTORY_PENDING.store(false, Ordering::Relaxed);
            if let Some(app) = weak.upgrade() {
                let (history, hidden) = {
                    let state = app.state.borrow();
                    (state.history.clone(), state.hidden.clone())
                };
                Records::save_history(&history);
                Records::save_hidden(&hidden);
                app.save_session();
            }
            glib::ControlFlow::Break
        });
    }

    pub fn navigate(&self, id: TabId, target: &str) {
        if target.is_empty() {
            return;
        }
        let url = resolve(self, target);
        if url.is_empty() {
            return;
        }
        self.ensure_view(id);
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.url = url.clone();
            tab.asleep = false;
        }
        curtain::apply_for_tab(self, id);
        if let Some(view) = self.view(id) {
            web::load(&view, &url);
            // A background tab loads without taking the screen: the tab bar
            // says which tab is on top, and it must agree with what is shown.
            if id == self.active_id() {
                self.ui.stack.set_visible_child_name(&format!("tab-{id}"));
                view.grab_focus();
            }
        }
        self.update_row_title(id);
        // Nothing is being typed at a page that just arrived: a field left open
        // over it hides the very page that was just asked for.
        if self.omni.visible.get() && self.omni.entry.text() != url {
            self.close_omni();
        }
        self.save_session();
    }

    pub fn navigate_active(&self, target: &str) {
        let id = self.active_id();
        self.navigate(id, target);
    }

    pub fn go_back(&self) {
        if let Some(view) = self.view(self.active_id()) {
            view.go_back();
        }
    }

    pub fn go_forward(&self) {
        if let Some(view) = self.view(self.active_id()) {
            view.go_forward();
        }
    }

    pub fn reload(&self) {
        let id = self.active_id();
        match self.view(id) {
            Some(view) => view.reload(),
            None => {
                let url = self.state.borrow().tab(id).map(|t| t.url.clone()).unwrap_or_default();
                self.navigate(id, &url);
            }
        }
    }


    pub fn toggle_pin(&self, id: TabId) {
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.pinned = !tab.pinned;
        }
        self.render();
        self.save_session();
    }

    pub fn duplicate(&self, id: TabId) {
        let url = self.state.borrow().tab(id).map(|t| t.url.clone()).unwrap_or_default();
        if url.is_empty() {
            self.new_tab(None, false, false);
        } else {
            self.new_tab(Some(&url), false, true);
        }
    }

    pub fn copy_address(&self) {
        let url = self.state.borrow().active_tab().map(|t| t.url.clone()).unwrap_or_default();
        let clipboard = gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD);
        clipboard.set_text(&url);
        clipboard.store();
        self.flash_toast("Address copied");
    }

    pub fn cycle(&self, forwards: bool) {
        let (ids, active) = {
            let state = self.state.borrow();
            (state.tabs.iter().map(|t| t.id).collect::<Vec<_>>(), state.active)
        };
        if ids.len() < 2 {
            return;
        }
        let index = ids.iter().position(|id| *id == active).unwrap_or(0);
        let next = if forwards { (index + 1) % ids.len() } else { (index + ids.len() - 1) % ids.len() };
        self.select(ids[next]);
    }

    pub fn jump(&self, number: usize) {
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|t| t.id).collect();
        if number == 9 {
            if let Some(last) = ids.last() {
                self.select(*last);
            }
        } else if let Some(id) = ids.get(number.saturating_sub(1)) {
            self.select(*id);
        }
    }

    pub fn reopen_closed(&self) {
        let entry = {
            let state = self.state.borrow();
            let open: Vec<String> = state.tabs.iter().map(|t| t.url.clone()).collect();
            // Read, not taken: bringing a tab back is not a reason to forget
            // that the page was ever visited.
            let index = state.history.iter().position(|e| !open.contains(&e.url));
            index.map(|i| state.history[i].clone())
        };
        match entry {
            Some(entry) => {
                self.new_tab(Some(&entry.url), false, true);
            }
            None => self.flash_toast("Nothing to bring back"),
        }
    }

    /// Tabs that have not been looked at let go of their page. The tab keeps
    /// its address and title; the page is made again when it is selected.
    pub fn sleep_others(&self) {
        let (enabled, minutes) = {
            let prefs = self.prefs.borrow();
            (prefs.memory_saver, prefs.sleep_minutes as u64)
        };
        if !enabled {
            return;
        }
        let now = crate::store::now_secs();
        let active = self.active_id();
        let sleeping: Vec<TabId> = {
            let state = self.state.borrow();
            state
                .tabs
                .iter()
                .filter(|tab| {
                    tab.id != active
                        && !tab.asleep
                        && !tab.loading
                        && now.saturating_sub(tab.last_seen) > minutes * 60
                })
                .map(|tab| tab.id)
                .collect()
        };
        if sleeping.is_empty() {
            return;
        }
        for id in &sleeping {
            if let Some(view) = self.view(*id) {
                self.ui.stack.remove(&view);
            }
            if let Some(tab) = self.state.borrow_mut().tab_mut(*id) {
                tab.asleep = true;
            }
        }
        self.render();
    }

    /// The tab-sleeping promise, kept: a minute's tick drops the pages of tabs
    /// that have not been looked at for as long as the setting says.
    pub fn start_memory_timer(&self) {
        let weak = self.weak();
        glib::timeout_add_seconds_local(60, move || {
            let Some(app) = weak.upgrade() else { return glib::ControlFlow::Break };
            app.sleep_others();
            glib::ControlFlow::Continue
        });
    }

    // ---- the field, panels, find ---------------------------------------

    pub fn open_omni(&self, over: bool) {
        let offers = self.offers("");
        self.omni.show(&self.ui, over, offers);
        let entry = self.omni.entry.clone();
        glib::idle_add_local(move || {
            entry.grab_focus();
            // The address comes up selected, so typing replaces it and Return
            // goes where the field already says.
            entry.select_region(0, -1);
            glib::ControlFlow::Break
        });
    }

    pub fn close_omni(&self) {
        self.omni.hide(&self.ui);
        if let Some(view) = self.view(self.active_id()) {
            view.grab_focus();
        }
    }

    pub fn offers(&self, typed: &str) -> Vec<Offer> {
        let mut offers: Vec<Offer> = Vec::new();
        let typed_lower = typed.to_lowercase();
        let state = self.state.borrow();
        for tab in &state.tabs {
            if tab.url.is_empty() || offers.len() >= 3 {
                continue;
            }
            let hay = format!("{} {}", tab.title, tab.url).to_lowercase();
            if typed_lower.is_empty() || hay.contains(&typed_lower) {
                offers.push(Offer {
                    title: if tab.title.is_empty() { tab.url.clone() } else { tab.title.clone() },
                    url: tab.url.clone(),
                    kind: "Open".into(),
                });
            }
        }
        for entry in &state.history {
            if offers.len() >= 8 {
                break;
            }
            let hay = format!("{} {}", entry.title, entry.url).to_lowercase();
            // One row per address: a page visited twice is not two suggestions.
            if (typed_lower.is_empty() || hay.contains(&typed_lower)) && !offers.iter().any(|o| o.url == entry.url) {
                offers.push(Offer {
                    title: if entry.title.is_empty() {
                        crate::shield::host_of(&entry.url)
                    } else {
                        entry.title.clone()
                    },
                    url: entry.url.clone(),
                    kind: "History".into(),
                });
            }
        }
        for bookmark in &state.bookmarks {
            if offers.len() >= 10 {
                break;
            }
            let hay = format!("{} {}", bookmark.title, bookmark.url).to_lowercase();
            if (typed_lower.is_empty() || hay.contains(&typed_lower)) && !offers.iter().any(|o| o.url == bookmark.url) {
                offers.push(Offer {
                    title: bookmark.title.clone(),
                    url: bookmark.url.clone(),
                    kind: "Kept".into(),
                });
            }
        }
        offers.truncate(8);
        offers
    }

    pub fn refresh_offers(&self, typed: &str) {
        let offers = self.offers(typed);
        if let Some(app) = current() {
            self.omni.set_offers(&self.ui, &app, offers);
        }
    }

    /// When dark pages are asked for, a site that paints white gets the
    /// browser's ground instead, and its text a colour that can be read on it.
    pub fn apply_page_tone(&self) {
        let on = self.prefs.borrow().darken_pages && self.dark();
        let sheet = if on {
            "html { background: #0b0b0b !important; }\nbody { background-color: transparent !important; }\n:root { color-scheme: dark; }"
        } else {
            ""
        };
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            if let Some(view) = self.view(id) {
                if let Some(manager) = view.user_content_manager() {
                    manager.remove_all_style_sheets();
                    if !sheet.is_empty() {
                        let style = webkit2gtk::UserStyleSheet::new(
                            sheet,
                            webkit2gtk::UserContentInjectedFrames::AllFrames,
                            webkit2gtk::UserStyleLevel::User,
                            &[],
                            &[],
                        );
                        manager.add_style_sheet(&style);
                    }
                }
            }
        }
    }

    /// Where the field, the panels and find stand in a window of this size.
    pub fn layout_layers(&self, width: i32, height: i32) {
        // Setting a size request from inside an allocation pass asks for
        // another pass: only do it when the window really changed size.
        let changed = {
            let mut last = self.last_layout.borrow_mut();
            if *last == (width, height) {
                false
            } else {
                *last = (width, height);
                true
            }
        };
        if changed {
            self.omni.holder.set_size_request(width, height);
            self.panels.borrow().holder.set_size_request(width, height);
            self.find.borrow().holder.set_size_request(width, height);
        }
        let column = self.ui.side.allocated_width().max(0);
        self.omni.layout_with_column(width, height, column);
        self.panels.borrow().layout_with_column(width, height, column);
        self.find.borrow().layout(width);
    }


    /// A search inside a panel re-opens it with the filter applied.
    ///
    /// Setting the field back to what it already says is not a change, and
    /// acting on it would rebuild the panel inside the borrow that is
    /// rebuilding it. Empty is a real change: clearing the words shows
    /// everything again.
    pub fn filter_panel(&self, kind: PanelKind, text: &str) {
        let wanted = text.to_lowercase();
        // A panel is rebuilding: the change that arrives is its own field
        // being given back what was typed, not a new search.
        let Ok(panels) = self.panels.try_borrow() else {
            return;
        };
        if wanted == *panels.filter.borrow() {
            return;
        }
        drop(panels);
        if let Some(app) = current() {
            self.panels.borrow_mut().set_filter(&app, kind, text);
        }
    }

    pub fn toggle_panel(&self, kind: PanelKind) {
        if self.panels.borrow().open.as_ref() == Some(&kind) {
            self.close_panels();
            return;
        }
        self.open_panel(kind);
    }

    pub fn open_panel(&self, kind: PanelKind) {
        if let Some(app) = current() {
            self.panels.borrow_mut().open_panel(&app, kind);
        }
    }

    pub fn toggle_panel_switch(&self, kind: PanelKind) {
        self.open_panel(kind);
    }

    pub fn close_panels(&self) {
        self.panels.borrow_mut().close(&self.ui);
    }

    pub fn toggle_find(&self) {
        let visible = self.find.borrow().visible;
        if visible {
            self.find.borrow_mut().hide(&self.ui);
            return;
        }
        {
            let mut find = self.find.borrow_mut();
            find.show(&self.ui);
            if let Some(view) = self.view(self.active_id()) {
                find.attach(&view);
            }
            find.focus();
        }
    }

    /// One step through the matches, in the direction asked for.
    pub fn find_step(&self, text: &str, forward: bool) {
        let Some(view) = self.view(self.active_id()) else { return };
        let Some(controller) = view.find_controller() else { return };
        if text.is_empty() {
            controller.search("", 0, 0);
            return;
        }
        if forward {
            controller.search_next();
        } else {
            controller.search_previous();
        }
    }

    pub fn find_query(&self, text: &str, _forward: bool) {
        let Some(view) = self.view(self.active_id()) else { return };
        let Some(controller) = view.find_controller() else { return };
        if text.is_empty() {
            controller.search("", 0, 0);
            controller.count_matches("", 0, 0);
        } else {
            let options = webkit2gtk::FindOptions::CASE_INSENSITIVE.bits();
            controller.search(text, options, 200);
            controller.count_matches(text, options, 20_000);
        }
    }

    pub fn add_bookmark(&self) {
        let (url, title) = {
            let state = self.state.borrow();
            match state.active_tab() {
                Some(tab) => (tab.url.clone(), tab.title.clone()),
                None => return,
            }
        };
        if url.is_empty() {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            if state.bookmarks.iter().any(|b| b.url == url) {
                state.bookmarks.retain(|b| b.url != url);
                self.flash_toast("No longer kept");
            } else {
                state.bookmarks.insert(
                    0,
                    crate::store::Bookmark {
                        url: url.clone(),
                        title: if title.is_empty() { url } else { title },
                    },
                );
                self.flash_toast("Kept");
            }
            Records::save_bookmarks(&state.bookmarks);
        }
        if self.panels.borrow().open.clone() == Some(PanelKind::Bookmarks) {
            self.open_panel(PanelKind::Bookmarks);
        }
    }

    pub fn reader(&self) {
        self.reading.set(!self.reading.get());
        let Some(app) = current() else { return };
        let Some(view) = self.view(self.active_id()) else { return };
        crate::reader::enter(&app, &view);
    }

    pub fn hide_element_mode(&self) {
        let on = !self.state.borrow().hide_mode;
        self.state.borrow_mut().hide_mode = on;
        if let Some(view) = self.view(self.active_id()) {
            curtain::pick(self, &view, on);
        }
        self.flash_toast(if on { "Click what should go" } else { "Done hiding" });
    }

    pub fn hide_selector(&self, host: &str, selector: &str) {
        if selector.is_empty() {
            return;
        }
        {
            let mut state = self.state.borrow_mut();
            let list = state.hidden.entry(host.to_string()).or_default();
            if !list.iter().any(|s| s == selector) {
                list.push(selector.to_string());
            }
            Records::save_hidden(&state.hidden);
        }
        if let Some(view) = self.view(self.active_id()) {
            curtain::apply_immediately(&view, self, host);
        }
        self.flash_toast("Hidden here for good");
    }

    pub fn unhide_all(&self) {
        let host = self
            .state
            .borrow()
            .active_tab()
            .map(|t| crate::shield::host_of(&t.url))
            .unwrap_or_default();
        {
            let mut state = self.state.borrow_mut();
            state.hidden.remove(&host);
            Records::save_hidden(&state.hidden);
        }
        self.reload();
        self.flash_toast("Nothing is hidden here now");
    }

    pub fn zoom(&self, delta: f64) {
        if let Some(view) = self.view(self.active_id()) {
            let next = (view.zoom_level() + delta).clamp(0.4, 3.0);
            view.set_zoom_level(next);
        }
    }

    pub fn reset_zoom(&self) {
        if let Some(view) = self.view(self.active_id()) {
            view.set_zoom_level(1.0);
        }
    }

    pub fn inspector(&self) {
        if let Some(view) = self.view(self.active_id()) {
            if let Some(inspector) = view.inspector() {
                inspector.show();
            }
        }
    }


    // ---- session --------------------------------------------------------

    /// Session writes are debounced: a tab switch should cost a repaint, not a
    /// JSON file written to disk on the way.
    pub fn schedule_session(&self) {
        if SESSION_PENDING.swap(true, Ordering::Relaxed) {
            return;
        }
        let weak = self.weak();
        glib::timeout_add_seconds_local(2, move || {
            SESSION_PENDING.store(false, Ordering::Relaxed);
            if let Some(app) = weak.upgrade() {
                app.save_session();
            }
            glib::ControlFlow::Break
        });
    }

    pub fn save_session(&self) {
        if !self.prefs.borrow().restore_session {
            return;
        }
        let records: Vec<TabRecord> = self
            .state
            .borrow()
            .tabs
            .iter()
            .filter(|t| !t.private && !t.url.is_empty())
            .map(|t| TabRecord { url: t.url.clone(), title: t.title.clone(), pinned: t.pinned })
            .collect();
        Records::save_session(&records);
    }

    // ---- menus and toasts ----------------------------------------------

    pub fn tab_menu(&self, id: TabId, event: &gtk::gdk::Event) {
        let menu = gtk::Menu::new();
        let pin_label = self
            .state
            .borrow()
            .tab(id)
            .map(|t| if t.pinned { "Unpin Tab" } else { "Pin Tab" })
            .unwrap_or("Pin Tab");
        let items: Vec<(&str, Box<dyn Fn(&App)>)> = vec![
            (pin_label, Box::new(move |app: &App| app.toggle_pin(id))),
            ("Duplicate Tab", Box::new(move |app: &App| app.duplicate(id))),
            ("Close Tab", Box::new(move |app: &App| app.close_tab(id))),
        ];
        for (label, action) in items {
            let item = gtk::MenuItem::with_label(label);
            let weak = self.weak();
            item.connect_activate(move |_| {
                if let Some(app) = weak.upgrade() {
                    action(&app);
                }
            });
            menu.append(&item);
        }
        menu.show_all();
        if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
            menu.popup_easy(button.button(), button.time());
        }
    }

    pub fn main_menu(&self) {
        let menu = gtk::Menu::new();
        let items: Vec<(&str, Box<dyn Fn(&App)>)> = vec![
            ("New Tab", Box::new(|app: &App| {
                app.new_tab(None, false, false);
            })),
            ("New Private Tab", Box::new(|app: &App| {
                app.new_tab(None, true, false);
            })),
            ("Reading Mode", Box::new(|app: &App| app.reader())),
            ("Bookmarks", Box::new(|app: &App| app.toggle_panel(PanelKind::Bookmarks))),
            ("History", Box::new(|app: &App| app.toggle_panel(PanelKind::History))),
            ("Downloads", Box::new(|app: &App| app.toggle_panel(PanelKind::Downloads))),
            ("Settings", Box::new(|app: &App| app.toggle_panel(PanelKind::Settings))),
            ("Shortcuts", Box::new(|app: &App| app.toggle_panel(PanelKind::Shortcuts))),
            ("Blocker Off Here", Box::new(|app: &App| {
                let host = app
                    .state
                    .borrow()
                    .active_tab()
                    .map(|tab| crate::shield::host_of(&tab.url))
                    .unwrap_or_default();
                if host.is_empty() {
                    return;
                }
                let off = crate::shield::allowed(&host);
                crate::shield::set_allowed(&host, !off);
                app.flash_toast(if off {
                    "Blocker on here again."
                } else {
                    "Blocker off here."
                });
                app.reload();
            })),
        ];
        for (label, action) in items {
            let item = gtk::MenuItem::with_label(label);
            let weak = self.weak();
            item.connect_activate(move |_| {
                if let Some(app) = weak.upgrade() {
                    action(&app);
                }
            });
            menu.append(&item);
        }
        menu.show_all();
        menu.popup_easy(1, 0);
    }

    pub fn flash_toast(&self, message: &str) {
        let toast = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        crate::ui::class(&toast, "dlbar");
        toast.set_margin_bottom(24);
        toast.set_halign(gtk::Align::Center);
        toast.set_valign(gtk::Align::End);
        let label = gtk::Label::new(Some(message));
        label.set_margin_start(12);
        label.set_margin_end(12);
        label.set_margin_top(7);
        label.set_margin_bottom(7);
        toast.add(&label);
        self.ui.overlay.add_overlay(&toast);
        toast.show_all();
        glib::timeout_add_seconds_local(2, move || {
            toast.hide();
            glib::ControlFlow::Break
        });
    }

    pub fn flash_error(&self, message: &str) {
        self.flash_toast(message);
    }

    /// A page that will not open, explained by the browser.
    pub fn show_error(&self, id: TabId, reason: &str, url: &str) {
        {
            let mut state = self.state.borrow_mut();
            // A tab holds its own failure: a 404 in one tab must not paint over
            // the page in another, and Try again must reload the right one.
            match state.errors.get_mut(&id) {
                Some(existing) => {
                    if existing.url.is_empty() && !url.is_empty() {
                        existing.url = url.to_string();
                    }
                    return;
                }
                None => {
                    state.errors.insert(
                        id,
                        ErrorPage { reason: reason.to_string(), url: url.to_string() },
                    );
                }
            }
        }
        if id != self.active_id() {
            // Not the page being looked at: the error is remembered above and
            // shows when its tab is selected.
            self.render_rows();
            return;
        }
        self.ui.error_title.set_text("That page did not open");
        self.ui.error_reason.set_text(reason);
        self.ui.error_address.set_text(url);
        // A child added to a stack after the window was shown still has to be
        // told it is visible before it can be the visible child.
        self.ui.errorpage.show_all();
        self.ui.stack.set_visible_child_name("error");
    }

    /// The engine says the load ended; only a load that did not fail takes the
    /// error page away.
    pub fn load_finished(&self, id: TabId) {
        let failed = self.state.borrow().tab(id).map(|tab| tab.failed).unwrap_or(false);
        if !failed {
            self.clear_error(id);
        }
    }

    pub fn mark_failed(&self, id: TabId) {
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.failed = true;
        }
    }

    pub fn clear_error(&self, id: TabId) {
        let had = {
            let mut state = self.state.borrow_mut();
            state.errors.remove(&id).is_some()
        };
        if had {
            self.render();
        }
    }

    pub fn retry(&self) {
        let id = self.active_id();
        let url = self
            .state
            .borrow()
            .errors
            .get(&id)
            .map(|error| error.url.clone())
            .unwrap_or_default();
        {
            let mut state = self.state.borrow_mut();
            state.errors.remove(&id);
        }
        if !url.is_empty() {
            self.navigate(id, &url);
        }
    }

    /// A picture of the window, without asking anything of the compositor:
    /// the browser's own widgets are drawn into a surface, and the page is
    /// taken from the engine's snapshot, then the two are laid together.
    pub fn capture(&self, path: &std::path::Path) -> Result<(), String> {
        let width = self.ui.overlay.allocated_width().max(self.ui.shell.allocated_width());
        let height = self.ui.overlay.allocated_height().max(self.ui.shell.allocated_height());
        if width < 2 || height < 2 {
            return Err("the window has not been laid out".into());
        }
        // Let GTK finish any allocation it has queued: a window that has not
        // been on screen since its last resize is only drawn once this runs.
        // A window that has not been on screen yet has not allocated its
        // overlay layers: give them the size they will have before drawing.
        let target = (
            self.ui.overlay.allocated_width().max(200),
            self.ui.overlay.allocated_height().max(200),
        );
        self.layout_layers(target.0, target.1);
        self.ui.window.check_resize();
        for _ in 0..120 {
            let context = glib::MainContext::default();
            while context.iteration(false) {}
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        while gtk::events_pending() {
            gtk::main_iteration_do(false);
        }
        // Everything is drawn in the overlay's own coordinates: the window's
        // client area starts a pixel below the top of the frame, and a picture
        // should not have a transparent line where a window edge is.
        let offset = self
            .ui
            .overlay
            .translate_coordinates(&self.ui.window, 0, 0)
            .unwrap_or((0, 0));
        let page = match self.view(self.active_id()) {
            Some(view) => {
                let surface = self.page_snapshot(&view);
                let origin = view.translate_coordinates(&self.ui.overlay, 0, 0).unwrap_or((0, 0));
                surface.map(|surface| (surface, origin.0, origin.1, view.allocated_width(), view.allocated_height()))
            }
            None => None,
        };
        eprintln!(
            "search: probe win={}x{} ov={}x{} shell={}x{}@{:?} side={}",
            self.ui.window.allocated_width(),
            self.ui.window.allocated_height(),
            self.ui.overlay.allocated_width(),
            self.ui.overlay.allocated_height(),
            self.ui.shell.allocated_width(),
            self.ui.shell.allocated_height(),
            self.ui.shell.translate_coordinates(&self.ui.window, 0, 0),
            self.ui.side.allocated_width()
        );
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height)
            .map_err(|error| error.to_string())?;
        {
            let context = cairo::Context::new(&surface).map_err(|error| error.to_string())?;
            // A picture is opaque: the ground goes down first, so a viewer
            // never renders an unpainted pixel as black.
            let ground = self.palette.borrow().card;
            context.set_source_rgba(
                ground.0 as f64 / 255.0,
                ground.1 as f64 / 255.0,
                ground.2 as f64 / 255.0,
                1.0,
            );
            context.set_operator(cairo::Operator::Source);
            let _ = context.paint();
            // The window's own corners, as the desktop draws them.
            let radius = 12.0;
            rounded_rectangle(&context, 0.0, 0.0, width as f64, height as f64, radius);
            let _ = context.clip();
            context.set_operator(cairo::Operator::Over);
            let _ = context.translate(offset.0 as f64, offset.1 as f64);

            // 1. the frame and the column. The whole overlay is drawn, so
            // every windowed row and light comes with it.
            self.ui.overlay.draw(&context);

            // 2. the page, from the engine's own snapshot.
            if let Some((page, x, y, page_width, page_height)) = page.as_ref() {
                let _ = context.save();
                context.rectangle(*x as f64, *y as f64, *page_width as f64, *page_height as f64);
                let _ = context.clip();
                let _ = context.translate(*x as f64, *y as f64);
                let _ = context.set_source_surface(page, 0.0, 0.0);
                let _ = context.paint();
                let _ = context.restore();
            }

            // 3. whatever is over the page: the field, find, a panel.
            for child in self.ui.overlay.children() {
                if child == self.ui.shell.clone().upcast::<gtk::Widget>() {
                    continue;
                }
                if child.is_visible() {
                    draw_at(&context, &child, &self.ui.overlay);
                }
            }
        }
        surface.flush();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut file = std::fs::File::create(path).map_err(|error| error.to_string())?;
        surface.write_to_png(&mut file).map_err(|error| error.to_string())?;
        Ok(())
    }

    fn page_snapshot(&self, view: &webkit2gtk::WebView) -> Option<cairo::Surface> {
        let slot: Rc<RefCell<Option<cairo::Surface>>> = Rc::new(RefCell::new(None));
        let slot_clone = slot.clone();
        let loop_ = glib::MainLoop::new(None, false);
        let loop_clone = loop_.clone();
        view.snapshot(
            webkit2gtk::SnapshotRegion::Visible,
            webkit2gtk::SnapshotOptions::NONE,
            None::<&gtk::gio::Cancellable>,
            move |result| {
                if let Ok(surface) = result {
                    *slot_clone.borrow_mut() = Some(surface);
                }
                loop_clone.quit();
            },
        );
        let loop_watchdog = loop_.clone();
        glib::timeout_add_seconds_local(8, move || {
            loop_watchdog.quit();
            glib::ControlFlow::Break
        });
        loop_.run();
        let taken = slot.borrow().clone();
        taken
    }

    pub fn install_keys(&self, widget: &impl IsA<gtk::Widget>) {
        let weak = self.weak();
        widget.connect_key_press_event(move |_, event| match weak.upgrade() {
            Some(app) => crate::keys::handle(&app, event),
            None => crate::gtkx::inh(false),
        });
    }

    // ---- wiring ---------------------------------------------------------

    fn wire(&self) {
        let weak = self.weak();
        self.ui.window.connect_key_press_event(move |_, event| match weak.upgrade() {
            Some(app) => crate::keys::handle(&app, event),
            None => crate::gtkx::inh(false),
        });

        // The lights.
        {
            let weak = self.weak();
            self.ui.light_red.connect_button_press_event(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    app.ui.window.close();
                }
                crate::gtkx::inh(true)
            });
        }
        {
            let weak = self.weak();
            self.ui.light_amber.connect_button_press_event(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    app.ui.window.iconify();
                }
                crate::gtkx::inh(true)
            });
        }
        {
            let weak = self.weak();
            self.ui.light_green.connect_button_press_event(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    app.toggle_maximised();
                }
                crate::gtkx::inh(true)
            });
        }

        // The page's doors.
        {
            let weak = self.weak();
            self.ui.back.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.go_back();
                }
            });
        }
        {
            let weak = self.weak();
            self.ui.forward.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.go_forward();
                }
            });
        }
        {
            let weak = self.weak();
            self.ui.reload.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.reload();
                }
            });
        }
        {
            let weak = self.weak();
            self.ui.menu.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.main_menu();
                }
            });
        }

        // Drag the window by the band above the pins.
        {
            let weak = self.weak();
            self.ui.lights.connect_button_press_event(move |_, event| {
                if let Some(app) = weak.upgrade() {
                    app.begin_drag(event);
                }
                crate::gtkx::inh(false)
            });
        }

        // The field.
        // Up and down belong to the field's list, Escape puts it away.
        {
            let weak = self.weak();
            self.omni.entry.connect_key_press_event(move |_, event| {
                let Some(app) = weak.upgrade() else { return crate::gtkx::inh(false) };
                match event.keyval().name().as_deref() {
                    Some("Down") => {
                        app.omni.step(1);
                        crate::gtkx::inh(true)
                    }
                    Some("Up") => {
                        app.omni.step(-1);
                        crate::gtkx::inh(true)
                    }
                    Some("Escape") => {
                        app.close_omni();
                        crate::gtkx::inh(true)
                    }
                    _ => crate::gtkx::inh(false),
                }
            });
        }
        {
            let weak = self.weak();
            self.omni.entry.connect_activate(move |_| {
                if let Some(app) = weak.upgrade() {
                    crate::omnibox::commit(&app);
                }
            });
        }
        {
            let weak = self.weak();
            self.omni.entry.connect_changed(move |entry| {
                if let Some(app) = weak.upgrade() {
                    app.refresh_offers(&entry.text());
                }
            });
        }
        {
            let weak = self.weak();
            self.omni.scrim.connect_button_press_event(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    app.close_omni();
                }
                crate::gtkx::inh(true)
            });
        }

        // Find: Return searches again.
        {
            let entry = self.find.borrow().entry.clone();
            let weak = self.weak();
            entry.connect_activate(move |entry| {
                if let Some(app) = weak.upgrade() {
                    app.find_query(&entry.text(), true);
                }
            });
        }

        // The three doors stay out of the way until the pointer comes to the
        // top of the column, the way the Mac build keeps its chrome quiet.
        {
            self.ui.helm.set_visible(false);
            let helm = self.ui.helm.clone();
            let lights = self.ui.lights.clone();
            lights.add_events(gtk::gdk::EventMask::ENTER_NOTIFY_MASK | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK);
            let helm_enter = helm.clone();
            lights.connect_enter_notify_event(move |_, _| {
                helm_enter.set_visible(true);
                crate::gtkx::inh(false)
            });
            let helm_leave = helm.clone();
            lights.connect_leave_notify_event(move |_, event| {
                let (x, y) = event.position();
                if x < 0.0 || y < 0.0 || x > 240.0 || y > 52.0 {
                    helm_leave.set_visible(false);
                }
                crate::gtkx::inh(false)
            });
        }

        // Every layer is laid out for the window it is over: a scrim has to
        // cover the window, not a corner of it.
        {
            let weak = self.weak();
            self.ui.overlay.connect_size_allocate(move |_, allocation| {
                if let Some(app) = weak.upgrade() {
                    app.layout_layers(allocation.width(), allocation.height());
                }
            });
        }

        // A blank page is a place to start: clicking it raises the field, so
        // the browser can be used with a mouse alone.
        {
            let weak = self.weak();
            self.ui.blank_holder.connect_button_press_event(move |_, _| {
                if let Some(app) = weak.upgrade() {
                    app.open_omni(true);
                }
                crate::gtkx::inh(true)
            });
        }
        {
            let weak = self.weak();
            self.ui.error_retry.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.retry();
                }
            });
        }

        self.install_keys(&self.ui.overlay);
    }

    fn begin_drag(&self, event: &gtk::gdk::Event) {
        if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
            if button.button() == 1 {
                if button.event_type() == gtk::gdk::EventType::DoubleButtonPress {
                    self.toggle_maximised();
                    return;
                }
                let time = button.time();
                let (x, y) = button.root();
                self.ui
                    .window
                    .begin_move_drag(1, x as i32, y as i32, time);
            }
        }
    }

    fn toggle_maximised(&self) {
        if self.ui.window.is_maximized() {
            self.ui.window.unmaximize();
        } else {
            self.ui.window.maximize();
        }
    }
}

pub fn install_self(app: &AppRef) {
    CURRENT.with(|c| *c.borrow_mut() = Some(app.clone()));
}

/// Draw a widget where it stands inside an ancestor.
fn draw_at(context: &cairo::Context, widget: &impl IsA<gtk::Widget>, ancestor: &impl IsA<gtk::Widget>) {
    let offset = widget.translate_coordinates(ancestor, 0, 0).unwrap_or((0, 0));
    let _ = context.save();
    let _ = context.translate(offset.0 as f64, offset.1 as f64);
    widget.draw(context);
    let _ = context.restore();
}

fn rounded_rectangle(context: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let radius = radius.min(width / 2.0).min(height / 2.0);
    let pi = std::f64::consts::PI;
    context.new_sub_path();
    context.arc(x + width - radius, y + radius, radius, -pi / 2.0, 0.0);
    context.arc(x + width - radius, y + height - radius, radius, 0.0, pi / 2.0);
    context.arc(x + radius, y + height - radius, radius, pi / 2.0, pi);
    context.arc(x + radius, y + radius, radius, pi, 3.0 * pi / 2.0);
    context.close_path();
}

/// What a field's text means, apart from any browser: the pure half of the
/// address bar, so it can be tested without a window.
pub fn plain_target(typed: &str, search: &dyn Fn(&str) -> String) -> String {
    let typed = typed.trim();
    if typed.is_empty() {
        return String::new();
    }
    if typed.contains("://") || typed.starts_with("about:") || typed.starts_with("file:") {
        return typed.to_string();
    }
    let hostlike = !typed.contains(char::is_whitespace)
        && (typed.contains('.') || typed.starts_with("localhost") || typed.starts_with("127.0.0.1"));
    if hostlike {
        format!("https://{typed}")
    } else {
        search(typed)
    }
}

pub fn resolve(app: &App, text: &str) -> String {
    let prefs = app.prefs.borrow();
    plain_target(text, &|query| prefs.search_url(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(query: &str) -> String {
        crate::store::Prefs::default().search_url(query)
    }

    #[test]
    fn what_was_typed_decides_where_it_goes() {
        // The resolver's pure half: an address keeps its scheme, a host gets one,
        // and words become a search.
        assert_eq!(crate::app::plain_target("https://example.com", &engine), "https://example.com");
        assert_eq!(crate::app::plain_target("about:blank", &engine), "about:blank");
        assert_eq!(crate::app::plain_target("example.com", &engine), "https://example.com");
        assert_eq!(crate::app::plain_target("localhost:8099", &engine), "https://localhost:8099");
        assert!(crate::app::plain_target("two words", &engine).contains("duckduckgo"));
        assert_eq!(crate::app::plain_target("   ", &engine), "");
    }
}
