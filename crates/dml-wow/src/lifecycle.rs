//! Native-mode `games start`/`stop`/`restart` orchestration primitives
//! (spike: `spike/docker-desktop-native`, Chunk 3b). Faithful port of the
//! shared helpers `cli/src/90-main.sh`'s `games)` group leans on:
//! `_games_resolve_or_fail` (164-191), `_games_start_impl` (194-253, covers
//! BOTH `start` and `restart`), `_check_port_conflicts` (255-296), and
//! `_flush_heal_flag` (`cli/src/40-config.sh:774-785`).
//!
//! ARCHITECTURE, mirroring `modmgr`: every REUSABLE, unit-testable primitive
//! (title/compose-dir resolution, the flush-heal breadcrumb decision, the
//! port-conflict line builder, the per-mode compose argv sequence) lives here
//! as a free function. The STREAMED orchestration itself
//! ([`games_lifecycle_stream`], [`world_restart_stream`]) and the read-only
//! [`games_status`] probe live at the BOTTOM of this file — moved out of the
//! launcher's `lib.rs` by the cargo-workspace refactor (Task 9) so the
//! standalone CLI can drive them too. They keep the same NDJSON vocabulary
//! (`section_start`/`line`/`section_end`/`done`/`error`) with domain failures
//! travelling IN the stream; the Tauri commands are now thin `spawn_blocking`
//! adapters.
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml` (`games_start`/
//! `games_stop`/`games_restart` in `lib.rs` branch on `is_native_backend()`
//! internally -- these are shared commands, not `_native` siblings, because
//! the Docker-Desktop-engine wrapping already lives inside them).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use dml_core::error::CmdError;

/// Title/compose-dir resolution, `games status`'s running-count helper, the
/// per-mode compose argv sequence, and the port-conflict bind probe — moved to
/// `dml_core::compose` (cargo-workspace refactor, Task 6). Every one of these
/// is game-agnostic (no WoW ports, no playerbots, no AC container names); what
/// stayed behind in this module is the flush-heal guard, the hardcoded
/// game-port registry + `check_port_conflicts`, and the backup-aware lifecycle
/// step list, all of which ARE WoW/playerbots-specific. Re-exported here so
/// every existing `lifecycle::X` call site in `lib.rs` keeps compiling
/// unchanged.
pub use dml_core::compose::{
    compose_down_argv, compose_file_name, compose_sequence_for_mode, compose_up_argv,
    count_running_ids, games_dir_from_env, is_compose_down, port_listening, resolve_compose_dir,
    title_dir_for_id,
};

// ---------------------------------------------------------------------------
// Flush-heal guard -- `_flush_heal_flag` (`40-config.sh:774-785`). THIS GUARD
// PREVENTS A BOT-WIPE ON BOOT: if a previous `wow bots flush` was
// interrupted (SIGKILL / power loss) while `AiPlayerbot.DeleteRandomBotAccounts`
// was armed, the breadcrumb survives and this boot must disarm the flag
// before the server starts, or every random bot gets deleted again.
// ---------------------------------------------------------------------------

pub const FLUSH_HEAL_NOTE: &str =
    "an interrupted bot flush had left the bot-delete flag armed - reset to 0 so this boot keeps your bots";

/// `<compose_dir>/.dml-bot-flush-armed` -- the arm marker.
pub fn flush_marker_path(compose_dir: &Path) -> PathBuf {
    compose_dir.join(".dml-bot-flush-armed")
}

/// `<compose_dir>/env/dist/etc/modules/playerbots.conf` -- same layout
/// `dml::config::ConfigReader`/`dml::tuning` use for every other conf read.
pub fn flush_conf_path(compose_dir: &Path) -> PathBuf {
    compose_dir.join("env").join("dist").join("etc").join("modules").join("playerbots.conf")
}

/// Self-heal: if `compose_dir` carries the arm marker, force
/// `AiPlayerbot.DeleteRandomBotAccounts` back to `0` (best-effort -- a
/// missing conf file just skips the write, matching the bash's own
/// `[[ -f "$conf" ]]` guard) and drop the marker (best-effort remove). Some(note)
/// when it healed something, `None` otherwise (nothing to do) -- never
/// fails; the caller emits the note as a `warn` line when present.
pub fn flush_heal_flag(compose_dir: &Path) -> Option<String> {
    let marker = flush_marker_path(compose_dir);
    if !marker.is_file() {
        return None;
    }
    let conf = flush_conf_path(compose_dir);
    if conf.is_file() {
        let _ = super::config::conf_write(&conf, "AiPlayerbot.DeleteRandomBotAccounts", "0");
    }
    let _ = std::fs::remove_file(&marker);
    Some(FLUSH_HEAL_NOTE.to_string())
}

// ---------------------------------------------------------------------------
// `wow bots flush` (Chunk 4b): the arm/disarm guard + the confirm gate. Same
// marker/conf paths as the heal helpers just above -- BYTE-COMPATIBLE with
// both the bash CLI (`40-config.sh:707-785`) and the Chunk 3b native heal:
// same marker file name, same conf key, same conf path.
// ---------------------------------------------------------------------------

/// `_flush`'s confirm gate (`90-main.sh:3963`): `btconfirm != 1 || btack !=
/// "flush"` negated. Ported for parity of the error path even though the
/// shipped Tauri command hardcodes `confirm=true`/`ack="flush"` (the
/// launcher's own typed-"flush" UI is the actual gate) -- same posture as
/// `games remove`'s ported-but-unreachable CONFIRM_REQUIRED check (Chunk 4a).
pub fn bots_flush_confirmed(confirm: bool, ack: &str) -> bool {
    confirm && ack == "flush"
}

