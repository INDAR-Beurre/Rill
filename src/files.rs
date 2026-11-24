// Small JSON files in the app's own folders, as the Mac build keeps them.
//
// Nothing here leaves the machine and nothing is precious: a file that will
// not parse is a file this app starts over from.
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Distinguishes two writes that land in the same nanosecond. Two tabs saving
/// history at once used to share one `.tmp` name and clobber each other's text.
static WRITE_COUNT: AtomicU64 = AtomicU64::new(0);

fn current_uid() -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata("/proc/self").ok().map(|m| m.uid())
}

fn user_fallback_base() -> PathBuf {
    if let Some(uid) = current_uid() {
        return PathBuf::from(format!("/tmp/search-{uid}"));
    }
    let raw = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| format!("pid-{}", std::process::id()));
    let clean: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(48)
        .collect();
    let tag = if clean.is_empty() {
        format!("pid-{}", std::process::id())
    } else {
        clean
    };
    PathBuf::from(format!("/tmp/search-{tag}"))
}

fn config_base_from(xdg: Option<&std::ffi::OsStr>, home: Option<&std::ffi::OsStr>) -> PathBuf {
    xdg.filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|s| !s.is_empty())
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .unwrap_or_else(user_fallback_base)
}

fn data_base_from(xdg: Option<&std::ffi::OsStr>, home: Option<&std::ffi::OsStr>) -> PathBuf {
    xdg.filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            home.filter(|s| !s.is_empty())
                .map(|h| PathBuf::from(h).join(".local/share"))
        })
        .unwrap_or_else(user_fallback_base)
}

pub fn config_dir() -> PathBuf {
    let xdg = std::env::var_os("XDG_CONFIG_HOME");
    let home = std::env::var_os("HOME");
    let base = config_base_from(xdg.as_deref(), home.as_deref());
    let dir = base.join("search");
    let _ = private_dir(&dir);
    dir
}

pub fn data_dir() -> PathBuf {
    let xdg = std::env::var_os("XDG_DATA_HOME");
    let home = std::env::var_os("HOME");
    let base = data_base_from(xdg.as_deref(), home.as_deref());
    let dir = base.join("search");
    let _ = private_dir(&dir);
    dir
}
pub fn path_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

/// The largest record file read back from disk. Every file the browser writes
/// for itself stays far below this. Anything bigger was hand-edited, damaged
/// or planted, and reading it whole would put all of it in memory before
/// parsing even starts. A file over the cap is treated as missing, the same as
/// one that doesn't parse.
const READ_JSON_LIMIT: u64 = 32 * 1024 * 1024;

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    use std::io::Read;

    // Read through `take` instead of checking the size first, so a file that
    // grows between the check and the read still can't go over the cap. One
    // extra byte shows whether the file was longer than the limit.
    let file = fs::File::open(path).ok()?;
    let mut text = String::new();
    file.take(READ_JSON_LIMIT + 1).read_to_string(&mut text).ok()?;
    if text.len() as u64 > READ_JSON_LIMIT {
        return None;
    }
    serde_json::from_str(&text).ok()
}

/// Write through a temporary file so a reader never sees half a file.
///
/// The record is owner-only before it lands: these files hold every page
/// visited, and the default umask would leave them readable by other users.
///
/// Returns true only when the record reached the disk. The save timer uses
/// this to keep a file marked dirty after a failed write (a full disk, a
/// read-only config folder), so the next timer tries again and the change
/// is not silently lost. Callers that don't care can ignore the result.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> bool {
    match serde_json::to_string(value) {
        Ok(text) => write_private(path, text.as_bytes()).is_ok(),
        Err(_) => false,
    }
}

/// The newest snapshot number written to each file through
/// `write_json_ordered`. The same lock covers the write itself, so only one
/// ordered writer works at a time, and a file never goes back to older data.
static LATEST_WRITE: std::sync::Mutex<Vec<(PathBuf, u64)>> = std::sync::Mutex::new(Vec::new());
static NEXT_SNAPSHOT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Numbers a snapshot. Call it on the main thread at the moment the data is
/// copied out of the app's state, so the numbers follow the order in which
/// the state actually changed.
pub fn snapshot_number() -> u64 {
    NEXT_SNAPSHOT.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1
}

/// Writes `value` to `path` unless a newer snapshot already reached that file.
///
/// It is safe to call from a worker thread. A background save of an older
/// snapshot that finishes late is skipped instead of renamed over newer data
/// (the synchronous save at window close, for one). A skip returns true:
/// what is on disk is newer than what this call carried, so nothing was lost
/// and the caller must not mark the file dirty again.
pub fn write_json_ordered<T: Serialize>(path: &Path, value: &T, snapshot: u64) -> bool {
    let Ok(text) = serde_json::to_string(value) else { return false };
    let mut latest = LATEST_WRITE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if latest.iter().any(|(written, seen)| written == path && *seen >= snapshot) {
        return true;
    }
    let landed = write_private(path, text.as_bytes()).is_ok();
    if landed {
        match latest.iter_mut().find(|(written, _)| written == path) {
            Some(entry) => entry.1 = snapshot,
            None => latest.push((path.to_path_buf(), snapshot)),
        }
    }
    landed
}

