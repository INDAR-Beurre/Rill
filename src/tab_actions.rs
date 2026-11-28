// Pure tab, space, URL, and named-session actions for Rill 1.2.
//
// Keeping selection rules, URL sanitization, tracking-parameter stripping,
// address-list parsing, and versioned named-session storage here keeps app.rs
// concise and makes every invariant directly unit-testable without a display.
use crate::app::{ClosedTab, Tab, TabId, TAB_LIMIT};
use crate::store::{CloseTabFocus, NewTabPlacement, TabRecord};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet};

pub const NAMED_SESSIONS_VERSION: u32 = 1;
pub const MAX_NAMED_SESSIONS: usize = 24;
pub const MAX_SESSION_NAME_LEN: usize = 48;
pub const MAX_KEEP_AWAKE_HOSTS: usize = 64;
pub const MAX_CLOSED_BATCHES: usize = 10;
pub const BULK_CLOSE_WARN_THRESHOLD: usize = 3;
pub const MAX_IMPORT_LIST_BYTES: usize = 512 * 1024;
pub const MAX_URL_LEN: usize = 4096;

/// Known tracking query parameters stripped by T45 ("Copy clean link").
/// Exact lowercase names plus prefix patterns (`utm_`, `mtm_`, `hsa_`).
const EXACT_TRACKING_PARAMS: &[&str] = &[
    "fbclid",
    "gclid",
    "gbraid",
    "wbraid",
    "dclid",
    "msclkid",
    "mc_cid",
    "mc_eid",
    "igshid",
    "yclid",
    "_hsenc",
    "_hsmi",
    "mkt_tok",
    "vero_id",
    "oly_enc_id",
    "oly_anon_id",
    "ref_src",
    "ref_url",
    "trk",
    "trkcampaign",
    "trksource",
    "trkmedium",
    "pk_campaign",
    "pk_kwd",
    "pk_source",
    "pk_medium",
    "pk_content",
];

fn is_tracking_param(key: &str) -> bool {
    let lower = percent_decode(key);
    let lower = lower.to_ascii_lowercase();
    lower.starts_with("utm_")
        || lower.starts_with("mtm_")
        || lower.starts_with("hsa_")
        || EXACT_TRACKING_PARAMS.contains(&lower.as_str())
}

/// Decode `%XX` escapes in a raw query-key without touching `+` (which is a
/// literal plus in a query key, not a space) and without allocating for plain
/// ASCII keys.
fn percent_decode(raw: &str) -> String {
    if !raw.contains('%') {
        return raw.to_string();
    }
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && index + 2 < bytes.len()
            && bytes[index + 1].is_ascii_hexdigit()
            && bytes[index + 2].is_ascii_hexdigit()
        {
            let hex = &raw[index + 1..index + 3];
            if let Ok(byte) = u8::from_str_radix(hex, 16) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// True when `url` is a valid explicit `http://` or `https://` URL with a
/// non-empty host and no embedded control characters or whitespace.
pub fn is_web_url(url: &str) -> bool {
    parse_explicit_web_url(url).is_some()
}

/// Parse an explicit `http://` or `https://` URL from user or page selection
/// text (T47). Rejects non-web schemes (`javascript:`, `file:`, `data:`,
/// `about:`, `blob:`), embedded whitespace/control chars, and empty hosts.
pub fn parse_explicit_web_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_URL_LEN {
        return None;
    }
    if trimmed.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme_lower = scheme.to_ascii_lowercase();
    if scheme_lower != "http" && scheme_lower != "https" {
        return None;
    }
    let host = crate::shield::host_of(trimmed);
    if host.is_empty() {
        return None;
    }
    // Require either an IPv6 bracket literal, `localhost`, or a dotted host
    // so arbitrary strings like `https://word` are not silently accepted as
    // explicit URLs unless they look like real network hosts.
    let valid_host = host.starts_with('[') || host == "localhost" || host.contains('.');
    if !valid_host {
        return None;
    }
    Some(format!("{scheme_lower}://{rest}"))
}

/// Remove `user:pass@` userinfo from a URL authority while preserving scheme,
/// host, port, path, query, and fragment.
pub fn strip_url_credentials(url: &str) -> String {
    let trimmed = url.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return trimmed.to_string();
    };
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..authority_end];
    let suffix = &rest[authority_end..];
    let clean_authority = match authority.rsplit_once('@') {
        Some((_, host_port)) => host_port,
        None => authority,
    };
    format!("{scheme}://{clean_authority}{suffix}")
}

/// Canonical `(scheme, host, port)` origin tuple for HTTP(S) URLs.
pub fn origin_key(url: &str) -> Option<(String, String, u16)> {
    let clean = parse_explicit_web_url(url)?;
    let (scheme, rest) = clean.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host_port = authority.rsplit('@').next().unwrap_or(authority);
    let default_port = if scheme == "https" { 443 } else { 80 };
    if host_port.starts_with('[') {
        let end = host_port.find(']')?;
        let host = host_port[..=end].to_ascii_lowercase();
        let after = &host_port[end + 1..];
        let port = if let Some(p) = after.strip_prefix(':') {
            p.parse::<u16>().ok()?
        } else {
            default_port
        };
        return Some((scheme.to_string(), host, port));
    }
    let (host, port) = match host_port.split_once(':') {
        Some((h, p)) => (h.to_ascii_lowercase(), p.parse::<u16>().ok()?),
        None => (host_port.to_ascii_lowercase(), default_port),
    };
    if host.is_empty() {
        return None;
    }
    Some((scheme.to_string(), host, port))
}

/// Normalized URL comparison key for duplicate detection: strips credentials,
/// lowercases scheme & host, strips default port, preserves path & query, drops
/// trailing empty `#`.
pub fn duplicate_url_key(url: &str) -> String {
    let stripped = strip_url_credentials(url);
    let trimmed = stripped.trim();
    let Some((scheme, rest)) = trimmed.split_once("://") else {
        return trimmed.to_string();
    };
    let scheme_lower = scheme.to_ascii_lowercase();
    let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = rest[..authority_end].to_ascii_lowercase();
    let authority = if scheme_lower == "https" {
        authority.strip_suffix(":443").unwrap_or(&authority)
    } else if scheme_lower == "http" {
        authority.strip_suffix(":80").unwrap_or(&authority)
    } else {
        authority.as_str()
    };
    let suffix = &rest[authority_end..];
    let suffix = if suffix.is_empty() { "/" } else { suffix };
    let suffix = suffix.strip_suffix('#').unwrap_or(suffix);
    format!("{scheme_lower}://{authority}{suffix}")
}

/// Normalize a host rule for T13 ("Keep chosen sites awake"). Accepts a bare
/// host or a full URL and returns an exact lowercase host, rejecting wildcards,
/// paths, whitespace, or invalid labels.
pub fn normalize_host_rule(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > 2048 || trimmed.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return None;
    }
    if trimmed.contains('*') {
        return None;
    }
    let host = if trimmed.contains("://") {
        if !is_web_url(trimmed) {
            return None;
        }
        crate::shield::host_of(trimmed)
    } else {
        // Bare host must not contain path/query/fragment/userinfo separators.
        if trimmed.contains(['/', '?', '#', '@']) {
            return None;
        }
        crate::shield::host_of(&format!("https://{trimmed}"))
    };
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    if !(host.starts_with('[') || host == "localhost" || host.contains('.')) {
        return None;
    }
    Some(host)
}

