//! Native Docker backend (spike: `spike/docker-desktop-native`).
//!
//! WHY THIS EXISTS. Today the launcher talks to the server through one seam:
//! `runner.rs` shells `wsl.exe -d dml-arch -u dml -- dml <cmd> --json`, and the
//! ~7000-line bash `dml` program *inside* the hand-built `dml-arch` distro is
//! what actually drives `docker compose`. The DML team wants to drop that
//! hand-built distro and let Docker Desktop manage its own Linux VM (it already
//! registers its own `docker-desktop` WSL distro the moment its engine starts).
//!
//! This module is the launcher-side half of that move: a backend that drives
//! `docker compose` DIRECTLY on the Windows host — no `dml-arch`, no bash
//! middleman. It is deliberately self-contained and NOT yet wired into the live
//! Tauri command surface: the WSL runner still owns every real feature. Swapping
//! the app over is the larger port (re-hosting the bash orchestration), which is
//! the shared-DML decision, not this launcher's. What this proves is that the
//! launcher CAN own a game's container lifecycle against Docker Desktop.
//!
//! See `poc/native-docker/` for the compose file this drives and the findings
//! write-up.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use dml_core::error::CmdError;

/// Docker discovery, `docker compose ps` parsing, and the native-mode Docker
/// Desktop engine lifecycle (poll/launch/stop) — moved to `dml_core::engine`
/// (cargo-workspace refactor, Task 6). `docker_program`/`docker_desktop_program`
/// had already moved to their own `dml_core::docker` module in Task 3; that
/// module folds into `dml_core::engine` as part of this move (one module, not
/// two). Re-exported here so every existing `native::X` call site in
/// `lib.rs`/`maint.rs` keeps compiling unchanged.
pub use dml_core::engine::{
    docker_desktop_program, docker_desktop_start_args, docker_desktop_stop_args, docker_info_args,
    docker_program, engine_presence, engine_running, ensure_decision, game_state, launch_detached,
    parse_ps_json, poll_until_ready, start_engine, start_engine_succeeded, stop_engine,
    stop_engine_enabled, EnginePresence, EnsureDecision, PollOutcome, PsRow,
    ENGINE_POLL_INTERVAL_MS, ENGINE_POLL_TIMEOUT_MS,
};

/// PATH for the docker child: the docker executable's own directory (which
/// also holds the credential helpers docker invokes by bare name) prepended to
/// the current PATH. Returns `None` when `program` has no directory component
/// (a bare `docker` resolved off PATH already has its bin dir on PATH, helpers
/// included), so the caller leaves the inherited PATH untouched.
fn augmented_path(program: &OsStr, current: Option<OsString>) -> Option<OsString> {
    let dir = Path::new(program).parent()?;
    if dir.as_os_str().is_empty() {
        return None;
    }
    let mut paths = vec![dir.to_path_buf()];
    if let Some(cur) = current {
        paths.extend(std::env::split_paths(&cur));
    }
    std::env::join_paths(paths).ok()
}

/// A single Compose project (one "game") driven natively on the Windows host.
///
/// `project_dir` holds the `docker-compose.yml`; `project_name` is passed as
/// `-p` so `ps`/`down` scope to exactly this stack and never collide with
/// another game's containers.
pub struct NativeDocker {
    pub program: OsString,
    pub project_dir: PathBuf,
    pub project_name: String,
}

impl NativeDocker {
    pub fn new(project_dir: impl Into<PathBuf>, project_name: impl Into<String>) -> Self {
        NativeDocker {
            program: docker_program(),
            project_dir: project_dir.into(),
            project_name: project_name.into(),
        }
    }

    /// The full `docker compose` argument vector for a subcommand, with the
    /// project name pinned. Split out from `command` so it can be asserted in
    /// tests without spawning anything.
    fn compose_args(&self, sub: &[&str]) -> Vec<String> {
        let mut v = vec![
            "compose".to_string(),
            "-p".to_string(),
            self.project_name.clone(),
        ];
        v.extend(sub.iter().map(|s| s.to_string()));
        v
    }

