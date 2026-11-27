// Suggestions from the search engine, in the address field and start page.
// Enabled by default with clear Settings disclosure and opt-out: search words
// you type are sent to the chosen engine as you type.
// Never from a private tab, never for a Ctrl K switcher, never for a keyword
// shortcut, and never for text that looks like an address, a file, an email
// or a key. See the reference EngineSuggest.swift for the original rules.

use crate::store::Prefs;
use gtk::gio;
use gtk::prelude::{EntryExt, StackExt};
use gtk::gio::prelude::CancellableExt;
use soup::prelude::SessionExt;
use std::cell::RefCell;
use std::collections::HashSet;
use std::time::Duration;

/// How long typing has to pause before a question goes out.
const PAUSE: Duration = Duration::from_millis(120);
/// An answer later than this is no use to someone typing.
const PATIENCE: Duration = Duration::from_millis(1000);
/// At most this many rows from the engine.
const MOST: usize = 6;
/// Longer than this, it is a paragraph or a paste, not a search.
const LONGEST: usize = 80;
/// Standard WebKitGTK desktop browser User-Agent (matching src/web.rs) so
/// suggestion endpoints like Startpage and Ecosia treat live browser queries
/// consistently instead of rejecting an empty libsoup header.
const BROWSER_UA: &str =
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";

thread_local! {
    // One session for every question: no cookie jar and no cache are added,
    // so none is kept. Redirects are refused per message.
    static SESSION: soup::Session = {
        let session = soup::Session::new();
        session.set_user_agent(BROWSER_UA);
        session
    };
    static STATE: RefCell<State> = RefCell::new(State::default());
}

#[derive(Default)]
struct State {
    // Bumped on every new question or engine switch, so late answers for older
    // queries or previous search engines are dropped.
    round: u64,
    cancel: Option<gio::Cancellable>,
    // The engine and trimmed text the words are for, and whether a question is
    // out or answered.
    engine: String,
    asked: String,
    in_flight: bool,
    answered: bool,
    words: Vec<String>,
}

impl State {
    fn begin(&mut self, engine: &str, asked: &str) -> Option<u64> {
        if self.engine == engine && self.asked == asked && (self.in_flight || self.answered) {
            return None;
        }
        self.round += 1;
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        self.engine = engine.to_string();
        self.asked = asked.to_string();
        self.in_flight = true;
        self.answered = false;
        self.words.clear();
        Some(self.round)
    }

    fn accept_landing(&mut self, round: u64, engine: &str, asked: &str, words: Vec<String>) -> bool {
        if self.round != round || self.engine != engine || self.asked != asked {
            return false;
        }
        self.words = words;
        self.in_flight = false;
        self.answered = true;
        self.cancel = None;
        true
    }

    fn rows_for(&self, engine: &str, asked: &str) -> Vec<String> {
        if self.answered && self.engine == engine && self.asked == asked {
            self.words.clone()
        } else {
            Vec::new()
        }
    }

    fn clear(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
        let round = self.round + 1;
        *self = State { round, ..State::default() };
    }
}

/// The engine's OpenSearch endpoint, with %s where the words go.
pub fn endpoint(engine: &str) -> Option<&'static str> {
    match engine {
        "Startpage" => Some("https://www.startpage.com/osuggestions?q=%s"),
        "Ecosia" => Some("https://ac.ecosia.org/autocomplete?q=%s&type=list"),
        "DuckDuckGo" => Some("https://duckduckgo.com/ac/?q=%s&type=list"),
        "Google" => Some("https://suggestqueries.google.com/complete/search?client=firefox&q=%s"),
        "Bing" => Some("https://www.bing.com/osjson.aspx?query=%s"),
        "Brave" => Some("https://search.brave.com/api/suggest?q=%s"),
        _ => None,
    }
}

/// The question for these words, or None when it is https only and not an engine.
pub fn url_for(engine: &str, words: &str) -> Option<String> {
    let template = endpoint(engine)?;
    let url = template.replace("%s", &crate::store::urlencode(words.trim()));
    url.starts_with("https://").then_some(url)
}

