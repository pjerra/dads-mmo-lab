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

use std::path::PathBuf;

use dml_core::error::CmdError;

/// Loopback host the native stack publishes MySQL on.
pub const DB_HOST: &str = "127.0.0.1";
/// The only DB user the stack (and `dml`) authenticates as.
pub const DB_USER: &str = "root";
/// Compose default for the published host port (`DB_EXTERNAL_PORT:-3306`).
pub const DEFAULT_DB_PORT: u16 = 3306;
/// Compose default for the root password (`DB_ROOT_PASSWORD:-password`).
pub const DEFAULT_DB_PASSWORD: &str = "password";

/// The four AzerothCore schemas this launcher reads. Names are fixed by the
/// server build (see the `AC_*_DATABASE_INFO` lines in the native
/// `docker-compose.yml`); an enum keeps callers from typo-ing a DB name into a
/// SQL connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Database {
    World,
    Characters,
    Auth,
    Playerbots,
}

impl Database {
    /// The literal schema name to `USE`.
    pub fn name(self) -> &'static str {
        match self {
            Database::World => "acore_world",
            Database::Characters => "acore_characters",
            Database::Auth => "acore_auth",
            Database::Playerbots => "acore_playerbots",
        }
    }
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
}

impl DbError {
    /// The stable code a caller reports, matching the CLI's `json_err` codes.
    pub fn code(&self) -> &'static str {
        match self {
            DbError::Unreachable(_) => "DB_UNREACHABLE",
            DbError::Query(_) => "DB_QUERY_FAILED",
        }
    }
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Unreachable(m) => write!(f, "{m}"),
            DbError::Query(m) => write!(f, "{m}"),
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
}

impl DbConfig {
    /// Resolve from the real process environment plus the title dir's `.env`
    /// (the file Compose reads for `${VAR}` interpolation). Title dir is
    /// `DML_GAMES_DIR/wow-server-playerbots`, shared with
    /// [`super::config::ConfigReader::title_dir_from_env`].
    pub fn from_env() -> Self {
        let dotenv = std::fs::read_to_string(dotenv_path()).ok();
        resolve_db_config(|k| std::env::var(k).ok(), dotenv.as_deref())
    }

    /// A [`mysql::Opts`] pointed at `db`, built from these params. Split out so
    /// tests can assert the shape without opening a socket.
    fn opts(&self, db: Database) -> mysql::Opts {
        mysql::OptsBuilder::new()
            .ip_or_hostname(Some(self.host.clone()))
            .tcp_port(self.port)
            .user(Some(self.user.clone()))
            .pass(Some(self.password.clone()))
            .db_name(Some(db.name()))
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
            .into()
    }
}

/// Path of the title dir's `.env` (`DML_GAMES_DIR/wow-server-playerbots/.env`).
fn dotenv_path() -> PathBuf {
    super::config::ConfigReader::title_dir_from_env().join(".env")
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

    DbConfig {
        host: DB_HOST.to_string(),
        port,
        user: DB_USER.to_string(),
        password,
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
    mysql::Conn::new(cfg.opts(db))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_names_match_the_server_schemas() {
        assert_eq!(Database::World.name(), "acore_world");
        assert_eq!(Database::Characters.name(), "acore_characters");
        assert_eq!(Database::Auth.name(), "acore_auth");
        assert_eq!(Database::Playerbots.name(), "acore_playerbots");
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
    }

    #[test]
    fn opts_carry_host_port_user_and_db() {
        let cfg = DbConfig {
            host: "127.0.0.1".into(),
            port: 13306,
            user: "root".into(),
            password: "password".into(),
        };
        let opts = cfg.opts(Database::Characters);
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
        };
        let opts = cfg.opts(Database::Characters);
        assert_eq!(opts.get_tcp_connect_timeout(), Some(std::time::Duration::from_secs(5)));
        assert_eq!(opts.get_read_timeout(), Some(&std::time::Duration::from_secs(30)));
        assert_eq!(opts.get_write_timeout(), Some(&std::time::Duration::from_secs(30)));
        assert_eq!(opts.get_init(), vec!["SET NAMES utf8mb4".to_string()]);
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
    #[test]
    fn live_db_roundtrip_when_reachable() {
        let cfg = DbConfig::from_env();
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
