use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

/// The `docker desktop start` argv — the SYMMETRIC sibling of
/// [`docker_desktop_stop_args`]. `-d` returns as soon as the request is
/// accepted; readiness is still waited for by [`poll_until_ready`], exactly as
/// with the exe route, so the two paths differ only in how the engine is asked
/// to come up. Pure, for tests.
pub fn docker_desktop_start_args() -> [&'static str; 3] {
    ["desktop", "start", "-d"]
}

/// How long `docker desktop start -d` may take to ANSWER.
///
/// Not how long the engine may take to become ready -- that is
/// [`ENGINE_POLL_TIMEOUT_MS`], and it runs after this returns. This bounds only
/// the ask, which returns as soon as Docker Desktop has accepted the request.
///
/// It exists because this function had no bound at all, which put the one
/// command reached specifically when "the engine is not answering" in the exact
/// position every other docker call in this project is bounded to avoid: a
/// dockerd wedged during startup accepts the connection and never answers, and
/// a deadline is only consulted after a call RETURNS. An unbounded ask here
/// hangs `install-native`, `migrate-import` and every native `games start` at
/// their first step, with the readiness timeout below never getting to fire.
pub const ENGINE_START_ASK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// Run `docker desktop start -d` against `program`.
///
/// WHY THIS EXISTS: launching `Docker Desktop.exe` starts the GUI, and the
/// dashboard WINDOW pops up over whatever the user was doing every time the
/// server starts with the engine down. We only ever wanted the engine. This
/// asks for the engine and nothing else.
///
/// It is not universally available — the `docker desktop` CLI plugin arrived in
/// Docker Desktop 4.37 — so the caller treats failure as "fall back to the exe",
/// never as a hard error. (Impure.)
///
/// BOUNDED by [`ENGINE_START_ASK_TIMEOUT`], and draining while it waits so a
/// chatty start cannot fill a pipe buffer and deadlock against our own read. A
/// timeout reads as the same "fall back to the exe" that a spawn failure and a
/// non-zero exit already do — which is the right answer, because a `docker
/// desktop start` that will not answer is exactly the case where launching the
/// exe is the remaining option.
pub fn start_engine(program: &OsStr) -> std::io::Result<std::process::Output> {
    let mut cmd = Command::new(program);
    cmd.args(docker_desktop_start_args());
    cmd.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    crate::proc::output_bounded_draining(cmd, ENGINE_START_ASK_TIMEOUT).ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "docker desktop start did not answer",
        )
    })
}

