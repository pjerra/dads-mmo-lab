# Drive the real 1.12.1 client STEP BY STEP, from an ssh session, and photograph
# every step.
#
# Why this is not 8.4b's `client-login.ps1` or 8.5c's `client-who-vanilla.ps1`:
# both of those are fire-and-forget. They take a fixed list of parameters, run a
# fixed sequence, and hand back a folder of frames afterwards. 8.4c's remaining
# clause cannot be driven that way. It needs
#
#   * an ordinary player action IN the client (unequip two pieces of gear),
#   * then a press of a Yu'lon button on the OTHER machine, which needs the
#     client's state to already be what the press is grounded against,
#   * then a look at the result in the client again.
#
# A timed script cannot do that: the middle step happens on m910q and takes as
# long as it takes. So this driver launches the client and then reads COMMANDS
# from a queue directory, one file at a time, and answers each with a `.done`
# file. The ssh side writes `007.cmd`, waits for `007.done`, reads the frame,
# decides what to do next. Nothing is flown blind and no coordinate is guessed
# twice.
#
# Still runs in the INTERACTIVE session (schtasks /it): a process started from
# ssh lands in session 0, which has no desktop, so the client draws nothing and
# SendKeys reaches nothing (`windows-gate-box-recipes`).
#
# MEASURED ON THIS CLIENT (1.12.1, build 5875, on desktop-fp27auv, 2026-09-07/08),
# either by this run or by 8.3c before it. The ones that differ from at least
# one of the other two clients are marked (DIFFERS).
#
#  1. (DIFFERS) It starts FULLSCREEN and plays its intro. `gxWindow 1` +
#     `movie 0` in WTF\Config.wtf, or the first frames are the cinematic (8.3c).
#  2. (DIFFERS) ESC QUITS this client rather than skipping anything. Nothing
#     here presses it (8.3c lost three runs to it and photographed a bare
#     desktop).
#  3. "Hardware changed. Reload default settings?" -- a plain #32770 message box
#     that can appear BEFORE the game window, so until it is answered there is
#     no window and every click lands on the desktop (8.4b, on 2.4.3). It did
#     NOT appear on either of this run's two launches, both of which rewrote
#     WTF\Config.wtf first. The handler is kept: its absence twice is not a
#     promise.
#  4. The realm screen DOES have 2.4.3's "Choose your language / Choose your
#     realm style / Suggest Realm" panel, and NOT a plain realm list. 8.5c's
#     `client-who-vanilla.ps1` warned that the panel might be a TBC thing and
#     told the next lane not to pass `-RealmStyle` until somebody looked.
#     Somebody has: it is there, and the controls sit at the SAME client
#     fractions as on 2.4.3 -- English 0.736,0.349, Suggest Realm 0.842,0.629 --
#     read off 8.3c's own `6-client-new-realms.png` BEFORE this run started and
#     then used unchanged. Accept is 0.395,0.529; Enter World 0.498,0.921.
#  5. NOT MEASURED HERE, and left standing: whether TAB moves from the account
#     box to the password box. The `driving-a-wow-client-unattended` memory and
#     this lane's brief say it does not on 1.12.1; 8.5c flagged
#     `client-who-vanilla.ps1` for documenting the opposite in a comment copied
#     from the TBC driver. This run settles neither, because it CLICKED both
#     boxes on every attempt -- account 0.508,0.525, password 0.508,0.624, Login
#     0.506,0.703 -- and a click is right on all three clients. 8.3c's run,
#     which did use `{TAB}`, proves nothing either way: the refusal it
#     photographed ("The information you have entered is not valid") is what a
#     wrong account AND a wrong password both produce, so it cannot say which
#     box the password landed in.
#  6. (DIFFERS) The rename prompt appears when ENTER WORLD is pressed, not on
#     arrival at the character screen where the 3.3.5a client shows it -- as on
#     2.4.3. Its wording here is "Your name has been found to be in violation of
#     naming rules / Please enter a new name", where 2.4.3 says the name has
#     been "flagged for rename". Its text box already has the focus and its
#     Okay is at 0.395,0.619.
#  7. (DIFFERS, and the one that cost this run twenty minutes) RIGHT-CLICKING an
#     equipped item in the character panel does NOT unequip it here. The slot
#     looks empty for a frame and the paper doll redraws bare-chested, so the
#     screenshot says it worked -- and the item is still on: the tooltip comes
#     back, the backpack never receives it, and `character_inventory` still
#     holds the slot after `saveall` AND after a logout. What works is the
#     two-click pick-up: LEFT-click the equipment slot, so the item goes on the
#     cursor, then LEFT-click an empty bag slot.
#  8. A character flagged for rename cannot be got past: the prompt stands
#     between Enter World and the world. "Leave the rename for last" therefore
#     means pressing the button again at the end -- a session that starts with
#     the flag set has to spend it to get in at all.
#
# THE HOST, not the client:
#
#  9. `schtasks /create ... /sc once /st 23:59` -- the recipe every earlier gate
#     on this box copied -- fires AT 23:59 as well as when `/run` starts it. At
#     23:59:00 tonight three of them went off together (`yulon-login-tbc`,
#     `yulon-who-tbc`, and this lane's own task), each starting a second client
#     that killed the first one mid-session. Give the task a start date that is
#     not today: `/sd 01/01/2030`.
# 10. Windows 10's full-screen "Windows 10 support has ended" nag can take the
#     foreground and MINIMISE the client. A minimised window answers
#     `GetClientRect` with a 0x0 rectangle, so every click computes to 0,0 --
#     which is why `ClickAt` refuses instead of clicking there, and why `Raise`
#     sends SW_RESTORE and not only SetForegroundWindow.
#
# THE ONE THING THAT IS NOT A CLIENT FACT: `SendKeys` cannot hold a key down, so
# it cannot walk. `hold` below uses keybd_event with a real down/up pair.
# Measured with it on this client: the keyboard turn rate is 180 degrees per
# second (500 ms of VK_LEFT took the character from 203.3 to 293.3 degrees, read
# off `characters.orientation` either side of a `saveall`), and forward running
# covers about 7 yards a second. That is enough to walk to a coordinate from an
# ssh session with no view of the screen: read the row, take the bearing, hold
# the turn key for bearing/180 seconds, hold forward for distance/7. It is how
# this run reached the Orgrimmar mailbox, 22.8 yards and 21 yards down from the
# `Orgrimmar` teleport point.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Label,
  [string]$Exe = 'WoW.exe',
  [string]$Queue = 'C:\Users\PK\client-drive',
  [int]$IdleMinutes = 90
)

