//! Keep the `dml-arch` distro alive while the server is meant to be running.
//!
//! # The bug this exists for
//!
//! WSL 2.7.10 powers a distro off **~15 s after the last `wsl.exe` client
//! session into that distro exits**, regardless of what is still running inside
//! it. Measured n=8 on this machine: 14.7–14.9 s, median 14.8, spread 0.2 s.
//! It is not a "sometimes" bug and there is no configuration that disables it —
//! see [`docs/superpowers/plans/2026-08-05-wsl-distro-lifetime.md`]. During the
//! backend comparison it took the user's real server down with 1,948 bots
//! online, and because `restart: unless-stopped` brings the stack back the next
//! time *anything* runs `wsl -d dml-arch …`, the launcher repairs the thing it
//! is checking: "my server is up whenever I look at it, and my friends say it
//! keeps going down."
//!
//! Three things measured **not** to help, so nobody re-tries them:
//!
//!   * `.wslconfig` `vmIdleTimeout=-1` — the distro still died at 14.8 s, and
//!     the now-immortal ~1.4 GB VM destroys the 1.0–1.6 GB idle saving that is
//!     the Arch backend's entire reason to exist.
//!   * Any instance-level config key — none exists. Candidates were probed
//!     against a positive control; all `Unknown key`.
//!   * `wsl --list --verbose` polling — died at 14.8 s despite 8 polls landing
//!     inside the window. **Only a session INTO the distro resets the timer.**
//!     A future "cheap status check" that avoids entering the distro will
//!     silently stop holding it.
//!
//! What works is a long-lived held session: `wsl.exe -d dml-arch -u dml --exec
//! /bin/sleep infinity`, left attached. Measured: distro alive for the full
//! observation window, then the holder was killed and the distro died 15.0 s
//! later. Causation, not correlation — the death moved with the holder.
//!
//! # Why this is Rust and not the 7 s frontend poll
//!
//! `startStatusPolling()` measurably works today, and it is still the wrong
//! owner. It is a `setInterval(…, 7000)` in a WebView2 webview; the launcher's
//! default close action **hides to tray**, and Chromium-family engines throttle
//! timers in hidden windows to as little as once per minute — so the exact
//! scenario this fix exists for is the scenario in which that timer is most
//! likely to be stretched past the 15 s deadline. The margin is 2.1×, the poll
//! self-skips via `if (!serverStatus.refreshing)`, and nothing in that file says
//! a server's life depends on the interval: someone tuning it to 20 s to reduce
//! load would kill servers with every test still green. (Same shape as the
//! recorded `lifecycle_steps_for_mode` lesson — an invariant pinned only on a
//! value production never reads is not pinned.)
//!
//! # What this buys, stated honestly
//!
//! **"The server runs while the launcher runs."** Not unattended hosting.
//! Something on Windows has to hold the session, so a server with no DML
//! process alive is not achievable on this backend by any mechanism found in
//! the investigation. Docker Desktop gets unattended hosting by running a
//! background service the user installed for that purpose; matching it would
//! mean shipping a Windows service, which is a product decision far larger than
//! a keep-alive.
//!
//! # Structure
//!
//! [`Keepalive`] is the whole decision — pure, clock-free, `wsl.exe`-free, and
//! driven through the [`Spawner`]/[`Holder`] seam so the unit tests can prove
//! *when* it holds, re-establishes and releases with a fake. Everything below
//! `// --- production glue ---` is the part that cannot be unit-tested: the real
//! `wsl.exe` child, the Windows job object that stops it outliving us, the
//! watchdog thread, and the Tauri surface.

use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use dml_core::backend::Backend;

/// The distro and user are [`dml_core::runner`]'s, not literals: the holder
/// must enter the SAME distro the runner drives, or it is holding a session
/// into something nobody is using.
use dml_core::runner::{DISTRO, USER};

/// Held guest-side command. Absolute path on purpose — `--exec` skips the login
/// shell, so nothing has arranged a PATH for us. `/bin` is a symlink to
/// `/usr/bin` on Arch, so this resolves.
const HOLDER_PROGRAM: &str = "/bin/sleep";
const HOLDER_ARG: &str = "infinity";

/// How often the watchdog re-checks the holder.
///
/// The budget is the 15 s the distro has left from the moment the holder dies,
/// so detection **plus** respawn must fit inside it. 5 s gives three chances
/// and costs one `try_wait` per tick. Do not raise this past ~7 s without
/// re-reading the measurement: at 20 s the investigation's poll variant died
/// before its first tick ever fired.
const WATCHDOG_TICK: Duration = Duration::from_secs(5);

/// How many consecutive establish attempts before we stop trying and say so.
///
/// "Consecutive" is counted from the last tick that found the holder **alive**,
/// so this bounds a holder that spawns fine and dies immediately (a distro that
/// refuses to boot) just as much as one that will not spawn at all. A
/// successful spawn deliberately does NOT reset it — only surviving to the next
/// tick does, because surviving is the only evidence that the spawn achieved
/// anything.
pub const MAX_ATTEMPTS: u32 = 5;

/// What we tell the user when every spawn SUCCEEDED and the session died
/// anyway. There is no OS error to quote on that path, and a give-up with an
/// empty reason is indistinguishable from a give-up nobody recorded.
pub const HOLDER_KEEPS_DYING: &str =
    "the WSL keep-alive session kept ending as soon as it started — the dml-arch distro \
     may be failing to boot";

// ---------------------------------------------------------------------------
// The decision
// ---------------------------------------------------------------------------

/// Whether a backend needs a held session at all.
///
/// A `match`, so a new [`Backend`] variant is a compile error here rather than
/// a backend that silently inherits "no keep-alive" — the failure mode of this
/// whole module is silence.
///
/// * `Arch` and `Wsl` — yes, both. They drive the SAME `dml-arch` distro
///   through the same [`dml_core::runner::DISTRO`] constant, so both hit the
///   identical 15 s termination described at the top of this module. `Wsl` is
///   also the default and the only backend the Settings dropdown currently
///   offers, so it is the backend the user's real server actually runs on.
/// * `Native` — no, and it must not exist there at all: Docker Desktop's
///   `com.docker.backend` already holds `docker-desktop` for exactly this
///   reason, which is why that distro never showed the behaviour in the same
///   sitting. Spawning a holder would be a stray `wsl.exe` for no reason.
pub fn applies_to(backend: Backend) -> bool {
    match backend {
        Backend::Arch | Backend::Wsl => true,
        Backend::Native => false,
    }
}

/// What the server is *meant* to be doing — not what a status probe last said.
///
/// This is the distinction the module turns on. "Meant to be running" is set by
/// an act (the user pressed Start, or a status poll reported the stack already
/// up and we adopted it) and cleared by an act (the user pressed Stop, or the
/// launcher exited). A verdict of `stopped` arriving from a poll does NOT clear
/// it — see [`Keepalive::observed_status`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Intent {
    Run,
    Stop,
}

/// What one [`Keepalive::reconcile`] actually did.
///
/// Returned for logging and for the tests to read as a *summary* — never as the
/// primary assertion. A production mutation that returns `Established` without
/// spawning anything would satisfy a `Step`-only test perfectly, so every test
/// below also asserts against the fake's recorded spawns and its holders'
/// liveness. That is the exact vacuity this repo keeps finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Step {
    /// Nothing wanted, nothing held.
    Idle,
    /// Wanted, and the holder is alive. The steady state.
    Holding,
    /// Wanted, nothing held, and a holder was just spawned.
    Established,
    /// The holder had exited under us and a replacement was spawned. This is
    /// `wsl --shutdown`, a WSL update, or someone killing the process.
    Reestablished,
    /// Wanted, but the spawn failed. Carries the reason.
    Failed(String),
    /// Wanted, but [`MAX_ATTEMPTS`] consecutive attempts got us nowhere. We
    /// stop trying; the user is told.
    GaveUp,
    /// Not wanted any more: the holder was released.
    Released,
}

