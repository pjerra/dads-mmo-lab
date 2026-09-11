# Log a real 3.3.5a client in, enter the world, and hold it there while the app
# sends its commands from the other side of the machine -- photographing at a
# fixed interval so every action lands in a frame.
#
# 8.3a's `client-login.ps1` with the holding loop added. The two facts that cost
# that box an evening are kept: run in the INTERACTIVE session (schtasks /it,
# because a process started from ssh lands in session 0 where there is no
# desktop and SendKeys reaches nothing), and write EVERY `Data\<locale>\
# realmlist.wtf` as well as `WTF\Config.wtf`, because this client reads the
# former FIRST.
#
# Every capture logs whether the client process is still alive. A client that
# has DIED photographs like a refusal -- a black or absent window is not
# evidence of anything until you know which it was.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [string]$Who = "",
  [int]$HoldSeconds = 300,
  [int]$ShotEvery = 15,
  [switch]$StopAtCharacterScreen,
  [switch]$Die,
  [string]$AnswerRenameWith = ""
)

$ErrorActionPreference = 'Stop'
$out = Join-Path $env:USERPROFILE "client-play"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$log = Join-Path $out "$Label.log"
Set-Content -Path $log -Value "" -Encoding utf8

$script:proc = $null

function Say([string]$text) {
  Add-Content -Path $log -Value ("[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $text) -Encoding utf8
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# The first run of this box photographed a login screen with an EMPTY account
# field: the keys had gone to whatever else owned the foreground (a browser
# update prompt, on this host). A client that is not focused eats nothing, and
# the frame looks exactly like a client that refused the account -- so the
# window is raised by handle before every burst, and the raise is logged.
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
}
"@

function Focus() {
  $script:proc.Refresh()
  $h = $script:proc.MainWindowHandle
  [Win32]::ShowWindow($h, 9) | Out-Null   # SW_RESTORE
  [Win32]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 700
  $now = [Win32]::GetForegroundWindow()
  Say ("raised the client window {0}; the foreground is now {1} (the same: {2})" -f $h, $now, ($now -eq $h))
}

function Alive() {
  if ($null -eq $script:proc) { return $false }
  return -not (Get-Process -Id $script:proc.Id -ErrorAction SilentlyContinue).HasExited
}

function Shoot([string]$name) {
  $live = $false
  try { $live = $null -ne (Get-Process -Id $script:proc.Id -ErrorAction SilentlyContinue) } catch {}
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $name  (the client process is alive: $live)"
}

Say "client=$ClientDir realmlist=$Realmlist account=$Account label=$Label"

$wtf = Join-Path $ClientDir "WTF\Config.wtf"
$dir = Split-Path $wtf
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
$existing = if (Test-Path $wtf) { Get-Content $wtf | Where-Object { $_ -notmatch '^\s*SET\s+realmlist' } } else { @() }
Set-Content -Path $wtf -Value (@($existing) + @("SET realmlist `"$Realmlist`"")) -Encoding ascii
Say "wrote realmlist into $wtf"
Get-ChildItem (Join-Path $ClientDir "Data") -Directory -ErrorAction SilentlyContinue |
  ForEach-Object {
    $rl = Join-Path $_.FullName "realmlist.wtf"
    Set-Content -Path $rl -Value "set realmlist $Realmlist" -Encoding ascii
    Say "wrote realmlist into $rl"
  }

Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 2
$script:proc = Start-Process -FilePath (Join-Path $ClientDir "Wow.exe") -WorkingDirectory $ClientDir -PassThru
Say "started Wow.exe as pid $($script:proc.Id)"
$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline -and $script:proc.MainWindowHandle -eq 0) {
  $script:proc.Refresh(); Start-Sleep -Milliseconds 500
}
Say "window handle $($script:proc.MainWindowHandle) after $([int]((Get-Date) - $script:proc.StartTime).TotalSeconds)s"
Start-Sleep -Seconds 14
Focus

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
Start-Sleep -Seconds 20
Shoot "$Label-1-character-screen"

if ($StopAtCharacterScreen) {
  Start-Sleep -Seconds 6
  Shoot "$Label-2-character-screen-again"
  Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
  Say "closed the client at the character screen, as asked"
  exit 0
}

# Enter World: the character screen's default button, so ENTER presses it.
Focus
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "entering the world"

if ($AnswerRenameWith -ne "") {
  # The rename prompt this box's own press raised. It appears at Enter World on
  # this client, NOT at the character screen -- the run before this one stopped
  # at the character screen and there was no prompt there. Answering it consumes
  # it, which is why it is the last thing any of these runs does.
  Start-Sleep -Seconds 8
  Shoot "$Label-2a-rename-prompt"
  [System.Windows.Forms.SendKeys]::SendWait($AnswerRenameWith)
  Start-Sleep -Milliseconds 700
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "answered the rename prompt with $AnswerRenameWith"
  Start-Sleep -Seconds 6
  Shoot "$Label-2a-rename-answered"
  Focus
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "entering the world again, after the rename"
}

Start-Sleep -Seconds 45
Shoot "$Label-2-in-world"

if ($Die) {
  # The revive clause needs a character who is actually dead, and `die <name>`
  # is NOT a command on this tree -- the console answered
  # "Command 'die Amvezuan' does not exist", because `.die` acts on a selection
  # and a SOAP session has none. So the character kills itself, from its own
  # chat line, on an account the app's own 8.3a button put at GM level 3.
  # `.die` needs a SELECTION here too -- one run of this box sent it with
  # nothing selected and the server answered "You should select a character or
  # a creature.", which photographs exactly like a character that refused to
  # die. F1 targets self, and the command then has something to act on.
  Focus
  [System.Windows.Forms.SendKeys]::SendWait("{F1}")
  Start-Sleep -Milliseconds 900
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Start-Sleep -Milliseconds 900
  [System.Windows.Forms.SendKeys]::SendWait(".die")
  Start-Sleep -Milliseconds 900
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "targeted self with F1 and sent .die from the client's own chat line"
  Start-Sleep -Seconds 8
  Shoot "$Label-2b-dead"
}

$shots = [int]($HoldSeconds / $ShotEvery)
for ($i = 1; $i -le $shots; $i++) {
  Start-Sleep -Seconds $ShotEvery
  Shoot ("{0}-hold-{1:d2}" -f $Label, $i)
}

if ($Who -ne "") {
  Focus
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Start-Sleep -Milliseconds 900
  [System.Windows.Forms.SendKeys]::SendWait("/who $Who")
  Start-Sleep -Milliseconds 900
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "asked /who $Who"
  Start-Sleep -Seconds 10
  Shoot "$Label-3-who"
}

Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Say "closed the client"
