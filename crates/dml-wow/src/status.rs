//! Native-mode status/console reads — SOAP `server info` parsing, the
//! `server-detail` composite (containers + SOAP + bots + ports + verdict),
//! and `console-tail` (spike: `spike/docker-desktop-native`, Task B2).
//!
//! WHY. `server-info`/`server-detail`/`console-tail` are the only DB-page
//! reads that also need SOAP and/or `docker` shell-outs (not just MySQL), so
//! they land in their own module rather than `pages.rs`. `server-detail`
//! backs the Home status card polled on an interval — every I/O call here is
//! bounded (via [`output_bounded_draining`], NOT `crate::output_bounded` —
//! see that function's own doc comment for why: `docker logs` output can
//! exceed the OS pipe buffer, which deadlocks the non-draining helper) so a
//! wedged/absent `docker` can never hang the poll.
//!
//! FAITHFUL PORTS. Each function mirrors, line for line, its `cli/src`
//! counterpart so the assembled JSON is byte-identical:
//!   - `server-info`   -> `90-main.sh:1405-1414` + `_parse_server_info`/
//!     `_parse_server_info_fields` (`40-config.sh:12-40`)
//!   - `server-detail` -> `90-main.sh:1415-1499` + `_detail_container_rows`/
//!     `_world_ready`/`_bots_counts`/`_host_port_json` (`40-config.sh:667-851`)
//!   - `console-tail`  -> `90-main.sh:1629-1656` + `_strip_ansi`/
//!     `_console_lines_json` (`45-console.sh:10-38`)
//! A cargo parity test (`status_parity.rs`, skip-guarded — the server may be
//! down) compares the STABLE fields (verdict, container states, world_ready,
//! bots.max, ports, classification) against a live `dml wow server-info` /
//! `server-detail` run; volatile fields (uptime, timing) are excluded, same
//! lesson as `soap_parity.rs`.
//!
//! NATIVE-MODE-ONLY by convention: WSL keeps calling `dml`; the Tauri command
//! layer (`lib.rs`) gates every entry point on `require_native_backend()`.

use std::ffi::OsStr;
use std::process::Command;
use std::time::Duration;

use serde_json::{json, Value};

use super::config::ConfigReader;
use super::db::{self, Database, DbConfig, DbError};
use super::soap::{self, SoapConfig, SoapOutcome};

/// Bounded timeout for every `docker inspect`/`ps`/`port`/`logs --since`
/// probe here — small reads that must never hang the status poll.
const DOCKER_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
/// `console-tail` can legitimately move more bytes (`--tail 1000`), so it
/// gets a longer budget than the small inspect/ps/port probes above.
const CONSOLE_TAIL_TIMEOUT: Duration = Duration::from_secs(20);

/// `true` when `s` is one-or-more ASCII digits — the bash `^[0-9]+$` guard
/// (same rule as `pages::is_all_digits`, duplicated locally to keep this
/// module self-contained).
fn is_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// [`windows_no_window`]/[`output_bounded_draining`] now live in
/// `dml_core::proc` (cargo-workspace refactor, Task 4 fix-up) — they're
/// generic, docker-agnostic subprocess primitives with no status/SOAP/DB
/// knowledge, so `dml-core` is their canonical home. Re-exported here under
/// the SAME names so every one of this crate's other call sites
/// (`dml::maint`'s docker-usage/port-check/update-check reads, `dml::modmgr`,
/// `dml::backup`, `dml::moduletail`, `dml::restore`, `lib.rs`, and this
/// module's own `docker_ps_rows`/`world_ready`/`host_port`/etc. below) keep
/// compiling unchanged, whether they reach these via a bare `use
/// super::status::{...}` import or a fully-qualified `super::status::`/
/// `status::` path.
pub use dml_core::proc::{output_bounded_draining, windows_no_window};

// ---------------------------------------------------------------------------
// `server-info` — `_parse_server_info_fields` / `_parse_server_info`.
// ---------------------------------------------------------------------------

/// Parse the raw text of a SOAP `server info` result into the JSON field
/// fragment `{"version","players","uptime","mean_ms","median_ms"}` (no
/// `online` key — shared by `server-info`'s envelope and `server-detail`'s
/// `soap` sub-object). A port of `_parse_server_info_fields`
/// (`40-config.sh:12-40`): strips the `&#xD;` entities `soap_parse_result`
/// leaves behind, then line-matches each field; unparseable fields become
/// `null` rather than failing the whole read.
pub fn parse_server_info_fields(raw: &str) -> Value {
    let raw = raw.replace("&#xD;", "");
    let mut version: Option<String> = None;
    let mut players_raw = String::new();
    let mut uptime: Option<String> = None;
    let mut mean_raw = String::new();
    let mut median_raw = String::new();

    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("AzerothCore rev. ") {
            version = Some(rest.to_string());
        } else if let Some(rest) = line.strip_prefix("Connected players: ") {
            players_raw = rest.split('.').next().unwrap_or("").to_string();
        } else if let Some(rest) = line.strip_prefix("Server uptime: ") {
            uptime = Some(rest.to_string());
        } else if line.contains("|- Mean:") {
            if let Some(idx) = line.find("Mean: ") {
                let after = &line[idx + "Mean: ".len()..];
                mean_raw = after.find("ms").map(|i| after[..i].to_string()).unwrap_or_else(|| after.to_string());
            }
        } else if line.contains("|- Median:") {
            if let Some(idx) = line.find("Median: ") {
                let after = &line[idx + "Median: ".len()..];
                median_raw = after.find("ms").map(|i| after[..i].to_string()).unwrap_or_else(|| after.to_string());
            }
        }
    }

    let players = if is_digits(&players_raw) { json!(players_raw.parse::<i64>().unwrap_or(0)) } else { Value::Null };
    let mean = if is_digits(&mean_raw) { json!(mean_raw.parse::<i64>().unwrap_or(0)) } else { Value::Null };
    let median = if is_digits(&median_raw) { json!(median_raw.parse::<i64>().unwrap_or(0)) } else { Value::Null };
    let version_json = version.filter(|v| !v.is_empty()).map(Value::String).unwrap_or(Value::Null);
    let uptime_json = uptime.filter(|v| !v.is_empty()).map(Value::String).unwrap_or(Value::Null);

    json!({
        "version": version_json,
        "players": players,
        "uptime": uptime_json,
        "mean_ms": mean,
        "median_ms": median,
    })
}

