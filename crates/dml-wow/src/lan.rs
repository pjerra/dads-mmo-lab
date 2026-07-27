//! Native-mode `dml lan on/off/status/refresh` pure helpers (Chunk 2 task
//! C2c item 3 -- see `.superpowers/sdd/chunk2-decisions.md`). A faithful
//! port of the regex classifiers and the exact message text from the
//! AzerothCore branch of `90-main.sh:858-1052`.
//!
//! LAYOUT. Everything above [`LAN_TITLE`] is PURE and testable without a
//! live server: the private-address classifier (security-relevant -- it is
//! the only thing standing between `dml lan on <ip>` and silently exposing
//! the realm to a public address) and the verbatim success/error text the
//! command returns, so the `<pre>`-rendered output stays byte-identical to
//! the WSL sibling's `dml lan` stdout (see `wowLan`'s doc comment in
//! `api.ts`). Below it sits the docker/DB orchestration ([`lan_action`] plus
//! [`lan_current_address`]/[`lan_set`]): container checks, the "wait for the
//! realm DB to answer" retry loop, the UPDATE + read-back write. That half
//! moved out of the launcher's `lib.rs` in the cargo-workspace refactor
//! (Task 9) so the standalone CLI can drive it too; input validation (the
//! closed action allowlist, the IP/hostname shape checks) stays with the
//! caller, which passes only pre-validated values in.
//!
//! AC-ONLY BY DECISION (chunk2-decisions.md item 3): `dml::db` has no
//! MaNGOS/Tortoise support, so native mode never reaches the oracle's
//! `tw_logon` branch (`90-main.sh:942-960`) -- WSL keeps handling every
//! other title family; native only ever drives the single fixed AC title.

use dml_core::error::{bad_arg, CmdError};

/// `true` iff `addr` starts with the private-LAN prefix `192.168.` or `10.`
/// or `172.(16-31).` -- a character-exact port of the bash regex
/// `^(192\.168\.|10\.|172\.(1[6-9]|2[0-9]|3[01])\.)` (`90-main.sh:901,1041`).
/// Deliberately matches the regex's CHARACTER classes rather than
/// numerically parsing the second octet, so a leading-zero shape like
/// `172.016.x.x` (which `1[6-9]|2[0-9]|3[01]` would NOT match, since it
/// requires the octet's first character to be `1`, `2` or `3`) is rejected
/// exactly like the bash does, not silently accepted by a `.parse::<u16>()`
/// shortcut that would read `"016"` as the number 16.
pub fn is_private_lan(addr: &str) -> bool {
    if addr.starts_with("192.168.") || addr.starts_with("10.") {
        return true;
    }
    match addr.strip_prefix("172.") {
        Some(rest) => matches_172_second_octet(rest),
        None => false,
    }
}

/// `rest` is what follows `"172."`; `true` iff it starts with a
/// `1[6-9]|2[0-9]|3[01]` octet immediately followed by a literal `.`.
fn matches_172_second_octet(rest: &str) -> bool {
    let b = rest.as_bytes();
    if b.len() < 3 || b[2] != b'.' {
        return false;
    }
    match b[0] {
        b'1' => (b'6'..=b'9').contains(&b[1]),
        b'2' => (b'0'..=b'9').contains(&b[1]),
        b'3' => b[1] == b'0' || b[1] == b'1',
        _ => false,
    }
}

/// `true` iff `addr` is loopback OR private -- `^(127\.|192\.168\.|10\.|
/// 172\.(1[6-9]|2[0-9]|3[01])\.)` (`90-main.sh:901`), the gate `on`/`refresh`
/// apply to a non-`--internet` address before ever touching docker/DB.
pub fn is_loopback_or_private(addr: &str) -> bool {
    addr.starts_with("127.") || is_private_lan(addr)
}

/// Retry budget for the "wait for the realm DB to answer" loop
/// (`90-main.sh:965`): `refresh` (fired automatically right after `dml
/// start`, while the DB may still be importing) gets a long budget; every
/// interactive action gets a short one. Returns `(tries, gap_secs)`.
pub fn tries_and_gap(action: &str) -> (u32, u64) {
    if action == "refresh" {
        (60, 10)
    } else {
        (18, 5)
    }
}

