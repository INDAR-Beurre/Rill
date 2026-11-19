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

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = base.join("search");
    let _ = private_dir(&dir);
    dir
}

pub fn data_dir() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = base.join("search");
    let _ = private_dir(&dir);
    dir
}
pub fn path_in(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Write through a temporary file so a reader never sees half a file.
///
/// The record is owner-only before it lands: these files hold every page
/// visited, and the default umask would leave them readable by other users.
pub fn write_json<T: Serialize>(path: &Path, value: &T) {
    if let Ok(text) = serde_json::to_string(value) {
        let _ = write_private(path, text.as_bytes());
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
    use std::os::unix::fs::PermissionsExt;
    let mut missing = Vec::new();
    let mut cursor = Some(path);
    while let Some(each) = cursor {
        if each.exists() {
            break;
        }
        missing.push(each.to_path_buf());
        cursor = each.parent();
    }
    fs::create_dir_all(path)?;
    // Newest last, so the whole chain this call made is owner-only.
    for each in missing {
        let _ = fs::set_permissions(&each, fs::Permissions::from_mode(0o700));
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
}
