// Pages: one WebKit view per tab, made when you first look at a tab and given
// back when it closes. The engine is the one the desktop already ships with,
// so there is no second browser inside this one.
use crate::app::{AppRef, TabId};
use crate::curtain;
use gtk::prelude::*;
use webkit2gtk::{
    DownloadExt, HardwareAccelerationPolicy, NavigationPolicyDecisionExt, PolicyDecisionExt,
    ResponsePolicyDecisionExt, Settings, SettingsExt, URIRequestExt, URIResponseExt,
    UserContentManager, UserContentManagerExt, WebContext, WebContextExt, WebView, WebViewExt,
};
use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    // Two of these, not one: the shield is per host, and the rules are baked
    // into the content manager a view is made with. A host where you turned
    // the blocker off needs a manager that never had the rules in it.
    // Keyed by whether the blocker is on and by which extensions are installed:
    // a new extension means a new generation, and a new manager to build.
    static MANAGERS: RefCell<Vec<(bool, usize, UserContentManager)>> = const { RefCell::new(Vec::new()) };
    static CONTEXT_READY: RefCell<bool> = const { RefCell::new(false) };
}

fn shielded_for(app: &AppRef, url: &str) -> bool {
    app.prefs.borrow().shield && !crate::shield::is_allowed(&crate::shield::host_of(url))
}

/// The content manager a view should be made with, keyed on whether the
/// blocker applies to that page's host.
pub fn shared_manager(app: &AppRef, shielded: bool) -> UserContentManager {
    let generation = crate::extensions::generation();
    if let Some((_, _, manager)) = MANAGERS.with(|m| {
        m.borrow().iter().find(|(on, gen, _)| *on == shielded && *gen == generation).cloned()
    }) {
        return manager;
    }
    let manager = build_manager(app, shielded);
    MANAGERS.with(|m| {
        // Only the current generation is kept: a manager from before the last
        // extension change holds its own copy of every rule list, and one left
        // behind is a dozen megabytes that will never be used again.
        m.borrow_mut().retain(|(_, gen, _)| *gen == generation);
        m.borrow_mut().push((shielded, generation, manager.clone()));
    });
    manager
}

/// One content manager, with everything that should be in every page of this
/// kind: the blocker, the curtain, and the extensions' own scripts and sheets.
fn build_manager(app: &AppRef, shielded: bool) -> UserContentManager {
    let manager = UserContentManager::new();
    if shielded {
        crate::shield::install(&manager);
    }
    curtain::install(app, &manager);
    for (source, allow, at_start, every_frame) in crate::extensions::installable().scripts {
        let when = if at_start {
            webkit2gtk::UserScriptInjectionTime::Start
        } else {
            webkit2gtk::UserScriptInjectionTime::End
        };
        let allowed: Vec<&str> = allow.iter().map(|s| s.as_str()).collect();
        let frames = if every_frame {
            webkit2gtk::UserContentInjectedFrames::AllFrames
        } else {
            webkit2gtk::UserContentInjectedFrames::TopFrame
        };
        let script = webkit2gtk::UserScript::new(&source, frames, when, &allowed, &[]);
        manager.add_script(&script);
    }
    manager
}