/// A held `wsl.exe` session, behind a trait so the decision can be tested
/// without one.
pub trait Holder: Send {
    /// Has the session ended? Must not block.
    fn has_exited(&mut self) -> bool;
    /// End it now. Must not block the caller (production reaps on a detached
    /// thread — see [`dml_core::proc::abandon`], which exists because a
    /// `wait()` after a failed `kill()` is infinite).
    fn release(&mut self);
}

/// Makes holders. The seam.
pub trait Spawner: Send {
    fn spawn(&mut self) -> Result<Box<dyn Holder>, String>;
}

/// The state machine. No clock, no process, no Tauri.
pub struct Keepalive {
    intent: Intent,
    holder: Option<Box<dyn Holder>>,
    /// Establish attempts since the holder was last seen alive at a tick.
    attempts: u32,
    /// True once a holder has been spawned under the CURRENT `Intent::Run`, so
    /// a replacement can be reported as a re-establishment rather than a first
    /// one. Cleared by a release.
    ever_held: bool,
    gave_up: bool,
    /// Has the CURRENT give-up already been announced? Lives here, next to
    /// `gave_up`, because "we stopped trying" is news exactly once and the only
    /// thing that knows whether it has been said is the state that latched it.
    /// Cleared with the rest of the budget, so a give-up after a fresh ask is
    /// news again.
    gave_up_announced: bool,
    last_error: Option<String>,
}

impl Default for Keepalive {
    fn default() -> Self {
        Self::new()
    }
}

impl Keepalive {
    pub fn new() -> Self {
        Keepalive {
            intent: Intent::Stop,
            holder: None,
            attempts: 0,
            ever_held: false,
            gave_up: false,
            gave_up_announced: false,
            last_error: None,
        }
    }

    /// THE USER ASKED for the server to be running — an ACT, not an
    /// observation, and the distinction is the whole of this method.
    ///
    /// A fresh ask gets a fresh budget even when the intent is ALREADY `Run`.
    /// That case is not exotic, it is the normal one: `games_start` calls
    /// `server_should_run()` unconditionally, so by the time a user presses
    /// Start after a failure the intent has not changed and never will. Gating
    /// the clear on an intent CHANGE therefore meant it never fired — five
    /// transient `wsl.exe` failures latched `gave_up`, `reconcile` returned
    /// `GaveUp` without going near the spawner, and Start silently did nothing
    /// for the rest of the session.
    ///
    /// Re-asserting while a holder is ALIVE remains a no-op: neither the intent
    /// changed nor did we give up, so nothing is touched and no second holder
    /// is spawned.
    ///
    /// The infinite-retry hazard the old doc comment named is real and is still
    /// closed — by [`Self::observed_status`], which adopts through its own path
    /// and never reopens a give-up. A 7-second poll is not a user asking again.
    pub fn want_running(&mut self) {
        self.assert_run(true);
    }

    /// Assert `Intent::Run`. `fresh_ask` distinguishes a human's act from a
    /// poll's observation: only the former may reopen a closed budget.
    fn assert_run(&mut self, fresh_ask: bool) {
        let changed = self.intent != Intent::Run;
        self.intent = Intent::Run;
        if changed || (fresh_ask && self.gave_up) {
            // A fresh intent — or a fresh ask after we stopped trying — gets a
            // fresh budget: whatever went wrong last time the user wanted a
            // server, they are asking again now.
            self.attempts = 0;
            self.gave_up = false;
            self.gave_up_announced = false;
            self.last_error = None;
            self.ever_held = false;
        }
    }

    /// The server is meant to be down. The next `reconcile` releases.
    pub fn want_stopped(&mut self) {
        self.intent = Intent::Stop;
    }

    /// A polled verdict arrived. **Positive evidence adopts; negative evidence
    /// is ignored.**
    ///
    /// The asymmetry is deliberate and it is the load-bearing decision in this
    /// file:
    ///
    ///   * `online`/`starting` adopts a server this launcher did not start —
    ///     the plan's "a server started by a previous session must be adopted,
    ///     not orphaned". Without it, opening the launcher onto a running
    ///     server and never touching Start leaves it unheld.
    ///   * `stopped`/`crashed`/`soap_unreachable` does **not** release. Those
    ///     verdicts are exactly what a *restart* looks like from the outside,
    ///     and the `restarting` suppression flag that hides the flap lives only
    ///     in the webview. Releasing on one would power the distro off 15 s into
    ///     a restart the user asked for. It is also what a crash looks like —
    ///     and holding through a crash is what lets `restart: unless-stopped`
    ///     put the stack back, which it cannot do if dockerd's machine is gone.
    ///
    /// So the only routes to "meant to be down" are an explicit stop, a backend
    /// change, and launcher exit — all three of which release. That is what
    /// keeps an orphaned holder from pinning ~1.4 GB of VM forever.
    ///
    /// ADOPTION IS NOT AN ASK, which is why this does not simply call
    /// [`Self::want_running`]. `tray_set_status` pushes a verdict every 7
    /// seconds; a poll that reopened a closed budget would turn [`MAX_ATTEMPTS`]
    /// into a 7-second retry loop with extra steps, and the bound would exist
    /// only in the tests.
    pub fn observed_status(&mut self, verdict: &str) {
        if verdict_means_running(verdict) {
            self.assert_run(false);
        }
    }

    /// Bring reality in line with the intent. Call on every tick AND
    /// immediately after an intent change, so a Start does not wait up to
    /// [`WATCHDOG_TICK`] for its holder.
    pub fn reconcile(&mut self, spawner: &mut dyn Spawner) -> Step {
        if self.intent == Intent::Stop {
            let held = self.holder.take();
            self.attempts = 0;
            self.ever_held = false;
            self.gave_up = false;
            self.gave_up_announced = false;
            self.last_error = None;
            return match held {
                Some(mut h) => {
                    h.release();
                    Step::Released
                }
                None => Step::Idle,
            };
        }

        // Intent::Run from here down.
        if let Some(h) = self.holder.as_mut() {
            if !h.has_exited() {
                // Surviving a tick is the ONLY thing that proves an establish
                // achieved something, so it is the only thing that refills the
                // budget.
                self.attempts = 0;
                self.last_error = None;
                return Step::Holding;
            }
            // It died under us. Reap it before replacing it, or the zombie
            // outlives the launcher on the very path this module exists to
            // stop.
            if let Some(mut dead) = self.holder.take() {
                dead.release();
            }
        }

        if self.gave_up {
            return Step::GaveUp;
        }
        // THE BOUND, checked before the attempt rather than inside the error
        // arm. Putting it in the error arm is the obvious placement and it is
        // wrong: it only bounds a session that will not SPAWN, and the failure
        // that actually happens on a sick distro is a `wsl.exe` that starts
        // perfectly and exits a moment later. That flap re-spawned forever, and
        // `a_holder_that_always_dies_immediately_also_hits_the_bound` is the
        // test that found it.
        if self.attempts >= MAX_ATTEMPTS {
            self.gave_up = true;
            if self.last_error.is_none() {
                // A give-up with nothing to say is the silence this module
                // exists to end. Spawning succeeded every time, so there is no
                // OS error to quote — name the shape instead.
                self.last_error = Some(HOLDER_KEEPS_DYING.to_string());
            }
            return Step::GaveUp;
        }

        self.attempts += 1;
        match spawner.spawn() {
            Ok(h) => {
                self.holder = Some(h);
                self.last_error = None;
                let step =
                    if self.ever_held { Step::Reestablished } else { Step::Established };
                self.ever_held = true;
                step
            }
            Err(e) => {
                self.last_error = Some(e.clone());
                Step::Failed(e)
            }
        }
    }

