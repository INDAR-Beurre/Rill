// What the browser remembers about how it was left, and what it keeps while
// it runs: tabs, history, bookmarks, hidden things.
//
// Each is a small JSON file of its own, so losing one loses only that one.
use crate::files;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum Look {
    Light,
    Dark,
    #[default]
    System,
}

impl Look {
    pub fn title(&self) -> &'static str {
        match self {
            Look::Light => "Light",
            Look::Dark => "Dark",
            Look::System => "System",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StartBookmarksMode {
    #[default]
    IconsAndNames,
    IconsOnly,
    IconsNamesUrl,
}

impl StartBookmarksMode {
    pub fn title(self) -> &'static str {
        match self {
            StartBookmarksMode::IconsAndNames => "Icons and names",
            StartBookmarksMode::IconsOnly => "Icons only",
            StartBookmarksMode::IconsNamesUrl => "Icons, names & URL",
        }
    }

    pub fn all() -> [StartBookmarksMode; 3] {
        [
            StartBookmarksMode::IconsAndNames,
            StartBookmarksMode::IconsOnly,
            StartBookmarksMode::IconsNamesUrl,
        ]
    }
}

pub const MIN_SIDE_WIDTH: f64 = 176.0;
pub const MAX_SIDE_WIDTH: f64 = 440.0;
pub const DEFAULT_SIDE_WIDTH: f64 = 224.0;
pub const MIN_TOP_TAB_WIDTH: i32 = 96;
pub const MAX_TOP_TAB_WIDTH: i32 = 280;
pub const DEFAULT_TOP_TAB_WIDTH: i32 = 144;
pub const MIN_START_SHORTCUTS_COUNT: usize = 2;
pub const MAX_START_SHORTCUTS_COUNT: usize = 12;
pub const DEFAULT_START_SHORTCUTS_COUNT: usize = 8;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StartDitherMode {
    Off,
    Subtle,
    #[default]
    Full,
}

impl StartDitherMode {
    pub fn title(self) -> &'static str {
        match self {
            StartDitherMode::Off => "Off",
            StartDitherMode::Subtle => "Subtle",
            StartDitherMode::Full => "Full",
        }
    }