/// The `{"online":false,...}` shape `server-info` reports when the server is
/// unreachable or the SOAP call faulted — down is an ANSWER, not an error.
pub fn server_info_down() -> Value {
    json!({
        "online": false,
        "version": null,
        "players": null,
        "uptime": null,
        "mean_ms": null,
        "median_ms": null,
    })
}

/// Assemble the full `server-info` envelope from a SOAP outcome — a port of
/// `_parse_server_info` plus the `server-info)` arm's rc dispatch
/// (`90-main.sh:1405-1414`): `Ok` -> `{"online":true,...fields}`; `Fault`/
/// `Unreachable` -> [`server_info_down`] (down is data); `Auth` -> `Err(())`,
/// the ONE case that stays a hard error (creds are wrong, not the server) —
/// the caller maps that to the `SOAP_AUTH` command error.
pub fn assemble_server_info(outcome: &SoapOutcome) -> Result<Value, ()> {
    match outcome {
        SoapOutcome::Ok(text) => {
            let mut fields = parse_server_info_fields(text);
            if let Some(obj) = fields.as_object_mut() {
                obj.insert("online".to_string(), json!(true));
            }
            Ok(fields)
        }
        SoapOutcome::Fault(_) | SoapOutcome::Unreachable(_) => Ok(server_info_down()),
        SoapOutcome::Auth => Err(()),
    }
}

/// Live `server-info` read: fire SOAP `server info` and assemble the
/// envelope. `Err(())` means SOAP auth failed (`SOAP_AUTH`, a hard error).
pub fn read_server_info(cfg: &SoapConfig) -> Result<Value, ()> {
    assemble_server_info(&soap::exec(cfg, "server info"))
}

// ---------------------------------------------------------------------------
// `server-detail` — containers, world-ready, SOAP section, verdict, ports,
// bots. Port of `90-main.sh:1415-1499` + its `40-config.sh` helpers.
// ---------------------------------------------------------------------------

/// One container row from the `server-detail` composite.
#[derive(Debug, Clone, PartialEq)]
pub struct ContainerRow {
    pub name: String,
    pub role: &'static str,
    pub state: String,
    pub status: String,
}

/// The `{version,players,uptime,mean_ms,median_ms}` fragment, all-null — the
/// `server-detail` arm's initial default AND the shape SOAP faulting-out
/// leaves in place, shared with [`soap_section_not_queried`].
pub fn stats_null_fragment() -> Value {
    json!({
        "version": null,
        "players": null,
        "uptime": null,
        "mean_ms": null,
        "median_ms": null,
    })
}

/// The `server-detail` composite's `soap` sub-object: `reachable`/`auth_ok`
/// plus the stats fragment (NOTE: no `online` key here — unlike `server-info`,
/// see the module header).
#[derive(Debug, Clone, PartialEq)]
pub struct SoapSection {
    pub reachable: bool,
    pub auth_ok: Option<bool>,
    pub stats: Value,
}

/// The `soap` section when the world container isn't running — SOAP is never
/// even called (`detail_reach=false; detail_auth=null` before the `if
/// world_state == running` gate, `90-main.sh:1436-1437`).
pub fn soap_section_not_queried() -> SoapSection {
    SoapSection { reachable: false, auth_ok: None, stats: stats_null_fragment() }
}

/// Classify a live `server info` SOAP call into the `soap` section — a port
/// of the rc dispatch at `90-main.sh:1440-1446`: `Ok` -> reachable+auth_ok
/// true, real stats; `Fault` (rc2) -> reachable+auth_ok true, stats stay
/// null (the arm never re-parses a fault body); `Auth` (rc3) -> reachable
/// true, auth_ok FALSE; `Unreachable` (rc4/other) -> not reachable, auth_ok
/// null.
pub fn soap_section_from_outcome(outcome: &SoapOutcome) -> SoapSection {
    match outcome {
        SoapOutcome::Ok(text) => {
            SoapSection { reachable: true, auth_ok: Some(true), stats: parse_server_info_fields(text) }
        }
        SoapOutcome::Fault(_) => SoapSection { reachable: true, auth_ok: Some(true), stats: stats_null_fragment() },
        SoapOutcome::Auth => SoapSection { reachable: true, auth_ok: Some(false), stats: stats_null_fragment() },
        SoapOutcome::Unreachable(_) => SoapSection { reachable: false, auth_ok: None, stats: stats_null_fragment() },
    }
}

