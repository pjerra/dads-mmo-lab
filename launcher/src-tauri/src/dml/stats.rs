//! Native-mode direct-MySQL reader for the STATISTICS page (Task 4, spike:
//! `spike/docker-desktop-native`).
//!
//! WHY. In WSL mode the Statistics page is served by shelling `dml wow stats`,
//! whose `_stats_payload` (cli/src/48-stats.sh) runs NINETEEN queries across
//! four schemas, each a `docker exec ac-database mysql …`. On Windows-native
//! that is ~19 × (bash + docker-exec) round-trips plus a per-row fork storm for
//! the loop-assembled arrays — multiple seconds per page load. In NATIVE mode
//! Docker Desktop publishes the same MySQL on `127.0.0.1:<port>`, so this reader
//! opens DIRECT connections (via [`super::db`]) and assembles the SAME envelope
//! `dml wow stats` emits — and it runs the 18 independent post-probe queries
//! CONCURRENTLY (one connection each), so the whole page is one fast batch.
//!
//! FAITHFUL PORT. Every query, the fixed query order, and the entire nested JSON
//! shape mirror `_stats_payload` line for line (see 48-stats.sh). The bash reads
//! `mysql -N -B` TEXT and funnels every interpolated number through `_stats_num`
//! (`10#`-normalized non-negative int, else 0) and every 0/1 flag through
//! `_stats_bool`; [`stats_num`]/[`stats_bool`] reproduce those exactly. Because
//! EVERY stat number is a non-negative integer (or the single sentinel `-1`
//! continent bucket), there are no floats here — the `CAST`-to-text float dance
//! the teleport reader needs does not apply.
//!
//! BOT DETECTION. Query 1 probes `acore_playerbots.playerbots_account_type`; a
//! box without that schema degrades the `bot` predicate to a constant `0=1`
//! (every bot stat becomes zero) instead of failing the whole envelope — exactly
//! the bash's "only tolerated failure". A fully-down DB fails the probe too, and
//! then also fails query 2, so the envelope still errors (mapped to
//! `DB_UNREACHABLE` by the caller, as the CLI's `stats` arm does).
//!
//! A cargo parity test (`stats_parity.rs`, DB-gated) deep-equals this reader
//! against a live `dml wow stats --json`. Native-mode-only by convention: WSL
//! keeps calling `dml`; the caller gates on `backend_mode()`.

use serde_json::{json, Value};

use super::db::{self, Database, DbConfig, DbError, QueryResult};
use super::pages::cell_text;

/// Query 1: the playerbots-schema probe. When this errors (schema absent OR DB
/// down) the `bot` predicate becomes `0=1`. Mirrors 48-stats.sh:67.
pub const PROBE_SQL: &str = "SELECT 1 FROM acore_playerbots.playerbots_account_type LIMIT 1;";

/// The authoritative bot-account predicate (account_type 1|2), used verbatim by
/// `_bots_counts` / `players online` / the stats envelope (48-stats.sh:55).
pub const BOT_SUBQUERY: &str = "c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))";

/// System accounts that are neither family nor ambient bots (48-stats.sh:58).
pub const SYS_SUBQUERY: &str = "c.account IN (SELECT id FROM acore_auth.account WHERE username IN ('AHBOT','DMLSOAP'))";

// Fixed positions of the 18 post-probe queries in the concurrent batch — the
// same order as 48-stats.sh's `-- 2` … `-- 19` (the probe is `-- 1`, run first
// and separately because it decides the `bot` predicate the rest embed).
const Q_POP: usize = 0; // 2  population overview
const Q_LEVELS: usize = 1; // 3  level buckets
const Q_CLASSES: usize = 2; // 4  class breakdown f/b
const Q_FACTIONS: usize = 3; // 5  faction split f/b
const Q_TOP_FAM: usize = 4; // 6  top-5 levels FAMILY
const Q_TOP_BOT: usize = 5; // 7  top-5 levels BOTS
const Q_GUILDS: usize = 6; // 8  guild count + members
const Q_COPPER: usize = 7; // 9  copper totals
const Q_RICH_FAM: usize = 8; // 10 top-5 richest FAMILY
const Q_RICH_BOT: usize = 9; // 11 top-5 richest BOTS
const Q_AUCTION: usize = 10; // 12 auction house
const Q_MAIL: usize = 11; // 13 mail counts
const Q_JOURNEY: usize = 12; // 14 family journey rows
const Q_UPTIME: usize = 13; // 15 uptime aggregates (auth)
const Q_REALM: usize = 14; // 16 realm name (auth)
const Q_RECENT: usize = 15; // 17 last-15 boots (auth)
const Q_ZONES: usize = 16; // 18 online-bot zones
const Q_CONTS: usize = 17; // 19 online-bot continents
const N_QUERIES: usize = 18;

