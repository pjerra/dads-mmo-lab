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
//! LINE ENDINGS ARE PART OF THE CONTRACT — see
//! `native_lua_tuning_write_matches_cli_for_odd_line_endings` below for the
//! measured evidence that CRLF `.lua` scripts really do land in
//! `lua_scripts/`, and for the platform-dependent oracle behaviour that
//! comparison uncovered.
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
    // Fallback probes are per-platform: Git Bash on Windows, the system
    // shell elsewhere. Probing ONLY the Windows locations made every Linux
    // run of this suite skip silently, even with bash at /usr/bin/bash.
    #[cfg(windows)]
    let candidates = [r"C:\Program Files\Git\bin\bash.exe", r"C:\Program Files\Git\usr\bin\bash.exe"];
    #[cfg(not(windows))]
    let candidates = ["/usr/bin/bash", "/bin/bash"];
    for c in candidates {
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
    // Windows probes the fixed dev-box location; Linux probes where a distro
    // package puts yq, so this OFFLINE suite can actually run there instead
    // of skipping forever on a path that can never exist.
    #[cfg(windows)]
    let candidates = [PathBuf::from(r"C:\Users\perzi\dml-native").join("tools").join("yq.exe")];
    #[cfg(not(windows))]
    let candidates = [PathBuf::from("/usr/bin/yq"), PathBuf::from("/usr/local/bin/yq")];
    candidates.into_iter().find(|p| p.exists())
}

/// The `awk` the spawned `dml` would itself use. Resolved by scanning the
/// SAME augmented PATH handed to the CLI (which already prepends bash's own
/// toolchain dirs), so it finds Git-Bash's `awk.exe` on Windows and
/// `/usr/bin/awk` on Linux/WSL with one mechanism. `DML_AWK` overrides.
///
/// A `None` here is NOT tolerated by the caller — see the doc comment on the
/// line-ending test: the cross-check this feeds is a hard requirement.
fn find_awk(path: &OsString) -> Option<PathBuf> {
    if let Some(a) = std::env::var_os("DML_AWK").filter(|s| !s.is_empty()) {
        let p = PathBuf::from(a);
        if p.exists() {
            return Some(p);
        }
    }
    for dir in std::env::split_paths(path) {
        for name in ["awk.exe", "awk"] {
            let c = dir.join(name);
            if c.is_file() {
                return Some(c);
            }
        }
    }
    None
}

