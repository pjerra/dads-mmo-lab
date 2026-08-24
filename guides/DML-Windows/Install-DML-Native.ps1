<#
.SYNOPSIS
    Prepares a Windows PC to run Dad's MMO Lab on Docker Desktop -- no WSL
    distro, no Arch, no bash CLI.

.DESCRIPTION
    This is the NATIVE route. Install-DML.ps1 stays exactly as it is and remains
    the WSL/Arch route; the two are separate on purpose and this one is named so
    a stranger cannot confuse them.

    WHAT IT DELIBERATELY DOES NOT DO, and why each omission is the point:

      * NO `wsl --install`, no Windows-feature switching, no reboot. Docker
        Desktop asks for WSL2 itself and does it better than we can. A script
        that enables Windows features needs elevation and a reboot-and-resume
        dance, which is the single largest source of "it stopped halfway"
        reports on the WSL route.
      * NO Arch distro import, no pacman, no systemd, no docker-inside-a-distro.
        Native mode drives Docker Desktop directly.
      * NO C# tray app. Install-DML.ps1 installs one, and the launcher now has
        its own tray -- a fresh run of both left the user with TWO tray icons
        and no way to tell them apart (SHIP-LIST 4.0b). Not installing one here
        resolves that by construction rather than by patch.
      * NO embedded copy of the bash CLI. Native mode does not use it.

    Docker Desktop is DETECTED and instructed by default, never installed
    silently: it is a separate product with its own licence terms (free for
    personal use, paid above a size threshold) and pulling it onto someone's
    machine unasked is not ours to do. -InstallDocker opts in to winget.

.PARAMETER GamesDir
    Where servers are installed. Defaults to %USERPROFILE%\dml-native, which is
    what the launcher itself falls back to when nothing is configured.

.PARAMETER InstallDocker
    Install Docker Desktop via winget instead of only checking for it. Opt-in.

.PARAMETER InstallGit
    Install Git for Windows via winget instead of only checking for it. Opt-in,
    like -InstallDocker, but for a different reason: Docker Desktop is detect-only
    by DEFAULT because its licence is the user's decision to make. Git is GPL with
    no such threshold, so this switch exists purely so an unattended run on a bare
    machine can complete -- which is what the native gate tests.

.PARAMETER InstallWebView2
    Install the WebView2 Evergreen Runtime via winget instead of only checking
    for it. Opt-in like the others. Windows 11 ships it; a bare Windows 10 may not.

.PARAMETER All
    Shorthand for -InstallDocker -InstallGit -InstallWebView2: install every
    prerequisite this script knows about. This is what the portable zip's
    Setup-DML.bat passes. Enabling WSL still needs an Administrator window.

.PARAMETER LauncherDir
    Use a launcher that is ALREADY on disk (the portable zip, unpacked) instead
    of downloading one. Must contain launcher.exe. A Start-menu shortcut is
    created for it, and no installer is fetched.

.PARAMETER NoLauncher
    Skip installing the DML Launcher. It is installed BY DEFAULT, unlike Docker
    and Git: those are third-party products whose licences are the user's
    decision, while the launcher is this project. A setup script that refuses to
    install the thing it is setting up leaves the user at a dead end.

.PARAMETER LauncherTag
    Install a specific release tag (e.g. v0.1.0-rc1) instead of the newest.

.PARAMETER DryRun
    Report every action without performing any of them. Nothing is downloaded,
    written, or installed.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File .\Install-DML-Native.ps1 -DryRun

.EXAMPLE
    # A bare machine, unattended: this is the combination the native gate runs.
    powershell -ExecutionPolicy Bypass -File .\Install-DML-Native.ps1 -InstallDocker -InstallGit
