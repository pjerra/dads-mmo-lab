//! LIVE, opt-in proof that the keep-alive holder works on `Backend::Wsl` —
//! the DEFAULT backend, and the only one the Settings dropdown offers today,
//! which makes it the backend the user's real server actually runs under.
//!
//! # Why this file exists
//!
//! `launcher/src-tauri/src/wsl_keepalive.rs` already has its own live proof
//! (`tests::live_the_holder_keeps_the_distro_up_and_letting_go_lets_it_die`),
//! written against `Keepalive` + `WslSpawner` directly. That test predates
//! this plan's Task 1, which changed `applies_to` so `Backend::Wsl` — not just
//! `Backend::Arch` — arms the holder. Nothing had proven the holder against
//! the backend the arming actually affects: `WslSpawner` does not take a
//! `Backend` at all, it always dials `runner::DISTRO`/`runner::USER`, so the
//! sibling test never distinguished "the holder works" from "the holder works
//! for the one backend somebody remembered to test." This file closes that
//! gap, on the crate that owns the constants both backends resolve to.
//!
//! # Why this cannot just call the production holder
//!
//! `Keepalive`, `Holder`, `Spawner` and `WslSpawner` live in
//! `launcher/src-tauri/src/wsl_keepalive.rs` — a private module of the
//! `launcher` crate, which DEPENDS ON `dml-core`. `dml-core` must never depend
//! on `launcher` (CLAUDE.md: dml-core must never depend on tauri, and the
//! dependency arrow already runs the other way), so an integration test under
//! `crates/dml-core/tests/` cannot import those types — there is no such
//! import to write.
//!
//! So this reproduces the PRODUCTION holder's exact spawn —
//! `wsl.exe -d <DISTRO> -u <USER> --exec /bin/sleep infinity`, per
//! `WslSpawner::spawn` — using [`dml_core::runner::DISTRO`] and
//! [`dml_core::runner::USER`] directly. Those are not stand-ins: they are the
//! SAME constants `DmlRunner::default()` (Backend::Wsl's own runner) resolves
//! its `-d`/`-u` prefix to, which is exactly Task 1's point — Arch and Wsl
//! drive the identical distro through the identical constant. Proving the
//! holder against these constants IS proving it for `Backend::Wsl`.
//!
//! The `wsl --list --verbose` state probe is reimplemented for the same
//! crate-boundary reason (`wsl_keepalive.rs`'s own copy is `#[cfg(test)]`,
//! private, and in the wrong crate) — but not reinvented: decoding reuses
//! [`dml_core::envelope::decode_wsl_output`], the SAME function
//! `live_arch_smoke.rs`'s `raw()` helper uses to read `wsl.exe`'s UTF-16LE
//! output, and the token-scanning loop below is a direct port of
//! `wsl_keepalive.rs`'s `distro_state`, bug fix included (see next section).
//! Release goes through [`dml_core::proc::abandon`] — the same non-blocking
//! kill-then-reap production's own `WslSession::release` uses, for the same
//! reason: CLAUDE.md's bounded-call incident measured a `kill()`-then-`wait()`
//! pair blocking 605s when the child had a grandchild holding its handles,
//! and a hang here would leave a real session holding the user's distro open
//! for as long as this shell lives.
//!
//! # Both halves are required
//!
//! Surviving past 15s alone would ALSO pass for `wsl --list --verbose`
//! polling, which the investigation measured NOT to work — it died at 14.8s
//! despite 8 polls landing inside the window; only a session INTO the distro
//! resets the timer. It is the death AFTER release that identifies the holder
//! as the cause rather than a coincidence. Measured baseline on this machine,
//! n=8: 14.7–14.9s, median 14.8, spread 0.2s.
//!
//! # Running it
//!
//!   cargo test -p dml-core --test live_wsl_keepalive -- --ignored --nocapture
//!
//! `#[ignore]` rather than a runtime skip, deliberately (see `live_arch_smoke.rs`'s
//! module doc): a self-skipping test that quietly passes on a machine with no
//! distro is the vacuous-pass trap, and this spawns against the user's REAL
//! distro.
//!
//! # Safety
//!
//!   * REFUSES outright unless `dml-arch` is already `Stopped` — it must never
//!     disturb a distro someone else is using, which may be the user's real
//!     server. No `wsl --terminate` is ever issued against a distro this test
//!     did not first confirm was already idle.
//!   * The held command is a bare `/bin/sleep infinity`. Nothing here runs a
//!     database query, a GM/SOAP verb, or a `games start`/`stop` — the only
//!     thing this test does INSIDE the distro is hold one idle session open,
//!     then let it go.
//!   * SIDE EFFECT, stated rather than discovered (same one `wsl_keepalive.rs`'s
//!     own live test documents): establishing the holder boots the distro,
//!     systemd starts dockerd, and `restart: unless-stopped` MAY bring the
//!     user's AzerothCore stack up with it if it was left in a
//!     should-be-running state. This test never touches the stack either way,
//!     and its exit path is the ordinary graceful poweroff the product already
//!     relies on every time the launcher closes — WSL asks systemd, and
//!     systemd stops dockerd (and anything dockerd is running) cleanly. Do NOT
//!     "tidy up" afterwards with `wsl --terminate`: that converts a graceful
//!     shutdown into the exact hard cut this whole plan exists to prevent.
//!   * Every assertion that could fail sits AFTER the holder has already been
//!     released — see [`HeldSession`]'s doc comment. A panic between spawn and
//!     release cannot leave an orphaned `wsl.exe`.
//!
//! # Status
//!
//! RUN, 2026-08-05: PASS. Boot after establishing the holder took 2.6s; HALF 1
//! (the hold) ran the full 40.1s with the distro reporting `Running`
//! throughout; HALF 2 (the death) landed 16.9s after release — inside the
//! ~25s expectation (15s idle timer + poweroff grace) and nowhere near the 45s
//! budget. `dml-arch` was confirmed `Stopped` afterward, and the transient
//! `vmmemWSL`/`wslrelay.exe` processes the boot created had wound down on
//! their own within a few minutes, with no `wsl.exe` client process left
//! behind at any point.

