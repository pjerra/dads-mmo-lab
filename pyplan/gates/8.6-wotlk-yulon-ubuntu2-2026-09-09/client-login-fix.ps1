# Drive the already-running client to the world by CLICKING the fields.
#
# Why this exists, recorded because it cost a frame: the first run typed the
# account with SendKeys straight after the window appeared, and the keystrokes
# landed with the password box focused. The client sat on the login screen and
# the script -- which counts seconds, not states -- photographed it and called
# the file `1-in-world-no-party`. That frame was a lie of exactly the shape
# 8.2d shipped, and it was deleted rather than explained.
#
# So this one clicks each field at a measured point, reads the result back with
# a screenshot after every step, and never names a frame for a state it has not
# photographed.
param(
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Out
)

$ErrorActionPreference = 'Stop'
$log = Join-Path $Out "login-fix.log"
Add-Content -Path $log -Value ("[{0}] start" -f (Get-Date -Format 'HH:mm:ss')) -Encoding utf8

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(int f, int x, int y, int d, int e);
}
"@

function Say([string]$t) {
  Add-Content -Path $log -Value ("[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $t) -Encoding utf8
}
function Shoot([string]$name) {
  $p = Get-Process Wow -ErrorAction SilentlyContinue
  $state = if ($null -eq $p) { "DEAD" } else { "alive pid=" + ($p | Select-Object -First 1).Id }
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $Out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $name -- Wow.exe $state"
}
function Click([int]$x, [int]$y) {
  [W]::SetCursorPos($x, $y) | Out-Null
  Start-Sleep -Milliseconds 250
  [W]::mouse_event(0x02, 0, 0, 0, 0)
  [W]::mouse_event(0x04, 0, 0, 0, 0)
  Start-Sleep -Milliseconds 350
}

$proc = Get-Process Wow -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -eq $proc) { Say "no client running"; exit 1 }
[W]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Seconds 2
Say "focused pid $($proc.Id)"

# A previous attempt's "The information you have entered is not valid" dialog
# sits ON TOP of the account box, so the first click would land on the dialog
# and the typing would go nowhere. Dismiss it first; clicking Okay when there
# is no dialog is a click on the login background and does nothing.
Click 958 592
Start-Sleep -Milliseconds 500

# Measured off `0-characters.png` at 1920x1080: the account box, the password
# box and the Login button of the 3.3.5a login screen in this window position.
Click 960 549
[System.Windows.Forms.SendKeys]::SendWait("^a")
[System.Windows.Forms.SendKeys]::SendWait("{BACKSPACE}")
[System.Windows.Forms.SendKeys]::SendWait($Account)
Start-Sleep -Milliseconds 500
Click 960 620
[System.Windows.Forms.SendKeys]::SendWait("^a")
[System.Windows.Forms.SendKeys]::SendWait("{BACKSPACE}")
[System.Windows.Forms.SendKeys]::SendWait($Password)
Start-Sleep -Milliseconds 500
Shoot "login-filled"
Click 958 720
Say "pressed Login"
Start-Sleep -Seconds 20
Shoot "login-after"
