//! The envelope/NDJSON output contract every `dml-wow` subcommand shares.
//!
//! Every subcommand prints EXACTLY ONE `{ok,...}` envelope on stdout (via
//! [`emit_ok`]/[`emit_err`]) or, for a long-running one, an NDJSON event
//! stream (via [`stream_sink`]) ending in a terminal `done`/`error` event —
//! the same vocabulary `dml_core::runner::run_stream`'s own (private)
//! `is_terminal` check uses. [`TerminalSeen`]/[`stream_exit`] let a streaming
//! dispatch arm track which terminal event ended the stream and turn that
//! into the process exit code, the same way [`emit_ok`]/[`emit_err`] do for
//! the single-envelope case.
//!
//! This module is the template Tasks 11-15's write/streaming subcommands
//! lean on — keep it dependency-free beyond `dml_core`/`serde_json`.
//!
//! The streaming half ([`stream_sink`]/[`TerminalSeen`]/[`stream_exit`]) was
//! built ahead of its first caller, in Task 10, for Tasks 11-15's write/
//! long-running subcommands to wire into their dispatch arms, same
//! convention as `dml_core::backend`/`dml_wow::db`. `party preset-load`
//! (Task 13, `run.rs::dispatch_party`'s `PresetLoad` arm) is that first
//! caller now.

use std::cell::Cell;
use std::io::{self, Write};

use serde_json::Value;

/// Render the ok-envelope line for `data` — split out from [`emit_ok`] so
/// tests can assert on the exact string without capturing process stdout.
fn ok_line(data: Value) -> String {
    dml_core::envelope::ok_envelope(data).to_string()
}

/// Render the error-envelope line for `(code, message, hint)` — split out
/// from [`emit_err`] for the same reason as [`ok_line`].
fn err_line(code: &str, message: &str, hint: &str) -> String {
    dml_core::envelope::error_envelope(code, message, hint).to_string()
}

// -- panic-free line writes -------------------------------------------------
//
// `println!`/`eprintln!` panic internally on ANY write error, including a
// downstream consumer closing the pipe early (`ErrorKind::BrokenPipe` --
// EPIPE on Unix, ERROR_BROKEN_PIPE/ERROR_NO_DATA on Windows, both surfaced by
// Rust the same way). `dml-wow console-tail --lines 1000 | head -1` is a
// normal, successful interaction -- the consumer got what it wanted and
// stopped reading -- so it must not produce a Rust backtrace and exit 101,
// which would contradict the clean 0/1/2 exit-code contract this module
// exists to establish (Task 10 review Finding 2). `classify_write` is the
// pure decision at the center of that: given a write's `io::Result`, what
// should happen next. Kept separate from the actual `io::stdout()`/`io::stderr()`
// calls so it can be unit-tested directly with synthetic `io::Error`s --
// there is no portable, non-flaky way to force a real OS pipe into a broken
// state from inside a single test process (the write race is one line, not a
// stream, so closing a spawned child's stdout handle can just as easily lose
// the race against the child's own near-instant write+exit); see the test
// module below for the values actually exercised.
#[derive(Debug, PartialEq, Eq)]
enum WriteOutcome {
    /// The write (and flush) succeeded — proceed normally.
    Wrote,
    /// The write failed because the reader went away — not our failure;
    /// terminate quietly rather than unwind.
    PipeClosed,
    /// Any other write failure (disk full, invalid handle, ...) — a real
    /// problem, but still not something to panic over.
    Failed(String),
}

fn classify_write(result: io::Result<()>) -> WriteOutcome {
    match result {
        Ok(()) => WriteOutcome::Wrote,
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => WriteOutcome::PipeClosed,
        Err(e) => WriteOutcome::Failed(e.to_string()),
    }
}

/// Best-effort diagnostic write to stderr: swallow ANY failure, broken pipe
/// included, rather than panicking. Stderr is a side channel for a human,
/// never part of the machine envelope contract on stdout, so losing it isn't
/// fatal and must never change -- or block -- the process's own exit code.
/// `pub(crate)`, not `pub` -- an internal helper `main.rs`'s
/// `handle_parse_error` also needs for its own clap-error stderr write, not
/// part of the crate's Tasks-11-15-facing API.
pub(crate) fn eprint_best_effort(line: &str) {
    let mut err = io::stderr().lock();
    let _ = writeln!(err, "{line}").and_then(|_| err.flush());
}

