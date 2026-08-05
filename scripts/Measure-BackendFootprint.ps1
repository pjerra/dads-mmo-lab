<#
.SYNOPSIS
    Samples the resource footprint of ONE backend (Docker Desktop or Arch WSL)
    at ONE named phase, and writes a machine-readable record.

.DESCRIPTION
    This is the measurement half of the Docker-Desktop-vs-Arch-WSL comparison
    described in docs/backend-comparison-method.md. Read that first: several of
    the numbers below cannot be attributed to a backend at all, and the doc says
    which and why.

    This script is READ-ONLY with respect to the machine. It never starts or
    stops a server, an engine, a distro or the WSL VM, and it never writes to a
    database or to any server config. Its only writes are the result files.

    Changing machine state is the OPERATOR's job, by hand, using the run
    procedure in the method doc. This script's job is to REFUSE when the state
    is wrong, so that a mixed reading is never recorded as a clean one.

.PARAMETER Backend
    'desktop' (Docker Desktop on Windows) or 'arch' (dockerd inside dml-arch).

.PARAMETER Phase
    Which moment is being sampled:
      off      - nothing running at all. The machine's own baseline. Subtract
                 this from everything else.
      engine   - the backend's engine is up, NO server containers running.
      server   - the server is up and world-ready, bots online.
      stopped  - after the server was stopped (engine may still be up). Sampled
                 as a SERIES by default, because the interesting question is how
                 memory comes back over time, not where it is one second later.
      disk     - disk sizes only. Needs -QueryDocker to be useful.

.PARAMETER Label
    Free-text run label, e.g. '2026-08-05-quiet-box'. Groups samples that belong
    to one sitting. Defaults to the UTC date.

.PARAMETER ResultsDir
    Where records go. Default: <repo>/results/backend-comparison.

.PARAMETER SettleSeconds
    Delay before the FIRST sample, to let the machine settle after whatever the
    operator just did. Defaults per phase (see $script:DefaultSettle).

.PARAMETER Repeat
    Number of samples to take. Default 1, except phase 'stopped' which defaults
    to 6 (the reclaim curve).

.PARAMETER IntervalSeconds
    Gap between repeated samples. Default 30.

.PARAMETER QueryDocker
    Ask the docker engine for image/build-cache sizes and the container list.
    OFF BY DEFAULT AND THAT IS DELIBERATE: on this machine a merely read-only
    'docker context ls' was observed to WAKE the stopped docker-desktop distro
    and create a fresh WSL VM. Querying docker changes the thing being measured.
    Use it for phase 'disk', and for 'server' where the engine is up anyway.

.PARAMETER TimeReady
    Measure start -> world-ready wall clock instead of taking a footprint
    sample. Records T0 at invocation, so the operator runs this IMMEDIATELY
    before pressing Start, then triggers the start. Requires -QueryDocker for
    the authoritative log-marker answer.

.PARAMETER ReadyTimeoutSeconds
    Bound for -TimeReady. Default 2400 (40 min), which covers a cold boot with
    500 bots on a slow box.

.PARAMETER BotsOnline
    OPERATOR-REPORTED bot count for this sample. The harness cannot see this
    (it would need SOAP or MySQL), so it records what you tell it and labels it
    as your claim, not its measurement.

.PARAMETER Note
    Free text stored with the sample.

.PARAMETER Force
    Record the sample even though preconditions failed. The violations are
    stored in the record and the sample is flagged 'forced', so a forced
    reading can never be mistaken for a clean one later.

.EXAMPLE
    # Baseline, nothing running:
    .\Measure-BackendFootprint.ps1 -Backend desktop -Phase off -Label run1

.EXAMPLE
    # Engine up, no server:
    .\Measure-BackendFootprint.ps1 -Backend desktop -Phase engine -Label run1

.EXAMPLE
    # Time a start:
    .\Measure-BackendFootprint.ps1 -Backend arch -TimeReady -QueryDocker -Label run1

.NOTES
    PowerShell 5.1 compatible: no ternary, no ?? , no && / || .
    Exit codes: 0 = sample written. 2 = refused on preconditions. 3 = usage /
    environment error. 4 = -TimeReady timed out.
