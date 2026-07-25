//! Live SOAP parity test (Task A3, spike: `spike/docker-desktop-native`).
//!
//! Compares the native `dml::soap::exec` client (Task A1) against the bash
//! oracle's `dml wow soap-exec "<cmd>"` arm (`cli/src/90-main.sh:1389-1404`)
//! for the SAME command string fired at the SAME live worldserver SOAP
//! listener. This is the load-bearing guarantee behind Task A2b/A2c's native
//! writes: the Rust client classifies (Ok/Fault/Auth/Unreachable) and reports
//! result/fault text exactly like `soap_exec` does.
//!
//! SKIP-GUARDED, not just DB-gated like `db_pages_parity.rs`: the server is
//! DOWN as of this task, so the test probes reachability FIRST via
//! `dml::soap::exec(&SoapConfig::load(), "server info")`. `Unreachable`/`Auth`
//! -> print why and return (pass) -- the suite must stay green with no server
//! running. Only `Ok`/`Fault` on the probe (i.e. SOAP is actually answering)
//! unlocks the real assertions.
//!
//! SAFE/REVERSIBLE commands only, per the brief: `server info` (read-only)
//! and a throwaway `__dmlpar<pid>` account create+delete, cleaned up in the
//! SAME test so nothing persists. NO teleport/gm/mail against real
//! characters -- destructive ops against live characters are out of scope.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use launcher_lib::dml::soap::{exec, SoapConfig, SoapOutcome};

fn games_dir() -> PathBuf {
    std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Users\perzi\dml-native"))
}

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
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..").join("cli").join("dml");
    p.exists().then_some(p)
}

/// Shells `dml wow soap-exec "<cmd>" --json` — same `DML_BACKEND=native` +
/// native games dir env setup `db_pages_parity.rs` uses for its own `dml`
/// invocations (SOAP config resolution itself doesn't consult `DML_BACKEND`
/// — see `20-soap.sh` — but this keeps the harness consistent with the rest
/// of the native-mode parity suite).
fn run_soap_exec_cli(bash: &Path, script: &Path, games: &Path, cmd: &str) -> serde_json::Value {
    let mut c = Command::new(bash);
    c.arg(script).arg("wow").arg("soap-exec").arg(cmd).arg("--json");
    c.env("DML_BACKEND", "native");
    c.env("DML_GAMES_DIR", games);
    let out = c.output().expect("spawn dml under bash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    serde_json::from_str(stdout.trim())
        .unwrap_or_else(|e| panic!("dml soap-exec {cmd:?} output not JSON ({e}): {stdout}"))
}

/// Fires `cmd` through both the native Rust SOAP client and the bash oracle's
/// `soap-exec` arm against the SAME live server, and asserts they classify +
/// report identically (Ok result text / Fault text+code / Auth / Unreachable
/// all compared). Returns `true` when the command succeeded (Ok) on both
/// sides, so callers can gate follow-up steps (e.g. only delete an account
/// that was actually created) on it.
fn assert_parity(cfg: &SoapConfig, bash: &Path, script: &Path, games: &Path, cmd: &str) -> bool {
    let rust_outcome = exec(cfg, cmd);
    let cli = run_soap_exec_cli(bash, script, games, cmd);
    match rust_outcome {
        SoapOutcome::Ok(text) => {
            assert_eq!(cli["ok"], true, "CLI soap-exec {cmd:?} disagreed (not ok) while Rust got Ok: {cli}");
            assert_eq!(cli["data"]["result"], text, "soap-exec {cmd:?} result text diverged from the CLI");
            true
        }
        SoapOutcome::Fault(text) => {
            assert_eq!(cli["ok"], false, "CLI soap-exec {cmd:?} disagreed (ok) while Rust got Fault: {cli}");
            assert_eq!(cli["error"]["code"], "SOAP_FAULT", "fault code diverged for {cmd:?}: {cli}");
            assert_eq!(cli["error"]["message"], text, "soap-exec {cmd:?} fault text diverged from the CLI");
            false
        }
        SoapOutcome::Auth => {
            assert_eq!(cli["ok"], false, "CLI soap-exec {cmd:?} disagreed (ok) while Rust got Auth: {cli}");
            assert_eq!(cli["error"]["code"], "SOAP_AUTH", "auth code diverged for {cmd:?}: {cli}");
            false
        }
        SoapOutcome::Unreachable(_) => {
            assert_eq!(cli["ok"], false, "CLI soap-exec {cmd:?} disagreed (ok) while Rust got Unreachable: {cli}");
            assert_eq!(cli["error"]["code"], "SOAP_UNREACHABLE", "unreachable code diverged for {cmd:?}: {cli}");
            false
        }
    }
}

#[test]
fn soap_parity_when_reachable() {
    let cfg = SoapConfig::load();

    // Reachability probe. This MUST NOT fail the test when the server is
    // down or SOAP auth is misconfigured — the suite stays green offline.
    match exec(&cfg, "server info") {
        SoapOutcome::Unreachable(_) => {
            eprintln!("SKIP soap_parity: server not reachable");
            return;
        }
        SoapOutcome::Auth => {
            eprintln!("SKIP soap_parity: SOAP auth failed (check ~/.dml/soap.env)");
            return;
        }
        _ => {}
    }

    let Some(bash) = find_bash() else {
        eprintln!("SKIP soap_parity: no bash (set DML_BASH)");
        return;
    };
    let bash = PathBuf::from(bash);
    let Some(script) = find_script() else {
        eprintln!("SKIP soap_parity: no dml script (set DML_SCRIPT)");
        return;
    };
    let games = games_dir();

    // 1. `server info` — read-only, both sides Ok with matching text.
    assert!(
        assert_parity(&cfg, &bash, &script, &games, "server info"),
        "server info should succeed on a reachable server"
    );

    // 2. Throwaway account create + delete, cleaned up in this same test.
    // `std::process::id()` gives per-run uniqueness (Date/rand unavailable
    // in workflow scripts, but this is a normal Rust test).
    let user = format!("__dmlpar{}", std::process::id());
    let pass = "Parity1!";
    let create_cmd = format!("account create {user} {pass}");
    let delete_cmd = format!("account delete {user}");

    let created = assert_parity(&cfg, &bash, &script, &games, &create_cmd);
    if created {
        assert!(
            assert_parity(&cfg, &bash, &script, &games, &delete_cmd),
            "cleanup delete of throwaway account {user} should succeed — MANUAL CLEANUP MAY BE NEEDED"
        );
    } else {
        // Nothing was created on either side (both classified the create as
        // Fault/Auth/Unreachable) — best-effort cleanup in case of a partial
        // state, but don't assert on it since there's nothing to compare.
        let _ = exec(&cfg, &delete_cmd);
    }
}
