//! Native-mode whole-server **backups** (spike: `spike/docker-desktop-native`,
//! Task C2a): `backup create` (streamed), `backup list`/`validate`/`delete`
//! (plain JSON). Faithful port of `cli/src/90-main.sh:3662-3785` +
//! `cli/src/60-backup.sh`.
//!
//! WHY THIS EXISTS. WSL/`dml` drives `docker exec ac-database mysqldump …
//! | gzip > ~/.dml/backups/wow-<ts>.sql.gz` via bash. Native mode has no bash
//! to pipe through, so this module does the same two things directly in
//! Rust: [`dump_to`] shells `docker exec … mysqldump` and gzips its stdout
//! with `flate2` instead of piping to a `gzip` process. Everything else (name
//! validation, prune, summary sidecar, validate, delete) is plain `std::fs` +
//! a hand-rolled decompress-and-scan, no shelling at all.
//!
//! `dump_to` STREAMS, IT NEVER BUFFERS THE WHOLE DUMP. A `--include-world`
//! dump can be many GB of stdout — [`dump_to`] is also the pre-restore SAFETY
//! dump inside `wow_backup_restore_native` (`lib.rs`), so an earlier
//! fully-buffered `Vec<u8>` capture (`status::output_bounded_draining`, fine
//! for the small bounded reads it was designed for — see that function's own
//! doc comment) meant an OOM here could kill the whole Tauri process while
//! the game server was stopped, with no recovery. [`dump_to`] instead spawns
//! the child with piped stdout/stderr directly (not via `output_bounded_
//! draining`) and reads stdout in fixed chunks straight into a
//! `flate2::write::GzEncoder<File>` writing the `.tmp` sibling — at no point
//! does more than one chunk of the dump exist in memory, however large the
//! whole thing is. Stderr is drained concurrently on its own thread into a
//! capped tail buffer (only the last [`err_tail`]-sized slice is ever
//! reported, so the cap costs nothing). The overall `DUMP_TIMEOUT` deadline
//! is enforced by the MAIN thread polling `try_wait()` and killing the child
//! on overrun — the exact same drain-while-polling shape [`super::status::
//! output_bounded_draining`]'s doc comment describes, just with the "drain"
//! half doing real work (gzip-encoding to disk) instead of only buffering.
//! Mirrors [`super::restore::stream_restore`]'s chunked-read/concurrent-drain
//! discipline for the opposite (import) direction.
//!
//! SIX SANCTIONED DIRECT MYSQL WRITES. `backup restore` (out of scope for
//! this task — see `60-backup.sh`'s header comment) is the CLI's one
//! sanctioned whole-DB-overwrite path; nothing here writes character data.
//! `write_meta`'s summary counts are read-only `SELECT COUNT(*)` queries via
//! [`super::db::query`], same as `dml::status`'s `bots_online`.
//!
//! ON-DISK FORMAT IS SHARED WITH WSL. The `~/.dml/backups` directory and its
//! `.sql.gz`/`.meta` files are backend-agnostic: a backup created by WSL
//! `dml` must validate/list/delete correctly under native mode and vice
//! versa. That is why [`format_summary_line`] builds the meta sidecar text
//! by hand (`{"characters":N,"accounts":N,"bots":M|null}`, exact key order)
//! rather than via `serde_json::Value`'s `Display` — `serde_json`'s `Map` is
//! NOT built with the `preserve_order` feature in this workspace (no
//! `indexmap` in `Cargo.lock` under `serde_json`'s own dependency list), so
//! serializing a `json!({"characters":...,"accounts":...,"bots":...})` value
//! would emit keys in **alphabetical** order (`accounts`, `bots`,
//! `characters`) — bytes [`read_summary`]'s own regex-equivalent parser (a
//! port of `_backup_summary_read`) would then reject, silently degrading
//! every native-written sidecar to `null`. Writing the literal string sidesteps
//! that trap entirely; reading is likewise a hand-rolled character scan (not
//! `serde_json::from_str` + shape-check) so it validates the exact compact
//! form a bash `printf` or this writer produces, not just "any valid JSON
//! object with these keys" — same doctrine as the CLI's own regex gate.
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml`; the Tauri command
//! layer (`lib.rs`) gates every entry point on `require_native_backend()`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::{json, Value};

use super::db::{self, Database, DbConfig};
use super::status::windows_no_window;

/// `_backup_dir` (`60-backup.sh:23`): `~/.dml/backups`.
pub fn backup_dir() -> Option<PathBuf> {
    super::dml_home_dir().map(|h| h.join("backups"))
}

/// Keep the newest N backups — `DML_BACKUP_KEEP`, default 10 (`60-backup.sh:33`).
/// Any unset/unparseable value falls back to the default, same convention as
/// `lib.rs`'s `wr_ready_timeout_secs`.
pub fn backup_keep_from_env() -> usize {
    std::env::var("DML_BACKUP_KEEP").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(10)
}

// ---------------------------------------------------------------------------
// Name validation / parsing — `_valid_backup_name` (`60-backup.sh:26`).
// ---------------------------------------------------------------------------

fn ascii_digits(b: &[u8]) -> bool {
    b.iter().all(|c| c.is_ascii_digit())
}

/// `_valid_backup_name`: `^wow-[0-9]{8}-[0-9]{6}(-full)?(-prerestore)?\.sql\.gz$`,
/// hand-rolled (no `regex` crate in this workspace — same convention as
/// `status::strip_ansi`). Gates every verb in this module: list/validate/
/// delete all refuse a name that doesn't match this shape.
///
/// Works entirely on `name.as_bytes()` with plain index/slice arithmetic —
/// NOT `&str` range-slicing at these fixed offsets — because a raw byte
/// slice never panics on a misaligned boundary (it just fails the
/// ASCII-digit check, same as the bash regex simply failing to match). A
/// `&str` slice at a fixed byte offset, by contrast, panics if that offset
/// lands inside a multi-byte UTF-8 character, which a crafted name (e.g. one
/// containing `\u{e9}`) can trigger. Same doctrine as
/// `dml::lan::matches_172_second_octet`, which indexes `addr.as_bytes()` for
/// exactly this reason.
pub fn valid_backup_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    const PREFIX: &[u8] = b"wow-";
    if !bytes.starts_with(PREFIX) {
        return false;
    }
    let mut i = PREFIX.len();
    if bytes.len() < i + 8 || !ascii_digits(&bytes[i..i + 8]) {
        return false;
    }
    i += 8;
    if bytes.get(i) != Some(&b'-') {
        return false;
    }
    i += 1;
    if bytes.len() < i + 6 || !ascii_digits(&bytes[i..i + 6]) {
        return false;
    }
    i += 6;
    let rest = &bytes[i..];
    let rest = rest.strip_prefix(b"-full").unwrap_or(rest);
    let rest = rest.strip_prefix(b"-prerestore").unwrap_or(rest);
    rest == b".sql.gz"
}

/// `bw` (`90-main.sh:3719-3720`): a `world` (whole-server, `--include-world`)
/// snapshot has `-full` in its name, with or without a trailing
/// `-prerestore`. Caller's responsibility to have already checked
/// [`valid_backup_name`].
pub fn is_full_name(name: &str) -> bool {
    name.ends_with("-full.sql.gz") || name.ends_with("-full-prerestore.sql.gz")
}

/// `created` (`90-main.sh:3717-3718`): `"YYYY-MM-DD HH:MM:SS"` sliced
/// straight out of the (already-validated) file name's fixed date/time
/// digits — a port of the bash's `${f:4:8}` / `${f:13:6}` substring slices.
/// `None` if `name` isn't [`valid_backup_name`]-shaped (defensive; callers
/// always check first).
///
/// Slices `name.as_bytes()` at the fixed offsets, not the `&str` itself —
/// raw byte indexing can never panic on a misaligned UTF-8 boundary (see
/// [`valid_backup_name`]'s doc comment for why that hazard is real here even
/// though [`valid_backup_name`] has already been checked: defense in depth,
/// not reliance on the caller having gotten that gate exactly right).
pub fn parse_created(name: &str) -> Option<String> {
    if !valid_backup_name(name) || name.len() < 19 {
        return None;
    }
    let bytes = name.as_bytes();
    let d = std::str::from_utf8(&bytes[4..12]).ok()?;
    let t = std::str::from_utf8(&bytes[13..19]).ok()?;
    Some(format!("{}-{}-{} {}:{}:{}", &d[0..4], &d[4..6], &d[6..8], &t[0..2], &t[2..4], &t[4..6]))
}

/// `"$out.tmp"` / `"<file>.meta"` — literal suffix append on the OS string
/// (not `Path::with_extension`, which would clobber the existing `.gz`
/// extension instead of appending).
fn append_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut os = path.as_os_str().to_os_string();
    os.push(suffix);
    PathBuf::from(os)
}

/// The `.meta` sidecar path for a `.sql.gz` backup.
pub fn meta_path_for(sql_gz_path: &Path) -> PathBuf {
    append_suffix(sql_gz_path, ".meta")
}

// ---------------------------------------------------------------------------
// UTC timestamp — `date -u +%Y%m%d-%H%M%S` (`90-main.sh:3676`), hand-rolled
// (no date/time crate is a dependency of this workspace) via Howard
// Hinnant's `civil_from_days` (public domain,
// http://howardhinnant.github.io/date_algorithms.html).
// ---------------------------------------------------------------------------

/// Proleptic-Gregorian civil `(year, month, day)` from a day count since the
/// Unix epoch (1970-01-01 = day 0). Pure.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// UTC `(year, month, day, hour, min, sec)` for a Unix timestamp. Pure.
pub fn utc_from_unix(secs: u64) -> (i64, u32, u32, u32, u32, u32) {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    (y, m, d, (rem / 3600) as u32, ((rem % 3600) / 60) as u32, (rem % 60) as u32)
}

/// `date -u +%Y%m%d-%H%M%S` for a Unix timestamp.
pub fn format_utc_compact(secs: u64) -> String {
    let (y, m, d, hh, mm, ss) = utc_from_unix(secs);
    format!("{y:04}{m:02}{d:02}-{hh:02}{mm:02}{ss:02}")
}

/// `bfile="wow-$(date -u +%Y%m%d-%H%M%S)$bsuffix.sql.gz"` (`90-main.sh:3674-3676`).
pub fn new_backup_file_name(include_world: bool) -> String {
    new_backup_file_name_at(
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0),
        include_world,
    )
}

/// Pure half of [`new_backup_file_name`], for testing without the real clock.
pub fn new_backup_file_name_at(unix_secs: u64, include_world: bool) -> String {
    let ts = format_utc_compact(unix_secs);
    if include_world { format!("wow-{ts}-full.sql.gz") } else { format!("wow-{ts}.sql.gz") }
}

// ---------------------------------------------------------------------------
// list / prune — directory scans. `sort -r` on the fixed-width
// `wow-YYYYMMDD-HHMMSS...` names IS a chronological descending sort (plain
// byte/lexicographic order agrees with numeric order for same-width
// zero-padded fields), so a plain descending string sort is faithful.
// ---------------------------------------------------------------------------

/// Every `*.sql.gz` entry directly under `dir`, newest-name-first — a port
/// of `ls -1 "$bdir" | grep -E '\.sql\.gz$' | sort -r`. Deliberately NOT
/// filtered by [`valid_backup_name`] (neither is the bash `ls` pipeline
/// `_backup_prune` reads): prune sweeps every `.sql.gz` file past the
/// retention window, even a stray mis-named one. Missing/unreadable `dir`
/// degrades to empty (matches `ls ... 2>/dev/null`).
pub fn sql_gz_names_desc(dir: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> = rd
        .flatten()
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".sql.gz"))
        .collect();
    names.sort_by(|a, b| b.cmp(a));
    names
}

/// Names beyond the newest `keep` in an already-sorted-descending list — the
/// pure core of `_backup_prune`'s `(( n > keep ))` loop.
pub fn prune_names(sorted_desc: &[String], keep: usize) -> &[String] {
    if sorted_desc.len() <= keep {
        &[]
    } else {
        &sorted_desc[keep..]
    }
}

/// Delete every backup (+ its `.meta` sidecar) beyond the retention window
/// under `dir`, returning the pruned names in the order they were pruned
/// (newest-of-the-pruned first, matching the bash loop's read order).
/// Best-effort per file (`rm -f` semantics: a missing/unremovable file is
/// silently skipped, never aborts the sweep).
pub fn prune(dir: &Path) -> Vec<String> {
    let names = sql_gz_names_desc(dir);
    let keep = backup_keep_from_env();
    let pruned = prune_names(&names, keep);
    for f in pruned {
        let path = dir.join(f);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(meta_path_for(&path));
    }
    pruned.to_vec()
}

/// One `backup list` row.
#[derive(Debug, Clone, PartialEq)]
pub struct BackupEntry {
    pub file: String,
    pub size: u64,
    pub created: String,
    pub world: bool,
    pub summary: Value,
}

/// `backup list` (`90-main.sh:3709-3729`): every [`valid_backup_name`] entry
/// under `dir`, newest first, each with its size/created/world/summary
/// fields. Missing `dir` degrades to an empty list (matches the bash's
/// `[[ -d "$bdir" ]]` guard).
pub fn list_backups(dir: &Path) -> Vec<BackupEntry> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut names: Vec<String> =
        rd.flatten().filter_map(|e| e.file_name().into_string().ok()).filter(|n| valid_backup_name(n)).collect();
    names.sort_by(|a, b| b.cmp(a));
    names
        .into_iter()
        .map(|f| {
            let path = dir.join(&f);
            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let created = parse_created(&f).unwrap_or_default();
            let world = is_full_name(&f);
            let summary = read_summary(&meta_path_for(&path));
            BackupEntry { file: f, size, created, world, summary }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Content-summary sidecar — `_backup_summary_json`/`_backup_write_meta`/
// `_backup_summary_read` (`60-backup.sh:81-117`).
// ---------------------------------------------------------------------------

const SUMMARY_CHARS_SQL: &str = "SELECT COUNT(*) FROM characters;";
const SUMMARY_ACCOUNTS_SQL: &str = "SELECT COUNT(*) FROM account;";
const SUMMARY_BOTS_SQL: &str = "SELECT COUNT(*) FROM characters WHERE account IN \
    (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2));";

/// Decode a single-cell `COUNT(*)` result: the binary protocol should hand
/// back a native `Int`, but a `Text` digit-string is accepted too (defensive
/// parity with the bash's `^[0-9]+$` guard on `mysql -N -B` text output).
fn scalar_i64(res: &db::QueryResult) -> Option<i64> {
    match res.rows.first()?.first()? {
        db::SqlValue::Int(n) => Some(*n),
        db::SqlValue::Text(s) if !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()) => s.parse().ok(),
        _ => None,
    }
}

/// The exact compact literal a `.meta` sidecar holds:
/// `{"characters":N,"accounts":N,"bots":M}` or `{"characters":N,"accounts":N,"bots":null}`.
/// Hand-built (not via `serde_json::Value`) so the key order is guaranteed —
/// see the module doc comment for why that matters.
pub fn format_summary_line(chars: i64, accounts: i64, bots: Option<i64>) -> String {
    let bots_str = bots.map(|b| b.to_string()).unwrap_or_else(|| "null".to_string());
    format!("{{\"characters\":{chars},\"accounts\":{accounts},\"bots\":{bots_str}}}")
}

/// `_backup_summary_json` (`60-backup.sh:81-93`): live `{characters,accounts,
/// bots}` counts, or `None` if either of the two REQUIRED counts (characters,
/// accounts) couldn't be read — `bots` alone is optional (stays `null`).
pub fn compute_summary(cfg: &DbConfig) -> Option<Value> {
    let chars = db::query(cfg, Database::Characters, SUMMARY_CHARS_SQL).ok().and_then(|r| scalar_i64(&r))?;
    let accounts = db::query(cfg, Database::Auth, SUMMARY_ACCOUNTS_SQL).ok().and_then(|r| scalar_i64(&r))?;
    let bots = db::query(cfg, Database::Characters, SUMMARY_BOTS_SQL).ok().and_then(|r| scalar_i64(&r));
    serde_json::from_str(&format_summary_line(chars, accounts, bots)).ok()
}

/// `_backup_write_meta` (`60-backup.sh:97-102`): best-effort — a failed
/// summary read (DB unreachable, query error) silently writes no sidecar and
/// never fails the caller's backup.
pub fn write_meta(cfg: &DbConfig, sql_gz_path: &Path) {
    let Some((chars, accounts, bots)) = compute_summary_parts(cfg) else { return };
    let line = format_summary_line(chars, accounts, bots);
    let _ = std::fs::write(meta_path_for(sql_gz_path), format!("{line}\n"));
}

/// [`compute_summary`]'s raw `(characters, accounts, bots)` triple, before
/// JSON assembly — split out so [`write_meta`] can hand it straight to
/// [`format_summary_line`] without an extra JSON round-trip.
fn compute_summary_parts(cfg: &DbConfig) -> Option<(i64, i64, Option<i64>)> {
    let chars = db::query(cfg, Database::Characters, SUMMARY_CHARS_SQL).ok().and_then(|r| scalar_i64(&r))?;
    let accounts = db::query(cfg, Database::Auth, SUMMARY_ACCOUNTS_SQL).ok().and_then(|r| scalar_i64(&r))?;
    let bots = db::query(cfg, Database::Characters, SUMMARY_BOTS_SQL).ok().and_then(|r| scalar_i64(&r));
    Some((chars, accounts, bots))
}

fn parse_digits(s: &str) -> Option<(&str, &str)> {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if end == 0 {
        None
    } else {
        Some((&s[..end], &s[end..]))
    }
}

/// `_backup_summary_read`'s regex, hand-rolled:
/// `^\{"characters":[0-9]+,"accounts":[0-9]+,"bots":([0-9]+|null)\}$`. Exact
/// literal shape only — NOT "any JSON object with these keys" (a
/// pretty-printed or reordered sidecar is rejected, same as the bash regex
/// would reject it).
fn valid_summary_line(s: &str) -> bool {
    let Some(rest) = s.strip_prefix("{\"characters\":") else { return false };
    let Some((_, rest)) = parse_digits(rest) else { return false };
    let Some(rest) = rest.strip_prefix(",\"accounts\":") else { return false };
    let Some((_, rest)) = parse_digits(rest) else { return false };
    let Some(rest) = rest.strip_prefix(",\"bots\":") else { return false };
    let rest = if let Some(r) = rest.strip_prefix("null") {
        r
    } else if let Some((_, r)) = parse_digits(rest) {
        r
    } else {
        return false;
    };
    rest == "}"
}

/// `_backup_summary_read` (`60-backup.sh:109-117`): the sidecar's parsed
/// summary object, or `Value::Null` when the file is absent or its first
/// line doesn't match [`valid_summary_line`] — a malformed/garbage sidecar
/// degrades to `null` rather than corrupting the `backup list` envelope.
pub fn read_summary(meta_path: &Path) -> Value {
    let Ok(raw) = std::fs::read_to_string(meta_path) else { return Value::Null };
    let first_line = raw.lines().next().unwrap_or("");
    if valid_summary_line(first_line) {
        serde_json::from_str(first_line).unwrap_or(Value::Null)
    } else {
        Value::Null
    }
}

// ---------------------------------------------------------------------------
// `backup create`'s dump half — `_backup_dump_to` (`60-backup.sh:51-63`).
// ---------------------------------------------------------------------------

/// Generous bound for `docker exec … mysqldump` — a full character-DB dump
/// is small, but `--include-world` can move a much larger `acore_world`
/// snapshot; 30 minutes is ample for either while still guaranteeing the
/// streamed command can never hang forever on a wedged docker.
pub const DUMP_TIMEOUT: Duration = Duration::from_secs(1800);

/// The `docker exec ac-database mysqldump …` argv — a port of
/// `_backup_dump_to`'s command line (`60-backup.sh:56`).
pub fn mysqldump_args(password: &str, include_world: bool) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "exec".into(),
        "ac-database".into(),
        "mysqldump".into(),
        "-uroot".into(),
        format!("-p{password}"),
        "--databases".into(),
        "acore_characters".into(),
        "acore_playerbots".into(),
        "acore_auth".into(),
    ];
    if include_world {
        args.push("acore_world".into());
    }
    args.push("--single-transaction".into());
    args.push("--quick".into());
    args
}

