// Pages: one WebKit view per tab, made when you first look at a tab and given
// back when it closes. The engine is the one the desktop already ships with,
// so there is no second browser inside this one.
use crate::app::{AppRef, TabId};
use crate::curtain;
use gtk::prelude::*;
use webkit2gtk::{
    DownloadExt, HardwareAccelerationPolicy, NavigationPolicyDecisionExt, PolicyDecisionExt,
    ResponsePolicyDecisionExt, Settings, SettingsExt, URIRequestExt, URIResponseExt,
    UserContentManager, WebContext, WebContextExt, WebView, WebViewExt,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    // Two of these, not one: the shield is per host, and the rules are baked
    // into the content manager a view is made with. A host where you turned
    // the blocker off needs a manager that never had the rules in it.
    static MANAGERS: RefCell<[Option<UserContentManager>; 2]> = const { RefCell::new([None, None]) };
    static CONTEXT_READY: RefCell<bool> = const { RefCell::new(false) };
}

fn shielded_for(app: &AppRef, url: &str) -> bool {
    app.prefs.borrow().shield && !crate::shield::is_allowed(&crate::shield::host_of(url))
}

/// The content manager a view should be made with, keyed on whether the
/// blocker applies to that page's host.
pub fn shared_manager(app: &AppRef, shielded: bool) -> UserContentManager {
    let slot = usize::from(shielded);
    if let Some(manager) = MANAGERS.with(|m| m.borrow()[slot].clone()) {
        return manager;
    }
    let manager = UserContentManager::new();
    if shielded {
        crate::shield::install(&manager);
    }
    curtain::install(app, &manager);
    MANAGERS.with(|m| m.borrow_mut()[slot] = Some(manager.clone()));
    manager
}

fn settings() -> Settings {
    let settings = Settings::new();
    settings.set_enable_developer_extras(true);
    settings.set_enable_smooth_scrolling(true);
    settings.set_enable_webgl(true);
    settings.set_enable_media_stream(true);
    settings.set_enable_mediasource(true);
    settings.set_enable_webaudio(true);
    settings.set_enable_page_cache(true);
    settings.set_javascript_can_open_windows_automatically(false);
    settings.set_hardware_acceleration_policy(HardwareAccelerationPolicy::Always);
    settings.set_enable_back_forward_navigation_gestures(true);
    settings
}

pub fn ephemeral_context() -> WebContext {
    WebContext::new_ephemeral()
}

pub fn view_for(app: &AppRef, tab: TabId) -> Option<WebView> {
    let private = {
        let state = app.state.borrow();
        state.tab(tab).map(|t| t.private).unwrap_or(false)
    };
    let context = if private {
        app.private_context()
    } else {
        let context = WebContext::default()?;
        install_context(app, &context);
        context
    };
    let url = app.state.borrow().tab(tab).map(|t| t.url.clone()).unwrap_or_default();
    let manager = if private {
        UserContentManager::new()
    } else {
        shared_manager(app, shielded_for(app, &url))
    };
    let view = WebView::builder()
        .web_context(&context)
        .user_content_manager(&manager)
        .settings(&settings())
        .build();
    view.set_hexpand(true);
    view.set_vexpand(true);
    // The engine's own background, so a slow page or a gap between loads is
    // the colour the browser is, not a white flash.
    let page = if app.dark() { (0.043, 0.043, 0.043) } else { (1.0, 1.0, 1.0) };
    view.set_background_color(&gtk::gdk::RGBA::new(page.0, page.1, page.2, 1.0));

    attach(app, tab, &view);
    Some(view)
}

