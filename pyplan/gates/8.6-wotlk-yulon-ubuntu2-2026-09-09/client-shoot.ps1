# Click a measured point in the client (or nothing), wait, and photograph.
#
# The one tool the rest of the run uses, so that every frame is taken the same
# way and every frame records whether `Wow.exe` was alive when it was taken. A
# client that has died photographs exactly like a client showing no bot, and
# the only thing that tells them apart is this line in the log.
param(
  [int]$X = 0,
  [int]$Y = 0,
  [int]$Wait = 5,
  [Parameter(Mandatory = $true)][string]$Name,
  [string]$Out = "C:\gate86\out"
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $Out | Out-Null
$log = Join-Path $Out "shoot.log"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class S {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(int f, int x, int y, int d, int e);
}
"@
function Say([string]$t) {
  Add-Content -Path $log -Value ("[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $t) -Encoding utf8
}

$p = Get-Process Wow -ErrorAction SilentlyContinue | Select-Object -First 1
if ($null -ne $p) { [S]::SetForegroundWindow($p.MainWindowHandle) | Out-Null; Start-Sleep -Seconds 1 }

if ($X -gt 0) {
  [S]::SetCursorPos($X, $Y) | Out-Null
  Start-Sleep -Milliseconds 250
  [S]::mouse_event(0x02, 0, 0, 0, 0)
  [S]::mouse_event(0x04, 0, 0, 0, 0)
  Say "clicked $X,$Y"
}
Start-Sleep -Seconds $Wait

$p = Get-Process Wow -ErrorAction SilentlyContinue | Select-Object -First 1
$state = if ($null -eq $p) { "DEAD" } else { "alive pid=" + $p.Id }
$b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
$bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
$bmp.Save((Join-Path $Out "$Name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
$g.Dispose(); $bmp.Dispose()
Say "photographed $Name -- Wow.exe $state"
