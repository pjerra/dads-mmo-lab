//! Dispatch a parsed [`crate::cli::Cmd`] to its `dml-wow` call and print the
//! resulting envelope via `out.rs`.
//!
//! Kept deliberately thin: one match arm per subcommand, each calling
//! exactly one `dml-wow`/`dml-core` function and handing the result to
//! [`crate::out::emit_ok`]/[`crate::out::emit_err`]. No business logic lives
//! here — if a subcommand needs more than "call one function, emit its
//! result", that logic belongs in `dml-wow`.
//!
//! Title/config resolution mirrors the launcher's native-mode Tauri commands
//! EXACTLY (`wow_server_info_read`/`wow_server_detail_read` in
//! `launcher/src-tauri/src/lib.rs`): the same `SoapConfig::load()`,
//! `DbConfig::from_env()`, `ConfigReader::from_env()`, and
//! `dml_core::engine::docker_program()` calls, so the CLI and the GUI agree
//! on where a title/its DB/its SOAP endpoint live without a second config
//! path to keep in sync.

use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use dml_core::engine::docker_program;
use dml_core::error::CmdError;
use dml_wow::config::ConfigReader;
use dml_wow::db::{DbConfig, DbError};
use dml_wow::soap::SoapConfig;
use serde_json::{json, Value};

use crate::cli::{Cmd, ConfigCmd, ModuleCmd, TuningCmd};
use crate::out::{emit_err, emit_ok};

/// The hint every `DB_UNREACHABLE` envelope below carries — the SAME text
/// `db_err_to_cmd`/`stats_err_to_cmd` use in the launcher's own native-mode
/// Tauri commands (`launcher/src-tauri/src/lib.rs`).
const DB_UNREACHABLE_HINT: &str = "Is the server running? (docker + MySQL on 127.0.0.1:3306)";

/// Print a `dml-wow` call's `Result` as the one envelope it maps to, and
/// return the process exit code. NOT a second output path — it is the
/// two-line `Ok`/`Err` match that would otherwise be copy-pasted into every
/// fallible arm below, funnelled through the same
/// [`emit_ok`]/[`emit_err`] pair as everything else. `CmdError`'s three
/// fields ARE the error envelope's three fields, so the mapping is total and
/// lossless: no arm ever has to invent a code, a message, or a hint for a
/// failure the library already described.
fn emit_result(result: Result<Value, CmdError>) -> i32 {
    match result {
        Ok(data) => emit_ok(data),
        Err(e) => emit_err(&e.code, &e.message, &e.hint),
    }
}

/// Print a `dml_wow` DB page-reader's `Result` the way EVERY Task 12
/// subcommand maps it: `Err(DbError)` collapses to `DB_UNREACHABLE` for BOTH
/// variants (a query error on a reachable DB reports the same code as an
/// unreachable one) — the exact collapse `db_err_to_cmd`/`stats_err_to_cmd`
/// apply in the launcher's own native-mode Tauri commands, so a script
/// driving both surfaces sees identical error codes for identical failures.
fn emit_db_result(result: Result<Value, DbError>) -> i32 {
    match result {
        Ok(data) => emit_ok(data),
        Err(e) => emit_err("DB_UNREACHABLE", &e.to_string(), DB_UNREACHABLE_HINT),
    }
}

/// Same collapse as [`emit_db_result`], but for a reader whose `Ok(None)`
/// means "no such character" — `paperdoll::read_paperdoll`,
/// `pages::read_char_progress` and `pages::read_achievements` all use this
/// shape (see each's own doc comment). `not_found_message` is the arm's own
/// wording (paperdoll's differs slightly from char-progress's/
/// achievements'), matching the launcher's `wow_*_read` commands exactly.
fn emit_db_option_result(result: Result<Option<Value>, DbError>, not_found_message: &str) -> i32 {
    match result {
        Ok(Some(data)) => emit_ok(data),
        Ok(None) => emit_err("NOT_FOUND", not_found_message, ""),
        Err(e) => emit_err("DB_UNREACHABLE", &e.to_string(), DB_UNREACHABLE_HINT),
    }
}

