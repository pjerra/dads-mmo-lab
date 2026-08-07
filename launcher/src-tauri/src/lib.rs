pub mod nativesetup;
pub mod payload;
pub mod power;
pub mod provision;
pub mod realmlist;
mod startup;
mod tray;
mod autostart;
mod single_instance;
pub mod watch;
pub mod wslconfig;
mod zam;

use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::Manager;
use tauri::State;

use dml_core::runner::DmlRunner;

// Helpers that USED to be private to this file and moved into the library
// with the orchestration bodies that needed them (cargo-workspace refactor,
// Task 9). Imported under their original names so the many remaining call
// sites here read exactly as before.
use dml_core::error::{bad_arg, io_internal_err, not_found_err};
use dml_core::proc::output_bounded;
// `pub` because `realmlist.rs` reaches it as `crate::envelope_to_result`.
pub use dml_core::envelope::envelope_to_result;
use dml_wow::config::{cfg_installed_err, cfg_missing_file_err, cfg_not_editable_err};
use dml_wow::db::{cell_string, count_result, db_err_to_cmd, db_unreachable_err, sql_row_int};
// `validate_ip`/`validate_host` are `pub use`d: `realmlist.rs` reaches
// `validate_ip` as `crate::validate_ip`.
pub use dml_wow::lan::{validate_host, validate_ip};
use dml_wow::lan::{is_loopback_or_private, LAN_ACTIONS, LAN_TITLE};
use dml_wow::party::{
    bot_member_classes, bot_member_names, live_spec_names, party_not_online_err, party_online_guid,
    preset_dir_or_internal_err,
};
use dml_wow::soap_cmds::{char_is_online, not_online_err, party_fire_result};

pub struct InstallSession {
    pub stdin: std::process::ChildStdin,
    pub pid: u32,
}

pub enum InstallSlot {
    Starting,
    Running(InstallSession),
    /// A NATIVE install running inside this process (`games_install_native`).
    ///
    /// It shares the slot with the WSL passthrough so the two can never run at
    /// once — they would fight over the same title directory — but it carries
    /// neither a `stdin` nor a killable pid, and that is not an oversight:
    /// * the engine asks no questions, so there is nothing to type at;
    /// * its children are `git`/`docker` spawned by US, so `taskkill /T` on our
    ///   own pid would take the launcher down with them.
    /// The two session commands therefore refuse for this variant rather than
    /// pretending, which is why this is a distinct variant and not a
    /// `Running` with dummy fields.
    Native,
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
    /// When the frontend last pushed a server status. The keep-awake watchdog
    /// reads this to notice a STALLED webview poll: keep-awake is engaged by
    /// the webview's 7s poll loop, and before close-to-tray existed it could
    /// never leak because process exit cleared it. Now that the app survives
    /// window close, a hidden window whose timers WebView2 throttles would
    /// otherwise hold the sleep block forever.
    pub last_status_push: Arc<Mutex<Option<std::time::Instant>>>,
    /// Where automatic SOAP account setup got to THIS launcher run.
    ///
    /// One attempt per run, and that bound is the feature. The trigger is the
    /// status poll, which ticks every few seconds; without a latch a server
    /// that keeps refusing us would get one INSERT per tick. Once this reaches
    /// `Done`, `wow_soap_autosetup` re-reports the conclusion it already
    /// reached without opening a SOAP connection or a DB connection.
    ///
    /// Known limit, deliberate: wiping the auth database mid-session needs a
    /// launcher restart to self-heal. The alternative is an unlatched loop
    /// writing rows into a database that keeps losing them.
    pub soap_autosetup: Arc<Mutex<dml_wow::soap_autosetup::AutoSetup>>,
}

pub use dml_core::error::CmdError;

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

const TAILSCALE_ACTIONS: [&str; 4] = ["install", "up", "status", "down"];
const TOOL_NAMES: [&str; 2] = ["unbound", "unbound-remove"];

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

/// The frontend pushes its polled verdict here so the tray can show server
/// state while the window is hidden. Rust has NO status poller of its own —
/// polling is entirely frontend-driven (a 7s setInterval), and duplicating it
/// here would open a second unserialized SOAP client and would flap during
/// restarts, because the `restarting` suppression flag lives only in the
/// webview. Sync and infallible, same doctrine as `set_keep_awake`.
#[tauri::command]
fn tray_set_status(app: tauri::AppHandle, verdict: String, state: State<'_, AppState>) {
    if let Ok(mut t) = state.last_status_push.lock() {
        *t = Some(std::time::Instant::now());
    }
    tray::apply_status(&app, &verdict);
}

/// Whether a Windows Run entry exists AND points at a file that still exists.
#[tauri::command]
fn autostart_get() -> bool {
    autostart::enabled()
}

