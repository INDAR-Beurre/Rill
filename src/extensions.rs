// Extensions: what a Chrome, Firefox or unpacked extension asks for, done with
// what this engine can actually do.
//
// WebKitGTK has no WebExtensions API. It does have the two primitives every
// content extension is built out of: a script injected into a page at a chosen
// moment, limited to the hosts the extension asked for, and a style sheet
// applied the same way. So this is a host for that kind of extension rather
// than a lie about the other kind: manifest, content scripts, CSS, host
// permissions and an options page. An extension that needs the background
// service worker of Chrome's MV3, or `chrome.tabs`, does not run here, and
// `tools/extensions.py` says so when it loads one instead of pretending.
use crate::files;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Bumped whenever the set of installed extensions changes, so the browser
/// knows a page's scripts are out of date and a view has to be built again.
static GENERATION: AtomicUsize = AtomicUsize::new(1);

pub fn generation() -> usize {
    GENERATION.load(Ordering::Relaxed)
}

fn changed() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

/// An archive is read whole into memory before it is unpacked, because that is
/// what `zip` needs to walk a central directory, so these are the bounds that
/// keep a hostile download from being a memory exhaustion: a quarter of a
/// gigabyte for the archive itself, sixteen megabytes for any one file, a
/// hundred and twenty-eight for everything it holds, and four thousand entries.
/// A real extension is a few hundred kilobytes; the largest seen in the wild,
/// grammar and ad blocking ones, are a couple of megabytes.
const MAX_ARCHIVE: u64 = 256 * 1024 * 1024;
const MAX_ENTRY: u64 = 16 * 1024 * 1024;
const MAX_TOTAL: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Manifest {
    #[serde(default)]
    pub manifest_version: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// Chrome: `content_scripts`. Firefox: `content_scripts` too.
    #[serde(default)]
    pub content_scripts: Vec<ContentScript>,
    /// Firefox only: style sheets by host. Read as any JSON so an unusual shape
    /// cannot stop the whole add-on from installing; its contents are not used.
    #[serde(default, rename = "user_scripts")]
    _user_scripts: serde_json::Value,
    /// The locale that `__MSG_key__` names resolve in.
    #[serde(default)]
    pub default_locale: Option<String>,
    /// Chrome's MV3 background worker, Firefox's background scripts. Read so
    /// the browser can say honestly whether an extension will work.
    #[serde(default)]
    pub background: Option<serde_json::Value>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub host_permissions: Vec<String>,
    #[serde(default)]
    pub options_ui: Option<serde_json::Value>,
    #[serde(default)]
    pub options_page: Option<String>,
    #[serde(default)]
    pub action: Option<serde_json::Value>,
    /// Firefox: `browser_specific_settings.gecko.id`, the add-on's own name
    /// for itself, such as `uBlock0@raymondhill.net`. It stays the same
    /// whatever the downloaded file is called.
    #[serde(default)]
    pub browser_specific_settings: Option<serde_json::Value>,
    /// The older name for the same block, still used by some add-ons on
    /// addons.mozilla.org. Kept as its own field, not as a serde alias: a
    /// manifest that has both keys would otherwise fail to parse as a
    /// duplicate field.
    #[serde(default)]
    pub applications: Option<serde_json::Value>,
}

impl Manifest {
    /// The Firefox add-on id, if the manifest declares one.
    pub fn gecko_id(&self) -> Option<String> {
        [&self.browser_specific_settings, &self.applications]
            .into_iter()
            .flatten()
            .filter_map(|block| block.get("gecko")?.get("id")?.as_str())
            .map(|id| id.trim().to_string())
            .find(|id| !id.is_empty())
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ContentScript {
    #[serde(default)]
    pub matches: Vec<String>,
    /// Addresses the script must stay off even when `matches` covers them,
    /// such as an add-on that runs everywhere except its own vendor's site.
    /// Handed to WebKit as the block list next to the allow list.
    #[serde(default)]
    pub exclude_matches: Vec<String>,
    #[serde(default)]
    pub js: Vec<String>,
    #[serde(default)]
    pub css: Vec<String>,
    /// "document_start", "document_end" or `document_idle`.
    #[serde(default)]
    pub run_at: Option<String>,
    #[serde(default)]
    pub all_frames: Option<bool>,
}

/// One installed extension, as it is kept on disk.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Installed {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    /// The folder the extension was unpacked into.
    pub path: String,
    /// Where it came from, for the panel: a .crx, an .xpi, a folder.
    pub source: String,
    /// Whether it does anything in this browser, and why not.
    pub runs: bool,
    /// Loads and injects, but part of it (a background page, or an API call
    /// in a content script) does not run here. The panel says so.
    #[serde(default)]
    pub partial: bool,
    pub why_not: String,
    pub enabled: bool,
    /// The Firefox add-on id, kept so a download of the same add-on finds
    /// the copy it replaces, whatever file it came in.
    #[serde(default)]
    pub gecko_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Registry {
    #[serde(default)]
    pub extensions: Vec<Installed>,
}

fn registry_path(base: &Path) -> PathBuf {
    files::path_in(base, "extensions.json")
}

/// Where the copies live. Nothing outside this folder is ever written to or
/// deleted on an extension's behalf, which is why `remove` checks it.
fn root_at(base: &Path) -> PathBuf {
    files::path_in(base, "extensions")
}

fn registry_at(base: &Path) -> Registry {
    let mut registry: Registry = files::read_json(&registry_path(base)).unwrap_or_default();
    // Entries saved before the Firefox id was kept get it from their own
    // manifest, so a re-download still finds the copy it replaces.
    for entry in &mut registry.extensions {
        if entry.gecko_id.is_none() {
            entry.gecko_id = read_manifest(Path::new(&entry.path)).and_then(|m| m.gecko_id());
        }
    }
    registry
}

fn save_at(base: &Path, registry: &Registry) {
    files::write_json(&registry_path(base), registry);
}

pub fn registry() -> Registry {
    registry_at(&files::data_dir())
}

fn enabled_at(base: &Path) -> Vec<Installed> {
    registry_at(base).extensions.into_iter().filter(|e| e.enabled && e.runs).collect()
}


/// A match pattern, split into the three parts that decide anything.
struct Pattern<'a> {
    scheme: &'a str,
    host: &'a str,
    path: &'a str,
}

/// Chrome's pattern grammar, which is narrow on purpose: scheme, host, path,
/// nothing else. A port is not in it (Chrome and Firefox both refuse one), the
/// host may only hold a wildcard as `*.` in front, and the path is required.
///
/// An unreadable pattern gets `None`, and `pattern_matches` turns that into "no
/// match". That direction matters: a pattern nobody understands, treated as a
/// wildcard, would run an extension on every site in the world.
fn parse_pattern(pattern: &str) -> Option<Pattern<'_>> {
    if pattern.is_empty() || pattern.len() > 2048 {
        return None;
    }
    let (scheme, rest) = pattern.split_once("://")?;
    let scheme_ok = scheme == "*"
        || (!scheme.is_empty()
            && scheme.chars().next()?.is_ascii_alphabetic()
            && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')));
    if !scheme_ok {
        return None;
    }
    // Every pattern has a path, and it starts at the first slash after the
    // host. `file:///*` is the one pattern with no host at all.
    let slash = rest.find('/')?;
    let host = &rest[..slash];
    let path = &rest[slash..];
    if path.contains([' ', '\t', '#']) {
        return None;
    }
    // A port, or credentials, which a pattern cannot name. An IPv6 literal is
    // the one host with colons in it, and it is written in brackets.
    let ipv6 = host.starts_with('[') && host.ends_with(']');
    if ipv6 {
        if !host[1..host.len() - 1].chars().all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.')) {
            return None;
        }
    } else if host.contains(['@', ':', ' ', '\t']) {
        return None;
    }
    if host.is_empty() {
        if !scheme.eq_ignore_ascii_case("file") {
            return None;
        }
    } else if host == "*" {
        // any host
    } else if let Some(base) = host.strip_prefix("*.") {
        if base.is_empty() || base.contains('*') {
            return None;
        }
    } else if host.contains('*') {
        return None;
    }
    Some(Pattern { scheme, host, path })
}

fn split_url(url: &str) -> Option<(String, &str)> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme.is_empty() || !scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    Some((scheme.to_ascii_lowercase(), rest))
}

/// The host part of everything after `scheme://`, with credentials and the port
/// taken off and the case flattened.
fn url_host(rest: &str) -> String {
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let authority = &rest[..end];
    let after_at = authority.rsplit('@').next().unwrap_or(authority);
    let host = match after_at.find(']') {
        // An IPv6 literal keeps its brackets and drops what follows them.
        Some(close) => &after_at[..=close],
        None => after_at.split(':').next().unwrap_or(after_at),
    };
    host.to_ascii_lowercase()
}

/// The path a pattern is matched against, which is the rest of the address from
/// the first slash. A page with no path is `/`, and a fragment is not part of
/// what a pattern sees.
fn url_path(rest: &str) -> &str {
    match rest.find(['/', '?', '#']) {
        Some(at) if rest.as_bytes()[at] == b'/' => &rest[at..],
        _ => "/",
    }
}

/// `*` stands for any run of characters, including none, and it may appear
/// anywhere in the path. The first part is anchored at the start of the path and
/// the last at the end, so a path with no star at all is an exact path: `/a`
/// matches `/a` and not `/x/a`.
fn path_matches(pattern: &str, path: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == path;
    }
    let first = parts[0];
    if !path.starts_with(first) {
        return false;
    }
    let mut rest = &path[first.len()..];
    let last = parts[parts.len() - 1];
    for part in &parts[1..parts.len() - 1] {
        let Some(at) = rest.find(part) else { return false };
        rest = &rest[at + part.len()..];
    }
    rest.len() >= last.len() && rest.ends_with(last)
}

/// "news.ycombinator.com" matches "https://*.ycombinator.com/*" and
/// "https://news.ycombinator.com/*" but not "*://example.com/*".
///
/// The evil-suffix case is the one worth writing down: `*.example.com` matches
/// example.com and anything under it, and never example.com.evil.net, because
/// the test is either equality or a dot before the base, not a suffix alone.
fn pattern_matches(pattern: &str, url: &str) -> bool {
    let Some((url_scheme, rest)) = split_url(url) else { return false };
    if pattern.trim() == "<all_urls>" {
        // The schemes a page can actually be reached by in this browser. Not
        // about:blank, not chrome-extension, not a data: page.
        return matches!(url_scheme.as_str(), "http" | "https" | "file" | "ftp");
    }
    let Some(pattern) = parse_pattern(pattern) else { return false };
    let scheme_ok = if pattern.scheme == "*" {
        // Chrome's star scheme means http and https only; `file` is asked for
        // by name, or by <all_urls>.
        matches!(url_scheme.as_str(), "http" | "https")
    } else {
        pattern.scheme.eq_ignore_ascii_case(&url_scheme)
    };
    if !scheme_ok {
        return false;
    }
    let host = url_host(rest);
    let host_ok = if pattern.host == "*" {
        !host.is_empty()
    } else if let Some(base) = pattern.host.strip_prefix("*.") {
        let base = base.to_ascii_lowercase();
        host == base || host.ends_with(&format!(".{base}"))
    } else {
        host == pattern.host.to_ascii_lowercase()
    };
    if !host_ok {
        return false;
    }
    path_matches(pattern.path, url_path(rest))
}

