// Honest Firefox Add-ons (addons.mozilla.org) discovery & bounded user-initiated
// search/install helper.
//
// Never auto-installs, never makes unsolicited network calls on panel open,
// and candidly distinguishes pure content-script add-ons from extensions that
// depend on WebExtensions background/storage/nativeMessaging APIs.

use soup::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::time::Duration;

const SEARCH_TIMEOUT: Duration = Duration::from_secs(8);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_SEARCH_BYTES: usize = 512 * 1024;
const MAX_XPI_BYTES: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CompatTier {
    /// Catalog metadata does not establish runtime compatibility.
    Unverified,
    /// Injects content scripts, but toolbar popups, background workers,
    /// storage, or WebExtensions APIs will not run here.
    Partial,
    /// Relies on WebExtensions background/blocking/native/storage APIs that
    /// WebKitGTK does not provide.
    Unsupported,
}

impl CompatTier {
    pub fn badge(self) -> &'static str {
        match self {
            CompatTier::Unverified => "Not verified",
            CompatTier::Partial => "API-dependent",
            CompatTier::Unsupported => "Requires WebExtensions APIs",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    pub name: String,
    pub slug: String,
    pub summary: String,
    pub listing_url: String,
    pub xpi_url: Option<String>,
    pub compat: CompatTier,
    pub compat_note: String,
}

/// Curated real official Firefox Add-ons listings (addons.mozilla.org) with
/// honest compatibility disclosures. No network request is made until the user
/// clicks Open, Search, or Install.
pub fn curated_entries() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            name: "Stylus".into(),
            slug: "styl-us".into(),
            summary: "Custom userstyles for websites.".into(),
            listing_url: "https://addons.mozilla.org/en-US/firefox/addon/styl-us/".into(),
            xpi_url: None,
            compat: CompatTier::Partial,
            compat_note: "May include content scripts; its editor and extension storage are unavailable here.".into(),
        },
        CatalogEntry {
            name: "Dark Reader".into(),
            slug: "darkreader".into(),
            summary: "Dark theme for websites. (Tip: Rill also has a built-in page darkener in Settings.)".into(),
            listing_url: "https://addons.mozilla.org/en-US/firefox/addon/darkreader/".into(),
            xpi_url: None,
            compat: CompatTier::Partial,
            compat_note: "Requires extension APIs for settings and background coordination; full functionality is unavailable.".into(),
        },
        CatalogEntry {
            name: "SponsorBlock for YouTube".into(),
            slug: "sponsorblock".into(),
            summary: "Skip sponsored segments on YouTube.".into(),
            listing_url: "https://addons.mozilla.org/en-US/firefox/addon/sponsorblock/".into(),
            xpi_url: None,
            compat: CompatTier::Partial,
            compat_note: "Requires extension storage and background APIs; sponsor skipping is not verified here.".into(),
        },
        CatalogEntry {
            name: "Return YouTube Dislike".into(),
            slug: "return-youtube-dislikes".into(),
            summary: "Displays dislike counts on YouTube pages.".into(),
            listing_url: "https://addons.mozilla.org/en-US/firefox/addon/return-youtube-dislikes/".into(),
            xpi_url: None,
            compat: CompatTier::Partial,
            compat_note: "Requires background messaging APIs; dislike counts are not verified here.".into(),
        },
        CatalogEntry {
            name: "uBlock Origin".into(),
            slug: "ublock-origin".into(),
            summary: "Network ad/tracker blocker. (Rill already includes a native engine blocker in Settings.)".into(),
            listing_url: "https://addons.mozilla.org/en-US/firefox/addon/ublock-origin/".into(),
            xpi_url: None,
            compat: CompatTier::Unsupported,
            compat_note: "Requires webRequestBlocking and background pages; use Rill's built-in Blocker instead.".into(),
        },
    ]
}

pub fn filter_curated(query: &str) -> Vec<CatalogEntry> {
    let q = query.trim().to_ascii_lowercase();
    if q.is_empty() {
        return curated_entries();
    }
    curated_entries()
        .into_iter()
        .filter(|e| {
            e.name.to_ascii_lowercase().contains(&q)
                || e.summary.to_ascii_lowercase().contains(&q)
                || e.slug.to_ascii_lowercase().contains(&q)
        })
        .collect()
}

pub fn is_allowed_amo_url(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    let Some(rest) = lower.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    matches!(host, "addons.mozilla.org" | "addons.cdn.mozilla.net")
}