/// RAII arm/disarm guard for the bots-flush delete-flag window (Chunk 4b) —
/// the Rust analogue of the bash arm's EXIT + HUP/INT/TERM/PIPE traps
/// (`_flush_restore_flag`/`_flush_restore_flag_signal`, `40-config.sh:739-
/// 754`).
///
/// TRAP -> RUST MAPPING. Rust has no signal-trap mechanism to port 1:1, but
/// `Drop` is the load-bearing analogue: it fires on every unwind path a bash
/// EXIT trap fires on too -- a normal `return`, an early `?`-propagated
/// error, AND a panic (so long as the panic unwinds rather than aborts,
/// which is this workspace's default `panic = "unwind"`). The ONE thing
/// neither bash's trap NOR this `Drop` can catch is an untrappable SIGKILL or
/// a power cut -- bash closes that gap with the on-disk marker + a heal on
/// the next start/restart/flush, and this guard closes it the exact same
/// way: [`arm`] writes the SAME marker file [`flush_heal_flag`] already
/// checks for (already ported, Chunk 3b), so a process that dies here with
/// no chance to run ANY Rust code at all still gets healed on the next boot.
///
/// USAGE: construct via [`FlushGuard::arm`] (marker written FIRST, matching
/// the bash comment "a crash between the marker and the conf write only
/// costs a redundant reset to 0" -- so the caller's own
/// `AiPlayerbot.DeleteRandomBotAccounts=1` conf_write happens immediately
/// after, not inside `arm` itself, keeping WRITE_FAILED reporting the
/// caller's job). Call [`FlushGuard::disarm`] only once the flag has ALREADY
/// been written back to `0` by the caller AND the guard's job is done (bash's
/// steps "(4)+(5)", right before the rebuild restart) -- `disarm` marks the
/// guard inert and removes the marker; a `Drop` on a still-armed guard (any
/// return path before that point) performs the conf-restore-to-0 + marker-
/// removal itself, unconditionally, exactly like the bash trap's own
/// `_flush_restore_flag` (which also retries the conf write even if the
/// caller's own write already failed, and always removes the marker
/// regardless of that retry's outcome).
pub struct FlushGuard {
    conf: PathBuf,
    marker: PathBuf,
    armed: bool,
}

impl FlushGuard {
    /// `: > "$(_flush_marker_for "$pbflush")" 2>/dev/null || true`
    /// (`90-main.sh:4022`): best-effort marker write -- a failure here (rare;
    /// e.g. a permissions issue) is swallowed, matching the bash's own `||
    /// true`, and does NOT stop the flow from arming (the guard is still
    /// useful even without the on-disk breadcrumb: the in-process `Drop`
    /// still covers every non-SIGKILL death).
    pub fn arm(conf: PathBuf, marker: PathBuf) -> Self {
        let _ = std::fs::write(&marker, b"");
        Self { conf, marker, armed: true }
    }

    /// Bash's steps "(4)+(5)": the caller has ALREADY written the conf flag
    /// back to `0` itself (so it can report its own `WRITE_FAILED` on that
    /// specific step if it fails, matching the oracle) -- `disarm` just
    /// removes the marker and tells `Drop` there is nothing left to do,
    /// mirroring `rm -f "$(_flush_marker_for ...)"; FLUSH_RESTORE_CONF=""`.
    pub fn disarm(mut self) {
        self.armed = false;
        let _ = std::fs::remove_file(&self.marker);
    }
}

impl Drop for FlushGuard {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        // `_flush_restore_flag` (`40-config.sh:739-744`): best-effort restore
        // + best-effort marker removal, in that order, both unconditional.
        let _ = super::config::conf_write(&self.conf, "AiPlayerbot.DeleteRandomBotAccounts", "0");
        let _ = std::fs::remove_file(&self.marker);
    }
}

// ---------------------------------------------------------------------------
// Port-conflict check -- `_check_port_conflicts` (`90-main.sh:255-296`).
// Best-effort / warn-only: every path here degrades to "no warning" rather
// than failing the start. The DB port (3306) gets a silent host-port remap
// (clients never connect to it directly); every game-server port only warns
// (clients connect to fixed ports, so a silent remap isn't possible).
// ---------------------------------------------------------------------------

/// `_ports` (`90-main.sh:268-286`), verbatim: (port, description).
pub const CONFLICT_PORTS: &[(u16, &str)] = &[
    (3724, "WoW auth/login server (TrinityCore, AzerothCore, MaNGOS)"),
    (8085, "WoW world server (TrinityCore, AzerothCore)"),
    (7878, "WoW SOAP API (TrinityCore, AzerothCore)"),
    (4000, "EverQuest zone server (EQEmu)"),
    (5998, "EverQuest login server (EQEmu)"),
    (5999, "EverQuest login server (EQEmu)"),
    (9000, "EverQuest world/zone server (EQEmu)"),
    (2593, "Ultima Online game server (ServUO / RunUO)"),
    (7171, "Tibia game server (OpenTibia / OTServBR)"),
    (6112, "Blizzard legacy port (Warcraft III / Diablo II)"),
    (43594, "RuneScape private server (RSPS)"),
    (2106, "Lineage II login server (L2J)"),
    (7777, "Lineage II game server (L2J)"),
    (54230, "Final Fantasy XI auth server (Darkstar)"),
    (54231, "Final Fantasy XI game server (Darkstar)"),
    (44453, "Star Wars Galaxies login server"),
    (44462, "Star Wars Galaxies connection server"),
];

/// `grep -q 'DOCKER_DB_EXTERNAL_PORT' .env` (`90-main.sh:261`): a plain
/// substring search over the whole file, no line anchor.
pub fn env_has_db_external_port(text: &str) -> bool {
    text.contains("DOCKER_DB_EXTERNAL_PORT")
}

/// Pure core of the DB-port (3306) branch (`90-main.sh:260-265`): `Some(...)`
/// only when 3306 is in use AND the `.env` doesn't already carry an override
/// (an already-remapped title stays silent -- nothing new happened).
pub fn db_port_conflict_message(port_3306_in_use: bool, env_already_has_override: bool) -> Option<&'static str> {
    if port_3306_in_use && !env_already_has_override {
        Some("[dml] Port 3306 in use -- remapped DB host port to 13306")
    } else {
        None
    }
}

