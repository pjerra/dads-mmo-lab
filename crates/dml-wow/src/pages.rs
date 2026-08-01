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
/// Bot identity comes from [`crate::botid::bot_clause`] (registry OR reserved
/// account prefix, cross-schema, fully qualified, no user input — the registry
/// alone is empty on some installs). A `--name` prefix has its LIKE
/// metacharacters `%`/`_` escaped with `!` (declared `ESCAPE '!'`) exactly as
/// the bash does — that escaping is LIKE-pattern semantics (stopping a literal
/// `%`/`_` in the name from acting as a wildcard), not SQL-string escaping, so
/// it still applies even though the value itself is bound rather than spliced
/// as a literal. `class`/`min_level`/`max_level` are typed `u32` (already
/// validated by the command layer — see [`valid_bot_class`] — and never routed
/// through string escaping), so splicing their decimal form is safe; only the
/// free-text name needs binding.
pub fn bots_where(f: &BotFilters, bot_prefix: &str) -> (String, Vec<mysql::Value>) {
    let mut w = crate::botid::bot_clause("c.account", bot_prefix);
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
pub fn bots_total_sql(f: &BotFilters, bot_prefix: &str) -> (String, Vec<mysql::Value>) {
    let (where_clause, params) = bots_where(f, bot_prefix);
    (format!("SELECT COUNT(*) FROM characters c WHERE {where_clause};"), params)
}

/// One page of bot rows, ordered/limited/offset exactly like the arm, plus its
/// bound parameters.
pub fn bots_rows_sql(f: &BotFilters, bot_prefix: &str) -> (String, Vec<mysql::Value>) {
    let (where_clause, params) = bots_where(f, bot_prefix);
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
    let bot_prefix = crate::botid::bot_account_prefix();
    let (total_sql, total_params) = bots_total_sql(f, &bot_prefix);
    let total_res = db::query_with_params(cfg, Database::Characters, &total_sql, total_params)?;
    let total = total_res
        .rows
        .first()
        .and_then(|r| r.first())
        .map(cell_text)
        .unwrap_or_default();
    let (rows_sql, rows_params) = bots_rows_sql(f, &bot_prefix);
    let rows = db::query_with_params(cfg, Database::Characters, &rows_sql, rows_params)?;
    Ok(assemble_bots(&total, f.limit, f.offset, &rows))
}

// ---------------------------------------------------------------------------
// Accounts — mirrors the `wow accounts` arm + `_accounts_rows_to_json`.
// ---------------------------------------------------------------------------

/// The exact SELECT the `accounts` arm runs (bot/AHBOT filtered; gmlevel =
/// MAX across realms; LEFT JOIN characters). Fully-qualified `acore_auth.*`, so
/// it runs against the characters DB like the CLI's `db_chars_query`.
///
/// The bot filter is [`crate::botid::username_is_bot`] rather than a hardcoded
/// `NOT LIKE 'RNDBOT%'`, so a server that customised
/// `AiPlayerbot.RandomBotAccountPrefix` does not get 250 bot accounts in its
/// character picker. It stays PREFIX-ONLY on purpose — see that function's
/// docs: this is the one bot filter that must not require the
/// `acore_playerbots` schema to exist.
pub fn accounts_sql(bot_prefix: &str) -> String {
    format!(
        "SELECT a.id, a.username, COALESCE(g.gmlevel,0), \
         COALESCE(c.guid,''), COALESCE(c.name,''), COALESCE(c.level,'') \
         FROM acore_auth.account a \
         LEFT JOIN (SELECT id, MAX(gmlevel) AS gmlevel FROM acore_auth.account_access GROUP BY id) g ON g.id = a.id \
         LEFT JOIN characters c ON c.account = a.id \
         WHERE NOT {} AND a.username <> 'AHBOT' \
         ORDER BY a.id, c.level DESC;",
        crate::botid::username_is_bot("a.username", bot_prefix)
    )
}

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
    let res = db::query(cfg, Database::Characters, &accounts_sql(&crate::botid::bot_account_prefix()))?;
    Ok(assemble_accounts(&res))
}