/// Write `line` + a newline to stdout and flush, panicking-macro-free.
/// Returns normally on success (the caller decides its own exit code from
/// there). On failure it terminates the process HERE, since there is nothing
/// further "print the envelope"/"print the next stream event" can usefully
/// retry: quietly with `0` on a broken pipe (see the module doc comment —
/// this is a successful-enough outcome, not a crash), or with `1` (after one
/// best-effort diagnostic on stderr) for any other write failure. This is
/// also the ONLY way [`stream_sink`]'s closure (which returns `()`, so it has
/// no channel to report "stop calling me" back to whatever loop is driving
/// it) can actually stop a stream whose consumer has gone away. `pub(crate)`
/// for the same reason as [`eprint_best_effort`] -- `main.rs` reuses it for
/// its own `BAD_ARGS` envelope write rather than duplicating the logic.
pub(crate) fn print_stdout_line_or_exit(line: &str) {
    let mut out = io::stdout().lock();
    let result = writeln!(out, "{line}").and_then(|_| out.flush());
    match classify_write(result) {
        WriteOutcome::Wrote => {}
        WriteOutcome::PipeClosed => std::process::exit(0),
        WriteOutcome::Failed(msg) => {
            eprint_best_effort(&format!("dml-wow: stdout write failed: {msg}"));
            std::process::exit(1);
        }
    }
}

/// Print exactly one `{"ok":true,"data":...}` envelope on stdout. Always
/// returns `0` — the process exit code for a successful command. (A broken
/// pipe while writing it exits `0` directly from [`print_stdout_line_or_exit`]
/// instead of returning here — same observable result, since nothing runs
/// after this call in any dispatch arm today.)
pub fn emit_ok(data: Value) -> i32 {
    print_stdout_line_or_exit(&ok_line(data));
    0
}

/// Print exactly one `{"ok":false,"error":{...}}` envelope on stdout. Always
/// returns `1` — the process exit code for a failed command.
pub fn emit_err(code: &str, message: &str, hint: &str) -> i32 {
    print_stdout_line_or_exit(&err_line(code, message, hint));
    1
}

/// A sink for one subcommand's NDJSON event stream: prints `event` as a
/// single compact-JSON line on stdout and flushes immediately, so a consumer
/// reading the pipe sees progress live rather than buffered until exit.
/// Stateless by design (`impl Fn`, not `FnMut`) — it can be handed straight
/// to any `dml-wow` orchestration function that takes an `emit: impl
/// Fn(serde_json::Value)` callback (e.g. `dml_wow::native::ensure_engine_up_stream`).
/// Broken-pipe-safe like [`emit_ok`]/[`emit_err`] — see
/// [`print_stdout_line_or_exit`].
pub fn stream_sink() -> impl Fn(Value) {
    |event: Value| {
        print_stdout_line_or_exit(&event.to_string());
    }
}

/// Tracks which terminal event (if any) ended an NDJSON stream — the same
/// `event: "done"` / `event: "error"` vocabulary `dml_core::runner`'s
/// `run_stream` uses to decide `saw_terminal`. Not wired into [`stream_sink`]
/// itself (that stays a plain, stateless printer); a streaming dispatch arm
/// composes both: `let seen = TerminalSeen::new(); let sink = stream_sink();
/// some_stream_fn(|v| { seen.observe(&v); sink(v); });` then reads the exit
/// code back out with [`stream_exit`].
#[derive(Default)]
pub struct TerminalSeen(Cell<Option<bool>>);

impl TerminalSeen {
    pub fn new() -> Self {
        TerminalSeen(Cell::new(None))
    }

    /// Update the tracked state from one stream event: `event: "done"` marks
    /// success, `event: "error"` marks failure, anything else is untouched.
    ///
    /// STICKY ON FAILURE (Task 14 review, Fix 2): once a failure has been
    /// observed, a later `done` can NEVER take it back. A single stream only
    /// ever emits one terminal event, so within one stream this changes
    /// nothing — but `run.rs::stream_dispatch` deliberately allows ONE arm to
    /// drive SEVERAL streams onto ONE tracker (`start` = engine-ensure then
    /// compose; `stop` = compose then engine-stop). Under last-terminal-wins,
    /// a future arm whose first stream fails and whose second succeeds would
    /// exit **0** and silently report success for a failed operation — and the
    /// hazard would be invisible at the call site. Failure-sticky is also the
    /// same doctrine [`stream_exit`] already applies to a stream that ends with
    /// no terminal event at all: when in doubt, report the failure.
    ///
    /// (No arm can trigger this today: neither `native::ensure_engine_up_stream`
    /// nor `native::stop_engine_stream` ever emits `done`, so masking is
    /// currently impossible. This closes it before a future arm can.)
    pub fn observe(&self, event: &Value) {
        match event["event"].as_str() {
            Some("done") if self.0.get() != Some(false) => self.0.set(Some(true)),
            Some("error") => self.0.set(Some(false)),
            _ => {}
        }
    }
}

