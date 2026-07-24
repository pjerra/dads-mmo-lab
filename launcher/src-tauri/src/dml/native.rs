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
use std::process::Command;

/// Environment override for the docker executable, so a boxed/portable install
/// or CI can point at an arbitrary `docker.exe` without patching discovery.
pub const DOCKER_ENV: &str = "DML_DOCKER";

/// Known absolute install locations for `docker.exe`, tried in order AFTER the
/// `DML_DOCKER` override and a bare `docker` on PATH both miss. The per-user
/// path is first because that is where a default Docker Desktop install landed
/// on this box (`%LOCALAPPDATA%\Programs\DockerDesktop\...`) — it is NOT on the
/// machine PATH, which is exactly why plain `Command::new("docker")` fails and
/// discovery has to look here.
fn candidate_docker_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(
            PathBuf::from(&local)
                .join("Programs/DockerDesktop/resources/bin/docker.exe"),
        );
    }
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        out.push(PathBuf::from(&pf).join("Docker/Docker/resources/bin/docker.exe"));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
        out.push(PathBuf::from(&pf86).join("Docker/Docker/resources/bin/docker.exe"));
    }
    out
}

/// Resolve which docker executable to invoke. Pure (no filesystem of its own)
/// so it is unit-testable: the caller supplies the override, the candidate
/// list, and an existence predicate.
///
/// Order: explicit `DML_DOCKER` override (used verbatim, even if absent — an
/// intentional override should surface its own error, not be silently
/// second-guessed) → first candidate the predicate accepts → bare `docker`
/// (let PATH resolution and its own "not found" error speak).
fn resolve_docker_program(
    env_override: Option<OsString>,
    candidates: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> OsString {
    if let Some(ov) = env_override {
        if !ov.is_empty() {
            return ov;
        }
    }
    for c in candidates {
        if exists(c) {
            return c.clone().into_os_string();
        }
    }
    OsString::from("docker")
}

/// The docker executable this process should use, resolved against the real
/// environment and filesystem.
pub fn docker_program() -> OsString {
    resolve_docker_program(
        std::env::var_os(DOCKER_ENV),
        &candidate_docker_paths(),
        |p| p.exists(),
    )
}

/// Environment override for the Docker Desktop APP exe (the `Docker Desktop.exe`
/// GUI, not the `docker` CLI), used by the "Start Docker Desktop" fix action.
pub const DOCKER_DESKTOP_ENV: &str = "DML_DOCKER_DESKTOP";

/// Known absolute install locations for `Docker Desktop.exe`, tried in order.
/// Per-user first (matches this box's `%LOCALAPPDATA%\Programs\DockerDesktop`
/// install — the same tree `candidate_docker_paths` finds the CLI under), then
/// the system location.
fn candidate_docker_desktop_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        out.push(PathBuf::from(&local).join("Programs/DockerDesktop/Docker Desktop.exe"));
    }
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        out.push(PathBuf::from(&pf).join("Docker/Docker/Docker Desktop.exe"));
    }
    if let Some(pf86) = std::env::var_os("ProgramFiles(x86)") {
        out.push(PathBuf::from(&pf86).join("Docker/Docker/Docker Desktop.exe"));
    }
    out
}

/// Resolve the Docker Desktop app exe. Order: `DML_DOCKER_DESKTOP` override
/// (used verbatim) → first candidate the predicate accepts → `None`. Unlike the
/// CLI resolver there is NO bare-name fallback: `Docker Desktop.exe` is never on
/// PATH, so a miss means "not found" and the caller declines to launch a guess
/// rather than spawn something arbitrary. Pure, for testing.
fn resolve_docker_desktop(
    env_override: Option<OsString>,
    candidates: &[PathBuf],
    exists: impl Fn(&Path) -> bool,
) -> Option<OsString> {
    if let Some(ov) = env_override {
        if !ov.is_empty() {
            return Some(ov);
        }
    }
    for c in candidates {
        if exists(c) {
            return Some(c.clone().into_os_string());
        }
    }
    None
}

/// The Docker Desktop app exe to launch, resolved against the real environment
/// and filesystem. `None` when no known install location exists.
pub fn docker_desktop_program() -> Option<OsString> {
    resolve_docker_desktop(
        std::env::var_os(DOCKER_DESKTOP_ENV),
        &candidate_docker_desktop_paths(),
        |p| p.exists(),
    )
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_wins_over_everything() {
        let got = resolve_docker_program(
            Some(OsString::from(r"C:\custom\docker.exe")),
            &[PathBuf::from(r"C:\a\docker.exe")],
            |_| true,
        );
        assert_eq!(got, OsString::from(r"C:\custom\docker.exe"));
    }

    #[test]
    fn empty_override_is_ignored() {
        // An empty env var must not short-circuit to "" — fall through to
        // candidates instead.
        let got = resolve_docker_program(
            Some(OsString::new()),
            &[PathBuf::from(r"C:\a\docker.exe")],
            |_| true,
        );
        assert_eq!(got, OsString::from(r"C:\a\docker.exe"));
    }

    #[test]
    fn first_existing_candidate_wins() {
        let cands = vec![
            PathBuf::from(r"C:\missing\docker.exe"),
            PathBuf::from(r"C:\present\docker.exe"),
        ];
        let got = resolve_docker_program(None, &cands, |p| {
            p == Path::new(r"C:\present\docker.exe")
        });
        assert_eq!(got, OsString::from(r"C:\present\docker.exe"));
    }

    #[test]
    fn falls_back_to_bare_docker_on_path() {
        let got = resolve_docker_program(
            None,
            &[PathBuf::from(r"C:\missing\docker.exe")],
            |_| false,
        );
        assert_eq!(got, OsString::from("docker"));
    }

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

    #[test]
    fn docker_desktop_override_wins() {
        let got = resolve_docker_desktop(
            Some(OsString::from(r"C:\custom\Docker Desktop.exe")),
            &[PathBuf::from(r"C:\a\Docker Desktop.exe")],
            |_| true,
        );
        assert_eq!(got, Some(OsString::from(r"C:\custom\Docker Desktop.exe")));
    }

    #[test]
    fn docker_desktop_first_existing_candidate_wins() {
        let cands = vec![
            PathBuf::from(r"C:\missing\Docker Desktop.exe"),
            PathBuf::from(r"C:\present\Docker Desktop.exe"),
        ];
        let got = resolve_docker_desktop(None, &cands, |p| {
            p == Path::new(r"C:\present\Docker Desktop.exe")
        });
        assert_eq!(got, Some(OsString::from(r"C:\present\Docker Desktop.exe")));
    }

    #[test]
    fn docker_desktop_none_when_nothing_exists() {
        // No bare-name fallback: a miss must be None, never a guessed launch.
        let got = resolve_docker_desktop(
            None,
            &[PathBuf::from(r"C:\missing\Docker Desktop.exe")],
            |_| false,
        );
        assert_eq!(got, None);
    }

    #[test]
    fn candidate_paths_include_per_user_location() {
        // With LOCALAPPDATA set, the per-user Docker Desktop path is a candidate
        // and comes before the system one.
        std::env::set_var("LOCALAPPDATA", r"C:\Users\test\AppData\Local");
        let cands = candidate_docker_paths();
        assert!(cands
            .first()
            .unwrap()
            .to_string_lossy()
            .contains("Programs"));
        assert!(cands
            .iter()
            .any(|p| p.to_string_lossy().contains("DockerDesktop")));
    }
}
