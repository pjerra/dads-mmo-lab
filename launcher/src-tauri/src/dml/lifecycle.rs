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

/// `GAMES_DIR` base -- same env var + fallback as `ConfigReader::
/// title_dir_from_env()`, but generalized to an arbitrary title `id` rather
/// than hardcoding `wow-server-playerbots` (mirrors `_games_resolve_or_fail`'s
/// `dir="$GAMES_DIR/$gid"`, which works for any installed title).
pub fn games_dir_from_env() -> PathBuf {
    std::env::var_os("DML_GAMES_DIR")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

/// `$GAMES_DIR/$gid` (`90-main.sh:174`).
pub fn title_dir_for_id(id: &str) -> PathBuf {
    games_dir_from_env().join(id)
}

fn has_compose(dir: &Path) -> bool {
    ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
        .iter()
        .any(|name| dir.join(name).is_file())
}

/// `_resolve_compose_dir` (`90-main.sh:61-71`): the title dir itself if it
/// carries a compose file, else its first subdir that does. `None` when
/// neither exists (title installed via `install.sh` only, or truly missing
/// its compose file) -- the caller's `NO_COMPOSE` gate.
pub fn resolve_compose_dir(title_dir: &Path) -> Option<PathBuf> {
    if has_compose(title_dir) {
        return Some(title_dir.to_path_buf());
    }
    let entries = std::fs::read_dir(title_dir).ok()?;
    entries.flatten().map(|e| e.path()).find(|p| p.is_dir() && has_compose(p))
}

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

/// Best-effort "is something already listening on this port" probe: attempt
/// to bind the wildcard address, exactly the address `docker-proxy` would
/// need for a published container port. A bind failure (any reason --
/// already bound, permission, whatever) reads as "in use"; this is
/// deliberately conservative (warn-only caller, never a hard gate).
pub fn port_listening(port: u16) -> bool {
    std::net::TcpListener::bind(("0.0.0.0", port)).is_err()
}

// ---------------------------------------------------------------------------
// Compose command sequencing -- `_games_start_impl`'s mode branch
// (`90-main.sh:225-236`, the bash arm's ELSE/no-`dml-start.sh` path) + the
// `stop)` arm's single `down` (`90-main.sh:1116`). Pure argv builders so the
// exact sequence per mode is independently unit-testable.
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

pub fn compose_up_argv() -> Vec<&'static str> {
    vec!["compose", "up", "-d"]
}

pub fn compose_down_argv() -> Vec<&'static str> {
    vec!["compose", "down", "-t", "180"]
}

/// `true` when `argv` is a `down` invocation, `false` for `up` -- argv[0] is
/// always `"compose"` (both builders above start with it), so the token that
/// actually distinguishes them is argv[1]. Used to pick the right progress
/// line + timeout per step of [`compose_sequence_for_mode`]'s sequence.
pub fn is_compose_down(argv: &[&str]) -> bool {
    argv.get(1) == Some(&"down")
}

/// The ordered sequence of `docker compose` argv's a `games` mode runs --
/// `start` = `[up]`; `restart` = `[down, up]` (mirrors the bash's own
/// `mode=="restart" && down; up` sequencing, stopping at the first
/// failure); `stop` = `[down]`. An unrecognized mode yields an empty
/// sequence (the caller never invokes this with anything else).
pub fn compose_sequence_for_mode(mode: &str) -> Vec<Vec<&'static str>> {
    match mode {
        "start" => vec![compose_up_argv()],
        "restart" => vec![compose_down_argv(), compose_up_argv()],
        "stop" => vec![compose_down_argv()],
        _ => vec![],
    }
}

/// The GUI's "faster restart" (`--no-saveall`) info line on the native
/// raw-compose path. WSL threads `DML_SKIP_SAVEALL` into `dml-start.sh`,
/// which then skips its own pre-stop SOAP `saveall` call; the native path
/// never runs `dml-start.sh` at all (KEY FACT: the native title dir has
/// none) so there is no separate pre-stop saveall step to skip in the first
/// place -- the flag is a no-op here. This line makes that explicit instead
/// of silently swallowing the option.
pub const SKIP_SAVEALL_NOTE: &str = "faster-restart requested -- the native compose path has no separate pre-stop saveall to skip; the graceful `docker compose down` already saves characters on shutdown.";

#[cfg(test)]
mod tests {
    use super::*;

    // -- title/compose-dir resolution --------------------------------------
    //
    // NOTE: `games_dir_from_env`/`title_dir_for_id` read the process-global
    // `DML_GAMES_DIR` env var directly -- deliberately NOT exercised here via
    // `std::env::set_var` (this crate's `cargo test` runs multi-threaded in
    // one process; mutating a process-global var would race any other test
    // reading it concurrently). The join logic itself is a one-line
    // `base.join(id)`, covered indirectly by every `resolve_compose_dir` test
    // below via a directly-constructed title dir.

    #[test]
    fn resolve_compose_dir_at_root() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("docker-compose.yml"), "").unwrap();
        assert_eq!(resolve_compose_dir(&dir), Some(dir.clone()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_compose_dir_in_subdir() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-sub-{}", std::process::id()));
        let sub = dir.join("wow-server-playerbots");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("compose.yaml"), "").unwrap();
        assert_eq!(resolve_compose_dir(&dir), Some(sub.clone()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_compose_dir_none_when_no_compose_anywhere() {
        let dir = std::env::temp_dir().join(format!("dml-lifecycle-test-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subdir")).unwrap();
        assert_eq!(resolve_compose_dir(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

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

    // -- compose sequencing ---------------------------------------------------

    #[test]
    fn compose_sequence_start_is_up_only() {
        assert_eq!(compose_sequence_for_mode("start"), vec![compose_up_argv()]);
    }

    #[test]
    fn compose_sequence_restart_is_down_then_up() {
        assert_eq!(compose_sequence_for_mode("restart"), vec![compose_down_argv(), compose_up_argv()]);
    }

    #[test]
    fn compose_sequence_stop_is_down_only() {
        assert_eq!(compose_sequence_for_mode("stop"), vec![compose_down_argv()]);
    }

    #[test]
    fn compose_sequence_unknown_mode_is_empty() {
        assert!(compose_sequence_for_mode("bogus").is_empty());
    }

    #[test]
    fn compose_argv_exact_tokens() {
        assert_eq!(compose_up_argv(), vec!["compose", "up", "-d"]);
        assert_eq!(compose_down_argv(), vec!["compose", "down", "-t", "180"]);
    }

    #[test]
    fn is_compose_down_distinguishes_up_from_down() {
        assert!(is_compose_down(&compose_down_argv()));
        assert!(!is_compose_down(&compose_up_argv()));
    }

    #[test]
    fn is_compose_down_every_step_of_every_mode_sequence() {
        // Regression: argv[0] is "compose" for BOTH builders, so a naive
        // `argv.first() == Some(&"down")` check is always false -- this
        // exercises the real per-mode sequence end-to-end, the shape a live
        // round-trip caught the bug in.
        for (mode, expect_down) in [("start", vec![false]), ("restart", vec![true, false]), ("stop", vec![true])] {
            let got: Vec<bool> = compose_sequence_for_mode(mode).iter().map(|argv| is_compose_down(argv)).collect();
            assert_eq!(got, expect_down, "mode={mode}");
        }
    }
}