/// Pure core of the game-port warn loop (`90-main.sh:287-295`): two lines per
/// occupied port, in [`CONFLICT_PORTS`] order. `port_in_use` is injected so
/// this stays testable without a real socket.
pub fn game_port_conflict_lines(port_in_use: impl Fn(u16) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    for &(port, desc) in CONFLICT_PORTS {
        if port_in_use(port) {
            lines.push(format!("[WARN] Port {port} is already in use -- {desc}."));
            lines.push(format!("[WARN]   Stop whatever is using port {port} before starting this server."));
        }
    }
    lines
}

/// Live `_check_port_conflicts` port: reads/writes `<compose_dir>/.env` for
/// the 3306 branch (best-effort -- a write failure is silently swallowed,
/// same as the bash's own unchecked `>> .env`), then the game-port warn
/// loop. Returns every line to surface as a `warn` NDJSON line, in the
/// bash's own order (DB message first, if any).
pub fn check_port_conflicts(compose_dir: &Path, port_in_use: impl Fn(u16) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    let db_in_use = port_in_use(3306);
    let env_path = compose_dir.join(".env");
    let env_text = std::fs::read_to_string(&env_path).unwrap_or_default();
    let has_override = env_has_db_external_port(&env_text);
    if let Some(msg) = db_port_conflict_message(db_in_use, has_override) {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&env_path) {
            let _ = writeln!(f, "DOCKER_DB_EXTERNAL_PORT=13306");
        }
        lines.push(msg.to_string());
    }
    lines.extend(game_port_conflict_lines(port_in_use));
    lines
}

// ---------------------------------------------------------------------------
// Compose command timeouts + the automatic-backup step list. The argv
// builders/sequencer these timeouts pair with (`compose_up_argv`,
// `compose_down_argv`, `is_compose_down`, `compose_sequence_for_mode`) and the
// port-listening bind probe moved to `dml_core::compose` (Task 6, re-exported
// above) -- they are game-agnostic. These timeouts and the step list below
// stay here: their sizing/ordering is justified by AC-specific save-on-
// shutdown behavior (see each doc comment).
// ---------------------------------------------------------------------------

/// `-t 180`: AC saves characters during a graceful shutdown and needs far
/// more than docker's 10s default before a force-kill; bounded here at
/// 240s (60s slack over the 180s the compose command itself requests) so a
/// truly wedged `docker compose down` still can't hang the UI forever.
pub const COMPOSE_DOWN_TIMEOUT: Duration = Duration::from_secs(240);
/// `docker compose up -d` normally returns as soon as containers are
/// created/started, but a cold start with a freshly-pulled image (or one
/// rebuilt after a module/core update) can take much longer -- generous like
/// `modmgr::GIT_NET_TIMEOUT`, bounded so a wedged registry can't hang forever.
pub const COMPOSE_UP_TIMEOUT: Duration = Duration::from_secs(600);

/// The GUI's "faster restart" (`--no-saveall`) info line on the native
/// raw-compose path. WSL threads `DML_SKIP_SAVEALL` into `dml-start.sh`,
/// which then skips its own pre-stop SOAP `saveall` call; the native path
/// never runs `dml-start.sh` at all (KEY FACT: the native title dir has
/// none) so there is no separate pre-stop saveall step to skip in the first
/// place -- the flag is a no-op here. This line makes that explicit instead
/// of silently swallowing the option.
pub const SKIP_SAVEALL_NOTE: &str = "faster-restart requested -- the native compose path has no separate pre-stop saveall to skip; the graceful `docker compose down` already saves characters on shutdown.";

/// The ordered HIGH-LEVEL steps `lib.rs`'s `games_lifecycle_native_blocking`
/// runs for `mode` -- `"backup"` = the automatic chars-only pre-down safety
/// dump (`backup::AUTO_STOP_NAME`, `lib.rs`'s `auto_backup_before_stop`),
/// `"down"`/`"up"` = one [`compose_sequence_for_mode`] step. Exists so the
/// invariant "the automatic backup always runs before the FIRST `down`" is a
/// pure, independently-testable fact rather than only visible by reading
/// `lib.rs`'s call order -- `games_lifecycle_native_blocking` is itself only
/// integration-testable (it shells real `docker`), same doctrine as every
/// other pure-primitives-here/orchestration-in-lib.rs split in this module.
/// `start` has no backup step (nothing is stopping, so there is nothing to
/// snapshot first).
pub fn lifecycle_steps_for_mode(mode: &str) -> Vec<&'static str> {
    match mode {
        "start" => vec!["up"],
        "restart" => vec!["backup", "down", "up"],
        "stop" => vec!["backup", "down"],
        _ => vec![],
    }
}

// ---------------------------------------------------------------------------
// STREAMED / blocking orchestration, moved out of the launcher's `lib.rs` by
// the cargo-workspace refactor (Task 9).
//
// `world-restart` was the first fully native STREAMED action: it emits NDJSON
// events DIRECTLY (no `dml` subprocess at all) rather than forwarding a WSL
// child's stream. A faithful port of `90-main.sh:1657-1724`'s
// `world-restart)` arm — same order, messages, and error codes. Every domain
// failure (NOT_FOUND/DOCKER_DOWN/NOT_RUNNING/RESTART_FAILED/READY_TIMEOUT)
// travels IN the event stream as `section_end{status:"error"}` + `error`, so
// the caller resolves `Ok` for those; only a genuinely unexpected internal
// failure (the blocking task panicking) surfaces as a rejected promise.
//
// `games_lifecycle_stream` is the same shape for `games start`/`stop`/
// `restart` (a port of `_games_start_impl`, `90-main.sh:194-253`, which
// covers BOTH `start` and `restart`, plus the `stop)` arm at 1111-1132).
// KEY FACT (verified live): the native title dir has no `dml-start.sh`, so it
// always takes the bash arm's ELSE branch -- pure `docker compose`
// orchestration, never `bash ./dml-start.sh`. If a future title dir grows
// one, there is no bash host here to run it.
// ---------------------------------------------------------------------------

/// Bounded timeout for `docker restart -t 300 ac-worldserver`: must exceed
/// the 300s graceful-stop window the flag itself requests, or the bound
/// would kill the graceful stop it's supposed to be waiting out.
const WORLD_RESTART_STOP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(330);

fn wr_event_section_start() -> serde_json::Value {
    serde_json::json!({"event": "section_start", "name": "world-restart"})
}