// ---------------------------------------------------------------------------
// Players online / Party online — mirror the `wow players online` /
// `wow party online` arms (90-main.sh). Both filter to real (non-bot)
// characters via the SAME cross-schema playerbots-account subselect, so the
// WHERE is factored into one constant to avoid duplicating it (per the task
// brief).
// ---------------------------------------------------------------------------

/// The shared `WHERE` both "who's online" queries use — real accounts only.
///
/// This used to be `c.account NOT IN (<registry subselect>)` alone, which is
/// TRUE for every row when the registry is empty (it is, on a freshly built
/// install) — so the launcher's Home page listed 1000 playerbots as real
/// players. Bot identity now comes from [`crate::botid::human_clause`], which
/// also checks the reserved account-name prefix. See `botid`'s module docs.
fn online_humans_where(bot_prefix: &str) -> String {
    format!("c.online = 1 AND {}", crate::botid::human_clause("c.account", bot_prefix))
}

/// The exact SELECT the `players online` arm runs (no bound params — nothing
/// user-controlled).
fn players_online_sql(bot_prefix: &str) -> String {
    format!(
        "SELECT c.name, c.level, c.class, c.zone FROM characters c WHERE {} \
         ORDER BY c.name;",
        online_humans_where(bot_prefix)
    )
}

/// The exact SELECT the `party online` arm runs — same WHERE, different
/// projection/order (guid, name, class, level).
fn party_online_sql(bot_prefix: &str) -> String {
    format!(
        "SELECT c.guid, c.name, c.class, c.level FROM characters c WHERE {} \
         ORDER BY c.name;",
        online_humans_where(bot_prefix)
    )
}

/// Assemble `{"players":[{name,level,class,zone}]}` — a port of the `players
/// online` while-loop. Rows with an empty name are skipped; `level`/`class`/
/// `zone` each guard to `0` when non-numeric (the bash guards EVERY
/// interpolated numeric here, not just zone, to avoid emitting invalid JSON
/// like `"level":,`).
pub fn assemble_players_online(res: &QueryResult) -> Value {
    let mut players = Vec::with_capacity(res.rows.len());
    for row in &res.rows {
        if row.len() < 4 {
            continue;
        }
        let name = cell_text(&row[0]);
        if name.is_empty() {
            continue;
        }
        let lvl = cell_text(&row[1]);
        let lvl = if is_all_digits(&lvl) { lvl } else { "0".to_string() };
        let cls = cell_text(&row[2]);
        let cls = if is_all_digits(&cls) { cls } else { "0".to_string() };
        let zone = cell_text(&row[3]);
        let zone = if is_all_digits(&zone) { zone } else { "0".to_string() };
        players.push(json!({
            "name": name,
            "level": num_token(&lvl),
            "class": num_token(&cls),
            "zone": num_token(&zone),
        }));
    }
    json!({ "players": players })
}

/// Assemble `{"online":[{guid,name,class,level}]}` — a port of the `party
/// online` while-loop. Rows with an empty guid are skipped; `guid`/`class`/
/// `level` each guard to `0` when non-numeric, exactly like `players online`
/// (0 and not `null`: both backends serve the same page, so a malformed row
/// must not mean two different things depending on which one answered).
pub fn assemble_party_online(res: &QueryResult) -> Value {
    let mut online = Vec::with_capacity(res.rows.len());
    for row in &res.rows {
        if row.len() < 4 {
            continue;
        }
        let guid = cell_text(&row[0]);
        if guid.is_empty() {
            continue;
        }
        let guid = if is_all_digits(&guid) { guid } else { "0".to_string() };
        let cls = cell_text(&row[2]);
        let cls = if is_all_digits(&cls) { cls } else { "0".to_string() };
        let lvl = cell_text(&row[3]);
        let lvl = if is_all_digits(&lvl) { lvl } else { "0".to_string() };
        online.push(json!({
            "guid": num_token(&guid),
            "name": cell_text(&row[1]),
            "class": num_token(&cls),
            "level": num_token(&lvl),
        }));
    }
    json!({ "online": online })
}

/// Run the players-online read against the live DB and assemble the
/// CLI-identical JSON.
pub fn read_players_online(cfg: &DbConfig) -> Result<Value, DbError> {
    let res = db::query(cfg, Database::Characters, &players_online_sql(&crate::botid::bot_account_prefix()))?;
    Ok(assemble_players_online(&res))
}