/// `tail -c 160 "$out.err" | tr -d '\r\n"\\'` (`90-main.sh:3683`), applied
/// directly to the captured stderr tail (native has no `.err` file — the
/// bytes are already in memory, drained concurrently by [`dump_to`] into a
/// [`STDERR_TAIL_CAP`]-bounded buffer; since only the last 160 bytes of
/// *that* buffer are ever used, the cap changes nothing this function sees).
fn err_tail(stderr: &[u8]) -> String {
    let start = stderr.len().saturating_sub(160);
    String::from_utf8_lossy(&stderr[start..]).chars().filter(|c| !matches!(c, '\r' | '\n' | '"' | '\\')).collect()
}

/// Read/write chunk size for [`dump_to`]'s streaming copy loop — same order
/// of magnitude as [`super::restore::stream_into`]'s 64 KiB import chunk,
/// picked from the "64-256 KiB" band called out in the hardening spec for
/// this rewrite.
const DUMP_CHUNK_SIZE: usize = 128 * 1024;

/// Hard cap on the stderr tail [`dump_to`] keeps in memory while draining the
/// dump's stderr pipe concurrently. Far larger than [`err_tail`] ever reads
/// (160 bytes) — this exists only so a pathologically chatty `mysqldump`
/// can't grow the buffer without bound the way the old fully-buffered
/// capture implicitly allowed; the *content* [`err_tail`] sees is identical
/// either way, since this cap always keeps the newest bytes (see
/// [`push_bounded_tail`]).
const STDERR_TAIL_CAP: usize = 64 * 1024;

