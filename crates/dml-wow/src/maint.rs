//! Native-mode reads for the 3 safest, read-only maintenance/diagnostics
//! commands (spike: `spike/docker-desktop-native`, Task C starter):
//! `docker-usage`, `port-check`, `update-check`. These are the first
//! non-DB/non-SOAP shell-outs ported off `dml` (docker disk usage, docker
//! port bindings, git behind-counts) — proving the native pattern extends
//! to plain docker/git reads, not just MySQL/SOAP. Destructive verbs
//! (`docker-clean`, `update`, `backup`/`restore`) are explicitly OUT of
//! scope; WSL keeps calling `dml` for everything not ported here.
//!
//! Each function is a faithful port of its `cli/src/90-main.sh` arm,
//! documented at each site:
//!   - `docker-usage`  -> `90-main.sh:1500-1518`
//!   - `port-check`    -> `90-main.sh:5821-5890` (`_pc_probe`/`_pc_obj`)
//!   - `update-check`  -> `90-main.sh:5618-5644` +
//!     `_wow_repo_check_json`/`_wow_git_*` (`70-modules.sh:96-869`)
//!
//! Every docker/git call here is bounded via
//! [`super::status::output_bounded_draining`] — deliberately NOT
//! `crate::output_bounded`, whose own doc comment says output must be
//! small (it doesn't drain the child's pipes while waiting). `docker
//! system df` and `git fetch` are not "small inspect" calls, so the
//! draining variant is used uniformly for every process spawned in this
//! module rather than assuming any one of them is safe with the
//! non-draining runner.
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml`; the Tauri
//! command layer (`lib.rs`) gates every entry point on
//! `require_native_backend()`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

use super::status::{output_bounded_draining, windows_no_window};

/// Bounded timeout for the small local docker/git probes here (`docker
/// info`, `docker port`, `git rev-parse`/`status`/`rev-list`) — mirrors
/// `status::DOCKER_PROBE_TIMEOUT`.
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// `docker system df` walks every image/container/volume/build-cache entry
/// on the host, so it gets more budget than a plain `inspect`/`port` probe.
pub const DOCKER_USAGE_TIMEOUT: Duration = Duration::from_secs(15);
/// `git fetch origin` hits the network — a generous timeout so a slow (not
/// hung) connection still completes, but still bounded so a wedged remote
/// can never hang the Tools/Updates page.
pub const GIT_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// `true` when `s` is one-or-more ASCII digits — the bash `^[0-9]+$` guard
/// (duplicated locally, same convention `status::is_digits` documents: keep
/// each `dml::` module self-contained).
fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Resolved compose/source dir for the WoW Playerbots title — a port of
/// `_wow_server_dir` (`90-main.sh:106-110`): the title dir itself if it
/// carries a compose file, else its first subdir that does. `None` when
/// neither exists (title not installed). Duplicated locally from
/// `config::wow_server_installed`'s `has_compose` check because callers
/// here need the resolved PATH (for `.env`/`.git` reads), not just a bool.
pub fn resolve_server_dir(title_dir: &Path) -> Option<PathBuf> {
    fn has_compose(dir: &Path) -> bool {
        ["docker-compose.yml", "docker-compose.yaml", "compose.yml", "compose.yaml"]
            .iter()
            .any(|name| dir.join(name).is_file())
    }
    if !title_dir.is_dir() {
        return None;
    }
    if has_compose(title_dir) {
        return Some(title_dir.to_path_buf());
    }
    let entries = std::fs::read_dir(title_dir).ok()?;
    entries.flatten().map(|e| e.path()).find(|p| p.is_dir() && has_compose(p))
}

/// `true` when a bounded `docker info` succeeds — deliberately NOT
/// `native::engine_running` (that helper calls `Command::status()` with no
/// timeout at all); every process call in this module must be bounded, so
/// this reuses `native::docker_info_args()`'s argv (pure) through the
/// draining bounded runner instead.
pub fn docker_engine_up(program: &OsStr, timeout: Duration) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(super::native::docker_info_args());
    windows_no_window(&mut cmd);
    matches!(output_bounded_draining(cmd, timeout), Some(out) if out.status.success())
}

