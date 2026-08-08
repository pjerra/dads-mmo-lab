# Run on the SERVER PC or inside the VM. Downloads the VM-test files from the dev PC
# and verifies every SHA256 hash.
# Usage:  powershell -ExecutionPolicy Bypass -File fetch-vm-files.ps1 -From http://192.168.1.50:8712
param(
    [Parameter(Mandatory = $true)][string]$From,
    [string]$Dest = "$env:USERPROFILE\Desktop\dml-vm-test"
)
$ErrorActionPreference = 'Stop'

if ($From -notmatch '^https?://') { $From = "http://$From" }
$From = $From.TrimEnd('/')

New-Item -ItemType Directory -Force $Dest | Out-Null
Write-Host "Fetching from $From into $Dest"

$sumsPath = Join-Path $Dest 'SHA256SUMS.txt'
Invoke-WebRequest -UseBasicParsing "$From/SHA256SUMS.txt" -OutFile $sumsPath

$failed = 0
foreach ($line in Get-Content $sumsPath) {
    if ($line -notmatch '^([0-9a-f]{64}) \*(.+)$') { continue }
    $hash = $Matches[1]; $name = $Matches[2]
    if ($name -eq 'SHA256SUMS.txt') { continue }
    $out = Join-Path $Dest $name
    $url = "$From/" + [uri]::EscapeDataString($name)
    Write-Host "  downloading $name ..."
    Invoke-WebRequest -UseBasicParsing $url -OutFile $out
    $actual = (Get-FileHash $out -Algorithm SHA256).Hash.ToLower()
    if ($actual -eq $hash) {
        Write-Host "  OK   $name" -ForegroundColor Green
    } else {
        Write-Host "  FAIL $name  (hash mismatch - re-run the fetch)" -ForegroundColor Red
        $failed++
    }
}

Write-Host ''
if ($failed -gt 0) {
    Write-Host "$failed file(s) FAILED verification. Do not install from this copy." -ForegroundColor Red
    exit 1
}
Write-Host "All files verified. Installer: $Dest\DML Launcher_0.1.0_x64-setup.exe" -ForegroundColor Green
Write-Host "Test plan:  $Dest\VM-ACCEPTANCE-TEST.md  (start at Phase 0)"
