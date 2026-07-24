//! Native-mode direct-MySQL readers for the SIMPLER DB-backed pages (Task 3,
//! spike: `spike/docker-desktop-native`).
//!
//! WHY. In WSL mode the Teleport, Bot Browser and Accounts pages are served by
//! shelling `dml wow teleport-list` / `dml wow bots list` / `dml wow accounts`,
//! each of which runs `docker exec ac-database mysql …` inside the distro — on
//! Windows-native that path costs ~377ms per query plus a per-row fork storm. In
//! NATIVE mode Docker Desktop publishes the same MySQL on `127.0.0.1:<port>`, so
//! these readers open a DIRECT connection (via [`super::db`]) and assemble the
//! SAME JSON the CLI arms emit — instant, no `docker exec`, no bash.
//!
//! FAITHFUL PORTS. Each `assemble_*`/`*_sql` helper mirrors, line for line, the
//! matching arm in `cli/src/90-main.sh` (+ the row emitters in `cli/src/30-db.sh`)
//! so the bytes are identical:
//!   - teleport  -> `wow teleport-list` arm (90-main.sh) — `game_tele`
//!   - bots      -> `wow bots list` arm (90-main.sh) — `characters` ⋈ playerbots
//!   - accounts  -> `wow accounts` arm + `_accounts_rows_to_json` (30-db.sh)
//! A cargo parity test (`db_pages_parity.rs`, DB-gated) deep-equals each reader
//! against a live `dml …` run. These are NATIVE-MODE-ONLY by convention: WSL keeps
//! calling `dml`; callers gate on [`super::backend::selected`]/`backend_mode()`.
//!
//! NUMBER FIDELITY. The CLI splices numeric columns RAW into its JSON string
//! (`"x":$x`), then the frontend `serde_json`-parses the whole envelope. We
//! reproduce that exactly by building each numeric cell with [`num_token`]
//! (`serde_json::from_str` on the value's text) so the resulting `Number` has the
//! bit-identical representation the CLI's parse produced. Floats are the only
//! subtle case: the CLI reads `mysql`'s TEXT-protocol rendering, while our driver
//! speaks the BINARY protocol (which would hand back an f32/f64 whose shortest
//! round-trip string can differ from MySQL's `my_gcvt`). So the teleport query
//! `CAST(position_x AS CHAR)`s each coordinate, forcing MySQL to render it with
//! its OWN float-to-string — byte-identical to the text protocol (verified live).

use serde_json::{json, Value};

use super::db::{self, Database, DbConfig, DbError, QueryResult, SqlValue};

/// The text `mysql -N -B` would print for a cell — the form every bash arm sees
/// on stdin before it splices/validates. Integers stringify; bytes/text pass
/// through; SQL `NULL` renders as the literal `NULL` the CLI would show (none of
/// these queries can actually yield a NULL — every column is `NOT NULL` or
/// `COALESCE`d — but the mapping is faithful regardless).
pub fn cell_text(v: &SqlValue) -> String {
    match v {
        SqlValue::Int(i) => i.to_string(),
        SqlValue::Text(s) => s.clone(),
        SqlValue::Null => "NULL".to_string(),
    }
}

/// Build the JSON `Number` the CLI would have produced for a raw numeric splice:
/// `serde_json::from_str` on the column's text, exactly as the frontend parses
/// the CLI's `"key":<raw>` output. Non-numeric text (never expected for the
/// columns this is used on) degrades to `null` rather than emitting invalid JSON.
pub fn num_token(s: &str) -> Value {
    serde_json::from_str::<Value>(s)
        .ok()
        .filter(Value::is_number)
        .unwrap_or(Value::Null)
}

/// `true` when `s` is one-or-more ASCII digits — the bash `^[0-9]+$` guard.
fn is_all_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// Teleport list — mirrors the `wow teleport-list [--search]` arm (90-main.sh).
// ---------------------------------------------------------------------------

