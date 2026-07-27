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
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dml_core::engine::docker_program;
use dml_core::error::CmdError;
use dml_wow::config::ConfigReader;
use dml_wow::db::{DbConfig, DbError};
use dml_wow::soap::{SoapConfig, SoapOutcome};
use serde_json::{json, Value};

use crate::cli::{
    AccountCmd, AccountwideCmd, BackupCmd, CacheCmd, Cmd, ClientPathCmd, ConfigCmd, GmCmd,
    ModuleCmd, PartyCmd, TuningCmd,
};
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

// -- Task 14 shared helpers -------------------------------------------------

/// Run one `dml-wow` streaming orchestration to completion and return the
/// process exit code, wiring `out.rs`'s documented composition ONCE instead of
/// in each of the fourteen streaming arms in this file: the stateless NDJSON printer
/// and the terminal-event tracker are separate objects, and the closure handed
/// to the orchestration feeds both. `run` takes `&dyn Fn(Value)` (rather than
/// being generic over the sink) so a single arm can drive SEVERAL streams onto
/// the same tracker — which `start` (engine-ensure then compose) and `stop`
/// (compose then engine-stop) both need.
///
/// Exit code: `0` iff the last terminal event seen was `done`; `1` for
/// `error`, and `1` for a stream that ended with NEITHER (a silent death is a
/// failure to report, not a success).
fn stream_dispatch(run: impl FnOnce(&dyn Fn(Value))) -> i32 {
    let seen = TerminalSeen::new();
    let sink = stream_sink();
    {
        let emit = |v: Value| {
            seen.observe(&v);
            sink(v);
        };
        run(&emit);
    }
    stream_exit(&seen)
}

/// The `--yes` refusal envelope, shared by the four IRREVERSIBLE subcommands
/// (`backup restore`, `docker-clean`, `bots-flush`, `games-remove`). PURE — no
/// I/O, no library call — so the gate is unit-testable without a server, a
/// docker engine, or even a stdout write, and so it CANNOT accidentally run
/// after something with a side effect.
///
/// This gate is the CLI's OWN: neither sibling has a `--yes` parameter at the
/// API level. The launcher's gate is a UI one (a typed-id / typed-"flush" /
/// two-click confirm) and its native commands hardcode `confirm = true`;
/// bash's `games remove`/`bots flush` arms do have `--yes`, but the launcher
/// hardcodes those too. A CLI has no UI to confirm in, so the flag IS the
/// confirmation. (`dml_wow::destructive::games_remove_stream` carries its own
/// ported CONFIRM_REQUIRED gate with a richer, targets-listing message; the
/// pre-gate here shadows it exactly the way the launcher's hardcoded
/// `confirm = true` does, and for the same reason — nothing may be probed,
/// stopped or deleted on the way to telling the caller to confirm.)
fn confirm_required(cmd: &str) -> CmdError {
    CmdError {
        code: "CONFIRM_REQUIRED".into(),
        message: format!("{cmd} is destructive; re-run with --yes"),
        hint: String::new(),
    }
}

/// `Some(refusal)` when `--yes` was not given, `None` when the command may
/// proceed. See [`confirm_required`].
fn confirm_refusal(cmd: &str, yes: bool) -> Option<CmdError> {
    (!yes).then(|| confirm_required(cmd))
}

/// `bots-flush`'s gate: BOTH `--yes` and the typed `--ack flush`.
///
/// The RULE is not reimplemented here — `dml_wow::lifecycle::
/// bots_flush_confirmed(confirm, ack)` already encodes it (it is the ported
/// `90-main.sh:3963` gate), and this only decides which of the two halves to
/// name in the refusal so the message is actionable rather than merely
/// correct.
fn bots_flush_refusal(yes: bool, ack: &str) -> Option<CmdError> {
    if dml_wow::lifecycle::bots_flush_confirmed(yes, ack) {
        return None;
    }
    let mut refusal = confirm_required("bots-flush");
    if yes {
        // `--yes` was given, so what is missing is the acknowledgement itself.
        refusal.message.push_str(" --ack flush");
    }
    Some(refusal)
}

/// The launcher's `validate_game_id` (`launcher/src-tauri/src/lib.rs:97-102`),
/// reproduced for the three lifecycle arms because `games_start`/`games_stop`/
/// `games_restart` run it before their native branch ever reaches
/// `games_lifecycle_stream` — which itself has NO id check (it goes straight to
/// `title_dir_for_id`, i.e. `DML_GAMES_DIR`-join-`id`). Controller ruling D1:
/// the guard stayed in the wrapper, so the CLI must carry its own copy.
///
/// Reproduced EXACTLY, warts included: this allows `.` and `..`, so
/// `--id ..` resolves to the games dir's PARENT. That is the launcher's
/// behaviour today and the CLI matches it rather than silently diverging; see
/// the task report's Concerns for the (pre-existing, launcher-side) finding.
fn valid_game_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The launcher's `bad_id` (`lib.rs:104-110`), same code/message/hint.
fn bad_id(id: &str) -> CmdError {
    CmdError {
        code: "BAD_ID".into(),
        message: format!("invalid game id: {id:?}"),
        hint: "Game ids come from games_list".into(),
    }
}

