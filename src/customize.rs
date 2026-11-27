use crate::store::{
    Look, Prefs, StartBookmarksMode, StartDitherMode, WindowControlsMode,
    DEFAULT_SIDE_WIDTH, DEFAULT_START_SHORTCUTS_COUNT, DEFAULT_TOP_TAB_WIDTH,
    MAX_SIDE_WIDTH, MAX_START_SHORTCUTS_COUNT, MAX_TOP_TAB_WIDTH,
    MIN_SIDE_WIDTH, MIN_START_SHORTCUTS_COUNT, MIN_TOP_TAB_WIDTH,
};
use serde::{Deserialize, Serialize};

pub const PROFILE_SCHEMA_VERSION: u32 = 1;
pub const MAX_IMPORT_BYTES: usize = 64 * 1024;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[serde(rename_all = "kebab-case")]
pub enum ToolbarAction {
    Back,
    Forward,
    Reload,
    Home,
    Address,
    Reader,
    Bookmark,
}

impl ToolbarAction {
    pub const CANONICAL: [ToolbarAction; 7] = [
        ToolbarAction::Back,
        ToolbarAction::Forward,
        ToolbarAction::Reload,
        ToolbarAction::Home,
        ToolbarAction::Address,
        ToolbarAction::Reader,
        ToolbarAction::Bookmark,
    ];

    pub fn default_order() -> Vec<ToolbarAction> {
        Self::CANONICAL.to_vec()
    }

    pub fn title(self) -> &'static str {
        match self {
            ToolbarAction::Back => "Back",
            ToolbarAction::Forward => "Forward",
            ToolbarAction::Reload => "Reload / Stop",
            ToolbarAction::Home => "Home",
            ToolbarAction::Address => "Address bar (required)",
            ToolbarAction::Reader => "Reader mode",
            ToolbarAction::Bookmark => "Bookmark",
        }
    }

    pub fn is_mandatory(self) -> bool {
        matches!(self, ToolbarAction::Address)
    }

    pub fn is_visible_in(self, prefs: &Prefs) -> bool {
        match self {
            ToolbarAction::Back => prefs.show_back_button,
            ToolbarAction::Forward => prefs.show_forward_button,
            ToolbarAction::Reload => prefs.show_reload_button,
            ToolbarAction::Home => prefs.show_home_button,
            ToolbarAction::Address => true,
            ToolbarAction::Reader => prefs.show_reader_button,
            ToolbarAction::Bookmark => prefs.show_bookmark_button,
        }
    }
}

pub fn normalize_toolbar_order(order: &[ToolbarAction]) -> Vec<ToolbarAction> {
    let mut normalized = Vec::with_capacity(ToolbarAction::CANONICAL.len());
    for &action in order {
        if !normalized.contains(&action) {
            normalized.push(action);
        }
    }
    for &action in &ToolbarAction::CANONICAL {
        if !normalized.contains(&action) {
            normalized.push(action);
        }
    }
    normalized
}

