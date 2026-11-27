// Hidden things: a per-site list of selectors, applied as a style sheet before
// the page draws, and a picker that adds to it.
use crate::app::{App, AppRef, TabId};
use javascriptcore::ValueExt;
use webkit2gtk::{
    UserContentInjectedFrames, UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebViewExt,
};

/// Isolated script world for the element picker so page JavaScript cannot spoof
/// or intercept `searchHide` messages or tamper with picker state.
pub(crate) const CURTAIN_WORLD: &str = "search-curtain";
pub fn selectors(app: &App, host: &str) -> Vec<String> {
    app.state
        .borrow()
        .hidden
        .get(host)
        .cloned()
        .unwrap_or_default()
}

/// Effective selectors for one tab on one host. A private tab checks its own
/// ephemeral per-tab overrides first (which may be empty after a private
/// `unhide_all`), leaving normal persisted rules untouched.
pub fn effective_selectors(app: &App, id: TabId, host: &str) -> Vec<String> {
    let private = app.state.borrow().tab(id).is_some_and(|t| t.private);
    if private {
        if let Some(list) = app
            .private_hidden
            .borrow()
            .get(&id)
            .and_then(|hosts| hosts.get(host))
        {
            return list.clone();
        }
    }
    if !app.prefs.borrow().curtain {
        return Vec::new();
    }
    selectors(app, host)
}

pub fn css_for(app: &App, host: &str) -> String {
    if !app.prefs.borrow().curtain {
        return String::new();
    }
    let list = selectors(app, host);
    css_from(&list)
}

