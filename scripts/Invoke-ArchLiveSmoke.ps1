<#
.SYNOPSIS
    Builds the Linux dml-wow, installs it into the dml-arch distro, and runs the
    READ-ONLY live differential smoke against it.

.DESCRIPTION
    dml_core::vocab translates the launcher's bash-CLI vocabulary onto dml-wow's
    flat subcommands. That table is covered by unit tests against clap's derive,
    which cannot reach the thing that actually matters: whether
    `wsl.exe -d dml-arch -u dml --exec dml-wow <translated argv>` is a command
    the distro accepts, and whether its answer matches the bash `dml` in that
    same distro.

    This script makes that check repeatable. It:

      1. builds crates/dml-wow-cli INSIDE the distro (never on the Windows side
         -- the artifact has to be a Linux ELF),
      2. installs it to /usr/local/bin/dml-wow at mode 0755,
      3. runs crates/dml-core/tests/live_arch_smoke.rs with --ignored,
      4. prints a verb-by-verb table.

    CARGO_TARGET_DIR is set to $HOME/target inside the distro on purpose.
    Building into the repo's own target/ goes through drvfs (slow) and would
    collide with the Windows-side target/ that this script's own `cargo test`
    uses moments later.

    Mode 0755 is not incidental. A 0644 binary fails `--exec` with "Permission
    denied", which reads like a MISSING binary -- so the mode is part of the
    diagnosis, and step 2 verifies it rather than assuming install(1) honoured
    the flag.

    READ-ONLY with respect to the server. This script never starts, stops or
    restarts anything, never touches the database, and runs no mutating GM or
    SOAP verb. Its only writes are the built binary and /usr/local/bin/dml-wow.
    The lifecycle counterpart (live_arch_lifecycle.rs) is deliberately NOT run
    from here; see that file's header.

    ASCII-only on purpose, and therefore no byte-order mark, matching the other
    scripts in this directory. If you add non-ASCII text (an em dash, say), you
    MUST save the file with a UTF-8 BOM: PowerShell 5.1 reads a BOM-less file as
    the ANSI codepage and mis-parses it. cli/tests/windows-smoke.ps1 carries a
    BOM for exactly that reason.

.PARAMETER SkipBuild
    Use the dml-wow already installed in the distro. Skips steps 1 and 2.

.PARAMETER BuildOnly
    Build and install, but do not run the tests.

.PARAMETER Distro
    WSL distro name. Defaults to dml-arch (dml_core::runner::DISTRO).

.PARAMETER User
    Linux user inside the distro. Defaults to dml (dml_core::runner::USER).

.EXAMPLE
    .\scripts\Invoke-ArchLiveSmoke.ps1

.EXAMPLE
    .\scripts\Invoke-ArchLiveSmoke.ps1 -SkipBuild
#>
[CmdletBinding()]
param(
    [switch] $SkipBuild,
    [switch] $BuildOnly,
    [string] $Distro = 'dml-arch',
    [string] $User = 'dml'
)

$ErrorActionPreference = 'Stop'

function Write-Step {
    param([string] $Text)
    Write-Host ''
    Write-Host "==> $Text" -ForegroundColor Cyan
}

function Write-Fail {
    param([string] $Text)
    Write-Host "    FAIL: $Text" -ForegroundColor Red
}

# -- preflight --------------------------------------------------------------

Write-Step "Preflight"

$wsl = Get-Command wsl.exe -ErrorAction SilentlyContinue
if ($null -eq $wsl) { throw "wsl.exe is not on PATH. This script needs WSL2." }

# `wsl -l -q` answers in UTF-16; PowerShell decodes it for us, but stray NULs
# survive on some builds, so strip them before comparing.
$distros = (& wsl.exe -l -q) | ForEach-Object { $_ -replace "`0", '' } | ForEach-Object { $_.Trim() }
if ($distros -notcontains $Distro) {
    throw "Distro '$Distro' not found. Present: $($distros -join ', ')"
}
Write-Host "    distro '$Distro' present"

$repoWin = Split-Path -Parent $PSScriptRoot
if (-not (Test-Path (Join-Path $repoWin 'Cargo.toml'))) {
    throw "Could not find the cargo workspace root from $PSScriptRoot (looked at $repoWin)."
}

# ASK wslpath; never do string surgery on the path. `C:\x` -> `/mnt/c/x` only
# holds for the default automount root, which /etc/wsl.conf can move.
$repoLinux = (& wsl.exe -d $Distro --exec wslpath -a -u $repoWin) | Select-Object -First 1
if ([string]::IsNullOrWhiteSpace($repoLinux)) {
    throw "wslpath could not translate $repoWin into $Distro."
}
$repoLinux = $repoLinux.Trim()
Write-Host "    repo: $repoWin"
Write-Host "    repo in distro: $repoLinux"

$cargo = Get-Command cargo -ErrorAction SilentlyContinue
if ($null -eq $cargo) {
    $candidate = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
    if (Test-Path $candidate) {
        $cargoExe = $candidate
    } else {
        throw "cargo is not on PATH and $candidate does not exist."
    }
} else {
    $cargoExe = $cargo.Source
}
Write-Host "    cargo: $cargoExe"

