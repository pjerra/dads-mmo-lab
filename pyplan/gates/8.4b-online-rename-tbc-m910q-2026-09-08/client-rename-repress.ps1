# Drive a real 2.4.3 client into the world, HOLD it there, and leave the way a
# player leaves.
#
# 8.4b's owed re-press: the ONLINE rename. This is `client-who-tbc.ps1` with the
# `/who` tail replaced by a `/logout`, and NOT the repo's older
# `client-login.ps1`, which lacks the "Hardware changed" message-box answer and
# puts the rename prompt at the wrong screen -- both measured on THIS client and
# THIS host by 8.4b and 8.5b, and both look like a broken feature rather than a
# broken driver when they go wrong.
#
# Why a chat `/logout` and not `Stop-Process`: killing the client drops the
# socket, and what the world then saves is a linkdead session rather than a
# logout. 8.4a's finding is that the row moves AT LOGOUT, so the logout has to
# be a real one or the reading is about something else.
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
  [double]$FieldX = 0.518,
  # Go past the character screen and into the world. One ENTER picks the
  # first character on the list and one more confirms; the client then
  # loads, which takes longer than the login did.
  [switch]$EnterWorld,
  [switch]$RealmStyle,
  [int]$WorldSeconds = 60,
  # A character flagged for rename cannot enter the world until it is
  # renamed: the prompt sits in front of the character screen. Giving a
  # name here finishes what `character rename` started, which is the
  # other half of that action's proof.
  [string]$RenameTo = '',
  # The names to ask the who-list about, in order. Two of them on this tree:
  # 8.5a measured on a 3.3.5a client that /who is faction-filtered, and whether
  # THIS client also filters by level is unmeasured -- the asker is level 25 and
  # the online bots run to 70. Nearest-level first, so a zero on the second one
  # is a level filter and a zero on both is something else.
  [string[]]$Who = @(),
  [int]$WhoSeconds = 10,
  # Leave the world through the client's own `/logout`, and photograph what is
  # on the screen afterwards.
  [switch]$LogoutFromChat,
  [int]$LogoutSettleSeconds = 45
)

$ErrorActionPreference = 'Stop'
$out = Join-Path $env:USERPROFILE "client-login"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$log = Join-Path $out "$Label.log"

