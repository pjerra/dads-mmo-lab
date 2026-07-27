//! Native-mode `dml lan on/off/status/refresh` pure helpers (Chunk 2 task
//! C2c item 3 -- see `.superpowers/sdd/chunk2-decisions.md`). A faithful
//! port of the regex classifiers and the exact message text from the
//! AzerothCore branch of `90-main.sh:858-1052`.
//!
//! WHY A SEPARATE MODULE. The docker/DB orchestration (container checks, the
//! "wait for the realm DB to answer" retry loop, the UPDATE + read-back
//! write) needs real I/O and lives in `lib.rs::wow_lan_native_blocking`,
//! alongside every other native command's blocking half. What's left here is
//! exactly what CAN be tested without a live server: the private-address
//! classifier (security-relevant -- it is the only thing standing between
//! `dml lan on <ip>` and silently exposing the realm to a public address)
//! and the verbatim success/error text `wow_lan_native` returns, so the
//! `<pre>`-rendered output stays byte-identical to the WSL sibling's
//! `dml lan` stdout (see `wowLan`'s doc comment in `api.ts`).
//!
//! AC-ONLY BY DECISION (chunk2-decisions.md item 3): `dml::db` has no
//! MaNGOS/Tortoise support, so native mode never reaches the oracle's
//! `tw_logon` branch (`90-main.sh:942-960`) -- WSL keeps handling every
//! other title family; native only ever drives the single fixed AC title.

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
}