pub fn sanitize_keep_awake_hosts(hosts: Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for raw in hosts {
        if out.len() >= MAX_KEEP_AWAKE_HOSTS {
            break;
        }
        if let Some(host) = normalize_host_rule(&raw) {
            if seen.insert(host.clone()) {
                out.push(host);
            }
        }
    }
    out
}

pub fn is_host_kept_awake(url: &str, keep_awake_hosts: &[String]) -> bool {
    let host = crate::shield::host_of(url);
    !host.is_empty() && keep_awake_hosts.iter().any(|h| h == &host)
}

/// Strip known tracking query parameters while keeping meaningful parameters
/// and fragments intact and stripping userinfo credentials (T45).
pub fn strip_tracking_params(url: &str) -> String {
    let clean = strip_url_credentials(url);
    let (before_frag, frag) = match clean.split_once('#') {
        Some((b, f)) => (b, Some(f)),
        None => (clean.as_str(), None),
    };
    let Some((base, query)) = before_frag.split_once('?') else {
        return clean;
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|pair| {
            if pair.is_empty() {
                return false;
            }
            let key = pair.split_once('=').map(|(k, _)| k).unwrap_or(pair);
            !is_tracking_param(key)
        })
        .collect();
    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    out
}

pub fn has_tracking_params(url: &str) -> bool {
    let clean = strip_url_credentials(url);
    let before_frag = clean.split_once('#').map(|(b, _)| b).unwrap_or(&clean);
    let Some((_, query)) = before_frag.split_once('?') else {
        return false;
    };
    query.split('&').any(|pair| {
        if pair.is_empty() {
            return false;
        }
        let key = pair.split_once('=').map(|(k, _)| k).unwrap_or(pair);
        is_tracking_param(key)
    })
}

/// Plain-text "Title\nURL" without credentials (T15).
pub fn format_title_and_url(title: &str, url: &str) -> String {
    let clean_url = strip_url_credentials(url);
    let clean_title: String = title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean_title.is_empty() {
        clean_url
    } else if clean_url.is_empty() {
        clean_title
    } else {
        format!("{clean_title}\n{clean_url}")
    }
}

/// Escaped Markdown `[Title](URL)` link (T16).
pub fn format_markdown_link(title: &str, url: &str) -> String {
    let clean_url = strip_url_credentials(url);
    let raw_title = if title.trim().is_empty() {
        let host = crate::shield::host_of(&clean_url);
        if host.is_empty() { clean_url.clone() } else { host }
    } else {
        title
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let mut escaped_title = String::with_capacity(raw_title.len());
    for ch in raw_title.chars() {
        if matches!(ch, '\\' | '[' | ']' | '(' | ')' | '*' | '_' | '`' | '<' | '>' | '~' | '|') {
            escaped_title.push('\\');
        }
        escaped_title.push(ch);
    }
    let mut safe_url = String::with_capacity(clean_url.len());
    for ch in clean_url.chars() {
        match ch {
            '(' => safe_url.push_str("%28"),
            ')' => safe_url.push_str("%29"),
            '<' => safe_url.push_str("%3C"),
            '>' => safe_url.push_str("%3E"),
            ' ' => safe_url.push_str("%20"),
            '"' => safe_url.push_str("%22"),
            '\\' => safe_url.push_str("%5C"),
            c if c.is_control() => {}
            c => safe_url.push(c),
        }
    }
    format!("[{escaped_title}]({safe_url})")
}

/// Validate a custom Home address (T38). Empty means built-in Start Page.
pub fn validate_home_url(raw: &str) -> Result<String, &'static str> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    if trimmed.len() > 2048 {
        return Err("Home address is too long");
    }
    parse_explicit_web_url(trimmed).ok_or("Enter a full http:// or https:// address, or leave empty for Start Page")
}

/// Sanitize a page title or URL into a safe local filename (T49).
pub fn sanitize_offline_filename(title: &str, url: &str, ext: &str) -> String {
    let fallback = crate::shield::host_of(url);
    let seed = if title.trim().is_empty() {
        if fallback.is_empty() { "page" } else { fallback.as_str() }
    } else {
        title.trim()
    };
    let mut slug = String::new();
    let mut last_dash = false;
    for ch in seed.chars() {
        if slug.chars().count() >= 72 {
            break;
        }
        if ch.is_alphanumeric() {
            slug.push(ch);
            last_dash = false;
        } else if (ch.is_whitespace() || matches!(ch, '-' | '_' | '.')) && !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    let trimmed = slug.trim_matches(|c| c == '-' || c == '.');
    let stem = if trimmed.is_empty() { "page" } else { trimmed };
    let clean_ext = ext.trim_start_matches('.');
    format!("{stem}.{clean_ext}")
}

// ---- Address list import/clipboard review (T35, T48) --------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportPlan {
    pub urls: Vec<String>,
    pub skipped_invalid: usize,
    pub skipped_duplicates: usize,
    pub truncated_by_limit: usize,
}

/// Parse one-address-per-line text into a safe, deduplicated
/// opening plan bounded by `available_slots`. Never treats input as a command
/// or allows unsafe schemes.
pub fn parse_address_list(raw: &str, existing_urls: &[String], available_slots: usize) -> ImportPlan {
    let bounded = if raw.len() > MAX_IMPORT_LIST_BYTES {
        let mut end = MAX_IMPORT_LIST_BYTES;
        while end > 0 && !raw.is_char_boundary(end) {
            end -= 1;
        }
        &raw[..end]
    } else {
        raw
    };
    let mut existing_keys: HashSet<String> = existing_urls
        .iter()
        .filter(|u| is_web_url(u))
        .map(|u| duplicate_url_key(u))
        .collect();
    let mut accepted = Vec::new();
    let mut skipped_invalid = 0usize;
    let mut skipped_duplicates = 0usize;
    let mut truncated_by_limit = 0usize;

    for line in bounded.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(url) = parse_explicit_web_url(trimmed) else {
            skipped_invalid += 1;
            continue;
        };
        let clean = strip_url_credentials(&url);
        let key = duplicate_url_key(&clean);
        if !existing_keys.insert(key) {
            skipped_duplicates += 1;
            continue;
        }
        if accepted.len() < available_slots {
            accepted.push(clean);
        } else {
            truncated_by_limit += 1;
        }
    }

    ImportPlan {
        urls: accepted,
        skipped_invalid,
        skipped_duplicates,
        truncated_by_limit,
    }
}

// ---- Tab batch planning, ordering, and invariants -----------------------

/// T01: Unpinned tabs strictly to the left of `anchor` within the same space.
/// Preserves all pinned tabs and all tabs in other spaces.
pub fn tabs_to_left(tabs: &[Tab], anchor: TabId) -> Vec<TabId> {
    let Some(pos) = tabs.iter().position(|t| t.id == anchor) else {
        return Vec::new();
    };
    if tabs[pos].pinned {
        return Vec::new();
    }
    let space_id = &tabs[pos].space_id;
    tabs[..pos]
        .iter()
        .filter(|t| t.space_id == *space_id && !t.pinned)
        .map(|t| t.id)
        .collect()
}

