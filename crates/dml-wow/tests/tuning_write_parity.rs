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
//! LINE ENDINGS ARE PART OF THE CONTRACT. The deployed files on a real box are
//! CRLF (the lua scripts and every `.conf` under `env/dist/etc`), and the
//! oracle is awk — which has its own `\r` handling in both `is_key_line` and
//! `rebuild`. A Rust-only CRLF test proves nothing about whether the two
//! AGREE, so `native_lua_tuning_write_matches_cli_for_odd_line_endings` runs
//! the same oracle comparison over CRLF, no-trailing-newline, and both
//! combined — including the sharpest case, where the edited line IS the final
//! record and has no terminator.
//!
//! FILE/TOOL-GATED, same skip convention as its siblings: skips (and passes)
//! when `bash` or the `dml` script can't be found, or when the temp fixtures
//! can't be prepared. Overridable via `DML_BASH` / `DML_SCRIPT`. NOTE: unlike
//! `config_write_parity.rs`, `yq` is NOT required and NOT part of the gate —
//! the oracle's `tuning-set` arm never calls `_cfg_preamble` (the only thing
//! that needs yq), so gating on it would only narrow where this suite runs,
//! and a skip is a silent pass. `DML_YQ_BIN` is still forwarded when it
//! happens to be available, which costs nothing.

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

/// Best-effort only — see the module doc comment: the `tuning-set` arm needs
/// no `yq`, so a `None` here is NOT a reason to skip. Forwarded when present
/// purely so the spawned `dml` behaves identically to a normal invocation.
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

/// The `awk` the spawned `dml` would itself use — probed next to `bash` so
/// the binary-mode cross-check below runs the SAME interpreter, just without
/// Windows text-mode I/O. `None` skips only that cross-check.
fn find_awk(bash: &Path) -> Option<PathBuf> {
    let bin = bash.parent()?;
    for c in [bin.join("awk.exe"), bin.parent()?.join("usr").join("bin").join("awk.exe")] {
        if c.exists() {
            return Some(c);
        }
    }
    None
}

