pub mod dml;
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

fn read_wslconfig_state() -> Result<WslConfigState, CmdError> {
    let path = wslconfig_path()?;
    let exists = path.is_file();
    // Absent file = empty content (the write side creates it).
    let content = if exists { std::fs::read_to_string(&path).unwrap_or_default() } else { String::new() };
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
    let mut content = if path.is_file() {
        std::fs::read_to_string(&path)
            .map_err(|e| bad_arg(format!("could not read {}: {e}", path.display())))?
    } else {
        String::new()
    };
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
            url_install,
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
            wow_config_set,
            wow_config_tuning_list,
            wow_config_tuning_set,
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
            wow_players_online,
            wow_bots_list,
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
            wow_backup_validate,
            wow_backup_restore,
            wow_bridge_setup,
            wow_gm_level,
            wow_gm_gold,
            wow_gm_heal,
            wow_gm_revive,
            wow_gm_summon,
            wow_gm_at_login,
            wow_lan,
            wow_lan_public_ip,
            dml_doctor,
            tool_install,
            open_shell,
            detect_lan_ip,
            wslconfig_read,
            wslconfig_write,
            restart_wsl,
            generate_compact_script,
            defender_hint,
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
}
