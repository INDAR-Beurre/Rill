// Hidden things: a per-site list of selectors, applied as a style sheet before
// the page draws, and a picker that adds to it.
use crate::app::{App, AppRef, TabId};
use gtk::prelude::*;
use javascriptcore::ValueExt;
use webkit2gtk::{
    UserContentInjectedFrames, UserContentManagerExt, UserStyleLevel, UserStyleSheet, WebView,
    WebViewExt,
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
    if list.is_empty() {
        return String::new();
    }
    format!("{}\n{{ display: none !important; }}", list.join(", "))
}

/// The style sheet the next document of the shared manager will wear.
pub fn sync(app: &AppRef, host: &str) {
    let manager = crate::web::shared_manager(app);
    manager.remove_all_style_sheets();
    let css = css_for(app, host);
    if css.is_empty() {
        return;
    }
    let sheet = UserStyleSheet::new(
        &css,
        UserContentInjectedFrames::AllFrames,
        UserStyleLevel::User,
        &[],
        &[],
    );
    manager.add_style_sheet(&sheet);
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
/// visit.
pub fn apply_immediately(view: &webkit2gtk::WebView, app: &App, host: &str) {
    let list = selectors(app, host);
    if list.is_empty() {
        return;
    }
    let css = format!("{}\n{{ display: none !important; }}", list.join(", "));
    let js = format!(
        "(() => {{ let s = document.getElementById('__search_hidden'); if (!s) {{ s = document.createElement('style'); s.id = '__search_hidden'; (document.head || document.documentElement).appendChild(s); }} s.textContent = {}; }})();",
        serde_json::to_string(&css).unwrap_or_else(|_| "\"\"".into())
    );
    view.run_javascript(&js, None::<&gtk::gio::Cancellable>, |_| {});
}

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
    view.run_javascript(&js, None::<&gtk::gio::Cancellable>, |_| {});
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
        if selector.is_empty() {
            return;
        }
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
