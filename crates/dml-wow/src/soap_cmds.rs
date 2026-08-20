//! Pure SOAP command layer — validators, command-string builders, fault-text
//! decoding, and the `SoapOutcome -> CmdError` mapper for the launcher's
//! account/GM/mail/teleport/motd write actions.
//!
//! Ported byte-for-byte against the bash parity oracle `cli/src/90-main.sh`
//! (arms named on each item below) and `cli/src/45-console.sh:18-27`
//! (`_soap_text_decode`). PURE logic only — no network, no Tauri, no DB.
//! Later tasks (A2b/A2c) wire these into `#[tauri::command]`s that call
//! `crate::soap::exec` and hand the resulting [`SoapOutcome`] to the
//! mappers at the bottom of this file.

use std::sync::{Arc, Mutex};

use super::soap::SoapOutcome;
use dml_core::error::CmdError;

// ---------------------------------------------------------------------
// Validators — port of `cli/src/90-main.sh:116-138`'s allowlist regexes.
// Every character class here is single-byte ASCII, so `str::len()` (bytes)
// and `chars().count()` agree — no separate byte-vs-char bookkeeping needed.
// ---------------------------------------------------------------------

/// `^[A-Za-z0-9_]{3,20}$` — SOAP account username (`_valid_account_user`).
pub fn valid_account_user(s: &str) -> bool {
    len_and_charset(s, 3, 20, |c| c.is_ascii_alphanumeric() || c == '_')
}

/// `^[A-Za-z0-9_@#%+=!-]{4,16}$` — SOAP account password (`_valid_account_pass`).
pub fn valid_account_pass(s: &str) -> bool {
    len_and_charset(s, 4, 16, |c| {
        c.is_ascii_alphanumeric() || matches!(c, '_' | '@' | '#' | '%' | '+' | '=' | '!' | '-')
    })
}

/// `^[A-Za-z0-9_]{1,12}$` — character name (`_valid_charname`).
pub fn valid_charname(s: &str) -> bool {
    len_and_charset(s, 1, 12, |c| c.is_ascii_alphanumeric() || c == '_')
}