/// The exact SELECT the `teleport-list` arm runs, with each float coordinate
/// `CAST(... AS CHAR)` so MySQL renders it identically to the CLI's text-protocol
/// read (see the module header), plus the bound parameter(s) for its `?`
/// placeholder(s). `search` empty/None -> `WHERE 1=1` (bash's `-n "$search"`
/// default), no parameter; non-empty -> `WHERE name LIKE ?` bound to
/// `%<search>%` (LIKE wildcards in the term are NOT escaped, exactly like the
/// bash — binding removes the need for string-literal quote/backslash escaping
/// entirely, since the driver sends the value out-of-band from the statement
/// text; see [`super::db::query_with_params`]).
pub fn teleport_sql(search: Option<&str>) -> (String, Vec<mysql::Value>) {
    let (where_clause, params): (&str, Vec<mysql::Value>) = match search {
        Some(s) if !s.is_empty() => ("name LIKE ?", vec![mysql::Value::from(format!("%{s}%"))]),
        _ => ("1=1", Vec::new()),
    };
    let sql = format!(
        "SELECT name,CAST(position_x AS CHAR),CAST(position_y AS CHAR),CAST(position_z AS CHAR),map \
         FROM game_tele WHERE {where_clause} ORDER BY name LIMIT 500;"
    );
    (sql, params)
}

/// Assemble `{"locations":[{name,x,y,z,map}]}` from the teleport result set —
/// a port of the `teleport-list` while-loop. Rows with an empty name are skipped
/// (`[[ -z "$nm" ]] && continue`); x/y/z/map splice raw as JSON numbers.
pub fn assemble_teleport(res: &QueryResult) -> Value {
    let mut locations = Vec::with_capacity(res.rows.len());
    for row in &res.rows {
        if row.len() < 5 {
            continue;
        }
        let name = cell_text(&row[0]);
        if name.is_empty() {
            continue;
        }
        locations.push(json!({
            "name": name,
            "x": num_token(&cell_text(&row[1])),
            "y": num_token(&cell_text(&row[2])),
            "z": num_token(&cell_text(&row[3])),
            "map": num_token(&cell_text(&row[4])),
        }));
    }
    json!({ "locations": locations })
}

/// Run the teleport read against the live DB and assemble the CLI-identical JSON.
pub fn read_teleport_list(cfg: &DbConfig, search: Option<&str>) -> Result<Value, DbError> {
    let (sql, params) = teleport_sql(search);
    let res = db::query_with_params(cfg, Database::World, &sql, params)?;
    Ok(assemble_teleport(&res))
}

// ---------------------------------------------------------------------------
// Bots list — mirrors the `wow bots list [filters]` arm (90-main.sh).
// ---------------------------------------------------------------------------

/// Already-validated bot-browser filters, matching the `bots list` flags. The
/// frontend supplies these (the same values it passes the CLI); `limit` is
/// pre-clamped via [`clamp_limit`] so the assembled JSON echoes the clamped value
/// the CLI would.
#[derive(Debug, Clone, PartialEq)]
pub struct BotFilters {
    pub name: Option<String>,
    pub class: Option<u32>,
    pub min_level: Option<u32>,
    pub max_level: Option<u32>,
    pub online: bool,
    pub limit: u32,
    pub offset: u32,
}

/// Clamp a raw `--limit` the way the arm does: absent -> 50, then bounded to
/// `1..=200` (`(( btlimit > 200 )) && =200; (( btlimit < 1 )) && =1`).
pub fn clamp_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(50).clamp(1, 200)
}

/// Allowed WoW class ids, mirroring the bash arm's allowlist `case "$btclass"
/// in 1|2|3|4|5|6|7|8|9|11)` (90-main.sh ~3891-3894) — 10 has never shipped a
/// class and is deliberately excluded. The command layer (`wow_bots_read` in
/// `lib.rs`) rejects anything outside this set with `BAD_ARG` BEFORE a
/// [`BotFilters`] is even built, matching the bash's validate-before-SQL
/// doctrine.
pub const VALID_BOT_CLASSES: [u32; 10] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 11];

/// `true` when `class` is one of [`VALID_BOT_CLASSES`].
pub fn valid_bot_class(class: u32) -> bool {
    VALID_BOT_CLASSES.contains(&class)
}

