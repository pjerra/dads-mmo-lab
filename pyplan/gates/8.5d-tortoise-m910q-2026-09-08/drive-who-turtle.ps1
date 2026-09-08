# Run one 8.5d Turtle-client session in the INTERACTIVE session and wait for it.
#
# RUN THREE TIMES on 2026-09-08 (09:43, 09:47, 09:56Z) and clause (4) is proved.
# It deleted its own scheduled task every time -- `schtasks /query` found no
# `yulon-who-turtle` after any of the three.
#
# 8.5b's `drive-who-tbc.ps1` with this tree's parameters, plus the `finally`
# that deletes the task. Use `-Account TORTGATE -Password 'T0RT-G@TE12'`:
# GATE83D has no character, and TORTGATE is the only account on this server that
# owns one. **Its character is now `Dortagate`, not `Dorta`** -- 8.4d renamed it
# in the client, through the app's own button, to gate that box's rename clause.
#
# The three that mattered, as typed:
#   -Label t84d-dry -DryRealm                          (learn the realm flow)
#   -Label t84d-r2 -EnterWorld -RenameTo Dortagate `
#     -LeaveChannels "World" -Who "Sietta,Caterinny,Gwenora" -WorldSeconds 30
#
# schtasks rather than Start-Process: a process started from an ssh session
# lands in session 0, where there is no desktop, so the client draws nothing and
# SendKeys reaches nothing (`windows-gate-box-recipes`). The command lives in a
# .cmd file because schtasks `/tr` is capped at 261 characters.
param(
  [string]$ClientDir = 'C:\clients\TurtleWoW',
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [string]$Who = '',
  # Comma-separated, forwarded verbatim: the World channel drowns a /who answer
  # on this server (500 bots), so a run that wants to READ one leaves it first.
  [string]$LeaveChannels = '',
  [string]$RenameTo = '',
  [string]$CharacterName = '',
  [switch]$EnterWorld,
  [switch]$RealmStyle,
  [switch]$DryRealm,
  [switch]$CreateCharacter,
  [int]$WorldSeconds = 60,
  # Long enough for the whole sequence: intro + login + realm + world + one
  # /who per name with a 35 s gap. Raise it rather than cut a run short -- a
  # truncated log is the artefact that reads as a refusal.
  [int]$WaitSeconds = 600
)

$ErrorActionPreference = 'Stop'
$runner = "C:\Users\PK\run-who-turtle.cmd"
$line = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\PK\client-who-turtle.ps1 ' +
        "-ClientDir `"$ClientDir`" -Realmlist $Realmlist -Account $Account " +
        "-Password `"$Password`" -Label $Label -WorldSeconds $WorldSeconds" +
        $(if ($EnterWorld)      { " -EnterWorld" }               else { "" }) +
        $(if ($RealmStyle)      { " -RealmStyle" }               else { "" }) +
        $(if ($DryRealm)        { " -DryRealm" }                 else { "" }) +
        $(if ($CreateCharacter) { " -CreateCharacter" }          else { "" }) +
        $(if ($CharacterName)   { " -CharacterName $CharacterName" } else { "" }) +
        $(if ($RenameTo)        { " -RenameTo $RenameTo" }       else { "" }) +
        $(if ($Who)             { " -Who $Who" }                 else { "" }) +
        $(if ($LeaveChannels)   { " -LeaveChannels $LeaveChannels" } else { "" })
Set-Content -Path $runner -Value $line -Encoding ascii
"the command that will run: $line"

# End any run still going: `schtasks /run` on a task that is already running is
# REFUSED, silently, and the log then belongs to the older run -- which is how
# two overlapping runs produced a four-line log and no frames (8.4b).
# `schtasks /end` on a task that does not exist writes to stderr, and under
# `ErrorActionPreference = Stop` PowerShell 5.1 turns a native command's stderr
# into a terminating NativeCommandError -- so it goes through cmd with its
# output swallowed.
cmd /c "schtasks /end /tn yulon-who-turtle >nul 2>&1"
Start-Sleep -Seconds 2
# The Turtle game process is WoW.exe; the launcher and installer are stopped
# too, because either of them holding the folder is a run that types into a
# wizard (8.3d).
Get-Process WoW, Wow, 'turtle-wow', 'TurtleWoW' -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1

# `/st` is a time in the PAST as often as not -- the box shows PST while this
# lane's clock is CEST -- so a `once` task that is left behind FIRES AGAIN at
# its own 23:59. Three of them did exactly that on 2026-09-07 and stomped a
# live session. The task is therefore deleted at the end of this script, in a
# `finally`, so an exception or a timeout still takes it away.
schtasks /create /tn yulon-who-turtle /tr $runner /sc once /st 23:59 /ru PK /it /f | Out-Null
try {
schtasks /run /tn yulon-who-turtle | Out-Null
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
}
finally {
  # /end first: `schtasks /delete` on a RUNNING task leaves the process behind.
  # Both go through cmd with their output swallowed, because under
  # `ErrorActionPreference = Stop` PowerShell 5.1 turns a native command's
  # stderr into a terminating NativeCommandError, and both of these write to
  # stderr when the task is already gone.
  cmd /c "schtasks /end /tn yulon-who-turtle >nul 2>&1"
  cmd /c "schtasks /delete /tn yulon-who-turtle /f >nul 2>&1"
  "deleted the scheduled task yulon-who-turtle"
}