/// Turn start-with-Windows on or off. Records the CURRENT exe path, so a dev
/// build and an installed build register different targets.
#[tauri::command]
fn autostart_set(on: bool) -> Result<(), CmdError> {
    autostart::set(on).map_err(|e| CmdError {
        code: "AUTOSTART_FAILED".into(),
        message: e,
        hint: String::new(),
    })
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

/// NATIVE-MODE `module repair` — see
/// [`dml_wow::moduletail::module_repair`]. The three closed-allowlist arg
/// checks stay here in the wrapper (webview input), the work itself is in
/// the library.
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
    tauri::async_runtime::spawn_blocking(move || dml_wow::moduletail::module_repair(key, db, mode, files))
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

// ---------------------------------------------------------------------------
// First-run backend probe (SHIP-LIST Phase 4). The chain and its state machine
// live in `dml_core::setup` (no tauri there); this is the adapter plus the one
// launcher-only fact the chain cannot know — whether the payload the setup
// command would install actually shipped inside this exe.
// ---------------------------------------------------------------------------

/// The typed answer `backend_status` returns.
///
/// [`dml_core::setup::BackendStatus`] is FLATTENED, so `state` sits at the top
/// level: this is a switch target, not a tree to walk. The two extra fields
/// exist so a consumer never needs a second round trip:
///
/// * `backend_mode` — a native-mode user runs a real server with no distro at
///   all. Without this, a first-run screen would read `NoWsl` and tell someone
///   with a working server to go install WSL.
/// * `payload` — `NoCli`/`CliOutdated` are the states with an "install the
///   backend" button on them, and that button is powered by the bundled
///   resources. If they did not ship, the honest screen says so instead of
///   offering a fix that cannot work.
#[derive(Debug, Serialize)]
pub struct BackendStatusReport {
    #[serde(flatten)]
    pub backend: dml_core::setup::BackendStatus,
    pub backend_mode: &'static str,
    pub payload: crate::payload::PayloadStatus,
}

/// Assemble the report. Pure, so the JSON shape the first-run screen and the
/// setup command build against is pinned by a test rather than by a click.
pub fn backend_status_report(
    backend: dml_core::setup::BackendStatus,
    payload: crate::payload::PayloadStatus,
    native: bool,
) -> BackendStatusReport {
    BackendStatusReport {
        backend,
        backend_mode: if native { "native" } else { "wsl" },
        payload,
    }
}

/// The probe environment [`backend_status`] hands the chain. Split out of the
/// command body so the wall-clock budgets are pinned by a test — a
/// `#[tauri::command]` needs an `AppHandle` and cannot be called from one.
///
/// The budgets themselves are `dml_core::setup`'s defaults; what splitting this
/// out lets a test assert is that `backend_status` does not quietly narrow
/// them. It is the call that gates Home on every app start, so the cold-boot
/// budget is load-bearing here specifically (SHIP-LIST Phase 4 review,
/// P4/P8/P12).
pub fn backend_probe_env() -> dml_core::setup::SetupProbeEnv {
    dml_core::setup::SetupProbeEnv::new(dml_core::runner::DISTRO, dml_core::runner::USER)
}

/// Everything [`backend_status`] does, with the one thing a test cannot have —
/// the actual `wsl.exe` spawn — injected. `spawn` is handed the program, the
/// argv and the budget ALREADY RESOLVED to a wall clock.
///
/// THIS IS THE SEAM, AND IT IS DELIBERATELY BELOW THE BUDGET. A test that only
/// asserts on what [`backend_probe_env`] returns pins a helper, not a call
/// site: narrowing the env between building it and handing it to the chain
/// leaves such a test green (verified — `env.cold_timeout =
/// Duration::from_secs(20)` here, the exact regression, went undetected). What
/// the chain is given can only be observed where the budget is spent, so the
/// injection point is the spawn and the thing under test is the path Home
/// takes on every app start.
pub fn backend_status_with(
    resource_dir: Option<&std::path::Path>,
    native: bool,
    mut spawn: impl FnMut(
        &std::ffi::OsStr,
        &[&str],
        std::time::Duration,
    ) -> dml_core::setup::ProbeOutcome,
) -> BackendStatusReport {
    // Native mode asks native questions. Running the WSL chain here answered
    // `no_wsl` for a machine with a perfectly good Docker setup, and the
    // frontend's only defence was to show a native user NO first-run screen at
    // all — so a native PC with no server landed on Home staring at a status
    // card for a server that does not exist. That is the exact problem this
    // phase exists to remove, and it was still live for the backend the project
    // is moving to.
    let backend = if native {
        dml_core::setup::derive_native(native_facts())
    } else {
        let env = backend_probe_env();
        dml_core::setup::probe_with(&env.distro, &env.user, |args, budget| {
            spawn(&env.wsl_program, args, env.budget(budget))
        })
    };
    backend_status_report(backend, crate::payload::resolve_opt(resource_dir), native)
}

/// The three facts the native chain runs on, gathered from this machine.
///
/// Tri-state throughout, and the ordering matters: `engine_running` is only
/// asked when Docker is actually installed, because `docker info` against a
/// program that does not exist is a failure that says nothing about the engine.
fn native_facts() -> dml_core::setup::NativeFacts {
    use dml_core::setup::Tri;
    let program = dml_core::engine::docker_desktop_program();
    let docker_installed = if program.is_some() { Tri::Yes } else { Tri::No };
    let engine_running = match &program {
        // BOUNDED, and that matters more here than almost anywhere else. This
        // runs on the path that gates Home on every app start, and the screen
        // it feeds is the one with no other repair on it -- a wedged docker
        // daemon would leave `probing` true forever and disable the single
        // button the user has. `dml_core::engine::engine_running` calls
        // `Command::status()` with no deadline at all; `maint::docker_engine_up`
        // exists precisely because of that and says so in its own doc comment.
        Some(_) => {
            if dml_wow::maint::docker_engine_up(
                &dml_core::engine::docker_program(),
                dml_wow::maint::PROBE_TIMEOUT,
            ) {
                Tri::Yes
            } else {
                Tri::No
            }
        }
        None => Tri::Unknown,
    };
    dml_core::setup::NativeFacts { docker_installed, engine_running, titles: native_title_count() }
}

/// How many titles are installed, or `None` when we could not look.
///
/// `None` is NOT zero: "install your server" and "we could not read your games
/// folder" are different screens, and conflating them would offer a fresh
/// install to someone whose existing server is merely unreadable.
fn native_title_count() -> Option<usize> {
    let dir = dml_core::compose::games_dir_from_env();
    let entries = match std::fs::read_dir(&dir) {
        Ok(it) => it,
        // A GAMES DIR THAT IS NOT THERE HOLDS ZERO TITLES. That is a definite
        // answer, not a shrug, and collapsing it into `None` broke the fresh
        // native machine this whole arm was built for: nothing creates
        // `%USERPROFILE%\dml-native` before the first install (the engine makes
        // it itself), so EVERY new native user hit the could-not-tell screen —
        // "the launcher couldn't read back the list of installed games", which
        // is false twice over — behind a "Check again" button that re-ran the
        // identical failing read forever. The `no_titles` → "Open Library" arm
        // was unreachable for exactly the user it exists for.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(0),
        // Anything else — a permissions failure, a dead drive — genuinely is a
        // could-not-tell, and must stay one.
        Err(_) => return None,
    };
    Some(entries.filter_map(|e| e.ok()).filter(|e| native_title_is_usable(&e.path())).count())
}

/// Is this directory a title the user could actually PLAY, as opposed to one
/// that merely exists?
///
/// Two conditions, and the second is the one that was missing:
///
/// 1. It has a generated compose file, so it is a title directory at all rather
///    than `tools/` or a stray folder.
/// 2. Its install is not still in progress. `generate-compose` is stage 5 of 8,
///    so a build that died during the multi-hour `build` stage leaves a
///    perfectly good compose file behind and satisfies condition 1 alone. Left
///    at that, the first-run chain called such a machine `Ready`, showed no
///    first-run screen, and dropped the user on Home in front of a Start button
///    for a server whose image was never compiled.
///
/// The state file is the authority for the second question — the same one
/// `games_install_native_state` and the engine's own resume logic use, so the
/// three cannot disagree.
fn native_title_is_usable(path: &std::path::Path) -> bool {
    if !path.join(dml_wow::composegen::BASE_FILE).is_file() {
        return false;
    }
    match dml_wow::install_native::load_state(path) {
        // An unfinished install is not a title the user can play. `next_stage`
        // is None when every stage is recorded, i.e. it DID finish.
        Some(st) => dml_wow::install_native::next_stage(&st).is_none(),
        // No state file: either a title installed some other way (the WSL
        // route, or a migrated server) or one predating the engine. Both are
        // real, working servers, so absence must not read as "unfinished".
        None => true,
    }
}

/// Probe this machine and report the FIRST thing standing between the user and
/// a running server: no WSL → no `dml-arch` distro → no `dml` CLI in it (or an
/// outdated one) → no titles installed.
///
/// SHIP-LIST Phase 4's seam. Both the first-run screen (4.4) and the setup
/// command (4.2) consume THIS — one chain, one answer, so the two can never
/// disagree about what state the machine is in.
///
/// Bounded end to end, with TWO budgets (see [`backend_probe_env`]): the
/// host-side calls are capped at [`dml_core::setup::DEFAULT_PROBE_TIMEOUT`] and
/// the one call that may cold-start the WSL2 VM at
/// [`dml_core::setup::DEFAULT_COLD_START_TIMEOUT`]. The chain short-circuits at
/// the first missing link, so a machine with no WSL answers immediately and a
/// wedged `wsl.exe` costs one timeout, not four. Runs on the blocking pool —
/// it shells subprocesses and must never sit on the IPC thread.
///
/// NEVER fails: an unreachable probe is reported as
/// [`dml_core::setup::SetupState::Unknown`] with the step named, not as a
/// command error. A first-run screen that throws has nothing to show, which is
/// the exact failure this whole phase exists to remove.
#[tauri::command]
async fn backend_status(app: tauri::AppHandle) -> Result<BackendStatusReport, CmdError> {
    // Resolve the resource dir on THIS side: `AppHandle::path()` is the only
    // piece that needs tauri, and `Option` already carries the could-not-tell.
    let resource_dir = app.path().resource_dir().ok();
    let native = is_native_backend();
    tauri::async_runtime::spawn_blocking(move || {
        backend_status_with(resource_dir.as_deref(), native, dml_core::setup::spawn_probe)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })
}

/// Provision the `dml-arch` distro from the resources bundled INTO THIS EXE:
/// the `dml` CLI to `/usr/local/bin`, the Eluna bridge scripts to
/// `/usr/local/share/dml/lua/{party,gm}`, and the six title installers to
/// `/usr/local/share/dml/installers`.
///
/// SHIP-LIST 4.2 — the user's replacement for `cli/dev-install.ps1`, which
/// hardcoded one developer's repo path and therefore could not run on anybody
/// else's machine. Everything here resolves at runtime: the sources come from
/// [`tauri::path::PathResolver::resource_dir`], never a repo path, and the
/// flow's own `wslpath` call translates that into something the distro can
/// read.
///
/// STREAMED, because it is slow enough to look hung: the first `wsl.exe` call
/// on a cold machine boots the WSL2 VM. Emits the ordinary TermEvent
/// vocabulary through the same `Channel` seam every other long job uses, so
/// the first-run screen renders it in the standard Terminal component.
///
/// NEVER returns `Err` for an unhappy machine — a missing distro, a failed
/// copy and a version mismatch are all `error` events on the stream (the
/// contract the rest of the streamed commands keep: the UI derives its verdict
/// from `done`/`error`, not from the promise). An `Err` here means the
/// blocking task itself could not be joined.
///
/// The state machine, the messages and the argv are all in
/// [`crate::provision`], where they are unit-tested without a machine that
/// happens to be in the right state; this is only the adapter.
#[tauri::command]
async fn backend_setup(
    app: tauri::AppHandle,
    on_event: Channel<serde_json::Value>,
) -> Result<(), CmdError> {
    // Resolve the resource dir on THIS side: `AppHandle::path()` is the only
    // piece that needs tauri, and `Option` already carries the could-not-tell
    // (which `provision` reports as PAYLOAD_UNKNOWN rather than as "missing").
    let resource_dir = app.path().resource_dir().ok();
    tauri::async_runtime::spawn_blocking(move || {
        let env = crate::provision::ProvisionEnv::new(
            dml_core::runner::DISTRO,
            dml_core::runner::USER,
        );
        crate::provision::provision(&env, resource_dir.as_deref(), |v| {
            let _ = on_event.send(v);
        });
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })
}

/// `~/.dml` or a typed error. Both launcher-config commands need it and both
/// must fail the same way when the home directory cannot be resolved.
fn launcher_home() -> Result<std::path::PathBuf, CmdError> {
    dml_core::util::dml_home_dir().ok_or_else(|| CmdError {
        code: "NO_HOME".into(),
        message: "Could not resolve the home directory".into(),
        hint: "Set USERPROFILE or HOME.".into(),
    })
}

/// The launcher's own settings, plus which source currently WINS for the
/// backend. The UI needs `backendSource` to explain why its dropdown is
/// read-only when an env var overrides the file — otherwise changing the
/// dropdown appears to do nothing.
#[tauri::command]
fn launcher_config_read() -> Result<serde_json::Value, CmdError> {
    let cfg = dml_core::launcher_config::load(&launcher_home()?);
    // MUST use the flag captured at startup, NOT std::env::var: by the time
    // any command runs, `resolve_and_export` has already written our own
    // resolved value into DML_BACKEND, so reading the env here would report
    // EVERY session as env-locked and leave the dropdown permanently
    // read-only — defeating the whole setting.
    let env_backend = if startup::backend_pinned_by_env() {
        std::env::var("DML_BACKEND").ok().filter(|v| !v.trim().is_empty())
    } else {
        None
    };
    let source = if env_backend.is_some() {
        "env"
    } else if cfg.backend.as_deref().is_some_and(|v| !v.eq_ignore_ascii_case("auto")) {
        "file"
    } else {
        "auto"
    };
    Ok(serde_json::json!({
        "config": cfg,
        "backendSource": source,
        "effectiveBackend": backend_mode(),
        "envBackend": env_backend,
    }))
}

/// Persist the settings. A backend change applies on the NEXT launch —
/// `AppState`'s runner is built once at startup from `selected()` — so the UI
/// must say so rather than imply a live switch.
#[tauri::command]
fn launcher_config_write(cfg: dml_core::launcher_config::LauncherConfig) -> Result<(), CmdError> {
    dml_core::launcher_config::save(&launcher_home()?, &cfg).map_err(|e| CmdError {
        code: "WRITE_FAILED".into(),
        message: format!("Could not write launcher.json: {e}"),
        hint: String::new(),
    })
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

/// Pure half of the mirror-image guard: some commands act on the dml-arch
/// DISTRO itself, and native mode has no distro at all. Split from the env read
/// so the decision is unit-testable without mutating `DML_BACKEND` in a
/// threaded test runner — the risk worth pinning is the one-keyword inversion
/// that would let such a command through in exactly the mode it cannot work in.
fn wsl_backend_guard(native: bool) -> Result<(), CmdError> {
    if native {
        Err(CmdError {
            code: "WRONG_BACKEND".into(),
            message: "This command only works in WSL mode -- native mode has no dml-arch distro"
                .into(),
            hint: "The launcher is running in native (Docker Desktop) mode; restart the engine \
                   from the Native setup card instead."
                .into(),
        })
    } else {
        Ok(())
    }
}

/// WSL-mode counterpart of [`require_native_backend`].
fn require_wsl_backend() -> Result<(), CmdError> {
    wsl_backend_guard(is_native_backend())
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

/// `dml wow config set` port — see [`dml_wow::config::config_set`] for the
/// `conf:`-vs-curated routing and both write paths. Native-only.
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

    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::config::config_set(key, value, soap_lock, config_lock, title_dir)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// `dml wow config tuning-set` port — see [`dml_wow::tuning::tuning_set`].
/// Native-only. No `runner`: as of the cargo-workspace refactor's Task 11
/// (controller ruling D2) BOTH tuning backends are native Rust, so this no
/// longer hands the lua-backend rows off to the bash CLI.
#[tauri::command]
async fn wow_config_tuning_set_native(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let config_lock = state.config_lock.clone();
    let title_dir = dml_wow::config::ConfigReader::title_dir_from_env();

    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::tuning::tuning_set(key, value, config_lock, title_dir)
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
// NATIVE-MODE `config raw-reset` + `pb-keys`/`conf-keys` (Part 5a).
// `raw-read`/`raw-write` moved to `dml_wow::config` in Task 9b; the shared
// `_cfg_preamble` equivalent went with them (every arm needs the WoW
// Playerbots title installed -- native has no yq dependency to check). Read
// paths take no lock; `raw-reset` takes `config_lock`, same as every other
// native writer -- Settings/Modules/raw-editor writes must never interleave
// on the same conf file.
// ---------------------------------------------------------------------------

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

/// NATIVE-MODE `config raw-read` — see [`dml_wow::config::raw_read`].
#[tauri::command]
async fn wow_config_raw_read_native(file: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    tauri::async_runtime::spawn_blocking(move || dml_wow::config::raw_read(file))
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
}

/// NATIVE-MODE `config raw-write` — see [`dml_wow::config::raw_write`],
/// including why the guard ORDER inside it is load-bearing.
#[tauri::command]
async fn wow_config_raw_write_native(
    file: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let config_lock = state.config_lock.clone();
    tauri::async_runtime::spawn_blocking(move || dml_wow::config::raw_write(file, content, config_lock))
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
// would reject -- membership by construction, charset via the frontend's
// buildSpecIndex/isValidSpecShape mirror of the CLI guard.
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

/// NATIVE-MODE `dml_summon_npc` bridge command — see
/// [`dml_wow::soap_cmds::gm_summon`]. The wrapper only builds (and thereby
/// validates) the SOAP command and resolves the lock.
#[tauri::command]
async fn wow_gm_summon_native(
    player: String,
    entry: i32,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let cmd = dml_wow::soap_cmds::gm_summon_cmd(&player, entry)?;
    let lock = state.soap_lock.clone();
    tauri::async_runtime::spawn_blocking(move || dml_wow::soap_cmds::gm_summon(player, entry, cmd, lock))
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

/// NATIVE-MODE `party add` — see [`dml_wow::party::party_add`]. The wrapper
/// builds (and thereby validates) the SOAP command and live-checks `--spec`
/// BEFORE ever touching the DB/SOAP, same ordering as the oracle.
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
    tauri::async_runtime::spawn_blocking(move || dml_wow::party::party_add(player, cmd, spec, lock))
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
    local: Option<String>,
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
    // Internet-play LAN fix: `--local <lan-ip>` also points realmlist's
    // localAddress at this host, so players inside the house aren't routed
    // out to the public address (which needs router NAT hairpinning). Always
    // a private/loopback IPv4 -- it is this PC's own address.
    let local_arg = match local {
        Some(l) => {
            if !validate_ip(&l) {
                return Err(bad_arg(format!("invalid IPv4 address: {l:?}")));
            }
            if !is_loopback_or_private(&l) {
                return Err(bad_arg(format!("not a private LAN address: {l:?}")));
            }
            Some(l)
        }
        None => None,
    };
    let mut args: Vec<String> = vec!["lan".into(), LAN_TITLE.into()];
    if inet {
        args.push("--internet".into());
    }
    if let Some(l) = local_arg {
        args.push("--local".into());
        args.push(l);
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

/// NATIVE-MODE `wow lan on/off/status/refresh` -- see
/// [`dml_wow::lan::validate_lan_request`] (the input gate, which also
/// documents the text-vs-typed-error split) and [`dml_wow::lan::lan_action`].
/// AC-ONLY BY DECISION: native mode only ever drives the single fixed title
/// `LAN_TITLE`; WSL keeps handling every title (including MaNGOS/Tortoise
/// ones) via `wow_lan` above. Native mode only.
#[tauri::command]
async fn wow_lan_native(
    action: String,
    ip: Option<String>,
    internet: Option<bool>,
    local: Option<String>,
) -> Result<String, CmdError> {
    require_native_backend()?;
    let (inet, ip_arg) = dml_wow::lan::validate_lan_request(&action, ip, internet.unwrap_or(false))?;
    let local_ip = dml_wow::lan::validate_lan_local(local)?;
    tauri::async_runtime::spawn_blocking(move || dml_wow::lan::lan_action(&action, ip_arg, inet, local_ip))
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
    /// The PENDING login URL, when tailscaled is holding one (`NeedsLogin`).
    ///
    /// This is the field that recovers the failure found live on a clean VM
    /// (2026-07-29): `tailscale up` can time out BEFORE the control plane
    /// returns a URL — measured at 30s there — while the daemon goes on to
    /// receive it and keep it. Reading it back turns a dead-end timeout into a
    /// clickable link.
    auth_url: Option<String>,
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
    // Empty string filtered out: tailscaled emits `"AuthURL": ""` once the
    // login has completed, and an empty URL is not a URL.
    let auth_url = v
        .get("AuthURL")
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| s.starts_with("http"))
        .map(str::to_string);
    TsStatusFields { backend_state, ip, auth_url }
}

/// Default seconds to let `tailscale up` wait for the login to be answered.
///
/// 45, and MEASURED rather than guessed: on a clean 2-vCPU Windows VM the
/// Tailscale control plane took **30 seconds** to hand back the auth URL
/// (tailscaled journal, 2026-07-29 — `RegisterReq` at 22:37:52, `AuthURL is …`
/// at 22:38:22). The previous hardcoded `--timeout=8s` therefore gave up 22
/// seconds before the URL existed, and the user got a bare "timeout waiting for
/// Tailscale service to enter a Running state" instead of the link that would
/// have let them finish the login on any device.
const TS_UP_TIMEOUT_DEFAULT_SECS: u64 = 45;

/// Extra wall-clock the OUTER process bound gets over the inner `--timeout`.
/// It must be strictly positive, or our own kill would land first and defeat
/// tailscale's own (gentler, URL-printing) timeout — which is how the 8s inner
/// vs 15s outer pair used to behave the moment anyone raised the inner one.
const TS_UP_OUTER_SLACK_SECS: u64 = 15;

/// Largest login wait an override may ask for. Two reasons, and the first one is
/// a real bug this exists to prevent: without a ceiling, `DML_TS_UP_TIMEOUT`
/// could parse to a value near `u64::MAX`, and `secs + TS_UP_OUTER_SLACK_SECS`
/// would then PANIC in debug or wrap in release — inverting the exact
/// outer-outlives-inner invariant the slack is there to guarantee (found by an
/// adversarial review, 2026-07-29). The second reason is plain sense: ten
/// minutes is already far past the ~30s the control plane actually takes, and a
/// larger number is a typo, not an intention.
const TS_UP_TIMEOUT_MAX_SECS: u64 = 600;

/// Parse a Go-style short duration (`45s`, `2m`, or bare seconds) into seconds.
/// `None` for anything else — the caller then keeps the default rather than
/// risking an outer bound shorter than the inner one.
fn parse_short_duration_secs(raw: &str) -> Option<u64> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    let (digits, mult) = match s.strip_suffix('s') {
        Some(d) => (d, 1),
        None => match s.strip_suffix('m') {
            Some(d) => (d, 60),
            None => (s, 1),
        },
    };
    let n: u64 = digits.trim().parse().ok()?;
    if n == 0 {
        return None;
    }
    n.checked_mul(mult)
}

/// The `--timeout=` value to hand `tailscale up`, and the outer process bound.
///
/// `DML_TS_UP_TIMEOUT` is the override — the SAME seam name and the same 45s
/// default as the WSL arm (`cli/src/90-main.sh`), so the two surfaces cannot
/// drift into different behaviour for the same user action.
fn ts_up_timeout() -> (String, std::time::Duration) {
    let secs = std::env::var("DML_TS_UP_TIMEOUT")
        .ok()
        .as_deref()
        .and_then(parse_short_duration_secs)
        // The ceiling is what makes the addition below infallible. Without it an
        // override near u64::MAX panics in debug and WRAPS in release, leaving
        // the outer bound below the inner timeout -- inverting the one invariant
        // this function exists to hold.
        .filter(|s| *s <= TS_UP_TIMEOUT_MAX_SECS)
        .unwrap_or(TS_UP_TIMEOUT_DEFAULT_SECS);
    (
        format!("--timeout={secs}s"),
        std::time::Duration::from_secs(secs + TS_UP_OUTER_SLACK_SECS),
    )
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
    let (timeout_flag, outer) = ts_up_timeout();
    let (_ok, raw) = run_bounded(exe.as_os_str(), &["up", &timeout_flag], outer)
        .unwrap_or_else(|| (false, String::new()));
    // SECOND chance at the URL: `up` may have given up before the control plane
    // answered, while tailscaled went on to receive the URL and keep it. That is
    // the live-found failure, and recovering it costs one bounded status read.
    let auth_url = extract_tailscale_auth_url(&raw).or_else(|| {
        run_bounded(exe.as_os_str(), &["status", "--json"], std::time::Duration::from_secs(5))
            .and_then(|(_, out)| parse_tailscale_status_json(&out).auth_url)
    });
    let ip = run_bounded(exe.as_os_str(), &["ip", "-4"], std::time::Duration::from_secs(5))
        .and_then(|(_, out)| first_tailnet_ip(&out));
    let connected = ip.is_some() && auth_url.is_none();
    if !connected && auth_url.is_none() {
        let tail = tail_str(&raw, 400);
        // Name the knob rather than implying something is broken: by this point
        // the usual cause is simply that the control server was slower than the
        // wait, and the wait is adjustable.
        let waited = format!(
            " (Waited {}; raise DML_TS_UP_TIMEOUT to wait longer.)",
            timeout_flag.trim_start_matches("--timeout=")
        );
        return Err(CmdError {
            code: "TAILSCALE_UP_FAILED".into(),
            message: "Could not start Tailscale login".into(),
            hint: if tail.is_empty() {
                format!("Try Log in again -- the Tailscale control server can take half a minute to answer.{waited}")
            } else {
                format!("{tail}{waited}")
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
/// plumbing — locating and driving the Tailscale app's `tailscale.exe` and
/// parsing `tailscale status --json` — with no
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
    // Deliberately NOT `games_dir_from_env()`: this chain carries the
    // native-mode default `%USERPROFILE%\dml-native`, which the core resolver
    // has no business knowing, and which the core's Windows answer (`.`) would
    // silently replace. So it takes the OVERRIDE and keeps its own tail.
    let base = dml_core::compose::games_dir_override()
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
/// Read Docker Desktop's "open the dashboard on startup" setting.
///
/// Reports Docker's OWN state rather than a DML preference mirroring it: this
/// key belongs to Docker Desktop and the user may change it there, so caching
/// our own copy would let the toggle lie. See `dml_core::dashboard`.
#[tauri::command]
fn docker_dashboard_get() -> Result<serde_json::Value, CmdError> {
    let st = dml_core::dashboard::get().map_err(|e| CmdError {
        code: "DOCKER_SETTINGS".into(),
        message: e,
        hint: "Open Docker Desktop > Settings > General and set it there instead.".into(),
    })?;
    serde_json::to_value(st).map_err(|e| CmdError {
        code: "DOCKER_SETTINGS".into(),
        message: e.to_string(),
        hint: String::new(),
    })
}

/// Write that setting. Preserves every other key and writes atomically — this
/// is the user's Docker config, shared by every container they run, not ours.
#[tauri::command]
fn docker_dashboard_set(disabled: bool) -> Result<serde_json::Value, CmdError> {
    dml_core::dashboard::set(disabled).map_err(|e| CmdError {
        code: "DOCKER_SETTINGS".into(),
        message: e,
        hint: "Open Docker Desktop > Settings > General and set it there instead.".into(),
    })?;
    docker_dashboard_get()
}

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

/// Incident follow-up 1 (2026-07-21): restart the Docker DAEMON inside
/// dml-arch (`dml wow docker-restart`). The WSL-mode counterpart of
/// [`start_docker_desktop`] above — same user problem (the engine is wedged or
/// dead), completely different machinery, which is why they are two commands
/// rather than one with a branch.
///
/// WSL-mode ONLY, and the guard is not decoration: native mode has no distro to
/// shell into, so without it a stale webview would spawn `wsl.exe -d dml-arch`
/// against a distro the user may not even have installed. The Tools card
/// already hides itself in native mode; this is the backstop.
///
/// A single captured envelope, not a stream — the CLI arm's own bounded wait
/// for dockerd is what takes the time, and there is nothing to narrate while it
/// polls.
///
/// Deliberately does NOT do `restart_wsl`'s graceful-stop-first dance. That
/// exists because `wsl --shutdown` is a scheduled hard kill of a HEALTHY stack;
/// this command is reached when Docker is already wedged, so a `games stop`
/// would be issued through the very daemon that stopped answering and would
/// stall for its full timeout before the fix could even start. The typed
/// confirmation on the Tools card owns the blast radius instead.
#[tauri::command]
async fn wow_docker_restart(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    require_wsl_backend()?;
    run_json_cmd(state, vec!["wow".into(), "docker-restart".into()]).await
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
    if let Some(g) = dml_core::compose::games_dir_override() {
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

/// NATIVE-MODE title install: [`dml_wow::install_native`]'s staged, resumable
/// engine, streamed over the same `Channel` the rest of the terminal uses.
///
/// This is the command that makes the native install REACHABLE. The engine has
/// been proven end-to-end on real hardware (2026-07-31, 8/8 stages, 21m18s) but
/// until now only by running the binary from a terminal, which is not a product.
///
/// Three things it deliberately does NOT do:
///
/// * **No bash mirror, and no fallback to one.** The six title installers are
///   Linux scripts (`sudo -v`, pacman/apt, `systemctl`), so bash's own
///   `_installers_supported` refuses on Windows. Native install is native-only
///   BY DESIGN — see `docs/cli-contract.md`.
/// * **No separate process.** The engine runs in `spawn_blocking` and spawns
///   `git`/`docker` itself, which is why the busy slot is
///   [`InstallSlot::Native`] rather than a `Running` with a pid.
/// * **No exit event.** The WSL passthrough is a raw pty and emits
///   `chunk`/`exit`; this engine speaks the project's NDJSON vocabulary, so the
///   terminal's existing `done`/`error` handling ends the run. Mixing the two
///   would give the frontend two ways to learn the same thing.
///
/// Resume is not a parameter: rerunning the same id IS the resume, because the
/// engine reads `.dml-install.json` from the title dir and continues at the
/// first unfinished stage.
#[tauri::command]
async fn games_install_native(
    id: String,
    allow_underspec: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    {
        let mut guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
        *guard = Some(InstallSlot::Native);
    }
    let slot = state.install.clone();
    let ch = on_event.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut opts = dml_wow::install_native::InstallOpts::new(
            id,
            dml_core::compose::games_dir_from_env(),
        );
        opts.allow_underspec = allow_underspec.unwrap_or(false);
        dml_wow::install_native::install_native_stream(&opts, |v| {
            let _ = ch.send(v);
        })
    })
    .await;
    // Release the slot whatever happened, INCLUDING a panic inside the engine.
    // A slot left held would make every later install fail with BUSY and no
    // running install to cancel — recoverable only by restarting the launcher.
    *slot.lock().unwrap() = None;
    result.map_err(|e| CmdError {
        code: "INTERNAL".into(),
        message: e.to_string(),
        hint: String::new(),
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Wrath Unbound add-on (native engine)
// ---------------------------------------------------------------------------
//
// These REPLACE the `tool_install("unbound")` route on the native backend.
// That route curl-downloads the upstream bash installer and runs it under Git
// Bash, where its `IS_WSL2` probe is false and its auto-detection therefore
// searches Linux home directories for a server that lives at a Windows path.
// The user met it as "Could not find a Dad's MMO Lab WotLK Playerbots install
// automatically" followed by a prompt no GUI can answer (2026-08-02).
//
// The engine takes the title dir as a parameter instead of searching for one,
// and refuses rather than prompting — which is what makes it drivable from a
// button at all. The WSL route is untouched and still correct there.

/// Install (or resume) the add-on. Streams the engine's NDJSON events.
///
/// Holds the SAME global install slot as `games_install_native`: this is a
/// 30–90 minute rebuild, and letting a title install start underneath it would
/// have two engines composing the same stack.
#[tauri::command]
async fn wow_unbound_install(
    accept_data_changes: bool,
    repair: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    {
        let mut guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
        *guard = Some(InstallSlot::Native);
    }
    let slot = state.install.clone();
    let ch = on_event.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut opts =
            dml_wow::unbound::UnboundOpts::new(dml_core::compose::games_dir_from_env());
        // Consent is COLLECTED BY THE CALLER and merely carried here. The
        // engine refuses without it and its refusal enumerates the deletes,
        // so a frontend that forgot to ask gets a readable error rather than
        // a silent data change.
        opts.accept_data_changes = accept_data_changes;
        opts.repair = repair.unwrap_or(false);
        dml_wow::unbound::unbound_install_stream(&opts, |v| {
            let _ = ch.send(v);
        })
    })
    .await;
    *slot.lock().unwrap() = None;
    result.map_err(|e| CmdError {
        code: "INTERNAL".into(),
        message: e.to_string(),
        hint: String::new(),
    })?;
    Ok(())
}

/// Remove the add-on and rebuild back to stock. Same slot, same streaming.
#[tauri::command]
async fn wow_unbound_uninstall(
    accept_data_changes: bool,
    force: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    {
        let mut guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
        *guard = Some(InstallSlot::Native);
    }
    let slot = state.install.clone();
    let ch = on_event.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut opts =
            dml_wow::unbound::UnboundOpts::new(dml_core::compose::games_dir_from_env());
        opts.accept_data_changes = accept_data_changes;
        opts.force = force.unwrap_or(false);
        dml_wow::unbound::unbound_uninstall_stream(&opts, |v| {
            let _ = ch.send(v);
        })
    })
    .await;
    *slot.lock().unwrap() = None;
    result.map_err(|e| CmdError {
        code: "INTERNAL".into(),
        message: e.to_string(),
        hint: String::new(),
    })?;
    Ok(())
}

/// Install the CLIENT addons into the saved WoW client folder.
///
/// The path is resolved HERE from `~/.dml/client-path`, never supplied by the
/// webview — same rule as `save_text_file`: a compromised webview must not be
/// able to name a directory to write 43 files into.
///
/// `unbound install` already does this automatically at the end of a run. This
/// exists for the case that does not deserve a 90-minute rebuild: a client that
/// lost its addons, or one configured after the server install.
#[tauri::command]
async fn wow_unbound_addons_install(
    _state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let cp = dml_wow::clientpath::read_client_path();
    let path = cp.get("path").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    if path.is_empty() {
        return Err(CmdError {
            code: "NOT_FOUND".into(),
            message: "No WoW client folder is configured.".into(),
            hint: "Set your client folder on the Settings page first.".into(),
        });
    }
    if !cp.get("valid").and_then(|v| v.as_bool()).unwrap_or(false) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("{path} does not look like a WoW client folder."),
            hint: "It should contain Wow.exe or an Interface directory.".into(),
        });
    }
    let done = dml_wow::unbound_addons::install_addons(std::path::Path::new(&path)).map_err(
        |e| CmdError {
            code: "WRITE_FAILED".into(),
            message: e,
            hint: "Check the folder is writable — close WoW if it is running.".into(),
        },
    )?;
    Ok(serde_json::json!({
        "addons_dir": done.addons_dir, "files": done.files, "addons": done.addons,
    }))
}

/// Export the addons to a folder the user picks, for handing to other players.
///
/// Sync on purpose, like `save_text_file`: Tauri runs it off the main thread,
/// which `blocking_pick_folder` requires. Returns `null` when the user cancels
/// — a cancel is not an error and must not surface as one.
#[tauri::command]
fn wow_unbound_addons_export(app: tauri::AppHandle) -> Result<Option<serde_json::Value>, String> {
    use tauri_plugin_dialog::DialogExt;
    let Some(picked) = app.dialog().file().blocking_pick_folder() else {
        return Ok(None);
    };
    let dir = picked.into_path().map_err(|e| e.to_string())?;
    let done = dml_wow::unbound_addons::export_addons(&dir)?;
    Ok(Some(serde_json::json!({
        "dir": done.addons_dir, "files": done.files, "addons": done.addons,
    })))
}

/// On-disk evidence about the add-on: installed, part-way, or absent.
///
/// Reads the state file and the six patched core files. No docker and no
/// database, so the card can label its buttons honestly while the server is
/// stopped — which is most of the time a user is deciding whether to install.
#[tauri::command]
async fn wow_unbound_status(_state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let games = dml_core::compose::games_dir_from_env();
    let st = dml_wow::unbound::unbound_status(&games, dml_wow::unbound::DEFAULT_TITLE_ID);
    serde_json::to_value(st).map_err(|e| CmdError {
        code: "INTERNAL".into(),
        message: e.to_string(),
        hint: String::new(),
    })
}

/// Import a WSL export into a native stack (Task 10).
///
/// Shares [`InstallSlot::Native`] with `games_install_native` rather than
/// having a slot of its own, and that is deliberate: both drive the SAME
/// docker engine towards the SAME engine-global `ac-*` container names, so
/// running them at once is never something a user meant. One busy signal, one
/// place for the UI to check.
///
/// Resume is not a parameter here either — rerunning the same id IS the
/// resume, because the engine reads `.dml-migrate.json` from the title dir.
#[tauri::command]
async fn wow_migrate_import(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    require_native_backend()?;
    {
        let mut guard = state.install.lock().unwrap();
        if guard.is_some() {
            return Err(CmdError {
                code: "BUSY".into(),
                message: "An install is already running".into(),
                hint: "Finish or cancel it first.".into(),
            });
        }
        *guard = Some(InstallSlot::Native);
    }
    let slot = state.install.clone();
    let ch = on_event.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let opts = dml_wow::migrate::MigrateOpts {
            id,
            games_dir: dml_core::compose::games_dir_from_env(),
            db_password: std::env::var("DB_ROOT_PASSWORD")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "password".to_string()),
            ..Default::default()
        };
        dml_wow::migrate::migrate_import_stream(&opts, |v| {
            let _ = ch.send(v);
        })
    })
    .await;
    // Release the slot whatever happened, INCLUDING a panic inside the engine.
    *slot.lock().unwrap() = None;
    result.map_err(|e| CmdError {
        code: "INTERNAL".into(),
        message: e.to_string(),
        hint: String::new(),
    })?;
    Ok(())
}

/// Is there an export in this title's folder, is it complete, and how far did a
/// previous import get?
///
/// Read-only and cheap (it stats files and reads one small JSON), so the page
/// can call it on mount. The point is that the button stops lying: an "Import"
/// that would actually resume, or that is about to fail because the payload is
/// half there, should say so BEFORE it is pressed.
#[tauri::command]
fn wow_migrate_status(id: String) -> Result<serde_json::Value, CmdError> {
    require_native_backend()?;
    let opts = dml_wow::migrate::MigrateOpts {
        id,
        games_dir: dml_core::compose::games_dir_from_env(),
        ..Default::default()
    };
    serde_json::to_value(dml_wow::migrate::status(&opts)).map_err(|e| CmdError {
        code: "INTERNAL".into(),
        message: e.to_string(),
        hint: String::new(),
    })
}

/// Is there a half-finished native install in this title's directory, and where
/// did it get to?
///
/// Exists so the Library button can stop lying. Re-running the engine on a
/// partly-installed title continues from the first unfinished stage rather than
/// starting over, so a button labelled "Install" describes something the app is
/// not about to do — and the difference matters most to the user who just lost
/// a two-hour build and needs to know they are not paying for it twice.
///
/// Reads the state file only. Deliberately NOT a verdict on whether the install
/// is healthy: the engine re-checks every stage against the disk when it runs
/// (a recorded stage whose evidence is gone is redone), and duplicating that
/// judgement here would give the UI a second opinion that can disagree with the
/// engine's.
#[tauri::command]
async fn games_install_native_state(
    id: String,
    _state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    if !dml_wow::install_native::valid_title_id(&id) {
        return Err(CmdError {
            code: "BAD_ID".into(),
            message: format!("{id:?} is not a valid title name."),
            hint: "Use letters, digits, '.', '_' and '-' only.".into(),
        });
    }
    let title_dir = dml_core::compose::games_dir_from_env().join(&id);
    let st = dml_wow::install_native::load_state(&title_dir);
    // `next_stage` is None when every stage is recorded — an install that is
    // finished, not one that is resumable. Reporting `in_progress` for it would
    // put "Resume" on a title that has nothing left to do.
    let next = st.as_ref().and_then(dml_wow::install_native::next_stage);
    Ok(serde_json::json!({
        "in_progress": next.is_some(),
        "next_stage": next.map(|s| s.name()),
        "last_error": st.as_ref().and_then(|s| s.last_error.clone()),
    }))
}

/// The guided post-install step: what to type, where, and the warning that
/// keeps a user from stopping their own server on the way out.
///
/// Read-only: it composes strings from `dml_wow::soap_bootstrap` and asks the
/// auth database exactly one question (is `dmlsoap` taken?) to decide which
/// name to prefill. Kept as a command rather than duplicated in TypeScript
/// so the console commands, the clipboard copy and the Rust tests all come from
/// one source: a mistyped account is indistinguishable from a broken SOAP
/// setup from the outside, and two copies of these two lines WILL drift.
#[tauri::command]
async fn wow_soap_bootstrap_info(
    user: Option<String>,
    pass: Option<String>,
) -> Result<serde_json::Value, CmdError> {
    use dml_wow::soap_bootstrap as sb;
    // Which name to PREFILL. Not always `dmlsoap`: this card renders after
    // automatic setup gave up, and two of the four ways it gives up mean
    // `dmlsoap` is already taken -- so prefilling it would hand the user a name
    // whose "Create the account" click is certain to come back "already
    // exists", from the one screen that exists to unstick them.
    //
    // DEGRADES DELIBERATELY: a database we cannot reach keeps the documented
    // default. This command is read-only and instant, and a card that fails to
    // load is worse than one carrying a stale default the user can retype.
    let default_user = tauri::async_runtime::spawn_blocking(|| {
        let cfg = dml_wow::db::DbConfig::from_env();
        match dml_wow::account_write::account_exists(&cfg, sb::DEFAULT_SOAP_USER) {
            Ok(true) => dml_wow::soap_autosetup::fallback_user(
                &dml_wow::soap_autosetup::random_hex6(),
            ),
            _ => sb::DEFAULT_SOAP_USER.to_string(),
        }
    })
    .await
    .unwrap_or_else(|_| sb::DEFAULT_SOAP_USER.to_string());
    let user = user.filter(|u| !u.trim().is_empty()).unwrap_or_else(|| default_user.clone());
    // A placeholder, never a generated secret: the password shown here is the
    // one the user is about to TYPE, so inventing one would mean the app knows
    // a credential the user does not.
    let pass = pass.unwrap_or_default();
    let project = dml_wow::composegen::project_name_for(
        &dml_core::compose::games_dir_from_env().join("wow-server-playerbots"),
    );
    Ok(serde_json::json!({
        "user": user,
        "commands": sb::console_commands(&user, &pass),
        "attach_hint": sb::attach_hint(&project),
        "detach_warning": sb::DETACH_WARNING,
        "default_user": default_user,
    }))
}

/// Does SOAP actually work right now, with the credentials the rest of the app
/// uses?
///
/// Drives whether the guided account step is OFFERED, and it asks by doing a
/// real round-trip rather than by looking for `~/.dml/soap.env`. That
/// distinction is not academic: a leftover soap.env from a DIFFERENT server
/// carries a real account name and a real password for a realm that no longer
/// exists, so a presence check reports "configured" while every SOAP feature
/// fails. Exactly that file was sitting on the author's machine.
///
/// Bounded by the SOAP client's own connect/read timeouts and serialized on the
/// same lock every other native SOAP call takes.
#[tauri::command]
async fn wow_soap_status(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    use dml_wow::soap_bootstrap as sb;
    let soap_lock = state.soap_lock.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        sb::soap_status_with(|cfg, cmd| {
            let _guard = soap_lock.lock();
            dml_wow::soap::exec(cfg, cmd)
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    let status = match &outcome {
        sb::VerifyOutcome::Ok => "ok",
        sb::VerifyOutcome::Rejected(_) => "rejected",
        sb::VerifyOutcome::Unreachable(_) => "unreachable",
    };
    Ok(serde_json::json!({
        "status": status,
        // The UI must not re-derive this. "Offer the setup" is true ONLY for a
        // server that answers and refuses us -- never for one that is simply
        // down, because the setup asks the user to type into a worldserver
        // console that does not exist then.
        "needs_bootstrap": sb::needs_bootstrap(&outcome),
    }))
}

/// Create the SOAP account directly, then prove it works.
///
/// The one-click replacement for the worldserver-console step. It is a WRITE
/// into `acore_auth` -- the third sanctioned MySQL write, user-approved
/// 2026-08-01 -- and `dml_wow::account_write` documents why SOAP could not do
/// this itself: SOAP needs the very account it would be creating.
///
/// VERIFIES BEFORE IT SAVES, exactly like the manual path: the credentials are
/// only written to `~/.dml/soap.env` after a real round-trip succeeds with
/// them. That matters more here, not less -- a mistake in the SRP6 produces a
/// verifier that is perfectly well-formed and simply never authenticates, so
/// "the INSERT returned Ok" proves nothing at all.
///
/// Never returns `Err` for an unhappy server: a refusal (name taken, bad
/// password, schema we do not understand) comes back as a verdict the card can
/// render alongside the manual instructions, which stay on screen as the
/// fallback.
#[tauri::command]
async fn wow_soap_account_create(
    user: String,
    pass: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    use dml_wow::soap_bootstrap as sb;
    let home = dml_core::util::home_dir().ok_or_else(|| CmdError {
        code: "NO_HOME".into(),
        message: "Could not find your user folder, so the credentials could not be saved.".into(),
        hint: String::new(),
    })?;
    let url = dml_wow::soap::SoapConfig::load().url;
    let soap_lock = state.soap_lock.clone();

    let result = tauri::async_runtime::spawn_blocking(move || {
        let cfg = dml_wow::db::DbConfig::from_env();
        if let Err(e) = dml_wow::account_write::create_gm_account(&cfg, &user, &pass) {
            return Err(e);
        }
        // Same verify-then-save routine the manual path uses, so there is one
        // definition of "done" rather than two that can disagree.
        sb::bootstrap_verify_with(&home, &url, &user, &pass, |c, cmd| {
            let _guard = soap_lock.lock();
            dml_wow::soap::exec(c, cmd)
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;

    match result {
        Ok((outcome, path)) => {
            let (status, detail) = match &outcome {
                sb::VerifyOutcome::Ok => ("ok", String::new()),
                sb::VerifyOutcome::Rejected(m) => ("rejected", m.clone()),
                sb::VerifyOutcome::Unreachable(m) => ("unreachable", m.clone()),
            };
            Ok(serde_json::json!({
                "status": status,
                "detail": detail,
                "saved_to": path.map(|p| p.display().to_string()),
            }))
        }
        // A refusal is an ANSWER about the server, not a failure of the command,
        // so it renders in the card next to the manual steps rather than as a
        // dead-end error toast.
        Err(e) => Ok(serde_json::json!({
            "status": "refused",
            "detail": format!("{}{}", e.message, if e.hint.is_empty() { String::new() } else { format!(" {}", e.hint) }),
            "saved_to": serde_json::Value::Null,
        })),
    }
}

/// Set SOAP up by itself, once per launcher run.
///
/// The fully automatic replacement for the account card: the user types
/// nothing, clicks nothing, and — when this succeeds — never learns the step
/// existed. `dml_wow::soap_autosetup` holds the decision tree; this function is
/// only the wiring between its seams and the real database, SOAP client and
/// filesystem.
///
/// Three properties are load-bearing and all three live in the seams below:
///
/// * **It asks before it acts.** The status comes from `soap_status_with`, the
///   same classifier `wow_soap_status` uses, so a `Fault` is not mistaken for
///   an auth failure and a world server that is merely still booting is not
///   mistaken for a broken account. A non-`Rejected` verdict returns
///   `not_needed` having opened no DB connection at all.
/// * **It proves itself before it saves.** The verify seam is
///   `bootstrap_verify_with`, which writes `~/.dml/soap.env` only after a real
///   round-trip. A mistake in the SRP6 produces a verifier that is perfectly
///   well-formed and simply never authenticates, so "the INSERT returned Ok"
///   proves nothing.
/// * **It stops.** The `AppState` latch means a poll that ticks every few
///   seconds cannot turn into one INSERT per tick.
///
/// Never returns `Err` for an unhappy server — a refusal is an answer about the
/// machine and comes back as a verdict the banner or the fallback card renders.
#[tauri::command]
async fn wow_soap_autosetup(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    use dml_wow::soap_autosetup as auto;
    use dml_wow::soap_bootstrap as sb;

    let home = dml_core::util::home_dir().ok_or_else(|| CmdError {
        code: "NO_HOME".into(),
        message: "Could not find your user folder, so the credentials could not be saved.".into(),
        hint: String::new(),
    })?;
    let url = dml_wow::soap::SoapConfig::load().url;
    let soap_lock = state.soap_lock.clone();
    let latch = state.soap_autosetup.clone();

    let outcome = tauri::async_runtime::spawn_blocking(move || {
        // Cheap exit first: a concluded run must not even ask the server. It
        // still ANSWERS, through the same derivation `advance_with` uses -- a
        // reloaded webview has forgotten how this run went, and a contentless
        // "already concluded" would leave it unable to render the fallback card
        // on the exact path where the card is the only remaining way in.
        {
            let g = latch.lock().unwrap();
            if let auto::AutoSetup::Done(c) = &*g {
                return auto::concluded_outcome(c);
            }
        }

        let status = sb::soap_status_with(|cfg, cmd| {
            let _guard = soap_lock.lock();
            dml_wow::soap::exec(cfg, cmd)
        });

        let cfg = dml_wow::db::DbConfig::from_env();
        let mut g = latch.lock().unwrap();
        let state_now = g.clone();
        let (next, outcome) = auto::advance_with(
            state_now,
            &status,
            |u| dml_wow::account_write::account_exists(&cfg, u),
            |p| dml_wow::account_write::account_family_exists(&cfg, p),
            |u, p| dml_wow::account_write::create_gm_account(&cfg, u, p),
            |u, p| {
                // This is the writer of ~/.dml/soap.env, and it writes only
                // after the round-trip below succeeds.
                sb::bootstrap_verify_with(&home, &url, u, p, |c, cmd| {
                    let _guard = soap_lock.lock();
                    dml_wow::soap::exec(c, cmd)
                })
                .map(|(v, _path)| v)
            },
            auto::random_hex6,
            auto::generate_password,
        );
        *g = next;
        outcome
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;

    Ok(serde_json::json!({
        "status": auto::outcome_status(&outcome),
        "user": match &outcome {
            auto::AutoOutcome::Created { user } => serde_json::Value::String(user.clone()),
            _ => serde_json::Value::Null,
        },
        "reason": match &outcome {
            auto::AutoOutcome::GaveUp { reason } => serde_json::Value::String(reason.clone()),
            _ => serde_json::Value::Null,
        },
    }))
}

/// Which account the launcher uses, and — only when asked — its password.
///
/// The launcher generates that password, so it is the one credential the app
/// knows and the user does not. This is where they can read it back: a
/// generated secret with no way to see it is a secret the user cannot use when
/// they need it (a second tool, a support question, a manual SOAP call).
///
/// `configured` says whether anyone SUPPLIED these credentials (`DML_SOAP_*` or
/// `~/.dml/soap.env`) or whether they fell through to the compiled-in
/// `admin`/`admin`. It is resolved by `SoapConfig::load_with_provenance` and
/// passed through untouched, because that resolver is the only thing that knows:
/// the frontend previously guessed by comparing `user`/`pass` against `"admin"`,
/// which reads a server whose SOAP account really is named `admin` as
/// unconfigured, and a fresh install with no account at all as configured.
///
/// Read-only. There is no write path here.
#[tauri::command]
async fn wow_soap_credentials(reveal: bool) -> Result<serde_json::Value, CmdError> {
    let (cfg, is_configured) = dml_wow::soap::SoapConfig::load_with_provenance();
    let (user, url, pass, configured) = dml_wow::soap_autosetup::credentials_payload(
        &cfg.user,
        &cfg.pass,
        &cfg.url,
        is_configured,
        reveal,
    );
    Ok(serde_json::json!({ "user": user, "url": url, "pass": pass, "configured": configured }))
}

/// Prove the account works, and ONLY then remember it.
///
/// The ordering is the entire feature. Writing `~/.dml/soap.env` first and
/// hoping would leave a plausible-looking credentials file that does not work —
/// exactly the "the app thinks it is configured while every SOAP button is
/// dead" state this step exists to eliminate, and harder to diagnose than
/// having no file at all.
///
/// Never returns `Err` for a server that answered honestly: rejected
/// credentials and an unreachable port are both `Ok` with a verdict, because
/// they are answers about the machine rather than failures of this command.
/// `Err` is reserved for a malformed request or a disk that would not take the
/// file.
#[tauri::command]
async fn wow_soap_bootstrap_verify(
    user: String,
    pass: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    use dml_wow::soap_bootstrap as sb;
    let home = dml_core::util::home_dir().ok_or_else(|| CmdError {
        code: "NO_HOME".into(),
        message: "Could not find your user folder, so the credentials could not be saved.".into(),
        hint: String::new(),
    })?;
    // The URL comes from the SAME resolver every other SOAP call uses, so a
    // verification cannot pass against an address the rest of the app will not
    // use.
    let url = dml_wow::soap::SoapConfig::load().url;
    let soap_lock = state.soap_lock.clone();
    let (outcome, path) = tauri::async_runtime::spawn_blocking(move || {
        sb::bootstrap_verify_with(&home, &url, &user, &pass, |cfg, cmd| {
            // Serialized like every other native SOAP call: the worldserver's
            // SOAP listener runs on the single world thread.
            let _guard = soap_lock.lock();
            dml_wow::soap::exec(cfg, cmd)
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })??;

    let (status, detail) = match &outcome {
        sb::VerifyOutcome::Ok => ("ok", String::new()),
        sb::VerifyOutcome::Rejected(m) => ("rejected", m.clone()),
        sb::VerifyOutcome::Unreachable(m) => ("unreachable", m.clone()),
    };
    Ok(serde_json::json!({
        "status": status,
        "detail": detail,
        "saved_to": path.map(|p| p.display().to_string()),
    }))
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
        // A native install answers no questions, so there is no stdin to write
        // to. Saying so beats a silent success the caller would read as "sent".
        Some(InstallSlot::Native) => Err(CmdError {
            code: "NOT_INTERACTIVE".into(),
            message: "This install does not ask questions, so there is nothing to answer.".into(),
            hint: String::new(),
        }),
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
            // The native engine's children are OUR children: `taskkill /F /T`
            // on this pid would kill the launcher. Refusing honestly is better
            // than a cancel that closes the app, and the work is resumable —
            // which is the part worth telling the user.
            Some(InstallSlot::Native) => {
                return Err(CmdError {
                    code: "NOT_CANCELLABLE".into(),
                    message: "A native install cannot be cancelled from here yet.".into(),
                    // Says only what is TRUE. The previous wording -- "closing
                    // the launcher stops it" -- was false under the shipped
                    // default: close-to-tray is ON, so the X hides the window
                    // and the build carries on. And "quit from the tray" was
                    // not offered instead, because nothing in this launcher
                    // puts the git/docker children in a job object, so quitting
                    // may orphan them rather than kill them. Promising a stop
                    // we have not verified is exactly the error being fixed, so
                    // the hint promises only what the engine really guarantees.
                    hint: "The build carries on by itself. Nothing is lost: running the install again continues from the last finished step, reusing Docker's build cache."
                        .into(),
                })
            }
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
    // MUST run before any thread exists (Builder::setup spawns one) and
    // before AppState captures backend::selected().
    startup::resolve_and_export();

    // Single instance. Must come before the builder: if another instance is
    // already live we surface ITS window and exit rather than starting a
    // second app that fights over the same server. Only matters now that
    // close-to-tray keeps the first one alive with no window showing.
    let instance_lock = match single_instance::acquire() {
        single_instance::Instance::First(l) => Some(l),
        single_instance::Instance::AlreadyRunning => return,
        single_instance::Instance::PortUnavailable => None,
    };

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
            last_status_push: Arc::new(Mutex::new(None)),
            soap_autosetup: Arc::new(Mutex::new(dml_wow::soap_autosetup::AutoSetup::Idle)),
        })
        .setup(|app| {
            // (Task 8 removed the startup registry prefetch here: the config/
            // tuning/module-catalog registries are now embedded in dml-wow —
            // see `dml_wow::registry` — so there is nothing left to warm.)
            tray::build(app.handle())?;
            if let Some(l) = instance_lock {
                single_instance::serve(l, app.handle().clone());
            }

            // Keep-awake safety net. Engagement is driven by the webview poll
            // loop; a hidden window whose timers get throttled would
            // otherwise hold the sleep block forever. Two minutes is ~17
            // missed 7s polls — long enough never to fight a briefly-busy
            // poll, short enough that a stalled webview cannot keep the PC
            // awake unnoticed.
            let pushes = app.state::<AppState>().last_status_push.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_secs(30));
                let stale = pushes
                    .lock()
                    .ok()
                    .and_then(|t| *t)
                    .map(|t| t.elapsed() > std::time::Duration::from_secs(120))
                    .unwrap_or(false);
                if stale {
                    power::keep_awake(false);
                }
            });
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
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() != tray::MAIN_WINDOW {
                    return;
                }
                // Read the preference fresh rather than caching it: the user
                // can change it in Settings without restarting, and a window
                // close is rare enough that a small file read is free.
                let hide = dml_core::util::dml_home_dir()
                    .map(|h| dml_core::launcher_config::load(&h).close_to_tray)
                    .unwrap_or(true);
                if hide {
                    api.prevent_close();
                    // HIDE, never destroy. The webview must keep running: it
                    // owns the 7s status poll that feeds the tray, and the
                    // auto-shutdown toggle is re-asserted to Rust from its
                    // onMount. Destroying it would silently kill both.
                    let _ = window.hide();
                }
            }
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
            games_install_native,
            wow_migrate_import,
            wow_migrate_status,
            games_install_native_state,
            wow_unbound_install,
            wow_unbound_uninstall,
            wow_unbound_status,
            wow_unbound_addons_install,
            wow_unbound_addons_export,
            wow_soap_bootstrap_info,
            wow_soap_autosetup,
            wow_soap_credentials,
            wow_soap_status,
            wow_soap_account_create,
            wow_soap_bootstrap_verify,
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
            backend_status,
            backend_setup,
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
            docker_dashboard_get,
            docker_dashboard_set,
            wow_docker_restart,
            native_yq_install,
            native_soap_copy,
            native_defender_script,
            save_text_file,
            set_auto_shutdown,
            set_keep_awake,
            set_taskbar_progress,
            realmlist_status,
            realmlist_fix,
            realmlist_lock,
            launcher_config_read,
            launcher_config_write,
            tray_set_status,
            autostart_get,
            autostart_set
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

    // -- native_title_is_usable ---------------------------------------------
    //
    // The bug these pin, found by the user on 2026-08-01: `games catalog`
    // decides `installed` with `[[ -d "$GAMES_DIR/$1" ]]` and the engine
    // creates that directory at stage 3 of 8, so a build that died hours in
    // still presented as an installed, startable server.

    fn title_fixture(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("dml-usable-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_compose(dir: &std::path::Path) {
        std::fs::write(dir.join(dml_wow::composegen::BASE_FILE), "services: {}
").unwrap();
    }

    /// THE FRESH-MACHINE BLOCKER. Nothing creates `%USERPROFILE%\dml-native`
    /// before the first install -- the engine makes it itself -- so a games dir
    /// that is simply ABSENT is the normal state of every new native PC.
    ///
    /// Collapsing that into `None` made the chain answer `Unknown` blocked at
    /// Titles, and the user's very first screen read "the launcher couldn't read
    /// back the list of installed games" (false twice over) behind a "Check
    /// again" button that re-ran the identical failing read forever. The
    /// `no_titles` -> "Open Library" arm was unreachable for exactly the user it
    /// was built for.
    #[test]
    fn an_absent_games_dir_holds_zero_titles_not_an_unknown_number() {
        let missing = std::env::temp_dir().join(format!("dml-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);
        assert!(!missing.exists());
        let prev = std::env::var_os("DML_GAMES_DIR");
        unsafe { std::env::set_var("DML_GAMES_DIR", &missing) };
        let got = native_title_count();
        match prev {
            Some(v) => unsafe { std::env::set_var("DML_GAMES_DIR", v) },
            None => unsafe { std::env::remove_var("DML_GAMES_DIR") },
        }
        assert_eq!(got, Some(0), "a games dir that is not there holds zero titles");
    }

    #[test]
    fn a_directory_without_a_compose_file_is_not_a_title() {
        // `tools/` lives in the games dir alongside real titles.
        let d = title_fixture("no-compose");
        assert!(!native_title_is_usable(&d));
    }

    #[test]
    fn a_title_with_no_state_file_is_usable() {
        // A WSL-route install, or a migrated server, or one predating the
        // engine. Absence of the bookkeeping file must never read as
        // "unfinished" -- that would hide every existing server behind a
        // Resume button.
        let d = title_fixture("no-state");
        write_compose(&d);
        assert!(native_title_is_usable(&d));
    }

    #[test]
    fn a_half_finished_install_is_not_usable_even_with_a_compose_file() {
        // THE BUG. generate-compose is stage 5 of 8, so a build that died in
        // the multi-hour `build` stage leaves a perfectly good compose file.
        use dml_wow::install_native::{InstallState, Stage};
        let d = title_fixture("half");
        write_compose(&d);
        let mut st = InstallState::new("wow-server-playerbots", &dml_wow::composegen::install_id(&d));
        st.mark(Stage::CloneCore);
        st.mark(Stage::CloneModule);
        st.mark(Stage::GenerateCompose);
        dml_wow::install_native::save_state(&d, &st).unwrap();
        assert!(
            !native_title_is_usable(&d),
            "an install that never built an image is not a server the user can start"
        );
    }

    #[test]
    fn a_completed_install_is_usable() {
        use dml_wow::install_native::{InstallState, Stage};
        let d = title_fixture("done");
        write_compose(&d);
        let mut st = InstallState::new("wow-server-playerbots", &dml_wow::composegen::install_id(&d));
        for stage in [
            Stage::CloneCore,
            Stage::CloneModule,
            Stage::GenerateCompose,
            Stage::Build,
            Stage::Up,
            Stage::Ready,
        ] {
            st.mark(stage);
        }
        dml_wow::install_native::save_state(&d, &st).unwrap();
        assert!(native_title_is_usable(&d), "{:?}", st.completed);
    }

    // -- WSL-only backend guard (incident follow-up 1: wow_docker_restart) ---
    // Tested through the pure half so the decision is provable without
    // mutating DML_BACKEND in a threaded test runner (same doctrine as
    // `resolve_tailscale_from_candidates`). The inversion is the whole risk
    // here: `require_native_backend` is one keyword away and would let the
    // command through in exactly the mode that has no distro.

    #[test]
    fn wsl_backend_guard_refuses_native_mode() {
        let err = wsl_backend_guard(true).expect_err("native mode must be refused");
        assert_eq!(err.code, "WRONG_BACKEND");
        // The message has to name the reason a parent can act on -- a bare
        // "wrong backend" on screen means nothing.
        let text = format!("{} {}", err.message, err.hint).to_lowercase();
        assert!(text.contains("native"), "message/hint should name native mode: {text}");
        assert!(text.contains("distro"), "message/hint should say there is no distro: {text}");
    }

    #[test]
    fn wsl_backend_guard_allows_wsl_mode() {
        assert!(wsl_backend_guard(false).is_ok());
    }

    // -- backend_status report shape (SHIP-LIST Phase 4) --------------------
    // This is the wire contract two other lanes build against (the first-run
    // screen and the setup command), so it is pinned here rather than
    // discovered by clicking. Breaking change = renaming/moving any key
    // asserted below.

    fn probes_ready() -> dml_core::setup::Probes {
        dml_core::setup::Probes {
            wsl: dml_core::setup::Tri::Yes,
            distro: dml_core::setup::Tri::Yes,
            cli: dml_core::setup::Tri::Yes,
            cli_version: Some(dml_core::setup::EXPECTED_CLI_VERSION.to_string()),
            titles: Some(2),
            detail: None,
        }
    }

    #[test]
    fn backend_status_report_carries_the_probe_message_through_the_flatten() {
        // SHIP-LIST Phase 4 review, P9. The chain now quotes what wsl.exe
        // actually said on a could-not-tell, but the screen reads it through
        // `#[serde(flatten)]` -- so a key that exists in `BackendStatus` and is
        // swallowed here helps nobody. `detail` must be a sibling of `state`.
        let mut probes = probes_ready();
        probes.wsl = dml_core::setup::Tri::Unknown;
        probes.detail = Some("Wsl/Service/CreateInstance/0x80370102".to_string());
        let report = backend_status_report(
            dml_core::setup::derive("dml-arch", probes),
            crate::payload::resolve_opt(None),
            false,
        );
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["state"], "unknown");
        assert_eq!(v["blocked_at"], "wsl");
        assert_eq!(
            v["detail"], "Wsl/Service/CreateInstance/0x80370102",
            "the retry card has nothing to show: {v}"
        );
    }

    #[test]
    fn backend_status_probes_with_a_budget_that_survives_a_cold_wsl2_boot() {
        // SHIP-LIST Phase 4 review, P4/P8/P12. `backend_status` runs from
        // +page.svelte's onMount, so its FIRST call into the distro is the one
        // that boots the WSL2 VM after a Windows reboot. On a 20s budget that
        // overruns, and an established user's working Home is replaced by
        // "Couldn't check this PC's setup".
        //
        // THIS ASSERTS ON THE CALL SITE, NOT ON A HELPER. The first version
        // read `backend_probe_env()` and checked its budgets -- which proves
        // only that the helper builds the right numbers, never that the
        // command hands them over intact. A verifier set
        // `env.cold_timeout = Duration::from_secs(20)` at the real call site,
        // the precise regression named above, and the test stayed GREEN. So
        // the budget is now observed where it is SPENT: at the spawn, through
        // the same seam `probe_with` gives the chain.
        use dml_core::setup::ProbeOutcome;
        let seen: std::cell::RefCell<Vec<(Vec<String>, std::time::Duration)>> =
            std::cell::RefCell::new(Vec::new());
        let report = backend_status_with(None, false, |program, args, budget| {
            assert_eq!(program, std::ffi::OsStr::new("wsl.exe"), "probes must spawn wsl.exe");
            seen.borrow_mut().push((args.iter().map(|s| s.to_string()).collect(), budget));
            let stdout = if args.first().copied() == Some("--list") {
                format!("{}\r\n", dml_core::runner::DISTRO)
            } else if args.contains(&"version") {
                r#"{"ok":true,"data":{"version":"3.0.0"}}"#.to_string()
            } else {
                r#"{"ok":true,"data":{"games":[]}}"#.to_string()
            };
            ProbeOutcome::Ran { code: Some(0), stdout, stderr: String::new() }
        });
        let seen = seen.into_inner();

        // The chain walked to the end, so the cold call below really is the
        // one the chain made -- not a short-circuit that never reached it.
        assert_eq!(serde_json::to_value(&report).unwrap()["state"], "no_titles");
        assert_eq!(seen.len(), 3, "the whole chain must have run: {seen:?}");

        // ...against this launcher's own distro and user.
        let cold = seen
            .iter()
            .find(|(args, _)| args.contains(&"version".to_string()))
            .expect("the CLI probe never ran");
        assert_eq!(
            cold.0,
            vec!["-d", dml_core::runner::DISTRO, "-u", dml_core::runner::USER, "--", "dml", "version", "--json"]
        );
        // THE ASSERTION: the wall clock the VM-booting call is actually given.
        assert!(
            cold.1 >= std::time::Duration::from_secs(90),
            "the cold call gets {:?}, which a cold WSL2 + systemd boot outruns",
            cold.1
        );
        // And the other half: a host-side call must NOT inherit that budget,
        // or a broken machine takes minutes to say so.
        let warm = seen.first().expect("nothing was probed");
        assert_eq!(warm.0, vec!["--list", "--quiet"]);
        assert!(
            warm.1 <= std::time::Duration::from_secs(30),
            "a wedged host-side wsl.exe must still be reported fast, got {:?}",
            warm.1
        );
    }

    #[test]
    fn backend_status_report_puts_state_at_the_top_level() {
        let report = backend_status_report(
            dml_core::setup::derive("dml-arch", probes_ready()),
            crate::payload::resolve_opt(None),
            false,
        );
        let v = serde_json::to_value(&report).unwrap();
        // Flattened: `state` is a sibling of `payload`, not nested under a
        // `backend` object the UI would have to reach through.
        assert_eq!(v["state"], "ready");
        assert_eq!(v["distro"], "dml-arch");
        assert_eq!(v["expected_cli_version"], dml_core::setup::EXPECTED_CLI_VERSION);
        assert!(v["blocked_at"].is_null());
        assert_eq!(v["probes"]["wsl"], "yes");
        assert_eq!(v["probes"]["titles"], 2);
        assert_eq!(v["backend_mode"], "wsl");
        assert_eq!(v["payload"]["present"], "unknown");
    }

    #[test]
    fn backend_status_report_serializes_states_in_snake_case() {
        let mut probes = probes_ready();
        probes.distro = dml_core::setup::Tri::No;
        let report = backend_status_report(
            dml_core::setup::derive("dml-arch", probes),
            crate::payload::resolve_opt(None),
            false,
        );
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["state"], "no_distro");
    }

    #[test]
    fn backend_status_report_names_the_blocked_step_for_unknown() {
        let mut probes = probes_ready();
        probes.wsl = dml_core::setup::Tri::Unknown;
        let report = backend_status_report(
            dml_core::setup::derive("dml-arch", probes),
            crate::payload::resolve_opt(None),
            false,
        );
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["state"], "unknown");
        assert_eq!(v["blocked_at"], "wsl");
    }

    #[test]
    fn backend_status_report_reports_native_mode() {
        // Without this a native-mode user -- who has no distro by design --
        // would be told to go install WSL.
        let report = backend_status_report(
            dml_core::setup::derive("dml-arch", probes_ready()),
            crate::payload::resolve_opt(None),
            true,
        );
        assert_eq!(serde_json::to_value(&report).unwrap()["backend_mode"], "native");
    }

    #[test]
    fn backend_status_report_carries_the_payload_verdict_through() {
        let payload = crate::payload::PayloadStatus {
            present: dml_core::setup::Tri::No,
            dir: Some("C:/somewhere".into()),
            missing: vec![crate::payload::CLI_SCRIPT.to_string()],
        };
        let report = backend_status_report(
            dml_core::setup::derive("dml-arch", probes_ready()),
            payload,
            false,
        );
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["payload"]["present"], "no");
        assert_eq!(v["payload"]["missing"][0], crate::payload::CLI_SCRIPT);
    }

    // -- the CLI_BAD_OUTPUT hint names the button on the user's screen -------
    // This test lives HERE, not in `dml-core`, on purpose. The hint is built in
    // `dml_core::error` (the game-agnostic bottom layer, which must never know
    // that a SvelteKit tree exists) but the labels it quotes are rendered by
    // `launcher/src/lib/first-run.ts`. This crate is the one that legitimately
    // spans both sides -- it depends on dml-core and it already reads the
    // frontend's source (`provision.rs` does the same with `api.ts`), so an
    // `include_str!` of a frontend file cannot red `cargo test -p dml-core`
    // (ubuntu CI included) the day that file is renamed.

    /// Every label `first-run.ts` puts on a `kind: "setup"` button, read out of
    /// the file that renders it.
    ///
    /// Parsed rather than duplicated: a copy of the literals here could neither
    /// notice a rename there nor notice that it was quoting the wrong one of the
    /// two. Tolerant of both quote styles and of property order, so a prettier
    /// config change is a formatting change and not a red test.
    fn first_run_setup_button_labels() -> Vec<String> {
        const TS: &str = include_str!("../../src/lib/first-run.ts");

        /// The `{ ... }` object literal that encloses byte `at`.
        fn enclosing_object(src: &str, at: usize) -> Option<(usize, usize)> {
            let b = src.as_bytes();
            let mut depth = 0i32;
            let mut start = None;
            for i in (0..at).rev() {
                match b[i] {
                    b'}' => depth += 1,
                    b'{' => {
                        if depth == 0 {
                            start = Some(i);
                            break;
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            let start = start?;
            depth = 0;
            for i in at..b.len() {
                match b[i] {
                    b'{' => depth += 1,
                    b'}' => {
                        if depth == 0 {
                            return Some((start, i + 1));
                        }
                        depth -= 1;
                    }
                    _ => {}
                }
            }
            None
        }

        /// `name: "value"` / `name: 'value'` inside one object literal. A
        /// non-literal value (the `label: string` of the type declaration)
        /// yields None, which is what keeps the type out of the results.
        fn string_prop(seg: &str, name: &str) -> Option<String> {
            let key = format!("{name}:");
            let mut from = 0usize;
            while let Some(rel) = seg[from..].find(&key) {
                let after = &seg[from + rel + key.len()..];
                let v = after.trim_start();
                let quote = v.chars().next()?;
                if quote == '"' || quote == '\'' {
                    let body = &v[quote.len_utf8()..];
                    if let Some(end) = body.find(quote) {
                        return Some(body[..end].to_string());
                    }
                }
                from += rel + key.len();
            }
            None
        }

        let mut out: Vec<String> = Vec::new();
        for (i, _) in TS.match_indices("kind:") {
            let v = TS[i + "kind:".len()..].trim_start();
            if !(v.starts_with("\"setup\"") || v.starts_with("'setup'")) {
                continue;
            }
            let Some((s, e)) = enclosing_object(TS, i) else { continue };
            let Some(label) = string_prop(&TS[s..e], "label") else { continue };
            if !label.is_empty() && !out.contains(&label) {
                out.push(label);
            }
        }
        out
    }

    #[test]
    fn the_cli_bad_output_hint_names_the_buttons_first_run_actually_renders() {
        // `first-run.ts` has TWO setup buttons and their labels differ by state:
        // `cli_outdated` renders "Update backend", `no_cli` renders "Set up
        // backend". BOTH of those machines produce `CLI_BAD_OUTPUT` -- an old
        // CLI answers in plain text, an absent one answers with nothing that
        // parses -- and that mapping has no way to tell which, so the copy has
        // to name both. It used to name only "Set up backend", the label the
        // RARER of the two sees.
        let labels = first_run_setup_button_labels();
        // Non-vacuity: a parse that found nothing would satisfy the loop below
        // without reading a single label.
        assert!(
            labels.len() >= 2,
            "expected first-run.ts's setup-button labels, parsed: {labels:?}"
        );
        let err = dml_core::error::CmdError::from(dml_core::runner::RunnerError::BadOutput {
            raw: "dml v2.6.0".into(),
        });
        assert_eq!(err.code, "CLI_BAD_OUTPUT");
        for label in &labels {
            assert!(
                err.hint.contains(label.as_str()),
                "a user in one of the states that produce CLI_BAD_OUTPUT sees a button \
                 labelled {label:?}, and the hint never names it: {}",
                err.hint
            );
        }
    }

    // -- the doc comments on the probe seam ----------------------------------

    /// The `///` block immediately above `anchor`, attributes skipped.
    fn doc_block_before(src: &str, anchor: &str) -> String {
        let at = src.find(anchor).unwrap_or_else(|| panic!("anchor not found: {anchor}"));
        let mut lines: Vec<&str> = Vec::new();
        for line in src[..at].lines().rev() {
            let t = line.trim();
            if let Some(rest) = t.strip_prefix("///") {
                lines.push(rest.trim());
            } else if t.starts_with("#[") || t.is_empty() {
                // An attribute (`#[tauri::command]`) still belongs to the item,
                // but a blank line ends the block.
                if t.is_empty() {
                    break;
                }
            } else {
                break;
            }
        }
        lines.reverse();
        lines.join(" ")
    }

    #[test]
    fn each_probe_seam_item_carries_its_own_doc() {
        // Phase 4 review N2. `backend_probe_env` was inserted BETWEEN
        // `backend_status`'s doc block and `backend_status` itself, which moved
        // the whole command doc onto the helper: the helper claimed to "probe
        // this machine", to run "on the blocking pool" and to "NEVER fail" --
        // none of which is true of a function that builds a `SetupProbeEnv` --
        // and the command that DOES make those promises was left undocumented.
        const SRC: &str = include_str!("lib.rs");
        let helper = doc_block_before(SRC, "pub fn backend_probe_env()");
        let command = doc_block_before(SRC, "async fn backend_status(app: tauri::AppHandle)");

        assert!(
            command.contains("NEVER fails"),
            "the never-fails contract must be documented on the command that implements it: {command:?}"
        );
        assert!(
            command.contains("Probe this machine"),
            "backend_status has no doc of its own: {command:?}"
        );
        assert!(
            !helper.contains("NEVER fails") && !helper.contains("Probe this machine"),
            "backend_probe_env is carrying the command's doc: {helper:?}"
        );
        assert!(
            helper.contains("probe environment"),
            "backend_probe_env needs a doc of its own: {helper:?}"
        );
    }

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

    // -- validate_lan_request_native (Chunk 2 task C2c item 3) ---------------

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
        let running_no_ip = TsStatusFields {
            backend_state: Some("Running".into()),
            ip: None,
            ..TsStatusFields::default()
        };
        let connected = running_no_ip.backend_state.as_deref() == Some("Running") && running_no_ip.ip.is_some();
        assert!(!connected);

        let running_with_ip = TsStatusFields {
            backend_state: Some("Running".into()),
            ip: Some("100.1.2.3".into()),
            ..TsStatusFields::default()
        };
        let connected = running_with_ip.backend_state.as_deref() == Some("Running") && running_with_ip.ip.is_some();
        assert!(connected);
    }

    /// The field that turns the live-found dead end into a clickable link:
    /// tailscaled holds the pending URL after our `up` has already given up.
    #[test]
    fn parses_the_pending_auth_url_out_of_status_json() {
        let raw = r#"{"BackendState":"NeedsLogin","TailscaleIPs":[],
                      "AuthURL":"https://login.tailscale.com/a/e73516d017e7e"}"#;
        let fields = parse_tailscale_status_json(raw);
        assert_eq!(fields.auth_url.as_deref(), Some("https://login.tailscale.com/a/e73516d017e7e"));

        // Logged in already: tailscaled reports the key as an empty string, and
        // an empty URL is not a URL. Without this filter `up` would report a
        // pending login that does not exist.
        let done = parse_tailscale_status_json(r#"{"BackendState":"Running","AuthURL":""}"#);
        assert_eq!(done.auth_url, None);
        // Absent entirely is also None, not a panic.
        assert_eq!(parse_tailscale_status_json(r#"{"BackendState":"Running"}"#).auth_url, None);
    }

    /// The 8s -> 45s change is the actual fix for the live failure, so the
    /// DEFAULT is pinned: a silent revert to a sub-30s wait would reintroduce a
    /// bug whose whole signature is "the URL arrives after we stopped waiting".
    #[test]
    fn the_login_wait_defaults_to_longer_than_the_measured_control_plane_delay() {
        // 30s was measured on the VM; the default must clear it with margin.
        assert!(
            TS_UP_TIMEOUT_DEFAULT_SECS >= 45,
            "the control plane took 30s live; {TS_UP_TIMEOUT_DEFAULT_SECS}s leaves no margin"
        );
        // And our own process bound must not fire before tailscale's own
        // timeout, or we kill the run before it can print the URL.
        assert!(TS_UP_OUTER_SLACK_SECS > 0);
    }

    #[test]
    fn short_duration_parsing_accepts_the_forms_the_seam_documents() {
        assert_eq!(parse_short_duration_secs("45s"), Some(45));
        assert_eq!(parse_short_duration_secs("90"), Some(90));
        assert_eq!(parse_short_duration_secs("2m"), Some(120));
        assert_eq!(parse_short_duration_secs(" 30s "), Some(30));
        // Rejected -> the caller keeps the default. Zero and garbage both mean
        // "no usable value": honouring 0 would make the outer bound shorter
        // than the inner timeout, the exact inversion this guards against.
        assert_eq!(parse_short_duration_secs("0s"), None);
        assert_eq!(parse_short_duration_secs(""), None);
        assert_eq!(parse_short_duration_secs("soon"), None);
        assert_eq!(parse_short_duration_secs("1h"), None);
        assert_eq!(parse_short_duration_secs("-5s"), None);
    }

    /// The outer bound must always exceed the inner `--timeout`, for the default
    /// AND for any accepted override — otherwise our kill lands first and
    /// tailscale never gets to print the URL it was about to print.
    /// An override near `u64::MAX` used to PANIC here in debug and wrap in
    /// release, leaving the outer bound below the inner timeout — inverting the
    /// exact invariant the slack exists to hold. Found by adversarial review,
    /// 2026-07-29; the ceiling is what makes the addition infallible.
    #[test]
    fn an_absurd_login_wait_override_is_refused_instead_of_overflowing() {
        // The parser itself must not be the thing that saves us -- it is allowed
        // to return the huge value; the CEILING is the guard.
        assert_eq!(parse_short_duration_secs("18446744073709551615"), Some(u64::MAX));
        assert!(u64::MAX.checked_add(TS_UP_OUTER_SLACK_SECS).is_none(), "the add would overflow");
        assert!(
            TS_UP_TIMEOUT_MAX_SECS.checked_add(TS_UP_OUTER_SLACK_SECS).is_some(),
            "anything at or under the ceiling must be addable without overflow"
        );
        // And a value over the ceiling is ignored in favour of the default,
        // rather than honoured into a GUI that hangs for centuries.
        assert!(TS_UP_TIMEOUT_MAX_SECS > TS_UP_TIMEOUT_DEFAULT_SECS);
    }

    #[test]
    fn the_outer_process_bound_always_outlives_the_inner_timeout() {
        let (flag, outer) = ts_up_timeout();
        let inner: u64 = flag
            .trim_start_matches("--timeout=")
            .trim_end_matches('s')
            .parse()
            .expect("the flag carries a plain seconds value");
        assert!(
            outer.as_secs() > inner,
            "outer {}s must outlive inner {inner}s",
            outer.as_secs()
        );
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

    // -- native world-restart: event-shape builders --------------------------
    // Task: world-restart-native. Assert the EXACT ndjson event shapes the
    // frontend's terminal-state.ts parses (see the task brief) -- these are
    // pure, so no docker/soap I/O is exercised.

    // -- native world-restart: pure decision logic ----------------------------

    // -- native bridge-setup: event-shape builders (Chunk 2 task C2c item 4) --

    // -- native ahbot-repair: event-shape builders (Chunk 2 task C2c item 8) --

}

/// THE RUST HALF OF "A RESULT IS NOT AN OUTCOME".
///
/// `DmlRunner::run_captured` never even reads the child's exit status — it
/// returns `Ok(combined_output)` for a clean run, a compose refusal and a
/// server that ignored the stop alike (`crates/dml-core/src/runner.rs:216`).
/// Its streaming sibling `run_stream` is the same story one layer up
/// (`Ok(code)` for every code), which is why the frontend derives lifecycle
/// success from the terminal `done` event and never from the promise —
/// pinned in `launcher/src/lib/lifecycle-surface.test.ts`.
///
/// Two places in this file stop the server with no terminal attached and then
/// have to TELL THE USER whether it worked:
///   * `auto_shutdown_watcher` — the card's `stopped` / `stop_failed` outcome;
///   * `restart_wsl` — the `stopped_server` flag, reported just before
///     `wsl --shutdown` power-kills whatever is left.
/// Both are honest only because they run `stop_confirmed_down`, a SECOND,
/// independent read of the server verdict. Both say so in a comment, and a
/// comment is not a guard: rewriting either to
/// `runner.run_captured(&[…]).is_ok()` compiles, type-checks, and tells a user
/// their world shut down gracefully when it did not — right before the VM is
/// hard-cut with ~2,000 bots on it.
///
/// Nothing pure can see this. Neither function is reachable from a unit test:
/// one is an infinite watcher loop holding a `tauri::AppHandle`, the other
/// spawns `wsl --shutdown`. So the assertions below read the real source.
///
/// Like `each_probe_seam_item_carries_its_own_doc` above, this scan lives in
/// the launcher crate and reads `include_str!("lib.rs")`, so it runs on the
/// WINDOWS CI job only — the ubuntu job builds the three crates, not the
/// launcher. Do not later read a green ubuntu run as coverage for it.
#[cfg(test)]
pub(crate) mod stop_outcome_scan_tests {
    /// Rust source with comments removed. MANDATORY, and this repo has been
    /// bitten TWICE by skipping it: `feature-keys.test.ts` read a comment as a
    /// call site, and `Test-InstallerNative.ps1` read the installer's own
    /// explanation of what it does NOT do as evidence that it does. The two
    /// call sites scanned here are surrounded by prose naming
    /// `run_captured`, `stop_confirmed_down` and `stop_failed` verbatim — the
    /// doc block directly above this module does it too.
    ///
    /// String and char literals are preserved (the argv literals ARE the data);
    /// raw strings and escapes are tracked so a `//` inside one is not mistaken
    /// for a comment.
    ///
    /// `pub(crate)` so `startup::games_dir_reader_scan_tests` shares it rather
    /// than growing a second stripper: two of these would drift, and the whole
    /// point of both scans is that a comment must never read as a call site.
    pub(crate) fn strip_comments(src: &str) -> String {
        let b = src.as_bytes();
        let mut out = String::with_capacity(src.len());
        let mut i = 0usize;
        let (mut in_str, mut in_raw, mut in_ch) = (false, false, false);
        while i < b.len() {
            let c = b[i] as char;
            let n = if i + 1 < b.len() { b[i + 1] as char } else { '\0' };
            if in_str {
                if c == '\\' && !in_raw {
                    out.push(c);
                    if i + 1 < b.len() {
                        out.push(n);
                    }
                    i += 2;
                    continue;
                }
                if c == '"' {
                    in_str = false;
                    in_raw = false;
                }
                out.push(c);
                i += 1;
                continue;
            }
            if in_ch {
                if c == '\\' {
                    out.push(c);
                    if i + 1 < b.len() {
                        out.push(n);
                    }
                    i += 2;
                    continue;
                }
                if c == '\'' {
                    in_ch = false;
                }
                out.push(c);
                i += 1;
                continue;
            }
            if c == '/' && n == '/' {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if c == '/' && n == '*' {
                i += 2;
                let mut depth = 1usize;
                while i < b.len() && depth > 0 {
                    if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                        depth += 1;
                        i += 2;
                    } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                        depth -= 1;
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                out.push(' ');
                continue;
            }
            if c == 'r' && (n == '"' || n == '#') {
                let mut j = i + 1;
                while j < b.len() && b[j] == b'#' {
                    j += 1;
                }
                if j < b.len() && b[j] == b'"' {
                    in_str = true;
                    in_raw = true;
                    for k in i..=j {
                        out.push(b[k] as char);
                    }
                    i = j + 1;
                    continue;
                }
            }
            if c == '"' {
                in_str = true;
                in_raw = false;
                out.push(c);
                i += 1;
                continue;
            }
            // A lifetime (`'a`) is not a char literal.
            if c == '\'' {
                let closes = (i + 2 < b.len() && b[i + 2] == b'\'')
                    || (n == '\\' && i + 3 < b.len() && b[i + 3] == b'\'');
                if closes {
                    in_ch = true;
                    out.push(c);
                    i += 1;
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
        out
    }

    /// Source with every `#[cfg(test)]` item removed, brace-matched.
    ///
    /// Deliberately NOT "cut at the first `#[cfg(test)]`". Truncating there
    /// silently stops scanning everything below, which makes a guard's coverage
    /// depend on where in the file you happen to type — a call site appended to
    /// the end of the file would sail through while the identical function 2000
    /// lines up would fail loudly. This file's `mod tests` is the last item
    /// today, so a truncating version would be harmless RIGHT NOW and latent in
    /// the worst direction. An item whose attribute is followed by a `;` before
    /// any `{` (`#[cfg(test)] use …;`) is cut at the semicolon instead.
    ///
    /// Input must already be comment-stripped, so a `{` inside prose cannot
    /// unbalance the match.
    ///
    /// `pub(crate)` for the same sharing reason as [`strip_comments`].
    pub(crate) fn strip_cfg_test(code: &str) -> String {
        let attr: Vec<char> = "#[cfg(test)]".chars().collect();
        let b: Vec<char> = code.chars().collect();
        let mut out = String::with_capacity(code.len());
        let mut i = 0usize;
        while i < b.len() {
            if b[i..].starts_with(&attr[..]) {
                i = end_of_item(&b, i + attr.len());
                continue;
            }
            out.push(b[i]);
            i += 1;
        }
        out
    }

    /// One past the end of the item starting at `from` — the first `;` at depth
    /// zero, or the `}` that closes its first block. String and char literals
    /// are skipped so a brace or semicolon inside one cannot unbalance it.
    fn end_of_item(b: &[char], from: usize) -> usize {
        let mut i = from;
        let mut depth = 0usize;
        while i < b.len() {
            match b[i] {
                '"' => {
                    i += 1;
                    while i < b.len() {
                        if b[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if b[i] == '"' {
                            break;
                        }
                        i += 1;
                    }
                }
                '\'' if i + 2 < b.len() && (b[i + 2] == '\'' || b[i + 1] == '\\') => {
                    i += 1;
                    while i < b.len() {
                        if b[i] == '\\' {
                            i += 2;
                            continue;
                        }
                        if b[i] == '\'' {
                            break;
                        }
                        i += 1;
                    }
                }
                '{' => depth += 1,
                '}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        return i + 1;
                    }
                }
                ';' if depth == 0 => return i + 1,
                _ => {}
            }
            i += 1;
        }
        b.len()
    }

    /// The production text: no comments, no test modules. Everything below
    /// scans THIS, never the raw file.
    fn production_half(src: &str) -> String {
        strip_cfg_test(&strip_comments(src))
    }

    /// The body of the item introduced by `anchor`. PANICS when the anchor is
    /// gone rather than returning "" — every assertion built on a scanner is
    /// vacuous if the scanner silently finds nothing.
    fn body_of(src: &str, anchor: &str) -> String {
        let at = src.find(anchor).unwrap_or_else(|| panic!("anchor not found: {anchor}"));
        let b: Vec<char> = src.chars().collect();
        let start = src[..at + anchor.len()].chars().count();
        let end = end_of_item(&b, start);
        b[start..end].iter().collect()
    }

    /// Every `run_captured(...)` whose argv is a `games stop`, as
    /// `(what BINDS the call, what runs immediately after it)`.
    ///
    /// The binding half has the receiver path removed (`… let _ = runner.` ->
    /// `… let _ =`), so the assertion is about how the Result is treated and
    /// not about what the runner happens to be called at that call site.
    fn stop_capture_sites(src: &str) -> Vec<(String, String)> {
        let needle = "run_captured(";
        let mut out = Vec::new();
        let mut from = 0usize;
        while let Some(rel) = src[from..].find(needle) {
            let at = from + rel;
            from = at + needle.len();
            let args_end = src[at..].find(')').map(|r| at + r).unwrap_or(src.len());
            let args = &src[at..args_end];
            if !(args.contains("\"games\"") && args.contains("\"stop\"")) {
                continue;
            }
            let lead = src[..at]
                .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == '.')
                .trim_end()
                .to_string();
            let semi = src[args_end..].find(';').map(|r| args_end + r + 1).unwrap_or(src.len());
            let tail: String = src[semi..].chars().take(120).collect();
            out.push((lead, tail.trim_start().to_string()));
        }
        out
    }

    // -----------------------------------------------------------------------

    /// Non-vacuity for the strippers. Both were ported from the sibling branch
    /// that wrote them, and a stripper that quietly returned its input (or
    /// everything) would make every assertion below meaningless in one
    /// direction or the other.
    #[test]
    fn the_strippers_strip_prose_and_keep_code() {
        assert!(!strip_comments("// run_captured(&[\"games\", \"stop\"])\nlet a = 1;")
            .contains("run_captured"));
        assert!(!strip_comments("/* stop_confirmed_down */ let a = 1;").contains("stop_confirmed"));
        // A `//` INSIDE a string literal is data, not a comment.
        assert!(strip_comments("let u = \"https://x/y\";").contains("https://x/y"));
        // The argv literals must survive — they are what the scan classifies on.
        assert!(strip_comments("run_captured(&[\"games\", \"stop\"]);").contains("\"games\""));
        // Removed, not truncated: production code AFTER a test module survives.
        let src = "fn a() {}\n#[cfg(test)]\nmod t { fn x() { let s = \"}\"; } }\nfn b() {}\n";
        let out = strip_cfg_test(src);
        assert!(out.contains("fn a()"), "{out:?}");
        assert!(
            out.contains("fn b()"),
            "a truncating strip_cfg_test stops scanning everything below the first test \
             module, which makes coverage depend on where in the file you type: {out:?}"
        );
        assert!(!out.contains("fn x()"), "{out:?}");
        // `#[cfg(test)] use …;` is cut at the semicolon, not at a later brace.
        let u = strip_cfg_test("#[cfg(test)]\nuse std::x;\nfn keep() {}\n");
        assert!(!u.contains("use std::x"), "{u:?}");
        assert!(u.contains("fn keep()"), "{u:?}");
    }

    /// Non-vacuity for the site extractor itself, with a known positive, two
    /// known negatives and the panic-on-miss contract.
    #[test]
    fn the_site_extractor_finds_stop_captures_and_nothing_else() {
        let one = stop_capture_sites("let _ = runner.run_captured(&[\"games\", \"stop\", T]);\nnext();");
        assert_eq!(one.len(), 1);
        assert!(one[0].0.ends_with("let _ ="), "{:?}", one[0].0);
        assert!(one[0].1.starts_with("next()"), "{:?}", one[0].1);
        // A different verb is not a stop.
        assert!(stop_capture_sites("runner.run_captured(&[\"doctor\"]);").is_empty());
        assert!(stop_capture_sites("runner.run_captured(&refs);").is_empty());
        // Prose is not a call site — the real file has this exact sentence in
        // a comment, so the scan MUST go through production_half().
        assert!(
            stop_capture_sites(&production_half(
                "// let _ = runner.run_captured(&[\"games\", \"stop\", T]);\n"
            ))
            .is_empty()
        );
        assert!(std::panic::catch_unwind(|| body_of("fn a() {}", "fn zz(")).is_err());
    }

    /// THE TEST.
    #[test]
    fn a_stop_is_judged_by_a_second_read_never_by_the_runners_result() {
        let src = production_half(include_str!("lib.rs"));
        let sites = stop_capture_sites(&src);

        // NON-VACUITY: an extractor that stopped matching must fail loudly
        // rather than pass an empty loop.
        assert_eq!(
            sites.len(),
            2,
            "this suite knows about 2 terminal-less `games stop` call sites in lib.rs \
             (auto_shutdown_watcher, restart_wsl), and found {}. A new one is not \
             automatically wrong — but it reports an outcome to the user with no terminal \
             attached, so it has to be READ and this number bumped deliberately.",
            sites.len()
        );

        for (lead, tail) in &sites {
            assert!(
                lead.ends_with("let _ ="),
                "a `games stop` result is being BOUND here. `run_captured` returns \
                 Ok(output) for every exit code — it never reads the child's status — so \
                 anything derived from that Result reports success for a stop that did not \
                 happen. Discard it (`let _ =`) and judge the stop by effect. Lead: {:?}",
                &lead[lead.len().saturating_sub(90)..]
            );
            assert!(
                tail.starts_with("stop_confirmed_down(") || tail.starts_with("if stop_confirmed_down("),
                "the statement immediately after a `games stop` must be the independent \
                 re-read that decides the outcome. Anything else means the outcome came \
                 from somewhere that cannot know whether the world actually went down — \
                 and both of these call sites then TELL THE USER it did. Found: {tail:?}"
            );
        }
    }

    /// ...and the re-read must itself be a re-read.
    ///
    /// The test above is satisfied by a `stop_confirmed_down` that returns
    /// `runner.run_captured(&["games","stop",…]).is_ok()`, or `true`. Then the
    /// effect check is a rubber stamp and every assertion above still passes.
    #[test]
    fn stop_confirmed_down_asks_the_server_again() {
        let body = body_of(&production_half(include_str!("lib.rs")), "fn stop_confirmed_down(");
        assert!(
            body.contains("read_server_verdict("),
            "stop_confirmed_down must re-read the server verdict — that second, independent \
             read is the ONLY thing that makes `stopped` an honest claim: {body:?}"
        );
        assert!(
            !body.contains("run_captured("),
            "stop_confirmed_down is judging the stop by a runner Result, which is Ok on \
             every exit code: {body:?}"
        );
        assert!(
            body.contains("verdict_needs_stop("),
            "stop_confirmed_down must map the verdict through the same predicate the caller \
             used to decide a stop was needed, or the two can disagree: {body:?}"
        );
    }
}