/// The shared `WHERE` both bot queries use, plus its bound parameters in the
/// SAME order its `?` placeholder(s) appear — a port of the `btwhere` builder.
/// Bot identity is the authoritative playerbots-table subselect (cross-schema,
/// fully qualified, no user input). A `--name` prefix has its LIKE
/// metacharacters `%`/`_` escaped with `!` (declared `ESCAPE '!'`) exactly as
/// the bash does — that escaping is LIKE-pattern semantics (stopping a literal
/// `%`/`_` in the name from acting as a wildcard), not SQL-string escaping, so
/// it still applies even though the value itself is bound rather than spliced
/// as a literal. `class`/`min_level`/`max_level` are typed `u32` (already
/// validated by the command layer — see [`valid_bot_class`] — and never routed
/// through string escaping), so splicing their decimal form is safe; only the
/// free-text name needs binding.
pub fn bots_where(f: &BotFilters) -> (String, Vec<mysql::Value>) {
    let mut w = "c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type \
                 WHERE account_type IN (1,2))"
        .to_string();
    let mut params: Vec<mysql::Value> = Vec::new();
    if let Some(name) = f.name.as_deref().filter(|n| !n.is_empty()) {
        let like = name.replace('%', "!%").replace('_', "!_");
        w.push_str(" AND c.name LIKE ? ESCAPE '!'");
        params.push(mysql::Value::from(format!("{like}%")));
    }
    if let Some(c) = f.class {
        w.push_str(&format!(" AND c.class = {c}"));
    }
    match (f.min_level, f.max_level) {
        (Some(a), Some(b)) => w.push_str(&format!(" AND c.level BETWEEN {a} AND {b}")),
        (Some(a), None) => w.push_str(&format!(" AND c.level >= {a}")),
        (None, Some(b)) => w.push_str(&format!(" AND c.level <= {b}")),
        (None, None) => {}
    }
    if f.online {
        w.push_str(" AND c.online = 1");
    }
    (w, params)
}

/// `SELECT COUNT(*)` over the filtered bot population (drives the `total`),
/// plus its bound parameters.
pub fn bots_total_sql(f: &BotFilters) -> (String, Vec<mysql::Value>) {
    let (where_clause, params) = bots_where(f);
    (format!("SELECT COUNT(*) FROM characters c WHERE {where_clause};"), params)
}

/// One page of bot rows, ordered/limited/offset exactly like the arm, plus its
/// bound parameters.
pub fn bots_rows_sql(f: &BotFilters) -> (String, Vec<mysql::Value>) {
    let (where_clause, params) = bots_where(f);
    let sql = format!(
        "SELECT c.guid, c.name, c.class, c.race, c.gender, c.level, c.online, c.zone \
         FROM characters c WHERE {where_clause} ORDER BY c.name LIMIT {} OFFSET {};",
        f.limit, f.offset
    );
    (sql, params)
}

/// Assemble `{total,limit,offset,bots:[…]}` — a port of the `bots list`
/// emitter. `total` is guarded to 0 when non-numeric; each row skips an empty
/// guid, guards `zone` to 0, and maps `online == "1"` to a JSON bool.
pub fn assemble_bots(total: &str, limit: u32, offset: u32, rows: &QueryResult) -> Value {
    let total_num: i64 = if is_all_digits(total) { total.parse().unwrap_or(0) } else { 0 };
    let mut bots = Vec::with_capacity(rows.rows.len());
    for row in &rows.rows {
        if row.len() < 8 {
            continue;
        }
        let guid = cell_text(&row[0]);
        if guid.is_empty() {
            continue;
        }
        let zone = cell_text(&row[7]);
        let zone_val = if is_all_digits(&zone) { num_token(&zone) } else { json!(0) };
        bots.push(json!({
            "guid": num_token(&guid),
            "name": cell_text(&row[1]),
            "class": num_token(&cell_text(&row[2])),
            "race": num_token(&cell_text(&row[3])),
            "gender": num_token(&cell_text(&row[4])),
            "level": num_token(&cell_text(&row[5])),
            "online": cell_text(&row[6]) == "1",
            "zone": zone_val,
        }));
    }
    json!({ "total": total_num, "limit": limit, "offset": offset, "bots": bots })
}

