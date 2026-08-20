//! Native-mode **party add/kick/dismiss-all/relogin/botcmd** + **preset
//! save/delete/load/show/import** (Chunk 5, Part 5b). Faithful port of
//! `cli/src/50-party.sh`'s helpers plus the matching sub-arms of the `party)`
//! case in `cli/src/90-main.sh:3067-3483`.
//!
//! ARCHITECTURE. Pure command-string builders, validators, bound-param SQL
//! text, and preset-file (de)serialization come first — no SOAP/DB/Tauri.
//! Below them sit the live DB read helpers that bind those SQL texts
//! ([`party_online_guid`], [`group_member_guids`], [`char_name_by_guid`],
//! [`bot_member_names`], [`bot_member_classes`], [`wait_new_member`]) and the
//! one STREAMED orchestration, [`party_preset_load_stream`]. That live half
//! moved out of the launcher's `lib.rs` in the cargo-workspace refactor
//! (Task 9) so the standalone CLI can drive it too; the SOAP serialization
//! mutex arrives as a plain `Arc<Mutex<()>>` parameter. `party online`/`party
//! specs`/`party list` are OUT of scope here — they were ported earlier
//! (task D1a: `pages::read_party_online`, `party_specs`) and this module's
//! own `dismiss-all`/`preset-save`/`preset-load` reuse their bot-membership
//! SQL shape, not their code.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use dml_core::error::CmdError;

use super::db::{cell_string, db_unreachable_err, sql_row_int};
use super::soap_cmds::{party_fire_result, valid_charname};

fn bad_arg(message: impl Into<String>, hint: impl Into<String>) -> CmdError {
    CmdError { code: "BAD_ARG".into(), message: message.into(), hint: hint.into() }
}

// ---------------------------------------------------------------------
// Validators — `_valid_preset_name`/`_valid_bot_class`/`_valid_bot_spec`
// (`50-party.sh:117,124-129,208-231`).
// ---------------------------------------------------------------------

/// `_valid_preset_name`: `^[A-Za-z0-9_-]{1,32}$`.
pub fn valid_preset_name(s: &str) -> bool {
    let n = s.chars().count();
    (1..=32).contains(&n) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// `Invalid preset name: {name}` (`BAD_ARG`) with a caller-supplied hint —
/// review Fix 3, same reasoning as [`invalid_player_err`]. `preset-save`'s
/// arm gives its own "Letters, digits, - and _ (max 32)." hint;
/// `preset-delete`/`preset-load` pass `""`.
pub fn invalid_preset_name_err(name: &str, hint: &str) -> CmdError {
    bad_arg(format!("Invalid preset name: {name}"), hint)
}

/// `_valid_bot_class`: the class set `party add --class` accepts.
/// Deathknight (class id 6) is deliberately excluded — see
/// `party_specs::class_name_from_id`'s matching exclusion.
pub fn valid_bot_class(s: &str) -> bool {
    matches!(
        s,
        "warrior" | "paladin" | "hunter" | "rogue" | "priest" | "shaman" | "mage" | "warlock" | "druid"
    )
}

/// `_valid_bot_spec`'s injection guard (`50-party.sh`):
/// `^[A-Za-z0-9][A-Za-z0-9 ._-]*$` — non-empty, starts alphanumeric, then
/// alphanumerics plus space/dot/underscore/hyphen. Deliberately wider than
/// the shipped names' plain lowercase-and-spaces: playerbots.conf is
/// hand-editable and the picker offers every conf name verbatim, so a
/// narrower rule here only produced specs the UI offered and this refused.
/// It stays narrow enough that the value is safe in the
/// `dml_whisper <p> <b> talents spec <name>` tail — no quotes, no backslash,
/// no CR/LF, no shell/SQL metacharacters. Mirrored by `isValidSpecShape` in
/// `launcher/src/lib/party-specs.ts`.
pub fn valid_bot_spec_shape(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '_' | '-'))
}

/// Static fallback spec-name mirror — the `_valid_bot_spec` fallback case
/// block (`50-party.sh:216-230`), used ONLY when no playerbots.conf is
/// deployed (verified against the shipped defaults, 2026-07-19; "bear pvp"/
/// "frostfire pvp" deliberately do NOT exist — do not "complete the
/// symmetry", per the bash comment this mirrors).
pub const FALLBACK_SPEC_NAMES: &[&str] = &[
    "arms pve", "arms pvp", "fury pve", "fury pvp", "prot pve", "prot pvp",
    "holy pve", "holy pvp", "ret pve", "ret pvp",
    "bm pve", "bm pvp", "mm pve", "mm pvp", "surv pve", "surv pvp",
    "as pve", "as pvp", "combat pve", "combat pvp", "subtlety pve", "subtlety pvp",
    "disc pve", "disc pvp", "shadow pve", "shadow pvp",
    "ele pve", "ele pvp", "enh pve", "enh pvp", "resto pve", "resto pvp",
    "arcane pve", "arcane pvp", "fire pve", "fire pvp", "frost pve", "frost pvp", "frostfire pve",
    "affli pve", "affli pvp", "demo pve", "demo pvp", "destro pve", "destro pvp",
    "balance pve", "balance pvp", "bear pve", "cat pve", "cat pvp",
];

/// `_valid_bot_spec` (`50-party.sh:208-231`): the shape guard first, then
/// membership against `live_names` (the deployed conf's
/// `AiPlayerbot.PremadeSpecName.*` values — the caller resolves these via
/// `party_specs::find_conf` + `parse_spec_rows`) when non-empty, else the
/// static [`FALLBACK_SPEC_NAMES`] mirror.
pub fn valid_bot_spec(want: &str, live_names: Option<&[String]>) -> bool {
    if !valid_bot_spec_shape(want) {
        return false;
    }
    match live_names {
        Some(names) if !names.is_empty() => names.iter().any(|n| n == want),
        _ => FALLBACK_SPEC_NAMES.contains(&want),
    }
}

/// `Unknown spec: {spec}` (`BAD_ARG`) with a caller-supplied hint — review
/// Fix 3, same reasoning as [`invalid_player_err`]. `party add --spec` and
/// `party botcmd … spec --spec` both reject through [`valid_bot_spec`] (shape
/// AND membership collapse into this one message — see that function's doc
/// comment) but each has its own hint text (`add`'s points at the launcher's
/// role picker; `botcmd`'s doesn't).
pub fn unknown_spec_err(spec: &str, hint: &str) -> CmdError {
    bad_arg(format!("Unknown spec: {spec}"), hint)
}