function Say([string]$text) {
  # UTC, and it really is UTC. The older driver stamped LOCAL time and suffixed
  # it with a Z; this run is ordered against a server log on another machine two
  # hours behind that lie, and the ordering is the whole evidence.
  $line = "[{0}] {1}" -f (Get-Date).ToUniversalTime().ToString('HH:mm:ssZ'), $text
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
  # Not `$proc` alone: this client can re-exec, leaving the handle we started
  # with at zero forever while another process of the same name owns the window
  # (measured on the 2.4.3 client, 2026-09-07, where every click went to 0,0).
  $windowed = Get-Process $processName -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
  if ($windowed) { $hwnd = $windowed.MainWindowHandle }
  elseif ($proc.MainWindowHandle -ne [IntPtr]::Zero) { $hwnd = $proc.MainWindowHandle }
  else {
    foreach ($cls in @('GxWindowClass', 'GxWindowClassD3d')) {
      $found = [Win]::FindWindow($cls, $null)
      if ($found -ne [IntPtr]::Zero) { $hwnd = $found; break }
    }
    if ($hwnd -eq [IntPtr]::Zero) {
      # Third route, and the 2.4.3 client needs it: no MainWindowHandle and no
      # GxWindowClass either, so the clicks went to 0,0 and the login was typed
      # at the desktop. The title is what is left.
      $found = [Win]::FindWindow($null, 'World of Warcraft')
      if ($found -ne [IntPtr]::Zero) { $hwnd = $found }
    }
  }
  if ($hwnd -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 500
}
Say "window handle: $hwnd (MainWindowHandle was $($proc.MainWindowHandle))"

# 3b. "Hardware changed. Reload default settings?" -- a plain Windows message
#     box this client puts up when WTF\Config.wtf has been edited, and it
#     appears BEFORE the game window, so until it is answered there is no window
#     to find and every click goes to 0,0 (8.4b, 2026-09-07: three runs lost to
#     it). Answered by its own dialog class rather than by sending a blind
#     ENTER, so that a run where it does NOT appear does not press the login
#     button with an empty account box.
$dlg = [Win]::FindWindow('#32770', $null)
if ($dlg -ne [IntPtr]::Zero) {
  [Win]::SetForegroundWindow($dlg) | Out-Null
  Start-Sleep -Milliseconds 500
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "answered a message box (class #32770, handle $dlg)"
  Start-Sleep -Seconds 6
  foreach ($cls in @('GxWindowClass', 'GxWindowClassD3d')) {
    $found = [Win]::FindWindow($cls, $null)
    if ($found -ne [IntPtr]::Zero) { $hwnd = $found; break }
  }
  Say "window handle after the message box: $hwnd"
} else {
  Say "no message box was up"
}
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
  # Whether the client was ALIVE at this capture, at EVERY capture. A client
  # that has died photographs exactly like a client refusing to do something,
  # and nothing in the picture tells the two apart afterwards.
  $proc.Refresh()
  $alive = if ($proc.HasExited) { "the client was GONE" } else { "the client was alive (pid $($proc.Id))" }
  $others = @(Get-Process $processName -ErrorAction SilentlyContinue).Count
  Say "photographed $path -- $alive, $others process(es) named $processName"
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
  if ($hwnd -eq [IntPtr]::Zero) {
    # No window handle means no coordinates: clicking 0,0 puts the pointer in
    # the corner of the desktop and types the login at whatever is there. The
    # 2.4.3 client publishes no handle on this host, so the keyboard is what is
    # left, and saying so is better than a click that goes nowhere.
    Say "no window handle, so no click at $fx,$fy -- sending ENTER instead"
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    return
  }
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

Start-Sleep -Seconds ([Math]::Max($SettleSeconds, 30))
if ($EnterWorld) {
  # Three screens, not one: the realm list, the character list, and the world.
  # Measured on the 3.3.5a client, 2026-09-07 -- a single ENTER left it sitting
  # on "Realm Selection", which the log called "asked to enter the world".
  Raise
  Shoot "$Label-3a-realms"
  # The realm dialog does NOT take ENTER -- measured twice, 2026-09-07: the
  # dialog was still up eight seconds later and the second ENTER dropped the
  # client back to the login screen. Its Okay button is clicked instead.
  if ($RealmStyle) {
    # The 2.4.3 realm screen asks for a LANGUAGE before it shows any realms:
    # its "Choose your language" panel has one checkbox and the list appears
    # only after it is ticked. The 3.3.5a client goes straight to the list.
    ClickAt 0.737 0.344
    Start-Sleep -Seconds 4
    Raise
    Shoot "$Label-3a2-language"
    # Ticking it lights up "Suggest Realm", which is how this client gets from
    # the realm-STYLE screen to an actual realm. The 3.3.5a client shows the
    # list straight away and has neither control.
    ClickAt 0.841 0.629
    Start-Sleep -Seconds 8
    Raise
    Shoot "$Label-3a3-realm-list"
    # "You have been assigned to the MaNGOS Realm." -- Accept, and this client
    # is at its character screen. Three clicks where the 3.3.5a client needs
    # one, and none of them in the same place.
    ClickAt 0.396 0.527
    # "Logging in to game server" follows, and the character list takes a while
    # to arrive. Clicking Okay during it RE-OPENS the realm dialog -- measured,
    # 2026-09-07 -- so this waits instead, and the Okay below is skipped
    # entirely because Accept has already chosen the realm.
    Start-Sleep -Seconds 25
    Raise
    Shoot "$Label-3a4-accepted"
  }
  # Okay on the realm-selection dialog. BOTH clients end up here: the 3.3.5a
  # one shows it straight after login, and the 2.4.3 one only after the
  # language, Suggest Realm and Accept above -- at almost exactly the same
  # place, which is why one coordinate serves both.
  # Okay on the realm-selection dialog, which BOTH clients put in front of the
  # character screen. It has to come AFTER "Logging in to game server" has
  # finished: clicked during it, the dialog simply re-opens (measured twice).
  ClickAt 0.616 0.788
  Start-Sleep -Seconds 20
  Raise
  Shoot "$Label-3b-characters"
  # The character screen: ENTER takes the selected character into the world.
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "pressed Enter World"
  Start-Sleep -Seconds 8
  Raise
  Shoot "$Label-3b2-enter-world"
  if ($RenameTo) {
    # The rename prompt comes up at ENTER WORLD on this client, not on arrival
    # at the character screen -- measured by 8.4b on this same client and host,
    # which photographed the prompt without answering it. So the name is typed
    # AFTER the first Enter World, and a second one is pressed afterwards
    # because answering the prompt puts the character screen back.
    [System.Windows.Forms.SendKeys]::SendWait($RenameTo)
    Start-Sleep -Milliseconds 600
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "answered the rename prompt with $RenameTo"
    Start-Sleep -Seconds 8
    Raise
    Shoot "$Label-3b3-renamed"
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "pressed Enter World again, after the rename"
  }
  Start-Sleep -Seconds 10
  Raise
  Shoot "$Label-3c-entering"
  Say "asked to enter the world"
  # A frame every fifteen seconds while the character is in the world, so the
  # effects of commands sent from the other side of the machine can be seen
  # afterwards. A screenshot cannot be taken from an ssh session -- session 0
  # has no desktop -- so the client's own driver is the only thing that can
  # photograph this, and it has to do it on a timer rather than on demand.
  # The frame name carries the SECONDS it was taken at, not the word "before"
  # or "after": this driver cannot see the other machine's press, so a frame it
  # named "before" would be a claim it has no way to check. 8.2d shipped one of
  # those. What actually orders the two sides is the pair of UTC clocks.
  $frames = [Math]::Max(1, [int]($WorldSeconds / 15))
  foreach ($n in 1..$frames) {
    Start-Sleep -Seconds 15
    Raise
    Shoot ("{0}-world-{1:d3}s" -f $Label, ($n * 15))
  }
  Raise

  if ($LogoutFromChat) {
    # ENTER opens the chat line, the command goes in it, ENTER sends it -- the
    # same three keystrokes the who-list tail used, which is why this is where
    # it was.
    Say "typing /logout in the client's own chat line"
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Start-Sleep -Milliseconds 900
    [System.Windows.Forms.SendKeys]::SendWait("/logout")
    Start-Sleep -Milliseconds 900
    Shoot "$Label-logout-typed"
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "sent /logout"
    Start-Sleep -Seconds $LogoutSettleSeconds
    Raise
    Shoot "$Label-after-logout"
  }

  # The who-list. ENTER opens the chat line and the command prints its answer
  # there. One shot per name, named after the name, because the pass criterion
  # is "the chosen name appears in the answer" -- read off the screenshot in
  # the client's own words rather than asserted from 8.5a's 3.3.5a string.
  # Split on commas here rather than relying on the parameter binder. The
  # driver hands the names through a .cmd file (/tr is capped at 261
  # characters), and an unquoted `-Who Ellalanea,Fika` arrived as ONE string:
  # the first run asked `/who Ellalanea,Fika` and the client answered
  # "0 players total", which is exactly what a real miss looks like.
  $names = @($Who) -split ',' | Where-Object { $_.Trim() } | ForEach-Object { $_.Trim() }
  Say "who-list names: $($names -join ' | ')"
  $n = 0
  foreach ($name in $names) {
    $n = $n + 1
    Raise
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Start-Sleep -Milliseconds 900
    [System.Windows.Forms.SendKeys]::SendWait("/who $name")
    Start-Sleep -Milliseconds 900
    Shoot ("{0}-who{1:d2}-{2}-typed" -f $Label, $n, $name)
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "asked /who $name"
    Start-Sleep -Seconds $WhoSeconds
    Raise
    Shoot ("{0}-who{1:d2}-{2}" -f $Label, $n, $name)
  }
}
$proc.Refresh()
Say "the client is $(if ($proc.HasExited) { 'GONE' } else { 'still running' }) at the answer"
Shoot "$Label-3-answer"

Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
Say "closed the client"