    fn command(&self, sub: &[&str]) -> Command {
        let mut cmd = Command::new(&self.program);
        cmd.args(self.compose_args(sub));
        // Run from the project dir so Compose finds docker-compose.yml the same
        // way the bash CLI does (`cd "$compose_dir"` before every compose call).
        cmd.current_dir(&self.project_dir);
        // docker shells out to a credential helper (docker-credential-desktop
        // .exe) even for anonymous Docker Hub pulls; it lives next to docker.exe
        // in the Docker Desktop bin dir, which is NOT on the machine PATH for a
        // per-user install. Put that dir on the child's PATH or the very first
        // `up` fails with "docker-credential-desktop: executable file not
        // found" before pulling a single layer. (Found the hard way in the live
        // PoC run.)
        if let Some(p) = augmented_path(&self.program, std::env::var_os("PATH")) {
            cmd.env("PATH", p);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    /// `docker compose up -d` — bring the stack up detached.
    pub fn up(&self) -> std::io::Result<std::process::Output> {
        self.command(&["up", "-d"]).output()
    }

    /// `docker compose down` — stop and remove the stack.
    pub fn down(&self) -> std::io::Result<std::process::Output> {
        self.command(&["down"]).output()
    }

    /// Parsed `docker compose ps` rows for this project. Compose can emit the
    /// `--format json` payload as either a single JSON array or one JSON object
    /// per line (NDJSON) depending on version — accept both.
    ///
    /// A non-zero docker exit is an ERROR, not an empty list: when the Docker
    /// Desktop engine is down (the most common failure), `compose ps` exits
    /// non-zero with empty stdout — swallowing that would make `status()`
    /// report a clean "stopped" and offer Start when the truth is unknown
    /// (review finding, 2026-07-24).
    pub fn ps(&self) -> std::io::Result<Vec<PsRow>> {
        let out = self.command(&["ps", "--format", "json"]).output()?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!(
                    "docker compose ps failed (exit {}): {}",
                    out.status.code().unwrap_or(-1),
                    stderr.trim()
                ),
            ));
        }
        let text = String::from_utf8_lossy(&out.stdout);
        Ok(parse_ps_json(&text))
    }

    /// Normalized lifecycle state for the whole stack, matching the string the
    /// WSL path's `games status` returns so a caller can treat both backends
    /// alike: `"running"` if any service container is up, else `"stopped"`.
    pub fn status(&self) -> std::io::Result<&'static str> {
        Ok(game_state(&self.ps()?))
    }

    /// `docker compose up -d`, aliased to the lifecycle verb the launcher uses.
    pub fn start(&self) -> std::io::Result<std::process::Output> {
        self.up()
    }

    /// `docker compose down`, aliased to the lifecycle verb the launcher uses.
    pub fn stop(&self) -> std::io::Result<std::process::Output> {
        self.down()
    }
}

// ---------------------------------------------------------------------------
// Docker Desktop engine lifecycle around start/stop — moved out of the
// launcher's `lib.rs` by the cargo-workspace refactor (Task 9).
//
// In native mode the Docker Desktop engine (and its docker-desktop WSL VM)
// must be up before any `docker compose` runs, so a cold `games start`
// ensures it first; and stopping it on `games stop` frees the VM's RAM, so
// the caller shuts it down afterwards when the (default-on) manage-docker
// toggle is set. The pure decision/poll logic lives in `dml_core::engine`
// (re-exported above); these two supply the real docker spawns, wall-clock
// sleeps, and the NDJSON progress stream (envelope `line`/`error` events, the
// same shape `dml games start/stop` emits). Blocking — the Tauri/CLI caller
// runs them off the async runtime.
// ---------------------------------------------------------------------------

/// Emit engine-lifecycle progress as an envelope `line` event onto the same
/// stream the games output uses, so the UI terminal shows it inline.
/// Section name for the Docker-engine phase of a native stop. Any name works
/// EXCEPT "output", which is the one the UI reducer fabricates for section-less
/// lines and which nothing is allowed to close by name.
const ENGINE_SECTION: &str = "docker-engine";

fn engine_line(emit: &impl Fn(serde_json::Value), level: &str, text: impl Into<String>) {
    emit(serde_json::json!({"event": "line", "level": level, "text": text.into()}));
}

/// Native-mode prerequisite for any start: make sure the Docker Desktop engine
/// is up before compose runs. Emits progress; on an unrecoverable failure it
/// emits a terminal `error` event AND returns Err so the caller ABORTS instead
/// of composing against a dead engine. Blocking (real spawns + sleeps) — run
/// under `spawn_blocking`.
pub fn ensure_engine_up_stream(emit: impl Fn(serde_json::Value)) -> Result<(), CmdError> {
    match dml_core::engine::EngineKind::for_this_host() {
        dml_core::engine::EngineKind::Systemd => ensure_engine_up_systemd(emit),
        dml_core::engine::EngineKind::Desktop => ensure_engine_up_desktop(emit),
    }
}

/// THE FIX FOR "start refuses inside the distro" (R1).
///
/// `Cmd::Start` calls the function above unconditionally, and before this it
/// went straight to the Docker Desktop branch: `docker_desktop_program()` finds
/// nothing on Linux, `ensure_decision` returns `NoDesktop`, and the arm emitted
/// a terminal `DOCKER_DESKTOP_MISSING` and ABORTED BEFORE COMPOSE. With the
/// distro's `dockerd` down — the one situation the prerequisite exists for —
/// the single operation the whole product is about failed, and the error named
/// a product that has no business being on that machine.
///
/// Inside `dml-arch` the engine is a systemd unit and the correct action is
/// `sudo -n systemctl start docker`.
fn ensure_engine_up_systemd(emit: impl Fn(serde_json::Value)) -> Result<(), CmdError> {
    use dml_core::engine as eng;
    ensure_engine_up_systemd_with(
        &crate::native::docker_program(),
        eng::start_engine_systemd,
        eng::ENGINE_POLL_INTERVAL_MS,
        eng::ENGINE_POLL_TIMEOUT_MS,
        emit,
    )
}