// ---------------------------------------------------------------------
// Command-string builders — `party add`/`kick`/`dismiss-all`/`relogin`/
// `botcmd` (`90-main.sh:3067-3282`). Online-guid lookups, the new-member
// poll, and DB member-list reads are orchestration (live in `lib.rs`), not
// pure builders.
// ---------------------------------------------------------------------

/// `Invalid player name: {player}` (`BAD_ARG`) with a caller-supplied hint —
/// review Fix 3. Every `dml-wow-cli` party arm that validates a player name
/// with its own hint (`kick`'s "needs --player…", the others' empty string)
/// previously hand-copied this exact message shape at each call site; a
/// second front end doing the same thing was exactly the drift the Task 12
/// review ruled out for `db_err_to_cmd` and this crate's own `29a7512`
/// follow-up fixed for the `botcmd` whisper. [`validate_player`] is this
/// function with an empty hint, kept private since none of its OWN callers
/// need a custom one.
pub fn invalid_player_err(player: &str, hint: &str) -> CmdError {
    bad_arg(format!("Invalid player name: {player}"), hint)
}

/// `Invalid bot name: {bot}` (`BAD_ARG`) — see [`invalid_player_err`].
pub fn invalid_bot_err(bot: &str, hint: &str) -> CmdError {
    bad_arg(format!("Invalid bot name: {bot}"), hint)
}

fn validate_player(player: &str) -> Result<(), CmdError> {
    if !valid_charname(player) {
        return Err(invalid_player_err(player, ""));
    }
    Ok(())
}

fn validate_bot(bot: &str) -> Result<(), CmdError> {
    if !valid_charname(bot) {
        return Err(invalid_bot_err(bot, ""));
    }
    Ok(())
}

/// `party add`'s pre-SOAP validation + bridge command (`90-main.sh:3067-
/// 3083`). `gender` must be `""`/`"male"`/`"female"`.
pub fn party_add_cmd(player: &str, class: &str, gender: &str) -> Result<String, CmdError> {
    validate_player(player)?;
    if !valid_bot_class(class) {
        return Err(bad_arg(
            format!("Invalid class: {class}"),
            "One of: warrior paladin hunter rogue priest shaman mage warlock druid",
        ));
    }
    match gender {
        "" | "male" | "female" => {}
        _ => return Err(bad_arg(format!("Invalid gender: {gender}"), "male or female")),
    }
    let mut cmd = format!("dml_addclass {player} {class}");
    if !gender.is_empty() {
        cmd.push(' ');
        cmd.push_str(gender);
    }
    Ok(cmd)
}

/// `party kick`'s uninvite fire (`90-main.sh:3158-3177`).
pub fn party_uninvite_cmd(bot: &str) -> Result<String, CmdError> {
    validate_bot(bot)?;
    Ok(format!("dml_uninvite {bot}"))
}

/// The master `logout` whisper `kick`/`dismiss-all`/`preset-load` fire after
/// an uninvite (best-effort at every call site — its own failure never
/// aborts the caller, matching the oracle's unchecked `|| true`).
pub fn party_logout_whisper_cmd(player: &str, bot: &str) -> Result<String, CmdError> {
    validate_player(player)?;
    validate_bot(bot)?;
    Ok(format!("dml_whisper {player} {bot} logout"))
}

/// `party relogin` (`90-main.sh:3235-3247`).
pub fn party_relogin_cmd(player: &str, bot: &str) -> Result<String, CmdError> {
    validate_player(player)?;
    validate_bot(bot)?;
    Ok(format!("dml_login {player} {bot}"))
}

/// `party botcmd`'s closed action allowlist (`90-main.sh:3265-3274`) for the
/// THREE fixed (no-argument) actions. `None` for `"spec"` (needs a live conf
/// lookup the caller owns — see [`valid_bot_spec`]/[`spec_action_wmsg`]) and
/// for anything else (the caller reports `BAD_ARG "Invalid action: …"`).
pub fn botcmd_fixed_tail(action: &str) -> Option<&'static str> {
    match action {
        "gear" => Some("autogear"),
        "talents" => Some("talents autopick"),
        "maintain" => Some("maintenance"),
        _ => None,
    }
}

/// `party botcmd`'s whisper (`90-main.sh:3277`), assembled once the caller has
/// validated BOTH names ([`crate::soap_cmds::valid_charname`]) and resolved
/// `wmsg` from the closed allowlist ([`botcmd_fixed_tail`] /
/// [`spec_action_wmsg`]). A builder rather than an inline `format!` at the
/// call site so the one place this command string is spelled is here, next to
/// its siblings — the launcher's `wow_party_botcmd_native` still formats it
/// inline and should be pointed at this instead when it is next touched.
pub fn botcmd_whisper_cmd(player: &str, bot: &str, wmsg: &str) -> String {
    format!("dml_whisper {player} {bot} {wmsg}")
}

/// The `action == "spec"` whisper tail, once `spec` has already passed the
/// non-empty + live-validity checks at the call site (`90-main.sh:3269-
/// 3273`).
pub fn spec_action_wmsg(spec: &str) -> String {
    format!("talents spec {spec}")
}

/// `preset-load`'s per-class bridge fire (`90-main.sh:3410`) — same shape as
/// [`party_add_cmd`] but with no gender/spec (the replace flow never carries
/// either).
pub fn preset_load_addclass_cmd(player: &str, class: &str) -> Result<String, CmdError> {
    validate_player(player)?;
    Ok(format!("dml_addclass {player} {class}"))
}

/// Post-join whisper pair — `talents autopick` then `autogear`
/// (`90-main.sh:3419-3420`, `preset-load`'s per-bot finish).
pub fn talents_autopick_whisper_cmd(player: &str, botname: &str) -> String {
    format!("dml_whisper {player} {botname} talents autopick")
}
pub fn autogear_whisper_cmd(player: &str, botname: &str) -> String {
    format!("dml_whisper {player} {botname} autogear")
}
/// The optional post-`party add` spec whisper pair (`90-main.sh:3109-3111`).
pub fn spec_whisper_cmd(player: &str, botname: &str, spec: &str) -> String {
    format!("dml_whisper {player} {botname} talents spec {spec}")
}

// ---------------------------------------------------------------------
// SQL — bound-param text only; the actual `db::query_with_params` calls
// happen in `lib.rs` (this module stays DB-connection-free, matching
// `soap_cmds`).
// ---------------------------------------------------------------------

/// `_party_online_guid` (`50-party.sh:46-49`).
pub const ONLINE_GUID_SQL: &str = "SELECT guid FROM characters WHERE name=? AND online=1 LIMIT 1";

/// `_party_group_member_guids` (`50-party.sh:52-55`).
pub const GROUP_MEMBER_GUIDS_SQL: &str =
    "SELECT memberGuid FROM group_member WHERE guid=(SELECT guid FROM group_member WHERE memberGuid=? LIMIT 1)";

