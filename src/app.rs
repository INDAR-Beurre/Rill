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
use webkit2gtk::{FindControllerExt, PrintOperationExt, UserContentManagerExt, WebInspectorExt, WebViewExt};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

pub type TabId = u64;
pub type AppRef = Rc<App>;

thread_local! {
    static CURRENT: RefCell<Option<AppRef>> = const { RefCell::new(None) };
    static FLUSH_PENDING: Cell<bool> = const { Cell::new(false) };
    static TONE_SHEETS: RefCell<std::collections::HashMap<usize, webkit2gtk::UserStyleSheet>> =
        RefCell::new(std::collections::HashMap::new());
}

// One save timer for history, hidden and session. A change marks its file
// dirty and arms the timer if no timer is armed yet. When the timer fires it
// writes only the dirty files. Before this, history and session each had their
// own 2s timer, so one visit could write session.json twice.
static HISTORY_DIRTY: AtomicBool = AtomicBool::new(false);
static SESSION_DIRTY: AtomicBool = AtomicBool::new(false);
static PERSIST_PENDING: AtomicBool = AtomicBool::new(false);

/// The files one run of the save timer writes. It is worked out once, at the
/// start of the run, by `take_persist_plan`. Then tests can check the grouping
/// rules without a display, a timer or the disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PersistPlan {
    /// history.json and hidden.json.
    history: bool,
    /// session.json.
    session: bool,
}

/// Reads the dirty flags for one timer run.
///
/// The history flag is cleared here, before the write. A visit that arrives
/// while the write runs marks it again, and a failed write sets it again, so
/// no change is lost. The session flag is only read, not cleared:
/// `save_session` clears it itself, so a direct save between two timer runs
/// (pin toggle, window close, bench) leaves nothing for the timer to write.
fn take_persist_plan(history_dirty: &AtomicBool, session_dirty: &AtomicBool) -> PersistPlan {
    PersistPlan {
        history: history_dirty.swap(false, Ordering::Relaxed),
        session: session_dirty.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
mod persist_plan_tests {
    use super::{take_persist_plan, PersistPlan};
    use std::sync::atomic::{AtomicBool, Ordering};

    const NOTHING: PersistPlan = PersistPlan { history: false, session: false };
    const BOTH: PersistPlan = PersistPlan { history: true, session: true };

    /// What save_session does to the flag when its write lands.
    fn session_written(session: &AtomicBool) {
        session.store(false, Ordering::Relaxed);
    }

    #[test]
    fn a_burst_of_changes_writes_each_file_once() {
        let (history, session) = (AtomicBool::new(false), AtomicBool::new(false));
        // Five visits before the timer fires mark the same two flags.
        for _ in 0..5 {
            history.store(true, Ordering::Relaxed);
            session.store(true, Ordering::Relaxed);
        }
        assert_eq!(take_persist_plan(&history, &session), BOTH);
        session_written(&session);
        // The next run has nothing left to write.
        assert_eq!(take_persist_plan(&history, &session), NOTHING);
    }

    #[test]
    fn a_direct_save_leaves_the_timer_nothing_to_write() {
        let (history, session) = (AtomicBool::new(false), AtomicBool::new(true));
        // A pin toggle or window close saves the session at once.
        session_written(&session);
        assert_eq!(take_persist_plan(&history, &session), NOTHING);
    }

    #[test]
    fn a_tab_switch_writes_the_session_but_not_history() {
        let (history, session) = (AtomicBool::new(false), AtomicBool::new(true));
        assert_eq!(
            take_persist_plan(&history, &session),
            PersistPlan { history: false, session: true }
        );
    }

    #[test]
    fn a_failed_history_write_is_tried_again_by_the_next_run() {
        let (history, session) = (AtomicBool::new(true), AtomicBool::new(false));
        assert!(take_persist_plan(&history, &session).history);
        // The write failed (full disk), so the timer marks history dirty again.
        history.store(true, Ordering::Relaxed);
        assert!(take_persist_plan(&history, &session).history);
    }

    #[test]
    fn a_failed_session_write_stays_marked() {
        let (history, session) = (AtomicBool::new(false), AtomicBool::new(true));
        assert!(take_persist_plan(&history, &session).session);
        // save_session clears the flag, fails, and marks it again.
        session.store(false, Ordering::Relaxed);
        session.store(true, Ordering::Relaxed);
        assert!(take_persist_plan(&history, &session).session);
    }
}

thread_local! {
    // The id of the armed "Let tabs sleep" timer, or None. It lives at module
    // level so start_memory_timer() and stop_memory_timer() share one slot.
    // The timer is armed only while the feature is on, and there is never
    // more than one.
    static MEMORY_TIMER: std::cell::RefCell<Option<glib::SourceId>> =
        const { std::cell::RefCell::new(None) };
}

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
    pub progress: f64,
    pub can_back: bool,
    pub can_forward: bool,
    pub failed: bool,
    pub asleep: bool,
    pub row: Option<TabRow>,
    pub pin: Option<ui::PinWidget>,
    pub last_seen: u64,
    /// Last time the page was making sound. Runtime only, never saved.
    pub last_audible: Option<std::time::Instant>,
    /// The page has an element in full screen (a video) right now.
    pub page_full: bool,
    /// The window was already full screen (F11) when the page asked, so leaving
    /// the page's full screen can give the window back as it was.
    pub window_was_full: bool,
}

impl Tab {
    pub fn letter(&self) -> char {
        self.pin_mark().chars().next().unwrap_or('N')
    }

    /// A one- or two-letter mark for a pinned tab. Titles like Next, Notion
    /// and Notes all started with N, so the old single initial made three
    /// different pins look like the same pin. The first letter of each of the
    /// first two words is enough to tell them apart without shrinking the
    /// type in a 35px tile.
    pub fn pin_mark(&self) -> String {
        let title = self.title.trim();
        let words: Vec<&str> = title.split_whitespace().collect();
        let mut mark = String::new();
        for word in &words {
            if let Some(letter) = word.chars().find(|c| c.is_alphabetic()) {
                mark.push(letter);
            }
            if mark.chars().count() == 2 {
                break;
            }
        }
        if !mark.is_empty() {
            return mark.to_uppercase();
        }
        // A page that has not sent its title yet (or never will) used to show
        // 'N' for "New tab" even with an address loaded, so every loading tab
        // looked the same. The host decides first now, with any leading
        // "www." skipped, and 'N' is kept only for a tab with no address.
        let host = crate::shield::host_of(&self.url);
        let host = host.strip_prefix("www.").unwrap_or(host.as_str());
        let text = if host.is_empty() { "New tab" } else { host };
        return text.chars().find(|c| c.is_alphanumeric()).unwrap_or('N').to_uppercase().to_string();
    }

    #[allow(dead_code)]
    fn legacy_letter(&self) -> char {
        let title = self.title.trim();
        if let Some(first) = title.chars().find(|c| c.is_alphabetic()) {
            return first;
        }
        // A page that has not sent its title yet (or never will) used to show
        // 'N' for "New tab" even with an address loaded, so every loading tab
        // looked the same. The host decides first now, with any leading
        // "www." skipped, and 'N' is kept only for a tab with no address.
        let host = crate::shield::host_of(&self.url);
        let host = host.strip_prefix("www.").unwrap_or(host.as_str());
        let text = if host.is_empty() { "New tab" } else { host };
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
            progress: 0.0,
            can_back: false,
            can_forward: false,
            failed: false,
            asleep: false,
            row: None,
            pin: None,
            last_seen: crate::store::now_secs(),
            last_audible: None,
            page_full: false,
            window_was_full: false,
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
    #[serde(skip)]
    pub id: u64,
    pub name: String,
    pub path: String,
    pub done: bool,
    #[serde(default)]
    pub failed: bool,
    #[serde(default)]
    pub got: u64,
    #[serde(default)]
    pub total: u64,
    /// Started by a private tab. The row is listed for this session only and is
    /// never written to downloads.json. The file itself is still in Downloads,
    /// because the person asked for it to be saved.
    #[serde(skip)]
    pub private: bool,
}

pub struct ErrorPage {
    pub reason: String,
    pub url: String,
}

/// A window a page asked for without a click and was refused. Kept per tab so
/// the card can offer to open it; the address is the one the page asked for.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockedPopup {
    pub url: String,
    /// How many windows this tab has been refused, so the headline can count them.
    pub count: usize,
}

impl BlockedPopup {
    /// The line the card shows: the blocked address, cut so a long one stays
    /// on one line. The full address is what "Open it anyway" will load.
    pub fn label(&self) -> String {
        let chars: Vec<char> = self.url.chars().collect();
        if chars.len() <= 60 {
            return self.url.clone();
        }
        let head: String = chars[..57].iter().collect();
        format!("{head}...")
    }

    /// What the tab's card says: one blocked window, or how many were refused.
    pub fn headline(count: usize) -> String {
        match count {
            0 => String::new(),
            1 => "Pop-up blocked".to_string(),
            n => format!("{n} pop-ups blocked"),
        }
    }
}

/// Records the refused window for a tab. Only the last one is kept, because the
/// card offers one address, and a page that opens many would fill it with noise.
pub fn note_blocked(blocked: &mut BTreeMap<TabId, BlockedPopup>, tab: TabId, url: &str) {
    if url.trim().is_empty() {
        return;
    }
    let count = blocked.get(&tab).map(|p| p.count).unwrap_or(0) + 1;
    blocked.insert(tab, BlockedPopup { url: url.to_string(), count });
}

/// Drops the record a card was showing, but only if it is still that record:
/// a newer refusal that arrived meanwhile keeps its own card.
pub fn forget_note(blocked: &mut BTreeMap<TabId, BlockedPopup>, tab: TabId, shown: &BlockedPopup) {
    if blocked.get(&tab) == Some(shown) {
        blocked.remove(&tab);
    }
}

fn forget_blocked(app: &App, tab: TabId, shown: &BlockedPopup) {
    forget_note(&mut app.state.borrow_mut().blocked, tab, shown);
}

#[cfg(test)]
mod popup_tests {
    use super::*;

    #[test]
    fn the_last_refused_window_is_the_one_kept_for_its_tab() {
        let mut blocked = BTreeMap::new();
        note_blocked(&mut blocked, 3, "https://a.example/one");
        note_blocked(&mut blocked, 3, "https://a.example/two");
        note_blocked(&mut blocked, 4, "https://b.example/");
        let three = blocked.get(&3).expect("tab 3 has a record");
        assert_eq!(three.url, "https://a.example/two");
        assert_eq!(three.count, 2);
        assert_eq!(blocked.get(&4).map(|p| p.count), Some(1));
    }

    #[test]
    fn an_empty_address_is_not_recorded() {
        let mut blocked = BTreeMap::new();
        note_blocked(&mut blocked, 1, "   ");
        assert!(blocked.is_empty());
    }

    #[test]
    fn the_record_is_gone_once_its_card_is_used() {
        let mut blocked = BTreeMap::new();
        note_blocked(&mut blocked, 3, "https://a.example/");
        let shown = blocked.get(&3).cloned().expect("record");
        forget_note(&mut blocked, 3, &shown);
        assert!(!blocked.contains_key(&3));
    }

    #[test]
    fn a_newer_refusal_keeps_its_record_when_an_old_card_goes() {
        let mut blocked = BTreeMap::new();
        note_blocked(&mut blocked, 3, "https://a.example/");
        let shown = blocked.get(&3).cloned().expect("record");
        note_blocked(&mut blocked, 3, "https://a.example/two");
        forget_note(&mut blocked, 3, &shown);
        assert_eq!(blocked.get(&3).map(|p| p.count), Some(2));
    }

    #[test]
    fn the_wording_counts_the_pop_ups_a_tab_refused() {
        assert_eq!(BlockedPopup::headline(1), "Pop-up blocked");
        assert_eq!(BlockedPopup::headline(3), "3 pop-ups blocked");
    }

    #[test]
    fn a_long_address_is_cut_on_the_card_but_kept_whole() {
        let long = format!("https://x.example/{}", "a".repeat(100));
        let popup = BlockedPopup { url: long.clone(), count: 1 };
        assert!(popup.label().chars().count() <= 60);
        assert!(popup.label().ends_with("..."));
        assert_eq!(popup.url, long);
        let short = BlockedPopup { url: "https://x.example/".into(), count: 1 };
        assert_eq!(short.label(), "https://x.example/");
    }
}

pub const CLOSED_TAB_LIMIT: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
    pub index: usize,
}

pub fn push_closed_tab(stack: &mut Vec<ClosedTab>, tab: &Tab, index: usize) {
    if tab.private || tab.url.trim().is_empty() {
        return;
    }
    if stack.len() >= CLOSED_TAB_LIMIT {
        stack.remove(0);
    }
    stack.push(ClosedTab {
        url: tab.url.clone(),
        title: tab.title.clone(),
        pinned: tab.pinned,
        index,
    });
}

pub fn restore_closed_tab(tabs: &mut Vec<Tab>, next_id: &mut TabId, closed: ClosedTab) -> TabId {
    let id = *next_id;
    *next_id += 1;
    let mut tab = Tab::new(id);
    tab.url = closed.url;
    tab.title = closed.title;
    tab.pinned = closed.pinned;
    tab.asleep = false;
    let at = closed.index.min(tabs.len());
    tabs.insert(at, tab);
    id
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
    /// The last window each tab asked for and was refused, for its card.
    pub blocked: BTreeMap<TabId, BlockedPopup>,
    pub closed_tabs: Vec<ClosedTab>,
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
    /// Whether the window is full screen, as the window last reported it in
    /// window-state-event. Read instead of the window, which may still be changing.
    pub window_full: std::cell::Cell<bool>,
    /// Zoom per host, for this session only. Kept in memory: a site's zoom is
    /// the person's choice, but writing it down is a new file this does not need.
    pub zoom_by_host: RefCell<BTreeMap<String, f64>>,
    /// Temporary view-source files keyed by tab id, unlinked on tab close or quit.
    pub source_files: RefCell<BTreeMap<TabId, std::path::PathBuf>>,
    /// Last computed pin column count, so divider drags only rebuild pins on change.
    pub last_pin_across: Cell<usize>,
    /// What the pin strip was last built from: the column count and, for each
    /// pinned tab, its id, letter and title. render() runs on every tab event
    /// (title, load, select), and the strip used to be torn down and rebuilt
    /// each time, which made new widgets, tooltips, ATK names and click
    /// handlers even when no pin had changed (Gemini R60 perf). The active tab
    /// is not stored here: a tab switch recolours the existing tiles.
    /// A favicon or theme change clears the signature.
    pub pin_signature: RefCell<Option<(usize, Vec<(TabId, String, String)>)>>,
    /// True while a column fold/unfold animation is driving the paned position.
    pub folding_anim: Cell<bool>,
}

/// The zoom after one step, kept between 40% and 300%.
pub fn zoom_step(current: f64, delta: f64) -> f64 {
    (current + delta).clamp(0.4, 3.0)
}

/// The key zoom is remembered under: the host, so every page on a site shares it.
pub fn zoom_key(url: &str) -> String {
    crate::shield::host_of(url)
}

pub fn record_zoom(zoom_by_host: &mut BTreeMap<String, f64>, private: bool, url: &str, level: f64) {
    if private {
        return;
    }
    let key = zoom_key(url);
    if !key.is_empty() {
        zoom_by_host.insert(key, level);
    }
}

pub fn pin_columns(usable_width: i32) -> usize {
    (((usable_width + 3) / (35 + 3)).max(1)) as usize
}

pub fn sanitize_selector(raw: &str) -> Option<&str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 400 {
        return None;
    }
    let bytes = trimmed.as_bytes();
    for (idx, &b) in bytes.iter().enumerate() {
        match b {
            b'<' | b'"' | b'\'' | b'{' | b'}' | b',' | b'\n' | b'\r' => return None,
            b'\\' if idx + 1 == bytes.len() => return None,
            _ => {}
        }
    }
    Some(trimmed)
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

pub fn chrome_address(url: &str, private: bool) -> String {
    if url.is_empty() {
        return if private { "Private tab" } else { "New tab" }.to_string();
    }
    let Some((scheme, address)) = url.split_once("://") else {
        return url.to_string();
    };
    let prefix = if scheme.eq_ignore_ascii_case("https") {
        ""
    } else if scheme.eq_ignore_ascii_case("http") {
        "http://"
    } else {
        return url.to_string();
    };
    let boundary = address.find(['/', '?', '#']).unwrap_or(address.len());
    let authority = address[..boundary].rsplit('@').next().unwrap_or_default();
    format!("{prefix}{authority}")
}

/// The path a field query should be matched as: a leading slash turns a
/// bare word search into a page name ("/office" means the /office page),
/// while ordinary words stay a search. Kept separate from plain_target so
/// matching and navigation cannot drift apart.
fn query_path(typed: &str) -> Option<&str> {
    let trimmed = typed.trim();
    if trimmed.starts_with('/') && !trimmed.contains(char::is_whitespace) {
        Some(trimmed)
    } else {
        None
    }
}

/// How well an open tab answers the text in the address field. An exact path
/// is the strongest signal: "/office" names that page, while a title that
/// merely mentions the word is a weaker hit. Keeping this pure makes the
/// ranking testable without a GTK window.
fn offer_strength(words: &[&str], title: &str, url: &str) -> i32 {
    if words.is_empty() {
        return 0;
    }
    let path = crate::shield::location_of(url).to_lowercase();
    let title = title.to_lowercase();
    let lower = url.to_lowercase();
    let exact = |hay: &str| words.iter().all(|word| hay.contains(*word));
    if exact(&path) {
        4
    } else if exact(&lower) {
        3
    } else if exact(&title) {
        2
    } else if crate::fuzzy::score(words, &path).is_some() {
        1
    } else {
        0
    }
}

fn tab_subtitle(url: &str, private: bool, asleep: bool) -> String {
    if private {
        return "Private browsing".to_string();
    }
    if url.is_empty() {
        return "Start exploring".to_string();
    }
    let host = crate::shield::host_of(url);
    let mut subtitle = if !host.is_empty() {
        host.strip_prefix("www.").unwrap_or(&host).to_string()
    } else if url.starts_with("file:") {
        "Local file".to_string()
    } else {
        "Browser page".to_string()
    };
    // Same-host pages used to collapse into identical rows: a local server
    // served /office, /titled and /missing, and every subtitle said only
    // 127.0.0.1. The path is a quiet suffix, not a second origin: the trusted
    // host still comes first, exactly as in the chrome's separate path label.
    let path = crate::shield::location_of(url);
    if path.len() > 1 && path.len() <= 24 {
        subtitle.push_str(&path);
    }
    if asleep { format!("{subtitle} · sleeping") } else { subtitle }
}