/// T02: Duplicate unpinned tabs in `keep`'s space. Preserves `keep`, all pinned
/// tabs, tabs in other spaces, and strictly separates private and regular pages.
pub fn duplicate_tabs_to_close(tabs: &[Tab], keep: TabId) -> Vec<TabId> {
    let Some(keep_tab) = tabs.iter().find(|t| t.id == keep) else {
        return Vec::new();
    };
    let space_id = &keep_tab.space_id;
    let mut seen: HashSet<(bool, String)> = HashSet::new();
    // Mark `keep` and all pinned tabs in this space first so they always win.
    if !keep_tab.url.trim().is_empty() {
        seen.insert((keep_tab.private, duplicate_url_key(&keep_tab.url)));
    }
    for tab in tabs.iter().filter(|t| t.space_id == *space_id && t.pinned && !t.url.trim().is_empty()) {
        seen.insert((tab.private, duplicate_url_key(&tab.url)));
    }
    let mut to_close = Vec::new();
    for tab in tabs {
        if tab.space_id != *space_id || tab.id == keep || tab.pinned || tab.url.trim().is_empty() {
            continue;
        }
        let key = (tab.private, duplicate_url_key(&tab.url));
        if !seen.insert(key) {
            to_close.push(tab.id);
        }
    }
    to_close
}

/// T03: Other unpinned tabs from the exact same origin `(scheme, host, port)`
/// and same privacy mode within `anchor`'s space.
pub fn other_site_tabs_to_close(tabs: &[Tab], anchor: TabId) -> Vec<TabId> {
    let Some(anchor_tab) = tabs.iter().find(|t| t.id == anchor) else {
        return Vec::new();
    };
    let Some(target_origin) = origin_key(&anchor_tab.url) else {
        return Vec::new();
    };
    let space_id = &anchor_tab.space_id;
    let privacy = anchor_tab.private;
    tabs.iter()
        .filter(|t| {
            t.space_id == *space_id
                && t.id != anchor
                && !t.pinned
                && t.private == privacy
                && origin_key(&t.url).as_ref() == Some(&target_origin)
        })
        .map(|t| t.id)
        .collect()
}

/// T04 / T05: Move `id` to the beginning (`to_start == true`) or end
/// (`to_start == false`) of its `(space_id, pinned)` partition.
pub fn move_tab_to_edge(tabs: &mut Vec<Tab>, id: TabId, to_start: bool) -> bool {
    let Some(from) = tabs.iter().position(|t| t.id == id) else {
        return false;
    };
    let pinned = tabs[from].pinned;
    let space_id = tabs[from].space_id.clone();
    let partition: Vec<usize> = tabs
        .iter()
        .enumerate()
        .filter(|(_, t)| t.space_id == space_id && t.pinned == pinned)
        .map(|(i, _)| i)
        .collect();
    if partition.len() < 2 {
        return false;
    }
    let target_idx = if to_start { partition[0] } else { *partition.last().unwrap_or(&from) };
    if from == target_idx {
        return false;
    }
    let tab = tabs.remove(from);
    tabs.insert(target_idx, tab);
    true
}

/// T21: Stable case-insensitive sort of unpinned tabs in `space_id` by title
/// (falling back to host/URL; blank tabs sorted last). Pinned tabs and tabs in
/// other spaces remain at their exact slots.
pub fn sort_unpinned_tabs_by_title(tabs: &mut [Tab], space_id: &str) -> bool {
    let indices: Vec<usize> = tabs
        .iter()
        .enumerate()
        .filter(|(_, t)| t.space_id == space_id && !t.pinned)
        .map(|(i, _)| i)
        .collect();
    if indices.len() < 2 {
        return false;
    }
    let mut order: Vec<usize> = (0..indices.len()).collect();
    order.sort_by(|&a, &b| {
        let ta = &tabs[indices[a]];
        let tb = &tabs[indices[b]];
        sort_title_key(ta).cmp(&sort_title_key(tb)).then(a.cmp(&b))
    });
    if order.iter().enumerate().all(|(pos, &orig)| pos == orig) {
        return false;
    }
    apply_index_permutation(tabs, &indices, &order);
    true
}

fn sort_title_key(tab: &Tab) -> (u8, String) {
    let title = tab.title.trim();
    if !title.is_empty() {
        return (0, title.to_lowercase());
    }
    let host = crate::shield::host_of(&tab.url);
    if !host.is_empty() {
        return (0, host.to_lowercase());
    }
    if !tab.url.trim().is_empty() {
        return (0, tab.url.trim().to_lowercase());
    }
    (1, String::new())
}

/// T22: Stable origin grouping of unpinned tabs in `space_id`, preserving
/// first-seen origin order and keeping blank tabs at the end.
pub fn group_unpinned_tabs_by_site(tabs: &mut [Tab], space_id: &str) -> bool {
    let indices: Vec<usize> = tabs
        .iter()
        .enumerate()
        .filter(|(_, t)| t.space_id == space_id && !t.pinned)
        .map(|(i, _)| i)
        .collect();
    if indices.len() < 2 {
        return false;
    }
    let mut first_seen: Vec<(String, usize)> = Vec::new();
    let mut keys: Vec<(u8, usize, usize)> = Vec::with_capacity(indices.len());
    for (local_pos, &tab_idx) in indices.iter().enumerate() {
        let tab = &tabs[tab_idx];
        let group = if let Some((scheme, host, port)) = origin_key(&tab.url) {
            Some(format!("{scheme}://{host}:{port}"))
        } else {
            let host = crate::shield::host_of(&tab.url);
            (!host.is_empty()).then_some(host)
        };
        match group {
            Some(g) => {
                let rank = match first_seen.iter().find(|(name, _)| name == &g) {
                    Some((_, r)) => *r,
                    None => {
                        let r = first_seen.len();
                        first_seen.push((g, r));
                        r
                    }
                };
                keys.push((0, rank, local_pos));
            }
            None => keys.push((1, usize::MAX, local_pos)),
        }
    }
    let mut order: Vec<usize> = (0..indices.len()).collect();
    order.sort_by_key(|&i| keys[i]);
    if order.iter().enumerate().all(|(pos, &orig)| pos == orig) {
        return false;
    }
    apply_index_permutation(tabs, &indices, &order);
    true
}

fn apply_index_permutation(tabs: &mut [Tab], indices: &[usize], order: &[usize]) {
    // Reorder elements at `indices` in-place via swaps from inverse permutation.
    let mut current: Vec<usize> = (0..indices.len()).collect();
    for target_pos in 0..order.len() {
        let wanted = order[target_pos];
        if current[target_pos] == wanted {
            continue;
        }
        if let Some(found) = current[target_pos + 1..].iter().position(|&v| v == wanted) {
            let swap_pos = target_pos + 1 + found;
            tabs.swap(indices[target_pos], indices[swap_pos]);
            current.swap(target_pos, swap_pos);
        }
    }
}

/// T06: Live non-blank, non-asleep tabs in `space_id`.
pub fn space_reload_targets(tabs: &[Tab], space_id: &str) -> Vec<TabId> {
    tabs.iter()
        .filter(|t| t.space_id == space_id && !t.url.trim().is_empty() && !t.asleep)
        .map(|t| t.id)
        .collect()
}

/// T07: Loading tabs in `space_id`.
pub fn space_stop_targets(tabs: &[Tab], space_id: &str) -> Vec<TabId> {
    tabs.iter()
        .filter(|t| t.space_id == space_id && t.loading)
        .map(|t| t.id)
        .collect()
}

/// T08: All other tabs in `keep`'s space.
pub fn other_space_tab_ids(tabs: &[Tab], keep: TabId) -> Vec<TabId> {
    let Some(space_id) = tabs.iter().find(|t| t.id == keep).map(|t| t.space_id.as_str()) else {
        return Vec::new();
    };
    tabs.iter()
        .filter(|t| t.space_id == space_id && t.id != keep)
        .map(|t| t.id)
        .collect()
}

