// Running script in a page. One helper, on the API that replaced
// webkit_web_view_run_javascript, so the rest of the browser never has to know
// which of the two it is holding.
use javascriptcore::ValueExt;
use webkit2gtk::{WebView, WebViewExt};

pub fn run<F>(view: &WebView, code: &str, done: F)
where
    F: FnOnce(Result<String, String>) + 'static,
{
    view.evaluate_javascript(code, None, None, None::<&gtk::gio::Cancellable>, move |result| {
        done(match result {
            Ok(value) => Ok(value.to_str().to_string()),
            Err(error) => Err(error.to_string()),
        })
    });
}

/// Evaluates JavaScript inside a named isolated WebKit script world so page
/// scripts in the main world cannot read, spoof, or tamper with browser state.
pub fn run_in_world<F>(view: &WebView, world: &str, code: &str, done: F)
where
    F: FnOnce(Result<String, String>) + 'static,
{
    view.evaluate_javascript(
        code,
        Some(world),
        None,
        None::<&gtk::gio::Cancellable>,
        move |result| {
            done(match result {
                Ok(value) => Ok(value.to_str().to_string()),
                Err(error) => Err(error.to_string()),
            })
        },
    );
}
