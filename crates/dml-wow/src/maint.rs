//! Native-mode reads for the 3 safest, read-only maintenance/diagnostics
//! commands (spike: `spike/docker-desktop-native`, Task C starter):
//! `docker-usage`, `port-check`, `update-check`. These are the first
//! non-DB/non-SOAP shell-outs ported off `dml` (docker disk usage, docker
//! port bindings, git behind-counts) — proving the native pattern extends
//! to plain docker/git reads, not just MySQL/SOAP.
//!
//! [`update_stream`] at the bottom is the one WRITE verb here: `wow update`,
//! the server self-update that `update-check` reports on. It moved out of
//! the launcher's `lib.rs` in the cargo-workspace refactor (Task 9) and sits
//! next to its read-only sibling rather than in `modmgr` (whose
//! `module_update_stream` updates ONE module, not the core checkout).
//! `docker-clean`/`backup`/`restore`/`games remove` live in `destructive`/
//! `backup`/`restore`.
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

use dml_core::error::{not_found_err, CmdError};
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

/// The resolved WoW Playerbots server dir, or the arm's own `NOT_FOUND`
/// (`90-main.sh`'s recurring `[[ -z "$sdir" ]] && { json_err NOT_FOUND
/// "WoW Playerbots server not installed" … }`). `resolve_server_dir` with
/// the env-derived title dir baked in — the shape every native-mode arm
/// that needs the server dir opens with.
pub fn require_server_dir(hint: &str) -> Result<PathBuf, CmdError> {
    let title_dir = super::config::ConfigReader::title_dir_from_env();
    resolve_server_dir(&title_dir)
        .ok_or_else(|| not_found_err("WoW Playerbots server not installed", hint))
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

/// One row of `docker stats --no-stream` for a stack container, verbatim
/// strings from docker (locale-formatted percentages and sizes; the UI
/// renders text, so parsing the numbers here would only add a way to be
/// wrong).
///
/// Parse the TAB-separated `{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}`
/// format, keeping ONLY the stack's own `ac-*` containers — the same
/// name-contract every other docker surface in this repo addresses the stack
/// by. Malformed lines are skipped, not errors: `docker stats` may interleave
/// a warning on stderr and the caller combines streams.
pub fn parse_container_stats(raw: &str) -> Vec<Value> {
    raw.lines()
        .filter_map(|l| {
            let l = l.trim_end_matches('\r');
            let mut it = l.split('\t');
            let (name, cpu, mem, mem_pct) = (it.next()?, it.next()?, it.next()?, it.next()?);
            if !name.starts_with("ac-") {
                return None;
            }
            Some(json!({
                "name": name,
                "cpu": cpu.trim(),
                "mem": mem.trim(),
                "mem_pct": mem_pct.trim(),
            }))
        })
        .collect()
}

/// Live per-container CPU/memory for the "Server resources" card: a `docker
/// info` gate like [`read_docker_usage`], then one bounded
/// `docker stats --no-stream` (it SAMPLES for ~2s by design, hence the same
/// generous bound as the usage read). Timeout on the stats call degrades to
/// an empty `rows` array — the card then says "no rows" rather than erroring
/// a page whose other cards are fine.
pub fn read_container_stats(program: &OsStr) -> Result<Value, ()> {
    if !docker_engine_up(program, PROBE_TIMEOUT) {
        return Err(());
    }
    let mut cmd = Command::new(program);
    cmd.args([
        "stats",
        "--no-stream",
        "--format",
        "{{.Name}}\t{{.CPUPerc}}\t{{.MemUsage}}\t{{.MemPerc}}",
    ]);
    windows_no_window(&mut cmd);
    let combined = match output_bounded_draining(cmd, DOCKER_USAGE_TIMEOUT) {
        Some(out) => {
            let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&out.stderr));
            s
        }
        None => String::new(),
    };
    Ok(json!({ "rows": parse_container_stats(&combined) }))
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

// ---------------------------------------------------------------------------
// NATIVE-MODE `wow update` (server self-update) — oracle `90-main.sh:5645-
// 5732` + `_wow_pull_repo`/`_wow_remote_ok`/`_wow_git_branch`
// (`70-modules.sh:843,850,872-933`, ported in `modmgr`). STREAMED, same
// NDJSON vocabulary as `lifecycle::world_restart_stream`. Fail-closed gates
// run IN ORDER, before any mutation: server dir -> `.git` checkout ->
// AzerothCore remote -> mod-playerbots remote (only if present) -> branch ->
// explicit `--backup`/`--no-backup` (see `modmgr::update_gate_order` for the
// same order, pure + unit-tested).
// ---------------------------------------------------------------------------

const WOW_UPDATE_SECTION: &str = "server-update";

/// The branch each repo tracks — the same two `install_native::
/// default_core_repo`/`default_module_repo` clone from. A native install then
/// detaches onto a pinned commit of that branch, so these names are how the
/// update finds its way back to the tip.
const CORE_BRANCH: &str = "Playerbot";
const MODULE_BRANCH: &str = "master";

/// Files whose current bytes must survive the git advance. `composegen` writes
/// the generated compose INTO the checkout, on top of AzerothCore's own
/// tracked `docker-compose.yml` — see `modmgr::wow_advance_repo`'s `protect`.
/// The other two are untracked on a native install, so restoring them is a
/// no-op there and cheap insurance if that ever changes.
const CORE_PROTECTED_FILES: [&str; 3] =
    ["docker-compose.yml", "docker-compose.override.yml", "docker-compose.build.yml"];

/// How to move a checkout forward, decided by what it actually IS: a detached
/// HEAD (every native install) can only be re-detached onto the branch tip; an
/// attached one keeps the bash oracle's `pull --ff-only`.
/// Where the update stages the EMBEDDED core patch while it works. Untracked,
/// so `git stash` (no `-u`) and `git checkout` both leave it alone; removed on
/// every exit path.
const UNBOUND_PATCH_TMP: &str = ".dml-update-unbound.patch";

/// `git apply` rewrites six files totalling ~1.2 MB — a probe budget is too
/// tight, and a patch that blows this deadline must not read as "applied".
const GIT_PATCH_TIMEOUT: Duration = Duration::from_secs(60);