#>
[CmdletBinding()]
param(
    [string]$GamesDir = (Join-Path $env:USERPROFILE 'dml-native'),
    [switch]$InstallDocker,
    [switch]$InstallGit,
    [switch]$InstallWebView2,
    [switch]$All,
    [string]$LauncherDir,
    [switch]$NoLauncher,
    [string]$LauncherTag,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

# -All is sugar, expanded HERE so every branch below keeps testing the one
# switch it is about (and the queued resume carries the expanded switches).
if ($All) {
    $InstallDocker   = $true
    $InstallGit      = $true
    $InstallWebView2 = $true
}

# --------------------------------------------------------------------------
# Pins. A download with no hash is a download you cannot trust twice.
# --------------------------------------------------------------------------

# yq is needed ONLY by the bash `dml` fallback that native mode still uses for
# `games list` / `games catalog` under Git Bash. The Rust config path needs no
# yq at all (crates/dml-wow/src/config.rs). Pinned by version AND hash: an
# unpinned "latest" download makes the install unreproducible, and a hash-less
# one makes it unverifiable.
# The hash was OBTAINED, not written from memory: downloaded on 2026-08-01,
# hashed, and the binary run to confirm it reports v4.44.3 (10,371,584 bytes).
# A pin nobody verified is worse than no pin -- it fails every install for a
# reason that looks like tampering, and the obvious "fix" is to delete the
# check. Re-verify the same way when bumping the version.
$YqVersion = 'v4.44.3'
$YqUrl     = "https://github.com/mikefarah/yq/releases/download/$YqVersion/yq_windows_amd64.exe"
$YqSha256  = 'D509D51E6DB30EBB7C9363B7CA8714224F93A456A421D7A7819AB564B868ACC7'

$LauncherConfigPath = Join-Path $env:USERPROFILE '.dml\launcher.json'

#Where the launcher itself comes from.
#
#Resolved from the GitHub API rather than a pinned URL. A pin would need editing
#on every release, and a STALE pin silently installs an old launcher onto a
#fresh machine -- worse than no pin, because nothing looks wrong.
#
#/releases/latest is deliberately NOT used: it excludes pre-releases, and the
#only release today is one.
$LauncherRepo = 'pjerra/dads-mmo-lab'
$LauncherAssetPattern = '-setup.exe'

# --------------------------------------------------------------------------
# Output helpers
# --------------------------------------------------------------------------

$script:Steps = 0
function Say([string]$m, [string]$color = 'Gray') { Write-Host $m -ForegroundColor $color }
function Step([string]$m) { $script:Steps++; Say "`n[$script:Steps] $m" 'Cyan' }
function Ok([string]$m)   { Say "    OK   $m" 'DarkGreen' }
function Info([string]$m) { Say "    ...  $m" 'Gray' }
function Warn([string]$m) { Say "    WARN $m" 'Yellow' }
function Fail([string]$m) { Say "    FAIL $m" 'Red' }

# Every side effect goes through this. That is what makes -DryRun trustworthy:
# there is one place a write can happen, so "did I miss one" is answerable by
# reading a single function rather than auditing the whole script.
function Invoke-Change([string]$What, [scriptblock]$Action) {
    if ($DryRun) {
        Say "    DRY  would $What" 'DarkYellow'
        return $false
    }
    Info $What
    & $Action
    return $true
}

# --------------------------------------------------------------------------
# Detection
# --------------------------------------------------------------------------

# Where a queued resume lives. HKCU RunOnce, ONE value, one name.
$RunOnceKey  = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\RunOnce'
$RunOnceName = 'DMLNativeSetup'

# Clear any queued resume. Called at the START of every run: a user who re-runs
# this script by hand before rebooting would otherwise leave an entry that fires
# later for no reason, and surprise execution is worse than no automation.
function Clear-QueuedResume {
    try {
        $existing = Get-ItemProperty -Path $RunOnceKey -Name $RunOnceName -ErrorAction SilentlyContinue
        if ($existing) {
            Remove-ItemProperty -Path $RunOnceKey -Name $RunOnceName -ErrorAction Stop
            Info 'Cleared a queued auto-resume from a previous run.'
        }
    } catch {
        # Never fatal. A stale entry is untidy; failing the install over it is worse.
        Warn 'Could not clear the queued auto-resume entry.'
    }
}

# Queue this script to run again after the reboot, with the SAME switches.
#
# Windows removes a RunOnce value before it executes it, so this cleans itself
# up -- no "delete the auto-run afterwards" step that could be skipped by the
# very failure it would be cleaning up after.
#
# -NoExit on purpose: an auto-started window that closes the instant it finishes
# shows the user nothing, and the whole point of resuming is to report what
# happened.
function Register-Resume {
    $psExe = Join-Path $PSHOME 'powershell.exe'
    $argList = @('-NoExit', '-ExecutionPolicy', 'Bypass', '-File', ('"{0}"' -f $PSCommandPath))
    if ($InstallDocker) { $argList += '-InstallDocker' }
    if ($InstallGit)    { $argList += '-InstallGit' }
    if ($InstallWebView2) { $argList += '-InstallWebView2' }
    if ($LauncherDir)   { $argList += @('-LauncherDir', ('"{0}"' -f $LauncherDir)) }
    # Only when it differs from the default, so the queued command stays as
    # close to what the user actually typed as possible.
    if ($GamesDir -ne (Join-Path $env:USERPROFILE 'dml-native')) {
        $argList += @('-GamesDir', ('"{0}"' -f $GamesDir))
    }
    $cmd = '"{0}" {1}' -f $psExe, ($argList -join ' ')
    try {
        if (-not (Test-Path $RunOnceKey)) { New-Item -Path $RunOnceKey -Force | Out-Null }
        New-ItemProperty -Path $RunOnceKey -Name $RunOnceName -Value $cmd `
                         -PropertyType String -Force -ErrorAction Stop | Out-Null
        Ok 'This script will run again automatically after you restart.'
        Info 'It removes that entry itself -- Windows deletes a RunOnce value before running it.'
        return $true
    } catch {
        Warn 'Could not queue the auto-resume; run this script again yourself after the restart.'
        return $false
    }
}

# Seconds before the automatic restart. Long enough to notice and reach the
# keyboard on a machine that may still be scrolling installer output.
$RestartCountdownSeconds = 60

# Count down, then restart. A keypress restarts NOW; otherwise the restart
# happens when the countdown ends. (User decision 2026-08-25: the previous
# version treated a keypress as CANCEL and refused to restart at all where no
# keypress could be read -- which is every SSH / scheduled run -- so the one
# step the whole setup hinges on was the one step it would not take.)
#
# Where a keypress cannot be read (redirected stdin, non-interactive host) the
# countdown simply runs to the end and restarts; the user was told, in the
# summary above, that this is coming.
function Invoke-RestartCountdown {
    $canRead = $false
    try {
        if ([Environment]::UserInteractive) {
            $null = [Console]::KeyAvailable   # throws on hosts without a real console
            $canRead = $true
        }
    } catch {
        $canRead = $false
    }

    # Drain anything already buffered, so a stray keystroke typed minutes ago
    # does not instantly restart a machine the user was still reading.
    if ($canRead) { while ([Console]::KeyAvailable) { $null = [Console]::ReadKey($true) } }

    Say ''
    $left = $RestartCountdownSeconds
    while ($left -gt 0) {
        if ($canRead) {
            Write-Host ("`r  Restarting in {0,3}s -- press any key to restart now. " -f $left) `
                       -NoNewline -ForegroundColor Yellow
            if ([Console]::KeyAvailable) {
                $null = [Console]::ReadKey($true)
                break
            }
        } else {
            Write-Host ("`r  Restarting in {0,3}s. " -f $left) -NoNewline -ForegroundColor Yellow
        }
        Start-Sleep -Seconds 1
        $left--
    }
    Write-Host ''
    Say '  Restarting now...' 'Yellow'
    Restart-Computer -Force
    return $true
}