/// Append `chunk` to `buf`, then trim from the FRONT (oldest bytes) if `buf`
/// now exceeds `cap` — keeps only the newest `cap` bytes, same shape as a
/// ring buffer but simpler since stderr for a `mysqldump` run is small by
/// construction (this only ever fires in a pathological case).
fn push_bounded_tail(buf: &mut Vec<u8>, chunk: &[u8], cap: usize) {
    buf.extend_from_slice(chunk);
    if buf.len() > cap {
        let excess = buf.len() - cap;
        buf.drain(0..excess);
    }
}

/// `_backup_dump_to` (`60-backup.sh:51-63`): run the dump, gzip its stdout
/// STREAMED (never buffered whole — see the module doc comment), atomic
/// tmp+rename into `out_path`. `Err` carries the [`err_tail`]-trimmed stderr
/// on a dump failure (mirrors the bash's `errtail` the caller reports as
/// `BACKUP_FAILED`'s hint), the same combined "timed out or could not be
/// started" message on a spawn failure or a [`DUMP_TIMEOUT`] overrun, or a
/// plain I/O message on a local gzip/rename failure. No partial file is ever
/// left at `out_path` on failure (tmp is written first, `.tmp` cleaned up on
/// error) — this holds even when the child dies mid-stream.
///
/// Thin wrapper over [`dump_stream`] (same split as [`super::restore::
/// stream_restore`] over `stream_into`): this keeps the real `docker exec …
/// mysqldump` argv AND the real [`DUMP_TIMEOUT`] fixed, while `dump_stream`
/// itself is generic over program/args/timeout so the streaming engine can
/// be exercised in tests against a harmless `cmd.exe` child and a
/// millisecond-scale deadline instead.
pub fn dump_to(program: &OsStr, password: &str, include_world: bool, out_path: &Path) -> Result<(), String> {
    dump_stream(program, &mysqldump_args(password, include_world), out_path, DUMP_TIMEOUT)
}

