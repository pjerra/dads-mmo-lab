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

/// Decode raw .wslconfig bytes to UTF-8 text for parse/merge.
/// * A leading UTF-8 BOM (EF BB BF) is stripped -- left in place it would glue
///   onto the first `[wsl2]` header and hide the section from the parser.
/// * A UTF-16 BOM (LE FF FE / BE FE FF) returns Err: `read_to_string` would
///   fail on it and the caller's `.unwrap_or_default()` would silently treat
///   the file as EMPTY, so the next merge-write would drop every existing
///   setting. Surface a clear "save as UTF-8" message instead.
/// Anything else is read as UTF-8 (lossy -- stray bytes never abort the read).
pub fn decode_wslconfig(bytes: &[u8]) -> Result<String, &'static str> {
    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        return Err("utf16");
    }
    let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    Ok(String::from_utf8_lossy(body).into_owned())
}

/// Memory spec for [wsl2] memory=: EXACTLY digits + one GB/MB unit
/// (e.g. "8GB", "512MB"). `strip_suffix` (not `trim_end_matches`, which peels
/// repeats) so a doubled unit like "8GBGB" leaves "8GB" as the digit part and
/// is correctly rejected.
pub fn valid_memory_spec(v: &str) -> bool {
    let upper = v.to_ascii_uppercase();
    let digits = match upper.strip_suffix("GB").or_else(|| upper.strip_suffix("MB")) {
        Some(d) => d,
        None => return false,
    };
    !digits.is_empty()
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

# 0. Refuse to run without Administrator rights: the trim, the WSL shutdown and
#    the diskpart compact below all need elevation, and failing halfway (WSL
#    already down) is worse than not starting. Exit BEFORE touching anything.
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

/// The PowerShell script dropped into Downloads to expose the game's MySQL
/// port to the LAN for a remote HeidiSQL (mirrors compact_script's
/// generate-and-run-as-admin pattern). It discovers this PC's LAN IPv4 at run
/// time (the adapter with a default gateway), ensures IP Helper is running,
/// then adds a netsh v4tov4 portproxy + a Private/Domain-only firewall rule.
///
/// Two safety rails baked in per research:
///   * listenaddress is the SPECIFIC LAN IP, never 0.0.0.0 -- a wildcard
///     triggers the wslrelay infinite-loop / IPv6 failure.
///   * the firewall rule is Domain,Private only, NEVER Public -- and the
///     header shouts that this must stay on a trusted LAN, never the internet.
/// `port` is the DB host port (3306, or the remapped value from the compose
/// .env, e.g. 13306) on BOTH sides -- the WSL localhost relay makes
/// 127.0.0.1:<port> reach the container.
pub fn mysql_expose_script(port: u16) -> String {
    format!(
        r#"# DML Launcher -- expose MySQL (port {port}) to your LAN for HeidiSQL (generated)
#
#   RUN AS ADMINISTRATOR: right-click this file -> "Run with PowerShell"
#   from an elevated PowerShell, or the netsh/firewall changes will fail.
#
#   *** SECURITY ***
#   This opens your game server's DATABASE to your LOCAL NETWORK so another PC
#   on your home network can connect with HeidiSQL. ONLY run this on a network
#   you trust. NEVER port-forward this on your router or expose it to the
#   internet -- it is your whole server's data. The DML installer deliberately
#   does NOT open this port; this reverses that on purpose.
#
#   You do NOT need this on the SAME PC: HeidiSQL there connects to
#   127.0.0.1 port {port} directly.
#
#   TO UNDO later (admin PowerShell):
#     netsh interface portproxy delete v4tov4 listenaddress=<the IP shown below> listenport={port}
#     Remove-NetFirewallRule -DisplayName 'DML MySQL (HeidiSQL)'

$ErrorActionPreference = 'Stop'
Write-Host '== DML expose MySQL to LAN =='

# 0. Refuse to run without Administrator rights: the IP Helper service change,
#    the port proxy and the firewall rule below all need elevation. Without this
#    a non-admin run throws mid-script, and under the documented right-click
#    route the window closes before the error can be read. Exit BEFORE anything.
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

# 1. This PC's LAN IPv4 = the up adapter that has a default gateway.
$lanIp = (Get-NetIPConfiguration |
    Where-Object {{ $_.IPv4DefaultGateway -and $_.NetAdapter.Status -eq 'Up' }} |
    Select-Object -First 1 -ExpandProperty IPv4Address).IPAddress
if (-not $lanIp) {{ Write-Host 'ERROR: could not find a LAN IPv4 address.'; pause; exit 1 }}
Write-Host "LAN address: $lanIp"

# 2. IP Helper (iphlpsvc) must run for portproxy to work.
Set-Service -Name iphlpsvc -StartupType Automatic
Start-Service -Name iphlpsvc

# 3. Forward LAN:{port} -> 127.0.0.1:{port}. listenaddress is the SPECIFIC LAN
#    IP on purpose -- 0.0.0.0 triggers a wslrelay loop / IPv6 failure.
netsh interface portproxy add v4tov4 listenaddress=$lanIp listenport={port} connectaddress=127.0.0.1 connectport={port}

# 4. Allow it through the firewall for PRIVATE/DOMAIN networks only (never Public).
if (Get-NetFirewallRule -DisplayName 'DML MySQL (HeidiSQL)' -ErrorAction SilentlyContinue) {{
    Write-Host 'Firewall rule already exists -- leaving it.'
}} else {{
    New-NetFirewallRule -DisplayName 'DML MySQL (HeidiSQL)' -Direction Inbound -Action Allow `
        -Protocol TCP -LocalPort {port} -Profile Domain,Private | Out-Null
}}

Write-Host ''
Write-Host 'Done. From another PC on your LAN, connect HeidiSQL to:'
Write-Host "    Host: $lanIp   Port: {port}   User: root   Password: password"
Write-Host 'On THIS PC use 127.0.0.1 instead.'
pause
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte span of a generated script's `if (-not $admin) {{ … }}` block:
    /// (start, one past the closing brace). Both generated scripts open with
    /// the same guard, and the closing brace is the first one at column 0
    /// after it (the later one-line `if (-not $lanIp) {{ … }}` guards keep
    /// their brace mid-line). Used to assert what is INSIDE the branch --
    /// asserting only that `IsInRole` exists and comes first says nothing
    /// about whether the branch actually stops the script.
    fn admin_guard_span(s: &str) -> (usize, usize) {
        let start = s.find("if (-not $admin) {").expect("admin guard block present");
        let end = start + s[start..].find("\n}").expect("admin guard block is closed") + 2;
        (start, end)
    }

    fn admin_guard_block(s: &str) -> &str {
        let (start, end) = admin_guard_span(s);
        &s[start..end]
    }

    fn admin_guard_end(s: &str) -> usize {
        admin_guard_span(s).1
    }

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
    fn memory_spec_rejects_doubled_or_mixed_units() {
        // trim_end_matches used to peel both "GB"s here and accept it.
        assert!(!valid_memory_spec("8GBGB"));
        assert!(!valid_memory_spec("8MBMB"));
        assert!(!valid_memory_spec("8GBMB"));
        assert!(!valid_memory_spec("8MBGB"));
        assert!(!valid_memory_spec("GBGB"));
    }

    #[test]
    fn decode_strips_utf8_bom_and_rejects_utf16() {
        // UTF-8 BOM stripped -- the [wsl2] header must still parse.
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"[wsl2]\nmemory=8GB\n");
        let s = decode_wslconfig(&bytes).expect("utf8 BOM should decode");
        assert!(!s.starts_with('\u{feff}'));
        assert_eq!(read_wsl2_key(&s, "memory").as_deref(), Some("8GB"));

        // UTF-16 LE/BE BOMs are rejected, never silently treated as empty.
        assert!(decode_wslconfig(&[0xFF, 0xFE, 0x00, 0x00]).is_err());
        assert!(decode_wslconfig(&[0xFE, 0xFF, 0x00, 0x00]).is_err());

        // Plain UTF-8 (no BOM) passes through byte-for-byte.
        assert_eq!(
            decode_wslconfig(b"[wsl2]\nmemory=4GB\n").unwrap(),
            "[wsl2]\nmemory=4GB\n"
        );
        // Empty file = empty string (absent .wslconfig).
        assert_eq!(decode_wslconfig(b"").unwrap(), "");
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
        // Admin self-check that bails BEFORE any privileged step.
        assert!(s.contains("IsInRole"));
        assert!(s.contains("Administrator"));
        let admin_at = s.find("IsInRole").expect("admin check present");
        let fstrim_at = s.find("fstrim").expect("fstrim present");
        let shutdown_at = s.find("wsl --shutdown").expect("shutdown present");
        assert!(admin_at < fstrim_at, "admin check must precede fstrim");
        assert!(admin_at < shutdown_at, "admin check must precede wsl --shutdown");
        // ...and ABORTS rather than merely warning -- see the identical
        // assertion in `mysql_expose_script_has_the_safety_rails_and_uses_the_port`
        // for why existing + ordered is not enough.
        let guard = admin_guard_block(&s);
        assert!(
            guard.contains("exit 1"),
            "the non-admin branch must abort, not just warn: {guard:?}"
        );
        assert!(
            admin_guard_end(&s) < fstrim_at,
            "the admin guard must abort BEFORE fstrim, not somewhere after it"
        );
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

    #[test]
    fn mysql_expose_script_has_the_safety_rails_and_uses_the_port() {
        let s = mysql_expose_script(13306);
        assert!(s.contains("RUN AS ADMINISTRATOR"));
        // Admin self-check that bails BEFORE any privileged step.
        assert!(s.contains("IsInRole"));
        assert!(s.contains("Administrator"));
        let admin_at = s.find("IsInRole").expect("admin check present");
        let service_at = s.find("Set-Service").expect("Set-Service present");
        // `find`, not `rfind`: the undo header mentions the portproxy DELETE, so
        // anchor on the `add` verb to hit the real privileged call.
        let netsh_at = s
            .find("netsh interface portproxy add")
            .expect("portproxy add present");
        assert!(admin_at < service_at, "admin check must precede Set-Service");
        assert!(admin_at < netsh_at, "admin check must precede the netsh portproxy add");
        // ...and it must ABORT, not merely warn (round-2 review finding).
        // Existing + ordered is satisfied by three Write-Host lines and a
        // `pause`: drop the `exit 1` and a non-admin who follows the documented
        // right-click route presses Enter and falls straight into Set-Service,
        // which throws mid-script under `$ErrorActionPreference = 'Stop'` and
        // closes the window before the error can be read -- the exact failure
        // the guard exists to prevent. Assert the abort INSIDE the guard block
        // (a stray `exit 1` further down the script must not satisfy this),
        // and that the whole block is over before the first privileged call.
        let guard = admin_guard_block(&s);
        assert!(
            guard.contains("exit 1"),
            "the non-admin branch must abort, not just warn: {guard:?}"
        );
        let guard_end = admin_guard_end(&s);
        assert!(
            guard_end < service_at,
            "the admin guard must abort BEFORE Set-Service, not somewhere after it"
        );
        assert!(
            guard_end < netsh_at,
            "the admin guard must abort BEFORE the netsh portproxy add"
        );
        // security warnings
        assert!(s.contains("SECURITY"));
        assert!(s.contains("NEVER port-forward"));
        assert!(s.to_ascii_lowercase().contains("trust"));
        // undo instructions
        assert!(s.contains("TO UNDO"));
        assert!(s.contains("Remove-NetFirewallRule"));
        // the port appears in the portproxy + firewall lines
        assert!(s.contains("listenport=13306"));
        assert!(s.contains("connectport=13306"));
        assert!(s.contains("-LocalPort 13306"));
        // never a wildcard listen address, never a Public firewall profile
        assert!(!s.contains("listenaddress=0.0.0.0"));
        assert!(s.contains("Domain,Private"));
        assert!(!s.contains("-Profile Public"));
        // uses the WSL localhost relay as the connect target
        assert!(s.contains("connectaddress=127.0.0.1"));
    }

    #[test]
    fn mysql_expose_script_defaults_and_custom_ports_render() {
        assert!(mysql_expose_script(3306).contains("listenport=3306"));
        assert!(mysql_expose_script(23306).contains("-LocalPort 23306"));
    }
}
