<#
.SYNOPSIS
    Reads the samples written by Measure-BackendFootprint.ps1 and prints the
    Docker-Desktop-vs-Arch-WSL comparison, with every reading that cannot be
    trusted flagged rather than averaged in.

.DESCRIPTION
    This script does no measuring. It pairs samples by phase, subtracts the
    'off' baseline, and - the part that matters - refuses to present a number
    whose provenance is bad:

      * a sample taken with -Force (preconditions failed),
      * two backends whose samples share a WSL VM GENERATION, meaning the
        second inherited the first one's high-water mark,
      * a missing 'off' baseline, without which the machine-level figures mean
        nothing,
      * a phase present for one backend and absent for the other.

    It prints VERDICTS only where the underlying samples support one, and says
    'not measurable' where they do not. That is the whole point: the deliverable
    has to be honest, including where the honest answer is 'less than you'd
    think' or 'this cannot be measured on this machine'.

.PARAMETER ResultsDir
    Directory of sample JSON files. Default <repo>/results/backend-comparison.

.PARAMETER Label
    Only consider samples with this run label. Default: all.

.PARAMETER Markdown
    Emit a Markdown table suitable for pasting into
    docs/backend-comparison-2026-08.md.

.NOTES
    PowerShell 5.1 compatible. Exit 0 always unless the results dir is unusable
    (3): an incomplete comparison is a finding, not an error.
