// Brings bookmarks and history over from the other browsers on this machine.
//
// A browser keeps its databases locked while it runs, so each one is copied
// (with its -wal) into a folder of ours and read from the copy. Nothing is
// written to the other browser's files.
use crate::app::App;
use crate::store::{Bookmark, HistoryEntry, Records};
use rusqlite::{Connection, OpenFlags};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// The same cap record_visit keeps.
const LIMIT: usize = 4000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Browser {
    Firefox,
    Chromium,
    Chrome,
    Brave,
    Edge,
    Vivaldi,
}

impl Browser {
    pub const ALL: [Browser; 6] = [
        Browser::Firefox,
        Browser::Chromium,
        Browser::Chrome,
        Browser::Brave,
        Browser::Edge,
        Browser::Vivaldi,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Browser::Firefox => "Firefox",
            Browser::Chromium => "Chromium",
            Browser::Chrome => "Chrome",
            Browser::Brave => "Brave",
            Browser::Edge => "Microsoft Edge",
            Browser::Vivaldi => "Vivaldi",
        }
    }

    /// Where a Chromium-family browser keeps its user-data folder, relative to
    /// home: the native install, then the Snap and Flatpak copies. Firefox has
    /// none (its profiles are found through profiles.ini instead).
    fn chromium_roots(self) -> &'static [&'static str] {
        match self {
            Browser::Firefox => &[],
            Browser::Chromium => &[
                ".config/chromium",
                "snap/chromium/common/chromium",
                ".var/app/org.chromium.Chromium/config/chromium",
            ],
            Browser::Chrome => &[".config/google-chrome", ".var/app/com.google.Chrome/config/google-chrome"],
            Browser::Brave => &[
                ".config/BraveSoftware/Brave-Browser",
                ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser",
            ],
            Browser::Edge => &[".config/microsoft-edge", ".var/app/com.microsoft.Edge/config/microsoft-edge"],
            Browser::Vivaldi => &[".config/vivaldi", ".var/app/com.vivaldi.Vivaldi/config/vivaldi"],
        }
    }

    /// Every profile folder (Default and Profile N) of this browser under `home`.
    fn profile_dirs(self, home: &Path) -> Vec<PathBuf> {
        self.chromium_roots().iter().flat_map(|root| chromium_profiles(&home.join(root))).collect()
    }
}

/// The Default and `Profile N` folders inside a Chromium user-data folder.
pub fn chromium_profiles(root: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
    let mut out: Vec<PathBuf> = entries
        .flatten()
        .filter(|e| e.file_name().to_str().is_some_and(is_profile_name))
        .map(|e| e.path())
        .collect();
    out.sort();
    out
}

fn is_profile_name(name: &str) -> bool {
    name == "Default" || name.strip_prefix("Profile ").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The real home folder. None when HOME is unset, so nothing is read from the
/// working directory by mistake.
pub fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").filter(|h| !h.is_empty()).map(PathBuf::from)
}

/// Firefox profile folders: the native install, then the Snap and Flatpak copies.
const FIREFOX_ROOTS: [&str; 3] = [
    ".mozilla/firefox",
    "snap/firefox/common/.mozilla/firefox",
    ".var/app/org.mozilla.firefox/.mozilla/firefox",
];

/// Each Firefox profile's places.sqlite, from profiles.ini and from any folder
/// under each Firefox root that has one, without listing a profile twice.
pub fn firefox_places(home: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in FIREFOX_ROOTS {
        let base = home.join(root);
        if let Ok(ini) = fs::read_to_string(base.join("profiles.ini")) {
            for db in profile_places(&base, &ini) {
                if db.exists() && !out.contains(&db) {
                    out.push(db);
                }
            }
        }
        if let Ok(entries) = fs::read_dir(&base) {
            for entry in entries.flatten() {
                let db = entry.path().join("places.sqlite");
                if db.exists() && !out.contains(&db) {
                    out.push(db);
                }
            }
        }
    }
    out
}

