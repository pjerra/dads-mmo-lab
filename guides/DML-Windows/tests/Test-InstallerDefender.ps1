# Test-InstallerDefender.ps1 -- covers the installer's Defender-exclusion code.
#
# There is no test framework in this corner of the repo, so this is a plain
# PS 5.1 script: it parses Install-DML.ps1 / Uninstall-DML.ps1, lifts the
# Defender functions out of the AST and runs them against stubbed
# Add-MpPreference / Get-MpPreference / Read-Host. Nothing here reads or
# changes the real Defender configuration, and neither script is executed --
# only the functions named below are pulled out and invoked.
#
#   powershell -ExecutionPolicy Bypass -File guides\DML-Windows\tests\Test-InstallerDefender.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$here        = Split-Path -Parent $MyInvocation.MyCommand.Path
$guides      = Split-Path -Parent $here
$installer   = Join-Path $guides 'Install-DML.ps1'
$uninstaller = Join-Path $guides 'Uninstall-DML.ps1'

$script:Failures = 0
$script:Checks   = 0
# The behavioural tests shadow Write-Host to capture the installer's prompt, so
# the harness's own output is fully qualified to reach the console regardless.
function Say([string]$m, [string]$color = 'Gray') {
    Microsoft.PowerShell.Utility\Write-Host $m -ForegroundColor $color
}
function Assert-True([bool]$cond, [string]$what) {
    $script:Checks++
    if ($cond) {
        Say "  ok   $what" 'DarkGreen'
    } else {
        $script:Failures++
        Say "  FAIL $what" 'Red'
    }
}
function Assert-Eq($expected, $actual, [string]$what) {
    Assert-True ("$expected" -eq "$actual") "$what (expected '$expected', got '$actual')"
}

function Get-FunctionAst($ast, [string]$name) {
    $hits = $ast.FindAll({
        param($n)
        $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name
    }, $true)
    if ($hits.Count -eq 0) { return $null }
    return $hits[0]
}

# -----------------------------------------------------------------------------
# 1. Both scripts must parse clean
# -----------------------------------------------------------------------------
Say ""
Say "Parse" 'Cyan'
foreach ($f in @($installer, $uninstaller)) {
    $tokErr = $null
    $null = [System.Management.Automation.PSParser]::Tokenize((Get-Content -Raw $f), [ref]$tokErr)
    Assert-Eq 0 @($tokErr).Count "$(Split-Path -Leaf $f) tokenizes without errors"
}

$parseErrors = $null
$installAst = [System.Management.Automation.Language.Parser]::ParseFile($installer, [ref]$null, [ref]$parseErrors)
Assert-Eq 0 @($parseErrors).Count "Install-DML.ps1 AST parse is error-free"
$parseErrors = $null
$uninstallAst = [System.Management.Automation.Language.Parser]::ParseFile($uninstaller, [ref]$null, [ref]$parseErrors)
Assert-Eq 0 @($parseErrors).Count "Uninstall-DML.ps1 AST parse is error-free"

# -----------------------------------------------------------------------------
# 2. The Defender functions exist, sit outside the installer's embedded
#    here-strings (the bootstrap CLI and friends) and are pure ASCII
#    (BOM-less PS 5.1 file)
# -----------------------------------------------------------------------------
Say ""
Say "Placement and encoding" 'Cyan'
# Located from the AST, not pinned to line numbers: every edit above the
# here-string moves it, and a stale window silently stops guarding anything.
$hereStrings = @($installAst.FindAll({
    param($n)
    $n -is [System.Management.Automation.Language.StringConstantExpressionAst] -and
    "$($n.StringConstantType)" -like '*HereString*'
}, $true))
Assert-True ($hereStrings.Count -gt 0) "the installer's embedded here-strings were located"
function Test-InsideHereString($fn) {
    foreach ($hs in $hereStrings) {
        if ($fn.Extent.StartLineNumber -le $hs.Extent.EndLineNumber -and
            $fn.Extent.EndLineNumber   -ge $hs.Extent.StartLineNumber) { return $true }
    }
    return $false
}

