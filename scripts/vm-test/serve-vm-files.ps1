# Run on the DEV PC (this machine). Stages the VM-test files and serves them over LAN.
# Usage:  powershell -ExecutionPolicy Bypass -File scripts\vm-test\serve-vm-files.ps1
#         (add -StageOnly to stage + hash without starting the server)
param(
    [int]$Port = 8712,
    [switch]$StageOnly
)
$ErrorActionPreference = 'Stop'

$RepoRoot = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$Drop = Join-Path $RepoRoot 'target\vm-test-drop'

$Files = @(
    (Join-Path $RepoRoot 'target\release\bundle\nsis\DML Launcher_0.1.0_x64-setup.exe'),
    (Join-Path $RepoRoot 'target\release\bundle\msi\DML Launcher_0.1.0_x64_en-US.msi'),
    (Join-Path $RepoRoot 'docs\VM-ACCEPTANCE-TEST.md'),
    (Join-Path $PSScriptRoot 'fetch-vm-files.ps1')
)

foreach ($f in $Files) {
    if (-not (Test-Path $f)) { throw "Missing: $f  (rebuild with 'npm run tauri build'?)" }
}

# Freshness guard: refuse to serve a stale installer without saying so.
$exe = Get-Item $Files[0]
$ageDays = ((Get-Date) - $exe.LastWriteTime).TotalDays
if ($ageDays -gt 2) {
    Write-Warning "Installer is $([math]::Round($ageDays,1)) days old ($($exe.LastWriteTime)). Is this the build you meant to test?"
}

if (Test-Path $Drop) { Remove-Item -Recurse -Force $Drop }
New-Item -ItemType Directory -Force $Drop | Out-Null
foreach ($f in $Files) { Copy-Item $f $Drop }

# SHA256SUMS.txt: "<hash> *<filename>" per line (fetch script verifies against this)
$sums = foreach ($f in Get-ChildItem $Drop -File) {
    $h = (Get-FileHash $f.FullName -Algorithm SHA256).Hash.ToLower()
    "$h *$($f.Name)"
}
$sums | Out-File (Join-Path $Drop 'SHA256SUMS.txt') -Encoding ascii

Write-Host ''
Write-Host 'Staged:' -ForegroundColor Green
Get-ChildItem $Drop -File | ForEach-Object { Write-Host ("  {0,10:N0}  {1}  {2}" -f $_.Length, $_.LastWriteTime, $_.Name) }

$ips = Get-NetIPAddress -AddressFamily IPv4 |
    Where-Object { $_.IPAddress -notlike '127.*' -and $_.IPAddress -notlike '169.254.*' } |
    Select-Object -ExpandProperty IPAddress

Write-Host ''
Write-Host 'On the SERVER PC (or inside the VM), run ONE of these:' -ForegroundColor Cyan
foreach ($ip in $ips) {
    Write-Host "  iwr http://${ip}:$Port/fetch-vm-files.ps1 -OutFile fetch.ps1; powershell -ExecutionPolicy Bypass -File fetch.ps1 -From http://${ip}:$Port"
}
Write-Host ''
Write-Host 'Pick the IP on the network the server PC shares (Tailscale 100.x works too if both are on it).'
Write-Host 'If Windows Firewall prompts when the server starts, click Allow.'

if ($StageOnly) { Write-Host 'StageOnly: not starting the server.'; exit 0 }

Write-Host ''
& node (Join-Path $PSScriptRoot 'server.js') $Port $Drop
