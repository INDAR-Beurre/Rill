// Reading mode: the page's own words, set in the browser's type, with nothing
// around them. Done in the page rather than beside it, so it costs no second
// view and no third process.
use crate::app::AppRef;

const TOGGLE_ON: &str = include_str!("../assets/reader.js");

pub fn enter(app: &AppRef, view: &webkit2gtk::WebView) {
    let dark = app.dark();
    let css_light = "background:#FFFFFF;color:#171717";
    let css_dark = "background:#0B0B0B;color:#EDEDED";
    let palette = if dark { css_dark } else { css_light };
    let script = format!(
        "{TOGGLE_ON}\nwindow.__searchReader && window.__searchReader({});",
        serde_json::to_string(palette).unwrap_or_else(|_| "\"\"".into())
    );
    let Some(id) = crate::app::view_id(view) else { return };
    let weak = app.weak();
    crate::js::run(view, &script, move |result| {
        let done = result.map(|value| value != "false").unwrap_or(false);
        if let Some(app) = weak.upgrade() {
            if done {
                if app.reading_tabs.borrow().contains(&id) {
                    app.reading_tabs.borrow_mut().remove(&id);
                } else {
                    app.reading_tabs.borrow_mut().insert(id);
                }
                app.reading.set(app.tab_reading(app.active_id()));
            }
            if !done {
                app.flash_error("Nothing to read on this page");
            }
        }
    });
}

