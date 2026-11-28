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
mod tab_actions;
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

const GST_AUTODETECT_PLUGIN: &str = "libgstautodetect.so";

const DEFAULT_GST_PLUGIN_DIRS: &[&str] = &[
    "/usr/lib/gstreamer-1.0",
    "/usr/lib/x86_64-linux-gnu/gstreamer-1.0",
    "/usr/lib64/gstreamer-1.0",
    "/usr/lib/aarch64-linux-gnu/gstreamer-1.0",
    "/usr/local/lib/gstreamer-1.0",
    "/app/lib/gstreamer-1.0",
];

#[derive(Debug, PartialEq, Eq)]
enum AudioCrashGuard {
    Available,
    AddPluginDir(std::path::PathBuf),
    EnableAudioMixerFallback,
    RespectExplicitOverride,
}

fn split_env_paths(value: Option<&std::ffi::OsStr>) -> Vec<std::path::PathBuf> {
    value
        .map(|raw| {
            std::env::split_paths(raw)
                .filter(|path| !path.as_os_str().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn gst_active_plugin_dirs(
    plugin_path_1_0: Option<&std::ffi::OsStr>,
    plugin_path: Option<&std::ffi::OsStr>,
    system_path_1_0: Option<&std::ffi::OsStr>,
    system_path: Option<&std::ffi::OsStr>,
) -> Vec<std::path::PathBuf> {
    let mut dirs = split_env_paths(plugin_path_1_0);
    dirs.extend(split_env_paths(plugin_path));
    if system_path_1_0.is_some() || system_path.is_some() {
        dirs.extend(split_env_paths(system_path_1_0));
        dirs.extend(split_env_paths(system_path));
    } else {
        dirs.extend(DEFAULT_GST_PLUGIN_DIRS.iter().map(std::path::PathBuf::from));
    }
    dirs
}

fn exe_adjacent_gst_dirs(exe: Option<&std::path::Path>) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    if let Some(bin_dir) = exe.and_then(|path| path.parent()) {
        dirs.push(bin_dir.join("gstreamer-1.0"));
        if let Some(prefix) = bin_dir.parent() {
            dirs.push(prefix.join("lib").join("gstreamer-1.0"));
            dirs.push(prefix.join("lib64").join("gstreamer-1.0"));
        }
    }
    dirs
}

fn decide_audio_crash_guard(
    active_dirs: &[std::path::PathBuf],
    adjacent_dirs: &[std::path::PathBuf],
    explicit_mixer_env: Option<&std::ffi::OsStr>,
) -> AudioCrashGuard {
    if active_dirs
        .iter()
        .any(|dir| dir.join(GST_AUTODETECT_PLUGIN).is_file())
    {
        return AudioCrashGuard::Available;
    }
    if let Some(dir) = adjacent_dirs
        .iter()
        .find(|dir| dir.join(GST_AUTODETECT_PLUGIN).is_file())
    {
        return AudioCrashGuard::AddPluginDir(dir.clone());
    }
    if explicit_mixer_env.is_some() {
        return AudioCrashGuard::RespectExplicitOverride;
    }
    AudioCrashGuard::EnableAudioMixerFallback
}

fn configure_gst_audio_crash_guard() {
    let active_dirs = gst_active_plugin_dirs(
        std::env::var_os("GST_PLUGIN_PATH_1_0").as_deref(),
        std::env::var_os("GST_PLUGIN_PATH").as_deref(),
        std::env::var_os("GST_PLUGIN_SYSTEM_PATH_1_0").as_deref(),
        std::env::var_os("GST_PLUGIN_SYSTEM_PATH").as_deref(),
    );
    let exe = std::env::current_exe().ok();
    let adjacent_dirs = exe_adjacent_gst_dirs(exe.as_deref());
    let explicit_mixer = std::env::var_os("WEBKIT_GST_ENABLE_AUDIO_MIXER");
    match decide_audio_crash_guard(&active_dirs, &adjacent_dirs, explicit_mixer.as_deref()) {
        AudioCrashGuard::Available | AudioCrashGuard::RespectExplicitOverride => {}
        AudioCrashGuard::AddPluginDir(dir) => {
            let mut paths = vec![dir];
            paths.extend(split_env_paths(std::env::var_os("GST_PLUGIN_PATH_1_0").as_deref()));
            if let Ok(joined) = std::env::join_paths(paths) {
                std::env::set_var("GST_PLUGIN_PATH_1_0", joined);
            }
        }
        AudioCrashGuard::EnableAudioMixerFallback => {
            // WebKitGTK 2.54.x's WebKitAudioSink calls
            // gst_element_factory_make("autoaudiosink", ...) and crashes the
            // WebKitWebProcess with a NULL dereference when gst-plugins-good
            // (libgstautodetect.so) is absent on the host. Routing through
            // WebKit's audio mixer path handles the missing sink cleanly
            // without killing media pages.
            std::env::set_var("WEBKIT_GST_ENABLE_AUDIO_MIXER", "1");
            note!(
                "search: GStreamer autoaudiosink plugin ({GST_AUTODETECT_PLUGIN}) not found; enabling WebKit audio mixer fallback to prevent media tab crashes"
            );
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum DmabufPolicy {
    /// Caller set `WEBKIT_DISABLE_DMABUF_RENDERER` explicitly; preserve it untouched.
    RespectExplicitOverride,
    /// Leave WebKitGTK's default DMA-BUF renderer behavior untouched.
    KeepDefault,
    /// Caller opted out via `SEARCH_ENABLE_DMABUF=0` or `SEARCH_DISABLE_DMABUF=1`.
    DisableByUserOptOut,
    /// Conservative automatic fallback for proprietary NVIDIA drivers on Wayland.
    DisableForNvidiaWayland,
}

fn env_truthy(value: Option<&std::ffi::OsStr>) -> Option<bool> {
    let raw = value?.to_str()?.trim();
    if raw.is_empty() {
        return None;
    }
    if raw == "1" || raw.eq_ignore_ascii_case("true") || raw.eq_ignore_ascii_case("yes") {
        Some(true)
    } else if raw == "0" || raw.eq_ignore_ascii_case("false") || raw.eq_ignore_ascii_case("no") {
        Some(false)
    } else {
        None
    }
}

fn is_wayland_session(
    gdk_backend: Option<&std::ffi::OsStr>,
    wayland_display: Option<&std::ffi::OsStr>,
    display: Option<&std::ffi::OsStr>,
    xdg_session_type: Option<&std::ffi::OsStr>,
) -> bool {
    let has_wayland_display = wayland_display.is_some_and(|value| !value.is_empty());
    let has_x11_display = display.is_some_and(|value| !value.is_empty());
    let session_is_wayland = xdg_session_type
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("wayland"));

    if let Some(raw_backend) = gdk_backend.and_then(|value| value.to_str()) {
        let trimmed = raw_backend.trim();
        if !trimmed.is_empty() {
            for backend in trimmed.split(',').map(str::trim).filter(|s| !s.is_empty()) {
                if backend.eq_ignore_ascii_case("x11") {
                    return false;
                }
                if backend.eq_ignore_ascii_case("wayland") || backend == "*" {
                    return has_wayland_display || (session_is_wayland && !has_x11_display);
                }
            }
            return false;
        }
    }

    has_wayland_display || (session_is_wayland && !has_x11_display)
}

fn detect_drm_drivers_in(drm_root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(drm_root) else {
        return Vec::new();
    };
    let mut drivers = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name_str) = name.to_str() else {
            continue;
        };
        // Only inspect primary DRM card devices (card0, card1, ...), not connectors (card0-DP-1).
        if !name_str.starts_with("card") || name_str.contains('-') {
            continue;
        }
        let driver_link = entry.path().join("device").join("driver");
        if let Ok(target) = std::fs::read_link(&driver_link) {
            if let Some(driver_name) = target.file_name().and_then(|n| n.to_str()) {
                let driver = driver_name.to_string();
                if !drivers.contains(&driver) {
                    drivers.push(driver);
                }
            }
        }
    }
    drivers
}

fn is_nvidia_proprietary_active(
    drm_drivers: &[String],
    gbm_backend: Option<&std::ffi::OsStr>,
    glx_vendor: Option<&std::ffi::OsStr>,
    nv_prime_offload: Option<&std::ffi::OsStr>,
) -> bool {
    let explicit_gbm_nvidia = gbm_backend
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("nvidia-drm"));
    let explicit_glx_nvidia = glx_vendor
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.trim().eq_ignore_ascii_case("nvidia"));
    let explicit_prime_offload = env_truthy(nv_prime_offload) == Some(true);
    if explicit_gbm_nvidia || explicit_glx_nvidia || explicit_prime_offload {
        return true;
    }

    let has_nvidia = drm_drivers.iter().any(|driver| driver == "nvidia");
    if !has_nvidia {
        return false;
    }
    // On hybrid laptops where Intel/AMD drives the compositor and NVIDIA is idle
    // without PRIME offload, leave WebKit's default DMA-BUF path enabled.
    let has_other_hw_gpu = drm_drivers
        .iter()
        .any(|driver| matches!(driver.as_str(), "i915" | "xe" | "amdgpu" | "radeon"));
    !has_other_hw_gpu
}

fn decide_dmabuf_policy(
    webkit_env: Option<&std::ffi::OsStr>,
    enable_dmabuf_env: Option<&std::ffi::OsStr>,
    disable_dmabuf_env: Option<&std::ffi::OsStr>,
    bench_mode: bool,
    wayland_session: bool,
    nvidia_proprietary_active: bool,
) -> DmabufPolicy {
    if webkit_env.is_some_and(|value| !value.is_empty()) {
        return DmabufPolicy::RespectExplicitOverride;
    }
    match env_truthy(enable_dmabuf_env) {
        Some(true) => return DmabufPolicy::KeepDefault,
        Some(false) => return DmabufPolicy::DisableByUserOptOut,
        None => {}
    }
    if env_truthy(disable_dmabuf_env) == Some(true) {
        return DmabufPolicy::DisableByUserOptOut;
    }
    // Keep headless CI / bench runs deterministic regardless of host GPU hardware.
    if bench_mode {
        return DmabufPolicy::KeepDefault;
    }
    if wayland_session && nvidia_proprietary_active {
        return DmabufPolicy::DisableForNvidiaWayland;
    }
    DmabufPolicy::KeepDefault
}

fn configure_dmabuf_policy() {
    let webkit_env = std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER");
    let enable_dmabuf_env = std::env::var_os("SEARCH_ENABLE_DMABUF");
    let disable_dmabuf_env = std::env::var_os("SEARCH_DISABLE_DMABUF");
    let bench_mode = bench::enabled();
    let wayland_session = is_wayland_session(
        std::env::var_os("GDK_BACKEND").as_deref(),
        std::env::var_os("WAYLAND_DISPLAY").as_deref(),
        std::env::var_os("DISPLAY").as_deref(),
        std::env::var_os("XDG_SESSION_TYPE").as_deref(),
    );
    let probe_gpu = !bench_mode
        && wayland_session
        && webkit_env.as_deref().is_none_or(|value| value.is_empty())
        && env_truthy(enable_dmabuf_env.as_deref()).is_none()
        && env_truthy(disable_dmabuf_env.as_deref()) != Some(true);
    let nvidia_active = if probe_gpu {
        let drm_drivers = detect_drm_drivers_in(std::path::Path::new("/sys/class/drm"));
        is_nvidia_proprietary_active(
            &drm_drivers,
            std::env::var_os("GBM_BACKEND").as_deref(),
            std::env::var_os("__GLX_VENDOR_LIBRARY_NAME").as_deref(),
            std::env::var_os("__NV_PRIME_RENDER_OFFLOAD").as_deref(),
        )
    } else {
        false
    };

    match decide_dmabuf_policy(
        webkit_env.as_deref(),
        enable_dmabuf_env.as_deref(),
        disable_dmabuf_env.as_deref(),
        bench_mode,
        wayland_session,
        nvidia_active,
    ) {
        DmabufPolicy::RespectExplicitOverride | DmabufPolicy::KeepDefault => {}
        DmabufPolicy::DisableByUserOptOut => {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        }
        DmabufPolicy::DisableForNvidiaWayland => {
            std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
            note!(
                "search: detected proprietary NVIDIA driver on Wayland; setting WEBKIT_DISABLE_DMABUF_RENDERER=1 (override with WEBKIT_DISABLE_DMABUF_RENDERER=0 or SEARCH_ENABLE_DMABUF=1)"
            );
        }
    }
}

fn main() {
    // WebKit's own knobs, set before the engine starts.
    //
    // Capping the allocator's arenas keeps it from holding per-thread pools it
    // never returns. DMA-BUF rendering is left at WebKitGTK's default unless a
    // known-bad driver/session combination is detected (proprietary NVIDIA on
    // Wayland) or the user opts out via WEBKIT_DISABLE_DMABUF_RENDERER=1 /
    // SEARCH_DISABLE_DMABUF=1 / SEARCH_ENABLE_DMABUF=0: forcing
    // WEBKIT_DISABLE_DMABUF_RENDERER=1 globally triggers deterministic UI-process
    // SIGSEGV (WebKitGTK 2.50.x) and WebProcess crashes (WebKitGTK 2.54.x) on
    // non-DMABUF fallback paths.
    std::env::set_var("MALLOC_ARENA_MAX", "2");
    // The low-memory switch disables the compositing path. WebKit reads
    // these variables once, when its process is forked, so the setting in the
    // browser has to be read here, before the first window.
    //
    // The preferences file is read once, here, and the same value is handed to
    // the window below. Reading it a second time after GTK started parsed the
    // same file twice on every launch, and a file changed between the two
    // reads could give the engine one low-memory setting and the window
    // another.
    let mut prefs = store::Prefs::load();
    configure_gst_audio_crash_guard();
    configure_dmabuf_policy();
    let low_memory = std::env::var_os("SEARCH_LOW_MEMORY").is_some() || prefs.low_memory;
    if low_memory {
        if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
            std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        }
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
    use super::{
        decide_audio_crash_guard, decide_dmabuf_policy, detect_drm_drivers_in,
        exe_adjacent_gst_dirs, flag_value, gst_active_plugin_dirs, is_nvidia_proprietary_active,
        is_wayland_session, AudioCrashGuard, DmabufPolicy, GST_AUTODETECT_PLUGIN,
    };
    use std::ffi::OsStr;

    #[test]
    fn flag_value_parses_space_and_equals_forms() {
        let space = vec!["search".into(), "--appearance".into(), "dark".into()];
        let equals = vec!["search".into(), "--appearance=dark".into()];
        assert_eq!(flag_value(&space, "--appearance").as_deref(), Some("dark"));
        assert_eq!(flag_value(&equals, "--appearance").as_deref(), Some("dark"));
    }

    #[test]
    fn dmabuf_policy_keeps_webkit_default_and_respects_overrides() {
        // Default on normal desktops (no known-bad driver/session): leave WebKit default alone.
        assert_eq!(
            decide_dmabuf_policy(None, None, None, false, false, false),
            DmabufPolicy::KeepDefault
        );
        assert_eq!(
            decide_dmabuf_policy(None, None, None, false, true, false),
            DmabufPolicy::KeepDefault
        );
        // Proprietary NVIDIA on X11 (e.g., Xorg / Xvfb) does not trigger the Wayland fallback.
        assert_eq!(
            decide_dmabuf_policy(None, None, None, false, false, true),
            DmabufPolicy::KeepDefault
        );
        // Proprietary NVIDIA on Wayland triggers conservative fallback unless overridden.
        assert_eq!(
            decide_dmabuf_policy(None, None, None, false, true, true),
            DmabufPolicy::DisableForNvidiaWayland
        );
        // Headless CI / bench stays deterministic regardless of host GPU/session state.
        assert_eq!(
            decide_dmabuf_policy(None, None, None, true, true, true),
            DmabufPolicy::KeepDefault
        );
        // Explicit WEBKIT_DISABLE_DMABUF_RENDERER always takes precedence untouched.
        assert_eq!(
            decide_dmabuf_policy(Some(OsStr::new("0")), None, Some(OsStr::new("1")), false, true, true),
            DmabufPolicy::RespectExplicitOverride
        );
        assert_eq!(
            decide_dmabuf_policy(Some(OsStr::new("1")), Some(OsStr::new("1")), None, true, false, false),
            DmabufPolicy::RespectExplicitOverride
        );
        // Explicit Rill env overrides allow opting in or out cleanly.
        assert_eq!(
            decide_dmabuf_policy(None, Some(OsStr::new("1")), None, false, true, true),
            DmabufPolicy::KeepDefault
        );
        assert_eq!(
            decide_dmabuf_policy(None, Some(OsStr::new("0")), None, false, false, false),
            DmabufPolicy::DisableByUserOptOut
        );
        assert_eq!(
            decide_dmabuf_policy(None, None, Some(OsStr::new("1")), true, false, false),
            DmabufPolicy::DisableByUserOptOut
        );
    }

    #[test]
    fn dmabuf_wayland_session_detection_respects_gdk_backend_and_xvfb() {
        assert!(!is_wayland_session(
            Some(OsStr::new("x11")),
            Some(OsStr::new("wayland-0")),
            Some(OsStr::new(":99")),
            Some(OsStr::new("wayland")),
        ));
        assert!(!is_wayland_session(
            Some(OsStr::new("x11,wayland")),
            Some(OsStr::new("wayland-0")),
            Some(OsStr::new(":0")),
            Some(OsStr::new("wayland")),
        ));
        assert!(is_wayland_session(
            Some(OsStr::new("wayland,x11,*")),
            Some(OsStr::new("wayland-0")),
            Some(OsStr::new(":0")),
            Some(OsStr::new("wayland")),
        ));
        assert!(!is_wayland_session(
            Some(OsStr::new("wayland,x11,*")),
            None,
            Some(OsStr::new(":99")),
            Some(OsStr::new("wayland")),
        ));
        assert!(is_wayland_session(
            None,
            Some(OsStr::new("wayland-0")),
            None,
            Some(OsStr::new("wayland")),
        ));
        assert!(!is_wayland_session(
            None,
            None,
            Some(OsStr::new(":99")),
            Some(OsStr::new("x11")),
        ));
    }

    #[test]
    fn dmabuf_nvidia_detection_is_conservative_for_hybrid_and_non_nvidia_hosts() {
        let pure_nvidia = vec!["nvidia".to_string()];
        let hybrid = vec!["i915".to_string(), "nvidia".to_string()];
        let intel_evdi = vec!["evdi".to_string(), "i915".to_string()];
        let nouveau = vec!["nouveau".to_string()];

        assert!(is_nvidia_proprietary_active(&pure_nvidia, None, None, None));
        assert!(!is_nvidia_proprietary_active(&hybrid, None, None, None));
        assert!(is_nvidia_proprietary_active(
            &hybrid,
            None,
            None,
            Some(OsStr::new("1"))
        ));
        assert!(is_nvidia_proprietary_active(
            &hybrid,
            Some(OsStr::new("nvidia-drm")),
            None,
            None
        ));
        assert!(is_nvidia_proprietary_active(
            &hybrid,
            None,
            Some(OsStr::new("nvidia")),
            None
        ));
        assert!(!is_nvidia_proprietary_active(&intel_evdi, None, None, None));
        assert!(!is_nvidia_proprietary_active(&nouveau, None, None, None));
        assert!(!is_nvidia_proprietary_active(&[], None, None, None));
    }

    #[test]
    fn detect_drm_drivers_reads_card_symlinks_and_skips_connectors() {
        let root = std::env::temp_dir().join(format!(
            "search-drm-detect-test-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let card0_dev = root.join("card0").join("device");
        let card1_dev = root.join("card1").join("device");
        let connector_dev = root.join("card0-DP-1").join("device");
        let fake_i915 = root.join("drivers").join("i915");
        let fake_nvidia = root.join("drivers").join("nvidia");
        std::fs::create_dir_all(&card0_dev).unwrap();
        std::fs::create_dir_all(&card1_dev).unwrap();
        std::fs::create_dir_all(&connector_dev).unwrap();
        std::fs::create_dir_all(&fake_i915).unwrap();
        std::fs::create_dir_all(&fake_nvidia).unwrap();
        std::os::unix::fs::symlink(&fake_i915, card0_dev.join("driver")).unwrap();
        std::os::unix::fs::symlink(&fake_i915, card1_dev.join("driver")).unwrap();
        std::os::unix::fs::symlink(&fake_nvidia, connector_dev.join("driver")).unwrap();

        let drivers = detect_drm_drivers_in(&root);
        assert_eq!(drivers, vec!["i915".to_string()]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn audio_crash_guard_detects_plugin_or_enables_safe_fallback() {
        let root = std::env::temp_dir().join(format!(
            "search-gst-guard-test-{}",
            std::process::id()
        ));
        let empty_dir = root.join("empty");
        let bundled_dir = root.join("bundled");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&empty_dir).unwrap();
        std::fs::create_dir_all(&bundled_dir).unwrap();
        std::fs::write(bundled_dir.join(GST_AUTODETECT_PLUGIN), b"stub").unwrap();

        // Missing everywhere -> enable audio mixer fallback unless caller explicitly overrode it.
        assert_eq!(
            decide_audio_crash_guard(std::slice::from_ref(&empty_dir), &[], None),
            AudioCrashGuard::EnableAudioMixerFallback
        );
        assert_eq!(
            decide_audio_crash_guard(
                std::slice::from_ref(&empty_dir),
                &[],
                Some(OsStr::new("0"))
            ),
            AudioCrashGuard::RespectExplicitOverride
        );

        // Present in active GStreamer plugin search path -> Available (no fallback needed).
        assert_eq!(
            decide_audio_crash_guard(std::slice::from_ref(&bundled_dir), &[], None),
            AudioCrashGuard::Available
        );

        // Absent in active dirs but present next to executable -> AddPluginDir.
        assert_eq!(
            decide_audio_crash_guard(
                std::slice::from_ref(&empty_dir),
                std::slice::from_ref(&bundled_dir),
                None
            ),
            AudioCrashGuard::AddPluginDir(bundled_dir.clone())
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn gst_active_plugin_dirs_respects_system_path_override_and_adjacent_layout() {
        let custom = gst_active_plugin_dirs(
            Some(OsStr::new("/opt/gst-a:/opt/gst-b")),
            None,
            Some(OsStr::new("")),
            None,
        );
        assert_eq!(
            custom,
            vec![
                std::path::PathBuf::from("/opt/gst-a"),
                std::path::PathBuf::from("/opt/gst-b"),
            ]
        );
        let adjacent = exe_adjacent_gst_dirs(Some(std::path::Path::new("/opt/rill/bin/search")));
        assert!(adjacent.contains(&std::path::PathBuf::from("/opt/rill/lib/gstreamer-1.0")));
    }
}