// ---------------------------------------------------------------------------
// Number / bool fidelity (ports of _stats_num / _stats_bool, 48-stats.sh:34).
// ---------------------------------------------------------------------------

/// The value `_stats_num` would emit: the `10#`-normalized integer when the
/// text is one-or-more ASCII digits, else 0. Reproduces the bash guard
/// `[[ "$v" =~ ^[0-9]+$ ]] && printf '%s' "$((10#$v))"` — a NULL/empty/garbage
/// mysql field degrades to 0, and leading zeros are stripped by the parse
/// (`"007" -> 7`). Every interpolated number in the stats envelope funnels
/// through this.
pub fn stats_num(s: &str) -> i64 {
    if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) {
        s.parse::<i64>().unwrap_or(0)
    } else {
        0
    }
}

/// [`stats_num`] as a JSON number (always a non-negative integer).
fn num(res: &QueryResult, row: usize, col: usize) -> Value {
    json!(stats_num(&cellt(res, row, col)))
}

/// The JSON bool `_stats_bool` would emit: `true` only for the exact text `"1"`.
pub fn stats_bool(s: &str) -> bool {
    s == "1"
}

/// The text a cell would have on `mysql -N -B` stdin, or `""` when the row/col
/// is absent (the bash sees an empty field and `_stats_num`s it to 0).
fn cellt(res: &QueryResult, row: usize, col: usize) -> String {
    res.rows
        .get(row)
        .and_then(|r| r.get(col))
        .map(cell_text)
        .unwrap_or_default()
}

/// `true` when `s` is one-or-more ASCII digits — the bash `^[0-9]+$` guard used
/// to drop rows whose key column is non-numeric.
fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// SQL builders (pure) — one per query, given the resolved predicates.
// ---------------------------------------------------------------------------