/// Parse an official AMO v5 `/api/v5/addons/search/` JSON payload.
pub fn parse_amo_search_response(bytes: &[u8]) -> Result<Vec<CatalogEntry>, String> {
    let root: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format!("Invalid AMO response: {e}"))?;
    let results = root
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "AMO response missing results list".to_string())?;
    let mut entries = Vec::new();
    for item in results.iter().take(8) {
        let slug = item
            .get("slug")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim()
            .to_string();
        if slug.is_empty() {
            continue;
        }
        let name = localized_text(item.get("name")).unwrap_or_else(|| slug.clone());
        let summary = localized_text(item.get("summary")).unwrap_or_default();
        let listing_url = item
            .get("url")
            .and_then(|v| v.as_str())
            .filter(|u| is_allowed_amo_url(u))
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("https://addons.mozilla.org/en-US/firefox/addon/{slug}/"));
        let file = item.get("current_version").and_then(|v| v.get("file"));
        let xpi_url = file
            .and_then(|f| f.get("url"))
            .and_then(|v| v.as_str())
            .filter(|u| is_allowed_amo_url(u))
            .map(ToString::to_string);
        let permissions: Vec<String> = file
            .and_then(|f| f.get("permissions"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let (compat, compat_note) = classify_permissions(&permissions);
        entries.push(CatalogEntry {
            name,
            slug,
            summary,
            listing_url,
            xpi_url,
            compat,
            compat_note,
        });
    }
    Ok(entries)
}

fn localized_text(value: Option<&serde_json::Value>) -> Option<String> {
    let val = value?;
    if let Some(s) = val.as_str() {
        return (!s.trim().is_empty()).then(|| s.trim().to_string());
    }
    let map = val.as_object()?;
    for key in ["en-US", "en-GB", "en"] {
        if let Some(s) = map.get(key).and_then(|v| v.as_str()) {
            if !s.trim().is_empty() {
                return Some(s.trim().to_string());
            }
        }
    }
    map.values()
        .find_map(|v| v.as_str().map(str::trim).filter(|s| !s.is_empty()))
        .map(ToString::to_string)
}

pub fn classify_permissions(permissions: &[String]) -> (CompatTier, String) {
    let has_blocking = permissions.iter().any(|p| {
        matches!(
            p.as_str(),
            "webRequest"
                | "webRequestBlocking"
                | "declarativeNetRequest"
                | "nativeMessaging"
                | "proxy"
                | "privacy"
        )
    });
    let has_api = permissions.iter().any(|p| {
        matches!(
            p.as_str(),
            "storage" | "tabs" | "cookies" | "bookmarks" | "history" | "contextMenus" | "alarms"
        )
    });
    let has_hosts = permissions
        .iter()
        .any(|p| p.contains("://") || p == "<all_urls>");
    if has_blocking && !has_hosts {
        (
            CompatTier::Unsupported,
            "Requires WebExtensions background/blocking APIs not supported in WebKitGTK.".into(),
        )
    } else if has_blocking || has_api {
        (
            CompatTier::Partial,
            "Any content scripts may load on matching sites; background/storage/WebExtensions APIs will not run.".into(),
        )
    } else {
        (
            CompatTier::Unverified,
            "Permissions alone cannot establish compatibility. The package manifest is checked before installation.".into(),
        )
    }
}

type FetchDone = Box<dyn FnOnce(Result<Vec<u8>, String>)>;

fn redirect_target(base: &str, location: &str) -> Result<String, String> {
    let target = glib::Uri::resolve_relative(Some(base), location, glib::UriFlags::NONE)
        .map_err(|_| "Invalid add-on redirect".to_string())?;
    if !is_allowed_amo_url(&target) {
        return Err("Refused redirect outside official AMO HTTPS hosts".into());
    }
    Ok(target.to_string())
}

fn read_limited(
    stream: gtk::gio::InputStream,
    cancel: gtk::gio::Cancellable,
    limit: usize,
    mut body: Vec<u8>,
    done: FetchDone,
) {
    let next = stream.clone();
    let next_cancel = cancel.clone();
    let count = limit.saturating_sub(body.len()).saturating_add(1).min(16 * 1024);
    stream.read_bytes_async(count, glib::Priority::DEFAULT, Some(&cancel), move |result| {
        match result {
            Ok(bytes) if bytes.is_empty() => {
                next.close_async(glib::Priority::DEFAULT, None::<&gtk::gio::Cancellable>, |_| {});
                done(Ok(body));
            }
            Ok(bytes) if bytes.len() > limit.saturating_sub(body.len()) => {
                next.close_async(glib::Priority::DEFAULT, None::<&gtk::gio::Cancellable>, |_| {});
                done(Err("AMO response exceeded size limit".into()));
            }
            Ok(bytes) => {
                body.extend_from_slice(bytes.as_ref());
                read_limited(next, next_cancel, limit, body, done);
            }
            Err(error) => {
                next.close_async(glib::Priority::DEFAULT, None::<&gtk::gio::Cancellable>, |_| {});
                done(Err(format!("AMO transfer failed: {error}")));
            }
        }
    });
}