/// Run both bot queries (count then page, same WHERE, same order as the CLI) and
/// assemble the CLI-identical JSON.
pub fn read_bots(cfg: &DbConfig, f: &BotFilters) -> Result<Value, DbError> {
    let (total_sql, total_params) = bots_total_sql(f);
    let total_res = db::query_with_params(cfg, Database::Characters, &total_sql, total_params)?;
    let total = total_res
        .rows
        .first()
        .and_then(|r| r.first())
        .map(cell_text)
        .unwrap_or_default();
    let (rows_sql, rows_params) = bots_rows_sql(f);
    let rows = db::query_with_params(cfg, Database::Characters, &rows_sql, rows_params)?;
    Ok(assemble_bots(&total, f.limit, f.offset, &rows))
}

// ---------------------------------------------------------------------------
// Accounts — mirrors the `wow accounts` arm + `_accounts_rows_to_json`.
// ---------------------------------------------------------------------------

/// The exact SELECT the `accounts` arm runs (RNDBOT%/AHBOT filtered; gmlevel =
/// MAX across realms; LEFT JOIN characters). Fully-qualified `acore_auth.*`, so
/// it runs against the characters DB like the CLI's `db_chars_query`.
pub const ACCOUNTS_SQL: &str = "SELECT a.id, a.username, COALESCE(g.gmlevel,0), \
    COALESCE(c.guid,''), COALESCE(c.name,''), COALESCE(c.level,'') \
    FROM acore_auth.account a \
    LEFT JOIN (SELECT id, MAX(gmlevel) AS gmlevel FROM acore_auth.account_access GROUP BY id) g ON g.id = a.id \
    LEFT JOIN characters c ON c.account = a.id \
    WHERE a.username NOT LIKE 'RNDBOT%' AND a.username <> 'AHBOT' \
    ORDER BY a.id, c.level DESC;";

/// One assembled account object `{id,username,gm_level,characters}` — the shape
/// `_accounts_rows_to_json` emits per group.
fn account_obj(id: &str, username: &str, gm_level: &str, characters: Vec<Value>) -> Value {
    json!({
        "id": num_token(id),
        "username": username,
        "gm_level": num_token(gm_level),
        "characters": characters,
    })
}

/// Assemble `{"accounts":[…]}` from the account result set — a faithful port of
/// `_accounts_rows_to_json` (30-db.sh:110). Rows arrive sorted by account id;
/// consecutive rows with the same id fold into one account. `gm_level` is taken
/// from the group's first row (guarded to 0 when non-numeric); a row contributes
/// a character only when its guid is non-empty (LEFT JOIN misses arrive empty).
pub fn assemble_accounts(res: &QueryResult) -> Value {
    let mut accounts: Vec<Value> = Vec::new();
    let mut cur_id: Option<String> = None;
    let mut cur_name = String::new();
    let mut cur_gm = String::from("0");
    let mut chars: Vec<Value> = Vec::new();

    for row in &res.rows {
        if row.len() < 6 {
            continue;
        }
        let aid = cell_text(&row[0]);
        if aid.is_empty() {
            continue;
        }
        if cur_id.as_deref() != Some(aid.as_str()) {
            if let Some(id) = cur_id.take() {
                account_flush(&mut accounts, &id, &cur_name, &cur_gm, std::mem::take(&mut chars));
            }
            cur_id = Some(aid);
            cur_name = cell_text(&row[1]);
            let gmlvl = cell_text(&row[2]);
            cur_gm = if is_all_digits(&gmlvl) { gmlvl } else { "0".to_string() };
            chars.clear();
        }
        let guid = cell_text(&row[3]);
        if !guid.is_empty() {
            chars.push(json!({
                "guid": num_token(&guid),
                "name": cell_text(&row[4]),
                "level": num_token(&cell_text(&row[5])),
            }));
        }
    }
    if let Some(id) = cur_id {
        account_flush(&mut accounts, &id, &cur_name, &cur_gm, chars);
    }
    json!({ "accounts": accounts })
}

fn account_flush(out: &mut Vec<Value>, id: &str, name: &str, gm: &str, chars: Vec<Value>) {
    out.push(account_obj(id, name, gm, chars));
}