/// The question to send for this field's text, or None when nothing may be asked.
pub fn url_if_asked(typed: &str, prefs: &Prefs, private: bool, switching: bool) -> Option<String> {
    let text = typed.trim();
    if !prefs.engine_suggestions || private || switching || !sendable(text) {
        return None;
    }
    if crate::keyword::find(text, &prefs.keywords).is_some() {
        return None;
    }
    url_for(&prefs.search_engine, text)
}

/// Whether these words may leave the machine at all. Only plain words do:
/// nothing with a scheme, nothing shaped like a host, a path or an email,
/// nothing long, and nothing that reads like a password or a key.
pub fn sendable(typed: &str) -> bool {
    let text = typed.trim();
    if text.is_empty() || text.chars().count() > LONGEST || text.chars().any(|c| c == '\n' || c == '\r') {
        return false;
    }
    if text.contains("://") || text.contains('\\') {
        return false;
    }
    let tokens: Vec<&str> = text.split_whitespace().collect();
    let first = tokens[0];
    // A path on this machine, or one of a project's files.
    if first.starts_with(|c: char| c == '/' || c == '~') || first.starts_with("./") || first.starts_with("../") {
        return false;
    }
    // A scheme of any kind: mailto:x, javascript:..., localhost:3000.
    if let Some((scheme, rest)) = first.split_once(':') {
        if !rest.is_empty()
            && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
        {
            return false;
        }
    }
    // A single token that is a host, a path or localhost.
    if tokens.len() == 1 && (text.contains('.') || text.contains('/') || text.eq_ignore_ascii_case("localhost")) {
        return false;
    }
    // Every word is checked, not only a lone one: "192.168.1.1 admin",
    // "jira.corp.internal PROJ-123" and "alex@buildbox" name a machine or
    // an account even when other words follow them.
    for (index, token) in tokens.iter().enumerate() {
        if token.contains('@') || token.contains('/') {
            return false;
        }
        if host_like(token) {
            return false;
        }
        // A label followed by a value, as in "password: hunter22" or
        // "token: abc": what comes after it is not sent.
        if token.ends_with(':') && index + 1 < tokens.len() {
            return false;
        }
        if secret_like(token) {
            return false;
        }
    }
    true
}

/// Suffixes that only name machines on a private network.
const INTERNAL: &[&str] = &["local", "internal", "lan", "corp", "home", "intranet", "localdomain", "localhost"];

/// A word shaped like a machine: an IP address, localhost, a name of three
/// or more dotted labels, or a name ending in a private-network suffix.
/// Two-label words such as "node.js" or "3.12" stay searchable.
fn host_like(token: &str) -> bool {
    let word = token.trim_matches(|c: char| ",;()[]\"'".contains(c));
    // An IPv6 address is full of colons, so it is checked before a port is
    // split off ("::1" would otherwise leave nothing to look at).
    if word.parse::<std::net::IpAddr>().is_ok() {
        return true;
    }
    let bare = word.split_once(':').map_or(word, |(host, _)| host);
    if bare.is_empty() {
        return false;
    }
    if bare.parse::<std::net::IpAddr>().is_ok() || bare.eq_ignore_ascii_case("localhost") {
        return true;
    }
    let labels: Vec<&str> = bare.trim_end_matches('.').split('.').collect();
    if labels.len() < 2 || labels.iter().any(|l| l.is_empty()) {
        return false;
    }
    if !labels.iter().all(|l| l.chars().all(|c| c.is_alphanumeric() || c == '-')) {
        return false;
    }
    let last = labels[labels.len() - 1].to_ascii_lowercase();
    labels.len() >= 3 || INTERNAL.contains(&last.as_str())
}

/// A token that reads like a password, a key, a card or an account number.
fn secret_like(token: &str) -> bool {
    let length = token.chars().count();
    let letters = token.chars().any(char::is_alphabetic);
    let digits = token.chars().any(|c| c.is_ascii_digit());
    // Eight or more characters mixing letters and digits reads like a
    // password ("hunter22") more often than a word, so it is kept back.
    if length >= 8 && letters && digits {
        return true;
    }
    // A run of nine or more digits anywhere in the token.
    if token.split(|c: char| !c.is_ascii_digit()).any(|run| run.len() >= 9) {
        return true;
    }
    // Long and unbroken, in the alphabet keys are written in.
    length >= 24 && token.chars().all(|c| c.is_ascii_alphanumeric() || "+/=_-".contains(c))
}

