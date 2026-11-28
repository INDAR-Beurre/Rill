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
pub const MIN_DEFAULT_ZOOM: f64 = 0.5;
pub const MAX_DEFAULT_ZOOM: f64 = 2.0;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NewTabPlacement {
    #[default]
    Adjacent,
    End,
}

impl NewTabPlacement {
    pub fn title(self) -> &'static str {
        match self {
            NewTabPlacement::Adjacent => "Next to active tab",
            NewTabPlacement::End => "At end of space",
        }
    }

    pub fn all() -> [NewTabPlacement; 2] {
        [NewTabPlacement::Adjacent, NewTabPlacement::End]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum StartupMode {
    #[default]
    Restore,
    Blank,
    Home,
}

impl StartupMode {
    pub fn title(self) -> &'static str {
        match self {
            StartupMode::Restore => "Restore previous tabs",
            StartupMode::Blank => "Open a blank Start Page",
            StartupMode::Home => "Open custom Home page",
        }
    }

    pub fn all() -> [StartupMode; 3] {
        [StartupMode::Restore, StartupMode::Blank, StartupMode::Home]
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum CloseTabFocus {
    #[default]
    Right,
    Left,
    Recent,
}

impl CloseTabFocus {
    pub fn title(self) -> &'static str {
        match self {
            CloseTabFocus::Right => "Next tab",
            CloseTabFocus::Left => "Previous tab",
            CloseTabFocus::Recent => "Most recently viewed",
        }
    }

    pub fn all() -> [CloseTabFocus; 3] {
        [CloseTabFocus::Right, CloseTabFocus::Left, CloseTabFocus::Recent]
    }
}

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
    /// T38: Custom Home page URL (`http://` or `https://`), or empty for built-in Start Page.
    #[serde(default)]
    pub home_url: String,
    /// T39: Default page zoom level (0.5..=2.0), used when a site has no explicit override.
    #[serde(default = "default_zoom_level")]
    pub default_zoom: f64,
    /// T40: Where new tabs open within the current space (adjacent to active or at end).
    #[serde(default)]
    pub new_tab_placement: NewTabPlacement,
    /// T41: Startup behavior: restore previous tabs, blank Start Page, or custom Home page.
    #[serde(default)]
    pub startup_mode: StartupMode,
    /// T42: Which tab activates after closing the active tab.
    #[serde(default)]
    pub close_tab_focus: CloseTabFocus,
    /// T43: Warn before bulk-closing multiple tabs.
    #[serde(default = "default_true")]
    pub warn_bulk_close: bool,
    /// T44: Protect pinned tabs from ordinary close actions until unpinned.
    #[serde(default)]
    pub protect_pinned_tabs: bool,
    /// T13: Exact host rules kept awake from automatic and bulk tab sleeping.
    #[serde(default)]
    pub keep_awake_hosts: Vec<String>,
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

fn default_zoom_level() -> f64 {
    1.0
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
            home_url: String::new(),
            default_zoom: 1.0,
            new_tab_placement: NewTabPlacement::Adjacent,
            startup_mode: StartupMode::Restore,
            close_tab_focus: CloseTabFocus::Right,
            warn_bulk_close: true,
            protect_pinned_tabs: false,
            keep_awake_hosts: Vec::new(),
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
        let has_startup_mode = value.get("startup_mode").is_some();
        let mut prefs: Prefs = serde_json::from_value(value).ok()?;
        if !has_back {
            prefs.show_back_button = if has_nav { prefs.show_nav_buttons } else { true };
        }
        if !has_forward {
            prefs.show_forward_button = if has_nav { prefs.show_nav_buttons } else { true };
        }
        if !has_startup_mode {
            prefs.startup_mode = if prefs.restore_session {
                StartupMode::Restore
            } else {
                StartupMode::Blank
            };
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
        if !self.default_zoom.is_finite() {
            self.default_zoom = 1.0;
        }
        self.default_zoom = self.default_zoom.clamp(MIN_DEFAULT_ZOOM, MAX_DEFAULT_ZOOM);
        self.home_url = crate::tab_actions::validate_home_url(&self.home_url).unwrap_or_default();
        self.keep_awake_hosts = crate::tab_actions::sanitize_keep_awake_hosts(self.keep_awake_hosts);
        self.restore_session = self.startup_mode == StartupMode::Restore;
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
    /// T08: a muted tab keeps its choice across sleep, session save and restart.
    #[serde(default)]
    pub muted: bool,
    /// T12: a tab kept awake keeps that choice across session save and restart.
    #[serde(default)]
    pub keep_awake: bool,
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
    /// L13: a plain label, never a path or an access-control boundary. Old
    /// bookmarks without the field stay in the flat Default folder.
    #[serde(default)]
    pub folder: String,
}

impl Bookmark {
    pub fn folder(&self) -> &str {
        if self.folder.trim().is_empty() {
            Bookmark::DEFAULT_FOLDER
        } else {
            self.folder.as_str()
        }
    }
}

impl Bookmark {
    pub const DEFAULT_FOLDER: &'static str = "Bookmarks";
}

pub const MAX_BOOKMARK_FOLDER_LEN: usize = 48;
pub const MAX_BOOKMARK_TITLE_LEN: usize = 200;
pub const MAX_BOOKMARKS: usize = 2_000;

/// L13/L15: normalise a folder label without inventing a hierarchy. A slash
/// is kept as text, not interpreted as a path separator.
pub fn sanitize_bookmark_folder(raw: &str) -> String {
    let mut text: String = raw
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_BOOKMARK_FOLDER_LEN)
        .collect();
    if text.trim().is_empty() {
        text = Bookmark::DEFAULT_FOLDER.to_string();
    }
    text
}

pub fn bookmark_site(url: &str) -> String {
    crate::shield::host_of(url)
        .trim_start_matches("www.")
        .to_ascii_lowercase()
}

pub fn bookmark_sort_key(bookmark: &Bookmark, by_site: bool) -> String {
    if by_site {
        let site = bookmark_site(&bookmark.url);
        format!("{site}\u{1f}{}", bookmark.title.trim().to_lowercase())
    } else {
        bookmark.title.trim().to_lowercase()
    }
}

pub fn rename_bookmark(
    bookmarks: &mut [Bookmark],
    url: &str,
    raw_title: &str,
) -> Result<(), &'static str> {
    let title: String = raw_title
        .trim()
        .chars()
        .filter(|ch| !ch.is_control())
        .take(MAX_BOOKMARK_TITLE_LEN)
        .collect();
    if title.trim().is_empty() {
        return Err("Enter a title for this bookmark");
    }
    let bookmark = bookmarks
        .iter_mut()
        .find(|bookmark| bookmark.url == url)
        .ok_or("That bookmark no longer exists")?;
    bookmark.title = title;
    Ok(())
}