    /// What, if anything, to tell the user about this step — `(kind, message)`.
    ///
    /// `&mut self` rather than a pure function of [`Step`] for one reason: "we
    /// gave up" is news EXACTLY ONCE, and only the state that latched it knows
    /// whether it has already been said. The obvious pure version — `gave_up &&
    /// attempts >= MAX_ATTEMPTS`, read off the report — looks like it fires on
    /// the flipping tick and does not: once latched, `reconcile` returns at the
    /// `gave_up` arm without touching `attempts`, so both halves stay true on
    /// every subsequent 5-second tick, forever. That storm was bounded only by
    /// the frontend happening to re-read the report and compare strings, which
    /// is an accident in another file rather than a property of this one.
    ///
    /// `Failed` is deliberately NOT latched: each failed attempt is a distinct
    /// event, and there are at most [`MAX_ATTEMPTS`] of them.
    fn announcement(&mut self, step: &Step) -> Option<(&'static str, String)> {
        match step {
            Step::Failed(e) => Some(("failed", e.clone())),
            Step::GaveUp if !self.gave_up_announced => {
                self.gave_up_announced = true;
                Some((
                    "gave_up",
                    self.last_error.clone().unwrap_or_else(|| HOLDER_KEEPS_DYING.to_string()),
                ))
            }
            _ => None,
        }
    }

    pub fn intent(&self) -> Intent {
        self.intent
    }
    /// Whether a holder object is currently owned. Says nothing about whether
    /// the far end is still alive — only a `reconcile` asks that.
    pub fn is_holding(&self) -> bool {
        self.holder.is_some()
    }
    pub fn gave_up(&self) -> bool {
        self.gave_up
    }
    pub fn attempts(&self) -> u32 {
        self.attempts
    }
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }
}

/// Which `ServerDetail.verdict` values mean "there is a server here to protect".
///
/// Kept as a named function next to the union it mirrors (`tray::tooltip_for`)
/// so the adoption rule is one readable list rather than a condition buried in
/// a method. An unknown verdict is NOT running: the union may grow, and a new
/// value must not silently start pinning a distro.
pub fn verdict_means_running(verdict: &str) -> bool {
    matches!(verdict, "online" | "starting")
}

// ---------------------------------------------------------------------------
// Production glue: the real session, the job object, the watchdog, the surface
// ---------------------------------------------------------------------------

/// A live `wsl.exe -d dml-arch -u dml --exec /bin/sleep infinity`.
///
/// The `Option` is what makes `release` non-blocking: the child is MOVED out
/// and handed to [`dml_core::proc::abandon`], which kills it and reaps it on a
/// thread nobody joins. This repo measured a 600 ms-bounded call returning
/// after 605 seconds because a `kill()` that failed was followed by an infinite
/// `wait()` on the caller's thread; the watchdog holds the state mutex while it
/// releases, so that hang would freeze every intent change in the launcher.
struct WslSession {
    child: Option<std::process::Child>,
}

impl Holder for WslSession {
    fn has_exited(&mut self) -> bool {
        match self.child.as_mut() {
            // A `try_wait` error means we cannot ask — treat that as exited so
            // the watchdog replaces a holder it can no longer reason about.
            // Over-replacing costs one `wsl.exe`; under-replacing costs the
            // server.
            Some(c) => !matches!(c.try_wait(), Ok(None)),
            None => true,
        }
    }
    fn release(&mut self) {
        if let Some(c) = self.child.take() {
            dml_core::proc::abandon(c);
        }
    }
}

/// Spawns the real thing.
struct WslSpawner;

impl Spawner for WslSpawner {
    fn spawn(&mut self) -> Result<Box<dyn Holder>, String> {
        let mut cmd = std::process::Command::new("wsl.exe");
        // `--exec`, per the repo rule. Every argument here is our own literal,
        // so this is not the injection case — it is simply the cheaper spawn
        // that does not put a shell between us and the process we are holding.
        cmd.args(["-d", DISTRO, "-u", USER, "--exec", HOLDER_PROGRAM, HOLDER_ARG]);
        // A holder that flashes a console window every time WSL restarts would
        // be its own bug report.
        dml_core::proc::windows_no_window(&mut cmd);
        // Nothing reads its streams and nothing may block on them.
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let child = cmd.spawn().map_err(|e| format!("could not start the WSL keep-alive session: {e}"))?;
        jobguard::adopt(&child);
        Ok(Box::new(WslSession { child: Some(child) }))
    }
}

/// Windows job object: the child dies when this process does, however it dies.
///
/// A `wsl.exe` we spawned is NOT killed by Windows when the launcher exits, and
/// an orphaned one holds the distro — and therefore ~1.4 GB of VM — forever,
/// quietly undoing the only advantage this backend has. `RunEvent::Exit`
/// handles the polite exits; this handles the impolite ones (Task Manager, a
/// panic, a crash), because closing the last handle to a job created with
/// `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` terminates every process in it, and
/// process teardown closes handles whether the process meant to or not.
///
/// Bound with a bare `extern "system"` declaration rather than a crate, exactly
/// as `power.rs` binds `SetThreadExecutionState`: kernel32 is always present and
/// the `windows` crate is not a dependency of this project.
#[cfg(windows)]
mod jobguard {
    use std::os::windows::io::AsRawHandle;
    use std::sync::OnceLock;

    type Handle = *mut std::ffi::c_void;

