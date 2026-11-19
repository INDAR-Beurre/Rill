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
        let title = self.title.trim();
        if let Some(first) = title.chars().find(|c| c.is_alphabetic()) {
            return first;
        }
        if title.is_empty() {
            return 'N';
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

/// One file the browser was asked to save.
///
/// The byte counts are here so the panel can say how far along a download is
/// rather than only whether it has finished: "Downloading to /a/path" for a
/// 40 MB file is a sentence that tells the reader nothing they want to know.
/// `failed` is separate from `done` because a download that failed used to
/// stay "Downloading" for the rest of the session.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct DownloadRec {
    pub name: String,
    pub path: String,
    pub done: bool,
    #[serde(default)]
    pub failed: bool,
    #[serde(default)]
    pub got: u64,
    #[serde(default)]
    pub total: u64,
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

/// One entry in a menu: what it says, and what it does.
type MenuItem = Box<dyn Fn(&App)>;

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
    pub reading_tabs: RefCell<std::collections::BTreeSet<TabId>>,
    /// The page icons the engine has handed us, per tab. Kept here rather than
    /// asked for during a redraw: a redraw happens while the rows are being
    /// rebuilt, and the view may not exist yet.
    pub icons: RefCell<std::collections::BTreeMap<TabId, cairo::Surface>>,
    /// What the bookmarks bar currently shows, so it is rebuilt on a change to
    /// the bookmarks rather than on every repaint.
    bmarks_shown: RefCell<Vec<(String, String)>>,
    /// The modifiers the last key carried, for the one shortcut whose meaning
    /// depends on a modifier the entry cannot report (Shift Return in find).
    pub last_modifiers: std::cell::Cell<gtk::gdk::ModifierType>,
    /// Whether this browser put the window in full screen, so F11 can undo
    /// exactly that and nothing else.
    pub fullscreen: std::cell::Cell<bool>,
}

/// A kept page's title, short enough for a chip in a row. A long title is cut
/// at a word boundary rather than mid-word, because a chip that reads "The
/// Guardian vie" looks broken.
fn short_title(title: &str) -> String {
    let title = if title.is_empty() { "Untitled" } else { title };
    let chars: Vec<char> = title.chars().collect();
    if chars.len() <= 26 {
        return title.to_string();
    }
    let head: String = chars.iter().take(24).collect();
    match head.rsplit_once(' ') {
        Some((left, _)) if left.chars().count() > 12 => format!("{left}\u{2026}"),
        _ => format!("{}\u{2026}", head.trim_end()),
    }
}

/// The size to open at next time. A maximised window reports the size of its
/// screen, so remembering that would leave the browser filling the screen the
/// next time it opens in a normal frame.
fn remembered_size(window: &gtk::Window) -> Option<(i32, i32)> {
    if window.is_maximized() {
        return None;
    }
    let (width, height) = window.size();
    if width < 400 || height < 300 {
        return None;
    }
    Some((width, height))
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
    window.set_resizable(true);
    window.set_default_size(prefs.window_size.0, prefs.window_size.1);
    window.set_position(gtk::WindowPosition::Center);

    prefs.desktop_dark = gtk::Settings::default()
        .map(|s| s.is_gtk_application_prefer_dark_theme())
        .unwrap_or(false);

    let palette = Palette::for_look(prefs.appearance.clone(), prefs.desktop_dark);
    // One opaque window colour, so a strip the shell does not cover is the
    // window's own ground rather than whatever the compositor shows through.
    window.set_app_paintable(true);

    // The window as it was left. Saved on the way out rather than on every
    // resize, because a resize drag is dozens of events and one write at the
    // end is the same answer.
    {
        let prefs_ref = prefs.clone();
        window.connect_delete_event(move |window, _event| {
            let mut saved = prefs_ref.clone();
            saved.window_maximised = window.is_maximized();
            if let Some((width, height)) = remembered_size(window) {
                saved.window_size = (width, height);
            }
            saved.save();
            glib::Propagation::Proceed
        });
    }

    let ui = ui::build(window.clone(), palette, &prefs);
    window.add(&ui.overlay);

    let provider = gtk::CssProvider::new();
    if let Some(screen) = gtk::gdk::Screen::default() {
        gtk::StyleContext::add_provider_for_screen(
            &screen,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER,
        );
    }

    let mut tabs: Vec<Tab> = Vec::new();
    let mut next_id: TabId = 1;
    if prefs.restore_session && !prefs.skip_restore {
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
        downloads: Records::load_downloads(),
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
        icons: RefCell::new(std::collections::BTreeMap::new()),
        bmarks_shown: RefCell::new(Vec::new()),
        last_modifiers: std::cell::Cell::new(gtk::gdk::ModifierType::empty()),
        fullscreen: std::cell::Cell::new(false),
        last_layout: RefCell::new((0, 0)),
        reading: std::cell::Cell::new(false),
        reading_tabs: RefCell::new(std::collections::BTreeSet::new()),
    });

    // Installed before `wire`, because every handler that calls `weak()` reads
    // this slot. Wired first and installed after, all of them got an empty
    // handle: clicks, keys on the controls and the drag did nothing at all.
    install_self(&app);
    app.ui.paned.set_position(prefs_side_width);
    {
        // A dragged divider is a choice, so it is written down. Without this
        // the column snapped back to its saved width on the next launch and
        // the pin wrap, which now measures the real column, disagreed with it.
        let weak = app.weak();
        app.ui.paned.connect_position_notify(move |paned| {
            let Some(app) = weak.upgrade() else { return };
            let width = paned.position() as f64;
            if width < 40.0 {
                return;
            }
            let mut prefs = app.prefs.borrow_mut();
            if (prefs.side_width - width).abs() < 0.5 {
                return;
            }
            prefs.side_width = width;
            prefs.save();
            drop(prefs);
            app.render_pins();
        });
    }
    app.wire();
    app.apply_style();
    app.render();
    // A restored session comes back asleep, which is right for every tab but
    // the one in front: nothing made its view, so the window opened on the
    // blank page while the column highlighted a tab with an address. Waking
    // the active tab here is what a click on it would have done.
    let active = app.active_id();
    app.select(active);
    app
}

pub fn view_id(view: &webkit2gtk::WebView) -> Option<TabId> {
    let app = current()?;
    let ids: Vec<TabId> = app.state.borrow().tabs.iter().map(|tab| tab.id).collect();
    ids.into_iter().find(|id| app.view(*id).as_ref() == Some(view))
}

impl App {
    pub fn weak(&self) -> Weak<App> {
        CURRENT.with(|c| c.borrow().as_ref().map(Rc::downgrade).unwrap_or_default())
    }

    pub fn dark(&self) -> bool {
        self.palette.borrow().is_dark()
    }

    pub fn apply_style(&self) {
        let css = crate::css::build(&self.palette.borrow(), self.prefs.borrow().row_height());
        if let Err(error) = self.provider.load_from_data(css.as_bytes()) {
            eprintln!("search: the stylesheet did not load: {error}");
        }
        if std::env::var_os("SEARCH_CSS_DUMP").is_some() {
            let _ = std::fs::write("/tmp/search-dump.css", &css);
        }
        // Pages follow the same appearance the frame does, which WebKit takes
        // from the toolkit's own preference. Only a look the person chose is
        // written: writing it while the look is "follow the desktop" would make
        // this browser's own answer the desktop's answer next launch.
        if !matches!(self.prefs.borrow().appearance, crate::store::Look::System) {
            if let Some(settings) = gtk::Settings::default() {
                settings.set_gtk_application_prefer_dark_theme(self.dark());
            }
        }
        for widget in [self.ui.shell.clone().upcast::<gtk::Widget>(), self.ui.window.clone().upcast::<gtk::Widget>()] {
            widget.style_context().remove_class("dark");
            widget.style_context().remove_class("light");
            widget.style_context().add_class(if self.dark() { "dark" } else { "light" });
        }
    }