pub fn update_bookmark_address(
    bookmarks: &mut Vec<Bookmark>,
    old_url: &str,
    raw_url: &str,
) -> Result<(), &'static str> {
    let parsed = crate::tab_actions::validate_home_url(raw_url)?;
    if parsed.is_empty() {
        return Err("Enter a full http:// or https:// address");
    }
    let position = bookmarks
        .iter()
        .position(|bookmark| bookmark.url == old_url)
        .ok_or("That bookmark no longer exists")?;
    let url = crate::tab_actions::duplicate_url_key(&parsed);
    bookmarks[position].url = url.clone();
    let mut index = 0;
    while index < bookmarks.len() {
        if index != position
            && crate::tab_actions::duplicate_url_key(&bookmarks[index].url) == url
        {
            bookmarks.remove(index);
            if index < position {
                let edited = bookmarks.remove(position - 1);
                bookmarks.insert(position.min(bookmarks.len()), edited);
            }
            break;
        }
        index += 1;
    }
    Ok(())
}

pub fn set_bookmark_folder(bookmarks: &mut [Bookmark], url: &str, folder: &str) -> bool {
    let Some(bookmark) = bookmarks.iter_mut().find(|bookmark| bookmark.url == url) else {
        return false;
    };
    bookmark.folder = sanitize_bookmark_folder(folder);
    true
}

pub fn move_bookmark(bookmarks: &mut Vec<Bookmark>, url: &str, delta: isize) -> bool {
    let Some(from) = bookmarks.iter().position(|bookmark| bookmark.url == url) else {
        return false;
    };
    let to = (from as isize)
        .saturating_add(delta)
        .clamp(0, bookmarks.len().saturating_sub(1) as isize) as usize;
    if to == from {
        return false;
    }
    let bookmark = bookmarks.remove(from);
    bookmarks.insert(to, bookmark);
    true
}

pub fn bookmark_matches_query(bookmark: &Bookmark, query: &str) -> bool {
    let query = query.trim();
    query.is_empty()
        || format!("{} {} {}", bookmark.title, bookmark.url, bookmark.folder())
            .to_lowercase()
            .contains(&query.to_lowercase())
}

/// L07–L11: validate and sanitize a bookmark address before exporting, opening
/// in a background/private tab, or placing it on the clipboard. Only explicit
/// `http://` and `https://` addresses are accepted, embedded credentials are
/// stripped, and oversized or control-bearing inputs are rejected.
pub fn safe_bookmark_url(raw_url: &str) -> Option<String> {
    let parsed = crate::tab_actions::parse_explicit_web_url(raw_url)?;
    let clean = crate::tab_actions::strip_url_credentials(&parsed);
    if clean.len() > crate::tab_actions::MAX_URL_LEN || !crate::tab_actions::is_web_url(&clean) {
        return None;
    }
    Some(clean)
}

/// HTML-escape bookmarks exported by L07. Quotes are escaped in both text and
/// attributes so a title cannot break out of either context.
pub fn html_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

