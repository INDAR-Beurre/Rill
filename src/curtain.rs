// Hidden things: a per-site list of selectors, applied as a style sheet before
// the page draws, and a picker that adds to it.
use crate::app::{App, AppRef, TabId};
use javascriptcore::ValueExt;
use webkit2gtk::{
    UserContentInjectedFrames, UserContentManagerExt, UserStyleLevel, UserStyleSheet,
};
pub fn selectors(app: &App, host: &str) -> Vec<String> {
    app.state
        .borrow()
        .hidden
        .get(host)
        .cloned()
        .unwrap_or_default()
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
fn css_from(list: &[String]) -> String {
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
/// The style sheet the next document of the shared manager will wear.
pub fn sync(app: &AppRef, host: &str) {
    // Same key the view will be made with, or the sheet lands on the manager
    // the page does not use.
    let shielded = app.prefs.borrow().shield && !crate::shield::is_allowed(host);
    let manager = crate::web::shared_manager(app, shielded);
    let css = css_for(app, host);
    replace_sheet(&manager, &css);
}
// The curtain puts one sheet on a manager and takes its own back off. Clearing
// every sheet also cleared the dark page's, so turning the curtain off used to
// leave a dark desktop with a white page. What the curtain added last is held
// here, so it can be taken off by identity instead.
thread_local! {
    static ADDED: std::cell::RefCell<std::collections::HashMap<usize, UserStyleSheet>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn replace_sheet(manager: &webkit2gtk::UserContentManager, css: &str) {
    use glib::translate::ToGlibPtr;
    // The manager is the key: a sheet belongs to one manager, and a new
    // generation of managers is a new sheet list. The set of managers is two
    // or three for the life of the process, so the map cannot grow.
    let raw: *mut webkit2gtk::ffi::WebKitUserContentManager = manager.to_glib_none().0;
    let key = raw as usize;
    let previous = ADDED.with(|added| added.borrow_mut().remove(&key));
    if let Some(previous) = previous {
        // A key whose manager is long gone simply holds nothing to remove.
        manager.remove_style_sheet(&previous);
    }
    if let Some(sheet) = sheet_for(css) {
        manager.add_style_sheet(&sheet);
        ADDED.with(|added| added.borrow_mut().insert(key, sheet));
    }
}
/// The sheet the curtain wants on a manager: one carrying its own css, or none
/// when there is nothing to hide.
fn sheet_for(css: &str) -> Option<UserStyleSheet> {
    if css.is_empty() {
        return None;
    }
    Some(UserStyleSheet::new(
        css,
        UserContentInjectedFrames::AllFrames,
        UserStyleLevel::User,
        &[],
        &[],
    ))
}
pub fn apply_for_tab(app: &App, id: TabId) {
    let Some(app_ref) = crate::app::current() else { return };
    let host = app
        .state
        .borrow()
        .tab(id)
        .map(|t| crate::shield::host_of(&t.url))
        .unwrap_or_default();
    if !host.is_empty() {
        sync(&app_ref, &host);
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
    crate::js::run(view, &js, |_| {});
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
        PICK.to_string()
    } else {
        "(() => { window.__searchHideOn = false; const st = document.getElementById('__search_hide_style'); if (st) st.remove(); })();".to_string()
    };
    crate::js::run(view, &js, |_| {});
}
/// A style sheet that hides what was hidden here, as the page begins.
pub fn install(app: &AppRef, manager: &webkit2gtk::UserContentManager) {
    manager.register_script_message_handler("searchHide");
    let weak = app.weak();
    manager.connect_script_message_received(Some("searchHide"), move |_, value| {
        let Some(app) = weak.upgrade() else { return };
        let selector = value
            .js_value()
            .map(|value| value.to_str().to_string())
            .unwrap_or_default();
        // Refused here as well as at the sheet: a selector this app cannot
        // write is not one worth storing, and the page chose it.
        let Some(selector) = sanitized(&selector) else { return };
        let host = app
            .state
            .borrow()
            .active_tab()
            .map(|t| crate::shield::host_of(&t.url))
            .unwrap_or_default();
        if !host.is_empty() {
            // Never call back into the view from inside its own message
            // callback: that is a page waiting on the browser waiting on the
            // page. The work is put on the loop and done after this returns.
            glib::timeout_add_local(std::time::Duration::from_millis(10), move || {
                if let Some(app) = crate::app::current() {
                    app.hide_selector(&host, &selector);
                    app.state.borrow_mut().hide_mode = false;
                }
                glib::ControlFlow::Break
            });
        }
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
