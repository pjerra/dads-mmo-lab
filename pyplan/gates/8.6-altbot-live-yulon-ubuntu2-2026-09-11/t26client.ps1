# T26 live half -- the client seat, one stage per press.
#
# Runs on the Hyper-V host `vmhost` (DESKTOP-FP27AUV) against its own 3.3.5a
# client, `C:\clients\WoW-WotLK-3.3.5a-min`. It is `client-login.ps1` (8.4a) and
# `client-who.ps1` (8.5a) taken apart into STAGES, because this lane's client
# must stay logged in across many presses made from another box: those two
# scripts do everything in one task and kill the client at the end.
#
# Every interactive stage runs in session 1 through `t26run.ps1`'s scheduled
# task -- an ssh session is session 0 and cannot draw or reach a window.
# The client itself is started with `start` from a .cmd, so it is NOT a child of
# the task and outlives it (measured: a task's own process tree is not killed
# when the task ends, unlike an ssh exec's).
#
# The password is never a parameter and never in argv: `login` reads
# `pw.txt` beside this script and the caller deletes that file afterwards.
param(
  [Parameter(Mandatory = $true)][string]$Stage,
  [string]$ClientDir = 'C:\clients\WoW-WotLK-3.3.5a-min',
  [string]$Realmlist = '',
  [string]$Account = '',
  [string]$Label = 'shot',
  [string]$Keys = '',
  [string]$Text = '',
  [double]$X = 0.5,
  [double]$Y = 0.5,
  [int]$Wait = 3
)

$ErrorActionPreference = 'Stop'
$Work = 'C:\Users\PK\t26'
$Shots = Join-Path $Work 'shots'
New-Item -ItemType Directory -Force -Path $Work, $Shots | Out-Null
$log = Join-Path $Work 't26client.log'
$done = Join-Path $Work 'stage.done'
Remove-Item $done -ErrorAction SilentlyContinue

