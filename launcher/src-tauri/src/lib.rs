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

use dml_wow::envelope::Envelope;
use dml_core::runner::DmlRunner;

// Helpers that USED to be private to this file and moved into the library
// with the orchestration bodies that needed them (cargo-workspace refactor,
// Task 9). Imported under their original names so the many remaining call
// sites here read exactly as before.
use dml_core::proc::output_bounded;
use dml_wow::config::{cfgset_clean_legacy_env, cfgset_err, env_frozen};
use dml_wow::db::{cell_string, db_unreachable_err, sql_row_int};
use dml_wow::lan::LAN_TITLE;
use dml_wow::party::{
    bot_member_classes, bot_member_names, char_name_by_guid, group_member_guids,
    party_online_guid, preset_dir_or_internal_err, wait_new_member,
};

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
    /// Serializes every native-mode SOAP call (Task A2b, carried forward from
    /// the A1 review): the worldserver's SOAP listener runs on the single
    /// world thread, and `dml` (bash) already serializes its own SOAP calls
    /// under a `~/.dml/soap.lock` file lock for the same reason. Held only
    /// across the `soap::exec` call inside each native SOAP command's
    /// `spawn_blocking` closure -- see e.g. `wow_console_send_native`.
    pub soap_lock: Arc<std::sync::Mutex<()>>,
    /// Serializes every native-mode conf/override-YAML WRITE (`wow config
    /// set` and `wow config tuning-set`'s conf backend). The bash oracle gets
    /// per-invocation tmp-file uniqueness for free because each `dml` call is
    /// its own forked process; this app is long-lived, so two conf-writing
    /// commands landing on the same target file from two different Svelte
    /// pages (e.g. Settings and Module Tuning both touching
    /// `mod_ahbot.conf`) could otherwise interleave their read-modify-write
    /// cycles. Held for the whole write (not just the final rename) across
    /// each command's `spawn_blocking` closure -- mirrors `soap_lock`'s
    /// single-global-mutex shape, just scoped to config writes instead of
    /// SOAP calls.
    pub config_lock: Arc<std::sync::Mutex<()>>,
}

pub use dml_core::error::CmdError;