/// Build the 18 post-probe `(schema, sql)` pairs in `Q_*` order, embedding the
/// resolved `bot` predicate (either [`BOT_SUBQUERY`] or `0=1`) and `fam`/`sys`.
pub fn build_queries(bot: &str, sys: &str) -> Vec<(Database, String)> {
    let fam = format!("NOT ({bot}) AND NOT ({sys})");
    let mut q = vec![(Database::Characters, String::new()); N_QUERIES];

    // 2: population overview — five numbers, one full-table pass.
    q[Q_POP] = (
        Database::Characters,
        format!(
            "SELECT COALESCE(SUM(CASE WHEN {fam} THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN ({fam}) AND c.online=1 THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN {bot} THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN ({bot}) AND c.online=1 THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN {bot} THEN c.totaltime ELSE 0 END),0) \
             FROM characters c;"
        ),
    );

    // 3: level spread bucketed by 10, family/bot split per bucket.
    q[Q_LEVELS] = (
        Database::Characters,
        format!(
            "SELECT FLOOR((GREATEST(c.level,1)-1)/10), \
             COALESCE(SUM(CASE WHEN {fam} THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN {bot} THEN 1 ELSE 0 END),0) \
             FROM characters c GROUP BY 1 ORDER BY 1;"
        ),
    );

    // 4: class breakdown, family/bots split per class (system excluded).
    q[Q_CLASSES] = (
        Database::Characters,
        format!(
            "SELECT c.class, \
             COALESCE(SUM(CASE WHEN {fam} THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN {bot} THEN 1 ELSE 0 END),0) \
             FROM characters c WHERE NOT ({sys}) GROUP BY c.class ORDER BY c.class;"
        ),
    );

    // 5: faction split per segment (race -> faction mapping).
    q[Q_FACTIONS] = (
        Database::Characters,
        format!(
            "SELECT COALESCE(SUM(CASE WHEN ({fam}) AND c.race IN (1,3,4,7,11) THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN ({fam}) AND c.race IN (2,5,6,8,10) THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN ({bot}) AND c.race IN (1,3,4,7,11) THEN 1 ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN ({bot}) AND c.race IN (2,5,6,8,10) THEN 1 ELSE 0 END),0) \
             FROM characters c;"
        ),
    );

    // 6+7: top-5 highest-level characters per segment.
    q[Q_TOP_FAM] = (
        Database::Characters,
        format!(
            "SELECT c.name, c.level FROM characters c WHERE {fam} \
             ORDER BY c.level DESC, c.totaltime DESC, c.name LIMIT 5;"
        ),
    );
    q[Q_TOP_BOT] = (
        Database::Characters,
        format!(
            "SELECT c.name, c.level FROM characters c WHERE {bot} \
             ORDER BY c.level DESC, c.totaltime DESC, c.name LIMIT 5;"
        ),
    );

    // 8: guild count + member total.
    q[Q_GUILDS] = (
        Database::Characters,
        "SELECT (SELECT COUNT(*) FROM guild), (SELECT COUNT(*) FROM guild_member);".to_string(),
    );

    // 9: copper totals (total = family+bots; system excluded).
    q[Q_COPPER] = (
        Database::Characters,
        format!(
            "SELECT COALESCE(SUM(CASE WHEN NOT ({sys}) THEN c.money ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN {fam} THEN c.money ELSE 0 END),0), \
             COALESCE(SUM(CASE WHEN {bot} THEN c.money ELSE 0 END),0) \
             FROM characters c;"
        ),
    );

    // 10+11: top-5 richest characters per segment.
    q[Q_RICH_FAM] = (
        Database::Characters,
        format!(
            "SELECT c.name, c.money FROM characters c WHERE {fam} \
             ORDER BY c.money DESC, c.name LIMIT 5;"
        ),
    );
    q[Q_RICH_BOT] = (
        Database::Characters,
        format!(
            "SELECT c.name, c.money FROM characters c WHERE {bot} \
             ORDER BY c.money DESC, c.name LIMIT 5;"
        ),
    );

    // 12: auction house stock.
    q[Q_AUCTION] = (
        Database::Characters,
        "SELECT COUNT(*), COALESCE(SUM(buyoutprice),0) FROM auctionhouse;".to_string(),
    );

    // 13: pending mail (+ how much is addressed to the family).
    q[Q_MAIL] = (
        Database::Characters,
        format!(
            "SELECT COUNT(*), \
             COALESCE(SUM(CASE WHEN m.receiver IN (SELECT c.guid FROM characters c WHERE {fam}) THEN 1 ELSE 0 END),0) \
             FROM mail m;"
        ),
    );

    // 14: the family's journey — one row per family character.
    q[Q_JOURNEY] = (
        Database::Characters,
        format!(
            "SELECT c.name, c.level, c.class, c.totaltime, c.logout_time, c.online, c.totalKills, \
             (SELECT COUNT(*) FROM character_achievement a WHERE a.guid = c.guid), \
             (SELECT COUNT(*) FROM character_queststatus_rewarded q WHERE q.guid = c.guid) \
             FROM characters c WHERE {fam} ORDER BY c.totaltime DESC, c.name;"
        ),
    );

    // 15: server history aggregates (acore_auth.uptime).
    q[Q_UPTIME] = (
        Database::Auth,
        "SELECT COUNT(*), COALESCE(SUM(uptime),0), COALESCE(MAX(uptime),0), COALESCE(MAX(maxplayers),0) FROM uptime;".to_string(),
    );

    // 16: realm name.
    q[Q_REALM] = (
        Database::Auth,
        "SELECT name FROM realmlist ORDER BY id LIMIT 1;".to_string(),
    );

    // 17: the last 15 boots, oldest first.
    q[Q_RECENT] = (
        Database::Auth,
        "SELECT starttime, uptime FROM (SELECT starttime, uptime FROM uptime ORDER BY starttime DESC LIMIT 15) t ORDER BY starttime;".to_string(),
    );

    // 18: top-8 busiest zones for ONLINE bots.
    q[Q_ZONES] = (
        Database::Characters,
        format!(
            "SELECT c.zone, COUNT(*) FROM characters c WHERE c.online = 1 AND ({bot}) \
             GROUP BY c.zone ORDER BY COUNT(*) DESC, c.zone LIMIT 8;"
        ),
    );

    // 19: online bots per continent, bounded to four maps + one "other" (-1).
    q[Q_CONTS] = (
        Database::Characters,
        format!(
            "SELECT CASE WHEN c.map IN (0,1,530,571) THEN c.map ELSE -1 END, COUNT(*) \
             FROM characters c WHERE c.online = 1 AND ({bot}) GROUP BY 1 ORDER BY 2 DESC, 1;"
        ),
    );

    q
}

