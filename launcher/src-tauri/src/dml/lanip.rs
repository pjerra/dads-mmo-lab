//! Native-mode **public IP lookup** reader (`wow lan public-ip`, task D1a).
//! Faithful port of `cli/src/90-main.sh:5594-5617`: `curl
//! https://api.ipify.org` -> validate the shape -> `{"public_ip": …|null}`.
//! NEVER errors — any transport failure, timeout, or unexpected body
//! degrades to `null`, exactly like the CLI arm. Uses `reqwest::blocking`
//! (bounded ~6s) instead of shelling `curl`.

use std::time::Duration;

/// Bounded timeout for the public-IP probe — matches the CLI's `curl
/// --max-time 5`, rounded up slightly for the extra process-vs-library
/// overhead this path doesn't otherwise have.
const TIMEOUT: Duration = Duration::from_secs(6);

/// `^[0-9]{1,3}(\.[0-9]{1,3}){3}$` (`90-main.sh` ~5610) — a SHAPE check
/// only, not a real IPv4 range validator (matches the CLI byte-for-byte:
/// e.g. `"999.999.999.999"` also passes here, same as the bash regex).
pub fn looks_like_ipv4(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() == 4
        && parts.iter().all(|p| !p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
}

/// First line only, CR stripped, then shape-validated — a port of:
/// `pubip="${pubip%%$'\n'*}"; pubip="${pubip//$'\r'/}"` (`90-main.sh:5606-
/// 5610`). `None` (-> JSON `null`) for anything that doesn't look like an
/// IPv4 literal (empty body, an HTML captive-portal page, …).
pub fn validate_public_ip(raw: &str) -> Option<String> {
    let first = raw.split('\n').next().unwrap_or("");
    let cleaned: String = first.chars().filter(|&c| c != '\r').collect();
    if looks_like_ipv4(&cleaned) {
        Some(cleaned)
    } else {
        None
    }
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .expect("reqwest client")
}

/// `lan public-ip`: best-effort fetch + validate. Any network error (DNS,
/// connect, timeout) or non-IP body degrades to `None` — this function
/// itself never returns an `Err`, matching the CLI arm which is `json_ok`
/// on every path.
pub fn fetch_public_ip() -> Option<String> {
    let resp = client().get("https://api.ipify.org").send().ok()?;
    let text = resp.text().ok()?;
    validate_public_ip(&text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_like_ipv4_shape_only() {
        assert!(looks_like_ipv4("1.2.3.4"));
        assert!(looks_like_ipv4("255.255.255.255"));
        // shape-only: no range validation, matching the bash regex exactly.
        assert!(looks_like_ipv4("999.999.999.999"));
        assert!(!looks_like_ipv4(""));
        assert!(!looks_like_ipv4("1.2.3"));
        assert!(!looks_like_ipv4("1.2.3.4.5"));
        assert!(!looks_like_ipv4("1.2.3.abc"));
        assert!(!looks_like_ipv4("1..3.4"));
        assert!(!looks_like_ipv4("1234.2.3.4")); // > 3 digits in an octet
    }

    #[test]
    fn validate_public_ip_first_line_and_cr_stripped() {
        assert_eq!(validate_public_ip("1.2.3.4"), Some("1.2.3.4".into()));
        assert_eq!(validate_public_ip("1.2.3.4\r\n"), Some("1.2.3.4".into()));
        assert_eq!(validate_public_ip("1.2.3.4\nsome trailing junk"), Some("1.2.3.4".into()));
    }

    #[test]
    fn validate_public_ip_rejects_non_ip_bodies() {
        assert_eq!(validate_public_ip(""), None);
        assert_eq!(validate_public_ip("<html>captive portal</html>"), None);
        assert_eq!(validate_public_ip("Rate limited"), None);
    }
}