#Find the newest release asset, or $null if we cannot.
function Resolve-LauncherAsset([string]$Tag) {
    $api = if ($Tag) {
        "https://api.github.com/repos/$LauncherRepo/releases/tags/$Tag"
    } else {
        "https://api.github.com/repos/$LauncherRepo/releases"
    }
    try {
        $resp = Invoke-RestMethod -Uri $api -UseBasicParsing -Headers @{ 'User-Agent' = 'dml-installer' }
    } catch {
        return $null
    }
    #The list endpoint returns newest-first; the tag endpoint returns one object.
    $rel = if ($Tag) { $resp } else { @($resp)[0] }
    if (-not $rel) { return $null }
    $asset = @($rel.assets) | Where-Object { $_.name -like "*$LauncherAssetPattern" } | Select-Object -First 1
    if (-not $asset) { return $null }
    [pscustomobject]@{
        Tag  = $rel.tag_name
        Name = $asset.name
        Url  = $asset.browser_download_url
        Size = $asset.size
    }
}

# A Start-menu shortcut for a portable launcher, so "open it from the Start
# menu" is true for the zip route as well as the installer route.
function New-LauncherShortcut([string]$ExePath) {
    $programs = [Environment]::GetFolderPath('Programs')
    $lnk = Join-Path $programs 'DML Launcher.lnk'
    Invoke-Change "create Start-menu shortcut $lnk" {
        $shell = New-Object -ComObject WScript.Shell
        $sc = $shell.CreateShortcut($lnk)
        $sc.TargetPath = $ExePath
        $sc.WorkingDirectory = Split-Path -Parent $ExePath
        $sc.Description = 'DML Launcher'
        $sc.Save()
    } | Out-Null
}

#Is the launcher already on this machine? Checked before downloading 7 MB.
function Get-InstalledLauncher {
    foreach ($root in @($env:LOCALAPPDATA, ${env:ProgramFiles}, ${env:ProgramFiles(x86)})) {
        if (-not $root) { continue }
        $dir = Join-Path $root 'DML Launcher'
        if (Test-Path -LiteralPath $dir) {
            $exe = Get-ChildItem -LiteralPath $dir -Filter *.exe -ErrorAction SilentlyContinue |
                   Select-Object -First 1
            if ($exe) { return $exe.FullName }
        }
    }
    return $null
}

function Test-IsElevated {
    ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
}

# Is WSL actually usable? THREE answers, never two.
#
# $true  - wsl reported its status cleanly.
# $false - wsl.exe is present (it ships with Windows) but the feature is not
#          installed, which is what Docker Desktop reports as "WSL not
#          installed".
# $null  - we could not tell. wsl.exe absent entirely, or the call blew up.
#          Treated as "say nothing", never as "not installed": claiming a
#          missing feature on a machine that has one sends the user to enable
#          something twice and reboot for no reason.
function Get-WslState {
    if (-not (Test-CommandExists 'wsl')) { return $null }
    try {
        # --status is read-only and works unelevated. Output is discarded; the
        # exit code is the answer.
        $null = & wsl.exe --status 2>&1
        return ($LASTEXITCODE -eq 0)
    } catch {
        return $null
    }
}

function Test-CommandExists([string]$Name) {
    $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

# Docker Desktop's own executable, not the `docker` CLI on PATH: the CLI can be
# present from another source (a WSL distro's, a leftover shim) while Docker
# Desktop itself is absent, and native mode needs the Desktop engine.
function Get-DockerDesktopPath {
    # THESE MUST MATCH crates/dml-core/src/engine.rs's own candidate list. They
    # did not: this probed %LOCALAPPDATA%\Docker\Docker Desktop.exe, which is
    # Docker's DATA folder (log, run, wsl, backend.lock) and never holds the
    # exe. The per-user install lives under %LOCALAPPDATA%\Programs\DockerDesktop.
    #
    # Verified on the author's own machine, where the launcher's path exists and
    # this one does not -- so the installer told a user with Docker Desktop
    # installed that Docker Desktop was missing, on every run, permanently.
    $candidates = @(
        (Join-Path $env:ProgramFiles 'Docker\Docker\Docker Desktop.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Docker\Docker\Docker Desktop.exe'),
        (Join-Path $env:LOCALAPPDATA 'Programs\DockerDesktop\Docker Desktop.exe')
    )
    foreach ($c in $candidates) {
        if ($c -and (Test-Path -LiteralPath $c)) { return $c }
    }
    return $null
}

# WebView2 is what the launcher's window is made of. Windows 11 ships it, but a
# freshly imaged Windows 10 may not -- found live on a test VM (SHIP-LIST 4.0).
# The runtime registers under both the per-machine and per-user hives.
function Test-WebView2Installed {
    $keys = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )
    foreach ($k in $keys) {
        try {
            $v = (Get-ItemProperty -Path $k -Name pv -ErrorAction Stop).pv
            if ($v -and $v -ne '0.0.0.0') { return $true }
        } catch { }
    }
    return $false
}