$installFns = @('Get-SourceCheckoutRoot', 'Test-ExclusionRecorded', 'Get-BuildToolExclusionPaths', 'Add-BuildToolDefenderExclusions')
$installAsts = @{}
foreach ($name in $installFns) {
    $fn = Get-FunctionAst $installAst $name
    Assert-True ($null -ne $fn) "Install-DML.ps1 defines $name"
    if ($fn) {
        $installAsts[$name] = $fn
        $start = $fn.Extent.StartLineNumber
        $end   = $fn.Extent.EndLineNumber
        Assert-True (-not (Test-InsideHereString $fn)) "$name is outside the embedded here-strings (lines $start-$end)"
        $nonAscii = @($fn.Extent.Text.ToCharArray() | Where-Object { [int]$_ -gt 127 }).Count
        Assert-Eq 0 $nonAscii "$name is pure ASCII"
    }
}
$removeFn = Get-FunctionAst $uninstallAst 'Remove-BuildToolDefenderExclusions'
Assert-True ($null -ne $removeFn) "Uninstall-DML.ps1 defines Remove-BuildToolDefenderExclusions"
if ($removeFn) {
    $nonAscii = @($removeFn.Extent.Text.ToCharArray() | Where-Object { [int]$_ -gt 127 }).Count
    Assert-Eq 0 $nonAscii "Remove-BuildToolDefenderExclusions is pure ASCII"
}
$uninstallHelper = Get-FunctionAst $uninstallAst 'Test-ExclusionRecorded'
Assert-True ($null -ne $uninstallHelper) "Uninstall-DML.ps1 defines Test-ExclusionRecorded"

# The installer must actually call the new prompt from Phase 2, or it is dead code.
$installText = Get-Content -Raw $installer
Assert-True ($installText -match 'Add-BuildToolDefenderExclusions\s+\(Get-SourceCheckoutRoot') `
    "Phase 2 calls Add-BuildToolDefenderExclusions with the computed checkout root"
$uninstallText = Get-Content -Raw $uninstaller
Assert-True ($uninstallText -match 'Remove-BuildToolDefenderExclusions') `
    "Uninstall-DML.ps1 calls Remove-BuildToolDefenderExclusions"

if ($script:Failures -gt 0) {
    Say ""
    Say "$script:Failures/$script:Checks checks failed -- stopping before the behavioural tests." 'Red'
    exit 1
}

# -----------------------------------------------------------------------------
# Stubs. Functions shadow cmdlets in PowerShell's command resolution, so these
# intercept the real Defender cmdlets for the behavioural tests below.
# -----------------------------------------------------------------------------
$script:LiveProcesses = @()
$script:LivePaths     = @()
$script:Answer        = 'n'
$script:AddThrows     = $false
$script:RemoveThrows  = $false
$script:GetThrows     = $false
$script:SwallowAdds   = $false   # simulates Tamper Protection: call returns, nothing recorded
$script:Removed       = @()
$script:Marked        = @()
$script:Warnings      = @()
$script:StepDone      = $false
$script:Prompt        = @()

function Reset-Stubs {
    $script:LiveProcesses = @()
    $script:LivePaths     = @()
    $script:Answer        = 'n'
    $script:AddThrows     = $false
    $script:RemoveThrows  = $false
    $script:GetThrows     = $false
    $script:SwallowAdds   = $false
    $script:Removed       = @()
    $script:Marked        = @()
    $script:Warnings      = @()
    $script:StepDone      = $false
    $script:Prompt        = @()
}

