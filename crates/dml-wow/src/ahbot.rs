//! Native-mode `wow ahbot repair` pure helpers (Chunk 2 task C2c item 8 --
//! see `.superpowers/sdd/chunk2-decisions.md`). A faithful port of the
//! module-detection and fork-specific conf-key logic from
//! `90-main.sh:4239-4416`.
//!
//! LAYOUT. The pure half -- which fork is installed, and which conf keys to
//! write for it -- comes first and is testable without a live server or
//! filesystem writes. The orchestration ([`ahbot_repair_stream`]) follows:
//! the DB lookup (`SELECT guid, account ...`), the `mod_ahbot.conf` writes
//! (via `config::conf_write`, REUSED not reimplemented -- Subsystem B1), the
//! legacy-env cleanup, and the SOAP `reload config` call, all streamed as
//! NDJSON. That half moved out of the launcher's `lib.rs` in the
//! cargo-workspace refactor (Task 9) so the standalone CLI can drive it too;
//! it takes the two serialization mutexes (SOAP + config write) as plain
//! `Arc<Mutex<()>>` parameters the caller supplies.
//!
//! FORK DIFFERENCE (verified against each fork's `conf/mod_ahbot.conf.dist`,
//! 2026-07-20, carried over from the bash comment at `90-main.sh:4337-4345`):
//! the original `azerothcore/mod-ah-bot` keys a single `Account` + `GUID`
//! with separate `Enable{Seller,Buyer}`; the `NathanHandley/mod-ah-bot-plus`
//! fork renamed these -- no `Account`, a plural `GUIDs`, and the buyer switch
//! nested under `Buyer.Enabled` -- only `EnableSeller` is shared.

use std::path::Path;
use std::sync::{Arc, Mutex};

use super::config::{cfgset_clean_legacy_env, env_frozen};
use super::db::sql_row_int;

/// The one manual step the automation can never do (character creation is
/// client-side only) -- surfaced both as an error hint and in the `done`
/// payload, verbatim (`90-main.sh:4271`).
pub const MANUAL_STEPS: &str = "Create a separate account for the bot (Accounts page), log into the game with it once, create ONE character, log out, then pick that character here.";

/// Which AH-bot fork is installed under `<server dir>/modules` -- a plain-dir
/// presence check, mirroring `90-main.sh:4295-4297`. `mod-ah-bot-plus` wins
/// if somehow both are present ("install one or the other, not both").
pub fn detect_module(server_dir: &Path) -> Option<&'static str> {
    if server_dir.join("modules").join("mod-ah-bot-plus").is_dir() {
        Some("mod-ah-bot-plus")
    } else if server_dir.join("modules").join("mod-ah-bot").is_dir() {
        Some("mod-ah-bot")
    } else {
        None
    }
}

/// Fork-specific `mod_ahbot.conf` keys to write, in write order -- a port of
/// the `ah_keys` array build (`90-main.sh:4346-4350`). Any `module` other
/// than `"mod-ah-bot-plus"` gets the original fork's keys (matches the
/// bash's `if/else`, not an exhaustive match on both names).
pub fn conf_keys(module: &str, guid: u64, account: u64) -> Vec<(&'static str, String)> {
    if module == "mod-ah-bot-plus" {
        vec![
            ("AuctionHouseBot.GUIDs", guid.to_string()),
            ("AuctionHouseBot.EnableSeller", "1".to_string()),
            ("AuctionHouseBot.Buyer.Enabled", "1".to_string()),
        ]
    } else {
        vec![
            ("AuctionHouseBot.Account", account.to_string()),
            ("AuctionHouseBot.GUID", guid.to_string()),
            ("AuctionHouseBot.EnableSeller", "1".to_string()),
            ("AuctionHouseBot.EnableBuyer", "1".to_string()),
        ]
    }
}

fn ar_event_section_start() -> serde_json::Value {
    serde_json::json!({"event": "section_start", "name": "ahbot-repair"})
}

