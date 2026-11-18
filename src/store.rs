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

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Prefs {
    pub appearance: Look,
    pub sidebar: bool,
    pub side_width: f64,
    pub roomy_tabs: bool,
    pub shield: bool,
    pub curtain: bool,
    pub memory_saver: bool,
    pub sleep_minutes: u32,
    pub restore_session: bool,
    pub search_engine: String,
    pub show_bookmarks_bar: bool,
    pub darken_pages: bool,
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

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            appearance: Look::System,
            sidebar: true,
            side_width: 232.0,
            roomy_tabs: false,
            shield: true,
            curtain: true,
            memory_saver: true,
            sleep_minutes: 30,
            restore_session: true,
            search_engine: "DuckDuckGo".into(),
            show_bookmarks_bar: false,
            darken_pages: false,
            desktop_dark: false,
            skip_restore: false,
        }
    }
}

impl Prefs {
    pub fn path() -> PathBuf {
        files::path_in(&files::config_dir(), "prefs.json")
    }

    pub fn load() -> Prefs {
        files::read_json(&Prefs::path()).map(|p: Prefs| p.sanitised()).unwrap_or_default()
    }

    pub fn sanitised(mut self) -> Prefs {
        self.side_width = self.side_width.clamp(176.0, 440.0);
        if self.search_engine.trim().is_empty() {
            self.search_engine = "DuckDuckGo".into();
        }
        self
    }

    pub fn save(&self) {
        files::write_json(&Prefs::path(), self);
    }

    pub fn row_height(&self) -> i32 {
        if self.roomy_tabs {
            36
        } else {
            28
        }
    }

    /// The address bar turns what was typed into a place to go.
    pub fn search_url(&self, query: &str) -> String {
        let q = urlencode(query);
        let base = match self.search_engine.as_str() {
            "Google" => "https://www.google.com/search?q=",
            "Bing" => "https://www.bing.com/search?q=",
            "Brave" => "https://search.brave.com/search?q=",
            "Startpage" => "https://www.startpage.com/sp/search?query=",
            _ => "https://duckduckgo.com/?q=",
        };
        format!("{base}{q}")
    }

    pub fn engines() -> [&'static str; 5] {
        ["DuckDuckGo", "Google", "Bing", "Brave", "Startpage"]
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

    pub fn save_session(tabs: &[TabRecord]) {
        files::write_json(&Self::file("session.json"), &tabs.to_vec());
    }

    pub fn load_history() -> Vec<HistoryEntry> {
        files::read_json(&Self::file("history.json")).unwrap_or_default()
    }

    pub fn save_history(history: &[HistoryEntry]) {
        files::write_json(&Self::file("history.json"), &history.to_vec());
    }

    /// A visit is kept once per address, newest first, and the list is capped
    /// so it stays a file you could read.
    pub fn record_visit(history: &mut Vec<HistoryEntry>, url: &str, title: &str) {
        if url.is_empty() || url.starts_with("about:") || url.starts_with("search:") {
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

    pub fn load_bookmarks() -> Vec<Bookmark> {
        files::read_json(&Self::file("bookmarks.json")).unwrap_or_default()
    }

    pub fn save_bookmarks(bookmarks: &[Bookmark]) {
        files::write_json(&Self::file("bookmarks.json"), &bookmarks.to_vec());
    }

    pub fn load_hidden() -> BTreeMap<String, Vec<String>> {
        files::read_json(&Self::file("hidden.json")).unwrap_or_default()
    }

    pub fn save_hidden(hidden: &BTreeMap<String, Vec<String>>) {
        files::write_json(&Self::file("hidden.json"), hidden);
    }
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// "3 days ago", the way a history list says it.
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
        assert!(prefs.search_url("two words").starts_with("https://duckduckgo.com/?q=two+words"));
    }

    #[test]
    fn another_engine_is_another_address() {
        let mut prefs = Prefs::default();
        prefs.search_engine = "Google".into();
        assert_eq!(prefs.search_url("rust gtk"), "https://www.google.com/search?q=rust+gtk");
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
}
