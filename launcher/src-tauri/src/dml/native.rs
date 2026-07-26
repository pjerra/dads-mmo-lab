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

#![allow(dead_code)] // spike foundation: exercised by tests + the live PoC, not yet by a Tauri command

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub use dml_core::docker::{docker_desktop_program, docker_program};

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

/// Collapse `ps` rows to the one-word stack state the UI shows. Any container
/// in a `running` state means the game is up; "restarting" counts as up too
/// (it is not stopped). Pure, so the mapping is unit-tested without an engine.
pub fn game_state(rows: &[PsRow]) -> &'static str {
    let up = rows
        .iter()
        .any(|r| matches!(r.state.as_str(), "running" | "restarting"));
    if up {
        "running"
    } else {
        "stopped"
    }
}

/// One container row from `docker compose ps --format json`. Only the fields the
/// launcher needs are captured; unknown fields are ignored.
#[derive(Debug, Clone, serde::Deserialize, PartialEq)]
pub struct PsRow {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Service", default)]
    pub service: String,
    #[serde(rename = "State", default)]
    pub state: String,
    #[serde(rename = "Health", default)]
    pub health: String,
}

/// Parse Compose `ps --format json` output, tolerating both the JSON-array and
/// the NDJSON (object-per-line) shapes. Malformed lines are skipped rather than
/// failing the whole read — a status listing should degrade, not error.
pub fn parse_ps_json(text: &str) -> Vec<PsRow> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('[') {
        if let Ok(rows) = serde_json::from_str::<Vec<PsRow>>(trimmed) {
            return rows;
        }
    }
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_str::<PsRow>(l).ok())
        .collect()
}

// ---------------------------------------------------------------------------
// Docker Desktop engine lifecycle (native mode only).
//
// In native mode the launcher owns the Docker Desktop engine's up/down around a
// server start/stop: the engine (and its `docker-desktop` WSL VM) must be up
// before any `docker compose` runs, and stopping it on server-stop frees the
// VM's RAM. These helpers are the pure/testable core of that flow — the Tauri
// command wrappers in `lib.rs` supply the real spawns, clock and event stream.
// ---------------------------------------------------------------------------

/// How often to re-check the engine while waiting for it to come up (ms).
pub const ENGINE_POLL_INTERVAL_MS: u64 = 3_000;
/// How long to wait for the engine to come up before giving up (ms).
pub const ENGINE_POLL_TIMEOUT_MS: u64 = 180_000;

/// The `docker info` argv used as the "is the engine up?" probe. `--format`
/// keeps it fast and tiny; `info` talks to the engine over the named pipe and
/// needs no credential helper, so PATH is left untouched. Pure, so the argv is
/// asserted in tests without spawning.
pub fn docker_info_args() -> [&'static str; 3] {
    ["info", "--format", "{{.ServerVersion}}"]
}

/// Whether `docker info` succeeds against `program` — the definition of "the
/// Docker Desktop engine is running". A missing docker.exe, a down engine, or
/// any non-zero exit all read as not-running. (Impure: real spawn.)
pub fn engine_running(program: &OsStr) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(docker_info_args());
    cmd.stdin(Stdio::null());
    cmd.stdout(Stdio::null());
    cmd.stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    matches!(cmd.status(), Ok(s) if s.success())
}

/// The `docker desktop stop` argv — stops the Docker Desktop engine AND its
/// `docker-desktop` WSL VM, freeing the VM's RAM. Pure, for tests.
pub fn docker_desktop_stop_args() -> [&'static str; 2] {
    ["desktop", "stop"]
}

/// Run `docker desktop stop` against `program`. Best-effort at the call site: a
/// non-zero exit or spawn error is a warning, not a hard failure. (Impure.)
pub fn stop_engine(program: &OsStr) -> std::io::Result<std::process::Output> {
    let mut cmd = Command::new(program);
    cmd.args(docker_desktop_stop_args());
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.output()
}