/// Run the party-online read against the live DB and assemble the
/// CLI-identical JSON.
pub fn read_party_online(cfg: &DbConfig) -> Result<Value, DbError> {
    let res = db::query(cfg, Database::Characters, &party_online_sql(&crate::botid::bot_account_prefix()))?;
    Ok(assemble_party_online(&res))
}

// ---------------------------------------------------------------------------
// Items search — mirrors `wow items search` (90-main.sh `items)` arm) +
// `build_item_search_sql`/`_items_rows_to_json` (30-db.sh).
// ---------------------------------------------------------------------------

/// The `items search` filters, matching the CLI's `--name`/`--quality`/
/// `--min-level`/`--max-level` flags. `name` mirrors `build_item_search_sql`'s
/// own `[[ -n "$name" ]]` gate (empty -> the filter is simply omitted); the
/// command layer additionally rejects an empty/whitespace-only name with
/// `BAD_ARG` BEFORE this is ever built (90-main.sh's own pre-check), so in
/// practice `name` always arrives non-empty here — this struct still handles
/// empty faithfully so the builder alone stays a faithful port.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ItemSearchOpts {
    pub name: String,
    pub quality: Option<u32>,
    pub min_level: Option<u32>,
    pub max_level: Option<u32>,
}

/// Default `--limit` the arm uses when the frontend doesn't pass one
/// (`limit=50` in the flag-parsing loop, 90-main.sh `items search`).
pub const ITEMS_SEARCH_DEFAULT_LIMIT: u32 = 50;

/// Build the exact SELECT `build_item_search_sql` would (30-db.sh:62-70),
/// with every filter value BOUND rather than spliced (the bash string-escapes
/// via `sql_escape`; binding is strictly safer and parity-equivalent — see
/// the module-wide rule in the task brief). Each optional filter appends its
/// own clause+param ONLY when present, in the same order the bash checks
/// them (name, quality, min-level, max-level), followed by the `LIMIT ?`
/// bound to [`ITEMS_SEARCH_DEFAULT_LIMIT`].
pub fn items_search_sql(opts: &ItemSearchOpts) -> (String, Vec<mysql::Value>) {
    let mut where_clause = String::from("1=1");
    let mut params: Vec<mysql::Value> = Vec::new();
    if !opts.name.is_empty() {
        where_clause.push_str(" AND name LIKE ?");
        params.push(mysql::Value::from(format!("%{}%", opts.name)));
    }
    if let Some(q) = opts.quality {
        where_clause.push_str(" AND Quality = ?");
        params.push(mysql::Value::from(q));
    }
    if let Some(minl) = opts.min_level {
        where_clause.push_str(" AND RequiredLevel >= ?");
        params.push(mysql::Value::from(minl));
    }
    if let Some(maxl) = opts.max_level {
        where_clause.push_str(" AND RequiredLevel <= ?");
        params.push(mysql::Value::from(maxl));
    }
    let sql = format!(
        "SELECT entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid \
         FROM item_template WHERE {where_clause} ORDER BY RequiredLevel, name LIMIT ?;"
    );
    params.push(mysql::Value::from(ITEMS_SEARCH_DEFAULT_LIMIT));
    (sql, params)
}

/// Assemble `{"items":[{entry,name,quality,item_level,required_level,class,\
/// subclass,inventory_type,displayid}]}` — a faithful port of
/// `_items_rows_to_json` (30-db.sh:90-101). Rows with an empty entry are
/// skipped; every numeric column splices RAW in the bash (no digit guard), so
/// each is passed through [`num_token`] like [`assemble_teleport`].
pub fn assemble_items_search(res: &QueryResult) -> Value {
    let mut items = Vec::with_capacity(res.rows.len());
    for row in &res.rows {
        if row.len() < 9 {
            continue;
        }
        let entry = cell_text(&row[0]);
        if entry.is_empty() {
            continue;
        }
        items.push(json!({
            "entry": num_token(&entry),
            "name": cell_text(&row[1]),
            "quality": num_token(&cell_text(&row[2])),
            "item_level": num_token(&cell_text(&row[3])),
            "required_level": num_token(&cell_text(&row[4])),
            "class": num_token(&cell_text(&row[5])),
            "subclass": num_token(&cell_text(&row[6])),
            "inventory_type": num_token(&cell_text(&row[7])),
            "displayid": num_token(&cell_text(&row[8])),
        }));
    }
    json!({ "items": items })
}