/// The oracle's awk program, lifted VERBATIM out of `_lua_cfg_write` at test
/// time — everything between its `K="$2" V="$3" awk '` line and the closing
/// `' "$1" > "$tmp"` line. Extracting it (rather than pasting a copy here)
/// means the cross-check below can never silently test a stale snapshot: edit
/// the shell function and this test picks the change up on the next run.
fn oracle_awk_program() -> Option<String> {
    let sh = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("cli")
            .join("src")
            .join("40-config.sh"),
    )
    .ok()?;
    let after = sh.split_once("V=\"$3\" awk '")?.1;
    let body = after.split_once("' \"$1\" > \"$tmp\"")?.0;
    (!body.trim().is_empty()).then(|| body.to_string())
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
    yq: Option<&Path>,
    path: &OsString,
    key: &str,
    value: &str,
) -> serde_json::Value {
    let mut cmd = Command::new(bash);
    cmd.arg(script)
        .args(["wow", "config", "tuning-set", "--key", key, "--value", value, "--json"]);
    cmd.env("DML_GAMES_DIR", games_dir);
    if let Some(yq) = yq {
        cmd.env("DML_YQ_BIN", yq);
    }
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
    // NOT gated on yq — see the module doc comment.
    let yq = find_yq();
    let yq = yq.as_deref();
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
    let want = run_cli_tuning_set(&bash, &script, &cli_games_dir, yq, &path, "unlimitedammo.enabled", "1");
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
        run_cli_tuning_set(&bash, &script, &cli_games_dir, yq, &path, "unlimitedammo.max_ammo", "007");
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
        run_cli_tuning_set(&bash, &script, &cli_games_dir, yq, &path, "sitmeansrest.duration", "45");
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
        run_cli_tuning_set(&bash, &script, &cli_games_dir, yq, &path, "sitmeansrest.duration", "45");
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
        run_cli_tuning_set(&bash, &script, &cli_games_dir, yq, &path, "sitmeansrest.regen_aura", "1");
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

/// The line-ending axis — and a RECORDED DIVERGENCE. Read this before
/// changing either implementation.
///
/// The shape a real box actually has is CRLF: the deployed lua scripts and
/// every `.conf` under `env/dist/etc` (`playerbots.conf` is 2366/2366 CRLF).
/// The oracle is awk, with its own `\r` handling in `is_key_line`
/// (`sub(/\r$/,"",s)`) and in `rebuild` (capture `cr`, re-append it), plus a
/// record model that re-adds `ORS` to a final partial record. So this axis
/// needs a real comparison, not just the Rust-side unit tests.
///
/// FINDING (measured here, 2026-07-27): **GNU Awk on Windows opens files in
/// TEXT mode**, so under Git Bash it strips `\r` on read before the script
/// ever sees it (`length($0)` is 5, not 6, for `A = 1\r`). The oracle's own
/// `\r`-preservation code is therefore DEAD in that environment, and any
/// `_lua_cfg_write` edit silently rewrites the WHOLE file LF-only. Run the
/// exact same awk program with `-v BINMODE=3` — which is simply how the Linux
/// gawk inside the `dml-arch` WSL distro (where the CLI actually ships)
/// behaves by default — and the `\r` is visible, that code fires, and the
/// oracle preserves CRLF **byte-for-byte identically to this port**.
///
/// So the port matches the oracle's source semantics AND its production
/// behaviour; it differs only from the Git-Bash-on-Windows harness artifact.
/// This test encodes BOTH facts rather than papering over either:
///
///   A) against the oracle's own awk in BINARY mode (extracted verbatim from
///      `cli/src/40-config.sh` at test time): **byte-identical**. This is the
///      real parity claim.
///   B) against the full Git-Bash CLI: identical after normalizing line
///      endings, AND the known LF-flattening is asserted POSITIVELY — if the
///      harness ever gains binary-mode awk (or this suite runs under WSL),
///      that assertion fails and whoever sees it is pointed back here.
///
/// Four rounds, each seeding both temp trees byte-identically:
///   1. CRLF throughout;
///   2. LF with NO trailing newline (awk adds exactly one);
///   3. CRLF with no trailing newline;
///   4. the sharpest case — CRLF where the EDITED line is the final record and
///      carries a `\r` but no terminator, so `rebuild`'s `cr` capture and
///      awk's `ORS` both have to fire on the same line.
///
/// Deliberately NOT done: changing the Rust to match the Windows-gawk
/// artifact. That would make one tuning edit rewrite all 2366 lines of a real
/// conf, and would contradict the awk the port was ported from.
#[test]
fn native_lua_tuning_write_matches_cli_for_odd_line_endings() {
    let Some(bash) = find_bash() else {
        eprintln!("SKIP tuning_write_parity(line endings): prereqs absent (no bash, set DML_BASH)");
        return;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP tuning_write_parity(line endings): prereqs absent (no dml script, set DML_SCRIPT)");
        return;
    };
    let yq = find_yq();
    let yq = yq.as_deref();
    let path = augmented_path(&bash);

    let tmp_base =
        std::env::temp_dir().join(format!("dml_tunewrite_eol_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp_base);
    let rust_games_dir = tmp_base.join("rust-side");
    let cli_games_dir = tmp_base.join("cli-side");

    let rust_title_dir = match seed_title_dir(&rust_games_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP tuning_write_parity(line endings): prereqs absent (seed rust-side: {e})");
            return;
        }
    };
    let cli_title_dir = match seed_title_dir(&cli_games_dir) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("SKIP tuning_write_parity(line endings): prereqs absent (seed cli-side: {e})");
            let _ = fs::remove_dir_all(&tmp_base);
            return;
        }
    };
    assert_ne!(rust_title_dir, real_title_dir(), "rust-side fixture must not be the real title dir");
    assert_ne!(cli_title_dir, real_title_dir(), "cli-side fixture must not be the real title dir");

    // (label, fixture bytes, value to write). Each round re-seeds BOTH trees
    // with the same bytes, so rounds are independent.
    let rounds: [(&str, &str, &str); 4] = [
        (
            "CRLF throughout",
            "-- SitMeansRest\r\nlocal CONFIG = {\r\n    DURATION = 20, -- seconds\r\n    REGEN_AURA = 25990,\r\n}\r\n",
            "45",
        ),
        (
            "LF, no trailing newline",
            "-- SitMeansRest\nlocal CONFIG = {\n    DURATION = 20, -- seconds\n    REGEN_AURA = 25990,\n}",
            "46",
        ),
        (
            "CRLF, no trailing newline",
            "-- SitMeansRest\r\nlocal CONFIG = {\r\n    DURATION = 20, -- seconds\r\n    REGEN_AURA = 25990,\r\n}",
            "47",
        ),
        (
            "CRLF, edited line IS the final unterminated record",
            "-- SitMeansRest\r\nlocal CONFIG = {\r\n    REGEN_AURA = 25990,\r\n    DURATION = 20, -- seconds\r",
            "48",
        ),
    ];

    for (label, fixture, value) in rounds {
        let rust_file = lua_path_in(&rust_title_dir, "SitMeansRest.lua");
        let cli_file = lua_path_in(&cli_title_dir, "SitMeansRest.lua");
        fs::write(&rust_file, fixture.as_bytes()).expect("seed rust-side fixture");
        fs::write(&cli_file, fixture.as_bytes()).expect("seed cli-side fixture");
        assert_eq!(
            fs::read(&rust_file).unwrap(),
            fs::read(&cli_file).unwrap(),
            "[{label}] fixtures must start byte-identical"
        );

        let got = native_tuning_set(&rust_title_dir, "sitmeansrest.duration", value)
            .unwrap_or_else(|e| panic!("[{label}] native tuning_set failed: {e}"));
        let want = run_cli_tuning_set(
            &bash,
            &script,
            &cli_games_dir,
            yq,
            &path,
            "sitmeansrest.duration",
            value,
        );
        assert_eq!(want["ok"], true, "[{label}] cli tuning-set not ok: {want}");
        assert_eq!(got, want["data"], "[{label}] envelope data diverged");
        assert_eq!(got["changed"], true, "[{label}] expected a real change");

        let rust_bytes = fs::read(&rust_file).expect("read rust lua");
        let cli_bytes = fs::read(&cli_file).expect("read cli lua");
        assert_ne!(rust_bytes, fixture.as_bytes(), "[{label}] the file must actually have changed");

        // (A) THE REAL PARITY CLAIM: the oracle's own awk, in binary mode
        // (== a Linux/WSL gawk), byte-for-byte.
        if let (Some(awk), Some(prog)) = (find_awk(&bash), oracle_awk_program()) {
            let prog_file = tmp_base.join("oracle.awk");
            fs::write(&prog_file, &prog).expect("write extracted oracle awk");
            let src = tmp_base.join("oracle-input.lua");
            fs::write(&src, fixture.as_bytes()).expect("write oracle input");
            let out = Command::new(&awk)
                .args(["-v", "BINMODE=3", "-f"])
                .arg(&prog_file)
                .arg(&src)
                .env("K", "DURATION")
                .env("V", value)
                .output()
                .expect("spawn the oracle awk in binary mode");
            assert!(out.status.success(), "[{label}] oracle awk failed: {out:?}");
            assert_eq!(
                out.stdout,
                rust_bytes,
                "[{label}] the port and the ORACLE'S OWN AWK (binary mode) produced different bytes\n\
                 oracle: {:?}\nrust:   {:?}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&rust_bytes)
            );
        } else {
            eprintln!("[{label}] NOTE: binary-mode oracle-awk cross-check skipped (no awk / could not extract the program)");
        }

        // (B) the full Git-Bash CLI: same content, and exactly the known
        // line-ending divergence -- see this test's doc comment.
        let strip_cr = |b: &[u8]| -> Vec<u8> { b.iter().copied().filter(|c| *c != b'\r').collect() };
        assert_eq!(
            strip_cr(&rust_bytes),
            strip_cr(&cli_bytes),
            "[{label}] the port and the bash CLI disagree on CONTENT, not merely line endings\n\
             rust: {:?}\ncli:  {:?}",
            String::from_utf8_lossy(&rust_bytes),
            String::from_utf8_lossy(&cli_bytes)
        );
        assert!(
            !cli_bytes.contains(&b'\r'),
            "[{label}] the Git-Bash CLI unexpectedly PRESERVED a CR. That is the good \
             outcome, but it means the recorded Windows-gawk TEXT-mode finding in this \
             test's doc comment no longer holds -- re-read it, then tighten this round \
             back to a plain byte-equality assertion."
        );
        assert_eq!(
            rust_bytes.contains(&b'\r'),
            fixture.contains('\r'),
            "[{label}] the port must PRESERVE the input's line endings (the oracle's own \
             awk does too, in binary mode)"
        );

        // Whatever each side produced, it must still read back as the value we
        // asked for — a pair of agreeing-but-WRONG files would otherwise pass.
        for (who, bytes) in [("rust", &rust_bytes), ("cli", &cli_bytes)] {
            let text = String::from_utf8_lossy(bytes);
            assert_eq!(
                dml_wow::tuning::lua_cfg_read(&text, "DURATION"),
                value,
                "[{label}] the {who} file does not read back as {value}: {text:?}"
            );
        }
    }

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
