# Run one 8.5d Turtle-client session in the INTERACTIVE session and wait for it.
#
# NOT YET RUN. 8.5b's `drive-who-tbc.ps1` with this tree's parameters.
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
        $(if ($Who)             { " -Who $Who" }                 else { "" })
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

schtasks /create /tn yulon-who-turtle /tr $runner /sc once /st 23:59 /ru PK /it /f | Out-Null
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