/// The injectable half. The same reasoning as [`stop_engine_stream_with`], and
/// the same file's own recorded lesson: a test that called the real thing here
/// would shell `sudo -n systemctl start docker` on the developer's machine, and
/// "just don't run it" is not a safeguard. It also lets the 180-second poll be
/// driven to its timeout in a millisecond.
fn ensure_engine_up_systemd_with(
    program: &std::ffi::OsStr,
    start: impl FnOnce() -> std::io::Result<std::process::Output>,
    interval_ms: u64,
    timeout_ms: u64,
    emit: impl Fn(serde_json::Value),
) -> Result<(), CmdError> {
    use dml_core::engine as eng;
    use dml_core::setup::Tri;

    // TRI-STATE, NOT A BOOL, and the difference is kept where it means
    // something. `engine_running` (bool) reads a probe that fell over as "not
    // running"; `engine_running_tri` keeps "docker answered no" apart from
    // "docker did not answer".
    match eng::engine_running_tri(program) {
        Tri::Yes => {
            engine_line(&emit, "info", "Docker engine already running.");
            return Ok(());
        }
        Tri::No => {
            engine_line(&emit, "info", "Docker engine is down. Starting docker.service...");
        }
        Tri::Unknown => {
            // Evidence of nothing. Refusing here would block a start because a
            // probe was slow; claiming it is up would compose against a dead
            // engine. `systemctl start` on an already-active unit is a no-op,
            // so asking is the safe move — but the operator is TOLD which of
            // the two situations this is, rather than being shown the same
            // line for both.
            engine_line(
                &emit,
                "warn",
                "Could not determine whether the Docker engine is running; \
                 asking systemd to start it anyway (harmless if it already is).",
            );
        }
    }

    // A FAILED START IS REPORTED, not discarded. The Tailscale round taught
    // this one on real hardware: a sudo refusal, a missing unit, a dying daemon
    // and a merely-slow one all produced the same useless message. `sudo -n`
    // means a distro without a NOPASSWD rule fails IMMEDIATELY and says so,
    // instead of blocking on a password prompt no button can answer.
    let started = start();
    let mut cause = String::new();
    if !eng::start_engine_succeeded(&started) {
        cause = match &started {
            Ok(out) => {
                let stderr = String::from_utf8_lossy(&out.stderr);
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    format!("sudo -n systemctl start docker exited {}", out.status.code().unwrap_or(-1))
                } else {
                    stderr.to_string()
                }
            }
            Err(e) => format!("could not run sudo -n systemctl start docker: {e}"),
        };
        engine_line(&emit, "warn", format!("Could not start docker.service: {cause}"));
    }

    // Poll regardless: the start may have failed only because the unit was
    // already active, or because another process won the race.
    let outcome = eng::poll_until_ready(
        interval_ms,
        timeout_ms,
        || eng::engine_running(program),
        |ms| {
            engine_line(&emit, "info", "Waiting for the Docker engine to be ready...");
            std::thread::sleep(std::time::Duration::from_millis(ms));
        },
    );
    match outcome {
        eng::PollOutcome::Ready { .. } => {
            engine_line(&emit, "info", "Docker engine is ready.");
            Ok(())
        }
        eng::PollOutcome::Timeout { waited_ms } => {
            let msg = format!("The Docker engine did not become ready within {}s.", waited_ms / 1000);
            // The start failure, if there was one, is carried INTO the timeout:
            // the cause was known 3 minutes ago and must not be lost by the
            // time the user reads the error.
            let hint = if cause.is_empty() {
                "Check the daemon inside the distro: wsl -d dml-arch -u dml --exec sudo systemctl status docker".to_string()
            } else {
                format!("The start command failed first: {cause}")
            };
            emit(serde_json::json!({"event": "error", "error": {
                "code": "DOCKER_ENGINE_TIMEOUT", "message": msg, "hint": hint,
            }}));
            Err(CmdError { code: "DOCKER_ENGINE_TIMEOUT".into(), message: msg, hint })
        }
    }
}

/// The Docker Desktop branch, exactly as it always was.
fn ensure_engine_up_desktop(emit: impl Fn(serde_json::Value)) -> Result<(), CmdError> {
    use crate::native;
    let program = native::docker_program();
    let desktop = native::docker_desktop_program();
    let probe_program = program.clone();
    let start_program = program.clone();
    let exe = desktop.clone();
    ensure_engine_up_stream_with(
        || native::engine_presence(&probe_program),
        desktop.is_some(),
        || native::start_engine_succeeded(&native::start_engine(&start_program)),
        || {
            // Only ever reached on the `Launch` arm, which implies a resolved
            // exe -- the same invariant (and the same message) the inline
            // version carried.
            let e = exe.as_ref().expect("Launch decision implies a resolved desktop exe");
            native::launch_detached(e)
        },
        |ms| std::thread::sleep(std::time::Duration::from_millis(ms)),
        emit,
    )
}

/// The code for "there is no docker CLI on this machine", which is a DIFFERENT
/// repair from [`DOCKER_DESKTOP_MISSING`](ensure_engine_up_stream) and from a
/// readiness timeout, and must not be reported as either.
pub const CODE_DOCKER_CLI_MISSING: &str = "DOCKER_CLI_MISSING";