pub fn matches(patterns: &[String], url: &str) -> bool {
    patterns.iter().any(|pattern| pattern_matches(pattern, url))
}

/// The manifest, or the reason it could not be read, with the parser's own
/// message: a manifest with a trailing comma says where it went wrong.
fn parse_manifest(folder: &Path) -> Result<Manifest, String> {
    let raw = std::fs::read_to_string(folder.join("manifest.json"))
        .map_err(|error| format!("no readable manifest.json ({error})"))?;
    let mut manifest: Manifest =
        serde_json::from_str(&raw).map_err(|error| format!("manifest.json does not parse: {error}"))?;
    localize(&mut manifest, folder);
    Ok(manifest)
}

fn read_manifest(folder: &Path) -> Option<Manifest> {
    parse_manifest(folder).ok()
}

/// Resolves `__MSG_key__` in the name and description from the default
/// locale's messages.json, the way Chrome and Firefox do. A key that cannot be
/// found is left as it is.
fn localize(manifest: &mut Manifest, folder: &Path) {
    let Some(locale) = manifest.default_locale.clone() else { return };
    // `inside` keeps the locale a single folder, so `../` in it cannot leave.
    let Some(path) = inside(folder, &format!("_locales/{locale}/messages.json")) else { return };
    let Ok(raw) = std::fs::read_to_string(path) else { return };
    let Ok(messages) = serde_json::from_str::<serde_json::Value>(&raw) else { return };
    let resolve = |text: &str| -> String {
        let Some(key) = text.strip_prefix("__MSG_").and_then(|rest| rest.strip_suffix("__")) else {
            return text.to_string();
        };
        // Chrome matches message keys without regard to case.
        messages
            .as_object()
            .and_then(|map| map.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)))
            .and_then(|(_, value)| value.get("message")?.as_str())
            .map(str::to_string)
            .unwrap_or_else(|| text.to_string())
    };
    manifest.name = resolve(&manifest.name);
    manifest.description = resolve(&manifest.description);
}

/// A folder to unpack into, made fresh so two installs at once cannot share it.
fn unique_temp(prefix: &str) -> Result<PathBuf, String> {
    use std::os::unix::fs::DirBuilderExt;
    static COUNTER: AtomicUsize = AtomicUsize::new(1);
    let base = std::env::temp_dir();
    for _ in 0..64 {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = base.join(format!("{prefix}-{}-{n}", std::process::id()));
        match std::fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("{}: {error}", path.display())),
        }
    }
    Err("could not make a temporary folder for the archive".to_string())
}

/// A temporary folder that goes away when the install finishes, whether it
/// worked or not.
struct Scratch {
    path: PathBuf,
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// A `.crx` is a zip with a header in front of it: `Cr24`, a version, and the
/// lengths of what follows. This returns where the zip starts.
///
/// Version 3 has one length, of a protobuf header; version 2 has two, the key
/// and the signature. Both are read as numbers from the file, so both are
/// bounds-checked before anything is sliced: a header that claims more than the
/// file holds is an error with the numbers in it, not a panic.
fn crx_offset(bytes: &[u8]) -> Result<usize, String> {
    if !bytes.starts_with(b"Cr24") {
        return Ok(0);
    }
    if bytes.len() < 12 {
        return Err(format!("the .crx header is too short: the file is {} bytes", bytes.len()));
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    let start = match version {
        3 => {
            let header = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as u64;
            12u64.checked_add(header)
        }
        2 => {
            if bytes.len() < 16 {
                return Err(format!("the .crx header is too short: the file is {} bytes", bytes.len()));
            }
            let key = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]) as u64;
            let signature = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]) as u64;
            16u64.checked_add(key).and_then(|at| at.checked_add(signature))
        }
        other => {
            return Err(format!(
                "this is a version {other} .crx, and only versions 2 and 3 are known here"
            ))
        }
    };
    let Some(start) = start else {
        return Err("the .crx header lengths overflow".to_string());
    };
    if start >= bytes.len() as u64 {
        return Err(format!(
            "the .crx header says the archive starts at byte {start}, past the end of a {} byte file",
            bytes.len()
        ));
    }
    Ok(start as usize)
}

/// Install from a `.crx`, an `.xpi`, or a folder. Returns what happened.
pub fn install(source: &Path) -> Result<Installed, String> {
    install_at(&files::data_dir(), source)
}

/// Whether a download address is the Firefox add-ons store, reached over
/// HTTPS. Only an add-on fetched from there is offered for installing. Any
/// other `.xpi` stays in Downloads as a plain file: a page that sends the
/// browser to `evil.example/x.xpi` gets a download, not an install.
pub fn from_store(uri: &str) -> bool {
    let Some(rest) = uri.strip_prefix("https://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase();
    // The store's own links carry no user name and no port other than 443.
    // `addons.mozilla.org@evil.example` and `addons.mozilla.org.evil.example`
    // both fail here.
    matches!(authority.as_str(), "addons.mozilla.org" | "addons.mozilla.org:443")
}

/// What an archive would install, read without installing it: what the
/// confirm card shows before the person says yes.
#[derive(Debug, Clone)]
pub struct Offer {
    pub name: String,
    pub version: String,
    /// The add-on's own claimed id. It is a claim: nothing here checks a
    /// signature, which is why replacing an installed add-on is said out loud.
    pub gecko_id: Option<String>,
    /// Every site pattern its scripts and styles would run on, sorted, once each.
    pub hosts: Vec<String>,
    pub runs: bool,
    pub why_not: String,
    /// The add-on already installed under the same id, which this one would
    /// replace.
    pub replaces: Option<Installed>,
}

pub fn inspect(source: &Path) -> Result<Offer, String> {
    inspect_at(&files::data_dir(), source)
}

fn inspect_at(base: &Path, source: &Path) -> Result<Offer, String> {
    let (folder, _scratch) = if source.is_dir() {
        (source.to_path_buf(), None)
    } else {
        let scratch = unpack(source)?;
        let path = scratch.path.clone();
        (path, Some(scratch))
    };
    let manifest = parse_manifest(&folder).map_err(|why| format!("{}: {why}", source.display()))?;
    if manifest.name.trim().is_empty() {
        return Err("the manifest has no name".to_string());
    }
    if !matches!(manifest.manifest_version, 2 | 3) {
        return Err(format!(
            "manifest_version {} is not one this browser knows: only 2 and 3 are",
            manifest.manifest_version
        ));
    }
    let id = id_for(&manifest, source);
    let mut hosts: Vec<String> = manifest
        .content_scripts
        .iter()
        .filter(|script| !script.js.is_empty() || !script.css.is_empty())
        .flat_map(|script| script.matches.iter().cloned())
        .chain(manifest.host_permissions.iter().cloned())
        .map(|pattern| pattern.trim().to_string())
        .filter(|pattern| !pattern.is_empty())
        .collect();
    hosts.sort();
    hosts.dedup();
    let (runs, _partial, why_not) = capability(&manifest, &folder);
    let gecko_id = manifest.gecko_id();
    // The same add-on is the same Firefox id, so a download of it finds the
    // copy it replaces, even under an id made before the gecko id was kept.
    let replaces = registry_at(base)
        .extensions
        .into_iter()
        .find(|e| e.id == id || (gecko_id.is_some() && e.gecko_id == gecko_id));
    Ok(Offer {
        gecko_id,
        name: manifest.name,
        version: manifest.version,
        hosts,
        runs,
        why_not,
        replaces,
    })
}

fn install_at(base: &Path, source: &Path) -> Result<Installed, String> {
    // An unpacked folder is used where it is; an archive is expanded into a
    // temporary folder that is thrown away once the copy is made.
    let (folder, _scratch) = if source.is_dir() {
        (source.to_path_buf(), None)
    } else {
        let scratch = unpack(source)?;
        let path = scratch.path.clone();
        (path, Some(scratch))
    };
    let manifest = read_manifest(&folder)
        .ok_or_else(|| format!("{} has no readable manifest.json", source.display()))?;
    if manifest.name.trim().is_empty() {
        return Err("the manifest has no name".to_string());
    }
    if !matches!(manifest.manifest_version, 2 | 3) {
        return Err(format!(
            "manifest_version {} is not one this browser knows: only 2 and 3 are",
            manifest.manifest_version
        ));
    }

    // The id is the extension's identity: its name, its version, and where it
    // came from. Name and version alone are not enough, since two extensions
    // can both be called Toggle at 1.0, and a shared id would make installing
    // the second replace the first. The same extension from the same source
    // hashes the same way, which is what makes a re-install a replacement.
    let id = id_for(&manifest, source);
    let root = root_at(base);
    std::fs::create_dir_all(&root).map_err(|error| format!("{}: {error}", root.display()))?;
    let target = root.join(&id);
    if target.exists() {
        std::fs::remove_dir_all(&target).map_err(|error| format!("{}: {error}", target.display()))?;
    }
    // Put it in the data folder so it survives the file it came from. A copy
    // that fails half way takes its folder with it: a half extension in the
    // registry is worse than none.
    if let Err(why) = copy_dir(&folder, &target) {
        let _ = std::fs::remove_dir_all(&target);
        return Err(why);
    }

    let (runs, partial, why_not) = capability(&manifest, &folder);
    let gecko_id = manifest.gecko_id();
    let entry = Installed {
        id,
        name: manifest.name,
        version: manifest.version,
        description: manifest.description,
        path: target.to_string_lossy().to_string(),
        source: source.to_string_lossy().to_string(),
        runs,
        partial,
        why_not,
        enabled: true,
        gecko_id: gecko_id.clone(),
    };
    let mut registry = registry_at(base);
    // An extension already recorded under an older id, from before the id
    // carried where it came from, is still the same extension: installing it
    // again replaces that entry instead of leaving a second copy of it in the
    // list, pointing at a folder that is no longer being updated. A Firefox
    // add-on is matched by its gecko id, whatever file it came in.
    let stale: Vec<String> = registry
        .extensions
        .iter()
        .filter(|e| {
            e.id != entry.id
                && ((gecko_id.is_some() && e.gecko_id == gecko_id)
                    || (e.name == entry.name && e.version == entry.version && e.source == entry.source))
        })
        .map(|e| e.id.clone())
        .collect();
    for old in registry.extensions.iter().filter(|e| stale.contains(&e.id)) {
        let path = PathBuf::from(&old.path);
        if path != target && path.starts_with(&root) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
    registry.extensions.retain(|e| e.id != entry.id && !stale.contains(&e.id));
    registry.extensions.push(entry.clone());
    save_at(base, &registry);
    changed();
    Ok(entry)
}

fn slug(text: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.len() > 48 {
        while out.len() > 48 {
            out.pop();
        }
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        fallback.to_string()
    } else {
        out
    }
}

/// A short, stable digest, so a name that turns into the same slug as another
/// name still gets an id of its own.
fn fingerprint(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash ^= 0x1f;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{:08x}", hash as u32)
}

fn id_for(manifest: &Manifest, origin: &Path) -> String {
    // A Firefox add-on names itself. Using that name means the same add-on
    // gets the same id whichever file it came from. Downloading it twice from
    // the store saves "ublock_origin-1.76.6.xpi" and then
    // "ublock_origin-1.76.6 (1).xpi". Before this change the two paths hashed
    // to two ids, so the second download became a second install. The
    // version is left out on purpose: a newer version of the same add-on
    // replaces the older one instead of sitting beside it.
    // A Firefox add-on is named by its own id alone: the name and version are
    // left out, so the same add-on gets the same id from any file it came in.
    if let Some(gecko) = manifest.gecko_id() {
        return fingerprint(&["gecko", &gecko]);
    }
    let origin = std::fs::canonicalize(origin).unwrap_or_else(|_| origin.to_path_buf());
    let origin = origin.to_string_lossy().to_string();
    format!(
        "{}-{}-{}",
        slug(&manifest.name, "extension"),
        slug(&manifest.version, "0"),
        fingerprint(&[&manifest.name, &manifest.version, &origin])
    )
}

/// What this extension will do here, and what it will not.
/// The capabilities an extension may ask for that this engine cannot give it.
///
/// Named explicitly, because "it does not run" is not a useful thing to tell
/// someone: the useful thing is which part will be missing.
const UNAVAILABLE: &[(&str, &str)] = &[
    ("tabs", "reading your open tabs"),
    ("webRequest", "rewriting requests itself"),
    ("cookies", "reading your cookies"),
    ("history", "reading your history"),
    ("downloads", "managing downloads"),
    ("bookmarks", "reading your bookmarks"),
    ("webNavigation", "watching navigation"),
    ("proxy", "choosing proxies"),
    ("management", "managing other extensions"),
];

/// Whether a content script touches `browser.` or `chrome.` as a name of its
/// own, such as `browser.runtime` or `chrome.storage`, which this engine lacks.
///
/// Only a call counts: the name must stand on its own (not the tail of a word,
/// a path or a host, so `https://chrome.google.com` is not one), it must be
/// followed by a lowercase API name (so "tell the browser. Then" is not one),
/// and it must not sit in a comment. A `//` right after a colon is a URL's,
/// not a comment's, so minified code with an address earlier on the same line
/// is still read.
fn calls_api(text: &str) -> bool {
    text.lines().any(|line| {
        let code = line.trim_start();
        if code.starts_with("//") || code.starts_with("/*") || code.starts_with('*') {
            return false;
        }
        ["browser.", "chrome."].iter().any(|prefix| {
            line.match_indices(prefix).any(|(at, _)| {
                let before = &line[..at];
                let joined = before
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '/' | ':' | '-'));
                let in_comment = before
                    .match_indices("//")
                    .any(|(i, _)| !before[..i].ends_with(':'));
                let names_api = line[at + prefix.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_lowercase());
                !joined && !in_comment && names_api
            })
        })
    })
}

