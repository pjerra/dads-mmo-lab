//! Native-mode setup helpers (spike: `spike/docker-desktop-native`).
//!
//! Pure, testable pieces behind the launcher's "Native setup" bootstrap card:
//! the pinned `yq` download URL + size floor, the elevated Defender-exclusion
//! PowerShell script text, CR-stripping for the copied SOAP creds, and the
//! Git-root discovery from a bash.exe path. All env/filesystem/network work
//! lives in the Tauri commands (lib.rs) that call these; keeping the logic here
//! pure means cargo covers it without a network, an admin prompt, or a real
//! Docker/WSL install.

/// Pinned mikefarah/yq release the installer fetches. Bumping this is a
/// deliberate, reviewable change — never resolve "latest" at runtime.
pub const YQ_VERSION: &str = "v4.44.3";

/// Windows amd64 single-exe asset name in that release.
pub const YQ_ASSET: &str = "yq_windows_amd64.exe";

/// Sanity floor for a successful download: the real asset is ~12 MB, so
/// anything under 1 MiB is an error page / truncated body, never yq. The
/// installer refuses to save a file smaller than this.
pub const YQ_MIN_BYTES: u64 = 1_048_576;

/// The https URL the pinned Windows `yq` exe is fetched from (a GitHub release
/// asset — the download redirects to objects.githubusercontent.com, so the
/// fetching client must FOLLOW redirects).
pub fn yq_download_url() -> String {
    format!("https://github.com/mikefarah/yq/releases/download/{YQ_VERSION}/{YQ_ASSET}")
}

/// Strip carriage returns from text copied out of WSL: `cat`-ing a Unix file
/// back through the Windows pipe can carry CRs, and a stray CR inside an env
/// value breaks shells that later source the file. Leaves LF newlines intact.
pub fn strip_cr(s: &str) -> String {
    s.replace('\r', "")
}

/// Given Git-for-Windows' `bash.exe` path (…\Git\bin\bash.exe or
/// …\Git\usr\bin\bash.exe), return the `Git` install root to exclude from
/// Defender. Walks ancestors for the directory literally named `Git`
/// (case-insensitive). `None` when the path has no such ancestor — the caller
/// then falls back to the standard install location.
pub fn git_root_from_bash(bash: &str) -> Option<String> {
    let p = std::path::Path::new(bash);
    for anc in p.ancestors() {
        if anc
            .file_name()
            .map(|n| n.eq_ignore_ascii_case("git"))
            .unwrap_or(false)
        {
            return Some(anc.to_string_lossy().into_owned());
        }
    }
    None
}

