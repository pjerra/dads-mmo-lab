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

/// Serve one request: cache hit -> bytes; else fetch (NO browser Origin
/// header -- that is the whole point), cache atomically, return. Any
/// failure -> None (handler answers 404).
pub fn zam_serve(cache_root: &std::path::Path, raw_path: &str) -> Option<(Vec<u8>, &'static str)> {
    let (url, rel) = zam_map_path(raw_path)?;
    let ct = content_type_for(&rel);
    let cached = cache_root.join("zam-cache").join(&rel);
    if let Ok(bytes) = std::fs::read(&cached) {
        return Some((bytes, ct));
    }
    let resp = client().get(&url).send().ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let mut bytes = Vec::new();
    resp.take(64 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    if let Some(parent) = cached.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let tmp = cached.with_extension(format!(
        "{}.tmp",
        cached.extension().and_then(|e| e.to_str()).unwrap_or("bin")
    ));
    if std::fs::write(&tmp, &bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &cached);
    }
    Some((bytes, ct))
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

    #[test]
    fn percent_decode_lossy_basics() {
        assert_eq!(percent_decode_lossy("a%2eb"), "a.b");
        assert_eq!(percent_decode_lossy("a%2"), "a%2");
        assert_eq!(percent_decode_lossy("a%zz"), "a%zz");
    }
}