fn ar_event_line(level: &str, text: impl Into<String>) -> serde_json::Value {
    serde_json::json!({"event": "line", "level": level, "text": text.into()})
}

fn ar_event_section_end(status: &str) -> serde_json::Value {
    serde_json::json!({"event": "section_end", "name": "ahbot-repair", "status": status})
}

#[allow(clippy::too_many_arguments)]
fn ar_event_done(
    char_name: &str,
    guid: u64,
    account: u64,
    applied: &str,
    restart_required: bool,
    module: &str,
    already: bool,
) -> serde_json::Value {
    serde_json::json!({"event": "done", "data": {
        "repaired": true,
        "already": already,
        "char": char_name,
        "guid": guid,
        "account": account,
        "applied": applied,
        "restart_required": restart_required,
        "module": module,
        "manual_steps": crate::ahbot::MANUAL_STEPS,
    }})
}

fn ar_event_error(code: &str, message: impl Into<String>, hint: &str) -> serde_json::Value {
    serde_json::json!({"event": "error", "error": {"code": code, "message": message.into(), "hint": hint}})
}

/// The blocking flow itself (real DB/fs/docker/SOAP I/O) -- run under
/// `spawn_blocking`. A faithful port of the `ahbot) repair)` arm
/// (`90-main.sh:4242-4410`), same order: char-name shape -> installed? ->
/// module detected? -> conf ensured? -> character lookup -> conf writes ->
/// legacy-env cleanup -> apply (live reload or restart-required).
pub fn ahbot_repair_stream(
    char_name: String,
    soap_lock: Arc<Mutex<()>>,
    config_lock: Arc<Mutex<()>>,
    emit: impl Fn(serde_json::Value),
) {
    use crate::{ahbot, config, db, maint, soap};

    emit(ar_event_section_start());

    if !crate::soap_cmds::valid_charname(&char_name) {
        emit(ar_event_section_end("error"));
        emit(ar_event_error(
            "BAD_ARG",
            "ahbot repair needs --char <the bot character's name>",
            ahbot::MANUAL_STEPS,
        ));
        return;
    }

    let title_dir = config::ConfigReader::title_dir_from_env();
    let Some(sdir) = maint::resolve_server_dir(&title_dir) else {
        emit(ar_event_section_end("error"));
        emit(ar_event_error("NOT_FOUND", "WoW Playerbots server not installed", "Install it first."));
        return;
    };

    let Some(ahmod) = ahbot::detect_module(&sdir) else {
        emit(ar_event_section_end("error"));
        emit(ar_event_error(
            "NOT_INSTALLED",
            "No Auction House Bot module is installed",
            "Install Auction House Bot (or Auction House Bot Plus) from the Modules page first.",
        ));
        return;
    };

    let ahconf = config::conf_path_in(&sdir, "mod_ahbot.conf");
    // Serializes against every other native conf/override write, same
    // discipline as `wow_config_set_native` -- `mod_ahbot.conf` is exactly
    // the shared-file example `AppState::config_lock`'s own doc comment
    // names.
    let _guard = config_lock.lock().unwrap_or_else(|e| e.into_inner());

    let ensured = match config::conf_ensure(&ahconf) {
        Ok(v) => v,
        Err(e) => {
            emit(ar_event_section_end("error"));
            emit(ar_event_error("WRITE_FAILED", format!("Could not write mod_ahbot.conf: {e}"), ""));
            return;
        }
    };
    if !ensured {
        emit(ar_event_section_end("error"));
        emit(ar_event_error(
            "NOT_FOUND",
            "mod_ahbot.conf not found (nor its .dist)",
            "Is the module fully installed? Try a rebuild from the Modules page.",
        ));
        return;
    }

    emit(ar_event_line("info", format!("looking up character {char_name}...")));
    let db_cfg = db::DbConfig::from_env();
    // utf8mb4_bin lookup -- canonical form or nothing (db::canon_char_name).
    let params: Vec<mysql::Value> = vec![mysql::Value::from(db::canon_char_name(char_name.as_str()))];
    let res = match db::query_with_params(
        &db_cfg,
        db::Database::Characters,
        "SELECT guid, account FROM characters WHERE name = ? LIMIT 1",
        params,
    ) {
        Ok(r) => r,
        Err(_e) => {
            emit(ar_event_section_end("error"));
            emit(ar_event_error("DB_UNREACHABLE", "Could not look up the character", "Is the server (ac-database) running?"));
            return;
        }
    };
    let Some(row) = res.rows.first() else {
        emit(ar_event_section_end("error"));
        emit(ar_event_error("NOT_FOUND", format!("No character named {char_name} exists yet"), ahbot::MANUAL_STEPS));
        return;
    };
    let guid = sql_row_int(row.first()).filter(|g| *g >= 0);
    let acct = sql_row_int(row.get(1)).filter(|a| *a >= 0);
    let (guid, acct) = match (guid, acct) {
        (Some(g), Some(a)) => (g as u64, a as u64),
        _ => {
            emit(ar_event_section_end("error"));
            emit(ar_event_error("DB_UNREACHABLE", "Unexpected character lookup result", ""));
            return;
        }
    };

    emit(ar_event_line("info", format!("selected: {char_name} (guid {guid}, account {acct})")));

    let keys = ahbot::conf_keys(ahmod, guid, acct);
    let mut cfg_changed = false;
    for (k, v) in &keys {
        match config::conf_write(&ahconf, k, v) {
            Ok(c) => cfg_changed = cfg_changed || c,
            Err(_e) => {
                emit(ar_event_section_end("error"));
                emit(ar_event_error("WRITE_FAILED", "Could not write mod_ahbot.conf", ""));
                return;
            }
        }
    }
    emit(ar_event_line("info", format!("wrote mod_ahbot.conf for {char_name} (guid {guid}): seller + buyer on")));

    // Legacy-env cleanup, one key at a time (mirrors the oracle's single
    // per-key if/elif -- NOT a "check removal for all keys, then check
    // frozen for all keys" two-pass, since a frozen line must be emitted for
    // EVERY matching key, not just the first).
    let override_path = sdir.join("docker-compose.override.yml");
    let mut env_was = false;
    for (k, _) in &keys {
        let ename = config::env_name_for(k);
        match cfgset_clean_legacy_env(&override_path, &ename) {
            Ok(true) => {
                env_was = true;
                cfg_changed = true;
                emit(ar_event_line(
                    "info",
                    format!("removed old override {ename} (the running server still has it until a restart)"),
                ));
            }
            Ok(false) => {
                if env_frozen(&ename) {
                    env_was = true;
                    emit(ar_event_line(
                        "info",
                        format!("the running server still carries {ename} from when it started - a restart is needed"),
                    ));
                }
            }
            Err(e) => {
                emit(ar_event_section_end("error"));
                emit(ar_event_error(&e.code, e.message, &e.hint));
                return;
            }
        }
    }

    let (applied, restart_required, already) = if cfg_changed {
        if !env_was {
            emit(ar_event_line("info", "asking the running server to reload its config..."));
            let _guard = soap_lock.lock().unwrap_or_else(|e| e.into_inner());
            let soap_cfg = soap::SoapConfig::load();
            let outcome = soap::exec(&soap_cfg, "reload config");
            if matches!(outcome, soap::SoapOutcome::Ok(_)) {
                emit(ar_event_line("info", format!("reloaded - the auction bot switches to {char_name} without a restart")));
                ("live".to_string(), false, false)
            } else {
                emit(ar_event_line("info", "server not reachable - the change applies on the next start"));
                ("restart".to_string(), true, false)
            }
        } else {
            ("restart".to_string(), true, false)
        }
    } else {
        emit(ar_event_line("info", format!("already configured for {char_name} - nothing to change")));
        ("none".to_string(), false, true)
    };

    emit(ar_event_section_end("ok"));
    emit(ar_event_done(&char_name, guid, acct, &applied, restart_required, ahmod, already));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tmp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("dml_ahbot_test_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn detect_module_none_when_neither_installed() {
        let dir = tmp_dir("none");
        std::fs::create_dir_all(dir.join("modules")).unwrap();
        assert_eq!(detect_module(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_plain_fork() {
        let dir = tmp_dir("plain");
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot")).unwrap();
        assert_eq!(detect_module(&dir), Some("mod-ah-bot"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_plus_fork() {
        let dir = tmp_dir("plus");
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot-plus")).unwrap();
        assert_eq!(detect_module(&dir), Some("mod-ah-bot-plus"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_plus_wins_when_both_present() {
        let dir = tmp_dir("both");
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot")).unwrap();
        std::fs::create_dir_all(dir.join("modules").join("mod-ah-bot-plus")).unwrap();
        assert_eq!(detect_module(&dir), Some("mod-ah-bot-plus"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_module_ignores_a_file_named_like_the_module() {
        // A plain-dir check (`is_dir()`) must not be fooled by a stray file.
        let dir = tmp_dir("file_not_dir");
        std::fs::create_dir_all(dir.join("modules")).unwrap();
        std::fs::write(dir.join("modules").join("mod-ah-bot"), b"not a dir").unwrap();
        assert_eq!(detect_module(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn conf_keys_plus_fork_shape() {
        let keys = conf_keys("mod-ah-bot-plus", 42, 7);
        assert_eq!(
            keys,
            vec![
                ("AuctionHouseBot.GUIDs", "42".to_string()),
                ("AuctionHouseBot.EnableSeller", "1".to_string()),
                ("AuctionHouseBot.Buyer.Enabled", "1".to_string()),
            ]
        );
    }

    #[test]
    fn conf_keys_plain_fork_shape() {
        let keys = conf_keys("mod-ah-bot", 42, 7);
        assert_eq!(
            keys,
            vec![
                ("AuctionHouseBot.Account", "7".to_string()),
                ("AuctionHouseBot.GUID", "42".to_string()),
                ("AuctionHouseBot.EnableSeller", "1".to_string()),
                ("AuctionHouseBot.EnableBuyer", "1".to_string()),
            ]
        );
    }

    #[test]
    fn ar_event_section_start_shape() {
        assert_eq!(ar_event_section_start(), serde_json::json!({"event":"section_start","name":"ahbot-repair"}));
    }

    #[test]
    fn ar_event_line_shape() {
        assert_eq!(
            ar_event_line("info", "selected: Bob (guid 5, account 3)"),
            serde_json::json!({"event":"line","level":"info","text":"selected: Bob (guid 5, account 3)"})
        );
    }

    #[test]
    fn ar_event_section_end_shape() {
        assert_eq!(ar_event_section_end("ok"), serde_json::json!({"event":"section_end","name":"ahbot-repair","status":"ok"}));
        assert_eq!(ar_event_section_end("error"), serde_json::json!({"event":"section_end","name":"ahbot-repair","status":"error"}));
    }

    #[test]
    fn ar_event_done_shape() {
        assert_eq!(
            ar_event_done("Bob", 5, 3, "live", false, "mod-ah-bot-plus", false),
            serde_json::json!({"event":"done","data":{
                "repaired": true,
                "already": false,
                "char": "Bob",
                "guid": 5,
                "account": 3,
                "applied": "live",
                "restart_required": false,
                "module": "mod-ah-bot-plus",
                "manual_steps": crate::ahbot::MANUAL_STEPS,
            }})
        );
    }

    #[test]
    fn ar_event_error_shape() {
        assert_eq!(
            ar_event_error("NOT_INSTALLED", "No Auction House Bot module is installed", "Install it first."),
            serde_json::json!({"event":"error","error":{
                "code":"NOT_INSTALLED",
                "message":"No Auction House Bot module is installed",
                "hint":"Install it first.",
            }})
        );
    }
}