/// The generic streaming-pipe engine behind [`dump_to`] — see that
/// function's doc comment for the split's rationale.
fn dump_stream(program: &OsStr, args: &[String], out_path: &Path, timeout: Duration) -> Result<(), String> {
    const START_OR_TIMEOUT_ERR: &str = "mysqldump timed out or the docker command could not be started";

    let mut cmd = Command::new(program);
    cmd.args(args);
    windows_no_window(&mut cmd);
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|_| START_OR_TIMEOUT_ERR.to_string())?;
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");

    let tmp = append_suffix(out_path, ".tmp");
    let tmp_for_writer = tmp.clone();

    // Drain stderr on its own thread, concurrently with the stdout-copy
    // thread below and this thread's deadline poll — same anti-deadlock
    // discipline `restore::stream_into`'s doc comment explains (a chatty
    // child can otherwise fill an unread pipe and block).
    let stderr_handle = std::thread::spawn(move || {
        let mut buf: Vec<u8> = Vec::new();
        let mut chunk = [0u8; 8 * 1024];
        loop {
            match std::io::Read::read(&mut stderr, &mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => push_bounded_tail(&mut buf, &chunk[..n], STDERR_TAIL_CAP),
            }
        }
        buf
    });

    // THE load-bearing streaming loop: fixed-size chunks, read from the
    // child's stdout pipe -> gzip-encode -> write to the `.tmp` file, repeat.
    // At no point does this hold more than one [`DUMP_CHUNK_SIZE`] chunk of
    // the dump in memory, however many GB `mysqldump` ultimately emits.
    let stdout_handle = std::thread::spawn(move || -> std::io::Result<()> {
        let f = std::fs::File::create(&tmp_for_writer)?;
        let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
        let mut chunk = [0u8; DUMP_CHUNK_SIZE];
        loop {
            let n = std::io::Read::read(&mut stdout, &mut chunk)?;
            if n == 0 {
                break;
            }
            std::io::Write::write_all(&mut enc, &chunk[..n])?;
        }
        enc.finish()?;
        Ok(())
    });

    // Deadline enforcement lives on THIS thread, exactly like `status::
    // output_bounded_draining`: poll `try_wait()` (never blocking on the
    // pipes ourselves), and kill+reap the child on overrun. Killing closes
    // both pipes, which unblocks the two reader threads above (EOF/broken
    // pipe) well before their joins below — same reasoning as that
    // function's own doc comment on why the join afterward isn't itself an
    // unbounded wait.
    let deadline = std::time::Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                timed_out = true;
                break None;
            }
        }
    };

    let write_result = stdout_handle
        .join()
        .unwrap_or_else(|_| Err(std::io::Error::new(std::io::ErrorKind::Other, "the dump-writer thread panicked")));
    let stderr_buf = stderr_handle.join().unwrap_or_default();

    // Priority mirrors the pre-streaming code's error classification order:
    // a timeout/never-exited child beats everything else, then a nonzero
    // exit (reported via `err_tail`, ignoring whatever partial bytes the
    // writer thread saw — a killed/failed child's stdout is never a dump
    // worth keeping), then a local write/rename failure, then success.
    if timed_out {
        let _ = std::fs::remove_file(&tmp);
        return Err(START_OR_TIMEOUT_ERR.to_string());
    }
    let Some(status) = status else {
        let _ = std::fs::remove_file(&tmp);
        return Err(START_OR_TIMEOUT_ERR.to_string());
    };
    if !status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(err_tail(&stderr_buf));
    }
    if let Err(e) = write_result {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("could not write the gzip archive: {e}"));
    }

    std::fs::rename(&tmp, out_path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("could not finalize the backup file: {e}")
    })
}

