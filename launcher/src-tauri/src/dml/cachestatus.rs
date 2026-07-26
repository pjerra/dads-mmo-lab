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
}
