pub mod dml;
pub mod nativesetup;
pub mod power;
pub mod realmlist;
pub mod watch;
pub mod wslconfig;
mod zam;

use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::Manager;
use tauri::State;

use crate::dml::envelope::Envelope;
use crate::dml::runner::{DmlRunner, RunnerError};

pub struct InstallSession {
    pub stdin: std::process::ChildStdin,
    pub pid: u32,
}

pub enum InstallSlot {
    Starting,
    Running(InstallSession),
}

/// Auto-shutdown watcher control block (Batch 2 F5). `generation` is bumped
/// on every set_auto_shutdown call; a watcher thread captures the generation
/// it was born with and exits as soon as the stored one differs (or enabled
/// drops), so rapid toggle flips can never leave two live watchers racing.
pub struct AutoShutdownCtl {
    pub generation: u64,
    pub enabled: bool,
}

pub struct AppState {
    pub runner: std::sync::Arc<DmlRunner>,
    pub install: Arc<Mutex<Option<InstallSlot>>>,
    pub auto_shutdown: Arc<Mutex<AutoShutdownCtl>>,
    /// Native-mode cache of the STATIC config registry rows (`dml wow config
    /// registry --json` → `.data.settings[]`). The registry never changes
    /// within a session, so `wow_config_read` fetches it at most once and then
    /// reads only the live VALUES itself from disk (see `dml::config`). `None`
    /// until the first native read populates it.
    pub config_registry: Arc<Mutex<Option<Vec<serde_json::Value>>>>,
    /// Native-mode cache of the STATIC tuning registry rows (`dml wow config
    /// tuning-registry --json` → `.data.settings[]`, 13 rows). Same one-shot
    /// contract as `config_registry`: `wow_tuning_read` fetches it once, then
    /// reads only the live value/installed fields itself (see `dml::tuning`).
    pub tuning_registry: Arc<Mutex<Option<Vec<serde_json::Value>>>>,
    /// Native-mode cache of the STATIC module catalog (`dml wow module catalog
    /// --json` → `.data`). `wow_module_read` fetches it once, then fills every
    /// dynamic per-row field itself from disk (see `dml::modules`).
    pub module_catalog: Arc<Mutex<Option<serde_json::Value>>>,
}

#[derive(Debug, Serialize)]
pub struct CmdError {
    pub code: String,
    pub message: String,
    pub hint: String,
}

impl From<RunnerError> for CmdError {
    fn from(e: RunnerError) -> Self {
        match e {
            RunnerError::Spawn(m) => CmdError {
                // Code stays WSL_SPAWN (frontend matches on it); the hint covers
                // both backends since this mapping has no runner context.
                code: "WSL_SPAWN".into(),
                message: m,
                hint: "Default mode: is WSL + the dml-arch distro present? (wsl -d dml-arch). Native mode (DML_BACKEND=native): are Git Bash and Docker Desktop installed and running?".into(),
            },
            RunnerError::BadOutput { raw } => CmdError {
                code: "CLI_BAD_OUTPUT".into(),
                message: raw,
                hint: "Is the dml CLI v3.0.0 installed? Run: powershell -File cli\\dev-install.ps1".into(),
            },
        }
    }
}

fn envelope_to_result(env: Envelope) -> Result<serde_json::Value, CmdError> {
    if env.ok {
        Ok(env.data)
    } else {
        let e = env.error.unwrap_or(crate::dml::envelope::ErrorInfo {
            code: "CLI_BAD_OUTPUT".into(),
            message: "ok=false with no error object".into(),
            hint: String::new(),
        });
        Err(CmdError { code: e.code, message: e.message, hint: e.hint })
    }
}

async fn run_json_cmd(
    state: State<'_, AppState>,
    args: Vec<String>,
) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        runner.run_json(&refs)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map_err(CmdError::from)
    .and_then(envelope_to_result)
}

pub fn validate_game_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn bad_id(id: &str) -> CmdError {
    CmdError {
        code: "BAD_ID".into(),
        message: format!("invalid game id: {id:?}"),
        hint: "Game ids come from games_list".into(),
    }
}

// --- LAN / doctor / tool-install plumbing (Round Q) ------------------------
//
// These commands take webview input directly (action, ip, tool), so every
// value is checked against a closed allowlist or a pure validator BEFORE it
// reaches a spawn. Nothing here is string-interpolated into a shell -- args
// go straight into Command::args as separate argv entries -- but validating
// up front keeps garbage/injection-shaped input from ever reaching the CLI
// and turns a bad webview call into a typed error instead of a mystery
// WSL/CLI failure.

const LAN_TITLE: &str = "wow-server-playerbots";
const LAN_ACTIONS: [&str; 4] = ["on", "off", "status", "refresh"];
const TAILSCALE_ACTIONS: [&str; 4] = ["install", "up", "status", "down"];
const TOOL_NAMES: [&str; 2] = ["unbound", "unbound-remove"];

/// Pure, testable IPv4-shape check: `^[0-9]{1,3}(\.[0-9]{1,3}){3}$`. Exactly
/// 4 dot-separated groups of 1-3 ASCII digits each -- matches the CLI's own
/// guard in `dml lan` (it re-validates independently) rather than a strict
/// 0-255 range check, so this only needs to reject shapes that could carry
/// something other than an address (whitespace, semicolons, letters, extra
/// segments) before the value is ever used to build a command line.
pub fn validate_ip(ip: &str) -> bool {
    let parts: Vec<&str> = ip.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()))
}

/// Install-from-URL check (Batch 4 F16): a plain https git URL --
/// `^https://[A-Za-z0-9./_-]+$`, bounded length. Deliberately closed (no
/// ssh/scp forms, no query strings, no credentials-in-URL) -- the value
/// becomes an argv token for `dml run <url>`, which git-clones it and runs
/// the repo's own install script; the typed-confirm + warning in the GUI
/// carry the trust decision, this only keeps shell-shaped garbage out.
pub fn validate_git_url(url: &str) -> bool {
    const PREFIX: &str = "https://";
    url.len() <= 300
        && url.len() > PREFIX.len()
        && url.starts_with(PREFIX)
        && url[PREFIX.len()..]
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '_' | '-'))
        // No `..` path segment: the URL becomes the clone target for
        // `dml run <url>`, and a `..` segment could escape the clone dir.
        // A dot-only charset can't otherwise be rejected by the char filter.
        && !url.split('/').any(|seg| seg == "..")
}

/// Internet-play address check (Batch 4 F15): a public IPv4 or hostname,
/// `^[A-Za-z0-9.-]{1,253}$` -- mirrors the CLI's own `--internet` guard
/// (which re-validates independently). Like validate_ip this only needs to
/// keep shell/SQL-shaped garbage out of an argv slot; DNS decides whether
/// the name actually resolves.
pub fn validate_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

fn bad_arg(message: impl Into<String>) -> CmdError {
    CmdError { code: "BAD_ARG".into(), message: message.into(), hint: "Check the value and try again.".into() }
}

// --- Auto-shutdown watcher (Batch 2 F5) -------------------------------------
//
// Every ~5s the watcher thread asks tasklist whether Wow.exe is running and
// feeds the answer to the pure WatchMachine (src/watch.rs). When the machine
// fires (client gone for 2 consecutive polls), the thread runs the same CLI
// stop the Home Stop button uses -- captured, not streamed, because there is
// no terminal to stream into -- guarded by a fresh server-detail check so a
// server that is already down is never "stopped" again. Events go to the
// webview on the "auto-shutdown" channel: {kind:"state",state:"waiting"|"armed"}
// and {kind:"fired",stopped:bool}.

/// Tri-state Wow.exe probe: `Some(true)` running, `Some(false)` a genuine
/// "not running" answer, `None` = the probe itself failed (spawn error,
/// nonzero exit, empty output). The watcher must treat None as "no
/// observation" and skip the debounce step -- counting a failed probe as
/// absence would let two correlated failures fire a stop on a live game
/// (see watch::classify_tasklist).
fn wow_client_probe() -> Option<bool> {
    let mut cmd = std::process::Command::new("tasklist");
    cmd.args(["/FI", "IMAGENAME eq Wow.exe", "/FO", "CSV", "/NH"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, as elsewhere
    }
    let out = cmd.output().ok()?;
    watch::classify_tasklist(out.status.success(), &String::from_utf8_lossy(&out.stdout))
}

/// Verdicts that mean "there are running containers worth a graceful stop":
/// online/starting/soap_unreachable (world alive) AND crashed (world dead
/// but auth/db typically still running -- `games stop`'s compose down
/// cleans those up). Plain stopped / absent / an unreadable state are NOT
/// here. Shared by the auto-shutdown watcher and restart_wsl so both agree
/// on when a stop is warranted.
fn verdict_needs_stop(verdict: Option<&str>) -> bool {
    matches!(
        verdict,
        Some("online") | Some("starting") | Some("soap_unreachable") | Some("crashed")
    )
}

/// Read the server verdict once. `Some(v)` when server-detail answered ok;
/// `None` when the read failed (docker/WSL hiccup) -- callers decide how to
/// treat "don't know".
fn read_server_verdict(runner: &DmlRunner) -> Option<String> {
    runner
        .run_json(&["wow", "server-detail"])
        .ok()
        .filter(|env| env.ok)
        .and_then(|env| env.data["verdict"].as_str().map(str::to_string))
}

/// True only when a follow-up server-detail read CONFIRMS the stack is down
/// (verdict readable and no longer needs-stop). A failed read returns false:
/// if we cannot confirm the world is down we must not claim a graceful stop
/// succeeded. Used to judge `games stop` by its effect, not by run_captured's
/// spawn-only Ok.
fn stop_confirmed_down(runner: &DmlRunner) -> bool {
    match read_server_verdict(runner) {
        Some(v) => !verdict_needs_stop(Some(v.as_str())),
        None => false,
    }
}

fn auto_shutdown_watcher(
    my_gen: u64,
    ctl: Arc<Mutex<AutoShutdownCtl>>,
    runner: Arc<DmlRunner>,
    app: tauri::AppHandle,
) {
    use tauri::Emitter;
    let mut machine = watch::WatchMachine::new();
    let _ = app.emit("auto-shutdown", serde_json::json!({"kind": "state", "state": "waiting"}));
    loop {
        {
            let c = ctl.lock().unwrap();
            if c.generation != my_gen || !c.enabled {
                return;
            }
        }
        // None = no usable probe this tick -> skip the debounce entirely, so
        // correlated tasklist failures can never advance toward a stop.
        let action = match wow_client_probe() {
            Some(running) => machine.step(running),
            None => watch::WatchAction::None,
        };
        match action {
            watch::WatchAction::Armed => {
                let _ = app
                    .emit("auto-shutdown", serde_json::json!({"kind": "state", "state": "armed"}));
            }
            watch::WatchAction::Fire => {
                // Read the server state once. Three honest outcomes so the
                // card never claims success on a failed stop or "wasn't
                // running" when the check itself errored:
                //   stopped     -- graceful stop ran AND the stack is down
                //   stop_failed -- server was up, the stop did not take
                //   not_running -- nothing was up to stop
                //   unknown     -- server-detail could not be read
                let verdict = read_server_verdict(&runner);
                let outcome = match verdict.as_deref() {
                    Some(v) if verdict_needs_stop(Some(v)) => {
                        // Same CLI verb as the Home Stop button (bounded:
                        // saveall + compose stop -t 180). Judge it by effect
                        // -- run_captured is Ok on any CLI exit code.
                        let _ = runner.run_captured(&["games", "stop", LAN_TITLE]);
                        if stop_confirmed_down(&runner) {
                            "stopped"
                        } else {
                            "stop_failed"
                        }
                    }
                    Some(_) => "not_running",
                    None => "unknown",
                };
                let _ = app.emit(
                    "auto-shutdown",
                    serde_json::json!({"kind": "fired", "outcome": outcome}),
                );
                let _ = app
                    .emit("auto-shutdown", serde_json::json!({"kind": "state", "state": "waiting"}));
            }
            watch::WatchAction::None => {}
        }
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

// --- Realmlist check + one-click fix (Batch 2 F7) ---------------------------
//
// No paths cross IPC: all three commands resolve the realmlist location from
// the client path the module manager already stores (via the CLI's
// `wow client-path get`). The optional lan_ip is comparison/status data
// supplied by the frontend from its own `wow_lan status` parse -- validated
// here like every webview input, and deliberately NOT fetched CLI-side:
// `dml lan ... status` can block for minutes while the realm DB warms up,
// which would wedge run_captured (callers must self-bound, see runner.rs).

fn validated_lan_ip(lan_ip: Option<String>) -> Result<Option<String>, CmdError> {
    match lan_ip {
        None => Ok(None),
        Some(ip) => {
            if validate_ip(&ip) {
                Ok(Some(ip))
            } else {
                Err(bad_arg(format!("invalid IPv4 address: {ip:?}")))
            }
        }
    }
}

#[tauri::command]
async fn realmlist_status(
    lan_ip: Option<String>,
    state: State<'_, AppState>,
) -> Result<realmlist::RealmlistStatus, CmdError> {
    let lan = validated_lan_ip(lan_ip)?;
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || realmlist::status(&runner, lan))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

#[tauri::command]
async fn realmlist_fix(
    target: String,
    lan_ip: Option<String>,
    state: State<'_, AppState>,
) -> Result<realmlist::RealmlistStatus, CmdError> {
    if !realmlist::validate_realmlist_target(&target) {
        return Err(bad_arg(format!(
            "invalid realmlist target: {target:?} (allowed: 127.0.0.1, a private LAN address, or a hostname)"
        )));
    }
    let lan = validated_lan_ip(lan_ip)?;
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || realmlist::fix(&runner, &target, lan))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

#[tauri::command]
async fn realmlist_lock(
    locked: bool,
    lan_ip: Option<String>,
    state: State<'_, AppState>,
) -> Result<realmlist::RealmlistStatus, CmdError> {
    let lan = validated_lan_ip(lan_ip)?;
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || realmlist::lock(&runner, locked, lan))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Keep-awake toggle (Batch 2 F6): see power.rs for the per-thread semantics.
/// Sync + infallible on purpose -- the call is a channel send to the manager
/// thread, and there is nothing useful to report on failure (shutdown races
/// only).
#[tauri::command]
fn set_keep_awake(on: bool) {
    power::keep_awake(on);
}

/// Taskbar progress cue (Batch 4): flip the main window's taskbar button to
/// an indeterminate "busy" state while a long streamed op runs (rebuild /
/// flush / server-update / restart / backup), and clear it when done -- so a
/// minimized launcher still shows work is in flight. Best-effort and
/// infallible: a cosmetic hint must never disrupt the operation it decorates,
/// so a missing window or an unsupported platform is silently ignored. On
/// Windows this is the marquee taskbar state; Linux/macOS render what they
/// can (or nothing).
#[tauri::command]
fn set_taskbar_progress(active: bool, app: tauri::AppHandle) {
    use tauri::window::{ProgressBarState, ProgressBarStatus};
    let status = if active {
        ProgressBarStatus::Indeterminate
    } else {
        ProgressBarStatus::None
    };
    let win = app
        .get_webview_window("main")
        .or_else(|| app.webview_windows().into_values().next());
    if let Some(w) = win {
        let _ = w.set_progress_bar(ProgressBarState {
            status: Some(status),
            progress: None,
        });
    }
}

/// Enable/disable the auto-shutdown watcher. Enabling spawns a fresh watcher
/// thread (fresh = DISARMED until Wow.exe is seen); disabling just bumps the
/// generation so the running thread exits on its next wake. Idempotent from
/// the webview's perspective -- re-enabling while enabled restarts the
/// watcher cleanly.
#[tauri::command]
fn set_auto_shutdown(
    enabled: bool,
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let my_gen = {
        let mut ctl = state.auto_shutdown.lock().unwrap();
        ctl.generation += 1;
        ctl.enabled = enabled;
        ctl.generation
    };
    if enabled {
        let ctl = state.auto_shutdown.clone();
        let runner = state.runner.clone();
        std::thread::spawn(move || auto_shutdown_watcher(my_gen, ctl, runner, app));
    }
    Ok(())
}

#[tauri::command]
async fn dml_version(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["version"]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn games_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["games", "list"]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn games_status(id: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_json(&["games", "status", &id]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
        .and_then(envelope_to_result)
}

#[tauri::command]
async fn wow_accounts(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "accounts".into()]).await
}

#[tauri::command]
async fn wow_account_create(
    user: String,
    pass: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "account".into(), "create".into(), "--user".into(), user, "--pass".into(), pass],
    )
    .await
}

#[tauri::command]
async fn wow_account_set_password(
    user: String,
    pass: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "account".into(), "set-password".into(), "--user".into(), user, "--pass".into(), pass],
    )
    .await
}