// ---------------------------------------------------------------------------
// `docker-usage` — `90-main.sh:1500-1518`.
// ---------------------------------------------------------------------------

/// Parse combined `docker system df` stdout+stderr into the `lines` array —
/// a port of the arm's read loop (`90-main.sh:1505-1513`): split on
/// newlines, drop empty lines, keep everything else verbatim. A trailing
/// `\r` is additionally stripped per line (this native path runs
/// `docker.exe` directly on Windows, which can emit CRLF where the bash's
/// WSL-side `docker` never did — matching the codebase's other line
/// parsers, e.g. `status::console_lines`).
pub fn parse_docker_usage_lines(raw: &str) -> Vec<String> {
    raw.lines()
        .map(|l| l.trim_end_matches('\r').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Assemble the `docker-usage` envelope — the arm's final `json_ok` line
/// (`90-main.sh:1518`).
pub fn assemble_docker_usage(raw: &str) -> Value {
    json!({ "lines": parse_docker_usage_lines(raw) })
}

/// Live `docker-usage` read: a `docker info` gate (`Err(())` on failure,
/// matching the arm's own `json_err DOCKER_DOWN`/`exit 1` — unlike
/// `server-detail`, this verb treats "down" as a hard error, not data),
/// then one bounded `docker system df` with combined stdout+stderr (the
/// arm's `2>&1`). A timeout on the `df` call itself degrades to an empty
/// `lines` array rather than erroring — the bash has no such bound to hit,
/// but an empty read is the closest faithful answer when one is imposed.
pub fn read_docker_usage(program: &OsStr) -> Result<Value, ()> {
    if !docker_engine_up(program, PROBE_TIMEOUT) {
        return Err(());
    }
    let mut cmd = Command::new(program);
    cmd.args(["system", "df"]);
    windows_no_window(&mut cmd);
    let combined = match output_bounded_draining(cmd, DOCKER_USAGE_TIMEOUT) {
        Some(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&out.stderr));
            s
        }
        None => String::new(),
    };
    Ok(assemble_docker_usage(&combined))
}

// ---------------------------------------------------------------------------
// `port-check` — `90-main.sh:5821-5890` (`_pc_probe`/`_pc_obj`).
// ---------------------------------------------------------------------------

/// One `docker port <container> <internal>` probe result — the raw pieces
/// `_pc_probe` computes (`90-main.sh:5843-5854`) before `_pc_obj`
/// re-validates `host_port` as digits-only at JSON-assembly time.
/// `host_ip`/`host_port` are kept as UNVALIDATED strings here on purpose
/// (empty when not published) — [`port_binding_json`] does the bash's own
/// two-stage validation.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PortProbe {
    pub published: bool,
    pub host_ip: String,
    pub host_port: String,
    pub lan_ready: bool,
}

/// Parse ONE `docker port` result's first line into a [`PortProbe`] — a
/// port of `_pc_probe`: empty output -> not published. Otherwise split at
/// the LAST `:` (`${bind%:*}` / `${bind##*:}`) into host_ip/host_port; a
/// bind with no colon at all lands the whole string in BOTH halves,
/// matching bash's own no-match parameter-expansion fallback (not a
/// realistic `docker port` output, but faithfully reproduced). `lan_ready`
/// is false only for loopback binds (`127.0.0.1`, `::1`, `[::1]`,
/// `localhost`) — anything else, including an unpublished-but-nonempty
/// oddity, reads as LAN-ready (mirrors the arm's `case` statement, which
/// has no other branch).
pub fn parse_port_probe(raw: &str) -> PortProbe {
    let first_line = raw.lines().next().unwrap_or("").trim();
    if first_line.is_empty() {
        return PortProbe::default();
    }
    let (host_ip, host_port) = match first_line.rfind(':') {
        Some(idx) => (first_line[..idx].to_string(), first_line[idx + 1..].to_string()),
        None => (first_line.to_string(), first_line.to_string()),
    };
    let lan_ready = !matches!(host_ip.as_str(), "127.0.0.1" | "::1" | "[::1]" | "localhost");
    PortProbe { published: true, host_ip, host_port, lan_ready }
}