# --------------------------------------------------------------------------
# Steps
# --------------------------------------------------------------------------

# How to read winget's exit code. Interpretation only: the `winget install`
# call itself deliberately stays inline in each opt-in branch, because
# Test-InstallerNative requires it to sit lexically inside the body of an
# -Install* clause and a call hidden in a helper would satisfy that guard only
# indirectly. See this file's git history for the attempt that went red.
#
# 0 is success. -1978335189 (0x8A15002B) is winget's "no applicable
# upgrade / already installed", which is not a failure for our purposes.
function Test-WingetOk([int]$Code, [string]$Label) {
    if ($Code -eq 0) { Ok "$Label installed"; return $true }
    if ($Code -eq -1978335189) { Ok "$Label was already installed"; return $true }
    Fail "$Label did not install (winget exit $Code)"
    return $false
}

function Install-YqPinned([string]$ToolsDir) {
    $target = Join-Path $ToolsDir 'yq.exe'
    if (Test-Path -LiteralPath $target) {
        $have = (Get-FileHash -LiteralPath $target -Algorithm SHA256).Hash
        if ($have -eq $YqSha256) { Ok "yq $YqVersion already present and verified"; return }
        Warn "yq.exe present but its hash does not match the pin -- replacing it"
    }
    $tmp = "$target.download"
    $done = Invoke-Change "download yq $YqVersion" {
        New-Item -ItemType Directory -Force -Path $ToolsDir | Out-Null
        # Set explicitly rather than assumed: a caller who set
        # ProgressPreference to SilentlyContinue (common in CI wrappers) would
        # otherwise get a silent minute with no way to tell a slow link from a
        # hung one. Restored in `finally` so we do not change the caller's shell.
        $prev = $ProgressPreference
        $ProgressPreference = 'Continue'
        try {
            Invoke-WebRequest -Uri $YqUrl -OutFile $tmp -UseBasicParsing
        } finally {
            $ProgressPreference = $prev
        }
        if (Test-Path -LiteralPath $tmp) {
            Say ("    downloaded {0:N1} MB" -f ((Get-Item -LiteralPath $tmp).Length / 1MB)) 'DarkGray'
        }
    }
    if (-not $done) { return }

    # VERIFY BEFORE INSTALLING, and delete on mismatch. A hash checked after the
    # file is already in place leaves a bad binary sitting at the path
    # everything else reads.
    $hash = (Get-FileHash -LiteralPath $tmp -Algorithm SHA256).Hash
    if ($hash -ne $YqSha256) {
        Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
        throw "Downloaded yq does not match the pinned SHA256 (got $hash, expected $YqSha256). Nothing was installed."
    }
    Move-Item -LiteralPath $tmp -Destination $target -Force
    Ok "yq $YqVersion installed and hash-verified"
}

# Directory-scoped, never process-scoped: excluding a compiler BINARY would
# leave it unscanned everywhere on the machine, which is a far bigger hole than
# skipping one build tree. Same shape as Install-DML.ps1's own machinery.
function Add-NativeDefenderExclusions([string]$Path) {
    if (-not (Test-CommandExists 'Add-MpPreference')) {
        Info 'Defender cmdlets not available -- skipping exclusions'
        return
    }
    # NEVER FATAL. Add-MpPreference needs elevation, and this script is designed
    # to run WITHOUT it -- its own DESCRIPTION says so. Under
    # $ErrorActionPreference='Stop' an unhandled throw here killed the whole run
    # at step 5, so the two steps that actually matter (yq, and writing
    # launcher.json) never happened and the "Not ready yet" summary never
    # printed. An OPTIONAL performance tweak was ending the install.
    #
    # The guard above tests whether the cmdlet EXISTS, which is a different
    # question from whether we may call it.
    try {
        Invoke-Change "add a Defender exclusion for $Path" {
            Add-MpPreference -ExclusionPath $Path -ErrorAction Stop
        } | Out-Null
    } catch {
        Warn 'Could not add the Defender exclusion (this usually needs Administrator).'
        Info 'Not fatal: the install works without it, the first build is just slower.'
        Info 'To speed it up, re-run this script as Administrator, or exclude the games folder in Windows Security by hand.'
    }
}

# Merge, never overwrite. The file also carries close_to_tray and
# start_with_windows, which the user sets from the app -- clobbering the whole
# document to write two keys would silently reset their preferences on every
# re-run of this installer.
function Write-LauncherConfig([string]$Path, [string]$Games, [string]$YqBin) {
    $existing = [ordered]@{}
    if (Test-Path -LiteralPath $Path) {
        try {
            $json = Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
            foreach ($p in $json.PSObject.Properties) { $existing[$p.Name] = $p.Value }
        } catch {
            Warn "Existing launcher.json could not be read -- writing a fresh one"
        }
    }
    # CAMEL CASE, because crates/dml-core/src/launcher_config.rs is
    # #[serde(rename_all = "camelCase")] with NO serde(alias). snake_case keys
    # are silently DROPPED -- not an error, just ignored -- so only `backend`
    # survived and the games dir the user asked for went nowhere.
    #
    # It hid because the default -GamesDir happens to equal what startup.rs
    # reconstructs on its own. With -GamesDir D:\dml the build lands on C:, the
    # Defender exclusion protects a folder nothing writes to, DML_YQ_BIN points
    # at a file that does not exist, and the script prints the D: path as
    # success.
    $existing['backend']   = 'native'
    $existing['gamesDir']  = $Games
    $existing['yqBin']     = $YqBin

    Invoke-Change "write $Path" {
        $dir = Split-Path -Parent $Path
        New-Item -ItemType Directory -Force -Path $dir | Out-Null
        # UTF8 without BOM: serde_json rejects a leading BOM, so a BOM here
        # would make the launcher fall back to defaults and silently ignore
        # everything this installer just configured.
        $text = ($existing | ConvertTo-Json -Depth 5)
        [System.IO.File]::WriteAllText($Path, $text, (New-Object System.Text.UTF8Encoding($false)))
    } | Out-Null
}

