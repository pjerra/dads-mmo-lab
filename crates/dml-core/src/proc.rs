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

/// Why a bounded run did not produce output — the distinction
/// [`output_bounded_draining`] throws away by collapsing everything into
/// `None`.
///
/// This exists for the setup probe chain (`crate::setup`), where "the program
/// is not on this machine" is a DEFINITIVE no and "it ran past its deadline"
/// is evidence of NOTHING. Collapsing those two is the tri-state mistake this
/// codebase keeps re-learning, so the probe layer gets a runner that keeps
/// them apart at the source instead of guessing afterwards.
#[derive(Debug)]
pub enum BoundedOutcome {
    /// The spawn itself failed. `kind() == NotFound` means the program does
    /// not exist; any other kind is an unknown.
    SpawnFailed(std::io::Error),
    /// The child was still alive at the deadline and was killed + reaped.
    TimedOut,
    /// It ran to completion. The exit status may still be non-zero.
    Ran(std::process::Output),
}

/// The two operations the timeout path needs from a child, behind a trait so
/// the "never block the caller on wait" invariant can be asserted against a
/// fake. A wall-clock test cannot assert it: the production hang requires
/// `kill()` to FAIL, and a test cannot force that.
pub trait Abandonable: Send + 'static {
    fn kill(&mut self) -> std::io::Result<()>;
    fn wait(&mut self) -> std::io::Result<()>;
}

impl Abandonable for std::process::Child {
    fn kill(&mut self) -> std::io::Result<()> {
        std::process::Child::kill(self)
    }
    fn wait(&mut self) -> std::io::Result<()> {
        std::process::Child::wait(self).map(|_| ())
    }
}

/// The one thing the poll loop needs on top of [`Abandonable`]: "has it exited
/// yet?". Same reason as that trait — it is a test seam, not an abstraction for
/// its own sake. See `bounded_outcome_after_spawn`.
pub trait Pollable: Abandonable {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl Pollable for std::process::Child {
    fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
        std::process::Child::try_wait(self)
    }
}

/// Abandon a child we have stopped waiting for: ask it to stop, then reap it on
/// a thread nobody joins, and RETURN IMMEDIATELY.
///
/// The reap must not happen on the caller's thread. `kill()` can fail, and the
/// `wait()` that followed it was then INFINITE against a process whose whole
/// problem is that it outlives the deadline. Measured 2026-08-03: a
/// 600ms-bounded call returned after 605 SECONDS.
pub fn abandon<C: Abandonable>(mut child: C) {
    let _ = child.kill();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// One pipe-draining thread, whose result can be collected WITH A DEADLINE.
///
/// The point is [`BoundedReader::collect_by`]: a plain `JoinHandle::join()` is
/// unbounded, and on this exact pipe it can be unbounded for a reason that has
/// nothing to do with the child. See that method for the failure it exists to
/// stop, and for why the thread is never joined afterwards.
pub struct BoundedReader {
    rx: std::sync::mpsc::Receiver<Vec<u8>>,
}

impl BoundedReader {
    /// Start draining `r` to EOF on its own thread.
    ///
    /// Generic over [`std::io::Read`] rather than taking a `ChildStdout`
    /// specifically, because that genericity is the test seam: the hang this
    /// type defends against needs a reader that never returns, and a fake
    /// whose `read` blocks produces that on demand, where a wall-clock race
    /// against a real grandchild does not (Task 1 proved that shape passes
    /// with the bug deliberately reinstated).
    pub fn spawn<R: std::io::Read + Send + 'static>(mut r: R) -> Self {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let _ = std::io::Read::read_to_end(&mut r, &mut buf);
            // The receiver may be long gone (the deadline fired); that is not
            // an error, it is the whole design.
            let _ = tx.send(buf);
        });
        BoundedReader { rx }
    }

    /// The drained bytes, or `None` if the reader had not finished by
    /// `deadline`.
    ///
    /// THE BUG THIS EXISTS FOR. `run_bounded_outcome`'s success path used to
    /// `join()` these threads outright, on the stated grounds that "the child
    /// exited by itself, so its handles are closed". That reasoning does not
    /// hold, and the timeout path fifteen lines below already says so: a
    /// GRANDCHILD that inherited the pipe handles keeps them open regardless
    /// of how the child ended, so the reader never sees EOF. `docker`,
    /// `wsl.exe` and `git` all spawn helpers, and the Arch backend puts a
    /// `wsl.exe` spawn on every single command — a child that exits at 0.1s
    /// while a helper holds the pipe blocked that join FOREVER, past the
    /// deadline, and the function still returned `Ran`, so the caller could
    /// not even tell the bound had been violated.
    ///
    /// The thread is deliberately NOT joined afterwards. It is blocked on a
    /// handle we do not control; abandoning it costs one stack until the
    /// writer finally closes it, and waiting for it costs the caller the
    /// entire bound it asked for. Same trade as [`abandon`], one layer up.
    pub fn collect_by(&self, deadline: std::time::Instant) -> Option<Vec<u8>> {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        self.rx.recv_timeout(remaining).ok()
    }
}