/// T10 / T11 / T12 / T13: Pure manual sleep eligibility (caller additionally
/// checks live WebView audio/capture state).
pub fn tab_eligible_for_manual_sleep(tab: &Tab, active: TabId, keep_awake_hosts: &[String]) -> bool {
    tab.id != active
        && !tab.asleep
        && !tab.loading
        && !tab.url.trim().is_empty()
        && !tab.keep_awake
        && !is_host_kept_awake(&tab.url, keep_awake_hosts)
}

/// T17 / T18 / T34: Visual-order regular HTTP(S) tabs in `space_id`, plus the
/// number of private tabs in that space that were excluded.
pub fn space_regular_web_pages(tabs: &[Tab], space_id: &str) -> (Vec<(String, String)>, usize) {
    let mut pages = Vec::new();
    let mut skipped_private = 0usize;
    for pinned_pass in [true, false] {
        for tab in tabs.iter().filter(|t| t.space_id == space_id && t.pinned == pinned_pass) {
            if tab.private {
                if !tab.url.trim().is_empty() {
                    skipped_private += 1;
                }
                continue;
            }
            if is_web_url(&tab.url) {
                let clean_url = strip_url_credentials(&tab.url);
                let title = if tab.title.trim().is_empty() {
                    crate::shield::host_of(&clean_url)
                } else {
                    tab.title.trim().to_string()
                };
                pages.push((title, clean_url));
            }
        }
    }
    (pages, skipped_private)
}

/// T40: Compute insertion index in `tabs` for a new tab in `target_space`.
pub fn new_tab_insert_index(
    tabs: &[Tab],
    anchor_id: TabId,
    target_space: &str,
    placement: NewTabPlacement,
) -> usize {
    match placement {
        NewTabPlacement::Adjacent => tabs
            .iter()
            .position(|t| t.id == anchor_id)
            .filter(|&p| tabs[p].space_id == target_space)
            .map(|p| p + 1)
            .unwrap_or_else(|| {
                tabs.iter()
                    .rposition(|t| t.space_id == target_space)
                    .map(|p| p + 1)
                    .unwrap_or(tabs.len())
            }),
        NewTabPlacement::End => tabs
            .iter()
            .rposition(|t| t.space_id == target_space)
            .map(|p| p + 1)
            .unwrap_or(tabs.len()),
    }
}

/// T42: Pick which tab in `closed_space` should activate after closing the tab
/// that occupied `closed_visual_pos` in `visual_before`.
pub fn choose_next_active_after_close(
    closed_visual_pos: usize,
    space_after: &[TabId],
    tabs_after: &[Tab],
    mode: CloseTabFocus,
) -> Option<TabId> {
    if space_after.is_empty() {
        return None;
    }
    match mode {
        CloseTabFocus::Right => space_after
            .get(closed_visual_pos.min(space_after.len() - 1))
            .copied(),
        CloseTabFocus::Left => space_after
            .get(closed_visual_pos.saturating_sub(1).min(space_after.len() - 1))
            .copied(),
        CloseTabFocus::Recent => {
            let mut best: Option<(u64, TabId)> = None;
            for &tid in space_after {
                let rank = tabs_after
                    .iter()
                    .find(|t| t.id == tid)
                    .map(|t| t.last_activated)
                    .unwrap_or(0);
                if best.is_none_or(|(best_rank, _)| rank > best_rank) {
                    best = Some((rank, tid));
                }
            }
            best.map(|(_, id)| id)
                .or_else(|| space_after.get(closed_visual_pos.min(space_after.len() - 1)).copied())
        }
    }
}

/// T24: Plan duplicating all regular (non-private) tabs from `source_space_id`
/// into a new space, enforcing `TAB_LIMIT` before mutating state.
pub fn plan_space_duplicate(
    tabs: &[Tab],
    source_space_id: &str,
    tab_limit: usize,
) -> Result<Vec<TabRecord>, &'static str> {
    let mut records = Vec::new();
    for pinned_pass in [true, false] {
        for tab in tabs
            .iter()
            .filter(|t| t.space_id == source_space_id && t.pinned == pinned_pass && !t.private)
        {
            records.push(TabRecord {
                url: strip_url_credentials(&tab.url),
                title: tab.title.clone(),
                pinned: tab.pinned,
                active: false,
                space_id: String::new(),
                space_active: false,
                muted: tab.muted,
                keep_awake: tab.keep_awake,
            });
        }
    }
    if records.is_empty() {
        return Err("No regular tabs in this space to duplicate");
    }
    if tabs.len().saturating_add(records.len()) > tab_limit {
        return Err("Duplicating this space would exceed the 40-tab limit");
    }
    Ok(records)
}

// ---- Recently closed batches (T20) --------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedBatch {
    pub id: u64,
    pub space_id: String,
    pub label: String,
    pub tabs: Vec<ClosedTab>,
}

/// T27 / T28: cycle only among spaces that actually have a tab to select.
/// Returns the target space and tab, never creating a tab and never choosing
/// an empty space. Stable across spaces with zero tabs.
pub fn space_cycle_target(
    spaces: &[crate::spaces::Space],
    active_space: &str,
    tabs: &[Tab],
    delta: isize,
) -> Option<(String, TabId)> {
    if spaces.len() < 2 || delta == 0 {
        return None;
    }
    let mut eligible: Vec<(&str, TabId)> = Vec::new();
    for space in spaces {
        let preferred = tabs
            .iter()
            .find(|t| t.space_id == space.id)
            .map(|t| t.id);
        if let Some(id) = preferred {
            eligible.push((&space.id, id));
        }
    }
    if eligible.len() < 2 {
        return None;
    }
    let current = eligible
        .iter()
        .position(|(space, _)| *space == active_space)
        .unwrap_or(0);
    let len = eligible.len();
    let next = if delta > 0 {
        (current + 1) % len
    } else {
        (current + len - 1) % len
    };
    let (space, id) = eligible[next];
    Some((space.to_string(), id))
}

/// Return the last `limit` closed tabs not already represented by a batch.
/// Batch restore consumes only its own individual entries, so restoring one
/// row never leaves a second copy of the same tab in Recently Closed.
pub fn retain_unbatched_closed_tabs(
    closed_tabs: Vec<ClosedTab>,
    batches: &[ClosedBatch],
    limit: usize,
) -> Vec<ClosedTab> {
    let mut batched: HashSet<(String, String)> = HashSet::new();
    for batch in batches {
        for tab in &batch.tabs {
            batched.insert((tab.space_id.clone(), tab.url.clone()));
        }
    }
    closed_tabs
        .into_iter()
        .filter(|tab| !batched.contains(&(tab.space_id.clone(), tab.url.clone())))
        .rev()
        .take(limit)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect()
}

/// Decide whether a batch restore can complete without losing tabs. If it
/// cannot, callers leave the batch untouched and tell the user to free slots.
pub fn batch_restore_fits(batch: &ClosedBatch, tab_count: usize, tab_limit: usize) -> usize {
    tab_limit.saturating_sub(tab_count).min(batch.tabs.len())
}

/// Zoom keys are host-based, but private tabs never consult or write the
/// regular per-host override map.
pub fn zoom_override<'a>(overrides: &'a BTreeMap<String, f64>, url: &str, private: bool) -> Option<f64> {
    if private {
        None
    } else {
        overrides.get(&crate::app::zoom_key(url)).copied()
    }
}