/// The injectable half of [`ensure_engine_up_stream`], so the WIRING is
/// testable and not merely the pure decision table underneath it.
///
/// Every impure thing this flow does is a parameter: what the CLI says
/// (`presence`), whether a GUI exe was found (`desktop_found`), asking the CLI
/// to start the engine (`start_via_cli`), launching the GUI (`launch_exe`), and
/// the wait between readiness checks (`sleep`).
///
/// WHY A SEAM AND NOT A WALL-CLOCK TEST. The invariant worth protecting here is
/// "the readiness wait is never ENTERED when it cannot succeed", and a test
/// that measured elapsed time would be asserting the wrong thing — it would
/// pass on a fast machine for reasons unrelated to the guard, and this repo has
/// already recorded a wall-clock pin that stayed green with its bug deliberately
/// reinstated. With `sleep` injected, "did we poll?" is a COUNTER: exact, and
/// red the instant the guard is removed. Same shape, and same reason, as
/// `stop_engine_stream_with` below.
#[allow(clippy::too_many_arguments)]
pub fn ensure_engine_up_stream_with(
    mut presence: impl FnMut() -> dml_core::engine::EnginePresence,
    desktop_found: bool,
    mut start_via_cli: impl FnMut() -> bool,
    mut launch_exe: impl FnMut() -> std::io::Result<()>,
    mut sleep: impl FnMut(u64),
    emit: impl Fn(serde_json::Value),
) -> Result<(), CmdError> {
    use crate::native;
    use dml_core::engine::EnginePresence;
    match native::ensure_decision(presence(), desktop_found) {
        native::EnsureDecision::AlreadyUp => {
            engine_line(&emit, "info", "Docker Desktop engine already running.");
            Ok(())
        }
        // NOTHING IS LAUNCHED AND NOTHING IS WAITED FOR. The readiness probe
        // runs through the docker CLI, so with no CLI the 180-second wait below
        // is dead time by construction: 61 checks, every one false because the
        // program cannot be spawned, then the same refusal that was available
        // before the first tick. Measured on the offending CI test: 184s, of
        // which 180 were spent here. See `ensure_decision`.
        native::EnsureDecision::NoDockerCli => {
            let msg = "The docker command was not found, so the Docker engine cannot be started.";
            let hint = "Install Docker Desktop, or set DML_DOCKER to the full path of docker.exe.";
            emit(serde_json::json!({"event": "error", "error": {
                "code": CODE_DOCKER_CLI_MISSING, "message": msg, "hint": hint,
            }}));
            Err(CmdError {
                code: CODE_DOCKER_CLI_MISSING.into(),
                message: msg.into(),
                hint: hint.into(),
            })
        }
        native::EnsureDecision::NoDesktop => {
            let msg = "Docker engine is down and Docker Desktop.exe was not found; \
                       cannot start the engine.";
            let hint = "Install Docker Desktop, or set DML_DOCKER_DESKTOP to its exe.";
            emit(serde_json::json!({"event": "error", "error": {
                "code": "DOCKER_DESKTOP_MISSING", "message": msg, "hint": hint,
            }}));
            Err(CmdError { code: "DOCKER_DESKTOP_MISSING".into(), message: msg.into(), hint: hint.into() })
        }
        native::EnsureDecision::Launch => {
            engine_line(&emit, "info", "Docker engine is down. Starting Docker Desktop...");
            // Ask the CLI for the ENGINE first: launching the GUI exe pops the
            // Docker dashboard window over whatever the user was doing, every
            // time the server starts with the engine down, and we never wanted
            // the dashboard. `docker desktop start -d` starts the engine with no
            // window. The `docker desktop` plugin only exists from Docker
            // Desktop 4.37, so a failure is NOT an error -- it falls back to the
            // exe, which is exactly the behaviour that shipped before this.
            if !start_via_cli() {
                if let Err(e) = launch_exe() {
                    let msg = format!("Failed to launch Docker Desktop: {e}");
                    emit(serde_json::json!({"event": "error", "error": {
                        "code": "DOCKER_DESKTOP_LAUNCH", "message": msg, "hint": "",
                    }}));
                    return Err(CmdError { code: "DOCKER_DESKTOP_LAUNCH".into(), message: msg, hint: String::new() });
                }
            }
            let outcome = native::poll_until_ready(
                native::ENGINE_POLL_INTERVAL_MS,
                native::ENGINE_POLL_TIMEOUT_MS,
                || presence() == EnginePresence::Up,
                |ms| {
                    engine_line(&emit, "info", "Waiting for Docker Desktop to be ready...");
                    sleep(ms);
                },
            );
            match outcome {
                native::PollOutcome::Ready { .. } => {
                    engine_line(&emit, "info", "Docker Desktop engine is ready.");
                    Ok(())
                }
                native::PollOutcome::Timeout { waited_ms } => {
                    let msg = format!(
                        "Docker Desktop did not become ready within {}s.",
                        waited_ms / 1000
                    );
                    let hint = "Start Docker Desktop manually, wait for it to finish, then retry.";
                    emit(serde_json::json!({"event": "error", "error": {
                        "code": "DOCKER_ENGINE_TIMEOUT", "message": msg, "hint": hint,
                    }}));
                    Err(CmdError { code: "DOCKER_ENGINE_TIMEOUT".into(), message: msg, hint: hint.into() })
                }
            }
        }
    }
}

/// Best-effort `docker desktop stop` after a native stop: stops the engine +
/// its docker-desktop WSL VM to free RAM. A failure emits a warning `line` but
/// never fails the server-stop. Blocking — run under `spawn_blocking`.
pub fn stop_engine_stream(emit: impl Fn(serde_json::Value)) {
    stop_engine_stream_with_kind(
        dml_core::engine::EngineKind::for_this_host(),
        &crate::native::docker_program(),
        emit,
    )
}

