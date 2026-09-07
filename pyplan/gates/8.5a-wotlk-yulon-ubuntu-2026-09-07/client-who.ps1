# Log a real client in, enter the world, and ask the in-game who-list about a name.
#
# The same interactive-session rules as client-login.ps1: schtasks /it, because a
# process started from ssh lands in session 0 where there is no desktop.
param(
  [Parameter(Mandatory = $true)][string]$ClientDir,
  [Parameter(Mandatory = $true)][string]$Realmlist,
  [Parameter(Mandatory = $true)][string]$Account,
  [Parameter(Mandatory = $true)][string]$Password,
  [Parameter(Mandatory = $true)][string]$Who,
  [Parameter(Mandatory = $true)][string]$Label,
  [int]$WorldSeconds = 60
)

$ErrorActionPreference = 'Stop'
$out = Join-Path $env:USERPROFILE "client-login"
New-Item -ItemType Directory -Force -Path $out | Out-Null
$log = Join-Path $out "$Label.log"
Set-Content -Path $log -Value "" -Encoding utf8

function Say([string]$text) {
  Add-Content -Path $log -Value ("[{0}] {1}" -f (Get-Date -Format 'HH:mm:ssZ'), $text) -Encoding utf8
}

Add-Type -AssemblyName System.Windows.Forms
Add-Type -AssemblyName System.Drawing

function Shoot([string]$name) {
  $b = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
  $bmp = New-Object System.Drawing.Bitmap $b.Width, $b.Height
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($b.Location, [System.Drawing.Point]::Empty, $b.Size)
  $bmp.Save((Join-Path $out "$name.png"), [System.Drawing.Imaging.ImageFormat]::Png)
  $g.Dispose(); $bmp.Dispose()
  Say "photographed $name"
}

$wtf = Join-Path $ClientDir "WTF\Config.wtf"
$existing = if (Test-Path $wtf) { Get-Content $wtf | Where-Object { $_ -notmatch '^\s*SET\s+realmlist' } } else { @() }
Set-Content -Path $wtf -Value (@($existing) + @("SET realmlist `"$Realmlist`"")) -Encoding ascii
Say "realmlist -> $Realmlist"

Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Start-Sleep -Seconds 1
$proc = Start-Process -FilePath (Join-Path $ClientDir "Wow.exe") -WorkingDirectory $ClientDir -PassThru
Say "started pid $($proc.Id)"
$deadline = (Get-Date).AddSeconds(90)
while ((Get-Date) -lt $deadline -and $proc.MainWindowHandle -eq 0) { $proc.Refresh(); Start-Sleep -Milliseconds 500 }
Start-Sleep -Seconds 12

[System.Windows.Forms.SendKeys]::SendWait("^a")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Account)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("{TAB}")
Start-Sleep -Milliseconds 300
[System.Windows.Forms.SendKeys]::SendWait($Password)
Start-Sleep -Milliseconds 400
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "sent the login"
Start-Sleep -Seconds 18
Shoot "$Label-1-characters"

# Enter World: the character screen's default button, so ENTER presses it.
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "entering the world"
Start-Sleep -Seconds $WorldSeconds
Shoot "$Label-2-in-world"

# The who-list. ENTER opens the chat line; the command prints its answer there.
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Start-Sleep -Milliseconds 800
[System.Windows.Forms.SendKeys]::SendWait("/who $Who")
Start-Sleep -Milliseconds 800
[System.Windows.Forms.SendKeys]::SendWait("{ENTER}")
Say "asked /who $Who"
Start-Sleep -Seconds 10
Shoot "$Label-3-who"

Get-Process Wow -ErrorAction SilentlyContinue | Stop-Process -Force
Say "closed the client"
