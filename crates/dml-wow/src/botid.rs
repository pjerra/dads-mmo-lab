//! Bot-vs-human account identification — the ONE place that answers "is this
//! `characters.account` a playerbot?".
//!
//! WHY THIS MODULE EXISTS (incident 2026-08-01). Every bot check in the repo
//! used to ask a single question: is the account in
//! `acore_playerbots.playerbots_account_type` with `account_type IN (1,2)`?
//! That registry is populated by mod-playerbots itself — and on a freshly
//! built install it can be **completely empty** while 1000 bot characters
//! exist and play. Live proof: the `native-test` install had 1000 bot
//! characters across 100 `RNDBOT*` accounts with `playerbots_account_type`,
//! `playerbots_random_bots` and `playerbots_account_links` ALL at zero rows.
//!
//! `account NOT IN (<empty set>)` is TRUE for every row, so the human filter
//! fails **OPEN**: every bot was reported as a real player on the launcher's
//! Home page, while `bots online` (the same predicate, inverted) reported 0.
//! A detector whose failure mode is "everything is a human" is worse than no
//! detector, because nothing about the output looks broken.
//!
//! The fix is a SECOND, independent signal that does not depend on the mod
//! writing a registry row: the account NAME. mod-playerbots creates every
//! account it owns as `<AiPlayerbot.RandomBotAccountPrefix><n>`, and its own
//! conf documents the prefix as reserved:
//!
//! > `# Prefix for created bot accounts (of any type).`
//! > `# Do not change this prefix while there are existing bot accounts.`
//! > `AiPlayerbot.RandomBotAccountPrefix = "rndbot"`
//!
//! An account is therefore a bot when it is registry-tagged **OR**
//! prefix-named. The two signals fail independently: an empty registry is
//! covered by the prefix, and a renamed/re-prefixed account is still covered
//! by the registry.
//!
//! FAIL-SAFE DIRECTION. The prefix arm carries the mirror-image hazard: an
//! EMPTY prefix would compile to `LIKE '%'`, which matches every account and
//! would classify the whole family as bots. [`normalize_prefix`] refuses an
//! empty value and falls back to the documented default, and the `%`/`_`/`\`
//! escaping in [`escape_like_literal`] keeps a custom prefix from turning into
//! a wildcard. Both are pinned by tests below.
//!
//! Mirrored in bash by `_bot_prefix`/`_bot_account_where`/`_human_account_where`
//! (`cli/src/30-db.sh`) — a fix on one surface only half-ships.

use super::config::ConfigReader;

/// The prefix mod-playerbots ships with, used when the conf cannot be read.
pub const DEFAULT_BOT_ACCOUNT_PREFIX: &str = "rndbot";

/// Process-env override (test seam + escape hatch for an exotic install).
pub const BOT_PREFIX_ENV: &str = "DML_BOT_ACCOUNT_PREFIX";

/// The conf route [`resolve_prefix`] reads, in `ConfigReader::compute_value`
/// spec form (which already falls back `.conf` -> `.conf.dist` -> default —
/// the `.dist` leg is load-bearing: a fresh native install has ONLY the
/// `.dist`, since the container writes the live conf inside its own layer).
pub const BOT_PREFIX_CONF: &str = "conf:playerbots.conf:AiPlayerbot.RandomBotAccountPrefix";

/// Trim, strip conf quoting, and refuse an empty result.
///
/// An empty prefix is not "no prefix" — spliced into `LIKE '%'` it would match
/// EVERY account and report the entire family as bots. Falling back to the
/// documented default keeps the failure direction safe.
pub fn normalize_prefix(raw: &str) -> String {
    let t = raw.trim();
    let t = t.strip_prefix('"').unwrap_or(t);
    let t = t.strip_suffix('"').unwrap_or(t);
    let t = t.strip_prefix('\'').unwrap_or(t);
    let t = t.strip_suffix('\'').unwrap_or(t);
    let t = t.trim();
    if t.is_empty() {
        DEFAULT_BOT_ACCOUNT_PREFIX.to_string()
    } else {
        t.to_string()
    }
}