#[tauri::command]
async fn wow_account_set_gm(
    user: String,
    level: u8,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "account".into(), "set-gm".into(), "--user".into(), user, "--level".into(), level.to_string()],
    )
    .await
}

#[tauri::command]
async fn wow_account_delete(
    user: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "account".into(), "delete".into(), "--user".into(), user],
    )
    .await
}

#[tauri::command]
async fn wow_server_info(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "server-info".into()]).await
}

#[tauri::command]
async fn wow_server_detail(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "server-detail".into()]).await
}

#[tauri::command]
async fn wow_stats(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "stats".into()]).await
}

#[tauri::command]
async fn wow_docker_usage(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "docker-usage".into()]).await
}

#[tauri::command]
async fn wow_docker_clean(
    level: u8,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "docker-clean".into(), "--level".into(), level.to_string()], on_event, state).await
}

#[tauri::command]
async fn wow_update_check(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "update-check".into()]).await
}

#[tauri::command]
async fn wow_server_update(
    backup: bool,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let flag = if backup { "--backup" } else { "--no-backup" };
    stream_args(vec!["wow".into(), "update".into(), flag.into()], on_event, state).await
}

#[tauri::command]
async fn wow_console_tail(
    lines: Option<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "console-tail".into()];
    if let Some(l) = lines {
        args.extend(["--lines".into(), l.to_string()]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_console_send(
    command: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "console-send".into(), "--command".into(), command]).await
}

#[tauri::command]
async fn wow_module_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "module".into(), "list".into()]).await
}

#[tauri::command]
async fn wow_commands(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "commands".into()]).await
}

#[tauri::command]
async fn wow_module_install(
    family: String,
    key: Option<String>,
    url: Option<String>,
    backup: Option<bool>,
    variant: Option<String>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "module".into(), "install".into(), "--family".into(), family];
    if let Some(k) = key {
        args.extend(["--key".into(), k]);
    }
    if let Some(u) = url {
        args.extend(["--url".into(), u]);
    }
    match backup {
        Some(true) => args.push("--backup".into()),
        Some(false) => args.push("--no-backup".into()),
        None => {}
    }
    if let Some(v) = variant {
        args.extend(["--variant".into(), v]);
    }
    stream_args(args, on_event, state).await
}

#[tauri::command]
async fn wow_module_remove(
    family: String,
    key: String,
    backup: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let mut args: Vec<String> =
        vec!["wow".into(), "module".into(), "remove".into(), "--family".into(), family, "--key".into(), key];
    match backup {
        Some(true) => args.push("--backup".into()),
        Some(false) => args.push("--no-backup".into()),
        None => {}
    }
    stream_args(args, on_event, state).await
}

#[tauri::command]
async fn wow_module_rebuild(
    backup: bool,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let flag = if backup { "--backup" } else { "--no-backup" };
    stream_args(vec!["wow".into(), "module".into(), "rebuild".into(), flag.into()], on_event, state).await
}

// Module-update round: per-module behind-count probe (fetches origin per
// installed cpp clone CLI-side, never mutates a worktree).
#[tauri::command]
async fn wow_module_update_check(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "module".into(), "update-check".into()]).await
}

// Module-update round: per-module source pull (patch backup + stash +
// ff-only pull + stash pop, no automatic rebuild). The CLI gates everything
// before any mutation -- key shape, mod-playerbots refusal, missing .git --
// so the key passes through as a plain argv value.
#[tauri::command]
async fn wow_module_update(
    key: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_args(
        vec!["wow".into(), "module".into(), "update".into(), "--key".into(), key],
        on_event,
        state,
    )
    .await
}

// Batch 5 F2: ARAC's server-DBC + client-MPQ patch step (CLI allowlists the
// key to mod-arac; passed through as a plain argv value).
#[tauri::command]
async fn wow_module_client_patch(
    key: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_args(
        vec!["wow".into(), "module".into(), "client-patch".into(), "--key".into(), key],
        on_event,
        state,
    )
    .await
}

#[tauri::command]
async fn wow_module_conf_activate(
    key: String,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "module".into(), "conf-activate".into(), "--key".into(), key];
    if force.unwrap_or(false) {
        args.push("--force".into());
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_module_tracking(
    key: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "module".into(), "tracking".into(), "--key".into(), key]).await
}

#[tauri::command]
async fn wow_module_repair(
    key: String,
    db: String,
    mode: String,
    files: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec![
        "wow".into(),
        "module".into(),
        "repair".into(),
        "--key".into(),
        key,
        "--db".into(),
        db,
        "--mode".into(),
        mode,
    ];
    if let Some(f) = files {
        args.extend(["--files".into(), f]);
    }
    run_json_cmd(state, args).await
}

// Batch 3 F13b: canned one-shot module fixes. Closed allowlist -- the CLI
// re-validates, but never forward an arbitrary string as an argv token.
#[tauri::command]
async fn wow_module_fixit(key: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    if key != "battlepass-npc" {
        return Err(bad_arg(format!("unknown fixit key: {key:?}")));
    }
    run_json_cmd(state, vec!["wow".into(), "module".into(), "fixit".into(), key]).await
}

// Batch 2 (overnight): spawn an installed NPC-mod's creature in both capitals
// from its ready-made coord block (CLI arm `module place-npc`). Closed
// allowlist -- the CLI re-validates against the same set, but never forward an
// arbitrary string as an argv token.
#[tauri::command]
async fn wow_module_place_npc(
    key: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    match key.as_str() {
        "mod-1v1-arena" | "mod-transmog" | "mod-npc-beastmaster" | "bmah" => {}
        _ => return Err(bad_arg(format!("unknown place-npc key: {key:?}"))),
    }
    run_json_cmd(
        state,
        vec!["wow".into(), "module".into(), "place-npc".into(), "--key".into(), key],
    )
    .await
}

#[tauri::command]
async fn wow_client_path_get(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "client-path".into(), "get".into()]).await
}

#[tauri::command]
async fn wow_client_path_set(
    path: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "client-path".into(), "set".into(), "--path".into(), path],
    )
    .await
}

#[tauri::command]
async fn wow_client_path_detect(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "client-path".into(), "detect".into()]).await
}

#[tauri::command]
async fn wow_items_search(
    name: String,
    quality: Option<u32>,
    min_level: Option<u32>,
    max_level: Option<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> =
        vec!["wow".into(), "items".into(), "search".into(), "--name".into(), name];
    if let Some(q) = quality {
        args.extend(["--quality".into(), q.to_string()]);
    }
    if let Some(l) = min_level {
        args.extend(["--min-level".into(), l.to_string()]);
    }
    if let Some(l) = max_level {
        args.extend(["--max-level".into(), l.to_string()]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_mail_item(
    to: String,
    items: String,
    subject: Option<String>,
    body: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> =
        vec!["wow".into(), "mail-item".into(), "--to".into(), to, "--items".into(), items];
    if let Some(s) = subject {
        args.extend(["--subject".into(), s]);
    }
    if let Some(b) = body {
        args.extend(["--body".into(), b]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_teleport_list(
    search: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "teleport-list".into()];
    if let Some(s) = search {
        args.extend(["--search".into(), s]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_teleport(
    char_name: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "teleport".into(), "--char".into(), char_name, "--to".into(), to],
    )
    .await
}

#[tauri::command]
async fn wow_paperdoll(
    char_name: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "paperdoll".into(), "--char".into(), char_name]).await
}

#[tauri::command]
async fn wow_item_info(
    entries: Vec<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let csv = entries.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(",");
    run_json_cmd(state, vec!["wow".into(), "item-info".into(), "--entries".into(), csv]).await
}

#[tauri::command]
async fn wow_char_progress(
    char_name: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "char-progress".into(), "--char".into(), char_name]).await
}

#[tauri::command]
async fn wow_achievements(
    char_name: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "achievements".into(), "--char".into(), char_name]).await
}

#[tauri::command]
async fn wow_entity_info(
    kind: String,
    ids: Vec<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let csv = ids.iter().map(|e| e.to_string()).collect::<Vec<_>>().join(",");
    run_json_cmd(state, vec!["wow".into(), "entity-info".into(), "--kind".into(), kind, "--ids".into(), csv]).await
}

#[tauri::command]
async fn wow_config_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "list".into()]).await
}