/// A fresh, single-use write lock for one `dml-wow` writer call.
///
/// The launcher hands these writers long-lived `AppState` mutexes because a
/// GUI has many concurrent Tauri commands that can target the same conf file
/// (Settings, Module Tuning and the raw editor all can). This CLI is a
/// ONE-SHOT process that performs exactly ONE write and exits, so there is
/// nothing in-process to serialize against and a per-call lock is the honest
/// equivalent. Cross-PROCESS serialization is not something either sibling
/// has ever had — the bash CLI takes no lock either — so this is parity, not
/// a regression: the writers themselves are atomic (tmp + rename), so the
/// worst concurrent-editor outcome is last-writer-wins on a whole file, never
/// a torn one.
fn write_lock() -> Arc<Mutex<()>> {
    Arc::new(Mutex::new(()))
}

/// The single title dir every config/tuning/module command resolves against —
/// `DML_GAMES_DIR` + `wow-server-playerbots`, the SAME `ConfigReader` helper
/// the launcher's native Tauri commands call, so the CLI and the GUI can
/// never disagree about which files they are editing.
fn title_dir() -> PathBuf {
    ConfigReader::title_dir_from_env()
}

/// Read a `config write` body from stdin. Bytes must be valid UTF-8 — the
/// files this can target are all text confs, and `raw_write` takes a
/// `String`.
fn read_stdin_body() -> std::io::Result<String> {
    let mut body = String::new();
    std::io::stdin().lock().read_to_string(&mut body)?;
    Ok(body)
}

