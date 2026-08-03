//! Route A native install engine — the staged, RESUMABLE state machine that
//! turns an empty title dir into a running AzerothCore + playerbots stack on
//! Docker Desktop, with no Arch distro anywhere.
//!
//! # What this is a reimplementation of
//!
//! `guides/wow-wotlk/install-wow-wotlk.sh` has a portable core — two git
//! clones, a compose override, `docker compose up -d --build`, a readiness poll
//! — wrapped in a large amount of Linux-host setup (pacman, `systemctl`,
//! `usermod -aG docker`, `/etc/sudoers.d`, a Steam Gaming-Mode launcher). Docker
//! Desktop makes the wrapper unnecessary; this module is the core, natively.
//! The clone URLs, branches, the `--depth 1` on the module and the
//! images-already-built skip are all taken from that proven script rather than
//! invented here.
//!
//! # RESUMABILITY, honestly described
//!
//! A source build takes HOURS. A user closed one halfway on 2026-07-29, which is
//! why this exists. But "resume" here does NOT mean the build process was
//! suspended and revived — nothing suspends a killed process, and the launcher's
//! cancel is a `taskkill /F /T`. Resume rests on exactly two things, both
//! external to this engine:
//!
//! 1. **[`STATE_FILE`] in the title dir** — which STAGES have completed. It is
//!    written only ever AFTER a stage really finished, so an interrupted stage
//!    is simply not recorded and runs again.
//! 2. **Docker's own BuildKit layer cache** — which makes the re-run of an
//!    interrupted `build` stage cheap. Compilation work that finished before the
//!    kill is recovered by the cache, not by us.
//!
//! Everything else (clones, generated compose files) is cheap enough to redo,
//! and each stage additionally checks ON-DISK evidence, so a state file that is
//! missing or stale is a slow path rather than a wrong one.
//!
//! # The two guards, and why they refuse rather than warn
//!
//! Both are recorded blockers of the native-first plan, and both are reachable
//! for the first time here because this is the first surface that CREATES a
//! stack.
//!
//! * **A compose file this engine did not generate.** The default games dir is
//!   `%USERPROFILE%\dml-native`, whose `wow-server-playerbots` IS the working
//!   migrated server. `composegen::write_all` rewrites `docker-compose.yml` in
//!   whatever dir it is handed, and the generated file carries a per-install
//!   project name — so one careless run would re-identify that server and orphan
//!   its volumes and images. See [`foreign_compose_file`].
//! * **Another stack owning the `ac-*` container names.** Container names are
//!   unique per Docker ENGINE, not per compose project, and the plan RATIFIED
//!   keeping them global (per-install names would mean routing every consumer —
//!   `docker exec ac-database mysqldump`, `docker restart -t 300 ac-worldserver`
//!   — through compose-project resolution; that is the honest end state and is a
//!   recorded follow-up, not this task). So the engine enforces ONE STACK AT A
//!   TIME and says exactly that when it refuses. See [`conflicting_owner`].
//!
//! Tri-state discipline applies to the second one: a `docker ps` that cannot
//! answer is evidence of NOTHING. It warns and proceeds — refusing on it would
//! block installs on a slow engine, and asserting "no conflict" would race one.
//!
//! # SOAP
//!
//! [`composegen`] defaults SOAP on and this engine takes that default
//! ([`InstallOpts::new`] → `ComposeOpts::default()`). A fresh install whose
//! launcher cannot talk SOAP has dead GM tools, dead My Party and a dead console
//! with no cause shown (SHIP-LIST 4.0e, found live), so
//! `a_fresh_install_generates_the_three_compose_files_with_soap_switched_on`
//! asserts the KEY IN THE WRITTEN FILE, not the option that produced it.
//!
//! # Injectable IO
//!
//! Every git and docker call goes through [`InstallIo`]. Production is
//! [`ProcIo`] (a thin adapter over `dml_core::proc`, itself covered by
//! `proc_io_really_spawns_a_process_and_streams_its_output` so the seam cannot
//! rot into a stub); the tests drive a recording fake and read the ORDER and
//! ARGUMENTS back off the calls that were actually made. No test clones or
//! builds anything.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dml_core::error::CmdError;
use dml_core::events::{
    done_event, error_event, line_event, pct_event, section_end, section_start,
    section_start_limited,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{composegen, lifecycle, logsnap, preflight, status};

/// Per-install bookkeeping, in the title dir so a copied/moved install carries
/// its own progress and a deleted title dir forgets it.
pub const STATE_FILE: &str = ".dml-install.json";
pub const STATE_VERSION: u32 = 1;

/// The first line of `data/native-compose.yml.tmpl`. This is how the guard tells
/// a file this engine wrote from one it must not touch; the tripwire test
/// `the_generated_base_carries_the_marker_the_guard_recognises` fails if a
/// template edit ever drops it.
pub const GENERATED_MARKER: &str = "GENERATED by dml";

/// Where the build tee lands, relative to the title dir.
pub const BUILD_LOG_DIR: &str = "logs";

/// Where `mod-playerbots` is cloned, relative to the title dir. The override's
/// `./modules:/azerothcore/modules` mount is what makes it reach the container,
/// and `modules.rs` decides "installed" by `<key>/.git` being a directory.
pub const MODULE_SUBDIR: &str = "modules/mod-playerbots";

/// How long the readiness wait may run. The WSL installer's own cap
/// (`install-wow-wotlk.sh`: `TIMEOUT=1800`) — a first boot after a fresh build
/// imports the whole world database, which really does take this long.
pub const READY_TIMEOUT: Duration = Duration::from_secs(1800);
/// Readiness poll cadence — the same 10s the WSL installer uses.
pub const READY_POLL: Duration = Duration::from_secs(10);

pub const CODE_BAD_ID: &str = "BAD_ARG";
pub const CODE_NO_GAMES_DIR: &str = "INSTALL_NO_GAMES_DIR";
pub const CODE_COMPOSE_EXISTS: &str = "INSTALL_COMPOSE_EXISTS";
pub const CODE_STACK_CONFLICT: &str = "INSTALL_STACK_CONFLICT";
pub const CODE_DIR_NOT_EMPTY: &str = "INSTALL_DIR_NOT_EMPTY";
pub const CODE_WRONG_REMOTE: &str = "INSTALL_WRONG_REMOTE";
pub const CODE_CLONE_FAILED: &str = "INSTALL_CLONE_FAILED";
pub const CODE_BUILD_FAILED: &str = "INSTALL_BUILD_FAILED";
pub const CODE_UP_FAILED: &str = "INSTALL_UP_FAILED";
pub const CODE_READY_TIMEOUT: &str = "INSTALL_READY_TIMEOUT";
/// The pinned commit could not be fetched, checked out, or -- the case that
/// matters -- did not read back from HEAD afterwards.
pub const CODE_PIN_FAILED: &str = "INSTALL_PIN_FAILED";

/// The container names the generated stack claims. GLOBAL to the docker engine
/// (see the module docs), which is exactly why [`conflicting_owner`] exists.
/// Same set, same spelling as `data/native-compose.yml.tmpl`.
pub const OWNED_CONTAINERS: [&str; 5] =
    ["ac-database", "ac-db-import", "ac-client-data-init", "ac-authserver", "ac-worldserver"];

/// The stages, in the order [`install_native_stream_with`] runs them.
///
/// This array is READ BY PRODUCTION — the driver loop iterates it — so pinning
/// it in a test is not the "ordering invariant on a list nobody reads" trap that
/// cost this project a silently-deleted log snapshot. The load-bearing ordering
/// assertions still read the order back off the calls the run actually made.
pub const STAGE_ORDER: [Stage; 8] = [
    Stage::Preflight,
    Stage::Guard,
    Stage::CloneCore,
    Stage::CloneModule,
    Stage::GenerateCompose,
    Stage::Build,
    Stage::Up,
    Stage::Ready,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Preflight,
    Guard,
    CloneCore,
    CloneModule,
    GenerateCompose,
    Build,
    Up,
    Ready,
}

impl Stage {
    /// The `section_start`/`section_end` name AND the token stored in the state
    /// file. A string, not an ordinal, so reordering the enum can never
    /// re-interpret an existing install's recorded progress.
    pub fn name(self) -> &'static str {
        match self {
            Stage::Preflight => "preflight",
            Stage::Guard => "guard",
            Stage::CloneCore => "clone-core",
            Stage::CloneModule => "clone-module",
            Stage::GenerateCompose => "generate-compose",
            Stage::Build => "build",
            Stage::Up => "up",
            Stage::Ready => "ready",
        }
    }

    /// Whether completing this stage is worth recording.
    ///
    /// `preflight` and `guard` are deliberately excluded: they are GUARDS, and a
    /// guard that a resume skips is not a guard. They re-run on every attempt,
    /// which costs a `docker info` and a `docker ps`.
    pub fn records_completion(self) -> bool {
        !matches!(self, Stage::Preflight | Stage::Guard)
    }
}

/// A git repository to clone: where from, which branch, and how deep.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoRef {
    pub url: String,
    pub branch: String,
    /// `Some(n)` adds `--depth n`. See [`default_core_repo`] for why the core
    /// checkout deliberately has none.
    pub depth: Option<u32>,
    /// The exact commit to build. `None` tracks the branch tip.
    ///
    /// A pin is only worth having if it is VERIFIED: after checkout the engine
    /// re-reads `HEAD` and refuses when it does not match, because a pin that
    /// silently fails to apply is worse than no pin -- it claims a
    /// reproducibility this build does not have.
    pub commit: Option<String>,
}

/// The commits the first successful native install actually built, captured from
/// that checkout (2026-07-31). Not chosen, not latest -- observed.
pub const CORE_PINNED_COMMIT: &str = "190184a04539937a617bf033e39378196c0c63f5";
pub const MODULE_PINNED_COMMIT: &str = "ba46fcdecde3d0c6c2f244fcb3ea862430b6ae5b";

/// `git rev-parse --verify <sha>^{commit}` -- is the pinned commit ALREADY in
/// this checkout?
///
/// Asked before any fetch, and that ordering is load-bearing. The core is
/// deliberately cloned WITHOUT `--depth` because AzerothCore's `genrev.cmake`
/// reads the repository's history to stamp a build revision, and
/// `git fetch --depth 1` against a complete repository would make it SHALLOW --
/// quietly breaking the very thing the full clone exists for.
pub fn have_commit_argv(dir: &Path, sha: &str) -> Vec<String> {
    vec![
        "-C".to_string(),
        dir.to_string_lossy().into_owned(),
        "rev-parse".to_string(),
        "--verify".to_string(),
        "--quiet".to_string(),
        format!("{sha}^{{commit}}"),
    ]
}

/// `git fetch --depth 1 origin <sha>` -- only reached when the commit is NOT
/// already present, i.e. a shallow checkout whose tip has moved past the pin.
pub fn fetch_commit_argv(dir: &Path, sha: &str) -> Vec<String> {
    vec![
        "-C".to_string(),
        dir.to_string_lossy().into_owned(),
        "fetch".to_string(),
        "--depth".to_string(),
        "1".to_string(),
        "origin".to_string(),
        sha.to_string(),
    ]
}

/// `git checkout --detach <sha>`. Detached on purpose: the branch name is how
/// the clone got here, but the COMMIT is what gets built.
pub fn checkout_commit_argv(dir: &Path, sha: &str) -> Vec<String> {
    vec![
        "-C".to_string(),
        dir.to_string_lossy().into_owned(),
        "checkout".to_string(),
        "--detach".to_string(),
        sha.to_string(),
    ]
}

/// `git rev-parse HEAD` -- the verification read.
pub fn head_sha_argv(dir: &Path) -> Vec<String> {
    vec![
        "-C".to_string(),
        dir.to_string_lossy().into_owned(),
        "rev-parse".to_string(),
        "HEAD".to_string(),
    ]
}

/// The AzerothCore fork the playerbots build needs, exactly as
/// `install-wow-wotlk.sh` clones it.
///
/// NOT SHALLOW, and not pinned to a SHA — both deliberate, and both are
/// "pin nothing you cannot justify":
///
/// * The proven installer does not use `--depth` here (only on the module), and
///   AzerothCore's `genrev.cmake` reads the repository's own history to stamp a
///   revision into the build. A shallow clone would save ~1.3 GB (measured on
///   this box) at the cost of a build-time behaviour nobody in this repo has
///   tested. Space is what the preflight's games-dir floor is for.
/// * PINNED as of 2026-08-01, and the precondition this comment used to state
///   is now met. It said a pin "needs the upstream tree actually fetched" and
///   that otherwise it "would be a number invented here". The first end-to-end
///   native install ran on 2026-07-31 -- 8/8 stages, 21m18s, a healthy
///   worldserver -- so this SHA is not invented: it is the tree that produced a
///   working server on real hardware. Bumping it means running that build again,
///   which is the point.
pub fn default_core_repo() -> RepoRef {
    RepoRef {
        url: "https://github.com/mod-playerbots/azerothcore-wotlk.git".to_string(),
        branch: "Playerbot".to_string(),
        depth: None,
        commit: Some(CORE_PINNED_COMMIT.to_string()),
    }
}

/// The playerbots module. Shallow, because the proven installer clones it
/// `--depth 1` and nothing reads its history: the build consumes the working
/// tree and `modules.rs` only asks whether `.git` is a directory.
pub fn default_module_repo() -> RepoRef {
    RepoRef {
        url: "https://github.com/mod-playerbots/mod-playerbots.git".to_string(),
        branch: "master".to_string(),
        depth: Some(1),
        commit: Some(MODULE_PINNED_COMMIT.to_string()),
    }
}

#[derive(Debug, Clone)]
pub struct InstallOpts {
    /// Title id — also the DIRECTORY NAME under the games dir, which is why it
    /// is validated ([`valid_title_id`]) before being joined onto anything.
    pub id: String,
    pub games_dir: PathBuf,
    /// The explicit "I insist" override for the preflight's HARDWARE floors.
    /// It never clears the docker/git refusals; see [`preflight::decide`].
    pub allow_underspec: bool,
    /// Everything the three compose files are rendered from. Default has SOAP
    /// ON — see the module docs.
    pub compose: composegen::ComposeOpts,
    pub core: RepoRef,
    pub module: RepoRef,
    pub ready_timeout: Duration,
    pub ready_poll: Duration,
}

impl InstallOpts {
    pub fn new(id: impl Into<String>, games_dir: impl Into<PathBuf>) -> Self {
        InstallOpts {
            id: id.into(),
            games_dir: games_dir.into(),
            allow_underspec: false,
            compose: composegen::ComposeOpts::default(),
            core: default_core_repo(),
            module: default_module_repo(),
            ready_timeout: READY_TIMEOUT,
            ready_poll: READY_POLL,
        }
    }

    pub fn title_dir(&self) -> PathBuf {
        self.games_dir.join(&self.id)
    }
}

/// Which external program a [`Call`] is for. Two, because those are the only two
/// this engine ever runs — and naming them (rather than carrying a free-form
/// path) is what lets [`ProcIo`] resolve each one its own way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Program {
    Git,
    Docker,
}

impl Program {
    pub fn label(self) -> &'static str {
        match self {
            Program::Git => "git",
            Program::Docker => "docker",
        }
    }
}

/// One external command the engine wants run. `cwd: None` inherits the process's
/// — used for every call whose arguments are already absolute, so a probe can
/// never fail merely because some directory does not exist yet. The compose
/// calls DO carry a cwd: it is what selects the project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    pub program: Program,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    /// Wall-clock bound, or `None` for "let it run".
    ///
    /// `None` is correct for the clone and the build — a first-time AzerothCore
    /// build legitimately runs for HOURS and killing it on a timer would be the
    /// bug. It is wrong for every PROBE, and that was the state of this engine
    /// until review caught it (2026-07-29): a dockerd wedged during startup
    /// ACCEPTS the socket connection and then never answers, so one hung
    /// `docker ps` blocked the guard forever and one hung `docker logs` blocked
    /// the readiness loop past its own deadline — the deadline is only consulted
    /// after a call RETURNS. The launcher's cancel is a `taskkill /F /T`, so the
    /// user's only escape was killing the process.
    ///
    /// This is the same discipline `logsnap` (`DML_LOG_SNAPSHOT_TIMEOUT`) and
    /// `preflight` (`DOCKER_INFO_TIMEOUT`) already apply for the same reason.
    pub timeout: Option<Duration>,
}

/// Wall-clock bound for the engine's docker/git PROBES — the calls whose answer
/// is the point and whose output is small. Generous enough that a busy engine
/// still answers, short enough that a wedged one cannot hold a stage open.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(30);

/// The tri-state of one command. `CouldNotTell` is a spawn failure — evidence of
/// NOTHING about the world, and deliberately not an exit code, so no caller can
/// mistake "we never asked" for "it said no".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    Exited(i32),
    CouldNotTell(String),
}

impl RunOutcome {
    pub fn is_ok(&self) -> bool {
        matches!(self, RunOutcome::Exited(0))
    }
    /// A one-line description for a failure message.
    pub fn detail(&self) -> String {
        match self {
            RunOutcome::Exited(c) => format!("exit {c}"),
            RunOutcome::CouldNotTell(m) if m.is_empty() => "it could not be started".to_string(),
            RunOutcome::CouldNotTell(m) => format!("it could not be started: {m}"),
        }
    }
}

/// The whole IO surface of this engine. Two methods, because those are the only
/// two impure things it does: ask the machine whether a build can succeed, and
/// run git/docker.
pub trait InstallIo {
    fn preflight(&self, games_dir: &Path) -> preflight::PreflightFacts;
    /// Run `call`, handing every output line (stdout and stderr, interleaved as
    /// they arrive) to `on_line`. Unbounded on purpose: a build legitimately
    /// runs for hours.
    fn run(&self, call: &Call, on_line: &mut dyn FnMut(&str)) -> RunOutcome;

    /// Bring the Docker engine up if it is down, WITHOUT showing the dashboard.
    ///
    /// Returns `true` when a start was actually attempted, meaning the caller
    /// must re-gather its facts rather than trust the ones it holds. `false`
    /// means nothing was tried: the engine was already up, or there is no
    /// Docker Desktop to start.
    ///
    /// Default is "did not try", so a fake that does not care need not
    /// implement it; [`ProcIo`] supplies the real behaviour.
    fn ensure_engine(&self, _on_line: &mut dyn FnMut(String, String)) -> bool {
        false
    }
}

/// The production [`InstallIo`]: real subprocesses.
pub struct ProcIo {
    pub docker: OsString,
    pub git: OsString,
}

/// Where to find `git`. `DML_GIT` wins; otherwise the first ABSOLUTE candidate
/// that exists (Git for Windows can be installed with "do not modify PATH", and
/// the preflight already accepts such an install), falling back to a bare `git`
/// resolved off PATH — which is the only candidate on Linux.
fn git_program() -> OsString {
    if let Some(p) = std::env::var_os("DML_GIT").filter(|v| !v.is_empty()) {
        return p;
    }
    for cand in preflight::git_candidates() {
        if cand.is_absolute() && cand.is_file() {
            return cand.into_os_string();
        }
    }
    OsString::from("git")
}

impl ProcIo {
    pub fn from_env() -> Self {
        ProcIo { docker: dml_core::engine::docker_program(), git: git_program() }
    }
}

impl InstallIo for ProcIo {
    fn preflight(&self, games_dir: &Path) -> preflight::PreflightFacts {
        preflight::gather(&self.docker, games_dir)
    }

