//! Native-mode direct MySQL access (spike: `spike/docker-desktop-native`).
//!
//! WHY THIS EXISTS. In WSL mode every DB-backed page is served by shelling
//! `dml … --json`, which inside the distro runs `docker exec ac-database mysql
//! -uroot -ppassword …`. On Windows-native that path costs ~377ms PER query
//! (bash startup + `docker exec` round-trip) plus a per-row fork storm, so the
//! character/account/statistics pages feel sluggish. In NATIVE mode Docker
//! Desktop publishes the same MySQL server on `127.0.0.1:<port>`, so the
//! launcher can open a DIRECT TCP connection and read the SAME rows in
//! microseconds — no `docker exec`, no bash.
//!
//! This module is the shared foundation the per-page native readers build on
//! (mirroring how [`super::config::ConfigReader`] is the shared foundation for
//! the file-backed pages). It adds NO Tauri commands and is native-mode-only by
//! CONVENTION: WSL mode keeps calling `dml`, so callers gate on
//! [`super::backend::selected`]/`backend_mode()` before reaching here. The data
//! is identical because it is the very same database `dml` talks to.
//!
//! CREDENTIALS. Host `127.0.0.1`, user `root`. Port and password mirror the
//! compose interpolation defaults the native stack runs with
//! (`docker-compose.yml`: `"${DB_EXTERNAL_PORT:-3306}:3306"` and
//! `MYSQL_ROOT_PASSWORD: ${DB_ROOT_PASSWORD:-password}`), with the same `.env`
//! override the CLI's `port-check` reads (`DOCKER_DB_EXTERNAL_PORT`, written
//! when 3306 is busy). See [`resolve_db_config`].
//!
//! ENGINE DOWN. When the server is stopped the DB is unreachable; connecting
//! surfaces [`DbError::Unreachable`] so a caller can report `DB_UNREACHABLE`
//! exactly like `dml` does (its `json_err DB_UNREACHABLE`).

#![allow(dead_code)] // spike foundation: consumed by the per-page reader tasks, not yet by a Tauri command

use dml_core::error::CmdError;

/// Loopback host the native stack publishes MySQL on.
pub const DB_HOST: &str = "127.0.0.1";
/// The only DB user the stack (and `dml`) authenticates as.
pub const DB_USER: &str = "root";
/// Compose default for the published host port (`DB_EXTERNAL_PORT:-3306`).
pub const DEFAULT_DB_PORT: u16 = 3306;
/// Compose default for the root password (`DB_ROOT_PASSWORD:-password`).
pub const DEFAULT_DB_PASSWORD: &str = "password";

/// The four AzerothCore schemas this launcher reads. Names are user-configurable
/// (renamed in the compose env's `AC_*_DATABASE_INFO`, or in `worldserver.conf`'s
/// `*DatabaseInfo` lines), so the enum only picks WHICH schema — the actual name
/// comes from a resolved [`DatabaseNames`]; an enum keeps callers from typo-ing a
/// DB name into a SQL connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Database {
    World,
    Characters,
    Auth,
    Playerbots,
}

impl Database {
    /// The schema name to `USE`, resolved from the server's own conf.
    ///
    /// `Playerbots` is the one exception: it keeps its hardcoded
    /// `"acore_playerbots"` because that name comes from a DIFFERENT evidence
    /// source (`AC_PLAYERBOTS_DATABASE_INFO` in the compose env, not
    /// `worldserver.conf`), and resolving it here would mix two sources for
    /// one value. Follow-up: give it its own conf-resolution path.
    pub fn name(self, names: &DatabaseNames) -> &str {
        match self {
            Database::World => &names.world,
            Database::Characters => &names.characters,
            Database::Auth => &names.auth,
            Database::Playerbots => "acore_playerbots",
        }
    }
}

/// Error code when the server's own config could not answer.
pub const ERR_DB_NAMES_UNRESOLVED: &str = "DB_NAMES_UNRESOLVED";

/// The three schema names, read from the server's own config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseNames {
    pub world: String,
    pub characters: String,
    pub auth: String,
}

/// The schema name out of a `host;port;user;pass;dbname` value.
///
/// SPLIT FROM THE RIGHT. A password may contain semicolons, and indexing from
/// the left would then shift the schema field and connect to whatever happened
/// to land at index 4.
///
/// A value with FEWER than five fields is refused rather than mined for its
/// last one: `"h;3306;u;secret"` (a truncated hand-edit — no dbname) would
/// otherwise resolve the schema to `secret`, the PASSWORD. That is not a loud
/// failure — it is a plausible-looking name that connects nowhere, and the
/// error the user then sees is `db_err_to_cmd`'s "Is ac-database running?"
/// about a server that is up and healthy. Refusing sends them to the conf
/// instead. Found by the final branch review, 2026-08-06.
pub fn parse_database_info(line: &str) -> Option<&str> {
    let v = line.trim().trim_matches('"');
    // Count from the right, since the password may itself contain semicolons:
    // five fields means at least four separators.
    if v.matches(';').count() < 4 {
        return None;
    }
    let (_, last) = v.rsplit_once(';')?;
    let last = last.trim();
    if last.is_empty() { None } else { Some(last) }
}

/// Read all three names for a title dir, or refuse. Never a partial answer and
/// never a default.
///
/// Each name comes from [`super::config::ConfigReader::database_info_value`] —
/// the ONE place that knows the precedence the server itself applies (compose
/// `AC_*_DATABASE_INFO` env → live `worldserver.conf` → `worldserver.conf.dist`).
/// Resolving from the conf ALONE was the original bug: a DML native install
/// carries the real names in the compose environment, and leaves
/// `worldserver.conf` byte-identical to its stock `.dist`.
pub fn database_names_for_title(title_dir: &std::path::Path) -> Option<DatabaseNames> {
    let mut reader = super::config::ConfigReader::for_title(title_dir);
    let mut name = |conf_key: &str| -> Option<String> {
        let raw = reader.database_info_value(conf_key)?;
        parse_database_info(&raw).map(str::to_string)
    };
    Some(DatabaseNames {
        auth: name("LoginDatabaseInfo")?,
        world: name("WorldDatabaseInfo")?,
        characters: name("CharacterDatabaseInfo")?,
    })
}

