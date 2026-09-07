# Drive the real Turtle 1.18.1 client into the world and ask the in-game
# who-list about a name. 8.5d's clause (4).
#
# NOT YET RUN. Written 2026-09-08 from 8.3d's `client-login.ps1` (the driver
# that actually drove THIS client, as far as the realm list) with 8.5b's `/who`
# tail grafted on. Everything past the realm list is UNMEASURED on this client
# and every such step below says so in its own comment. Read the log and the
# frames before believing any of it.
#
# Runs in the INTERACTIVE session (schtasks /it): a client started from an ssh
# session lands in session 0, where there is no desktop, so it draws nothing and
# SendKeys reaches nothing (`windows-gate-box-recipes`).
#
# ------------------------------------------------------------------ MEASURED
# on this client (1.18.1, build 7272, C:\clients\TurtleWoW, on the Hyper-V host)
#
#  * The game is `WoW.exe`. `TurtleWoW.exe` is the INSTALLER and `turtle-wow.exe`
#    is the LAUNCHER: 8.3d typed a login into a setup wizard and into an update
#    dialog before it found that out.
#  * TAB does NOT move from the account box to the password box, and neither
#    does ENTER. realmd logged `WHERE username = 'OLD-P@SS12'` on one attempt --
#    the PASSWORD had gone into the name field -- and a stale name on the next.
#    BOTH boxes are CLICKED. The fractions below are 8.3d's, which produced
#    `clicked 538,426` / `clicked 538,510` and a successful login.
#  * It starts FULLSCREEN; `SET gxWindow "1"` is what makes it driveable.
#  * `SET movie "0"`, because on this client **ESC QUITS** rather than skipping.
#    Nothing in this file sends ESC, anywhere, for any reason.
#  * It publishes a real `MainWindowHandle` (8.3d saw 5310062 and 5834546), so
#    the class-name and title fallbacks below are belt and braces.
#  * A client that has DIED photographs exactly like a client that refused, so
#    `Shoot` logs whether the process is alive at every single capture.
#
# ------------------------------------------------------------------ MEASURED
# on the SERVER (m910q, /home/pk/tortoise-server, 2026-09-08, stack down)
#
#  * `src/game/Handlers/MiscHandler.cpp:253-255` -- a **30-second /who cooldown**
#    for a SEC_PLAYER asker: a second /who inside 30 s returns NOTHING AT ALL,
#    which photographs exactly like a miss. `-WhoSeconds` is 35 by default and
#    the gap between names is never shorter. A GM asker (account rank > 0)
#    bypasses it, because `m_lastWhoRequest` is only stamped for SEC_PLAYER.
#  * `etc/mangosd.conf:919` `AllowTwoSide.WhoList = 1` -- the faction filter is
#    OFF on this install, the opposite of the Vanilla one. Both factions should
#    answer.
#  * `MiscHandler.cpp:140` drops any subject outside the LEVEL RANGE THE CLIENT
#    SENDS. What range this client puts in a bare `/who Name` is UNMEASURED.
#    That is why the gate chooses a subject near the asker's level first.
#
# ------------------------------------------------------------------- ASSUMED
#  * everything from the realm list onwards. 8.3d reached the realm list and
#    stopped there. Whether this client shows a language panel and a "Suggest
#    Realm" button (the 2.4.3 flow) or a plain realm list (the 3.3.5a flow) has
#    NOT been seen, so `-RealmStyle` is a switch and both routes photograph
#    every step. Run it once with `-DryRealm` to find out before committing.
#  * the character-creation coordinates in `-CreateCharacter`. They are named
#    parameters precisely so the first run can correct them from the frames.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Label,
  [int]$SettleSeconds = 30,
  [int]$IntroSeconds = 12,
  # MEASURED: the game is WoW.exe on this client. The process name follows.
  [string]$Exe = 'WoW.exe',
  # MEASURED: neither of these moves between the boxes on 1.18.1. Kept only so
  # a future run can say it tried, and never used while $ClickFields is true.
  [string]$NextField = '{TAB}',
  # MEASURED on 1.18.1: both boxes must be clicked. Default ON here, unlike the
  # sibling drivers, where it is an opt-in switch.
  [bool]$ClickFields = $true,
  [double]$AccountY = 0.514,
  [double]$PasswordY = 0.624,
  [double]$FieldX = 0.518,
  # Go past the character screen and into the world.
  [switch]$EnterWorld,
  # ASSUMED: the 2.4.3-style language + Suggest Realm + Accept sequence. Leave
  # it off until a frame shows this client needs it.
  [switch]$RealmStyle,
  # Stop after the realm screen and photograph it, changing nothing else. The
  # cheap way to learn which realm flow this client has.
  [switch]$DryRealm,
  [int]$WorldSeconds = 60,
  # A character flagged for rename cannot enter the world until it is renamed;
  # on the 2.4.3 client the prompt appears at ENTER WORLD, not at the character
  # screen. Whether 1.18.1 does the same is ASSUMED.
  [string]$RenameTo = '',
  # ASSUMED throughout: make a character on the account before entering the
  # world. Needed on this tree if no non-bot character exists (gate85d.py's
  # `who` stage is what says whether one does). Every step photographs.
  [switch]$CreateCharacter,
  [string]$CharacterName = '',
  [double]$CreateButtonX = 0.845, [double]$CreateButtonY = 0.905,
  [double]$AcceptButtonX = 0.845, [double]$AcceptButtonY = 0.955,
  [double]$NameBoxX = 0.500,      [double]$NameBoxY = 0.500,
  # The names to ask the who-list about, comma-separated. Split inside this
  # file: 8.5b handed `-Who 'A,B'` through a .cmd file, it bound as ONE string,
  # the client was asked `/who A,B` and answered "0 players total" -- which is
  # what a real miss looks like.
  [string[]]$Who = @(),
  # MEASURED on the server: 30-second cooldown. Never set this below 35.
  [int]$WhoSeconds = 35
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
Say "client=$ClientDir realmlist=$Realmlist account=$Account label=$Label exe=$Exe"
Say "MEASURED defaults: ClickFields=$ClickFields WhoSeconds=$WhoSeconds (30 s server cooldown)"

