//! Tuning-WRITE parity gate for the LUA backend (Task 11, controller ruling
//! D2).
//!
//! Asserts that `dml_wow::tuning::tuning_set`'s NATIVE lua write path (the
//! `_lua_cfg_write` / `_mtune_to_lua` port added by Task 11) and the bash
//! oracle's `wow config tuning-set --key <lua row> --value <v>`
//! (`90-main.sh:2909-2932`) produce BYTE-IDENTICAL `.lua` files for the same
//! edits, and report the same `{changed, applied, restart_required}` verdicts.
//!
//! This is the gate the ruling asked for: before Task 11 that lua branch
//! shelled out to this very CLI (so "parity" was tautological); now the two
//! implementations are independent and have to be compared for real. Five of
//! the 13 embedded tuning rows are lua-backend —
//! `unlimitedammo.{enabled,max_ammo,min_threshold}` and
//! `sitmeansrest.{duration,regen_aura}` — and both files are exercised here.
//!
//! HARD RULE, identical to `config_write_parity.rs`: this test never touches
//! the real title dir (`C:/Users/perzi/dml-native/wow-server-playerbots`).
//! Every file it reads or writes lives under a fresh `std::env::temp_dir()`
//! subtree, built per run and removed at the end. Two independent temp "title
//! dirs" are seeded with byte-identical starting `.lua` scripts; the SAME edit
//! is applied to one via `tuning_set` and to the other via the real `dml` CLI
//! (bash-spawned, `DML_GAMES_DIR` pointed at that copy) — then the resulting
//! files are byte-compared.
//!
//! NO SERVER, NO DOCKER: the lua backend applies live via `.reload ale` and
//! makes no SOAP call, no `docker inspect`, and no DB query on either side, so
//! the whole comparison is a pure offline file edit — safe (and meaningful)
//! with Docker Desktop closed.
//!
//! FILE/TOOL-GATED, same skip convention as its siblings: skips (and passes)
//! when `bash`, the `dml` script, or `yq` can't be found, or when the temp
//! fixtures can't be prepared. Overridable via `DML_BASH` / `DML_SCRIPT` /
//! `DML_YQ_BIN`.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use dml_wow::tuning::{lua_path_in, tuning_set};

fn find_bash() -> Option<OsString> {
    if let Some(b) = std::env::var_os("DML_BASH").filter(|s| !s.is_empty()) {
        return Some(b);
    }
    for c in [
        r"C:\Program Files\Git\bin\bash.exe",
        r"C:\Program Files\Git\usr\bin\bash.exe",
    ] {
        if Path::new(c).exists() {
            return Some(OsString::from(c));
        }
    }
    None
}

fn find_script() -> Option<PathBuf> {
    if let Some(s) = std::env::var_os("DML_SCRIPT").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(s));
    }
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("cli")
        .join("dml");
    p.exists().then_some(p)
}

fn find_yq() -> Option<PathBuf> {
    if let Some(y) = std::env::var_os("DML_YQ_BIN").filter(|s| !s.is_empty()) {
        let p = PathBuf::from(y);
        if p.exists() {
            return Some(p);
        }
    }
    let p = PathBuf::from(r"C:\Users\perzi\dml-native").join("tools").join("yq.exe");
    p.exists().then_some(p)
}