    pub fn all() -> [StartDitherMode; 3] {
        [
            StartDitherMode::Off,
            StartDitherMode::Subtle,
            StartDitherMode::Full,
        ]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum WindowControlsMode {
    #[default]
    System,
    Browser,
    Hidden,
}

impl WindowControlsMode {
    pub fn title(self) -> &'static str {
        match self {
            WindowControlsMode::System => "System",
            WindowControlsMode::Browser => "Browser",
            WindowControlsMode::Hidden => "Hidden",
        }
    }

    pub fn all() -> [WindowControlsMode; 3] {
        [
            WindowControlsMode::System,
            WindowControlsMode::Browser,
            WindowControlsMode::Hidden,
        ]
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Prefs {
    pub appearance: Look,
    pub sidebar: bool,
    #[serde(default)]
    pub column_folded: bool,
    pub side_width: f64,
    #[serde(default = "default_top_tab_width")]
    pub top_tab_width: i32,
    #[serde(default = "default_true")]
    pub show_toolbar: bool,
    #[serde(default = "default_true")]
    pub show_nav_buttons: bool,
    #[serde(default = "default_true")]
    pub show_back_button: bool,
    #[serde(default = "default_true")]
    pub show_forward_button: bool,
    #[serde(default = "default_true")]
    pub show_reload_button: bool,
    #[serde(default = "default_true")]
    pub show_home_button: bool,
    #[serde(default)]
    pub show_reader_button: bool,
    #[serde(default)]
    pub show_bookmark_button: bool,
    #[serde(default = "crate::customize::ToolbarAction::default_order")]
    pub toolbar_order: Vec<crate::customize::ToolbarAction>,
    #[serde(default = "default_true")]
    pub show_start_shortcuts: bool,
    #[serde(default = "default_start_shortcuts_count")]
    pub start_shortcuts_count: usize,
    #[serde(default)]
    pub start_bookmarks_mode: StartBookmarksMode,
    #[serde(default)]
    pub start_dither: StartDitherMode,
    #[serde(default)]
    pub window_controls: WindowControlsMode,
    pub roomy_tabs: bool,
    pub shield: bool,
    pub curtain: bool,
    pub memory_saver: bool,
    pub sleep_minutes: u32,
    pub restore_session: bool,
    pub search_engine: String,
    pub show_bookmarks_bar: bool,
    /// The window's size as it was left: a browser that opens the size you
    /// last had it is a browser that respects what you were doing. Not its
    /// place, though: the window opens in the middle of the screen whatever
    /// the last one did, because a coordinate is only meaningful on the
    /// monitor it was recorded on.
    #[serde(default = "default_window")]
    pub window_size: (i32, i32),
    /// Whether it was maximised when it was closed. Saved as its own answer
    /// rather than as a size, because a maximised window reports the size of
    /// the screen it filled, which means nothing on a different one.
    #[serde(default)]
    pub window_maximised: bool,
    pub darken_pages: bool,
    /// Turn off the one painting path that is worth a switch.
    ///
    /// What it does, before GTK starts: sets WEBKIT_DISABLE_COMPOSITING_MODE=1.
    /// WebKit reads that once, when its processes are forked, so changing this
    /// takes effect at the next launch and not before. Two other knobs used to
    /// be part of this switch and are now always on, because they were measured
    /// to help with nothing lost: WEBKIT_DISABLE_DMABUF_RENDERER=1 and
    /// MALLOC_ARENA_MAX=2. See main() for the four-run numbers.
    ///
    /// What it is worth, measured over four runs of one idle tab: 87.0, 96.9,
    /// 102.7 and 104.0 MB of the browser's own memory with nothing set;
    /// 60.4, 75.1, 75.2 and 87.8 MB with the two always-on knobs; and 61.8,
    /// 62.2, 64.5 and 66.4 MB with this one added. The last is the steadiest,
    /// which is the reason to keep it. Compositing off on its own, without the
    /// other two, was worse than leaving everything alone: 77.6, 78.2, 80.2
    /// and 88.1 MB. The cost is a slower first paint on pages with many images.
    #[serde(default)]
    pub low_memory: bool,
    /// Keyword shortcuts for the address field: "yt cats" goes straight to
    /// YouTube's search. A file from before this field existed gets the
    /// starting set, the same as a fresh profile. See src/keyword.rs.
    #[serde(default = "crate::keyword::defaults")]
    pub keywords: Vec<crate::keyword::Keyword>,
    /// Suggestions from the search engine in the address field and start page.
    /// Enabled by default; what you type as a search query is sent to the chosen
    /// engine as you type (never in private tabs, or for URLs, emails, paths,
    /// secrets, or keyword shortcuts). See src/suggest.rs.
    #[serde(default = "default_engine_suggestions")]
    pub engine_suggestions: bool,
    /// Set once the welcome card has been shown and started with, so it is
    /// not shown on every launch. See src/welcome.rs.
    #[serde(default)]
    pub welcomed: bool,
    /// Filled in by the browser at launch: whether the desktop is dark.
    #[serde(skip)]
    pub desktop_dark: bool,
    /// Set by --fresh for this launch only: start with one blank tab, but do
    /// not stop recording where you are. Telling a test to start clean is not
    /// the same as telling the browser to forget the session, and conflating
    /// them meant a --fresh start never wrote a session at all.
    #[serde(skip)]
    pub skip_restore: bool,
}

fn default_true() -> bool {
    true
}

fn default_top_tab_width() -> i32 {
    DEFAULT_TOP_TAB_WIDTH
}

fn default_start_shortcuts_count() -> usize {
    DEFAULT_START_SHORTCUTS_COUNT
}

fn default_window() -> (i32, i32) {
    (1180, 760)
}

fn default_engine_suggestions() -> bool {
    true
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            appearance: Look::System,
            sidebar: false,
            column_folded: false,
            side_width: DEFAULT_SIDE_WIDTH,
            top_tab_width: DEFAULT_TOP_TAB_WIDTH,
            show_toolbar: true,
            show_nav_buttons: true,
            show_back_button: true,
            show_forward_button: true,
            show_reload_button: true,
            show_home_button: false,
            show_reader_button: false,
            show_bookmark_button: false,
            toolbar_order: crate::customize::ToolbarAction::default_order(),
            show_start_shortcuts: true,
            start_shortcuts_count: DEFAULT_START_SHORTCUTS_COUNT,
            start_bookmarks_mode: StartBookmarksMode::IconsAndNames,
            start_dither: StartDitherMode::Full,
            window_controls: WindowControlsMode::System,
            roomy_tabs: false,
            shield: true,
            curtain: true,
            memory_saver: true,
            // Ten minutes, not thirty. A tab left alone for ten minutes is
            // rarely the next one used, and a sleeping tab gives back
            // about 30 MB of its own memory (123.3 -> 90.8 MB measured). A tab
            // playing sound is never put to sleep, so music and calls are safe.
            sleep_minutes: 10,
            restore_session: true,
            search_engine: "Startpage".into(),
            show_bookmarks_bar: false,
            window_size: default_window(),
            window_maximised: false,
            darken_pages: false,
            low_memory: false,
            keywords: crate::keyword::defaults(),
            engine_suggestions: true,
            welcomed: false,
            desktop_dark: false,
            skip_restore: false,
        }
    }
}

impl Prefs {
    pub fn reset_toolbar_layout(&mut self) {
        crate::customize::LayoutPreset::Minimal.apply(self);
    }