# --------------------------------------------------------------------------
# Main
# --------------------------------------------------------------------------

Say ''
Say '  Dad''s MMO Lab -- native (Docker Desktop) setup' 'White'
Say '  No WSL distro, no Arch, no tray app.' 'DarkGray'
if ($DryRun) { Say '  DRY RUN -- nothing will be changed.' 'Yellow' }

$problems = New-Object System.Collections.Generic.List[string]

# Initialised UP FRONT because Set-StrictMode -Version Latest makes reading an
# unassigned variable a hard error, and the Docker step now READS
# RebootRequired to decide whether to defer -- a read that happens before any
# assignment on the happy path where WSL was already present.
$script:RebootRequired = $false
$script:ResumeQueued   = $false

# Before anything: a queued resume from a previous run is now redundant -- we
# ARE the run it was queued for, or the user started one by hand.
if (-not $DryRun) { Clear-QueuedResume }

Step 'Checking CPU virtualization'
# A script CAN enable Windows features (wsl --install turns on both WSL and
# VirtualMachinePlatform). It CANNOT enable VT-x / AMD-V: that lives in the
# firmware and needs a human in the BIOS. It is also the single most common
# reason Docker Desktop will not start on a real machine, so naming it here --
# in one sentence, before anything long-running -- beats letting it surface as
# a Docker error three layers down.
#
# BOTH signals are needed and neither is sufficient alone. HypervisorPresent is
# true when something is ALREADY virtualizing, which proves the firmware bit is
# on; but VirtualizationFirmwareEnabled reports FALSE once Hyper-V has claimed
# the CPU, so reading that one by itself would tell a perfectly working machine
# its BIOS setting is off.
$virtOk = $null
try {
    $cs = Get-CimInstance Win32_ComputerSystem -ErrorAction Stop
    if ($cs.HypervisorPresent) {
        $virtOk = $true
    } else {
        $cpu = @(Get-CimInstance Win32_Processor -ErrorAction Stop)[0]
        $virtOk = [bool]$cpu.VirtualizationFirmwareEnabled
    }
} catch {
    # Could not ask. Say nothing rather than guess -- a false "virtualization is
    # off" sends someone into their BIOS for no reason.
    $virtOk = $null
}
if ($virtOk -eq $true) {
    Ok 'CPU virtualization is enabled'
} elseif ($null -eq $virtOk) {
    Info 'Could not read the CPU virtualization state; Docker Desktop will report it if it is off.'
} else {
    Fail 'CPU virtualization (VT-x / AMD-V) is disabled in firmware'
    Info 'No script can turn this on -- it is a BIOS/UEFI setting. Reboot into firmware setup and'
    Info 'enable Intel VT-x / AMD-V (sometimes "SVM Mode" or "Virtualization Technology"), then re-run.'
    Info 'Docker Desktop cannot run without it, so nothing further here would work.'
    $problems.Add('CPU virtualization is disabled in BIOS/UEFI.')
}