/// What the update must do about Wrath Unbound before it can move the core.
///
/// Unbound is not a module: it EDITS six AzerothCore source files in place, so
/// its changes sit in the checkout as local edits that any core update has to
/// merge. Rather than refuse the update (the first shape of this guard) or let
/// the conflict recovery silently drop the patch (the shape before that — see
/// `wow_advance_repo`), the update REVERTS the patch cleanly, moves the core,
/// and re-applies it to the new tree.
///
/// That is safe because the patch is the only thing at risk: everything a
/// player earned through Unbound lives in the DATABASE, which no git operation
/// touches. Verified 2026-08-14 against the branch tip 151 commits ahead — all
/// 7 hunks applied clean, at line offsets. The alternative the user asked
/// about — `unbound uninstall` then reinstall — is NOT progress-safe: its
/// revert drops `unbound_character_unlocks` and `unbound_milestones` and
/// restores `ValidateSkillLearnedBySpells` to AzerothCore's default, which
/// strips every character's cross-class spells at their next login.
#[derive(Debug, PartialEq, Eq)]
enum UnboundPrep {
    /// No patch on this tree — an ordinary server, nothing to do.
    NotPresent,
    /// Reverted; [`UNBOUND_PATCH_TMP`] is staged and MUST be re-applied.
    Reverted,
    /// Cannot proceed safely. The core is skipped (the module still updates —
    /// Unbound never touches `modules/mod-playerbots`). Carries the reason,
    /// which is shown to the user verbatim.
    Refused(String),
}

/// The reason a `PatchPresence` cannot be prepared, or `None` when it can.
/// Pure half of [`prepare_unbound`], so the classification is testable without
/// a checkout.
fn unbound_prep_refusal(presence: &crate::unbound::PatchPresence) -> Option<String> {
    match presence {
        crate::unbound::PatchPresence::None => None,
        crate::unbound::PatchPresence::All => None,
        // A HALF-applied patch is the more fragile state, not the safer one,
        // and a reverse-apply cannot succeed on a mixed tree (the uninstall
        // engine reaches the same conclusion at `do_patch_revert`).
        crate::unbound::PatchPresence::Mixed { missing } => Some(format!(
            "Wrath Unbound's core patch is HALF-applied ({} of 6 files) -- skipping the AzerothCore update. Restore the six files (git checkout -- <file>) and retry.",
            6 - missing.len()
        )),
    }
}

/// `true` on a successful bounded `git -C <dir> <args…>`.
fn git_ok_in(program: &OsStr, dir: &Path, args: &[&str], timeout: Duration) -> bool {
    let mut cmd = Command::new(program);
    cmd.arg("-C").arg(dir).args(args);
    windows_no_window(&mut cmd);
    matches!(output_bounded_draining(cmd, timeout), Some(out) if out.status.success())
}

/// Stage the embedded patch and reverse-apply it, leaving a clean core tree.
/// `patch_body` is a PARAMETER, not a lookup, so tests can drive the whole
/// revert/re-apply cycle through real `git apply` with a small synthetic patch
/// instead of the 1.2 MB core payload. Production always passes the EMBEDDED
/// bytes ([`crate::unbound_payload::PATCH_DEST`]) — never the copy in
/// `modules/mod-unbound`, which is the user's to delete.
fn prepare_unbound(
    program: &OsStr,
    sdir: &Path,
    presence: &crate::unbound::PatchPresence,
    patch_body: &[u8],
    emit: &impl Fn(serde_json::Value),
) -> UnboundPrep {
    if let Some(reason) = unbound_prep_refusal(presence) {
        return UnboundPrep::Refused(reason);
    }
    if *presence == crate::unbound::PatchPresence::None {
        return UnboundPrep::NotPresent;
    }

    if std::fs::write(sdir.join(UNBOUND_PATCH_TMP), patch_body).is_err() {
        return UnboundPrep::Refused("could not stage Wrath Unbound's patch -- skipping the AzerothCore update.".to_string());
    }

    crate::lifecycle::gl_line(emit, "info", "Wrath Unbound detected -- reverting its core patch so the update can apply cleanly.");
    if !git_ok_in(program, sdir, &["apply", "-R", "--check", UNBOUND_PATCH_TMP], GIT_PATCH_TIMEOUT) {
        let _ = std::fs::remove_file(sdir.join(UNBOUND_PATCH_TMP));
        return UnboundPrep::Refused(
            "Wrath Unbound's core patch cannot be cleanly reverted (the six files have diverged from the shipped patch) -- skipping the AzerothCore update. Nothing was changed.".to_string(),
        );
    }
    if !git_ok_in(program, sdir, &["apply", "-R", UNBOUND_PATCH_TMP], GIT_PATCH_TIMEOUT) {
        let _ = std::fs::remove_file(sdir.join(UNBOUND_PATCH_TMP));
        return UnboundPrep::Refused(
            "Wrath Unbound's core patch failed to revert after a clean check -- skipping the AzerothCore update.".to_string(),
        );
    }
    // Believe the FILES, not git's exit code -- but ask the question that
    // actually matters, which is NOT "is the symbol gone from all six files".
    //
    // That was the first version, and it refused a healthy server (found live
    // on the VM 2026-08-15, on the very first real run). Its `Player.cpp`
    // carries TWO `UnboundClassMask` references: the one this patch owns
    // (trainer spell visibility) and a separate hand edit that lets multiclass
    // characters learn talents from an unlocked class. A symbol-absence probe
    // cannot tell "our patch is still applied" from "someone else's edit also
    // mentions it", so a correct, complete revert still read as a failure.
    //
    // Extra local edits that merely REFERENCE the symbol are ordinary local
    // work, and `wow_advance_repo`'s stash carries them across exactly like
    // any other (the VM's Feral Spirit patch takes that same route). What this
    // step must establish is narrower and entirely about OUR patch: it is off
    // the tree, and it can go back on. A forward `--check` proves both at once
    // -- it can only succeed if the lines the patch adds are absent.
    if !git_ok_in(program, sdir, &["apply", "--check", UNBOUND_PATCH_TMP], GIT_PATCH_TIMEOUT) {
        // Put it back rather than leave a half-reverted tree behind. The
        // reverse-apply already passed its own `--check` and `git apply` is
        // atomic, so this restore is the same operation in reverse and should
        // not fail -- if it does, say so, because a tree with Unbound half
        // off is the one state nobody can recover from by guessing.
        if git_ok_in(program, sdir, &["apply", UNBOUND_PATCH_TMP], GIT_PATCH_TIMEOUT) {
            let _ = std::fs::remove_file(sdir.join(UNBOUND_PATCH_TMP));
            return UnboundPrep::Refused(
                "Wrath Unbound's core patch reverted but will not re-apply -- skipping the AzerothCore update. It has been put back and the server is unchanged.".to_string(),
            );
        }
        let _ = std::fs::remove_file(sdir.join(UNBOUND_PATCH_TMP));
        return UnboundPrep::Refused(format!(
            "Wrath Unbound's core patch was reverted and could NOT be put back. Do not rebuild. Restore by hand: cd {} && git checkout -- {}",
            sdir.display(),
            crate::unbound::PATCHED_FILES.join(" ")
        ));
    }
    UnboundPrep::Reverted
}