/// The process exit code for a finished stream: `0` iff the last terminal
/// event observed was `done`; `1` if it was `error`, or if the stream ended
/// with no terminal event at all (silently dying without `done`/`error` is
/// itself a failure to report).
pub fn stream_exit(saw: &TerminalSeen) -> i32 {
    match saw.0.get() {
        Some(true) => 0,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn emit_ok_returns_zero() {
        assert_eq!(emit_ok(json!({"x": 1})), 0);
    }

    #[test]
    fn emit_ok_line_matches_envelope_serialization() {
        let data = json!({"x": 1});
        assert_eq!(
            ok_line(data.clone()),
            dml_core::envelope::ok_envelope(data).to_string()
        );
    }

    #[test]
    fn emit_err_returns_one() {
        assert_eq!(emit_err("X", "y", ""), 1);
    }

    #[test]
    fn emit_err_line_matches_envelope_serialization() {
        assert_eq!(
            err_line("X", "y", ""),
            dml_core::envelope::error_envelope("X", "y", "").to_string()
        );
    }

    #[test]
    fn stream_sink_does_not_panic_on_call() {
        let sink = stream_sink();
        sink(json!({"event": "line", "level": "info", "text": "hi"}));
    }

    #[test]
    fn terminal_seen_starts_unset_and_reports_failure() {
        let seen = TerminalSeen::new();
        assert_eq!(stream_exit(&seen), 1);
    }

    #[test]
    fn terminal_seen_done_is_exit_zero() {
        let seen = TerminalSeen::new();
        seen.observe(&json!({"event": "section_start"}));
        seen.observe(&json!({"event": "done", "data": {}}));
        assert_eq!(stream_exit(&seen), 0);
    }

    #[test]
    fn terminal_seen_error_is_exit_one() {
        let seen = TerminalSeen::new();
        seen.observe(&json!({"event": "error", "error": {"code": "X", "message": "y", "hint": ""}}));
        assert_eq!(stream_exit(&seen), 1);
    }

    /// Fix 2, both orderings: a tracker that has seen a failure stays failed,
    /// whichever order the two terminal events arrive in. `error` then `done`
    /// is the one that matters (multi-stream masking); `done` then `error` was
    /// already correct under last-terminal-wins and must stay correct.
    #[test]
    fn terminal_seen_is_sticky_on_failure_in_both_orderings() {
        let error_then_done = TerminalSeen::new();
        error_then_done.observe(&json!({"event": "error", "error": {"code": "STOP_FAILED"}}));
        error_then_done.observe(&json!({"event": "line", "level": "info", "text": "second stream"}));
        error_then_done.observe(&json!({"event": "done", "data": {}}));
        assert_eq!(
            stream_exit(&error_then_done),
            1,
            "a later stream's `done` must not mask an earlier stream's failure"
        );

        let done_then_error = TerminalSeen::new();
        done_then_error.observe(&json!({"event": "done", "data": {}}));
        done_then_error.observe(&json!({"event": "error", "error": {"code": "X"}}));
        assert_eq!(stream_exit(&done_then_error), 1);

        // And the ordinary single-stream success is untouched by the change.
        let just_done = TerminalSeen::new();
        just_done.observe(&json!({"event": "done", "data": {}}));
        assert_eq!(stream_exit(&just_done), 0);
        // ...including a `done` after a NON-terminal event.
        let line_then_done = TerminalSeen::new();
        line_then_done.observe(&json!({"event": "line", "text": "hi"}));
        line_then_done.observe(&json!({"event": "done", "data": {}}));
        assert_eq!(stream_exit(&line_then_done), 0);
    }

    #[test]
    fn terminal_seen_ignores_non_terminal_events() {
        let seen = TerminalSeen::new();
        seen.observe(&json!({"event": "line", "text": "hi"}));
        assert_eq!(stream_exit(&seen), 1);
    }

    // -- classify_write (Task 10 review Finding 2: broken-pipe-safe writes) --
    //
    // `print_stdout_line_or_exit` itself calls `std::process::exit` on
    // anything but success, so it cannot be exercised in-process without
    // killing the test binary. `classify_write` is the pure decision pulled
    // out of it specifically so this behavior is still testable: feed it
    // synthetic `io::Result`s and check the outcome, exactly as suggested
    // when a live broken-pipe simulation isn't portable/deterministic here.

    #[test]
    fn classify_write_ok_is_wrote() {
        assert_eq!(classify_write(Ok(())), WriteOutcome::Wrote);
    }

    #[test]
    fn classify_write_broken_pipe_is_pipe_closed() {
        let err = io::Error::from(io::ErrorKind::BrokenPipe);
        assert_eq!(classify_write(Err(err)), WriteOutcome::PipeClosed);
    }

    #[test]
    fn classify_write_other_error_is_failed_with_message() {
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "disk gone");
        match classify_write(Err(err)) {
            WriteOutcome::Failed(msg) => assert!(msg.contains("disk gone")),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn eprint_best_effort_does_not_panic() {
        // Real stderr in a test process is never broken -- this just proves
        // the happy path doesn't blow up; the broken-pipe branch itself is
        // covered by classify_write's own tests above plus the binary-level
        // smoke test (piping a real `dml-wow` invocation into `head -1`).
        eprint_best_effort("dml-wow-cli test: ignore this line");
    }
}
