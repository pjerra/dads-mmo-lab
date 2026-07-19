// Batch 4 F17: Windows disk & performance tools -- pure helpers.
//
// * .wslconfig parse/merge: minimal section-aware INI editing of
//   %USERPROFILE%\.wslconfig. Only the [wsl2] section is ever touched;
//   every unrelated line/section/comment is preserved verbatim. An absent
//   file is just empty content.
// * Lxss registry parse: locate a distro's BasePath from `reg query`
//   output (HKCU\...\Lxss) -- used to show the ext4.vhdx location for the
//   Defender-exclusion card. The shrink SCRIPT does its own discovery at
//   run time (it may run elevated, later, on a changed system).
// * Shrink-script builder: the PowerShell text dropped into Downloads.
//
// All pure (no fs/registry access here) so cargo tests cover them without
// touching the machine.

/// True when a line is the `[wsl2]` section header (case-insensitive).
fn is_wsl2_header(line: &str) -> bool {
    line.trim().eq_ignore_ascii_case("[wsl2]")
}

/// True when a trimmed line is ANY `[section]` header.
fn is_section_header(line: &str) -> bool {
    let t = line.trim();
    t.starts_with('[') && t.ends_with(']')
}

/// Split an INI line into (key, value) if it is a `key=value` assignment.
fn kv(line: &str) -> Option<(&str, &str)> {
    let t = line.trim();
    if t.starts_with('#') || t.starts_with(';') {
        return None;
    }
    let eq = t.find('=')?;
    Some((t[..eq].trim(), t[eq + 1..].trim()))
}