/// One resolved cell from a result set, typed just enough for faithful JSON
/// assembly: SQL `NULL`, signed/unsigned integers, and everything else as text
/// (the same way the `mysql` CLI renders it). Floats, dates and blobs decode to
/// their string form so a reader can pass them straight through to JSON.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Int(i64),
    Text(String),
}

impl SqlValue {
    /// JSON projection: `Null -> null`, `Int -> number`, `Text -> string`.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            SqlValue::Null => serde_json::Value::Null,
            SqlValue::Int(i) => serde_json::Value::from(*i),
            SqlValue::Text(s) => serde_json::Value::String(s.clone()),
        }
    }
}

/// A decoded result set: column names in select order plus the rows, each a
/// vector of [`SqlValue`] aligned to `columns`.
#[derive(Debug, Clone, PartialEq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<SqlValue>>,
}

/// Why a DB read failed, split so callers can map the "server is down" case to
/// the same `DB_UNREACHABLE` code `dml` emits while still surfacing genuine SQL
/// errors (bad table, syntax) distinctly.
#[derive(Debug)]
pub enum DbError {
    /// Could not open a connection — engine/DB down, wrong port, refused.
    Unreachable(String),
    /// Connected, but running the query failed (SQL/protocol error).
    Query(String),
    /// The server's own config could not answer the schema names (no
    /// `AC_*_DATABASE_INFO` in the compose env AND no `*DatabaseInfo` key in
    /// `worldserver.conf`/its `.dist`) — see [`database_names_for_title`].
    /// REFUSED, never a fallback to a guessed name.
    NamesUnresolved(String),
}

impl DbError {
    /// The stable code a caller reports, matching the CLI's `json_err` codes.
    pub fn code(&self) -> &'static str {
        match self {
            DbError::Unreachable(_) => "DB_UNREACHABLE",
            DbError::Query(_) => "DB_QUERY_FAILED",
            DbError::NamesUnresolved(_) => ERR_DB_NAMES_UNRESOLVED,
        }
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Unreachable(m) => write!(f, "{m}"),
            DbError::Query(m) => write!(f, "{m}"),
            DbError::NamesUnresolved(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for DbError {}

/// Connection parameters for the native stack's MySQL server.
#[derive(Debug, Clone, PartialEq)]
pub struct DbConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    /// The schema names read from the server's own config (compose env first,
    /// then `worldserver.conf`, then its `.dist`), or `None` when none of them
    /// answered. NEVER a default — a caller reaching for a name resolves
    /// against this and gets [`DbError::NamesUnresolved`] rather than a guessed
    /// `acore_*` name.
    pub db_names: Option<DatabaseNames>,
}

impl DbConfig {
    /// Resolve from the real process environment plus the title dir's `.env`
    /// (the file Compose reads for `${VAR}` interpolation), and the schema
    /// names via [`database_names_for_title`]. Title dir is
    /// `DML_GAMES_DIR/wow-server-playerbots`, shared with
    /// [`super::config::ConfigReader::title_dir_from_env`].
    pub fn from_env() -> Self {
        Self::for_title(&super::config::ConfigReader::title_dir_from_env())
    }

    /// [`Self::from_env`] with the title dir supplied instead of read out of
    /// `DML_GAMES_DIR` — same `.env` and schema-name resolution, one explicit
    /// server.
    ///
    /// Exists for the live parity suites: each carries its OWN `games_dir()`
    /// (env var, else the snapshot server's path) and hands it to the bash
    /// oracle explicitly, so the in-process half has to be able to name the
    /// same server. Since schema names stopped being hardcoded, an unset
    /// `DML_GAMES_DIR` left `db_names` `None` and every native read refused
    /// with `DB_NAMES_UNRESOLVED` while bash — handed `games_dir()` — answered
    /// normally: 13 live tests failed or skipped for a reason that had nothing
    /// to do with the code under test.
    pub fn for_title(title_dir: &std::path::Path) -> Self {
        let dotenv = std::fs::read_to_string(title_dir.join(".env")).ok();
        let mut cfg = resolve_db_config(|k| std::env::var(k).ok(), dotenv.as_deref());
        cfg.db_names = database_names_for_title(title_dir);
        cfg
    }

    /// A [`mysql::Opts`] pointed at `db`, built from these params. Split out so
    /// tests can assert the shape without opening a socket. Refuses with
    /// [`DbError::NamesUnresolved`] rather than falling back to a guessed name
    /// when [`Self::db_names`] never resolved.
    fn opts(&self, db: Database) -> Result<mysql::Opts, DbError> {
        let names = self.db_names.as_ref().ok_or_else(|| {
            DbError::NamesUnresolved(
                "could not read the schema names from the server's compose env or worldserver.conf"
                    .to_string(),
            )
        })?;
        let db_name = db.name(names).to_string();
        Ok(mysql::OptsBuilder::new()
            .ip_or_hostname(Some(self.host.clone()))
            .tcp_port(self.port)
            .user(Some(self.user.clone()))
            .pass(Some(self.password.clone()))
            .db_name(Some(db_name))
            // Fail fast when the engine is down rather than hanging the UI: a
            // few seconds is plenty for a loopback connect.
            .tcp_connect_timeout(Some(std::time::Duration::from_secs(5)))
            // A stalled/wedged engine (connected but not answering) must also
            // surface as an error instead of hanging the Tauri command
            // forever -- 30s is generous for a loopback query but still
            // bounded (review finding, 2026-07-24).
            .read_timeout(Some(std::time::Duration::from_secs(30)))
            .write_timeout(Some(std::time::Duration::from_secs(30)))
            // Pin the session charset explicitly rather than relying on
            // whatever the handshake negotiates -- cheap defense-in-depth
            // alongside bound parameters (see dml::pages/dml::paperdoll): a
            // narrow negotiated charset is the classic precondition for the
            // multi-byte-escape SQL-injection class that bound params are
            // meant to close outright (review finding, 2026-07-24).
            .init(vec!["SET NAMES utf8mb4"])
            .into())
    }
}

/// Look one key out of a parsed `.env`-style text: `KEY=VALUE` lines, `#`
/// comments and blanks ignored, surrounding single/double quotes stripped. The
/// first match wins (Compose semantics). Pure, for testing.
pub fn dotenv_get(text: &str, key: &str) -> Option<String> {
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() != key {
            continue;
        }
        let v = v.trim();
        let v = v
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
            .unwrap_or(v);
        return Some(v.to_string());
    }
    None
}