/// Launch `program` (the Docker Desktop GUI exe) detached, without a console
/// window. Returns as soon as the process is spawned — the engine comes up
/// asynchronously and is waited for separately via [`poll_until_ready`].
pub fn launch_detached(program: &OsStr) -> std::io::Result<()> {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn().map(|_| ())
}

/// What to do to satisfy the "engine must be up" prerequisite, decided from two
/// facts: is the engine already up, and did we find a Docker Desktop.exe to
/// launch. Pure, so the branch table is unit-tested without spawns.
#[derive(Debug, PartialEq, Eq)]
pub enum EnsureDecision {
    /// Engine already running — nothing to do.
    AlreadyUp,
    /// Engine down and no Docker Desktop.exe found — abort (do not compose
    /// against a dead engine, and there is nothing to start).
    NoDesktop,
    /// Engine down but Docker Desktop.exe found — launch it and poll.
    Launch,
}

/// Decide how to satisfy the engine prerequisite. See [`EnsureDecision`].
pub fn ensure_decision(engine_up: bool, desktop_found: bool) -> EnsureDecision {
    if engine_up {
        EnsureDecision::AlreadyUp
    } else if !desktop_found {
        EnsureDecision::NoDesktop
    } else {
        EnsureDecision::Launch
    }
}

/// Whether server-stop should also stop the Docker Desktop engine. Only in
/// native mode, and only when the (default-on) `nativeManageDocker` toggle is
/// on — `None` means "not passed", which defaults to ON. Pure, for tests.
pub fn stop_engine_enabled(native: bool, manage_docker: Option<bool>) -> bool {
    native && manage_docker.unwrap_or(true)
}

/// How a wait-for-engine loop ended.
#[derive(Debug, PartialEq, Eq)]
pub enum PollOutcome {
    /// The engine became ready; `waited_ms` is how long we waited first.
    Ready { waited_ms: u64 },
    /// The budget elapsed without the engine coming up.
    Timeout { waited_ms: u64 },
}

