//! Realmlist check + one-click fix (Batch 2 F7).
//!
//! The WoW 3.3.5 client picks its login server from
//! `Data/<locale>/realmlist.wtf` (`set realmlist <host>`); when that file is
//! empty the client falls back to the `SET realmList "<host>"` CVar it last
//! persisted into `WTF/Config.wtf` (verified on the user's real client: an
//! empty realmlist.wtf + the effective pointer living in Config.wtf). Status
//! therefore reports BOTH, and "matches" is computed on the effective value
//! (realmlist.wtf first, Config.wtf fallback). The fix only ever writes
//! realmlist.wtf -- a non-empty realmlist.wtf wins on the next game start,
//! and Config.wtf is the client's own file to manage.
//!
//! No paths cross IPC: everything derives from the client path stored by the
//! module manager (`dml wow client-path get`, stored in /mnt/c form),
//! translated to a native `C:\` path here and read/written natively.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use dml_wow::runner::DmlRunner;
use crate::CmdError;

/// Locale folders a 3.3.5 client can ship with. Matched case-insensitively:
/// real installs vary (the user's client has `Data/enus`).
pub const LOCALES: [&str; 12] = [
    "enUS", "enGB", "deDE", "frFR", "esES", "esMX", "ruRU", "zhCN", "zhTW", "koKR", "ptBR",
    "itIT",
];

// --- pure helpers (cargo-tested) -------------------------------------------

/// `/mnt/c/wow335` -> `C:\wow335`. The CLI stores the client path in WSL
/// form (see `_client_win_to_wsl` in cli/src/70-modules.sh); this is the
/// exact inverse. A path already in Windows form passes through; anything
/// else (a WSL-internal path like /home/dml/...) is None -- not reachable
/// from the Windows side.
pub fn wsl_to_windows_path(p: &str) -> Option<String> {
    let rest = p.strip_prefix("/mnt/")?;
    let mut chars = rest.chars();
    let drive = chars.next()?;
    if !drive.is_ascii_alphabetic() {
        return None;
    }
    match chars.next() {
        None => Some(format!("{}:\\", drive.to_ascii_uppercase())),
        Some('/') => {
            let tail: String = chars.collect();
            Some(format!("{}:\\{}", drive.to_ascii_uppercase(), tail.replace('/', "\\")))
        }
        Some(_) => None,
    }
}

/// Windows-form passthrough for robustness: `C:\x` or `C:/x` is already
/// native. Combined entry point used by the commands.
pub fn client_path_to_windows(p: &str) -> Option<String> {
    let bytes = p.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Some(p.replace('/', "\\"));
    }
    wsl_to_windows_path(p)
}

/// Extract the target of the last `set realmlist <host>` line. Handles both
/// realmlist.wtf (`set realmlist logon.example.com`) and Config.wtf's CVar
/// form (`SET realmList "127.0.0.1"`): case-insensitive keyword match,
/// quotes stripped. Empty/comment/unrelated lines are ignored; last match
/// wins (the client executes the file top to bottom).
pub fn parse_set_realmlist(content: &str) -> Option<String> {
    let mut found = None;
    for line in content.lines() {
        let t = line.trim();
        let lower = t.to_ascii_lowercase();
        let rest = if let Some(r) = lower.strip_prefix("set realmlist") {
            r
        } else {
            continue;
        };
        // "set realmlistfoo" must not match -- require a separator (or end).
        if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
            continue;
        }
        // Slice the ORIGINAL line at the same offset to preserve case.
        let value = t[t.len() - rest.len()..].trim().trim_matches('"').trim();
        if !value.is_empty() {
            found = Some(value.to_string());
        }
    }
    found
}