/// The elevated PowerShell script the "Speed up native mode" button drops into
/// Downloads. Adds a Defender real-time-scan exclusion for each discovered
/// `path`. Mirrors the generate-and-run-as-admin pattern + safety-header style
/// of `wslconfig::mysql_expose_script` / `compact_script`: an Administrator
/// self-check that bails BEFORE any `Add-MpPreference`, a header explaining WHY
/// (the per-process AV scan that makes a Git-bash fork cost ~165ms) and HOW to
/// run it, and undo instructions at the end. The app never runs this — the user
/// right-clicks it and runs it as Administrator.
pub fn defender_exclusion_script(paths: &[&str]) -> String {
    // Each path becomes a single-quoted array element; embedded single quotes
    // are doubled per PowerShell literal-string escaping.
    let adds = paths
        .iter()
        .map(|p| format!("    '{}'", p.replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",\n");
    format!(
        r#"# DML Launcher -- speed up NATIVE mode (Windows Defender exclusions) [generated]
#
#   RUN AS ADMINISTRATOR: right-click this file -> "Run with PowerShell"
#   from an elevated PowerShell, or the Add-MpPreference calls will fail.
#
#   WHY THIS EXISTS
#   In native mode the `dml` CLI runs under Git Bash on Windows. Git Bash
#   (Cygwin) emulates Unix fork() by copy-and-relaunch, so every command the
#   CLI runs is a brand-new Windows process -- and Defender's real-time
#   protection scans EACH one. That per-process scan is what makes a single
#   bash fork cost ~165ms here (vs ~1ms without it), so a CLI arm that spawns
#   hundreds of small helpers crawls. Excluding the trusted folders below from
#   real-time scanning removes that per-fork tax.
#
#   This is OPTIONAL and easy to undo (see the bottom of this file). These are
#   local program/data folders (Git, Docker Desktop, your game data) -- a
#   considered trade-off, NOT a blanket "turn antivirus off".

$ErrorActionPreference = 'Stop'
Write-Host '== DML native-mode Defender exclusions =='

# Refuse to run without Administrator rights: Add-MpPreference needs elevation.
# Bail BEFORE touching Defender so a non-admin run changes nothing.
$admin = ([Security.Principal.WindowsPrincipal] `
    [Security.Principal.WindowsIdentity]::GetCurrent() `
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
if (-not $admin) {{
    Write-Host 'ERROR: This script must be run as Administrator.'
    Write-Host 'Right-click the file and choose "Run with PowerShell", or start an'
    Write-Host 'elevated PowerShell (Run as administrator) and run it from there.'
    pause
    exit 1
}}

$paths = @(
{adds}
)

foreach ($p in $paths) {{
    if (Test-Path $p) {{
        Write-Host "Excluding: $p"
        Add-MpPreference -ExclusionPath $p
    }} else {{
        Write-Host "Skipping (not found): $p"
    }}
}}

Write-Host ''
Write-Host 'Done. Native-mode CLI commands should fork noticeably faster now.'
Write-Host ''
Write-Host 'TO UNDO later (admin PowerShell), for any path above:'
Write-Host '    Remove-MpPreference -ExclusionPath "<path>"'
pause
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yq_url_pins_version_and_windows_asset() {
        let url = yq_download_url();
        assert!(url.starts_with("https://github.com/mikefarah/yq/releases/download/"));
        assert!(url.contains(YQ_VERSION));
        assert!(url.ends_with("yq_windows_amd64.exe"));
    }

    #[test]
    fn yq_min_bytes_is_one_mib() {
        assert_eq!(YQ_MIN_BYTES, 1024 * 1024);
    }

    #[test]
    fn strip_cr_removes_carriage_returns_only() {
        assert_eq!(strip_cr("A=1\r\nB=2\r\n"), "A=1\nB=2\n");
        assert_eq!(strip_cr("no-cr\n"), "no-cr\n");
        assert_eq!(strip_cr("\r\r"), "");
    }

    #[test]
    fn git_root_found_in_both_bash_layouts() {
        assert_eq!(
            git_root_from_bash(r"C:\Program Files\Git\bin\bash.exe").as_deref(),
            Some(r"C:\Program Files\Git")
        );
        assert_eq!(
            git_root_from_bash(r"C:\Program Files\Git\usr\bin\bash.exe").as_deref(),
            Some(r"C:\Program Files\Git")
        );
        // Case-insensitive on the directory name.
        assert_eq!(
            git_root_from_bash(r"D:\tools\GIT\bin\bash.exe").as_deref(),
            Some(r"D:\tools\GIT")
        );
    }

    #[test]
    fn git_root_none_when_no_git_ancestor() {
        assert_eq!(git_root_from_bash(r"C:\msys64\usr\bin\bash.exe"), None);
        assert_eq!(git_root_from_bash("bash"), None);
    }

    #[test]
    fn defender_script_has_safety_rails_and_the_why() {
        let s = defender_exclusion_script(&[r"C:\Program Files\Git", r"C:\games"]);
        assert!(s.contains("RUN AS ADMINISTRATOR"));
        // Admin self-check that bails BEFORE any Add-MpPreference.
        assert!(s.contains("IsInRole"));
        let admin_at = s.find("IsInRole").expect("admin check present");
        // rfind: the header comment mentions Add-MpPreference too; the LAST
        // occurrence is the real privileged call, which must follow the check.
        let add_at = s.rfind("Add-MpPreference").expect("add present");
        assert!(admin_at < add_at, "admin check must precede the Add-MpPreference call");
        // The WHY (per-fork AV scan cost) is spelled out.
        assert!(s.contains("165ms"));
        assert!(s.contains("fork"));
        // Undo instructions.
        assert!(s.contains("TO UNDO"));
        assert!(s.contains("Remove-MpPreference"));
        // Both paths made it into the array.
        assert!(s.contains(r"'C:\Program Files\Git'"));
        assert!(s.contains(r"'C:\games'"));
    }

    #[test]
    fn defender_script_escapes_single_quotes_in_paths() {
        let s = defender_exclusion_script(&[r"C:\o'brien\Git"]);
        // A literal apostrophe is doubled for a PowerShell single-quoted string.
        assert!(s.contains(r"'C:\o''brien\Git'"));
    }

    #[test]
    fn defender_script_renders_with_no_paths() {
        // Degenerate but must not panic or produce a broken array.
        let s = defender_exclusion_script(&[]);
        assert!(s.contains("$paths = @(\n\n)"));
        assert!(s.contains("Add-MpPreference"));
    }
}
