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
pub mod wsl_keepalive;
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
    /// Whether the credentials this launcher proved have reached the CLI that
    /// uses them — see [`dml_core::soap_env`].
    ///
    /// A SEPARATE latch from `soap_autosetup`, deliberately. That one answers
    /// "has an account been made this run"; this one answers "does the CLI route
    /// authenticate", and the two come apart in the ordinary case: a launcher
    /// whose own SOAP works creates no account at all (`not_needed`, never
    /// latched) while the in-distro CLI is refused on every verb. Folding them
    /// into one flag would tie the repair to a decision that was never taken.
    pub soap_env_sync: Arc<Mutex<dml_core::soap_env::SoapEnvSync>>,
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
    // ADOPTION, and it is the only thing the keep-alive takes from the webview.
    // A server the user started in a PREVIOUS launcher session is running with
    // nobody holding its distro; the first positive verdict after this launcher
    // starts is what tells Rust it exists. Everything AFTER that — noticing the
    // holder died, respawning it, releasing it — is owned by a plain OS thread
    // that no amount of timer throttling can reach. A negative verdict
    // deliberately does nothing (see `Keepalive::observed_status`).
    wsl_keepalive::observed_status(&verdict);
    tray::apply_status(&app, &verdict);
}

/// What the keep-alive is doing right now, re-derived from live state.
///
/// Sync and infallible, same doctrine as `set_keep_awake`. Called on mount as
/// well as on the event, because the frontend's memory of a failure is a
/// module-level store and a webview reload wipes it — the recorded
/// soap-autosetup lesson, in the one place where the thing being forgotten is
/// "your server is going to stop in 15 seconds".
#[tauri::command]
fn wsl_keepalive_status() -> wsl_keepalive::KeepaliveReport {
    wsl_keepalive::keepalive_report()
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
        // DELIBERATE, AND LOCAL TO THIS FUNCTION. The value here feeds a
        // frontend union with exactly two members (`"wsl" | "native"`, see
        // `first-run.ts`), and both Arch and Wsl route through the same distro
        // and the same daemon, so the frontend's question has one answer for
        // both. Task 3 is what makes this differ.
        //
        // DO NOT copy this collapse to `startup.rs::backend_env_value`. That
        // one answers a different question — which string to write into
        // `DML_BACKEND` — and collapsing Arch onto "wsl" THERE makes the whole
        // `launcher.json` opt-in silently inert, with no error and no red test
        // until `every_backend_round_trips_through_the_value_we_export` was
        // added. This comment used to read "Arch and Wsl name the same distro
        // and the same daemon", which is true and was read as a licence to
        // dedup the two sites.
        dml_wow::backend::Backend::Arch | dml_wow::backend::Backend::Wsl => "wsl",
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
    native_title_count_in(&dml_core::compose::games_dir_from_env())
}