/// Does the awk we are about to drive actually SEE a `\r` at end of record?
///
/// gawk on Windows opens files in TEXT mode and strips it before the script
/// runs; every Unix awk does not. `length($0)` for a record `A\r` is therefore
/// `1` there and `2` elsewhere. Probed rather than inferred from `cfg!(windows)`
/// or a path shape, because it is the exact property the assertions below
/// depend on — and a wrong guess would turn correct behaviour into a red test.
fn awk_strips_cr(awk: &Path, scratch: &Path) -> bool {
    let probe = scratch.join("cr-probe.txt");
    fs::write(&probe, b"A\r\n").expect("write awk CR probe");
    let out = Command::new(awk)
        .arg("{ print length($0) }")
        .arg(&probe)
        .output()
        .expect("spawn awk for the CR probe");
    let seen = String::from_utf8_lossy(&out.stdout).trim().to_string();
    match seen.as_str() {
        "1" => true,  // TEXT mode: the CR was stripped on read
        "2" => false, // binary/Unix: the CR is part of the record
        other => panic!("awk CR probe returned an unexpected record length {other:?} (stderr: {})", String::from_utf8_lossy(&out.stderr)),
    }
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

/// The line-ending axis — and a RECORDED PLATFORM DIVERGENCE. Read this
/// before changing either implementation.
///
/// ## Why CRLF matters on THIS path
///
/// A correction first, because an earlier version of this comment asserted the
/// opposite and was wrong: the `.conf` files are **not** CRLF. Measured on this
/// box, every `.conf`/`.conf.dist` under
/// `dml-native/wow-server-playerbots/env/dist/etc` has ZERO CR bytes, and
/// `playerbots.conf` is md5-identical to its `.dist`
/// (`a0aafcae552044dcd1da2d4e78cf2877`) — it was LF from install and has never
/// been rewritten.
///
/// The `.lua` scripts are a different story, and they are exactly what this
/// writer edits. Measured in the live `lua_scripts/` directory, **6 of the 13
/// deployed `.lua` files carry CRLF** (`gasino/30_gasino_host.lua` has 323 CR,
/// `31_gasino_blackjack.lua` 199, `20_gasino_vault.lua` 116, and three more).
/// The mechanism is structural, not an accident:
///
///   - ALE lua modules install by `git clone --depth 1` (70-modules.sh:423)
///     followed by a plain `cp` into `lua_scripts/` (e.g. :476, :486 — the
///     SitMeansRest and UnlimitedAmmo files two of this suite's rows target);
///   - in native mode that clone runs under Git Bash, whose SYSTEM gitconfig
///     sets `core.autocrlf=true` (verified:
///     `file:C:/Program Files/Git/etc/gitconfig  true`), so a cloned `.lua`
///     lands CRLF unless upstream ships a `.gitattributes`;
///   - this repo's own `.gitattributes` scopes its `eol=lf` lua rule to
///     `cli/lua/**` deliberately, and says why: the upstream reference copies
///     under `guides/` carry CRLF. They measurably do —
///     `guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse/BMAH.lua` has 605 CR
///     and `SeasonOfDiscovery/SOD.lua` has 96 — and the bmah/sod installers
///     copy those straight into `lua_scripts/`.
///
/// IMPACT SCOPE, stated honestly: this is round-trip / diff-noise stability,
/// NOT corruption. AzerothCore's conf parser and Lua both accept LF, CRLF and
/// mixed. What is at stake is whether one knob edit produces a one-line diff
/// or rewrites every line of the file (and the backup churn that implies).
///
/// ## The divergence
///
/// The oracle is awk, with its own `\r` handling in `is_key_line`
/// (`sub(/\r$/,"",s)`) and in `rebuild` (capture `cr`, re-append it), plus a
/// record model that re-adds `ORS` to a final partial record.
///
/// FINDING (measured here, 2026-07-27): **GNU Awk on Windows opens files in
/// TEXT mode**, so under Git Bash it strips `\r` on read before the script
/// ever sees it (`length($0)` is 5, not 6, for `A = 1\r`). The oracle's own
/// `\r`-preservation code is therefore DEAD in that environment, and any
/// `_lua_cfg_write` edit rewrites the WHOLE file LF-only. Run the exact same
/// awk program with `-v BINMODE=3` — simply how the Linux gawk inside the
/// `dml-arch` WSL distro behaves by default — and the `\r` is visible, that
/// code fires, and the oracle preserves CRLF **byte-for-byte identically to
/// this port**.
///
/// That LF-flattening is REAL PRODUCT BEHAVIOUR, not merely a test artifact:
/// `DmlRunner::native()` drives this same bash script under Git Bash, and
/// native mode (`DML_BACKEND=native`) is a shipped, opt-in product mode. The
/// user-visible effect is that a `bash cli/dml` edit of a CRLF lua script on
/// Windows rewrites every line, where the Rust CLI produces a one-line diff.
///
/// So the port matches the oracle's source semantics and its Linux/WSL
/// behaviour, and differs from its Git-Bash-on-Windows behaviour. This test
/// encodes BOTH rather than papering over either:
///
///   A) against the oracle's own awk in BINARY mode (extracted verbatim from
///      `cli/src/40-config.sh` at test time): **byte-identical**. This is the
///      real parity claim, and it is a HARD requirement — if awk or the
///      program cannot be found, this test PANICS rather than skipping the
///      check, because a silent no-op in the one assertion guarding a write
///      path is worse than a failure.
///   B) against the full bash CLI: what is asserted depends on what the awk
///      being driven actually DOES, probed at runtime (`awk_strips_cr`)
///      rather than assumed from the platform — LF-flattening where awk is in
///      text mode, full byte-parity where it is not. A fixture with no CR at
///      all is held to plain byte-equality either way.
///
/// Four rounds, each seeding both temp trees byte-identically:
///   1. CRLF throughout;
///   2. LF with NO trailing newline (awk adds exactly one) — no CR anywhere,
///      so this round is a strict byte-equality round on every platform;
///   3. CRLF with no trailing newline;
///   4. the sharpest case — CRLF where the EDITED line is the final record and
///      carries a `\r` but no terminator, so `rebuild`'s `cr` capture and
///      awk's `ORS` both have to fire on the same line.
///
/// Deliberately NOT done: changing the Rust to match the Windows-gawk
/// behaviour. That would contradict the awk this was ported from, and would
/// turn every knob edit on a CRLF lua script into a whole-file rewrite.
///
/// KNOWN GAP: check (A) drives the extracted awk program only, so
/// `_lua_cfg_write`'s shell wrapper around it — the tmp-file verify step, the
/// NOT_FOUND arm, the no-op arm — is never exercised on CRLF input. Those are
/// covered on LF input by `native_lua_tuning_write_matches_cli`.
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

    // Past the skip gate: from here on, a missing prerequisite is a FAILURE,
    // never a quiet degradation. `find_awk`/`oracle_awk_program` feed the only
    // assertion that can catch a partial-CR regression, and cargo swallows
    // `eprintln!` without `--nocapture`, so an else-arm here would silently
    // turn this suite's strongest check into a no-op while still reporting
    // green.
    let awk = find_awk(&path).unwrap_or_else(|| {
        panic!(
            "tuning_write_parity: bash+script are present so this suite is RUNNING, but no \
             `awk` was found on the augmented PATH. The binary-mode oracle cross-check cannot \
             be skipped -- set DML_AWK to the awk the `dml` CLI itself would use."
        )
    });
    let oracle_prog = oracle_awk_program().unwrap_or_else(|| {
        panic!(
            "tuning_write_parity: could not extract the awk program from `_lua_cfg_write` in \
             cli/src/40-config.sh. The extraction anchors are the literal `V=\"$3\" awk '` and \
             `' \"$1\" > \"$tmp\"` -- if that function was reformatted, update them here. \
             This is NOT skippable: it is the only byte-exact oracle comparison in the suite."
        )
    });
    let prog_file = tmp_base.join("oracle.awk");
    fs::write(&prog_file, &oracle_prog).expect("write extracted oracle awk");
    // Probed, not assumed -- see `awk_strips_cr`. Reported (not branched on
    // silently) so a `--nocapture` run shows which platform behaviour the
    // assertions below were held to.
    let strips_cr = awk_strips_cr(&awk, &tmp_base);
    eprintln!(
        "tuning_write_parity(line endings): awk={} -> {} (BINMODE=3 cross-check is unconditional)",
        awk.display(),
        if strips_cr { "TEXT mode, strips CR on read" } else { "binary/Unix, CR preserved" }
    );

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

        // (A) THE REAL PARITY CLAIM, and a hard requirement: the oracle's own
        // awk, in binary mode (== a Linux/WSL gawk), byte-for-byte. Both its
        // prerequisites were resolved (or panicked) before the loop.
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

        // (B) the full bash CLI. What to expect depends on what the awk it
        // drives actually does with a CR -- probed, not assumed, so this is
        // correct under Git Bash AND under WSL/Linux (which the yq-gate
        // removal deliberately widened this suite to reach).
        let strip_cr = |b: &[u8]| -> Vec<u8> { b.iter().copied().filter(|c| *c != b'\r').collect() };
        if !fixture.contains('\r') {
            // No CR anywhere: text-mode I/O has nothing to strip, so the two
            // must agree exactly on every platform. (This is what keeps round
            // 2 from being a vacuous CR test.)
            assert_eq!(
                rust_bytes,
                cli_bytes,
                "[{label}] CR-free fixture: the port and the bash CLI must be byte-identical\n\
                 rust: {:?}\ncli:  {:?}",
                String::from_utf8_lossy(&rust_bytes),
                String::from_utf8_lossy(&cli_bytes)
            );
        } else if strips_cr {
            // Text-mode awk (Git Bash on Windows): content must still match,
            // and the documented whole-file LF-flattening must be exactly what
            // happened -- asserted positively so a change is loud.
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
                "[{label}] awk probed as CR-stripping (TEXT mode), so the CLI was expected to \
                 flatten the file to LF -- but it kept a CR. Re-read this test's doc comment: \
                 the recorded platform finding may no longer hold."
            );
        } else {
            // Binary/Unix awk: the oracle's own \r handling is live, so there
            // is no divergence left to tolerate.
            assert_eq!(
                rust_bytes,
                cli_bytes,
                "[{label}] awk probed as CR-preserving, so the port and the bash CLI must be \
                 byte-identical\nrust: {:?}\ncli:  {:?}",
                String::from_utf8_lossy(&rust_bytes),
                String::from_utf8_lossy(&cli_bytes)
            );
        }
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