/// Live bounded `docker port <container> <internal>` probe. Exit status is
/// IGNORED (matches the bash's `2>/dev/null | head -1 || true`) — a
/// failing call yields empty stdout, which [`parse_port_probe`] already
/// reads as "not published". A timeout degrades the same way.
pub fn probe_port(program: &OsStr, container: &str, internal: &str, timeout: Duration) -> PortProbe {
    let mut cmd = Command::new(program);
    cmd.args(["port", container, internal]);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, timeout) {
        Some(out) => parse_port_probe(&String::from_utf8_lossy(&out.stdout)),
        None => PortProbe::default(),
    }
}

/// One `ports[]` entry — a port of `_pc_obj` (`90-main.sh:5856-5862`):
/// `host_ip` is `null` unless the probe reports a non-empty IP; `host_port`
/// is a JSON NUMBER only when digits-only, else `null` — the bash's own
/// SECOND validation pass (a raw non-numeric `host_port` from
/// [`parse_port_probe`] must degrade to `null`, not surface as a string).
pub fn port_binding_json(name: &str, service: &str, internal: u16, probe: &PortProbe) -> Value {
    let host_ip = if probe.host_ip.is_empty() { Value::Null } else { json!(probe.host_ip) };
    let host_port =
        if is_digits(&probe.host_port) { json!(probe.host_port.parse::<i64>().unwrap_or(0)) } else { Value::Null };
    json!({
        "name": name,
        "service": service,
        "internal": internal,
        "published": probe.published,
        "host_ip": host_ip,
        "host_port": host_port,
        "lan_ready": probe.lan_ready,
    })
}

/// Parse the `.env` file's `DOCKER_DB_EXTERNAL_PORT=` value — a port of the
/// arm's preamble (`90-main.sh:5828-5833`): first matching line only
/// (`grep -m1 '^DOCKER_DB_EXTERNAL_PORT='`), the field between the first
/// and second `=` (`cut -d= -f2`), every whitespace char removed anywhere
/// in the field (`tr -d '[:space:]'`), kept only if digits-only. `None` on
/// no match or an invalid value — the caller's own default (3306) applies
/// then, exactly like the bash leaving `pc_dbport` at its initial value.
pub fn parse_db_external_port(env_content: &str) -> Option<u32> {
    for line in env_content.lines() {
        let Some(after) = line.strip_prefix("DOCKER_DB_EXTERNAL_PORT=") else { continue };
        let field2 = after.split('=').next().unwrap_or("");
        let cleaned: String = field2.chars().filter(|c| !c.is_whitespace()).collect();
        return if is_digits(&cleaned) { cleaned.parse().ok() } else { None };
    }
    None
}

/// Assemble the full `port-check` envelope from the three already-probed
/// ports plus the `.env` DB-port fallback — a port of the arm's final
/// assembly (`90-main.sh:5865-5879`). The DB probe's `lan_ready` is FORCED
/// `false` in the emitted `ports[]` entry (and `db_lan_exposed` is always
/// `false`) regardless of what [`parse_port_probe`] computed for it —
/// intentional: the DB's `0.0.0.0` docker bind lives inside WSL2's NAT and
/// is reachable from THIS PC only, never truly LAN-reachable without the
/// separate portproxy/firewall exposure step this endpoint can't detect
/// (rationale ported verbatim from the arm's comment). `game_lan_ready`
/// uses the UNMODIFIED auth/world readiness.
pub fn assemble_port_check(auth: &PortProbe, world: &PortProbe, db: &PortProbe, db_env_fallback: u32) -> Value {
    let running = auth.published || world.published || db.published;
    let game_lan_ready = auth.lan_ready && world.lan_ready;
    let db_host_port =
        if is_digits(&db.host_port) { json!(db.host_port.parse::<i64>().unwrap_or(0)) } else { json!(db_env_fallback) };
    let db_forced = PortProbe { lan_ready: false, ..db.clone() };
    json!({
        "running": running,
        "game_lan_ready": game_lan_ready,
        "db_host_port": db_host_port,
        "db_lan_exposed": false,
        "ports": [
            port_binding_json("ac-authserver", "login", 3724, auth),
            port_binding_json("ac-worldserver", "world", 8085, world),
            port_binding_json("ac-database", "database", 3306, &db_forced),
        ],
    })
}