/// Decode the five HTML entities used by Netscape bookmark files. Bounded by
/// the caller's file limit; numeric entities outside Unicode are left alone.
pub fn html_unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '&' {
            out.push(ch);
            continue;
        }
        let mut entity = String::new();
        let mut terminated = false;
        while let Some(next) = chars.peek() {
            if *next == ';' {
                chars.next();
                terminated = true;
                break;
            }
            if entity.len() >= 16 || next.is_whitespace() || *next == '<' || *next == '&' {
                break;
            }
            entity.push(*next);
            chars.next();
        }
        let decoded = if terminated {
            match entity.as_str() {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" | "#39" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                "colon" => Some(':'),
                _ => {
                    let number = entity
                        .strip_prefix("#x")
                        .or_else(|| entity.strip_prefix("#X"))
                        .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                        .or_else(|| entity.strip_prefix('#').and_then(|decimal| decimal.parse().ok()));
                    number.and_then(char::from_u32)
                }
            }
        } else {
            None
        };
        if let Some(decoded) = decoded {
            out.push(decoded);
        } else {
            out.push('&');
            out.push_str(&entity);
            if terminated {
                out.push(';');
            }
        }
    }
    out
}

/// Export the flat folder list in the Netscape format browsers understand.
pub fn bookmarks_html(bookmarks: &[Bookmark]) -> String {
    let mut out = String::from(
        "<!DOCTYPE NETSCAPE-Bookmark-file-1>\n<META HTTP-EQUIV=\"Content-Type\" CONTENT=\"text/html; charset=UTF-8\">\n<TITLE>Bookmarks</TITLE>\n<H1>Bookmarks</H1>\n<DL><p>\n",
    );
    let mut folders: Vec<&str> = bookmarks
        .iter()
        .filter(|bookmark| safe_bookmark_url(&bookmark.url).is_some())
        .map(|bookmark| bookmark.folder())
        .collect();
    folders.sort();
    folders.dedup();
    for folder in folders {
        out.push_str(&format!(
            "    <DT><H3>{}</H3>\n    <DL><p>\n",
            html_escape(folder)
        ));
        for (bookmark, safe_url) in bookmarks.iter().filter_map(|bookmark| {
            (bookmark.folder() == folder)
                .then(|| safe_bookmark_url(&bookmark.url).map(|url| (bookmark, url)))
                .flatten()
        }) {
            out.push_str(&format!(
                "        <DT><A HREF=\"{}\" ADD_DATE=\"0\">{}</A>\n",
                html_escape(&safe_url),
                html_escape(&bookmark.title)
            ));
        }
        out.push_str("    </DL><p>\n");
    }
    out.push_str("</DL><p>\n");
    out
}

/// Bounded, lenient Netscape bookmark parser for L08. Tags are scanned as
/// text: this never builds a DOM, follows no links, and executes no script.
pub fn parse_bookmarks_html(raw: &str) -> (Vec<Bookmark>, usize) {
    const MAX_HTML: usize = 4 * 1024 * 1024;
    const MAX_URL: usize = crate::tab_actions::MAX_URL_LEN;
    let mut end = raw.len().min(MAX_HTML);
    while !raw.is_char_boundary(end) {
        end -= 1;
    }
    let raw = &raw[..end];
    let lower = raw.to_ascii_lowercase();
    let mut out: Vec<Bookmark> = Vec::new();
    let mut skipped = 0usize;
    let mut folder_stack = Vec::new();
    let mut pending_folder: Option<String> = None;
    let mut pos = 0usize;
    while pos < lower.len() {
        let Some(open) = lower[pos..].find('<') else { break };
        let open = pos + open;
        let bytes = lower.as_bytes();
        let mut quote = None;
        let mut close = None;
        for (index, byte) in bytes.iter().enumerate().skip(open + 1) {
            match (*byte, quote) {
                (b'"' | b'\'', None) => quote = Some(*byte),
                (byte, Some(active)) if byte == active => quote = None,
                (b'>', None) => {
                    close = Some(index + 1);
                    break;
                }
                _ => {}
            }
        }
        let Some(tag_end) = close else { break };
        let content = &lower[open + 1..tag_end - 1];
        let original = &raw[open + 1..tag_end - 1];
        if content.starts_with("!--") {
            pos = lower[tag_end..]
                .find("-->")
                .map(|offset| tag_end + offset + 3)
                .unwrap_or(lower.len());
            continue;
        }
        let (closing, name, attributes) = html_tag_parts(content);
        if closing && name == "dl" {
            folder_stack.pop();
            pending_folder = None;
        } else if !closing && name == "dl" {
            let folder = pending_folder.take().unwrap_or_else(|| {
                folder_stack
                    .last()
                    .cloned()
                    .unwrap_or_else(|| Bookmark::DEFAULT_FOLDER.to_string())
            });
            folder_stack.push(folder);
        } else if !closing && name == "script" {
            pos = lower[tag_end..]
                .find("</script")
                .and_then(|offset| {
                    let close_start = tag_end + offset;
                    lower[close_start..]
                        .find('>')
                        .map(|end_offset| close_start + end_offset + 1)
                })
                .unwrap_or(lower.len());
            continue;
        } else if !closing && name == "h3" {
            let (text, after) = html_element_text(raw, &lower, tag_end, "h3");
            pending_folder = Some(sanitize_bookmark_folder(&text));
            pos = after;
            continue;
        } else if !closing && name == "a" {
            let attr_offset = content.len() - attributes.len();
            let raw_url = html_attribute(&original[attr_offset..], attributes, "href");
            let url = raw_url.map(html_unescape).map(|value| value.trim().to_string());
            let (title, after) = html_element_text(raw, &lower, tag_end, "a");
            if let Some(url) = url {
                if url.len() <= MAX_URL
                    && crate::tab_actions::parse_explicit_web_url(&url).is_some()
                    && out.len() < MAX_BOOKMARKS
                {
                    let folder = pending_folder
                        .as_deref()
                        .or_else(|| folder_stack.last().map(String::as_str))
                        .unwrap_or(Bookmark::DEFAULT_FOLDER);
                    out.push(Bookmark {
                        url: crate::tab_actions::strip_url_credentials(&url),
                        title: if title.is_empty() {
                            crate::shield::host_of(&url)
                        } else {
                            title
                        },
                        folder: sanitize_bookmark_folder(folder),
                    });
                } else {
                    skipped += 1;
                }
            }
            pos = after;
            continue;
        }
        pos = tag_end;
    }
    (out, skipped)
}