# -- 1. build inside the distro ---------------------------------------------

if (-not $SkipBuild) {
    Write-Step "Building dml-wow inside $Distro (release)"

    $buildCmd = "cd '$repoLinux' && export CARGO_TARGET_DIR=`$HOME/target && cargo build --release -p dml-wow-cli"
    & wsl.exe -d $Distro -u $User --exec bash -lc $buildCmd
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed inside $Distro (exit $LASTEXITCODE)." }

    Write-Step "Installing to /usr/local/bin/dml-wow at mode 0755"

    # `install -m 0755` in one step: a copy-then-chmod leaves a window in which
    # the binary exists at the wrong mode, and the wrong mode is the failure
    # that masquerades as a missing binary.
    $installCmd = "sudo -n install -m 0755 `$HOME/target/release/dml-wow /usr/local/bin/dml-wow"
    & wsl.exe -d $Distro -u $User --exec bash -lc $installCmd
    if ($LASTEXITCODE -ne 0) { throw "install to /usr/local/bin failed (exit $LASTEXITCODE). Does '$User' have passwordless sudo?" }

    # Verify the mode rather than trusting the flag.
    $mode = (& wsl.exe -d $Distro -u $User --exec stat -c '%a' /usr/local/bin/dml-wow) | Select-Object -First 1
    $mode = "$mode".Trim()
    if ($mode -ne '755') { throw "dml-wow installed at mode $mode, expected 755." }
    Write-Host "    /usr/local/bin/dml-wow mode=$mode"
}

# -- 2. prove both binaries answer through their production spawn forms ------

Write-Step "Probing both CLIs through the exact spawn the launcher uses"

$archVersion = (& wsl.exe -d $Distro -u $User --exec dml-wow version) | Select-Object -First 1
if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($archVersion)) {
    throw "dml-wow did not answer through --exec. If it is installed, check the mode (0644 fails as 'Permission denied')."
}
Write-Host "    dml-wow  : $archVersion"

# The bash CLI keeps the bare `--` form: that IS its documented production
# contract (dml_core::runner::DmlRunner::default), and its args are our own
# literals. Anything passing user text or a host path must use --exec.
$bashVersion = (& wsl.exe -d $Distro -u $User -- dml version --json) | Select-Object -First 1
if ([string]::IsNullOrWhiteSpace($bashVersion)) {
    throw "the bash dml did not answer. Run cli/dev-install.ps1 first."
}
Write-Host "    dml (bash): $bashVersion"

if ($BuildOnly) {
    Write-Host ''
    Write-Host "BuildOnly: stopping before the tests." -ForegroundColor Yellow
    exit 0
}

# -- 3. the read-only differential suite -------------------------------------

Write-Step "Running the read-only live differential suite"
Write-Host "    (each verb is asked of BOTH backends and the answers compared)"
Write-Host ''

Push-Location $repoWin
try {
    # --test-threads=1 so the interleaved --nocapture output stays readable and
    # attributable to a verb. These tests spawn wsl.exe; they are not CPU-bound.
    $output = & $cargoExe test -p dml-core --test live_arch_smoke -- --ignored --nocapture --test-threads=1
    $testExit = $LASTEXITCODE
} finally {
    Pop-Location
}

$output | ForEach-Object { Write-Host $_ }

# -- 4. the table ------------------------------------------------------------

Write-Step "Result"

$rows = @()
foreach ($line in $output) {
    $m = [regex]::Match("$line", '^test\s+(?<name>\S+)\s+\.\.\.\s+(?<verdict>ok|FAILED|ignored)')
    if ($m.Success) {
        $rows += [pscustomobject]@{
            Test    = $m.Groups['name'].Value -replace '^live_', ''
            Verdict = $m.Groups['verdict'].Value.ToUpperInvariant()
        }
    }
}

if ($rows.Count -eq 0) {
    Write-Fail "no test results were parsed -- did the suite compile?"
    exit 1
}

$width = ($rows | ForEach-Object { $_.Test.Length } | Measure-Object -Maximum).Maximum
foreach ($r in $rows) {
    $colour = 'Green'
    if ($r.Verdict -ne 'OK') { $colour = 'Red' }
    Write-Host ("    {0}  {1}" -f $r.Test.PadRight($width), $r.Verdict) -ForegroundColor $colour
}

$failed = @($rows | Where-Object { $_.Verdict -eq 'FAILED' })
Write-Host ''
Write-Host ("    {0} passed, {1} failed, of {2}" -f @($rows | Where-Object { $_.Verdict -eq 'OK' }).Count, $failed.Count, $rows.Count)

if ($failed.Count -gt 0) {
    Write-Host ''
    Write-Host "    A failure here is a REAL divergence between the two backends," -ForegroundColor Yellow
    Write-Host "    not a flaky test. Read the assertion message above: it names the" -ForegroundColor Yellow
    Write-Host "    fields that disagreed and, where known, the cause." -ForegroundColor Yellow
}

exit $testExit