#>
[CmdletBinding()]
param(
    # Not declared Mandatory: -SelfTest runs without a backend, and a mandatory
    # parameter would try to PROMPT under a non-interactive host rather than
    # fail cleanly. Checked by hand below instead.
    [ValidateSet('desktop', 'arch')]
    [string]$Backend,

    # Run the precondition rules against synthetic machine states and exit.
    # Takes no measurements and touches nothing.
    [switch]$SelfTest,

    [ValidateSet('off', 'engine', 'server', 'stopped', 'disk')]
    [string]$Phase = 'engine',

    [string]$Label,

    [string]$ResultsDir,

    [int]$SettleSeconds = -1,

    [int]$Repeat = 0,

    [int]$IntervalSeconds = 30,

    [switch]$QueryDocker,

    [switch]$TimeReady,

    [int]$ReadyTimeoutSeconds = 2400,

    [int]$BotsOnline = -1,

    [string]$Note = '',

    [switch]$Force
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

# wsl.exe emits UTF-16LE by default, which PowerShell renders as text with a
# NUL between every character; every naive parse of it fails. WSL_UTF8=1 (WSL
# 0.64+) makes it emit UTF-8. Verified on WSL 2.7.10 on this box.
$env:WSL_UTF8 = '1'

# ---------------------------------------------------------------------------
# Constants
# ---------------------------------------------------------------------------

$script:SchemaVersion = 'dml-backend-footprint/1'

# The distro that IS each backend's engine.
$script:BackendDistro = @{
    'desktop' = 'docker-desktop'
    'arch'    = 'dml-arch'
}

# Windows-side process names belonging to Docker Desktop. Matched EXACTLY,
# case-insensitively, against Process.Name (which carries no .exe suffix).
$script:DesktopProcessNames = @(
    'Docker Desktop',
    'com.docker.backend',
    'com.docker.build',
    'com.docker.cli',
    'com.docker.dev-envs',
    'com.docker.extensions',
    'com.docker.proxy',
    'docker',
    'dockerd',
    'docker-index',
    'vpnkit',
    'vpnkit-bridge'
)

# Windows-side WSL plumbing. Present for BOTH backends (Docker Desktop uses WSL
# too), so this is not attributable to either on its own.
$script:WslProcessNames = @(
    'wsl', 'wslservice', 'wslhost', 'wslrelay', 'wslg', 'wslconfig'
)

# THE VM PROCESS, MATCHED BY EXACT NAME - NEVER BY 'vmmem*' WILDCARD.
# This machine also runs 'vmmemCmZygote' (806 MB private, PID with no readable
# start time, no executable path): a Windows container-manager VM that has
# nothing to do with WSL. A wildcard match silently adds it to whichever
# backend happens to be under test.
$script:VmProcessNames = @('vmmemWSL', 'vmmem')

# The ports this stack publishes. Same on both backends - WSL2 localhost
# forwarding puts the distro's published ports on the Windows 127.0.0.1 - which
# is exactly why only ONE backend can run a server at a time.
$script:StackPorts = @(3724, 8085, 7878)
# 3306 is watched but never used to refuse: it is remappable via the .env
# DOCKER_DB_EXTERNAL_PORT and collides with any local MySQL.
$script:WatchPorts = @(3306)

$script:DefaultSettle = @{
    'off' = 60; 'engine' = 60; 'server' = 120; 'stopped' = 30; 'disk' = 5
}

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

function Write-Section {
    param([string]$Text)
    Write-Host ''
    Write-Host "== $Text" -ForegroundColor Cyan
}

function ConvertTo-Mb {
    param($Bytes)
    if ($null -eq $Bytes) { return $null }
    return [math]::Round([double]$Bytes / 1MB, 1)
}

function Get-RepoRoot {
    # scripts/ lives directly under the repo root.
    return (Split-Path -Parent $PSScriptRoot)
}

<#
    Parse `wsl --list --verbose` into objects.

    Returns @{ Ok = $bool; Distros = @(...); Error = <string> }.

    Ok = $false means "WSL could not be asked", which is NOT the same as "no
    distros are running" - the tri-state discipline this repo uses everywhere.
    A caller that treats a failed probe as an empty list will happily certify a
    contaminated machine as clean.
#>
function Get-WslDistro {
    $result = @{ Ok = $false; Distros = @(); Error = '' }
    try {
        $raw = & wsl.exe --list --verbose 2>&1
        $code = $LASTEXITCODE
    } catch {
        $result.Error = "wsl.exe threw: $_"
        return $result
    }
    if ($code -ne 0) {
        $result.Error = "wsl --list --verbose exited $code : $($raw -join ' ')"
        return $result
    }

    $list = @()
    foreach ($line in @($raw)) {
        $t = ([string]$line).Trim()
        if ($t.Length -eq 0) { continue }
        # Header row, in whatever language Windows is set to; skip by shape:
        # it is the only row whose second column is not a known state AND which
        # contains 'NAME' in the invariant-culture upper form.
        if ($t.ToUpperInvariant() -match '^\*?\s*NAME\s') { continue }

        $isDefault = $t.StartsWith('*')
        if ($isDefault) { $t = $t.Substring(1).Trim() }

        $cols = $t -split '\s+'
        if ($cols.Count -lt 2) { continue }

        # Distro names can contain spaces; state and version are the LAST two
        # columns, so build the name from everything before them.
        $version = $cols[$cols.Count - 1]
        $state = $cols[$cols.Count - 2]
        $name = ($cols[0..($cols.Count - 3)] -join ' ')
        if ([string]::IsNullOrWhiteSpace($name)) { $name = $cols[0] }

        $list += [PSCustomObject]@{
            Name      = $name
            State     = $state
            Version   = $version
            IsDefault = $isDefault
        }
    }

    $result.Ok = $true
    $result.Distros = $list
    return $result
}

function Get-RunningDistroName {
    param($DistroProbe)
    if (-not $DistroProbe.Ok) { return @() }
    return @($DistroProbe.Distros | Where-Object { $_.State -eq 'Running' } | ForEach-Object { $_.Name })
}

<#
    The WSL utility VM process.

    ALL distros share ONE VM and ONE kernel, so this single process's memory
    covers every running distro at once. It is attributable to a backend ONLY
    when that backend's distro is the only one running - which is what the
    precondition check exists to establish.

    Pid + StartTime form a VM GENERATION marker. If two samples taken for
    DIFFERENT backends share a generation, they shared a VM instance, and the
    second one inherited the first one's high-water mark. Compare-BackendFootprint
    flags that.
#>
function Get-VmSample {
    $procs = @()
    foreach ($n in $script:VmProcessNames) {
        $p = Get-Process -Name $n -ErrorAction SilentlyContinue
        if ($p) { $procs += @($p) }
    }
    if ($procs.Count -eq 0) {
        return [PSCustomObject]@{
            Present = $false; Name = $null; Pid = $null; StartTimeUtc = $null
            Generation = $null; WorkingSetMB = 0; PrivateMB = 0
        }
    }
    # Expect exactly one. If more, sum them and let the caveat engine say so.
    $ws = 0; $pm = 0
    foreach ($p in $procs) { $ws += $p.WorkingSet64; $pm += $p.PrivateMemorySize64 }
    $first = $procs[0]
    $start = $null
    try { $start = $first.StartTime.ToUniversalTime().ToString('o') } catch { $start = $null }
    $gen = "$($first.Name)#$($first.Id)@$start"
    return [PSCustomObject]@{
        Present      = $true
        Name         = $first.Name
        Pid          = $first.Id
        StartTimeUtc = $start
        Generation   = $gen
        Count        = $procs.Count
        WorkingSetMB = (ConvertTo-Mb $ws)
        PrivateMB    = (ConvertTo-Mb $pm)
    }
}

function Get-ProcessGroupSample {
    param([string[]]$Names)
    $ws = 0; $pm = 0; $detail = @()
    foreach ($n in $Names) {
        $ps = Get-Process -Name $n -ErrorAction SilentlyContinue
        if (-not $ps) { continue }
        foreach ($p in @($ps)) {
            $ws += $p.WorkingSet64
            $pm += $p.PrivateMemorySize64
            $detail += [PSCustomObject]@{
                Name = $p.Name; Pid = $p.Id
                WorkingSetMB = (ConvertTo-Mb $p.WorkingSet64)
                PrivateMB = (ConvertTo-Mb $p.PrivateMemorySize64)
            }
        }
    }
    return [PSCustomObject]@{
        ProcessCount = $detail.Count
        WorkingSetMB = (ConvertTo-Mb $ws)
        PrivateMB    = (ConvertTo-Mb $pm)
        Processes    = $detail
    }
}

<#
    Machine-level memory. THIS IS THE HEADLINE NUMBER.

    Per-process working sets cannot honestly be summed for a VM-backed
    workload: the guest's memory is not fully visible as any one Windows
    process's working set, and adding Desktop's Windows processes to vmmem
    double-counts some pages while missing others. The machine-level figure
    sidesteps the whole argument - it asks how much of the box's RAM is in use,
    full stop - at the cost of including everything else running. That is why
    the 'off' baseline exists and why the box must be quiet.
#>
function Get-MachineMemorySample {
    $os = Get-CimInstance Win32_OperatingSystem
    $totalVisibleMB = [math]::Round($os.TotalVisibleMemorySize / 1KB, 1)
    $freePhysicalMB = [math]::Round($os.FreePhysicalMemory / 1KB, 1)

    $committedMB = $null
    $availableMB = $null
    try {
        $c = Get-Counter '\Memory\Committed Bytes', '\Memory\Available MBytes' -ErrorAction Stop
        foreach ($s in $c.CounterSamples) {
            if ($s.Path -like '*committed bytes*') { $committedMB = [math]::Round($s.CookedValue / 1MB, 1) }
            if ($s.Path -like '*available mbytes*') { $availableMB = [math]::Round($s.CookedValue, 1) }
        }
    } catch {
        # Performance counters can be disabled or corrupt. Say so rather than
        # substituting a different number under the same name.
        $committedMB = $null
        $availableMB = $null
    }

    $inUseMB = $null
    if ($null -ne $availableMB) {
        $inUseMB = [math]::Round($totalVisibleMB - $availableMB, 1)
    } else {
        $inUseMB = [math]::Round($totalVisibleMB - $freePhysicalMB, 1)
    }

    return [PSCustomObject]@{
        TotalVisibleMB   = $totalVisibleMB
        FreePhysicalMB   = $freePhysicalMB
        AvailableMB      = $availableMB
        CommittedBytesMB = $committedMB
        InUseMB          = $inUseMB
        CounterSource    = $(if ($null -ne $availableMB) { 'perfcounter' } else { 'cim-fallback' })
    }
}

function Test-TcpPort {
    param([int]$Port, [int]$TimeoutMs = 400)
    $client = New-Object System.Net.Sockets.TcpClient
    $open = $false
    try {
        $iar = $client.BeginConnect('127.0.0.1', $Port, $null, $null)
        if ($iar.AsyncWaitHandle.WaitOne($TimeoutMs)) {
            $open = $client.Connected
        }
    } catch {
        $open = $false
    } finally {
        try { $client.Close() } catch { }
    }
    return $open
}

function Get-PortSample {
    $h = @{}
    foreach ($p in ($script:StackPorts + $script:WatchPorts)) {
        $h["$p"] = (Test-TcpPort -Port $p)
    }
    return $h
}

function Test-AnyStackPortOpen {
    foreach ($p in $script:StackPorts) {
        if (Test-TcpPort -Port $p) { return $true }
    }
    return $false
}

<#
    Read .wslconfig. Every RAM figure in this comparison is conditioned on it:
    a 'memory=' cap means a loaded reading can be the CAP rather than the
    demand, and autoMemoryReclaim decides whether 'RAM returned after stop'
    means anything at all.
#>
function Get-WslConfigSample {
    $path = Join-Path $env:USERPROFILE '.wslconfig'
    $o = [PSCustomObject]@{
        Path = $path; Present = $false; MemoryCap = $null; Processors = $null
        Swap = $null; AutoMemoryReclaim = $null; VmIdleTimeoutMs = $null
        LocalhostForwarding = $null; Raw = $null
    }
    if (-not (Test-Path $path)) { return $o }
    $o.Present = $true
    $lines = Get-Content $path -ErrorAction SilentlyContinue
    $o.Raw = ($lines -join "`n")
    foreach ($l in $lines) {
        $t = ([string]$l).Trim()
        if ($t.StartsWith('#') -or $t.StartsWith(';')) { continue }
        if ($t -match '^\s*memory\s*=\s*(.+)$') { $o.MemoryCap = $Matches[1].Trim() }
        elseif ($t -match '^\s*processors\s*=\s*(.+)$') { $o.Processors = $Matches[1].Trim() }
        elseif ($t -match '^\s*swap\s*=\s*(.+)$') { $o.Swap = $Matches[1].Trim() }
        elseif ($t -match '^\s*autoMemoryReclaim\s*=\s*(.+)$') { $o.AutoMemoryReclaim = $Matches[1].Trim() }
        elseif ($t -match '^\s*vmIdleTimeout\s*=\s*(.+)$') { $o.VmIdleTimeoutMs = $Matches[1].Trim() }
        elseif ($t -match '^\s*localhostForwarding\s*=\s*(.+)$') { $o.LocalhostForwarding = $Matches[1].Trim() }
    }
    return $o
}

<#
    Per-distro virtual disk sizes, from the registry (never guessed from a
    path convention - docker-desktop's BasePath carries a \\?\ prefix and
    dml-arch lives at C:\DML\wsl, neither of which a convention would find).

    A .vhdx is SPARSE and is a HIGH-WATER MARK: it grows and does not shrink
    when files inside are deleted. It is therefore an honest answer to "how
    much disk has this cost me" and a dishonest answer to "how much is it using
    now". Both readings are reported and labelled.
#>
function Get-DistroDiskSample {
    $out = @()
    $key = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Lxss'
    if (-not (Test-Path $key)) { return $out }
    foreach ($sub in (Get-ChildItem $key -ErrorAction SilentlyContinue)) {
        $p = $null
        try { $p = Get-ItemProperty $sub.PSPath -ErrorAction Stop } catch { continue }
        if (-not $p.PSObject.Properties.Match('DistributionName').Count) { continue }
        $base = ''
        if ($p.PSObject.Properties.Match('BasePath').Count) { $base = [string]$p.BasePath }
        # Strip the \\?\ long-path prefix so Test-Path/Get-ChildItem behave.
        $clean = $base
        if ($clean.StartsWith('\\?\')) { $clean = $clean.Substring(4) }

        $files = @()
        $totalBytes = 0
        if ($clean -and (Test-Path $clean)) {
            foreach ($f in (Get-ChildItem $clean -Filter '*.vhdx' -ErrorAction SilentlyContinue)) {
                $files += [PSCustomObject]@{ Name = $f.Name; LogicalGB = [math]::Round($f.Length / 1GB, 2) }
                $totalBytes += $f.Length
            }
        }
        $out += [PSCustomObject]@{
            Distro       = [string]$p.DistributionName
            BasePath     = $base
            Resolved     = $clean
            Found        = ($files.Count -gt 0)
            VhdxTotalGB  = [math]::Round($totalBytes / 1GB, 2)
            Vhdx         = $files
        }
    }

    # Docker Desktop keeps the image store in a SEPARATE vhdx that belongs to no
    # distro registry entry. Missing it understates Desktop's disk by tens of GB.
    $ddData = Join-Path $env:LOCALAPPDATA 'Docker\wsl'
    if (Test-Path $ddData) {
        $files = @()
        $totalBytes = 0
        foreach ($f in (Get-ChildItem $ddData -Recurse -Filter '*.vhdx' -ErrorAction SilentlyContinue)) {
            $files += [PSCustomObject]@{ Name = $f.Name; Path = $f.FullName; LogicalGB = [math]::Round($f.Length / 1GB, 2) }
            $totalBytes += $f.Length
        }
        $out += [PSCustomObject]@{
            Distro      = '(docker-desktop data root)'
            BasePath    = $ddData
            Resolved    = $ddData
            Found       = ($files.Count -gt 0)
            VhdxTotalGB = [math]::Round($totalBytes / 1GB, 2)
            Vhdx        = $files
        }
    }
    return $out
}

<#
    Quote one argv element for the single Windows command line.

    ProcessStartInfo.ArgumentList - the API that takes a real argv and does
    this for you - DOES NOT EXIST on .NET Framework, which is what Windows
    PowerShell 5.1 runs on. It was added in .NET Core 2.1. This file declares
    itself 5.1-compatible, so reaching for it made every -QueryDocker sample
    die with "You cannot call a method on a null-valued expression" - a null
    property, not a missing method, so the failure named the symptom and not
    the cause. Measured on this box: PSVersion 5.1.26100.8875, CLR 4.0.30319,
    $psi.ArgumentList -eq $null.

    So the string is built here, by the Windows CommandLineToArgvW rules:
    quote when the value contains whitespace or a quote, double the run of
    backslashes that immediately precedes a quote, and escape embedded quotes.
    Nothing user-supplied crosses this boundary today (every caller passes our
    own literals plus a docker-emitted RFC3339 timestamp), but a helper that is
    only correct for its current callers is a trap for the next one.
#>
function ConvertTo-Win32Arg {
    param([string]$Value)
    if ($Value -eq '') { return '""' }
    if ($Value -notmatch '[\s"]') { return $Value }
    $sb = New-Object System.Text.StringBuilder
    $null = $sb.Append('"')
    $slashes = 0
    foreach ($ch in $Value.ToCharArray()) {
        if ($ch -eq '\') {
            $slashes++
            continue
        }
        if ($ch -eq '"') {
            $null = $sb.Append('\' * ($slashes * 2 + 1))
            $slashes = 0
            $null = $sb.Append('"')
            continue
        }
        if ($slashes -gt 0) { $null = $sb.Append('\' * $slashes); $slashes = 0 }
        $null = $sb.Append($ch)
    }
    if ($slashes -gt 0) { $null = $sb.Append('\' * ($slashes * 2)) }
    $null = $sb.Append('"')
    return $sb.ToString()
}

<#
    Build the argv for a docker call on the backend under test.

    Arch goes through the distro. --exec, never --, because 'wsl -- ' runs a
    SHELL: it splits on ';', expands $HOME and globs '*' against the cwd
    (verified 2026-07-28, recorded in CLAUDE.md). Nothing user-supplied crosses
    this boundary here, but the habit is the point.
#>
function Invoke-BackendDocker {
    param([string[]]$DockerArgs, [int]$TimeoutSec = 60)
    $exe = 'docker'
    $argv = @()
    if ($Backend -eq 'arch') {
        $exe = 'wsl.exe'
        $argv = @('-d', $script:BackendDistro['arch'], '-u', 'dml', '--exec', 'docker') + $DockerArgs
    } else {
        $argv = $DockerArgs
    }

    $psi = New-Object System.Diagnostics.ProcessStartInfo
    $psi.FileName = $exe
    $psi.Arguments = (($argv | ForEach-Object { ConvertTo-Win32Arg -Value $_ }) -join ' ')
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true

    $proc = [System.Diagnostics.Process]::Start($psi)
    # Read to completion FIRST, then wait: a child that fills the pipe buffer
    # while we block in WaitForExit deadlocks. Same lesson as
    # output_bounded_draining in status.rs.
    $stdout = $proc.StandardOutput.ReadToEndAsync()
    $stderr = $proc.StandardError.ReadToEndAsync()
    $exited = $proc.WaitForExit($TimeoutSec * 1000)
    if (-not $exited) {
        try { $proc.Kill() } catch { }
        return [PSCustomObject]@{ Ok = $false; TimedOut = $true; ExitCode = $null; Stdout = ''; Stderr = 'timed out' }
    }
    $o = $stdout.Result
    $e = $stderr.Result
    return [PSCustomObject]@{ Ok = ($proc.ExitCode -eq 0); TimedOut = $false; ExitCode = $proc.ExitCode; Stdout = $o; Stderr = $e }
}

function Get-DockerSample {
    $sample = [PSCustomObject]@{
        Queried = $true; Reachable = $false; Error = ''
        Containers = @(); AcContainerCount = 0
        ImagesSizeGB = $null; BuildCacheGB = $null; ContainersSizeGB = $null
        VolumesSizeGB = $null; ImagesReclaimableGB = $null
        ServerVersion = ''; Raw = ''
    }

    $ver = Invoke-BackendDocker -DockerArgs @('version', '--format', '{{.Server.Version}}') -TimeoutSec 45
    if (-not $ver.Ok) {
        $sample.Error = "docker version failed (exit $($ver.ExitCode)): $($ver.Stderr.Trim())"
        return $sample
    }
    $sample.Reachable = $true
    $sample.ServerVersion = $ver.Stdout.Trim()

    $ps = Invoke-BackendDocker -DockerArgs @('ps', '-a', '--format', '{{.Names}}|{{.State}}|{{.Image}}') -TimeoutSec 45
    if ($ps.Ok) {
        $rows = @()
        foreach ($l in ($ps.Stdout -split "`n")) {
            $t = $l.Trim()
            if ($t.Length -eq 0) { continue }
            $c = $t -split '\|'
            if ($c.Count -lt 2) { continue }
            $img = ''
            if ($c.Count -ge 3) { $img = $c[2] }
            $rows += [PSCustomObject]@{ Name = $c[0]; State = $c[1]; Image = $img }
        }
        $sample.Containers = $rows
        $sample.AcContainerCount = @($rows | Where-Object { $_.Name -like 'ac-*' -and $_.State -eq 'running' }).Count
    }

    # `system df -v` is heavy; the plain form gives the four totals we want.
    $df = Invoke-BackendDocker -DockerArgs @('system', 'df', '--format', '{{.Type}}|{{.Size}}|{{.Reclaimable}}') -TimeoutSec 120
    if ($df.Ok) {
        $sample.Raw = $df.Stdout.Trim()
        foreach ($l in ($df.Stdout -split "`n")) {
            $t = $l.Trim()
            if ($t.Length -eq 0) { continue }
            $c = $t -split '\|'
            if ($c.Count -lt 2) { continue }
            $gb = ConvertFrom-DockerSize $c[1]
            switch -Wildcard ($c[0]) {
                'Images*'      { $sample.ImagesSizeGB = $gb }
                'Containers*'  { $sample.ContainersSizeGB = $gb }
                'Local Volumes*' { $sample.VolumesSizeGB = $gb }
                'Build Cache*' { $sample.BuildCacheGB = $gb }
            }
        }
    }
    return $sample
}

function ConvertFrom-DockerSize {
    param([string]$Text)
    $t = ([string]$Text).Trim()
    if ($t -match '^([0-9]+(?:\.[0-9]+)?)\s*([KMGT]?i?B)$') {
        $n = [double]$Matches[1]
        switch ($Matches[2]) {
            'B'   { return [math]::Round($n / 1GB, 3) }
            'kB'  { return [math]::Round($n * 1KB / 1GB, 3) }
            'KB'  { return [math]::Round($n * 1KB / 1GB, 3) }
            'KiB' { return [math]::Round($n * 1KB / 1GB, 3) }
            'MB'  { return [math]::Round($n / 1KB, 3) }
            'MiB' { return [math]::Round($n / 1KB, 3) }
            'GB'  { return [math]::Round($n, 3) }
            'GiB' { return [math]::Round($n, 3) }
            'TB'  { return [math]::Round($n * 1KB, 3) }
            'TiB' { return [math]::Round($n * 1KB, 3) }
        }
    }
    return $null
}

<#
    Docker Desktop's THREE states, which the obvious probe conflates.

    'docker desktop status' answered 'running' on this box at a moment when the
    docker-desktop distro was Stopped and no engine could serve a request. It
    reports the APP, not the ENGINE. Treating it as an engine probe certifies a
    dead engine as live.

    So: app-running is the Windows process check, engine-running is the distro
    state, and they are reported separately.
#>
function Get-DesktopStateSample {
    param($DistroProbe)
    $procs = Get-ProcessGroupSample -Names $script:DesktopProcessNames
    $engineDistro = $null
    if ($DistroProbe.Ok) {
        $engineDistro = $DistroProbe.Distros | Where-Object { $_.Name -eq 'docker-desktop' } | Select-Object -First 1
    }
    $engineUp = $false
    if ($null -ne $engineDistro -and $engineDistro.State -eq 'Running') { $engineUp = $true }
    return [PSCustomObject]@{
        AppProcessCount = $procs.ProcessCount
        AppRunning      = ($procs.ProcessCount -gt 0)
        EngineDistroState = $(if ($null -ne $engineDistro) { $engineDistro.State } else { '(not registered)' })
        EngineUp        = $engineUp
        WindowsWorkingSetMB = $procs.WorkingSetMB
        WindowsPrivateMB    = $procs.PrivateMB
        Processes       = $procs.Processes
    }
}

# ---------------------------------------------------------------------------
# Preconditions
# ---------------------------------------------------------------------------

<#
    Refuse rather than produce a mixed reading.

    The rules, and why each one is here:

    * The OTHER backend must be genuinely OFF, not merely idle. Both stacks
      publish the same ports and use the same engine-global ac-* container
      names, so they cannot both host a server; and both live in the SAME WSL
      utility VM, so an idle-but-running other backend is still inside every
      memory number taken.

    * NO unrelated distro may be running. All distros share one VM. A throwaway
      gate distro left running adds its whole footprint to whichever backend is
      being measured, invisibly. (On the box this was written on, four distros
      were registered and a fifth was being created by another process.)

    * Phase must match observed reality. Phase 'engine' claims no server is
      running - but this stack's compose declares `restart: unless-stopped` on
      ac-database, ac-authserver and ac-worldserver, so merely starting the
      engine can bring the whole server up by itself. An 'engine' sample taken
      while the ports are open is a 'server' sample wearing the wrong label,
      and it would understate the difference between the backends by exactly
      the amount the comparison is trying to find.
#>
function Test-Precondition {
    param(
        $DistroProbe,
        $DesktopState,
        [string]$ForPhase,
        [string]$ForBackend,
        # $null = probe the live machine (production). Set only by -SelfTest, so
        # the port-mismatch refusals can be exercised without a running server.
        $PortsOpenOverride = $null
    )

    if ([string]::IsNullOrWhiteSpace($ForBackend)) { $ForBackend = $Backend }

    $violations = @()
    $expected = $script:BackendDistro[$ForBackend]
    $other = $script:BackendDistro[$(if ($ForBackend -eq 'desktop') { 'arch' } else { 'desktop' })]

    if (-not $DistroProbe.Ok) {
        $violations += "WSL could not be queried, so nothing about distro state is known: $($DistroProbe.Error). A probe that cannot answer is not a 'no'."
        return $violations
    }

    $running = @(Get-RunningDistroName -DistroProbe $DistroProbe)

    # --- the other backend must be off ---
    if ($running -contains $other) {
        $violations += "The other backend's distro '$other' is Running. Both backends share one WSL VM, so its memory is inside every number this sample would take."
    }
    if ($ForBackend -eq 'arch' -and $DesktopState.AppRunning) {
        $violations += "Docker Desktop's Windows processes are running ($($DesktopState.AppProcessCount) of them, $($DesktopState.WindowsWorkingSetMB) MB working set). For an Arch reading Desktop must be EXITED, not merely engine-stopped: quit it from the tray."
    }

    # --- no unrelated distro ---
    $allowed = @()
    if ($ForPhase -ne 'off') { $allowed = @($expected) }
    foreach ($r in $running) {
        if ($allowed -notcontains $r) {
            if ($r -eq $other) { continue }  # already reported above, do not double-count
            $violations += "Distro '$r' is Running and is not part of this measurement. All distros share ONE WSL utility VM; its memory is indistinguishable from the backend's. Terminate it: wsl --terminate $r"
        }
    }

    # --- the backend under test must be in the right state ---
    if ($ForPhase -eq 'off') {
        if ($running.Count -gt 0) {
            $violations += "Phase 'off' means nothing is running, but these distros are Running: $($running -join ', ')."
        }
        if ($DesktopState.AppRunning) {
            $violations += "Phase 'off' requires Docker Desktop to be exited, but $($DesktopState.AppProcessCount) of its processes are running."
        }
    } else {
        if ($running -notcontains $expected) {
            $violations += "Backend '$ForBackend' expects distro '$expected' to be Running; it is not. Nothing would be measured."
        }
        if ($ForBackend -eq 'desktop' -and -not $DesktopState.EngineUp) {
            $violations += "Docker Desktop's engine distro is '$($DesktopState.EngineDistroState)'. NB 'docker desktop status' reports the APP and says 'running' even then - it is not an engine probe."
        }
    }

    # --- phase must match the ports ---
    if ($null -ne $PortsOpenOverride) {
        $portsOpen = [bool]$PortsOpenOverride
    } else {
        $portsOpen = Test-AnyStackPortOpen
    }
    if ($ForPhase -eq 'engine' -and $portsOpen) {
        $violations += "Phase 'engine' claims no server is up, but a stack port (3724/8085/7878) is OPEN. This compose declares 'restart: unless-stopped', so starting the engine can start the server by itself. Stop the stack before sampling 'engine'."
    }
    if ($ForPhase -eq 'off' -and $portsOpen) {
        $violations += "Phase 'off' claims nothing is running, but a stack port is OPEN."
    }
    if ($ForPhase -eq 'server' -and -not $portsOpen) {
        $violations += "Phase 'server' expects the stack ports to be open; none of 3724/8085/7878 answered."
    }
    if ($ForPhase -eq 'stopped' -and $portsOpen) {
        $violations += "Phase 'stopped' expects the server to be down, but a stack port is still OPEN."
    }

    return $violations
}

# ---------------------------------------------------------------------------
# Caveat engine - what this sample cannot honestly claim
# ---------------------------------------------------------------------------

function Get-Caveat {
    param($Vm, $WslConfig, $Machine, $DistroProbe, $Docker)

    $c = @()

    $vmLabel = $Vm.Name
    if ([string]::IsNullOrWhiteSpace($vmLabel)) { $vmLabel = 'WSL utility VM' }
    $c += "ATTRIBUTION: all WSL distros share ONE utility VM and ONE Linux kernel. The '$vmLabel' figure is the WHOLE VM, not this backend's share of it. It is attributable to '$Backend' only because the preconditions established that no other distro was running."

    if ($Vm.Present -and $Vm.PSObject.Properties.Match('Count').Count -and $Vm.Count -gt 1) {
        $c += "More than one VM process matched ($($Vm.Count)). The memory figure is their sum and is NOT attributable to one backend."
    }

    if (-not $Vm.Present) {
        $c += "No WSL VM process was running at sample time, so the VM figure is 0 by absence, not by measurement. WSL tears the VM down entirely once the last distro stops (vmIdleTimeout), which is the only state in which 'all memory returned' is literally true."
    }

    if ($WslConfig.Present -and $WslConfig.MemoryCap) {
        $c += "A memory cap is set in .wslconfig (memory=$($WslConfig.MemoryCap)). Any loaded reading at or near this cap is the CAP, not the demand: the workload may want more and be unable to take it. Do not read a capped figure as 'this backend needs N GB'."
    }
    if ($WslConfig.Present -and $WslConfig.AutoMemoryReclaim) {
        $c += "autoMemoryReclaim=$($WslConfig.AutoMemoryReclaim) is set. Memory freed inside the VM returns to Windows GRADUALLY and on an undocumented schedule, so any single post-stop sample measures the reclaim schedule as much as the workload. Read the 'stopped' SERIES, never one point from it."
    }
    if ($WslConfig.Present -and -not $WslConfig.AutoMemoryReclaim) {
        $c += "autoMemoryReclaim is NOT set. The WSL VM does not return freed guest memory to Windows during its lifetime, so 'RAM returned after stop' will read as approximately zero regardless of how much the server actually freed. That is a property of WSL, not of the backend, and it is not a difference between the two backends."
    }

    if ($null -eq $Machine.CommittedBytesMB) {
        $c += "Performance counters could not be read; machine memory fell back to CIM FreePhysicalMemory. Committed-bytes comparisons across samples are unavailable."
    }

    $c += "The machine-level figures include EVERYTHING running on the box, not just this backend. They are only meaningful against an 'off' baseline taken on the same quiet machine in the same sitting."

    if ($null -ne $Docker -and $Docker.Queried) {
        $c += "Docker was queried during this sample. On this machine a read-only docker command was observed to WAKE a stopped docker-desktop distro and create a fresh WSL VM; if that happened here, the VM generation changed mid-measurement."
    }

    if ($DistroProbe.Ok) {
        $running = @(Get-RunningDistroName -DistroProbe $DistroProbe)
        $c += "Distros Running at sample time: $(if ($running.Count -gt 0) { $running -join ', ' } else { '(none)' })."
    }

    return $c
}

function Get-NotMeasured {
    $n = @()
    $n += "Per-distro memory. WSL2 runs every distro in ONE VM with ONE kernel, and /proc/meminfo is not namespaced, so a distro asked about memory reports the whole VM's. Two distros running at once cannot be told apart from Windows OR from inside. The harness handles this by REFUSING to sample when more than one is up, not by splitting the number."
    $n += "The kernel's own share (page cache, slab, tmpfs) within the VM. It is real memory the backend causes to be used, but it belongs to the shared kernel and cannot be assigned to a distro."
    $n += "Bots actually online. That needs SOAP or MySQL; -BotsOnline records the OPERATOR's claim and is not a measurement."
    $n += "Disk actually freed. A .vhdx never shrinks, so the disk figures are high-water marks; 'docker system df' gives the engine's logical view, which is the comparable one."
    $n += "Docker Desktop's Windows-side memory in the Arch column: it is zero only because Desktop is required to be exited. Any user who keeps Desktop installed and running for other work pays it on both backends."
    return $n
}

# ---------------------------------------------------------------------------
# Readiness timing
# ---------------------------------------------------------------------------

<#
    start -> world-ready wall clock.

    TWO answers are recorded, because the cheap one lies:

    * TcpOpenSeconds - when 127.0.0.1:8085 first accepted a connection. This is
      NECESSARY but NOT SUFFICIENT: docker's userland proxy binds and accepts
      the moment the container starts, long before the worldserver has finished
      loading. Taken alone it would report a 30-minute boot as a 5-second one.

    * MarkerSeconds - when the worldserver logged AzerothCore's boot-complete
      marker. This is the AUTHORITATIVE answer and is exactly the definition
      the product itself uses: crates/dml-wow/src/status.rs::world_ready does
      `docker inspect -f {{.State.StartedAt}} ac-worldserver` then
      `docker logs --since <started> ac-worldserver` and scans case-insensitively
      for "world initialized in". Mirrored here so the number in the comparison
      and the number the launcher reports mean the same thing.

    Both are stored so the gap between them is visible.
#>
function Measure-WorldReady {
    $t0 = Get-Date
    Write-Host "T0 = $($t0.ToUniversalTime().ToString('o'))"
    Write-Host "Trigger the start NOW. Polling for world-ready (bound $ReadyTimeoutSeconds s)..."

    $tcpOpen = $null
    $marker = $null
    $lastNote = Get-Date

    while ($true) {
        $elapsed = ((Get-Date) - $t0).TotalSeconds
        if ($elapsed -gt $ReadyTimeoutSeconds) { break }

        if ($null -eq $tcpOpen) {
            if (Test-TcpPort -Port 8085 -TimeoutMs 300) {
                $tcpOpen = [math]::Round($elapsed, 1)
                Write-Host "  port 8085 accepted at ${tcpOpen}s (NOT world-ready - docker's proxy accepts early)"
            }
        }

        if ($QueryDocker) {
            $ins = Invoke-BackendDocker -DockerArgs @('inspect', '-f', '{{.State.StartedAt}}', 'ac-worldserver') -TimeoutSec 20
            if ($ins.Ok) {
                $started = $ins.Stdout.Trim()
                if ($started -and $started -ne '<no value>') {
                    $logs = Invoke-BackendDocker -DockerArgs @('logs', '--since', $started, 'ac-worldserver') -TimeoutSec 45
                    $text = ($logs.Stdout + "`n" + $logs.Stderr)
                    if ($text.ToLowerInvariant().Contains('world initialized in')) {
                        $marker = [math]::Round(((Get-Date) - $t0).TotalSeconds, 1)
                        Write-Host "  world-ready marker at ${marker}s" -ForegroundColor Green
                        break
                    }
                }
            }
        }

        if (((Get-Date) - $lastNote).TotalSeconds -ge 30) {
            Write-Host ("  ... {0:N0}s elapsed" -f $elapsed)
            $lastNote = Get-Date
        }
        Start-Sleep -Seconds 5
    }

    $method = 'tcp-8085-only (NOT authoritative)'
    if ($QueryDocker) { $method = 'docker logs marker "world initialized in" (matches status.rs::world_ready)' }

    return [PSCustomObject]@{
        Measured        = $true
        T0Utc           = $t0.ToUniversalTime().ToString('o')
        TcpOpenSeconds  = $tcpOpen
        MarkerSeconds   = $marker
        TimedOut        = ($null -eq $marker -and $QueryDocker)
        Method          = $method
        TimeoutSeconds  = $ReadyTimeoutSeconds
    }
}

# ---------------------------------------------------------------------------
# Self-test
# ---------------------------------------------------------------------------

<#
    Exercises the REAL Test-Precondition against synthetic machine states.

    This exists because the states that matter most cannot be produced on
    demand: proving that a stray distro triggers a refusal would otherwise mean
    booting one, and proving the 'engine phase but the server auto-started'
    refusal would mean starting the user's server. Both are exactly what the
    harness must never do.

    It calls the production function, never a restatement of its rules - the
    lesson from lifecycle_steps_for_mode, whose four tests stayed green while
    the real ordering rotted, because they asserted against a pure list nothing
    read.

    Anti-vacuity: case 1 asserts ZERO violations. Without it every other case
    would still pass if Test-Precondition were mutated to refuse everything.
#>
function Invoke-SelfTest {
    $pass = 0
    $fail = 0

    function New-Probe {
        param([string[]]$Running, [string[]]$Stopped = @(), [bool]$Ok = $true, [string]$Err = '')
        $d = @()
        foreach ($n in $Running) { $d += [PSCustomObject]@{ Name = $n; State = 'Running'; Version = '2'; IsDefault = $false } }
        foreach ($n in $Stopped) { $d += [PSCustomObject]@{ Name = $n; State = 'Stopped'; Version = '2'; IsDefault = $false } }
        return @{ Ok = $Ok; Distros = $d; Error = $Err }
    }
    function New-Desktop {
        param([bool]$AppRunning, [string]$EngineState)
        return [PSCustomObject]@{
            AppProcessCount = $(if ($AppRunning) { 8 } else { 0 })
            AppRunning = $AppRunning
            EngineDistroState = $EngineState
            EngineUp = ($EngineState -eq 'Running')
            WindowsWorkingSetMB = $(if ($AppRunning) { 700 } else { 0 })
            WindowsPrivateMB = 0
            Processes = @()
        }
    }

    function Assert-Case {
        param(
            [string]$Name, $Probe, $Desktop, [string]$Phase, [string]$ForBackend,
            $PortsOpen, [int]$ExpectCount = -1, [string[]]$ExpectMatch = @()
        )
        $v = @(Test-Precondition -DistroProbe $Probe -DesktopState $Desktop -ForPhase $Phase -ForBackend $ForBackend -PortsOpenOverride $PortsOpen)
        $ok = $true
        $why = @()
        if ($ExpectCount -ge 0 -and $v.Count -ne $ExpectCount) {
            $ok = $false; $why += "expected $ExpectCount violation(s), got $($v.Count)"
        }
        foreach ($m in $ExpectMatch) {
            if (-not (@($v | Where-Object { $_ -like "*$m*" }).Count -gt 0)) {
                $ok = $false; $why += "no violation mentioned '$m'"
            }
        }
        if ($ok) {
            $script:stPass++
            Write-Host "  PASS  $Name" -ForegroundColor Green
        } else {
            $script:stFail++
            Write-Host "  FAIL  $Name" -ForegroundColor Red
            foreach ($w in $why) { Write-Host "          $w" -ForegroundColor Red }
            foreach ($x in $v) { Write-Host "          got: $x" -ForegroundColor DarkGray }
        }
    }

    $script:stPass = 0
    $script:stFail = 0

    Write-Section 'Self-test: precondition rules (synthetic states, real function)'

    # 1. THE ANTI-VACUITY CASE. A genuinely clean arch machine must produce
    #    ZERO violations. If this fails, every refusal below proves nothing.
    Assert-Case -Name 'clean arch machine passes with no violations' `
        -Probe (New-Probe -Running @('dml-arch') -Stopped @('docker-desktop')) `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'engine' -ForBackend 'arch' -PortsOpen $false -ExpectCount 0

    # 2. Same for a clean desktop machine.
    Assert-Case -Name 'clean desktop machine passes with no violations' `
        -Probe (New-Probe -Running @('docker-desktop') -Stopped @('dml-arch')) `
        -Desktop (New-Desktop -AppRunning $true -EngineState 'Running') `
        -Phase 'engine' -ForBackend 'desktop' -PortsOpen $false -ExpectCount 0

    # 3. THE CONTAMINATION CASE the whole comparison turns on: a stray distro
    #    left running by earlier work shares the VM and is inside every number.
    Assert-Case -Name 'stray distro (dml-arch-gate2) is refused by name' `
        -Probe (New-Probe -Running @('dml-arch', 'dml-arch-gate2')) `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'engine' -ForBackend 'arch' -PortsOpen $false `
        -ExpectMatch @('dml-arch-gate2', 'share ONE WSL utility VM')

    # 4. The other backend running at the same time.
    Assert-Case -Name 'other backend distro running is refused' `
        -Probe (New-Probe -Running @('dml-arch', 'docker-desktop')) `
        -Desktop (New-Desktop -AppRunning $true -EngineState 'Running') `
        -Phase 'engine' -ForBackend 'arch' -PortsOpen $false `
        -ExpectMatch @("distro 'docker-desktop' is Running", 'Docker Desktop''s Windows processes')

    # 5. Desktop merely engine-stopped is NOT off, for an arch reading.
    Assert-Case -Name 'Desktop app still running is refused for an arch reading' `
        -Probe (New-Probe -Running @('dml-arch') -Stopped @('docker-desktop')) `
        -Desktop (New-Desktop -AppRunning $true -EngineState 'Stopped') `
        -Phase 'engine' -ForBackend 'arch' -PortsOpen $false `
        -ExpectMatch @('must be EXITED')

    # 6. THE restart:unless-stopped CASE - 'engine' phase with the server up.
    Assert-Case -Name 'engine phase with stack ports open is refused' `
        -Probe (New-Probe -Running @('dml-arch')) `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'engine' -ForBackend 'arch' -PortsOpen $true `
        -ExpectMatch @('restart', 'unless-stopped')

    # 7. 'server' phase with nothing listening is a mislabelled sample.
    Assert-Case -Name 'server phase with no ports open is refused' `
        -Probe (New-Probe -Running @('dml-arch')) `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'server' -ForBackend 'arch' -PortsOpen $false `
        -ExpectMatch @("Phase 'server' expects")

    # 8. 'stopped' phase while the server is still listening.
    Assert-Case -Name 'stopped phase with ports still open is refused' `
        -Probe (New-Probe -Running @('dml-arch')) `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'stopped' -ForBackend 'arch' -PortsOpen $true `
        -ExpectMatch @("Phase 'stopped' expects")

    # 9. 'off' baseline must be genuinely empty.
    Assert-Case -Name 'off phase with a distro running is refused' `
        -Probe (New-Probe -Running @('dml-arch')) `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'off' -ForBackend 'arch' -PortsOpen $false `
        -ExpectMatch @("Phase 'off' means nothing is running")

    # 10. Truly off passes - the other half of anti-vacuity for phase 'off'.
    Assert-Case -Name 'off phase with nothing running passes' `
        -Probe (New-Probe -Stopped @('dml-arch', 'docker-desktop')) `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'off' -ForBackend 'arch' -PortsOpen $false -ExpectCount 0

    # 11. TRI-STATE: WSL failing to answer must refuse, never read as 'nothing
    #     is running'. A probe that cannot answer is not a 'no'.
    Assert-Case -Name 'unqueryable WSL refuses rather than assuming a clean box' `
        -Probe (New-Probe -Ok $false -Err 'wsl exited 1') `
        -Desktop (New-Desktop -AppRunning $false -EngineState 'Stopped') `
        -Phase 'engine' -ForBackend 'arch' -PortsOpen $false `
        -ExpectCount 1 -ExpectMatch @('not a ''no''')

    # 12. Desktop engine down is named, with the docker-desktop-status trap.
    Assert-Case -Name 'desktop engine stopped names the status-command trap' `
        -Probe (New-Probe -Stopped @('docker-desktop', 'dml-arch')) `
        -Desktop (New-Desktop -AppRunning $true -EngineState 'Stopped') `
        -Phase 'engine' -ForBackend 'desktop' -PortsOpen $false `
        -ExpectMatch @('reports the APP')

    Write-Host ''
    Write-Host ("Self-test: {0} passed, {1} failed" -f $script:stPass, $script:stFail) -ForegroundColor $(if ($script:stFail -eq 0) { 'Green' } else { 'Red' })

    # Prove the VM-process name list excludes the container-manager VM. A
    # 'vmmem*' wildcard would silently add vmmemCmZygote (measured 806 MB on
    # the box this was written for) to whichever backend was under test.
    Write-Section 'Self-test: VM process name list'
    if ($script:VmProcessNames -contains 'vmmemCmZygote') {
        Write-Host '  FAIL  vmmemCmZygote is in the VM name list; it is not a WSL VM.' -ForegroundColor Red
        $script:stFail++
    } else {
        Write-Host '  PASS  vmmemCmZygote is not counted as a WSL VM.' -ForegroundColor Green
        $script:stPass++
    }
    foreach ($n in $script:VmProcessNames) {
        if ($n -like '*`**') {
            Write-Host "  FAIL  '$n' is a wildcard; VM processes must be matched by exact name." -ForegroundColor Red
            $script:stFail++
        }
    }

    # Argv quoting. ProcessStartInfo.ArgumentList - which would have done this
    # for us - is absent on .NET Framework, so Windows PowerShell 5.1 builds the
    # command line by hand and every -QueryDocker sample depends on it being
    # right. Case 1 is the ANTI-VACUITY case: it asserts that a plain value is
    # passed through UNQUOTED, so a helper mutated to quote everything (or to
    # return its input unchanged) cannot leave this block green.
    Write-Section 'Self-test: argv quoting (ConvertTo-Win32Arg)'
    $argCases = @(
        @{ In = 'version';                Out = 'version';                Why = 'plain value is not quoted' },
        @{ In = '{{.Names}}|{{.State}}';  Out = '{{.Names}}|{{.State}}';  Why = 'docker --format strings pass through untouched' },
        @{ In = '';                       Out = '""';                     Why = 'empty argument survives as an empty argv slot' },
        @{ In = 'a b';                    Out = '"a b"';                  Why = 'whitespace forces quoting' },
        @{ In = 'a"b';                    Out = '"a\"b"';                 Why = 'embedded quote is escaped' },
        @{ In = 'a\"b';                   Out = '"a\\\"b"';               Why = 'backslash run before a quote is doubled' },
        @{ In = 'a b\';                   Out = '"a b\\"';                Why = 'trailing backslash is doubled so it does not escape the closing quote' }
    )
    foreach ($c in $argCases) {
        $got = ConvertTo-Win32Arg -Value $c.In
        if ($got -ceq $c.Out) {
            $script:stPass++
            Write-Host "  PASS  $($c.Why)" -ForegroundColor Green
        } else {
            $script:stFail++
            Write-Host "  FAIL  $($c.Why)" -ForegroundColor Red
            Write-Host "          in='$($c.In)' expected='$($c.Out)' got='$got'" -ForegroundColor Red
        }
    }

    if ($script:stFail -gt 0) { return 1 }
    return 0
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

if ($SelfTest) {
    $rc = Invoke-SelfTest
    exit $rc
}

if ([string]::IsNullOrWhiteSpace($Backend)) {
    Write-Host "-Backend is required (desktop|arch). Use -SelfTest to run the rule checks without one." -ForegroundColor Red
    exit 3
}

if ([string]::IsNullOrWhiteSpace($Label)) {
    $Label = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
}
if ([string]::IsNullOrWhiteSpace($ResultsDir)) {
    $ResultsDir = Join-Path (Get-RepoRoot) 'results\backend-comparison'
}
if ($SettleSeconds -lt 0) { $SettleSeconds = $script:DefaultSettle[$Phase] }
if ($Repeat -le 0) {
    if ($Phase -eq 'stopped') { $Repeat = 6 } else { $Repeat = 1 }
}
if ($TimeReady) { $Repeat = 1 }

if (-not (Test-Path $ResultsDir)) {
    $null = New-Item -ItemType Directory -Path $ResultsDir -Force
}

$runId = "$Label-$Backend-$Phase-" + (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')

Write-Section "DML backend footprint - $Backend / $Phase"
Write-Host "run id     : $runId"
Write-Host "results    : $ResultsDir"
Write-Host "settle     : $SettleSeconds s   samples: $Repeat  every $IntervalSeconds s"
Write-Host "docker     : $(if ($QueryDocker) { 'WILL BE QUERIED (this can wake a stopped distro)' } else { 'not queried' })"

# --- state probes (cheap, before settling, so a refusal is instant) ---
$distroProbe = Get-WslDistro
$desktopState = Get-DesktopStateSample -DistroProbe $distroProbe
$wslConfig = Get-WslConfigSample

Write-Section 'Observed state'
if ($distroProbe.Ok) {
    foreach ($d in $distroProbe.Distros) {
        Write-Host ("  distro {0,-20} {1}" -f $d.Name, $d.State)
    }
} else {
    Write-Host "  WSL could not be queried: $($distroProbe.Error)" -ForegroundColor Yellow
}
Write-Host ("  Docker Desktop app      : {0} ({1} procs, {2} MB WS)" -f $desktopState.AppRunning, $desktopState.AppProcessCount, $desktopState.WindowsWorkingSetMB)
Write-Host ("  Docker Desktop engine   : {0}" -f $desktopState.EngineDistroState)
$portsNow = Get-PortSample
Write-Host ("  stack ports open        : {0}" -f (@($portsNow.GetEnumerator() | Where-Object { $_.Value } | ForEach-Object { $_.Key }) -join ', '))

# --- preconditions ---
$phaseForCheck = $Phase
if ($TimeReady) { $phaseForCheck = 'engine' }   # a timed start begins with no server up
$violations = @(Test-Precondition -DistroProbe $distroProbe -DesktopState $desktopState -ForPhase $phaseForCheck -ForBackend $Backend)

Write-Section 'Preconditions'
if ($violations.Count -eq 0) {
    Write-Host '  OK - state matches the declared backend and phase.' -ForegroundColor Green
} else {
    foreach ($v in $violations) { Write-Host "  REFUSE: $v" -ForegroundColor Red }
    if (-not $Force) {
        Write-Host ''
        Write-Host 'Refusing to sample. A reading taken now would blend backends or mislabel a phase,' -ForegroundColor Red
        Write-Host 'and a wrong number is worse than no number. Fix the state (see' -ForegroundColor Red
        Write-Host 'docs/backend-comparison-method.md, "Run procedure") and re-run.' -ForegroundColor Red
        Write-Host 'Use -Force only to record a deliberately dirty sample; it is flagged as such.' -ForegroundColor DarkGray
        exit 2
    }
    Write-Host '  -Force given: sampling anyway. The record is flagged forced.' -ForegroundColor Yellow
}

# --- settle ---
if ($SettleSeconds -gt 0) {
    Write-Section "Settling $SettleSeconds s"
    Start-Sleep -Seconds $SettleSeconds
}

# --- readiness timing branch ---
$readySample = $null
if ($TimeReady) {
    Write-Section 'Timing start -> world-ready'
    $readySample = Measure-WorldReady
}

# --- samples ---
$records = @()
for ($i = 1; $i -le $Repeat; $i++) {
    if ($i -gt 1) {
        Start-Sleep -Seconds $IntervalSeconds
    }

    $machine = Get-MachineMemorySample
    $vm = Get-VmSample
    $desktopNow = Get-DesktopStateSample -DistroProbe (Get-WslDistro)
    $wslPlumbing = Get-ProcessGroupSample -Names $script:WslProcessNames
    $distroNow = Get-WslDistro
    $ports = Get-PortSample

    $docker = $null
    if ($QueryDocker) { $docker = Get-DockerSample }

    $disk = $null
    if ($Phase -eq 'disk' -or $i -eq 1) { $disk = Get-DistroDiskSample }

    $rec = [PSCustomObject]@{
        schema         = $script:SchemaVersion
        runId          = $runId
        label          = $Label
        backend        = $Backend
        phase          = $Phase
        sampleIndex    = $i
        sampleCount    = $Repeat
        timestampUtc   = (Get-Date).ToUniversalTime().ToString('o')
        elapsedFromFirstSec = $(if ($i -eq 1) { 0 } else { ($i - 1) * $IntervalSeconds })
        host           = [PSCustomObject]@{
            machine     = $env:COMPUTERNAME
            wslConfig   = $wslConfig
        }
        machine        = $machine
        vm             = $vm
        desktopWindows = $desktopNow
        wslWindows     = $wslPlumbing
        distros        = $(if ($distroNow.Ok) { $distroNow.Distros } else { @() })
        distroProbeOk  = $distroNow.Ok
        ports          = $ports
        docker         = $docker
        disk           = $disk
        ready          = $readySample
        botsOnlineOperatorReported = $BotsOnline
        note           = $Note
        preconditions  = [PSCustomObject]@{
            passed     = ($violations.Count -eq 0)
            forced     = [bool]$Force
            violations = $violations
        }
        caveats        = (Get-Caveat -Vm $vm -WslConfig $wslConfig -Machine $machine -DistroProbe $distroNow -Docker $docker)
        notMeasured    = (Get-NotMeasured)
    }

    $records += $rec

    Write-Section "Sample $i/$Repeat"
    Write-Host ("  machine in use   : {0,10:N1} MB   (available {1:N1} MB of {2:N1} MB)" -f $machine.InUseMB, $machine.AvailableMB, $machine.TotalVisibleMB)
    Write-Host ("  machine committed: {0,10:N1} MB" -f $machine.CommittedBytesMB)
    if ($vm.Present) {
        Write-Host ("  WSL VM ({0,-8}): {1,10:N1} MB working set / {2:N1} MB private   gen={3}" -f $vm.Name, $vm.WorkingSetMB, $vm.PrivateMB, $vm.Generation)
    } else {
        Write-Host '  WSL VM           :        (absent - no distro running, VM torn down)'
    }
    Write-Host ("  Desktop (Windows): {0,10:N1} MB working set over {1} procs" -f $desktopNow.WindowsWorkingSetMB, $desktopNow.AppProcessCount)
    Write-Host ("  WSL plumbing     : {0,10:N1} MB working set over {1} procs" -f $wslPlumbing.WorkingSetMB, $wslPlumbing.ProcessCount)
    if ($null -ne $docker) {
        if ($docker.Reachable) {
            Write-Host ("  docker           : v{0}, {1} ac-* running, images {2} GB, build cache {3} GB" -f $docker.ServerVersion, $docker.AcContainerCount, $docker.ImagesSizeGB, $docker.BuildCacheGB)
        } else {
            Write-Host ("  docker           : UNREACHABLE - {0}" -f $docker.Error) -ForegroundColor Yellow
        }
    }
}

# ---------------------------------------------------------------------------
# Write results
# ---------------------------------------------------------------------------

$utf8NoBom = New-Object System.Text.UTF8Encoding($false)

$jsonPath = Join-Path $ResultsDir "$runId.json"
$json = $records | ConvertTo-Json -Depth 12
[System.IO.File]::WriteAllText($jsonPath, $json, $utf8NoBom)

# Flat CSV, appended, so two runs diff without retyping anything.
$csvPath = Join-Path $ResultsDir 'samples.csv'
$csvRows = @()
foreach ($r in $records) {
    $csvRows += [PSCustomObject]@{
        runId              = $r.runId
        label              = $r.label
        backend            = $r.backend
        phase              = $r.phase
        sampleIndex        = $r.sampleIndex
        elapsedSec         = $r.elapsedFromFirstSec
        timestampUtc       = $r.timestampUtc
        machineInUseMB     = $r.machine.InUseMB
        machineCommittedMB = $r.machine.CommittedBytesMB
        machineAvailableMB = $r.machine.AvailableMB
        vmPresent          = $r.vm.Present
        vmWorkingSetMB     = $r.vm.WorkingSetMB
        vmPrivateMB        = $r.vm.PrivateMB
        vmGeneration       = $r.vm.Generation
        desktopWinWsMB     = $r.desktopWindows.WindowsWorkingSetMB
        desktopProcs       = $r.desktopWindows.AppProcessCount
        wslPlumbingWsMB    = $r.wslWindows.WorkingSetMB
        distrosRunning     = (@($r.distros | Where-Object { $_.State -eq 'Running' } | ForEach-Object { $_.Name }) -join '+')
        portsOpen          = (@($r.ports.GetEnumerator() | Where-Object { $_.Value } | ForEach-Object { $_.Key }) -join '+')
        dockerImagesGB     = $(if ($null -ne $r.docker) { $r.docker.ImagesSizeGB } else { '' })
        dockerBuildCacheGB = $(if ($null -ne $r.docker) { $r.docker.BuildCacheGB } else { '' })
        acContainersRunning = $(if ($null -ne $r.docker) { $r.docker.AcContainerCount } else { '' })
        readyMarkerSec     = $(if ($null -ne $r.ready) { $r.ready.MarkerSeconds } else { '' })
        readyTcpSec        = $(if ($null -ne $r.ready) { $r.ready.TcpOpenSeconds } else { '' })
        botsOnlineReported = $r.botsOnlineOperatorReported
        preconditionsOk    = $r.preconditions.passed
        forced             = $r.preconditions.forced
        note               = $r.note
    }
}
if (Test-Path $csvPath) {
    $existing = Import-Csv $csvPath
    $all = @($existing) + @($csvRows)
} else {
    $all = $csvRows
}
$csvText = ($all | ConvertTo-Csv -NoTypeInformation) -join "`r`n"
[System.IO.File]::WriteAllText($csvPath, $csvText + "`r`n", $utf8NoBom)

# ---------------------------------------------------------------------------
# Say what was and was not measured. Every run, not just the first.
# ---------------------------------------------------------------------------

Write-Section 'What this sample measured'
Write-Host '  * Machine-level memory in use and committed (the headline; needs an "off" baseline).'
Write-Host '  * The WSL utility VM process, attributed to this backend only because the'
Write-Host '    preconditions established that no other distro was running.'
Write-Host '  * Docker Desktop Windows-side processes, by exact name.'
Write-Host '  * Distro states, stack ports, and virtual-disk high-water marks.'
if ($QueryDocker) { Write-Host '  * The engine''s own image-store and build-cache sizes.' }
if ($TimeReady) { Write-Host '  * start -> world-ready wall clock, by the product''s own log-marker definition.' }

Write-Section 'What this sample could NOT measure'
foreach ($n in $records[0].notMeasured) { Write-Host "  * $n" }

Write-Section 'Caveats attached to this record'
foreach ($c in $records[0].caveats) { Write-Host "  ! $c" -ForegroundColor DarkYellow }

Write-Section 'Written'
Write-Host "  $jsonPath"
Write-Host "  $csvPath"

if ($TimeReady -and $null -ne $readySample -and $readySample.TimedOut) {
    Write-Host ''
    Write-Host "World-ready marker never appeared within $ReadyTimeoutSeconds s. The sample is written; the timing is a timeout, not a measurement." -ForegroundColor Yellow
    exit 4
}

exit 0