/// Run the items-search read against the live DB and assemble the
/// CLI-identical JSON.
pub fn read_items_search(cfg: &DbConfig, opts: &ItemSearchOpts) -> Result<Value, DbError> {
    let (sql, params) = items_search_sql(opts);
    let res = db::query_with_params(cfg, Database::World, &sql, params)?;
    Ok(assemble_items_search(&res))
}

// ---------------------------------------------------------------------------
// Char-progress / Achievements — mirror the `wow char-progress` /
// `wow achievements` arms (90-main.sh:2135-2169, 2170-2192). Both start with
// the same guid lookup and share the achievement-row shaping, so those are
// factored into one lookup + one assembler.
// ---------------------------------------------------------------------------

const CHAR_GUID_SQL: &str = "SELECT guid FROM characters WHERE name = ? LIMIT 1;";
const CHAR_TALENT_META_SQL: &str =
    "SELECT activeTalentGroup, talentGroupsCount FROM characters WHERE guid = ?;";
const CHAR_ACHIEVEMENT_TOTAL_SQL: &str =
    "SELECT COUNT(*) FROM character_achievement WHERE guid = ?;";
const CHAR_ACHIEVEMENT_RECENT_SQL: &str =
    "SELECT achievement, date FROM character_achievement WHERE guid = ? ORDER BY date DESC LIMIT 10;";
const CHAR_ACHIEVEMENT_EARNED_SQL: &str =
    "SELECT achievement, date FROM character_achievement WHERE guid = ? ORDER BY achievement;";
const CHAR_TALENT_SPELLS_SQL: &str =
    "SELECT spell FROM character_talent WHERE guid = ? AND (specMask & (1 << ?)) ORDER BY spell;";

/// Extract the character guid from a `CHAR_GUID_SQL` result set, matching the
/// bash's `[[ "$cguid" =~ ^[0-9]+$ ]]` guard — `None` (NOT_FOUND) unless the
/// first row's first cell is one-or-more digits.
fn extract_char_guid(res: &QueryResult) -> Option<i64> {
    let raw = res.rows.first().and_then(|r| r.first()).map(cell_text).unwrap_or_default();
    is_all_digits(&raw).then(|| raw.parse().ok()).flatten()
}

/// Extract `(active_group, groups_count)` from a `CHAR_TALENT_META_SQL`
/// result, guarding each to the bash's fallback (`agroup` -> `"0"`, `gcount`
/// -> `"1"`) when non-numeric.
fn extract_talent_meta(res: &QueryResult) -> (String, String) {
    let row = res.rows.first();
    let agroup = row.and_then(|r| r.first()).map(cell_text).unwrap_or_default();
    let gcount = row.and_then(|r| r.get(1)).map(cell_text).unwrap_or_default();
    let agroup = if is_all_digits(&agroup) { agroup } else { "0".to_string() };
    let gcount = if is_all_digits(&gcount) { gcount } else { "1".to_string() };
    (agroup, gcount)
}

/// Extract the achievement total from a `CHAR_ACHIEVEMENT_TOTAL_SQL` result,
/// guarding to `"0"` when non-numeric/missing (the bash's `|| atotal=0` plus
/// its digit-regex guard).
fn extract_achievement_total(res: &QueryResult) -> String {
    let raw = res.rows.first().and_then(|r| r.first()).map(cell_text).unwrap_or_default();
    if is_all_digits(&raw) { raw } else { "0".to_string() }
}

/// Assemble a `[{id,date}]` achievement list — shared by `char-progress`'s
/// "recent 10" and `achievements`' full "earned" set (both bash loops are
/// byte-identical: skip an empty/non-numeric id, guard `date` to `0` when
/// non-numeric).
fn assemble_achievement_entries(res: &QueryResult) -> Vec<Value> {
    let mut out = Vec::with_capacity(res.rows.len());
    for row in &res.rows {
        if row.is_empty() {
            continue;
        }
        let aid = cell_text(&row[0]);
        if aid.is_empty() || !is_all_digits(&aid) {
            continue;
        }
        let adate = row.get(1).map(cell_text).unwrap_or_default();
        let adate = if is_all_digits(&adate) { adate } else { "0".to_string() };
        out.push(json!({ "id": num_token(&aid), "date": num_token(&adate) }));
    }
    out
}