/// Resolve [`DbConfig`] from an environment getter and an optional `.env` text.
/// Pure, so the precedence is unit-tested without touching the process env.
///
/// Port precedence (first non-empty, valid `u16` wins): env
/// `DOCKER_DB_EXTERNAL_PORT` → env `DB_EXTERNAL_PORT` → `.env`
/// `DOCKER_DB_EXTERNAL_PORT` → `.env` `DB_EXTERNAL_PORT` → `3306`. Password:
/// env `DB_ROOT_PASSWORD` → `.env` `DB_ROOT_PASSWORD` → `"password"`.
pub fn resolve_db_config(
    env_get: impl Fn(&str) -> Option<String>,
    dotenv_text: Option<&str>,
) -> DbConfig {
    let de = |key: &str| dotenv_text.and_then(|t| dotenv_get(t, key));

    let port = [
        env_get("DOCKER_DB_EXTERNAL_PORT"),
        env_get("DB_EXTERNAL_PORT"),
        de("DOCKER_DB_EXTERNAL_PORT"),
        de("DB_EXTERNAL_PORT"),
    ]
    .into_iter()
    .flatten()
    .find_map(|v| v.trim().parse::<u16>().ok().filter(|p| *p != 0))
    .unwrap_or(DEFAULT_DB_PORT);

    let password = env_get("DB_ROOT_PASSWORD")
        .or_else(|| de("DB_ROOT_PASSWORD"))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_DB_PASSWORD.to_string());

    // db_names is deliberately NOT resolved here: this function is the pure
    // host/port/user/password precedence logic, unit-tested without touching
    // the filesystem. Schema-name resolution (which DOES touch the
    // filesystem, reading worldserver.conf) happens only in `from_env`.
    DbConfig {
        host: DB_HOST.to_string(),
        port,
        user: DB_USER.to_string(),
        password,
        db_names: None,
    }
}

/// Decode one `mysql::Value` into a JSON-ready [`SqlValue`]. Integers stay
/// numeric; everything else (bytes, floats, dates, times) renders to the same
/// text the `mysql` CLI would print, so readers get faithful strings.
fn convert_value(v: mysql::Value) -> SqlValue {
    use mysql::Value as V;
    match v {
        V::NULL => SqlValue::Null,
        V::Int(i) => SqlValue::Int(i),
        // Unsigned values that overflow i64 degrade to text rather than wrap.
        V::UInt(u) => match i64::try_from(u) {
            Ok(i) => SqlValue::Int(i),
            Err(_) => SqlValue::Text(u.to_string()),
        },
        V::Bytes(b) => SqlValue::Text(String::from_utf8_lossy(&b).into_owned()),
        V::Float(f) => SqlValue::Text(f.to_string()),
        V::Double(d) => SqlValue::Text(d.to_string()),
        // Dates/times: reuse the driver's own display (matches CLI text form).
        other => SqlValue::Text(other.as_sql(false).trim_matches('\'').to_string()),
    }
}

/// Open a connection to `db` using `cfg`. Any failure is [`DbError::Unreachable`]
/// — connecting IS the reachability probe, so a refused socket, a down engine or
/// bad creds all read as "the DB could not be reached".
///
/// DELIBERATELY opens a fresh TCP connection per call rather than drawing from
/// a pool (a separate, deferred review finding) — a loopback connect is cheap
/// relative to the per-`docker exec` cost this module replaces, and every
/// reader here is one-or-a-few queries per page load, not a hot inner loop.
/// Pooling stays out of scope for this pass; revisit if a reader's connect
/// overhead ever shows up in practice.
pub fn connect(cfg: &DbConfig, db: Database) -> Result<mysql::Conn, DbError> {
    let opts = cfg.opts(db)?;
    mysql::Conn::new(opts)
        .map_err(|e| DbError::Unreachable(format!("Could not reach the database: {e}")))
}

/// Run `sql` against `db` (no bound parameters) and decode the whole result
/// set. A thin convenience wrapper over [`query_with_params`] for the readers'
/// fixed queries (nothing user-controlled spliced in). SYNCHRONOUS and
/// BLOCKING — see [`query_with_params`] for the full contract.
pub fn query(cfg: &DbConfig, db: Database, sql: &str) -> Result<QueryResult, DbError> {
    query_with_params(cfg, db, sql, Vec::<mysql::Value>::new())
}

/// Run `sql` against `db` with `params` bound to its `?` placeholders (in
/// order) and decode the whole result set. Connection failures map to
/// [`DbError::Unreachable`]; a failure once connected (bad SQL, a param-count
/// mismatch) maps to [`DbError::Query`]. SYNCHRONOUS and BLOCKING — call it
/// inside `tauri::async_runtime::spawn_blocking` so it never blocks the async
/// runtime.
///
/// The query runs over the PREPARED (binary) protocol (`exec_iter`), NOT the
/// text protocol: the text protocol hands every column back as raw bytes, so
/// an integer column would decode to `Text("5")`; the binary protocol
/// preserves the server's real column type so integers arrive as
/// [`SqlValue::Int`] — the "integer columns faithfully" the readers need.
/// Binding `params` (rather than splicing an escaped string literal into
/// `sql`) is also the load-bearing SQL-injection defense for every builder
/// that carries user free-text (teleport search, bot-name prefix, character
/// name): the driver sends bound values out-of-band from the statement text,
/// so there is no SQL text for a quote/backslash/charset trick to corrupt,
/// independent of the connection's `sql_mode`.
pub fn query_with_params(
    cfg: &DbConfig,
    db: Database,
    sql: &str,
    params: impl Into<mysql::Params>,
) -> Result<QueryResult, DbError> {
    use mysql::prelude::Queryable;
    let mut conn = connect(cfg, db)?;
    let mut result = conn
        .exec_iter(sql, params)
        .map_err(|e| DbError::Query(format!("Query failed: {e}")))?;

    let columns: Vec<String> = result
        .columns()
        .as_ref()
        .iter()
        .map(|c| c.name_str().into_owned())
        .collect();

    let mut rows = Vec::new();
    for row in result.by_ref() {
        let row = row.map_err(|e| DbError::Query(format!("Row decode failed: {e}")))?;
        let cells = row.unwrap().into_iter().map(convert_value).collect();
        rows.push(cells);
    }
    Ok(QueryResult { columns, rows })
}