/// `^[0-9]+:[0-9]+$` — mail item spec `itemid:count` (`_valid_item_spec`).
pub fn valid_item_spec(s: &str) -> bool {
    match s.split_once(':') {
        Some((id, count)) => {
            !id.is_empty()
                && !count.is_empty()
                && id.chars().all(|c| c.is_ascii_digit())
                && count.chars().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

/// `^[A-Za-z0-9_-]+$` — teleport `--to` location token (the `wow teleport` arm).
pub fn valid_tele_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Length-bounded (inclusive, char count) ASCII charset check shared by the
/// validators above.
fn len_and_charset(s: &str, min: usize, max: usize, allowed: impl Fn(char) -> bool) -> bool {
    let len = s.chars().count();
    len >= min && len <= max && s.chars().all(allowed)
}

/// Map-id counterpart of `wow teleport-coords`'s `^[0-9]{1,3}$` gate
/// (`90-main.sh:1916`). The native command receives an already-parsed `u32`
/// (not a raw CLI string), so "1-3 digits" reduces to the numeric range it
/// actually encodes: `0..=999`.
pub fn valid_map_id(map: u32) -> bool {
    map <= 999
}

/// Coordinate counterpart of `_valid_coord`'s magnitude cap
/// (`90-main.sh:123-126`): `|v| <= 20000` (bash's `awk ... exit (v>20000)`
/// rejects only STRICTLY greater, so exactly 20000 is allowed). NaN/infinite
/// are rejected outright. The native command receives an already-parsed
/// `f64`, so `_valid_coord`'s digit-COUNT half of the regex
/// (`^-?[0-9]{1,5}(\.[0-9]+)?$`) is moot: any finite value within this
/// magnitude cap already has at most 5 integer digits.
pub fn valid_coord(v: f64) -> bool {
    v.is_finite() && v.abs() <= 20000.0
}

// ---------------------------------------------------------------------
// Local error helper.
// ---------------------------------------------------------------------

fn bad_arg(message: impl Into<String>, hint: impl Into<String>) -> CmdError {
    CmdError {
        code: "BAD_ARG".into(),
        message: message.into(),
        hint: hint.into(),
    }
}

// ---------------------------------------------------------------------
// Command-string builders — oracle `90-main.sh:1952-2010` (account).
// ---------------------------------------------------------------------

fn validate_account_user(user: &str) -> Result<(), CmdError> {
    if !valid_account_user(user) {
        return Err(bad_arg("Invalid username (3-20 letters/digits/_)", ""));
    }
    Ok(())
}

fn validate_account_pass(pass: &str) -> Result<(), CmdError> {
    if !valid_account_pass(pass) {
        return Err(bad_arg(
            "Invalid password (4-16 chars, letters/digits/_@#%+=!-)",
            "",
        ));
    }
    Ok(())
}

pub fn account_create_cmd(user: &str, pass: &str) -> Result<String, CmdError> {
    validate_account_user(user)?;
    validate_account_pass(pass)?;
    Ok(format!("account create {user} {pass}"))
}

pub fn account_set_password_cmd(user: &str, pass: &str) -> Result<String, CmdError> {
    validate_account_user(user)?;
    validate_account_pass(pass)?;
    Ok(format!("account set password {user} {pass} {pass}"))
}

pub fn account_set_gm_cmd(user: &str, level: &str) -> Result<String, CmdError> {
    validate_account_user(user)?;
    if !matches!(level, "0" | "1" | "2" | "3") {
        return Err(bad_arg("--level must be 0-3", ""));
    }
    Ok(format!("account set gmlevel {user} {level} -1"))
}

pub fn account_delete_cmd(user: &str) -> Result<String, CmdError> {
    validate_account_user(user)?;
    if user.to_lowercase() == "admin" {
        return Err(bad_arg(
            "Refusing to delete the admin account",
            "The launcher uses it for server access (SOAP).",
        ));
    }
    Ok(format!("account delete {user}"))
}

// ---------------------------------------------------------------------
// GM (SOAP-direct + bridge commands) — oracle gm arm `90-main.sh:3490-3655`.
// ---------------------------------------------------------------------

fn validate_player(player: &str) -> Result<(), CmdError> {
    if !valid_charname(player) {
        return Err(bad_arg(format!("Invalid player name: {player}"), ""));
    }
    Ok(())
}

pub fn gm_level_cmd(player: &str, level: i32) -> Result<String, CmdError> {
    validate_player(player)?;
    if !(1..=255).contains(&level) {
        return Err(bad_arg(
            format!("Invalid level: {level}"),
            "Use 1-255 (your server's own max level still applies).",
        ));
    }
    Ok(format!(".character level {player} {level}"))
}

pub fn gm_at_login_cmd(player: &str, flag: &str) -> Result<String, CmdError> {
    validate_player(player)?;
    if !matches!(flag, "rename" | "customize" | "changerace" | "changefaction") {
        return Err(bad_arg(
            format!("Invalid flag: {flag}"),
            "One of: rename customize changerace changefaction",
        ));
    }
    Ok(format!("character {flag} {player}"))
}

pub fn gm_gold_cmd(player: &str, gold: i32) -> Result<String, CmdError> {
    validate_player(player)?;
    if !(0..=214748).contains(&gold) {
        return Err(bad_arg(
            format!("Invalid gold amount: {gold}"),
            "Whole gold, 0-214748 (the WotLK money cap).",
        ));
    }
    let copper = gold * 10000;
    Ok(format!("dml_gm_money {player} {copper}"))
}

pub fn gm_heal_cmd(player: &str) -> Result<String, CmdError> {
    validate_player(player)?;
    Ok(format!("dml_gm_health {player} 100"))
}

pub fn gm_revive_cmd(player: &str) -> Result<String, CmdError> {
    validate_player(player)?;
    Ok(format!("dml_gm_revive {player}"))
}

pub fn gm_summon_cmd(player: &str, entry: i32) -> Result<String, CmdError> {
    validate_player(player)?;
    if !(1..=999999).contains(&entry) {
        return Err(bad_arg(
            format!("Invalid creature entry: {entry}"),
            "Creature entry id, 1-999999.",
        ));
    }
    Ok(format!("dml_summon_npc {player} {entry}"))
}

// ---------------------------------------------------------------------
// Mail — oracle `90-main.sh:1785-1833`.
// ---------------------------------------------------------------------

pub fn mail_items_cmd(
    to: &str,
    item_specs: &[&str],
    subject: &str,
    body: &str,
) -> Result<String, CmdError> {
    if !valid_charname(to) {
        return Err(bad_arg(
            format!("Invalid character name: {to}"),
            "1-12 letters/digits/underscore.",
        ));
    }
    if item_specs.is_empty() || item_specs.len() > 12 {
        return Err(bad_arg("Provide 1-12 items as id:count[,id:count…]", ""));
    }
    let mut attach = String::new();
    for spec in item_specs.iter().copied() {
        if !valid_item_spec(spec) {
            return Err(bad_arg(
                format!("Malformed item spec: {spec}"),
                "Use itemid:count",
            ));
        }
        attach.push(' ');
        attach.push_str(spec);
    }
    let subject = sanitize_mail_text(subject);
    let body = sanitize_mail_text(body);
    Ok(format!("send items {to} \"{subject}\" \"{body}\"{attach}"))
}

/// Strip `"` and replace CR/LF each with a single space — matching bash's
/// `${var//\"/}` / `${var//$'\n'/ }` / `${var//$'\r'/ }` chain EXACTLY
/// (replace, not delete, so words don't glue together). Closes the AC #2695
/// newline-injection crash surface for `.send items`.
fn sanitize_mail_text(s: &str) -> String {
    s.chars()
        .filter(|&c| c != '"')
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .collect()
}

// ---------------------------------------------------------------------
// Teleport — oracle `90-main.sh:1852-1893`.
// ---------------------------------------------------------------------

pub fn teleport_name_cmd(char_name: &str, to: &str) -> Result<String, CmdError> {
    if !valid_charname(char_name) {
        return Err(bad_arg(format!("Invalid character name: {char_name}"), ""));
    }
    if to.is_empty() {
        return Err(bad_arg(
            "Missing --to <location>",
            "List with: dml wow teleport-list --json",
        ));
    }
    if !valid_tele_name(to) {
        return Err(bad_arg(
            format!("Invalid location name: {to}"),
            "Single token, letters/digits/_/- only; list names with: dml wow teleport-list --json",
        ));
    }
    Ok(format!("teleport name {char_name} {to}"))
}

// ---------------------------------------------------------------------
// MOTD / announce — oracle `90-main.sh:2466-2480`, the `server.motd`
// special case in the config-write arm. That arm applies its own per-type
// (`text`) handling — strip `"`, replace CR/LF with a space — to `$value`
// BEFORE calling into this builder; this function does no validation or
// sanitization of its own, matching the config arm which does none either
// at the point it builds this exact command string.
// ---------------------------------------------------------------------

pub fn motd_cmd(text: &str) -> String {
    format!("server set motd 1 enUS {text}")
}

// ---------------------------------------------------------------------
// `soap_text_decode` — oracle `cli/src/45-console.sh:18-27`.
// ---------------------------------------------------------------------

pub fn soap_text_decode(s: &str) -> String {
    let s = s.replace("&#xD;", "");
    let s = s.replace("&lt;", "<");
    let s = s.replace("&gt;", ">");
    let s = s.replace("&quot;", "\"");
    s.replace("&amp;", "&")
}

// ---------------------------------------------------------------------
// `SoapOutcome -> CmdError` mappers.
// ---------------------------------------------------------------------

fn soap_auth_err() -> CmdError {
    CmdError {
        code: "SOAP_AUTH".into(),
        message: "SOAP authentication failed".into(),
        hint: "Check ~/.dml/soap.env".into(),
    }
}

fn soap_unreachable_err() -> CmdError {
    CmdError {
        code: "SOAP_UNREACHABLE".into(),
        message: "Could not reach the server".into(),
        hint: "Is it running?".into(),
    }
}

/// Generic path (console-send / soap-exec): `Ok` -> raw result string;
/// `Fault` -> `SOAP_FAULT` with RAW (undecoded) text.
pub fn outcome_to_result_raw(o: SoapOutcome) -> Result<String, CmdError> {
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: t,
            hint: "The worldserver rejected the command.".into(),
        }),
        SoapOutcome::Auth => Err(soap_auth_err()),
        SoapOutcome::Unreachable(_) => Err(soap_unreachable_err()),
    }
}