/// Read `key`'s value from the `[wsl2]` section. Absent file = pass "".
pub fn read_wsl2_key(content: &str, key: &str) -> Option<String> {
    let mut in_wsl2 = false;
    for line in content.lines() {
        if is_section_header(line) {
            in_wsl2 = is_wsl2_header(line);
            continue;
        }
        if !in_wsl2 {
            continue;
        }
        if let Some((k, v)) = kv(line) {
            if k.eq_ignore_ascii_case(key) {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// Set `key = value` inside `[wsl2]`, preserving everything else:
/// * key present in [wsl2]  -> the FIRST occurrence is replaced in place
///   (later duplicates are dropped -- they would silently win otherwise);
/// * key absent, section present -> appended right after the header;
/// * section absent -> a new [wsl2] section is appended at the end.
/// Unrelated sections' identical key names are never touched.
pub fn merge_wsl2_key(content: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_wsl2 = false;
    let mut seen_section = false;
    let mut replaced = false;
    for line in content.lines() {
        if is_section_header(line) {
            in_wsl2 = is_wsl2_header(line);
            if in_wsl2 {
                seen_section = true;
                out.push(line.to_string());
                continue;
            }
        }
        if in_wsl2 && !is_section_header(line) {
            if let Some((k, _)) = kv(line) {
                if k.eq_ignore_ascii_case(key) {
                    if !replaced {
                        out.push(format!("{key}={value}"));
                        replaced = true;
                    }
                    continue; // drop duplicates
                }
            }
        }
        out.push(line.to_string());
    }
    if !replaced {
        if seen_section {
            // insert right after the [wsl2] header line
            let mut merged: Vec<String> = Vec::new();
            let mut inserted = false;
            for line in out {
                let header = is_wsl2_header(&line);
                merged.push(line);
                if header && !inserted {
                    merged.push(format!("{key}={value}"));
                    inserted = true;
                }
            }
            out = merged;
        } else {
            if !out.is_empty() && !out.last().map(|l| l.trim().is_empty()).unwrap_or(true) {
                out.push(String::new());
            }
            out.push("[wsl2]".to_string());
            out.push(format!("{key}={value}"));
        }
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// Memory spec for [wsl2] memory=: digits + GB/MB (e.g. "8GB", "512MB").
pub fn valid_memory_spec(v: &str) -> bool {
    let upper = v.to_ascii_uppercase();
    let digits = upper.trim_end_matches("GB").trim_end_matches("MB");
    (upper.ends_with("GB") || upper.ends_with("MB"))
        && !digits.is_empty()
        && digits.len() <= 4
        && digits.chars().all(|c| c.is_ascii_digit())
        && digits.parse::<u32>().map(|n| n > 0).unwrap_or(false)
}

/// Processor count for [wsl2] processors=: 1..=999.
pub fn valid_processors_spec(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 3
        && v.chars().all(|c| c.is_ascii_digit())
        && v.parse::<u32>().map(|n| n > 0).unwrap_or(false)
}

/// Find `distro`'s BasePath in `reg query HKCU\...\Lxss /s` output. The
/// output is blocks of:
///   HKEY_CURRENT_USER\...\Lxss\{guid}
///       DistributionName    REG_SZ    dml-arch
///       BasePath            REG_SZ    C:\...\dml-arch
/// Values can arrive in any order inside a block, so collect per block.
pub fn parse_lxss_base_path(reg_output: &str, distro: &str) -> Option<String> {
    let mut block_base: Option<String> = None;
    let mut block_matches = false;
    for line in reg_output.lines().chain(std::iter::once("HKEY_")) {
        if line.starts_with("HKEY_") {
            // new block boundary -- flush the previous one
            if block_matches {
                if let Some(b) = block_base {
                    return Some(b);
                }
            }
            block_base = None;
            block_matches = false;
            continue;
        }
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("DistributionName") {
            if let Some(v) = rest.split("REG_SZ").nth(1) {
                if v.trim() == distro {
                    block_matches = true;
                }
            }
        } else if let Some(rest) = t.strip_prefix("BasePath") {
            if let Some(v) = rest.split("REG_SZ").nth(1) {
                block_base = Some(v.trim().to_string());
            }
        }
    }
    None
}

/// The PowerShell shrink script dropped into Downloads. Discovers the
/// distro's ext4.vhdx itself via HKCU Lxss (run-time truth, works when run
/// later/elevated), trims, shuts WSL down, then diskpart-compacts.
/// Deliberately NO `Optimize-VHD` (needs Hyper-V module) and NO
/// `--set-sparse` (upstream WSL explicitly warns against it).
pub fn compact_script(distro: &str) -> String {
    format!(
        r#"# DML Launcher -- shrink the {distro} WSL disk (generated script)
#
#   RUN AS ADMINISTRATOR: right-click this file -> "Run with PowerShell"
#   from an elevated PowerShell, or it will fail at the diskpart step.
#
#   * Stop your game server from the DML Launcher FIRST.
#   * This shuts down ALL WSL distros (next start is a cold start).
#   * Compacting can take MANY minutes on a large disk. Let it finish.

$ErrorActionPreference = 'Stop'
Write-Host '== DML shrink disk =='

# 1. Find {distro}'s virtual disk via the registry
$lxss = Get-ChildItem 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss' |
    Where-Object {{ ($_ | Get-ItemProperty).DistributionName -eq '{distro}' }}
if (-not $lxss) {{ Write-Host 'ERROR: distro {distro} not found in the registry.'; pause; exit 1 }}
$base = ($lxss | Get-ItemProperty).BasePath -replace '^\\\\\?\\', ''
$vhdx = Join-Path $base 'ext4.vhdx'
if (-not (Test-Path $vhdx)) {{ Write-Host "ERROR: $vhdx not found."; pause; exit 1 }}
Write-Host "Disk: $vhdx"
Write-Host ('Size before: {{0:N1}} GB' -f ((Get-Item $vhdx).Length / 1GB))

# 2. Trim free space inside the distro, then shut all of WSL down
Write-Host 'Trimming free space inside {distro} (fstrim)...'
wsl -d {distro} -u root -- fstrim /
Write-Host 'Shutting down WSL...'
wsl --shutdown
Start-Sleep -Seconds 3

# 3. Compact the virtual disk with diskpart (read-only attach)
Write-Host 'Compacting (this can take many minutes, progress shows below)...'
$dp = @"
select vdisk file="$vhdx"
attach vdisk readonly
compact vdisk
detach vdisk
"@
$dp | diskpart

Write-Host ('Size after: {{0:N1}} GB' -f ((Get-Item $vhdx).Length / 1GB))
Write-Host 'Done. Start the server from the DML Launcher as usual (cold start).'
pause
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_from_absent_file_is_none() {
        assert_eq!(read_wsl2_key("", "memory"), None);
    }

    #[test]
    fn read_finds_keys_only_inside_wsl2() {
        let c = "[experimental]\nmemory=1GB\n[wsl2]\nmemory=8GB\nprocessors=4\n";
        assert_eq!(read_wsl2_key(c, "memory").as_deref(), Some("8GB"));
        assert_eq!(read_wsl2_key(c, "processors").as_deref(), Some("4"));
        assert_eq!(read_wsl2_key(c, "swap"), None);
    }

    #[test]
    fn read_is_case_insensitive_and_trims() {
        let c = "[WSL2]\n  Memory = 12GB\n";
        assert_eq!(read_wsl2_key(c, "memory").as_deref(), Some("12GB"));
    }

    #[test]
    fn merge_into_absent_file_creates_the_section() {
        let out = merge_wsl2_key("", "memory", "8GB");
        assert_eq!(out, "[wsl2]\nmemory=8GB\n");
        assert_eq!(read_wsl2_key(&out, "memory").as_deref(), Some("8GB"));
    }

    #[test]
    fn merge_replaces_in_place_and_preserves_other_lines() {
        let c = "# my comment\n[experimental]\nsparseVhd=true\n[wsl2]\nmemory=4GB\nswap=2GB\n";
        let out = merge_wsl2_key(c, "memory", "16GB");
        assert!(out.contains("# my comment"));
        assert!(out.contains("[experimental]\nsparseVhd=true"));
        assert!(out.contains("memory=16GB"));
        assert!(out.contains("swap=2GB"));
        assert!(!out.contains("memory=4GB"));
        // the untouched [experimental] section must not have gained the key
        assert_eq!(read_wsl2_key(&out, "memory").as_deref(), Some("16GB"));
    }

    #[test]
    fn merge_appends_when_key_missing_in_existing_section() {
        let c = "[wsl2]\nprocessors=4\n";
        let out = merge_wsl2_key(c, "memory", "8GB");
        assert_eq!(read_wsl2_key(&out, "memory").as_deref(), Some("8GB"));
        assert_eq!(read_wsl2_key(&out, "processors").as_deref(), Some("4"));
        // appended right after the header, before existing keys
        assert!(out.starts_with("[wsl2]\nmemory=8GB\nprocessors=4"));
    }

    #[test]
    fn merge_appends_section_after_unrelated_sections() {
        let c = "[experimental]\nautoMemoryReclaim=gradual\n";
        let out = merge_wsl2_key(c, "processors", "6");
        assert!(out.contains("[experimental]\nautoMemoryReclaim=gradual"));
        assert!(out.contains("[wsl2]\nprocessors=6"));
        assert_eq!(read_wsl2_key(&out, "processors").as_deref(), Some("6"));
    }

    #[test]
    fn merge_collapses_duplicate_keys_to_one() {
        let c = "[wsl2]\nmemory=4GB\nmemory=6GB\n";
        let out = merge_wsl2_key(c, "memory", "8GB");
        assert_eq!(out.matches("memory=").count(), 1);
        assert_eq!(read_wsl2_key(&out, "memory").as_deref(), Some("8GB"));
    }

    #[test]
    fn memory_spec_validation() {
        assert!(valid_memory_spec("8GB"));
        assert!(valid_memory_spec("512MB"));
        assert!(valid_memory_spec("16gb"));
        assert!(!valid_memory_spec(""));
        assert!(!valid_memory_spec("8"));
        assert!(!valid_memory_spec("GB"));
        assert!(!valid_memory_spec("0GB"));
        assert!(!valid_memory_spec("8 GB"));
        assert!(!valid_memory_spec("8GB; rm"));
        assert!(!valid_memory_spec("99999GB"));
    }

    #[test]
    fn processors_spec_validation() {
        assert!(valid_processors_spec("1"));
        assert!(valid_processors_spec("16"));
        assert!(!valid_processors_spec(""));
        assert!(!valid_processors_spec("0"));
        assert!(!valid_processors_spec("4.5"));
        assert!(!valid_processors_spec("all"));
        assert!(!valid_processors_spec("1234"));
    }

    #[test]
    fn lxss_parse_finds_the_matching_block() {
        let reg = "\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss\\{aaa}\r\n    DistributionName    REG_SZ    Ubuntu\r\n    BasePath    REG_SZ    C:\\wsl\\ubuntu\r\n\r\nHKEY_CURRENT_USER\\Software\\Microsoft\\Windows\\CurrentVersion\\Lxss\\{bbb}\r\n    BasePath    REG_SZ    C:\\wsl\\dml-arch\r\n    DistributionName    REG_SZ    dml-arch\r\n";
        assert_eq!(parse_lxss_base_path(reg, "dml-arch").as_deref(), Some("C:\\wsl\\dml-arch"));
        assert_eq!(parse_lxss_base_path(reg, "Ubuntu").as_deref(), Some("C:\\wsl\\ubuntu"));
        assert_eq!(parse_lxss_base_path(reg, "missing"), None);
        assert_eq!(parse_lxss_base_path("", "dml-arch"), None);
    }

    #[test]
    fn compact_script_contains_the_safety_rails_and_no_set_sparse() {
        let s = compact_script("dml-arch");
        assert!(s.contains("RUN AS ADMINISTRATOR"));
        assert!(s.contains("fstrim /"));
        assert!(s.contains("wsl --shutdown"));
        assert!(s.contains("attach vdisk readonly"));
        assert!(s.contains("compact vdisk"));
        assert!(s.contains("detach vdisk"));
        assert!(!s.contains("--set-sparse"));
        assert!(!s.contains("set-sparse"));
        // registry discovery, not a baked-in path
        assert!(s.contains("Lxss"));
        assert!(s.contains("DistributionName"));
    }
}