/// Re-apply the staged patch to the UPDATED tree. `Ok(())` when the six files
/// carry the symbols again; `Err(reason)` when the caller must roll back.
fn reapply_unbound(program: &OsStr, sdir: &Path) -> Result<(), String> {
    if !git_ok_in(program, sdir, &["apply", "--check", UNBOUND_PATCH_TMP], GIT_PATCH_TIMEOUT) {
        return Err("Wrath Unbound's core patch does not apply to the updated AzerothCore".to_string());
    }
    if !git_ok_in(program, sdir, &["apply", UNBOUND_PATCH_TMP], GIT_PATCH_TIMEOUT) {
        return Err("Wrath Unbound's core patch failed to apply after a clean check".to_string());
    }
    if crate::unbound::probe_patch_presence(sdir) != crate::unbound::PatchPresence::All {
        return Err("Wrath Unbound's patch applied but the six files do not all carry the change".to_string());
    }
    Ok(())
}

/// How to move a checkout forward, decided by what it actually IS: a detached
/// HEAD (every native install) can only be re-detached onto the branch tip; an
/// attached one keeps the bash oracle's `pull --ff-only`.
///
/// Everything needed to put a half-done update back sits in
/// [`UpdateRollback`] below.
fn advance_for<'a>(branch_reading: &str, tracks: &'a str) -> crate::modmgr::Advance<'a> {
    if branch_reading == crate::modmgr::DETACHED_HEAD {
        crate::modmgr::Advance::FetchDetach { branch: tracks }
    } else {
        crate::modmgr::Advance::PullFfOnly
    }
}

/// Everything needed to put a half-done update back. An empty sha means that
/// repo never moved, so there is nothing to undo — a rollback target for a
/// repo that never moved is an invitation to move it.
#[derive(Debug, Default, PartialEq, Eq)]
struct UpdateRollback {
    ac_sha: String,
    ac_detached: bool,
    pb_sha: String,
    pb_detached: bool,
    /// Unbound's patch is currently OFF the tree and must be put back.
    unbound: bool,
}

/// Put one repo back on `sha`, preserving whatever the user has in the tree.
///
/// The two shapes need different verbs, and BOTH of these refuse rather than
/// destroy: `checkout --detach` and `reset --keep` each abort if the move
/// would clobber a locally modified file. A plain `reset --hard` would be
/// simpler and would silently delete the user's edits — a rollback exists to
/// undo our own damage, so it must not be able to do any of its own.
fn restore_repo(program: &OsStr, dir: &Path, sha: &str, detached: bool) -> bool {
    if sha.is_empty() {
        return true; // never moved
    }
    if detached {
        git_ok_in(program, dir, &["checkout", "--detach", sha], GIT_PATCH_TIMEOUT)
    } else {
        git_ok_in(program, dir, &["reset", "--keep", sha], GIT_PATCH_TIMEOUT)
    }
}

/// Undo a failed update across BOTH repos and put Unbound's patch back.
/// `true` when everything landed; the caller must say so either way, loudly.
fn roll_back_update(
    program: &OsStr,
    sdir: &Path,
    moddir: &Path,
    undo: &UpdateRollback,
    emit: &impl Fn(serde_json::Value),
) -> bool {
    if *undo == UpdateRollback::default() {
        return true; // nothing moved and no patch is off the tree
    }
    crate::lifecycle::gl_line(emit, "warn", "Rolling everything back to where it was before the update...");

    let ac_ok = restore_repo(program, sdir, &undo.ac_sha, undo.ac_detached);
    let pb_ok = restore_repo(program, moddir, &undo.pb_sha, undo.pb_detached);
    // The patch goes back on whatever commit we ended up on: if the core
    // rollback worked that is the original, and if it did not, an Unbound tree
    // on a moved core is still far better than a stripped one.
    let unbound_ok = !undo.unbound || reapply_unbound(program, sdir).is_ok();

    if ac_ok && pb_ok && unbound_ok {
        crate::lifecycle::gl_line(
            emit,
            "warn",
            "Rolled back. The core and the playerbots module are exactly as they were -- nothing was updated, nothing was lost, and no rebuild is needed.",
        );
        return true;
    }
    if !ac_ok {
        crate::lifecycle::gl_line(emit, "error", format!("Could not roll AzerothCore back to {}. Restore by hand: cd {} && git checkout --detach {}", undo.ac_sha, sdir.display(), undo.ac_sha));
    }
    if !pb_ok {
        crate::lifecycle::gl_line(emit, "error", format!("Could not roll mod-playerbots back to {}. Restore by hand: cd {} && git checkout --detach {}", undo.pb_sha, moddir.display(), undo.pb_sha));
    }
    if !unbound_ok {
        crate::lifecycle::gl_line(emit, "error", "Wrath Unbound's core patch is NOT applied. Do NOT rebuild -- a rebuild now would produce a worldserver without Unbound while its database still has it.");
    }
    false
}

