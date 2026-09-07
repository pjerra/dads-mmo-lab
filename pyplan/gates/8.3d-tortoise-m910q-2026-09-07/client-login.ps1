# Drive a real WoW client through one login attempt, and photograph what it says.
#
# Runs in the INTERACTIVE session (schtasks /it), because a client with no
# desktop draws nothing and SendKeys reaches nothing.
#
# Every step writes a line to the log, so a run that goes wrong says where.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [int]$SettleSeconds = 22,
  [int]$IntroSeconds = 10,
  # The Tortoise client is `turtle-wow.exe`; the two Blizzard clients are
  # `Wow.exe`. The process name follows the file, so both come from here.
  [string]$Exe = 'Wow.exe',
  # How to get from the account box to the password box. Measured per
  # client: {TAB} moves on the 2.4.3 and 1.12.1 clients, and on the
  # 1.18.1 Turtle client it does NOT -- there the password landed in the
  # account box and realmd logged `WHERE username = 'OLD-P@SS12'`.
  [string]$NextField = '{TAB}',
  # Click the two boxes instead of tabbing between them. The 1.18.1
  # Turtle client ignored both {TAB} and {ENTER}: realmd logged the
  # PASSWORD as the username on one run and a name left over from an
  # earlier session on the next, so the keystrokes were not reaching the
  # account box at all. The fractions are of the client area, read off
  # the login screen this client draws.
  [switch]$ClickFields,
  [double]$AccountY = 0.514,
  [double]$PasswordY = 0.624,
  [double]$FieldX = 0.518
)

$ErrorActionPreference = 'Stop'
$out = Join-Path $env:USERPROFILE "client-login"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$log = Join-Path $out "$Label.log"

function Say([string]$text) {
  $line = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ssZ'), $text
  Add-Content -Path $log -Value $line -Encoding utf8
}

Set-Content -Path $log -Value "" -Encoding utf8
Say "client=$ClientDir realmlist=$Realmlist account=$Account label=$Label"

# 1. The realmlist, in every place THIS client might read it. 2.4.3 keeps its
#    own `realmlist.wtf` beside Wow.exe, the locale folder can hold another,
#    and WTF\Config.wtf is where the 3.3.5a client kept it. Writing one and
#    guessing wrong is how the 8.1d capture ended up photographing a refusal
#    by a completely different server, so all of them are written and each is
#    logged.
$targets = @(
  (Join-Path $ClientDir "realmlist.wtf"),
  (Join-Path $ClientDir "WTF\Config.wtf")
) + @(Get-ChildItem (Join-Path $ClientDir "Data") -Directory -ErrorAction SilentlyContinue |
      Where-Object { $_.Name -match '^[a-z]{2}[A-Z]{2}$' } |
      ForEach-Object { Join-Path $_.FullName "realmlist.wtf" })
foreach ($wtf in $targets) {
  $dir = Split-Path $wtf
  if (-not (Test-Path $dir)) { continue }
  $existing = if (Test-Path $wtf) { Get-Content $wtf | Where-Object { $_ -notmatch '^\s*[Ss][Ee][Tt]\s+realmlist' } } else { @() }
  Set-Content -Path $wtf -Value (@($existing) + @("set realmlist $Realmlist")) -Encoding ascii
  Say "wrote realmlist into $wtf"
}

# 1b. Windowed, and small. Measured on the 1.12.1 client, 2026-09-07: it starts
#     FULLSCREEN and plays its intro, so the screenshot at 22 seconds was the
#     cinematic and every keystroke went into a client that had no login box
#     yet. A windowed client can also be raised; a fullscreen one takes the
#     whole screen and the log cannot show what else was in front of it.
$config = Join-Path $ClientDir "WTF\Config.wtf"
$keep = if (Test-Path $config) {
  Get-Content $config | Where-Object { $_ -notmatch '^\s*[Ss][Ee][Tt]\s+(gxWindow|gxMaximize|gxResolution|readTOS|readEULA|movie)' }
} else { @() }
Set-Content -Path $config -Value (@($keep) + @(
  'SET gxWindow "1"',
  'SET gxMaximize "0"',
  'SET gxResolution "1024x768"',
  'SET readTOS "1"',
  'SET readEULA "1"',
  'SET movie "0"'
)) -Encoding ascii
Say "asked for a 1024x768 window"

