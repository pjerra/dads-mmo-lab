//! Generic, docker/domain-agnostic subprocess helpers — moved out of the
//! launcher's `dml::destructive` (cargo-workspace refactor, Task 4). This
//! module knows nothing about docker, compose, or titles: it is just "run a
//! program, get its output back" in two shapes —
//!
//!   - [`run_captured`]: bounded, captured-then-split (the `docker builder
//!     prune -af` / `docker image prune -af` shape: the bash captures the
//!     WHOLE combined output into a variable first, THEN loops it
//!     line-by-line).
//!   - [`run_streamed_unbounded`]: unbounded, drained AS IT ARRIVES (the
//!     `module rebuild` shape: a first-time AzerothCore build can run
//!     30-90 minutes, so a wall-clock kill would abort real progress, and
//!     the UI wants to see build output LIVE, not just at the end).
//!
//! `launcher::dml::destructive` keeps every DOMAIN-specific primitive
//! (titles, volumes, compose, removal targets) and re-exports these names so
//! every existing caller keeps compiling unchanged.
//!
//! This module is also now the CANONICAL home of the launcher's two
//! lower-level subprocess primitives, [`windows_no_window`] and
//! [`output_bounded_draining`] — moved here (not just `run_captured`/
//! `run_streamed_unbounded`, which are built on top of them) because Rust
//! re-exports are transparent: `launcher/src-tauri/src/dml/status.rs` (their
//! original home) now does `pub(crate) use dml_core::proc::{output_bounded_draining,
//! windows_no_window};` and every one of its many OTHER call sites (`maint`,
//! `modmgr`, `backup`, `moduletail`, `restore`, and `status.rs` itself) kept
//! compiling with zero changes, since they all reached these two exclusively
//! through `super::status::`/`status::`-qualified paths.

use std::ffi::OsStr;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

/// `CREATE_NO_WINDOW` on Windows so a spawned child never flashes a console —
/// same flag every other native docker-shelling call in this codebase sets
/// (see `dml::native`/`lib.rs::run_bounded`; `launcher::dml::status`
/// re-exports this under the same name for its own call sites).
pub fn windows_no_window(cmd: &mut Command) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(windows))]
    {
        let _ = cmd;
    }
}

/// Bounded `Command` runner, deliberately NOT a non-draining `output()`
/// call: output must stay small with that shape — a modestly-chatty
/// long-running child can emit output larger than the OS pipe buffer (64KiB
/// on Windows), which blocks the child writing to a full pipe nobody is
/// reading while a bare `try_wait()`-poller never observes it exit, silently
/// timing out every call. This variant drains both pipes on background
/// threads WHILE polling for exit, so the child can never block on a full
/// buffer, no matter the output size. `launcher::dml::status` re-exports
/// this under the same name for its own (much more numerous) call sites —
/// `docker inspect`/`ps`/`port`/`logs`, `git fetch`, and more.
pub fn output_bounded_draining(mut cmd: Command, timeout: Duration) -> Option<std::process::Output> {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stdout, &mut buf);
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = std::io::Read::read_to_end(&mut stderr, &mut buf);
        buf
    });

    let deadline = std::time::Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
        }
    };
    // Killing/waiting the child closes its end of both pipes, so the reader
    // threads see EOF and finish promptly either way — this join is not
    // itself an unbounded wait.
    let stdout_buf = stdout_handle.join().unwrap_or_default();
    let stderr_buf = stderr_handle.join().unwrap_or_default();
    if timed_out {
        return None;
    }
    status.map(|status| std::process::Output { status, stdout: stdout_buf, stderr: stderr_buf })
}

// ---------------------------------------------------------------------------
// Bounded "run, capture, split into non-empty lines" — `docker builder prune
// -af` / `docker image prune -af`'s streaming-after-the-fact shape
// (`90-main.sh:1554-1559,1579-1585`: the bash captures the WHOLE combined
// output into a variable first, THEN loops it line-by-line into
// `ndjson_line info` — this is NOT the live-as-it-happens shape
// [`run_streamed_unbounded`] provides; that one is reserved for the actual
// 30-90-minute build).
// ---------------------------------------------------------------------------