/// Parse `docker ps -a --format '{{.Names}}|{{.State}}|{{.Status}}'` output
/// into the fixed-order `[world, auth, database]` row list — a port of
/// `_detail_container_rows` (`40-config.sh:667-682`). A name with no
/// matching line reports `state:"absent", status:""` (covers both "the
/// container was never created" and "the docker daemon is down", which
/// yields empty `ps_out` for every name).
pub fn parse_container_rows(ps_out: &str) -> Vec<ContainerRow> {
    let entries: Vec<(&str, &str, &str)> = ps_out
        .lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '|');
            let name = it.next()?;
            let state = it.next()?;
            let status = it.next().unwrap_or("");
            Some((name, state, status))
        })
        .collect();
    ["ac-worldserver", "ac-authserver", "ac-database"]
        .iter()
        .map(|&name| {
            let role: &'static str = match name {
                "ac-worldserver" => "world",
                "ac-authserver" => "auth",
                _ => "database",
            };
            match entries.iter().find(|(n, _, _)| *n == name) {
                Some((_, state, status)) => {
                    ContainerRow { name: name.to_string(), role, state: (*state).to_string(), status: (*status).to_string() }
                }
                None => ContainerRow { name: name.to_string(), role, state: "absent".to_string(), status: String::new() },
            }
        })
        .collect()
}

/// Run `docker ps -a --format ...` (bounded) and parse its rows. Docker
/// down/absent degrades to empty stdout -> every container reports
/// `"absent"`, matching the bash's `2>/dev/null || true` (no exit-status
/// gate at all — down is data).
pub fn docker_ps_rows(program: &OsStr, timeout: Duration) -> Vec<ContainerRow> {
    let mut cmd = Command::new(program);
    cmd.args(["ps", "-a", "--format", "{{.Names}}|{{.State}}|{{.Status}}"]);
    windows_no_window(&mut cmd);
    let ps_out = match output_bounded_draining(cmd, timeout) {
        Some(out) => String::from_utf8_lossy(&out.stdout).into_owned(),
        None => String::new(),
    };
    parse_container_rows(&ps_out)
}

/// The world container's own row state (`"running"`, `"exited"`,
/// `"restarting"`, `"absent"`, ...), or `"absent"` if somehow missing from
/// the (always fixed-order, always-3-row) list.
pub fn world_state_of(containers: &[ContainerRow]) -> &str {
    containers.iter().find(|c| c.name == "ac-worldserver").map(|c| c.state.as_str()).unwrap_or("absent")
}

/// `true` when a container OTHER than the world is `"running"` — the
/// crashed-vs-deliberate-stop signal for exit code 137 (`detail_others_up`,
/// `90-main.sh:1428`).
pub fn others_up(containers: &[ContainerRow]) -> bool {
    containers.iter().any(|c| c.name != "ac-worldserver" && c.state == "running")
}

/// `true` when the CURRENT worldserver run has logged AzerothCore's
/// boot-complete marker (case-insensitive `"World Initialized In"`) since
/// its `StartedAt` timestamp — a port of `_world_ready` (`40-config.sh:688-694`).
/// Pure line-scan half; see [`world_ready`] for the live docker calls.
pub fn world_ready_from_logs(logs: &str) -> bool {
    logs.lines().any(|l| l.to_lowercase().contains("world initialized in"))
}

/// Live readiness probe: `docker inspect -f '{{.State.StartedAt}}'` then
/// `docker logs --since <started>`, both bounded. `false` on ANY failure/
/// timeout/empty StartedAt — matches the bash's `return 1` fallback (a
/// clean "not ready" read is correct when there's no way to ask).
pub fn world_ready(program: &OsStr, timeout: Duration) -> bool {
    let mut inspect = Command::new(program);
    inspect.args(["inspect", "-f", "{{.State.StartedAt}}", "ac-worldserver"]);
    windows_no_window(&mut inspect);
    let Some(out) = output_bounded_draining(inspect, timeout) else { return false };
    if !out.status.success() {
        return false;
    }
    let started = String::from_utf8_lossy(&out.stdout);
    let started = started.trim();
    if started.is_empty() {
        return false;
    }

    let mut logs = Command::new(program);
    logs.args(["logs", "--since", started, "ac-worldserver"]);
    windows_no_window(&mut logs);
    // Unlike `inspect` above, the bash `_world_ready` does NOT gate on
    // `docker logs`'s own exit status (only the grep's `|| true`) — so a
    // failing `docker logs` still flows its (likely empty/error) output
    // through the same line-scan rather than short-circuiting to false.
    // Uses `output_bounded_draining`, not `crate::output_bounded` — a
    // long-running world's accumulated log output can exceed the OS pipe
    // buffer, and the non-draining helper deadlocks on that (see its
    // definition above for the live repro).
    let Some(logs_out) = output_bounded_draining(logs, timeout) else { return false };
    let mut combined = String::from_utf8_lossy(&logs_out.stdout).into_owned();
    combined.push_str(&String::from_utf8_lossy(&logs_out.stderr));
    world_ready_from_logs(&combined)
}

/// Parse a `docker inspect -f '{{.State.ExitCode}}'` result: first line,
/// trimmed, must be `^[0-9]+$` (bash's regex guard before the `10#`
/// base-10 cast) or the exit code is unknown (`None`).
pub fn parse_exit_code(raw: &str) -> Option<i64> {
    let first_line = raw.lines().next().unwrap_or("").trim();
    if is_digits(first_line) {
        first_line.parse::<i64>().ok()
    } else {
        None
    }
}

/// Live world exit-code probe (bounded); `None` on any failure/timeout/
/// non-numeric output.
pub fn world_exit_code(program: &OsStr, timeout: Duration) -> Option<i64> {
    let mut cmd = Command::new(program);
    cmd.args(["inspect", "-f", "{{.State.ExitCode}}", "ac-worldserver"]);
    windows_no_window(&mut cmd);
    let out = output_bounded_draining(cmd, timeout)?;
    if !out.status.success() {
        return None;
    }
    parse_exit_code(&String::from_utf8_lossy(&out.stdout))
}