if ($Exe -match 'TurtleWoW\.exe|turtle-wow\.exe') {
  Say "REFUSING: $Exe is the installer/launcher on this client, not the game. Use WoW.exe."
  throw "$Exe is not the game executable on the Turtle client"
}

# 1. The realmlist, in every place THIS client might read it -- writing one and
#    guessing wrong is how the 8.1d capture photographed a refusal by a
#    completely different server. 8.3d wrote realmlist.wtf and WTF\Config.wtf on
#    this client and the login reached the realm list, so both are known good.
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

# 1b. Windowed, small, and NO INTRO MOVIE. `movie "0"` is not a nicety here: on
#     this client ESC quits, so an intro cannot be skipped, only prevented.
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
Say "asked for a 1024x768 window with no intro movie"

# 2. Launch from the client's own directory: the client resolves Data\ relative
#    to its working directory and exits without a window otherwise.
$processName = [System.IO.Path]::GetFileNameWithoutExtension($Exe)
Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$exe = Join-Path $ClientDir $Exe
if (-not (Test-Path $exe)) { Say "no $exe on this box"; throw "missing $exe" }
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
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref System.Drawing.Point p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, int extra);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
}
'@

# 3. Wait for a window. MEASURED: this client publishes a MainWindowHandle. The
#    other two routes are kept because the 2.4.3 client on this same host
#    answered all three ways across runs, and a click with no handle goes to
#    0,0 -- i.e. types the login at the desktop.
$hwnd = [IntPtr]::Zero
$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline) {
  $proc.Refresh()
  if ($proc.HasExited) { Say "the client EXITED before a window appeared"; break }
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
      $found = [Win]::FindWindow($null, 'World of Warcraft')
      if ($found -ne [IntPtr]::Zero) { $hwnd = $found }
    }
  }
  if ($hwnd -ne [IntPtr]::Zero) { break }
  Start-Sleep -Milliseconds 500
}
Say "window handle: $hwnd (MainWindowHandle was $($proc.MainWindowHandle))"

