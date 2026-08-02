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

/// REFUSE to start when another stack already owns the `ac-*` container names.
///
/// ## This replaced a port check, and the reason is measured
///
/// The first version of this guard asked "can I bind 3724/8085/7878?" and
/// refused if not. On Windows with Docker Desktop -- the only platform the beta
/// ships on -- that question has NO BEARING on whether `docker compose up` will
/// work, verified both directions on 2026-08-01 against a live engine:
///
/// | situation | bind probe said | reality |
/// |---|---|---|
/// | Docker publishing 0.0.0.0:47893 (`netstat` LISTENING, serving HTTP 200) | **FREE** | taken |
/// | a plain `TcpListener` holding 0.0.0.0:47895 | **TAKEN** | `docker run -p 47895:80` came up anyway |
///
/// So it was inert for the only cause its own message named ("another DML
/// server is already running") and fired for cases where that message was
/// wrong -- with no override. A guard that is wrong in both directions is worse
/// than none, because people trust it.
///
/// The honest question was never about ports. It is the one the INSTALL guard
/// already asks: the `ac-*` container names are global to the docker ENGINE, so
/// a second stack cannot exist regardless of which ports anything holds. This
/// now uses the same pure helpers ([`crate::install_native::parse_stack_owners`]
/// / [`conflicting_owner`](crate::install_native::conflicting_owner)), so the
/// install-time and start-time answers cannot disagree.
///
/// `ps_output` is the raw [`crate::install_native::stack_owner_argv`] text,
/// or `None` when docker could not answer -- which is evidence of NOTHING and
/// never refuses.
///
/// `compose_dir` is the directory about to be composed from. It is the
/// second, LOAD-BEARING "this is ours" signal: the user's migrated server
/// runs under the project name `dml-wow-native`, which no derivation can
/// produce, and comparing the derived name alone refused their own server as
/// a foreign stack (live incident, 2026-08-02). The working-dir label is
/// ground truth about which directory a stack came from.
pub fn stack_conflict_refusal(
    ps_output: Option<&str>,
    our_project: &str,
    compose_dir: &Path,
) -> Option<(String, String)> {
    let out = ps_output?;
    let owners = crate::install_native::parse_stack_owners(out);
    let (container, owner) =
        crate::install_native::conflicting_owner(&owners, our_project, compose_dir)?;
    Some((
        crate::install_native::stack_conflict_message(&container, &owner),
        "Stop the other server first (Home > Stop, or `docker compose down` in its folder), then start this one."
            .to_string(),
    ))
}

/// The IANA dynamic/private range. The OS hands these out to OUTBOUND client
/// sockets, so "port N is in use" there says nothing about which server is
/// running — it usually means a browser opened a connection a moment ago.
pub const EPHEMERAL_FLOOR: u16 = 49152;