/// A failed tab's row says what failed, instead of falling back to the bare
/// host. Kept pure so the title can be tested without a GTK window.
fn error_tab_title(reason: &str, url: &str) -> String {
    let lead = if reason.contains("(404)") { "404" } else { "Didn't open" };
    let path = crate::shield::location_of(url);
    if path.len() > 1 {
        let short: String = path.chars().take(18).collect();
        format!("{lead} · {short}{}", if path.chars().count() > 18 { "…" } else { "" })
    } else {
        lead.to_string()
    }
}

#[cfg(test)]
mod chrome_tests {
    use super::*;

    #[test]
    fn address_display_preserves_insecure_scheme_and_port() {
        assert_eq!(chrome_address("https://example.com/", false), "example.com");
        assert_eq!(chrome_address("http://example.com:8080/a?b=1#c", false), "http://example.com:8080");
        assert_eq!(chrome_address("file:///tmp/readme.html", false), "file:///tmp/readme.html");
    }

    #[test]
    fn credentials_never_appear_in_the_browser_chrome() {
        assert_eq!(chrome_address("https://name:secret@example.com/a", false), "example.com");
        assert_eq!(chrome_address("HTTPS://name:secret@example.com/a", false), "example.com");
        assert_eq!(chrome_address("http://name:secret@example.com/", false), "http://example.com");
    }

    #[test]
    fn a_path_cannot_impersonate_the_displayed_origin() {
        assert_eq!(chrome_address("https://accounts.example.com.attacker.test/accounts.example.com/sign-in", false), "accounts.example.com.attacker.test");
        assert_eq!(chrome_address("https://example.com@attacker.test/example.com", false), "attacker.test");
        assert_eq!(chrome_address("http://[::1]:8080/path", false), "http://[::1]:8080");
    }

    #[test]
    fn a_path_query_is_not_treated_as_search_words() {
        assert_eq!(query_path("/office"), Some("/office"));
        assert_eq!(query_path("/two words"), None);
        assert_eq!(query_path("office"), None);
        assert_eq!(query_path("http://example.com/office"), None);
    }

    #[test]
    fn an_exact_path_is_ranked_above_a_title_that_merely_mentions_it() {
        let words = ["/office"];
        let wanted = offer_strength(&words, "Office Commun", "http://127.0.0.1:8099/office");
        let title_only = offer_strength(&words, "/office notes", "http://127.0.0.1:8099/titled?t=Notes");
        let unrelated = offer_strength(&words, "X", "http://127.0.0.1:8099/titled?t=X");
        assert!(wanted > title_only && title_only >= unrelated);
        assert_eq!(wanted, 4);
    }

    #[test]
    fn private_tabs_are_distinct_without_exposing_their_host_in_subtitles() {
        assert_eq!(chrome_address("", true), "Private tab");
        assert_eq!(chrome_address("", false), "New tab");
        assert_eq!(tab_subtitle("https://secret.example/", true, false), "Private browsing");
        assert_eq!(tab_subtitle("https://www.example.com/a", false, true), "example.com/a · sleeping");
    }

    #[test]
    fn same_host_tabs_get_distinct_subtitles() {
        assert_eq!(tab_subtitle("http://127.0.0.1:8099/office", false, false), "127.0.0.1/office");
        assert_eq!(tab_subtitle("http://127.0.0.1:8099/missing", false, false), "127.0.0.1/missing");
        assert_eq!(tab_subtitle("http://127.0.0.1:8099/", false, false), "127.0.0.1");
        assert_eq!(error_tab_title("There is nothing at this address (404).", "http://127.0.0.1:8099/missing"), "404 · /missing");
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
    // Decode the window icon with cairo's own PNG reader (cairo-rs is built
    // with the png feature), then turn the surface into a pixbuf for GTK.
    // Pixbuf::from_read goes through the gdk-pixbuf loader. On this system that
    // loader is glycin, which starts a sandboxed image process
    // (glycin-image-rs plus two bwrap) and keeps it alive for the whole session,
    // all to decode one PNG at startup.
    let icon_png = include_bytes!("../packaging/search.png");
    if let Ok(surface) = cairo::ImageSurface::create_from_png(&mut std::io::Cursor::new(icon_png.as_slice())) {
        let (width, height) = (surface.width(), surface.height());
        if let Some(icon) = gtk::gdk::pixbuf_get_from_surface(&surface, 0, 0, width, height) {
            window.set_icon(Some(&icon));
        }
    }
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
        // The launch snapshot is only a fallback. Saving it on close used to
        // overwrite every change made during the session (theme, fold,
        // divider width), so the live preferences on the app come first.
        let prefs_ref = prefs.clone();
        window.connect_delete_event(move |window, _event| {
            let mut saved = current()
                .and_then(|app| app.prefs.try_borrow().ok().map(|p| p.clone()))
                .unwrap_or_else(|| prefs_ref.clone());
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
    // The tab that was on top when the session was saved. A session file
    // written before records carried `active`, or one where the active tab
    // fell past the cap, has none, and the first tab is shown as before.
    let mut restored_active: Option<TabId> = None;
    if prefs.restore_session && !prefs.skip_restore {
        // Restore obeys the same cap as opening new tabs, so an oversized
        // or hand-edited session file can't start the browser with more
        // tabs than it would ever let you open.
        for record in Records::load_session().into_iter().take(TAB_LIMIT) {
            let mut tab = Tab::new(next_id);
            tab.url = record.url;
            tab.title = record.title;
            tab.pinned = record.pinned;
            tab.asleep = true;
            // A hand-edited file may mark more than one tab; the first wins.
            if record.active && restored_active.is_none() {
                restored_active = Some(next_id);
            }
            tabs.push(tab);
            next_id += 1;
        }
    }
    if tabs.is_empty() {
        tabs.push(Tab::new(next_id));
        next_id += 1;
    }
    let active = restored_active.unwrap_or(tabs[0].id);

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
        blocked: BTreeMap::new(),
        closed_tabs: Vec::new(),
    };

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
        window_full: std::cell::Cell::new(false),
        zoom_by_host: RefCell::new(BTreeMap::new()),
        source_files: RefCell::new(BTreeMap::new()),
        last_pin_across: Cell::new(0),
        pin_signature: RefCell::new(None),
        folding_anim: Cell::new(false),
        last_layout: RefCell::new((0, 0)),
        reading: std::cell::Cell::new(false),
        reading_tabs: RefCell::new(std::collections::BTreeSet::new()),
    });

    // Installed before `wire`, because every handler that calls `weak()` reads
    // this slot. Wired first and installed after, all of them got an empty
    // handle: clicks, keys on the controls and the drag did nothing at all.
    install_self(&app);
    app.apply_column(false);
    app.ui.strip.connect_button_press_event({
        let weak = app.weak();
        move |_, event| {
            if event.button() == 1 {
                if let Some(app) = weak.upgrade() {
                    app.toggle_fold();
                }
            }
            crate::gtkx::inh(true)
        }
    });
    {
        // A dragged divider is a choice, so it is written down. Without this
        // the column snapped back to its saved width on the next launch and
        // the pin wrap, which now measures the real column, disagreed with it.
        let weak = app.weak();
        app.ui.paned.connect_position_notify(move |paned| {
            let Some(app) = weak.upgrade() else { return };
            if app.folding_anim.get() {
                return;
            }
            let width = paned.position() as f64;
            if width < 40.0 {
                return;
            }
            let mut prefs = app.prefs.borrow_mut();
            if (prefs.side_width - width).abs() < 0.5 {
                return;
            }
            let old_across = app.last_pin_across.get();
            let usable = (width.round() as i32) - 20;
            let new_across = pin_columns(usable);
            // Only the live value changes here. A divider drag sends dozens of
            // position events, and each one used to serialize and fsync the
            // preferences on the UI thread. The close handler now saves the
            // live preferences, so the final width is still written once.
            prefs.side_width = width;
            drop(prefs);
            if new_across != old_across {
                app.render_pins_with_width(Some(usable));
            }
            let (ow, oh) = (app.ui.overlay.allocated_width(), app.ui.overlay.allocated_height());
            if ow > 1 && oh > 1 {
                app.layout_layers(ow, oh);
            }
        });
    }
    {
        let weak = app.weak();
        let last_side = Cell::new(-1i32);
        app.ui.side.connect_size_allocate(move |_, allocation| {
            let w = allocation.width();
            if last_side.get() == w {
                return;
            }
            last_side.set(w);
            if let Some(app) = weak.upgrade() {
                let (ow, oh) = (app.ui.overlay.allocated_width(), app.ui.overlay.allocated_height());
                if ow > 1 && oh > 1 {
                    app.layout_layers(ow, oh);
                }
            }
        });
    }
    {
        let weak = app.weak();
        app.ui.window.connect_destroy(move |_| {
            if let Some(app) = weak.upgrade() {
                app.cleanup_source_files();
            }
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

    pub fn side_width(&self) -> i32 {
        if self.ui.side.is_visible() && self.ui.side.is_child_visible() {
            self.ui.side.allocated_width().max(0)
        } else {
            0
        }
    }

    fn cleanup_source_files(&self) {
        let paths: Vec<std::path::PathBuf> = {
            let mut files = self.source_files.borrow_mut();
            std::mem::take(&mut *files).into_values().collect()
        };
        for path in paths {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn dark(&self) -> bool {
        self.palette.borrow().is_dark()
    }

    /// The backing canvas behind every WebView: dark only when the user opted
    /// into darkening pages, otherwise white so sites with no explicit ground
    /// never paint dark text on a dark canvas.
    pub fn page_canvas(&self) -> gtk::gdk::RGBA {
        if self.prefs.borrow().darken_pages {
            gtk::gdk::RGBA::new(0.043, 0.043, 0.043, 1.0)
        } else {
            gtk::gdk::RGBA::new(1.0, 1.0, 1.0, 1.0)
        }
    }

    pub fn apply_style(&self) {
        let css = crate::css::build(&self.palette.borrow(), self.prefs.borrow().row_height());
        if let Err(error) = self.provider.load_from_data(css.as_bytes()) {
            note!("search: the stylesheet did not load: {error}");
        }
        if std::env::var_os("SEARCH_CSS_DUMP").is_some() {
            let _ = std::fs::write("/tmp/search-dump.css", &css);
        }
        // Pages follow the same appearance the frame does (`prefers-color-scheme`),
        // which WebKit takes from GTK's own preference. Always write `self.dark()`
        // so switching from Dark back to System on a light desktop resets it.
        // `follow_desktop` ignores notifications while `appearance != Look::System`.
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_application_prefer_dark_theme(self.dark());
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
        self.pin_signature.replace(None);
        self.apply_style();
        self.apply_page_tone();
        let canvas = self.page_canvas();
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            if let Some(view) = self.view(id) {
                view.set_background_color(&canvas);
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
        self.pin_signature.replace(None);
        self.apply_style();
        self.apply_page_tone();
        // A page already open keeps the background it was made with unless it
        // is told the appearance changed.
        let canvas = self.page_canvas();
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            if let Some(view) = self.view(id) {
                view.set_background_color(&canvas);
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
        if !background && self.panels.borrow().open.is_some() {
            self.close_panels();
        }
        let id = {
            let mut state = self.state.borrow_mut();
            let id = state.next_id;
            state.next_id += 1;
            let mut tab = Tab::new(id);
            tab.private = private;
            // A foreground tab with an address starts out blank. If it had
            // the URL, select() below would build its view and load the page,
            // and then navigate() would load the same page again. navigate()
            // sets the URL and builds the view itself, so the page loads once.
            // A background tab is never selected, so it keeps the URL for its
            // row title until navigate() replaces it with the resolved one.
            tab.url = if background { url.unwrap_or("").to_string() } else { String::new() };
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
            None if !background => {
                self.ui.start.search.set_text("");
                let weak = self.weak();
                glib::idle_add_local_once(move || {
                    if let Some(app) = weak.upgrade() { app.ui.start.search.grab_focus(); }
                });
            }
            None => {}
        }
        id
    }

    /// A window a page opened, taken as a tab around the view WebKit was handed
    /// back (web::related_view). WebKit loads the request into that view itself,
    /// so this does not navigate. Opened in the background, after the opener.
    pub fn adopt_tab(&self, url: &str, private: bool, view: &webkit2gtk::WebView) -> Option<TabId> {
        if self.state.borrow().tabs.len() >= TAB_LIMIT {
            self.flash_error("That is as many tabs as this browser will hold");
            return None;
        }
        let id = {
            let mut state = self.state.borrow_mut();
            let id = state.next_id;
            state.next_id += 1;
            let mut tab = Tab::new(id);
            tab.private = private;
            tab.url = url.to_string();
            let at = state.position(state.active).map(|p| p + 1).unwrap_or(state.tabs.len());
            state.tabs.insert(at, tab);
            id
        };
        view.set_widget_name(&format!("page-{id}"));
        self.tone_for(view);
        self.ui.stack.add_named(view, &format!("tab-{id}"));
        view.show_all();
        self.render();
        Some(id)
    }

    /// A window the page asked for without a click: kept for the tab, and a card
    /// offers it back. The card is shown on the tab that asked, when it is in front.
    pub fn note_blocked_popup(&self, tab: TabId, url: &str) {
        note_blocked(&mut self.state.borrow_mut().blocked, tab, url);
        if self.active_id() == tab {
            self.show_blocked_card(tab);
        }
    }

    fn show_blocked_card(&self, tab: TabId) {
        let Some(popup) = self.state.borrow().blocked.get(&tab).cloned() else { return };
        let private = self.state.borrow().tab(tab).map(|t| t.private).unwrap_or(true);
        // One card at a time: the newest replaces the one on screen.
        for old in self.ui.overlay.children() {
            if old.widget_name() == "blocked-card" {
                crate::motion::cancel(&old);
                self.ui.overlay.remove(&old);
            }
        }
        let card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        card.set_widget_name("blocked-card");
        crate::ui::class(&card, "dlbar");
        card.set_margin_bottom(24);
        card.set_margin_start(self.side_width() + 12);
        card.set_margin_end(12);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::End);
        let label = gtk::Label::new(Some(&format!("{}: {}", BlockedPopup::headline(popup.count), popup.label())));
        label.set_margin_start(12);
        label.set_margin_top(7);
        label.set_margin_bottom(7);
        card.add(&label);
        let open = gtk::Button::with_label("Open it anyway");
        crate::ui::class(&open, "act");
        crate::ui::class(&open, "primary");
        let weak = self.weak();
        let url = popup.url.clone();
        let card_for_click = card.clone();
        let shown = popup.clone();
        open.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                if card_for_click.parent().is_some() {
                    app.ui.overlay.remove(&card_for_click);
                }
                forget_blocked(&app, tab, &shown);
                app.new_tab(Some(&url), private, true);
            } else {
                card_for_click.hide();
            }
        });
        card.add(&open);
        let dismiss = gtk::Button::with_label("×");
        crate::ui::class(&dismiss, "act");
        dismiss.set_tooltip_text(Some("Dismiss"));
        dismiss.set_margin_end(8);
        {
            let weak = self.weak();
            let card_for_dismiss = card.clone();
            let shown = popup.clone();
            dismiss.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    if card_for_dismiss.parent().is_some() {
                        app.ui.overlay.remove(&card_for_dismiss);
                    }
                    forget_blocked(&app, tab, &shown);
                } else {
                    card_for_dismiss.hide();
                }
            });
        }
        card.add(&dismiss);
        self.ui.overlay.add_overlay(&card);
        card.show_all();
        crate::motion::float_toast_in(&card, 24, crate::motion::dur::TRAVEL, crate::motion::dur::ENTER);
        // Dismissal by timer also drops the record, or select() would bring the card back.
        let card_for_timer = card.clone();
        let weak_window = self.ui.window.clone();
        let weak = self.weak();
        let shown = popup.clone();
        glib::timeout_add_seconds_local(10, move || {
            if card_for_timer.parent().is_none() {
                return glib::ControlFlow::Break;
            }
            // A card with focus inside it stays until the person leaves it.
            let focus_inside = weak_window.focused_widget().is_some_and(|w| w.is_ancestor(&card_for_timer));
            if card_for_timer.is_visible() && focus_inside {
                return glib::ControlFlow::Continue;
            }
            if let Some(app) = weak.upgrade() {
                if card_for_timer.parent().is_some() {
                    app.ui.overlay.remove(&card_for_timer);
                }
                forget_blocked(&app, tab, &shown);
            } else {
                card_for_timer.hide();
            }
            glib::ControlFlow::Break
        });
    }

    /// Ctrl K: the field over the page, empty, listing only the open tabs.
    /// Typing filters them with the same typo-tolerant matching as the
    /// address field, and Return or a click switches to the picked tab
    /// (omnibox::choose finds the open tab instead of loading its address
    /// again).
    pub fn open_tab_switcher(&self) {
        TAB_SWITCHER.with(|mode| mode.set(true));
        self.omni.show(&self.ui, true);
        self.ui.overlay.reorder_overlay(&self.omni.holder, -1);
        let (w, h) = (
            self.ui.overlay.allocated_width().max(self.ui.window.allocated_width()),
            self.ui.overlay.allocated_height().max(self.ui.window.allocated_height()),
        );
        if w > 1 && h > 1 {
            self.omni.layout_in(w, h, self.side_width(), 0);
        }
        let entry = self.omni.entry.clone();
        // show() fills the field with the current address; a switcher starts
        // empty so the first letter typed is already a filter.
        entry.set_text("");
        entry.set_placeholder_text(Some("Switch to a tab"));
        // The focus grab waits for an idle so the field is mapped first. A
        // switcher closed before that idle runs (Escape, or a tab picked in the
        // same burst of events) used to get focus back anyway. That left the
        // keyboard in a field that was hidden or fading out (Gemini R60 fix 6).
        // open_omni uses the same guard: hide() clears `visible` at once, so
        // the idle checks that flag before it does anything.
        let still_open = self.omni.visible.clone();
        glib::idle_add_local(move || {
            if still_open.get() {
                entry.grab_focus();
            }
            glib::ControlFlow::Break
        });
    }

