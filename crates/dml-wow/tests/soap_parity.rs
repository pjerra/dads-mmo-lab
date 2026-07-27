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
//!
//! LEAK-PROOF CLEANUP. A plain "delete at the bottom of the function" only
//! runs when every assertion above it passes -- a panicking `assert_eq!`, a
//! `#[should_panic]`-less failure, or the process getting interrupted mid-test
//! all skip straight past it and leave the throwaway account behind (this
//! happened live: seven `__dmlpar*` orphans accumulated on the auth DB across
//! 2026-07-25..27). [`ThrowawayAccountGuard`] below is an RAII guard --
//! Rust's `Drop` runs during a panic's unwind too, so tying the delete to a
//! guard's destructor makes cleanup unconditional. No `scopeguard` dependency
//! (none is available in this workspace) -- a plain hand-rolled guard is the
//! idiomatic equivalent for one call site.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_wow::soap::{exec, SoapConfig, SoapOutcome};

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
    // Result/fault TEXT parity is compared trailing-newline-normalized. The
    // raw SOAP `<result>`/`<faultstring>` body ends console output with a
    // trailing line-terminator, and the reqwest-decoded body (Rust) vs curl's
    // (the CLI) differ by exactly that trailing `\n` — a cosmetic transport
    // artifact, not a divergence in our extraction (`extract_after` mirrors
    // bash's `${x#*<t>}`/`%%</t>*}` exactly, verified live). It is not
    // load-bearing: the typed write commands (account/gm/mail/teleport)
    // DISCARD the success text and answer with a fixed JSON shape, so the only
    // place this text ever surfaces is console-send / fault display, where a
    // trailing newline is irrelevant. Compare the substantive content.
    match rust_outcome {
        SoapOutcome::Ok(text) => {
            assert_eq!(cli["ok"], true, "CLI soap-exec {cmd:?} disagreed (not ok) while Rust got Ok: {cli}");
            let cli_result = cli["data"]["result"].as_str().unwrap_or_default();
            assert_eq!(cli_result.trim_end(), text.trim_end(), "soap-exec {cmd:?} result text diverged from the CLI");
            true
        }
        SoapOutcome::Fault(text) => {
            assert_eq!(cli["ok"], false, "CLI soap-exec {cmd:?} disagreed (ok) while Rust got Fault: {cli}");
            assert_eq!(cli["error"]["code"], "SOAP_FAULT", "fault code diverged for {cmd:?}: {cli}");
            let cli_msg = cli["error"]["message"].as_str().unwrap_or_default();
            assert_eq!(cli_msg.trim_end(), text.trim_end(), "soap-exec {cmd:?} fault text diverged from the CLI");
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

/// Fires `account delete <user>` — the one command both the guard's `Drop`
/// and the test's own explicit happy-path cleanup need, factored out so
/// there is exactly one place that builds that command string.
fn delete_account(cfg: &SoapConfig, user: &str) -> SoapOutcome {
    exec(cfg, &format!("account delete {user}"))
}

/// RAII guard for the throwaway `__dmlpar<pid>` SOAP account: deletes it when
/// dropped, unless [`ThrowawayAccountGuard::cleanup_now`] already did so.
/// Construct it as soon as the account name is decided (BEFORE the account
/// is even created) so every assertion downstream — including ones that
/// panic — unwinds through this guard's `Drop` and gets a delete attempt.
///
/// `user` is `Some` until cleaned up once (either explicitly via
/// `cleanup_now`, or implicitly via `Drop`); taking it on the way out makes
/// a second delete attempt a no-op instead of firing twice (which would
/// otherwise print a spurious "already doesn't exist" warning on every
/// normal, non-panicking run, since the happy path already deletes the
/// account explicitly and asserts on the result).
///
/// `drop()` MUST NOT panic: if it fires while a test's own assertion is
/// already unwinding, a second panic during that unwind would ABORT THE
/// WHOLE PROCESS instead of just failing the one test — strictly worse than
/// a leaked throwaway account. So `exec` is additionally wrapped in
/// `catch_unwind` as defense-in-depth (it has no known panic path today —
/// every fallible step inside it maps to `SoapOutcome::Unreachable` — but a
/// future change to `exec` could add one), and ANY failure to delete
/// (fault, unreachable, or a caught panic) is reported via `eprintln!` and
/// swallowed, never propagated.
struct ThrowawayAccountGuard<'a> {
    cfg: &'a SoapConfig,
    user: Option<String>,
}

impl<'a> ThrowawayAccountGuard<'a> {
    fn new(cfg: &'a SoapConfig, user: String) -> Self {
        Self { cfg, user: Some(user) }
    }

    /// Explicit happy-path cleanup: delete now, return the outcome so the
    /// caller can still assert on it (same as the old inline cleanup did),
    /// and disarm the guard so `Drop` doesn't try again afterward.
    fn cleanup_now(&mut self) -> SoapOutcome {
        let user = self.user.take().expect("cleanup_now called at most once");
        delete_account(self.cfg, &user)
    }
}

impl Drop for ThrowawayAccountGuard<'_> {
    fn drop(&mut self) {
        let Some(user) = self.user.take() else {
            return; // already cleaned up via `cleanup_now`
        };
        let cfg = self.cfg;
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| delete_account(cfg, &user)));
        match result {
            Ok(SoapOutcome::Ok(_)) => {}
            Ok(other) => eprintln!(
                "WARNING: soap_parity Drop-guard could not delete throwaway account {user} ({other:?}) -- it may be left over; delete manually via SOAP `account delete {user}` if so"
            ),
            Err(_) => eprintln!(
                "WARNING: soap_parity Drop-guard PANICKED deleting throwaway account {user} -- it may be left over; delete manually via SOAP `account delete {user}` if so"
            ),
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

    // 1. `server info` — read-only. Its output carries VOLATILE fields
    // (`Update time diff`, `Server uptime`) that change between the Rust call
    // and the separate CLI call microseconds later, so compare CLASSIFICATION
    // only here (both must succeed). Exact result-TEXT parity — the
    // load-bearing guarantee for A2b/A2c's writes — is asserted below on the
    // DETERMINISTIC account create/delete, whose output ("Account created.")
    // is fixed.
    let rust_info = exec(&cfg, "server info");
    let cli_info = run_soap_exec_cli(&bash, &script, &games, "server info");
    assert!(
        matches!(rust_info, SoapOutcome::Ok(_)),
        "Rust server info should classify Ok on a reachable server, got {rust_info:?}"
    );
    assert_eq!(
        cli_info["ok"], true,
        "CLI server info should be ok on a reachable server: {cli_info}"
    );

    // 2. Exact result-TEXT parity on a DETERMINISTIC, repeatable command.
    // `assert_parity` fires the command through BOTH clients, so it must be
    // idempotent in RESULT. A FRESH `account create` is NOT: the first client
    // succeeds and the second then faults "already exist". A DUPLICATE create
    // is: `account create <existing>` faults "Account with this name already
    // exist!" every time with no state change. So: create the throwaway once
    // (single-fire via Rust), then compare the duplicate-create fault text
    // across both clients, then clean up. This exercises the Ok path (setup),
    // the Fault path + exact fault text (the compared dup-create), and delete
    // (cleanup). `std::process::id()` gives per-run uniqueness.
    let user = format!("__dmlpar{}", std::process::id());
    let pass = "Parity1!";
    let create_cmd = format!("account create {user} {pass}");

    // Guard the throwaway account from HERE ON: every assertion below --
    // setup, the parity comparison, or an interruption -- unwinds through
    // this guard's `Drop` and gets a best-effort delete attempt, so the
    // account can never outlive this test no matter how it exits. See
    // `ThrowawayAccountGuard` above for the full rationale and why `Drop`
    // itself is written to never panic.
    let mut account_guard = ThrowawayAccountGuard::new(&cfg, user.clone());

    // Ensure the account EXISTS (the precondition for the duplicate-create
    // parity check below). Best-effort pre-clean, then create. EITHER outcome
    // is a valid setup state: Ok = we just created it; a Fault containing
    // "already exist" = a prior run left it behind (Windows reuses PIDs across
    // the session's many parity runs, and SOAP account-delete can lag) -- the
    // account still exists, which is all the dup-create check needs. Only a
    // genuinely different outcome (Auth/Unreachable/other Fault) is a failure.
    let _ = delete_account(&cfg, &user);
    let setup = exec(&cfg, &create_cmd);
    let exists = match &setup {
        SoapOutcome::Ok(_) => true,
        SoapOutcome::Fault(t) => t.to_lowercase().contains("already exist"),
        _ => false,
    };
    assert!(
        exists,
        "setup: throwaway account {user} should be creatable or already exist, got {setup:?}"
    );

    // Duplicate create -> identical Fault + identical fault TEXT on both.
    let dup_ok = assert_parity(&cfg, &bash, &script, &games, &create_cmd);
    assert!(!dup_ok, "a duplicate account create should Fault on both clients, not Ok");

    // Cleanup (explicit, so the happy path still asserts it worked, same as
    // before this fix). `cleanup_now` also disarms the guard so its `Drop`
    // doesn't attempt a second, redundant delete afterward.
    let cleanup = account_guard.cleanup_now();
    assert!(
        matches!(cleanup, SoapOutcome::Ok(_)),
        "cleanup: deleting throwaway account {user} should succeed -- MANUAL CLEANUP MAY BE NEEDED ({cleanup:?})"
    );
}