/// Typed path (account/gm/teleport/return-home): `Fault` text is
/// `soap_text_decode()`'d before surfacing.
pub fn outcome_to_result_decoded(o: SoapOutcome) -> Result<String, CmdError> {
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: soap_text_decode(&t),
            hint: "The worldserver rejected the command.".into(),
        }),
        SoapOutcome::Auth => Err(soap_auth_err()),
        SoapOutcome::Unreachable(_) => Err(soap_unreachable_err()),
    }
}

/// `SoapOutcome -> CmdError` for `server.motd` (`90-main.sh:2475-2481`, Task
/// B2a): RAW fault text with its own hint; a different (unstarted-server)
/// unreachable hint than the generic mappers — this is the one arm where
/// SOAP failure means "start the server first" rather than "is it running?".
pub fn motd_result(o: crate::soap::SoapOutcome) -> Result<(), CmdError> {
    use crate::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(_) => Ok(()),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: t,
            hint: "The server rejected the motd command.".into(),
        }),
        SoapOutcome::Auth => Err(CmdError {
            code: "SOAP_AUTH".into(),
            message: "SOAP authentication failed".into(),
            hint: "Check ~/.dml/soap.env".into(),
        }),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: "Could not reach the server".into(),
            hint: "The server must be running to change the message of the day - start it first."
                .into(),
        }),
    }
}

/// `SoapOutcome -> CmdError` matching `_party_fire`'s exact case block
/// (`cli/src/50-party.sh:67-79`) -- used by the bridge-backed gm ops
/// (gold/heal/revive/summon), which all fire through `_party_fire`. `label`
/// is the short noun `_party_fire`'s caller passes as its `$2` (e.g. "gold",
/// "heal", "revive", "summon"), spliced into the fixed fault message. Unlike
/// the generic mappers, the SOAP_FAULT text here is NEVER the server's own
/// fault string -- bash's `_party_fire` discards `$out` entirely on rc=2.
pub fn party_fire_result(o: crate::soap::SoapOutcome, label: &str) -> Result<String, CmdError> {
    use crate::soap::SoapOutcome;
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(_) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: format!("The {label} command was rejected"),
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

// ---------------------------------------------------------------------
// PER-ARM `SoapOutcome -> CmdError` mappers (Task 13).
//
// The oracle does NOT use one mapper for every write arm: each `90-main.sh`
// arm spells its own fault/auth/unreachable strings, and the launcher
// reproduced that arm-by-arm in `launcher/src-tauri/src/lib.rs` as a set of
// PRIVATE `fn *_result` helpers (`console_send_result`, `account_result`,
// `gm_level_result`, `gm_at_login_result`, `mail_result`, `teleport_result`).
// The standalone CLI needs the SAME strings for the SAME subcommands, and the
// Task 12 review rule is explicit: never hand-copy an error mapping into a
// second call site where it can drift. So the mappers move HERE, next to
// `motd_result`/`party_fire_result` (the two the Task 9 hoist already brought
// across for the same reason), and both front ends can share one definition.
//
// NOTE for whoever next touches the launcher: `lib.rs` still carries its own
// private copies (this task was not allowed to modify the launcher). They are
// byte-identical to these today — pinned by the unit tests below AND by the
// launcher's own tests over its copies — and the launcher's should simply be
// deleted in favour of these.
// ---------------------------------------------------------------------

/// `console-send` (`90-main.sh:1736-1746`) — the arm the Console tab actually
/// fires, NOT `soap-exec`. Unique in TWO ways: it entity-decodes the OK result
/// as well as the fault text, and its unreachable branch names the endpoint
/// (`soap_url`) and points at `soap-setup`.
pub fn console_send_result(o: SoapOutcome, soap_url: &str) -> Result<String, CmdError> {
    match o {
        SoapOutcome::Ok(t) => Ok(soap_text_decode(&t)),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: soap_text_decode(&t),
            hint: "The worldserver rejected the command.".into(),
        }),
        SoapOutcome::Auth => Err(soap_auth_err()),
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: format!("Could not reach SOAP at {soap_url}"),
            hint: "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup".into(),
        }),
    }
}