#[cfg(test)]
mod ordered_write_tests {
    use super::{snapshot_number, write_json_ordered};

    fn scratch(name: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir()
            .join(format!("search-ordered-{}-{nanos}", std::process::id()))
            .join(name)
    }

    fn read(path: &std::path::Path) -> Vec<String> {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    }

    #[test]
    fn a_late_older_snapshot_does_not_overwrite_a_newer_one() {
        let path = scratch("session.json");
        let older = snapshot_number();
        let newer = snapshot_number();
        assert!(write_json_ordered(&path, &vec!["newer"], newer));
        // The older one finishes last, as a slow background write would.
        assert!(write_json_ordered(&path, &vec!["older"], older));
        assert_eq!(read(&path), vec!["newer".to_string()]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn snapshots_written_in_order_all_land() {
        let path = scratch("history.json");
        for word in ["one", "two", "three"] {
            assert!(write_json_ordered(&path, &vec![word], snapshot_number()));
            assert_eq!(read(&path), vec![word.to_string()]);
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn ordering_is_kept_per_file() {
        let first = scratch("a.json");
        let second = first.with_file_name("b.json");
        let early = snapshot_number();
        assert!(write_json_ordered(&first, &vec!["late"], snapshot_number()));
        // A newer write to a.json says nothing about b.json.
        assert!(write_json_ordered(&second, &vec!["early"], early));
        assert_eq!(read(&second), vec!["early".to_string()]);
        let _ = std::fs::remove_dir_all(first.parent().unwrap());
    }
}

/// One place where a byte of the user's own data reaches the disk, so there is
/// one place that has to be right about permissions and about half-written
/// files.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

    if let Some(parent) = path.parent() {
        // A missing config folder is not a reason to fail the write, and a
        // folder the umask left at 0755 is a listing of what the user browsed.
        // An empty parent is a bare file name, which already names a folder.
        if !parent.as_os_str().is_empty() {
            private_dir(parent)?;
        }
    }

    let tmp = temp_path(path);
    // create_new, not create: a name left behind by a crashed run must not be
    // opened and shared. mode(0o600) applies at creation, before a byte of
    // history is in it.
    let opened = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp);
    let mut file = opened?;

    // sync_all before the rename: a rename can be durable before the contents
    // are, which leaves an empty file where the old one used to be.
    let written = file.write_all(bytes).and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = written {
        // Nothing is renamed over, so the file that was there is still there,
        // and the half-written temp file is not left to be mistaken for one.
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }

    // Belt and braces on top of mode(0o600) above: the same ensure the read
    // side relies on, so an older temp file cannot carry a looser mode in.
    let _ = fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600));
    if let Err(error) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(error);
    }
    Ok(())
}

/// The app's own folders are the user's business only.
///
/// Only directories this call creates are made 0700. A directory that was
/// already there is left exactly as it is: the parent of a config folder can be
/// the temp dir, and tightening that would lock every other user out of it.
fn private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, PermissionsExt};
    let mut missing = Vec::new();
    let mut cursor = Some(path);
    while let Some(each) = cursor {
        if each.symlink_metadata().is_ok() {
            break;
        }
        missing.push(each.to_path_buf());
        cursor = each.parent();
    }
    fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    // Newest last, so the whole chain this call made is owner-only.
    for each in missing {
        let _ = fs::set_permissions(&each, fs::Permissions::from_mode(0o700));
    }
    if let Ok(meta) = fs::symlink_metadata(path) {
        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "refusing symlinked private directory",
            ));
        }
        let is_shared_root = path == Path::new("/")
            || path == Path::new("/tmp")
            || path == std::env::temp_dir();
        let owned_by_us = current_uid().map_or(true, |uid| meta.uid() == uid);
        if meta.is_dir() && !is_shared_root && owned_by_us {
            let mode = meta.permissions().mode() & 0o777;
            if mode != 0o700 {
                fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
            }
        }
    }
    Ok(())
}