# 3b. "Hardware changed. Reload default settings?" -- a plain message box some
#     clients put up after Config.wtf is edited, BEFORE the game window. 8.3d
#     never saw one on this client; it is answered by its own dialog class
#     rather than by a blind ENTER, so a run where it does not appear does not
#     press the login button with an empty account box.
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

function Alive {
  $proc.Refresh()
  if ($proc.HasExited) { return "DEAD (pid $($proc.Id) exited)" }
  $others = @(Get-Process $processName -ErrorAction SilentlyContinue).Count
  return "alive (pid $($proc.Id), $others process(es) named $processName)"
}

function Raise {
  if ($hwnd -eq [IntPtr]::Zero) { Say "no window handle to raise"; return }
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
  # The alive/dead reading is taken AT the capture and written beside it: a
  # client that has died photographs exactly like one that refused, and 8.3c
  # photographed a bare desktop as though it were an answer.
  $state = Alive
  $bounds = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $bounds.Width, $bounds.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($bounds.Location, [System.Drawing.Point]::Empty, $bounds.Size)
  $path = Join-Path $out "$name.png"
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $path -- the client is $state"
}

function ClickAt([double]$fx, [double]$fy, [string]$what) {
  if ($hwnd -eq [IntPtr]::Zero) {
    Say "no window handle, so NO click at $fx,$fy ($what) -- a click would land at 0,0"
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
  Say "clicked $($p.X),$($p.Y) for $what"
}

# The intro is switched off in the configuration; the wait is just a wait. ESC
# is never sent: on this client it quits.
Start-Sleep -Seconds $IntroSeconds
Shoot "$Label-1-login-screen"

# 4. Type it. MEASURED: both boxes are clicked on this client.
Raise
if ($ClickFields) { ClickAt $FieldX $AccountY "the account box" }
[System.Windows.Forms.SendKeys]::SendWait("^a")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Account)
Start-Sleep -Milliseconds 400
if ($ClickFields) { ClickAt $FieldX $PasswordY "the password box" }
else { [System.Windows.Forms.SendKeys]::SendWait($NextField) }
Start-Sleep -Milliseconds 300
if ($ClickFields) { [System.Windows.Forms.SendKeys]::SendWait("^a") }
[System.Windows.Forms.SendKeys]::SendWait($Password)
Start-Sleep -Milliseconds 400
Shoot "$Label-2-typed"
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "sent the login"

Start-Sleep -Seconds ([Math]::Max($SettleSeconds, 30))
Raise
Shoot "$Label-3-after-login"

if ($DryRealm) {
  # Learn the realm flow and change nothing else. Four frames a minute apart is
  # enough to see whether a language panel or a plain list is up, and whether
  # the client is still alive while it sits there.
  foreach ($n in 1..4) {
    Start-Sleep -Seconds 15
    Raise
    Shoot ("{0}-realm{1:d2}" -f $Label, $n)
  }
  Say "DryRealm: stopping before touching anything on the realm screen"
  Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
  Say "closed the client"
  return
}