/// [`output_bounded_draining`] with the failure reason preserved. Same
/// draining semantics (both pipes drained on background threads while the
/// main thread polls for exit), same kill-and-reap on the deadline — the only
/// difference is that the caller learns WHY there is no output.
///
/// THE BOUND IS A REAL WALL CLOCK ON BOTH PATHS. The child exiting is not the
/// end of the story: see [`BoundedReader::collect_by`] for why the drain that
/// follows can outlast the deadline on its own.
pub fn run_bounded_outcome(mut cmd: Command, timeout: Duration) -> BoundedOutcome {
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        // The ONE definitive negative the probe layer can act on: keep the
        // io::Error so the caller can read its `kind()`.
        Err(e) => return BoundedOutcome::SpawnFailed(e),
    };
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    bounded_outcome_after_spawn(
        child,
        BoundedReader::spawn(stdout),
        BoundedReader::spawn(stderr),
        timeout,
    )
}

/// Everything [`run_bounded_outcome`] does after the spawn: poll for exit
/// against ONE wall clock, then drain both pipes against THAT SAME clock.
///
/// WHY THIS IS A SEPARATE FUNCTION. The bound is not a property of the poll
/// loop or of [`BoundedReader::collect_by`] — each of those is correct in
/// isolation and each already has tests. The bound is a property of the WIRING
/// between them, and the final gate proved that wiring unpinned: replacing both
/// `collect_by(deadline)` arguments with a fabricated `Instant::now() + 3600s`
/// left all 154 `proc::`/`setup::`/`engine::` tests green, and so would a full
/// revert to an unbounded drain. The natural typo is worse than either, because
/// it looks right: `collect_by(Instant::now() + timeout)` re-derives the clock
/// AFTER the poll loop has already spent part of it, silently doubling the
/// bound.
///
/// Generic over the child for the same reason [`Abandonable`] is: a fake that
/// exits at once, paired with two readers that never reach EOF, reproduces
/// "child gone, grandchild still holding the pipes" deterministically — where a
/// real grandchild is a wall-clock race that passes with the bug reinstated.
/// Pinned by `the_drain_is_bounded_by_the_same_clock_the_poll_loop_used`, which
/// brackets the elapsed time from BOTH sides: shorter than the bound means the
/// drain got an already-expired deadline, longer means it got a fabricated one.
fn bounded_outcome_after_spawn<C: Pollable>(
    mut child: C,
    stdout_reader: BoundedReader,
    stderr_reader: BoundedReader,
    timeout: Duration,
) -> BoundedOutcome {
    // THE one clock. Both the loop below and the drain at the bottom read this
    // binding; nothing between them may re-derive it from `timeout`.
    let deadline = std::time::Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                timed_out = true;
                break None;
            }
        }
    };
    // THE TIMEOUT PATH TOUCHES NOTHING BLOCKING. Two separate traps live here,
    // and each one on its own is enough to make a bound fictional.
    //
    // 1. Joining the reader threads. `child.kill()` kills the CHILD; a
    //    GRANDCHILD that inherited the pipe handles keeps them open, so the
    //    readers never see EOF. Nothing is lost by skipping the join —
    //    `TimedOut` carries no output and never did.
    //
    // 2. `child.wait()` after the kill. `kill()` can fail, and its result was
    //    discarded; `wait()` then blocks INFINITE for a process we have
    //    already decided to stop waiting for. Measured 2026-08-03: a
    //    600ms-bounded call against `cmd /C ping -n 600` returned after 605
    //    SECONDS, and the deadline had fired correctly — the time was spent
    //    here. The reap still happens, on a thread nobody waits for — see
    //    `abandon`, which owns this step and is what `abandon_never_blocks_
    //    the_caller_on_the_reap` actually pins.
    if timed_out {
        abandon(child);
        return BoundedOutcome::TimedOut;
    }
    // THE SUCCESS PATH IS BOUNDED TOO, and the original reasoning for why it
    // did not need to be ("the child exited by itself, so its handles are
    // closed") is the same reasoning the timeout path above rejects. A
    // grandchild holding the pipes keeps them open however the child ended.
    //
    // WHAT WE RETURN when the child exited cleanly but the drain overran, and
    // why it is `TimedOut` rather than a `Ran` carrying whatever arrived:
    // `Ran` is the shape every caller reads as "here is the answer". A
    // truncated stdout under `Ran { code: Some(0) }` is not a smaller answer,
    // it is a WRONG one — `classify_wsl_list` would read a half-read distro
    // list as a complete list, and settle. `TimedOut` reaches the probe layer
    // as `ProbeOutcome::CouldNotTell`, which is exactly what happened: we ran
    // out of the time we were given before we had the whole answer. The one
    // thing that must never happen is telling the caller the bound held when
    // it did not, and only one of these two options can do that.
    let (Some(stdout_buf), Some(stderr_buf)) =
        (stdout_reader.collect_by(deadline), stderr_reader.collect_by(deadline))
    else {
        return BoundedOutcome::TimedOut;
    };
    match status {
        Some(status) => BoundedOutcome::Ran(std::process::Output {
            status,
            stdout: stdout_buf,
            stderr: stderr_buf,
        }),
        // Unreachable: the `None` break above always sets `timed_out`. A
        // panic in a diagnostic path would be worse than a conservative shrug.
        None => BoundedOutcome::TimedOut,
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
///
/// A thin wrapper over [`run_bounded_outcome`] since the setup probe chain
/// needed the failure reason: every existing caller keeps the identical
/// `Option` contract (spawn failure, timeout and a try_wait error all read as
/// `None`).
///
/// A thin wrapper over [`run_bounded_outcome`] since the setup probe chain
/// needed the failure reason. Every existing caller keeps the identical
/// `Option` contract: a spawn failure, a timeout and a `try_wait` error all
/// still read as `None`.
pub fn output_bounded_draining(cmd: Command, timeout: Duration) -> Option<std::process::Output> {
    match run_bounded_outcome(cmd, timeout) {
        BoundedOutcome::Ran(out) => Some(out),
        BoundedOutcome::SpawnFailed(_) | BoundedOutcome::TimedOut => None,
    }
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
    drain_lines_split(buf, chunk, LineSplit::Newline)
}

/// What counts as the end of a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineSplit {
    /// `\n` only. The default, and right for every command whose output is
    /// ordinary log lines.
    #[default]
    Newline,
    /// `\n` **or** `\r`.
    ///
    /// For tools that report progress by REDRAWING one line. `git clone
    /// --progress` writes `Receiving objects:  1% …\rReceiving objects:  2% …\r`
    /// and emits a newline only when the phase ENDS — so under [`Self::Newline`]
    /// a whole download arrives as a single line, once, after the download it
    /// was describing is already over. That is not a cosmetic difference: it is
    /// the difference between live progress and none.
    NewlineOrReturn,
}