#![cfg(windows)]

use dml_core::envelope::decode_wsl_output;
use dml_core::runner::{DISTRO, USER};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Held guest-side command. Matches `wsl_keepalive::HOLDER_PROGRAM`/`HOLDER_ARG`
/// exactly — absolute path because `--exec` skips the login shell that would
/// otherwise have set a PATH.
const HOLDER_PROGRAM: &str = "/bin/sleep";
const HOLDER_ARG: &str = "infinity";

// -- the state probe: a port, not a reinvention -----------------------------
//
// `wsl_keepalive.rs`'s own `distro_state` cannot be imported (private, wrong
// crate — see the module doc). This is the same logic, decoding through
// dml-core's own `decode_wsl_output` instead of a second hand-rolled UTF-16
// check.

/// The state `wsl --list --verbose` reports for one distro, e.g. `"Running"` /
/// `"Stopped"`. `None` if the distro is not listed at all.
fn distro_state(name: &str) -> Option<String> {
    let out = Command::new("wsl.exe")
        .args(["--list", "--verbose"])
        .output()
        .expect("wsl.exe must be runnable for a live test");
    let text = decode_wsl_output(&out.stdout);
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        // `continue`, NOT `?`/`return None`. The bug this repo already found
        // once: an early return here would abort the WHOLE scan on the first
        // blank line in `wsl --list --verbose`'s output (there is at least
        // one — the header gap), which reads as "distro not found" and would
        // make a genuinely Running distro look dead.
        let Some(first) = tokens.next() else { continue };
        // The default distro is marked with a leading `*` token.
        let (distro, state) = if first == "*" {
            (tokens.next(), tokens.next())
        } else {
            (Some(first), tokens.next())
        };
        if distro == Some(name) {
            return state.map(str::to_string);
        }
    }
    None
}

fn is_running(name: &str) -> bool {
    distro_state(name).as_deref() == Some("Running")
}

// -- the holder: production's exact spawn, reproduced ------------------------