/// Execute a write (UPDATE/INSERT/DELETE) with bound params over the prepared
/// (binary) protocol and return the affected row count. Bound params keep
/// user data out of the SQL text -- same injection rationale as
/// [`query_with_params`], just for `exec_drop` instead of `exec_iter`. This is
/// the FIRST write path in the native core (Task A2c, `gm return-home`'s
/// offline arm): every reader above only ever `SELECT`s. SYNCHRONOUS and
/// BLOCKING like [`query_with_params`] -- call it inside
/// `tauri::async_runtime::spawn_blocking`.
pub fn execute(
    cfg: &DbConfig,
    db: Database,
    sql: &str,
    params: impl Into<mysql::Params>,
) -> Result<u64, DbError> {
    use mysql::prelude::Queryable;
    let mut conn = connect(cfg, db)?;
    conn.exec_drop(sql, params)
        .map_err(|e| DbError::Query(format!("Query failed: {e}")))?;
    Ok(conn.affected_rows())
}

/// Decode one `characters`-row cell to `i64`, tolerating both the binary
/// protocol's native `Int`/`UInt` decode and (defensively) a text fallback --
/// `guid`/`race`/`online` are all integer columns, but [`db::SqlValue`]'s
/// `Text` variant is the safe catch-all for anything the driver didn't map to
/// `Int`.
pub fn sql_row_int(v: Option<&crate::db::SqlValue>) -> Option<i64> {
    match v {
        Some(crate::db::SqlValue::Int(i)) => Some(*i),
        Some(crate::db::SqlValue::Text(s)) => s.parse::<i64>().ok(),
        _ => None,
    }
}

pub fn cell_string(v: Option<&crate::db::SqlValue>) -> Option<String> {
    match v {
        Some(crate::db::SqlValue::Text(s)) if !s.is_empty() => Some(s.clone()),
        Some(crate::db::SqlValue::Int(i)) => Some(i.to_string()),
        _ => None,
    }
}

pub fn db_unreachable_err(message: impl Into<String>) -> CmdError {
    CmdError { code: "DB_UNREACHABLE".into(), message: message.into(), hint: "Is ac-database running?".into() }
}

/// Map a native-mode [`crate::db::DbError`] to the [`CmdError`] the frontend
/// already knows how to render. `Unreachable`/`Query` collapse to
/// `DB_UNREACHABLE`, matching the CLI: every one of these arms
/// (`teleport-list` / `bots list` / `accounts` / `paperdoll`) reports
/// `DB_UNREACHABLE` for ANY `db_*_query` failure in `90-main.sh` — the bash has
/// no separate "connected but the query itself failed" code path, so a native
/// `DbError::Query` (e.g. a genuinely malformed statement) must still read as
/// `DB_UNREACHABLE` to stay byte-identical to `dml`. Same collapse
/// [`stats_err_to_cmd`] already does for the `stats` arm — see its comment for
/// the fuller rationale.
///
/// `NamesUnresolved` is NOT part of that collapse: bash `dml` never reads
/// `*DatabaseInfo` at all (it uses its own hardcoded `acore_*` constants), so
/// this failure mode does not exist on that side to be byte-identical with —
/// it is native-only, and folding it into `DB_UNREACHABLE` would tell the user
/// "is the engine down?" for a server whose config never names its schemas,
/// which is a different question with a different fix.
pub fn db_err_to_cmd(e: crate::db::DbError) -> CmdError {
    match &e {
        crate::db::DbError::NamesUnresolved(_) => CmdError {
            code: crate::db::ERR_DB_NAMES_UNRESOLVED.into(),
            message: e.to_string(),
            // Names DML_GAMES_DIR, because on the CLI that is the likeliest
            // cause by far: with it unset the title dir resolves to a RELATIVE
            // `./wow-server-playerbots` (config.rs), so a command run from any
            // other directory refuses. A hint that only describes the symptom
            // leaves the user editing a conf that was never the problem.
            // (The launcher is unaffected — startup.rs exports the var into its
            // own process before any in-process DB read.)
            hint: "Could not read the schema names from the server's compose env or \
                   worldserver.conf. Is DML_GAMES_DIR set to the directory holding \
                   your server?"
                .into(),
        },
        _ => CmdError {
            code: "DB_UNREACHABLE".into(),
            message: e.to_string(),
            hint: "Is ac-database running? (native mode reads MySQL directly on 127.0.0.1)".into(),
        },
    }
}