/// Which orchestration backend this process selected — `"native"` (Docker
/// Desktop, `DML_BACKEND=native`) or `"wsl"` (default). The single seam the
/// frontend router (T3) reads to decide whether to call the fast native
/// `wow_config_read` or the WSL `wow_config_list`. Cheap and pure: no spawn.
#[tauri::command]
fn backend_mode() -> &'static str {
    match crate::dml::backend::selected() {
        crate::dml::backend::Backend::Native => "native",
        crate::dml::backend::Backend::Wsl => "wsl",
    }
}

/// NATIVE-MODE fast read of the config settings: returns the SAME shape as
/// `wow_config_list` (`{"settings":[…66 rows…]}`) with zero bash/yq/fork on the
/// hot path. The static registry is fetched from the CLI ONCE per session (the
/// only subprocess here, and only on the first call) and cached; every live
/// `value` is then read directly off the runtime files in Rust (see
/// `dml::config`). Docker Desktop may be closed — these are pure file reads.
///
/// This command is for native mode only. In WSL mode the frontend keeps calling
/// `wow_config_list`; `backend_mode` tells it which to use.
#[tauri::command]
async fn wow_config_read(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    let cache = state.config_registry.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let rows = fetch_registry_rows(
            &runner,
            &cache,
            &["wow", "config", "registry"],
            "config registry",
        )?;
        let mut reader = crate::dml::config::ConfigReader::from_env();
        Ok(reader.assemble(&rows))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Fetch-once-and-cache a `…registry`/`…catalog` arm that returns
/// `.data.settings[]` as an array of rows. The lock is held across the one-time
/// CLI fetch so racing first-calls don't each spawn the CLI. Shared by
/// `wow_config_read`, `wow_tuning_read`, and the startup prefetch.
fn fetch_registry_rows(
    runner: &DmlRunner,
    cache: &Mutex<Option<Vec<serde_json::Value>>>,
    args: &[&str],
    label: &str,
) -> Result<Vec<serde_json::Value>, CmdError> {
    let mut guard = cache.lock().map_err(|_| CmdError {
        code: "INTERNAL".into(),
        message: format!("{label} cache poisoned"),
        hint: String::new(),
    })?;
    if let Some(rows) = guard.as_ref() {
        return Ok(rows.clone());
    }
    let env = runner.run_json(args).map_err(CmdError::from)?;
    let data = envelope_to_result(env)?;
    let rows = data
        .get("settings")
        .and_then(|v| v.as_array())
        .cloned()
        .ok_or_else(|| CmdError {
            code: "CLI_BAD_OUTPUT".into(),
            message: format!("{label} response missing settings[]"),
            hint: "Is the dml CLI up to date?".into(),
        })?;
    *guard = Some(rows.clone());
    Ok(rows)
}

/// Fetch-once-and-cache the module catalog `.data` object (families +
/// placeholders). Same locking contract as `fetch_registry_rows`.
fn fetch_catalog_data(
    runner: &DmlRunner,
    cache: &Mutex<Option<serde_json::Value>>,
) -> Result<serde_json::Value, CmdError> {
    let mut guard = cache.lock().map_err(|_| CmdError {
        code: "INTERNAL".into(),
        message: "module catalog cache poisoned".into(),
        hint: String::new(),
    })?;
    if let Some(data) = guard.as_ref() {
        return Ok(data.clone());
    }
    let env = runner.run_json(&["wow", "module", "catalog"]).map_err(CmdError::from)?;
    let data = envelope_to_result(env)?;
    if data.get("families").is_none() {
        return Err(CmdError {
            code: "CLI_BAD_OUTPUT".into(),
            message: "module catalog response missing families".into(),
            hint: "Is the dml CLI up to date (has `wow module catalog`)?".into(),
        });
    }
    *guard = Some(data.clone());
    Ok(data)
}

/// NATIVE-MODE fast read of the module-tuning settings: same shape as
/// `wow_config_tuning_list` (`{"settings":[…13 rows…]}`) with zero bash/fork on
/// the hot path. The static registry is fetched once per session and cached;
/// each row's `value` + `installed` are then read straight off the runtime
/// files in Rust (see `dml::tuning`). Native mode only — WSL keeps calling
/// `wow_config_tuning_list`.
#[tauri::command]
async fn wow_tuning_read(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    let cache = state.tuning_registry.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let rows = fetch_registry_rows(
            &runner,
            &cache,
            &["wow", "config", "tuning-registry"],
            "tuning registry",
        )?;
        let mut reader = crate::dml::tuning::TuningReader::from_env();
        Ok(reader.assemble(&rows))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the module list: same shape as `wow_module_list`
/// (`{families:{cpp,lua,sql}, rebuild_pending, ale_ready}`) with zero bash/fork
/// on the hot path (only LOCAL `git` reads for installed clones' head/date). The
/// static catalog is fetched once per session and cached; every dynamic field
/// is then filled from the runtime files in Rust (see `dml::modules`). Native
/// mode only — WSL keeps calling `wow_module_list`.
#[tauri::command]
async fn wow_module_read(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    let cache = state.module_catalog.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let catalog = fetch_catalog_data(&runner, &cache)?;
        let reader = crate::dml::modules::ModuleReader::from_env();
        Ok(reader.assemble(&catalog))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Map a native-mode [`crate::dml::db::DbError`] to the [`CmdError`] the frontend
/// already knows how to render. Both variants collapse to `DB_UNREACHABLE`,
/// matching the CLI: every one of these arms (`teleport-list` / `bots list` /
/// `accounts` / `paperdoll`) reports `DB_UNREACHABLE` for ANY `db_*_query`
/// failure in `90-main.sh` — the bash has no separate "connected but the query
/// itself failed" code path, so a native `DbError::Query` (e.g. a genuinely
/// malformed statement) must still read as `DB_UNREACHABLE` to stay
/// byte-identical to `dml`. Same collapse [`stats_err_to_cmd`] already does for
/// the `stats` arm — see its comment for the fuller rationale.
fn db_err_to_cmd(e: crate::dml::db::DbError) -> CmdError {
    CmdError {
        code: "DB_UNREACHABLE".into(),
        message: e.to_string(),
        hint: "Is ac-database running? (native mode reads MySQL directly on 127.0.0.1)".into(),
    }
}

/// Guard every native-mode DB command with the same backend check the frontend
/// router already uses to decide whether to CALL them: if a stale webview
/// (backend flipped, or a bug in the router) invokes a native-only command
/// while this process is on the WSL backend, refuse before ever opening a DB
/// socket rather than dialing 127.0.0.1 against a database that may not even
/// be the right one (finding #7).
fn require_native_backend() -> Result<(), CmdError> {
    if is_native_backend() {
        Ok(())
    } else {
        Err(CmdError {
            code: "WRONG_BACKEND".into(),
            message: "This command is native-mode-only".into(),
            hint: "The launcher is running in WSL mode; use the WSL sibling command instead."
                .into(),
        })
    }
}

/// NATIVE-MODE fast read of the teleport locations: same shape as
/// `wow_teleport_list` (`{"locations":[…≤500 rows…]}`), read over a direct MySQL
/// connection instead of `docker exec ac-database mysql`. Native mode only — WSL
/// keeps calling `wow_teleport_list`; `backend_mode` tells the frontend which.
#[tauri::command]
async fn wow_teleport_list_read(search: Option<String>) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = crate::dml::db::DbConfig::from_env();
        crate::dml::pages::read_teleport_list(&cfg, search.as_deref()).map_err(db_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of one bot-browser page: same shape as `wow_bots_list`
/// (`{total,limit,offset,bots:[…]}`), over a direct MySQL connection. The filter
/// args mirror `wow_bots_list` exactly (the frontend passes the same values);
/// `limit` is clamped `1..=200` (default 50) like the CLI so the echoed value
/// matches. Native mode only.
#[tauri::command]
async fn wow_bots_read(
    name: Option<String>,
    class: Option<u32>,
    min_level: Option<u32>,
    max_level: Option<u32>,
    online: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    // Validate BEFORE any SQL is built, matching the bash arm's doctrine
    // (90-main.sh ~3884-3894: `_valid_charname` on --name, an allowlist `case`
    // on --class) — this is native/WSL behavioral parity for invalid input,
    // not a defense the bound-parameter query builder in `dml::pages` already
    // needed (finding #2).
    if let Some(n) = name.as_deref().filter(|n| !n.is_empty()) {
        if !crate::dml::paperdoll::valid_charname(n) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Invalid name prefix: {n}"),
                hint: "1-12 letters/digits/underscore.".into(),
            });
        }
    }
    if let Some(c) = class {
        if !crate::dml::pages::valid_bot_class(c) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Invalid class id: {c}"),
                hint: "1-9 or 11.".into(),
            });
        }
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = crate::dml::db::DbConfig::from_env();
        let f = crate::dml::pages::BotFilters {
            name,
            class,
            min_level,
            max_level,
            online: online.unwrap_or(false),
            limit: crate::dml::pages::clamp_limit(limit),
            offset: offset.unwrap_or(0),
        };
        crate::dml::pages::read_bots(&cfg, &f).map_err(db_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the accounts list: same shape as `wow_accounts`
/// (`{"accounts":[{id,username,gm_level,characters}]}`), over a direct MySQL
/// connection. Native mode only — WSL keeps calling `wow_accounts`.
#[tauri::command]
async fn wow_accounts_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = crate::dml::db::DbConfig::from_env();
        crate::dml::pages::read_accounts(&cfg).map_err(db_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Map a native-mode stats [`crate::dml::db::DbError`] to a [`CmdError`] whose
/// code matches the CLI's `stats` arm: that arm reports `DB_UNREACHABLE` for
/// EVERY payload failure (including a query error on a reachable DB — see the
/// "honest hint" branch in 90-main.sh), so both DbError variants collapse to
/// `DB_UNREACHABLE` here to stay byte-identical to `dml wow stats`.
fn stats_err_to_cmd(e: crate::dml::db::DbError) -> CmdError {
    CmdError {
        code: "DB_UNREACHABLE".into(),
        message: e.to_string(),
        hint: "Is ac-database running? (native mode reads MySQL directly on 127.0.0.1)".into(),
    }
}

/// NATIVE-MODE fast read of the Statistics envelope: the SAME nested `data`
/// object `wow_stats` emits, assembled from 19 direct-MySQL queries (the 18
/// independent ones run concurrently) instead of `docker exec ac-database
/// mysql` × 19 + a per-row fork storm. Native mode only — WSL keeps calling
/// `wow_stats`; `backend_mode` tells the frontend which.
#[tauri::command]
async fn wow_stats_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = crate::dml::db::DbConfig::from_env();
        crate::dml::stats::read_stats(&cfg).map_err(stats_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of a character paperdoll: same shape as `wow_paperdoll`
/// (`{name,level,class,race,gender,skin,…,equipped:[…]}`), over a direct MySQL
/// connection (no SOAP saveall — see the reader's module header). Rejects an
/// invalid name with `BAD_ARG` and an unknown/gearless character with
/// `NOT_FOUND`, exactly like the CLI arm. Native mode only.
#[tauri::command]
async fn wow_paperdoll_read(char_name: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !crate::dml::paperdoll::valid_charname(&char_name) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid character name: {char_name}"),
            hint: String::new(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = crate::dml::db::DbConfig::from_env();
        match crate::dml::paperdoll::read_paperdoll(&cfg, &char_name).map_err(db_err_to_cmd)? {
            Some(v) => Ok(v),
            None => Err(CmdError {
                code: "NOT_FOUND".into(),
                message: format!("No such character or no equipped items: {char_name}"),
                hint: String::new(),
            }),
        }
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

#[tauri::command]
async fn wow_config_set(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "config".into(), "set".into(), "--key".into(), key, "--value".into(), value],
    )
    .await
}

#[tauri::command]
async fn wow_config_pb_keys(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "pb-keys".into()]).await
}

// Module-tuning rework: pb-keys generalized to any editable module conf.
// The CLI enforces the allowlist (module confs only; rejects .env/the
// compose override/worldserver.conf/authserver.conf) and enriches each key
// with its .dist comment-block help.
#[tauri::command]
async fn wow_config_conf_keys(
    file: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "config".into(), "conf-keys".into(), "--file".into(), file],
    )
    .await
}

#[tauri::command]
async fn wow_config_files(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "files".into()]).await
}

// Guided module tuning (overnight Batch 3): plain-JSON read/write of the
// curated activator knobs for a few optional modules (NPC Beastmaster + Learn
// Spells via their .conf; Unlimited Ammo + Sit Means Rest via their deployed
// ALE .lua). `list` reports each knob's value + whether its module is
// deployed; `set` writes one knob with the right backend.
#[tauri::command]
async fn wow_config_tuning_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "tuning-list".into()]).await
}

