//! Native-mode **WoW client path** reader (`wow client-path get|detect`,
//! task D1a). Faithful port of `cli/src/90-main.sh:5551-5593` +
//! `_client_path_file`/`_client_valid`/`_client_detect`
//! (`cli/src/70-modules.sh:315-373`). `client-path SET` is a write — out of
//! scope here.
//!
//! `detect`'s candidate ROOTS are necessarily Windows-native here (this
//! runs directly on Windows, not inside WSL): there is no `/mnt/<drive>` —
//! the per-drive `Games` / `Program Files (x86)` / `wow wotlk` / drive-root
//! scan is the Windows-native analogue of the bash's `/mnt/[a-z]` sweep.
//! Shape parity (`{"candidates":[...]}`) is the gate, not literal path
//! identity, since the two backends walk different filesystems.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

/// `_client_valid` (`70-modules.sh:328-330`): does `dir` look like a WoW
/// client install?
pub fn valid_client_dir(dir: &Path) -> bool {
    dir.join("Wow.exe").is_file()
        || dir.join("wow.exe").is_file()
        || dir.join("WowT.exe").is_file()
        || dir.join("Interface").is_dir()
}

/// `_client_path_file` (`70-modules.sh:315`): `~/.dml/client-path`.
pub fn client_path_file() -> Option<PathBuf> {
    super::dml_home_dir().map(|h| h.join("client-path"))
}

/// Pure parse/validate step of `client-path get` (`90-main.sh:5551-5559`),
/// given the file's raw content (`None` = missing/unreadable, matching
/// `[[ -r "$f" ]]` failing). Bash's `saved="$(cat "$f")"` strips ALL
/// trailing newlines via command substitution — mirrored here by trimming
/// trailing `\n`/`\r`.
pub fn client_path_value(raw: Option<&str>) -> Value {
    let saved = raw.unwrap_or("");
    let saved = saved.trim_end_matches(['\n', '\r']);
    if saved.is_empty() {
        json!({"path": Value::Null, "valid": false})
    } else {
        let p = Path::new(saved);
        let valid = p.is_dir() && valid_client_dir(p);
        json!({"path": saved, "valid": valid})
    }
}

/// `client-path get`: reads the real `~/.dml/client-path` file, then
/// [`client_path_value`].
pub fn read_client_path() -> Value {
    let content = client_path_file().and_then(|f| std::fs::read_to_string(f).ok());
    client_path_value(content.as_deref())
}

/// Candidate scan roots for `detect`. `DML_CLIENT_SCAN_ROOTS` (platform
/// path-list separator, i.e. `;` on Windows — `std::env::split_paths`)
/// overrides for tests, mirroring the bash's `DML_CLIENT_SCAN_ROOTS` test
/// seam (`70-modules.sh:351-352`, there colon-separated for a POSIX shell).
/// Absent the override: the user's home `Games` / `wow wotlk` / home
/// itself, then per existing drive letter A-Z, `<drive>:\Games`,
/// `<drive>:\Program Files (x86)`, `<drive>:\wow wotlk`, `<drive>:\`.
pub fn default_scan_roots() -> Vec<PathBuf> {
    if let Some(over) = std::env::var_os("DML_CLIENT_SCAN_ROOTS") {
        return std::env::split_paths(&over).collect();
    }
    let mut roots = Vec::new();
    if let Some(home) = super::home_dir() {
        roots.push(home.join("Games"));
        roots.push(home.join("wow wotlk"));
        roots.push(home);
    }
    for letter in b'A'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        if !root.is_dir() {
            continue;
        }
        roots.push(root.join("Games"));
        roots.push(root.join("Program Files (x86)"));
        roots.push(root.join("wow wotlk"));
        roots.push(root);
    }
    roots
}

/// `_client_detect` (`70-modules.sh:349-373`): one level deep under each
/// root, directories only (dotfiles skipped, matching a bare bash glob),
/// capped at 10 candidates total across all roots.
pub fn detect_client(roots: &[PathBuf]) -> Vec<String> {
    let mut out = Vec::new();
    for r in roots {
        let Ok(rd) = std::fs::read_dir(r) else { continue };
        for e in rd.flatten() {
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(ft) = e.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let p = e.path();
            if valid_client_dir(&p) {
                out.push(p.display().to_string());
                if out.len() >= 10 {
                    return out;
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dml-clientpath-{name}-{}", std::process::id()))
    }

    #[test]
    fn valid_client_dir_detects_exe_or_interface() {
        let d = tmp("valid");
        std::fs::create_dir_all(&d).unwrap();
        assert!(!valid_client_dir(&d));
        std::fs::write(d.join("Wow.exe"), "x").unwrap();
        assert!(valid_client_dir(&d));
        let _ = std::fs::remove_dir_all(&d);

        let d2 = tmp("valid-iface");
        std::fs::create_dir_all(d2.join("Interface")).unwrap();
        assert!(valid_client_dir(&d2));
        let _ = std::fs::remove_dir_all(&d2);
    }

    #[test]
    fn client_path_value_missing_or_empty_is_null_invalid() {
        assert_eq!(client_path_value(None), json!({"path": null, "valid": false}));
        assert_eq!(client_path_value(Some("")), json!({"path": null, "valid": false}));
        assert_eq!(client_path_value(Some("\n\r\n")), json!({"path": null, "valid": false}));
    }

    #[test]
    fn client_path_value_nonexistent_saved_path_is_invalid_but_present() {
        let v = client_path_value(Some("C:\\this\\does\\not\\exist\n"));
        assert_eq!(v["path"], "C:\\this\\does\\not\\exist");
        assert_eq!(v["valid"], false);
    }

    #[test]
    fn client_path_value_real_valid_dir() {
        let d = tmp("real-valid");
        std::fs::create_dir_all(d.join("Interface")).unwrap();
        let raw = format!("{}\n", d.display());
        let v = client_path_value(Some(&raw));
        assert_eq!(v["path"], d.display().to_string());
        assert_eq!(v["valid"], true);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn detect_client_finds_valid_subdirs_skips_invalid_and_dotdirs() {
        let root = tmp("detect-root");
        std::fs::create_dir_all(root.join("GoodClient").join("Interface")).unwrap();
        std::fs::create_dir_all(root.join("NotAClient")).unwrap();
        std::fs::create_dir_all(root.join(".hidden").join("Interface")).unwrap();
        let got = detect_client(&[root.clone()]);
        assert_eq!(got.len(), 1);
        assert!(got[0].ends_with("GoodClient"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn detect_client_caps_at_10_across_roots() {
        let root = tmp("detect-cap");
        for i in 0..15 {
            std::fs::create_dir_all(root.join(format!("Client{i}")).join("Interface")).unwrap();
        }
        let got = detect_client(&[root.clone()]);
        assert_eq!(got.len(), 10);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn default_scan_roots_env_override_uses_platform_separator() {
        let a = tmp("root-a");
        let b = tmp("root-b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let joined = std::env::join_paths([&a, &b]).unwrap();
        std::env::set_var("DML_CLIENT_SCAN_ROOTS", &joined);
        let roots = default_scan_roots();
        std::env::remove_var("DML_CLIENT_SCAN_ROOTS");
        assert_eq!(roots, vec![a.clone(), b.clone()]);
        let _ = std::fs::remove_dir_all(&a);
        let _ = std::fs::remove_dir_all(&b);
    }
}