/// The places.sqlite of each [Profile] section in a profiles.ini. A relative
/// path is taken from the Firefox folder; IsRelative=0 means it is absolute.
pub fn profile_places(base: &Path, ini: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for section in ini_sections(ini) {
        let get = |want: &str| section.iter().find(|(k, _)| *k == want).map(|(_, v)| *v);
        let Some(path) = get("Path") else { continue };
        let folder = if get("IsRelative") == Some("0") {
            PathBuf::from(path)
        } else {
            base.join(path)
        };
        out.push(folder.join("places.sqlite"));
    }
    out
}

fn ini_sections(ini: &str) -> Vec<Vec<(&str, &str)>> {
    let mut sections: Vec<Vec<(&str, &str)>> = Vec::new();
    for line in ini.lines().map(str::trim) {
        if line.starts_with('[') {
            sections.push(Vec::new());
        } else if let (Some((key, value)), Some(section)) = (line.split_once('='), sections.last_mut()) {
            section.push((key.trim(), value.trim()));
        }
    }
    sections
}

/// The display name of each profile a browser has under `home`, in the same
/// order as its folders. Chromium names come from Local State; Firefox names
/// come from profiles.ini for profiles whose places.sqlite exists on disk, plus
/// any unlisted profile folder that contains places.sqlite. A profile with no
/// name shows its folder name.
pub fn profile_names(home: &Path, browser: Browser) -> Vec<String> {
    if browser == Browser::Firefox {
        let mut names = Vec::new();
        let mut seen = Vec::new();
        for root in FIREFOX_ROOTS {
            let base = home.join(root);
            if let Ok(ini) = fs::read_to_string(base.join("profiles.ini")) {
                for section in ini_sections(&ini) {
                    let get = |want: &str| section.iter().find(|(k, _)| *k == want).map(|(_, v)| *v);
                    let Some(path) = get("Path") else { continue };
                    let folder = if get("IsRelative") == Some("0") {
                        PathBuf::from(path)
                    } else {
                        base.join(path)
                    };
                    let db = folder.join("places.sqlite");
                    if !db.exists() {
                        continue;
                    }
                    let key = fs::canonicalize(&db).unwrap_or(db);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    let fallback = folder
                        .file_name()
                        .and_then(|n| n.to_str())
                        .filter(|s| !s.is_empty())
                        .unwrap_or(path);
                    let name = get("Name")
                        .filter(|n| !n.is_empty())
                        .unwrap_or(fallback)
                        .to_string();
                    names.push(name);
                }
            }
            if let Ok(entries) = fs::read_dir(&base) {
                let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
                dirs.sort();
                for folder in dirs {
                    let db = folder.join("places.sqlite");
                    if !db.exists() {
                        continue;
                    }
                    let key = fs::canonicalize(&db).unwrap_or(db);
                    if seen.contains(&key) {
                        continue;
                    }
                    seen.push(key);
                    if let Some(name) = folder.file_name().and_then(|n| n.to_str()).filter(|s| !s.is_empty()) {
                        names.push(name.to_string());
                    }
                }
            }
        }
        return names;
    }
    let mut out = Vec::new();
    for root in browser.chromium_roots() {
        let base = home.join(root);
        let info: serde_json::Value = fs::read_to_string(base.join("Local State"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::Value::Null);
        for dir in chromium_profiles(&base) {
            let key = dir.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let name = info["profile"]["info_cache"][&key]["name"].as_str().map(str::to_string).unwrap_or(key);
            out.push(name);
        }
    }
    out
}

/// The browser's name, followed by its profile names only when it has more
/// than one profile, so a single-profile browser keeps its plain row.
pub fn label(home: &Path, browser: Browser) -> String {
    let names = profile_names(home, browser);
    if names.len() > 1 {
        format!("{} - {}", browser.name(), names.join(", "))
    } else {
        browser.name().to_string()
    }
}

/// Keep only what a browser can open from its address field: web pages.
pub fn keep_web(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://")
}

/// Chromium's Bookmarks file: a tree of folders and urls under "roots".
pub fn chromium_bookmarks(text: &str) -> Vec<Bookmark> {
    let mut out = Vec::new();
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return out;
    };
    if let Some(roots) = value.get("roots").and_then(|r| r.as_object()) {
        for root in roots.values() {
            collect_bookmarks(root, &mut out);
        }
    }
    out
}