function Say([string]$text) {
  $line = "[{0}] {1}" -f (Get-Date -Format 'yyyy-MM-dd HH:mm:ss K'), $text
  Add-Content -Path $log -Value $line -Encoding utf8
  Write-Output $line
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
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

function ClientWindow() {
  $p = Get-Process wow -ErrorAction SilentlyContinue | Select-Object -First 1
  if ($p -and $p.MainWindowHandle -ne [IntPtr]::Zero) { return $p.MainWindowHandle }
  foreach ($cls in @('GxWindowClass', 'GxWindowClassD3d')) {
    $found = [Win]::FindWindow($cls, $null)
    if ($found -ne [IntPtr]::Zero) { return $found }
  }
  return [IntPtr]::Zero
}

function Raise() {
  $h = ClientWindow
  if ($h -eq [IntPtr]::Zero) { Say "no client window to raise"; return [IntPtr]::Zero }
  [Win]::ShowWindow($h, 9) | Out-Null
  [Win]::BringWindowToTop($h) | Out-Null
  [Win]::SetForegroundWindow($h) | Out-Null
  Start-Sleep -Milliseconds 800
  $front = [Win]::GetForegroundWindow()
  if ($front -ne $h) { Say "the client is NOT in front (foreground $front, client $h)" }
  return $h
}

function Grab([System.Drawing.Point]$at, [System.Drawing.Size]$size, [string]$path) {
  $bmp = New-Object System.Drawing.Bitmap $size.Width, $size.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($at, [System.Drawing.Point]::Empty, $size)
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $path"
}

function Shoot([string]$name) {
  # TWO frames per press: the whole desktop, which shows this is a real screen
  # with a real client on it, and the client's own rectangle, which is where the
  # chat window's words are legible without a crop.
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  Grab $b.Location $b.Size (Join-Path $Shots "$name.png")
  $h = ClientWindow
  if ($h -ne [IntPtr]::Zero) {
    $r = New-Object RECT
    [void][Win]::GetClientRect($h, [ref]$r)
    $o = New-Object System.Drawing.Point 0, 0
    [void][Win]::ClientToScreen($h, [ref]$o)
    $size = New-Object System.Drawing.Size ($r.Right), ($r.Bottom)
    if ($size.Width -gt 0 -and $size.Height -gt 0) {
      Grab $o $size (Join-Path $Shots "$name-client.png")
    }
  }
}

function ClickAt([double]$fx, [double]$fy) {
  $h = Raise
  if ($h -eq [IntPtr]::Zero) { return }
  $r = New-Object RECT
  [void][Win]::GetClientRect($h, [ref]$r)
  $p = New-Object System.Drawing.Point ([int]($r.Right * $fx)), ([int]($r.Bottom * $fy))
  [void][Win]::ClientToScreen($h, [ref]$p)
  [void][Win]::SetCursorPos($p.X, $p.Y)
  Start-Sleep -Milliseconds 250
  [Win]::mouse_event(0x0002, 0, 0, 0, 0)
  Start-Sleep -Milliseconds 80
  [Win]::mouse_event(0x0004, 0, 0, 0, 0)
  Start-Sleep -Milliseconds 250
  Say "clicked $fx,$fy -> $($p.X),$($p.Y)"
}

Say "=== stage $Stage  label=$Label  session=$((Get-Process -Id $PID).SessionId) ==="

switch ($Stage) {

  'config' {
    # The realmlist in every place this client might read it, and the window
    # settings. `Data\<locale>\realmlist.wtf` is the one that wins on a 3.3.5a
    # client and it OVERRIDES `WTF\Config.wtf` (memory note
    # `wow-client-realmlist-lives-in-loginui`, corrected 2026-09-04), so all of
    # them are written and each is logged.
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
      Say "wrote 'set realmlist $Realmlist' into $wtf"
    }
    $config = Join-Path $ClientDir "WTF\Config.wtf"
    $keep = if (Test-Path $config) {
      Get-Content $config | Where-Object { $_ -notmatch '^\s*[Ss][Ee][Tt]\s+(gxWindow|gxMaximize|gxResolution|readTOS|readEULA|movie|chatLog)' }
    } else { @() }
    Set-Content -Path $config -Value (@($keep) + @(
      'SET gxWindow "1"',
      'SET gxMaximize "0"',
      'SET gxResolution "1024x768"',
      'SET readTOS "1"',
      'SET readEULA "1"',
      'SET movie "0"',
      'SET chatLog "1"'
    )) -Encoding ascii
    Say "asked for a 1024x768 window and chatLog on"
    Get-Content $config | ForEach-Object { Say "  Config.wtf: $_" }
  }

  'launch' {
    Get-Process wow -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 1
    $cmd = Join-Path $Work 'launch.cmd'
    Set-Content -Path $cmd -Value @(
      '@echo off',
      "cd /d `"$ClientDir`"",
      'start "" wow.exe'
    ) -Encoding ascii
    & cmd.exe /c $cmd
    Say "started wow.exe from $ClientDir"
    $deadline = (Get-Date).AddSeconds(90)
    $h = [IntPtr]::Zero
    while ((Get-Date) -lt $deadline) {
      $h = ClientWindow
      if ($h -ne [IntPtr]::Zero) { break }
      Start-Sleep -Milliseconds 500
    }
    Say "window handle: $h"
    Start-Sleep -Seconds $Wait
    Raise | Out-Null
    Shoot $Label
  }

  'login' {
    $pw = Get-Content (Join-Path $Work 'pw.txt') -Raw
    $pw = $pw.Trim()
    Raise | Out-Null
    [System.Windows.Forms.SendKeys]::SendWait("^a")
    Start-Sleep -Milliseconds 300
    [System.Windows.Forms.SendKeys]::SendWait($Account)
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait("{TAB}")
    Start-Sleep -Milliseconds 300
    [System.Windows.Forms.SendKeys]::SendWait($pw)
    Start-Sleep -Milliseconds 400
    Say "typed the account $Account and its password (the password is not in this log)"
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "sent the login"
    Start-Sleep -Seconds $Wait
    Raise | Out-Null
    Shoot $Label
  }

  'keys' {
    Raise | Out-Null
    foreach ($k in $Keys.Split('|')) {
      if ($k -eq '') { continue }
      [System.Windows.Forms.SendKeys]::SendWait($k)
      Say "sent keys $k"
      Start-Sleep -Milliseconds 600
    }
    Start-Sleep -Seconds $Wait
    Raise | Out-Null
    Shoot $Label
  }

  'type' {
    # One line into the game's chat box: ENTER opens it, the text goes in, ENTER
    # sends it. Used for the staging a client alone can do (a character's own
    # /commands) and never for the four claims, which are pressed from the box.
    Raise | Out-Null
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Start-Sleep -Milliseconds 700
    [System.Windows.Forms.SendKeys]::SendWait($Text)
    Start-Sleep -Milliseconds 700
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "typed into the chat line: $Text"
    Start-Sleep -Seconds $Wait
    Raise | Out-Null
    Shoot $Label
  }

  'click' {
    ClickAt $X $Y
    Start-Sleep -Seconds $Wait
    Raise | Out-Null
    Shoot $Label
  }

  'shoot' {
    Raise | Out-Null
    Shoot $Label
  }

  'state' {
    $p = Get-Process wow -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($p) {
      Say "wow.exe pid=$($p.Id) session=$($p.SessionId) responding=$($p.Responding) window=$(ClientWindow)"
    } else {
      Say "wow.exe is NOT running"
    }
  }

  'quit' {
    Get-Process wow -ErrorAction SilentlyContinue | Stop-Process -Force
    Start-Sleep -Seconds 2
    $p = Get-Process wow -ErrorAction SilentlyContinue
    Say "closed the client; still running: $([bool]$p)"
  }

  default { Say "unknown stage $Stage" }
}

Set-Content -Path $done -Value (Get-Date -Format 'o') -Encoding ascii