fn envelope_to_result(env: Envelope) -> Result<serde_json::Value, CmdError> {
    if env.ok {
        Ok(env.data)
    } else {
        let e = env.error.unwrap_or(dml_wow::envelope::ErrorInfo {
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

// --- Automatic "Auto (6h)" interval backup watcher --------------------------
//
// A second always-on background watcher (native mode only), started once at
// app setup (see `run()`'s `.setup()`) -- same "plain OS thread + sleep loop"
// shape as `auto_shutdown_watcher` above, just with no enable/disable toggle:
// there is no UI control for this one, so it runs for the app's whole
// lifetime with no generation tracking to race. Fires a chars-only dump named
// `backup::AUTO_INTERVAL_NAME` whenever `backup::should_run_interval_backup`
// says it's due (world running + >=6h since the last one); best-effort and
// silent on failure (`eprintln!` only, never `emit`/panic) -- this is
// unattended housekeeping nobody is watching a terminal for, so it must never
// surface as a user-facing error or crash the app.

/// One [`dml_wow::backup::INTERVAL_CHECK_SECS`] (30 min) tick: reads/
/// updates `last_run` in place. Split out from [`interval_backup_watcher`]
/// so the real docker/db work is easy to reason about independent of the
/// sleep loop around it.
fn interval_backup_tick(last_run: &Arc<Mutex<Option<u64>>>) {
    use dml_wow::{backup, db, maint, native, status};

    let program = native::docker_program();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // `container_running` degrades to `false` on any docker-down/timeout
    // failure (see its own doc comment), so no separate `docker_engine_up`
    // probe is needed here -- an unreachable engine already reads as "not up".
    let world_up = status::container_running(&program, "ac-worldserver", maint::PROBE_TIMEOUT);

    let last = *last_run.lock().unwrap_or_else(|e| e.into_inner());
    if !backup::should_run_interval_backup(last, now, world_up) {
        return;
    }

    let Some(bdir) = backup::backup_dir() else { return };
    if std::fs::create_dir_all(&bdir).is_err() {
        return;
    }
    let db_cfg = db::DbConfig::from_env();
    let file_name = backup::new_backup_file_name(false);
    let out_path = bdir.join(&file_name);
    match backup::dump_to(&program, &db_cfg.password, false, &out_path) {
        Ok(()) => {
            backup::write_meta(&db_cfg, &out_path, Some(backup::AUTO_INTERVAL_NAME));
            let _ = backup::prune(&bdir);
            // Only advance the clock on SUCCESS -- a failed dump leaves
            // `last_run` untouched so the very next 30-min tick retries,
            // rather than silently waiting a further 6h for another chance.
            *last_run.lock().unwrap_or_else(|e| e.into_inner()) = Some(now);
        }
        Err(e) => {
            eprintln!("[dml] interval auto-backup failed: {e}");
        }
    }
}

/// The watcher loop itself — spawned once from `run()`'s `.setup()` (native
/// mode only) and never stopped. `last_run` starts pre-seeded from whatever
/// [`dml_wow::backup::latest_auto_interval_backup_unix`] found on disk, so
/// a relaunch doesn't restart the 6h clock at zero.
fn interval_backup_watcher(last_run: Arc<Mutex<Option<u64>>>) {
    loop {
        std::thread::sleep(std::time::Duration::from_secs(dml_wow::backup::INTERVAL_CHECK_SECS));
        interval_backup_tick(&last_run);
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
    // Part 5a: native mode answers this itself (title-dir + compose-file
    // resolution + a bounded `docker compose ps` probe, no `dml` subprocess)
    // -- same "branch inside the shared command" shape as
    // `games_start`/`games_stop`/`games_restart` below, not a `_native`
    // sibling (this command has no engine-lifecycle wrapping to preserve).
    if is_native_backend() {
        let id_for_blocking = id.clone();
        return tauri::async_runtime::spawn_blocking(move || {
            dml_wow::lifecycle::games_status(&id_for_blocking, &dml_wow::lifecycle::games_dir_from_env())
        })
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
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

/// NATIVE-MODE `wow docker-clean` — see
/// [`dml_wow::destructive::docker_clean_stream`]. Native mode only — WSL
/// keeps calling `wow_docker_clean`.
#[tauri::command]
async fn wow_docker_clean_native(level: u8, on_event: Channel<serde_json::Value>) -> Result<(), CmdError> {
    require_native_backend()?;
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::destructive::docker_clean_stream(level, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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

// ---------------------------------------------------------------------------
// NATIVE-MODE `module install`/`module update`/`module remove` (Chunk 3a) —
// same NDJSON vocabulary as `wow_world_restart_native`: every domain failure
// travels IN the stream (`section_end{status:"error"}` + `error`), the
// command itself still resolves `Ok(())`. The orchestration itself lives in
// `dml_wow::modmgr::{module_install_stream, module_update_stream,
// module_remove_stream}` (cargo-workspace refactor, Task 9) — see those
// functions' doc comments. Native mode only — WSL keeps calling
// `wow_module_install`/`wow_module_update`/`wow_module_remove` (the
// `dml`-shelling siblings just above).
// ---------------------------------------------------------------------------

/// NATIVE-MODE `wow module install` — see
/// [`dml_wow::modmgr::module_install_stream`]. Native mode only — WSL keeps
/// calling `wow_module_install`.
#[tauri::command]
async fn wow_module_install_native(
    family: String,
    key: Option<String>,
    url: Option<String>,
    backup: Option<bool>,
    variant: Option<String>,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    let db_cfg = dml_wow::db::DbConfig::from_env();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::modmgr::module_install_stream(family, key, url, backup, variant, db_cfg, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

/// NATIVE-MODE `wow module update` — see
/// [`dml_wow::modmgr::module_update_stream`]. Native mode only — WSL keeps
/// calling `wow_module_update`.
#[tauri::command]
async fn wow_module_update_native(key: String, on_event: Channel<serde_json::Value>) -> Result<(), CmdError> {
    require_native_backend()?;
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::modmgr::module_update_stream(key, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

/// NATIVE-MODE `wow module remove` — see
/// [`dml_wow::modmgr::module_remove_stream`]. Native mode only — WSL keeps
/// calling `wow_module_remove`.
#[tauri::command]
async fn wow_module_remove_native(
    family: String,
    key: String,
    backup: Option<bool>,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    let db_cfg = dml_wow::db::DbConfig::from_env();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::modmgr::module_remove_stream(family, key, backup, db_cfg, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

/// NATIVE-MODE `wow module rebuild` — see
/// [`dml_wow::modmgr::module_rebuild_stream`]. Native mode only — WSL keeps
/// calling `wow_module_rebuild`.
#[tauri::command]
async fn wow_module_rebuild_native(backup: Option<bool>, on_event: Channel<serde_json::Value>) -> Result<(), CmdError> {
    require_native_backend()?;
    let db_cfg = dml_wow::db::DbConfig::from_env();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::modmgr::module_rebuild_stream(backup, db_cfg, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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

// ---------------------------------------------------------------------------
// NATIVE-MODE module update-check / conf-activate / tracking / repair /
// fixit / place-npc / client-patch (Chunk 5, Part 5b). Faithful ports of the
// matching sub-arms of `cli/src/90-main.sh`'s `module)` case (conf-activate
// 5026-5050, tracking 5052-5106, repair 5107-5180, fixit 5181-5253,
// place-npc 5254-5345, client-patch 5346-5432, update-check 5434-5470). Pure
// lookups/parses/SQL text live in `dml::moduletail`; DB reads/writes go
// through `db::query_with_params`/`db::execute` (bound params) except the
// handful of genuinely multi-statement fixit SQL blocks, which reuse
// `modmgr::mysql_run_stmt` (docker exec -e) — see that function's doc
// comment for why the `mysql` crate can't run those.
// ---------------------------------------------------------------------------

fn not_found_err(message: impl Into<String>, hint: impl Into<String>) -> CmdError {
    CmdError { code: "NOT_FOUND".into(), message: message.into(), hint: hint.into() }
}

/// One row's `COUNT(*)` decoded as `i64` (defaulting to 0 on anything odd —
/// every caller only ever asks "is this nonzero").
fn count_result(res: dml_wow::db::QueryResult) -> i64 {
    sql_row_int(res.rows.first().and_then(|r| r.first())).unwrap_or(0)
}

/// NATIVE-MODE `module update-check` (`90-main.sh:5434-5470`).
#[tauri::command]
async fn wow_module_update_check_native() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let sdir = dml_wow::maint::require_server_dir("Install it first, then re-run.")?;
        let program = std::ffi::OsString::from("git");
        Ok(dml_wow::moduletail::module_update_check(&program, &sdir))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `module conf-activate` (`90-main.sh:5026-5050`).
#[tauri::command]
async fn wow_module_conf_activate_native(
    key: String,
    force: Option<bool>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::modules::valid_cpp_key(&key) {
        return Err(bad_arg("Invalid module key"));
    }
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let sdir = dml_wow::maint::require_server_dir("")?;
        let Some(conf_name) = dml_wow::moduletail::module_conf_name(&key) else {
            return Err(CmdError { code: "NO_CONF".into(), message: format!("{key} has no standard conf file"), hint: String::new() });
        };
        let active = sdir.join("env").join("dist").join("etc").join("modules").join(conf_name);
        if active.is_file() && !force {
            return Err(CmdError {
                code: "EXISTS".into(),
                message: format!("Active conf already exists: {conf_name}"),
                hint: "Pass --force to overwrite with defaults.".into(),
            });
        }
        let Some(dist) = dml_wow::moduletail::module_conf_dist_path(&sdir, &key) else {
            return Err(CmdError {
                code: "NEEDS_REBUILD".into(),
                message: format!("No {conf_name}.dist yet"),
                hint: "The .dist appears after a worldserver rebuild with the module present.".into(),
            });
        };
        if let Some(parent) = active.parent() {
            std::fs::create_dir_all(parent).map_err(io_internal_err)?;
        }
        std::fs::copy(&dist, &active).map_err(io_internal_err)?;
        Ok(serde_json::json!({"key": key, "activated": true, "conf_name": conf_name}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `module tracking` (`90-main.sh:5052-5106`): read-only,
/// per-db (world/characters/auth) LIKE-diagnosis plus a per-discovered-file
/// exact-tracked check.
#[tauri::command]
async fn wow_module_tracking_native(key: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::modules::valid_cpp_key(&key) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid module key: {key}"), hint: String::new() });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let sdir = dml_wow::maint::require_server_dir("")?;
        if !dml_wow::modmgr::cpp_installed(&sdir, &key) {
            return Err(not_found_err(format!("Module not installed: {key}"), "Install it first."));
        }
        let (stripped, term1) = dml_wow::moduletail::tracking_like_terms(&key);
        let cfg = dml_wow::db::DbConfig::from_env();
        let mut dbs = serde_json::Map::new();
        for db_short in ["world", "characters", "auth"] {
            let db = dml_wow::moduletail::database_for_short(db_short).expect("closed 3-value list");
            let params: Vec<mysql::Value> =
                vec![mysql::Value::from(format!("%{stripped}%")), mysql::Value::from(format!("%{term1}%"))];
            let res = dml_wow::db::query_with_params(&cfg, db, dml_wow::moduletail::TRACKING_LIKE_SQL, params)
                .map_err(|e| db_unreachable_err(format!("Could not reach the {db_short} database: {e}")))?;
            let tracked_rows: Vec<String> = res.rows.iter().filter_map(|r| cell_string(r.first())).collect();

            let mut files_json = Vec::new();
            for f in dml_wow::moduletail::module_discover_sql_files(&sdir, &key, db_short) {
                if !dml_wow::moduletail::valid_module_sql_filename(&f) {
                    continue;
                }
                let params: Vec<mysql::Value> = vec![mysql::Value::from(&f)];
                let cnt = dml_wow::db::query_with_params(&cfg, db, dml_wow::moduletail::TRACKING_EXACT_COUNT_SQL, params)
                    .map(count_result)
                    .unwrap_or(0);
                files_json.push(serde_json::json!({"name": f, "tracked": cnt > 0}));
            }
            dbs.insert(db_short.to_string(), serde_json::json!({"tracked_rows": tracked_rows, "files": files_json}));
        }
        Ok(serde_json::json!({"key": key, "dbs": serde_json::Value::Object(dbs)}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `module repair` (`90-main.sh:5107-5180`): the FOURTH
/// sanctioned direct MySQL write (see `db.rs`/`backup.rs` headers) — INSERT/
/// DELETE on the `updates` tracking tables ONLY, via bound-param `db::
/// execute`. Every filename is validated BEFORE any SQL runs, matching the
/// oracle's abort-before-mutation contract.
#[tauri::command]
async fn wow_module_repair_native(
    key: String,
    db: String,
    mode: String,
    files: Option<String>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::modules::valid_cpp_key(&key) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid module key: {key}"), hint: String::new() });
    }
    if !matches!(db.as_str(), "world" | "characters" | "auth") {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid --db: {db}"), hint: "Use world, characters, or auth.".into() });
    }
    if !matches!(mode.as_str(), "mark" | "clear") {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid --mode: {mode}"), hint: "Use mark or clear.".into() });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let sdir = dml_wow::maint::require_server_dir("")?;
        if !dml_wow::modmgr::cpp_installed(&sdir, &key) {
            return Err(not_found_err(format!("Module not installed: {key}"), "Install it first."));
        }
        let file_list: Vec<String> = match &files {
            Some(f) => f.split_whitespace().map(str::to_string).collect(),
            None => dml_wow::moduletail::module_discover_sql_files(&sdir, &key, &db),
        };
        for f in &file_list {
            if !dml_wow::moduletail::valid_module_sql_filename(f) {
                return Err(CmdError {
                    code: "BAD_ARG".into(),
                    message: format!("Invalid filename: {f}"),
                    hint: "Filenames must match ^[A-Za-z0-9._-]+\\.sql$ (no slashes).".into(),
                });
            }
        }
        let database = dml_wow::moduletail::database_for_short(&db).expect("validated above");
        let cfg = dml_wow::db::DbConfig::from_env();
        let mut results = Vec::new();
        for f in file_list {
            let res = if mode == "mark" {
                match dml_wow::moduletail::find_module_sql_file(&sdir, &key, &f) {
                    None => "file_missing",
                    Some(path) => {
                        let bytes = std::fs::read(&path).map_err(io_internal_err)?;
                        let hash = {
                            use sha1::Digest;
                            let mut hasher = sha1::Sha1::new();
                            hasher.update(&bytes);
                            let digest = hasher.finalize();
                            digest.iter().map(|b| format!("{b:02X}")).collect::<String>()
                        };
                        let params: Vec<mysql::Value> =
                            vec![mysql::Value::from(&f), mysql::Value::from(&hash), mysql::Value::from(&hash)];
                        dml_wow::db::execute(&cfg, database, dml_wow::moduletail::REPAIR_MARK_SQL, params)
                            .map_err(|e| db_unreachable_err(format!("Could not write to acore_{db}.updates: {e}")))?;
                        "marked"
                    }
                }
            } else {
                let cnt_params: Vec<mysql::Value> = vec![mysql::Value::from(&f)];
                let cnt = dml_wow::db::query_with_params(&cfg, database, dml_wow::moduletail::REPAIR_CLEAR_COUNT_SQL, cnt_params)
                    .map_err(|e| db_unreachable_err(format!("Could not reach the {db} database: {e}")))
                    .map(count_result)?;
                if cnt == 0 {
                    "not_tracked"
                } else {
                    let del_params: Vec<mysql::Value> = vec![mysql::Value::from(&f)];
                    dml_wow::db::execute(&cfg, database, dml_wow::moduletail::REPAIR_CLEAR_DELETE_SQL, del_params)
                        .map_err(|e| db_unreachable_err(format!("Could not write to acore_{db}.updates: {e}")))?;
                    "cleared"
                }
            };
            results.push(serde_json::json!({"file": f, "result": res}));
        }
        Ok(serde_json::json!({"key": key, "db": db, "mode": mode, "results": results}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// One capital's `(map, x, y, z, o)` spawn-if-missing step, shared by fixit
/// and place-npc: reads the current spawn count, inserts only if it's zero,
/// returns whether a spawn was just placed.
fn ensure_capital_spawn(
    cfg: &dml_wow::db::DbConfig,
    entry: u32,
    map: u32,
    x: f64,
    y: f64,
    z: f64,
    o: f64,
) -> Result<bool, CmdError> {
    let cnt_params: Vec<mysql::Value> = vec![mysql::Value::from(entry), mysql::Value::from(map)];
    let cnt = dml_wow::db::query_with_params(cfg, dml_wow::db::Database::World, dml_wow::moduletail::CREATURE_SPAWN_COUNT_SQL, cnt_params)
        .map_err(|e| db_unreachable_err(format!("Could not reach the world database: {e}")))
        .map(count_result)?;
    if cnt > 0 {
        return Ok(false);
    }
    let ins_params: Vec<mysql::Value> = vec![
        mysql::Value::from(entry),
        mysql::Value::from(map),
        mysql::Value::from(x),
        mysql::Value::from(y),
        mysql::Value::from(z),
        mysql::Value::from(o),
    ];
    dml_wow::db::execute(cfg, dml_wow::db::Database::World, dml_wow::moduletail::CREATURE_SPAWN_INSERT_SQL, ins_params)
        .map_err(|_| CmdError {
            code: "SQL_FAILED".into(),
            message: format!("Could not insert the spawn for map {map}"),
            hint: "Is ac-database running?".into(),
        })?;
    Ok(true)
}

/// NATIVE-MODE `module fixit battlepass-npc` (`90-main.sh:5181-5253`).
#[tauri::command]
async fn wow_module_fixit_native(key: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if key != "battlepass-npc" {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Unknown fixit: {key}"),
            hint: "Available: battlepass-npc".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        use dml_wow::moduletail as mt;
        let cfg = dml_wow::db::DbConfig::from_env();
        let entry = mt::BATTLEPASS_NPC_ENTRY;

        let sw_params: Vec<mysql::Value> = vec![mysql::Value::from(entry), mysql::Value::from(0u32)];
        let bp_sw = dml_wow::db::query_with_params(&cfg, dml_wow::db::Database::World, mt::CREATURE_SPAWN_COUNT_SQL, sw_params)
            .map_err(|e| db_unreachable_err(format!("Could not reach the world database: {e}")))
            .map(count_result)?;
        let og_params: Vec<mysql::Value> = vec![mysql::Value::from(entry), mysql::Value::from(1u32)];
        let bp_og = dml_wow::db::query_with_params(&cfg, dml_wow::db::Database::World, mt::CREATURE_SPAWN_COUNT_SQL, og_params)
            .map_err(|e| db_unreachable_err(format!("Could not reach the world database: {e}")))
            .map(count_result)?;
        if bp_sw > 0 && bp_og > 0 {
            return Ok(serde_json::json!({
                "key": "battlepass-npc", "already_placed": true, "template": "exists",
                "spawns_placed": 0, "restart_required": false,
                "note": "The Battle Pass NPC is already placed in both capitals.",
            }));
        }

        let tcnt_params: Vec<mysql::Value> = vec![mysql::Value::from(entry)];
        let tcnt = dml_wow::db::query_with_params(&cfg, dml_wow::db::Database::World, mt::CREATURE_TEMPLATE_COUNT_SQL, tcnt_params)
            .map_err(|e| db_unreachable_err(format!("Could not reach the world database: {e}")))
            .map(count_result)?;

        let template = if tcnt == 0 {
            let docker_program = dml_wow::native::docker_program();
            if !dml_wow::modmgr::mysql_run_stmt(&docker_program, &cfg.password, "acore_world", mt::BATTLEPASS_TEMPLATE_INSERT_SQL) {
                return Err(CmdError {
                    code: "SQL_FAILED".into(),
                    message: "Could not create the Battle Pass NPC template".into(),
                    hint: "Is ac-database running?".into(),
                });
            }
            // Schema-adaptive model/scale statements: best-effort, matching the oracle's own `|| true`.
            let _ = dml_wow::modmgr::mysql_run_stmt(&docker_program, &cfg.password, "acore_world", mt::BATTLEPASS_SCALE_UPDATE_SQL);
            let _ = dml_wow::modmgr::mysql_run_stmt(&docker_program, &cfg.password, "acore_world", mt::BATTLEPASS_MODEL_DELETE_SQL);
            let _ = dml_wow::modmgr::mysql_run_stmt(&docker_program, &cfg.password, "acore_world", mt::BATTLEPASS_MODEL_INSERT_SQL);
            let _ = dml_wow::modmgr::mysql_run_stmt(&docker_program, &cfg.password, "acore_world", mt::BATTLEPASS_MODELID1_UPDATE_SQL);
            "created"
        } else {
            "exists"
        };

        let mut placed = 0;
        for &(map, x, y, z, o) in mt::BATTLEPASS_CAPITALS.iter() {
            if ensure_capital_spawn(&cfg, entry, map, x, y, z, o)? {
                placed += 1;
            }
        }
        Ok(serde_json::json!({
            "key": "battlepass-npc", "already_placed": false, "template": template,
            "spawns_placed": placed, "restart_required": true,
            "note": "Restart the world server for the NPC to appear (Stormwind trade district + Orgrimmar Valley of Strength).",
        }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `module place-npc` (`90-main.sh:5254-5345`).
#[tauri::command]
async fn wow_module_place_npc_native(key: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::moduletail::valid_place_npc_key(&key) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("place-npc does not support: {key}"),
            hint: "Eligible: mod-1v1-arena, mod-transmog, mod-npc-beastmaster, bmah".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let sdir = dml_wow::maint::require_server_dir("Install it first.")?;
        let installed = if key == "bmah" {
            dml_wow::modmgr::lua_deployed(&sdir, "bmah")
        } else {
            sdir.join("modules").join(&key).is_dir()
        };
        if !installed {
            return Err(not_found_err(format!("{key} is not installed"), "Install it on the Modules page first."));
        }
        let specs = dml_wow::moduletail::npc_coord_specs(&key);
        if specs.is_empty() {
            return Err(CmdError {
                code: "NO_COORDS".into(),
                message: format!("No capital coordinates defined for {key}"),
                hint: "This module has no ready-made spawn block.".into(),
            });
        }
        let entry = specs[0].entry;
        let cfg = dml_wow::db::DbConfig::from_env();
        let tcnt_params: Vec<mysql::Value> = vec![mysql::Value::from(entry)];
        let tcnt = dml_wow::db::query_with_params(&cfg, dml_wow::db::Database::World, dml_wow::moduletail::CREATURE_TEMPLATE_COUNT_SQL, tcnt_params)
            .map_err(|e| db_unreachable_err(format!("Could not reach the world database: {e}")))
            .map(count_result)?;
        if tcnt == 0 {
            return Err(CmdError {
                code: "NO_TEMPLATE".into(),
                message: format!("The NPC template (entry {entry}) does not exist yet"),
                hint: "Install and rebuild the module (cpp) or deploy it (Lua) so its NPC exists, then try again.".into(),
            });
        }
        let mut placed = 0;
        let mut maps = Vec::new();
        for spec in &specs {
            let did = ensure_capital_spawn(&cfg, spec.entry, spec.map, spec.x, spec.y, spec.z, spec.o)?;
            if did {
                placed += 1;
            }
            maps.push(serde_json::json!({"map": spec.map, "placed": did}));
        }
        let already = placed == 0;
        let note = if placed > 0 {
            format!("Placed the NPC in {placed} capital(s). Restart the world server (Home) for it to appear.")
        } else {
            "The NPC is already placed in both capitals.".to_string()
        };
        Ok(serde_json::json!({
            "key": key, "entry": entry, "maps": maps, "spawns_placed": placed,
            "already_placed": already, "restart_required": placed > 0, "note": note,
        }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `module client-patch` — see
/// [`dml_wow::moduletail::module_client_patch_stream`].
#[tauri::command]
async fn wow_module_client_patch_native(
    key: String,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::moduletail::module_client_patch_stream(key, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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
    match dml_wow::backend::selected() {
        dml_wow::backend::Backend::Native => "native",
        dml_wow::backend::Backend::Wsl => "wsl",
    }
}

/// NATIVE-MODE fast read of the config settings: returns the SAME shape as
/// `wow_config_list` (`{"settings":[…66 rows…]}`) with zero bash/yq/fork on the
/// hot path. The static registry is embedded in `dml_wow::registry` (Task 8 —
/// no CLI fetch at all anymore); every live `value` is read directly off the
/// runtime files in Rust (see `dml::config`). Docker Desktop may be closed —
/// these are pure file reads.
///
/// This command is for native mode only. In WSL mode the frontend keeps calling
/// `wow_config_list`; `backend_mode` tells it which to use.
#[tauri::command]
async fn wow_config_read() -> Result<serde_json::Value, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let mut reader = dml_wow::config::ConfigReader::from_env();
        Ok(reader.assemble(dml_wow::registry::config_registry_rows()))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the module-tuning settings: same shape as
/// `wow_config_tuning_list` (`{"settings":[…13 rows…]}`) with zero bash/fork on
/// the hot path. The static registry is embedded in `dml_wow::registry`
/// (Task 8); each row's `value` + `installed` are read straight off the
/// runtime files in Rust (see `dml::tuning`). Native mode only — WSL keeps
/// calling `wow_config_tuning_list`.
#[tauri::command]
async fn wow_tuning_read() -> Result<serde_json::Value, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let mut reader = dml_wow::tuning::TuningReader::from_env();
        Ok(reader.assemble(dml_wow::registry::tuning_registry_rows()))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the module list: same shape as `wow_module_list`
/// (`{families:{cpp,lua,sql}, rebuild_pending, ale_ready}`) with zero bash/fork
/// on the hot path (only LOCAL `git` reads for installed clones' head/date). The
/// static catalog is embedded in `dml_wow::registry` (Task 8); every dynamic
/// field is filled from the runtime files in Rust (see `dml::modules`). Native
/// mode only — WSL keeps calling `wow_module_list`.
#[tauri::command]
async fn wow_module_read() -> Result<serde_json::Value, CmdError> {
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let reader = dml_wow::modules::ModuleReader::from_env();
        Ok(reader.assemble(dml_wow::registry::module_catalog()))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Map a native-mode [`dml_wow::db::DbError`] to the [`CmdError`] the frontend
/// already knows how to render. Both variants collapse to `DB_UNREACHABLE`,
/// matching the CLI: every one of these arms (`teleport-list` / `bots list` /
/// `accounts` / `paperdoll`) reports `DB_UNREACHABLE` for ANY `db_*_query`
/// failure in `90-main.sh` — the bash has no separate "connected but the query
/// itself failed" code path, so a native `DbError::Query` (e.g. a genuinely
/// malformed statement) must still read as `DB_UNREACHABLE` to stay
/// byte-identical to `dml`. Same collapse [`stats_err_to_cmd`] already does for
/// the `stats` arm — see its comment for the fuller rationale.
fn db_err_to_cmd(e: dml_wow::db::DbError) -> CmdError {
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
        let cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::pages::read_teleport_list(&cfg, search.as_deref()).map_err(db_err_to_cmd)
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
        if !dml_wow::paperdoll::valid_charname(n) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Invalid name prefix: {n}"),
                hint: "1-12 letters/digits/underscore.".into(),
            });
        }
    }
    if let Some(c) = class {
        if !dml_wow::pages::valid_bot_class(c) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Invalid class id: {c}"),
                hint: "1-9 or 11.".into(),
            });
        }
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        let f = dml_wow::pages::BotFilters {
            name,
            class,
            min_level,
            max_level,
            online: online.unwrap_or(false),
            limit: dml_wow::pages::clamp_limit(limit),
            offset: offset.unwrap_or(0),
        };
        dml_wow::pages::read_bots(&cfg, &f).map_err(db_err_to_cmd)
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
        let cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::pages::read_accounts(&cfg).map_err(db_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the "who's playing right now" Home card: same
/// shape as `wow_players_online` (`{"players":[{name,level,class,zone}]}`),
/// over a direct MySQL connection. Native mode only — WSL keeps calling
/// `wow_players_online`.
#[tauri::command]
async fn wow_players_online_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::pages::read_players_online(&cfg).map_err(db_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of online (non-bot) characters for the party UI:
/// same shape as `wow_party_online` (`{"online":[{guid,name,class,level}]}`),
/// over a direct MySQL connection. Native mode only — WSL keeps calling
/// `wow_party_online`.
#[tauri::command]
async fn wow_party_online_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::pages::read_party_online(&cfg).map_err(db_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast item search: same shape as `wow_items_search`
/// (`{"items":[…]}`), over a direct MySQL connection. Rejects an
/// empty/whitespace-only `name` with `BAD_ARG` BEFORE any SQL is built,
/// exactly like the arm's own pre-check (90-main.sh `items search`). Native
/// mode only — WSL keeps calling `wow_items_search`.
#[tauri::command]
async fn wow_items_search_read(
    name: String,
    quality: Option<u32>,
    min_level: Option<u32>,
    max_level: Option<u32>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if name.trim().is_empty() {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "items search requires a non-empty --name".into(),
            hint: "Example: dml wow items search --name hearthstone --json".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        let opts = dml_wow::pages::ItemSearchOpts { name, quality, min_level, max_level };
        dml_wow::pages::read_items_search(&cfg, &opts).map_err(db_err_to_cmd)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of a character's achievement/talent summary: same
/// shape as `wow_char_progress`
/// (`{"achievements":{total,recent},"talents":{groups_count,active_group,spells}}`),
/// over a direct MySQL connection (5-query sequence, same order as the arm).
/// Rejects an invalid name with `BAD_ARG` and an unknown character with
/// `NOT_FOUND`, exactly like the CLI arm. Native mode only.
#[tauri::command]
async fn wow_char_progress_read(char_name: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&char_name) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid character name: {char_name}"),
            hint: String::new(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        match dml_wow::pages::read_char_progress(&cfg, &char_name).map_err(db_err_to_cmd)? {
            Some(v) => Ok(v),
            None => Err(CmdError {
                code: "NOT_FOUND".into(),
                message: format!("No such character: {char_name}"),
                hint: String::new(),
            }),
        }
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of a character's full earned-achievement list: same
/// shape as `wow_achievements` (`{"earned":[{id,date}]}`), over a direct
/// MySQL connection. Rejects an invalid name with `BAD_ARG` and an unknown
/// character with `NOT_FOUND`, exactly like the CLI arm. Native mode only.
#[tauri::command]
async fn wow_achievements_read(char_name: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&char_name) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid character name: {char_name}"),
            hint: String::new(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        match dml_wow::pages::read_achievements(&cfg, &char_name).map_err(db_err_to_cmd)? {
            Some(v) => Ok(v),
            None => Err(CmdError {
                code: "NOT_FOUND".into(),
                message: format!("No such character: {char_name}"),
                hint: String::new(),
            }),
        }
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Map a native-mode stats [`dml_wow::db::DbError`] to a [`CmdError`] whose
/// code matches the CLI's `stats` arm: that arm reports `DB_UNREACHABLE` for
/// EVERY payload failure (including a query error on a reachable DB — see the
/// "honest hint" branch in 90-main.sh), so both DbError variants collapse to
/// `DB_UNREACHABLE` here to stay byte-identical to `dml wow stats`.
fn stats_err_to_cmd(e: dml_wow::db::DbError) -> CmdError {
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
        let cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::stats::read_stats(&cfg).map_err(stats_err_to_cmd)
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
    if !dml_wow::paperdoll::valid_charname(&char_name) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid character name: {char_name}"),
            hint: String::new(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        match dml_wow::paperdoll::read_paperdoll(&cfg, &char_name).map_err(db_err_to_cmd)? {
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

/// NATIVE-MODE fast read of the Home status card's SOAP snapshot: same shape
/// as `wow_server_info`
/// (`{online,version,players,uptime,mean_ms,median_ms}`), firing SOAP `server
/// info` directly instead of shelling `dml`. Down/faulted -> `online:false`
/// (an answer, not an error); only a SOAP auth failure is a hard
/// `SOAP_AUTH` error. Native mode only — WSL keeps calling `wow_server_info`.
#[tauri::command]
async fn wow_server_info_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::soap::SoapConfig::load();
        dml_wow::status::read_server_info(&cfg).map_err(|_| CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP authentication failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the Home status card's full composite: same
/// shape as `wow_server_detail`
/// (`{verdict,exit_code,containers,world_ready,soap,bots,ports}`), assembled
/// from direct `docker`/SOAP/MySQL calls (see `dml::status`) instead of
/// shelling `dml`. Polled on an interval by the frontend, so every I/O call
/// underneath is bounded — never hangs. Native mode only — WSL keeps calling
/// `wow_server_detail`.
#[tauri::command]
async fn wow_server_detail_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let program = dml_wow::native::docker_program();
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let db_cfg = dml_wow::db::DbConfig::from_env();
        let mut reader = dml_wow::config::ConfigReader::from_env();
        Ok(dml_wow::status::read_server_detail(&program, &soap_cfg, &db_cfg, &mut reader))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the Console page's log tail: same shape as
/// `wow_console_tail` (`{available,lines}`), via a direct bounded `docker
/// logs --tail` instead of shelling `dml`. `--lines` defaults to 200 and is
/// validated 1-1000 BEFORE any docker call, exactly like the CLI arm's own
/// pre-check (`90-main.sh` `console-tail)`). Native mode only — WSL keeps
/// calling `wow_console_tail`.
#[tauri::command]
async fn wow_console_tail_read(lines: Option<u32>) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let lines = lines.unwrap_or(200);
    if !(1..=1000).contains(&lines) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "--lines must be 1-1000".into(),
            hint: "Usage: dml wow console-tail [--lines N] --json".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let program = dml_wow::native::docker_program();
        Ok(dml_wow::status::read_console_tail(&program, lines))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the Tools page's Docker disk-usage card: same
/// `{"lines":[...]}"` shape as `wow_docker_usage`, via a direct bounded
/// `docker system df` instead of shelling `dml`. `docker info` down is a
/// hard `DOCKER_DOWN` error (matching the CLI arm's own gate — unlike
/// `server-detail`, this verb does NOT treat "down" as data). Native mode
/// only — WSL keeps calling `wow_docker_usage`.
#[tauri::command]
async fn wow_docker_usage_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let program = dml_wow::native::docker_program();
        dml_wow::maint::read_docker_usage(&program).map_err(|_| CmdError {
            code: "DOCKER_DOWN".into(),
            message: "Docker is not running".into(),
            hint: "Start Docker Desktop, then retry.".into(),
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the Tools/LAN port diagnostic: same envelope
/// shape as `wow_port_check`, via direct bounded `docker port` probes plus
/// a `.env` read instead of shelling `dml`. Gates mirror the CLI arm:
/// `NOT_FOUND` when the WoW Playerbots title isn't installed, `DOCKER_DOWN`
/// when the engine isn't up. Native mode only — WSL keeps calling
/// `wow_port_check`.
#[tauri::command]
async fn wow_port_check_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        let Some(server_dir) = dml_wow::maint::resolve_server_dir(&title_dir) else {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "WoW Playerbots server not installed".into(),
                hint: "Install it first.".into(),
            });
        };
        let program = dml_wow::native::docker_program();
        if !dml_wow::maint::docker_engine_up(&program, dml_wow::maint::PROBE_TIMEOUT) {
            return Err(CmdError {
                code: "DOCKER_DOWN".into(),
                message: "Docker is not running".into(),
                hint: "Start the server first, then re-check.".into(),
            });
        }
        Ok(dml_wow::maint::read_port_check(&program, &server_dir, dml_wow::maint::PROBE_TIMEOUT))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the Tools/Updates page's behind-count check:
/// same envelope shape as `wow_update_check`, via direct bounded `git
/// fetch --quiet origin` + `git rev-list --count` instead of shelling
/// `dml`. NEVER mutates the worktree (no pull/stash). Gates mirror the CLI
/// arm: `NOT_FOUND` when the title isn't installed, `GIT_MISSING` when the
/// resolved dir isn't a git checkout. Native mode only — WSL keeps calling
/// `wow_update_check`.
#[tauri::command]
async fn wow_update_check_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        let Some(server_dir) = dml_wow::maint::resolve_server_dir(&title_dir) else {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "WoW Playerbots server not installed".into(),
                hint: "Install it first, then re-run.".into(),
            });
        };
        if !dml_wow::maint::is_git_checkout(&server_dir) {
            return Err(CmdError {
                code: "GIT_MISSING".into(),
                message: format!("{} is not a git checkout", server_dir.display()),
                hint: "Can't check for updates.".into(),
            });
        }
        let program = std::ffi::OsString::from("git");
        Ok(dml_wow::maint::read_update_check(&program, &server_dir))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

// --- Task D1a: 5 small native-mode reads ------------------------------------
// commands / party-specs / client-path (get+detect) / cache-status /
// lan-public-ip. Each is a faithful port of its `cli/src/90-main.sh` arm
// (see the `dml::{commands,party_specs,clientpath,cachestatus,lanip}`
// module doc comments for the exact source lines). Native mode only — WSL
// keeps calling the un-suffixed sibling command.

/// NATIVE-MODE fast read of the in-game command reference: same shape as
/// `wow_commands` (`{"mods":[{key,name,text}]}`). The static per-mod text
/// blocks are ported verbatim in `dml::commands::cmd_block_for`;
/// install-state comes from the same `ModuleReader` `wow_module_read` uses;
/// the module catalog is the same embedded `dml_wow::registry::module_catalog`
/// (Task 8) `wow_module_read` reads. `NOT_FOUND` when the WoW Playerbots title
/// isn't installed, matching the CLI arm.
#[tauri::command]
async fn wow_commands_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        if dml_wow::maint::resolve_server_dir(&title_dir).is_none() {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "WoW Playerbots server not installed".into(),
                hint: "Install it first.".into(),
            });
        }
        let catalog = dml_wow::registry::module_catalog();
        let reader = dml_wow::modules::ModuleReader::from_env();
        let modules_dir = title_dir.join("modules");
        Ok(dml_wow::commands::assemble_commands(catalog, &reader, &modules_dir))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the live playerbots premade specs: same shape
/// as `wow_party_specs` (`{"source","specs":[{class_id,class,specno,name,
/// link,tree}]}`), parsed straight off the deployed `playerbots.conf` (or
/// its `.dist`). `NOT_FOUND` ("playerbots.conf not found (nor its .dist)")
/// when neither exists, matching the CLI arm's single error path.
#[tauri::command]
async fn wow_party_specs_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        let Some((conf_path, source)) = dml_wow::party_specs::find_conf(&title_dir) else {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "playerbots.conf not found (nor its .dist)".into(),
                hint: "Is the WoW server fully installed?".into(),
            });
        };
        let content = std::fs::read_to_string(&conf_path).unwrap_or_default();
        Ok(dml_wow::party_specs::build_specs_value(&content, source))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the saved WoW client path: same shape as
/// `wow_client_path_get` (`{"path","valid"}`). `client-path set` stays a
/// WSL/CLI write (out of scope for this read chunk).
#[tauri::command]
async fn wow_client_path_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        Ok(dml_wow::clientpath::read_client_path())
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast client-detect scan: same shape as `wow_client_path_
/// detect` (`{"candidates":[...]}`). Windows-native scan roots (no WSL
/// `/mnt/*` here) — see `dml::clientpath`'s module doc comment.
#[tauri::command]
async fn wow_client_path_detect_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        let roots = dml_wow::clientpath::default_scan_roots();
        let candidates = dml_wow::clientpath::detect_client(&roots);
        Ok(serde_json::json!({ "candidates": candidates }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of the wowhead item-info cache size: same shape as
/// `wow_cache_status` (`{"caches":[{key,label,path,present,bytes,files}]}`),
/// via a plain `std::fs` walk instead of shelling `du`/`find`.
#[tauri::command]
async fn wow_cache_status_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        Ok(dml_wow::cachestatus::read_cache_status())
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `wow cache-clean` (Chunk 2 task 1): same shape as
/// `wow_cache_clean` (`{"wiped":true,"freed_bytes","path"}`), a plain
/// `std::fs::remove_dir_all` instead of shelling `rm -rf`. PRESERVES the
/// suffix-guard safety invariant byte-for-byte in spirit — see
/// `dml::cachestatus::passes_wipe_guard`'s doc comment — this can NEVER nuke
/// `~/.dml` itself. Native mode only.
#[tauri::command]
async fn wow_cache_clean_native() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        dml_wow::cachestatus::clean_cache().map_err(|e| match e {
            dml_wow::cachestatus::CacheCleanError::Guard(m) => {
                CmdError { code: "INTERNAL".into(), message: m, hint: String::new() }
            }
            dml_wow::cachestatus::CacheCleanError::Wipe(m) => {
                CmdError { code: "WIPE_FAILED".into(), message: m, hint: String::new() }
            }
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `wow client-path set` (Chunk 2 task 2): same shape as
/// `wow_client_path_set` (`{"path","valid":true}`). **NATIVE DECISION**:
/// stores the WINDOWS path exactly as given — skips the WSL
/// `_client_win_to_wsl` (`/mnt/c`) translation, since native has no WSL
/// filesystem and a native folder picker already hands back a native path.
/// Native mode only.
#[tauri::command]
async fn wow_client_path_set_native(path: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        dml_wow::clientpath::set_client_path(std::path::Path::new(&path)).map_err(|e| match e {
            dml_wow::clientpath::ClientPathSetError::BadPath(m) => CmdError {
                code: "BAD_PATH".into(),
                message: m,
                hint: "Check the folder exists and try again.".into(),
            },
            dml_wow::clientpath::ClientPathSetError::NotClient(m) => CmdError {
                code: "NOT_CLIENT".into(),
                message: m,
                hint: "Expected Wow.exe or an Interface folder inside it.".into(),
            },
            dml_wow::clientpath::ClientPathSetError::Io(m) => {
                CmdError { code: "INTERNAL".into(), message: m, hint: String::new() }
            }
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast public-IP lookup: same shape as `wow_lan_public_ip`
/// (`{"public_ip": …|null}`), via `reqwest::blocking` instead of shelling
/// `curl`. Never errors — an unreachable network degrades to `null`,
/// matching the CLI arm.
#[tauri::command]
async fn wow_lan_public_ip_read() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        let ip = dml_wow::lanip::fetch_public_ip();
        Ok(serde_json::json!({ "public_ip": ip }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of item tooltip/icon info: same shape as
/// `wow_item_info` (`{"items":[{entry,source,icon,icon_b64,wowhead?,name?,
/// quality?,tooltip_html?,display_id?}]}`), via direct `reqwest` + the SAME
/// `~/.dml/wowhead-cache` disk cache the bash CLI reads/writes (so a cache
/// warmed by either backend is reused by the other), with a local
/// `item_template` DB fallback for custom/unknown-to-wowhead items. `
/// --entries` max 25 enforced here (`BAD_ARG`), matching the CLI arm;
/// duplicates deduped preserving first-seen order (`dml::iteminfo::
/// dedupe_preserve_order`). Never hard-fails on network/DB trouble --
/// degradation is per-item, exactly like `_iteminfo_one`. Native mode only.
#[tauri::command]
async fn wow_item_info_read(entries: Vec<u32>) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if entries.len() > 25 {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "--entries max 25 ids per call".into(),
            hint: String::new(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let Some(cache_root) = dml_wow::cachestatus::cache_dir() else {
            return Err(CmdError {
                code: "INTERNAL".into(),
                message: "Could not resolve the wowhead cache directory".into(),
                hint: String::new(),
            });
        };
        let db_cfg = dml_wow::db::DbConfig::from_env();
        Ok(dml_wow::iteminfo::read_item_info(&cache_root, Some(&db_cfg), &entries))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast read of spell/achievement tooltip/icon info: same shape
/// as `wow_entity_info` (`{"entities":[{id,source,icon,icon_b64,wowhead?}]}`),
/// same wowhead+cache machinery as [`wow_item_info_read`] but NO local/DB
/// fallback (unknown -> `{"id":N,"source":"unavailable"}`). `--kind` must be
/// `spell` or `achievement` and `--ids` max 25, both `BAD_ARG` like the CLI
/// arm. Native mode only.
#[tauri::command]
async fn wow_entity_info_read(kind: String, ids: Vec<u32>) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if kind != "spell" && kind != "achievement" {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "--kind must be spell or achievement".into(),
            hint: String::new(),
        });
    }
    if ids.len() > 25 {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "--ids max 25 per call".into(),
            hint: String::new(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let Some(cache_root) = dml_wow::cachestatus::cache_dir() else {
            return Err(CmdError {
                code: "INTERNAL".into(),
                message: "Could not resolve the wowhead cache directory".into(),
                hint: String::new(),
            });
        };
        Ok(dml_wow::iteminfo::read_entity_info(&cache_root, &kind, &ids))
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

// ---------------------------------------------------------------------------
// `wow_config_set_native` (Task B2a) — native port of `dml wow config set`
// (`90-main.sh:2344-2561`, the `set)` case). Two independent routes, exactly
// like the oracle:
//   A) `key` starts with `conf:` — a DIRECT module-conf write, no registry
//      row involved (`config_set_direct`).
//   B) otherwise — a CURATED registry-row write: SOAP for `server.motd`, a
//      conf-file write for `conf:` env columns (with `ahbot.character` and
//      `bots.population`'s special-case companion writes), or an override-env
//      write for anything else (`config_set_curated`).
// Both routes share `env_frozen` (the `_cfg_env_frozen` port) to decide
// whether a legacy AC_* env still beats the conf in the RUNNING container.
// ---------------------------------------------------------------------------

/// Route A — the direct `conf:` route (`90-main.sh:2354-2438`). `full_key` is
/// the ORIGINAL `--key` value (including the `conf:` prefix), used verbatim in
/// the "Bad conf key" message exactly like the oracle.
fn config_set_direct(
    title_dir: &std::path::Path,
    full_key: &str,
    value: &str,
    soap_lock: &Arc<std::sync::Mutex<()>>,
) -> Result<serde_json::Value, CmdError> {
    let Some((conf_file, conf_key)) = dml_wow::config::route_conf(full_key) else {
        return Err(cfgset_err("BAD_ARG", format!("Bad conf key: {full_key}"), ""));
    };
    if dml_wow::config::is_core_conf_file(&conf_file) {
        return Err(cfgset_err(
            "BAD_ARG",
            "Direct conf keys are limited to module confs",
            "Core server settings live in the curated list: dml wow config list --json",
        ));
    }
    if !dml_wow::config::is_valid_direct_conf_key(&conf_key) {
        return Err(cfgset_err(
            "BAD_ARG",
            format!("Invalid conf key: {conf_key}"),
            "Letters, digits, dots and underscores only.",
        ));
    }
    if dml_wow::config::is_denylisted_direct_key(&conf_key) {
        return Err(cfgset_err(
            "BAD_ARG",
            format!("{conf_key} is managed by the bot flush tool"),
            "Use: dml wow bots flush --yes --ack flush (backs your characters up first and always disarms the flag afterwards).",
        ));
    }
    if !dml_wow::config::is_single_line(value) {
        return Err(cfgset_err("BAD_ARG", "The value must be a single line", ""));
    }
    if !dml_wow::config::within_max_len(value, 200) {
        return Err(cfgset_err("BAD_ARG", "Value too long (max 200 characters)", ""));
    }

    let cpath = dml_wow::config::direct_conf_path(title_dir, &conf_file).ok_or_else(|| {
        cfgset_err(
            "NOT_FOUND",
            format!("Not an editable module conf: {conf_file}"),
            "See: dml wow config files --json",
        )
    })?;
    let ensured = dml_wow::config::conf_ensure(&cpath)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
    if !ensured {
        return Err(cfgset_err(
            "NOT_FOUND",
            format!("{conf_file} not found (nor its .dist)"),
            "Is the WoW server fully installed?",
        ));
    }
    let mut changed = dml_wow::config::conf_write(&cpath, &conf_key, value)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;

    let ename = dml_wow::config::env_name_for(&conf_key);
    let override_path = title_dir.join("docker-compose.override.yml");
    let mut env_was = cfgset_clean_legacy_env(&override_path, &ename)?;
    if env_was {
        changed = true;
    }

    let mut applied = "none".to_string();
    let mut restart_required = false;
    if changed {
        applied = "restart".to_string();
        restart_required = true;
        if let Some(reload_cmd) = dml_wow::config::conf_reload_cmd(&conf_file) {
            if !env_was && env_frozen(&ename) {
                env_was = true;
            }
            if !env_was {
                let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
                let soap_cfg = dml_wow::soap::SoapConfig::load();
                let outcome = dml_wow::soap::exec(&soap_cfg, reload_cmd);
                if matches!(outcome, dml_wow::soap::SoapOutcome::Ok(_)) {
                    applied = "live".to_string();
                    restart_required = false;
                }
            }
        }
    }
    Ok(serde_json::json!({
        "changed": changed,
        "restart_required": restart_required,
        "applied": applied,
    }))
}

/// Route B — the curated registry-row route (`90-main.sh:2440-2560`).
fn config_set_curated(
    title_dir: &std::path::Path,
    key: &str,
    raw_value: &str,
    rows: &[serde_json::Value],
    soap_lock: &Arc<std::sync::Mutex<()>>,
) -> Result<serde_json::Value, CmdError> {
    let row = rows
        .iter()
        .find(|r| r.get("key").and_then(|v| v.as_str()) == Some(key))
        .ok_or_else(|| {
            cfgset_err("NOT_FOUND", format!("Unknown setting: {key}"), "See: dml wow config list --json")
        })?;
    let kind = row.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let label = row.get("label").and_then(|v| v.as_str()).unwrap_or(key);
    let env = row.get("env").and_then(|v| v.as_str()).unwrap_or("");
    let min_num = row.get("min").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let max_num = row.get("max").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let min_disp = row.get("min").cloned().unwrap_or(serde_json::Value::Null);
    let max_disp = row.get("max").cloned().unwrap_or(serde_json::Value::Null);

    // `_cfg_preamble` (`40-config.sh:161-174`), inlined: the oracle resolves
    // `cfg_sdir` right after parsing the row and BEFORE any type/range
    // validation (`90-main.sh:2440-2445`), so a not-installed server always
    // wins over a bad value with the SAME top-level verdict the oracle gives.
    if !dml_wow::config::wow_server_installed(title_dir) {
        return Err(cfgset_err(
            "NOT_FOUND",
            "WoW Playerbots server not installed",
            "Install it first, then re-run.",
        ));
    }

    let mut value = raw_value.to_string();
    match kind {
        "float" => {
            if !dml_wow::config::float_in_range(&value, min_num, max_num) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("{label} must be a number between {min_disp} and {max_disp}, got: {value}"),
                    "",
                ));
            }
        }
        "int" => {
            if !dml_wow::config::int_in_range(&value, min_num as i64, max_num as i64) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("{label} must be a whole number between {min_disp} and {max_disp}, got: {value}"),
                    "",
                ));
            }
        }
        "bool" => {
            if !dml_wow::config::is_bool01(&value) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("{label} takes 1 (on) or 0 (off), got: {value}"),
                    "",
                ));
            }
        }
        "text" => {
            value = dml_wow::config::sanitize_text_value(&value);
        }
        "char" => {
            if !dml_wow::soap_cmds::valid_charname(&value) {
                return Err(cfgset_err(
                    "BAD_ARG",
                    format!("Invalid character name: {value}"),
                    "1-12 letters/digits/underscore.",
                ));
            }
        }
        _ => {}
    }

    if key == "server.motd" {
        let cmd = dml_wow::soap_cmds::motd_cmd(&value);
        let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
        motd_result(outcome)?;
        return Ok(serde_json::json!({ "changed": true, "restart_required": false }));
    }

    if let Some((conf_file, conf_key)) = dml_wow::config::route_conf(env) {
        let mut write_value = value.clone();
        let mut extra_writes: Vec<(String, String)> = Vec::new();

        if key == "ahbot.character" {
            let db_cfg = dml_wow::db::DbConfig::from_env();
            let params: Vec<mysql::Value> = vec![mysql::Value::from(value.as_str())];
            let res = dml_wow::db::query_with_params(
                &db_cfg,
                dml_wow::db::Database::Characters,
                "SELECT guid, account FROM characters WHERE name = ? LIMIT 1",
                params,
            )
            .map_err(db_err_to_cmd)?;
            let row0 = res
                .rows
                .first()
                .ok_or_else(|| cfgset_err("NOT_FOUND", format!("No such character: {value}"), ""))?;
            let guid = sql_row_int(row0.first()).filter(|g| *g >= 0);
            let acct = sql_row_int(row0.get(1)).filter(|a| *a >= 0);
            let (guid, acct) = match (guid, acct) {
                (Some(g), Some(a)) => (g, a),
                _ => {
                    return Err(cfgset_err(
                        "DB_UNREACHABLE",
                        "Unexpected character lookup result",
                        "",
                    ))
                }
            };
            write_value = guid.to_string();
            extra_writes.push(("AuctionHouseBot.Account".to_string(), acct.to_string()));
        }

        let cpath = dml_wow::config::conf_path_in(title_dir, &conf_file);
        let ensured = dml_wow::config::conf_ensure(&cpath)
            .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
        if !ensured {
            return Err(cfgset_err(
                "NOT_FOUND",
                format!("{conf_file} not found (nor its .dist)"),
                "Is the WoW server fully installed?",
            ));
        }
        let mut changed = dml_wow::config::conf_write(&cpath, &conf_key, &write_value)
            .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;

        for (k, v) in &extra_writes {
            let c = dml_wow::config::conf_write(&cpath, k, v)
                .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
            changed = changed || c;
        }
        if key == "bots.population" {
            let c = dml_wow::config::conf_write(&cpath, "AiPlayerbot.MinRandomBots", &write_value)
                .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {conf_file}: {e}"), ""))?;
            changed = changed || c;
        }

        let mut envnames = vec![dml_wow::config::env_name_for(&conf_key)];
        if key == "bots.population" {
            envnames.push(dml_wow::config::env_name_for("AiPlayerbot.MinRandomBots"));
        }
        if key == "ahbot.character" {
            envnames.push(dml_wow::config::env_name_for("AuctionHouseBot.Account"));
        }

        let override_path = title_dir.join("docker-compose.override.yml");
        let mut env_was = false;
        for ename in &envnames {
            if cfgset_clean_legacy_env(&override_path, ename)? {
                env_was = true;
                changed = true;
            }
        }
        if !env_was {
            for ename in &envnames {
                if env_frozen(ename) {
                    env_was = true;
                    break;
                }
            }
        }

        let mut applied = "none".to_string();
        let mut restart_required = false;
        if changed {
            applied = "restart".to_string();
            restart_required = true;
            if (conf_file == "worldserver.conf" || conf_file == "mod_ahbot.conf") && !env_was {
                let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
                let soap_cfg = dml_wow::soap::SoapConfig::load();
                let outcome = dml_wow::soap::exec(&soap_cfg, "reload config");
                if matches!(outcome, dml_wow::soap::SoapOutcome::Ok(_)) {
                    applied = "live".to_string();
                    restart_required = false;
                }
            }
        }
        return Ok(serde_json::json!({
            "changed": changed,
            "restart_required": restart_required,
            "applied": applied,
        }));
    }

    // Non-conf env column (currently unreachable — every real registry row is
    // either `conf:` or `server.motd`'s `-`; kept for oracle parity).
    let override_path = title_dir.join("docker-compose.override.yml");
    let changed = dml_wow::config::override_env_write(&override_path, env, &value)
        .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write the config override: {e}"), ""))?;
    Ok(serde_json::json!({ "changed": changed, "restart_required": changed }))
}

/// `dml wow config set` port (`90-main.sh:2344-2561`, Task B2a). Native-only.
#[tauri::command]
async fn wow_config_set_native(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let soap_lock = state.soap_lock.clone();
    let config_lock = state.config_lock.clone();
    let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();

    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        // Serializes against every other native conf/override write (Settings
        // AND Module Tuning can both target the same conf file) -- see the
        // doc comment on `AppState::config_lock`.
        let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
        if key.starts_with("conf:") {
            return config_set_direct(&title_dir, &key, &value, &soap_lock);
        }
        let rows = dml_wow::registry::config_registry_rows();
        config_set_curated(&title_dir, &key, &value, rows, &soap_lock)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

// ---------------------------------------------------------------------------
// `wow_config_tuning_set_native` (Task B2b) — native port of
// `dml wow config tuning-set` (`90-main.sh:2859-2934`, the `tuning-set)` case).
// Looks up the row in the cached tuning registry (the same one-shot cache
// `wow_tuning_read` populates), validates by `type` (shared by BOTH
// backends, exactly like the oracle validates once before branching), then
// writes it: `conf` backend is fully native (mirrors `config_set_curated`'s
// conf-write tail almost verbatim, minus the ahbot/bots.population
// special-case companion writes that don't apply to tuning rows); `lua`
// backend defers to the CLI -- see the TODO on that arm below.
// ---------------------------------------------------------------------------

/// `dml wow config tuning-set` port (`90-main.sh:2859-2934`, Task B2b).
/// Native-only.
#[tauri::command]
async fn wow_config_tuning_set_native(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let runner = state.runner.clone();
    let config_lock = state.config_lock.clone();
    let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();

    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        // Serializes against every other native conf/override write -- see
        // the doc comment on `AppState::config_lock`.
        let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
        let rows = dml_wow::registry::tuning_registry_rows();
        let row = rows
            .iter()
            .find(|r| r.get("key").and_then(|v| v.as_str()) == Some(key.as_str()))
            .ok_or_else(|| {
                cfgset_err(
                    "NOT_FOUND",
                    format!("Unknown tuning setting: {key}"),
                    "See: dml wow config tuning-list --json",
                )
            })?;

        let backend = row.get("backend").and_then(|v| v.as_str()).unwrap_or("");
        let file = row.get("file").and_then(|v| v.as_str()).unwrap_or("");
        let ty = row.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let label = row.get("label").and_then(|v| v.as_str()).unwrap_or(key.as_str());
        let module = row.get("module").and_then(|v| v.as_str()).unwrap_or("");
        let min = row.get("min").and_then(|v| v.as_i64()).unwrap_or(0);
        let max = row.get("max").and_then(|v| v.as_i64()).unwrap_or(0);
        // The registry JSON carries no `confkey` column (Task 1's cheap
        // sibling arm deliberately omits it); `tuning_confkey` is the source
        // of truth, kept in lock-step with `_mtune_rows` by `tuning.rs`'s own
        // parity test. Every row the registry can ever emit has an entry
        // here, so this is unreachable in practice -- defensive, not a real
        // oracle branch.
        let confkey = dml_wow::tuning::tuning_confkey(&key).ok_or_else(|| {
            cfgset_err(
                "NOT_FOUND",
                format!("Unknown tuning setting: {key}"),
                "See: dml wow config tuning-list --json",
            )
        })?;

        let norm_value = dml_wow::tuning::validate_tuning_value(ty, &value, label, min, max)
            .map_err(|msg| cfgset_err("BAD_ARG", msg, ""))?;

        if backend == "lua" {
            // TODO: native lua-tuning writer (deferred). Porting
            // `_lua_cfg_write` + `_mtune_to_lua` (the `.lua` assignment
            // editor with re-verify, `40-config.sh:969-1019`) is out of scope
            // for this pass -- only 2 of 13 tuning rows are lua-backend
            // (unlimitedammo.enabled, sitmeansrest.*'s file is also lua but
            // that's 4 rows total; still a small minority). Shell the
            // existing CLI so lua-backend rows keep working (byte-parity,
            // since `dml` does the actual write) without porting the lua
            // editor tonight. The CLI re-validates independently; the
            // validation above still runs first so a bad value fails fast
            // without a subprocess, and to keep both backends going through
            // one shared validation path exactly like the oracle does.
            let env = runner
                .run_json(&["wow", "config", "tuning-set", "--key", &key, "--value", &value])
                .map_err(CmdError::from)?;
            return envelope_to_result(env);
        }

        // `_wow_server_dir` re-check (`90-main.sh:2888-2892`): the oracle runs
        // this AFTER type/range validation but BEFORE either backend branch
        // (it feeds the lua branch's `_lua_cfg_path` too, at 2913) -- for the
        // conf backend specifically, that puts it before `_cfg_conf_ensure`'s
        // own NOT_INSTALLED check, so a not-installed server reports the
        // oracle's uniform "server not installed" rather than "{module} is
        // not installed". The lua branch above already shelled out to the CLI
        // before we get here, so it gets this same check for free.
        if !dml_wow::config::wow_server_installed(&title_dir) {
            return Err(cfgset_err(
                "NOT_FOUND",
                "WoW Playerbots server not installed",
                "Install it first, then re-run.",
            ));
        }

        // backend == "conf" -- same `_cfg_conf_path` resolution as B2a's
        // curated route (`conf_path_in`: worldserver/authserver.conf under
        // `env/dist/etc/`, else `env/dist/etc/modules/{file}`).
        let cpath = dml_wow::config::conf_path_in(&title_dir, file);
        let ensured = dml_wow::config::conf_ensure(&cpath)
            .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {file}: {e}"), ""))?;
        if !ensured {
            return Err(cfgset_err(
                "NOT_INSTALLED",
                format!("{module} is not installed"),
                format!("Install {module} from the Modules page first, then reopen this page."),
            ));
        }
        let changed = dml_wow::config::conf_write(&cpath, confkey, &norm_value)
            .map_err(|e| cfgset_err("WRITE_FAILED", format!("Could not write {file}: {e}"), ""))?;

        Ok(if changed {
            serde_json::json!({
                "key": key, "backend": "conf", "changed": true,
                "restart_required": true, "applied": "restart",
            })
        } else {
            serde_json::json!({
                "key": key, "backend": "conf", "changed": false,
                "restart_required": false, "applied": "none",
            })
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
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

// ---------------------------------------------------------------------------
// NATIVE-MODE `config raw-read`/`raw-reset`/`raw-write` + `pb-keys`/
// `conf-keys` (Part 5a). Shared `_cfg_preamble` equivalent: every arm below
// needs the WoW Playerbots title installed (native has no yq dependency to
// check -- `wow_config_set_native`'s routes already established that a
// native `_cfg_preamble` port is just the dir check). Read paths take no
// lock; the two writers (raw-reset/raw-write) take `config_lock`, same as
// `wow_config_set_native` -- Settings/Modules/raw-editor writes must never
// interleave on the same conf file.
// ---------------------------------------------------------------------------

fn cfg_installed_err() -> CmdError {
    CmdError {
        code: "NOT_FOUND".into(),
        message: "WoW Playerbots server not installed".into(),
        hint: "Install it first, then re-run.".into(),
    }
}

fn cfg_not_editable_err(fname: &str) -> CmdError {
    CmdError {
        code: "NOT_FOUND".into(),
        message: format!("Not an editable file: {fname}"),
        hint: "See: dml wow config files --json".into(),
    }
}

/// `[[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }`
/// -- shared verbatim by the raw-read/raw-reset/raw-write arms
/// (`90-main.sh:2695,2714,2735`; NOTE the EMPTY hint here, unlike conf-keys'
/// own missing-file message which carries a "See: ..." hint).
fn cfg_missing_file_err() -> CmdError {
    CmdError { code: "BAD_ARG".into(), message: "Missing --file <name>".into(), hint: String::new() }
}

/// NATIVE-MODE `config pb-keys` (`90-main.sh:2562-2606`, Part 5a): every
/// active `Key = value` line of `playerbots.conf` (falling back to its
/// `.dist` when the conf doesn't exist yet), each with its own `.dist`
/// default when both files exist -- see [`dml_wow::config::key_browser_rows`]'s
/// doc comment for the exact default-derivation rule.
#[tauri::command]
async fn wow_config_pb_keys_native() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        if !dml_wow::config::wow_server_installed(&title_dir) {
            return Err(cfg_installed_err());
        }
        let pbconf =
            title_dir.join("env").join("dist").join("etc").join("modules").join("playerbots.conf");
        let pbdist = dml_wow::config::dist_sibling(&pbconf);

        let (pbsrc, src_is_dist) =
            if pbconf.is_file() { (pbconf, false) } else { (pbdist.clone(), true) };
        if !pbsrc.is_file() {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "playerbots.conf not found (nor its .dist)".into(),
                hint: "Is the WoW server fully installed?".into(),
            });
        }
        let src_content = std::fs::read_to_string(&pbsrc).unwrap_or_default();
        let dist_content = (!src_is_dist && pbdist.is_file())
            .then(|| std::fs::read_to_string(&pbdist).unwrap_or_default());

        let rows = dml_wow::config::key_browser_rows(&src_content, dist_content.as_deref(), src_is_dist);
        let keys: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| serde_json::json!({ "key": r.key, "value": r.value, "default": r.default, "line": r.line }))
            .collect();
        let source = if src_is_dist { "playerbots.conf.dist" } else { "playerbots.conf" };
        Ok(serde_json::json!({ "source": source, "keys": keys }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `config conf-keys` (`90-main.sh:2607-2669`, Part 5a):
/// `pb-keys` generalized to any editable module conf, plus each key's
/// comment-block help from the `.dist` ([`dml_wow::config::conf_help_lines`]).
#[tauri::command]
async fn wow_config_conf_keys_native(file: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        if file.is_empty() {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: "Missing --file <name>".into(),
                hint: "See: dml wow config files --json".into(),
            });
        }
        // Order matches the oracle exactly (`90-main.sh:2624-2635`): the
        // installed-server check runs BEFORE the core-conf-name rejection.
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        if !dml_wow::config::wow_server_installed(&title_dir) {
            return Err(cfg_installed_err());
        }
        if matches!(
            file.as_str(),
            ".env" | "docker-compose.override.yml" | "worldserver.conf" | "authserver.conf"
        ) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Not a module conf: {file}"),
                hint: "Core server settings live in the curated list: dml wow config list --json".into(),
            });
        }
        let ckpath = dml_wow::config::direct_conf_path(&title_dir, &file).ok_or_else(|| CmdError {
            code: "NOT_FOUND".into(),
            message: format!("Not an editable module conf: {file}"),
            hint: "See: dml wow config files --json".into(),
        })?;
        let ckdist = dml_wow::config::dist_sibling(&ckpath);
        let (cksrc, src_is_dist) =
            if ckpath.is_file() { (ckpath, false) } else { (ckdist.clone(), true) };
        let src_content = std::fs::read_to_string(&cksrc).unwrap_or_default();
        let dist_exists = ckdist.is_file();
        let dist_content =
            (!src_is_dist && dist_exists).then(|| std::fs::read_to_string(&ckdist).unwrap_or_default());

        let rows = dml_wow::config::key_browser_rows(&src_content, dist_content.as_deref(), src_is_dist);

        // Help source: the `.dist` when it exists, else the live conf itself
        // (`90-main.sh:2650-2651`). When `cksrc` IS the dist already
        // (`src_is_dist`), reuse `src_content` instead of re-reading the
        // same file.
        let help_content = if src_is_dist {
            src_content.clone()
        } else if dist_exists {
            dist_content.clone().unwrap_or_else(|| std::fs::read_to_string(&ckdist).unwrap_or_default())
        } else {
            src_content.clone()
        };
        let help_map: std::collections::HashMap<String, String> =
            dml_wow::config::conf_help_lines(&help_content).into_iter().collect();

        let keys: Vec<serde_json::Value> = rows
            .into_iter()
            .map(|r| {
                let help = help_map.get(&r.key).cloned().unwrap_or_default();
                serde_json::json!({ "key": r.key, "value": r.value, "default": r.default, "line": r.line, "help": help })
            })
            .collect();
        let source = if src_is_dist { "dist" } else { "conf" };
        Ok(serde_json::json!({ "file": file, "source": source, "keys": keys }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
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

/// NATIVE-MODE fast read of the account-wide sharing state (Chunk 2 task 5):
/// same shape as `wow_accountwide_get`. A `NOT_FOUND` server-not-installed
/// check happens here (matching `90-main.sh:4110-4113`); "accountwide isn't
/// deployed" is NOT an error — see `dml::accountwide::build_get`'s doc
/// comment. Native mode only.
#[tauri::command]
async fn wow_accountwide_get_native() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        let Some(server_dir) = dml_wow::maint::resolve_server_dir(&title_dir) else {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "WoW Playerbots server not installed".into(),
                hint: "Install it first.".into(),
            });
        };
        Ok(dml_wow::accountwide::build_get(&server_dir))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast write of one account-wide sharing flag (Chunk 2 task 5):
/// same shapes as `wow_accountwide_set` (generic-flag and reputation
/// pick-one, see `dml::accountwide::set_flag`'s doc comment). `--value`/
/// flag-shape validation happens HERE, before the server-dir resolve,
/// matching the bash arm's ordering (`90-main.sh:4149-4155` before
/// `4156-4163`) — `set_flag` re-validates too, so this is belt-and-braces,
/// not the only gate. Serialized under `AppState::config_lock` — same
/// concurrent-native-fs-write hazard as a conf write. Native mode only.
#[tauri::command]
async fn wow_accountwide_set_native(
    key: String,
    value: String,
    variant: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if value != "on" && value != "off" {
        return Err(CmdError { code: "BAD_ARG".into(), message: "--value must be on or off".into(), hint: String::new() });
    }
    if !dml_wow::accountwide::valid_flag(&key) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid flag name: {key}"),
            hint: "Flags look like ENABLE_ACCOUNTWIDE_MOUNTS -- see: dml wow accountwide get --json".into(),
        });
    }
    let config_lock = state.config_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        let Some(server_dir) = dml_wow::maint::resolve_server_dir(&title_dir) else {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "WoW Playerbots server not installed".into(),
                hint: "Install it first.".into(),
            });
        };
        dml_wow::accountwide::set_flag(&server_dir, &key, &value, variant.as_deref())
            .map_err(|e| CmdError { code: e.code.into(), message: e.message, hint: e.hint })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
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

/// NATIVE-MODE `wow bots flush` — see
/// [`dml_wow::destructive::bots_flush_stream`]. No `--yes`/
/// `--ack` parameters exist on either backend: the launcher's typed-"flush"
/// confirm UI is the gate, unchanged from `wow_bots_flush`. Native mode only.
#[tauri::command]
async fn wow_bots_flush_native(on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    require_native_backend()?;
    let soap_lock = state.soap_lock.clone();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::destructive::bots_flush_stream(soap_lock, db_cfg, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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

/// NATIVE-MODE `config raw-reset` (`90-main.sh:2708-2731`, Part 5a):
/// re-copy `<name>.conf.dist` over the live conf, backing up an existing
/// conf first (`<name>.bak`). `.env`/the compose override have no `.dist`
/// and are rejected up front, exactly like the oracle.
#[tauri::command]
async fn wow_config_raw_reset_native(
    file: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let config_lock = state.config_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        if file.is_empty() {
            return Err(cfg_missing_file_err());
        }
        let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        if !dml_wow::config::wow_server_installed(&title_dir) {
            return Err(cfg_installed_err());
        }
        let fpath = dml_wow::config::cfg_file_path(&title_dir, &file)
            .ok_or_else(|| cfg_not_editable_err(&file))?;
        if matches!(file.as_str(), ".env" | "docker-compose.override.yml") {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: "That file has no defaults to reset to".into(),
                hint: String::new(),
            });
        }
        let dist = dml_wow::config::dist_sibling(&fpath);
        if !dist.is_file() {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: format!("No {file}.dist to reset from"),
                hint: String::new(),
            });
        }
        let mut backup = serde_json::Value::Null;
        if fpath.is_file() {
            let bak = dml_wow::config::bak_sibling(&fpath);
            std::fs::copy(&fpath, &bak).map_err(|e| CmdError {
                code: "WRITE_FAILED".into(),
                message: format!("Could not write {file}: {e}"),
                hint: String::new(),
            })?;
            backup = serde_json::Value::String(format!("{file}.bak"));
        }
        std::fs::copy(&dist, &fpath).map_err(|e| CmdError {
            code: "WRITE_FAILED".into(),
            message: format!("Could not write {file}: {e}"),
            hint: String::new(),
        })?;
        Ok(serde_json::json!({ "reset": true, "file": file, "backup": backup }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// The oracle's `raw-read` arm never reads the file directly -- it captures
/// it through a `$(cat "$fpath")` command substitution
/// (`90-main.sh:2702,2706`), and POSIX command substitution strips EVERY
/// trailing newline from the captured text (not just one). A plain
/// `std::fs::read_to_string` keeps the file's real trailing newline(s), so
/// without this the native reader would hand back one extra `\n` at EOF for
/// any conf that (like every conf in this codebase) ends in a newline --
/// caught live by `part5a_parity.rs`.
fn strip_command_sub_trailing_newlines(s: &str) -> &str {
    s.trim_end_matches('\n')
}

/// NATIVE-MODE `config raw-read` (`90-main.sh:2692-2707`, Part 5a): read an
/// allowlisted file, falling back to its `.dist` when only that exists yet
/// (the first save then creates the real conf via raw-write).
#[tauri::command]
async fn wow_config_raw_read_native(file: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        if file.is_empty() {
            return Err(cfg_missing_file_err());
        }
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        if !dml_wow::config::wow_server_installed(&title_dir) {
            return Err(cfg_installed_err());
        }
        let fpath = dml_wow::config::cfg_file_path(&title_dir, &file)
            .ok_or_else(|| cfg_not_editable_err(&file))?;
        let dist = dml_wow::config::dist_sibling(&fpath);
        if !fpath.is_file() && dist.is_file() {
            let content = std::fs::read_to_string(&dist).unwrap_or_default();
            return Ok(serde_json::json!({
                "file": file,
                "source": "dist",
                "content": strip_command_sub_trailing_newlines(&content),
            }));
        }
        if !fpath.is_file() {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: format!("File does not exist yet: {file}"),
                hint: String::new(),
            });
        }
        let content = std::fs::read_to_string(&fpath).unwrap_or_default();
        Ok(serde_json::json!({
            "file": file,
            "source": "conf",
            "content": strip_command_sub_trailing_newlines(&content),
        }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `config raw-write` (`90-main.sh:2733-2774`, Part 5a):
/// full-file write with the arm's exact guards, in the SAME order as the
/// oracle -- override-YAML syntax validation happens BEFORE the `.env`/
/// override read-only rejection (so submitting broken YAML for the override
/// reports "not valid YAML", never "read-only"), tmp+rename atomic, with an
/// automatic `.bak` of any existing file.
#[tauri::command]
async fn wow_config_raw_write_native(
    file: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let config_lock = state.config_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        if file.is_empty() {
            return Err(cfg_missing_file_err());
        }
        let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        if !dml_wow::config::wow_server_installed(&title_dir) {
            return Err(cfg_installed_err());
        }
        let fpath = dml_wow::config::cfg_file_path(&title_dir, &file)
            .ok_or_else(|| cfg_not_editable_err(&file))?;

        if file == "docker-compose.override.yml"
            && serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&content).is_err()
        {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: "That is not valid YAML - not saved".into(),
                hint: "Fix the syntax and save again.".into(),
            });
        }
        // SECURITY: same posture as `_cfg_file_path`'s writable module-conf
        // allowlist -- `.env`/the compose override are readable (raw-read)
        // but NOT writable here (see `cli/src/90-main.sh:2752-2765`'s own
        // comment for the exact rationale: this + `games restart` would let
        // the editor drive host command execution).
        if matches!(file.as_str(), ".env" | "docker-compose.override.yml") {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: "That file is read-only in the editor".into(),
                hint: "Change these settings from the Settings tab; .env and the compose override can't be overwritten here.".into(),
            });
        }

        if let Some(parent) = fpath.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut backup = serde_json::Value::Null;
        if fpath.is_file() {
            let bak = dml_wow::config::bak_sibling(&fpath);
            std::fs::copy(&fpath, &bak).map_err(|e| CmdError {
                code: "WRITE_FAILED".into(),
                message: format!("Could not write {file}: {e}"),
                hint: String::new(),
            })?;
            backup = serde_json::Value::String(format!("{file}.bak"));
        }
        dml_wow::config::atomic_write(&fpath, &content).map_err(|e| CmdError {
            code: "WRITE_FAILED".into(),
            message: format!("Could not write {file}: {e}"),
            hint: String::new(),
        })?;
        Ok(serde_json::json!({ "written": true, "backup": backup }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
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

/// NATIVE-MODE fast world-only restart: same flow/messages/codes as `dml wow
/// world-restart` (see [`dml_wow::lifecycle::world_restart_stream`]'s doc
/// comment), via direct `docker`/SOAP calls instead of shelling `dml`. Native
/// mode only — WSL keeps calling `wow_world_restart`.
#[tauri::command]
async fn wow_world_restart_native(
    on_event: Channel<serde_json::Value>,
    no_saveall: bool,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    let soap_lock = state.soap_lock.clone();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::lifecycle::world_restart_stream(no_saveall, soap_lock, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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

/// NATIVE-MODE `wow bridge-setup`/`party-setup`/`setup` -- see
/// [`dml_wow::bridge::bridge_setup_stream`]. ONE native command backs BOTH
/// `wowBridgeSetup` (GMTools.svelte) and `wowPartySetup` (Playerbots.svelte)
/// in `api.ts` -- like their WSL siblings `wow_bridge_setup`/
/// `wow_party_setup` above/below, they are aliases for the identical bash arm
/// (`bridge-setup|party-setup|setup)`), so one native implementation covers
/// both call sites. Native mode only — WSL keeps calling
/// `wow_bridge_setup`/`wow_party_setup`.
#[tauri::command]
async fn wow_bridge_setup_native(on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    require_native_backend()?;
    let soap_lock = state.soap_lock.clone();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::bridge::bridge_setup_stream(soap_lock, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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

/// `teleport-coords`'s SELECT lookup — `90-main.sh:1920`. A named const (not
/// an inline literal) purely so the exact SQL text is unit-testable, same
/// convention as `RETURN_HOME_SELECT_SQL`/`RETURN_HOME_UPDATE_SQL`.
const TELEPORT_COORDS_SELECT_SQL: &str = "SELECT guid, online FROM characters WHERE name = ? LIMIT 1";
/// `teleport-coords`'s position UPDATE — `90-main.sh:1929`.
const TELEPORT_COORDS_UPDATE_SQL: &str =
    "UPDATE characters SET position_x=?, position_y=?, position_z=?, map=?, orientation=0 WHERE guid=?";

/// `CHAR_ONLINE` — `90-main.sh:1925-1928`, byte-identical message/hint.
fn char_online_err(char_name: &str) -> CmdError {
    CmdError {
        code: "CHAR_ONLINE".into(),
        message: format!("Character must be logged out: {char_name}"),
        hint: "Character must be logged out.".into(),
    }
}

/// NATIVE-MODE `wow teleport-coords` (`90-main.sh:1895-1933`, Part 5a).
/// UNLIKE `teleport`/`gm return-home`, this arm NEVER calls SOAP -- an
/// ONLINE character is REJECTED (`CHAR_ONLINE`), not teleported live: a
/// running worldserver holds its own in-memory position and would clobber
/// this direct write on the character's next auto-save/logout. Order
/// matches the oracle exactly: validate char/map/x/y/z -> lookup
/// (guid, online) -> reject if online -> UPDATE. THIRD sanctioned direct
/// `characters` write (see `dml::db`/`gm_return_home`'s doc comments) --
/// same table, same `orientation=0` convention, same guid-keyed WHERE.
#[tauri::command]
async fn wow_teleport_coords_native(
    char_name: String,
    map: u32,
    x: f64,
    y: f64,
    z: f64,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&char_name) {
        return Err(bad_arg(format!("Invalid character name: {char_name}")));
    }
    if !dml_wow::soap_cmds::valid_map_id(map) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid map id: {map}"),
            hint: "A map id is 1-3 digits, e.g. --map 0 for Eastern Kingdoms.".into(),
        });
    }
    // Message text matches the oracle's exactly (`90-main.sh:1917-1919`):
    // "Invalid coordinate: $value", the SAME wording for all three axes --
    // it never names which one failed, just echoes the bad value.
    for v in [x, y, z] {
        if !dml_wow::soap_cmds::valid_coord(v) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Invalid coordinate: {v}"),
                hint: "Coordinates are plain numbers with a magnitude of 20000 or less.".into(),
            });
        }
    }

    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        let params: Vec<mysql::Value> = vec![mysql::Value::from(char_name.as_str())];
        let res = dml_wow::db::query_with_params(
            &cfg,
            dml_wow::db::Database::Characters,
            TELEPORT_COORDS_SELECT_SQL,
            params,
        )
        .map_err(db_err_to_cmd)?;
        let row = res.rows.first().ok_or_else(|| CmdError {
            code: "NOT_FOUND".into(),
            message: format!("No such character: {char_name}"),
            hint: String::new(),
        })?;
        let guid = sql_row_int(row.first()).filter(|g| *g >= 0).ok_or_else(|| CmdError {
            code: "DB_UNREACHABLE".into(),
            message: "Unexpected character lookup result".into(),
            hint: String::new(),
        })?;
        let online = sql_row_int(row.get(1)).unwrap_or(0);
        if online != 0 {
            return Err(char_online_err(&char_name));
        }

        let update_params: Vec<mysql::Value> = vec![
            mysql::Value::from(x),
            mysql::Value::from(y),
            mysql::Value::from(z),
            mysql::Value::from(map),
            mysql::Value::from(guid as u64),
        ];
        dml_wow::db::execute(&cfg, dml_wow::db::Database::Characters, TELEPORT_COORDS_UPDATE_SQL, update_params)
            .map_err(|_e| CmdError {
                code: "DB_UNREACHABLE".into(),
                message: "Could not update the character's position".into(),
                hint: "Is ac-database running?".into(),
            })?;
        Ok(serde_json::json!({
            "teleported": true,
            "char": char_name,
            "map": map,
            "x": x,
            "y": y,
            "z": z,
        }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
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

// -----------------------------------------------------------------------
// Native SOAP commands (Task A2b) -- native siblings of the `dml`-backed
// account/gm/mail/teleport/console-send writes above. Each validates via the
// pure builders in `dml::soap_cmds` (A2a), fires over `dml::soap::exec`
// inside `spawn_blocking` (network -- must never run on the async runtime
// thread) under `AppState::soap_lock` (the worldserver's SOAP listener runs
// on a single world thread, same reason bash serializes under
// `~/.dml/soap.lock`), then wraps the result in the SAME JSON shape the
// matching `90-main.sh` arm emits. Coords-teleport / return-home (A2c) and
// MOTD/announce (Subsystem B) are NOT here.
//
// FAULT-TEXT PARITY. Every arm below has its own `case "$rc" in` block in
// the bash oracle, and several of them differ from the two generic mappers
// in `dml::soap_cmds` (`outcome_to_result_raw`/`_decoded`) on at least one
// branch (a different SOAP_AUTH message, a fixed non-decoded fault string,
// a different SOAP_UNREACHABLE message/hint, or a different hint) -- those
// get a small local mapper each, copied verbatim from the arm's `json_err`
// calls rather than reusing the generic ones, per the brief's "match the
// exact fault hints per arm" instruction. `account_result` (the `account`
// arm, `90-main.sh:1999-2010`) reuses `outcome_to_result_decoded` for its
// Ok/Fault/Auth branches (those three match byte-for-byte) but overrides
// Unreachable with the arm's own `Could not reach SOAP at $(soap_url)` /
// `Is the worldserver running?` wording. `console_send_result` (the
// `console-send` arm, `90-main.sh:1736-1746` -- the true oracle for
// `wow_console_send_native`/`wowConsoleSend`, NOT `soap-exec`) copies that
// arm's case block verbatim, including its entity-decode of both the Ok and
// Fault paths and its whitespace-only empty-command check.
// -----------------------------------------------------------------------

/// `SoapOutcome -> CmdError` matching `_party_fire`'s exact case block
/// (`cli/src/50-party.sh:67-79`) -- used by the bridge-backed gm ops
/// (gold/heal/revive/summon), which all fire through `_party_fire`. `label`
/// is the short noun `_party_fire`'s caller passes as its `$2` (e.g. "gold",
/// "heal", "revive", "summon"), spliced into the fixed fault message. Unlike
/// the generic mappers, the SOAP_FAULT text here is NEVER the server's own
/// fault string -- bash's `_party_fire` discards `$out` entirely on rc=2.
fn party_fire_result(o: dml_wow::soap::SoapOutcome, label: &str) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(_) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: format!("The {label} command was rejected"),
            hint: "Deploy the server bridges (bridge-setup) and restart the server first.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP auth failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "Is it running?".into(),
        }),
    }
}

/// `SoapOutcome -> CmdError` for `gm level` (`90-main.sh:3509-3516`): the
/// stock `.character level` command. The fault case is a FIXED message (not
/// the server's fault text -- bash discards `$out` on rc=2), and the auth
/// message is "SOAP auth failed" (shorter than the generic mappers'
/// "SOAP authentication failed" -- this arm's own wording, not a typo).
fn gm_level_result(o: dml_wow::soap::SoapOutcome) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(_) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: "The level command was rejected".into(),
            hint: "Does the character exist? The server said no.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP auth failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "Is it running?".into(),
        }),
    }
}

/// `SoapOutcome -> CmdError` for `gm at-login` (`90-main.sh:3595-3601`): the
/// stock `character <flag>` command. The fault text IS the server's own
/// (decoded) fault string here, unlike `gm_level_result` -- but the auth
/// message is still the shorter "SOAP auth failed" this arm-family uses.
fn gm_at_login_result(o: dml_wow::soap::SoapOutcome) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: dml_wow::soap_cmds::soap_text_decode(&t),
            hint: "The worldserver rejected the command.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP auth failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "Is it running?".into(),
        }),
    }
}

/// `SoapOutcome -> CmdError` for `mail-item` (`90-main.sh:1828-1833`): RAW
/// (undecoded) fault text, its own hint, and empty-hint auth/a different
/// unreachable hint than the generic mappers.
fn mail_result(o: dml_wow::soap::SoapOutcome) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: t,
            hint: "The server rejected the mail command.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP authentication failed".into(),
            hint: String::new(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "Run: dml wow soap-setup, then start the server.".into(),
        }),
    }
}

/// `SoapOutcome -> CmdError` for `server.motd` (`90-main.sh:2475-2481`, Task
/// B2a): RAW fault text with its own hint; a different (unstarted-server)
/// unreachable hint than the generic mappers — this is the one arm where
/// SOAP failure means "start the server first" rather than "is it running?".
fn motd_result(o: dml_wow::soap::SoapOutcome) -> Result<(), CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(_) => Ok(()),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: t,
            hint: "The server rejected the motd command.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP authentication failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "The server must be running to change the message of the day - start it first."
                .into(),
        }),
    }
}

/// `SoapOutcome -> CmdError` for `teleport` (`90-main.sh:1888-1893`): RAW
/// (undecoded) fault text with its own hint; empty-hint auth/unreachable.
fn teleport_result(o: dml_wow::soap::SoapOutcome) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: t,
            hint: "Unknown location? See dml wow teleport-list.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP authentication failed".into(),
            hint: String::new(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: String::new(),
        }),
    }
}

/// `SoapOutcome -> CmdError` for `gm return-home`'s ONLINE arm
/// (`90-main.sh:3629-3634`, Task A2c): decoded fault text (the SAME decode
/// `gm_at_login_result` uses) but a return-home-specific fault hint about
/// combat/flight-path; "SOAP auth failed" (the shorter wording this arm
/// family uses, like `gm_level_result`/`gm_at_login_result`); generic
/// unreachable.
fn return_home_online_result(o: dml_wow::soap::SoapOutcome) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: dml_wow::soap_cmds::soap_text_decode(&t),
            hint: "The character can't be teleported in combat or on a flight path -- try again once it is idle."
                .into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP auth failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "Is it running?".into(),
        }),
    }
}

/// One faction capital: the fixed teleport/DB-write target for
/// `gm return-home` (`90-main.sh:3614-3621`). `name`/`map`/coords are FIXED
/// LITERALS -- never derived from user input; only the character lookup
/// (`player`, `race`) that picks WHICH capital is user-influenced, and that
/// influence is limited to a closed 10-race case match.
struct Capital {
    name: &'static str,
    map: i32,
    x: f64,
    y: f64,
    z: f64,
}

/// `characters.race` -> faction capital, matching the oracle's exact case
/// block byte-for-byte: Alliance races `1,3,4,7,11` -> Stormwind (map 0);
/// Horde races `2,5,6,8,10` -> Orgrimmar (map 1); anything else (e.g. race 9
/// = goblin, which owns no faction capital in this map) -> `None`, matching
/// bash's `*)` fallthrough.
fn faction_capital(race: u8) -> Option<Capital> {
    match race {
        1 | 3 | 4 | 7 | 11 => Some(Capital { name: "Stormwind", map: 0, x: -8819.3, y: 636.2, z: 94.1 }),
        2 | 5 | 6 | 8 | 10 => Some(Capital { name: "Orgrimmar", map: 1, x: 1609.2, y: -4407.7, z: 17.5 }),
        _ => None,
    }
}

/// `gm return-home`'s character lookup (`90-main.sh:3616-3617`). A named
/// const (not an inline literal) so the exact SQL text is unit-testable
/// (brief's "SQL builder test" requirement) independent of the live-DB path.
const RETURN_HOME_SELECT_SQL: &str = "SELECT guid, race, online FROM characters WHERE name = ? LIMIT 1";

/// `gm return-home`'s OFFLINE-arm position write (`90-main.sh:3648-3649`).
/// Same reasoning as [`RETURN_HOME_SELECT_SQL`].
const RETURN_HOME_UPDATE_SQL: &str =
    "UPDATE characters SET position_x=?, position_y=?, position_z=?, map=?, orientation=0 WHERE guid=?";

/// `NOT_FOUND` for an offline character, matching `_gm_require_online`
/// (`cli/src/55-gm.sh:9-14`) exactly.
fn not_online_err(player: &str) -> CmdError {
    CmdError {
        code: "NOT_FOUND".into(),
        message: format!("Character not online: {player}"),
        hint: "This action needs the character logged in. (Set level works offline.)".into(),
    }
}

/// Whether `player` is currently online -- a native-mode port of
/// `_gm_require_online`/`_party_online_guid` (`cli/src/55-gm.sh:9-14`,
/// `cli/src/50-party.sh:46-49`): a `characters` row with `online=1`. Any
/// query failure reads as "not online", matching bash: `_party_online_guid`
/// redirects `db_chars_query`'s stderr to `/dev/null` and always `return`s 0,
/// so a DB error there surfaces as an empty guid (== not online) rather than
/// a separate DB_UNREACHABLE branch -- this mirrors that swallow rather than
/// inventing a new error path the oracle doesn't have.
fn char_is_online(cfg: &dml_wow::db::DbConfig, player: &str) -> bool {
    let params: Vec<mysql::Value> = vec![mysql::Value::from(player)];
    dml_wow::db::query_with_params(
        cfg,
        dml_wow::db::Database::Characters,
        "SELECT guid FROM characters WHERE name=? AND online=1 LIMIT 1",
        params,
    )
    .map(|res| !res.rows.is_empty())
    .unwrap_or(false)
}

/// Split a mail `--items` CSV the way bash's `IFS=',' read -ra specs <<<
/// "$items"` does: an EMPTY string splits to ZERO fields (bash's word
/// splitting produces no fields for empty input), unlike Rust's
/// `"".split(',')` which yields one empty-string field -- that mismatch
/// would turn an empty `items` arg into a "Malformed item spec: " BAD_ARG
/// instead of the oracle's "Provide 1-12 items…" one, so the empty case is
/// special-cased. Any other input (including doubled/trailing commas, which
/// bash also turns into empty fields) splits exactly like `str::split`.
fn split_mail_items(items: &str) -> Vec<&str> {
    if items.is_empty() {
        Vec::new()
    } else {
        items.split(',').collect()
    }
}

/// `SoapOutcome -> CmdError` for the `console-send` arm (`90-main.sh:1736-
/// 1746`) -- the true bash sibling of `wow_console_send_native` (both are
/// what `wowConsoleSend()` in `api.ts` calls for the Console tab, native vs
/// WSL). Unlike the generic `outcome_to_result_raw`, this arm entity-decodes
/// BOTH the Ok result and the Fault text, and uses its own SOAP_UNREACHABLE
/// wording (`Could not reach SOAP at $(soap_url)` / mentions `soap-setup`).
fn console_send_result(o: dml_wow::soap::SoapOutcome, soap_url: &str) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(dml_wow::soap_cmds::soap_text_decode(&t)),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: dml_wow::soap_cmds::soap_text_decode(&t),
            hint: "The worldserver rejected the command.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP authentication failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: format!("Could not reach SOAP at {soap_url}"),
            hint: "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup".into(),
        }),
    }
}

/// NATIVE-MODE: the free-text console/SOAP command box, wired to the
/// `console-send` oracle (`90-main.sh:1725-1746`) -- the arm
/// `wow_console_send`/`wowConsoleSend` actually shells in WSL mode
/// (`dml wow console-send --command ...`, see `wow_console_send` above),
/// NOT `soap-exec` -- so this keeps native and WSL mode byte-identical for
/// the same Console-tab action. The empty-command check mirrors bash's
/// whitespace-only test (`[[ -z "${cmd//[[:space:]]/}" ]]`), not a plain
/// `is_empty()`, so a command that's e.g. all spaces is also rejected.
#[tauri::command]
async fn wow_console_send_native(
    command: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if command.trim().is_empty() {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: "console-send requires a non-empty --command".into(),
            hint: "Example: dml wow console-send --command \"server info\" --json".into(),
        });
    }
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &command);
        let result = console_send_result(outcome, &cfg.url)?;
        Ok(serde_json::json!({ "result": result }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// `SoapOutcome -> CmdError` for the `account` arm's catch-all case block
/// (`90-main.sh:1999-2010`), shared by create/set-password/set-gm/delete.
/// Ok/Fault/Auth match `outcome_to_result_decoded` byte-for-byte (both use
/// the same decoded-fault / "SOAP authentication failed" text), but this
/// arm's Unreachable branch has its own wording -- `Could not reach SOAP at
/// $(soap_url)` / `Is the worldserver running?` -- instead of the generic
/// mapper's `Could not reach the server` / `Is it running?`.
fn account_result(o: dml_wow::soap::SoapOutcome, soap_url: &str) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: format!("Could not reach SOAP at {soap_url}"),
            hint: "Is the worldserver running?".into(),
        }),
        other => dml_wow::soap_cmds::outcome_to_result_decoded(other),
    }
}