/// Parse a `docker inspect -f '{{.State.Running}}'` result: the FIRST line
/// only, compared literally against `"true"` — a port of the world-restart
/// precondition's own bash idiom (`90-main.sh:1684-1688`:
/// `wr_wrun="${wr_wrun%%$'\n'*}"` then `[[ "$wr_wrun" != true ]]`). Anything
/// else (empty, "false", multi-line garbage, an error message) is `false` —
/// same "can't tell -> not running" fallback `container_running` below relies
/// on for a failed/timed-out `docker inspect`.
pub fn parse_running(raw: &str) -> bool {
    raw.lines().next().unwrap_or("") == "true"
}

/// Live "is this container running" probe (bounded) — a port of the
/// world-restart precondition's two `docker inspect -f '{{.State.Running}}'`
/// calls (`90-main.sh:1684-1685`). `pub` (was `pub(crate)` before the dml-wow
/// crate split, Task 7): consumed by `lib.rs`'s
/// `wow_world_restart_native` (Task: native world-restart). Any failure/
/// timeout degrades to `false` (matches the bash's `2>/dev/null || true` —
/// an unreadable/absent container is not a running one).
pub fn container_running(program: &OsStr, name: &str, timeout: Duration) -> bool {
    let mut cmd = Command::new(program);
    cmd.args(["inspect", "-f", "{{.State.Running}}", name]);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, timeout) {
        Some(out) => parse_running(&String::from_utf8_lossy(&out.stdout)),
        None => false,
    }
}

/// Tri-state sibling of [`container_running`]: `None` means *docker could not
/// answer* (engine hiccup, inspect timeout, garbage output), as opposed to
/// `Some(false)` = it answered and the container is down.
///
/// The "can't tell -> not running" collapse above is the right default for a
/// PRECONDITION (refusing to act on an unreadable stack is safe). It is the
/// wrong default for the world-restart liveness strike counter, where it turns
/// a few seconds of Docker unavailability into a fabricated "the world server
/// exited" abort of a perfectly healthy restart. Callers that count strikes
/// must use this and ignore `None`.
pub fn container_running_probe(program: &OsStr, name: &str, timeout: Duration) -> Option<bool> {
    let mut cmd = Command::new(program);
    cmd.args(["inspect", "-f", "{{.State.Running}}", name]);
    windows_no_window(&mut cmd);
    let out = output_bounded_draining(cmd, timeout)?;
    match String::from_utf8_lossy(&out.stdout).lines().next().unwrap_or("") {
        "true" => Some(true),
        "false" => Some(false),
        // Empty or an error message: docker did not give us a verdict.
        _ => None,
    }
}

/// The four-state (five-string) verdict machine — a faithful port of the
/// `detail_verdict` derivation (`90-main.sh:1454-1492`). `exit_code` must
/// already be the CALLER's decision of "is there a usable exit code to
/// classify" (`None` when the world is running, absent, or `docker inspect`
/// didn't answer with digits) — this function only classifies what it's
/// given, exactly like the bash arm's own gated assignment.
pub fn compute_verdict(world_state: &str, others_up: bool, exit_code: Option<i64>, soap_reachable: bool, world_ready: bool) -> &'static str {
    if world_state != "running" {
        let mut verdict = "stopped";
        if world_state != "absent" {
            if let Some(ec) = exit_code {
                match ec {
                    0 | 143 => {}
                    137 => {
                        if others_up {
                            verdict = "crashed";
                        }
                    }
                    _ => verdict = "crashed",
                }
            }
        }
        // Docker's own restart backoff overrides even a just-computed
        // "crashed" — a cold start legitimately loops here (90-main.sh
        // comment: "Docker self-heals -- normal").
        if world_state == "restarting" {
            verdict = "starting";
        }
        verdict
    } else if soap_reachable {
        "online"
    } else if world_ready {
        "soap_unreachable"
    } else {
        "starting"
    }
}

/// Parse the first line of a `docker port <container> <internal>` result
/// into the bound HOST port, or `None` — a port of `_host_port_json`
/// (`40-config.sh:845-851`): take the text after the LAST `:` on the first
/// line, keep it only if it's all digits.
pub fn parse_host_port(raw: &str) -> Option<String> {
    let first_line = raw.lines().next().unwrap_or("").trim();
    let after_colon = first_line.rsplit(':').next().unwrap_or("");
    if is_digits(after_colon) {
        Some(after_colon.to_string())
    } else {
        None
    }
}

/// Live host-port probe (bounded); exit status is IGNORED (matching the
/// bash's `2>/dev/null | head -n1 || true` — a failing `docker port` yields
/// empty stdout, which [`parse_host_port`] already turns into `None`).
pub fn host_port(program: &OsStr, container: &str, internal_port: &str, timeout: Duration) -> Option<String> {
    let mut cmd = Command::new(program);
    cmd.args(["port", container, internal_port]);
    windows_no_window(&mut cmd);
    let out = output_bounded_draining(cmd, timeout)?;
    parse_host_port(&String::from_utf8_lossy(&out.stdout))
}

/// The four host-port bindings the `server-detail` composite reports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Ports {
    pub world: Option<String>,
    pub auth: Option<String>,
    pub soap: Option<String>,
    pub db: Option<String>,
}

/// Live probe of all four ports — world(8085)/auth(3724)/soap(7878, on the
/// worldserver container)/db(3306).
pub fn read_ports(program: &OsStr, timeout: Duration) -> Ports {
    Ports {
        world: host_port(program, "ac-worldserver", "8085", timeout),
        auth: host_port(program, "ac-authserver", "3724", timeout),
        soap: host_port(program, "ac-worldserver", "7878", timeout),
        db: host_port(program, "ac-database", "3306", timeout),
    }
}

