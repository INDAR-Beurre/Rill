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
use std::time::Duration;

pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("SEARCH_BENCH_SOCKET") {
        return PathBuf::from(path);
    }
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("search-bench.sock")
}

pub fn enabled() -> bool {
    std::env::var_os("SEARCH_BENCH").is_some() || std::env::args().any(|argument| argument == "--bench")
}

pub fn serve(_app: &AppRef) -> std::io::Result<PathBuf> {
    let path = socket_path();
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path)?;
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
            _ => {
                let answer = dispatch(&app, &command, &argument);
                let _ = immediate.send(answer);
            }
        }
    });
    match receiver.recv_timeout(Duration::from_secs(20)) {
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

fn dispatch(app: &AppRef, command: &str, argument: &str) -> String {
    match command {
        "ping" => ok(&format!(",\"version\":\"{}\"", env!("CARGO_PKG_VERSION"))),
        "probe" => probe(app),
        "debug" => debug(app),
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
        "key" => key(app, argument),
        "activate" => {
            app.ui.window.present();
            ok("")
        }
        "escape" => {
            app.close_omni();
            if app.find.borrow().visible {
                app.toggle_find();
            }
            app.close_panels();
            ok("")
        }
        "omni" => {
            app.open_omni(true);
            app.omni.entry.set_text(argument);
            app.refresh_offers(argument);
            crate::omnibox::commit(app);
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
        "quit" => {
            app.ui.window.close();
            ok("")
        }
        other => fail(&format!("unknown command: {other}")),
    }
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
    format!(
        "{{\"ok\":true,\"active\":{},\"url\":\"{}\",\"title\":\"{}\",\"tabs\":[{}],\"omni\":{},\"omniCount\":{},\"omniPicked\":{},\"omniText\":\"{}\",\"panel\":\"{}\",\"panelFilter\":\"{}\",\"find\":{},\"findText\":\"{}\",\"findCount\":\"{}\",\"hideMode\":{},\"maximised\":{},\"visibleChild\":\"{}\",\"stackChildren\":{},\"width\":{},\"height\":{}}}",
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
        app.ui.stack.visible_child_name().map(|name| name.to_string()).unwrap_or_default(),
        app.ui.stack.children().len(),
        app.ui.window.allocated_width(),
        app.ui.window.allocated_height()
    )
}

fn debug(app: &AppRef) -> String {
    let omni = &app.omni;
    ok(&format!(
        ",\"omni\":{{\"visible\":{},\"mapped\":{},\"holder\":\"{}x{}\",\"block\":\"{}x{}\",\"field\":\"{}x{}\",\"entry\":\"{}x{}\",\"list\":\"{}x{}\"}}",
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
        omni.list.allocated_height()
    ))
}

fn tabs(app: &AppRef) -> String {
    let state = app.state.borrow();
    let tabs: Vec<String> = state
        .tabs
        .iter()
        .map(|tab| {
            format!(
                "{{\"id\":{},\"url\":\"{}\",\"title\":\"{}\",\"pinned\":{}}}",
                tab.id,
                json_escape(&tab.url),
                json_escape(&tab.title),
                tab.pinned
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
        "maximise" => {
            if app.ui.window.is_maximized() {
                app.ui.window.unmaximize();
            } else {
                app.ui.window.maximize();
            }
            return ok("");
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
    let js = r#"(() => {
        const body = (document.body && document.body.textContent) || '';
        return JSON.stringify({ title: document.title || '', url: location.href, length: body.length, text: body.slice(0, 400) });
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

fn page_eval(app: &AppRef, code: &str, sender: &mpsc::Sender<String>) {
    let Some(view) = app.view(app.active_id()) else {
        let _ = sender.send(fail("no page"));
        return;
    };
    // A test script is wrapped so an exception or a bare statement comes back
    // as an answer rather than as an evaluation error.
    let wrapped = format!(
        "(function(){{ try {{ const value = eval({}); return value === undefined ? 'undefined' : String(value); }} catch (error) {{ return 'error: ' + error; }} }})()",
        serde_json::to_string(code).unwrap_or_else(|_| "\"\"".into())
    );
    let sender = sender.clone();
    crate::js::run(&view, &wrapped, move |result| {
        let answer = match result {
            Ok(value) => ok(&format!(",\"value\":\"{}\"", json_escape(&value))),
            Err(_) => fail("the page did not answer"),
        };
        let _ = sender.send(answer);
    });
}