fn wr_event_line(level: &str, text: impl Into<String>) -> serde_json::Value {
    serde_json::json!({"event": "line", "level": level, "text": text.into()})
}

fn wr_event_section_end(status: &str) -> serde_json::Value {
    serde_json::json!({"event": "section_end", "name": "world-restart", "status": status})
}

fn wr_event_done() -> serde_json::Value {
    serde_json::json!({"event": "done", "data": {
        "restarted": "world-only",
        "note": "settings changes were NOT applied -- use full Restart for that",
    }})
}

fn wr_event_error(code: &str, message: impl Into<String>, hint: &str) -> serde_json::Value {
    serde_json::json!({"event": "error", "error": {
        "code": code, "message": message.into(), "hint": hint,
    }})
}

/// Both containers must already be running for a world-only restart to
/// proceed — a port of the precondition gate at `90-main.sh:1686-1689`. A
/// `docker restart` on a STOPPED container STARTS it, which on a fully
/// stopped stack would boot the worldserver alone against a down database
/// and hang until `READY_TIMEOUT` (~30 min); requiring both up first turns
/// that into an instant, correct `NOT_RUNNING` answer instead.
fn wr_preconditions_ok(world_running: bool, db_running: bool) -> bool {
    world_running && db_running
}

/// `90-main.sh:1716`'s `(( wr_elapsed - wr_note >= 60 ))` cadence check, pure
/// (elapsed/last-note in whole seconds; `saturating_sub` since `elapsed` is
/// always `>= last_note` in the real loop, but a pure function shouldn't
/// panic on out-of-order test input).
pub fn wr_should_note_wait(elapsed_secs: u64, last_note_secs: u64) -> bool {
    elapsed_secs.saturating_sub(last_note_secs) >= 60
}

/// `90-main.sh:1718`'s "still waiting" progress line text.
fn wr_wait_note_text(elapsed_secs: u64) -> String {
    format!("still waiting (~{}m) - bots respawning takes a while...", elapsed_secs / 60)
}

/// `90-main.sh:1712`'s `(( wr_elapsed >= wr_timeout ))` timeout check, pure.
pub fn wr_timeout_exceeded(elapsed_secs: u64, timeout_secs: u64) -> bool {
    elapsed_secs >= timeout_secs
}