fn collect_bookmarks(node: &serde_json::Value, out: &mut Vec<Bookmark>) {
    if node.get("type").and_then(|t| t.as_str()) == Some("url") {
        let url = node.get("url").and_then(|u| u.as_str());
        let name = node.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if let Some(url) = url {
            out.push(Bookmark { url: url.to_string(), title: name.to_string() });
        }
    } else if let Some(children) = node.get("children").and_then(|c| c.as_array()) {
        for child in children {
            collect_bookmarks(child, out);
        }
    }
}

/// Firefox stores visit times in microseconds since 1970.
pub fn firefox_secs(micros: i64) -> u64 {
    (micros / 1_000_000).max(0) as u64
}

/// Chromium stores times in microseconds since 1601-01-01, 11644473600 s before 1970.
pub fn chromium_secs(micros: i64) -> u64 {
    (micros / 1_000_000 - 11_644_473_600).max(0) as u64
}

/// What one browser gave: bookmarks and visits, already deduplicated by URL.
#[derive(Default, Debug)]
pub struct Haul {
    pub bookmarks: Vec<Bookmark>,
    pub visits: Vec<HistoryEntry>,
}

/// A copy of the database in a folder of its own, removed when dropped. The
/// connection is declared after it, so it closes before the folder goes.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut name = db.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

/// Runs `read` on a read-only copy of `db`, its -wal included, so changes the
/// browser has not yet checkpointed are seen too.
fn with_copy<T>(db: &Path, read: impl FnOnce(&Connection) -> rusqlite::Result<T>) -> Option<T> {
    static COUNT: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "search-import-{}-{}",
        std::process::id(),
        COUNT.fetch_add(1, Ordering::Relaxed)
    ));
    use std::os::unix::fs::DirBuilderExt;
    // The folder is created owner-only (0700) and must not already exist, so
    // another local user can neither read the copied history nor plant this
    // folder in advance. The guard is armed only once the folder is ours.
    fs::DirBuilder::new().mode(0o700).create(&dir).ok()?;
    let _guard = Scratch(dir.clone());
    let copy = dir.join(db.file_name()?);
    fs::copy(db, &copy).ok()?;
    let wal = sidecar(db, "-wal");
    if wal.exists() {
        fs::copy(&wal, sidecar(&copy, "-wal")).ok()?;
    }
    let conn = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    match read(&conn) {
        Ok(value) => Some(value),
        Err(error) => {
            note!("search: import from {} failed: {error}", db.display());
            None
        }
    }
}