/// Run the accounts read against the live DB and assemble the CLI-identical JSON.
pub fn read_accounts(cfg: &DbConfig) -> Result<Value, DbError> {
    let res = db::query(cfg, Database::Characters, ACCOUNTS_SQL)?;
    Ok(assemble_accounts(&res))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> SqlValue {
        SqlValue::Text(s.to_string())
    }
    fn i(n: i64) -> SqlValue {
        SqlValue::Int(n)
    }

    #[test]
    fn num_token_parses_and_rejects() {
        assert_eq!(num_token("2502"), json!(2502));
        assert_eq!(num_token("-10964"), json!(-10964));
        assert_eq!(num_token("31.5573"), json!(31.5573));
        assert_eq!(num_token("0.16"), json!(0.16));
        assert_eq!(num_token(""), Value::Null);
        assert_eq!(num_token("NULL"), Value::Null);
    }

    #[test]
    fn teleport_sql_default_and_search() {
        let (sql, params) = teleport_sql(None);
        assert!(sql.contains("WHERE 1=1"));
        assert!(params.is_empty());
        let (sql, params) = teleport_sql(Some(""));
        assert!(sql.contains("WHERE 1=1"));
        assert!(params.is_empty());
        let (sql, _) = teleport_sql(None);
        assert!(sql.contains("CAST(position_x AS CHAR)"));
        assert!(sql.contains("ORDER BY name LIMIT 500"));
        // Searched: no literal in the SQL text -- a quote in the term is bound,
        // not spliced (finding #1). The '%'-wrapped value is the bound param.
        let (sql, params) = teleport_sql(Some("Orgri'mmar"));
        assert!(sql.contains("name LIKE ?"), "got: {sql}");
        assert!(!sql.contains('\''), "search term must not appear in the SQL text: {sql}");
        assert_eq!(params, vec![mysql::Value::from("%Orgri'mmar%")]);
    }

    #[test]
    fn assemble_teleport_shapes_rows_and_skips_empty_name() {
        let res = QueryResult {
            columns: vec![],
            rows: vec![
                vec![t("Orgrimmar"), t("1629.85"), t("-4373.64"), t("31.5573"), i(1)],
                // empty name -> skipped, exactly like the bash guard
                vec![t(""), t("0"), t("0"), t("0"), i(0)],
                vec![t("Deep"), t("-10964"), t("240.019"), t("28.5578"), i(0)],
            ],
        };
        let got = assemble_teleport(&res);
        assert_eq!(
            got,
            json!({"locations":[
                {"name":"Orgrimmar","x":1629.85,"y":-4373.64,"z":31.5573,"map":1},
                {"name":"Deep","x":-10964,"y":240.019,"z":28.5578,"map":0}
            ]})
        );
    }

    #[test]
    fn clamp_limit_bounds() {
        assert_eq!(clamp_limit(None), 50);
        assert_eq!(clamp_limit(Some(0)), 1);
        assert_eq!(clamp_limit(Some(75)), 75);
        assert_eq!(clamp_limit(Some(9999)), 200);
    }

    #[test]
    fn bots_where_builds_filters() {
        let base = BotFilters {
            name: None,
            class: None,
            min_level: None,
            max_level: None,
            online: false,
            limit: 50,
            offset: 0,
        };
        let (w, p) = bots_where(&base);
        assert_eq!(
            w,
            "c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))"
        );
        assert!(p.is_empty());
        let full = BotFilters {
            name: Some("Foo_bar".into()),
            class: Some(5),
            min_level: Some(10),
            max_level: Some(20),
            online: true,
            limit: 50,
            offset: 0,
        };
        let (w, p) = bots_where(&full);
        // LIKE metachars escaped with ! and declared ESCAPE '!'; the value is
        // bound (finding #1), not spliced as a quoted literal.
        assert!(w.contains("AND c.name LIKE ? ESCAPE '!'"), "got: {w}");
        assert_eq!(p, vec![mysql::Value::from("Foo!_bar%")]);
        assert!(w.contains("AND c.class = 5"));
        assert!(w.contains("AND c.level BETWEEN 10 AND 20"));
        assert!(w.ends_with("AND c.online = 1"));
        // min-only / max-only branches.
        let minonly = BotFilters { max_level: None, min_level: Some(7), ..base.clone() };
        assert!(bots_where(&minonly).0.contains("AND c.level >= 7"));
        let maxonly = BotFilters { min_level: None, max_level: Some(7), ..base.clone() };
        assert!(bots_where(&maxonly).0.contains("AND c.level <= 7"));
    }

    #[test]
    fn bots_rows_sql_carries_limit_offset() {
        let f = BotFilters {
            name: None,
            class: None,
            min_level: None,
            max_level: None,
            online: false,
            limit: 25,
            offset: 100,
        };
        let (s, _) = bots_rows_sql(&f);
        assert!(s.contains("ORDER BY c.name LIMIT 25 OFFSET 100"), "got: {s}");
        assert!(bots_total_sql(&f).0.starts_with("SELECT COUNT(*) FROM characters c WHERE"));
    }

    #[test]
    fn valid_bot_class_matches_bash_allowlist() {
        for c in VALID_BOT_CLASSES {
            assert!(valid_bot_class(c), "class {c} should be valid");
        }
        assert!(!valid_bot_class(0));
        assert!(!valid_bot_class(10)); // never shipped
        assert!(!valid_bot_class(12));
        assert!(!valid_bot_class(255));
    }

    #[test]
    fn assemble_bots_shapes_page() {
        let rows = QueryResult {
            columns: vec![],
            // guid, name, class, race, gender, level, online, zone
            rows: vec![
                vec![i(162), t("Aallaena"), i(2), i(11), i(1), i(6), i(0), i(3524)],
                vec![i(460), t("Aastra"), i(11), i(4), i(1), i(37), i(1), i(15)],
                // empty guid -> skipped
                vec![t(""), t("X"), i(1), i(1), i(1), i(1), i(0), i(0)],
            ],
        };
        let got = assemble_bots("2500", 3, 0, &rows);
        assert_eq!(
            got,
            json!({
                "total": 2500, "limit": 3, "offset": 0,
                "bots": [
                    {"guid":162,"name":"Aallaena","class":2,"race":11,"gender":1,"level":6,"online":false,"zone":3524},
                    {"guid":460,"name":"Aastra","class":11,"race":4,"gender":1,"level":37,"online":true,"zone":15}
                ]
            })
        );
        // non-numeric total guards to 0.
        assert_eq!(assemble_bots("", 3, 0, &QueryResult { columns: vec![], rows: vec![] })["total"], json!(0));
    }

    #[test]
    fn assemble_accounts_groups_and_nests() {
        // Sorted by id; COALESCE misses arrive as empty guid/name/level.
        let res = QueryResult {
            columns: vec![],
            // aid, uname, gmlvl, guid, cname, clvl
            rows: vec![
                vec![i(251), t("HYPEER"), i(3), t("2502"), t("Hypeer"), t("80")],
                vec![i(251), t("HYPEER"), i(3), t("2506"), t("Shashasha"), t("80")],
                vec![i(254), t("DMLSOAP"), i(3), t(""), t(""), t("")],
                vec![i(256), t("TEST2"), i(0), t("2504"), t("Testto"), t("1")],
            ],
        };
        let got = assemble_accounts(&res);
        assert_eq!(
            got,
            json!({"accounts":[
                {"id":251,"username":"HYPEER","gm_level":3,"characters":[
                    {"guid":2502,"name":"Hypeer","level":80},
                    {"guid":2506,"name":"Shashasha","level":80}
                ]},
                {"id":254,"username":"DMLSOAP","gm_level":3,"characters":[]},
                {"id":256,"username":"TEST2","gm_level":0,"characters":[
                    {"guid":2504,"name":"Testto","level":1}
                ]}
            ]})
        );
    }

    #[test]
    fn assemble_accounts_guards_nonnumeric_gm() {
        let res = QueryResult {
            columns: vec![],
            rows: vec![vec![i(1), t("A"), t("NULL"), t(""), t(""), t("")]],
        };
        let got = assemble_accounts(&res);
        assert_eq!(got["accounts"][0]["gm_level"], json!(0));
    }
}