fn html_tag_parts(content: &str) -> (bool, &str, &str) {
    let content = content.trim_start();
    let (closing, content) = if let Some(content) = content.strip_prefix('/') {
        (true, content.trim_start())
    } else {
        (false, content)
    };
    let name_end = content
        .find(|ch: char| ch.is_ascii_whitespace() || ch == '/')
        .unwrap_or(content.len());
    let name = &content[..name_end];
    let attrs = content[name_end..].trim_start();
    (closing, name, attrs)
}

fn html_attribute<'a>(original: &'a str, attributes_lower: &str, wanted: &str) -> Option<&'a str> {
    let lower_bytes = attributes_lower.as_bytes();
    let mut index = 0;
    while index < lower_bytes.len() {
        while index < lower_bytes.len()
            && (lower_bytes[index].is_ascii_whitespace() || lower_bytes[index] == b'/')
        {
            index += 1;
        }
        let start = index;
        while index < lower_bytes.len()
            && !lower_bytes[index].is_ascii_whitespace()
            && lower_bytes[index] != b'='
            && lower_bytes[index] != b'/'
        {
            index += 1;
        }
        if start == index {
            break;
        }
        let name = &attributes_lower[start..index];
        while index < lower_bytes.len() && lower_bytes[index].is_ascii_whitespace() {
            index += 1;
        }
        let mut value = None;
        if index < lower_bytes.len() && lower_bytes[index] == b'=' {
            index += 1;
            while index < lower_bytes.len() && lower_bytes[index].is_ascii_whitespace() {
                index += 1;
            }
            let start_value;
            if index < lower_bytes.len() && matches!(lower_bytes[index], b'"' | b'\'') {
                let quote = lower_bytes[index];
                index += 1;
                start_value = index;
                while index < lower_bytes.len() && lower_bytes[index] != quote {
                    index += 1;
                }
                value = original.get(start_value..index);
                if index < lower_bytes.len() {
                    index += 1;
                }
            } else {
                start_value = index;
                while index < lower_bytes.len() && !lower_bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                value = original.get(start_value..index);
            }
        }
        if name.eq_ignore_ascii_case(wanted) {
            return value;
        }
    }
    None
}

fn html_element_text(raw: &str, lower: &str, start: usize, name: &str) -> (String, usize) {
    let close_marker = format!("</{name}");
    let Some(close_start_offset) = lower[start..].find(&close_marker) else {
        return (String::new(), start);
    };
    let close_start = start + close_start_offset;
    let text = &raw[start..close_start];
    let mut plain = String::with_capacity(text.len());
    let mut cursor = 0;
    while cursor < text.len() {
        let Some(offset) = text[cursor..].find('<') else {
            plain.push_str(&text[cursor..]);
            break;
        };
        let tag_start = cursor + offset;
        plain.push_str(&text[cursor..tag_start]);
        let Some(tag_offset) = text[tag_start..].find('>') else { break };
        cursor = tag_start + tag_offset + 1;
    }
    let decoded = html_unescape(&plain);
    let normalized = decoded.split_whitespace().collect::<Vec<_>>().join(" ");
    let bounded = normalized.chars().take(MAX_BOOKMARK_TITLE_LEN).collect();
    let after = lower[close_start..]
        .find('>')
        .map(|offset| close_start + offset + 1)
        .unwrap_or(lower.len());
    (bounded, after)
}