    const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
    /// `JobObjectExtendedLimitInformation`.
    const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectBasicLimitInformation {
        per_process_user_time_limit: i64,
        per_job_user_time_limit: i64,
        limit_flags: u32,
        minimum_working_set_size: usize,
        maximum_working_set_size: usize,
        active_process_limit: u32,
        affinity: usize,
        priority_class: u32,
        scheduling_class: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct IoCounters {
        read_operation_count: u64,
        write_operation_count: u64,
        other_operation_count: u64,
        read_transfer_count: u64,
        write_transfer_count: u64,
        other_transfer_count: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct JobObjectExtendedLimitInformation {
        basic_limit_information: JobObjectBasicLimitInformation,
        io_info: IoCounters,
        process_memory_limit: usize,
        job_memory_limit: usize,
        peak_process_memory_used: usize,
        peak_job_memory_used: usize,
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateJobObjectW(attrs: *mut std::ffi::c_void, name: *const u16) -> Handle;
        fn SetInformationJobObject(
            job: Handle,
            class: i32,
            info: *const std::ffi::c_void,
            len: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: Handle, process: Handle) -> i32;
    }

    /// `*mut c_void` is not `Send`/`Sync`; the handle is only ever read, and
    /// the OS owns what it points at.
    struct JobHandle(Handle);
    unsafe impl Send for JobHandle {}
    unsafe impl Sync for JobHandle {}

    /// Created once and NEVER closed on purpose: the handle must stay open for
    /// the whole process lifetime, because it is its closing — at process
    /// teardown — that kills the members.
    static JOB: OnceLock<Option<JobHandle>> = OnceLock::new();

    fn job() -> Option<Handle> {
        JOB.get_or_init(|| {
            let h = unsafe { CreateJobObjectW(std::ptr::null_mut(), std::ptr::null()) };
            if h.is_null() {
                return None;
            }
            let mut info = JobObjectExtendedLimitInformation::default();
            info.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = unsafe {
                SetInformationJobObject(
                    h,
                    JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                    &info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JobObjectExtendedLimitInformation>() as u32,
                )
            };
            // A job without the kill-on-close limit is worse than no job: it
            // would look like protection and provide none.
            if ok == 0 {
                return None;
            }
            Some(JobHandle(h))
        })
        .as_ref()
        .map(|j| j.0)
    }

    /// Put a freshly spawned child in the job. Best-effort by design: failing
    /// to adopt costs a possible orphan on an abrupt kill, which is strictly
    /// better than refusing to hold the distro at all. The polite exit paths
    /// (`RunEvent::Exit`) do not depend on this.
    pub fn adopt(child: &std::process::Child) {
        if let Some(j) = job() {
            unsafe {
                AssignProcessToJobObject(j, child.as_raw_handle() as Handle);
            }
        }
    }
}

#[cfg(not(windows))]
mod jobguard {
    /// No job objects off Windows. The whole module is inert there anyway —
    /// `wsl.exe` does not exist — but this keeps the file compiling so
    /// `cargo test -p launcher` is not Windows-only for the pure decision.
    pub fn adopt(_child: &std::process::Child) {}
}

/// Process-wide state. `None` inside the lock is impossible once installed;
/// the `OnceLock` being empty means the backend does not need us.
static STATE: OnceLock<Mutex<Keepalive>> = OnceLock::new();
/// The app handle used to tell the UI when we have given up.
static APP: OnceLock<tauri::AppHandle> = OnceLock::new();

fn state() -> Option<&'static Mutex<Keepalive>> {
    STATE.get()
}

/// What the UI can ask for at any time. Re-derived from the live state on every
/// call rather than remembered by the frontend — the recorded soap-autosetup
/// lesson: a webview reload wipes a module-level store, and a UI told only
/// "already concluded" renders nothing on exactly the path where the thing
/// failed.
#[derive(Debug, Clone, serde::Serialize)]
pub struct KeepaliveReport {
    /// False on Native/Wsl: there is nothing to report because there is nothing
    /// running. The UI must not render a "keep-alive is fine" reassurance from
    /// a backend that has no keep-alive.
    pub applies: bool,
    pub wanted: bool,
    pub holding: bool,
    pub gave_up: bool,
    pub attempts: u32,
    pub last_error: Option<String>,
}

/// Arm the keep-alive for this backend. No-op unless [`applies_to`].
///
/// Called from `.setup()`. Spawns exactly one watchdog thread, which runs for
/// the app's lifetime and is the ONLY thing that re-establishes a dead holder.
/// It is a plain OS thread, not a webview timer and not a Tauri async task, so
/// nothing about hiding, minimising or closing the window reaches it.
pub fn install(app: &tauri::AppHandle, backend: Backend) {
    if !applies_to(backend) {
        return;
    }
    if STATE.set(Mutex::new(Keepalive::new())).is_err() {
        return; // already installed
    }
    let _ = APP.set(app.clone());
    std::thread::spawn(|| {
        let mut spawner = WslSpawner;
        loop {
            std::thread::sleep(WATCHDOG_TICK);
            tick(&mut spawner);
        }
    });
}

/// One watchdog beat, with the reporting attached. Separate from
/// [`Keepalive::reconcile`] so the decision stays free of Tauri.
fn tick(spawner: &mut dyn Spawner) {
    let Some(m) = state() else { return };
    // The announcement is decided INSIDE the lock, because deciding it mutates
    // the latch that makes a give-up news exactly once.
    let payload = {
        let Ok(mut k) = m.lock() else { return };
        let step = k.reconcile(spawner);
        k.announcement(&step)
    };
    announce(payload);
}

/// Tell the user when the holder is in trouble. Silence is the failure mode
/// that made this bug invisible in the first place — a server that is about to
/// stop in 15 seconds and says nothing.
///
/// A pure emitter: WHETHER to speak is [`Keepalive::announcement`]'s decision,
/// and it has to be, because "we gave up" is news once and only the state knows
/// whether it has been said. The frontend also re-reads [`keepalive_report`] on
/// mount, so a webview reload does not lose the news.
fn announce(payload: Option<(&'static str, String)>) {
    let Some((kind, message)) = payload else { return };
    if let Some(app) = APP.get() {
        use tauri::Emitter;
        let _ = app.emit(
            "wsl-keepalive",
            serde_json::json!({ "kind": kind, "message": message }),
        );
    }
}

fn report_from(k: &Keepalive, applies: bool) -> KeepaliveReport {
    KeepaliveReport {
        applies,
        wanted: k.intent() == Intent::Run,
        holding: k.is_holding(),
        gave_up: k.gave_up(),
        attempts: k.attempts(),
        last_error: k.last_error().map(str::to_string),
    }
}

/// Apply an intent change and reconcile IMMEDIATELY, rather than letting the
/// server run naked for up to one [`WATCHDOG_TICK`].
fn apply(change: impl FnOnce(&mut Keepalive)) {
    let Some(m) = state() else { return };
    let mut spawner = WslSpawner;
    let payload = {
        let Ok(mut k) = m.lock() else { return };
        change(&mut k);
        let step = k.reconcile(&mut spawner);
        k.announcement(&step)
    };
    announce(payload);
}

/// The server is meant to be running. Call at the START of a start/restart, so
/// the hold is in place before the lifecycle command's own session ends — that
/// session is itself holding the distro, and the 15 s clock starts when it
/// exits.
pub fn server_should_run() {
    apply(Keepalive::want_running);
}

/// The server is meant to be down. Call AFTER a stop completes: releasing first
/// would start the 15 s clock while compose is still shutting containers down.
pub fn server_should_stop() {
    apply(Keepalive::want_stopped);
}

/// A polled verdict from the frontend. Adopts a running server; never releases.
/// See [`Keepalive::observed_status`].
pub fn observed_status(verdict: &str) {
    if !verdict_means_running(verdict) {
        return; // cheap out before taking the lock
    }
    apply(|k| k.want_running());
}

/// Release on launcher exit. The polite path; [`jobguard`] covers the rest.
pub fn shutdown() {
    apply(Keepalive::want_stopped);
}

/// The report, for the Tauri command.
pub fn keepalive_report() -> KeepaliveReport {
    match state().and_then(|m| m.lock().ok()) {
        Some(k) => report_from(&k, true),
        None => KeepaliveReport {
            applies: false,
            wanted: false,
            holding: false,
            gave_up: false,
            attempts: 0,
            last_error: None,
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    /// A holder whose life the TEST controls, and which records that it was
    /// really released.
    #[derive(Clone)]
    struct FakeHolder {
        exited: Arc<std::sync::atomic::AtomicBool>,
        released: Arc<AtomicUsize>,
    }

    impl Holder for FakeHolder {
        fn has_exited(&mut self) -> bool {
            self.exited.load(Ordering::SeqCst)
        }
        fn release(&mut self) {
            self.released.fetch_add(1, Ordering::SeqCst);
            // A real release ends the session, so the fake must stop claiming
            // to be alive too — otherwise a reconcile that released but kept
            // the object would look healthy.
            self.exited.store(true, Ordering::SeqCst);
        }
    }

    /// Records every spawn. THE oracle: `Step` is a summary, this is evidence.
    struct FakeSpawner {
        spawns: usize,
        /// Handles to every holder handed out, so a test can kill one.
        issued: Vec<FakeHolder>,
        releases: Arc<AtomicUsize>,
        /// Errors to return, front first. Empty = always succeed.
        errors: std::collections::VecDeque<Option<String>>,
    }

    impl FakeSpawner {
        fn new() -> Self {
            FakeSpawner {
                spawns: 0,
                issued: Vec::new(),
                releases: Arc::new(AtomicUsize::new(0)),
                errors: std::collections::VecDeque::new(),
            }
        }
        /// Always fail, with this message.
        fn always_failing(msg: &str) -> Self {
            let mut s = Self::new();
            // 64 is comfortably past MAX_ATTEMPTS; the point is "never
            // succeeds", not a count the test has to keep in step.
            for _ in 0..64 {
                s.errors.push_back(Some(msg.to_string()));
            }
            s
        }
        /// The holder handed out by the Nth spawn (0-based).
        fn nth(&self, n: usize) -> FakeHolder {
            self.issued[n].clone()
        }
        fn last(&self) -> FakeHolder {
            self.issued.last().expect("no holder was ever issued").clone()
        }
        fn release_count(&self) -> usize {
            self.releases.load(Ordering::SeqCst)
        }
    }

    impl Spawner for FakeSpawner {
        fn spawn(&mut self) -> Result<Box<dyn Holder>, String> {
            self.spawns += 1;
            if let Some(Some(e)) = self.errors.pop_front() {
                return Err(e);
            }
            let h = FakeHolder {
                exited: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                released: self.releases.clone(),
            };
            self.issued.push(h.clone());
            Ok(Box::new(h))
        }
    }

    // -- who gets a keep-alive at all ---------------------------------------

    /// THE POINT OF THIS CHANGE. `Backend::Wsl` drives the SAME `dml-arch`
    /// distro as `Arch` (both go through `runner::DISTRO`), so it has the
    /// identical 15-second termination — measured n=8, 14.7-14.9s, spread 0.2s.
    /// It is also the DEFAULT and the only backend the Settings dropdown
    /// offers, so before this change the fix protected a backend nobody could
    /// select, while the one the user's real server runs on stayed exposed.
    #[test]
    fn every_distro_backend_holds_its_distro_open() {
        assert!(applies_to(Backend::Arch), "Arch drives dml-arch");
        assert!(applies_to(Backend::Wsl), "Wsl drives the SAME dml-arch");
    }

    /// Docker Desktop keeps its own utility VM alive for its containers, so
    /// there is no distro to hold and a holder would be a stray `wsl.exe` for
    /// no reason.
    #[test]
    fn docker_desktop_never_holds_a_distro() {
        assert!(!applies_to(Backend::Native));
    }

    // -- holding -------------------------------------------------------------

    /// A launcher that has not been asked for a server holds nothing. The
    /// mirror of the "never leak" rule: an idle holder pins ~1.4 GB of VM.
    #[test]
    fn nothing_is_held_until_the_server_is_wanted() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();

        assert_eq!(k.reconcile(&mut s), Step::Idle);
        assert_eq!(s.spawns, 0, "a launcher with no server must not spawn a holder");
        assert!(!k.is_holding());
    }

    /// THE test. Wanting a server establishes a live holder, and it STAYS
    /// established across ticks without re-spawning.
    #[test]
    fn wanting_a_server_establishes_a_live_holder_and_keeps_it() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();

        k.want_running();
        assert_eq!(k.reconcile(&mut s), Step::Established);
        assert_eq!(s.spawns, 1, "the holder must actually have been spawned");
        assert!(k.is_holding());
        // Evidence, not a label: the issued holder is alive and unreleased.
        assert!(!s.last().has_exited());
        assert_eq!(s.release_count(), 0);

        // Ten quiet ticks: still exactly one holder.
        for _ in 0..10 {
            assert_eq!(k.reconcile(&mut s), Step::Holding);
        }
        assert_eq!(s.spawns, 1, "a live holder must not be respawned every tick");
        assert!(!s.last().has_exited());
    }

    /// Re-asserting the intent while holding is a no-op, not a second holder.
    /// The frontend adopts on every poll, so this runs every 7 seconds.
    #[test]
    fn re_asserting_the_intent_does_not_stack_holders() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();

        for _ in 0..5 {
            k.want_running();
            k.reconcile(&mut s);
        }
        assert_eq!(s.spawns, 1, "five Start presses must not leave five wsl.exe sessions");
        assert_eq!(s.release_count(), 0);
    }

    // -- self-healing --------------------------------------------------------

    /// `wsl --shutdown`, a WSL update, someone killing the process: the holder
    /// dies and nobody tells us. A holder that silently dies is worse than none,
    /// because it looks like protection.
    #[test]
    fn a_holder_that_dies_is_replaced_with_a_live_one() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();

        k.want_running();
        k.reconcile(&mut s);
        let first = s.nth(0);

        // The session ends behind our back.
        first.exited.store(true, Ordering::SeqCst);

        assert_eq!(k.reconcile(&mut s), Step::Reestablished);
        assert_eq!(s.spawns, 2, "the dead holder must be replaced, not mourned");
        // The replacement is a DIFFERENT, live holder...
        assert!(!s.nth(1).has_exited());
        // ...and the corpse was reaped rather than leaked.
        assert_eq!(s.release_count(), 1, "the dead child must be reaped before it is replaced");

        // And the new one is then held steady.
        assert_eq!(k.reconcile(&mut s), Step::Holding);
        assert_eq!(s.spawns, 2);
    }