fn settings() -> Settings {
    let settings = Settings::new();
    // The inspector is a debugging tool, and a page can tell it is there. It
    // is off unless the person asked for it with SEARCH_DEVTOOLS=1.
    settings.set_enable_developer_extras(std::env::var_os("SEARCH_DEVTOOLS").is_some());
    settings.set_enable_smooth_scrolling(true);
    settings.set_enable_webgl(true);
    settings.set_enable_media_stream(true);
    settings.set_enable_mediasource(true);
    settings.set_enable_webaudio(true);
    settings.set_enable_page_cache(true);
    settings.set_javascript_can_open_windows_automatically(false);
    // Compositing is left on. Two things were tried against it and both lost.
    // OnDemand is deprecated in this WebKit and prints a warning saying it is
    // now the same as Always, so it buys nothing. Turning the compositing path
    // off outright (WEBKIT_DISABLE_COMPOSITING_MODE, which is what the low
    // memory switch also sets) was measured over four runs of one idle tab:
    // 77.6-88.1 MB of the browser's own memory against 66.5-73.9 MB with it on.
    // It costs memory rather than saving it, so nothing turns it off for RAM.
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
        // Nothing sets the process model: both of its values are deprecated
        // since WebKitGTK 2.40 because several web processes is already what a
        // default context does. Asking for it changes nothing, which was
        // measured (the freezes after twenty tabs were unchanged).
        install_context(app, &context);
        context
    };
    let url = app.state.borrow().tab(tab).map(|t| t.url.clone()).unwrap_or_default();
    // A private tab gets its own manager, never the shared one, so nothing it
    // carries can be read back from another tab's memory of the same pages.
    let manager = if private {
        build_manager(app, shielded_for(app, &url))
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
                    app.reading_tabs.borrow_mut().remove(&tab);
                    if app.active_id() == tab { app.reading.set(false); }
                    app.set_loading(tab, true);
                    if let Some(slot) = app.state.borrow_mut().tab_mut(tab) {
                        slot.failed = false;
                    }
                    if let Some(uri) = view.uri().map(|u| u.to_string()) {
                        curtain::sync_for_url(&app, &uri);
                    }
                }
                // The page can keep arriving long after it is committed: a
                // stalled or streaming load must still read as running, so the
                // browser can stop it and the tab does not look idle.
                webkit2gtk::LoadEvent::Committed => {}
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
                // Every event is a failure worth explaining, not only the
                // one that starts a load: a connection dropped after the page
                // began arriving would otherwise fall through to the engine's
                // own grey page, which is not this browser's voice.
                let cancelled = error.matches(gtk::gio::IOErrorEnum::Cancelled)
                    || error.matches(webkit2gtk::NetworkError::Cancelled);
                if cancelled {
                    return false;
                }
                let reason = match event {
                    webkit2gtk::LoadEvent::Started => message_for(error),
                    webkit2gtk::LoadEvent::Committed | webkit2gtk::LoadEvent::Finished => {
                        format!("The page stopped arriving. ({})", message_for(error))
                    }
                    _ => message_for(error),
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
            // A frame or an image that answers 404 is that frame's business.
            // Only the page the address bar is showing may be replaced.
            if !policy.is_main_frame_main_resource() {
                return false;
            }
            let reason = reason_for_status(status);
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
    {
        let weak = app.weak();
        view.connect_web_process_terminated(move |_, reason| {
            if let Some(app) = weak.upgrade() {
                // Every reason leaves a dead view behind, so every reason gets
                // a sentence. Only `Crashed` used to be answered, which left a
                // tab the engine had killed for using too much memory sitting
                // grey with nothing said and no way to try again.
                let sentence = reason_for_termination(reason);
                let uri = app.state.borrow().tab(tab).map(|t| t.url.clone()).unwrap_or_default();
                app.mark_failed(tab);
                app.show_error(tab, &sentence, &uri);
            }
        });
    }
    // The page asks, the browser answers: a fresh address syncs what is hidden
    // there before the first frame, and anything that wants a window of its
    // own becomes a tab.
    // A navigation to a scheme the browser does not handle is refused here
    // rather than handed to the desktop. `mailto:` and `tel:` belong to
    // whatever the person has registered, and a page that can fire one on
    // load can open a mail composer with a body it wrote. Nothing outside the
    // list below is navigated to at all.
    {
        let weak = app.weak();
        view.connect_decide_policy(move |_, decision, kind| {
            if kind != webkit2gtk::PolicyDecisionType::NavigationAction {
                return false;
            }
            let Some(policy) = decision.downcast_ref::<webkit2gtk::NavigationPolicyDecision>()
            else {
                return false;
            };
            let Some(mut action) = policy.navigation_action() else { return false };
            let Some(uri) = action.request().and_then(|request| request.uri()) else {
                return false;
            };
            let main_frame = action.frame_name().map(|name| name == "_main").unwrap_or(true);
            if scheme_is_allowed(&uri, !main_frame) {
                return false;
            }
            // Only the page the address bar is showing gets an explanation. A
            // subframe that tries a custom scheme is that frame's business.
            if main_frame {
                if let Some(app) = weak.upgrade() {
                    app.mark_failed(tab);
                    app.show_error(
                        tab,
                        "That address is not a web page, so this browser did not open it.",
                        &uri,
                    );
                }
            }
            decision.ignore();
            true
        });
    }
    // The page does not get the camera, the microphone or the position by
    // asking. WebKit's own prompt is a stock GTK dialog with no relationship to
    // this window. Denial leaves the page and the reading flow intact.
    view.connect_permission_request(|_, request| {
        use webkit2gtk::PermissionRequestExt;
        request.deny();
        true
    });
    // window.open: a tab, never a pop-up. The page asks for a window, the
    // browser answers with a tab and gives WebKit no view of its own.
    {
        let weak = app.weak();
        view.connect_create(move |_, action| {
            if let Some(app) = weak.upgrade() {
                if let Some(uri) = action.request().and_then(|request| request.uri().map(|u| u.to_string())) {
                    if !scheme_is_allowed(&uri, false) {
                        app.flash_toast("This link asks for an address the browser cannot open.");
                        return None;
                    }
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
        // The name the server suggested is not the name that gets written: a
        // suggestion of "../../.bashrc" has to land inside Downloads like any
        // other file. The row shows the name that was actually used, so what
        // the panel says and what is on disk cannot disagree.
        let name = safe_filename(&suggested);
        let path = unique_path(&dir, &name);
        download.set_destination(&path.to_string_lossy());
        let record = crate::app::DownloadRec {
            name: name.clone(),
            path: path.to_string_lossy().to_string(),
            done: false,
            failed: false,
            got: 0,
            total: download
                .response()
                .map(|r| r.content_length())
                .unwrap_or(0),
        };
        app.state.borrow_mut().downloads.insert(0, record);
        let weak = weak.clone();
        let weak2 = weak.clone();
        let weak3 = weak.clone();
        let path_text = path.to_string_lossy().to_string();
        // Progress, so the row can say "12.4 of 40 MB" instead of only
        // "Downloading". WebKit reports the fraction, not the byte count, so
        // the bytes are the fraction of the length the response declared; a
        // response with no length leaves the row without a total, which the
        // panel renders as a plain byte count.
        {
            let path_text = path_text.clone();
            download.connect_estimated_progress_notify(move |download| {
                let Some(app) = weak3.upgrade() else { return };
                let fraction = download.estimated_progress();
                let total = download
                    .response()
                    .map(|r| r.content_length())
                    .unwrap_or(0);
                let mut state = app.state.borrow_mut();
                for entry in state.downloads.iter_mut() {
                    if entry.path == path_text {
                        entry.total = total;
                        entry.got = (total as f64 * fraction).round() as u64;
                    }
                }
                drop(state);
                if app.panels.borrow().open.clone() == Some(crate::panels::PanelKind::Downloads) {
                    app.open_panel(crate::panels::PanelKind::Downloads);
                }
            });
        }
        let finished_path = path_text.clone();
        download.connect_finished(move |_| {
            if let Some(app) = weak.upgrade() {
                {
                    let mut state = app.state.borrow_mut();
                    for entry in state.downloads.iter_mut() {
                        if entry.path == finished_path {
                            entry.done = true;
                            if entry.total > 0 {
                                entry.got = entry.total;
                            }
                        }
                    }
                    crate::store::Records::save_downloads(&state.downloads);
                }
                if app.panels.borrow().open.clone() == Some(crate::panels::PanelKind::Downloads) {
                    app.open_panel(crate::panels::PanelKind::Downloads);
                }
            }
        });
        {
            let path_text = path_text.clone();
            download.connect_failed(move |_, _| {
                if let Some(app) = weak2.upgrade() {
                    {
                        let mut state = app.state.borrow_mut();
                        for entry in state.downloads.iter_mut() {
                            if entry.path == path_text {
                                entry.failed = true;
                            }
                        }
                        crate::store::Records::save_downloads(&state.downloads);
                    }
                    app.flash_error("That download failed");
                    if app.panels.borrow().open.clone() == Some(crate::panels::PanelKind::Downloads) {
                        app.open_panel(crate::panels::PanelKind::Downloads);
                    }
                }
            });
        }
        if app.panels.borrow().open.clone() == Some(crate::panels::PanelKind::Downloads) {
            app.open_panel(crate::panels::PanelKind::Downloads);
        }
    });
}

pub fn downloads_dir() -> PathBuf {
    // The variable is a directory, not a path to a file; the test harness sets
    // it so a run cannot write into the person's real Downloads folder.
    std::env::var_os("XDG_DOWNLOAD_DIR")
        .map(PathBuf::from)
        .filter(|dir| !dir.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| Path::new(&h).join("Downloads")))
        .unwrap_or_else(|| PathBuf::from(".").join("Downloads"))
}

/// A filename a server suggested, reduced to something that can only name a
/// file inside the directory it is going into.
///
/// `Content-Disposition` is the server's text, and `Path::join` does not
/// sandbox: `dir.join("../../.bashrc")` leaves the directory, an absolute
/// `/home/you/.profile` replaces it outright, and a name of `..` or a leading
/// dot writes something the person will not see in their file manager. None of
/// those are exotic — a hostile or merely buggy server can send any of them,
/// and the download starts without a prompt.
///
/// So: take the last component only, drop separators and NULs, refuse the two
/// directory names, strip leading dots, and fall back to a plain word when
/// nothing readable is left. The extension is kept because it is what the
/// desktop uses to decide what opens the file.
fn safe_filename(suggested: &str) -> String {
    let last = suggested
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .replace('\0', "");
    let trimmed = last.trim();
    if trimmed == "." || trimmed == ".." {
        return "download".to_string();
    }
    let clean: String = trimmed.trim_start_matches('.').chars().take(200).collect();
    if clean.is_empty() {
        "download".to_string()
    } else {
        clean
    }
}

fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let _ = std::fs::create_dir_all(dir);
    let name = &safe_filename(name);
    let candidate = dir.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(name);
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let extension = path.extension().map(|s| s.to_string_lossy().to_string());
    for n in 1..10000 {
        let file = match &extension {
            Some(ext) => format!("{stem} ({n}).{ext}"),
            None => format!("{stem} ({n})"),
        };
        let candidate = dir.join(file);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("download-{}", std::process::id()))
}

/// The schemes this browser will navigate to.
///
/// Everything else — `mailto:`, `tel:`, `file:` from a remote page, and any
/// custom scheme a desktop application registered — is refused. A browser that
/// hands those to `xdg-open` lets a page launch a program on the person's
/// machine, which is not a thing a page should be able to do by being loaded.
/// `file:` is allowed only when the address bar asked for it, which is handled
/// by the caller: a local file the person opened is a page like any other.
pub fn scheme_is_allowed(uri: &str, user_entered: bool) -> bool {
    let lower = uri.trim().to_ascii_lowercase();
    (user_entered && lower.starts_with("file://"))
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("about:")
        || lower.starts_with("data:")
        || lower.starts_with("blob:")
}

/// What the engine says when it stops a page's process, in the browser's words.
fn reason_for_termination(reason: webkit2gtk::WebProcessTerminationReason) -> String {
    use webkit2gtk::WebProcessTerminationReason as Reason;
    match reason {
        Reason::Crashed => "The page stopped responding.".to_string(),
        Reason::ExceededMemoryLimit => {
            "The page used more memory than the system would give it, so it was stopped.".to_string()
        }
        _ => "The page was stopped by the engine.".to_string(),
    }
}

/// The sentence a status code is answered with, in the words a person would
/// use. Kept pure and separate from the view so the words can be tested: the
/// error pane and the stress suite both match on this text exactly.
fn reason_for_status(status: u32) -> String {
    match status {
        400 => "The site did not understand the request (400).".to_string(),
        401 | 403 => format!("The site would not let this page be read ({status})."),
        404 => "There is nothing at this address (404).".to_string(),
        408 => "The site took too long to answer (408).".to_string(),
        429 => "The site is asking for fewer requests (429).".to_string(),
        500..=599 => format!("The site broke while answering ({status})."),
        // Anything else still has to say something: an empty sentence would
        // be a blank pane, which reads as a bug rather than an answer.
        _ => format!("The site answered with {status}."),
    }
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

#[cfg(test)]
mod tests {
    use super::reason_for_status;

    // These are the words another module and the stress suite match on. A test
    // here needs no display and no network: the mapping is a plain function.
    fn sentence(status: u32) -> String {
        let text = reason_for_status(status);
        assert!(!text.trim().is_empty(), "status {status} answered with nothing");
        assert!(text.ends_with('.'), "status {status} is not a sentence: {text:?}");
        text
    }

    #[test]
    fn each_client_error_has_its_own_sentence() {
        let four_oh_four = sentence(404);
        let forbidden = sentence(403);
        let unauthorized = sentence(401);
        let too_many = sentence(429);
        assert_eq!(four_oh_four, "There is nothing at this address (404).");
        assert_ne!(four_oh_four, forbidden);
        assert_ne!(four_oh_four, unauthorized);
        assert_ne!(forbidden, unauthorized);
        assert_ne!(too_many, four_oh_four);
        assert_ne!(too_many, forbidden);
        // The number is in the sentence, so two refusals are still told apart.
        assert!(forbidden.contains("403") && unauthorized.contains("401"));
    }

    #[test]
    fn a_server_error_names_the_status() {
        let broke = sentence(500);
        let unavailable = sentence(503);
        assert_ne!(broke, unavailable);
        assert!(broke.contains("500") && unavailable.contains("503"));
    }

    #[test]
    fn the_other_ones_are_sentences_too() {
        for status in [400, 408, 500, 501, 599] {
            sentence(status);
        }
    }

    #[test]
    fn an_unknown_status_still_answers() {
        // A teapot has no sentence of its own, but a blank pane is worse than
        // an honest one, so the fallback has to say the number.
        let teapot = sentence(418);
        assert!(teapot.contains("418"), "{teapot:?} does not name the status");
        assert!(!teapot.trim().is_empty());
    }
}

#[cfg(test)]
mod filename_tests {
    use super::{safe_filename, scheme_is_allowed, unique_path};

    /// A download starts without a prompt, so the server's suggested name is
    /// the only thing standing between a page and the person's home directory.
    /// `Path::join` is not a sandbox: an absolute path replaces the directory
    /// and `..` climbs out of it. These are the shapes that must not survive.
    #[test]
    fn a_server_cannot_name_a_file_outside_downloads() {
        for hostile in [
            "../../.bashrc",
            "..\\..\\windows\\system32\\config",
            "/home/someone/.profile",
            "..",
            ".",
            "a/../../b",
            "\0evil",
            "....hidden",
            "   ",
            "",
        ] {
            let got = safe_filename(hostile);
            assert!(!got.is_empty(), "{hostile:?} became nothing");
            assert!(!got.contains('/'), "{hostile:?} kept a separator: {got}");
            assert!(!got.contains('\\'), "{hostile:?} kept a separator: {got}");
            assert!(!got.contains('\0'), "{hostile:?} kept a NUL");
            assert!(!got.starts_with('.'), "{hostile:?} is still hidden: {got}");
            assert_ne!(got, "..");
            assert_ne!(got, ".");
            // The real test: joining it to a directory stays in that directory.
            let joined = std::path::Path::new("/tmp/downloads").join(&got);
            assert_eq!(joined.parent(), Some(std::path::Path::new("/tmp/downloads")));
        }
    }

    /// An ordinary name must come through untouched, extension and all, or the
    /// desktop will not know what opens it.
    #[test]
    fn an_ordinary_name_is_left_alone() {
        assert_eq!(safe_filename("report.pdf"), "report.pdf");
        assert_eq!(safe_filename("My Report (final).pdf"), "My Report (final).pdf");
        assert_eq!(safe_filename("archive.tar.gz"), "archive.tar.gz");
        assert_eq!(safe_filename("  spaced.txt  "), "spaced.txt");
    }

    /// A name long enough to be a filesystem problem is cut, not refused.
    #[test]
    fn a_silly_length_is_cut() {
        let long = format!("{}.txt", "x".repeat(500));
        assert!(safe_filename(&long).chars().count() <= 200);
    }

    #[test]
    fn a_remote_page_cannot_read_a_local_file() {
        assert!(scheme_is_allowed("file:///tmp/a.html", true));
        assert!(!scheme_is_allowed("file:///tmp/a.html", false));
        assert!(scheme_is_allowed(" https://example.com ", false));
        assert!(!scheme_is_allowed("mailto:person@example.com", false));
        assert!(!scheme_is_allowed("javascript:alert(1)", true));
    }

    #[test]
    fn a_download_name_is_never_reused() {
        let root = std::env::temp_dir().join(format!("search-name-{}", std::process::id()));
        let first = unique_path(&root, "report.pdf");
        std::fs::write(&first, b"one").unwrap();
        let second = unique_path(&root, "report.pdf");
        assert_ne!(first, second);
        assert_eq!(second, root.join("report (1).pdf"));
        std::fs::remove_file(first).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
