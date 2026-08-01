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

.PARAMETER DryRun
    Report every action without performing any of them. Nothing is downloaded,
    written, or installed.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File .\Install-DML-Native.ps1 -DryRun
#>
[CmdletBinding()]
param(
    [string]$GamesDir = (Join-Path $env:USERPROFILE 'dml-native'),
    [switch]$InstallDocker,
    [switch]$DryRun
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

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

function Test-CommandExists([string]$Name) {
    $null -ne (Get-Command $Name -ErrorAction SilentlyContinue)
}

# Docker Desktop's own executable, not the `docker` CLI on PATH: the CLI can be
# present from another source (a WSL distro's, a leftover shim) while Docker
# Desktop itself is absent, and native mode needs the Desktop engine.
function Get-DockerDesktopPath {
    $candidates = @(
        (Join-Path $env:ProgramFiles 'Docker\Docker\Docker Desktop.exe'),
        (Join-Path ${env:ProgramFiles(x86)} 'Docker\Docker\Docker Desktop.exe'),
        (Join-Path $env:LOCALAPPDATA 'Docker\Docker Desktop.exe')
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
        Invoke-WebRequest -Uri $YqUrl -OutFile $tmp -UseBasicParsing
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
    Invoke-Change "add a Defender exclusion for $Path" {
        Add-MpPreference -ExclusionPath $Path -ErrorAction Stop
    } | Out-Null
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
    $existing['backend']   = 'native'
    $existing['games_dir'] = $Games
    $existing['yq_bin']    = $YqBin

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

Step 'Checking Docker Desktop'
$docker = Get-DockerDesktopPath
if ($docker) {
    Ok "found at $docker"
} elseif ($InstallDocker) {
    if (-not (Test-CommandExists 'winget')) {
        $problems.Add('Docker Desktop is missing and winget is not available to install it. Install Docker Desktop from https://www.docker.com/products/docker-desktop/ and re-run.')
        Fail 'winget not available'
    } else {
        Invoke-Change 'install Docker Desktop via winget' {
            winget install --id Docker.DockerDesktop --accept-package-agreements --accept-source-agreements
        } | Out-Null
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

Step 'Checking Git for Windows'
if (Test-CommandExists 'git') {
    Ok 'git found'
} else {
    # Native mode HARD-REQUIRES git: the install engine clones AzerothCore and
    # mod-playerbots with it, and `games list`/`games catalog` still shell bash
    # (Git Bash) today.
    Fail 'git is not installed'
    Info 'Get it from https://git-scm.com/download/win -- native mode needs it to download the server source.'
    $problems.Add('Git for Windows is not installed.')
}

Step 'Checking WebView2 (the launcher window)'
if (Test-WebView2Installed) {
    Ok 'WebView2 runtime present'
} else {
    Warn 'WebView2 runtime not detected'
    Info 'Windows 11 ships it; a freshly imaged Windows 10 may not.'
    Info 'Get the Evergreen Runtime: https://developer.microsoft.com/microsoft-edge/webview2/'
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

Step 'Writing launcher settings'
Write-LauncherConfig $LauncherConfigPath $GamesDir (Join-Path $tools 'yq.exe')
Ok "backend=native, games_dir=$GamesDir"
Info 'DML_BACKEND / DML_GAMES_DIR environment variables still win over this file.'

Say ''
if ($problems.Count -gt 0) {
    Say '  Not ready yet:' 'Yellow'
    foreach ($p in $problems) { Say "    - $p" 'Yellow' }
    Say ''
    Say '  Fix those, then run this script again.' 'Yellow'
    exit 1
}

Say '  Ready.' 'Green'
Say ''
Say '  Next: start Docker Desktop, open the DML Launcher, and install a server' 'Gray'
Say '  from the Library. The first install builds from source and takes hours.' 'Gray'
if ($DryRun) { Say '' ; Say '  (Dry run -- nothing was actually changed.)' 'Yellow' }
exit 0

