// The ad and tracker blocker: a WebKit content blocker, compiled once at
// launch and enforced inside WebKit's own networking, before a request is
// made. There is no script of ours in the page and nothing to pay per frame.
//
// The rules live in assets/shield-rules.json and are copied to the user's data
// folder on first run, so anyone can read or edit what is blocked.
use crate::files;
use glib::translate::ToGlibPtr;
use std::collections::BTreeMap;
use std::ffi::{c_void, CString};
use webkit2gtk::gio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use webkit2gtk::ffi;

const EMBEDDED: &str = include_str!("../assets/shield-rules.json");

fn rules_path() -> std::path::PathBuf {
    files::path_in(&files::data_dir(), "shield-rules-1.json")
}

/// The rules as JSON, written out once so they can be audited and edited.
pub fn rules_json() -> String {
    let path = rules_path();
    if !path.exists() {
        let _ = std::fs::write(&path, EMBEDDED);
        EMBEDDED.to_string()
    } else {
        std::fs::read_to_string(&path).unwrap_or_else(|_| EMBEDDED.to_string())
    }
}

struct Slot {
    filter: Option<*mut ffi::WebKitUserContentFilter>,
    done: bool,
}

unsafe extern "C" fn saved(
    source: *mut glib::gobject_ffi::GObject,
    res: *mut gio::ffi::GAsyncResult,
    data: *mut c_void,
) {
    let slot = &*(data as *const Mutex<Slot>);
    let mut err: *mut glib::ffi::GError = std::ptr::null_mut();
    let filter = ffi::webkit_user_content_filter_store_save_finish(
        source as *mut ffi::WebKitUserContentFilterStore,
        res,
        &mut err,
    );
    let mut s = slot.lock().unwrap();
    if !filter.is_null() {
        s.filter = Some(filter);
    } else if !err.is_null() {
        unsafe {
            let msg = std::ffi::CStr::from_ptr((*err).message).to_string_lossy().to_string();
            eprintln!("search: shield rules did not compile: {msg}");
        }
    }
    s.done = true;
}

/// Compile the rule list and hand it to a content manager. Blocks for a few
/// milliseconds at launch, then costs nothing.
pub fn install(manager: &webkit2gtk::UserContentManager) -> bool {
    let json = rules_json();
    if json.trim().is_empty() || !json.trim_start().starts_with('[') {
        eprintln!("search: shield rules are not a JSON array; leaving it off");
        return false;
    }
    let store_dir = files::data_dir().join("shield-store");
    let _ = std::fs::create_dir_all(&store_dir);
    let Ok(store_path) = CString::new(store_dir.to_string_lossy().to_string()) else { return false };
    let Ok(ident) = CString::new("search-shield") else { return false };

    let bytes = glib::Bytes::from(json.as_bytes());
    let slot = Arc::new(Mutex::new(Slot { filter: None, done: false }));
    let raw = Arc::into_raw(slot.clone()) as *mut c_void;

    unsafe {
        let store = ffi::webkit_user_content_filter_store_new(store_path.as_ptr());
        if store.is_null() {
            // The slot is leaked on purpose: a callback may still hold it.
            return false;
        }
        ffi::webkit_user_content_filter_store_save(
            store,
            ident.as_ptr(),
            bytes.to_glib_none().0,
            std::ptr::null_mut(),
            Some(saved),
            raw,
        );
        glib::gobject_ffi::g_object_unref(store as *mut glib::gobject_ffi::GObject);
    }

    let deadline = Instant::now() + Duration::from_secs(5);
    while !slot.lock().unwrap().done && Instant::now() < deadline {
        glib::MainContext::default().iteration(false);
        std::thread::sleep(Duration::from_millis(1));
    }

    let filter = slot.lock().unwrap().filter;
    let mut ok = false;
    unsafe {
        if let Some(filter) = filter {
            ffi::webkit_user_content_manager_add_filter(manager.to_glib_none().0, filter);
            glib::gobject_ffi::g_object_unref(filter as *mut glib::gobject_ffi::GObject);
            ok = true;
        }
        // The slot is not freed: the async compile may still be holding the
        // pointer on a slow machine, and a use-after-free in an ad blocker is
        // not a trade worth making for a few bytes at launch.
    }
    ok
}

/// Sites where the shield is off, kept beside the hidden elements.
pub fn allow_path() -> std::path::PathBuf {
    files::path_in(&files::data_dir(), "shield-off.json")
}

/// Whether the blocker is off for this host.
pub fn allowed(host: &str) -> bool {
    allow_list().get(host).copied().unwrap_or(false)
}

pub fn allow_list() -> BTreeMap<String, bool> {
    files::read_json(&allow_path()).unwrap_or_default()
}

/// Turn the blocker off for one host, and on again. The curtain menu uses
/// this; the allowlist it writes is read at load time.
pub fn set_allowed(host: &str, off: bool) {
    let mut list = allow_list();
    if off {
        list.insert(host.to_string(), true);
    } else {
        list.remove(host);
    }
    files::write_json(&allow_path(), &list);
}

pub fn is_allowed(host: &str) -> bool {
    allow_list().contains_key(host)
}

/// A tiny host helper: "news.ycombinator.com" from any address.
pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    let host = host.split('@').last().unwrap_or(host);
    host.split(':').next().unwrap_or(host).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rule_list_is_a_json_array() {
        let json = rules_json();
        let rules: serde_json::Value = serde_json::from_str(&json).expect("rules must parse");
        assert!(rules.as_array().expect("rules are an array").len() > 40);
    }

    #[test]
    fn a_host_is_a_host() {
        assert_eq!(host_of("https://news.ycombinator.com/item?id=1"), "news.ycombinator.com");
        assert_eq!(host_of("http://127.0.0.1:8099/plain?x=1"), "127.0.0.1");
        assert_eq!(host_of("https://user:pw@example.com:443/x"), "example.com");
        assert_eq!(host_of("example.com/path"), "example.com");
    }
}