/// R2's other half, closed at the root rather than only in the translation.
///
/// Stopping "the engine" after a server stop is a Docker Desktop idea: it frees
/// the docker-desktop WSL VM's RAM. Inside `dml-arch` the daemon is the
/// distro's own shared `dockerd` — nothing else reclaims RAM by stopping it,
/// and anything else using that engine loses it. Today `docker desktop stop`
/// merely fails there with a warning; the day that plugin resolves on Linux it
/// would take the distro's daemon down behind the user's back.
///
/// The launcher's Arch translation also appends `--no-stop-engine`, so this is
/// the belt to that braces — it covers `dml-wow stop` typed by hand in the
/// distro, which no translation sees.
pub fn stop_engine_stream_with_kind(
    kind: dml_core::engine::EngineKind,
    program: &std::ffi::OsStr,
    emit: impl Fn(serde_json::Value),
) {
    if kind == dml_core::engine::EngineKind::Systemd {
        emit(serde_json::json!({"event": "section_start", "name": ENGINE_SECTION}));
        engine_line(
            &emit,
            "info",
            "Leaving the Docker engine running: inside the distro it is a shared \
             system daemon, not a per-server VM.",
        );
        emit(serde_json::json!({"event": "section_end", "name": ENGINE_SECTION, "status": "ok"}));
        return;
    }
    stop_engine_stream_with(program, emit)
}

/// The injectable half, so a test can exercise the event shape WITHOUT stopping
/// the developer's real Docker Desktop. The first version of the test below did
/// exactly that -- it called the real thing, took the engine down mid-session
/// and destroyed the running containers (2026-07-31). A unit test must not be
/// able to do that, and "just don't run it" is not a safeguard.
pub fn stop_engine_stream_with(program: &std::ffi::OsStr, emit: impl Fn(serde_json::Value)) {
    use crate::native;
    // A REAL section, not bare lines. These lines are emitted AFTER the stop
    // lifecycle's terminal `done` event, and the UI reducer fabricates an
    // implicit "output" section for any line arriving outside one -- a section
    // nothing can ever close, because no emitter in this repo sends
    // `section_end{name:"output"}`. On the start path the terminal event closes
    // it (see terminal-state.ts's `done` arm); here the terminal event has
    // ALREADY passed, so the reducer cannot help and the spinner would turn
    // forever after a successful stop. Closing our own section is the fix.
    emit(serde_json::json!({"event": "section_start", "name": ENGINE_SECTION}));
    engine_line(&emit, "info", "Stopping Docker Desktop...");
    match native::stop_engine(program) {
        Ok(out) if out.status.success() => {
            engine_line(&emit, "info", "Docker Desktop stopped.");
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            engine_line(
                &emit,
                "warn",
                format!(
                    "Could not stop Docker Desktop (exit {}): {}",
                    out.status.code().unwrap_or(-1),
                    stderr.trim()
                ),
            );
        }
        Err(e) => {
            engine_line(&emit, "warn", format!("Could not stop Docker Desktop: {e}"));
        }
    }
    // Always "ok": stopping the engine is best-effort by contract and never
    // fails the server-stop, so a warn line inside is not a failed section.
    emit(serde_json::json!({"event": "section_end", "name": ENGINE_SECTION, "status": "ok"}));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_args_pin_the_project_name() {
        let nd = NativeDocker {
            program: "docker".into(),
            project_dir: PathBuf::from("."),
            project_name: "dml-poc".into(),
        };
        assert_eq!(
            nd.compose_args(&["up", "-d"]),
            vec!["compose", "-p", "dml-poc", "up", "-d"]
        );
        assert_eq!(
            nd.compose_args(&["ps", "--format", "json"]),
            vec!["compose", "-p", "dml-poc", "ps", "--format", "json"]
        );
    }

    /// WINDOWS-ONLY: the drive-letter program path, the `;` PATH separator and
    /// `join_paths`' rules are all platform-specific — on Linux
    /// `C:\Windows;C:\…` is a single entry containing `:`, which
    /// `join_paths` rejects outright. The POSIX equivalent is below.
    #[cfg(windows)]
    #[test]
    fn augmented_path_prepends_docker_bin_dir() {
        let got = augmented_path(
            OsStr::new(r"C:\Users\me\AppData\Local\Programs\DockerDesktop\resources\bin\docker.exe"),
            Some(OsString::from(r"C:\Windows;C:\Windows\System32")),
        )
        .unwrap();
        let s = got.to_string_lossy();
        assert!(s.starts_with(r"C:\Users\me\AppData\Local\Programs\DockerDesktop\resources\bin"));
        assert!(s.contains(r"C:\Windows"));
    }

    #[cfg(not(windows))]
    #[test]
    fn augmented_path_prepends_docker_bin_dir() {
        let got = augmented_path(
            OsStr::new("/opt/docker/resources/bin/docker"),
            Some(OsString::from("/usr/bin:/bin")),
        )
        .unwrap();
        let s = got.to_string_lossy();
        assert!(s.starts_with("/opt/docker/resources/bin"), "s={s}");
        assert!(s.contains("/usr/bin"), "s={s}");
    }

    #[test]
    fn augmented_path_is_none_for_bare_docker() {
        // A bare `docker` (no directory) means PATH already resolves it and its
        // sibling helpers — don't rewrite PATH.
        assert!(augmented_path(OsStr::new("docker"), Some(OsString::from(r"C:\Windows"))).is_none());
    }
}

#[cfg(test)]
mod engine_section_tests {
    use std::sync::{Arc, Mutex};