/// Escape a value for splicing into a single-quoted MySQL `LIKE` pattern.
///
/// Backslash FIRST (else the escapes we add would themselves be escaped),
/// then the two LIKE wildcards, then the quote. Without the wildcard half a
/// prefix of `bot_` would match `botX`, and a prefix containing `%` would
/// match everything.
pub fn escape_like_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '%' => out.push_str("\\%"),
            '_' => out.push_str("\\_"),
            '\'' => out.push_str("''"),
            _ => out.push(ch),
        }
    }
    out
}

/// Signal 1: the playerbots registry (`1` = random bot, `2` = addclass bot).
/// Authoritative when populated, silently empty on some installs.
///
/// `playerbots` is the RESOLVED schema name (Task 6) — it must come from a
/// [`crate::db::DatabaseNames`], whose parse gate
/// ([`crate::db::parse_database_info`]) enforces the `[A-Za-z0-9_$]`
/// identifier charset before any value can reach this splice.
pub fn registry_clause(col: &str, playerbots: &str) -> String {
    format!(
        "{col} IN (SELECT account_id FROM {playerbots}.playerbots_account_type \
         WHERE account_type IN (1,2))"
    )
}

/// Signal 2: the reserved account-name prefix. Independent of the registry and
/// of the playerbots schema being populated at all.
///
/// `UPPER(username)` + an upper-cased prefix, because the two sides disagree
/// on case by nature: AzerothCore stores account names upper-cased
/// (`RNDBOT0`), while the conf value that names them is lower-case
/// (`AiPlayerbot.RandomBotAccountPrefix = "rndbot"`). A bare `LIKE 'rndbot%'`
/// happens to work only because the shipped collation is case-INsensitive —
/// this makes the match independent of that. (The `accounts` verb's
/// long-standing `NOT LIKE 'RNDBOT%'` filter is safe for the same reason: its
/// literal is already upper-case.)
///
/// `auth` is the RESOLVED auth-schema name — see [`registry_clause`] for the
/// charset gate that makes it splice-safe.
pub fn prefix_clause(col: &str, prefix: &str, auth: &str) -> String {
    format!(
        "{col} IN (SELECT id FROM {auth}.account WHERE {})",
        username_is_bot("username", prefix)
    )
}

/// The same prefix test applied DIRECTLY to a username column, for queries
/// that already select from `acore_auth.account` and need no subselect (the
/// `accounts` picker). Split out so the pattern itself — upper-casing,
/// escaping, empty-prefix fallback — has exactly ONE definition.
///
/// NB the `accounts` verb deliberately uses THIS and not [`bot_clause`]: it is
/// the one bot filter that does not require the playerbots schema to
/// exist, and the launcher's character picker should not start erroring on a
/// box without the playerbots module (the same concern that made the stats
/// envelope probe for that schema).
pub fn username_is_bot(col: &str, prefix: &str) -> String {
    let p = escape_like_literal(&normalize_prefix(prefix).to_ascii_uppercase());
    format!("UPPER({col}) LIKE '{p}%'")
}

/// "This account is a bot" — either signal is enough. Always parenthesised so
/// it can be `AND`-ed into a larger WHERE without changing its meaning.
///
/// `playerbots: None` means the server has no playerbots schema (a legal
/// state since Task 6 resolves the name instead of assuming
/// `acore_playerbots`) — the clause degrades to
/// [`bot_clause_prefix_only`], EXACTLY the 2026-08-01 incident posture: the
/// prefix signal keeps finding bots, and the missing registry never reads as
/// "this box has no bots".
pub fn bot_clause(col: &str, prefix: &str, auth: &str, playerbots: Option<&str>) -> String {
    match playerbots {
        Some(pb) => format!("({} OR {})", registry_clause(col, pb), prefix_clause(col, prefix, auth)),
        None => bot_clause_prefix_only(col, prefix, auth),
    }
}

/// "This account is a real player" — the exact negation of [`bot_clause`], so
/// the two can never drift apart and double-count or double-drop a character.
/// (That includes the degrade: with `playerbots: None` this is the negation
/// of the prefix-only form.)
pub fn human_clause(col: &str, prefix: &str, auth: &str, playerbots: Option<&str>) -> String {
    format!("NOT {}", bot_clause(col, prefix, auth, playerbots))
}