$ErrorActionPreference = 'Stop'
$out = Join-Path $env:USERPROFILE "client-login"
New-Item -ItemType Directory -Force -Path $out | Out-Null
New-Item -ItemType Directory -Force -Path $Queue | Out-Null
$log = Join-Path $out "$Label.log"

function Say([string]$text) {
  $line = "[{0}] {1}" -f (Get-Date -Format 'HH:mm:ssZ'), $text
  Add-Content -Path $log -Value $line -Encoding utf8
  $script:replies += $line
}
$script:replies = @()

Set-Content -Path $log -Value "" -Encoding utf8
Say "client=$ClientDir realmlist=$Realmlist label=$Label queue=$Queue"

# 1. The realmlist, in every place THIS client might read it.
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

$processName = [System.IO.Path]::GetFileNameWithoutExtension($Exe)
Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$exe = Join-Path $ClientDir $Exe
$proc = Start-Process -FilePath $exe -WorkingDirectory $ClientDir -PassThru
Say "started $Exe as pid $($proc.Id)"

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
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref System.Drawing.Point p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, int extra);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, int extra);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
}
'@

function FindClient {
  $h = [IntPtr]::Zero
  $windowed = Get-Process $processName -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
  if ($windowed) { return $windowed.MainWindowHandle }
  foreach ($cls in @('GxWindowClass', 'GxWindowClassD3d')) {
    $found = [Win]::FindWindow($cls, $null)
    if ($found -ne [IntPtr]::Zero) { return $found }
  }
  $found = [Win]::FindWindow($null, 'World of Warcraft')
  if ($found -ne [IntPtr]::Zero) { return $found }
  return $h
}

