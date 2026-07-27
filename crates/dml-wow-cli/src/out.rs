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
//! Task 10's four subcommands (version/status/server-info/console-tail) are
//! all single-envelope reads, so nothing calls the streaming half
//! ([`stream_sink`]/[`TerminalSeen`]/[`stream_exit`]) yet — it is built now
//! for Tasks 11-15's write/long-running subcommands to wire into their
//! dispatch arms, same convention as `dml_core::backend`/`dml_wow::db`.

#![allow(dead_code)]

use std::cell::Cell;
use std::io::Write;

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

/// Print exactly one `{"ok":true,"data":...}` envelope on stdout. Always
/// returns `0` — the process exit code for a successful command.
pub fn emit_ok(data: Value) -> i32 {
    println!("{}", ok_line(data));
    0
}

/// Print exactly one `{"ok":false,"error":{...}}` envelope on stdout. Always
/// returns `1` — the process exit code for a failed command.
pub fn emit_err(code: &str, message: &str, hint: &str) -> i32 {
    println!("{}", err_line(code, message, hint));
    1
}

/// A sink for one subcommand's NDJSON event stream: prints `event` as a
/// single compact-JSON line on stdout and flushes immediately, so a consumer
/// reading the pipe sees progress live rather than buffered until exit.
/// Stateless by design (`impl Fn`, not `FnMut`) — it can be handed straight
/// to any `dml-wow` orchestration function that takes an `emit: impl
/// Fn(serde_json::Value)` callback (e.g. `dml_wow::native::ensure_engine_up_stream`).
pub fn stream_sink() -> impl Fn(Value) {
    |event: Value| {
        println!("{event}");
        // A caller may be reading this over a pipe expecting live progress;
        // stdout is line-buffered only when connected to a terminal, so
        // flush explicitly for the piped case too.
        let _ = std::io::stdout().flush();
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
    /// A later terminal event overwrites an earlier one (a stream is only
    /// ever expected to emit one, but this is not the place to assert that).
    pub fn observe(&self, event: &Value) {
        match event["event"].as_str() {
            Some("done") => self.0.set(Some(true)),
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

    #[test]
    fn terminal_seen_ignores_non_terminal_events() {
        let seen = TerminalSeen::new();
        seen.observe(&json!({"event": "line", "text": "hi"}));
        assert_eq!(stream_exit(&seen), 1);
    }
}