/// Combine stdout+stderr and split into non-empty lines — the pure core of
/// the read-loop both prune arms share, split out so it's tested without a
/// real spawn.
pub fn combined_nonempty_lines(stdout: &[u8], stderr: &[u8]) -> Vec<String> {
    let mut combined = String::from_utf8_lossy(stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(stderr));
    combined.lines().filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// One bounded, captured-then-split subprocess run.
pub struct CapturedRun {
    pub lines: Vec<String>,
    pub success: bool,
    /// The process exit code, or `None` on a spawn failure/timeout (treated
    /// the same as "some failure code" by callers — there is no real exit
    /// code to report either way).
    pub code: Option<i32>,
}

/// Run `program args...` (no `cwd` — `docker builder prune`/`docker image
/// prune` are host-level, not compose-scoped), bounded+drained via
/// [`output_bounded_draining`], and split its combined output into
/// non-empty lines.
pub fn run_captured(program: &OsStr, args: &[&str], timeout: Duration) -> CapturedRun {
    let mut cmd = Command::new(program);
    cmd.args(args);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, timeout) {
        Some(out) => CapturedRun {
            lines: combined_nonempty_lines(&out.stdout, &out.stderr),
            success: out.status.success(),
            code: out.status.code(),
        },
        None => CapturedRun { lines: Vec::new(), success: false, code: None },
    }
}

// ---------------------------------------------------------------------------
// `run_streamed_unbounded` — the module rebuild build stream. See the module
// doc comment for the full rationale.
// ---------------------------------------------------------------------------

/// Split newly-arrived `chunk` bytes (appended to the carry-over `buf`) into
/// complete `\n`-terminated lines, UTF-8-lossy decoded per line (never a hard
/// error on a stray non-UTF8 byte — a multi-hour build log must not go
/// silent over one bad byte). Any trailing partial line stays in `buf` for
/// the next call. Pure, so the incremental-decode logic is unit-tested
/// without spawning anything.
pub fn drain_lines(buf: &mut Vec<u8>, chunk: &[u8]) -> Vec<String> {
    buf.extend_from_slice(chunk);
    let mut out = Vec::new();
    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
        out.push(String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1]).into_owned());
    }
    out
}

/// Drain `reader` to EOF, sending each complete line (via [`drain_lines`])
/// down `tx` as it arrives, plus a final trailing partial line (if any) once
/// EOF is reached — this is what lets a docker-build line with no trailing
/// newline still surface. Returns early if the receiver has gone away
/// (`tx.send` failing means main thread already exited its loop, e.g. the
/// child died and BOTH threads are racing to notice — not an error).
fn stream_reader<R: std::io::Read>(mut reader: R, tx: std::sync::mpsc::Sender<String>) {
    let mut pending = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                for line in drain_lines(&mut pending, &chunk[..n]) {
                    if tx.send(line).is_err() {
                        return;
                    }
                }
            }
            Err(_) => break,
        }
    }
    if !pending.is_empty() {
        let _ = tx.send(String::from_utf8_lossy(&pending).into_owned());
    }
}