fn fetch_hop(
    url: &str,
    limit: usize,
    cancel: &gtk::gio::Cancellable,
    redirects_left: u8,
    done: FetchDone,
) {
    if !is_allowed_amo_url(url) {
        done(Err("Refused non-AMO HTTPS URL".into()));
        return;
    }
    let Ok(message) = soup::Message::new("GET", url) else {
        done(Err("Could not prepare AMO request".into()));
        return;
    };
    message.add_flags(soup::MessageFlags::NO_REDIRECT);
    let response = message.clone();
    let base = url.to_string();
    let next_cancel = cancel.clone();
    AMO_SESSION.with(|session| {
        session.send_async(&message, glib::Priority::DEFAULT, Some(cancel), move |result| {
            let stream = match result {
                Ok(stream) => stream,
                Err(error) => {
                    done(Err(format!("Could not reach addons.mozilla.org: {error}")));
                    return;
                }
            };
            let status = response.status_code();
            if matches!(status, 301 | 302 | 303 | 307 | 308) {
                stream.close_async(glib::Priority::DEFAULT, None::<&gtk::gio::Cancellable>, |_| {});
                if redirects_left == 0 {
                    done(Err("Too many AMO redirects".into()));
                    return;
                }
                let target = response.response_headers()
                    .and_then(|headers| headers.one("Location"))
                    .ok_or_else(|| "AMO redirect omitted its destination".to_string())
                    .and_then(|location| redirect_target(&base, &location));
                match target {
                    Ok(target) => fetch_hop(&target, limit, &next_cancel, redirects_left - 1, done),
                    Err(error) => done(Err(error)),
                }
                return;
            }
            if status != 200 {
                stream.close_async(glib::Priority::DEFAULT, None::<&gtk::gio::Cancellable>, |_| {});
                done(Err(format!("AMO returned HTTP {status}")));
                return;
            }
            let content_length = response.response_headers()
                .map(|headers| headers.content_length())
                .unwrap_or(-1);
            if content_length > limit as i64 {
                stream.close_async(glib::Priority::DEFAULT, None::<&gtk::gio::Cancellable>, |_| {});
                done(Err("AMO response exceeded size limit".into()));
                return;
            }
            read_limited(stream, next_cancel, limit, Vec::new(), done);
        });
    });
}

fn fetch_amo(
    url: &str,
    limit: usize,
    timeout: Duration,
    cancel: &gtk::gio::Cancellable,
    done: FetchDone,
) {
    let timer = std::rc::Rc::new(RefCell::new(None::<glib::SourceId>));
    let timer_done = timer.clone();
    let timeout_cancel = cancel.clone();
    let source = glib::timeout_add_local_once(timeout, move || {
        timer_done.borrow_mut().take();
        timeout_cancel.cancel();
    });
    timer.borrow_mut().replace(source);
    fetch_hop(url, limit, cancel, 4, Box::new(move |result| {
        if let Some(source) = timer.borrow_mut().take() {
            source.remove();
        }
        done(result);
    }));
}

thread_local! {
    static AMO_SESSION: soup::Session = {
        let s = soup::Session::new();
        s.set_user_agent("Rill-Browser-Linux");
        s
    };
    static ACTIVE_SEARCH: RefCell<Option<gtk::gio::Cancellable>> = const { RefCell::new(None) };
    static ACTIVE_DOWNLOAD: RefCell<Option<gtk::gio::Cancellable>> = const { RefCell::new(None) };
}

pub fn cancel_pending() {
    cancel_search();
    ACTIVE_DOWNLOAD.with(|c| {
        let cancel = c.borrow_mut().take();
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
    });
}

pub fn cancel_search() {
    ACTIVE_SEARCH.with(|c| {
        let cancel = c.borrow_mut().take();
        if let Some(cancel) = cancel {
            cancel.cancel();
        }
    });
}