/// Poll `ready` until it returns true or the cumulative wait exceeds
/// `timeout_ms`. Checks immediately (t=0), then sleeps `interval_ms` between
/// checks via the injected `sleep`. Pure w.r.t. I/O — the caller supplies both
/// the readiness probe and the sleeper, so tests drive it with a canned
/// sequence and a no-op (counting) sleeper, exercising the timeout/round
/// arithmetic without a real engine or real delays.
pub fn poll_until_ready(
    interval_ms: u64,
    timeout_ms: u64,
    mut ready: impl FnMut() -> bool,
    mut sleep: impl FnMut(u64),
) -> PollOutcome {
    let mut waited = 0u64;
    loop {
        if ready() {
            return PollOutcome::Ready { waited_ms: waited };
        }
        if waited >= timeout_ms {
            return PollOutcome::Timeout { waited_ms: waited };
        }
        sleep(interval_ms);
        waited = waited.saturating_add(interval_ms);
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
    fn ps_json_parses_ndjson() {
        let text = r#"{"Name":"dml-poc-game-1","Service":"game","State":"running","Health":""}
{"Name":"dml-poc-db-1","Service":"db","State":"exited","Health":""}"#;
        let rows = parse_ps_json(text);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].name, "dml-poc-game-1");
        assert_eq!(rows[0].state, "running");
        assert_eq!(rows[1].service, "db");
    }

    #[test]
    fn ps_json_parses_array() {
        let text = r#"[{"Name":"dml-poc-game-1","Service":"game","State":"running","Health":"healthy"}]"#;
        let rows = parse_ps_json(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].health, "healthy");
    }

    #[test]
    fn ps_json_skips_garbage_lines() {
        let text = "not json\n{\"Name\":\"ok\",\"State\":\"running\"}\n";
        let rows = parse_ps_json(text);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "ok");
    }

    #[test]
    fn ps_json_empty_is_empty() {
        assert!(parse_ps_json("").is_empty());
        assert!(parse_ps_json("   \n  ").is_empty());
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

    #[test]
    fn game_state_running_when_any_up() {
        let rows = vec![
            PsRow { name: "db".into(), service: "ac-database".into(), state: "running".into(), health: "healthy".into() },
            PsRow { name: "auth".into(), service: "ac-authserver".into(), state: "exited".into(), health: "".into() },
        ];
        assert_eq!(game_state(&rows), "running");
    }

    #[test]
    fn game_state_stopped_when_none_up() {
        let rows = vec![PsRow {
            name: "db".into(), service: "ac-database".into(), state: "exited".into(), health: "".into(),
        }];
        assert_eq!(game_state(&rows), "stopped");
        assert_eq!(game_state(&[]), "stopped");
    }

    // --- engine lifecycle --------------------------------------------------

    #[test]
    fn docker_info_args_probe_the_server_version() {
        assert_eq!(docker_info_args(), ["info", "--format", "{{.ServerVersion}}"]);
    }

    #[test]
    fn docker_desktop_stop_args_are_desktop_stop() {
        assert_eq!(docker_desktop_stop_args(), ["desktop", "stop"]);
    }

    #[test]
    fn ensure_decision_already_up_when_engine_running() {
        // Engine up wins regardless of whether a desktop exe was found.
        assert_eq!(ensure_decision(true, false), EnsureDecision::AlreadyUp);
        assert_eq!(ensure_decision(true, true), EnsureDecision::AlreadyUp);
    }

    #[test]
    fn ensure_decision_no_desktop_aborts() {
        // Engine down and nothing to launch -> abort, never compose against a
        // dead engine.
        assert_eq!(ensure_decision(false, false), EnsureDecision::NoDesktop);
    }

    #[test]
    fn ensure_decision_launch_when_down_but_installed() {
        assert_eq!(ensure_decision(false, true), EnsureDecision::Launch);
    }

    #[test]
    fn stop_engine_enabled_only_native_and_toggle_on() {
        // Default (None) is ON in native mode.
        assert!(stop_engine_enabled(true, None));
        assert!(stop_engine_enabled(true, Some(true)));
        // Explicitly defeated by the user.
        assert!(!stop_engine_enabled(true, Some(false)));
        // WSL mode never stops Docker, whatever the toggle says.
        assert!(!stop_engine_enabled(false, None));
        assert!(!stop_engine_enabled(false, Some(true)));
    }

    #[test]
    fn poll_ready_immediately_does_not_sleep() {
        let mut sleeps = 0u32;
        let out = poll_until_ready(3_000, 180_000, || true, |_| sleeps += 1);
        assert_eq!(out, PollOutcome::Ready { waited_ms: 0 });
        assert_eq!(sleeps, 0);
    }

    #[test]
    fn poll_ready_after_a_few_rounds() {
        // false, false, true -> ready on the third check, two sleeps of 3s.
        let seq = std::cell::Cell::new(0u32);
        let mut sleeps = 0u32;
        let out = poll_until_ready(
            3_000,
            180_000,
            || {
                let n = seq.get();
                seq.set(n + 1);
                n >= 2
            },
            |_| sleeps += 1,
        );
        assert_eq!(out, PollOutcome::Ready { waited_ms: 6_000 });
        assert_eq!(sleeps, 2);
    }

    #[test]
    fn poll_times_out_when_never_ready() {
        // Never ready: checks at 0,3,6,9s; at 9s waited>=timeout -> Timeout.
        let mut sleeps = 0u32;
        let out = poll_until_ready(3_000, 9_000, || false, |ms| {
            assert_eq!(ms, 3_000);
            sleeps += 1;
        });
        assert_eq!(out, PollOutcome::Timeout { waited_ms: 9_000 });
        assert_eq!(sleeps, 3);
    }

    #[test]
    fn poll_timeout_zero_gives_up_immediately() {
        let mut sleeps = 0u32;
        let out = poll_until_ready(3_000, 0, || false, |_| sleeps += 1);
        assert_eq!(out, PollOutcome::Timeout { waited_ms: 0 });
        assert_eq!(sleeps, 0);
    }
}
