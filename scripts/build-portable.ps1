<#
.SYNOPSIS
    Build the PORTABLE DML Launcher zip: launcher.exe + its payload + setup
    scripts. No NSIS, no MSI -- users unpack and double-click.

.DESCRIPTION
    1. `npx tauri build --no-bundle` in launcher/ (frontend + release exe).
    2. Stage target\release\portable\DML Launcher\ with:
         launcher.exe
         cli\dml, cli\lua\party\*, cli\lua\gm\*      (the bash CLI + Eluna bridges)
         installers\install-*.sh                     (the six title installers)
         Setup-DML.bat, Setup-DML.ps1                (prerequisites: WSL2, Docker
                                                      Desktop, Git, WebView2)
         README.txt, LICENSE-AGPL, DISCLAIMER.md
       The payload layout mirrors bundle.resources in tauri.conf.json, which is
       what the exe looks for next to itself (launcher/src-tauri/src/payload.rs).
    3. Zip it to target\release\DML-Launcher-<version>-portable-x64.zip.

.PARAMETER SkipBuild
    Reuse the existing target\release\launcher.exe instead of rebuilding.

.EXAMPLE
    powershell -ExecutionPolicy Bypass -File scripts\build-portable.ps1
#>
[CmdletBinding()]
param([switch]$SkipBuild)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root   = Split-Path -Parent $PSScriptRoot
$conf   = Get-Content -LiteralPath (Join-Path $root 'launcher\src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
$ver    = $conf.version
$exe    = Join-Path $root 'target\release\launcher.exe'
$stage  = Join-Path $root 'target\release\portable\DML Launcher'
$zip    = Join-Path $root "target\release\DML-Launcher-$ver-portable-x64.zip"

if (-not $SkipBuild) {
    Push-Location (Join-Path $root 'launcher')
    try {
        if (-not (Test-Path 'node_modules')) {
            npm ci
            if ($LASTEXITCODE -ne 0) { throw "npm ci failed ($LASTEXITCODE)" }
        }
        npx tauri build --no-bundle
        if ($LASTEXITCODE -ne 0) { throw "tauri build failed ($LASTEXITCODE)" }
    } finally { Pop-Location }
}
if (-not (Test-Path -LiteralPath $exe)) { throw "no exe at $exe" }

if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

# Payload: every entry of bundle.resources, placed at its target path.
foreach ($p in $conf.bundle.resources.PSObject.Properties) {
    $src = Join-Path (Join-Path $root 'launcher\src-tauri') $p.Name
    $dst = Join-Path $stage ($p.Value -replace '/', '\')
    if (Test-Path -LiteralPath $src -PathType Container) {
        New-Item -ItemType Directory -Force -Path $dst | Out-Null
        Copy-Item -Path (Join-Path $src '*') -Destination $dst -Recurse -Force
    } else {
        New-Item -ItemType Directory -Force -Path (Split-Path -Parent $dst) | Out-Null
        Copy-Item -LiteralPath $src -Destination $dst -Force
    }
}

Copy-Item -LiteralPath $exe -Destination (Join-Path $stage 'launcher.exe')
Copy-Item -LiteralPath (Join-Path $root 'guides\DML-Windows\Install-DML-Native.ps1') -Destination (Join-Path $stage 'Setup-DML.ps1')
Copy-Item -LiteralPath (Join-Path $root 'guides\DML-Windows\Setup-DML.bat')          -Destination (Join-Path $stage 'Setup-DML.bat')
Copy-Item -LiteralPath (Join-Path $root 'LICENSE-AGPL')  -Destination $stage
Copy-Item -LiteralPath (Join-Path $root 'DISCLAIMER.md') -Destination $stage

$readme = @"
DML Launcher $ver (portable)
============================

1. Keep this whole folder together (launcher.exe needs the cli\ and
   installers\ folders next to it).
2. First time only: double-click Setup-DML.bat. It asks for Administrator and
   installs what the launcher needs -- WSL2, Docker Desktop, Git for Windows,
   WebView2. If it enables WSL2 it will ask to restart, then finish by itself.
3. Double-click launcher.exe. Library -> install "WoW WotLK Playerbots".
   The first install builds the server from source and takes hours.
4. Point your 3.3.5a client's realmlist.wtf at 127.0.0.1 and log in.

Unsigned: SmartScreen may warn -> More info -> Run anyway.
Docs + source: https://github.com/pjerra/dads-mmo-lab (branch release/dml-launcher)
"@
[System.IO.File]::WriteAllText((Join-Path $stage 'README.txt'), $readme, (New-Object System.Text.UTF8Encoding($false)))

# Verify the payload the exe will look for is really there.
$must = @('launcher.exe', 'cli\dml', 'cli\lua\party', 'cli\lua\gm',
          'installers\install-wow-wotlk.sh', 'Setup-DML.bat', 'Setup-DML.ps1')
foreach ($m in $must) {
    if (-not (Test-Path -LiteralPath (Join-Path $stage $m))) { throw "staging is missing $m" }
}

if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal
$mb = [Math]::Round((Get-Item -LiteralPath $zip).Length / 1MB, 1)
Write-Host "OK  $zip  ($mb MB)" -ForegroundColor Green