/// Whether the content scripts of an extension call an API this engine lacks.
fn content_calls_api(manifest: &Manifest, folder: &Path) -> bool {
    manifest
        .content_scripts
        .iter()
        .flat_map(|script| script.js.iter())
        .filter_map(|file| inside(folder, file))
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .any(|text| calls_api(&text))
}

/// What this extension does here: whether it runs at all, whether part of it
/// is missing (`partial`), and the words the panel and the card show.
fn capability(manifest: &Manifest, folder: &Path) -> (bool, bool, String) {
    let missing: Vec<&str> = UNAVAILABLE
        .iter()
        .filter(|(name, _)| manifest.permissions.iter().any(|p| p == name))
        .map(|(_, said)| *said)
        .collect();
    let also = if missing.is_empty() {
        String::new()
    } else {
        format!("it also asks for permissions this engine cannot grant: {}.", missing.join(", "))
    };
    let has_scripts = manifest
        .content_scripts
        .iter()
        .any(|s| !s.js.is_empty() || !s.css.is_empty());
    let has_background = manifest.background.is_some();
    // Partial means it loads and injects, but a part of it is missing: a
    // background page that never starts, or a content script that calls an
    // API that is not here. Both are said before the person installs.
    let (runs, partial, base) = if has_scripts {
        if has_background {
            // Both halves, said plainly: the part that runs, and the part that
            // does not, including the consequence people notice first, which is
            // a content script that asks the background for something and waits
            // for an answer that never comes.
            (
                true,
                true,
                "its content scripts and styles run; its background page never starts, because this \
                 engine has no WebExtensions API, so nothing that happens only there happens, and a \
                 content script that asks it for an answer gets none",
            )
        } else if content_calls_api(manifest, folder) {
            (
                true,
                true,
                "its content scripts and styles run, but they call browser or chrome APIs this engine \
                 does not have, so those calls fail",
            )
        } else {
            (true, false, "")
        }
    } else if has_background {
        (
            false,
            false,
            "everything it does is in a background page, which this engine cannot start: WebKitGTK has \
             no WebExtensions API, so chrome.runtime, chrome.tabs and the rest of chrome.* are missing",
        )
    } else {
        (false, false, "it declares no content scripts, so there is nothing this engine can run")
    };
    let base = if partial && !base.is_empty() {
        format!("loads, but its main part does not run here: {base}")
    } else {
        base.to_string()
    };
    let why = match (base.is_empty(), also.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{base}."),
        (true, false) => also,
        (false, false) => format!("{base}; {also}"),
    };
    (runs, partial, why)
}

fn unpack(source: &Path) -> Result<Scratch, String> {
    let size = std::fs::metadata(source)
        .map_err(|error| format!("{}: {error}", source.display()))?
        .len();
    if size > MAX_ARCHIVE {
        return Err(format!(
            "{} is {} MB, and an extension may be at most {} MB",
            source.display(),
            size / (1024 * 1024),
            MAX_ARCHIVE / (1024 * 1024)
        ));
    }
    let bytes = std::fs::read(source).map_err(|error| format!("{}: {error}", source.display()))?;
    let zip_start = crx_offset(&bytes)?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[zip_start..]))
        .map_err(|error| format!("{} is not a readable archive: {error}", source.display()))?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!(
            "{} holds {} entries, and an extension may hold at most {MAX_ENTRIES}",
            source.display(),
            zip.len()
        ));
    }
    let scratch = Scratch { path: unique_temp("search-ext")? };
    let folder = &scratch.path;
    let mut total: u64 = 0;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|error| format!("entry {index}: {error}"))?;
        if entry.is_dir() {
            continue;
        }
        // A zip can carry a symlink, and a symlink is how an archive escapes a
        // folder: the name stays inside, the target does not. Extensions have no
        // use for one, so they are skipped rather than followed or copied.
        if entry.unix_mode().is_some_and(|mode| mode & 0o170000 == 0o120000) {
            continue;
        }
        // A zip can name `../../.bashrc` or `/etc/passwd`; this refuses both and
        // also anything with a `..` in the middle, which no extension needs.
        let Some(name) = entry.enclosed_name() else { continue };
        if name.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            continue;
        }
        // The declared size is a claim by the archive, so it is checked before
        // anything is written, and the copy below stops at the cap as well in
        // case the claim understates what the stream produces.
        let declared = entry.size();
        if declared > MAX_ENTRY {
            return Err(format!(
                "{} declares {} MB for {}, and one file may be at most {} MB",
                source.display(),
                declared / (1024 * 1024),
                name.display(),
                MAX_ENTRY / (1024 * 1024)
            ));
        }
        total = total.saturating_add(declared);
        if total > MAX_TOTAL {
            return Err(format!(
                "{} unpacks to more than the {} MB an extension may hold",
                source.display(),
                MAX_TOTAL / (1024 * 1024)
            ));
        }
        let target = folder.join(&name);
        if !target.starts_with(folder) {
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("{}: {error}", parent.display()))?;
        }
        let mut out = std::fs::File::create(&target)
            .map_err(|error| format!("{}: {error}", target.display()))?;
        let limit = MAX_ENTRY + 1;
        let written = std::io::copy(&mut (&mut entry).take(limit), &mut out)
            .map_err(|error| format!("{}: {error}", target.display()))?;
        if written > MAX_ENTRY {
            return Err(format!(
                "{} writes more than the {} MB one file may hold",
                name.display(),
                MAX_ENTRY / (1024 * 1024)
            ));
        }
        // Critic 7, round 2: the declared size is only the archive's claim. An
        // archive of thousands of entries that each declare 0 bytes but inflate
        // to the per-file cap would pass a total built from claims alone and
        // fill the disk. So the claim for this entry is swapped for the bytes
        // really written, and the whole archive is checked against what is on
        // disk.
        total = total.saturating_sub(declared).saturating_add(written);
        if total > MAX_TOTAL {
            return Err(format!(
                "{} unpacks to more than the {} MB an extension may hold",
                source.display(),
                MAX_TOTAL / (1024 * 1024)
            ));
        }
    }
    Ok(scratch)
}

/// Copy a folder of files into the extension's own folder.
///
/// Only ordinary files and folders are copied. A symlink in the source would
/// otherwise be followed, which turns a link to `~/.ssh` in an unpacked
/// extension into a copy of it in the data folder, and a link to a folder into
/// a copy of whatever it points at.
fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    copy_dir_at(from, to, 0)
}