/// Assemble a `spells:[...]` array of raw talent-spell ids — a port of the
/// `tspells` loop (skip empty/non-numeric ids).
fn assemble_talent_spells(res: &QueryResult) -> Vec<Value> {
    let mut out = Vec::with_capacity(res.rows.len());
    for row in &res.rows {
        let Some(sid) = row.first().map(cell_text) else { continue };
        if sid.is_empty() || !is_all_digits(&sid) {
            continue;
        }
        out.push(num_token(&sid));
    }
    out
}

/// Assemble the full `char-progress` envelope from its five already-decoded
/// pieces — a pure port of the arm's final `json_ok` line.
pub fn assemble_char_progress(
    total: &str,
    recent: Vec<Value>,
    groups_count: &str,
    active_group: &str,
    spells: Vec<Value>,
) -> Value {
    json!({
        "achievements": { "total": num_token(total), "recent": recent },
        "talents": {
            "groups_count": num_token(groups_count),
            "active_group": num_token(active_group),
            "spells": spells,
        },
    })
}

/// Run the `char-progress` 5-query sequence against the live DB (guid lookup,
/// talent-group meta, achievement total, recent-10 achievements, active-spec
/// talent spells — same order as the arm) and assemble the CLI-identical
/// JSON. `Ok(None)` means "no such character" (NOT_FOUND at the command
/// layer); the caller is expected to have already validated `name` with
/// [`super::soap_cmds::valid_charname`].
pub fn read_char_progress(cfg: &DbConfig, name: &str) -> Result<Option<Value>, DbError> {
    let guid_res = db::query_with_params(
        cfg,
        Database::Characters,
        CHAR_GUID_SQL,
        vec![mysql::Value::from(name)],
    )?;
    let Some(guid) = extract_char_guid(&guid_res) else {
        return Ok(None);
    };
    let guid_param = mysql::Value::from(guid);

    let meta_res = db::query_with_params(
        cfg,
        Database::Characters,
        CHAR_TALENT_META_SQL,
        vec![guid_param.clone()],
    )?;
    let (active_group, groups_count) = extract_talent_meta(&meta_res);

    // The achievement-total, recent-achievements, and talent-spells queries
    // degrade gracefully on failure (matches `90-main.sh:2135-2169`'s
    // `atotal="$(db_chars_query ...)" || atotal=0` and the recent/spells
    // loops' `db_chars_query "..." || true`) -- only the guid lookup above
    // and the talent-group-meta query are hard DB_UNREACHABLE errors.
    let total_res = db::query_with_params(
        cfg,
        Database::Characters,
        CHAR_ACHIEVEMENT_TOTAL_SQL,
        vec![guid_param.clone()],
    )
    .unwrap_or_else(|_| QueryResult { columns: vec![], rows: vec![] });
    let total = extract_achievement_total(&total_res);

    let recent_res = db::query_with_params(
        cfg,
        Database::Characters,
        CHAR_ACHIEVEMENT_RECENT_SQL,
        vec![guid_param.clone()],
    )
    .unwrap_or_else(|_| QueryResult { columns: vec![], rows: vec![] });
    let recent = assemble_achievement_entries(&recent_res);

    let active_group_num: i64 = active_group.parse().unwrap_or(0);
    let spells_res = db::query_with_params(
        cfg,
        Database::Characters,
        CHAR_TALENT_SPELLS_SQL,
        vec![guid_param, mysql::Value::from(active_group_num)],
    )
    .unwrap_or_else(|_| QueryResult { columns: vec![], rows: vec![] });
    let spells = assemble_talent_spells(&spells_res);

    Ok(Some(assemble_char_progress(&total, recent, &groups_count, &active_group, spells)))
}

