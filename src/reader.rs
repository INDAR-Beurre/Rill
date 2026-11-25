// Reading mode: the page's own words, set in the browser's type, with nothing
// around them. Done in the page rather than beside it, so it costs no second
// view and no third process.
use crate::app::AppRef;
use crate::palette::Reader;

const TOGGLE_ON: &str = include_str!("../assets/reader.js");

/// The call appended to reader.js. The page answers with one word and the
/// host reads only that word. A page where the function never arrived answers
/// `missing`, so absence is a stated answer rather than a value that happens
/// to compare unequal to a failure.
const CALL_OPEN: &str = concat!(
    "\n(function () {\n",
    "  if (typeof window.__searchReader !== 'function') return 'missing';\n",
    "  return String(window.__searchReader("
);
const CALL_CLOSE: &str = "));\n})();";

/// The page script: reader.js whole, then the call with this palette's tokens.
/// Kept out of `enter` so a test can read it without a web view.
fn script_for(tokens: &Reader) -> String {
    let css = format!("background:{};color:{}", tokens.ground.hex(), tokens.ink.hex());
    let arguments = serde_json::json!({
        "palette": css, "ground": tokens.ground.hex(),
        "muted": tokens.muted.hex(), "rule": tokens.rule.hex(),
    }).to_string();
    // The source contains JavaScript braces; only the argument placeholder
    // belongs to Rust's formatter.
    TOGGLE_ON.to_owned() + CALL_OPEN + &arguments + CALL_CLOSE
}

/// What the page said it did. Only these words move a tab's state; any other
/// value is a page that answered something else, and guessing from it would
/// flip the flag of a page that never changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Answer {
    Entered,
    Left,
    Empty,
    Missing,
    Failed,
    Unreadable,
}

fn answer(word: &str) -> Answer {
    match word.trim() {
        "entered" => Answer::Entered,
        "left" => Answer::Left,
        "empty" => Answer::Empty,
        "missing" => Answer::Missing,
        "failed" => Answer::Failed,
        _ => Answer::Unreadable,
    }
}

/// What a toast can hold of a WebKit error: the callback string carries the
/// source line, the URL and a stack trace, and only its first line is about
/// the page. A toast is one line wide, so cut it there.
fn brief_error(message: &str) -> String {
    const WIDTH: usize = 80;
    let line = message
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty() && !line.starts_with("at "))
        .unwrap_or("the page script failed");
    let mut text: String = line.chars().take(WIDTH).collect();
    if text.chars().count() < line.chars().count() {
        text = text.trim_end().to_string();
        text.push('…');
    }
    format!("Reading mode failed: {text}")
}

