// A socket a script can drive the browser through, as the Mac build has: a
// test that needs a real window still needs a way to open a tab, read the page
// and ask the browser what it is showing.
//
// Line protocol over a Unix socket under the user's runtime folder. One
// command per line, one JSON answer per line.
use crate::app::{App, AppRef};
use crate::panels::PanelKind;
use gtk::prelude::*;
use javascriptcore::ValueExt;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use webkit2gtk::WebViewExt;

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
            let ids: Vec<u64> = app.state.borrow().tabs.iter().map(|tab| tab.id).collect();
            for id in ids {
                app.close_tab(id);
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
            app.find_query(argument, true);
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
        "{{\"ok\":true,\"active\":{},\"url\":\"{}\",\"title\":\"{}\",\"tabs\":[{}],\"omni\":{},\"omniText\":\"{}\",\"panel\":\"{}\",\"find\":{},\"findText\":\"{}\",\"hideMode\":{},\"maximised\":{},\"visibleChild\":\"{}\",\"stackChildren\":{},\"width\":{},\"height\":{}}}",
        active,
        json_escape(&url),
        json_escape(&title),
        tabs.join(","),
        app.omni.visible.get(),
        json_escape(&app.omni.entry.text()),
        panels.open.as_ref().map(|kind| format!("{kind:?}")).unwrap_or_default(),
        find.visible,
        json_escape(&find.entry.text()),
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
    match name {
        "omni" => app.open_omni(true),
        "newtab" => {
            app.new_tab(None, false, false);
        }
        "privatetab" => {
            app.new_tab(None, true, false);
        }
        "closetab" => app.close_tab(app.active_id()),
        "reopen" => app.reopen_closed(),
        "reload" => app.reload(),
        "back" => app.go_back(),
        "forward" => app.go_forward(),
        "nexttab" => app.cycle(true),
        "previoustab" => app.cycle(false),
        "pin" => app.toggle_pin(app.active_id()),
        "bookmark" => app.add_bookmark(),
        "copy" => app.copy_address(),
        "reader" => app.reader(),
        "hide" => app.hide_element_mode(),
        "unhide" => app.unhide_all(),
        "find" => app.toggle_find(),
        "history" => app.toggle_panel(PanelKind::History),
        "bookmarks" => app.toggle_panel(PanelKind::Bookmarks),
        "downloads" => app.toggle_panel(PanelKind::Downloads),
        "settings" => app.toggle_panel(PanelKind::Settings),
        "closepanel" => app.close_panels(),
        "zoomin" => app.zoom(0.1),
        "zoomout" => app.zoom(-0.1),
        "zoomreset" => app.reset_zoom(),
        "maximise" => {
            if app.ui.window.is_maximized() {
                app.ui.window.unmaximize();
            } else {
                app.ui.window.maximize();
            }
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
    view.run_javascript(js, None::<&gtk::gio::Cancellable>, move |result| {
        let value = result
            .ok()
            .and_then(|javascript| javascript.js_value())
            .map(|value| value.to_str().to_string())
            .filter(|value| !value.is_empty());
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
    view.run_javascript(&wrapped, None::<&gtk::gio::Cancellable>, move |result| {
        let answered = result.is_ok();
        let value = result
            .ok()
            .and_then(|javascript| javascript.js_value())
            .map(|value| value.to_str().to_string())
            .unwrap_or_default();
        let answer = if answered {
            ok(&format!(",\"value\":\"{}\"", json_escape(&value)))
        } else {
            fail("the page did not answer")
        };
        let _ = sender.send(answer);
    });
}

#[allow(dead_code)]
pub fn unused(_: &App) {}
