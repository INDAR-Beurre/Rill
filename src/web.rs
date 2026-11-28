// Pages: one WebKit view per tab, made when you first look at a tab and given
// back when it closes. The engine is the one the desktop already ships with,
// so there is no second browser inside this one.
use crate::app::{AppRef, TabId};
use crate::curtain;
use gtk::prelude::*;
use javascriptcore::ValueExt;
use webkit2gtk::{
    CacheModel, CookieManagerExt, CookiePersistentStorage, DownloadExt, FileChooserRequestExt,
    HardwareAccelerationPolicy, HitTestResultExt, NavigationPolicyDecisionExt, PolicyDecisionExt,
    ResponsePolicyDecisionExt, Settings, SettingsExt, URIRequestExt, URIResponseExt,
    UserContentManager, UserContentManagerExt, WebContext, WebContextExt, WebView, WebViewExt,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOWNLOAD_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    // Two of these, not one: the shield is per host, and the rules are baked
    // into the content manager a view is made with. A host where you turned
    // the blocker off needs a manager that never had the rules in it.
    // Keyed by whether the blocker is on and by which extensions are installed:
    // a new extension means a new generation, and a new manager to build.
    static MANAGERS: RefCell<Vec<(bool, usize, UserContentManager)>> = const { RefCell::new(Vec::new()) };
    // Every context that already has the download handler. One flag for the
    // whole process used to stop at the first context, so the private
    // session's context, which is built later and rebuilt after the last
    // private tab closes, never got the handler. Weak references are kept so
    // a dropped context does not count, even if a new one lands at the same
    // address.
    static CONTEXT_READY: RefCell<Vec<glib::WeakRef<WebContext>>> = const { RefCell::new(Vec::new()) };
    // Active downloads keyed by download record id so the downloads panel can
    // cancel an in-flight WebKit download directly.
    static ACTIVE_DOWNLOADS: RefCell<HashMap<u64, webkit2gtk::Download>> = RefCell::new(HashMap::new());
}

/// Cancel an active download by its record id, returning true if it was active.
pub fn cancel_download(id: u64) -> bool {
    let download = ACTIVE_DOWNLOADS.with(|map| map.borrow_mut().remove(&id));
    if let Some(download) = download {
        download.cancel();
        true
    } else {
        false
    }
}

pub(crate) fn shielded_for(app: &crate::app::App, url: &str) -> bool {
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

/// All currently live shared content managers (shielded and unshielded) for
/// the active extension generation. Used to propagate per-host curtain updates
/// or removals without rebuilding managers.
pub(crate) fn live_shared_managers() -> Vec<UserContentManager> {
    let generation = crate::extensions::generation();
    MANAGERS.with(|m| {
        m.borrow()
            .iter()
            .filter(|(_, gen, _)| *gen == generation)
            .map(|(_, _, manager)| manager.clone())
            .collect()
    })
}

/// One content manager, with everything that should be in every page of this
/// kind: the blocker, the curtain, and the extensions' own scripts and sheets.
fn build_manager(app: &AppRef, shielded: bool) -> UserContentManager {
    let manager = UserContentManager::new();
    if shielded {
        crate::shield::install(&manager);
    }
    curtain::install(app, &manager);
    // Leaving reading mode from the page's exit button. Registered strictly
    // inside `READER_WORLD` so page scripts in the main world cannot post to
    // it, and still verified by probing `READER_WORLD` state on each reading
    // tab before clearing host membership.
    manager.register_script_message_handler_in_world("searchReader", crate::reader::READER_WORLD);
    let weak = app.weak();
    manager.connect_script_message_received(Some("searchReader"), move |_, value| {
        let Some(app) = weak.upgrade() else { return };
        let said = value.js_value().map(|value| value.to_str().to_string()).unwrap_or_default();
        if said.trim() != "off" {
            return;
        }
        let ids: Vec<_> = app.reading_tabs.borrow().iter().copied().collect();
        for id in ids {
            let Some(view) = app.view(id) else {
                app.reading_tabs.borrow_mut().remove(&id);
                continue;
            };
            let uri = view.uri().map(|value| value.to_string());
            let app = app.clone();
            crate::js::run_in_world(&view, crate::reader::READER_WORLD, crate::reader::STATE_PROBE, move |value| {
                if value.as_deref() != Ok("false") { return; }
                let same_document = app.view(id)
                    .and_then(|view| view.uri())
                    .map(|value| value.to_string()) == uri;
                if !same_document { return; }
                app.reading_tabs.borrow_mut().remove(&id);
                if app.active_id() == id {
                    app.reading.set(false);
                    app.refresh_helm();
                    app.layout_layers(
                        gtk::prelude::WidgetExt::allocated_width(&app.ui.overlay),
                        gtk::prelude::WidgetExt::allocated_height(&app.ui.overlay),
                    );
                }
            });
        }
    });
    for (source, allow, block, at_start, every_frame) in crate::extensions::installable().scripts {
        let when = if at_start {
            webkit2gtk::UserScriptInjectionTime::Start
        } else {
            webkit2gtk::UserScriptInjectionTime::End
        };
        let allowed: Vec<&str> = allow.iter().map(|s| s.as_str()).collect();
        // exclude_matches: WebKit's block list, checked before the allow list.
        let blocked: Vec<&str> = block.iter().map(|s| s.as_str()).collect();
        let frames = if every_frame {
            webkit2gtk::UserContentInjectedFrames::AllFrames
        } else {
            webkit2gtk::UserContentInjectedFrames::TopFrame
        };
        // Critic 7, round 2: add-on scripts used to run in the page's own
        // JavaScript world. There the page could read, change or intercept
        // them and anything they put on the page, such as a password
        // manager's fields. A named world gives them their own globals while
        // they still share the page's DOM, the same split Firefox and Chrome
        // use for content scripts. The add-on's messages to and from the page
        // still have to go through the DOM or postMessage.
        let script = webkit2gtk::UserScript::for_world(
            &source,
            frames,
            when,
            EXTENSION_WORLD,
            &allowed,
            &blocked,
        );
        manager.add_script(&script);
    }
    manager
}

/// The script world add-on content scripts run in. It is kept apart from the
/// page's world, so a page cannot see or tamper with an add-on's globals.
pub(crate) const EXTENSION_WORLD: &str = "search-ext";

fn devtools_enabled_for(value: Option<&std::ffi::OsStr>) -> bool {
    value != Some(std::ffi::OsStr::new("0"))
}

/// Whether the low-memory profile is on. main() sets SEARCH_LOW_MEMORY before
/// GTK starts when either the saved preference or the environment asks for
/// it. The answer is read once, so every context and view built during this
/// run uses the same profile.
pub(crate) fn low_memory_profile() -> bool {
    static PROFILE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *PROFILE.get_or_init(|| std::env::var_os("SEARCH_LOW_MEMORY").is_some())
}

/// The cache model for both contexts. DocumentBrowser keeps decoded pages and
/// resources around so Back and Forward feel instant. DocumentViewer keeps
/// almost nothing between pages, which is the trade the low-memory profile
/// asks for.
fn cache_model() -> CacheModel {
    if low_memory_profile() {
        CacheModel::DocumentViewer
    } else {
        CacheModel::DocumentBrowser
    }
}

pub(crate) fn settings() -> Settings {
    let settings = Settings::new();
    // The inspector is enabled by default so F12 / Ctrl+Shift+I work out of
    // the box; set SEARCH_DEVTOOLS=0 to disable it explicitly.
    settings.set_enable_developer_extras(devtools_enabled_for(
        std::env::var_os("SEARCH_DEVTOOLS").as_deref(),
    ));
    settings.set_enable_smooth_scrolling(true);
    settings.set_enable_webgl(true);
    settings.set_enable_webrtc(true);
    settings.set_enable_media_stream(true);
    settings.set_enable_mediasource(true);
    settings.set_enable_webaudio(true);
    // The page cache holds whole live pages (DOM, scripts, layers) in memory
    // for instant Back. In the low-memory profile Back reloads the page.
    settings.set_enable_page_cache(!low_memory_profile());
    settings.set_default_charset("utf-8");
    settings.set_enable_site_specific_quirks(true);
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
    // Present a standard modern Linux WebKit User-Agent so sites do not
    // downgrade or reject an unfamiliar application token.
    settings.set_user_agent(Some(
        "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15",
    ));
    settings
}

fn ensure_private_dir(path: &Path) {
    use std::os::unix::fs::DirBuilderExt;
    let _ = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path);
}

