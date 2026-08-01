//! One-shot startup resolution of the four `DML_*` variables.
//!
//! WHY THE ENVIRONMENT. `backend::selected()` and the three path readers
//! (`games_dir_from_env`, `ConfigReader::title_dir_from_env`,
//! `find_dml_script`) read the process environment fresh on EVERY call, and
//! native children inherit it (`DmlRunner` only prepends PATH). Writing the
//! resolved values here therefore fixes ~60 native command gates and the
//! bash children at once, without threading a resolver through any of them.
//!
//! WHY ONLY-IF-UNSET. Precedence is `env → launcher.json → auto-detect`, and
//! env must stay highest: the parity, bats and CLI integration suites all
//! inject these variables as override seams.
//!
//! ORDERING. `std::env::set_var` is only sound before other threads exist, so
//! `resolve_and_export()` MUST be the first statement of `run()` — before
//! `tauri::Builder::default()`, whose `.setup()` spawns the interval-backup
//! watcher thread.

use std::path::PathBuf;

/// Whether the USER set `DML_BACKEND` before launch, captured before we
/// export our own resolved value over the top of that emptiness.
///
/// Without this the Settings dropdown is permanently read-only: we always
/// export `DML_BACKEND`, so a later `std::env::var` can never distinguish
/// "the user pinned it" from "we resolved it", and the UI would report every
/// session as env-locked.
static BACKEND_WAS_USER_SET: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// True only when `DML_BACKEND` was already set in the environment we
/// inherited. Defaults to false if `resolve_and_export` never ran.
pub fn backend_was_user_set() -> bool {
    *BACKEND_WAS_USER_SET.get().unwrap_or(&false)
}

/// Pure: what to write for one variable, or `None` to leave it alone.
pub fn value_to_export(env_value: Option<&str>, resolved: Option<&str>) -> Option<String> {
    if env_value.map(str::trim).is_some_and(|v| !v.is_empty()) {
        return None; // the user set it; never overwrite
    }
    resolved.map(str::to_string)
}

/// The conventional native install location, used when neither the
/// environment nor `launcher.json` names one.
pub fn default_games_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("dml-native"))
}