/// L21: only the selected, regular history rows reach this payload. Private
/// visits never enter `history` in the first place; this is a second guard.
pub fn history_csv(entries: &[HistoryEntry]) -> String {
    let mut out = String::from("title,address,visited_at_unix\n");
    for entry in entries {
        let safe_url = crate::tab_actions::strip_url_credentials(&entry.url);
        if !crate::tab_actions::is_web_url(&safe_url) {
            continue;
        }
        let title = entry.title.replace(['\r', '\n'], " ");
        out.push_str(&format!(
            "\"{}\",\"{}\",{}\n",
            title.replace('"', "\"\""),
            safe_url.replace('"', "\"\""),
            entry.at
        ));
    }
    out
}

/// L16-L20: local-day boundaries in seconds since the Unix epoch. `now` is
/// taken from the app clock so tests can pin a timezone-independent instant.
pub fn local_day_start(now: u64) -> u64 {
    let Some(local) = gtk::glib::DateTime::from_unix_local(now as i64).ok() else {
        return now.saturating_sub(86_400);
    };
    // Search backwards in six-hour steps, then forward by seconds. Local
    // days are 23 or 25 hours across DST, so a fixed 86,400 subtraction is
    // wrong twice a year in most of Europe and North America.
    let mut boundary = now.saturating_sub(local.hour() as u64 * 3600)
        .saturating_sub(local.minute() as u64 * 60)
        .saturating_sub(local.seconds() as u64);
    while let Some(next) = gtk::glib::DateTime::from_unix_local(boundary as i64).ok() {
        if next.hour() == 0 && next.minute() == 0 && next.seconds() == 0.0 {
            return boundary;
        }
        if next.hour() > 12 {
            boundary = boundary.saturating_add(3600);
        } else {
            boundary = boundary.saturating_sub(1800);
        }
        if now.saturating_sub(boundary) > 100_000 {
            break;
        }
    }
    now.saturating_sub(86_400)
}

