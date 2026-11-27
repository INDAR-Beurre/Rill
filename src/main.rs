// Rill: a small, fast, quiet web browser for Linux.
//
// The engine is the one the desktop already ships (WebKitGTK), so there is no
// second copy of a browser inside this one and the whole thing starts in the
// time it takes to draw a window.
// Every note to the terminal goes through this, not eprintln!. eprintln!
// panics when stderr cannot be written, and the release profile has
// panic = "abort", so a full disk under the log file (or stderr on /dev/full)
// killed the browser with SIGABRT and no message. A note that cannot be
// written is dropped; the browser keeps running. Defined before the `mod`
// lines so every module can use it.
macro_rules! note {
    ($($arg:tt)*) => {{
        use std::io::Write as _;
        let _ = writeln!(std::io::stderr(), $($arg)*);
    }};
}

mod addons_catalog;
mod app;
mod bench;
mod css;
mod curtain;
mod customize;
mod decor;
mod files;
mod extensions;
mod find;
mod peek;
mod permissions;
mod dialogs;
mod gtkx;
mod icons;
mod js;
mod keys;
mod layer;
mod motion;
mod omnibox;
mod palette;
mod panels;
mod reader;
mod menu;
mod fuzzy;
mod keyword;
mod import;
mod welcome;
mod shield;
mod sitecard;
mod spaces;
mod status;
mod shield_assets;
mod store;
mod suggest;
mod startpage;
mod ui;
mod web;

use gtk::prelude::*;

fn flag_value(args: &[String], name: &str) -> Option<String> {
    let mut iterator = args.iter();
    while let Some(argument) = iterator.next() {
        if argument == name {
            return iterator.next().cloned();
        }
        if let Some(rest) = argument.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|argument| argument == name)
}

fn should_disable_dmabuf(
    low_memory: bool,
    enable_dmabuf_env: Option<&std::ffi::OsStr>,
    webkit_env: Option<&std::ffi::OsStr>,
) -> bool {
    if low_memory {
        return true;
    }
    if enable_dmabuf_env == Some(std::ffi::OsStr::new("1")) {
        return false;
    }
    if webkit_env == Some(std::ffi::OsStr::new("0")) {
        return false;
    }
    true
}