pub fn update_stream(backup: Option<bool>, emit: impl Fn(serde_json::Value)) {
    use crate::{config::ConfigReader, maint, modmgr, native};

    emit(serde_json::json!({"event": "section_start", "name": WOW_UPDATE_SECTION}));

    let title_dir = ConfigReader::title_dir_from_env();
    let Some(sdir) = maint::resolve_server_dir(&title_dir) else {
        emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
        emit(crate::lifecycle::gl_error("NOT_FOUND", "WoW Playerbots server not installed", "Install it first."));
        return;
    };

    if !modmgr::is_git_checkout(&sdir) {
        emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
        emit(crate::lifecycle::gl_error("GIT_MISSING", format!("{} is not a git checkout", sdir.display()), "Can't update from source."));
        return;
    }

    let git_program = std::ffi::OsString::from("git");

    // AzerothCore must be the custom mod-playerbots fork on the Playerbot
    // branch -- pulling upstream azerothcore/azerothcore-wotlk here would
    // break the playerbots integration. No override: hard error.
    let acurl = modmgr::git_remote_url(&git_program, &sdir);
    if !modmgr::wow_remote_ok(&acurl, "mod-playerbots/azerothcore-wotlk") {
        emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
        emit(crate::lifecycle::gl_error(
            "REMOTE_MISMATCH",
            "AzerothCore origin is not the expected Playerbots fork",
            &format!(
                "found: {} -- pulling upstream AzerothCore would break Playerbots. Fix the remote manually, then retry.",
                if acurl.is_empty() { "<none>" } else { acurl.as_str() }
            ),
        ));
        return;
    }

    let moddir = sdir.join("modules").join("mod-playerbots");
    let pb_present = modmgr::is_git_checkout(&moddir);
    if pb_present {
        let pburl = modmgr::git_remote_url(&git_program, &moddir);
        if !modmgr::wow_remote_ok(&pburl, "mod-playerbots/mod-playerbots") {
            emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
            emit(crate::lifecycle::gl_error(
                "REMOTE_MISMATCH",
                "mod-playerbots origin is not the expected fork",
                &format!("found: {}", if pburl.is_empty() { "<none>" } else { pburl.as_str() }),
            ));
            return;
        }
    }

    let acbranch = modmgr::git_branch(&git_program, &sdir);
    if !modmgr::update_branch_ok(&acbranch, CORE_BRANCH) {
        emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
        emit(crate::lifecycle::gl_error(
            "BRANCH_MISMATCH",
            if acbranch.is_empty() {
                "could not read the AzerothCore checkout's branch".to_string()
            } else {
                format!("AzerothCore checkout is on branch '{acbranch}' (expected '{CORE_BRANCH}' or a detached pin)")
            },
            "Switch it back before updating: git checkout Playerbot",
        ));
        return;
    }

    let Some(do_backup) = backup else {
        emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
        emit(crate::lifecycle::gl_error(
            "BAD_ARG",
            "Pick --backup or --no-backup",
            "New core revisions can run DB migrations at next start -- decide explicitly.",
        ));
        return;
    };

    if do_backup {
        let docker_program = native::docker_program();
        let db_cfg = crate::db::DbConfig::from_env();
        if !modmgr::module_backup_now(&docker_program, &db_cfg, &emit) {
            emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
            emit(crate::lifecycle::gl_error("BACKUP_FAILED", "Safety backup failed -- update not started", ""));
            return;
        }
    }

    let mut changed = false;

    // See `UnboundPrep`: the patch is reverted, the core moves, the patch goes
    // back on. ONLY the core is ever skipped -- Unbound never touches
    // `modules/mod-playerbots`, so the module updates either way, and a
    // refusal that blocked it too would answer a question nobody asked.
    let prep = match crate::unbound_payload::file(crate::unbound_payload::PATCH_DEST) {
        Some(f) => {
            prepare_unbound(&git_program, &sdir, &crate::unbound::probe_patch_presence(&sdir), f.body.as_bytes(), &emit)
        }
        None => UnboundPrep::Refused(
            "Wrath Unbound's patch is missing from this build -- skipping the AzerothCore update.".to_string(),
        ),
    };

    let core_skipped = matches!(prep, UnboundPrep::Refused(_));
    if let UnboundPrep::Refused(reason) = &prep {
        crate::lifecycle::gl_line(&emit, "warn", reason.clone());
        crate::lifecycle::gl_line(
            &emit,
            "warn",
            "The playerbots MODULE still updates -- Wrath Unbound does not touch modules/mod-playerbots.",
        );
    }

    // ALL OR NOTHING (user requirement, 2026-08-14): "if failed it should go
    // back to before the update, nothing should keep the updates if it fails,
    // just so playerbot core and mod isn't in 2 different versions."
    //
    // The core and the module are ONE product built from two repos, so a run
    // that moves one and fails on the other leaves a combination nobody
    // upstream tests -- and the failure that produces it (a network drop on
    // the second fetch) is far more likely than either repo being broken. So
    // BOTH rollback targets are captured before anything moves, and any
    // failure past this point restores both.
    //
    // Unbound's patch stays REVERTED for the whole window and is re-applied
    // exactly once at the end, whichever commit we finish on. Re-applying it
    // earlier would mean reverting it again before every rollback checkout --
    // `git checkout` refuses to clobber modified files, so a rollback with the
    // patch on the tree would fail precisely when it is needed.
    let ac_before = modmgr::git_short_head(&git_program, &sdir);
    let pb_branch = if pb_present { modmgr::git_branch(&git_program, &moddir) } else { String::new() };
    let pb_before = if pb_present { modmgr::git_short_head(&git_program, &moddir) } else { String::new() };
    let mut undo = UpdateRollback {
        ac_sha: String::new(),
        ac_detached: acbranch == modmgr::DETACHED_HEAD,
        pb_sha: String::new(),
        pb_detached: pb_branch == modmgr::DETACHED_HEAD,
        unbound: prep == UnboundPrep::Reverted,
    };

    let mut failed: Option<String> = None;

    // -- core -----------------------------------------------------------------
    let mut ac_changed = false;
    if !core_skipped {
        let sha = modmgr::git_head_sha(&git_program, &sdir);
        match modmgr::wow_advance_repo(
            &git_program,
            &sdir,
            "AzerothCore",
            &advance_for(&acbranch, CORE_BRANCH),
            &CORE_PROTECTED_FILES,
            &emit,
        ) {
            Ok(o) => {
                // Recorded only AFTER it really moved: `wow_advance_repo`
                // restores the tree itself on failure, and a rollback target
                // for a repo that never moved is an invitation to move it.
                undo.ac_sha = sha;
                ac_changed = o.changed;
                // A dropped local core patch IS a failure, whatever git's exit
                // code says. `wow_advance_repo`'s conflict path keeps the
                // update and puts the user's edits in a patch file + the stash
                // -- right for one module, wrong for the core, where those
                // edits are why the server behaves the way it does. Live case
                // (2026-08-14): the VM carries a Feral Spirit / hunter-pet
                // patch on two files Unbound never touches, so none of the
                // Unbound machinery above would have noticed it going missing
                // until a player's pet vanished.
                if o.edits_conflicted {
                    failed = Some(
                        "local AzerothCore changes could not be re-applied on top of the update (your own core patches)"
                            .to_string(),
                    );
                }
            }
            // The advance emitted its own error event already.
            Err(()) => failed = Some("the AzerothCore update failed".to_string()),
        }
    }

    // -- module ---------------------------------------------------------------
    let mut pb_changed = false;
    if failed.is_none() && pb_present {
        let sha = modmgr::git_head_sha(&git_program, &moddir);
        match modmgr::wow_advance_repo(
            &git_program,
            &moddir,
            "mod-playerbots",
            &advance_for(&pb_branch, MODULE_BRANCH),
            &[],
            &emit,
        ) {
            Ok(o) => {
                undo.pb_sha = sha;
                pb_changed = o.changed;
                if o.edits_conflicted {
                    failed = Some(
                        "local mod-playerbots changes could not be re-applied on top of the update".to_string(),
                    );
                }
            }
            Err(()) => failed = Some("the mod-playerbots update failed".to_string()),
        }
    } else if !pb_present {
        crate::lifecycle::gl_line(&emit, "warn", "modules/mod-playerbots not found -- skipping module update.");
    }

    // -- Unbound goes back on, last ------------------------------------------
    if failed.is_none() && undo.unbound {
        match reapply_unbound(&git_program, &sdir) {
            Ok(()) => {
                undo.unbound = false;
                crate::lifecycle::gl_line(
                    &emit,
                    "info",
                    "Wrath Unbound's core patch re-applied to the updated AzerothCore -- your Unbound data was never touched.",
                );
            }
            Err(reason) => failed = Some(reason),
        }
    }

    if let Some(why) = failed {
        crate::lifecycle::gl_line(&emit, "error", format!("UPDATE FAILED: {why}."));
        let restored = roll_back_update(&git_program, &sdir, &moddir, &undo, &emit);
        let _ = std::fs::remove_file(sdir.join(UNBOUND_PATCH_TMP));
        emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "error"}));
        emit(crate::lifecycle::gl_error(
            "UPDATE_FAILED",
            why,
            if restored {
                "Everything was rolled back -- the core and the module are exactly as they were, and nothing needs rebuilding."
            } else {
                "ROLLBACK INCOMPLETE -- see the lines above and do NOT rebuild until the repos are back in step."
            },
        ));
        return;
    }
    let _ = std::fs::remove_file(sdir.join(UNBOUND_PATCH_TMP));

    let ac_summary = if core_skipped {
        "skipped (Wrath Unbound)".to_string()
    } else {
        modmgr::pull_summary(&ac_before, &modmgr::git_short_head(&git_program, &sdir))
    };
    let pb_summary = if pb_present {
        modmgr::pull_summary(&pb_before, &modmgr::git_short_head(&git_program, &moddir))
    } else {
        "skipped".to_string()
    };
    if ac_changed || pb_changed {
        changed = true;
    }

    if changed {
        let _ = modmgr::rebuild_pending_add(&sdir, "core-update");
        crate::lifecycle::gl_line(&emit, "info", "Rebuild required to compile the update -- use the rebuild banner on this page.");
    }

    emit(serde_json::json!({"event": "section_end", "name": WOW_UPDATE_SECTION, "status": "ok"}));
    emit(serde_json::json!({"event": "done", "data": {"changed": changed, "ac": ac_summary, "playerbots": pb_summary}}));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modmgr;

    // -- the native self-update fix (2026-08-14) -----------------------------
    //
    // The fix has TWO INDEPENDENT HALVES and the repo has already been bitten
    // by testing only the conjunction (root CLAUDE.md, "A FIX WITH TWO HALVES
    // IS NOT PROVEN BY A SEQUENCE TEST"): (a) the branch gate must accept a
    // detached HEAD, and (b) the advance must then FETCH+RE-DETACH instead of
    // running `git pull`, which cannot move a detached HEAD at all. Each half
    // gets its own isolating test below, because deleting either one leaves a
    // native update just as broken — with a different error code.

    #[test]
    fn advance_for_detached_head_fetches_and_re_detaches() {
        // HALF (b), isolated: this is the assertion that goes red if the
        // advance is reverted to a bare `pull --ff-only`, regardless of what
        // the gate does.
        assert_eq!(
            advance_for(crate::modmgr::DETACHED_HEAD, CORE_BRANCH),
            crate::modmgr::Advance::FetchDetach { branch: "Playerbot" }
        );
        assert_eq!(
            advance_for(crate::modmgr::DETACHED_HEAD, MODULE_BRANCH),
            crate::modmgr::Advance::FetchDetach { branch: "master" }
        );
    }

    #[test]
    fn advance_for_attached_branch_keeps_the_bash_oracles_pull() {
        // The WSL `.sh` route leaves an attached checkout; that path must not
        // change shape just because the native one needed a new one.
        assert_eq!(advance_for("Playerbot", CORE_BRANCH), crate::modmgr::Advance::PullFfOnly);
        assert_eq!(advance_for("master", MODULE_BRANCH), crate::modmgr::Advance::PullFfOnly);
    }

    #[test]
    fn unbound_prep_refuses_only_a_half_applied_patch() {
        use crate::unbound::PatchPresence;
        // A fully applied patch is NOT a refusal any more: it is reverted,
        // the core moves, and it goes back on. That is the whole point of the
        // 2026-08-14 round -- the alternative (`unbound uninstall`) DROPS
        // `unbound_character_unlocks` and `unbound_milestones`.
        assert_eq!(unbound_prep_refusal(&PatchPresence::All), None);
        assert_eq!(unbound_prep_refusal(&PatchPresence::None), None);

        // Half-applied is the more fragile state: a reverse-apply cannot
        // succeed on a mixed tree, so this one still refuses -- and names the
        // count, because "some of six" is the only useful thing to say.
        let refusal = unbound_prep_refusal(&PatchPresence::Mixed {
            missing: vec![
                "src/server/game/Entities/Player/Player.h".to_string(),
                "src/server/game/Entities/Player/Player.cpp".to_string(),
            ],
        })
        .expect("a half-applied patch must refuse");
        assert!(refusal.contains("HALF-applied"), "refusal must name the state: {refusal}");
        assert!(refusal.contains("4 of 6"), "refusal must name the count: {refusal}");
    }

    // -- all-or-nothing rollback, through REAL git ----------------------------
    //
    // User requirement (2026-08-14): "if failed it should go back to before
    // the update, nothing should keep the updates if it fails, just so
    // playerbot core and mod isn't in 2 different versions."

    /// A repo with two commits, left DETACHED on the second. Returns
    /// `(dir, first_sha, second_sha)`.
    fn two_commit_repo(root: &Path, name: &str) -> (PathBuf, String, String) {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        git_must(&dir, &["-c", "init.defaultBranch=master", "init", "--quiet"]);
        git_must(&dir, &["config", "user.email", "t@example.com"]);
        git_must(&dir, &["config", "user.name", "t"]);
        std::fs::write(dir.join("src.txt"), "A").unwrap();
        git_must(&dir, &["add", "-A"]);
        git_must(&dir, &["commit", "-q", "-m", "A"]);
        let a = modmgr::git_head_sha(OsStr::new("git"), &dir);
        std::fs::write(dir.join("src.txt"), "B").unwrap();
        git_must(&dir, &["commit", "-q", "-am", "B"]);
        let b = modmgr::git_head_sha(OsStr::new("git"), &dir);
        git_must(&dir, &["checkout", "--quiet", "--detach", &b]);
        (dir, a, b)
    }

    #[test]
    fn a_failed_module_update_rolls_the_core_back_too() {
        // THE case the requirement is about: the core moved, the module blew
        // up, and leaving it there would run new core against old bots.
        let root = std::env::temp_dir().join(format!("dml-rollback-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sdir, ac_old, ac_new) = two_commit_repo(&root, "server");
        let (moddir, _pb_old, pb_new) = two_commit_repo(&root, "module");

        // Core advanced (now on ac_new, rollback target ac_old); the module
        // never moved, so its sha stays empty.
        let undo = UpdateRollback { ac_sha: ac_old.clone(), ac_detached: true, ..Default::default() };
        assert_eq!(modmgr::git_head_sha(OsStr::new("git"), &sdir), ac_new);

        assert!(roll_back_update(OsStr::new("git"), &sdir, &moddir, &undo, &|_| {}));
        assert_eq!(modmgr::git_head_sha(OsStr::new("git"), &sdir), ac_old, "the core was not rolled back");
        assert_eq!(
            modmgr::git_head_sha(OsStr::new("git"), &moddir),
            pb_new,
            "a repo that never moved must not be touched by the rollback"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rollback_of_both_repos_puts_each_back_on_its_own_sha() {
        let root = std::env::temp_dir().join(format!("dml-rollback2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sdir, ac_old, _) = two_commit_repo(&root, "server");
        let (moddir, pb_old, _) = two_commit_repo(&root, "module");

        let undo = UpdateRollback {
            ac_sha: ac_old.clone(),
            ac_detached: true,
            pb_sha: pb_old.clone(),
            pb_detached: true,
            unbound: false,
        };
        assert!(roll_back_update(OsStr::new("git"), &sdir, &moddir, &undo, &|_| {}));
        assert_eq!(modmgr::git_head_sha(OsStr::new("git"), &sdir), ac_old);
        assert_eq!(modmgr::git_head_sha(OsStr::new("git"), &moddir), pb_old);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_repo_refuses_rather_than_destroying_local_edits() {
        // The comment on `restore_repo` claims a plain `reset --hard` would
        // silently delete the user's work and that these verbs will not. If
        // that ever stops being true, this goes red instead of a user's edits
        // going missing.
        let root = std::env::temp_dir().join(format!("dml-rollback3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sdir, ac_old, _) = two_commit_repo(&root, "server");
        std::fs::write(sdir.join("src.txt"), "MY PRECIOUS EDIT").unwrap();

        assert!(
            !restore_repo(OsStr::new("git"), &sdir, &ac_old, true),
            "a rollback that would clobber a local edit must FAIL, not succeed"
        );
        assert_eq!(
            std::fs::read_to_string(sdir.join("src.txt")).unwrap(),
            "MY PRECIOUS EDIT",
            "the edit was destroyed by the rollback"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_empty_rollback_does_nothing_at_all() {
        let root = std::env::temp_dir().join(format!("dml-rollback4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let (sdir, _, ac_new) = two_commit_repo(&root, "server");
        let (moddir, _, pb_new) = two_commit_repo(&root, "module");

        assert!(roll_back_update(OsStr::new("git"), &sdir, &moddir, &UpdateRollback::default(), &|_| {}));
        assert_eq!(modmgr::git_head_sha(OsStr::new("git"), &sdir), ac_new);
        assert_eq!(modmgr::git_head_sha(OsStr::new("git"), &moddir), pb_new);
        let _ = std::fs::remove_dir_all(&root);
    }

    // -- the revert -> update -> re-apply cycle, through REAL git -------------
    //
    // The whole reason this exists: `unbound uninstall` is NOT progress-safe
    // (its revert DROPs `unbound_character_unlocks` and `unbound_milestones`,
    // and restores `ValidateSkillLearnedBySpells` to AzerothCore's default,
    // which strips every character's cross-class spells at next login). So the
    // update must carry the patch across a core change WITHOUT touching the
    // database at all. These drive that cycle end to end with a real repo, a
    // real `git apply`, and the real `probe_patch_presence` reading the real
    // six paths -- only the patch BODY is synthetic, so the fixture stays small.

    fn git_must(dir: &Path, args: &[&str]) {
        let out = Command::new("git").arg("-C").arg(dir).args(args).output().expect("git must be on PATH");
        assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
    }

    /// A plausible core source file: 20 filler lines, the line Unbound anchors
    /// on, 20 more, then `tail`. Long enough that an edit at either end sits
    /// well outside the diff's 3 lines of context.
    fn core_file_body(tail: &str) -> String {
        let mut s = String::from("// upstream header\n");
        for i in 0..20 {
            s.push_str(&format!("int filler_{i}() {{ return {i}; }}\n"));
        }
        s.push_str("int anchor() { return 2; }\n");
        for i in 20..40 {
            s.push_str(&format!("int filler_{i}() {{ return {i}; }}\n"));
        }
        s.push_str(tail);
        s
    }

    /// A repo carrying the six real `PATCHED_FILES` paths, plus a patch (made
    /// by git itself) that adds the `UnboundClassMask` symbol to each.
    /// Returns `(root, sdir, patch_bytes)` with the patch ALREADY applied —
    /// i.e. the shape an Unbound server is in.
    fn unbound_fixture(tag: &str) -> (PathBuf, PathBuf, Vec<u8>) {
        let root = std::env::temp_dir().join(format!("dml-unbound-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let sdir = root.join("server");
        std::fs::create_dir_all(&sdir).unwrap();
        git_must(&sdir, &["-c", "init.defaultBranch=master", "init", "--quiet"]);
        git_must(&sdir, &["config", "user.email", "t@example.com"]);
        git_must(&sdir, &["config", "user.name", "t"]);

        // Clean tree: the six files with upstream-looking content. They must be
        // LONG relative to the diff's 3 lines of context -- a 4-line fixture
        // makes the context cover the whole file, so every upstream edit
        // "breaks" the patch and the happy path can never be tested.
        for rel in crate::unbound::PATCHED_FILES {
            let p = sdir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, core_file_body("")).unwrap();
        }
        git_must(&sdir, &["add", "-A"]);
        git_must(&sdir, &["commit", "-q", "-m", "clean core"]);

        // Apply "Unbound" by hand, then let GIT produce the patch from it.
        for rel in crate::unbound::PATCHED_FILES {
            let p = sdir.join(rel);
            let body = std::fs::read_to_string(&p).unwrap();
            std::fs::write(&p, body.replace("int anchor() { return 2; }", "int anchor() { return UnboundClassMask; }")).unwrap();
        }
        let out = Command::new("git").arg("-C").arg(&sdir).arg("diff").output().unwrap();
        let patch = out.stdout;
        assert!(!patch.is_empty(), "the fixture must produce a real patch");
        assert_eq!(crate::unbound::probe_patch_presence(&sdir), crate::unbound::PatchPresence::All);
        (root, sdir, patch)
    }

    /// Stand in for the core update: rewrite lines the patch does NOT touch,
    /// exactly like 151 upstream commits moving code around the hunk.
    fn simulate_core_update(sdir: &Path, breaks_the_patch: bool) {
        for rel in crate::unbound::PATCHED_FILES {
            let p = sdir.join(rel);
            let body = std::fs::read_to_string(&p).unwrap();
            let updated = if breaks_the_patch {
                // Upstream rewrote the very line Unbound anchors on.
                body.replace("int anchor() { return 2; }", "int anchor(int mode) { return mode; }")
            } else {
                // Edits at BOTH ends, far outside the hunk's context -- which
                // is what 151 commits of unrelated core work actually looks
                // like, and what shifts the hunk's line offsets.
                format!("// NEW upstream line\n{}int upstream_tail() {{ return 99; }}\n", body.replace("int filler_0() { return 0; }", "int filler_0() { return 1000; }"))
            };
            std::fs::write(&p, updated).unwrap();
        }
        git_must(sdir, &["commit", "-q", "-am", "upstream moved on"]);
    }

    #[test]
    fn unbound_patch_is_reverted_and_re_applied_across_a_core_update() {
        let (root, sdir, patch) = unbound_fixture("carry");
        let git = OsStr::new("git");

        let prep = prepare_unbound(git, &sdir, &crate::unbound::probe_patch_presence(&sdir), &patch, &|_| {});
        assert_eq!(prep, UnboundPrep::Reverted);
        assert_eq!(
            crate::unbound::probe_patch_presence(&sdir),
            crate::unbound::PatchPresence::None,
            "the tree must be clean of Unbound before the core moves"
        );

        simulate_core_update(&sdir, false);

        reapply_unbound(git, &sdir).expect("the patch must survive an ordinary core update");
        assert_eq!(
            crate::unbound::probe_patch_presence(&sdir),
            crate::unbound::PatchPresence::All,
            "all six files must carry the symbol again"
        );
        // The update's own content must still be there -- re-applying the
        // patch must not undo the core update it was carried across.
        let one = std::fs::read_to_string(sdir.join(crate::unbound::PATCHED_FILES[0])).unwrap();
        assert!(one.contains("NEW upstream line"), "the core update was lost: {one}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn an_extra_hand_edit_mentioning_the_symbol_does_not_block_the_update() {
        // THE VM's OWN CASE, 2026-08-15, first real run. Its `Player.cpp`
        // carries two `UnboundClassMask` references: the one the shipped patch
        // owns, and a separate hand edit allowing multiclass talent learning.
        // The first version of this check asked "is the symbol gone from all
        // six files?" and so refused a server whose revert had worked
        // perfectly -- a symbol-absence probe cannot tell "our patch is still
        // applied" from "someone else also mentions it".
        let (root, sdir, patch) = unbound_fixture("handedit");

        // A second, independent reference the shipped patch knows nothing
        // about -- in a file the patch DOES touch, which is what made this
        // indistinguishable from a failed revert.
        let victim = sdir.join(crate::unbound::PATCHED_FILES[2]);
        let body = std::fs::read_to_string(&victim).unwrap();
        std::fs::write(&victim, format!("{body}int hand_edit() {{ return UnboundClassMask; }}\n")).unwrap();
        git_must(&sdir, &["commit", "-q", "-am", "someone's own talent fix"]);

        let prep = prepare_unbound(OsStr::new("git"), &sdir, &crate::unbound::probe_patch_presence(&sdir), &patch, &|_| {});
        assert_eq!(prep, UnboundPrep::Reverted, "a healthy server with its own extra edit must not be refused");

        // The hand edit is untouched -- it was never ours to revert.
        assert!(
            std::fs::read_to_string(&victim).unwrap().contains("int hand_edit()"),
            "the update must not disturb an edit the shipped patch does not own"
        );

        // And the patch still goes back on afterwards.
        simulate_core_update(&sdir, false);
        reapply_unbound(OsStr::new("git"), &sdir).expect("the patch must still re-apply alongside the hand edit");
        assert_eq!(crate::unbound::probe_patch_presence(&sdir), crate::unbound::PatchPresence::All);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stale_unbound_patch_is_refused_rather_than_half_applied() {
        let (root, sdir, patch) = unbound_fixture("stale");
        let git = OsStr::new("git");

        assert_eq!(
            prepare_unbound(git, &sdir, &crate::unbound::probe_patch_presence(&sdir), &patch, &|_| {}),
            UnboundPrep::Reverted
        );
        simulate_core_update(&sdir, true);

        let err = reapply_unbound(git, &sdir).expect_err("a patch whose anchor is gone must NOT report success");
        assert!(err.contains("does not apply"), "the reason must name the cause: {err}");
        assert_eq!(
            crate::unbound::probe_patch_presence(&sdir),
            crate::unbound::PatchPresence::None,
            "a refused re-apply must leave NO partial patch behind"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn prepare_unbound_refuses_a_patch_it_did_not_write() {
        // The tree carries the symbols but not OUR patch -- reverse-apply
        // cannot work, and forcing it would corrupt someone's hand edits.
        let (root, sdir, _patch) = unbound_fixture("foreign");
        let git = OsStr::new("git");
        let foreign = b"diff --git a/src/server/game/Entities/Player/Player.h b/src/server/game/Entities/Player/Player.h\n--- a/src/server/game/Entities/Player/Player.h\n+++ b/src/server/game/Entities/Player/Player.h\n@@ -1,1 +1,2 @@\n-nothing that exists\n+UnboundClassMask\n";

        match prepare_unbound(git, &sdir, &crate::unbound::probe_patch_presence(&sdir), foreign, &|_| {}) {
            UnboundPrep::Refused(r) => assert!(r.contains("cannot be cleanly reverted"), "wrong reason: {r}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(
            crate::unbound::probe_patch_presence(&sdir),
            crate::unbound::PatchPresence::All,
            "a refusal must leave the server exactly as it was"
        );
        assert!(!sdir.join(UNBOUND_PATCH_TMP).exists(), "the staged patch must be cleaned up");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// LIVE, network: does the REAL Unbound patch still apply to the REAL
    /// current branch tip? Run before a core update:
    /// `cargo test -p dml-wow --lib live_unbound -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_unbound_patch_applies_to_the_current_core_tip() {
        let root = std::env::temp_dir().join(format!("dml-unbound-live-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        git_must(&root, &["-c", "init.defaultBranch=master", "init", "--quiet"]);
        for rel in crate::unbound::PATCHED_FILES {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            let url = format!("https://raw.githubusercontent.com/mod-playerbots/azerothcore-wotlk/{CORE_BRANCH}/{rel}");
            let out = Command::new("curl").args(["-sfL", &url, "-o"]).arg(&p).output().expect("curl");
            assert!(out.status.success() && p.exists(), "could not fetch {rel}");
        }
        let patch = crate::unbound_payload::file(crate::unbound_payload::PATCH_DEST).expect("pinned").body;
        std::fs::write(root.join(UNBOUND_PATCH_TMP), patch).unwrap();
        let out = Command::new("git")
            .arg("-C")
            .arg(&root)
            .args(["apply", "--check", "--verbose", UNBOUND_PATCH_TMP])
            .output()
            .unwrap();
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        assert!(out.status.success(), "the Unbound patch NO LONGER applies to {CORE_BRANCH} -- a core update would strand it");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn core_protected_files_include_the_generated_compose() {
        // `composegen` writes this INTO the checkout on top of AzerothCore's
        // tracked file; losing it to the conflict recovery's `reset --hard`
        // breaks the server the update was meant to improve.
        assert!(CORE_PROTECTED_FILES.contains(&"docker-compose.yml"));
    }

    // -- parse_docker_usage_lines / assemble_docker_usage --------------------

    /// Real `docker stats --no-stream` output from the Ubuntu box (locale
    /// decimals and all) — only the stack's `ac-*` rows survive, a foreign
    /// container on the same engine does not, and a stderr warning line
    /// interleaved by the combined read is skipped rather than fatal.
    #[test]
    fn parse_container_stats_keeps_ac_rows_and_skips_foreign_and_malformed() {
        let raw = "ac-worldserver\t102.87%\t4.827GiB / 15.51GiB\t31.12%\r\n\
                   ac-database\t0.82%\t890.1MiB / 15.51GiB\t5.60%\n\
                   someones-nginx\t0.01%\t10MiB / 15.51GiB\t0.06%\n\
                   WARNING: something docker printed\n";
        let got = parse_container_stats(raw);
        assert_eq!(got.len(), 2, "exactly the two ac-* rows: {got:?}");
        assert_eq!(got[0]["name"], "ac-worldserver");
        assert_eq!(got[0]["cpu"], "102.87%");
        assert_eq!(got[0]["mem"], "4.827GiB / 15.51GiB");
        assert_eq!(got[1]["mem_pct"], "5.60%");
    }

    #[test]
    fn parse_container_stats_empty_input_is_empty() {
        assert!(parse_container_stats("").is_empty());
    }

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