// ---------------------------------------------------------------------------
// Message text -- verbatim ports of the arm's `echo`/error lines. Every
// function here returns exactly what `wow_lan_native` hands back as its
// `Ok(String)`, matching `run_captured`'s "combined stdout+stderr, exit code
// irrelevant" contract the WSL sibling already relies on (see
// `DmlRunner::run_captured`'s doc comment) -- domain-level failures are
// TEXT, not a `CmdError`.
// ---------------------------------------------------------------------------

pub fn not_installed_message() -> String {
    "[dml] ERROR: WoW Playerbots server not installed. Install it first.".to_string()
}

pub fn docker_down_message() -> String {
    "[dml] ERROR: Docker is not running. Start Docker in the distro first.".to_string()
}

pub fn not_running_message(title: &str) -> String {
    format!("[dml] ERROR: '{title}' is not running. Start the server first, then change LAN settings.")
}

pub fn db_not_answering_message() -> String {
    "[dml] ERROR: The realm database is not answering yet. Wait for the server to finish starting, then try again.".to_string()
}

pub fn not_private_message(title: &str, ip: &str) -> String {
    format!(
        "[dml] ERROR: '{ip}' is not a private LAN address.\n[dml]   For internet play use the Tools page stepper (or: dml lan {title} --internet on {ip})."
    )
}

pub fn on_message(title: &str, ip: &str) -> String {
    format!(
        "[ok] LAN play ENABLED for {title}.\n\nOther PCs on your network: set realmlist {ip}\n(in realmlist.wtf inside the WoW client folder)\n\nThis PC keeps working with 127.0.0.1 or {ip} -- both reach the server."
    )
}

pub fn off_message(title: &str) -> String {
    format!("[ok] LAN play DISABLED for {title}.\nThe server only accepts world connections from this PC again.")
}

pub fn status_no_current_message() -> String {
    "[dml] ERROR: Could not read the realm address from the database.".to_string()
}

pub fn status_off_message() -> String {
    "LAN play: OFF (realm address 127.0.0.1 -- this PC only)".to_string()
}

pub fn status_on_message(current: &str) -> String {
    format!("LAN play: ON  (realm address {current})\nOther PCs use: set realmlist {current}")
}

pub fn refresh_off_message(title: &str) -> String {
    format!("[dml] LAN play is off for {title} -- nothing to refresh.")
}

pub fn refresh_already_message(ip: &str) -> String {
    format!("[ok] LAN address already current ({ip}).")
}

pub fn refresh_not_lan_message(current: &str) -> String {
    format!("[dml] Realm address {current} is not a LAN address -- leaving it alone.")
}

pub fn refresh_done_message(old: &str, new: &str) -> String {
    format!("[ok] LAN address refreshed: {old} -> {new}")
}

pub fn update_failed_message() -> String {
    "[dml] ERROR: Could not update the realm address.".to_string()
}

pub fn address_mismatch_message(wanted: &str, got: Option<&str>) -> String {
    format!(
        "[dml] ERROR: The realm address did not change (no realm with id 1?).\n[dml]   Wanted '{wanted}' but the database says '{}'.",
        got.unwrap_or("nothing")
    )
}

// ---------------------------------------------------------------------------
// Live orchestration — moved out of the launcher's `lib.rs` (Task 9).
// Unlike every other hoisted command this one returns plain TEXT, not an
// NDJSON stream or a typed envelope: `dml lan`'s output is human-readable
// stdout the UI renders verbatim inside a `<pre>` (see the module doc above).
// `action`/`ip_arg`/`inet` arrive PRE-VALIDATED by the caller.
// ---------------------------------------------------------------------------

/// The single fixed AC title native mode drives (see the AC-ONLY note above).
pub const LAN_TITLE: &str = "wow-server-playerbots";

/// `_lan_sql "SELECT address FROM realmlist WHERE id=1;"`, decoded to a
/// plain `String` (NULL/an empty/unreadable result -> `None`).
pub fn lan_current_address(db_cfg: &crate::db::DbConfig) -> Option<String> {
    let res = crate::db::query(db_cfg, crate::db::Database::Auth, "SELECT address FROM realmlist WHERE id = 1").ok()?;
    let row = res.rows.first()?;
    match row.first()? {
        crate::db::SqlValue::Text(s) => Some(s.clone()),
        crate::db::SqlValue::Int(i) => Some(i.to_string()),
        crate::db::SqlValue::Null => None,
    }
}