/// The bot-members-of-a-party query shared BYTE-IDENTICALLY by `dismiss-all`
/// (`90-main.sh:3189-3194`) and `preset-load`'s kick phase (`90-main.sh:3377-
/// 3382`).
/// Bot identity is [`crate::botid::bot_clause`] (registry OR reserved account
/// prefix): with registry-only detection, a party full of bots read back as
/// zero bots, so `dismiss-all` dismissed nobody and `preset-save` saved an
/// empty preset.
pub fn bot_member_names_sql(bot_prefix: &str, names: &crate::db::DatabaseNames) -> String {
    format!(
        "SELECT c.name \
         FROM group_member gm \
         JOIN characters c ON c.guid = gm.memberGuid \
         WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=? LIMIT 1) \
           AND {} \
         ORDER BY c.name",
        crate::botid::bot_clause("c.account", bot_prefix, &names.auth, names.playerbots.as_deref())
    )
}

/// `preset-save`'s bot-class query (`90-main.sh:3296-3301`) — same JOIN
/// shape as [`bot_member_names_sql`] but selects `c.class` (no name needed
/// for a class-only preset).
pub fn bot_member_classes_sql(bot_prefix: &str, names: &crate::db::DatabaseNames) -> String {
    format!(
        "SELECT c.class \
         FROM group_member gm \
         JOIN characters c ON c.guid = gm.memberGuid \
         WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=? LIMIT 1) \
           AND {} \
         ORDER BY c.name",
        crate::botid::bot_clause("c.account", bot_prefix, &names.auth, names.playerbots.as_deref())
    )
}

/// Bot name-by-guid lookup after a successful join (`90-main.sh:3102,3417`).
pub const CHAR_NAME_BY_GUID_SQL: &str = "SELECT name FROM characters WHERE guid=? LIMIT 1";

// ---------------------------------------------------------------------
// New-member poll — `_party_wait_new_member` (`50-party.sh:85-101`).
// ---------------------------------------------------------------------

/// One iteration's membership test: the first guid in `members_now` (a fresh
/// DB read, in query order) that is neither `pguid` itself nor already in
/// `before` (the pre-fire snapshot). `None` means no new member showed up
/// THIS iteration — the caller retries up to [`poll_tries_from_env`] times.
pub fn find_new_member(members_now: &[i64], pguid: i64, before: &std::collections::HashSet<i64>) -> Option<i64> {
    members_now.iter().copied().find(|&g| g != pguid && !before.contains(&g))
}

/// `DML_PARTY_POLL_TRIES` (default 12) — env override, same convention as
/// `backup::backup_keep_from_env`.
pub fn poll_tries_from_env() -> u32 {
    std::env::var("DML_PARTY_POLL_TRIES").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(12)
}

/// `DML_PARTY_POLL_SLEEP` (default 0.5s) — env override, fractional seconds
/// allowed (matching the bash's `sleep "$slp"`).
pub fn poll_sleep_from_env() -> std::time::Duration {
    std::env::var("DML_PARTY_POLL_SLEEP")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .map(std::time::Duration::from_secs_f64)
        .unwrap_or(std::time::Duration::from_millis(500))
}

// ---------------------------------------------------------------------
// Preset file management — `_preset_dir` + preset-save/-list/-delete/-load/
// -show/-import (`90-main.sh:3283-3483`). One class name per line,
// LF-terminated, no header/metadata.
// ---------------------------------------------------------------------

/// `_preset_dir` (`50-party.sh:114`): `~/.dml/party-presets`.
pub fn preset_dir() -> Option<PathBuf> {
    super::dml_home_dir().map(|h| h.join("party-presets"))
}

pub fn preset_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(name)
}

/// Preset FILE CONTENT for `preset-save`/`preset-import`: one class name per
/// line, LF-terminated (`printf '%s' "$names"` where `$names` was itself
/// built as `"$cname"$'\n'` per bot — i.e. every line, including the last,
/// ends in `\n`, and there is no extra trailing blank line beyond that).
pub fn preset_file_content(classes: &[String]) -> String {
    classes.iter().map(|c| format!("{c}\n")).collect()
}

/// Parse a preset file's raw content into its class-name lines, dropping
/// any blank line (`[[ -z "$cls" ]] && continue`, shared by preset-load's
/// read loop and preset-show, `90-main.sh:3405-3406,3443-3444`).
pub fn parse_preset_classes(content: &str) -> Vec<String> {
    content.split('\n').filter(|l| !l.is_empty()).map(str::to_string).collect()
}

/// One `preset-list` row (`90-main.sh:3322-3337`).
#[derive(Debug, Clone, PartialEq)]
pub struct PresetInfo {
    pub name: String,
    pub bots: usize,
}

/// `preset-list` (`90-main.sh:3322-3337`): every [`valid_preset_name`] FILE
/// (not subdir) directly under `dir`, sorted by name ascending — matching
/// `glob(3)`'s default sort order, which the oracle's unsorted
/// `for f in "$pdir"/*` relies on (bash does not pass `GLOB_NOSORT`) — each
/// paired with its non-blank line count (the oracle's `grep -c .`, which is
/// the same count [`parse_preset_classes`] would yield). A missing `dir`
/// degrades to an empty list, matching the oracle's `[[ -d "$pdir" ]]` guard;
/// an unreadable file degrades to a 0 count, matching `grep -c . "$f" \
/// 2>/dev/null || cnt=0`.
pub fn list_presets(dir: &Path) -> Vec<PresetInfo> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| valid_preset_name(n))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| {
            let content = std::fs::read_to_string(dir.join(&name)).unwrap_or_default();
            let bots = parse_preset_classes(&content).len();
            PresetInfo { name, bots }
        })
        .collect()
}