/// Run one parsed subcommand to completion, printing exactly one envelope on
/// stdout, and return the process exit code (0 ok / 1 error).
pub fn dispatch(command: Cmd) -> i32 {
    match command {
        Cmd::Version => emit_ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "contract": "dml-json-v3",
            "backend": "native",
        })),

        Cmd::Status => {
            let program = docker_program();
            let soap_cfg = SoapConfig::load();
            let db_cfg = DbConfig::from_env();
            let mut reader = ConfigReader::from_env();
            let detail = dml_wow::status::read_server_detail(&program, &soap_cfg, &db_cfg, &mut reader);
            emit_ok(detail)
        }

        Cmd::ServerInfo => {
            let soap_cfg = SoapConfig::load();
            match dml_wow::status::read_server_info(&soap_cfg) {
                Ok(info) => emit_ok(info),
                // `read_server_info` returns `Err(())` for exactly one case:
                // a SOAP AUTH failure (HTTP 401 -- wrong `~/.dml/soap.env`
                // credentials). A down/unreachable server is NOT this arm --
                // it folds into `Ok(server_info_down())` (`{"online":false,
                // ...}` is itself the answer; see `dml_wow::status`'s and
                // `wow_server_info_read`'s doc comments). So this mirrors the
                // launcher's own mapping (`lib.rs::wow_server_info_read`)
                // exactly, code/message/hint included, rather than a
                // "server unreachable" message that would be actively wrong
                // here (the server can be perfectly up with bad creds).
                Err(()) => emit_err(
                    "SOAP_AUTH",
                    "SOAP authentication failed",
                    "Check ~/.dml/soap.env",
                ),
            }
        }

        Cmd::ConsoleTail { lines } => {
            let program = docker_program();
            let tail = dml_wow::status::read_console_tail(&program, lines);
            emit_ok(tail)
        }

        Cmd::Config { cmd } => dispatch_config(cmd),
        Cmd::Tuning { cmd } => dispatch_tuning(cmd),
        Cmd::Module { cmd } => dispatch_module(cmd),

        Cmd::PlayersOnline => {
            let cfg = DbConfig::from_env();
            emit_db_result(dml_wow::pages::read_players_online(&cfg))
        }

        Cmd::Accounts => {
            let cfg = DbConfig::from_env();
            emit_db_result(dml_wow::pages::read_accounts(&cfg))
        }

        Cmd::Bots { name, class, min_level, max_level, online, limit, offset } => {
            // Validate BEFORE any SQL is built, matching the bash arm's own
            // doctrine (90-main.sh ~3884-3894) and the launcher's identical
            // native-mode pre-check (`wow_bots_read`, lib.rs) — not a defense
            // the bound-parameter query builder in `dml_wow::pages` needs,
            // parity with both siblings for invalid input.
            if let Some(n) = name.as_deref().filter(|n| !n.is_empty()) {
                if !dml_wow::paperdoll::valid_charname(n) {
                    return emit_err(
                        "BAD_ARG",
                        &format!("Invalid name prefix: {n}"),
                        "1-12 letters/digits/underscore.",
                    );
                }
            }
            if let Some(c) = class {
                if !dml_wow::pages::valid_bot_class(c) {
                    return emit_err("BAD_ARG", &format!("Invalid class id: {c}"), "1-9 or 11.");
                }
            }
            let cfg = DbConfig::from_env();
            let f = dml_wow::pages::BotFilters {
                name,
                class,
                min_level,
                max_level,
                online,
                limit: dml_wow::pages::clamp_limit(limit),
                offset,
            };
            emit_db_result(dml_wow::pages::read_bots(&cfg, &f))
        }

        Cmd::TeleportList { search } => {
            let cfg = DbConfig::from_env();
            emit_db_result(dml_wow::pages::read_teleport_list(&cfg, search.as_deref()))
        }

        Cmd::ItemsSearch { name, quality, min_level, max_level } => {
            // Rejects an empty/whitespace-only name BEFORE any SQL is built,
            // matching the bash arm's own pre-check and the launcher's
            // identical native-mode one (`wow_items_search_read`, lib.rs).
            if name.trim().is_empty() {
                return emit_err(
                    "BAD_ARG",
                    "items search requires a non-empty --name",
                    "Example: dml-wow items-search --name hearthstone",
                );
            }
            let cfg = DbConfig::from_env();
            let opts = dml_wow::pages::ItemSearchOpts { name, quality, min_level, max_level };
            emit_db_result(dml_wow::pages::read_items_search(&cfg, &opts))
        }

        Cmd::Paperdoll { name } => {
            if !dml_wow::paperdoll::valid_charname(&name) {
                return emit_err("BAD_ARG", &format!("Invalid character name: {name}"), "");
            }
            let cfg = DbConfig::from_env();
            emit_db_option_result(
                dml_wow::paperdoll::read_paperdoll(&cfg, &name),
                &format!("No such character or no equipped items: {name}"),
            )
        }

        Cmd::CharProgress { name } => {
            if !dml_wow::paperdoll::valid_charname(&name) {
                return emit_err("BAD_ARG", &format!("Invalid character name: {name}"), "");
            }
            let cfg = DbConfig::from_env();
            emit_db_option_result(
                dml_wow::pages::read_char_progress(&cfg, &name),
                &format!("No such character: {name}"),
            )
        }

        Cmd::Achievements { name } => {
            if !dml_wow::paperdoll::valid_charname(&name) {
                return emit_err("BAD_ARG", &format!("Invalid character name: {name}"), "");
            }
            let cfg = DbConfig::from_env();
            emit_db_option_result(
                dml_wow::pages::read_achievements(&cfg, &name),
                &format!("No such character: {name}"),
            )
        }

        Cmd::Stats => {
            let cfg = DbConfig::from_env();
            emit_db_result(dml_wow::stats::read_stats(&cfg))
        }

        Cmd::ItemInfo { ids } => match dml_wow::cachestatus::cache_dir() {
            Some(cache_root) => {
                let cfg = DbConfig::from_env();
                emit_ok(dml_wow::iteminfo::read_item_info(&cache_root, Some(&cfg), &ids.0))
            }
            None => emit_err(
                "INTERNAL",
                "Could not resolve the wowhead cache directory",
                "",
            ),
        },
    }
}

