// Search: a small, fast, quiet web browser for Linux.
//
// The engine is the one the desktop already ships (WebKitGTK), so there is no
// second copy of a browser inside this one and the whole thing starts in the
// time it takes to draw a window.
mod app;
mod bench;
mod css;
mod curtain;
mod files;
mod find;
mod gtkx;
mod icons;
mod js;
mod keys;
mod layer;
mod omnibox;
mod palette;
mod panels;
mod reader;
mod shield;
mod store;
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

fn main() {
    // WebKit's own knobs, set before the engine starts.
    //
    // Low-memory mode trades the DMA-BUF renderer and compositing for a
    // smaller footprint; off by default because the path it skips is the one
    // that draws pages best on machines that have it.
    if std::env::var_os("SEARCH_LOW_MEMORY").is_some() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
        // Fewer malloc arenas means less memory the allocator never gives back.
        // It also means more contention between the threads that use them.
        std::env::set_var("MALLOC_ARENA_MAX", "2");
    }

    // A page's light or dark comes from the toolkit's own preference, so the
    // toolkit is told what the browser is set to before it starts.
    if std::env::var_os("GTK_THEME").is_none() {
        match std::env::args()
            .collect::<Vec<String>>()
            .iter()
            .position(|argument| argument == "--appearance")
            .and_then(|index| std::env::args().nth(index + 1))
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "light" => std::env::set_var("GTK_THEME", "Adwaita"),
            "dark" => std::env::set_var("GTK_THEME", "Adwaita:dark"),
            _ => {}
        }
    }

    if gtk::init().is_err() {
        eprintln!("search: could not open a display");
        std::process::exit(1);
    }

    let args: Vec<String> = std::env::args().collect();
    let shot = flag_value(&args, "--shot");
    let url = flag_value(&args, "--url");
    let size = flag_value(&args, "--size");
    let appearance = flag_value(&args, "--appearance");
    let fresh = flag(&args, "--fresh");

    let mut prefs = store::Prefs::load();
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

    let app = app::start(prefs);
    app::install_self(&app);

    if bench::enabled() {
        match bench::serve(&app) {
            Ok(path) => eprintln!("search: bench socket at {}", path.display()),
            Err(error) => eprintln!("search: bench socket did not open: {error}"),
        }
    }

    app.ui.window.show_all();
    app.ui.window.present();

    {
        let app_weak = app.weak();
        app.ui.window.connect_destroy(move |_| {
            if let Some(app) = app_weak.upgrade() {
                app.save_session();
                let (history, hidden) = {
                    let state = app.state.borrow();
                    (state.history.clone(), state.hidden.clone())
                };
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

    if let Some(size) = size {
        let parts: Vec<i32> = size.split('x').filter_map(|part| part.trim().parse().ok()).collect();
        if parts.len() == 2 {
            app.ui.window.resize(parts[0], parts[1]);
        }
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
                    Ok(()) => eprintln!(
                        "search: wrote {} ({}x{})",
                        path.display(),
                        app.ui.window.allocated_width(),
                        app.ui.window.allocated_height()
                    ),
                    Err(error) => eprintln!("search: could not write the picture: {error}"),
                }
                app.ui.window.close();
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        });
    }

    gtk::main();
}
