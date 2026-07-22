use std::io::Read;
use std::sync::OnceLock;

/// Percent-decodes `%XX` hex-pair escapes in `s` to their raw bytes.
/// Invalid/incomplete escapes are left as literal characters. The result
/// is built from the decoded bytes lossily (invalid UTF-8 -> replacement).
fn percent_decode_lossy(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() && b[i + 1].is_ascii_hexdigit() && b[i + 2].is_ascii_hexdigit() {
            let hi = (b[i + 1] as char).to_digit(16).unwrap();
            let lo = (b[i + 2] as char).to_digit(16).unwrap();
            out.push((hi * 16 + lo) as u8);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Returns true if `path` fails any of the traversal/allowlist checks.
fn zam_path_is_rejected(path: &str) -> bool {
    if path.is_empty() || path.contains('\\') || path.contains("://") {
        return true;
    }
    if !(path.starts_with("modelviewer/") || path.starts_with("images/")) {
        return true;
    }
    if path.split('/').any(|seg| seg == ".." || seg.is_empty()) {
        return true;
    }
    false
}

/// Maps a zam-scheme request path to (upstream URL, cache-relative path).
/// Security: fixed host, prefix allowlist, no traversal, no embedded schemes.
/// Checks run against both the original (still-encoded) path and its
/// percent-decoded form, so a percent-encoded traversal segment (e.g.
/// `%2e%2e`) is rejected even though the raw string contains no literal
/// `..`. The returned URL/cache path are built from the ORIGINAL path.
pub fn zam_map_path(raw: &str) -> Option<(String, String)> {
    let path = raw.trim_start_matches('/');
    let path = path.split('?').next().unwrap_or("");
    let decoded = percent_decode_lossy(path);
    if zam_path_is_rejected(path) || zam_path_is_rejected(&decoded) {
        return None;
    }
    Some((format!("https://wow.zamimg.com/{path}"), path.to_string()))
}

/// Cross-tree fallback (Warglaive fix, 2026-07-22): Wowhead's wrath content
/// tree has gaps -- metas (and the m2/skin/texture files they reference)
/// that exist on the sibling trees but 404 on wrath. Both Warglaives of
/// Azzinoth (wowhead display ids 45150/45146) live on tbc + cata but not
/// wrath, curl-verified. When a wrath-tree fetch misses upstream, the SAME
/// rest-path is retried on these trees in order; the winning bytes are
/// cached under the ORIGINAL wrath rel path so the engine's own follow-up
/// fetches hit the cache transparently. The tree names come ONLY from this
/// fixed table -- never from the request path (zam_map_path's allowlist and
/// traversal checks are unchanged and still run first).
const FALLBACK_TREES: [&str; 3] = ["tbc", "classic", "cata"];
const WRATH_TREE_PREFIX: &str = "modelviewer/wrath/";

/// Pure: the ordered upstream candidate URLs for one ALREADY-VALIDATED
/// cache-relative path (`zam_map_path`'s `rel`). Non-wrath paths (images/,
/// other trees) get exactly their own URL; wrath-tree paths append the
/// tbc -> classic -> cata variants of the same rest.
pub fn zam_candidate_urls(rel: &str) -> Vec<String> {
    let mut urls = vec![format!("https://wow.zamimg.com/{rel}")];
    if let Some(rest) = rel.strip_prefix(WRATH_TREE_PREFIX) {
        for tree in FALLBACK_TREES {
            urls.push(format!("https://wow.zamimg.com/modelviewer/{tree}/{rest}"));
        }
    }
    urls
}

pub fn content_type_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("") {
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client")
    })
}

/// One upstream fetch attempt's aggregate outcome across the candidate
/// list: first 2xx wins (Hit); all candidates answered definitively
/// non-success => Miss; otherwise (at least one transport failure and no
/// hit) => Err -- a fallback tree being unreachable can never be spun as
/// "this file does not exist".
enum UpstreamResult {
    Hit(Vec<u8>),
    Miss,
    Err,
}

/// Try each candidate URL in order (NO browser Origin header -- that is the
/// whole point of the proxy); first success returns its bytes.
fn fetch_first_hit(urls: &[String]) -> UpstreamResult {
    let mut saw_transport_err = false;
    for url in urls {
        let resp = match client().get(url).send() {
            Ok(r) => r,
            Err(_) => {
                saw_transport_err = true;
                continue;
            }
        };
        if !resp.status().is_success() {
            continue;
        }
        let mut bytes = Vec::new();
        if resp.take(64 * 1024 * 1024).read_to_end(&mut bytes).is_err() {
            saw_transport_err = true;
            continue;
        }
        return UpstreamResult::Hit(bytes);
    }
    if saw_transport_err {
        UpstreamResult::Err
    } else {
        UpstreamResult::Miss
    }
}