/// `preset-import`'s `--classes` CSV split + per-token validation
/// (`90-main.sh:3461-3469`): the empty-string precheck happens first (its
/// own message), then EVERY token must be a valid class BEFORE any write —
/// the first invalid token's error is returned and nothing is written,
/// matching the oracle's abort-before-any-fs-mutation contract.
pub fn parse_import_classes(classes: &str) -> Result<Vec<String>, CmdError> {
    if classes.is_empty() {
        return Err(bad_arg(
            "Missing --classes <comma-separated list>",
            "One of: warrior paladin hunter rogue priest shaman mage warlock druid",
        ));
    }
    let mut out = Vec::new();
    for c in classes.split(',') {
        if !valid_bot_class(c) {
            return Err(bad_arg(
                format!("Invalid class: {c}"),
                "One of: warrior paladin hunter rogue priest shaman mage warlock druid",
            ));
        }
        out.push(c.to_string());
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Live DB reads + the `preset-load` stream — moved out of the launcher's
// `lib.rs` by the cargo-workspace refactor (Task 9). Every read below is
// bound-param over `db::query_with_params` against the Characters DB (the
// same schema `party online`/`party list`/the gm bridge commands already
// read).
// ---------------------------------------------------------------------------

/// The DML state dir that holds saved presets, or an `INTERNAL` error when
/// neither `USERPROFILE` nor `HOME` is set.
pub fn preset_dir_or_internal_err() -> Result<std::path::PathBuf, CmdError> {
    crate::party::preset_dir().ok_or_else(|| CmdError {
        code: "INTERNAL".into(),
        message: "could not resolve the DML state directory (USERPROFILE/HOME not set)".into(),
        hint: String::new(),
    })
}

/// `_party_online_guid` (`50-party.sh:46-49`) over a direct MySQL
/// connection. Any query failure reads as "not online" (`None`), matching
/// the bash's own `2>/dev/null` swallow — same doctrine as `char_is_online`.
pub fn party_online_guid(cfg: &crate::db::DbConfig, name: &str) -> Option<i64> {
    // utf8mb4_bin lookup -- canonical form or nothing (db::canon_char_name).
    let params: Vec<mysql::Value> = vec![mysql::Value::from(crate::db::canon_char_name(name))];
    crate::db::query_with_params(cfg, crate::db::Database::Characters, crate::party::ONLINE_GUID_SQL, params)
        .ok()
        .and_then(|res| sql_row_int(res.rows.first().and_then(|r| r.first())))
}

/// `_party_group_member_guids` (`50-party.sh:52-55`).
pub fn group_member_guids(cfg: &crate::db::DbConfig, pguid: i64) -> Vec<i64> {
    let params: Vec<mysql::Value> = vec![mysql::Value::from(pguid)];
    crate::db::query_with_params(cfg, crate::db::Database::Characters, crate::party::GROUP_MEMBER_GUIDS_SQL, params)
        .map(|res| res.rows.iter().filter_map(|r| sql_row_int(r.first())).collect())
        .unwrap_or_default()
}

/// Bot's name-by-guid lookup after a successful join (`90-main.sh:3102,
/// 3417`).
pub fn char_name_by_guid(cfg: &crate::db::DbConfig, guid: i64) -> Option<String> {
    let params: Vec<mysql::Value> = vec![mysql::Value::from(guid)];
    crate::db::query_with_params(cfg, crate::db::Database::Characters, crate::party::CHAR_NAME_BY_GUID_SQL, params)
        .ok()
        .and_then(|res| cell_string(res.rows.first().and_then(|r| r.first())))
}

/// The bot-members-of-a-party names query, shared by `dismiss-all` and
/// `preset-load`'s kick phase ([`crate::party::BOT_MEMBER_NAMES_SQL`]).
/// Unlike `party_online_guid`/`group_member_guids`/`char_name_by_guid`
/// (which swallow query failure exactly like the oracle's own `2>/dev/null`
/// helpers do), a failure here MUST surface: `dismiss-all`'s bash caller
/// explicitly checks this query and exits `DB_UNREACHABLE "Could not read
/// the party"` on failure (`90-main.sh:3195-3196`) rather than treating an
/// unreachable DB as "zero bots" -- so this returns `Result`, not a
/// silently-emptied `Vec`.
pub fn bot_member_names(cfg: &crate::db::DbConfig, pguid: i64) -> Result<Vec<String>, CmdError> {
    // Names-unresolved surfaces as DB_NAMES_UNRESOLVED (via db_err_to_cmd),
    // never as "Could not read the party" about a healthy server.
    let names = cfg.names().map_err(crate::db::db_err_to_cmd)?;
    let params: Vec<mysql::Value> = vec![mysql::Value::from(pguid)];
    let sql = bot_member_names_sql(&crate::botid::bot_account_prefix(), names);
    crate::db::query_with_params(cfg, crate::db::Database::Characters, &sql, params)
        .map(|res| res.rows.iter().filter_map(|r| cell_string(r.first())).collect())
        .map_err(|_| db_unreachable_err("Could not read the party"))
}

/// `preset-save`'s bot-classes query ([`crate::party::
/// BOT_MEMBER_CLASSES_SQL`]). Same doctrine as `bot_member_names`: the
/// oracle's `preset-save` caller checks this query and exits
/// `DB_UNREACHABLE "Could not read the party"` on failure
/// (`90-main.sh:3302-3303`), so a query error must propagate rather than be
/// swallowed into an empty (and thus falsely "no bots to save") list.
pub fn bot_member_classes(cfg: &crate::db::DbConfig, pguid: i64) -> Result<Vec<i64>, CmdError> {
    let names = cfg.names().map_err(crate::db::db_err_to_cmd)?;
    let params: Vec<mysql::Value> = vec![mysql::Value::from(pguid)];
    let sql = bot_member_classes_sql(&crate::botid::bot_account_prefix(), names);
    crate::db::query_with_params(cfg, crate::db::Database::Characters, &sql, params)
        .map(|res| res.rows.iter().filter_map(|r| sql_row_int(r.first())).collect())
        .map_err(|_| db_unreachable_err("Could not read the party"))
}

/// `_party_wait_new_member` (`50-party.sh:85-101`): poll up to
/// `poll_tries_from_env()` times (sleeping `poll_sleep_from_env()` between)
/// for a group member guid that wasn't in `before`. The per-iteration
/// membership test is [`crate::party::find_new_member`] (pure,
/// unit-tested); this wrapper owns only the retry/sleep mechanics.
pub fn wait_new_member(cfg: &crate::db::DbConfig, pguid: i64, before: &[i64]) -> Option<i64> {
    let before_set: std::collections::HashSet<i64> = before.iter().copied().collect();
    let tries = crate::party::poll_tries_from_env();
    let sleep = crate::party::poll_sleep_from_env();
    for i in 0..tries {
        let now = group_member_guids(cfg, pguid);
        if let Some(g) = crate::party::find_new_member(&now, pguid, &before_set) {
            return Some(g);
        }
        if i + 1 < tries && !sleep.is_zero() {
            std::thread::sleep(sleep);
        }
    }
    None
}

const PRESET_LOAD_SECTION: &str = "preset-load";

/// `party preset-load`'s full orchestration (`90-main.sh:3348-3435`): kick
/// phase (replace semantics — every current bot goes, best-effort per bot)
/// then a join phase (one `dml_addclass` + new-member poll per preset
/// class line, talents/gear whispers on a successful join). Streamed NDJSON
/// (`section_start`/`line`/`section_end`/`done`/`error`) — same vocabulary
/// as `modmgr::module_install_stream`.
pub fn party_preset_load_stream(
    player: String,
    name: String,
    lock: Arc<std::sync::Mutex<()>>,
    emit: impl Fn(serde_json::Value),
) {
    use crate::modmgr::{done_event, error_event, line_event, section_end, section_start};

    emit(section_start(PRESET_LOAD_SECTION));

    let dir = match preset_dir_or_internal_err() {
        Ok(d) => d,
        Err(e) => {
            emit(section_end(PRESET_LOAD_SECTION, "error"));
            emit(error_event(&e.code, e.message, &e.hint));
            return;
        }
    };
    let path = crate::party::preset_path(&dir, &name);
    if !path.is_file() {
        emit(section_end(PRESET_LOAD_SECTION, "error"));
        emit(error_event("NOT_FOUND", format!("No preset named {name}"), ""));
        return;
    }

    let db_cfg = crate::db::DbConfig::from_env();
    let Some(pguid) = party_online_guid(&db_cfg, &player) else {
        emit(section_end(PRESET_LOAD_SECTION, "error"));
        emit(error_event(
            "NOT_FOUND",
            format!("Character not online: {player}"),
            "Log the character into the game first.",
        ));
        return;
    };

    let soap_cfg = crate::soap::SoapConfig::load();

    // Kick phase (replace semantics): every current bot goes. Byte-faithful
    // swallow here (unlike `dismiss-all`/`preset-save`'s propagation): bash's
    // own `kicklist="$(db_chars_query ...)" || kicklist=""` at
    // `90-main.sh:3383` silently treats a query failure as "nothing to kick"
    // too.
    for b in bot_member_names(&db_cfg, pguid).unwrap_or_default() {
        if !crate::soap_cmds::valid_charname(&b) {
            continue;
        }
        let kicked_ok = {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            matches!(crate::soap::exec(&soap_cfg, &format!("dml_uninvite {b}")), crate::soap::SoapOutcome::Ok(_))
        };
        emit(line_event(if kicked_ok { "info" } else { "warn" }, if kicked_ok { format!("kicked {b}") } else { format!("could not kick {b}") }));
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let _ = crate::soap::exec(&soap_cfg, &format!("dml_whisper {player} {b} logout"));
    }

    // Join phase: one addclass + new-member poll per preset class line.
    let content = std::fs::read_to_string(&path).unwrap_or_default();
    let (mut requested, mut joined) = (0u32, 0u32);
    for cls in crate::party::parse_preset_classes(&content) {
        if !crate::party::valid_bot_class(&cls) {
            emit(line_event("warn", format!("skipping unknown class: {cls}")));
            continue;
        }
        requested += 1;
        let before = group_member_guids(&db_cfg, pguid);
        let fired_ok = {
            let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
            matches!(
                crate::soap::exec(&soap_cfg, &format!("dml_addclass {player} {cls}")),
                crate::soap::SoapOutcome::Ok(_)
            )
        };
        if !fired_ok {
            emit(line_event("warn", format!("add {cls} was rejected")));
            continue;
        }
        match wait_new_member(&db_cfg, pguid, &before) {
            Some(g) => {
                joined += 1;
                match char_name_by_guid(&db_cfg, g) {
                    Some(bname) => {
                        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
                        let _ = crate::soap::exec(&soap_cfg, &crate::party::talents_autopick_whisper_cmd(&player, &bname));
                        let _ = crate::soap::exec(&soap_cfg, &crate::party::autogear_whisper_cmd(&player, &bname));
                        emit(line_event("info", format!("{bname} joined -- talents + gear applied")));
                    }
                    None => emit(line_event("info", format!("a {cls} joined"))),
                }
            }
            None => emit(line_event("warn", format!("{cls} did not attach in time"))),
        }
    }

    emit(section_end(PRESET_LOAD_SECTION, "ok"));
    emit(done_event(serde_json::json!({"loaded": true, "requested": requested, "joined": joined})));
}

/// `NOT_FOUND` for an offline character in a `party`-family arm — same code
/// as `not_online_err` (gm) but with a CALLER-SUPPLIED hint, since each
/// `party` sub-arm's oracle spells a slightly different one (`90-main.sh`:
/// add "Log the character into the game first, then try again.";
/// dismiss-all/preset-save/preset-load "Log the character into the game
/// first."; botcmd's bot-side check "The bot must be in the world -- is it
/// still in your party?").
pub fn party_not_online_err(who: &str, hint: &str) -> CmdError {
    CmdError { code: "NOT_FOUND".into(), message: format!("Character not online: {who}"), hint: hint.into() }
}

/// `_party_spec_names` (`50-party.sh:151-165`) read straight off the
/// deployed playerbots.conf (or its `.dist`) via the already-native `party_
/// specs` reader — the single source of truth `wow_party_specs_read` also
/// uses. `None` when no conf is deployed at all (the caller then falls back
/// to `valid_bot_spec`'s static mirror), matching `_party_pb_conf`'s own
/// "nothing deployed" case.
pub fn live_spec_names(title_dir: &std::path::Path) -> Option<Vec<String>> {
    let (conf_path, _source) = crate::party_specs::find_conf(title_dir)?;
    let content = std::fs::read_to_string(&conf_path).ok()?;
    Some(
        crate::party_specs::parse_spec_rows(&content)
            .into_iter()
            .map(|r| r.name)
            .filter(|n| !n.is_empty())
            .collect(),
    )
}

// ---------------------------------------------------------------------------
// NATIVE-MODE `party add` (`90-main.sh:3067-3130`) — moved out of the
// launcher's `lib.rs` by the cargo-workspace refactor (Task 9b). A
// four-outcome partial-success state machine, kept whole: online-guid lookup
// -> pre-fire member snapshot -> SOAP fire -> new-member poll (may time out:
// "added but not joined") -> name resolution (may fail: "joined but
// unnamed") -> spec + autogear whispers under the lock. `cmd` is the
// already-built (and therefore already-validated) `party_add_cmd` string and
// `spec`, if present, has already been checked against `live_spec_names`.
// ---------------------------------------------------------------------------

pub fn party_add(
    player: String,
    cmd: String,
    spec: Option<String>,
    lock: Arc<Mutex<()>>,
) -> Result<serde_json::Value, CmdError> {
    let db_cfg = crate::db::DbConfig::from_env();
    let pguid = party_online_guid(&db_cfg, &player)
        .ok_or_else(|| party_not_online_err(&player, "Log the character into the game first, then try again."))?;
    let before = group_member_guids(&db_cfg, pguid);
    {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = crate::soap::SoapConfig::load();
        let outcome = crate::soap::exec(&soap_cfg, &cmd);
        party_fire_result(outcome, "add")?;
    }
    let Some(newguid) = wait_new_member(&db_cfg, pguid, &before) else {
        return Ok(if let Some(s) = &spec {
            serde_json::json!({"added":true,"joined":false,"bot":null,"note":"Added but spec not applied -- bot not attached in time","spec":s,"spec_applied":false})
        } else {
            serde_json::json!({"added":true,"joined":false,"bot":null,"note":"Spawned but not attached yet -- give it a moment and Refresh."})
        });
    };
    let Some(botname) = char_name_by_guid(&db_cfg, newguid) else {
        return Ok(if let Some(s) = &spec {
            serde_json::json!({"added":true,"joined":true,"bot":null,"note":"Added but spec not applied -- bot not attached in time","spec":s,"spec_applied":false})
        } else {
            serde_json::json!({"added":true,"joined":true,"bot":null,"note":null})
        });
    };
    if let Some(s) = &spec {
        let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
        let soap_cfg = crate::soap::SoapConfig::load();
        let o1 = crate::soap::exec(&soap_cfg, &crate::party::spec_whisper_cmd(&player, &botname, s));
        party_fire_result(o1, "spec")?;
        let o2 = crate::soap::exec(&soap_cfg, &crate::party::autogear_whisper_cmd(&player, &botname));
        party_fire_result(o2, "spec")?;
        Ok(serde_json::json!({"added":true,"joined":true,"bot":botname,"note":null,"spec":s,"spec_applied":true}))
    } else {
        Ok(serde_json::json!({"added":true,"joined":true,"bot":botname,"note":null}))
    }
}

// ---------------------------------------------------------------------------
// Preset save/list/delete orchestrations (Task 13). Same hoist rationale as
// `party_add`/`party_preset_load_stream` above: each is a DB read and/or a
// filesystem mutation with its own domain rules (an empty party is NOT_FOUND,
// a missing preset file is NOT_FOUND), which is library work, not CLI work.
// `launcher/src-tauri/src/lib.rs` still runs its own inline copies of these
// three bodies (`wow_party_preset_save_native` / `_list_native` /
// `_delete_native`) — this task was not allowed to modify the launcher, so
// those should be collapsed onto these functions next time it is touched.
//
// The NAME/PLAYER guards deliberately stay OUT of these functions, exactly
// like `party_add`'s: each caller runs `valid_charname` / `valid_preset_name`
// first (the launcher does, and the CLI must too), because a bad preset name
// is a path-traversal question that has to be answered before a path is ever
// joined.
// ---------------------------------------------------------------------------

/// `NOT_FOUND` for a preset that isn't on disk (`90-main.sh:3343,3440`).
pub fn preset_not_found(name: &str) -> CmdError {
    CmdError { code: "NOT_FOUND".into(), message: format!("No preset named {name}"), hint: String::new() }
}

/// `party preset-save` (`90-main.sh:3283-3321`): snapshot the caller's current
/// bot party as a class list. `player` MUST already have passed
/// [`crate::soap_cmds::valid_charname`] and `name` [`valid_preset_name`].
pub fn preset_save(player: String, name: String) -> Result<serde_json::Value, CmdError> {
    let db_cfg = crate::db::DbConfig::from_env();
    let pguid = party_online_guid(&db_cfg, &player)
        .ok_or_else(|| party_not_online_err(&player, "Log the character into the game first."))?;
    let names: Vec<String> = bot_member_classes(&db_cfg, pguid)?
        .into_iter()
        .filter_map(crate::party_specs::class_name_from_id)
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
    std::fs::create_dir_all(&dir).map_err(dml_core::error::io_internal_err)?;
    let path = preset_path(&dir, &name);
    let overwrote = path.is_file();
    std::fs::write(&path, preset_file_content(&names)).map_err(dml_core::error::io_internal_err)?;
    Ok(serde_json::json!({"saved": true, "name": name, "bots": names, "overwrote": overwrote}))
}

/// `party preset-list` (`90-main.sh:3322-3337`) — read-only; a missing preset
/// dir is an empty list, not an error (see [`list_presets`]).
pub fn preset_list() -> Result<serde_json::Value, CmdError> {
    let dir = preset_dir_or_internal_err()?;
    let presets: Vec<serde_json::Value> = list_presets(&dir)
        .into_iter()
        .map(|p| serde_json::json!({"name": p.name, "bots": p.bots}))
        .collect();
    Ok(serde_json::json!({"presets": presets}))
}

/// `party preset-delete` (`90-main.sh:3339-3347`). `name` MUST already have
/// passed [`valid_preset_name`] — that check, not this function, is what keeps
/// a `../…` argument from being joined onto the preset dir.
pub fn preset_delete(name: String) -> Result<serde_json::Value, CmdError> {
    let dir = preset_dir_or_internal_err()?;
    let path = preset_path(&dir, &name);
    if !path.is_file() {
        return Err(preset_not_found(&name));
    }
    std::fs::remove_file(&path).map_err(dml_core::error::io_internal_err)?;
    Ok(serde_json::json!({"deleted": true, "name": name}))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- valid_preset_name -------------------------------------------------

    #[test]
    fn valid_preset_name_boundaries() {
        assert!(!valid_preset_name(""));
        assert!(valid_preset_name("a"));
        assert!(valid_preset_name(&"a".repeat(32)));
        assert!(!valid_preset_name(&"a".repeat(33)));
        assert!(valid_preset_name("my-preset_1"));
        assert!(!valid_preset_name("bad name"));
        assert!(!valid_preset_name("../evil"));
    }

    // -- review Fix 3: shared error-message builders (`invalid_player_err`/
    // `invalid_bot_err`/`invalid_preset_name_err`/`unknown_spec_err`) --------

    #[test]
    fn invalid_preset_name_err_carries_message_and_caller_hint() {
        let e = invalid_preset_name_err("../evil", "Letters, digits, - and _ (max 32).");
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Invalid preset name: ../evil");
        assert_eq!(e.hint, "Letters, digits, - and _ (max 32).");

        let e2 = invalid_preset_name_err("../evil", "");
        assert_eq!(e2.hint, "");
    }

    // -- valid_bot_class -------------------------------------------------

    #[test]
    fn valid_bot_class_excludes_deathknight_and_unknowns() {
        for c in ["warrior", "paladin", "hunter", "rogue", "priest", "shaman", "mage", "warlock", "druid"] {
            assert!(valid_bot_class(c), "{c} should be valid");
        }
        assert!(!valid_bot_class("deathknight"));
        assert!(!valid_bot_class("Warrior"));
        assert!(!valid_bot_class(""));
    }

    // -- valid_bot_spec_shape / valid_bot_spec -------------------------------

    #[test]
    fn valid_bot_spec_shape_accepts_the_names_a_conf_can_realistically_carry() {
        assert!(valid_bot_spec_shape("frost pve"));
        // A hand-written conf entry is not obliged to be lowercase-with-spaces;
        // rejecting these made the picker offer specs the validator refused.
        assert!(valid_bot_spec_shape("Frost PvE"));
        assert!(valid_bot_spec_shape("frost-pve"));
        assert!(valid_bot_spec_shape("frost_pve"));
        assert!(valid_bot_spec_shape("frost pve 2.0"));
    }

    #[test]
    fn valid_bot_spec_shape_still_rejects_everything_whisper_unsafe() {
        // The tail is spliced into `dml_whisper <p> <b> talents spec <name>`,
        // so quotes/backslashes/CR-LF/shell+SQL metacharacters stay out.
        assert!(!valid_bot_spec_shape(""));
        assert!(!valid_bot_spec_shape(" frost")); // leading space
        assert!(!valid_bot_spec_shape("-frost")); // must start alphanumeric
        assert!(!valid_bot_spec_shape("frost'pve"));
        assert!(!valid_bot_spec_shape("frost\"pve"));
        assert!(!valid_bot_spec_shape("frost\\pve"));
        assert!(!valid_bot_spec_shape("frost\npve"));
        assert!(!valid_bot_spec_shape("frost\rpve"));
        assert!(!valid_bot_spec_shape("frost pve; .server shutdown"));
        assert!(!valid_bot_spec_shape("frost'; DROP TABLE bots; --"));
        assert!(!valid_bot_spec_shape("$(id)"));
        assert!(!valid_bot_spec_shape("frost<pve>"));
        assert!(!valid_bot_spec_shape("frost&pve"));
    }

    #[test]
    fn valid_bot_spec_uses_live_names_when_present() {
        let live = vec!["custom spec".to_string()];
        assert!(valid_bot_spec("custom spec", Some(&live)));
        // Not in the live list, and live list is non-empty -> fallback NOT consulted.
        assert!(!valid_bot_spec("frost pve", Some(&live)));
        // A non-lowercase conf name is now shape-legal, so membership decides:
        // the picker offers it and the validator accepts the same string.
        let live_mixed = vec!["Arms PvE".to_string()];
        assert!(valid_bot_spec("Arms PvE", Some(&live_mixed)));
        assert!(!valid_bot_spec("arms pve", Some(&live_mixed)));
    }

    #[test]
    fn valid_bot_spec_falls_back_to_static_mirror_when_no_live_conf() {
        assert!(valid_bot_spec("frost pve", None));
        assert!(valid_bot_spec("frost pve", Some(&[])));
        assert!(!valid_bot_spec("bear pvp", None)); // deliberately absent
        assert!(!valid_bot_spec("nonsense", None));
    }

    #[test]
    fn unknown_spec_err_carries_message_and_caller_hint() {
        let e = unknown_spec_err("Frost PvE", "A premade spec name like 'frost pve'.");
        assert_eq!(e.code, "BAD_ARG");
        assert_eq!(e.message, "Unknown spec: Frost PvE");
        assert_eq!(e.hint, "A premade spec name like 'frost pve'.");
    }

    // -- invalid_player_err / invalid_bot_err --------------------------------

    #[test]
    fn invalid_player_err_and_invalid_bot_err_carry_message_and_caller_hint() {
        let p = invalid_player_err("bad name", "Kick needs --player (the bot's master).");
        assert_eq!(p.code, "BAD_ARG");
        assert_eq!(p.message, "Invalid player name: bad name");
        assert_eq!(p.hint, "Kick needs --player (the bot's master).");

        let b = invalid_bot_err("bad bot", "");
        assert_eq!(b.code, "BAD_ARG");
        assert_eq!(b.message, "Invalid bot name: bad bot");
        assert_eq!(b.hint, "");
    }

    /// [`validate_player`]/[`validate_bot`] (the builders' own internal
    /// guards) are now thin wrappers over the same two functions with an
    /// empty hint — proven here so the two never drift apart silently.
    /// `CmdError` has no `PartialEq`, so the fields are compared by hand.
    #[test]
    fn validate_player_and_validate_bot_delegate_to_the_shared_err_builders() {
        let got = validate_player("bad name").unwrap_err();
        let want = invalid_player_err("bad name", "");
        assert_eq!(got.code, want.code);
        assert_eq!(got.message, want.message);
        assert_eq!(got.hint, want.hint);

        let got = validate_bot("bad bot").unwrap_err();
        let want = invalid_bot_err("bad bot", "");
        assert_eq!(got.code, want.code);
        assert_eq!(got.message, want.message);
        assert_eq!(got.hint, want.hint);
    }

    // -- party_add_cmd -------------------------------------------------

    #[test]
    fn party_add_cmd_happy_path_no_gender() {
        assert_eq!(party_add_cmd("Testen", "warrior", "").unwrap(), "dml_addclass Testen warrior");
    }

    #[test]
    fn party_add_cmd_with_gender() {
        assert_eq!(party_add_cmd("Testen", "priest", "female").unwrap(), "dml_addclass Testen priest female");
    }

    #[test]
    fn party_add_cmd_rejects_bad_player_class_gender() {
        assert_eq!(party_add_cmd("bad name", "warrior", "").unwrap_err().code, "BAD_ARG");
        assert_eq!(party_add_cmd("Testen", "deathknight", "").unwrap_err().code, "BAD_ARG");
        assert_eq!(party_add_cmd("Testen", "warrior", "other").unwrap_err().code, "BAD_ARG");
    }

    // -- party_uninvite_cmd / party_logout_whisper_cmd / party_relogin_cmd --

    #[test]
    fn party_uninvite_cmd_happy_path() {
        assert_eq!(party_uninvite_cmd("Botty").unwrap(), "dml_uninvite Botty");
    }

    #[test]
    fn party_logout_whisper_cmd_happy_path() {
        assert_eq!(party_logout_whisper_cmd("Testen", "Botty").unwrap(), "dml_whisper Testen Botty logout");
    }

    #[test]
    fn party_relogin_cmd_happy_path() {
        assert_eq!(party_relogin_cmd("Testen", "Botty").unwrap(), "dml_login Testen Botty");
    }

    #[test]
    fn party_relogin_cmd_rejects_invalid_names() {
        assert_eq!(party_relogin_cmd("bad name", "Botty").unwrap_err().code, "BAD_ARG");
        assert_eq!(party_relogin_cmd("Testen", "bad name").unwrap_err().code, "BAD_ARG");
    }

    // -- botcmd_fixed_tail / spec_action_wmsg -------------------------------

    #[test]
    fn botcmd_fixed_tail_gear_talents_maintain() {
        assert_eq!(botcmd_fixed_tail("gear"), Some("autogear"));
        assert_eq!(botcmd_fixed_tail("talents"), Some("talents autopick"));
        assert_eq!(botcmd_fixed_tail("maintain"), Some("maintenance"));
    }

    #[test]
    fn botcmd_fixed_tail_none_for_spec_and_unknown() {
        assert_eq!(botcmd_fixed_tail("spec"), None);
        assert_eq!(botcmd_fixed_tail("dance"), None);
    }

    #[test]
    fn spec_action_wmsg_builds_tail() {
        assert_eq!(spec_action_wmsg("frost pve"), "talents spec frost pve");
    }

    #[test]
    fn botcmd_whisper_cmd_joins_master_bot_and_tail() {
        assert_eq!(
            botcmd_whisper_cmd("Testen", "Botty", botcmd_fixed_tail("gear").unwrap()),
            "dml_whisper Testen Botty autogear"
        );
        assert_eq!(
            botcmd_whisper_cmd("Testen", "Botty", &spec_action_wmsg("frost pve")),
            "dml_whisper Testen Botty talents spec frost pve"
        );
    }

    // -- find_new_member -------------------------------------------------

    #[test]
    fn find_new_member_finds_first_guid_not_in_before_or_self() {
        let before: std::collections::HashSet<i64> = [1, 2].into_iter().collect();
        assert_eq!(find_new_member(&[1, 2, 3], 1, &before), Some(3));
        assert_eq!(find_new_member(&[1, 2], 1, &before), None);
        assert_eq!(find_new_member(&[], 1, &before), None);
    }

    // -- poll_tries_from_env / poll_sleep_from_env ------------------------

    #[test]
    fn poll_tries_from_env_default_is_twelve() {
        std::env::remove_var("DML_PARTY_POLL_TRIES");
        assert_eq!(poll_tries_from_env(), 12);
    }

    #[test]
    fn poll_sleep_from_env_default_is_half_second() {
        std::env::remove_var("DML_PARTY_POLL_SLEEP");
        assert_eq!(poll_sleep_from_env(), std::time::Duration::from_millis(500));
    }

    // -- preset file content / parse -------------------------------------

    #[test]
    fn preset_file_content_lf_terminates_every_line() {
        let classes = vec!["warrior".to_string(), "priest".to_string()];
        assert_eq!(preset_file_content(&classes), "warrior\npriest\n");
    }

    #[test]
    fn preset_file_content_empty_is_empty() {
        assert_eq!(preset_file_content(&[]), "");
    }

    #[test]
    fn parse_preset_classes_drops_blank_lines() {
        assert_eq!(
            parse_preset_classes("warrior\n\npriest\n"),
            vec!["warrior".to_string(), "priest".to_string()]
        );
    }

    #[test]
    fn parse_preset_classes_roundtrips_with_preset_file_content() {
        let classes = vec!["warrior".to_string(), "mage".to_string(), "druid".to_string()];
        let content = preset_file_content(&classes);
        assert_eq!(parse_preset_classes(&content), classes);
    }

    // -- parse_import_classes -------------------------------------------------

    #[test]
    fn parse_import_classes_happy_path() {
        assert_eq!(
            parse_import_classes("warrior,priest,mage").unwrap(),
            vec!["warrior".to_string(), "priest".to_string(), "mage".to_string()]
        );
    }

    #[test]
    fn parse_import_classes_rejects_empty() {
        let e = parse_import_classes("").unwrap_err();
        assert_eq!(e.message, "Missing --classes <comma-separated list>");
    }

    #[test]
    fn parse_import_classes_rejects_first_bad_token_before_any_success() {
        let e = parse_import_classes("warrior,deathknight,priest").unwrap_err();
        assert_eq!(e.message, "Invalid class: deathknight");
    }

    // -- preset_path -------------------------------------------------

    #[test]
    fn preset_path_joins_dir_and_name() {
        let dir = std::path::PathBuf::from("/home/x/.dml/party-presets");
        assert_eq!(preset_path(&dir, "myPreset"), dir.join("myPreset"));
    }

    // -- list_presets -------------------------------------------------

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dml-party-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn list_presets_shape_sorted_with_bot_counts() {
        let d = tmp_dir("list");
        std::fs::write(d.join("raiders"), "warrior\npriest\nmage\n").unwrap();
        std::fs::write(d.join("duo"), "warrior\npriest\n").unwrap();
        // Invalid name (would fail _valid_preset_name) -- must be skipped.
        std::fs::write(d.join("bad name"), "warrior\n").unwrap();
        // A subdirectory -- must be skipped ([[ -f "$f" ]] guard).
        std::fs::create_dir_all(d.join("subdir")).unwrap();

        let presets = list_presets(&d);
        assert_eq!(
            presets,
            vec![
                PresetInfo { name: "duo".to_string(), bots: 2 },
                PresetInfo { name: "raiders".to_string(), bots: 3 },
            ]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn list_presets_drops_blank_lines_from_bot_count() {
        let d = tmp_dir("blank-lines");
        std::fs::write(d.join("gappy"), "warrior\n\npriest\n\n").unwrap();
        assert_eq!(list_presets(&d), vec![PresetInfo { name: "gappy".to_string(), bots: 2 }]);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn list_presets_empty_dir_is_empty() {
        let d = tmp_dir("empty");
        assert!(list_presets(&d).is_empty());
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- preset_not_found (Task 13) ---------------------------------------
    //
    // `preset_save`/`preset_list`/`preset_delete` themselves resolve
    // `~/.dml/party-presets` from USERPROFILE/HOME, so exercising them in a
    // library unit test would either touch the developer's real preset dir or
    // race every other test in this binary over a process-wide env var. They
    // are covered instead as SUBPROCESS tests in
    // `crates/dml-wow-cli/tests/cli_integration.rs`, which can give the child
    // its own USERPROFILE safely. Only the pure error constructor is pinned
    // here.
    #[test]
    fn preset_not_found_shape() {
        let e = preset_not_found("raiders");
        assert_eq!(e.code, "NOT_FOUND");
        assert_eq!(e.message, "No preset named raiders");
        assert_eq!(e.hint, "");
    }

    #[test]
    fn list_presets_missing_dir_is_empty() {
        let d = tmp_dir("missing");
        let _ = std::fs::remove_dir_all(&d);
        assert!(list_presets(&d).is_empty());
    }
}