    /// The real engine start, reusing the SAME path Home's Start button takes
    /// ([`crate::native::ensure_engine_up_stream`]): `docker desktop start -d`,
    /// which asks for the ENGINE and not the GUI, so no dashboard window pops
    /// up over whatever the user was doing. It polls until the engine answers
    /// and gives up on a bounded timeout.
    fn ensure_engine(&self, on_line: &mut dyn FnMut(String, String)) -> bool {
        use dml_core::engine;
        if engine::engine_running(&self.docker) {
            return false; // nothing to start, nothing to re-probe for
        }
        if engine::docker_desktop_program().is_none() {
            // Nothing to start it WITH. The preflight refusal that follows
            // already says the right thing; a second message here would only
            // be noise in front of it.
            return false;
        }
        // `ensure_engine_up_stream` takes an `Fn`, so the lines are collected
        // here and replayed after it returns rather than forwarded live. The
        // whole call is seconds and its output is a handful of lines, so
        // nothing is lost by not streaming it -- and the alternative would mean
        // widening a shared helper's bound for one caller.
        let collected = std::cell::RefCell::new(Vec::<(String, String)>::new());
        crate::native::ensure_engine_up_stream(|v| {
            // Only the human-readable lines. The helper's own section events
            // belong to its caller's framing, and this engine already has the
            // user inside a `preflight` section.
            if v.get("event").and_then(|e| e.as_str()) == Some("line") {
                let level = v.get("level").and_then(|l| l.as_str()).unwrap_or("info");
                let text = v.get("text").and_then(|t| t.as_str()).unwrap_or("");
                collected.borrow_mut().push((level.to_string(), text.to_string()));
            }
        })
        .ok();
        for (level, text) in collected.into_inner() {
            on_line(level, text);
        }
        true
    }

    fn run(&self, call: &Call, on_line: &mut dyn FnMut(&str)) -> RunOutcome {
        let program = match call.program {
            Program::Git => &self.git,
            Program::Docker => &self.docker,
        };
        // Bounded path for the probes. A miss is a CouldNotTell, never a
        // fabricated exit code -- the caller's tri-state handling depends on
        // being able to tell "it said no" from "we never got an answer".
        if let Some(limit) = call.timeout {
            let mut cmd = std::process::Command::new(program);
            cmd.args(&call.args);
            if let Some(dir) = call.cwd.as_deref() {
                cmd.current_dir(dir);
            }
            dml_core::proc::windows_no_window(&mut cmd);
            // DRAINING, not `output_bounded` (review finding, 2026-08-02):
            // the plain variant polls `try_wait` without reading the pipes,
            // which its own doc scopes to callers whose output is small. The
            // ready loop's `docker logs --since <StartedAt>` is the whole
            // boot log — a first boot with playerbots logging in exceeds the
            // pipe buffer easily, the child blocks mid-write, `try_wait`
            // never answers, and every poll reads as CouldNotTell until the
            // loop times out on a server that was READY. The draining runner
            // has the identical Option contract.
            return match dml_core::proc::output_bounded_draining(cmd, limit) {
                Some(out) => {
                    for l in String::from_utf8_lossy(&out.stdout).lines() {
                        on_line(l);
                    }
                    for l in String::from_utf8_lossy(&out.stderr).lines() {
                        on_line(l);
                    }
                    RunOutcome::Exited(out.status.code().unwrap_or(-1))
                }
                None => RunOutcome::CouldNotTell(format!(
                    "{} did not answer within {}s",
                    Path::new(program).display(),
                    limit.as_secs()
                )),
            };
        }
        let args: Vec<&str> = call.args.iter().map(String::as_str).collect();
        // Derived from the TOOL, not carried on the Call: git is the one that
        // reports progress by redrawing a line with `\r`, and docker's build
        // wall is plain `\n` output whose blank lines separate vertices. Making
        // it a Call field would put the same constant answer at every
        // construction site and invite one of them to get it wrong.
        let split = match call.program {
            Program::Git => dml_core::proc::LineSplit::NewlineOrReturn,
            Program::Docker => dml_core::proc::LineSplit::Newline,
        };
        match dml_core::proc::run_streamed_lines(program, &args, call.cwd.as_deref(), split, |l| {
            on_line(l)
        }) {
            Some(st) => RunOutcome::Exited(st.code().unwrap_or(-1)),
            // The ONLY `None` this helper returns is a spawn failure, so it is a
            // could-not-tell and never a fabricated exit code.
            None => RunOutcome::CouldNotTell(format!(
                "{} could not be started",
                Path::new(program).display()
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// What a previous attempt got through. Written ONLY after a stage really
/// completed — a state file that claims a stage the run never finished is the
/// one failure mode that turns "resume" into "silently skip".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallState {
    pub version: u32,
    pub id: String,
    /// [`composegen::install_id`] of the title dir this file belongs to. The
    /// identity binding: a state file COPIED into another directory describes
    /// that directory's contents not at all, so [`load_state`] refuses it.
    pub install_id: String,
    /// Completed stage names, in completion order.
    pub completed: Vec<String>,
    /// Why the last attempt stopped. Cleared by a successful run.
    pub last_error: Option<String>,
    pub updated_unix: u64,
}

impl InstallState {
    pub fn new(id: &str, install_id: &str) -> Self {
        InstallState {
            version: STATE_VERSION,
            id: id.to_string(),
            install_id: install_id.to_string(),
            completed: Vec::new(),
            last_error: None,
            updated_unix: 0,
        }
    }

    pub fn is_done(&self, stage: Stage) -> bool {
        self.is_done_named(stage.name())
    }

    pub fn mark(&mut self, stage: Stage) {
        self.mark_named(stage.name());
    }

    /// The same two questions by stage NAME.
    ///
    /// The state file already stores names rather than ordinals — so that
    /// reordering an enum cannot re-interpret recorded progress — which makes
    /// this struct usable by any staged engine. [`crate::migrate`] is the
    /// second one, and sharing the type means the identity binding, the
    /// version check and the "recorded only after it really finished" rule are
    /// written once instead of being re-derived per engine.
    pub fn is_done_named(&self, stage: &str) -> bool {
        self.completed.iter().any(|s| s == stage)
    }

    pub fn mark_named(&mut self, stage: &str) {
        if !self.is_done_named(stage) {
            self.completed.push(stage.to_string());
        }
    }
}

pub fn state_path(title_dir: &Path) -> PathBuf {
    title_dir.join(STATE_FILE)
}

/// The first stage a resume has to do again — i.e. the first recordable stage
/// this state does not claim. `None` when the state claims all of them.
///
/// Production reads this (the engine narrates "continuing from <stage>"), which
/// is what keeps every recorded stage meaningful rather than inert bookkeeping.
pub fn next_stage(state: &InstallState) -> Option<Stage> {
    STAGE_ORDER
        .into_iter()
        .find(|s| s.records_completion() && !state.is_done(*s))
}

/// Read the state file, or `None` for absent / unreadable / wrong-version /
/// WRONG-DIRECTORY. Every one of those means "no trustworthy progress", and the
/// stages then fall back to their on-disk evidence.
pub fn load_state(title_dir: &Path) -> Option<InstallState> {
    let text = std::fs::read_to_string(state_path(title_dir)).ok()?;
    let state: InstallState = serde_json::from_str(&text).ok()?;
    if state.version != STATE_VERSION {
        return None;
    }
    if state.install_id != composegen::install_id(title_dir) {
        return None;
    }
    Some(state)
}

pub fn save_state(title_dir: &Path, state: &InstallState) -> Result<(), CmdError> {
    std::fs::create_dir_all(title_dir).map_err(|e| CmdError {
        code: "WRITE_FAILED".to_string(),
        message: format!("Could not create {}: {e}", title_dir.display()),
        hint: String::new(),
    })?;
    let mut state = state.clone();
    state.updated_unix = now_unix();
    let text = serde_json::to_string_pretty(&state).unwrap_or_default();
    std::fs::write(state_path(title_dir), text).map_err(|e| CmdError {
        code: "WRITE_FAILED".to_string(),
        message: format!("Could not write {}: {e}", state_path(title_dir).display()),
        hint: String::new(),
    })
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Where an install may write, or a refusal naming what to set.
///
/// This exists because [`dml_core::compose::games_dir_from_env`] falls back to
/// `PathBuf::from(".")`, which is a fine default for the READING commands that
/// have always used it — an unset games dir just makes them miss — and a bad one
/// here. `install-native` is the first consumer that CREATES a directory and
/// clones gigabytes into it, so the same fallback means "install into whatever
/// directory the shell happened to be in": the repo root, the user's home, or
/// `C:\Windows\System32`. Worse, [`composegen::install_id`] hashes the ABSOLUTE
/// path, so the same command run from two places yields two compose projects and
/// two sets of volumes for what the user thinks is one server.
///
/// All four reviewers flagged it (2026-07-29). The rule lives in the library
/// rather than in the CLI arm so that the launcher — which will call this engine
/// once Task 6 lands — cannot reintroduce the same default.
pub fn games_dir_for_install() -> Result<PathBuf, CmdError> {
    match std::env::var_os("DML_GAMES_DIR").filter(|v| !v.is_empty()) {
        Some(v) => Ok(PathBuf::from(v)),
        None => Err(CmdError {
            code: CODE_NO_GAMES_DIR.to_string(),
            message: "No games directory is configured, so there is nowhere to install to."
                .to_string(),
            hint: "Set DML_GAMES_DIR to the folder your servers live in (the launcher's Settings \
                   page calls this the games directory), then run the install again. It is not \
                   assumed, because an install writes several gigabytes and must never land in \
                   whatever directory you happened to be in."
                .to_string(),
        }),
    }
}

/// A plain title/directory name. This value is joined onto the games dir, so
/// separators, drive letters and `..` are refused rather than normalised — the
/// same `[A-Za-z0-9._-]+` rule the launcher applies before any spawn.
pub fn valid_title_id(id: &str) -> bool {
    !id.is_empty()
        && id != "."
        && id != ".."
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

// ---------------------------------------------------------------------------
// Pure argv
// ---------------------------------------------------------------------------

/// `git clone --config core.autocrlf=input --branch <b> [--depth N] <url> <dest>`
///
/// `--config` (not `-c`) because it writes the setting into the NEW repository's
/// config before the checkout happens — which is the whole point: a Windows-side
/// checkout with `autocrlf=true` hands CRLF shell scripts to a Linux build
/// container. `-c` would apply to the clone process and be forgotten afterwards,
/// so every later `git pull` in that tree would reintroduce the problem.
pub fn clone_argv(repo: &RepoRef, dest: &Path) -> Vec<String> {
    let mut argv = vec![
        "clone".to_string(),
        // Git suppresses its counter entirely when stdout is not a terminal,
        // and ours never is. Without this flag the download is silent for the
        // however-many minutes AzerothCore's full history takes, and there is
        // nothing for `CloneProgress` to read.
        "--progress".to_string(),
        "--config".to_string(),
        "core.autocrlf=input".to_string(),
        "--branch".to_string(),
        repo.branch.clone(),
    ];
    if let Some(d) = repo.depth {
        argv.push("--depth".to_string());
        argv.push(d.to_string());
    }
    argv.push(repo.url.clone());
    argv.push(dest.to_string_lossy().into_owned());
    argv
}

/// `git -C <dir> remote get-url origin` — one call that answers both "is this a
/// git repository" (it fails outside one) and "is it the RIGHT one".
pub fn checkout_probe_argv(dir: &Path) -> Vec<String> {
    vec![
        "-C".to_string(),
        dir.to_string_lossy().into_owned(),
        "remote".to_string(),
        "get-url".to_string(),
        "origin".to_string(),
    ]
}

/// The build: all three files, explicitly.
///
/// Naming files at all turns compose's auto-loading OFF, so the base and the
/// override have to be listed too — and the build overlay is only ever reachable
/// this way, which is what stops a later `up` from starting a multi-hour
/// rebuild.
pub fn build_argv() -> Vec<String> {
    vec![
        "compose".to_string(),
        "-f".to_string(),
        composegen::BASE_FILE.to_string(),
        "-f".to_string(),
        composegen::OVERRIDE_FILE.to_string(),
        "-f".to_string(),
        composegen::BUILD_FILE.to_string(),
        "build".to_string(),
    ]
}

/// The start: NO `-f`, so compose auto-loads base + override and CANNOT see the
/// build overlay. Identical to what `games start` runs afterwards.
pub fn up_argv() -> Vec<String> {
    vec!["compose".to_string(), "up".to_string(), "-d".to_string()]
}

/// `docker compose images -q` — the portable "already built?" probe, in the
/// spirit of the WSL installer's own check (`install-wow-wotlk.sh:426-435`,
/// `| grep -qi worldserver`) but asking a question prose cannot accidentally
/// answer.
///
/// `-q` is load-bearing: it prints bare image IDs, so the probe reduces to "was
/// the output empty?". Without it the check was a substring scan for
/// "worldserver" over merged stdout+stderr, which any compose warning naming
/// `ac-worldserver` could satisfy — and a false positive there LATCHES into the
/// state file and disables the build permanently. See [`Engine::do_build`].
pub fn images_argv() -> Vec<String> {
    vec!["compose".to_string(), "images".to_string(), "-q".to_string()]
}

/// Every container on this ENGINE with the compose project that owns it AND
/// the directory that project was composed from. Engine-wide on purpose: the
/// question is precisely "does something outside this project already hold
/// the `ac-*` names".
///
/// TAB-separated because the working dir routinely contains spaces
/// (`C:\Users\First Last\...`), and the working dir is carried at all because
/// the project NAME is not ground truth (live incident, 2026-08-02): the
/// user's migrated server runs under the project `dml-wow-native` — a name
/// from the migration era that `composegen::project_name_for` can never
/// derive — so a comparison against the derived name refused the user's OWN
/// server as a foreign stack. The working-dir label says which directory the
/// stack really came from, immune to every project-name override compose
/// honours.
pub fn stack_owner_argv() -> Vec<String> {
    vec![
        "ps".to_string(),
        "-a".to_string(),
        "--format".to_string(),
        "{{.Names}}{{\"\\t\"}}{{.Label \"com.docker.compose.project\"}}{{\"\\t\"}}{{.Label \"com.docker.compose.project.working_dir\"}}".to_string(),
    ]
}

// ---------------------------------------------------------------------------
// Pure guards
// ---------------------------------------------------------------------------

/// The four names `dml_core::compose::resolve_compose_dir` recognises. Kept in
/// step with it deliberately: a file it would treat as "this title is installed"
/// is exactly the file this guard must not overwrite.
const COMPOSE_CANDIDATES: [&str; 4] =
    ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"];

/// The first compose file in `title_dir` that this engine did not write, if any.
///
/// Identification is the [`GENERATED_MARKER`] header, not a guess about content.
/// An unreadable file counts as FOREIGN: a compose file we cannot read is
/// certainly not one we can prove we wrote.
pub fn foreign_compose_file(title_dir: &Path) -> Option<PathBuf> {
    for name in COMPOSE_CANDIDATES {
        let p = title_dir.join(name);
        if !p.is_file() {
            continue;
        }
        match std::fs::read_to_string(&p) {
            Ok(text) if text.contains(GENERATED_MARKER) => continue,
            _ => return Some(p),
        }
    }
    None
}

/// One container's ownership facts, per [`stack_owner_argv`]'s format.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StackOwner {
    pub project: String,
    /// `com.docker.compose.project.working_dir` — empty for hand-run
    /// containers and for rows produced by the old two-field format.
    pub working_dir: String,
}

/// Parse [`stack_owner_argv`] output into `name -> owner facts`.
///
/// A container with NO project label yields an EMPTY project rather than a
/// missing entry — it still owns the name, and dropping it is how a hand-run
/// `docker run --name ac-database` would sail past the guard.
///
/// Splits on TAB (the format's separator, because working dirs hold spaces);
/// a line with no tab falls back to the old whitespace split so a stale
/// caller's output still parses instead of reading as one giant name.
pub fn parse_stack_owners(out: &str) -> BTreeMap<String, StackOwner> {
    let mut map = BTreeMap::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let (name, project, working_dir) = if line.contains('\t') {
            let mut it = line.split('\t');
            (
                it.next().unwrap_or("").trim(),
                it.next().unwrap_or("").trim(),
                it.next().unwrap_or("").trim(),
            )
        } else {
            match line.split_once(char::is_whitespace) {
                Some((n, p)) => (n.trim(), p.trim(), ""),
                None => (line, "", ""),
            }
        };
        if name.is_empty() {
            continue;
        }
        map.insert(
            name.to_string(),
            StackOwner { project: project.to_string(), working_dir: working_dir.to_string() },
        );
    }
    map
}

/// One spelling for a directory path, so labels written by different shells
/// compare equal when they name the same place.
///
/// The same directory arrives in at least four spellings depending on who ran
/// compose: `C:\Users\x` (PowerShell), `C:/Users/x`, `/c/Users/x` (Git Bash)
/// and `/mnt/c/Users/x` (WSL). All four map to `c:/users/x`. Drive-rooted
/// results are lowercased (the Windows filesystem is case-insensitive);
/// genuinely POSIX paths keep their case, because on Linux `/srv/A` and
/// `/srv/a` really are different directories.
pub fn canon_path(p: &str) -> String {
    let mut s = p.replace('\\', "/");
    if let Some(rest) = s.strip_prefix("//?/") {
        s = rest.to_string();
    }
    let b = s.as_bytes();
    if s.len() >= 6
        && s.starts_with("/mnt/")
        && b[5].is_ascii_alphabetic()
        && (s.len() == 6 || b[6] == b'/')
    {
        s = format!("{}:{}", b[5] as char, &s[6..]);
    } else if s.len() >= 2
        && b[0] == b'/'
        && b[1].is_ascii_alphabetic()
        && (s.len() == 2 || b[2] == b'/')
    {
        s = format!("{}:{}", b[1] as char, &s[2..]);
    }
    while s.len() > 1 && s.ends_with('/') {
        s.pop();
    }
    if s.len() >= 2 && s.as_bytes()[1] == b':' {
        s = s.to_lowercase();
    }
    s
}

/// The first of [`OWNED_CONTAINERS`] held by a genuinely FOREIGN stack, as
/// `(container, owning project)`. `None` = the names are free or already ours.
///
/// "Ours" is true on EITHER signal: the project name matches what we derive,
/// OR the working-dir label names the directory we are about to compose from.
/// The second signal is the load-bearing one — see [`stack_owner_argv`] for
/// the live incident where the derived name alone refused the user's own
/// server (project `dml-wow-native`, a migration-era name).
pub fn conflicting_owner(
    owners: &BTreeMap<String, StackOwner>,
    our_project: &str,
    our_dir: &Path,
) -> Option<(String, String)> {
    let ours = canon_path(&our_dir.display().to_string());
    OWNED_CONTAINERS.into_iter().find_map(|name| {
        owners
            .get(name)
            .filter(|o| o.project != our_project)
            .filter(|o| o.working_dir.is_empty() || canon_path(&o.working_dir) != ours)
            .map(|o| (name.to_string(), o.project.clone()))
    })
}

/// The refusal copy. It has to explain a fact the user has no way to guess: the
/// names are global to Docker, so this is one-stack-at-a-time by construction.
pub fn stack_conflict_message(container: &str, owner: &str) -> String {
    let owner = if owner.is_empty() {
        "something that is not managed by Docker Compose".to_string()
    } else {
        format!("the stack \"{owner}\"")
    };
    format!(
        "The container name {container} is already taken by {owner}. Those names are global to Docker, not per server, so only one DML server can exist on this PC at a time."
    )
}

/// The readiness poll's single `docker inspect` format: container start time and
/// restart count in one call, `|`-separated.
///
/// Both are needed every poll — `StartedAt` scopes the log read (see
/// [`Engine::do_ready`]) and `RestartCount` feeds the boot-loop watch — so asking
/// once is one fewer subprocess per poll on a 30-minute wait.
pub const READY_INSPECT_FORMAT: &str = "{{.State.StartedAt}}|{{.RestartCount}}";

/// Split [`READY_INSPECT_FORMAT`]'s output into `(started_at, restart_count)`.
///
/// Each half degrades INDEPENDENTLY, and neither is ever guessed: an unparseable
/// restart count stays `None` (the boot-loop watch treats a missed reading as
/// evidence of nothing rather than as zero), and an absent start time stays
/// `None` so the caller falls back to a tail read instead of passing garbage to
/// `docker logs --since`. Go templates render a missing field as `<no value>`,
/// which is why that is filtered explicitly.
pub fn parse_started_and_restarts(raw: &str) -> (Option<String>, Option<u64>) {
    let line = raw.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
    let (started_raw, count_raw) = match line.split_once('|') {
        Some((a, b)) => (a.trim(), b.trim()),
        // No separator: an older/odd docker, or an error line. Treat the whole
        // thing as a possible count and give up on the timestamp.
        None => ("", line),
    };
    let started = Some(started_raw)
        .filter(|s| !s.is_empty() && *s != "<no value>")
        // A zero Go time means "never started"; it is not a usable --since value.
        .filter(|s| !s.starts_with("0001-01-01"))
        .map(str::to_string);
    (started, status::parse_restart_count(count_raw))
}

/// `build-<UTC>.log` — timestamp first so a plain name sort is chronological,
/// the same rule the worldserver log snapshots follow.
pub fn build_log_name_at(unix_secs: u64) -> String {
    format!("build-{}.log", crate::backup::format_utc_compact(unix_secs))
}

// ---------------------------------------------------------------------------
// Build progress
// ---------------------------------------------------------------------------

/// One ninja step, as BuildKit's plain progress passes it through.
///
/// AzerothCore configures with ninja (`apps/docker/Dockerfile` installs
/// `ninja-build`), and ninja prints a step counter with a KNOWN denominator —
/// which is the only reason this feature reports a real percentage instead of a
/// wall-clock guess.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildStep {
    /// The BuildKit vertex (`#26`) the line belongs to.
    pub vertex: u32,
    pub done: u64,
    pub total: u64,
}

/// `#26 782.2 [1803/1808] Building CXX object …` → that fraction.
///
/// Two shapes in the same stream look almost identical and only one of them is
/// progress:
///
/// ```text
/// #26 782.2 [1803/1808] Building CXX object …          <- ninja: real progress
/// #7 [ac-client-data-init skeleton 2/4] RUN mkdir -pv  <- BuildKit vertex header
/// ```
///
/// The second one's `2/4` is a DOCKERFILE STAGE step — matching it would jump
/// the bar to 50% during a 0.1s `mkdir`. What separates them is the
/// elapsed-seconds field: BuildKit prefixes a vertex's OUTPUT with `#N <secs>`
/// and its own status lines (`DONE`, `CACHED`, the header) with nothing. So the
/// second token must parse as a number, and the third must be a bracket holding
/// nothing but `<digits>/<digits>`.
pub fn parse_build_step(line: &str) -> Option<BuildStep> {
    let mut tokens = line.strip_prefix('#')?.split_ascii_whitespace();
    let vertex: u32 = tokens.next()?.parse().ok()?;

    let elapsed = tokens.next()?;
    // `.` alone would satisfy an all()-style check, so a digit is required
    // rather than merely permitted.
    if !elapsed.bytes().any(|b| b.is_ascii_digit())
        || !elapsed.bytes().all(|b| b.is_ascii_digit() || b == b'.')
    {
        return None;
    }

    let fraction = tokens.next()?.strip_prefix('[')?.strip_suffix(']')?;
    let (done_raw, total_raw) = fraction.split_once('/')?;
    if !done_raw.bytes().all(|b| b.is_ascii_digit())
        || !total_raw.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some(BuildStep { vertex, done: done_raw.parse().ok()?, total: total_raw.parse().ok()? })
}

/// A stream of build lines → at most 101 percentages.
///
/// Two rules beyond the parsing, both from what a real build actually looks
/// like:
///
///  * **The largest total wins.** Four images build in PARALLEL, so fractions
///    from different vertices interleave. A three-step sidecar reporting `2/3`
///    must not shove the display to 66% while the 1808-step compile is at 4%.
///  * **The number never goes down.** A bar that walks backwards reads as a bug
///    even when every number behind it is honest.
///
/// Emitting only on CHANGE is what keeps a 1808-step build to ~101 events
/// rather than 1808.
#[derive(Debug, Default)]
pub struct BuildProgress {
    best_total: u64,
    /// The last value REPORTED — a floor, not a memory of the previous line.
    reported: Option<u8>,
}

impl BuildProgress {
    /// The percentage to report for this line, or `None` when the line is not
    /// progress, is progress from a lesser vertex, or would not move the
    /// number forward.
    pub fn observe(&mut self, line: &str) -> Option<u8> {
        let step = parse_build_step(line)?;
        // A zero total is a malformed line, not a finished build: reporting
        // anything for it would mean dividing by it.
        if step.total == 0 || step.total < self.best_total {
            return None;
        }
        self.best_total = step.total;
        let pct = ((step.done.min(step.total) * 100) / step.total) as u8;
        match self.reported {
            Some(prev) if pct <= prev => None,
            _ => {
                self.reported = Some(pct);
                Some(pct)
            }
        }
    }
}

/// The two `git clone` phases worth reporting.
///
/// Git runs FOUR — Enumerating, Counting, Compressing, Receiving, then
/// Resolving — and each counts 0-100% on its own. Reporting them raw gives four
/// sawtooths that every user reads as a broken bar. The first three are the
/// SERVER's work and are announced with a `remote: ` prefix (which is also what
/// keeps them from matching here, since these prefixes are anchored); the two
/// that describe the local machine's wait are these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClonePhase {
    /// The download itself. On AzerothCore's full history this is the long one.
    Receiving,
    /// Rebuilding the objects afterwards. Shorter, but far from instant on a
    /// repository this size.
    Resolving,
}

/// `Receiving objects:  45% (12345/27000), 12.34 MiB | 5.67 MiB/s` → the phase
/// and its own percentage.
pub fn parse_clone_phase(line: &str) -> Option<(ClonePhase, u8)> {
    let text = line.trim_start();
    let (phase, rest) = if let Some(r) = text.strip_prefix("Receiving objects:") {
        (ClonePhase::Receiving, r)
    } else if let Some(r) = text.strip_prefix("Resolving deltas:") {
        (ClonePhase::Resolving, r)
    } else {
        return None;
    };
    let rest = rest.trim_start();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    // The `%` must be the very next character. Without that check a future git
    // line beginning with these words but counting something else would be read
    // as a percentage.
    if digits.is_empty() || !rest[digits.len()..].starts_with('%') {
        return None;
    }
    Some((phase, digits.parse::<u32>().ok()?.min(100) as u8))
}

/// Is this line one of git's progress redraws?
///
/// Asked separately from the percentage because the two questions have
/// different answers: a redraw that did not move the number is still a redraw,
/// and showing it would bury the clone's real output under hundreds of
/// near-identical lines.
pub fn is_clone_progress_line(line: &str) -> bool {
    parse_clone_phase(line).is_some()
}

/// A stream of clone lines → one monotonic 0-100.
///
/// The two phases are WEIGHTED into a single climb — receiving 0-90, resolving
/// 90-100 — rather than shown as two runs of 0-100. The split is deliberately
/// uneven because the phases are: on AzerothCore's history the download
/// dominates, and giving resolving an equal half would park the number at 50%
/// for most of the wait.
#[derive(Debug, Default)]
pub struct CloneProgress {
    reported: Option<u8>,
}

impl CloneProgress {
    const RECEIVING_SHARE: u32 = 90;

    pub fn observe(&mut self, line: &str) -> Option<u8> {
        let (phase, p) = parse_clone_phase(line)?;
        let pct = match phase {
            ClonePhase::Receiving => u32::from(p) * Self::RECEIVING_SHARE / 100,
            ClonePhase::Resolving => {
                Self::RECEIVING_SHARE + u32::from(p) * (100 - Self::RECEIVING_SHARE) / 100
            }
        } as u8;
        match self.reported {
            Some(prev) if pct <= prev => None,
            _ => {
                self.reported = Some(pct);
                Some(pct)
            }
        }
    }
}

/// `Container ac-worldserver Started` → the container and the state.
pub fn parse_container_event(line: &str) -> Option<(String, String)> {
    let mut parts = line.trim().strip_prefix("Container ")?.split_whitespace();
    let name = parts.next()?.to_string();
    let state = parts.next()?.to_string();
    Some((name, state))
}

/// The states that mean a container is ACCOUNTED FOR.
///
/// `Creating`/`Created`/`Starting`/`Waiting` are all mid-flight and every
/// container passes through several of them, so counting those would run the
/// number past 100% and back. `Exited` belongs here because the one-shot
/// services (`ac-db-import`, `ac-client-data-init`) finish that way BY DESIGN —
/// treating their success as "not done yet" would leave the step stuck at 60%
/// through a working install.
pub const UP_DONE_STATES: [&str; 4] = ["Started", "Running", "Healthy", "Exited"];

/// Containers finished / containers expected.
///
/// Counts each container ONCE (compose narrates several states per container),
/// and only ever climbs.
#[derive(Debug)]
pub struct UpProgress {
    total: usize,
    seen: std::collections::BTreeSet<String>,
    reported: Option<u8>,
}

impl UpProgress {
    pub fn new(total: usize) -> Self {
        UpProgress { total, seen: std::collections::BTreeSet::new(), reported: None }
    }

    pub fn observe(&mut self, line: &str) -> Option<u8> {
        if self.total == 0 {
            return None;
        }
        let (name, state) = parse_container_event(line)?;
        if !UP_DONE_STATES.contains(&state.as_str()) || !self.seen.insert(name) {
            return None;
        }
        // Capped rather than trusted: a container this build did not declare
        // (an override adding one) must not produce 120%.
        let pct = (self.seen.len().min(self.total) * 100 / self.total) as u8;
        match self.reported {
            Some(prev) if pct <= prev => None,
            _ => {
                self.reported = Some(pct);
                Some(pct)
            }
        }
    }
}

/// Compare two clone URLs for "same repository": trailing `/` and `.git` are
/// cosmetic, and Git hosting is case-insensitive about the host.
fn same_repo(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        let s = s.trim().trim_end_matches('/');
        s.strip_suffix(".git").unwrap_or(s).to_lowercase()
    }
    norm(a) == norm(b)
}

/// Does `dir` hold anything other than our own [`STATE_FILE`]? `git clone`
/// refuses a non-empty destination, and this engine will not delete a user's
/// files to make room — so this is the difference between a clean refusal and
/// an opaque git error.
fn has_foreign_content(dir: &Path) -> bool {
    let Ok(rd) = std::fs::read_dir(dir) else { return false };
    rd.flatten().any(|e| e.file_name() != std::ffi::OsStr::new(STATE_FILE))
}

// ---------------------------------------------------------------------------
// The engine
// ---------------------------------------------------------------------------

/// A stage's refusal/failure: exactly the fields of the terminal `error` event.
struct Fail {
    code: &'static str,
    message: String,
    hint: String,
}

impl Fail {
    fn new(code: &'static str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Fail { code, message: message.into(), hint: hint.into() }
    }
}

struct Engine<'a> {
    io: &'a dyn InstallIo,
    opts: &'a InstallOpts,
    emit: &'a dyn Fn(Value),
    title_dir: PathBuf,
    project: String,
    state: InstallState,
    /// True when a trustworthy state file was found on entry — the difference
    /// between "fresh install" and "resume" in the copy, and the only thing that
    /// lets a foreign-looking (i.e. upstream's own) compose file in the title dir
    /// be accepted rather than refused.
    resumed: bool,
}