/// `~/.dml/backups`, or the launcher's own INTERNAL envelope when it cannot be
/// resolved — `wow_backup_{list,validate,delete}_native` each open with this
/// exact check.
fn backup_dir_or_err() -> Result<PathBuf, CmdError> {
    dml_wow::backup::backup_dir().ok_or_else(|| CmdError {
        code: "INTERNAL".into(),
        message: "Could not resolve the backups directory".into(),
        hint: String::new(),
    })
}

/// The launcher's `require_backup_file` (`lib.rs:4299-4308`), reproduced: the
/// shared `--file` gate for `backup validate`/`delete` — name shape first
/// (`BAD_ARG`, and this is the traversal guard, so it MUST precede the join),
/// then on-disk existence (`NOT_FOUND`). `backup restore` deliberately does
/// NOT use it: `restore::backup_restore_stream` runs both checks itself, in
/// the same order, in-stream — matching `wow_backup_restore_native`, which
/// likewise passes the name straight through.
fn require_backup_file(bdir: &Path, file: &str) -> Result<PathBuf, CmdError> {
    if !dml_wow::backup::valid_backup_name(file) {
        return Err(CmdError {
            code: "BAD_ARG".into(),
            message: format!("Invalid backup name: {file}"),
            hint: String::new(),
        });
    }
    let path = bdir.join(file);
    if !path.is_file() {
        return Err(CmdError {
            code: "NOT_FOUND".into(),
            message: format!("No backup named {file}"),
            hint: String::new(),
        });
    }
    Ok(path)
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

        // -- Task 14: lifecycle, backup/restore, destructive ----------------
        //
        // Every arm from here down STREAMS (see [`stream_dispatch`]) except
        // `backup list`/`validate`/`delete`. The `--yes` gates run FIRST, on
        // the pure [`confirm_refusal`]/[`bots_flush_refusal`] decisions, so a
        // refusal is one ordinary error envelope: no stream is opened, no
        // docker/SOAP/DB call is made, nothing on disk is read or written.
        Cmd::Start { id } => {
            // GUARD (`games_start`): the id, before `title_dir_for_id` joins it
            // onto `DML_GAMES_DIR`.
            if !valid_game_id(&id) {
                let e = bad_id(&id);
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // `games_start`'s native branch, in order: the Docker Desktop
            // engine is a HARD prerequisite (regardless of any toggle) — bring
            // it up first or abort before touching compose. Its failure path
            // already emits a terminal `error` event, so the tracker turns that
            // into exit 1 without this arm inventing an envelope of its own.
            stream_dispatch(|emit| {
                if dml_wow::native::ensure_engine_up_stream(emit).is_err() {
                    return;
                }
                dml_wow::lifecycle::games_lifecycle_stream("start", id, false, emit);
            })
        }

        Cmd::Stop { id, stop_engine, no_stop_engine } => {
            if !valid_game_id(&id) {
                let e = bad_id(&id);
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // `games_stop`'s native branch: stop the stack, THEN free the VM's
            // RAM by stopping the engine — best-effort, and only when the
            // manage-docker preference says so. The decision is
            // `dml_core::engine::stop_engine_enabled`'s, not a second copy:
            // `None` (neither flag) means "the caller didn't thread the
            // toggle", which that function documents as default-ON, the same
            // answer the launcher's default-checked Tools-page checkbox gives.
            // `native = true` unconditionally — this binary IS the native
            // backend (`version` reports `"backend":"native"`).
            let manage = match (stop_engine, no_stop_engine) {
                (true, _) => Some(true),
                (_, true) => Some(false),
                _ => None,
            };
            stream_dispatch(|emit| {
                // TERMINAL EVENT HELD BACK, and this is a CORRECTNESS fix, not
                // tidiness (Task 14 review, Fix 1). `stop` is the only arm that
                // writes anything AFTER its orchestration's terminal event, and
                // `out.rs`'s contract says a stream ENDS at that event. A
                // consumer that believes the contract — `dml-wow stop | head -n
                // <k>`, a `jq` `first(...)`, any reader that stops at
                // `done`/`error` — closes the pipe there; the next engine-stop
                // write then hits BrokenPipe, and
                // `print_stdout_line_or_exit`'s (correct, deliberate)
                // broken-pipe rule exits **0** — reporting success for a stop
                // that may have FAILED. Buffering the terminal event and
                // re-emitting it last makes the contract true again, so there
                // is never a write after it to lose the race on.
                //
                // The launcher's channel ordering (engine lines after the
                // terminal event) is NOT parity worth keeping here: a Tauri
                // Channel has no pipe to break and no exit code to corrupt.
                // Only the ORDER of the engine lines relative to the terminal
                // event differs; every line is still emitted, still in the same
                // order relative to each other.
                let held: std::cell::RefCell<Vec<Value>> = std::cell::RefCell::new(Vec::new());
                dml_wow::lifecycle::games_lifecycle_stream("stop", id, false, |v| {
                    if matches!(v["event"].as_str(), Some("done") | Some("error")) {
                        held.borrow_mut().push(v);
                    } else {
                        emit(v);
                    }
                });
                if dml_wow::native::stop_engine_enabled(true, manage) {
                    // Emits `line` events only — never a terminal one — so the
                    // exit code still comes from the stop above, exactly as the
                    // launcher's `result` does.
                    dml_wow::native::stop_engine_stream(emit);
                }
                // ...and now the stream really does end at its terminal event.
                for v in held.into_inner() {
                    emit(v);
                }
            })
        }

        Cmd::Restart { id, no_saveall } => {
            if !valid_game_id(&id) {
                let e = bad_id(&id);
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // No engine wrapping, matching `games_restart`: a restart assumes
            // the server -- and so the engine -- is already up.
            stream_dispatch(|emit| {
                dml_wow::lifecycle::games_lifecycle_stream("restart", id, no_saveall, emit)
            })
        }

        Cmd::Backup { cmd } => dispatch_backup(cmd),

        Cmd::DockerClean { level, yes } => {
            if let Some(e) = confirm_refusal("docker-clean", yes) {
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // `--level`'s 1..=3 rule is `docker_clean_stream`'s own first check
            // (in-stream BAD_ARG, before it touches docker) — `wow_docker_clean_native`
            // adds nothing either.
            stream_dispatch(|emit| dml_wow::destructive::docker_clean_stream(level, emit))
        }

        Cmd::BotsFlush { ack, yes } => {
            if let Some(e) = bots_flush_refusal(yes, &ack) {
                return emit_err(&e.code, &e.message, &e.hint);
            }
            let db_cfg = DbConfig::from_env();
            stream_dispatch(|emit| {
                dml_wow::destructive::bots_flush_stream(soap_lock(), db_cfg, emit)
            })
        }

        Cmd::GamesRemove { id, keep_data, remove_images, yes } => {
            if let Some(e) = confirm_refusal("games-remove", yes) {
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // No id guard here, matching `games_remove_native` exactly: the
            // traversal guard is `destructive::title_row`'s EXACT registry
            // match, which `games_remove_stream` runs before any path is built
            // (see its doc comment) — a stricter check here would only change
            // the error code for garbage input. `confirm = true` because
            // `--yes` was given; the gate above is what enforces that.
            stream_dispatch(|emit| {
                dml_wow::destructive::games_remove_stream(id, keep_data, remove_images, true, emit)
            })
        }

        Cmd::SelfUpdate { backup } => {
            // `backup.choice() == None` is passed THROUGH, not defaulted: it is
            // `update_stream`'s own last fail-closed gate ("Pick --backup or
            // --no-backup"), matching `wow_update_native`'s `Option<bool>`.
            stream_dispatch(|emit| dml_wow::maint::update_stream(backup.choice(), emit))
        }

        // -- Task 15: misc reads + the interactive `install` passthrough ----

        Cmd::Lan { action, ip, internet } => {
            // GUARD (`wow_lan_native`, lib.rs:4430-4437): `validate_lan_request`
            // owns the closed LAN_ACTIONS allowlist and the IP-vs-hostname
            // shape check, and MUST run before `lan_action` ever touches
            // docker/DB -- `lan_action` itself `.expect()`s ("validated: ip
            // required for on/refresh") and `unreachable!()`s on an
            // unvalidated action (see the task report's D1 section).
            match dml_wow::lan::validate_lan_request(&action, ip, internet) {
                Ok((inet, ip_arg)) => {
                    // `lan_action` is TEXT-mode (its own doc comment): every
                    // domain-level outcome (not installed, docker down, DB not
                    // answering yet, ...) is folded into the returned
                    // `Ok(String)` rather than a typed error, matching
                    // `wow_lan_native`'s `Result<String, CmdError>` (whose
                    // `Err` side is reserved for a `spawn_blocking` join
                    // failure, not reachable here since this CLI runs
                    // synchronously). So this arm always emits an ok envelope
                    // wrapping that text -- never an error envelope, mirroring
                    // the launcher's own contract.
                    let text = dml_wow::lan::lan_action(&action, ip_arg, inet);
                    emit_ok(json!({ "output": text }))
                }
                Err(e) => emit_err(&e.code, &e.message, &e.hint),
            }
        }

        Cmd::Cache { cmd } => dispatch_cache(cmd),
        Cmd::ClientPath { cmd } => dispatch_client_path(cmd),
        Cmd::Accountwide { cmd } => dispatch_accountwide(cmd),

        Cmd::Commands => {
            // GUARD (`wow_commands_read`, lib.rs:1899-1924): NOT_FOUND before
            // the catalog/reader are even touched, exact code/message/hint.
            let title_dir = title_dir();
            if dml_wow::maint::resolve_server_dir(&title_dir).is_none() {
                return emit_err(
                    "NOT_FOUND",
                    "WoW Playerbots server not installed",
                    "Install it first.",
                );
            }
            let catalog = dml_wow::registry::module_catalog();
            let reader = dml_wow::modules::ModuleReader::from_env();
            let modules_dir = title_dir.join("modules");
            emit_ok(dml_wow::commands::assemble_commands(catalog, &reader, &modules_dir))
        }

        Cmd::Install { id } => cmd_install(&id),
    }
}

/// `dml-wow cache …` — `dml_wow::cachestatus`. No guards beyond
/// `require_native_backend` (a no-op here, this binary IS the native
/// backend) in either `wow_cache_status_read`/`wow_cache_clean_native`.
fn dispatch_cache(cmd: CacheCmd) -> i32 {
    match cmd {
        CacheCmd::Status => emit_ok(dml_wow::cachestatus::read_cache_status()),
        CacheCmd::Clean => emit_result(dml_wow::cachestatus::clean_cache().map_err(|e| match e {
            dml_wow::cachestatus::CacheCleanError::Guard(m) => {
                CmdError { code: "INTERNAL".into(), message: m, hint: String::new() }
            }
            dml_wow::cachestatus::CacheCleanError::Wipe(m) => {
                CmdError { code: "WIPE_FAILED".into(), message: m, hint: String::new() }
            }
        })),
    }
}

/// `dml-wow client-path …` — `dml_wow::clientpath`. Same "no guards beyond
/// require_native_backend" shape as `dispatch_cache` for `get`/`detect`
/// (`wow_client_path_read`/`wow_client_path_detect_read`); `set` is
/// "let the builder validate" (`wow_client_path_set_native`), same doctrine
/// as Task 13's account commands — `set_client_path` owns the BAD_PATH/
/// NOT_CLIENT wording.
fn dispatch_client_path(cmd: ClientPathCmd) -> i32 {
    match cmd {
        ClientPathCmd::Get => emit_ok(dml_wow::clientpath::read_client_path()),
        ClientPathCmd::Set { dir } => {
            emit_result(dml_wow::clientpath::set_client_path(Path::new(&dir)).map_err(|e| match e {
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
            }))
        }
        ClientPathCmd::Detect => {
            let roots = dml_wow::clientpath::default_scan_roots();
            let candidates = dml_wow::clientpath::detect_client(&roots);
            emit_ok(json!({ "candidates": candidates }))
        }
    }
}

/// `dml-wow accountwide …` — `dml_wow::accountwide`.
fn dispatch_accountwide(cmd: AccountwideCmd) -> i32 {
    match cmd {
        AccountwideCmd::Get => {
            // GUARD (`wow_accountwide_get_native`, lib.rs:2397-2413): NOT_FOUND
            // before `build_get` is ever called, exact code/message/hint.
            let title_dir = title_dir();
            match dml_wow::maint::resolve_server_dir(&title_dir) {
                Some(server_dir) => emit_ok(dml_wow::accountwide::build_get(&server_dir)),
                None => emit_err(
                    "NOT_FOUND",
                    "WoW Playerbots server not installed",
                    "Install it first.",
                ),
            }
        }

        AccountwideCmd::Set { key, value, variant } => {
            // GUARD SET (`wow_accountwide_set_native`, lib.rs:2423-2457), in
            // its EXACT order — value shape, then flag-name shape, BOTH
            // before the server-dir resolve (`set_flag` re-validates both
            // internally too, "belt and braces, not the only gate" per that
            // wrapper's own doc comment; reproducing the pre-checks here is
            // what keeps a bad `--value`/key rejected with BAD_ARG even
            // against an uninstalled server, rather than NOT_FOUND winning
            // by dumb luck of which check runs first).
            if value != "on" && value != "off" {
                return emit_err("BAD_ARG", "--value must be on or off", "");
            }
            if !dml_wow::accountwide::valid_flag(&key) {
                return emit_err(
                    "BAD_ARG",
                    &format!("Invalid flag name: {key}"),
                    "Flags look like ENABLE_ACCOUNTWIDE_MOUNTS -- see: dml wow accountwide get --json",
                );
            }
            let title_dir = title_dir();
            let server_dir = match dml_wow::maint::resolve_server_dir(&title_dir) {
                Some(d) => d,
                None => {
                    return emit_err(
                        "NOT_FOUND",
                        "WoW Playerbots server not installed",
                        "Install it first.",
                    )
                }
            };
            emit_result(
                dml_wow::accountwide::set_flag(&server_dir, &key, &value, variant.as_deref())
                    .map_err(|e| CmdError { code: e.code.into(), message: e.message, hint: e.hint }),
            )
        }
    }
}

// -- `install`: the interactive, non-JSON passthrough (task brief's ONE
// deliberate deviation) --------------------------------------------------
//
// `games_install` (lib.rs:5531-5583) is the launcher's ONLY install command
// (shared by both backends through `state.runner`, no `_native` sibling to
// diverge from) and it has NO id validation of its own -- only a "an install
// is already running" BUSY guard, which does not apply to a one-shot CLI
// process. The `valid_game_id` check below is therefore a CLI-specific
// hardening addition, not a reproduction of an existing wrapper guard (see
// the task report).
//
// Runner construction reuses `dml_core::runner::DmlRunner::native()` rather
// than re-deriving `find_bash`/`find_dml_script`'s resolution by hand: its
// `program`/`prefix_args`/`path_prepend` fields are already `pub`, and are
// the EXACT values the native backend's own `dml` invocations use (Docker
// Desktop's bin dir on PATH included, so the installer's own `docker`
// calls resolve exactly like every other native command's do). Its
// `spawn_interactive`/`run_captured`/etc. methods are not used here because
// every one of them pipes or nulls at least one stdio stream; a true
// interactive passthrough needs all three INHERITED, so this builds its own
// `Command` from those public fields instead.
fn cmd_install(id: &str) -> i32 {
    if !valid_game_id(id) {
        let e = bad_id(id);
        return emit_err(&e.code, &e.message, &e.hint);
    }

    let runner = dml_core::runner::DmlRunner::native();
    let script = runner.prefix_args.first().cloned().unwrap_or_default();
    if !resolves_on_path_or_disk(&runner.program) || !Path::new(&script).is_file() {
        return emit_err(
            "INSTALL_PREREQS",
            "Git Bash (or bash on PATH) and the dml script (DML_SCRIPT) are required for install",
            "",
        );
    }

    let mut cmd = std::process::Command::new(&runner.program);
    cmd.args(&runner.prefix_args).args(["games", "install", id]);
    if let Some(dir) = &runner.path_prepend {
        if !dir.is_empty() {
            let mut paths = vec![std::path::PathBuf::from(dir)];
            if let Some(cur) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&cur));
            }
            if let Ok(joined) = std::env::join_paths(paths) {
                cmd.env("PATH", joined);
            }
        }
    }
    // ALL THREE stdio streams inherited -- no pipes -- so the installer's
    // prompts and the user's answers pass straight through. This is the
    // task brief's ONE deliberate non-JSON, non-NDJSON command: no envelope
    // is printed on this path, ever -- the installer's own raw output IS
    // the output.
    cmd.stdin(std::process::Stdio::inherit());
    cmd.stdout(std::process::Stdio::inherit());
    cmd.stderr(std::process::Stdio::inherit());

    match cmd.spawn().and_then(|mut child| child.wait()) {
        Ok(status) => std::process::exit(status.code().unwrap_or(1)),
        // Unreachable in ordinary operation (the preflight above just
        // confirmed both resolve) short of a race (the file vanishing
        // between the check and the spawn) or a permissions error -- handled
        // rather than unwrapped, but deliberately its own distinct code so it
        // is never mistaken for the preflight's own INSTALL_PREREQS.
        Err(e) => emit_err(
            "INSTALL_SPAWN_FAILED",
            &format!("failed to launch the installer: {e}"),
            "",
        ),
    }
}

/// Does `candidate` (an `OsString`, as `DmlRunner::native().program` is)
/// actually resolve to a real, runnable file, WITHOUT spawning anything?
///
/// `find_bash` (`dml_core::runner`) returns one of three shapes: (a) a
/// `DML_BASH` override, taken VERBATIM with no existence check of its own;
/// (b) one of two hardcoded Git-Bash absolute paths, already
/// existence-checked before `find_bash` returns it; (c) the ultimate bare
/// `"bash"` fallback, which relies on PATH resolution at spawn time. Cases
/// (a)/(b) are both "looks like a path" (contains a separator) and get a
/// direct [`Path::is_file`] check; case (c) is a bare command name and gets
/// a manual PATH walk instead, since `Command::spawn` performs that same
/// resolution internally and there is no portable "does this resolve"
/// query short of trying it.
fn resolves_on_path_or_disk(candidate: &std::ffi::OsStr) -> bool {
    let text = candidate.to_string_lossy();
    if text.contains(['\\', '/']) {
        Path::new(candidate).is_file()
    } else {
        on_path(&text)
    }
}

/// `true` iff `name` resolves to a file in some `PATH` directory. Thin env
/// wrapper over the pure [`finds_on`] so the actual search logic is
/// testable without mutating the process-wide `PATH` (which every parallel
/// `cargo test` thread in this binary shares). No `PATH` at all is "not
/// found", not "found everywhere".
fn on_path(name: &str) -> bool {
    let Some(path_var) = std::env::var_os("PATH") else { return false };
    finds_on(name, std::env::split_paths(&path_var))
}

/// Pure search core of [`on_path`]: `true` iff `name` is a file directly
/// under some directory in `dirs` (with a bare `.exe` fallback on Windows,
/// mirroring how `CreateProcess` resolves an extension-less name).
fn finds_on(name: &str, dirs: impl IntoIterator<Item = PathBuf>) -> bool {
    dirs.into_iter().any(|dir| {
        dir.join(name).is_file() || (cfg!(windows) && dir.join(format!("{name}.exe")).is_file())
    })
}

/// `dml-wow backup …` — the launcher's five native backup commands.
/// `create`/`restore` stream; `list`/`validate`/`delete` are plain envelopes.
fn dispatch_backup(cmd: BackupCmd) -> i32 {
    match cmd {
        BackupCmd::Create { include_world, name } => {
            // Additive: writes ONE new `.sql.gz` (plus its sidecar) under
            // `~/.dml/backups` and prunes to the keep-N pool. No guards in
            // `wow_backup_create_native` beyond the backend check; `name` is
            // sanitized inside `backup_create_stream`.
            let db_cfg = DbConfig::from_env();
            stream_dispatch(|emit| {
                dml_wow::backup::backup_create_stream(include_world, name, db_cfg, emit)
            })
        }

        BackupCmd::List => emit_result(backup_dir_or_err().map(|bdir| {
            let backups: Vec<Value> = dml_wow::backup::list_backups(&bdir)
                .into_iter()
                .map(|e| {
                    json!({
                        "file": e.file, "size": e.size, "created": e.created,
                        "world": e.world, "summary": e.summary, "name": e.name,
                    })
                })
                .collect();
            json!({ "backups": backups })
        })),

        BackupCmd::Validate { file } => emit_result(backup_dir_or_err().and_then(|bdir| {
            let path = require_backup_file(&bdir, &file)?;
            let result = dml_wow::backup::validate_backup(&path);
            Ok(dml_wow::backup::validate_result_json(&file, &result))
        })),

        BackupCmd::Delete { file } => emit_result(backup_dir_or_err().and_then(|bdir| {
            // The name gate runs BEFORE the delete, and the existence check
            // before that decides NOT_FOUND vs a silent no-op — both are
            // `require_backup_file`'s, so `delete_backup` (which also drops the
            // `.meta.json` sidecar) only ever sees a validated, present file.
            require_backup_file(&bdir, &file)?;
            dml_wow::backup::delete_backup(&bdir, &file);
            Ok(json!({ "deleted": true, "file": file }))
        })),

        BackupCmd::Restore { file, yes } => {
            if let Some(e) = confirm_refusal("backup restore", yes) {
                return emit_err(&e.code, &e.message, &e.hint);
            }
            // No name/existence pre-check, matching `wow_backup_restore_native`:
            // `backup_restore_stream` runs `valid_backup_name` then the on-disk
            // check as its own first two steps, in-stream, before it stops a
            // single container.
            let db_cfg = DbConfig::from_env();
            stream_dispatch(|emit| {
                dml_wow::restore::backup_restore_stream(file, soap_lock(), db_cfg, emit)
            })
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
            // event at all) -> 1. That composition now lives in ONE place,
            // [`stream_dispatch`] (Task 14): this arm was where it was first
            // spelled out, and hand-inlining it a second time in each of the
            // thirteen arms that task added would have been thirteen chances
            // for one of them to forget the tracker and always exit 0.
            stream_dispatch(|emit| {
                dml_wow::party::party_preset_load_stream(player, name, soap_lock(), emit)
            })
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

/// `dml-wow module …` — the launcher's `wow_module_read` path (`list`), the
/// embedded static catalog it is assembled from (`catalog`), and the five
/// MUTATING commands (Task 14).
///
/// GUARD NOTE (controller ruling D1). `install`/`remove`/`update`/`rebuild`
/// need NO guards here, and that is a checked fact rather than an assumption:
/// `wow_module_{install,remove,update,rebuild}_native` run `require_native_backend()`
/// and NOTHING else before `spawn_blocking` — every rule (the family
/// allowlist, `valid_cpp_key`, `valid_module_url`, the `--url`/`--key`
/// exclusivity, the per-family backup-flag rules, "not installed", "not a git
/// checkout", the mod-playerbots refusal) lives inside the `_stream` functions
/// and their `install_*`/`remove_*`/`update_module` bodies, which emit their
/// own in-stream BAD_ARG/NOT_FOUND before any git/docker/SQL call.
/// `repair` is the exception, and the one this task's brief names: its three
/// closed-allowlist checks STAYED in the Tauri wrapper, and
/// `moduletail::module_repair` PANICS (`database_for_short(&db).expect(
/// "validated above")`) on a `--db` that never passed one. All three are
/// reproduced below, in the wrapper's order.
fn dispatch_module(cmd: ModuleCmd) -> i32 {
    match cmd {
        ModuleCmd::List => {
            let reader = dml_wow::modules::ModuleReader::from_env();
            emit_ok(reader.assemble(dml_wow::registry::module_catalog()))
        }
        ModuleCmd::Catalog => emit_ok(dml_wow::registry::module_catalog().clone()),

        ModuleCmd::Install { family, key, url, variant, backup } => {
            let db_cfg = DbConfig::from_env();
            let backup = backup.choice();
            stream_dispatch(|emit| {
                dml_wow::modmgr::module_install_stream(family, key, url, backup, variant, db_cfg, emit)
            })
        }

        ModuleCmd::Remove { family, key, backup } => {
            let db_cfg = DbConfig::from_env();
            let backup = backup.choice();
            stream_dispatch(|emit| {
                dml_wow::modmgr::module_remove_stream(family, key, backup, db_cfg, emit)
            })
        }

        ModuleCmd::Update { key } => {
            stream_dispatch(|emit| dml_wow::modmgr::module_update_stream(key, emit))
        }

        ModuleCmd::Rebuild { backup } => {
            let db_cfg = DbConfig::from_env();
            let backup = backup.choice();
            stream_dispatch(|emit| dml_wow::modmgr::module_rebuild_stream(backup, db_cfg, emit))
        }

        ModuleCmd::Repair { key, db, mode, files } => {
            // GUARDS (`wow_module_repair_native`, lib.rs:1050-1058), same
            // order, same code/message/hint. The `--db` one is load-bearing
            // beyond input hygiene: without it `module_repair`'s
            // `database_for_short(&db).expect("validated above")` PANICS
            // (exit 101, no envelope at all) instead of answering BAD_ARG.
            if !dml_wow::modules::valid_cpp_key(&key) {
                return emit_err("BAD_ARG", &format!("Invalid module key: {key}"), "");
            }
            if !matches!(db.as_str(), "world" | "characters" | "auth") {
                return emit_err(
                    "BAD_ARG",
                    &format!("Invalid --db: {db}"),
                    "Use world, characters, or auth.",
                );
            }
            if !matches!(mode.as_str(), "mark" | "clear") {
                return emit_err("BAD_ARG", &format!("Invalid --mode: {mode}"), "Use mark or clear.");
            }
            emit_result(dml_wow::moduletail::module_repair(key, db, mode, files))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Task 14: the `--yes` gate ------------------------------------------
    //
    // These are the PURE halves of the destructive arms' guards, unit-tested
    // in-process with no server, no docker engine, no filesystem and no stdout
    // write — which is exactly what makes them safe to assert on: a test that
    // could only observe the gate by letting the command run would have to run
    // the command. The "fires BEFORE any side effect" half is proven
    // separately, at the process boundary, in `tests/cli_integration.rs`
    // (sealed environment + before/after snapshots).

    #[test]
    fn confirm_refusal_is_none_when_yes_is_given() {
        assert!(confirm_refusal("backup restore", true).is_none());
        assert!(confirm_refusal("docker-clean", true).is_none());
        assert!(confirm_refusal("games-remove", true).is_none());
    }

    #[test]
    fn confirm_refusal_is_the_briefs_exact_envelope_without_yes() {
        for cmd in ["backup restore", "docker-clean", "games-remove"] {
            let e = confirm_refusal(cmd, false).expect("must refuse without --yes");
            assert_eq!(e.code, "CONFIRM_REQUIRED");
            assert_eq!(e.message, format!("{cmd} is destructive; re-run with --yes"));
            assert_eq!(e.hint, "");
        }
    }

    /// `bots-flush` needs BOTH halves. The rule itself is
    /// `lifecycle::bots_flush_confirmed`'s (asserted here against the same
    /// truth table so a drift in either direction fails), and only the WORDING
    /// distinguishes which half was missing.
    #[test]
    fn bots_flush_refusal_requires_both_yes_and_the_typed_ack() {
        // Passes only for (true, "flush") -- and agrees with the library rule.
        assert!(bots_flush_refusal(true, "flush").is_none());
        assert!(dml_wow::lifecycle::bots_flush_confirmed(true, "flush"));

        for (yes, ack) in [(false, "flush"), (false, ""), (true, ""), (true, "FLUSH"), (true, "yes")]
        {
            assert!(
                !dml_wow::lifecycle::bots_flush_confirmed(yes, ack),
                "library rule changed for ({yes}, {ack:?})"
            );
            let e = bots_flush_refusal(yes, ack)
                .unwrap_or_else(|| panic!("must refuse ({yes}, {ack:?})"));
            assert_eq!(e.code, "CONFIRM_REQUIRED");
            assert_eq!(e.hint, "");
            if yes {
                assert_eq!(e.message, "bots-flush is destructive; re-run with --yes --ack flush");
            } else {
                assert_eq!(e.message, "bots-flush is destructive; re-run with --yes");
            }
        }
    }

    // -- the reproduced launcher guards --------------------------------------

    #[test]
    fn valid_game_id_matches_the_launchers_rule() {
        assert!(valid_game_id("wow-server-playerbots"));
        assert!(valid_game_id("wow_tbc.server-1"));
        assert!(!valid_game_id(""));
        assert!(!valid_game_id("wow/../etc"));
        assert!(!valid_game_id("wow server"));
        assert!(!valid_game_id("wow;rm"));
        assert!(!valid_game_id("wow\\evil"));
        // Reproduced warts (see `valid_game_id`'s doc comment + the report):
        // the launcher's rule accepts bare dots.
        assert!(valid_game_id(".."));
    }

    #[test]
    fn bad_id_is_the_launchers_exact_envelope() {
        let e = bad_id("wow server");
        assert_eq!(e.code, "BAD_ID");
        assert_eq!(e.message, "invalid game id: \"wow server\"");
        assert_eq!(e.hint, "Game ids come from games_list");
    }

    /// Controller ruling D1, made a checkable fact rather than a claim:
    /// `moduletail::module_repair` ends its argument handling with
    /// `database_for_short(&db).expect("validated above")`, so the `--db`
    /// allowlist in [`dispatch_module`]'s `Repair` arm is the ONLY thing
    /// standing between a bad `--db` and a process panic (exit 101, no
    /// envelope at all — a broken contract, not an error). This pins that:
    /// exactly the three allowlisted spellings resolve, and every near-miss
    /// the guard rejects would indeed have reached the `expect`.
    #[test]
    fn the_repair_db_allowlist_is_load_bearing_not_decorative() {
        for good in ["world", "characters", "auth"] {
            assert!(
                dml_wow::moduletail::database_for_short(good).is_some(),
                "{good} must be accepted by BOTH the guard and the library"
            );
        }
        for bad in ["acore_world", "World", "AUTH", "", "world ", "world;drop"] {
            assert!(
                dml_wow::moduletail::database_for_short(bad).is_none(),
                "`module repair --db {bad}` would hit `.expect(\"validated above\")` and PANIC"
            );
        }
    }

    #[test]
    fn require_backup_file_rejects_a_bad_name_before_touching_the_dir() {
        // A dir that does NOT exist: if the name gate did not run first, the
        // join+`is_file` would be the thing that failed, and with the WRONG
        // code (NOT_FOUND instead of BAD_ARG).
        let nowhere = std::env::temp_dir().join("dml-cli-t14-no-such-backup-dir");
        for bad in ["../../etc/passwd", "wow.sql.gz\0", "not a backup", ""] {
            let e = require_backup_file(&nowhere, bad).expect_err("must reject");
            assert_eq!(e.code, "BAD_ARG", "{bad:?}");
            assert_eq!(e.message, format!("Invalid backup name: {bad}"));
        }
    }

    #[test]
    fn require_backup_file_reports_not_found_for_a_wellformed_absent_file() {
        let nowhere = std::env::temp_dir().join("dml-cli-t14-no-such-backup-dir");
        let e = require_backup_file(&nowhere, "wow-20260101-000000.sql.gz").expect_err("absent");
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "No backup named wow-20260101-000000.sql.gz");
    }

    #[test]
    fn require_backup_file_accepts_a_present_file() {
        let dir = std::env::temp_dir()
            .join(format!("dml-cli-t14-backupdir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let name = "wow-20260101-000000.sql.gz";
        std::fs::write(dir.join(name), b"not really gzip").unwrap();
        assert_eq!(require_backup_file(&dir, name).unwrap(), dir.join(name));
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- Task 15: `install`'s preflight resolver -----------------------------
    //
    // `finds_on`/`resolves_on_path_or_disk` are tested with an EXPLICIT
    // directory list / absolute temp paths rather than mutating the
    // process-wide `PATH` env var, which every parallel `cargo test` thread
    // in this binary shares (mutating it would be exactly the kind of
    // flaky-by-construction test this crate's own doc comments warn against
    // elsewhere).

    fn t15_dir(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dml-cli-t15-{}-{tag}", std::process::id()))
    }

    #[test]
    fn finds_on_locates_a_bare_name_in_a_directory_list() {
        let dir = t15_dir("finds-on");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bash"), b"").unwrap();

        assert!(finds_on("bash", vec![dir.clone()]));
        assert!(!finds_on("bash", vec![t15_dir("finds-on-nowhere")]));
        assert!(!finds_on("bash", Vec::<PathBuf>::new()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The Windows-only `.exe` fallback, mirroring how `CreateProcess`
    /// resolves an extension-less name -- exercised only on the platform it
    /// applies to (matching `finds_on`'s own `cfg!(windows)` gate).
    #[test]
    fn finds_on_matches_the_dotexe_fallback_on_windows() {
        let dir = t15_dir("finds-on-exe");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bash.exe"), b"").unwrap();

        assert_eq!(finds_on("bash", vec![dir.clone()]), cfg!(windows));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_on_path_or_disk_checks_a_path_shaped_candidate_directly() {
        let dir = t15_dir("resolves-abs");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let real = dir.join("bash.exe");
        std::fs::write(&real, b"").unwrap();

        assert!(resolves_on_path_or_disk(real.as_os_str()));
        let missing = dir.join("does-not-exist.exe");
        assert!(!resolves_on_path_or_disk(missing.as_os_str()));

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A bare name (no path separators) is never treated as found just
    /// because IT happens to be a relative filename that doesn't exist in
    /// the test process's cwd -- it must go through the PATH search branch
    /// (proven indirectly: this cwd never contains a file literally named
    /// `definitely-not-a-real-binary-xyz`).
    #[test]
    fn resolves_on_path_or_disk_bare_name_does_not_shortcut_to_true() {
        assert!(!resolves_on_path_or_disk(std::ffi::OsStr::new(
            "definitely-not-a-real-binary-xyz"
        )));
    }
}