    /// Re-read the desktop's answer, for the look that follows it. Called when
    /// the desktop's own appearance changes, so "follow the desktop" means
    /// followed rather than remembered from launch.
    pub fn follow_desktop(&self) {
        if !matches!(self.prefs.borrow().appearance, crate::store::Look::System) {
            return;
        }
        let dark_desktop = gtk::Settings::default()
            .map(|s| s.is_gtk_application_prefer_dark_theme())
            .unwrap_or(false);
        {
            let mut prefs = self.prefs.borrow_mut();
            if prefs.desktop_dark == dark_desktop {
                return;
            }
            prefs.desktop_dark = dark_desktop;
        }
        *self.palette.borrow_mut() =
            crate::palette::Palette::for_look(crate::store::Look::System, dark_desktop);
        self.apply_style();
        self.apply_page_tone();
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            if let Some(view) = self.view(id) {
                let page = if self.dark() { (0.043, 0.043, 0.043) } else { (1.0, 1.0, 1.0) };
                view.set_background_color(&gtk::gdk::RGBA::new(page.0, page.1, page.2, 1.0));
            }
        }
        self.render();
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
        // Every tab is a web process. A page that opens tabs in a loop, or a
        // person holding Ctrl+T, would otherwise keep forking until the system
        // kills something — and what it kills is not necessarily this browser.
        // Forty is far past what the column can show and far short of what the
        // machine can carry.
        if self.state.borrow().tabs.len() >= TAB_LIMIT {
            self.flash_error("That is as many tabs as this browser will hold");
            return self.active_id();
        }
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

    pub fn tab_reading(&self, id: TabId) -> bool {
        self.reading_tabs.borrow().contains(&id)
    }

