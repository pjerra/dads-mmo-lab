//! Native-mode `games start`/`stop`/`restart` orchestration primitives
//! (spike: `spike/docker-desktop-native`, Chunk 3b). Faithful port of the
//! shared helpers `cli/src/90-main.sh`'s `games)` group leans on:
//! `_games_resolve_or_fail` (164-191), `_games_start_impl` (194-253, covers
//! BOTH `start` and `restart`), `_check_port_conflicts` (255-296), and
//! `_flush_heal_flag` (`cli/src/40-config.sh:774-785`).
//!
//! ARCHITECTURE, mirroring `dml::modmgr`: every REUSABLE, unit-testable
//! primitive (title/compose-dir resolution, the flush-heal breadcrumb
//! decision, the port-conflict line builder, the per-mode compose argv
//! sequence) lives here as a free function. The actual STREAMED Tauri
//! orchestration (`section_start`/`line`/`section_end`/`done`/`error`
//! sequencing, the real bounded `docker compose` spawns) lives in `lib.rs`
//! right next to `wow_world_restart_native_blocking`, which it follows
//! event-for-event.
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml` (`games_start`/
//! `games_stop`/`games_restart` in `lib.rs` branch on `is_native_backend()`
//! internally -- these are shared commands, not `_native` siblings, because
//! the Docker-Desktop-engine wrapping already lives inside them).

use std::path::{Path, PathBuf};
use std::time::Duration;

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
}