// ---------------------------------------------------------------------------
// Assembly (pure) — the exact nested envelope of the `printf` at 48-stats.sh:308.
// ---------------------------------------------------------------------------

/// Assemble the full stats `data` payload from the 18 post-probe result sets
/// (indexed by `Q_*`). A port of the `_stats_payload` assembly: each array is
/// built with the same row guards (skip empty/non-numeric keys), each number
/// via [`stats_num`], each flag via [`stats_bool`]. Panics only if `res` is
/// shorter than [`N_QUERIES`] (an internal contract, upheld by [`read_stats`]).
pub fn assemble_stats(res: &[QueryResult]) -> Value {
    // -- population -------------------------------------------------------
    let pop = &res[Q_POP];
    let fam_total = num(pop, 0, 0);
    let fam_online = num(pop, 0, 1);
    let bot_total = num(pop, 0, 2);
    let bot_online = num(pop, 0, 3);
    let bot_playtime = num(pop, 0, 4);

    // -- levels: skip empty / non-numeric bucket keys ---------------------
    let mut levels = Vec::new();
    for row_i in 0..res[Q_LEVELS].rows.len() {
        let b = cellt(&res[Q_LEVELS], row_i, 0);
        if b.is_empty() || !is_digits(&b) {
            continue;
        }
        levels.push(json!({
            "bucket": stats_num(&b),
            "family": num(&res[Q_LEVELS], row_i, 1),
            "bots": num(&res[Q_LEVELS], row_i, 2),
        }));
    }

    // -- classes: drop zero-count entries per segment ---------------------
    let (mut cls_fam, mut cls_bot) = (Vec::new(), Vec::new());
    for row_i in 0..res[Q_CLASSES].rows.len() {
        let cls = cellt(&res[Q_CLASSES], row_i, 0);
        if cls.is_empty() || !is_digits(&cls) {
            continue;
        }
        let fv = stats_num(&cellt(&res[Q_CLASSES], row_i, 1));
        let bv = stats_num(&cellt(&res[Q_CLASSES], row_i, 2));
        if fv > 0 {
            cls_fam.push(json!({ "class": stats_num(&cls), "count": fv }));
        }
        if bv > 0 {
            cls_bot.push(json!({ "class": stats_num(&cls), "count": bv }));
        }
    }

    // -- factions ---------------------------------------------------------
    let fac = &res[Q_FACTIONS];
    let factions = json!({
        "family": { "alliance": num(fac, 0, 0), "horde": num(fac, 0, 1) },
        "bots": { "alliance": num(fac, 0, 2), "horde": num(fac, 0, 3) },
    });

    // -- top levels (family flag by construction) -------------------------
    let tops_fam = top_levels(&res[Q_TOP_FAM], true);
    let tops_bot = top_levels(&res[Q_TOP_BOT], false);

    // -- guilds -----------------------------------------------------------
    let guilds = json!({ "count": num(&res[Q_GUILDS], 0, 0), "members": num(&res[Q_GUILDS], 0, 1) });

    // -- copper -----------------------------------------------------------
    let cop = &res[Q_COPPER];
    let copper = json!({ "total": num(cop, 0, 0), "family": num(cop, 0, 1), "bots": num(cop, 0, 2) });

    // -- richest ----------------------------------------------------------
    let rich_fam = richest(&res[Q_RICH_FAM], true);
    let rich_bot = richest(&res[Q_RICH_BOT], false);

    // -- auction / mail ---------------------------------------------------
    let auction = json!({ "count": num(&res[Q_AUCTION], 0, 0), "buyout": num(&res[Q_AUCTION], 0, 1) });
    let mail = json!({ "total": num(&res[Q_MAIL], 0, 0), "to_family": num(&res[Q_MAIL], 0, 1) });

    // -- journey ----------------------------------------------------------
    let mut journey = Vec::new();
    for row_i in 0..res[Q_JOURNEY].rows.len() {
        let name = cellt(&res[Q_JOURNEY], row_i, 0);
        if name.is_empty() {
            continue;
        }
        journey.push(json!({
            "name": name,
            "level": num(&res[Q_JOURNEY], row_i, 1),
            "class": num(&res[Q_JOURNEY], row_i, 2),
            "playtime": num(&res[Q_JOURNEY], row_i, 3),
            "last_seen": num(&res[Q_JOURNEY], row_i, 4),
            "online": stats_bool(&cellt(&res[Q_JOURNEY], row_i, 5)),
            "kills": num(&res[Q_JOURNEY], row_i, 6),
            "achievements": num(&res[Q_JOURNEY], row_i, 7),
            "quests": num(&res[Q_JOURNEY], row_i, 8),
        }));
    }

    // -- history ----------------------------------------------------------
    let up = &res[Q_UPTIME];
    // realm: first row's name, or "" when realmlist is empty.
    let realm = cellt(&res[Q_REALM], 0, 0);
    let mut recent = Vec::new();
    for row_i in 0..res[Q_RECENT].rows.len() {
        let start = cellt(&res[Q_RECENT], row_i, 0);
        if start.is_empty() || !is_digits(&start) {
            continue;
        }
        recent.push(json!({ "start": stats_num(&start), "uptime": num(&res[Q_RECENT], row_i, 1) }));
    }
    let history = json!({
        "boots": num(up, 0, 0),
        "total_uptime": num(up, 0, 1),
        "longest": num(up, 0, 2),
        "peak": num(up, 0, 3),
        "realm": realm,
        "recent": recent,
    });

    // -- botwatch: zones + continents -------------------------------------
    let mut zones = Vec::new();
    for row_i in 0..res[Q_ZONES].rows.len() {
        let zone = cellt(&res[Q_ZONES], row_i, 0);
        if zone.is_empty() || !is_digits(&zone) {
            continue;
        }
        zones.push(json!({ "zone": stats_num(&zone), "count": num(&res[Q_ZONES], row_i, 1) }));
    }
    let mut conts = Vec::new();
    for row_i in 0..res[Q_CONTS].rows.len() {
        let cmap = cellt(&res[Q_CONTS], row_i, 0);
        if cmap.is_empty() {
            continue;
        }
        // Bash gate: ^-?[0-9]+$ ; the only legal negative is the -1 "other" bucket.
        let map_val = if let Some(rest) = cmap.strip_prefix('-') {
            if rest.is_empty() || !is_digits(rest) {
                continue;
            }
            -1i64
        } else if is_digits(&cmap) {
            stats_num(&cmap)
        } else {
            continue;
        };
        conts.push(json!({ "map": map_val, "count": num(&res[Q_CONTS], row_i, 1) }));
    }

    json!({
        "population": {
            "family": { "total": fam_total, "online": fam_online },
            "bots": { "total": bot_total, "online": bot_online },
            "levels": levels,
            "classes": { "family": cls_fam, "bots": cls_bot },
            "factions": factions,
            "top_levels": { "family": tops_fam, "bots": tops_bot },
            "guilds": guilds,
        },
        "economy": {
            "copper": copper,
            "richest": { "family": rich_fam, "bots": rich_bot },
            "auction": auction,
            "mail": mail,
        },
        "journey": journey,
        "history": history,
        "botwatch": { "zones": zones, "continents": conts, "playtime": bot_playtime },
    })
}

