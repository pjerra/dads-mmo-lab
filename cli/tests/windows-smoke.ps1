# Smoke-tests the Windows->WSL->dml --json path the DML Launcher will use.
$ErrorActionPreference = 'Stop'
& "$PSScriptRoot\..\dev-install.ps1"

$raw = wsl -d dml-arch -u dml -- dml version --json
$v = ($raw | ConvertFrom-Json)
if (-not $v.ok) { throw "version --json not ok: $raw" }
if ($v.data.version -ne '3.0.0') { throw "unexpected version: $($v.data.version)" }

$raw = wsl -d dml-arch -u dml -- dml games list --json
$g = ($raw | ConvertFrom-Json)
if (-not $g.ok) { throw "games list --json not ok: $raw" }
Write-Host "SMOKE OK — $($g.data.games.Count) game(s):" ($g.data.games.id -join ', ')