impl<'a> Engine<'a> {
    fn line(&self, level: &str, text: impl Into<String>) {
        (self.emit)(line_event(level, text));
    }

    /// Run a call and COLLECT its output — for probes, whose answer is the point
    /// and whose output is not interesting to the user.
    fn run_collect(&self, call: &Call) -> (RunOutcome, String) {
        let mut buf = String::new();
        let outcome = self.io.run(call, &mut |l| {
            buf.push_str(l);
            buf.push('\n');
        });
        (outcome, buf)
    }

    /// Run a call and STREAM its output to the terminal, optionally teeing every
    /// line into `tee`. Used for the clones, the build and the `up` — the three
    /// places a user is watching a progress wall.
    fn run_echo(&self, call: &Call, tee: Option<&Path>) -> RunOutcome {
        self.run_echo_with(call, tee, &mut |_| true)
    }

    /// [`Self::run_echo`] with a per-line hook that both observes each line and
    /// decides whether it reaches the TERMINAL (`true` = show it).
    ///
    /// The ordering is fixed and load-bearing: the tee is written FIRST, so the
    /// log file on disk stays the complete record no matter what the hook
    /// decides. Only the on-screen wall is ever filtered.
    ///
    /// That distinction exists because of the clone. `git clone --progress`
    /// redraws its counter hundreds of times a second; showing every redraw
    /// would bury the surrounding output, and dropping them from the LOG too
    /// would destroy exactly the evidence a stalled download needs.
    fn run_echo_with(
        &self,
        call: &Call,
        tee: Option<&Path>,
        on_line: &mut dyn FnMut(&str) -> bool,
    ) -> RunOutcome {
        use std::io::Write;
        let mut file = tee.and_then(|p| {
            p.parent().map(|d| std::fs::create_dir_all(d));
            std::fs::File::create(p).ok()
        });
        self.io.run(call, &mut |l| {
            if let Some(f) = file.as_mut() {
                let _ = writeln!(f, "{l}");
                let _ = f.flush();
            }
            if on_line(l) {
                self.line("info", l);
            }
        })
    }

    /// An UNBOUNDED docker call — only for the two that legitimately take hours
    /// (`build`) or minutes (`up`, which may pull images).
    fn docker(&self, args: Vec<String>, cwd: Option<PathBuf>) -> Call {
        Call { program: Program::Docker, args, cwd, timeout: None }
    }
    /// A BOUNDED docker probe. Every call whose answer is the point goes through
    /// here; see [`Call::timeout`] for why an unbounded probe is a hang.
    fn docker_probe(&self, args: Vec<String>, cwd: Option<PathBuf>) -> Call {
        Call { program: Program::Docker, args, cwd, timeout: Some(PROBE_TIMEOUT) }
    }
    fn git(&self, args: Vec<String>) -> Call {
        // Every git call this engine makes names its target absolutely (`clone
        // <url> <abs dest>`, `-C <abs dir>`), so it needs no working directory —
        // and inheriting avoids a spawn that fails only because the directory
        // does not exist yet.
        // Bounded: every git call the engine makes through this helper is a
        // PROBE (`remote get-url`). The clone does NOT come through here -- it
        // builds its own unbounded Call, because a 3.7 GB fetch is not a probe.
        Call { program: Program::Git, args, cwd: None, timeout: Some(PROBE_TIMEOUT) }
    }
    /// The clone: unbounded, and it must stay that way.
    fn git_clone(&self, args: Vec<String>) -> Call {
        Call { program: Program::Git, args, cwd: None, timeout: None }
    }

    /// Persist progress. Best-effort ON PURPOSE: losing the bookkeeping makes
    /// the next run slower, while failing the install over it would throw away
    /// work that really did complete.
    fn persist(&self) {
        if let Err(e) = save_state(&self.title_dir, &self.state) {
            self.line("warn", format!("could not record install progress: {}", e.message));
        }
    }

    /// Persist a FAILURE — but only into a title dir that already has real
    /// content. A clone that failed before creating anything must leave the
    /// destination untouched, or the state file itself becomes the non-empty
    /// directory that blocks the retry.
    fn persist_failure(&mut self, f: &Fail) {
        self.state.last_error = Some(format!("{}: {}", f.code, f.message));
        if !self.state.completed.is_empty() || state_path(&self.title_dir).exists() {
            self.persist();
        }
    }

    /// Is the title dir a checkout of the core repository this engine installs?
    ///
    /// This is the EVIDENCE that lets the foreign-compose guard accept
    /// upstream's own `docker-compose.yml`, which legitimately sits in the title
    /// dir between `clone-core` and `generate-compose`.
    ///
    /// It replaces an earlier "a state file loaded" (`resumed`) test that was
    /// wrong in BOTH directions — found by adversarial review, 2026-07-29:
    ///
    /// * **Too weak.** `.dml-install.json`'s only integrity check is that its
    ///   `install_id` equals [`composegen::install_id`] of the directory — a
    ///   plain hash of the path, derivable by anyone, and no evidence at all
    ///   that DML created that directory. Dropping such a file beside a REAL
    ///   server therefore disabled this guard, and because [`Self::do_clone`]
    ///   also used to trust `is_done` ahead of its own disk checks, both clones
    ///   then no-opped and `generate-compose` rewrote the real server's
    ///   `docker-compose.yml` under a freshly derived project name — orphaning
    ///   the volumes holding its characters.
    /// * **Too strong.** A `git clone` killed after upstream's compose file
    ///   landed records no state at all, so the retry was non-resumed and got
    ///   refused with "that folder already holds a server" — telling the user to
    ///   delete a multi-gigabyte checkout that was in fact ours.
    ///
    /// Asking git is the same question [`Self::do_clone`] already asks, and it
    /// needs no bookkeeping file to be true.
    fn core_checkout_is_ours(&self) -> bool {
        if !self.title_dir.join(".git").is_dir() {
            return false;
        }
        let (outcome, out) = self.run_collect(&self.git(checkout_probe_argv(&self.title_dir)));
        if !outcome.is_ok() {
            // Tri-state: git declining to answer is NOT evidence that this
            // checkout is ours. Falling through to the refusal is the safe
            // direction — it costs a retry, never a rewritten server.
            return false;
        }
        let url = out.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("");
        same_repo(url, &self.opts.core.url)
    }

    // -- stages ------------------------------------------------------------