/// `_lan_set` (`90-main.sh:976-997`): UPDATE + read-back verify. `Err`
/// carries the already-formatted `[dml] ERROR: ...` text to return verbatim
/// -- this is a TEXT-mode command (see the module doc comment above).
pub fn lan_set(db_cfg: &crate::db::DbConfig, ip: &str) -> Result<(), String> {
    let params: Vec<mysql::Value> = vec![mysql::Value::from(ip)];
    if crate::db::execute(db_cfg, crate::db::Database::Auth, "UPDATE realmlist SET address = ? WHERE id = 1", params).is_err() {
        return Err(crate::lan::update_failed_message());
    }
    let newaddr = lan_current_address(db_cfg);
    if newaddr.as_deref() != Some(ip) {
        return Err(crate::lan::address_mismatch_message(ip, newaddr.as_deref()));
    }
    Ok(())
}

/// `dml lan on/off/status/refresh`'s full flow (real docker/DB I/O) -- run
/// off the caller's async runtime. Named `lan_action` rather than the bare
/// `lan` the Task 9 naming rule literally produces, purely to avoid a
/// `lan::lan` stutter. Order mirrors the oracle top-to-bottom: private-address
/// gate -> installed? -> docker up? -> `ac-database` running? -> DB
/// answering (retry loop)? -> the requested action.
pub fn lan_action(action: &str, ip_arg: Option<String>, inet: bool) -> String {
    use crate::{config::ConfigReader, db, lan, maint, native, status};

    if !inet {
        if let Some(ip) = ip_arg.as_deref() {
            if (action == "on" || action == "refresh") && !lan::is_loopback_or_private(ip) {
                return lan::not_private_message(LAN_TITLE, ip);
            }
        }
    }

    let title_dir = ConfigReader::title_dir_from_env();
    if maint::resolve_server_dir(&title_dir).is_none() {
        return lan::not_installed_message();
    }

    let program = native::docker_program();
    if !maint::docker_engine_up(&program, maint::PROBE_TIMEOUT) {
        return lan::docker_down_message();
    }
    if !status::container_running(&program, "ac-database", maint::PROBE_TIMEOUT) {
        return lan::not_running_message(LAN_TITLE);
    }

    let db_cfg = db::DbConfig::from_env();
    let (tries, gap) = lan::tries_and_gap(action);
    let mut reachable = false;
    for i in 0..tries {
        if db::query(&db_cfg, db::Database::Auth, "SELECT 1").is_ok() {
            reachable = true;
            break;
        }
        if i + 1 < tries {
            std::thread::sleep(std::time::Duration::from_secs(gap));
        }
    }
    if !reachable {
        return lan::db_not_answering_message();
    }

    let current = lan_current_address(&db_cfg);

    match action {
        "on" => {
            let ip = ip_arg.expect("validated: ip required for on");
            match lan_set(&db_cfg, &ip) {
                Ok(()) => lan::on_message(LAN_TITLE, &ip),
                Err(e) => e,
            }
        }
        "off" => match lan_set(&db_cfg, "127.0.0.1") {
            Ok(()) => lan::off_message(LAN_TITLE),
            Err(e) => e,
        },
        "status" => match current.as_deref() {
            None | Some("") => lan::status_no_current_message(),
            Some("127.0.0.1") => lan::status_off_message(),
            Some(cur) => lan::status_on_message(cur),
        },
        "refresh" => {
            let ip = ip_arg.expect("validated: ip required for refresh");
            match current.as_deref() {
                None | Some("") | Some("127.0.0.1") => lan::refresh_off_message(LAN_TITLE),
                Some(cur) if cur == ip => lan::refresh_already_message(&ip),
                Some(cur) if !lan::is_private_lan(cur) => lan::refresh_not_lan_message(cur),
                Some(cur) => {
                    let cur = cur.to_string();
                    match lan_set(&db_cfg, &ip) {
                        Ok(()) => lan::refresh_done_message(&cur, &ip),
                        Err(e) => e,
                    }
                }
            }
        }
        _ => unreachable!("action pre-validated by validate_lan_request_native"),
    }
}

pub const LAN_ACTIONS: [&str; 4] = ["on", "off", "status", "refresh"];