/// Strict numeric IPv4 with every octet <= 255 (validate_ip only checks the
/// shape) that is loopback or RFC1918-private.
pub fn is_private_ipv4(ip: &str) -> bool {
    if !crate::validate_ip(ip) {
        return false;
    }
    let octets: Vec<u32> = ip.split('.').map(|s| s.parse().unwrap_or(999)).collect();
    if octets.len() != 4 || octets.iter().any(|&o| o > 255) {
        return false;
    }
    octets[0] == 127
        || octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

/// Fix-target allowlist: 127.0.0.1 / private IPv4 / hostname. Anything
/// shaped like an IPv4 address (digits and dots only) MUST be a valid
/// private address -- without that rule the hostname arm would happily
/// accept 8.8.8.8 or 999.1.1.1.
pub fn validate_realmlist_target(target: &str) -> bool {
    if target.is_empty() || target.len() > 253 {
        return false;
    }
    if target.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return is_private_ipv4(target);
    }
    target.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// The addresses the realmlist is allowed to point at right now.
pub fn expected_targets(lan_ip: Option<&str>) -> Vec<String> {
    let mut v = vec!["127.0.0.1".to_string()];
    if let Some(ip) = lan_ip {
        if !ip.is_empty() && ip != "127.0.0.1" {
            v.push(ip.to_string());
        }
    }
    v
}

/// Case-insensitive membership of the effective pointer in the expected set.
pub fn realmlist_matches(effective: Option<&str>, expected: &[String]) -> bool {
    match effective {
        None => false,
        Some(cur) => expected.iter().any(|e| e.eq_ignore_ascii_case(cur)),
    }
}

// --- impure part (thin, manual smoke only) ----------------------------------

#[derive(Debug, Serialize)]
pub struct RealmlistStatus {
    /// Stored client path as the CLI keeps it (WSL form), null when unset.
    pub client_path: Option<String>,
    /// Same path translated to C:\ form; null when not on a Windows drive.
    pub windows_path: Option<String>,
    /// Full path of Data/<locale>/realmlist.wtf, null when no locale dir.
    pub path: Option<String>,
    pub exists: bool,
    pub readonly: bool,
    pub content: String,
    /// Target parsed from realmlist.wtf itself.
    pub current: Option<String>,
    /// Fallback target parsed from WTF/Config.wtf (informational).
    pub config_wtf: Option<String>,
    pub expected: Vec<String>,
    pub matches: bool,
}

fn empty_status(client_path: Option<String>, windows_path: Option<String>, expected: Vec<String>) -> RealmlistStatus {
    RealmlistStatus {
        client_path,
        windows_path,
        path: None,
        exists: false,
        readonly: false,
        content: String::new(),
        current: None,
        config_wtf: None,
        expected,
        matches: false,
    }
}

/// Stored client path via the existing CLI arm. None = not set / no longer a
/// valid folder (the CLI's `get` reports valid:false then; we only trust a
/// path the CLI still considers valid).
fn stored_client_path(runner: &DmlRunner) -> Result<Option<String>, CmdError> {
    let env = runner.run_json(&["wow", "client-path", "get"]).map_err(CmdError::from)?;
    let data = crate::envelope_to_result(env)?;
    if data["valid"].as_bool() != Some(true) {
        return Ok(None);
    }
    Ok(data["path"].as_str().map(str::to_string))
}

/// Find Data/<locale>/realmlist.wtf under the client. Prefers a locale dir
/// that already has the file; otherwise the first locale dir (the fix will
/// create the file there).
fn find_realmlist(client_win: &str) -> Option<PathBuf> {
    let data = Path::new(client_win).join("Data");
    let mut fallback: Option<PathBuf> = None;
    for entry in std::fs::read_dir(data).ok()?.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy().to_string();
        if !LOCALES.iter().any(|l| l.eq_ignore_ascii_case(&name)) {
            continue;
        }
        let p = entry.path();
        if !p.is_dir() {
            continue;
        }
        let rl = p.join("realmlist.wtf");
        if rl.is_file() {
            return Some(rl);
        }
        if fallback.is_none() {
            fallback = Some(rl);
        }
    }
    fallback
}

fn build_status(
    client_path: Option<String>,
    windows_path: Option<String>,
    lan_ip: Option<String>,
) -> RealmlistStatus {
    let expected = expected_targets(lan_ip.as_deref());
    let win = match &windows_path {
        Some(w) => w.clone(),
        None => return empty_status(client_path, windows_path, expected),
    };
    let mut st = empty_status(client_path, windows_path, expected);
    // Informational fallback pointer from WTF/Config.wtf.
    st.config_wtf = std::fs::read_to_string(Path::new(&win).join("WTF").join("Config.wtf"))
        .ok()
        .as_deref()
        .and_then(parse_set_realmlist);
    if let Some(rl) = find_realmlist(&win) {
        st.path = Some(rl.display().to_string());
        if let Ok(meta) = std::fs::metadata(&rl) {
            st.exists = meta.is_file();
            st.readonly = meta.permissions().readonly();
        }
        if st.exists {
            st.content = std::fs::read_to_string(&rl).unwrap_or_default();
            st.current = parse_set_realmlist(&st.content);
        }
    }
    let effective = st.current.as_deref().or(st.config_wtf.as_deref());
    st.matches = realmlist_matches(effective, &st.expected);
    st
}

pub fn status(runner: &Arc<DmlRunner>, lan_ip: Option<String>) -> Result<RealmlistStatus, CmdError> {
    let client = stored_client_path(runner)?;
    let win = client.as_deref().and_then(client_path_to_windows);
    Ok(build_status(client, win, lan_ip))
}