/// The bot half when the playerbots schema is unusable (stats' probe) or not
/// configured at all — the prefix signal alone. Previously this degraded to a
/// constant `0=1`, i.e. "there are no bots", which is a lie on any box that
/// has them.
pub fn bot_clause_prefix_only(col: &str, prefix: &str, auth: &str) -> String {
    format!("({})", prefix_clause(col, prefix, auth))
}

/// Live prefix for this install: process env override, else the title's
/// `playerbots.conf`/`.dist`, else the shipped default.
pub fn resolve_prefix(reader: &mut ConfigReader) -> String {
    // `filter(!is_empty)` and not a bare `is_some`: an env var set to empty
    // means "unset" here, so the conf still gets its turn — the same reading
    // bash's `[[ -n "${DML_BOT_ACCOUNT_PREFIX-}" ]]` gives it.
    if let Some(v) = std::env::var_os(BOT_PREFIX_ENV).filter(|s| !s.is_empty()) {
        return normalize_prefix(&v.to_string_lossy());
    }
    normalize_prefix(&reader.compute_value(
        "bots.account_prefix",
        BOT_PREFIX_CONF,
        DEFAULT_BOT_ACCOUNT_PREFIX,
    ))
}

/// [`resolve_prefix`] against the ambient title dir (`DML_GAMES_DIR`), for the
/// read paths that hold only a `DbConfig` and no reader of their own.
pub fn bot_account_prefix() -> String {
    let mut reader = ConfigReader::from_env();
    resolve_prefix(&mut reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DELIBERATE update (Task 6): the schema qualifiers are now the RESOLVED
    /// names the caller passes, so the pins assert them through the
    /// parameters — with renamed values, so a builder that quietly fell back
    /// to `acore_*` goes red here.
    #[test]
    fn bot_clause_accepts_either_signal() {
        let c = bot_clause("c.account", "rndbot", "my_auth", Some("my_pb"));
        assert!(
            c.contains(
                "c.account IN (SELECT account_id FROM my_pb.playerbots_account_type WHERE account_type IN (1,2))"
            ),
            "registry signal missing: {c}"
        );
        assert!(
            c.contains("c.account IN (SELECT id FROM my_auth.account WHERE UPPER(username) LIKE 'RNDBOT%')"),
            "prefix signal missing: {c}"
        );
        assert!(c.contains(" OR "), "the two signals must be OR-ed, not AND-ed: {c}");
        // Parenthesised, so `... AND <bot_clause>` cannot re-associate.
        assert!(c.starts_with('(') && c.ends_with(')'), "got: {c}");
        assert!(!c.contains("acore_"), "a resolved clause must not fall back to acore_*: {c}");
    }

    #[test]
    fn human_clause_is_exactly_the_negation_of_bot_clause() {
        let bot = bot_clause("c.account", "rndbot", "my_auth", Some("my_pb"));
        let human = human_clause("c.account", "rndbot", "my_auth", Some("my_pb"));
        assert_eq!(human, format!("NOT {bot}"));
        // The invariant survives the degrade too — one definition, negated.
        let bot = bot_clause("c.account", "rndbot", "my_auth", None);
        let human = human_clause("c.account", "rndbot", "my_auth", None);
        assert_eq!(human, format!("NOT {bot}"));
    }

    /// The regression this module exists for: with an EMPTY registry the human
    /// filter must still exclude bots. Asserted on the SQL rather than a live
    /// DB — the prefix arm is what survives an empty subselect, so its presence
    /// in the human clause IS the fix.
    #[test]
    fn human_clause_does_not_rest_on_the_registry_alone() {
        let human = human_clause("c.account", "rndbot", "my_auth", Some("my_pb"));
        assert!(
            human.contains("my_auth.account WHERE UPPER(username) LIKE 'RNDBOT%'"),
            "an empty playerbots registry would classify every bot as human: {human}"
        );
    }

    /// The Task 6 degrade, ISOLATED: a `None` playerbots name produces the
    /// prefix-only form — byte-identical to [`bot_clause_prefix_only`] — and
    /// never mentions the registry table (there is no schema to qualify it
    /// with, and a guessed `acore_playerbots` would be the exact fallback
    /// this branch removed).
    #[test]
    fn a_missing_playerbots_name_degrades_to_the_prefix_only_form() {
        let degraded = bot_clause("c.account", "rndbot", "my_auth", None);
        assert_eq!(degraded, bot_clause_prefix_only("c.account", "rndbot", "my_auth"));
        assert!(!degraded.contains("playerbots_account_type"), "got: {degraded}");
        assert!(!degraded.contains("acore_"), "got: {degraded}");
        assert!(degraded.contains("LIKE 'RNDBOT%'"), "the prefix signal must survive: {degraded}");
    }

    #[test]
    fn empty_or_blank_prefix_falls_back_to_the_default() {
        assert_eq!(normalize_prefix(""), "rndbot");
        assert_eq!(normalize_prefix("   "), "rndbot");
        assert_eq!(normalize_prefix("\"\""), "rndbot");
        // The hazard this guards: LIKE '%' matches every account.
        let c = prefix_clause("c.account", "", "acore_auth");
        assert!(!c.contains("LIKE '%'"), "empty prefix became a match-all: {c}");
        assert!(c.contains("LIKE 'RNDBOT%'"), "got: {c}");
    }

    #[test]
    fn conf_quoting_and_whitespace_are_stripped() {
        assert_eq!(normalize_prefix("\"rndbot\""), "rndbot");
        assert_eq!(normalize_prefix("'rndbot'"), "rndbot");
        assert_eq!(normalize_prefix("  rndbot  "), "rndbot");
    }

    #[test]
    fn like_wildcards_in_a_custom_prefix_are_escaped() {
        // `_` matches any single char and `%` any run — an unescaped custom
        // prefix would silently widen the bot set.
        assert_eq!(escape_like_literal("bot_%x"), "bot\\_\\%x");
        assert_eq!(escape_like_literal("a\\b"), "a\\\\b");
        let c = prefix_clause("c.account", "my_bots", "acore_auth");
        assert!(c.contains("LIKE 'MY\\_BOTS%'"), "got: {c}");
    }

    #[test]
    fn a_quote_in_the_prefix_cannot_break_out_of_the_literal() {
        let c = prefix_clause("c.account", "o'brien", "acore_auth");
        assert!(c.contains("LIKE 'O''BRIEN%'"), "got: {c}");
        // One opening + one closing quote around the pattern, nothing loose.
        assert_eq!(c.matches('\'').count() % 2, 0, "unbalanced quoting: {c}");
    }

    #[test]
    fn prefix_only_degrade_still_finds_bots() {
        // The stats probe's fallback used to be `0=1` — "no bots exist".
        let c = bot_clause_prefix_only("c.account", "rndbot", "acore_auth");
        assert!(c.contains("LIKE 'RNDBOT%'"), "got: {c}");
        assert!(!c.contains("playerbots_account_type"), "got: {c}");
    }

    /// Both env cases in ONE test on purpose: cargo runs a module's tests as
    /// parallel threads of a single process, so two tests mutating the same
    /// process env race (they did — the override leaked into the default
    /// case). Set and cleared inside one thread, the sequence is honest.
    #[test]
    fn env_override_beats_the_conf_and_absence_yields_the_default() {
        let title = std::env::temp_dir().join("dml-botid-no-such-title");

        std::env::set_var(BOT_PREFIX_ENV, "\"zzbot\"");
        let mut r = ConfigReader::for_title(&title);
        assert_eq!(resolve_prefix(&mut r), "zzbot");

        // An override set to empty must NOT disarm the filter (the `LIKE '%'`
        // hazard) — it falls back like any other blank value.
        std::env::set_var(BOT_PREFIX_ENV, "");
        let mut r = ConfigReader::for_title(&title);
        assert_eq!(resolve_prefix(&mut r), DEFAULT_BOT_ACCOUNT_PREFIX);

        std::env::remove_var(BOT_PREFIX_ENV);
        let mut r = ConfigReader::for_title(&title);
        assert_eq!(resolve_prefix(&mut r), DEFAULT_BOT_ACCOUNT_PREFIX);
    }
}