/// Pure, testable IPv4-shape check: `^[0-9]{1,3}(\.[0-9]{1,3}){3}$`. Exactly
/// 4 dot-separated groups of 1-3 ASCII digits each -- matches the CLI's own
/// guard in `dml lan` (it re-validates independently) rather than a strict
/// 0-255 range check, so this only needs to reject shapes that could carry
/// something other than an address (whitespace, semicolons, letters, extra
/// segments) before the value is ever used to build a command line.
pub fn validate_ip(ip: &str) -> bool {
    let parts: Vec<&str> = ip.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.len() <= 3 && p.chars().all(|c| c.is_ascii_digit()))
}

/// Internet-play address check (Batch 4 F15): a public IPv4 or hostname,
/// `^[A-Za-z0-9.-]{1,253}$` -- mirrors the CLI's own `--internet` guard
/// (which re-validates independently). Like validate_ip this only needs to
/// keep shell/SQL-shaped garbage out of an argv slot; DNS decides whether
/// the name actually resolves.
pub fn validate_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// THE INPUT GATE for [`lan_action`]: run this first and pass its
/// `(inet, ip_arg)` straight in — every caller (the launcher's Tauri command
/// and the CLI alike) gets the same closed rules, so `lan_action` itself can
/// keep asserting they already hold. Deliberately duplicating (not
/// refactoring) the launcher's WSL `wow_lan` inline checks: SAME rules --
/// [`LAN_ACTIONS`] membership, an IPv4-SHAPE ([`validate_ip`]) or
/// hostname-SHAPE ([`validate_host`]) check depending on `--internet`,
/// `--internet` itself narrowed to `action == "on"` only (mirrors `wow_lan`'s
/// existing `internet.unwrap_or(false) && action == "on"` -- a deliberate
/// product narrowing already shipped for WSL, not a gap to "fix" here). The
/// private-vs-public "not a private LAN address" check is NOT here -- that
/// stays a DOMAIN-level TEXT error inside `lan_action`, matching where the
/// bash oracle itself performs it (`90-main.sh:901-905`, before ever touching
/// docker/DB).
///
/// TEXT-MODE vs TYPED: `CmdError` here is reserved for genuinely malformed
/// input (bad action, bad address SHAPE). Every DOMAIN-level failure (not a
/// private address, server not running, DB not answering yet, the address
/// didn't land) instead comes back from `lan_action` as
/// `"[dml] ERROR: ..."` TEXT -- see the module doc comment.
///
/// Moved out of the launcher's `lib.rs` by the cargo-workspace refactor
/// (Task 9b).
pub fn validate_lan_request(
    action: &str,
    ip: Option<String>,
    internet: bool,
) -> Result<(bool, Option<String>), CmdError> {
    if !LAN_ACTIONS.contains(&action) {
        return Err(bad_arg(format!("invalid lan action: {action:?}")));
    }
    let inet = internet && action == "on";
    let ip_arg = if action == "on" || action == "refresh" {
        let ip = ip.ok_or_else(|| bad_arg("ip is required for the on/refresh actions"))?;
        if inet {
            if !validate_host(&ip) {
                return Err(bad_arg(format!("invalid address or hostname: {ip:?}")));
            }
        } else if !validate_ip(&ip) {
            return Err(bad_arg(format!("invalid IPv4 address: {ip:?}")));
        }
        Some(ip)
    } else {
        None
    };
    Ok((inet, ip_arg))
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- is_private_lan / is_loopback_or_private -----------------------------

    #[test]
    fn private_lan_matches_192_168_and_10() {
        assert!(is_private_lan("192.168.1.5"));
        assert!(is_private_lan("10.0.0.1"));
        assert!(!is_private_lan("192.169.1.5"));
        assert!(!is_private_lan("100.0.0.1")); // NOT "10." prefix
    }

    #[test]
    fn private_lan_matches_172_16_through_31_only() {
        assert!(is_private_lan("172.16.0.1"));
        assert!(is_private_lan("172.19.0.1"));
        assert!(is_private_lan("172.20.0.1"));
        assert!(is_private_lan("172.29.0.1"));
        assert!(is_private_lan("172.30.0.1"));
        assert!(is_private_lan("172.31.0.1"));
        assert!(!is_private_lan("172.15.0.1"));
        assert!(!is_private_lan("172.32.0.1"));
        assert!(!is_private_lan("172.5.0.1"));
    }

    #[test]
    fn private_lan_rejects_leading_zero_shape_like_the_bash_regex() {
        // "1[6-9]|2[0-9]|3[01]" requires the octet's first CHARACTER to be
        // 1/2/3 -- "016" starts with '0', so the bash regex would NOT match
        // it even though it numerically equals 16.
        assert!(!is_private_lan("172.016.0.1"));
    }

    #[test]
    fn private_lan_rejects_public_and_malformed() {
        assert!(!is_private_lan("8.8.8.8"));
        assert!(!is_private_lan("172."));
        assert!(!is_private_lan("172.1.2.3"));
        assert!(!is_private_lan(""));
    }

    #[test]
    fn loopback_or_private_includes_127() {
        assert!(is_loopback_or_private("127.0.0.1"));
        assert!(is_loopback_or_private("192.168.1.1"));
        assert!(!is_loopback_or_private("8.8.8.8"));
        assert!(!is_loopback_or_private("128.0.0.1"));
    }

    // -- tries_and_gap ---------------------------------------------------

    #[test]
    fn tries_and_gap_refresh_gets_the_long_budget() {
        assert_eq!(tries_and_gap("refresh"), (60, 10));
        assert_eq!(tries_and_gap("on"), (18, 5));
        assert_eq!(tries_and_gap("off"), (18, 5));
        assert_eq!(tries_and_gap("status"), (18, 5));
    }

    // -- message text --------------------------------------------------------

    #[test]
    fn on_message_carries_title_and_ip() {
        let m = on_message("wow-server-playerbots", "192.168.1.50");
        assert!(m.contains("[ok] LAN play ENABLED for wow-server-playerbots."));
        assert!(m.contains("set realmlist 192.168.1.50"));
        assert!(m.contains("127.0.0.1 or 192.168.1.50"));
    }

    #[test]
    fn off_message_shape() {
        let m = off_message("wow-server-playerbots");
        assert!(m.starts_with("[ok] LAN play DISABLED for wow-server-playerbots."));
    }

    #[test]
    fn status_messages() {
        assert_eq!(status_off_message(), "LAN play: OFF (realm address 127.0.0.1 -- this PC only)");
        let m = status_on_message("192.168.1.50");
        assert!(m.contains("LAN play: ON  (realm address 192.168.1.50)"));
        assert!(m.contains("Other PCs use: set realmlist 192.168.1.50"));
    }

    #[test]
    fn refresh_messages() {
        assert!(refresh_off_message("t").contains("LAN play is off for t"));
        assert_eq!(refresh_already_message("1.2.3.4"), "[ok] LAN address already current (1.2.3.4).");
        assert!(refresh_not_lan_message("8.8.8.8").contains("8.8.8.8 is not a LAN address"));
        assert_eq!(
            refresh_done_message("192.168.1.5", "192.168.1.6"),
            "[ok] LAN address refreshed: 192.168.1.5 -> 192.168.1.6"
        );
    }

    #[test]
    fn address_mismatch_message_reports_nothing_when_absent() {
        let m = address_mismatch_message("1.2.3.4", None);
        assert!(m.contains("Wanted '1.2.3.4'"));
        assert!(m.contains("says 'nothing'"));
        let m2 = address_mismatch_message("1.2.3.4", Some("9.9.9.9"));
        assert!(m2.contains("says '9.9.9.9'"));
    }

    #[test]
    fn not_private_message_mentions_internet_flag() {
        let m = not_private_message("wow-server-playerbots", "8.8.8.8");
        assert!(m.contains("'8.8.8.8' is not a private LAN address"));
        assert!(m.contains("--internet on 8.8.8.8"));
    }

    #[test]
    fn lan_current_address_none_on_unreachable_db() {
        // A bogus port guarantees an unreachable connection without touching
        // the real DB -- exercises the `.ok()?` degrade-to-None path.
        let cfg = crate::db::DbConfig { host: "127.0.0.1".into(), port: 1, user: "root".into(), password: "x".into() };
        assert_eq!(lan_current_address(&cfg), None);
    }

    #[test]
    fn ip_validation_accepts_well_shaped_ipv4() {
        assert!(validate_ip("192.168.1.1"));
        assert!(validate_ip("8.8.8.8"));
        assert!(validate_ip("1.2.3.4"));
        assert!(validate_ip("255.255.255.255"));
        assert!(validate_ip("0.0.0.0"));
    }

    #[test]
    fn ip_validation_rejects_garbage() {
        assert!(!validate_ip(""));
        assert!(!validate_ip("not an ip"));
        assert!(!validate_ip("1.2.3"));
        assert!(!validate_ip("1.2.3.4.5"));
        assert!(!validate_ip("1..3.4"));
        assert!(!validate_ip(".1.2.3"));
        assert!(!validate_ip("1.2.3."));
        assert!(!validate_ip("1.2.3.4444"));
    }

    #[test]
    fn ip_validation_rejects_injection_shaped_strings() {
        assert!(!validate_ip("1.2.3.4; rm -rf /"));
        assert!(!validate_ip("1.2.3.4 && whoami"));
        assert!(!validate_ip("1.2.3.4\nrm -rf /"));
        assert!(!validate_ip("$(rm -rf /)"));
        assert!(!validate_ip("1.2.3.4`id`"));
        assert!(!validate_ip("../../etc/passwd"));
    }

    #[test]
    fn host_validation_accepts_public_ips_and_hostnames() {
        assert!(validate_host("84.210.13.37"));
        assert!(validate_host("myserver.duckdns.org"));
        assert!(validate_host("my-name.example-host.net"));
        assert!(validate_host("localhost"));
    }

    #[test]
    fn host_validation_rejects_garbage_and_injection_shapes() {
        assert!(!validate_host(""));
        assert!(!validate_host("foo bar"));
        assert!(!validate_host("evil;drop"));
        assert!(!validate_host("a`id`"));
        assert!(!validate_host("$(reboot)"));
        assert!(!validate_host("host\nname"));
        assert!(!validate_host("x'y"));
        assert!(!validate_host(&"a".repeat(254)));
    }

    #[test]
    fn lan_action_allowlist_is_closed() {
        assert!(LAN_ACTIONS.contains(&"on"));
        assert!(LAN_ACTIONS.contains(&"off"));
        assert!(LAN_ACTIONS.contains(&"status"));
        assert!(LAN_ACTIONS.contains(&"refresh"));
        assert!(!LAN_ACTIONS.contains(&"on; rm -rf /"));
        assert!(!LAN_ACTIONS.contains(&"reset"));
    }

    #[test]
    fn validate_lan_request_native_rejects_unknown_action() {
        let e = validate_lan_request("reset", None, false).unwrap_err();
        assert_eq!(e.code, "BAD_ARG");
    }

    #[test]
    fn validate_lan_request_native_requires_ip_for_on_and_refresh() {
        assert_eq!(validate_lan_request("on", None, false).unwrap_err().code, "BAD_ARG");
        assert_eq!(validate_lan_request("refresh", None, false).unwrap_err().code, "BAD_ARG");
    }

    #[test]
    fn validate_lan_request_native_off_and_status_need_no_ip() {
        let (inet, ip) = validate_lan_request("off", None, false).unwrap();
        assert!(!inet);
        assert_eq!(ip, None);
        let (inet, ip) = validate_lan_request("status", None, true).unwrap();
        assert!(!inet); // internet is narrowed to action=="on" only
        assert_eq!(ip, None);
    }

    #[test]
    fn validate_lan_request_native_shape_checks_ip_when_not_internet() {
        assert_eq!(
            validate_lan_request("on", Some("not-an-ip".into()), false).unwrap_err().code,
            "BAD_ARG"
        );
        let (inet, ip) = validate_lan_request("on", Some("192.168.1.5".into()), false).unwrap();
        assert!(!inet);
        assert_eq!(ip.as_deref(), Some("192.168.1.5"));
    }

    #[test]
    fn validate_lan_request_native_internet_only_narrows_for_on() {
        // --internet is only honored for action=="on" (matches wow_lan's own
        // narrowing) -- refresh with internet=true still gets the strict
        // IPv4-shape check, not the loose hostname one.
        let (inet, ip) = validate_lan_request("on", Some("myserver.duckdns.org".into()), true).unwrap();
        assert!(inet);
        assert_eq!(ip.as_deref(), Some("myserver.duckdns.org"));
        assert_eq!(
            validate_lan_request("refresh", Some("myserver.duckdns.org".into()), true).unwrap_err().code,
            "BAD_ARG"
        );
    }

    #[test]
    fn validate_lan_request_native_internet_shape_checks_hostname() {
        assert_eq!(
            validate_lan_request("on", Some("bad host;".into()), true).unwrap_err().code,
            "BAD_ARG"
        );
    }
}