fn clean_language_tags<S: AsRef<str>>(raw: &[S]) -> Vec<String> {
    let mut tags = Vec::new();
    for item in raw {
        let base = item
            .as_ref()
            .trim()
            .split(['.', '@'])
            .next()
            .unwrap_or("")
            .trim();
        if base.is_empty()
            || base.eq_ignore_ascii_case("C")
            || base.eq_ignore_ascii_case("POSIX")
        {
            continue;
        }
        if !tags.iter().any(|existing: &String| existing == base) {
            tags.push(base.to_string());
        }
    }
    tags
}

fn configure_languages(context: &WebContext) {
    let raw = glib::language_names();
    let tags = clean_language_tags(&raw);
    if tags.is_empty() {
        return;
    }
    let refs: Vec<&str> = tags.iter().map(|s| s.as_str()).collect();
    // Accept-Language is always sent, so sites still pick the right language.
    context.set_preferred_languages(&refs);
    // Spellcheck loads a dictionary for every locale up front, in every page
    // process. The low-memory profile skips it; ordinary runs keep it.
    if low_memory_profile() {
        context.set_spell_checking_enabled(false);
        return;
    }
    context.set_spell_checking_enabled(true);
    context.set_spell_checking_languages(&refs);
}

/// The memory limit each page process gets. WebKit reads it only when a
/// WebContext is built (`memory-pressure-settings` is construct-only), so it
/// cannot be added to a context that already exists. These are the same
/// numbers as the network process limit in `src/main.rs`: start freeing
/// caches at a third of 384 MB, free everything possible at half, and check
/// every 5 seconds.
pub fn page_memory_pressure() -> webkit2gtk::MemoryPressureSettings {
    let mut pressure = webkit2gtk::MemoryPressureSettings::new();
    pressure.set_memory_limit(384);
    pressure.set_conservative_threshold(0.33);
    pressure.set_strict_threshold(0.5);
    pressure.set_poll_interval(5.0);
    pressure
}


/// The private session's context. It used to be `WebContext::new_ephemeral()`,
/// which takes no settings, so private page processes had no memory limit.
/// Building it with an ephemeral data manager is the same thing as
/// new_ephemeral, plus the limit. Nothing it stores reaches disk.
pub fn ephemeral_context() -> WebContext {
    let manager = webkit2gtk::WebsiteDataManager::new_ephemeral();
    let context = WebContext::builder()
        .website_data_manager(&manager)
        .memory_pressure_settings(&page_memory_pressure())
        .build();
    context.set_cache_model(cache_model());
    configure_languages(&context);
    context
}

thread_local! {
    // The one context every ordinary tab shares, built once on first use.
    static SHARED_CONTEXT: RefCell<Option<WebContext>> = const { RefCell::new(None) };
}

/// The context ordinary tabs share. It used to be `WebContext::default()`,
/// which WebKit builds itself, so its page processes could never get a memory
/// limit: `memory-pressure-settings` is construct-only. This context is built
/// with the limit instead. Its data manager points at the same folders
/// WebKit's default one uses (XDG data and cache dirs, each followed by the
/// program name, which hold hsts-storage.sqlite, localstorage, mediakeys and
/// the rest today). Without that, everything saved before this change
/// (sign-ins, site storage, HSTS) would be lost.
pub fn shared_context() -> WebContext {
    if let Some(context) = SHARED_CONTEXT.with(|c| c.borrow().clone()) {
        return context;
    }
    let name = glib::prgname()
        .map(|n| n.to_string())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "search".to_string());
    let data = glib::user_data_dir().join(&name);
    let cache = glib::user_cache_dir().join(&name);
    ensure_private_dir(&data);
    let manager = webkit2gtk::WebsiteDataManager::builder()
        .base_data_directory(data.to_string_lossy().to_string())
        .base_cache_directory(cache.to_string_lossy().to_string())
        .build();
    let context = WebContext::builder()
        .website_data_manager(&manager)
        .memory_pressure_settings(&page_memory_pressure())
        .build();
    let favicon_dir = cache.join("webkit-favicons");
    ensure_private_dir(&favicon_dir);
    context.set_favicon_database_directory(Some(&favicon_dir.to_string_lossy()));
    context.set_cache_model(cache_model());
    if let Some(cookies) = context.cookie_manager() {
        cookies.set_persistent_storage(
            &data.join("cookies.sqlite").to_string_lossy(),
            CookiePersistentStorage::Sqlite,
        );
    }
    configure_languages(&context);
    SHARED_CONTEXT.with(|c| *c.borrow_mut() = Some(context.clone()));
    context
}

fn page_background(app: &AppRef) -> gtk::gdk::RGBA {
    let page = if app.prefs.borrow().darken_pages {
        (0.043, 0.043, 0.043)
    } else {
        (1.0, 1.0, 1.0)
    };
    gtk::gdk::RGBA::new(page.0, page.1, page.2, 1.0)
}