    fn do_preflight(&mut self) -> Result<(), Fail> {
        let mut facts = self.io.preflight(&self.opts.games_dir);
        // START THE ENGINE RATHER THAN REFUSING OVER IT.
        //
        // A stopped Docker Desktop is the single most likely reason an install
        // cannot begin: it does not run at boot on a default install, so a user
        // resuming a build the next day meets this every time. Home's Start
        // button already brings the engine up hidden, so the installer telling
        // the user to go and do by hand what the app does for them elsewhere
        // was an inconsistency, not a safeguard.
        //
        // Only attempted when the facts say docker did not answer, and the
        // facts are RE-GATHERED afterwards -- deciding on the stale ones would
        // refuse over a state that no longer exists. If the engine still will
        // not come up, the refusal below is unchanged and still honest.
        if facts.docker.reachable != dml_core::setup::Tri::Yes {
            let mut lines: Vec<(String, String)> = Vec::new();
            let attempted = self.io.ensure_engine(&mut |level, text| lines.push((level, text)));
            for (level, text) in lines {
                self.line(&level, text);
            }
            if attempted {
                facts = self.io.preflight(&self.opts.games_dir);
            }
        }
        let report = preflight::decide(&facts, self.opts.allow_underspec);
        for f in &report.findings {
            let level = match f.severity {
                preflight::Severity::Ok => "info",
                preflight::Severity::Warn => "warn",
                preflight::Severity::Refuse => "error",
            };
            // The NUMBERS travel with the finding, including on the overridden
            // path -- a user who forced past a floor is exactly the one who will
            // need them when the build dies.
            self.line(level, f.message.clone());
        }
        if !report.is_refusal() {
            return Ok(());
        }
        let blocking: Vec<&preflight::Finding> = report
            .findings
            .iter()
            .filter(|f| f.severity == preflight::Severity::Refuse)
            .collect();
        let message = blocking
            .iter()
            .map(|f| f.message.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let hint = blocking.first().map(|f| f.hint.clone()).unwrap_or_default();
        Err(Fail {
            code: report.code.unwrap_or(preflight::CODE_UNDERSPEC),
            message,
            hint,
        })
    }

    fn do_guard(&mut self) -> Result<(), Fail> {
        // 1. Never generate over somebody else's compose file. What makes an
        //    unrecognised compose file acceptable is EVIDENCE THAT THE CHECKOUT
        //    IS OURS, not the presence of a bookkeeping file: between
        //    `clone-core` and `generate-compose` the title dir legitimately holds
        //    UPSTREAM's own docker-compose.yml. See `core_checkout_is_ours` for
        //    why the earlier `!self.resumed` test was both too weak and too
        //    strong.
        if !self.core_checkout_is_ours() {
            if let Some(p) = foreign_compose_file(&self.title_dir) {
                return Err(Fail::new(
                    CODE_COMPOSE_EXISTS,
                    format!(
                        "{} already exists and was not created by DML, so this install will not overwrite it.",
                        p.display()
                    ),
                    "That folder already holds a server. Install into a different folder (set the games directory in Settings), or move/delete that folder first if it is a failed install you no longer want.",
                ));
            }
        }

        // 2. One stack at a time -- the `ac-*` names are global to the engine.
        let (outcome, out) = self.run_collect(&self.docker_probe(stack_owner_argv(), None));
        if !outcome.is_ok() {
            // Tri-state: docker failing to answer is evidence of NOTHING. Say so
            // and continue; a real collision surfaces as a compose error we did
            // not fabricate.
            self.line(
                "warn",
                format!(
                    "could not check whether another server already owns the ac-* container names ({}) -- continuing.",
                    outcome.detail()
                ),
            );
            return Ok(());
        }
        let owners = parse_stack_owners(&out);
        if let Some((container, owner)) = conflicting_owner(&owners, &self.project, &self.title_dir)
        {
            return Err(Fail::new(
                CODE_STACK_CONFLICT,
                stack_conflict_message(&container, &owner),
                "Stop the other server first (Home > Stop, or `docker compose down` in its folder), then run this install again.",
            ));
        }
        self.line("info", "the ac-* container names are free for this install.");
        Ok(())
    }

    fn do_clone(&mut self, stage: Stage) -> Result<(), Fail> {
        let (repo, dest, what) = match stage {
            Stage::CloneCore => (&self.opts.core, self.title_dir.clone(), "AzerothCore (playerbots fork)"),
            _ => (
                &self.opts.module,
                self.title_dir.join("modules").join("mod-playerbots"),
                "the mod-playerbots module",
            ),
        };

        // NO `is_done` short-circuit here, deliberately. The state file used to
        // be allowed to skip this stage outright, which made it AUTHORITY rather
        // than a hint and contradicted this module's own promise that "each stage
        // additionally checks ON-DISK evidence, so a state file that is missing
        // or stale is a slow path rather than a wrong one". It also opened the
        // hole described on `core_checkout_is_ours`: a state file claiming the
        // clones were done skipped them both without ever reaching
        // `has_foreign_content`, so nothing stopped `generate-compose` from
        // rewriting a real server's compose file. Now the disk decides, and a
        // resume pays one `git remote get-url` per checkout to prove it.
        //
        // On-disk evidence: an existing checkout of the RIGHT repository is
        // adopted, one of a stranger is refused rather than clobbered.
        if dest.join(".git").is_dir() {
            let (outcome, out) = self.run_collect(&self.git(checkout_probe_argv(&dest)));
            if outcome.is_ok() {
                let url = out.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
                if same_repo(&url, &repo.url) {
                    self.line("info", format!("found an existing checkout of {what} -- keeping it."));
                    // Deliberately NOT re-pinned. We just promised to keep it,
                    // and moving someone's HEAD can discard local work. Report
                    // the mismatch instead: honest, and the user can act on it.
                    if let Some(want) = repo.commit.clone() {
                        let (o, head) = self.run_collect(&self.git(head_sha_argv(&dest)));
                        let head = head.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
                        if o.is_ok() && !head.is_empty() && head != want {
                            self.line(
                                "warn",
                                format!(
                                    "that checkout is at {} but this DML build is pinned to {} -- it was NOT moved. \
                                     Delete the folder and re-run to build the pinned tree.",
                                    &head[..head.len().min(12)],
                                    &want[..want.len().min(12)]
                                ),
                            );
                        }
                    }
                    return Ok(());
                }
                return Err(Fail::new(
                    CODE_WRONG_REMOTE,
                    format!(
                        "{} is a git checkout of {}, not {}.",
                        dest.display(),
                        if url.is_empty() { "an unknown repository" } else { url.as_str() },
                        repo.url
                    ),
                    "Move or delete that folder and run the install again, or install into a different games directory.",
                ));
            }
            // A `.git` that git itself will not answer for is not evidence of a
            // usable checkout; fall through to the emptiness check, which gives
            // the user a message they can act on.
        }

        if dest.is_dir() {
            if has_foreign_content(&dest) {
                return Err(Fail::new(
                    CODE_DIR_NOT_EMPTY,
                    format!(
                        "{} already exists and is not empty, so {what} cannot be cloned into it.",
                        dest.display()
                    ),
                    "Move or delete that folder and run the install again. DML will not delete it for you.",
                ));
            }
            // Only OUR bookkeeping file is in the way, and git refuses even that.
            // Removing it is safe: it is re-written the moment the clone succeeds.
            let _ = std::fs::remove_file(state_path(&dest));
        }
        if let Some(parent) = dest.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        self.line("info", format!("cloning {what} ({}, branch {})...", repo.url, repo.branch));
        let argv = clone_argv(repo, &dest);
        let mut progress = CloneProgress::default();
        let outcome = self.run_echo_with(&self.git_clone(argv), None, &mut |l| {
            match progress.observe(l) {
                Some(pct) => {
                    (self.emit)(pct_event(pct));
                    // The redraw that MOVED the number is worth one line; that
                    // caps a download at ~101 lines instead of thousands.
                    true
                }
                // A redraw that did not move the number is noise. Anything that
                // is not a redraw at all is real output and always shown.
                None => !is_clone_progress_line(l),
            }
        });
        if outcome.is_ok() {
            if let Some(sha) = repo.commit.clone() {
                self.apply_pin(&dest, &sha, what)?;
            }
            return Ok(());
        }
        Err(Fail::new(
            CODE_CLONE_FAILED,
            format!("Could not clone {what} from {} ({}).", repo.url, outcome.detail()),
            "Check your internet connection and that GitHub is reachable, then run the install again -- it continues where it stopped.",
        ))
    }

    /// Check the pinned commit out, then PROVE it took.
    ///
    /// The verification is the whole point. A pin that silently fails to apply
    /// claims a reproducibility the build does not have, which is worse than
    /// tracking the branch openly -- and this repo has been bitten repeatedly by
    /// exactly that shape (an override that was never read, a stub whose default
    /// was substituted for an empty value).
    fn apply_pin(&mut self, dest: &std::path::Path, sha: &str, what: &str) -> Result<(), Fail> {
        let short = &sha[..sha.len().min(12)];
        // Fetch ONLY when the commit is missing. `git fetch --depth 1` against a
        // complete repository makes it shallow, and the core is deliberately a
        // full clone because genrev.cmake reads its history.
        let (have, _) = self.run_collect(&self.git(have_commit_argv(dest, sha)));
        if !have.is_ok() {
            self.line("info", format!("fetching pinned commit {short} for {what}..."));
            let outcome = self.run_echo(&self.git_clone(fetch_commit_argv(dest, sha)), None);
            if !outcome.is_ok() {
                return Err(Fail::new(
                    CODE_PIN_FAILED,
                    format!("Could not fetch the pinned commit {short} for {what} ({}).", outcome.detail()),
                    "Check your connection. If the pin has been removed upstream, this DML build cannot reproduce its server.",
                ));
            }
        }

        let outcome = self.run_echo(&self.git_clone(checkout_commit_argv(dest, sha)), None);
        if !outcome.is_ok() {
            return Err(Fail::new(
                CODE_PIN_FAILED,
                format!("Could not check out the pinned commit {short} for {what} ({}).", outcome.detail()),
                "Delete the folder and run the install again.",
            ));
        }

        // Read it back. Trusting the checkout's exit code would be trusting the
        // thing under test.
        let (o, head) = self.run_collect(&self.git(head_sha_argv(dest)));
        let head = head.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_string();
        if !o.is_ok() || head != sha {
            return Err(Fail::new(
                CODE_PIN_FAILED,
                format!(
                    "{what} was pinned to {short} but HEAD reads {}.",
                    if head.is_empty() { "nothing".to_string() } else { head[..head.len().min(12)].to_string() }
                ),
                "This build cannot be reproduced. Delete the folder and run the install again.",
            ));
        }
        self.line("info", format!("{what} pinned at {short}."));
        Ok(())
    }

    fn do_generate(&mut self) -> Result<(), Fail> {
        // May we regenerate an override that is already there?
        //
        // Only while this install has never started its containers. `up` is the
        // moment the override becomes live configuration: from then on
        // `crate::config` writes the user's bot counts, rates and SOAP settings
        // into it and a regeneration would eat them. Before it, the file is
        // purely our own output and a stale copy is a liability — the bug that
        // broke the first live install was a missing key in a TEMPLATE, and a
        // fix that cannot reach an existing directory is not a fix.
        //
        // Deliberately NOT `ready`: an install that reached `up` and then timed
        // out waiting for the world server has a RUNNING stack the Settings page
        // can already write to, so resuming it must not discard those edits.
        let replace_override = !self.state.is_done(Stage::Up);
        let gen = composegen::write_all_with(&self.title_dir, &self.opts.compose, replace_override)
            .map_err(|e| Fail {
            // composegen already speaks this envelope's codes (BAD_ARG /
            // WRITE_FAILED / COMPOSE_TEMPLATE); re-coding them here would hide
            // which of them actually happened.
            code: Box::leak(e.code.into_boxed_str()),
            message: e.message,
            hint: e.hint,
        })?;
        self.line("info", format!("wrote {}", gen.base.display()));
        self.line("info", format!("wrote {}", gen.build.display()));
        if gen.override_replaced {
            // Say it plainly. A resume that quietly rewrites a settings file is
            // the kind of thing a user should be able to read back afterwards.
            self.line(
                "info",
                format!(
                    "refreshed {} from the current templates (this install has not started yet, so it held no settings of yours)",
                    gen.overrides.display()
                ),
            );
        } else if gen.override_written {
            self.line("info", format!("wrote {}", gen.overrides.display()));
        } else {
            self.line(
                "info",
                format!("kept your existing {} (it holds your settings)", gen.overrides.display()),
            );
        }
        if let Some(p) = &gen.dotenv {
            self.line("info", format!("updated {}", p.display()));
        }
        if self.opts.compose.soap {
            self.line(
                "info",
                format!(
                    "SOAP is enabled on 127.0.0.1:{} -- the launcher's GM tools, My Party and console all need it.",
                    self.opts.compose.ports.soap
                ),
            );
        } else {
            self.line(
                "warn",
                "SOAP is DISABLED for this install -- the launcher's GM tools, My Party and console will not work.",
            );
        }
        Ok(())
    }

    fn do_build(&mut self) -> Result<(), Fail> {
        // Again no `is_done` short-circuit, and here the reason is sharper than
        // for the clones: a FALSE "already built" answer LATCHES. The old probe
        // asked `docker compose images` and matched the substring "worldserver"
        // anywhere in merged stdout+stderr, so any warning naming
        // `ac-worldserver` on a run that still exited 0 satisfied it — and
        // because a skipped stage is recorded as completed, `is_done` then
        // skipped the build on every later run too. `up` would fail forever on a
        // missing image with no way out but deleting `.dml-install.json` by hand
        // (adversarial review, 2026-07-29).
        //
        // `compose images -q` prints image IDs and nothing else, so "is there an
        // image?" is answered by whether the output is EMPTY rather than by
        // fishing for a word in prose. The intent is the same portable skip the
        // proven WSL installer uses.
        let (outcome, out) =
            self.run_collect(&self.docker_probe(images_argv(), Some(self.title_dir.clone())));
        if outcome.is_ok() && out.lines().any(|l| !l.trim().is_empty()) {
            self.line("info", "a built server image is already here -- skipping the build.");
            return Ok(());
        }

        let log_dir = self.title_dir.join(BUILD_LOG_DIR);
        let log_path = log_dir.join(build_log_name_at(now_unix()));
        self.line(
            "info",
            format!(
                "building the server from source. This takes HOURS and depends on your PC; the full output is saved to {}.",
                log_path.display()
            ),
        );
        self.line(
            "info",
            "you can close the launcher -- reopening and running the install again continues from here, reusing Docker's build cache.",
        );
        // The one place a `pct` event comes from. Local to this call, so a
        // resumed build starts its percentage over rather than inheriting a
        // floor from an attempt whose step total no longer applies.
        let mut progress = BuildProgress::default();
        let outcome = self.run_echo_with(
            &self.docker(build_argv(), Some(self.title_dir.clone())),
            Some(&log_path),
            &mut |l| {
                if let Some(pct) = progress.observe(l) {
                    (self.emit)(pct_event(pct));
                }
                // The build wall is shown in FULL: BuildKit's plain output does
                // not redraw, so there is nothing here to suppress and every
                // line is one a failed build needs.
                true
            },
        );
        if outcome.is_ok() {
            return Ok(());
        }
        Err(Fail::new(
            CODE_BUILD_FAILED,
            format!("The build failed ({}).", outcome.detail()),
            format!(
                "The full build output is in {}. Running the install again continues from here rather than starting over.",
                log_path.display()
            ),
        ))
    }

    fn do_up(&mut self) -> Result<(), Fail> {
        self.line("info", "starting the server containers...");
        let mut progress = UpProgress::new(composegen::base_container_names().len());
        let outcome = self.run_echo_with(
            &self.docker(up_argv(), Some(self.title_dir.clone())),
            None,
            &mut |l| {
                if let Some(pct) = progress.observe(l) {
                    (self.emit)(pct_event(pct));
                }
                // Compose's up output is a few dozen lines and does not redraw;
                // all of it is shown.
                true
            },
        );
        if outcome.is_ok() {
            return Ok(());
        }
        Err(Fail::new(
            CODE_UP_FAILED,
            format!("The server containers failed to start ({}).", outcome.detail()),
            "Check the output above; a port already in use is the usual cause. Running the install again retries this step without rebuilding.",
        ))
    }

    /// Wait for the world to finish booting.
    ///
    /// This closes a gap the codebase had recorded against itself: the native
    /// path had NO readiness wait at all (`compose up -d` returns as soon as its
    /// dependency conditions are met), so the boot-loop watch had nothing to span
    /// — and on a FRESH install this boot is the longest one there will ever be,
    /// because it imports the whole world database.
    ///
    /// The container is resolved through THIS project's compose file every poll,
    /// never by the bare `ac-worldserver` name: that name answers for whichever
    /// stack owns it (the log-snapshot incident), and `up` recreates containers,
    /// so a cached id goes stale exactly when the evidence starts.
    fn do_ready(&mut self) -> Result<(), Fail> {
        self.line("info", "waiting for the world server to finish starting...");
        let started = Instant::now();
        let deadline = started + self.opts.ready_timeout;
        let mut watch = lifecycle::BootLoopWatch::new();
        let mut inspect_warned = false;

        loop {
            let (ps_outcome, ps_out) = self.run_collect(&self.docker_probe(
                logsnap::world_container_argv().into_iter().map(String::from).collect(),
                Some(self.title_dir.clone()),
            ));
            let container = if ps_outcome.is_ok() {
                logsnap::parse_container_id(ps_out.as_bytes())
            } else {
                None
            };

            if let Some(cid) = container {
                // ONE inspect for both facts we need per poll: when the container
                // started (which scopes the log read) and how many times it has
                // restarted (the boot-loop watch).
                let (rc_outcome, rc_out) = self.run_collect(&self.docker_probe(
                    vec![
                        "inspect".to_string(),
                        "-f".to_string(),
                        READY_INSPECT_FORMAT.to_string(),
                        cid.clone(),
                    ],
                    None,
                ));
                let (started_at, reading) = if rc_outcome.is_ok() {
                    parse_started_and_restarts(&rc_out)
                } else {
                    // A missed reading must fall straight out: collapsing it to
                    // zero either fabricates a loop or hides one.
                    //
                    // But it must also be SAID. This inspect failing sends the
                    // log read to a fixed `--tail` window, and the marker
                    // prints once -- a boot with playerbots logging in pushes
                    // it out of that window in seconds, so the wait then runs
                    // its full timeout on a server that is already up. That is
                    // not hypothetical: it happened for weeks because the
                    // format string asked for `.State.RestartCount`, which does
                    // not exist (2026-08-03). Once per run, so a wedged docker
                    // cannot spam the terminal.
                    if !inspect_warned {
                        inspect_warned = true;
                        self.line(
                            "warn",
                            format!(
                                "could not read the container's start time ({}) -- falling back to a log tail, which can miss the ready marker on a busy boot.",
                                rc_outcome.detail()
                            ),
                        );
                    }
                    (None, None)
                };

                // `--since <StartedAt>` — the WHOLE log since the container came
                // up — not a fixed tail. `World initialized in ...` is printed
                // ONCE, so a tail window can scroll past it between polls and
                // report INSTALL_READY_TIMEOUT after 30 minutes on a server that
                // actually started. A first boot with playerbots logging in emits
                // far more than 200 lines in a 10s poll interval, so this is
                // reachable, not theoretical (review finding, 2026-07-29). This is
                // the same approach `status::world_ready` already takes for the
                // live probe; the tail is only a fallback for when the container
                // will not tell us its start time.
                let log_args = match started_at.as_deref() {
                    Some(started) => vec![
                        "logs".to_string(),
                        "--since".to_string(),
                        started.to_string(),
                        cid.clone(),
                    ],
                    None => vec![
                        "logs".to_string(),
                        "--tail".to_string(),
                        lifecycle::BOOT_LOOP_CAUSE_TAIL_LINES.to_string(),
                        cid.clone(),
                    ],
                };
                let (log_outcome, logs) = self.run_collect(&self.docker_probe(log_args, None));
                if log_outcome.is_ok() && status::world_ready_from_logs(&logs) {
                    self.line("info", "the world server is ready.");
                    return Ok(());
                }
                if let Some(new_restarts) = watch.observe(reading) {
                    let mysql = status::mysql_connect_failures(&logs) >= lifecycle::BOOT_LOOP_MYSQL_HITS_MIN;
                    self.line("warn", lifecycle::boot_loop_note(new_restarts, mysql));
                }
            }

            let now = Instant::now();
            if now >= deadline {
                break;
            }
            std::thread::sleep(self.opts.ready_poll.min(deadline.saturating_duration_since(now)));
        }

        Err(Fail::new(
            CODE_READY_TIMEOUT,
            format!(
                "The world server did not report ready within {} minutes.",
                self.opts.ready_timeout.as_secs() / 60
            ),
            "The containers are still running -- open the Console to see where it stopped. A first boot after a fresh build imports the whole world database and is the slowest one you will see.",
        ))
    }

    fn run_stage(&mut self, stage: Stage) -> Result<(), Fail> {
        // `ready` is the one stage with no denominator — it WAITS. It carries
        // its ceiling instead of a percentage so a consumer can show "waited
        // 4:31 of up to 30:00" without dressing a clock up as progress.
        (self.emit)(match stage {
            Stage::Ready => {
                section_start_limited(stage.name(), self.opts.ready_timeout.as_secs())
            }
            _ => section_start(stage.name()),
        });
        let result = match stage {
            Stage::Preflight => self.do_preflight(),
            Stage::Guard => self.do_guard(),
            Stage::CloneCore | Stage::CloneModule => self.do_clone(stage),
            Stage::GenerateCompose => self.do_generate(),
            Stage::Build => self.do_build(),
            Stage::Up => self.do_up(),
            Stage::Ready => self.do_ready(),
        };
        match &result {
            Ok(()) => {
                (self.emit)(section_end(stage.name(), "ok"));
                // ONLY here, and only after the stage really finished. Recording
                // a stage before it completes is the one bug that turns "resume"
                // into "silently skip".
                if stage.records_completion() {
                    self.state.mark(stage);
                    self.persist();
                }
            }
            Err(_) => (self.emit)(section_end(stage.name(), "error")),
        }
        result
    }

    fn go(&mut self) -> Result<(), Fail> {
        for stage in STAGE_ORDER {
            self.run_stage(stage)?;
        }
        Ok(())
    }
}

/// Install (or resume installing) the native WoW stack, streaming NDJSON events.
/// Returns the process exit code: `0` installed, `1` refused or failed.
pub fn install_native_stream(opts: &InstallOpts, emit: impl Fn(Value)) -> i32 {
    install_native_stream_with(&ProcIo::from_env(), opts, &emit)
}

/// [`install_native_stream`] with its IO supplied rather than resolved — the
/// seam the tests drive. Production reaches this through the wrapper above, so
/// this IS the real orchestration and not a test-only restatement of it.
pub fn install_native_stream_with(
    io: &dyn InstallIo,
    opts: &InstallOpts,
    emit: &dyn Fn(Value),
) -> i32 {
    if !valid_title_id(&opts.id) {
        emit(error_event(
            CODE_BAD_ID,
            format!("{:?} is not a valid title name.", opts.id),
            "Use letters, digits, '.', '_' and '-' only.",
        ));
        return 1;
    }

    let title_dir = opts.title_dir();
    let project = opts
        .compose
        .project_name
        .clone()
        .unwrap_or_else(|| composegen::project_name_for(&title_dir));
    let existing = load_state(&title_dir);
    let resumed = existing.is_some();
    let state = existing
        .unwrap_or_else(|| InstallState::new(&opts.id, &composegen::install_id(&title_dir)));

    let mut engine = Engine { io, opts, emit, title_dir, project, state, resumed };

    if resumed {
        match next_stage(&engine.state) {
            Some(s) => engine.line(
                "info",
                format!("found a previous install in progress -- continuing from {}.", s.name()),
            ),
            None => engine.line(
                "info",
                "this install is already complete -- checking it over and starting it.",
            ),
        }
        if let Some(err) = engine.state.last_error.clone() {
            engine.line("info", format!("the last attempt stopped with: {err}"));
        }
    }

    match engine.go() {
        Ok(()) => {
            engine.state.last_error = None;
            engine.persist();
            (engine.emit)(done_event(serde_json::json!({
                "id": engine.opts.id,
                "title_dir": engine.title_dir.display().to_string(),
                "project": engine.project,
                "resumed": engine.resumed,
            })));
            0
        }
        Err(f) => {
            engine.persist_failure(&f);
            (engine.emit)(error_event(f.code, f.message, &f.hint));
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dml_core::setup::Tri;
    use std::cell::{Cell, RefCell};

    // -- fixtures ------------------------------------------------------------

    fn fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dml-install-native-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn healthy_facts() -> preflight::PreflightFacts {
        preflight::PreflightFacts {
            docker: preflight::DockerFacts {
                reachable: Tri::Yes,
                detail: None,
                mem_bytes: Some(16 * preflight::GB),
                ncpu: Some(4),
                docker_root_dir: Some("/var/lib/docker".to_string()),
                server_version: Some("27.0.0".to_string()),
            },
            disk: Some(preflight::DiskFacts {
                path: "/docker-data".to_string(),
                free_bytes: Some(200 * preflight::GB),
            }),
            games_disk: Some(preflight::DiskFacts {
                path: "/games".to_string(),
                free_bytes: Some(200 * preflight::GB),
            }),
            games_shares_docker_volume: false,
            git: Tri::Yes,
            hub: Tri::Yes,
            hub_detail: None,
        }
    }

    struct Reply {
        key: String,
        code: i32,
        /// One entry per successive call; the last entry repeats forever. That
        /// is what lets a probe answer a CHANGING value (a climbing restart
        /// count) without the fake needing a scripting language.
        out: Vec<Vec<String>>,
        unanswerable: bool,
        seen: Cell<usize>,
    }

    /// The whole IO seam, faked. Records every call in order and answers from a
    /// scripted table keyed on a substring of the joined argv.
    struct FakeIo {
        facts: preflight::PreflightFacts,
        calls: RefCell<Vec<Call>>,
        replies: Vec<Reply>,
        /// `Some(true)` = the engine is down and a start would SUCCEED;
        /// `Some(false)` = down and the start fails. `None` = already up, so
        /// `ensure_engine` is never reached. Mirrors what ProcIo does for real.
        engine_start: Option<bool>,
        engine_tried: RefCell<bool>,
    }

    impl FakeIo {
        fn healthy() -> Self {
            FakeIo {
                facts: healthy_facts(),
                calls: RefCell::new(Vec::new()),
                replies: Vec::new(),
                engine_start: None,
                engine_tried: RefCell::new(false),
            }
        }
        fn facts(mut self, f: preflight::PreflightFacts) -> Self {
            self.facts = f;
            self
        }
        /// Model a STOPPED engine that a start will (or will not) revive.
        ///
        /// The initial facts are set unreachable here rather than by the test,
        /// so a test cannot accidentally exercise the autostart path while
        /// still reporting a healthy docker -- which would pass for the wrong
        /// reason.
        fn with_engine_start(mut self, succeeds: bool) -> Self {
            self.facts.docker.reachable = dml_core::setup::Tri::No;
            self.facts.docker.detail = Some("stub: engine down".to_string());
            self.engine_start = Some(succeeds);
            self
        }
        fn engine_start_attempted(&self) -> bool {
            *self.engine_tried.borrow()
        }
        /// Register `reply`, REPLACING any existing entry with the same key
        /// in place rather than appending a second one.
        ///
        /// Load-bearing, and the reason is a bug this harness actually had:
        /// [`FakeIo::run`] resolves a call with a FIRST-match scan over this
        /// vec, so a plain `push` left every override registered after
        /// [`happy_io`] permanently unreachable — the test then asserted
        /// against happy_io's answer instead of its own. Seven tests in this
        /// module failed that way, all reading as engine bugs that did not
        /// exist. Replacing in place (rather than pushing to the front) keeps
        /// the relative order of DISTINCT keys, which matters because the keys
        /// are substrings: `compose ps -a -q …` and `ps -a --format …` must
        /// each keep matching only their own call.
        fn set(mut self, reply: Reply) -> Self {
            match self.replies.iter().position(|r| r.key == reply.key) {
                Some(i) => self.replies[i] = reply,
                None => self.replies.push(reply),
            }
            self
        }
        fn reply(self, key: &str, code: i32, out: &[&str]) -> Self {
            self.reply_seq(key, code, &[out])
        }
        fn reply_seq(self, key: &str, code: i32, out: &[&[&str]]) -> Self {
            self.set(Reply {
                key: key.to_string(),
                code,
                out: out.iter().map(|o| o.iter().map(|s| s.to_string()).collect()).collect(),
                unanswerable: false,
                seen: Cell::new(0),
            })
        }
        fn unanswerable(self, key: &str) -> Self {
            self.set(Reply {
                key: key.to_string(),
                code: -1,
                out: vec![Vec::new()],
                unanswerable: true,
                seen: Cell::new(0),
            })
        }
        /// `"<program> <joined argv>"` for every recorded call, in order.
        fn log(&self) -> Vec<String> {
            self.calls
                .borrow()
                .iter()
                .map(|c| format!("{} {}", c.program.label(), c.args.join(" ")))
                .collect()
        }
        fn pos(&self, needle: &str) -> usize {
            let log = self.log();
            log.iter()
                .position(|l| l.contains(needle))
                .unwrap_or_else(|| panic!("no call containing `{needle}`:\n{log:#?}"))
        }
        fn has(&self, needle: &str) -> bool {
            self.log().iter().any(|l| l.contains(needle))
        }
    }

    impl InstallIo for FakeIo {
        fn preflight(&self, _games_dir: &Path) -> preflight::PreflightFacts {
            let mut f = self.facts.clone();
            // The RE-GATHER is what makes the autostart real: after a
            // successful start the engine answers, and deciding on the
            // pre-start facts would refuse over a state that no longer exists.
            if *self.engine_tried.borrow() && self.engine_start == Some(true) {
                f.docker.reachable = dml_core::setup::Tri::Yes;
                f.docker.detail = None;
            }
            f
        }

        fn ensure_engine(&self, on_line: &mut dyn FnMut(String, String)) -> bool {
            match self.engine_start {
                None => false, // already up -- ProcIo returns false here too
                Some(ok) => {
                    *self.engine_tried.borrow_mut() = true;
                    on_line(
                        "info".to_string(),
                        if ok { "engine started".into() } else { "engine did not start".to_string() },
                    );
                    true
                }
            }
        }
        fn run(&self, call: &Call, on_line: &mut dyn FnMut(&str)) -> RunOutcome {
            self.calls.borrow_mut().push(call.clone());
            let joined = call.args.join(" ");
            let outcome = match self.replies.iter().find(|r| joined.contains(&r.key)) {
                Some(r) if r.unanswerable => {
                    RunOutcome::CouldNotTell("fake: nothing answered".to_string())
                }
                Some(r) => {
                    let i = r.seen.get();
                    r.seen.set(i + 1);
                    for l in &r.out[i.min(r.out.len() - 1)] {
                        on_line(l);
                    }
                    RunOutcome::Exited(r.code)
                }
                None => RunOutcome::Exited(0),
            };
            // A SUCCESSFUL clone leaves a checkout behind. Modelling that is not
            // cosmetic: the engine decides whether to re-clone by looking for
            // `<dest>/.git` on disk, so a fake that "clones" without creating one
            // describes a world that cannot exist — generated compose files in a
            // title dir with no checkout — and any test resuming through it would
            // be asserting against a fiction. (The previous code hid this because
            // the state file was allowed to skip the stage before the disk was
            // ever consulted; removing that authority exposed it.)
            if call.program == Program::Git
                && call.args.first().map(String::as_str) == Some("clone")
                && outcome.is_ok()
            {
                if let Some(dest) = call.args.last() {
                    let _ = std::fs::create_dir_all(Path::new(dest).join(".git"));
                }
            }
            outcome
        }
    }

    /// A fake wired for a complete, successful install. The two `remote get-url`
    /// answers are keyed on the DIRECTORY the probe names, which is how one fake
    /// can answer for both checkouts.
    /// A pin is only worth having if it is VERIFIED. If HEAD reads back as
    /// something other than the pin, the build is NOT reproducible, and saying
    /// so is the whole feature -- this repo has been bitten repeatedly by
    /// overrides that were silently never applied.
    #[test]
    fn a_pin_that_does_not_take_is_a_refusal_not_a_shrug() {
        let games = fixture("pin-mismatch");
        let io = happy_io().reply(
            "wow-server-playerbots rev-parse HEAD",
            0,
            &["deadbeefdeadbeefdeadbeefdeadbeefdeadbeef"],
        );
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 1, "{events:#?}");
        assert_eq!(error_code(&events), CODE_PIN_FAILED, "{events:#?}");
    }

    /// The core is cloned WITHOUT `--depth` because AzerothCore's genrev.cmake
    /// reads the repository's history. `git fetch --depth 1` against a complete
    /// repository makes it SHALLOW, so the fetch must be reached ONLY when the
    /// commit is genuinely absent.
    #[test]
    fn a_commit_already_present_is_never_fetched() {
        let games = fixture("pin-nofetch");
        // happy_io answers `rev-parse --verify` with 0 == the commit is here.
        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");
        assert!(
            !io.has("fetch --depth"),
            "a commit already present must not trigger a shallowing fetch"
        );
        // ...and the pin was still applied, so this is not passing because
        // nothing happened.
        assert!(io.has("checkout --detach"), "the pin must still be checked out");
    }

    #[test]
    fn a_missing_commit_is_fetched_before_it_is_checked_out() {
        let games = fixture("pin-fetch");
        // 1 == `rev-parse --verify --quiet` found nothing.
        let io = happy_io().reply("rev-parse --verify", 1, &[]);
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");
        assert!(io.has("fetch --depth"), "a missing commit must be fetched");
    }

    /// Build a title dir that looks like a real interrupted install: our
    /// checkout (both `.git` dirs, so the foreign-compose guard accepts it) plus
    /// generated files already on disk and the stages recorded.
    fn resumable_title(games: &Path, through: &[Stage]) -> PathBuf {
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(title.join(".git")).unwrap();
        std::fs::create_dir_all(title.join("modules/mod-playerbots/.git")).unwrap();
        for f in [composegen::BASE_FILE, composegen::BUILD_FILE, composegen::OVERRIDE_FILE] {
            std::fs::write(title.join(f), "STALE-GENERATED-OUTPUT
").unwrap();
        }
        let mut st = InstallState::new("wow-server-playerbots", &composegen::install_id(&title));
        for s in through {
            st.mark(*s);
        }
        save_state(&title, &st).unwrap();
        title
    }

    /// THE RESUME GAP. A fix to a TEMPLATE has to be able to reach a directory
    /// that already has generated files, or "resume" quietly serves the broken
    /// output the fix was written to replace.
    ///
    /// This is not hypothetical: the first live native install died after five
    /// green stages and 600+ MB of clone because the build overlay named no
    /// `dockerfile:`, and the whole class is invisible to a test that drives a
    /// fake docker which never opens the file.
    ///
    /// Before `up`, every generated file — the override included — is ours.
    #[test]
    fn a_resume_before_up_refreshes_every_generated_file() {
        let games = fixture("regen-before-up");
        let title = resumable_title(
            &games,
            &[Stage::CloneCore, Stage::CloneModule, Stage::GenerateCompose],
        );
        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");
        for f in [composegen::BASE_FILE, composegen::BUILD_FILE, composegen::OVERRIDE_FILE] {
            let body = std::fs::read_to_string(title.join(f)).unwrap();
            assert!(
                !body.contains("STALE-GENERATED-OUTPUT"),
                "{f} was not regenerated on resume -- a template fix cannot reach this install"
            );
        }
    }

    /// The other half, and the reason the switch is not simply "always
    /// regenerate": once the containers have started, the override is where
    /// `crate::config` keeps the user's bot counts, rates and SOAP settings.
    /// Note the boundary is `up`, NOT `ready` — a stack that came up and then
    /// timed out waiting for the world server is RUNNING and reachable from the
    /// Settings page, so its override is already live configuration.
    #[test]
    fn a_resume_after_up_never_touches_the_users_settings() {
        let games = fixture("regen-after-up");
        let title = resumable_title(
            &games,
            &[
                Stage::CloneCore,
                Stage::CloneModule,
                Stage::GenerateCompose,
                Stage::Build,
                Stage::Up,
            ],
        );
        // What a user's saved settings look like to this code: content we did
        // not write, in the file the config system owns.
        std::fs::write(
            title.join(composegen::OVERRIDE_FILE),
            "services:
  ac-worldserver:
    environment:
      AC_PLAYERBOTS_MAXRANDOMBOTS: \"40\"
",
        )
        .unwrap();

        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");
        let body = std::fs::read_to_string(title.join(composegen::OVERRIDE_FILE)).unwrap();
        assert!(
            body.contains("AC_PLAYERBOTS_MAXRANDOMBOTS: \"40\""),
            "a resume ate the user's settings: {body}"
        );
        // ...and the machine-owned files were still refreshed, so this is not
        // passing because the whole stage was skipped.
        let base = std::fs::read_to_string(title.join(composegen::BASE_FILE)).unwrap();
        assert!(!base.contains("STALE-GENERATED-OUTPUT"), "the base file must still be regenerated");
    }

    /// A STOPPED DOCKER DESKTOP MUST NOT END AN INSTALL.
    ///
    /// It does not run at boot on a default install, so a user resuming a build
    /// the next day meets this every single time -- and Home's Start button
    /// already brings the engine up hidden. Refusing here while doing it there
    /// was an inconsistency, not a safeguard.
    ///
    /// The FACTS ARE RE-GATHERED after the start. Deciding on the pre-start
    /// facts would refuse over a state that no longer exists, which is the
    /// whole bug this closes.
    #[test]
    fn a_stopped_engine_is_started_rather_than_refused() {
        let games = fixture("engine-autostart");
        let io = happy_io().with_engine_start(true);
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "a startable engine must not fail the install: {events:#?}");
        assert!(io.engine_start_attempted(), "the engine start was never tried");
    }

    /// ...and when it genuinely cannot be started, the refusal is unchanged.
    /// An autostart that swallowed the failure would be worse than none: the
    /// user would watch a clone begin against an engine that is not there.
    #[test]
    fn an_engine_that_will_not_start_still_refuses_honestly() {
        let games = fixture("engine-dead");
        let io = happy_io().with_engine_start(false);
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 1, "{events:#?}");
        assert_eq!(error_code(&events), preflight::CODE_DOCKER_UNREACHABLE, "{events:#?}");
    }

    /// A healthy engine must not be poked. Calling `docker desktop start` on a
    /// running engine is harmless but slow, and this runs before every install.
    #[test]
    fn a_running_engine_is_left_alone() {
        let games = fixture("engine-up");
        let io = happy_io();
        let (rc, _) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0);
        assert!(!io.engine_start_attempted(), "a running engine must not be restarted");
    }