# 2. Launch, from the client's own directory: the client resolves Data\ relative
#    to its working directory and exits without a window otherwise.
$processName = [System.IO.Path]::GetFileNameWithoutExtension($Exe)
Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$exe = Join-Path $ClientDir $Exe
$proc = Start-Process -FilePath $exe -WorkingDirectory $ClientDir -PassThru
Say "started Wow.exe as pid $($proc.Id)"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# 3. Wait for a window, and then MAKE IT THE FOREGROUND WINDOW.
#
#    Measured on this host, 2026-09-07: the 2.4.3 client never publishes a
#    `MainWindowHandle` -- the wait ran its full 90 seconds and reported 0 --
#    and the task's own console sits on top of it, so every keystroke went into
#    cmd.exe and the login screen was photographed with an empty password box.
#    The window is found by its class instead (`GxWindowClass`, WoW's own) and
#    raised, and the keys are only sent once Windows agrees it has the focus.
Add-Type -ReferencedAssemblies System.Drawing @'
using System;
using System.Runtime.InteropServices;
public struct RECT { public int Left, Top, Right, Bottom; }
public static class Win {
  [DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)]
  public static extern IntPtr FindWindow(string cls, string name);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref System.Drawing.Point p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, int extra);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
}
'@

$hwnd = [IntPtr]::Zero
$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline) {
  $proc.Refresh()
  if ($proc.HasExited) { Say "the client exited before a window appeared"; break }
  # Both routes, because this client has answered BOTH ways on this host: one
  # run published no `MainWindowHandle` at all, and the next published one
  # while `GxWindowClass` found nothing. Whichever answers is the window.
  if ($proc.MainWindowHandle -ne [IntPtr]::Zero) { $hwnd = $proc.MainWindowHandle }
  else {
    foreach ($cls in @('GxWindowClass', 'GxWindowClassD3d')) {
      $found = [Win]::FindWindow($cls, $null)
      if ($found -ne [IntPtr]::Zero) { $hwnd = $found; break }
    }
  }
  if ($hwnd -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 500
}
Say "window handle: $hwnd (MainWindowHandle was $($proc.MainWindowHandle))"
Start-Sleep -Seconds 12

function Raise {
  if ($hwnd -eq [IntPtr]::Zero) { return }
  [Win]::ShowWindow($hwnd, 9) | Out-Null   # SW_RESTORE
  [Win]::BringWindowToTop($hwnd) | Out-Null
  [Win]::SetForegroundWindow($hwnd) | Out-Null
  Start-Sleep -Seconds 1
  $front = [Win]::GetForegroundWindow()
  Say "foreground window: $front (the client's is $hwnd)"
  if ($front -ne $hwnd) { Say "the client is NOT in front" }
}
Raise


function Shoot([string]$name) {
  $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
  $path = Join-Path $out "$name.png"
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $path"
}

# NOT with ESC. Measured on the 1.12.1 client, 2026-09-07: three of them left
# no client running at all, and the answer shot was a bare desktop. The intro
# is switched off in the configuration instead, and the wait is just a wait.
Start-Sleep -Seconds $IntroSeconds
Shoot "$Label-1-login-screen"

# 4. Type it. The account field has focus on this login screen too; the
#    account box is cleared first because the client remembers the last name.
Raise

function ClickAt([double]$fx, [double]$fy) {
  $r = New-Object RECT
  [void][Win]::GetClientRect($hwnd, [ref]$r)
  $p = New-Object System.Drawing.Point ([int]($r.Right * $fx)), ([int]($r.Bottom * $fy))
  [void][Win]::ClientToScreen($hwnd, [ref]$p)
  [void][Win]::SetCursorPos($p.X, $p.Y)
  Start-Sleep -Milliseconds 250
  [Win]::mouse_event(0x0002, 0, 0, 0, 0)   # left down
  Start-Sleep -Milliseconds 80
  [Win]::mouse_event(0x0004, 0, 0, 0, 0)   # left up
  Start-Sleep -Milliseconds 250
  Say "clicked $($p.X),$($p.Y)"
}

if ($ClickFields) { ClickAt $FieldX $AccountY }
[System.Windows.Forms.SendKeys]::SendWait("^a")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Account)
Start-Sleep -Milliseconds 400
if ($ClickFields) { ClickAt $FieldX $PasswordY } else { [System.Windows.Forms.SendKeys]::SendWait($NextField) }
Start-Sleep -Milliseconds 300
if ($ClickFields) { [System.Windows.Forms.SendKeys]::SendWait("^a") }
[System.Windows.Forms.SendKeys]::SendWait($Password)
Start-Sleep -Milliseconds 400
Shoot "$Label-2-typed"
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "sent the login"

Start-Sleep -Seconds $SettleSeconds
$proc.Refresh()
Say "the client is $(if ($proc.HasExited) { 'GONE' } else { 'still running' }) at the answer"
Shoot "$Label-3-answer"

Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
Say "closed the client"