/// One rule per selector rather than a comma list: a selector the page broke
/// then costs only itself, not every other thing hidden on the site, and each
/// one carries its own brackets so nothing can be smuggled past the rule.
pub(crate) fn css_from(list: &[String]) -> String {
    if list.is_empty() {
        return String::new();
    }
    list.iter()
        .filter_map(|raw| {
            sanitized(raw).map(|selector| format!("{selector} {{ display: none !important; }}"))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Normalizes a host for WebKit `UserContentURLPattern` matching.
///
/// WebKit match patterns use `<scheme>://<host><path>` with no port component
/// and match `<host>` against the URL's host across all ports. Rejecting empty
/// hosts and wildcard/separator characters guarantees no caller can smuggle a
/// cross-origin wildcard pattern into a stylesheet's allow list.
pub(crate) fn normalize_host(raw: &str) -> Option<String> {
    let trimmed = raw.trim().to_ascii_lowercase();
    if trimmed.is_empty() {
        return None;
    }
    let host = if trimmed.starts_with('[') {
        let end = trimmed.find(']')?;
        let after = &trimmed[end + 1..];
        let valid_after = after.is_empty()
            || (after.starts_with(':')
                && after.len() > 1
                && after[1..].chars().all(|c| c.is_ascii_digit()));
        if !valid_after {
            return None;
        }
        let inner = &trimmed[1..end];
        if inner.is_empty()
            || !inner.contains(':')
            || !inner.chars().all(|c| c.is_ascii_hexdigit() || c == ':' || c == '.')
        {
            return None;
        }
        &trimmed[..=end]
    } else {
        let mut parts = trimmed.split(':');
        let h = parts.next().unwrap_or("");
        if let Some(port) = parts.next() {
            if port.is_empty() || !port.chars().all(|c| c.is_ascii_digit()) || parts.next().is_some() {
                return None;
            }
        }
        h
    };
    if host.is_empty()
        || host.chars().any(|c| {
            c.is_whitespace()
                || c.is_control()
                || matches!(c, '*' | '/' | '?' | '#' | '@' | '\\' | ',' | ';')
        })
    {
        return None;
    }
    Some(host.to_string())
}

/// Strict per-host WebKit allow list patterns (`http://<host>/*`, `https://<host>/*`).
/// Returns empty when `host` is invalid so no unscoped wildcard sheet is ever built.
pub(crate) fn allow_list_for(host: &str) -> Vec<String> {
    let Some(host) = normalize_host(host) else {
        return Vec::new();
    };
    vec![format!("http://{host}/*"), format!("https://{host}/*")]
}

/// Pure `(manager_id, host)` sheet registry so multi-host scoping, private
/// manager isolation, and cleanup can be unit-tested without a GTK display.
#[cfg(test)]
pub(crate) struct ScopedRegistry<S> {
    managers: std::collections::HashMap<usize, std::collections::HashMap<String, S>>,
}

#[cfg(test)]
impl<S> ScopedRegistry<S> {
    pub(crate) fn new() -> Self {
        Self { managers: std::collections::HashMap::new() }
    }

    pub(crate) fn replace(&mut self, manager_id: usize, host: &str, sheet: Option<S>) -> Option<S> {
        let Some(host) = normalize_host(host) else {
            return None;
        };
        let previous = self
            .managers
            .get_mut(&manager_id)
            .and_then(|hosts| hosts.remove(&host));
        if let Some(sheet) = sheet {
            self.managers.entry(manager_id).or_default().insert(host, sheet);
        } else if self.managers.get(&manager_id).is_some_and(|hosts| hosts.is_empty()) {
            self.managers.remove(&manager_id);
        }
        previous
    }

    pub(crate) fn clear_manager(&mut self, manager_id: usize) -> Vec<S> {
        self.managers
            .remove(&manager_id)
            .map(|hosts| hosts.into_values().collect())
            .unwrap_or_default()
    }

    #[cfg(test)]
    pub(crate) fn get(&self, manager_id: usize, host: &str) -> Option<&S> {
        let host = normalize_host(host)?;
        self.managers.get(&manager_id)?.get(&host)
    }

    #[cfg(test)]
    pub(crate) fn manager_len(&self, manager_id: usize) -> usize {
        self.managers.get(&manager_id).map_or(0, |h| h.len())
    }
}

struct ManagerSheets {
    weak: glib::WeakRef<webkit2gtk::UserContentManager>,
    sheets: std::collections::HashMap<String, UserStyleSheet>,
}

// The curtain tracks every sheet it installs by `(manager, host)` so shared
// content managers never leak one host's selectors onto another host or
// clobber another live tab's sheet, and dark-page tone sheets remain untouched.
// Holding a weak reference to each manager also prunes closed private managers
// and prevents pointer-reuse collisions.
thread_local! {
    static ADDED: std::cell::RefCell<std::collections::HashMap<usize, ManagerSheets>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

/// Captures the exact tab, host, privacy class, navigation epoch, and
/// `UserContentManager` identity that armed element-hide mode, so delayed
/// `searchHide` callbacks never read a different active tab or cross-tab
/// manager.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HideOrigin {
    pub tab_id: TabId,
    pub host: String,
    pub private: bool,
    pub nav_epoch: u64,
    pub manager_key: Option<usize>,
}

/// Live snapshot of the originating tab at callback or delayed-application time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HideTargetSnapshot<'a> {
    pub private: bool,
    pub host: &'a str,
    pub nav_epoch: u64,
    pub manager_key: Option<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HideDecision {
    ApplyPrivate,
    ApplyNormal,
    Cancel,
}

/// Validates that a `searchHide` message arriving from `source_manager_key`
/// belongs to the armed `HideOrigin` and that the originating tab is still on
/// the same host, privacy class, navigation epoch, and manager.
pub(crate) fn validate_hide_source(
    hide_mode: bool,
    origin: Option<&HideOrigin>,
    source_manager_key: usize,
    snapshot: Option<HideTargetSnapshot<'_>>,
) -> Option<HideOrigin> {
    if !hide_mode {
        return None;
    }
    let origin = origin?;
    if origin.host.is_empty() || origin.manager_key != Some(source_manager_key) {
        return None;
    }
    match validate_hide_target(origin, snapshot) {
        HideDecision::ApplyPrivate | HideDecision::ApplyNormal => Some(origin.clone()),
        HideDecision::Cancel => None,
    }
}

/// Validates that the originating tab is still alive and unchanged at
/// delayed-application time (after the 10ms timeout).
pub(crate) fn validate_hide_target(
    origin: &HideOrigin,
    snapshot: Option<HideTargetSnapshot<'_>>,
) -> HideDecision {
    if origin.host.is_empty() {
        return HideDecision::Cancel;
    }
    let Some(snap) = snapshot else {
        return HideDecision::Cancel;
    };
    if snap.private != origin.private
        || snap.nav_epoch != origin.nav_epoch
        || snap.host != origin.host
    {
        return HideDecision::Cancel;
    }
    if origin.manager_key.is_some() && snap.manager_key != origin.manager_key {
        return HideDecision::Cancel;
    }
    if origin.private {
        HideDecision::ApplyPrivate
    } else {
        HideDecision::ApplyNormal
    }
}

pub(crate) fn manager_key(manager: &webkit2gtk::UserContentManager) -> usize {
    use glib::translate::ToGlibPtr;
    let raw: *mut webkit2gtk::ffi::WebKitUserContentManager = manager.to_glib_none().0;
    raw as usize
}

fn prune_dead(added: &mut std::collections::HashMap<usize, ManagerSheets>) {
    added.retain(|_, entry| entry.weak.upgrade().is_some());
}

fn replace_sheet(manager: &webkit2gtk::UserContentManager, host: &str, css: &str) {
    let Some(host) = normalize_host(host) else {
        return;
    };
    let key = manager_key(manager);
    let previous = ADDED.with(|added| {
        let mut map = added.borrow_mut();
        prune_dead(&mut map);
        // If a freed private manager's address was reused, drop the stale entry
        // instead of removing its sheets from the brand-new manager.
        let same_instance = map
            .get(&key)
            .is_some_and(|entry| entry.weak.upgrade().as_ref() == Some(manager));
        if !same_instance {
            map.remove(&key);
            return None;
        }
        let prev = map.get_mut(&key).and_then(|entry| entry.sheets.remove(&host));
        if map.get(&key).is_some_and(|entry| entry.sheets.is_empty()) {
            map.remove(&key);
        }
        prev
    });
    if let Some(previous) = previous {
        manager.remove_style_sheet(&previous);
    }
    if let Some(sheet) = sheet_for(&host, css) {
        manager.add_style_sheet(&sheet);
        ADDED.with(|added| {
            let mut map = added.borrow_mut();
            let entry = map.entry(key).or_insert_with(|| {
                let weak = glib::WeakRef::new();
                weak.set(Some(manager));
                ManagerSheets { weak, sheets: std::collections::HashMap::new() }
            });
            entry.sheets.insert(host, sheet);
        });
    }
}

/// Removes all curtain sheets tracked for one content manager (e.g. when the
/// curtain preference is turned off or a private view is retired).
pub fn clear_manager(manager: &webkit2gtk::UserContentManager) {
    let key = manager_key(manager);
    let removed = ADDED.with(|added| {
        let mut map = added.borrow_mut();
        prune_dead(&mut map);
        let same_instance = map
            .get(&key)
            .is_some_and(|entry| entry.weak.upgrade().as_ref() == Some(manager));
        if !same_instance {
            map.remove(&key);
            return Vec::new();
        }
        map.remove(&key)
            .map(|entry| entry.sheets.into_values().collect())
            .unwrap_or_default()
    });
    for sheet in removed {
        manager.remove_style_sheet(&sheet);
    }
}

/// Drops tracking state for a manager whose view is closing/rebuilding.
pub fn forget_manager(manager: &webkit2gtk::UserContentManager) {
    clear_manager(manager);
}

/// The style sheet the next document of the given manager will wear for `host`.
pub fn sync_manager(app: &AppRef, manager: &webkit2gtk::UserContentManager, host: &str) {
    if !app.prefs.borrow().curtain {
        clear_manager(manager);
        return;
    }
    let css = css_for(app, host);
    replace_sheet(manager, host, &css);
}

/// Syncs the curtain stylesheet for one tab on `manager` and `host`, honoring
/// private-tab ephemeral overrides without touching persisted rules.
pub fn sync_for_tab(app: &AppRef, id: TabId, manager: &webkit2gtk::UserContentManager, host: &str) {
    let private = app.state.borrow().tab(id).is_some_and(|t| t.private);
    if private {
        let list = effective_selectors(app, id, host);
        replace_sheet(manager, host, &css_from(&list));
        return;
    }
    sync_manager(app, manager, host);
}

/// Updates a private tab's dedicated manager for one host with an explicit
/// ephemeral selector list.
pub fn sync_private_sheet(
    manager: &webkit2gtk::UserContentManager,
    host: &str,
    selectors: &[String],
) {
    replace_sheet(manager, host, &css_from(selectors));
}

/// The style sheet the next document of the shared manager will wear for `host`.
pub fn sync(app: &AppRef, host: &str) {
    // Same key the view will be made with, or the sheet lands on the manager
    // the page does not use.
    let shielded = app.prefs.borrow().shield && !crate::shield::is_allowed(host);
    let manager = crate::web::shared_manager(app, shielded);
    sync_manager(app, &manager, host);
}

/// Propagates a persisted host rule change (such as normal `unhide_all`) across
/// every currently live shared content manager.
pub fn sync_shared_managers_for_host(app: &AppRef, host: &str) {
    for manager in crate::web::live_shared_managers() {
        sync_manager(app, &manager, host);
    }
}

/// Clears curtain stylesheets from every live shared content manager.
pub fn clear_all_shared_managers() {
    for manager in crate::web::live_shared_managers() {
        clear_manager(&manager);
    }
}

/// Synchronizes every live shared and private content manager plus live
/// immediate `<style>` elements after `prefs.curtain` changes.
/// - When `curtain` is OFF: clears all live shared managers, removes immediate
///   styles from all live normal views, and re-syncs private views (clearing
///   inherited persisted sheets/styles while preserving explicit per-tab
///   private overrides).
/// - When `curtain` is ON: re-applies per-tab curtain sheets to every live tab.
pub fn apply_curtain_pref(app: &AppRef) {
    let enabled = app.prefs.borrow().curtain;
    let tabs: Vec<(TabId, String, bool)> = app
        .state
        .borrow()
        .tabs
        .iter()
        .map(|t| (t.id, crate::shield::host_of(&t.url), t.private))
        .collect();
    if !enabled {
        clear_all_shared_managers();
        for (id, host, private) in tabs {
            let Some(view) = app.view(id) else { continue };
            if private {
                if let Some(manager) = view.user_content_manager() {
                    clear_manager(&manager);
                    if !host.is_empty() {
                        sync_for_tab(app, id, &manager, &host);
                    }
                }
                if host.is_empty() || effective_selectors(app, id, &host).is_empty() {
                    clear_immediate_styles(&view);
                }
            } else {
                clear_immediate_styles(&view);
            }
        }
    } else {
        for (id, _, _) in tabs {
            apply_for_tab(app, id);
        }
    }
}

/// The sheet the curtain wants on a manager for one host: always scoped by a
/// non-empty per-host allow list so rules never bleed across origins.
fn sheet_for(host: &str, css: &str) -> Option<UserStyleSheet> {
    if css.is_empty() {
        return None;
    }
    let allow = allow_list_for(host);
    if allow.is_empty() {
        return None;
    }
    let allow_refs: Vec<&str> = allow.iter().map(String::as_str).collect();
    Some(UserStyleSheet::new(
        css,
        UserContentInjectedFrames::AllFrames,
        UserStyleLevel::User,
        &allow_refs,
        &[],
    ))
}

pub fn apply_for_tab(app: &App, id: TabId) {
    let Some(app_ref) = crate::app::current() else { return };
    let (host, private) = app
        .state
        .borrow()
        .tab(id)
        .map(|t| (crate::shield::host_of(&t.url), t.private))
        .unwrap_or_default();
    if !host.is_empty() {
        if let Some(manager) = app.view(id).and_then(|v| v.user_content_manager()) {
            sync_for_tab(&app_ref, id, &manager, &host);
        } else if !private {
            sync(&app_ref, &host);
        }
    }
}
/// For a navigation WebKit starts itself: the style sheet has to be in place
/// before the document begins, so it is set from the address being asked for.
pub fn sync_for_url(app: &AppRef, url: &str) {
    let host = crate::shield::host_of(url);
    if !host.is_empty() {
        sync(app, &host);
    }
}
/// Something added while the page is up: hide it now rather than on the next
/// visit. The style element is looked up by its id and rewritten, never
/// appended twice, and its text is handed over as a JSON string so a selector
/// cannot end the script early.
pub fn apply_immediately(view: &webkit2gtk::WebView, app: &App, host: &str) {
    let list = selectors(app, host);
    let css = css_from(&list);
    if css.is_empty() {
        return;
    }
    let text = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into());
    let js = format!(
        "(() => {{ let s = document.getElementById('{id}'); if (!s) {{ s = document.createElement('style'); s.id = '{id}'; (document.head || document.documentElement).appendChild(s); }} s.textContent = {text}; }})();",
        id = STYLE_ID
    );
    crate::js::run_in_world(view, CURTAIN_WORLD, &js, |_| {});
}
/// Applies a private tab's ephemeral hidden selectors to the live document
/// inside `CURTAIN_WORLD` without persisting anything.
pub fn apply_immediate_private(view: &webkit2gtk::WebView, selectors: &[String]) {
    let css = css_from(selectors);
    if css.is_empty() {
        clear_immediate_styles(view);
        return;
    }
    let text = serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into());
    let js = format!(
        "(() => {{ let s = document.getElementById('__search_hidden_private'); if (!s) {{ s = document.createElement('style'); s.id = '__search_hidden_private'; (document.head || document.documentElement).appendChild(s); }} s.textContent = {text}; }})();"
    );
    crate::js::run_in_world(view, CURTAIN_WORLD, &js, |_| {});
}

/// Removes any immediately injected curtain style tags from the current page
/// (used when a private tab unhides elements without touching persisted rules).
pub fn clear_immediate_styles(view: &webkit2gtk::WebView) {
    let js = format!(
        "(() => {{ for (const id of ['{STYLE_ID}', '__search_hidden_private']) {{ const el = document.getElementById(id); if (el) el.remove(); }} }})();"
    );
    crate::js::run_in_world(view, CURTAIN_WORLD, &js, |_| {});
}

/// The id the injected style element wears, so the next navigation rewrites it
/// instead of adding a second one.
const STYLE_ID: &str = "__search_hidden";

/// A selector as it can safely sit inside a style element.
///
/// A selector is not ours: it is built from the page's own id and class names,
/// and the page can also post whatever it likes to the picker's handler. A
/// literal `</style>` inside an id ends the sheet and lets the rest be markup,
/// a quote closes the string the rule sits in, and a brace closes the rule. A
/// backslash is allowed only inside the selector, because the picker's own
/// `CSS.escape` output needs it (`#a\:b`); at the very end it would escape the
/// brace that follows.
fn sanitized(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_SELECTOR {
        return None;
    }
    if trimmed.contains("/*") || trimmed.contains("*/") {
        return None;
    }
    let chars: Vec<char> = trimmed.chars().collect();
    let mut out = String::with_capacity(trimmed.len());
    for (index, ch) in chars.iter().enumerate() {
        match ch {
            '<' | '"' | '\'' | '{' | '}' | ',' | '\n' | '\r' => return None,
            '\\' if index + 1 == chars.len() => return None,
            _ => out.push(*ch),
        }
    }
    if out.is_empty() {
        return None;
    }
    Some(out)
}

/// Long enough for the deepest path the picker builds, short enough that a
/// hostile id cannot put a megabyte in a style element.
const MAX_SELECTOR: usize = 400;

const PICK: &str = r#"
(() => {
  if (window.__searchHideOn) {
    window.__searchHideOn = false;
    const st = document.getElementById('__search_hide_style');
    if (st) st.remove();
    if (window.__searchHideLast) window.__searchHideLast.classList.remove('__search_hide_hover');
    return 'off';
  }
  window.__searchHideOn = true;
  const st = document.createElement('style');
  st.id = '__search_hide_style';
  st.textContent = '.__search_hide_hover{outline:2px solid #4a90ff !important;outline-offset:-2px !important;cursor:crosshair !important;}';
  document.documentElement.appendChild(st);
  let last = null;
  const onMove = (e) => {
    if (last) last.classList.remove('__search_hide_hover');
    last = e.target;
    window.__searchHideLast = last;
    if (last && last.nodeType === 1 && last !== document.documentElement) last.classList.add('__search_hide_hover');
  };
  const path = (el) => {
    if (el.id) return '#' + CSS.escape(el.id);
    const parts = [];
    let node = el;
    while (node && node.nodeType === 1 && parts.length < 4) {
      let part = node.tagName.toLowerCase();
      const cls = Array.from(node.classList).find((c) => !c.startsWith('__search_hide'));
      if (cls) part += '.' + CSS.escape(cls);
      const parent = node.parentElement;
      if (parent) {
        const same = Array.from(parent.children).filter((c) => c.tagName === node.tagName);
        if (same.length > 1) part += ':nth-of-type(' + (same.indexOf(node) + 1) + ')';
      }
      parts.unshift(part);
      node = parent;
    }
    return parts.join(' > ');
  };
  const onClick = (e) => {
    e.preventDefault();
    e.stopPropagation();
    const selector = last ? path(last) : '';
    try { window.webkit.messageHandlers.searchHide.postMessage(selector); } catch (_) {}
    window.__searchHideOn = false;
    st.remove();
    if (last) last.classList.remove('__search_hide_hover');
    document.removeEventListener('click', onClick, true);
    document.removeEventListener('mousemove', onMove, true);
  };
  document.addEventListener('mousemove', onMove, true);
  document.addEventListener('click', onClick, true);
  return 'on';
})();
"#;
pub fn pick(_app: &App, view: &webkit2gtk::WebView, on: bool) {
    let js = if on {
        PICK
    } else {
        "(() => { window.__searchHideOn = false; const st = document.getElementById('__search_hide_style'); if (st) st.remove(); if (window.__searchHideLast) window.__searchHideLast.classList.remove('__search_hide_hover'); })();"
    };
    view.evaluate_javascript(
        js,
        Some(CURTAIN_WORLD),
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
}
/// Exercises the isolated picker world for the bench suite: turns hide mode on
/// and posts `selector` from `CURTAIN_WORLD`, matching a real picker click.
pub(crate) fn pick_selector(app: &AppRef, selector: &str) -> Result<(), &'static str> {
    let active = app.active_id();
    let Some(view) = app.view(active) else {
        return Err("no page");
    };
    let Some(clean) = sanitized(selector) else {
        return Err("selector refused");
    };
    if app.arm_hide_mode(active).is_none() {
        return Err("no host");
    }
    let payload = serde_json::to_string(&clean).unwrap_or_else(|_| "\"\"".into());
    let js = format!("window.webkit.messageHandlers.searchHide.postMessage({payload});");
    view.evaluate_javascript(
        &js,
        Some(CURTAIN_WORLD),
        None,
        None::<&gtk::gio::Cancellable>,
        |_| {},
    );
    Ok(())
}

/// A style sheet that hides what was hidden here, as the page begins.
pub fn install(app: &AppRef, manager: &webkit2gtk::UserContentManager) {
    manager.register_script_message_handler_in_world("searchHide", CURTAIN_WORLD);
    let weak = app.weak();
    manager.connect_script_message_received(Some("searchHide"), move |source_manager, value| {
        let Some(app) = weak.upgrade() else { return };
        let selector = value
            .js_value()
            .map(|value| value.to_str().to_string())
            .unwrap_or_default();
        // Refused here as well as at the sheet: a selector this app cannot
        // write is not one worth storing, and the page chose it.
        let Some(selector) = sanitized(&selector) else { return };
        let source_key = manager_key(source_manager);
        let Some(origin) = app.take_verified_hide_origin(source_key) else {
            return;
        };
        // Never call back into the view from inside its own message callback:
        // that is a page waiting on the browser waiting on the page. The work
        // is deferred and strictly bound to the originating tab and host so
        // tab switches, navigation, or closing cannot apply to the wrong target.
        let weak_delayed = app.weak();
        glib::timeout_add_local(std::time::Duration::from_millis(10), move || {
            if let Some(app) = weak_delayed.upgrade() {
                app.hide_selector_for_origin(&origin, &selector);
            }
            glib::ControlFlow::Break
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_normal_selector_survives() {
        assert_eq!(
            sanitized("#ad-slot").as_deref(),
            Some("#ad-slot"),
            "an id the picker builds is written as it is"
        );
        assert_eq!(
            sanitized("div.news > p:nth-of-type(2)").as_deref(),
            Some("div.news > p:nth-of-type(2)"),
            "a path with a combinator is fine"
        );
    }

    #[test]
    fn a_closing_style_tag_cannot_end_the_sheet() {
        // This is the one that used to break out: the id closes the element and
        // the rest of it is markup in the page.
        assert_eq!(sanitized("#x</style><script>alert(1)</script>"), None);
        assert_eq!(sanitized("#x</style>"), None);
    }

    #[test]
    fn quotes_and_backslashes_are_refused() {
        assert_eq!(sanitized("#a\"b"), None, "a quote can open a string");
        assert_eq!(sanitized("#a'b"), None);
        assert_eq!(sanitized("#a\\"), None, "a trailing backslash eats the brace after it");
        assert_eq!(sanitized("#a}b"), None, "a brace closes the rule early");
        assert_eq!(sanitized("#a{b"), None);
        // The picker's own escaping produces these, so they have to survive.
        assert_eq!(sanitized("#a\\:b").as_deref(), Some("#a\\:b"));
        assert_eq!(sanitized("#a\\3c b").as_deref(), Some("#a\\3c b"));
    }

    #[test]
    fn a_selector_split_into_several_is_refused() {
        assert_eq!(sanitized("#ad, body"), None);
        assert_eq!(sanitized("#ad\nbody"), None);
        assert_eq!(sanitized("   "), None);
        assert_eq!(sanitized(&"#".repeat(MAX_SELECTOR + 1)), None);
    }

    #[test]
    fn css_comment_delimiters_are_refused() {
        assert_eq!(sanitized("#ad/*"), None);
        assert_eq!(sanitized("*/#ad"), None);
        assert_eq!(sanitized("#a/*comment*/b"), None);
    }

    #[test]
    fn an_empty_host_reports_nothing_hidden() {
        // What the settings panel counts is the length of the list stored under
        // the host key, so a host with no entry has to read as zero rather than
        // as a name with nothing behind it.
        let store: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        let count = |host: &str| store.get(host).map(Vec::len).unwrap_or(0);
        assert_eq!(count(""), 0, "a tab with no page of its own hides nothing");
        assert_eq!(count("example.com"), 0, "a site never visited hides nothing");
        assert!(css_from(&[]).is_empty());
    }

    #[test]
    fn the_sheet_hides_exactly_what_was_stored() {
        let css = css_from(&["#ad".to_string(), "#slot".to_string()]);
        assert_eq!(
            css,
            "#ad { display: none !important; }\n#slot { display: none !important; }"
        );
        assert!(css_from(&[]).is_empty(), "an empty store writes no sheet");
        // A selector the page broke costs only itself.
        assert_eq!(
            css_from(&["#ad".to_string(), "#x</style>".to_string()]),
            "#ad { display: none !important; }"
        );
        assert!(css_from(&["#x</style>".to_string()]).is_empty());
    }

    #[test]
    fn a_hidden_element_stays_on_its_own_host() {
        // The key is `crate::shield::host_of`, the same helper the tab column
        // and the settings panel use, so the name the panel prints is the name
        // the sheet is filed under.
        let mut store: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        let host = crate::shield::host_of("https://example.com/page?x=1");
        store.insert(host.clone(), vec!["#ad".to_string()]);

        let on = |url: &str| store.get(&crate::shield::host_of(url)).map(Vec::len).unwrap_or(0);
        assert_eq!(on("https://example.com/other/path"), 1, "another page on the site sees it");
        assert_eq!(on("http://example.com/"), 1, "the scheme is not part of the site");
        assert_eq!(on("https://evil-example.com/"), 0, "a look-alike domain must not");
        assert_eq!(on("https://sub.example.com/"), 0, "a subdomain is its own host here");
        assert_eq!(on("about:blank"), 0);
        // An empty host has nothing stored, which is what makes the panel say
        // "nothing hidden" for a tab with no page of its own.
        assert!(!store.contains_key(""));
        assert_eq!(store.get("").map(Vec::len).unwrap_or(0), 0);
    }
}