/// One row's `COUNT(*)` decoded as `i64` (defaulting to 0 on anything odd —
/// every caller only ever asks "is this nonzero").
pub fn count_result(res: crate::db::QueryResult) -> i64 {
    sql_row_int(res.rows.first().and_then(|r| r.first())).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The value is `host;port;user;pass;dbname` — the schema is the LAST
    /// field, and the password may itself contain anything, so split from the
    /// right and take one field. Never parse left-to-right by index.
    #[test]
    fn the_schema_is_the_last_semicolon_field() {
        assert_eq!(
            parse_database_info("127.0.0.1;3306;acore;pw;acore_world"),
            Some("acore_world")
        );
        assert_eq!(
            parse_database_info("\"127.0.0.1;3306;acore;p;w;d;my_world\""),
            Some("my_world"),
            "a password containing semicolons must not shift the schema field"
        );
        assert_eq!(parse_database_info(""), None);
        assert_eq!(parse_database_info("nosemicolons"), None);
    }

    /// A short value must REFUSE, not hand back the password as a schema name.
    #[test]
    fn a_truncated_value_refuses_rather_than_returning_the_password() {
        assert_eq!(
            parse_database_info("127.0.0.1;3306;acore;hunter2"),
            None,
            "four fields means no dbname; returning the last one hands back the PASSWORD \
             as a schema, which connects nowhere and reports the failure as \
             'Is ac-database running?' about a healthy server"
        );
        assert_eq!(parse_database_info("127.0.0.1;3306;acore"), None);
        assert_eq!(parse_database_info("a;b"), None);
        // The boundary: exactly five fields is the shortest legal value.
        assert_eq!(parse_database_info("h;3306;u;p;d"), Some("d"));
        // And a semicolon-laden password still resolves, since the count is a
        // FLOOR — it must not become a second way to refuse a legal value.
        assert_eq!(parse_database_info("h;3306;u;p;w;d;my_world"), Some("my_world"));
    }

    // -----------------------------------------------------------------------
    // Schema-name resolution fixtures. These build a real title dir on disk
    // rather than feeding a conf STRING, because the defect being pinned is
    // precisely that the conf is not the only source (nor the winning one).
    // -----------------------------------------------------------------------

    /// What a DML native install actually leaves in `worldserver.conf`: the
    /// stock `.dist` values, naming a host the worldserver container cannot
    /// even reach. Verified byte-for-byte on the live install at
    /// `C:\Users\perzi\dml-native\wow-server-playerbots` (2026-08-06) — the
    /// conf and its `.dist` are the same size and carry these three lines.
    const STOCK_CONF: &str = "\
LoginDatabaseInfo     = \"127.0.0.1;3306;acore;acore;acore_auth\"
WorldDatabaseInfo     = \"127.0.0.1;3306;acore;acore;acore_world\"
CharacterDatabaseInfo = \"127.0.0.1;3306;acore;acore;acore_characters\"
";

    /// A compose document carrying `services.ac-worldserver.environment` —
    /// the shape `data/native-compose.yml.tmpl` generates and the shape
    /// [`crate::config::parse_override_env`] reads.
    fn compose_yaml(entries: &[(&str, &str)]) -> String {
        let mut s = String::from("services:\n  ac-worldserver:\n    environment:\n");
        for (k, v) in entries {
            s.push_str(&format!("      {k}: \"{v}\"\n"));
        }
        s
    }

    /// `AC_*_DATABASE_INFO` entries naming `<prefix>_auth/_world/_chars`, in
    /// the `Host;Port;User;Pass;DBName` form the compose env really uses.
    fn db_info_entries(prefix: &str) -> Vec<(String, String)> {
        [
            ("AC_LOGIN_DATABASE_INFO", "auth"),
            ("AC_WORLD_DATABASE_INFO", "world"),
            ("AC_CHARACTER_DATABASE_INFO", "chars"),
        ]
        .iter()
        .map(|(k, sfx)| ((*k).to_string(), format!("ac-database;3306;root;pw;{prefix}_{sfx}")))
        .collect()
    }

    fn compose_db_info(prefix: &str) -> String {
        let owned = db_info_entries(prefix);
        let refs: Vec<(&str, &str)> =
            owned.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        compose_yaml(&refs)
    }

    /// Build a title-dir fixture. Each source is optional so a test can leave
    /// exactly the tiers it means to exercise.
    fn title_fixture(
        name: &str,
        base_compose: Option<&str>,
        override_compose: Option<&str>,
        conf: Option<&str>,
        dist: Option<&str>,
    ) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("dml-dbnames-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let etc = dir.join("env").join("dist").join("etc");
        std::fs::create_dir_all(&etc).unwrap();
        if let Some(t) = base_compose {
            std::fs::write(dir.join("docker-compose.yml"), t).unwrap();
        }
        if let Some(t) = override_compose {
            std::fs::write(dir.join("docker-compose.override.yml"), t).unwrap();
        }
        if let Some(t) = conf {
            std::fs::write(etc.join("worldserver.conf"), t).unwrap();
        }
        if let Some(t) = dist {
            std::fs::write(etc.join("worldserver.conf.dist"), t).unwrap();
        }
        dir
    }

    fn triple(n: &DatabaseNames) -> (&str, &str, &str) {
        (n.auth.as_str(), n.world.as_str(), n.characters.as_str())
    }

    /// THE regression test for the defect this round exists to fix. A DML
    /// native install writes the real schema names into the compose
    /// `environment:` and leaves `worldserver.conf` byte-identical to its
    /// stock `.dist` — and an `AC_*` env var OVERRIDES the matching conf key
    /// (composegen's shadowing rule, `composegen.rs:47-53`). A resolver that
    /// reads only the conf therefore answers `acore_*` for a server that is
    /// using something else entirely: the exact bug Task 5 exists to remove.
    ///
    /// All three names are asserted INDIVIDUALLY and the conf supplies a
    /// (wrong) value for each, so dropping the env tier for any ONE key is
    /// caught on its own rather than being masked by its siblings.
    #[test]
    fn the_compose_env_beats_the_conf_the_server_ignores() {
        let dir = title_fixture(
            "env-beats-conf",
            Some(&compose_db_info("renamed")),
            None,
            Some(STOCK_CONF),
            Some(STOCK_CONF),
        );
        let n = database_names_for_title(&dir).expect("the compose env answers all three");
        assert_eq!(
            triple(&n),
            ("renamed_auth", "renamed_world", "renamed_chars"),
            "AC_*_DATABASE_INFO must win: it is what worldserver reads, and the \
             on-disk conf on a real install is untouched stock"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Compose's own later-file-wins rule: `docker-compose.override.yml` is
    /// merged on top of the base file, so an `AC_*_DATABASE_INFO` there beats
    /// the base compose's.
    #[test]
    fn the_override_compose_beats_the_base_compose() {
        let dir = title_fixture(
            "override-beats-base",
            Some(&compose_db_info("base")),
            Some(&compose_db_info("ovr")),
            Some(STOCK_CONF),
            None,
        );
        let n = database_names_for_title(&dir).expect("both compose files answer");
        assert_eq!(triple(&n), ("ovr_auth", "ovr_world", "ovr_chars"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// With no compose env at all the conf is still read — and READ, not
    /// assumed. (This is the tier the WSL-era/hand-rolled stacks rely on.)
    #[test]
    fn conf_names_are_read_not_assumed() {
        let conf = "\
LoginDatabaseInfo     = \"127.0.0.1;3306;acore;pw;my_auth\"
WorldDatabaseInfo     = \"127.0.0.1;3306;acore;pw;my_world\"
CharacterDatabaseInfo = \"127.0.0.1;3306;acore;pw;my_chars\"
";
        let dir = title_fixture("conf-only", None, None, Some(conf), None);
        let n = database_names_for_title(&dir).expect("all three keys present");
        assert_eq!(triple(&n), ("my_auth", "my_world", "my_chars"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `worldserver.conf` that EXISTS but omits a `*DatabaseInfo` key is
    /// legal — AC merges the `.conf` over the `.conf.dist` — so the resolver
    /// must fall through to the `.dist` rather than refusing. `conf_path_in`
    /// has no `.dist` fallback of its own; every caller adds one, and this
    /// pins that this caller does too.
    #[test]
    fn a_conf_that_omits_the_keys_falls_through_to_the_dist() {
        let dir = title_fixture(
            "dist-fallback",
            None,
            None,
            Some("Rate.XP.Kill = 3\n"),
            Some(STOCK_CONF),
        );
        let n = database_names_for_title(&dir).expect(".dist answers what the conf omits");
        assert_eq!(triple(&n), ("acore_auth", "acore_world", "acore_characters"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// REFUSE, do not fall back. Falling back to the old constants restores the
    /// exact bug this change exists to fix, invisibly — the reader would go on
    /// answering ok:true against a database that is not the server's.
    #[test]
    fn a_missing_key_refuses_rather_than_defaulting() {
        let dir = title_fixture(
            "missing-key",
            None,
            None,
            Some("LoginDatabaseInfo = \"h;3306;u;p;my_auth\"\n"),
            None,
        );
        assert_eq!(database_names_for_title(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Nothing anywhere — no compose, no conf, no `.dist` — is a refusal, not
    /// a guess. This is the state a not-yet-installed title is in, and it is
    /// what `DB_NAMES_UNRESOLVED` reports.
    #[test]
    fn an_empty_title_dir_refuses() {
        let dir = title_fixture("empty", None, None, None, None);
        assert_eq!(database_names_for_title(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// ISOLATES each key's refusal from the other two. The test above leaves
    /// BOTH `WorldDatabaseInfo` and `CharacterDatabaseInfo` absent, so a
    /// broken (fallback-instead-of-refuse) mutation on EITHER of those two
    /// fields alone still returns `None` overall, because the OTHER missing
    /// field's own (unmutated) `?` masks it — proved live: reintroducing
    /// `.unwrap_or("acore_world")` on just the `world` field left that test
    /// green. This is the "fix with two/three independent halves is not
    /// proven by a sequence test" trap this repo has hit before (see the
    /// crates/CLAUDE.md gotcha). Each case below supplies every OTHER key
    /// with a real value, so only the one key under test can make it fail.
    #[test]
    fn each_missing_key_refuses_on_its_own_not_only_in_combination() {
        let cases: [(&str, &str); 3] = [
            (
                "world",
                "LoginDatabaseInfo     = \"h;3306;u;p;a\"\n\
                 CharacterDatabaseInfo = \"h;3306;u;p;c\"\n",
            ),
            (
                "auth",
                "WorldDatabaseInfo     = \"h;3306;u;p;w\"\n\
                 CharacterDatabaseInfo = \"h;3306;u;p;c\"\n",
            ),
            (
                "characters",
                "LoginDatabaseInfo = \"h;3306;u;p;a\"\n\
                 WorldDatabaseInfo = \"h;3306;u;p;w\"\n",
            ),
        ];
        for (missing, conf) in cases {
            let dir = title_fixture(&format!("missing-{missing}"), None, None, Some(conf), None);
            assert_eq!(
                database_names_for_title(&dir),
                None,
                "missing only {missing} must still refuse, not partially default"
            );
            let _ = std::fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn commented_out_keys_are_not_read() {
        let conf = "\
# WorldDatabaseInfo = \"h;3306;u;p;WRONG\"
LoginDatabaseInfo     = \"h;3306;u;p;a\"
WorldDatabaseInfo     = \"h;3306;u;p;w\"
CharacterDatabaseInfo = \"h;3306;u;p;c\"
";
        let dir = title_fixture("commented", None, None, Some(conf), None);
        let n = database_names_for_title(&dir).expect("all three present");
        assert_eq!(n.world, "w", "a commented key must not win");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn database_names_match_the_server_schemas() {
        let names = DatabaseNames {
            world: "acore_world".to_string(),
            characters: "acore_characters".to_string(),
            auth: "acore_auth".to_string(),
        };
        assert_eq!(Database::World.name(&names), "acore_world");
        assert_eq!(Database::Characters.name(&names), "acore_characters");
        assert_eq!(Database::Auth.name(&names), "acore_auth");
        assert_eq!(Database::Playerbots.name(&names), "acore_playerbots");
    }

    #[test]
    fn resolve_defaults_when_nothing_set() {
        let cfg = resolve_db_config(|_| None, None);
        assert_eq!(cfg.host, "127.0.0.1");
        assert_eq!(cfg.user, "root");
        assert_eq!(cfg.port, 3306);
        assert_eq!(cfg.password, "password");
    }

    #[test]
    fn resolve_env_port_precedence() {
        // DOCKER_DB_EXTERNAL_PORT beats DB_EXTERNAL_PORT.
        let cfg = resolve_db_config(
            |k| match k {
                "DOCKER_DB_EXTERNAL_PORT" => Some("13306".into()),
                "DB_EXTERNAL_PORT" => Some("23306".into()),
                _ => None,
            },
            None,
        );
        assert_eq!(cfg.port, 13306);
        // Falls through to DB_EXTERNAL_PORT when the first is unset.
        let cfg = resolve_db_config(
            |k| (k == "DB_EXTERNAL_PORT").then(|| "23306".into()),
            None,
        );
        assert_eq!(cfg.port, 23306);
    }

    #[test]
    fn resolve_env_beats_dotenv() {
        let dotenv = "DOCKER_DB_EXTERNAL_PORT=13306\nDB_ROOT_PASSWORD=fromfile\n";
        let cfg = resolve_db_config(
            |k| match k {
                "DOCKER_DB_EXTERNAL_PORT" => Some("40000".into()),
                "DB_ROOT_PASSWORD" => Some("fromenv".into()),
                _ => None,
            },
            Some(dotenv),
        );
        assert_eq!(cfg.port, 40000);
        assert_eq!(cfg.password, "fromenv");
    }

    #[test]
    fn resolve_dotenv_fallback() {
        // No env vars -> the title dir's .env supplies both.
        let dotenv = "# comment\nDOCKER_DB_EXTERNAL_PORT=13306\nDB_ROOT_PASSWORD=\"s3cret\"\n";
        let cfg = resolve_db_config(|_| None, Some(dotenv));
        assert_eq!(cfg.port, 13306);
        assert_eq!(cfg.password, "s3cret");
    }

    #[test]
    fn resolve_ignores_blank_and_invalid_port() {
        // Empty / non-numeric / zero / out-of-range values are skipped, not
        // treated as 0 or a panic — resolution falls through to the default.
        let cfg = resolve_db_config(
            |k| match k {
                "DOCKER_DB_EXTERNAL_PORT" => Some("".into()),
                "DB_EXTERNAL_PORT" => Some("notaport".into()),
                _ => None,
            },
            Some("DOCKER_DB_EXTERNAL_PORT=0\nDB_EXTERNAL_PORT=99999\n"),
        );
        assert_eq!(cfg.port, 3306);
        // An empty password env var must not win over the default either.
        let cfg = resolve_db_config(|k| (k == "DB_ROOT_PASSWORD").then(String::new), None);
        assert_eq!(cfg.password, "password");
    }

    #[test]
    fn dotenv_get_parses_lines() {
        let text = "\
# a comment
export DB_ROOT_PASSWORD='quoted'
DOCKER_DB_EXTERNAL_PORT = 13306
DB_EXTERNAL_PORT=\"7\"
BLANK=
";
        assert_eq!(dotenv_get(text, "DB_ROOT_PASSWORD").as_deref(), Some("quoted"));
        // Surrounding blanks around key and value are trimmed.
        assert_eq!(dotenv_get(text, "DOCKER_DB_EXTERNAL_PORT").as_deref(), Some("13306"));
        assert_eq!(dotenv_get(text, "DB_EXTERNAL_PORT").as_deref(), Some("7"));
        assert_eq!(dotenv_get(text, "BLANK").as_deref(), Some(""));
        assert_eq!(dotenv_get(text, "MISSING"), None);
    }

    #[test]
    fn dotenv_get_first_match_wins() {
        let text = "PORT=1\nPORT=2\n";
        assert_eq!(dotenv_get(text, "PORT").as_deref(), Some("1"));
    }

    #[test]
    fn sql_value_json_projection() {
        assert_eq!(SqlValue::Null.to_json(), serde_json::Value::Null);
        assert_eq!(SqlValue::Int(42).to_json(), serde_json::json!(42));
        assert_eq!(SqlValue::Text("hi".into()).to_json(), serde_json::json!("hi"));
    }

    #[test]
    fn convert_value_maps_types_faithfully() {
        assert_eq!(convert_value(mysql::Value::NULL), SqlValue::Null);
        assert_eq!(convert_value(mysql::Value::Int(-5)), SqlValue::Int(-5));
        assert_eq!(convert_value(mysql::Value::UInt(9)), SqlValue::Int(9));
        // Bytes decode as UTF-8 text (the common column case: VARCHAR/name).
        assert_eq!(
            convert_value(mysql::Value::Bytes(b"Bob".to_vec())),
            SqlValue::Text("Bob".into())
        );
        // UInt beyond i64::MAX degrades to text rather than wrapping negative.
        assert_eq!(
            convert_value(mysql::Value::UInt(u64::MAX)),
            SqlValue::Text(u64::MAX.to_string())
        );
    }

    #[test]
    fn db_error_codes() {
        assert_eq!(DbError::Unreachable("x".into()).code(), "DB_UNREACHABLE");
        assert_eq!(DbError::Query("x".into()).code(), "DB_QUERY_FAILED");
        assert_eq!(DbError::NamesUnresolved("x".into()).code(), ERR_DB_NAMES_UNRESOLVED);
    }

    fn test_names() -> DatabaseNames {
        DatabaseNames {
            world: "acore_world".to_string(),
            characters: "acore_characters".to_string(),
            auth: "acore_auth".to_string(),
        }
    }

    #[test]
    fn opts_carry_host_port_user_and_db() {
        let cfg = DbConfig {
            host: "127.0.0.1".into(),
            port: 13306,
            user: "root".into(),
            password: "password".into(),
            db_names: Some(test_names()),
        };
        let opts = cfg.opts(Database::Characters).expect("names resolved");
        assert_eq!(opts.get_ip_or_hostname(), "127.0.0.1");
        assert_eq!(opts.get_tcp_port(), 13306);
        assert_eq!(opts.get_user().as_deref(), Some("root"));
        assert_eq!(opts.get_db_name().as_deref(), Some("acore_characters"));
    }

    #[test]
    fn opts_carry_timeouts_and_pin_charset() {
        // Finding #3/#4: a stalled DB must surface as an error instead of
        // hanging the Tauri command forever, and the session charset is
        // pinned explicitly rather than left to negotiation.
        let cfg = DbConfig {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: "password".into(),
            db_names: Some(test_names()),
        };
        let opts = cfg.opts(Database::Characters).expect("names resolved");
        assert_eq!(opts.get_tcp_connect_timeout(), Some(std::time::Duration::from_secs(5)));
        assert_eq!(opts.get_read_timeout(), Some(&std::time::Duration::from_secs(30)));
        assert_eq!(opts.get_write_timeout(), Some(&std::time::Duration::from_secs(30)));
        assert_eq!(opts.get_init(), vec!["SET NAMES utf8mb4".to_string()]);
    }

    /// The refusal is load-bearing at the `opts` boundary too, not only in
    /// `database_names_from_conf`: a `DbConfig` whose names never resolved
    /// must refuse to build ANY connection options rather than falling back
    /// to a guessed `acore_*` name.
    #[test]
    fn opts_refuses_when_names_are_unresolved() {
        let cfg = DbConfig {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: "password".into(),
            db_names: None,
        };
        let err = cfg.opts(Database::Characters).expect_err("names unresolved");
        assert_eq!(err.code(), ERR_DB_NAMES_UNRESOLVED);
    }

    /// `Playerbots` is the one variant that does NOT need resolved names —
    /// its literal comes from a different evidence source entirely — but
    /// `opts` still refuses when `db_names` is `None`, because [`Database::name`]
    /// unconditionally takes a `&DatabaseNames` argument. This pins that
    /// documented (not accidental) coupling so a future change either keeps it
    /// or updates this test deliberately.
    #[test]
    fn opts_for_playerbots_still_requires_resolved_names() {
        let cfg = DbConfig {
            host: "127.0.0.1".into(),
            port: 3306,
            user: "root".into(),
            password: "password".into(),
            db_names: None,
        };
        assert!(cfg.opts(Database::Playerbots).is_err());

        let cfg = DbConfig { db_names: Some(test_names()), ..cfg };
        let opts = cfg.opts(Database::Playerbots).expect("names resolved");
        assert_eq!(opts.get_db_name().as_deref(), Some("acore_playerbots"));
    }

    #[test]
    fn execute_accepts_bound_positional_params() {
        // Pure shape check (no socket), mirroring
        // query_with_params_accepts_bound_positional_params below -- the
        // live roundtrip for an actual write isn't testable without a
        // reachable, mutable DB (server down; see the A2c brief).
        let params: Vec<mysql::Value> = vec![
            mysql::Value::from(-8819.3f64),
            mysql::Value::from(636.2f64),
            mysql::Value::from(94.1f64),
            mysql::Value::from(0i64),
            mysql::Value::from(12345u64),
        ];
        let as_params: mysql::Params = params.into();
        assert!(matches!(as_params, mysql::Params::Positional(v) if v.len() == 5));
    }

    #[test]
    fn query_with_params_accepts_bound_positional_params() {
        // Pure shape check (no socket): a Vec<mysql::Value> is a valid `impl
        // Into<mysql::Params>` for query_with_params's signature. The live
        // roundtrip below covers actual binding against a real server.
        let params: Vec<mysql::Value> = vec![mysql::Value::from("hi"), mysql::Value::from(5i64)];
        let as_params: mysql::Params = params.into();
        assert!(matches!(as_params, mysql::Params::Positional(v) if v.len() == 2));
    }

    /// Live smoke test: only meaningful when the native DB is actually up on
    /// loopback. Gated on a TCP connect to `127.0.0.1:<resolved port>` so the
    /// suite stays green on a box with no server running (it becomes a no-op),
    /// but exercises the real driver end-to-end when the server IS up.
    ///
    /// THE SCHEMA COMES FROM A FIXTURE, not from the ambient environment.
    /// Since schema names stopped being hardcoded, `DbConfig::from_env()`
    /// answers `None` unless a real title dir is installed at `DML_GAMES_DIR`
    /// — which is exported on the author's box and set NOWHERE in
    /// `.github/workflows/rust.yml`, so this test panicked with
    /// `NamesUnresolved` on CI. It is given the server context it needs
    /// instead of being weakened or skipped (a test that skips on CI is this
    /// repo's recorded vacuous-pass trap). `information_schema` is the schema
    /// EVERY MySQL server has, so the fixture also removes this test's old
    /// dependency on an AzerothCore install; host/port/password still come
    /// from the real `DbConfig::from_env()` precedence.
    #[test]
    fn live_db_roundtrip_when_reachable() {
        let mut cfg = DbConfig::from_env();
        let dir = title_fixture(
            "live-roundtrip",
            Some(&compose_yaml(&[
                ("AC_LOGIN_DATABASE_INFO", "ac-database;3306;root;pw;information_schema"),
                ("AC_WORLD_DATABASE_INFO", "ac-database;3306;root;pw;information_schema"),
                ("AC_CHARACTER_DATABASE_INFO", "ac-database;3306;root;pw;information_schema"),
            ])),
            None,
            None,
            None,
        );
        cfg.db_names = database_names_for_title(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        // Assert the fixture really resolved: if it silently did not, the
        // whole test below would fail for the wrong reason (or, worse, a
        // future `expect`-free rewrite would pass vacuously).
        assert_eq!(
            cfg.db_names.as_ref().map(|n| n.auth.as_str()),
            Some("information_schema"),
            "the fixture must resolve through the real resolver"
        );
        let addr = format!("{}:{}", cfg.host, cfg.port);
        let reachable = std::net::TcpStream::connect_timeout(
            &addr.parse().expect("valid loopback addr"),
            std::time::Duration::from_millis(400),
        )
        .is_ok();
        if !reachable {
            eprintln!("skipping live_db_roundtrip: no DB on {addr}");
            return;
        }
        // SELECT 1 AS one -> exactly one integer column named "one".
        let res = query(&cfg, Database::Auth, "SELECT 1 AS one")
            .expect("SELECT 1 on a reachable DB");
        assert_eq!(res.columns, vec!["one".to_string()]);
        assert_eq!(res.rows, vec![vec![SqlValue::Int(1)]]);

        // Bound-parameter roundtrip (finding #1): a value containing a quote
        // and a backslash must come back byte-identical -- proof the driver
        // is truly binding it rather than splicing it into the statement
        // text, independent of sql_mode/NO_BACKSLASH_ESCAPES.
        let tricky = r#"O'Brien\test"#;
        let params: Vec<mysql::Value> = vec![mysql::Value::from(tricky)];
        let res = query_with_params(&cfg, Database::Auth, "SELECT ? AS echoed", params)
            .expect("bound-param SELECT on a reachable DB");
        assert_eq!(res.rows, vec![vec![SqlValue::Text(tricky.to_string())]]);
    }

    #[test]
    fn sql_row_int_reads_int_and_text_variants() {
        use crate::db::SqlValue;
        assert_eq!(sql_row_int(Some(&SqlValue::Int(42))), Some(42));
        assert_eq!(sql_row_int(Some(&SqlValue::Text("7".into()))), Some(7));
        assert_eq!(sql_row_int(Some(&SqlValue::Text("nope".into()))), None);
        assert_eq!(sql_row_int(Some(&SqlValue::Null)), None);
        assert_eq!(sql_row_int(None), None);
    }
}