/// The words in an OpenSearch answer, ["what was asked", ["one", "two"]]:
/// trimmed, each once (case-insensitively), none the same as what was typed,
/// at most MOST.
pub fn parse(bytes: &[u8], asked: &str) -> Vec<String> {
    let Ok(serde_json::Value::Array(top)) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return Vec::new();
    };
    let Some(serde_json::Value::Array(words)) = top.get(1) else {
        return Vec::new();
    };
    let typed = asked.trim().to_lowercase();
    let mut seen = HashSet::from([typed]);
    let mut out = Vec::new();
    for word in words {
        let Some(text) = word.as_str() else { continue };
        let clean = text.trim();
        if clean.is_empty()
            || clean.chars().count() > 200
            || clean.chars().any(|c| c == '\n' || c == '\r')
            || !seen.insert(clean.to_lowercase())
        {
            continue;
        }
        out.push(clean.to_string());
        if out.len() == MOST {
            break;
        }
    }
    out
}

/// The words the last answer gave for exactly this engine and text, if one came back.
pub fn rows_for(engine: &str, asked: &str) -> Vec<String> {
    STATE.with(|s| s.borrow().rows_for(engine, asked))
}

/// Asks for `asked` (trimmed) using `engine` after the pause. Asking again for
/// the same engine and text while it is in flight or answered does nothing; new
/// text or a new engine cancels the question before.
pub fn ask(engine: &str, asked: &str, url: String) {
    let round = STATE.with(|s| s.borrow_mut().begin(engine, asked));
    let Some(round) = round else { return };
    let engine = engine.to_string();
    let asked = asked.to_string();
    glib::timeout_add_local_once(PAUSE, move || {
        if !is_current(round) {
            return;
        }
        let cancel = gio::Cancellable::new();
        STATE.with(|s| s.borrow_mut().cancel = Some(cancel.clone()));
        fetch(&url, cancel, move |bytes| {
            let words = bytes.map(|b| parse(&b, &asked)).unwrap_or_default();
            landed(round, &engine, &asked, words)
        });
    });
}

/// Nothing is wanted any more: the field emptied, went away, engine changed,
/// or holds text not to be sent.
pub fn stop() {
    STATE.with(|s| s.borrow_mut().clear());
}

fn is_current(round: u64) -> bool {
    STATE.with(|s| s.borrow().round == round)
}

fn landed(round: u64, engine: &str, asked: &str, words: Vec<String>) {
    let current = STATE.with(|s| s.borrow_mut().accept_landing(round, engine, asked, words));
    if !current {
        return;
    }
    if let Some(app) = crate::app::current() {
        let prefs = app.prefs.borrow().clone();
        if !prefs.engine_suggestions || prefs.search_engine != engine {
            return;
        }
        let private = app.state.borrow().active_tab().is_none_or(|tab| tab.private);
        if private {
            return;
        }
        let omni_text = app.omni.entry.text();
        if app.omni.visible.get() && !crate::app::tab_switcher_on() && omni_text.trim() == asked {
            app.refresh_offers(&omni_text);
            return;
        }
        let on_blank = app.ui.stack.visible_child_name().as_deref() == Some("blank");
        let panel_open = app.panels.borrow().open.is_some();
        let start_text = app.ui.start.search.text();
        if on_blank && !app.omni.visible.get() && !panel_open && start_text.trim() == asked {
            app.ui.start.refresh_suggestions_async();
        }
    }
}