/// PATH with the bash toolchain dirs (bin + usr/bin) prepended, so the spawned
/// `dml` finds the `awk` `_lua_cfg_write` is written in.
fn augmented_path(bash: &Path) -> OsString {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(bin) = bash.parent() {
        dirs.push(bin.to_path_buf());
        if let Some(root) = bin.parent() {
            dirs.push(root.join("usr").join("bin"));
        }
    }
    if let Some(cur) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

/// The two deployed ALE scripts the lua-backend tuning rows address, shaped
/// like the real upstream files: `UnlimitedAmmo.lua` uses column-0 namespaced
/// keys, `SitMeansRest.lua` uses indented bare table keys with trailing commas
/// and inline comments — between them they cover every formatting branch of
/// the awk `rebuild` this port mirrors.
const UNLIMITED_AMMO_LUA: &str = "-- Unlimited Ammo (fixture)\n\
     UnlimitedAmmoNamespace = {}\n\
     UnlimitedAmmoNamespace.ENABLED = false\n\
     UnlimitedAmmoNamespace.MAX_AMMO = 1000 -- how many to keep stocked\n\
     UnlimitedAmmoNamespace.MIN_AMMO_THRESHOLD = 52\n";

const SIT_MEANS_REST_LUA: &str = "-- SitMeansRest (fixture)\n\
     local CONFIG = {\n\
     \x20   DURATION = 20, -- seconds\n\
     \x20   REGEN_AURA = 25990,\n\
     }\n";

/// Seeds a fresh temp "games dir" with one `wow-server-playerbots` title: a
/// minimal `docker-compose.yml` (so `_wow_server_dir`/`_resolve_compose_dir`
/// resolve it) plus the two deployed lua scripts.
fn seed_title_dir(games_dir: &Path) -> std::io::Result<PathBuf> {
    let title_dir = games_dir.join("wow-server-playerbots");
    let lua_dir = title_dir
        .join("env")
        .join("dist")
        .join("etc")
        .join("modules")
        .join("lua_scripts");
    fs::create_dir_all(&lua_dir)?;
    fs::write(
        title_dir.join("docker-compose.yml"),
        "services:\n  ac-worldserver:\n    image: dummy\n",
    )?;
    fs::write(lua_dir.join("UnlimitedAmmo.lua"), UNLIMITED_AMMO_LUA)?;
    fs::write(lua_dir.join("SitMeansRest.lua"), SIT_MEANS_REST_LUA)?;
    Ok(title_dir)
}

fn run_cli_tuning_set(
    bash: &Path,
    script: &Path,
    games_dir: &Path,
    yq: &Path,
    path: &OsString,
    key: &str,
    value: &str,
) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script)
        .args(["wow", "config", "tuning-set", "--key", key, "--value", value, "--json"]);
    cmd.env("DML_GAMES_DIR", games_dir);
    cmd.env("DML_YQ_BIN", yq);
    cmd.env("PATH", path);
    let out = cmd.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
        panic!(
            "dml config tuning-set output not JSON ({e}): {stdout}\nstderr: {}",
            String::from_utf8_lossy(&out.stderr)
        )
    })
}

/// Real title dir the launcher/CLI use day to day — never written here.
fn real_title_dir() -> PathBuf {
    PathBuf::from(r"C:\Users\perzi\dml-native\wow-server-playerbots")
}

/// One `tuning_set` call with a private, single-use config lock — the CLI does
/// exactly this (one process, no other writer to serialize against).
fn native_tuning_set(title_dir: &Path, key: &str, value: &str) -> Result<serde_json::Value, String> {
    tuning_set(
        key.to_string(),
        value.to_string(),
        Arc::new(Mutex::new(())),
        title_dir.to_path_buf(),
    )
    .map_err(|e| format!("{}|{}|{}", e.code, e.message, e.hint))
}

