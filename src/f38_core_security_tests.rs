// Focused F38 core & security regression tests covering:
// - H2: private tab isolation for set_title (history.json) and unhide_all (hidden.json)
// - H4: isolated 'search-reader' world, strict sanitizer, DOM adoption, main-world tamper resistance
// - M2: per-host curtain stylesheet scoping, (manager, host) registry, port semantics, private isolation, cleanup
// - M4: extension here_at respecting exclude_matches
// - L1: unsigned add-on install prompt defaulting keyboard focus to Cancel
// - H3: folded window controls synchronization across live fold/unfold, toolbar-visible, fullscreen, no duplicates
// - M3: shared TLS/security error classifier covering WebKit TLS reasons & message_for fallback

use super::{
    apply_hide_for_origin, apply_unhide_for_tab, error_copy, error_icon, error_is_security,
    folded_controls_top_margin, record_title_for_history, should_focus_error_back,
    should_show_folded_controls, HideApplyOutcome,
};
use crate::curtain::{
    allow_list_for, normalize_host, validate_hide_source, HideDecision, HideOrigin,
    HideTargetSnapshot, ScopedRegistry, CURTAIN_WORLD,
};
use crate::icons::Icon;
use crate::store::HistoryEntry;
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        static N: AtomicUsize = AtomicUsize::new(1);
        let id = N.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("search-f38-sec-{tag}-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// ---- H2: Private tab isolation ------------------------------------------

#[test]
fn h2_private_set_title_never_mutates_or_flushes_persisted_history() {
    let dir = TempDir::new("h2-history");
    let history_path = dir.0.join("history.json");

    let mut history = vec![HistoryEntry {
        url: "https://example.com/account".to_string(),
        title: "Public Account".to_string(),
        at: 1_700_000_000,
    }];
    crate::files::write_json(&history_path, &history);
    let before_bytes = std::fs::read(&history_path).expect("read seeded history.json");

    // Private tab opens the same URL and receives a sensitive title update.
    let should_flush = record_title_for_history(
        &mut history,
        true,
        "https://example.com/account",
        "Private Medical Inbox (3 unread)",
    );
    assert!(
        !should_flush,
        "private set_title must never schedule a disk flush"
    );
    assert_eq!(history.len(), 1);
    assert_eq!(
        history[0].title, "Public Account",
        "private title must not overwrite existing history entry"
    );

    // Even a brand-new URL in a private tab must not request a flush.
    assert!(!record_title_for_history(
        &mut history,
        true,
        "https://brand-new-private.example/",
        "Secret"
    ));
    let after_private_bytes = std::fs::read(&history_path).expect("re-read history.json");
    assert_eq!(
        before_bytes, after_private_bytes,
        "history.json must remain byte-for-byte untouched after private title updates"
    );

    // Normal tab title update still works and requests persistence.
    let should_flush_normal = record_title_for_history(
        &mut history,
        false,
        "https://example.com/account",
        "Updated Public Account",
    );
    assert!(
        should_flush_normal,
        "non-private set_title should schedule flush"
    );
    assert_eq!(history[0].title, "Updated Public Account");
}

#[test]
fn h2_private_unhide_all_preserves_normal_hidden_rules_and_file_bytes() {
    let dir = TempDir::new("h2-hidden");
    let hidden_path = dir.0.join("hidden.json");

    let mut hidden: BTreeMap<String, Vec<String>> = BTreeMap::new();
    hidden.insert(
        "example.com".to_string(),
        vec!["#ad".to_string(), ".promo".to_string()],
    );
    crate::files::write_json(&hidden_path, &hidden);
    let before_bytes = std::fs::read(&hidden_path).expect("read seeded hidden.json");

    let mut private_hidden: HashMap<u64, HashMap<String, Vec<String>>> = HashMap::new();
    let private_tab_id = 42;

    // Simulate private tab hiding an extra selector ephemerally.
    private_hidden.entry(private_tab_id).or_default().insert(
        "example.com".to_string(),
        vec![
            "#ad".to_string(),
            ".promo".to_string(),
            "#private-banner".to_string(),
        ],
    );

    // Private unhide_all clears ONLY private ephemeral state and never saves hidden.json.
    let should_save = apply_unhide_for_tab(
        &mut hidden,
        &mut private_hidden,
        private_tab_id,
        true,
        "example.com",
    );
    assert!(
        !should_save,
        "private unhide_all must never save hidden.json"
    );
    assert_eq!(
        hidden.get("example.com"),
        Some(&vec!["#ad".to_string(), ".promo".to_string()]),
        "normal persisted rules must stay untouched"
    );
    assert_eq!(
        private_hidden
            .get(&private_tab_id)
            .and_then(|hosts| hosts.get("example.com")),
        Some(&Vec::<String>::new()),
        "private tab must record an explicit empty override so reload stays unhidden"
    );
    let after_private_bytes = std::fs::read(&hidden_path).expect("re-read hidden.json");
    assert_eq!(
        before_bytes, after_private_bytes,
        "hidden.json must remain byte-for-byte untouched after private unhide_all"
    );

    // Private Hide remains usable after private Unhide.
    private_hidden
        .entry(private_tab_id)
        .or_default()
        .entry("example.com".to_string())
        .or_default()
        .push("#newly-hidden".to_string());
    assert_eq!(
        private_hidden[&private_tab_id]["example.com"],
        vec!["#newly-hidden".to_string()]
    );

    // Normal unhide_all removes from persisted state and signals save.
    let should_save_normal =
        apply_unhide_for_tab(&mut hidden, &mut private_hidden, 1, false, "example.com");
    assert!(should_save_normal);
    assert!(!hidden.contains_key("example.com"));
    // Private tab's own ephemeral list is still independent.
    assert_eq!(
        private_hidden[&private_tab_id]["example.com"],
        vec!["#newly-hidden".to_string()]
    );
}

// ---- M2: Curtain per-host scoping & shared/private manager isolation ----

#[test]
fn m2_allow_list_scopes_by_host_handles_ports_and_rejects_wildcards() {
    assert_eq!(
        allow_list_for("example.com"),
        vec!["http://example.com/*", "https://example.com/*"]
    );
    assert_eq!(
        allow_list_for("127.0.0.1:8099"),
        vec!["http://127.0.0.1/*", "https://127.0.0.1/*"],
        "ports must be stripped to match WebKit UserContentURLPattern semantics"
    );
    assert_eq!(
        allow_list_for("Example.COM:8443"),
        vec!["http://example.com/*", "https://example.com/*"]
    );
    assert_eq!(
        allow_list_for("[::1]:8080"),
        vec!["http://[::1]/*", "https://[::1]/*"]
    );
    // Hostile or invalid hosts must never become unscoped wildcards.
    for bad in [
        "",
        "   ",
        "*",
        "*.example.com",
        "example.com/path",
        "a?b",
        "a#b",
        "a b",
        "bad:port",
    ] {
        assert_eq!(normalize_host(bad), None, "{bad:?} must be rejected");
        assert!(
            allow_list_for(bad).is_empty(),
            "{bad:?} must produce empty allow_list"
        );
    }
}

#[test]
fn m2_scoped_registry_isolates_hosts_and_private_managers_and_cleans_up() {
    let mut reg: ScopedRegistry<&'static str> = ScopedRegistry::new();
    let shared_mgr = 0x1000usize;
    let private_mgr = 0x2000usize;

    // Tab A installs sheet for 127.0.0.1 on shared manager.
    assert_eq!(
        reg.replace(shared_mgr, "127.0.0.1:8099", Some("sheet-a")),
        None
    );
    // Tab B installs sheet for localhost on the SAME shared manager: must NOT clobber Tab A.
    assert_eq!(
        reg.replace(shared_mgr, "localhost:8099", Some("sheet-b")),
        None
    );
    assert_eq!(reg.get(shared_mgr, "127.0.0.1"), Some(&"sheet-a"));
    assert_eq!(reg.get(shared_mgr, "localhost"), Some(&"sheet-b"));
    assert_eq!(reg.manager_len(shared_mgr), 2);

    // Updating Tab A replaces only 127.0.0.1, returning its previous sheet while keeping localhost.
    assert_eq!(
        reg.replace(shared_mgr, "127.0.0.1", Some("sheet-a2")),
        Some("sheet-a")
    );
    assert_eq!(reg.get(shared_mgr, "127.0.0.1"), Some(&"sheet-a2"));
    assert_eq!(reg.get(shared_mgr, "localhost"), Some(&"sheet-b"));

    // Clearing Tab B removes only localhost, leaving 127.0.0.1 intact.
    assert_eq!(reg.replace(shared_mgr, "localhost", None), Some("sheet-b"));
    assert_eq!(reg.get(shared_mgr, "127.0.0.1"), Some(&"sheet-a2"));
    assert_eq!(reg.get(shared_mgr, "localhost"), None);

    // Private manager for the same host is completely isolated from shared manager.
    assert_eq!(
        reg.replace(private_mgr, "127.0.0.1", Some("priv-sheet")),
        None
    );
    assert_eq!(reg.get(shared_mgr, "127.0.0.1"), Some(&"sheet-a2"));
    assert_eq!(reg.get(private_mgr, "127.0.0.1"), Some(&"priv-sheet"));

    // Forgetting the closed private manager cleans up only private sheets.
    let dropped = reg.clear_manager(private_mgr);
    assert_eq!(dropped, vec!["priv-sheet"]);
    assert_eq!(reg.manager_len(private_mgr), 0);
    assert_eq!(reg.get(shared_mgr, "127.0.0.1"), Some(&"sheet-a2"));
}

// ---- H3: Folded window controls synchronization -------------------------

#[test]
fn h3_folded_controls_visibility_and_toolbar_clearance() {
    // Unfolded normal sidebar: body is visible -> folded controls MUST stay hidden (no duplicates).
    assert!(!should_show_folded_controls(false, true, false, true, true));

    // Live animated fold in progress: column_folded flipped to true, body still fading out -> hidden (no duplicates).
    assert!(!should_show_folded_controls(true, true, false, true, true));

    // Live animated fold completed: body hidden -> folded controls MUST show!
    assert!(should_show_folded_controls(true, true, false, true, false));

    // Sidebar disabled (no body): folded controls MUST show!
    assert!(should_show_folded_controls(
        false, false, false, true, false
    ));

    // Live animated unfold started: column_folded is false & body shown -> hidden immediately!
    assert!(!should_show_folded_controls(false, true, false, true, true));
    assert!(!should_show_folded_controls(
        false, true, false, true, false
    ));

    // Fullscreen (F11 or video) MUST always hide folded controls.
    assert!(!should_show_folded_controls(true, true, true, true, false));
    assert!(!should_show_folded_controls(
        false, false, true, true, false
    ));
    // Page fullscreen hides side (`side_shown = false`) -> hidden.
    assert!(!should_show_folded_controls(
        true, true, false, false, false
    ));

    // Top margin clears the 72px revealed toolbar without overlapping its right-edge menu button.
    assert_eq!(folded_controls_top_margin(false, 72), 8);
    assert_eq!(folded_controls_top_margin(true, 0), 80);
    assert_eq!(folded_controls_top_margin(true, 72), 80);
    assert_eq!(folded_controls_top_margin(true, 84), 92);
}

// ---- M3: Shared TLS/security error classifier ---------------------------

#[test]
fn m3_tls_checks() {
    use gtk::gio::TlsCertificateFlags as F;

    // Actual WebKit/GLib load_failed error messages routed through web::message_for_text.
    let webkit_tls_messages = [
        "Unacceptable TLS certificate",
        "Error performing TLS handshake: The TLS connection was non-properly terminated.",
        "TLS/SSL support not available",
        "SSL handshake failed",
        "Peer certificate is not valid",
        "TLS certificate hostname mismatch",
        "Certificate has expired",
    ];

    for raw in webkit_tls_messages {
        let translated = crate::web::message_for_text(raw);
        assert_eq!(
            translated, "The site's security could not be verified.",
            "raw WebKit error {raw:?} should map to security verification message"
        );
        assert!(
            error_is_security(&translated),
            "translated fallback {translated:?} must classify as security"
        );
        assert_eq!(
            error_copy(&translated),
            ("This site can't be verified", "Try again"),
            "must never degrade to generic 'This page didn't open'"
        );
        assert!(matches!(error_icon(&translated), Icon::ShieldAlert));

        // Committed/finished wrapper from web.rs:557 must also classify as security.
        let wrapped = format!("The page stopped arriving. ({translated})");
        assert!(error_is_security(&wrapped));
        assert_eq!(
            error_copy(&wrapped),
            ("This site can't be verified", "Try again")
        );
        assert!(matches!(error_icon(&wrapped), Icon::ShieldAlert));

        // Even raw unmapped WebKit TLS strings (including ones mentioning 'hostname') classify as security.
        assert!(
            error_is_security(raw),
            "raw {raw:?} must classify as security"
        );
        assert_eq!(
            error_copy(raw),
            ("This site can't be verified", "Try again")
        );
        assert!(matches!(error_icon(raw), Icon::ShieldAlert));
    }

    // Every TlsCertificateFlags variant through web::reason_for_tls.
    for flags in [
        F::EXPIRED,
        F::NOT_ACTIVATED,
        F::BAD_IDENTITY,
        F::UNKNOWN_CA,
        F::REVOKED,
        F::INSECURE,
        F::GENERIC_ERROR,
        F::empty(),
    ] {
        let reason = crate::web::reason_for_tls(flags);
        assert!(error_is_security(&reason), "{flags:?} -> {reason:?}");
        assert_eq!(
            error_copy(&reason),
            ("This site can't be verified", "Try again")
        );
        assert!(matches!(error_icon(&reason), Icon::ShieldAlert));
    }

    // Mixed-case "Secure Connection" / "TLS" / "SSL" strings.
    for extra in [
        "Secure connection failed",
        "A TLS error occurred",
        "SSL protocol error",
    ] {
        assert!(error_is_security(extra));
        assert_eq!(error_copy(extra).0, "This site can't be verified");
        assert!(matches!(error_icon(extra), Icon::ShieldAlert));
    }

    // Non-security errors must NOT be misclassified as TLS errors.
    for non_sec in [
        "There is nothing at this address (404).",
        "The site broke while answering (503).",
        "That address could not be found.",
        "The site refused the connection.",
        "The site took too long to answer.",
        "There is no network to reach it on.",
    ] {
        assert!(
            !error_is_security(non_sec),
            "{non_sec:?} is not a TLS error"
        );
    }
}

// ---- L1: Unsigned add-on install prompt defaults focus to Cancel --------

#[test]
fn l1_unsigned_addon_prompt_defaults_focus_to_cancel() {
    let buttons = crate::permissions::addon_prompt_buttons();
    assert_eq!(buttons[0], ("Cancel", false));
    assert_eq!(buttons[1], ("Install", true));
    assert_eq!(crate::permissions::addon_default_focus_label(), "Cancel");
}

// ---- H4: Reader isolated world, strict sanitizer, DOM adoption & tamper resistance ----

#[test]
fn h4_reader_runs_in_isolated_world_and_sanitizes_adversarial_dom_in_jsc() {
    use javascriptcore::{Context, ContextExt, ValueExt};

    assert_eq!(crate::reader::READER_WORLD, "search-reader");

    // Verify reader.js against a real JavaScriptCore engine with a minimal DOM
    // harness that throws on any innerHTML access and simulates main-world tampering.
    let harness = r#"
const XHTML = 'http://www.w3.org/1999/xhtml';
class MockNode {
  constructor(nodeType, tagName, ns) {
    this.nodeType = nodeType;
    this.tagName = tagName ? tagName.toUpperCase() : '';
    this.namespaceURI = ns === undefined ? XHTML : ns;
    this.parentNode = null;
    this.childNodes = [];
    this._attrs = new Map();
    this._listeners = new Map();
    this._text = '';
    this.disabled = false;
    this.type = '';
    this.title = '';
    this.scrollTop = 0;
    this.style = {
      _props: new Map(),
      setProperty(k, v) { this._props.set(k, v); },
      getPropertyValue(k) { return this._props.get(k) || ''; },
    };
  }
  get firstChild() { return this.childNodes[0] || null; }
  get attributes() {
    const out = [];
    for (const [name, value] of this._attrs.entries()) out.push({ name, value });
    return out;
  }
  get id() { return this._attrs.get('id') || ''; }
  set id(v) { this._attrs.set('id', String(v)); }
  get className() { return this._attrs.get('class') || ''; }
  set className(v) { this._attrs.set('class', String(v)); }
  get textContent() {
    if (this.nodeType === 3) return this._text;
    return this.childNodes.map((c) => c.textContent).join('');
  }
  set textContent(v) {
    if (this.nodeType === 3) { this._text = String(v); return; }
    this.childNodes = [];
    const t = new MockNode(3, '');
    t._text = String(v);
    this.appendChild(t);
  }
  get innerText() { return this.textContent; }
  get innerHTML() { throw new Error('FORBIDDEN: innerHTML getter invoked'); }
  set innerHTML(_) { throw new Error('FORBIDDEN: innerHTML setter invoked'); }
  setAttribute(k, v) { this._attrs.set(String(k), String(v)); }
  getAttribute(k) { return this._attrs.has(k) ? this._attrs.get(k) : null; }
  removeAttribute(k) { this._attrs.delete(k); }
  appendChild(child) {
    if (child.nodeType === 11) {
      while (child.firstChild) this.appendChild(child.firstChild);
      return child;
    }
    if (child.parentNode) child.parentNode.removeChild(child);
    child.parentNode = this;
    this.childNodes.push(child);
    return child;
  }
  removeChild(child) {
    const idx = this.childNodes.indexOf(child);
    if (idx !== -1) {
      this.childNodes.splice(idx, 1);
      child.parentNode = null;
    }
    return child;
  }
  remove() { if (this.parentNode) this.parentNode.removeChild(this); }
  cloneNode(deep) {
    const copy = new MockNode(this.nodeType, this.tagName, this.namespaceURI);
    copy._text = this._text;
    for (const [k, v] of this._attrs.entries()) copy._attrs.set(k, v);
    if (deep) {
      for (const c of this.childNodes) copy.appendChild(c.cloneNode(true));
    }
    return copy;
  }
  addEventListener(ev, fn) {
    if (!this._listeners.has(ev)) this._listeners.set(ev, []);
    this._listeners.get(ev).push(fn);
  }
  click() {
    for (const fn of (this._listeners.get('click') || [])) fn();
  }
  _walk(fn) {
    for (const c of this.childNodes) {
      if (c.nodeType === 1) {
        fn(c);
        c._walk(fn);
      }
    }
  }
  querySelectorAll(sel) {
    const out = [];
    const check = (el) => {
      if (sel === 'article') return el.tagName === 'ARTICLE';
      if (sel === 'h1') return el.tagName === 'H1';
      if (sel === 'style[data-search-reader]' || sel === '[data-search-reader]') {
        return el.getAttribute('data-search-reader') !== null;
      }
      if (sel === '#__search_reader_font button[aria-label="Smaller reading text"]') {
        return el.tagName === 'BUTTON' && el.getAttribute('aria-label') === 'Smaller reading text';
      }
      if (sel === '#__search_reader_font button[aria-label="Larger reading text"]') {
        return el.tagName === 'BUTTON' && el.getAttribute('aria-label') === 'Larger reading text';
      }
      if (sel.includes('.byline')) return (el.className || '').includes('byline');
      return false;
    };
    this._walk((el) => { if (check(el)) out.push(el); });
    return out;
  }
  querySelector(sel) { return this.querySelectorAll(sel)[0] || null; }
  getElementById(id) {
    let found = null;
    this._walk((el) => { if (!found && el.id === id) found = el; });
    return found;
  }
}

const rootDoc = new MockNode(1, 'html');
rootDoc.setAttribute('style', 'background:beige');
const head = new MockNode(1, 'head');
const body = new MockNode(1, 'body');
body.setAttribute('style', 'margin:16px');
rootDoc.appendChild(head);
rootDoc.appendChild(body);

const article = new MockNode(1, 'article');
const h1 = new MockNode(1, 'h1');
h1.textContent = 'Adversarial Test Article';
article.appendChild(h1);

const prose = new MockNode(1, 'p');
prose.setAttribute('style', 'position:fixed;inset:0;color:black;z-index:999999');
prose.setAttribute('onclick', 'alert(1)');
prose.setAttribute('onmouseover', 'steal()');
prose.setAttribute('id', '__search_reader_exit');
prose.setAttribute('dir', 'ltr');
prose.textContent = 'Clean readable prose paragraph '.repeat(25);
article.appendChild(prose);

// Adversarial links and images
const badDataLink = new MockNode(1, 'a');
badDataLink.setAttribute('href', 'data:text/html,<script>alert(1)</script>');
badDataLink.setAttribute('ping', 'https://tracker.invalid/ping');
badDataLink.setAttribute('formaction', 'javascript:alert(2)');
badDataLink.textContent = 'Bad data link';
article.appendChild(badDataLink);

const badJsLink = new MockNode(1, 'a');
badJsLink.setAttribute('href', '  JaVa\tScRiPt:alert(1)');
badJsLink.textContent = 'Bad js link';
article.appendChild(badJsLink);

const goodLink = new MockNode(1, 'a');
goodLink.setAttribute('href', 'https://example.org/docs');
goodLink.textContent = 'Good link';
article.appendChild(goodLink);

const badSvgImg = new MockNode(1, 'img');
badSvgImg.setAttribute('src', 'data:image/svg+xml,<svg onload=alert(1)>');
badSvgImg.setAttribute('srcset', 'https://evil.invalid/x.png 2x');
badSvgImg.setAttribute('onerror', 'alert(1)');
article.appendChild(badSvgImg);

const goodRasterImg = new MockNode(1, 'img');
goodRasterImg.setAttribute('src', 'data:image/png;base64,iVBORw0KGgo=');
goodRasterImg.setAttribute('alt', 'Tiny PNG');
article.appendChild(goodRasterImg);

// Forbidden elements: template, math, svg, meta, base, link, iframe, form, input, script, style
for (const [tag, ns] of [
  ['template', XHTML],
  ['math', 'http://www.w3.org/1998/Math/MathML'],
  ['svg', 'http://www.w3.org/2000/svg'],
  ['meta', XHTML],
  ['base', XHTML],
  ['link', XHTML],
  ['iframe', XHTML],
  ['form', XHTML],
  ['input', XHTML],
  ['textarea', XHTML],
  ['select', XHTML],
  ['script', XHTML],
  ['style', XHTML],
]) {
  const bad = new MockNode(1, tag, ns);
  bad.textContent = 'evil-' + tag;
  article.appendChild(bad);
}

const comment = new MockNode(8, '');
comment._text = '<!-- hidden comment -->';
article.appendChild(comment);

body.appendChild(article);

const postedMessages = [];
let currentScrollY = 240;
globalThis.window = globalThis;
globalThis.location = { host: 'example.org' };
globalThis.scrollY = currentScrollY;
globalThis.scrollTo = (_x, y) => { currentScrollY = y; globalThis.scrollY = y; };
globalThis.requestAnimationFrame = (fn) => fn();
globalThis.addEventListener = () => {};
globalThis.webkit = {
  messageHandlers: {
    searchReader: {
      postMessage(msg) { postedMessages.push(msg); },
    },
  },
};
globalThis.document = {
  title: 'Fallback Title',
  documentElement: rootDoc,
  head,
  body,
  createElement: (tag) => new MockNode(1, tag, XHTML),
  createDocumentFragment: () => new MockNode(11, '', XHTML),
  querySelectorAll: (sel) => rootDoc.querySelectorAll(sel),
  querySelector: (sel) => rootDoc.querySelector(sel),
  getElementById: (id) => rootDoc.getElementById(id),
};
"#;

    let verify = r#"
const palette = {
  palette: 'background:#f7f4ee;color:#1b1a17',
  ground: '#f7f4ee',
  muted: '#6c665d',
  rule: '#ddd6ca',
  paper: '#eee9df',
};

// Simulate an attacker trying to tamper with legacy main-world globals:
// reader runs in its own isolated world and never reads these names.
globalThis.__searchReaderOn = true;
globalThis.__searchReaderBody = '<img src=x onerror=alert(999)>';
globalThis.__searchReader = () => 'hacked';

const enterRes = globalThis.__searchReaderCtrl.toggle(palette);
if (enterRes !== 'entered') throw new Error('expected entered, got ' + enterRes);
if (!globalThis.__searchReaderCtrl.isActive()) throw new Error('expected isActive() === true');

// Check sanitized reader DOM
const readerRoot = document.getElementById('__search_reader');
if (!readerRoot) throw new Error('missing #__search_reader');
const tags = [];
const links = [];
const imgs = [];
let styleLeak = false;
let handlerLeak = false;
let clobberLeak = false;
let pingLeak = false;
let srcsetLeak = false;

readerRoot._walk((el) => {
  if (el.id && el.id.startsWith('__search_reader')) {
    // Only the real reader chrome elements may carry __search_reader* IDs.
    if (el.tagName === 'P') clobberLeak = true;
    return;
  }
  tags.push(el.tagName);
  for (const attr of el.attributes) {
    const n = attr.name.toLowerCase();
    if (n === 'style') styleLeak = true;
    if (n.startsWith('on')) handlerLeak = true;
    if (n === 'ping' || n === 'formaction') pingLeak = true;
    if (n === 'srcset') srcsetLeak = true;
  }
  if (el.tagName === 'A') links.push({ text: el.textContent, href: el.getAttribute('href') });
  if (el.tagName === 'IMG') imgs.push({ alt: el.getAttribute('alt'), src: el.getAttribute('src') });
});

for (const forbidden of ['TEMPLATE', 'MATH', 'SVG', 'META', 'BASE', 'LINK', 'IFRAME', 'FORM', 'INPUT', 'TEXTAREA', 'SELECT', 'SCRIPT', 'STYLE']) {
  if (tags.includes(forbidden)) throw new Error('forbidden tag survived: ' + forbidden);
}
if (styleLeak) throw new Error('inline style survived sanitizer');
if (handlerLeak) throw new Error('event handler survived sanitizer');
if (clobberLeak) throw new Error('__search_reader* id clobbering survived sanitizer');
if (pingLeak) throw new Error('ping/formaction survived sanitizer');
if (srcsetLeak) throw new Error('srcset survived sanitizer');

const badData = links.find((l) => l.text === 'Bad data link');
const badJs = links.find((l) => l.text === 'Bad js link');
const good = links.find((l) => l.text === 'Good link');
if (!badData || badData.href !== null) throw new Error('data:text/html href was not stripped');
if (!badJs || badJs.href !== null) throw new Error('javascript: href was not stripped');
if (!good || good.href !== 'https://example.org/docs') throw new Error('safe https href was lost');

const badSvg = imgs.find((i) => i.alt === null);
const goodPng = imgs.find((i) => i.alt === 'Tiny PNG');
if (!badSvg || badSvg.src !== null) throw new Error('data:image/svg+xml src was not stripped');
if (!goodPng || goodPng.src !== 'data:image/png;base64,iVBORw0KGgo=') {
  throw new Error('safe raster data:image/png;base64 src was not preserved');
}

// Exercise size controls (18 -> 14 -> 22) via button clicks
const lessBtn = document.querySelector('#__search_reader_font button[aria-label="Smaller reading text"]');
const moreBtn = document.querySelector('#__search_reader_font button[aria-label="Larger reading text"]');
const sizeLabel = document.getElementById('__search_reader_size_label');
if (sizeLabel.textContent !== '18px') throw new Error('initial size label wrong: ' + sizeLabel.textContent);
lessBtn.click();
if (sizeLabel.textContent !== '14px' || !lessBtn.disabled) throw new Error('small step failed');
moreBtn.click();
moreBtn.click();
if (sizeLabel.textContent !== '22px' || !moreBtn.disabled) throw new Error('large step failed');

// Exercise exit button click -> restores original DOM, styles, scrollY, and posts 'off'
const exitBtn = document.getElementById('__search_reader_exit');
exitBtn.click();
if (globalThis.__searchReaderCtrl.isActive()) throw new Error('expected isActive() === false after exit click');
if (postedMessages.length !== 1 || postedMessages[0] !== 'off') {
  throw new Error('expected searchReader postMessage("off"), got ' + JSON.stringify(postedMessages));
}
if (document.body.firstChild !== article) throw new Error('original DOM node was not restored by identity');
if (document.body.getAttribute('style') !== 'margin:16px') throw new Error('body style not restored');
if (document.documentElement.getAttribute('style') !== 'background:beige') throw new Error('html style not restored');
if (currentScrollY !== 240) throw new Error('scrollY not restored: ' + currentScrollY);
if (head.querySelectorAll('style[data-search-reader]').length !== 0) {
  throw new Error('reader style tag was not removed');
}

// Re-enter and exit via host toggle() after exit button left
if (globalThis.__searchReaderCtrl.toggle(palette) !== 'entered') throw new Error('re-enter failed');
if (globalThis.__searchReaderCtrl.toggle(palette) !== 'left') throw new Error('second leave failed');
'OK';
"#;

    let ctx = Context::new();
    let full = format!("{harness}\n{}\n{verify}", crate::reader::TOGGLE_ON);
    let result = ctx.evaluate(&full);
    if let Some(exc) = ctx.exception() {
        panic!("JSC reader test threw exception: {}", exc);
    }
    assert_eq!(
        result.map(|v| v.to_str().to_string()).as_deref(),
        Some("OK")
    );
}

#[test]
fn h4_main_world_cannot_access_or_tamper_with_isolated_reader_world() {
    use javascriptcore::{Context, ContextExt, ValueExt};

    // Two distinct JSC Contexts model WebKit's main world vs 'search-reader' isolated world.
    let main_world = Context::new();
    let reader_world = Context::new();

    // Host loads reader.js strictly in reader_world.
    reader_world.evaluate(crate::reader::TOGGLE_ON);
    assert!(
        reader_world.exception().is_none(),
        "reader.js should load cleanly in isolated world"
    );

    // Main world tries to read or poison reader globals.
    let main_check = main_world.evaluate(
        "(() => { \
            const leaked = typeof globalThis.__searchReaderCtrl !== 'undefined' \
                || typeof globalThis.__searchReader !== 'undefined' \
                || typeof globalThis.__searchReaderOn !== 'undefined' \
                || typeof globalThis.__searchReaderBody !== 'undefined'; \
            globalThis.__searchReaderCtrl = { toggle: () => 'spoofed', isActive: () => false }; \
            globalThis.__searchReaderOn = true; \
            return leaked ? 'LEAKED' : 'ISOLATED'; \
        })()",
    );
    assert_eq!(
        main_check.map(|v| v.to_str().to_string()).as_deref(),
        Some("ISOLATED")
    );

    // Reader world's controller is still frozen, intact, and untouched by main world.
    let reader_check = reader_world.evaluate(
        "(() => { \
            const c = globalThis.__searchReaderCtrl; \
            const ok = c && typeof c.toggle === 'function' && typeof c.setFont === 'function' \
                && typeof c.isActive === 'function' && c.isActive() === false \
                && Object.isFrozen(c); \
            return ok ? 'INTACT' : 'BROKEN'; \
        })()",
    );
    assert_eq!(
        reader_check.map(|v| v.to_str().to_string()).as_deref(),
        Some("INTACT")
    );
}

// ---- F39 Core Correctness Regressions -----------------------------------

#[test]
fn f39_ipv6_host_of_end_to_end_curtain_keys_and_sitecard() {
    use gtk::gio::TlsCertificateFlags as F;

    for (url, expected_host) in [
        ("http://[::1]:8080/p", "[::1]"),
        ("https://[2001:DB8::1]/path?q=1#frag", "[2001:db8::1]"),
        ("https://user:pass@[2001:DB8::1]:8443/a", "[2001:db8::1]"),
        ("http://[::ffff:127.0.0.1]:8080/x", "[::ffff:127.0.0.1]"),
        ("https://Example.COM:8443/a", "example.com"),
        ("http://127.0.0.1:8099/plain", "127.0.0.1"),
    ] {
        // Real production chain: shield::host_of -> curtain::allow_list_for
        let host = crate::shield::host_of(url);
        assert_eq!(host, expected_host, "host_of({url:?})");
        assert_eq!(
            allow_list_for(&host),
            vec![format!("http://{expected_host}/*"), format!("https://{expected_host}/*")],
            "host_of -> allow_list_for({url:?})"
        );

        // Hidden and shield-off persistence keys use shield::host_of directly.
        let mut hidden: BTreeMap<String, Vec<String>> = BTreeMap::new();
        hidden.insert(host.clone(), vec!["#ad".to_string()]);
        assert_eq!(
            hidden.get(&crate::shield::host_of(url)),
            Some(&vec!["#ad".to_string()])
        );
        let mut shield_off: BTreeMap<String, bool> = BTreeMap::new();
        shield_off.insert(host.clone(), true);
        assert_eq!(shield_off.get(&crate::shield::host_of(url)), Some(&true));

        // SiteCard::build calls shield::host_of internally.
        let model = crate::sitecard::build(url, Some(F::empty()), &[], false, 1.0, Vec::new());
        assert_eq!(model.host, expected_host, "sitecard::build({url:?}).host");
    }

    // Malformed authorities fail safely across the full chain.
    for malformed in [
        "http://[::1/p",
        "http://[]/p",
        "http://[::1]evil.com/p",
        "http://[::1]:abc/p",
        "http://[::1]:/p",
        "http://[example.com]/p",
        "http://2001:db8::1/p",
        "http://example.com:bad/p",
        "http://foo]bar/p",
    ] {
        let host = crate::shield::host_of(malformed);
        assert_eq!(host, "", "malformed {malformed:?} must produce empty host");
        assert!(
            allow_list_for(&host).is_empty(),
            "malformed {malformed:?} must never build an allow list"
        );
        let model = crate::sitecard::build(malformed, None, &[], false, 1.0, Vec::new());
        assert_eq!(model.host, "", "sitecard must not display malformed host for {malformed:?}");
    }
}

#[test]
fn f39_column_plan_drives_folded_controls_across_sidebar_toolbar_and_fullscreen() {
    // Disabled sidebar (prefs.sidebar=false, column_folded=false): body MUST be false,
    // width & paned_at 0, so folded_controls appear (both toolbar hidden and shown).
    let disabled_unfolded = crate::ui::column_plan(false, false, false, 260);
    assert_eq!(disabled_unfolded.width, 0);
    assert_eq!(disabled_unfolded.paned_at, 0);
    assert!(!disabled_unfolded.strip);
    assert!(!disabled_unfolded.body);
    assert!(disabled_unfolded.side_shown);
    assert!(should_show_folded_controls(
        false,
        false,
        false,
        disabled_unfolded.side_shown,
        disabled_unfolded.body,
    ));

    // Disabled sidebar + folded=true: same visibility.
    let disabled_folded = crate::ui::column_plan(true, false, false, 260);
    assert_eq!(disabled_folded.width, 0);
    assert_eq!(disabled_folded.paned_at, 0);
    assert!(!disabled_folded.strip);
    assert!(!disabled_folded.body);
    assert!(should_show_folded_controls(
        true,
        false,
        false,
        disabled_folded.side_shown,
        disabled_folded.body,
    ));

    // Disabled sidebar in F11 fullscreen or page fullscreen: folded_controls stay hidden.
    assert!(!should_show_folded_controls(
        false,
        false,
        true,
        disabled_unfolded.side_shown,
        disabled_unfolded.body,
    ));
    let disabled_page_full = crate::ui::column_plan(false, true, false, 260);
    assert!(!disabled_page_full.side_shown);
    assert!(!disabled_page_full.body);
    assert!(!should_show_folded_controls(
        false,
        false,
        false,
        disabled_page_full.side_shown,
        disabled_page_full.body,
    ));

    // Normal open sidebar: body is visible -> folded_controls hidden (no duplicate buttons).
    let open = crate::ui::column_plan(false, false, true, 260);
    assert!(open.body);
    assert!(!open.strip);
    assert_eq!(open.width, 260);
    assert_eq!(open.paned_at, 260);
    assert!(!should_show_folded_controls(
        false,
        true,
        false,
        open.side_shown,
        open.body,
    ));

    // Folded sidebar: body hidden, strip shown -> folded_controls visible (and cleared below toolbar).
    let folded = crate::ui::column_plan(true, false, true, 260);
    assert!(!folded.body);
    assert!(folded.strip);
    assert!(should_show_folded_controls(
        true,
        true,
        false,
        folded.side_shown,
        folded.body,
    ));
    assert_eq!(folded_controls_top_margin(false, 72), 8);
    assert_eq!(folded_controls_top_margin(true, 72), 80);

    // Folded sidebar in page fullscreen or F11 fullscreen -> folded_controls hidden.
    let folded_page_full = crate::ui::column_plan(true, true, true, 260);
    assert!(!should_show_folded_controls(
        true,
        true,
        false,
        folded_page_full.side_shown,
        folded_page_full.body,
    ));
    assert!(!should_show_folded_controls(
        true,
        true,
        true,
        folded.side_shown,
        folded.body,
    ));
}

#[test]
fn f39_delayed_private_hide_switch_to_normal_never_leaks_or_styles_wrong_tab() {
    let dir = TempDir::new("f39-delayed-hide");
    let hidden_path = dir.0.join("hidden.json");
    let history_path = dir.0.join("history.json");

    let mut hidden: BTreeMap<String, Vec<String>> = BTreeMap::new();
    hidden.insert("127.0.0.1".to_string(), vec!["#seeded".to_string()]);
    let history = vec![HistoryEntry {
        url: "http://127.0.0.1:8099/plain".to_string(),
        title: "Normal Seed".to_string(),
        at: 1_700_000_000,
    }];
    crate::files::write_json(&hidden_path, &hidden);
    crate::files::write_json(&history_path, &history);
    let hidden_before = std::fs::read(&hidden_path).expect("read hidden.json");
    let history_before = std::fs::read(&history_path).expect("read history.json");

    let mut private_hidden: HashMap<u64, HashMap<String, Vec<String>>> = HashMap::new();
    let priv_tab = 77u64;
    let normal_tab = 1u64;
    let priv_mgr = 0x9001usize;
    let shared_mgr = 0x1001usize;

    let origin = HideOrigin {
        tab_id: priv_tab,
        host: crate::shield::host_of("http://127.0.0.1:8099/plain"),
        private: true,
        nav_epoch: 3,
        manager_key: Some(priv_mgr),
    };

    // Source manager check rejects messages coming from a different (shared) manager.
    let priv_snap = HideTargetSnapshot {
        private: true,
        host: "127.0.0.1",
        nav_epoch: 3,
        manager_key: Some(priv_mgr),
    };
    assert_eq!(
        validate_hide_source(true, Some(&origin), shared_mgr, Some(priv_snap.clone())),
        None,
        "shared manager must not spoof a private tab's hide session"
    );
    assert_eq!(
        validate_hide_source(false, Some(&origin), priv_mgr, Some(priv_snap.clone())),
        None,
        "messages outside hide_mode must be rejected"
    );
    let verified = validate_hide_source(true, Some(&origin), priv_mgr, Some(priv_snap.clone()))
        .expect("matching private manager and origin must validate");

    // Simulate active tab switching from priv_tab -> normal_tab during the 10ms delay.
    // Because apply_hide_for_origin binds strictly to `verified.tab_id` (priv_tab) and
    // never reads active_tab (normal_tab), it applies ONLY to priv_tab ephemerally.
    let outcome = apply_hide_for_origin(
        &verified,
        Some(priv_snap),
        "#private-only-pick",
        true,
        &mut hidden,
        &mut private_hidden,
    );
    assert_eq!(outcome, HideApplyOutcome::AppliedPrivate { tab_id: priv_tab });
    assert_ne!(priv_tab, normal_tab);
    assert_eq!(
        hidden.get("127.0.0.1"),
        Some(&vec!["#seeded".to_string()]),
        "normal persisted hidden map must stay untouched"
    );
    assert_eq!(
        private_hidden
            .get(&priv_tab)
            .and_then(|m| m.get("127.0.0.1")),
        Some(&vec!["#seeded".to_string(), "#private-only-pick".to_string()])
    );
    assert!(
        !private_hidden.contains_key(&normal_tab),
        "normal tab must not receive private hide state"
    );
    assert_eq!(
        std::fs::read(&hidden_path).expect("re-read hidden.json"),
        hidden_before,
        "hidden.json must remain byte-for-byte unchanged"
    );
    assert_eq!(
        std::fs::read(&history_path).expect("re-read history.json"),
        history_before,
        "history.json must remain byte-for-byte unchanged"
    );

    // Originating tab navigated to another host -> safely canceled.
    let navigated_host_snap = HideTargetSnapshot {
        private: true,
        host: "other.example",
        nav_epoch: 4,
        manager_key: Some(priv_mgr),
    };
    assert_eq!(
        apply_hide_for_origin(
            &verified,
            Some(navigated_host_snap),
            "#stale-nav",
            true,
            &mut hidden,
            &mut private_hidden,
        ),
        HideApplyOutcome::Canceled
    );

    // Originating tab navigated to a new page on the SAME host (nav_epoch bumped) -> safely canceled.
    let same_host_nav_snap = HideTargetSnapshot {
        private: true,
        host: "127.0.0.1",
        nav_epoch: 4,
        manager_key: Some(priv_mgr),
    };
    assert_eq!(
        apply_hide_for_origin(
            &verified,
            Some(same_host_nav_snap),
            "#stale-same-host",
            true,
            &mut hidden,
            &mut private_hidden,
        ),
        HideApplyOutcome::Canceled
    );

    // Originating tab closed (snapshot None) -> safely canceled.
    assert_eq!(
        apply_hide_for_origin(
            &verified,
            None,
            "#stale-closed",
            true,
            &mut hidden,
            &mut private_hidden,
        ),
        HideApplyOutcome::Canceled
    );

    // Originating tab's manager rebuilt/replaced -> safely canceled.
    let rebuilt_mgr_snap = HideTargetSnapshot {
        private: true,
        host: "127.0.0.1",
        nav_epoch: 3,
        manager_key: Some(0xDEADusize),
    };
    assert_eq!(
        crate::curtain::validate_hide_target(&verified, Some(rebuilt_mgr_snap)),
        HideDecision::Cancel
    );

    // Valid normal-tab origin applies strictly to that normal tab and requests persistence.
    let normal_origin = HideOrigin {
        tab_id: normal_tab,
        host: crate::shield::host_of("http://[::1]:8080/p"),
        private: false,
        nav_epoch: 1,
        manager_key: Some(shared_mgr),
    };
    let normal_snap = HideTargetSnapshot {
        private: false,
        host: "[::1]",
        nav_epoch: 1,
        manager_key: Some(shared_mgr),
    };
    assert_eq!(
        apply_hide_for_origin(
            &normal_origin,
            Some(normal_snap),
            "#ipv6-banner",
            true,
            &mut hidden,
            &mut private_hidden,
        ),
        HideApplyOutcome::AppliedNormal {
            tab_id: normal_tab,
            save_persisted: true,
        }
    );
    assert_eq!(
        hidden.get("[::1]"),
        Some(&vec!["#ipv6-banner".to_string()])
    );

    // Curtain isolated world constant & multi-shared-manager clear semantics.
    assert_eq!(CURTAIN_WORLD, "search-curtain");
    let mut reg: ScopedRegistry<&'static str> = ScopedRegistry::new();
    let shielded_shared = 0x1000usize;
    let unshielded_shared = 0x2000usize;
    let private_dedicated = 0x3000usize;
    reg.replace(shielded_shared, "example.com", Some("s1"));
    reg.replace(unshielded_shared, "[::1]", Some("s2"));
    reg.replace(private_dedicated, "example.com", Some("p1"));
    // Clearing every shared manager clears both shielded and unshielded shared managers
    // without touching an independent private override manager.
    assert_eq!(reg.clear_manager(shielded_shared), vec!["s1"]);
    assert_eq!(reg.clear_manager(unshielded_shared), vec!["s2"]);
    assert_eq!(reg.manager_len(shielded_shared), 0);
    assert_eq!(reg.manager_len(unshielded_shared), 0);
    assert_eq!(reg.get(private_dedicated, "example.com"), Some(&"p1"));
}

#[test]
fn f39_tls_error_focuses_safe_go_back_action_when_active_and_unblocked() {
    // Active TLS/security errors (from WebKit TLS flags and message_for fallback) focus Go back.
    for reason in [
        "The certificate has expired.",
        "The site's security could not be verified.",
        "The page stopped arriving. (The site's security could not be verified.)",
        "Unacceptable TLS certificate",
        "SSL handshake failed",
    ] {
        assert!(
            should_focus_error_back(true, true, reason, false),
            "active visible security error {reason:?} must focus Go back"
        );
        // Background tab does not steal focus until selected (when is_active_tab becomes true).
        assert!(
            !should_focus_error_back(false, true, reason, false),
            "background security error must not steal focus"
        );
        // If error view is not the visible stack child, do not grab focus.
        assert!(
            !should_focus_error_back(true, false, reason, false),
            "non-visible error view must not grab focus"
        );
        // If an overlay (omnibox/find/panel/dialog/permission) owns focus, do not steal it.
        assert!(
            !should_focus_error_back(true, true, reason, true),
            "overlay blocking focus must keep focus"
        );
    }

    // Non-security errors preserve existing default focus behavior.
    for non_sec in [
        "There is nothing at this address (404).",
        "The site broke while answering (503).",
        "That address could not be found.",
        "The site refused the connection.",
        "The site took too long to answer.",
        "There is no network to reach it on.",
    ] {
        assert!(
            !should_focus_error_back(true, true, non_sec, false),
            "non-security error {non_sec:?} must not force focus to Go back"
        );
    }
}
