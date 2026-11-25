// A socket a script can drive the browser through, as the Mac build has: a
// test that needs a real window still needs a way to open a tab, read the page
// and ask the browser what it is showing.
//
// Line protocol over a Unix socket under the user's runtime folder. One
// command per line, one JSON answer per line.
use crate::app::AppRef;
use crate::keys::Intent;
use gtk::prelude::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SEARCH_BENCH_SOCKET") {
        return PathBuf::from(path);
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("search-bench.sock")
}

// A main loop that stops answering for ten seconds is indistinguishable from a
// hung browser from outside, and measuring it from outside cannot say which
// side stopped. This tick runs on the loop itself: the longest gap between two
// ticks is the longest time the loop was unavailable.
static LAST_TICK: Mutex<Option<Instant>> = Mutex::new(None);
static WORST_GAP_MS: Mutex<u64> = Mutex::new(0);
static GAPS: Mutex<Vec<u64>> = Mutex::new(Vec::new());
static TICKS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn watch_loop() {
    if let Ok(mut last) = LAST_TICK.lock() {
        *last = Some(Instant::now());
    }
    glib::timeout_add_local(Duration::from_millis(50), move || {
        TICKS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let now = Instant::now();
        if let Ok(mut last) = LAST_TICK.lock() {
            if let Some(previous) = *last {
                let gap = now.duration_since(previous).as_millis() as u64;
                if gap > 120 {
                    if let Ok(mut worst) = WORST_GAP_MS.lock() {
                        *worst = (*worst).max(gap);
                    }
                    if let Ok(mut gaps) = GAPS.lock() {
                        if gaps.len() < 40 {
                            gaps.push(gap);
                        }
                    }
                }
            }
            *last = Some(now);
        }
        glib::ControlFlow::Continue
    });
}

pub fn enabled() -> bool {
    std::env::var_os("SEARCH_BENCH").is_some() || std::env::args().any(|argument| argument == "--bench")
}

pub fn serve(_app: &AppRef) -> std::io::Result<PathBuf> {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
    // The bench is a control surface, not a world-writable socket.
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            std::thread::spawn(move || session(stream));
        }
    });
    Ok(path)
}

fn session(stream: UnixStream) {
    let reader = BufReader::new(match stream.try_clone() {
        Ok(clone) => clone,
        Err(_) => return,
    });
    let mut writer = stream;
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }
        let response = run(&line);
        if writer.write_all(format!("{response}\n").as_bytes()).is_err() {
            break;
        }
        let _ = writer.flush();
    }
}

fn run(line: &str) -> String {
    let (command, argument) = match line.split_once(' ') {
        Some((command, argument)) => (command.to_string(), argument.trim().to_string()),
        None => (line.to_string(), String::new()),
    };
    let (sender, receiver) = mpsc::channel::<String>();
    let immediate = sender.clone();
    // Measured: raising this to HIGH priority deadlocks the suite at the first
    // `open`, because the command then runs inside the engine's own update
    // rather than between updates. Default (idle) priority is correct; the
    // 30-second wait below is the guard for a busy main loop.
    glib::MainContext::default().invoke(move || {
        let Some(app) = crate::app::current() else {
            let _ = immediate.send("{\"ok\":false,\"error\":\"no browser\"}".to_string());
            return;
        };
        match command.as_str() {
            // These two wait for the page, so they answer from the page's own
            // callback: the main loop has to stay free for that to arrive.
            "text" => page_text(&app, &sender),
            "js" => page_eval(&app, &argument, &sender),
            "peekjs" => peek_eval(&app, &argument, &sender),
            "pagefont" => {
                let code = if argument.is_empty() {
                    "getComputedStyle(document.body).fontSize".to_string()
                } else {
                    format!(
                        "(() => {{ document.body.style.fontSize = '{}px'; return getComputedStyle(document.body).fontSize; }})()",
                        argument.trim()
                    )
                };
                page_eval(&app, &code, &sender);
                return;
            }
            _ => {
                let answer = dispatch(&app, &command, &argument);
                let _ = immediate.send(answer);
            }
        }
    });
    match receiver.recv_timeout(Duration::from_secs(30)) {
        Ok(answer) => answer,
        Err(_) => "{\"ok\":false,\"error\":\"no answer in time\"}".to_string(),
    }
}

fn json_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 8);
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn ok(body: &str) -> String {
    format!("{{\"ok\":true{body}}}")
}

fn fail(message: &str) -> String {
    format!("{{\"ok\":false,\"error\":\"{}\"}}", json_escape(message))
}

fn tab_id(argument: &str) -> Option<u64> {
    argument.trim().parse().ok()
}

// One line of JSON is what every verb answers with, so the panel's text cannot
// be allowed to run to the length a full Shortcuts sheet has. This cap is well
// past the longest panel this browser builds; the walk stops collecting at it
// rather than answering with a megabyte of text.
const PANEL_TEXT_CAP: usize = 4000;

// What the open panel says, in the order it is drawn: every label under the
// body container, which is where a panel puts its rows. A button's caption is
// a label child, so the action a panel offers reads here too, and a test can
// check that the sheet lists the keys that work or that Settings says
// something true without knowing any widget name.
fn collect_labels(widget: &gtk::Widget, lines: &mut Vec<String>, budget: &mut usize) {
    if *budget == 0 {
        return;
    }
    if !widget.get_visible() {
        return;
    }
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        let text = label.text().to_string();
        if !text.trim().is_empty() {
            *budget = budget.saturating_sub(text.chars().count() + 1);
            lines.push(text);
        }
    }
    if let Some(container) = widget.downcast_ref::<gtk::Container>() {
        for child in container.children() {
            collect_labels(&child, lines, budget);
            if *budget == 0 {
                return;
            }
        }
    }
}