fn attach(app: &AppRef, tab: TabId, view: &WebView) {
    {
        let weak = app.weak();
        view.connect_title_notify(move |view| {
            let title = view.title().map(|t| t.to_string()).unwrap_or_default();
            if let Some(app) = weak.upgrade() {
                app.set_title(tab, &title);
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_favicon_notify(move |view| {
            let icon = view.favicon();
            if let Some(app) = weak.upgrade() {
                app.set_icon(tab, icon);
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_uri_notify(move |view| {
            if let (Some(app), Some(uri)) = (weak.upgrade(), view.uri()) {
                app.set_uri(tab, &uri);
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_load_changed(move |view, event| {
            let Some(app) = weak.upgrade() else { return };
            match event {
                webkit2gtk::LoadEvent::Started => {
                    app.set_loading(tab, true);
                    if let Some(slot) = app.state.borrow_mut().tab_mut(tab) {
                        slot.failed = false;
                    }
                    if let Some(uri) = view.uri().map(|u| u.to_string()) {
                        curtain::sync_for_url(&app, &uri);
                    }
                }
                webkit2gtk::LoadEvent::Committed => app.set_loading(tab, false),
                webkit2gtk::LoadEvent::Finished => {
                    app.set_loading(tab, false);
                    app.sync_nav(tab, view);
                    app.load_finished(tab);
                    app.record_visit(tab);
                }
                _ => {}
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_is_loading_notify(move |view| {
            if let Some(app) = weak.upgrade() {
                app.sync_nav(tab, view);
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_load_failed(move |_, event, uri, error| {
            if let Some(app) = weak.upgrade() {
                // The engine's own error page is not the browser's voice: the
                // browser explains it itself.
                let reason = match event {
                    webkit2gtk::LoadEvent::Started => {
                        let cancelled = error.matches(gtk::gio::IOErrorEnum::Cancelled)
                            || error.matches(webkit2gtk::NetworkError::Cancelled);
                        if cancelled {
                            return false;
                        }
                        message_for(error)
                    }
                    _ => return false,
                };
                app.mark_failed(tab);
                app.show_error(tab, &reason, uri);
            }
            true
        });
    }
    {
        let weak = app.weak();
        view.connect_decide_policy(move |_, decision, kind| {
            if kind != webkit2gtk::PolicyDecisionType::Response {
                return false;
            }
            let Some(policy) = decision.downcast_ref::<webkit2gtk::ResponsePolicyDecision>() else {
                return false;
            };
            let Some(response) = policy.response() else { return false };
            let status = response.status_code();
            if status < 400 {
                return false;
            }
            let reason = match status {
                400 => "The site did not understand the request (400).".to_string(),
                401 | 403 => format!("The site would not let this page be read ({status})."),
                404 => "There is nothing at this address (404).".to_string(),
                408 => "The site took too long to answer (408).".to_string(),
                429 => "The site is asking for fewer requests (429).".to_string(),
                500..=599 => format!("The site broke while answering ({status})."),
                _ => format!("The site answered with {status}."),
            };
            if let Some(app) = weak.upgrade() {
                // The response's own uri is empty for a document the engine
                // refused to commit; the request is what was asked for, and it
                // is what the address bar and the tab have to say.
                let asked = policy.request().and_then(|request| request.uri().map(|uri| uri.to_string()));
                let answered = response.uri().map(|uri| uri.to_string());
                let uri = asked
                    .filter(|uri| !uri.is_empty())
                    .or(answered.filter(|uri| !uri.is_empty()))
                    .unwrap_or_else(|| {
                        app.state.borrow().tab(tab).map(|tab| tab.url.clone()).unwrap_or_default()
                    });
                app.mark_failed(tab);
                if !uri.is_empty() {
                    app.set_uri(tab, &uri);
                }
                app.show_error(tab, &reason, &uri);
            }
            decision.ignore();
            true
        });
    }
    // The page asks, the browser answers: a fresh address syncs what is hidden
    // there before the first frame, and anything that wants a window of its
    // own becomes a tab.
    // A navigation decision is a place to look, not to touch: the style sheet
    // is set when the load starts instead.
    view.connect_decide_policy(|_, decision, kind| {
        if kind != webkit2gtk::PolicyDecisionType::NavigationAction {
            return false;
        }
        decision
            .downcast_ref::<webkit2gtk::NavigationPolicyDecision>()
            .and_then(|policy| policy.navigation_action())
            .is_some()
            && false
    });
    // window.open: a tab, never a pop-up. The page asks for a window, the
    // browser answers with a tab and gives WebKit no view of its own.
    {
        let weak = app.weak();
        view.connect_create(move |_, action| {
            if let Some(app) = weak.upgrade() {
                if let Some(uri) = action.request().and_then(|request| request.uri().map(|u| u.to_string())) {
                    // A window asked for by a private tab is a private tab.
                    // It used to become an ordinary one, and be written down.
                    let private = app
                        .state
                        .borrow()
                        .tab(tab)
                        .map(|t| t.private)
                        .unwrap_or(false);
                    app.new_tab(Some(&uri), private, true);
                }
            }
            None
        });
    }
    {
        let weak = app.weak();
        view.connect_close(move |view| {
            if let Some(app) = weak.upgrade() {
                app.close_tab_of_view(view);
            }
        });
    }
    // The mouse's own doors.
    {
        let weak = app.weak();
        view.connect_button_press_event(move |_view, event| {
            if event.event_type() == gtk::gdk::EventType::ButtonPress {
                if let Some(button) = event.downcast_ref::<gtk::gdk::EventButton>() {
                    if let Some(app) = weak.upgrade() {
                        match button.button() {
                            8 => app.go_back(),
                            9 => app.go_forward(),
                            _ => {}
                        }
                    }
                }
            }
            crate::gtkx::inh(false)
        });
    }
    app.install_keys(view);
}

fn install_context(app: &AppRef, context: &WebContext) {
    if CONTEXT_READY.with(|c| *c.borrow()) {
        return;
    }
    CONTEXT_READY.with(|c| *c.borrow_mut() = true);
    let weak = app.weak();
    context.connect_download_started(move |_, download| {
        let Some(app) = weak.upgrade() else { return };
        let suggested = download
            .response()
            .and_then(|r| r.suggested_filename().map(|s| s.to_string()))
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                download
                    .request()
                    .and_then(|r| r.uri().map(|u| u.to_string()))
                    .and_then(|u| u.rsplit('/').next().map(|s| s.to_string()))
            })
            .unwrap_or_else(|| "download".to_string());
        let dir = downloads_dir();
        let path = unique_path(&dir, &suggested);
        download.set_destination(&path.to_string_lossy());
        let record = crate::app::DownloadRec {
            name: suggested.clone(),
            path: path.to_string_lossy().to_string(),
            done: false,
        };
        app.state.borrow_mut().downloads.insert(0, record);
        let weak = weak.clone();
        let weak2 = weak.clone();
        let path_text = path.to_string_lossy().to_string();
        download.connect_finished(move |_| {
            if let Some(app) = weak.upgrade() {
                let mut state = app.state.borrow_mut();
                for entry in state.downloads.iter_mut() {
                    if entry.path == path_text {
                        entry.done = true;
                    }
                }
            }
        });
        download.connect_failed(move |_, _| {
            if let Some(app) = weak2.upgrade() {
                app.flash_error("That download failed");
            }
        });
        if app.panels.borrow().open.clone() == Some(crate::panels::PanelKind::Downloads) {
            app.open_panel(crate::panels::PanelKind::Downloads);
        }
    });
}

pub fn downloads_dir() -> PathBuf {
    std::env::var_os("XDG_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join("Downloads")))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let _ = std::fs::create_dir_all(dir);
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());
    for n in 1..1000 {
        let file = match &extension {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(file);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(name)
}

/// A failure's reason, in the words a person would use.
fn message_for(error: &glib::Error) -> String {
    let text = error.message().to_lowercase();
    if text.contains("name or service not known")
        || text.contains("resolve")
        || text.contains("not found")
        || text.contains("dns")
    {
        "That address could not be found.".to_string()
    } else if text.contains("refused") {
        "The site refused the connection.".to_string()
    } else if text.contains("timed out") || text.contains("timeout") {
        "The site took too long to answer.".to_string()
    } else if text.contains("certificate") || text.contains("tls") || text.contains("ssl") {
        "The site's security could not be verified.".to_string()
    } else if text.contains("restricted network port") {
        "That address is not one a page may open.".to_string()
    } else if text.contains("network is unreachable") || text.contains("network") {
        "There is no network to reach it on.".to_string()
    } else {
        "The page could not be loaded.".to_string()
    }
}

pub fn load(view: &WebView, url: &str) {
    if url.is_empty() {
        return;
    }
    if url.starts_with("about:") || url.starts_with("http") || url.starts_with("file:") || url.starts_with("data:") {
        view.load_uri(url);
    } else {
        view.load_uri(&format!("https://{url}"));
    }
}

