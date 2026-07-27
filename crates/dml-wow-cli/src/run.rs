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
use dml_wow::soap::{SoapConfig, SoapOutcome};
use serde_json::{json, Value};

use crate::cli::{AccountCmd, Cmd, ConfigCmd, GmCmd, ModuleCmd, PartyCmd, TuningCmd};
use crate::out::{emit_err, emit_ok, stream_exit, stream_sink, TerminalSeen};

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
/// subcommand maps it: `Err(DbError)` is handed straight to
/// [`dml_wow::db::db_err_to_cmd`] — the SAME mapper `wow_bots_read` /
/// `wow_accounts_read` / `wow_paperdoll_read` (etc.) call in the launcher's
/// own native-mode Tauri commands (`launcher/src-tauri/src/lib.rs`). Reusing
/// it here (rather than hand-copying its code/message/hint into a second
/// constant, as an earlier revision of this file did — review finding 1)
/// means the CLI's `DB_UNREACHABLE` envelope can never drift from the
/// launcher's. `stats_err_to_cmd`, the launcher's own private mapper for the
/// `stats` Tauri command specifically, produces a byte-identical `CmdError`
/// (same code, same hint) for the same `DbError`, so `db_err_to_cmd` alone
/// covers every Task 12 subcommand including `stats` — no second export
/// needed.
fn emit_db_result(result: Result<Value, DbError>) -> i32 {
    emit_result(result.map_err(dml_wow::db::db_err_to_cmd))
}