/// Run `program args...` in `cwd`, streaming every combined stdout+stderr
/// line to `on_line` AS IT ARRIVES and appending each (with a trailing
/// `\n`) to `log_path` (created/truncated up front) — a port of the bash's
/// `docker compose up -d --build 2>&1 | tee rebuild.log | while read...`
/// pipeline (`90-main.sh:5000`).
///
/// DELIBERATELY UNBOUNDED: no wall-clock timeout, no `kill()` anywhere in
/// this function — a first-time AzerothCore rebuild can legitimately run
/// 30-90 minutes, and killing it on an arbitrary clock would abort a build
/// that was still making real progress. "Supervised" instead means: two
/// reader threads continuously drain stdout/stderr concurrently (so the
/// child can never block writing to a full OS pipe — the same hazard
/// [`output_bounded_draining`]'s doc comment above describes for `docker
/// logs`), and this thread only ever blocks on the mpsc channel those two
/// threads feed, not on the child directly. If the caller's `on_line`
/// callback is a no-op (e.g. the Tauri `Channel` consumer went away — every
/// `_native` command's convention is `let _ = ch.send(v)`, a silently
/// swallowed failure), lines keep landing in `log_path` and the child is
/// left completely alone to finish; it is NEVER killed just because nobody
/// is listening. Returns the real exit status, or `None` only on a spawn/
/// log-file-create failure (never on a timeout — there isn't one).
pub fn run_streamed_unbounded(
    program: &OsStr,
    args: &[&str],
    cwd: &Path,
    log_path: &Path,
    mut on_line: impl FnMut(&str),
) -> Option<std::process::ExitStatus> {
    let mut log_file = std::fs::File::create(log_path).ok()?;
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd);
    windows_no_window(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_err = tx.clone();
    let out_handle = std::thread::spawn(move || stream_reader(stdout, tx));
    let err_handle = std::thread::spawn(move || stream_reader(stderr, tx_err));

    use std::io::Write;
    // Ends when BOTH reader threads have dropped their `Sender` (EOF on both
    // pipes) — not on any timer.
    while let Ok(line) = rx.recv() {
        let _ = writeln!(log_file, "{line}");
        let _ = log_file.flush();
        on_line(&line);
    }

    let _ = out_handle.join();
    let _ = err_handle.join();
    // By now the child's own stdout/stderr have both hit EOF, which in
    // practice means it has already exited -- `wait()` reaps it and hands
    // back the real status. No `kill()` anywhere on this path, ever.
    child.wait().ok()
}

