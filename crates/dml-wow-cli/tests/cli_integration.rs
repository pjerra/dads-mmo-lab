//! Subprocess-level integration tests for the REAL compiled `dml-wow`
//! binary's own process-boundary wiring: argv parsing, exit codes, and the
//! stdout/stderr split. Everything in `cli.rs`/`out.rs` is unit-tested
//! in-process against pure logic; these instead spawn the actual binary via
//! the `CARGO_BIN_EXE_dml-wow` env var cargo sets for every integration test
//! in this package, so a future edit to `main.rs::handle_parse_error` (or
//! the clap tree in `cli.rs`) that silently breaks the 0/1/2 exit-code
//! contract fails a test instead of shipping quietly. (Task 10 review,
//! Minor finding.)
//!
//! Deliberately NOT here: an automated broken-pipe (Finding 2) simulation.
//! All four of today's subcommands emit exactly one line, so a
//! close-the-child's-stdout-handle-immediately trick races the child's own
//! near-instant write+exit and is not reliably able to land on the
//! BrokenPipe branch — the actual OS process-startup overhead usually (but
//! not deterministically) resolves the race, and a flaky regression test is
//! worse than none. The pure `classify_write` decision has direct unit
//! coverage in `out.rs`; the REAL end-to-end broken-pipe path (piping into
//! `head -1`) was verified manually and recorded verbatim in
//! `task-10-report.md`.

use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_dml-wow"))
}

fn parse_envelope(stdout: &[u8]) -> serde_json::Value {
    let text = String::from_utf8_lossy(stdout);
    serde_json::from_str(text.trim())
        .unwrap_or_else(|e| panic!("stdout was not one JSON envelope ({e}): {text:?}"))
}

/// (a) An unknown subcommand is a usage error: exit 2, a BAD_ARGS envelope
/// on stdout, and clap's own message on stderr.
#[test]
fn unknown_subcommand_is_bad_args_exit_2() {
    let out = bin()
        .arg("definitely-not-a-cmd")
        .output()
        .expect("spawn dml-wow definitely-not-a-cmd");

    assert_eq!(out.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&out.stderr));

    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["ok"], false);
    assert_eq!(envelope["error"]["code"], "BAD_ARGS");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unrecognized subcommand"),
        "expected clap's usage message on stderr, got: {stderr:?}"
    );
}

/// (b) `--help` and `--version` are not usage errors — clap's own handling
/// exits 0 for both (not folded into the BAD_ARGS/exit-2 path).
#[test]
fn help_flag_exits_zero() {
    let out = bin().arg("--help").output().expect("spawn dml-wow --help");
    assert_eq!(out.status.code(), Some(0));
    assert!(!out.stdout.is_empty(), "expected clap's help text on stdout");
}

#[test]
fn version_flag_exits_zero() {
    let out = bin().arg("--version").output().expect("spawn dml-wow --version");
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("dml-wow"), "expected clap's own version line, got: {stdout:?}");
}

/// (c) The `version` SUBCOMMAND (distinct from the `--version` FLAG above)
/// emits exactly one ok envelope and exits 0.
#[test]
fn version_subcommand_emits_exactly_one_ok_envelope() {
    let out = bin().arg("version").output().expect("spawn dml-wow version");
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stderr.is_empty(), "expected no stderr, got: {:?}", String::from_utf8_lossy(&out.stderr));

    let stdout = String::from_utf8_lossy(&out.stdout);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 1, "expected exactly one line on stdout, got: {stdout:?}");

    let envelope: serde_json::Value = serde_json::from_str(lines[0]).expect("valid JSON");
    assert_eq!(envelope["ok"], true);
    assert_eq!(envelope["data"]["contract"], "dml-json-v3");
    assert_eq!(envelope["data"]["backend"], "native");
}

/// Bonus, tying Finding 1's range gate to the real process boundary too (not
/// just the in-process clap parse tests in `cli.rs`): an out-of-range
/// `--lines` is a usage error, not a value that reaches `docker logs --tail`.
#[test]
fn console_tail_lines_out_of_range_is_bad_args_exit_2() {
    let out = bin()
        .args(["console-tail", "--lines", "1001"])
        .output()
        .expect("spawn dml-wow console-tail --lines 1001");

    assert_eq!(out.status.code(), Some(2));
    let envelope = parse_envelope(&out.stdout);
    assert_eq!(envelope["error"]["code"], "BAD_ARGS");
}