/// Same mapping as [`emit_db_result`], but for a reader whose `Ok(None)`
/// means "no such character" — `paperdoll::read_paperdoll`,
/// `pages::read_char_progress` and `pages::read_achievements` all use this
/// shape (see each's own doc comment). `not_found_message` is the arm's own
/// wording (paperdoll's differs slightly from char-progress's/
/// achievements'), matching the launcher's `wow_*_read` commands exactly.
fn emit_db_option_result(result: Result<Option<Value>, DbError>, not_found_message: &str) -> i32 {
    match result {
        Ok(Some(data)) => emit_ok(data),
        Ok(None) => emit_err("NOT_FOUND", not_found_message, ""),
        Err(e) => emit_result(Err(dml_wow::db::db_err_to_cmd(e))),
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

/// A fresh, single-use SOAP lock for one `dml-wow` call that fires a SOAP
/// command (or several, in `party preset-load`'s case).
///
/// Same reasoning as [`write_lock`], for the launcher's `AppState::soap_lock`:
/// a GUI can have several Tauri commands in flight against ONE worldserver
/// SOAP listener, so it serializes them; this CLI runs one subcommand and
/// exits, so there is nothing in-process to serialize against. Cross-PROCESS
/// serialization is NOT provided and never was — two concurrent `dml-wow`
/// invocations (or a `dml-wow` alongside the GUI, or alongside the bash CLI)
/// can interleave their SOAP calls. That is parity with the bash CLI, which
/// takes no lock either, and it is recorded as a contract caveat rather than
/// silently implied by this parameter existing.
fn soap_lock() -> Arc<Mutex<()>> {
    Arc::new(Mutex::new(()))
}

/// One SOAP round trip: resolve the endpoint the way EVERY launcher native
/// command does (`SoapConfig::load()`), fire `cmd`, and hand the outcome to
/// the arm's own mapper. `map` also receives the resolved URL, because three
/// of the per-arm mappers (`console_send_result`, `account_result`) name the
/// endpoint in their SOAP_UNREACHABLE message.
///
/// The mappers all live in `dml_wow::soap_cmds` — deliberately: an arm's
/// fault/auth/unreachable strings are arm-specific (see that module's Task 13
/// section), and hand-copying one into this crate is exactly the drift the
/// Task 12 review ruled out for `db_err_to_cmd`. No arm below invents a code,
/// a message or a hint of its own for a SOAP failure.
fn soap_fire<T>(
    cmd: &str,
    map: impl FnOnce(SoapOutcome, &str) -> Result<T, CmdError>,
) -> Result<T, CmdError> {
    let cfg = SoapConfig::load();
    let outcome = dml_wow::soap::exec(&cfg, cmd);
    map(outcome, &cfg.url)
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

        Cmd::ItemInfo { ids } => {
            // The 25-id cap (review finding 2): a domain rejection, checked
            // BEFORE `read_item_info`'s own internal dedup so an argv list
            // like "1,1,1,...,1" (26 copies of the same id) still trips it,
            // matching the bash arm's `(( ${#earr[@]} > 25 ))` check (which
            // also runs before ITS dedup loop) and `wow_item_info_read`'s
            // identical `entries.len() > 25` check in the launcher. Same
            // code/message/hint as both siblings (`cli.rs`'s own
            // `parse_item_ids` only validates FORMAT, not cardinality — see
            // its doc comment for why this check lives here instead).
            if ids.0.len() > 25 {
                return emit_err("BAD_ARG", "--entries max 25 ids per call", "");
            }
            match dml_wow::cachestatus::cache_dir() {
                Some(cache_root) => {
                    let cfg = DbConfig::from_env();
                    emit_ok(dml_wow::iteminfo::read_item_info(&cache_root, Some(&cfg), &ids.0))
                }
                None => emit_err(
                    "INTERNAL",
                    "Could not resolve the wowhead cache directory",
                    "",
                ),
            }
        }

        // -- Task 13: SOAP write actions -----------------------------------
        //
        // GUARD DOCTRINE for every arm from here down. Several `dml-wow`
        // functions were moved verbatim out of the launcher in Task 9 while
        // the validation that protected them stayed behind in the Tauri
        // wrapper. So each arm below reproduces the FULL guard set its
        // `#[tauri::command]` sibling in `launcher/src-tauri/src/lib.rs` runs
        // before `spawn_blocking`, in the same order, BEFORE any SOAP call or
        // DB write. `require_native_backend()` is the one launcher guard with
        // no analogue here: it rejects a native-only command when the GUI is
        // on the WSL backend, and this binary IS the native backend (`version`
        // reports `"backend":"native"`), so there is no wrong-backend state to
        // refuse. Everything else is reproduced.

        Cmd::Console { command } => {
            let command = command.join(" ");
            // GUARD (`wow_console_send_native`): WHITESPACE-only is rejected,
            // not merely empty — bash tests `[[ -z "${cmd//[[:space:]]/}" ]]`,
            // so `dml-wow console "   "` must not reach the worldserver.
            if command.trim().is_empty() {
                return emit_err(
                    "BAD_ARG",
                    "console-send requires a non-empty --command",
                    "Example: dml wow console-send --command \"server info\" --json",
                );
            }
            emit_result(
                soap_fire(&command, dml_wow::soap_cmds::console_send_result)
                    .map(|result| json!({ "result": result })),
            )
        }

        Cmd::Account { cmd } => dispatch_account(cmd),
        Cmd::Gm { cmd } => dispatch_gm(cmd),
        Cmd::Party { cmd } => dispatch_party(cmd),

        Cmd::MailItem { to, items, subject, body } => {
            // Tokens are joined and re-split so that BOTH `6948:1 2589:5` and
            // `6948:1,2589:5` land on the same list, through bash's own
            // `IFS=','` semantics (`split_mail_items`) rather than a second
            // splitting rule invented here. `mail_items_cmd` then owns every
            // guard: recipient name, the 1-12 count, each `id:count` spec, and
            // the subject/body sanitization (strip `"`, CR/LF -> space) that
            // closes AC #2695. Nothing is re-sanitized or quote-wrapped here.
            let joined = items.join(",");
            let specs = dml_wow::soap_cmds::split_mail_items(&joined);
            let attachments = specs.len();
            let built = match dml_wow::soap_cmds::mail_items_cmd(&to, &specs, &subject, &body) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                soap_fire(&built, |o, _| dml_wow::soap_cmds::mail_result(o))
                    .map(|_| json!({ "sent": true, "to": to, "attachments": attachments })),
            )
        }

        Cmd::Teleport { char_name, to } => {
            // `teleport_name_cmd` validates the character name AND the
            // location token (single token, letters/digits/_/-) — the latter
            // matters because AC's console parser keeps quotes LITERAL, so a
            // destination is never quote-wrapped to make a space safe.
            let built = match dml_wow::soap_cmds::teleport_name_cmd(&char_name, &to) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                soap_fire(&built, |o, _| dml_wow::soap_cmds::teleport_result(o))
                    .map(|_| json!({ "teleported": true, "char": char_name, "to": to })),
            )
        }

        Cmd::Motd { text } => {
            // There is no standalone motd command in EITHER sibling: the
            // launcher reaches it through `wow_config_set_native` and bash
            // through `config set --key server.motd`, both of which land in
            // `config::config_set`'s `server.motd` special case. This arm goes
            // through that same door rather than calling `motd_cmd` directly,
            // which matters for safety: `motd_cmd` does NO sanitization of its
            // own (see its doc comment) — the `"text"` registry kind's
            // `sanitize_text_value` pass, applied by `config_set_curated`
            // immediately before it, is what strips quotes and CR/LF. Calling
            // the builder straight from here would have dropped that guard.
            emit_result(dml_wow::config::config_set(
                "server.motd".to_string(),
                text,
                soap_lock(),
                write_lock(),
                title_dir(),
            ))
        }
    }
}