/// Spawn `cmd`, wait up to `timeout` wall-clock, and — crucially — **kill and
/// reap** the child if it overruns instead of abandoning it. The previous
/// idiom ran a blocking `output()` on a detached helper thread and left it
/// (and its un-reaped child holding open stdio pipes) alive forever on a hung
/// `docker`/`tailscale` subprocess; repeated calls against a wedged engine
/// slowly leaked threads + process handles. Here the child is owned, polled,
/// and terminated on the deadline. Output is small for every caller (a docker
/// env list, a tailscale status) so a full drain via `wait_with_output()`
/// after exit cannot deadlock on the pipe buffer.
pub fn output_bounded(mut cmd: std::process::Command, timeout: std::time::Duration) -> Option<std::process::Output> {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait(); // reap so it never zombies
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    child.wait_with_output().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dml-core-proc-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // -- combined_nonempty_lines -----------------------------------------------

    #[test]
    fn combined_nonempty_lines_merges_and_drops_blanks() {
        let got = combined_nonempty_lines(b"a\n\nb\n", b"c\n");
        assert_eq!(got, vec!["a".to_string(), "b".to_string(), "c".to_string()]);
    }

    #[test]
    fn combined_nonempty_lines_empty_both_is_empty() {
        assert!(combined_nonempty_lines(b"", b"").is_empty());
    }

    // -- drain_lines (incremental streaming decode) ----------------------------

    #[test]
    fn drain_lines_splits_complete_lines_and_buffers_partial() {
        let mut buf = Vec::new();
        let got1 = drain_lines(&mut buf, b"hello\nwor");
        assert_eq!(got1, vec!["hello".to_string()]);
        assert_eq!(buf, b"wor".to_vec());

        let got2 = drain_lines(&mut buf, b"ld\nfinal-partial");
        assert_eq!(got2, vec!["world".to_string()]);
        assert_eq!(buf, b"final-partial".to_vec());
    }

    #[test]
    fn drain_lines_lossy_decodes_invalid_utf8_instead_of_erroring() {
        let mut buf = Vec::new();
        let chunk = b"good \xFF\xFE bytes\n".to_vec();
        let got = drain_lines(&mut buf, &chunk);
        assert_eq!(got.len(), 1);
        assert!(got[0].starts_with("good "));
        assert!(got[0].contains("bytes"));
    }

    #[test]
    fn drain_lines_empty_chunk_yields_nothing() {
        let mut buf = Vec::new();
        assert!(drain_lines(&mut buf, b"").is_empty());
    }

    // -- real-subprocess test plumbing: same fixture-file convention as
    // `runner.rs`'s `fixture_runner`/`fixture(name)` (Task 3) -- a `.cmd`
    // fixture on Windows, an `.sh` sibling everywhere else, so these tests
    // never hardcode `cmd.exe` and can run on `ubuntu-latest` CI (Task 16).

    #[cfg(windows)]
    const FIXTURE_EXT: &str = "cmd";
    #[cfg(not(windows))]
    const FIXTURE_EXT: &str = "sh";

    fn fixture(name: &str) -> String {
        format!("{}/tests/fixtures/{}.{}", env!("CARGO_MANIFEST_DIR"), name, FIXTURE_EXT)
    }

    #[cfg(windows)]
    fn shell_program() -> &'static str {
        "cmd"
    }
    #[cfg(not(windows))]
    fn shell_program() -> &'static str {
        "sh"
    }

    /// Windows runs the fixture via `cmd /C <path>`; everywhere else `sh
    /// <path>` interprets the script directly (no `+x` needed — the path is
    /// an argument to `sh`, never exec'd on its own).
    #[cfg(windows)]
    fn shell_args(fixture_path: &str) -> Vec<&str> {
        vec!["/C", fixture_path]
    }
    #[cfg(not(windows))]
    fn shell_args(fixture_path: &str) -> Vec<&str> {
        vec![fixture_path]
    }

    // -- run_streamed_unbounded (real spawn, real log file, no docker needed) --

    #[test]
    fn run_streamed_unbounded_captures_lines_tees_log_and_returns_exit_status() {
        let dir = tmp_dir("streamed-cmd");
        let log_path = dir.join("out.log");
        let mut lines: Vec<String> = Vec::new();
        // `streamed_two_lines` prints "one" then "two" then exits 3 -- a
        // tiny multi-line, nonzero-exit program with no docker dependency.
        let path = fixture("streamed_two_lines");
        let status = run_streamed_unbounded(
            OsStr::new(shell_program()),
            &shell_args(&path),
            &dir,
            &log_path,
            |line| lines.push(line.to_string()),
        );
        let status = status.expect("the fixture shell must spawn");
        assert_eq!(status.code(), Some(3));
        assert!(lines.iter().any(|l| l.trim() == "one"));
        assert!(lines.iter().any(|l| l.trim() == "two"));
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert!(logged.contains("one"));
        assert!(logged.contains("two"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn run_streamed_unbounded_truncates_a_pre_existing_log_file() {
        let dir = tmp_dir("streamed-truncate");
        let log_path = dir.join("out.log");
        std::fs::write(&log_path, "OLD CONTENT THAT MUST NOT SURVIVE\n").unwrap();
        let path = fixture("streamed_fresh");
        let _ = run_streamed_unbounded(OsStr::new(shell_program()), &shell_args(&path), &dir, &log_path, |_| {});
        let logged = std::fs::read_to_string(&log_path).unwrap();
        assert!(!logged.contains("OLD CONTENT"));
        assert!(logged.contains("fresh"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- run_captured -----------------------------------------------------------

    #[test]
    fn run_captured_real_command_success_and_failure() {
        let hi_path = fixture("captured_hi");
        let ok = run_captured(OsStr::new(shell_program()), &shell_args(&hi_path), Duration::from_secs(10));
        assert!(ok.success);
        assert_eq!(ok.code, Some(0));
        assert_eq!(ok.lines, vec!["hi".to_string()]);

        let exit7_path = fixture("captured_exit7");
        let bad = run_captured(OsStr::new(shell_program()), &shell_args(&exit7_path), Duration::from_secs(10));
        assert!(!bad.success);
        assert_eq!(bad.code, Some(7));
    }

    #[cfg(windows)]
    #[test]
    fn output_bounded_returns_output_for_a_fast_command() {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/c", "echo", "bounded_ok"]);
        let out = super::output_bounded(cmd, std::time::Duration::from_secs(5))
            .expect("fast command should return Some");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("bounded_ok"));
    }

    #[cfg(windows)]
    #[test]
    fn output_bounded_kills_and_returns_none_on_timeout() {
        // `ping -n 20` runs ~19s; a 300ms bound must return None well before
        // that, proving the child was killed rather than waited out.
        let start = std::time::Instant::now();
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/c", "ping", "-n", "20", "127.0.0.1"]);
        let out = super::output_bounded(cmd, std::time::Duration::from_millis(300));
        assert!(out.is_none(), "an overrunning command must time out to None");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "must return promptly after the deadline, not wait for the child"
        );
    }
}
