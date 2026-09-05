# bug-checklist §39 / 7.1 clause 15: log a real 3.3.5a client in from ANOTHER MACHINE.
#
# Runs in the Hyper-V host's console session (session 1, PK) as a scheduled task with
# /it, because a process started over ssh dies with the ssh session and CopyFromScreen
# from session 0 captures nothing. Shape taken from
# pyplan/gates/7.1-client-login/wowdrive.ps1 -- keybd_event with SCANCODES, which is what
# WoW's login screen reads.
#
# The realm is 172.30.55.119, the VM's own address on the Default Switch: no tunnel, no
# loopback. The server therefore sees this host's 172.30.48.1 as the client's address,
# which is the one thing a loopback tunnel could never show.

$ErrorActionPreference = 'Continue'
$CLIENT = 'C:\clients\WoW-WotLK-3.3.5a-min'
$OUT    = 'C:\clients\b39-shots'
$LOG    = 'C:\clients\b39-wowdrive.log'
$ACCOUNT = 'LANGATE'
$PASSWORD = 'langate1'

New-Item -ItemType Directory -Force -Path $OUT | Out-Null

function Log($msg) {
  $line = "{0} {1}" -f (Get-Date -Format 'yyyy-MM-ddTHH:mm:ssK'), $msg
  Add-Content -Path $LOG -Value $line -Encoding utf8
}

Log "=== b39 client login run starts. session id: $((Get-Process -Id $PID).SessionId)"

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

if (-not ("W32" -as [type])) {
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W32 {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int n);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern short VkKeyScan(char ch);
  [DllImport("user32.dll")] public static extern uint MapVirtualKey(uint uCode, uint uMapType);
  [DllImport("user32.dll", CharSet=CharSet.Unicode)] public static extern IntPtr FindWindow(string cls, string title);
}
"@
}

function WowProc { Get-Process -Name wow -ErrorAction SilentlyContinue | Select-Object -First 1 }

function WowHandle {
  # MainWindowHandle came back 0 on the first run (the client was sitting on a modal
  # "Missing or corrupted data" box), so the window is also looked up by class name.
  $p = WowProc
  if ($p -and $p.MainWindowHandle -ne 0) { return $p.MainWindowHandle }
  foreach ($cls in 'GxWindowClassD3d','GxWindowClass','GxWindowClassOpenGl') {
    $h = [W32]::FindWindow($cls, $null)
    if ($h -ne [IntPtr]::Zero) { Log "found window by class $cls -> $h"; return $h }
  }
  $h = [W32]::FindWindow($null, 'World of Warcraft')
  if ($h -ne [IntPtr]::Zero) { Log "found window by title -> $h"; return $h }
  return [IntPtr]::Zero
}

function Focus {
  $h = WowHandle
  if ($h -eq [IntPtr]::Zero) { return $false }
  [void][W32]::ShowWindow($h, 9)
  [void][W32]::SetForegroundWindow($h)
  Start-Sleep -Milliseconds 700
  return ([W32]::GetForegroundWindow() -eq $h)
}