pub fn site_key(url: &str) -> String {
    crate::shield::host_of(url).to_ascii_lowercase()
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

    pub fn load_named_sessions() -> crate::tab_actions::NamedSessionStore {
        files::read_json::<crate::tab_actions::NamedSessionStore>(&Self::file("named_sessions.json"))
            .map(|s| s.sanitised())
            .unwrap_or_default()
    }

    pub fn save_named_sessions(store: &crate::tab_actions::NamedSessionStore) -> bool {
        files::write_json(&Self::file("named_sessions.json"), store)
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
            home_url: "https://example.org/home".into(),
            default_zoom: 1.25,
            new_tab_placement: NewTabPlacement::End,
            startup_mode: StartupMode::Blank,
            close_tab_focus: CloseTabFocus::Recent,
            warn_bulk_close: false,
            protect_pinned_tabs: true,
            keep_awake_hosts: vec!["docs.rs".into()],
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

    #[test]
    fn legacy_restore_session_migrates_to_startup_mode_and_clamps_new_prefs() {
        let legacy_off = Prefs::from_json_str(r#"{"restore_session":false}"#).expect("parses");
        assert_eq!(legacy_off.startup_mode, StartupMode::Blank);
        assert!(!legacy_off.restore_session);

        let legacy_on = Prefs::from_json_str(r#"{"restore_session":true}"#).expect("parses");
        assert_eq!(legacy_on.startup_mode, StartupMode::Restore);
        assert!(legacy_on.restore_session);

        let clamped = Prefs::from_json_str(
            r#"{"home_url":"javascript:alert(1)","default_zoom":9.5,"startup_mode":"home","keep_awake_hosts":["DOCS.RS","*.bad.test","docs.rs"]}"#,
        )
        .expect("parses");
        assert_eq!(clamped.home_url, "");
        assert_eq!(clamped.default_zoom, MAX_DEFAULT_ZOOM);
        assert_eq!(clamped.startup_mode, StartupMode::Home);
        assert!(!clamped.restore_session, "legacy field is migration input only");
        assert_eq!(clamped.keep_awake_hosts, vec!["docs.rs"]);
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

#[cfg(test)]
mod bookmark_batch1a_tests {
    use super::*;

    fn bm(url: &str, title: &str) -> Bookmark {
        Bookmark {
            url: url.to_string(),
            title: title.to_string(),
            folder: String::new(),
        }
    }

    #[test]
    fn l01_rename_bookmark_updates_title_and_rejects_empty_or_missing() {
        let mut list = vec![bm("https://example.org/a", "Old")];
        rename_bookmark(&mut list, "https://example.org/a", "  New\nTitle\t ").expect("rename ok");
        assert_eq!(list[0].title, "NewTitle");
        assert!(rename_bookmark(&mut list, "https://example.org/a", "   \n\t").is_err());
        assert!(rename_bookmark(&mut list, "https://missing.example/", "Name").is_err());
        let long = "x".repeat(MAX_BOOKMARK_TITLE_LEN + 40);
        rename_bookmark(&mut list, "https://example.org/a", &long).expect("bounded");
        assert_eq!(list[0].title.chars().count(), MAX_BOOKMARK_TITLE_LEN);
    }

    #[test]
    fn l02_update_bookmark_address_validates_scheme_strips_creds_and_dedupes() {
        let mut list = vec![
            bm("https://a.example/", "Alpha"),
            bm("https://b.example/", "Beta"),
            bm("https://keep.example/", "Keep"),
        ];
        assert!(update_bookmark_address(&mut list, "https://a.example/", "javascript:alert(1)").is_err());
        assert!(update_bookmark_address(&mut list, "https://a.example/", "file:///etc/passwd").is_err());
        assert!(update_bookmark_address(&mut list, "https://a.example/", "data:text/html,hi").is_err());
        assert!(update_bookmark_address(&mut list, "https://a.example/", "about:blank").is_err());
        assert!(update_bookmark_address(&mut list, "https://a.example/", "blob:https://a.example/1").is_err());
        assert!(update_bookmark_address(&mut list, "https://a.example/", "   ").is_err());
        assert!(update_bookmark_address(&mut list, "https://missing.example/", "https://valid.example/").is_err());

        update_bookmark_address(
            &mut list,
            "https://a.example/",
            "https://user:secret@C.EXAMPLE:443/path",
        )
        .expect("valid https");
        assert_eq!(list[0].url, "https://c.example/path");
        assert_eq!(list[0].title, "Alpha");
        assert!(!list[0].url.contains("user:"));
        assert!(!list[0].url.contains("secret"));

        update_bookmark_address(&mut list, "https://c.example/path", "https://B.EXAMPLE:443/")
            .expect("merges duplicate");
        assert_eq!(list.len(), 2, "the edited row survives and the duplicate is removed");
        assert_eq!(list[0].url, "https://b.example/", "edited URL stays at its original position");
        assert_eq!(list[0].title, "Alpha", "edited title is preserved");
        assert_eq!(list[1].url, "https://keep.example/", "unrelated rows keep their order");

        let mut reverse = vec![
            bm("https://first.example/", "First"),
            bm("https://mid.example/", "Middle"),
            bm("https://last.example/", "Last"),
        ];
        update_bookmark_address(
            &mut reverse,
            "https://last.example/",
            "https://user:pass@FIRST.EXAMPLE:443/#",
        )
        .expect("merges duplicate earlier in list");
        assert_eq!(reverse.len(), 2);
        assert_eq!(
            reverse.iter().map(|b| (b.url.as_str(), b.title.as_str())).collect::<Vec<_>>(),
            vec![
                ("https://mid.example/", "Middle"),
                ("https://first.example/", "Last"),
            ],
            "edited row survives at its trailing position when duplicate was earlier"
        );
    }

    #[test]
    fn l03_l04_move_bookmark_up_and_down_clamps_boundaries() {
        let mut list = vec![
            bm("https://one.example/", "One"),
            bm("https://two.example/", "Two"),
            bm("https://three.example/", "Three"),
        ];
        assert!(!move_bookmark(&mut list, "https://one.example/", -1));
        assert!(!move_bookmark(&mut list, "https://three.example/", 1));
        assert!(move_bookmark(&mut list, "https://two.example/", -1));
        assert_eq!(list[0].url, "https://two.example/");
        assert_eq!(list[1].url, "https://one.example/");
        assert!(move_bookmark(&mut list, "https://two.example/", 1));
        assert_eq!(list[0].url, "https://one.example/");
        assert_eq!(list[1].url, "https://two.example/");
        assert!(!move_bookmark(&mut list, "https://missing.example/", 1));
    }

    #[test]
    fn l05_l06_sort_by_title_and_site_are_stable_and_case_insensitive() {
        let mut list = vec![
            bm("https://www.zeta.example/2", "beta"),
            bm("https://alpha.example/1", "Beta"),
            bm("https://beta.example/3", "alpha"),
        ];
        list.sort_by_key(|b| bookmark_sort_key(b, false));
        assert_eq!(
            list.iter().map(|b| b.url.as_str()).collect::<Vec<_>>(),
            vec![
                "https://beta.example/3",
                "https://www.zeta.example/2",
                "https://alpha.example/1",
            ],
            "L05: case-insensitive sort preserves relative order of equal keys"
        );

        let mut by_site = vec![
            bm("https://www.docs.example/z", "Same"),
            bm("https://docs.example/a", "Same"),
            bm("https://aardvark.example/1", "First"),
        ];
        by_site.sort_by_key(|b| bookmark_sort_key(b, true));
        assert_eq!(
            by_site.iter().map(|b| b.url.as_str()).collect::<Vec<_>>(),
            vec![
                "https://aardvark.example/1",
                "https://www.docs.example/z",
                "https://docs.example/a",
            ],
            "L06: host extraction strips www., sorts by host then title, and stays stable"
        );
    }

    #[test]
    fn l07_l08_html_export_escapes_strips_creds_and_roundtrips_safely() {
        let exported = bookmarks_html(&[
            bm(
                "https://user:pass@example.org/q?a=1&b=2",
                "Tom & Jerry <\"Quotes\"> 'Single' Café",
            ),
            bm("javascript:alert(1)", "Evil"),
        ]);
        assert!(exported.contains("<!DOCTYPE NETSCAPE-Bookmark-file-1>"));
        assert!(exported.contains("https://example.org/q?a=1&amp;b=2"));
        assert!(!exported.contains("user:pass"));
        assert!(!exported.contains("javascript:"));
        assert!(exported.contains("Tom &amp; Jerry &lt;&quot;Quotes&quot;&gt; &#39;Single&#39; Café"));
        assert!(!exported.contains(".ics"));

        let (parsed, skipped) = parse_bookmarks_html(&exported);
        assert_eq!(skipped, 0);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].url, "https://example.org/q?a=1&b=2");
        assert_eq!(parsed[0].title, "Tom & Jerry <\"Quotes\"> 'Single' Café");
    }

    #[test]
    fn l08_html_import_rejects_unsafe_schemes_scripts_and_bounds_input() {
        let raw = r#"<!DOCTYPE NETSCAPE-Bookmark-file-1>
        <DL><p>
            <!-- <A HREF="https://commented.example/">Ignored</A> -->
            <SCRIPT>var s = '<A HREF="https://scripted.example/">No</A>';</SCRIPT>
            <DT><A HREF="javascript:alert(1)">Bad JS</A>
            <DT><A HREF="data:text/html,hi">Bad Data</A>
            <DT><A HREF="file:///etc/passwd">Bad File</A>
            <DT><A HREF="https://alice:secret@good.example/a?x=1&amp;y=2">Good &amp; Safe</A>
            <DT><A HREF='http://plain.example/page'></A>
        </DL><p>"#;
        let (parsed, skipped) = parse_bookmarks_html(raw);
        assert_eq!(skipped, 3);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].url, "https://good.example/a?x=1&y=2");
        assert_eq!(parsed[0].title, "Good & Safe");
        assert_eq!(parsed[1].url, "http://plain.example/page");
        assert_eq!(parsed[1].title, "plain.example");
    }

    #[test]
    fn l08_html_import_respects_max_bookmarks_and_utf8_boundary() {
        let mut huge = String::from("<DL>");
        for i in 0..(MAX_BOOKMARKS + 10) {
            huge.push_str(&format!("<DT><A HREF=\"https://s{i}.example/\">Site {i}</A>\n"));
        }
        huge.push_str("</DL>");
        let (parsed, skipped) = parse_bookmarks_html(&huge);
        assert_eq!(parsed.len(), MAX_BOOKMARKS);
        assert_eq!(skipped, 10);

        let mut over_4mb = "é".repeat(2 * 1024 * 1024 + 50);
        over_4mb.push_str("<A HREF=\"https://tail.example/\">Tail</A>");
        let (parsed_tail, _) = parse_bookmarks_html(&over_4mb);
        assert!(parsed_tail.is_empty());
    }

    #[test]
    fn l07_atomic_export_writes_and_replaces_without_temp_leftovers() {
        let dir = std::env::temp_dir().join(format!(
            "rill-l07-export-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        let target = dir.join("bookmarks.html");
        let html = bookmarks_html(&[bm("https://example.org/", "Example")]);
        crate::files::write_atomic_export(&target, html.as_bytes()).expect("atomic export");
        assert_eq!(std::fs::read_to_string(&target).expect("read"), html);
        let tmps: Vec<_> = std::fs::read_dir(&dir)
            .expect("readdir")
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(tmps.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod bookmark_batch1b_tests {
    use super::*;
    use crate::app::{DownloadRec, Tab, TAB_LIMIT};

    fn bm(url: &str, title: &str) -> Bookmark {
        Bookmark {
            url: url.to_string(),
            title: title.to_string(),
            folder: String::new(),
        }
    }

    #[test]
    fn l11_safe_bookmark_url_strips_credentials_and_rejects_unsafe_schemes() {
        let dirty = "  https://alice:secret@docs.example.org/guide?tab=1#top  ";
        assert_eq!(
            safe_bookmark_url(dirty).as_deref(),
            Some("https://docs.example.org/guide?tab=1#top")
        );

        let http = "http://user@plain.example:8080/path";
        assert_eq!(
            safe_bookmark_url(http).as_deref(),
            Some("http://plain.example:8080/path")
        );

        for unsafe_url in [
            "",
            "   ",
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "about:blank",
            "blob:https://example.org/1234",
            "ftp://files.example.org/pub",
            "https://example.org/bad\nline",
            "https://example.org/bad\r\nheader",
            "https://example.org/with spaces",
        ] {
            assert_eq!(
                safe_bookmark_url(unsafe_url),
                None,
                "L11 must reject unsafe or malformed URL {unsafe_url:?}"
            );
        }

        let huge = format!("https://example.org/{}", "a".repeat(crate::tab_actions::MAX_URL_LEN));
        assert_eq!(
            safe_bookmark_url(&huge),
            None,
            "L11 must reject oversized URLs"
        );
    }

    #[test]
    fn l09_background_open_insertion_preserves_active_tab_and_respects_space_and_limit() {
        let mut active = Tab::new(1);
        active.url = "https://active.example/".to_string();
        let mut other = Tab::new(2);
        other.url = "https://other.example/".to_string();
        let tabs = vec![active, other];
        let space = crate::spaces::default_space_id();

        let adjacent = crate::tab_actions::new_tab_insert_index(
            &tabs,
            1,
            &space,
            NewTabPlacement::Adjacent,
        );
        assert_eq!(adjacent, 1, "L09 background tab opens adjacent to active without replacing it");

        let at_end = crate::tab_actions::new_tab_insert_index(
            &tabs,
            1,
            &space,
            NewTabPlacement::End,
        );
        assert_eq!(at_end, 2, "L09 background tab honors EndOfSpace placement");
        assert_eq!(
            safe_bookmark_url("https://user:pass@news.example/article").as_deref(),
            Some("https://news.example/article")
        );
        assert!(TAB_LIMIT >= 40, "TAB_LIMIT remains bounded");
    }

    #[test]
    fn l10_private_bookmark_tab_stays_out_of_regular_pages_closed_tabs_zoom_and_downloads() {
        let bookmark_url =
            safe_bookmark_url("https://user:pass@private.example/secret-doc")
                .expect("safe bookmark url");

        let mut regular_tab = Tab::new(1);
        regular_tab.url = "https://regular.example/".to_string();
        regular_tab.private = false;
        let mut private_tab = Tab::new(2);
        private_tab.url = bookmark_url.clone();
        private_tab.private = true;
        let tabs = vec![regular_tab, private_tab];
        let (regular_pages, skipped_private) =
            crate::tab_actions::space_regular_web_pages(&tabs, &crate::spaces::default_space_id());
        assert_eq!(skipped_private, 1);
        assert_eq!(regular_pages.len(), 1);
        assert_eq!(regular_pages[0].1, "https://regular.example/");

        let mut closed_stack = Vec::new();
        crate::app::push_closed_tab(&mut closed_stack, &tabs[1], 1);
        assert!(
            closed_stack.is_empty(),
            "L10: closing a privately opened bookmark tab must not enter closed-tab recovery"
        );

        let mut zoom_map = BTreeMap::new();
        zoom_map.insert("private.example".to_string(), 1.5);
        assert_eq!(
            crate::tab_actions::zoom_override(&zoom_map, &bookmark_url, true),
            None,
            "L10: private bookmark tab must not read regular per-host zoom overrides"
        );
        assert_eq!(
            crate::tab_actions::zoom_override(&zoom_map, &bookmark_url, false),
            Some(1.5)
        );

        let private_download = DownloadRec {
            id: 1,
            name: "secret.pdf".into(),
            path: "/tmp/secret.pdf".into(),
            done: true,
            failed: false,
            got: 1024,
            total: 1024,
            private: true,
            source_url: bookmark_url,
            missing: false,
        };
        let kept: Vec<_> = [private_download]
            .into_iter()
            .filter(|entry| (entry.done || entry.failed) && !entry.private)
            .collect();
        assert!(
            kept.is_empty(),
            "L10: private tab downloads stay excluded from persistence filter"
        );
    }

    #[test]
    fn l12_bookmark_removal_and_position_restore_preserves_order_and_dedupe_key() {
        let mut list = vec![
            bm("https://one.example/", "One"),
            bm("https://two.example/", "Two"),
            bm("https://three.example/", "Three"),
        ];
        let index = list
            .iter()
            .position(|kept| kept.url == "https://two.example/")
            .expect("present");
        let removed = list.remove(index);
        assert_eq!(index, 1);
        assert_eq!(list.len(), 2);

        let restore_at = index.min(list.len());
        list.insert(restore_at, removed.clone());
        assert_eq!(
            list.iter().map(|b| b.url.as_str()).collect::<Vec<_>>(),
            vec![
                "https://one.example/",
                "https://two.example/",
                "https://three.example/",
            ]
        );

        let canonical_key = crate::tab_actions::duplicate_url_key(&removed.url);
        assert_eq!(
            crate::tab_actions::duplicate_url_key("https://TWO.EXAMPLE:443/"),
            canonical_key,
            "L12: canonical URL key detects equivalent duplicate bookmark"
        );

        let tail_idx = list
            .iter()
            .position(|kept| kept.url == "https://three.example/")
            .expect("tail");
        let tail = list.remove(tail_idx);
        list.clear();
        let clamped_at = tail_idx.min(list.len());
        list.insert(clamped_at, tail);
        assert_eq!(clamped_at, 0, "L12: restore clamps index when list shrank");
        assert_eq!(list[0].url, "https://three.example/");
    }
}