if ($EnterWorld) {
  # ---------------------------------------------------------------- ASSUMED
  # Everything from here down is unmeasured on THIS client. Each step
  # photographs immediately after acting, so a run that goes wrong says where.
  Raise
  Shoot "$Label-3a-realms"
  if ($RealmStyle) {
    ClickAt 0.737 0.344 "the language checkbox (ASSUMED, 2.4.3 coordinates)"
    Start-Sleep -Seconds 4
    Raise
    Shoot "$Label-3a2-language"
    ClickAt 0.841 0.629 "Suggest Realm (ASSUMED)"
    Start-Sleep -Seconds 8
    Raise
    Shoot "$Label-3a3-realm-list"
    ClickAt 0.396 0.527 "Accept (ASSUMED)"
    Start-Sleep -Seconds 25
    Raise
    Shoot "$Label-3a4-accepted"
  }
  # Okay on the realm-selection dialog. It has to come AFTER "Logging in to
  # game server" has finished: clicked during it, the dialog re-opens
  # (measured twice on the other clients, 2026-09-07).
  ClickAt 0.616 0.788 "Okay on the realm dialog (ASSUMED on this client)"
  Start-Sleep -Seconds 20
  Raise
  Shoot "$Label-3b-characters"

  if ($CreateCharacter) {
    # ------------------------------------------------------------- ASSUMED
    # No run has ever driven this client's creation screen. The coordinates
    # are parameters so the first run can correct them from these frames, and
    # a name is typed only if one was given. A bot's account is NOT a spare
    # seat: this must run on a human account whose password is already known.
    if (-not $CharacterName) { Say "CreateCharacter with no -CharacterName"; throw "no name" }
    ClickAt $CreateButtonX $CreateButtonY "Create New Character (ASSUMED)"
    Start-Sleep -Seconds 6
    Raise
    Shoot "$Label-3b1-creation-screen"
    ClickAt $NameBoxX $NameBoxY "the name box (ASSUMED)"
    Start-Sleep -Milliseconds 400
    [System.Windows.Forms.SendKeys]::SendWait("^a")
    [System.Windows.Forms.SendKeys]::SendWait($CharacterName)
    Start-Sleep -Milliseconds 600
    Shoot "$Label-3b2-name-typed"
    ClickAt $AcceptButtonX $AcceptButtonY "Accept (ASSUMED)"
    Say "asked to create $CharacterName -- the DATABASE is the oracle for whether it exists"
    Start-Sleep -Seconds 12
    Raise
    Shoot "$Label-3b3-created"
  }

  # The character screen: ENTER takes the selected character into the world.
  [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
  Say "pressed Enter World"
  Start-Sleep -Seconds 8
  Raise
  Shoot "$Label-3b4-enter-world"
  if ($RenameTo) {
    # ASSUMED on this client: on the 2.4.3 one the rename prompt appears at
    # ENTER WORLD rather than at the character screen, so the name is typed
    # after the first Enter World and a second one is pressed afterwards.
    [System.Windows.Forms.SendKeys]::SendWait($RenameTo)
    Start-Sleep -Milliseconds 600
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "answered a rename prompt with $RenameTo"
    Start-Sleep -Seconds 8
    Raise
    Shoot "$Label-3b5-renamed"
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "pressed Enter World again, after the rename"
  }
  Start-Sleep -Seconds 10
  Raise
  Shoot "$Label-3c-entering"
  Say "asked to enter the world"

  # A frame every fifteen seconds while the character is in the world: a
  # screenshot cannot be taken from an ssh session (session 0 has no desktop),
  # so this driver is the only thing that can photograph the world, and it has
  # to do it on a timer rather than on demand.
  $frames = [Math]::Max(1, [int]($WorldSeconds / 15))
  foreach ($n in 1..$frames) {
    Start-Sleep -Seconds 15
    Raise
    Shoot ("{0}-w{1:d2}" -f $Label, $n)
  }
  Raise

  # The who-list. ENTER opens the chat line and the command prints its answer
  # there. One shot per name, named after the name, because the pass criterion
  # is "the chosen name appears in the answer" -- read off the screenshot in
  # the client's own words, not asserted from a sibling gate's string.
  #
  # THE 30-SECOND COOLDOWN IS THE REASON FOR THE WAIT: a second /who inside it
  # is dropped by the server with no reply, and that photographs as a miss.
  $names = @($Who) -split ',' | Where-Object { $_.Trim() } | ForEach-Object { $_.Trim() }
  Say "who-list names: $($names -join ' | ') (>= $WhoSeconds s apart, server cooldown is 30 s)"
  $n = 0
  foreach ($name in $names) {
    $n = $n + 1
    if ($n -gt 1) {
      Say "waiting $WhoSeconds s for the /who cooldown before asking about $name"
      Start-Sleep -Seconds $WhoSeconds
    }
    Raise
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Start-Sleep -Milliseconds 900
    [System.Windows.Forms.SendKeys]::SendWait("/who $name")
    Start-Sleep -Milliseconds 900
    Shoot ("{0}-who{1:d2}-{2}-typed" -f $Label, $n, $name)
    [System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
    Say "asked /who $name"
    Start-Sleep -Seconds 12
    Raise
    Shoot ("{0}-who{1:d2}-{2}" -f $Label, $n, $name)
  }
}

Shoot "$Label-4-answer"
Say "the client is $(Alive) at the end of the run"
Get-Process $processName -ErrorAction SilentlyContinue | Stop-Process -Force
Say "closed the client"