/// NATIVE-MODE account create (`90-main.sh:1952-2010`, `asub == create`).
#[tauri::command]
async fn wow_account_create_native(
    user: String,
    pass: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::account_create_cmd(&user, &pass)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        account_result(outcome, &cfg.url)?;
        Ok(serde_json::json!({ "created": true, "user": user }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE account set-password (`90-main.sh:1952-2010`, `asub ==
/// set-password`).
#[tauri::command]
async fn wow_account_set_password_native(
    user: String,
    pass: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::account_set_password_cmd(&user, &pass)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        account_result(outcome, &cfg.url)?;
        Ok(serde_json::json!({ "password_set": true, "user": user }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE account set-gm (`90-main.sh:1952-2010`, `asub == set-gm`).
/// `level` arrives as a number over IPC; stringified before the A2a builder
/// (which only regex-matches it, same as bash) so out-of-`0..=3` values
/// still fail with the exact BAD_ARG the CLI gives.
#[tauri::command]
async fn wow_account_set_gm_native(
    user: String,
    level: u8,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let level_str = level.to_string();
    let cmd = dml_wow::soap_cmds::account_set_gm_cmd(&user, &level_str)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        account_result(outcome, &cfg.url)?;
        Ok(serde_json::json!({ "gm_set": true, "user": user, "level": level }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE account delete (`90-main.sh:1952-2010`, `asub == delete`) --
/// the admin-account refusal happens inside `account_delete_cmd` (A2a).
#[tauri::command]
async fn wow_account_delete_native(
    user: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::account_delete_cmd(&user)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        account_result(outcome, &cfg.url)?;
        Ok(serde_json::json!({ "deleted": true, "user": user }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `.character level` (`90-main.sh:3493-3517`) -- stock AC
/// command, works for OFFLINE characters too (no online precondition).
#[tauri::command]
async fn wow_gm_level_native(
    player: String,
    level: i32,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::gm_level_cmd(&player, level)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        gm_level_result(outcome)?;
        Ok(serde_json::json!({ "leveled": true, "player": player, "level": level }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `character <flag>` (`90-main.sh:3579-3601`) -- stock AC
/// per-character at-next-login flag, works for OFFLINE characters too.
#[tauri::command]
async fn wow_gm_at_login_native(
    player: String,
    flag: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::gm_at_login_cmd(&player, &flag)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        gm_at_login_result(outcome)?;
        Ok(serde_json::json!({ "applied": true, "player": player, "flag": flag }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `dml_gm_money` bridge command (`90-main.sh:3519-3537`) --
/// REQUIRES the character online (`_gm_require_online`), checked BEFORE the
/// SOAP fire, same order as the oracle.
#[tauri::command]
async fn wow_gm_gold_native(
    player: String,
    gold: i32,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::gm_gold_cmd(&player, gold)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        if !char_is_online(&cfg, &player) {
            return Err(not_online_err(&player));
        }
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
        party_fire_result(outcome, "gold")?;
        Ok(serde_json::json!({ "gold_set": true, "player": player, "gold": gold }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `dml_gm_health` bridge command (`90-main.sh:3538-3545`) --
/// REQUIRES the character online, checked BEFORE the SOAP fire.
#[tauri::command]
async fn wow_gm_heal_native(
    player: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::gm_heal_cmd(&player)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        if !char_is_online(&cfg, &player) {
            return Err(not_online_err(&player));
        }
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
        party_fire_result(outcome, "heal")?;
        Ok(serde_json::json!({ "healed": true, "player": player }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `dml_gm_revive` bridge command (`90-main.sh:3546-3552`) --
/// REQUIRES the character online, checked BEFORE the SOAP fire.
#[tauri::command]
async fn wow_gm_revive_native(
    player: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::gm_revive_cmd(&player)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        if !char_is_online(&cfg, &player) {
            return Err(not_online_err(&player));
        }
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
        party_fire_result(outcome, "revive")?;
        Ok(serde_json::json!({ "revived": true, "player": player }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `dml_summon_npc` bridge command (`90-main.sh:3554-3577`).
/// Order matches the oracle exactly: validate -> creature_template
/// existence+name lookup (World DB) -> online check (Characters DB) -> SOAP
/// fire -> success with the looked-up NPC name.
#[tauri::command]
async fn wow_gm_summon_native(
    player: String,
    entry: i32,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::gm_summon_cmd(&player, entry)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        let params: Vec<mysql::Value> = vec![mysql::Value::from(entry)];
        let npc_res = dml_wow::db::query_with_params(
            &cfg,
            dml_wow::db::Database::World,
            "SELECT name FROM creature_template WHERE entry=? LIMIT 1",
            params,
        )
        .map_err(|_e| CmdError {
            code: "DB_UNREACHABLE".into(),
            message: "Could not check the creature entry".into(),
            hint: "Is ac-database running?".into(),
        })?;
        let npc_name: Option<String> =
            npc_res.rows.first().and_then(|r| r.first()).and_then(|v| match v {
                dml_wow::db::SqlValue::Text(s) => Some(s.clone()),
                dml_wow::db::SqlValue::Int(i) => Some(i.to_string()),
                dml_wow::db::SqlValue::Null => None,
            });
        let npc_name = match npc_name {
            Some(n) if !n.is_empty() => n,
            _ => {
                return Err(CmdError {
                    code: "NOT_FOUND".into(),
                    message: format!("No creature with entry {entry}"),
                    hint: "Check the id (creature_template.entry).".into(),
                })
            }
        };
        if !char_is_online(&cfg, &player) {
            return Err(not_online_err(&player));
        }
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
        party_fire_result(outcome, "summon")?;
        Ok(serde_json::json!({
            "summoned": true,
            "player": player,
            "entry": entry,
            "npc": npc_name,
        }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

// ---------------------------------------------------------------------------
// NATIVE-MODE party add/kick/dismiss-all/relogin/botcmd + preset save/
// delete/load/show/import (Chunk 5, Part 5b). Faithful ports of the matching
// sub-arms of `cli/src/90-main.sh`'s `party)` case (3067-3483). Pure
// validators/builders/SQL text live in `dml::party`; every DB read below is
// bound-param over `db::query_with_params` (Characters DB — same schema
// `party online`/`party list`/the gm bridge commands already read), every
// SOAP fire reuses `party_fire_result` (== `_party_fire`'s exact case
// block), and `preset-load` follows the SAME ndjson vocabulary as
// `modmgr::module_install_stream`/`lifecycle::world_restart_stream`.
// ---------------------------------------------------------------------------

/// `NOT_FOUND` for an offline character in a `party`-family arm — same code
/// as `not_online_err` (gm) but with a CALLER-SUPPLIED hint, since each
/// `party` sub-arm's oracle spells a slightly different one (`90-main.sh`:
/// add "Log the character into the game first, then try again.";
/// dismiss-all/preset-save/preset-load "Log the character into the game
/// first."; botcmd's bot-side check "The bot must be in the world -- is it
/// still in your party?").
fn party_not_online_err(who: &str, hint: &str) -> CmdError {
    CmdError { code: "NOT_FOUND".into(), message: format!("Character not online: {who}"), hint: hint.into() }
}

/// `_party_spec_names` (`50-party.sh:151-165`) read straight off the
/// deployed playerbots.conf (or its `.dist`) via the already-native `party_
/// specs` reader — the single source of truth `wow_party_specs_read` also
/// uses. `None` when no conf is deployed at all (the caller then falls back
/// to `valid_bot_spec`'s static mirror), matching `_party_pb_conf`'s own
/// "nothing deployed" case.
fn live_spec_names(title_dir: &std::path::Path) -> Option<Vec<String>> {
    let (conf_path, _source) = dml_wow::party_specs::find_conf(title_dir)?;
    let content = std::fs::read_to_string(&conf_path).ok()?;
    Some(
        dml_wow::party_specs::parse_spec_rows(&content)
            .into_iter()
            .map(|r| r.name)
            .filter(|n| !n.is_empty())
            .collect(),
    )
}

/// `SoapOutcome -> Result<String, CmdError>` for `dismiss-all`'s "every fire
/// failed" case (`90-main.sh:3224-3231`): SAME code/hint table as
/// `party_fire_result`, but the FAULT message is the arm's own fixed "Every
/// dismiss was rejected" (not "The dismiss command was rejected" —
/// `dismiss-all` has no single "the" command, it fires one per bot).
fn dismiss_fire_result(o: dml_wow::soap::SoapOutcome) -> Result<String, CmdError> {
    use dml_wow::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(_) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: "Every dismiss was rejected".into(),
            hint: "Deploy the server bridges (bridge-setup) and restart the server first.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP auth failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "Is it running?".into(),
        }),
    }
}

/// NATIVE-MODE `party add` (`90-main.sh:3067-3130`). Validates + (if given) a
/// live-checked `--spec` BEFORE ever touching the DB/SOAP; the online-guid
/// lookup, pre-fire member snapshot, SOAP fire, new-member poll, and the
/// post-join spec whispers all run inside the one `spawn_blocking` closure,
/// same ordering as the oracle.
#[tauri::command]
async fn wow_party_add_native(
    player: String,
    class: String,
    gender: Option<String>,
    spec: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let gender = gender.unwrap_or_default();
    let cmd = dml_wow::party::party_add_cmd(&player, &class, &gender)?;
    let spec = spec.filter(|s| !s.is_empty());
    if let Some(s) = &spec {
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        let live = live_spec_names(&title_dir);
        if !dml_wow::party::valid_bot_spec(s, live.as_deref()) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Unknown spec: {s}"),
                hint: "A premade spec name like 'frost pve' -- see the launcher's role picker for the full list.".into(),
            });
        }
    }
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let db_cfg = dml_wow::db::DbConfig::from_env();
        let pguid = party_online_guid(&db_cfg, &player)
            .ok_or_else(|| party_not_online_err(&player, "Log the character into the game first, then try again."))?;
        let before = group_member_guids(&db_cfg, pguid);
        {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let soap_cfg = dml_wow::soap::SoapConfig::load();
            let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
            party_fire_result(outcome, "add")?;
        }
        let Some(newguid) = wait_new_member(&db_cfg, pguid, &before) else {
            return Ok(if let Some(s) = &spec {
                serde_json::json!({"added":true,"joined":false,"bot":null,"note":"Added but spec not applied -- bot not attached in time","spec":s,"spec_applied":false})
            } else {
                serde_json::json!({"added":true,"joined":false,"bot":null,"note":"Spawned but not attached yet -- give it a moment and Refresh."})
            });
        };
        let Some(botname) = char_name_by_guid(&db_cfg, newguid) else {
            return Ok(if let Some(s) = &spec {
                serde_json::json!({"added":true,"joined":true,"bot":null,"note":"Added but spec not applied -- bot not attached in time","spec":s,"spec_applied":false})
            } else {
                serde_json::json!({"added":true,"joined":true,"bot":null,"note":null})
            });
        };
        if let Some(s) = &spec {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let soap_cfg = dml_wow::soap::SoapConfig::load();
            let o1 = dml_wow::soap::exec(&soap_cfg, &dml_wow::party::spec_whisper_cmd(&player, &botname, s));
            party_fire_result(o1, "spec")?;
            let o2 = dml_wow::soap::exec(&soap_cfg, &dml_wow::party::autogear_whisper_cmd(&player, &botname));
            party_fire_result(o2, "spec")?;
            Ok(serde_json::json!({"added":true,"joined":true,"bot":botname,"note":null,"spec":s,"spec_applied":true}))
        } else {
            Ok(serde_json::json!({"added":true,"joined":true,"bot":botname,"note":null}))
        }
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party kick` (`90-main.sh:3158-3181`): uninvite (hard fire,
/// `_party_fire`-mapped) then a best-effort master `logout` whisper (its
/// failure only flips `dismissed` to `false`, never aborts the command —
/// matches the 2026-07-22 smoke fix this arm's own comment documents).
#[tauri::command]
async fn wow_party_kick_native(
    player: String,
    bot: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&player) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid player name: {player}"),
            hint: "Kick needs --player (the bot's master) so the bot can also be dismissed.".into(),
        });
    }
    let cmd = dml_wow::party::party_uninvite_cmd(&bot)?;
    let logout_cmd = dml_wow::party::party_logout_whisper_cmd(&player, &bot)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
        party_fire_result(outcome, "kick")?;
        let dismissed = matches!(dml_wow::soap::exec(&soap_cfg, &logout_cmd), dml_wow::soap::SoapOutcome::Ok(_));
        Ok(serde_json::json!({"kicked": true, "dismissed": dismissed}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party dismiss-all` (`90-main.sh:3182-3234`): best-effort
/// uninvite+logout PER bot (one unreachable bot must not strand the rest of
/// the party); `dismissed` counts only successful uninvite fires — an
/// EVERY-fire failure (`attempted>0 && dismissed==0`) reports the LAST
/// failure's mapped error instead of a fabricated success (the 2026-07-22
/// review finding this arm's own comment documents).
#[tauri::command]
async fn wow_party_dismiss_all_native(
    player: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&player) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid player name: {player}"), hint: String::new() });
    }
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let db_cfg = dml_wow::db::DbConfig::from_env();
        let pguid = party_online_guid(&db_cfg, &player)
            .ok_or_else(|| party_not_online_err(&player, "Log the character into the game first."))?;
        let bots = bot_member_names(&db_cfg, pguid)?;
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let (mut dismissed, mut attempted) = (0i64, 0i64);
        let mut last_err: Option<CmdError> = None;
        let mut names = Vec::new();
        for b in bots {
            if !dml_wow::soap_cmds::valid_charname(&b) {
                continue;
            }
            attempted += 1;
            let outcome = {
                let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                dml_wow::soap::exec(&soap_cfg, &format!("dml_uninvite {b}"))
            };
            match dismiss_fire_result(outcome) {
                Ok(_) => {
                    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = dml_wow::soap::exec(&soap_cfg, &format!("dml_whisper {player} {b} logout"));
                    dismissed += 1;
                    names.push(b);
                }
                Err(e) => {
                    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                    let _ = dml_wow::soap::exec(&soap_cfg, &format!("dml_whisper {player} {b} logout"));
                    last_err = Some(e);
                }
            }
        }
        if attempted > 0 && dismissed == 0 {
            return Err(last_err.expect("attempted>0 && dismissed==0 implies at least one recorded failure"));
        }
        Ok(serde_json::json!({"dismissed": dismissed, "attempted": attempted, "bots": names}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party relogin` (`90-main.sh:3235-3247`).
#[tauri::command]
async fn wow_party_relogin_native(
    player: String,
    bot: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::party::party_relogin_cmd(&player, &bot)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
        party_fire_result(outcome, "relogin")?;
        Ok(serde_json::json!({"relogged": true}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party botcmd` (`90-main.sh:3249-3282`): validate player/bot
/// names, THEN build the fixed whisper tail (the `spec` action's own
/// non-empty + live-validity checks happen here, in that order, matching
/// the oracle), THEN require BOTH player and bot online (bot's own
/// NOT_FOUND hint differs from every other party arm's), THEN fire.
#[tauri::command]
async fn wow_party_botcmd_native(
    player: String,
    bot: String,
    action: String,
    spec: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&player) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid player name: {player}"), hint: String::new() });
    }
    if !dml_wow::soap_cmds::valid_charname(&bot) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid bot name: {bot}"), hint: String::new() });
    }
    let wmsg = if let Some(tail) = dml_wow::party::botcmd_fixed_tail(&action) {
        tail.to_string()
    } else if action == "spec" {
        let spec_val = spec.filter(|s| !s.is_empty()).ok_or_else(|| CmdError {
            code: "BAD_ARG".into(),
            message: "Action spec requires --spec <name>".into(),
            hint: "e.g. --spec 'frost pve'".into(),
        })?;
        let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();
        let live = live_spec_names(&title_dir);
        if !dml_wow::party::valid_bot_spec(&spec_val, live.as_deref()) {
            return Err(CmdError {
                code: "BAD_ARG".into(),
                message: format!("Unknown spec: {spec_val}"),
                hint: "A premade spec name like 'frost pve'.".into(),
            });
        }
        dml_wow::party::spec_action_wmsg(&spec_val)
    } else {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid action: {action}"),
            hint: "One of: gear talents maintain spec".into(),
        });
    };
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let db_cfg = dml_wow::db::DbConfig::from_env();
        if party_online_guid(&db_cfg, &player).is_none() {
            return Err(party_not_online_err(&player, "Log the character into the game first."));
        }
        if party_online_guid(&db_cfg, &bot).is_none() {
            return Err(party_not_online_err(&bot, "The bot must be in the world -- is it still in your party?"));
        }
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&soap_cfg, &format!("dml_whisper {player} {bot} {wmsg}"));
        party_fire_result(outcome, "botcmd")?;
        Ok(serde_json::json!({"sent": true, "player": player, "bot": bot, "action": action}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

fn preset_not_found(name: &str) -> CmdError {
    CmdError { code: "NOT_FOUND".into(), message: format!("No preset named {name}"), hint: String::new() }
}

fn io_internal_err(e: std::io::Error) -> CmdError {
    CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() }
}

/// NATIVE-MODE `party preset-save` (`90-main.sh:3283-3321`).
#[tauri::command]
async fn wow_party_preset_save_native(player: String, name: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&player) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid player name: {player}"), hint: String::new() });
    }
    if !dml_wow::party::valid_preset_name(&name) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid preset name: {name}"),
            hint: "Letters, digits, - and _ (max 32).".into(),
        });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let db_cfg = dml_wow::db::DbConfig::from_env();
        let pguid = party_online_guid(&db_cfg, &player)
            .ok_or_else(|| party_not_online_err(&player, "Log the character into the game first."))?;
        let names: Vec<String> = bot_member_classes(&db_cfg, pguid)?
            .into_iter()
            .filter_map(dml_wow::party_specs::class_name_from_id)
            .map(str::to_string)
            .collect();
        if names.is_empty() {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: "Party has no bots to save".into(),
                hint: "Add some bots first.".into(),
            });
        }
        let dir = preset_dir_or_internal_err()?;
        std::fs::create_dir_all(&dir).map_err(io_internal_err)?;
        let path = dml_wow::party::preset_path(&dir, &name);
        let overwrote = path.is_file();
        std::fs::write(&path, dml_wow::party::preset_file_content(&names)).map_err(io_internal_err)?;
        Ok(serde_json::json!({"saved": true, "name": name, "bots": names, "overwrote": overwrote}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party preset-list` (`90-main.sh:3322-3337`) — read-only, no
/// player/name argument to validate.
#[tauri::command]
async fn wow_party_preset_list_native() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        let dir = preset_dir_or_internal_err()?;
        let presets: Vec<serde_json::Value> = dml_wow::party::list_presets(&dir)
            .into_iter()
            .map(|p| serde_json::json!({"name": p.name, "bots": p.bots}))
            .collect();
        Ok(serde_json::json!({"presets": presets}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party preset-delete` (`90-main.sh:3339-3347`).
#[tauri::command]
async fn wow_party_preset_delete_native(name: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::party::valid_preset_name(&name) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid preset name: {name}"), hint: String::new() });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let dir = preset_dir_or_internal_err()?;
        let path = dml_wow::party::preset_path(&dir, &name);
        if !path.is_file() {
            return Err(preset_not_found(&name));
        }
        std::fs::remove_file(&path).map_err(io_internal_err)?;
        Ok(serde_json::json!({"deleted": true, "name": name}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party preset-show` (`90-main.sh:3436-3449`).
#[tauri::command]
async fn wow_party_preset_show_native(name: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::party::valid_preset_name(&name) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid preset name: {name}"), hint: String::new() });
    }
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let dir = preset_dir_or_internal_err()?;
        let path = dml_wow::party::preset_path(&dir, &name);
        if !path.is_file() {
            return Err(preset_not_found(&name));
        }
        let content = std::fs::read_to_string(&path).map_err(io_internal_err)?;
        let classes = dml_wow::party::parse_preset_classes(&content);
        Ok(serde_json::json!({"name": name, "classes": classes}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party preset-import` (`90-main.sh:3450-3483`): every token
/// validated (via [`dml_wow::party::parse_import_classes`]) BEFORE any
/// filesystem write, matching the oracle's abort-before-mutation contract.
#[tauri::command]
async fn wow_party_preset_import_native(
    name: String,
    classes: String,
    force: Option<bool>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::party::valid_preset_name(&name) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid preset name: {name}"),
            hint: "Letters, digits, - and _ (max 32).".into(),
        });
    }
    let parsed = dml_wow::party::parse_import_classes(&classes)?;
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let dir = preset_dir_or_internal_err()?;
        let path = dml_wow::party::preset_path(&dir, &name);
        if path.is_file() && !force {
            return Err(CmdError {
                code: "EXISTS".into(),
                message: format!("Preset already exists: {name}"),
                hint: "Pass --force to overwrite.".into(),
            });
        }
        std::fs::create_dir_all(&dir).map_err(io_internal_err)?;
        std::fs::write(&path, dml_wow::party::preset_file_content(&parsed)).map_err(io_internal_err)?;
        Ok(serde_json::json!({"imported": true, "name": name, "classes": parsed}))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `party preset-load` — see [`dml_wow::party::party_preset_load_stream`].
#[tauri::command]
async fn wow_party_preset_load_native(
    player: String,
    name: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&player) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid player name: {player}"), hint: String::new() });
    }
    if !dml_wow::party::valid_preset_name(&name) {
        return Err(CmdError { code: "BAD_ARG".into(), message: format!("Invalid preset name: {name}"), hint: String::new() });
    }
    let lock = state.soap_lock.clone();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::party::party_preset_load_stream(player, name, lock, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

/// NATIVE-MODE `.send items` mail (`90-main.sh:1785-1833`). `items` is a CSV
/// of `id:count` specs (see `split_mail_items`); `subject`/`body` default the
/// same as the CLI's own flag defaults.
#[tauri::command]
async fn wow_mail_item_native(
    to: String,
    items: String,
    subject: Option<String>,
    body: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let specs = split_mail_items(&items);
    let attachments = specs.len();
    let subject = subject.unwrap_or_else(|| "Dad's MMO Lab".into());
    let body = body.unwrap_or_else(|| "Enjoy!".into());
    let cmd = dml_wow::soap_cmds::mail_items_cmd(&to, &specs, &subject, &body)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        mail_result(outcome)?;
        Ok(serde_json::json!({ "sent": true, "to": to, "attachments": attachments }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `teleport name` (`90-main.sh:1852-1893`).
#[tauri::command]
async fn wow_teleport_native(
    char_name: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::teleport_name_cmd(&char_name, &to)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = dml_wow::soap::SoapConfig::load();
        let outcome = dml_wow::soap::exec(&cfg, &cmd);
        teleport_result(outcome)?;
        Ok(serde_json::json!({ "teleported": true, "char": char_name, "to": to }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `gm return-home` (`90-main.sh:3603-3654`, Task A2c). Sends a
/// character to its faction capital: ONLINE -> the same `.teleport name`
/// SOAP fire [`wow_teleport_native`] uses (capital name is a fixed literal,
/// never user input); OFFLINE -> a direct `characters`-table position
/// UPDATE, the FIRST db-write path in the native core (see [`dml_wow::db::execute`]).
/// Order matches the oracle exactly: validate -> lookup (guid/race/online)
/// -> faction-capital case match -> online branch.
#[tauri::command]
async fn wow_gm_return_home_native(
    player: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    if !dml_wow::soap_cmds::valid_charname(&player) {
        return Err(bad_arg(format!("Invalid player name: {player}")));
    }
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let cfg = dml_wow::db::DbConfig::from_env();
        let params: Vec<mysql::Value> = vec![mysql::Value::from(player.as_str())];
        let res = dml_wow::db::query_with_params(
            &cfg,
            dml_wow::db::Database::Characters,
            RETURN_HOME_SELECT_SQL,
            params,
        )
        .map_err(db_err_to_cmd)?;
        let row = res.rows.first().ok_or_else(|| CmdError {
            code: "NOT_FOUND".into(),
            message: format!("No such character: {player}"),
            hint: String::new(),
        })?;
        let guid = sql_row_int(row.first()).filter(|g| *g >= 0).ok_or_else(|| CmdError {
            code: "DB_UNREACHABLE".into(),
            message: "Unexpected character lookup result".into(),
            hint: String::new(),
        })?;
        let race_num = sql_row_int(row.get(1)).unwrap_or(-1);
        let online = sql_row_int(row.get(2)).unwrap_or(0);

        let race_u8 = u8::try_from(race_num).unwrap_or(u8::MAX);
        let capital = faction_capital(race_u8).ok_or_else(|| CmdError {
            code: "NOT_FOUND".into(),
            message: format!("Could not determine the faction of {player} (race {race_num})"),
            hint: String::new(),
        })?;

        if online == 1 {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            let soap_cfg = dml_wow::soap::SoapConfig::load();
            let cmd = format!("teleport name {player} {}", capital.name);
            let outcome = dml_wow::soap::exec(&soap_cfg, &cmd);
            return_home_online_result(outcome)?;
            Ok(serde_json::json!({
                "sent_home": true,
                "player": player,
                "capital": capital.name,
                "via": "teleport",
            }))
        } else {
            let guid = guid as u64;
            let update_params: Vec<mysql::Value> = vec![
                mysql::Value::from(capital.x),
                mysql::Value::from(capital.y),
                mysql::Value::from(capital.z),
                mysql::Value::from(capital.map),
                mysql::Value::from(guid),
            ];
            dml_wow::db::execute(
                &cfg,
                dml_wow::db::Database::Characters,
                RETURN_HOME_UPDATE_SQL,
                update_params,
            )
            .map_err(|_e| CmdError {
                code: "DB_UNREACHABLE".into(),
                message: "Could not update the character's position".into(),
                hint: "Is ac-database running?".into(),
            })?;
            Ok(serde_json::json!({
                "sent_home": true,
                "player": player,
                "capital": capital.name,
                "via": "db",
            }))
        }
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
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

/// NATIVE-MODE `wow ahbot repair` -- see
/// [`dml_wow::ahbot::ahbot_repair_stream`]. Native mode only — WSL keeps
/// calling `wow_ahbot_repair`.
#[tauri::command]
async fn wow_ahbot_repair_native(
    char_name: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    let soap_lock = state.soap_lock.clone();
    let config_lock = state.config_lock.clone();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::ahbot::ahbot_repair_stream(char_name, soap_lock, config_lock, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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

// ---------------------------------------------------------------------------
// NATIVE-MODE backup family (Chunk 2, task C2a): `backup create` (streamed),
// `list`/`validate`/`delete` (plain JSON). Faithful port of `90-main.sh:
// 3662-3785` + `60-backup.sh` via `dml::backup` -- direct `docker exec …
// mysqldump` + `flate2` gzip instead of shelling `dml`/`gzip`. `backup
// restore` stays WSL-only (out of scope for this task; see `60-backup.sh`'s
// header comment on why restore is the one sanctioned whole-DB-overwrite
// write path).
// ---------------------------------------------------------------------------

/// NATIVE-MODE fast `backup create`: same flow/messages/codes as `dml wow
/// backup create` (see [`dml_wow::backup::backup_create_stream`]'s doc comment),
/// via a direct `docker exec … mysqldump` + `flate2` gzip instead of
/// shelling `dml`/`gzip`. Native mode only — WSL keeps calling
/// `wow_backup_create`. `name` (backup display names) is native-only for the
/// same reason: the CLI has no `--name` flag, so the WSL sibling never
/// receives one (see `api.ts`'s `wowBackupCreate`).
#[tauri::command]
async fn wow_backup_create_native(
    include_world: Option<bool>,
    name: Option<String>,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    // No shared AppState needed (unlike world-restart's SOAP lock): backup
    // create only ever touches docker/fs, so this command takes no `State`.
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::backup::backup_create_stream(include_world.unwrap_or(false), name, db_cfg, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

/// NATIVE-MODE fast `backup list`: same shape as `wow_backup_list` plus a
/// `name` field (`{"backups":[{file,size,created,world,summary,name}]}` —
/// backup display names; `null` on a legacy sidecar, same as `summary` is
/// `null` on a missing one), a plain `std::fs` directory scan instead of
/// shelling `dml`. Native mode only.
#[tauri::command]
async fn wow_backup_list_native() -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(|| -> Result<serde_json::Value, CmdError> {
        let Some(bdir) = dml_wow::backup::backup_dir() else {
            return Err(CmdError { code: "INTERNAL".into(), message: "Could not resolve the backups directory".into(), hint: String::new() });
        };
        let backups: Vec<serde_json::Value> = dml_wow::backup::list_backups(&bdir)
            .into_iter()
            .map(|e| serde_json::json!({
                "file": e.file, "size": e.size, "created": e.created, "world": e.world, "summary": e.summary,
                "name": e.name,
            }))
            .collect();
        Ok(serde_json::json!({ "backups": backups }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Shared `--file` gate for `backup validate`/`delete`: [`dml_wow::backup::valid_backup_name`]
/// (`BAD_ARG`) then on-disk existence (`NOT_FOUND`) — a port of both arms'
/// identical two-step guard (`90-main.sh:3733,3735,3756,3758`).
fn require_backup_file(bdir: &std::path::Path, file: &str) -> Result<std::path::PathBuf, CmdError> {
    if !dml_wow::backup::valid_backup_name(file) {
        return Err(bad_arg(format!("Invalid backup name: {file}")));
    }
    let path = bdir.join(file);
    if !path.is_file() {
        return Err(CmdError { code: "NOT_FOUND".into(), message: format!("No backup named {file}"), hint: String::new() });
    }
    Ok(path)
}

/// NATIVE-MODE fast `backup validate`: same shape as `wow_backup_validate`
/// (`{valid,file,size,gzip_ok,sql_ok,markers,detail}`), a local gzip
/// decompress + marker scan instead of shelling `gzip -t`/`gunzip`/`grep`.
/// Native mode only.
#[tauri::command]
async fn wow_backup_validate_native(file: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let Some(bdir) = dml_wow::backup::backup_dir() else {
            return Err(CmdError { code: "INTERNAL".into(), message: "Could not resolve the backups directory".into(), hint: String::new() });
        };
        let path = require_backup_file(&bdir, &file)?;
        let result = dml_wow::backup::validate_backup(&path);
        Ok(dml_wow::backup::validate_result_json(&file, &result))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE fast `backup delete`: same shape as `wow_backup_delete`
/// (`{"deleted":true,"file"}`), a plain `std::fs::remove_file` instead of
/// shelling `dml`. Native mode only.
#[tauri::command]
async fn wow_backup_delete_native(file: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, CmdError> {
        let Some(bdir) = dml_wow::backup::backup_dir() else {
            return Err(CmdError { code: "INTERNAL".into(), message: "Could not resolve the backups directory".into(), hint: String::new() });
        };
        require_backup_file(&bdir, &file)?;
        dml_wow::backup::delete_backup(&bdir, &file);
        Ok(serde_json::json!({ "deleted": true, "file": file }))
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `backup restore` — see
/// [`dml_wow::restore::backup_restore_stream`]. No `--yes` exists on either
/// backend: the launcher's two-click confirm UI is the gate, unchanged from
/// `wow_backup_restore`. Native mode only.
#[tauri::command]
async fn wow_backup_restore_native(
    file: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    let soap_lock = state.soap_lock.clone();
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let db_cfg = dml_wow::db::DbConfig::from_env();
        dml_wow::restore::backup_restore_stream(file, soap_lock, db_cfg, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
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

// ---------------------------------------------------------------------------
// NATIVE-MODE `wow lan on/off/status/refresh` (Chunk 2 task C2c item 3):
// faithful port of the AzerothCore branch of `90-main.sh:858-1052`. AC-ONLY
// BY DECISION (`dml::db` has no MaNGOS/Tortoise support) -- native mode only
// ever drives the single fixed title `LAN_TITLE`; WSL keeps handling every
// title (including MaNGOS/Tortoise ones) via `wow_lan` above.
//
// TEXT-MODE, NOT JSON: like its WSL sibling, this returns `Result<String,
// CmdError>` where `CmdError` is reserved for genuinely malformed input
// (bad action, bad address SHAPE) -- the same split `wow_lan`'s own
// pre-validation already draws. Every DOMAIN-level failure (not a private
// address, server not running, DB not answering yet, the address didn't
// land) instead comes back as `Ok("[dml] ERROR: ...")` TEXT, because that's
// what `run_captured` already does for the WSL sibling: a `dml lan` that
// exits 1 with informational stdout is still `Ok` to the JS caller (see
// `DmlRunner::run_captured`'s doc comment) -- so the Svelte side, which just
// dumps this string into a `<pre>` (see `wowLan`'s doc comment in `api.ts`),
// needs no branching between backends.
// ---------------------------------------------------------------------------

/// Shared arg validation for `wow_lan_native`, deliberately duplicating (not
/// refactoring) `wow_lan`'s own inline checks above: SAME rules --
/// `LAN_ACTIONS` membership, an IPv4-SHAPE or hostname-SHAPE check depending
/// on `--internet`, `--internet` itself narrowed to `action == "on"` only
/// (mirrors `wow_lan`'s existing `internet.unwrap_or(false) && action ==
/// "on"` -- a deliberate product narrowing already shipped for WSL, not a
/// gap to "fix" here). The private-vs-public "not a private LAN address"
/// check is NOT here -- that stays a DOMAIN-level TEXT error further down
/// (`dml::lan::not_private_message`), matching where the bash oracle itself
/// performs it (`90-main.sh:901-905`, before ever touching docker/DB).
fn validate_lan_request_native(
    action: &str,
    ip: Option<String>,
    internet: bool,
) -> Result<(bool, Option<String>), CmdError> {
    if !LAN_ACTIONS.contains(&action) {
        return Err(bad_arg(format!("invalid lan action: {action:?}")));
    }
    let inet = internet && action == "on";
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
    Ok((inet, ip_arg))
}

/// NATIVE-MODE `wow lan on/off/status/refresh` -- see the module doc comment
/// above `validate_lan_request_native` for the text-vs-typed-error split.
/// Native mode only — WSL keeps calling `wow_lan`.
#[tauri::command]
async fn wow_lan_native(action: String, ip: Option<String>, internet: Option<bool>) -> Result<String, CmdError> {
    require_native_backend()?;
    let (inet, ip_arg) = validate_lan_request_native(&action, ip, internet.unwrap_or(false))?;
    tauri::async_runtime::spawn_blocking(move || dml_wow::lan::lan_action(&action, ip_arg, inet))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })
}

// --- Native Tailscale (spike/docker-desktop-native) -------------------------
//
// The WSL arm (`dml wow tailscale <action>`, 90-main.sh:5891+) does everything
// through `sudo -n pacman|systemctl|iptables` -- none of which exist on
// Windows, so in native mode that arm hits the real `sudo.exe` and dies on
// `-n` (the reported bug). There is no distro to sudo into on native mode:
// the free Windows Tailscale app installs `tailscale.exe` and does its own
// browser-based login with NO sudo, no daemon-enable step, no iptables --
// the app owns all of that itself. So native mode drives `tailscale.exe`
// directly and maps its output into the SAME JSON shapes the WSL arm returns
// (api.ts:1226-1245), so the frontend needs no branching.

/// Known absolute install locations for the Windows Tailscale CLI, tried
/// before falling back to a bare `tailscale.exe` resolved off PATH. Unlike
/// `dml::native::candidate_docker_paths` there is no LOCALAPPDATA candidate --
/// Tailscale's Windows installer is machine-wide, not per-user.
fn candidate_tailscale_paths() -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    if let Some(pf) = std::env::var_os("ProgramFiles") {
        out.push(std::path::PathBuf::from(&pf).join("Tailscale").join("tailscale.exe"));
    }
    if let Some(pf) = std::env::var_os("ProgramW6432") {
        out.push(std::path::PathBuf::from(&pf).join("Tailscale").join("tailscale.exe"));
    }
    out
}

/// Pure resolver: first candidate the predicate accepts, else `None`. No bare
/// -PATH fallback here (that needs a real process spawn) -- keeps this half
/// unit-testable with a fake candidate list and predicate, same shape as
/// `dml::native::resolve_docker_program`.
fn resolve_tailscale_from_candidates(
    candidates: &[std::path::PathBuf],
    exists: impl Fn(&std::path::Path) -> bool,
) -> Option<std::path::PathBuf> {
    candidates.iter().find(|c| exists(c)).cloned()
}

/// Windows Tailscale CLI. Default install is
/// `%ProgramFiles%\Tailscale\tailscale.exe`; falls back to `%ProgramW6432%`
/// (same binary, seen on some 32-on-64 shells), then a bare `tailscale.exe`
/// probed off PATH (`tailscale.exe version`, bounded, output discarded --
/// only the exit code matters). `None` when none of those pan out; the
/// caller treats that as "not installed" rather than guessing a path.
fn find_tailscale_exe() -> Option<std::path::PathBuf> {
    if let Some(p) = resolve_tailscale_from_candidates(&candidate_tailscale_paths(), |p| p.exists()) {
        return Some(p);
    }
    let ok = run_bounded(
        std::ffi::OsStr::new("tailscale.exe"),
        &["version"],
        std::time::Duration::from_secs(3),
    )
    .map(|(ok, _)| ok)
    .unwrap_or(false);
    if ok {
        return Some(std::path::PathBuf::from("tailscale.exe"));
    }
    None
}

/// Runs `program` with `args`, bounded by `timeout` wall-clock (see
/// `output_bounded` — a hung/unresponsive Tailscale daemon is killed at the
/// deadline, never abandoned). Returns the combined stdout+stderr (lossy
/// UTF-8) and whether the process exited 0; `None` on timeout or spawn failure.
fn run_bounded(program: &std::ffi::OsStr, args: &[&str], timeout: std::time::Duration) -> Option<(bool, String)> {
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = output_bounded(cmd, timeout)?;
    let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&out.stderr));
    Some((out.status.success(), combined))
}

/// The bits of `tailscale status --json` this command actually needs, pulled
/// out with `serde_json::Value` field lookups rather than a full typed
/// struct (the real payload has many more fields we don't care about).
#[derive(Debug, Default, PartialEq, Eq)]
struct TsStatusFields {
    backend_state: Option<String>,
    ip: Option<String>,
}

/// Pure JSON parse: `BackendState` -> backend_state; the first `100.x`
/// address in `TailscaleIPs` (checked at the top level, then under `Self` --
/// real payloads carry it in both places, and the brief that drove this
/// command only names `Self.TailscaleIPs`) -> ip. Any parse failure (bad
/// JSON, missing fields) reads as "unknown", never a panic.
fn parse_tailscale_status_json(raw: &str) -> TsStatusFields {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) else {
        return TsStatusFields::default();
    };
    let backend_state = v.get("BackendState").and_then(|x| x.as_str()).map(str::to_string);
    let find_ip = |arr: &serde_json::Value| -> Option<String> {
        arr.as_array()?
            .iter()
            .find_map(|e| e.as_str().filter(|s| s.starts_with("100.")))
            .map(str::to_string)
    };
    let ip = v
        .get("TailscaleIPs")
        .and_then(find_ip)
        .or_else(|| v.get("Self").and_then(|s| s.get("TailscaleIPs")).and_then(find_ip));
    TsStatusFields { backend_state, ip }
}

/// Pure: pulls the first-time Tailscale login URL out of `tailscale up`'s
/// combined stdout+stderr. Prefers a `login.tailscale.com` URL (what the
/// real CLI prints); falls back to the first bare `https://` URL so a
/// differently-worded CLI version still surfaces something clickable.
/// Mirrors 90-main.sh's two-stage `grep -oE`.
fn extract_tailscale_auth_url(text: &str) -> Option<String> {
    // The path charset the brief's regex uses AFTER the literal scheme
    // prefix: `[A-Za-z0-9./_-]`. The prefix itself carries `:` (`https://`),
    // which is why the scan only applies this filter to what comes after it
    // -- applying it to the whole match (prefix included) would cut the
    // string off right after "https" on the `:`.
    fn is_url_char(c: char) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '_' | '-')
    }
    fn scan(text: &str, prefix: &str) -> Option<String> {
        let start = text.find(prefix)?;
        let after_prefix = start + prefix.len();
        let rest = &text[after_prefix..];
        let end = rest.find(|c: char| !is_url_char(c)).unwrap_or(rest.len());
        Some(text[start..after_prefix + end].to_string())
    }
    scan(text, "https://login.tailscale.com/").or_else(|| scan(text, "https://"))
}

/// First `100.x` line in `tailscale ip -4`'s output. Mirrors the WSL arm's
/// `head -1` + `^100\.` filter.
fn first_tailnet_ip(text: &str) -> Option<String> {
    text.lines().map(str::trim).find(|l| l.starts_with("100.")).map(str::to_string)
}

/// Last `n` bytes of `s` (char-boundary safe), newlines flattened to spaces --
/// mirrors bash's `tail -c 400 | tr -d '\r' | tr '\n' ' '` so the error hint
/// reads the same on both backends.
fn tail_str(s: &str, n: usize) -> String {
    let mut start = s.len().saturating_sub(n);
    while start < s.len() && !s.is_char_boundary(start) {
        start += 1;
    }
    s[start..].split_whitespace().collect::<Vec<_>>().join(" ")
}

fn tailscale_not_installed_err() -> CmdError {
    CmdError {
        code: "NOT_INSTALLED".into(),
        message: "Tailscale isn't installed on Windows".into(),
        hint: "Install the free Windows app from tailscale.com/download, then click Refresh status.".into(),
    }
}

fn tailscale_install_native() -> Result<serde_json::Value, CmdError> {
    if find_tailscale_exe().is_some() {
        Ok(serde_json::json!({"installed": true, "already": true}))
    } else {
        Err(tailscale_not_installed_err())
    }
}

/// Read-only, so a missing/unreachable Tailscale reads as an answer, never
/// an error -- mirrors how the WSL server-info treats "down" as an answer.
fn tailscale_status_native() -> serde_json::Value {
    let Some(exe) = find_tailscale_exe() else {
        return serde_json::json!({
            "connected": false,
            "ip": null,
            "backend_state": "not-installed",
            "status_text": "Tailscale is not installed on Windows.",
        });
    };
    match run_bounded(exe.as_os_str(), &["status", "--json"], std::time::Duration::from_secs(5)) {
        Some((_ok, raw)) => {
            let fields = parse_tailscale_status_json(&raw);
            let connected = fields.backend_state.as_deref() == Some("Running") && fields.ip.is_some();
            let status_text = match (&fields.backend_state, &fields.ip) {
                (Some(bs), Some(ip)) => format!("{bs} \u{2014} {ip}"),
                (Some(bs), None) => bs.clone(),
                (None, _) => "Tailscale status unavailable.".to_string(),
            };
            serde_json::json!({
                "connected": connected,
                "ip": fields.ip,
                "backend_state": fields.backend_state,
                "status_text": status_text,
            })
        }
        None => serde_json::json!({
            "connected": false,
            "ip": null,
            "backend_state": "unreachable",
            "status_text": "Could not read Tailscale status (timed out).",
        }),
    }
}

fn tailscale_up_native() -> Result<serde_json::Value, CmdError> {
    let Some(exe) = find_tailscale_exe() else {
        return Err(tailscale_not_installed_err());
    };
    let (_ok, raw) = run_bounded(exe.as_os_str(), &["up", "--timeout=8s"], std::time::Duration::from_secs(15))
        .unwrap_or_else(|| (false, String::new()));
    let auth_url = extract_tailscale_auth_url(&raw);
    let ip = run_bounded(exe.as_os_str(), &["ip", "-4"], std::time::Duration::from_secs(5))
        .and_then(|(_, out)| first_tailnet_ip(&out));
    let connected = ip.is_some() && auth_url.is_none();
    if !connected && auth_url.is_none() {
        let tail = tail_str(&raw, 400);
        return Err(CmdError {
            code: "TAILSCALE_UP_FAILED".into(),
            message: "Could not start Tailscale login".into(),
            hint: if tail.is_empty() {
                "Is Tailscale running? Try Install, then Log in again.".into()
            } else {
                tail
            },
        });
    }
    Ok(serde_json::json!({
        "connected": connected,
        "auth_url": auth_url,
        "ip": ip,
        "daemon": "windows-app",
        "firewall": "n/a",
    }))
}

/// Best-effort, always `{"down":true}`: down is idempotent from the user's
/// POV (already-down, not-installed, and a real `tailscale.exe down` failure
/// all leave the same "not connected" end state), and there is nothing
/// actionable left to offer beyond "log in again" from Up.
fn tailscale_down_native() -> serde_json::Value {
    if let Some(exe) = find_tailscale_exe() {
        let _ = run_bounded(exe.as_os_str(), &["down"], std::time::Duration::from_secs(8));
    }
    serde_json::json!({"down": true})
}

/// The one `*_blocking` body deliberately LEFT in the launcher by the
/// cargo-workspace refactor (Task 9), which hoisted every other one into
/// `dml-wow`. This whole Tailscale cluster is Windows-HOST desktop-app
/// plumbing — locating and driving the Tailscale app's `tailscale.exe`,
/// running its MSI installer, parsing `tailscale status --json` — with no
/// WoW/AzerothCore domain content at all, so `dml-wow` ("the WoW game
/// library") is the wrong home for it and `dml-core` has no host-networking
/// module to put it in. No planned `dml-wow-cli` subcommand consumes it
/// either. It is also not orchestration: it is a six-line dispatcher over
/// the four `tailscale_*_native` helpers directly above.
fn native_tailscale_blocking(action: &str) -> Result<serde_json::Value, CmdError> {
    match action {
        "install" => tailscale_install_native(),
        "status" => Ok(tailscale_status_native()),
        "up" => tailscale_up_native(),
        "down" => Ok(tailscale_down_native()),
        // Unreachable: `wow_tailscale` checks TAILSCALE_ACTIONS before calling in.
        other => Err(bad_arg(format!("invalid tailscale action: {other:?}"))),
    }
}

async fn native_tailscale(action: &str) -> Result<serde_json::Value, CmdError> {
    let action = action.to_string();
    tauri::async_runtime::spawn_blocking(move || native_tailscale_blocking(&action))
        .await
        .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Batch 5 (overnight): Tailscale "Play Together" -- `wow tailscale
/// install|up|status|down`. The action arrives from the webview, so it is
/// checked against a closed allowlist before it becomes an argv token (same
/// doctrine as wow_lan). WSL mode is unchanged -- each arm is a captured JSON
/// envelope from `dml`, `up` bounded by a `--timeout` CLI-side so this never
/// hangs waiting on the browser login. Native mode (spike/docker-desktop-
/// native) has no distro to shell into -- `dml wow tailscale` sudo's into
/// pacman/systemd/iptables, none of which exist on Windows -- so it drives
/// the Windows Tailscale app's `tailscale.exe` directly instead, mapping its
/// output into the same JSON shapes.
#[tauri::command]
async fn wow_tailscale(action: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    if !TAILSCALE_ACTIONS.contains(&action.as_str()) {
        return Err(bad_arg(format!("invalid tailscale action: {action:?}")));
    }
    if is_native_backend() {
        return native_tailscale(&action).await;
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
    use dml_wow::runner::{DISTRO, USER};
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
                    let text = dml_wow::envelope::decode_wsl_output(&buf[..n]);
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
                    let text = dml_wow::envelope::decode_wsl_output(&buf[..n]);
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
    use dml_wow::runner::DISTRO;
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
    use dml_wow::runner::DISTRO;
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
    dml_wow::backend::selected() == dml_wow::backend::Backend::Native
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
            let text = dml_wow::envelope::decode_wsl_output(&o.stdout);
            text.lines().any(|l| l.trim() == distro)
        }
        _ => false,
    }
}

/// Read-only aggregate the Native-setup card loads on mount: which backend is
/// active plus the pass/fail of each native-mode prerequisite. Never mutates.
#[tauri::command]
fn native_setup_status() -> Result<serde_json::Value, CmdError> {
    let docker_prog = dml_wow::native::docker_program();
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
    let distro_available = wsl_distro_present(dml_wow::runner::DISTRO);

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
    let exe = dml_wow::native::docker_desktop_program().ok_or_else(|| {
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
            dml_wow::runner::DISTRO,
            "-u",
            dml_wow::runner::USER,
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
            let err = dml_wow::envelope::decode_wsl_output(&out.stderr);
            return Err(bad_arg(format!(
                "could not read soap.env from the {} distro: {}",
                dml_wow::runner::DISTRO,
                err.trim()
            )));
        }
        let cleaned = crate::nativesetup::strip_cr(&dml_wow::envelope::decode_wsl_output(
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
    let prog = dml_wow::native::docker_program();
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
                    let text = dml_wow::envelope::decode_wsl_output(&buf[..n]);
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

/// NATIVE-MODE `games remove` — see
/// [`dml_wow::destructive::games_remove_stream`]. Native mode
/// only — WSL keeps calling `games_remove` (the sibling above). `confirm` is
/// ALWAYS `true` here (the typed-id UI is the gate, matching the WSL
/// sibling's hardcoded `--yes`) — no parameter exposes it, so a stray caller
/// can never accidentally skip the confirmation UI ever existed for.
#[tauri::command]
async fn games_remove_native(
    id: String,
    keep_data: Option<bool>,
    remove_images: Option<bool>,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::destructive::games_remove_stream(id, keep_data.unwrap_or(false), remove_images.unwrap_or(false), true, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Native-mode Docker Desktop engine lifecycle around start/stop.
//
// In native mode the Docker Desktop engine (and its docker-desktop WSL VM) must
// be up before any `docker compose` runs, so `games_start` ensures it first;
// and stopping it on `games_stop` frees the VM's RAM, so `games_stop` shuts it
// down afterwards when the (default-on) `nativeManageDocker` toggle is set. WSL
// mode does neither — it is byte-for-byte unchanged. The decision/poll logic
// AND the blocking spawn/sleep/stream orchestration both live in
// `dml_wow::native` ([`dml_wow::native::ensure_engine_up_stream`] /
// [`dml_wow::native::stop_engine_stream`]); these two are only the async
// `spawn_blocking` adapters.
// ---------------------------------------------------------------------------

/// Async wrapper: ensure the engine is up before a native start. Aborts (Err)
/// when it cannot be brought up.
async fn ensure_engine_up(on_event: &Channel<serde_json::Value>) -> Result<(), CmdError> {
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::native::ensure_engine_up_stream(|v| { let _ = ch.send(v); })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// Async wrapper: best-effort stop of the Docker Desktop engine after a native
/// server-stop. Never returns an error — the server-stop result stands.
async fn stop_engine_best_effort(on_event: &Channel<serde_json::Value>) {
    let ch = on_event.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || {
        dml_wow::native::stop_engine_stream(|v| { let _ = ch.send(v); })
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
        // Chunk 3b: native mode replaces the inner `dml` shell-out with
        // direct compose orchestration (see `games_lifecycle_stream`
        // below) -- the engine-ensure-up wrapping just above is unchanged.
        return run_games_lifecycle_native("start", id, false, on_event).await;
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
    let stop_docker = dml_wow::native::stop_engine_enabled(is_native_backend(), manage_docker);
    // Stop the server exactly as today (clone the channel so we can keep
    // streaming the engine-stop afterwards). Chunk 3b: native mode replaces
    // the inner `dml` shell-out with direct compose orchestration -- the
    // engine-stop wrapping below is unchanged either way.
    let result = if is_native_backend() {
        run_games_lifecycle_native("stop", id, false, on_event.clone()).await
    } else {
        stream_action("stop", id, on_event.clone(), state).await
    };
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
    let skip = skip_saveall.unwrap_or(false);
    // Chunk 3b: native mode replaces the inner `dml` shell-out with direct
    // compose orchestration. No engine-lifecycle wrapping here (matches
    // today's WSL sibling: a restart assumes the server -- and so the
    // engine -- is already up; only cold `start` brings Docker Desktop up).
    if is_native_backend() {
        return run_games_lifecycle_native("restart", id, skip, on_event).await;
    }
    // --no-saveall = the GUI's "faster restart" option (skip the redundant
    // pre-stop saveall; the graceful stop still saves on shutdown).
    let mut args: Vec<String> = vec!["games".into(), "restart".into(), id];
    if skip {
        args.push("--no-saveall".into());
    }
    stream_args(args, on_event, state).await
}

/// Async wrapper shared by `games_start`/`games_stop`/`games_restart`'s
/// native branch: spawns [`dml_wow::lifecycle::games_lifecycle_stream`] and
/// joins it. NOT gated on `require_native_backend()` — unlike
/// `wow_world_restart_native` these aren't `_native`-suffixed siblings; the
/// THREE CALLERS (`games_start`/`games_stop`/`games_restart` above) already
/// branch on `is_native_backend()` themselves (that's where the Docker-
/// Desktop-engine wrapping lives), so this only ever runs in native mode.
/// Domain failures already traveled in the event stream, so this resolves
/// `Ok(())` unless the blocking task itself panicked.
async fn run_games_lifecycle_native(
    mode: &'static str,
    id: String,
    skip_saveall: bool,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CmdError> {
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::lifecycle::games_lifecycle_stream(mode, id, skip_saveall, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

/// NATIVE-MODE `wow update` (server self-update) — see
/// [`dml_wow::maint::update_stream`]. A brand new `_native` sibling (unlike
/// `games_start`/`stop`/`restart` above, WSL's `wow_server_update` has no
/// engine-lifecycle wrapping to preserve), so `require_native_backend()`
/// gates it, matching every other `_native` command. Native mode only -- WSL
/// keeps calling `wow_server_update` (the `dml`-shelling sibling above).
/// `backup: None` reaches the arm's own `BAD_ARG` gate (the frontend always
/// passes an explicit `true`/`false`, matching WSL's plain-`bool` sibling —
/// `None` only reaches a future/alternate caller that omits the choice).
#[tauri::command]
async fn wow_update_native(backup: Option<bool>, on_event: Channel<serde_json::Value>) -> Result<(), CmdError> {
    require_native_backend()?;
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::maint::update_stream(backup, |v| {
            let _ = ch.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState {
            // Backend switch (spike/docker-desktop-native): default WSL, or the
            // native Docker-Desktop path when DML_BACKEND=native. Same dml brain
            // either way — native just hosts it on Windows against Docker Desktop.
            runner: std::sync::Arc::new(DmlRunner::for_backend(
                dml_wow::backend::selected(),
            )),
            install: Arc::new(Mutex::new(None)),
            auto_shutdown: Arc::new(Mutex::new(AutoShutdownCtl { generation: 0, enabled: false })),
            soap_lock: Arc::new(Mutex::new(())),
            config_lock: Arc::new(Mutex::new(())),
        })
        .setup(|_app| {
            // (Task 8 removed the startup registry prefetch here: the config/
            // tuning/module-catalog registries are now embedded in dml-wow —
            // see `dml_wow::registry` — so there is nothing left to warm.)
            if dml_wow::backend::selected() == dml_wow::backend::Backend::Native {
                // Automatic "Auto (6h)" interval backup watcher (see the
                // `interval_backup_watcher` doc comment above): started once,
                // runs for the app's whole lifetime, no UI toggle. Seed
                // `last_run` from whatever's already on disk so a relaunch
                // doesn't restart the 6h clock at zero.
                let last_run = Arc::new(Mutex::new(
                    dml_wow::backup::backup_dir()
                        .and_then(|d| dml_wow::backup::latest_auto_interval_backup_unix(&d)),
                ));
                std::thread::spawn(move || interval_backup_watcher(last_run));
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
            games_remove_native,
            games_start,
            games_stop,
            games_restart,
            zam_probe,
            zam_cache_status,
            zam_cache_clear,
            wow_cache_status,
            wow_cache_clean,
            wow_cache_clean_native,
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
            wow_docker_clean_native,
            wow_update_check,
            wow_server_update,
            wow_update_native,
            wow_console_tail,
            wow_console_send,
            wow_module_list,
            wow_commands,
            wow_module_install,
            wow_module_install_native,
            wow_module_remove,
            wow_module_remove_native,
            wow_module_rebuild,
            wow_module_rebuild_native,
            wow_module_update_check,
            wow_module_update_check_native,
            wow_module_update,
            wow_module_update_native,
            wow_module_conf_activate,
            wow_module_conf_activate_native,
            wow_module_client_patch,
            wow_module_client_patch_native,
            wow_module_tracking,
            wow_module_tracking_native,
            wow_module_repair,
            wow_module_repair_native,
            wow_module_fixit,
            wow_module_fixit_native,
            wow_module_place_npc,
            wow_module_place_npc_native,
            wow_client_path_get,
            wow_client_path_set,
            wow_client_path_set_native,
            wow_client_path_detect,
            wow_items_search,
            wow_mail_item,
            wow_teleport_list,
            wow_teleport,
            wow_teleport_coords,
            wow_teleport_coords_native,
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
            wow_players_online_read,
            wow_party_online_read,
            wow_items_search_read,
            wow_char_progress_read,
            wow_achievements_read,
            wow_server_info_read,
            wow_server_detail_read,
            wow_console_tail_read,
            wow_docker_usage_read,
            wow_port_check_read,
            wow_update_check_read,
            wow_commands_read,
            wow_party_specs_read,
            wow_client_path_read,
            wow_client_path_detect_read,
            wow_cache_status_read,
            wow_lan_public_ip_read,
            wow_item_info_read,
            wow_entity_info_read,
            backend_mode,
            wow_config_set,
            wow_config_set_native,
            wow_config_tuning_list,
            wow_config_tuning_set,
            wow_config_tuning_set_native,
            wow_config_conf_keys,
            wow_config_conf_keys_native,
            wow_config_raw_read,
            wow_config_raw_read_native,
            wow_config_pb_keys,
            wow_config_pb_keys_native,
            wow_config_files,
            wow_config_raw_reset,
            wow_config_raw_reset_native,
            wow_config_raw_write,
            wow_config_raw_write_native,
            wow_accountwide_get,
            wow_accountwide_set,
            wow_accountwide_get_native,
            wow_accountwide_set_native,
            wow_bots_flush,
            wow_bots_flush_native,
            wow_ahbot_repair,
            wow_ahbot_repair_native,
            wow_party_setup,
            wow_party_online,
            wow_party_specs,
            wow_players_online,
            wow_bots_list,
            wow_world_restart,
            wow_world_restart_native,
            wow_party_add,
            wow_party_add_native,
            wow_party_list,
            wow_party_kick,
            wow_party_kick_native,
            wow_party_dismiss_all,
            wow_party_dismiss_all_native,
            wow_party_relogin,
            wow_party_relogin_native,
            wow_party_botcmd,
            wow_party_botcmd_native,
            wow_party_preset_save,
            wow_party_preset_save_native,
            wow_party_preset_list,
            wow_party_preset_list_native,
            wow_party_preset_delete,
            wow_party_preset_delete_native,
            wow_party_preset_load,
            wow_party_preset_load_native,
            wow_party_preset_show,
            wow_party_preset_show_native,
            wow_party_preset_import,
            wow_party_preset_import_native,
            wow_backup_create,
            wow_backup_list,
            wow_backup_delete,
            wow_backup_validate,
            wow_backup_restore,
            wow_backup_create_native,
            wow_backup_list_native,
            wow_backup_validate_native,
            wow_backup_delete_native,
            wow_backup_restore_native,
            wow_bridge_setup,
            wow_bridge_setup_native,
            wow_gm_level,
            wow_gm_gold,
            wow_gm_heal,
            wow_gm_revive,
            wow_gm_summon,
            wow_gm_at_login,
            wow_gm_return_home,
            wow_gm_return_home_native,
            wow_console_send_native,
            wow_account_create_native,
            wow_account_set_password_native,
            wow_account_set_gm_native,
            wow_account_delete_native,
            wow_gm_level_native,
            wow_gm_at_login_native,
            wow_gm_gold_native,
            wow_gm_heal_native,
            wow_gm_revive_native,
            wow_gm_summon_native,
            wow_mail_item_native,
            wow_teleport_native,
            wow_lan,
            wow_lan_native,
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

    // -- validate_lan_request_native (Chunk 2 task C2c item 3) ---------------

    #[test]
    fn validate_lan_request_native_rejects_unknown_action() {
        let e = validate_lan_request_native("reset", None, false).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
    }

    #[test]
    fn validate_lan_request_native_requires_ip_for_on_and_refresh() {
        assert_eq!(validate_lan_request_native("on", None, false).unwrap_err().code, "BAD_ARG");
        assert_eq!(validate_lan_request_native("refresh", None, false).unwrap_err().code, "BAD_ARG");
    }

    #[test]
    fn validate_lan_request_native_off_and_status_need_no_ip() {
        let (inet, ip) = validate_lan_request_native("off", None, false).unwrap();
        assert!(!inet);
        assert_eq!(ip, None);
        let (inet, ip) = validate_lan_request_native("status", None, true).unwrap();
        assert!(!inet); // internet is narrowed to action=="on" only
        assert_eq!(ip, None);
    }

    #[test]
    fn validate_lan_request_native_shape_checks_ip_when_not_internet() {
        assert_eq!(
            validate_lan_request_native("on", Some("not-an-ip".into()), false).unwrap_err().code,
            "BAD_ARG"
        );
        let (inet, ip) = validate_lan_request_native("on", Some("192.168.1.5".into()), false).unwrap();
        assert!(!inet);
        assert_eq!(ip.as_deref(), Some("192.168.1.5"));
    }

    #[test]
    fn validate_lan_request_native_internet_only_narrows_for_on() {
        // --internet is only honored for action=="on" (matches wow_lan's own
        // narrowing) -- refresh with internet=true still gets the strict
        // IPv4-shape check, not the loose hostname one.
        let (inet, ip) = validate_lan_request_native("on", Some("myserver.duckdns.org".into()), true).unwrap();
        assert!(inet);
        assert_eq!(ip.as_deref(), Some("myserver.duckdns.org"));
        assert_eq!(
            validate_lan_request_native("refresh", Some("myserver.duckdns.org".into()), true).unwrap_err().code,
            "BAD_ARG"
        );
    }

    #[test]
    fn validate_lan_request_native_internet_shape_checks_hostname() {
        assert_eq!(
            validate_lan_request_native("on", Some("bad host;".into()), true).unwrap_err().code,
            "BAD_ARG"
        );
    }

    // -- lan_current_address / lan_set shape (Chunk 2 task C2c item 3) -------

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

    // --- Native Tailscale (spike/docker-desktop-native) --------------------

    #[test]
    fn find_tailscale_exe_candidate_resolver_returns_none_when_nothing_exists() {
        // Pure half only -- `find_tailscale_exe()` itself also probes a bare
        // `tailscale.exe` on PATH, which is environment-dependent (this dev
        // box genuinely has Tailscale installed at the default path), so the
        // "absent" case is proven against the injectable resolver instead,
        // same doctrine as `dml::native`'s `resolve_docker_program` tests.
        let cands = vec![
            std::path::PathBuf::from(r"C:\missing\Tailscale\tailscale.exe"),
            std::path::PathBuf::from(r"C:\also-missing\Tailscale\tailscale.exe"),
        ];
        let got = resolve_tailscale_from_candidates(&cands, |_| false);
        assert_eq!(got, None);
    }

    #[test]
    fn find_tailscale_exe_candidate_resolver_picks_first_existing() {
        let cands = vec![
            std::path::PathBuf::from(r"C:\missing\Tailscale\tailscale.exe"),
            std::path::PathBuf::from(r"C:\present\Tailscale\tailscale.exe"),
        ];
        let got = resolve_tailscale_from_candidates(&cands, |p| {
            p == std::path::Path::new(r"C:\present\Tailscale\tailscale.exe")
        });
        assert_eq!(got, Some(std::path::PathBuf::from(r"C:\present\Tailscale\tailscale.exe")));
    }

    #[test]
    fn parses_tailscale_status_json_running_and_connected() {
        let raw = r#"{
            "Version": "1.66.0",
            "BackendState": "Running",
            "TailscaleIPs": ["100.101.102.103", "fd7a:115c:a1e0::1"],
            "Self": {"TailscaleIPs": ["100.101.102.103"]}
        }"#;
        let fields = parse_tailscale_status_json(raw);
        assert_eq!(fields.backend_state.as_deref(), Some("Running"));
        assert_eq!(fields.ip.as_deref(), Some("100.101.102.103"));
    }

    #[test]
    fn parses_tailscale_status_json_falls_back_to_self_ips() {
        // Some payload shapes carry TailscaleIPs only under Self.
        let raw = r#"{"BackendState":"Running","Self":{"TailscaleIPs":["100.64.0.5"]}}"#;
        let fields = parse_tailscale_status_json(raw);
        assert_eq!(fields.ip.as_deref(), Some("100.64.0.5"));
    }

    #[test]
    fn parses_tailscale_status_json_needs_login_has_no_ip() {
        let raw = r#"{"BackendState":"NeedsLogin","TailscaleIPs":[]}"#;
        let fields = parse_tailscale_status_json(raw);
        assert_eq!(fields.backend_state.as_deref(), Some("NeedsLogin"));
        assert_eq!(fields.ip, None);
    }

    #[test]
    fn parses_tailscale_status_json_garbage_input_is_unknown_not_a_panic() {
        let fields = parse_tailscale_status_json("not json at all");
        assert_eq!(fields, TsStatusFields::default());
    }

    #[test]
    fn tailscale_status_connected_requires_both_running_and_an_ip() {
        // Mirrors the brief's `connected = backend_state=="Running" &&
        // ip.is_some()` -- Running with no IP yet (mid-login) must NOT read
        // as connected.
        let running_no_ip = TsStatusFields { backend_state: Some("Running".into()), ip: None };
        let connected = running_no_ip.backend_state.as_deref() == Some("Running") && running_no_ip.ip.is_some();
        assert!(!connected);

        let running_with_ip =
            TsStatusFields { backend_state: Some("Running".into()), ip: Some("100.1.2.3".into()) };
        let connected = running_with_ip.backend_state.as_deref() == Some("Running") && running_with_ip.ip.is_some();
        assert!(connected);
    }

    #[test]
    fn extracts_auth_url_from_tailscale_up_output() {
        let out = "\nTo authenticate, visit:\n\n\thttps://login.tailscale.com/a/abc123\n\n";
        assert_eq!(
            extract_tailscale_auth_url(out).as_deref(),
            Some("https://login.tailscale.com/a/abc123")
        );
    }

    #[test]
    fn extracts_auth_url_falls_back_to_bare_https() {
        let out = "Please visit: https://example.com/some/path?query=dropped and finish there";
        // The query string isn't URL_CHARS-safe, so it stops before `?` --
        // matches the bash grep -oE's own charset.
        assert_eq!(extract_tailscale_auth_url(out).as_deref(), Some("https://example.com/some/path"));
    }

    #[test]
    fn extracts_auth_url_none_when_already_authenticated() {
        let out = "Success.\n";
        assert_eq!(extract_tailscale_auth_url(out), None);
    }

    #[test]
    fn first_tailnet_ip_picks_the_100_address() {
        assert_eq!(first_tailnet_ip("100.101.102.103\n"), Some("100.101.102.103".to_string()));
        assert_eq!(first_tailnet_ip("  100.64.0.1  \nfd7a::1\n"), Some("100.64.0.1".to_string()));
        assert_eq!(first_tailnet_ip("fd7a::1\n"), None);
        assert_eq!(first_tailnet_ip(""), None);
    }

    #[test]
    fn tail_str_flattens_and_bounds_output() {
        let long = "a".repeat(500);
        let got = tail_str(&long, 400);
        assert_eq!(got.len(), 400);

        let multiline = "line one\r\nline two\r\nline three";
        assert_eq!(tail_str(multiline, 4000), "line one line two line three");
    }

    #[test]
    fn db_err_to_cmd_collapses_every_variant_to_db_unreachable() {
        // Finding #5: the bash arms these commands mirror can only ever emit
        // DB_UNREACHABLE (90-main.sh has no separate "connected but the query
        // failed" code for teleport-list/bots/accounts/paperdoll), so a native
        // DbError::Query must collapse to the same code, not surface
        // DB_QUERY_FAILED and diverge from the CLI.
        use dml_wow::db::DbError;
        assert_eq!(db_err_to_cmd(DbError::Unreachable("down".into())).code, "DB_UNREACHABLE");
        assert_eq!(db_err_to_cmd(DbError::Query("bad sql".into())).code, "DB_UNREACHABLE");
    }

    #[test]
    fn stats_err_to_cmd_collapses_every_variant_to_db_unreachable() {
        use dml_wow::db::DbError;
        assert_eq!(stats_err_to_cmd(DbError::Unreachable("down".into())).code, "DB_UNREACHABLE");
        assert_eq!(stats_err_to_cmd(DbError::Query("bad sql".into())).code, "DB_UNREACHABLE");
    }

    // -- Task A2b: native SOAP command fault-mapping helpers -----------------
    // Network-touching command bodies aren't unit-testable (no live server --
    // see the task brief), so these tests cover the pure `SoapOutcome ->
    // CmdError` mappers + the mail CSV-split helper, which carry all the
    // per-arm parity logic.

    use dml_wow::soap::SoapOutcome;

    #[test]
    fn party_fire_result_ok_passes_through() {
        assert_eq!(party_fire_result(SoapOutcome::Ok("done".into()), "gold").unwrap(), "done");
    }

    #[test]
    fn party_fire_result_fault_uses_fixed_label_message_not_server_text() {
        let e = party_fire_result(SoapOutcome::Fault("server said no".into()), "gold").unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "The gold command was rejected");
        assert_eq!(
            e.hint,
            "Deploy the server bridges (bridge-setup) and restart the server first."
        );
    }

    #[test]
    fn party_fire_result_fault_label_varies_per_caller() {
        for label in ["gold", "heal", "revive", "summon"] {
            let e = party_fire_result(SoapOutcome::Fault("x".into()), label).unwrap_err();
            assert_eq!(e.message, format!("The {label} command was rejected"));
        }
    }

    #[test]
    fn party_fire_result_auth_uses_shorter_message() {
        let e = party_fire_result(SoapOutcome::Auth, "gold").unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP auth failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");
    }

    #[test]
    fn party_fire_result_unreachable() {
        let e = party_fire_result(SoapOutcome::Unreachable("boom".into()), "gold").unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(e.hint, "Is it running?");
    }

    #[test]
    fn gm_level_result_ok_passes_through() {
        assert_eq!(gm_level_result(SoapOutcome::Ok("ok".into())).unwrap(), "ok");
    }

    #[test]
    fn gm_level_result_fault_is_fixed_message_ignoring_server_text() {
        let e = gm_level_result(SoapOutcome::Fault("whatever the server said".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "The level command was rejected");
        assert_eq!(e.hint, "Does the character exist? The server said no.");
    }

    #[test]
    fn gm_level_result_auth_uses_shorter_message() {
        let e = gm_level_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP auth failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");
    }

    #[test]
    fn gm_level_result_unreachable() {
        let e = gm_level_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(e.hint, "Is it running?");
    }

    #[test]
    fn gm_at_login_result_fault_decodes_server_text() {
        let e = gm_at_login_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "a<b");
        assert_eq!(e.hint, "The worldserver rejected the command.");
    }

    #[test]
    fn gm_at_login_result_auth_uses_shorter_message() {
        let e = gm_at_login_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP auth failed");
    }

    #[test]
    fn mail_result_fault_is_raw_not_decoded() {
        let e = mail_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "a&lt;b");
        assert_eq!(e.hint, "The server rejected the mail command.");
    }

    #[test]
    fn mail_result_auth_has_empty_hint() {
        let e = mail_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "");
    }

    #[test]
    fn mail_result_unreachable_hint() {
        let e = mail_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(e.hint, "Run: dml wow soap-setup, then start the server.");
    }

    #[test]
    fn teleport_result_fault_is_raw_not_decoded() {
        let e = teleport_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.message, "a&lt;b");
        assert_eq!(e.hint, "Unknown location? See dml wow teleport-list.");
    }

    #[test]
    fn teleport_result_auth_and_unreachable_have_empty_hints() {
        let e = teleport_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "");
        let e = teleport_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(e.hint, "");
    }

    // -- Subsystem-A review fixes: account_result / console_send_result -----

    #[test]
    fn account_result_ok_passes_through() {
        assert_eq!(
            account_result(SoapOutcome::Ok("Account created.".into()), "http://x/").unwrap(),
            "Account created."
        );
    }

    #[test]
    fn account_result_fault_matches_decoded_oracle() {
        let e = account_result(SoapOutcome::Fault("a&lt;b".into()), "http://x/").unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "a<b");
        assert_eq!(e.hint, "The worldserver rejected the command.");
    }

    #[test]
    fn account_result_auth_matches_decoded_oracle() {
        let e = account_result(SoapOutcome::Auth, "http://x/").unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");
    }

    #[test]
    fn account_result_unreachable_uses_the_account_arms_own_wording() {
        // 90-main.sh:2009 -- the account arm's catch-all `*)` branch, NOT the
        // generic outcome_to_result_decoded's Unreachable text.
        let e = account_result(SoapOutcome::Unreachable("boom".into()), "http://127.0.0.1:7878/")
            .unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach SOAP at http://127.0.0.1:7878/");
        assert_eq!(e.hint, "Is the worldserver running?");
    }

    #[test]
    fn console_send_result_ok_is_entity_decoded() {
        // 90-main.sh:1741 -- console-send decodes BOTH the Ok and Fault text,
        // unlike the generic outcome_to_result_raw the soap-exec arm uses.
        assert_eq!(
            console_send_result(SoapOutcome::Ok("a&lt;b".into()), "http://x/").unwrap(),
            "a<b"
        );
    }

    #[test]
    fn console_send_result_fault_is_entity_decoded() {
        let e = console_send_result(SoapOutcome::Fault("a&lt;b".into()), "http://x/").unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "a<b");
        assert_eq!(e.hint, "The worldserver rejected the command.");
    }

    #[test]
    fn console_send_result_auth_matches_console_send_arm() {
        let e = console_send_result(SoapOutcome::Auth, "http://x/").unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");
    }

    #[test]
    fn console_send_result_unreachable_matches_console_send_arm_not_soap_exec() {
        // 90-main.sh:1745 -- console-send's own Unreachable wording (mentions
        // soap-setup), distinct from both the generic mapper and soap-exec's
        // arm (which has the same text here, but a different Auth hint).
        let e = console_send_result(SoapOutcome::Unreachable("boom".into()), "http://127.0.0.1:7878/")
            .unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach SOAP at http://127.0.0.1:7878/");
        assert_eq!(
            e.hint,
            "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup"
        );
    }

    #[test]
    fn not_online_err_matches_oracle_text() {
        let e = not_online_err("Testen");
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "Character not online: Testen");
        assert_eq!(
            e.hint,
            "This action needs the character logged in. (Set level works offline.)"
        );
    }

    #[test]
    fn split_mail_items_empty_string_is_zero_fields() {
        // Rust's "".split(',') yields one empty field; bash's `IFS=','
        // read -ra` on empty input yields zero. Must special-case to match.
        let got: Vec<&str> = split_mail_items("");
        assert!(got.is_empty());
    }

    #[test]
    fn split_mail_items_splits_on_commas() {
        assert_eq!(split_mail_items("6948:1,2589:5"), vec!["6948:1", "2589:5"]);
    }

    #[test]
    fn split_mail_items_single_spec_no_comma() {
        assert_eq!(split_mail_items("6948:1"), vec!["6948:1"]);
    }

    // --- Task A2c: gm return-home ---------------------------------------

    #[test]
    fn faction_capital_alliance_races_map_to_stormwind() {
        for race in [1u8, 3, 4, 7, 11] {
            let cap = faction_capital(race).unwrap_or_else(|| panic!("race {race} should map"));
            assert_eq!(cap.name, "Stormwind");
            assert_eq!(cap.map, 0);
            assert_eq!(cap.x, -8819.3);
            assert_eq!(cap.y, 636.2);
            assert_eq!(cap.z, 94.1);
        }
    }

    #[test]
    fn faction_capital_horde_races_map_to_orgrimmar() {
        for race in [2u8, 5, 6, 8, 10] {
            let cap = faction_capital(race).unwrap_or_else(|| panic!("race {race} should map"));
            assert_eq!(cap.name, "Orgrimmar");
            assert_eq!(cap.map, 1);
            assert_eq!(cap.x, 1609.2);
            assert_eq!(cap.y, -4407.7);
            assert_eq!(cap.z, 17.5);
        }
    }

    #[test]
    fn faction_capital_non_capital_race_is_none() {
        // Race 9 = goblin -- not a faction-capital owner in this map,
        // matching the oracle's `*)` fallthrough.
        assert!(faction_capital(9).is_none());
        assert!(faction_capital(0).is_none());
        assert!(faction_capital(255).is_none());
    }

    #[test]
    fn return_home_select_sql_is_exact() {
        assert_eq!(
            RETURN_HOME_SELECT_SQL,
            "SELECT guid, race, online FROM characters WHERE name = ? LIMIT 1"
        );
    }

    #[test]
    fn return_home_update_sql_is_exact() {
        assert_eq!(
            RETURN_HOME_UPDATE_SQL,
            "UPDATE characters SET position_x=?, position_y=?, position_z=?, map=?, orientation=0 WHERE guid=?"
        );
    }

    #[test]
    fn return_home_online_result_ok_passes_through() {
        assert_eq!(
            return_home_online_result(SoapOutcome::Ok("done".into())).unwrap(),
            "done"
        );
    }

    #[test]
    fn return_home_online_result_fault_decodes_with_combat_hint() {
        let e = return_home_online_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "a<b");
        assert_eq!(
            e.hint,
            "The character can't be teleported in combat or on a flight path -- try again once it is idle."
        );
    }

    #[test]
    fn return_home_online_result_auth_uses_shorter_message() {
        let e = return_home_online_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP auth failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");
    }

    #[test]
    fn return_home_online_result_unreachable() {
        let e = return_home_online_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(e.hint, "Is it running?");
    }

    // --- Part 5a: `strip_command_sub_trailing_newlines` (`config raw-read`) -

    #[test]
    fn cfg_missing_file_err_has_empty_hint() {
        // Unlike conf-keys' own missing-file message, raw-read/raw-reset/
        // raw-write all carry an EMPTY hint (`90-main.sh:2695,2714,2735`).
        let e = cfg_missing_file_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Missing --file <name>");
        assert_eq!(e.hint, "");
    }

    #[test]
    fn strip_command_sub_trailing_newlines_strips_every_trailing_newline() {
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\n"), "A = 1");
        // Command substitution strips EVERY trailing newline, not just one.
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\n\n\n"), "A = 1");
        assert_eq!(strip_command_sub_trailing_newlines("A = 1"), "A = 1");
        assert_eq!(strip_command_sub_trailing_newlines(""), "");
        // Only trailing -- interior blank lines are untouched.
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\n\nB = 2\n"), "A = 1\n\nB = 2");
        // CR right before the final \n is not itself a newline char -- left alone.
        assert_eq!(strip_command_sub_trailing_newlines("A = 1\r\n"), "A = 1\r");
    }

    // --- Part 5a: `wow_teleport_coords_native` / `games_status` ---------

    #[test]
    fn teleport_coords_select_sql_is_exact() {
        assert_eq!(TELEPORT_COORDS_SELECT_SQL, "SELECT guid, online FROM characters WHERE name = ? LIMIT 1");
    }

    #[test]
    fn teleport_coords_update_sql_is_exact() {
        assert_eq!(
            TELEPORT_COORDS_UPDATE_SQL,
            "UPDATE characters SET position_x=?, position_y=?, position_z=?, map=?, orientation=0 WHERE guid=?"
        );
    }

    #[test]
    fn char_online_err_matches_oracle_message_and_hint() {
        let e = char_online_err("Kaldric");
        assert_eq!(e.code, "CHAR_ONLINE");
        assert_eq!(e.message, "Character must be logged out: Kaldric");
        assert_eq!(e.hint, "Character must be logged out.");
    }

    // -- Task B2a: `wow_config_set_native` --------------------------------

    #[test]
    fn motd_result_ok_and_error_shapes() {
        assert!(motd_result(SoapOutcome::Ok("ignored".into())).is_ok());

        let e = motd_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        // RAW, not entity-decoded (matches the oracle's `$out` interpolation).
        assert_eq!(e.message, "a&lt;b");
        assert_eq!(e.hint, "The server rejected the motd command.");

        let e = motd_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");

        let e = motd_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(
            e.hint,
            "The server must be running to change the message of the day - start it first."
        );
    }

    // -- config_set_direct / config_set_curated ---------------------------

    struct TmpTitleDir(std::path::PathBuf);
    impl TmpTitleDir {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("dml-b2a-cfgset-test-{}-{}", std::process::id(), name));
            let _ = std::fs::remove_dir_all(&dir);
            let modules = dir.join("env").join("dist").join("etc").join("modules");
            std::fs::create_dir_all(&modules).unwrap();
            // Marks the title as "installed" for `wow_server_installed`'s
            // compose-file check, matching a real title dir.
            std::fs::write(dir.join("docker-compose.yml"), "services: {}\n").unwrap();
            TmpTitleDir(dir)
        }
        fn modules_dir(&self) -> std::path::PathBuf {
            self.0.join("env").join("dist").join("etc").join("modules")
        }
        fn write_module_conf(&self, name: &str, content: &str) {
            std::fs::write(self.modules_dir().join(name), content).unwrap();
        }
        fn write_override(&self, yaml: &str) {
            std::fs::write(self.0.join("docker-compose.override.yml"), yaml).unwrap();
        }
        fn read_module_conf(&self, name: &str) -> String {
            std::fs::read_to_string(self.modules_dir().join(name)).unwrap()
        }
    }
    impl Drop for TmpTitleDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn no_soap_lock() -> Arc<std::sync::Mutex<()>> {
        Arc::new(std::sync::Mutex::new(()))
    }

    #[test]
    fn config_set_direct_rejects_core_conf() {
        let t = TmpTitleDir::new("core");
        let e = config_set_direct(&t.0, "conf:Rate.XP.Kill", "3", &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Direct conf keys are limited to module confs");
        assert_eq!(
            e.hint,
            "Core server settings live in the curated list: dml wow config list --json"
        );
    }

    #[test]
    fn config_set_direct_rejects_denylisted_key() {
        let t = TmpTitleDir::new("denylist");
        let e = config_set_direct(
            &t.0,
            "conf:playerbots.conf:AiPlayerbot.DeleteRandomBotAccounts",
            "1",
            &no_soap_lock(),
        )
        .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(
            e.message,
            "AiPlayerbot.DeleteRandomBotAccounts is managed by the bot flush tool"
        );
        assert!(e.hint.starts_with("Use: dml wow bots flush"));
    }

    #[test]
    fn config_set_direct_rejects_bad_conf_key_shape() {
        let t = TmpTitleDir::new("shape");
        let e =
            config_set_direct(&t.0, "conf:playerbots.conf:Has Space", "1", &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Invalid conf key: Has Space");
        assert_eq!(e.hint, "Letters, digits, dots and underscores only.");
    }

    #[test]
    fn config_set_direct_rejects_multiline_and_overlong_values() {
        let t = TmpTitleDir::new("valueshape");
        let e = config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", "a\nb", &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "The value must be a single line");

        let long = "x".repeat(201);
        let e =
            config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", &long, &no_soap_lock())
                .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Value too long (max 200 characters)");
    }

    #[test]
    fn config_set_direct_not_found_when_conf_and_dist_both_absent() {
        let t = TmpTitleDir::new("notfound");
        let e =
            config_set_direct(&t.0, "conf:ghost.conf:Some.Key", "1", &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "Not an editable module conf: ghost.conf");
        assert_eq!(e.hint, "See: dml wow config files --json");
    }

    #[test]
    fn config_set_direct_writes_conf_and_reports_restart_when_no_reload_cmd() {
        let t = TmpTitleDir::new("success");
        t.write_module_conf("playerbots.conf", "AiPlayerbot.Foo = 1\n");
        let out =
            config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", "42", &no_soap_lock())
                .unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        assert_eq!(out["applied"], "restart");
        assert_eq!(t.read_module_conf("playerbots.conf"), "AiPlayerbot.Foo = 42\n");
    }

    #[test]
    fn config_set_direct_cleans_legacy_env_and_still_reports_restart() {
        let t = TmpTitleDir::new("legacyenv");
        t.write_module_conf("playerbots.conf", "AiPlayerbot.Foo = 1\n");
        t.write_override(
            "services:\n  ac-worldserver:\n    environment:\n      AC_AI_PLAYERBOT_FOO: \"1\"\n",
        );
        let out =
            config_set_direct(&t.0, "conf:playerbots.conf:AiPlayerbot.Foo", "42", &no_soap_lock())
                .unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        assert_eq!(out["applied"], "restart");
        let text = std::fs::read_to_string(t.0.join("docker-compose.override.yml")).unwrap();
        assert!(!dml_wow::config::parse_override_env(&text).contains_key("AC_AI_PLAYERBOT_FOO"));
    }

    fn curated_row(
        key: &str,
        kind: &str,
        min: serde_json::Value,
        max: serde_json::Value,
        env: &str,
        label: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "key": key, "group": "Test", "label": label, "explain": "x",
            "type": kind, "min": min, "max": max, "value": "", "default": "0",
            "restart_required": true, "env": env,
        })
    }

    #[test]
    fn config_set_curated_unknown_key_is_not_found() {
        let t = TmpTitleDir::new("curated-unknown");
        let rows = vec![curated_row("rates.xp_kill", "float", 0.5.into(), 20.into(), "conf:Rate.XP.Kill", "XP")];
        let e =
            config_set_curated(&t.0, "no.such.key", "1", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "Unknown setting: no.such.key");
        assert_eq!(e.hint, "See: dml wow config list --json");
    }

    #[test]
    fn config_set_curated_not_found_when_server_not_installed() {
        // A title dir that doesn't exist at all -- `wow_server_installed`
        // must fail closed, mirroring `_wow_server_dir` (`90-main.sh:106-110`)
        // returning empty when `$GAMES_DIR/wow-server-playerbots` is absent.
        let missing_dir = std::env::temp_dir()
            .join(format!("dml-b2a-cfgset-test-{}-not-installed-at-all", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing_dir);
        let rows = vec![curated_row("rates.xp_kill", "float", 0.5.into(), 20.into(), "conf:Rate.XP.Kill", "XP")];
        let e = config_set_curated(&missing_dir, "rates.xp_kill", "3", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "WoW Playerbots server not installed");
        assert_eq!(e.hint, "Install it first, then re-run.");
    }

    #[test]
    fn config_set_curated_not_installed_check_wins_over_bad_arg() {
        // Server not installed AND the value is out of range -- the oracle's
        // `_cfg_preamble` (`90-main.sh:2443`) runs BEFORE the type/range
        // switch (`90-main.sh:2445`), so the top-level verdict must be
        // NOT_FOUND, never BAD_ARG.
        let missing_dir = std::env::temp_dir()
            .join(format!("dml-b2a-cfgset-test-{}-not-installed-badval", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing_dir);
        let rows = vec![curated_row(
            "rates.xp_kill",
            "float",
            0.5.into(),
            20.into(),
            "conf:Rate.XP.Kill",
            "XP from kills",
        )];
        let e = config_set_curated(&missing_dir, "rates.xp_kill", "999.9", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "WoW Playerbots server not installed");
    }

    #[test]
    fn config_set_curated_motd_not_found_when_server_not_installed() {
        // The `server.motd` sub-branch has no conf-write of its own (it goes
        // straight to SOAP) -- confirm it still gets the install-check gate
        // instead of ever reaching the SOAP call.
        let missing_dir = std::env::temp_dir()
            .join(format!("dml-b2a-cfgset-test-{}-not-installed-motd", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing_dir);
        let rows = vec![curated_row(
            "server.motd",
            "text",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "-",
            "Message of the day",
        )];
        let e = config_set_curated(&missing_dir, "server.motd", "hi", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "WoW Playerbots server not installed");
    }

    #[test]
    fn config_set_curated_float_out_of_range_is_bad_arg() {
        let t = TmpTitleDir::new("curated-floatrange");
        let rows = vec![curated_row(
            "rates.xp_kill",
            "float",
            0.5.into(),
            20.into(),
            "conf:Rate.XP.Kill",
            "XP from kills",
        )];
        let e = config_set_curated(&t.0, "rates.xp_kill", "20.5", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "XP from kills must be a number between 0.5 and 20, got: 20.5");
        assert_eq!(e.hint, "");
    }

    #[test]
    fn config_set_curated_bool_non01_is_bad_arg() {
        let t = TmpTitleDir::new("curated-bool");
        let rows = vec![curated_row(
            "crossfaction.group",
            "bool",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "conf:AllowTwoSide.Interaction.Group",
            "Group across factions",
        )];
        let e =
            config_set_curated(&t.0, "crossfaction.group", "2", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Group across factions takes 1 (on) or 0 (off), got: 2");
    }

    #[test]
    fn config_set_curated_int_out_of_range_is_bad_arg() {
        let t = TmpTitleDir::new("curated-int");
        let rows = vec![curated_row(
            "bots.population",
            "int",
            0.into(),
            3000.into(),
            "conf:playerbots.conf:AiPlayerbot.MaxRandomBots",
            "World bot population",
        )];
        let e = config_set_curated(&t.0, "bots.population", "3001", &rows, &no_soap_lock()).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(
            e.message,
            "World bot population must be a whole number between 0 and 3000, got: 3001"
        );
    }

    #[test]
    fn config_set_curated_char_invalid_is_bad_arg() {
        let t = TmpTitleDir::new("curated-char");
        let rows = vec![curated_row(
            "ahbot.character",
            "char",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "conf:mod_ahbot.conf:AuctionHouseBot.GUID",
            "Seller character",
        )];
        let e = config_set_curated(&t.0, "ahbot.character", "bad name!", &rows, &no_soap_lock())
            .unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Invalid character name: bad name!");
        assert_eq!(e.hint, "1-12 letters/digits/underscore.");
    }

    #[test]
    fn config_set_curated_text_row_sanitizes_and_writes_conf() {
        // Deliberately NOT worldserver.conf/mod_ahbot.conf -- keeps this test
        // network-free (those two conf files are the only ones that attempt a
        // live SOAP `reload config`, which is smoke-gated per the brief).
        let t = TmpTitleDir::new("curated-text");
        t.write_module_conf("mod_something.conf", "Some.Text = old\n");
        let rows = vec![curated_row(
            "some.text",
            "text",
            serde_json::Value::Null,
            serde_json::Value::Null,
            "conf:mod_something.conf:Some.Text",
            "Some text",
        )];
        let out = config_set_curated(
            &t.0,
            "some.text",
            "has \"quotes\" and\nnewline",
            &rows,
            &no_soap_lock(),
        )
        .unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        assert_eq!(out["applied"], "restart");
        assert_eq!(t.read_module_conf("mod_something.conf"), "Some.Text = has quotes and newline\n");
    }

    #[test]
    fn config_set_curated_bots_population_writes_min_and_max() {
        let t = TmpTitleDir::new("curated-botspop");
        t.write_module_conf(
            "playerbots.conf",
            "AiPlayerbot.MaxRandomBots = 500\nAiPlayerbot.MinRandomBots = 500\n",
        );
        let rows = vec![curated_row(
            "bots.population",
            "int",
            0.into(),
            3000.into(),
            "conf:playerbots.conf:AiPlayerbot.MaxRandomBots",
            "World bot population",
        )];
        let out =
            config_set_curated(&t.0, "bots.population", "800", &rows, &no_soap_lock()).unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["applied"], "restart");
        let text = t.read_module_conf("playerbots.conf");
        assert!(text.contains("AiPlayerbot.MaxRandomBots = 800"));
        assert!(text.contains("AiPlayerbot.MinRandomBots = 800"));
    }

    #[test]
    fn config_set_curated_non_conf_env_column_writes_override() {
        let t = TmpTitleDir::new("curated-envcol");
        // No real registry row has a bare env column today (every row is
        // `conf:` or motd's `-`) -- this exercises the oracle's fallback
        // `else` branch (`90-main.sh:2557-2560`) for parity anyway.
        let rows = vec![curated_row(
            "made.up",
            "int",
            0.into(),
            10.into(),
            "AC_MADE_UP",
            "Made up",
        )];
        let out = config_set_curated(&t.0, "made.up", "5", &rows, &no_soap_lock()).unwrap();
        assert_eq!(out["changed"], true);
        assert_eq!(out["restart_required"], true);
        // NOTE: no `applied` field on this branch, matching the bash oracle.
        assert!(out.get("applied").is_none());
        let text = std::fs::read_to_string(t.0.join("docker-compose.override.yml")).unwrap();
        assert_eq!(
            dml_wow::config::parse_override_env(&text).get("AC_MADE_UP").map(String::as_str),
            Some("5")
        );
    }

    // -- native world-restart: event-shape builders --------------------------
    // Task: world-restart-native. Assert the EXACT ndjson event shapes the
    // frontend's terminal-state.ts parses (see the task brief) -- these are
    // pure, so no docker/soap I/O is exercised.

    // -- native world-restart: pure decision logic ----------------------------

    // -- native bridge-setup: event-shape builders (Chunk 2 task C2c item 4) --

    // -- native ahbot-repair: event-shape builders (Chunk 2 task C2c item 8) --

}
