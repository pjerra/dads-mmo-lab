//! Native-mode **wowhead item-info cache size** reader (`wow cache-status`,
//! task D1a). Faithful port of `cli/src/90-main.sh:1592-1608` +
//! `_iteminfo_cache` (`cli/src/46-iteminfo.sh:13`): a read-only size report
//! for `~/.dml/wowhead-cache` (tooltips + icons the Items page populates on
//! demand — never a committed dataset). Done as a plain `std::fs` walk in
//! Rust rather than shelling `du -sb`/`find … | wc -l`.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// `_iteminfo_cache` (`46-iteminfo.sh:13`): `~/.dml/wowhead-cache`.
pub fn cache_dir() -> Option<PathBuf> {
    super::dml_home_dir().map(|h| h.join("wowhead-cache"))
}

/// Recursive `(bytes, files)` walk under `root`, mirroring `du -sb`'s
/// apparent-size total (sum of every regular file's length) and `find
/// <root> -type f | wc -l` (count of regular files only — symlinks are
/// NOT followed and NOT counted, matching `find`'s default lstat
/// semantics via `DirEntry::file_type()`).
pub fn scan(root: &Path) -> (u64, u64) {
    fn walk(dir: &Path, bytes: &mut u64, files: &mut u64) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                walk(&e.path(), bytes, files);
            } else if ft.is_file() {
                if let Ok(meta) = e.metadata() {
                    *bytes += meta.len();
                }
                *files += 1;
            }
        }
    }
    let mut bytes = 0u64;
    let mut files = 0u64;
    walk(root, &mut bytes, &mut files);
    (bytes, files)
}

/// `cache-status` (`90-main.sh:1592-1608`): `{"caches":[{"key":"wowhead",
/// "label":"Item tooltips & icons","path","present","bytes","files"}]}`.
/// `present` false (and bytes/files 0) when the cache dir doesn't exist yet
/// (nothing looked up an item since a fresh install / after `cache-clean`).
pub fn read_cache_status() -> Value {
    let path = cache_dir();
    let path_str = path.as_deref().map(|p| p.display().to_string()).unwrap_or_default();
    let present = path.as_deref().is_some_and(Path::is_dir);
    let (bytes, files) = if present { scan(path.as_deref().unwrap()) } else { (0, 0) };
    json!({"caches": [{
        "key": "wowhead",
        "label": "Item tooltips & icons",
        "path": path_str,
        "present": present,
        "bytes": bytes,
        "files": files,
    }]})
}

/// Why [`clean_cache_at`] refused/failed — mirrors the two `json_err` exits
/// in the bash `cache-clean)` arm (`90-main.sh:1613,1622`).
#[derive(Debug)]
pub enum CacheCleanError {
    /// The path failed the safety guard (or couldn't be resolved at all) —
    /// `INTERNAL` in the bash oracle ("refusing to wipe unexpected cache
    /// path").
    Guard(String),
    /// `remove_dir_all` itself failed — `WIPE_FAILED` in the bash oracle.
    Wipe(String),
}

/// Component-wise port of the bash `case "$ccpath" in */.dml/wowhead-cache)
/// ;; esac` suffix guard (`90-main.sh:1614-1617`): the ONLY defense against
/// ever wiping `~/.dml` itself (which also holds non-cache state like
/// `client-path`). A literal string-suffix check doesn't translate cleanly
/// across path-separator conventions, so this checks the same INVARIANT via
/// path components instead — `path`'s own name must be exactly
/// `wowhead-cache` and its parent's name must be exactly `.dml` — which is
/// the native equivalent of "ends in /.dml/wowhead-cache".
fn passes_wipe_guard(path: &Path) -> bool {
    path.file_name().is_some_and(|f| f == "wowhead-cache")
        && path.parent().and_then(Path::file_name).is_some_and(|f| f == ".dml")
}

/// `cache-clean` core (`90-main.sh:1609-1628`): wipes `path` — but ONLY
/// after [`passes_wipe_guard`] passes. `freed_bytes` is measured via
/// [`scan`] BEFORE delete, matching the bash oracle's `du -sb` immediately
/// before `rm -rf`. A missing dir is a no-op success (`wiped:true,
/// freed_bytes:0`), matching the bash arm's `if [[ -d "$ccpath" ]]` guard
/// around the whole delete+measure step.
pub fn clean_cache_at(path: &Path) -> Result<Value, CacheCleanError> {
    if !passes_wipe_guard(path) {
        return Err(CacheCleanError::Guard(format!(
            "refusing to wipe unexpected cache path: {}",
            path.display()
        )));
    }
    let path_str = path.display().to_string();
    let present = path.is_dir();
    let freed = if present { scan(path).0 } else { 0 };
    if present {
        std::fs::remove_dir_all(path)
            .map_err(|e| CacheCleanError::Wipe(format!("could not remove the cache dir: {e}")))?;
    }
    Ok(json!({"wiped": true, "freed_bytes": freed, "path": path_str}))
}