/// A WebKit MHTML save is only useful when the file has a MIME header and the
/// page's URL is present in its headers. Keep validation tiny and case-agnostic.
pub fn valid_mhtml(bytes: &[u8], expected_url: &str) -> bool {
    if bytes.len() < 64 {
        return false;
    }
    let head_end = bytes.len().min(8192);
    let head = String::from_utf8_lossy(&bytes[..head_end]).to_ascii_lowercase();
    let url = expected_url.to_ascii_lowercase();
    head.contains("mime-version: 1.0")
        && (head.contains("multipart/related") || head.contains("multipart/alternative"))
        && (url.is_empty() || head.contains(&url))
}

pub fn push_closed_batch(batches: &mut Vec<ClosedBatch>, space_id: &str, label: &str, tabs: Vec<ClosedTab>) {
    if tabs.len() < 2 {
        return;
    }
    if batches.len() >= MAX_CLOSED_BATCHES {
        batches.remove(0);
    }
    batches.push(ClosedBatch {
        id: crate::app::next_closed_id(),
        space_id: space_id.to_string(),
        label: label.to_string(),
        tabs,
    });
}

// ---- Versioned, bounded Named Sessions (T31, T32, T33, T36, T37) --------

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SavedSessionTab {
    pub url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct NamedSession {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub tabs: Vec<SavedSessionTab>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct NamedSessionStore {
    pub version: u32,
    pub next_seq: u64,
    pub sessions: Vec<NamedSession>,
}

impl Default for NamedSessionStore {
    fn default() -> Self {
        Self {
            version: NAMED_SESSIONS_VERSION,
            next_seq: 1,
            sessions: Vec::new(),
        }
    }
}

pub fn sanitize_session_name(raw: &str) -> Result<String, &'static str> {
    let collapsed: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let bounded: String = collapsed.chars().take(MAX_SESSION_NAME_LEN).collect();
    let trimmed = bounded.trim();
    if trimmed.is_empty() {
        return Err("Enter a session name");
    }
    Ok(trimmed.to_string())
}

fn session_suffix_number(id: &str) -> Option<u64> {
    id.strip_prefix("session-")?.parse::<u64>().ok()
}

impl NamedSessionStore {
    pub fn sanitised(self) -> Self {
        // Reject unsupported future schema versions safely by falling back.
        if self.version == 0 || self.version > NAMED_SESSIONS_VERSION {
            return Self::default();
        }
        let mut seen_ids = HashSet::new();
        let mut seen_names = HashSet::new();
        let mut max_seq = self.next_seq.saturating_sub(1);
        let mut sessions = Vec::new();
        for item in self.sessions {
            if sessions.len() >= MAX_NAMED_SESSIONS {
                break;
            }
            let id = item.id.trim().to_string();
            if id.is_empty() || !seen_ids.insert(id.clone()) {
                continue;
            }
            if let Some(n) = session_suffix_number(&id) {
                max_seq = max_seq.max(n);
            }
            let Ok(name) = sanitize_session_name(&item.name) else {
                continue;
            };
            if !seen_names.insert(name.to_lowercase()) {
                continue;
            }
            let mut clean_tabs = Vec::new();
            for t in item.tabs {
                if clean_tabs.len() >= TAB_LIMIT {
                    break;
                }
                if !is_web_url(&t.url) {
                    continue;
                }
                let url = strip_url_credentials(&t.url);
                let title = t.title.trim().chars().take(200).collect();
                clean_tabs.push(SavedSessionTab {
                    url,
                    title,
                    pinned: t.pinned,
                });
            }
            if clean_tabs.is_empty() {
                continue;
            }
            sessions.push(NamedSession {
                id,
                name,
                created_at: item.created_at,
                tabs: clean_tabs,
            });
        }
        Self {
            version: NAMED_SESSIONS_VERSION,
            next_seq: max_seq.saturating_add(1).max(1),
            sessions,
        }
    }

    pub fn get(&self, id: &str) -> Option<&NamedSession> {
        self.sessions.iter().find(|s| s.id == id)
    }

    /// Collect regular (non-private) web tabs from `space_id` and store as a
    /// named session (T31).
    pub fn save_from_space(
        &mut self,
        raw_name: &str,
        tabs: &[Tab],
        space_id: &str,
        now: u64,
    ) -> Result<(NamedSession, usize), &'static str> {
        if self.sessions.len() >= MAX_NAMED_SESSIONS {
            return Err("Saved session limit reached (24)");
        }
        let name = sanitize_session_name(raw_name)?;
        if self.sessions.iter().any(|s| s.name.to_lowercase() == name.to_lowercase()) {
            return Err("A saved session with that name already exists");
        }
        let mut saved = Vec::new();
        let mut skipped_private = 0usize;
        for pinned_pass in [true, false] {
            for tab in tabs.iter().filter(|t| t.space_id == space_id && t.pinned == pinned_pass) {
                if tab.private {
                    if !tab.url.trim().is_empty() {
                        skipped_private += 1;
                    }
                    continue;
                }
                if !is_web_url(&tab.url) || saved.len() >= TAB_LIMIT {
                    continue;
                }
                saved.push(SavedSessionTab {
                    url: strip_url_credentials(&tab.url),
                    title: tab.title.trim().chars().take(200).collect(),
                    pinned: tab.pinned,
                });
            }
        }
        if saved.is_empty() {
            return Err("No regular web pages in this space to save");
        }
        let mut seq = self.next_seq.max(1);
        let id = loop {
            let candidate = format!("session-{seq}");
            seq = seq.checked_add(1).unwrap_or(1);
            if !self.sessions.iter().any(|s| s.id == candidate) {
                break candidate;
            }
        };
        self.next_seq = seq;
        let session = NamedSession {
            id,
            name,
            created_at: now,
            tabs: saved,
        };
        self.sessions.insert(0, session.clone());
        Ok((session, skipped_private))
    }

    /// T36: Rename a saved session with collision check.
    pub fn rename(&mut self, id: &str, raw_name: &str) -> Result<String, &'static str> {
        let name = sanitize_session_name(raw_name)?;
        if !self.sessions.iter().any(|s| s.id == id) {
            return Err("Saved session not found");
        }
        if self
            .sessions
            .iter()
            .any(|s| s.id != id && s.name.to_lowercase() == name.to_lowercase())
        {
            return Err("A saved session with that name already exists");
        }
        if let Some(slot) = self.sessions.iter_mut().find(|s| s.id == id) {
            slot.name = name.clone();
        }
        Ok(name)
    }

    /// T37: Delete a saved session, returning `(index, removed)` for undo.
    pub fn delete(&mut self, id: &str) -> Result<(usize, NamedSession), &'static str> {
        let Some(pos) = self.sessions.iter().position(|s| s.id == id) else {
            return Err("Saved session not found");
        };
        Ok((pos, self.sessions.remove(pos)))
    }

    pub fn restore_deleted(&mut self, index: usize, session: NamedSession) -> bool {
        if self.sessions.len() >= MAX_NAMED_SESSIONS
            || self.sessions.iter().any(|s| s.id == session.id || s.name.to_lowercase() == session.name.to_lowercase())
        {
            return false;
        }
        let at = index.min(self.sessions.len());
        self.sessions.insert(at, session);
        true
    }

    /// T32: Validate capacity before opening a saved session alongside current tabs.
    pub fn check_open_alongside(&self, id: &str, current_tab_count: usize, tab_limit: usize) -> Result<NamedSession, String> {
        let session = self.get(id).cloned().ok_or_else(|| "Saved session not found".to_string())?;
        let needed = session.tabs.len();
        if current_tab_count.saturating_add(needed) > tab_limit {
            let free = tab_limit.saturating_sub(current_tab_count);
            return Err(format!(
                "Cannot open {} tabs: only {} of {} tab slots are free",
                needed, free, tab_limit
            ));
        }
        Ok(session)
    }

    /// T33: Validate capacity before replacing a space's regular tabs with a saved session.
    pub fn check_replace_space(
        &self,
        id: &str,
        tabs: &[Tab],
        space_id: &str,
        tab_limit: usize,
    ) -> Result<(NamedSession, Vec<TabId>), String> {
        let session = self.get(id).cloned().ok_or_else(|| "Saved session not found".to_string())?;
        let replaced_ids: Vec<TabId> = tabs
            .iter()
            .filter(|t| t.space_id == space_id && !t.private)
            .map(|t| t.id)
            .collect();
        let remaining_after_remove = tabs.len().saturating_sub(replaced_ids.len());
        if remaining_after_remove.saturating_add(session.tabs.len()) > tab_limit {
            return Err(format!(
                "Replacing this space with {} tabs would exceed the {}-tab limit",
                session.tabs.len(),
                tab_limit
            ));
        }
        Ok((session, replaced_ids))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk_tab(id: TabId, space: &str, url: &str, title: &str, pinned: bool, private: bool) -> Tab {
        let mut t = Tab::new(id);
        t.space_id = space.to_string();
        t.url = url.to_string();
        t.title = title.to_string();
        t.pinned = pinned;
        t.private = private;
        t
    }

    #[test]
    fn t01_close_left_preserves_pins_and_other_spaces() {
        let tabs = vec![
            mk_tab(1, "default", "https://pin.test/", "Pin", true, false),
            mk_tab(2, "space-1", "https://other.test/", "Other", false, false),
            mk_tab(3, "default", "https://a.test/", "A", false, false),
            mk_tab(4, "default", "https://b.test/", "B", false, true),
            mk_tab(5, "default", "https://anchor.test/", "Anchor", false, false),
            mk_tab(6, "default", "https://right.test/", "Right", false, false),
        ];
        assert_eq!(tabs_to_left(&tabs, 5), vec![3, 4]);
        assert!(tabs_to_left(&tabs, 1).is_empty(), "pinned anchor closes nothing");
        assert!(tabs_to_left(&tabs, 2).is_empty(), "other space is isolated");
    }

    #[test]
    fn t02_close_duplicates_keeps_chosen_and_separates_private_and_regular() {
        let tabs = vec![
            mk_tab(1, "default", "https://a.test/", "A pin", true, false),
            mk_tab(2, "default", "https://a.test", "A dup", false, false),
            mk_tab(3, "default", "https://a.test/", "A private", false, true),
            mk_tab(4, "default", "https://b.test/page", "B1", false, false),
            mk_tab(5, "default", "https://b.test/page", "B2 chosen", false, false),
            mk_tab(6, "space-1", "https://b.test/page", "Other space", false, false),
            mk_tab(7, "default", "https://a.test/", "A private dup", false, true),
        ];
        // Choosing tab 5 keeps tab 5 (closing 4), keeps pinned 1 (closing 2), keeps first private 3 (closing 7), leaves space-1 untouched.
        assert_eq!(duplicate_tabs_to_close(&tabs, 5), vec![2, 4, 7]);
    }

    #[test]
    fn t03_close_same_site_matches_exact_origin_and_privacy() {
        let tabs = vec![
            mk_tab(1, "default", "https://example.com/a", "Pin", true, false),
            mk_tab(2, "default", "https://example.com/b", "Anchor", false, false),
            mk_tab(3, "default", "https://example.com/c", "Same", false, false),
            mk_tab(4, "default", "https://example.com:8443/d", "Different port", false, false),
            mk_tab(5, "default", "http://example.com/e", "Different scheme", false, false),
            mk_tab(6, "default", "https://example.com/f", "Private same host", false, true),
            mk_tab(7, "space-1", "https://example.com/g", "Other space", false, false),
        ];
        assert_eq!(other_site_tabs_to_close(&tabs, 2), vec![3]);
    }

    #[test]
    fn t04_t05_move_to_start_and_end_stay_within_space_and_partition() {
        let mut tabs = vec![
            mk_tab(1, "default", "https://p1.test/", "P1", true, false),
            mk_tab(2, "default", "https://p2.test/", "P2", true, false),
            mk_tab(3, "space-1", "https://s.test/", "S", false, false),
            mk_tab(4, "default", "https://a.test/", "A", false, false),
            mk_tab(5, "default", "https://b.test/", "B", false, false),
            mk_tab(6, "default", "https://c.test/", "C", false, false),
        ];
        assert!(move_tab_to_edge(&mut tabs, 6, true));
        assert_eq!(tabs.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2, 3, 6, 4, 5]);
        assert!(!move_tab_to_edge(&mut tabs, 6, true));
        assert!(move_tab_to_edge(&mut tabs, 6, false));
        assert_eq!(tabs.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2, 3, 4, 5, 6]);
        assert!(move_tab_to_edge(&mut tabs, 2, true));
        assert_eq!(tabs.iter().map(|t| t.id).collect::<Vec<_>>(), vec![2, 1, 3, 4, 5, 6]);
    }

    #[test]
    fn t21_t22_sort_by_title_and_group_by_site_are_stable_and_scoped() {
        let mut tabs = vec![
            mk_tab(1, "default", "https://z.test/", "Zebra pin", true, false),
            mk_tab(2, "space-1", "https://a.test/", "Other space", false, false),
            mk_tab(3, "default", "https://beta.test/1", "Charlie", false, false),
            mk_tab(4, "default", "https://alpha.test/1", "alpha", false, false),
            mk_tab(5, "default", "https://beta.test/2", "Bravo", false, false),
            mk_tab(6, "default", "", "", false, false),
        ];
        assert!(sort_unpinned_tabs_by_title(&mut tabs, "default"));
        assert_eq!(tabs.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2, 4, 5, 3, 6]);

        // Reset to test site grouping preserving first-seen origin order (beta before alpha).
        let mut tabs2 = vec![
            mk_tab(1, "default", "https://z.test/", "Pin", true, false),
            mk_tab(2, "default", "https://beta.test/1", "B1", false, false),
            mk_tab(3, "default", "https://alpha.test/1", "A1", false, false),
            mk_tab(4, "default", "", "", false, false),
            mk_tab(5, "default", "https://beta.test/2", "B2", false, false),
            mk_tab(6, "space-1", "https://beta.test/3", "Other", false, false),
        ];
        assert!(group_unpinned_tabs_by_site(&mut tabs2, "default"));
        assert_eq!(tabs2.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1, 2, 5, 3, 4, 6]);
    }

    #[test]
    fn t14_t15_t16_t45_url_formatting_and_tracking_stripping() {
        let dirty = "https://user:secret@example.com/watch?v=123&utm_source=x&fbclid=abc&t=42#top";
        assert_eq!(
            strip_url_credentials(dirty),
            "https://example.com/watch?v=123&utm_source=x&fbclid=abc&t=42#top"
        );
        assert_eq!(
            strip_tracking_params(dirty),
            "https://example.com/watch?v=123&t=42#top"
        );
        assert!(has_tracking_params(dirty));
        assert!(!has_tracking_params("https://example.com/watch?v=123#top"));
        assert_eq!(
            format_title_and_url("Hello\nWorld", "https://u:p@a.test/p"),
            "Hello World\nhttps://a.test/p"
        );
        assert_eq!(
            format_markdown_link("Rust [1.2] *Guide*", "https://u:p@a.test/path_(1)"),
            r"[Rust \[1.2\] \*Guide\*](https://a.test/path_%281%29)"
        );
    }

    #[test]
    fn t35_t47_t48_url_and_list_validation_rejects_unsafe_schemes_and_caps() {
        assert_eq!(
            parse_explicit_web_url("  HTTPS://Example.COM/a?b=1  "),
            Some("https://Example.COM/a?b=1".to_string())
        );
        for bad in [
            "",
            "hello world",
            "javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,hi",
            "https://bad host/x",
            "https://nodot",
        ] {
            assert_eq!(parse_explicit_web_url(bad), None, "should reject {bad:?}");
        }

        let input = "# comment\nhttps://a.test/1\nhttps://a.test/1\njavascript:alert(1)\nhttps://b.test/2\nhttps://c.test/3\n";
        let existing = vec!["https://b.test/2".to_string()];
        let plan = parse_address_list(input, &existing, 1);
        assert_eq!(plan.urls, vec!["https://a.test/1"]);
        assert_eq!(plan.skipped_invalid, 1);
        assert_eq!(plan.skipped_duplicates, 2);
        assert_eq!(plan.truncated_by_limit, 1);
    }

    #[test]
    fn t13_keep_awake_host_rules_are_exact_and_bounded() {
        assert_eq!(normalize_host_rule("https://Docs.RS:443/crate"), Some("docs.rs".into()));
        assert_eq!(normalize_host_rule("example.org"), Some("example.org".into()));
        assert_eq!(normalize_host_rule("*.example.org"), None);
        assert_eq!(normalize_host_rule("javascript:alert(1)"), None);
        assert_eq!(normalize_host_rule("example.org/path"), None);
        let many: Vec<String> = (0..100).map(|i| format!("host{i}.example.com")).collect();
        let clean = sanitize_keep_awake_hosts(many);
        assert_eq!(clean.len(), MAX_KEEP_AWAKE_HOSTS);
    }

    #[test]
    fn t31_t32_t33_t36_t37_named_sessions_exclude_private_and_enforce_bounds() {
        let tabs = vec![
            mk_tab(1, "default", "https://a.test/", "A", true, false),
            mk_tab(2, "default", "https://secret.test/", "Secret", false, true),
            mk_tab(3, "default", "https://b.test/", "B", false, false),
            mk_tab(4, "space-1", "https://other.test/", "Other", false, false),
        ];
        let mut store = NamedSessionStore::default();
        let (saved, skipped) = store.save_from_space("  Morning  ", &tabs, "default", 1000).unwrap();
        assert_eq!(saved.name, "Morning");
        assert_eq!(skipped, 1);
        assert_eq!(saved.tabs.len(), 2);
        assert!(saved.tabs.iter().all(|t| !t.url.contains("secret.test")));

        // Duplicate name rejected case-insensitively.
        assert!(store.save_from_space("morning", &tabs, "default", 1001).is_err());

        // Rename & collision check.
        assert_eq!(store.rename(&saved.id, "Deep Work").unwrap(), "Deep Work");
        let (second, _) = store.save_from_space("Evening", &tabs, "default", 1002).unwrap();
        assert!(store.rename(&second.id, "deep work").is_err());

        // Capacity checks for alongside vs replace.
        assert!(store.check_open_alongside(&saved.id, TAB_LIMIT - 1, TAB_LIMIT).is_err());
        assert!(store.check_open_alongside(&saved.id, TAB_LIMIT - 2, TAB_LIMIT).is_ok());

        let (rep_session, rep_ids) = store.check_replace_space(&saved.id, &tabs, "default", TAB_LIMIT).unwrap();
        assert_eq!(rep_session.id, saved.id);
        assert_eq!(rep_ids, vec![1, 3], "private tab 2 and other-space tab 4 are not replaced");

        // Delete & undo restore.
        let (idx, deleted) = store.delete(&saved.id).unwrap();
        assert!(store.get(&saved.id).is_none());
        assert!(store.restore_deleted(idx, deleted));
        assert!(store.get(&saved.id).is_some());

        // Future version or bad schemes sanitise safely.
        let bad_version = NamedSessionStore {
            version: 99,
            next_seq: 10,
            sessions: store.sessions.clone(),
        }
        .sanitised();
        assert_eq!(bad_version.version, NAMED_SESSIONS_VERSION);
        assert!(bad_version.sessions.is_empty());
    }

    #[test]
    fn t24_t40_t42_space_dup_placement_and_close_focus_modes() {
        let mut t1 = mk_tab(1, "default", "https://a.test/", "A", false, false);
        t1.last_activated = 10;
        let mut t2 = mk_tab(2, "default", "https://b.test/", "B", false, false);
        t2.last_activated = 30;
        let mut t3 = mk_tab(3, "default", "https://c.test/", "C", false, false);
        t3.last_activated = 20;
        let tabs = vec![t1, t2, t3];

        assert_eq!(new_tab_insert_index(&tabs, 1, "default", NewTabPlacement::Adjacent), 1);
        assert_eq!(new_tab_insert_index(&tabs, 1, "default", NewTabPlacement::End), 3);

        // Suppose tab 2 (at visual pos 1) was closed, leaving [1, 3].
        let remaining_tabs = vec![tabs[0].clone_meta(), tabs[2].clone_meta()];
        assert_eq!(choose_next_active_after_close(1, &[1, 3], &remaining_tabs, CloseTabFocus::Right), Some(3));
        assert_eq!(choose_next_active_after_close(1, &[1, 3], &remaining_tabs, CloseTabFocus::Left), Some(1));
        assert_eq!(choose_next_active_after_close(1, &[1, 3], &remaining_tabs, CloseTabFocus::Recent), Some(3));

        // Duplicate space checks cap before creating.
        assert!(plan_space_duplicate(&tabs, "default", 5).is_err());
        assert_eq!(plan_space_duplicate(&tabs, "default", 6).unwrap().len(), 3);
    }

    #[test]
    fn t06_t07_t08_t10_t17_t20_t34_t38_t49_helpers_and_atomic_export() {
        let mut t1 = mk_tab(1, "default", "https://u:p@a.test/one", "One", true, false);
        t1.loading = true;
        let mut t2 = mk_tab(2, "default", "https://b.test/two", "Two", false, false);
        t2.asleep = true;
        let mut t3 = mk_tab(3, "default", "https://c.test/three", "Three", false, true);
        t3.keep_awake = true;
        let t4 = mk_tab(4, "default", "", "", false, false);
        let t5 = mk_tab(5, "space-1", "https://other.test/", "Other", false, false);
        let t6 = mk_tab(6, "default", "https://docs.rs/gtk", "GTK", false, false);
        let tabs = vec![t1, t2, t3, t4, t5, t6];

        // T06: skips asleep, blank, and other spaces.
        assert_eq!(space_reload_targets(&tabs, "default"), vec![1, 3, 6]);
        // T07: stops only loading tabs in active space.
        assert_eq!(space_stop_targets(&tabs, "default"), vec![1]);
        // T08: other tabs in space excludes keep tab and other spaces.
        assert_eq!(other_space_tab_ids(&tabs, 1), vec![2, 3, 4, 6]);
        // T10/T11/T12/T13: manual sleep respects active, asleep, loading, blank, keep_awake, and host rules.
        let hosts = vec!["docs.rs".to_string()];
        assert!(!tab_eligible_for_manual_sleep(&tabs[0], 99, &hosts), "loading tab cannot sleep");
        assert!(!tab_eligible_for_manual_sleep(&tabs[1], 99, &hosts), "already asleep");
        assert!(!tab_eligible_for_manual_sleep(&tabs[2], 99, &hosts), "keep_awake tab cannot sleep");
        assert!(!tab_eligible_for_manual_sleep(&tabs[3], 99, &hosts), "blank tab cannot sleep");
        assert!(!tab_eligible_for_manual_sleep(&tabs[5], 99, &hosts), "keep_awake host cannot sleep");
        assert!(tab_eligible_for_manual_sleep(&tabs[4], 99, &hosts), "eligible background tab");
        assert!(!tab_eligible_for_manual_sleep(&tabs[4], 5, &hosts), "active tab cannot sleep");

        // T17/T18/T34: visual order regular web pages exclude private, blank, other spaces, and strip credentials.
        let (pages, skipped_private) = space_regular_web_pages(&tabs, "default");
        assert_eq!(skipped_private, 1);
        assert_eq!(
            pages,
            vec![
                ("One".to_string(), "https://a.test/one".to_string()),
                ("Two".to_string(), "https://b.test/two".to_string()),
                ("GTK".to_string(), "https://docs.rs/gtk".to_string()),
            ]
        );

        // T20: closed batches ignore <2 tabs and cap at MAX_CLOSED_BATCHES.
        let mut batches = Vec::new();
        let sample = ClosedTab {
            id: crate::app::next_closed_id(),
            space_id: "default".into(),
            url: "https://a.test/".into(),
            title: "A".into(),
            pinned: false,
            index: 0,
            ordinal: 0,
        };
        push_closed_batch(&mut batches, "default", "Single", vec![sample.clone()]);
        assert!(batches.is_empty());
        for i in 0..(MAX_CLOSED_BATCHES + 3) {
            push_closed_batch(&mut batches, "default", &format!("Batch {i}"), vec![sample.clone(), sample.clone()]);
        }
        assert_eq!(batches.len(), MAX_CLOSED_BATCHES);
        assert_eq!(batches[0].label, "Batch 3");

        // T38: Home URL validation.
        assert_eq!(validate_home_url(""), Ok(String::new()));
        assert_eq!(validate_home_url("https://example.org/"), Ok("https://example.org/".to_string()));
        assert!(validate_home_url("javascript:alert(1)").is_err());

        // T49/T50: Safe offline filenames.
        assert_eq!(sanitize_offline_filename("Rust / GTK: Guide?", "https://a.test/", "mhtml"), "Rust-GTK-Guide.mhtml");
        assert_eq!(sanitize_offline_filename("   ", "https://docs.rs/gtk", ".txt"), "docs-rs.txt");

        // T34: Atomic export write & overwrite.
        let dir = std::env::temp_dir().join(format!("rill_export_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("urls.txt");
        crate::files::write_atomic_export(&path, b"https://a.test/\n").expect("first write");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "https://a.test/\n");
        crate::files::write_atomic_export(&path, b"https://b.test/\n").expect("atomic overwrite");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "https://b.test/\n");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn repair_regressions_space_cycle_batch_restore_zoom_mhtml_and_credentials() {
        // P0-1: space_cycle_target skips empty spaces and never asks to spawn a blank tab.
        let mut spaces = crate::spaces::SpaceStore::default();
        let work = spaces.create("Work").unwrap();
        let empty = spaces.create("Empty").unwrap();
        let tabs = vec![
            mk_tab(1, "default", "https://a.test/", "A", false, false),
            mk_tab(2, &work.id, "https://b.test/", "B", false, false),
        ];
        assert_eq!(space_cycle_target(&spaces.spaces, "default", &tabs, 1), Some((work.id.clone(), 2)));
        assert_eq!(space_cycle_target(&spaces.spaces, &work.id, &tabs, 1), Some(("default".into(), 1)));
        assert_eq!(space_cycle_target(&spaces.spaces, &empty.id, &tabs, -1), Some((work.id.clone(), 2)));
        // Single populated space -> None.
        assert_eq!(space_cycle_target(&spaces.spaces, "default", &tabs[..1], 1), None);

        // P0-3 / M4: batch_restore_fits and retain_unbatched_closed_tabs prevent silent truncation and duplicates.
        let c1 = ClosedTab {
            id: crate::app::next_closed_id(),
            space_id: "default".into(),
            url: "https://one.test/".into(),
            title: "One".into(),
            pinned: false,
            index: 0,
            ordinal: 0,
        };
        let c2 = ClosedTab {
            id: crate::app::next_closed_id(),
            space_id: "default".into(),
            url: "https://two.test/".into(),
            title: "Two".into(),
            pinned: false,
            index: 1,
            ordinal: 1,
        };
        let c3 = ClosedTab {
            id: crate::app::next_closed_id(),
            space_id: "default".into(),
            url: "https://keep.test/".into(),
            title: "Keep".into(),
            pinned: false,
            index: 2,
            ordinal: 2,
        };
        let mut batches = Vec::new();
        push_closed_batch(&mut batches, "default", "Bulk", vec![c1.clone(), c2.clone()]);
        assert_eq!(batch_restore_fits(&batches[0], 39, 40), 1);
        assert_eq!(batch_restore_fits(&batches[0], 38, 40), 2);
        let retained = retain_unbatched_closed_tabs(vec![c1, c3.clone(), c2], &batches, 20);
        assert_eq!(retained.len(), 1);
        assert_eq!(retained[0].url, c3.url);

        // M2: plan_space_duplicate strips URL credentials and preserves muted flag.
        let mut cred_tab = mk_tab(10, "default", "https://user:secret@example.org/app", "App", true, false);
        cred_tab.muted = true;
        let dup = plan_space_duplicate(&[cred_tab], "default", 40).expect("fits");
        assert_eq!(dup.len(), 1);
        assert_eq!(dup[0].url, "https://example.org/app");
        assert!(dup[0].muted);

        // M8: zoom_override isolates private tabs from regular per-host zoom map.
        let mut overrides = BTreeMap::new();
        overrides.insert("example.org".to_string(), 1.5);
        assert_eq!(zoom_override(&overrides, "https://example.org/page", false), Some(1.5));
        assert_eq!(zoom_override(&overrides, "https://example.org/page", true), None);

        // M9: valid_mhtml accepts real MIME headers with URL and rejects truncated/invalid payloads.
        let mhtml = b"From: <Saved by WebKit>\r\nSubject: Example\r\nMIME-Version: 1.0\r\nContent-Type: multipart/related; boundary=\"b\"\r\nContent-Location: https://example.org/doc\r\n\r\nbody";
        assert!(valid_mhtml(mhtml, "https://example.org/doc"));
        assert!(!valid_mhtml(mhtml, "https://other.test/"));
        assert!(!valid_mhtml(b"tiny", "https://example.org/doc"));
        assert!(!valid_mhtml(b"<html><body>plain html without mime headers at all, padded past sixty four bytes</body></html>", "https://example.org/doc"));
    }
}