/// `DML_READY_TIMEOUT_SECS`, default 1800s — `90-main.sh:1709`'s
/// `"${DML_READY_TIMEOUT_SECS:-1800}"`. Any unset/unparseable value falls
/// back to the same default (matches bash's parameter-expansion fallback,
/// which never validates the override either).
pub fn wr_ready_timeout_secs() -> u64 {
    std::env::var("DML_READY_TIMEOUT_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(1800)
}

pub fn gl_line(emit: &impl Fn(serde_json::Value), level: &str, text: impl Into<String>) {
    emit(serde_json::json!({"event": "line", "level": level, "text": text.into()}));
}

pub fn gl_error(code: &str, message: impl Into<String>, hint: &str) -> serde_json::Value {
    serde_json::json!({"event": "error", "error": {"code": code, "message": message.into(), "hint": hint}})
}

/// Automatic pre-stop/-restart safety dump (see `dml::lifecycle::
/// lifecycle_steps_for_mode`'s doc comment for why this always runs before
/// the sequence's first `down`): if `ac-database` isn't running there is
/// nothing to dump — skipped silently, no line at all, matching every other
/// best-effort backup call site's "DB unreachable -> no sidecar" doctrine.
/// A dump failure only warns and returns — an automatic backup must NEVER
/// block a stop/restart the user asked for. Chars-only (no `--include-world`,
/// same as the bots-flush safety dump above), named `backup::AUTO_STOP_NAME`
/// so the Backups page can tell it apart from a manual one. Feeds the SAME
/// keep-10 prune pool as every other backup (see the `dml::backup` "Automatic
/// backups" section doc comment).
fn auto_backup_before_stop(docker_program: &std::ffi::OsStr, emit: &impl Fn(serde_json::Value)) {
    use crate::{backup, db, maint, status};

    if !status::container_running(docker_program, "ac-database", maint::PROBE_TIMEOUT) {
        return;
    }
    let Some(bdir) = backup::backup_dir() else { return };
    if std::fs::create_dir_all(&bdir).is_err() {
        return;
    }

    gl_line(emit, "info", "automatic backup before stop...");
    let db_cfg = db::DbConfig::from_env();
    let file_name = backup::new_backup_file_name(false);
    let out_path = bdir.join(&file_name);
    match backup::dump_to(docker_program, &db_cfg.password, false, &out_path) {
        Ok(()) => {
            backup::write_meta(&db_cfg, &out_path, Some(backup::AUTO_STOP_NAME));
            gl_line(emit, "info", format!("automatic backup saved: {file_name}"));
            for p in backup::prune(&bdir) {
                gl_line(emit, "info", format!("pruned old backup: {p}"));
            }
        }
        Err(errtail) => {
            gl_line(emit, "warn", format!("automatic backup failed -- continuing: {errtail}"));
        }
    }
}

/// NATIVE-MODE `games status <id>` (`90-main.sh:1074-1091`, Part 5a):
/// title-dir existence -> `_resolve_compose_dir` -> (if resolved)
/// `_compose_running`'s `docker compose -f <file> ps --status running -q`
/// probe. Read-only; never mutates anything. A title with no resolvable
/// compose dir reports `"stopped"` WITHOUT ever invoking docker (matches the
/// oracle's own `[[ -n "$compose_dir" ]] &&` short-circuit) -- and a
/// down/absent docker engine degrades the same way (`output_bounded_draining`
/// returning `None`, or an empty/failed `ps` -> zero running ids -> stopped).
pub fn games_status(id: &str, games_dir: &std::path::Path) -> Result<serde_json::Value, CmdError> {
    use crate::{lifecycle, native, status};

    let title_dir = games_dir.join(id);
    if !title_dir.is_dir() {
        return Err(CmdError {
            code: "NOT_FOUND".into(),
            message: format!("Title not found: {id}"),
            hint: "Run: dml games list --json".into(),
        });
    }

    let mut state = "stopped";
    if let Some(compose_dir) = lifecycle::resolve_compose_dir(&title_dir) {
        if let Some(name) = lifecycle::compose_file_name(&compose_dir) {
            let program = native::docker_program();
            let mut cmd = std::process::Command::new(&program);
            cmd.arg("compose").arg("-f").arg(compose_dir.join(name));
            cmd.args(["ps", "--status", "running", "-q"]);
            status::windows_no_window(&mut cmd);
            if let Some(out) = status::output_bounded_draining(cmd, std::time::Duration::from_secs(5)) {
                let text = String::from_utf8_lossy(&out.stdout);
                if lifecycle::count_running_ids(&text) > 0 {
                    state = "running";
                }
            }
        }
    }
    Ok(serde_json::json!({ "id": id, "state": state }))
}

/// The blocking flow itself (real docker/SOAP I/O + wall-clock sleeps) — run
/// under `spawn_blocking`. `emit` sends one NDJSON event per call; every
/// return path emits its own terminal event(s) first, so the caller never
/// needs to synthesize one. `soap_lock` serializes the `saveall` SOAP call
/// against any other native SOAP command in flight, same discipline as
/// `wow_console_send_native`.
pub fn world_restart_stream(
    no_saveall: bool,
    soap_lock: Arc<Mutex<()>>,
    emit: impl Fn(serde_json::Value),
) {
    use crate::{config::ConfigReader, maint, native, soap, status};

    emit(wr_event_section_start());

    let title_dir = ConfigReader::title_dir_from_env();
    if maint::resolve_server_dir(&title_dir).is_none() {
        emit(wr_event_section_end("error"));
        emit(wr_event_error("NOT_FOUND", "WoW Playerbots server not installed", "Install it first."));
        return;
    }

    let program = native::docker_program();
    if !maint::docker_engine_up(&program, maint::PROBE_TIMEOUT) {
        emit(wr_event_section_end("error"));
        emit(wr_event_error("DOCKER_DOWN", "Docker is not running", "Start Docker in the distro first."));
        return;
    }

    let world_running = status::container_running(&program, "ac-worldserver", maint::PROBE_TIMEOUT);
    let db_running = status::container_running(&program, "ac-database", maint::PROBE_TIMEOUT);
    if !wr_preconditions_ok(world_running, db_running) {
        emit(wr_event_section_end("error"));
        emit(wr_event_error(
            "NOT_RUNNING",
            "The server is not running",
            "A world-only restart needs the world server and database already up. Start the server (full Start) first.",
        ));
        return;
    }

    emit(wr_event_line("warn", "world-only restart does NOT apply settings changes -- use full Restart for that"));

    if no_saveall {
        emit(wr_event_line(
            "info",
            "skipping pre-stop saveall (faster) -- the graceful stop still saves characters on shutdown",
        ));
    } else {
        emit(wr_event_line("info", "saving all characters (best effort)..."));
        let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
        let cfg = soap::SoapConfig::load();
        // Best-effort: outcome (Ok/Fault/Auth/Unreachable) is deliberately
        // ignored, matching the bash's `soap_exec 'saveall' >/dev/null 2>&1
        // || true`.
        let _ = soap::exec(&cfg, "saveall");
    }

    emit(wr_event_line("info", "restarting the world server (graceful stop, up to 300s)..."));
    let mut cmd = std::process::Command::new(&program);
    cmd.args(["restart", "-t", "300", "ac-worldserver"]);
    status::windows_no_window(&mut cmd);
    let restart_ok =
        matches!(status::output_bounded_draining(cmd, WORLD_RESTART_STOP_TIMEOUT), Some(out) if out.status.success());
    if !restart_ok {
        emit(wr_event_section_end("error"));
        emit(wr_event_error(
            "RESTART_FAILED",
            "docker restart failed for ac-worldserver",
            "Is the server installed and started? Check: dml doctor",
        ));
        return;
    }

    emit(wr_event_line("info", "waiting for the world to come back..."));
    let timeout_secs = wr_ready_timeout_secs();
    let t0 = std::time::Instant::now();
    let mut last_note: u64 = 0;
    loop {
        if status::world_ready(&program, maint::PROBE_TIMEOUT) {
            break;
        }
        let elapsed = t0.elapsed().as_secs();
        if wr_timeout_exceeded(elapsed, timeout_secs) {
            emit(wr_event_section_end("error"));
            emit(wr_event_error(
                "READY_TIMEOUT",
                format!("The world did not come back within {timeout_secs}s"),
                "Check the Console logs; a full Restart may be needed.",
            ));
            return;
        }
        if wr_should_note_wait(elapsed, last_note) {
            last_note = elapsed;
            emit(wr_event_line("info", wr_wait_note_text(elapsed)));
        }
        std::thread::sleep(std::time::Duration::from_secs(2));
    }

    emit(wr_event_section_end("ok"));
    emit(wr_event_done());
}

/// The blocking flow itself (real docker spawns) — run under
/// `spawn_blocking`. `mode` is `"start"`/`"restart"`/`"stop"`.
pub fn games_lifecycle_stream(mode: &str, id: String, skip_saveall: bool, emit: impl Fn(serde_json::Value)) {
    use crate::{lifecycle, maint, native, status};

    emit(serde_json::json!({"event": "section_start", "name": mode}));

    let title_dir = lifecycle::title_dir_for_id(&id);
    if !title_dir.is_dir() {
        emit(serde_json::json!({"event": "section_end", "name": mode, "status": "error"}));
        emit(gl_error("NOT_FOUND", format!("Title not found: {id}"), "Run: dml games list --json"));
        return;
    }
    let Some(compose_dir) = lifecycle::resolve_compose_dir(&title_dir) else {
        emit(serde_json::json!({"event": "section_end", "name": mode, "status": "error"}));
        emit(gl_error(
            "NO_COMPOSE",
            format!("No compose file found in {id} or its subdirectories."),
            &format!("Reinstall the title or check {}", title_dir.display()),
        ));
        return;
    };

    let docker_program = native::docker_program();
    if !maint::docker_engine_up(&docker_program, maint::PROBE_TIMEOUT) {
        emit(serde_json::json!({"event": "section_end", "name": mode, "status": "error"}));
        emit(gl_error("DOCKER_DOWN", "Docker is not running.", "Try: dml doctor"));
        return;
    }

    // Automatic pre-down safety backup -- stop/restart only, BEFORE anything
    // below touches compose (see `lifecycle::lifecycle_steps_for_mode`'s doc
    // comment for the pure "backup precedes the first down" invariant this
    // mirrors). Best-effort: never aborts the stop/restart.
    if mode == "stop" || mode == "restart" {
        auto_backup_before_stop(&docker_program, &emit);
    }

    // Self-heal an interrupted `wow bots flush` -- start+restart only (a
    // `stop` never boots the server, so there is nothing to heal against
    // before it runs). THIS GUARD PREVENTS A BOT-WIPE ON BOOT.
    if mode == "start" || mode == "restart" {
        if let Some(note) = lifecycle::flush_heal_flag(&compose_dir) {
            gl_line(&emit, "warn", note);
        }
    }

    // Cold starts only: on a restart the ports are (expectedly) held by this
    // server's own still-running containers, so the check would cry wolf.
    if mode == "start" {
        for line in lifecycle::check_port_conflicts(&compose_dir, lifecycle::port_listening) {
            gl_line(&emit, "warn", line);
        }
    }

    if mode == "restart" && skip_saveall {
        gl_line(&emit, "info", lifecycle::SKIP_SAVEALL_NOTE);
    }

    let mut rc: Result<(), i32> = Ok(());
    for argv in lifecycle::compose_sequence_for_mode(mode) {
        let is_down = lifecycle::is_compose_down(&argv);
        gl_line(
            &emit,
            "info",
            if is_down { "stopping containers (docker compose down)..." } else { "starting containers (docker compose up -d)..." },
        );
        let timeout = if is_down { lifecycle::COMPOSE_DOWN_TIMEOUT } else { lifecycle::COMPOSE_UP_TIMEOUT };
        let mut cmd = std::process::Command::new(&docker_program);
        cmd.current_dir(&compose_dir).args(argv);
        status::windows_no_window(&mut cmd);
        rc = match status::output_bounded_draining(cmd, timeout) {
            Some(out) if out.status.success() => Ok(()),
            Some(out) => Err(out.status.code().unwrap_or(-1)),
            None => Err(-1),
        };
        if rc.is_err() {
            break;
        }
    }

    match rc {
        Ok(()) => {
            emit(serde_json::json!({"event": "section_end", "name": mode, "status": "ok"}));
            let state = if mode == "stop" { "stopped" } else { "running" };
            emit(serde_json::json!({"event": "done", "data": {"id": id, "state": state}}));
        }
        Err(code) if mode == "stop" => {
            emit(serde_json::json!({"event": "section_end", "name": mode, "status": "error"}));
            emit(gl_error("STOP_FAILED", format!("{id} failed to stop (exit {code})"), &format!("Try: dml kill {id}")));
        }
        Err(code) => {
            emit(serde_json::json!({"event": "section_end", "name": mode, "status": "error"}));
            emit(gl_error(
                "START_FAILED",
                format!("{id} failed to {mode} (exit {code})"),
                "Check logs: docker compose logs, or dml doctor",
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- flush-heal decision ------------------------------------------------

    #[test]
    fn flush_heal_flag_noop_when_marker_absent() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-heal-absent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(flush_heal_flag(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flush_heal_flag_heals_and_removes_marker_when_present() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-heal-present-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let conf_dir = dir.join("env").join("dist").join("etc").join("modules");
        std::fs::create_dir_all(&conf_dir).unwrap();
        std::fs::write(conf_dir.join("playerbots.conf"), "AiPlayerbot.DeleteRandomBotAccounts = 1\n").unwrap();
        std::fs::write(flush_marker_path(&dir), "").unwrap();

        let note = flush_heal_flag(&dir);
        assert_eq!(note.as_deref(), Some(FLUSH_HEAL_NOTE));
        assert!(!flush_marker_path(&dir).is_file(), "marker must be removed");
        let conf = std::fs::read_to_string(flush_conf_path(&dir)).unwrap();
        assert!(conf.contains("AiPlayerbot.DeleteRandomBotAccounts = 0"), "conf was: {conf}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flush_heal_flag_removes_marker_even_when_conf_missing() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-heal-noconf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(flush_marker_path(&dir), "").unwrap();

        let note = flush_heal_flag(&dir);
        assert_eq!(note.as_deref(), Some(FLUSH_HEAL_NOTE));
        assert!(!flush_marker_path(&dir).is_file());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- bots-flush confirm gate ---------------------------------------------

    #[test]
    fn bots_flush_confirmed_requires_both_flag_and_exact_ack() {
        assert!(bots_flush_confirmed(true, "flush"));
        assert!(!bots_flush_confirmed(false, "flush"));
        assert!(!bots_flush_confirmed(true, "FLUSH"));
        assert!(!bots_flush_confirmed(true, ""));
        assert!(!bots_flush_confirmed(false, ""));
    }

    // -- FlushGuard: arm/disarm/Drop as a state sequence ----------------------

    fn flush_guard_fixture(name: &str) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-flushguard-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let conf = dir.join("playerbots.conf");
        std::fs::write(&conf, "AiPlayerbot.DeleteRandomBotAccounts = 0\n").unwrap();
        let marker = dir.join(".dml-bot-flush-armed");
        (dir, conf, marker)
    }

    #[test]
    fn flush_guard_arm_writes_the_marker_immediately() {
        let (dir, conf, marker) = flush_guard_fixture("arm-writes-marker");
        assert!(!marker.is_file());
        let _guard = FlushGuard::arm(conf, marker.clone());
        assert!(marker.is_file(), "arm() must write the marker before the caller's own conf write");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flush_guard_drop_restores_flag_and_removes_marker_when_still_armed() {
        // Simulates: arm() -> caller sets the flag to "1" -> some early
        // return/error BEFORE disarm() is ever reached (e.g. the restart
        // failed) -- the guard going out of scope must be the ONLY thing
        // that restores the flag, matching bash's EXIT-trap safety net.
        let (dir, conf, marker) = flush_guard_fixture("drop-restores");
        {
            let _guard = FlushGuard::arm(conf.clone(), marker.clone());
            super::super::config::conf_write(&conf, "AiPlayerbot.DeleteRandomBotAccounts", "1").unwrap();
            let live = std::fs::read_to_string(&conf).unwrap();
            assert!(live.contains("= 1"), "live: {live}");
            // guard drops here, at end of this inner scope, WITHOUT disarm().
        }
        let restored = std::fs::read_to_string(&conf).unwrap();
        assert!(restored.contains("= 0"), "Drop must restore the flag to 0; got: {restored}");
        assert!(!marker.is_file(), "Drop must remove the marker");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flush_guard_disarm_then_drop_touches_nothing_further() {
        // Simulates the happy path: arm -> set flag to 1 -> restart succeeds
        // -> caller restores the flag to 0 itself -> disarm(). A SECOND,
        // out-of-band flag write made AFTER disarm (standing in for restart
        // #2's own activity) must survive untouched -- disarm's guard is
        // truly inert, not merely "restore skipped this once".
        let (dir, conf, marker) = flush_guard_fixture("disarm-then-drop");
        let guard = FlushGuard::arm(conf.clone(), marker.clone());
        super::super::config::conf_write(&conf, "AiPlayerbot.DeleteRandomBotAccounts", "1").unwrap();
        super::super::config::conf_write(&conf, "AiPlayerbot.DeleteRandomBotAccounts", "0").unwrap();
        guard.disarm();
        assert!(!marker.is_file(), "disarm() must remove the marker");

        // Stand-in for restart #2 changing something unrelated afterward --
        // must remain exactly as this test left it (no guard is watching).
        super::super::config::conf_write(&conf, "AiPlayerbot.DeleteRandomBotAccounts", "1").unwrap();
        let after = std::fs::read_to_string(&conf).unwrap();
        assert!(after.contains("= 1"), "a disarmed guard must never touch the conf again; got: {after}");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flush_guard_drop_fires_on_panic_unwind() {
        // Proves the `?`/early-return analogy extends to an actual panic:
        // the guard's Drop must still run during unwind and heal the flag,
        // exactly like bash's EXIT trap firing on a `set -e` death.
        let (dir, conf, marker) = flush_guard_fixture("panic-unwind");
        let conf_for_panic = conf.clone();
        let marker_for_panic = marker.clone();
        let result = std::panic::catch_unwind(move || {
            let _guard = FlushGuard::arm(conf_for_panic.clone(), marker_for_panic);
            super::super::config::conf_write(&conf_for_panic, "AiPlayerbot.DeleteRandomBotAccounts", "1").unwrap();
            panic!("simulated failure mid-flush, after arming");
        });
        assert!(result.is_err(), "the panic must have actually happened");

        let restored = std::fs::read_to_string(&conf).unwrap();
        assert!(restored.contains("= 0"), "Drop must fire during unwind; got: {restored}");
        assert!(!marker.is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn flush_guard_drop_is_a_noop_when_conf_file_is_gone() {
        // A missing conf at drop time (e.g. the title was uninstalled mid-
        // flush) must not panic -- `conf_write`'s own `NotFound` handling
        // degrades to "create it fresh"; the guard's Drop swallows any
        // outcome either way (`let _ =`), so this just proves no panic.
        let (dir, conf, marker) = flush_guard_fixture("conf-gone");
        std::fs::remove_file(&conf).unwrap();
        {
            let _guard = FlushGuard::arm(conf.clone(), marker.clone());
        }
        assert!(!marker.is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- port-conflict parsing ------------------------------------------------

    #[test]
    fn env_has_db_external_port_substring_match() {
        assert!(env_has_db_external_port("FOO=1\nDOCKER_DB_EXTERNAL_PORT=13306\n"));
        assert!(!env_has_db_external_port("FOO=1\nBAR=2\n"));
        assert!(!env_has_db_external_port(""));
    }

    #[test]
    fn db_port_conflict_message_only_when_in_use_and_no_override() {
        assert!(db_port_conflict_message(true, false).is_some());
        assert_eq!(db_port_conflict_message(false, false), None);
        assert_eq!(db_port_conflict_message(true, true), None, "already remapped -> stay silent");
        assert_eq!(db_port_conflict_message(false, true), None);
    }

    #[test]
    fn game_port_conflict_lines_only_for_occupied_ports() {
        let lines = game_port_conflict_lines(|p| p == 3724);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("3724"));
        assert!(lines[0].contains("already in use"));
        assert!(lines[1].contains("Stop whatever is using port 3724"));
    }

    #[test]
    fn game_port_conflict_lines_none_when_all_free() {
        assert!(game_port_conflict_lines(|_| false).is_empty());
    }

    #[test]
    fn game_port_conflict_lines_multiple_ports_all_reported_in_order() {
        let lines = game_port_conflict_lines(|p| p == 8085 || p == 7878);
        // 8085 sorts before 7878 in CONFLICT_PORTS -- assert registry order preserved.
        assert_eq!(lines.len(), 4);
        assert!(lines[0].contains("8085"));
        assert!(lines[2].contains("7878"));
    }

    #[test]
    fn check_port_conflicts_writes_env_line_once() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-envwrite-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let lines = check_port_conflicts(&dir, |p| p == 3306);
        assert_eq!(lines, vec!["[dml] Port 3306 in use -- remapped DB host port to 13306".to_string()]);
        let env_text = std::fs::read_to_string(dir.join(".env")).unwrap();
        assert!(env_text.contains("DOCKER_DB_EXTERNAL_PORT=13306"));

        // Second call: .env already carries the override -> silent (no new line).
        let lines2 = check_port_conflicts(&dir, |p| p == 3306);
        assert!(lines2.is_empty());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn check_port_conflicts_empty_when_nothing_in_use() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-envnone-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(check_port_conflicts(&dir, |_| false).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- automatic-backup step ordering ---------------------------------------

    #[test]
    fn lifecycle_steps_backup_always_precedes_the_first_down() {
        for mode in ["stop", "restart"] {
            let steps = lifecycle_steps_for_mode(mode);
            let backup_pos = steps.iter().position(|s| *s == "backup").unwrap_or_else(|| panic!("mode={mode} has no backup step"));
            let down_pos = steps.iter().position(|s| *s == "down").unwrap_or_else(|| panic!("mode={mode} has no down step"));
            assert!(backup_pos < down_pos, "mode={mode} steps={steps:?}");
        }
    }

    #[test]
    fn lifecycle_steps_start_has_no_backup_step() {
        assert_eq!(lifecycle_steps_for_mode("start"), vec!["up"]);
    }

    #[test]
    fn lifecycle_steps_exact_sequences() {
        assert_eq!(lifecycle_steps_for_mode("stop"), vec!["backup", "down"]);
        assert_eq!(lifecycle_steps_for_mode("restart"), vec!["backup", "down", "up"]);
        assert!(lifecycle_steps_for_mode("bogus").is_empty());
    }

    #[test]
    fn games_status_native_reports_not_found_for_missing_title() {
        // Takes `games_dir` as an explicit parameter (not read from
        // `DML_GAMES_DIR`) specifically so this test never touches a
        // process-global env var -- `cargo test` runs this crate's tests in
        // one process, and mutating `DML_GAMES_DIR` would race every OTHER
        // test that reads it via `ConfigReader::title_dir_from_env`/
        // `lifecycle::games_dir_from_env` concurrently (see the caution in
        // `dml::lifecycle`'s own test module header).
        let dir = std::env::temp_dir().join(format!("dml-gamesstatus-test-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let err = games_status("wow-server-playerbots", &dir).unwrap_err();
        assert_eq!(err.code, "NOT_FOUND");
        assert_eq!(err.message, "Title not found: wow-server-playerbots");
    }

    #[test]
    fn games_status_native_reports_stopped_when_no_compose_dir() {
        let dir = std::env::temp_dir().join(format!("dml-gamesstatus-test-nocompose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("some-title")).unwrap();
        // A title dir that exists but carries no compose file anywhere ->
        // `_compose_running` is never even invoked; state is "stopped".
        let out = games_status("some-title", &dir).unwrap();
        assert_eq!(out, serde_json::json!({ "id": "some-title", "state": "stopped" }));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn wr_event_section_start_shape() {
        assert_eq!(wr_event_section_start(), serde_json::json!({"event":"section_start","name":"world-restart"}));
    }

    #[test]
    fn wr_event_line_shape() {
        assert_eq!(
            wr_event_line("info", "saving all characters (best effort)..."),
            serde_json::json!({"event":"line","level":"info","text":"saving all characters (best effort)..."})
        );
        assert_eq!(
            wr_event_line("warn", "world-only restart does NOT apply settings changes -- use full Restart for that"),
            serde_json::json!({"event":"line","level":"warn","text":"world-only restart does NOT apply settings changes -- use full Restart for that"})
        );
    }

    #[test]
    fn wr_event_section_end_shape() {
        assert_eq!(
            wr_event_section_end("ok"),
            serde_json::json!({"event":"section_end","name":"world-restart","status":"ok"})
        );
        assert_eq!(
            wr_event_section_end("error"),
            serde_json::json!({"event":"section_end","name":"world-restart","status":"error"})
        );
    }

    #[test]
    fn wr_event_done_shape() {
        assert_eq!(
            wr_event_done(),
            serde_json::json!({"event":"done","data":{
                "restarted":"world-only",
                "note":"settings changes were NOT applied -- use full Restart for that",
            }})
        );
    }

    #[test]
    fn wr_event_error_shape() {
        assert_eq!(
            wr_event_error("NOT_RUNNING", "The server is not running", "Start the server (full Start) first."),
            serde_json::json!({"event":"error","error":{
                "code":"NOT_RUNNING",
                "message":"The server is not running",
                "hint":"Start the server (full Start) first.",
            }})
        );
    }

    #[test]
    fn wr_preconditions_ok_requires_both_running() {
        assert!(wr_preconditions_ok(true, true));
        assert!(!wr_preconditions_ok(true, false));
        assert!(!wr_preconditions_ok(false, true));
        assert!(!wr_preconditions_ok(false, false));
    }

    #[test]
    fn wr_should_note_wait_60s_cadence() {
        assert!(!wr_should_note_wait(0, 0));
        assert!(!wr_should_note_wait(59, 0));
        assert!(wr_should_note_wait(60, 0));
        assert!(wr_should_note_wait(125, 60));
        assert!(!wr_should_note_wait(119, 60));
        assert!(wr_should_note_wait(120, 60));
    }

    #[test]
    fn wr_wait_note_text_formats_minutes() {
        assert_eq!(wr_wait_note_text(60), "still waiting (~1m) - bots respawning takes a while...");
        assert_eq!(wr_wait_note_text(125), "still waiting (~2m) - bots respawning takes a while...");
        assert_eq!(wr_wait_note_text(0), "still waiting (~0m) - bots respawning takes a while...");
    }

    #[test]
    fn wr_timeout_exceeded_boundary() {
        assert!(!wr_timeout_exceeded(1799, 1800));
        assert!(wr_timeout_exceeded(1800, 1800));
        assert!(wr_timeout_exceeded(1801, 1800));
    }

    #[test]
    fn wr_ready_timeout_secs_defaults_and_reads_env() {
        std::env::remove_var("DML_READY_TIMEOUT_SECS");
        assert_eq!(wr_ready_timeout_secs(), 1800);
        std::env::set_var("DML_READY_TIMEOUT_SECS", "60");
        assert_eq!(wr_ready_timeout_secs(), 60);
        // Unparseable override falls back to the default, same as bash's
        // parameter expansion never validating the env var either.
        std::env::set_var("DML_READY_TIMEOUT_SECS", "not-a-number");
        assert_eq!(wr_ready_timeout_secs(), 1800);
        std::env::remove_var("DML_READY_TIMEOUT_SECS");
    }
}