/// `wsl.exe -d <DISTRO> -u <USER> --exec /bin/sleep infinity` — byte-identical
/// to `wsl_keepalive::WslSpawner::spawn`'s argv, built from dml-core's own
/// `DISTRO`/`USER` rather than the launcher crate's re-export of them (which
/// this test cannot reach). See the module doc for why that identity is the
/// whole proof.
fn spawn_holder() -> Child {
    let mut cmd = Command::new("wsl.exe");
    cmd.args(["-d", DISTRO, "-u", USER, "--exec", HOLDER_PROGRAM, HOLDER_ARG]);
    dml_core::proc::windows_no_window(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().expect("could not start the WSL keep-alive session")
}

/// RAII guard around the held child: GUARANTEES the session is killed even if
/// a panic unwinds through the hold/observe window, on top of (not instead
/// of) the explicit release the test calls at its intended point. Production
/// has an equivalent belt-and-braces pair for the same reason — the Windows
/// job object (`wsl_keepalive.rs`'s `jobguard`, `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`)
/// exists specifically to catch the abrupt-exit case the polite path misses.
/// `release_now` is idempotent (`Option::take`), so the explicit call and the
/// `Drop` firing afterward on an empty `Option` is not a double-kill.
struct HeldSession(Option<Child>);

impl HeldSession {
    fn release_now(&mut self) {
        if let Some(c) = self.0.take() {
            dml_core::proc::abandon(c);
        }
    }
}

impl Drop for HeldSession {
    fn drop(&mut self) {
        self.release_now();
    }
}

/// `#[ignore]`d: touches the real `dml-arch` distro.
///
/// BOTH halves are required. Surviving past 15s alone would also pass for
/// `wsl --list` polling, which is MEASURED not to work (only a session INTO
/// the distro counts). It is the death AFTER release that identifies the
/// holder as the cause.
///
/// Every assertion capable of failing runs AFTER the holder is released
/// (`session.release_now()` below) — the hold-window loop only ever COLLECTS
/// evidence (`died_at`), it never asserts, so a bad result still reaches the
/// release. See `HeldSession` for the second, unconditional line of defence.
#[ignore = "touches the real dml-arch distro; run with -- --ignored"]
#[test]
fn the_wsl_backend_holds_its_distro_open_and_lets_go() {
    /// Comfortably past the ~15s deadline (2.7x) — the plan's own number for
    /// this test.
    const HOLD_FOR: Duration = Duration::from_secs(40);
    /// 15s idle timer + WSL's ~10s poweroff grace ≈ 25s expected; this is the
    /// hard ceiling the poll loop will wait before declaring "never died" a
    /// failure, with slack for scheduler/poll-granularity jitter. Loose enough
    /// not to flake on a busy box, tight enough that reaching it still means
    /// something — nowhere near the sibling live test's 90s exploratory budget.
    const DEATH_BUDGET: Duration = Duration::from_secs(45);
    /// Booting a cold distro (this test only ever starts from Stopped) takes a
    /// few seconds; generous so a slow first boot is not misread as a failure
    /// to establish.
    const BOOT_BUDGET: Duration = Duration::from_secs(60);

    // -- 1. refuse rather than disturb a distro someone else is using -------
    //
    // SAFETY: `dml-arch` is the user's REAL distro and may be running their
    // REAL server. Forcing a `wsl --terminate` to "start clean" could cut
    // that server the exact way this whole plan exists to prevent, so this
    // test only ever proceeds from an ALREADY-Stopped distro — confirmed by
    // reading its state, never assumed.
    let before = distro_state(DISTRO);
    assert_eq!(
        before.as_deref(),
        Some("Stopped"),
        "REFUSING: {DISTRO} reports {before:?}, not Stopped. This test must never disturb a \
         distro someone else may be using -- possibly the user's real server -- so it only runs \
         from an already-idle distro. If a server is running, this is not the moment to run it."
    );
    // Confirmed Stopped above, so this is a documented no-op, not a forced
    // reset: terminating an already-stopped distro ends nothing that was
    // running. Kept as its own step so the run demonstrably starts from a
    // known state rather than merely an inferred one.
    let term = Command::new("wsl.exe")
        .args(["--terminate", DISTRO])
        .output()
        .expect("wsl --terminate must be runnable");
    eprintln!(
        "[live] wsl --terminate {DISTRO} (already Stopped -- establishing a known state): exit={:?}",
        term.status.code()
    );

    // -- 2. establish the holder AS Backend::Wsl resolves it -----------------
    let t0 = Instant::now();
    let mut session = HeldSession(Some(spawn_holder()));
    eprintln!(
        "[live] holder established ({HOLDER_PROGRAM} {HOLDER_ARG} in {DISTRO} via DISTRO={DISTRO:?} USER={USER:?}); waiting for boot"
    );

    while !is_running(DISTRO) && t0.elapsed() < BOOT_BUDGET {
        std::thread::sleep(Duration::from_secs(1));
    }
    if !is_running(DISTRO) {
        // Release before panicking -- see the module doc's safety rule.
        let boot_state = distro_state(DISTRO);
        session.release_now();
        panic!(
            "{DISTRO} never came up within {BOOT_BUDGET:?} of establishing the holder \
             (state: {boot_state:?})"
        );
    }
    eprintln!(
        "[live] {DISTRO} up after {:.1}s of establishing the holder; now holding for {HOLD_FOR:?}",
        t0.elapsed().as_secs_f64()
    );

    // -- 3. HALF 1: still Running well past the ~15s deadline ---------------
    //
    // Collected, not asserted mid-loop: a panic here, before release, would
    // leave a real wsl.exe holding the user's distro open for as long as this
    // shell lives.
    let hold_start = Instant::now();
    let mut died_at: Option<f64> = None;
    while hold_start.elapsed() < HOLD_FOR {
        std::thread::sleep(Duration::from_secs(2));
        if !is_running(DISTRO) && died_at.is_none() {
            died_at = Some(hold_start.elapsed().as_secs_f64());
            break;
        }
    }
    let held_for = hold_start.elapsed();

    // -- 4. release ------------------------------------------------------
    session.release_now();
    let released_at = Instant::now();
    eprintln!("[live] released after holding {:.1}s", held_for.as_secs_f64());

    // -- 5. HALF 2: it stops within ~45s (15s timer + ~10s poweroff grace, \
    //    expected ~25s; budget carries slack) -------------------------------
    let mut death_delay: Option<f64> = None;
    while released_at.elapsed() < DEATH_BUDGET {
        std::thread::sleep(Duration::from_secs(2));
        if !is_running(DISTRO) {
            death_delay = Some(released_at.elapsed().as_secs_f64());
            break;
        }
    }

    // -- now judge, with the holder ALREADY released either way --------------
    assert_eq!(
        died_at, None,
        "{DISTRO} died {died_at:?}s into the {HOLD_FOR:?} hold -- the holder is not holding. \
         (The deadline is ~15s; anything at all here means the session is not being counted.)"
    );
    eprintln!(
        "[live] HALF 1: survived {:.1}s of holding -- {:.1}x the ~15s deadline",
        held_for.as_secs_f64(),
        held_for.as_secs_f64() / 15.0
    );

    let delay = death_delay.unwrap_or_else(|| {
        panic!(
            "{DISTRO} was STILL Running {DEATH_BUDGET:?} after the holder was released. Either \
             something else is holding the distro (check for a stray wsl.exe), or release() is \
             not actually ending the session -- in which case HALF 1 proved nothing about the \
             holder, only about something else keeping the distro up."
        )
    });
    eprintln!(
        "[live] HALF 2: died {delay:.1}s after release (expected ~15s idle timer + ~10s poweroff \
         grace ~= 25s; budget was {DEATH_BUDGET:?})"
    );
    assert!(
        delay >= 5.0,
        "the distro went down {delay:.1}s after release -- too fast to be the ~15s idle timer, \
         which means it was probably already on its way out during the hold (HALF 1 would then \
         be suspect too, not just this measurement)"
    );

    // -- final state, stated rather than assumed -----------------------------
    assert_eq!(
        distro_state(DISTRO).as_deref(),
        Some("Stopped"),
        "the test must leave {DISTRO} Stopped, the way it found it"
    );
}