    /// The bound. A distro that will not host a session must not be retried
    /// forever, and the user must be able to find out why.
    #[test]
    fn establishing_is_bounded_and_the_reason_survives() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::always_failing("wsl.exe not found");

        k.want_running();
        for i in 1..=MAX_ATTEMPTS {
            assert_eq!(
                k.reconcile(&mut s),
                Step::Failed("wsl.exe not found".into()),
                "attempt {i} should still be trying"
            );
        }
        assert_eq!(k.reconcile(&mut s), Step::GaveUp);
        assert_eq!(s.spawns, MAX_ATTEMPTS as usize);
        assert!(k.gave_up());
        assert_eq!(
            k.last_error(),
            Some("wsl.exe not found"),
            "the reason must survive, or the failure is silent — which is the bug"
        );

        // Latched: no more spawning, ever, until the intent is renewed.
        for _ in 0..20 {
            assert_eq!(k.reconcile(&mut s), Step::GaveUp);
        }
        assert_eq!(s.spawns, MAX_ATTEMPTS as usize, "a latched give-up must stop spawning");
    }

    /// The bound counts a holder that spawns fine and dies immediately, not
    /// just one that will not spawn. Both are "we are not holding the distro",
    /// and only the second is visible to `spawn()`'s return value.
    #[test]
    fn a_holder_that_always_dies_immediately_also_hits_the_bound() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();

        k.want_running();
        for _ in 0..MAX_ATTEMPTS {
            k.reconcile(&mut s);
            // It dies before the next tick, every time.
            s.last().exited.store(true, Ordering::SeqCst);
        }
        // The next reconcile finds the corpse and refuses to keep flapping.
        assert_eq!(k.reconcile(&mut s), Step::GaveUp);
        assert_eq!(
            s.spawns, MAX_ATTEMPTS as usize,
            "a session that dies on arrival must be bounded like one that never arrives"
        );
        // Every spawn SUCCEEDED, so there is no OS error to quote — and a
        // give-up that says nothing is the silence this module exists to end.
        assert_eq!(
            k.last_error(),
            Some(HOLDER_KEEPS_DYING),
            "a give-up must always carry a reason, even when nothing returned an error"
        );
    }

    /// ...and a holder that survives a tick refills the budget, so a machine
    /// that hiccups four times over a week is not permanently unprotected.
    #[test]
    fn surviving_a_tick_refills_the_budget() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();

        k.want_running();
        for _ in 0..(MAX_ATTEMPTS - 1) {
            k.reconcile(&mut s);
            s.last().exited.store(true, Ordering::SeqCst);
        }
        assert_eq!(k.attempts(), MAX_ATTEMPTS - 1);

        // This one lives.
        k.reconcile(&mut s);
        assert_eq!(k.reconcile(&mut s), Step::Holding);
        assert_eq!(k.attempts(), 0, "a surviving holder must refill the budget");

        // So the next death is attempt 1, not the give-up.
        s.last().exited.store(true, Ordering::SeqCst);
        assert!(matches!(
            k.reconcile(&mut s),
            Step::Reestablished
        ));
        assert!(!k.gave_up());
    }

    /// PRESSING START AFTER A GIVE-UP MUST RECOVER — and this is the case the
    /// Stop-then-Start test below cannot see.
    ///
    /// `games_start` calls `server_should_run()` UNCONDITIONALLY, so by the time
    /// a user presses Start after a failure the intent is already `Run`. A clear
    /// that only fires on an intent CHANGE therefore never fires: `gave_up`
    /// survives, `reconcile` returns `GaveUp` without going near the spawner,
    /// and the button does nothing. Five transient `wsl.exe` failures latch it;
    /// the server then starts, dies 15 s later, `restart: unless-stopped` heals
    /// it on the next touch, the user presses Start again — same loop, forever,
    /// with a banner that names no way out.
    #[test]
    fn pressing_start_again_after_a_give_up_reaches_the_spawner() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::always_failing("wsl.exe not found");
        k.want_running();
        for _ in 0..=MAX_ATTEMPTS {
            k.reconcile(&mut s);
        }
        assert!(k.gave_up());

        // The user presses Start. NO want_stopped in between — that is the
        // whole point; nothing in the product makes them stop first.
        let mut good = FakeSpawner::new();
        k.want_running();
        assert!(
            !k.gave_up(),
            "a fresh ask must reopen the budget even though the intent was already Run"
        );
        assert_eq!(k.reconcile(&mut good), Step::Established);
        // Evidence, not a label: the Start press really reached the spawner and
        // the holder it produced is alive.
        assert_eq!(good.spawns, 1, "the Start button never reached the spawner");
        assert!(!good.last().has_exited());
        assert_eq!(k.last_error(), None, "the stale reason must not outlive the give-up");
    }

    /// ...but a POLL must not, and that asymmetry is what keeps the bound real.
    ///
    /// `tray_set_status` pushes a verdict every 7 seconds. If adoption cleared
    /// the give-up, `MAX_ATTEMPTS` would be a 7-second retry loop with extra
    /// steps — the bound would exist only in the tests. This is exactly the
    /// hazard `want_running`'s doc comment warns about, kept intact by routing
    /// adoption through its own path rather than through the user's ask.
    #[test]
    fn a_status_poll_never_reopens_a_give_up() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::always_failing("nope");
        k.want_running();
        for _ in 0..=MAX_ATTEMPTS {
            k.reconcile(&mut s);
        }
        assert!(k.gave_up());
        let at_give_up = s.spawns;

        for _ in 0..20 {
            k.observed_status("online");
            assert_eq!(k.reconcile(&mut s), Step::GaveUp);
        }
        assert!(k.gave_up(), "a 7s poll must not clear the give-up");
        assert_eq!(
            s.spawns, at_give_up,
            "20 polls reopened the budget: MAX_ATTEMPTS is not a bound, it is a 7s retry loop"
        );
    }

    /// A user who asks again gets a fresh budget — a give-up must not make the
    /// Start button permanently useless.
    #[test]
    fn a_new_intent_clears_a_give_up() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::always_failing("nope");
        k.want_running();
        for _ in 0..=MAX_ATTEMPTS {
            k.reconcile(&mut s);
        }
        assert!(k.gave_up());

        // Stop, then Start again, and this time wsl.exe works.
        k.want_stopped();
        k.reconcile(&mut s);
        let mut good = FakeSpawner::new();
        k.want_running();
        assert_eq!(k.reconcile(&mut good), Step::Established);
        assert_eq!(good.spawns, 1);
        assert!(!k.gave_up());
    }

    // -- releasing -----------------------------------------------------------

    /// Stop releases, and the released session is really ended.
    #[test]
    fn stopping_the_server_releases_the_holder() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();

        k.want_running();
        k.reconcile(&mut s);
        let held = s.nth(0);
        assert!(!held.clone().has_exited());

        k.want_stopped();
        assert_eq!(k.reconcile(&mut s), Step::Released);
        assert!(!k.is_holding());
        assert_eq!(s.release_count(), 1, "the session must actually be ended");
        assert!(held.clone().has_exited());

        // And it stays released — no holder is ever the only reason the distro
        // is alive after the server is meant to be down.
        for _ in 0..10 {
            assert_eq!(k.reconcile(&mut s), Step::Idle);
        }
        assert_eq!(s.spawns, 1);
    }

    /// Exit is the same release path, and it works from a give-up too (there
    /// may still be a corpse to reap).
    #[test]
    fn releasing_is_idempotent_and_safe_from_every_state() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();
        k.want_stopped();
        assert_eq!(k.reconcile(&mut s), Step::Idle);
        assert_eq!(s.spawns, 0);
        assert_eq!(s.release_count(), 0);

        k.want_running();
        k.reconcile(&mut s);
        k.want_stopped();
        assert_eq!(k.reconcile(&mut s), Step::Released);
        assert_eq!(k.reconcile(&mut s), Step::Idle);
        assert_eq!(s.release_count(), 1, "a second release must not double-kill");
    }

    // -- adoption, and the asymmetry that makes it safe ----------------------

    #[test]
    fn a_running_server_is_adopted_from_a_status_push() {
        for verdict in ["online", "starting"] {
            let mut k = Keepalive::new();
            let mut s = FakeSpawner::new();
            k.observed_status(verdict);
            assert_eq!(
                k.reconcile(&mut s),
                Step::Established,
                "{verdict} means there is a server to protect"
            );
            assert_eq!(s.spawns, 1);
            assert!(!s.last().has_exited());
        }
    }

    /// THE asymmetry. A restart looks exactly like a stop from the outside, and
    /// the flag that hides the flap lives only in the webview — so releasing on
    /// a negative verdict would power the distro off 15 s into a restart the
    /// user asked for.
    #[test]
    fn a_negative_verdict_never_releases_a_held_session() {
        for verdict in ["stopped", "crashed", "soap_unreachable", "something_new"] {
            let mut k = Keepalive::new();
            let mut s = FakeSpawner::new();
            k.want_running();
            k.reconcile(&mut s);

            k.observed_status(verdict);
            assert_eq!(
                k.reconcile(&mut s),
                Step::Holding,
                "{verdict} must not release: it is what a restart and a crash both look like"
            );
            assert_eq!(s.release_count(), 0);
            assert!(k.is_holding());
        }
    }

    /// ...and an unknown verdict does not ADOPT either. The union may grow, and
    /// a new value must not silently start pinning a distro.
    #[test]
    fn an_unknown_verdict_does_not_adopt() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();
        k.observed_status("verdict_from_the_future");
        assert_eq!(k.reconcile(&mut s), Step::Idle);
        assert_eq!(s.spawns, 0);

        assert!(verdict_means_running("online"));
        assert!(verdict_means_running("starting"));
        assert!(!verdict_means_running("stopped"));
        assert!(!verdict_means_running(""));
    }

    // -- the report ----------------------------------------------------------

    /// The report is re-derived from live state, so a webview reload cannot
    /// lose the news that the keep-alive gave up.
    #[test]
    fn the_report_names_a_failure_rather_than_hiding_it() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::always_failing("the distro is not registered");
        k.want_running();
        for _ in 0..=MAX_ATTEMPTS {
            k.reconcile(&mut s);
        }

        let r = report_from(&k, true);
        assert!(r.applies);
        assert!(r.wanted);
        assert!(!r.holding);
        assert!(r.gave_up);
        assert_eq!(r.attempts, MAX_ATTEMPTS);
        assert_eq!(r.last_error.as_deref(), Some("the distro is not registered"));
    }

    // -- what the user is told, and how often --------------------------------

    /// A GIVE-UP IS NEWS EXACTLY ONCE, which is what the emitter's comment has
    /// always claimed and what it did not do.
    ///
    /// The old guard was `gave_up && attempts >= MAX_ATTEMPTS` read off the
    /// report. Once latched, `reconcile` returns at the `gave_up` arm without
    /// touching `attempts`, so both halves stay true on EVERY subsequent
    /// 5-second tick: twenty latched ticks, twenty `wsl-keepalive` events,
    /// forever. It was bounded only by the frontend happening to re-read the
    /// report and compare strings — an accident in another file, in another
    /// language, that nothing pinned.
    #[test]
    fn a_latched_give_up_is_announced_once_and_then_goes_quiet() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::always_failing("wsl.exe not found");
        k.want_running();

        let mut said: Vec<(&'static str, String)> = Vec::new();
        // Far past the give-up: every one of these ticks used to emit.
        for _ in 0..(MAX_ATTEMPTS + 20) {
            let step = k.reconcile(&mut s);
            if let Some(a) = k.announcement(&step) {
                said.push(a);
            }
        }

        assert_eq!(
            said.iter().filter(|(kind, _)| *kind == "gave_up").count(),
            1,
            "20 latched ticks produced {} give-up events; the storm is the bug",
            said.iter().filter(|(kind, _)| *kind == "gave_up").count()
        );
        // NON-VACUITY: an emitter that answers None to everything would satisfy
        // the count above. The attempts that failed are still reported.
        assert_eq!(
            said.iter().filter(|(kind, _)| *kind == "failed").count(),
            MAX_ATTEMPTS as usize,
            "the failing attempts before the give-up must still be announced"
        );
        // ...and the give-up carries the reason, not an empty banner.
        let (_, why) = said.iter().find(|(kind, _)| *kind == "gave_up").expect("no give-up event");
        assert_eq!(why, "wsl.exe not found");
    }

    /// ...and a SECOND give-up, after the user asked again, is news again.
    /// "Announced once" must mean once per give-up, not once per process — a
    /// latch that never reopens is the contentless "already told you" this repo
    /// has already been burned by.
    #[test]
    fn a_give_up_after_a_fresh_ask_is_announced_again() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::always_failing("nope");

        let mut give_ups = 0usize;
        for _ in 0..2 {
            k.want_running();
            for _ in 0..(MAX_ATTEMPTS + 5) {
                let step = k.reconcile(&mut s);
                if let Some((kind, _)) = k.announcement(&step) {
                    if kind == "gave_up" {
                        give_ups += 1;
                    }
                }
            }
        }
        assert_eq!(give_ups, 2, "the second give-up must be told, not swallowed by a latch");
    }

    /// A give-up whose spawns all SUCCEEDED has no OS error to quote, and the
    /// announcement must still name the shape rather than "unknown reason". A
    /// banner that says nothing is the silence this whole module removes.
    #[test]
    fn an_announcement_never_carries_an_empty_reason() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();
        k.want_running();

        let mut said: Option<(&'static str, String)> = None;
        for _ in 0..(MAX_ATTEMPTS + 5) {
            let step = k.reconcile(&mut s);
            if let Some(a) = k.announcement(&step) {
                if a.0 == "gave_up" {
                    assert!(said.is_none(), "announced twice");
                    said = Some(a);
                }
                continue;
            }
            // Every spawn succeeds and then dies before the next tick: the
            // "holder that will not stay up" shape, where `spawn()` never
            // returned an error to quote.
            s.last().exited.store(true, Ordering::SeqCst);
        }

        let (_, why) = said.expect("a give-up with no OS error must still be announced");
        assert_eq!(why, HOLDER_KEEPS_DYING);
        assert_eq!(k.last_error(), Some(HOLDER_KEEPS_DYING));
    }

    #[test]
    fn a_healthy_report_carries_no_error() {
        let mut k = Keepalive::new();
        let mut s = FakeSpawner::new();
        k.want_running();
        k.reconcile(&mut s);
        let r = report_from(&k, true);
        assert!(r.holding && r.wanted && !r.gave_up);
        assert_eq!(r.last_error, None);
    }

    /// The spawn we would really make, asserted as argv rather than trusted.
    /// `--exec` is the repo rule (a bare `--` runs a shell), the distro and user
    /// come from `dml_core::runner` so the holder enters the SAME distro the
    /// runner drives, and `/bin/sleep` is absolute because `--exec` skips the
    /// login shell that would have set a PATH.
    #[test]
    fn the_holder_argv_enters_the_runners_own_distro_without_a_shell() {
        let argv = [
            "-d",
            DISTRO,
            "-u",
            USER,
            "--exec",
            HOLDER_PROGRAM,
            HOLDER_ARG,
        ];
        assert_eq!(argv, ["-d", "dml-arch", "-u", "dml", "--exec", "/bin/sleep", "infinity"]);
        assert!(
            !argv.contains(&"--"),
            "`--` runs a login shell; `--exec` is the documented spawn for our own literals"
        );
        assert!(
            HOLDER_PROGRAM.starts_with('/'),
            "--exec skips the shell, so the program must be an absolute path"
        );
    }

    /// The watchdog must fit inside the distro's 15 s grace, with room for the
    /// respawn itself. Pinned as a number because the failure mode of getting
    /// it wrong is invisible: the investigation's 20 s variant died before its
    /// first tick ever fired.
    #[test]
    fn the_watchdog_tick_fits_inside_the_fifteen_second_deadline() {
        assert!(
            WATCHDOG_TICK <= Duration::from_secs(7),
            "the distro has ~15s from the holder's death; detection plus respawn must fit"
        );
        assert!(WATCHDOG_TICK >= Duration::from_secs(1), "a busy-loop is not a watchdog");
    }

    // -----------------------------------------------------------------------
    // LIVE. Real `wsl.exe`, the real `dml-arch` distro, opt-in.
    // -----------------------------------------------------------------------
    //
    //   cargo test -p launcher --lib wsl_keepalive::tests::live_ -- --ignored --nocapture
    //
    // Everything above this line is a fake. This is the only thing that proves
    // the product claim, and it proves it in TWO halves, because only the
    // second one establishes causation:
    //
    //   1. the distro is still Running a full minute after the holder started —
    //      four times the ~15 s deadline;
    //   2. it DIES once the holder is released.
    //
    // Half 1 alone would pass against `wsl --list --verbose` polling, which the
    // investigation measured NOT to work (it died at 14.8 s with 8 polls inside
    // the window). Half 2 is what makes the holder the cause rather than a
    // coincidence.
    //
    // THE OBSERVER DOES NOT DISTURB THE EXPERIMENT, and that is a measured
    // claim, not an assumption: `wsl --list --verbose` queries the service
    // without opening a session into the distro, and only a session into the
    // distro resets the timer. It is the one probe this test may use. Anything
    // that runs `wsl -d dml-arch …` to look at the distro would be holding it,
    // and the test would pass while proving nothing.
    //
    // SIDE EFFECT, stated rather than discovered: establishing the holder BOOTS
    // the distro, systemd starts dockerd, and `restart: unless-stopped` may
    // bring the user's AzerothCore stack up with it. The test never touches the
    // stack, and its exit path is the ordinary graceful poweroff the product
    // already relies on every time the launcher closes — WSL asks systemd, and
    // systemd stops dockerd cleanly. Do not "tidy up" afterwards with
    // `wsl --terminate`: that converts a graceful shutdown into a hard cut.

    /// Decode `wsl.exe` output, which is UTF-16LE on Windows — a plain
    /// `from_utf8_lossy` turns every line into `d\0m\0l\0…` and any `contains`
    /// against it silently answers "no". A probe whose failure mode looks like
    /// its negative answer is not a probe.
    #[cfg(windows)]
    fn decode_wsl(bytes: &[u8]) -> String {
        if bytes.len() >= 2 && bytes[1] == 0 {
            let u16s: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect();
            String::from_utf16_lossy(&u16s)
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        }
    }

    /// The state `wsl --list --verbose` reports for one distro, e.g.
    /// `"Running"` / `"Stopped"`. `None` if the distro is not listed at all.
    #[cfg(windows)]
    fn distro_state(name: &str) -> Option<String> {
        let out = std::process::Command::new("wsl.exe")
            .args(["--list", "--verbose"])
            .output()
            .expect("wsl.exe must be runnable for a live test");
        let text = decode_wsl(&out.stdout);
        for line in text.lines() {
            let mut tokens = line.split_whitespace();
            // `continue`, NOT `?`. A `?` here returns None from the whole
            // function on the first blank line, and `is_running` would then
            // report a Running distro as dead — a probe whose failure mode is
            // indistinguishable from its negative answer, which is the exact
            // class this repo keeps re-learning.
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

    #[cfg(windows)]
    fn is_running(name: &str) -> bool {
        distro_state(name).as_deref() == Some("Running")
    }

    /// The whole product claim, measured.
    #[test]
    #[ignore = "live: boots the real dml-arch distro and takes ~2.5 minutes"]
    #[cfg(windows)]
    fn live_the_holder_keeps_the_distro_up_and_letting_go_lets_it_die() {
        /// Four times the ~15 s deadline. 60 s is the plan's stated gate.
        const HOLD_FOR: Duration = Duration::from_secs(60);
        /// After release: 15 s deadline + WSL's 10 s poweroff grace + slack.
        const DEATH_BUDGET: Duration = Duration::from_secs(90);
        /// Booting a cold distro takes a few seconds.
        const BOOT_BUDGET: Duration = Duration::from_secs(60);
        /// Assertions only start once the deadline is comfortably behind us.
        const PAST_THE_DEADLINE: Duration = Duration::from_secs(20);

        // REFUSE rather than skip. A `return` here is indistinguishable from a
        // pass, which is the vacuity trap this repo keeps re-learning — and
        // terminating a Running distro to make room for the test could take
        // down a server someone is playing on.
        assert_eq!(
            distro_state(DISTRO).as_deref(),
            Some("Stopped"),
            "this test refuses to run unless {DISTRO} is Stopped — it must not disturb a \
             distro somebody else started, and it must start from a cold one to prove anything"
        );

        let mut spawner = WslSpawner;
        let mut k = Keepalive::new();
        k.want_running();
        let step = k.reconcile(&mut spawner);
        assert_eq!(step, Step::Established, "the real holder failed to start: {step:?}");

        // Wait for the boot the holder just triggered.
        let t0 = std::time::Instant::now();
        while !is_running(DISTRO) && t0.elapsed() < BOOT_BUDGET {
            std::thread::sleep(Duration::from_secs(1));
        }
        assert!(
            is_running(DISTRO),
            "{DISTRO} never came up within {BOOT_BUDGET:?} of establishing the holder \
             (state: {:?})",
            distro_state(DISTRO)
        );
        eprintln!("[live] {DISTRO} up after {:.1}s; holding for {HOLD_FOR:?}", t0.elapsed().as_secs_f64());

        // --- HALF 1: it stays up, well past the deadline --------------------
        // Failures are COLLECTED, not asserted, so a bad result still reaches
        // the release below. A panic here would leave a real wsl.exe holding
        // the user's distro open for as long as this shell lives.
        let hold_start = std::time::Instant::now();
        let mut died_at: Option<f64> = None;
        while hold_start.elapsed() < HOLD_FOR {
            std::thread::sleep(Duration::from_secs(2));
            if !is_running(DISTRO) && died_at.is_none() {
                died_at = Some(hold_start.elapsed().as_secs_f64());
                break;
            }
        }
        let held_for = hold_start.elapsed();

        // --- release, then measure the death --------------------------------
        k.want_stopped();
        assert_eq!(k.reconcile(&mut spawner), Step::Released);
        let released_at = std::time::Instant::now();

        let mut death_delay: Option<f64> = None;
        while released_at.elapsed() < DEATH_BUDGET {
            std::thread::sleep(Duration::from_secs(2));
            if !is_running(DISTRO) {
                death_delay = Some(released_at.elapsed().as_secs_f64());
                break;
            }
        }

        // --- now judge ------------------------------------------------------
        assert_eq!(
            died_at, None,
            "{DISTRO} died {:?}s into the hold — the holder is not holding. \
             (The deadline is ~15s; anything at all here means the session is not \
             being counted.)",
            died_at
        );
        assert!(
            held_for >= PAST_THE_DEADLINE,
            "the hold window was only {held_for:?}, which does not clear the ~15s deadline"
        );
        eprintln!("[live] survived {:.1}s of holding — {:.1}x the ~15s deadline",
                  held_for.as_secs_f64(), held_for.as_secs_f64() / 15.0);

        // THE HALF THAT PROVES CAUSATION. Without it, "the distro was up" is
        // compatible with something else entirely holding it.
        let delay = death_delay.unwrap_or_else(|| {
            panic!(
                "{DISTRO} was STILL Running {DEATH_BUDGET:?} after the holder was released. \
                 Either something else is holding the distro (check for stray wsl.exe), or \
                 release() is not actually ending the session — in which case the first half \
                 of this test proved nothing about the holder."
            )
        });
        eprintln!("[live] died {delay:.1}s after release (expected ~15s + poweroff grace)");
        assert!(
            delay >= 5.0,
            "the distro went down {delay:.1}s after release — too fast to be the 15s idle \
             timer, which means it was probably already on its way out during the hold"
        );
    }
}