/// Atomic-ish cache write (tmp + rename); best-effort, failures ignored --
/// the caller already holds the bytes it is about to serve.
fn cache_store(cached: &std::path::Path, bytes: &[u8]) {
    if let Some(parent) = cached.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = cached.with_extension(format!(
        "{}.tmp",
        cached.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, cached);
    }
}

/// Serve one request: cache hit -> bytes; else fetch with cross-tree
/// fallback (zam_candidate_urls), cache the winner atomically UNDER THE
/// REQUESTED rel path, return. Any failure -> None (handler answers 404).
pub fn zam_serve(cache_root: &std::path::Path, raw_path: &str) -> Option<(Vec<u8>, &'static str)> {
    let (_url, rel) = zam_map_path(raw_path)?;
    let ct = content_type_for(&rel);
    let cached = cache_root.join(ZAM_CACHE_DIR).join(&rel);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Some((bytes, ct));
    }
    match fetch_first_hit(&zam_candidate_urls(&rel)) {
        UpstreamResult::Hit(bytes) => {
            cache_store(&cached, &bytes);
            Some((bytes, ct))
        }
        UpstreamResult::Miss | UpstreamResult::Err => None,
    }
}

/// Outcome of a native meta probe (see `zam_probe` in lib.rs): the WebView's
/// fetch() cannot be trusted to distinguish a clean upstream 404 from a
/// network failure for custom-scheme responses on Windows (WebView2 surfaces
/// non-2xx scheme responses as load errors), so the 3D item pre-flight asks
/// the Rust side instead. Hit also warms the shared cache for the engine.
pub enum ProbeOutcome {
    /// Meta exists; bytes returned (cached for the engine's own fetch).
    Hit(Vec<u8>),
    /// Upstream answered definitively: this meta does not exist.
    Miss,
    /// Transport-level failure -- unknown, caller should not treat as Miss.
    Err,
}

/// Like `zam_serve`, but three-valued: cache hit / any candidate 200 => Hit
/// (cached under the requested rel), ALL candidates definitively
/// non-success => Miss, any transport error without a hit => Err. Path
/// validation is identical to `zam_serve` (rejects map to Err -- an invalid
/// path can never claim an item is missing). Shares the same cross-tree
/// fallback as zam_serve, so a probe Hit on a fallback tree warms exactly
/// the cache entry the engine's own wrath-path fetch will read.
pub fn zam_probe_upstream(cache_root: &std::path::Path, raw_path: &str) -> ProbeOutcome {
    let Some((_url, rel)) = zam_map_path(raw_path) else {
        return ProbeOutcome::Err;
    };
    let cached = cache_root.join(ZAM_CACHE_DIR).join(&rel);
    if let Ok(bytes) = std::fs::read(&cached) {
        return ProbeOutcome::Hit(bytes);
    }
    match fetch_first_hit(&zam_candidate_urls(&rel)) {
        UpstreamResult::Hit(bytes) => {
            cache_store(&cached, &bytes);
            ProbeOutcome::Hit(bytes)
        }
        UpstreamResult::Miss => ProbeOutcome::Miss,
        UpstreamResult::Err => ProbeOutcome::Err,
    }
}

/// Pure: pull `Item.InventoryType` out of a meta JSON body (used by the 3D
/// pre-flight to distinguish shields/bows from generic held items). Any
/// parse failure -> None -- the caller degrades to its default slot.
pub fn parse_inventory_type(bytes: &[u8]) -> Option<i64> {
    let v: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    v.get("Item")?.get("InventoryType")?.as_i64()
}

/// The single runtime-cache subdir name under the app cache root. All the
/// zam-scheme model/icon bytes live under `<app_cache_dir>/zam-cache`; the
/// maintenance commands (Batch 6 C) only ever construct paths from this
/// constant, so a wipe can never be aimed at anything but the cache.
pub const ZAM_CACHE_DIR: &str = "zam-cache";

