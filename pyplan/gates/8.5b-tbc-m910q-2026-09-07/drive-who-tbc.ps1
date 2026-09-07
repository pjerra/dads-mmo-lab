# Run one 8.5b who-list session in the INTERACTIVE session and wait for it.
#
# 8.4b's `drive-login-tbc.ps1` with `-Who` added. schtasks rather than
# Start-Process: a process started from an ssh session lands in session 0, where
# there is no desktop, so the client draws nothing and SendKeys reaches nothing
# (`windows-gate-box-recipes`). The command lives in a .cmd file because /tr is
# capped at 261 characters.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [Parameter(Mandatory = $true)][string]$Who,
  [string]$RenameTo = '',
  [int]$WorldSeconds = 30,
  [int]$WaitSeconds = 330
)

$ErrorActionPreference = 'Stop'
$runner = "C:\Users\PK\run-who-tbc.cmd"
$line = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\PK\client-who-tbc.ps1 ' +
        "-ClientDir `"$ClientDir`" -Realmlist $Realmlist -Account $Account " +
        "-Password `"$Password`" -Label $Label -ClickFields -EnterWorld " +
        "-WorldSeconds $WorldSeconds -RealmStyle -Who $Who" +
        $(if ($RenameTo) { " -RenameTo $RenameTo" } else { "" })
Set-Content -Path $runner -Value $line -Encoding ascii

# End any run still going: `schtasks /run` on a task that is already running is
# REFUSED, silently, and the log then belongs to the older run -- which is how
# two overlapping runs produced a four-line log and no frames (8.4b, 2026-09-07).
# `schtasks /end` on a task that does not exist yet writes to stderr, and under
# `ErrorActionPreference = Stop` PowerShell 5.1 turns a native command's stderr
# into a terminating NativeCommandError -- so the first run of this driver died
# before it started anything (measured here, 2026-09-07).
cmd /c "schtasks /end /tn yulon-who-tbc >nul 2>&1"
Start-Sleep -Seconds 2
Get-Process Wow, WoW -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

schtasks /create /tn yulon-who-tbc /tr $runner /sc once /st 23:59 /ru PK /it /f | Out-Null
schtasks /run /tn yulon-who-tbc | Out-Null
"started $Label"

$log = "C:\Users\PK\client-login\$Label.log"
$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
  if (Test-Path $log) {
    if ((Get-Content $log -Raw) -match 'closed the client') { break }
  }
  Start-Sleep -Seconds 5
}
if (Test-Path $log) { Get-Content $log } else { "no log at $log" }
