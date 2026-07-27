//! Native-mode `wow bridge-setup`/`party-setup`/`setup` (Chunk 2 task C2c
//! item 4 -- see `.superpowers/sdd/chunk2-decisions.md`). Deploys the Eluna
//! bridge `*.lua` scripts, a faithful port of `_bridge_deploy_scripts`/
//! `_bridge_lua_root` (`cli/src/50-party.sh:11-43`).
//!
//! NATIVE SOURCE ROOT DECISION (chunk2-decisions.md item 4). The bash
//! default (`DML_LUA_DIR` unset -> `/usr/local/share/dml/lua`) is a
//! DISTRO-installed path that does not exist on native Windows. Native mode
//! instead derives the source root from `DML_SCRIPT` -- the same env var
//! `dml::runner`'s `find_dml_script` resolves the `dml` script itself from
//! (in dev, the repo's `cli/dml`; a bundled copy in a release build) -- so
//! its SIBLING `lua` dir (`cli/lua` in dev) is the deploy source:
//! `<parent of DML_SCRIPT>/lua`.
//!
//! [`bridge_setup_stream`] at the bottom is the streamed orchestration (SOAP
//! `server info` preflight, then deploy), moved out of the launcher's
//! `lib.rs` by the cargo-workspace refactor (Task 9) so the standalone CLI
//! can drive it too.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// `<parent of DML_SCRIPT>/lua` -- native's answer to `_bridge_lua_root`.
/// Falls back to a bare `"lua"` (relative to the process cwd) when
/// `DML_SCRIPT` is unset/empty or has no parent directory -- the same
/// "honest not-found" spirit `find_dml_script`'s own bare-`"dml"` fallback
/// has: `deploy_scripts` then simply finds no families under it (see its own
/// doc comment) rather than panicking.
pub fn lua_root_from_env() -> PathBuf {
    let script = std::env::var_os("DML_SCRIPT").filter(|s| !s.is_empty());
    lua_root_from_script(script.as_deref().map(Path::new))
}

/// Pure half of [`lua_root_from_env`], for testing without touching the
/// process environment.
pub fn lua_root_from_script(script_path: Option<&Path>) -> PathBuf {
    match script_path.and_then(Path::parent) {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join("lua"),
        _ => PathBuf::from("lua"),
    }
}

/// Host dir where mod-ale loads scripts (bind-mounted into the container at
/// `ALE.ScriptPath`) -- a port of `_party_lua_dest_dir` (`50-party.sh:19-22`):
/// `<server dir>/env/dist/etc/modules/lua_scripts`.
pub fn dest_dir(server_dir: &Path) -> PathBuf {
    server_dir.join("env").join("dist").join("etc").join("modules").join("lua_scripts")
}

/// Copy every family's (`gm/`, `party/`, ...) `*.lua` under `root` into the
/// flat `dest` dir -- a port of `_bridge_deploy_scripts` (`50-party.sh:26-43`):
/// content-compare each file (byte-for-byte), copy on new-or-mismatched,
/// leave an already-identical file untouched. `Ok(true)` iff anything
/// changed.
///
/// A missing/unreadable `root` degrades to "no families found" (`Ok(false)`,
/// after still creating `dest`) rather than an error -- mirrors the bash's
/// `for d in "$root"/*/` glob silently matching nothing on an absent dir
/// (no error, `changed` just stays empty), not `mkdir -p`'s own failure mode
/// (that one DOES propagate, matching the bash requiring `dest` to exist
/// before the loop can even try `cp`).
pub fn deploy_scripts(root: &Path, dest: &Path) -> std::io::Result<bool> {
    std::fs::create_dir_all(dest)?;
    let mut changed = false;
    let Ok(read) = std::fs::read_dir(root) else {
        return Ok(false);
    };
    let mut family_dirs: Vec<PathBuf> = read.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
    family_dirs.sort();
    for family in family_dirs {
        let Ok(files) = std::fs::read_dir(&family) else { continue };
        let mut lua_files: Vec<PathBuf> = files
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|e| e == "lua"))
            .collect();
        lua_files.sort();
        for src in lua_files {
            let Some(name) = src.file_name() else { continue };
            let dst = dest.join(name);
            let need_copy = match std::fs::read(&dst) {
                Ok(existing) => existing != std::fs::read(&src)?,
                Err(_) => true,
            };
            if need_copy {
                std::fs::copy(&src, &dst)?;
                changed = true;
            }
        }
    }
    Ok(changed)
}

fn bs_event_section_start() -> serde_json::Value {
    serde_json::json!({"event": "section_start", "name": "bridge-setup"})
}

fn bs_event_line(level: &str, text: impl Into<String>) -> serde_json::Value {
    serde_json::json!({"event": "line", "level": level, "text": text.into()})
}