    pub fn from_json_str(raw: &str) -> Option<Prefs> {
        let value: serde_json::Value = serde_json::from_str(raw).ok()?;
        let has_back = value.get("show_back_button").is_some();
        let has_forward = value.get("show_forward_button").is_some();
        let has_nav = value.get("show_nav_buttons").is_some();
        let mut prefs: Prefs = serde_json::from_value(value).ok()?;
        if !has_back {
            prefs.show_back_button = if has_nav { prefs.show_nav_buttons } else { true };
        }
        if !has_forward {
            prefs.show_forward_button = if has_nav { prefs.show_nav_buttons } else { true };
        }
        Some(prefs.sanitised())
    }

    pub fn path() -> PathBuf {
        files::path_in(&files::config_dir(), "prefs.json")
    }

    pub fn load() -> Prefs {
        use std::io::Read;
        let Ok(file) = std::fs::File::open(Prefs::path()) else {
            return Prefs::default();
        };
        let mut raw = String::new();
        if file.take(2 * 1024 * 1024 + 1).read_to_string(&mut raw).is_err() {
            return Prefs::default();
        }
        if raw.len() > 2 * 1024 * 1024 {
            return Prefs::default();
        }
        Prefs::from_json_str(&raw).unwrap_or_default()
    }

    pub fn sanitised(mut self) -> Prefs {
        if !self.side_width.is_finite() {
            self.side_width = DEFAULT_SIDE_WIDTH;
        }
        self.side_width = self.side_width.clamp(MIN_SIDE_WIDTH, MAX_SIDE_WIDTH);
        self.top_tab_width = self.top_tab_width.clamp(MIN_TOP_TAB_WIDTH, MAX_TOP_TAB_WIDTH);
        self.start_shortcuts_count = self
            .start_shortcuts_count
            .clamp(MIN_START_SHORTCUTS_COUNT, MAX_START_SHORTCUTS_COUNT);
        self.toolbar_order = crate::customize::normalize_toolbar_order(&self.toolbar_order);
        self.show_nav_buttons = self.show_back_button || self.show_forward_button;
        if self.search_engine.trim().is_empty() {
            self.search_engine = "Startpage".into();
        }
        self
    }

    pub fn save(&self) {
        files::write_json(&Prefs::path(), self);
    }

    pub fn row_height(&self) -> i32 {
        if self.roomy_tabs {
            48
        } else {
            34
        }
    }

    /// The address bar turns what was typed into a place to go.
    pub fn search_url(&self, query: &str) -> String {
        let q = urlencode(query);
        let base = match self.search_engine.as_str() {
            "Ecosia" => "https://www.ecosia.org/search?q=",
            "DuckDuckGo" => "https://duckduckgo.com/?q=",
            "Google" => "https://www.google.com/search?q=",
            "Bing" => "https://www.bing.com/search?q=",
            "Brave" => "https://search.brave.com/search?q=",
            _ => "https://www.startpage.com/sp/search?query=",
        };
        format!("{base}{q}")
    }