function Add-MpPreference {
    [CmdletBinding()]
    param([string]$ExclusionProcess, [string]$ExclusionPath)
    if ($script:AddThrows) { throw 'stub: Add-MpPreference denied' }
    if ($script:SwallowAdds) { return }
    if ($ExclusionProcess) { $script:LiveProcesses = @($script:LiveProcesses) + $ExclusionProcess }
    if ($ExclusionPath)    { $script:LivePaths     = @($script:LivePaths) + $ExclusionPath }
}
function Remove-MpPreference {
    [CmdletBinding()]
    param([string]$ExclusionProcess, [string]$ExclusionPath)
    if ($script:RemoveThrows) { throw 'stub: Remove-MpPreference denied' }
    if ($ExclusionProcess) {
        $script:Removed = @($script:Removed) + $ExclusionProcess
        $script:LiveProcesses = @(@($script:LiveProcesses) | Where-Object { $_ -ne $ExclusionProcess })
    }
    if ($ExclusionPath) {
        $script:Removed = @($script:Removed) + $ExclusionPath
        $script:LivePaths = @(@($script:LivePaths) | Where-Object { $_ -ne $ExclusionPath })
    }
}
function Get-MpPreference {
    [CmdletBinding()]
    param()
    if ($script:GetThrows) { throw 'stub: Get-MpPreference denied' }
    [pscustomobject]@{
        ExclusionProcess = $script:LiveProcesses
        ExclusionPath    = $script:LivePaths
    }
}
function Read-Host { param([string]$Prompt) $script:Prompt = @($script:Prompt) + $Prompt; return $script:Answer }
function Write-Host { param([Parameter(Position = 0)]$Object, [string]$ForegroundColor) $script:Prompt = @($script:Prompt) + "$Object" }
function Write-Step([string]$m) { }
function Write-Diag([string]$m) { }
function Write-Info([string]$m) { }
function Write-Ok([string]$m)   { }
function Write-Warn([string]$m) { $script:Warnings = @($script:Warnings) + $m }
function Test-StepDone([string]$s) { return $script:StepDone }
function Mark-StepDone([string]$s) { $script:Marked = @($script:Marked) + $s }

foreach ($name in $installFns) { Invoke-Expression $installAsts[$name].Extent.Text }

# Tripwire. The uninstaller keeps its OWN copy of Test-ExclusionRecorded and the
# symmetry section below exists to exercise THAT copy. PowerShell resolves calls
# by name, so if both copies share one scope the installer's wins and any
# divergence in the uninstaller's copy (a `-ceq`, a gutted body) is invisible.
# Wrapping the installer's copy in a call counter turns "the wrong copy ran"
# into a failing assertion instead of a silent pass.
$script:InstallerHelperImpl = ${function:Test-ExclusionRecorded}
$script:InstallerHelperHits = 0
function Test-ExclusionRecorded($Recorded, $Wanted) {
    $script:InstallerHelperHits++
    & $script:InstallerHelperImpl $Recorded $Wanted
}

# The uninstaller's two functions run in a CHILD scope of their own, so its copy
# of Test-ExclusionRecorded shadows the installer's for the whole call and is
# what Remove-BuildToolDefenderExclusions actually resolves. Invoke-Expression
# keeps the harness's script scope (the $script: stub state and the stubbed
# Get-MpPreference / Remove-MpPreference / Write-* stay visible), while the
# `& { ... }` wrapper gives the lifted definitions a scope the parent's copies
# cannot reach into.
$script:UninstallScope = @'
& {
'@ + "`r`n" + $uninstallHelper.Extent.Text + "`r`n" + $removeFn.Extent.Text + "`r`n" + @'
    Remove-BuildToolDefenderExclusions $Record
}
'@
function Invoke-UninstallerRemove($Record) {
    Invoke-Expression $script:UninstallScope
}