/// Resolve the backend and the three paths, then export whatever the user has
/// not already set. Call FIRST in `run()`.
pub fn resolve_and_export() {
    let home = match dml_core::util::dml_home_dir() {
        Some(h) => h,
        None => return, // no USERPROFILE/HOME: nothing to read, nothing to write
    };
    let cfg = dml_core::launcher_config::load(&home);

    // Capture user-set-ness BEFORE any export, or it is unrecoverable.
    let env_backend_raw = std::env::var("DML_BACKEND").ok();
    let _ = BACKEND_WAS_USER_SET.set(
        env_backend_raw.as_deref().map(str::trim).is_some_and(|v| !v.is_empty()),
    );

    // --- games dir -------------------------------------------------------
    // Env FIRST. It is not merely an override to pass through: the probe
    // below uses this path, so ignoring a user's DML_GAMES_DIR would detect
    // against the wrong directory and could land them on the very "offline
    // while the server runs" bug this module exists to fix.
    let games_dir: Option<PathBuf> = std::env::var("DML_GAMES_DIR")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            cfg.games_dir
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from)
        })
        .or_else(default_games_dir);

    // --- probes for auto-detection ---------------------------------------
    // A COMPOSE FILE, not a bare directory. Same weak-evidence class as the
    // catalog's `[[ -d ... ]]` test: the install engine creates the title dir at
    // stage 3 of 8, so bare existence would call a folder holding nothing but a
    // half-done clone a native install. Requiring the generated compose file at
    // least means something was configured. (It does not close the class --
    // generate-compose is stage 5, so a failed BUILD still looks
    // native-installed. That is the right answer here anyway: it IS a native
    // title dir, and `native_title_count` is what decides whether it is
    // playable.)
    let native_dir_exists = games_dir
        .as_ref()
        .map(|g| {
            g.join("wow-server-playerbots").join(dml_wow::composegen::BASE_FILE).is_file()
        })
        .unwrap_or(false);
    // `docker_desktop_program` has NO bare-name fallback, so `Some` means a
    // real Docker Desktop executable was found on disk.
    let docker_present = dml_core::engine::docker_desktop_program().is_some();
    // "Could the WSL backend work at all here?" Without this, detection could
    // never select Native on a fresh machine: its other signal is "a native
    // server directory exists", which only becomes true AFTER a native install
    // — and installing requires being in native mode already.
    //
    // Only asked when it can change the answer. When Docker is absent the
    // result is Wsl regardless, and when a native server dir is already there
    // the result is Native regardless; spawning `wsl.exe` in either case would
    // add startup latency for a value nothing reads. This runs before the
    // window is shown, so that matters.
    let wsl_usable = if docker_present && !native_dir_exists {
        dml_core::setup::distro_registered(&dml_core::setup::SetupProbeEnv::new(
            dml_core::runner::DISTRO,
            dml_core::runner::USER,
        ))
    } else {
        dml_core::setup::Tri::Unknown
    };

    let backend = dml_core::backend::resolve(
        env_backend_raw.as_deref(),
        cfg.backend.as_deref(),
        native_dir_exists,
        docker_present,
        wsl_usable,
    );
    let backend_str = match backend {
        dml_core::backend::Backend::Native => "native",
        dml_core::backend::Backend::Wsl => "wsl",
    };

    // --- yq: default to the path the one-click installer downloads to -----
    let yq: Option<String> = cfg
        .yq_bin
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| {
            games_dir
                .as_ref()
                .map(|g| g.join("tools").join("yq.exe").to_string_lossy().into_owned())
        });

    // --- script: NO invented default, but a SHIPPED one is not invented ---
    //
    // Precedence stays env > launcher.json, then falls back to the copy of
    // `cli/dml` this build bundles next to the exe (SHIP-LIST 4.1). Without
    // that last step the whole point of bundling the payload was lost for
    // native mode: `find_dml_script` fell back to a bare `dml`, so a packaged
    // launcher still needed a repo checkout to run natively -- exactly the
    // "clone the repo, be named perzi" problem Phase 4 exists to end.
    //
    // Only used when the file is actually THERE, so a dev build without a
    // bundled payload still behaves as before. Consumed by DmlRunner::native()
    // alone; WSL mode never reads DML_SCRIPT, so this cannot disturb it.
    let script: Option<String> = cfg
        .dml_script
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(bundled_cli_script);

    let exports: Vec<(&str, Option<String>)> = vec![
        ("DML_BACKEND", value_to_export(env_backend_raw.as_deref(), Some(backend_str))),
        (
            "DML_GAMES_DIR",
            value_to_export(
                std::env::var("DML_GAMES_DIR").ok().as_deref(),
                games_dir.as_ref().map(|g| g.to_string_lossy().into_owned()).as_deref(),
            ),
        ),
        ("DML_SCRIPT", value_to_export(std::env::var("DML_SCRIPT").ok().as_deref(), script.as_deref())),
        ("DML_YQ_BIN", value_to_export(std::env::var("DML_YQ_BIN").ok().as_deref(), yq.as_deref())),
    ];

    for (name, value) in exports {
        if let Some(v) = value {
            std::env::set_var(name, v);
        }
    }
}

/// The `cli/dml` this build bundles beside the exe, when it is really there.
///
/// Resolved from the executable's own directory rather than tauri's resource
/// API because this runs BEFORE the app is built (see `resolve_and_export`'s
/// placement) — and on Windows tauri's `resource_dir()` returns exactly that
/// directory anyway, so the two agree.
fn bundled_cli_script() -> Option<String> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let script = crate::payload::paths(&exe_dir).cli_script;
    script.is_file().then(|| script.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_only_when_env_is_absent_or_empty() {
        // Env wins: a set value is never overwritten.
        assert_eq!(value_to_export(Some("C:/set-by-user"), Some("C:/resolved")), None);
        // Unset or empty: the resolved value fills in.
        assert_eq!(value_to_export(None, Some("C:/resolved")), Some("C:/resolved".to_string()));
        assert_eq!(value_to_export(Some(""), Some("C:/resolved")), Some("C:/resolved".to_string()));
        assert_eq!(value_to_export(Some("   "), Some("C:/resolved")), Some("C:/resolved".to_string()));
    }

    #[test]
    fn export_nothing_when_there_is_nothing_to_resolve() {
        // No env AND no resolved value: leave it unset so downstream failures
        // stay honest ("not found") instead of pointing at an invented path.
        assert_eq!(value_to_export(None, None), None);
        assert_eq!(value_to_export(Some(""), None), None);
    }
}
