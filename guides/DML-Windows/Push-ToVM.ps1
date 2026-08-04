<#
.SYNOPSIS
    Push the native installer (and optionally the launcher) into a Hyper-V VM.

.DESCRIPTION
    A GATE/TESTING helper, not part of the product. It exists because the Task 12
    leg-2 gate needs a clean VM, and a clean VM is exactly the machine you cannot
    conveniently copy files into: Hyper-V shares no clipboard in a basic session,
    and Enhanced Session Mode needs an RDP server the guest may not have (Windows
    11 Home has none).

    Copy-VMFile goes over the VM bus instead, so it needs no clipboard, no
    network in the guest, and no session mode.

    WHAT IT DOES
      1. Downloads the CURRENT Install-DML-Native.ps1 from the repo.
      2. Verifies it is not stale -- each marker is a fix the gate depends on,
         and a silent stale copy would make the gate test the wrong script.
      3. Copies it into the VM, with a tiny runner beside it.
      4. Optionally copies the built launcher installer too, if you point at one.

.PARAMETER VMName
    The Hyper-V VM to push into. It must be RUNNING: Copy-VMFile talks to the
    guest integration service, which does not exist on a stopped machine.

.PARAMETER LauncherSetup
    Path to 'DML Launcher_x.y.z_x64-setup.exe' if you have one built. Optional --
    the installer does not install the launcher, so until there is a GitHub
    Release this is the only way to get it onto a clean machine. Skipped silently
    when not supplied.

.PARAMETER Branch
    Repo branch to fetch from. Defaults to rust-main, the integration line.

.PARAMETER DestDir
    Where to land the files inside the guest.

.EXAMPLE
    .\Push-ToVM.ps1 -VMName DML-Gate2

.EXAMPLE
    .\Push-ToVM.ps1 -VMName DML-Gate2 -LauncherSetup "D:\dl\DML Launcher_0.1.0_x64-setup.exe"
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$VMName,
    [string]$LauncherSetup,
    [string]$Branch = 'rust-main',
    [string]$DestDir = 'C:\dml'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Say([string]$m, [string]$c = 'Gray') { Write-Host $m -ForegroundColor $c }
function Step([string]$m) { Write-Host ''; Write-Host "==> $m" -ForegroundColor White }

#Refuse before doing anything, rather than half-way through.
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Not elevated. Right-click PowerShell -> Run as Administrator.'
}

Step "Checking the VM"
$vm = Get-VM -Name $VMName -ErrorAction SilentlyContinue
if (-not $vm) { throw "No such VM: $VMName. Check 'Get-VM' for the exact name." }
if ($vm.State -ne 'Running') {
    #Copy-VMFile talks to the guest integration service, which only exists on a
    #running machine. Saying so beats the cmdlet's own generic failure.
    throw "VM '$VMName' is $($vm.State). Start it and log in, then re-run this."
}
Say "  $VMName is running" 'Green'

Step "Enabling the guest file-copy service"
#Idempotent. Off by default on many templates, and its absence is the usual
#cause of Copy-VMFile failing with an unhelpful message.
Enable-VMIntegrationService -VMName $VMName -Name 'Guest Service Interface' -ErrorAction SilentlyContinue
$svc = Get-VMIntegrationService -VMName $VMName -Name 'Guest Service Interface'
if (-not $svc.Enabled) { throw 'Could not enable the Guest Service Interface on the VM.' }
Say '  enabled' 'Green'
if (-not $svc.PrimaryStatusDescription -or $svc.PrimaryStatusDescription -ne 'OK') {
    #Enabled on the HOST side is not the same as running in the GUEST. Warn
    #rather than fail: it often settles a few seconds after boot.
    Say "  note: guest side reports '$($svc.PrimaryStatusDescription)' -- if the copy fails, wait for the guest to finish booting and retry." 'Yellow'
}