pub fn move_toolbar_action(order: &[ToolbarAction], action: ToolbarAction, delta: isize) -> Vec<ToolbarAction> {
    let mut next = normalize_toolbar_order(order);
    let Some(index) = next.iter().position(|item| *item == action) else {
        return next;
    };
    let target = (index as isize + delta).clamp(0, (next.len() - 1) as isize) as usize;
    if target != index {
        let item = next.remove(index);
        next.insert(target, item);
    }
    next
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutPreset {
    Minimal,
    Standard,
    Full,
}

impl LayoutPreset {
    pub const ALL: [LayoutPreset; 3] = [
        LayoutPreset::Minimal,
        LayoutPreset::Standard,
        LayoutPreset::Full,
    ];

    pub fn title(self) -> &'static str {
        match self {
            LayoutPreset::Minimal => "Minimal",
            LayoutPreset::Standard => "Standard",
            LayoutPreset::Full => "Full",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            LayoutPreset::Minimal => "Quiet defaults: Back, Forward, Reload, and Address bar; extra actions in menu.",
            LayoutPreset::Standard => "Adds Home and Bookmark buttons in the toolbar while staying compact.",
            LayoutPreset::Full => "Shows every toolbar action, Bookmarks bar, and roomier spacing.",
        }
    }

    pub fn apply(self, prefs: &mut Prefs) {
        prefs.toolbar_order = ToolbarAction::default_order();
        prefs.show_toolbar = true;
        prefs.column_folded = false;
        prefs.side_width = DEFAULT_SIDE_WIDTH;
        prefs.top_tab_width = DEFAULT_TOP_TAB_WIDTH;
        prefs.show_start_shortcuts = true;
        prefs.start_shortcuts_count = DEFAULT_START_SHORTCUTS_COUNT;
        prefs.start_dither = StartDitherMode::Full;
        match self {
            LayoutPreset::Minimal => {
                prefs.show_back_button = true;
                prefs.show_forward_button = true;
                prefs.show_nav_buttons = true;
                prefs.show_reload_button = true;
                prefs.show_home_button = false;
                prefs.show_reader_button = false;
                prefs.show_bookmark_button = false;
                prefs.show_bookmarks_bar = false;
                prefs.sidebar = false;
                prefs.roomy_tabs = false;
                prefs.start_bookmarks_mode = StartBookmarksMode::IconsAndNames;
            }
            LayoutPreset::Standard => {
                prefs.show_back_button = true;
                prefs.show_forward_button = true;
                prefs.show_nav_buttons = true;
                prefs.show_reload_button = true;
                prefs.show_home_button = true;
                prefs.show_reader_button = false;
                prefs.show_bookmark_button = true;
                prefs.show_bookmarks_bar = false;
                prefs.sidebar = false;
                prefs.roomy_tabs = false;
                prefs.start_bookmarks_mode = StartBookmarksMode::IconsAndNames;
            }
            LayoutPreset::Full => {
                prefs.show_back_button = true;
                prefs.show_forward_button = true;
                prefs.show_nav_buttons = true;
                prefs.show_reload_button = true;
                prefs.show_home_button = true;
                prefs.show_reader_button = true;
                prefs.show_bookmark_button = true;
                prefs.show_bookmarks_bar = true;
                prefs.sidebar = false;
                prefs.roomy_tabs = true;
                prefs.start_bookmarks_mode = StartBookmarksMode::IconsNamesUrl;
            }
        }
    }

    pub fn detect(prefs: &Prefs) -> Option<LayoutPreset> {
        if normalize_toolbar_order(&prefs.toolbar_order) != ToolbarAction::default_order() {
            return None;
        }
        if !prefs.show_toolbar
            || prefs.sidebar
            || prefs.column_folded
            || (prefs.side_width - DEFAULT_SIDE_WIDTH).abs() > 0.5
            || prefs.top_tab_width != DEFAULT_TOP_TAB_WIDTH
            || !prefs.show_start_shortcuts
            || prefs.start_shortcuts_count != DEFAULT_START_SHORTCUTS_COUNT
            || prefs.start_dither != StartDitherMode::Full
        {
            return None;
        }
        if prefs.show_back_button
            && prefs.show_forward_button
            && prefs.show_reload_button
            && !prefs.show_home_button
            && !prefs.show_reader_button
            && !prefs.show_bookmark_button
            && !prefs.show_bookmarks_bar
            && !prefs.roomy_tabs
            && prefs.start_bookmarks_mode == StartBookmarksMode::IconsAndNames
        {
            Some(LayoutPreset::Minimal)
        } else if prefs.show_back_button
            && prefs.show_forward_button
            && prefs.show_reload_button
            && prefs.show_home_button
            && !prefs.show_reader_button
            && prefs.show_bookmark_button
            && !prefs.show_bookmarks_bar
            && !prefs.roomy_tabs
            && prefs.start_bookmarks_mode == StartBookmarksMode::IconsAndNames
        {
            Some(LayoutPreset::Standard)
        } else if prefs.show_back_button
            && prefs.show_forward_button
            && prefs.show_reload_button
            && prefs.show_home_button
            && prefs.show_reader_button
            && prefs.show_bookmark_button
            && prefs.show_bookmarks_bar
            && prefs.roomy_tabs
            && prefs.start_bookmarks_mode == StartBookmarksMode::IconsNamesUrl
        {
            Some(LayoutPreset::Full)
        } else {
            None
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct AppearanceProfile {
    pub schema_version: u32,
    pub appearance: Look,
    pub window_controls: WindowControlsMode,
    pub sidebar: bool,
    pub column_folded: bool,
    pub side_width: f64,
    pub top_tab_width: i32,
    pub roomy_tabs: bool,
    pub show_toolbar: bool,
    pub show_back_button: bool,
    pub show_forward_button: bool,
    pub show_reload_button: bool,
    pub show_home_button: bool,
    pub show_reader_button: bool,
    pub show_bookmark_button: bool,
    pub toolbar_order: Vec<ToolbarAction>,
    pub show_bookmarks_bar: bool,
    pub show_start_shortcuts: bool,
    pub start_shortcuts_count: usize,
    pub start_bookmarks_mode: StartBookmarksMode,
    pub start_dither: StartDitherMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileError {
    TooLarge,
    InvalidJson,
    UnsupportedVersion(u32),
    InvalidSideWidth,
    InvalidTopTabWidth,
    InvalidStartShortcutsCount,
    InvalidToolbarOrder,
}

impl ProfileError {
    pub fn message(&self) -> String {
        match self {
            ProfileError::TooLarge => "Layout file is too large (max 64 KB).".into(),
            ProfileError::InvalidJson => "Layout file is not a valid Rill appearance schema.".into(),
            ProfileError::UnsupportedVersion(v) => format!("Unsupported layout schema version {v}."),
            ProfileError::InvalidSideWidth => {
                format!(
                    "Sidebar width must be between {} and {} px.",
                    MIN_SIDE_WIDTH as i32, MAX_SIDE_WIDTH as i32
                )
            }
            ProfileError::InvalidTopTabWidth => {
                format!("Top tab width must be between {MIN_TOP_TAB_WIDTH} and {MAX_TOP_TAB_WIDTH} px.")
            }
            ProfileError::InvalidStartShortcutsCount => {
                format!(
                    "Homepage shortcut count must be between {MIN_START_SHORTCUTS_COUNT} and {MAX_START_SHORTCUTS_COUNT}."
                )
            }
            ProfileError::InvalidToolbarOrder => {
                "Toolbar action order must list each action once including the address bar.".into()
            }
        }
    }
}

impl AppearanceProfile {
    pub fn from_prefs(prefs: &Prefs) -> Self {
        Self {
            schema_version: PROFILE_SCHEMA_VERSION,
            appearance: prefs.appearance.clone(),
            window_controls: prefs.window_controls,
            sidebar: prefs.sidebar,
            column_folded: prefs.column_folded,
            side_width: prefs.side_width.clamp(MIN_SIDE_WIDTH, MAX_SIDE_WIDTH),
            top_tab_width: prefs.top_tab_width.clamp(MIN_TOP_TAB_WIDTH, MAX_TOP_TAB_WIDTH),
            roomy_tabs: prefs.roomy_tabs,
            show_toolbar: prefs.show_toolbar,
            show_back_button: prefs.show_back_button,
            show_forward_button: prefs.show_forward_button,
            show_reload_button: prefs.show_reload_button,
            show_home_button: prefs.show_home_button,
            show_reader_button: prefs.show_reader_button,
            show_bookmark_button: prefs.show_bookmark_button,
            toolbar_order: normalize_toolbar_order(&prefs.toolbar_order),
            show_bookmarks_bar: prefs.show_bookmarks_bar,
            show_start_shortcuts: prefs.show_start_shortcuts,
            start_shortcuts_count: prefs
                .start_shortcuts_count
                .clamp(MIN_START_SHORTCUTS_COUNT, MAX_START_SHORTCUTS_COUNT),
            start_bookmarks_mode: prefs.start_bookmarks_mode,
            start_dither: prefs.start_dither,
        }
    }

    pub fn validate(&self) -> Result<(), ProfileError> {
        if self.schema_version != PROFILE_SCHEMA_VERSION {
            return Err(ProfileError::UnsupportedVersion(self.schema_version));
        }
        if !self.side_width.is_finite()
            || self.side_width < MIN_SIDE_WIDTH
            || self.side_width > MAX_SIDE_WIDTH
        {
            return Err(ProfileError::InvalidSideWidth);
        }
        if self.top_tab_width < MIN_TOP_TAB_WIDTH || self.top_tab_width > MAX_TOP_TAB_WIDTH {
            return Err(ProfileError::InvalidTopTabWidth);
        }
        if self.start_shortcuts_count < MIN_START_SHORTCUTS_COUNT
            || self.start_shortcuts_count > MAX_START_SHORTCUTS_COUNT
        {
            return Err(ProfileError::InvalidStartShortcutsCount);
        }
        if self.toolbar_order.len() != ToolbarAction::CANONICAL.len()
            || !self.toolbar_order.contains(&ToolbarAction::Address)
        {
            return Err(ProfileError::InvalidToolbarOrder);
        }
        let mut unique = Vec::with_capacity(ToolbarAction::CANONICAL.len());
        for &action in &self.toolbar_order {
            if unique.contains(&action) {
                return Err(ProfileError::InvalidToolbarOrder);
            }
            unique.push(action);
        }
        if unique.len() != ToolbarAction::CANONICAL.len() {
            return Err(ProfileError::InvalidToolbarOrder);
        }
        Ok(())
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json_str(raw: &str) -> Result<Self, ProfileError> {
        if raw.len() > MAX_IMPORT_BYTES {
            return Err(ProfileError::TooLarge);
        }
        let parsed: Self = serde_json::from_str(raw).map_err(|_| ProfileError::InvalidJson)?;
        parsed.validate()?;
        Ok(parsed)
    }

    pub fn apply_to(&self, prefs: &mut Prefs) -> Result<(), ProfileError> {
        self.validate()?;
        let mut staged = prefs.clone();
        staged.appearance = self.appearance.clone();
        staged.window_controls = self.window_controls;
        staged.sidebar = self.sidebar;
        staged.column_folded = self.column_folded;
        staged.side_width = self.side_width;
        staged.top_tab_width = self.top_tab_width;
        staged.roomy_tabs = self.roomy_tabs;
        staged.show_toolbar = self.show_toolbar;
        staged.show_back_button = self.show_back_button;
        staged.show_forward_button = self.show_forward_button;
        staged.show_nav_buttons = self.show_back_button || self.show_forward_button;
        staged.show_reload_button = self.show_reload_button;
        staged.show_home_button = self.show_home_button;
        staged.show_reader_button = self.show_reader_button;
        staged.show_bookmark_button = self.show_bookmark_button;
        staged.toolbar_order = normalize_toolbar_order(&self.toolbar_order);
        staged.show_bookmarks_bar = self.show_bookmarks_bar;
        staged.show_start_shortcuts = self.show_start_shortcuts;
        staged.start_shortcuts_count = self.start_shortcuts_count;
        staged.start_bookmarks_mode = self.start_bookmarks_mode;
        staged.start_dither = self.start_dither;
        *prefs = staged;
        Ok(())
    }
}

pub fn export_prefs_json(prefs: &Prefs) -> Result<String, serde_json::Error> {
    AppearanceProfile::from_prefs(prefs).to_json_pretty()
}

pub fn import_prefs_json(raw: &str, prefs: &mut Prefs) -> Result<AppearanceProfile, ProfileError> {
    let profile = AppearanceProfile::from_json_str(raw)?;
    profile.apply_to(prefs)?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_order_normalization_deduplicates_and_keeps_address() {
        let input = vec![
            ToolbarAction::Reload,
            ToolbarAction::Reload,
            ToolbarAction::Home,
        ];
        let normalized = normalize_toolbar_order(&input);
        assert_eq!(normalized.len(), ToolbarAction::CANONICAL.len());
        assert_eq!(normalized[0], ToolbarAction::Reload);
        assert_eq!(normalized[1], ToolbarAction::Home);
        assert!(normalized.contains(&ToolbarAction::Address));
        for action in ToolbarAction::CANONICAL {
            assert_eq!(
                normalized.iter().filter(|a| **a == action).count(),
                1,
                "missing or duplicated action {action:?}"
            );
        }
    }

    #[test]
    fn move_toolbar_action_respects_bounds_and_reorders_address() {
        let base = ToolbarAction::default_order();
        let unchanged = move_toolbar_action(&base, ToolbarAction::Back, -1);
        assert_eq!(unchanged, base);

        let moved = move_toolbar_action(&base, ToolbarAction::Address, -1);
        assert_eq!(moved[3], ToolbarAction::Address);
        assert_eq!(moved[4], ToolbarAction::Home);

        let moved_end = move_toolbar_action(&base, ToolbarAction::Bookmark, 1);
        assert_eq!(moved_end, base);
    }

    #[test]
    fn layout_presets_apply_and_detect_round_trip() {
        let mut prefs = Prefs::default();
        assert_eq!(LayoutPreset::detect(&prefs), Some(LayoutPreset::Minimal));

        for preset in LayoutPreset::ALL {
            preset.apply(&mut prefs);
            assert_eq!(LayoutPreset::detect(&prefs), Some(preset));
        }

        prefs.toolbar_order = move_toolbar_action(&prefs.toolbar_order, ToolbarAction::Home, -1);
        assert_eq!(LayoutPreset::detect(&prefs), None);
    }

    #[test]
    fn export_contains_only_allowlisted_appearance_and_layout_fields() {
        let mut prefs = Prefs::default();
        prefs.shield = false;
        prefs.curtain = false;
        prefs.low_memory = true;
        prefs.search_engine = "DuckDuckGo".into();
        prefs.engine_suggestions = false;
        prefs.restore_session = false;
        prefs.darken_pages = true;

        let json = export_prefs_json(&prefs).expect("export serializes");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("valid json");
        let map = parsed.as_object().expect("json object");

        for forbidden in [
            "shield",
            "curtain",
            "low_memory",
            "memory_saver",
            "sleep_minutes",
            "restore_session",
            "search_engine",
            "engine_suggestions",
            "keywords",
            "darken_pages",
            "window_size",
            "window_maximised",
            "welcomed",
            "cookies",
            "history",
            "passwords",
        ] {
            assert!(!map.contains_key(forbidden), "unexpected sensitive/runtime key {forbidden}");
        }
        assert_eq!(map.get("schema_version").and_then(|v| v.as_u64()), Some(1));
    }

    #[test]
    fn import_rejects_invalid_schema_bounds_and_unknown_fields_atomically() {
        let mut prefs = Prefs::default();
        prefs.appearance = Look::Dark;
        prefs.side_width = 280.0;
        prefs.top_tab_width = 180;
        prefs.show_home_button = true;
        prefs.shield = false;
        prefs.search_engine = "Brave".into();
        let before = serde_json::to_string(&prefs).unwrap();

        let bad_cases = [
            ("not json", ProfileError::InvalidJson),
            (
                r#"{"schema_version":99,"appearance":"light","window_controls":"system","sidebar":false,"column_folded":false,"side_width":224.0,"top_tab_width":144,"roomy_tabs":false,"show_toolbar":true,"show_back_button":true,"show_forward_button":true,"show_reload_button":true,"show_home_button":false,"show_reader_button":false,"show_bookmark_button":false,"toolbar_order":["back","forward","reload","home","address","reader","bookmark"],"show_bookmarks_bar":false,"show_start_shortcuts":true,"start_shortcuts_count":8,"start_bookmarks_mode":"icons-and-names","start_dither":"full"}"#,
                ProfileError::UnsupportedVersion(99),
            ),
            (
                r#"{"schema_version":1,"appearance":"light","window_controls":"system","sidebar":false,"column_folded":false,"side_width":999.0,"top_tab_width":144,"roomy_tabs":false,"show_toolbar":true,"show_back_button":true,"show_forward_button":true,"show_reload_button":true,"show_home_button":false,"show_reader_button":false,"show_bookmark_button":false,"toolbar_order":["back","forward","reload","home","address","reader","bookmark"],"show_bookmarks_bar":false,"show_start_shortcuts":true,"start_shortcuts_count":8,"start_bookmarks_mode":"icons-and-names","start_dither":"full"}"#,
                ProfileError::InvalidSideWidth,
            ),
            (
                r#"{"schema_version":1,"appearance":"light","window_controls":"system","sidebar":false,"column_folded":false,"side_width":224.0,"top_tab_width":40,"roomy_tabs":false,"show_toolbar":true,"show_back_button":true,"show_forward_button":true,"show_reload_button":true,"show_home_button":false,"show_reader_button":false,"show_bookmark_button":false,"toolbar_order":["back","forward","reload","home","address","reader","bookmark"],"show_bookmarks_bar":false,"show_start_shortcuts":true,"start_shortcuts_count":8,"start_bookmarks_mode":"icons-and-names","start_dither":"full"}"#,
                ProfileError::InvalidTopTabWidth,
            ),
            (
                r#"{"schema_version":1,"appearance":"light","window_controls":"system","sidebar":false,"column_folded":false,"side_width":224.0,"top_tab_width":144,"roomy_tabs":false,"show_toolbar":true,"show_back_button":true,"show_forward_button":true,"show_reload_button":true,"show_home_button":false,"show_reader_button":false,"show_bookmark_button":false,"toolbar_order":["back","forward","reload","home","address","reader","bookmark"],"show_bookmarks_bar":false,"show_start_shortcuts":true,"start_shortcuts_count":99,"start_bookmarks_mode":"icons-and-names","start_dither":"full"}"#,
                ProfileError::InvalidStartShortcutsCount,
            ),
            (
                r#"{"schema_version":1,"appearance":"light","window_controls":"system","sidebar":false,"column_folded":false,"side_width":224.0,"top_tab_width":144,"roomy_tabs":false,"show_toolbar":true,"show_back_button":true,"show_forward_button":true,"show_reload_button":true,"show_home_button":false,"show_reader_button":false,"show_bookmark_button":false,"toolbar_order":["back","forward","reload","home","reader","bookmark"],"show_bookmarks_bar":false,"show_start_shortcuts":true,"start_shortcuts_count":8,"start_bookmarks_mode":"icons-and-names","start_dither":"full"}"#,
                ProfileError::InvalidToolbarOrder,
            ),
            (
                r#"{"schema_version":1,"appearance":"light","window_controls":"system","sidebar":false,"column_folded":false,"side_width":224.0,"top_tab_width":144,"roomy_tabs":false,"show_toolbar":true,"show_back_button":true,"show_forward_button":true,"show_reload_button":true,"show_home_button":false,"show_reader_button":false,"show_bookmark_button":false,"toolbar_order":["back","forward","reload","home","address","reader","bookmark"],"show_bookmarks_bar":false,"show_start_shortcuts":true,"start_shortcuts_count":8,"start_bookmarks_mode":"icons-and-names","start_dither":"full","shield":false}"#,
                ProfileError::InvalidJson,
            ),
        ];

        for (payload, expected_err) in bad_cases {
            let err = import_prefs_json(payload, &mut prefs).unwrap_err();
            assert_eq!(err, expected_err);
            assert_eq!(serde_json::to_string(&prefs).unwrap(), before);
        }

        let oversized = " ".repeat(MAX_IMPORT_BYTES + 1);
        assert_eq!(import_prefs_json(&oversized, &mut prefs).unwrap_err(), ProfileError::TooLarge);
        assert_eq!(serde_json::to_string(&prefs).unwrap(), before);
    }

    #[test]
    fn valid_import_updates_layout_without_touching_privacy_or_search_prefs() {
        let mut source = Prefs::default();
        LayoutPreset::Full.apply(&mut source);
        source.appearance = Look::Light;
        source.top_tab_width = 196;
        source.side_width = 272.0;
        source.show_forward_button = false;
        source.start_shortcuts_count = 6;
        source.start_dither = StartDitherMode::Subtle;
        source.toolbar_order = move_toolbar_action(&source.toolbar_order, ToolbarAction::Bookmark, -2);
        let exported = export_prefs_json(&source).expect("export succeeds");

        let mut target = Prefs::default();
        target.shield = false;
        target.curtain = false;
        target.search_engine = "DuckDuckGo".into();
        target.engine_suggestions = false;

        import_prefs_json(&exported, &mut target).expect("import succeeds");
        assert_eq!(target.appearance, Look::Light);
        assert_eq!(target.top_tab_width, 196);
        assert_eq!(target.side_width, 272.0);
        assert!(target.show_back_button);
        assert!(!target.show_forward_button);
        assert!(target.show_nav_buttons);
        assert_eq!(target.start_shortcuts_count, 6);
        assert_eq!(target.start_dither, StartDitherMode::Subtle);
        assert_eq!(target.toolbar_order, source.toolbar_order);
        assert!(!target.shield);
        assert!(!target.curtain);
        assert_eq!(target.search_engine, "DuckDuckGo");
        assert!(!target.engine_suggestions);
    }

    #[test]
    fn import_rejects_duplicate_toolbar_actions_atomically() {
        let mut prefs = Prefs::default();
        let before = serde_json::to_string(&prefs).unwrap();
        let dup = r#"{"schema_version":1,"appearance":"light","window_controls":"system","sidebar":false,"column_folded":false,"side_width":224.0,"top_tab_width":144,"roomy_tabs":false,"show_toolbar":true,"show_back_button":true,"show_forward_button":true,"show_reload_button":true,"show_home_button":false,"show_reader_button":false,"show_bookmark_button":false,"toolbar_order":["back","back","reload","home","address","reader","bookmark"],"show_bookmarks_bar":false,"show_start_shortcuts":true,"start_shortcuts_count":8,"start_bookmarks_mode":"icons-and-names","start_dither":"full"}"#;
        assert_eq!(import_prefs_json(dup, &mut prefs).unwrap_err(), ProfileError::InvalidToolbarOrder);
        assert_eq!(serde_json::to_string(&prefs).unwrap(), before);
    }
}