function Shot($name) {
  $bmp = New-Object System.Drawing.Bitmap ([System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Width), ([System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen(0, 0, 0, 0, $bmp.Size)
  $path = Join-Path $OUT $name
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Log "shot -> $path ($((Get-Item $path).Length) bytes)"
}

function KeyVk([int]$vk, [int]$holdMs = 40) {
  $sc = [W32]::MapVirtualKey([uint32]$vk, 0)
  [W32]::keybd_event([byte]$vk, [byte]$sc, 0x0008, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds $holdMs
  [W32]::keybd_event([byte]$vk, [byte]$sc, 0x0008 -bor 0x0002, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds $holdMs
}

function TypeText($text) {
  foreach ($ch in $text.ToCharArray()) {
    $vks = [W32]::VkKeyScan($ch)
    $vk = $vks -band 0xFF
    $shift = ($vks -shr 8) -band 1
    if ($shift) { $sc = [W32]::MapVirtualKey(0x10, 0); [W32]::keybd_event(0x10, [byte]$sc, 0x0008, [UIntPtr]::Zero) }
    KeyVk $vk 45
    if ($shift) { $sc = [W32]::MapVirtualKey(0x10, 0); [W32]::keybd_event(0x10, [byte]$sc, 0x000A, [UIntPtr]::Zero) }
  }
}

$VK_RETURN = 0x0D; $VK_TAB = 0x09; $VK_ESC = 0x1B; $VK_BACK = 0x08

# --- the realm the client is told to use, written before launch ------------------
Log "realmlist.wtf: $(Get-Content (Join-Path $CLIENT 'Data\enUS\realmlist.wtf') -ErrorAction SilentlyContinue)"
Log "Config.wtf realmList: $((Select-String -Path (Join-Path $CLIENT 'WTF\Config.wtf') -Pattern 'realmList').Line)"

# --- launch ----------------------------------------------------------------------
Shot '00-desktop-before-launch.png'
$existing = WowProc
if ($existing) { Log "a wow.exe was already running (pid $($existing.Id)); killing it first"; $existing | Stop-Process -Force; Start-Sleep -Seconds 3 }

Log "launching $CLIENT\wow.exe"
$proc = Start-Process -FilePath (Join-Path $CLIENT 'wow.exe') -WorkingDirectory $CLIENT -PassThru
Log "started pid $($proc.Id)"

$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline) {
  Start-Sleep -Seconds 2
  $p = WowProc
  if ((WowHandle) -ne [IntPtr]::Zero) { break }
  if (-not $p) { Log "wow.exe is GONE while waiting for its window"; break }
}
$p = WowProc
if (-not $p) {
  Log "FAILED: no wow.exe process. Exit code was $($proc.ExitCode)"
  Shot '01-no-client.png'
  Log "=== run ends (client did not start)"
  exit 3
}
Log "window handle $(WowHandle), MainWindowHandle $($p.MainWindowHandle), title '$($p.MainWindowTitle)'"
Start-Sleep -Seconds 20
Log "focus: $(Focus)"
Shot '01-login-screen.png'

# --- type the account into the field that already has focus, then the password ---
# NO Escape here. The second run sent one "in case a dialog is up" and the login
# screen took it as Quit: the client was gone one second later and the remaining
# screenshots were all of the desktop (b39-wowdrive-run2.log, 01:28:26). On this
# screen the account field is focused the moment it draws (01-login-screen.png).
Log "focus before typing: $(Focus)"
for ($i = 0; $i -lt 20; $i++) { KeyVk $VK_BACK 25 }   # clear any prefilled account
TypeText $ACCOUNT
Start-Sleep -Milliseconds 400
Shot '03-account-typed.png'
KeyVk $VK_TAB; Start-Sleep -Milliseconds 400
TypeText $PASSWORD
Start-Sleep -Milliseconds 400
Shot '04-password-typed.png'
KeyVk $VK_RETURN
Log "sent Return"
Start-Sleep -Seconds 10
Shot '05-after-return.png'
Start-Sleep -Seconds 10
Shot '06-realm-or-characters.png'
# a realm list dialog wants one more Return to pick the highlighted realm
KeyVk $VK_RETURN
Start-Sleep -Seconds 12
Shot '07-after-second-return.png'
Start-Sleep -Seconds 8
Shot '08-final.png'

$connlog = Join-Path $CLIENT 'Logs\connection.log'
if (Test-Path $connlog) {
  Log "--- connection.log ($((Get-Item $connlog).Length) bytes)"
  Get-Content $connlog | ForEach-Object { Log "  $_" }
} else {
  Log "no connection.log at $connlog"
  Get-ChildItem (Join-Path $CLIENT 'Logs') -ErrorAction SilentlyContinue | ForEach-Object { Log "  Logs\$($_.Name) $($_.Length)" }
}

Log "=== run ends. wow.exe still running: $([bool](WowProc))"