/// User-initiated bounded HTTPS search against the official Firefox Add-ons API.
pub fn search_amo(query: &str, done: impl FnOnce(Result<Vec<CatalogEntry>, String>) + 'static) {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        cancel_search();
        done(Ok(curated_entries()));
        return;
    }
    let url = format!(
        "https://addons.mozilla.org/api/v5/addons/search/?q={}&type=extension&page_size=8&sort=users",
        crate::store::urlencode(trimmed)
    );
    let cancel = gtk::gio::Cancellable::new();
    ACTIVE_SEARCH.with(|slot| {
        let prev = slot.borrow_mut().replace(cancel.clone());
        if let Some(prev) = prev {
            prev.cancel();
        }
    });
    let completion_cancel = cancel.clone();
    fetch_amo(&url, MAX_SEARCH_BYTES, SEARCH_TIMEOUT, &cancel, Box::new(move |result| {
        let current = ACTIVE_SEARCH.with(|slot| slot.borrow().as_ref() == Some(&completion_cancel));
        if current {
            ACTIVE_SEARCH.with(|slot| { slot.borrow_mut().take(); });
            done(result.and_then(|bytes| parse_amo_search_response(&bytes)));
        }
    }));
}

/// User-initiated bounded download of an official AMO `.xpi` archive into the
/// browser cache staging directory (never `/tmp`), ready for `extensions::inspect`
/// and `permissions::offer_addon`.
pub fn download_amo_xpi(
    slug: &str,
    xpi_url: &str,
    done: impl FnOnce(Result<PathBuf, String>) + 'static,
) {
    if !is_allowed_amo_url(xpi_url) {
        done(Err("Refused non-AMO download URL".into()));
        return;
    }
    let cancel = gtk::gio::Cancellable::new();
    ACTIVE_DOWNLOAD.with(|slot| {
        let prev = slot.borrow_mut().replace(cancel.clone());
        if let Some(prev) = prev {
            prev.cancel();
        }
    });
    let safe_slug: String = slug
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(64)
        .collect();
    let safe_slug = if safe_slug.is_empty() { "addon".into() } else { safe_slug };
    let completion_cancel = cancel.clone();
    fetch_amo(xpi_url, MAX_XPI_BYTES, DOWNLOAD_TIMEOUT, &cancel, Box::new(move |result| {
        let current = ACTIVE_DOWNLOAD.with(|slot| slot.borrow().as_ref() == Some(&completion_cancel));
        if !current {
            return;
        }
        ACTIVE_DOWNLOAD.with(|slot| { slot.borrow_mut().take(); });
        done(result.and_then(|bytes| {
            if bytes.is_empty() {
                return Err("Add-on package was empty".into());
            }
            let dir = crate::files::data_dir().join("addon-staging");
            std::fs::create_dir_all(&dir)
                .map_err(|error| format!("Could not create staging directory: {error}"))?;
            let path = dir.join(format!("{safe_slug}.xpi"));
            std::fs::write(&path, &bytes)
                .map(|()| path)
                .map_err(|error| format!("Could not save staged package: {error}"))
        }));
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amo_url_guard_allows_only_official_https_hosts() {
        assert!(is_allowed_amo_url("https://addons.mozilla.org/en-US/firefox/addon/stylus/"));
        assert!(is_allowed_amo_url("https://addons.cdn.mozilla.net/user-media/addons/1/a.xpi"));
        assert!(!is_allowed_amo_url("http://addons.mozilla.org/a.xpi"));
        assert!(!is_allowed_amo_url("https://evil.example/addons.mozilla.org/a.xpi"));
        assert!(!is_allowed_amo_url("file:///tmp/a.xpi"));
        assert!(!is_allowed_amo_url("https://addons.mozilla.org@evil.example/a.xpi"));
        assert!(!is_allowed_amo_url("https://addons.mozilla.org.evil.example/a.xpi"));
    }

    #[test]
    fn amo_redirects_remain_on_official_https_hosts() {
        let base = "https://addons.mozilla.org/firefox/downloads/file/1/a.xpi";
        assert_eq!(redirect_target(base, "/user-media/a.xpi").unwrap(),
            "https://addons.mozilla.org/user-media/a.xpi");
        assert!(redirect_target(base, "https://addons.cdn.mozilla.net/a.xpi").is_ok());
        assert!(redirect_target(base, "https://evil.example/a.xpi").is_err());
        assert!(redirect_target(base, "//evil.example/a.xpi").is_err());
        assert!(redirect_target(base, "http://addons.mozilla.org/a.xpi").is_err());
    }

    #[test]
    fn amo_stream_limit_accepts_exact_bound_and_rejects_oversized_bodies() {
        let context = glib::MainContext::new();
        context.with_thread_default(|| {
            for length in [16_383, 16_384, 16_385, 32_768] {
                let data = glib::Bytes::from_owned(vec![7u8; length]);
                let stream = gtk::gio::MemoryInputStream::from_bytes(&data);
                let result = std::rc::Rc::new(RefCell::new(None));
                let result_done = result.clone();
                read_limited(stream.upcast(), gtk::gio::Cancellable::new(), 16_384,
                    Vec::new(), Box::new(move |outcome| {
                        result_done.borrow_mut().replace(outcome);
                    }));
                while result.borrow().is_none() {
                    context.iteration(true);
                }
                let outcome = result.borrow_mut().take().unwrap();
                if length <= 16_384 {
                    assert_eq!(outcome.unwrap().len(), length);
                } else {
                    assert!(outcome.unwrap_err().contains("size limit"));
                }
            }
        }).unwrap();
    }

    #[test]
    fn parses_real_amo_v5_payload_and_classifies_honestly() {
        let sample = br#"{
          "results": [
            {
              "slug": "example-css",
              "name": {"en-US": "Example CSS"},
              "summary": {"en-US": "Restyles pages."},
              "url": "https://addons.mozilla.org/en-US/firefox/addon/example-css/",
              "current_version": {
                "file": {
                  "url": "https://addons.mozilla.org/firefox/downloads/file/1/example.xpi",
                  "permissions": ["https://example.com/*"]
                }
              }
            },
            {
              "slug": "example-blocker",
              "name": {"en-US": "Example Blocker"},
              "summary": {"en-US": "Blocks network requests."},
              "url": "https://addons.mozilla.org/en-US/firefox/addon/example-blocker/",
              "current_version": {
                "file": {
                  "url": "https://addons.mozilla.org/firefox/downloads/file/2/blocker.xpi",
                  "permissions": ["webRequest", "webRequestBlocking"]
                }
              }
            }
          ]
        }"#;
        let parsed = parse_amo_search_response(sample).expect("parses");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].compat, CompatTier::Unverified);
        assert_eq!(parsed[1].compat, CompatTier::Unsupported);
        assert!(parsed[0].xpi_url.is_some());
    }

    #[test]
    fn curated_entries_use_only_official_amo_urls_and_never_claim_password_managers_work() {
        let list = curated_entries();
        assert!(!list.is_empty());
        for entry in &list {
            assert!(is_allowed_amo_url(&entry.listing_url), "{}", entry.listing_url);
            assert!(!entry.name.to_ascii_lowercase().contains("bitwarden"));
            assert!(!entry.summary.contains("Search "), "{}", entry.summary);
            assert!(!entry.compat_note.contains("Search's"), "{}", entry.compat_note);
        }
    }

    #[test]
    #[ignore = "contacts addons.mozilla.org and downloads a real package without installing it"]
    fn live_amo_search_and_bounded_package_download() {
        let context = glib::MainContext::new();
        context.with_thread_default(|| {
            let result = std::rc::Rc::new(RefCell::new(None));
            let result_done = result.clone();
            search_amo("video-background-play-fix", move |outcome| {
                result_done.borrow_mut().replace(outcome);
            });
            while result.borrow().is_none() {
                context.iteration(true);
            }
            let entries = result.borrow_mut().take().unwrap().expect("live AMO search");
            let entry = entries.iter().find(|entry| entry.slug == "video-background-play-fix")
                .expect("official package listing");
            let url = entry.xpi_url.as_ref().expect("official download URL");
            let download = std::rc::Rc::new(RefCell::new(None));
            let download_done = download.clone();
            fetch_amo(url, MAX_XPI_BYTES, DOWNLOAD_TIMEOUT, &gtk::gio::Cancellable::new(),
                Box::new(move |outcome| {
                    download_done.borrow_mut().replace(outcome);
                }));
            while download.borrow().is_none() {
                context.iteration(true);
            }
            let bytes = download.borrow_mut().take().unwrap().expect("bounded official download");
            assert!(bytes.starts_with(b"PK"));
            assert!(bytes.len() <= MAX_XPI_BYTES);
            println!("Official AMO listing {}: fetched {} package bytes without installing", entry.slug, bytes.len());
        }).unwrap();
    }
}
