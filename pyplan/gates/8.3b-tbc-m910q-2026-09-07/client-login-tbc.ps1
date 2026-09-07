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
  [int]$SettleSeconds = 22
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

# 2. Launch, from the client's own directory: the client resolves Data\ relative
#    to its working directory and exits without a window otherwise.
Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$exe = Join-Path $ClientDir "Wow.exe"
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
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Win {
  [DllImport("user32.dll", SetLastError=true, CharSet=CharSet.Auto)]
  public static extern IntPtr FindWindow(string cls, string name);
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

if ($hwnd -ne [IntPtr]::Zero) {
  [Win]::ShowWindow($hwnd, 9) | Out-Null   # SW_RESTORE
  [Win]::BringWindowToTop($hwnd) | Out-Null
  [Win]::SetForegroundWindow($hwnd) | Out-Null
  Start-Sleep -Seconds 1
}
$front = [Win]::GetForegroundWindow()
Say "foreground window: $front (the client's is $hwnd)"
if ($front -ne $hwnd) { Say "WARNING: the client is not in front; what follows was typed at something else" }

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

Shoot "$Label-1-login-screen"

# 4. Type it. The account field has focus on this login screen too; the
#    account box is cleared first because the client remembers the last name.
[System.Windows.Forms.SendKeys]::SendWait("^a")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Account)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("{TAB}")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Password)
Start-Sleep -Milliseconds 400
Shoot "$Label-2-typed"
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "sent the login"

Start-Sleep -Seconds $SettleSeconds
Shoot "$Label-3-answer"

Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Say "closed the client"