/// Live `port-check` read: probes auth(3724)/world(8085)/db(3306), reads
/// the `.env` DB-port fallback, assembles the envelope — a port of the
/// arm's body (`90-main.sh:5832-5879`). The caller (`lib.rs`) owns the
/// `NOT_FOUND`/`DOCKER_DOWN` gates ahead of this (server dir resolved,
/// docker confirmed up); this function assumes both already hold.
pub fn read_port_check(program: &OsStr, server_dir: &Path, timeout: Duration) -> Value {
    let mut db_fallback: u32 = 3306;
    if let Ok(env_text) = std::fs::read_to_string(server_dir.join(".env")) {
        if let Some(p) = parse_db_external_port(&env_text) {
            db_fallback = p;
        }
    }
    let auth = probe_port(program, "ac-authserver", "3724", timeout);
    let world = probe_port(program, "ac-worldserver", "8085", timeout);
    let db = probe_port(program, "ac-database", "3306", timeout);
    assemble_port_check(&auth, &world, &db, db_fallback)
}

// ---------------------------------------------------------------------------
// `update-check` — `90-main.sh:5618-5644` + `_wow_repo_check_json`/
// `_wow_git_*` (`70-modules.sh:96-105,854-869`).
// ---------------------------------------------------------------------------

/// Trimmed stdout of a bounded LOCAL `git -C <dir> <args…>` call, or `""`
/// on any spawn failure/nonzero exit/timeout — a port of the `_wow_git_*`
/// guarded-assignment helpers (`70-modules.sh:102-105`): each ALWAYS
/// "succeeds" from the caller's point of view (the bash's own `|| printf
/// ''` fallback), so a missing remote / detached HEAD / whatever never
/// aborts the read.
fn git_field(program: &OsStr, dir: &Path, args: &[&str], timeout: Duration) -> String {
    let mut cmd = Command::new(program);
    cmd.arg("-C").arg(dir).args(args);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, timeout) {
        Some(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim_end_matches(['\n', '\r']).to_string()
        }
        _ => String::new(),
    }
}

/// Non-empty-line count of a `git status --porcelain` result — a port of
/// the dirty-count step (`70-modules.sh:862`): `grep -c .` counts lines
/// with at least one character; an empty `dirty` string (the guarded
/// helper's own "no output" answer, or a clean tree) short-circuits to 0
/// without even running the count, matching the bash's `[[ -n "$dirty" ]]
/// &&` gate.
pub fn parse_dirty_count(dirty_raw: &str) -> i64 {
    if dirty_raw.is_empty() {
        0
    } else {
        dirty_raw.lines().filter(|l| !l.is_empty()).count() as i64
    }
}

/// Parse `git rev-list --count HEAD..origin/<branch>` output into the
/// behind-count, or `None` — a port of the arm's count step
/// (`70-modules.sh:864-866`): first line, digits-only guard, the same
/// convention as every other numeric parse in this codebase.
pub fn parse_behind_count(raw: &str) -> Option<i64> {
    let first_line = raw.lines().next().unwrap_or("").trim();
    is_digits(first_line).then(|| first_line.parse().ok()).flatten()
}