/// `[{name,level,family}]` for a top-levels result set (name skip-empty guard).
fn top_levels(res: &QueryResult, family: bool) -> Vec<Value> {
    let mut out = Vec::new();
    for row_i in 0..res.rows.len() {
        let name = cellt(res, row_i, 0);
        if name.is_empty() {
            continue;
        }
        out.push(json!({ "name": name, "level": num(res, row_i, 1), "family": family }));
    }
    out
}

/// `[{name,copper,family}]` for a richest result set (name skip-empty guard).
fn richest(res: &QueryResult, family: bool) -> Vec<Value> {
    let mut out = Vec::new();
    for row_i in 0..res.rows.len() {
        let name = cellt(res, row_i, 0);
        if name.is_empty() {
            continue;
        }
        out.push(json!({ "name": name, "copper": num(res, row_i, 1), "family": family }));
    }
    out
}

// ---------------------------------------------------------------------------
// Execution.
// ---------------------------------------------------------------------------

/// Run every query and assemble the CLI-identical stats envelope.
///
/// Order matches `_stats_payload`: probe first (it decides the `bot` predicate),
/// then the 18 independent queries run CONCURRENTLY — each on its own
/// connection — because they never depend on one another. Any query failure
/// aborts with that [`DbError`] (the caller maps it to `DB_UNREACHABLE`, as the
/// CLI's `stats` arm does); an empty result set is NOT a failure (a fresh DB
/// answers with zeros / empty arrays).
pub fn read_stats(cfg: &DbConfig) -> Result<Value, DbError> {
    // Query 1: probe. Any error (schema absent OR DB down) -> `0=1`, exactly
    // like the bash. A fully-down DB then also fails query 2 below, so the
    // envelope still errors.
    let bot = if db::query(cfg, Database::Characters, PROBE_SQL).is_err() {
        "0=1"
    } else {
        BOT_SUBQUERY
    };
    let queries = build_queries(bot, SYS_SUBQUERY);
    let results = run_concurrent(cfg, &queries)?;
    Ok(assemble_stats(&results))
}

