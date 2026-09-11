# T5 live half -- an interactive client agent on the Hyper-V host.
#
# The 8.6-panel-live agent, unchanged but for its root directory: the command
# protocol, the liveness stamp on every capture and the interactive-session
# requirement are that gate's measurements and this run inherits them rather
# than re-deriving them.
#
# Runs in the HOST's interactive session (started through `schtasks /it`, because a
# process started from ssh lands in session 0, which has no desktop: Wow.exe draws
# nothing there and SendKeys reaches nobody -- 8.3a's finding, restated in the
# 8.6 folder's client-party.ps1 header).
#
# It is a COMMAND LOOP, not a fixed script, because this gate has to CREATE a
# character rather than log an existing one in, and nobody on this tree has
# measured that sequence of screens: each step has to be looked at before the
# next one is chosen. Commands arrive as C:\gate86p\cmd-<n>.txt; each is
# executed and answered with C:\gate86p\done-<n>.txt.
#
# Every capture records whether Wow.exe is alive: a client that has DIED
# photographs exactly like a client refusing to show something.
$ErrorActionPreference = 'Continue'
$Root = 'C:\gateT5'
New-Item -ItemType Directory -Force -Path $Root | Out-Null
$log = Join-Path $Root 'agent.log'
Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class W {
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint x, uint y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
}
"@
function Say([string]$t) { Add-Content -Path $log -Value ("[{0}] {1}" -f (Get-Date -Format 'HH:mm:ss'), $t) -Encoding utf8 }
function Alive() {
  $p = Get-Process Wow -ErrorAction SilentlyContinue
  if ($null -eq $p) { return 'DEAD' }
  return ('alive pid=' + ($p | Select-Object -First 1).Id)
}
function Shoot([string]$name) {
  $state = Alive
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $Root "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $name -- Wow.exe $state"
  return $state
}
function Focus() {
  $p = Get-Process Wow -ErrorAction SilentlyContinue
  if ($p) { [W]::SetForegroundWindow(($p | Select-Object -First 1).MainWindowHandle) | Out-Null }
}
Say "agent up, screen $([System.Windows.Forms.Screen]::PrimaryScreen.Bounds)"
$deadline = (Get-Date).AddMinutes(240)
while ((Get-Date) -lt $deadline) {
  $cmds = Get-ChildItem -Path $Root -Filter 'cmd-*.txt' -ErrorAction SilentlyContinue | Sort-Object Name
  foreach ($c in $cmds) {
    $n = $c.BaseName -replace '^cmd-', ''
    $out = @()
    foreach ($line in (Get-Content $c.FullName)) {
      $line = $line.Trim()
      if (-not $line) { continue }
      $parts = $line -split '\s+', 2
      $verb = $parts[0]; $rest = if ($parts.Count -gt 1) { $parts[1] } else { '' }
      # A password must never reach this log. The 8.6-panel-live agent logged
      # every command line verbatim and its committed `client-agent.log` carries
      # `slow PANELGATE86` -- the account password, in the evidence. `secret`
      # types the same way and logs only how many characters it sent.
      if ($verb -eq 'secret') { Say "cmd $n : secret <$($rest.Length) chars>" } else { Say "cmd $n : $line" }
      switch ($verb) {
        'shoot'  { $out += "shoot $rest -- Wow.exe $(Shoot $rest)" }
        'click'  { $xy = $rest -split '\s+'; [W]::SetCursorPos([int]$xy[0], [int]$xy[1]) | Out-Null; Start-Sleep -Milliseconds 250; [W]::mouse_event(0x02,0,0,0,[IntPtr]::Zero); Start-Sleep -Milliseconds 80; [W]::mouse_event(0x04,0,0,0,[IntPtr]::Zero); $out += "clicked $rest" }
        'move'   { $xy = $rest -split '\s+'; [W]::SetCursorPos([int]$xy[0], [int]$xy[1]) | Out-Null; $out += "moved $rest" }
        'type'   { [System.Windows.Forms.SendKeys]::SendWait($rest); $out += "typed" }
        'slow'   { foreach ($ch in $rest.ToCharArray()) { [System.Windows.Forms.SendKeys]::SendWait($ch); Start-Sleep -Milliseconds 150 }; $out += "slow-typed" }
        'secret' { foreach ($ch in $rest.ToCharArray()) { [System.Windows.Forms.SendKeys]::SendWait($ch); Start-Sleep -Milliseconds 150 }; $out += "secret-typed $($rest.Length) chars" }
        'key'    { [System.Windows.Forms.SendKeys]::SendWait($rest); $out += "key $rest" }
        'sleep'  { Start-Sleep -Seconds ([int]$rest); $out += "slept $rest" }
        'focus'  { Focus; $out += "focused" }
        'alive'  { $out += "Wow.exe $(Alive)" }
        'start'  { Start-Process -FilePath $rest -WorkingDirectory (Split-Path $rest); $out += "started $rest" }
        'kill'   { Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force; $out += "killed" }
        'quit'   { $out += "quitting"; Set-Content -Path (Join-Path $Root "done-$n.txt") -Value ($out -join "`n") -Encoding utf8; Remove-Item $c.FullName -Force; Say "agent down"; return }
        default  { $out += "unknown verb $verb" }
      }
    }
    Set-Content -Path (Join-Path $Root "done-$n.txt") -Value ($out -join "`n") -Encoding utf8
    Remove-Item $c.FullName -Force
  }
  Start-Sleep -Milliseconds 700
}
Say "agent timed out"