/// Recursively total the file sizes (bytes) and file count under `dir`.
/// A missing/unreadable dir yields (0, 0); unreadable subdirs are skipped
/// rather than aborting the walk. Symlinks are not followed (file_type on the
/// dir entry, not the target), so the count is of real files under the tree.
pub fn dir_report(dir: &std::path::Path) -> (u64, u64) {
    let mut bytes: u64 = 0;
    let mut files: u64 = 0;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let rd = match std::fs::read_dir(&d) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for ent in rd.flatten() {
            match ent.file_type() {
                Ok(ft) if ft.is_dir() => stack.push(ent.path()),
                Ok(ft) if ft.is_file() => {
                    if let Ok(m) = ent.metadata() {
                        bytes = bytes.saturating_add(m.len());
                        files += 1;
                    }
                }
                _ => {}
            }
        }
    }
    (bytes, files)
}

/// Size report for the zam runtime cache (`<cache_root>/zam-cache`).
pub fn zam_cache_report(cache_root: &std::path::Path) -> (u64, u64) {
    dir_report(&cache_root.join(ZAM_CACHE_DIR))
}

/// Delete `<cache_root>/zam-cache` entirely, returning the bytes freed
/// (measured before deletion; 0 if the dir was absent). The target path is
/// always built here from the fixed `ZAM_CACHE_DIR` name joined onto the
/// caller's cache root, so this can only ever remove the cache subdir --
/// never the cache root or anything outside it.
pub fn zam_cache_clear(cache_root: &std::path::Path) -> std::io::Result<u64> {
    let dir = cache_root.join(ZAM_CACHE_DIR);
    let (bytes, _) = dir_report(&dir);
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_allowlisted_paths() {
        let (url, rel) = zam_map_path("/modelviewer/wrath/viewer/viewer.min.js").unwrap();
        assert_eq!(url, "https://wow.zamimg.com/modelviewer/wrath/viewer/viewer.min.js");
        assert_eq!(rel, "modelviewer/wrath/viewer/viewer.min.js");
        assert!(zam_map_path("/images/wow/icons/large/inv_sword_39.jpg").is_some());
    }

    #[test]
    fn strips_query_strings() {
        let (url, _) = zam_map_path("/modelviewer/wrath/meta/armor/1/123.json?v=2").unwrap();
        assert!(!url.contains('?'));
    }

    #[test]
    fn rejects_bad_paths() {
        assert!(zam_map_path("/etc/passwd").is_none());
        assert!(zam_map_path("/modelviewer/../images/x.png").is_none());
        assert!(zam_map_path("/modelviewer//x").is_none());
        assert!(zam_map_path("/images/https://evil.example/x").is_none());
        assert!(zam_map_path("/modelviewer/wrath\\x").is_none());
        assert!(zam_map_path("/").is_none());
        assert!(zam_map_path("/other/x.js").is_none());
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type_for("a/b.js"), "application/javascript");
        assert_eq!(content_type_for("a/b.json"), "application/json");
        assert_eq!(content_type_for("a/b.mo3"), "application/octet-stream");
    }

    #[test]
    fn cache_hit_without_network() {
        let dir = std::env::temp_dir().join(format!("zamtest{}", std::process::id()));
        let rel = "modelviewer/wrath/meta/x.json";
        let f = dir.join("zam-cache").join(rel);
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, b"{\"cached\":true}").unwrap();
        let (bytes, ct) = zam_serve(&dir, "/modelviewer/wrath/meta/x.json").unwrap();
        assert_eq!(bytes, b"{\"cached\":true}");
        assert_eq!(ct, "application/json");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_percent_encoded_traversal() {
        assert!(zam_map_path("/modelviewer/%2e%2e/images/x.png").is_none());
        assert!(zam_map_path("/modelviewer/.%2e/x").is_none());
        assert!(zam_map_path("/modelviewer/%2E%2E/x").is_none());
        assert!(zam_map_path("/images/a%5cb.png").is_none());
        // benign encodings still pass
        assert!(zam_map_path("/modelviewer/wrath/meta/a%20b.json").is_some());
    }

    // --- Cross-tree fallback (Warglaive fix) -------------------------------

    #[test]
    fn candidate_urls_wrath_falls_back_tbc_classic_cata_in_order() {
        assert_eq!(
            zam_candidate_urls("modelviewer/wrath/meta/item/45150.json"),
            vec![
                "https://wow.zamimg.com/modelviewer/wrath/meta/item/45150.json",
                "https://wow.zamimg.com/modelviewer/tbc/meta/item/45150.json",
                "https://wow.zamimg.com/modelviewer/classic/meta/item/45150.json",
                "https://wow.zamimg.com/modelviewer/cata/meta/item/45150.json",
            ]
        );
    }

    #[test]
    fn candidate_urls_preserve_nested_rest_exactly() {
        let urls = zam_candidate_urls("modelviewer/wrath/textures/item/objectcomponents/weapon/x_01.webp");
        assert_eq!(urls.len(), 4);
        for (url, tree) in urls.iter().zip(["wrath", "tbc", "classic", "cata"]) {
            assert_eq!(
                url,
                &format!(
                    "https://wow.zamimg.com/modelviewer/{tree}/textures/item/objectcomponents/weapon/x_01.webp"
                )
            );
        }
    }

    #[test]
    fn candidate_urls_non_wrath_paths_get_no_fallback() {
        assert_eq!(
            zam_candidate_urls("images/wow/icons/large/inv_sword_39.jpg"),
            vec!["https://wow.zamimg.com/images/wow/icons/large/inv_sword_39.jpg"]
        );
        assert_eq!(
            zam_candidate_urls("modelviewer/live/viewer/viewer.min.js"),
            vec!["https://wow.zamimg.com/modelviewer/live/viewer/viewer.min.js"]
        );
        // A direct request for a sibling tree stays on that tree -- the
        // fallback ladder only ever extends wrath requests.
        assert_eq!(
            zam_candidate_urls("modelviewer/tbc/meta/item/45146.json"),
            vec!["https://wow.zamimg.com/modelviewer/tbc/meta/item/45146.json"]
        );
    }

    #[test]
    fn candidate_urls_wrath_prefix_is_segment_bounded() {
        // "wrathx" must not be treated as the wrath tree.
        assert_eq!(
            zam_candidate_urls("modelviewer/wrathx/meta/item/1.json"),
            vec!["https://wow.zamimg.com/modelviewer/wrathx/meta/item/1.json"]
        );
    }

    #[test]
    fn percent_decode_lossy_basics() {
        assert_eq!(percent_decode_lossy("a%2eb"), "a.b");
        assert_eq!(percent_decode_lossy("a%2"), "a%2");
        assert_eq!(percent_decode_lossy("a%zz"), "a%zz");
    }

    // --- Batch 6 C: cache maintenance --------------------------------------

    fn tmp_root(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("zamcache_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn dir_report_counts_bytes_and_files_recursively() {
        let root = tmp_root("report");
        let sub = root.join("images/wow/icons");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(root.join("a.json"), b"1234").unwrap(); // 4 bytes
        std::fs::write(sub.join("b.jpg"), b"abcdef").unwrap(); // 6 bytes
        let (bytes, files) = dir_report(&root);
        assert_eq!(files, 2);
        assert_eq!(bytes, 10);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dir_report_missing_dir_is_zero() {
        let (bytes, files) = dir_report(&tmp_root("missing").join("nope"));
        assert_eq!((bytes, files), (0, 0));
    }

    #[test]
    fn zam_cache_clear_removes_only_the_cache_subdir() {
        let root = tmp_root("clear");
        // The cache subdir with content...
        let cache = root.join(ZAM_CACHE_DIR).join("images");
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("x.jpg"), b"xxxxx").unwrap(); // 5 bytes
        // ...plus a SIBLING file in the cache root that must survive.
        std::fs::write(root.join("keep-me.txt"), b"keep").unwrap();

        let freed = zam_cache_clear(&root).unwrap();
        assert_eq!(freed, 5);
        assert!(!root.join(ZAM_CACHE_DIR).exists(), "zam-cache subdir wiped");
        assert!(root.join("keep-me.txt").exists(), "sibling state preserved");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn zam_cache_clear_absent_is_ok_and_zero() {
        let root = tmp_root("clear_absent");
        std::fs::create_dir_all(&root).unwrap();
        let freed = zam_cache_clear(&root).unwrap();
        assert_eq!(freed, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn zam_cache_report_targets_the_subdir() {
        let root = tmp_root("report_sub");
        let cache = root.join(ZAM_CACHE_DIR);
        std::fs::create_dir_all(&cache).unwrap();
        std::fs::write(cache.join("t.json"), b"abc").unwrap(); // 3 bytes
        // A file OUTSIDE the cache subdir must not be counted.
        std::fs::write(root.join("outside.txt"), b"zzzzzzzz").unwrap();
        let (bytes, files) = zam_cache_report(&root);
        assert_eq!((bytes, files), (3, 1));
        let _ = std::fs::remove_dir_all(&root);
    }
}