#[tauri::command]
async fn wow_config_tuning_set(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "config".into(), "tuning-set".into(), "--key".into(), key, "--value".into(), value],
    )
    .await
}

// Account-wide sharing configurator (overnight Batch 1): read/write the
// ENABLE_* flags in the deployed accountwide lua files. Both are plain-JSON
// (no streaming) -- get reports installed-state + per-subsystem on/off + the
// reputation pick-one block; set flips one flag (reputation takes an optional
// variant).
#[tauri::command]
async fn wow_accountwide_get(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "accountwide".into(), "get".into()]).await
}

#[tauri::command]
async fn wow_accountwide_set(
    key: String,
    value: String,
    variant: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec![
        "wow".into(),
        "accountwide".into(),
        "set".into(),
        "--key".into(),
        key,
        "--value".into(),
        value,
    ];
    if let Some(v) = variant {
        args.push("--variant".into());
        args.push(v);
    }
    run_json_cmd(state, args).await
}

// Flush & rebuild the ambient bot population. The typed "flush" ack is
// enforced CLI-side too -- this command always passes it, so the webview's
// own typed-confirm is the user-facing gate while the CLI contract keeps
// scripts honest.
#[tauri::command]
async fn wow_bots_flush(
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_args(
        vec!["wow".into(), "bots".into(), "flush".into(), "--yes".into(), "--ack".into(), "flush".into()],
        on_event,
        state,
    )
    .await
}

#[tauri::command]
async fn wow_config_raw_reset(
    file: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "raw-reset".into(), "--file".into(), file]).await
}

#[tauri::command]
async fn wow_config_raw_read(
    file: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "raw-read".into(), "--file".into(), file]).await
}

#[tauri::command]
async fn wow_config_raw_write(
    file: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runner.run_json_with_stdin(
            &["wow", "config", "raw-write", "--file", &file],
            &content,
        )
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map_err(CmdError::from)
    .and_then(envelope_to_result)
}

// Save-dialog + write in one command: the webview supplies only a suggested
// file name and the content -- the path comes exclusively from the native
// dialog, so a compromised webview cannot write to arbitrary locations.
// Sync command on purpose: Tauri runs it off the main thread, which
// blocking_save_file requires (it blocks until the main-thread dialog closes).
#[tauri::command]
fn save_text_file(app: tauri::AppHandle, default_name: String, content: String) -> Result<bool, String> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().set_file_name(&default_name).blocking_save_file();
    match picked {
        Some(p) => {
            let path = p.into_path().map_err(|e| e.to_string())?;
            std::fs::write(&path, content).map_err(|e| e.to_string())?;
            Ok(true)
        }
        None => Ok(false),
    }
}

async fn stream_action(
    action: &'static str,
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runner.run_stream(&["games", action, &id], |v| {
            let _ = on_event.send(v);
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map(|_exit| ())
    .map_err(CmdError::from)
}

async fn stream_args(
    args: Vec<String>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        runner.run_stream(&refs, |v| { let _ = on_event.send(v); })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map(|_| ())
    .map_err(CmdError::from)
}

#[tauri::command]
async fn wow_party_setup(on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "party-setup".into()], on_event, state).await
}

#[tauri::command]
async fn wow_party_online(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "online".into()]).await
}

// Batch 5 F5 follow-up: read-only dump of the LIVE premade specs parsed from
// the deployed playerbots.conf. Drives the launcher's spec picker AND (CLI
// side) _valid_bot_spec, so the picker can never offer a spec the validator
// would reject.
#[tauri::command]
async fn wow_party_specs(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "specs".into()]).await
}

// Batch 3 F11a: read-only who's-playing list for the Home card.
#[tauri::command]
async fn wow_players_online(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "players".into(), "online".into()]).await
}

// Batch 5 F1 (Bot Browser): read-only paged browse of the random bot
// population. Fixed argv skeleton; every optional filter appends a flag with
// its value as a separate argv entry (never string-interpolated) -- the CLI
// re-validates each one independently.
#[tauri::command]
async fn wow_bots_list(
    name: Option<String>,
    class: Option<u32>,
    min_level: Option<u32>,
    max_level: Option<u32>,
    online: Option<bool>,
    limit: Option<u32>,
    offset: Option<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "bots".into(), "list".into()];
    if let Some(n) = name {
        args.extend(["--name".into(), n]);
    }
    if let Some(c) = class {
        args.extend(["--class".into(), c.to_string()]);
    }
    if let Some(l) = min_level {
        args.extend(["--min-level".into(), l.to_string()]);
    }
    if let Some(l) = max_level {
        args.extend(["--max-level".into(), l.to_string()]);
    }
    if online.unwrap_or(false) {
        args.push("--online".into());
    }
    if let Some(l) = limit {
        args.extend(["--limit".into(), l.to_string()]);
    }
    if let Some(o) = offset {
        args.extend(["--offset".into(), o.to_string()]);
    }
    run_json_cmd(state, args).await
}

// Batch 3 F11f: fast world-only restart (does NOT apply settings changes --
// the CLI stream carries that caveat).
#[tauri::command]
async fn wow_world_restart(
    skip_saveall: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "world-restart".into()];
    if skip_saveall.unwrap_or(false) {
        args.push("--no-saveall".into());
    }
    stream_args(args, on_event, state).await
}

#[tauri::command]
async fn wow_party_add(player: String, class: String, gender: Option<String>, spec: Option<String>, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let mut a: Vec<String> = vec!["wow".into(),"party".into(),"add".into(),"--player".into(),player,"--class".into(),class];
    if let Some(g) = gender { a.extend(["--gender".into(), g]); }
    // Batch 5 F5: premade spec, passed as a plain argv value -- the CLI
    // closed-allowlists it (_valid_bot_spec).
    if let Some(s) = spec { a.extend(["--spec".into(), s]); }
    run_json_cmd(state, a).await
}

#[tauri::command]
async fn wow_party_list(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"list".into(),"--player".into(),player]).await
}

#[tauri::command]
async fn wow_party_kick(player: String, bot: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    // --player is the bot's master: the CLI kick now also whispers `logout`
    // as the master so the bot actually despawns (smoke-findings fix).
    run_json_cmd(state, vec!["wow".into(),"party".into(),"kick".into(),"--player".into(),player,"--bot".into(),bot]).await
}

#[tauri::command]
async fn wow_party_dismiss_all(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"dismiss-all".into(),"--player".into(),player]).await
}

#[tauri::command]
async fn wow_party_relogin(player: String, bot: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"relogin".into(),"--player".into(),player,"--bot".into(),bot]).await
}

#[tauri::command]
async fn wow_bridge_setup(on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "bridge-setup".into()], on_event, state).await
}

#[tauri::command]
async fn wow_gm_level(player: String, level: u32, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "gm".into(), "level".into(), "--player".into(), player, "--level".into(), level.to_string()],
    )
    .await
}

#[tauri::command]
async fn wow_gm_gold(player: String, gold: u32, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "gm".into(), "gold".into(), "--player".into(), player, "--gold".into(), gold.to_string()],
    )
    .await
}

#[tauri::command]
async fn wow_gm_heal(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "gm".into(), "heal".into(), "--player".into(), player]).await
}

#[tauri::command]
async fn wow_gm_revive(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "gm".into(), "revive".into(), "--player".into(), player]).await
}

#[tauri::command]
async fn wow_gm_summon(player: String, entry: u32, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "gm".into(), "summon".into(), "--player".into(), player, "--entry".into(), entry.to_string()],
    )
    .await
}

#[tauri::command]
async fn wow_teleport_coords(
    char_name: String,
    map: u32,
    x: f64,
    y: f64,
    z: f64,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec![
            "wow".into(),
            "teleport-coords".into(),
            "--char".into(),
            char_name,
            "--map".into(),
            map.to_string(),
            "--x".into(),
            x.to_string(),
            "--y".into(),
            y.to_string(),
            "--z".into(),
            z.to_string(),
        ],
    )
    .await
}

#[tauri::command]
async fn wow_gm_at_login(player: String, flag: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "gm".into(), "at-login".into(), "--player".into(), player, "--flag".into(), flag],
    )
    .await
}

// Batch 4 C: send a (possibly stuck) character to their hearth/home via the
// stock `.unstuck <name> inn` SOAP command. The name re-validates CLI-side
// and travels as its own argv token.
#[tauri::command]
async fn wow_gm_return_home(char_name: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "gm".into(), "return-home".into(), "--char".into(), char_name]).await
}

#[tauri::command]
async fn wow_party_preset_show(name: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "preset-show".into(), "--name".into(), name]).await
}

#[tauri::command]
async fn wow_party_preset_import(
    name: String,
    classes: String,
    force: Option<bool>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec![
        "wow".into(),
        "party".into(),
        "preset-import".into(),
        "--name".into(),
        name,
        "--classes".into(),
        classes,
    ];
    if force.unwrap_or(false) {
        args.push("--force".into());
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_party_botcmd(player: String, bot: String, action: String, spec: Option<String>, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let mut a: Vec<String> = vec!["wow".into(), "party".into(), "botcmd".into(), "--player".into(), player, "--bot".into(), bot, "--action".into(), action];
    // Batch 5 F5: required by the CLI when action == "spec", allowlisted there.
    if let Some(s) = spec { a.extend(["--spec".into(), s]); }
    run_json_cmd(state, a).await
}

#[tauri::command]
async fn wow_party_preset_save(player: String, name: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "party".into(), "preset-save".into(), "--player".into(), player, "--name".into(), name],
    )
    .await
}

#[tauri::command]
async fn wow_party_preset_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "preset-list".into()]).await
}

#[tauri::command]
async fn wow_party_preset_delete(name: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "preset-delete".into(), "--name".into(), name]).await
}

#[tauri::command]
async fn wow_party_preset_load(player: String, name: String, on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(
        vec!["wow".into(), "party".into(), "preset-load".into(), "--player".into(), player, "--name".into(), name],
        on_event,
        state,
    )
    .await
}

// Batch 4 F14: guided Auction House repair -- streams `wow ahbot repair`.
// The char name is re-validated CLI-side (^[A-Za-z0-9_]{1,12}$); it still
// travels as its own argv token, never through a shell.
#[tauri::command]
async fn wow_ahbot_repair(
    char_name: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_args(
        vec!["wow".into(), "ahbot".into(), "repair".into(), "--char".into(), char_name],
        on_event,
        state,
    )
    .await
}

#[tauri::command]
async fn wow_backup_create(include_world: Option<bool>, on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "backup".into(), "create".into()];
    if include_world.unwrap_or(false) {
        args.push("--include-world".into());
    }
    stream_args(args, on_event, state).await
}

#[tauri::command]
async fn wow_backup_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "backup".into(), "list".into()]).await
}

#[tauri::command]
async fn wow_backup_delete(file: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "backup".into(), "delete".into(), "--file".into(), file]).await
}

// Batch 4 A: verify a backup archive (gzip integrity + light SQL-sanity)
// before trusting it in a restore. Pure local file checks CLI-side -- the
// file name travels as its own argv token and is re-validated there.
#[tauri::command]
async fn wow_backup_validate(file: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "backup".into(), "validate".into(), "--file".into(), file]).await
}

#[tauri::command]
async fn wow_backup_restore(file: String, on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "backup".into(), "restore".into(), "--file".into(), file], on_event, state).await
}

#[tauri::command]
async fn wow_lan(
    action: String,
    ip: Option<String>,
    internet: Option<bool>,
    state: State<'_, AppState>,
) -> Result<String, CmdError> {
    if !LAN_ACTIONS.contains(&action.as_str()) {
        return Err(bad_arg(format!("invalid lan action: {action:?}")));
    }
    // Batch 4 F15: the internet-play stepper passes internet=true with
    // action "on" and a public IPv4 or hostname; every other combination
    // keeps the strict IPv4 shape check (and the CLI additionally enforces
    // private-only without --internet).
    let inet = internet.unwrap_or(false) && action == "on";
    let ip_arg = if action == "on" || action == "refresh" {
        let ip = ip.ok_or_else(|| bad_arg("ip is required for the on/refresh actions"))?;
        if inet {
            if !validate_host(&ip) {
                return Err(bad_arg(format!("invalid address or hostname: {ip:?}")));
            }
        } else if !validate_ip(&ip) {
            return Err(bad_arg(format!("invalid IPv4 address: {ip:?}")));
        }
        Some(ip)
    } else {
        None
    };
    let mut args: Vec<String> = vec!["lan".into(), LAN_TITLE.into()];
    if inet {
        args.push("--internet".into());
    }
    args.push(action);
    if let Some(ip) = ip_arg {
        args.push(ip);
    }
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        runner.run_captured(&refs)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map_err(CmdError::from)
}

