# Run one `t26client.ps1` stage in the INTERACTIVE session and wait for it.
#
# `drive-login.ps1` (8.4a) is the shape: a scheduled task with `/it` and
# `/ru PK`, because a process started from ssh lands in session 0 where there is
# no desktop and no window to send a keystroke to. The difference is that this
# one waits for the stage's own `stage.done` marker rather than for a sentence
# in a log, and prints the tail of the client log so the caller sees what the
# stage did without a second round trip.
# The argument line arrives BASE64-ENCODED. Measured at 02:00: a stage argument
# carrying spaces cannot be quoted through bash -> ssh -> Windows sshd ->
# PowerShell and survive -- the quotes were stripped somewhere on the way and
# this script was handed six arguments instead of one. Base64 has no quoting.
param(
  [Parameter(Mandatory = $true)][string]$B64,
  [int]$WaitSeconds = 240,
  [int]$Tail = 30
)
$ArgLine = [Text.Encoding]::UTF8.GetString([Convert]::FromBase64String($B64))

$ErrorActionPreference = 'Stop'
$Work = 'C:\Users\PK\t26'
New-Item -ItemType Directory -Force -Path $Work | Out-Null
$done = Join-Path $Work 'stage.done'
$log = Join-Path $Work 't26client.log'
$runner = Join-Path $Work 'run.cmd'

Remove-Item $done -ErrorAction SilentlyContinue
$before = if (Test-Path $log) { (Get-Content $log).Count } else { 0 }

Set-Content -Path $runner -Value @(
  '@echo off',
  "powershell -NoProfile -ExecutionPolicy Bypass -File $Work\t26client.ps1 $ArgLine"
) -Encoding ascii

schtasks /create /tn T26Client /tr $runner /sc once /st 23:59 /ru PK /it /f | Out-Null
schtasks /run /tn T26Client | Out-Null

$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
  if (Test-Path $done) { break }
  Start-Sleep -Seconds 2
}
if (-not (Test-Path $done)) { "STAGE DID NOT FINISH within $WaitSeconds s" }

if (Test-Path $log) {
  $lines = Get-Content $log
  $new = if ($lines.Count -gt $before) { $lines[$before..($lines.Count - 1)] } else { @() }
  $new | Select-Object -Last $Tail
}
