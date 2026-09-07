# Drive the owner's own client, on the owner's own laptop, through one login.
#
# No schtasks here: this session already runs in the interactive desktop
# session, which is the thing `schtasks /it` exists to reach on the gate box.
#
# `-Create` clicks through character creation when the account has no character
# yet, which a freshly installed server never does. The two clicks are computed
# from the client window's own rectangle rather than from screen constants, so
# they survive the window being anywhere.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [string]$CharacterName = "",
  [switch]$RealmStep,
  [int]$WorldSeconds = 75,
  [int]$StaySeconds = 45
)

$ErrorActionPreference = 'Stop'
$out = Join-Path $env:USERPROFILE "yulon-client"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$log = Join-Path $out "$Label.log"
Set-Content -Path $log -Value "" -Encoding utf8
function Say([string]$t) {
  $line = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $t
  Add-Content -Path $log -Value $line -Encoding utf8
  Write-Output $line
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, int e);
}
"@

function Shoot([string]$name) {
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $name"
}

function ClickIn([IntPtr]$h, [double]$fx, [double]$fy) {
  $r = New-Object Win+RECT
  [void][Win]::GetWindowRect($h, [ref]$r)
  $x = [int]($r.L + ($r.R - $r.L) * $fx)
  $y = [int]($r.T + ($r.B - $r.T) * $fy)
  [System.Windows.Forms.Cursor]::Position = New-Object System.Drawing.Point($x, $y)
  Start-Sleep -Milliseconds 250
  [Win]::mouse_event(0x0002, 0, 0, 0, 0)   # left down
  Start-Sleep -Milliseconds 80
  [Win]::mouse_event(0x0004, 0, 0, 0, 0)   # left up
  Say "clicked at ($x,$y) = ($fx,$fy) of the client window"
  Start-Sleep -Milliseconds 600
}

# Where the realmlist lives is a per-CLIENT fact. The 1.12-era clients Turtle
# ships read `realmlist.wtf` at the client root; the 3.3.5a client reads
# `WTF\Config.wtf`. Both are written when both exist, because writing the wrong
# one is a gate that "cannot reach the server" for a reason nothing reports.
$targets = @()
$root = Join-Path $ClientDir "realmlist.wtf"
if (Test-Path $root) { $targets += ,@($root, "set realmlist $Realmlist") }
$cfg = Join-Path $ClientDir "WTF\Config.wtf"
if (Test-Path $cfg) { $targets += ,@($cfg, "SET realmlist `"$Realmlist`"") }
if ($targets.Count -eq 0) {
  New-Item -ItemType Directory -Force -Path (Split-Path $cfg) | Out-Null
  $targets += ,@($cfg, "SET realmlist `"$Realmlist`"")
}
foreach ($t in $targets) {
  $path = $t[0]
  $keep = if (Test-Path $path) { Get-Content $path | Where-Object { $_ -notmatch '(?i)^\s*set\s+realmlist' } } else { @() }
  Set-Content -Path $path -Value (@($keep) + @($t[1])) -Encoding ascii
  Say "realmlist -> $Realmlist in $path"
}

Get-Process Wow, WoW -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$exe = Join-Path $ClientDir "WoW.exe"
$proc = Start-Process -FilePath $exe -WorkingDirectory $ClientDir -PassThru
Say "started $exe as pid $($proc.Id)"
$deadline = (Get-Date).AddSeconds(120)
while ((Get-Date) -lt $deadline) {
  $proc.Refresh()
  if ($proc.HasExited) { Say "the client exited early"; exit 1 }
  if ($proc.MainWindowHandle -ne 0) { break }
  Start-Sleep -Milliseconds 500
}
$h = $proc.MainWindowHandle
Say "window handle $h"
[void][Win]::SetForegroundWindow($h)
Start-Sleep -Seconds 14
Shoot "$Label-1-login"

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
Shoot "$Label-2-after-login"

if ($RealmStep) {
  # This fork shows a realm list after the login that the 3.3.5a client does
  # not; ENTER takes the highlighted realm.
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "chose the realm"
  Start-Sleep -Seconds 10
  Shoot "$Label-2b-characters"
}

if ($CharacterName) {
  # "Create New Character" sits on the right-hand panel, low; the name box is
  # centre-low on the creation screen and Accept is bottom-right.
  ClickIn $h 0.86 0.80
  Start-Sleep -Seconds 4
  Shoot "$Label-3-creation"
  ClickIn $h 0.50 0.86
  [System.Windows.Forms.SendKeys]::SendWait($CharacterName)
  Start-Sleep -Milliseconds 600
  Shoot "$Label-4-named"
  ClickIn $h 0.86 0.93
  Start-Sleep -Seconds 6
  Shoot "$Label-5-created"
}

[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "entering the world"
Start-Sleep -Seconds $WorldSeconds
Shoot "$Label-6-in-world"
Say "in the world; staying $StaySeconds seconds"
Start-Sleep -Seconds $StaySeconds
Shoot "$Label-7-before-leaving"

Get-Process Wow, WoW -ErrorAction SilentlyContinue | Stop-Process -Force
Say "closed the client"