    /// Ctrl Shift K: the active page again in a new tab right beside it, kept
    /// private if the tab it came from is private. A blank tab has nothing to
    /// copy, so nothing happens.
    pub fn duplicate_tab(&self) {
        let Some((url, private)) = self
            .state
            .borrow()
            .active_tab()
            .map(|tab| (tab.url.clone(), tab.private))
        else {
            return;
        };
        if url.is_empty() {
            return;
        }
        self.new_tab(Some(&url), private, false);
    }

    pub fn tab_reading(&self, id: TabId) -> bool {
        self.reading_tabs.borrow().contains(&id)
    }

    pub fn select(&self, id: TabId) {
        if self.state.borrow().position(id).is_none() {
            return;
        }
        let old = self.active_id();
        if old != id && self.state.borrow().tab(old).is_some_and(|t| t.page_full) {
            // A video full screen belongs to the tab that asked. Leaving it on
            // the old tab would hide the new tab's column too.
            self.exit_page_fullscreen(old);
            self.page_fullscreen(false);
        }
        self.state.borrow_mut().active = id;
        self.reading.set(self.tab_reading(id));
        self.ensure_view(id);
        // A blank tab has no page child now. render() below switches the
        // stack to the start page, and naming a child that doesn't exist
        // would only make GTK log a warning.
        if let Some(child) = self.ui.stack.child_by_name(&format!("tab-{id}")) {
            self.ui.stack.set_visible_child(&child);
        }
        self.sync_progress(id);
        // The card describes the tab it was opened on, so a switch takes it away
        // rather than carrying it over to the new tab.
        crate::sitecard::dismiss(self);
        crate::permissions::sync_visible(self);
        crate::peek::clear(self);
        crate::dialogs::sync_visible(self);
        crate::status::hide();
        // A blocked-pop-up card belongs to the tab that asked. Left up across a
        // switch, it would offer tab A's address over tab B, so it goes with the
        // switch. The new tab's own record, if it has one, is shown again, so a
        // pop-up blocked while its tab was in the background is not lost.
        for old in self.ui.overlay.children() {
            if old.widget_name() == "blocked-card" {
                crate::motion::cancel(&old);
                self.ui.overlay.remove(&old);
            }
        }
        if self.state.borrow().blocked.contains_key(&id) {
            self.show_blocked_card(id);
        }
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.asleep = false;
            tab.last_seen = crate::store::now_secs();
        }
        let find_query = if self.find.try_borrow().map(|f| f.visible).unwrap_or(false) {
            if let Ok(find) = self.find.try_borrow() {
                // Find follows the switch to the new tab. Before it does, the
                // old tab's page gets back exactly what it had. Without this,
                // the step script's <mark data-search-find> wrapper and its
                // remembered position stayed in the old tab's DOM. The page
                // kept an amber word nobody was searching for, and the next
                // find there started from a stale offset (Gemini R60 craft
                // fix 1). FindBar::hide runs the same cleanup for the active
                // view.
                if old != id {
                    if let Some(old_view) = self.view(old) {
                        if let Some(controller) = old_view.find_controller() {
                            controller.search_finish();
                        }
                        crate::js::run(
                            &old_view,
                            "(() => { const parents = new Set(); for (const m of document.querySelectorAll('mark[data-search-find]')) { const p = m.parentNode; if (!p) continue; while (m.firstChild) p.insertBefore(m.firstChild, m); p.removeChild(m); parents.add(p); } for (const p of parents) p.normalize(); window.__searchFindAt = undefined; if (window.CSS && CSS.highlights) CSS.highlights.delete('search-find'); return 'cleared'; })()",
                            |_| {},
                        );
                    }
                }
                find.detach();
                if let Some(view) = self.view(id) {
                    find.attach(&view);
                }
                let q = find.entry.text().to_string();
                if q.is_empty() {
                    find.count.set_text("");
                    None
                } else {
                    Some(q)
                }
            } else {
                None
            }
        } else {
            None
        };
        if let Some(q) = find_query {
            self.find_query(&q);
        }
        // A tab chosen from the address field's list is still a tab chosen:
        // leaving the field open over the page it just opened would mean the
        // next keystroke goes into an address box nobody is looking at.
        // The field is hidden before focus moves, so the shared policy below
        // does not hand focus back to the field that is closing.
        if self.omni.visible.get() {
            self.omni.hide(&self.ui);
        }
        self.render();
        // Focus goes through the shared layer policy, not straight to the
        // view. A panel or find bar that is still open keeps the keyboard, and
        // a blank tab (which has no view) focuses its start page search field.
        // This runs after render(), so the stack already shows the tab that was
        // chosen, and restore_page_focus() sees the 'blank' child when it is
        // there. A blank tab used to call start.search.grab_focus() after the
        // policy had run. That took the keyboard from an open panel or find
        // bar, and from a dialog or permission prompt waiting to focus (Gemini
        // R59 craft fix 3).
        if !crate::dialogs::focus_pending(self, id) && !crate::permissions::focus_pending(self, id) {
            self.restore_page_focus();
        }
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
        // A prompt the tab is waiting on is refused before the tab goes, and
        // the rest of its page layers come down with it (retire_page_layers):
        // dialogs, the peek card, the site card, page fullscreen and reader
        // mode. Closing a tab whose video was fullscreen used to leave the
        // window without its column and toolbar.
        self.retire_page_layers(id);
        // A closed tab's refused window is nothing to offer any more, and a
        // later tab reusing nothing of it must not inherit its card.
        self.state.borrow_mut().blocked.remove(&id);
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
        // A pin tile built while the favicon existed would keep showing it.
        self.pin_signature.replace(None);
        self.state.borrow_mut().errors.remove(&id);
        let is_source_file = self.source_files.borrow_mut().remove(&id);
        if let Some(path) = is_source_file.as_ref() {
            let _ = std::fs::remove_file(path);
        }
        let next_active = {
            let mut state = self.state.borrow_mut();
            let removed = state.tabs.remove(index);
            if is_source_file.is_none() {
                push_closed_tab(&mut state.closed_tabs, &removed, index);
            }
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
        // The last private tab takes the private session with it. The
        // ephemeral context used to live for the rest of the run, so signing in
        // inside a private tab, closing every private tab and opening a new one
        // later still found the user signed in. Dropping it here lets its
        // network process and memory cache go, and the next private tab gets a
        // fresh WebContext::new_ephemeral().
        let any_private = self.state.borrow().tabs.iter().any(|t| t.private);
        if !any_private {
            self.private_ctx.borrow_mut().take();
        }
        self.select(next_active);
        if trace {
            note!(
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
        // A tab's page is only ever added to the stack under 'tab-{id}', and
        // what goes in under that name is always the WebView itself (add_tab_view
        // and create_view). So the lookup goes straight to it by name instead of
        // walking every child of the stack. That walk ran 50+ times per
        // interaction across app, web, bench and the layers (Gemini R60 perf).
        // The widget-name check stays as a guard, so a child that is not this
        // tab's page (it shouldn't happen) still gives None, as before.
        let child = self.ui.stack.child_by_name(&format!("tab-{id}"))?;
        if child.widget_name() != format!("page-{id}").as_str() {
            return None;
        }
        child.downcast::<webkit2gtk::WebView>().ok()
    }

    pub fn id_of_page(&self, view: &webkit2gtk::WebView) -> Option<TabId> {
        view.widget_name().strip_prefix("page-")?.parse().ok()
    }

    /// Makes the tab's WebView only when the tab has a page to show. A blank
    /// tab shows the native start page, so a view made for it was never seen.
    /// It still cost a web process and its memory for every new tab. The view
    /// is made later, when the tab first navigates.
    fn ensure_view(&self, id: TabId) {
        if self.state.borrow().tab(id).map_or(true, |t| t.url.is_empty()) {
            return;
        }
        self.create_view(id, true);
    }

    /// Builds the tab's view if it has none. With `load`, the stored URL is
    /// loaded straight away. `navigate` passes false because it loads the
    /// address itself after the shield is applied.
    fn create_view(&self, id: TabId, load: bool) {
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
        if load && !url.is_empty() {
            web::load(&view, &url);
        }
        // The sleep timer stops itself when no background tab has a live view
        // (has_sleepable_tab). A new view may be one that can sleep later, so
        // arm the timer again here. start_memory_timer does nothing when "Let
        // tabs sleep" is off or a timer is already armed, so there is never
        // more than one.
        self.start_memory_timer();
    }

    /// Throw a tab's view away so it is made again: used when the shield is
    /// turned off for a host, which changes which content manager the view
    /// needs. The page is reloaded, so nothing is lost but the engine's cache.
    /// Turn the shield off for the page in front of you, or on again. The rules
    /// are baked into the content manager a view is made with, so the view has
    /// to be made again; writing the file and reloading was a no-op.
    pub fn toggle_blocker(&self) {
        let (host, private) = self
            .state
            .borrow()
            .active_tab()
            .map(|tab| (crate::shield::host_of(&tab.url), tab.private))
            .unwrap_or((String::new(), false));
        if host.is_empty() {
            return;
        }
        if private {
            self.flash_toast("Private tabs do not save site exceptions. Change exceptions from a regular tab.");
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

    /// Takes down everything that belongs to a tab's current page: permission
    /// prompts, script dialogs, a peek card opened from it, the site card,
    /// page fullscreen and reader mode. A view that is about to be rebuilt, or
    /// that has crashed, can no longer answer any of these.
    ///
    /// This is the only place that does it. Before, the crash path skipped the
    /// peek card and the rebuild path skipped fullscreen. A tab rebuilt while a
    /// video was fullscreen stayed without its column and toolbar, and a crash
    /// left a peek card open with nothing behind it.
    pub fn retire_page_layers(&self, id: TabId) {
        crate::permissions::dismiss(self, id);
        crate::dialogs::dismiss(self, id);
        crate::peek::clear_for(self, id);
        crate::sitecard::dismiss_tab(self, id);
        // A retired view never sends leave-fullscreen, so clear the tab's
        // page_full flag and give the chrome back here. The return value only
        // matters to WebKit's default handler, and that does not run for a
        // view being taken down.
        let _ = self.page_fullscreen_left(id);
        self.reading_tabs.borrow_mut().remove(&id);
        if id == self.active_id() {
            self.reading.set(false);
        }
    }

    pub fn rebuild_view(&self, id: TabId) {
        let name = format!("tab-{id}");
        if let Some(view) = self.view(id) {
            self.retire_page_layers(id);
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
                // A woken or rebuilt view takes focus only if no layer
                // (omnibox, find bar, panel) is open on top of it.
                self.restore_page_focus();
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
            let on_page = state.active_tab().is_some_and(|tab| !tab.url.is_empty());
            (items, on_page && prefs.show_bookmarks_bar && !state.bookmarks.is_empty())
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
            let state = self.state.borrow();
            let prefs = self.prefs.borrow();
            self.ui.start.refresh(
                &state.bookmarks,
                &state.history,
                state.active_tab().is_some_and(|tab| tab.private),
                prefs.shield,
                &prefs.search_engine,
            );
            if let Some(child) = self.ui.stack.child_by_name("blank") { child.show_all(); }
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
        let same = {
            let state = self.state.borrow();
            let expected: Vec<gtk::Widget> = state
                .tabs
                .iter()
                .filter(|tab| !tab.pinned)
                .filter_map(|tab| tab.row.as_ref().map(|row| row.event.clone().upcast()))
                .collect();
            expected.len() == state.tabs.iter().filter(|tab| !tab.pinned).count()
                && self.ui.rows.children() == expected
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
                let info = {
                    let state = self.state.borrow();
                    state.tab(id).and_then(|t| {
                        t.row.as_ref().map(|row| (row.event.clone(), row.title.clone(), row.close.clone(), t.asleep))
                    })
                };
                if let Some((event, title, close, asleep)) = info {
                    let context = event.style_context();
                    if id == active {
                        context.add_class("live");
                    } else {
                        context.remove_class("live");
                    }
                    if asleep {
                        context.add_class("asleep");
                    } else {
                        context.remove_class("asleep");
                    }
                    let title_ctx = title.style_context();
                    if id == active {
                        title_ctx.add_class("liveword");
                    } else {
                        title_ctx.remove_class("liveword");
                    }
                    // Set in code, not styled: see ui::close_opacity. A row
                    // under the pointer keeps its x at full strength while the
                    // active tab changes.
                    // The close button can be hovered even when the pointer
                    // is no longer reported as over the row (the button is a
                    // child with its own window). Ask both widgets so the x
                    // does not blink out under the pointer on a keyboard
                    // tab switch.
                    let prelit = |w: &gtk::Widget| w.state_flags().contains(gtk::StateFlags::PRELIGHT);
                    let hovered = prelit(event.upcast_ref::<gtk::Widget>())
                        || close.parent().as_ref().map(prelit).unwrap_or(false);
                    close.set_opacity(ui::close_opacity(id == active, hovered));
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
        for tab in &mut self.state.borrow_mut().tabs {
            tab.row = None;
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
                    progress: t.progress,
                    can_back: t.can_back,
                    can_forward: t.can_forward,
                    failed: t.failed,
                    asleep: t.asleep,
                    row: None,
                    pin: None,
                    last_seen: t.last_seen,
                    last_audible: t.last_audible,
                    page_full: t.page_full,
                    window_was_full: t.window_was_full,
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
            row.subtitle.set_text(&tab_subtitle(&tab.url, tab.private, tab.asleep));
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
            // Tab and Shift+Tab can reach a row, so the tab list works without
            // a pointer. A click still leaves focus where it was, so the page
            // keeps the keyboard when you pick a tab with the mouse. A focused
            // row selects on Enter or Space and closes on Delete, the same
            // actions a left click and a middle click give.
            row.event.set_can_focus(true);
            row.event.set_focus_on_click(false);
            row.event.add_events(gtk::gdk::EventMask::KEY_PRESS_MASK);
            {
                let weak = self.weak();
                let id = tab.id;
                row.event.connect_key_press_event(move |_, event| {
                    use gtk::gdk::keys::constants as key;
                    let Some(app) = weak.upgrade() else { return crate::gtkx::inh(false) };
                    let state = event.state();
                    let modified = state.intersects(
                        gtk::gdk::ModifierType::CONTROL_MASK
                            | gtk::gdk::ModifierType::MOD1_MASK
                            | gtk::gdk::ModifierType::SHIFT_MASK,
                    );
                    if modified {
                        return crate::gtkx::inh(false);
                    }
                    let keyval = event.keyval();
                    if keyval == key::Return || keyval == key::KP_Enter || keyval == key::space {
                        app.select(id);
                        crate::gtkx::inh(true)
                    } else if keyval == key::Delete || keyval == key::KP_Delete {
                        app.close_tab(id);
                        crate::gtkx::inh(true)
                    } else {
                        crate::gtkx::inh(false)
                    }
                });
            }
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
                // Hover used to be CSS `.row:hover button.close`. The close
                // button's opacity is now set in code (ui::close_opacity), so
                // the row says when the pointer comes and goes.
                row.event.add_events(
                    gtk::gdk::EventMask::ENTER_NOTIFY_MASK | gtk::gdk::EventMask::LEAVE_NOTIFY_MASK,
                );
                let weak = self.weak();
                let id = tab.id;
                let close = row.close.clone();
                row.event.connect_enter_notify_event(move |_, _| {
                    if let Some(app) = weak.upgrade() {
                        close.set_opacity(ui::close_opacity(app.active_id() == id, true));
                    }
                    crate::gtkx::inh(false)
                });
                let weak = self.weak();
                let close = row.close.clone();
                row.event.connect_leave_notify_event(move |_, event| {
                    // Moving onto the close button is a leave into a child
                    // (the button's input window), not a leave from the row.
                    if event.detail() == gtk::gdk::NotifyType::Inferior {
                        return crate::gtkx::inh(false);
                    }
                    if let Some(app) = weak.upgrade() {
                        close.set_opacity(ui::close_opacity(app.active_id() == id, false));
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
        self.ui.rows.show_all();
        // After show_all, not before: a row that is not mapped has no
        // allocated height, and the anchor is computed from real geometry.
        self.scroll_active_into_view();
    }

    fn render_pins(&self) {
        self.render_pins_with_width(None);
    }

    /// The tooltip and accessible name for a pin. One place for the first
    /// draw and for the in-place update after a tab switch, so they cannot
    /// drift apart.
    fn pin_words(title: &str, live: bool) -> (String, String) {
        if live {
            (format!("{title}\nActive tab"), format!("Pinned tab: {title}, active"))
        } else {
            (title.to_string(), format!("Pinned tab: {title}"))
        }
    }

    fn render_pins_with_width(&self, usable_override: Option<i32>) {
        // Each pin carries its own words and whether it is the tab you are
        // on. A pinned tab with no title and no address used to produce an
        // empty tooltip: the fallback the rows use gives it something to say.
        let pinned: Vec<(TabId, String, String, bool)> = {
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
                    (t.id, t.pin_mark(), title, t.id == state.active)
                })
                .collect()
        };
        if pinned.is_empty() {
            for child in self.ui.pins.children() {
                self.ui.pins.remove(&child);
            }
            self.pin_signature.replace(None);
            self.last_pin_across.set(0);
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
            let usable = match usable_override {
                Some(w) if w > 40 => w,
                _ => {
                    let measured = self.ui.pins.allocated_width();
                    if measured > 40 { measured } else { (self.prefs.borrow().side_width as i32) - 20 }
                }
            };
            pin_columns(usable)
        };
        self.last_pin_across.set(across);
        // Same pins, same order, same words and same column count: the tiles
        // already on screen are what a rebuild would make, so they stay. The
        // active tab is deliberately not in the signature: a tab switch must
        // recolour a tile, not throw the whole strip away.
        let signature = (
            across,
            pinned.iter().map(|(id, letter, title, _)| (*id, letter.clone(), title.clone())).collect::<Vec<_>>(),
        );
        if self.pin_signature.borrow().as_ref() == Some(&signature)
            && self.ui.pins.children().len() == pinned.len()
        {
            // CSS paints `.pin.live` and its letter, so the class on the
            // event box is the whole visual change. Words and ATK name follow
            // the same live flag.
            let state = self.state.borrow();
            for (id, _, title, live) in &pinned {
                let Some(tile) = state.tab(*id).and_then(|tab| tab.pin.as_ref()) else { continue };
                let context = tile.event.style_context();
                if *live {
                    context.add_class("live");
                } else {
                    context.remove_class("live");
                }
                let (tip, name) = Self::pin_words(title, *live);
                tile.event.set_tooltip_text(Some(&tip));
                if let Some(access) = tile.event.accessible() {
                    gtk::atk::prelude::AtkObjectExt::set_name(&access, &name);
                }
            }
            return;
        }
        for child in self.ui.pins.children() {
            self.ui.pins.remove(&child);
        }
        self.pin_signature.replace(Some(signature));
        for (index, (id, letter, title, live)) in pinned.iter().enumerate() {
            let tile = ui::pin_tile(letter, *live, *self.palette.borrow());
            // A page that offers a mark shows it and drops the letter; a page
            // with neither keeps the letter, which is at least its own.
            if let Some(surface) = self.icons.borrow().get(id).cloned() {
                ui::mark(&tile.icon, letter.chars().next().unwrap_or('?'), Some(&surface), 22);
                tile.icon.set_no_show_all(false);
                tile.icon.show_all();
                tile.letter.hide();
            }
            // The tooltip alone said what the page was, so the pin you were
            // on read exactly like the other three. It now says which one is
            // live, and the tile - a letter or a favicon, with no text inside
            // - gets the same words as an accessible name under a tab role,
            // because a screen reader cannot read the letter at all.
            let (tip, name) = Self::pin_words(title, *live);
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
        let (back, forward, url, loading, private, kept, count) = {
            let state = self.state.borrow();
            let (back, forward, url, loading, private) = state
                .active_tab()
                .map(|tab| (tab.can_back, tab.can_forward, tab.url.clone(), tab.loading, tab.private))
                .unwrap_or_default();
            let kept = state.bookmarks.iter().any(|bookmark| bookmark.url == url);
            (back, forward, url, loading, private, kept, state.tabs.iter().filter(|tab| !tab.pinned).count())
        };
        self.ui.back.set_sensitive(back);
        self.ui.forward.set_sensitive(forward);
        self.ui.reload.set_sensitive(!url.is_empty());
        self.ui.reader.set_sensitive(!url.is_empty());
        self.ui.bookmark.set_sensitive(!url.is_empty());
        self.ui.address_label.set_text(&chrome_address(&url, private));
        // Round-68 review: the trusted origin stays its own chip, but the
        // path that actually failed is now visible beside it. Keeping the two
        // in separate labels means a path can never impersonate the origin.
        let path = crate::shield::location_of(&url);
        self.ui.address_path.set_text(if path == "/" { "" } else { &path });
        self.ui.address.set_tooltip_text(Some(if url.is_empty() { "Search or enter an address (Ctrl L)" } else { "Open the full address or search (Ctrl L)" }));
        self.ui.tab_count.set_text(&format!("{count:02}"));
        // The accent shield means two things: the blocker is on for this page,
        // and the page came over a secure connection. Gemini R59 fix 7 found
        // plain http pages getting the accent shield too, because
        // shielded_for only checks the blocker pref and the per-site allow
        // list. An http page is not protected in transit, so its shield stays
        // muted. The blocker still runs on it: web.rs builds content managers
        // from shielded_for directly, and that does not change here.
        let secure = url.starts_with("https://");
        let protected = secure && web::shielded_for(self, &url);
        let context = self.ui.site_info.style_context();
        if protected { context.add_class("protected"); } else { context.remove_class("protected"); }
        self.ui.site_info.set_tooltip_text(Some(if url.is_empty() {
            "Privacy and browsing settings"
        } else if protected {
            "Ad and tracker blocking is on · Site information (Ctrl I)"
        } else {
            "Site information and permissions (Ctrl I)"
        }));
        let context = self.ui.bookmark.style_context();
        if kept { context.add_class("kept"); } else { context.remove_class("kept"); }
        let context = self.ui.reader.style_context();
        if self.reading.get() { context.add_class("reading"); } else { context.remove_class("reading"); }
        let context = self.ui.reload.style_context();
        if context.has_class("stopping") != loading {
            if let Some(child) = self.ui.reload.child() {
                self.ui.reload.remove(&child);
            }
            let glyph = crate::icons::area(
                if loading { crate::icons::Icon::Stop } else { crate::icons::Icon::Reload },
                17,
                self.palette.borrow().muted,
            );
            self.ui.reload.add(&glyph);
            glyph.show();
            if loading { context.add_class("stopping"); } else { context.remove_class("stopping"); }
        }
        let reload_label = if loading { "Stop loading (Escape)" } else { "Reload (Ctrl R)" };
        self.ui.reload.set_tooltip_text(Some(reload_label));
        if let Some(access) = self.ui.reload.accessible() {
            gtk::atk::prelude::AtkObjectExt::set_name(&access, reload_label);
        }
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
            if id == self.active_id() { self.refresh_helm(); }
        }
    }

    fn update_row_title(&self, id: TabId) {
        let info = {
            let state = self.state.borrow();
            let Some(tab) = state.tab(id) else { return };
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
            let row = tab
                .row
                .as_ref()
                .map(|row| (row.title.clone(), row.letter.clone(), row.subtitle.clone()));
            let pin_letter = tab.pin.as_ref().map(|pin| pin.letter.clone());
            (title, letter, row, pin_letter, tab_subtitle(&tab.url, tab.private, tab.asleep))
        };
        let (title, letter, row, pin_letter, subtitle) = info;
        if let Some((row_title, row_letter, row_subtitle)) = row {
            row_title.set_text(&title);
            row_subtitle.set_text(&subtitle);
            if let Some(label) = row_letter {
                label.set_text(&letter);
            }
        }
        if let Some(label) = pin_letter {
            label.set_text(&letter);
        }
    }

    /// The engine found a page icon. Store it and put it where the letter was.
    pub fn set_icon(&self, id: TabId, surface: Option<cairo::Surface>) {
        let changed = {
            let mut icons = self.icons.borrow_mut();
            match surface {
                // A new favicon always counts as a change, including one that
                // replaces an older favicon. The old check counted only the first
                // icon (insert(..).is_none()), so when a site later swapped its
                // favicon (a different page on the same tab, or an unread badge),
                // the cache stored the new surface but the row and pin kept
                // drawing the old one (Gemini R60 craft fix 2). Redrawing costs a
                // single mark() call per favicon event.
                Some(surface) => {
                    icons.insert(id, surface);
                    true
                }
                None => icons.remove(&id).is_some(),
            }
        };
        if !changed {
            return;
        }
        let targets = {
            let state = self.state.borrow();
            state.tab(id).map(|tab| {
                (
                    tab.letter(),
                    tab.row.as_ref().map(|row| row.icon.clone()),
                    tab.pin.as_ref().map(|pin| pin.icon.clone()),
                    tab.private,
                    tab.loading,
                )
            })
        };
        let Some((letter, row_icon, pin_icon, private, loading)) = targets else {
            return;
        };
        let icon = self.icons.borrow().get(&id).cloned();
        if let Some(holder) = row_icon {
            if !private && !loading {
                // When a favicon arrives, the letter label leaves the screen and
                // mark returns None. When the icon goes away, mark builds a new
                // tile. In both cases row.letter must hold what is on screen now.
                let label = crate::ui::mark(&holder, letter, icon.as_ref(), 16);
                if let Some(row) = self.state.borrow_mut().tab_mut(id).and_then(|tab| tab.row.as_mut()) {
                    row.letter = label;
                }
            }
        }
        if let Some(holder) = pin_icon {
            if !private {
                crate::ui::mark(&holder, letter, icon.as_ref(), 22);
            }
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
            if id == self.active_id() { self.refresh_helm(); }
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
                // ui::mark builds a new tile with a new label each time. Keep
                // the label the row now shows in row.letter, so update_row_title
                // renames the label on screen. Before this, the tile kept the
                // letter from the first title it had, e.g. "1" (Gemini R59 fix 3).
                // The spinner has no letter, so the row has none while loading.
                let label = if loading {
                    crate::ui::loading_mark(&icon, self.palette.borrow().muted);
                    None
                } else {
                    crate::ui::mark(&icon, letter, surface.as_ref(), 16)
                };
                if let Some(row) = self.state.borrow_mut().tab_mut(id).and_then(|tab| tab.row.as_mut()) {
                    row.letter = label;
                }
            }
        }
        if id == self.active_id() { self.refresh_helm(); }
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

    pub fn set_load_progress(&self, id: TabId, value: f64) {
        let value = value.clamp(0.0, 1.0);
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.progress = value;
        }
        if id != self.active_id() {
            return;
        }
        if value <= 0.0 {
            crate::motion::cancel(&self.ui.progress);
            self.ui.progress_frac.set(0.0);
            self.ui.progress.set_opacity(1.0);
            self.ui.progress.hide();
            return;
        }
        if value >= 1.0 {
            if !self.ui.progress.is_visible() {
                self.ui.progress_frac.set(0.0);
                return;
            }
            if self.ui.progress.opacity() < 0.99 && self.ui.progress_frac.get() >= 0.99 {
                return;
            }
            let from = self.ui.progress_frac.get();
            let progress = self.ui.progress.clone();
            let progress_done = self.ui.progress.clone();
            let frac = self.ui.progress_frac.clone();
            let frac_done = self.ui.progress_frac.clone();
            crate::motion::animate(
                // The bar fills its last stretch on MICRO, then clears on the
                // shared EXIT role like every other layer that leaves.
                &self.ui.progress,
                crate::motion::dur::MICRO,
                move |t| {
                    let eased = crate::motion::ease_out_cubic(t);
                    frac.set(crate::motion::lerp(from, 1.0, eased));
                    progress.queue_draw();
                },
                move || {
                    let progress_hide = progress_done.clone();
                    crate::motion::fade_out_then(&progress_done, crate::motion::dur::EXIT, move || {
                        progress_hide.hide();
                        progress_hide.set_opacity(1.0);
                        frac_done.set(0.0);
                    });
                },
            );
            return;
        }
        let was_fading = !self.ui.progress.is_visible()
            || self.ui.progress.opacity() < 0.99
            || self.ui.progress_frac.get() >= 0.99;
        crate::motion::cancel(&self.ui.progress);
        self.ui.progress.set_opacity(1.0);
        self.ui.progress.show();
        let from = if was_fading { 0.0 } else { self.ui.progress_frac.get() };
        if (value - from).abs() < 0.001 {
            self.ui.progress_frac.set(value);
            self.ui.progress.queue_draw();
            return;
        }
        let progress = self.ui.progress.clone();
        let frac = self.ui.progress_frac.clone();
        // Each step of the load bar is a small state change, so it moves on
        // the shared MICRO role, the same as its final stretch, instead of
        // its own 150ms.
        crate::motion::animate(
            &self.ui.progress,
            crate::motion::dur::MICRO,
            move |t| {
                let eased = crate::motion::ease_out_cubic(t);
                frac.set(crate::motion::lerp(from, value, eased));
                progress.queue_draw();
            },
            || {},
        );
    }

    fn sync_progress(&self, id: TabId) {
        let value = self.state.borrow().tab(id).map(|t| t.progress).unwrap_or(0.0);
        crate::motion::cancel(&self.ui.progress);
        self.ui.progress.set_opacity(1.0);
        if value > 0.0 && value < 1.0 {
            self.ui.progress_frac.set(value);
            self.ui.progress.show();
            self.ui.progress.queue_draw();
        } else {
            self.ui.progress_frac.set(0.0);
            self.ui.progress.hide();
        }
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

    /// A visit or a title change. History and hidden become dirty. The session
    /// does too, because it stores tab titles.
    fn schedule_flush(&self) {
        HISTORY_DIRTY.store(true, Ordering::Relaxed);
        SESSION_DIRTY.store(true, Ordering::Relaxed);
        self.schedule_persist();
    }

    /// Arms the single save timer. Calls made while it is armed only add dirty
    /// flags, so a burst gives one wakeup and at most one write per file.
    fn schedule_persist(&self) {
        if PERSIST_PENDING.swap(true, Ordering::Relaxed) {
            return;
        }
        let weak = self.weak();
        glib::timeout_add_seconds_local(2, move || {
            PERSIST_PENDING.store(false, Ordering::Relaxed);
            if let Some(app) = weak.upgrade() {
                // The flag is cleared before the write, so a visit that lands
                // while it runs marks it dirty again. If the write fails (full
                // disk, read-only config folder), the flag is set again, so the
                // next save timer writes it instead of the change being lost.
                // There is no retry loop: a disk that stays full must not wake
                // the browser every 2s. The next visit or title arms the timer.
                let plan = take_persist_plan(&HISTORY_DIRTY, &SESSION_DIRTY);
                // The data is copied and numbered here, on the main thread, so
                // the snapshot numbers follow the order the state changed in.
                // Only the serialising, sync_all and rename move to the worker.
                let history = plan.history.then(|| {
                    let state = app.state.borrow();
                    (state.history.clone(), state.hidden.clone(), crate::files::snapshot_number())
                });
                let session = if plan.session {
                    // Cleared before the copy, as save_session does, so a
                    // change that lands while the worker writes marks it again.
                    SESSION_DIRTY.store(false, Ordering::Relaxed);
                    app.session_records()
                        .map(|records| (records, crate::files::snapshot_number()))
                } else {
                    None
                };
                if history.is_none() && session.is_none() {
                    return glib::ControlFlow::Break;
                }
                // A slow disk no longer holds up a frame. The ordered writer
                // keeps one writer per file at a time, and a late older
                // snapshot never replaces newer data, such as the synchronous
                // save at window close. A failed write marks its file dirty
                // again, so the next timer run tries it again. The flags are
                // atomics, so the worker may set them.
                drop(gtk::gio::spawn_blocking(move || {
                    if let Some((history, hidden, snapshot)) = history {
                        // Both files are written even if the first one fails.
                        let history_ok = Records::save_history_at(&history, snapshot);
                        let hidden_ok = Records::save_hidden_at(&hidden, snapshot);
                        if !(history_ok && hidden_ok) {
                            HISTORY_DIRTY.store(true, Ordering::Relaxed);
                        }
                    }
                    if let Some((records, snapshot)) = session {
                        if !Records::save_session_at(&records, snapshot) {
                            SESSION_DIRTY.store(true, Ordering::Relaxed);
                        }
                    }
                }));
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
        // Record the address first. A blank tab has no view yet, so navigating
        // is what makes one, and anything create_view reads from the tab
        // (its address, whether it is asleep) has to describe the page that
        // is about to load, not the empty tab it was a moment ago. The view is
        // built without loading, and the load below then runs once, after the
        // shield is applied.
        if let Some(tab) = self.state.borrow_mut().tab_mut(id) {
            tab.url = url.clone();
            tab.asleep = false;
        }
        self.create_view(id, false);
        curtain::apply_for_tab(self, id);
        if let Some(view) = self.view(id) {
            web::load(&view, &url);
            // A background tab loads without taking the screen: the tab bar
            // says which tab is on top, and it must agree with what is shown.
            if id == self.active_id() {
                self.ui.stack.set_visible_child_name(&format!("tab-{id}"));
                // The shared policy decides where focus goes. If the omnibox is
                // still up, close_omni below hands focus to the page once it hides.
                self.restore_page_focus();
            }
        }
        self.update_row_title(id);
        // Nothing is being typed at a page that just arrived: a field left open
        // over it hides the very page that was just asked for.
        if self.omni.visible.get() && self.omni.entry.text() != url {
            self.close_omni();
        }
        // Coalesced, not immediate: a burst of navigations (redirects, quick
        // link clicks) used to serialize and fsync the session on the UI
        // thread each time. One save a moment later records the same result.
        self.schedule_session();
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
        if self.state.borrow().errors.contains_key(&id) {
            self.retry();
            return;
        }
        self.reload_tab(id);
    }

    pub fn reload_or_stop(&self) {
        if let Some(view) = self.view(self.active_id()).filter(|view| view.is_loading()) {
            view.stop_loading();
        } else {
            self.reload();
        }
    }

    pub fn reload_tab(&self, id: TabId) {
        if self.state.borrow().errors.contains_key(&id) {
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
            return;
        }
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
        let Some((url, private)) = self
            .state
            .borrow()
            .tab(id)
            .map(|t| (t.url.clone(), t.private))
        else {
            return;
        };
        if url.is_empty() {
            self.new_tab(None, private, false);
        } else {
            self.new_tab(Some(&url), private, true);
        }
    }

    /// The page's own source, written out and opened: the browser has no
    /// second window to put it in, and a file is honest about that.
    pub fn view_source(&self) {
        let active = self.active_id();
        let Some(view) = self.view(active) else { return };
        let private = self
            .state
            .borrow()
            .tab(active)
            .map(|t| t.private)
            .unwrap_or(true);
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
            if app.state.borrow().tabs.len() >= TAB_LIMIT {
                app.flash_error("That is as many tabs as this browser will hold");
                return;
            }
            // The copy goes in the per-user runtime folder (owner-only and
            // held in memory, so a private tab's page never reaches the disk).
            // A private tab never falls back to shared `/tmp`. The file gets a
            // new name each time and is created owner-only (0600) and only if
            // it does not already exist, so another local user can neither
            // read it nor plant a file or link under that name first.
            use std::io::Write as _;
            use std::os::unix::fs::OpenOptionsExt;
            use std::sync::atomic::{AtomicU64, Ordering};
            static COUNT: AtomicU64 = AtomicU64::new(0);
            let runtime = glib::user_runtime_dir();
            let base = if runtime.is_dir() {
                runtime
            } else if private {
                app.flash_toast("No source: no private runtime folder");
                return;
            } else {
                crate::files::data_dir()
            };
            let stamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            let path = base.join(format!(
                "search-source-{}-{}-{stamp:x}.html",
                std::process::id(),
                COUNT.fetch_add(1, Ordering::Relaxed)
            ));
            let written = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
                .and_then(|mut file| file.write_all(body.as_bytes()));
            if written.is_err() {
                let _ = std::fs::remove_file(&path);
                app.flash_toast("No source: could not write it");
                return;
            }
            let new_id = app.new_tab(Some(&format!("file://{}", path.display())), private, false);
            app.source_files.borrow_mut().insert(new_id, path);
        });
    }

    /// Whether the window really fills the screen right now. A page's video
    /// can put the window there and take it back without going through
    /// toggle_fullscreen, so the window is asked instead of trusting a flag.
    pub fn window_is_fullscreen(&self) -> bool {
        self.ui
            .window
            .window()
            .is_some_and(|w| w.state().contains(gtk::gdk::WindowState::FULLSCREEN))
    }

    /// Full screen, and back. The window is the only thing that changes.
    pub fn toggle_fullscreen(&self) {
        // window_full follows window-state-event; the window itself may still
        // be changing from the last press, so it is not asked here.
        let filling = self.window_full.get();
        if filling {
            self.ui.window.unfullscreen();
            // Leaving with F11 while a video was fullscreen must bring the
            // column back too, and end the page's own full screen.
            let active = self.active_id();
            self.exit_page_fullscreen(active);
            self.page_fullscreen(false);
        } else {
            self.ui.window.fullscreen();
        }
        self.fullscreen.set(!filling);
    }

    /// The page's element asked for full screen. Remembers whether the window
    /// was already full (F11), so leaving can give it back as it was.
    pub fn page_fullscreen_entered(&self, tab: TabId) {
        let window_was_full = self.window_full.get();
        if let Some(t) = self.state.borrow_mut().tab_mut(tab) {
            t.page_full = true;
            t.window_was_full = window_was_full;
        }
        if tab == self.active_id() {
            self.page_fullscreen(true);
        }
    }

    /// The page's element gave full screen back. Returns true when WebKit
    /// should leave the window alone (the user had F11 on before the page
    /// asked), false when WebKit's default may take the window out of full screen.
    pub fn page_fullscreen_left(&self, tab: TabId) -> bool {
        let (was_page, window_was_full) = {
            let mut state = self.state.borrow_mut();
            match state.tab_mut(tab) {
                Some(t) => (
                    std::mem::replace(&mut t.page_full, false),
                    std::mem::replace(&mut t.window_was_full, false),
                ),
                None => (false, false),
            }
        };
        if was_page && tab == self.active_id() {
            self.page_fullscreen(false);
        }
        keep_window_on_leave(window_was_full)
    }

    /// Ends a tab's page full screen by asking the page. The column comes back
    /// by the caller (or by page_fullscreen_left), so this only clears the flag
    /// and runs the page's own exit.
    pub fn exit_page_fullscreen(&self, tab: TabId) {
        let was_page = {
            let mut state = self.state.borrow_mut();
            state.tab_mut(tab).is_some_and(|t| std::mem::replace(&mut t.page_full, false))
        };
        if !was_page {
            return;
        }
        if let Some(view) = self.view(tab) {
            crate::js::run(
                &view,
                "if (document.fullscreenElement) document.exitFullscreen()",
                |_| {},
            );
        }
    }

    /// Stamps a tab as audible now. Called from the page's is-playing-audio
    /// notify, so it is the moment the page started or stopped making sound.
    pub fn mark_audible(&self, tab: TabId) {
        if let Some(t) = self.state.borrow_mut().tab_mut(tab) {
            t.last_audible = Some(std::time::Instant::now());
        }
    }

    /// A page's element (a video, a game, slides) took the whole screen, or
    /// gave it back. Only the browser's own furniture is hidden: the column
    /// with its top band, and the bookmark row. child_visible is used rather
    /// than set_visible so the bookmark row's own shown-or-hidden choice is
    /// kept and comes back exactly as it was.
    pub fn page_fullscreen(&self, on: bool) {
        self.apply_column(on);
        self.ui.bmarkbar.set_child_visible(!on);
        self.ui.toolbar.set_child_visible(!on);
        self.fullscreen.set(on || self.window_full.get());
        self.ui.window.queue_resize();
        let (w, h) = (self.ui.overlay.allocated_width(), self.ui.overlay.allocated_height());
        if w > 1 && h > 1 {
            self.layout_layers(w, h);
        }
    }

    /// Lays the column out from the saved fold state and the page's full
    /// screen state. Full screen hides the column; leaving it returns to the
    /// fold the person chose, never a forced open column.
    pub fn apply_column(&self, page_full: bool) {
        let (folded, sidebar, width) = {
            let prefs = self.prefs.borrow();
            (prefs.column_folded, prefs.sidebar, prefs.side_width.round() as i32)
        };
        let plan = crate::ui::column_plan(folded, page_full, sidebar, width);
        self.ui.side.set_child_visible(plan.side_shown);
        let min_width = if !folded && sidebar { crate::ui::MIN_COL.min(plan.width) } else { plan.width };
        self.ui.side.set_size_request(min_width, -1);
        self.ui.strip.set_visible(plan.strip);
        if plan.body {
            // GTK3's show_all returns at once on a widget marked no_show_all,
            // so it never reached the body's children: the column opened as an
            // empty box (seen in the failed-page capture, 15875 bytes). The
            // mark is lifted for this one call and put back, so the window's
            // own show_all still cannot unfold a column saved as folded.
            // Children marked no_show_all themselves (helm, pins) keep their
            // own choice, as before.
            self.ui.body.set_no_show_all(false);
            self.ui.body.show_all();
            self.ui.body.set_no_show_all(true);
        }
        self.ui.body.set_visible(plan.body);
        // Set only while the column is shown: a hidden first child has no
        // divider to place, and the saved width is still there for later.
        if !page_full {
            self.ui.paned.set_position(plan.width);
        }
    }

    /// Ctrl S, and a click on the strip: fold the column, or open it again.
    pub fn toggle_fold(&self) {
        let (folded, sidebar, target_width) = {
            let mut prefs = self.prefs.borrow_mut();
            if prefs.sidebar {
                prefs.column_folded = !prefs.column_folded;
            } else {
                prefs.sidebar = true;
                prefs.column_folded = false;
            }
            prefs.save();
            (prefs.column_folded, prefs.sidebar, prefs.side_width.round() as i32)
        };
        // A page in full screen hides the column; the fold is still chosen.
        let page_full = !self.ui.side.is_child_visible();
        if page_full || !sidebar || !crate::motion::enabled() {
            self.apply_column(page_full);
            let (w, h) = (self.ui.overlay.allocated_width(), self.ui.overlay.allocated_height());
            if w > 1 && h > 1 {
                self.layout_layers(w, h);
            }
            return;
        }

        crate::motion::cancel(&self.ui.paned);
        crate::motion::cancel(&self.ui.body);
        crate::motion::cancel(&self.ui.strip);

        if folded {
            let from = self.ui.paned.position().max(crate::ui::STRIP_WIDTH);
            self.folding_anim.set(true);
            // Lower the column minimum width before animating so GtkPaned can
            // glide below 140px down to STRIP_WIDTH.
            self.ui.side.set_size_request(crate::ui::STRIP_WIDTH, -1);

            let body = self.ui.body.clone();
            let strip = self.ui.strip.clone();
            // Folding is one layout move on the shared LAYOUT role. The body
            // clears in the first part of it (LAYOUT - MICRO), and the strip
            // then fades in over MICRO. Its fade ends on the same frame the
            // column reaches STRIP_WIDTH, so the two halves never drift apart
            // and the column is never left empty.
            crate::motion::fade_out_then(
                &self.ui.body,
                crate::motion::dur::LAYOUT - crate::motion::dur::MICRO,
                move || {
                    body.hide();
                    body.set_opacity(1.0);
                    strip.set_opacity(0.0);
                    strip.show();
                    crate::motion::fade_in(&strip, crate::motion::dur::MICRO);
                },
            );

            let weak = self.weak();
            crate::motion::slide_paned(&self.ui.paned, from, crate::ui::STRIP_WIDTH, crate::motion::dur::LAYOUT, move || {
                if let Some(app) = weak.upgrade() {
                    crate::motion::cancel(&app.ui.body);
                    app.ui.side.set_size_request(crate::ui::STRIP_WIDTH, -1);
                    app.ui.body.hide();
                    app.ui.body.set_opacity(1.0);
                    app.ui.strip.show();
                    app.folding_anim.set(false);
                    let (w, h) = (app.ui.overlay.allocated_width(), app.ui.overlay.allocated_height());
                    if w > 1 && h > 1 {
                        app.layout_layers(w, h);
                    }
                }
            });
        } else {
            let from = self.ui.paned.position().max(crate::ui::STRIP_WIDTH);
            let target_width = target_width.max(crate::ui::STRIP_WIDTH);
            self.folding_anim.set(true);
            self.ui.side.set_child_visible(true);
            // Reversing a fold partway must not blink: if the body is still on
            // screen (its exit fade was cancelled above), carry on from the
            // opacity it has now instead of snapping it back to zero first.
            let resume_from = if self.ui.body.is_visible() {
                self.ui.body.opacity().clamp(0.0, 1.0)
            } else {
                0.0
            };
            self.ui.strip.hide();
            self.ui.strip.set_opacity(1.0);
            self.ui.body.set_no_show_all(false);
            self.ui.body.show_all();
            self.ui.body.set_no_show_all(true);
            // Unfolding mirrors folding: the column opens on LAYOUT, and the
            // body arrives on the shared ENTER role, so it is fully visible
            // just before the column stops moving. A resumed body covers only
            // the opacity it still lacks, so it needs proportionally less time.
            let remaining = ((1.0 - resume_from) * crate::motion::dur::ENTER as f64).round() as u64;
            crate::motion::fade_from(&self.ui.body, resume_from, 1.0, remaining.max(crate::motion::dur::MICRO / 2));

            let weak = self.weak();
            crate::motion::slide_paned(&self.ui.paned, from, target_width, crate::motion::dur::LAYOUT, move || {
                if let Some(app) = weak.upgrade() {
                    app.ui.side.set_size_request(crate::ui::MIN_COL.min(target_width), -1);
                    app.ui.body.set_opacity(1.0);
                    app.folding_anim.set(false);
                    let (w, h) = (app.ui.overlay.allocated_width(), app.ui.overlay.allocated_height());
                    if w > 1 && h > 1 {
                        app.layout_layers(w, h);
                    }
                }
            });
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
        if self.state.borrow().tabs.len() >= TAB_LIMIT {
            self.flash_error("That is as many tabs as this browser will hold");
            return;
        }
        let popped = self.state.borrow_mut().closed_tabs.pop();
        if let Some(closed) = popped {
            let id = {
                let mut guard = self.state.borrow_mut();
                let state = &mut *guard;
                restore_closed_tab(&mut state.tabs, &mut state.next_id, closed)
            };
            // Selecting the restored tab builds its view, and ensure_view
            // already loads the stored URL. Calling navigate as well loaded
            // the page a second time.
            self.select(id);
            self.update_row_title(id);
            self.schedule_session();
            return;
        }
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
                self.new_tab(Some(&entry.url), false, false);
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
        let instant_now = std::time::Instant::now();
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
                        && may_sleep(tab.last_audible, instant_now, minutes)
                        && now.saturating_sub(tab.last_seen) > minutes * 60
                })
                .map(|tab| tab.id)
                .collect()
        };
        // A tab that is playing sound is in use even when nobody has looked at
        // it for the whole sleep setting: music, a podcast or a call in the
        // background.
        // Putting it to sleep destroyed its view and cut the sound off. This
        // check runs after the state borrow above has ended, because asking a
        // view for its audio state must not happen while the state is borrowed.
        let sleeping: Vec<TabId> = sleeping
            .into_iter()
            .filter(|id| {
                !self.view(*id).is_some_and(|view| {
                    view.is_playing_audio()
                        || is_capturing(view.camera_capture_state(), view.microphone_capture_state())
                })
            })
            .collect();
        if sleeping.is_empty() {
            return;
        }
        for id in &sleeping {
            // A sleeping tab loses its view, so everything that view was
            // holding open goes the same way as on close, crash or rebuild.
            self.retire_page_layers(*id);
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
    /// Removes the armed sleep timer at once. The Settings switch calls this
    /// when "Let tabs sleep" is turned off, so no tick comes after that. The
    /// slot is taken before remove(), and a tick that returns Break clears the
    /// slot itself, so a source is never removed twice.
    pub fn stop_memory_timer(&self) {
        if let Some(id) = MEMORY_TIMER.with(|slot| slot.borrow_mut().take()) {
            id.remove();
        }
    }

    /// True when some tab other than the one in front still holds a live view,
    /// so a later tick could put it to sleep. With one tab, or with every
    /// background tab already asleep, there is nothing for the sleep timer to
    /// do. The timer then stops itself instead of waking the process every two
    /// minutes for nothing. create_view arms it again when a new view appears.
    /// The ids are collected first so that no view lookup runs while the state
    /// is borrowed.
    fn has_sleepable_tab(&self) -> bool {
        let candidates: Vec<TabId> = {
            let state = self.state.borrow();
            let active = state.active;
            state
                .tabs
                .iter()
                .filter(|tab| tab.id != active && !tab.asleep)
                .map(|tab| tab.id)
                .collect()
        };
        candidates.into_iter().any(|id| self.view(id).is_some())
    }

    pub fn start_memory_timer(&self) {
        // Armed only while "Let tabs sleep" is on: with the feature off,
        // nothing wakes the process every two minutes just to return early.
        // The Settings switch calls this again when it is turned back on. It
        // does nothing if a timer is already armed, so there is never more
        // than one.
        if !self.prefs.borrow().memory_saver {
            return;
        }
        if MEMORY_TIMER.with(|slot| slot.borrow().is_some()) {
            return;
        }
        let weak = self.weak();
        // Every two minutes. A setting of N minutes means "not looked at for
        // N minutes", and a tab is put to sleep at most two minutes after
        // that. At the default of ten minutes that is 10 to 12 minutes, which
        // is close enough to the setting, and the tick costs nothing
        // measurable.
        let id = glib::timeout_add_seconds_local(120, move || {
            let stop = match weak.upgrade() {
                Some(app) if app.prefs.borrow().memory_saver => {
                    app.sleep_others();
                    // No background tab has a live view left, so every tick
                    // from now on would wake the process for nothing. Stop,
                    // and let create_view arm the timer again when a new
                    // view appears.
                    !app.has_sleepable_tab()
                }
                // The app is gone, or the feature was turned off since the
                // last tick. Break removes the source, so the slot is
                // cleared and the next start_memory_timer() can re-arm.
                _ => true,
            };
            if stop {
                MEMORY_TIMER.with(|slot| slot.borrow_mut().take());
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        MEMORY_TIMER.with(|slot| *slot.borrow_mut() = Some(id));
    }

    // ---- the field, panels, find ---------------------------------------

    pub fn open_omni(&self, over: bool) {
        // Two overlays answering at once read as a broken one: the site card
        // would keep focus under the address field and Escape would only take
        // down whichever opened first. The field takes the screen alone.
        crate::sitecard::dismiss(self);
        // The ordinary field: every kind of row. Ctrl K sets the tabs-only
        // mode after this, so each opening starts from a known mode.
        TAB_SWITCHER.with(|mode| mode.set(false));
        // Keep the old text until show() sets the address: setting the
        // placeholder first fired `changed` with the previous session's
        // query, which asked the search engine and filled the list with its
        // suggestions before the real text replaced them.
        self.omni.show(&self.ui, over);
        self.omni.entry.set_placeholder_text(Some("Search or enter address"));
        self.ui.overlay.reorder_overlay(&self.omni.holder, -1);
        let (w, h) = (
            self.ui.overlay.allocated_width().max(self.ui.window.allocated_width()),
            self.ui.overlay.allocated_height().max(self.ui.window.allocated_height()),
        );
        if w > 1 && h > 1 {
            self.omni.layout_in(w, h, self.side_width(), 0);
        }
        let entry = self.omni.entry.clone();
        let still_open = self.omni.visible.clone();
        glib::idle_add_local(move || {
            // The omnibox can close before this idle runs (Escape pressed in
            // the same frame, or a quick open and close). Then the page owns
            // the keyboard, so the entry must not take focus back.
            if still_open.get() {
                entry.grab_focus();
                // The address comes up selected, so typing replaces it and
                // Return goes where the field already says.
                entry.select_region(0, -1);
            }
            glib::ControlFlow::Break
        });
    }

    pub fn close_omni(&self) {
        TAB_SWITCHER.with(|mode| mode.set(false));
        // hide() sets the visible flag to false at once, before the exit fade
        // runs, so restore_page_focus() skips the omnibox and goes to the next
        // open layer. With no layer open it goes to the start page search on a
        // blank tab, or to the active view if that view is mapped. An error
        // page keeps its view but leaves it unmapped, and in that case the
        // helper falls back to the stack.
        self.omni.hide(&self.ui);
        self.restore_page_focus();
    }

    pub fn offers(&self, typed: &str) -> Vec<Offer> {
        let mut offers: Vec<Offer> = Vec::new();
        let typed_lower = typed.to_lowercase();
        // A path-shaped query names a page, not a web search. Do not insert
        // the "Search for ..." row above the exact hit in that case, and
        // ask the engine for suggestions only for a word search.
        let path_query = query_path(typed).is_some();
        // Every typed word has to be found, in any order. A word with one
        // letter wrong, missing or swapped still counts (two for words of
        // eight letters or more), so "youtbue" finds YouTube. See src/fuzzy.rs.
        let words: Vec<&str> = typed_lower.split_whitespace().collect();
        let hit = |hay: &str| words.is_empty() || crate::fuzzy::score(&words, hay).is_some();
        // How well a page answers the query: an exact path beats a title that
        // happens to contain the word, which beats a typo match. This is the
        // ranking the Open group had no notion of: it filled its three seats
        // in tab order, so "/office" showed X, Notion and Figma while the
        // one tab actually on /office was filed under History.
        let strength = |title: &str, url: &str| offer_strength(&words, title, url);
        // Ctrl K lists open tabs only, and all of them up to the list's eight
        // seats instead of the usual three.
        let switching = tab_switcher_on();
        let tab_seats = if switching { 8 } else { 3 };
        let state = self.state.borrow();
        let mut open: Vec<(i32, usize, Offer)> = Vec::new();
        for (index, tab) in state.tabs.iter().enumerate() {
            if tab.url.is_empty() {
                continue;
            }
            let hay = format!("{} {}", tab.title, tab.url).to_lowercase();
            // Two tabs on the same page are one suggestion, not two.
            if open.iter().any(|(_, _, o)| o.url == tab.url) {
                continue;
            }
            if hit(&hay) {
                open.push((
                    strength(&tab.title, &tab.url),
                    index,
                    Offer {
                        title: if tab.title.is_empty() { tab.url.clone() } else { tab.title.clone() },
                        url: tab.url.clone(),
                        kind: "Open".into(),
                    },
                ));
            }
        }
        open.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        for (_, _, offer) in open.into_iter().take(tab_seats) {
            offers.push(offer);
        }
        // The tab switcher stops here: no kept pages, no history and no
        // search row, because Return there means "that tab", not "go".
        if switching {
            return offers;
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
            if hit(&hay) && !offers.iter().any(|o| o.url == bookmark.url) {
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
            if hit(&hay) && !offers.iter().any(|o| o.url == entry.url) {
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
        if !words.is_empty() && !path_query {
            let prefs = self.prefs.borrow();
            // "yt cats" is what Return sends to YouTube, so the top row says
            // that. It must not offer the default engine for two words that
            // Return will never send there.
            let shortcut = crate::keyword::find_named(words, &prefs.keywords);
            let target = plain_target(words, &|query| prefs.search_url(query));
            let is_search = target == prefs.search_url(words);
            drop(prefs);
            if let Some((url, name, rest)) = shortcut {
                if !offers.iter().any(|o| o.url == url) {
                    offers.insert(0, Offer {
                        title: format!("Search {name} for \"{rest}\""),
                        url,
                        kind: "Keyword".into(),
                    });
                    offers.truncate(8);
                }
            } else if is_search && !offers.iter().any(|o| o.url == target) {
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
        let mut offers = self.offers(typed);
        // The engine's words go under the field's own rows, in the seats left.
        // Asking never happens from a private tab, the Ctrl K switcher, or a
        // keyword shortcut: see crate::suggest::url_if_asked.
        let private = self.state.borrow().active_tab().is_none_or(|tab| tab.private);
        let prefs = self.prefs.borrow().clone();
        let no_engine = tab_switcher_on() || query_path(typed).is_some();
        match crate::suggest::url_if_asked(typed, &prefs, private, no_engine) {
            Some(url) => {
                let asked = typed.trim();
                crate::suggest::ask(asked, url);
                for word in crate::suggest::rows_for(asked) {
                    if offers.len() >= 8 {
                        break;
                    }
                    // A suggestion is always a search. Sending it through resolve()
                    // would turn "google.com" into a visit to google.com and
                    // "yt cats" into a keyword jump, neither of which was picked.
                    // The search is built directly with the engine the field uses.
                    let url = prefs.search_url(&word);
                    if !offers.iter().any(|o| o.url == url) {
                        offers.push(Offer { title: word, url, kind: "Suggest".into() });
                    }
                }
            }
            None => crate::suggest::stop(),
        }
        if let Some(app) = current() {
            self.omni.set_offers(&self.ui, &app, offers);
        }
    }

    /// Every colour a dark page forces. Kept as one string so a test can
    /// check that inline-styled descendants are covered too: without `b`,
    /// `strong` and classed links, round 77's dark office page still had
    /// near-black words on a near-black ground.
    const DARK_PAGE_TONE: &str = "html { background: #0b0b0b !important; color: #ededed !important; }\n\
        body { background-color: transparent !important; color: #ededed !important; }\n\
        p, span, div, li, dt, dd, td, th, h1, h2, h3, h4, h5, h6, blockquote, label, figcaption,\n\
        b, strong, em, small, code, pre, time, font, abbr, q, cite { color: #ededed !important; }\n\
        a, a:visited { color: #9ecbff !important; }\n\
        a[class] { color: #ededed !important; }\n\
        :root { color-scheme: dark; }";

    /// When dark pages are asked for, a site that paints white gets the
    /// browser's ground instead, and its text a colour that can be read on it.
    /// The page tone, applied to one view. Kept separate from the loop so a tab
    /// opened after the switch gets the same sheet as the ones already open.
    pub(crate) fn tone_for(&self, view: &webkit2gtk::WebView) {
        // The switch means what it says. It used to be gated on the frame
        // being dark as well, so in Light it flipped, saved, and did nothing —
        // a control that lies about its own state.
        let on = self.prefs.borrow().darken_pages;
        // Forcing the ground without forcing the ink is what made a page of
        // #222 text on #0b0b0b: 1.24:1, unreadable. The two go together or
        // not at all.
        let sheet = if on { Self::DARK_PAGE_TONE } else { "" };
        let Some(manager) = view.user_content_manager() else { return };
        use glib::translate::ToGlibPtr;
        let raw: *mut webkit2gtk::ffi::WebKitUserContentManager = manager.to_glib_none().0;
        let key = raw as usize;
        let previous = TONE_SHEETS.with(|sheets| sheets.borrow_mut().remove(&key));
        if let Some(previous) = previous {
            manager.remove_style_sheet(&previous);
        }
        view.set_background_color(&self.page_canvas());
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
        TONE_SHEETS.with(|sheets| {
            sheets.borrow_mut().insert(key, style);
        });
    }

    pub fn apply_page_tone(&self) {
        let canvas = self.page_canvas();
        let ids: Vec<TabId> = self.state.borrow().tabs.iter().map(|tab| tab.id).collect();
        for id in ids {
            if let Some(view) = self.view(id) {
                self.tone_for(&view);
                view.set_background_color(&canvas);
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
            self.omni.holder.set_size_request(-1, -1);
            if let Ok(panels) = self.panels.try_borrow() {
                panels.holder.set_size_request(-1, -1);
            }
            if let Ok(find) = self.find.try_borrow() {
                find.holder.set_size_request(-1, -1);
            }
        }
        let column = self.side_width();
        self.omni.layout_in(width, height, column, 0);
        if let Ok(panels) = self.panels.try_borrow() {
            panels.layout_with_column(width, height, column);
        }
        if let Ok(find) = self.find.try_borrow() {
            find.layout_in(width, crate::find::toolbar_band(&self.ui), column);
        }
        crate::peek::relayout(self);
        crate::status::relayout(self);
        crate::welcome::relayout(self);
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
            {
                let mut panels = self.panels.borrow_mut();
                panels.set_filter(&app, kind, text);
                self.ui.overlay.reorder_overlay(&panels.holder, -1);
            }
            let (w, h) = (
                self.ui.overlay.allocated_width().max(self.ui.window.allocated_width()),
                self.ui.overlay.allocated_height().max(self.ui.window.allocated_height()),
            );
            if w > 1 && h > 1 {
                self.panels.borrow().layout_with_column(w, h, self.side_width());
            }
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
        // The site card belongs to the page; a panel replaces it, rather than
        // sitting on top of it under the same Escape key.
        crate::sitecard::dismiss(self);
        if let Some(app) = current() {
            {
                let mut panels = self.panels.borrow_mut();
                panels.open_panel(&app, kind);
                self.ui.overlay.reorder_overlay(&panels.holder, -1);
            }
            let (w, h) = (
                self.ui.overlay.allocated_width().max(self.ui.window.allocated_width()),
                self.ui.overlay.allocated_height().max(self.ui.window.allocated_height()),
            );
            if w > 1 && h > 1 {
                self.panels.borrow().layout_with_column(w, h, self.side_width());
            }
        }
    }

    pub fn toggle_panel_switch(&self, kind: PanelKind) {
        self.open_panel(kind);
    }

    pub fn panel_arrow(&self, forwards: bool) {
        let current = self.panels.borrow().open.clone();
        let Some(current) = current.as_ref().and_then(|kind| PanelKind::all().iter().position(|candidate| candidate == kind)) else { return };
        let all = PanelKind::all();
        let next = if forwards { (current + 1) % all.len() } else { (current + all.len() - 1) % all.len() };
        self.open_panel(all[next].clone());
    }

    pub fn panel_focus(&self) {
        let (card, opened) = {
            let panels = self.panels.borrow();
            (panels.card.clone(), panels.open.clone())
        };
        card.grab_focus();
        // The deferred step only runs if the same panel is still open and no
        // newer layer (the omnibox) has taken over. Before, it ran
        // unconditionally and could steal the next keystroke from whatever
        // opened in those 10ms.
        glib::timeout_add_local_once(std::time::Duration::from_millis(10), move || {
            let Some(app) = current() else { return };
            let still_open = app
                .panels
                .try_borrow()
                .ok()
                .is_some_and(|panels| panels.open.is_some() && panels.open == opened);
            if !still_open || app.omni.visible.get() {
                return;
            }
            if !card.child_focus(gtk::DirectionType::TabForward) {
                let close = app.panels.borrow().close_button.clone();
                close.grab_focus();
            }
        });
    }

    pub fn close_panels(&self) {
        self.panels.borrow_mut().close(&self.ui);
        self.restore_page_focus();
    }

    /// After a panel or the find bar closes, keyboard focus has to go somewhere
    /// on purpose. Otherwise it stays on a widget that is fading out, and the
    /// next keystroke goes nowhere. The topmost layer that is still open keeps
    /// focus. If no layer is open, focus goes to the page, or to the start page
    /// when the tab has no view.
    pub(crate) fn restore_page_focus(&self) {
        // The topmost layer that is still open gets focus. The old early
        // returns left focus on the widget that was fading out, so the next
        // key went nowhere.
        if self.omni.visible.get() {
            self.omni.entry.grab_focus();
            return;
        }
        let find_entry = self
            .find
            .try_borrow()
            .ok()
            .filter(|find| find.visible)
            .map(|find| find.entry.clone());
        if let Some(entry) = find_entry {
            entry.grab_focus();
            return;
        }
        // Clone the holder first, so no borrow of the panels is held while
        // GTK runs focus handlers that may borrow them again.
        let panel_holder = self
            .panels
            .try_borrow()
            .ok()
            .filter(|panels| panels.open.is_some())
            .map(|panels| panels.holder.clone());
        if let Some(holder) = panel_holder {
            if holder.child_focus(gtk::DirectionType::TabForward) {
                return;
            }
        }
        // No layer is open. A blank tab has no view, so its start page search
        // field takes focus, the same as close_omni does.
        if self.ui.stack.visible_child_name().as_deref() == Some("blank") {
            self.ui.start.search.grab_focus();
            return;
        }
        if let Some(view) = self.view(self.active_id()) {
            if view.is_mapped() {
                view.grab_focus();
                return;
            }
        }
        self.ui.stack.child_focus(gtk::DirectionType::TabForward);
    }

    pub fn toggle_find(&self) {
        let visible = self.find.borrow().visible;
        if visible {
            self.find.borrow_mut().hide(&self.ui);
            self.restore_page_focus();
            return;
        }
        {
            let mut find = self.find.borrow_mut();
            find.show(&self.ui);
            self.ui.overlay.reorder_overlay(&find.holder, -1);
            let w = self.ui.overlay.allocated_width().max(self.ui.window.allocated_width());
            if w > 1 {
                // find.show() has already placed the bar under the toolbar. A band
                // of 0 here moved it straight back over the toolbar controls and
                // the window marks, so use the same toolbar band as show().
                find.layout_in(w, crate::find::toolbar_band(&self.ui), self.side_width());
            }
            if let Some(view) = self.view(self.active_id()) {
                find.detach();
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
        // Takes down the previous match mark. Each <mark data-search-find> is
        // unwrapped and its parent normalised, so the text nodes the walker
        // reads below are the page's own again. This runs at the start of
        // every step, so the page never holds more than one match mark.
        const unmark = () => {
            const parents = new Set();
            for (const mark of document.querySelectorAll('mark[data-search-find]')) {
                const parent = mark.parentNode;
                if (!parent) continue;
                while (mark.firstChild) parent.insertBefore(mark.firstChild, mark);
                parent.removeChild(mark);
                parents.add(parent);
            }
            for (const parent of parents) parent.normalize();
            if (window.CSS && CSS.highlights) CSS.highlights.delete('search-find');
        };
        unmark();
        // An empty query clears the mark and the selection, so clearing the
        // box leaves no amber behind.
        if (!text) {
            window.__searchFindAt = undefined;
            if (selection) selection.removeAllRanges();
            return 'none';
        }
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
        // Where the current match is. The step marks a match with a <mark>,
        // not a selection (see below), and records its offset in the stitched
        // text as window.__searchFindAt. unmark() above restores exactly the
        // text that was there, so the offset still points at the same match.
        // The page selection is the fallback, for text the reader selected by
        // hand before Find.
        const pointOf = (node, offset) => {
            if (!node) return -1;
            const probe = document.createRange();
            if (node.nodeType === Node.TEXT_NODE) probe.setStart(node, Math.min(offset, node.length));
            else probe.setStart(node, Math.min(offset, node.childNodes.length));
            probe.collapse(true);
            if (probe.startContainer.nodeType !== Node.TEXT_NODE) return -1;
            const index = nodes.indexOf(probe.startContainer);
            return index >= 0 ? starts[index] + probe.startOffset : -1;
        };
        let current = typeof window.__searchFindAt === 'number' && window.__searchFindAt < hay.length
            ? window.__searchFindAt
            : selection && selection.rangeCount ? pointOf(selection.anchorNode, selection.anchorOffset) : -1;
        const finder = new RegExp(text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'), 'gi');
        const hits = [];
        let found;
        while ((found = finder.exec(hay)) !== null && hits.length < 20000) hits.push([found.index, found[0].length]);
        // No match left: drop the previous mark too, so typing on to
        // '0 matches' does not leave the old amber highlight on the page.
        if (!hits.length) {
            window.__searchFindAt = undefined;
            if (selection) selection.removeAllRanges();
            return 'none';
        }
        let pick = null;
        if (forward) {
            pick = hits.find((hit) => hit[0] > current || (__STAY__ && hit[0] === current)) || hits[0];
        } else {
            for (const hit of hits) if (hit[0] < current) pick = hit;
            pick = pick || hits[hits.length - 1];
        }
        const start = at(pick[0]), end = at(pick[0] + pick[1] - 1);
        if (!start || !end) return 'none';
        const range = document.createRange();
        range.setStart(start[0], start[1]);
        range.setEnd(end[0], end[1] + 1);
        // WebKitGTK 2.54 accepts CSS custom highlights but does not paint them.
        // Probe 4 set a magenta highlight on a plain word and the amber one on
        // the code chip; both were set and neither showed, while a plainly
        // styled span did. A page selection is drawn grey while the find entry
        // has focus. So the current match is wrapped in <mark data-search-find>,
        // one per text node it touches. unmark() takes them down at the start
        // of the next step, on no hits, on an empty query, and when Find closes.
        if (selection) selection.removeAllRanges();
        if (!document.getElementById('__search_find_style')) {
            const style = document.createElement('style');
            style.id = '__search_find_style';
            style.textContent = 'mark[data-search-find] { all: unset; background-color: #f2c94c !important; color: #1b1a17 !important; border-radius: 2px; -webkit-box-decoration-break: clone; box-decoration-break: clone; }';
            (document.head || document.documentElement).appendChild(style);
        }
        const first = nodes.indexOf(start[0]), last = nodes.indexOf(end[0]);
        const marks = [];
        for (let i = first; i >= 0 && i <= last; i++) {
            let node = nodes[i];
            const from = i === first ? start[1] : 0;
            const to = i === last ? end[1] + 1 : node.nodeValue.length;
            if (to <= from) continue;
            if (to < node.nodeValue.length) node.splitText(to);
            if (from > 0) node = node.splitText(from);
            const mark = document.createElement('mark');
            mark.setAttribute('data-search-find', '');
            node.parentNode.insertBefore(mark, node);
            mark.appendChild(node);
            marks.push(mark);
        }
        window.__searchFindAt = pick[0];
        // Centred on the match itself. The element holding it can be a whole
        // paragraph, so aiming at the element put a short match on a tall
        // block's middle rather than in the middle of the view; the range's
        // own box is the text that was actually found. Sideways only when the
        // match would otherwise be off the edge.
        // Splitting and wrapping moved the original range's boundaries, so the
        // box comes from the marks themselves.
        let box = range.getBoundingClientRect();
        if (marks.length) {
            const span = document.createRange();
            span.setStartBefore(marks[0]);
            span.setEndAfter(marks[marks.length - 1]);
            box = span.getBoundingClientRect();
        }
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
        // The answer is '<offset> <position> <total>'. The offset comes first, so
        // the bench line 'find step answered 38 ...' still starts with the same
        // number. find_select reads the position and total so the bar can say
        // '3 of 12' (Gemini R60 polish). These are the same hits the step walks
        // through, so the position always matches the match that is highlighted.
        return pick[0] + ' ' + (hits.indexOf(pick) + 1) + ' ' + hits.length;
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
        self.find_select(text, forward, false);
    }

    /// Selects a match in the page. With `stay`, a match that starts where the
    /// selection already starts is kept, so typing one more letter of the same
    /// word leaves the match where it is instead of jumping to the next one.
    fn find_select(&self, text: &str, forward: bool, stay: bool) {
        let Some(view) = self.view(self.active_id()) else { return };
        if text.is_empty() {
            if let Some(controller) = view.find_controller() {
                controller.search("", 0, 0);
            }
        }
        let quote = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".into());
        // The flags are filled in first: the query comes last, and a query that
        // happened to spell one of the markers would otherwise be replaced too.
        let script = Self::FIND_STEP_JS
            .replacen("__FORWARD__", if forward { "true" } else { "false" }, 1)
            .replacen("__STAY__", if stay { "true" } else { "false" }, 1)
            .replacen("__TEXT__", &quote, 1);
        // The step script's answer used to be dropped, so a script that threw
        // left no trace: find counted matches but painted nothing, and nothing
        // said why. Under the bench the answer or the engine's error is printed,
        // so a probe can see where the script stopped. Normal runs stay quiet.
        let bench = std::env::var_os("SEARCH_BENCH").is_some();
        // The step answers '<offset> <position> <total>' (see FIND_STEP_JS).
        // The bar shows '3 of 12' from the same hits the step walked, so the
        // number always names the highlighted match (Gemini R60 polish). The
        // answer 'none' leaves the engine's 'no matches' in place.
        let count = self.find.try_borrow().ok().map(|find| find.count.clone());
        crate::js::run(&view, &script, move |result| {
            if bench {
                match &result {
                    Ok(value) => eprintln!("search: find step answered {value}"),
                    Err(error) => eprintln!("search: find step failed: {error}"),
                }
            }
            if let (Some(count), Ok(value)) = (count, result.as_ref()) {
                let mut parts = value.split_whitespace().skip(1).map(|part| part.parse::<u32>());
                if let (Some(Ok(at)), Some(Ok(total))) = (parts.next(), parts.next()) {
                    if at >= 1 && at <= total {
                        count.set_text(&format!("{at} of {total}"));
                    }
                }
            }
        });
    }

    /// Counts matches with the engine, then marks the current one as a page
    /// selection.
    ///
    /// The engine's own search painted the match with WebKit's built-in
    /// text-match highlight: bright yellow with the page's own text colour on
    /// top. Page CSS cannot change it, so on a dark page it showed light text
    /// on yellow. A selection uses the reader's ::selection rule (dark text on
    /// amber), so the match now looks the same in light and dark. The count
    /// still comes from the engine's counted-matches signal.
    pub fn find_query(&self, text: &str) {
        let Some(view) = self.view(self.active_id()) else { return };
        let Some(controller) = view.find_controller() else { return };
        if text.is_empty() {
            controller.search("", 0, 0);
            controller.count_matches("", 0, 0);
        } else {
            let options = webkit2gtk::FindOptions::CASE_INSENSITIVE.bits();
            controller.search_finish();
            controller.count_matches(text, options, 20_000);
            self.find_select(text, true, true);
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
        let (toast, turned_bar_on) = {
            let mut state = self.state.borrow_mut();
            let result = if state.bookmarks.iter().any(|b| b.url == url) {
                state.bookmarks.retain(|b| b.url != url);
                ("No longer kept", false)
            } else {
                let first = state.bookmarks.is_empty();
                state.bookmarks.insert(
                    0,
                    crate::store::Bookmark {
                        url: url.clone(),
                        title: if title.is_empty() { url } else { title },
                    },
                );
                ("Kept", first)
            };
            Records::save_bookmarks(&state.bookmarks);
            result
        };
        // Keeping the first page turns the bar on, so the thing just kept is
        // visible. After that the switch is the person's: it is read with
        // `&&`, so off means off even with pages kept.
        if turned_bar_on {
            let mut prefs = self.prefs.borrow_mut();
            prefs.show_bookmarks_bar = true;
            prefs.save();
        }
        self.flash_toast(toast);
        self.render_bmarks();
        self.refresh_helm();
        if self.panels.borrow().open.clone() == Some(PanelKind::Bookmarks) {
            self.open_panel(PanelKind::Bookmarks);
        }
    }

    pub fn reader(&self) {
        let Some(app) = current() else { return };
        let Some(view) = self.view(self.active_id()) else { return };
        crate::reader::enter(&app, &view);
        self.refresh_helm();
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
        let Some(selector) = sanitize_selector(selector) else {
            return;
        };
        if host.is_empty() {
            return;
        }
        let (private, active) = {
            let mut state = self.state.borrow_mut();
            if !state.hide_mode {
                return;
            }
            let private = state.active_tab().map(|t| t.private).unwrap_or(true);
            let active = state.active;
            state.hide_mode = false;
            (private, active)
        };
        if private {
            if let Some(view) = self.view(active) {
                let css = format!("{selector} {{ display: none !important; }}");
                if let Some(manager) = view.user_content_manager() {
                    if let Some(app) = current() {
                        crate::curtain::sync_manager(&app, &manager, host);
                    }
                    let style = webkit2gtk::UserStyleSheet::new(
                        &css,
                        webkit2gtk::UserContentInjectedFrames::AllFrames,
                        webkit2gtk::UserStyleLevel::User,
                        &[],
                        &[],
                    );
                    manager.add_style_sheet(&style);
                }
                let text = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into());
                let js = format!(
                    "(() => {{ let s = document.getElementById('__search_hidden_private'); if (!s) {{ s = document.createElement('style'); s.id = '__search_hidden_private'; (document.head || document.documentElement).appendChild(s); }} s.textContent = (s.textContent ? s.textContent + '\\n' : '') + {text}; }})();"
                );
                crate::js::run(&view, &js, |_| {});
            }
            self.flash_toast("Hidden for this private tab only");
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
        if let Some(view) = self.view(active) {
            curtain::apply_immediately(&view, self, host);
            if let (Some(app), Some(manager)) = (current(), view.user_content_manager()) {
                crate::curtain::sync_manager(&app, &manager, host);
            }
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
        let active = self.active_id();
        let Some(view) = self.view(active) else { return };
        let (url, private) = self
            .state
            .borrow()
            .tab(active)
            .map(|t| (t.url.clone(), t.private))
            .unwrap_or((String::new(), true));
        let next = zoom_step(view.zoom_level(), delta);
        view.set_zoom_level(next);
        record_zoom(&mut self.zoom_by_host.borrow_mut(), private, &url, next);
    }

    pub fn reset_zoom(&self) {
        let active = self.active_id();
        let Some(view) = self.view(active) else { return };
        let (url, private) = self
            .state
            .borrow()
            .tab(active)
            .map(|t| (t.url.clone(), t.private))
            .unwrap_or((String::new(), true));
        view.set_zoom_level(1.0);
        if !private {
            self.zoom_by_host.borrow_mut().remove(&zoom_key(&url));
        }
    }

    /// The zoom a page at this address should start at: what was last set on
    /// its host this session, else the engine's default.
    pub fn zoom_for(&self, url: &str) -> f64 {
        let key = zoom_key(url);
        if key.is_empty() {
            return 1.0;
        }
        self.zoom_by_host.borrow().get(&key).copied().unwrap_or(1.0)
    }

    /// Prints the active page through the engine's own dialog, over the window.
    pub fn print_page(&self) {
        let Some(view) = self.view(self.active_id()) else { return };
        webkit2gtk::PrintOperation::new(&view).run_dialog(Some(&self.ui.window));
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
        SESSION_DIRTY.store(true, Ordering::Relaxed);
        self.schedule_persist();
    }

    /// Writes the session now. Any direct write (pin toggle, window close,
    /// bench) clears the dirty flag, so the armed timer does not write the
    /// same session again.
    /// Writes session.json now. Returns true when it reached the disk, or
    /// when there was nothing to write because session restore is off.
    /// On a failed write the session stays marked dirty, so the next save
    /// timer tries again. Direct callers (pin, window close, the bench)
    /// may ignore the result.
    pub fn save_session(&self) -> bool {
        SESSION_DIRTY.store(false, Ordering::Relaxed);
        let Some(records) = self.session_records() else {
            return true;
        };
        let saved = Records::save_session(&records);
        if !saved {
            SESSION_DIRTY.store(true, Ordering::Relaxed);
        }
        saved
    }

    /// The tabs session.json would hold right now: no private tabs and no
    /// blank ones, with the active tab marked. Returns None when session
    /// restore is off, which means there is nothing to write. Both the
    /// direct save and the background timer use it, so they record the same thing.
    fn session_records(&self) -> Option<Vec<TabRecord>> {
        if !self.prefs.borrow().restore_session {
            return None;
        }
        let state = self.state.borrow();
        let active = state.active;
        Some(
            state
                .tabs
                .iter()
                .filter(|t| !t.private && !t.url.is_empty())
                .map(|t| TabRecord {
                    url: t.url.clone(),
                    title: t.title.clone(),
                    pinned: t.pinned,
                    active: t.id == active,
                })
                .collect(),
        )
    }

    // ---- menus and toasts ----------------------------------------------

    pub fn close_other_tabs(&self, keep: TabId) {
        let (targets, active) = {
            let state = self.state.borrow();
            let targets: Vec<TabId> = state
                .tabs
                .iter()
                .filter(|t| t.id != keep && !t.pinned)
                .map(|t| t.id)
                .collect();
            (targets, state.active)
        };
        if targets.is_empty() {
            return;
        }
        if active != keep && targets.contains(&active) {
            self.select(keep);
        }
        for id in targets.into_iter().rev() {
            self.close_tab(id);
        }
    }

    pub fn close_tabs_below(&self, anchor: TabId) {
        let (targets, active) = {
            let state = self.state.borrow();
            let Some(pos) = state.position(anchor) else { return };
            if state.tabs[pos].pinned {
                return;
            }
            let targets: Vec<TabId> = state.tabs[pos + 1..]
                .iter()
                .filter(|t| !t.pinned)
                .map(|t| t.id)
                .collect();
            (targets, state.active)
        };
        if targets.is_empty() {
            return;
        }
        if active != anchor && targets.contains(&active) {
            self.select(anchor);
        }
        for id in targets.into_iter().rev() {
            self.close_tab(id);
        }
    }

    pub fn tab_menu(&self, id: TabId, event: &gtk::gdk::Event) {
        let menu = gtk::Menu::new();
        let (pinned, has_others, has_below) = {
            let state = self.state.borrow();
            let pinned = state.tab(id).map(|t| t.pinned).unwrap_or(false);
            let has_others = state.tabs.iter().any(|t| t.id != id && !t.pinned);
            let has_below = if pinned {
                false
            } else {
                state
                    .position(id)
                    .map(|pos| state.tabs[pos + 1..].iter().any(|t| !t.pinned))
                    .unwrap_or(false)
            };
            (pinned, has_others, has_below)
        };
        let view = self.view(id);
        let muted = view.as_ref().is_some_and(|v| v.is_muted());
        let pin_label = if pinned { "Unpin Tab" } else { "Pin Tab" };
        let mute_label = if muted { "Unmute Tab" } else { "Mute Tab" };
        let items: Vec<(&str, bool, MenuItem)> = vec![
            ("Reload Tab", true, Box::new(move |app: &App| app.reload_tab(id))),
            (
                mute_label,
                view.is_some(),
                Box::new(move |app: &App| {
                    if let Some(view) = app.view(id) {
                        view.set_is_muted(!view.is_muted());
                    }
                }),
            ),
            (pin_label, true, Box::new(move |app: &App| app.toggle_pin(id))),
            ("Duplicate Tab", true, Box::new(move |app: &App| app.duplicate(id))),
            ("Close Tab", true, Box::new(move |app: &App| app.close_tab(id))),
            ("Close Other Tabs", has_others, Box::new(move |app: &App| app.close_other_tabs(id))),
            ("Close Tabs Below", has_below, Box::new(move |app: &App| app.close_tabs_below(id))),
        ];
        for (label, sensitive, action) in items {
            let item = gtk::MenuItem::with_label(label);
            item.set_sensitive(sensitive);
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
        crate::ui::class(&menu, "browser-menu");
        let (url, private) = self
            .state
            .borrow()
            .active_tab()
            .map(|t| (t.url.clone(), t.private))
            .unwrap_or_default();
        let lower = url.to_ascii_lowercase();
        let is_http = lower.starts_with("http://") || lower.starts_with("https://");
        let host = if is_http { crate::shield::host_of(&url) } else { String::new() };
        let blocker_off = !private && !host.is_empty() && crate::shield::allowed(&host);
        let blocker_label = if blocker_off { "Blocker On Here" } else { "Blocker Off Here" };
        let (back, forward, loading) = self.state.borrow().active_tab()
            .map(|tab| (tab.can_back, tab.can_forward, tab.loading)).unwrap_or_default();
        let items: Vec<(&str, bool, MenuItem)> = vec![
            ("Back", back, Box::new(|app: &App| app.go_back())),
            ("Forward", forward, Box::new(|app: &App| app.go_forward())),
            (if loading { "Stop Loading" } else { "Reload" }, !url.is_empty(), Box::new(|app: &App| app.reload_or_stop())),
            ("New Tab", true, Box::new(|app: &App| {
                app.new_tab(None, false, false);
            })),
            ("New Private Tab", true, Box::new(|app: &App| {
                app.new_tab(None, true, false);
            })),
            (
                if self.reading.get() { "Leave Reading Mode" } else { "Reading Mode" },
                !url.is_empty(),
                Box::new(|app: &App| app.reader()),
            ),
            ("Find in Page", true, Box::new(|app: &App| app.toggle_find())),
            ("Bookmarks", true, Box::new(|app: &App| app.toggle_panel(PanelKind::Bookmarks))),
            ("History", true, Box::new(|app: &App| app.toggle_panel(PanelKind::History))),
            ("Downloads", true, Box::new(|app: &App| app.toggle_panel(PanelKind::Downloads))),
            ("Extensions", true, Box::new(|app: &App| app.toggle_panel(PanelKind::Extensions))),
            ("Settings", true, Box::new(|app: &App| app.toggle_panel(PanelKind::Settings))),
            ("Shortcuts", true, Box::new(|app: &App| app.toggle_panel(PanelKind::Shortcuts))),
            (blocker_label, is_http && !host.is_empty(), Box::new(|app: &App| app.toggle_blocker())),
        ];
        for (label, sensitive, action) in items {
            let item = gtk::MenuItem::with_label(label);
            item.set_sensitive(sensitive);
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
        menu.popup_at_widget(&self.ui.more, gtk::gdk::Gravity::SouthEast, gtk::gdk::Gravity::NorthEast, None);
    }

    pub fn flash_toast(&self, message: &str) {
        let mut has_blocked = false;
        for old in self.ui.overlay.children() {
            if old.widget_name() == "toast-bar" {
                crate::motion::cancel(&old);
                self.ui.overlay.remove(&old);
            } else if old.widget_name() == "blocked-card" && old.is_visible() {
                has_blocked = true;
            }
        }
        let bottom = if has_blocked { 72 } else { 24 };
        let toast = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        toast.set_widget_name("toast-bar");
        crate::ui::class(&toast, "dlbar");
        toast.set_margin_bottom(bottom);
        toast.set_margin_start(self.side_width() + 12);
        toast.set_margin_end(12);
        toast.set_halign(gtk::Align::Center);
        toast.set_valign(gtk::Align::End);
        let label = gtk::Label::new(Some(message));
        label.set_line_wrap(true);
        label.set_lines(3);
        label.set_max_width_chars(60);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_margin_start(12);
        label.set_margin_end(12);
        label.set_margin_top(7);
        label.set_margin_bottom(7);
        toast.add(&label);
        self.ui.overlay.add_overlay(&toast);
        toast.show_all();
        crate::motion::float_toast_in(&toast, bottom, crate::motion::dur::TRAVEL, crate::motion::dur::ENTER);
        let weak = self.weak();
        let toast_for_timer = toast.clone();
        glib::timeout_add_seconds_local(2, move || {
            if let Some(app) = weak.upgrade() {
                if toast_for_timer.parent().is_some() {
                    let overlay = app.ui.overlay.clone();
                    let toast_done = toast_for_timer.clone();
                    crate::motion::fade_out_then(&toast_for_timer, crate::motion::dur::EXIT, move || {
                        if toast_done.parent().is_some() {
                            overlay.remove(&toast_done);
                        }
                    });
                }
            } else {
                toast_for_timer.hide();
            }
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
                tab.title = error_tab_title(reason, url);
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
        self.ui.error_title.set_text("This page didn't open");
        self.ui.error_reason.set_text(reason);
        self.ui.error_address.set_text(url);
        self.ui.error_retry.set_label(error_action(reason));
        // A child added to a stack after the window was shown still has to be
        // told it is visible before it can be the visible child.
        self.ui.errorpage.show_all();
        self.ui.stack.set_visible_child_name("error");
        self.ui.window.check_resize();
        self.ui.overlay.check_resize();
        self.ui.error_holder.queue_resize();
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
    /// The second way out of a failure. A 4xx has no useful retry, so Back
    /// returns to the page that led here, or opens the address field when
    /// there is nowhere to go back to.
    pub fn error_back_pressed(&self) {
        let id = self.active_id();
        let can_back = self
            .state
            .borrow()
            .tabs
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| tab.can_back)
            .unwrap_or(false);
        if can_back {
            self.go_back();
        } else {
            self.open_omni(true);
        }
    }

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
            let mut dispatched = 0;
            while dispatched < 256 && context.iteration(false) {
                dispatched += 1;
            }
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
            self.ui.start.root.check_resize();
            // The panel's scroller is moved by the bench's panelscroll without
            // a resize of its own, so its pending allocation is settled here too.
            let panels = self.panels.borrow();
            panels.scroll.check_resize();
            panels.holder.check_resize();
            drop(panels);
            let mut events = 0;
            while events < 256 && gtk::events_pending() {
                gtk::main_iteration_do(false);
                events += 1;
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
        let page = self.view(self.active_id())
            .filter(|view| self.ui.stack.visible_child().as_ref() == Some(view.upcast_ref()) && view.is_mapped())
            .and_then(|view| {
                let surface = self.page_snapshot(&view);
                let origin = view.translate_coordinates(&self.ui.overlay, 0, 0).unwrap_or((0, 0));
                surface.map(|surface| (surface, origin.0, origin.1, view.allocated_width(), view.allocated_height()))
            });
        // Any WebView living inside an overlay (such as the Peek card's view)
        // must be composited from WebKit's own snapshot too: `gtk_widget_draw`
        // on an offset WebView shifts its backing surface right and clips it.
        let mut overlay_views: Vec<(gtk::Widget, cairo::Surface, i32, i32, i32, i32)> = Vec::new();
        let shell_widget = self.ui.shell.clone().upcast::<gtk::Widget>();
        let mut found_views = Vec::new();
        for child in self.ui.overlay.children() {
            if child != shell_widget && child.is_visible() {
                collect_webviews(&child, &mut found_views);
            }
        }
        for view in found_views {
            if let Some(snap) = self.page_snapshot(&view) {
                let origin = view.translate_coordinates(&self.ui.overlay, 0, 0).unwrap_or((0, 0));
                let (w, h) = (view.allocated_width(), view.allocated_height());
                overlay_views.push((view.upcast::<gtk::Widget>(), snap, origin.0, origin.1, w, h));
            }
        }
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

            // A failed page is not the WebView behind it. Draw the browser's
            // own error child explicitly, once, after the page composite.
            if self.ui.stack.visible_child_name().as_deref() == Some("error") {
                draw_at(&context, &self.ui.error_holder, &self.ui.overlay, &[]);
            }

            // 2b. the scrim, painted rather than drawn. Its widget is a
            // GtkDrawingArea inside a GtkFixed, and GTK drops a draw call on a
            // child whose allocation is still pending, which is exactly the
            // state a layer is in at capture time. A solid rectangle over the
            // page is the whole of what the scrim is, so fill it here.
            let (red, green, blue, alpha) = crate::ui::scrim_colour(&self.palette.borrow());
            let omni_scrim_visible = self.omni.scrim.is_visible();
            let panels_scrim_visible = self.panels.borrow().scrim.is_visible();
            if self.omni.visible.get() || self.panels.borrow().open.is_some() {
                context.set_source_rgba(red, green, blue, alpha);
                let column = self.side_width() as f64;
                context.rectangle(column, 0.0, width as f64 - column, height as f64);
                let _ = context.fill();
            }

            // 3. whatever is over the page: the field, find, a panel, a peek card.
            // Hide the real scrims while their painted stand-in is in place;
            // drawing both applies the veil twice and makes a light panel black.
            self.omni.scrim.set_visible(false);
            self.panels.borrow().scrim.set_visible(false);
            for child in self.ui.overlay.children() {
                if child == self.ui.shell.clone().upcast::<gtk::Widget>() {
                    continue;
                }
                if child.is_visible() {
                    draw_at(&context, &child, &self.ui.overlay, &overlay_views);
                }
            }
            self.omni.scrim.set_visible(omni_scrim_visible);
            self.panels.borrow().scrim.set_visible(panels_scrim_visible);
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
        // The window's own state, as the window reports each change. The
        // full-screen decisions read this flag, not the window, because the
        // window is still changing when a video's leave signal arrives.
        self.ui.window.connect_window_state_event(move |_, event| {
            if event.changed_mask().contains(gtk::gdk::WindowState::FULLSCREEN) {
                if let Some(app) = weak.upgrade() {
                    app.window_full.set(event.new_window_state().contains(gtk::gdk::WindowState::FULLSCREEN));
                }
            }
            glib::Propagation::Proceed
        });
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
                    app.reload_or_stop();
                }
            });
        }
        {
            let weak = self.weak();
            self.ui.menu.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.open_panel(PanelKind::Settings);
                }
            });
        }

        for (button, intent) in [
            (&self.ui.fold, crate::keys::Intent::FoldColumn),
            (&self.ui.address, crate::keys::Intent::Omni),
            (&self.ui.newtab, crate::keys::Intent::NewTab),
            (&self.ui.private_tab, crate::keys::Intent::NewPrivateTab),
            (&self.ui.library, crate::keys::Intent::Bookmarks),
            (&self.ui.history, crate::keys::Intent::History),
            (&self.ui.downloads, crate::keys::Intent::Downloads),
            (&self.ui.bookmark, crate::keys::Intent::Bookmark),
            (&self.ui.reader, crate::keys::Intent::Reader),
        ] {
            let weak = self.weak();
            button.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    crate::keys::apply(&app, intent);
                }
            });
        }
        {
            let weak = self.weak();
            self.ui.more.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() { app.main_menu(); }
            });
        }
        {
            let weak = self.weak();
            self.ui.site_info.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    if app.state.borrow().active_tab().is_some_and(|tab| !tab.url.is_empty()) {
                        crate::sitecard::open(&app);
                    } else {
                        app.open_panel(PanelKind::Settings);
                    }
                }
            });
        }

        // Drag the window by the band at the top of the column; double-click
        // maximises. The three controls sit inside it, so they swallow their
        // own clicks first; everything from the last control to the page is
        // the handle.
        {
            let weak = self.weak();
            for handle in [&self.ui.top, &self.ui.toolbar] {
                let weak = weak.clone();
                handle.connect_button_press_event(move |_, event| {
                    if let Some(app) = weak.upgrade() { app.begin_drag(event); }
                    crate::gtkx::inh(false)
                });
            }
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

        // Every layer is laid out for the window it is over: a scrim has to
        // cover the window, not a corner of it.
        {
            let weak = self.weak();
            self.ui.overlay.connect_size_allocate(move |_, allocation| {
                if let Some(app) = weak.upgrade() {
                    app.layout_layers(allocation.width(), allocation.height());
                    // An open site card follows its button when the window
                    // is resized.
                    crate::sitecard::relayout(&app);
                }
            });
        }
        {
            let weak = self.weak();
            let fitted = Rc::new(Cell::new(None));
            self.ui.toolbar.connect_size_allocate(move |_, allocation| {
                // A column fold or unfold moves the toolbar, and the site
                // button with it, without resizing the overlay. Re-anchor an
                // open site card on every toolbar allocation, before the
                // early return below, so it follows the button through the
                // fold animation too.
                if let Some(app) = weak.upgrade() {
                    crate::sitecard::relayout(&app);
                }
                let page_width = allocation.width();
                let fit = (page_width >= 720, page_width >= 520, page_width >= 480);
                if fitted.replace(Some(fit)) == Some(fit) {
                    return;
                }
                let fitted = fitted.clone();
                let weak = weak.clone();
                glib::idle_add_local_once(move || {
                    if fitted.get() != Some(fit) {
                        return;
                    }
                    let Some(app) = weak.upgrade() else { return };
                    app.ui.reader.set_visible(fit.0);
                    app.ui.bookmark.set_visible(fit.0);
                    app.ui.address_hint.set_visible(fit.0);
                    app.ui.forward.set_visible(fit.1);
                    app.ui.reload.set_visible(fit.2);
                    app.ui.toolbar.queue_resize();
                    app.ui.toolbar.check_resize();
                });
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
                let back: &gtk::Widget = app.ui.error_back.upcast_ref();
                if !over_widget(page, &app.ui.window, x, y)
                    || over_widget(retry, &app.ui.window, x, y)
                    || over_widget(back, &app.ui.window, x, y)
                {
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
        {
            let weak = self.weak();
            self.ui.error_back.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.error_back_pressed();
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

/// Whether a tab that made sound at `last_audible` may be put to sleep now.
/// Sound inside the sleep window keeps the tab awake, same as the window does
/// for looking at it. A tab that never made sound may sleep.
pub fn may_sleep(last_audible: Option<std::time::Instant>, now: std::time::Instant, minutes: u64) -> bool {
    match last_audible {
        None => true,
        Some(at) => now.saturating_duration_since(at) > std::time::Duration::from_secs(minutes * 60),
    }
}

/// A page holding the camera or microphone is in a call or a recording, even
/// while it is silent. Muted still holds the device, so it counts as capturing.
pub fn is_capturing(camera: webkit2gtk::MediaCaptureState, microphone: webkit2gtk::MediaCaptureState) -> bool {
    use webkit2gtk::MediaCaptureState;
    camera != MediaCaptureState::None || microphone != MediaCaptureState::None
}

/// On leaving a page's full screen: true keeps the window full screen, which
/// is right when the user had F11 on before the page asked. False lets WebKit's
/// default take the window out of full screen, as before.
pub fn keep_window_on_leave(window_was_full: bool) -> bool {
    window_was_full
}

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
    if !widget.is_visible() || !widget.is_child_visible() || !widget.is_mapped() {
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
    let _ = context.save();
    clip_viewport(context, &widget, offset);
    paint_one(context, &widget, offset);
    if layout_only(&widget) {
        for child in capture_children(&widget) {
            draw_shell(context, &child, ancestor, skip);
        }
    }
    let _ = context.restore();
}

fn clip_viewport(context: &cairo::Context, widget: &gtk::Widget, origin: (i32, i32)) {
    if widget.is::<gtk::ScrolledWindow>() || widget.is::<gtk::Viewport>() {
        context.rectangle(origin.0 as f64, origin.1 as f64, widget.allocated_width() as f64, widget.allocated_height() as f64);
        context.clip();
    }
}

fn capture_children(widget: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut children = widget.dynamic_cast_ref::<gtk::Container>().map(|container| container.children()).unwrap_or_default();
    if let Some(scroll) = widget.dynamic_cast_ref::<gtk::ScrolledWindow>() {
        for scrollbar in [scroll.hscrollbar(), scroll.vscrollbar()].into_iter().flatten() {
            if !children.contains(&scrollbar) {
                children.push(scrollbar);
            }
        }
    }
    children
}

fn collect_webviews(widget: &gtk::Widget, out: &mut Vec<webkit2gtk::WebView>) {
    if not_shown(widget) {
        return;
    }
    if let Ok(view) = widget.clone().downcast::<webkit2gtk::WebView>() {
        out.push(view);
        return;
    }
    if let Some(container) = widget.dynamic_cast_ref::<gtk::Container>() {
        for child in container.children() {
            collect_webviews(&child, out);
        }
    }
}

fn draw_at(
    context: &cairo::Context,
    widget: &impl IsA<gtk::Widget>,
    ancestor: &impl IsA<gtk::Widget>,
    views: &[(gtk::Widget, cairo::Surface, i32, i32, i32, i32)],
) {
    let widget = widget.clone().upcast::<gtk::Widget>();
    draw_into(context, &widget, ancestor, views);
}

/// The opacity a capture should paint a widget with. `gtk_widget_draw`
/// ignores CSS opacity, so close buttons need care:
/// - A tab row's close button (`.rowclose`) carries its opacity on the widget
///   itself. `ui::close_opacity` sets it in tab_row and render_rows (0 / 0.6
///   live / 1 hovered), and CSS pins `button.close.rowclose` at 1, so the
///   widget's own value is already the answer. Multiplying again gave 0.36.
/// - A pin's close button is shown only by `.pin:hover button.close`
///   (src/css.rs:141), so it is drawn while the pin or the button is hovered
///   and hidden otherwise.
/// The tab column itself is painted by draw_shell with widget.draw(); this
/// covers rows or pins that show up in an overlay layer via draw_into.
fn opacity_of(widget: &gtk::Widget) -> f64 {
    let context = widget.style_context();
    if !context.has_class("close") || context.has_class("rowclose") {
        return widget.opacity();
    }
    // The button can sit a few boxes below its pin, so walk up to it.
    let mut pin = widget.parent();
    while let Some(candidate) = pin.clone() {
        if candidate.style_context().has_class("pin") {
            break;
        }
        pin = candidate.parent();
    }
    let Some(pin) = pin else {
        return widget.opacity();
    };
    let prelit = |w: &gtk::Widget| w.state_flags().contains(gtk::StateFlags::PRELIGHT);
    let hovered = prelit(widget)
        || widget.parent().as_ref().map(prelit).unwrap_or(false)
        || prelit(&pin);
    if hovered {
        widget.opacity()
    } else {
        0.0
    }
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
        || widget.is::<gtk::Viewport>()
}

fn draw_into(
    context: &cairo::Context,
    widget: &gtk::Widget,
    ancestor: &impl IsA<gtk::Widget>,
    views: &[(gtk::Widget, cairo::Surface, i32, i32, i32, i32)],
) {
    if not_shown(widget) {
        return;
    }
    if widget.is::<webkit2gtk::WebView>() {
        if let Some((_, surface, x, y, w, h)) = views.iter().find(|(w, ..)| w == widget) {
            let _ = context.save();
            context.rectangle(*x as f64, *y as f64, *w as f64, *h as f64);
            context.clip();
            context.translate(*x as f64, *y as f64);
            let _ = context.set_source_surface(surface, 0.0, 0.0);
            context.set_operator(cairo::Operator::Source);
            let _ = context.paint();
            context.set_operator(cairo::Operator::Over);
            let _ = context.restore();
        }
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
    let _ = context.save();
    clip_viewport(context, widget, offset);
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
        for child in capture_children(widget) {
            draw_into(context, &child, ancestor, views);
        }
    }
    let _ = context.restore();
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
            let (horizontal, vertical) = label.layout_offsets();
            let allocation = label.allocation();
            gtk::render_layout(&style, context, (horizontal - allocation.x()) as f64, (vertical - allocation.y()) as f64, &layout);
        }
        style.restore();
        let _status = context.restore();
        return;
    }
    let allocation = widget.allocation();
    let (left, right) = if widget.direction() == gtk::TextDirection::Rtl {
        (widget.margin_end(), widget.margin_start())
    } else {
        (widget.margin_start(), widget.margin_end())
    };
    let slot = gtk::Allocation::new(
        allocation.x() - left,
        allocation.y() - widget.margin_top(),
        allocation.width() + left + right,
        allocation.height() + widget.margin_top() + widget.margin_bottom(),
    );
    widget.size_allocate(&slot);
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

thread_local! {
    // Whether the field is open as the Ctrl K tab switcher. It is set by
    // open_tab_switcher and cleared by open_omni and close_omni.
    static TAB_SWITCHER: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn tab_switcher_on() -> bool {
    TAB_SWITCHER.with(|mode| mode.get())
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
    // A keyword shortcut comes first: "yt cats" is a YouTube search, not two
    // words for the default engine. Only a template that passes
    // keyword::accepts is ever followed.
    if let Some(url) = crate::keyword::find(text, &prefs.keywords) {
        return url;
    }
    plain_target(text, &|query| prefs.search_url(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn engine(query: &str) -> String {
        crate::store::Prefs::default().search_url(query)
    }

    #[test]
    fn zoom_is_remembered_per_host_and_clamped() {
        assert_eq!(zoom_key("https://WWW.Example.com:8443/a?b=1"), "www.example.com");
        assert_eq!(zoom_key("https://example.com/other"), "example.com");
        assert_ne!(zoom_key("https://a.test/"), zoom_key("https://b.test/"));
        assert_eq!(zoom_step(1.0, 0.1), 1.1);
        assert_eq!(zoom_step(0.4, -0.1), 0.4);
        assert_eq!(zoom_step(2.95, 0.1), 3.0);
    }

    #[test]
    fn reverse_find_uses_the_selection_range_not_a_raw_node_offset() {
        let script = App::FIND_STEP_JS;
        assert!(script.contains("document.createRange()"));
        assert!(script.contains("probe.collapse(true)"));
        assert!(!script.contains("current = starts[i] + selection.anchorOffset"));
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

#[cfg(test)]
mod fullscreen_sleep_tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn f11_on_before_the_page_asked_is_kept_on_leave() {
        assert!(keep_window_on_leave(true));
        assert!(!keep_window_on_leave(false));
    }

    #[test]
    fn a_tab_that_never_made_sound_may_sleep() {
        assert!(may_sleep(None, Instant::now(), 10));
    }

    #[test]
    fn a_tab_that_made_sound_inside_the_window_stays_awake() {
        let now = Instant::now();
        let heard = now - Duration::from_secs(9 * 60);
        assert!(!may_sleep(Some(heard), now, 10));
    }

    #[test]
    fn a_tab_quiet_past_the_window_may_sleep() {
        let now = Instant::now();
        let heard = now - Duration::from_secs(11 * 60);
        assert!(may_sleep(Some(heard), now, 10));
    }

    #[test]
    fn a_tab_holding_the_camera_or_microphone_is_capturing_even_when_silent() {
        use webkit2gtk::MediaCaptureState::{Active, Muted, None};
        assert!(!is_capturing(None, None));
        assert!(is_capturing(Active, None));
        assert!(is_capturing(None, Active));
        assert!(is_capturing(Muted, None));
    }

    /// Music that played for half an hour and then stopped. The hook stamps
    /// both edges, so the last stamp is the moment the sound stopped, and the
    /// whole quiet window is counted from there, not from when it started.
    #[test]
    fn the_quiet_window_counts_from_when_the_sound_stopped() {
        let started = Instant::now();
        let stopped = started + Duration::from_secs(30 * 60);
        // Stamped on start, then again on stop: the stop stamp wins, so the
        // value the sleep check reads is the moment the sound stopped.
        let stamps = [started, stopped];
        let last_audible = stamps.iter().copied().max();
        assert_eq!(last_audible, Some(stopped));
        // One minute after it went quiet: still inside the window.
        assert!(!may_sleep(last_audible, stopped + Duration::from_secs(60), 10));
        // Nine minutes after: still inside.
        assert!(!may_sleep(last_audible, stopped + Duration::from_secs(9 * 60), 10));
        // Eleven minutes after: the window has passed, it may sleep.
        assert!(may_sleep(last_audible, stopped + Duration::from_secs(11 * 60), 10));
        // Measured from the start, as before the fix, it would already have
        // been allowed to sleep the moment it stopped.
        assert!(may_sleep(Some(started), stopped, 10));
    }
}

#[cfg(test)]
mod state_logic_tests {
    use super::*;

    #[test]
    fn closed_tab_stack_skips_private_and_blank_and_caps_at_limit() {
        let mut stack = Vec::new();
        let mut blank = Tab::new(1);
        blank.url = "   ".into();
        push_closed_tab(&mut stack, &blank, 0);
        assert!(stack.is_empty());

        let mut priv_tab = Tab::new(2);
        priv_tab.private = true;
        priv_tab.url = "https://secret.example/".into();
        push_closed_tab(&mut stack, &priv_tab, 0);
        assert!(stack.is_empty());

        for i in 0..(CLOSED_TAB_LIMIT + 5) {
            let mut tab = Tab::new(10 + i as TabId);
            tab.url = format!("https://example.com/{i}");
            tab.title = format!("Title {i}");
            tab.pinned = i % 2 == 0;
            push_closed_tab(&mut stack, &tab, i);
        }
        assert_eq!(stack.len(), CLOSED_TAB_LIMIT);
        assert_eq!(stack.first().unwrap().url, "https://example.com/5");
        assert_eq!(stack.last().unwrap().url, format!("https://example.com/{}", CLOSED_TAB_LIMIT + 4));
    }

    #[test]
    fn restore_closed_tab_restores_pinned_and_clamps_index() {
        let mut tabs = vec![Tab::new(1), Tab::new(2)];
        let mut next_id = 3;
        let closed = ClosedTab {
            url: "https://restored.example/".into(),
            title: "Restored".into(),
            pinned: true,
            index: 99,
        };
        let id = restore_closed_tab(&mut tabs, &mut next_id, closed);
        assert_eq!(id, 3);
        assert_eq!(next_id, 4);
        assert_eq!(tabs.len(), 3);
        assert_eq!(tabs[2].url, "https://restored.example/");
        assert_eq!(tabs[2].title, "Restored");
        assert!(tabs[2].pinned);
        assert!(!tabs[2].private);

        let id2 = restore_closed_tab(
            &mut tabs,
            &mut next_id,
            ClosedTab {
                url: "https://first.example/".into(),
                title: "First".into(),
                pinned: false,
                index: 1,
            },
        );
        assert_eq!(tabs[1].id, id2);
        assert_eq!(tabs[1].url, "https://first.example/");
    }

    #[test]
    fn private_zoom_does_not_leak_to_zoom_by_host() {
        let mut map = BTreeMap::new();
        record_zoom(&mut map, true, "https://private.example/page", 1.5);
        assert!(map.is_empty());

        record_zoom(&mut map, false, "https://public.example/page", 1.25);
        assert_eq!(map.get("public.example"), Some(&1.25));
    }

    #[test]
    fn dark_page_tone_covers_styled_descendants() {
        let sheet = App::DARK_PAGE_TONE;
        for selector in ["b,", "strong,", "a[class]", "#ededed !important", "color-scheme: dark"] {
            assert!(sheet.contains(selector), "the dark sheet is missing {selector}");
        }
    }

    #[test]
    fn pin_columns_and_selector_sanitization() {
        assert_eq!(pin_columns(10), 1);
        assert_eq!(pin_columns(212), 5);
        assert_eq!(sanitize_selector("#ad-slot"), Some("#ad-slot"));
        assert_eq!(sanitize_selector("#x</style>"), None);
        assert_eq!(sanitize_selector("   "), None);
    }
}
