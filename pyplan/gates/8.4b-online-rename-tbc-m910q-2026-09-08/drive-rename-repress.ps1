# 8.4b re-press: drive one 2.4.3 client run in the INTERACTIVE session.
#
# This is `drive-login-tbc.ps1` with four constants changed (the script it
# starts, the task name, the runner file and the two new switches), which is the
# shape the 8.2b/8.3a READMEs say to copy rather than inventing another driver.
#
# schtasks rather than Start-Process: a process started from an ssh session
# lands in session 0, where there is no desktop, so the client draws nothing and
# SendKeys reaches nothing. The command lives in a .cmd file because /tr is
# capped at 261 characters.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [string]$Exe = 'Wow.exe',
  [string]$NextField = '{TAB}',
  [switch]$ClickFields,
  [switch]$EnterWorld,
  [switch]$RealmStyle,
  [switch]$LogoutFromChat,
  [string]$RenameTo = '',
  [int]$WorldSeconds = 60,
  [int]$LogoutSettleSeconds = 45,
  [int]$WaitSeconds = 600
)

$ErrorActionPreference = 'Stop'
$runner = "C:\Users\PK\run-rename-repress.cmd"
$line = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\PK\client-rename-repress.ps1 ' +
        "-ClientDir `"$ClientDir`" -Realmlist $Realmlist -Account $Account " +
        # DOUBLE quotes around $NextField, not single ones. Windows command-line
        # parsing knows only the double quote, so the inherited
        # `-NextField '{TAB}'` reached the script as the literal seven-character
        # string `'{TAB}'` -- SendKeys then typed an apostrophe into the account
        # box, TAB'd, and typed another into the password box. Measured
        # 2026-09-08: the login screen photographed `TBCGATE'` and a password one
        # character too long, and realmd logged nothing at all, which looks
        # exactly like a wrong password. Same family as 8.5b's unquoted `-Who`.
        "-Password `"$Password`" -Label $Label -Exe $Exe -NextField `"$NextField`"" +
        $(if ($ClickFields) { " -ClickFields" } else { "" }) +
        $(if ($EnterWorld) { " -EnterWorld -WorldSeconds $WorldSeconds" } else { "" }) +
        $(if ($LogoutFromChat) { " -LogoutFromChat -LogoutSettleSeconds $LogoutSettleSeconds" } else { "" }) +
        $(if ($RenameTo) { " -RenameTo $RenameTo" } else { "" }) +
        $(if ($RealmStyle) { " -RealmStyle" } else { "" })
Set-Content -Path $runner -Value $line -Encoding ascii

# End any run still going: `schtasks /run` on a task that is already running is
# REFUSED, silently, and the log then belongs to the older run -- which is how
# two overlapping runs produced a four-line log and no frames (2026-09-07).
# `2>$null` does NOT silence a native exe under `-ErrorAction Stop`: PowerShell
# 5.1 wraps each stderr line in a NativeCommandError and the script dies on the
# FIRST run, when the task does not exist yet. Measured 2026-09-08.
$ErrorActionPreference = 'Continue'
schtasks /end /tn yulon-rename-repress 2>&1 | Out-Null
$ErrorActionPreference = 'Stop'
Start-Sleep -Seconds 2
Get-Process Wow, WoW -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

# Delete the PREVIOUS run's log before starting, and wait for a new one. The
# wait below looks for "closed the client", and on 2026-09-08 it found that
# string in the log of the run BEFORE this one -- so the driver printed a
# complete, plausible, seven-minute transcript of a client that had been shut
# down twenty minutes earlier, three seconds after being asked to start a new
# one. An artefact left in place reads as this run's own answer.
$log = "C:\Users\PK\client-login\$Label.log"
Remove-Item -Path $log -Force -ErrorAction SilentlyContinue

schtasks /create /tn yulon-rename-repress /tr $runner /sc once /st 23:59 /ru PK /it /f | Out-Null
schtasks /run /tn yulon-rename-repress | Out-Null
"started $Label at $((Get-Date).ToUniversalTime().ToString('HH:mm:ssZ')) UTC"

$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
  if (Test-Path $log) {
    $text = Get-Content $log -Raw
    if ($text -match 'closed the client') { break }
  }
  Start-Sleep -Seconds 5
}
if (Test-Path $log) { Get-Content $log } else { "no log at $log" }