fn copy_dir_at(from: &Path, to: &Path, depth: usize) -> Result<(), String> {
    if depth > 32 {
        return Err(format!("{} is nested deeper than a folder should be", from.display()));
    }
    std::fs::create_dir_all(to).map_err(|error| format!("{}: {error}", to.display()))?;
    let entries = std::fs::read_dir(from).map_err(|error| format!("{}: {error}", from.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let target = to.join(entry.file_name());
        let Ok(kind) = std::fs::symlink_metadata(&path) else { continue };
        if kind.file_type().is_symlink() {
            continue;
        }
        if kind.is_dir() {
            copy_dir_at(&path, &target, depth + 1)?;
        } else if kind.is_file() {
            std::fs::copy(&path, &target)
                .map_err(|error| format!("could not copy {}: {error}", path.display()))?;
        }
    }
    Ok(())
}

pub fn set_enabled(id: &str, on: bool) {
    set_enabled_at(&files::data_dir(), id, on);
}

fn set_enabled_at(base: &Path, id: &str, on: bool) {
    let mut registry = registry_at(base);
    let mut touched = false;
    for extension in &mut registry.extensions {
        if extension.id != id {
            continue;
        }
        touched = true;
        if on {
            // Turning it on re-reads what is on disk, so the switch cannot
            // claim to run something whose files were deleted or edited.
            match read_manifest(Path::new(&extension.path)) {
                Some(manifest) => {
                    let (runs, partial, why_not) = capability(&manifest, Path::new(&extension.path));
                    extension.runs = runs;
                    extension.partial = partial;
                    extension.why_not = why_not;
                    extension.enabled = true;
                }
                None => {
                    extension.runs = false;
                    extension.partial = false;
                    extension.why_not =
                        "its files are gone from the data folder, so there is nothing to run; install it again"
                            .to_string();
                    extension.enabled = false;
                }
            }
        } else {
            extension.enabled = false;
        }
    }
    if touched {
        save_at(base, &registry);
        changed();
    }
}

#[allow(dead_code)]
pub fn remove(id: &str) {
    remove_at(&files::data_dir(), id);
}

#[allow(dead_code)]
fn remove_at(base: &Path, id: &str) {
    let mut registry = registry_at(base);
    let root = root_at(base);
    let mut found = false;
    if let Some(entry) = registry.extensions.iter().find(|e| e.id == id) {
        found = true;
        // The path comes out of a JSON file, so it is not trusted: only a
        // folder under the extensions root may be deleted.
        let path = PathBuf::from(&entry.path);
        if path.starts_with(&root) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
    if !found {
        return;
    }
    registry.extensions.retain(|e| e.id != id);
    save_at(base, &registry);
    changed();
}

/// Everything an enabled extension wants added to a page, in the two forms the
/// engine takes: source text plus the address patterns it is allowed on.
///
/// The patterns go to WebKit rather than being checked here, so the engine
/// decides frame by frame, which is what makes a content script follow a page
/// as it navigates instead of only working on the address it was loaded at.
pub struct Installable {
    /// (source, allow list, block list, before the document or after it,
    /// every frame). The block list comes from `exclude_matches` and wins
    /// over the allow list, as it does in Firefox and Chrome.
    pub scripts: Vec<(String, Vec<String>, Vec<String>, bool, bool)>,
}

/// A manifest names its files by relative path and a manifest is not trusted,
/// so a `js` entry of `../../../../etc/passwd` must not be read. Only ordinary
/// components are allowed, and since neither the unzipper nor the copier ever
/// creates a symlink, a lexical check is enough to keep a name inside.
fn inside(folder: &Path, name: &str) -> Option<PathBuf> {
    if name.trim().is_empty() {
        return None;
    }
    let path = Path::new(name);
    if path.is_absolute() {
        return None;
    }
    if path.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
        return None;
    }
    Some(folder.join(path))
}

/// The patterns to hand the engine, which reads them slightly differently than
/// Chrome does, so they are expanded here rather than passed through.
///
/// WebKit's own pattern type treats a star scheme as any scheme at all, which
/// would put an extension on local `file:` pages it never asked for, and treats
/// `*.example.com` as strictly a subdomain, so the base domain would be skipped.
/// Both are fixed by naming what the pattern means: the two schemes, and the
/// base host next to the wildcard one.
fn allow_list(pattern: &str) -> Vec<String> {
    let all_urls = pattern.trim() == "<all_urls>";
    if all_urls {
        return ["http://*/*", "https://*/*", "file:///*"].map(str::to_string).to_vec();
    }
    let Some(parsed) = parse_pattern(pattern) else {
        // A pattern this file cannot read is not handed on to the engine, where
        // it might be read more generously than the extension meant.
        return Vec::new();
    };
    let schemes: Vec<&str> = if parsed.scheme == "*" {
        vec!["http", "https"]
    } else {
        vec![parsed.scheme]
    };
    let path = if parsed.path.is_empty() { "/*" } else { parsed.path };
    let mut out = Vec::new();
    for scheme in schemes {
        match parsed.host.strip_prefix("*.") {
            Some(base) => {
                out.push(format!("{scheme}://{base}{path}"));
                out.push(format!("{scheme}://{}.{base}{path}", "*"));
            }
            None => out.push(format!("{scheme}://{}{path}", parsed.host)),
        }
    }
    out
}

/// An extension's style sheet, as a script that puts a style element in the
/// page.
///
/// WebKitGTK's user style sheets do not reach the page on this engine: a sheet
/// added to the content manager is accepted and then has no effect, at user or
/// author level, which was measured with a rule that changed the page's own
/// background. A style element a script appends is author-level CSS, which is
/// where an extension's CSS belongs, and it is what the CSS in a Chrome content
/// script amounts to.
fn css_script(css: &str) -> String {
    // A backslash is doubled first, so the escapes added after it are not
    // doubled too. `${` would start a template substitution, so it is escaped.
    let escaped = css
        .replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
        .replace("</", "<\\/");
    format!(
        r#"(function () {{
  const text = `{escaped}`;
  const put = () => {{
    if (!text || document.getElementById('__search_ext_style_{hash}')) return true;
    const holder = document.head || document.documentElement;
    if (!holder) return false;
    const node = document.createElement('style');
    node.id = '__search_ext_style_{hash}';
    node.textContent = text;
    holder.appendChild(node);
    return true;
  }};
  if (put()) return;
  const watch = new MutationObserver(() => {{ if (put()) watch.disconnect(); }});
  watch.observe(document, {{ childList: true, subtree: true }});
  document.addEventListener('DOMContentLoaded', put, {{ once: true }});
}})();"#,
        escaped = escaped,
        hash = fingerprint(&[css])
    )
}


pub fn installable() -> Installable {
    installable_at(&files::data_dir())
}

fn installable_at(base: &Path) -> Installable {
    let mut out = Installable { scripts: Vec::new() };
    for entry in enabled_at(base) {
        let folder = PathBuf::from(&entry.path);
        let Some(manifest) = read_manifest(&folder) else { continue };
        for script in &manifest.content_scripts {
            let allow: Vec<String> = script.matches.iter().flat_map(|p| allow_list(p)).collect();
            if allow.is_empty() {
                continue;
            }
            // Expanded the same way as the allow list, so `*.example.com`
            // keeps the script off the base domain too. A pattern this file
            // cannot read expands to nothing and blocks nothing.
            let block: Vec<String> = script.exclude_matches.iter().flat_map(|p| allow_list(p)).collect();
            let at_start = script.run_at.as_deref() == Some("document_start");
            for file in &script.js {
                let Some(path) = inside(&folder, file) else { continue };
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.scripts.push((
                        text,
                        allow.clone(),
                        block.clone(),
                        at_start,
                        script.all_frames.unwrap_or(false),
                    ));
                }
            }
            for file in &script.css {
                let Some(path) = inside(&folder, file) else { continue };
                if let Ok(text) = std::fs::read_to_string(&path) {
                    // Styles go in before the document is drawn, so a page is
                    // never seen unstyled and then restyled.
                    out.scripts.push((
                        css_script(&text),
                        allow.clone(),
                        block.clone(),
                        true,
                        script.all_frames.unwrap_or(false),
                    ));
                }
            }
        }
    }
    out
}

/// What the manifest asks for that the browser will not draw, in the words the
/// panel can show. A toolbar button is the common one: extensions expect one in
/// the toolbar and this browser has no toolbar to put it in, which is worth
/// saying out loud rather than leaving someone to wonder where it went.
pub fn not_drawn(entry: &Installed) -> Vec<String> {
    let Some(manifest) = read_manifest(Path::new(&entry.path)) else { return Vec::new() };
    let mut out = Vec::new();
    if manifest.action.is_some() {
        out.push("its toolbar button is not drawn: this browser has no toolbar".to_string());
    }
    if !manifest.host_permissions.is_empty() {
        // Host permissions gate an extension's own API calls in Chrome. Nothing
        // here calls an API, so the only access list that means anything is the
        // one on each content script.
        out.push(format!(
            "it asks for access to {} address patterns, which changes nothing here: the patterns on \
             its content scripts decide where it runs",
            manifest.host_permissions.len()
        ));
    }
    out
}

/// An extension that ships an options page, if it has one, as a path that can be
/// opened in a tab. Chrome keeps it in `options_ui.page`, Firefox in
/// `options_ui.page` too, and older ones in `options_page`.
pub fn options_page(entry: &Installed) -> Option<String> {
    let manifest = read_manifest(Path::new(&entry.path))?;
    let page = manifest
        .options_ui
        .as_ref()
        .and_then(|ui| ui.get("page"))
        .and_then(|page| page.as_str())
        .or(manifest.options_page.as_deref())?;
    // The page is a file the tab opens, so it stays inside the extension: a
    // manifest that names `../../../../etc/passwd` gets nothing.
    let path = inside(Path::new(&entry.path), page)?;
    Some(path.to_string_lossy().to_string())
}

/// The extensions that act on one address, by name. The panel uses this to say
/// which of them this page is subject to, rather than only that they exist.
pub fn here(url: &str) -> Vec<String> {
    here_at(&files::data_dir(), url)
}

