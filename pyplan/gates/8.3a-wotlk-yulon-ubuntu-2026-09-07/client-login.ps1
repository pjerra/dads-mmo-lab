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

# 1. The realmlist. The 3.3.5a client reads WTF\Config.wtf, and a stale value
#    here is the single most common reason a gate "cannot reach the server".
$wtf = Join-Path $ClientDir "WTF\Config.wtf"
$dir = Split-Path $wtf
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
$existing = if (Test-Path $wtf) { Get-Content $wtf | Where-Object { $_ -notmatch '^\s*SET\s+realmlist' } } else { @() }
$kept = @($existing) + @("SET realmlist `"$Realmlist`"")
Set-Content -Path $wtf -Value $kept -Encoding ascii
Say "wrote realmlist into $wtf"

# 2. Launch, from the client's own directory: the client resolves Data\ relative
#    to its working directory and exits without a window otherwise.
Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$exe = Join-Path $ClientDir "Wow.exe"
$proc = Start-Process -FilePath $exe -WorkingDirectory $ClientDir -PassThru
Say "started Wow.exe as pid $($proc.Id)"

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

# 3. Wait for a window to exist before typing at it.
$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline) {
  $proc.Refresh()
  if ($proc.HasExited) { Say "the client exited before a window appeared"; break }
  if ($proc.MainWindowHandle -ne 0) { break }
  Start-Sleep -Milliseconds 500
}
Say "window handle: $($proc.MainWindowHandle)"
Start-Sleep -Seconds 12

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

# 4. Type it. The account field has focus on the 3.3.5a login screen; the
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