/// `dml-wow account …` — the four SOAP account actions. The launcher runs
/// these as four near-identical `#[tauri::command]`s
/// (`wow_account_{create,set_password,set_gm,delete}_native`); their entire
/// guard set is "let the builder validate", since `account_create_cmd` /
/// `account_set_password_cmd` / `account_set_gm_cmd` / `account_delete_cmd`
/// each check their own arguments (and `delete` additionally refuses the
/// `admin` account the launcher itself uses for SOAP). Build FIRST, fire
/// second: an invalid username never reaches the worldserver.
fn dispatch_account(cmd: AccountCmd) -> i32 {
    let built = match &cmd {
        AccountCmd::Create { user, pass } => dml_wow::soap_cmds::account_create_cmd(user, pass),
        AccountCmd::SetPassword { user, pass } => {
            dml_wow::soap_cmds::account_set_password_cmd(user, pass)
        }
        AccountCmd::SetGm { user, level } => dml_wow::soap_cmds::account_set_gm_cmd(user, level),
        AccountCmd::Delete { user } => dml_wow::soap_cmds::account_delete_cmd(user),
    };
    let built = match built {
        Ok(c) => c,
        Err(e) => return emit_err(&e.code, &e.message, &e.hint),
    };
    if let Err(e) = soap_fire(&built, dml_wow::soap_cmds::account_result) {
        return emit_err(&e.code, &e.message, &e.hint);
    }
    emit_ok(match cmd {
        AccountCmd::Create { user, .. } => json!({ "created": true, "user": user }),
        AccountCmd::SetPassword { user, .. } => json!({ "password_set": true, "user": user }),
        AccountCmd::SetGm { user, level } => json!({
            "gm_set": true,
            "user": user,
            // A NUMBER in the envelope, matching `wow_account_set_gm_native`
            // (whose `level` arrives as a `u8` over IPC). The parse cannot
            // fail here: `account_set_gm_cmd` returned Ok, which it only does
            // for the literals "0".."3".
            "level": level.parse::<u8>().unwrap_or_default(),
        }),
        AccountCmd::Delete { user } => json!({ "deleted": true, "user": user }),
    })
}

