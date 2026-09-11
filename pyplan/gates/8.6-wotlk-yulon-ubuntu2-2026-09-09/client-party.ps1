# 8.6 part 2 -- a real 3.3.5a client, logged in and left in the world, photographed
# on demand while the app presses My Party from the other side.
#
# Runs on the Hyper-V HOST (`vmhost`), where the client lives. Started through
# `schtasks /it` for 8.3a's reason: a process started from ssh lands in session 0,
# which has no desktop, so `Wow.exe` draws nothing and SendKeys reaches nobody.
#
# It is a WAITER, not a one-shot. The party frame only means anything beside a
# press that happened while this client was in the world, so the script logs in,
# photographs the ground, and then waits for a file to appear for each later
# frame. Every capture records whether `Wow.exe` is still alive: a client that
# has DIED photographs exactly like a client refusing to show a bot.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Out,
  [int]$WorldSeconds = 45,
  [int]$WaitMinutes = 25
)

$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$log = Join-Path $Out "client.log"
Set-Content -Path $log -Value "" -Encoding utf8

function Say([string]$text) {
  Add-Content -Path $log -Value ("[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $text) -Encoding utf8
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

function Alive() {
  $p = Get-Process Wow -ErrorAction SilentlyContinue
  if ($null -eq $p) { return "DEAD" }
  return ("alive pid=" + ($p | Select-Object -First 1).Id)
}

function Shoot([string]$name) {
  # The liveness is read BEFORE the frame and written beside it: a dead client
  # and a client showing no bot are the same picture.
  $state = Alive
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $Out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $name -- Wow.exe $state"
}

function WaitFor([string]$flag) {
  $path = Join-Path $Out $flag
  $deadline = (Get-Date).AddMinutes($WaitMinutes)
  Say "waiting for $flag"
  while ((Get-Date) -lt $deadline) {
    if (Test-Path $path) { Say "saw $flag"; return $true }
    Start-Sleep -Seconds 2
  }
  Say "gave up waiting for $flag"
  return $false
}

# THREE files, not one, and this cost an hour on 2026-09-09. `Config.wtf`'s
# `SET realmlist` is what 8.5a's `client-who.ps1` writes and it is NOT the file
# a 3.3.5a client obeys: `Data\<locale>\realmlist.wtf` wins over it. This
# client's held `172.30.55.155`, a VM that no longer exists, so every login
# attempt went to an address nothing answers -- the client sat on "Connecting"
# and then said "Unable to connect", and the auth server's own
# `account.failed_logins` and `last_login` were UNTOUCHED, which is how the
# difference between "wrong password" and "never arrived" was read.
$wtf = Join-Path $ClientDir "WTF\Config.wtf"
$existing = if (Test-Path $wtf) { Get-Content $wtf | Where-Object { $_ -notmatch '^\s*SET\s+realmlist' } } else { @() }
Set-Content -Path $wtf -Value (@($existing) + @("SET realmlist `"$Realmlist`"")) -Encoding ascii
foreach ($rl in @((Join-Path $ClientDir "realmlist.wtf"), (Join-Path $ClientDir "Data\enUS\realmlist.wtf"))) {
  if (Test-Path $rl) {
    Set-Content -Path $rl -Value "set realmlist $Realmlist" -Encoding ascii
    Say "rewrote $rl"
  }
}
Say "realmlist -> $Realmlist"

Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$proc = Start-Process -FilePath (Join-Path $ClientDir "Wow.exe") -WorkingDirectory $ClientDir -PassThru
Say "started pid $($proc.Id)"
$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline -and $proc.MainWindowHandle -eq 0) { $proc.Refresh(); Start-Sleep -Milliseconds 500 }
Start-Sleep -Seconds 12

[System.Windows.Forms.SendKeys]::SendWait("^a")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Account)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("{TAB}")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Password)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "sent the login"
Start-Sleep -Seconds 18
Shoot "0-characters"

[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "entering the world"
Start-Sleep -Seconds $WorldSeconds
# The GROUND frame: in the world, no party. Named for what it is rather than
# "before", because 8.2d shipped a frame called before that showed the after.
Shoot "1-in-world-no-party"
Set-Content -Path (Join-Path $Out "ready.flag") -Value "in world" -Encoding ascii

if (WaitFor "shoot-added.flag") { Start-Sleep -Seconds 3; Shoot "2-party-frame-with-bot" }
Set-Content -Path (Join-Path $Out "added.done") -Value "shot" -Encoding ascii

if (WaitFor "shoot-dismissed.flag") { Start-Sleep -Seconds 3; Shoot "3-party-frame-after-dismiss" }
Set-Content -Path (Join-Path $Out "dismissed.done") -Value "shot" -Encoding ascii

if (WaitFor "shoot-absent.flag") { Start-Sleep -Seconds 3; Shoot "4-bridge-absent-still-in-world" }
Set-Content -Path (Join-Path $Out "absent.done") -Value "shot" -Encoding ascii

Say "final state -- Wow.exe $(Alive)"
if (WaitFor "quit.flag") { Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force; Say "closed the client" }