Step "Downloading the installer ($Branch)"
$staging = Join-Path $env:TEMP ("dml-push-" + [Guid]::NewGuid().ToString('N').Substring(0, 8))
New-Item -ItemType Directory -Force -Path $staging | Out-Null
$installer = Join-Path $staging 'Install-DML-Native.ps1'
$url = "https://raw.githubusercontent.com/pjerra/dads-mmo-lab/$Branch/guides/DML-Windows/Install-DML-Native.ps1"
Invoke-WebRequest -Uri $url -OutFile $installer -UseBasicParsing
Say ("  {0:N0} bytes" -f (Get-Item $installer).Length) 'Green'

Step "Verifying it is the current build"
#Each marker is a fix the gate depends on. A stale copy would not error -- it
#would quietly test a script that no longer exists, which is the failure mode
#this whole check exists to prevent.
$src = Get-Content -LiteralPath $installer -Raw
$markers = [ordered]@{
    'InstallGit'                    = 'installs Git itself'
    'VirtualizationFirmwareEnabled' = 'checks VT-x before the big download'
    'NoNewWindow'                   = 'winget progress reaches the user'
    'Register-Resume'               = 'auto-resumes after the reboot'
    'Invoke-RestartCountdown'       = 'counts down, then restarts'
}
foreach ($m in $markers.Keys) {
    if ($src -notmatch $m) { throw "Stale download: missing '$m' ($($markers[$m])). Wrong branch?" }
    Say "  ok  $m -- $($markers[$m])" 'DarkGray'
}

Step "Writing the runner"
#A tiny wrapper so the only thing typed inside the VM is one path. The flags
#matter: without them the installer only DETECTS Docker and Git, and a gate that
#stops to have you install prerequisites by hand proves less than it looks like.
$runner = Join-Path $staging 'Run-Gate.ps1'
@"
#Generated by Push-ToVM.ps1 -- run this ELEVATED.
`$ErrorActionPreference = 'Stop'
if (-not ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()
    ).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Not elevated. Right-click PowerShell -> Run as Administrator.'
}
Set-ExecutionPolicy -Scope Process Bypass -Force
& '$DestDir\Install-DML-Native.ps1' -InstallDocker -InstallGit
"@ | Set-Content -LiteralPath $runner -Encoding utf8
Say '  Run-Gate.ps1' 'Green'

Step "Copying into $VMName`:$DestDir"
$files = @(
    @{ Src = $installer; Name = 'Install-DML-Native.ps1' },
    @{ Src = $runner;    Name = 'Run-Gate.ps1' }
)
if ($LauncherSetup) {
    if (-not (Test-Path -LiteralPath $LauncherSetup)) { throw "LauncherSetup not found: $LauncherSetup" }
    $files += @{ Src = $LauncherSetup; Name = Split-Path $LauncherSetup -Leaf }
}

foreach ($f in $files) {
    $dest = Join-Path $DestDir $f.Name
    #-Force overwrites: without it a second run fails on every file that is
    #already there, which is exactly the case when you are iterating on a gate.
    Copy-VMFile -Name $VMName -SourcePath $f.Src -DestinationPath $dest `
                -CreateFullPath -FileSource Host -Force
    Say ("  {0,-40} {1:N0} bytes" -f $f.Name, (Get-Item -LiteralPath $f.Src).Length) 'Green'
}

Remove-Item -LiteralPath $staging -Recurse -Force -ErrorAction SilentlyContinue

Write-Host ''
Say '  Done. In the VM, open PowerShell as Administrator and run:' 'Green'
Say "      $DestDir\Run-Gate.ps1" 'White'
Write-Host ''
if (-not $LauncherSetup) {
    #Stated every time, because it is the step that ends the gate one short of a
    #server and there is no release to point at yet.
    Say '  NOTE: the launcher itself was NOT pushed. Install-DML-Native.ps1 prepares' 'Yellow'
    Say '  the PC but does not install the launcher, and there is no GitHub Release' 'Yellow'
    Say '  yet -- so re-run with -LauncherSetup <path to the built setup.exe> if you' 'Yellow'
    Say '  want the run to reach a working server.' 'Yellow'
}