Step 'Checking WSL2 (Docker Desktop''s engine)'
$wsl = Get-WslState
# WSL is "installed" only when BOTH halves are there: the two Windows features
# (Microsoft-Windows-Subsystem-Linux + VirtualMachinePlatform) AND the WSL
# package itself. `wsl --status` answers 0 only for the second, so it is the
# one probe that covers both.
#
# WHY NOT `wsl --install --no-distribution` (2026-08-25, proven on a clean
# Win11 26200 VM): when the WSL package is absent, C:\Windows\System32\wsl.exe
# is an inbox STUB that rejects EVERY flag -- --no-distribution, --update,
# --version, --inbox -- with "WSL is not installed. You can install by running
# 'wsl.exe --install'", exit 1. The only thing it accepts is a bare --install,
# which also downloads Ubuntu. And this script used to treat that exit 1 as
# "nothing to say" and print Ready. So the package comes from winget
# (Microsoft.WSL, the same MSI the Store ships) and the features from DISM,
# neither of which depends on the stub's mood.
if ($wsl -eq $true) {
    Ok 'WSL is installed'
} elseif ($InstallDocker -and (Test-IsElevated)) {
    # Could not tell, or genuinely missing -- either way we are about to
    # install something that RUNS on it, and every step below is idempotent,
    # so trying settles the question in the one way a probe could not.
    if ($null -eq $wsl) { Info 'Could not read the WSL state; setting it up anyway (harmless if already present).' }
    $script:WslFeatureRestart = $false
    $script:WslFeatureOk = $true
    Invoke-Change 'enable the WSL and Virtual Machine Platform Windows features' {
        foreach ($f in @('Microsoft-Windows-Subsystem-Linux', 'VirtualMachinePlatform')) {
            try {
                $r = Enable-WindowsOptionalFeature -Online -FeatureName $f -All -NoRestart -ErrorAction Stop
                if ($r.RestartNeeded) { $script:WslFeatureRestart = $true }
                Ok "$f enabled"
            } catch {
                Fail "$f could not be enabled: $($_.Exception.Message)"
                $script:WslFeatureOk = $false
            }
        }
    } | Out-Null
    if (-not (Test-CommandExists 'winget')) {
        $problems.Add('WSL is missing and winget is not available to install it. Run "wsl --install" as Administrator, reboot, and re-run this script.')
        Fail 'winget not available'
    } else {
        Invoke-Change 'install the WSL package via winget (Microsoft.WSL)' {
            Say '    WSL is ~200 MB. winget shows its own progress below.' 'DarkGray'
            $proc = Start-Process -FilePath 'winget' -NoNewWindow -Wait -PassThru `
                -ArgumentList @('install', '--id', 'Microsoft.WSL',
                                '--accept-package-agreements', '--accept-source-agreements')
            $script:WslWingetOk = Test-WingetOk $proc.ExitCode 'WSL'
        } | Out-Null
        if (-not $DryRun) {
            if ($script:WslWingetOk -and $script:WslFeatureOk) {
                # A REBOOT IS REQUIRED when a feature was just switched on, so
                # this run must not end in "Ready." A machine that needs
                # restarting is not ready, and a script that says otherwise
                # sends the user to open a launcher that cannot work.
                if ($script:WslFeatureRestart) {
                    $script:RebootRequired = $true
                    # Queue ourselves so the user does not have to remember.
                    $script:ResumeQueued = Register-Resume
                    if ($script:ResumeQueued) {
                        $problems.Add('WSL was just enabled -- RESTART; this script continues by itself afterwards.')
                    } else {
                        $problems.Add('WSL was just enabled -- REBOOT, then run this script again.')
                    }
                } else {
                    # Features were already on; the package alone needs no reboot.
                    # Prove it rather than assume it.
                    if ((Get-WslState) -eq $true) { Ok 'WSL is installed' }
                    else {
                        Fail 'WSL still does not report as installed'
                        $problems.Add('WSL did not come up after install; reboot and re-run this script.')
                    }
                }
            } else {
                Fail 'WSL could not be set up'
                $problems.Add('WSL could not be set up automatically; run "wsl --install" as Administrator, reboot, and re-run this script.')
            }
        }
    }
} elseif ($InstallDocker) {
    # We can install Docker but cannot enable the feature it needs. This
    # script is designed to run WITHOUT Administrator, so a missing privilege
    # is REPORTED with the exact command, never a crash and never a silent skip.
    Fail 'WSL is not installed, and enabling it needs Administrator'
    Info 'Run this script again from an ADMIN PowerShell (or use Setup-DML.bat, which asks for it).'
    $problems.Add('WSL is not installed; re-run this script as Administrator.')
} elseif ($null -eq $wsl) {
    # Could not tell, and nothing here depends on the answer yet.
    Info 'Could not determine whether WSL is installed; Docker Desktop will say so on first run.'
} else {
    # We did NOT install Docker, so its own interactive setup will enable
    # WSL -- which is why this script deliberately does not switch Windows
    # features on the default path.
    Warn 'WSL is not installed -- Docker Desktop needs it for its engine.'
    Info 'Docker Desktop enables it during its own setup; run Docker Desktop once and let it finish.'
    Info 'Or re-run this script as Administrator with -InstallDocker (or -All) to set it up here.'
    $problems.Add('WSL is not installed (Docker Desktop cannot run without it).')
}

Step 'Checking Docker Desktop'
# Deliberately AFTER virtualization and WSL. Docker Desktop runs on WSL2, so
# installing it before its platform exists is backwards -- and it puts the one
# unfixable check (VT-x in firmware) behind a ~600 MB download, so a user whose
# BIOS setting is off pays for the download to be told so.
#
# A pending reboot also means the platform is not ready YET: installing Docker
# now would run its setup against half-enabled WSL. The resumed run does it.
if ($script:RebootRequired) {
    Info 'Skipping the Docker step until after the restart -- WSL is not active yet.'
} else {
$docker = Get-DockerDesktopPath
if ($docker) {
    Ok "found at $docker"
} elseif ($InstallDocker) {
    if (-not (Test-CommandExists 'winget')) {
        $problems.Add('Docker Desktop is missing and winget is not available to install it. Install Docker Desktop from https://www.docker.com/products/docker-desktop/ and re-run.')
        Fail 'winget not available'
    } else {
        Invoke-Change 'install Docker Desktop via winget' {
            Say '    Docker Desktop is ~600 MB, and the install itself takes 5-15 minutes' 'DarkGray'
            Say '    in a VM. winget shows its own progress below.' 'DarkGray'
            # Start-Process, NOT a pipeline. Two reasons, and the second is
            # measured rather than theoretical:
            #
            # 1. -NoNewWindow lets the child inherit THIS console, so winget
            #    renders its own progress natively. (The bug this replaces was
            #    `| Out-Null` swallowing it entirely.)
            # 2. `winget ... | Out-Host` aborted with E_ABORT on a real VM while
            #    the identical command run bare on the same machine succeeded.
            #    One sample, so not proof -- but the pipeline was the only
            #    difference, and there is nothing to gain by keeping it.
            $proc = Start-Process -FilePath 'winget' -NoNewWindow -Wait -PassThru `
                -ArgumentList @('install', '--id', 'Docker.DockerDesktop',
                                '--accept-package-agreements', '--accept-source-agreements')
            $script:DockerWingetOk = Test-WingetOk $proc.ExitCode 'Docker Desktop'
        } | Out-Null
        if (-not $DryRun -and -not $script:DockerWingetOk) {
            $problems.Add('Docker Desktop failed to install via winget.')
        }
    }
} else {
    # Instruct, do not install. Docker Desktop's licence is free for personal
    # use and paid above a size threshold; that is the user's decision, and a
    # script that installs it silently makes it for them.
    Fail 'Docker Desktop is not installed'
    Info 'Get it from https://www.docker.com/products/docker-desktop/ (free for personal use).'
    Info 'It sets up WSL2 itself -- this script does not touch Windows features.'
    Info 'Or re-run this script with -InstallDocker to install it via winget.'
    $problems.Add('Docker Desktop is not installed.')
}

}