/// Run `queries` concurrently (one thread + connection each) and collect the
/// result sets in the SAME order. Returns the first error encountered (thread
/// order), or all result sets on success. Uses a scoped thread pool so the
/// borrowed `cfg`/`queries` need no cloning.
fn run_concurrent(
    cfg: &DbConfig,
    queries: &[(Database, String)],
) -> Result<Vec<QueryResult>, DbError> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = queries
            .iter()
            .map(|(dbn, sql)| scope.spawn(move || db::query(cfg, *dbn, sql)))
            .collect();
        let mut out = Vec::with_capacity(handles.len());
        for h in handles {
            let r = h
                .join()
                .map_err(|_| DbError::Query("stats query thread panicked".to_string()))?;
            out.push(r?);
        }
        Ok(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dml::db::SqlValue;

    fn t(s: &str) -> SqlValue {
        SqlValue::Text(s.to_string())
    }
    fn i(n: i64) -> SqlValue {
        SqlValue::Int(n)
    }
    fn qr(rows: Vec<Vec<SqlValue>>) -> QueryResult {
        QueryResult { columns: vec![], rows }
    }

    #[test]
    fn stats_num_normalizes_like_bash() {
        assert_eq!(stats_num("2500"), 2500);
        assert_eq!(stats_num("007"), 7); // 10# strips leading zeros
        assert_eq!(stats_num("0"), 0);
        assert_eq!(stats_num(""), 0);
        assert_eq!(stats_num("NULL"), 0);
        assert_eq!(stats_num("-1"), 0); // sign is not a digit -> 0
        assert_eq!(stats_num("12x"), 0);
    }

    #[test]
    fn stats_bool_only_one_is_true() {
        assert!(stats_bool("1"));
        assert!(!stats_bool("0"));
        assert!(!stats_bool(""));
        assert!(!stats_bool("true"));
    }

    #[test]
    fn build_queries_embeds_predicates_in_order() {
        let q = build_queries(BOT_SUBQUERY, SYS_SUBQUERY);
        assert_eq!(q.len(), N_QUERIES);
        // fam = NOT (bot) AND NOT (sys) appears in the population query.
        assert!(q[Q_POP].1.contains("NOT (c.account IN (SELECT account_id"));
        assert!(q[Q_POP].1.contains("FROM characters c;"));
        // The 0=1 degrade path threads through where `bot` is used.
        let qd = build_queries("0=1", SYS_SUBQUERY);
        assert!(qd[Q_ZONES].1.contains("AND (0=1)"));
        // Auth-schema queries are routed to Auth.
        assert_eq!(q[Q_UPTIME].0, Database::Auth);
        assert_eq!(q[Q_REALM].0, Database::Auth);
        assert_eq!(q[Q_RECENT].0, Database::Auth);
        assert_eq!(q[Q_POP].0, Database::Characters);
        // Faithful fragments.
        assert!(q[Q_LEVELS].1.contains("FLOOR((GREATEST(c.level,1)-1)/10)"));
        assert!(q[Q_CONTS].1.contains("CASE WHEN c.map IN (0,1,530,571) THEN c.map ELSE -1 END"));
        assert!(q[Q_TOP_FAM].1.contains("ORDER BY c.level DESC, c.totaltime DESC, c.name LIMIT 5"));
    }

    /// Build a full 18-element result vector with sensible fakes, then assert
    /// the assembled envelope matches the exact nested shape byte-for-byte.
    #[test]
    fn assemble_stats_builds_the_full_envelope() {
        let mut res = vec![qr(vec![]); N_QUERIES];
        // 2 population: fam_total, fam_online, bot_total, bot_online, playtime
        res[Q_POP] = qr(vec![vec![t("3"), i(1), t("2500"), t("40"), t("999999")]]);
        // 3 levels: bucket, family, bots (one empty + one non-numeric dropped)
        res[Q_LEVELS] = qr(vec![
            vec![t("0"), t("2"), t("100")],
            vec![t(""), t("9"), t("9")],   // empty bucket -> skip
            vec![t("x"), t("9"), t("9")],  // non-numeric -> skip
            vec![t("7"), t("1"), t("5")],
        ]);
        // 4 classes: class, fam, bot (zero-count per segment dropped)
        res[Q_CLASSES] = qr(vec![
            vec![t("1"), t("1"), t("50")],
            vec![t("2"), t("0"), t("30")], // fam 0 -> only bot side
            vec![t("8"), t("2"), t("0")],  // bot 0 -> only fam side
        ]);
        // 5 factions: fam_a, fam_h, bot_a, bot_h
        res[Q_FACTIONS] = qr(vec![vec![t("2"), t("1"), t("1200"), t("1300")]]);
        // 6/7 top levels
        res[Q_TOP_FAM] = qr(vec![vec![t("Hypeer"), i(80)], vec![t(""), i(1)]]);
        res[Q_TOP_BOT] = qr(vec![vec![t("Botto"), i(80)]]);
        // 8 guilds
        res[Q_GUILDS] = qr(vec![vec![i(4), i(37)]]);
        // 9 copper
        res[Q_COPPER] = qr(vec![vec![t("10000"), t("6000"), t("4000")]]);
        // 10/11 richest
        res[Q_RICH_FAM] = qr(vec![vec![t("Rich"), t("5000")]]);
        res[Q_RICH_BOT] = qr(vec![vec![t("Botrich"), t("3000")]]);
        // 12 auction
        res[Q_AUCTION] = qr(vec![vec![i(12), t("120000")]]);
        // 13 mail
        res[Q_MAIL] = qr(vec![vec![i(5), t("2")]]);
        // 14 journey
        res[Q_JOURNEY] = qr(vec![vec![
            t("Hypeer"), i(80), i(2), t("36000"), t("1700000000"), i(1), i(500), i(120), i(300),
        ]]);
        // 15 uptime
        res[Q_UPTIME] = qr(vec![vec![i(15), t("864000"), t("86400"), i(42)]]);
        // 16 realm
        res[Q_REALM] = qr(vec![vec![t("DML")]]);
        // 17 recent (empty start dropped)
        res[Q_RECENT] = qr(vec![vec![t("1699990000"), t("3600")], vec![t(""), t("1")]]);
        // 18 zones
        res[Q_ZONES] = qr(vec![vec![i(85), i(30)], vec![i(12), i(10)]]);
        // 19 continents: 0, then the -1 "other" bucket
        res[Q_CONTS] = qr(vec![vec![i(1), i(20)], vec![i(-1), i(5)]]);

        let got = assemble_stats(&res);
        let want = json!({
            "population": {
                "family": { "total": 3, "online": 1 },
                "bots": { "total": 2500, "online": 40 },
                "levels": [
                    {"bucket":0,"family":2,"bots":100},
                    {"bucket":7,"family":1,"bots":5}
                ],
                "classes": {
                    "family": [{"class":1,"count":1},{"class":8,"count":2}],
                    "bots": [{"class":1,"count":50},{"class":2,"count":30}]
                },
                "factions": {
                    "family": {"alliance":2,"horde":1},
                    "bots": {"alliance":1200,"horde":1300}
                },
                "top_levels": {
                    "family": [{"name":"Hypeer","level":80,"family":true}],
                    "bots": [{"name":"Botto","level":80,"family":false}]
                },
                "guilds": {"count":4,"members":37}
            },
            "economy": {
                "copper": {"total":10000,"family":6000,"bots":4000},
                "richest": {
                    "family": [{"name":"Rich","copper":5000,"family":true}],
                    "bots": [{"name":"Botrich","copper":3000,"family":false}]
                },
                "auction": {"count":12,"buyout":120000},
                "mail": {"total":5,"to_family":2}
            },
            "journey": [
                {"name":"Hypeer","level":80,"class":2,"playtime":36000,"last_seen":1700000000,"online":true,"kills":500,"achievements":120,"quests":300}
            ],
            "history": {
                "boots":15,"total_uptime":864000,"longest":86400,"peak":42,
                "realm":"DML",
                "recent": [{"start":1699990000,"uptime":3600}]
            },
            "botwatch": {
                "zones": [{"zone":85,"count":30},{"zone":12,"count":10}],
                "continents": [{"map":1,"count":20},{"map":-1,"count":5}],
                "playtime": 999999
            }
        });
        assert_eq!(got, want);
    }

    #[test]
    fn assemble_stats_empty_db_is_all_zeros() {
        // Aggregate queries always return one row; on a fresh DB that row is
        // zeros. The loop-built arrays are empty. realm falls back to "".
        let mut res = vec![qr(vec![]); N_QUERIES];
        res[Q_POP] = qr(vec![vec![t("0"), t("0"), t("0"), t("0"), t("0")]]);
        res[Q_FACTIONS] = qr(vec![vec![t("0"), t("0"), t("0"), t("0")]]);
        res[Q_GUILDS] = qr(vec![vec![t("0"), t("0")]]);
        res[Q_COPPER] = qr(vec![vec![t("0"), t("0"), t("0")]]);
        res[Q_AUCTION] = qr(vec![vec![t("0"), t("0")]]);
        res[Q_MAIL] = qr(vec![vec![t("0"), t("0")]]);
        res[Q_UPTIME] = qr(vec![vec![t("0"), t("0"), t("0"), t("0")]]);
        // Q_REALM empty -> realm ""
        let got = assemble_stats(&res);
        assert_eq!(got["population"]["family"]["total"], json!(0));
        assert_eq!(got["population"]["levels"], json!([]));
        assert_eq!(got["history"]["realm"], json!(""));
        assert_eq!(got["botwatch"]["continents"], json!([]));
        assert_eq!(got["botwatch"]["playtime"], json!(0));
    }
}