/// The `SELECT COUNT(*)` the `_bots_counts` "online" half runs — same
/// cross-schema playerbots-registry idiom as `players online`/`party
/// online` (`dml::pages`), inverted to INCLUDE bot accounts.
const BOTS_ONLINE_SQL: &str = "SELECT COUNT(*) FROM characters WHERE online = 1 \
    AND account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type \
    WHERE account_type IN (1,2));";

/// Live online-bot count. A query/connection failure degrades to `None`
/// (matches `_bots_counts`'s `|| true` swallow — a read-only lookup never
/// errors the whole `server-detail` envelope).
pub fn bots_online(cfg: &DbConfig) -> Result<Option<i64>, DbError> {
    let res = db::query(cfg, Database::Characters, BOTS_ONLINE_SQL)?;
    let raw = res.rows.first().and_then(|r| r.first()).map(super::pages::cell_text).unwrap_or_default();
    Ok(is_digits(&raw).then(|| raw.parse().ok()).flatten())
}

/// Live `AiPlayerbot.MaxRandomBots` lookup: override env first, else the
/// live `playerbots.conf` value (via [`ConfigReader::compute_value`], which
/// already resolves this exact key — see the task brief). `None` when
/// neither source has a numeric value, matching `_bots_counts`'s null
/// fallback. Passing `default: ""` means an absent value stays empty (never
/// silently substitutes a registry default the bash's own raw-grep fallback
/// has no notion of).
pub fn bots_max(reader: &mut ConfigReader) -> Option<i64> {
    let val = reader.compute_value("wow.status.bots_max", "conf:playerbots.conf:AiPlayerbot.MaxRandomBots", "");
    is_digits(&val).then(|| val.parse().ok()).flatten()
}

/// The `{"online":...,"max":...}` bots fragment (the `bots` value of the
/// `server-detail` envelope). Both computed ONLY when the world is
/// `"running"` — see [`bots_for_state`].
pub fn bots_fragment(online: Option<i64>, max: Option<i64>) -> Value {
    json!({ "online": online, "max": max })
}

/// Compute `(online, max)` for the `bots` fragment — a port of
/// `_bots_counts`'s own state gate (`40-config.sh:804-806`): both stay
/// `None` unless `world_state == "running"` (a stopped/booting world has no
/// live count and the max lookup isn't worth a docker exec either).
pub fn bots_for_state(world_state: &str, cfg: &DbConfig, reader: &mut ConfigReader) -> (Option<i64>, Option<i64>) {
    if world_state != "running" {
        return (None, None);
    }
    let online = bots_online(cfg).ok().flatten();
    let max = bots_max(reader);
    (online, max)
}

/// Assemble the full `server-detail` envelope from its already-computed
/// pieces — a pure port of the arm's final `json_ok` line
/// (`90-main.sh:1499`). Key set/order matches the CLI's (order is
/// irrelevant for JSON equality, but every key name is load-bearing for the
/// frontend's `ServerDetail` type).
pub fn assemble_server_detail(
    verdict: &str,
    exit_code: Option<i64>,
    containers: &[ContainerRow],
    world_ready: bool,
    soap: &SoapSection,
    ports: &Ports,
    bots: (Option<i64>, Option<i64>),
) -> Value {
    let containers_json: Vec<Value> = containers
        .iter()
        .map(|c| json!({ "name": c.name, "role": c.role, "state": c.state, "status": c.status }))
        .collect();

    let mut soap_json = json!({ "reachable": soap.reachable, "auth_ok": soap.auth_ok });
    if let (Some(obj), Some(stats_obj)) = (soap_json.as_object_mut(), soap.stats.as_object()) {
        for (k, v) in stats_obj {
            obj.insert(k.clone(), v.clone());
        }
    }

    json!({
        "verdict": verdict,
        "exit_code": exit_code,
        "containers": containers_json,
        "world_ready": world_ready,
        "soap": soap_json,
        "bots": bots_fragment(bots.0, bots.1),
        "ports": {
            "world": ports.world,
            "auth": ports.auth,
            "soap": ports.soap,
            "db": ports.db,
        },
    })
}

/// Live `server-detail` orchestration: containers -> world state -> (world
/// readiness, SOAP section, exit code — each gated exactly like the bash
/// arm) -> ports (always) -> bots (gated on world state) -> verdict ->
/// assemble. Never errors — down/booting/crashed are all answers.
pub fn read_server_detail(program: &OsStr, soap_cfg: &SoapConfig, db_cfg: &DbConfig, reader: &mut ConfigReader) -> Value {
    let containers = docker_ps_rows(program, DOCKER_PROBE_TIMEOUT);
    let world_state = world_state_of(&containers).to_string();
    let others = others_up(&containers);

    let ready = world_state == "running" && world_ready(program, DOCKER_PROBE_TIMEOUT);

    let soap = if world_state == "running" {
        soap_section_from_outcome(&soap::exec(soap_cfg, "server info"))
    } else {
        soap_section_not_queried()
    };

    let exit_code = if world_state != "running" && world_state != "absent" {
        world_exit_code(program, DOCKER_PROBE_TIMEOUT)
    } else {
        None
    };

    let verdict = compute_verdict(&world_state, others, exit_code, soap.reachable, ready);
    let ports = read_ports(program, DOCKER_PROBE_TIMEOUT);
    let bots = bots_for_state(&world_state, db_cfg, reader);

    assemble_server_detail(verdict, exit_code, &containers, ready, &soap, &ports, bots)
}

// ---------------------------------------------------------------------------
// `console-tail` — `90-main.sh:1629-1656` + `_strip_ansi`/`_console_lines_json`
// (`45-console.sh:10-38`).
// ---------------------------------------------------------------------------