fn main() {
    // WebKit's own knobs, set before the engine starts.
    //
    // Capping the allocator's arenas keeps it from holding per-thread pools it
    // never returns. WebKitGTK's Wayland DMA-BUF renderer adds ~85 MB per
    // process and stalls multi-tab page loads on several Linux compositors, so
    // it stays off by default (GPU compositing via HardwareAccelerationPolicy
    // remains on) unless opted in with SEARCH_ENABLE_DMABUF=1 or
    // WEBKIT_DISABLE_DMABUF_RENDERER=0.
    std::env::set_var("MALLOC_ARENA_MAX", "2");
    // The low-memory switch also disables the compositing path. WebKit reads
    // these variables once, when its process is forked, so the setting in the
    // browser has to be read here, before the first window.
    //
    // The preferences file is read once, here, and the same value is handed to
    // the window below. Reading it a second time after GTK started parsed the
    // same file twice on every launch, and a file changed between the two
    // reads could give the engine one low-memory setting and the window
    // another.
    let mut prefs = store::Prefs::load();
    let low_memory = std::env::var_os("SEARCH_LOW_MEMORY").is_some() || prefs.low_memory;
    if should_disable_dmabuf(
        low_memory,
        std::env::var_os("SEARCH_ENABLE_DMABUF").as_deref(),
        std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").as_deref(),
    ) {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if low_memory {
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        // Settings in the page engine (page cache, cache model, spellcheck)
        // read the profile from here, so the saved preference and the
        // environment switch reach them the same way, and the preferences
        // file is not read again for it.
        std::env::set_var("SEARCH_LOW_MEMORY", "1");
    }

    glib::set_prgname(Some("search"));
    glib::set_application_name("Rill");

    if gtk::init().is_err() {
        note!("search: could not open a display");
        std::process::exit(1);
    }
    // Bench runs are deterministic by default: motion completes immediately
    // so pixel gates read a settled frame. A motion probe opts back in with
    // SEARCH_BENCH_ANIMATE=1 and still gets the scriptable bench socket.
    if std::env::var_os("SEARCH_BENCH").is_some()
        && std::env::var_os("SEARCH_BENCH_ANIMATE").is_none()
    {
        if let Some(settings) = gtk::Settings::default() {
            settings.set_gtk_enable_animations(false);
        }
    }

    // A memory limit for the network process. WebKit reads it only once, when
    // the first website data manager is made, so it is set here: after GTK is
    // initialised (the binding's MemoryPressureSettings::new() asserts that,
    // and panicked with "GTK has not been initialized" when this block ran
    // earlier) and before any window or context exists. Without a limit,
    // WebKit's own threshold is a share of the machine's RAM, so a spray of
    // tabs grew to 1141 MB at its peak before anything was released. At 384 MB
    // the process starts freeing caches at a third of the limit (conservative)
    // and frees all it can at half (strict), checking every 5 seconds. Page
    // processes get the same numbers from web::page_memory_pressure() when
    // their WebContext is built.
    webkit2gtk::WebsiteDataManager::set_memory_pressure_settings(&mut web::page_memory_pressure());

    let args: Vec<String> = std::env::args().collect();
    let shot = flag_value(&args, "--shot");
    let url = flag_value(&args, "--url");
    let size = flag_value(&args, "--size");
    let appearance = flag_value(&args, "--appearance");
    let fresh = flag(&args, "--fresh");

    if fresh {
        prefs.skip_restore = true;
    }
    if let Some(look) = appearance {
        prefs.appearance = match look.to_lowercase().as_str() {
            "dark" => store::Look::Dark,
            "light" => store::Look::Light,
            _ => store::Look::System,
        };
    }
    if url.is_some() {
        prefs.skip_restore = true;
    }
    // The window is mapped inside `start`; setting its size after that point
    // lets the window manager keep its first allocation (820px on this host).
    if let Some(size) = size {
        let parts: Vec<i32> = size.split('x').filter_map(|part| part.trim().parse().ok()).collect();
        if parts.len() == 2 {
            prefs.window_size = (parts[0], parts[1]);
        }
    }

    // `start` installs the handle itself, and that install is what arms the
    // memory tick, so doing it again here armed a second one.
    let app = app::start(prefs);

    if bench::enabled() {
        bench::watch_loop();
        // Not eprintln!: it panics when stderr cannot be written, and the
        // release profile has panic = "abort". With stderr on a full disk (or
        // /dev/full), this line killed the browser with SIGABRT just after the
        // socket opened, so the first ping got "Connection reset by peer".
        // A note that cannot be written is dropped; the browser keeps running.
        use std::io::Write as _;
        let _ = match bench::serve(&app) {
            Ok(path) => writeln!(std::io::stderr(), "search: bench socket at {}", path.display()),
            Err(error) => writeln!(std::io::stderr(), "search: bench socket did not open: {error}"),
        };
    }

    app.ui.window.show_all();
    // show_all shows every child that did not opt out; enforce exclusive tab
    // layout (never both topbar and vertical sidebar) and bookmarks/toolbar
    // visibility immediately afterwards.
    app.sync_tab_layout();
    app.render();
    app.ui.window.present();
    // Only show full-tab first-run introduction on a blank normal startup tab;
    // never hijack a restored web-page session or explicit --url launch.
    if !fresh
        && !app.prefs.borrow().welcomed
        && app
            .state
            .borrow()
            .active_tab()
            .is_none_or(|t| t.url.is_empty() && !t.private)
    {
        welcome::show(&app);
    }

    {
        let app_weak = app.weak();
        app.ui.window.connect_destroy(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.save_session();
                let (history, hidden, spaces) = {
                    let state = app.state.borrow();
                    (state.history.clone(), state.hidden.clone(), state.spaces.clone())
                };
                store::Records::save_spaces(&spaces);
                store::Records::save_history(&history);
                store::Records::save_hidden(&hidden);
            }
            gtk::main_quit();
        });
    }
    {
        let app_weak = app.weak();
        app.ui.window.connect_delete_event(move |_, _| {
            if let Some(app) = app_weak.upgrade() {
                app.save_session();
            }
            crate::gtkx::inh(false)
        });
    }
    {
        let window = app.ui.window.clone();
        window.connect_notify_local(Some("is-active"), move |window, _| {
            if window.is_active() {
                window.style_context().remove_class("unfocused");
            } else {
                window.style_context().add_class("unfocused");
            }
        });
    }

    if let Some(url) = url {
        let app_weak = app.weak();
        glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
            if let Some(app) = app_weak.upgrade() {
                app.close_omni();
                app.navigate_active(&url);
            }
            glib::ControlFlow::Break
        });
    }

    if let Some(path) = shot {
        let app_weak = app.weak();
        let waited = std::cell::Cell::new(0u32);
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            let Some(app) = app_weak.upgrade() else { return glib::ControlFlow::Break };
            waited.set(waited.get() + 1);
            let ready = app.ui.window.allocated_width() > 100
                && app.ui.window.allocated_height() > 100
                && waited.get() >= 10;
            if ready || waited.get() >= 40 {
                let path = std::path::PathBuf::from(&path);
                match app.capture(&path) {
                    Ok(()) => note!(
                        "search: wrote {} ({}x{})",
                        path.display(),
                        app.ui.window.allocated_width(),
                        app.ui.window.allocated_height()
                    ),
                    Err(error) => note!("search: could not write the picture: {error}"),
                }
                app.ui.window.close();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    gtk::main();
}

#[cfg(test)]
mod tests {
    use super::{flag_value, should_disable_dmabuf};
    use std::ffi::OsStr;

    #[test]
    fn flag_value_parses_space_and_equals_forms() {
        let space = vec!["search".into(), "--appearance".into(), "dark".into()];
        let equals = vec!["search".into(), "--appearance=dark".into()];
        assert_eq!(flag_value(&space, "--appearance").as_deref(), Some("dark"));
        assert_eq!(flag_value(&equals, "--appearance").as_deref(), Some("dark"));
    }

    #[test]
    fn dmabuf_is_disabled_by_default_unless_explicitly_opted_in() {
        assert!(should_disable_dmabuf(false, None, None));
        assert!(!should_disable_dmabuf(false, Some(OsStr::new("1")), None));
        assert!(!should_disable_dmabuf(false, None, Some(OsStr::new("0"))));
        assert!(should_disable_dmabuf(true, Some(OsStr::new("1")), Some(OsStr::new("0"))));
    }
}
