use std::io::Read;
use std::sync::OnceLock;

/// Maps a zam-scheme request path to (upstream URL, cache-relative path).
/// Security: fixed host, prefix allowlist, no traversal, no embedded schemes.
pub fn zam_map_path(raw: &str) -> Option<(String, String)> {
    let path = raw.trim_start_matches('/');
    let path = path.split('?').next().unwrap_or("");
    if path.is_empty() || path.contains('\\') || path.contains("://") {
        return None;
    }
    if !(path.starts_with("modelviewer/") || path.starts_with("images/")) {
        return None;
    }
    if path.split('/').any(|seg| seg == ".." || seg.is_empty()) {
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
}