#[test]
fn native_lua_tuning_write_matches_cli() {
    // --- gate: everything must be present, else skip (and pass) ---
    let Some(bash) = find_bash() else {
        eprintln!("SKIP tuning_write_parity: prereqs absent (no bash, set DML_BASH)");
        return;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP tuning_write_parity: prereqs absent (no dml script, set DML_SCRIPT)");
        return;
    };
    let Some(yq) = find_yq() else {
        eprintln!("SKIP tuning_write_parity: prereqs absent (no yq, set DML_YQ_BIN)");
        return;
    };
    let path = augmented_path(&bash);

    // --- fixtures: two independent temp games dirs, never the real one ---
    let tmp_base = std::env::temp_dir().join(format!("dml_tunewrite_parity_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp_base);
    let rust_games_dir = tmp_base.join("rust-side");
    let cli_games_dir = tmp_base.join("cli-side");

    let rust_title_dir = match seed_title_dir(&rust_games_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP tuning_write_parity: prereqs absent (seed rust-side fixture: {e})");
            return;
        }
    };
    let cli_title_dir = match seed_title_dir(&cli_games_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP tuning_write_parity: prereqs absent (seed cli-side fixture: {e})");
            let _ = fs::remove_dir_all(&tmp_base);
            return;
        }
    };
    assert_ne!(rust_title_dir, real_title_dir(), "rust-side fixture must not be the real title dir");
    assert_ne!(cli_title_dir, real_title_dir(), "cli-side fixture must not be the real title dir");

    let rust_lua = |f: &str| lua_path_in(&rust_title_dir, f);
    let cli_lua = |f: &str| lua_path_in(&cli_title_dir, f);
    for f in ["UnlimitedAmmo.lua", "SitMeansRest.lua"] {
        assert_eq!(
            fs::read(rust_lua(f)).expect("read seeded rust lua"),
            fs::read(cli_lua(f)).expect("read seeded cli lua"),
            "fixtures must start identical: {f}"
        );
    }

    // --- edit 1: a BOOL row (0 -> 1, i.e. false -> true in the file) --------
    let got = native_tuning_set(&rust_title_dir, "unlimitedammo.enabled", "1")
        .expect("native tuning_set (bool)");
    let want = run_cli_tuning_set(&bash, &script, &cli_games_dir, &yq, &path, "unlimitedammo.enabled", "1");
    assert_eq!(want["ok"], true, "cli tuning-set not ok: {want}");
    assert_eq!(got, want["data"], "envelope data diverged on the bool row");
    assert_eq!(got["changed"], true);
    assert_eq!(got["applied"], "reload-ale");
    let rust_bytes = fs::read(rust_lua("UnlimitedAmmo.lua")).expect("read rust lua");
    let cli_bytes = fs::read(cli_lua("UnlimitedAmmo.lua")).expect("read cli lua");
    assert_eq!(
        rust_bytes,
        cli_bytes,
        "bool-row write diverged\nrust:\n{}\ncli:\n{}",
        String::from_utf8_lossy(&rust_bytes),
        String::from_utf8_lossy(&cli_bytes)
    );
    assert_ne!(rust_bytes, UNLIMITED_AMMO_LUA.as_bytes(), "the file must actually have changed");

    // --- edit 2: an INT row whose line carries an inline comment ------------
    let got = native_tuning_set(&rust_title_dir, "unlimitedammo.max_ammo", "007")
        .expect("native tuning_set (int, leading zeros)");
    let want =
        run_cli_tuning_set(&bash, &script, &cli_games_dir, &yq, &path, "unlimitedammo.max_ammo", "007");
    assert_eq!(want["ok"], true, "cli tuning-set not ok: {want}");
    assert_eq!(got, want["data"], "envelope data diverged on the int row");
    let rust_bytes = fs::read(rust_lua("UnlimitedAmmo.lua")).expect("read rust lua");
    let cli_bytes = fs::read(cli_lua("UnlimitedAmmo.lua")).expect("read cli lua");
    assert_eq!(
        rust_bytes,
        cli_bytes,
        "int-row write (leading-zero normalization + inline comment) diverged\nrust:\n{}\ncli:\n{}",
        String::from_utf8_lossy(&rust_bytes),
        String::from_utf8_lossy(&cli_bytes)
    );
    // Both sides must have normalized "007" to "7" and kept the comment.
    assert!(
        String::from_utf8_lossy(&rust_bytes).contains("MAX_AMMO = 7 -- how many to keep stocked"),
        "expected the normalized value and preserved comment, got:\n{}",
        String::from_utf8_lossy(&rust_bytes)
    );

    // --- edit 3: an indented bare table key with a trailing comma -----------
    let got =
        native_tuning_set(&rust_title_dir, "sitmeansrest.duration", "45").expect("native tuning_set (table key)");
    let want =
        run_cli_tuning_set(&bash, &script, &cli_games_dir, &yq, &path, "sitmeansrest.duration", "45");
    assert_eq!(want["ok"], true, "cli tuning-set not ok: {want}");
    assert_eq!(got, want["data"], "envelope data diverged on the table-key row");
    let rust_bytes = fs::read(rust_lua("SitMeansRest.lua")).expect("read rust lua");
    let cli_bytes = fs::read(cli_lua("SitMeansRest.lua")).expect("read cli lua");
    assert_eq!(
        rust_bytes,
        cli_bytes,
        "table-key write diverged\nrust:\n{}\ncli:\n{}",
        String::from_utf8_lossy(&rust_bytes),
        String::from_utf8_lossy(&cli_bytes)
    );

    // --- edit 4: the NO-OP case (same value again) --------------------------
    let got = native_tuning_set(&rust_title_dir, "sitmeansrest.duration", "45").expect("native no-op");
    let want =
        run_cli_tuning_set(&bash, &script, &cli_games_dir, &yq, &path, "sitmeansrest.duration", "45");
    assert_eq!(want["ok"], true, "cli tuning-set (no-op) not ok: {want}");
    assert_eq!(got, want["data"], "envelope data diverged on the no-op");
    assert_eq!(got["changed"], false);
    assert_eq!(got["applied"], "none");
    assert_eq!(
        fs::read(rust_lua("SitMeansRest.lua")).unwrap(),
        rust_bytes,
        "the no-op must leave the file byte-identical"
    );
    assert_eq!(fs::read(cli_lua("SitMeansRest.lua")).unwrap(), cli_bytes, "same on the cli side");

    // --- edit 5: a key the installed script does NOT carry ------------------
    // Both sides must report the oracle's NOT_FOUND (not WRITE_FAILED) and
    // leave the file untouched. `REGEN_AURA` is removed from the rust-side and
    // cli-side scripts identically first.
    for p in [rust_lua("SitMeansRest.lua"), cli_lua("SitMeansRest.lua")] {
        let text = fs::read_to_string(&p).unwrap();
        let stripped: String = text
            .split_inclusive('\n')
            .filter(|l| !l.contains("REGEN_AURA"))
            .collect();
        fs::write(&p, stripped).unwrap();
    }
    let before = fs::read(rust_lua("SitMeansRest.lua")).unwrap();
    assert_eq!(before, fs::read(cli_lua("SitMeansRest.lua")).unwrap(), "stripped fixtures must match");

    let err = native_tuning_set(&rust_title_dir, "sitmeansrest.regen_aura", "1")
        .expect_err("native tuning_set must fail on an absent key");
    let want =
        run_cli_tuning_set(&bash, &script, &cli_games_dir, &yq, &path, "sitmeansrest.regen_aura", "1");
    assert_eq!(want["ok"], false, "cli should have failed too: {want}");
    let want_err = format!(
        "{}|{}|{}",
        want["error"]["code"].as_str().unwrap_or(""),
        want["error"]["message"].as_str().unwrap_or(""),
        want["error"]["hint"].as_str().unwrap_or("")
    );
    assert_eq!(err, want_err, "absent-key error diverged");
    assert!(err.starts_with("NOT_FOUND|"), "expected NOT_FOUND, got: {err}");
    assert_eq!(fs::read(rust_lua("SitMeansRest.lua")).unwrap(), before, "failed write must not touch the file");
    assert_eq!(fs::read(cli_lua("SitMeansRest.lua")).unwrap(), before, "same on the cli side");

    // --- cleanup: never leave temp fixtures behind --------------------------
    let _ = fs::remove_dir_all(&tmp_base);
}