/// `dml-wow config …`. Every arm is one `dml-wow` call: the read arms use the
/// SAME `ConfigReader` + embedded registry the launcher's `wow_config_read`
/// uses, and the write arms call the hoisted `dml_wow::config` bodies
/// (`config_set`, `raw_write`) that `wow_config_set_native` /
/// `wow_config_raw_write_native` call — including, for `write`, the
/// allowlist that rejects `.env` and `docker-compose.override.yml`. None of
/// that logic is (or may be) duplicated here.
fn dispatch_config(cmd: ConfigCmd) -> i32 {
    match cmd {
        ConfigCmd::List => {
            let mut reader = ConfigReader::from_env();
            emit_ok(reader.assemble(dml_wow::registry::config_registry_rows()))
        }

        ConfigCmd::Get { key } => {
            let mut reader = ConfigReader::from_env();
            match reader.assemble_key(dml_wow::registry::config_registry_rows(), &key) {
                Some(row) => emit_ok(row),
                // Same NOT_FOUND wording `config_set_curated` gives an
                // unknown key (`90-main.sh:2441`), pointed at this CLI's own
                // listing arm. Mapped here rather than in the library for the
                // same reason `Cmd::ServerInfo` maps its one `Err(())` here:
                // the library answer is "no such row", and what that MEANS to
                // a caller is the caller's decision.
                None => emit_err(
                    "NOT_FOUND",
                    &format!("Unknown setting: {key}"),
                    "See: dml-wow config list",
                ),
            }
        }

        ConfigCmd::Set { key, value } => emit_result(dml_wow::config::config_set(
            key,
            value,
            write_lock(),
            write_lock(),
            title_dir(),
        )),

        ConfigCmd::Registry => {
            emit_ok(json!({ "settings": dml_wow::registry::config_registry_rows() }))
        }

        ConfigCmd::Files => emit_result(dml_wow::config::config_files(&title_dir())),

        ConfigCmd::Read { name } => emit_result(dml_wow::config::raw_read(name)),

        ConfigCmd::Write { name } => match read_stdin_body() {
            Ok(content) => emit_result(dml_wow::config::raw_write(name, content, write_lock())),
            Err(e) => emit_err(
                "BAD_ARG",
                &format!("Could not read the new file contents from stdin: {e}"),
                "Pipe the body in, e.g. dml-wow config write mod_x.conf < mod_x.conf",
            ),
        },
    }
}

/// `dml-wow tuning …` — the launcher's `wow_tuning_read` /
/// `wow_config_tuning_set_native` paths. Since Task 11's lua-writer port
/// (ruling D2) `tuning_set` is fully native for BOTH backends, so `tuning
/// set` needs no `DmlRunner` and never shells the bash CLI.
fn dispatch_tuning(cmd: TuningCmd) -> i32 {
    match cmd {
        TuningCmd::List => {
            let mut reader = dml_wow::tuning::TuningReader::from_env();
            emit_ok(reader.assemble(dml_wow::registry::tuning_registry_rows()))
        }
        TuningCmd::Set { key, value } => {
            emit_result(dml_wow::tuning::tuning_set(key, value, write_lock(), title_dir()))
        }
    }
}

/// `dml-wow module …` — the launcher's `wow_module_read` path (`list`) and
/// the embedded static catalog it is assembled from (`catalog`).
fn dispatch_module(cmd: ModuleCmd) -> i32 {
    match cmd {
        ModuleCmd::List => {
            let reader = dml_wow::modules::ModuleReader::from_env();
            emit_ok(reader.assemble(dml_wow::registry::module_catalog()))
        }
        ModuleCmd::Catalog => emit_ok(dml_wow::registry::module_catalog().clone()),
    }
}