    fn happy_io() -> FakeIo {
        FakeIo::healthy()
            .reply(
                "mod-playerbots remote get-url",
                0,
                &["https://github.com/mod-playerbots/mod-playerbots.git"],
            )
            .reply(
                "wow-server-playerbots remote get-url",
                0,
                &["https://github.com/mod-playerbots/azerothcore-wotlk.git"],
            )
            // The pin calls. Order matters and mirrors the get-url pair above:
            // the module's dest path CONTAINS the core's, so the narrower
            // "mod-playerbots ..." key must be registered first (find() takes
            // the first match).
            .reply("rev-parse --verify", 0, &[])
            .reply("checkout --detach", 0, &[])
            .reply("mod-playerbots rev-parse HEAD", 0, &[MODULE_PINNED_COMMIT])
            .reply("wow-server-playerbots rev-parse HEAD", 0, &[CORE_PINNED_COMMIT])
            .reply("ps -a --format", 0, &[])
            .reply("compose images", 0, &[])
            .reply(
                "docker-compose.build.yml build",
                0,
                &["#5 [worldserver 3/9] RUNNING cmake", "BUILD-EVIDENCE-9f8e7d"],
            )
            .reply("compose up -d", 0, &["Container ac-worldserver Started"])
            .reply("compose ps -a -q", 0, &["c0ffee1234ab"])
            .reply("logs --tail", 0, &["World initialized in 4 minutes 2 seconds."])
            .reply("inspect -f", 0, &["0"])
    }

    fn fast_opts(games: &Path) -> InstallOpts {
        let mut o = InstallOpts::new("wow-server-playerbots", games);
        o.ready_timeout = Duration::from_millis(400);
        o.ready_poll = Duration::from_millis(20);
        o
    }

