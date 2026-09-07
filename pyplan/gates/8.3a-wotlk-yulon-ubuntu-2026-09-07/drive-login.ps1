# Run one client login in the INTERACTIVE session and wait for it to finish.
#
# schtasks rather than Start-Process: a process started from an ssh session
# lands in session 0, where there is no desktop, so the client draws nothing and
# SendKeys reaches nothing (`windows-gate-box-recipes`). The command lives in a
# .cmd file because /tr is capped at 261 characters.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [int]$WaitSeconds = 150
)

$ErrorActionPreference = 'Stop'
$runner = "C:\Users\PK\run-login.cmd"
$line = 'powershell -NoProfile -ExecutionPolicy Bypass -File C:\Users\PK\client-login.ps1 ' +
        "-ClientDir `"$ClientDir`" -Realmlist $Realmlist -Account $Account " +
        "-Password `"$Password`" -Label $Label"
Set-Content -Path $runner -Value $line -Encoding ascii

schtasks /create /tn yulon-login /tr $runner /sc once /st 23:59 /ru PK /it /f | Out-Null
schtasks /run /tn yulon-login | Out-Null
"started $Label"

$log = "C:\Users\PK\client-login\$Label.log"
$deadline = (Get-Date).AddSeconds($WaitSeconds)
while ((Get-Date) -lt $deadline) {
  if (Test-Path $log) {
    $text = Get-Content $log -Raw
    if ($text -match 'closed the client') { break }
  }
  Start-Sleep -Seconds 5
}
if (Test-Path $log) { Get-Content $log } else { "no log at $log" }