/// The three bridge-backed GM ops (`gold`/`heal`/`revive`) share one shape:
/// REQUIRE the character online, THEN fire. The order is load-bearing and is
/// the oracle's (`_gm_require_online`, `cli/src/55-gm.sh:9-14`) as well as the
/// launcher's (`wow_gm_{gold,heal,revive}_native` all check before taking the
/// SOAP lock) — a SOAP fire is a side effect, so an offline target must be
/// refused before it happens, not after. `label` is the noun `party_fire_result`
/// splices into its fixed fault message.
fn gm_bridge_fire(player: &str, cmd: &str, label: &str) -> Result<(), CmdError> {
    let db_cfg = DbConfig::from_env();
    if !dml_wow::soap_cmds::char_is_online(&db_cfg, player) {
        return Err(dml_wow::soap_cmds::not_online_err(player));
    }
    soap_fire(cmd, |o, _| dml_wow::soap_cmds::party_fire_result(o, label)).map(|_| ())
}

/// `dml-wow gm …`. `level`/`at-login` are stock AzerothCore console commands
/// and work on OFFLINE characters (no online precondition — matching
/// `wow_gm_level_native`/`wow_gm_at_login_native`); `gold`/`heal`/`revive` go
/// through the DML bridges and require the character online
/// ([`gm_bridge_fire`]); `summon` is fully hoisted
/// (`soap_cmds::gm_summon` does creature lookup -> online check -> fire).
fn dispatch_gm(cmd: GmCmd) -> i32 {
    match cmd {
        GmCmd::Level { player, level } => {
            // Validates the name AND the 1..=255 range before anything fires.
            let built = match dml_wow::soap_cmds::gm_level_cmd(&player, level) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                soap_fire(&built, |o, _| dml_wow::soap_cmds::gm_level_result(o))
                    .map(|_| json!({ "leveled": true, "player": player, "level": level })),
            )
        }

        GmCmd::AtLogin { player, flag } => {
            // Validates the name AND the four-flag allowlist.
            let built = match dml_wow::soap_cmds::gm_at_login_cmd(&player, &flag) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                soap_fire(&built, |o, _| dml_wow::soap_cmds::gm_at_login_result(o))
                    .map(|_| json!({ "applied": true, "player": player, "flag": flag })),
            )
        }

        GmCmd::Gold { player, gold } => {
            let built = match dml_wow::soap_cmds::gm_gold_cmd(&player, gold) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                gm_bridge_fire(&player, &built, "gold")
                    .map(|_| json!({ "gold_set": true, "player": player, "gold": gold })),
            )
        }

        GmCmd::Heal { player } => {
            let built = match dml_wow::soap_cmds::gm_heal_cmd(&player) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                gm_bridge_fire(&player, &built, "heal")
                    .map(|_| json!({ "healed": true, "player": player })),
            )
        }

        GmCmd::Revive { player } => {
            let built = match dml_wow::soap_cmds::gm_revive_cmd(&player) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                gm_bridge_fire(&player, &built, "revive")
                    .map(|_| json!({ "revived": true, "player": player })),
            )
        }

        GmCmd::Summon { player, entry } => {
            // `gm_summon_cmd` validates the name and the 1..=999999 entry
            // range; `gm_summon` then owns the whole orchestration (World-DB
            // creature lookup -> online check -> fire), exactly as
            // `wow_gm_summon_native` calls it.
            let built = match dml_wow::soap_cmds::gm_summon_cmd(&player, entry) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(dml_wow::soap_cmds::gm_summon(player, entry, built, soap_lock()))
        }
    }
}