fn resolve_realmlist_path(runner: &Arc<DmlRunner>) -> Result<(Option<String>, Option<String>, PathBuf), CmdError> {
    let client = stored_client_path(runner)?.ok_or_else(|| CmdError {
        code: "NO_CLIENT_PATH".into(),
        message: "No WoW client folder is set".into(),
        hint: "Set the client folder on the Modules page first.".into(),
    })?;
    let win = client_path_to_windows(&client).ok_or_else(|| CmdError {
        code: "NOT_WINDOWS_PATH".into(),
        message: format!("The stored client path is not on a Windows drive: {client}"),
        hint: "Realmlist fixing needs a client under C:\\ (or another drive letter).".into(),
    })?;
    let rl = find_realmlist(&win).ok_or_else(|| CmdError {
        code: "NO_LOCALE_DIR".into(),
        message: "Couldn't find a locale folder (like Data\\enUS) in the client".into(),
        hint: "Is this really a WoW client folder?".into(),
    })?;
    Ok((Some(client), Some(win), rl))
}

fn set_readonly(path: &Path, readonly: bool) -> Result<(), CmdError> {
    let meta = std::fs::metadata(path).map_err(|e| CmdError {
        code: "IO".into(),
        message: format!("Couldn't read attributes of {}: {e}", path.display()),
        hint: String::new(),
    })?;
    let mut perms = meta.permissions();
    perms.set_readonly(readonly);
    std::fs::set_permissions(path, perms).map_err(|e| CmdError {
        code: "IO".into(),
        message: format!("Couldn't change the read-only flag on {}: {e}", path.display()),
        hint: String::new(),
    })
}

/// Write `set realmlist <target>` (target already validated by the command
/// layer). A read-only file is temporarily unlocked and re-locked after.
pub fn fix(runner: &Arc<DmlRunner>, target: &str, lan_ip: Option<String>) -> Result<RealmlistStatus, CmdError> {
    let (client, win, rl) = resolve_realmlist_path(runner)?;
    let was_readonly =
        std::fs::metadata(&rl).map(|m| m.permissions().readonly()).unwrap_or(false);
    if was_readonly {
        set_readonly(&rl, false)?;
    }
    let res = std::fs::write(&rl, format!("set realmlist {target}\r\n"));
    if was_readonly {
        let _ = set_readonly(&rl, true);
    }
    res.map_err(|e| CmdError {
        code: "WRITE_FAILED".into(),
        message: format!("Couldn't write {}: {e}", rl.display()),
        hint: "Is the game running? Close it and try again.".into(),
    })?;
    Ok(build_status(client, win, lan_ip))
}

