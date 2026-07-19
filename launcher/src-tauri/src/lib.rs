pub mod dml;
pub mod power;
pub mod realmlist;
pub mod watch;
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
                code: "WSL_SPAWN".into(),
                message: m,
                hint: "Is WSL installed and the dml-arch distro present? Try: wsl -d dml-arch".into(),
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

/// True when tasklist reports a live Wow.exe process. Probe failures count
/// as "not running" -- with the 2-poll debounce, one flaky probe can never
/// fire a stop on its own.
fn wow_client_running() -> bool {
    let mut cmd = std::process::Command::new("tasklist");
    cmd.args(["/FI", "IMAGENAME eq Wow.exe", "/FO", "CSV", "/NH"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW, as elsewhere
    }
    match cmd.output() {
        Ok(out) => watch::tasklist_shows_wow(&String::from_utf8_lossy(&out.stdout)),
        Err(_) => false,
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
        match machine.step(wow_client_running()) {
            watch::WatchAction::Armed => {
                let _ = app
                    .emit("auto-shutdown", serde_json::json!({"kind": "state", "state": "armed"}));
            }
            watch::WatchAction::Fire => {
                // Guard: never fire when the server isn't up. A failed or
                // ok=false detail read counts as "not up" -- skipping a stop
                // is always safe, stopping blind is not.
                let up = runner
                    .run_json(&["wow", "server-detail"])
                    .ok()
                    .filter(|env| env.ok)
                    .map(|env| {
                        matches!(
                            env.data["verdict"].as_str(),
                            Some("online") | Some("starting") | Some("soap_unreachable")
                        )
                    })
                    .unwrap_or(false);
                let stopped = if up {
                    // Same CLI verb as the Home Stop button (games stop is
                    // itself bounded: saveall + compose stop -t 180).
                    runner.run_captured(&["games", "stop", LAN_TITLE]).is_ok()
                } else {
                    false
                };
                let _ = app.emit(
                    "auto-shutdown",
                    serde_json::json!({"kind": "fired", "stopped": stopped}),
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

#[tauri::command]
async fn wow_config_files(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "files".into()]).await
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

// Batch 3 F11a: read-only who's-playing list for the Home card.
#[tauri::command]
async fn wow_players_online(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "players".into(), "online".into()]).await
}

// Batch 3 F11f: fast world-only restart (does NOT apply settings changes --
// the CLI stream carries that caveat).
#[tauri::command]
async fn wow_world_restart(
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "world-restart".into()], on_event, state).await
}

#[tauri::command]
async fn wow_party_add(player: String, class: String, gender: Option<String>, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    let mut a: Vec<String> = vec!["wow".into(),"party".into(),"add".into(),"--player".into(),player,"--class".into(),class];
    if let Some(g) = gender { a.extend(["--gender".into(), g]); }
    run_json_cmd(state, a).await
}

#[tauri::command]
async fn wow_party_list(player: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"list".into(),"--player".into(),player]).await
}

#[tauri::command]
async fn wow_party_kick(bot: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(),"party".into(),"kick".into(),"--bot".into(),bot]).await
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
async fn wow_party_botcmd(player: String, bot: String, action: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "party".into(), "botcmd".into(), "--player".into(), player, "--bot".into(), bot, "--action".into(), action],
    )
    .await
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

#[tauri::command]
async fn wow_backup_restore(file: String, on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(vec!["wow".into(), "backup".into(), "restore".into(), "--file".into(), file], on_event, state).await
}

#[tauri::command]
async fn wow_lan(
    action: String,
    ip: Option<String>,
    state: State<'_, AppState>,
) -> Result<String, CmdError> {
    if !LAN_ACTIONS.contains(&action.as_str()) {
        return Err(bad_arg(format!("invalid lan action: {action:?}")));
    }
    let ip_arg = if action == "on" || action == "refresh" {
        let ip = ip.ok_or_else(|| bad_arg("ip is required for the on/refresh actions"))?;
        if !validate_ip(&ip) {
            return Err(bad_arg(format!("invalid IPv4 address: {ip:?}")));
        }
        Some(ip)
    } else {
        None
    };
    let mut args: Vec<String> = vec!["lan".into(), LAN_TITLE.into(), action];
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
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let mut args: Vec<String> = vec!["games".into(), "remove".into(), id, "--yes".into()];
    // Batch 3 F13c: preserve the ~6 GB client-data volume for reinstalls.
    if keep_data.unwrap_or(false) {
        args.push("--keep-data".into());
    }
    stream_args(args, on_event, state).await
}

#[tauri::command]
async fn games_start(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("start", id, on_event, state).await
}

#[tauri::command]
async fn games_stop(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("stop", id, on_event, state).await
}

#[tauri::command]
async fn games_restart(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("restart", id, on_event, state).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            runner: std::sync::Arc::new(DmlRunner::default()),
            install: Arc::new(Mutex::new(None)),
            auto_shutdown: Arc::new(Mutex::new(AutoShutdownCtl { generation: 0, enabled: false })),
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
            games_install_input,
            games_install_cancel,
            games_remove,
            games_start,
            games_stop,
            games_restart,
            wow_accounts,
            wow_account_create,
            wow_account_set_password,
            wow_account_set_gm,
            wow_account_delete,
            wow_server_info,
            wow_server_detail,
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
            wow_module_conf_activate,
            wow_module_tracking,
            wow_module_repair,
            wow_module_fixit,
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
            wow_config_set,
            wow_config_raw_read,
            wow_config_pb_keys,
            wow_config_files,
            wow_config_raw_reset,
            wow_config_raw_write,
            wow_bots_flush,
            wow_party_setup,
            wow_party_online,
            wow_players_online,
            wow_world_restart,
            wow_party_add,
            wow_party_list,
            wow_party_kick,
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
            wow_backup_restore,
            wow_bridge_setup,
            wow_gm_level,
            wow_gm_gold,
            wow_gm_heal,
            wow_gm_revive,
            wow_gm_summon,
            wow_gm_at_login,
            wow_lan,
            dml_doctor,
            tool_install,
            open_shell,
            detect_lan_ip,
            save_text_file,
            set_auto_shutdown,
            set_keep_awake,
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
}