/// The lua branch's NOT_INSTALLED arm: the deployed script is absent entirely
/// (this module family has no `.dist` to seed from, unlike the conf backend).
/// Pure-Rust, no bash needed — the oracle's text is pinned here so a reworded
/// message shows up as a test failure rather than a silent GUI copy change.
#[test]
fn native_lua_tuning_write_reports_not_installed_without_the_script() {
    let base = std::env::temp_dir().join(format!("dml_tunewrite_noinstall_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let title_dir = base.join("wow-server-playerbots");
    fs::create_dir_all(&title_dir).unwrap();
    fs::write(
        title_dir.join("docker-compose.yml"),
        "services:\n  ac-worldserver:\n    image: dummy\n",
    )
    .unwrap();

    let err = native_tuning_set(&title_dir, "unlimitedammo.enabled", "1")
        .expect_err("an undeployed lua script must fail");
    assert_eq!(
        err,
        "NOT_INSTALLED|Unlimited Ammo is not installed|Install Unlimited Ammo from the Modules page (Lua scripts) first, then reopen this page."
    );
    let _ = fs::remove_dir_all(&base);
}

/// A not-installed TITLE outranks everything, for the lua backend too: the
/// oracle resolves `_wow_server_dir` after validation but BEFORE either
/// backend branch (`90-main.sh:2888-2892`), so this reports the uniform
/// "server not installed", never the module-level NOT_INSTALLED above.
#[test]
fn native_lua_tuning_write_reports_uninstalled_server_first() {
    let base = std::env::temp_dir().join(format!("dml_tunewrite_nosrv_{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let title_dir = base.join("wow-server-playerbots");
    fs::create_dir_all(&title_dir).unwrap(); // exists, but carries no compose file

    let err = native_tuning_set(&title_dir, "unlimitedammo.enabled", "1")
        .expect_err("a title with no compose file is not installed");
    assert_eq!(
        err,
        "NOT_FOUND|WoW Playerbots server not installed|Install it first, then re-run."
    );
    // ...and a BAD value still loses to validation, which the oracle runs
    // first of all (`90-main.sh:2872-2887`).
    let err = native_tuning_set(&title_dir, "unlimitedammo.enabled", "7")
        .expect_err("a bad bool must fail validation");
    assert_eq!(err, "BAD_ARG|Enable unlimited ammo takes 1 (on) or 0 (off), got: 7|");
    let _ = fs::remove_dir_all(&base);
}