/// One GET, no redirects followed, cancelled after PATIENCE or when the text changes.
/// `done` always runs once: None when there is no answer to use.
fn fetch(url: &str, cancel: gio::Cancellable, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
    let Ok(msg) = soup::Message::new("GET", url) else {
        done(None);
        return;
    };
    msg.set_flags(soup::MessageFlags::NO_REDIRECT);
    if let Some(headers) = msg.request_headers() {
        headers.append("Accept", "application/json, text/javascript;q=0.9");
    }
    let late = cancel.clone();
    glib::timeout_add_local_once(PATIENCE, move || late.cancel());
    let check = msg.clone();
    SESSION.with(|session| {
        session.send_and_read_async(&msg, glib::Priority::DEFAULT, Some(&cancel), move |result| {
            let bytes = match result {
                Ok(bytes) if check.status_code() == 200 => Some(bytes.to_vec()),
                _ => None,
            };
            done(bytes);
        })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_words_are_sent() {
        assert!(sendable("coffee"));
        assert!(sendable("rust gtk tutorial"));
        // Two-label words and short versions stay searchable.
        assert!(sendable("node.js tutorial"));
        assert!(sendable("python 3.12 release"));
        assert!(sendable("what is a colon:"));
        assert!(sendable("mp3 player"));
    }

    // Critic 2, round 2 (F4): a machine, an account or a labelled value is
    // never sent, even when other words come with it.
    #[test]
    fn hosts_accounts_and_labelled_values_are_never_sent_among_other_words() {
        for typed in [
            "192.168.1.1 admin",
            "admin 10.0.0.1",
            "::1 login",
            "jira.corp.internal PROJ-123",
            "build.example.org failing",
            "nas.local backup",
            "printer.lan setup",
            "localhost admin",
            "alex@buildbox",
            "ssh alex@buildbox",
            "password: hunter22",
            "token: abc",
            "hunter22",
            "my hunter22 login",
            "open src/main.rs",
        ] {
            assert!(!sendable(typed), "{typed:?} must not be sent");
        }
    }

    #[test]
    fn addresses_paths_and_secrets_are_never_sent() {
        for typed in [
            "", "   ", "https://example.com", "http://x", "localhost", "localhost:3000", "mailto:a@b.co",
            "javascript:alert(1)", "example.com", "example.com/path", "a@b.com", "/etc/passwd", "~/notes",
            "./main.rs", "../x", "C:\\Users", "line\nbreak", "abc123def456", "4111111111111111", "a1b2c3d4e5f6g7h8",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(!sendable(typed), "{typed:?} must not be sent");
        }
        assert!(!sendable(&format!("{} {}", "word ".repeat(16), "tail")));
        assert!(sendable(&format!("{}{}", "word ".repeat(15), "tail")));
    }

    #[test]
    fn parse_trims_dedupes_drops_the_typed_text_and_caps_at_six() {
        let body = br#"["cof", [" coffee ", "COFFEE", "cof", "coffee maker", "", "coffee beans", "x1", "x2", "x3"]]"#;
        assert_eq!(
            parse(body, "Cof"),
            vec!["coffee", "coffee maker", "coffee beans", "x1", "x2", "x3"]
        );
    }

    #[test]
    fn parse_refuses_what_is_not_the_shape() {
        assert!(parse(b"not json", "x").is_empty());
        assert!(parse(br#"["x", "nope"]"#, "x").is_empty());
        assert!(parse(b"<html><head><title>Access Denied - Startpage</title></head></html>", "linux").is_empty());
    }

    #[test]
    fn parses_startpage_and_ecosia_opensearch_shapes() {
        let startpage = br#"["linux",["linux","linux kernel","linux distribution","linux lite"]]"#;
        assert_eq!(
            parse(startpage, "linux"),
            vec!["linux kernel", "linux distribution", "linux lite"]
        );
        let ecosia = br#"["linux",["linux mint","linux","linux ubuntu","linux download"]]"#;
        assert_eq!(
            parse(ecosia, "linux"),
            vec!["linux mint", "linux ubuntu", "linux download"]
        );
    }

    #[test]
    fn stale_query_and_stale_engine_responses_are_suppressed() {
        let mut state = State::default();
        let r1 = state.begin("Startpage", "lin").expect("r1 starts");
        let r2 = state.begin("Startpage", "linux").expect("r2 supersedes r1");
        assert_ne!(r1, r2);
        // Late response for superseded query "lin" must be dropped.
        assert!(!state.accept_landing(r1, "Startpage", "lin", vec!["linear".into()]));
        assert!(state.rows_for("Startpage", "lin").is_empty());

        // Switching engine for the exact same query supersedes the in-flight request.
        let r3 = state.begin("Ecosia", "linux").expect("engine change starts new round");
        assert!(!state.accept_landing(r2, "Startpage", "linux", vec!["startpage row".into()]));
        assert!(state.rows_for("Startpage", "linux").is_empty());
        assert!(state.rows_for("Ecosia", "linux").is_empty());

        assert!(state.accept_landing(r3, "Ecosia", "linux", vec!["linux mint".into()]));
        assert_eq!(state.rows_for("Ecosia", "linux"), vec!["linux mint"]);
        assert!(state.rows_for("Startpage", "linux").is_empty());

        // stop() clears cached rows and rejects any late landing from r3.
        state.clear();
        assert!(state.rows_for("Ecosia", "linux").is_empty());
        assert!(!state.accept_landing(r3, "Ecosia", "linux", vec!["late".into()]));
    }

    #[test]
    fn url_for_is_https_only_and_escapes_the_words() {
        assert_eq!(
            url_for("Startpage", "a b&c").as_deref(),
            Some("https://www.startpage.com/osuggestions?q=a+b%26c")
        );
        assert_eq!(
            url_for("Ecosia", "a b&c").as_deref(),
            Some("https://ac.ecosia.org/autocomplete?q=a+b%26c&type=list")
        );
        assert_eq!(
            url_for("DuckDuckGo", "a b&c").as_deref(),
            Some("https://duckduckgo.com/ac/?q=a+b%26c&type=list")
        );
        assert_eq!(
            url_for("Google", "rust").as_deref(),
            Some("https://suggestqueries.google.com/complete/search?client=firefox&q=rust")
        );
        assert_eq!(url_for("Unknown", "rust"), None);
        for engine in Prefs::engines() {
            assert!(url_for(engine, "x").is_some_and(|u| u.starts_with("https://")), "{engine}");
        }
    }

    // The privacy gates themselves: what decides whether anything leaves the
    // machine at all. Enabled by default for new profiles, respects explicit
    // opt-out, and never sends from a private tab, Ctrl K switcher, keyword
    // shortcut, or URL/email/path/secret.
    #[test]
    fn nothing_is_asked_when_off_private_switching_or_a_keyword() {
        let mut prefs = Prefs::default();
        assert!(prefs.engine_suggestions, "suggestions default on for new profiles");
        assert_eq!(prefs.search_engine, "Startpage");
        assert_eq!(
            url_if_asked("coffee", &prefs, false, false).as_deref(),
            Some("https://www.startpage.com/osuggestions?q=coffee")
        );

        assert_eq!(url_if_asked("coffee", &prefs, true, false), None, "a private tab never asks");
        assert_eq!(url_if_asked("coffee", &prefs, false, true), None, "the Ctrl K switcher never asks");
        assert_eq!(url_if_asked("coffee", &prefs, true, true), None);
        assert!(
            crate::keyword::find("yt cats", &prefs.keywords).is_some(),
            "the default yt keyword must exist for this check to mean anything"
        );
        assert_eq!(url_if_asked("yt cats", &prefs, false, false), None, "a keyword shortcut never asks");
        assert_eq!(url_if_asked("example.com", &prefs, false, false), None, "an address never asks");
        assert_eq!(url_if_asked("   ", &prefs, false, false), None);

        prefs.engine_suggestions = false;
        assert_eq!(url_if_asked("coffee", &prefs, false, false), None, "opt-out means nothing is sent");
    }

    // Makes real requests to Startpage, Ecosia, and DuckDuckGo to confirm answer shapes.
    // Run with: cargo test --release live_ -- --ignored --nocapture
    #[test]
    #[ignore]
    fn live_startpage_and_ecosia_answer_in_the_documented_shape() {
        let session = soup::Session::new();
        session.set_user_agent(BROWSER_UA);
        for engine in ["Startpage", "Ecosia", "DuckDuckGo"] {
            let url = url_for(engine, "linux").expect("an https question");
            let msg = soup::Message::new("GET", &url).expect("a message");
            msg.set_flags(soup::MessageFlags::NO_REDIRECT);
            if let Some(headers) = msg.request_headers() {
                headers.append("Accept", "application/json, text/javascript;q=0.9");
            }
            let bytes = session
                .send_and_read(&msg, None::<&gio::Cancellable>)
                .expect("an answer from engine");
            assert_eq!(msg.status_code(), 200, "engine={engine}");
            let words = parse(&bytes, "linux");
            eprintln!("{engine} said: {words:?}");
            assert!(!words.is_empty(), "engine={engine} returned empty suggestions");
        }
    }
}