/// `dml-wow party …`. NOT ported here (still launcher-only): `dismiss-all`,
/// `preset-show`, `preset-import` — see the Task 13 report.
fn dispatch_party(cmd: PartyCmd) -> i32 {
    match cmd {
        PartyCmd::Add { player, class, gender, spec } => {
            // Guard set, in `wow_party_add_native`'s exact order:
            //  1. `party_add_cmd` validates player / class / gender;
            //  2. an empty `--spec` is treated as absent;
            //  3. a present spec is checked against the DEPLOYED
            //     playerbots.conf's premade spec names (falling back to the
            //     static mirror when no conf is deployed).
            // Only then does `party_add` touch the DB or SOAP.
            let gender = gender.unwrap_or_default();
            let built = match dml_wow::party::party_add_cmd(&player, &class, &gender) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            let spec = spec.filter(|s| !s.is_empty());
            if let Some(s) = &spec {
                let live = dml_wow::party::live_spec_names(&title_dir());
                if !dml_wow::party::valid_bot_spec(s, live.as_deref()) {
                    let e = dml_wow::party::unknown_spec_err(
                        s,
                        "A premade spec name like 'frost pve' -- see the launcher's role picker for the full list.",
                    );
                    return emit_err(&e.code, &e.message, &e.hint);
                }
            }
            emit_result(dml_wow::party::party_add(player, built, spec, soap_lock()))
        }

        PartyCmd::Kick { player, bot } => {
            // GUARD (`wow_party_kick_native`): the MASTER's name is validated
            // here with the arm's own hint — `party_uninvite_cmd` only sees
            // the bot, so without this an invalid master would reach the
            // logout whisper. Both command strings are built (and thereby
            // both names validated) before either fires.
            if !dml_wow::soap_cmds::valid_charname(&player) {
                let e = dml_wow::party::invalid_player_err(
                    &player,
                    "Kick needs --player (the bot's master) so the bot can also be dismissed.",
                );
                return emit_err(&e.code, &e.message, &e.hint);
            }
            let uninvite = match dml_wow::party::party_uninvite_cmd(&bot) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            let logout = match dml_wow::party::party_logout_whisper_cmd(&player, &bot) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            let cfg = SoapConfig::load();
            let outcome = dml_wow::soap::exec(&cfg, &uninvite);
            if let Err(e) = dml_wow::soap_cmds::party_fire_result(outcome, "kick") {
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // Best-effort, exactly like the launcher: a failed logout whisper
            // only flips `dismissed`, it never fails the kick.
            let dismissed =
                matches!(dml_wow::soap::exec(&cfg, &logout), SoapOutcome::Ok(_));
            emit_ok(json!({ "kicked": true, "dismissed": dismissed }))
        }

        PartyCmd::Relogin { player, bot } => {
            // `party_relogin_cmd` validates BOTH names.
            let built = match dml_wow::party::party_relogin_cmd(&player, &bot) {
                Ok(c) => c,
                Err(e) => return emit_err(&e.code, &e.message, &e.hint),
            };
            emit_result(
                soap_fire(&built, |o, _| dml_wow::soap_cmds::party_fire_result(o, "relogin"))
                    .map(|_| json!({ "relogged": true })),
            )
        }

        PartyCmd::Botcmd { player, bot, action, spec } => {
            // Guard set, in `wow_party_botcmd_native`'s exact order: both
            // names, then the closed action allowlist (with `spec`'s own
            // non-empty + live-validity pair), then BOTH parties online — the
            // bot's NOT_FOUND hint differs from every other party arm's — and
            // only then the whisper.
            if !dml_wow::soap_cmds::valid_charname(&player) {
                let e = dml_wow::party::invalid_player_err(&player, "");
                return emit_err(&e.code, &e.message, &e.hint);
            }
            if !dml_wow::soap_cmds::valid_charname(&bot) {
                let e = dml_wow::party::invalid_bot_err(&bot, "");
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // `Invalid action`/`Action spec requires --spec <name>` stay
            // inline literals (review Fix 3): each is spelled exactly ONCE
            // in this crate — `botcmd` is the only arm with an action
            // allowlist or a `spec`-needs-a-value case — so there is no
            // intra-crate copy for either to drift from. (Both also exist,
            // unavoidably duplicated, in the launcher's own
            // `wow_party_botcmd_native`, which this task's brief did not
            // allow touching — same status as the `eda5085`-hoisted SOAP
            // mappers' launcher-side copies.)
            let wmsg = if let Some(tail) = dml_wow::party::botcmd_fixed_tail(&action) {
                tail.to_string()
            } else if action == "spec" {
                let Some(spec_val) = spec.filter(|s| !s.is_empty()) else {
                    return emit_err(
                        "BAD_ARG",
                        "Action spec requires --spec <name>",
                        "e.g. --spec 'frost pve'",
                    );
                };
                let live = dml_wow::party::live_spec_names(&title_dir());
                if !dml_wow::party::valid_bot_spec(&spec_val, live.as_deref()) {
                    let e = dml_wow::party::unknown_spec_err(
                        &spec_val,
                        "A premade spec name like 'frost pve'.",
                    );
                    return emit_err(&e.code, &e.message, &e.hint);
                }
                dml_wow::party::spec_action_wmsg(&spec_val)
            } else {
                return emit_err(
                    "BAD_ARG",
                    &format!("Invalid action: {action}"),
                    "One of: gear talents maintain spec",
                );
            };
            let db_cfg = DbConfig::from_env();
            if dml_wow::party::party_online_guid(&db_cfg, &player).is_none() {
                let e = dml_wow::party::party_not_online_err(
                    &player,
                    "Log the character into the game first.",
                );
                return emit_err(&e.code, &e.message, &e.hint);
            }
            if dml_wow::party::party_online_guid(&db_cfg, &bot).is_none() {
                let e = dml_wow::party::party_not_online_err(
                    &bot,
                    "The bot must be in the world -- is it still in your party?",
                );
                return emit_err(&e.code, &e.message, &e.hint);
            }
            let whisper = dml_wow::party::botcmd_whisper_cmd(&player, &bot, &wmsg);
            emit_result(
                soap_fire(&whisper, |o, _| dml_wow::soap_cmds::party_fire_result(o, "botcmd")).map(
                    |_| json!({ "sent": true, "player": player, "bot": bot, "action": action }),
                ),
            )
        }

        PartyCmd::PresetSave { player, name } => {
            // GUARDS (`wow_party_preset_save_native`): the character name, and
            // the preset name — the latter is what keeps a `../…` argument
            // from ever being joined onto `~/.dml/party-presets`, so it MUST
            // run here, before `preset_save` builds a path.
            if !dml_wow::soap_cmds::valid_charname(&player) {
                let e = dml_wow::party::invalid_player_err(&player, "");
                return emit_err(&e.code, &e.message, &e.hint);
            }
            if !dml_wow::party::valid_preset_name(&name) {
                let e = dml_wow::party::invalid_preset_name_err(
                    &name,
                    "Letters, digits, - and _ (max 32).",
                );
                return emit_err(&e.code, &e.message, &e.hint);
            }
            emit_result(dml_wow::party::preset_save(player, name))
        }

        PartyCmd::PresetList => emit_result(dml_wow::party::preset_list()),

        PartyCmd::PresetDelete { name } => {
            // Same path-traversal guard as `preset-save`, with this arm's own
            // (empty) hint — `wow_party_preset_delete_native`.
            if !dml_wow::party::valid_preset_name(&name) {
                let e = dml_wow::party::invalid_preset_name_err(&name, "");
                return emit_err(&e.code, &e.message, &e.hint);
            }
            emit_result(dml_wow::party::preset_delete(name))
        }

        PartyCmd::PresetLoad { player, name } => {
            // The CLI's FIRST streaming subcommand. Both guards
            // (`wow_party_preset_load_native`) run BEFORE the stream starts,
            // so a rejection is a single ordinary error envelope + exit 1 —
            // never a half-emitted NDJSON stream, and never a preset path
            // built from an unvalidated name.
            if !dml_wow::soap_cmds::valid_charname(&player) {
                let e = dml_wow::party::invalid_player_err(&player, "");
                return emit_err(&e.code, &e.message, &e.hint);
            }
            if !dml_wow::party::valid_preset_name(&name) {
                let e = dml_wow::party::invalid_preset_name_err(&name, "");
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // Streaming wiring, per `out.rs`'s documented composition: the
            // stateless printer and the terminal-event tracker are separate,
            // and the closure feeds both. Exit code comes from which terminal
            // event ended the stream — `done` -> 0, `error` (or no terminal
            // event at all) -> 1.
            let seen = TerminalSeen::new();
            let sink = stream_sink();
            dml_wow::party::party_preset_load_stream(player, name, soap_lock(), |v| {
                seen.observe(&v);
                sink(v);
            });
            stream_exit(&seen)
        }
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