/// Did `docker desktop start` actually take responsibility for the engine?
///
/// Pure so the fallback rule is unit-tested without spawning anything. Only a
/// clean exit counts: an older Docker Desktop answers an unknown `desktop`
/// subcommand with a non-zero exit, and a spawn error means no docker CLI at
/// all. Both mean "fall back to launching the exe", which is the behaviour that
/// shipped before this and still works everywhere.
pub fn start_engine_succeeded(result: &std::io::Result<std::process::Output>) -> bool {
    matches!(result, Ok(out) if out.status.success())
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

    /// A program that never exits, per platform.
    ///
    /// Never a hardcoded `cmd.exe` or drive-letter literal: the convention this
    /// crate already follows in `runner.rs`/`proc.rs`, so the suite runs on the
    /// Linux CI job too.
    fn never_returns() -> (&'static str, Vec<&'static str>) {
        #[cfg(windows)]
        {
            // NOT `cmd /C pause`: it READS stdin, and this call null-stdins its
            // child, so pause sees EOF and exits at once -- a "never returns"
            // helper that returns immediately makes the assertion below vacuous
            // in the most convincing possible way. ping ignores stdin.
            ("ping", vec!["-n", "600", "127.0.0.1"])
        }
        #[cfg(not(windows))]
        {
            ("sh", vec!["-c", "sleep 600"])
        }
    }

    /// `start_engine` must give up rather than block forever.
    ///
    /// The bug this pins: it used a bare `cmd.output()`, so the one command
    /// reached specifically BECAUSE the engine is not answering had no
    /// wall-clock bound at all. A Docker Desktop wedged during startup would
    /// hang `install-native`, `migrate-import` and every native `games start`
    /// at their first step — and the 180s readiness poll meant to cover that
    /// case runs AFTER this returns, so it could never fire.
    ///
    /// Asserts elapsed >= the bound, not merely that it returned. A failed
    /// spawn also returns "quickly", so a version of this test that only
    /// checked the result would pass on a machine where the child never ran —
    /// the vacuous-pass shape this project has recorded. The elapsed check is
    /// something only a real wait can satisfy.
    #[test]
    fn start_engine_is_bounded_rather_than_blocking_forever() {
        // A short bound so the test is quick; the production constant is
        // asserted separately below.
        let (prog, args) = never_returns();
        let mut cmd = Command::new(prog);
        cmd.args(&args);
        cmd.stdin(Stdio::null());
        let began = std::time::Instant::now();
        let out = crate::proc::output_bounded_draining(cmd, std::time::Duration::from_millis(600));
        let elapsed = began.elapsed();

        assert!(out.is_none(), "a process that never exits must time out, not return output");
        assert!(
            elapsed >= std::time::Duration::from_millis(600),
            "returned in {elapsed:?} — that is a failed spawn, not a real bounded wait"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(30),
            "took {elapsed:?}; the bound did not fire"
        );
    }

    /// A deadline must bound the CALL, not merely the child.
    ///
    /// The regression this pins, measured on 2026-08-03: a 600ms-bounded call
    /// against a child that spawns a GRANDCHILD returned after 605 seconds.
    /// `child.kill()` kills the child; the grandchild inherited the stdout and
    /// stderr pipe handles and keeps them open, so the reader threads never see
    /// EOF and the join AFTER the kill blocks for as long as the grandchild
    /// lives. The same child spawned without a shell returned in 0.61s, which
    /// is what made the cause unambiguous.
    ///
    /// This matters because `docker`, `wsl.exe` and `git` all spawn helper
    /// processes, so it is every bounded probe in the project quietly losing
    /// its bound — the exact failure the bounds exist to prevent.
    ///
    /// Windows-only because the grandchild shape needs `cmd /C`, and because
    /// that is where it was found. Asserts elapsed >= the bound as well, so a
    /// spawn that never ran cannot satisfy it.
    #[cfg(windows)]
    #[test]
    fn a_deadline_bounds_the_call_even_when_a_grandchild_holds_the_pipes() {
        let mut cmd = Command::new("cmd");
        cmd.args(["/C", "ping", "-n", "600", "127.0.0.1"]);
        let began = std::time::Instant::now();
        let out = crate::proc::output_bounded_draining(cmd, std::time::Duration::from_millis(600));
        let elapsed = began.elapsed();

        assert!(out.is_none(), "a child that never exits must time out");
        assert!(
            elapsed >= std::time::Duration::from_millis(600),
            "returned in {elapsed:?} — that is a failed spawn, not a real wait"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(20),
            "took {elapsed:?}: the call outlived its own deadline, which is the bug"
        );
    }

    /// The production bound is set, and set to something honest.
    #[test]
    fn the_engine_start_ask_is_bounded_well_inside_the_readiness_wait() {
        // Two properties, both load-bearing. Non-zero, or there is no bound at
        // all; and comfortably shorter than the readiness poll that follows it,
        // because an ask that could outlast the whole wait would make the wait
        // meaningless.
        assert!(ENGINE_START_ASK_TIMEOUT > std::time::Duration::ZERO);
        assert!(
            ENGINE_START_ASK_TIMEOUT
                < std::time::Duration::from_millis(ENGINE_POLL_TIMEOUT_MS),
            "the ask ({ENGINE_START_ASK_TIMEOUT:?}) must not be able to outlast the readiness wait"
        );
    }

    /// A timed-out ask must fall back to the exe, exactly as a spawn failure
    /// and a non-zero exit already do.
    ///
    /// This is the behaviour that keeps the new bound from becoming a new way
    /// to fail: a `docker desktop start` that will not answer is precisely the
    /// case where launching Docker Desktop.exe is the remaining option.
    #[test]
    fn a_timed_out_ask_reads_as_fall_back_to_the_exe() {
        let timed_out: std::io::Result<std::process::Output> = Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "docker desktop start did not answer",
        ));
        assert!(!start_engine_succeeded(&timed_out));
    }

    // -- docker/docker-desktop executable discovery (Task 3) -----------------

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

    // -- ps parsing / game_state ---------------------------------------------

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

    // --- engine lifecycle ----------------------------------------------------

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

#[cfg(test)]
mod start_engine_tests {
    use super::*;

    /// Symmetric with the stop argv, and `-d` is load-bearing: without it the
    /// call blocks until Docker Desktop reports ready, which would duplicate
    /// (and fight with) `poll_until_ready`'s own bounded wait.
    #[test]
    fn the_start_argv_asks_for_the_engine_and_returns_immediately() {
        assert_eq!(docker_desktop_start_args(), ["desktop", "start", "-d"]);
        // The pair must stay symmetric -- a reader should be able to see that
        // one command starts what the other stops.
        assert_eq!(docker_desktop_stop_args()[0], docker_desktop_start_args()[0]);
    }

    fn fake_output(code: i32) -> std::io::Result<std::process::Output> {
        // Build a real Output with a chosen status by running a trivial command
        // that is guaranteed present on this platform.
        #[cfg(windows)]
        let out = Command::new("cmd").args(["/C", &format!("exit {code}")]).output();
        #[cfg(not(windows))]
        let out = Command::new("sh").args(["-c", &format!("exit {code}")]).output();
        out
    }

    /// The fallback rule, which is the whole safety story: ONLY a clean exit
    /// means the CLI took responsibility for the engine. An older Docker Desktop
    /// (pre-4.37) rejects the unknown `desktop` subcommand with a non-zero exit,
    /// and a missing docker CLI fails to spawn at all -- both must fall back to
    /// launching the exe, which is what shipped before and works everywhere.
    #[test]
    fn only_a_clean_exit_counts_as_the_cli_having_started_the_engine() {
        assert!(start_engine_succeeded(&fake_output(0)));
        assert!(!start_engine_succeeded(&fake_output(1)), "an old Desktop rejects the subcommand");
        assert!(
            !start_engine_succeeded(&Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no docker",
            ))),
            "a missing docker CLI must fall back, not abort"
        );
    }

    /// Guards the one way this could take down a working setup: if a spawn
    /// failure were ever read as success, the exe fallback would be skipped and
    /// the engine would never start at all.
    #[test]
    fn a_spawn_failure_is_never_mistaken_for_a_started_engine() {
        let missing = std::ffi::OsString::from("dml-no-such-docker-binary-ever.exe");
        let result = start_engine(&missing);
        assert!(result.is_err(), "the fake binary must not exist: {result:?}");
        assert!(!start_engine_succeeded(&result));
    }
}