    /// The stop path emits its engine lines AFTER the lifecycle's terminal
    /// `done`, so the UI reducer cannot close them -- see the comment on
    /// `stop_engine_stream`. They must therefore arrive inside a section this
    /// function closes itself, and that section must NOT be called "output"
    /// (the reducer fabricates that name for section-less lines and nothing may
    /// close it by name).
    ///
    /// Driven through `stop_engine_stream_with` against a binary that does not
    /// exist, so it takes the Err arm and emits the warn line. The FIRST version
    /// of this test called `stop_engine_stream` directly and stopped the real
    /// Docker Desktop mid-session, destroying the running containers. The seam
    /// is not a nicety.
    #[test]
    fn the_engine_stop_wraps_its_lines_in_a_section_it_closes_itself() {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let fake = std::ffi::OsString::from("dml-no-such-docker-binary-ever.exe");
        super::stop_engine_stream_with(&fake, move |v| sink.lock().unwrap().push(v));

        let events = seen.lock().unwrap().clone();
        let first = events.first().expect("at least one event");
        assert_eq!(first["event"], "section_start");
        assert_eq!(first["name"], super::ENGINE_SECTION);

        let last = events.last().expect("at least one event");
        assert_eq!(last["event"], "section_end");
        assert_eq!(last["name"], super::ENGINE_SECTION);
        assert_eq!(last["status"], "ok", "best-effort by contract, so never a failed section");

        assert_ne!(super::ENGINE_SECTION, "output", "that name is unclosable by contract");
        // Every line sits between the two markers, so none can leak into the
        // reducer's implicit section...
        assert!(
            events[1..events.len() - 1].iter().all(|e| e["event"] == "line"),
            "unexpected event between the section markers: {events:?}"
        );
        // ...and the run really reached the failure arm, so this is not passing
        // because nothing happened.
        assert!(
            events.iter().any(|e| e["level"] == "warn"),
            "expected the missing-binary warn line: {events:?}"
        );
    }

    // -- the 180-second dead wait (CI blocker, 2026-08-05) --------------------

    /// One run of [`ensure_engine_up_stream_with`] against fakes, with every
    /// impure step COUNTED. The counters are the whole point: "did we enter the
    /// readiness wait?" is then an exact integer, not an elapsed time.
    struct EngineRun {
        result: Result<dml_core::error::CmdError, ()>,
        probes: u32,
        starts: u32,
        launches: u32,
        sleeps: u32,
        events: Vec<serde_json::Value>,
    }

    /// Drive the flow with a scripted sequence of presence answers. The LAST
    /// entry is repeated forever, so a test that expects the wait to run out
    /// does not have to script 61 identical replies.
    fn run_engine(script: &[dml_core::engine::EnginePresence], desktop_found: bool, cli_starts: bool) -> EngineRun {
        use std::cell::{Cell, RefCell};
        let probes = Cell::new(0u32);
        let starts = Cell::new(0u32);
        let launches = Cell::new(0u32);
        let sleeps = Cell::new(0u32);
        let events = RefCell::new(Vec::new());

        let result = super::ensure_engine_up_stream_with(
            || {
                let n = probes.get();
                probes.set(n + 1);
                script[(n as usize).min(script.len() - 1)]
            },
            desktop_found,
            || {
                starts.set(starts.get() + 1);
                cli_starts
            },
            || {
                launches.set(launches.get() + 1);
                Ok(())
            },
            |_ms| sleeps.set(sleeps.get() + 1),
            |v| events.borrow_mut().push(v),
        );

        EngineRun {
            result: match result {
                Ok(()) => Err(()),
                Err(e) => Ok(e),
            },
            probes: probes.get(),
            starts: starts.get(),
            launches: launches.get(),
            sleeps: sleeps.get(),
            events: events.into_inner(),
        }
    }

    /// THE CI BLOCKER, pinned deterministically.
    ///
    /// `install-native`'s preflight refusal took 184 SECONDS, of which 180 were
    /// spent here: the docker CLI could not be spawned at all, that arrived at
    /// `ensure_decision` as a plain "engine is down", and the flow launched the
    /// Docker Desktop GUI and then polled `docker info` — through the CLI that
    /// does not exist — 61 times, 3 seconds apart, for an answer that was false
    /// by construction on every tick. It then refused with exactly the message
    /// that was available before the first tick. That single test wedged
    /// `cargo test --workspace`, and with it both CI jobs.
    ///
    /// `desktop_found: true` is the load-bearing part of the setup, not
    /// incidental: it is the real machine's shape (Docker Desktop installed,
    /// the docker CLI unreachable), and it is what makes this a test of
    /// `CliMissing` OUTRANKING a present GUI rather than a restatement of the
    /// pre-existing `NoDesktop` arm.
    ///
    /// NOT a wall-clock test, deliberately. This repo has already recorded a
    /// timing pin that stayed green with its bug reinstated
    /// (`a_deadline_bounds_the_call_even_when_a_grandchild_holds_the_pipes`),
    /// because the failure it chased needed a race nothing could force. Here
    /// the sleep is injected, so "we never waited" is `sleeps == 0` — exact,
    /// instant, and red the moment the guard is removed.
    #[test]
    fn a_missing_docker_cli_is_refused_without_entering_the_readiness_wait() {
        use dml_core::engine::EnginePresence;
        let run = run_engine(&[EnginePresence::CliMissing], true, true);

        // THE COUNTERS COME FIRST, and the order is deliberate. These are the
        // invariant; the error code below is only how it is reported. Asserting
        // the code first would let a future "fix" that merely renamed the
        // timeout satisfy this test while the 180-second wait stayed exactly
        // where it was -- pinning the symptom instead of the bug.
        assert_eq!(run.sleeps, 0, "the readiness wait must never be ENTERED: it cannot succeed behind a CLI that does not exist");
        assert_eq!(run.launches, 0, "nothing may be launched to satisfy an engine we cannot then talk to");
        assert_eq!(run.starts, 0, "`docker desktop start` runs through the same absent CLI");
        assert_eq!(run.probes, 1, "asked exactly once, never polled");

        let err = run.result.expect("a missing docker CLI must be a refusal, not a success");
        assert_eq!(
            err.code,
            super::CODE_DOCKER_CLI_MISSING,
            "a missing CLI is its own repair -- not a readiness timeout, not a missing GUI"
        );

        // The refusal is a terminal error event, so the UI ends the section
        // instead of spinning -- and it names the CLI, not the GUI.
        let last = run.events.last().expect("a terminal event");
        assert_eq!(last["event"], "error");
        assert_eq!(last["error"]["code"], super::CODE_DOCKER_CLI_MISSING);
        assert!(
            last["error"]["hint"].as_str().unwrap_or("").contains("DML_DOCKER"),
            "the hint must point at the CLI override: {last}"
        );
    }