/// [`native_title_count`]'s answer for a games directory handed in.
///
/// THE DIRECTORY IS AN ARGUMENT, and that is not tidiness. The test for the
/// absent-directory case used to `set_var("DML_GAMES_DIR", …)` — a
/// PROCESS-GLOBAL mutation inside a test binary whose threads run in parallel,
/// while `native_title_count` and `native_facts` read that same variable in the
/// same binary. It is the flake generator an earlier task already removed from
/// `games_dir_from`, reintroduced one function along. A pure function taking the
/// value cannot race anything.
fn native_title_count_in(dir: &std::path::Path) -> Option<usize> {
    let entries = match std::fs::read_dir(dir) {
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
    // `backend_pinned_by_env`, not "was set": `DML_BACKEND=auto` asks us to
    // detect, so it must NOT report as an env lock. Reporting it as one greyed
    // out the dropdown AND named a backend the user never chose — after the
    // export fix, `std::env::var` here would read back our own resolved
    // "native"/"wsl" and attribute it to them.
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

/// Did a streamed lifecycle run actually FAIL? — FIX ROUND 3 (2026-08-05), C1.
///
/// `exit_stop_and_close` used to decide with `after_stop(result.is_ok())`, and
/// that expression is `true` for every failure it exists to catch:
/// [`DmlRunner::run_stream`] returns `Ok(code)` for EVERY exit code (it
/// synthesizes an `error` event for a non-zero one and still returns `Ok` —
/// `crates/dml-core/src/runner.rs:449`), and `stream_action` then threw the
/// code away with `.map(|_exit| ())`. So `AfterStop::ReportFailure` was
/// unreachable, a confirmed stop that failed still closed the launcher, the
/// holder was still released, and the distro still powered off ~15s later on
/// top of whatever a failed `compose down` left running. That is the original
/// incident, unchanged by the fix that was supposed to close it.
///
/// THE SIGNAL IS THE EVENT, not the exit code, because the event is the only
/// one BOTH backends produce. The native path
/// (`dml_wow::lifecycle::games_lifecycle_stream` via
/// [`run_games_lifecycle_native`]) reports domain failures purely in the
/// stream and its wrapper resolves `Ok(())` by design — its doc comment says
/// so. This is the repo's own recorded rule ("UI outcome must be derived from
/// done/error events, not promise rejection") finally applied on the Rust
/// side, where the close decision is actually made. The exit code is kept as a
/// second signal because a CLI that dies before emitting anything at all
/// (`CLI_CRASH`) has no event to read.
///
/// Scoped DELIBERATELY to the exit path. Making `stream_action` itself return
/// `Err` on a non-zero exit would change what Home's Start/Stop/Restart
/// buttons see, which is a product change nobody asked for; every other caller
/// keeps today's contract by passing [`StreamOutcome::default`] and ignoring
/// it.
#[derive(Clone, Default)]
struct StreamOutcome(Arc<AtomicBool>);

impl StreamOutcome {
    /// Every event on its way to the frontend passes through here.
    fn observe(&self, v: &serde_json::Value) {
        if v.get("event").and_then(serde_json::Value::as_str) == Some("error") {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    /// The second signal: a CLI that exited non-zero without ever emitting an
    /// `error` event (killed, or dead before its first line).
    fn note_exit_code(&self, code: i32) {
        if code != 0 {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn failed(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

async fn stream_action(
    action: &'static str,
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
    watch: StreamOutcome,
) -> Result<(), CmdError> {
    if !validate_game_id(&id) {
        return Err(bad_id(&id));
    }
    let runner = state.runner.clone();
    let seen = watch.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runner.run_stream(&["games", action, &id], |v| {
            seen.observe(&v);
            let _ = on_event.send(v);
        })
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map(|exit| watch.note_exit_code(exit))
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
    let runner = state.runner.clone();
    let env_latch = state.soap_env_sync.clone();

    let (result, synced) = tauri::async_runtime::spawn_blocking(move || {
        let cfg = dml_wow::db::DbConfig::from_env();
        let result = (|| {
            dml_wow::account_write::create_gm_account(&cfg, &user, &pass)?;
            // Same verify-then-save routine the manual path uses, so there is
            // one definition of "done" rather than two that can disagree.
            sb::bootstrap_verify_with(&home, &url, &user, &pass, |c, cmd| {
                let _guard = soap_lock.lock();
                dml_wow::soap::exec(c, cmd)
            })
        })();
        // The manual fallback must repair the same split automatic setup does.
        // Fixing the Windows copy and leaving the in-distro CLI refused would
        // reproduce R6 through the one screen that exists to unstick a user.
        let synced = sync_distro_soap_env(&runner, &env_latch);
        (result, synced)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;

    let (soap_env, soap_env_detail) = soap_env_sync_json(&synced);
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
                "soap_env": soap_env,
                "soap_env_detail": soap_env_detail,
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

/// Make the credentials this launcher proved reach the CLI that uses them.
///
/// ## The split (R6)
///
/// Everything above this line runs IN THE LAUNCHER, on Windows, and persists to
/// the Windows `~/.dml/soap.env`. On `Backend::Arch` and `Backend::Wsl` the CLI
/// that answers every SOAP-backed verb — GM Tools, My Party, console send,
/// announcements, motd — runs INSIDE `dml-arch` and reads
/// `/home/dml/.dml/soap.env`. Nothing has ever copied one to the other, so the
/// launcher could prove a round-trip, report success and show the account in its
/// credentials panel while every one of those features returned `SOAP_AUTH`.
///
/// ## Why it hangs off this command rather than the status poll
///
/// The frontend calls automatic setup exactly when `detail.soap.auth_ok ===
/// false`, and on the two in-distro backends that `detail` IS the distro's own
/// answer (`wow server-detail` goes through the runner). So the one trigger the
/// UI already has is, on those backends, precisely "the in-distro CLI is being
/// refused" — the question this repair exists to answer. Adding a second poll
/// would ask the same thing again, less accurately.
///
/// Everything else lives in [`dml_core::soap_env`]: the `Backend::Native` gate
/// (no distro, same file, zero spawns), the rule that a credential the CLI route
/// still authenticates with is NEVER overwritten, the tri-state that keeps a
/// booting world server from being read as a broken account, the per-credential
/// latch, and the stdin delivery that keeps the password out of every argv.
///
/// Best-effort and non-fatal by construction: the outcome is REPORTED, never
/// thrown. A failure here must not turn a successful account setup into an error
/// the card cannot render.
fn sync_distro_soap_env(
    runner: &DmlRunner,
    latch: &Mutex<dml_core::soap_env::SoapEnvSync>,
) -> dml_core::soap_env::SyncOutcome {
    use dml_core::soap_env as se;
    let backend = dml_wow::backend::selected();
    // The `Backend::Native` exit, taken before ANY resolution, spawn or read.
    if !se::cli_home_is_the_distro(backend) {
        return se::SyncOutcome::NotApplicable;
    }
    // Resolved HERE rather than passed in, so a credential a caller just created
    // and proved is the one that gets published. `configured` is carried through
    // from the resolver for the same reason the credentials panel does it: only
    // that resolver can tell a real account named `admin` from the compiled-in
    // default nobody supplied.
    let (cfg, configured) = dml_wow::soap::SoapConfig::load_with_provenance();
    let contents = dml_wow::soap_bootstrap::soap_env_contents(&cfg.url, &cfg.user, &cfg.pass);
    let mut st = match latch.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    se::sync_with(
        backend,
        configured,
        &contents,
        &mut st,
        || se::probe_cli_soap(runner),
        se::publish_to_distro,
    )
}

/// The non-secret half of a sync outcome, for the command payloads.
///
/// `(status, detail)`. The detail strings come from [`dml_core::soap_env`]'s own
/// variants, none of which can carry a credential — the write script echoes
/// nothing and the probe reports envelope codes.
fn soap_env_sync_json(o: &dml_core::soap_env::SyncOutcome) -> (String, serde_json::Value) {
    use dml_core::soap_env::SyncOutcome as S;
    let detail = match o {
        S::Unknown(m) | S::Unproven(m) | S::Failed(m) => serde_json::Value::String(m.clone()),
        _ => serde_json::Value::Null,
    };
    (dml_core::soap_env::outcome_status(o).to_string(), detail)
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
    let runner = state.runner.clone();
    let env_latch = state.soap_env_sync.clone();

    let (outcome, synced) = tauri::async_runtime::spawn_blocking(move || {
        // The account machine, UNCHANGED. Wrapped in a closure only so the
        // cheap exit below still returns from IT rather than from the whole
        // task -- the distro sync after it has to run on that path too, and it
        // is the path a concluded-but-still-broken run depends on.
        let outcome = (|| {
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
        })();

        // AFTER the machine has advanced, and on every arm including the cheap
        // exit. Two reasons for that placement, both real: a credential
        // `advance_with` has just created and PROVED is the one that should
        // reach the distro, and the common case for this repair
        // (`AutoOutcome::NotNeeded` -- the launcher's own SOAP works, the
        // in-distro CLI's does not) never reaches the machine's write path at
        // all. It reads nothing out of `outcome` and decides nothing from it,
        // so it cannot perturb that state machine.
        let synced = sync_distro_soap_env(&runner, &env_latch);
        (outcome, synced)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;

    let (soap_env, soap_env_detail) = soap_env_sync_json(&synced);
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
        // Diagnostic only. The UI switches on `status` alone and must keep
        // doing so: whether the CLI got the credentials is a different fact
        // from whether an account was made, and conflating them is what let
        // the split hide in the first place.
        "soap_env": soap_env,
        "soap_env_detail": soap_env_detail,
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
    let runner = state.runner.clone();
    let env_latch = state.soap_env_sync.clone();
    let (result, synced) = tauri::async_runtime::spawn_blocking(move || {
        let result = sb::bootstrap_verify_with(&home, &url, &user, &pass, |cfg, cmd| {
            // Serialized like every other native SOAP call: the worldserver's
            // SOAP listener runs on the single world thread.
            let _guard = soap_lock.lock();
            dml_wow::soap::exec(cfg, cmd)
        });
        // Same repair as the automatic path, for the same reason: credentials
        // the launcher has just proved are no use to GM Tools, My Party or the
        // console while the CLI that runs them reads a different file.
        let synced = sync_distro_soap_env(&runner, &env_latch);
        (result, synced)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?;
    let (outcome, path) = result?;

    let (status, detail) = match &outcome {
        sb::VerifyOutcome::Ok => ("ok", String::new()),
        sb::VerifyOutcome::Rejected(m) => ("rejected", m.clone()),
        sb::VerifyOutcome::Unreachable(m) => ("unreachable", m.clone()),
    };
    let (soap_env, soap_env_detail) = soap_env_sync_json(&synced);
    Ok(serde_json::json!({
        "status": status,
        "detail": detail,
        "saved_to": path.map(|p| p.display().to_string()),
        "soap_env": soap_env,
        "soap_env_detail": soap_env_detail,
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
    // INTENT, declared BEFORE the work starts. On the Arch backend the distro
    // gets 15 seconds from the moment the last session into it exits — and this
    // command IS such a session, so the hold has to exist before it returns or
    // the server we just started dies a quarter of a minute later. No-op on
    // every other backend.
    wsl_keepalive::server_should_run();
    // NATIVE MODE: the engine is a hard prerequisite (regardless of the manage
    // toggle) — bring it up first, or abort before touching compose. WSL mode
    // skips this entirely and behaves exactly as before.
    if is_native_backend() {
        ensure_engine_up(&on_event).await?;
        // Chunk 3b: native mode replaces the inner `dml` shell-out with
        // direct compose orchestration (see `games_lifecycle_stream`
        // below) -- the engine-ensure-up wrapping just above is unchanged.
        return run_games_lifecycle_native("start", id, false, on_event, StreamOutcome::default())
            .await;
    }
    stream_action("start", id, on_event, state, StreamOutcome::default()).await
}

/// The IPC surface. A PURE DELEGATE: every line of the stop — and in
/// particular the `server_should_stop()` ordering — lives in
/// [`games_stop_watched`], which is also what [`exit_stop_and_close`] calls, so
/// the two callers cannot drift and the ordering scan has one body to read.
/// Pinned by `the_stop_command_is_a_pure_delegate`.
#[tauri::command]
async fn games_stop(
    id: String,
    manage_docker: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    games_stop_watched(id, manage_docker, on_event, state, StreamOutcome::default()).await
}

/// The stop itself. `watch` is how [`exit_stop_and_close`] learns whether the
/// stop actually worked — see [`StreamOutcome`]; ordinary callers pass a
/// default and ignore it.
async fn games_stop_watched(
    id: String,
    manage_docker: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
    watch: StreamOutcome,
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
        run_games_lifecycle_native("stop", id, false, on_event.clone(), watch).await
    } else {
        stream_action("stop", id, on_event.clone(), state, watch).await
    };
    // Then free the VM's RAM by stopping the engine. Best-effort: this runs
    // even if the server-stop reported an error (the containers die with the
    // engine anyway), and its own failure only warns — `result` is what the
    // command returns.
    if stop_docker {
        stop_engine_best_effort(&on_event).await;
    }
    // INTENT, declared AFTER the work. Releasing first would start the distro's
    // 15s clock while compose is still shutting containers down — and a distro
    // that powers off mid-`down` is the ungraceful stop this backend already
    // struggles with. The result stands regardless: a stop that FAILED still
    // means the user wants it down, and the watchdog must not go on holding a
    // distro for a server nobody wants.
    wsl_keepalive::server_should_stop();
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
    // A restart means the server is meant to be UP on the far side, and the
    // window in between is precisely when the distro is most likely to lose its
    // last session.
    wsl_keepalive::server_should_run();
    // Chunk 3b: native mode replaces the inner `dml` shell-out with direct
    // compose orchestration. No engine-lifecycle wrapping here (matches
    // today's WSL sibling: a restart assumes the server -- and so the
    // engine -- is already up; only cold `start` brings Docker Desktop up).
    if is_native_backend() {
        return run_games_lifecycle_native("restart", id, skip, on_event, StreamOutcome::default())
            .await;
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
    watch: StreamOutcome,
) -> Result<(), CmdError> {
    let ch = on_event.clone();
    tauri::async_runtime::spawn_blocking(move || {
        dml_wow::lifecycle::games_lifecycle_stream(mode, id, skip_saveall, |v| {
            // C1: this wrapper resolves Ok(()) BY DESIGN (see the doc comment
            // above) — the stream is the only place a native domain failure is
            // reported, so it is the only place it can be observed.
            watch.observe(&v);
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

use std::sync::atomic::{AtomicBool, Ordering};

/// Set once the user has confirmed the exit (or chosen to close anyway), so the
/// `app.exit(0)` that follows is not intercepted and prompted a second time.
static EXIT_CONFIRMED: AtomicBool = AtomicBool::new(false);

/// Pure: does this exit need a dialog, given the latch?
fn should_prompt_on_exit(action: wsl_keepalive::ExitAction) -> bool {
    if EXIT_CONFIRMED.load(Ordering::SeqCst) {
        return false;
    }
    !matches!(action, wsl_keepalive::ExitAction::ExitNow)
}

/// FIX ROUND 2 (2026-08-05) — F1: `prevent_exit()` had no bound and no
/// fallback, so a dead webview made the launcher UNCLOSABLE.
///
/// The trap was self-reinforcing. A webview that fails to load (vite down under
/// `tauri dev`, a broken WebView2, a JS error before `onMount`) never calls
/// `tray_set_status`, so `Keepalive::last_verdict` stays `None`, so
/// `presence_from(false, None)` is `Unknown`, so `exit_decision` is
/// `PromptUnknown`, so `should_prompt_on_exit` is true, so `prevent_exit()`
/// fires — and the dialog that would answer it is exactly what cannot render.
/// Tray Quit then did nothing, window X is `prevent_close()`d unconditionally
/// and routes to `HideToTray`/`PromptVisible` (neither of which exits), and NO
/// UI PATH CLOSED THE PROCESS. The user reaches for Task Manager, which skips
/// `RunEvent::Exit` and hands the server the exact hard WSL cut this plan
/// exists to prevent. It was also a REGRESSION: before this plan `RunEvent::
/// Exit` prevented nothing and the app always terminated.
///
/// THE PROPERTY GUARANTEED, in one line: **no sequence of exit requests can be
/// prevented forever — at most [`MAX_UNANSWERED_EXIT_PREVENTIONS`] of them in a
/// row are, and a webview that has never spoken is never prevented at all.**
/// Both halves are needed, and each covers the other's hole:
///
/// * **Never spoken → never prevent.** `AppState::last_status_push` is `None`
///   until the frontend's first `tray_set_status`, so it is the one thing Rust
///   knows about whether a webview exists to answer a question. When it never
///   spoke, the ONLY reason we would prompt is `PromptUnknown` — and that
///   Unknown is *caused by* the same silence, so it is evidence of a dead
///   webview, not of a running server. Closing on the first click there is
///   exactly the pre-plan behaviour, i.e. no regression at all in the case
///   that matters most (a launcher whose UI never came up is useless anyway).
/// * **A bound on consecutive preventions.** A webview that speaks (the poll
///   keeps running) but cannot show THIS dialog — the exact regression the
///   Task-4 fix round chased, and the one a renamed event literal reproduces —
///   would sail past the rule above forever. So the guard also counts: after
///   two prevented requests with no Rust-visible answer, the third closes.
///
/// A count alone would be wrong, which is why [`EXIT_REQUEST_WINDOW`] exists:
/// Cancel is invisible to Rust (it touches no command), so three cancels
/// spread over a working six-hour session would otherwise disarm the fourth
/// Quit and hard-cut a live server. Only requests CLOSE TOGETHER count as one
/// run of unanswered asks; a quiet gap starts fresh.
///
/// FIX ROUND 3 (2026-08-05) — C2. THE COUNT MUST NOT RUN WHILE WE ARE BUSY
/// ANSWERING. This comment used to end "nothing needs to reset the count on a
/// real answer, because both answers end the process". That is false for
/// exactly one call, and it is the dangerous one: `exit_stop_and_close` awaits
/// `games_stop` — tens of seconds at ~2,000 bots — and does not latch
/// `EXIT_CONFIRMED` until the stop settles. An impatient second click spends a
/// prevention and, because the frontend drops it (`if (exitGuard.busy)
/// return`) on an already-visible window, produces NO visible change at all.
/// A third click inside the same window met the bound, was not prevented, and
/// killed the process mid-`compose down` — holder released, distro off ~15s
/// later. The launcher was sitting inside the await, holding ground truth that
/// the request WAS being answered, and never asked itself.
///
/// So the bound is suspended, not enlarged, while a confirmed stop is
/// draining: see [`stop_in_flight`]. It is bounded by the stop itself rather
/// than by a clock, and "Close anyway" (`exit_anyway`, which latches
/// `EXIT_CONFIRMED` and never reaches this guard) remains the unconditional
/// escape throughout — so this cannot rebuild F1's trap.
const MAX_UNANSWERED_EXIT_PREVENTIONS: u32 = 2;

/// A gap this long between two exit requests starts a fresh count. See
/// [`MAX_UNANSWERED_EXIT_PREVENTIONS`] for why the window, not just the count,
/// is load-bearing.
const EXIT_REQUEST_WINDOW: std::time::Duration = std::time::Duration::from_secs(60);

/// Pure: may this exit request be PREVENTED at all?
///
/// `webview_has_spoken` — has the frontend ever pushed a status
/// (`AppState::last_status_push`)? `prevented_in_window` — how many requests we
/// have already prevented in the current run of asks. `stop_in_flight` — is a
/// CONFIRMED stop draining right now (C2)?
///
/// `stop_in_flight` outranks both, and it is the one input that can say YES on
/// its own. The other two answer "is there any point asking?"; this one answers
/// "are we already executing the answer?", and killing the process in the
/// middle of `compose down` is the precise harm this whole module exists to
/// prevent. It cannot trap anyone: it is true only while a `games_stop` the
/// user themselves confirmed is running, and "Close anyway" bypasses this
/// function entirely by latching `EXIT_CONFIRMED` first.
fn may_prevent_exit(
    webview_has_spoken: bool,
    prevented_in_window: u32,
    stop_in_flight: bool,
) -> bool {
    if stop_in_flight {
        return true;
    }
    webview_has_spoken && prevented_in_window < MAX_UNANSWERED_EXIT_PREVENTIONS
}

/// The run of unanswered exit requests, and the clock that ends it.
struct ExitPromptGuard {
    prevented: u32,
    last_request: Option<std::time::Instant>,
}

impl ExitPromptGuard {
    /// Record an exit request and answer whether it may be prevented.
    ///
    /// Takes `now` rather than reading the clock itself so the window can be
    /// driven in a test — the guarantee this whole module exists for is a
    /// statement about a SEQUENCE of requests, and a sequence that has to be
    /// waited out in real time is a guarantee nobody pins.
    fn request(
        &mut self,
        webview_has_spoken: bool,
        stop_in_flight: bool,
        now: std::time::Instant,
    ) -> bool {
        let fresh_run = self
            .last_request
            .map_or(true, |t| now.duration_since(t) >= EXIT_REQUEST_WINDOW);
        if fresh_run {
            self.prevented = 0;
        }
        self.last_request = Some(now);
        let allow = may_prevent_exit(webview_has_spoken, self.prevented, stop_in_flight);
        // C2: a request made while we are already executing the user's answer
        // is not an UNANSWERED ask, so it must not spend the budget either.
        // Counting it would only defer the same death by a click or two.
        if allow && !stop_in_flight {
            self.prevented += 1;
        }
        allow
    }

    /// The run is over because a real answer arrived that did NOT end the
    /// process — today that is exactly one thing: `exit_stop_and_close`'s
    /// failure arm (C1). Anything the user does next is a fresh decision made
    /// with the failure in front of them, so it must not inherit a budget
    /// already spent waiting for an answer they have now been given.
    fn answered(&mut self) {
        self.prevented = 0;
        self.last_request = None;
    }
}

static EXIT_PROMPT_GUARD: Mutex<ExitPromptGuard> =
    Mutex::new(ExitPromptGuard { prevented: 0, last_request: None });

/// The ONE gate both exit surfaces consult, and the only place a prevention is
/// recorded. Impure (it reads `AppState` and the clock and mutates the run
/// counter); the decision itself is [`may_prevent_exit`].
///
/// FAILS OPEN TOWARD CLOSING, deliberately: a missing `AppState` or a poisoned
/// lock answers `false`, i.e. "do not prevent". Every uncertainty here must
/// resolve in favour of the user being able to close their launcher — that is
/// the whole point of the guard, and an uncertainty that resolved the other way
/// would rebuild the trap out of the fix.
///
/// CALL IT ONLY WHEN A PROMPT IS ACTUALLY ON THE TABLE. Hiding to the tray asks
/// the user nothing, so it must not spend a prevention: three ordinary X-clicks
/// with `closeToTray` on (the default) would otherwise disarm the next Tray
/// Quit's dialog and hard-cut a live server.
fn exit_prevention_allowed(app: &tauri::AppHandle) -> bool {
    exit_prevention_allowed_with(
        webview_has_spoken(app),
        stop_in_flight(),
        std::time::Instant::now(),
    )
}

/// FIX ROUND 3 (2026-08-05) — H6. Split out of [`exit_prevention_allowed`] so
/// the decision has a seam a test can drive. Before this, BOTH of the reads
/// below could be replaced by their own inverse — `let webview_has_spoken =
/// true`, `.unwrap_or(false)` → `.unwrap_or(true)` — and the whole suite
/// stayed at 273 passed. A paragraph of doc comment established the fail-open
/// as load-bearing while nothing enforced it.
///
/// FAILS OPEN TOWARD CLOSING: a missing `AppState` or a poisoned lock answers
/// `false`, i.e. "we have no evidence a webview exists to answer a question, so
/// do not prevent". Inverted, a poisoned lock would start PREVENTING — the trap
/// rebuilt out of the fix.
fn webview_has_spoken(app: &tauri::AppHandle) -> bool {
    app.try_state::<AppState>()
        .and_then(|s| s.last_status_push.lock().ok().map(|t| t.is_some()))
        .unwrap_or(false)
}

/// The counter half, with the clock as a parameter — same reason
/// [`ExitPromptGuard::request`] takes one.
fn exit_prevention_allowed_with(
    webview_has_spoken: bool,
    stop_in_flight: bool,
    now: std::time::Instant,
) -> bool {
    let mut guard = EXIT_PROMPT_GUARD.lock().unwrap_or_else(|e| e.into_inner());
    guard.request(webview_has_spoken, stop_in_flight, now)
}

/// C2: how many confirmed stops are draining right now. A DEPTH rather than a
/// flag so two overlapping runs cannot have the first one's completion clear
/// the second one's protection — the dialog disables Confirm while busy, but
/// that is a frontend fact and this is the last line before the process dies.
static EXIT_STOPS_IN_FLIGHT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn stop_in_flight() -> bool {
    EXIT_STOPS_IN_FLIGHT.load(Ordering::SeqCst) > 0
}

/// RAII, deliberately: a `store(false)` at the end of `exit_stop_and_close`
/// would leak the protection forever if that future were ever dropped
/// (a webview reload cancels in-flight invokes), and a permanently-true
/// `stop_in_flight` is an unbounded veto — F1 rebuilt from the other side.
struct StopInFlight;

impl StopInFlight {
    fn begin() -> Self {
        EXIT_STOPS_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        StopInFlight
    }
}

impl Drop for StopInFlight {
    fn drop(&mut self) {
        EXIT_STOPS_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
    }
}

/// A real answer arrived that did not end the process — see
/// [`ExitPromptGuard::answered`].
fn exit_prompt_run_answered() {
    EXIT_PROMPT_GUARD
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .answered();
}

/// The wire vocabulary the frontend switches on.
fn exit_action_wire(action: wsl_keepalive::ExitAction) -> &'static str {
    match action {
        wsl_keepalive::ExitAction::ExitNow => "exit_now",
        wsl_keepalive::ExitAction::PromptRunning => "prompt_running",
        wsl_keepalive::ExitAction::PromptUnknown => "prompt_unknown",
    }
}

fn current_exit_action() -> wsl_keepalive::ExitAction {
    let report = wsl_keepalive::keepalive_report();
    let presence = wsl_keepalive::presence_from(report.holding, report.last_verdict.as_deref());
    wsl_keepalive::exit_decision(dml_core::backend::selected(), presence)
}

#[tauri::command]
fn exit_intent() -> String {
    exit_action_wire(current_exit_action()).to_string()
}

/// Close anyway — the escape hatch. The user is entitled to close their
/// launcher even when the server misbehaves or the stop overruns.
#[tauri::command]
fn exit_anyway(app: tauri::AppHandle) {
    EXIT_CONFIRMED.store(true, Ordering::SeqCst);
    app.exit(0);
}

/// What [`exit_stop_and_close`] does once the stop settles. Pure, and read by
/// production — see the `after_stop(result.is_ok())` call below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterStop {
    /// The stop succeeded: latch the confirmation and close, as asked.
    CloseNow,
    /// The stop FAILED: stay up and report it. See `after_stop`.
    ReportFailure,
}

/// Pure: a stop that failed does not close the launcher.
fn after_stop(stop_ok: bool) -> AfterStop {
    if stop_ok {
        AfterStop::CloseNow
    } else {
        AfterStop::ReportFailure
    }
}

/// Stop the server the ordinary way, then close.
///
/// ORDER IS THE CONTRACT: the stop runs BEFORE the holder is released.
/// Releasing first starts the distro's 15-second clock while compose is still
/// shutting containers down, which is the ungraceful stop this whole command
/// exists to avoid. `games_stop` already gets that ordering right internally.
///
/// FIX ROUND 2 (2026-08-05) — F3: A CONFIRMED STOP THAT FAILED USED TO CLOSE
/// THE LAUNCHER WITHOUT REPORTING. The old body was `let result = games_stop(…)
/// .await; EXIT_CONFIRMED.store(true); app.exit(0); result` — unconditional.
/// Two costs. (1) `app.exit(0)` is dispatched to the event loop before the IPC
/// result reaches the webview, so `confirmExit`'s catch — the only thing that
/// would say "The stop reported a problem" — RACED a process exit. The user
/// clicked a button labelled "Stop server and close", the stop did not happen,
/// and they were told nothing. (2) `games_stop` calls
/// `wsl_keepalive::server_should_stop()` regardless of outcome, so the holder
/// was already released and the distro's 15-second clock was running while
/// containers a failed `compose down` left up were still alive.
///
/// Both are fixed here rather than in `games_stop`, whose unconditional release
/// is CORRECT for its other caller (Home's Stop button: a stop that failed
/// still means the user wants it down, and the watchdog must not go on holding
/// a distro for a server nobody wants). What changed is only this command's
/// meaning: the launcher is NOT leaving, and the server was not confirmed down,
/// so whatever survived the failed stop must not be cut by a power-off fifteen
/// seconds later — hence the re-declared Run intent on the failure arm. The
/// dialog is still open on the frontend; it reports the failure and its
/// "Close anyway" (`exit_anyway`) carries the decision, which is exactly what
/// that button exists for. A failure deliberately does NOT latch
/// `EXIT_CONFIRMED` either: latching would disarm the prompt on the next close
/// attempt, i.e. close silently with the server still up.
#[tauri::command]
async fn exit_stop_and_close(
    app: tauri::AppHandle,
    id: String,
    manage_docker: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    // FIX ROUND 3 (2026-08-05) — C1. `result.is_ok()` ALONE IS NOT AN ANSWER:
    // `run_stream` returns `Ok(code)` for every exit code and the native
    // wrapper resolves `Ok(())` by design, so the old `after_stop(result
    // .is_ok())` could never be false and `ReportFailure` was unreachable for
    // exactly the failure it exists to catch. The stream is where a lifecycle
    // failure is actually reported — see `StreamOutcome`.
    let watch = StreamOutcome::default();
    // FIX ROUND 3 (2026-08-05) — C2. From here until the stop settles, an exit
    // request is not an unanswered ask: we are INSIDE the answer. Held as an
    // RAII depth (see `StopInFlight`) so a dropped future cannot leave the veto
    // permanently armed.
    let in_flight = StopInFlight::begin();
    let result = games_stop_watched(id, manage_docker, on_event, state, watch.clone()).await;
    drop(in_flight);
    let stop_ok = result.is_ok() && !watch.failed();
    match after_stop(stop_ok) {
        AfterStop::CloseNow => {
            EXIT_CONFIRMED.store(true, Ordering::SeqCst);
            app.exit(0);
        }
        AfterStop::ReportFailure => {
            // Re-take the hold `games_stop` released on its way out. The stop
            // did not succeed, so containers may still be up, and we are no
            // longer on our way out of the process — a released holder here
            // means the distro powers off ~15s from now underneath a server
            // nobody confirmed was down.
            wsl_keepalive::server_should_run();
            // C2: the user asked, we answered, and the answer did not end the
            // process. Whatever they click next is a fresh decision taken with
            // the failure in front of them — it must not inherit a budget spent
            // waiting for an answer they have now been given.
            exit_prompt_run_answered();
        }
    }
    result
}

/// What `WindowEvent::CloseRequested` on the main window should do.
///
/// FIX ROUND 1 (2026-08-05). Before this, the window's close handler only
/// called `api.prevent_close()` when `closeToTray` was on; with it off the
/// window was DESTROYED. Confirmed against the vendored
/// `tauri-runtime-wry-2.11.4` source: destroying the last window fires
/// `RunEvent::ExitRequested` itself (`TaoWindowEvent::Destroyed`, `lib.rs`
/// ~4310-4326), independent of `app.exit()`. That landed in the
/// `ExitRequested` arm below with a running server, so `should_prompt_on_exit`
/// returned true and `api.prevent_exit()` fired -- but the window was already
/// gone, so there was nothing left to show a dialog in, `tray::show_main_window`
/// could not recreate it (`get_webview_window` returns `None` for a destroyed
/// window), and every later exit attempt re-entered the same unlatched check
/// and was prevented again: unclosable except via Task Manager, which skips
/// `RunEvent::Exit` and hands the server the exact hard WSL cut this plan
/// exists to prevent. Before this fix the same click sequence exited
/// cleanly (ungracefully, but it terminated) -- a regression, not a
/// pre-existing gap.
///
/// The fix: the window is NEVER destroyed by a close click, full stop --
/// `on_window_event` below now calls `api.prevent_close()` unconditionally,
/// before even reading `closeToTray`, so this function only ever chooses
/// between hiding (to tray), staying visible to prompt (see `PromptVisible`'s
/// own doc comment — TASK 4 FIX ROUND 1 changed this arm from hiding to
/// surfacing), or asking `app.exit(0)` to close the process the ordinary way
/// (`RunEvent::ExitRequested`, which CAN be prevented and answered, unlike a
/// window destroy). Pure and built from pieces that already exist —
/// `should_prompt_on_exit` (which itself checks `EXIT_CONFIRMED`) rather than
/// a second decision — so a confirmed exit still closes even if this is the
/// event that carries it, and the two exit surfaces cannot drift apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowCloseAction {
    /// `closeToTray` is on: hide, unconditionally. Server state and the
    /// confirm latch are irrelevant here — hiding is always safe/recoverable,
    /// which is the whole reason the setting needs no confirmation dialog.
    HideToTray,
    /// `closeToTray` is off and nothing needs protecting (or the exit is
    /// already confirmed): finish what the user asked for.
    ExitNow,
    /// `closeToTray` is off, but a server is running or its state is unknown,
    /// and the exit is not yet confirmed: SURFACE the window (never hide it)
    /// and ask, exactly like the tray path already does.
    ///
    /// TASK 4 FIX ROUND 1 (2026-08-05). This variant used to hide first, like
    /// `HideToTray` — wrong for a different reason than the destroy bug
    /// documented above: a hidden window has nowhere to show the dialog the
    /// emit asks the frontend to render, so the user's X click produced no
    /// visible change at all. "I clicked X and nothing happened" is exactly
    /// how someone reaches for Task Manager, which skips the clean shutdown
    /// entirely — the same hard WSL cut this whole plan exists to prevent.
    /// The user pressed X expecting SOMETHING to change, and only a visible
    /// window can host a confirmation, so this now surfaces instead — see
    /// `every_exit_requested_emit_is_preceded_by_a_window_surface`.
    PromptVisible,
}

/// `guard_allows` is [`exit_prevention_allowed`]'s answer — FIX ROUND 2 (F1).
/// Without it this surface kept its own unbounded opinion: with `closeToTray`
/// off, a dead webview met `PromptVisible` on every X click forever, and
/// `PromptVisible` never exits. The guard has to reach BOTH surfaces or the
/// bound is only half a bound. Callers pass `false` whenever no prompt is on
/// the table (hiding to the tray, an already-`ExitNow` action) so that a mere
/// hide never spends a prevention — see `exit_prevention_allowed`.
fn window_close_action(
    hide_to_tray: bool,
    action: wsl_keepalive::ExitAction,
    guard_allows: bool,
) -> WindowCloseAction {
    if hide_to_tray {
        return WindowCloseAction::HideToTray;
    }
    if guard_allows && should_prompt_on_exit(action) {
        WindowCloseAction::PromptVisible
    } else {
        WindowCloseAction::ExitNow
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    use tauri::Emitter;

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
            soap_env_sync: Arc::new(Mutex::new(dml_core::soap_env::SoapEnvSync::default())),
        })
        .setup(|app| {
            // (Task 8 removed the startup registry prefetch here: the config/
            // tuning/module-catalog registries are now embedded in dml-wow —
            // see `dml_wow::registry` — so there is nothing left to warm.)
            tray::build(app.handle())?;
            if let Some(l) = instance_lock {
                single_instance::serve(l, app.handle().clone());
            }

            // WSL keep-alive (Arch backend only; a no-op everywhere else). WSL
            // powers a distro off ~15s after the last session into it exits,
            // whatever is running inside — see wsl_keepalive.rs. This arms the
            // watchdog thread; nothing is held until something declares the
            // server is meant to be running. Reads the backend from
            // `backend::selected()` for the same reason AppState does: a
            // backend change needs a relaunch, so the value cannot go stale
            // underneath us.
            wsl_keepalive::install(app.handle(), dml_wow::backend::selected());

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
                // ALWAYS prevent the destroy, unconditionally, before even
                // reading `closeToTray`. See `WindowCloseAction`'s doc
                // comment: a destroyed window can never come back, and that
                // is what let a running server end up unprotected here.
                api.prevent_close();

                // Read the preference fresh rather than caching it: the user
                // can change it in Settings without restarting, and a window
                // close is rare enough that a small file read is free.
                let hide_to_tray = dml_core::util::dml_home_dir()
                    .map(|h| dml_core::launcher_config::load(&h).close_to_tray)
                    .unwrap_or(true);
                let action = current_exit_action();
                // FIX ROUND 2 (F1). The guard is consulted — and a prevention
                // SPENT — only when a dialog is actually on the table: not
                // when we are merely hiding to the tray, and not when the
                // action needs no prompt at all. Ordinary X-clicks with
                // `closeToTray` on are the common case and must stay free.
                let guard_allows = !hide_to_tray
                    && should_prompt_on_exit(action)
                    && exit_prevention_allowed(window.app_handle());

                match window_close_action(hide_to_tray, action, guard_allows) {
                    WindowCloseAction::HideToTray => {
                        // HIDE, never destroy. The webview must keep running:
                        // it owns the 7s status poll that feeds the tray, and
                        // the auto-shutdown toggle is re-asserted to Rust
                        // from its onMount. Destroying it would silently
                        // kill both.
                        let _ = window.hide();
                    }
                    WindowCloseAction::PromptVisible => {
                        // SURFACE, never hide (TASK 4 FIX ROUND 1). The user
                        // just clicked X on a visible window; hiding it here
                        // and then asking a question of the hidden result is
                        // exactly the bug this round fixes. show_main_window
                        // also unminimizes/focuses, which additionally
                        // covers a minimized window, not only the
                        // already-visible common case. Every
                        // "exit-requested" emit site does this immediately
                        // before emitting -- see
                        // every_exit_requested_emit_is_preceded_by_a_window_surface.
                        tray::show_main_window(window.app_handle());
                        let _ = window.app_handle().emit("exit-requested", exit_action_wire(action));
                    }
                    WindowCloseAction::ExitNow => {
                        // Through `app.exit(0)` -- the SAME path Tray Quit
                        // and a confirmed dialog use -- rather than letting
                        // this event's own close proceed. One way to actually
                        // leave the process, not two that could drift.
                        window.app_handle().exit(0);
                    }
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
            wsl_keepalive_status,
            autostart_get,
            autostart_set,
            exit_intent,
            exit_stop_and_close,
            exit_anyway
        ])
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app, event| match event {
            // THE HOOK THAT CAN SAY NO. `RunEvent::Exit` fires too late to ask
            // anything — by then the decision is made. Tray Quit routes through
            // `app.exit(0)` (tray.rs:90), so it reaches this same arm and there
            // is no second path to maintain.
            tauri::RunEvent::ExitRequested { api, .. } => {
                let action = current_exit_action();
                // FIX ROUND 2 (F1): `exit_prevention_allowed` is what keeps
                // this from being an unbounded veto. Short-circuited on
                // purpose — an action that needs no prompt must not spend a
                // prevention. See MAX_UNANSWERED_EXIT_PREVENTIONS for the
                // property this guarantees.
                if should_prompt_on_exit(action) && exit_prevention_allowed(app) {
                    api.prevent_exit();
                    // SURFACE before asking (TASK 4 FIX ROUND 1, 2026-08-05).
                    // Tray Quit (tray.rs:90) is the only production caller
                    // that reaches this arm with a real prompt, and
                    // `closeToTray` defaults ON -- so "close the window
                    // (hides to tray, no prompt -- correct), then Quit from
                    // the tray icon" is the DEFAULT path through this whole
                    // feature, not an edge case. It used to leave the window
                    // hidden while the frontend dutifully opened a dialog
                    // nobody could see: a tray icon that ignores a click.
                    // Same show_main_window the sibling tray_open/
                    // tray_start/tray_stop branches already use
                    // (tray.rs:76,83).
                    tray::show_main_window(app);
                    let _ = app.emit("exit-requested", exit_action_wire(action));
                }
            }
            tauri::RunEvent::Exit => {
                power::keep_awake(false);
                // Release the held WSL session. THE POLITE PATH ONLY: an abrupt
                // kill never reaches here, which is why the child is also in a
                // KILL_ON_JOB_CLOSE job object.
                wsl_keepalive::shutdown();
            }
            _ => {}
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    // Brings the bare `ExitAction` used by the exit-prompt tests below into
    // scope. Production code always spells it out as `wsl_keepalive::ExitAction`
    // (see `should_prompt_on_exit`/`exit_action_wire`/`current_exit_action`), so
    // this import has no production-side use and lives here rather than at the
    // crate root, where it would be an unused-import warning on every non-test
    // build.
    use super::wsl_keepalive::ExitAction;

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
    ///
    /// Takes the directory as an ARGUMENT rather than exporting
    /// `DML_GAMES_DIR`. The env version mutated process-global state inside a
    /// test binary whose threads run in parallel, while `native_title_count`
    /// and `native_facts` read that same variable in the same binary — the
    /// flake generator an earlier task removed from `games_dir_from`, one
    /// function along. Nothing here can now race anything.
    #[test]
    fn an_absent_games_dir_holds_zero_titles_not_an_unknown_number() {
        let missing = std::env::temp_dir().join(format!("dml-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&missing);
        assert!(!missing.exists());
        assert_eq!(
            native_title_count_in(&missing),
            Some(0),
            "a games dir that is not there holds zero titles"
        );
    }

    /// The other half: a directory we CANNOT read is a genuine "could not
    /// tell", and must stay `None`. Asserted through the same seam, so the two
    /// answers are shown to differ rather than assumed to.
    #[test]
    fn a_games_dir_that_is_a_file_is_unknown_not_zero() {
        // A path that exists but is not a directory: `read_dir` fails with a
        // kind that is NOT NotFound, which is the branch that must answer None.
        let f = std::env::temp_dir().join(format!("dml-notadir-{}", std::process::id()));
        std::fs::write(&f, b"not a directory").unwrap();
        let got = native_title_count_in(&f);
        let _ = std::fs::remove_file(&f);
        assert_eq!(
            got, None,
            "a games dir we could not read is 'we could not tell', never 'you have no titles'"
        );
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
            dml_wow_bin_present: false,
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

    // -- exit / close-to-tray: ask before stopping the server (Task 3) -------
    //
    // `EXIT_CONFIRMED` is process-global `static` state and the two tests
    // below both flip it. `cargo test` runs test functions on separate
    // threads by default, so without serializing them one test's
    // `store(true, ...)` could land between another's `store(false, ...)` and
    // its assertion -- a flaky failure with nothing to do with the code under
    // test. `wsl_keepalive`'s own `STATE` `OnceLock` hit the identical hazard
    // in Task 2; the fix here is the same shape: a private lock held for each
    // racey test's whole body, rather than anything touching the production
    // type. `the_exit_intent_wire_values_are_stable` does not touch
    // `EXIT_CONFIRMED` at all, so it does not need the lock.
    static EXIT_CONFIRMED_TEST_LOCK: Mutex<()> = Mutex::new(());

    /// Re-entrancy. Confirming the dialog calls `app.exit(0)`, which fires
    /// `ExitRequested` a SECOND time. Without the latch that second pass would
    /// prompt again and the launcher could never close.
    #[test]
    fn a_confirmed_exit_is_not_prompted_a_second_time() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
        assert!(should_prompt_on_exit(ExitAction::PromptRunning), "first pass asks");
        EXIT_CONFIRMED.store(true, Ordering::SeqCst);
        assert!(
            !should_prompt_on_exit(ExitAction::PromptRunning),
            "once confirmed, every later ExitRequested must pass straight through"
        );
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn exit_now_never_prompts_whatever_the_latch_says() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for latched in [false, true] {
            EXIT_CONFIRMED.store(latched, Ordering::SeqCst);
            assert!(!should_prompt_on_exit(ExitAction::ExitNow));
        }
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
    }

    /// The wire strings the frontend switches on. A rename here is a silent
    /// UI break, so they are pinned.
    #[test]
    fn the_exit_intent_wire_values_are_stable() {
        assert_eq!(exit_action_wire(ExitAction::ExitNow), "exit_now");
        assert_eq!(exit_action_wire(ExitAction::PromptRunning), "prompt_running");
        assert_eq!(exit_action_wire(ExitAction::PromptUnknown), "prompt_unknown");
    }

    // -- window close (Fix round 1): never destroy, hide only for tray -----
    //
    // `window_close_action` calls `should_prompt_on_exit`, which reads
    // `EXIT_CONFIRMED` -- so every test here takes `EXIT_CONFIRMED_TEST_LOCK`
    // too, for the same reason the two tests above do.

    /// `closeToTray` ON always hides, whatever the server is doing and
    /// whatever the confirm latch says -- unchanged by this fix. Pinned so a
    /// future edit to this function cannot silently start asking a question
    /// the user turned off.
    #[test]
    fn hide_to_tray_wins_regardless_of_server_state_or_the_latch() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        for latched in [false, true] {
            EXIT_CONFIRMED.store(latched, Ordering::SeqCst);
            for action in [ExitAction::ExitNow, ExitAction::PromptRunning, ExitAction::PromptUnknown] {
                for guard_allows in [false, true] {
                    assert_eq!(window_close_action(true, action, guard_allows), WindowCloseAction::HideToTray);
                }
            }
        }
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
    }

    /// `closeToTray` OFF with nothing running closes cleanly — unchanged.
    #[test]
    fn close_to_tray_off_with_nothing_running_exits() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
        assert_eq!(window_close_action(false, ExitAction::ExitNow, true), WindowCloseAction::ExitNow);
    }

    /// THE REGRESSION TASK 3 FIXED. `closeToTray` OFF with a server running
    /// or its state unknown must never destroy — a destroyed window can
    /// never come back (`get_webview_window` returns `None`), which is what
    /// made the process unclosable except via Task Manager, which skips
    /// `RunEvent::Exit` and hands the server the exact hard WSL cut this plan
    /// exists to prevent. Renamed from
    /// `close_to_tray_off_with_a_server_hides_and_asks_instead_of_destroying`
    /// in TASK 4 FIX ROUND 1: this branch no longer hides at all (see
    /// `PromptVisible`'s doc comment) — it stays visible and asks, instead
    /// of destroying.
    #[test]
    fn close_to_tray_off_with_a_server_prompts_visibly_instead_of_destroying() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
        for action in [ExitAction::PromptRunning, ExitAction::PromptUnknown] {
            assert_eq!(window_close_action(false, action, true), WindowCloseAction::PromptVisible);
        }
    }

    /// A latched (already-confirmed) close must still close even when THIS
    /// `WindowEvent::CloseRequested` is the one carrying it — otherwise a
    /// confirm followed by a second click on X, or a slow
    /// `exit_stop_and_close` still in flight, could re-prompt forever.
    /// `window_close_action` gets this for free by calling
    /// `should_prompt_on_exit` rather than re-deciding independently — this
    /// test is what makes that a proven property instead of an assumption.
    #[test]
    fn a_confirmed_exit_still_closes_from_the_window_path_too() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXIT_CONFIRMED.store(true, Ordering::SeqCst);
        assert_eq!(window_close_action(false, ExitAction::PromptRunning, true), WindowCloseAction::ExitNow);
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
    }

    // -- FIX ROUND 2, F1: the launcher can always be closed ------------------
    //
    // The bug: `prevent_exit()` had no bound and no fallback, and the condition
    // that reached it was CAUSED by the same dead webview that could not answer
    // it (`last_verdict: None` -> `Unknown` -> `PromptUnknown` -> prompt). Tray
    // Quit did nothing, X was `prevent_close()`d, and no UI path closed the
    // process. See `MAX_UNANSWERED_EXIT_PREVENTIONS` for the property below.

    /// HALF ONE. A webview that has never pushed a status cannot render a
    /// dialog, so its exit is never prevented — whatever the count says. This
    /// is the `tauri dev`-with-vite-down / broken-WebView2 case, where the
    /// first click must close, exactly as it did before this plan existed.
    #[test]
    fn a_webview_that_never_spoke_is_never_prevented_from_closing() {
        for prevented in 0..=(MAX_UNANSWERED_EXIT_PREVENTIONS + 3) {
            assert!(
                !may_prevent_exit(false, prevented, false),
                "an exit was prevented with no evidence a webview exists to answer it \
                 (prevented_in_window={prevented}) — that is the unclosable launcher"
            );
        }
    }

    /// HALF TWO. Even a webview that DOES speak only buys a bounded number of
    /// preventions — the Task-4-style regression (poll alive, dialog broken)
    /// sails past half one and would otherwise veto forever.
    #[test]
    fn preventions_are_bounded_even_for_a_speaking_webview() {
        for prevented in 0..MAX_UNANSWERED_EXIT_PREVENTIONS {
            assert!(may_prevent_exit(true, prevented, false), "prevented={prevented} must still ask");
        }
        for prevented in MAX_UNANSWERED_EXIT_PREVENTIONS..(MAX_UNANSWERED_EXIT_PREVENTIONS + 4) {
            assert!(
                !may_prevent_exit(true, prevented, false),
                "prevented={prevented} is past the bound and must let the process go"
            );
        }
    }

    /// THE PROPERTY, stated as a sequence rather than a table: **no run of exit
    /// requests can be prevented forever.** This is the one that fails if
    /// either half of `may_prevent_exit` is deleted, and it is driven through
    /// the real `ExitPromptGuard` (the counter production mutates), not through
    /// the pure predicate alone.
    #[test]
    fn repeated_exit_requests_always_reach_an_exit() {
        for webview_has_spoken in [false, true] {
            let mut guard = ExitPromptGuard { prevented: 0, last_request: None };
            let t0 = std::time::Instant::now();
            let mut prevented = 0u32;
            let mut let_go = false;
            // Twenty asks inside ONE window (1s apart, window is 60s), i.e. the
            // user clicking Quit over and over because nothing is happening.
            for i in 0..20u32 {
                let now = t0 + std::time::Duration::from_secs(u64::from(i));
                if guard.request(webview_has_spoken, false, now) {
                    prevented += 1;
                } else {
                    let_go = true;
                    break;
                }
            }
            assert!(
                let_go,
                "spoken={webview_has_spoken}: twenty consecutive exit requests were ALL \
                 prevented — there is no path out of the process except Task Manager, \
                 which skips RunEvent::Exit and hard-cuts the distro"
            );
            assert!(
                prevented <= MAX_UNANSWERED_EXIT_PREVENTIONS,
                "spoken={webview_has_spoken}: {prevented} preventions before giving up, \
                 bound is {MAX_UNANSWERED_EXIT_PREVENTIONS}"
            );
        }
    }

    /// ONCE IT LETS GO IT STAYS LET GO, within the same run of asks. A guard
    /// that re-armed on the next request would make the escape a coin flip:
    /// click, click, click-closes, and the NEXT launcher session traps again on
    /// its second click.
    #[test]
    fn the_guard_does_not_rearm_inside_the_same_run_of_asks() {
        let mut guard = ExitPromptGuard { prevented: 0, last_request: None };
        let t0 = std::time::Instant::now();
        for i in 0..MAX_UNANSWERED_EXIT_PREVENTIONS {
            assert!(guard.request(true, false, t0 + std::time::Duration::from_secs(u64::from(i))));
        }
        for i in MAX_UNANSWERED_EXIT_PREVENTIONS..(MAX_UNANSWERED_EXIT_PREVENTIONS + 5) {
            assert!(
                !guard.request(true, false, t0 + std::time::Duration::from_secs(u64::from(i))),
                "request {i} re-armed the veto inside the same run"
            );
        }
    }

    /// THE WINDOW, and why it is not a knob. Cancel touches no Rust command, so
    /// a cancelled dialog is indistinguishable from a dead one at this
    /// boundary. Without the quiet-gap reset, three cancels spread across a
    /// working six-hour session would disarm the fourth Quit's dialog and
    /// hard-cut a live server — the exact harm this plan exists to prevent,
    /// rebuilt out of its own fix.
    #[test]
    fn a_quiet_gap_starts_a_fresh_run_of_asks() {
        let mut guard = ExitPromptGuard { prevented: 0, last_request: None };
        let mut now = std::time::Instant::now();
        for _ in 0..MAX_UNANSWERED_EXIT_PREVENTIONS {
            assert!(guard.request(true, false, now));
            now += std::time::Duration::from_secs(1);
        }
        assert!(!guard.request(true, false, now), "the bound must be reachable at all");
        now += EXIT_REQUEST_WINDOW;
        assert!(
            guard.request(true, false, now),
            "an exit request a full window later is a NEW ask and must still be able to \
             protect a running server"
        );
    }

    /// The guard reaches the window-close surface too. `PromptVisible` never
    /// exits, so a `closeToTray`-off X click against a dead webview was its own
    /// unbounded veto — the bound has to hold on BOTH surfaces or it is half a
    /// bound.
    #[test]
    fn the_window_path_closes_once_the_guard_has_let_go() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
        for action in [ExitAction::PromptRunning, ExitAction::PromptUnknown] {
            assert_eq!(
                window_close_action(false, action, false),
                WindowCloseAction::ExitNow,
                "{action:?}: with the guard spent, X must close the process rather than \
                 prompt a webview that cannot answer"
            );
        }
    }

    // -- FIX ROUND 3, C2: the bound must not run while we are answering ------

    /// THE BUG, REPRODUCED. Confirm -> `exit_stop_and_close` awaits
    /// `games_stop` (tens of seconds at ~2,000 bots) and `EXIT_CONFIRMED` does
    /// not latch until it settles. Click 2 spent a prevention and, because the
    /// frontend drops it (`if (exitGuard.busy) return`) on an already-visible
    /// window, produced NO visible change at all. Click 3 inside the same 60s
    /// window met the bound, was not prevented, and killed the process
    /// mid-`compose down` — holder released, distro off ~15s later. Twenty asks
    /// here, not three, because "it survived exactly three" is a coincidence
    /// and "it survives the whole stop" is the property.
    #[test]
    fn clicks_during_a_confirmed_stop_never_reach_an_exit() {
        let mut guard = ExitPromptGuard { prevented: 0, last_request: None };
        let t0 = std::time::Instant::now();
        for i in 0..20u32 {
            let now = t0 + std::time::Duration::from_secs(2 * u64::from(i));
            assert!(
                guard.request(true, true, now),
                "ask {i} was let through while a confirmed stop was still draining. That \
                 kills the process mid-`compose down`, releases the WSL holder and powers \
                 the distro off ~15s later — the exact harm the bound was added to bound."
            );
        }
    }

    /// C2 HAS TWO HALVES AND THEY MASK EACH OTHER. Found by mutation while
    /// writing this round: deleting the `stop_in_flight` override from
    /// `may_prevent_exit` left the sequence test above GREEN, because the
    /// second half (not spending the budget) keeps `prevented` at zero, so the
    /// ordinary rule happens to answer yes for the first two clicks and then
    /// forever. The override is only load-bearing where the ordinary rule says
    /// NO — a budget already spent, or a webview `last_status_push` has not
    /// heard from yet. That is the case this test states, and it is the only
    /// one that reddens when the override goes.
    #[test]
    fn a_draining_stop_outranks_both_other_inputs() {
        for spoken in [false, true] {
            for prevented in 0..=(MAX_UNANSWERED_EXIT_PREVENTIONS + 3) {
                assert!(
                    may_prevent_exit(spoken, prevented, true),
                    "spoken={spoken} prevented={prevented}: an exit was allowed through \
                     while a confirmed stop was draining. Both other inputs answer \"is \
                     there any point asking?\"; this one answers \"are we already \
                     executing the answer?\", and killing the process mid-`compose down` \
                     is the harm, not the remedy."
                );
            }
        }
    }

    /// AND IT COSTS NOTHING. Suspending the bound is only half the fix: if
    /// those clicks still spent the budget, the very next ask after the stop
    /// settled would find it exhausted and the same death would land one click
    /// later. Asserted WITHOUT `answered()`, so this holds even on a path that
    /// forgets to reset.
    #[test]
    fn a_stop_in_flight_does_not_spend_the_budget() {
        let mut guard = ExitPromptGuard { prevented: 0, last_request: None };
        let mut now = std::time::Instant::now();
        for _ in 0..(MAX_UNANSWERED_EXIT_PREVENTIONS + 3) {
            assert!(guard.request(true, true, now));
            now += std::time::Duration::from_secs(1);
        }
        for i in 0..MAX_UNANSWERED_EXIT_PREVENTIONS {
            assert!(
                guard.request(true, false, now),
                "ask {i} after the stop settled found the budget already spent — the \
                 in-flight clicks were counted after all"
            );
            now += std::time::Duration::from_secs(1);
        }
        assert!(
            !guard.request(true, false, now),
            "the ordinary bound must still be reachable — a stop-in-flight that permanently \
             disarmed the counter would be F1 rebuilt from the other side"
        );
    }

    /// AN ANSWER THAT DID NOT END THE PROCESS STARTS A FRESH RUN. The failure
    /// arm (C1) is the one call where the old justification — "both answers end
    /// the process, so nothing needs to reset the count" — is simply false.
    #[test]
    fn an_answer_that_did_not_close_starts_a_fresh_run() {
        let mut guard = ExitPromptGuard { prevented: 0, last_request: None };
        let mut now = std::time::Instant::now();
        for _ in 0..MAX_UNANSWERED_EXIT_PREVENTIONS {
            assert!(guard.request(true, false, now));
            now += std::time::Duration::from_secs(1);
        }
        assert!(!guard.request(true, false, now), "the bound must be reachable at all");
        guard.answered();
        assert!(
            guard.request(true, false, now + std::time::Duration::from_secs(1)),
            "the stop failed and the launcher stayed up; the next click is a NEW decision \
             taken with the failure on screen and must not inherit a spent budget"
        );
    }

    /// THE DEPTH IS RELEASED BY DROP, and an inner run finishing does not clear
    /// an outer one. A `store(false)` at the end of `exit_stop_and_close` would
    /// leak the protection forever if that future were dropped (a webview
    /// reload cancels in-flight invokes) — and a permanently-true
    /// `stop_in_flight` is an unbounded veto, F1 rebuilt.
    ///
    /// Shares `EXIT_CONFIRMED_TEST_LOCK` because it mutates a process-global
    /// the exit tests read; the crate runs as one binary with parallel threads.
    #[test]
    fn the_in_flight_marker_is_released_by_drop() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        assert!(!stop_in_flight(), "leaked from an earlier test");
        {
            let _outer = StopInFlight::begin();
            assert!(stop_in_flight());
            {
                let _inner = StopInFlight::begin();
                assert!(stop_in_flight());
            }
            assert!(
                stop_in_flight(),
                "an inner stop finishing cleared the outer one's protection — a depth, not \
                 a flag, is exactly what stops that"
            );
        }
        assert!(
            !stop_in_flight(),
            "the marker outlived its guard. A stop_in_flight that never clears prevents \
             every exit forever."
        );
    }

    /// THE ESCAPE HATCH IS UNAFFECTED, which is what makes suspending the bound
    /// safe. "Close anyway" latches `EXIT_CONFIRMED` first, and
    /// `should_prompt_on_exit` short-circuits on that latch before the guard is
    /// consulted at all — so it closes during a stop exactly as it does
    /// outside one.
    #[test]
    fn close_anyway_still_closes_while_a_stop_is_in_flight() {
        let _guard = EXIT_CONFIRMED_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _in_flight = StopInFlight::begin();
        EXIT_CONFIRMED.store(true, Ordering::SeqCst);
        for action in [ExitAction::PromptRunning, ExitAction::PromptUnknown] {
            assert!(
                !should_prompt_on_exit(action),
                "{action:?}: a confirmed exit must not be prompted again, stop or no stop"
            );
            assert_eq!(
                window_close_action(false, action, true),
                WindowCloseAction::ExitNow,
                "{action:?}: Close anyway must leave, or a hung stop traps the user"
            );
        }
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
    }

    // -- FIX ROUND 2, F3: a confirmed stop that FAILS does not close ---------

    /// The whole decision, and production reads this exact function — see
    /// `exit_stop_and_close`'s `after_stop(result.is_ok())` and the wiring test
    /// `a_failed_confirmed_stop_does_not_reach_the_exit_call`, which is what
    /// keeps this from being a pure list nothing consults.
    #[test]
    fn a_failed_stop_reports_instead_of_closing() {
        assert_eq!(after_stop(true), AfterStop::CloseNow);
        assert_eq!(
            after_stop(false),
            AfterStop::ReportFailure,
            "the user clicked \"Stop server and close\", the stop did not happen, and \
             closing anyway tells them nothing while the holder is already released"
        );
    }

    // -- FIX ROUND 3, C1: what `stop_ok` is actually made of -----------------

    /// THE PREMISE THE FIX ROUND 2 SHAPE GOT WRONG, stated as a test.
    ///
    /// `after_stop` was fed `result.is_ok()`, which reads as a branch and is a
    /// constant. A lifecycle failure travels in the STREAM: bash's `dml`
    /// emits `{"event":"error", …}` and exits non-zero, and
    /// `dml_wow::lifecycle::games_lifecycle_stream` (the native path) emits
    /// the same event and reports nothing else at all. So the observer is what
    /// the decision has to rest on.
    #[test]
    fn an_error_event_marks_the_stream_failed() {
        let w = StreamOutcome::default();
        assert!(!w.failed(), "a stream that has said nothing has not failed");
        w.observe(&serde_json::json!({"event": "section_start", "name": "stop"}));
        w.observe(&serde_json::json!({"event": "line", "text": "Stopping ac-worldserver"}));
        assert!(!w.failed(), "ordinary progress is not a failure");
        w.observe(&serde_json::json!({
            "event": "error",
            "error": {"code": "COMPOSE_FAILED", "message": "down failed", "hint": ""}
        }));
        assert!(w.failed(), "an error event is the failure signal on BOTH backends");
    }

    /// A `done` event is not a failure, and the check is on the EVENT NAME
    /// rather than on the presence of an `error` key — a `done` payload that
    /// happens to carry a nested `error` field must not be read as a failure.
    #[test]
    fn a_done_event_is_not_a_failure() {
        let w = StreamOutcome::default();
        w.observe(&serde_json::json!({"event": "done", "data": {"id": "wow", "error": null}}));
        assert!(!w.failed(), "the stop finished — that is the success shape");
    }

    /// THE SECOND SIGNAL. A CLI that dies before emitting anything has no
    /// event to read, and `run_stream` still hands back `Ok(code)` — the exact
    /// return that made `result.is_ok()` a constant.
    #[test]
    fn a_nonzero_exit_code_marks_the_stream_failed() {
        let w = StreamOutcome::default();
        w.note_exit_code(0);
        assert!(!w.failed(), "exit 0 is success");
        w.note_exit_code(3);
        assert!(w.failed(), "a non-zero exit with no error event is still a failed stop");
    }

    /// The two signals share one flag, and NEITHER can clear the other. A
    /// failure observed mid-stream must survive the `Ok(0)` that a CLI which
    /// reported an error and then exited cleanly would hand back.
    #[test]
    fn a_clean_exit_code_cannot_clear_an_observed_failure() {
        let w = StreamOutcome::default();
        w.observe(&serde_json::json!({"event": "error", "error": {"code": "X"}}));
        w.note_exit_code(0);
        assert!(
            w.failed(),
            "the stop reported an error and then exited 0 — that is a FAILED stop, and \
             letting the exit code overwrite it rebuilds C1 from the other side"
        );
    }

    /// The observer is shared by clone (production hands one copy to the
    /// streaming closure and keeps another), so a failure seen by the closure
    /// has to be visible to the caller that decides whether to close.
    #[test]
    fn the_observer_is_shared_across_clones() {
        let w = StreamOutcome::default();
        let in_closure = w.clone();
        in_closure.observe(&serde_json::json!({"event": "error", "error": {"code": "X"}}));
        assert!(
            w.failed(),
            "the clone the streaming closure holds does not share state with the one \
             exit_stop_and_close reads — the failure would be observed and then dropped"
        );
    }
}

/// T2 — IS EVERY CALL SITE IN THIS FILE CLASSIFIED?
///
/// The vocabulary bug shipped because nothing on either side compared the
/// launcher's verbs with `dml-wow`'s. T1 (in `dml-wow-cli`) pins the CLI end to
/// the clap derive; this pins the LAUNCHER end to the launcher's own source. The
/// hand-written `dml_core::vocab::TABLE` sits between them and is red the moment
/// either end moves without it.
///
/// It cannot live in `dml-core` — that crate is the game-agnostic bottom layer
/// and must not know the launcher's source tree exists (a settled ruling, see
/// the `CLI_BAD_OUTPUT` hint in `error.rs`); an `include_str!` reaching up into
/// `launcher/src` would red `cargo test -p dml-core`, ubuntu CI included.
/// CONSEQUENCE, stated rather than discovered: this test runs on the WINDOWS CI
/// job only, because the ubuntu job builds the three crates and not the
/// launcher. Do not later read a green ubuntu run as coverage for it.
#[cfg(test)]
pub(crate) mod vocab_coverage_tests {
    /// Rust source with comments removed. MANDATORY, and this repo has been
    /// bitten TWICE by skipping it: `feature-keys.test.ts` read a comment as a
    /// call site, and `Test-InstallerNative.ps1` read the installer's own
    /// explanation of what it does NOT do as evidence that it does. This very
    /// file is dense with prose naming `games list` and `wow server-detail`.
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
    /// Deliberately NOT "cut at the first `#[cfg(test)]`", which is what
    /// [`production_half`] used to do: this file carries THREE test modules with
    /// production code above them, and `dml-core/src/engine.rs` has two with
    /// production code BETWEEN them. Truncating at the first silently stops
    /// scanning everything below — a hole in the one direction that makes a
    /// guard useless, because it makes coverage depend on where in the file you
    /// happen to type. An item whose attribute is followed by a `;` before any
    /// `{` (`#[cfg(test)] use …;`) is cut at the semicolon instead.
    ///
    /// Input must already be comment-stripped, so a `{` inside prose cannot
    /// unbalance the match.
    ///
    /// `pub(crate)` and living HERE, beside [`strip_comments`], because it was
    /// implemented 700 lines away in `startup.rs` while `production_half` did
    /// the wrong thing — two answers to one question, in one crate, is how the
    /// hole survived. One home, both scans.
    pub(crate) fn strip_cfg_test(code: &str) -> String {
        let attr: Vec<char> = "#[cfg(test)]".chars().collect();
        let b: Vec<char> = code.chars().collect();
        let mut out = String::with_capacity(code.len());
        let mut i = 0usize;
        while i < b.len() {
            if b[i..].starts_with(&attr[..]) {
                i = end_of_cfg_test_item(&b, i + attr.len());
                continue;
            }
            out.push(b[i]);
            i += 1;
        }
        out
    }

    /// One past the end of the item the attribute ending at `from` decorates.
    fn end_of_cfg_test_item(b: &[char], from: usize) -> usize {
        let mut i = from;
        let mut depth = 0usize;
        while i < b.len() {
            match b[i] {
                // String and char literals may hold braces and semicolons.
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

    fn skip_ws(b: &[u8], mut i: usize) -> usize {
        while i < b.len() && (b[i] as char).is_whitespace() {
            i += 1;
        }
        i
    }

    /// The leading run of STRING LITERALS from `from` — the verb. Stops at the
    /// first token that is not one (a variable, a `format!`, a closing bracket)
    /// or at a `-`-prefixed literal, which is a flag rather than a verb token.
    fn leading_literals(code: &str, from: usize) -> Vec<String> {
        let b = code.as_bytes();
        let mut i = from;
        let mut out: Vec<String> = Vec::new();
        loop {
            i = skip_ws(b, i);
            if i >= b.len() {
                break;
            }
            let c = b[i] as char;
            if c == ',' || c == '&' {
                i += 1;
                continue;
            }
            if c != '"' {
                break;
            }
            i += 1;
            let start = i;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            let lit = code[start..i.min(code.len())].to_string();
            i += 1;
            // Skip any method chain on the literal (`.to_string()`, `.into()`)
            // by MATCHING PARENS. Scanning to the next comma instead reads
            // `.into()`'s own `)` as the end of the argv list, which silently
            // truncated every verb to its first token.
            loop {
                let mut j = skip_ws(b, i);
                if j >= b.len() || b[j] != b'.' {
                    break;
                }
                j += 1;
                while j < b.len() && ((b[j] as char).is_alphanumeric() || b[j] == b'_') {
                    j += 1;
                }
                if j < b.len() && b[j] == b'(' {
                    let mut d = 1i32;
                    j += 1;
                    while j < b.len() && d > 0 {
                        match b[j] {
                            b'(' => d += 1,
                            b')' => d -= 1,
                            _ => {}
                        }
                        j += 1;
                    }
                }
                i = j;
            }
            if lit.starts_with('-') {
                break;
            }
            out.push(lit);
        }
        out
    }

    fn ident_at(code: &str, i: usize) -> Option<(String, usize)> {
        let b = code.as_bytes();
        let mut j = i;
        while j < b.len() && ((b[j] as char).is_alphanumeric() || b[j] == b'_') {
            j += 1;
        }
        if j == i {
            None
        } else {
            Some((code[i..j].to_string(), j))
        }
    }

    /// The three helpers that WRAP a runner method. A call inside one of these
    /// is not a call site — its argv comes from the caller, and every caller is
    /// enumerated separately.
    const HELPERS: [&str; 3] = ["run_json_cmd", "stream_args", "stream_action"];

    /// `(start offset, name)` of the item-level `fn` containing `at`.
    ///
    /// LOAD-BEARING, and its absence was a real bug: without it the backward
    /// search for `let args = vec![…]` walked out of the function and into a
    /// DIFFERENT one, so `stream_args`' own body resolved to some unrelated
    /// caller's argv and reported a verb that call site never sends.
    fn enclosing_fn(code: &str, at: usize) -> Option<(usize, String)> {
        let mut best: Option<(usize, String)> = None;
        let mut from = 0usize;
        while let Some(rel) = code[from..].find("fn ") {
            let m = from + rel;
            from = m + 3;
            if m >= at {
                break;
            }
            let line_start = code[..m].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let prefix = code[line_start..m].trim();
            let is_item = matches!(
                prefix,
                "" | "pub" | "async" | "pub async" | "unsafe" | "pub unsafe" | "pub(crate)" | "pub(crate) async"
            );
            if !is_item {
                continue;
            }
            if let Some((name, _)) = ident_at(code, m + 3) {
                best = Some((line_start, name));
            }
        }
        best
    }

    /// Most call sites pass a `Vec` the caller built (`let mut args = vec![…]`
    /// then `run_json_cmd(state, args)`), so an identifier argument is resolved
    /// back to its binding — searching ONLY within the enclosing function.
    /// Two hops, because some sites go through `let refs: Vec<&str> = args.iter()…`.
    ///
    /// Returns the chain that was walked, so an UNRESOLVABLE site gets a label
    /// that survives the lines around it moving.
    fn resolve_binding(code: &str, ident: &str, before: usize) -> (Option<Vec<String>>, String) {
        let scope = enclosing_fn(code, before).map(|(s, _)| s).unwrap_or(0);
        let mut chain = ident.to_string();
        let mut name = ident.to_string();
        for _ in 0..3 {
            let hay = &code[scope..before];
            let mut rhs: Option<usize> = None;
            let mut from = 0usize;
            while let Some(rel) = hay[from..].find("let ") {
                let at = from + rel + 4;
                from = at;
                let abs = scope + at;
                let b = code.as_bytes();
                let mut k = skip_ws(b, abs);
                if code[k..].starts_with("mut ") {
                    k = skip_ws(b, k + 4);
                }
                let Some((got, after)) = ident_at(code, k) else { continue };
                if got != name {
                    continue;
                }
                let mut m = skip_ws(b, after);
                if m < b.len() && b[m] == b':' {
                    while m < b.len() && b[m] != b'=' {
                        m += 1;
                    }
                }
                m = skip_ws(b, m);
                if m >= b.len() || b[m] != b'=' {
                    continue;
                }
                rhs = Some(skip_ws(b, m + 1)); // nearest preceding wins
            }
            let Some(p) = rhs else { return (None, chain) };
            if code[p..].starts_with("vec![") {
                return (Some(leading_literals(code, p + 5)), chain);
            }
            let Some((next, _)) = ident_at(code, p) else { return (None, chain) };
            chain.push_str("->");
            chain.push_str(&next);
            name = next;
        }
        (None, chain)
    }

    /// Every argv literal that reaches a `DmlRunner` method, a label for each
    /// site whose verb could NOT be read off the source, and how many calls sat
    /// inside one of the three [`HELPERS`].
    fn extract(code: &str) -> (Vec<Vec<String>>, Vec<String>, usize) {
        let b = code.as_bytes();
        let mut found: Vec<Vec<String>> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        let mut helper_bodies = 0usize;
        // (call token, leading args to skip before the argv argument).
        // These are the SHAPES, not bare verb greps: a site must literally be a
        // call to one of the runner methods (or to the two helpers that wrap
        // them) to be read at all.
        let shapes: [(&str, usize); 7] = [
            ("run_json_cmd(", 1),
            ("stream_args(", 0),
            (".run_json(", 0),
            ("run_json_with_stdin(", 0),
            (".run_captured(", 0),
            (".run_stream(", 0),
            ("spawn_interactive(", 0),
        ];
        for (call, skip_args) in shapes {
            let mut from = 0usize;
            while let Some(rel) = code[from..].find(call) {
                let at = from + rel + call.len();
                from = at;
                // A DEFINITION (`fn name(`), not a call.
                if code[..at - call.len()].trim_end().ends_with("fn") {
                    continue;
                }
                // Inside one of the wrappers: not a call site, its callers are.
                if enclosing_fn(code, at).is_some_and(|(_, n)| HELPERS.contains(&n.as_str())) {
                    helper_bodies += 1;
                    continue;
                }
                let mut i = at;
                for _ in 0..skip_args {
                    let mut d = 0i32;
                    while i < b.len() {
                        match b[i] {
                            b'(' | b'[' => d += 1,
                            b')' | b']' => {
                                if d == 0 {
                                    break;
                                }
                                d -= 1;
                            }
                            b',' if d == 0 => break,
                            _ => {}
                        }
                        i += 1;
                    }
                    i += 1;
                }
                i = skip_ws(b, i);
                let start = if code[i..].starts_with("vec![") {
                    Some(i + 5)
                } else if code[i..].starts_with("&[") {
                    Some(i + 2)
                } else if code[i..].starts_with('[') {
                    Some(i + 1)
                } else {
                    None
                };
                match start {
                    Some(p) => {
                        let v = leading_literals(code, p);
                        if v.is_empty() {
                            unresolved.push(format!("{call}<non-literal argv>"));
                        } else {
                            found.push(v);
                        }
                    }
                    None => {
                        let j = if i < b.len() && b[i] == b'&' { i + 1 } else { i };
                        let Some((name, _)) = ident_at(code, j) else {
                            unresolved.push(format!("{call}<unparseable>"));
                            continue;
                        };
                        match resolve_binding(code, &name, at) {
                            (Some(v), _) if !v.is_empty() => found.push(v),
                            (_, chain) => unresolved.push(format!("{call}{chain}")),
                        }
                    }
                }
            }
        }
        // `stream_action(action, …)` builds `["games", action, &id]` itself, so
        // its literal is the ACTION and the verb is `games <action>`.
        let mut from = 0usize;
        while let Some(rel) = code[from..].find("stream_action(") {
            let at = from + rel + "stream_action(".len();
            from = at;
            if code[..at - "stream_action(".len()].trim_end().ends_with("fn") {
                continue;
            }
            let v = leading_literals(code, at);
            if v.is_empty() {
                unresolved.push("stream_action(<non-literal action>".to_string());
            } else {
                found.push(vec!["games".to_string(), v[0].clone()]);
            }
        }
        (found, unresolved, helper_bodies)
    }

    /// A test module is not a call site. Removing them also stops the extractor
    /// finding its OWN shape strings (`"run_json_cmd("` et al) and reporting
    /// them as unclassifiable sites.
    ///
    /// REMOVES rather than TRUNCATES, and the difference is the whole point.
    /// This used to be `src.find("#[cfg(test)]")` and a cut, so nothing below
    /// the first test module was ever scanned — everything past line 7765 of
    /// this file. Harmless on the day it was found (no production
    /// `#[tauri::command]` lived down there) and latent in the worst possible
    /// direction: a real call site APPENDED to the end of the file would sail
    /// through the classification guard, while the identical function moved
    /// 2000 lines up would fail it loudly. A guard whose coverage depends on
    /// where in the file you type is not a guard. `strip_cfg_test`, 700 lines
    /// away in `startup.rs`, already did this correctly.
    ///
    /// `pub(crate)` so `keepalive_wiring_tests` reads the same production text
    /// this does — two definitions of "the production half" would drift, and
    /// both scans exist precisely because a comment or a test must never read
    /// as a production call site.
    pub(crate) fn production_half(src: &str) -> String {
        strip_cfg_test(&strip_comments(src))
    }

    fn all_sites() -> (Vec<Vec<String>>, Vec<String>, usize) {
        let mut found = Vec::new();
        let mut unresolved = Vec::new();
        let mut helpers = 0usize;
        for src in [include_str!("lib.rs"), include_str!("realmlist.rs")] {
            let (f, u, h) = extract(&production_half(src));
            found.extend(f);
            unresolved.extend(u);
            helpers += h;
        }
        (found, unresolved, helpers)
    }

    /// THE TEST. Every verb this launcher sends must have an EXPLICIT row in
    /// `dml_core::vocab::TABLE` — not merely a working destination, which the
    /// bash fallback gives everything. The runtime falls back silently; this
    /// does not.
    #[test]
    fn every_launcher_call_site_is_classified() {
        let (found, _, _) = all_sites();

        // NON-VACUITY (a): an extractor that silently stopped matching must
        // fail, not pass an empty loop.
        assert!(
            found.len() >= 100,
            "only {} call sites extracted — the extractor is broken, not the launcher",
            found.len()
        );
        // NON-VACUITY (b): two verbs known to be in this file, so an extractor
        // that lost ONE of the shapes is caught even while the floor holds.
        let flat: Vec<String> = found.iter().map(|v| v.join(" ")).collect();
        for probe in ["games list", "wow server-detail", "games start", "wow backup create"] {
            assert!(
                flat.iter().any(|v| v == probe),
                "extractor never found the known call site {probe:?}"
            );
        }

        let mut missing: Vec<String> = Vec::new();
        for verb in &found {
            let refs: Vec<&str> = verb.iter().map(String::as_str).collect();
            if !dml_core::vocab::is_classified(&refs) {
                let s = verb.join(" ");
                if !missing.contains(&s) {
                    missing.push(s);
                }
            }
        }
        assert!(
            missing.is_empty(),
            "these launcher call sites have no dml_core::vocab::TABLE row: {missing:?}\n\
             Add one to crates/dml-core/src/vocab.rs and decide whether it is \
             Target::DmlWow or Target::Bash. (Leaving it out does NOT break the \
             app — it falls back to bash — which is exactly why this test exists.)"
        );
    }

    /// The sites whose verb is a runtime value, each named with what it is and
    /// where its verbs are classified instead. Keyed on the call shape rather
    /// than a line number, so it survives the file moving around — and a NEW
    /// unresolvable site fails rather than joining the list silently.
    #[test]
    fn the_only_unreadable_call_site_is_the_one_we_know_about() {
        let (_, unresolved, helper_bodies) = all_sites();
        let mut got: Vec<String> = unresolved.clone();
        got.sort();
        got.dedup();
        assert_eq!(
            got,
            vec![
                // `tool_install`: the whole argv IS the tool name, a runtime
                // value from the closed TOOL_NAMES allowlist. Those names are
                // read out of the source and checked below.
                "spawn_interactive(<non-literal argv>".to_string()
            ],
            "the set of call sites whose verb cannot be read off the source has \
             changed. A new one must be understood and named here, never appended \
             quietly: an unreadable site is a verb nothing checks."
        );
        // The three wrappers each hold exactly one runner call. If a wrapper is
        // rewired — or a fourth appears — that is a change to the seam every
        // other assertion here depends on, so it must be seen.
        assert_eq!(
            helper_bodies, 3,
            "expected one runner call inside each of {HELPERS:?}, found {helper_bodies}"
        );
    }

    /// `tool_install`'s argv is a runtime value, so the extractor cannot read
    /// it — but the values come from a closed allowlist IN THIS FILE. Read that
    /// allowlist and require every entry to be classified, so adding a third
    /// tool goes red here instead of shipping an unclassified verb.
    #[test]
    fn every_tool_install_name_is_classified() {
        let src = production_half(include_str!("lib.rs"));
        let at = src
            .find("const TOOL_NAMES")
            .expect("TOOL_NAMES moved; tool_install's verbs are no longer pinned");
        let eq = src[at..].find('=').expect("TOOL_NAMES has no initializer") + at;
        let open = src[eq..].find('[').expect("TOOL_NAMES is not an array") + eq;
        let names = leading_literals(&src, open + 1);
        assert!(
            names.len() >= 2,
            "read {names:?} out of TOOL_NAMES — the extractor lost the array"
        );
        for n in &names {
            assert!(
                dml_core::vocab::is_classified(&[n.as_str()]),
                "tool_install can send {n:?}, which has no vocab::TABLE row"
            );
        }
    }

    /// EVERY WRITER OF THE LAUNCHER'S `soap.env` ALSO PUBLISHES IT TO THE CLI.
    ///
    /// `soap_bootstrap::bootstrap_verify_with` is the single choke point where
    /// this app persists SOAP credentials — automatic setup, the manual card's
    /// one-click create, and the manual card's verify all go through it. On the
    /// two in-distro backends the CLI that runs every SOAP verb reads a
    /// DIFFERENT file (R6), so a writer that does not also call
    /// `sync_distro_soap_env` leaves the launcher reporting success while GM
    /// Tools, My Party and the console stay dead.
    ///
    /// A count would not catch that: the failure is per-command, and a fourth
    /// writer added without the repair would keep any total ≥ 3 satisfied. So
    /// this scans command-by-command, and the comment stripper is mandatory —
    /// this file's prose names both functions repeatedly.
    #[test]
    fn every_command_that_saves_soap_credentials_also_publishes_them_to_the_cli() {
        let src = production_half(include_str!("lib.rs"));
        // Chunk by `#[tauri::command]`, so each piece is one command's body plus
        // the next one's attributes — near enough, since a writer and its repair
        // are always in the same body.
        let chunks: Vec<&str> = src.split("#[tauri::command]").skip(1).collect();
        let writers: Vec<&&str> = chunks
            .iter()
            .filter(|c| c.contains("bootstrap_verify_with("))
            .collect();
        // Non-vacuity, and the number is the point: three commands persist
        // credentials today. If one disappears this must be read, not adjusted.
        assert_eq!(
            writers.len(),
            3,
            "expected exactly 3 credential-writing commands (autosetup, account_create, \
             bootstrap_verify); found {}",
            writers.len()
        );
        for c in writers {
            let name = c
                .lines()
                .find(|l| l.contains("fn "))
                .unwrap_or("<unnamed>")
                .trim();
            assert!(
                c.contains("sync_distro_soap_env("),
                "this command saves SOAP credentials but never publishes them to the in-distro \
                 CLI, so on Backend::Arch/Wsl every SOAP verb keeps returning SOAP_AUTH: {name}"
            );
        }
    }

    /// `production_half` REMOVES test modules; it does not stop at the first.
    ///
    /// The truncating version scanned nothing below line 7765 of this file, so
    /// a genuine call site appended to the END would have passed the
    /// classification guard while the same function 2000 lines higher up failed
    /// it. Both test modules' calls must vanish AND the production call between
    /// them must survive — one assertion cannot show both.
    #[test]
    fn production_half_removes_test_modules_rather_than_truncating_at_the_first() {
        let src = r#"
fn early() { run_json_cmd(state, vec!["wow".into(), "before".into()]); }
#[cfg(test)]
mod a { fn t() { run_json_cmd(state, vec!["wow".into(), "invented-by-test-a".into()]); } }
fn late() { run_json_cmd(state, vec!["wow".into(), "after".into()]); }
#[cfg(test)]
mod b { fn t() { run_json_cmd(state, vec!["wow".into(), "invented-by-test-b".into()]); } }
"#;
        let (found, _, _) = extract(&production_half(src));
        let flat: Vec<String> = found.iter().map(|v| v.join(" ")).collect();
        assert_eq!(
            flat,
            vec!["wow before".to_string(), "wow after".to_string()],
            "got {flat:?} — `wow after` missing means the scan still truncates; an \
             `invented-by-test-*` present means a test module is being read as production"
        );
    }

    /// ...and the `#[cfg(test)] use …;` form is cut at the semicolon, not at a
    /// brace it does not have — otherwise it would swallow the next item whole,
    /// which is the same hole in a smaller package.
    #[test]
    fn a_cfg_test_use_statement_is_cut_at_its_semicolon() {
        let src = r#"
#[cfg(test)]
use something::else_;
fn real() { run_json_cmd(state, vec!["wow".into(), "survivor".into()]); }
"#;
        let (found, _, _) = extract(&production_half(src));
        let flat: Vec<String> = found.iter().map(|v| v.join(" ")).collect();
        assert_eq!(flat, vec!["wow survivor".to_string()], "got {flat:?}");
    }

    /// Comment stripping, proven on the exact shape that has burned this repo
    /// twice — prose that mentions a call, and a `//` inside a string literal
    /// (which must SURVIVE, or real argv would be eaten).
    #[test]
    fn comments_are_stripped_but_string_literals_are_not() {
        let src = r#"
// run_json_cmd(state, vec!["wow".into(), "invented-by-a-comment".into()])
/* run_json_cmd(state, vec!["wow".into(), "invented-by-a-block".into()]) */
fn real() { run_json_cmd(state, vec!["wow".into(), "real-one".into()]); }
fn url() { let _ = "https://example.invalid/x"; }
"#;
        let (found, _, _) = extract(&strip_comments(src));
        let flat: Vec<String> = found.iter().map(|v| v.join(" ")).collect();
        assert_eq!(flat, vec!["wow real-one".to_string()], "got {flat:?}");
        assert!(
            strip_comments(src).contains("https://example.invalid/x"),
            "the // inside a string literal must not be treated as a comment"
        );
    }
}

// ---------------------------------------------------------------------------
// The WSL keep-alive's PRODUCTION WIRING
// ---------------------------------------------------------------------------

/// THE KEEP-ALIVE IS ACTUALLY WIRED IN, AND IN THE DOCUMENTED ORDER.
///
/// `wsl_keepalive`'s own unit tests drive [`wsl_keepalive::Keepalive`] through a
/// fake spawner and prove the decision thoroughly. They prove NOTHING about this
/// file, and the gap is total rather than partial: delete
/// `wsl_keepalive::install(...)` and `STATE` is never set, so `state()` answers
/// `None`, every `apply()` returns at its first line, no holder is ever spawned
/// — and all eighteen still pass, because each builds its own `Keepalive`.
/// Measured: with `install` no-op'd and the other four call sites deleted, this
/// crate reported 234 passed / 0 failed. The feature can be removed from
/// production with a green suite, SILENTLY, which is the exact failure mode the
/// module exists to end.
///
/// Same shape as `every_command_that_saves_soap_credentials_also_publishes_them_to_the_cli`
/// above: wiring a unit test cannot see is pinned by reading the source, with
/// comments stripped first because this file's prose names every one of these
/// functions repeatedly.
#[cfg(test)]
mod keepalive_wiring_tests {
    use crate::vocab_coverage_tests::production_half;

    /// The calls that make a lifecycle command take TIME. Each one either
    /// spawns `wsl.exe`/`docker` or awaits something that does, so each is a
    /// moment at which the distro must already be held — or, for a stop, must
    /// still be held.
    const WORK: &[&str] = &[
        "ensure_engine_up(",
        "run_games_lifecycle_native(",
        "stream_action(",
        "stream_args(",
        "stop_engine_best_effort(",
    ];

    /// Every glue entry point in `wsl_keepalive`, and the function that must
    /// call it. A `pub fn` in the production-glue half with no caller here is a
    /// feature that exists only in its own tests.
    const WIRING: &[(&str, &str)] = &[
        // Arms STATE and the watchdog thread. Without this ONE call every other
        // entry point below is a no-op at its first line.
        ("wsl_keepalive::install(", "run"),
        ("wsl_keepalive::server_should_run()", "games_start"),
        ("wsl_keepalive::server_should_run()", "games_restart"),
        // `games_stop_watched`, not `games_stop`: fix round 3 (C1) made the
        // `#[tauri::command]` a pure delegate so `exit_stop_and_close` can pass
        // a `StreamOutcome` and learn whether the stop actually worked. The
        // body — and so the ordering this table pins — moved with it. See
        // `the_stop_command_is_a_pure_delegate`, which is what stops the work
        // from drifting back into the command where this scan would not see it.
        ("wsl_keepalive::server_should_stop()", "games_stop_watched"),
        // Adoption: a server started by a PREVIOUS launcher session is running
        // with nobody holding its distro.
        ("wsl_keepalive::observed_status(", "tray_set_status"),
        // The polite release. Without it an orphaned holder pins ~1.4 GB of VM.
        ("wsl_keepalive::shutdown()", "run"),
    ];

    fn src() -> String {
        production_half(include_str!("lib.rs"))
    }

    /// The brace-matched body of `fn <name>(`.
    ///
    /// String- and char-literal aware, so a `{` inside an argv literal cannot
    /// unbalance the match. Comments are already gone — `production_half`
    /// strips them — which is what makes this safe at all: the prose around
    /// these very functions is dense with braces and with their own names.
    ///
    /// The first `{` after the signature opens the body: a Rust parameter list
    /// and return type carry parens and angle brackets, never braces.
    fn fn_body(code: &str, name: &str) -> String {
        let needle = format!("fn {name}(");
        let at = code.find(&needle).unwrap_or_else(|| {
            panic!(
                "no `{needle}` in the production half of lib.rs — the keep-alive's host \
                 function has been renamed, deleted, or moved below a #[cfg(test)] module"
            )
        });
        let b: Vec<char> = code[at..].chars().collect();
        let mut i = 0usize;
        while i < b.len() && b[i] != '{' {
            i += 1;
        }
        assert!(i < b.len(), "`{needle}` has no body");
        let start = i;
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
                // A char literal, distinguished from a lifetime (`&'a str`) by
                // the closing quote two chars along, or by an escape.
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
                    depth -= 1;
                    if depth == 0 {
                        return b[start..=i].iter().collect();
                    }
                }
                _ => {}
            }
            i += 1;
        }
        panic!("unbalanced braces scanning `{needle}`");
    }

    /// EVERY glue entry point has a production caller.
    ///
    /// This is the one that goes red on the reviewer's mutation, and it is
    /// deliberately per-call-site rather than a count: the failure is per
    /// command, so a total that stayed at six while `games_restart` lost its
    /// declaration would be satisfied by a seventh call somewhere harmless.
    #[test]
    fn every_wsl_keepalive_entry_point_has_a_production_call_site() {
        let code = src();
        for (call, host) in WIRING {
            let body = fn_body(&code, host);
            assert!(
                body.contains(call),
                "`{host}` does not call `{call}`. On Backend::Arch that is not a missing \
                 nicety: WSL powers the distro off ~15s after the last session into it \
                 exits, so an unwired keep-alive means the server goes down a quarter of \
                 a minute later and `restart: unless-stopped` hides it by healing on the \
                 next touch. wsl_keepalive's own unit tests cannot see this — they build \
                 their own Keepalive and pass either way."
            );
        }
        // NON-VACUITY. A `fn_body` that returned something trivially containing
        // everything (say the whole file) would satisfy the loop above, so pin
        // the negatives too.
        let stop = fn_body(&code, "games_stop_watched");
        assert!(
            !stop.contains("wsl_keepalive::server_should_run()"),
            "fn_body is not isolating one function — games_stop_watched cannot declare Run"
        );
        assert!(
            !fn_body(&code, "games_start").contains("wsl_keepalive::shutdown()"),
            "fn_body is not isolating one function — games_start is not the exit handler"
        );
    }

    /// ARMING IS SINGULAR. Two `install` calls would be harmless today (the
    /// second `OnceLock::set` fails and returns), but the reason it is harmless
    /// is a detail of `install`, not a property of the call sites — and a
    /// second watchdog thread is exactly the duplicate-poller shape this repo
    /// already refused for the tray status.
    #[test]
    fn the_keep_alive_is_armed_exactly_once() {
        let code = src();
        assert_eq!(
            code.matches("wsl_keepalive::install(").count(),
            1,
            "expected exactly one wsl_keepalive::install call in production"
        );
    }

    /// THE ORDERING, which is load-bearing rather than tidy.
    ///
    /// A lifecycle command is ITSELF a `wsl.exe` session into the distro, so
    /// the 15 s clock starts when the command exits. Declaring Run after the
    /// work leaves the window this module exists to close: the server starts,
    /// the command's own session ends, and 15 s later the distro — and the
    /// stack inside it — is gone. Declaring Stop before the work is the mirror
    /// image: the hold is dropped while `compose down` is still stopping
    /// containers, so the distro can power off mid-shutdown.
    ///
    /// Asserted against the REAL work calls in the real body, not a restated
    /// list of steps: the recorded `lifecycle_steps_for_mode` lesson is that an
    /// ordering pinned on a pure list production never reads is not pinned.
    #[test]
    fn the_intent_is_declared_before_a_start_and_after_a_stop() {
        let code = src();

        for host in ["games_start", "games_restart"] {
            let body = fn_body(&code, host);
            let at = body
                .find("wsl_keepalive::server_should_run()")
                .unwrap_or_else(|| panic!("{host} never declares the keep-alive intent"));
            let first_work = WORK
                .iter()
                .filter_map(|w| body.find(w))
                .min()
                .unwrap_or_else(|| panic!("{host}: found none of the work calls {WORK:?}"));
            // NON-VACUITY: an `at < first_work` that holds because the scan
            // found only one late call proves nothing about the rest.
            assert!(
                WORK.iter().filter(|w| body.contains(*w)).count() >= 2,
                "{host}: fewer than two work calls found — the scan is broken, not the command"
            );
            assert!(
                at < first_work,
                "{host} declares the keep-alive intent AFTER it starts working. The hold \
                 must exist before this command's own wsl.exe session ends, or the server \
                 it just started dies ~15s later."
            );
        }

        let body = fn_body(&code, "games_stop_watched");
        let at = body
            .find("wsl_keepalive::server_should_stop()")
            .expect("games_stop_watched never releases the keep-alive intent");
        let last_work = WORK
            .iter()
            .filter_map(|w| body.rfind(w))
            .max()
            .expect("games_stop_watched: found none of the work calls");
        assert!(
            WORK.iter().filter(|w| body.contains(*w)).count() >= 2,
            "games_stop_watched: fewer than two work calls found — the scan is broken"
        );
        assert!(
            at > last_work,
            "games_stop_watched releases the hold BEFORE the stop has finished. That starts \
             the distro's 15s clock while compose is still shutting containers down — the \
             ungraceful stop this backend already struggles with."
        );
    }

    /// H6 (final review 2026-08-05). THE FAIL-OPEN THE DOC COMMENT CALLS
    /// LOAD-BEARING, ENFORCED.
    ///
    /// `exit_prevention_allowed` had zero tests, and both of its reads shipped
    /// at 273 passed under their own inverse: `let webview_has_spoken = true`,
    /// and `.unwrap_or(false)` -> `.unwrap_or(true)`. A paragraph of prose spent
    /// establishing "every uncertainty here must resolve in favour of the user
    /// being able to close their launcher" while nothing checked it. Inverted, a
    /// poisoned lock or an unmanaged `AppState` starts PREVENTING — the trap
    /// rebuilt out of the fix.
    ///
    /// The counter half now has a seam (`exit_prevention_allowed_with`) that
    /// unit tests drive; this is the part that still needs a real `AppHandle`,
    /// so it is pinned by reading it.
    #[test]
    fn the_webview_evidence_fails_open_toward_closing() {
        let code = src();
        let body = fn_body(&code, "webview_has_spoken");

        assert!(
            body.contains("last_status_push"),
            "webview_has_spoken no longer reads last_status_push — it is the ONLY thing \
             Rust knows about whether a webview exists to answer a question"
        );
        assert!(
            body.contains("try_state::<AppState>()"),
            "webview_has_spoken no longer asks AppState at all; a hardcoded answer here \
             is invisible to every unit test in this crate (H6)"
        );
        assert!(
            body.contains(".unwrap_or(false)"),
            "the fail-open is gone. A missing AppState or a poisoned lock must answer \
             `false` — do not prevent. Any other default makes an uncertainty START \
             preventing, which is the unclosable launcher (F1) rebuilt out of its own fix."
        );
        assert!(
            !body.contains(".unwrap_or(true)"),
            "the fail-open is INVERTED — a poisoned lock now vetoes the user's exit (H6)"
        );

        // NON-VACUITY: `fn_body` must really be isolating this function.
        assert!(
            !body.contains("EXIT_PROMPT_GUARD"),
            "fn_body is not isolating one function — the counter lives in \
             exit_prevention_allowed_with, not here"
        );
    }

    /// C2's WIRING HALF, and this branch has twice paid for leaving it out: a
    /// pure guard is perfectly green under a revert that only unhooks it.
    /// `clicks_during_a_confirmed_stop_never_reach_an_exit` drives
    /// `ExitPromptGuard` directly and would stay green if production simply
    /// stopped passing `stop_in_flight` — which is exactly the shape of the
    /// bug, not a hypothetical.
    #[test]
    fn the_exit_guard_knows_a_confirmed_stop_is_draining() {
        let code = src();

        let allowed = fn_body(&code, "exit_prevention_allowed");
        assert!(
            allowed.contains("stop_in_flight()"),
            "exit_prevention_allowed does not ask whether a confirmed stop is draining. \
             Then an impatient third click during a stop meets the bound, is not \
             prevented, and kills the process mid-`compose down` — holder released, \
             distro off ~15s later (C2)."
        );

        let stop = fn_body(&code, "exit_stop_and_close");
        let begin_at = stop.find("StopInFlight::begin()").expect(
            "exit_stop_and_close never marks the stop as in flight, so exit_prevention_allowed \
             can only ever see `false` and C2 is back",
        );
        let work_at = stop
            .find("games_stop_watched(")
            .expect("exit_stop_and_close no longer calls games_stop_watched");
        assert!(
            begin_at < work_at,
            "the in-flight marker is taken AFTER the stop starts. The window it exists to \
             cover is the await itself, so a marker set after it protects nothing."
        );

        // And the release is by Drop, not by a store the failure arm could skip
        // or a dropped future could leak.
        assert!(
            !stop.contains("EXIT_STOPS_IN_FLIGHT.store("),
            "exit_stop_and_close clears the in-flight depth by hand. Use the RAII guard: a \
             future that is dropped (a webview reload cancels in-flight invokes) would \
             otherwise leave the veto permanently armed, which is F1 rebuilt."
        );

        // The other half of the ruling: an answer that did NOT end the process
        // starts a fresh run of asks.
        let answered_at = stop.find("exit_prompt_run_answered()").expect(
            "the failure arm does not reset the run of asks. The user asked, we answered, \
             and the answer left the launcher up — the next click is a fresh decision and \
             must not inherit a budget spent waiting for it (C2).",
        );
        let fail_arm = stop
            .find("AfterStop::ReportFailure =>")
            .expect("the ReportFailure arm was renamed or removed");
        assert!(
            answered_at > fail_arm,
            "the run is reset outside the ReportFailure arm — on the success path the \
             process is leaving and there is nothing to reset"
        );
    }

    /// C1's structural half: the ordering scan above reads
    /// `games_stop_watched`, so nothing may quietly reappear in the
    /// `#[tauri::command]` that wraps it. A stop step added to `games_stop`
    /// would run for the Home button and NOT for the exit dialog (which calls
    /// the inner function directly) — two stops that differ, with the divergent
    /// one on the path where the distro is about to power off — and the
    /// ordering scan would never see it.
    #[test]
    fn the_stop_command_is_a_pure_delegate() {
        let code = src();
        let body = fn_body(&code, "games_stop");
        assert!(
            body.contains("games_stop_watched("),
            "games_stop no longer delegates to games_stop_watched — the ordering scan and \
             exit_stop_and_close are now reading different stops"
        );
        for call in WORK {
            assert!(
                !body.contains(call),
                "games_stop does work of its own (`{call}`). Everything the stop does must \
                 live in games_stop_watched, which is what exit_stop_and_close calls and \
                 what the keep-alive ordering scan reads."
            );
        }
        assert!(
            !body.contains("wsl_keepalive::"),
            "games_stop declares keep-alive intent of its own — that belongs in \
             games_stop_watched, next to the work whose ordering it is about"
        );
    }

    /// THE WINDOW IS NEVER DESTROYED BY A CLOSE CLICK. Fix round 1
    /// (2026-08-05).
    ///
    /// Before this fix, `on_window_event` only called `api.prevent_close()`
    /// INSIDE `if hide { … }`, so with `closeToTray` off a close click let
    /// Tauri destroy the window — and destroying the LAST window makes
    /// tauri-runtime-wry fire `RunEvent::ExitRequested` itself, landing in
    /// the `ExitRequested` arm with no window left to show a dialog in:
    /// unclosable except via Task Manager, which skips `RunEvent::Exit` and
    /// hands the server the exact hard WSL cut this plan exists to prevent.
    ///
    /// `window_close_action`'s own tests (in `mod tests`) prove the PURE
    /// decision is right in isolation, but they call it directly and cannot
    /// see whether `on_window_event` actually reaches it before letting a
    /// close proceed — reverting only the wiring, leaving
    /// `window_close_action` itself untouched, would leave every one of
    /// those tests green. Same shape as
    /// `every_wsl_keepalive_entry_point_has_a_production_call_site` above:
    /// an ordering read from the real source, not a restated list.
    #[test]
    fn the_window_close_handler_never_lets_a_destroy_through_unprotected() {
        let code = src();
        let body = fn_body(&code, "run");
        let prevent_at = body
            .find("api.prevent_close();")
            .expect("on_window_event no longer calls api.prevent_close() at all");
        let close_to_tray_read_at = body
            .find("close_to_tray")
            .expect("on_window_event no longer reads the close_to_tray preference");
        // NON-VACUITY: both anchors must actually be found (the `.expect`s
        // above already guarantee that) AND be the real call sites, not an
        // incidental match — `window_close_action(` must also appear, or
        // `api.prevent_close()` could be a leftover from anywhere.
        assert!(
            body.contains("window_close_action("),
            "on_window_event no longer routes through window_close_action — the pure \
             decision and the production wiring have drifted apart"
        );
        assert!(
            prevent_at < close_to_tray_read_at,
            "api.prevent_close() must run BEFORE close_to_tray is even read, so the window \
             is never destroyed regardless of the setting. A destroyed window cannot show \
             the exit-requested dialog and cannot be recreated by tray Open — that is the \
             regression this test pins."
        );
    }

    /// EVERY `exit-requested` EMIT IS PRECEDED, WITHIN ITS OWN ARM, BY A
    /// WINDOW SURFACE. TASK 4 FIX ROUND 1 (2026-08-05).
    ///
    /// `closeToTray` defaults ON, so "close the window (hides to tray -- no
    /// prompt, correct), then Quit from the tray icon" is the DEFAULT path
    /// through this whole feature, not an edge case. Before this fix neither
    /// emit site called `tray::show_main_window`: the window stayed hidden
    /// (or, on the window-close path, was actively hidden by this very arm),
    /// the webview -- alive per `the_window_close_handler_never_lets_a_destroy_through_unprotected`
    /// above -- dutifully set `exitGuard.open = true` on the frontend, and
    /// nothing on screen ever changed. A tray icon that ignores a click reads
    /// as broken, and the natural next move is Task Manager, which bypasses
    /// the clean shutdown entirely and reproduces the exact hard WSL cut this
    /// whole plan exists to prevent.
    ///
    /// Bounded PER ARM (not "anywhere earlier in `run`'s body") on purpose:
    /// an unbounded search for the nearest preceding `show_main_window(`
    /// would credit the window-close arm's own call to the unrelated
    /// `RunEvent::ExitRequested` arm simply because it appears earlier in the
    /// file -- passing even with THAT arm's own call deleted, which is
    /// exactly the false pass the Step-5-style mutation below is run to
    /// catch. Each arm is isolated by slicing to the next arm's own unique
    /// anchor before searching -- same "read the real source, not a restated
    /// list" shape as `the_window_close_handler_never_lets_a_destroy_through_unprotected`.
    #[test]
    fn every_exit_requested_emit_is_preceded_by_a_window_surface() {
        let code = src();
        let body = fn_body(&code, "run");

        let window_close_arm_start = body
            .find("WindowCloseAction::PromptVisible => {")
            .expect("the window-close handler's prompt arm was renamed or removed");
        let window_close_arm_end = body[window_close_arm_start..]
            .find("WindowCloseAction::ExitNow => {")
            .map(|rel| window_close_arm_start + rel)
            .expect("WindowCloseAction::ExitNow arm not found after the prompt arm");
        let window_close_arm = &body[window_close_arm_start..window_close_arm_end];

        let tray_quit_arm_start = body
            .find("tauri::RunEvent::ExitRequested { api, .. } => {")
            .expect("the RunEvent::ExitRequested arm was renamed or removed");
        let tray_quit_arm_end = body[tray_quit_arm_start..]
            .find("tauri::RunEvent::Exit =>")
            .map(|rel| tray_quit_arm_start + rel)
            .expect("RunEvent::Exit arm not found after RunEvent::ExitRequested");
        let tray_quit_arm = &body[tray_quit_arm_start..tray_quit_arm_end];

        for (name, arm) in [
            ("the window-close handler's prompt arm", window_close_arm),
            ("the RunEvent::ExitRequested arm", tray_quit_arm),
        ] {
            let emit_at = arm
                .find("emit(\"exit-requested\"")
                .unwrap_or_else(|| panic!("{name} no longer emits \"exit-requested\" at all"));
            let surface_at = arm.find("show_main_window(").unwrap_or_else(|| {
                panic!(
                    "{name} emits \"exit-requested\" with no show_main_window(...) call \
                     anywhere in the same arm -- a hidden or minimized window cannot host the \
                     dialog this emit asks the frontend to show, and the user's click \
                     produces no visible change at all (see TASK 4 FIX ROUND 1)."
                )
            });
            assert!(
                surface_at < emit_at,
                "{name}: show_main_window(...) must run BEFORE the \"exit-requested\" emit, \
                 not merely appear somewhere in the same arm."
            );
        }
    }

    /// NEITHER EXIT SURFACE MAY PREVENT WITHOUT CONSULTING THE GUARD. FIX
    /// ROUND 2 (2026-08-05), finding F1.
    ///
    /// `may_prevent_exit`/`ExitPromptGuard`'s own tests (in `mod tests`) prove
    /// the bound is right in isolation, and every one of them stays green if
    /// the production condition goes back to a bare `should_prompt_on_exit(
    /// action)` — which is the entire bug: an unbounded `prevent_exit()` whose
    /// triggering condition (`last_verdict: None` -> `Unknown` ->
    /// `PromptUnknown`) is CAUSED by the same dead webview that cannot answer
    /// it. Tray Quit then does nothing, X is `prevent_close()`d
    /// unconditionally, and the only way out is Task Manager — which skips
    /// `RunEvent::Exit` and hands the server the hard WSL cut this plan exists
    /// to prevent.
    ///
    /// Bounded per arm, and the ordering is asserted rather than mere presence,
    /// for the same reason the test above does it: a guard call that happens
    /// AFTER `api.prevent_exit()` has already fired is not a guard.
    #[test]
    fn no_exit_surface_can_prevent_without_consulting_the_guard() {
        let code = src();
        let body = fn_body(&code, "run");

        let window_arm_start = body
            .find("let hide_to_tray")
            .expect("on_window_event no longer reads the close_to_tray preference");
        let window_arm_end = body[window_arm_start..]
            .find("tauri::RunEvent::ExitRequested")
            .map(|rel| window_arm_start + rel)
            .expect("RunEvent::ExitRequested not found after the window-close handler");
        let window_arm = &body[window_arm_start..window_arm_end];

        let tray_quit_arm_start = body
            .find("tauri::RunEvent::ExitRequested { api, .. } => {")
            .expect("the RunEvent::ExitRequested arm was renamed or removed");
        let tray_quit_arm_end = body[tray_quit_arm_start..]
            .find("tauri::RunEvent::Exit =>")
            .map(|rel| tray_quit_arm_start + rel)
            .expect("RunEvent::Exit arm not found after RunEvent::ExitRequested");
        let tray_quit_arm = &body[tray_quit_arm_start..tray_quit_arm_end];

        for (name, arm) in [
            ("the window-close handler", window_arm),
            ("the RunEvent::ExitRequested arm", tray_quit_arm),
        ] {
            assert!(
                arm.contains("exit_prevention_allowed("),
                "{name} decides to prompt without consulting exit_prevention_allowed(...). \
                 That is an UNBOUNDED veto: a webview that cannot render the dialog can \
                 never answer it, so the launcher becomes unclosable except via Task \
                 Manager (F1)."
            );
        }

        // H5 (final review 2026-08-05). CONSULTING IT IS NOT THE SAME AS USING
        // THE ANSWER. `window_close_action(hide_to_tray, action, true)` —
        // leaving the `guard_allows` binding in place so only an
        // `unused_variables` warning marks it, and CI sets no `-D warnings` —
        // passed all 273 tests under the test written to prevent exactly this.
        // Half of F1 restored: with `closeToTray` off, a dead webview gets a
        // `PromptVisible` on every X click forever, and `PromptVisible` never
        // exits. The assertion above only proves the string appears SOMEWHERE
        // in a slice spanning the whole plugin/invoke block.
        assert!(
            window_arm.contains("window_close_action(hide_to_tray, action, guard_allows)"),
            "the window-close handler no longer passes the guard's own answer to \
             window_close_action. A literal there (or a second variable) makes the bound \
             advisory on this surface: closeToTray off + a webview that cannot render the \
             dialog = PromptVisible on every X click, forever, and PromptVisible never \
             exits (H5)."
        );
        let guard_binding = window_arm
            .find("let guard_allows =")
            .expect("the guard's answer is no longer bound as `guard_allows` in the window arm");
        let binding_line: String = window_arm[guard_binding..]
            .chars()
            .take_while(|c| *c != ';')
            .collect();
        assert!(
            binding_line.contains("exit_prevention_allowed("),
            "`guard_allows` is bound to something other than the guard's answer, so the \
             call site above is pinned to a name that no longer means what it says (H5)"
        );

        // The tray/exit arm is the one that actually calls prevent_exit, so its
        // ORDER is checkable: the guard must be consulted first.
        let guard_at = tray_quit_arm.find("exit_prevention_allowed(").unwrap();
        let prevent_at = tray_quit_arm
            .find("api.prevent_exit();")
            .expect("the RunEvent::ExitRequested arm no longer calls api.prevent_exit()");
        assert!(
            guard_at < prevent_at,
            "exit_prevention_allowed(...) must be consulted BEFORE api.prevent_exit() — a \
             bound checked after the veto has fired is not a bound"
        );

        // NON-VACUITY: the two slices must really be different regions, or a
        // `fn_body` that returned the whole file would satisfy both.
        assert!(
            !window_arm.contains("api.prevent_exit();"),
            "the arm slicing is broken — the window-close handler does not call prevent_exit"
        );
        assert!(
            !tray_quit_arm.contains("window_close_action("),
            "the arm slicing is broken — the RunEvent arm does not route through \
             window_close_action"
        );
    }

    /// A FAILED CONFIRMED STOP DOES NOT REACH THE EXIT CALL. FIX ROUND 2
    /// (2026-08-05), finding F3.
    ///
    /// The old body was unconditional — `let result = games_stop(…).await;
    /// EXIT_CONFIRMED.store(true, …); app.exit(0); result` — so a stop that
    /// FAILED still closed the launcher, `app.exit(0)` raced the IPC error back
    /// to the webview, and the dialog's "The stop reported a problem" was never
    /// painted. `after_stop`'s own unit test cannot see any of that: reverting
    /// production to the three unconditional lines leaves it perfectly green
    /// (the recorded `lifecycle_steps_for_mode` lesson — never pin an
    /// invariant on a pure value production does not read).
    #[test]
    fn a_failed_confirmed_stop_does_not_reach_the_exit_call() {
        let code = src();
        let body = fn_body(&code, "exit_stop_and_close");

        assert!(
            body.contains("after_stop(stop_ok)"),
            "exit_stop_and_close no longer branches on the stop's outcome at all — a stop \
             that failed closes the launcher and tells the user nothing (F3)"
        );

        // C1: AND WHAT `stop_ok` IS MADE OF. `after_stop(result.is_ok())` — the
        // fix round 2 shape — reads as a branch and is a constant: `run_stream`
        // returns `Ok(code)` for every exit code and the native wrapper
        // resolves `Ok(())` by design, so `ReportFailure` was unreachable for
        // the one failure it exists to catch. Pinning the CALL alone let that
        // ship; the composition is the invariant.
        assert!(
            body.contains("result.is_ok() && !watch.failed()"),
            "exit_stop_and_close decides on the IPC result alone. That expression is TRUE \
             for a stop that failed — run_stream returns Ok(code) for every exit code \
             (crates/dml-core/src/runner.rs) and run_games_lifecycle_native resolves \
             Ok(()) by design — so the launcher closes, the holder is released, and the \
             distro powers off ~15s later on top of whatever a failed compose down left \
             running. The stream is the only place the failure is reported (C1)."
        );
        assert!(
            body.contains("games_stop_watched("),
            "exit_stop_and_close no longer calls games_stop_watched, so nothing observes \
             the stream and `watch` can only ever say the stop succeeded (C1)"
        );

        let close_arm = body
            .find("AfterStop::CloseNow =>")
            .expect("the CloseNow arm was renamed or removed");
        let fail_arm = body
            .find("AfterStop::ReportFailure =>")
            .expect("the ReportFailure arm was renamed or removed");
        assert!(close_arm < fail_arm, "arms reordered — this test's slicing assumes CloseNow first");

        // EXACTLY ONE exit call, and it is inside the success arm.
        assert_eq!(
            body.matches("app.exit(0)").count(),
            1,
            "exit_stop_and_close must call app.exit(0) exactly once"
        );
        let exit_at = body.find("app.exit(0)").unwrap();
        assert!(
            exit_at > close_arm && exit_at < fail_arm,
            "app.exit(0) is not inside the CloseNow arm — a failed stop still closes the \
             launcher, and the error it returns races the process exit (F3)"
        );

        // The latch too: latching on a failure would silence the prompt on the
        // NEXT close attempt, i.e. close with the server still up.
        let latch_at = body
            .find("EXIT_CONFIRMED.store(true")
            .expect("exit_stop_and_close no longer latches EXIT_CONFIRMED at all");
        assert!(
            latch_at > close_arm && latch_at < fail_arm,
            "EXIT_CONFIRMED is latched outside the CloseNow arm — a failed stop would then \
             disarm the dialog on the next close attempt"
        );

        // And the holder: `games_stop` releases it whatever happened (correct
        // for Home's Stop button), so the failure arm — which no longer exits —
        // has to take it back, or the distro powers off ~15s from now
        // underneath containers a failed `compose down` left running.
        let rehold_at = body.find("wsl_keepalive::server_should_run()").expect(
            "the failure arm does not re-take the keep-alive hold that games_stop released \
             on its way out — the launcher stays up while the distro's 15s clock runs",
        );
        assert!(
            rehold_at > fail_arm,
            "the hold is re-taken outside the ReportFailure arm"
        );
    }
}