/// Batch 4 F15: best-effort public-IP lookup (`wow lan public-ip`). The CLI
/// answers null instead of erroring when it can't tell.
#[tauri::command]
async fn wow_lan_public_ip(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "lan".into(), "public-ip".into()]).await
}

/// Batch 5 (overnight): Tailscale "Play Together" -- `wow tailscale
/// install|up|status|down`. The action arrives from the webview, so it is
/// checked against a closed allowlist before it becomes an argv token (same
/// doctrine as wow_lan). Each arm is a captured JSON envelope; `up` uses a
/// bounded `--timeout` CLI-side so this never hangs waiting on the browser
/// login.
#[tauri::command]
async fn wow_tailscale(action: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    if !TAILSCALE_ACTIONS.contains(&action.as_str()) {
        return Err(bad_arg(format!("invalid tailscale action: {action:?}")));
    }
    run_json_cmd(state, vec!["wow".into(), "tailscale".into(), action]).await
}

/// Batch 5 (overnight): LAN-readiness port diagnostic (`wow port-check`).
/// Read-only -- reports how Docker publishes the game/DB ports so the Tools
/// "Database access / LAN diagnostic" card can tell the user whether other
/// PCs can reach the server (and hand them the DB host port for HeidiSQL).
#[tauri::command]
async fn wow_port_check(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "port-check".into()]).await
}

#[tauri::command]
async fn dml_doctor(state: State<'_, AppState>) -> Result<String, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || runner.run_captured(&["doctor"]))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
        .map_err(CmdError::from)
}

/// Detached, no-wait: opens a terminal window for the user, it doesn't
/// report back through the command's return value. `wt.exe` (Windows
/// Terminal) is preferred; when it isn't on PATH (spawn fails), fall back to
/// a plain `cmd /c start` so a basic console window still opens. Distro/user
/// come from the same constants the runner's default WSL invocation uses --
/// see `dml::runner::{DISTRO, USER}` -- so this can never drift from where
/// every other command actually talks to.
#[tauri::command]
fn open_shell() -> Result<(), String> {
    use crate::dml::runner::{DISTRO, USER};
    let cwd = format!("/home/{USER}");
    let wsl_args = ["wsl", "-d", DISTRO, "-u", USER, "--cd", &cwd];
    if std::process::Command::new("wt.exe").args(wsl_args).spawn().is_ok() {
        return Ok(());
    }
    let mut cmd_args: Vec<&str> = vec!["/C", "start", "wsl"];
    cmd_args.extend_from_slice(&wsl_args[1..]);
    std::process::Command::new("cmd")
        .args(cmd_args)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// No packets are sent: connecting a UDP socket only makes the OS pick a
/// local route/address for that destination, which is enough to read back
/// this machine's LAN-facing IP without any traffic actually leaving.
/// 8.8.8.8:80 is just a stand-in destination on the public internet's
/// address space to force a real (non-loopback) route decision.
#[tauri::command]
fn detect_lan_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    Some(socket.local_addr().ok()?.ip().to_string())
}

