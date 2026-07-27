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
    docker_desktop_program, docker_desktop_stop_args, docker_info_args, docker_program,
    engine_running, ensure_decision, game_state, launch_detached, parse_ps_json, poll_until_ready,
    stop_engine, stop_engine_enabled, EnsureDecision, PollOutcome, PsRow, ENGINE_POLL_INTERVAL_MS,
    ENGINE_POLL_TIMEOUT_MS,
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
fn engine_line(emit: &impl Fn(serde_json::Value), level: &str, text: impl Into<String>) {
    emit(serde_json::json!({"event": "line", "level": level, "text": text.into()}));
}

/// Native-mode prerequisite for any start: make sure the Docker Desktop engine
/// is up before compose runs. Emits progress; on an unrecoverable failure it
/// emits a terminal `error` event AND returns Err so the caller ABORTS instead
/// of composing against a dead engine. Blocking (real spawns + sleeps) — run
/// under `spawn_blocking`.
pub fn ensure_engine_up_stream(emit: impl Fn(serde_json::Value)) -> Result<(), CmdError> {
    use crate::native;
    let program = native::docker_program();
    let desktop = native::docker_desktop_program();
    match native::ensure_decision(native::engine_running(&program), desktop.is_some()) {
        native::EnsureDecision::AlreadyUp => {
            engine_line(&emit, "info", "Docker Desktop engine already running.");
            Ok(())
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
            // desktop is Some here (decision returned Launch).
            let exe = desktop.expect("Launch decision implies a resolved desktop exe");
            engine_line(&emit, "info", "Docker engine is down. Starting Docker Desktop...");
            if let Err(e) = native::launch_detached(&exe) {
                let msg = format!("Failed to launch Docker Desktop: {e}");
                emit(serde_json::json!({"event": "error", "error": {
                    "code": "DOCKER_DESKTOP_LAUNCH", "message": msg, "hint": "",
                }}));
                return Err(CmdError { code: "DOCKER_DESKTOP_LAUNCH".into(), message: msg, hint: String::new() });
            }
            let outcome = native::poll_until_ready(
                native::ENGINE_POLL_INTERVAL_MS,
                native::ENGINE_POLL_TIMEOUT_MS,
                || native::engine_running(&program),
                |ms| {
                    engine_line(&emit, "info", "Waiting for Docker Desktop to be ready...");
                    std::thread::sleep(std::time::Duration::from_millis(ms));
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
    use crate::native;
    let program = native::docker_program();
    engine_line(&emit, "info", "Stopping Docker Desktop...");
    match native::stop_engine(&program) {
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

    #[test]
    fn augmented_path_is_none_for_bare_docker() {
        // A bare `docker` (no directory) means PATH already resolves it and its
        // sibling helpers — don't rewrite PATH.
        assert!(augmented_path(OsStr::new("docker"), Some(OsString::from(r"C:\Windows"))).is_none());
    }
}