fn bs_event_section_end(status: &str) -> serde_json::Value {
    serde_json::json!({"event": "section_end", "name": "bridge-setup", "status": status})
}

fn bs_event_done(changed: bool) -> serde_json::Value {
    serde_json::json!({"event": "done", "data": {"changed": changed, "restart_required": changed}})
}

fn bs_event_error(code: &str, message: impl Into<String>, hint: &str) -> serde_json::Value {
    serde_json::json!({"event": "error", "error": {"code": code, "message": message.into(), "hint": hint}})
}

/// The blocking flow itself (real SOAP + fs I/O) -- run under
/// `spawn_blocking`. A port of the `bridge-setup)` arm (`90-main.sh:2941-
/// 2979`): server installed? -> SOAP `server info` preflight -> deploy.
pub fn bridge_setup_stream(soap_lock: Arc<Mutex<()>>, emit: impl Fn(serde_json::Value)) {
    use crate::{bridge, config::ConfigReader, maint, soap};

    emit(bs_event_section_start());

    let title_dir = ConfigReader::title_dir_from_env();
    let Some(sdir) = maint::resolve_server_dir(&title_dir) else {
        emit(bs_event_section_end("error"));
        emit(bs_event_error("NOT_FOUND", "WoW Playerbots server not installed", "Install it first."));
        return;
    };

    emit(bs_event_line("info", "checking SOAP..."));
    let outcome = {
        let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = soap::SoapConfig::load();
        soap::exec(&cfg, "server info")
    };
    match outcome {
        soap::SoapOutcome::Ok(_) => {}
        soap::SoapOutcome::Auth => {
            emit(bs_event_section_end("error"));
            emit(bs_event_error("SOAP_AUTH", "SOAP auth failed", "Check ~/.dml/soap.env"));
            return;
        }
        // Fault and Unreachable both collapse to the oracle's default `rc
        // 3) ...; *) ...` case (90-main.sh:2962-2965) -- only auth (rc 3)
        // gets its own message.
        _ => {
            emit(bs_event_section_end("error"));
            emit(bs_event_error("SOAP_UNREACHABLE", "Could not reach the server over SOAP", "Start the server, then re-run."));
            return;
        }
    }

    emit(bs_event_line("info", "deploying bridge scripts..."));
    let root = bridge::lua_root_from_env();
    let dest = bridge::dest_dir(&sdir);
    let changed = match bridge::deploy_scripts(&root, &dest) {
        Ok(c) => c,
        Err(e) => {
            emit(bs_event_section_end("error"));
            emit(bs_event_error("WRITE_FAILED", format!("Could not deploy bridge scripts: {e}"), ""));
            return;
        }
    };
    emit(bs_event_line("info", format!("scripts deployed (changed={changed})")));
    emit(bs_event_section_end("ok"));
    emit(bs_event_done(changed));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("dml_bridge_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    // -- lua_root_from_script ---------------------------------------------

    #[test]
    fn lua_root_from_script_uses_the_scripts_parent_dir() {
        let root = lua_root_from_script(Some(Path::new("/repo/cli/dml")));
        assert_eq!(root, PathBuf::from("/repo/cli").join("lua"));
    }

    /// WINDOWS-ONLY by construction: `\` is a path separator only under
    /// `std::path`'s Windows flavour. On Linux `C:\repo\cli\dml` is one
    /// component with no parent, so the fallback (covered by
    /// `lua_root_from_script_falls_back_on_a_bare_filename_with_no_parent`)
    /// is the correct answer there, not this one. The POSIX-path case is
    /// covered by `lua_root_from_script_uses_the_scripts_parent_dir`.
    #[cfg(windows)]
    #[test]
    fn lua_root_from_script_windows_style_path() {
        let root = lua_root_from_script(Some(Path::new(r"C:\repo\cli\dml")));
        assert_eq!(root, PathBuf::from(r"C:\repo\cli").join("lua"));
    }

    #[test]
    fn lua_root_from_script_falls_back_when_absent() {
        assert_eq!(lua_root_from_script(None), PathBuf::from("lua"));
    }

    #[test]
    fn lua_root_from_script_falls_back_on_a_bare_filename_with_no_parent() {
        // A bare "dml" (find_dml_script's own fallback) has an empty parent.
        assert_eq!(lua_root_from_script(Some(Path::new("dml"))), PathBuf::from("lua"));
    }

    // -- dest_dir -----------------------------------------------------------

    #[test]
    fn dest_dir_shape() {
        let d = dest_dir(Path::new("/servers/wow"));
        assert_eq!(
            d,
            PathBuf::from("/servers/wow").join("env").join("dist").join("etc").join("modules").join("lua_scripts")
        );
    }

    // -- deploy_scripts -------------------------------------------------------

    #[test]
    fn deploy_scripts_copies_new_files_and_reports_changed() {
        let root = tmp_dir("deploy_new_root");
        let dest = tmp_dir("deploy_new_dest");
        std::fs::create_dir_all(root.join("party")).unwrap();
        std::fs::create_dir_all(root.join("gm")).unwrap();
        std::fs::write(root.join("party").join("dml_login.lua"), b"-- login v1").unwrap();
        std::fs::write(root.join("gm").join("dml_gm.lua"), b"-- gm v1").unwrap();
        // Not a .lua file: must be ignored.
        std::fs::write(root.join("party").join("README.md"), b"nope").unwrap();

        let changed = deploy_scripts(&root, &dest).unwrap();
        assert!(changed);
        assert_eq!(std::fs::read(dest.join("dml_login.lua")).unwrap(), b"-- login v1");
        assert_eq!(std::fs::read(dest.join("dml_gm.lua")).unwrap(), b"-- gm v1");
        assert!(!dest.join("README.md").exists());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn deploy_scripts_is_idempotent_when_unchanged() {
        let root = tmp_dir("deploy_idem_root");
        let dest = tmp_dir("deploy_idem_dest");
        std::fs::create_dir_all(root.join("party")).unwrap();
        std::fs::write(root.join("party").join("a.lua"), b"same").unwrap();

        assert!(deploy_scripts(&root, &dest).unwrap());
        // Second run: content is identical -> no copy, changed=false.
        assert!(!deploy_scripts(&root, &dest).unwrap());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn deploy_scripts_recopies_on_content_mismatch() {
        let root = tmp_dir("deploy_mismatch_root");
        let dest = tmp_dir("deploy_mismatch_dest");
        std::fs::create_dir_all(root.join("party")).unwrap();
        std::fs::write(root.join("party").join("a.lua"), b"v1").unwrap();
        assert!(deploy_scripts(&root, &dest).unwrap());

        std::fs::write(root.join("party").join("a.lua"), b"v2").unwrap();
        assert!(deploy_scripts(&root, &dest).unwrap());
        assert_eq!(std::fs::read(dest.join("a.lua")).unwrap(), b"v2");

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn deploy_scripts_missing_root_creates_dest_and_reports_unchanged() {
        let root = tmp_dir("deploy_missing_root_never_created");
        std::fs::remove_dir_all(&root).unwrap(); // now genuinely absent
        let dest = tmp_dir("deploy_missing_dest");

        let changed = deploy_scripts(&root, &dest).unwrap();
        assert!(!changed);
        assert!(dest.is_dir());

        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn deploy_scripts_creates_dest_when_absent() {
        let root = tmp_dir("deploy_dest_absent_root");
        std::fs::create_dir_all(root.join("party")).unwrap();
        std::fs::write(root.join("party").join("a.lua"), b"x").unwrap();
        let mut dest = std::env::temp_dir();
        dest.push(format!("dml_bridge_test_deploy_dest_absent_dest_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        assert!(!dest.exists());

        let changed = deploy_scripts(&root, &dest).unwrap();
        assert!(changed);
        assert!(dest.join("a.lua").exists());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn bs_event_section_start_shape() {
        assert_eq!(bs_event_section_start(), serde_json::json!({"event":"section_start","name":"bridge-setup"}));
    }

    #[test]
    fn bs_event_line_shape() {
        assert_eq!(
            bs_event_line("info", "deploying bridge scripts..."),
            serde_json::json!({"event":"line","level":"info","text":"deploying bridge scripts..."})
        );
    }

    #[test]
    fn bs_event_section_end_shape() {
        assert_eq!(bs_event_section_end("ok"), serde_json::json!({"event":"section_end","name":"bridge-setup","status":"ok"}));
        assert_eq!(bs_event_section_end("error"), serde_json::json!({"event":"section_end","name":"bridge-setup","status":"error"}));
    }

    #[test]
    fn bs_event_done_shape() {
        assert_eq!(bs_event_done(true), serde_json::json!({"event":"done","data":{"changed":true,"restart_required":true}}));
        assert_eq!(bs_event_done(false), serde_json::json!({"event":"done","data":{"changed":false,"restart_required":false}}));
    }

    #[test]
    fn bs_event_error_shape() {
        assert_eq!(
            bs_event_error("SOAP_AUTH", "SOAP auth failed", "Check ~/.dml/soap.env"),
            serde_json::json!({"event":"error","error":{"code":"SOAP_AUTH","message":"SOAP auth failed","hint":"Check ~/.dml/soap.env"}})
        );
    }
}