/// Strip ANSI CSI escape sequences (`ESC [ <0-9;?>* <letter>`) — a
/// hand-rolled port of `_strip_ansi`'s sed pattern
/// (`s/\x1b\[[0-9;?]*[a-zA-Z]//g`; no `regex` crate in this workspace). An
/// `ESC [` with no terminating letter (a truncated/split sequence) is left
/// untouched, matching sed's own "no match, no substitution" behavior.
pub fn strip_ansi(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\u{1b}' && chars.get(i + 1) == Some(&'[') {
            let mut j = i + 2;
            while j < chars.len() && matches!(chars[j], '0'..='9' | ';' | '?') {
                j += 1;
            }
            if j < chars.len() && chars[j].is_ascii_alphabetic() {
                i = j + 1;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

/// Turn combined `docker logs` output into the sanitized line list — a port
/// of the pipeline `printf '%s\n' "$raw" | _strip_ansi | _console_lines_json`
/// (`90-main.sh:1644-1647`): trailing newlines stripped (matching command
/// substitution) then exactly one re-added, ANSI-stripped, every `\r`
/// dropped, split on `\n`. Empty (after trailing-newline-strip) input yields
/// no lines at all (the bash's `[[ -n "$raw" ]]` short-circuit to `arr="[]"`).
pub fn console_lines(raw_combined: &str) -> Vec<String> {
    let raw = raw_combined.trim_end_matches('\n');
    if raw.is_empty() {
        return Vec::new();
    }
    let text = format!("{raw}\n");
    let stripped = strip_ansi(&text);
    let no_cr: String = stripped.chars().filter(|&c| c != '\r').collect();
    no_cr.lines().map(str::to_string).collect()
}

/// Live `console-tail` read: `docker logs --tail <lines> ac-worldserver`
/// (bounded, pipe-draining — `--tail 1000` can comfortably exceed the OS
/// pipe buffer, see [`output_bounded_draining`]), combined stdout+stderr
/// (best-effort merge — see `launcher_lib::run_bounded`'s identical convention).
/// A non-zero exit or timeout reports `{"available":false,"lines":[]}` —
/// down is an answer, this verb never errors (`90-main.sh:1644-1656`).
pub fn read_console_tail(program: &OsStr, lines: u32) -> Value {
    let mut cmd = Command::new(program);
    cmd.args(["logs", "--tail", &lines.to_string(), "ac-worldserver"]);
    windows_no_window(&mut cmd);
    match output_bounded_draining(cmd, CONSOLE_TAIL_TIMEOUT) {
        Some(out) if out.status.success() => {
            let mut combined = String::from_utf8_lossy(&out.stdout).into_owned();
            combined.push_str(&String::from_utf8_lossy(&out.stderr));
            json!({ "available": true, "lines": console_lines(&combined) })
        }
        _ => json!({ "available": false, "lines": [] }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- parse_server_info_fields / server_info_down / assemble_server_info --

    const FIXTURE_INFO: &str = "\
AzerothCore rev. 44758c9e2b12+ 2026-07-01 00:00:00 +0000 (master branch) (Win64, RelWithDebInfo, Static)\r
Using SD3: YTDB Eluna\r
Connected players: 42. Characters in world: 50.\r
Connection peak: 55.\r
Server uptime: 2 days 3 hours 12 minutes 5 seconds.\r
Update time diff: 50ms, average: 45ms.\r
Update time diff:\r
        |- Min: 10ms\r
        |- Max: 200ms\r
        |- Mean: 47ms\r
        |- Median: 40ms\r
        |- Percentile95: 90ms\r
        |- Percentile99: 150ms\r
";

    #[test]
    fn parse_server_info_fields_extracts_every_field() {
        let raw = FIXTURE_INFO.replace('\r', "&#xD;");
        let got = parse_server_info_fields(&raw);
        assert_eq!(
            got,
            json!({
                "version": "44758c9e2b12+ 2026-07-01 00:00:00 +0000 (master branch) (Win64, RelWithDebInfo, Static)",
                "players": 42,
                "uptime": "2 days 3 hours 12 minutes 5 seconds.",
                "mean_ms": 47,
                "median_ms": 40,
            })
        );
    }

    #[test]
    fn parse_server_info_fields_garbage_is_all_null() {
        let got = parse_server_info_fields("nothing recognizable here\nnope\n");
        assert_eq!(
            got,
            json!({ "version": null, "players": null, "uptime": null, "mean_ms": null, "median_ms": null })
        );
    }

    #[test]
    fn parse_server_info_fields_empty_is_all_null() {
        assert_eq!(parse_server_info_fields(""), stats_null_fragment());
    }

    #[test]
    fn server_info_down_shape() {
        assert_eq!(
            server_info_down(),
            json!({"online":false,"version":null,"players":null,"uptime":null,"mean_ms":null,"median_ms":null})
        );
    }

    #[test]
    fn assemble_server_info_ok_sets_online_true() {
        let got = assemble_server_info(&SoapOutcome::Ok("Connected players: 5. Total: 5.".into())).unwrap();
        assert_eq!(got["online"], json!(true));
        assert_eq!(got["players"], json!(5));
    }

    #[test]
    fn assemble_server_info_fault_and_unreachable_are_down() {
        assert_eq!(assemble_server_info(&SoapOutcome::Fault("boom".into())).unwrap(), server_info_down());
        assert_eq!(
            assemble_server_info(&SoapOutcome::Unreachable("timeout".into())).unwrap(),
            server_info_down()
        );
    }

    #[test]
    fn assemble_server_info_auth_is_a_hard_error() {
        assert_eq!(assemble_server_info(&SoapOutcome::Auth), Err(()));
    }

    // -- soap_section_from_outcome / soap_section_not_queried ----------------

    #[test]
    fn soap_section_not_queried_is_unreachable_shaped() {
        let s = soap_section_not_queried();
        assert!(!s.reachable);
        assert_eq!(s.auth_ok, None);
        assert_eq!(s.stats, stats_null_fragment());
    }

    #[test]
    fn soap_section_from_outcome_all_four_branches() {
        let ok = soap_section_from_outcome(&SoapOutcome::Ok("Connected players: 1. X.".into()));
        assert!(ok.reachable);
        assert_eq!(ok.auth_ok, Some(true));
        assert_eq!(ok.stats["players"], json!(1));

        let fault = soap_section_from_outcome(&SoapOutcome::Fault("x".into()));
        assert!(fault.reachable);
        assert_eq!(fault.auth_ok, Some(true));
        assert_eq!(fault.stats, stats_null_fragment());

        let auth = soap_section_from_outcome(&SoapOutcome::Auth);
        assert!(auth.reachable);
        assert_eq!(auth.auth_ok, Some(false));
        assert_eq!(auth.stats, stats_null_fragment());

        let unreachable = soap_section_from_outcome(&SoapOutcome::Unreachable("boom".into()));
        assert!(!unreachable.reachable);
        assert_eq!(unreachable.auth_ok, None);
        assert_eq!(unreachable.stats, stats_null_fragment());
    }

    // -- parse_container_rows / world_state_of / others_up -------------------

    #[test]
    fn parse_container_rows_fixed_order_and_absent_fill() {
        let ps_out = "ac-authserver|running|Up 2 hours\nac-database|running|Up 3 hours (healthy)\n";
        let rows = parse_container_rows(ps_out);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0], ContainerRow { name: "ac-worldserver".into(), role: "world", state: "absent".into(), status: "".into() });
        assert_eq!(rows[1], ContainerRow { name: "ac-authserver".into(), role: "auth", state: "running".into(), status: "Up 2 hours".into() });
        assert_eq!(rows[2].role, "database");
        assert_eq!(rows[2].state, "running");
    }

    #[test]
    fn parse_container_rows_all_absent_on_empty_ps_out() {
        let rows = parse_container_rows("");
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.state == "absent" && r.status.is_empty()));
    }

    #[test]
    fn world_state_of_and_others_up() {
        let running_world = vec![
            ContainerRow { name: "ac-worldserver".into(), role: "world", state: "running".into(), status: "Up".into() },
            ContainerRow { name: "ac-authserver".into(), role: "auth", state: "exited".into(), status: "".into() },
            ContainerRow { name: "ac-database".into(), role: "database", state: "running".into(), status: "".into() },
        ];
        assert_eq!(world_state_of(&running_world), "running");
        assert!(others_up(&running_world)); // db is running

        let world_down_others_up = vec![
            ContainerRow { name: "ac-worldserver".into(), role: "world", state: "exited".into(), status: "".into() },
            ContainerRow { name: "ac-authserver".into(), role: "auth", state: "running".into(), status: "".into() },
            ContainerRow { name: "ac-database".into(), role: "database", state: "running".into(), status: "".into() },
        ];
        assert_eq!(world_state_of(&world_down_others_up), "exited");
        assert!(others_up(&world_down_others_up));

        let all_absent = parse_container_rows("");
        assert_eq!(world_state_of(&all_absent), "absent");
        assert!(!others_up(&all_absent));
    }

    // -- world_ready_from_logs -------------------------------------------------

    #[test]
    fn world_ready_from_logs_case_insensitive_match() {
        assert!(world_ready_from_logs("boot line\nWORLD INITIALIZED IN 12s\nother\n"));
        assert!(world_ready_from_logs("...world initialized in 3s..."));
        assert!(!world_ready_from_logs("still booting\nloading maps\n"));
        assert!(!world_ready_from_logs(""));
    }

    // -- parse_exit_code ---------------------------------------------------

    #[test]
    fn parse_exit_code_digits_only_first_line() {
        assert_eq!(parse_exit_code("0\n"), Some(0));
        assert_eq!(parse_exit_code("137\n"), Some(137));
        // leading zeros: base-10, not octal (matches bash's `10#` cast).
        assert_eq!(parse_exit_code("007\n"), Some(7));
        assert_eq!(parse_exit_code(""), None);
        assert_eq!(parse_exit_code("not-a-number\n"), None);
        assert_eq!(parse_exit_code("-1\n"), None);
    }

    // -- parse_running (world-restart precondition) --------------------------

    #[test]
    fn parse_running_true_first_line_only() {
        assert!(parse_running("true\n"));
        assert!(parse_running("true"));
        // Only the FIRST line matters (matches the bash `${x%%$'\n'*}` trim).
        assert!(parse_running("true\nfalse\n"));
    }

    #[test]
    fn parse_running_false_or_garbage_or_empty() {
        assert!(!parse_running("false\n"));
        assert!(!parse_running(""));
        assert!(!parse_running("Error: No such container: ac-worldserver\n"));
        // No trim -- trailing whitespace on the line makes it not literally "true".
        assert!(!parse_running("true \n"));
    }

    // -- compute_verdict -----------------------------------------------------

    #[test]
    fn compute_verdict_running_branches() {
        assert_eq!(compute_verdict("running", false, None, true, false), "online");
        assert_eq!(compute_verdict("running", false, None, false, true), "soap_unreachable");
        assert_eq!(compute_verdict("running", false, None, false, false), "starting");
    }

    #[test]
    fn compute_verdict_not_running_exit_code_classification() {
        assert_eq!(compute_verdict("exited", false, Some(0), false, false), "stopped");
        assert_eq!(compute_verdict("exited", false, Some(143), false, false), "stopped");
        assert_eq!(compute_verdict("exited", false, Some(1), false, false), "crashed");
        // 137 (SIGKILL): crashed only when something ELSE is still up.
        assert_eq!(compute_verdict("exited", true, Some(137), false, false), "crashed");
        assert_eq!(compute_verdict("exited", false, Some(137), false, false), "stopped");
    }

    #[test]
    fn compute_verdict_absent_ignores_exit_code() {
        assert_eq!(compute_verdict("absent", false, None, false, false), "stopped");
        // Even if a caller mistakenly passed an exit code for "absent", the
        // gate never classifies it (matches the bash's own `!= absent` guard).
        assert_eq!(compute_verdict("absent", true, Some(1), false, false), "stopped");
    }

    #[test]
    fn compute_verdict_restarting_overrides_crashed() {
        assert_eq!(compute_verdict("restarting", true, Some(137), false, false), "starting");
        assert_eq!(compute_verdict("restarting", false, Some(1), false, false), "starting");
        assert_eq!(compute_verdict("restarting", false, None, false, false), "starting");
    }

    // -- parse_host_port -----------------------------------------------------

    #[test]
    fn parse_host_port_extracts_after_last_colon() {
        assert_eq!(parse_host_port("0.0.0.0:8085\n"), Some("8085".to_string()));
        assert_eq!(parse_host_port("[::]:3724\n"), Some("3724".to_string()));
        assert_eq!(parse_host_port(""), None);
        assert_eq!(parse_host_port("garbage\n"), None);
        // only the FIRST line matters.
        assert_eq!(parse_host_port("0.0.0.0:8085\n0.0.0.0:9999\n"), Some("8085".to_string()));
    }

    // -- bots_fragment / bots_for_state (state gate only; DB calls are live) --

    #[test]
    fn bots_fragment_shapes_values_and_nulls() {
        assert_eq!(bots_fragment(Some(3), Some(2000)), json!({"online":3,"max":2000}));
        assert_eq!(bots_fragment(None, None), json!({"online":null,"max":null}));
    }

    // -- strip_ansi / console_lines --------------------------------------------

    #[test]
    fn strip_ansi_removes_csi_sequences() {
        assert_eq!(strip_ansi("\u{1b}[31mRed\u{1b}[0m text"), "Red text");
        assert_eq!(strip_ansi("\u{1b}[1;32mBold Green\u{1b}[39;49m"), "Bold Green");
        assert_eq!(strip_ansi("no escapes here"), "no escapes here");
    }

    #[test]
    fn strip_ansi_leaves_unterminated_sequence_alone() {
        // ESC[ with no terminating letter never matches sed's own pattern.
        let s = "\u{1b}[123";
        assert_eq!(strip_ansi(s), s);
    }

    #[test]
    fn console_lines_strips_ansi_and_cr_and_splits() {
        let raw = "\u{1b}[32mWorld Initialized In 12s\u{1b}[0m\r\nSecond line\r\n";
        let got = console_lines(raw);
        assert_eq!(got, vec!["World Initialized In 12s".to_string(), "Second line".to_string()]);
    }

    #[test]
    fn console_lines_empty_raw_is_empty_vec() {
        assert_eq!(console_lines(""), Vec::<String>::new());
        assert_eq!(console_lines("\n\n\n"), Vec::<String>::new());
    }

    #[test]
    fn console_lines_no_trailing_newline_still_captures_last_line() {
        let got = console_lines("only one line, no trailing newline");
        assert_eq!(got, vec!["only one line, no trailing newline".to_string()]);
    }

    // -- assemble_server_detail (full envelope shape) -------------------------

    #[test]
    fn assemble_server_detail_full_shape() {
        let containers = parse_container_rows("ac-worldserver|running|Up 1 hour\nac-authserver|running|Up 1 hour\nac-database|running|Up 1 hour (healthy)\n");
        let soap = SoapSection { reachable: true, auth_ok: Some(true), stats: parse_server_info_fields("Connected players: 5. X.") };
        let ports = Ports { world: Some("8085".into()), auth: Some("3724".into()), soap: Some("7878".into()), db: Some("3306".into()) };
        let got = assemble_server_detail("online", None, &containers, true, &soap, &ports, (Some(5), Some(2000)));
        assert_eq!(got["verdict"], json!("online"));
        assert_eq!(got["exit_code"], json!(null));
        assert_eq!(got["world_ready"], json!(true));
        assert_eq!(got["containers"].as_array().unwrap().len(), 3);
        assert_eq!(got["containers"][0]["role"], json!("world"));
        assert_eq!(got["soap"]["reachable"], json!(true));
        assert_eq!(got["soap"]["auth_ok"], json!(true));
        assert_eq!(got["soap"]["players"], json!(5));
        assert_eq!(got["bots"], json!({"online":5,"max":2000}));
        assert_eq!(got["ports"], json!({"world":"8085","auth":"3724","soap":"7878","db":"3306"}));
    }

    #[test]
    fn assemble_server_detail_stopped_shape_has_null_bots_and_ports() {
        let containers = parse_container_rows("");
        let soap = soap_section_not_queried();
        let ports = Ports::default();
        let got = assemble_server_detail("stopped", None, &containers, false, &soap, &ports, (None, None));
        assert_eq!(got["verdict"], json!("stopped"));
        assert_eq!(got["bots"], json!({"online":null,"max":null}));
        assert_eq!(got["ports"], json!({"world":null,"auth":null,"soap":null,"db":null}));
        assert_eq!(got["soap"], json!({"reachable":false,"auth_ok":null,"version":null,"players":null,"uptime":null,"mean_ms":null,"median_ms":null}));
    }
}