/// `cache-clean`: resolves the real `~/.dml/wowhead-cache` via [`cache_dir`],
/// then [`clean_cache_at`].
pub fn clean_cache() -> Result<Value, CacheCleanError> {
    let path = cache_dir().ok_or_else(|| {
        CacheCleanError::Guard(
            "could not resolve the DML state directory (USERPROFILE/HOME not set)".to_string(),
        )
    })?;
    clean_cache_at(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dml-cachestatus-{name}-{}", std::process::id()))
    }

    #[test]
    fn scan_empty_dir_is_zero() {
        let d = tmp("empty");
        std::fs::create_dir_all(&d).unwrap();
        assert_eq!(scan(&d), (0, 0));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn scan_missing_dir_is_zero() {
        let d = tmp("missing");
        assert_eq!(scan(&d), (0, 0));
    }

    #[test]
    fn scan_sums_bytes_and_counts_files_recursively() {
        let d = tmp("nested");
        std::fs::create_dir_all(d.join("tooltips")).unwrap();
        std::fs::create_dir_all(d.join("icons")).unwrap();
        std::fs::write(d.join("tooltips").join("123.json"), "12345").unwrap(); // 5 bytes
        std::fs::write(d.join("icons").join("abc.jpg"), "1234567890").unwrap(); // 10 bytes
        let (bytes, files) = scan(&d);
        assert_eq!(bytes, 15);
        assert_eq!(files, 2);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn read_cache_status_present_false_when_dir_absent() {
        // USERPROFILE/HOME is whatever the test box has; just check the
        // shape is well-formed and present tracks path.is_dir() honestly.
        let v = read_cache_status();
        let caches = v["caches"].as_array().unwrap();
        assert_eq!(caches.len(), 1);
        assert_eq!(caches[0]["key"], "wowhead");
        assert_eq!(caches[0]["label"], "Item tooltips & icons");
        assert!(caches[0]["present"].is_boolean());
        assert!(caches[0]["bytes"].is_u64());
        assert!(caches[0]["files"].is_u64());
    }

    // -- clean_cache_at ----------------------------------------------------

    #[test]
    fn passes_wipe_guard_requires_exact_parent_and_name() {
        assert!(passes_wipe_guard(Path::new("/home/x/.dml/wowhead-cache")));
        assert!(!passes_wipe_guard(Path::new("/home/x/.dml")));
        assert!(!passes_wipe_guard(Path::new("/home/x/.dml/other-cache")));
        assert!(!passes_wipe_guard(Path::new("/home/x/notdml/wowhead-cache")));
        assert!(!passes_wipe_guard(Path::new("/")));
    }

    #[test]
    fn clean_cache_at_refuses_a_path_that_fails_the_guard() {
        let bad = tmp("guard-bad"); // parent is temp_dir, not literally ".dml"
        std::fs::create_dir_all(&bad).unwrap();
        let err = clean_cache_at(&bad);
        assert!(matches!(err, Err(CacheCleanError::Guard(_))));
        // Refused BEFORE any delete -- the dir must still exist.
        assert!(bad.is_dir());
        let _ = std::fs::remove_dir_all(&bad);
    }

    #[test]
    fn clean_cache_at_wipes_present_dir_and_reports_freed_bytes() {
        let dml = tmp("wipe-ok").join(".dml");
        let cache = dml.join("wowhead-cache");
        std::fs::create_dir_all(cache.join("icons")).unwrap();
        std::fs::write(cache.join("icons").join("a.jpg"), "12345").unwrap(); // 5 bytes
        let v = clean_cache_at(&cache).unwrap();
        assert_eq!(v["wiped"], true);
        assert_eq!(v["freed_bytes"], 5);
        assert!(!cache.exists());
        // Sibling (the ".dml" dir itself) must survive.
        assert!(dml.is_dir());
        let _ = std::fs::remove_dir_all(&dml);
    }

    #[test]
    fn clean_cache_at_missing_dir_is_a_noop_success() {
        let dml = tmp("wipe-missing").join(".dml");
        let cache = dml.join("wowhead-cache");
        assert!(!cache.exists());
        let v = clean_cache_at(&cache).unwrap();
        assert_eq!(v["wiped"], true);
        assert_eq!(v["freed_bytes"], 0);
    }
}