pub fn enter(app: &AppRef, view: &webkit2gtk::WebView) {
    let script = script_for(&app.palette.borrow().reader());
    let Some(id) = crate::app::view_id(view) else { return };
    let weak = app.weak();
    crate::js::run(view, &script, move |result| {
        let Some(app) = weak.upgrade() else { return };
        // The page is the only witness: it reports what it did, and only an
        // entry or a departure may change the tab's state. Anything else, a
        // script that never ran or a word the host did not ask for, leaves the
        // page and the flag as they were.
        match result {
            Ok(word) => match answer(&word) {
                Answer::Entered => {
                    app.reading_tabs.borrow_mut().insert(id);
                    app.reading.set(app.tab_reading(app.active_id()));
                    // refresh_helm is the only place that puts `.reading` on
                    // the toolbar's book button (src/app.rs refresh_helm).
                    // Entering or leaving reader mode did not call it, so the
                    // button kept its old tint until something else redrew
                    // the toolbar (Gemini R60 bug 4). It is one class toggle
                    // on widgets that already exist, with no new allocation.
                    app.refresh_helm();
                    // The find band depends on reading mode (it clears the
                    // reader's exit pill), so an open Find bar moves now.
                    app.layout_layers(
                        gtk::prelude::WidgetExt::allocated_width(&app.ui.overlay),
                        gtk::prelude::WidgetExt::allocated_height(&app.ui.overlay),
                    );
                }
                Answer::Left => {
                    app.reading_tabs.borrow_mut().remove(&id);
                    app.reading.set(app.tab_reading(app.active_id()));
                    app.refresh_helm();
                    app.layout_layers(
                        gtk::prelude::WidgetExt::allocated_width(&app.ui.overlay),
                        gtk::prelude::WidgetExt::allocated_height(&app.ui.overlay),
                    );
                }
                Answer::Empty => app.flash_error("Nothing to read on this page"),
                // The page stayed in reading mode because it could not rebuild
                // its own markup; the flag already matches that, so only the
                // reason needs saying.
                Answer::Failed => app.flash_error("Reading mode could not put the page back"),
                // reader.js never reached the page, or ran under a policy that
                // stopped the call. The tab is untouched and the reason said.
                Answer::Missing => app.flash_error("Reading mode did not load in this page"),
                // A page answering something else may or may not have changed.
                // Guessing from that word would flip a tab on no evidence.
                Answer::Unreadable => {}
            },
            Err(message) => app.flash_error(&brief_error(&message)),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palette::Palette;
    use crate::store::Look;

    #[test]
    fn the_script_keeps_its_object_and_its_braces() {
        let script = script_for(&Palette::for_look(Look::Light, false).reader());
        assert!(script.starts_with(TOGGLE_ON), "reader.js was not sent whole");
        let call = script
            .strip_prefix(TOGGLE_ON)
            .and_then(|rest| rest.strip_prefix(CALL_OPEN))
            .and_then(|rest| rest.strip_suffix(CALL_CLOSE))
            .expect("the call is not the wrapper the host sends");
        let value: serde_json::Value = serde_json::from_str(call).expect("arguments are not JSON");
        assert!(value["palette"].as_str().is_some_and(|v| v.starts_with("background:#")));
        for key in ["ground", "muted", "rule"] {
            let hex = value.get(key).and_then(|v| v.as_str()).unwrap_or("");
            assert!(hex.starts_with('#'), "{key} is {hex:?}, not a colour");
        }
    }

    #[test]
    fn the_call_states_a_missing_function_instead_of_staying_silent() {
        let script = script_for(&Palette::for_look(Look::Dark, false).reader());
        // A short-circuit call would answer `null` where the function is gone,
        // and a host that only looks for `false` would read that as an entry.
        assert!(!script.contains("&& window.__searchReader("), "the call still short-circuits");
        assert!(script.contains("return 'missing'"), "absence is not stated");
    }

    #[test]
    fn reading_text_has_visible_size_controls() {
        let script = script_for(&Palette::for_look(Look::Light, false).reader());
        assert!(script.contains("window.__searchReaderFont"), "no font helper");
        assert!(script.contains("--search-reader-size"), "no size variable");
        assert!(script.contains("Smaller reading text"), "no smaller control");
        assert!(script.contains("Larger reading text"), "no larger control");
        assert!(script.contains("|| 18"), "the default is not the middle step");
        assert!(script.contains("Math.max(14"), "the small step is missing");
        assert!(script.contains("Math.min(22"), "the large step is missing");
        assert!(script.contains("['A-', -4"), "the controls do not step 14/18/22");
        assert!(script.contains("['A+', 4"), "the controls do not step 14/18/22");
    }

    #[test]
    fn only_entered_and_left_move_a_tab() {
        assert_eq!(answer("entered"), Answer::Entered);
        assert_eq!(answer("left"), Answer::Left);
        assert_eq!(answer("empty"), Answer::Empty);
        assert_eq!(answer("missing"), Answer::Missing);
        assert_eq!(answer("failed"), Answer::Failed);
        // Everything else is a page answering something the host did not ask
        // for, and must not be read as either a yes or a no.
        for word in ["", "true", "false", "null", "undefined", "OK", "{}\n", "entered!"] {
            assert_eq!(answer(word), Answer::Unreadable, "{word:?} moved a tab");
        }
        // Surrounding whitespace is the same word: WebKit pads nothing, but a
        // page that answers through a template might.
        assert_eq!(answer(" entered\n"), Answer::Entered);
        assert_eq!(answer("  left  "), Answer::Left);
    }

    #[test]
    fn the_tab_flips_on_those_two_words_and_no_others() {
        // The rule the host applies, independent of a web view: entry adds the
        // tab, departure removes it, anything else leaves the set alone.
        fn apply(reading: &mut std::collections::BTreeSet<u32>, word: &str, tab: u32) {
            match answer(word) {
                Answer::Entered => {
                    reading.insert(tab);
                }
                Answer::Left => {
                    reading.remove(&tab);
                }
                _ => {}
            }
        }
        let mut reading: std::collections::BTreeSet<u32> = Default::default();
        apply(&mut reading, "entered", 7);
        assert!(reading.contains(&7), "entry did not record the tab");
        apply(&mut reading, "entered", 7);
        assert_eq!(reading.len(), 1, "a repeated entry doubled the tab");
        for word in ["empty", "missing", "failed", "", "false", "true"] {
            apply(&mut reading, word, 7);
            assert!(reading.contains(&7), "{word:?} took the tab out of reading mode");
        }
        apply(&mut reading, "left", 7);
        assert!(reading.is_empty(), "departure did not clear the tab");
        // Leaving a tab that was never in the mode is a no-op, not a flip of
        // whichever tab happens to be active.
        apply(&mut reading, "left", 9);
        assert!(reading.is_empty());
        apply(&mut reading, "entered", 9);
        apply(&mut reading, "left", 7);
        assert!(reading.contains(&9), "another tab's departure touched this one");
    }

    #[test]
    fn a_script_error_becomes_one_short_line() {
        let error = "ReferenceError: Can't find variable: __searchReader\n\n    at <anonymous>:1:1\n    at file:///tmp/page.html:3:9";
        let line = brief_error(error);
        assert_eq!(line, "Reading mode failed: ReferenceError: Can't find variable: __searchReader");
        assert!(!line.contains('\n'), "the toast would grow past one line");

        let long = brief_error(&format!("Error: {}", "x".repeat(400)));
        assert!(long.ends_with('…'), "a long error was not cut: {long:?}");
        let body = long.trim_end_matches('…');
        let body = body.strip_prefix("Reading mode failed: ").expect("no prefix");
        assert_eq!(body.trim_end().chars().count(), 80);

        // An error with no lines we can trust still says what failed.
        assert_eq!(brief_error(" \n  "), "Reading mode failed: the page script failed");
    }
}