Step 'Checking Git for Windows'
if (Test-CommandExists 'git') {
    Ok 'git found'
} elseif ($InstallGit) {
    if (-not (Test-CommandExists 'winget')) {
        $problems.Add('Git is missing and winget is not available to install it. Install Git for Windows from https://git-scm.com/download/win and re-run.')
        Fail 'winget not available'
    } else {
        Invoke-Change 'install Git for Windows via winget' {
            Say '    Git for Windows is ~60 MB. winget shows its own progress below.' 'DarkGray'
            $proc = Start-Process -FilePath 'winget' -NoNewWindow -Wait -PassThru `
                -ArgumentList @('install', '--id', 'Git.Git',
                                '--accept-package-agreements', '--accept-source-agreements')
            $script:GitWingetOk = Test-WingetOk $proc.ExitCode 'Git for Windows'
        } | Out-Null
        if (-not $DryRun -and -not $script:GitWingetOk) {
            $problems.Add('Git for Windows failed to install via winget.')
        }
        # winget writes the MACHINE PATH; this process keeps the environment it
        # was started with. Without this refresh the very next check still says
        # "git is not installed" -- a successful install that reports as a
        # failure, which is worse than not installing at all.
        if (-not $DryRun) {
            $env:Path = [Environment]::GetEnvironmentVariable('Path', 'Machine') + ';' +
                        [Environment]::GetEnvironmentVariable('Path', 'User')
            if (Test-CommandExists 'git') {
                Ok 'git installed and on PATH'
            } else {
                # Do NOT claim success we cannot see. Some winget packages need a
                # new shell before their shims resolve.
                Info 'Git was installed but is not on this shell''s PATH yet -- open a new PowerShell and re-run this script to confirm.'
                $problems.Add('Git installed, but not visible on PATH in this session.')
            }
        }
    }
} else {
    # Native mode HARD-REQUIRES git: the install engine clones AzerothCore and
    # mod-playerbots with it, and `games list`/`games catalog` still shell bash
    # (Git Bash) today.
    Fail 'git is not installed'
    Info 'Get it from https://git-scm.com/download/win -- native mode needs it to download the server source.'
    Info 'Or re-run this script with -InstallGit to install it via winget.'
    $problems.Add('Git for Windows is not installed.')
}

Step 'Checking WebView2 (the launcher window)'
if (Test-WebView2Installed) {
    Ok 'WebView2 runtime present'
} elseif ($InstallWebView2) {
    if (-not (Test-CommandExists 'winget')) {
        $problems.Add('WebView2 is missing and winget is not available to install it. Get the Evergreen Runtime from https://developer.microsoft.com/microsoft-edge/webview2/ and re-run.')
        Fail 'winget not available'
    } else {
        Invoke-Change 'install the WebView2 runtime via winget' {
            Say '    WebView2 Evergreen Runtime: a small bootstrapper, then ~150 MB.' 'DarkGray'
            $proc = Start-Process -FilePath 'winget' -NoNewWindow -Wait -PassThru `
                -ArgumentList @('install', '--id', 'Microsoft.EdgeWebView2Runtime',
                                '--accept-package-agreements', '--accept-source-agreements')
            $script:WebViewWingetOk = Test-WingetOk $proc.ExitCode 'WebView2 runtime'
        } | Out-Null
        if (-not $DryRun -and -not $script:WebViewWingetOk) {
            $problems.Add('The WebView2 runtime failed to install via winget.')
        }
    }
} else {
    Warn 'WebView2 runtime not detected'
    Info 'Windows 11 ships it; a freshly imaged Windows 10 may not.'
    Info 'Get the Evergreen Runtime: https://developer.microsoft.com/microsoft-edge/webview2/'
    Info 'Or re-run this script with -InstallWebView2 to install it via winget.'
    # Deliberately NOT fatal: the detection reads registry keys that a managed
    # or unusual install may not populate, and refusing on a maybe would block
    # a machine that works.
}

Step "Preparing the games folder ($GamesDir)"
$tools = Join-Path $GamesDir 'tools'
Invoke-Change "create $GamesDir and $tools" {
    New-Item -ItemType Directory -Force -Path $tools | Out-Null
} | Out-Null

Step 'Defender exclusion for the build folder'
# BEFORE any build runs, which is the whole point of doing it here: a
# multi-hour C++ build writes hundreds of thousands of object files, and
# real-time scanning each one is the difference between hours and many hours.
Add-NativeDefenderExclusions $GamesDir

Step 'Installing yq (pinned)'
try {
    Install-YqPinned $tools
} catch {
    Fail $_.Exception.Message
    $problems.Add('yq could not be installed.')
}