    /// The other half, and the reason the guard cannot be "fixed" by simply
    /// never waiting: an engine that is merely DOWN must still be started and
    /// still be waited for.
    ///
    /// Without this, deleting the readiness poll outright would leave the test
    /// above green while breaking the feature it guards — the over-fix that the
    /// counter shape above makes so easy to reach for.
    #[test]
    fn an_engine_that_is_merely_down_is_still_started_and_waited_for() {
        use dml_core::engine::EnginePresence;
        // Down at the decision, down on the first readiness check, up on the
        // second -- one sleep, then ready.
        let run = run_engine(
            &[EnginePresence::Down, EnginePresence::Down, EnginePresence::Up],
            true,
            false, // the CLI plugin is absent (pre-4.37), so the exe is launched
        );

        assert!(run.result.is_err(), "a reachable engine that came up is a success: {:?}", run.result);
        assert_eq!(run.starts, 1, "the CLI is still asked for the engine first");
        assert_eq!(run.launches, 1, "a failed CLI ask still falls back to the GUI exe");
        assert_eq!(run.sleeps, 1, "it really waited -- the poll is intact");
        assert!(
            run.events.iter().any(|e| e["text"].as_str().unwrap_or("").contains("ready")),
            "the ready line must still be emitted: {:?}", run.events
        );
    }

    /// A `CouldNotTell` probe must NOT be promoted to the definitive negative.
    /// An engine that is slow to answer is the case the wait exists for, and
    /// refusing it would be a worse bug than the one being fixed here — so the
    /// guard is asserted to be narrow, at the real call site.
    #[test]
    fn a_docker_that_merely_did_not_answer_is_still_waited_for() {
        use dml_core::engine::EnginePresence;
        // `engine_presence` maps a timed-out probe to `Down`, so this is the
        // shape a wedged dockerd arrives in.
        let run = run_engine(&[EnginePresence::Down], true, true);

        let err = run.result.expect("never ready means a refusal");
        assert_eq!(err.code, "DOCKER_ENGINE_TIMEOUT", "a slow engine times out; it is not a missing CLI");
        assert!(run.sleeps > 0, "the wait must actually run for an engine that might still come up");
    }

    /// R2 at the root: inside the distro the daemon is a SHARED system service,
    /// so a server stop must leave it alone. Driven through the kind-taking
    /// seam, so it asserts the Systemd behaviour on a Windows build too — the
    /// bug it guards against is one only a Linux host can actually suffer.
    #[test]
    fn a_systemd_engine_is_never_stopped_by_a_server_stop() {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        // A program that WOULD succeed if it were ever called. If the guard
        // regressed, the events below would carry a stop attempt instead.
        let docker = std::ffi::OsString::from("docker");
        super::stop_engine_stream_with_kind(
            dml_core::engine::EngineKind::Systemd,
            &docker,
            move |v| sink.lock().unwrap().push(v),
        );
        let events = seen.lock().unwrap().clone();
        assert_eq!(events.first().unwrap()["event"], "section_start");
        assert_eq!(events.last().unwrap()["event"], "section_end");
        assert_eq!(events.last().unwrap()["status"], "ok");
        let text: String = events.iter().filter_map(|e| e["text"].as_str()).collect();
        assert!(
            text.contains("Leaving the Docker engine running"),
            "the stop must SAY it is deliberately leaving the daemon up: {text:?}"
        );
        assert!(
            !text.contains("Stopping Docker Desktop"),
            "a systemd host must never take the Desktop stop path: {text:?}"
        );
    }