#>
[CmdletBinding()]
param(
    [string]$ResultsDir,
    [string]$Label,
    [switch]$Markdown
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 2.0

function Get-RepoRoot { return (Split-Path -Parent $PSScriptRoot) }

if ([string]::IsNullOrWhiteSpace($ResultsDir)) {
    $ResultsDir = Join-Path (Get-RepoRoot) 'results\backend-comparison'
}
if (-not (Test-Path $ResultsDir)) {
    Write-Host "No results directory at $ResultsDir - nothing to compare." -ForegroundColor Red
    exit 3
}

# @() matters: Get-ChildItem returns a bare object for a single match, and under
# Set-StrictMode -Version 2.0 that object has no .Count.
$files = @(Get-ChildItem $ResultsDir -Filter '*.json' -ErrorAction SilentlyContinue)
if ($files.Count -eq 0) {
    Write-Host "No sample files in $ResultsDir." -ForegroundColor Red
    exit 3
}

$samples = @()
foreach ($f in $files) {
    try {
        $content = Get-Content $f.FullName -Raw | ConvertFrom-Json
    } catch {
        Write-Host "  skipping unreadable $($f.Name): $_" -ForegroundColor Yellow
        continue
    }
    foreach ($s in @($content)) {
        if ($Label -and $s.label -ne $Label) { continue }
        $samples += $s
    }
}

if ($samples.Count -eq 0) {
    Write-Host 'No samples matched.' -ForegroundColor Red
    exit 3
}

function Write-Section { param([string]$T) Write-Host ''; Write-Host "== $T" -ForegroundColor Cyan }

# ---------------------------------------------------------------------------
# Provenance
# ---------------------------------------------------------------------------

Write-Section 'Provenance'

$problems = @()

$forced = @($samples | Where-Object { $_.preconditions.forced })
if ($forced.Count -gt 0) {
    $problems += "FORCED: $($forced.Count) sample(s) were recorded with failing preconditions. They are excluded from the verdicts below."
    foreach ($f in $forced) {
        Write-Host "  forced: $($f.runId) ($($f.backend)/$($f.phase))" -ForegroundColor Yellow
    }
}

$clean = @($samples | Where-Object { -not $_.preconditions.forced })
Write-Host "  $($samples.Count) sample(s) loaded, $($clean.Count) clean."

# THE CONTAMINATION CHECK. Two backends measured inside one VM instance is the
# single most likely way this comparison produces a confident wrong answer:
# the WSL VM does not shrink promptly, so whichever backend was measured SECOND
# inherits the first one's high-water mark and looks worse than it is.
$byBackendGen = @{}
foreach ($s in $clean) {
    if (-not $s.vm.Present) { continue }
    $g = [string]$s.vm.Generation
    if ([string]::IsNullOrWhiteSpace($g)) { continue }
    if (-not $byBackendGen.ContainsKey($g)) { $byBackendGen[$g] = @() }
    if ($byBackendGen[$g] -notcontains $s.backend) { $byBackendGen[$g] += $s.backend }
}
$shared = @($byBackendGen.GetEnumerator() | Where-Object { $_.Value.Count -gt 1 })
if ($shared.Count -gt 0) {
    foreach ($g in $shared) {
        $problems += "SHARED VM GENERATION: backends [$($g.Value -join ', ')] were both measured inside VM generation '$($g.Key)'. The WSL VM keeps its high-water mark, so the second backend's numbers are inflated by the first. Re-run with 'wsl --shutdown' between backends."
    }
} else {
    Write-Host '  VM generations: no backend shared a VM instance with the other. Good.' -ForegroundColor Green
}

foreach ($b in @('desktop', 'arch')) {
    $off = @($clean | Where-Object { $_.backend -eq $b -and $_.phase -eq 'off' })
    if ($off.Count -eq 0) {
        $problems += "NO BASELINE: backend '$b' has no 'off' sample. Machine-level memory figures for it cannot be baseline-corrected and are not comparable."
    }
}

if ($problems.Count -gt 0) {
    Write-Host ''
    foreach ($p in $problems) { Write-Host "  ! $p" -ForegroundColor Yellow }
}

# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------

function Get-PhaseStat {
    param([string]$B, [string]$P)
    $rows = @($clean | Where-Object { $_.backend -eq $B -and $_.phase -eq $P })
    if ($rows.Count -eq 0) { return $null }
    # For a repeated phase, the FIRST sample is the one taken at the defined
    # settling delay; later ones describe the curve. Report the first for
    # point comparisons and keep the whole series for the reclaim question.
    $first = @($rows | Sort-Object sampleIndex)[0]
    $last = @($rows | Sort-Object sampleIndex)[-1]
    return [PSCustomObject]@{
        Backend = $B; Phase = $P; Count = $rows.Count
        FirstInUseMB = $first.machine.InUseMB
        LastInUseMB = $last.machine.InUseMB
        FirstVmMB = $(if ($first.vm.Present) { $first.vm.WorkingSetMB } else { 0 })
        LastVmMB = $(if ($last.vm.Present) { $last.vm.WorkingSetMB } else { 0 })
        FirstDesktopWinMB = $first.desktopWindows.WindowsWorkingSetMB
        ElapsedSec = $last.elapsedFromFirstSec
        Rows = $rows
    }
}

$phases = @('off', 'engine', 'server', 'stopped')
$stats = @{}
foreach ($b in @('desktop', 'arch')) {
    foreach ($p in $phases) {
        $stats["$b/$p"] = Get-PhaseStat -B $b -P $p
    }
}

function Format-Or {
    param($V, [string]$Fallback = 'not measured')
    if ($null -eq $V) { return $Fallback }
    return ('{0:N0}' -f $V)
}

function Get-Delta {
    param($B, $P)
    $s = $stats["$B/$P"]
    $off = $stats["$B/off"]
    if ($null -eq $s) { return $null }
    if ($null -eq $off) { return $null }
    return [math]::Round($s.FirstInUseMB - $off.FirstInUseMB, 1)
}

Write-Section 'Memory, machine-level, baseline-corrected (MB over the "off" sample)'
Write-Host '  This is the headline. It asks how much of the box''s RAM the backend costs,'
Write-Host '  and sidesteps the per-process attribution argument entirely.'
Write-Host ''
Write-Host ('  {0,-12} {1,16} {2,16} {3,16}' -f 'phase', 'desktop', 'arch', 'arch saves')
foreach ($p in @('engine', 'server', 'stopped')) {
    $dd = Get-Delta -B 'desktop' -P $p
    $da = Get-Delta -B 'arch' -P $p
    $saves = 'n/a'
    if ($null -ne $dd -and $null -ne $da) { $saves = '{0:N0}' -f ($dd - $da) }
    Write-Host ('  {0,-12} {1,16} {2,16} {3,16}' -f $p, (Format-Or $dd), (Format-Or $da), $saves)
}

Write-Section 'WSL utility VM working set (MB, absolute)'
Write-Host '  Attributable ONLY because each sample required every other distro to be off.'
Write-Host ''
Write-Host ('  {0,-12} {1,16} {2,16}' -f 'phase', 'desktop', 'arch')
foreach ($p in $phases) {
    $sd = $stats["desktop/$p"]; $sa = $stats["arch/$p"]
    $vd = $null; $va = $null
    if ($null -ne $sd) { $vd = $sd.FirstVmMB }
    if ($null -ne $sa) { $va = $sa.FirstVmMB }
    Write-Host ('  {0,-12} {1,16} {2,16}' -f $p, (Format-Or $vd), (Format-Or $va))
}

Write-Section 'Docker Desktop Windows-side processes (MB working set)'
$sd = $stats['desktop/engine']
$sa = $stats['arch/engine']
$vd = 'not measured'; $va = 'not measured'
if ($null -ne $sd) { $vd = '{0:N0}' -f $sd.FirstDesktopWinMB }
if ($null -ne $sa) { $va = '{0:N0}' -f $sa.FirstDesktopWinMB }
Write-Host "  desktop: $vd    arch: $va"
Write-Host '  The arch figure should be 0: the preconditions require Desktop to be exited.'
Write-Host '  This is a REAL saving only for a user who would otherwise leave Desktop running.'

# ---------------------------------------------------------------------------
# RAM returned after stop - the metric most likely to be un-measurable
# ---------------------------------------------------------------------------

Write-Section 'RAM returned after stop'
foreach ($b in @('desktop', 'arch')) {
    $srv = $stats["$b/server"]
    $stp = $stats["$b/stopped"]
    if ($null -eq $srv -or $null -eq $stp) {
        Write-Host "  $b : not measurable - needs both a 'server' and a 'stopped' sample."
        continue
    }
    $returnedAtFirst = [math]::Round($srv.FirstInUseMB - $stp.FirstInUseMB, 1)
    $returnedAtLast = [math]::Round($srv.FirstInUseMB - $stp.LastInUseMB, 1)
    Write-Host ("  {0,-8} at first post-stop sample: {1,8:N0} MB returned" -f $b, $returnedAtFirst)
    Write-Host ("  {0,-8} after {1} s of settling:   {2,8:N0} MB returned  ({3} samples)" -f '', $stp.ElapsedSec, $returnedAtLast, $stp.Count)
    $vmDrop = [math]::Round($srv.FirstVmMB - $stp.LastVmMB, 1)
    Write-Host ("  {0,-8} WSL VM working set drop:  {1,8:N0} MB" -f '', $vmDrop)
    if ($stp.Count -lt 3) {
        Write-Host '           WARNING: fewer than 3 post-stop samples. A single point measures the' -ForegroundColor Yellow
        Write-Host '           reclaim schedule, not the workload. Re-run phase stopped with -Repeat 6.' -ForegroundColor Yellow
    }
}
Write-Host ''
Write-Host '  READ THIS BEFORE QUOTING THE ABOVE: with autoMemoryReclaim=gradual the VM'
Write-Host '  gives memory back on an undocumented schedule, and without it the VM does not'
Write-Host '  give it back at all while it lives. Either way a low number here is a property'
Write-Host '  of WSL, shared by BOTH backends, and is not evidence that one leaks and the'
Write-Host '  other does not. The only state in which all memory is provably returned is the'
Write-Host '  one where the VM process is gone entirely.'

# ---------------------------------------------------------------------------
# Time and disk
# ---------------------------------------------------------------------------

Write-Section 'Start -> world ready'
$anyReady = $false
foreach ($b in @('desktop', 'arch')) {
    $r = @($clean | Where-Object { $_.backend -eq $b -and $null -ne $_.ready -and $null -ne $_.ready.MarkerSeconds })
    if ($r.Count -eq 0) {
        $tcpOnly = @($clean | Where-Object { $_.backend -eq $b -and $null -ne $_.ready })
        if ($tcpOnly.Count -gt 0) {
            Write-Host "  $b : only a TCP-open time was recorded, which is NOT world-ready (docker's proxy accepts early). Re-run -TimeReady with -QueryDocker."  -ForegroundColor Yellow
        } else {
            Write-Host "  $b : not measured."
        }
        continue
    }
    $anyReady = $true
    foreach ($x in $r) {
        Write-Host ("  {0,-8} {1,8:N0} s to the world-ready marker (tcp accepted at {2} s)" -f $b, $x.ready.MarkerSeconds, $x.ready.TcpOpenSeconds)
    }
}
if (-not $anyReady) { Write-Host '  No authoritative readiness timings recorded.' }

Write-Section 'Disk'
$latest = @($samples | Where-Object { $null -ne $_.disk } | Sort-Object timestampUtc)
if ($latest.Count -eq 0) {
    Write-Host '  No disk samples.'
} else {
    $d = $latest[-1]
    Write-Host "  Virtual-disk high-water marks (as of $($d.timestampUtc)):"
    foreach ($x in $d.disk) {
        Write-Host ('    {0,-30} {1,8:N2} GB' -f $x.Distro, $x.VhdxTotalGB)
    }
    Write-Host ''
    Write-Host '  THESE ARE NOT LIKE FOR LIKE. dml-arch''s vhdx holds the OS, the docker image'
    Write-Host '  store, the build cache AND the server directory in one file. Docker Desktop''s'
    Write-Host '  image store is the separate docker_data.vhdx, and its server directory lives'
    Write-Host '  on the Windows filesystem outside every figure here. Comparing the two totals'
    Write-Host '  directly overstates Arch. The comparable number is "docker system df" below.'
}
Write-Host ''
foreach ($b in @('desktop', 'arch')) {
    $dk = @($clean | Where-Object { $_.backend -eq $b -and $null -ne $_.docker -and $_.docker.Reachable })
    if ($dk.Count -eq 0) {
        Write-Host "  docker system df, $b : not measured (re-run a phase with -QueryDocker)."
        continue
    }
    $x = $dk[-1]
    Write-Host ("  docker system df, {0,-8}: images {1} GB, build cache {2} GB, volumes {3} GB" -f $b, $x.docker.ImagesSizeGB, $x.docker.BuildCacheGB, $x.docker.VolumesSizeGB)
}

# ---------------------------------------------------------------------------
# Coverage
# ---------------------------------------------------------------------------

Write-Section 'Coverage'
$missing = @()
foreach ($b in @('desktop', 'arch')) {
    foreach ($p in $phases) {
        if ($null -eq $stats["$b/$p"]) { $missing += "$b/$p" }
    }
}
if ($missing.Count -eq 0) {
    Write-Host '  All backend/phase pairs present.' -ForegroundColor Green
} else {
    Write-Host "  MISSING: $($missing -join ', ')" -ForegroundColor Yellow
    Write-Host '  A metric is reported only where BOTH backends have the sample. Anything above'
    Write-Host '  that says "not measured" is missing, not zero.'
}

if ($problems.Count -gt 0) {
    Write-Section 'Verdict reliability'
    Write-Host '  This comparison has provenance problems (listed at the top). Fix them and'  -ForegroundColor Yellow
    Write-Host '  re-run before publishing any number from it.' -ForegroundColor Yellow
}

if ($Markdown) {
    Write-Section 'Markdown'
    Write-Host ''
    Write-Host '| Metric | Docker Desktop | Arch WSL | Arch saves |'
    Write-Host '|---|---|---|---|'
    foreach ($p in @('engine', 'server', 'stopped')) {
        $dd = Get-Delta -B 'desktop' -P $p
        $da = Get-Delta -B 'arch' -P $p
        $saves = 'n/a'
        if ($null -ne $dd -and $null -ne $da) { $saves = '{0:N0} MB' -f ($dd - $da) }
        $ddT = 'not measured'; $daT = 'not measured'
        if ($null -ne $dd) { $ddT = '{0:N0} MB' -f $dd }
        if ($null -ne $da) { $daT = '{0:N0} MB' -f $da }
        Write-Host "| RAM over baseline, $p | $ddT | $daT | $saves |"
    }
    $vdT = $vd; $vaT = $va
    if ($vdT -ne 'not measured') { $vdT = "$vdT MB" }
    if ($vaT -ne 'not measured') { $vaT = "$vaT MB" }
    Write-Host "| Desktop Windows processes | $vdT | $vaT | |"
}

exit 0