    pub fn select(&self, id: TabId) {
        if self.state.borrow().position(id).is_none() {
            return;
        }
        self.state.borrow_mut().active = id;
        self.reading.set(self.tab_reading(id));
        self.ensure_view(id);
        self.ui.stack.set_visible_child_name(&format!("tab-{id}"));
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.asleep = false;
            tab.last_seen = crate::store::now_secs();
        }
        if let Some(view) = self.view(id) {
            view.grab_focus();
        }
        // A tab chosen from the address field's list is still a tab chosen:
        // leaving the field open over the page it just opened would mean the
        // next keystroke goes into an address box nobody is looking at.
        if self.omni.visible.get() {
            self.omni.hide(&self.ui);
        }
        self.render();
        self.scroll_active_into_view();
        self.schedule_session();
    }

    /// Bring the row that was just selected inside the visible part of the
    /// list. Ctrl Tab past the bottom used to change the page and leave the
    /// highlight off-screen, which reads as "nothing happened".
    fn scroll_active_into_view(&self) {
        let active = self.state.borrow().active;
        let row = self.state.borrow().tab(active).and_then(|tab| tab.row.as_ref().map(|row| row.event.clone()));
        let Some(row) = row else { return };
        let adjust = self.ui.rows_scroll.vadjustment();
        let Some((_, y)) = row.translate_coordinates(&self.ui.rows, 0, 0) else { return };
        let height = row.allocated_height().max(1);
        let top = adjust.value();
        let page = adjust.page_size();
        let wanted = if (y as f64) < top {
            y as f64
        } else if (y + height) as f64 > top + page {
            (y + height) as f64 - page
        } else {
            return;
        };
        adjust.set_value(wanted.clamp(adjust.lower(), (adjust.upper() - page).max(adjust.lower())));
    }

    /// How many views the stack is still holding for tabs that no longer
    /// exist. A view dropped without being removed from its container keeps a
    /// whole engine inside the window, so the count is worth being able to read.
    pub fn orphan_views(&self) -> usize {
        let live: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        self.ui
            .stack
            .children()
            .iter()
            .filter(|child| child.widget_name().starts_with("page-"))
            .filter(|child| {
                let name = child.widget_name();
                let id: TabId = name.trim_start_matches("page-").parse().unwrap_or(0);
                !live.contains(&id)
            })
            .count()
    }

    pub fn close_tab(&self, id: TabId) {
        let trace = std::env::var_os("SEARCH_TRACE_CLOSE").is_some();
        let started = std::time::Instant::now();
        let (index, was_active) = {
            let state = self.state.borrow();
            match state.position(id) {
                Some(index) => (index, state.active == id),
                None => return,
            }
        };
        if let Some(view) = self.view(id) {
            // Removing it from the stack is not enough: the view is still a
            // child of the window and keeps its process, its page and its
            // rendering alive. It has to be destroyed.
            self.ui.stack.remove(&view);
            // SAFETY: the view was just taken out of its container and nothing
            // else holds a reference to it in this scope beyond the local
            // binding, which is not used again.
            unsafe { view.destroy() };
        }
        // Tab ids only ever go up, so a map keyed by id that is not pruned
        // here grows for as long as the session lasts.
        self.icons.borrow_mut().remove(&id);
        self.state.borrow_mut().errors.remove(&id);
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
        self.reading_tabs.borrow_mut().remove(&id);
        if self.reading.get() {
            self.reading.set(false);
        }
        self.select(next_active);
        if trace {
            eprintln!(
                "search: close {id} took {}ms (views left: {})",
                started.elapsed().as_millis(),
                self.ui.stack.children().len()
            );
        }
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
        self.tone_for(&view);
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

    /// Throw a tab's view away so it is made again: used when the shield is
    /// turned off for a host, which changes which content manager the view
    /// needs. The page is reloaded, so nothing is lost but the engine's cache.
    /// Turn the shield off for the page in front of you, or on again. The rules
    /// are baked into the content manager a view is made with, so the view has
    /// to be made again; writing the file and reloading was a no-op.
    pub fn toggle_blocker(&self) {
        let host = self
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
        self.flash_toast(if off { "Blocker on here again." } else { "Blocker off here." });
        self.rebuild_view(self.active_id());
        self.reload();
    }

    /// Every view is built on a content manager that carries the extensions, so
    /// installing, removing or switching one on has to build them again. The
    /// pages reload from cache; nothing is lost.
    pub fn rebuild_every_view(&self) {
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            self.rebuild_view(id);
        }
    }

    pub fn rebuild_view(&self, id: TabId) {
        let name = format!("tab-{id}");
        if let Some(view) = self.view(id) {
            // Removing it from the stack is not enough: the view is still a
            // child of the window and keeps its process, its page and its
            // rendering alive. It has to be destroyed.
            self.ui.stack.remove(&view);
            // SAFETY: the view was just taken out of its container and nothing
            // else holds a reference to it in this scope beyond the local
            // binding, which is not used again.
            unsafe { view.destroy() };
        }
        if let Some(old) = self.ui.stack.child_by_name(&name) {
            self.ui.stack.remove(&old);
        }
        self.ensure_view(id);
        if let Some(view) = self.view(id) {
            if id == self.active_id() {
                self.ui.stack.set_visible_child(&view);
                view.grab_focus();
            }
        }
    }

    /// The bookmarks bar: one chip per kept page, in the order they were kept.
    ///
    /// Rebuilt only when the set changes, because a row of buttons rebuilt on
    /// every title is a row of buttons that flickers while a page loads.
    pub fn render_bmarks(&self) {
        let (items, wanted): (Vec<(String, String)>, bool) = {
            let prefs = self.prefs.borrow();
            let state = self.state.borrow();
            let items: Vec<(String, String)> = state
                .bookmarks
                .iter()
                .map(|b| (b.title.clone(), b.url.clone()))
                .collect();
            // The switch has to mean what it says. `||` here meant that
            // turning "Show the bookmarks bar" off did nothing at all as soon
            // as one page had been kept.
            (items, prefs.show_bookmarks_bar && !state.bookmarks.is_empty())
        };
        let built = self.bmarks_shown.borrow().clone();
        if built != items {
            for child in self.ui.bmarks.children() {
                self.ui.bmarks.remove(&child);
            }
            for (title, url) in &items {
                let chip = gtk::Button::with_label(&short_title(title));
                crate::ui::class(&chip, "bmark");
                chip.set_tooltip_text(Some(url));
                chip.set_relief(gtk::ReliefStyle::None);
                let target = url.clone();
                let weak = self.weak();
                chip.connect_clicked(move |_| {
                    if let Some(app) = weak.upgrade() {
                        // A kept page opens in the tab you are in, which is
                        // what a bookmarks bar does everywhere else.
                        let id = app.active_id();
                        if app.state.borrow().tab(id).map(|t| t.url.is_empty()).unwrap_or(false) {
                            app.navigate(id, &target);
                        } else {
                            app.new_tab(Some(&target), false, false);
                        }
                    }
                });
                self.ui.bmarks.add(&chip);
            }
            self.ui.bmarks.show_all();
            *self.bmarks_shown.borrow_mut() = items;
        }
        if wanted {
            self.ui.bmarkbar.show_all();
        } else {
            self.ui.bmarkbar.hide();
        }
    }

    pub fn render(&self) {
        self.render_pins();
        self.render_bmarks();
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
                self.ui.error_retry.set_label(error_action(&error.reason));
            }
            self.ui.errorpage.show_all();
            self.ui.stack.set_visible_child_name("error");
        } else if blank {
            self.ui.stack.set_visible_child_name("blank");
        } else {
            // Back from an error page: the page that loaded fine behind it has
            // to be the visible child again, or a failed load leaves the
            // browser showing an apology over a page that is actually there.
            let active = self.state.borrow().active;
            let name = format!("tab-{active}");
            if self.ui.stack.child_by_name(&name).is_some() {
                self.ui.stack.set_visible_child_name(&name);
            }
        }
    }

    /// Rebuild the tab column only when the tabs have actually changed shape.
    /// A title arriving used to empty and refill the whole column, which is the
    /// moment a tab switch felt like a redraw instead of a repaint.
    fn render_rows(&self) {
        let shape: Vec<(TabId, bool)> = {
            let state = self.state.borrow();
            state.tabs.iter().map(|t| (t.id, t.pinned)).collect()
        };
        let same = {
            let state = self.state.borrow();
            let built: Vec<(TabId, bool)> = state
                .tabs
                .iter()
                .filter(|t| t.row.is_some())
                .map(|t| (t.id, t.pinned))
                .collect();
            let wanted: Vec<(TabId, bool)> = shape.iter().copied().filter(|(_, pinned)| !pinned).collect();
            built.len() == wanted.len() && built.iter().zip(wanted.iter()).all(|(a, b)| a.0 == b.0)
        };
        if same {
            // The column is already the right shape, so only what has changed
            // is repainted: the titles, and which row is the one you are on.
            // Collected first: update_row_title borrows the state for writing,
            // and a borrow held across that is how this panicked at launch.
            let (ids, active): (Vec<TabId>, TabId) = {
                let state = self.state.borrow();
                (
                    state.tabs.iter().filter(|t| !t.pinned).map(|t| t.id).collect(),
                    state.active,
                )
            };
            for id in ids {
                self.update_row_title(id);
                let event = {
                    let state = self.state.borrow();
                    state.tab(id).and_then(|t| t.row.as_ref().map(|row| row.event.clone()))
                };
                if let Some(event) = event {
                    let context = event.style_context();
                    if id == active {
                        context.add_class("live");
                    } else {
                        context.remove_class("live");
                    }
                    if let Some(row) = self.state.borrow().tab(id).and_then(|t| t.row.as_ref()) {
                        let context = row.title.style_context();
                        if id == active { context.add_class("liveword"); } else { context.remove_class("liveword"); }
                    }
                }
            }
            // The column was already built, so nothing moved it: a tab that
            // became active without a click on its row (Ctrl Tab, Ctrl 1..9,
            // a launcher hand-off) still has to be brought into view.
            self.scroll_active_into_view();
            return;
        }
        for child in self.ui.rows.children() {
            self.ui.rows.remove(&child);
        }
        // The old rows are gone with their geometry, so the adjustment still
        // holds last frame's values: carrying them over would leave a freshly
        // built column scrolled to a place that no longer means anything.
        // Re-anchoring below costs one pass and is what keeps the live row
        // visible after a rebuild.
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
            let row = ui::tab_row(
                &title,
                *self.palette.borrow(),
                height,
                Some(tab.letter()),
                tab.id == active,
                ui::TabState { asleep: tab.asleep, loading: tab.loading, private: tab.private },
            );
            // A page with its own mark shows it; one without keeps its letter.
            let icon = self.icons.borrow().get(&tab.id).cloned();
            if let Some(surface) = icon.as_ref() {
                // The mask and the spinner outrank the site's own mark: what
                // the tab is doing matters more than whose page it is.
                if !tab.private && !tab.loading {
                    ui::mark(&row.icon, tab.letter(), Some(surface), 16);
                }
            }
            let tip = if tab.url.is_empty() {
                "New tab".to_string()
            } else {
                format!("{}\n{}", if title.is_empty() { "Untitled" } else { title.as_str() }, tab.url)
            };
            row.event.set_tooltip_text(Some(&tip));
            // A row is a tab, not a picture of one: the mark is drawn and the
            // title may be a host name, so a screen reader gets the same words
            // the pointer gets. No focus is taken: the page keeps the keyboard.
            if let Some(access) = row.event.accessible() {
                // Fully qualified: gtk's prelude brings several traits with a
                // `set_name`, and this one is atk's.
                gtk::atk::prelude::AtkObjectExt::set_name(&access, &tip);
                gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::PageTab);
            }
            row.event.set_can_focus(false);
            row.event.set_focus_on_click(false);
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
        let plus = crate::icons::area(crate::icons::Icon::Plus, 16, self.palette.borrow().faint);
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
        // After show_all, not before: a row that is not mapped has no
        // allocated height, and the anchor is computed from real geometry.
        self.scroll_active_into_view();
    }

    fn render_pins(&self) {
        for child in self.ui.pins.children() {
            self.ui.pins.remove(&child);
        }
        // Each pin carries its own words and whether it is the tab you are
        // on. A pinned tab with no title and no address used to produce an
        // empty tooltip: the fallback the rows use gives it something to say.
        let pinned: Vec<(TabId, char, String, bool)> = {
            let state = self.state.borrow();
            state
                .tabs
                .iter()
                .filter(|t| t.pinned)
                .map(|t| {
                    let title = if t.title.is_empty() {
                        if t.url.is_empty() { "New tab".to_string() } else { t.url.clone() }
                    } else {
                        t.title.clone()
                    };
                    (t.id, t.letter(), title, t.id == state.active)
                })
                .collect()
        };
        if pinned.is_empty() {
            self.ui.pins_holder.hide();
            self.ui.pins_holder.set_no_show_all(true);
            return;
        }
        // A widget that opts out of show_all is not shown by it, even when it
        // is the one being asked: the flag comes off first.
        self.ui.pins_holder.set_no_show_all(false);
        self.ui.pins_holder.show_all();
        // As many pins in a row as the column can hold: six 35px tiles and
        // their spacing run past the 210px a 232px column leaves, so the sixth
        // used to be clipped at the edge.
        let across = {
            // The column's real width, not the preference: the divider can be
            // dragged, and a wrap computed from a stale number clipped the
            // last tile in every row after the first drag.
            let measured = self.ui.pins.allocated_width();
            let usable = if measured > 40 { measured } else { (self.prefs.borrow().side_width as i32) - 20 };
            (((usable + 3) / (35 + 3)).max(1)) as usize
        };
        for (index, (id, letter, title, live)) in pinned.iter().enumerate() {
            let tile = ui::pin_tile(*letter, *live, *self.palette.borrow());
            // A page that offers a mark shows it and drops the letter; a page
            // with neither keeps the letter, which is at least its own.
            if let Some(surface) = self.icons.borrow().get(id).cloned() {
                ui::mark(&tile.icon, *letter, Some(&surface), 22);
                tile.icon.set_no_show_all(false);
                tile.icon.show_all();
                tile.letter.hide();
            }
            // The tooltip alone said what the page was, so the pin you were
            // on read exactly like the other three. It now says which one is
            // live, and the tile - a letter or a favicon, with no text inside
            // - gets the same words as an accessible name under a tab role,
            // because a screen reader cannot read the letter at all.
            let tip = if *live {
                format!("{}\nActive tab", title)
            } else {
                title.clone()
            };
            let name = if *live {
                format!("Pinned tab: {}, active", title)
            } else {
                format!("Pinned tab: {}", title)
            };
            tile.event.set_tooltip_text(Some(&tip));
            if let Some(access) = tile.event.accessible() {
                gtk::atk::prelude::AtkObjectExt::set_name(&access, &name);
                gtk::atk::prelude::AtkObjectExt::set_role(&access, gtk::atk::Role::PageTab);
            }
            {
                let weak = self.weak();
                let id = *id;
                tile.event.connect_button_press_event(move |_, event| {
                    let Some(app) = weak.upgrade() else { return crate::gtkx::inh(false) };
                    if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
                        match button.button() {
                            1 => app.select(id),
                            2 => app.close_tab(id),
                            // Pinned tabs are not in the rows, so the row's
                            // menu is the only place "Unpin" lived: pinning
                            // was a one-way door.
                            3 => app.tab_menu(id, event),
                            _ => {}
                        }
                    }
                    crate::gtkx::inh(false)
                });
            }
            self.ui.pins.attach(&tile.event, (index % across) as i32, (index / across) as i32, 1, 1);
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
            let url = self.state.borrow().tab(id).map(|t| t.url.clone()).unwrap_or_default();
            if !url.is_empty() {
                let mut state = self.state.borrow_mut();
                Records::name_visit(&mut state.history, &url, title);
            }
            self.schedule_flush();
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
            if let Some(label) = row.letter.as_ref() {
                label.set_text(&letter);
            }
        }
        if let Some(pin) = tab.pin.as_ref() {
            pin.letter.set_text(&letter);
        }
    }

    /// The engine found a page icon. Store it and put it where the letter was.
    pub fn set_icon(&self, id: TabId, surface: Option<cairo::Surface>) {
        let changed = {
            let mut icons = self.icons.borrow_mut();
            match surface {
                Some(surface) => icons.insert(id, surface).is_none(),
                None => icons.remove(&id).is_some(),
            }
        };
        if !changed {
            return;
        }
        let paint = |holder: Option<&gtk::Box>, letter: char, size: i32| {
            if let Some(holder) = holder {
                let icon = self.icons.borrow().get(&id).cloned();
                crate::ui::mark(holder, letter, icon.as_ref(), size);
            }
        };
        let state = self.state.borrow();
        if let Some(tab) = state.tab(id) {
            paint(tab.row.as_ref().map(|row| &row.icon), tab.letter(), 16);
            paint(tab.pin.as_ref().map(|pin| &pin.icon), tab.letter(), 22);
        }
    }

    pub fn set_uri(&self, id: TabId, uri: &str) {
        let changed = {
            let mut state = self.state.borrow_mut();
            match state.tab_mut(id) {
                // A tab that is showing a failure keeps the address it failed
                // on: the engine's document never changed, and reporting the
                // old page over the new address is what made a 404 tab claim
                // to be the site you came from.
                Some(tab) if tab.failed && tab.url != uri => false,
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
        let view = {
            let state = self.state.borrow();
            state.tab(id).and_then(|tab| tab.row.as_ref().map(|row| (row.event.clone(), row.icon.clone(), tab.private, tab.letter())))
        };
        if let Some((row, icon, private, letter)) = view {
            let context = row.style_context();
            if loading { context.add_class("loading"); } else { context.remove_class("loading"); }
            // Update the existing row. Rebuilding it here would restart the
            // spinner and recreate its close button on every engine signal.
            if !private {
                let surface = self.icons.borrow().get(&id).cloned();
                if loading {
                    crate::ui::loading_mark(&icon, self.palette.borrow().muted);
                } else {
                    crate::ui::mark(&icon, letter, surface.as_ref(), 16);
                }
            }
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
                // A private tab's address is the one thing it promised not to
                // keep. It was being written to history.json and then offered
                // back in the address field.
                Some(tab) if !tab.private => (tab.url.clone(), tab.title.clone()),
                _ => return,
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

    /// A remote document must not turn the address bar into a file reader.
    /// `file` is for an address the person entered or a link on a local page,
    /// never a redirect or link supplied by a remote page.
    pub fn navigate(&self, id: TabId, target: &str) {
        if target.is_empty() {
            return;
        }
        let url = resolve(self, target);
        if url.is_empty() {
            return;
        }
        if !crate::web::scheme_is_allowed(&url, true) {
            self.show_error(id, "This browser only opens local pages or web addresses.", &url);
            self.render();
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

    /// The page's own source, written out and opened: the browser has no
    /// second window to put it in, and a file is honest about that.
    pub fn view_source(&self) {
        let Some(view) = self.view(self.active_id()) else { return };
        let weak = self.weak();
        crate::js::run(&view, "document.documentElement.outerHTML", move |result| {
            let Some(app) = weak.upgrade() else { return };
            let body = match result {
                Ok(text) => text,
                Err(error) => {
                    app.flash_toast(&format!("No source: {error}"));
                    return;
                }
            };
            let path = std::env::temp_dir().join("search-source.html");
            if std::fs::write(&path, body).is_err() {
                app.flash_toast("No source: could not write it");
                return;
            }
            app.new_tab(Some(&format!("file://{}", path.display())), false, false);
        });
    }

    /// Full screen, and back. The window is the only thing that changes.
    pub fn toggle_fullscreen(&self) {
        // A window that is already filling the screen is one the user put
        // there; the state lives on the window, so ask it.
        let filling = self.fullscreen.get();
        if filling {
            self.ui.window.unfullscreen();
        } else {
            self.ui.window.fullscreen();
        }
        self.fullscreen.set(!filling);
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
                // Same rule as closing: out of the stack is not gone. A slept
                // tab that keeps its view keeps its whole engine.
                self.ui.stack.remove(&view);
                // SAFETY: the view was just taken out of its container and the
                // local binding is not used again.
                unsafe { view.destroy() };
            }
            if let Some(tab) = self.state.borrow_mut().tab_mut(*id) {
                tab.asleep = true;
            }
        }
        self.render();
    }

    /// The tab-sleeping promise, kept: a minute's tick drops the pages of tabs
    /// that have not been looked at for as long as the setting says.
    /// Started from `install_self`, not from `start`: `self.weak()` reads the
    /// thread-local that `install_self` fills, so a timer armed inside `start`
    /// held an empty weak reference and broke on its first tick. Tabs never
    /// slept, and nothing said so.
    pub fn start_memory_timer(&self) {
        let weak = self.weak();
        // Two minutes, not one. A setting of N minutes means "not looked at
        // for N minutes", and a one-minute tick makes that N+1 at best and 2N
        // at worst. Two minutes keeps the promise inside half a minute of what
        // the setting says, at the default half hour, for no measurable cost.
        glib::timeout_add_seconds_local(120, move || {
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
            // Two tabs on the same page are one suggestion, not two.
            if offers.iter().any(|o| o.url == tab.url) {
                continue;
            }
            if typed_lower.is_empty() || hay.contains(&typed_lower) {
                offers.push(Offer {
                    title: if tab.title.is_empty() { tab.url.clone() } else { tab.title.clone() },
                    url: tab.url.clone(),
                    kind: "Open".into(),
                });
            }
        }
        // Kept pages come first, with three seats of their own: history is
        // longer, so letting it fill the list is what kept a page somebody
        // saved on purpose from ever appearing.
        let mut kept = 0;
        for bookmark in &state.bookmarks {
            if kept >= 3 {
                break;
            }
            let hay = format!("{} {}", bookmark.title, bookmark.url).to_lowercase();
            if (typed_lower.is_empty() || hay.contains(&typed_lower)) && !offers.iter().any(|o| o.url == bookmark.url) {
                offers.push(Offer {
                    title: bookmark.title.clone(),
                    url: bookmark.url.clone(),
                    kind: "Bookmark".into(),
                });
                kept += 1;
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
        offers.truncate(8);
        // What Return would do, offered as a row. Typing words that are not an
        // address used to empty the list while Return still searched: the
        // field looked broken exactly when it was about to do the right thing.
        let words = typed.trim();
        if !words.is_empty() {
            let prefs = self.prefs.borrow();
            let target = plain_target(words, &|query| prefs.search_url(query));
            let is_search = target == prefs.search_url(words);
            drop(prefs);
            if is_search && !offers.iter().any(|o| o.url == target) {
                offers.insert(0, Offer {
                    title: format!("Search for \"{words}\""),
                    url: target,
                    kind: "Search".into(),
                });
                offers.truncate(8);
            }
        }
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
    /// The page tone, applied to one view. Kept separate from the loop so a tab
    /// opened after the switch gets the same sheet as the ones already open.
    fn tone_for(&self, view: &webkit2gtk::WebView) {
        // The switch means what it says. It used to be gated on the frame
        // being dark as well, so in Light it flipped, saved, and did nothing —
        // a control that lies about its own state.
        let on = self.prefs.borrow().darken_pages;
        // Forcing the ground without forcing the ink is what made a page of
        // #222 text on #0b0b0b: 1.24:1, unreadable. The two go together or
        // not at all.
        let sheet = if on {
            "html { background: #0b0b0b !important; color: #ededed !important; }\n\
             body { background-color: transparent !important; color: #ededed !important; }\n\
             p, span, div, li, dt, dd, td, th, h1, h2, h3, h4, h5, h6, blockquote, label, figcaption { color: #ededed !important; }\n\
             a, a:visited { color: #9ecbff !important; }\n\
             :root { color-scheme: dark; }"
        } else {
            ""
        };
        let Some(manager) = view.user_content_manager() else { return };
        manager.remove_all_style_sheets();
        if sheet.is_empty() {
            return;
        }
        let style = webkit2gtk::UserStyleSheet::new(
            sheet,
            webkit2gtk::UserContentInjectedFrames::AllFrames,
            webkit2gtk::UserStyleLevel::User,
            &[],
            &[],
        );
        manager.add_style_sheet(&style);
    }

    pub fn apply_page_tone(&self) {
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            if let Some(view) = self.view(id) {
                self.tone_for(&view);
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

    // Finding one match by hand, over the text the reader can see. Nodes are
    // stitched into one string so a match can cross an inline element, with a
    // newline between blocks so a word ending one paragraph does not join the
    // word starting the next. An empty query drops the selection; otherwise
    // the match after (or before) the current one is selected and centred, and
    // the search wraps the way the engine's does.
    const FIND_STEP_JS: &str = r#"(() => {
        const text = __TEXT__, forward = __FORWARD__;
        const root = document.body || document.documentElement;
        if (!root) return 'none';
        const selection = document.getSelection();
        if (!text) { if (selection) selection.removeAllRanges(); return 'none'; }
        // A text node counts when it is drawn: display:none and its subtree are
        // out, which is what keeps a collapsed menu or a hidden template from
        // joining the haystack. Nothing is dropped for being scrolled away,
        // since a match below the fold is exactly the one a step should reach.
        const visible = (el) => {
            if (!el) return false;
            if (el.checkVisibility && !el.checkVisibility({ visibilityProperty: true, contentVisibilityAuto: true })) return false;
            // Older engines: the same question asked a node at a time, since a
            // collapsed ancestor hides everything under it.
            for (let cur = el; cur && cur !== root; cur = cur.parentElement) {
                const style = getComputedStyle(cur);
                if (style.display === 'none' || style.visibility === 'hidden' || style.visibility === 'collapse') return false;
            }
            return true;
        };
        // A break or a replaced thing between two text nodes renders a gap the
        // string would not otherwise have, so a word on either side must not be
        // allowed to match as one.
        const BREAKS = /^(BR|WBR|HR|IMG|PICTURE|INPUT|SELECT|TEXTAREA|CANVAS|VIDEO|AUDIO|IFRAME|OBJECT|EMBED|SVG|MATH)$/;
        const nodes = [], starts = [], parts = [];
        let block = null, length = 0, pending = false;
        const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT);
        while (walker.nextNode()) {
            const node = walker.currentNode;
            if (node.nodeType === Node.ELEMENT_NODE) {
                if (BREAKS.test(node.nodeName.toUpperCase())) pending = true;
                continue;
            }
            const owner = node.parentElement;
            if (!owner || !node.nodeValue || !visible(owner)) continue;
            const here = owner.closest('p,li,td,th,h1,h2,h3,h4,h5,h6,div,section,article,blockquote,pre') || root;
            const gap = pending || (block && block !== here) ? '\n' : '';
            pending = false;
            block = here;
            parts.push(gap, node.nodeValue);
            starts.push(length + gap.length);
            length += gap.length + node.nodeValue.length;
            nodes.push(node);
        }
        const hay = parts.join('');
        const at = (index) => {
            for (let i = nodes.length - 1; i >= 0; i--) {
                if (index >= starts[i]) return [nodes[i], index - starts[i]];
            }
            return null;
        };
        let current = -1;
        if (selection && selection.rangeCount && selection.anchorNode && selection.anchorNode.nodeType === Node.TEXT_NODE) {
            const i = nodes.indexOf(selection.anchorNode);
            if (i >= 0) current = starts[i] + selection.anchorOffset;
        }
        const finder = new RegExp(text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
        const hits = [];
        let found;
        while ((found = finder.exec(hay)) !== null && hits.length < 20000) hits.push([found.index, found[0].length]);
        if (!hits.length) return 'none';
        let pick = null;
        if (forward) {
            pick = hits.find((hit) => hit[0] > current) || hits[0];
        } else {
            for (const hit of hits) if (hit[0] < current) pick = hit;
            pick = pick || hits[hits.length - 1];
        }
        const start = at(pick[0]), end = at(pick[0] + pick[1] - 1);
        if (!start || !end) return 'none';
        const range = document.createRange();
        range.setStart(start[0], start[1]);
        range.setEnd(end[0], end[1] + 1);
        if (selection) { selection.removeAllRanges(); selection.addRange(range); }
        // Centred on the match itself. The element holding it can be a whole
        // paragraph, so aiming at the element put a short match on a tall
        // block's middle rather than in the middle of the view; the range's
        // own box is the text that was actually found. Sideways only when the
        // match would otherwise be off the edge.
        const box = range.getBoundingClientRect();
        if (box.width || box.height) {
            const dx = box.left < 0 || box.right > window.innerWidth
                ? box.left + box.width / 2 - window.innerWidth / 2 : 0;
            window.scrollBy(dx, box.top + box.height / 2 - window.innerHeight / 2);
        } else {
            // No box at all (an engine that will not measure a range yet): the
            // match's own element is the only thing left to aim at.
            const e = start[0].parentElement;
            if (e) e.scrollIntoView({ block: 'center', inline: 'nearest' });
        }
        return String(pick[0]);
    })()"#;

    /// One step through the matches, in the direction asked for.
    ///
    /// The engine's own step found the same text again and left the selection
    /// where it was, and a fresh search does the same, so a second press of
    /// Return moved nothing. The step is done in the page instead, over the
    /// text the reader can see: it selects the match it lands on and brings it
    /// to the middle of the view, which is also what makes the step visible.
    /// The count still comes from the engine in `find_query`.
    pub fn find_step(&self, text: &str, forward: bool) {
        let Some(view) = self.view(self.active_id()) else { return };
        if text.is_empty() {
            if let Some(controller) = view.find_controller() {
                controller.search("", 0, 0);
            }
        }
        let quote = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        // The flag is filled in first: the query comes next, and a query that
        // happened to spell the other marker would otherwise be replaced too.
        let script = Self::FIND_STEP_JS
            .replacen("__FORWARD__", if forward { "true" } else { "false" }, 1)
            .replacen("__TEXT__", &quote, 1);
        crate::js::run(&view, &script, |_| {});
    }

    pub fn find_query(&self, text: &str) {
        let Some(view) = self.view(self.active_id()) else { return };
        let Some(controller) = view.find_controller() else { return };
        if text.is_empty() {
            controller.search("", 0, 0);
            controller.count_matches("", 0, 0);
        } else {
            let options = webkit2gtk::FindOptions::CASE_INSENSITIVE.bits();
            controller.count_matches(text, options, 20_000);
            controller.search(text, options, 200);
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
                let first = state.bookmarks.is_empty();
                state.bookmarks.insert(
                    0,
                    crate::store::Bookmark {
                        url: url.clone(),
                        title: if title.is_empty() { url } else { title },
                    },
                );
                // Keeping the first page turns the bar on, so the thing just
                // kept is visible. After that the switch is the person's: it
                // is read with `&&`, so off means off even with pages kept.
                if first {
                    let mut prefs = self.prefs.borrow_mut();
                    prefs.show_bookmarks_bar = true;
                    prefs.save();
                }
                self.flash_toast("Kept");
            }
            Records::save_bookmarks(&state.bookmarks);
        }
        self.render_bmarks();
        if self.panels.borrow().open.clone() == Some(PanelKind::Bookmarks) {
            self.open_panel(PanelKind::Bookmarks);
        }
    }

    pub fn reader(&self) {
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
        let items: Vec<(&str, MenuItem)> = vec![
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
        let items: Vec<(&str, MenuItem)> = vec![
            ("New Tab", Box::new(|app: &App| {
                app.new_tab(None, false, false);
            })),
            ("New Private Tab", Box::new(|app: &App| {
                app.new_tab(None, true, false);
            })),
            (
                if self.reading.get() { "Leave Reading Mode" } else { "Reading Mode" },
                Box::new(|app: &App| app.reader()),
            ),
            ("Find in Page", Box::new(|app: &App| app.toggle_find())),
            ("Bookmarks", Box::new(|app: &App| app.toggle_panel(PanelKind::Bookmarks))),
            ("History", Box::new(|app: &App| app.toggle_panel(PanelKind::History))),
            ("Downloads", Box::new(|app: &App| app.toggle_panel(PanelKind::Downloads))),
            ("Extensions", Box::new(|app: &App| app.toggle_panel(PanelKind::Extensions))),
            ("Settings", Box::new(|app: &App| app.toggle_panel(PanelKind::Settings))),
            ("Shortcuts", Box::new(|app: &App| app.toggle_panel(PanelKind::Shortcuts))),
            ("Blocker Off Here", Box::new(|app: &App| app.toggle_blocker())),
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
        // At the pointer, not in the corner: a menu that opens far from where
        // you clicked reads as a different menu.
        menu.show_all();
        menu.popup_at_pointer(None);
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
            if let Some(tab) = state.tab_mut(id) {
                if !url.is_empty() && tab.url != url {
                    tab.url = url.to_string();
                }
                tab.title.clear();
                tab.failed = true;
            }
        }
        self.update_row_title(id);
        if id != self.active_id() {
            // Not the page being looked at: the error is remembered above and
            // shows when its tab is selected.
            self.render_rows();
            return;
        }
        self.ui.error_title.set_text("That page did not open");
        self.ui.error_reason.set_text(reason);
        self.ui.error_address.set_text(url);
        self.ui.error_retry.set_label(error_action(reason));
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

    /// The failed address, asked for again. For a server error or a dead
    /// connection the address is still the right one, so this is the whole
    /// retry; a 4xx goes the other way, through error_pressed.
    pub fn retry(&self) {
        let id = self.active_id();
        let url = self
            .state
            .borrow()
            .errors
            .get(&id)
            .map(|error| error.url.clone())
            .unwrap_or_default();
        self.state.borrow_mut().errors.remove(&id);
        if !url.is_empty() {
            self.navigate(id, &url);
        }
    }

    /// The one button on the error page. A 4xx is the address's fault, so the
    /// way out is a different address in the field; anything else is worth
    /// asking for again. The failure is read when the button is pressed, so
    /// the caption and the work cannot drift apart.
    pub fn error_pressed(&self) {
        let id = self.active_id();
        let (reason, url) = self
            .state
            .borrow()
            .errors
            .get(&id)
            .map(|error| (error.reason.clone(), error.url.clone()))
            .unwrap_or_default();
        if !error_is_client(&reason) {
            self.retry();
            return;
        }
        // Dropping the failure puts the browser back on the tab, and show_error
        // keeps what it already has, so a stale one would answer the next
        // failure with this address's reason.
        let had = self.state.borrow_mut().errors.remove(&id).is_some();
        if had {
            self.render();
        }
        self.open_omni(true);
        if !url.is_empty() {
            // The field abbreviates a page you are on; an address that did not
            // open has to be whole, or Return would search for "/missing".
            self.omni.entry.set_text(&url);
            self.omni.entry.select_region(0, -1);
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
        // A rebuild of the suggestion list, or a pick that changes a row's
        // fill, leaves an allocation pending, and GTK silently drops a draw
        // call on a widget that needs one. Run the pass until nothing is
        // pending any more, or a capture of a state reached by a keystroke
        // documents the state before it.
        for _ in 0..6 {
            self.ui.window.check_resize();
            self.ui.overlay.check_resize();
            while gtk::events_pending() {
                gtk::main_iteration_do(false);
            }
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
            // Square, because the window is: a rounded clip here cut one
            // corner off every picture. No clip at all, in fact — the shell
            // fills the surface, and a clip set on a context that later saves
            // and restores per widget leaked into the page composite and left
            // the column the colour of the band above it.
            context.reset_clip();
            context.set_operator(cairo::Operator::Over);
            context.translate(offset.0 as f64, offset.1 as f64);

            // 1. the frame and the column. Only the shell: drawing the
            // overlay itself would walk into its overlay children, and the page
            // is painted over that in step 2, so the field's scrim would end up
            // underneath the page it is meant to cover.
            // The shell is drawn without its WebView. The page is composited
            // from the engine's own snapshot in step 2, and a GTK draw of the
            // WebView paints a stale copy of the same page at the same place:
            // two pictures of one page, half a pixel apart, which reads as a
            // ghost behind the field.
            let page_widget = self.view(self.active_id()).map(|view| view.upcast::<gtk::Widget>());
            draw_shell(&context, &self.ui.shell, &self.ui.overlay, page_widget.as_ref());

            // 2. the page, from the engine's own snapshot.
            if let Some((page, x, y, page_width, page_height)) = page.as_ref() {
                let _ = context.save();
                context.rectangle(*x as f64, *y as f64, *page_width as f64, *page_height as f64);
                context.clip();
                context.translate(*x as f64, *y as f64);
                let _ = context.set_source_surface(page, 0.0, 0.0);
                // Source, not Over. A snapshot is not opaque everywhere: a page
                // with no background of its own leaves holes, and painting it
                // over whatever is already there leaves the frame's earlier
                // copy of the same page showing through those holes.
                context.set_operator(cairo::Operator::Source);
                let _ = context.paint();
                context.set_operator(cairo::Operator::Over);
                let _ = context.restore();
            }

            // 2b. the scrim, painted rather than drawn. Its widget is a
            // GtkDrawingArea inside a GtkFixed, and GTK drops a draw call on a
            // child whose allocation is still pending, which is exactly the
            // state a layer is in at capture time. A solid rectangle over the
            // page is the whole of what the scrim is, so fill it here.
            if self.omni.visible.get() || self.panels.borrow().open.is_some() {
                let (red, green, blue, alpha) = crate::ui::scrim_colour(&self.palette.borrow());
                context.set_source_rgba(red, green, blue, alpha);
                let column = self.ui.side.allocated_width().max(0) as f64;
                context.rectangle(column, 0.0, width as f64 - column, height as f64);
                let _ = context.fill();
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
        cut_top_left(&surface, self.palette.borrow().desk());
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
            Some(app) => {
                app.last_modifiers.set(event.state());
                crate::keys::handle(&app, event)
            }
            None => crate::gtkx::inh(false),
        });
        {
            // "Follow the desktop" has to follow it while running.
            let weak = self.weak();
            if let Some(settings) = gtk::Settings::default() {
                settings.connect_notify_local(Some("gtk-application-prefer-dark-theme"), move |_, _| {
                    if let Some(app) = weak.upgrade() {
                        app.follow_desktop();
                    }
                });
            }
        }

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

        // Drag the window by the band at the top of the column. The three
        // controls sit inside it, so they swallow their own clicks first;
        // everything from the last control to the page is the handle.
        {
            let weak = self.weak();
            self.ui.top.connect_button_press_event(move |_, event| {
                if let Some(app) = weak.upgrade() {
                    app.begin_drag(event);
                }
                crate::gtkx::inh(false)
            });
        }
        // The band above the window is chrome, so it drags the window and a
        // double click on it maximises, exactly as the strip inside the column
        // does. A band that looks like a title bar and does nothing is worse
        // than no band.
        {
            let weak = self.weak();
            self.ui.desk.connect_button_press_event(move |_, event| {
                if let Some(app) = weak.upgrade() {
                    if event.event_type() == gtk::gdk::EventType::DoubleButtonPress {
                        app.toggle_maximised();
                    } else {
                        app.begin_drag(event);
                    }
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
                    app.omni.chip_for_text(&entry.text());
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

        // Find: Return goes to the next match, Shift Return to the one before,
        // which is what the two small arrows do and what every other browser
        // does with the key.
        {
            let entry = self.find.borrow().entry.clone();
            let weak = self.weak();
            entry.connect_activate(move |entry| {
                if let Some(app) = weak.upgrade() {
                    let backward = app.last_modifiers.get().contains(gtk::gdk::ModifierType::SHIFT_MASK);
                    app.find_step(&entry.text(), !backward);
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
        // The error page is a Box, and a Box has no window of its own, so
        // there is no press event to give it: the press lands on the window
        // and is offered to nothing below it, so the door is cut here instead.
        // The gate is the error being the child that is drawn, the page's own
        // frame, and anything floating over it, so a press on a real page, on
        // the field or on the button is not answered twice.
        {
            let weak = self.weak();
            self.ui.window.connect_button_press_event(move |_, event| {
                let Some(app) = weak.upgrade() else { return crate::gtkx::inh(false) };
                if app.ui.stack.visible_child_name().as_deref() != Some("error") {
                    return crate::gtkx::inh(false);
                }
                // The field, the find bar and a panel all float over the page.
                // A press there belongs to them, not to the page underneath.
                if app.omni.visible.get()
                    || app.find.borrow().visible
                    || app.panels.borrow().open.is_some()
                {
                    return crate::gtkx::inh(false);
                }
                let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() else {
                    return crate::gtkx::inh(false);
                };
                if button.button() != 1 {
                    return crate::gtkx::inh(false);
                }
                // Inside the page's own frame, and not the button, which has
                // its own click to answer with.
                let (x, y) = button.position();
                let page: &gtk::Widget = app.ui.error_holder.upcast_ref();
                let retry: &gtk::Widget = app.ui.error_retry.upcast_ref();
                if !over_widget(page, &app.ui.window, x, y) || over_widget(retry, &app.ui.window, x, y) {
                    return crate::gtkx::inh(false);
                }
                // The same door as the blank page: the field, over this page, so
                // an address can be typed without knowing that Ctrl L exists.
                // Pressing the button is left to the button, which asks again.
                app.open_omni(true);
                crate::gtkx::inh(true)
            });
        }
        {
            let weak = self.weak();
            self.ui.error_retry.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.error_pressed();
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

/// The most tabs one window will hold. See `new_tab`.
pub const TAB_LIMIT: usize = 40;

pub fn install_self(app: &AppRef) {
    CURRENT.with(|c| *c.borrow_mut() = Some(app.clone()));
    // Only now can `weak()` see the app, so this is where the minute's tick is
    // armed. See `start_memory_timer`.
    app.start_memory_timer();
}

/// Draw a widget where it stands inside an ancestor.
/// Draw a widget and everything inside it, windowed or not.
///
/// GTK draws a widget with its own `GdkWindow` in that window, and a container's
/// draw handler does not walk into such children, so calling `draw` on the
/// holder alone paints nothing: the field, the find bar and every panel have
/// entries and boxes with their own windows. Walking the tree and drawing each
/// one at its own offset is what makes a capture of a layer look like the layer.
/// The frame, without the page inside it.
///
/// The page is composited separately from the engine's own snapshot, because a
/// GTK draw of a WebView does not produce the page: it produces whatever the
/// widget last cached. Drawing both put two copies of one page in the picture,
/// offset by a fraction of a pixel, which showed as a ghost of a button behind
/// the address field. `skip` is left out of the walk entirely.
/// Whether a widget is worth drawing at all.
///
/// Two ways a widget can be in the tree and not on the screen. A GtkStack
/// keeps every page realised, so the error page is walked even while the blank
/// page is showing — and its label, at its natural width, painted "That page
/// did not open" across the top of every picture. And an overlay layer that
/// has never been allocated sits at 1x1 with its children at their natural
/// size, which paints the same way. A 1px separator is legitimate, so only a
/// widget small in BOTH directions is treated as unallocated.
fn not_shown(widget: &gtk::Widget) -> bool {
    if !widget.is_visible() {
        return true;
    }
    if widget.allocated_width() < 2 && widget.allocated_height() < 2 {
        return true;
    }
    if let Some(parent) = widget.parent() {
        if let Some(stack) = parent.dynamic_cast_ref::<gtk::Stack>() {
            return stack.visible_child().as_ref() != Some(widget);
        }
    }
    false
}

fn draw_shell(
    context: &cairo::Context,
    widget: &impl IsA<gtk::Widget>,
    ancestor: &impl IsA<gtk::Widget>,
    skip: Option<&gtk::Widget>,
) {
    let widget = widget.clone().upcast::<gtk::Widget>();
    if skip == Some(&widget) || not_shown(&widget) {
        return;
    }
    let offset = widget.translate_coordinates(ancestor, 0, 0).unwrap_or((0, 0));
    paint_one(context, &widget, offset);
    if layout_only(&widget) {
        if let Some(container) = widget.dynamic_cast_ref::<gtk::Container>() {
            for child in container.children() {
                draw_shell(context, &child, ancestor, skip);
            }
        }
    }
}

/// The window's own top-left corner, cut the way the original's is.
///
/// The band above the window is the desktop's, and the original's white sheet
/// curves away from it at one corner only. A CSS radius cannot do it: GTK
/// rounds a widget's background but not its border, so the column's hairline
/// would be drawn straight through the curve. So it is cut here, out of the
/// finished picture, with the band's colour put back into the sliver.
fn cut_top_left(surface: &cairo::Surface, band: crate::palette::Rgb) {
    let r = 14.0;
    let Ok(context) = cairo::Context::new(surface) else { return };
    // Match the frame in the current appearance, not a hardcoded light grey.
    context.set_source_rgb(band.0 as f64 / 255.0, band.1 as f64 / 255.0, band.2 as f64 / 255.0);
    context.set_operator(cairo::Operator::Source);
    context.move_to(0.0, 23.0);
    context.line_to(r, 23.0);
    context.arc_negative(r, 23.0 + r, r, -std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    context.line_to(0.0, 23.0);
    context.close_path();
    let _ = context.fill();
}

fn draw_at(context: &cairo::Context, widget: &impl IsA<gtk::Widget>, ancestor: &impl IsA<gtk::Widget>) {
    let widget = widget.clone().upcast::<gtk::Widget>();
    draw_into(context, &widget, ancestor);
}

fn opacity_of(widget: &gtk::Widget) -> f64 {
    if widget.style_context().has_class("close") {
        // `.row:hover button.close` is the only rule that shows it, and a
        // capture is taken with no pointer over a row.
        let hovered = widget
            .parent()
            .map(|parent| parent.state_flags().contains(gtk::StateFlags::PRELIGHT))
            .unwrap_or(false);
        if !hovered {
            return 0.0;
        }
    }
    widget.opacity()
}

/// A container whose whole appearance is its own fill and border, with every
/// mark inside it belonging to a child widget.
///
/// This is the distinction the capture turns on. `gtk_widget_draw` on one of
/// these paints the children too, and painting them again by hand — which is
/// what walking the tree used to do — draws every label twice, a fraction of a
/// pixel apart. Two passes of the same glyphs is a bold weight nobody asked
/// for: the stylesheet said `font-weight: normal` and the picture said bold.
/// So these are painted with `gtk_render_background`/`frame`, which is their
/// own share of the work, and their children are drawn once each, by
/// themselves, where a label renders at the weight it was given.
fn layout_only(widget: &gtk::Widget) -> bool {
    widget.is::<gtk::Box>()
        || widget.is::<gtk::EventBox>()
        || widget.is::<gtk::Grid>()
        || widget.is::<gtk::Fixed>()
        || widget.is::<gtk::Overlay>()
        || widget.is::<gtk::Stack>()
        || widget.is::<gtk::Paned>()
        || widget.is::<gtk::ScrolledWindow>()
}

fn draw_into(context: &cairo::Context, widget: &gtk::Widget, ancestor: &impl IsA<gtk::Widget>) {
    if not_shown(widget) {
        return;
    }
    // `gtk_widget_draw` ignores the widget's opacity, so a control the
    // stylesheet hides until hover (a tab's close button) would be drawn as
    // though it were showing. A capture has to respect what the stylesheet
    // asked for, or it documents a state the user never sees.
    let opacity = opacity_of(widget);
    if opacity <= 0.001 {
        return;
    }
    let offset = widget.translate_coordinates(ancestor, 0, 0).unwrap_or((0, 0));
    // A capture is best effort: a surface that is already unusable has
    // nothing to report that the caller could act on.
    //
    // Only opacity needs a group. A translate and a clip put on the context
    // here and taken off again after the paint were not balanced against the
    // saves inside `paint_one`, and the leftovers accumulated down the walk
    // until the page composite ran against a stale transform.
    if opacity < 0.999 {
        let _status = context.save();
        context.translate(offset.0 as f64, offset.1 as f64);
        context.push_group();
        paint_one(context, widget, (0, 0));
        let _status = context.pop_group_to_source();
        let _status = context.paint_with_alpha(opacity);
        let _status = context.restore();
    } else {
        paint_one(context, widget, offset);
    }
    // A layout container has had its own fill and border painted and nothing
    // else, so its children are still owed a draw. Anything else has already
    // painted its children itself, at their own weights, and drawing them
    // again is how a normal label came out bold.
    if layout_only(widget) {
        if let Some(container) = widget.dynamic_cast_ref::<gtk::Container>() {
            for child in container.children() {
                draw_into(context, &child, ancestor);
            }
        }
    }
}

/// One widget's own share of the picture.
fn paint_one(context: &cairo::Context, widget: &gtk::Widget, offset: (i32, i32)) {
    let style = widget.style_context();
    let _status = context.save();
    context.translate(offset.0 as f64, offset.1 as f64);
    let (width, height) = (
        widget.allocated_width() as f64,
        widget.allocated_height() as f64,
    );
    if layout_only(widget) {
        gtk::render_background(&style, context, 0.0, 0.0, width, height);
        gtk::render_frame(&style, context, 0.0, 0.0, width, height);
        let _status = context.restore();
        return;
    }
    // A viewport's draw routine paints its complete child, including rows
    // below its own allocation. ScrolledWindow remains layout-only so the
    // viewport gets an explicit clip in the capture walk.
    if widget.is::<gtk::Viewport>() {
        context.rectangle(0.0, 0.0, width, height);
        context.clip();
        let _ = widget.draw(context);
        let _status = context.restore();
        return;
    }
    // A leaf is drawn with its own style context pushed onto the stack.
    // `gtk_widget_draw` alone resolves the colour against whatever context is
    // current, so a tab title on the active row came out the resting grey
    // instead of near-black, and never bold. `render_layout` takes the colour
    // and the weight from the context it is given.
    if let Some(label) = widget.dynamic_cast_ref::<gtk::Label>() {
        style.save();
        style.set_state(widget.state_flags());
        gtk::render_background(&style, context, 0.0, 0.0, width, height);
        gtk::render_frame(&style, context, 0.0, 0.0, width, height);
        if let Some(layout) = label.layout() {
            let (lw, lh) = layout.pixel_size();
            let x = label.xalign() as f64 * (width - lw as f64).max(0.0);
            let y = ((height - lh as f64) / 2.0).max(0.0);
            gtk::render_layout(&style, context, x, y, &layout);
        }
        let _status = context.restore();
        return;
    }
    widget.draw(context);
    let _status = context.restore();
}

/// Whether a point given in window coordinates lands on a widget. A widget
/// that has not been allocated yet answers no: a click cannot be on it.
fn over_widget(widget: &gtk::Widget, window: &gtk::Window, x: f64, y: f64) -> bool {
    match widget.translate_coordinates(window, 0, 0) {
        Some((left, top)) => {
            let right = left + widget.allocated_width();
            let bottom = top + widget.allocated_height();
            x >= left as f64 && y >= top as f64 && x < right as f64 && y < bottom as f64
        }
        None => false,
    }
}

/// The status in a failure's sentence. src/web.rs writes the code into the
/// reason and nowhere else, so it is read back rather than becoming a field in
/// a struct another builder owns.
fn http_status(reason: &str) -> Option<u16> {
    reason
        .split(|c: char| !c.is_ascii_digit())
        .find(|run| run.len() == 3)
        .and_then(|run| run.parse().ok())
}

/// A 4xx is the address's fault: there is nothing there to ask again.
fn error_is_client(reason: &str) -> bool {
    matches!(http_status(reason), Some(status) if (400..500).contains(&status))
}

/// What the one button on the error page should say.
fn error_action(reason: &str) -> &'static str {
    if error_is_client(reason) {
        "Change address"
    } else {
        "Try again"
    }
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

    #[test]
    fn a_missing_page_offers_a_new_address_and_a_broken_one_another_try() {
        // The reason is the only place a 4xx announces itself, so this pins the
        // sentences src/web.rs writes: if they change shape, the button would
        // quietly go back to reloading a dead address.
        assert_eq!(error_action("There is nothing at this address (404)."), "Change address");
        assert_eq!(error_action("The site would not let this page be read (403)."), "Change address");
        assert_eq!(error_action("The site broke while answering (503)."), "Try again");
        assert_eq!(error_action("The site refused the connection."), "Try again");
        assert_eq!(error_action("That address could not be found."), "Try again");
        assert_eq!(error_action("The page stopped arriving. (There is no network to reach it on.)"), "Try again");
    }

    #[test]
    fn a_number_in_a_sentence_is_not_by_itself_a_client_error() {
        assert_eq!(http_status("The site answered with 100."), Some(100));
        assert_eq!(http_status("The site refused the connection."), None);
        // A port is a number too, so the test is not that it parses nothing but
        // that what it parses is never a client error: a 404's reload must not
        // come from an address that happens to have three digits in it.
        assert_eq!(error_action("There is nothing at 127.0.0.1:8099 (going nowhere)."), "Try again");
    }
}