    /// The Desktop branch still reaches its stop attempt — so the test above is
    /// measuring the guard, not a function that stopped doing anything.
    #[test]
    fn a_desktop_engine_still_takes_the_stop_path() {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        let fake = std::ffi::OsString::from("dml-no-such-docker-binary-ever.exe");
        super::stop_engine_stream_with_kind(
            dml_core::engine::EngineKind::Desktop,
            &fake,
            move |v| sink.lock().unwrap().push(v),
        );
        let events = seen.lock().unwrap().clone();
        let text: String = events.iter().filter_map(|e| e["text"].as_str()).collect();
        assert!(text.contains("Stopping Docker Desktop"), "{text:?}");
    }
}

#[cfg(test)]
mod engine_kind_tests {
    /// THE R1 REGRESSION GUARD.
    ///
    /// `dml-wow start` calls `ensure_engine_up_stream` unconditionally. Before
    /// the fix that went straight to Docker Desktop, which does not exist on
    /// Linux — so inside `dml-arch`, with the distro's dockerd down, `start`
    /// aborted with `DOCKER_DESKTOP_MISSING` before compose ever ran.
    ///
    /// Asserted for BOTH platforms from either, because the failure is on the
    /// platform CI mostly is not.
    #[test]
    fn a_linux_host_controls_the_engine_with_systemd_not_docker_desktop() {
        use dml_core::engine::EngineKind;
        assert_eq!(
            EngineKind::for_host(false),
            EngineKind::Systemd,
            "inside dml-arch the engine is docker.service; resolving Docker Desktop \
             there is what made `start` refuse on the one path the product exists for"
        );
        assert_eq!(EngineKind::for_host(true), EngineKind::Desktop);
        // ...and the built-for-this-platform helper agrees with the platform.
        assert_eq!(EngineKind::for_this_host(), EngineKind::for_host(cfg!(windows)));
    }

    /// The argv actually sent, read off the pure builders rather than trusted.
    /// `-n` is load-bearing: without it a distro missing its NOPASSWD rule
    /// blocks on a password prompt no button in the launcher can answer.
    #[test]
    fn the_systemd_start_is_non_interactive() {
        let argv = dml_core::engine::systemd_start_argv();
        assert_eq!(argv, ["-n", "systemctl", "start", "docker"]);
        assert_eq!(argv[0], "-n", "a sudo prompt is unanswerable from the launcher");
    }

    use std::sync::{Arc, Mutex};

    fn drive_systemd(
        start: std::io::Result<std::process::Output>,
    ) -> (Result<(), dml_core::error::CmdError>, Vec<serde_json::Value>) {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&seen);
        // A docker that does not exist, so the probe says No and the poll never
        // becomes ready; timeout 0 drives the whole path in a millisecond.
        let r = super::ensure_engine_up_systemd_with(
            std::ffi::OsStr::new("dml-no-such-docker-binary-ever.exe"),
            move || start,
            1,
            0,
            move |v| sink.lock().unwrap().push(v),
        );
        let events = seen.lock().unwrap().clone();
        (r, events)
    }

    fn failed_output() -> std::io::Result<std::process::Output> {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "sudo: a password is required",
        ))
    }

    /// THE FIX, driven rather than asserted. Inside the distro with the engine
    /// down, `start` used to abort with `DOCKER_DESKTOP_MISSING` before compose
    /// ran — an error naming a product that has no business on that machine.
    /// The systemd path must never produce it.
    #[test]
    fn the_systemd_path_never_blames_docker_desktop() {
        let (r, events) = drive_systemd(failed_output());
        let blob = serde_json::to_string(&events).unwrap();
        assert!(
            !blob.contains("DOCKER_DESKTOP_MISSING"),
            "the in-distro engine path must never mention Docker Desktop: {blob}"
        );
        // It still FAILS honestly (nothing came up) — this is not passing
        // because the function did nothing.
        let err = r.expect_err("a dead engine must still refuse");
        assert_eq!(err.code, "DOCKER_ENGINE_TIMEOUT");
    }

    /// The Tailscale lesson, applied: a sudo refusal must be NAMED, both when
    /// it happens and in the failure the user finally reads three minutes
    /// later. Previously a refusal, a missing unit and a slow daemon all
    /// produced the same useless message.
    #[test]
    fn a_sudo_refusal_is_reported_and_carried_into_the_timeout() {
        let (r, events) = drive_systemd(failed_output());
        let warn: String = events
            .iter()
            .filter(|e| e["level"] == "warn")
            .filter_map(|e| e["text"].as_str())
            .collect();
        assert!(
            warn.contains("a password is required"),
            "the start failure's cause must be reported when it happens: {warn:?}"
        );
        let err = r.unwrap_err();
        assert!(
            err.hint.contains("a password is required"),
            "the cause was known three minutes earlier and must not be lost: {:?}",
            err.hint
        );
    }

    /// ...and when the start SUCCEEDED, the timeout must not invent a start
    /// failure that never happened.
    #[test]
    fn a_timeout_after_a_clean_start_points_at_the_daemon_not_at_sudo() {
        let ok = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" })
            .args(if cfg!(windows) { ["/C", "exit 0"] } else { ["-c", "exit 0"] })
            .output();
        let (r, events) = drive_systemd(ok);
        let warn_count = events.iter().filter(|e| e["level"] == "warn").count();
        // The `Unknown`/`No` probe line is info; a clean start adds no warn.
        assert_eq!(warn_count, 0, "a clean start must not warn: {events:?}");
        let err = r.unwrap_err();
        assert!(
            err.hint.contains("systemctl status docker"),
            "point at the daemon when the start itself was fine: {:?}",
            err.hint
        );
    }
}