    pub fn engines() -> [&'static str; 6] {
        ["Startpage", "Ecosia", "DuckDuckGo", "Google", "Bing", "Brave"]
    }
}

pub fn urlencode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct TabRecord {
    pub url: String,
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
    /// The tab that was on top when the session was saved, so a restart
    /// shows the same page instead of always the first tab. Defaults to
    /// false so session files written before this field still load.
    #[serde(default)]
    pub active: bool,
    /// Which Space this tab belongs to. Defaults to `"default"` so session
    /// files written before Spaces existed migrate seamlessly into Default.
    #[serde(default = "crate::spaces::default_space_id")]
    pub space_id: String,
    /// True for the last active tab within its space, so switching spaces
    /// after a restart returns to the tab you were last viewing there.
    #[serde(default)]
    pub space_active: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HistoryEntry {
    pub url: String,
    pub title: String,
    pub at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Bookmark {
    pub url: String,
    pub title: String,
}


pub struct Records;

impl Records {
    fn file(name: &str) -> PathBuf {
        files::path_in(&files::data_dir(), name)
    }

    pub fn load_session() -> Vec<TabRecord> {
        files::read_json(&Self::file("session.json")).unwrap_or_default()
    }

    /// True when session.json reached the disk, or when a newer snapshot was
    /// already there. The snapshot is numbered here, on the calling thread,
    /// so a direct save (window close, pin, bench) always counts as newest.
    pub fn save_session(tabs: &[TabRecord]) -> bool {
        Self::save_session_at(tabs, files::snapshot_number())
    }

    /// Writes a session snapshot numbered earlier on the main thread. It is
    /// safe to call from a worker: an older snapshot never replaces a newer one.
    pub fn save_session_at(tabs: &[TabRecord], snapshot: u64) -> bool {
        files::write_json_ordered(&Self::file("session.json"), &tabs.to_vec(), snapshot)
    }

    pub fn load_spaces() -> crate::spaces::SpaceStore {
        files::read_json::<crate::spaces::SpaceStore>(&Self::file("spaces.json"))
            .map(|s| s.sanitised())
            .unwrap_or_default()
    }

    pub fn save_spaces(spaces: &crate::spaces::SpaceStore) -> bool {
        Self::save_spaces_at(spaces, files::snapshot_number())
    }

    pub fn save_spaces_at(spaces: &crate::spaces::SpaceStore, snapshot: u64) -> bool {
        files::write_json_ordered(&Self::file("spaces.json"), spaces, snapshot)
    }

    pub fn load_history() -> Vec<HistoryEntry> {
        files::read_json(&Self::file("history.json")).unwrap_or_default()
    }

    /// True when history.json reached the disk, or when a newer snapshot was
    /// already there.
    pub fn save_history(history: &[HistoryEntry]) -> bool {
        Self::save_history_at(history, files::snapshot_number())
    }

    /// Writes a history snapshot numbered earlier on the main thread.
    pub fn save_history_at(history: &[HistoryEntry], snapshot: u64) -> bool {
        files::write_json_ordered(&Self::file("history.json"), &history.to_vec(), snapshot)
    }

    /// A visit is kept once per address, newest first, and the list is capped
    /// so it stays a file you could read.
    pub fn record_visit(history: &mut Vec<HistoryEntry>, url: &str, title: &str) {
        let lower = url.trim().to_ascii_lowercase();
        if lower.is_empty()
            || lower.starts_with("about:")
            || lower.starts_with("search:")
            || lower.starts_with("data:")
            || lower.starts_with("blob:")
            || lower.starts_with("file:")
        {
            return;
        }
        history.retain(|e| e.url != url);
        history.insert(
            0,
            HistoryEntry {
                url: url.to_string(),
                title: title.to_string(),
                at: now_secs(),
            },
        );
        history.truncate(4000);
    }

    /// A visit is written before the page has said its name, so the address
    /// field used to offer four rows called "127.0.0.1". When the title does
    /// arrive, it goes onto the newest entry for that address.
    pub fn name_visit(history: &mut [HistoryEntry], url: &str, title: &str) {
        if title.is_empty() {
            return;
        }
        if let Some(entry) = history.iter_mut().find(|e| e.url == url) {
            entry.title = title.to_string();
        }
    }

    pub fn load_bookmarks() -> Vec<Bookmark> {
        files::read_json(&Self::file("bookmarks.json")).unwrap_or_default()
    }

    pub fn save_bookmarks(bookmarks: &[Bookmark]) {
        files::write_json(&Self::file("bookmarks.json"), &bookmarks.to_vec());
    }

    /// The download list survives a restart, like history and bookmarks. The
    /// files are still in the Downloads folder either way; a panel that forgets
    /// them makes the browser look like it lost them.
    pub fn load_downloads() -> Vec<crate::app::DownloadRec> {
        files::read_json(&Self::file("downloads.json")).unwrap_or_default()
    }

    pub fn save_downloads(downloads: &[crate::app::DownloadRec]) {
        // Only what finished or failed is worth keeping: a half-written file
        // from a session that ended is not a download anyone can open. A
        // download from a private tab is never kept, like its history.
        let settled: Vec<crate::app::DownloadRec> = downloads
            .iter()
            .filter(|entry| (entry.done || entry.failed) && !entry.private)
            .take(200)
            .cloned()
            .collect();
        files::write_json(&Self::file("downloads.json"), &settled);
    }

    /// Site decisions, one per host and kind. Saved only for a non-private tab,
    /// which the caller checks before it calls this.
    /// The remembered site decisions, kept in memory after the first read.
    /// Every permission request, every site card and every Settings render used
    /// to read and parse permissions.json from disk again (six call sites in
    /// src/permissions.rs and src/sitecard.rs, Gemini R60 perf). The file is only
    /// written through save_permissions, which updates this copy first, so the
    /// copy and the file can't drift apart within one run. It lives on the GTK
    /// main thread, the only thread that asks.
    fn with_permission_cache<R>(
        f: impl FnOnce(&mut Option<Vec<crate::permissions::Grant>>) -> R,
    ) -> R {
        thread_local! {
            static GRANTS: std::cell::RefCell<Option<Vec<crate::permissions::Grant>>> =
                std::cell::RefCell::new(None);
        }
        GRANTS.with(|cache| f(&mut cache.borrow_mut()))
    }

    pub fn load_permissions() -> Vec<crate::permissions::Grant> {
        Self::with_permission_cache(|cache| {
            cache
                .get_or_insert_with(|| {
                    files::read_json(&Self::file("permissions.json")).unwrap_or_default()
                })
                .clone()
        })
    }

    pub fn save_permissions(grants: &[crate::permissions::Grant]) {
        // The in-memory copy follows the decision even if the disk write fails,
        // so the browser keeps honouring what the person just chose this run.
        Self::with_permission_cache(|cache| *cache = Some(grants.to_vec()));
        files::write_json(&Self::file("permissions.json"), &grants.to_vec());
    }

    pub fn load_hidden() -> BTreeMap<String, Vec<String>> {
        files::read_json(&Self::file("hidden.json")).unwrap_or_default()
    }

    /// True when hidden.json reached the disk, or when a newer snapshot was
    /// already there.
    pub fn save_hidden(hidden: &BTreeMap<String, Vec<String>>) -> bool {
        Self::save_hidden_at(hidden, files::snapshot_number())
    }

    /// Writes a hidden-list snapshot numbered earlier on the main thread.
    pub fn save_hidden_at(hidden: &BTreeMap<String, Vec<String>>, snapshot: u64) -> bool {
        files::write_json_ordered(&Self::file("hidden.json"), hidden, snapshot)
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "3 days ago", the way a history list says it.
/// The heading a visit belongs under: Today, Yesterday, or a date.
///
/// "43 days ago" is honest and useless. A person looking for something they
/// read last Tuesday needs a calendar, and the local one at that — a UTC date
/// would put an evening's reading under the wrong day for half the world.
pub fn day_of(at: u64, now: u64) -> String {
    let local = |secs: u64| {
        gtk::glib::DateTime::from_unix_utc(secs as i64)
            .ok()
            .and_then(|utc| utc.to_local().ok())
    };
    let Some(then) = local(at) else {
        return "Earlier".to_string();
    };
    let today = local(now);
    if let Some(today) = &today {
        let same = then.year() == today.year() && then.day_of_year() == today.day_of_year();
        if same {
            return "Today".to_string();
        }
        if let Ok(yesterday) = today.add_days(-1) {
            if then.year() == yesterday.year() && then.day_of_year() == yesterday.day_of_year() {
                return "Yesterday".to_string();
            }
        }
    }
    then.format("%e %B %Y")
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "Earlier".to_string())
}

pub fn ago(at: u64, now: u64) -> String {
    let s = now.saturating_sub(at);
    match s {
        0..=59 => "just now".into(),
        60..=3599 => format!("{} min ago", s / 60),
        3600..=86399 => format!("{} hr ago", s / 3600),
        86400..=172799 => "yesterday".into(),
        _ => format!("{} days ago", s / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_become_a_search() {
        let prefs = Prefs::default();
        assert_eq!(prefs.search_engine, "Startpage");
        assert!(prefs.engine_suggestions);
        assert_eq!(
            prefs.search_url("two words"),
            "https://www.startpage.com/sp/search?query=two+words"
        );
    }

    #[test]
    fn another_engine_is_another_address() {
        let mut prefs = Prefs::default();
        for (engine, expected) in [
            ("Startpage", "https://www.startpage.com/sp/search?query=rust+%26+gtk"),
            ("Ecosia", "https://www.ecosia.org/search?q=rust+%26+gtk"),
            ("DuckDuckGo", "https://duckduckgo.com/?q=rust+%26+gtk"),
            ("Google", "https://www.google.com/search?q=rust+%26+gtk"),
            ("Bing", "https://www.bing.com/search?q=rust+%26+gtk"),
            ("Brave", "https://search.brave.com/search?q=rust+%26+gtk"),
        ] {
            prefs.search_engine = engine.into();
            assert_eq!(prefs.search_url("rust & gtk"), expected, "engine={engine}");
        }
    }

    #[test]
    fn existing_saved_engine_and_suggestion_opt_out_are_preserved() {
        let dir = scratch("preserve-optout");
        let path = dir.join("prefs.json");
        std::fs::write(
            &path,
            r#"{"search_engine":"DuckDuckGo","engine_suggestions":false}"#,
        )
        .expect("write existing prefs");

        let back: Prefs = files::read_json::<Prefs>(&path)
            .expect("saved prefs parse")
            .sanitised();
        assert_eq!(back.search_engine, "DuckDuckGo");
        assert!(!back.engine_suggestions);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_query_is_escaped() {
        assert_eq!(urlencode("a b&c"), "a+b%26c");
        assert_eq!(urlencode("ünïcode"), "%C3%BCn%C3%AFcode");
    }

    #[test]
    fn visits_are_kept_once_newest_first() {
        let mut history = Vec::new();
        Records::record_visit(&mut history, "https://a.example", "A");
        Records::record_visit(&mut history, "https://b.example", "B");
        Records::record_visit(&mut history, "https://a.example", "A again");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].url, "https://a.example");
        assert_eq!(history[0].title, "A again");
    }

    #[test]
    fn about_pages_are_not_history() {
        let mut history = Vec::new();
        Records::record_visit(&mut history, "about:blank", "");
        assert!(history.is_empty());
    }

    #[test]
    fn ephemeral_and_local_schemes_are_not_history() {
        let mut history = Vec::new();
        for skipped in [
            "data:text/html,<h1>secret</h1>",
            "DATA:text/plain;base64,SGVsbG8=",
            "blob:https://example.com/1234-5678",
            "BLOB:https://example.com/abcd",
            "file:///run/user/1000/search-source-1.html",
            "FILE:///etc/passwd",
            "search:settings",
            "about:blank",
        ] {
            Records::record_visit(&mut history, skipped, "Title");
        }
        assert!(history.is_empty(), "expected no entries, got: {history:?}");

        Records::record_visit(&mut history, "https://example.com/", "Example");
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn a_date_is_said_the_way_a_person_says_it() {
        let now = 1_000_000;
        assert_eq!(ago(now, now), "just now");
        assert_eq!(ago(now - 90, now), "1 min ago");
        assert_eq!(ago(now - 7200, now), "2 hr ago");
        assert_eq!(ago(now - 90_000, now), "yesterday");
        assert_eq!(ago(now - 300_000, now), "3 days ago");
    }

    #[test]
    fn preferences_are_clamped_when_they_are_read() {
        let mut prefs = Prefs::default();
        prefs.side_width = 5000.0;
        let text = serde_json::to_string(&prefs).unwrap();
        let loaded: Prefs = serde_json::from_str(&text).unwrap();
        let fixed = loaded.sanitised();
        assert!((176.0..=440.0).contains(&fixed.side_width));
        assert_eq!(fixed.side_width, 440.0);
    }

    /// A folder of this test's own under the temp dir. Nothing in this file
    /// may read or write the real config folder: a test that saved there would
    /// rewrite the preferences of whoever ran it.
    fn scratch(test: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "search-store-{}-{test}-{nanos}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp folder");
        dir
    }

    #[test]
    fn preferences_survive_a_trip_through_a_file() {
        let dir = scratch("round-trip");
        let path = dir.join("prefs.json");

        let prefs = Prefs {
            show_toolbar: false,
            show_nav_buttons: false,
            show_back_button: false,
            show_forward_button: false,
            show_reload_button: false,
            show_home_button: false,
            show_reader_button: true,
            show_bookmark_button: true,
            toolbar_order: vec![
                crate::customize::ToolbarAction::Address,
                crate::customize::ToolbarAction::Back,
                crate::customize::ToolbarAction::Forward,
                crate::customize::ToolbarAction::Reload,
                crate::customize::ToolbarAction::Home,
                crate::customize::ToolbarAction::Reader,
                crate::customize::ToolbarAction::Bookmark,
            ],
            show_start_shortcuts: false,
            start_shortcuts_count: 6,
            start_bookmarks_mode: StartBookmarksMode::IconsOnly,
            start_dither: StartDitherMode::Subtle,
            window_controls: WindowControlsMode::Hidden,
            appearance: Look::Dark,
            sidebar: false,
            column_folded: true,
            side_width: 301.0,
            top_tab_width: 192,
            roomy_tabs: true,
            shield: false,
            curtain: false,
            memory_saver: false,
            sleep_minutes: 90,
            restore_session: false,
            search_engine: "Startpage".into(),
            show_bookmarks_bar: true,
            window_size: (1280, 720),
            window_maximised: true,
            darken_pages: true,
            low_memory: true,
            keywords: vec![crate::keyword::Keyword::new("rs", "https://docs.rs/releases/search?query=%s")],
            engine_suggestions: true,
            // Neither of these is written: one is answered at launch, the
            // other belongs to this run alone.
            welcomed: true,
            desktop_dark: true,
            skip_restore: true,
        };

        files::write_json(&path, &prefs);
        let back: Prefs = files::read_json(&path).expect("the file this test just wrote parses");

        assert!(!back.show_toolbar);
        assert!(!back.show_nav_buttons && !back.show_back_button && !back.show_forward_button);
        assert!(!back.show_reload_button && !back.show_home_button);
        assert!(back.show_reader_button && back.show_bookmark_button);
        assert_eq!(back.toolbar_order[0], crate::customize::ToolbarAction::Address);
        assert!(!back.show_start_shortcuts);
        assert_eq!(back.start_shortcuts_count, 6);
        assert_eq!(back.start_bookmarks_mode, StartBookmarksMode::IconsOnly);
        assert_eq!(back.start_dither, StartDitherMode::Subtle);
        assert_eq!(back.top_tab_width, 192);
        assert_eq!(back.window_controls, WindowControlsMode::Hidden);
        assert_eq!(back.window_size, (1280, 720));
        assert!(back.window_maximised);
        assert!(back.low_memory);
        assert!(back.show_bookmarks_bar);
        assert_eq!(back.side_width, 301.0);
        assert!(back.column_folded);
        assert_eq!(back.sleep_minutes, 90);
        assert_eq!(back.search_engine, "Startpage");
        assert!(back.roomy_tabs && back.darken_pages);
        assert!(!back.sidebar && !back.shield && !back.curtain && !back.memory_saver);
        assert!(!back.restore_session);
        assert!(matches!(back.appearance, Look::Dark));
        // The two skipped fields come back as defaults, not as what was saved.
        assert!(!back.desktop_dark && !back.skip_restore);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_from_an_older_version_still_loads() {
        let dir = scratch("older");
        let path = dir.join("prefs.json");
        // What a build that predates window_size, window_maximised, low_memory
        // and show_bookmarks_bar wrote. Every one of them has to arrive as its
        // default rather than as a parse error, or an upgrade would silently
        // throw the whole file away.
        std::fs::write(
            &path,
            r#"{"appearance":"light","sidebar":false,"side_width":260.0,
                "roomy_tabs":true,"shield":false,"curtain":false,
                "memory_saver":false,"sleep_minutes":15,
                "restore_session":false,"search_engine":"Bing",
                "darken_pages":true}"#,
        )
        .expect("write the older file");

        let back: Prefs = files::read_json(&path).expect("an older file still parses");

        assert_eq!(back.window_size, default_window());
        assert_eq!(back.window_size, (1180, 760));
        assert!(!back.window_maximised);
        assert!(!back.low_memory);
        assert!(!back.show_bookmarks_bar);
        // The fields that were there are honoured.
        assert_eq!(back.side_width, 260.0);
        assert_eq!(back.sleep_minutes, 15);
        assert_eq!(back.search_engine, "Bing");
        assert!(back.roomy_tabs && back.darken_pages);
        assert!(!back.sidebar);
        assert!(matches!(back.appearance, Look::Light));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_is_not_json_does_not_parse() {
        let dir = scratch("garbage");
        let path = dir.join("prefs.json");
        std::fs::write(&path, b"not json at all").expect("write it");

        // read_json answers None rather than panicking, and load() turns that
        // into the defaults. Nothing in that path may unwrap a user's file.
        assert!(files::read_json::<Prefs>(&path).is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_session_without_space_fields_migrates_into_default() {
        let legacy = r#"[
            {"url":"https://a.example/","title":"A","pinned":true,"active":false},
            {"url":"https://b.example/","title":"B","pinned":false,"active":true}
        ]"#;
        let records: Vec<TabRecord> = serde_json::from_str(legacy).expect("legacy session parses");
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].space_id, crate::spaces::DEFAULT_SPACE_ID);
        assert_eq!(records[1].space_id, crate::spaces::DEFAULT_SPACE_ID);
        assert!(!records[0].space_active);
        assert!(records[1].active);
    }

    #[test]
    fn spaces_store_round_trips_through_disk() {
        let dir = scratch("spaces-roundtrip");
        let path = dir.join("spaces.json");
        let mut store = crate::spaces::SpaceStore::default();
        let work = store.create("Work").unwrap();
        assert!(store.set_active(&work.id));
        files::write_json(&path, &store);

        let loaded = files::read_json::<crate::spaces::SpaceStore>(&path)
            .expect("spaces parse")
            .sanitised();
        assert_eq!(loaded.spaces.len(), 2);
        assert_eq!(loaded.active_space, work.id);
        assert_eq!(loaded.spaces[1].name, "Work");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn toolbar_minimal_defaults_legacy_home_and_reset() {
        let fresh = Prefs::default();
        assert!(fresh.show_toolbar);
        assert!(fresh.show_nav_buttons);
        assert!(fresh.show_reload_button);
        assert!(!fresh.show_home_button);
        assert!(!fresh.show_reader_button);
        assert!(!fresh.show_bookmark_button);

        let legacy: Prefs = serde_json::from_str(r#"{"appearance":"dark"}"#).expect("legacy parses");
        assert!(legacy.show_home_button);
        assert!(!legacy.show_reader_button);
        assert!(!legacy.show_bookmark_button);

        let explicit_false: Prefs = serde_json::from_str(
            r#"{"show_home_button":false,"show_reader_button":true,"show_bookmark_button":true}"#,
        )
        .expect("explicit false parses");
        assert!(!explicit_false.show_home_button);
        assert!(explicit_false.show_reader_button);
        assert!(explicit_false.show_bookmark_button);

        let explicit_true: Prefs = serde_json::from_str(r#"{"show_home_button":true}"#)
            .expect("explicit true parses");
        assert!(explicit_true.show_home_button);

        let mut customized = Prefs {
            show_toolbar: false,
            show_nav_buttons: false,
            show_back_button: false,
            show_forward_button: false,
            show_reload_button: false,
            show_home_button: true,
            show_reader_button: true,
            show_bookmark_button: true,
            show_bookmarks_bar: true,
            sidebar: true,
            column_folded: true,
            top_tab_width: 220,
            side_width: 320.0,
            roomy_tabs: true,
            show_start_shortcuts: false,
            start_shortcuts_count: 4,
            start_dither: StartDitherMode::Off,
            ..Prefs::default()
        };
        customized.reset_toolbar_layout();
        assert!(customized.show_toolbar);
        assert!(customized.show_nav_buttons);
        assert!(customized.show_back_button);
        assert!(customized.show_forward_button);
        assert!(customized.show_reload_button);
        assert!(!customized.show_home_button);
        assert!(!customized.show_reader_button);
        assert!(!customized.show_bookmark_button);
        assert!(!customized.show_bookmarks_bar);
        assert!(!customized.sidebar);
        assert!(!customized.column_folded);
        assert!(!customized.roomy_tabs);
        assert_eq!(customized.top_tab_width, DEFAULT_TOP_TAB_WIDTH);
        assert_eq!(customized.side_width, DEFAULT_SIDE_WIDTH);
        assert!(customized.show_start_shortcuts);
        assert_eq!(customized.start_shortcuts_count, DEFAULT_START_SHORTCUTS_COUNT);
        assert_eq!(customized.start_dither, StartDitherMode::Full);
        assert_eq!(customized.toolbar_order, crate::customize::ToolbarAction::default_order());
    }

    #[test]
    fn legacy_nav_group_migrates_to_independent_back_and_forward() {
        let legacy_off = Prefs::from_json_str(r#"{"show_nav_buttons":false}"#).expect("parses");
        assert!(!legacy_off.show_back_button);
        assert!(!legacy_off.show_forward_button);
        assert!(!legacy_off.show_nav_buttons);

        let legacy_default = Prefs::from_json_str(r#"{"appearance":"dark"}"#).expect("parses");
        assert!(legacy_default.show_back_button);
        assert!(legacy_default.show_forward_button);
        assert!(legacy_default.show_nav_buttons);
        assert!(legacy_default.show_home_button);
        assert!(legacy_default.show_start_shortcuts);
        assert_eq!(legacy_default.start_shortcuts_count, DEFAULT_START_SHORTCUTS_COUNT);
        assert_eq!(legacy_default.start_dither, StartDitherMode::Full);
        assert_eq!(legacy_default.top_tab_width, DEFAULT_TOP_TAB_WIDTH);

        let split = Prefs::from_json_str(r#"{"show_nav_buttons":false,"show_back_button":true,"show_forward_button":false}"#).expect("parses");
        assert!(split.show_back_button);
        assert!(!split.show_forward_button);
        assert!(split.show_nav_buttons);
    }

    #[test]
    fn sanitised_clamps_new_layout_bounds_and_normalizes_toolbar_order() {
        let loaded = Prefs::from_json_str(
            r#"{"side_width":999.0,"top_tab_width":20,"start_shortcuts_count":100,"toolbar_order":["home","home"]}"#,
        )
        .expect("parses");
        assert_eq!(loaded.side_width, MAX_SIDE_WIDTH);
        assert_eq!(loaded.top_tab_width, MIN_TOP_TAB_WIDTH);
        assert_eq!(loaded.start_shortcuts_count, MAX_START_SHORTCUTS_COUNT);
        assert_eq!(loaded.toolbar_order.len(), crate::customize::ToolbarAction::CANONICAL.len());
        assert_eq!(loaded.toolbar_order[0], crate::customize::ToolbarAction::Home);
        assert!(loaded.toolbar_order.contains(&crate::customize::ToolbarAction::Address));
    }
}

#[cfg(test)]
mod naming_tests {
    use super::*;

    fn entry(url: &str, title: &str) -> HistoryEntry {
        HistoryEntry {
            url: url.to_string(),
            title: title.to_string(),
            at: 0,
        }
    }

    #[test]
    fn a_visit_takes_its_name_when_the_page_gives_one() {
        let mut history = vec![entry("https://a.test/", "a.test")];
        Records::name_visit(&mut history, "https://a.test/", "A page");
        assert_eq!(history[0].title, "A page");
    }

    #[test]
    fn a_visit_does_not_get_a_blank_name() {
        let mut history = vec![entry("https://a.test/", "a.test")];
        Records::name_visit(&mut history, "https://a.test/", "");
        assert_eq!(history[0].title, "a.test");
    }
}