/// Pure core of the game-port warn loop (`90-main.sh:287-295`): two lines per
/// occupied port, in [`CONFLICT_PORTS`] order. `port_in_use` is injected so
/// this stays testable without a real socket.
///
/// PORTS IN THE EPHEMERAL RANGE NEVER WARN (found live, 2026-08-02). A start
/// reported "Port 54230 is already in use -- Final Fantasy XI auth server
/// (Darkstar)" on a machine running no such thing; by the time it was
/// checked, nothing held the port at all. Windows' dynamic range is
/// 49152-65535 (`netsh int ipv4 show dynamicport tcp`, confirmed on the
/// user's box), which swallows the two Darkstar entries whole — so that
/// warning fires on a transient client socket and can never be trusted.
///
/// This matters more than the noise: the three ports that DO mean something
/// (3724/8085/7878, the ones `stack_conflict_refusal` actually refuses on)
/// all sit far below the floor, and a user taught to scroll past port
/// warnings is a user who will scroll past those too.
pub fn game_port_conflict_lines(port_in_use: impl Fn(u16) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    for &(port, desc) in CONFLICT_PORTS {
        if port >= EPHEMERAL_FLOOR {
            continue;
        }
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

// The ordered HIGH-LEVEL steps a `games` mode runs -- the pre-stop worldserver
// log snapshot, then the automatic chars-only safety dump, then the
// `compose_sequence_for_mode` steps -- USED to be restated here as a pure
// `lifecycle_steps_for_mode(mode) -> Vec<&str>` list so the ordering
// invariants were "independently testable". Deleted (round-2 finding G17): no
// production code ever read that list, so moving or deleting the REAL snapshot
// call in `games_lifecycle_stream_with` left all four of its tests green while
// native stops silently stopped preserving the log. The order now lives in one
// place only -- that call site -- and is asserted there, against a fake
// `docker`, via `LifecycleEnv`.

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

/// The DATABASE must already be running for a world-only restart to proceed —
/// a port of the precondition gate at `90-main.sh:1722-1735`. A `docker
/// restart` on a STOPPED container STARTS it, which with the database down
/// would boot the worldserver alone against nothing and hang until
/// `READY_TIMEOUT` (~30 min); requiring the database up first turns that into
/// an instant, correct `NOT_RUNNING` answer instead. `world_running` is
/// deliberately NOT part of the verdict (it is kept for the truth table this
/// pins): restarting a crashed/stopped world against a healthy database is a
/// legitimate recovery, not an error.
fn wr_preconditions_ok(_world_running: bool, db_running: bool) -> bool {
    db_running
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

/// Poll cadence of the readiness wait — `90-main.sh:1769`'s `sleep 2`.
const WR_POLL: Duration = Duration::from_secs(2);

/// How many CONSECUTIVE not-running observations of `ac-worldserver` the
/// readiness wait tolerates before it gives up (Round 2 F1). WHY FIVE: by the
/// time the wait starts, `docker restart` has already RETURNED, and it only
/// returns once the engine has started the container again — so in the healthy
/// case the very first probe already sees it running. A single stray `false`
/// is still possible (a crash-looping container spends its restart backoff in
/// `restarting`, where `.State.Running` is false), so one observation must not
/// be enough. Five consecutive misses at [`WR_POLL`] apart is ~8s of
/// continuous downtime — an order of magnitude longer than any gap `docker
/// restart` itself leaves behind, and ~200x shorter than the 1800s readiness
/// budget this replaces for the exited/crash-looping world the DB-only
/// precondition now admits.
pub const WR_WORLD_DOWN_STRIKES: u32 = 5;

/// The world-died-during-the-wait error, byte-identical to the bash twin at
/// `90-main.sh`'s `world-restart)` arm. Reuses the arm's existing
/// `RESTART_FAILED` code rather than inventing one: the restart, as an
/// operation, failed — the container came back and went straight back down.
const WR_WORLD_DOWN_MSG: &str = "The world server exited instead of coming back up";
const WR_WORLD_DOWN_HINT: &str = "Check the Console logs for the boot error; fix it and try a full Restart.";

/// How the readiness wait ended. `WorldDown` is the Round-2 F1 fast-fail: the
/// world container is not running while we are waiting for it to become ready.
#[derive(Debug, PartialEq, Eq)]
pub enum WrWaitOutcome {
    Ready,
    WorldDown,
    Timeout,
}

// ---------------------------------------------------------------------------
// Boot-loop detection inside the readiness wait (incident follow-up 2).
//
// THE INCIDENT: the world crash-retried on "Can't connect to MySQL (110)" for
// ten minutes while this wait printed "still waiting (~Nm) - bots respawning
// takes a while...". Every one of those lines was true about the elapsed time
// and false about what was happening. The wait now recognises the loop and
// says so.
//
// ADVISORY ONLY. This never aborts a wait and never changes an outcome or an
// exit code -- the 1800s budget exists precisely because bot creation really
// is that slow, and a diagnosis that could cut a legitimately slow boot short
// would be worse than the silence it replaces.
// ---------------------------------------------------------------------------

/// How many restarts NEW SINCE THIS WAIT BEGAN make it a boot loop rather than
/// a slow boot.
///
/// WHY THREE. Docker's restart policy increments `RestartCount` only when it
/// revives a container that DIED; a healthy boot — however slow, however many
/// bots it is creating — never increments it at all, because the process stays
/// up. So even ONE new restart is already abnormal, and the threshold is not
/// about tolerating slowness. It is about tolerating a one-off: a single
/// OOM-kill or a transient that the next boot survives is a hiccup, not a
/// loop, and calling that a boot loop would train users to ignore the warning.
/// Three consecutive failures to get through boot is a pattern no healthy
/// start produces. The count is a DELTA against a baseline taken at the start
/// of the wait (see [`wr_wait_for_world`]), so a long-lived server carrying
/// hundreds of historical restarts can never trip it.
pub const BOOT_LOOP_RESTART_STRIKES: u64 = 3;

/// How many `Could not connect to MySQL`/`Can't connect to MySQL` lines in the
/// log tail before the note NAMES the database as the cause. Two, not one: a
/// single connect failure is normal during a cold start (the world races the
/// database container's own boot and retries), so only a REPEATED failure is
/// worth pinning the blame on.
pub const BOOT_LOOP_MYSQL_HITS_MIN: usize = 2;

/// Log tail read once, at detection time, to classify the cause. 200 lines is
/// several crash cycles' worth of the tail — enough to see a repeating
/// failure without pulling a large read into a loop that is already polling.
pub const BOOT_LOOP_CAUSE_TAIL_LINES: u32 = 200;

/// The single `warn` line the wait emits when it recognises a boot loop. Must
/// stay byte-identical to the bash twin in `90-main.sh`'s `world-restart)` arm
/// (CLAUDE.md: any new NDJSON line lands on BOTH sides or the two surfaces
/// diverge).
///
/// `mysql_evidence` is the ONLY thing that changes the wording: with repeated
/// connect failures in the log the note names the database and the Restart
/// Docker action directly; without them it must not assert a cause it did not
/// observe, so it points at the Console log first and offers Restart Docker as
/// the follow-up. One line, no newlines — it is an NDJSON `text` field.
pub fn boot_loop_note(new_restarts: u64, mysql_evidence: bool) -> String {
    let head = format!(
        "boot loop detected: the world server has restarted {new_restarts} times since this wait began -- it is crash-retrying, not slow-booting."
    );
    if mysql_evidence {
        format!("{head} Its log shows repeated MySQL connection errors, so the world cannot reach the database. Try Restart Docker (Tools), then start the server again.")
    } else {
        format!("{head} Check the Console log for the boot error; if it shows database connection errors, try Restart Docker (Tools), then start the server again.")
    }
}

/// The tri-state delta latch itself — ONE implementation of the decision,
/// shared by every readiness wait on the start path (bash twin:
/// `_boot_loop_check` in `cli/src/40-config.sh`).
///
/// Round-2 findings G3/G9: the detection originally lived only inside
/// [`wr_wait_for_world`], i.e. behind the feature-locked `wow world-restart`
/// button, while Home's primary Start/Restart buttons went somewhere else
/// entirely. Both call sites now drive THIS type, so a threshold or wording
/// can never drift between them.
///
/// RULES, all of them load-bearing:
/// * `None` is docker failing to answer, NOT zero restarts — skipped
///   entirely, so it neither sets nor resets the baseline. Collapsing it to
///   `Some(0)` fabricates a loop on a long-lived server; re-baselining on it
///   hides a real one on the wedged daemon this feature exists for.
/// * The baseline is the FIRST READABLE reading, never a fixed zero — a
///   server carrying hundreds of historical restarts must not trip it.
/// * A reading BELOW the baseline can only mean the container was RECREATED
///   (a fresh container starts at 0), so it re-baselines. `games restart`
///   recreates containers mid-boot via compose, and measuring a new container
///   against the old one's count would blind the watch for the rest of the
///   boot.
/// * Latched: at most one accusation per watch.
#[derive(Debug, Default)]
pub struct BootLoopWatch {
    baseline: Option<u64>,
    reported: bool,
}

impl BootLoopWatch {
    pub fn new() -> Self {
        Self { baseline: None, reported: false }
    }

    /// Feed one `.State.RestartCount` reading. `Some(new_restarts)` exactly
    /// once — on the reading that proves the loop — and `None` otherwise.
    pub fn observe(&mut self, reading: Option<u64>) -> Option<u64> {
        // `?`, not `unwrap_or(0)`: a missed reading falls straight out and
        // leaves every field exactly as it was.
        let current = reading?;
        let Some(base) = self.baseline else {
            self.baseline = Some(current);
            return None;
        };
        if current < base {
            // Only a recreated container can count DOWN.
            self.baseline = Some(current);
            return None;
        }
        let new_restarts = current - base;
        if self.reported || new_restarts < BOOT_LOOP_RESTART_STRIKES {
            return None;
        }
        self.reported = true;
        Some(new_restarts)
    }
}

/// Poll cadence for the watch that runs ALONGSIDE another operation (the
/// readiness wait in [`wr_wait_for_world`] keeps its own 2s cadence and does
/// not use this). Bash twin: `_boot_loop_poll_secs`. `DML_BOOT_LOOP_POLL_SECS`
/// is a test-only override seam, same shape as `DML_READY_TIMEOUT_SECS`.
pub const BOOT_LOOP_POLL_SECS: u64 = 15;

pub fn boot_loop_poll() -> Duration {
    let secs = std::env::var("DML_BOOT_LOOP_POLL_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|s| *s > 0)
        .unwrap_or(BOOT_LOOP_POLL_SECS);
    Duration::from_secs(secs)
}

/// Which `games` modes arm the boot-loop watch. `stop` does not: a boot loop
/// is a boot-time phenomenon and there is no boot to diagnose. Called by
/// [`games_lifecycle_stream`] itself — not a fact only a test knows.
pub fn watches_boot_loop(mode: &str) -> bool {
    matches!(mode, "start" | "restart")
}

/// Longest the watch will sleep before re-checking `stop`. The poll cadence is
/// coarse (15s) but a lifecycle command that finishes must not be held open
/// waiting for the next tick, so the sleep is sliced.
const BOOT_LOOP_STOP_SLICE: Duration = Duration::from_millis(100);

/// The standalone watch loop: poll `restart_count` every `poll` until `stop`
/// is set, reporting at most one boot loop. Parameterized over its probe, its
/// cadence and its stop signal so it is unit-testable without docker or
/// threads — production runs it on a background thread for the duration of a
/// native `games start`/`restart` (see [`games_lifecycle_stream`]).
///
/// ADVISORY ONLY: it observes and reports; it never cancels anything and never
/// influences the caller's outcome or exit code.
pub fn boot_loop_watch_run(
    poll: Duration,
    stop: &std::sync::atomic::AtomicBool,
    mut restart_count: impl FnMut() -> Option<u64>,
    mut on_boot_loop: impl FnMut(u64),
) {
    use std::sync::atomic::Ordering;

    let mut watch = BootLoopWatch::new();
    while !stop.load(Ordering::Relaxed) {
        if let Some(new_restarts) = watch.observe(restart_count()) {
            on_boot_loop(new_restarts);
        }
        // Sliced sleep: the cadence is deliberately coarse, but a lifecycle
        // command that has finished must not wait out a whole period before
        // this thread can be joined.
        let mut slept = Duration::ZERO;
        while slept < poll && !stop.load(Ordering::Relaxed) {
            let slice = std::cmp::min(BOOT_LOOP_STOP_SLICE, poll - slept);
            std::thread::sleep(slice);
            slept += slice;
        }
    }
}

/// The readiness wait itself, parameterized over its two probes and its poll
/// interval so it can be unit-tested without docker (production passes
/// [`WR_POLL`] and the live `status::world_ready` / `status::container_running`
/// probes). Same decision order as the `until _world_ready` loop in
/// `90-main.sh`'s `world-restart)` arm: readiness first (so an already-ready
/// world never even reaches the liveness probe), then liveness, then the
/// timeout, then the 60s progress note.
pub fn wr_wait_for_world(
    timeout_secs: u64,
    poll: Duration,
    mut ready: impl FnMut() -> bool,
    mut world_running: impl FnMut() -> Option<bool>,
    mut restart_count: impl FnMut() -> Option<u64>,
    on_note: impl Fn(u64),
    mut on_boot_loop: impl FnMut(u64),
) -> WrWaitOutcome {
    let t0 = std::time::Instant::now();
    let mut last_note: u64 = 0;
    let mut down_strikes: u32 = 0;
    // Boot-loop state (incident follow-up 2) — the SHARED latch, the same one
    // the native `games start|restart` watch drives (round-2 findings G3/G9).
    // See [`BootLoopWatch`] for why every one of its rules is load-bearing.
    let mut boot_loop = BootLoopWatch::new();
    loop {
        if ready() {
            return WrWaitOutcome::Ready;
        }
        // Diagnosis BEFORE the liveness verdict below: a crash-looping
        // container spends its backoff in `restarting` (.State.Running ==
        // false), so the liveness guard can legitimately give up on the very
        // iteration the loop becomes provable -- and the user must get the
        // explanation before the error, not instead of it. Latched: one line
        // per wait, never one per poll.
        if let Some(new_restarts) = boot_loop.observe(restart_count()) {
            on_boot_loop(new_restarts);
        }
        // Liveness (Round 2 F1): waiting for a container that is not even
        // running can only ever end in READY_TIMEOUT. Consecutive misses only
        // -- one live observation clears the count (see WR_WORLD_DOWN_STRIKES).
        //
        // `None` is docker failing to answer, NOT a down container: counting it
        // as a strike would let a few seconds of engine hiccup abort a healthy
        // restart with a fabricated boot-failure error. Inconclusive probes
        // neither strike nor clear; the readiness timeout stays the backstop.
        match world_running() {
            Some(true) => down_strikes = 0,
            Some(false) => {
                down_strikes += 1;
                if down_strikes >= WR_WORLD_DOWN_STRIKES {
                    return WrWaitOutcome::WorldDown;
                }
            }
            None => {}
        }
        let elapsed = t0.elapsed().as_secs();
        if wr_timeout_exceeded(elapsed, timeout_secs) {
            return WrWaitOutcome::Timeout;
        }
        if wr_should_note_wait(elapsed, last_note) {
            last_note = elapsed;
            on_note(elapsed);
        }
        std::thread::sleep(poll);
    }
}

pub fn gl_line(emit: &impl Fn(serde_json::Value), level: &str, text: impl Into<String>) {
    emit(serde_json::json!({"event": "line", "level": level, "text": text.into()}));
}

pub fn gl_error(code: &str, message: impl Into<String>, hint: &str) -> serde_json::Value {
    serde_json::json!({"event": "error", "error": {"code": code, "message": message.into(), "hint": hint}})
}

/// Pre-stop/-restart worldserver log snapshot (incident follow-up 3): the
/// compose `down`+`up` below RECREATES the containers, and the old
/// container's log dies with it — twice during the 2026-07-21 incident the
/// freeze evidence was destroyed by the very restart meant to fix it.
///
/// STRICTLY BEST-EFFORT, same doctrine as [`auto_backup_before_stop`]: a
/// failure is one `warn` line and the lifecycle continues. A `Skipped`
/// outcome (this title's compose project owns no world container — i.e. every
/// non-WoW title — or a docker that could not answer) is reported as NOTHING
/// at all: there was no evidence to lose, so a warning would be noise on every
/// other game in the library. Both docker calls inside `logsnap` are bounded
/// and pipe-draining.
///
/// SCOPED BY `compose_dir`, not by container name: `docker logs
/// ac-worldserver` answers for whichever title owns that container, so a
/// non-WoW title's stop used to file the WoW world's log under its own name
/// and evict the real evidence from the shared newest-N window.
///
/// The lines themselves come from `logsnap::snapshot_lines` — one place, so
/// the bash twin (`_snapshot_world_log_report`) has one thing to match.
fn snapshot_world_log_before_stop(
    docker_program: &std::ffi::OsStr,
    logs_dir: Option<&std::path::Path>,
    keep: usize,
    compose_dir: &std::path::Path,
    title: &str,
    mode: &str,
    emit: &impl Fn(serde_json::Value),
) {
    use crate::logsnap;

    // No resolvable `~/.dml` (no HOME/USERPROFILE at all) -- nowhere to put a
    // snapshot, and a stop the user asked for is not the place to complain
    // about it.
    let Some(dir) = logs_dir else { return };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let outcome = logsnap::snapshot_world_log_into(dir, docker_program, compose_dir, title, mode, keep, now);
    for (level, text) in logsnap::snapshot_lines(&outcome) {
        gl_line(emit, level, text);
    }
}

/// Automatic pre-stop/-restart safety dump — always AFTER
/// [`snapshot_world_log_before_stop`] and BEFORE the sequence's first `down`
/// (see [`games_lifecycle_stream_with`]'s call site, which is where that order
/// is decided and asserted): if `ac-database` isn't running there is
/// nothing to dump — skipped silently, no line at all, matching every other
/// best-effort backup call site's "DB unreachable -> no sidecar" doctrine.
/// A dump failure only warns and returns — an automatic backup must NEVER
/// block a stop/restart the user asked for. Chars-only (no `--include-world`,
/// same as the bots-flush safety dump above), named `backup::AUTO_STOP_NAME`
/// so the Backups page can tell it apart from a manual one. Feeds the SAME
/// keep-10 prune pool as every other backup (see the `dml::backup` "Automatic
/// backups" section doc comment).
fn auto_backup_before_stop(
    docker_program: &std::ffi::OsStr,
    backups_dir: Option<&std::path::Path>,
    emit: &impl Fn(serde_json::Value),
) {
    use crate::{backup, db, maint, status};

    if !status::container_running(docker_program, "ac-database", maint::PROBE_TIMEOUT) {
        return;
    }
    let Some(bdir) = backups_dir else { return };
    if std::fs::create_dir_all(bdir).is_err() {
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
            for p in backup::prune(bdir) {
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

/// Every container on this engine and the compose project that owns it, or
/// `None` when docker could not answer.
///
/// `None` is a could-not-tell and the caller must treat it as evidence of
/// nothing: a docker that is slow, wedged or mid-restart must never be read as
/// "the names are taken" (which would block a legitimate start) OR as "the
/// names are free" — the caller simply does not refuse, and a real collision
/// then surfaces as a compose error we did not fabricate.
fn stack_owner_ps(docker_program: &std::ffi::OsStr) -> Option<String> {
    let mut cmd = std::process::Command::new(docker_program);
    cmd.args(crate::install_native::stack_owner_argv());
    crate::status::windows_no_window(&mut cmd);
    let out = crate::status::output_bounded_draining(cmd, crate::maint::PROBE_TIMEOUT)?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The compose project name this title's stack runs under — the same
/// derivation `composegen` uses, so the owner comparison is against our OWN
/// project rather than a guess.
fn dml_wow_project_name(compose_dir: &std::path::Path) -> String {
    crate::composegen::project_name_for(compose_dir)
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
            "The database is not running",
            "A world-only restart needs the database already up. Start the server (full Start) first.",
        ));
        return;
    }

    emit(wr_event_line("warn", "world-only restart does NOT apply settings changes -- use full Restart for that"));
    if !world_running {
        emit(wr_event_line("info", "the world server is not running -- this restart will start it back up"));
    }

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
    let outcome = wr_wait_for_world(
        timeout_secs,
        WR_POLL,
        || status::world_ready(&program, maint::PROBE_TIMEOUT),
        || status::container_running_probe(&program, "ac-worldserver", maint::PROBE_TIMEOUT),
        || status::container_restart_count(&program, "ac-worldserver", maint::PROBE_TIMEOUT),
        |elapsed| emit(wr_event_line("info", wr_wait_note_text(elapsed))),
        // Boot-loop diagnosis (incident follow-up 2). The cause read happens
        // HERE, not in the wait loop: it costs one extra `docker logs` call
        // and only ever runs once, on the single iteration the loop is
        // recognised — polling it every 2s would double the log traffic of a
        // 30-minute wait to answer a question asked at most once.
        |restarts| {
            let hits = status::mysql_connect_failures(&status::world_log_tail(
                &program,
                BOOT_LOOP_CAUSE_TAIL_LINES,
                maint::PROBE_TIMEOUT,
            ));
            emit(wr_event_line("warn", boot_loop_note(restarts, hits >= BOOT_LOOP_MYSQL_HITS_MIN)));
        },
    );
    match outcome {
        WrWaitOutcome::Ready => {}
        WrWaitOutcome::WorldDown => {
            emit(wr_event_section_end("error"));
            emit(wr_event_error("RESTART_FAILED", WR_WORLD_DOWN_MSG, WR_WORLD_DOWN_HINT));
            return;
        }
        WrWaitOutcome::Timeout => {
            emit(wr_event_section_end("error"));
            emit(wr_event_error(
                "READY_TIMEOUT",
                format!("The world did not come back within {timeout_secs}s"),
                "Check the Console logs; a full Restart may be needed.",
            ));
            return;
        }
    }

    emit(wr_event_section_end("ok"));
    emit(wr_event_done());
}

/// Everything [`games_lifecycle_stream`] resolves from the process
/// environment before it orchestrates anything: the games root, the docker
/// executable, and the two `~/.dml` children its best-effort pre-stop steps
/// write to (each `None` when the home dir cannot be resolved at all).
///
/// WHY IT EXISTS. The pre-stop ORDER — worldserver log snapshot, then the
/// automatic mysqldump, then the first `compose down` — is the entire point of
/// incident follow-up 3, and it is decided in exactly one place: the call site
/// in [`games_lifecycle_stream_with`]. Hoisting these four lookups one level up
/// makes that call site drivable against a fake `docker` and temp directories,
/// so the order is asserted WHERE IT IS DECIDED. It used to be restated in a
/// parallel `lifecycle_steps_for_mode` list that no production code read, and
/// that list stayed green when the real snapshot call was moved or deleted
/// (round-2 finding G17) — a stop would silently stop preserving the log with
/// every test still passing.
pub struct LifecycleEnv {
    /// `DML_GAMES_DIR` (`GAMES_DIR`), the parent of every title dir.
    pub games_dir: PathBuf,
    /// The `docker` executable (`DML_DOCKER` / the Docker Desktop candidates).
    pub docker: std::ffi::OsString,
    /// `~/.dml/logs` — where the pre-stop worldserver snapshot lands.
    pub logs_dir: Option<PathBuf>,
    /// `DML_LOG_SNAPSHOT_KEEP`; `0` turns snapshots off entirely.
    pub log_snapshot_keep: usize,
    /// `~/.dml/backups` — where the automatic pre-stop safety dump lands.
    pub backups_dir: Option<PathBuf>,
}

impl LifecycleEnv {
    /// The live resolution, done once at the top of a lifecycle command.
    pub fn from_env() -> Self {
        use crate::{backup, lifecycle, logsnap, native};

        Self {
            games_dir: lifecycle::games_dir_from_env(),
            docker: native::docker_program(),
            logs_dir: logsnap::logs_dir(),
            log_snapshot_keep: logsnap::snapshot_keep_from_env(),
            backups_dir: backup::backup_dir(),
        }
    }
}

/// The blocking flow itself (real docker spawns) — run under
/// `spawn_blocking`. `mode` is `"start"`/`"restart"`/`"stop"`.
pub fn games_lifecycle_stream(mode: &str, id: String, skip_saveall: bool, emit: impl Fn(serde_json::Value)) {
    games_lifecycle_stream_with(&LifecycleEnv::from_env(), mode, id, skip_saveall, emit);
}

/// [`games_lifecycle_stream`] with its environment supplied rather than read —
/// the seam the ordering tests drive (see [`LifecycleEnv`]). Production reaches
/// this through the wrapper above, so this IS the real orchestration, not a
/// test-only restatement of it.
pub fn games_lifecycle_stream_with(
    env: &LifecycleEnv,
    mode: &str,
    id: String,
    skip_saveall: bool,
    emit: impl Fn(serde_json::Value),
) {
    use crate::{lifecycle, maint, status};

    emit(serde_json::json!({"event": "section_start", "name": mode}));

    let title_dir = env.games_dir.join(&id);
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

    let docker_program = env.docker.clone();
    if !maint::docker_engine_up(&docker_program, maint::PROBE_TIMEOUT) {
        emit(serde_json::json!({"event": "section_end", "name": mode, "status": "error"}));
        emit(gl_error("DOCKER_DOWN", "Docker is not running.", "Try: dml doctor"));
        return;
    }

    // THE ORDERING DECISION, and the only place it is made: pre-down
    // worldserver log snapshot, THEN the automatic safety backup, THEN (below)
    // the compose sequence -- stop/restart only, all of it ahead of anything
    // that touches compose.
    //
    // WHY THE SNAPSHOT IS FIRST: the compose `down`+`up` RECREATES the
    // containers and the old container's log dies with it (2026-07-21, twice),
    // and the backup between them is a full mysqldump that can run for minutes
    // and can fail -- capturing the log first means the evidence is already on
    // disk however that goes. A cold `start` has neither step: nothing is
    // stopping, so there is nothing to snapshot or dump first.
    //
    // Both are best-effort: neither ever aborts the stop/restart. The order is
    // pinned by `games_stop_snapshots_the_world_log_before_the_backup_and_the_
    // compose_down` in this module's tests, which drives THIS function against
    // a fake `docker` and reads the order back off the calls it made -- the
    // same oracle the bash twin's `games-log-snapshot.bats` uses.
    if mode == "stop" || mode == "restart" {
        snapshot_world_log_before_stop(
            &docker_program,
            env.logs_dir.as_deref(),
            env.log_snapshot_keep,
            &compose_dir,
            &id,
            mode,
            &emit,
        );
        auto_backup_before_stop(&docker_program, env.backups_dir.as_deref(), &emit);
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
        // REFUSAL FIRST, and it asks about CONTAINER NAMES rather than ports.
        //
        // The first version of this guard probed the three published ports and
        // refused if it could not bind them. Measured against a live Docker
        // Desktop on 2026-08-01, that question turned out to have no bearing on
        // whether the start can work: a port Docker was publishing (LISTENING,
        // serving) probed as FREE, and a port a plain listener held probed as
        // TAKEN yet `docker run -p` came up over it anyway. Wrong in both
        // directions, i.e. worse than nothing, because people trust a guard.
        //
        // The `ac-*` names ARE global to the docker engine, so this asks the
        // same question the install-time guard asks, through the same pure
        // helpers -- the two cannot now disagree.
        //
        // Deliberately AHEAD of the advisory sweep below: emitting a page of
        // warnings and then refusing anyway buries the sentence that matters.
        let project = dml_wow_project_name(&compose_dir);
        let ps_out = stack_owner_ps(&docker_program);
        if let Some((message, hint)) =
            lifecycle::stack_conflict_refusal(ps_out.as_deref(), &project, &compose_dir)
        {
            emit(serde_json::json!({"event": "section_end", "name": mode, "status": "error"}));
            emit(gl_error("STACK_CONFLICT", message, &hint));
            return;
        }
        for line in lifecycle::check_port_conflicts(&compose_dir, lifecycle::port_listening) {
            gl_line(&emit, "warn", line);
        }
    }

    if mode == "restart" && skip_saveall {
        gl_line(&emit, "info", lifecycle::SKIP_SAVEALL_NOTE);
    }

    // Boot-loop watch (round-2 findings G3/G9): the SAME detection the
    // `world-restart` wait and the WSL `dml-start.sh` watch use, armed for the
    // whole of a native start/restart. It runs on its own thread because the
    // compose steps below block; notes come back over a channel and are
    // emitted between steps, so `emit` stays single-threaded and needs no
    // `Send` bound.
    //
    // SCOPED BY COMPOSE PROJECT, never by the bare `ac-worldserver` name --
    // that name answers for whichever title owns it, so an unscoped watch
    // would accuse a MapleStory start of looping because the WoW world was
    // crash-looping beside it. Re-resolved every poll: `compose up` recreates
    // containers mid-boot, and a cached id would go stale exactly when the
    // evidence starts.
    //
    // HONEST LIMIT (reported with the fix, not hidden): unlike the WSL path,
    // the native path has NO readiness wait -- `compose up -d` returns as soon
    // as its dependency conditions are met -- so this watch only spans the
    // lifecycle command itself. It is armed here so both backends share one
    // decision and so a native readiness wait inherits it for free; the
    // missing native wait is its own change.
    let watch_stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (note_tx, note_rx) = std::sync::mpsc::channel::<String>();
    let watch_thread = if lifecycle::watches_boot_loop(mode) {
        let prog = docker_program.clone();
        let cdir = compose_dir.clone();
        let stop = watch_stop.clone();
        Some(std::thread::spawn(move || {
            lifecycle::boot_loop_watch_run(
                lifecycle::boot_loop_poll(),
                &stop,
                || {
                    crate::logsnap::resolve_world_container(&prog, &cdir)
                        .and_then(|cid| status::container_restart_count(&prog, &cid, maint::PROBE_TIMEOUT))
                },
                |restarts| {
                    // The cause read happens HERE, once, on the single poll the
                    // loop is recognised -- polling it every cadence would
                    // double the log traffic to answer a question asked once.
                    let hits = crate::logsnap::resolve_world_container(&prog, &cdir)
                        .map(|cid| {
                            status::mysql_connect_failures(&status::world_log_tail_of(
                                &prog,
                                &cid,
                                lifecycle::BOOT_LOOP_CAUSE_TAIL_LINES,
                                maint::PROBE_TIMEOUT,
                            ))
                        })
                        .unwrap_or(0);
                    let _ = note_tx
                        .send(lifecycle::boot_loop_note(restarts, hits >= lifecycle::BOOT_LOOP_MYSQL_HITS_MIN));
                },
            );
        }))
    } else {
        None
    };

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
        for note in note_rx.try_iter() {
            gl_line(&emit, "warn", note);
        }
        if rc.is_err() {
            break;
        }
    }

    // Wind the watch down BEFORE the terminal event: the diagnosis has to
    // reach the terminal ahead of the done/error line, or the launcher's
    // stream would have already closed the run. Best-effort throughout -- a
    // watch that somehow panicked must not turn a healthy start into a failure.
    watch_stop.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(h) = watch_thread {
        let _ = h.join();
    }
    for note in note_rx.try_iter() {
        gl_line(&emit, "warn", note);
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

    // -- the stack-conflict REFUSAL (Task 7) ---------------------------------
    //
    // These replaced a set of tests that pinned a PORT-BIND guard. The guard
    // was measured wrong in both directions against a live Docker Desktop on
    // 2026-08-01 -- a port Docker was publishing probed as FREE, a port a plain
    // listener held probed as TAKEN yet `docker run -p` came up over it -- so
    // the old tests were green over a guard that could not work. Deleted rather
    // than adapted: they asserted the wrong question confidently.

    fn ps_line(name: &str, project: &str) -> String {
        ps_row(name, project, "")
    }
    /// The REAL (tab-separated) format, working dir included.
    fn ps_row(name: &str, project: &str, wdir: &str) -> String {
        format!("{name}\t{project}\t{wdir}\n")
    }
    fn dir(p: &str) -> std::path::PathBuf {
        std::path::PathBuf::from(p)
    }

    #[test]
    fn another_stack_holding_our_container_names_refuses_the_start() {
        let out = ps_line("ac-worldserver", "some-other-stack");
        let (message, hint) = stack_conflict_refusal(Some(&out), "ours", &dir("C:/games/wow")).expect("must refuse");
        assert!(message.contains("ac-worldserver"), "{message}");
        // Naming the OTHER stack is the whole point: "a conflict" the user
        // cannot locate is not actionable.
        assert!(message.contains("some-other-stack"), "{message}");
        assert!(hint.contains("Stop the other server"), "{hint}");
    }

    #[test]
    fn our_own_running_stack_never_refuses_our_own_start() {
        // The names being held BY US is the normal state of a restart or a
        // re-start after a partial stop. Refusing here would make the app
        // unable to start the very server it manages.
        let out = ps_line("ac-worldserver", "ours") + &ps_line("ac-database", "ours");
        assert!(stack_conflict_refusal(Some(&out), "ours", &dir("C:/games/wow")).is_none());
    }

    #[test]
    fn an_unrelated_container_is_not_a_conflict() {
        let out = ps_line("nginx", "someone-else") + &ps_line("postgres", "another");
        assert!(stack_conflict_refusal(Some(&out), "ours", &dir("C:/games/wow")).is_none());
    }

    /// TRI-STATE. Docker being slow, wedged or mid-restart is evidence of
    /// NOTHING. Reading it as "the names are taken" would block a legitimate
    /// start with no override; a real collision instead surfaces as a compose
    /// error we did not fabricate.
    #[test]
    fn a_docker_that_could_not_answer_never_refuses() {
        assert!(stack_conflict_refusal(None, "ours", &dir("C:/games/wow")).is_none());
    }

    #[test]
    fn a_stack_from_our_own_directory_is_ours_whatever_its_project_is_called() {
        // THE LIVE INCIDENT (2026-08-02): the user's migrated server runs
        // under the project name "dml-wow-native" -- a migration-era name no
        // derivation produces -- and the guard refused their OWN server as a
        // foreign stack. The working-dir label is the ground truth that must
        // rescue this, across every spelling a shell can produce for the
        // same directory.
        for spelling in [
            "C:\\Users\\perzi\\dml-native\\wow-server-playerbots",
            "C:/Users/perzi/dml-native/wow-server-playerbots",
            "/c/Users/perzi/dml-native/wow-server-playerbots",
            "/mnt/c/Users/perzi/dml-native/wow-server-playerbots",
            "c:/users/PERZI/dml-native/wow-server-playerbots/",
        ] {
            let out = ps_row("ac-database", "dml-wow-native", spelling);
            assert!(
                stack_conflict_refusal(
                    Some(&out),
                    "dml-wow-server-playerbots-5c541930",
                    &dir("C:/Users/perzi/dml-native/wow-server-playerbots"),
                )
                .is_none(),
                "refused our own server over the label spelling {spelling:?}"
            );
        }
    }

    #[test]
    fn a_foreign_directory_with_a_foreign_name_still_refuses() {
        // The exclusion must not decay into "exclude everything that has a
        // working_dir": a DIFFERENT directory's stack is exactly what the
        // guard exists to catch.
        let out = ps_row("ac-database", "dml-wow-native", "C:/Users/perzi/OTHER-server/wow");
        assert!(
            stack_conflict_refusal(
                Some(&out),
                "dml-wow-server-playerbots-5c541930",
                &dir("C:/Users/perzi/dml-native/wow-server-playerbots"),
            )
            .is_some(),
            "a foreign stack with a working_dir label must still refuse"
        );
    }

    #[test]
    fn a_hand_run_container_with_no_compose_project_still_conflicts() {
        // `docker run --name ac-database ...` owns the name just as firmly as a
        // compose stack does, and reports an EMPTY project label. Skipping the
        // unlabelled case is how that sails past the guard.
        let out = ps_line("ac-database", "");
        let (message, _) = stack_conflict_refusal(Some(&out), "ours", &dir("C:/games/wow")).expect("must refuse");
        assert!(message.contains("not managed by Docker Compose"), "{message}");
    }

    #[test]
    fn every_container_the_generated_stack_claims_is_covered() {
        // A name missing from the guard is a collision it cannot see. Asserted
        // against the install engine's own list so the two cannot drift.
        for name in crate::install_native::OWNED_CONTAINERS {
            let out = ps_line(name, "other");
            assert!(
                stack_conflict_refusal(Some(&out), "ours", &dir("C:/games/wow")).is_some(),
                "{name} must be guarded"
            );
        }
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
    fn an_ephemeral_range_port_never_warns_even_when_it_is_in_use() {
        // FOUND LIVE 2026-08-02: a start reported "Port 54230 is already in
        // use -- Final Fantasy XI auth server (Darkstar)" on a machine with no
        // such thing, and by the time it was checked nothing held the port --
        // a browser had briefly borrowed it. Windows hands out 49152-65535 to
        // OUTBOUND sockets, so occupancy there is not evidence about servers.
        let lines = game_port_conflict_lines(|p| p == 54230 || p == 54231);
        assert!(lines.is_empty(), "ephemeral-range ports must never warn: {lines:?}");
    }

    #[test]
    fn the_ports_this_stack_publishes_are_all_below_the_ephemeral_floor() {
        // The filter must never silence the ports that matter -- the three
        // this stack actually publishes. If one ever moved above the floor it
        // would become unwarnable, so assert it rather than assume it.
        for p in [3724u16, 8085, 7878] {
            assert!(
                p < EPHEMERAL_FLOOR,
                "port {p} is published by this stack but sits in the ephemeral range, where warnings are suppressed"
            );
            assert!(
                CONFLICT_PORTS.iter().any(|(c, _)| *c == p),
                "port {p} is published by this stack but is not in the advisory registry"
            );
            assert_eq!(
                game_port_conflict_lines(|q| q == p).len(),
                2,
                "port {p} must still warn"
            );
        }
        // ...and the suppression is real, not vacuous: something IS filtered.
        let suppressed = CONFLICT_PORTS.iter().filter(|(p, _)| *p >= EPHEMERAL_FLOOR).count();
        assert!(suppressed > 0, "nothing is above the floor -- the filter test proves nothing");
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

    // -- the pre-stop step ORDER, asserted where it is DECIDED ---------------
    //
    // These drive the REAL orchestration (`games_lifecycle_stream_with`, the
    // one and only place the order is chosen) against a fake `docker` and temp
    // directories, and read the order back off the calls it actually made.
    // Round-2 finding G17: the previous coverage asserted on a pure
    // `lifecycle_steps_for_mode` list that NO production code read, so moving
    // or deleting the real snapshot call left every test green while native
    // stops silently stopped preserving the worldserver log.

    /// A body only the `docker logs` read can produce — no `(`/`)`/quotes/`&`,
    /// which would terminate the `.cmd` `if (...)` block or the `sh` quoting.
    const FAKE_WORLD_LOG: &str = "WORLD-LOG-EVIDENCE-abc123";

    fn lifecycle_fixture(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// A stand-in `docker` for the whole lifecycle flow: appends
    /// `<argv[0]> <argv[1]>` to `log` for EVERY call (that pair alone separates
    /// `compose ps` / `logs --tail` / `exec ac-database` / `compose down` /
    /// `compose up`, and it keeps shell-hostile argv — the `-p<password>`, the
    /// `{{.State.Running}}` templates — out of the echo), answers `inspect`
    /// with `true` so the automatic backup's "is the database up" gate opens,
    /// `compose ps -a -q` with a container id so the snapshot resolves one, and
    /// `logs` with [`FAKE_WORLD_LOG`]. Per-platform, per CLAUDE.md's
    /// test-portability rule — never a hardcoded interpreter.
    #[cfg(windows)]
    fn write_lifecycle_fake_docker(dir: &Path, log: &Path) -> PathBuf {
        let p = dir.join("fake-docker-lifecycle.cmd");
        let script = format!(
            "@echo off\r\n\
             >>\"{log}\" echo %~1 %~2\r\n\
             if \"%~1\"==\"inspect\" (\r\necho true\r\nexit /b 0\r\n)\r\n\
             if \"%~1\"==\"logs\" (\r\necho {FAKE_WORLD_LOG}\r\nexit /b 0\r\n)\r\n\
             if \"%~2\"==\"ps\" (\r\necho c0ffee1234ab\r\nexit /b 0\r\n)\r\n\
             exit /b 0\r\n",
            log = log.display()
        );
        std::fs::write(&p, script).unwrap();
        p
    }
    #[cfg(not(windows))]
    fn write_lifecycle_fake_docker(dir: &Path, log: &Path) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let p = dir.join("fake-docker-lifecycle.sh");
        let script = format!(
            "#!/bin/sh\n\
             echo \"$1 $2\" >> '{log}'\n\
             case \"$1\" in\n\
             \x20 inspect) echo true; exit 0;;\n\
             \x20 logs) echo {FAKE_WORLD_LOG}; exit 0;;\n\
             esac\n\
             if [ \"$2\" = \"ps\" ]; then echo c0ffee1234ab; fi\n\
             exit 0\n",
            log = log.display()
        );
        std::fs::write(&p, script).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        p
    }

    /// `<games>/<title>/docker-compose.yml` + a fake docker, wired into a
    /// [`LifecycleEnv`] whose `~/.dml` children are temp dirs (the real ones
    /// must never be touched: this flow WRITES snapshots and backups and PRUNES
    /// both pools).
    fn lifecycle_env_fixture(name: &str, title: &str) -> (PathBuf, PathBuf, PathBuf, LifecycleEnv) {
        let base = lifecycle_fixture(name);
        let games = base.join("games");
        let compose_dir = games.join(title);
        std::fs::create_dir_all(&compose_dir).unwrap();
        std::fs::write(compose_dir.join("docker-compose.yml"), "services: {}\n").unwrap();
        let calls = base.join("docker-calls.log");
        let docker = write_lifecycle_fake_docker(&base, &calls);
        let logs = base.join("logs");
        let env = LifecycleEnv {
            games_dir: games,
            docker: docker.into_os_string(),
            logs_dir: Some(logs.clone()),
            log_snapshot_keep: 10,
            backups_dir: Some(base.join("backups")),
        };
        (base, calls, logs, env)
    }

    fn event_texts(events: &[serde_json::Value]) -> Vec<String> {
        events.iter().filter_map(|e| e.get("text").and_then(|t| t.as_str()).map(str::to_string)).collect()
    }

    fn text_pos(texts: &[String], prefix: &str) -> usize {
        texts
            .iter()
            .position(|t| t.starts_with(prefix))
            .unwrap_or_else(|| panic!("no line starting `{prefix}` in the stream:\n{texts:#?}"))
    }

    #[test]
    fn games_stop_snapshots_the_world_log_before_the_backup_and_the_compose_down() {
        // Incident follow-up 3: `compose down` RECREATES the container and the
        // old container's log dies with it. The snapshot therefore has to be
        // the FIRST thing a stop does -- ahead of the automatic mysqldump too,
        // which can run for minutes and can fail.
        let title = "wow-server-playerbots";
        let (base, calls, logs, env) = lifecycle_env_fixture("stop-order", title);

        let events = std::cell::RefCell::new(Vec::new());
        games_lifecycle_stream_with(&env, "stop", title.to_string(), false, |v| events.borrow_mut().push(v));
        let events = events.into_inner();

        // 1. The order, read off the docker calls the run ACTUALLY made -- the
        //    same oracle the bash twin's bats test uses.
        let calls_text = std::fs::read_to_string(&calls).unwrap();
        let at = |needle: &str| {
            calls_text
                .find(needle)
                .unwrap_or_else(|| panic!("no `{needle}` among the docker calls:\n{calls_text}"))
        };
        let snapshot_read = at("logs --tail");
        let dump = at("exec ac-database");
        let down = at("compose down");
        assert!(
            snapshot_read < dump,
            "the worldserver log must be captured BEFORE the mysqldump that can run for minutes:\n{calls_text}"
        );
        assert!(
            dump < down,
            "the safety dump must run BEFORE the compose down:\n{calls_text}"
        );

        // 2. The evidence is really on disk, in the injected logs dir, with the
        //    log body in it -- a call that merely happened cannot satisfy this.
        let written: Vec<PathBuf> = std::fs::read_dir(&logs).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(written.len(), 1, "exactly one snapshot expected, got {written:?}");
        assert!(
            std::fs::read_to_string(&written[0]).unwrap().contains(FAKE_WORLD_LOG),
            "the snapshot must hold the worldserver log body"
        );

        // 3. The stream narrates it in that same order, and the stop still
        //    finished (both pre-steps are best-effort, never a gate).
        let texts = event_texts(&events);
        assert!(
            text_pos(&texts, "worldserver log snapshot saved:") < text_pos(&texts, "automatic backup before stop"),
            "{texts:#?}"
        );
        assert!(
            text_pos(&texts, "automatic backup before stop") < text_pos(&texts, "stopping containers"),
            "{texts:#?}"
        );
        assert_eq!(events.last().unwrap()["event"], "done", "{events:#?}");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn games_restart_snapshots_the_world_log_before_the_down_that_recreates_it() {
        // The incident case itself: the restart reached for to FIX the freeze
        // is what erased the reason for it. Asserted on the event stream rather
        // than the docker call log because `restart` also arms the boot-loop
        // watch, which calls docker from its own thread -- the call log's byte
        // order is not a sound oracle with a second writer, while events are
        // emitted from this thread alone.
        let title = "wow-server-playerbots";
        let (base, _calls, logs, env) = lifecycle_env_fixture("restart-order", title);

        let events = std::cell::RefCell::new(Vec::new());
        games_lifecycle_stream_with(&env, "restart", title.to_string(), false, |v| events.borrow_mut().push(v));
        let events = events.into_inner();

        let texts = event_texts(&events);
        assert!(
            text_pos(&texts, "worldserver log snapshot saved:") < text_pos(&texts, "stopping containers"),
            "{texts:#?}"
        );
        let written: Vec<PathBuf> = std::fs::read_dir(&logs).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(written.len(), 1, "exactly one snapshot expected, got {written:?}");
        assert!(std::fs::read_to_string(&written[0]).unwrap().contains(FAKE_WORLD_LOG));
        assert_eq!(events.last().unwrap()["event"], "done", "{events:#?}");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn games_start_neither_snapshots_nor_dumps() {
        // A cold start destroys no evidence it could have preserved (there is
        // no previous run of THIS container to read) and has nothing to dump
        // before -- so neither pre-stop step may fire.
        let title = "wow-server-playerbots";
        let (base, calls, logs, env) = lifecycle_env_fixture("start-no-presteps", title);

        let events = std::cell::RefCell::new(Vec::new());
        games_lifecycle_stream_with(&env, "start", title.to_string(), false, |v| events.borrow_mut().push(v));
        let events = events.into_inner();

        let calls_text = std::fs::read_to_string(&calls).unwrap();
        assert!(calls_text.contains("compose up"), "the start itself must have run:\n{calls_text}");
        assert!(!calls_text.contains("logs --tail"), "a start must not read a log tail:\n{calls_text}");
        assert!(!calls_text.contains("exec ac-database"), "a start must not dump:\n{calls_text}");
        assert!(!logs.exists(), "a start must not even create the logs dir");
        assert_eq!(events.last().unwrap()["event"], "done", "{events:#?}");

        let _ = std::fs::remove_dir_all(&base);
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
    fn wr_preconditions_ok_requires_only_the_database() {
        assert!(wr_preconditions_ok(true, true));
        assert!(!wr_preconditions_ok(true, false));
        // The recovery case: a crashed/stopped world against a healthy DB is
        // exactly what a world-only restart is for -- `docker restart` starts it.
        assert!(wr_preconditions_ok(false, true));
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
    fn wr_wait_for_world_fails_fast_when_the_world_is_not_running() {
        // Round 2 F1: the readiness wait's ONLY exit used to be the boot
        // marker, so an exited/crash-looping world -- exactly what the DB-only
        // precondition now admits through -- pinned the stream (and the
        // launcher's "Restarting..." UI) for the whole readiness budget.
        let timeout_secs = 10;
        let poll = Duration::from_millis(10);
        let t0 = std::time::Instant::now();
        let outcome = wr_wait_for_world(timeout_secs, poll, || false, || Some(false), || None, |_| {}, |_| {});
        let elapsed = t0.elapsed();
        assert_eq!(outcome, WrWaitOutcome::WorldDown, "a world that is never running must not be waited out");
        // The load-bearing assertion (see CLAUDE.md's vacuous-pass trap): the
        // wait must end WELL INSIDE the budget, not merely return an error.
        // The pre-fix loop satisfies "an error came back" -- at t=timeout.
        assert!(elapsed < Duration::from_secs(timeout_secs), "did not fast-fail: took {elapsed:?} of {timeout_secs}s");
        assert!(elapsed < Duration::from_secs(2), "fast-fail should be ~5 polls, took {elapsed:?}");
        // ...and NOT on the first observation: a restart legitimately passes
        // through a brief not-running window.
        assert!(
            elapsed >= poll * (WR_WORLD_DOWN_STRIKES - 1),
            "gave up before {WR_WORLD_DOWN_STRIKES} consecutive probes ({elapsed:?})"
        );
    }

    #[test]
    fn wr_wait_for_world_tolerates_a_transient_not_running_blip() {
        // One short of the strike count, then the container is live again: the
        // counter resets and the wait proceeds to a normal Ready.
        let mut ready_calls = 0u32;
        let mut running_calls = 0u32;
        let outcome = wr_wait_for_world(
            10,
            Duration::from_millis(1),
            || {
                ready_calls += 1;
                ready_calls > 2 * WR_WORLD_DOWN_STRIKES
            },
            || {
                running_calls += 1;
                Some(running_calls >= WR_WORLD_DOWN_STRIKES)
            },
            || None,
            |_| {},
            |_| {},
        );
        assert_eq!(outcome, WrWaitOutcome::Ready, "a {}-poll blip must not abort the wait", WR_WORLD_DOWN_STRIKES - 1);
    }

    #[test]
    fn wr_wait_for_world_never_strikes_on_an_inconclusive_probe() {
        // Round 2 fix-wave residual: `container_running` collapses "docker did
        // not answer" into `false`, so a few seconds of engine hiccup during
        // the wait used to look exactly like an exited world and aborted a
        // HEALTHY restart with a fabricated boot-failure error. An
        // inconclusive probe must neither strike nor clear -- the readiness
        // timeout stays the backstop.
        // The budget must be generous and the probe count must EXCEED the
        // strike threshold, or the wait exits on the timeout before the strikes
        // could ever have accumulated and the test passes whatever the code
        // does (the first cut of this test made exactly that mistake).
        let mut probes = 0u32;
        let mut ready_calls = 0u32;
        let outcome = wr_wait_for_world(
            10,
            Duration::from_millis(1),
            || {
                ready_calls += 1;
                ready_calls > 3 * WR_WORLD_DOWN_STRIKES
            },
            || {
                probes += 1;
                None
            },
            || None,
            |_| {},
            |_| {},
        );
        assert_eq!(outcome, WrWaitOutcome::Ready, "an unreadable docker must not be reported as a dead world");
        assert!(
            probes > WR_WORLD_DOWN_STRIKES,
            "only {probes} inconclusive probes: fewer than the {WR_WORLD_DOWN_STRIKES} strikes needed to trip the guard, so this proves nothing"
        );
    }

    #[test]
    fn wr_wait_for_world_does_not_let_hiccups_break_a_strike_streak() {
        // The strikes are CONSECUTIVE-down; an inconclusive probe in the middle
        // must not silently reset them either, or a container that is genuinely
        // down while docker is flaky would never trip the guard.
        // The hiccup must RECUR, or a counter that resets on it still reaches
        // the threshold from the probes that follow and the test proves nothing
        // (the first cut of this test made exactly that mistake). With every
        // Nth probe inconclusive, only a counter that SURVIVES the hiccup can
        // ever reach N consecutive downs.
        let mut calls = 0u32;
        let t0 = std::time::Instant::now();
        let outcome = wr_wait_for_world(
            1,
            Duration::from_millis(1),
            || false,
            || {
                calls += 1;
                if calls % WR_WORLD_DOWN_STRIKES == 0 { None } else { Some(false) }
            },
            || None,
            |_| {},
            |_| {},
        );
        assert_eq!(outcome, WrWaitOutcome::WorldDown);
        // A reset-on-hiccup would never trip the guard and would instead burn
        // the whole budget, so the speed is the discriminating assertion.
        assert!(t0.elapsed() < Duration::from_millis(500), "reached the verdict only by timing out ({:?})", t0.elapsed());
    }

    #[test]
    fn wr_wait_for_world_still_times_out_when_the_world_is_up_but_never_ready() {
        // The liveness check must not swallow the READY_TIMEOUT path: a
        // running-but-slow world (bots respawning) still ends at the budget.
        let outcome = wr_wait_for_world(0, Duration::from_millis(1), || false, || Some(true), || None, |_| {}, |_| {});
        assert_eq!(outcome, WrWaitOutcome::Timeout);
    }

    #[test]
    fn wr_wait_for_world_returns_ready_without_probing_liveness() {
        // Readiness is checked FIRST -- the crashed-world recovery restart that
        // boots fine must never touch the liveness probe.
        let mut liveness_probes = 0u32;
        let outcome = wr_wait_for_world(
            10,
            Duration::from_millis(1),
            || true,
            || {
                liveness_probes += 1;
                Some(false)
            },
            || None,
            |_| {},
            |_| {},
        );
        assert_eq!(outcome, WrWaitOutcome::Ready);
        assert_eq!(liveness_probes, 0);
    }

    // -- boot-loop detection (incident follow-up 2) --------------------------

    #[test]
    fn boot_loop_note_names_the_count_the_loop_and_the_action() {
        let n = boot_loop_note(4, false);
        assert!(n.contains("boot loop"), "{n}");
        assert!(n.contains('4'), "the note must say how many restarts were seen: {n}");
        assert!(n.contains("Restart Docker"), "the note must point at the action: {n}");
        // It must contradict the "still loading" story the wait otherwise tells.
        assert!(n.to_lowercase().contains("not slow-booting") || n.to_lowercase().contains("crash-retrying"), "{n}");
        assert!(!n.contains('\n'), "one NDJSON line only: {n}");
    }

    #[test]
    fn boot_loop_note_names_mysql_only_when_the_log_shows_it() {
        let with = boot_loop_note(3, true).to_lowercase();
        let without = boot_loop_note(3, false).to_lowercase();
        assert!(with.contains("mysql") && with.contains("database"), "{with}");
        // Without evidence it must NOT assert a cause it did not observe.
        assert!(!without.contains("mysql"), "{without}");
        assert!(without.contains("console"), "with no cause evidence, point at the log: {without}");
        assert!(with.contains("restart docker") && without.contains("restart docker"));
    }

    // -- BootLoopWatch: the shared latch both call sites drive ---------------

    #[test]
    fn boot_loop_watch_baselines_the_first_readable_reading_and_latches_once() {
        // A long-lived server carrying 47 historical restarts: only restarts
        // NEW since the watch began are evidence, so 47 is the baseline and
        // the accusation lands at 47+3 -- never on the first reading.
        let mut w = BootLoopWatch::new();
        assert_eq!(w.observe(Some(47)), None, "the first reading is the baseline, never an accusation");
        assert_eq!(w.observe(Some(48)), None);
        assert_eq!(w.observe(Some(49)), None, "+2 is under the {BOOT_LOOP_RESTART_STRIKES}-strike threshold");
        assert_eq!(w.observe(Some(50)), Some(3), "+3 since the baseline is the loop");
        assert_eq!(w.observe(Some(51)), None, "latched: one accusation per watch, not one per poll");
        assert_eq!(w.observe(Some(99)), None);
    }

    #[test]
    fn boot_loop_watch_skips_unreadable_readings_without_touching_the_baseline() {
        // "docker could not answer" is evidence of NOTHING. Two failure modes
        // this pins apart, both of which a constant-unreadable feed would hide:
        // an unreadable FIRST reading must not become a fake zero baseline...
        let mut w = BootLoopWatch::new();
        assert_eq!(w.observe(None), None);
        assert_eq!(w.observe(Some(7)), None, "7 is the baseline; collapsing the miss to 0 would call this +7");
        assert_eq!(w.observe(Some(7)), None);
        // ...and a miss BETWEEN readings must not re-base and hide a real climb.
        let mut w2 = BootLoopWatch::new();
        assert_eq!(w2.observe(Some(0)), None);
        assert_eq!(w2.observe(None), None);
        assert_eq!(w2.observe(Some(3)), Some(3), "the delta is measured from the pre-miss baseline");
    }

    #[test]
    fn boot_loop_watch_rebaselines_when_the_container_is_recreated() {
        // `games restart` recreates containers mid-boot (compose down+up), and
        // a fresh container's RestartCount starts at 0. Measuring it against
        // the old container's 40 would make every delta negative and blind the
        // watch for the whole boot -- exactly the boot it is there to watch.
        let mut w = BootLoopWatch::new();
        assert_eq!(w.observe(Some(40)), None);
        assert_eq!(w.observe(Some(0)), None, "a drop can only mean a new container: re-baseline, never accuse");
        assert_eq!(w.observe(Some(1)), None);
        assert_eq!(w.observe(Some(3)), Some(3), "the new container's own climb is still caught");
    }

    // -- boot_loop_watch_run: the standalone watch the games path arms -------

    #[test]
    fn boot_loop_watch_run_never_accuses_a_slow_but_healthy_boot() {
        // The whole point of the threshold: a boot can be arbitrarily slow --
        // thousands of bots -- and still be healthy. A healthy process never
        // dies, so RestartCount never moves, however long the wait.
        let poll = Duration::from_millis(5);
        let stop = std::sync::atomic::AtomicBool::new(false);
        let probes = std::sync::atomic::AtomicU32::new(0);
        let mut accusations = Vec::new();
        let t0 = std::time::Instant::now();
        boot_loop_watch_run(
            poll,
            &stop,
            || {
                // A long-lived server that is simply booting slowly.
                let n = probes.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                if n >= 12 {
                    stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Some(4)
            },
            |n| accusations.push(n),
        );
        let elapsed = t0.elapsed();
        assert!(accusations.is_empty(), "a healthy boot was accused: {accusations:?}");
        // ANTI-VACUITY: the two assertions a watch that returns early -- or
        // never polls at all -- cannot satisfy. The absence above has to be a
        // decision, not a no-op.
        let n = probes.load(std::sync::atomic::Ordering::Relaxed);
        assert!(n >= 12, "the watch only polled {n} times; it never ran long enough to conclude anything");
        assert!(
            elapsed >= poll * 10,
            "the watch finished in {elapsed:?}, less than 10 poll periods -- it did not actually wait"
        );
    }

    #[test]
    fn boot_loop_watch_run_names_a_climbing_restart_count_once() {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let mut count = 0u64;
        let mut accusations = Vec::new();
        boot_loop_watch_run(
            Duration::from_millis(1),
            &stop,
            || {
                count += 1;
                if count >= 10 {
                    stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                Some(count)
            },
            |n| accusations.push(n),
        );
        assert_eq!(accusations.len(), 1, "latched: {accusations:?}");
        assert!(accusations[0] >= BOOT_LOOP_RESTART_STRIKES, "reported {}", accusations[0]);
    }

    #[test]
    fn boot_loop_watch_run_stops_when_asked_even_with_a_long_poll() {
        // The watch must never hold a finished lifecycle command open waiting
        // for its next tick.
        let stop = std::sync::atomic::AtomicBool::new(false);
        let t0 = std::time::Instant::now();
        boot_loop_watch_run(
            Duration::from_secs(30),
            &stop,
            || {
                stop.store(true, std::sync::atomic::Ordering::Relaxed);
                None
            },
            |_| panic!("an unreadable reading must never accuse anything"),
        );
        assert!(t0.elapsed() < Duration::from_secs(5), "took {:?} to notice the stop flag", t0.elapsed());
    }

    #[test]
    fn games_lifecycle_watches_boot_loop_on_start_and_restart_but_not_stop() {
        // The Home buttons that boot the server get the watch; `stop` has no
        // boot to diagnose.
        assert!(watches_boot_loop("start"));
        assert!(watches_boot_loop("restart"));
        assert!(!watches_boot_loop("stop"));
        assert!(!watches_boot_loop(""));
    }

    #[test]
    fn wr_wait_for_world_reports_a_boot_loop_once_without_changing_the_outcome() {
        // The incident: the world crash-retried for ten minutes while the
        // stream kept printing "still waiting ... bots respawning". The wait
        // must SAY SO -- and must still end exactly as it did before (this is
        // a diagnosis, not a new failure mode).
        let mut count = 0u64;
        let mut loops = Vec::new();
        let outcome = wr_wait_for_world(
            1,
            Duration::from_millis(1),
            || false,
            || Some(true), // up between crashes -- the liveness guard never trips
            || {
                count += 1;
                Some(count)
            },
            |_| {},
            |restarts| loops.push(restarts),
        );
        assert_eq!(outcome, WrWaitOutcome::Timeout, "the terminal outcome must be untouched");
        assert_eq!(loops.len(), 1, "the diagnosis must be latched, not repeated every poll: {loops:?}");
        assert!(
            loops[0] >= BOOT_LOOP_RESTART_STRIKES,
            "reported {} new restarts, fewer than the {BOOT_LOOP_RESTART_STRIKES} needed to conclude anything",
            loops[0]
        );
    }

    #[test]
    fn wr_wait_for_world_baselines_the_restart_count_instead_of_trusting_zero() {
        // A long-lived server can legitimately carry a big RestartCount from
        // days ago. Only restarts NEW since this wait began are evidence --
        // an absolute-threshold check would scream on the first poll.
        let mut probes = 0u32;
        let mut fired = 0u32;
        let outcome = wr_wait_for_world(
            1,
            Duration::from_millis(1),
            || false,
            || Some(true),
            || {
                probes += 1;
                Some(47)
            },
            |_| {},
            |_| fired += 1,
        );
        assert_eq!(outcome, WrWaitOutcome::Timeout);
        assert_eq!(fired, 0, "a stable (even large) restart count is a healthy slow boot");
        assert!(
            u64::from(probes) > BOOT_LOOP_RESTART_STRIKES,
            "only {probes} probes: fewer than the {BOOT_LOOP_RESTART_STRIKES} strikes needed, so this proves nothing"
        );
    }

    #[test]
    fn wr_wait_for_world_never_calls_a_boot_loop_on_an_unreadable_container() {
        // Same lesson the liveness guard already learned: "docker could not
        // answer" is not evidence of anything. An inconclusive restart-count
        // probe must never manufacture a boot-loop diagnosis.
        let mut probes = 0u32;
        let mut fired = 0u32;
        let outcome = wr_wait_for_world(
            1,
            Duration::from_millis(1),
            || false,
            || Some(true),
            || {
                probes += 1;
                None
            },
            |_| {},
            |_| fired += 1,
        );
        assert_eq!(outcome, WrWaitOutcome::Timeout);
        assert_eq!(fired, 0);
        assert!(u64::from(probes) > BOOT_LOOP_RESTART_STRIKES, "only {probes} probes -- proves nothing");
    }

    #[test]
    fn wr_wait_for_world_survives_a_hiccup_between_restart_count_readings() {
        // The baseline is the first READABLE reading; a later unreadable one
        // must neither reset it nor be counted as a restart.
        let mut probes = 0u32;
        let mut loops = Vec::new();
        let outcome = wr_wait_for_world(
            1,
            Duration::from_millis(1),
            || false,
            || Some(true),
            || {
                probes += 1;
                if probes % 2 == 0 {
                    None
                } else {
                    Some(10 + u64::from(probes) / 2)
                }
            },
            |_| {},
            |restarts| loops.push(restarts),
        );
        assert_eq!(outcome, WrWaitOutcome::Timeout);
        assert_eq!(loops.len(), 1, "a hiccup must not stop the loop from being recognised: {loops:?}");
    }

    #[test]
    fn wr_wait_for_world_reports_the_boot_loop_even_on_the_iteration_it_gives_up() {
        // A crash-looping container spends its backoff in `restarting`
        // (.State.Running == false), so the liveness guard may well fire first.
        // The user must still get the explanation before the error.
        //
        // The probes are tuned so BOTH conclusions land on the SAME iteration
        // (the restart count only crosses the threshold on the probe where the
        // liveness strikes reach WR_WORLD_DOWN_STRIKES). That makes this test
        // sensitive to the ORDER of the two checks: a boot-loop check placed
        // after the liveness `return` would never emit the note at all.
        let mut probes = 0u32;
        let mut loops = Vec::new();
        let outcome = wr_wait_for_world(
            10,
            Duration::from_millis(1),
            || false,
            || Some(false), // down every probe -> WorldDown at WR_WORLD_DOWN_STRIKES
            || {
                probes += 1;
                Some(if probes >= WR_WORLD_DOWN_STRIKES { BOOT_LOOP_RESTART_STRIKES } else { 0 })
            },
            |_| {},
            |restarts| loops.push(restarts),
        );
        assert_eq!(outcome, WrWaitOutcome::WorldDown, "the liveness guard still owns the outcome");
        assert_eq!(loops.len(), 1, "the diagnosis must be emitted before giving up: {loops:?}");
    }

    #[test]
    fn wr_world_down_error_shape_matches_the_bash_twin() {
        assert_eq!(
            wr_event_error("RESTART_FAILED", WR_WORLD_DOWN_MSG, WR_WORLD_DOWN_HINT),
            serde_json::json!({"event":"error","error":{
                "code":"RESTART_FAILED",
                "message":"The world server exited instead of coming back up",
                "hint":"Check the Console logs for the boot error; fix it and try a full Restart.",
            }})
        );
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