/// The `account` arm's catch-all case block (`90-main.sh:1999-2010`), shared
/// by create/set-password/set-gm/delete. Ok/Fault/Auth are
/// [`outcome_to_result_decoded`] byte-for-byte; only Unreachable differs
/// (endpoint named, `Is the worldserver running?`).
pub fn account_result(o: SoapOutcome, soap_url: &str) -> Result<String, CmdError> {
    match o {
        SoapOutcome::Unreachable(_) => Err(CmdError {
            code: "SOAP_UNREACHABLE".into(),
            message: format!("Could not reach SOAP at {soap_url}"),
            hint: "Is the worldserver running?".into(),
        }),
        other => outcome_to_result_decoded(other),
    }
}

/// `gm level` (`90-main.sh:3509-3516`). The fault message is FIXED — bash
/// discards the server's own `$out` on rc=2 — and the auth message is the
/// shorter "SOAP auth failed" this arm family uses (not a typo; see
/// [`gm_at_login_result`] and [`party_fire_result`], which agree).
pub fn gm_level_result(o: SoapOutcome) -> Result<String, CmdError> {
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(_) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: "The level command was rejected".into(),
            hint: "Does the character exist? The server said no.".into(),
        }),
        SoapOutcome::Auth => Err(short_soap_auth_err()),
        SoapOutcome::Unreachable(_) => Err(soap_unreachable_err()),
    }
}

/// `gm at-login` (`90-main.sh:3595-3601`). Unlike [`gm_level_result`] the
/// server's own fault text DOES surface here (entity-decoded), but the auth
/// message is still the short one.
pub fn gm_at_login_result(o: SoapOutcome) -> Result<String, CmdError> {
    match o {
        SoapOutcome::Ok(t) => Ok(t),
        SoapOutcome::Fault(t) => Err(CmdError {
            code: "SOAP_FAULT".into(),
            message: soap_text_decode(&t),
            hint: "The worldserver rejected the command.".into(),
        }),
        SoapOutcome::Auth => Err(short_soap_auth_err()),
        SoapOutcome::Unreachable(_) => Err(soap_unreachable_err()),
    }
}