    fn run_install(io: &FakeIo, opts: &InstallOpts) -> (i32, Vec<Value>) {
        let events = RefCell::new(Vec::new());
        let rc = install_native_stream_with(io, opts, &|v| events.borrow_mut().push(v));
        (rc, events.into_inner())
    }

    /// The `pct` values emitted while `section` was the open one.
    ///
    /// Scoping is the assertion, not a convenience: a percentage credited to
    /// the wrong stage is a lie about what is being measured, and every stage
    /// that can report a number now does — so a global collection would pass
    /// while measuring the wrong thing.
    fn pcts_in_section(events: &[Value], section: &str) -> Vec<u64> {
        let mut open = String::new();
        let mut out = Vec::new();
        for e in events {
            if e["event"] == "section_start" {
                open = e["name"].as_str().unwrap_or_default().to_string();
            }
            if e["event"] == "pct" && open == section {
                out.push(e["value"].as_u64().unwrap_or_default());
            }
        }
        out
    }

    fn sections(events: &[Value]) -> Vec<String> {
        events
            .iter()
            .filter(|e| e["event"] == "section_start")
            .map(|e| e["name"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    fn terminal(events: &[Value]) -> Value {
        events.last().cloned().unwrap_or(Value::Null)
    }

    fn error_code(events: &[Value]) -> String {
        terminal(events)["error"]["code"].as_str().unwrap_or_default().to_string()
    }

    fn error_message(events: &[Value]) -> String {
        terminal(events)["error"]["message"].as_str().unwrap_or_default().to_string()
    }

    fn line_texts(events: &[Value]) -> Vec<String> {
        events
            .iter()
            .filter(|e| e["event"] == "line")
            .map(|e| e["text"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    fn seed_checkout(dir: &Path) {
        std::fs::create_dir_all(dir.join(".git")).unwrap();
    }

    // -- pure argv -----------------------------------------------------------

    #[test]
    fn the_core_clone_pins_autocrlf_input_and_the_playerbot_branch_and_is_not_shallow() {
        let dest = Path::new("/games/wow-server-playerbots");
        let argv = clone_argv(&default_core_repo(), dest);
        let joined = argv.join(" ");
        assert_eq!(argv.first().map(String::as_str), Some("clone"), "{argv:?}");
        assert!(
            joined.contains("--config core.autocrlf=input"),
            "a Windows-side checkout must keep LF or the Linux build chokes: {joined}"
        );
        assert!(joined.contains("--branch Playerbot"), "{joined}");
        assert!(
            joined.contains("https://github.com/mod-playerbots/azerothcore-wotlk.git"),
            "{joined}"
        );
        assert!(
            !joined.contains("--depth"),
            "the core checkout is NOT shallow -- AzerothCore's genrev.cmake reads git history: {joined}"
        );
        assert_eq!(argv.last().map(String::as_str), Some(dest.to_string_lossy().as_ref()), "{argv:?}");
    }

    #[test]
    fn the_module_clone_is_shallow_like_the_proven_installer() {
        let argv = clone_argv(&default_module_repo(), Path::new("/games/t/modules/mod-playerbots"));
        let joined = argv.join(" ");
        assert!(joined.contains("--depth 1"), "{joined}");
        assert!(joined.contains("--branch master"), "{joined}");
        assert!(joined.contains("--config core.autocrlf=input"), "{joined}");
        assert!(joined.contains("mod-playerbots/mod-playerbots.git"), "{joined}");
    }

    #[test]
    fn build_passes_all_three_compose_files_and_up_passes_none() {
        let b = build_argv().join(" ");
        assert!(b.contains(&format!("-f {}", composegen::BASE_FILE)), "{b}");
        assert!(b.contains(&format!("-f {}", composegen::OVERRIDE_FILE)), "{b}");
        assert!(b.contains(&format!("-f {}", composegen::BUILD_FILE)), "{b}");
        assert!(b.ends_with("build"), "{b}");

        let u = up_argv().join(" ");
        assert!(
            !u.contains("-f"),
            "`up` must auto-load base+override ONLY -- naming the build file here is how a post-install start turns into a multi-hour rebuild: {u}"
        );
        assert_eq!(u, "compose up -d", "{u}");
    }

    // -- state ---------------------------------------------------------------

    #[test]
    fn state_round_trips_and_a_state_from_another_dir_is_not_trusted() {
        let base = fixture("state-roundtrip");
        let a = base.join("a");
        let b = base.join("b");
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();

        let mut st = InstallState::new("wow-server-playerbots", &composegen::install_id(&a));
        st.mark(Stage::CloneCore);
        save_state(&a, &st).unwrap();

        let back = load_state(&a).expect("state must load from the dir it was written for");
        assert!(back.is_done(Stage::CloneCore));
        assert!(!back.is_done(Stage::Build));
        assert_eq!(next_stage(&back), Some(Stage::CloneModule));

        // The same bytes copied into another title dir must NOT be trusted: the
        // identity is what proves the dir is this install's.
        std::fs::copy(state_path(&a), state_path(&b)).unwrap();
        assert!(
            load_state(&b).is_none(),
            "a state file copied into another dir must not adopt it"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    // -- guards --------------------------------------------------------------

    #[test]
    fn the_generated_base_carries_the_marker_the_guard_recognises() {
        let dir = fixture("marker");
        let text = composegen::render_base(&dir, &composegen::ComposeOpts::default()).unwrap();
        assert!(
            text.contains(GENERATED_MARKER),
            "the guard tells ours from a stranger's by this marker; the template must carry it"
        );
        assert!(
            foreign_compose_file(&dir).is_none(),
            "an empty dir has no foreign compose file"
        );
        std::fs::write(dir.join("docker-compose.yml"), &text).unwrap();
        assert!(
            foreign_compose_file(&dir).is_none(),
            "a file we generated must not be called foreign"
        );
        std::fs::write(dir.join("docker-compose.yml"), "services: {}\n").unwrap();
        assert_eq!(foreign_compose_file(&dir), Some(dir.join("docker-compose.yml")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_guard_refuses_a_compose_file_the_engine_did_not_generate() {
        // The default games dir is %USERPROFILE%\dml-native, whose
        // wow-server-playerbots IS the working migrated server. Generating over
        // it re-identifies that stack and orphans its volumes and images.
        let games = fixture("guard-foreign-compose");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        std::fs::write(title.join("docker-compose.yml"), "name: dml-wow-native\nservices: {}\n").unwrap();

        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_COMPOSE_EXISTS, "{events:#?}");
        assert!(
            error_message(&events).contains("docker-compose.yml"),
            "the refusal must NAME the file: {}",
            error_message(&events)
        );
        assert!(
            !io.has("clone --progress"),
            "nothing may be cloned once the guard has refused: {:#?}",
            io.log()
        );
        assert!(!state_path(&title).exists(), "a refused install must leave no state file");
        assert!(
            !title.join(composegen::BUILD_FILE).exists(),
            "a refused install must write nothing"
        );

        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn the_guard_refuses_when_another_stack_owns_the_ac_container_names() {
        // container_name: ac-* is GLOBAL to the docker engine, not per project.
        // Two generated stacks can never be up at once, and every bare-name call
        // in this codebase (`docker exec ac-database mysqldump` is a WRITE) acts
        // on whichever stack owns the name.
        let games = fixture("guard-stack-conflict");
        let io = happy_io().reply(
            "ps -a --format",
            0,
            &["ac-database dml-wow-native", "ac-worldserver dml-wow-native"],
        );
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_STACK_CONFLICT, "{events:#?}");
        let msg = error_message(&events);
        assert!(msg.contains("ac-database"), "must name the container it collided on: {msg}");
        assert!(msg.contains("dml-wow-native"), "must name the stack that owns it: {msg}");
        assert!(
            !io.has("clone --progress"),
            "nothing may be cloned once the guard has refused: {:#?}",
            io.log()
        );

        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_container_owned_by_this_install_is_not_a_conflict() {
        let games = fixture("guard-own-stack");
        let title = games.join("wow-server-playerbots");
        let ours = composegen::project_name_for(&title);
        let io = happy_io().reply("ps -a --format", 0, &[&format!("ac-worldserver {ours}")]);
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "resuming our own stack must not be called a conflict: {events:#?}");
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_docker_ps_that_cannot_answer_warns_and_proceeds() {
        // Tri-state discipline: docker failing to answer is evidence of NOTHING.
        // Refusing on it would block installs on a slow engine; asserting "no
        // conflict" would race one. It warns.
        let games = fixture("guard-ps-unanswerable");
        let io = happy_io().unanswerable("ps -a --format");
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 0, "{events:#?}");
        let warned = events.iter().any(|e| {
            e["event"] == "line"
                && e["level"] == "warn"
                && e["text"].as_str().unwrap_or_default().contains("could not check")
        });
        assert!(warned, "a could-not-tell must be SAID, not swallowed: {:#?}", line_texts(&events));
        let _ = std::fs::remove_dir_all(&games);
    }

    fn owner(project: &str, wdir: &str) -> StackOwner {
        StackOwner { project: project.to_string(), working_dir: wdir.to_string() }
    }

    #[test]
    fn conflicting_owner_reports_the_first_owned_container_a_stranger_holds() {
        let here = Path::new("C:/games/wow");
        let mut owners = BTreeMap::new();
        owners.insert("ac-worldserver".to_string(), owner("someone-elses", "C:/theirs/wow"));
        assert_eq!(
            conflicting_owner(&owners, "dml-wow-server-playerbots-1234abcd", here),
            Some(("ac-worldserver".to_string(), "someone-elses".to_string()))
        );
        assert_eq!(conflicting_owner(&owners, "someone-elses", here), None);

        // A container with no compose project label still OWNS the name.
        let mut bare = BTreeMap::new();
        bare.insert("ac-database".to_string(), StackOwner::default());
        let hit = conflicting_owner(&bare, "dml-x", here).expect("an unlabelled ac-* container is still a conflict");
        assert_eq!(hit.0, "ac-database");
        assert!(
            stack_conflict_message(&hit.0, &hit.1).contains("not managed by Docker Compose"),
            "{}",
            stack_conflict_message(&hit.0, &hit.1)
        );
    }

    #[test]
    fn a_stack_composed_from_our_own_directory_is_ours_regardless_of_its_name() {
        // The live incident (2026-08-02): project "dml-wow-native" on the
        // user's own migrated server, underivable, refused as foreign. The
        // working-dir label rescues it -- in every spelling a shell produces.
        let here = Path::new("C:\\Users\\perzi\\dml-native\\wow-server-playerbots");
        for spelling in [
            "C:\\Users\\perzi\\dml-native\\wow-server-playerbots",
            "/c/Users/perzi/dml-native/wow-server-playerbots",
            "/mnt/c/Users/perzi/dml-native/wow-server-playerbots",
            "c:/users/perzi/DML-NATIVE/wow-server-playerbots/",
        ] {
            let mut owners = BTreeMap::new();
            owners.insert("ac-database".to_string(), owner("dml-wow-native", spelling));
            assert_eq!(
                conflicting_owner(&owners, "dml-wow-server-playerbots-5c541930", here),
                None,
                "spelling {spelling:?} was read as foreign"
            );
        }
        // ...while a genuinely different directory still refuses.
        let mut foreign = BTreeMap::new();
        foreign.insert("ac-database".to_string(), owner("dml-wow-native", "C:/somewhere/else"));
        assert!(conflicting_owner(&foreign, "dml-wow-server-playerbots-5c541930", here).is_some());
    }

    #[test]
    fn canon_path_folds_every_shell_spelling_of_the_same_windows_dir() {
        let want = "c:/users/first last/dml-native";
        for s in [
            "C:\\Users\\First Last\\dml-native",
            "C:/Users/First Last/dml-native/",
            "/c/Users/First Last/dml-native",
            "/mnt/c/Users/First Last/dml-native",
            "//?/C:/Users/First Last/dml-native",
        ] {
            assert_eq!(canon_path(s), want, "spelling {s:?}");
        }
        // POSIX paths keep their case: /srv/A and /srv/a really differ.
        assert_eq!(canon_path("/srv/Games/wow/"), "/srv/Games/wow");
        assert_ne!(canon_path("/srv/A"), canon_path("/srv/a"));
    }

    #[test]
    fn the_readiness_inspect_asks_for_fields_docker_actually_has() {
        // LIVE INCIDENT 2026-08-03. This constant said
        // `{{.State.RestartCount}}`. Docker has no such field -- RestartCount
        // is TOP-LEVEL -- so every inspect failed with "map has no entry for
        // key", `started_at` came back None, and both ready loops fell through
        // to `docker logs --tail <N>`.
        //
        // That fallback is precisely what `--since` was introduced to replace:
        // the ready marker prints ONCE, early, and a boot with 500 playerbots
        // logging in pushes it out of a fixed tail window within seconds. The
        // wait then sat for its full timeout on a server that was up and had
        // already printed the line.
        //
        // A pin, not a proof -- only docker can say which fields exist, which
        // is what the live test below is for.
        assert!(
            READY_INSPECT_FORMAT.contains("{{.RestartCount}}"),
            "RestartCount is top-level, not under .State: {READY_INSPECT_FORMAT}"
        );
        assert!(
            !READY_INSPECT_FORMAT.contains(".State.RestartCount"),
            "the field that does not exist is back: {READY_INSPECT_FORMAT}"
        );
        // StartedAt genuinely IS under .State.
        assert!(READY_INSPECT_FORMAT.contains("{{.State.StartedAt}}"));
        // And the two halves must stay `|`-separated, which is what
        // parse_started_and_restarts splits on.
        assert_eq!(READY_INSPECT_FORMAT.matches('|').count(), 1);
    }

    /// LIVE. Runs the real inspect against a real container and asserts BOTH
    /// halves parse.
    ///
    /// This is the only test that can catch a bad template field: a fake
    /// answers whatever it was told to, so the entire suite stayed green for
    /// weeks against a format string docker rejected outright.
    ///
    /// ```text
    /// cargo test -p dml-wow --lib install_native::tests::live_ -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs a running container (set DML_LIVE_CONTAINER, default ac-worldserver)"]
    fn live_the_readiness_inspect_format_is_accepted_by_docker() {
        let container =
            std::env::var("DML_LIVE_CONTAINER").unwrap_or_else(|_| "ac-worldserver".to_string());
        let out = std::process::Command::new(dml_core::engine::docker_program())
            .args(["inspect", "-f", READY_INSPECT_FORMAT, &container])
            .output()
            .expect("spawn docker");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        eprintln!("stdout: {stdout}\nstderr: {stderr}");
        assert!(
            out.status.success(),
            "docker rejected the format string -- this is the 2026-08-03 bug: {stderr}"
        );
        // A template error can still exit 0 on some versions while printing the
        // complaint, so parse the ANSWER rather than trusting the exit code.
        let (started, restarts) = parse_started_and_restarts(&stdout);
        assert!(started.is_some(), "no StartedAt parsed from {stdout:?}");
        assert!(restarts.is_some(), "no RestartCount parsed from {stdout:?}");
        assert!(
            started.as_deref().unwrap_or("").contains('T'),
            "StartedAt is not a timestamp: {started:?}"
        );
    }

    #[test]
    fn parse_stack_owners_reads_name_and_project_and_ignores_blank_lines() {
        // Tab-separated (the real format), with the space-separated fallback
        // still parsing as name+project.
        let owners = parse_stack_owners(
            "ac-database\tproj-a\tC:/a b/wow\n\nac-worldserver proj-a\nother-thing\tproj-b\t\n",
        );
        assert_eq!(owners.get("ac-database").map(|o| o.project.as_str()), Some("proj-a"));
        assert_eq!(
            owners.get("ac-database").map(|o| o.working_dir.as_str()),
            Some("C:/a b/wow"),
            "a working dir containing a space must survive the parse"
        );
        assert_eq!(owners.get("ac-worldserver").map(|o| o.project.as_str()), Some("proj-a"));
        assert_eq!(owners.get("other-thing").map(|o| o.project.as_str()), Some("proj-b"));
        assert_eq!(owners.len(), 3);
        // A container with no project label parses to an empty project, never
        // to a missing entry -- it still owns the name.
        let bare = parse_stack_owners("ac-database\n");
        assert_eq!(bare.get("ac-database"), Some(&StackOwner::default()));
    }

    // -- preflight gate ------------------------------------------------------

    #[test]
    fn a_preflight_refusal_stops_before_anything_is_probed_or_written() {
        let games = fixture("preflight-refuse");
        let mut facts = healthy_facts();
        facts.docker.reachable = Tri::Unknown;
        facts.docker.mem_bytes = None;
        let io = happy_io().facts(facts);
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), preflight::CODE_DOCKER_UNREACHABLE, "{events:#?}");
        assert!(io.log().is_empty(), "no docker/git call may run after a preflight refusal: {:#?}", io.log());
        assert!(!games.join("wow-server-playerbots").exists(), "nothing may be created");
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn allow_underspec_downgrades_a_hardware_floor_and_the_install_proceeds() {
        let games = fixture("preflight-underspec");
        let mut facts = healthy_facts();
        facts.docker.mem_bytes = Some(4 * preflight::GB);
        let io = happy_io().facts(facts.clone());

        let mut refused = fast_opts(&games);
        refused.allow_underspec = false;
        let (rc, events) = run_install(&io, &refused);
        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), preflight::CODE_UNDERSPEC, "{events:#?}");

        let games2 = fixture("preflight-underspec-allowed");
        let io2 = happy_io().facts(facts);
        let mut allowed = fast_opts(&games2);
        allowed.allow_underspec = true;
        let (rc2, events2) = run_install(&io2, &allowed);
        assert_eq!(rc2, 0, "{events2:#?}");
        assert!(
            line_texts(&events2).iter().any(|t| t.contains("4.0 GB")),
            "the override must keep the NUMBERS in the copy: {:#?}",
            line_texts(&events2)
        );

        let _ = std::fs::remove_dir_all(&games);
        let _ = std::fs::remove_dir_all(&games2);
    }

    // -- the happy path ------------------------------------------------------

    #[test]
    fn a_fresh_install_runs_the_stages_in_order_and_calls_git_then_docker_in_that_order() {
        let games = fixture("happy-order");
        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 0, "{events:#?}");
        assert_eq!(
            sections(&events),
            vec![
                "preflight",
                "guard",
                "clone-core",
                "clone-module",
                "generate-compose",
                "build",
                "up",
                "ready"
            ],
            "{events:#?}"
        );

        // The ORDER read back off the calls that were actually made -- never a
        // pure list production does not consume.
        let core = io.pos("azerothcore-wotlk.git");
        let module = io.pos("mod-playerbots.git");
        let build = io.pos("docker-compose.build.yml build");
        let up = io.pos("compose up -d");
        let ready = io.pos("compose ps -a -q");
        assert!(core < module, "{:#?}", io.log());
        assert!(module < build, "{:#?}", io.log());
        assert!(build < up, "{:#?}", io.log());
        assert!(up < ready, "{:#?}", io.log());

        // The guard's engine-wide probe runs before ANY clone.
        assert!(io.pos("ps -a --format") < core, "{:#?}", io.log());

        assert_eq!(terminal(&events)["event"], "done", "{events:#?}");
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_fresh_install_generates_the_three_compose_files_with_soap_switched_on() {
        // SHIP-LIST 4.0e, found live: an install whose launcher cannot talk SOAP
        // has dead GM tools, dead My Party and a dead console, with no cause
        // shown. This asserts the file on disk, not the option that produced it.
        let games = fixture("happy-soap");
        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");

        let title = games.join("wow-server-playerbots");
        assert!(title.join(composegen::BASE_FILE).is_file());
        assert!(title.join(composegen::BUILD_FILE).is_file());
        let over = std::fs::read_to_string(title.join(composegen::OVERRIDE_FILE)).unwrap();
        assert!(over.contains("AC_SOAP_ENABLED"), "{over}");
        assert!(over.contains("AC_SOAP_PORT"), "{over}");
        assert!(over.contains("./modules:/azerothcore/modules"), "{over}");

        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn the_build_output_is_teed_into_the_title_dir() {
        let games = fixture("happy-tee");
        let io = happy_io();
        let (rc, _events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0);

        let logs = games.join("wow-server-playerbots").join(BUILD_LOG_DIR);
        let files: Vec<PathBuf> = std::fs::read_dir(&logs)
            .unwrap_or_else(|e| panic!("no build log dir at {}: {e}", logs.display()))
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.file_name().unwrap_or_default().to_string_lossy().starts_with("build-"))
            .collect();
        assert_eq!(files.len(), 1, "exactly one build log expected, got {files:?}");
        let body = std::fs::read_to_string(&files[0]).unwrap();
        assert!(
            body.contains("BUILD-EVIDENCE-9f8e7d"),
            "the tee must hold the build's own output: {body}"
        );

        let _ = std::fs::remove_dir_all(&games);
    }

    // -- resume --------------------------------------------------------------

    #[test]
    fn a_resume_skips_the_clones_and_the_build_when_the_disk_agrees() {
        let games = fixture("resume-skip");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        seed_checkout(&title);
        seed_checkout(&title.join("modules").join("mod-playerbots"));

        let mut st = InstallState::new("wow-server-playerbots", &composegen::install_id(&title));
        st.mark(Stage::CloneCore);
        st.mark(Stage::CloneModule);
        st.mark(Stage::GenerateCompose);
        st.mark(Stage::Build);
        save_state(&title, &st).unwrap();

        // The IMAGE has to exist for the build to be skippable. The state file
        // recording `build` is no longer enough on its own, and that is the
        // point: if the image is gone, rebuilding is the correct answer.
        let io = happy_io().reply("compose images", 0, &["sha256:c0ffee1234ab"]);
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 0, "{events:#?}");
        assert!(!io.has("clone --progress"), "a resume must not re-clone: {:#?}", io.log());
        assert!(
            !io.has("docker-compose.build.yml build"),
            "a resume must not re-run a completed build: {:#?}",
            io.log()
        );
        assert!(io.has("compose up -d"), "a resume must still start the stack: {:#?}", io.log());
        assert!(io.has("compose ps -a -q"), "a resume must still wait for ready: {:#?}", io.log());
        assert!(
            line_texts(&events).iter().any(|t| t.contains("continuing from up")),
            "the recorded progress must be USED, not just stored: {:#?}",
            line_texts(&events)
        );

        let _ = std::fs::remove_dir_all(&games);
    }

    /// THE SECURITY TEST for the hole adversarial review found on 2026-07-29,
    /// and the one no test covered: a state file must NOT be able to disable the
    /// foreign-compose guard.
    ///
    /// `.dml-install.json`'s only integrity check is that its `install_id`
    /// matches a hash OF THE PATH, which anyone can compute and which says
    /// nothing about who created the directory. When "a state file loaded" was
    /// what bypassed the guard, dropping one beside the user's real migrated
    /// server let `generate-compose` rewrite its `docker-compose.yml` under a new
    /// project name — orphaning the volumes that hold his characters.
    #[test]
    fn a_state_file_cannot_disable_the_foreign_compose_guard() {
        let games = fixture("guard-not-bypassed-by-state");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        // A real server: someone else's compose file, and NOT a checkout of ours.
        std::fs::write(title.join("docker-compose.yml"), "name: dml-wow-native\nservices: {}\n")
            .unwrap();
        let original = std::fs::read(title.join("docker-compose.yml")).unwrap();

        // A perfectly well-formed state file claiming the clones are finished --
        // exactly what the old code accepted as proof this directory was ours.
        let mut st = InstallState::new("wow-server-playerbots", &composegen::install_id(&title));
        st.mark(Stage::CloneCore);
        st.mark(Stage::CloneModule);
        save_state(&title, &st).unwrap();

        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1, "{events:#?}");
        assert_eq!(error_code(&events), CODE_COMPOSE_EXISTS, "{events:#?}");
        assert_eq!(
            std::fs::read(title.join("docker-compose.yml")).unwrap(),
            original,
            "the user's compose file must be byte-identical afterwards"
        );
        assert!(
            !title.join(composegen::BUILD_FILE).exists(),
            "nothing may be generated into a directory the guard refused"
        );
        assert!(!io.has("compose up -d"), "no stack may be started: {:#?}", io.log());

        let _ = std::fs::remove_dir_all(&games);
    }

    /// The readiness marker is printed ONCE, so the log read must be scoped to
    /// the container's start rather than to a fixed tail — otherwise a chatty
    /// first boot scrolls it away between polls and a server that came up fine is
    /// reported as `INSTALL_READY_TIMEOUT` half an hour later.
    #[test]
    fn readiness_reads_the_log_since_container_start_not_a_fixed_tail() {
        let games = fixture("ready-since");
        let io = happy_io()
            .reply("inspect -f", 0, &["2026-07-30T21:15:04.123456789Z|0"])
            // Only a --since read is answered with the marker. If production
            // asked with --tail instead, this key would not match, the fake would
            // return its default success with NO output, the marker would never be
            // seen, and the test would fail at the deadline.
            .reply("logs --since 2026-07-30T21:15:04.123456789Z", 0, &["World initialized in 4 minutes."]);

        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");
        assert!(
            io.has("logs --since 2026-07-30T21:15:04.123456789Z"),
            "the log read must be scoped to StartedAt: {:#?}",
            io.log()
        );
        assert!(
            !io.has("logs --tail"),
            "a fixed tail can scroll past a one-time marker: {:#?}",
            io.log()
        );
        let _ = std::fs::remove_dir_all(&games);
    }

    /// Each half of the combined inspect degrades on its own, and neither is
    /// guessed — a missed restart count must not become a fake zero, and an
    /// unusable timestamp must not be handed to `docker logs --since`.
    #[test]
    fn the_readiness_inspect_parses_each_half_independently() {
        assert_eq!(
            parse_started_and_restarts("2026-07-30T21:15:04Z|3"),
            (Some("2026-07-30T21:15:04Z".to_string()), Some(3))
        );
        // Restart count unreadable -> None, NOT 0 (0 would hide a real loop).
        assert_eq!(
            parse_started_and_restarts("2026-07-30T21:15:04Z|"),
            (Some("2026-07-30T21:15:04Z".to_string()), None)
        );
        // A Go template with a missing field, and the zero time: neither is a
        // usable `--since` value, so the caller falls back to a tail.
        assert_eq!(parse_started_and_restarts("<no value>|2"), (None, Some(2)));
        assert_eq!(parse_started_and_restarts("0001-01-01T00:00:00Z|0"), (None, Some(0)));
        // No separator at all: give up on the timestamp, still try the count.
        assert_eq!(parse_started_and_restarts("7"), (None, Some(7)));
        assert_eq!(parse_started_and_restarts(""), (None, None));
    }

    /// THE ACCEPTANCE HALF of the guard, and the half nobody had covered.
    ///
    /// Between `clone-core` and `generate-compose` the title dir legitimately
    /// holds UPSTREAM's own `docker-compose.yml`, which carries no
    /// [`GENERATED_MARKER`] and is therefore "foreign" by
    /// [`foreign_compose_file`]'s reckoning. The guard MUST let that through, or
    /// every real install that stops after the core clone can never continue.
    ///
    /// Without this test the bypass was mutation-survivable in the dangerous
    /// direction: deleting it (i.e. always refusing) left the whole suite green
    /// while breaking every genuine resume, because no fixture ever combined a
    /// valid checkout WITH an unmarked compose file. Caught by review after the
    /// refusal-side test was already written — the mirror image of
    /// [`Self::a_state_file_cannot_disable_the_foreign_compose_guard`].
    #[test]
    fn upstreams_own_compose_file_is_accepted_when_the_checkout_is_ours() {
        let games = fixture("guard-accepts-our-checkout");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        // Exactly the state a completed `clone-core` leaves behind: our checkout,
        // and upstream's unmarked compose file at the title-dir root.
        seed_checkout(&title);
        std::fs::write(
            title.join("docker-compose.yml"),
            "services:\n  ac-worldserver:\n    image: acore/ac-wotlk-worldserver:master\n",
        )
        .unwrap();
        assert!(
            foreign_compose_file(&title).is_some(),
            "precondition: upstream's file must look foreign, or this proves nothing"
        );
        // NOTE: no state file at all. The guard must decide from the checkout.
        assert!(load_state(&title).is_none());

        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 0, "a resume over our own checkout must proceed: {events:#?}");
        // The CORE checkout specifically must be adopted rather than re-cloned.
        // The MODULE clone does run, and correctly so -- only the core checkout
        // was seeded here, which is exactly the state a run interrupted after
        // `clone-core` leaves behind.
        assert!(
            !io.has("azerothcore-wotlk.git"),
            "the existing core checkout must be adopted, not re-cloned: {:#?}",
            io.log()
        );
        assert!(
            io.has("mod-playerbots.git"),
            "the module was never cloned, so this run must still clone it: {:#?}",
            io.log()
        );
        // And generate-compose legitimately replaced upstream's file with ours.
        let text = std::fs::read_to_string(title.join("docker-compose.yml")).unwrap();
        assert!(
            text.contains(GENERATED_MARKER),
            "generate-compose must have run over upstream's file"
        );

        let _ = std::fs::remove_dir_all(&games);
    }

    /// The module's documented promise — "a state file that is missing or stale
    /// is a slow path rather than a wrong one" — made checkable. It used to be
    /// false: `is_done` short-circuited both clones and the build before any disk
    /// check, so a state file that LIED produced skipped stages, not slow ones.
    #[test]
    fn a_state_file_that_lies_about_the_disk_takes_the_slow_path_not_a_wrong_one() {
        let games = fixture("state-lies");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        // State claims EVERYTHING is done. Disk has no checkout, and the fake
        // reports no image (happy_io's `compose images` answers empty).
        let mut st = InstallState::new("wow-server-playerbots", &composegen::install_id(&title));
        for s in STAGE_ORDER.into_iter().filter(|s| s.records_completion()) {
            st.mark(s);
        }
        save_state(&title, &st).unwrap();

        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 0, "{events:#?}");
        assert!(
            io.has("clone --progress"),
            "a checkout that is not on disk must be re-cloned, whatever the state file says: {:#?}",
            io.log()
        );
        assert!(
            io.has("docker-compose.build.yml build"),
            "an image that does not exist must be rebuilt, whatever the state file says: {:#?}",
            io.log()
        );

        let _ = std::fs::remove_dir_all(&games);
    }

    /// A false "already built" LATCHES: a skipped build is recorded as completed,
    /// so once prose fooled the probe the build was disabled on every later run
    /// too and `up` failed forever on a missing image. `compose images -q` prints
    /// IDs only, so the question is "was the output empty?" and no warning can
    /// answer it.
    #[test]
    fn a_warning_that_merely_mentions_worldserver_does_not_pass_for_a_built_image() {
        let games = fixture("build-probe-prose");
        let io = happy_io().reply(
            "compose images",
            0,
            &["", "  ", "\t"],
        );
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");
        assert!(
            io.has("docker-compose.build.yml build"),
            "blank-only output means NO image, so the build must run: {:#?}",
            io.log()
        );
        assert!(
            io.has("compose images -q"),
            "the probe must ask for ids, not prose: {:#?}",
            io.log()
        );
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_build_already_in_the_engine_is_skipped_even_without_a_state_file() {
        // The portable skip the proven WSL installer uses
        // (install-wow-wotlk.sh:426-435): `docker compose images | worldserver`.
        let games = fixture("resume-images");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        seed_checkout(&title);
        seed_checkout(&title.join("modules").join("mod-playerbots"));
        let mut st = InstallState::new("wow-server-playerbots", &composegen::install_id(&title));
        st.mark(Stage::CloneCore);
        save_state(&title, &st).unwrap();

        // `-q` output: bare image ids, which is what makes the probe unambiguous.
        let io = happy_io().reply("compose images", 0, &["sha256:9f8e7d6c5b4a"]);
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 0, "{events:#?}");
        assert!(
            !io.has("docker-compose.build.yml build"),
            "images already built must skip the multi-hour build: {:#?}",
            io.log()
        );
        // The module checkout on disk was ADOPTED rather than re-cloned.
        assert!(io.has("remote get-url"), "{:#?}", io.log());
        assert!(!io.has("clone --progress"), "{:#?}", io.log());
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_checkout_whose_remote_is_a_stranger_is_refused_rather_than_clobbered() {
        let games = fixture("resume-wrong-remote");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        seed_checkout(&title);
        let st = InstallState::new("wow-server-playerbots", &composegen::install_id(&title));
        save_state(&title, &st).unwrap();

        let io = FakeIo::healthy()
            .reply("remote get-url", 0, &["https://example.invalid/other.git"])
            .reply("ps -a --format", 0, &[]);
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_WRONG_REMOTE, "{events:#?}");
        assert!(error_message(&events).contains("example.invalid"), "{}", error_message(&events));
        assert!(!io.has("clone --progress"), "the stranger's checkout must not be clobbered: {:#?}", io.log());
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_non_empty_target_that_is_not_a_checkout_is_refused_by_name_not_deleted() {
        let games = fixture("resume-dirty-dir");
        let title = games.join("wow-server-playerbots");
        std::fs::create_dir_all(&title).unwrap();
        std::fs::write(title.join("my-notes.txt"), "keep me").unwrap();
        let st = InstallState::new("wow-server-playerbots", &composegen::install_id(&title));
        save_state(&title, &st).unwrap();

        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_DIR_NOT_EMPTY, "{events:#?}");
        assert!(
            error_message(&events).contains("wow-server-playerbots"),
            "{}",
            error_message(&events)
        );
        assert!(title.join("my-notes.txt").is_file(), "the engine must never delete a user's files");
        let _ = std::fs::remove_dir_all(&games);
    }

    // -- failure paths -------------------------------------------------------

    #[test]
    fn a_failing_build_leaves_a_state_the_next_run_resumes_from() {
        let games = fixture("fail-build-resume");
        let title = games.join("wow-server-playerbots");

        let io = happy_io().reply("docker-compose.build.yml build", 2, &["cc1plus: fatal error", "Killed"]);
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_BUILD_FAILED, "{events:#?}");

        let st = load_state(&title).expect("a failed build must still leave a resumable state file");
        assert!(st.is_done(Stage::CloneCore), "{st:?}");
        assert!(st.is_done(Stage::CloneModule), "{st:?}");
        assert!(st.is_done(Stage::GenerateCompose), "{st:?}");
        assert!(
            !st.is_done(Stage::Build),
            "a stage may only be recorded AFTER it succeeded: {st:?}"
        );
        assert_eq!(next_stage(&st), Some(Stage::Build));
        assert!(st.last_error.is_some(), "{st:?}");

        // The re-run continues where it stopped: no re-clone, and the build is
        // retried rather than skipped.
        let io2 = happy_io();
        let (rc2, events2) = run_install(&io2, &fast_opts(&games));
        assert_eq!(rc2, 0, "{events2:#?}");
        assert!(!io2.has("clone --progress"), "{:#?}", io2.log());
        assert!(io2.has("docker-compose.build.yml build"), "{:#?}", io2.log());
        assert!(
            load_state(&title).unwrap().last_error.is_none(),
            "a successful run must clear the recorded failure"
        );

        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_failing_clone_reports_it_and_records_nothing_as_done() {
        let games = fixture("fail-clone");
        let title = games.join("wow-server-playerbots");
        let io = happy_io().reply("clone --progress", 128, &["fatal: could not read from remote"]);
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_CLONE_FAILED, "{events:#?}");
        // NB the needle is the BUILD OVERLAY, not "compose build": build_argv()
        // renders `compose -f … -f … -f docker-compose.build.yml build`, which
        // never contains the substring "compose build" — so the obvious spelling
        // was an assertion that could not fail (adversarial review, 2026-07-29).
        assert!(
            !io.has("docker-compose.build.yml build"),
            "the build must not start after a failed clone: {:#?}",
            io.log()
        );
        assert!(
            !state_path(&title).exists(),
            "a clone that created nothing must leave the target untouched -- a stray state file is itself the non-empty dir that would block the retry"
        );
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_failing_up_is_reported_and_the_readiness_wait_never_starts() {
        let games = fixture("fail-up");
        let io = happy_io().reply("compose up -d", 1, &["Error response from daemon: port is already allocated"]);
        let (rc, events) = run_install(&io, &fast_opts(&games));

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_UP_FAILED, "{events:#?}");
        assert!(!io.has("compose ps -a -q"), "{:#?}", io.log());
        assert!(
            load_state(&games.join("wow-server-playerbots")).unwrap().is_done(Stage::Build),
            "the completed build must survive a failed up so the retry is cheap"
        );
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn the_readiness_wait_runs_to_its_deadline_before_reporting_a_timeout() {
        // ANTI-VACUOUS: a fake that fails instantly cannot satisfy
        // `elapsed >= deadline`, so this cannot pass by never having polled.
        let games = fixture("ready-timeout");
        let io = happy_io().reply("logs --tail", 0, &["still loading maps"]);
        let mut opts = fast_opts(&games);
        opts.ready_timeout = Duration::from_millis(300);
        opts.ready_poll = Duration::from_millis(25);

        let started = Instant::now();
        let (rc, events) = run_install(&io, &opts);
        let elapsed = started.elapsed();

        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_READY_TIMEOUT, "{events:#?}");
        assert!(
            elapsed >= Duration::from_millis(300),
            "the wait must actually span its deadline; took {elapsed:?}"
        );
        let st = load_state(&games.join("wow-server-playerbots")).unwrap();
        assert!(st.is_done(Stage::Up), "{st:?}");
        assert!(!st.is_done(Stage::Ready), "{st:?}");
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn the_readiness_wait_diagnoses_a_climbing_restart_count_once_and_only_once() {
        // The recorded native gap: `compose up -d` returns in seconds, so the
        // boot-loop watch had nothing to span. This wait is that span.
        let games = fixture("ready-bootloop");
        let io = happy_io()
            .reply("logs --tail", 0, &["Could not connect to MySQL", "Could not connect to MySQL"])
            .reply_seq(
                "inspect -f",
                0,
                &[&["0"], &["1"], &["2"], &["3"], &["4"], &["5"], &["6"], &["7"]],
            );
        let mut opts = fast_opts(&games);
        opts.ready_timeout = Duration::from_millis(300);
        opts.ready_poll = Duration::from_millis(20);

        let (rc, events) = run_install(&io, &opts);
        assert_eq!(rc, 1, "a diagnosed boot loop is advisory -- it must not change the outcome");
        assert_eq!(error_code(&events), CODE_READY_TIMEOUT, "{events:#?}");
        let notes: Vec<String> = line_texts(&events)
            .into_iter()
            .filter(|t| t.contains("boot loop detected"))
            .collect();
        assert_eq!(notes.len(), 1, "the accusation is LATCHED -- exactly one: {notes:#?}");
        assert!(
            notes[0].contains("cannot reach the database"),
            "repeated MySQL errors in the log must name the cause: {}",
            notes[0]
        );
        let _ = std::fs::remove_dir_all(&games);
    }

    #[test]
    fn a_constant_restart_count_is_never_called_a_boot_loop() {
        // The baseline is the FIRST READABLE reading, never a fixed zero: a
        // container carrying historical restarts must not trip it.
        let games = fixture("ready-no-bootloop");
        let io = happy_io()
            .reply("logs --tail", 0, &["still loading maps"])
            .reply("inspect -f", 0, &["117"]);
        let mut opts = fast_opts(&games);
        opts.ready_timeout = Duration::from_millis(200);
        opts.ready_poll = Duration::from_millis(20);

        let (_rc, events) = run_install(&io, &opts);
        assert!(
            !line_texts(&events).iter().any(|t| t.contains("boot loop detected")),
            "{:#?}",
            line_texts(&events)
        );
        let _ = std::fs::remove_dir_all(&games);
    }

    /// An unset games dir must REFUSE, not silently install into the cwd.
    ///
    /// Deliberately does not touch the process environment (these tests run in
    /// parallel and `set_var` is global): it asserts the two pure halves —
    /// that the refusal carries an actionable code and names the variable, and
    /// that a set value is passed through unchanged.
    #[test]
    fn an_unset_games_dir_is_refused_with_something_the_user_can_act_on() {
        // Whatever the ambient environment is, exactly one of these holds.
        match games_dir_for_install() {
            Err(e) => {
                assert_eq!(e.code, CODE_NO_GAMES_DIR);
                assert!(
                    e.hint.contains("DML_GAMES_DIR"),
                    "the refusal must name the variable to set: {}",
                    e.hint
                );
                assert!(!e.message.is_empty());
            }
            Ok(d) => {
                // Set in this environment -- then it must be the value verbatim,
                // never the cwd fallback that `games_dir_from_env` would give.
                let raw = std::env::var_os("DML_GAMES_DIR").expect("Ok implies it is set");
                assert_eq!(d, PathBuf::from(raw));
                assert_ne!(d, PathBuf::from("."), "the cwd fallback must not be reachable here");
            }
        }
    }

    #[test]
    fn an_id_that_is_not_a_plain_title_name_is_refused_before_any_path_is_joined() {
        assert!(valid_title_id("wow-server-playerbots"));
        assert!(valid_title_id("wow_server.1"));
        assert!(!valid_title_id(""));
        assert!(!valid_title_id(".."));
        assert!(!valid_title_id("."));
        assert!(!valid_title_id("../escape"));
        assert!(!valid_title_id("a b"));
        assert!(!valid_title_id("C:\\x"));

        let games = fixture("bad-id");
        let io = happy_io();
        let mut opts = fast_opts(&games);
        opts.id = "../escape".to_string();
        let (rc, events) = run_install(&io, &opts);
        assert_eq!(rc, 1);
        assert_eq!(error_code(&events), CODE_BAD_ID, "{events:#?}");
        assert!(io.log().is_empty(), "{:#?}", io.log());
        let _ = std::fs::remove_dir_all(&games);
    }

    /// Every PROBE carries a wall-clock bound; the two long jobs carry none.
    ///
    /// Asserted against the calls a REAL run actually made, not against a list
    /// restated here — the trap this project already paid for once was pinning an
    /// order/shape on a pure list production never reads.
    ///
    /// Why it matters in both directions: an unbounded probe lets a wedged
    /// dockerd hold a stage open forever (the readiness loop only consults its
    /// deadline after a call RETURNS), and a bounded BUILD would be killed
    /// mid-compile after hours of work.
    #[test]
    fn every_probe_is_time_bounded_and_the_long_jobs_are_not() {
        let games = fixture("probe-bounds");
        let io = happy_io();
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");

        let calls = io.calls.borrow();
        let find = |needle: &str| -> Call {
            calls
                .iter()
                .find(|c| c.args.join(" ").contains(needle))
                .unwrap_or_else(|| panic!("no call containing `{needle}`"))
                .clone()
        };
        for needle in ["ps -a --format", "compose images -q", "compose ps -a -q", "logs --tail"] {
            assert!(
                find(needle).timeout.is_some(),
                "probe `{needle}` must be time-bounded -- a wedged dockerd answers the socket and then never replies"
            );
        }
        for needle in ["clone --progress", "docker-compose.build.yml build", "compose up -d"] {
            assert!(
                find(needle).timeout.is_none(),
                "`{needle}` must NOT be killed on a timer: a first build legitimately runs for hours"
            );
        }
        drop(calls);
        let _ = std::fs::remove_dir_all(&games);

        // The restart-count `inspect` is only reached when the world is NOT
        // ready on the first poll (readiness is checked first and returns), so
        // it needs its own short run — and it is the probe that most needs a
        // bound, being inside the loop.
        let games2 = fixture("probe-bounds-notready");
        let io2 = happy_io().reply("logs --tail", 0, &["still loading maps"]);
        let mut o2 = fast_opts(&games2);
        o2.ready_timeout = Duration::from_millis(120);
        o2.ready_poll = Duration::from_millis(20);
        let _ = run_install(&io2, &o2);
        let c2 = io2.calls.borrow();
        let insp = c2
            .iter()
            .find(|c| c.args.join(" ").contains("inspect -f"))
            .expect("a world that is not ready must have its restart count read");
        assert!(
            insp.timeout.is_some(),
            "the in-loop restart-count probe must be time-bounded"
        );
        drop(c2);
        let _ = std::fs::remove_dir_all(&games2);
    }

    // -- build progress ------------------------------------------------------
    //
    // Every fixture below is a line copied verbatim out of a real build log on
    // this machine (`dml-native/native-test/logs/build-20260731-210436.log` and
    // `dml-uitest/wow-server-playerbots/logs/build-20260801-081450.log`), not a
    // line invented to match the parser.

    #[test]
    fn a_ninja_step_line_parses_to_its_fraction() {
        assert_eq!(
            parse_build_step("#26 782.2 [1803/1808] Building CXX object modules/CMakeFiles/modules.dir/mod-playerbots/src/Mgr/Travel/TravelMgr.cpp.o"),
            Some(BuildStep { vertex: 26, done: 1803, total: 1808 })
        );
        assert_eq!(
            parse_build_step("#26 3.703 [16/1808] Building CXX object deps/fmt/CMakeFiles/fmt.dir/src/os.cc.o"),
            Some(BuildStep { vertex: 26, done: 16, total: 1808 })
        );
    }

    /// THE trap this parser exists to avoid. BuildKit's vertex header carries a
    /// DOCKERFILE STAGE step (`skeleton 2/4`) that looks exactly like progress
    /// and is not: matching it would slam the bar to 50% during a 0.1s `mkdir`
    /// and then walk it back.
    #[test]
    fn a_buildkit_vertex_header_is_not_progress() {
        assert_eq!(
            parse_build_step("#7 [ac-client-data-init skeleton 2/4] RUN mkdir -pv /azerothcore/bin"),
            None
        );
        assert_eq!(
            parse_build_step("#12 [ac-client-data-init client-data 3/3] COPY --chown=acore:acore apps apps"),
            None
        );
    }

    #[test]
    fn buildkit_status_lines_are_not_progress() {
        for line in [
            "#49 DONE 0.0s",
            "#7 CACHED",
            "#14 ...",
            "#6 transferring context: 6.69kB done",
            " Image dml.local/ac-wotlk-worldserver:native-5c541930 Building ",
            "",
        ] {
            assert_eq!(parse_build_step(line), None, "{line:?} must not parse as progress");
        }
    }

    #[test]
    fn progress_is_reported_once_per_change_and_never_backwards() {
        let mut p = BuildProgress::default();
        assert_eq!(p.observe("#26 1.0 [18/1808] Building CXX object a.cpp.o"), Some(0));
        // 181/1808 is 10%, and so is 190/1808 -- one event, not two.
        assert_eq!(p.observe("#26 2.0 [181/1808] Building CXX object b.cpp.o"), Some(10));
        assert_eq!(p.observe("#26 3.0 [190/1808] Building CXX object c.cpp.o"), None);
        // Ninja does not go backwards, but a stale line arriving late must not
        // be able to walk the display back either.
        assert_eq!(p.observe("#26 4.0 [20/1808] Building CXX object d.cpp.o"), None);
        assert_eq!(p.observe("#26 5.0 [1808/1808] Linking CXX executable worldserver"), Some(100));
    }

    /// Four images build in PARALLEL. A three-step sidecar reporting 2/3 must
    /// not shove the display to 66% while the 1808-step compile is at 1%.
    #[test]
    fn a_lesser_vertex_cannot_outbid_the_real_compile() {
        let mut p = BuildProgress::default();
        assert_eq!(p.observe("#26 1.0 [181/1808] Building CXX object a.cpp.o"), Some(10));
        assert_eq!(
            p.observe("#31 1.1 [2/3] Building CXX object side.cpp.o"),
            None,
            "a 3-step vertex must not speak for a 1808-step build"
        );
        assert_eq!(p.observe("#26 2.0 [362/1808] Building CXX object b.cpp.o"), Some(20));
    }

    #[test]
    fn a_zero_total_is_a_malformed_line_not_a_finished_build() {
        let mut p = BuildProgress::default();
        assert_eq!(p.observe("#26 1.0 [0/0] Building CXX object a.cpp.o"), None);
        assert_eq!(p.observe("#26 2.0 [5/10] Building CXX object b.cpp.o"), Some(50));
    }

    /// The parser is only worth having if the ENGINE actually feeds it. This
    /// drives the real `install_native_stream_with` with ninja lines coming
    /// back from the build call, so a `do_build` that stopped emitting would
    /// fail here even with every parser test above still green.
    #[test]
    fn the_build_stage_emits_pct_events_from_the_lines_it_streams() {
        let games = fixture("pct-events");
        let io = happy_io().reply(
            "docker-compose.build.yml build",
            0,
            &[
                "#26 1.0 [18/1808] Building CXX object a.cpp.o",
                "#7 [ac-client-data-init skeleton 2/4] RUN mkdir -pv /azerothcore/bin",
                "#26 2.0 [904/1808] Building CXX object b.cpp.o",
                "#26 3.0 [1808/1808] Linking CXX executable worldserver",
                "#49 DONE 0.0s",
            ],
        );
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");

        // Scoped to the build section. Every stage that can report a number now
        // does, so a global collection here would silently mix in `up`'s.
        let pcts = pcts_in_section(&events, Stage::Build.name());
        assert_eq!(pcts, vec![0, 50, 100], "{events:#?}");

        // Advisory means advisory: the raw lines still reach the terminal
        // unchanged, because the install panel shows the build wall and not a
        // percentage.
        let lines: Vec<String> = events
            .iter()
            .filter(|e| e["event"] == "line")
            .map(|e| e["text"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("[904/1808]")),
            "the build output must still be echoed verbatim: {lines:#?}"
        );

    }

    // -- clone progress ------------------------------------------------------

    #[test]
    fn a_git_progress_redraw_parses_to_its_phase_and_percentage() {
        assert_eq!(
            parse_clone_phase("Receiving objects:  45% (12345/27000), 12.34 MiB | 5.67 MiB/s"),
            Some((ClonePhase::Receiving, 45))
        );
        assert_eq!(
            parse_clone_phase("Resolving deltas: 100% (900000/900000), done."),
            Some((ClonePhase::Resolving, 100))
        );
    }

    /// The SERVER's phases each count 0-100% too, and counting them would make
    /// the bar run to 100 and restart three times before the download even
    /// begins. They are excluded by anchoring the match, which the `remote: `
    /// prefix defeats.
    #[test]
    fn the_servers_own_phases_are_not_our_progress() {
        for line in [
            "remote: Enumerating objects: 1234567, done.",
            "remote: Counting objects: 100% (123/123), done.",
            "remote: Compressing objects:  67% (100/150)",
            "Cloning into '/games/wow-server-playerbots'...",
            "Receiving objects: not-a-number",
            "",
        ] {
            assert_eq!(parse_clone_phase(line), None, "{line:?} must not parse as progress");
        }
    }

    #[test]
    fn the_two_phases_are_one_monotonic_climb_not_two_runs_of_a_hundred() {
        let mut p = CloneProgress::default();
        assert_eq!(p.observe("Receiving objects:   0% (1/27000)"), Some(0));
        assert_eq!(p.observe("Receiving objects:  50% (13500/27000)"), Some(45));
        assert_eq!(p.observe("Receiving objects: 100% (27000/27000), done."), Some(90));
        // Resolving starts its OWN 0-100 here. If it were reported raw the bar
        // would fall from 90 to 0 -- the exact thing users read as a crash.
        assert_eq!(p.observe("Resolving deltas:   0% (0/900000)"), None);
        assert_eq!(p.observe("Resolving deltas:  50% (450000/900000)"), Some(95));
        assert_eq!(p.observe("Resolving deltas: 100% (900000/900000), done."), Some(100));
    }

    #[test]
    fn a_redraw_that_does_not_move_the_number_reports_nothing() {
        let mut p = CloneProgress::default();
        assert_eq!(p.observe("Receiving objects:  10% (2700/27000)"), Some(9));
        // 11% of 90 is still 9 after integer division -- one event, not two.
        assert_eq!(p.observe("Receiving objects:  11% (2970/27000)"), None);
        assert_eq!(p.observe("Receiving objects:  12% (3240/27000)"), Some(10));
    }

    /// Drives the REAL engine: `--progress` on the argv, `pct` events out, and
    /// the terminal filtered to the redraws that moved the number while the
    /// non-progress lines all survive.
    #[test]
    fn the_clone_stage_reports_progress_and_keeps_the_wall_readable() {
        let games = fixture("clone-progress");
        let io = happy_io().reply_seq(
            "clone --progress",
            0,
            &[
                &[
                    "Cloning into '/games/wow-server-playerbots'...",
                    "remote: Counting objects: 100% (123/123), done.",
                    "Receiving objects:   0% (1/27000)",
                    "Receiving objects:  50% (13500/27000)",
                    "Receiving objects:  51% (13770/27000)",
                    "Receiving objects: 100% (27000/27000), done.",
                    "Resolving deltas: 100% (900000/900000), done.",
                ],
                &["Cloning into '/games/modules/mod-playerbots'...", "Receiving objects: 100% (5/5), done."],
            ],
        );
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");

        assert!(
            io.has("clone --progress"),
            "git prints nothing to a pipe without --progress: {:#?}",
            io.log()
        );

        let pcts = pcts_in_section(&events, Stage::CloneCore.name());
        assert_eq!(pcts, vec![0, 45, 90, 100], "{events:#?}");

        let lines: Vec<String> = events
            .iter()
            .filter(|e| e["event"] == "line")
            .map(|e| e["text"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(
            lines.iter().any(|l| l.contains("Cloning into")),
            "real clone output must survive the filter: {lines:#?}"
        );
        assert!(
            !lines.iter().any(|l| l.contains("51% (13770/27000)")),
            "a redraw that did not move the number must not reach the wall: {lines:#?}"
        );
        assert!(
            lines.iter().any(|l| l.contains("50% (13500/27000)")),
            "the redraw that DID move it is worth one line: {lines:#?}"
        );
    }

    // -- compose-up progress -------------------------------------------------

    #[test]
    fn the_up_denominator_comes_from_the_template_that_declares_the_containers() {
        let names = composegen::base_container_names();
        assert_eq!(
            names,
            vec![
                "ac-database".to_string(),
                "ac-db-import".to_string(),
                "ac-client-data-init".to_string(),
                "ac-authserver".to_string(),
                "ac-worldserver".to_string(),
            ],
            "a service added to native-compose.yml.tmpl changes this -- update it deliberately"
        );
    }

    #[test]
    fn a_container_is_counted_once_and_only_on_a_state_that_means_finished() {
        let mut p = UpProgress::new(5);
        // Mid-flight states are not progress: every container passes through
        // several, so counting them would run past 100% and back.
        assert_eq!(p.observe(" Container ac-database  Creating"), None);
        assert_eq!(p.observe(" Container ac-database  Created"), None);
        assert_eq!(p.observe(" Container ac-database  Starting"), None);
        assert_eq!(p.observe(" Container ac-database  Started"), Some(20));
        // Same container reaching another terminal state adds nothing.
        assert_eq!(p.observe(" Container ac-database  Healthy"), None);
        // A one-shot service finishing is SUCCESS, not a stall.
        assert_eq!(p.observe(" Container ac-db-import  Exited"), Some(40));
        assert_eq!(p.observe(" Network ac-network  Created"), None);
    }

    #[test]
    fn an_undeclared_container_cannot_push_the_number_past_a_hundred() {
        let mut p = UpProgress::new(2);
        assert_eq!(p.observe("Container ac-database Started"), Some(50));
        assert_eq!(p.observe("Container ac-worldserver Started"), Some(100));
        assert_eq!(p.observe("Container ac-someone-elses Started"), None);
    }

    #[test]
    fn the_up_stage_emits_pct_from_the_lines_it_streams() {
        let games = fixture("up-progress");
        let io = happy_io().reply(
            "compose up -d",
            0,
            &[
                " Container ac-database  Started",
                " Container ac-db-import  Exited",
                " Container ac-client-data-init  Exited",
                " Container ac-authserver  Started",
                " Container ac-worldserver  Started",
            ],
        );
        let (rc, events) = run_install(&io, &fast_opts(&games));
        assert_eq!(rc, 0, "{events:#?}");

        assert_eq!(pcts_in_section(&events, Stage::Up.name()), vec![20, 40, 60, 80, 100], "{events:#?}");
    }

    #[test]
    fn the_ready_stage_carries_its_ceiling_and_no_percentage() {
        let games = fixture("ready-ceiling");
        let io = happy_io();
        let mut opts = fast_opts(&games);
        opts.ready_timeout = Duration::from_secs(1800);
        let (_rc, events) = run_install(&io, &opts);

        let ready = events
            .iter()
            .find(|e| e["event"] == "section_start" && e["name"] == Stage::Ready.name())
            .unwrap_or_else(|| panic!("no ready section: {events:#?}"));
        assert_eq!(ready["limit_secs"].as_u64(), Some(1800), "{ready:#?}");

        // Every OTHER section stays a bare section_start -- the ceiling is not a
        // field every stage suddenly has to answer for.
        let build = events
            .iter()
            .find(|e| e["event"] == "section_start" && e["name"] == Stage::Build.name())
            .unwrap();
        assert!(build["limit_secs"].is_null(), "{build:#?}");

        // A WAIT has no denominator, so it must never report one.
        assert!(
            pcts_in_section(&events, Stage::Ready.name()).is_empty(),
            "the ready wait must not report a percentage: {events:#?}"
        );
    }

    // -- the real adapter ----------------------------------------------------

    #[test]
    fn proc_io_really_spawns_a_process_and_streams_its_output() {
        // The fake proves the ORCHESTRATION; this proves the seam's production
        // implementation is a real spawn and not a stub that returns Ok.
        #[cfg(windows)]
        let (prog, args) = ("cmd", vec!["/c".to_string(), "echo PROCIO-EVIDENCE".to_string()]);
        #[cfg(not(windows))]
        let (prog, args) = ("sh", vec!["-c".to_string(), "echo PROCIO-EVIDENCE".to_string()]);

        let io = ProcIo { docker: OsString::from(prog), git: OsString::from(prog) };
        let mut seen: Vec<String> = Vec::new();
        let outcome = io.run(
            &Call { program: Program::Docker, args, cwd: None, timeout: None },
            &mut |l| seen.push(l.to_string()),
        );
        assert_eq!(outcome, RunOutcome::Exited(0), "{seen:?}");
        assert!(seen.iter().any(|l| l.contains("PROCIO-EVIDENCE")), "{seen:?}");

        let missing = ProcIo {
            docker: OsString::from("dml-no-such-program-9f8e7d"),
            git: OsString::from("dml-no-such-program-9f8e7d"),
        };
        let outcome = missing.run(
            &Call { program: Program::Docker, args: vec!["--version".to_string()], cwd: None, timeout: None },
            &mut |_| {},
        );
        assert!(
            matches!(outcome, RunOutcome::CouldNotTell(_)),
            "a spawn failure is a could-not-tell, never an exit code: {outcome:?}"
        );
    }
}