/// Toggle the read-only attribute ("stops other launchers from overwriting
/// it"). The file must exist -- locking a missing file is meaningless.
pub fn lock(runner: &Arc<DmlRunner>, locked: bool, lan_ip: Option<String>) -> Result<RealmlistStatus, CmdError> {
    let (client, win, rl) = resolve_realmlist_path(runner)?;
    if !rl.is_file() {
        return Err(CmdError {
            code: "NOT_FOUND".into(),
            message: "realmlist.wtf doesn't exist yet".into(),
            hint: "Use one of the Fix buttons first.".into(),
        });
    }
    set_readonly(&rl, locked)?;
    Ok(build_status(client, win, lan_ip))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- path translation ---------------------------------------------------

    #[test]
    fn wsl_paths_translate_to_windows_form() {
        assert_eq!(wsl_to_windows_path("/mnt/c/wow335ahd"), Some("C:\\wow335ahd".into()));
        assert_eq!(
            wsl_to_windows_path("/mnt/d/Games/WoW 3.3.5/client"),
            Some("D:\\Games\\WoW 3.3.5\\client".into())
        );
        assert_eq!(wsl_to_windows_path("/mnt/c"), Some("C:\\".into()));
    }

    #[test]
    fn non_windows_paths_do_not_translate() {
        assert_eq!(wsl_to_windows_path("/home/dml/Games/wow"), None);
        assert_eq!(wsl_to_windows_path("/mnt/wsl/something"), None); // multi-char "drive"
        assert_eq!(wsl_to_windows_path(""), None);
        assert_eq!(wsl_to_windows_path("relative/path"), None);
    }

    #[test]
    fn windows_form_passes_through_client_path_to_windows() {
        assert_eq!(client_path_to_windows("C:\\wow"), Some("C:\\wow".into()));
        assert_eq!(client_path_to_windows("c:/wow/335"), Some("c:\\wow\\335".into()));
        assert_eq!(client_path_to_windows("/mnt/c/wow"), Some("C:\\wow".into()));
        assert_eq!(client_path_to_windows("/srv/wow"), None);
    }

    // --- realmlist parsing --------------------------------------------------

    #[test]
    fn parses_plain_realmlist_wtf_lines() {
        assert_eq!(parse_set_realmlist("set realmlist 127.0.0.1\r\n"), Some("127.0.0.1".into()));
        assert_eq!(
            parse_set_realmlist("set realmlist logon.warmane.com"),
            Some("logon.warmane.com".into())
        );
    }

    #[test]
    fn parses_config_wtf_cvar_form_with_quotes_and_case() {
        // Exactly what the user's real WTF/Config.wtf contains.
        assert_eq!(parse_set_realmlist("SET realmList \"127.0.0.1\"\r\n"), Some("127.0.0.1".into()));
    }

    #[test]
    fn last_set_realmlist_line_wins() {
        let c = "set realmlist old.example.com\nset realmlist 192.168.1.50\n";
        assert_eq!(parse_set_realmlist(c), Some("192.168.1.50".into()));
    }

    #[test]
    fn ignores_empty_files_comments_and_lookalikes() {
        assert_eq!(parse_set_realmlist(""), None);
        assert_eq!(parse_set_realmlist("\r\n\r\n"), None);
        assert_eq!(parse_set_realmlist("# set the realmlist below\nset patchlist 127.0.0.1"), None);
        assert_eq!(parse_set_realmlist("set realmlistfoo bar"), None);
        assert_eq!(parse_set_realmlist("set realmlist"), None); // no target
    }

    // --- fix-target validation ----------------------------------------------

    #[test]
    fn accepts_localhost_and_private_ipv4_targets() {
        assert!(validate_realmlist_target("127.0.0.1"));
        assert!(validate_realmlist_target("192.168.1.50"));
        assert!(validate_realmlist_target("10.0.0.2"));
        assert!(validate_realmlist_target("172.16.0.1"));
        assert!(validate_realmlist_target("172.31.255.254"));
    }

    #[test]
    fn accepts_hostnames() {
        assert!(validate_realmlist_target("logon.example.com"));
        assert!(validate_realmlist_target("my-pc"));
        assert!(validate_realmlist_target("wow-server.local"));
    }

    #[test]
    fn rejects_public_and_malformed_ipv4_shaped_targets() {
        assert!(!validate_realmlist_target("8.8.8.8")); // public
        assert!(!validate_realmlist_target("172.32.0.1")); // outside 172.16-31
        assert!(!validate_realmlist_target("999.1.1.1")); // octet > 255
        assert!(!validate_realmlist_target("1.2.3.4.5")); // 5 groups
        assert!(!validate_realmlist_target("1.2.3")); // 3 groups
    }

    #[test]
    fn rejects_injection_shaped_and_oversized_targets() {
        assert!(!validate_realmlist_target(""));
        assert!(!validate_realmlist_target("127.0.0.1; rm -rf /"));
        assert!(!validate_realmlist_target("host name"));
        assert!(!validate_realmlist_target("host\nname"));
        assert!(!validate_realmlist_target("host_name")); // underscore not in the allowlist
        assert!(!validate_realmlist_target(&"a".repeat(254)));
    }

    // --- expected/matches ---------------------------------------------------

    #[test]
    fn expected_targets_always_include_localhost_and_dedupe_it() {
        assert_eq!(expected_targets(None), vec!["127.0.0.1".to_string()]);
        assert_eq!(
            expected_targets(Some("192.168.1.50")),
            vec!["127.0.0.1".to_string(), "192.168.1.50".to_string()]
        );
        assert_eq!(expected_targets(Some("127.0.0.1")), vec!["127.0.0.1".to_string()]);
        assert_eq!(expected_targets(Some("")), vec!["127.0.0.1".to_string()]);
    }

    #[test]
    fn matches_is_case_insensitive_membership_of_the_effective_pointer() {
        let exp = expected_targets(Some("192.168.1.50"));
        assert!(realmlist_matches(Some("127.0.0.1"), &exp));
        assert!(realmlist_matches(Some("192.168.1.50"), &exp));
        assert!(!realmlist_matches(Some("logon.warmane.com"), &exp));
        assert!(!realmlist_matches(None, &exp));
    }

    #[test]
    fn hostname_match_ignores_case() {
        let exp = vec!["My-Server.local".to_string()];
        assert!(realmlist_matches(Some("my-server.LOCAL"), &exp));
    }
}