/// A temp name no other writer can pick: the file's own name, the process, the
/// nanosecond, and a counter for anything that lands in the same one.
fn temp_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "data".to_string());
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos())
        .unwrap_or(0);
    let count = WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
    let unique = format!("{name}.{}.{nanos}.{count}.tmp", std::process::id());
    match path.parent() {
        Some(parent) => parent.join(unique),
        None => PathBuf::from(unique),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A folder of this test's own under the temp dir, so nothing here can
    /// touch the real config or data folders.
    fn scratch(test: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("search-files-{}-{test}-{nanos}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("temp folder");
        dir
    }

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).expect("file exists").permissions().mode() & 0o777
    }

    #[test]
    fn a_small_json_value_round_trips() {
        let dir = scratch("round-trip");
        let path = dir.join("hidden.json");
        let mut value = std::collections::BTreeMap::new();
        value.insert("example.com".to_string(), vec!["#ad".to_string()]);

        write_json(&path, &value);
        let back: std::collections::BTreeMap<String, Vec<String>> =
            read_json(&path).expect("written file parses");

        assert_eq!(back, value);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_written_file_is_owner_only() {
        let dir = scratch("mode");
        let path = dir.join("history.json");

        write_json(&path, &vec!["https://example.com/".to_string()]);

        assert_eq!(mode_of(&path), 0o600, "history must not be world-readable");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn overwriting_an_existing_file_keeps_it_owner_only() {
        let dir = scratch("overwrite-mode");
        let path = dir.join("session.json");
        write_json(&path, &vec![1, 2]);
        // What an older build left behind: umask 0644.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");

        write_json(&path, &vec![3]);

        let back: Vec<i32> = read_json(&path).expect("parses");
        assert_eq!(back, vec![3]);
        assert_eq!(mode_of(&path), 0o600, "an overwrite must not inherit the old mode");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn two_writes_in_a_row_both_land() {
        let dir = scratch("twice");
        let path = dir.join("history.json");

        write_json(&path, &vec!["first"]);
        write_json(&path, &vec!["second"]);

        let back: Vec<String> = read_json(&path).expect("parses");
        assert_eq!(back, vec!["second"]);
        // No temp file survives a successful write, whatever it was called.
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .expect("listing")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_write_into_a_missing_nested_folder_creates_it() {
        let dir = scratch("nested");
        let path = dir.join("one").join("two").join("bookmarks.json");

        write_json(&path, &vec!["https://example.com/"]);

        assert_eq!(read_json::<Vec<String>>(&path).map(|v| v.len()), Some(1));
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(mode_of(&path.parent().expect("parent")), 0o700);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_write_that_cannot_land_does_not_touch_the_old_file() {
        let dir = scratch("failure");
        // A regular file where a folder would have to be: the temp file cannot
        // be created at all, so there is nothing to rename over anything.
        let blocker = dir.join("blocker");
        fs::write(&blocker, b"not a folder").expect("file");
        let path = blocker.join("history.json");

        assert!(write_private(&path, b"[]").is_err(), "the write has to report failure");
        assert_eq!(fs::read(&blocker).expect("readable"), b"not a folder");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_rename_that_fails_leaves_no_temp_file() {
        let dir = scratch("rename-fails");
        // A non-empty folder sitting on the name: the rename must fail, and the
        // temp file written next to it must not be left lying around.
        let path = dir.join("history.json");
        fs::create_dir_all(&path).expect("dir");
        fs::write(path.join("inside"), b"x").expect("file");

        assert!(write_private(&path, b"[]").is_err());
        assert!(path.is_dir(), "the folder the rename could not replace is still there");
        let leftovers: Vec<_> = fs::read_dir(&dir)
            .expect("listing")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "temp files left behind: {leftovers:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn empty_xdg_and_missing_home_fall_back_safely() {
        use std::ffi::OsStr;
        let expected_fallback = user_fallback_base();
        assert_ne!(expected_fallback, PathBuf::from("/tmp"));
        assert!(expected_fallback.to_string_lossy().starts_with("/tmp/search-"));

        // Empty XDG falls back to HOME/.config and HOME/.local/share.
        assert_eq!(
            config_base_from(Some(OsStr::new("")), Some(OsStr::new("/home/alice"))),
            PathBuf::from("/home/alice/.config")
        );
        assert_eq!(
            data_base_from(Some(OsStr::new("")), Some(OsStr::new("/home/alice"))),
            PathBuf::from("/home/alice/.local/share")
        );

        // Empty or unset HOME falls back to the user-scoped /tmp/search-<uid> folder.
        assert_eq!(config_base_from(None, None), expected_fallback);
        assert_eq!(config_base_from(Some(OsStr::new("")), Some(OsStr::new(""))), expected_fallback);
        assert_eq!(data_base_from(None, None), expected_fallback);
        assert_eq!(data_base_from(Some(OsStr::new("")), Some(OsStr::new(""))), expected_fallback);
    }

    #[test]
    fn private_dir_tightens_existing_loose_directory_to_0700() {
        let dir = scratch("loose-dir");
        let target = dir.join("search");
        fs::create_dir_all(&target).expect("dir");
        fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(mode_of(&target), 0o755);

        private_dir(&target).expect("private_dir");
        assert_eq!(mode_of(&target), 0o700);
        let _ = fs::remove_dir_all(&dir);
    }
}