fn dispatch(app: &AppRef, command: &str, argument: &str) -> String {
    match command {
        "ping" => ok(&format!(",\"version\":\"{}\"", env!("CARGO_PKG_VERSION"))),
        "probe" => probe(app),
        "minsize" => {
            fn largest(widget: &gtk::Widget, depth: usize, out: &mut Vec<(i32, String)>) {
                let (minimum, natural) = widget.preferred_size();
                out.push((minimum.width, format!("{}{} {} min={} natural={}", "  ".repeat(depth), widget.widget_name(), widget.type_().name(), minimum.width, natural.width)));
                if let Ok(container) = widget.clone().downcast::<gtk::Container>() {
                    for child in container.children() {
                        largest(child.upcast_ref::<gtk::Widget>(), depth + 1, out);
                    }
                }
            }
            let mut out = Vec::new();
            largest(app.ui.window.upcast_ref::<gtk::Widget>(), 0, &mut out);
            out.sort_by(|a, b| b.0.cmp(&a.0));
            ok(&format!(",\"sizes\":[{}]", out.into_iter().take(30).map(|(w, n)| format!("[{},\"{}\"]", w, json_escape(&n))).collect::<Vec<_>>().join(",")))
        }
        "chrome" => chrome(app),
        "menuentries" => {
            let Some(menu) = browser_menu() else { return fail("no browser menu is open") };
            let items: Vec<_> = menu.children().into_iter().filter_map(|child| {
                child.downcast::<gtk::MenuItem>().ok().map(|item| serde_json::json!({"label":item.label().as_deref(), "sensitive":item.is_sensitive()}))
            }).collect();
            serde_json::json!({"ok":true,"items":items}).to_string()
        }
        "menupick" => {
            let Some(menu) = browser_menu() else { return fail("no browser menu is open") };
            let item = menu.children().into_iter().filter_map(|child| child.downcast::<gtk::MenuItem>().ok())
                .find(|item| item.label().as_deref() == Some(argument));
            let Some(item) = item else { return fail("no matching menu item") };
            if !item.is_sensitive() { return fail("menu item is disabled") }
            menu.activate_item(&item, true);
            ok("")
        }
        "uitree" => ui_tree(app),
        "starttext" => {
            let mut lines = Vec::new();
            let mut budget = PANEL_TEXT_CAP;
            collect_labels(app.ui.start.root.upcast_ref(), &mut lines, &mut budget);
            ok(&format!(",\"text\":\"{}\"", json_escape(&lines.join("\n"))))
        }
        "start" => {
            if app.ui.stack.visible_child_name().as_deref() != Some("blank") {
                return fail("start needs a new tab");
            }
            app.ui.start.search.set_text(argument);
            app.ui.start.search.emit_activate();
            ok("")
        }
        "theme" => {
            let look = match argument {
                "light" => crate::store::Look::Light,
                "dark" => crate::store::Look::Dark,
                "system" => crate::store::Look::System,
                _ => return fail("theme needs light, dark or system"),
            };
            app.switch_look(look);
            ok("")
        }
        "uiclick" => {
            let button = match argument {
                "newtab" => &app.ui.newtab,
                "private" => &app.ui.private_tab,
                "library" => &app.ui.library,
                "history" => &app.ui.history,
                "downloads" => &app.ui.downloads,
                "settings" => &app.ui.menu,
                "address" => &app.ui.address,
                "site" => &app.ui.site_info,
                "fold" => &app.ui.fold,
                "bookmark" => &app.ui.bookmark,
                "reader" => &app.ui.reader,
                "back" => &app.ui.back,
                "forward" => &app.ui.forward,
                "reload" => &app.ui.reload,
                "more" => &app.ui.more,
                _ => return fail("unknown chrome control"),
            };
            if !button.is_sensitive() { return fail("control is disabled"); }
            button.emit_clicked();
            ok("")
        }
        "paneltext" => paneltext(app),
        "paneltree" => paneltree(app),
        "setdark" => {
            // The dark capture needs the page canvas to follow the frame.
            // The setting is normally a person's choice in Settings; this
            // verb only drives the same setter the switch uses, so the
            // screenshots show the feature rather than a special build.
            let on = match argument.trim() {
                "on" => true,
                "off" => false,
                _ => return fail("setdark needs on or off"),
            };
            app.prefs.borrow_mut().darken_pages = on;
            app.prefs.borrow().save();
            app.apply_page_tone();
            app.reload();
            ok("")
        }
        "panelscroll" => {
            let amount: f64 = argument.parse().unwrap_or(0.0);
            let panels = app.panels.borrow();
            // Settings nests its rows in a second scroller inside `body`.
            // Scrolling only the outer card left the scrolled evidence shot
            // byte-identical to the unscrolled one, so pick the deepest
            // scrollable child that is actually visible.
            let scroll = if panels.open.as_ref() == Some(&crate::panels::PanelKind::Settings) {
                panels
                    .settings_holder
                    .children()
                    .first()
                    .and_then(|holder| holder.clone().downcast::<gtk::ScrolledWindow>().ok())
                    .unwrap_or_else(|| panels.scroll.clone())
            } else {
                panels.scroll.clone()
            };
            let adjustment = scroll.vadjustment();
            adjustment.set_value((adjustment.value() + amount).clamp(adjustment.lower(), adjustment.upper() - adjustment.page_size()));
            ok(&format!(",\"value\":{},\"upper\":{},\"page\":{}", adjustment.value(), adjustment.upper(), adjustment.page_size()))
        }
        "panelgeometry" => {
            settle(app);
            let panels = app.panels.borrow();
            let adjustment = panels.scroll.vadjustment();
            ok(&format!(",\"card\":[{},{}],\"body\":[{},{}],\"scroll\":[{},{}],\"value\":{},\"upper\":{},\"page\":{}", panels.card.allocated_width(), panels.card.allocated_height(), panels.body.allocated_width(), panels.body.allocated_height(), panels.scroll.allocated_width(), panels.scroll.allocated_height(), adjustment.value(), adjustment.upper(), adjustment.page_size()))
        }
        "panelkey" => {
            let intent = match argument.trim() {
                "left" => Intent::PanelLeft,
                "right" => Intent::PanelRight,
                "tab" => Intent::PanelFocus,
                _ => return fail("panelkey needs left, right or tab"),
            };
            crate::keys::apply(app, intent);
            ok(",\"handled\":true")
        }
        "panelfocus" => {
            let panels = app.panels.borrow();
            let card = &panels.card;
            let holder = &panels.holder;
            let focus = app.ui.window.focused_widget();
            let inside = focus.as_ref().is_some_and(|widget|
                widget.is_ancestor(card) || widget == card ||
                widget.is_ancestor(holder) || widget == holder);
            let mut chain = Vec::new();
            let mut current = focus.clone();
            while let Some(widget) = current {
                chain.push(widget.type_().name().to_string());
                current = widget.parent();
            }
            ok(&format!(",\"inside\":{inside},\"focus\":\"{}\",\"focusParent\":\"{}\",\"focusChain\":\"{}\"", focus.as_ref().map(|widget| widget.type_().name()).unwrap_or_default(), focus.and_then(|widget| widget.parent()).map(|widget| widget.type_().name()).unwrap_or_default(), json_escape(&chain.join(">"))))
        }
        "errorgeometry" => {
            let ui = &app.ui;
            ok(&format!(",\"page\":[{},{}],\"holder\":[{},{}],\"stackSize\":[{},{}],\"reason\":\"{}\",\"address\":\"{}\",\"visible\":{},\"mapped\":{},\"stack\":\"{}\"", ui.errorpage.allocated_width(), ui.errorpage.allocated_height(), ui.error_holder.allocated_width(), ui.error_holder.allocated_height(), ui.stack.allocated_width(), ui.stack.allocated_height(), json_escape(&ui.error_reason.text()), json_escape(&ui.error_address.text()), ui.errorpage.get_visible(), ui.errorpage.is_mapped(), ui.stack.visible_child_name().unwrap_or_default()))
        }
        "fullscreen" => {
            // Proves page_fullscreen without a screenshot. The bench capture
            // draws the shell itself, so a shot can still show the column
            // after it was hidden; this reads the widgets' own state instead.
            // "on"/"off" drive the same path a page's video takes; no argument
            // only reports.
            match argument.trim() {
                "on" => app.page_fullscreen(true),
                "off" => app.page_fullscreen(false),
                _ => {}
            }
            settle(app);
            ok(&format!(
                ",\"sideShown\":{},\"bookmarksShown\":{},\"sideWidth\":{},\"windowFullscreen\":{},\"flag\":{},\"toolbarShown\":{}",
                app.ui.side.is_child_visible(),
                app.ui.bmarkbar.is_child_visible(),
                app.ui.side.allocated_width(),
                app.window_is_fullscreen(),
                app.fullscreen.get(),
                app.ui.toolbar.is_child_visible()
            ))
        }
        "debug" => debug(app),
        "settle" => settle(app),
        "tabs" => tabs(app),
        "pins" => ok(&format!(
            ",\"holderMapped\":{},\"holder\":\"{}x{}\",\"grid\":\"{}x{}\",\"children\":{},\"pinned\":{}",
            app.ui.pins_holder.is_mapped(),
            app.ui.pins_holder.allocated_width(),
            app.ui.pins_holder.allocated_height(),
            app.ui.pins.allocated_width(),
            app.ui.pins.allocated_height(),
            app.ui.pins.children().len(),
            app.state.borrow().tabs.iter().filter(|tab| tab.pinned).count()
        )),
        "filter" => {
            // The borrow is released before the panel is asked to rebuild: a
            // match on a `borrow()` keeps it alive for the whole arm.
            let kind = app.panels.borrow().open.clone();
            match kind {
                Some(kind) => app.filter_panel(kind, argument),
                None => return fail("no panel is open"),
            }
            ok("")
        }
        "open" => {
            if argument.is_empty() {
                return fail("open needs an address");
            }
            let id = app.new_tab(None, false, true);
            app.select(id);
            app.navigate(id, argument);
            ok(&format!(",\"id\":{id}"))
        }
        "close" => match tab_id(argument) {
            Some(id) => {
                app.close_tab(id);
                ok("")
            }
            None => fail("close needs a tab id"),
        },
        "closeall" => {
            // Close every tab, then leave exactly one empty tab behind: closing
            // the last tab already makes a fresh one, so the loop must stop at
            // one or the browser ends up with two "New tab" rows and every
            // picture of the empty state shows both.
            let ids: Vec<u64> = app.state.borrow().tabs.iter().map(|tab| tab.id).collect();
            for id in ids.iter().take(ids.len().saturating_sub(1)) {
                app.close_tab(*id);
            }
            let left: Vec<u64> = app.state.borrow().tabs.iter().map(|tab| tab.id).collect();
            if left.len() == 1 {
                app.close_tab(left[0]);
            }
            ok("")
        },
        "select" => match tab_id(argument) {
            Some(id) => {
                app.select(id);
                ok("")
            }
            None => fail("select needs a tab id"),
        },
        "go" => {
            if argument.is_empty() {
                return fail("go needs an address");
            }
            app.navigate_active(argument);
            ok("")
        }
        "peek" => {
            // Peek opens when you Shift-click a link, and the socket cannot
            // send a pointer click. This verb calls the same peek::open that
            // the click handler in web.rs calls. The active tab's view is the
            // opener, so the card gets the same context, content manager and
            // private flag as a real Shift-click. A screenshot run can show
            // the card without a display or a pointer.
            if argument.is_empty() {
                return fail("peek needs an address");
            }
            let tab = app.active_id();
            match app.view(tab) {
                Some(opener) => {
                    crate::peek::open(app, tab, &opener, argument);
                    ok("")
                }
                None => fail("peek needs an open page to peek from"),
            }
        }
        "peekkeep" => {
            // Keep on the Peek card. The socket cannot click the button, so
            // this calls peek::keep, which runs the same keep_peek the button
            // runs. The stress suite uses it to check that a kept page becomes
            // a tab without loading again, sits right after the active tab and
            // keeps the private flag of the tab it was peeked from.
            if crate::peek::keep(app) {
                ok("")
            } else {
                fail("no peek card is open")
            }
        }
        "key" => key(app, argument),
        "activate" => {
            app.ui.window.present();
            ok("")
        }
        "hide" => match crate::curtain::pick_selector(app, argument) {
            Ok(()) => ok(""),
            Err(reason) => fail(reason),
        },
        "escape" => {
            // Everything a person can take down with Escape comes down here,
            // the Peek card and the site card included. Without them a test
            // that opens one card and then another kept the first on screen,
            // and the screenshots after it showed the wrong thing: round 43's
            // Peek shot still had the site card behind it, and Settings sat
            // under the Peek card. (`key escape` is not a shortcut name and
            // fails with "unknown key: escape"; this verb is the way in.)
            app.close_omni();
            if app.find.borrow().visible {
                app.toggle_find();
            }
            crate::peek::clear(app);
            crate::sitecard::dismiss(app);
            crate::welcome::dismiss(app);
            app.close_panels();
            ok("")
        }
        "cards" => {
            // Where each floating card sits over the page, and where its first
            // children sit inside it. Round 43's site card kept its text against
            // the border after set_border_width(12), and the Peek page ran off
            // the card's right edge. These numbers separate a layout bug (the
            // label really is at x=0 inside the card) from a capture bug (the
            // label is at x=12 and the screenshot draws it elsewhere).
            settle(app);
            let shell = app.ui.shell.clone().upcast::<gtk::Widget>();
            let mut out: Vec<String> = Vec::new();
            for child in app.ui.overlay.children() {
                if child == shell || !child.is_visible() {
                    continue;
                }
                let (x, y) = child.translate_coordinates(&app.ui.overlay, 0, 0).unwrap_or((-1, -1));
                let border = child
                    .dynamic_cast_ref::<gtk::Container>()
                    .map(|container| container.border_width())
                    .unwrap_or(0);
                let inner: Vec<String> = child
                    .dynamic_cast_ref::<gtk::Container>()
                    .map(|container| {
                        container
                            .children()
                            .iter()
                            .take(4)
                            .map(|kid| {
                                let (kx, ky) = kid.translate_coordinates(&child, 0, 0).unwrap_or((-1, -1));
                                format!(
                                    "{{\"type\":\"{}\",\"name\":\"{}\",\"at\":[{kx},{ky}],\"size\":[{},{}]}}",
                                    kid.type_().name(),
                                    json_escape(&kid.widget_name()),
                                    kid.allocated_width(),
                                    kid.allocated_height()
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                out.push(format!(
                    "{{\"type\":\"{}\",\"name\":\"{}\",\"at\":[{x},{y}],\"size\":[{},{}],\"border\":{border},\"margins\":[{},{},{},{}],\"children\":[{}]}}",
                    child.type_().name(),
                    json_escape(&child.widget_name()),
                    child.allocated_width(),
                    child.allocated_height(),
                    child.margin_start(),
                    child.margin_end(),
                    child.margin_top(),
                    child.margin_bottom(),
                    inner.join(",")
                ));
            }
            ok(&format!(",\"overlay\":[{},{}],\"cards\":[{}]", app.ui.overlay.allocated_width(), app.ui.overlay.allocated_height(), out.join(",")))
        }
        "pump" => {
            // Bench commands run synchronously inside one main-context
            // dispatch. Widgets added in that dispatch are still 1x1 until a
            // real allocation pass; the browser gets one before a person can
            // see the field, but the capture harness must ask for it.
            app.ui.window.queue_resize();
            let context = glib::MainContext::default();
            for _ in 0..256 {
                if !context.iteration(false) {
                    break;
                }
            }
            ok("")
        }
        "omnigeo" => {
            let omni = &app.omni;
            let rows: Vec<String> = omni.list.children().iter().map(|child| format!(
                "{{\"type\":\"{}\",\"w\":{},\"h\":{},\"vis\":{},\"mapped\":{},\"childvis\":{}}}",
                child.type_().name(), child.allocated_width(), child.allocated_height(),
                child.is_visible(), child.is_mapped(), child.is_child_visible())).collect();
            ok(&format!(",\"list\":{{\"w\":{},\"h\":{},\"vis\":{},\"mapped\":{},\"children\":[{}]}}",
                omni.list.allocated_width(), omni.list.allocated_height(), omni.list.is_visible(),
                omni.list.is_mapped(), rows.join(",")))
        }
        "omni" => {
            app.open_omni(true);
            app.omni.entry.set_text(argument);
            app.refresh_offers(argument);
            crate::omnibox::commit(app);
            ok("")
        }
        "omnitext" => {
            // Set the field's words without committing: lets a harness test
            // arrow-key movement through the full, unfiltered offer list.
            app.omni.entry.set_text(argument);
            app.refresh_offers(argument);
            ok("")
        }
        "find" => {
            if !app.find.borrow().visible {
                app.toggle_find();
            }
            app.find.borrow().entry.set_text(argument);
            app.find_query(argument);
            ok("")
        }
        "blocker" => {
            app.toggle_blocker();
            ok("")
        }
        "session" => {
            app.save_session();
            ok("")
        }
        "hidden" => {
            let state = app.state.borrow();
            let mut items: Vec<String> = Vec::new();
            for (host, list) in state.hidden.iter() {
                for selector in list {
                    items.push(format!(
                        "{{\"host\":\"{}\",\"selector\":\"{}\"}}",
                        json_escape(host),
                        json_escape(selector)
                    ));
                }
            }
            ok(&format!(",\"hidden\":[{}]", items.join(",")))
        }
        "shield" => {
            let (lists, done, rules) = crate::shield::state();
            ok(&format!(
                ",\"lists\":{lists},\"finished\":{done},\"rules\":{rules}"
            ))
        }
        "forget" => {
            // A scenario that is about what the browser looks like with
            // nothing kept needs a way to keep nothing.
            let books = {
                let mut state = app.state.borrow_mut();
                let books = state.bookmarks.clone();
                state.bookmarks.clear();
                books
            };
            let _ = books;
            crate::store::Records::save_bookmarks(&[]);
            app.render_bmarks();
            ok("")
        }
        "extensions" => {
            let entries: Vec<String> = crate::extensions::registry()
                .extensions
                .iter()
                .map(|entry| {
                    format!(
                        "{{\"id\":\"{}\",\"name\":\"{}\",\"runs\":{},\"enabled\":{},\"why_not\":\"{}\"}}",
                        json_escape(&entry.id),
                        json_escape(&entry.name),
                        entry.runs,
                        entry.enabled,
                        json_escape(&entry.why_not.replace('\n', " "))
                    )
                })
                .collect();
            ok(&format!(",\"extensions\":[{}]", entries.join(",")))
        }
        "installext" => {
            if argument.is_empty() {
                return fail("installext needs a path");
            }
            match crate::extensions::install(std::path::Path::new(argument)) {
                Ok(entry) => {
                    app.rebuild_every_view();
                    ok(&format!(
                        ",\"name\":\"{}\",\"runs\":{},\"why_not\":\"{}\"",
                        json_escape(&entry.name),
                        entry.runs,
                        json_escape(&entry.why_not.replace('\n', " "))
                    ))
                }
                Err(why) => fail(&why),
            }
        }
        // Error variants that a local fixture server cannot produce. They
        // use the same renderer as a real page failure, so the shot shows a
        // real error page instead of a mock drawn by the harness.
        "showerror" => {
            let (reason, url) = match argument {
                "dns" => ("That address could not be found.", "https://no-such-host.invalid/page"),
                "refused" => ("The site refused the connection.", "http://127.0.0.1:1/refused"),
                "timeout" => ("The site took too long to answer.", "http://127.0.0.1:9/slow"),
                "tls" => ("This site's security could not be verified: Let's Encrypt certificate expired on 2026-01-01. It was not opened.", "https://expired.badssl.com/account"),
                "offline" => ("There is no network to reach it on.", "https://example.com/offline"),
                _ => return fail("showerror needs dns, refused, timeout, tls or offline"),
            };
            let id = app.state.borrow().active;
            app.show_error(id, reason, url);
            // The renderer's error page names the URL it failed to open; the
            // chrome has to agree, or the screenshot shows two different
            // addresses at once. show_error changes the tab, refresh_helm
            // copies it into the visible field and the path label.
            app.refresh_helm();
            ok("")
        }
        // Seed the populated library panels. These are the records a person
        // accumulates over weeks; an empty panel only proves the empty text.
        "seeddata" => {
            {
                let mut state = app.state.borrow_mut();
                match argument {
                    "bookmarks" => {
                        for (url, title) in [
                            ("http://127.0.0.1:8765/office", "Office Commun"),
                            ("http://127.0.0.1:8765/long", "The Long Read"),
                        ] {
                            if !state.bookmarks.iter().any(|b| b.url == url) {
                                state.bookmarks.push(crate::store::Bookmark { url: url.to_string(), title: title.to_string() });
                            }
                        }
                    }
                    "downloads" => {
                        for (name, done, failed, got, total) in [
                            ("release-notes.pdf", true, false, 2400000u64, 2400000u64),
                            ("field-report.csv", true, false, 184000, 184000),
                            ("setup.dmg", false, true, 61000000, 92000000),
                            ("dataset.zip", false, false, 128000000, 410000000),
                        ] {
                            if !state.downloads.iter().any(|d| d.name == name) {
                                state.downloads.push(crate::app::DownloadRec {
                                    id: 0, name: name.to_string(), path: format!("/home/user/Downloads/{name}"),
                                    done, failed, got, total, private: false,
                                });
                            }
                        }
                    }
                    _ => return fail("seeddata needs bookmarks or downloads"),
                }
            }
            let state = app.state.borrow();
            crate::store::Records::save_bookmarks(&state.bookmarks);
            crate::store::Records::save_downloads(&state.downloads);
            drop(state);
            app.render_bmarks();
            ok("")
        }
        "downloads" => {
            let state = app.state.borrow();
            let items: Vec<String> = state
                .downloads
                .iter()
                .map(|download| {
                    format!(
                        "{{\"name\":\"{}\",\"path\":\"{}\",\"done\":{}}}",
                        json_escape(&download.name),
                        json_escape(&download.path),
                        download.done
                    )
                })
                .collect();
            ok(&format!(",\"downloads\":[{}]", items.join(",")))
        }
        "resize" => {
            let parts: Vec<i32> = argument.split('x').filter_map(|part| part.trim().parse().ok()).collect();
            if parts.len() != 2 {
                return fail("resize needs WxH");
            }
            app.ui.window.set_default_size(parts[0], parts[1]);
            app.ui.window.resize(parts[0], parts[1]);
            ok("")
        }
        "shot" => {
            if argument.is_empty() {
                return fail("shot needs a path");
            }
            match app.capture(std::path::Path::new(argument)) {
                Ok(()) => ok(&format!(",\"path\":\"{}\"", json_escape(argument))),
                Err(error) => fail(&error),
            }
        }
        "orphans" => ok(&format!(",\"count\":{}", app.orphan_views())),
        "contrast" => {
            let mut pairs = Vec::new();
            for (name, look) in [("light", crate::store::Look::Light), ("dark", crate::store::Look::Dark)] {
                let p = crate::palette::Palette::for_look(look, false);
                let values = [
                    ("ink/ground", p.ink, p.ground),
                    ("muted/ground", p.muted, p.ground),
                    ("faint/ground", p.faint, p.ground),
                    ("danger/ground", p.danger, p.ground),
                    ("ink/side", p.ink, p.side_bg()),
                    ("muted/side", p.muted, p.side_bg()),
                ];
                for (label, ink, ground) in values {
                    pairs.push(format!(
                        "{{\"pair\":\"{name} {label}\",\"ratio\":{:.2}}}",
                        crate::palette::contrast(ink, ground)
                    ));
                }
            }
            ok(&format!(",\"pairs\":[{}]", pairs.join(",")))
        }
        "typeramp" => {
            let default = crate::css::Scale::from_points(10.0);
            let large = crate::css::Scale::from_points(18.0);
            ok(&format!(
                ",\"default\":{{\"caption\":{},\"body\":{},\"title\":{},\"headline\":{},\"panel\":{}}},\"large\":{{\"caption\":{},\"body\":{},\"title\":{},\"headline\":{},\"panel\":{}}}",
                default.caption, default.body, default.title, default.start_headline, default.panel_heading,
                large.caption, large.body, large.title, large.start_headline, large.panel_heading
            ))
        }
        "motioncfg" => {
            let gtk = gtk::Settings::default()
                .map(|settings| settings.is_gtk_enable_animations())
                .unwrap_or(true);
            ok(&format!(
                ",\"animations\":{},\"gtkAnimations\":{},\"enterMs\":{}",
                crate::motion::enabled(),
                gtk,
                crate::motion::dur::ENTER
            ))
        }
        "drift" => {
            let worst = WORST_GAP_MS.lock().map(|value| *value).unwrap_or(0);
            let gaps = GAPS.lock().map(|gaps| gaps.clone()).unwrap_or_default();
            let list: Vec<String> = gaps.iter().map(|gap| gap.to_string()).collect();
            let ticks = TICKS.load(std::sync::atomic::Ordering::Relaxed);
            ok(&format!(",\"worst\":{worst},\"ticks\":{ticks},\"gaps\":[{}]", list.join(",")))
        }
        "reset" => {
            if let Ok(mut worst) = WORST_GAP_MS.lock() {
                *worst = 0;
            }
            if let Ok(mut gaps) = GAPS.lock() {
                gaps.clear();
            }
            // A reset starts a fresh observation window. Without clearing the
            // previous tick the first tick measures idle time before reset.
            if let Ok(mut last) = LAST_TICK.lock() {
                *last = None;
            }
            ok("")
        }
        "quit" => {
            app.ui.window.close();
            ok("")
        }
        other => fail(&format!("unknown command: {other}")),
    }
}

fn chrome(app: &AppRef) -> String {
    settle(app);
    let controls: Vec<_> = [
        ("newtab", app.ui.newtab.upcast_ref::<gtk::Widget>()),
        ("private", app.ui.private_tab.upcast_ref::<gtk::Widget>()),
        ("address", app.ui.address.upcast_ref::<gtk::Widget>()),
        ("site", app.ui.site_info.upcast_ref::<gtk::Widget>()),
        ("fold", app.ui.fold.upcast_ref::<gtk::Widget>()),
        ("back", app.ui.back.upcast_ref::<gtk::Widget>()),
        ("forward", app.ui.forward.upcast_ref::<gtk::Widget>()),
        ("reload", app.ui.reload.upcast_ref::<gtk::Widget>()),
        ("bookmark", app.ui.bookmark.upcast_ref::<gtk::Widget>()),
        ("reader", app.ui.reader.upcast_ref::<gtk::Widget>()),
        ("more", app.ui.more.upcast_ref::<gtk::Widget>()),
        ("history", app.ui.history.upcast_ref::<gtk::Widget>()),
        ("library", app.ui.library.upcast_ref::<gtk::Widget>()),
        ("settings", app.ui.menu.upcast_ref::<gtk::Widget>()),
        ("minimise", app.ui.light_amber.upcast_ref::<gtk::Widget>()),
        ("maximise", app.ui.light_green.upcast_ref::<gtk::Widget>()),
        ("close", app.ui.light_red.upcast_ref::<gtk::Widget>()),
        ("start-search", app.ui.start.search.upcast_ref::<gtk::Widget>()),
    ].into_iter().map(|(name, widget)| {
        let origin = widget.translate_coordinates(&app.ui.overlay, 0, 0).unwrap_or((-1, -1));
        serde_json::json!({
            "name": name,
            "at": [origin.0, origin.1],
            "size": [widget.allocated_width(), widget.allocated_height()],
            "visible": widget.is_visible() && widget.is_child_visible() && widget.is_mapped(),
            "sensitive": widget.is_sensitive(),
            "focusOnClick": widget.gets_focus_on_click(),
            "accessibleName": widget.accessible().and_then(|access| gtk::atk::prelude::AtkObjectExt::name(&access)).map(|name| name.to_string()),
        })
    }).collect();
    serde_json::json!({
        "ok": true,
        "address": app.ui.address_label.text().as_str(),
        "tabCount": app.ui.tab_count.text().as_str(),
        "dark": app.dark(),
        "toolbarShown": app.ui.toolbar.is_child_visible(),
        "bookmarksShown": app.ui.bmarkbar.is_visible() && app.ui.bmarkbar.is_child_visible() && app.ui.bmarkbar.is_mapped(),
        "rowCount": app.ui.rows.children().len(),
        "startFocused": app.ui.window.focused_widget().as_ref() == Some(app.ui.start.search.upcast_ref()),
        "startHasFocus": app.ui.start.search.has_focus(),
        "windowActive": app.ui.window.is_active(),
        "protected": app.ui.site_info.style_context().has_class("protected"),
        "controls": controls,
    }).to_string()
}

fn browser_menu() -> Option<gtk::Menu> {
    fn find(widget: &gtk::Widget) -> Option<gtk::Menu> {
        if widget.style_context().has_class("browser-menu") && widget.is_visible() {
            return widget.clone().downcast::<gtk::Menu>().ok();
        }
        widget.downcast_ref::<gtk::Container>().and_then(|container| container.children().iter().find_map(find))
    }
    gtk::Window::list_toplevels().iter().find_map(find)
}

fn ui_tree(app: &AppRef) -> String {
    fn walk(widget: &gtk::Widget, root: &gtk::Overlay, nodes: &mut Vec<serde_json::Value>) {
        if !widget.is_visible() || !widget.is_child_visible() || !widget.is_mapped() || widget.is::<webkit2gtk::WebView>() {
            return;
        }
        let origin = widget.translate_coordinates(root, 0, 0).unwrap_or((-1, -1));
        let text = widget.downcast_ref::<gtk::Label>().map(|label| label.text().to_string());
        let preferred = widget.preferred_width();
        let context = widget.style_context();
        let foreground = context.color(widget.state_flags()).to_string();
        let background = context.style_property_for_state("background-color", widget.state_flags())
            .get::<gtk::gdk::RGBA>().ok().map(|color| color.to_string());
        // What the stylesheet resolved, next to what the widget carries: a
        // close button can sit at CSS opacity 0.6 while its row looks idle,
        // and only the flags say whether GTK still thinks it is hovered.
        let css_opacity = context.style_property_for_state("opacity", widget.state_flags())
            .get::<f64>().ok();
        let state_flags = format!("{:?}", widget.state_flags());
        nodes.push(serde_json::json!({
            "type": widget.type_().name(),
            "classes": widget.style_context().list_classes().into_iter().map(|class| class.to_string()).collect::<Vec<_>>(),
            "text": text,
            "at": [origin.0, origin.1],
            "size": [widget.allocated_width(), widget.allocated_height()],
            "preferredWidth": [preferred.0, preferred.1],
            "foreground": foreground,
            "background": background,
            "stateFlags": state_flags,
            "opacity": widget.opacity(),
            "cssOpacity": css_opacity,
        }));
        if let Some(container) = widget.downcast_ref::<gtk::Container>() {
            for child in container.children() { walk(&child, root, nodes); }
        }
    }
    settle(app);
    let mut nodes = Vec::new();
    walk(app.ui.overlay.upcast_ref(), &app.ui.overlay, &mut nodes);
    serde_json::json!({"ok":true, "nodes":nodes}).to_string()
}

fn probe(app: &AppRef) -> String {
    let (tabs, active, url, title) = {
        let state = app.state.borrow();
        let tabs: Vec<String> = state
            .tabs
            .iter()
            .map(|tab| {
                format!(
                    "{{\"id\":{},\"url\":\"{}\",\"title\":\"{}\",\"pinned\":{},\"loading\":{},\"private\":{},\"asleep\":{},\"hasView\":{}}}",
                    tab.id,
                    json_escape(&tab.url),
                    json_escape(&tab.title),
                    tab.pinned,
                    tab.loading,
                    tab.private,
                    tab.asleep,
                    app.view(tab.id).is_some()
                )
            })
            .collect();
        (
            tabs,
            state.active,
            state.active_tab().map(|tab| tab.url.clone()).unwrap_or_default(),
            state.active_tab().map(|tab| tab.title.clone()).unwrap_or_default(),
        )
    };
    let find = app.find.borrow();
    let panels = app.panels.borrow();
    let hide_mode = app.state.borrow().hide_mode;
    let (screen_width, screen_height) = gdk::Screen::default()
        .and_then(|screen| screen.root_window())
        .map(|window| (window.width(), window.height()))
        .unwrap_or((0, 0));
    // The error page is only the browser's story while it is the child being
    // drawn: a failed tab that is not the active one remembers its error but
    // shows a page. The button's caption is read rather than assumed, because
    // a failed load that can only be retried would reload a dead address.
    let error = app
        .ui
        .stack
        .visible_child_name()
        .map(|name| name.as_str() == "error")
        .unwrap_or(false);
    let error_action = app
        .ui
        .error_retry
        .label()
        .map(|label| label.to_string())
        .unwrap_or_default();
    format!(
        "{{\"ok\":true,\"active\":{},\"url\":\"{}\",\"title\":\"{}\",\"tabs\":[{}],\"omni\":{},\"omniCount\":{},\"omniPicked\":{},\"omniText\":\"{}\",\"panel\":\"{}\",\"panelFilter\":\"{}\",\"find\":{},\"findText\":\"{}\",\"findCount\":\"{}\",\"hideMode\":{},\"maximised\":{},\"bookmarkBar\":{},\"bookmarkCount\":{},\"visibleChild\":\"{}\",\"stackChildren\":{},\"orphanViews\":{},\"reading\":{},\"width\":{},\"height\":{},\"viewWidth\":{},\"viewHeight\":{},\"gdkWidth\":{},\"gdkHeight\":{},\"minimumWidth\":{},\"naturalWidth\":{},\"screen\":{{\"width\":{},\"height\":{}}},\"error\":{},\"errorAction\":\"{}\"}}",
        active,
        json_escape(&url),
        json_escape(&title),
        tabs.join(","),
        app.omni.visible.get(),
        app.omni.offers.borrow().len(),
        app.omni.picked.get(),
        json_escape(&app.omni.entry.text()),
        panels.open.as_ref().map(|kind| format!("{kind:?}")).unwrap_or_default(),
        json_escape(&panels.filter.borrow()),
        find.visible,
        json_escape(&find.entry.text()),
        json_escape(&find.count.text()),
        hide_mode,
        app.ui.window.is_maximized(),
        app.ui.bmarkbar.is_visible(),
        app.state.borrow().bookmarks.len(),
        app.ui.stack.visible_child_name().map(|name| name.to_string()).unwrap_or_default(),
        app.ui.stack.children().len(),
        app.orphan_views(),
        app.reading.get(),
        app.ui.window.allocated_width(),
        app.ui.window.allocated_height(),
        app.view(active).map(|v| v.allocated_width()).unwrap_or(0),
        app.view(active).map(|v| v.allocated_height()).unwrap_or(0),
        app.ui.window.window().map(|w| w.geometry().2).unwrap_or(0),
        app.ui.window.window().map(|w| w.geometry().3).unwrap_or(0),
        app.ui.window.preferred_size().0.width,
        app.ui.window.preferred_size().1.width,
        screen_width,
        screen_height,
        error,
        json_escape(&error_action)
    )
}

// The open panel's visible text. A closed panel is not an error: the caller
// asked what is on screen and nothing is.
fn paneltext(app: &AppRef) -> String {
    let panels = app.panels.borrow();
    let Some(kind) = panels.open.clone() else {
        return ok(",\"panel\":null,\"text\":\"\"");
    };
    let mut lines: Vec<String> = Vec::new();
    let mut budget = PANEL_TEXT_CAP;
    collect_labels(panels.body.upcast_ref::<gtk::Widget>(), &mut lines, &mut budget);
    let text = lines.join("\n");
    let text: String = if text.chars().count() > PANEL_TEXT_CAP {
        text.chars().take(PANEL_TEXT_CAP).collect()
    } else {
        text
    };
    ok(&format!(
        ",\"panel\":\"{kind:?}\",\"text\":\"{}\"",
        json_escape(&text)
    ))
}

// Geometry of visible panel controls, for checking capture versus layout.
fn paneltree(app: &AppRef) -> String {
    let panels = app.panels.borrow();
    if panels.open.is_none() { return ok(",\"widgets\":[]"); }
    fn walk(w: &gtk::Widget, out: &mut Vec<String>) {
        if out.len() >= 200 { return; }
        if w.is::<gtk::Entry>() || w.is::<gtk::Label>() {
            let p = w.parent().map(|p| p.type_().name()).unwrap_or_default();
            let hidden = !w.get_visible();
            let text = w.dynamic_cast_ref::<gtk::Entry>().map(|e| e.text().to_string())
                .or_else(|| w.dynamic_cast_ref::<gtk::Label>().map(|l| l.text().to_string())).unwrap_or_default();
            out.push(format!("{{\"kind\":\"{}\",\"parent\":\"{}\",\"size\":[{},{}],\"mapped\":{},\"visible\":{},\"text\":\"{}\"}}",
                w.type_().name(), p, w.allocated_width(), w.allocated_height(), w.is_mapped(), !hidden, json_escape(&text)));
        }
        if let Some(c) = w.dynamic_cast_ref::<gtk::Container>() {
            for child in c.children() { walk(&child, out); }
        }
    }
    let mut out = Vec::new();
    walk(panels.card.upcast_ref(), &mut out);
    ok(&format!(",\"widgets\":[{}]", out.join(",")))
}

fn debug(app: &AppRef) -> String {
    let omni = &app.omni;
    // The field's allocation is relative to the box that holds it, so a field
    // that stays put while its parent moves would still read as still. Window
    // coordinates are what a test compares across two probes with the offers
    // list open and shut, and -1 says the window has not been laid out rather
    // than pretending the field sits in the corner.
    let (field_x, field_y) = omni
        .field
        .translate_coordinates(&app.ui.window, 0, 0)
        .unwrap_or((-1, -1));
    let (entry_x, entry_y) = omni
        .entry
        .translate_coordinates(&app.ui.window, 0, 0)
        .unwrap_or((-1, -1));
    ok(&format!(
        ",\"omni\":{{\"visible\":{},\"mapped\":{},\"holder\":\"{}x{}\",\"block\":\"{}x{}\",\"field\":\"{}x{}\",\"entry\":\"{}x{}\",\"list\":\"{}x{}\",\"listVis\":{},\"scrollVis\":{},\"rows\":{},\"rowsVis\":{},\"fieldAt\":\"{},{}\",\"entryAt\":\"{},{}\",\"offers\":{}}}",
        omni.visible.get(),
        omni.holder.is_mapped(),
        omni.holder.allocated_width(),
        omni.holder.allocated_height(),
        omni.block.allocated_width(),
        omni.block.allocated_height(),
        omni.field.allocated_width(),
        omni.field.allocated_height(),
        omni.entry.allocated_width(),
        omni.entry.allocated_height(),
        omni.list.allocated_width(),
        omni.list.allocated_height(),
        omni.list.get_visible(),
        omni.list_scroll.get_visible(),
        omni.list.children().len(),
        omni.list.children().iter().filter(|c| c.get_visible()).count(),
        field_x,
        field_y,
        entry_x,
        entry_y,
        // Offers the model holds, which can exceed `rowsVis` when the list is
        // capped and scrolls: geometry paired with a count says whether a
        // field moved or the list under it grew.
        omni.offers.borrow().len()
    ))
}

/// Let the window finish laying itself out, and say so when it has.
///
/// GTK allocates an overlay child only in a pass that follows the change that
/// made it visible. A command that raises the address field and returns
/// immediately therefore leaves the field's holder at 1x1, and a test that
/// reads its position reads the corner. A screenshot already runs this pass;
/// this is the same pass without the picture, so a check can wait for the
/// geometry it is about to assert instead of sleeping a guessed amount.
fn settle(app: &AppRef) -> String {
    let mut rounds = 0;
    for _ in 0..60 {
        rounds += 1;
        app.ui.window.check_resize();
        app.ui.overlay.check_resize();
        app.ui.start.root.check_resize();
        let context = glib::MainContext::default();
        let mut dispatched = 0;
        while dispatched < 256 && context.iteration(false) {
            dispatched += 1;
        }
        let width = app.ui.overlay.allocated_width();
        let height = app.ui.overlay.allocated_height();
        let layers_ready = [&app.omni.holder, &app.find.borrow().holder]
            .iter()
            .all(|holder| !holder.get_visible() || holder.allocated_width() == width);
        if width > 1 && height > 1 && layers_ready {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    ok(&format!(
        ",\"rounds\":{},\"width\":{},\"height\":{}",
        rounds,
        app.ui.overlay.allocated_width(),
        app.ui.overlay.allocated_height()
    ))
}

fn tabs(app: &AppRef) -> String {
    let state = app.state.borrow();
    let tabs: Vec<String> = state
        .tabs
        .iter()
        .map(|tab| {
            // The letter the row's tile shows, and whether that label is really
            // on screen. Gemini R59 found the tile stuck on "1": set_loading
            // dropped the label ui::mark returned, so row.letter pointed at a
            // detached label that update_row_title kept renaming. A test can
            // now see both the text and that the label it names is mapped.
            let label = tab.row.as_ref().and_then(|row| row.letter.as_ref());
            let letter = match label {
                Some(label) => format!("\"{}\"", json_escape(label.text().as_str())),
                None => "null".to_string(),
            };
            let letter_mapped = label.map(|label| label.is_mapped()).unwrap_or(false);
            format!(
                "{{\"id\":{},\"url\":\"{}\",\"title\":\"{}\",\"pinned\":{},\"letter\":{},\"letterMapped\":{}}}",
                tab.id,
                json_escape(&tab.url),
                json_escape(&tab.title),
                tab.pinned,
                letter,
                letter_mapped
            )
        })
        .collect();
    ok(&format!(",\"count\":{},\"tabs\":[{}]", state.tabs.len(), tabs.join(",")))
}

fn key(app: &AppRef, name: &str) -> String {
    // A verb that names a shortcut goes through `keys::intent_of`, which folds
    // the keyval exactly as a real key press does. A verb that changes a
    // shortcut's own meaning therefore fails here instead of quietly testing
    // something the browser no longer does.
    if let Some(intent) = crate::keys::intent_of(name) {
        if intent != Intent::Nothing {
            crate::keys::apply(app, intent);
            return ok("");
        }
    }
    match name {
        "down" => app.omni.step(1),
        "up" => app.omni.step(-1),
        "blocker" => {
            app.toggle_blocker();
            return ok("");
        }
        "pin" => {
            app.toggle_pin(app.active_id());
            return ok("");
        }
        "closepanel" => {
            app.close_panels();
            return ok("");
        }
        "extensions" => {
            // The extensions panel has no key of its own: it is reached from
            // the menu. A test still has to be able to open it, or the panel
            // that explains what an extension can and cannot do is the one
            // panel nobody ever looks at.
            app.toggle_panel(crate::panels::PanelKind::Extensions);
            return ok("");
        }
        "maximise" => {
            let before = app.ui.window.is_maximized();
            if before {
                app.ui.window.unmaximize();
            } else {
                app.ui.window.maximize();
            }
            let mut pumped = 0;
            while app.ui.window.is_maximized() == before && pumped < 20 {
                let context = glib::MainContext::default();
                let mut dispatched = 0;
                while dispatched < 64 && context.iteration(false) {
                    dispatched += 1;
                }
                pumped += 1;
            }
            return ok(&format!(",\"maximised\":{}", app.ui.window.is_maximized()));
        }
        other => {
            if let Ok(number) = other.parse::<usize>() {
                app.jump(number);
            } else {
                return fail(&format!("unknown key: {other}"));
            }
        }
    }
    ok("")
}

fn page_text(app: &AppRef, sender: &mpsc::Sender<String>) {
    let Some(view) = app.view(app.active_id()) else {
        let _ = sender.send(ok(",\"text\":{\"text\":\"\"}"));
        return;
    };
    // innerText is what a person can read: it leaves out <style> and <script>
    // source, which textContent includes (example.org's first 80 characters
    // were CSS). The old 400-character cut also meant a strict content check
    // could never find a heading further down a real page, so every real site
    // failed it. Return the readable text, capped at a size the socket handles.
    let js = r#"(() => {
        const body = (document.body && (document.body.innerText || document.body.textContent)) || '';
        return JSON.stringify({ title: document.title || '', url: location.href, length: body.length, text: body.slice(0, 20000) });
    })();"#;
    let sender = sender.clone();
    crate::js::run(&view, js, move |result| {
        let value = result.ok().filter(|value| !value.is_empty());
        let answer = match value {
            Some(value) => ok(&format!(",\"text\":{value}")),
            None => fail("the page did not answer"),
        };
        let _ = sender.send(answer);
    });
}

/// The page view inside an open Peek card, if there is one. `js` only reaches
/// the active tab's view, and round 43's Peek shots cut the page off on the
/// right although the `cards` verb measured the view at 1011px inside a
/// 1013px card. Running a probe in the Peek page itself tells whether the page
/// really lays out wider than its view (a page or layout bug) or fits and is
/// only drawn cut off by `capture()` (a screenshot bug).
fn peek_view(app: &AppRef) -> Option<webkit2gtk::WebView> {
    for child in app.ui.overlay.children() {
        if child.widget_name() != "peek-card" {
            continue;
        }
        let container = child.dynamic_cast_ref::<gtk::Container>()?;
        for kid in container.children() {
            if let Ok(view) = kid.downcast::<webkit2gtk::WebView>() {
                return Some(view);
            }
        }
    }
    None
}

fn peek_eval(app: &AppRef, code: &str, sender: &mpsc::Sender<String>) {
    let Some(view) = peek_view(app) else {
        let _ = sender.send(fail("no peek card is open"));
        return;
    };
    eval_in(&view, code, sender);
}

fn page_eval(app: &AppRef, code: &str, sender: &mpsc::Sender<String>) {
    let Some(view) = app.view(app.active_id()) else {
        let _ = sender.send(fail("no page"));
        return;
    };
    eval_in(&view, code, sender);
}

fn eval_in(view: &webkit2gtk::WebView, code: &str, sender: &mpsc::Sender<String>) {
    // A test script is an expression, placed in the wrapper as source rather
    // than passed to eval(). Pages that require Trusted Types (YouTube is one)
    // refuse eval() of a string, and every probe of them came back refused;
    // a script the browser itself evaluates is not subject to that policy.
    // An exception still comes back as an answer; a syntax error comes back as
    // the engine's own message instead of a bare "did not answer".
    let wrapped = format!(
        "(function(){{ try {{ const value = ({code}\n); return value === undefined ? 'undefined' : String(value); }} catch (error) {{ return 'error: ' + error; }} }})()"
    );
    let sender = sender.clone();
    crate::js::run(view, &wrapped, move |result| {
        let answer = match result {
            Ok(value) => ok(&format!(",\"value\":\"{}\"", json_escape(&value))),
            Err(error) => fail(&format!("the page did not answer: {error}")),
        };
        let _ = sender.send(answer);
    });
}