pub fn view_for(app: &AppRef, tab: TabId) -> Option<WebView> {
    // A tab that cannot be found is treated as private. Before, it got the
    // shared context and the shared content manager, which write to disk, so a
    // private tab replaced or closed while its view was being built could leave
    // its activity behind. permissions::on_request already chose this default.
    let private = {
        let state = app.state.borrow();
        state.tab(tab).map(|t| t.private).unwrap_or(true)
    };
    let context = if private {
        // The private context needs the same download handler as the shared
        // one. Without it, a private download skipped safe_filename and
        // unique_path and went to WebKit's default folder.
        let context = app.private_context();
        install_context(app, &context);
        context
    } else {
        // Built by the browser, not WebKit's default, so its page processes
        // get the same 384 MB limit as the private ones. Same data folders
        // as before, so nothing saved is lost. The cache model is set in
        // shared_context(): bounded retained resources, page cache still on.
        let context = shared_context();
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
    // Keep the canvas white unless page darkening is explicitly on, so
    // websites without an explicit body background never draw dark text on dark.
    view.set_background_color(&page_background(app));

    attach(app, tab, &view);
    Some(view)
}

/// True when `view` is still the view the tab shows. Callbacks capture the
/// tab id when the view is attached. After a sleep and wake, a Keep from
/// Peek, or a crash and reload, the tab can have a newer view, and a signal
/// from the retired one must not change the live tab's title, address,
/// icon, loading state or error page. Every callback that changes tab state
/// checks this first, the same check dialogs.rs and permissions.rs use.
fn is_live(app: &crate::app::App, tab: TabId, view: &WebView) -> bool {
    app.view(tab).as_ref() == Some(view)
}

/// Binds a view's callbacks to a tab id. A peek that is kept calls this once
/// its view has been adopted as a tab, the same way a pop-up window does.
pub(crate) fn attach(app: &AppRef, tab: TabId, view: &WebView) {
    {
        let weak = app.weak();
        view.connect_title_notify(move |view| {
            let Some(app) = weak.upgrade() else { return };
            if !is_live(&app, tab, view) {
                return;
            }
            let title = view.title().map(|t| t.to_string()).unwrap_or_default();
            app.set_title(tab, &title);
        });
    }
    {
        let weak = app.weak();
        view.connect_favicon_notify(move |view| {
            let Some(app) = weak.upgrade() else { return };
            if !is_live(&app, tab, view) {
                return;
            }
            app.set_icon(tab, view.favicon());
        });
    }
    {
        let weak = app.weak();
        view.connect_uri_notify(move |view| {
            let Some(app) = weak.upgrade() else { return };
            if !is_live(&app, tab, view) {
                return;
            }
            if let Some(uri) = view.uri() {
                app.set_uri(tab, &uri);
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_load_changed(move |view, event| {
            let Some(app) = weak.upgrade() else { return };
            // A retired view's load events must not dismiss the live tab's
            // prompts, reset its reader state or restart its progress bar.
            if !is_live(&app, tab, view) {
                return;
            }
            match event {
                webkit2gtk::LoadEvent::Started => {
                    app.reading_tabs.borrow_mut().remove(&tab);
                    // Leaving the page takes its open prompt with it: the
                    // request it was answering is denied, not left waiting.
                    crate::permissions::dismiss(&app, tab);
                    crate::dialogs::dismiss(&app, tab);
                    crate::sitecard::dismiss_tab(&app, tab);
                    if app.active_id() == tab { app.reading.set(false); }
                    app.set_loading(tab, true);
                    app.set_load_progress(tab, 0.08);
                    if let Some(slot) = app.state.borrow_mut().tab_mut(tab) {
                        slot.failed = false;
                    }
                    if let Some(uri) = view.uri().map(|u| u.to_string()) {
                        if let Some(manager) = view.user_content_manager() {
                            let host = crate::shield::host_of(&uri);
                            if !host.is_empty() {
                                curtain::sync_for_tab(&app, tab, &manager, &host);
                            }
                        } else {
                            curtain::sync_for_url(&app, &uri);
                        }
                        let private = app.state.borrow().tab(tab).is_some_and(|t| t.private);
                        view.set_zoom_level(app.zoom_for(&uri, private));
                    }
                }
                // The page can keep arriving long after it is committed: a
                // stalled or streaming load must still read as running, so the
                // browser can stop it and the tab does not look idle.
                webkit2gtk::LoadEvent::Committed => {}
                webkit2gtk::LoadEvent::Finished => {
                    app.set_loading(tab, false);
                    app.set_load_progress(tab, 1.0);
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
            let Some(app) = weak.upgrade() else { return };
            if !is_live(&app, tab, view) {
                return;
            }
            app.sync_nav(tab, view);
        });
    }
    {
        let weak = app.weak();
        view.connect_estimated_load_progress_notify(move |view| {
            if let Some(app) = weak.upgrade() {
                if !is_live(&app, tab, view) {
                    return;
                }
                if view.is_loading() {
                    app.set_load_progress(tab, view.estimated_load_progress().clamp(0.08, 0.96));
                }
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_load_failed(move |view, event, uri, error| {
            if let Some(app) = weak.upgrade() {
                // A failure on a retired view must not put an error page on
                // the live tab. Returning true still stops the engine's own
                // grey page on the retired view.
                if !is_live(&app, tab, view) {
                    return true;
                }
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
                // A load that the browser turned into a download ends here with
                // WebKitPolicyError 102, "Frame load interrupted". That is not a
                // failure: the page became a file, as asked. Treating it as one
                // marked the tab failed and put up an error page for the .xpi
                // address. In a trace, the same address was then asked for
                // again, so every add-on from addons.mozilla.org downloaded
                // twice, saved as "name.xpi" and "name (1).xpi". Leave the tab
                // as it was and let the download hook take it from here.
                if error.matches(webkit2gtk::PolicyError::FrameLoadInterruptedByPolicyChange) {
                    app.set_load_progress(tab, 1.0);
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
                app.set_load_progress(tab, 1.0);
                app.show_error(tab, &reason, uri);
            }
            true
        });
    }
    {
        // A bad certificate is not an ordinary failed load: WebKit reports it
        // on its own signal, and nothing answered it, so an expired or forged
        // site left the tab finished, untitled and blank. The browser has to
        // say plainly that the site could not be trusted, and never open it.
        let weak = app.weak();
        view.connect_load_failed_with_tls_errors(move |view, uri, _certificate, flags| {
            if let Some(app) = weak.upgrade() {
                if !is_live(&app, tab, view) {
                    return true;
                }
                let reason = reason_for_tls(flags);
                app.mark_failed(tab);
                app.set_load_progress(tab, 1.0);
                app.show_error(tab, &reason, uri);
            }
            true
        });
    }
    {
        let weak = app.weak();
        view.connect_decide_policy(move |view, decision, kind| {
            if kind != webkit2gtk::PolicyDecisionType::Response {
                return false;
            }
            let Some(policy) = decision.downcast_ref::<webkit2gtk::ResponsePolicyDecision>() else {
                return false;
            };
            let Some(response) = policy.response() else { return false };
            let status = response.status_code();
            // An add-on from addons.mozilla.org is served as
            // application/x-xpinstall. Nothing in this engine can show that
            // type, and leaving the choice to WebKit downloaded nothing in a
            // test run: the tab stayed on the .xpi address and Downloads
            // stayed empty. Ask for a download here instead. The download hook
            // then gives the finished .xpi to the same installer the
            // Extensions panel uses.
            if status < 400 && policy.is_main_frame_main_resource() {
                let mime = response.mime_type().map(|m| m.to_ascii_lowercase()).unwrap_or_default();
                let is_xpi = mime == "application/x-xpinstall"
                    || response
                        .uri()
                        .map(|uri| uri.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase().ends_with(".xpi"))
                        .unwrap_or(false);
                if is_xpi {
                    // Only the tab's live view may start an add-on download. A
                    // view that was replaced, put to sleep or crashed could
                    // otherwise hand an .xpi to the installer for a page that
                    // is no longer on screen (Gemini R59 craft fix 5).
                    let live = weak.upgrade().is_some_and(|app| is_live(&app, tab, view));
                    if live {
                        decision.download();
                    } else {
                        decision.ignore();
                    }
                    return true;
                }
            }
            if status < 400 {
                return false;
            }
            // A frame or an image that answers 404 is that frame's business.
            // Only the page the address bar is showing may be replaced.
            if !policy.is_main_frame_main_resource() {
                return false;
            }
            // An error status with a page of its own is the site talking, and
            // the site is shown. Cloudflare's "Just a moment..." check arrives
            // as a 403 or 503, GitHub's 404 has a search box, a single-page
            // app may answer its sign-in screen with a 401, and a 429 says when
            // to try again. Replacing every one of them with the browser's
            // sentence made those sites impossible to open. Only an answer with
            // nothing to show (no HTML, or an empty body) gets the browser's
            // own page.
            if error_page_has_content(
                response.mime_type().as_deref(),
                response.content_length(),
            ) {
                return false;
            }
            let reason = reason_for_status(status);
            // A retired view (slept, woken, replaced by a Keep from Peek) may
            // still get a response. It must not mark the live page as failed
            // or rewrite its address. Its own load is still refused below.
            if let Some(app) = weak.upgrade().filter(|app| is_live(app, tab, view)) {
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
        // A video (or any element) asking for fullscreen. WebKit's default
        // handler fills the screen with the window when this returns false, so
        // the browser only takes its own column, band and bookmark row away and
        // gives them back on the way out. Before, the video went fullscreen
        // with the side column still beside it, and the browser's own
        // fullscreen flag fell out of step with the window.
        let weak = app.weak();
        view.connect_enter_fullscreen(move |view| {
            // Only the tab's live view may take the chrome away. A retired
            // view asking for fullscreen is refused (true stops WebKit's
            // default from filling the screen).
            match weak.upgrade() {
                Some(app) if is_live(&app, tab, view) => {
                    app.page_fullscreen_entered(tab);
                    false
                }
                Some(_) => true,
                None => false,
            }
        });
        let weak = app.weak();
        view.connect_leave_fullscreen(move |_| {
            // true keeps the window full screen when the user had F11 on before
            // the page asked; false lets WebKit's default take it out.
            // Deliberately not guarded by is_live: if the view that entered
            // fullscreen has been retired, leaving must still give the chrome
            // back, or the window is stuck without its column and toolbar.
            weak.upgrade().is_some_and(|app| app.page_fullscreen_left(tab))
        });
        let weak = app.weak();
        // Stamped on both edges: when sound starts and when it stops. The
        // sleep window is counted from the last moment the tab was heard, so
        // a tab that played music for half an hour and then went quiet still
        // gets the whole quiet window before it may sleep. Stamping only the
        // start measured the window from when the music began, and such a tab
        // could be put to sleep the moment it stopped.
        view.connect_is_playing_audio_notify(move |view| {
            if let Some(app) = weak.upgrade().filter(|app| is_live(app, tab, view)) {
                app.mark_audible(tab);
            }
        });
        // A call holds the camera or microphone while silent. Stamping on these
        // edges too keeps the window counted from when the call ended.
        // A retired view's edges say nothing about the live page, so they
        // must not keep a tab awake.
        let weak = app.weak();
        view.connect_camera_capture_state_notify(move |view| {
            if let Some(app) = weak.upgrade().filter(|app| is_live(app, tab, view)) {
                app.mark_audible(tab);
            }
        });
        let weak = app.weak();
        view.connect_microphone_capture_state_notify(move |view| {
            if let Some(app) = weak.upgrade().filter(|app| is_live(app, tab, view)) {
                app.mark_audible(tab);
            }
        });
    }
    {
        let weak = app.weak();
        view.connect_web_process_terminated(move |view, reason| {
            if let Some(app) = weak.upgrade() {
                // The tab id was captured when this view was attached. If the
                // tab has since been given another view (a sleep and wake, a
                // Keep from Peek), this signal comes from a retired view and
                // must not mark the live page as failed.
                if app.view(tab).as_ref() != Some(view) {
                    return;
                }
                // Every reason leaves a dead view behind, so every reason gets
                // a sentence. Only `Crashed` used to be answered, which left a
                // tab the engine had killed for using too much memory sitting
                // grey with nothing said and no way to try again.
                // A dead page cannot answer anything, so close everything that
                // was waiting on it, the same way a new load does in
                // connect_load_changed(Started). Before this, a crash left the
                // page's permission prompt, script dialog and site card open
                // over the error page. It also left the loading bar running and
                // reader mode still on, with no page behind them.
                // The same cleanup a rebuild does (App::retire_page_layers):
                // prompts, dialogs, the peek card, the site card, page
                // fullscreen and reader mode. A page that crashed while
                // fullscreen never sends leave-fullscreen, and a peek card
                // opened from it has nothing left to show.
                let view_uri = view.uri().map(|u| u.to_string());
                let tab_url = app.state.borrow().tab(tab).map(|t| t.url.clone()).unwrap_or_default();
                let uri = resolve_crash_uri(view_uri.as_deref(), &tab_url);
                app.retire_page_layers(tab);
                app.set_loading(tab, false);
                app.set_load_progress(tab, 1.0);
                let sentence = reason_for_termination(reason);
                app.show_process_terminated(tab, &sentence, &uri);
            }
        });
    }
    crate::menu::attach(app, tab, view);
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
        view.connect_decide_policy(move |view, decision, kind| {
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
            // Shift-click on a link reads it in a peek card instead of leaving
            // the page. A plain click takes the path below, unchanged.
            let link = action.navigation_type() == webkit2gtk::NavigationType::LinkClicked;
            if crate::peek::is_peek_click(
                crate::peek::shift_held(action.modifiers()),
                action.mouse_button(),
                link,
                crate::peek::is_peek_view(view),
            ) {
                // Only the tab's live view may open a peek card. A retired view
                // (slept, rebuilt, or replaced by Keep) still gets its click
                // ignored, but it must not put a card over the page on screen.
                if let Some(app) = weak.upgrade().filter(|app| is_live(app, tab, view)) {
                    crate::peek::open(&app, tab, view, &uri);
                }
                decision.ignore();
                return true;
            }
            let main_frame = action.frame_name().map(|name| name == "_main").unwrap_or(true);
            let allowed_file = uri.trim().to_ascii_lowercase().starts_with("file://")
                && weak.upgrade().is_some_and(|app| {
                    is_live(&app, tab, view) && {
                        let tab_url = app
                            .state
                            .borrow()
                            .tab(tab)
                            .map(|t| t.url.clone());
                        let view_uri = view.uri().map(|u| u.to_string());
                        allow_file_navigation(
                            main_frame,
                            action.is_redirect(),
                            matches!(
                                action.navigation_type(),
                                webkit2gtk::NavigationType::Other
                                    | webkit2gtk::NavigationType::Reload
                            ),
                            view_uri.as_deref(),
                            tab_url.as_deref(),
                            &uri,
                        )
                    }
                });
            if scheme_is_allowed(&uri, false) || allowed_file {
                return false;
            }
            // Only the page the address bar is showing gets an explanation. A
            // subframe that tries a custom scheme is that frame's business.
            // A retired view's refused navigation is still refused, but it
            // must not mark the live tab failed or replace it with an error
            // page.
            if main_frame {
                if let Some(app) = weak.upgrade().filter(|app| is_live(app, tab, view)) {
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
    // Notifications, once a site is allowed them, use WebKit's own default
    // show path and no handler is connected here. A gio::Notification was not
    // used: this app has no GtkApplication, so there is nothing for gio to
    // send from. The installed library exports WebKitNotificationPermissionRequest
    // and the session bus has org.freedesktop.Notifications. Neither was seen
    // producing a notification on a display, which this machine does not have.
    // Asked for by the page, answered by the browser: a prompt over the page
    // for the kinds a person is asked about, a silent refusal for the rest.
    // See permissions::kind_for for why each refused kind is refused.
    {
        let weak = app.weak();
        view.connect_permission_request(move |view, request| {
            match weak.upgrade() {
                Some(app) => crate::permissions::on_request(&app, tab, view, request),
                None => {
                    use webkit2gtk::PermissionRequestExt;
                    request.deny();
                    true
                }
            }
        });
    }
    {
        let weak = app.weak();
        // A sign-in card belongs to the page the tab is showing. A retired
        // view asking for credentials is cancelled, not shown over the live
        // page where the user would type into the wrong origin's prompt.
        view.connect_authenticate(move |view, request| match weak.upgrade() {
            Some(app) if is_live(&app, tab, view) => crate::permissions::on_auth(&app, tab, request),
            Some(_) => {
                use webkit2gtk::AuthenticationRequestExt;
                request.cancel();
                true
            }
            None => false,
        });
    }
    // Alerts, confirms, prompts and leave-page questions are answered by the
    // browser's card over the tab. The dialog is kept alive by the card until
    // it is answered: see dialogs::on_dialog.
    {
        let weak = app.weak();
        view.connect_script_dialog(move |view, dialog| match weak.upgrade() {
            Some(app) => crate::dialogs::on_dialog(&app, tab, view, dialog),
            None => {
                dialog.confirm_set_confirmed(false);
                dialog.close();
                true
            }
        });
    }
    {
        let weak = app.weak();
        // A retired view cannot open a native file chooser over the live page,
        // and neither can a background tab. The desktop's chooser is modal over
        // the whole window, so a tab nobody is looking at must not be able to
        // put one up over the page in front (Gemini R59 craft fix 5).
        view.connect_run_file_chooser(move |view, request| match weak.upgrade() {
            Some(app) if is_live(&app, tab, view) && app.active_id() == tab => {
                crate::dialogs::on_file_chooser(&app, tab, view, request)
            }
            _ => {
                request.cancel();
                true
            }
        });
    }
    // The address under the pointer, from the engine's hit test. The title and
    // the page's own text are never shown here: a page can fake those.
    {
        let weak = app.weak();
        view.connect_mouse_target_changed(move |_, hit, _| {
            let Some(app) = weak.upgrade() else { return };
            match hit.link_uri().filter(|_| hit.context_is_link()) {
                Some(uri) => crate::status::show(&app, &uri),
                None => crate::status::hide(),
            }
        });
    }
    // window.open: an allowed window becomes a tab, and the view WebKit is
    // handed back is the tab's own view. It is built related to the opener
    // (related_view), so window.opener works and it shares the opener's
    // context and content manager, which keeps a private tab's window private.
    // The callback must return that view synchronously, so the tab is made
    // here (adopt_tab) before the view is returned, not later by navigate.
    {
        let weak = app.weak();
        view.connect_create(move |opener, action| {
            // A retired view must not open tabs: its tab id now points at a
            // different view, and the new tab would claim the wrong opener.
            let app = weak.upgrade().filter(|app| is_live(app, tab, opener))?;
            let uri = action.request().and_then(|request| request.uri().map(|u| u.to_string()))?;
            if !scheme_is_allowed(&uri, false) {
                app.flash_toast("This link asks for an address the browser cannot open.");
                return None;
            }
            // A window is opened only right after a click or a key press. One
            // the page asks for by itself (on load, on a timer, on scroll) is
            // refused, its address kept for this tab, and offered on a card.
            if !action.is_user_gesture() {
                app.note_blocked_popup(tab, &uri);
                return None;
            }
            // A tab that cannot be found counts as private, the same safe
            // default view_for uses. The related view takes the opener's
            // context and content manager, so a private window stays private.
            let private = app.state.borrow().tab(tab).map(|t| t.private).unwrap_or(true);
            // Both are construct-only in WebKit, so they go in the builder.
            // related_view alone would share the process but not the manager.
            let mut builder = WebView::builder().related_view(opener).settings(&settings());
            if let Some(context) = opener.context() {
                builder = builder.web_context(&context);
            }
            if let Some(manager) = opener.user_content_manager() {
                builder = builder.user_content_manager(&manager);
            }
            let child = builder.build();
            child.set_hexpand(true);
            child.set_vexpand(true);
            child.set_background_color(&page_background(&app));
            let id = app.adopt_tab(&uri, private, &child)?;
            attach(&app, id, &child);
            Some(child.upcast::<gtk::Widget>())
        });
    }
    {
        let weak = app.weak();
        view.connect_close(move |view| {
            // window.close() from a retired view used to find its tab by the
            // old widget name and could close the replacement page. Only the
            // tab's live view may close it.
            if let Some(app) = weak.upgrade().filter(|app| is_live(app, tab, view)) {
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
    // Critic 2, round 2 (F3): the handler is installed once per context, not
    // once per process, so private downloads also get a safe file name, a
    // unique path in Downloads and a row in the panel.
    let done = CONTEXT_READY.with(|ready| {
        let mut ready = ready.borrow_mut();
        ready.retain(|weak| weak.upgrade().is_some());
        if ready.iter().any(|weak| weak.upgrade().as_ref() == Some(context)) {
            return true;
        }
        ready.push(context.downgrade());
        false
    });
    if done {
        return;
    }
    let weak = app.weak();
    context.connect_download_started(move |_, download| {
        let Some(app) = weak.upgrade() else { return };
        // Which tab asked for it. A download from a private tab stays out of
        // downloads.json. If the download has a view but that view's tab can no
        // longer be found, it is treated as private, which is the safe choice. A
        // download with no view at all was started by the browser itself, not
        // by a page, so it is recorded as usual.
        let private = download
            .web_view()
            .map(|view| {
                app.id_of_page(&view)
                    .and_then(|tab_id| app.state.borrow().tab(tab_id).map(|t| t.private))
                    .unwrap_or(true)
            })
            .unwrap_or(false);
        let id = NEXT_DOWNLOAD_ID.fetch_add(1, Ordering::Relaxed);
        // Where the download was asked for, kept before it starts. An add-on is
        // only offered for installing when this is the Firefox add-ons store
        // over HTTPS: see extensions::from_store. The address the page sent the
        // browser to decides, not the file's name or its type.
        let came_from = download
            .request()
            .and_then(|request| request.uri().map(|uri| uri.to_string()))
            .unwrap_or_default();
        let source_address = came_from.clone();
        // The tab that asked, kept now: by the time the download finishes the
        // focused tab may be another one.
        let asked_from = download.web_view().and_then(|view| app.id_of_page(&view));

        let weak_decide = weak.clone();
        download.connect_decide_destination(move |download, suggested| {
            let Some(app) = weak_decide.upgrade() else { return false };
            let dir = downloads_dir();
            let request_uri = download
                .request()
                .and_then(|r| r.uri().map(|u| u.to_string()));
            // The name the server suggested is not the name that gets written: a
            // suggestion of "../../.bashrc" has to land inside Downloads like any
            // other file. The row shows the name that was actually used, so what
            // the panel says and what is on disk cannot disagree.
            let name = suggested_name(suggested, request_uri.as_deref());
            let path = unique_path(&dir, &name);
            let path_text = path.to_string_lossy().to_string();
            download.set_destination(&path_text);
            let record = crate::app::DownloadRec {
                id,
                name: name.clone(),
                path: path_text,
                done: false,
                failed: false,
                got: 0,
                total: download
                    .response()
                    .map(|r| r.content_length())
                    .unwrap_or(0),
                private,
                source_url: crate::tab_actions::strip_url_credentials(&source_address),
                missing: false,
            };
            app.state.borrow_mut().downloads.insert(0, record);
            ACTIVE_DOWNLOADS.with(|map| {
                map.borrow_mut().insert(id, download.clone());
            });
            if app.panels.try_borrow().map(|p| p.open.clone()).unwrap_or(None)
                == Some(crate::panels::PanelKind::Downloads)
            {
                crate::panels::refresh_downloads(&app);
            } else {
                app.flash_toast(&format!("Downloading {name}"));
            }
            true
        });

        let weak_progress = weak.clone();
        // Progress, so the row can say "12.4 of 40 MB" instead of only
        // "Downloading". WebKit reports the fraction, not the byte count, so
        // the bytes are the fraction of the length the response declared; when
        // no length was declared, WebKit's own received byte counter is used
        // instead of reporting 0.
        download.connect_estimated_progress_notify(move |download| {
            let Some(app) = weak_progress.upgrade() else { return };
            let fraction = download.estimated_progress();
            let total = download
                .response()
                .map(|r| r.content_length())
                .unwrap_or(0);
            let got = if total > 0 {
                (total as f64 * fraction).round() as u64
            } else {
                download.received_data_length()
            };
            {
                let mut state = app.state.borrow_mut();
                if let Some(entry) = state.downloads.iter_mut().find(|entry| entry.id == id) {
                    entry.total = total;
                    entry.got = got;
                }
            }
            if app.panels.try_borrow().map(|p| p.open.clone()).unwrap_or(None)
                == Some(crate::panels::PanelKind::Downloads)
            {
                crate::panels::refresh_downloads(&app);
            }
        });

        let weak_finished = weak.clone();
        download.connect_finished(move |download| {
            ACTIVE_DOWNLOADS.with(|map| {
                map.borrow_mut().remove(&id);
            });
            let Some(app) = weak_finished.upgrade() else { return };
            let finished = {
                let mut state = app.state.borrow_mut();
                let found = if let Some(entry) =
                    state.downloads.iter_mut().find(|entry| entry.id == id)
                {
                    entry.done = true;
                    if entry.total > 0 {
                        entry.got = entry.total;
                    } else {
                        let received = download.received_data_length();
                        if received > 0 {
                            entry.got = received;
                        }
                    }
                    Some((entry.name.clone(), entry.path.clone()))
                } else {
                    None
                };
                crate::store::Records::save_downloads(&state.downloads);
                found
            };
            let (finished_name, finished_path) = finished.unwrap_or_default();
            if app.panels.try_borrow().map(|p| p.open.clone()).unwrap_or(None)
                == Some(crate::panels::PanelKind::Downloads)
            {
                crate::panels::refresh_downloads(&app);
            } else if !finished_name.is_empty() {
                app.flash_toast(&format!("Downloaded {finished_name}"));
            }
            // An add-on from addons.mozilla.org arrives here as an .xpi
            // download. Nothing is installed without the person saying
            // yes: the archive is read, not installed, and a card over the
            // tab names the add-on, where it came from, every site it would
            // run on, and the add-on it would replace. Only Install on that
            // card calls the installer. An .xpi from anywhere else stays in
            // Downloads as a plain file, so a page that sends the browser to
            // evil.example/x.xpi gets a download and nothing more.
            let is_addon_type = download
                .response()
                .and_then(|r| r.mime_type())
                .is_some_and(|m| m.eq_ignore_ascii_case("application/x-xpinstall"));
            if !finished_path.is_empty()
                && (is_addon_type || finished_path.to_ascii_lowercase().ends_with(".xpi"))
            {
                if crate::extensions::from_store(&came_from) {
                    match crate::extensions::inspect(std::path::Path::new(&finished_path)) {
                        Ok(offer) => crate::permissions::offer_addon(
                            &app,
                            asked_from.unwrap_or_else(|| app.active_id()),
                            std::path::PathBuf::from(&finished_path),
                            came_from.clone(),
                            offer,
                        ),
                        Err(why) => app.flash_toast(&format!("Could not read the add-on: {why}")),
                    }
                } else {
                    app.flash_toast(
                        "Saved to Downloads. Add-ons install only from addons.mozilla.org, so this one was not installed.",
                    );
                }
            }
        });

        let weak_failed = weak.clone();
        download.connect_failed(move |_, _| {
            ACTIVE_DOWNLOADS.with(|map| {
                map.borrow_mut().remove(&id);
            });
            if let Some(app) = weak_failed.upgrade() {
                {
                    let mut state = app.state.borrow_mut();
                    if let Some(entry) = state.downloads.iter_mut().find(|entry| entry.id == id) {
                        entry.failed = true;
                    }
                    crate::store::Records::save_downloads(&state.downloads);
                }
                app.flash_error("That download failed");
                if app.panels.try_borrow().map(|p| p.open.clone()).unwrap_or(None)
                    == Some(crate::panels::PanelKind::Downloads)
                {
                    crate::panels::refresh_downloads(&app);
                }
            }
        });
    });
}

fn suggested_name(suggested: &str, request_uri: Option<&str>) -> String {
    let candidate = suggested.trim();
    let raw = if !candidate.is_empty() {
        candidate.to_string()
    } else {
        request_uri
            .and_then(|uri| uri.split(['?', '#']).next())
            .and_then(|path| path.rsplit('/').next())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or("download")
            .to_string()
    };
    safe_filename(&raw)
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

/// Permit main-frame `file://` navigation only when explicitly initiated by
/// browser/user navigation (`app::navigate`, extension options pages, or view-source)
/// or following a link from an already-local `file://` page — never from a remote
/// document, subframe, or HTTP redirect.
fn allow_file_navigation(
    main_frame: bool,
    is_redirect: bool,
    from_browser_load: bool,
    current_view_uri: Option<&str>,
    tab_url: Option<&str>,
    target_uri: &str,
) -> bool {
    if !main_frame || is_redirect {
        return false;
    }
    let target = target_uri.trim();
    if !target.to_ascii_lowercase().starts_with("file://") {
        return false;
    }
    if from_browser_load && tab_url.is_some_and(|u| u.trim() == target) {
        return true;
    }
    current_view_uri.is_some_and(|u| u.trim().to_ascii_lowercase().starts_with("file://"))
}

/// Pick the most accurate URL to display and retry after a web-process crash:
/// prefer the WebView's last URI when non-empty and not `about:blank`, falling
/// back to the tab's recorded URL.
pub(crate) fn resolve_crash_uri(view_uri: Option<&str>, tab_url: &str) -> String {
    if let Some(uri) = view_uri.map(str::trim) {
        if !uri.is_empty() && !uri.eq_ignore_ascii_case("about:blank") {
            return uri.to_string();
        }
    }
    tab_url.trim().to_string()
}

/// What the engine says when it stops a page's process, in the browser's words.
pub(crate) fn reason_for_termination(reason: webkit2gtk::WebProcessTerminationReason) -> String {
    use webkit2gtk::WebProcessTerminationReason as Reason;
    match reason {
        // A crash is not a hang: WebKit's page process died (for example an
        // abort inside the engine), so say exactly that, and say it as soon as
        // the engine reports it rather than after a load timeout.
        Reason::Crashed => "The page crashed.".to_string(),
        Reason::ExceededMemoryLimit => {
            "The page used more memory than the system would give it, so it was stopped.".to_string()
        }
        _ => "The page was stopped by the engine.".to_string(),
    }
}

/// The sentence a status code is answered with, in the words a person would
/// use. Kept pure and separate from the view so the words can be tested: the
/// error pane and the stress suite both match on this text exactly.
/// Whether a 4xx/5xx answer carries a page worth showing as it is. HTML (or
/// XHTML) with a body that is not declared empty counts; plain text, JSON, an
/// image or a zero-length body does not, and gets the browser's own page.
/// A length of 0 from WebKit can also mean "not declared" on a chunked
/// answer, so only the MIME type decides when the body is HTML and chunked:
/// the empty case the browser has to cover is an HTML type with an explicit
/// empty body, which servers send as Content-Length: 0 and WebKit reports as 0
/// too. That ambiguity is resolved toward showing the site's page.
pub(crate) fn error_page_has_content(mime: Option<&str>, length: u64) -> bool {
    let _ = length;
    let mime = mime.unwrap_or("").trim().to_ascii_lowercase();
    let essence = mime.split(';').next().unwrap_or("").trim();
    matches!(essence, "text/html" | "application/xhtml+xml")
}

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
    message_for_text(error.message())
}

/// Pure text mapping for `message_for` so unit tests can assert real WebKit /
/// GLib TLS and network error strings end-to-end with the error classifier.
pub(crate) fn message_for_text(raw: &str) -> String {
    let text = raw.to_lowercase();
    if text.contains("certificate") || text.contains("tls") || text.contains("ssl") {
        "The site's security could not be verified.".to_string()
    } else if text.contains("name or service not known")
        || text.contains("resolve")
        || text.contains("not found")
        || text.contains("dns")
    {
        "That address could not be found.".to_string()
    } else if text.contains("refused") {
        "The site refused the connection.".to_string()
    } else if text.contains("timed out") || text.contains("timeout") {
        "The site took too long to answer.".to_string()
    } else if text.contains("restricted network port") {
        "That address is not one a page may open.".to_string()
    } else if text.contains("network is unreachable") || text.contains("network") {
        "There is no network to reach it on.".to_string()
    } else {
        "The page could not be loaded.".to_string()
    }
}

/// What a certificate problem means, in one sentence. The site is never opened
/// behind it: an expired or forged certificate is exactly what an attacker in
/// the middle would present, so the page says so plainly instead of going blank.
pub fn reason_for_tls(flags: gtk::gio::TlsCertificateFlags) -> String {
    format!("This site's security could not be verified: {}. It was not opened.", tls_problem(flags))
}

/// The cause of a certificate problem, shared by the error page and the site card.
pub fn tls_problem(flags: gtk::gio::TlsCertificateFlags) -> &'static str {
    use gtk::gio::TlsCertificateFlags as F;
    if flags.contains(F::EXPIRED) {
        "its certificate has expired"
    } else if flags.contains(F::NOT_ACTIVATED) {
        "its certificate is not valid yet"
    } else if flags.contains(F::BAD_IDENTITY) {
        "its certificate belongs to a different site"
    } else if flags.contains(F::UNKNOWN_CA) {
        "its certificate was not issued by anyone this computer trusts"
    } else if flags.contains(F::REVOKED) {
        "its certificate has been revoked"
    } else if flags.contains(F::INSECURE) {
        "its certificate uses insecure encryption"
    } else {
        "its certificate could not be checked"
    }
}

pub(crate) fn normalize_load_uri(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    let has_scheme = ["http://", "https://", "about:", "file://", "data:", "blob:", "search:"]
        .iter()
        .any(|prefix| lower.starts_with(prefix));
    if has_scheme {
        Some(trimmed.to_string())
    } else {
        Some(format!("https://{trimmed}"))
    }
}

pub fn load(view: &WebView, url: &str) {
    if let Some(target) = normalize_load_uri(url) {
        view.load_uri(&target);
    }
}

#[cfg(test)]
mod tests {
    use super::{error_page_has_content, reason_for_status, reason_for_termination, reason_for_tls};

    #[test]
    fn an_error_status_with_a_page_of_its_own_is_shown_as_the_site_sent_it() {
        // Cloudflare's check, GitHub's 404, a 401 sign-in screen: HTML, shown.
        assert!(error_page_has_content(Some("text/html"), 0));
        assert!(error_page_has_content(Some("text/html; charset=utf-8"), 5120));
        assert!(error_page_has_content(Some("TEXT/HTML;charset=UTF-8"), 5120));
        assert!(error_page_has_content(Some("application/xhtml+xml"), 300));
        // Nothing a person can read as a page gets the browser's own page.
        assert!(!error_page_has_content(Some("text/plain"), 9));
        assert!(!error_page_has_content(Some("application/json"), 40));
        assert!(!error_page_has_content(Some("image/png"), 70));
        assert!(!error_page_has_content(Some(""), 0));
        assert!(!error_page_has_content(None, 0));
    }
    use gtk::gio::TlsCertificateFlags as F;

    /// A crashed page process must be called a crash, never a hang: GitHub's
    /// page process aborted inside WebKit and the old words sent people
    /// looking for a slow site instead of a missing audio plugin.
    #[test]
    fn a_crash_is_called_a_crash() {
        let crashed = reason_for_termination(webkit2gtk::WebProcessTerminationReason::Crashed);
        assert_eq!(crashed, "The page crashed.");
        assert!(!crashed.contains("responding"), "{crashed:?} reads as a hang");
        let memory =
            reason_for_termination(webkit2gtk::WebProcessTerminationReason::ExceededMemoryLimit);
        assert_ne!(crashed, memory);
        assert!(memory.contains("memory"));
    }

    #[test]
    fn resolve_crash_uri_prefers_live_view_uri_over_blank_or_stale_tab_url() {
        assert_eq!(
            super::resolve_crash_uri(Some("https://www.youtube.com/watch?v=1"), "https://www.youtube.com/"),
            "https://www.youtube.com/watch?v=1"
        );
        assert_eq!(
            super::resolve_crash_uri(Some("about:blank"), "https://www.youtube.com/"),
            "https://www.youtube.com/"
        );
        assert_eq!(
            super::resolve_crash_uri(Some("   "), "https://github.com/"),
            "https://github.com/"
        );
        assert_eq!(
            super::resolve_crash_uri(None, "https://example.com/"),
            "https://example.com/"
        );
    }

    /// Every certificate problem gets its own reason, and every reason says the
    /// site was not opened, so a bad certificate can never end in a blank pane.
    #[test]
    fn each_certificate_problem_is_named() {
        let cases = [
            (F::EXPIRED, "expired"),
            (F::NOT_ACTIVATED, "not valid yet"),
            (F::BAD_IDENTITY, "different site"),
            (F::UNKNOWN_CA, "trusts"),
            (F::REVOKED, "revoked"),
            (F::INSECURE, "insecure"),
        ];
        let mut seen = std::collections::HashSet::new();
        for (flag, words) in cases {
            let text = reason_for_tls(flag);
            assert!(text.contains(words), "{flag:?} gave {text:?}");
            assert!(text.ends_with("It was not opened."), "{flag:?} gave {text:?}");
            assert!(seen.insert(text), "{flag:?} shares its sentence with another flag");
        }
    }

    /// Expiry wins over anything else reported with it, and an empty or
    /// unexpected set of flags still produces a sentence, not nothing.
    #[test]
    fn certificate_flags_have_a_fallback_and_an_order() {
        let both = reason_for_tls(F::EXPIRED | F::UNKNOWN_CA);
        assert!(both.contains("expired"), "{both:?}");
        let unknown = reason_for_tls(F::GENERIC_ERROR);
        assert!(unknown.contains("could not be checked"), "{unknown:?}");
        assert!(unknown.ends_with("It was not opened."));
        assert!(!reason_for_tls(F::empty()).trim().is_empty());
    }

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
        assert!(super::allow_file_navigation(
            true,
            false,
            true,
            None,
            Some("file:///tmp/options.html"),
            "file:///tmp/options.html"
        ));
        assert!(!super::allow_file_navigation(
            true,
            false,
            true,
            Some("https://evil.example/"),
            Some("https://evil.example/"),
            "file:///etc/passwd"
        ));
        assert!(!super::allow_file_navigation(
            true,
            true,
            true,
            None,
            Some("file:///tmp/options.html"),
            "file:///etc/passwd"
        ));
        assert!(!super::allow_file_navigation(
            false,
            false,
            true,
            Some("file:///tmp/options.html"),
            Some("file:///tmp/options.html"),
            "file:///etc/passwd"
        ));
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

    #[test]
    fn load_uri_normalizes_schemes_case_insensitively() {
        assert_eq!(super::normalize_load_uri(""), None);
        assert_eq!(super::normalize_load_uri("   "), None);
        assert_eq!(
            super::normalize_load_uri("  HTTPS://Example.COM/Path  ").as_deref(),
            Some("HTTPS://Example.COM/Path")
        );
        assert_eq!(
            super::normalize_load_uri("HTTP://example.com").as_deref(),
            Some("HTTP://example.com")
        );
        assert_eq!(
            super::normalize_load_uri("blob:https://example.com/123").as_deref(),
            Some("blob:https://example.com/123")
        );
        assert_eq!(
            super::normalize_load_uri("BLOB:https://example.com/123").as_deref(),
            Some("BLOB:https://example.com/123")
        );
        assert_eq!(
            super::normalize_load_uri("DATA:text/plain,Hello").as_deref(),
            Some("DATA:text/plain,Hello")
        );
        assert_eq!(
            super::normalize_load_uri("ABOUT:blank").as_deref(),
            Some("ABOUT:blank")
        );
        assert_eq!(
            super::normalize_load_uri("FILE:///tmp/a.html").as_deref(),
            Some("FILE:///tmp/a.html")
        );
        assert_eq!(
            super::normalize_load_uri("SEARCH:settings").as_deref(),
            Some("SEARCH:settings")
        );
        assert_eq!(
            super::normalize_load_uri("example.com").as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            super::normalize_load_uri("httpevil.com").as_deref(),
            Some("https://httpevil.com")
        );
    }

    #[test]
    fn devtools_is_enabled_unless_explicitly_zero() {
        use std::ffi::OsStr;
        assert!(super::devtools_enabled_for(None));
        assert!(super::devtools_enabled_for(Some(OsStr::new("1"))));
        assert!(super::devtools_enabled_for(Some(OsStr::new(""))));
        assert!(!super::devtools_enabled_for(Some(OsStr::new("0"))));
    }

    #[test]
    fn cancelling_an_unknown_download_returns_false() {
        assert!(!super::cancel_download(999_999));
    }

    #[test]
    fn clean_language_tags_strips_encodings_and_drops_posix_locales() {
        let cleaned = super::clean_language_tags(&[
            "en_US.UTF-8",
            "en_US",
            "en",
            "de_DE.UTF-8@euro",
            "C.UTF-8",
            "POSIX",
            "c",
            "   ",
            "",
            "fr_FR.utf8",
            "en_US.UTF-8",
        ]);
        assert_eq!(cleaned, vec!["en_US", "en", "de_DE", "fr_FR"]);
    }

    #[test]
    fn suggested_name_uses_server_suggestion_or_stripped_url_segment() {
        assert_eq!(
            super::suggested_name("report.pdf", Some("https://example.com/dl?id=1")),
            "report.pdf"
        );
        assert_eq!(
            super::suggested_name("", Some("https://example.com/files/archive.tar.gz?token=123#top")),
            "archive.tar.gz"
        );
        assert_eq!(
            super::suggested_name("   ", Some("https://example.com/")),
            "download"
        );
        assert_eq!(
            super::suggested_name("../../.bashrc", Some("https://example.com/x")),
            "bashrc"
        );
    }
}