/// Bookmarks and the newest visits from one Firefox places.sqlite.
fn firefox_db(db: &Path) -> Option<(Vec<Bookmark>, Vec<HistoryEntry>)> {
    with_copy(db, |conn| {
        let mut marks = conn.prepare(
            "SELECT p.url, COALESCE(b.title, p.title, '') FROM moz_bookmarks b \
             JOIN moz_places p ON p.id = b.fk WHERE b.type = 1",
        )?;
        let bookmarks = marks
            .query_map([], |r| Ok(Bookmark { url: r.get(0)?, title: r.get(1)? }))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut visits = conn.prepare(
            "SELECT p.url, COALESCE(p.title, ''), v.visit_date FROM moz_historyvisits v \
             JOIN moz_places p ON p.id = v.place_id ORDER BY v.visit_date DESC LIMIT ?1",
        )?;
        let history = visits
            .query_map([LIMIT as i64], |r| {
                let at: i64 = r.get(2)?;
                Ok(HistoryEntry { url: r.get(0)?, title: r.get(1)?, at: firefox_secs(at) })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok((bookmarks, history))
    })
}

/// The newest URLs from a Chromium History database.
fn chromium_history(db: &Path) -> Option<Vec<HistoryEntry>> {
    with_copy(db, |conn| {
        let mut stmt = conn.prepare(
            "SELECT url, COALESCE(title, ''), last_visit_time FROM urls \
             ORDER BY last_visit_time DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([LIMIT as i64], |r| {
                let at: i64 = r.get(2)?;
                Ok(HistoryEntry { url: r.get(0)?, title: r.get(1)?, at: chromium_secs(at) })
            })?
            .collect::<rusqlite::Result<Vec<_>>>();
        rows
    })
}

/// Reads one browser under `home`. A browser that is not there gives an empty
/// haul; a database that cannot be read is skipped with a line on stderr.
pub fn read(home: &Path, browser: Browser) -> Haul {
    let mut haul = Haul::default();
    let mut marks = Vec::new();
    let mut visits = Vec::new();
    match browser {
        Browser::Firefox => {
            for db in firefox_places(home) {
                if let Some((b, v)) = firefox_db(&db) {
                    marks.extend(b);
                    visits.extend(v);
                }
            }
        }
        _ => {
            for dir in browser.profile_dirs(home) {
                if let Ok(text) = fs::read_to_string(dir.join("Bookmarks")) {
                    marks.extend(chromium_bookmarks(&text));
                }
                visits.extend(chromium_history(&dir.join("History")).unwrap_or_default());
            }
        }
    }
    marks.retain(|b| keep_web(&b.url));
    visits.retain(|v| keep_web(&v.url));
    merge_bookmarks(&mut haul.bookmarks, marks);
    merge_history(&mut haul.visits, visits);
    haul
}

/// The browsers that have a bookmarks or history file under `home`.
pub fn installed(home: &Path) -> Vec<Browser> {
    Browser::ALL
        .into_iter()
        .filter(|b| match b {
            Browser::Firefox => !firefox_places(home).is_empty(),
            _ => b
                .profile_dirs(home)
                .iter()
                .any(|dir| dir.join("Bookmarks").exists() || dir.join("History").exists()),
        })
        .collect()
}

/// Adds the bookmarks whose URL is not kept yet. The kept one wins, so a title
/// the user has already changed is not overwritten.
pub fn merge_bookmarks(existing: &mut Vec<Bookmark>, incoming: Vec<Bookmark>) {
    for bookmark in incoming {
        if !existing.iter().any(|b| b.url == bookmark.url) {
            existing.push(bookmark);
        }
    }
}

/// One entry per URL, the newest visit kept, newest first, capped at LIMIT.
/// Only web pages are taken from `incoming`; `existing` is kept as it is.
pub fn merge_history(existing: &mut Vec<HistoryEntry>, incoming: Vec<HistoryEntry>) {
    let mut merged: Vec<HistoryEntry> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let all = existing.drain(..).chain(incoming.into_iter().filter(|e| keep_web(&e.url)));
    for entry in all {
        match seen.get(&entry.url) {
            Some(&i) => {
                if entry.at > merged[i].at {
                    merged[i] = entry;
                }
            }
            None => {
                seen.insert(entry.url.clone(), merged.len());
                merged.push(entry);
            }
        }
    }
    merged.sort_by(|a, b| b.at.cmp(&a.at));
    merged.truncate(LIMIT);
    *existing = merged;
}

/// "Firefox: 120 bookmarks, 2,000 visits". None when nothing was found, and
/// then the lists are left as they were. The window imports through
/// `bring_later`; this synchronous form is kept for the tests, which read a
/// scratch home on the test thread.
#[cfg(test)]
pub fn apply(home: &Path, browser: Browser, bookmarks: &mut Vec<Bookmark>, history: &mut Vec<HistoryEntry>) -> Option<String> {
    let haul = read(home, browser);
    if haul.bookmarks.is_empty() && haul.visits.is_empty() {
        return None;
    }
    let line = format!(
        "{}: {} bookmarks, {} visits",
        browser.name(),
        grouped(haul.bookmarks.len()),
        grouped(haul.visits.len())
    );
    merge_bookmarks(bookmarks, haul.bookmarks);
    merge_history(history, haul.visits);
    Some(line)
}

fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

pub(crate) fn bring_later_checked(
    app: &crate::app::AppRef,
    browsers: Vec<Browser>,
    done: impl FnOnce(bool, String) + 'static,
) {
    // A second press while one import runs would read the same databases and
    // land the same haul twice, so it is refused rather than queued.
    if IMPORT_RUNNING.swap(true, Ordering::SeqCst) {
        done(false, "An import is already running.".into());
        return;
    }
    let Some(home) = home() else {
        IMPORT_RUNNING.store(false, Ordering::SeqCst);
        done(false, "No home folder to read from.".into());
        return;
    };
    let weak = app.weak();
    gtk::glib::MainContext::default().spawn_local(async move {
        let read_all = gtk::gio::spawn_blocking(move || {
            browsers.into_iter().map(|b| (b, read(&home, b))).collect::<Vec<(Browser, Haul)>>()
        });
        let hauls = read_all.await;
        IMPORT_RUNNING.store(false, Ordering::SeqCst);
        let Some(app) = weak.upgrade() else { return };
        let (ok, message) = match hauls {
            Ok(hauls) => (true, land(&app, hauls)),
            Err(_) => (false, "The import stopped before it finished. Nothing was added.".into()),
        };
        done(ok, message);
    });
}

/// Set while `bring_later` is reading or landing a haul.
static IMPORT_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Merges what was read into the open lists, saves them and redraws the bar.
/// Runs on the main loop: it is the only part that touches the app's state.
fn land(app: &App, hauls: Vec<(Browser, Haul)>) -> String {
    let mut guard = app.state.borrow_mut();
    let state = &mut *guard;
    let mut lines = Vec::new();
    for (browser, haul) in hauls {
        if haul.bookmarks.is_empty() && haul.visits.is_empty() {
            continue;
        }
        lines.push(format!(
            "{}: {} bookmarks, {} visits",
            browser.name(),
            grouped(haul.bookmarks.len()),
            grouped(haul.visits.len())
        ));
        merge_bookmarks(&mut state.bookmarks, haul.bookmarks);
        merge_history(&mut state.history, haul.visits);
    }
    if lines.is_empty() {
        return "Nothing to bring.".into();
    }
    Records::save_bookmarks(&state.bookmarks);
    Records::save_history(&state.history);
    drop(guard);
    app.render_bmarks();
    lines.join("; ")
}

/// The Settings row: what was found, and one button that brings it all in.
pub fn settings_row(app: &crate::app::AppRef) -> gtk::Box {
    use gtk::prelude::*;
    let found = home().map(|h| installed(&h)).unwrap_or_default();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_margin_top(7);
    row.set_margin_bottom(7);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some("Import from another browser"));
    crate::ui::class(&title, "setting");
    title.set_xalign(0.0);
    let names: Vec<String> = match home() {
        Some(h) => found.iter().map(|b| label(&h, *b)).collect(),
        None => Vec::new(),
    };
    let sub_text = if found.is_empty() {
        "No other browser found here. Bookmarks and history come from Firefox, Chromium, Chrome, Brave, Edge or Vivaldi.".to_string()
    } else {
        format!("Found {}. Bookmarks and history are added; nothing is removed.", names.join(", "))
    };
    let sub = gtk::Label::new(Some(&sub_text));
    crate::ui::class(&sub, "settingdesc");
    sub.set_xalign(0.0);
    sub.set_line_wrap(true);
    text.add(&title);
    text.add(&sub);
    crate::ui::pack(&row, &text, true);

    let button = gtk::Button::with_label("Import");
    crate::ui::class(&button, "act");
    button.set_valign(gtk::Align::Center);
    button.set_sensitive(!found.is_empty());
    {
        let weak = app.weak();
        button.connect_clicked(move |b| {
            b.set_sensitive(false);
            let Some(app) = weak.upgrade() else { return };
            // The reading happens off the main loop; the row says so meanwhile,
            // so a slow import does not look like a frozen button.
            sub.set_text("Reading bookmarks and history…");
            let sub = sub.clone();
            let b_weak = b.downgrade();
            bring_later_checked(&app, found.clone(), move |ok, message| {
                sub.set_text(&message);
                if !ok {
                    if let Some(b) = b_weak.upgrade() {
                        b.set_sensitive(true);
                    }
                }
            });
        });
    }
    crate::ui::pack(&row, &button, false);
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("search-import-test-{}-{test}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn history(url: &str, at: u64) -> HistoryEntry {
        HistoryEntry { url: url.into(), title: String::new(), at }
    }

    #[test]
    fn chromium_bookmarks_are_read_at_any_depth() {
        let json = r#"{"roots":{
            "bookmark_bar":{"type":"folder","children":[
                {"type":"url","name":"Top","url":"https://top.test/"},
                {"type":"folder","children":[
                    {"type":"url","name":"Deep","url":"http://deep.test/"},
                    {"type":"url","name":"No url"}
                ]}
            ]},
            "other":{"type":"folder","children":[]},
            "sync_transaction_version":"1"
        }}"#;
        let mut found: Vec<String> = chromium_bookmarks(json).into_iter().map(|b| b.url).collect();
        found.sort();
        assert_eq!(found, vec!["http://deep.test/", "https://top.test/"]);
        assert!(chromium_bookmarks("not json").is_empty());
    }

    #[test]
    fn only_web_pages_are_kept() {
        assert!(keep_web("https://a.test/"));
        assert!(keep_web("HTTP://a.test/"));
        assert!(!keep_web("file:///etc/passwd"));
        assert!(!keep_web("javascript:alert(1)"));
        assert!(!keep_web("ftp://a.test/"));
    }

    #[test]
    fn firefox_profiles_come_from_the_ini_once_each() {
        let base = Path::new("/h/.mozilla/firefox");
        let ini = "[General]\nStartWithLastProfile=1\n\n[Profile0]\nName=a\nIsRelative=1\nPath=abc.default\n\
                   [Profile1]\nIsRelative=0\nPath=/data/work\n[Install1]\nDefault=abc.default\n";
        let found = profile_places(base, ini);
        assert_eq!(
            found,
            vec![
                PathBuf::from("/h/.mozilla/firefox/abc.default/places.sqlite"),
                PathBuf::from("/data/work/places.sqlite"),
            ]
        );
    }

    #[test]
    fn firefox_and_chromium_times_become_unix_seconds() {
        assert_eq!(firefox_secs(1_700_000_000_123_456), 1_700_000_000);
        assert_eq!(chromium_secs((1_700_000_000 + 11_644_473_600) * 1_000_000), 1_700_000_000);
        assert_eq!(chromium_secs(0), 0);
    }

    #[test]
    fn merge_keeps_the_newest_visit_and_the_cap() {
        let mut kept = vec![history("https://a.test/", 50), history("https://b.test/", 10)];
        let incoming = vec![
            history("https://a.test/", 90),
            history("https://c.test/", 70),
            history("file:///x", 999),
        ];
        merge_history(&mut kept, incoming);
        let urls: Vec<(&str, u64)> = kept.iter().map(|e| (e.url.as_str(), e.at)).collect();
        assert_eq!(urls, vec![("https://a.test/", 90), ("https://c.test/", 70), ("https://b.test/", 10)]);

        let many: Vec<HistoryEntry> = (0..4500).map(|i| history(&format!("https://{i}.test/"), i)).collect();
        merge_history(&mut kept, many);
        assert_eq!(kept.len(), LIMIT);
        assert_eq!(kept[0].at, 4499);
    }

    #[test]
    fn bookmarks_are_not_duplicated() {
        let mut kept = vec![Bookmark { url: "https://a.test/".into(), title: "mine".into() }];
        merge_bookmarks(
            &mut kept,
            vec![
                Bookmark { url: "https://a.test/".into(), title: "theirs".into() },
                Bookmark { url: "https://b.test/".into(), title: "b".into() },
            ],
        );
        assert_eq!(kept.len(), 2);
        assert_eq!(kept[0].title, "mine");
    }

    #[test]
    fn firefox_database_is_read_through_its_wal() {
        let home = scratch("firefox");
        let profile = home.join(".mozilla/firefox/p1");
        fs::create_dir_all(&profile).unwrap();
        let db = profile.join("places.sqlite");
        // Left open so the changes stay in the -wal file, as a running browser leaves them.
        let conn = Connection::open(&db).unwrap();
        conn.pragma_update(None, "journal_mode", "WAL").unwrap();
        conn.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, fk INTEGER, type INTEGER, title TEXT);
             CREATE TABLE moz_historyvisits (id INTEGER PRIMARY KEY, place_id INTEGER, visit_date INTEGER);
             INSERT INTO moz_places VALUES (1, 'https://a.test/', 'A'), (2, 'about:config', ''), (3, 'https://b.test/', NULL);
             INSERT INTO moz_bookmarks VALUES (1, 1, 1, NULL), (2, 3, 1, 'B named'), (3, 2, 2, NULL);
             INSERT INTO moz_historyvisits VALUES (1, 1, 1700000000000000), (2, 3, 1700000100000000), (3, 2, 1700000200000000);",
        )
        .unwrap();
        let haul = read(&home, Browser::Firefox);
        let marks: Vec<(String, String)> = haul.bookmarks.iter().map(|b| (b.url.clone(), b.title.clone())).collect();
        assert_eq!(
            marks,
            vec![("https://a.test/".into(), "A".into()), ("https://b.test/".into(), "B named".into())]
        );
        let visits: Vec<(String, u64)> = haul.visits.iter().map(|v| (v.url.clone(), v.at)).collect();
        assert_eq!(visits, vec![("https://b.test/".into(), 1_700_000_100), ("https://a.test/".into(), 1_700_000_000)]);
        drop(conn);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn chromium_history_is_read_with_its_own_epoch() {
        let home = scratch("chromium");
        let dir = home.join(".config/BraveSoftware/Brave-Browser/Default");
        fs::create_dir_all(&dir).unwrap();
        let conn = Connection::open(dir.join("History")).unwrap();
        let t = (1_700_000_000i64 + 11_644_473_600) * 1_000_000;
        conn.execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, last_visit_time INTEGER);")
            .unwrap();
        conn.execute("INSERT INTO urls VALUES (1, 'https://c.test/', 'C', ?1)", [t]).unwrap();
        conn.execute("INSERT INTO urls VALUES (2, 'chrome://settings/', 'S', ?1)", [t]).unwrap();
        drop(conn);
        assert_eq!(installed(&home), vec![Browser::Brave]);
        let haul = read(&home, Browser::Brave);
        assert_eq!(haul.visits.len(), 1);
        assert_eq!(haul.visits[0].at, 1_700_000_000);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn a_browser_with_several_profiles_is_labelled_with_their_names() {
        let home = scratch("profile-labels");
        for dir in [".config/google-chrome/Default", ".config/google-chrome/Profile 1", ".config/BraveSoftware/Brave-Browser/Default"] {
            fs::create_dir_all(home.join(dir)).unwrap();
        }
        fs::write(
            home.join(".config/google-chrome/Local State"),
            r#"{"profile":{"info_cache":{"Default":{"name":"Home"},"Profile 1":{"name":"Work"}}}}"#,
        )
        .unwrap();
        let ff = home.join(".mozilla/firefox");
        for dir in ["a.default", "b.dev"] {
            fs::create_dir_all(ff.join(dir)).unwrap();
            fs::write(ff.join(dir).join("places.sqlite"), b"").unwrap();
        }
        fs::write(
            ff.join("profiles.ini"),
            "[Profile0]\nName=default\nIsRelative=1\nPath=a.default\n[Profile1]\nName=Dev\nIsRelative=1\nPath=b.dev\n",
        )
        .unwrap();
        assert_eq!(profile_names(&home, Browser::Chrome), vec!["Home", "Work"]);
        assert_eq!(label(&home, Browser::Chrome), "Chrome - Home, Work");
        assert_eq!(profile_names(&home, Browser::Firefox), vec!["default", "Dev"]);
        assert_eq!(label(&home, Browser::Firefox), "Firefox - default, Dev");
        // One profile, no Local State name: the row keeps the plain browser name.
        assert_eq!(profile_names(&home, Browser::Brave), vec!["Default"]);
        assert_eq!(label(&home, Browser::Brave), "Brave");
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn unused_firefox_profiles_in_ini_are_skipped_and_unlisted_ones_are_kept() {
        let home = scratch("firefox-unused-profiles");
        let ff = home.join(".mozilla/firefox");
        let used = ff.join("used.default");
        let unlisted = ff.join("extra.profile");
        let abs = home.join("external-profile");
        fs::create_dir_all(&used).unwrap();
        fs::create_dir_all(&unlisted).unwrap();
        fs::create_dir_all(&abs).unwrap();
        fs::write(used.join("places.sqlite"), b"").unwrap();
        fs::write(unlisted.join("places.sqlite"), b"").unwrap();
        fs::write(abs.join("places.sqlite"), b"").unwrap();
        // `ghost.default` is listed in profiles.ini without places.sqlite on disk.
        let ini = format!(
            "[General]\nStartWithLastProfile=1\n\
             [Profile0]\nName=Active\nIsRelative=1\nPath=used.default\n\
             [Profile1]\nName=Unused\nIsRelative=1\nPath=ghost.default\n\
             [Profile2]\nName=External\nIsRelative=0\nPath={}\n",
            abs.display()
        );
        fs::write(ff.join("profiles.ini"), ini).unwrap();
        assert_eq!(
            profile_names(&home, Browser::Firefox),
            vec!["Active", "External", "extra.profile"]
        );
        assert_eq!(
            label(&home, Browser::Firefox),
            "Firefox - Active, External, extra.profile"
        );
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn snap_and_flatpak_firefox_profiles_are_found() {
        let home = scratch("firefox-roots");
        for root in [
            "snap/firefox/common/.mozilla/firefox/s1.default",
            ".var/app/org.mozilla.firefox/.mozilla/firefox/f1.default",
            ".mozilla/firefox/n1.default",
        ] {
            fs::create_dir_all(home.join(root)).unwrap();
            fs::write(home.join(root).join("places.sqlite"), b"").unwrap();
        }
        assert_eq!(installed(&home), vec![Browser::Firefox]);
        assert_eq!(firefox_places(&home).len(), 3);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn every_chromium_profile_is_read_and_flatpak_or_snap_roots_count() {
        let home = scratch("chromium-profiles");
        let profiles = [
            ".config/google-chrome/Default",
            ".config/google-chrome/Profile 1",
            ".config/google-chrome/Profile 12",
            ".var/app/com.google.Chrome/config/google-chrome/Default",
            ".config/chromium/Guest Profile",
            ".config/chromium/Profile x",
            "snap/chromium/common/chromium/Default",
            ".var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser/Profile 2",
            ".config/microsoft-edge/Default",
            ".var/app/com.vivaldi.Vivaldi/config/vivaldi/Default",
        ];
        for dir in profiles {
            let dir = home.join(dir);
            fs::create_dir_all(&dir).unwrap();
            let conn = Connection::open(dir.join("History")).unwrap();
            let t = (1_700_000_000i64 + 11_644_473_600) * 1_000_000;
            conn.execute_batch("CREATE TABLE urls (id INTEGER PRIMARY KEY, url TEXT, title TEXT, last_visit_time INTEGER);")
                .unwrap();
            conn.execute("INSERT INTO urls VALUES (1, 'https://c.test/', 'C', ?1)", [t]).unwrap();
        }
        assert_eq!(
            installed(&home),
            vec![Browser::Chromium, Browser::Chrome, Browser::Brave, Browser::Edge, Browser::Vivaldi]
        );
        // Every profile holds the same URL, so the merge keeps one visit; the count only shows the read ran.
        assert_eq!(read(&home, Browser::Chrome).visits.len(), 1);
        assert_eq!(chromium_profiles(&home.join(".config/google-chrome")).len(), 3);
        assert_eq!(chromium_profiles(&home.join(".config/chromium")).len(), 0);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn an_empty_home_changes_nothing() {
        let home = scratch("empty");
        assert!(installed(&home).is_empty());
        let mut marks = vec![Bookmark { url: "https://a.test/".into(), title: "A".into() }];
        let mut hist = vec![history("https://a.test/", 5)];
        assert_eq!(apply(&home, Browser::Chromium, &mut marks, &mut hist), None);
        assert_eq!(marks.len(), 1);
        assert_eq!(hist.len(), 1);
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn landing_the_same_haul_twice_changes_nothing() {
        let mut bookmarks = Vec::new();
        let mut visits = Vec::new();
        let haul = || {
            (
                vec![Bookmark { url: "https://a.example/".into(), title: "A".into() }],
                vec![history("https://a.example/", 5), history("https://b.example/", 6)],
            )
        };
        let (b, h) = haul();
        merge_bookmarks(&mut bookmarks, b);
        merge_history(&mut visits, h);
        let (b, h) = haul();
        merge_bookmarks(&mut bookmarks, b);
        merge_history(&mut visits, h);
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(visits.len(), 2);
    }

    #[test]
    fn counts_are_grouped() {
        assert_eq!(grouped(0), "0");
        assert_eq!(grouped(999), "999");
        assert_eq!(grouped(2000), "2,000");
        assert_eq!(grouped(1234567), "1,234,567");
    }
}
