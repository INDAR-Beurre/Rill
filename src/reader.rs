// Reading mode: the page's own words, set in the browser's type, with nothing
// around them. Done in the page rather than beside it, so it costs no second
// view and no third process.
use crate::app::{App, AppRef};
use javascriptcore::ValueExt;
use webkit2gtk::WebViewExt;

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
    let weak = app.weak();
    view.run_javascript(&script, None::<&gtk::gio::Cancellable>, move |result| {
        let ok = result
            .ok()
            .and_then(|r| r.js_value())
            .map(|value| value.to_str() != "false")
            .unwrap_or(false);
        if !ok {
            if let Some(app) = weak.upgrade() {
                app.flash_error("Nothing to read on this page");
            }
        }
    });
}

#[allow(dead_code)]
pub fn unused(_: &App) {}