Step 'Installing the DML Launcher'
if ($NoLauncher) {
    Info 'Skipped (-NoLauncher). Get it from https://github.com/pjerra/dads-mmo-lab/releases'
} else {
    $portable = if ($LauncherDir) { Join-Path $LauncherDir 'launcher.exe' } else { $null }
    $have = Get-InstalledLauncher
    if ($portable -and (Test-Path -LiteralPath $portable)) {
        # The portable zip: the launcher is already unpacked next to this
        # script, so there is nothing to download -- just make it findable.
        Ok "using the portable launcher at $portable"
        New-LauncherShortcut $portable
    } elseif ($portable) {
        Fail "-LauncherDir was given but $portable does not exist"
        $problems.Add("No launcher.exe in $LauncherDir. Unpack the whole zip, then run Setup-DML.bat from inside it.")
    } elseif ($have) {
        Ok "already installed at $have"
    } else {
        $asset = Resolve-LauncherAsset $LauncherTag
        if (-not $asset) {
            #Never fatal. Everything else on this machine is now correct, and a
            #user who can read one URL is not blocked -- whereas failing the
            #whole run over a download would throw away a working setup.
            Warn 'Could not find a launcher release to download.'
            Info 'Get it by hand: https://github.com/pjerra/dads-mmo-lab/releases'
            $problems.Add('The DML Launcher was not installed (release lookup failed).')
        } else {
            $tmp = Join-Path $env:TEMP $asset.Name
            $done = Invoke-Change "download and install the DML Launcher ($($asset.Tag))" {
                Say ("    {0} -- {1:N1} MB" -f $asset.Name, ($asset.Size / 1MB)) 'DarkGray'
                $prev = $ProgressPreference
                $ProgressPreference = 'Continue'
                try {
                    Invoke-WebRequest -Uri $asset.Url -OutFile $tmp -UseBasicParsing
                } finally {
                    $ProgressPreference = $prev
                }
                #/S is NSIS silent. The whole point of doing this here is that
                #the user never sees a wizard to click through.
                Say '    installing silently...' 'DarkGray'
                $p = Start-Process -FilePath $tmp -ArgumentList '/S' -Wait -PassThru
                $script:LauncherExit = $p.ExitCode
            }
            if ($done) {
                #VERIFY. A silent installer that fails silently is the worst
                #combination there is, so the exe has to be on disk before this
                #claims anything.
                $now = Get-InstalledLauncher
                if ($now) {
                    Ok "installed at $now"
                } else {
                    Fail "the launcher installer exited $script:LauncherExit but no launcher is on disk"
                    Info 'Install it by hand: https://github.com/pjerra/dads-mmo-lab/releases'
                    $problems.Add('The DML Launcher did not install.')
                }
                Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
            }
        }
    }
}

Step 'Writing launcher settings'
Write-LauncherConfig $LauncherConfigPath $GamesDir (Join-Path $tools 'yq.exe')
Ok "backend=native, games_dir=$GamesDir"
Info 'DML_BACKEND / DML_GAMES_DIR environment variables still win over this file.'

Say ''
if ($problems.Count -gt 0) {
    Say '  Not ready yet:' 'Yellow'
    foreach ($p in $problems) { Say "    - $p" 'Yellow' }
    Say ''
    if ($script:RebootRequired) {
        # Stated separately and last, because it is an ACTION rather than a
        # complaint, and because "fix those and re-run" reads as optional
        # advice next to a restart that is not.
        if ($script:ResumeQueued) {
            Say '  This PC will RESTART. The script then continues automatically.' 'Yellow'
        } else {
            Say '  This PC will RESTART. Run this script again afterwards.' 'Yellow'
        }
        # NEVER under -DryRun. The test harness runs this script with -DryRun on
        # a developer's machine; without this guard the suite would hang for a
        # minute and then reboot the box running it.
        if (-not $DryRun) {
            $null = Invoke-RestartCountdown
        }
    } else {
        Say '  Fix those, then run this script again.' 'Yellow'
    }
    exit 1
}

Say '  Ready.' 'Green'
Say ''
# DELIBERATELY does not say "start Docker Desktop". The launcher starts the
# engine itself before it composes anything (dml_wow::native::ensure_engine_up_stream,
# called first by both `games start` and the native install), so telling the user
# to do it by hand describes a step the product does not need -- and it is the
# same wrong advice that used to occupy a whole first-run screen until it was
# removed on 2026-08-03.
Say '  Next: open the DML Launcher and install a server from the Library.' 'Gray'
Say '  It starts Docker Desktop itself if the engine is down -- you do not' 'DarkGray'
Say '  need to start it first. The first install builds from source and' 'DarkGray'
Say '  takes hours.' 'DarkGray'
Say ''
#The launcher is now installed by this script, so the old "go and find it
#yourself" paragraph is gone. It only speaks up when the user opted out.
if ($NoLauncher) {
    Say '  You skipped the launcher (-NoLauncher). Get it from:' 'Yellow'
    Say '    https://github.com/pjerra/dads-mmo-lab/releases' 'Yellow'
} else {
    Say '  The DML Launcher is installed -- open it from the Start menu.' 'Gray'
    if ($LauncherDir) { Say "  (portable: it runs from $LauncherDir -- keep that folder)" 'DarkGray' }
}
if ($DryRun) { Say '' ; Say '  (Dry run -- nothing was actually changed.)' 'Yellow' }
exit 0