fn here_at(base: &Path, url: &str) -> Vec<String> {
    let mut names = Vec::new();
    for entry in enabled_at(base) {
        let folder = PathBuf::from(&entry.path);
        let Some(manifest) = read_manifest(&folder) else { continue };
        let acts = manifest
            .content_scripts
            .iter()
            .any(|script| matches(&script.matches, url));
        if acts {
            names.push(entry.name);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A temporary folder that is not the user's data folder, so a test cannot
    /// read or write anything real.
    struct Temp(PathBuf);

    impl Temp {
        fn new(tag: &str) -> Temp {
            static COUNTER: AtomicUsize = AtomicUsize::new(1);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "search-ext-test-{tag}-{}-{n}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("a temporary folder");
            Temp(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn write_folder(base: &Path, name: &str, manifest: serde_json::Value, files: &[(&str, &str)]) -> PathBuf {
        let folder = base.join(name);
        std::fs::create_dir_all(&folder).expect("a folder");
        std::fs::write(folder.join("manifest.json"), manifest.to_string()).expect("a manifest");
        for (file, text) in files {
            let path = folder.join(file);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).expect("a parent");
            }
            std::fs::write(path, text).expect("a file");
        }
        folder
    }

    fn zip_of(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, text) in entries {
            writer.start_file(*name, options).expect("an entry");
            writer.write_all(text.as_bytes()).expect("bytes");
        }
        writer.finish().expect("a finished archive").into_inner()
    }

    fn crx3(zip: &[u8]) -> Vec<u8> {
        let header = b"a small protobuf header";
        let mut out = Vec::new();
        out.extend_from_slice(b"Cr24");
        out.extend_from_slice(&3u32.to_le_bytes());
        out.extend_from_slice(&(header.len() as u32).to_le_bytes());
        out.extend_from_slice(header);
        out.extend_from_slice(zip);
        out
    }

    fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
        hay.windows(needle.len()).position(|w| w == needle)
    }

    /// Rewrite the uncompressed size an archive declares for a name, leaving the
    /// data alone: the zip header and the central directory both carry it.
    fn patch_declared_size(zip: &mut [u8], name: &str, size: u32) {
        let needle = name.as_bytes();
        let mut at = 0;
        while let Some(found) = find(&zip[at..], needle) {
            let pos = at + found;
            if pos >= 30 && &zip[pos - 30..pos - 26] == b"PK\x03\x04" {
                zip[pos - 8..pos - 4].copy_from_slice(&size.to_le_bytes());
            } else if pos >= 46 && &zip[pos - 46..pos - 42] == b"PK\x01\x02" {
                zip[pos - 22..pos - 18].copy_from_slice(&size.to_le_bytes());
            }
            at = pos + 1;
        }
    }

    fn manifest(name: &str) -> serde_json::Value {
        serde_json::json!({ "manifest_version": 3, "name": name, "version": "1.0" })
    }

    #[test]
    fn a_pattern_knows_its_hosts() {
        assert!(pattern_matches("https://*.ycombinator.com/*", "https://news.ycombinator.com/item?id=1"));
        assert!(pattern_matches("*://*/*", "https://example.com/"));
        assert!(pattern_matches("<all_urls>", "https://example.com/x"));
        assert!(!pattern_matches("https://*.ycombinator.com/*", "https://example.com/"));
        assert!(!pattern_matches("https://example.com/a/*", "https://example.com/b/1"));
        assert!(pattern_matches("https://example.com/a/*", "https://example.com/a/1"));
        assert!(!pattern_matches("http://*/*", "https://example.com/"));
    }

    #[test]
    fn a_wildcard_host_covers_the_domain_and_no_lookalike() {
        let pattern = "*://*.example.com/*";
        // The base domain itself, which is the case a naive suffix test drops.
        assert!(pattern_matches(pattern, "http://example.com/"));
        assert!(pattern_matches(pattern, "https://example.com/page?x=1"));
        assert!(pattern_matches(pattern, "https://sub.example.com/"));
        assert!(pattern_matches(pattern, "https://a.b.example.com/deep/path"));
        assert!(pattern_matches(pattern, "https://example.com:8443/page"));
        assert!(pattern_matches(pattern, "https://user:pw@example.com/page"));
        // The three ways a suffix test goes wrong.
        assert!(!pattern_matches(pattern, "https://notexample.com/"));
        assert!(!pattern_matches(pattern, "https://example.com.evil.net/"));
        assert!(!pattern_matches(pattern, "https://evil.net/?next=example.com"));
        assert!(!pattern_matches(pattern, "https://example.com@evil.net/"));
        assert!(!pattern_matches(pattern, "file:///example.com"));
    }

    #[test]
    fn an_explicit_path_is_matched_and_nothing_else() {
        let pattern = "https://example.com/path/*";
        assert!(pattern_matches(pattern, "https://example.com/path/"));
        assert!(pattern_matches(pattern, "https://example.com/path/one/two"));
        assert!(pattern_matches(pattern, "https://example.com/path/one?q=1"));
        assert!(!pattern_matches(pattern, "https://example.com/path"));
        assert!(!pattern_matches(pattern, "https://example.com/other"));
        assert!(!pattern_matches(pattern, "https://example.com/pathological"));
        assert!(!pattern_matches(pattern, "http://example.com/path/one"));
        // A path without a trailing star is an exact path, not a suffix.
        assert!(pattern_matches("https://example.com/a", "https://example.com/a"));
        assert!(!pattern_matches("https://example.com/a", "https://example.com/a/b"));
        assert!(!pattern_matches("https://example.com/a", "https://example.com/x/a"));
        // Any host is written as a star on its own and is still valid.
        assert!(pattern_matches("https://*/path", "https://example.com/path"));
        assert!(!pattern_matches("https://*/path", "https://example.com/other"));
        // An IPv6 literal can be named, which a "no colons" host test would
        // otherwise refuse along with a port.
        assert!(pattern_matches("http://[::1]/*", "http://[::1]:8123/page"));
        assert!(!pattern_matches("http://[::1]/*", "http://[::2]/page"));
        // A star in the middle of a path still stands for anything.
        assert!(pattern_matches("https://example.com/*/edit", "https://example.com/page/edit"));
        assert!(!pattern_matches("https://example.com/*/edit", "https://example.com/page/view"));
    }

    #[test]
    fn a_port_in_a_pattern_makes_it_match_nothing() {
        // Neither Chrome nor Firefox allow a port in a match pattern, so the
        // only safe reading of one is that it matches nothing. Reading it as
        // the host alone would run the extension on every port.
        assert!(!pattern_matches("http://example.com:8080/*", "http://example.com:8080/page"));
        assert!(!pattern_matches("http://example.com:8080/*", "http://example.com/page"));
        // A port in the address is not part of what a pattern matches.
        assert!(pattern_matches("http://example.com/*", "http://example.com:8080/page"));
        assert!(pattern_matches("http://127.0.0.1/*", "http://127.0.0.1:8123/plain"));
    }

    #[test]
    fn a_file_pattern_matches_file_addresses_only() {
        assert!(pattern_matches("file:///*", "file:///home/someone/page.html"));
        assert!(!pattern_matches("file:///*", "https://example.com/"));
        assert!(!pattern_matches("https://*/*", "file:///home/someone/page.html"));
        assert!(!pattern_matches("*://*/*", "file:///home/someone/page.html"));
        assert!(pattern_matches("file:///*", "file:///"));
    }

    #[test]
    fn all_urls_covers_the_schemes_a_page_can_have() {
        assert!(pattern_matches("<all_urls>", "http://example.com/"));
        assert!(pattern_matches("<all_urls>", "https://example.com/"));
        assert!(pattern_matches("<all_urls>", "file:///home/someone/page.html"));
        assert!(pattern_matches("<all_urls>", "ftp://example.com/file"));
        assert!(!pattern_matches("<all_urls>", "about:blank"));
        assert!(!pattern_matches("<all_urls>", "data:text/html,hi"));
        assert!(!pattern_matches("<all_urls>", ""));
    }

    #[test]
    fn an_invalid_pattern_matches_nothing() {
        // Each of these is something a manifest might hold by mistake. None of
        // them may be read as "everything".
        for pattern in [
            "",
            "   ",
            "example.com/*",
            "://example.com/*",
            "*://",
            "*://*",
            "*://*example.com/*",
            "https://example.com",
            "https://*.example.com",
            "https://exa mple.com/*",
            "https://example.com/*#frag",
            "https://example.com:443/*",
            "*://*.example.com",
            "https://[::1/*",
        ] {
            for url in [
                "https://example.com/x",
                "http://example.com/x",
                "file:///x",
                "about:blank",
            ] {
                assert!(!pattern_matches(pattern, url), "{pattern:?} matched {url}");
            }
        }
        // And a pattern that is fine does not match a thing that is not an URL.
        assert!(!pattern_matches("https://example.com/*", "not an url"));
        assert!(!pattern_matches("https://example.com/*", "/a/path"));
    }

    #[test]
    fn a_manifest_version_outside_two_and_three_is_refused() {
        let temp = Temp::new("mv");
        for version in [1, 4] {
            let folder = write_folder(
                temp.path(),
                &format!("v{version}"),
                serde_json::json!({ "manifest_version": version, "name": "Versioned", "version": "1.0" }),
                &[],
            );
            let why = install_at(temp.path(), &folder).expect_err("a version this browser does not know");
            assert!(why.contains("manifest_version"), "{why}");
        }
        for version in [2, 3] {
            let folder = write_folder(
                temp.path(),
                &format!("ok{version}"),
                serde_json::json!({ "manifest_version": version, "name": format!("Fine {version}"), "version": "1.0" }),
                &[],
            );
            assert!(install_at(temp.path(), &folder).is_ok(), "version {version}");
        }
    }

    #[test]
    fn a_manifest_without_a_name_is_refused() {
        let temp = Temp::new("noname");
        let folder = write_folder(
            temp.path(),
            "anonymous",
            serde_json::json!({ "manifest_version": 3, "name": "   ", "version": "1.0" }),
            &[],
        );
        let why = install_at(temp.path(), &folder).expect_err("a manifest with no name");
        assert!(why.contains("no name"), "{why}");
    }

    #[test]
    fn the_style_script_carries_its_css_and_lands_once() {
        let css = "body { background: #123456 !important; }";
        let script = css_script(css);
        assert!(script.contains(css), "the css text is in the script");
        assert!(script.contains("__search_ext_style_"), "it has an id to check");
        assert!(script.contains("getElementById"), "a second run checks first");
        // The same css makes the same script, so two pages do not fight over
        // two different ids for one sheet.
        assert_eq!(script, css_script(css));
        assert_ne!(script, css_script("body { color: red; }"));
        // A closing tag inside the css must not end the script element.
        let hostile = css_script("a{} </style><script>alert(1)</script>");
        assert!(!hostile.contains("</style"), "the tag is escaped");
        assert!(!hostile.contains("</script"), "the tag is escaped");
    }

    #[test]
    fn two_extensions_do_not_share_an_id() {
        let temp = Temp::new("ids");
        // Same name, same version, different extension: a slug alone would
        // collide here and one would replace the other.
        let one = write_folder(temp.path(), "one", manifest("Toggle"), &[]);
        let two = write_folder(temp.path(), "two", manifest("Toggle"), &[]);
        let first = install_at(temp.path(), &one).expect("the first");
        let second = install_at(temp.path(), &two).expect("the second");
        assert_ne!(first.id, second.id);
        assert_eq!(registry_at(temp.path()).extensions.len(), 2);

        // Two names that slug the same way are still two extensions.
        let a = write_folder(temp.path(), "a", manifest("Cool!"), &[]);
        let b = write_folder(temp.path(), "b", manifest("Cool?"), &[]);
        let third = install_at(temp.path(), &a).expect("punctuation one");
        let fourth = install_at(temp.path(), &b).expect("punctuation two");
        assert_ne!(third.id, fourth.id);
    }

    #[test]
    fn installing_the_same_extension_again_replaces_it() {
        let temp = Temp::new("replace");
        let folder = write_folder(temp.path(), "same", manifest("Marker"), &[("a.js", "1;")]);
        let first = install_at(temp.path(), &folder).expect("the first");
        std::fs::write(folder.join("a.js"), "2;").expect("an edited script");
        let second = install_at(temp.path(), &folder).expect("the second");
        assert_eq!(first.id, second.id);
        let registry = registry_at(temp.path());
        assert_eq!(registry.extensions.len(), 1, "one entry, not two");
        // And the copy on disk is the new one, not the old one left behind.
        let copied = std::fs::read_to_string(Path::new(&second.path).join("a.js")).expect("the copy");
        assert_eq!(copied, "2;");
    }

    #[test]
    fn an_entry_from_an_older_id_is_replaced_not_left_behind() {
        let temp = Temp::new("migrate");
        let folder = write_folder(temp.path(), "same", manifest("Marker"), &[("a.js", "1;")]);
        // What an older build would have recorded: name and version only, in its
        // own folder.
        let old_folder = root_at(temp.path()).join("marker-1_0");
        std::fs::create_dir_all(&old_folder).expect("the old folder");
        let mut registry = Registry::default();
        registry.extensions.push(Installed {
            id: "marker-1_0".to_string(),
            name: "Marker".to_string(),
            version: "1.0".to_string(),
            description: String::new(),
            path: old_folder.to_string_lossy().to_string(),
            source: folder.to_string_lossy().to_string(),
            runs: true,
            partial: false,
            why_not: String::new(),
            enabled: true,
            gecko_id: None,
        });
        save_at(temp.path(), &registry);

        let entry = install_at(temp.path(), &folder).expect("a re-install");
        let registry = registry_at(temp.path());
        assert_eq!(registry.extensions.len(), 1, "one Marker, not two");
        assert_eq!(registry.extensions[0].id, entry.id);
        assert!(!old_folder.exists(), "the folder nobody points at is gone");
    }

    #[test]
    fn a_firefox_add_on_downloaded_twice_is_one_install() {
        let temp = Temp::new("gecko-twice");
        // The store saves the second download as "name (1).xpi". Two folders
        // stand in for the two files: same add-on, different paths.
        let addon = serde_json::json!({
            "manifest_version": 2,
            "name": "uBlock Origin",
            "version": "1.76.6",
            "browser_specific_settings": { "gecko": { "id": "uBlock0@raymondhill.net" } }
        });
        let one = write_folder(temp.path(), "ublock_origin-1.76.6", addon.clone(), &[("a.js", "1;")]);
        let two = write_folder(temp.path(), "ublock_origin-1.76.6 (1)", addon, &[("a.js", "2;")]);
        let first = install_at(temp.path(), &one).expect("the first download");
        let second = install_at(temp.path(), &two).expect("the second download");
        assert_eq!(first.id, second.id, "the add-on's own id decides, not the file name");
        let registry = registry_at(temp.path());
        assert_eq!(registry.extensions.len(), 1, "one uBlock Origin, not two");
        let installed: Vec<_> = std::fs::read_dir(root_at(temp.path()))
            .expect("the extensions folder")
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().is_dir())
            .collect();
        assert_eq!(installed.len(), 1, "one folder on disk");
        let copied = std::fs::read_to_string(Path::new(&second.path).join("a.js")).expect("the copy");
        assert_eq!(copied, "2;", "the later download is the one kept");
    }

    #[test]
    fn a_newer_version_of_a_firefox_add_on_replaces_the_older() {
        let temp = Temp::new("gecko-update");
        let with_version = |version: &str| {
            serde_json::json!({
                "manifest_version": 2,
                "name": "Marker",
                "version": version,
                "browser_specific_settings": { "gecko": { "id": "marker@example.org" } }
            })
        };
        let old = write_folder(temp.path(), "old", with_version("1.0"), &[]);
        let new = write_folder(temp.path(), "new", with_version("2.0"), &[]);
        let first = install_at(temp.path(), &old).expect("1.0");
        let second = install_at(temp.path(), &new).expect("2.0");
        assert_eq!(first.id, second.id);
        let registry = registry_at(temp.path());
        assert_eq!(registry.extensions.len(), 1);
        assert_eq!(registry.extensions[0].version, "2.0");
    }

    #[test]
    fn only_the_add_ons_store_over_https_is_the_store() {
        assert!(from_store("https://addons.mozilla.org/firefox/downloads/file/1/ublock_origin-1.76.6.xpi"));
        assert!(from_store("https://addons.mozilla.org:443/x.xpi"));
        assert!(from_store("https://ADDONS.MOZILLA.ORG/x.xpi"));
        assert!(from_store("https://addons.mozilla.org?x=1"));
        // Plain HTTP, a look-alike host, a user name in front, another port,
        // another site, and nothing at all are all refused.
        assert!(!from_store("http://addons.mozilla.org/x.xpi"));
        assert!(!from_store("https://addons.mozilla.org.evil.example/x.xpi"));
        assert!(!from_store("https://addons.mozilla.org@evil.example/x.xpi"));
        assert!(!from_store("https://addons.mozilla.org:8443/x.xpi"));
        assert!(!from_store("https://evil.example/addons.mozilla.org/x.xpi"));
        assert!(!from_store("https://xaddons.mozilla.org/x.xpi"));
        assert!(!from_store(""));
    }

    #[test]
    fn inspecting_an_add_on_installs_nothing_and_lists_its_sites() {
        let temp = Temp::new("inspect");
        let addon = serde_json::json!({
            "manifest_version": 2,
            "name": "Marker",
            "version": "1.0",
            "browser_specific_settings": { "gecko": { "id": "marker@example.org" } },
            "content_scripts": [
                { "matches": ["https://b.example/*", "https://a.example/*"], "js": ["a.js"] },
                { "matches": ["https://a.example/*"], "css": ["a.css"] },
                { "matches": ["https://nothing.example/*"] }
            ]
        });
        let folder = write_folder(temp.path(), "marker", addon, &[("a.js", "1;"), ("a.css", "p{}")]);
        let offer = inspect_at(temp.path(), &folder).expect("an offer");
        assert_eq!(offer.name, "Marker");
        assert_eq!(offer.gecko_id.as_deref(), Some("marker@example.org"));
        assert_eq!(offer.hosts, vec!["https://a.example/*".to_string(), "https://b.example/*".to_string()]);
        assert!(offer.runs);
        assert!(offer.replaces.is_none());
        assert!(registry_at(temp.path()).extensions.is_empty(), "nothing recorded");
        assert!(!root_at(temp.path()).exists(), "nothing copied");
    }

    #[test]
    fn inspecting_names_the_add_on_it_would_replace() {
        let temp = Temp::new("inspect-replace");
        let addon = |name: &str| {
            serde_json::json!({
                "manifest_version": 2,
                "name": name,
                "version": "1.0",
                "browser_specific_settings": { "gecko": { "id": "uBlock0@raymondhill.net" } },
                "content_scripts": [{ "matches": ["<all_urls>"], "js": ["a.js"] }]
            })
        };
        let real = write_folder(temp.path(), "real", addon("uBlock Origin"), &[("a.js", "1;")]);
        install_at(temp.path(), &real).expect("the real one");
        let fake = write_folder(temp.path(), "fake", addon("uBlock Origin"), &[("a.js", "evil();")]);
        let offer = inspect_at(temp.path(), &fake).expect("an offer");
        let old = offer.replaces.as_ref().expect("the installed one is named");
        assert_eq!(old.name, "uBlock Origin");
        let message = crate::permissions::addon_message("addons.mozilla.org", &offer);
        assert!(message.contains("from addons.mozilla.org"), "{message}");
        assert!(message.contains("every page you visit"), "{message}");
        assert!(message.contains("It replaces uBlock Origin 1.0"), "{message}");
        assert!(message.contains("nothing here checks a signature"), "{message}");
        // Still only one install on disk, and it is the real one: reading the
        // offer replaced nothing.
        let registry = registry_at(temp.path());
        assert_eq!(registry.extensions.len(), 1);
        let copied = std::fs::read_to_string(Path::new(&registry.extensions[0].path).join("a.js")).expect("the copy");
        assert_eq!(copied, "1;");
    }

    #[test]
    fn the_confirm_card_lists_a_few_sites_and_counts_the_rest() {
        let offer = Offer {
            name: "Many".to_string(),
            version: "2.0".to_string(),
            gecko_id: None,
            hosts: (1..=7).map(|n| format!("https://s{n}.example/*")).collect(),
            runs: true,
            why_not: String::new(),
            replaces: None,
        };
        let message = crate::permissions::addon_message("", &offer);
        assert!(message.starts_with("Install Many 2.0 from an unknown address?"), "{message}");
        assert!(message.contains("https://s5.example/*, and 2 more."), "{message}");
        assert!(!message.contains("s6.example"), "{message}");
        assert!(!message.contains("replaces"), "{message}");
    }

    #[test]
    fn the_older_applications_key_is_read_too() {
        let manifest: Manifest = serde_json::from_value(serde_json::json!({
            "manifest_version": 2,
            "name": "Old style",
            "version": "1.0",
            "applications": { "gecko": { "id": "old@example.org" } }
        }))
        .expect("a manifest");
        assert_eq!(manifest.gecko_id().as_deref(), Some("old@example.org"));

        // Both keys present must still parse; the newer one wins.
        let both: Manifest = serde_json::from_value(serde_json::json!({
            "manifest_version": 2,
            "name": "Both",
            "version": "1.0",
            "browser_specific_settings": { "gecko": { "id": "new@example.org" } },
            "applications": { "gecko": { "id": "old@example.org" } }
        }))
        .expect("a manifest with both keys");
        assert_eq!(both.gecko_id().as_deref(), Some("new@example.org"));

        // A blank id is no id: the install falls back to name, version and source.
        let blank: Manifest = serde_json::from_value(serde_json::json!({
            "manifest_version": 2,
            "name": "Blank",
            "version": "1.0",
            "browser_specific_settings": { "gecko": { "id": "  " } }
        }))
        .expect("a manifest with a blank id");
        assert_eq!(blank.gecko_id(), None);
    }

    #[test]
    fn removing_and_disabling_leave_the_registry_true() {
        let temp = Temp::new("state");
        let folder = write_folder(
            temp.path(),
            "marker",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Marker",
                "version": "1.0",
                "content_scripts": [{ "matches": ["*://example.com/*"], "js": ["a.js"] }]
            }),
            &[("a.js", "1;")],
        );
        let entry = install_at(temp.path(), &folder).expect("an install");
        assert_eq!(here_at(temp.path(), "https://example.com/x"), vec!["Marker".to_string()]);

        set_enabled_at(temp.path(), &entry.id, false);
        assert!(here_at(temp.path(), "https://example.com/x").is_empty(), "a disabled one does not act");
        assert!(!registry_at(temp.path()).extensions[0].enabled);

        set_enabled_at(temp.path(), &entry.id, true);
        assert_eq!(here_at(temp.path(), "https://example.com/x").len(), 1);

        // A copy that is deleted behind the browser's back cannot be switched
        // on into a claim that it runs.
        std::fs::remove_dir_all(&entry.path).expect("the folder goes");
        set_enabled_at(temp.path(), &entry.id, true);
        let entry = &registry_at(temp.path()).extensions[0];
        assert!(!entry.enabled && !entry.runs, "{entry:?}");
        assert!(entry.why_not.contains("gone"), "{}", entry.why_not);

        remove_at(temp.path(), &entry.id);
        assert!(registry_at(temp.path()).extensions.is_empty());
        assert!(crate::extensions::installable_at(temp.path()).scripts.is_empty());
    }

    #[test]
    fn removing_does_not_delete_outside_the_extensions_folder() {
        let temp = Temp::new("delete");
        let outside = temp.path().join("precious");
        std::fs::create_dir_all(&outside).expect("a folder");
        std::fs::write(outside.join("keep.txt"), "keep").expect("a file");
        // A registry entry whose path was tampered with must not be obeyed.
        let mut registry = Registry::default();
        registry.extensions.push(Installed {
            id: "evil-1_0-00000000".to_string(),
            name: "Evil".to_string(),
            version: "1.0".to_string(),
            description: String::new(),
            path: outside.to_string_lossy().to_string(),
            source: String::new(),
            runs: true,
            partial: false,
            why_not: String::new(),
            enabled: true,
            gecko_id: None,
        });
        save_at(temp.path(), &registry);
        remove_at(temp.path(), "evil-1_0-00000000");
        assert!(outside.join("keep.txt").exists(), "nothing outside the root is deleted");
        assert!(registry_at(temp.path()).extensions.is_empty(), "and the entry is gone");
    }

    #[test]
    fn an_archive_cannot_write_outside_its_folder() {
        let temp = Temp::new("zipescape");
        let archive = zip_of(&[
            ("manifest.json", &manifest("Escaper").to_string()),
            ("../escaped.txt", "outside"),
            ("/absolute.txt", "outside"),
            ("a/../../escaped-too.txt", "outside"),
            ("keep.txt", "inside"),
        ]);
        let source = temp.path().join("escaper.zip");
        std::fs::write(&source, archive).expect("an archive");
        let entry = install_at(temp.path(), &source).expect("it installs");
        let folder = PathBuf::from(&entry.path);
        assert!(folder.join("keep.txt").exists(), "the ordinary file is there");
        assert!(!folder.join("escaped.txt").exists());
        assert!(!folder.join("absolute.txt").exists());
        assert!(!folder.join("escaped-too.txt").exists());
        assert!(!temp.path().join("escaped.txt").exists(), "nothing landed above the folder");
        assert!(!Path::new("/absolute.txt").exists(), "and no absolute path was obeyed");
    }

    #[test]
    fn a_symlink_in_an_archive_is_not_created() {
        let temp = Temp::new("symlink");
        let mut writer = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        writer.start_file("manifest.json", options).expect("an entry");
        writer.write_all(manifest("Linker").to_string().as_bytes()).expect("bytes");
        // A Unix symlink entry: mode 0o120777 in the high half of the external
        // attributes, with the target as the body.
        let link = options.unix_permissions(0o120777);
        writer.start_file("passwd", link).expect("an entry");
        writer.write_all(b"/etc/passwd").expect("bytes");
        let archive = writer.finish().expect("a finished archive").into_inner();
        let source = temp.path().join("linker.zip");
        std::fs::write(&source, archive).expect("an archive");
        let entry = install_at(temp.path(), &source).expect("it installs");
        let linked = PathBuf::from(&entry.path).join("passwd");
        let kind = std::fs::symlink_metadata(&linked).expect("a file entry, not a link");
        assert!(!kind.file_type().is_symlink(), "a symlink entry is skipped");
    }

    #[test]
    fn a_crx_header_that_lies_is_refused_not_panicked() {
        let temp = Temp::new("crx");
        let zip = zip_of(&[("manifest.json", &manifest("Crx").to_string())]);

        // A real version 3 header installs: this is what a Chrome store file is.
        let good = temp.path().join("good.crx");
        std::fs::write(&good, crx3(&zip)).expect("a file");
        assert!(install_at(temp.path(), &good).is_ok());

        // A header claiming more header than the file holds.
        let mut lying = Vec::new();
        lying.extend_from_slice(b"Cr24");
        lying.extend_from_slice(&3u32.to_le_bytes());
        lying.extend_from_slice(&1_000_000u32.to_le_bytes());
        lying.extend_from_slice(&zip);
        let lying_path = temp.path().join("lying.crx");
        std::fs::write(&lying_path, &lying).expect("a file");
        let why = install_at(temp.path(), &lying_path).expect_err("a header past the end");
        assert!(why.contains("past the end"), "{why}");

        // A truncated header, an unknown version, and a file that is only magic.
        let short = temp.path().join("short.crx");
        std::fs::write(&short, b"Cr24\x03\x00").expect("a file");
        assert!(install_at(temp.path(), &short).is_err());

        let mut version = Vec::new();
        version.extend_from_slice(b"Cr24");
        version.extend_from_slice(&9u32.to_le_bytes());
        version.extend_from_slice(&0u32.to_le_bytes());
        version.extend_from_slice(b"....");
        let version_path = temp.path().join("v9.crx");
        std::fs::write(&version_path, &version).expect("a file");
        let why = install_at(temp.path(), &version_path).expect_err("an unknown version");
        assert!(why.contains("version 9"), "{why}");

        let magic = temp.path().join("magic.crx");
        std::fs::write(&magic, b"Cr24").expect("a file");
        assert!(install_at(temp.path(), &magic).is_err());
    }

    #[test]
    fn an_entry_that_declares_more_than_the_cap_is_refused() {
        let temp = Temp::new("cap");
        let mut archive = zip_of(&[
            ("manifest.json", &manifest("Huge").to_string()),
            ("nothing.bin", "x"),
        ]);
        patch_declared_size(&mut archive, "nothing.bin", (MAX_ENTRY + 1) as u32);
        let source = temp.path().join("huge.xpi");
        std::fs::write(&source, archive).expect("an archive");
        let why = install_at(temp.path(), &source).expect_err("an entry over the cap");
        assert!(why.contains("at most"), "{why}");
    }

    #[test]
    fn a_manifest_cannot_name_a_file_outside_its_folder() {
        let temp = Temp::new("manifest-paths");
        let secret = temp.path().join("secret.js");
        std::fs::write(&secret, "document.title = 'stolen';").expect("a file");
        let folder = write_folder(
            temp.path(),
            "sneaky",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Sneaky",
                "version": "1.0",
                "content_scripts": [{
                    "matches": ["<all_urls>"],
                    "js": ["../secret.js", "ok.js"],
                    "css": ["../secret.css"]
                }],
                "options_page": "../secret.js"
            }),
            &[("ok.js", "1;")],
        );
        let entry = install_at(temp.path(), &folder).expect("an install");
        let scripts = installable_at(temp.path());
        let texts: Vec<&str> = scripts.scripts.iter().map(|(text, ..)| text.as_str()).collect();
        assert!(texts.contains(&"1;"), "the file inside is used");
        assert!(!texts.iter().any(|text| text.contains("stolen")), "{texts:?}");
        assert!(options_page(&entry).is_none(), "an options page outside the folder is not offered");
    }

    #[test]
    fn here_names_only_the_extensions_that_act_on_an_address() {
        let temp = Temp::new("here");
        let wide = write_folder(
            temp.path(),
            "wide",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Wide",
                "version": "1.0",
                "content_scripts": [{ "matches": ["*://*.example.com/*"], "js": ["a.js"] }]
            }),
            &[("a.js", "1;")],
        );
        let narrow = write_folder(
            temp.path(),
            "narrow",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Narrow",
                "version": "1.0",
                "content_scripts": [{ "matches": ["https://news.example.com/item/*"], "js": ["b.js"] }]
            }),
            &[("b.js", "2;")],
        );
        let idle = write_folder(
            temp.path(),
            "idle",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Idle",
                "version": "1.0",
                "content_scripts": [{ "matches": ["*://other.test/*"], "js": ["c.js"] }]
            }),
            &[("c.js", "3;")],
        );
        install_at(temp.path(), &wide).expect("wide");
        install_at(temp.path(), &narrow).expect("narrow");
        install_at(temp.path(), &idle).expect("idle");

        let mut at_news = here_at(temp.path(), "https://news.example.com/item/1");
        at_news.sort();
        assert_eq!(at_news, vec!["Narrow".to_string(), "Wide".to_string()]);
        assert_eq!(here_at(temp.path(), "https://example.com/"), vec!["Wide".to_string()]);
        assert!(here_at(temp.path(), "https://notexample.com/").is_empty());
        assert!(here_at(temp.path(), "https://example.com.evil.net/").is_empty());
        assert!(here_at(temp.path(), "about:blank").is_empty());
    }

    #[test]
    fn the_engine_is_told_what_the_pattern_means() {
        // A star scheme is http and https only: WebKit would read it as any
        // scheme, and put the extension on local files it never asked for.
        let all = allow_list("*://*/*");
        assert_eq!(all, vec!["http://*/*".to_string(), "https://*/*".to_string()]);
        assert!(!all.iter().any(|p| p.starts_with("*://")), "{all:?}");
        // <all_urls> names file: as well, which is what "all" says.
        let every = allow_list("<all_urls>");
        assert!(every.contains(&"file:///*".to_string()), "{every:?}");
        assert!(every.contains(&"http://*/*".to_string()), "{every:?}");
        assert!(every.contains(&"https://*/*".to_string()), "{every:?}");
        // The base domain is named, because the engine's wildcard host means a
        // subdomain and would leave example.com itself alone.
        let subdomains = allow_list("*://*.example.com/*");
        assert!(subdomains.contains(&"https://example.com/*".to_string()), "{subdomains:?}");
        assert!(subdomains.contains(&"https://*.example.com/*".to_string()), "{subdomains:?}");
        assert!(subdomains.contains(&"http://example.com/*".to_string()), "{subdomains:?}");
        // A pattern with no reading at all is handed on as nothing, never as
        // "everything".
        assert!(allow_list("example.com/*").is_empty());
        assert!(allow_list("https://example.com:8080/*").is_empty());
        // An ordinary one is passed as itself.
        assert_eq!(allow_list("https://example.com/path/*"), vec!["https://example.com/path/*".to_string()]);
        assert_eq!(allow_list("file:///*"), vec!["file:///*".to_string()]);
    }

    #[test]
    fn an_extension_that_needs_a_background_page_is_told_so() {
        let manifest = Manifest {
            name: "Background only".into(),
            background: Some(serde_json::json!({ "service_worker": "bg.js" })),
            ..Default::default()
        };
        let (runs, _partial, why) = capability(&manifest, Path::new(""));
        assert!(!runs);
        assert!(why.contains("background"), "{why}");
        // The reason names the missing API rather than only saying "no".
        assert!(why.contains("WebExtensions API"), "{why}");
        assert!(why.contains("chrome.tabs"), "{why}");
    }

    #[test]
    fn an_extension_with_both_halves_says_which_half_does_not_run() {
        let manifest = Manifest {
            name: "Both".into(),
            background: Some(serde_json::json!({ "service_worker": "bg.js" })),
            content_scripts: vec![ContentScript {
                matches: vec!["<all_urls>".into()],
                js: vec!["content.js".into()],
                ..Default::default()
            }],
            permissions: vec!["tabs".into(), "storage".into()],
            ..Default::default()
        };
        let (runs, _partial, why) = capability(&manifest, Path::new(""));
        assert!(runs, "the content half runs");
        assert!(why.contains("content scripts"), "{why}");
        assert!(why.contains("background page never starts"), "{why}");
        // The permission it cannot have is named, and a permission it can
        // ignore is not treated as fatal.
        assert!(why.contains("reading your open tabs"), "{why}");
        assert!(!why.contains("storage"), "{why}");
        assert!(why.ends_with('.'), "{why}");
    }

    #[test]
    fn a_content_script_extension_runs() {
        let manifest = Manifest {
            name: "Content".into(),
            content_scripts: vec![ContentScript {
                matches: vec!["<all_urls>".into()],
                js: vec!["content.js".into()],
                ..Default::default()
            }],
            ..Default::default()
        };
        let (runs, _partial, why) = capability(&manifest, Path::new(""));
        assert!(runs, "{why}");
        // Nothing to apologise for, so nothing is said.
        assert!(why.is_empty(), "{why}");
    }

    #[test]
    fn an_extension_with_nothing_runnable_is_told_why() {
        let manifest = Manifest { name: "Empty".into(), ..Default::default() };
        let (runs, _partial, why) = capability(&manifest, Path::new(""));
        assert!(!runs);
        assert!(why.contains("no content scripts"), "{why}");
    }

    #[test]
    fn not_drawn_says_only_what_is_true() {
        let temp = Temp::new("not-drawn");
        let folder = write_folder(
            temp.path(),
            "with-ui",
            serde_json::json!({
                "manifest_version": 3,
                "name": "With UI",
                "version": "1.0",
                "action": { "default_popup": "popup.html" },
                "host_permissions": ["*://example.com/*", "*://other.test/*"],
                "content_scripts": [{ "matches": ["*://example.com/*"], "js": ["a.js"] }]
            }),
            &[("a.js", "1;")],
        );
        let entry = install_at(temp.path(), &folder).expect("an install");
        let notes = not_drawn(&entry);
        assert!(notes.iter().any(|note| note.contains("toolbar")), "{notes:?}");
        assert!(
            notes.iter().any(|note| note.contains("2 address patterns")),
            "{notes:?}"
        );
        // It does not claim the extension runs where the host permissions say.
        assert!(!notes.iter().any(|note| note.contains("runs on")), "{notes:?}");
    }

    #[test]
    fn a_content_script_that_calls_a_browser_api_is_partial() {
        let temp = Temp::new("partial-api");
        let folder = write_folder(
            temp.path(),
            "api-user",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Bitwarden-like",
                "version": "1.0",
                "content_scripts": [{ "matches": ["<all_urls>"], "js": ["cs.js"] }]
            }),
            &[("cs.js", "browser.runtime.sendMessage({ x: 1 }); const u = 'chrome.google.com';")],
        );
        let (runs, _partial, why) = capability(&parse_manifest(&folder).expect("a manifest"), &folder);
        assert!(runs, "{why}");
        assert!(why.contains("loads, but its main part does not run here"), "{why}");
        // A plain content script with no API call is not partial.
        std::fs::write(folder.join("cs.js"), "document.title = 'x';").expect("a file");
        let (_, _, plain) = capability(&parse_manifest(&folder).expect("a manifest"), &folder);
        assert!(plain.is_empty(), "{plain}");
    }

    /// Only a real call marks an add-on partial. Comments, sentences that end
    /// on the word, and web addresses are not calls, and a URL earlier on a
    /// minified line does not hide a call after it.
    #[test]
    fn only_a_real_api_call_counts_not_a_comment_a_sentence_or_an_address() {
        // Not calls.
        assert!(!calls_api("// uses browser.runtime to talk to the page"));
        assert!(!calls_api("/* chrome.storage is not used here */"));
        assert!(!calls_api(" * browser.runtime is described below"));
        assert!(!calls_api("x = 1; // see chrome.tabs in the docs"));
        assert!(!calls_api("const store = 'https://chrome.google.com/webstore';"));
        assert!(!calls_api("label.textContent = 'Please tell the browser. Then reload';"));
        assert!(!calls_api("document.title = 'plain';"));
        // Calls.
        assert!(calls_api("chrome.storage.local.get('k', done);"));
        assert!(calls_api("  browser.runtime.sendMessage({ kind: 'hello' });"));
        assert!(calls_api(
            "fetch('https://a.example/x');browser.runtime.sendMessage(1)"
        ));
        // A call on a later line is still found after a comment line.
        assert!(calls_api("// header\nchrome.runtime.connect();"));
    }

    /// exclude_matches keeps a script off the sites it names: they go to the
    /// engine as a block list, expanded the same way as the allow list, and a
    /// script that excludes nothing gets an empty block list.
    #[test]
    fn excluded_sites_are_handed_on_as_a_block_list() {
        let temp = Temp::new("exclude");
        let folder = write_folder(
            temp.path(),
            "exclude",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Everywhere but admin",
                "version": "1.0",
                "content_scripts": [
                    {
                        "matches": ["*://*.example.com/*"],
                        "exclude_matches": ["*://admin.example.com/*"],
                        "js": ["a.js"]
                    },
                    { "matches": ["https://other.example/*"], "js": ["b.js"] }
                ]
            }),
            &[("a.js", "1;"), ("b.js", "2;")],
        );
        install_at(temp.path(), &folder).expect("an install");
        let scripts = installable_at(temp.path()).scripts;
        let (_, allow, block, _, _) = scripts
            .iter()
            .find(|(text, ..)| text == "1;")
            .expect("the first script is handed on");
        assert!(allow.contains(&"https://*.example.com/*".to_string()), "{allow:?}");
        assert!(block.contains(&"http://admin.example.com/*".to_string()), "{block:?}");
        assert!(block.contains(&"https://admin.example.com/*".to_string()), "{block:?}");
        let (_, _, plain_block, _, _) = scripts
            .iter()
            .find(|(text, ..)| text == "2;")
            .expect("the second script is handed on");
        assert!(plain_block.is_empty(), "{plain_block:?}");
    }

    #[test]
    fn a_background_page_makes_an_extension_partial_and_the_card_says_so() {
        let temp = Temp::new("partial-bg");
        let folder = write_folder(
            temp.path(),
            "bg-user",
            serde_json::json!({
                "manifest_version": 3,
                "name": "Dark Reader-like",
                "version": "4.9",
                "background": { "service_worker": "bg.js" },
                "content_scripts": [{ "matches": ["<all_urls>"], "js": ["cs.js"] }]
            }),
            &[("cs.js", "1;")],
        );
        let (runs, partial, why) = capability(&parse_manifest(&folder).expect("a manifest"), &folder);
        assert!(runs && partial, "{why}");
        assert!(why.starts_with("loads, but its main part does not run here"), "{why}");
    }

    #[test]
    fn a_manifest_with_a_trailing_comma_says_where_it_failed() {
        let temp = Temp::new("bad-json");
        let folder = temp.path().join("broken");
        std::fs::create_dir_all(&folder).expect("a folder");
        std::fs::write(folder.join("manifest.json"), "{ \"name\": \"Bad\", }").expect("a manifest");
        let why = parse_manifest(&folder).expect_err("a broken manifest is refused");
        assert!(why.starts_with("manifest.json does not parse:"), "{why}");
        assert!(!why.contains("has no readable"), "{why}");
    }

    #[test]
    fn a_manifest_with_an_odd_user_scripts_still_installs() {
        let temp = Temp::new("odd-user-scripts");
        let folder = write_folder(
            temp.path(),
            "odd",
            serde_json::json!({
                "manifest_version": 2,
                "name": "Odd",
                "version": "1.0",
                "user_scripts": ["not", "a", "map"]
            }),
            &[],
        );
        assert!(install_at(temp.path(), &folder).is_ok());
    }

    #[test]
    fn a_name_in_a_locale_is_resolved_from_the_default_locale() {
        let temp = Temp::new("locale");
        let folder = write_folder(
            temp.path(),
            "bitwarden-like",
            serde_json::json!({
                "manifest_version": 3,
                "name": "__MSG_extName__",
                "version": "2024.1",
                "description": "__MSG_extDesc__",
                "default_locale": "en"
            }),
            &[(
                "_locales/en/messages.json",
                r#"{ "extName": { "message": "Bitwarden" }, "extDesc": { "message": "A password manager" } }"#,
            )],
        );
        let entry = install_at(temp.path(), &folder).expect("an install");
        assert_eq!(entry.name, "Bitwarden");
        assert_eq!(entry.description, "A password manager");
    }

    #[test]
    fn an_unknown_locale_key_is_left_as_it_is() {
        let temp = Temp::new("locale-missing");
        let folder = write_folder(
            temp.path(),
            "uBlock-like",
            serde_json::json!({
                "manifest_version": 3,
                "name": "__MSG_extName__",
                "version": "1.0",
                "default_locale": "en"
            }),
            &[],
        );
        let manifest = parse_manifest(&folder).expect("a manifest");
        assert_eq!(manifest.name, "__MSG_extName__");
    }

    #[test]
    fn a_firefox_add_on_gets_one_id_whatever_its_name() {
        let first = Manifest {
            name: "uBlock Origin".into(),
            version: "1.76.6".into(),
            browser_specific_settings: Some(serde_json::json!({ "gecko": { "id": "uBlock0@raymondhill.net" } })),
            ..Default::default()
        };
        let renamed = Manifest { name: "uBlock Origin (Beta)".into(), version: "1.77".into(), ..first.clone() };
        let origin = Path::new("/dl/x.xpi");
        assert_eq!(id_for(&first, origin), id_for(&renamed, origin));
        assert!(!id_for(&first, origin).contains("ublock"), "{}", id_for(&first, origin));
    }

    #[test]
    fn a_firefox_add_on_under_an_older_id_is_replaced_not_duplicated() {
        let temp = Temp::new("gecko-migrate");
        let addon = serde_json::json!({
            "manifest_version": 2,
            "name": "uBlock Origin",
            "version": "1.76.6",
            "browser_specific_settings": { "gecko": { "id": "uBlock0@raymondhill.net" } }
        });
        let folder = write_folder(temp.path(), "ublock", addon.clone(), &[("a.js", "1;")]);
        // An entry made before the gecko id was kept: its own id, no gecko_id.
        let old_folder = root_at(temp.path()).join("ublock-old");
        std::fs::create_dir_all(&old_folder).expect("the old folder");
        std::fs::write(old_folder.join("manifest.json"), addon.to_string()).expect("a manifest");
        let mut registry = Registry::default();
        registry.extensions.push(Installed {
            id: "ublock-old".to_string(),
            name: "uBlock Origin".to_string(),
            version: "1.76.5".to_string(),
            description: String::new(),
            path: old_folder.to_string_lossy().to_string(),
            source: "elsewhere".to_string(),
            runs: true,
            partial: false,
            why_not: String::new(),
            enabled: true,
            gecko_id: None,
        });
        save_at(temp.path(), &registry);
        let offer = inspect_at(temp.path(), &folder).expect("an inspect");
        assert!(offer.replaces.is_some(), "the old copy is named as replaced");
        install_at(temp.path(), &folder).expect("an install");
        let registry = registry_at(temp.path());
        assert_eq!(registry.extensions.len(), 1, "one uBlock Origin, not two");
        assert!(!old_folder.exists(), "the old folder is gone");
    }

    #[test]
    fn a_style_sheet_with_a_template_or_a_backslash_is_escaped() {
        let script = css_script("a::before { content: '${x}'; } b { c: `d`; }\\");
        assert!(script.contains("\\${x}"), "{script}");
        assert!(script.contains("\\`d\\`"), "{script}");
        assert!(script.contains("\\\\\\\\") || script.contains("\\\\"), "{script}");
        // A real hash, so two sheets with the same byte total do not share an id.
        let a = css_script("a{color:red}");
        let b = css_script("a{color:blue}");
        assert_ne!(a, b);
        assert_ne!(
            a.split("__search_ext_style_").nth(1).and_then(|s| s.split('\'').next()),
            b.split("__search_ext_style_").nth(1).and_then(|s| s.split('\'').next())
        );
    }

    #[test]
    fn unique_temp_creates_owner_only_directory() {
        use std::os::unix::fs::PermissionsExt;
        let dir = unique_temp("search-ext-test").expect("temp dir");
        let mode = std::fs::metadata(&dir).expect("meta").permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let _ = std::fs::remove_dir(&dir);
    }

}