/// Assemble one `repos[]` entry — a port of `_wow_repo_check_json`'s
/// `printf` (`70-modules.sh:867-869`). `dirty`/`behind` are the CALLER's
/// already-computed values ([`parse_dirty_count`] / [`parse_behind_count`],
/// or `None` on a failed fetch), so this stays a pure assembly step.
pub fn repo_check_json(label: &str, url: &str, branch: &str, head: &str, dirty: i64, behind: Option<i64>) -> Value {
    json!({
        "label": label,
        "url": url,
        "branch": branch,
        "head": head,
        "dirty": dirty,
        "behind": behind,
    })
}

/// `true` when `dir/.git` exists — the arm's git-checkout gate
/// (`90-main.sh:5624`: `[[ ! -d "$sdir/.git" ]]` -> `GIT_MISSING`).
pub fn is_git_checkout(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Live `_wow_repo_check_json` port: reads url/branch/head/dirty via LOCAL
/// git calls (fast, no network), then a bounded `git fetch --quiet origin`
/// (network, [`GIT_FETCH_TIMEOUT`]) and — ONLY on a successful fetch —
/// `git rev-list --count HEAD..origin/<branch>` for the behind-count.
/// NEVER mutates the worktree (no pull/stash, matching the arm's own
/// "read-only-ish" comment, `90-main.sh:5618-5620`) and never hard-fails:
/// a fetch failure just leaves `behind: null`.
pub fn read_repo_check(program: &OsStr, dir: &Path, label: &str) -> Value {
    let url = git_field(program, dir, &["remote", "get-url", "origin"], PROBE_TIMEOUT);
    let branch = git_field(program, dir, &["rev-parse", "--abbrev-ref", "HEAD"], PROBE_TIMEOUT);
    let head = git_field(program, dir, &["rev-parse", "--short", "HEAD"], PROBE_TIMEOUT);
    let dirty_raw = git_field(program, dir, &["status", "--porcelain", "--untracked-files=no"], PROBE_TIMEOUT);
    let dirty = parse_dirty_count(&dirty_raw);

    let mut fetch_cmd = Command::new(program);
    fetch_cmd.arg("-C").arg(dir).args(["fetch", "--quiet", "origin"]);
    windows_no_window(&mut fetch_cmd);
    let fetch_ok = matches!(output_bounded_draining(fetch_cmd, GIT_FETCH_TIMEOUT), Some(out) if out.status.success());

    let behind = if fetch_ok {
        let range = format!("HEAD..origin/{branch}");
        let raw = git_field(program, dir, &["rev-list", "--count", &range], PROBE_TIMEOUT);
        parse_behind_count(&raw)
    } else {
        None
    };

    repo_check_json(label, &url, &branch, &head, dirty, behind)
}

/// Assemble the `update-check` envelope from already-computed repo-check
/// values — the pure half of [`read_update_check`] (a port of the arm's
/// final assembly, `90-main.sh:5630-5644`), so the "AzerothCore always,
/// mod-playerbots only if present, else a `note`" logic is unit-testable
/// with fixture `Value`s rather than a live git checkout.
pub fn assemble_update_check(azerothcore: Value, mod_playerbots: Option<Value>) -> Value {
    let mut repos = vec![azerothcore];
    let note = match mod_playerbots {
        Some(v) => {
            repos.push(v);
            None
        }
        None => Some("mod-playerbots module is not installed -- nothing to check there"),
    };
    let mut envelope = json!({ "repos": repos });
    if let Some(n) = note {
        envelope.as_object_mut().expect("object literal").insert("note".into(), json!(n));
    }
    envelope
}

/// Live `update-check` read: AzerothCore repo always, `modules/mod-playerbots`
/// only if it's itself a git checkout — a port of the arm's body
/// (`90-main.sh:5630-5644`). The caller (`lib.rs`) owns the
/// `NOT_FOUND`/`GIT_MISSING` gates ahead of this.
pub fn read_update_check(program: &OsStr, server_dir: &Path) -> Value {
    let azerothcore = read_repo_check(program, server_dir, "AzerothCore");
    let mod_dir = server_dir.join("modules").join("mod-playerbots");
    let mod_playerbots =
        if is_git_checkout(&mod_dir) { Some(read_repo_check(program, &mod_dir, "mod-playerbots")) } else { None };
    assemble_update_check(azerothcore, mod_playerbots)
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_docker_usage_lines / assemble_docker_usage --------------------

    #[test]
    fn parse_docker_usage_lines_splits_and_drops_empties() {
        let raw = "TYPE\tTOTAL\tACTIVE\tSIZE\r\nImages\t5\t3\t1.2GB\r\n\r\nContainers\t3\t2\t100MB\r\n";
        let got = parse_docker_usage_lines(raw);
        assert_eq!(got, vec!["TYPE\tTOTAL\tACTIVE\tSIZE", "Images\t5\t3\t1.2GB", "Containers\t3\t2\t100MB"]);
    }

    #[test]
    fn parse_docker_usage_lines_empty_input_is_empty_vec() {
        assert_eq!(parse_docker_usage_lines(""), Vec::<String>::new());
        assert_eq!(parse_docker_usage_lines("\n\n\n"), Vec::<String>::new());
    }

    #[test]
    fn assemble_docker_usage_shape() {
        let got = assemble_docker_usage("a\nb\n");
        assert_eq!(got, json!({ "lines": ["a", "b"] }));
    }

    // -- parse_port_probe ------------------------------------------------------

    #[test]
    fn parse_port_probe_not_published_on_empty() {
        assert_eq!(parse_port_probe(""), PortProbe::default());
        assert_eq!(parse_port_probe("\n"), PortProbe::default());
    }

    #[test]
    fn parse_port_probe_ipv4_lan_ready() {
        let got = parse_port_probe("0.0.0.0:8085\n");
        assert_eq!(
            got,
            PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "8085".into(), lan_ready: true }
        );
    }

    #[test]
    fn parse_port_probe_loopback_variants_not_lan_ready() {
        for (bind, ip) in [
            ("127.0.0.1:3724\n", "127.0.0.1"),
            ("::1:3724\n", "::1"),
            ("[::1]:3724\n", "[::1]"),
            ("localhost:3724\n", "localhost"),
        ] {
            let got = parse_port_probe(bind);
            assert!(!got.lan_ready, "expected not lan_ready for {bind}");
            assert_eq!(got.host_ip, ip);
        }
    }

    #[test]
    fn parse_port_probe_ipv6_any_is_lan_ready() {
        let got = parse_port_probe("[::]:8085\n");
        assert_eq!(got.host_ip, "[::]");
        assert_eq!(got.host_port, "8085");
        assert!(got.lan_ready);
    }

    #[test]
    fn parse_port_probe_only_first_line_matters() {
        let got = parse_port_probe("0.0.0.0:8085\n0.0.0.0:9999\n");
        assert_eq!(got.host_port, "8085");
    }

    // -- port_binding_json -----------------------------------------------------

    #[test]
    fn port_binding_json_full_shape() {
        let probe = PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "8085".into(), lan_ready: true };
        let got = port_binding_json("ac-worldserver", "world", 8085, &probe);
        assert_eq!(
            got,
            json!({
                "name": "ac-worldserver",
                "service": "world",
                "internal": 8085,
                "published": true,
                "host_ip": "0.0.0.0",
                "host_port": 8085,
                "lan_ready": true,
            })
        );
    }

    #[test]
    fn port_binding_json_not_published_nulls_ip_and_port() {
        let got = port_binding_json("ac-authserver", "login", 3724, &PortProbe::default());
        assert_eq!(got["published"], json!(false));
        assert_eq!(got["host_ip"], Value::Null);
        assert_eq!(got["host_port"], Value::Null);
        assert_eq!(got["lan_ready"], json!(false));
    }

    #[test]
    fn port_binding_json_nonnumeric_host_port_degrades_to_null() {
        // A probe that somehow carries a non-digit host_port (defensive —
        // should never happen from a real `docker port` line) still
        // degrades to null, not a string, per the arm's second gate.
        let probe = PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "abc".into(), lan_ready: true };
        let got = port_binding_json("ac-database", "database", 3306, &probe);
        assert_eq!(got["host_port"], Value::Null);
    }

    // -- parse_db_external_port -------------------------------------------------

    #[test]
    fn parse_db_external_port_valid_line() {
        assert_eq!(parse_db_external_port("DOCKER_DB_EXTERNAL_PORT=13306\n"), Some(13306));
    }

    #[test]
    fn parse_db_external_port_strips_whitespace_anywhere() {
        assert_eq!(parse_db_external_port("DOCKER_DB_EXTERNAL_PORT= 1 3 3 0 6 \r\n"), Some(13306));
    }

    #[test]
    fn parse_db_external_port_only_second_cut_field() {
        // Mirrors `cut -d= -f2`: only the field between the 1st and 2nd `=`.
        assert_eq!(parse_db_external_port("DOCKER_DB_EXTERNAL_PORT=13306=extra\n"), Some(13306));
    }

    #[test]
    fn parse_db_external_port_first_match_only() {
        let env = "DOCKER_DB_EXTERNAL_PORT=13306\nDOCKER_DB_EXTERNAL_PORT=99999\n";
        assert_eq!(parse_db_external_port(env), Some(13306));
    }

    #[test]
    fn parse_db_external_port_missing_or_invalid_is_none() {
        assert_eq!(parse_db_external_port(""), None);
        assert_eq!(parse_db_external_port("SOME_OTHER_VAR=1\n"), None);
        assert_eq!(parse_db_external_port("DOCKER_DB_EXTERNAL_PORT=notanumber\n"), None);
    }

    // -- assemble_port_check ----------------------------------------------------

    #[test]
    fn assemble_port_check_full_running_shape() {
        let auth = PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "3724".into(), lan_ready: true };
        let world = PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "8085".into(), lan_ready: true };
        let db = PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "3306".into(), lan_ready: true };
        let got = assemble_port_check(&auth, &world, &db, 3306);
        assert_eq!(got["running"], json!(true));
        assert_eq!(got["game_lan_ready"], json!(true));
        assert_eq!(got["db_host_port"], json!(3306));
        // DB is never reported LAN-exposed, even though its own probe was
        // LAN-ready — the WSL2-NAT caveat forces this false.
        assert_eq!(got["db_lan_exposed"], json!(false));
        assert_eq!(got["ports"][2]["lan_ready"], json!(false));
        assert_eq!(got["ports"][2]["published"], json!(true)); // published still reported truthfully
    }

    #[test]
    fn assemble_port_check_nothing_running() {
        let none = PortProbe::default();
        let got = assemble_port_check(&none, &none, &none, 3306);
        assert_eq!(got["running"], json!(false));
        assert_eq!(got["game_lan_ready"], json!(false));
        assert_eq!(got["db_host_port"], json!(3306)); // falls back — db.host_port not digits
    }

    #[test]
    fn assemble_port_check_db_host_port_prefers_live_probe_over_fallback() {
        let none = PortProbe::default();
        let db = PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "13306".into(), lan_ready: true };
        let got = assemble_port_check(&none, &none, &db, 3306);
        assert_eq!(got["db_host_port"], json!(13306));
    }

    #[test]
    fn assemble_port_check_game_ready_needs_both_auth_and_world() {
        let ready = PortProbe { published: true, host_ip: "0.0.0.0".into(), host_port: "1".into(), lan_ready: true };
        let not_ready =
            PortProbe { published: true, host_ip: "127.0.0.1".into(), host_port: "1".into(), lan_ready: false };
        let none = PortProbe::default();
        let got = assemble_port_check(&ready, &not_ready, &none, 3306);
        assert_eq!(got["game_lan_ready"], json!(false));
        assert_eq!(got["running"], json!(true));
    }

    // -- resolve_server_dir ------------------------------------------------------

    #[test]
    fn resolve_server_dir_missing_title_is_none() {
        let dir = std::env::temp_dir().join(format!("dml-maint-test-missing-{}", std::process::id()));
        assert_eq!(resolve_server_dir(&dir), None);
    }

    #[test]
    fn resolve_server_dir_compose_at_root() {
        let dir = std::env::temp_dir().join(format!("dml-maint-test-root-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("docker-compose.yml"), "").unwrap();
        assert_eq!(resolve_server_dir(&dir), Some(dir.clone()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_server_dir_compose_in_subdir() {
        let dir = std::env::temp_dir().join(format!("dml-maint-test-sub-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sub = dir.join("wow-server-playerbots");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("compose.yaml"), "").unwrap();
        assert_eq!(resolve_server_dir(&dir), Some(sub));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn resolve_server_dir_no_compose_anywhere_is_none() {
        let dir = std::env::temp_dir().join(format!("dml-maint-test-nocompose-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("subdir")).unwrap();
        assert_eq!(resolve_server_dir(&dir), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- parse_dirty_count -----------------------------------------------------

    #[test]
    fn parse_dirty_count_empty_is_zero() {
        assert_eq!(parse_dirty_count(""), 0);
    }

    #[test]
    fn parse_dirty_count_counts_lines() {
        let dirty = " M src/foo.rs\n?? new_file.txt\n M src/bar.rs";
        assert_eq!(parse_dirty_count(dirty), 3);
    }

    // -- parse_behind_count ------------------------------------------------------

    #[test]
    fn parse_behind_count_digits_only_first_line() {
        assert_eq!(parse_behind_count("0\n"), Some(0));
        assert_eq!(parse_behind_count("42\n"), Some(42));
        assert_eq!(parse_behind_count(""), None);
        assert_eq!(parse_behind_count("fatal: bad revision\n"), None);
        assert_eq!(parse_behind_count("-1\n"), None);
    }

    // -- repo_check_json ---------------------------------------------------------

    #[test]
    fn repo_check_json_shape() {
        let got = repo_check_json("AzerothCore", "https://example/repo.git", "master", "abc1234", 2, Some(5));
        assert_eq!(
            got,
            json!({
                "label": "AzerothCore",
                "url": "https://example/repo.git",
                "branch": "master",
                "head": "abc1234",
                "dirty": 2,
                "behind": 5,
            })
        );
    }

    #[test]
    fn repo_check_json_behind_null_on_fetch_failure() {
        let got = repo_check_json("mod-playerbots", "", "", "", 0, None);
        assert_eq!(got["behind"], Value::Null);
    }

    // -- is_git_checkout -----------------------------------------------------

    #[test]
    fn is_git_checkout_true_and_false() {
        let dir = std::env::temp_dir().join(format!("dml-maint-test-git-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".git")).unwrap();
        assert!(is_git_checkout(&dir));
        assert!(!is_git_checkout(&dir.join("modules").join("mod-playerbots")));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // -- assemble_update_check ---------------------------------------------------

    #[test]
    fn assemble_update_check_notes_missing_module() {
        let ac = repo_check_json("AzerothCore", "u", "master", "abc", 0, Some(3));
        let got = assemble_update_check(ac.clone(), None);
        assert_eq!(got["note"], json!("mod-playerbots module is not installed -- nothing to check there"));
        let repos = got["repos"].as_array().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0], ac);
    }

    #[test]
    fn assemble_update_check_both_repos_no_note() {
        let ac = repo_check_json("AzerothCore", "u1", "master", "abc", 0, Some(3));
        let mp = repo_check_json("mod-playerbots", "u2", "master", "def", 1, None);
        let got = assemble_update_check(ac.clone(), Some(mp.clone()));
        assert!(got.get("note").is_none());
        let repos = got["repos"].as_array().unwrap();
        assert_eq!(repos.len(), 2);
        assert_eq!(repos[0], ac);
        assert_eq!(repos[1], mp);
    }
}
