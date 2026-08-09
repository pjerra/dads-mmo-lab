//! Build-capability primitives shared by the Unbound engine and the module
//! subsystem: which `-f` files a build-aware compose call needs, and whether
//! the effective config can build ac-worldserver at all.
//!
//! Extracted from `unbound.rs` (review CRITICAL 2026-08-02: `build:` lives in
//! `docker-compose.build.yml`, which compose NEVER auto-loads — a bare
//! `compose build` there builds NOTHING and exits 0) so `modmgr`'s rebuild
//! cannot drift from the engine that already got this right.

use std::path::Path;

use super::composegen::{BASE_FILE, BUILD_FILE, OVERRIDE_FILE};

/// The `-f` set a compose call needs to SEE build config, from disk evidence.
/// Composegen servers keep `build:` in [`BUILD_FILE`] (never auto-loaded);
/// bash-era servers keep it in the base compose and need no flags. Body is
/// the former `UnboundEngine::resolve_build_files` verbatim.
pub fn build_files(sdir: &Path) -> Vec<String> {
    let mut files: Vec<String> = Vec::new();
    if sdir.join(BUILD_FILE).is_file() {
        for f in [BASE_FILE, OVERRIDE_FILE, BUILD_FILE] {
            if sdir.join(f).is_file() {
                files.push("-f".into());
                files.push(f.into());
            }
        }
    }
    files
}

/// Does the effective compose config let ac-worldserver build? Input is
/// `docker compose <files> config --format json` stdout. `None` means the
/// answer could not be read — tri-state, callers warn and proceed.
pub fn worldserver_has_build(config_json: &str) -> Option<bool> {
    let cfg: serde_json::Value = serde_json::from_str(config_json).ok()?;
    Some(
        cfg.get("services")
            .and_then(|s| s.get("ac-worldserver"))
            .map(|w| w.get("build").is_some())
            .unwrap_or(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tdir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dml-buildcap-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn a_composegen_server_gets_all_three_files() {
        let d = tdir("three");
        for f in [BASE_FILE, OVERRIDE_FILE, BUILD_FILE] {
            std::fs::write(d.join(f), "").unwrap();
        }
        assert_eq!(
            build_files(&d),
            vec!["-f", BASE_FILE, "-f", OVERRIDE_FILE, "-f", BUILD_FILE]
        );
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_missing_override_is_skipped_but_base_and_build_survive() {
        let d = tdir("noover");
        std::fs::write(d.join(BASE_FILE), "").unwrap();
        std::fs::write(d.join(BUILD_FILE), "").unwrap();
        assert_eq!(build_files(&d), vec!["-f", BASE_FILE, "-f", BUILD_FILE]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_server_without_the_build_overlay_needs_no_flags() {
        // bash-era/WSL servers keep build: in the base compose — an empty -f
        // set makes compose auto-load base+override, which is correct there.
        let d = tdir("wsl");
        std::fs::write(d.join(BASE_FILE), "").unwrap();
        assert_eq!(build_files(&d), Vec::<String>::new());
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn build_key_present_is_some_true() {
        let j = r#"{"services":{"ac-worldserver":{"build":{"context":"."},"image":"x"}}}"#;
        assert_eq!(worldserver_has_build(j), Some(true));
    }

    #[test]
    fn build_key_absent_is_some_false() {
        let j = r#"{"services":{"ac-worldserver":{"image":"dml.local/x:migrated"}}}"#;
        assert_eq!(worldserver_has_build(j), Some(false));
    }

    #[test]
    fn missing_service_is_some_false_and_garbage_is_none() {
        assert_eq!(worldserver_has_build(r#"{"services":{}}"#), Some(false));
        assert_eq!(worldserver_has_build("not json at all"), None);
    }
}