/// Run the `achievements` read (guid lookup + full earned-achievement dump,
/// ordered by achievement id rather than date) against the live DB and
/// assemble the CLI-identical JSON. `Ok(None)` means "no such character"
/// (NOT_FOUND at the command layer), same convention as
/// [`read_char_progress`].
pub fn read_achievements(cfg: &DbConfig, name: &str) -> Result<Option<Value>, DbError> {
    let guid_res = db::query_with_params(
        cfg,
        Database::Characters,
        CHAR_GUID_SQL,
        vec![mysql::Value::from(name)],
    )?;
    let Some(guid) = extract_char_guid(&guid_res) else {
        return Ok(None);
    };
    // Matches `90-main.sh:2170-2192`'s `db_chars_query "..." || true`: a
    // failure here degrades to `earned:[]` inside an `ok:true` envelope --
    // only the guid lookup above is a hard DB_UNREACHABLE error.
    let earned_res = db::query_with_params(
        cfg,
        Database::Characters,
        CHAR_ACHIEVEMENT_EARNED_SQL,
        vec![mysql::Value::from(guid)],
    )
    .unwrap_or_else(|_| QueryResult { columns: vec![], rows: vec![] });
    let earned = assemble_achievement_entries(&earned_res);
    Ok(Some(json!({ "earned": earned })))
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
        let (w, p) = bots_where(&base, "rndbot");
        assert_eq!(w, crate::botid::bot_clause("c.account", "rndbot"));
        // Both signals, so an unpopulated playerbots registry cannot empty the
        // bots page (the 2026-08-01 incident).
        assert!(w.contains("playerbots_account_type"), "got: {w}");
        assert!(w.contains("UPPER(username) LIKE 'RNDBOT%'"), "got: {w}");
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
        let (w, p) = bots_where(&full, "rndbot");
        // LIKE metachars escaped with ! and declared ESCAPE '!'; the value is
        // bound (finding #1), not spliced as a quoted literal.
        assert!(w.contains("AND c.name LIKE ? ESCAPE '!'"), "got: {w}");
        assert_eq!(p, vec![mysql::Value::from("Foo!_bar%")]);
        assert!(w.contains("AND c.class = 5"));
        assert!(w.contains("AND c.level BETWEEN 10 AND 20"));
        assert!(w.ends_with("AND c.online = 1"));
        // min-only / max-only branches.
        let minonly = BotFilters { max_level: None, min_level: Some(7), ..base.clone() };
        assert!(bots_where(&minonly, "rndbot").0.contains("AND c.level >= 7"));
        let maxonly = BotFilters { min_level: None, max_level: Some(7), ..base.clone() };
        assert!(bots_where(&maxonly, "rndbot").0.contains("AND c.level <= 7"));
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
        let (s, _) = bots_rows_sql(&f, "rndbot");
        assert!(s.contains("ORDER BY c.name LIMIT 25 OFFSET 100"), "got: {s}");
        assert!(bots_total_sql(&f, "rndbot").0.starts_with("SELECT COUNT(*) FROM characters c WHERE"));
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

    // -- players online / party online --------------------------------

    #[test]
    fn players_online_and_party_online_sql_share_the_where() {
        let p = players_online_sql("rndbot");
        let o = party_online_sql("rndbot");
        assert!(p.contains("SELECT c.name, c.level, c.class, c.zone"), "got: {p}");
        assert!(o.contains("SELECT c.guid, c.name, c.class, c.level"), "got: {o}");
        for sql in [&p, &o] {
            assert!(sql.contains("c.online = 1"), "got: {sql}");
            assert!(sql.contains(&crate::botid::human_clause("c.account", "rndbot")), "got: {sql}");
            assert!(sql.contains("ORDER BY c.name;"), "got: {sql}");
        }
    }

    /// The character picker's bot filter: conf-driven prefix, and NO
    /// dependency on the playerbots schema (the deliberate difference from
    /// every other bot filter — see `botid::username_is_bot`).
    #[test]
    fn accounts_sql_uses_the_configured_prefix_and_never_the_playerbots_schema() {
        let sql = accounts_sql("rndbot");
        assert!(sql.contains("NOT UPPER(a.username) LIKE 'RNDBOT%'"), "got: {sql}");
        assert!(sql.contains("a.username <> 'AHBOT'"), "got: {sql}");
        assert!(
            !sql.contains("acore_playerbots"),
            "the picker must keep working on a box with no playerbots module: {sql}"
        );
        // A customised prefix reaches the SQL — this was hardcoded 'RNDBOT%',
        // so such a server got every bot account in its picker.
        let custom = accounts_sql("fakebot");
        assert!(custom.contains("NOT UPPER(a.username) LIKE 'FAKEBOT%'"), "got: {custom}");
        assert!(!custom.contains("RNDBOT"), "got: {custom}");
    }

    /// The 2026-08-01 incident, pinned: `native-test` had 1000 bot characters
    /// and an EMPTY `playerbots_account_type`, so the registry-only filter
    /// (`account NOT IN (<empty set>)` -> TRUE for every row) listed all of
    /// them on Home as real players. The human filter must therefore never
    /// depend on the registry alone.
    #[test]
    fn who_is_online_does_not_rest_on_the_playerbots_registry_alone() {
        for sql in [players_online_sql("rndbot"), party_online_sql("rndbot")] {
            assert!(
                sql.contains("acore_auth.account WHERE UPPER(username) LIKE 'RNDBOT%'"),
                "an empty playerbots registry would let every bot through: {sql}"
            );
        }
    }

    #[test]
    fn assemble_players_online_shapes_rows_and_guards_numerics() {
        let res = QueryResult {
            columns: vec![],
            rows: vec![
                vec![t("Hypeer"), i(80), i(1), i(12)],
                // empty name -> skipped
                vec![t(""), i(1), i(1), i(1)],
                // non-numeric level/class/zone -> guard to 0
                vec![t("Weird"), t("NULL"), t(""), t("x")],
            ],
        };
        let got = assemble_players_online(&res);
        assert_eq!(
            got,
            json!({"players":[
                {"name":"Hypeer","level":80,"class":1,"zone":12},
                {"name":"Weird","level":0,"class":0,"zone":0}
            ]})
        );
    }

    #[test]
    fn assemble_party_online_shapes_rows_and_skips_empty_guid() {
        let res = QueryResult {
            columns: vec![],
            rows: vec![
                vec![i(2502), t("Hypeer"), i(1), i(80)],
                // empty guid -> skipped
                vec![t(""), t("X"), i(1), i(1)],
                // non-numeric guid/class/level -> guard to 0, same as
                // `players online` (0, never null: the two backends must not
                // disagree on what a malformed row means).
                vec![t("NULL"), t("Weird"), t(""), t("x")],
            ],
        };
        let got = assemble_party_online(&res);
        assert_eq!(
            got,
            json!({"online":[
                {"guid":2502,"name":"Hypeer","class":1,"level":80},
                {"guid":0,"name":"Weird","class":0,"level":0}
            ]})
        );
    }

    // -- items search ---------------------------------------------------

    #[test]
    fn items_search_sql_no_filters() {
        let opts = ItemSearchOpts { name: String::new(), ..Default::default() };
        let (sql, params) = items_search_sql(&opts);
        assert_eq!(
            sql,
            "SELECT entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid \
             FROM item_template WHERE 1=1 ORDER BY RequiredLevel, name LIMIT ?;"
        );
        assert_eq!(params, vec![mysql::Value::from(ITEMS_SEARCH_DEFAULT_LIMIT)]);
    }

    #[test]
    fn items_search_sql_name_only() {
        let opts = ItemSearchOpts { name: "Hearthstone".into(), ..Default::default() };
        let (sql, params) = items_search_sql(&opts);
        assert_eq!(
            sql,
            "SELECT entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid \
             FROM item_template WHERE 1=1 AND name LIKE ? ORDER BY RequiredLevel, name LIMIT ?;"
        );
        assert_eq!(
            params,
            vec![mysql::Value::from("%Hearthstone%"), mysql::Value::from(ITEMS_SEARCH_DEFAULT_LIMIT)]
        );
    }

    #[test]
    fn items_search_sql_all_filters() {
        let opts = ItemSearchOpts {
            name: "Sword".into(),
            quality: Some(4),
            min_level: Some(10),
            max_level: Some(60),
        };
        let (sql, params) = items_search_sql(&opts);
        assert_eq!(
            sql,
            "SELECT entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid \
             FROM item_template WHERE 1=1 AND name LIKE ? AND Quality = ? AND RequiredLevel >= ? \
             AND RequiredLevel <= ? ORDER BY RequiredLevel, name LIMIT ?;"
        );
        assert_eq!(
            params,
            vec![
                mysql::Value::from("%Sword%"),
                mysql::Value::from(4u32),
                mysql::Value::from(10u32),
                mysql::Value::from(60u32),
                mysql::Value::from(ITEMS_SEARCH_DEFAULT_LIMIT),
            ]
        );
    }

    #[test]
    fn assemble_items_search_shapes_rows_and_skips_empty_entry() {
        let res = QueryResult {
            columns: vec![],
            rows: vec![
                vec![i(6948), t("Hearthstone"), i(1), i(1), i(1), i(15), i(0), i(0), i(4341)],
                // empty entry -> skipped
                vec![t(""), t("X"), i(0), i(0), i(0), i(0), i(0), i(0), i(0)],
            ],
        };
        let got = assemble_items_search(&res);
        assert_eq!(
            got,
            json!({"items":[
                {"entry":6948,"name":"Hearthstone","quality":1,"item_level":1,"required_level":1,
                 "class":15,"subclass":0,"inventory_type":0,"displayid":4341}
            ]})
        );
    }

    // -- char-progress / achievements ------------------------------------

    #[test]
    fn extract_char_guid_parses_or_none() {
        let ok = QueryResult { columns: vec![], rows: vec![vec![t("2502")]] };
        assert_eq!(extract_char_guid(&ok), Some(2502));
        let none = QueryResult { columns: vec![], rows: vec![] };
        assert_eq!(extract_char_guid(&none), None);
        let bad = QueryResult { columns: vec![], rows: vec![vec![t("")]] };
        assert_eq!(extract_char_guid(&bad), None);
    }

    #[test]
    fn extract_talent_meta_guards_defaults() {
        let ok = QueryResult { columns: vec![], rows: vec![vec![i(1), i(2)]] };
        assert_eq!(extract_talent_meta(&ok), ("1".to_string(), "2".to_string()));
        let bad = QueryResult { columns: vec![], rows: vec![vec![t("NULL"), t("")]] };
        assert_eq!(extract_talent_meta(&bad), ("0".to_string(), "1".to_string()));
        let empty = QueryResult { columns: vec![], rows: vec![] };
        assert_eq!(extract_talent_meta(&empty), ("0".to_string(), "1".to_string()));
    }

    #[test]
    fn extract_achievement_total_guards_to_zero() {
        let ok = QueryResult { columns: vec![], rows: vec![vec![i(42)]] };
        assert_eq!(extract_achievement_total(&ok), "42");
        let bad = QueryResult { columns: vec![], rows: vec![vec![t("NULL")]] };
        assert_eq!(extract_achievement_total(&bad), "0");
    }

    #[test]
    fn assemble_achievement_entries_skips_bad_id_and_guards_date() {
        let res = QueryResult {
            columns: vec![],
            rows: vec![
                vec![i(6), i(1234567890)],
                vec![t(""), i(1)],       // empty id -> skipped
                vec![t("nope"), i(1)],   // non-numeric id -> skipped
                vec![i(9), t("NULL")],   // non-numeric date -> guarded to 0
            ],
        };
        let got = assemble_achievement_entries(&res);
        assert_eq!(got, vec![json!({"id":6,"date":1234567890}), json!({"id":9,"date":0})]);
    }

    #[test]
    fn assemble_talent_spells_skips_bad_ids() {
        let res = QueryResult {
            columns: vec![],
            rows: vec![vec![i(133)], vec![t("")], vec![t("nope")], vec![i(2050)]],
        };
        let got = assemble_talent_spells(&res);
        assert_eq!(got, vec![json!(133), json!(2050)]);
    }

    #[test]
    fn assemble_char_progress_shapes_envelope() {
        let recent = vec![json!({"id":6,"date":100})];
        let spells = vec![json!(133), json!(2050)];
        let got = assemble_char_progress("42", recent, "2", "1", spells);
        assert_eq!(
            got,
            json!({
                "achievements": {"total":42, "recent":[{"id":6,"date":100}]},
                "talents": {"groups_count":2, "active_group":1, "spells":[133,2050]},
            })
        );
    }
}