$hwnd = [IntPtr]::Zero
$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline) {
  $proc.Refresh()
  if ($proc.HasExited) { Say "the client exited before a window appeared"; break }
  $hwnd = FindClient
  if ($hwnd -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 500
}
Say "window handle: $hwnd"

# "Hardware changed. Reload default settings?" -- answered by its own class, not
# by a blind ENTER, so a run where it does not appear does not press something.
$dlg = [Win]::FindWindow('#32770', $null)
if ($dlg -ne [IntPtr]::Zero) {
  [Win]::SetForegroundWindow($dlg) | Out-Null
  Start-Sleep -Milliseconds 500
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "answered a message box (class #32770, handle $dlg)"
  Start-Sleep -Seconds 6
  $hwnd = FindClient
  Say "window handle after the message box: $hwnd"
} else {
  Say "no message box was up"
}

function Raise {
  if ($script:hwnd -eq [IntPtr]::Zero) { $script:hwnd = FindClient }
  if ($script:hwnd -eq [IntPtr]::Zero) { Say "no window to raise"; return }
  [Win]::ShowWindow($script:hwnd, 9) | Out-Null   # SW_RESTORE
  [Win]::BringWindowToTop($script:hwnd) | Out-Null
  [Win]::SetForegroundWindow($script:hwnd) | Out-Null
  Start-Sleep -Milliseconds 700
  $front = [Win]::GetForegroundWindow()
  if ($front -ne $script:hwnd) { Say "the client is NOT in front (front=$front, client=$($script:hwnd))" }
}

# Every photograph records whether a client was alive when it was taken and
# where the pointer was. 8.3c produced a shot of a bare desktop that read as an
# answer; a shot with no live client is a failed step, not a result.
function Shoot([string]$name, [bool]$full) {
  $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $origin = $bounds.Location
  $size = $bounds.Size
  if (-not $full -and $script:hwnd -ne [IntPtr]::Zero) {
    $r = New-Object RECT
    if ([Win]::GetWindowRect($script:hwnd, [ref]$r)) {
      $w = $r.Right - $r.Left
      $h = $r.Bottom - $r.Top
      if ($w -gt 100 -and $h -gt 100) {
        $origin = New-Object System.Drawing.Point $r.Left, $r.Top
        $size = New-Object System.Drawing.Size $w, $h
      }
    }
  }
  $bmp = New-Object System.Drawing.Bitmap $size.Width, $size.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($origin, [System.Drawing.Point]::Empty, $size)
  $path = Join-Path $out "$name.png"
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  $alive = @(Get-Process $processName -ErrorAction SilentlyContinue).Count -gt 0
  Say "photographed $path ($($size.Width)x$($size.Height) at $($origin.X),$($origin.Y); client alive: $alive)"
}

function ClientPoint([double]$fx, [double]$fy) {
  $r = New-Object RECT
  [void][Win]::GetClientRect($script:hwnd, [ref]$r)
  $p = New-Object System.Drawing.Point ([int]($r.Right * $fx)), ([int]($r.Bottom * $fy))
  [void][Win]::ClientToScreen($script:hwnd, [ref]$p)
  return $p
}

function ClickAt([double]$fx, [double]$fy, [string]$button) {
  if ($script:hwnd -eq [IntPtr]::Zero) { Say "no window handle, so no click at $fx,$fy"; return }
  $p = ClientPoint $fx $fy
  [void][Win]::SetCursorPos($p.X, $p.Y)
  Start-Sleep -Milliseconds 250
  if ($button -eq 'right') { $down = 0x0008; $up = 0x0010 } else { $down = 0x0002; $up = 0x0004 }
  [Win]::mouse_event($down, 0, 0, 0, 0)
  Start-Sleep -Milliseconds 90
  [Win]::mouse_event($up, 0, 0, 0, 0)
  Start-Sleep -Milliseconds 250
  Say "$button-clicked $($p.X),$($p.Y)  (fractions $fx,$fy)"
}

# SendKeys has no way to hold a key. Walking, and anything else that needs a key
# held down, goes through keybd_event with a real down/up pair.
function HoldKey([byte]$vk, [int]$ms) {
  [Win]::keybd_event($vk, 0, 0, 0)
  Start-Sleep -Milliseconds $ms
  [Win]::keybd_event($vk, 0, 2, 0)   # KEYEVENTF_KEYUP
  Say "held vk $vk for $ms ms"
}

function Literal([string]$text) {
  # SendKeys' own metacharacters, escaped so a password or a chat line arrives
  # as itself.
  return ($text -replace '([+^%~()\[\]{}])', '{$1}')
}

Say "READY -- watching $Queue for NNN.cmd"
Set-Content -Path (Join-Path $Queue "READY") -Value (Get-Date -Format 'o') -Encoding ascii

$idle = (Get-Date).AddMinutes($IdleMinutes)
$running = $true
while ($running -and (Get-Date) -lt $idle) {
  $next = Get-ChildItem $Queue -Filter '*.cmd' -ErrorAction SilentlyContinue |
    Where-Object { -not (Test-Path (Join-Path $Queue ($_.BaseName + '.done'))) } |
    Sort-Object Name | Select-Object -First 1
  if (-not $next) { Start-Sleep -Milliseconds 800; continue }

  $script:replies = @()
  Say "=== $($next.Name)"
  foreach ($raw in (Get-Content $next.FullName)) {
    $line = $raw.Trim()
    if (-not $line -or $line.StartsWith('#')) { continue }
    $verb, $rest = ($line -split '\s+', 2)
    $verb = $verb.ToLower()
    try {
      switch ($verb) {
        'shoot'      { Raise; Shoot $rest $false }
        'shootnow'   { Shoot $rest $false }          # no raise: photograph what is actually in front
        'shootfull'  { Raise; Shoot $rest $true }
        'raise'      { Raise; Say "front is $([Win]::GetForegroundWindow()), client is $($script:hwnd)" }
        'find'       { $script:hwnd = FindClient; Say "window handle: $($script:hwnd)" }
        'click'      { $a = $rest -split '\s+'; ClickAt ([double]$a[0]) ([double]$a[1]) 'left' }
        'rclick'     { $a = $rest -split '\s+'; ClickAt ([double]$a[0]) ([double]$a[1]) 'right' }
        'dclick'     { $a = $rest -split '\s+'; ClickAt ([double]$a[0]) ([double]$a[1]) 'left'; Start-Sleep -Milliseconds 120; ClickAt ([double]$a[0]) ([double]$a[1]) 'left' }
        'move'       { $a = $rest -split '\s+'; $p = ClientPoint ([double]$a[0]) ([double]$a[1]); [void][Win]::SetCursorPos($p.X, $p.Y); Say "moved to $($p.X),$($p.Y)" }
        'key'        { [System.Windows.Forms.SendKeys]::SendWait($rest); Say "sent key $rest" }
        'type'       { [System.Windows.Forms.SendKeys]::SendWait((Literal $rest)); Say "typed $rest" }
        'chat'       {
                       [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
                       Start-Sleep -Milliseconds 900
                       [System.Windows.Forms.SendKeys]::SendWait((Literal $rest))
                       Start-Sleep -Milliseconds 900
                       [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
                       Say "chat: $rest"
                     }
        'hold'       { $a = $rest -split '\s+'; HoldKey ([byte]$a[0]) ([int]$a[1]) }
        'sleep'      { Start-Sleep -Seconds ([double]$rest); Say "slept $rest s" }
        'dialog'     {
                       $d = [Win]::FindWindow('#32770', $null)
                       if ($d -ne [IntPtr]::Zero) {
                         [Win]::SetForegroundWindow($d) | Out-Null
                         Start-Sleep -Milliseconds 500
                         [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
                         Say "answered a message box ($d)"
                       } else { Say "no message box was up" }
                     }
        'alive'      {
                       $ps = @(Get-Process $processName -ErrorAction SilentlyContinue)
                       Say "clients running: $($ps.Count); handles: $(($ps | ForEach-Object { $_.MainWindowHandle }) -join ',')"
                     }
        'endloop'    { $running = $false; Say "leaving the loop, client left running" }
        'quit'       {
                       Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
                       $running = $false
                       Say "closed the client"
                     }
        default      { Say "UNKNOWN COMMAND: $line" }
      }
    } catch {
      Say "ERROR on '$line': $($_.Exception.Message)"
    }
  }
  Set-Content -Path (Join-Path $Queue ($next.BaseName + '.done')) -Value $script:replies -Encoding utf8
  $idle = (Get-Date).AddMinutes($IdleMinutes)
}

Say "driver finished (running=$running)"
Set-Content -Path (Join-Path $Queue "FINISHED") -Value (Get-Date -Format 'o') -Encoding ascii