#[tauri::command]
async fn tool_install(
    tool: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !TOOL_NAMES.contains(&tool.as_str()) {
        return Err(bad_arg(format!("invalid tool: {tool:?}")));
    }
    let runner = state.runner.clone();
    {
        let mut guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
        *guard = Some(InstallSlot::Starting);
    }
    let state_arc = state.install.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let mut child = match runner.spawn_interactive(&[tool.as_str()]) {
            Ok(c) => c,
            Err(e) => {
                *state_arc.lock().unwrap() = None;
                let _ = on_event.send(serde_json::json!({"event":"chunk","text": format!("failed to start: {e}\n")}));
                let _ = on_event.send(serde_json::json!({"event":"exit","code": -1}));
                return;
            }
        };
        let stdin = child.stdin.take().expect("stdin piped");
        let pid = child.id();
        *state_arc.lock().unwrap() = Some(InstallSlot::Running(InstallSession { stdin, pid }));
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = crate::dml::envelope::decode_wsl_output(&buf[..n]);
                    let _ = on_event.send(serde_json::json!({"event":"chunk","text": text}));
                }
                Err(_) => break,
            }
        }
        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
        *state_arc.lock().unwrap() = None;
        let _ = on_event.send(serde_json::json!({"event":"exit","code": code}));
    })
    .await
    .map_err(|e| CmdError { code: "IPC".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

/// Batch 4 F16: install a community title from a pasted git URL -- streams
/// the EXISTING interactive `dml run <url>` arm (clone + run the repo's own
/// install script). Same single global InstallSlot as games_install /
/// tool_install (deliberately the same body shape as those two): the
/// BUSY guard, stdin handoff for games_install_input, and
/// games_install_cancel's pid kill all work against this session unchanged.
#[tauri::command]
async fn url_install(
    url: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !validate_git_url(&url) {
        return Err(bad_arg(format!(
            "invalid install URL: {url:?} (a plain https git link, e.g. https://github.com/user/repo.git)"
        )));
    }
    let runner = state.runner.clone();
    {
        let mut guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
        *guard = Some(InstallSlot::Starting);
    }
    let state_arc = state.install.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let mut child = match runner.spawn_interactive(&["run", url.as_str()]) {
            Ok(c) => c,
            Err(e) => {
                *state_arc.lock().unwrap() = None;
                let _ = on_event.send(serde_json::json!({"event":"chunk","text": format!("failed to start: {e}\n")}));
                let _ = on_event.send(serde_json::json!({"event":"exit","code": -1}));
                return;
            }
        };
        let stdin = child.stdin.take().expect("stdin piped");
        let pid = child.id();
        *state_arc.lock().unwrap() = Some(InstallSlot::Running(InstallSession { stdin, pid }));
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = crate::dml::envelope::decode_wsl_output(&buf[..n]);
                    let _ = on_event.send(serde_json::json!({"event":"chunk","text": text}));
                }
                Err(_) => break,
            }
        }
        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
        *state_arc.lock().unwrap() = None;
        let _ = on_event.send(serde_json::json!({"event":"exit","code": code}));
    })
    .await
    .map_err(|e| CmdError { code: "IPC".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

// --- Windows disk & performance tools (Batch 4 F17) -------------------------
//
// All of these run on the WINDOWS side (native fs/registry/process), not
// through the distro: .wslconfig lives in the Windows user profile, and the
// shrink/restart flows manage WSL itself. Pure parse/merge logic lives in
// wslconfig.rs (cargo-tested); these commands only do the I/O.

#[derive(Debug, Serialize)]
pub struct WslConfigState {
    pub path: String,
    pub exists: bool,
    pub memory: Option<String>,
    pub processors: Option<String>,
}

fn wslconfig_path() -> Result<std::path::PathBuf, CmdError> {
    let profile = std::env::var("USERPROFILE")
        .map_err(|_| bad_arg("USERPROFILE is not set -- cannot locate .wslconfig"))?;
    Ok(std::path::PathBuf::from(profile).join(".wslconfig"))
}

/// Read .wslconfig into UTF-8 text, stripping a leading UTF-8 BOM and
/// rejecting a UTF-16-encoded file with a clear "save as UTF-8" message.
/// Absent file = empty content (the write side creates it). Shared by the
/// read-state and merge-write paths so both agree on encoding.
fn read_wslconfig_content(path: &std::path::Path) -> Result<String, CmdError> {
    if !path.is_file() {
        return Ok(String::new());
    }
    let bytes = std::fs::read(path)
        .map_err(|e| bad_arg(format!("could not read {}: {e}", path.display())))?;
    wslconfig::decode_wslconfig(&bytes).map_err(|_| {
        bad_arg(format!(
            "{} is not UTF-8 (looks like a UTF-16 file). Open it in Notepad and re-save it with encoding UTF-8, then try again.",
            path.display()
        ))
    })
}

fn read_wslconfig_state() -> Result<WslConfigState, CmdError> {
    let path = wslconfig_path()?;
    let exists = path.is_file();
    let content = read_wslconfig_content(&path)?;
    Ok(WslConfigState {
        path: path.to_string_lossy().into_owned(),
        exists,
        memory: wslconfig::read_wsl2_key(&content, "memory"),
        processors: wslconfig::read_wsl2_key(&content, "processors"),
    })
}

#[tauri::command]
fn wslconfig_read() -> Result<WslConfigState, CmdError> {
    read_wslconfig_state()
}

/// Merge memory/processors into [wsl2], preserving every unrelated
/// line/section. Only provided fields are written. Takes effect after WSL
/// restarts (the GUI says so).
#[tauri::command]
fn wslconfig_write(
    memory: Option<String>,
    processors: Option<String>,
) -> Result<WslConfigState, CmdError> {
    if memory.is_none() && processors.is_none() {
        return Err(bad_arg("nothing to write"));
    }
    if let Some(m) = &memory {
        if !wslconfig::valid_memory_spec(m) {
            return Err(bad_arg(format!("invalid memory value: {m:?} (use e.g. 8GB or 512MB)")));
        }
    }
    if let Some(p) = &processors {
        if !wslconfig::valid_processors_spec(p) {
            return Err(bad_arg(format!("invalid processors value: {p:?} (a whole number, e.g. 4)")));
        }
    }
    let path = wslconfig_path()?;
    // Same BOM-stripping / UTF-16-rejecting read as the state path: merging
    // into a silently-emptied UTF-16 file would drop the user's other keys.
    let mut content = read_wslconfig_content(&path)?;
    if let Some(m) = &memory {
        content = wslconfig::merge_wsl2_key(&content, "memory", m);
    }
    if let Some(p) = &processors {
        content = wslconfig::merge_wsl2_key(&content, "processors", p);
    }
    std::fs::write(&path, content)
        .map_err(|e| bad_arg(format!("could not write {}: {e}", path.display())))?;
    read_wslconfig_state()
}

/// Restart WSL: graceful server stop FIRST when the world is up (same CLI
/// verb as the Home Stop button, captured -- there is no terminal here),
/// then `wsl --shutdown`. The GUI's typed confirm explains the blast
/// radius: all WSL stops, next start is a cold start.
#[tauri::command]
async fn restart_wsl(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        // `wsl --shutdown` hard-kills every distro process, so any running
        // worldserver/mysqld would die WITHOUT a saveall. Run the graceful
        // stop first whenever the stack might be up. "crashed" counts (auth/
        // db are still running), and an UNREADABLE state also counts -- here
        // skipping the stop is fail-DANGEROUS (unlike the watcher, we go on
        // to power-kill the VM), so we attempt the stop rather than assume
        // it is safe to shut down.
        let verdict = read_server_verdict(&runner);
        let should_stop = match verdict.as_deref() {
            Some(v) => verdict_needs_stop(Some(v)),
            None => true, // don't know -> attempt the graceful stop anyway
        };
        // stopped_server is TRUE only when we ran the stop AND a follow-up
        // read confirms the stack is down -- an honest signal for the card's
        // "stopped gracefully" claim (run_captured alone is Ok on any exit).
        let stopped = if should_stop {
            let _ = runner.run_captured(&["games", "stop", LAN_TITLE]);
            stop_confirmed_down(&runner)
        } else {
            false
        };
        let mut cmd = std::process::Command::new("wsl");
        cmd.args(["--shutdown"]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        match cmd.output() {
            Ok(out) if out.status.success() => Ok(serde_json::json!({
                "shutdown": true,
                "stopped_server": stopped,
                // Lets the card distinguish "nothing was running" (attempted
                // false) from "tried to stop but could not confirm it went
                // down" (attempted true, stopped false) before wsl --shutdown
                // force-killed whatever was left.
                "stop_attempted": should_stop,
            })),
            Ok(out) => Err(bad_arg(format!(
                "wsl --shutdown failed (exit {:?})",
                out.status.code()
            ))),
            Err(e) => Err(bad_arg(format!("could not run wsl --shutdown: {e}"))),
        }
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Drop the shrink-disk PowerShell script into Downloads and open Explorer
/// there. NO elevation from the app -- the user right-clicks and runs it as
/// admin themselves (the script's header says so too).
#[tauri::command]
fn generate_compact_script() -> Result<String, CmdError> {
    use crate::dml::runner::DISTRO;
    let profile = std::env::var("USERPROFILE")
        .map_err(|_| bad_arg("USERPROFILE is not set -- cannot locate Downloads"))?;
    let dir = std::path::PathBuf::from(profile).join("Downloads");
    let path = dir.join("dml-shrink-wsl-disk.ps1");
    std::fs::create_dir_all(&dir)
        .map_err(|e| bad_arg(format!("could not create {}: {e}", dir.display())))?;
    std::fs::write(&path, wslconfig::compact_script(DISTRO))
        .map_err(|e| bad_arg(format!("could not write {}: {e}", path.display())))?;
    let select = format!("/select,{}", path.display());
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(&select);
    let _ = cmd.spawn(); // best-effort -- the returned path is shown either way
    Ok(path.to_string_lossy().into_owned())
}

/// Batch 5 (overnight): drop the "expose MySQL to LAN" PowerShell script into
/// Downloads and open Explorer at it -- same generate-and-run-as-admin flow as
/// the shrink-disk script (NO elevation from the app; the user right-clicks
/// and runs it as admin, and the header shouts the LAN-only warning). `port`
/// is the DB host port from the port-check diagnostic (3306, or the remapped
/// value like 13306); validated to a real TCP port before it reaches the
/// generated script.
#[tauri::command]
fn generate_mysql_proxy_script(port: Option<u32>) -> Result<String, CmdError> {
    let p = port.unwrap_or(3306);
    if p == 0 || p > 65535 {
        return Err(bad_arg(format!("invalid port: {p} (expected 1-65535)")));
    }
    let profile = std::env::var("USERPROFILE")
        .map_err(|_| bad_arg("USERPROFILE is not set -- cannot locate Downloads"))?;
    let dir = std::path::PathBuf::from(profile).join("Downloads");
    let path = dir.join("dml-expose-mysql.ps1");
    std::fs::create_dir_all(&dir)
        .map_err(|e| bad_arg(format!("could not create {}: {e}", dir.display())))?;
    std::fs::write(&path, wslconfig::mysql_expose_script(p as u16))
        .map_err(|e| bad_arg(format!("could not write {}: {e}", path.display())))?;
    let select = format!("/select,{}", path.display());
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(&select);
    let _ = cmd.spawn(); // best-effort -- the returned path is shown either way
    Ok(path.to_string_lossy().into_owned())
}

/// Read-only Defender-exclusion hint: locate the distro's disk folder via
/// the Lxss registry (reg.exe, no elevation needed for HKCU reads) and
/// build the copyable Add-MpPreference command. Everything degrades to
/// null -- the card then shows generic instructions instead.
#[tauri::command]
fn defender_hint() -> Result<serde_json::Value, CmdError> {
    use crate::dml::runner::DISTRO;
    let mut cmd = std::process::Command::new("reg");
    cmd.args([
        "query",
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Lxss",
        "/s",
    ]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let base = cmd
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| {
            wslconfig::parse_lxss_base_path(&String::from_utf8_lossy(&o.stdout), DISTRO)
        })
        .map(|b| b.trim_start_matches(r"\\?\").to_string());
    let command = base
        .as_ref()
        .map(|b| format!("Add-MpPreference -ExclusionPath \"{b}\""));
    Ok(serde_json::json!({"vhdx_dir": base, "command": command}))
}

// --- Native-mode setup bootstrap (spike/docker-desktop-native) -------------
//
// The Tools "Native setup" card checks (read-only) and one-click-fixes what
// native mode needs: a running Docker Desktop engine, the `yq` binary, the SOAP
// creds file, and (optionally) Defender exclusions for fork speed. The status
// probe is read-only; the three mutating fixes (yq download, soap copy,
// Defender-script generation) are gated FRONTEND-side behind the native-setup
// feature lock, matching how every other mutating Tools action is locked (the
// commands themselves don't re-check the flag — the buttons disable).

/// True when this process selected the native (Docker Desktop) backend.
fn is_native_backend() -> bool {
    crate::dml::backend::selected() == crate::dml::backend::Backend::Native
}

/// Where native mode expects the `yq` exe: `DML_YQ_BIN` verbatim if set, else
/// `<DML_GAMES_DIR>\tools\yq.exe`, else `<USERPROFILE>\dml-native\tools\yq.exe`.
fn yq_target_path() -> std::path::PathBuf {
    if let Some(p) = std::env::var_os("DML_YQ_BIN") {
        if !p.is_empty() {
            return std::path::PathBuf::from(p);
        }
    }
    let base = std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("USERPROFILE").map(|u| std::path::PathBuf::from(u).join("dml-native"))
        })
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    base.join("tools").join("yq.exe")
}

/// The Windows-home `~/.dml/soap.env` path the copy action writes to.
fn windows_soap_env_path() -> Option<std::path::PathBuf> {
    std::env::var_os("USERPROFILE")
        .map(|u| std::path::PathBuf::from(u).join(".dml").join("soap.env"))
}

/// Whether `docker info` succeeds against the resolved native docker.exe — the
/// definition of "the engine is running". A missing docker.exe, a down engine
/// or any non-zero exit all read as not-running.
fn docker_info_ok(program: &std::ffi::OsStr) -> bool {
    let mut cmd = std::process::Command::new(program);
    // `--format` keeps it fast and tiny; info talks to the engine over the
    // named pipe and needs no credential helper, so PATH is left untouched.
    cmd.args(["info", "--format", "{{.ServerVersion}}"]);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::null());
    cmd.stderr(std::process::Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    matches!(cmd.status(), Ok(s) if s.success())
}

/// Whether a WSL distro named `distro` is registered (`wsl --list --quiet`).
/// Used to decide whether the "Copy SOAP credentials" action can offer to pull
/// the file out of the distro. Any failure reads as "not available".
fn wsl_distro_present(distro: &str) -> bool {
    let mut cmd = std::process::Command::new("wsl.exe");
    cmd.args(["--list", "--quiet"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.output() {
        Ok(o) if o.status.success() => {
            let text = crate::dml::envelope::decode_wsl_output(&o.stdout);
            text.lines().any(|l| l.trim() == distro)
        }
        _ => false,
    }
}

/// Read-only aggregate the Native-setup card loads on mount: which backend is
/// active plus the pass/fail of each native-mode prerequisite. Never mutates.
#[tauri::command]
fn native_setup_status() -> Result<serde_json::Value, CmdError> {
    let docker_prog = crate::dml::native::docker_program();
    let docker_running = docker_info_ok(&docker_prog);
    let dp = std::path::Path::new(&docker_prog);
    let docker_path =
        (dp.is_absolute() && dp.exists()).then(|| docker_prog.to_string_lossy().into_owned());

    let yq_path = yq_target_path();
    let yq_present = yq_path.is_file();

    let soap = windows_soap_env_path();
    let soap_present = soap.as_ref().map(|p| p.is_file()).unwrap_or(false);
    let soap_path_str = soap
        .as_ref()
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_default();
    let distro_available = wsl_distro_present(crate::dml::runner::DISTRO);

    Ok(serde_json::json!({
        "native": is_native_backend(),
        "docker": { "running": docker_running, "path": docker_path },
        "yq": { "present": yq_present, "path": yq_path.to_string_lossy() },
        "soap": {
            "present": soap_present,
            "path": soap_path_str,
            "distro_available": distro_available,
        },
    }))
}

/// Launch the Docker Desktop app (the fix for a down engine). Just starts the
/// GUI exe — no elevation, no engine wait; the card tells the user to re-check
/// once the engine settles. Errors when Docker Desktop isn't installed.
#[tauri::command]
fn start_docker_desktop() -> Result<serde_json::Value, CmdError> {
    let exe = crate::dml::native::docker_desktop_program().ok_or_else(|| {
        bad_arg("Could not find Docker Desktop.exe -- is Docker Desktop installed?")
    })?;
    let mut cmd = std::process::Command::new(&exe);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.spawn()
        .map_err(|e| bad_arg(format!("could not launch Docker Desktop: {e}")))?;
    Ok(serde_json::json!({ "launched": true, "path": exe.to_string_lossy() }))
}

/// Download the pinned mikefarah `yq` Windows exe into the native tools dir and
/// verify it is a plausible size (>1MB) before saving. Written tmp-then-rename
/// so a half-download never leaves a truncated yq.exe at the target. LOCKED
/// frontend-side behind the native-setup flag.
#[tauri::command]
async fn native_yq_install() -> Result<serde_json::Value, CmdError> {
    let target = yq_target_path();
    tauri::async_runtime::spawn_blocking(move || {
        let url = crate::nativesetup::yq_download_url();
        // Default redirect policy is deliberate: the GitHub release URL 302s to
        // objects.githubusercontent.com, so (unlike the zam client) we must
        // FOLLOW redirects to reach the asset bytes.
        let client = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|e| bad_arg(format!("could not build HTTP client: {e}")))?;
        let resp = client
            .get(&url)
            .send()
            .map_err(|e| bad_arg(format!("download failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(bad_arg(format!(
                "download failed: HTTP {}",
                resp.status().as_u16()
            )));
        }
        let bytes = resp
            .bytes()
            .map_err(|e| bad_arg(format!("download read failed: {e}")))?;
        if (bytes.len() as u64) < crate::nativesetup::YQ_MIN_BYTES {
            return Err(bad_arg(format!(
                "downloaded file is only {} bytes -- expected the yq exe (>1MB); not saving",
                bytes.len()
            )));
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| bad_arg(format!("could not create {}: {e}", parent.display())))?;
        }
        let tmp = target.with_extension("exe.download");
        std::fs::write(&tmp, &bytes)
            .map_err(|e| bad_arg(format!("could not write {}: {e}", tmp.display())))?;
        std::fs::rename(&tmp, &target)
            .map_err(|e| bad_arg(format!("could not place {}: {e}", target.display())))?;
        Ok(serde_json::json!({
            "installed": true,
            "path": target.to_string_lossy(),
            "bytes": bytes.len(),
        }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Copy the SOAP creds file out of the dml-arch distro to the Windows home,
/// stripping CRs. Only meaningful when the distro exists (the card only offers
/// the button then). LOCKED frontend-side behind the native-setup flag.
#[tauri::command]
async fn native_soap_copy() -> Result<serde_json::Value, CmdError> {
    let dest = windows_soap_env_path()
        .ok_or_else(|| bad_arg("USERPROFILE is not set -- cannot locate the Windows home"))?;
    tauri::async_runtime::spawn_blocking(move || {
        let mut cmd = std::process::Command::new("wsl.exe");
        cmd.args([
            "-d",
            crate::dml::runner::DISTRO,
            "-u",
            crate::dml::runner::USER,
            "--",
            "cat",
            "/home/dml/.dml/soap.env",
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        let out = cmd
            .output()
            .map_err(|e| bad_arg(format!("could not run wsl cat: {e}")))?;
        if !out.status.success() {
            let err = crate::dml::envelope::decode_wsl_output(&out.stderr);
            return Err(bad_arg(format!(
                "could not read soap.env from the {} distro: {}",
                crate::dml::runner::DISTRO,
                err.trim()
            )));
        }
        let cleaned = crate::nativesetup::strip_cr(&crate::dml::envelope::decode_wsl_output(
            &out.stdout,
        ));
        if cleaned.trim().is_empty() {
            return Err(bad_arg("soap.env in the distro is empty -- nothing to copy"));
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| bad_arg(format!("could not create {}: {e}", parent.display())))?;
        }
        std::fs::write(&dest, cleaned.as_bytes())
            .map_err(|e| bad_arg(format!("could not write {}: {e}", dest.display())))?;
        Ok(serde_json::json!({ "copied": true, "path": dest.to_string_lossy() }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// The Git install root to exclude — `DML_BASH`'s Git root if that resolves,
/// else the standard install dir.
fn git_install_dir() -> String {
    if let Some(b) = std::env::var_os("DML_BASH") {
        if !b.is_empty() {
            if let Some(root) = crate::nativesetup::git_root_from_bash(&b.to_string_lossy()) {
                return root;
            }
        }
    }
    r"C:\Program Files\Git".to_string()
}

/// The Docker Desktop bin dir (holding docker.exe) to exclude, when docker
/// resolved to an absolute path.
fn docker_bin_dir_for_exclusion() -> Option<String> {
    let prog = crate::dml::native::docker_program();
    let p = std::path::Path::new(&prog);
    if p.is_absolute() {
        p.parent().map(|d| d.to_string_lossy().into_owned())
    } else {
        None
    }
}

/// Drop the elevated Defender-exclusion PowerShell script into Downloads and
/// open Explorer at it — same generate-and-run-as-admin flow as the shrink /
/// mysql-expose scripts (NO elevation from the app; the header shouts the
/// right-click-run-as-admin instruction and the WHY). Excludes Git, the Docker
/// Desktop bin dir, and the games dir. LOCKED frontend-side behind the
/// native-setup flag.
#[tauri::command]
fn native_defender_script() -> Result<String, CmdError> {
    let profile = std::env::var("USERPROFILE")
        .map_err(|_| bad_arg("USERPROFILE is not set -- cannot locate Downloads"))?;
    let mut paths: Vec<String> = vec![git_install_dir()];
    if let Some(d) = docker_bin_dir_for_exclusion() {
        paths.push(d);
    }
    if let Some(g) = std::env::var_os("DML_GAMES_DIR").filter(|s| !s.is_empty()) {
        paths.push(g.to_string_lossy().into_owned());
    }
    let refs: Vec<&str> = paths.iter().map(String::as_str).filter(|s| !s.is_empty()).collect();

    let dir = std::path::PathBuf::from(&profile).join("Downloads");
    let path = dir.join("dml-native-defender-exclusions.ps1");
    std::fs::create_dir_all(&dir)
        .map_err(|e| bad_arg(format!("could not create {}: {e}", dir.display())))?;
    std::fs::write(&path, crate::nativesetup::defender_exclusion_script(&refs))
        .map_err(|e| bad_arg(format!("could not write {}: {e}", path.display())))?;
    let select = format!("/select,{}", path.display());
    let mut cmd = std::process::Command::new("explorer");
    cmd.arg(&select);
    let _ = cmd.spawn(); // best-effort -- the returned path is shown either way
    Ok(path.to_string_lossy().into_owned())
}

// --- Enrichment-cache maintenance (Batch 6 C) ------------------------------
//
// Two runtime caches, on two filesystems:
//  * zam-cache -- Windows-side 3D-model + icon bytes under app_cache_dir();
//    measured/wiped directly in Rust (crate::zam).
//  * wowhead-cache -- WSL-side item tooltip/icon JSON under ~/.dml; measured/
//    wiped by the `dml wow cache-status|cache-clean` CLI verbs.
// Committed datasets (talent-trees-wotlk.json, achievements-wotlk.json) are
// bundled into the binary, live on NO cache path, and are never touched.

/// Native 3D-meta probe for the model pre-flight. The WebView's fetch()
/// cannot distinguish a clean upstream 404 from a network failure for
/// custom-scheme responses on Windows (WebView2 surfaces non-2xx scheme
/// responses as load errors), which silently poisoned every item probe --
/// robes/weapons were kept on best-guess slots and the engine dropped them
/// invisibly. This asks reqwest directly and warms the shared cache on hit.
#[tauri::command]
async fn zam_probe(path: String, app: tauri::AppHandle) -> Result<serde_json::Value, CmdError> {
    let root = app.path().app_cache_dir().map_err(|e| CmdError {
        code: "CACHE_DIR".into(),
        message: e.to_string(),
        hint: "Could not locate the app cache directory.".into(),
    })?;
    let out = tauri::async_runtime::spawn_blocking(move || {
        match crate::zam::zam_probe_upstream(&root, &path) {
            crate::zam::ProbeOutcome::Hit(bytes) => serde_json::json!({
                "status": "hit",
                "inventoryType": crate::zam::parse_inventory_type(&bytes),
            }),
            crate::zam::ProbeOutcome::Miss => serde_json::json!({ "status": "miss" }),
            crate::zam::ProbeOutcome::Err => serde_json::json!({ "status": "err" }),
        }
    })
    .await
    .map_err(|e| CmdError {
        code: "INTERNAL".into(),
        message: e.to_string(),
        hint: String::new(),
    })?;
    Ok(out)
}

/// Size report for the Windows-side zam model/icon cache.
#[tauri::command]
fn zam_cache_status(app: tauri::AppHandle) -> Result<serde_json::Value, CmdError> {
    let root = app.path().app_cache_dir().map_err(|e| CmdError {
        code: "CACHE_DIR".into(),
        message: e.to_string(),
        hint: "Could not locate the app cache directory.".into(),
    })?;
    let (bytes, files) = crate::zam::zam_cache_report(&root);
    let path = root.join(crate::zam::ZAM_CACHE_DIR);
    Ok(serde_json::json!({
        "key": "models",
        "label": "3D models & icons (this PC)",
        "path": path.to_string_lossy(),
        "present": path.exists(),
        "bytes": bytes,
        "files": files,
    }))
}

/// Wipe the Windows-side zam model/icon cache. Only ever removes the
/// `<app_cache_dir>/zam-cache` subdir (path built from a fixed constant).
#[tauri::command]
fn zam_cache_clear(app: tauri::AppHandle) -> Result<serde_json::Value, CmdError> {
    let root = app.path().app_cache_dir().map_err(|e| CmdError {
        code: "CACHE_DIR".into(),
        message: e.to_string(),
        hint: "Could not locate the app cache directory.".into(),
    })?;
    let freed = crate::zam::zam_cache_clear(&root).map_err(|e| CmdError {
        code: "WIPE_FAILED".into(),
        message: e.to_string(),
        hint: "The cache could not be removed -- close other app windows and retry.".into(),
    })?;
    Ok(serde_json::json!({"cleared": true, "freed_bytes": freed}))
}

/// Size report for the WSL-side item tooltip/icon cache (via the CLI).
#[tauri::command]
async fn wow_cache_status(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "cache-status".into()]).await
}

/// Wipe the WSL-side item tooltip/icon cache (via the CLI). The CLI asserts
/// its target ends in /.dml/wowhead-cache before any rm -rf.
#[tauri::command]
async fn wow_cache_clean(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "cache-clean".into()]).await
}

#[tauri::command]
async fn games_catalog(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["games".into(), "catalog".into()]).await
}

#[tauri::command]
async fn games_install(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let runner = state.runner.clone();
    {
        let mut guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
        *guard = Some(InstallSlot::Starting);
    }
    let state_arc = state.install.clone();
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let mut child = match runner.spawn_interactive(&["games", "install", &id]) {
            Ok(c) => c,
            Err(e) => {
                *state_arc.lock().unwrap() = None;
                let _ = on_event.send(serde_json::json!({"event":"chunk","text": format!("failed to start: {e}\n")}));
                let _ = on_event.send(serde_json::json!({"event":"exit","code": -1}));
                return;
            }
        };
        let stdin = child.stdin.take().expect("stdin piped");
        let pid = child.id();
        *state_arc.lock().unwrap() = Some(InstallSlot::Running(InstallSession { stdin, pid }));
        let mut stdout = child.stdout.take().expect("stdout piped");
        let mut buf = [0u8; 4096];
        loop {
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let text = crate::dml::envelope::decode_wsl_output(&buf[..n]);
                    let _ = on_event.send(serde_json::json!({"event":"chunk","text": text}));
                }
                Err(_) => break,
            }
        }
        let code = child.wait().ok().and_then(|s| s.code()).unwrap_or(-1);
        *state_arc.lock().unwrap() = None;
        let _ = on_event.send(serde_json::json!({"event":"exit","code": code}));
    })
    .await
    .map_err(|e| CmdError { code: "IPC".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

#[tauri::command]
async fn games_install_input(text: String, state: State<'_, AppState>) -> Result<(), CmdError> {
    use std::io::Write;
    let mut guard = state.install.lock().unwrap();
    match guard.as_mut() {
        Some(InstallSlot::Running(sess)) => sess
            .stdin
            .write_all(format!("{text}\n").as_bytes())
            .map_err(|e| CmdError { code: "STDIN".into(), message: e.to_string(), hint: String::new() }),
        Some(InstallSlot::Starting) | None => Err(CmdError {
            code: "NO_SESSION".into(),
            message: "No install is running".into(),
            hint: String::new(),
        }),
    }
}

#[tauri::command]
async fn games_install_cancel(state: State<'_, AppState>) -> Result<(), CmdError> {
    let pid = {
        let guard = state.install.lock().unwrap();
        match guard.as_ref() {
            Some(InstallSlot::Running(s)) => s.pid,
            Some(InstallSlot::Starting) | None => {
                return Err(CmdError {
                    code: "NO_SESSION".into(),
                    message: "No install is running".into(),
                    hint: String::new(),
                })
            }
        }
    };
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/F", "/T", "/PID", &pid.to_string()]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd.output()
        .map_err(|e| CmdError { code: "KILL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

#[tauri::command]
async fn games_remove(
    id: String,
    keep_data: Option<bool>,
    remove_images: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let mut args: Vec<String> = vec!["games".into(), "remove".into(), id, "--yes".into()];
    // Batch 3 F13c: preserve the ~6 GB client-data volume for reinstalls.
    if keep_data.unwrap_or(false) {
        args.push("--keep-data".into());
    }
    // Batch 6 B: also delete the AzerothCore/MySQL images (~3-5 GB).
    if remove_images.unwrap_or(false) {
        args.push("--remove-images".into());
    }
    stream_args(args, on_event, state).await
}

// ---------------------------------------------------------------------------
// Native-mode Docker Desktop engine lifecycle around start/stop.
//
// In native mode the Docker Desktop engine (and its docker-desktop WSL VM) must
// be up before any `docker compose` runs, so `games_start` ensures it first;
// and stopping it on `games_stop` frees the VM's RAM, so `games_stop` shuts it
// down afterwards when the (default-on) `nativeManageDocker` toggle is set. WSL
// mode does neither — it is byte-for-byte unchanged. The pure decision/poll
// logic lives in `dml::native`; these wrappers supply the real docker spawns,
// wall-clock sleeps, and the NDJSON progress stream (envelope `line`/`error`
// events, the same shape `dml games start/stop` emits).
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
fn ensure_engine_up_blocking(emit: impl Fn(serde_json::Value)) -> Result<(), CmdError> {
    use crate::dml::native;
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

/// Async wrapper: ensure the engine is up before a native start. Aborts (Err)
/// when it cannot be brought up.
async fn ensure_engine_up(on_event: &Channel<serde_json::Value>) -> Result<(), CmdError> {
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_engine_up_blocking(|v| { let _ = ch.send(v); })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Best-effort `docker desktop stop` after a native stop: stops the engine +
/// its docker-desktop WSL VM to free RAM. A failure emits a warning `line` but
/// never fails the server-stop. Blocking — run under `spawn_blocking`.
fn stop_engine_blocking(emit: impl Fn(serde_json::Value)) {
    use crate::dml::native;
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

/// Async wrapper: best-effort stop of the Docker Desktop engine after a native
/// server-stop. Never returns an error — the server-stop result stands.
async fn stop_engine_best_effort(on_event: &Channel<serde_json::Value>) {
    let ch = on_event.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        stop_engine_blocking(|v| { let _ = ch.send(v); })
    })
    .await;
}

#[tauri::command]
async fn games_start(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    // NATIVE MODE: the engine is a hard prerequisite (regardless of the manage
    // toggle) — bring it up first, or abort before touching compose. WSL mode
    // skips this entirely and behaves exactly as before.
    if is_native_backend() {
        ensure_engine_up(&on_event).await?;
    }
    stream_action("start", id, on_event, state).await
}

#[tauri::command]
async fn games_stop(
    id: String,
    manage_docker: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    // Decide up front whether we also stop the engine (native + toggle-on).
    // The frontend passes the persisted `toolPrefs.manageDocker` preference
    // (Tools page, "Stop Docker Desktop when the server stops" — default
    // checked); `None` (a caller that omits it) still defaults ON, same as
    // the checkbox's default, so nothing regresses if a call site is ever
    // added that doesn't thread the toggle through.
    let stop_docker = crate::dml::native::stop_engine_enabled(is_native_backend(), manage_docker);
    // Stop the server exactly as today (clone the channel so we can keep
    // streaming the engine-stop afterwards).
    let result = stream_action("stop", id, on_event.clone(), state).await;
    // Then free the VM's RAM by stopping the engine. Best-effort: this runs
    // even if the server-stop reported an error (the containers die with the
    // engine anyway), and its own failure only warns — `result` is what the
    // command returns.
    if stop_docker {
        stop_engine_best_effort(&on_event).await;
    }
    result
}

#[tauri::command]
async fn games_restart(
    id: String,
    skip_saveall: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    // --no-saveall = the GUI's "faster restart" option (skip the redundant
    // pre-stop saveall; the graceful stop still saves on shutdown).
    let mut args: Vec<String> = vec!["games".into(), "restart".into(), id];
    if skip_saveall.unwrap_or(false) {
        args.push("--no-saveall".into());
    }
    stream_args(args, on_event, state).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            // Backend switch (spike/docker-desktop-native): default WSL, or the
            // native Docker-Desktop path when DML_BACKEND=native. Same dml brain
            // either way — native just hosts it on Windows against Docker Desktop.
            runner: std::sync::Arc::new(DmlRunner::for_backend(
                crate::dml::backend::selected(),
            )),
            install: Arc::new(Mutex::new(None)),
            auto_shutdown: Arc::new(Mutex::new(AutoShutdownCtl { generation: 0, enabled: false })),
            config_registry: Arc::new(Mutex::new(None)),
            tuning_registry: Arc::new(Mutex::new(None)),
            module_catalog: Arc::new(Mutex::new(None)),
        })
        .setup(|app| {
            // Startup registry prefetch (native mode only): warm the three
            // static caches (config + tuning + module catalog) off the main
            // thread so the first Settings/Tuning/Modules open pays no
            // one-time CLI-fetch wait. NON-BLOCKING and best-effort — every
            // error is swallowed (the native files may be absent, Docker may
            // be closed), and it must never delay or panic app startup. In WSL
            // mode it does nothing: those pages keep calling the CLI directly.
            if crate::dml::backend::selected() == crate::dml::backend::Backend::Native {
                let state = app.state::<AppState>();
                let runner = state.runner.clone();
                let config_cache = state.config_registry.clone();
                let tuning_cache = state.tuning_registry.clone();
                let catalog_cache = state.module_catalog.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = tauri::async_runtime::spawn_blocking(move || {
                        let _ = fetch_registry_rows(
                            &runner,
                            &config_cache,
                            &["wow", "config", "registry"],
                            "config registry",
                        );
                        let _ = fetch_registry_rows(
                            &runner,
                            &tuning_cache,
                            &["wow", "config", "tuning-registry"],
                            "tuning registry",
                        );
                        let _ = fetch_catalog_data(&runner, &catalog_cache);
                    })
                    .await;
                });
            }
            Ok(())
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // Batch 3 F10: Windows toast for the "Azeroth is ready" moment --
        // JS side sends via @tauri-apps/plugin-notification (best-effort).
        .plugin(tauri_plugin_notification::init())
        .register_asynchronous_uri_scheme_protocol("zam", |ctx, request, responder| {
            let cache = ctx
                .app_handle()
                .path()
                .app_cache_dir()
                .unwrap_or_else(|_| std::env::temp_dir());
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let resp = match crate::zam::zam_serve(&cache, &path) {
                    Some((bytes, ct)) => tauri::http::Response::builder()
                        .status(200)
                        .header("content-type", ct)
                        .header("access-control-allow-origin", "*")
                        .body(bytes)
                        .unwrap(),
                    None => tauri::http::Response::builder()
                        .status(404)
                        .body(Vec::new())
                        .unwrap(),
                };
                responder.respond(resp);
            });
        })
        .invoke_handler(tauri::generate_handler![
            dml_version,
            games_list,
            games_status,
            games_catalog,
            games_install,
            url_install,
            games_install_input,
            games_install_cancel,
            games_remove,
            games_start,
            games_stop,
            games_restart,
            zam_probe,
            zam_cache_status,
            zam_cache_clear,
            wow_cache_status,
            wow_cache_clean,
            wow_accounts,
            wow_account_create,
            wow_account_set_password,
            wow_account_set_gm,
            wow_account_delete,
            wow_server_info,
            wow_server_detail,
            wow_stats,
            wow_docker_usage,
            wow_docker_clean,
            wow_update_check,
            wow_server_update,
            wow_console_tail,
            wow_console_send,
            wow_module_list,
            wow_commands,
            wow_module_install,
            wow_module_remove,
            wow_module_rebuild,
            wow_module_update_check,
            wow_module_update,
            wow_module_conf_activate,
            wow_module_client_patch,
            wow_module_tracking,
            wow_module_repair,
            wow_module_fixit,
            wow_module_place_npc,
            wow_client_path_get,
            wow_client_path_set,
            wow_client_path_detect,
            wow_items_search,
            wow_mail_item,
            wow_teleport_list,
            wow_teleport,
            wow_teleport_coords,
            wow_paperdoll,
            wow_item_info,
            wow_char_progress,
            wow_achievements,
            wow_entity_info,
            wow_config_list,
            wow_config_read,
            wow_tuning_read,
            wow_module_read,
            wow_teleport_list_read,
            wow_bots_read,
            wow_accounts_read,
            wow_stats_read,
            wow_paperdoll_read,
            backend_mode,
            wow_config_set,
            wow_config_tuning_list,
            wow_config_tuning_set,
            wow_config_conf_keys,
            wow_config_raw_read,
            wow_config_pb_keys,
            wow_config_files,
            wow_config_raw_reset,
            wow_config_raw_write,
            wow_accountwide_get,
            wow_accountwide_set,
            wow_bots_flush,
            wow_ahbot_repair,
            wow_party_setup,
            wow_party_online,
            wow_party_specs,
            wow_players_online,
            wow_bots_list,
            wow_world_restart,
            wow_party_add,
            wow_party_list,
            wow_party_kick,
            wow_party_dismiss_all,
            wow_party_relogin,
            wow_party_botcmd,
            wow_party_preset_save,
            wow_party_preset_list,
            wow_party_preset_delete,
            wow_party_preset_load,
            wow_party_preset_show,
            wow_party_preset_import,
            wow_backup_create,
            wow_backup_list,
            wow_backup_delete,
            wow_backup_validate,
            wow_backup_restore,
            wow_bridge_setup,
            wow_gm_level,
            wow_gm_gold,
            wow_gm_heal,
            wow_gm_revive,
            wow_gm_summon,
            wow_gm_at_login,
            wow_gm_return_home,
            wow_lan,
            wow_lan_public_ip,
            wow_tailscale,
            wow_port_check,
            dml_doctor,
            tool_install,
            open_shell,
            detect_lan_ip,
            wslconfig_read,
            wslconfig_write,
            restart_wsl,
            generate_compact_script,
            generate_mysql_proxy_script,
            defender_hint,
            native_setup_status,
            start_docker_desktop,
            native_yq_install,
            native_soap_copy,
            native_defender_script,
            save_text_file,
            set_auto_shutdown,
            set_keep_awake,
            set_taskbar_progress,
            realmlist_status,
            realmlist_fix,
            realmlist_lock
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app, event| {
            // Belt-and-braces: Windows clears the execution state when the
            // process dies, but clear it explicitly on the way out too so a
            // slow teardown never holds the PC awake.
            if let tauri::RunEvent::Exit = event {
                power::keep_awake(false);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_id_validation() {
        assert!(validate_game_id("wow-server-playerbots"));
        assert!(validate_game_id("Mu_Online.v2"));
        assert!(!validate_game_id(""));
        assert!(!validate_game_id("wow; rm -rf /"));
        assert!(!validate_game_id("wow server"));
        assert!(!validate_game_id("../escape"));
    }

    #[test]
    fn ip_validation_accepts_well_shaped_ipv4() {
        assert!(validate_ip("192.168.1.1"));
        assert!(validate_ip("8.8.8.8"));
        assert!(validate_ip("1.2.3.4"));
        assert!(validate_ip("255.255.255.255"));
        assert!(validate_ip("0.0.0.0"));
    }

    #[test]
    fn ip_validation_rejects_garbage() {
        assert!(!validate_ip(""));
        assert!(!validate_ip("not an ip"));
        assert!(!validate_ip("1.2.3"));
        assert!(!validate_ip("1.2.3.4.5"));
        assert!(!validate_ip("1..3.4"));
        assert!(!validate_ip(".1.2.3"));
        assert!(!validate_ip("1.2.3."));
        assert!(!validate_ip("1.2.3.4444"));
    }

    #[test]
    fn ip_validation_rejects_injection_shaped_strings() {
        assert!(!validate_ip("1.2.3.4; rm -rf /"));
        assert!(!validate_ip("1.2.3.4 && whoami"));
        assert!(!validate_ip("1.2.3.4\nrm -rf /"));
        assert!(!validate_ip("$(rm -rf /)"));
        assert!(!validate_ip("1.2.3.4`id`"));
        assert!(!validate_ip("../../etc/passwd"));
    }

    #[test]
    fn git_url_validation_accepts_plain_https_repo_links() {
        assert!(validate_git_url("https://github.com/user/repo.git"));
        assert!(validate_git_url("https://github.com/user/repo"));
        assert!(validate_git_url("https://gitlab.com/group/sub_group/my-game.git"));
    }

    #[test]
    fn git_url_validation_rejects_non_https_and_shell_shapes() {
        assert!(!validate_git_url(""));
        assert!(!validate_git_url("https://"));
        assert!(!validate_git_url("http://github.com/user/repo.git"));
        assert!(!validate_git_url("git@github.com:user/repo.git"));
        assert!(!validate_git_url("https://github.com/user/repo.git; rm -rf /"));
        assert!(!validate_git_url("https://github.com/user/repo.git && whoami"));
        assert!(!validate_git_url("https://evil.com/$(id)"));
        assert!(!validate_git_url("https://user:pass@github.com/user/repo.git"));
        assert!(!validate_git_url(&format!("https://x.com/{}", "a".repeat(300))));
        // No `..` path segment (clone-dir escape) -- the char filter alone
        // would pass these since '.' and '/' are allowed.
        assert!(!validate_git_url("https://github.com/../../../etc/passwd"));
        assert!(!validate_git_url("https://github.com/user/../repo"));
        assert!(!validate_git_url("https://../repo"));
        // A dot that isn't a lone `..` segment stays valid (real repo names).
        assert!(validate_git_url("https://github.com/user/repo..git"));
    }

    #[test]
    fn host_validation_accepts_public_ips_and_hostnames() {
        assert!(validate_host("84.210.13.37"));
        assert!(validate_host("myserver.duckdns.org"));
        assert!(validate_host("my-name.example-host.net"));
        assert!(validate_host("localhost"));
    }

    #[test]
    fn host_validation_rejects_garbage_and_injection_shapes() {
        assert!(!validate_host(""));
        assert!(!validate_host("foo bar"));
        assert!(!validate_host("evil;drop"));
        assert!(!validate_host("a`id`"));
        assert!(!validate_host("$(reboot)"));
        assert!(!validate_host("host\nname"));
        assert!(!validate_host("x'y"));
        assert!(!validate_host(&"a".repeat(254)));
    }

    #[test]
    fn lan_action_allowlist_is_closed() {
        assert!(LAN_ACTIONS.contains(&"on"));
        assert!(LAN_ACTIONS.contains(&"off"));
        assert!(LAN_ACTIONS.contains(&"status"));
        assert!(LAN_ACTIONS.contains(&"refresh"));
        assert!(!LAN_ACTIONS.contains(&"on; rm -rf /"));
        assert!(!LAN_ACTIONS.contains(&"reset"));
    }

    #[test]
    fn tool_name_allowlist_is_closed() {
        assert!(TOOL_NAMES.contains(&"unbound"));
        assert!(TOOL_NAMES.contains(&"unbound-remove"));
        assert!(!TOOL_NAMES.contains(&"unbound; rm -rf /"));
        assert!(!TOOL_NAMES.contains(&"anything-else"));
    }

    #[test]
    fn tailscale_action_allowlist_is_closed() {
        assert!(TAILSCALE_ACTIONS.contains(&"install"));
        assert!(TAILSCALE_ACTIONS.contains(&"up"));
        assert!(TAILSCALE_ACTIONS.contains(&"status"));
        assert!(TAILSCALE_ACTIONS.contains(&"down"));
        assert!(!TAILSCALE_ACTIONS.contains(&"up; rm -rf /"));
        assert!(!TAILSCALE_ACTIONS.contains(&"login"));
    }

    #[test]
    fn db_err_to_cmd_collapses_every_variant_to_db_unreachable() {
        // Finding #5: the bash arms these commands mirror can only ever emit
        // DB_UNREACHABLE (90-main.sh has no separate "connected but the query
        // failed" code for teleport-list/bots/accounts/paperdoll), so a native
        // DbError::Query must collapse to the same code, not surface
        // DB_QUERY_FAILED and diverge from the CLI.
        use crate::dml::db::DbError;
        assert_eq!(db_err_to_cmd(DbError::Unreachable("down".into())).code, "DB_UNREACHABLE");
        assert_eq!(db_err_to_cmd(DbError::Query("bad sql".into())).code, "DB_UNREACHABLE");
    }

    #[test]
    fn stats_err_to_cmd_collapses_every_variant_to_db_unreachable() {
        use crate::dml::db::DbError;
        assert_eq!(stats_err_to_cmd(DbError::Unreachable("down".into())).code, "DB_UNREACHABLE");
        assert_eq!(stats_err_to_cmd(DbError::Query("bad sql".into())).code, "DB_UNREACHABLE");
    }
}