# Sandbox for the state file the installer writes for the uninstaller.
$sandbox = Join-Path ([System.IO.Path]::GetTempPath()) ("dml-defender-test-" + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Force -Path $sandbox
$recordFile = Join-Path $sandbox 'defender-build-exclusions.json'

# A fake source checkout: <root>\guides\DML-Windows\Install-DML.ps1 plus the
# workspace markers that tell a checkout apart from a standalone download.
$fakeRepo = Join-Path $sandbox 'repo'
$fakeDir  = Join-Path $fakeRepo 'guides\DML-Windows'
$null = New-Item -ItemType Directory -Force -Path $fakeDir
$null = New-Item -ItemType Directory -Force -Path (Join-Path $fakeRepo 'crates')
Set-Content -Path (Join-Path $fakeRepo 'Cargo.toml') -Value '[workspace]' -Encoding ASCII
$fakeScript = Join-Path $fakeDir 'Install-DML.ps1'
Set-Content -Path $fakeScript -Value '# fake' -Encoding ASCII
$fakeTarget = Join-Path $fakeRepo 'target'

# The exclusion list is COMPUTED from the environment, so pin the environment:
# whatever cargo/rustup layout this dev box happens to have must not decide
# whether the tests pass. Restored in the finally block.
$savedCargoHome  = $env:CARGO_HOME
$savedRustupHome = $env:RUSTUP_HOME
$fakeCargo  = Join-Path $sandbox 'cargo-home'
$fakeRustup = Join-Path $sandbox 'rustup-home'
function Set-EnvOrClear([string]$Name, $Value) {
    if ($Value) { Set-Item -Path "Env:\$Name" -Value $Value }
    else        { Remove-Item -Path "Env:\$Name" -ErrorAction SilentlyContinue }
}

try {
    Set-EnvOrClear 'CARGO_HOME'  $fakeCargo
    Set-EnvOrClear 'RUSTUP_HOME' $fakeRustup

    # -------------------------------------------------------------------------
    # 3. Checkout detection
    # -------------------------------------------------------------------------
    Say ""
    Say "Checkout detection" 'Cyan'
    Assert-Eq $fakeRepo (Get-SourceCheckoutRoot $fakeScript) "resolves the repo root from the script path"
    $loose = Join-Path $sandbox 'Install-DML.ps1'
    Set-Content -Path $loose -Value '# fake' -Encoding ASCII
    Assert-True ($null -eq (Get-SourceCheckoutRoot $loose)) "a standalone download is not a checkout"
    Assert-True ($null -eq (Get-SourceCheckoutRoot '')) "empty script path is handled"
    Assert-True ($null -eq (Get-SourceCheckoutRoot $null)) "null script path is handled"

    # A script saved AT a drive root: Split-Path -Parent 'D:\' hands back an
    # EMPTY string, and feeding that back into Split-Path is a terminating
    # binding error -- which would abort Phase 2 with a bogus [FAIL] after the
    # install has already succeeded. Every level must be guarded.
    foreach ($odd in @('D:\Install-DML.ps1', 'C:\Install-DML.ps1', 'D:\guides\Install-DML.ps1')) {
        $threw  = $false
        $result = 'not-set'
        try { $result = Get-SourceCheckoutRoot $odd } catch { $threw = $true }
        Assert-True (-not $threw) "'$odd' does not throw"
        Assert-True ($null -eq $result) "'$odd' is not a checkout"
    }

    # -------------------------------------------------------------------------
    # 4. Read-back comparison tolerates Defender's path normalization
    # -------------------------------------------------------------------------
    Say ""
    Say "Read-back comparison" 'Cyan'
    Assert-True (Test-ExclusionRecorded @('C:\a\target') 'C:\a\target') "exact match"
    Assert-True (Test-ExclusionRecorded @('C:\a\target\') 'C:\a\target') "trailing separator match"
    Assert-True (Test-ExclusionRecorded @('c:\A\TARGET') 'C:\a\target') "case-insensitive match"
    Assert-True (-not (Test-ExclusionRecorded @('C:\b\target') 'C:\a\target')) "unrelated entry does not match"
    Assert-True (-not (Test-ExclusionRecorded @() 'C:\a\target')) "empty list does not match"
    Assert-True (-not (Test-ExclusionRecorded @($null) 'C:\a\target')) "null entry does not match"

    # -------------------------------------------------------------------------
    # 4b. The exclusion list is computed, not hardcoded
    # -------------------------------------------------------------------------
    Say ""
    Say "Computed exclusion paths" 'Cyan'
    Assert-Eq 0 @(Get-BuildToolExclusionPaths $null).Count "no checkout means no paths"
    $computed = @(Get-BuildToolExclusionPaths $fakeRepo)
    Assert-True ($computed -contains $fakeTarget) "computes <repo>\target from the checkout root"
    Assert-True ($computed -contains $fakeCargo)  "honours CARGO_HOME"
    Assert-True ($computed -contains $fakeRustup) "honours RUSTUP_HOME"
    Assert-Eq 3 $computed.Count "nothing beyond those three directories"
    Assert-Eq 0 @($computed | Where-Object { -not $_ }).Count "no empty entry ever reaches Defender"

    # Unset env -> the per-user default locations, still computed.
    Set-EnvOrClear 'CARGO_HOME'  $null
    Set-EnvOrClear 'RUSTUP_HOME' $null
    $fallback = @(Get-BuildToolExclusionPaths $fakeRepo)
    Assert-True ($fallback -contains (Join-Path $env:USERPROFILE '.cargo'))  "falls back to the profile cargo dir"
    Assert-True ($fallback -contains (Join-Path $env:USERPROFILE '.rustup')) "falls back to the profile rustup dir"

    # A trailing separator, and two vars pointing at one directory, must not
    # produce a duplicate Defender entry.
    Set-EnvOrClear 'CARGO_HOME'  $fakeCargo
    Set-EnvOrClear 'RUSTUP_HOME' ($fakeCargo + '\')
    Assert-Eq 2 @(Get-BuildToolExclusionPaths $fakeRepo).Count "duplicate homes collapse to one entry"
    Set-EnvOrClear 'RUSTUP_HOME' $fakeRustup

    # -------------------------------------------------------------------------
    # 5. The prompt defaults to no
    # -------------------------------------------------------------------------
    Say ""
    Say "Opt-in prompt" 'Cyan'
    Reset-Stubs
    Add-BuildToolDefenderExclusions $fakeRepo $sandbox
    $promptText = @($script:Prompt) -join "`n"
    Assert-True ($promptText -match '(?i)separate') "prompt says it is a separate question"
    Assert-True ($promptText -match '(?i)source build') "prompt names the source-builder audience"
    Assert-True ($promptText -match '\(y/N\)') "prompt defaults to no"

    # The consent text has to be literally true and complete: every directory
    # that stops being scanned is named, and nothing is excluded by program
    # name (a bare-name -ExclusionProcess exempts that process everywhere on
    # the machine, forever, which no short prompt can honestly describe).
    foreach ($p in @($fakeTarget, $fakeCargo, $fakeRustup)) {
        Assert-True ($promptText.Contains($p)) "prompt names the directory it excludes: $p"
    }
    Assert-True ($promptText -notmatch '(?i)\.exe') "prompt promises no program-name exclusions"
    Assert-True ($promptText -match '(?i)director|folder') "prompt says these are directories"

    # NB the loop variable must not be named $answer: PowerShell variable names
    # are case-insensitive, so $answer and $script:Answer are the same cell and
    # Reset-Stubs would overwrite the case under test with its own default.
    foreach ($candidate in @('', ' ', 'n', 'no', 'nope', 'Y E S')) {
        Reset-Stubs
        $script:Answer = $candidate
        Add-BuildToolDefenderExclusions $fakeRepo $sandbox
        Assert-Eq 0 @($script:LiveProcesses).Count "answer '$candidate' adds no process exclusion"
        Assert-Eq 0 @($script:LivePaths).Count "answer '$candidate' adds no path exclusion"
        Assert-True (-not (Test-Path $recordFile)) "answer '$candidate' writes no uninstall record"
    }

    # -------------------------------------------------------------------------
    # 6. Accepting excludes the computed build directories -- and nothing else
    # -------------------------------------------------------------------------
    Say ""
    Say "Accepted" 'Cyan'
    Reset-Stubs
    $script:Answer = 'y'
    Add-BuildToolDefenderExclusions $fakeRepo $sandbox
    Assert-Eq 0 @($script:LiveProcesses).Count "no -ExclusionProcess entry is ever added"
    foreach ($p in @($fakeTarget, $fakeCargo, $fakeRustup)) {
        Assert-True (@($script:LivePaths) -contains $p) "path exclusion added: $p"
    }
    Assert-Eq 3 @($script:LivePaths).Count "exactly the three build directories are excluded"
    Assert-True (@($script:Marked) -contains 'defender-build-exclusions') "step marked done so a re-run does not re-prompt"
    Assert-Eq 0 @($script:Warnings).Count "no warnings on the happy path"

    Assert-True (Test-Path $recordFile) "uninstall record written"
    $record = Get-Content $recordFile -Raw | ConvertFrom-Json
    Assert-Eq 0 @($record.Processes).Count "record lists no process exclusions"
    Assert-Eq 3 @($record.Paths).Count "record lists the three directories"
    Assert-True (@($record.Paths) -contains $fakeTarget) "record lists the target dir"

    # A second Phase 2 run must not ask again.
    $script:StepDone = $true
    $script:Answer = 'y'
    $before = @($script:LivePaths).Count
    Add-BuildToolDefenderExclusions $fakeRepo $sandbox
    Assert-Eq $before @($script:LivePaths).Count "already-done step does not re-add"

    # -------------------------------------------------------------------------
    # 6b. An exclusion the developer already had is not ours to record -- and so
    #     not ours for the uninstaller to delete later.
    # -------------------------------------------------------------------------
    Say ""
    Say "Pre-existing exclusions" 'Cyan'
    Reset-Stubs
    Remove-Item $recordFile -Force -ErrorAction SilentlyContinue
    $script:Answer = 'y'
    # Seeded with entries the CURRENT code also wants, so this cannot pass just
    # because the seeded path happens to be outside the computed list.
    $script:LivePaths = @($fakeTarget, $fakeCargo)   # the developer excluded these himself
    Add-BuildToolDefenderExclusions $fakeRepo $sandbox
    Assert-Eq 3 @($script:LivePaths).Count "a pre-existing exclusion is not added a second time"
    Assert-Eq 0 @($script:Warnings).Count "a pre-existing exclusion is not a failure"
    Assert-True (@($script:Marked) -contains 'defender-build-exclusions') "a pre-existing exclusion still marks the step done"
    Assert-True (Test-Path $recordFile) "uninstall record still written"
    $record = Get-Content $recordFile -Raw | ConvertFrom-Json
    Assert-True (-not (@($record.Paths) -contains $fakeTarget)) "a pre-existing exclusion is NOT recorded as ours"
    Assert-True (-not (@($record.Paths) -contains $fakeCargo)) "the other pre-existing exclusion is NOT recorded either"
    Assert-True (@($record.Paths) -contains $fakeRustup) "what we did add IS recorded"
    Assert-Eq 1 @($record.Paths).Count "only what we added is recorded"

    $script:InstallerHelperHits = 0
    Invoke-UninstallerRemove $record
    Assert-Eq 0 $script:InstallerHelperHits "the pre-existing check also runs the uninstaller's own copy"
    Assert-True (@($script:LivePaths) -contains $fakeTarget) "uninstall leaves the developer's own exclusion in place"
    Assert-True (@($script:LivePaths) -contains $fakeCargo) "uninstall leaves the developer's other exclusion in place"
    Assert-True (-not (@($script:LivePaths) -contains $fakeRustup)) "uninstall removes what we added"

    # -------------------------------------------------------------------------
    # 6c. A retry after a PARTIAL first attempt must MERGE into the record, not
    #     overwrite it. Run 1 got one path in and recorded it; run 2 sees that
    #     path as already-live (so the F7 pre-existing rule correctly declines
    #     to re-record it) and adds the other two. If run 2 overwrote the file,
    #     run 1's path would drop out of the uninstall record and stay excluded
    #     in Defender forever -- breaking the very contract the record exists
    #     for ("the uninstaller removes exactly what is written here").
    # -------------------------------------------------------------------------
    Say ""
    Say "Retry after a partial first attempt" 'Cyan'
    Reset-Stubs
    @{
        Processes = @()
        Paths     = @($fakeTarget)
        Timestamp = (Get-Date -Format 'o')
    } | ConvertTo-Json | Set-Content -Path $recordFile -Encoding UTF8
    $script:LivePaths = @($fakeTarget)   # what run 1 managed to add before failing
    $script:Answer = 'y'
    Add-BuildToolDefenderExclusions $fakeRepo $sandbox
    $record = Get-Content $recordFile -Raw | ConvertFrom-Json
    Assert-True (@($record.Paths) -contains $fakeTarget) "run 1's recorded path survives run 2"
    Assert-True (@($record.Paths) -contains $fakeCargo) "run 2's new path is recorded"
    Assert-True (@($record.Paths) -contains $fakeRustup) "run 2's other new path is recorded"
    Assert-Eq 3 @($record.Paths).Count "the record is the union of both runs, with no duplicates"

    # And the merged record must still drive a complete uninstall.
    Invoke-UninstallerRemove $record
    Assert-Eq 0 @($script:LivePaths).Count "a merged record removes every exclusion both runs added"

    # -------------------------------------------------------------------------
    # 7. Not a checkout -> never prompts at all
    # -------------------------------------------------------------------------
    Reset-Stubs
    Remove-Item $recordFile -Force -ErrorAction SilentlyContinue
    $script:Answer = 'y'
    Add-BuildToolDefenderExclusions $null $sandbox
    Assert-Eq 0 @($script:LivePaths).Count "no checkout means no exclusions"
    Assert-True (-not (Test-Path $recordFile)) "no checkout means no uninstall record"

    # -------------------------------------------------------------------------
    # 8. Non-fatal: Tamper Protection and hard failures only warn
    # -------------------------------------------------------------------------
    Say ""
    Say "Failure handling" 'Cyan'
    Reset-Stubs
    $script:Answer = 'y'
    $script:AddThrows = $true
    $threw = $false
    try { Add-BuildToolDefenderExclusions $fakeRepo $sandbox } catch { $threw = $true }
    Assert-True (-not $threw) "Add-MpPreference failure does not throw"
    Assert-True (@($script:Warnings).Count -gt 0) "Add-MpPreference failure warns"
    Assert-True ((@($script:Warnings) -join "`n") -match 'Windows Security') "failure points at the Windows Security GUI"
    Assert-True (-not (@($script:Marked) -contains 'defender-build-exclusions')) "failed add is not marked done"

    Reset-Stubs
    $script:Answer = 'y'
    $script:SwallowAdds = $true    # call succeeds, Defender records nothing
    $threw = $false
    try { Add-BuildToolDefenderExclusions $fakeRepo $sandbox } catch { $threw = $true }
    Assert-True (-not $threw) "silently-ignored add does not throw"
    Assert-True ((@($script:Warnings) -join "`n") -match 'Windows Security') "read-back catches the silent no-op"
    Assert-True (-not (@($script:Marked) -contains 'defender-build-exclusions')) "unverified add is not marked done"

    Reset-Stubs
    $script:Answer = 'y'
    $script:GetThrows = $true
    $threw = $false
    try { Add-BuildToolDefenderExclusions $fakeRepo $sandbox } catch { $threw = $true }
    Assert-True (-not $threw) "Get-MpPreference failure does not throw"

    # -------------------------------------------------------------------------
    # 9. Uninstall symmetry
    # -------------------------------------------------------------------------
    Say ""
    Say "Uninstall symmetry" 'Cyan'
    Reset-Stubs
    Remove-Item $recordFile -Force -ErrorAction SilentlyContinue
    $script:Answer = 'y'
    Add-BuildToolDefenderExclusions $fakeRepo $sandbox
    $script:LiveProcesses = @($script:LiveProcesses) + 'someone-elses.exe'
    $script:LivePaths     = @($script:LivePaths) + 'C:\Not\DML'
    $record = Get-Content $recordFile -Raw | ConvertFrom-Json

    $script:InstallerHelperHits = 0
    Invoke-UninstallerRemove $record
    Assert-Eq 0 $script:InstallerHelperHits "the UNINSTALLER's Test-ExclusionRecorded is the one that runs"
    foreach ($p in @($fakeTarget, $fakeCargo, $fakeRustup)) {
        Assert-True (@($script:Removed) -contains $p) "uninstall removes path exclusion: $p"
    }
    Assert-Eq 0 @($script:LivePaths | Where-Object { $_ -eq $fakeTarget }).Count "target dir no longer excluded after uninstall"
    Assert-True (@($script:LiveProcesses) -contains 'someone-elses.exe') "unrelated process exclusion survives"
    Assert-True (@($script:LivePaths) -contains 'C:\Not\DML') "unrelated path exclusion survives"

    # Defender hands paths back case-normalized. The uninstaller's own copy of
    # the comparison must be case-blind, or nothing is ever removed. This is
    # only observable now that the uninstaller's copy is what actually runs.
    Reset-Stubs
    $script:LivePaths = @($fakeTarget.ToUpper())
    $script:InstallerHelperHits = 0
    Invoke-UninstallerRemove ([pscustomobject]@{ Processes = @(); Paths = @($fakeTarget) })
    Assert-Eq 0 $script:InstallerHelperHits "case-normalization check also runs the uninstaller's copy"
    Assert-True (@($script:Removed) -contains $fakeTarget) "uninstall matches a case-normalized path from Defender"

    # Records written by an older installer listed process exclusions; the
    # uninstaller must still clean those up.
    Reset-Stubs
    $script:LiveProcesses = @('cargo.exe', 'node.exe')
    Invoke-UninstallerRemove ([pscustomobject]@{ Processes = @('cargo.exe', 'node.exe'); Paths = @() })
    Assert-True (@($script:Removed) -contains 'cargo.exe') "a legacy process record is still honoured"
    Assert-True (@($script:Removed) -contains 'node.exe') "a legacy process record is still honoured (node.exe)"

    # Nothing recorded / nothing present: must be a quiet no-op, never a throw.
    Reset-Stubs
    $threw = $false
    try { Invoke-UninstallerRemove $null } catch { $threw = $true }
    Assert-True (-not $threw) "missing record is a no-op"
    Reset-Stubs
    $threw = $false
    try { Invoke-UninstallerRemove ([pscustomobject]@{ Processes = @('cargo.exe'); Paths = @($fakeTarget) }) } catch { $threw = $true }
    Assert-True (-not $threw) "record for already-removed exclusions is a no-op"
    Assert-Eq 0 @($script:Removed).Count "nothing is removed when nothing is present"

    Reset-Stubs
    $script:LivePaths    = @($fakeTarget)
    $script:RemoveThrows = $true
    $threw = $false
    try { Invoke-UninstallerRemove ([pscustomobject]@{ Processes = @(); Paths = @($fakeTarget) }) } catch { $threw = $true }
    Assert-True (-not $threw) "Remove-MpPreference failure does not throw"
    Assert-True ((@($script:Warnings) -join "`n") -match 'Windows Security') "uninstall failure points at the Windows Security GUI"
} finally {
    Set-EnvOrClear 'CARGO_HOME'  $savedCargoHome
    Set-EnvOrClear 'RUSTUP_HOME' $savedRustupHome
    Remove-Item $sandbox -Recurse -Force -ErrorAction SilentlyContinue
}

Say ""
if ($script:Failures -eq 0) {
    Say "$script:Checks checks passed" 'Green'
    exit 0
} else {
    Say "$script:Failures/$script:Checks checks FAILED" 'Red'
    exit 1
}