// ---------------------------------------------------------------------------
// `backup validate` — `90-main.sh:3739-3785`.
// ---------------------------------------------------------------------------

/// Fully decompress a `.gz` file into memory, validating its trailer CRC/
/// ISIZE along the way (equivalent to `gzip -t`'s integrity check — `flate2`
/// surfaces a checksum mismatch as a read error, same as `read_to_end`
/// failing partway).
fn gzip_decompress_to_vec(path: &Path) -> std::io::Result<Vec<u8>> {
    let f = std::fs::File::open(path)?;
    let mut dec = flate2::read::GzDecoder::new(f);
    let mut buf = Vec::new();
    std::io::Read::read_to_end(&mut dec, &mut buf)?;
    Ok(buf)
}

/// `grep -aoE 'CREATE TABLE `(characters|account)`'` reduced to one literal
/// substring test, byte-level (not UTF-8 text) so a dump containing non-UTF8
/// column bytes elsewhere can never perturb the search — a plain-Rust
/// `windows().any()` scan, no `regex`/`memchr` dependency.
fn contains_marker(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

const MARKER_CHARACTERS: &[u8] = b"CREATE TABLE `characters`";
const MARKER_ACCOUNT: &[u8] = b"CREATE TABLE `account`";

/// `backup validate`'s result shape.
#[derive(Debug, Clone, PartialEq)]
pub struct ValidateResult {
    pub valid: bool,
    pub size: u64,
    pub gzip_ok: bool,
    pub sql_ok: bool,
    pub markers: Vec<&'static str>,
    pub detail: String,
}

/// The `vvalid`/`vdetail` decision (`90-main.sh:3777-3783`), pure — split
/// out from [`validate_backup`] so the three-way branch is unit-tested
/// without real gzip fixtures.
fn classify(gzip_ok: bool, sql_ok: bool) -> (bool, &'static str) {
    if gzip_ok && sql_ok {
        (true, "Archive is intact and looks like a full character backup.")
    } else if !gzip_ok {
        (false, "gzip integrity check failed -- the file is truncated or corrupt. Do NOT restore it.")
    } else {
        (
            false,
            "Archive decompresses, but the expected character/account tables were not found -- it may be an incomplete or unrelated dump.",
        )
    }
}

/// `backup validate` (`90-main.sh:3739-3785`): one gzip decompression pass
/// serves BOTH the integrity check (a truncated/corrupt archive fails to
/// fully decompress / fails its CRC) and the marker scan — the bash runs two
/// separate passes (`gzip -t` then `gunzip -c | grep`), but reusing the same
/// decoded bytes here is an equivalent-output, one-less-pass optimization.
pub fn validate_backup(path: &Path) -> ValidateResult {
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let decompressed = gzip_decompress_to_vec(path).ok();
    let gzip_ok = decompressed.is_some();

    let mut markers = Vec::new();
    let mut chars_ok = false;
    let mut acct_ok = false;
    if let Some(bytes) = &decompressed {
        chars_ok = contains_marker(bytes, MARKER_CHARACTERS);
        acct_ok = contains_marker(bytes, MARKER_ACCOUNT);
    }
    if chars_ok {
        markers.push("characters");
    }
    if acct_ok {
        markers.push("account");
    }
    let sql_ok = chars_ok && acct_ok;

    let (valid, detail) = classify(gzip_ok, sql_ok);
    ValidateResult { valid, size, gzip_ok, sql_ok, markers, detail: detail.to_string() }
}

/// Assemble the `backup validate` JSON envelope (`90-main.sh:3784`).
pub fn validate_result_json(file: &str, r: &ValidateResult) -> Value {
    json!({
        "valid": r.valid,
        "file": file,
        "size": r.size,
        "gzip_ok": r.gzip_ok,
        "sql_ok": r.sql_ok,
        "markers": r.markers,
        "detail": r.detail,
    })
}

// ---------------------------------------------------------------------------
// `backup delete` — `90-main.sh:3730-3738`.
// ---------------------------------------------------------------------------

/// `rm -f "$bdir/$file" "$bdir/$file.meta"`: best-effort, ignores a missing/
/// unremovable file (matches `rm -f` semantics). Caller (`lib.rs`) is
/// responsible for the [`valid_backup_name`] gate and the `NOT_FOUND`
/// existence check BEFORE calling this — those are error-reporting
/// decisions, not this function's concern.
pub fn delete_backup(dir: &Path, file: &str) {
    let path = dir.join(file);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(meta_path_for(&path));
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- valid_backup_name / is_full_name / parse_created --------------------

    #[test]
    fn valid_backup_name_accepts_every_shape() {
        assert!(valid_backup_name("wow-20260726-143022.sql.gz"));
        assert!(valid_backup_name("wow-20260726-143022-full.sql.gz"));
        assert!(valid_backup_name("wow-20260726-143022-prerestore.sql.gz"));
        assert!(valid_backup_name("wow-20260726-143022-full-prerestore.sql.gz"));
    }

    #[test]
    fn valid_backup_name_rejects_garbage() {
        assert!(!valid_backup_name("wow-2026726-143022.sql.gz")); // date too short
        assert!(!valid_backup_name("wow-20260726-14302.sql.gz")); // time too short
        assert!(!valid_backup_name("wow-20260726-143022.sql.gz.bak"));
        assert!(!valid_backup_name("wow-20260726-143022.tar.gz"));
        assert!(!valid_backup_name("../../etc/passwd"));
        assert!(!valid_backup_name(""));
        assert!(!valid_backup_name("wow-20260726-143022-prerestore-full.sql.gz")); // wrong order
        assert!(!valid_backup_name("xwow-20260726-143022.sql.gz"));
        assert!(!valid_backup_name("wow-20260726-143022.sql.gzX"));
    }

    #[test]
    fn valid_backup_name_never_panics_on_multibyte_utf8_at_fixed_offsets() {
        // Regression for the byte-offset-vs-char-boundary hazard: a
        // multi-byte UTF-8 char straddling one of the old fixed &str-slice
        // offsets (7/8, 12/13, 18/19 -- see the doc comment) used to panic
        // with "byte index N is not a char boundary". Byte-slice indexing
        // must instead simply fail the ASCII-digit check and return false.
        // 2-byte char (U+00E9 'é') straddling the date block's tail.
        assert!(!valid_backup_name("wow-1234567\u{e9}-143022.sql.gz"));
        // 2-byte char straddling the boundary right after the date block.
        assert!(!valid_backup_name("wow-12345678\u{e9}143022.sql.gz"));
        // 2-byte char straddling the time block's tail.
        assert!(!valid_backup_name("wow-20260726-14302\u{e9}.sql.gz"));
        // 2-byte char straddling the boundary right after the time block.
        assert!(!valid_backup_name("wow-20260726-143022\u{e9}sql.gz"));
        // Multi-byte char right at the very front (prefix check).
        assert!(!valid_backup_name("\u{e9}wow-20260726-143022.sql.gz"));
        // 3-byte (€) and 4-byte (😀) forms too, not just 2-byte.
        assert!(!valid_backup_name("wow-1234567\u{20ac}-143022.sql.gz"));
        assert!(!valid_backup_name("wow-1234567\u{1f600}-143022.sql.gz"));
    }

    #[test]
    fn parse_created_never_panics_on_multibyte_utf8() {
        assert_eq!(parse_created("wow-1234567\u{e9}-143022.sql.gz"), None);
        assert_eq!(parse_created("wow-20260726-14302\u{e9}.sql.gz"), None);
        assert_eq!(parse_created("wow-1234567\u{1f600}-143022.sql.gz"), None);
    }

    #[test]
    fn is_full_name_detects_world_snapshots() {
        assert!(is_full_name("wow-20260726-143022-full.sql.gz"));
        assert!(is_full_name("wow-20260726-143022-full-prerestore.sql.gz"));
        assert!(!is_full_name("wow-20260726-143022.sql.gz"));
        assert!(!is_full_name("wow-20260726-143022-prerestore.sql.gz"));
    }

    #[test]
    fn parse_created_slices_the_fixed_date_time() {
        assert_eq!(parse_created("wow-20260726-143022.sql.gz").as_deref(), Some("2026-07-26 14:30:22"));
        assert_eq!(parse_created("wow-20260726-143022-full.sql.gz").as_deref(), Some("2026-07-26 14:30:22"));
        assert_eq!(parse_created("not-a-backup"), None);
    }

    // -- UTC timestamp ---------------------------------------------------------

    #[test]
    fn format_utc_compact_epoch() {
        assert_eq!(format_utc_compact(0), "19700101-000000");
    }

    #[test]
    fn format_utc_compact_known_timestamp() {
        // 1700000000 is a widely-documented reference value: 2023-11-14T22:13:20Z.
        assert_eq!(format_utc_compact(1_700_000_000), "20231114-221320");
    }

    #[test]
    fn format_utc_compact_leap_day() {
        // 2024-02-29T00:00:00Z = 1709164800.
        assert_eq!(format_utc_compact(1_709_164_800), "20240229-000000");
    }

    #[test]
    fn new_backup_file_name_at_shapes() {
        let plain = new_backup_file_name_at(1_700_000_000, false);
        assert_eq!(plain, "wow-20231114-221320.sql.gz");
        assert!(valid_backup_name(&plain));
        let full = new_backup_file_name_at(1_700_000_000, true);
        assert_eq!(full, "wow-20231114-221320-full.sql.gz");
        assert!(valid_backup_name(&full));
        assert!(is_full_name(&full));
    }

    // -- sql_gz_names_desc / prune_names / prune ------------------------------

    fn tmp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dml-backup-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn sql_gz_names_desc_sorts_newest_first_and_ignores_other_files() {
        let d = tmp_dir("sort");
        for f in ["wow-20260101-000000.sql.gz", "wow-20260301-000000.sql.gz", "wow-20260201-000000.sql.gz", "readme.txt"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let names = sql_gz_names_desc(&d);
        assert_eq!(
            names,
            vec!["wow-20260301-000000.sql.gz", "wow-20260201-000000.sql.gz", "wow-20260101-000000.sql.gz"]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn sql_gz_names_desc_missing_dir_is_empty() {
        let d = tmp_dir("missing");
        let _ = std::fs::remove_dir_all(&d);
        assert!(sql_gz_names_desc(&d).is_empty());
    }

    #[test]
    fn prune_names_keeps_the_newest_n() {
        let all = vec!["c".to_string(), "b".to_string(), "a".to_string()];
        assert_eq!(prune_names(&all, 2).to_vec(), vec!["a".to_string()]);
        assert!(prune_names(&all, 3).is_empty());
        assert!(prune_names(&all, 10).is_empty());
        assert_eq!(prune_names(&all, 0).to_vec(), all);
    }

    #[test]
    fn prune_deletes_files_beyond_the_keep_window() {
        let d = tmp_dir("prune");
        std::env::set_var("DML_BACKUP_KEEP", "2");
        for f in ["wow-20260101-000000.sql.gz", "wow-20260201-000000.sql.gz", "wow-20260301-000000.sql.gz"] {
            std::fs::write(d.join(f), b"x").unwrap();
            std::fs::write(meta_path_for(&d.join(f)), b"{}").unwrap();
        }
        let pruned = prune(&d);
        assert_eq!(pruned, vec!["wow-20260101-000000.sql.gz".to_string()]);
        assert!(!d.join("wow-20260101-000000.sql.gz").exists());
        assert!(!meta_path_for(&d.join("wow-20260101-000000.sql.gz")).exists());
        assert!(d.join("wow-20260201-000000.sql.gz").exists());
        assert!(d.join("wow-20260301-000000.sql.gz").exists());
        std::env::remove_var("DML_BACKUP_KEEP");
        let _ = std::fs::remove_dir_all(&d);
    }

    // -- summary sidecar -------------------------------------------------------

    #[test]
    fn format_summary_line_matches_bash_shape() {
        assert_eq!(format_summary_line(5, 3, Some(2)), r#"{"characters":5,"accounts":3,"bots":2}"#);
        assert_eq!(format_summary_line(5, 3, None), r#"{"characters":5,"accounts":3,"bots":null}"#);
    }

    #[test]
    fn valid_summary_line_accepts_the_exact_shape_only() {
        assert!(valid_summary_line(r#"{"characters":5,"accounts":3,"bots":2}"#));
        assert!(valid_summary_line(r#"{"characters":0,"accounts":0,"bots":null}"#));
        // Reordered keys, extra whitespace, or a pretty-printed form all fail
        // the exact-shape check -- same as the bash regex would reject them.
        assert!(!valid_summary_line(r#"{"accounts":3,"characters":5,"bots":2}"#));
        assert!(!valid_summary_line(r#"{"characters": 5,"accounts":3,"bots":2}"#));
        assert!(!valid_summary_line(""));
        assert!(!valid_summary_line("garbage"));
        assert!(!valid_summary_line(r#"{"characters":5,"accounts":3,"bots":2}extra"#));
    }

    #[test]
    fn read_summary_round_trips_through_write_and_missing_file_is_null() {
        let d = tmp_dir("summary");
        let sidecar = d.join("wow-20260101-000000.sql.gz.meta");
        std::fs::write(&sidecar, format!("{}\n", format_summary_line(7, 4, Some(1)))).unwrap();
        assert_eq!(read_summary(&sidecar), json!({"characters":7,"accounts":4,"bots":1}));

        let missing = d.join("nope.sql.gz.meta");
        assert_eq!(read_summary(&missing), Value::Null);

        let garbage = d.join("garbage.meta");
        std::fs::write(&garbage, "not json at all\n").unwrap();
        assert_eq!(read_summary(&garbage), Value::Null);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn meta_path_for_appends_literal_suffix() {
        let p = PathBuf::from(r"C:\backups\wow-20260101-000000.sql.gz");
        assert_eq!(meta_path_for(&p), PathBuf::from(r"C:\backups\wow-20260101-000000.sql.gz.meta"));
    }

    // -- dump args / err_tail --------------------------------------------------

    #[test]
    fn mysqldump_args_characters_only() {
        let args = mysqldump_args("hunter2", false);
        assert_eq!(
            args,
            vec![
                "exec", "ac-database", "mysqldump", "-uroot", "-phunter2", "--databases", "acore_characters",
                "acore_playerbots", "acore_auth", "--single-transaction", "--quick",
            ]
        );
    }

    #[test]
    fn mysqldump_args_include_world_adds_the_schema_before_the_flags() {
        let args = mysqldump_args("hunter2", true);
        assert_eq!(
            args,
            vec![
                "exec", "ac-database", "mysqldump", "-uroot", "-phunter2", "--databases", "acore_characters",
                "acore_playerbots", "acore_auth", "acore_world", "--single-transaction", "--quick",
            ]
        );
    }

    #[test]
    fn err_tail_strips_control_and_quote_chars_and_keeps_last_160_bytes() {
        assert_eq!(err_tail(b"plain error"), "plain error");
        assert_eq!(err_tail(b"a\r\nb\"c\\d"), "abcd");
        let long = vec![b'x'; 500];
        assert_eq!(err_tail(&long).len(), 160);
    }

    // -- dump_stream: real subprocess, no docker dependency ---------------------
    //
    // These exercise the FULL streaming engine (spawn -> chunked stdout read
    // -> gzip-encode-to-disk -> concurrent stderr drain -> deadline poll ->
    // reap) against `cmd.exe`-provided stand-ins instead of `docker exec …
    // mysqldump` — mirrors `dml::restore::tests`'s `stream_into` coverage for
    // the opposite (import) direction.

    #[test]
    fn dump_stream_real_multichunk_child_round_trips_through_gzip() {
        // A real `cmd.exe /c type <file>` child streams a payload comfortably
        // larger than several `DUMP_CHUNK_SIZE` chunks to stdout, proving the
        // read/gzip-encode loop's chunk boundaries never drop or corrupt
        // bytes -- not just a single small in-memory buffer.
        let d = tmp_dir("dump-stream-multichunk");
        let src = d.join("payload.txt");
        let mut payload = String::new();
        for i in 0..20_000 {
            payload.push_str(&format!("INSERT INTO characters VALUES ({i});\n"));
        }
        assert!(payload.len() > DUMP_CHUNK_SIZE * 3, "payload too small to force multiple chunks");
        std::fs::write(&src, payload.as_bytes()).unwrap();

        let out = d.join("wow-20260101-000000.sql.gz");
        dump_stream(
            OsStr::new("cmd.exe"),
            &["/c".to_string(), "type".to_string(), src.display().to_string()],
            &out,
            DUMP_TIMEOUT,
        )
        .unwrap();

        assert!(out.is_file());
        assert!(!append_suffix(&out, ".tmp").exists(), "the .tmp sibling must be gone after a successful rename");

        let decompressed = gzip_decompress_to_vec(&out).unwrap();
        let got = String::from_utf8_lossy(&decompressed).replace("\r\n", "\n");
        assert_eq!(got, payload, "content must survive the chunked read -> gzip -> decompress round trip exactly");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dump_stream_kills_and_reports_timeout_when_the_child_overruns_the_deadline() {
        // `cmd.exe /c "for /L %i in (1,0,2) do @rem"` is an infinite loop
        // BUILT INTO cmd.exe itself (a zero increment never terminates the
        // `for /L`) -- deliberately NOT `ping`/`timeout`/anything that shells
        // out to a separate .exe: a grandchild process inherits the same
        // piped stdout handle, and Windows `TerminateProcess` only kills the
        // process it's given, not its descendants, so killing just the
        // immediate `cmd.exe` would leave that grandchild holding the pipe
        // open -- our reader thread would then block on it for however long
        // THAT process keeps running, silently defeating the timeout this
        // test is supposed to exercise (confirmed live with `ping -n 60`: the
        // call didn't return for the full ~60s despite the child being
        // "killed" well before). A loop with no subprocess at all sidesteps
        // that trap: killing `cmd.exe` directly closes its own stdout handle,
        // so the reader thread sees EOF immediately.
        let d = tmp_dir("dump-stream-timeout");
        let out = d.join("wow-20260101-000000.sql.gz");

        let start = std::time::Instant::now();
        let err = dump_stream(
            OsStr::new("cmd.exe"),
            &["/c".to_string(), "for /L %i in (1,0,2) do @rem".to_string()],
            &out,
            Duration::from_millis(200),
        )
        .unwrap_err();
        assert!(err.contains("timed out"), "err={err}");
        // The kill must land close to the deadline, not after running
        // indefinitely (this loop would otherwise never exit on its own).
        assert!(start.elapsed() < Duration::from_secs(10), "elapsed={:?}", start.elapsed());

        assert!(!out.exists(), "no partial/renamed file may be left behind on a timeout");
        assert!(!append_suffix(&out, ".tmp").exists(), "the .tmp sibling must be cleaned up on a timeout");

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dump_stream_nonzero_exit_reports_err_tail_and_removes_tmp() {
        // A real child that writes to stderr then exits nonzero -- proving
        // the concurrently-drained stderr buffer actually reaches `err_tail`
        // and the `.tmp` file never survives a failed dump.
        let d = tmp_dir("dump-stream-nonzero");
        let out = d.join("wow-20260101-000000.sql.gz");

        let err = dump_stream(
            OsStr::new("cmd.exe"),
            &["/c".to_string(), "echo dump failed badly 1>&2 & exit 5".to_string()],
            &out,
            DUMP_TIMEOUT,
        )
        .unwrap_err();

        assert!(err.contains("dump failed badly"), "err={err}");
        assert!(!out.exists());
        assert!(!append_suffix(&out, ".tmp").exists());

        let _ = std::fs::remove_dir_all(&d);
    }

    // -- validate: pure classify + marker scan ----------------------------------

    #[test]
    fn classify_branches() {
        assert_eq!(classify(true, true).0, true);
        assert_eq!(classify(false, false).0, false);
        assert_eq!(classify(false, true).0, false);
        assert_eq!(classify(true, false).0, false);
        assert!(classify(false, true).1.contains("gzip integrity"));
        assert!(classify(true, false).1.contains("were not found"));
    }

    #[test]
    fn contains_marker_finds_and_misses() {
        let hay = b"USE acore_characters;\nCREATE TABLE `characters` (\n  `guid` int\n);\n";
        assert!(contains_marker(hay, MARKER_CHARACTERS));
        assert!(!contains_marker(hay, MARKER_ACCOUNT));
    }

    #[test]
    fn validate_backup_end_to_end_with_real_gzip_fixtures() {
        let d = tmp_dir("validate");

        // A well-formed dump: gzip-valid, both markers present.
        let good = d.join("wow-20260101-000000.sql.gz");
        {
            use std::io::Write;
            let f = std::fs::File::create(&good).unwrap();
            let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            enc.write_all(b"-- dump\nCREATE TABLE `characters` (`guid` int);\nCREATE TABLE `account` (`id` int);\n").unwrap();
            enc.finish().unwrap();
        }
        let r = validate_backup(&good);
        assert!(r.valid);
        assert!(r.gzip_ok);
        assert!(r.sql_ok);
        assert_eq!(r.markers, vec!["characters", "account"]);
        assert!(r.size > 0);

        // Corrupt gzip: truncate the file mid-stream.
        let corrupt = d.join("wow-20260201-000000.sql.gz");
        std::fs::write(&corrupt, b"\x1f\x8b\x08\x00not really gzip data").unwrap();
        let r = validate_backup(&corrupt);
        assert!(!r.valid);
        assert!(!r.gzip_ok);
        assert!(!r.sql_ok);
        assert!(r.markers.is_empty());
        assert!(r.detail.contains("truncated or corrupt"));

        // Valid gzip, but not a character dump.
        let unrelated = d.join("wow-20260301-000000.sql.gz");
        {
            use std::io::Write;
            let f = std::fs::File::create(&unrelated).unwrap();
            let mut enc = flate2::write::GzEncoder::new(f, flate2::Compression::default());
            enc.write_all(b"just some unrelated text\n").unwrap();
            enc.finish().unwrap();
        }
        let r = validate_backup(&unrelated);
        assert!(!r.valid);
        assert!(r.gzip_ok);
        assert!(!r.sql_ok);
        assert!(r.detail.contains("were not found"));

        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn validate_backup_missing_file_is_not_valid() {
        let d = tmp_dir("validate-missing");
        let r = validate_backup(&d.join("nope.sql.gz"));
        assert!(!r.valid);
        assert!(!r.gzip_ok);
        assert_eq!(r.size, 0);
    }

    #[test]
    fn validate_result_json_shape() {
        let r = ValidateResult {
            valid: true,
            size: 123,
            gzip_ok: true,
            sql_ok: true,
            markers: vec!["characters", "account"],
            detail: "ok".to_string(),
        };
        assert_eq!(
            validate_result_json("wow-20260101-000000.sql.gz", &r),
            json!({
                "valid": true,
                "file": "wow-20260101-000000.sql.gz",
                "size": 123,
                "gzip_ok": true,
                "sql_ok": true,
                "markers": ["characters", "account"],
                "detail": "ok",
            })
        );
    }

    // -- list_backups ------------------------------------------------------------

    #[test]
    fn list_backups_shape_newest_first_with_summary_and_world_flag() {
        let d = tmp_dir("list");
        std::fs::write(d.join("wow-20260101-000000.sql.gz"), b"aaaa").unwrap();
        std::fs::write(d.join("wow-20260201-000000-full.sql.gz"), b"bb").unwrap();
        std::fs::write(
            meta_path_for(&d.join("wow-20260201-000000-full.sql.gz")),
            format!("{}\n", format_summary_line(1, 1, Some(0))),
        )
        .unwrap();
        // Not a valid backup name -- must be skipped entirely.
        std::fs::write(d.join("stray.sql.gz"), b"z").unwrap();

        let entries = list_backups(&d);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].file, "wow-20260201-000000-full.sql.gz");
        assert!(entries[0].world);
        assert_eq!(entries[0].size, 2);
        assert_eq!(entries[0].summary, json!({"characters":1,"accounts":1,"bots":0}));
        assert_eq!(entries[1].file, "wow-20260101-000000.sql.gz");
        assert!(!entries[1].world);
        assert_eq!(entries[1].summary, Value::Null);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn list_backups_missing_dir_is_empty() {
        let d = tmp_dir("list-missing");
        let _ = std::fs::remove_dir_all(&d);
        assert!(list_backups(&d).is_empty());
    }

    // -- delete_backup -----------------------------------------------------------

    #[test]
    fn delete_backup_removes_file_and_meta_and_is_idempotent() {
        let d = tmp_dir("delete");
        let file = "wow-20260101-000000.sql.gz";
        std::fs::write(d.join(file), b"x").unwrap();
        std::fs::write(meta_path_for(&d.join(file)), b"{}").unwrap();
        delete_backup(&d, file);
        assert!(!d.join(file).exists());
        assert!(!meta_path_for(&d.join(file)).exists());
        // Calling again on an already-gone file must not panic (rm -f semantics).
        delete_backup(&d, file);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn backup_keep_from_env_default_and_override() {
        std::env::remove_var("DML_BACKUP_KEEP");
        assert_eq!(backup_keep_from_env(), 10);
        std::env::set_var("DML_BACKUP_KEEP", "3");
        assert_eq!(backup_keep_from_env(), 3);
        std::env::set_var("DML_BACKUP_KEEP", "not-a-number");
        assert_eq!(backup_keep_from_env(), 10);
        std::env::remove_var("DML_BACKUP_KEEP");
    }
}