/// [`drain_lines`] with the line terminator chosen by the caller.
///
/// Under [`LineSplit::NewlineOrReturn`], EMPTY segments are dropped. A `\r\n`
/// pair would otherwise yield the real line followed by a phantom empty one,
/// and a blank progress redraw carries nothing worth forwarding. Under
/// [`LineSplit::Newline`] blank lines are preserved exactly as before — the
/// docker build wall uses them to separate vertices.
pub fn drain_lines_split(buf: &mut Vec<u8>, chunk: &[u8], split: LineSplit) -> Vec<String> {
    buf.extend_from_slice(chunk);
    let mut out = Vec::new();
    let is_end = |b: u8| match split {
        LineSplit::Newline => b == b'\n',
        LineSplit::NewlineOrReturn => b == b'\n' || b == b'\r',
    };
    while let Some(pos) = buf.iter().position(|&b| is_end(b)) {
        let line_bytes: Vec<u8> = buf.drain(..=pos).collect();
        let line = String::from_utf8_lossy(&line_bytes[..line_bytes.len() - 1]).into_owned();
        if split == LineSplit::NewlineOrReturn && line.is_empty() {
            continue;
        }
        out.push(line);
    }
    out
}

/// Drain `reader` to EOF, sending each complete line (via [`drain_lines`])
/// down `tx` as it arrives, plus a final trailing partial line (if any) once
/// EOF is reached — this is what lets a docker-build line with no trailing
/// newline still surface. Returns early if the receiver has gone away
/// (`tx.send` failing means main thread already exited its loop, e.g. the
/// child died and BOTH threads are racing to notice — not an error).
fn stream_reader<R: std::io::Read>(
    mut reader: R,
    tx: std::sync::mpsc::Sender<String>,
    split: LineSplit,
) {
    let mut pending = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                for line in drain_lines_split(&mut pending, &chunk[..n], split) {
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
    use std::io::Write;
    let mut log_file = std::fs::File::create(log_path).ok()?;
    // The tee is the ONLY thing this wrapper adds; the supervision itself lives
    // in [`run_streamed_lines`] so a caller that wants the same unbounded,
    // both-pipes-drained streaming WITHOUT a log file (the native install
    // engine's git/docker probes) does not have to reimplement it — or invent a
    // scratch file per probe just to satisfy this signature.
    run_streamed_lines(program, args, Some(cwd), LineSplit::Newline, |line| {
        let _ = writeln!(log_file, "{line}");
        let _ = log_file.flush();
        on_line(line);
    })
}

/// [`run_streamed_unbounded`] without the tee, and with an OPTIONAL working
/// directory (`None` inherits this process's — which is what a call whose
/// arguments are all absolute wants, and what keeps a probe from failing merely
/// because some directory does not exist yet).
///
/// Same contract as its tee-ing wrapper, and for the same reasons: deliberately
/// unbounded (no timeout, no `kill()` — a first-time AzerothCore build runs for
/// hours), both pipes drained concurrently by their own threads so the child can
/// never block on a full OS pipe, and `None` returned ONLY on a spawn failure —
/// which is a could-not-tell, never an exit code.
pub fn run_streamed_lines(
    program: &OsStr,
    args: &[&str],
    cwd: Option<&Path>,
    split: LineSplit,
    mut on_line: impl FnMut(&str),
) -> Option<std::process::ExitStatus> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    windows_no_window(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx_err = tx.clone();
    let out_handle = std::thread::spawn(move || stream_reader(stdout, tx, split));
    let err_handle = std::thread::spawn(move || stream_reader(stderr, tx_err, split));

    // Ends when BOTH reader threads have dropped their `Sender` (EOF on both
    // pipes) — not on any timer.
    while let Ok(line) = rx.recv() {
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
                    abandon(child);
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => {
                abandon(child);
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

    // -- abandon -----------------------------------------------------------

    /// THE deterministic pin. A wall-clock grandchild test cannot do this job:
    /// the production hang needs `kill()` to fail, which a test cannot force,
    /// so that test passes whether the bug is present or not (verified
    /// 2026-08-04 — it reported ok on ten consecutive runs, including with the
    /// blocking wait deliberately reinstated). This one fails deterministically
    /// the moment the reap moves back onto the caller's thread.
    #[test]
    fn abandon_never_blocks_the_caller_on_the_reap() {
        struct SlowToReap;
        impl Abandonable for SlowToReap {
            fn kill(&mut self) -> std::io::Result<()> {
                Ok(())
            }
            fn wait(&mut self) -> std::io::Result<()> {
                std::thread::sleep(Duration::from_secs(2));
                Ok(())
            }
        }
        let began = std::time::Instant::now();
        abandon(SlowToReap);
        let elapsed = began.elapsed();
        assert!(
            elapsed < Duration::from_millis(250),
            "abandon blocked the caller for {elapsed:?}; the reap belongs on a thread nobody joins"
        );
    }

    /// …and the reap still HAPPENS. A fix that simply dropped the wait would
    /// satisfy the test above while leaking a zombie on every timeout.
    #[test]
    fn abandon_still_kills_and_still_reaps() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        struct Recording {
            killed: Arc<AtomicBool>,
            reaped: Arc<AtomicBool>,
        }
        impl Abandonable for Recording {
            fn kill(&mut self) -> std::io::Result<()> {
                self.killed.store(true, Ordering::SeqCst);
                Ok(())
            }
            fn wait(&mut self) -> std::io::Result<()> {
                self.reaped.store(true, Ordering::SeqCst);
                Ok(())
            }
        }
        let killed = Arc::new(AtomicBool::new(false));
        let reaped = Arc::new(AtomicBool::new(false));
        abandon(Recording { killed: killed.clone(), reaped: reaped.clone() });
        assert!(killed.load(Ordering::SeqCst), "the child must still be killed");
        // The reap is on another thread, so poll rather than assuming ordering.
        let began = std::time::Instant::now();
        while !reaped.load(Ordering::SeqCst) && began.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(reaped.load(Ordering::SeqCst), "the child must still be reaped, just not inline");
    }

    // -- BoundedReader ---------------------------------------------------------
    // The success path's drain is bounded too. A wall-clock grandchild test
    // cannot pin this (Task 1 proved that shape passes with the bug
    // reinstated), so the reader itself is the seam and the fake blocks on
    // demand.

    /// A pipe nobody will ever close: `read` blocks until the process ends.
    /// This is a real grandchild holding an inherited handle, made
    /// deterministic.
    struct NeverEnds;
    impl std::io::Read for NeverEnds {
        fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
            loop {
                std::thread::sleep(Duration::from_secs(3600));
            }
        }
    }

    /// THE deterministic pin for the success path. `join()` here was unbounded:
    /// a child that exits at 0.1s while a helper still holds the pipe blocked
    /// it forever, past the deadline, and `run_bounded_outcome` then returned
    /// `Ran` — telling the caller the bound had held when it had not.
    #[test]
    fn a_reader_that_never_reaches_eof_does_not_outlast_the_deadline() {
        let reader = BoundedReader::spawn(NeverEnds);
        let began = std::time::Instant::now();
        let got = reader.collect_by(began + Duration::from_millis(200));
        let elapsed = began.elapsed();
        assert!(got.is_none(), "a reader that never finished must not report bytes");
        assert!(
            elapsed >= Duration::from_millis(200),
            "must actually wait out the deadline, not give up early: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "collect_by blocked for {elapsed:?}; the drain must not outlive the bound"
        );
    }

    /// ...and the ordinary case still hands back every byte. A "fix" that
    /// simply stopped collecting would satisfy the test above and silently
    /// empty every probe's stdout.
    #[test]
    fn a_reader_that_finishes_hands_back_all_of_it() {
        let reader = BoundedReader::spawn(std::io::Cursor::new(b"hello pipe".to_vec()));
        let got = reader
            .collect_by(std::time::Instant::now() + Duration::from_secs(5))
            .expect("a finished reader has bytes");
        assert_eq!(got, b"hello pipe".to_vec());
    }

    // -- the WIRING between the poll loop and the drain -------------------------

    /// An exit status, built per-platform because `ExitStatus` has no portable
    /// constructor. (Test-portability rule: a `#[cfg]` helper, never a literal
    /// that only compiles on one OS.)
    #[cfg(windows)]
    fn exited_ok() -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }
    #[cfg(unix)]
    fn exited_ok() -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt;
        std::process::ExitStatus::from_raw(0)
    }

    /// A child that is already gone the first time we ask — the shape that made
    /// the old `join()` look safe ("it exited, so its handles are closed").
    struct ExitsAtOnce;
    impl Abandonable for ExitsAtOnce {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn wait(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Pollable for ExitsAtOnce {
        fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            Ok(Some(exited_ok()))
        }
    }

    /// A child that stays alive until `at`, then exits — so the poll loop spends
    /// a KNOWN fraction of the bound before the drain begins. That fraction is
    /// the only thing that distinguishes the real deadline from one re-derived
    /// at drain time.
    struct ExitsAfter(std::time::Instant);
    impl Abandonable for ExitsAfter {
        fn kill(&mut self) -> std::io::Result<()> {
            Ok(())
        }
        fn wait(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    impl Pollable for ExitsAfter {
        fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
            Ok((std::time::Instant::now() >= self.0).then(exited_ok))
        }
    }

    /// Run `f` on its own thread and refuse to wait for it longer than `patience`.
    ///
    /// The point is the failure MODE. Every mutation this test exists to catch
    /// turns the drain into a long block (an hour, or forever), and a test that
    /// simply called the function inline would HANG rather than fail — the exact
    /// signature that let FIX 4's production half go missing without a red
    /// (`cargo test` never completes, so nobody sees a result at all). Off-thread
    /// with a `recv_timeout`, the same defect is a fast, ordinary assertion
    /// failure. The stranded thread is deliberate and harmless: `NeverEnds`
    /// already leaks one per test in this module.
    fn within<T: Send + 'static>(
        patience: Duration,
        f: impl FnOnce() -> T + Send + 'static,
    ) -> Option<T> {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
        rx.recv_timeout(patience).ok()
    }

    /// THE WIRING PIN, and the gap the final gate measured: the poll loop and
    /// the drain each honour a deadline correctly, but nothing checked they
    /// honour the SAME one. Both `collect_by(deadline)` arguments replaced with
    /// a fabricated `Instant::now() + 3600s` left 154 tests green.
    ///
    /// Brackets the elapsed time from both sides, which is what makes it a pin
    /// on the wiring rather than on the drain:
    ///   * `>= timeout` — a drain handed an already-expired or freshly-shortened
    ///     deadline would return early;
    ///   * `< patience` — a drain handed a fabricated or re-derived (doubled)
    ///     deadline, or no bound at all, would return late or never.
    /// Only the deadline the bound was computed from satisfies both.
    #[test]
    fn the_drain_is_bounded_by_the_same_clock_the_poll_loop_used() {
        let timeout = Duration::from_millis(300);
        let began = std::time::Instant::now();
        let outcome = within(Duration::from_secs(10), move || {
            // Child already exited; BOTH pipes still held open by a "grandchild".
            bounded_outcome_after_spawn(
                ExitsAtOnce,
                BoundedReader::spawn(NeverEnds),
                BoundedReader::spawn(NeverEnds),
                timeout,
            )
        });
        let elapsed = began.elapsed();

        let outcome = outcome.unwrap_or_else(|| {
            panic!(
                "the success-path drain did not return within 10s for a {timeout:?} bound — \
                 it is not using the deadline the poll loop enforced"
            )
        });
        assert!(
            matches!(outcome, BoundedOutcome::TimedOut),
            "a drain that never finished must not be reported as a completed run: {outcome:?}"
        );
        assert!(
            elapsed >= timeout,
            "returned after {elapsed:?}, before its own {timeout:?} bound — the drain was \
             handed a deadline SHORTER than the one the bound was computed from"
        );
    }

    /// THE "LOOKS RIGHT" TYPO, which neither test above can see:
    /// `collect_by(Instant::now() + timeout)`. `timeout` is in scope at the
    /// drain, so re-deriving the clock there reads as obviously correct — and it
    /// silently MULTIPLIES the bound, because the poll loop has already spent
    /// part of it and the two pipes are then collected in SEQUENCE, each buying
    /// itself a fresh full timeout. Against a child that exits at once the two
    /// spellings are indistinguishable (the loop spends nothing), which is why
    /// this test buys a real gap: the child lives for 90% of the bound.
    ///
    /// Measured, not predicted: honest 1.00s, typo 2.93s (0.9 poll + 1.0 + 1.0).
    ///
    /// The 500ms slack is the deliberate trade. It sits between the two outcomes
    /// — 500ms of headroom for the correct code, 1.4s of overshoot for the typo
    /// — rather than hugging either, because a tight bracket on a wall-clock
    /// test is how this repo has produced flakes before, and a test that goes
    /// red under load teaches people to ignore it.
    #[test]
    fn the_drain_does_not_re_derive_the_clock_the_poll_loop_already_spent() {
        let timeout = Duration::from_millis(1000);
        let poll_time = Duration::from_millis(900);
        let began = std::time::Instant::now();
        let outcome = within(Duration::from_secs(20), move || {
            bounded_outcome_after_spawn(
                ExitsAfter(std::time::Instant::now() + poll_time),
                BoundedReader::spawn(NeverEnds),
                BoundedReader::spawn(NeverEnds),
                timeout,
            )
        });
        let elapsed = began.elapsed();

        assert!(
            matches!(outcome, Some(BoundedOutcome::TimedOut)),
            "expected TimedOut, got {outcome:?}"
        );
        assert!(
            elapsed < timeout + Duration::from_millis(500),
            "took {elapsed:?} for a {timeout:?} bound after the poll loop had already spent \
             {poll_time:?} of it — the drain re-derived the deadline from `timeout` instead of \
             using the one the loop enforced, which multiplies the bound"
        );
    }

    /// …and the same wiring still hands back a real answer. A "fix" that always
    /// returned `TimedOut` would satisfy the test above and silently blind every
    /// probe in the project.
    #[test]
    fn the_wiring_still_returns_the_bytes_when_both_pipes_finish() {
        let outcome = within(Duration::from_secs(10), || {
            bounded_outcome_after_spawn(
                ExitsAtOnce,
                BoundedReader::spawn(std::io::Cursor::new(b"out".to_vec())),
                BoundedReader::spawn(std::io::Cursor::new(b"err".to_vec())),
                Duration::from_secs(5),
            )
        })
        .expect("a finished drain must not need 10s");
        match outcome {
            BoundedOutcome::Ran(out) => {
                assert_eq!(out.stdout, b"out".to_vec());
                assert_eq!(out.stderr, b"err".to_vec());
                assert!(out.status.success());
            }
            other => panic!("expected Ran, got {other:?}"),
        }
    }

    /// ONE slow pipe is enough. The drain collects stdout and stderr in
    /// sequence, so a bound applied to only the first of them still lets the
    /// second run unbounded — and stderr is the one a wedged helper usually
    /// holds.
    #[test]
    fn a_single_unfinished_pipe_still_ends_at_the_deadline() {
        for (label, stdout_ends, stderr_ends) in
            [("stdout hangs", false, true), ("stderr hangs", true, false)]
        {
            let timeout = Duration::from_millis(300);
            let outcome = within(Duration::from_secs(10), move || {
                let mk = |ends: bool| {
                    if ends {
                        BoundedReader::spawn(Box::new(std::io::Cursor::new(b"done".to_vec()))
                            as Box<dyn std::io::Read + Send>)
                    } else {
                        BoundedReader::spawn(Box::new(NeverEnds) as Box<dyn std::io::Read + Send>)
                    }
                };
                bounded_outcome_after_spawn(
                    ExitsAtOnce,
                    mk(stdout_ends),
                    mk(stderr_ends),
                    timeout,
                )
            });
            assert!(
                matches!(outcome, Some(BoundedOutcome::TimedOut)),
                "{label}: expected TimedOut within 10s, got {outcome:?}"
            );
        }
    }

    /// An ALREADY-PAST deadline is not a panic and not an infinite wait — it
    /// is the honest "no time left". `Instant` subtraction is the one arithmetic
    /// in this file that panics on underflow, and by the time the success path
    /// collects, the deadline has often only just been checked.
    #[test]
    fn collecting_after_the_deadline_has_passed_returns_at_once() {
        let reader = BoundedReader::spawn(NeverEnds);
        let began = std::time::Instant::now();
        let got = reader.collect_by(began - Duration::from_secs(30));
        assert!(got.is_none());
        assert!(began.elapsed() < Duration::from_secs(1), "{:?}", began.elapsed());
    }

    // -- run_bounded_outcome ---------------------------------------------------
    // The setup probe chain (`crate::setup`) needs "the program is not on this
    // machine" (a definitive NO) kept apart from "it ran past its deadline"
    // (evidence of nothing). `output_bounded_draining` collapses both into
    // `None`, so these three tests pin the distinction at the source.

    #[test]
    fn run_bounded_outcome_missing_program_is_spawn_failed_not_found() {
        let cmd = Command::new("definitely-not-a-real-exe-9f2.exe");
        match run_bounded_outcome(cmd, Duration::from_secs(5)) {
            BoundedOutcome::SpawnFailed(e) => {
                assert_eq!(
                    e.kind(),
                    std::io::ErrorKind::NotFound,
                    "a missing program must be reported as NotFound, not a generic failure"
                );
            }
            other => panic!("expected SpawnFailed, got {other:?}"),
        }
    }

    #[test]
    fn run_bounded_outcome_returns_ran_with_output_and_exit_code() {
        let path = fixture("captured_exit7");
        let mut cmd = Command::new(shell_program());
        cmd.args(shell_args(&path));
        match run_bounded_outcome(cmd, Duration::from_secs(10)) {
            BoundedOutcome::Ran(out) => {
                assert_eq!(out.status.code(), Some(7));
            }
            other => panic!("expected Ran, got {other:?}"),
        }
    }

    #[test]
    fn run_bounded_outcome_times_out_after_actually_spanning_the_deadline() {
        // The vacuous-pass guard (CLAUDE.md): a stubbed/broken runner that
        // returns TimedOut immediately would satisfy the variant assertion, so
        // assert something only a real wall-clock wait can satisfy — elapsed
        // must reach the deadline — while still returning promptly after it.
        let start = std::time::Instant::now();
        let mut cmd = hang_command();
        windows_no_window(&mut cmd);
        let outcome = run_bounded_outcome(cmd, Duration::from_millis(400));
        let elapsed = start.elapsed();
        assert!(
            matches!(outcome, BoundedOutcome::TimedOut),
            "an overrunning child must report TimedOut, got {outcome:?}"
        );
        assert!(
            elapsed >= Duration::from_millis(400),
            "must have actually waited out the deadline, elapsed only {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "must return promptly after the deadline, took {elapsed:?}"
        );
    }

    /// A child that outlives any short deadline, spawned DIRECTLY (one
    /// process, so the kill closes the pipes the draining threads read — the
    /// grandchild trap documented in `dml_wow::backup`'s timeout test).
    #[cfg(windows)]
    fn hang_command() -> Command {
        let mut cmd = Command::new("ping");
        cmd.args(["-n", "30", "127.0.0.1"]);
        cmd
    }
    #[cfg(not(windows))]
    fn hang_command() -> Command {
        let mut cmd = Command::new("sleep");
        cmd.arg("30");
        cmd
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

    /// THE reason `LineSplit` exists. `git clone --progress` redraws one line
    /// with `\r` and only emits `\n` when the phase ends, so the default mode
    /// hands the caller a whole download as ONE line after it is over.
    #[test]
    fn newline_only_hides_a_carriage_return_progress_phase_until_it_ends() {
        let chunk = b"Receiving objects:   1% (270/27000)\rReceiving objects:  50% (13500/27000)\r";
        let mut buf = Vec::new();
        assert!(
            drain_lines(&mut buf, chunk).is_empty(),
            "under Newline the redraws are still buffered -- nothing to report yet"
        );

        let mut buf = Vec::new();
        let got = drain_lines_split(&mut buf, chunk, LineSplit::NewlineOrReturn);
        assert_eq!(
            got,
            vec![
                "Receiving objects:   1% (270/27000)".to_string(),
                "Receiving objects:  50% (13500/27000)".to_string(),
            ],
            "each redraw must surface AS IT ARRIVES"
        );
    }

    #[test]
    fn a_crlf_does_not_produce_a_phantom_empty_line() {
        // Both terminators are line ends in this mode, so "abc\r\n" would
        // otherwise yield "abc" plus an empty string.
        let mut buf = Vec::new();
        let got = drain_lines_split(&mut buf, b"abc\r\ndef\n", LineSplit::NewlineOrReturn);
        assert_eq!(got, vec!["abc".to_string(), "def".to_string()]);
    }

    #[test]
    fn newline_mode_still_preserves_blank_lines() {
        // The docker build wall separates vertices with blank lines, and the
        // install panel's readability depends on them. Only the \r mode drops
        // empties.
        let mut buf = Vec::new();
        let got = drain_lines_split(&mut buf, b"#12 DONE\n\n#13 CACHED\n", LineSplit::Newline);
        assert_eq!(got, vec!["#12 DONE".to_string(), String::new(), "#13 CACHED".to_string()]);
    }

    #[test]
    fn a_partial_redraw_is_buffered_until_its_terminator_arrives() {
        let mut buf = Vec::new();
        assert!(drain_lines_split(&mut buf, b"Receiving objects:  7", LineSplit::NewlineOrReturn)
            .is_empty());
        let got = drain_lines_split(&mut buf, b"5% (20250/27000)\r", LineSplit::NewlineOrReturn);
        assert_eq!(got, vec!["Receiving objects:  75% (20250/27000)".to_string()]);
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

    // The two POSIX siblings of the pair above. `output_bounded` is ordinary
    // portable std code, so gating its only real-subprocess coverage behind
    // #[cfg(windows)] left it entirely untested on Linux — the platform the
    // CLI now ships on. Same assertions, platform-appropriate children.

    #[cfg(not(windows))]
    #[test]
    fn output_bounded_returns_output_for_a_fast_command() {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", "echo bounded_ok"]);
        let out = super::output_bounded(cmd, std::time::Duration::from_secs(5))
            .expect("fast command should return Some");
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).contains("bounded_ok"));
    }

    #[cfg(not(windows))]
    #[test]
    fn output_bounded_kills_and_returns_none_on_timeout() {
        // `sleep 20` spawned DIRECTLY, not via `sh -c` — one process, so the
        // kill closes the pipes the draining threads are reading and nothing
        // is left holding them open (the grandchild trap documented in
        // `dml_wow::backup`'s timeout test).
        let start = std::time::Instant::now();
        let mut cmd = std::process::Command::new("sleep");
        cmd.arg("20");
        let out = super::output_bounded(cmd, std::time::Duration::from_millis(300));
        assert!(out.is_none(), "an overrunning command must time out to None");
        assert!(
            start.elapsed() < std::time::Duration::from_secs(5),
            "must return promptly after the deadline, not wait for the child"
        );
    }
}