/// `mail-item` (`90-main.sh:1828-1833`): RAW (undecoded) fault text, its own
/// fault hint, an EMPTY auth hint, and an unreachable hint that walks the user
/// through `soap-setup`.
pub fn mail_result(o: SoapOutcome) -> Result<String, CmdError> {
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

/// `teleport` (`90-main.sh:1888-1893`): RAW fault text with a location-shaped
/// hint; empty hints on both auth and unreachable.
pub fn teleport_result(o: SoapOutcome) -> Result<String, CmdError> {
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

/// The SHORTER auth wording used by the `gm`/`party` bridge arm family
/// ("SOAP auth failed"), as opposed to [`soap_auth_err`]'s "SOAP
/// authentication failed". Both spellings are real and arm-specific — see
/// `party_fire_result`, which has carried the short one since the Task 9
/// hoist.
fn short_soap_auth_err() -> CmdError {
    CmdError {
        code: "SOAP_AUTH".into(),
        message: "SOAP auth failed".into(),
        hint: "Check ~/.dml/soap.env".into(),
    }
}

/// Split a mail `--items` CSV the way bash's `IFS=',' read -ra specs <<<
/// "$items"` does: an EMPTY string splits to ZERO fields, unlike Rust's
/// `"".split(',')` which yields one empty-string field — that mismatch would
/// turn an empty items argument into a "Malformed item spec: " BAD_ARG instead
/// of the oracle's "Provide 1-12 items…" one. Any other input (including
/// doubled/trailing commas, which bash also turns into empty fields) splits
/// exactly like `str::split`.
pub fn split_mail_items(items: &str) -> Vec<&str> {
    if items.is_empty() {
        Vec::new()
    } else {
        items.split(',').collect()
    }
}

/// `NOT_FOUND` for an offline character, matching `_gm_require_online`
/// (`cli/src/55-gm.sh:9-14`) exactly.
pub fn not_online_err(player: &str) -> CmdError {
    CmdError {
        code: "NOT_FOUND".into(),
        message: format!("Character not online: {player}"),
        hint: "This action needs the character logged in. (Set level works offline.)".into(),
    }
}

/// Whether `player` is currently online -- a native-mode port of
/// `_gm_require_online`/`_party_online_guid` (`cli/src/55-gm.sh:9-14`,
/// `cli/src/50-party.sh:46-49`): a `characters` row with `online=1`. Any
/// query failure reads as "not online", matching bash: `_party_online_guid`
/// redirects `db_chars_query`'s stderr to `/dev/null` and always `return`s 0,
/// so a DB error there surfaces as an empty guid (== not online) rather than
/// a separate DB_UNREACHABLE branch -- this mirrors that swallow rather than
/// inventing a new error path the oracle doesn't have.
pub fn char_is_online(cfg: &crate::db::DbConfig, player: &str) -> bool {
    // Canonical stored form -- characters.name is utf8mb4_bin, see
    // db::canon_char_name. Without this, `gm gold testa` answered NOT_ONLINE
    // for an online `Testa` before SOAP was ever consulted.
    let params: Vec<mysql::Value> = vec![mysql::Value::from(crate::db::canon_char_name(player))];
    crate::db::query_with_params(
        cfg,
        crate::db::Database::Characters,
        "SELECT guid FROM characters WHERE name=? AND online=1 LIMIT 1",
        params,
    )
    .map(|res| !res.rows.is_empty())
    .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// NATIVE-MODE `dml_summon_npc` bridge command (`90-main.sh:3554-3577`) —
// moved out of the launcher's `lib.rs` by the cargo-workspace refactor
// (Task 9b). Order matches the oracle exactly: creature_template
// existence+name lookup (World DB) -> online check (Characters DB) -> SOAP
// fire -> success with the looked-up NPC name. `cmd` is the already-built
// (and therefore already-validated) SOAP command string from
// `gm_summon_cmd`; `lock` serializes the fire against every other SOAP call.
// ---------------------------------------------------------------------------

pub fn gm_summon(
    player: String,
    entry: i32,
    cmd: String,
    lock: Arc<Mutex<()>>,
) -> Result<serde_json::Value, CmdError> {
    let cfg = crate::db::DbConfig::from_env();
    let params: Vec<mysql::Value> = vec![mysql::Value::from(entry)];
    let npc_res = crate::db::query_with_params(
        &cfg,
        crate::db::Database::World,
        "SELECT name FROM creature_template WHERE entry=? LIMIT 1",
        params,
    )
    .map_err(|_e| CmdError {
        code: "DB_UNREACHABLE".into(),
        message: "Could not check the creature entry".into(),
        hint: "Is ac-database running?".into(),
    })?;
    let npc_name: Option<String> =
        npc_res.rows.first().and_then(|r| r.first()).and_then(|v| match v {
            crate::db::SqlValue::Text(s) => Some(s.clone()),
            crate::db::SqlValue::Int(i) => Some(i.to_string()),
            crate::db::SqlValue::Null => None,
        });
    let npc_name = match npc_name {
        Some(n) if !n.is_empty() => n,
        _ => {
            return Err(CmdError {
                code: "NOT_FOUND".into(),
                message: format!("No creature with entry {entry}"),
                hint: "Check the id (creature_template.entry).".into(),
            })
        }
    };
    if !char_is_online(&cfg, &player) {
        return Err(not_online_err(&player));
    }
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    let soap_cfg = crate::soap::SoapConfig::load();
    let outcome = crate::soap::exec(&soap_cfg, &cmd);
    party_fire_result(outcome, "summon")?;
    Ok(serde_json::json!({
        "summoned": true,
        "player": player,
        "entry": entry,
        "npc": npc_name,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- valid_account_user -------------------------------------------------

    #[test]
    fn valid_account_user_boundaries() {
        assert!(!valid_account_user("ab")); // too short (2)
        assert!(valid_account_user("abc")); // min ok (3)
        assert!(valid_account_user(&"a".repeat(20))); // max ok (20)
        assert!(!valid_account_user(&"a".repeat(21))); // too long (21)
        assert!(!valid_account_user("bad name")); // space not allowed
        assert!(valid_account_user("bob_the_2nd"));
    }

    // -- valid_account_pass -------------------------------------------------

    #[test]
    fn valid_account_pass_boundaries() {
        assert!(!valid_account_pass("pw1")); // too short (3)
        assert!(valid_account_pass("pw12")); // min ok (4)
        assert!(valid_account_pass(&"a".repeat(16))); // max ok (16)
        assert!(!valid_account_pass(&"a".repeat(17))); // too long (17)
        assert!(valid_account_pass("p@#%+=!-_9"));
        assert!(!valid_account_pass("bad pass")); // space not allowed
        assert!(!valid_account_pass("bad$pass")); // $ not in allowlist
    }

    // -- valid_charname -------------------------------------------------

    #[test]
    fn valid_charname_boundaries() {
        assert!(!valid_charname("")); // too short (0)
        assert!(valid_charname("A")); // min ok (1)
        assert!(valid_charname(&"A".repeat(12))); // max ok (12)
        assert!(!valid_charname(&"A".repeat(13))); // too long (13)
        assert!(!valid_charname("bad name"));
        assert!(valid_charname("Testen_1"));
    }

    // -- valid_item_spec -------------------------------------------------

    #[test]
    fn valid_item_spec_shape() {
        assert!(valid_item_spec("6948:1"));
        assert!(!valid_item_spec("6948"));
        assert!(!valid_item_spec("6948:"));
        assert!(!valid_item_spec(":1"));
        assert!(!valid_item_spec("a:1"));
        assert!(!valid_item_spec("6948:1:2"));
    }

    // -- valid_tele_name -------------------------------------------------

    #[test]
    fn valid_tele_name_shape() {
        assert!(valid_tele_name("Orgrimmar"));
        assert!(valid_tele_name("Deeprun_Tram-1"));
        assert!(!valid_tele_name("")); // + quantifier requires >=1
        assert!(!valid_tele_name("bad name")); // space not allowed
    }

    // -- valid_map_id / valid_coord (teleport-coords, Part 5a) ------------

    #[test]
    fn valid_map_id_boundaries() {
        assert!(valid_map_id(0));
        assert!(valid_map_id(1));
        assert!(valid_map_id(999));
        assert!(!valid_map_id(1000));
        assert!(!valid_map_id(u32::MAX));
    }

    #[test]
    fn valid_coord_magnitude_cap_is_inclusive() {
        assert!(valid_coord(0.0));
        assert!(valid_coord(-4421.94));
        assert!(valid_coord(20000.0));
        assert!(valid_coord(-20000.0));
        assert!(!valid_coord(20000.1));
        assert!(!valid_coord(-20000.1));
        assert!(!valid_coord(f64::NAN));
        assert!(!valid_coord(f64::INFINITY));
        assert!(!valid_coord(f64::NEG_INFINITY));
    }

    // -- account_create_cmd / account_set_password_cmd ----------------------

    #[test]
    fn account_create_cmd_happy_path() {
        assert_eq!(
            account_create_cmd("bob", "pw12").unwrap(),
            "account create bob pw12"
        );
    }

    #[test]
    fn account_create_cmd_invalid_user() {
        let e = account_create_cmd("ab", "pw12").unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Invalid username (3-20 letters/digits/_)");
    }

    #[test]
    fn account_create_cmd_invalid_pass() {
        let e = account_create_cmd("bob", "x").unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(
            e.message,
            "Invalid password (4-16 chars, letters/digits/_@#%+=!-)"
        );
    }

    #[test]
    fn account_set_password_cmd_repeats_pass_twice() {
        assert_eq!(
            account_set_password_cmd("bob", "pw12").unwrap(),
            "account set password bob pw12 pw12"
        );
    }

    // -- account_set_gm_cmd -------------------------------------------------

    #[test]
    fn account_set_gm_cmd_happy_path() {
        assert_eq!(
            account_set_gm_cmd("bob", "3").unwrap(),
            "account set gmlevel bob 3 -1"
        );
    }

    #[test]
    fn account_set_gm_cmd_bad_level() {
        let e = account_set_gm_cmd("bob", "4").unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "--level must be 0-3");
    }

    // -- account_delete_cmd -------------------------------------------------

    #[test]
    fn account_delete_cmd_happy_path() {
        assert_eq!(account_delete_cmd("bob").unwrap(), "account delete bob");
    }

    #[test]
    fn account_delete_cmd_refuses_admin_case_insensitive() {
        for u in ["admin", "Admin", "ADMIN"] {
            let e = account_delete_cmd(u).unwrap_err();
            assert_eq!(e.code, "BAD_ARG");
            assert_eq!(e.message, "Refusing to delete the admin account");
            assert_eq!(e.hint, "The launcher uses it for server access (SOAP).");
        }
    }

    // -- gm_level_cmd -------------------------------------------------

    #[test]
    fn gm_level_cmd_happy_path() {
        assert_eq!(
            gm_level_cmd("Testen", 80).unwrap(),
            ".character level Testen 80"
        );
    }

    #[test]
    fn gm_level_cmd_rejects_below_min_and_above_max() {
        assert_eq!(gm_level_cmd("Testen", 0).unwrap_err().code, "BAD_ARG");
        assert_eq!(gm_level_cmd("Testen", 256).unwrap_err().code, "BAD_ARG");
    }

    #[test]
    fn gm_level_cmd_invalid_player() {
        let e = gm_level_cmd("bad name", 10).unwrap_err();
        assert_eq!(e.message, "Invalid player name: bad name");
    }

    // -- gm_at_login_cmd -------------------------------------------------

    #[test]
    fn gm_at_login_cmd_happy_path() {
        assert_eq!(
            gm_at_login_cmd("Testen", "rename").unwrap(),
            "character rename Testen"
        );
    }

    #[test]
    fn gm_at_login_cmd_rejects_unknown_flag() {
        let e = gm_at_login_cmd("Testen", "resurrect").unwrap_err();
        assert_eq!(e.message, "Invalid flag: resurrect");
    }

    // -- gm_gold_cmd -------------------------------------------------

    #[test]
    fn gm_gold_cmd_converts_gold_to_copper() {
        assert_eq!(
            gm_gold_cmd("Testen", 5).unwrap(),
            "dml_gm_money Testen 50000"
        );
    }

    #[test]
    fn gm_gold_cmd_rejects_over_cap() {
        let e = gm_gold_cmd("Testen", 214749).unwrap_err();
        assert_eq!(e.message, "Invalid gold amount: 214749");
    }

    #[test]
    fn gm_gold_cmd_allows_zero() {
        assert_eq!(gm_gold_cmd("Testen", 0).unwrap(), "dml_gm_money Testen 0");
    }

    // -- gm_heal_cmd / gm_revive_cmd -------------------------------------------------

    #[test]
    fn gm_heal_cmd_happy_path() {
        assert_eq!(gm_heal_cmd("Testen").unwrap(), "dml_gm_health Testen 100");
    }

    #[test]
    fn gm_revive_cmd_happy_path() {
        assert_eq!(gm_revive_cmd("Testen").unwrap(), "dml_gm_revive Testen");
    }

    // -- gm_summon_cmd -------------------------------------------------

    #[test]
    fn gm_summon_cmd_happy_path() {
        assert_eq!(
            gm_summon_cmd("Testen", 190).unwrap(),
            "dml_summon_npc Testen 190"
        );
    }

    #[test]
    fn gm_summon_cmd_rejects_out_of_range_entry() {
        assert_eq!(gm_summon_cmd("Testen", 0).unwrap_err().code, "BAD_ARG");
        assert_eq!(
            gm_summon_cmd("Testen", 1_000_000).unwrap_err().code,
            "BAD_ARG"
        );
    }

    // -- mail_items_cmd -------------------------------------------------

    #[test]
    fn mail_items_cmd_happy_path() {
        assert_eq!(
            mail_items_cmd("Testen", &["6948:1", "2589:5"], "hi", "bye").unwrap(),
            r#"send items Testen "hi" "bye" 6948:1 2589:5"#
        );
    }

    #[test]
    fn mail_items_cmd_sanitizes_subject_and_body() {
        let cmd = mail_items_cmd("Testen", &["6948:1"], "hi\"x", "a\nb").unwrap();
        assert!(cmd.contains("\"hix\""));
        assert!(cmd.contains("\"a b\""));
    }

    #[test]
    fn mail_items_cmd_sanitizes_cr() {
        let cmd = mail_items_cmd("Testen", &["6948:1"], "s", "a\rb").unwrap();
        assert!(cmd.contains("\"a b\""));
    }

    #[test]
    fn mail_items_cmd_rejects_zero_items() {
        let e = mail_items_cmd("Testen", &[], "s", "b").unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Provide 1-12 items as id:count[,id:count…]");
    }

    #[test]
    fn mail_items_cmd_rejects_thirteen_items() {
        let specs: Vec<&str> = vec!["1:1"; 13];
        let e = mail_items_cmd("Testen", &specs, "s", "b").unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
    }

    #[test]
    fn mail_items_cmd_allows_twelve_items() {
        let specs: Vec<&str> = vec!["1:1"; 12];
        assert!(mail_items_cmd("Testen", &specs, "s", "b").is_ok());
    }

    #[test]
    fn mail_items_cmd_rejects_invalid_charname() {
        let e = mail_items_cmd("bad name", &["1:1"], "s", "b").unwrap_err();
        assert_eq!(e.message, "Invalid character name: bad name");
        assert_eq!(e.hint, "1-12 letters/digits/underscore.");
    }

    #[test]
    fn mail_items_cmd_rejects_malformed_spec() {
        let e = mail_items_cmd("Testen", &["nope"], "s", "b").unwrap_err();
        assert_eq!(e.message, "Malformed item spec: nope");
        assert_eq!(e.hint, "Use itemid:count");
    }

    // -- teleport_name_cmd -------------------------------------------------

    #[test]
    fn teleport_name_cmd_happy_path() {
        assert_eq!(
            teleport_name_cmd("Testen", "Orgrimmar").unwrap(),
            "teleport name Testen Orgrimmar"
        );
    }

    #[test]
    fn teleport_name_cmd_rejects_bad_location() {
        let e = teleport_name_cmd("Testen", "bad name").unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Invalid location name: bad name");
    }

    #[test]
    fn teleport_name_cmd_rejects_missing_to() {
        let e = teleport_name_cmd("Testen", "").unwrap_err();
        assert_eq!(e.message, "Missing --to <location>");
    }

    #[test]
    fn teleport_name_cmd_rejects_bad_charname() {
        let e = teleport_name_cmd("bad name", "Orgrimmar").unwrap_err();
        assert_eq!(e.message, "Invalid character name: bad name");
    }

    // -- motd_cmd -------------------------------------------------

    #[test]
    fn motd_cmd_builds_command() {
        assert_eq!(
            motd_cmd("Welcome to the Lab!"),
            "server set motd 1 enUS Welcome to the Lab!"
        );
    }

    // -- soap_text_decode -------------------------------------------------

    #[test]
    fn soap_text_decode_decodes_all_entities() {
        assert_eq!(soap_text_decode("a&lt;b&amp;c&#xD;"), "a<b&c");
    }

    #[test]
    fn soap_text_decode_amp_last_prevents_double_decode() {
        // &amp; must be decoded LAST, so "&amp;lt;" becomes "&lt;", not "<".
        assert_eq!(soap_text_decode("&amp;lt;"), "&lt;");
    }

    #[test]
    fn soap_text_decode_quot_and_gt() {
        assert_eq!(soap_text_decode("&quot;hi&quot;&gt;"), "\"hi\">");
    }

    // -- outcome_to_result_raw / outcome_to_result_decoded ---------------------

    #[test]
    fn outcome_to_result_raw_ok_passes_through() {
        assert_eq!(
            outcome_to_result_raw(SoapOutcome::Ok("Account created.".into())).unwrap(),
            "Account created."
        );
    }

    #[test]
    fn outcome_to_result_raw_fault_is_undecoded() {
        let e = outcome_to_result_raw(SoapOutcome::Fault("x".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "x");
        assert_eq!(e.hint, "The worldserver rejected the command.");
    }

    #[test]
    fn outcome_to_result_raw_fault_does_not_decode_entities() {
        let e = outcome_to_result_raw(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.message, "a&lt;b");
    }

    #[test]
    fn outcome_to_result_decoded_fault_is_decoded() {
        let e = outcome_to_result_decoded(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.message, "a<b");
    }

    #[test]
    fn outcome_to_result_raw_auth_is_soap_auth() {
        let e = outcome_to_result_raw(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");
    }

    #[test]
    fn outcome_to_result_raw_unreachable_is_soap_unreachable() {
        let e = outcome_to_result_raw(SoapOutcome::Unreachable("boom".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(e.hint, "Is it running?");
    }

    #[test]
    fn outcome_to_result_decoded_auth_and_unreachable_match_raw() {
        let e = outcome_to_result_decoded(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        let e = outcome_to_result_decoded(SoapOutcome::Unreachable("boom".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
    }

    // -- Task 13 per-arm mappers -------------------------------------------
    //
    // Every assertion below is the EXACT code/message/hint the launcher's
    // matching private `fn *_result` produces (`launcher/src-tauri/src/lib.rs`
    // 3242-3153). These are the pin that makes the two definitions provably
    // the same text while both exist.

    #[test]
    fn console_send_result_decodes_ok_as_well_as_fault() {
        assert_eq!(
            console_send_result(SoapOutcome::Ok("a&lt;b".into()), "http://x/").unwrap(),
            "a<b"
        );
        let e = console_send_result(SoapOutcome::Fault("a&lt;b".into()), "http://x/").unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "a<b");
        assert_eq!(e.hint, "The worldserver rejected the command.");
    }

    #[test]
    fn console_send_result_auth_and_unreachable_name_the_endpoint() {
        let e = console_send_result(SoapOutcome::Auth, "http://x/").unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");

        let e = console_send_result(SoapOutcome::Unreachable("boom".into()), "http://127.0.0.1:7878/")
            .unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach SOAP at http://127.0.0.1:7878/");
        assert_eq!(e.hint, "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup");
    }

    #[test]
    fn account_result_matches_decoded_except_unreachable() {
        // Ok/Fault/Auth delegate to `outcome_to_result_decoded` verbatim.
        assert_eq!(account_result(SoapOutcome::Ok("done".into()), "http://x/").unwrap(), "done");
        let e = account_result(SoapOutcome::Fault("a&lt;b".into()), "http://x/").unwrap_err();
        assert_eq!(e.message, "a<b");
        let e = account_result(SoapOutcome::Auth, "http://x/").unwrap_err();
        assert_eq!(e.message, "SOAP authentication failed");
        // ...only Unreachable is this arm's own wording.
        let e = account_result(SoapOutcome::Unreachable("x".into()), "http://127.0.0.1:7878/")
            .unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach SOAP at http://127.0.0.1:7878/");
        assert_eq!(e.hint, "Is the worldserver running?");
    }

    #[test]
    fn gm_level_result_fault_is_fixed_and_auth_is_the_short_wording() {
        assert_eq!(gm_level_result(SoapOutcome::Ok("ok".into())).unwrap(), "ok");
        let e = gm_level_result(SoapOutcome::Fault("the server said something".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        assert_eq!(e.message, "The level command was rejected");
        assert_eq!(e.hint, "Does the character exist? The server said no.");
        let e = gm_level_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.message, "SOAP auth failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");
        let e = gm_level_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(e.hint, "Is it running?");
    }

    #[test]
    fn gm_at_login_result_decodes_the_server_fault_but_keeps_short_auth() {
        let e = gm_at_login_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.message, "a<b");
        assert_eq!(e.hint, "The worldserver rejected the command.");
        assert_eq!(gm_at_login_result(SoapOutcome::Auth).unwrap_err().message, "SOAP auth failed");
    }

    #[test]
    fn mail_result_fault_is_raw_with_empty_auth_hint() {
        let e = mail_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.message, "a&lt;b", "mail faults are NOT entity-decoded");
        assert_eq!(e.hint, "The server rejected the mail command.");
        let e = mail_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "");
        let e = mail_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.hint, "Run: dml wow soap-setup, then start the server.");
    }

    #[test]
    fn teleport_result_fault_is_raw_with_empty_auth_and_unreachable_hints() {
        let e = teleport_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.message, "a&lt;b");
        assert_eq!(e.hint, "Unknown location? See dml wow teleport-list.");
        assert_eq!(teleport_result(SoapOutcome::Auth).unwrap_err().hint, "");
        assert_eq!(teleport_result(SoapOutcome::Unreachable("x".into())).unwrap_err().hint, "");
    }

    // -- split_mail_items -------------------------------------------------

    #[test]
    fn split_mail_items_empty_string_is_zero_fields() {
        assert!(split_mail_items("").is_empty());
    }

    #[test]
    fn split_mail_items_splits_on_commas_and_keeps_empty_inner_fields() {
        assert_eq!(split_mail_items("6948:1,2589:5"), vec!["6948:1", "2589:5"]);
        assert_eq!(split_mail_items("6948:1"), vec!["6948:1"]);
        // Doubled/trailing commas yield empty fields in bash too -- they then
        // fail `valid_item_spec`, which is the oracle's behaviour.
        assert_eq!(split_mail_items("6948:1,,"), vec!["6948:1", "", ""]);
    }

    /// The pair that matters end to end: an empty items string must reach
    /// `mail_items_cmd`'s "Provide 1-12 items…" branch, not its "Malformed
    /// item spec: " one.
    #[test]
    fn split_mail_items_empty_routes_to_the_count_error_not_the_spec_error() {
        let specs = split_mail_items("");
        let e = mail_items_cmd("Testen", &specs, "s", "b").unwrap_err();
        assert_eq!(e.message, "Provide 1-12 items as id:count[,id:count…]");
    }

    #[test]
    fn motd_result_ok_and_error_shapes() {
        assert!(motd_result(SoapOutcome::Ok("ignored".into())).is_ok());

        let e = motd_result(SoapOutcome::Fault("a&lt;b".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_FAULT");
        // RAW, not entity-decoded (matches the oracle's `$out` interpolation).
        assert_eq!(e.message, "a&lt;b");
        assert_eq!(e.hint, "The server rejected the motd command.");

        let e = motd_result(SoapOutcome::Auth).unwrap_err();
        assert_eq!(e.code, "SOAP_AUTH");
        assert_eq!(e.message, "SOAP authentication failed");
        assert_eq!(e.hint, "Check ~/.dml/soap.env");

        let e = motd_result(SoapOutcome::Unreachable("x".into())).unwrap_err();
        assert_eq!(e.code, "SOAP_UNREACHABLE");
        assert_eq!(e.message, "Could not reach the server");
        assert_eq!(
            e.hint,
            "The server must be running to change the message of the day - start it first."
        );
    }
}
