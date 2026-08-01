# Test-InstallerNative.ps1 -- covers Install-DML-Native.ps1.
#
# Same no-framework style as Test-InstallerDefender.ps1: a plain PS 5.1 script
# that parses the installer, lifts functions out of the AST and runs them
# against stubs. The installer itself is NEVER executed end to end -- it would
# download yq and touch Defender.
#
#   powershell -ExecutionPolicy Bypass -File guides\DML-Windows\tests\Test-InstallerNative.ps1

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$here      = Split-Path -Parent $MyInvocation.MyCommand.Path
$guides    = Split-Path -Parent $here
$installer = Join-Path $guides 'Install-DML-Native.ps1'

$script:Failures = 0
$script:Checks   = 0
function Say([string]$m, [string]$color = 'Gray') {
    Microsoft.PowerShell.Utility\Write-Host $m -ForegroundColor $color
}
function Assert-True([bool]$cond, [string]$what) {
    $script:Checks++
    if ($cond) { Say "  ok   $what" 'DarkGreen' }
    else { $script:Failures++; Say "  FAIL $what" 'Red' }
}
function Assert-Eq($expected, $actual, [string]$what) {
    Assert-True ("$expected" -eq "$actual") "$what (expected '$expected', got '$actual')"
}

if (-not (Test-Path -LiteralPath $installer)) {
    Say "Install-DML-Native.ps1 not found at $installer" 'Red'
    exit 1
}
$rawSrc = Get-Content -LiteralPath $installer -Raw
$tokens = $null
$errors = $null
$ast = [System.Management.Automation.Language.Parser]::ParseFile($installer, [ref]$tokens, [ref]$errors)

# $src is the CODE with comments stripped. The banned-string scan below must
# check what the script DOES, not what its comments say it deliberately does
# not do -- the installer explains at length that it never runs `wsl --install`
# or `pacman`, and a naive whole-file grep reads those explanations as the very
# thing they rule out. (The same trap bit feature-keys.test.ts on 2026-08-01,
# where a comment containing a lock call was parsed as a call site.)
$src = ($tokens | Where-Object { $_.Kind -ne 'Comment' } | ForEach-Object { $_.Text }) -join ' '

function Get-FunctionAst($root, [string]$name) {
    $root.FindAll({
        param($n)
        $n -is [System.Management.Automation.Language.FunctionDefinitionAst] -and $n.Name -eq $name
    }, $true) | Select-Object -First 1
}

Say "`n== It parses ==" 'White'
Assert-True ($null -ne $ast) 'the script parses as valid PowerShell'

# --------------------------------------------------------------------------
Say "`n== The WSL era must not leak back in ==" 'White'
# This script exists BECAUSE the WSL route is being retired. A reviewer adding
# "just one" wsl step here would quietly recreate the thing it replaces, and
# nothing else in the repo would notice.
foreach ($banned in @('wsl --install', 'wsl --set-default', 'pacman', 'systemctl', 'usermod', 'DML-Launcher.exe', 'dml-arch')) {
    Assert-True (-not ($src -match [regex]::Escape($banned))) "no '$banned' anywhere in the native installer"
}
# The C# tray is the specific thing that produced two indistinguishable
# launchers on one machine (SHIP-LIST 4.0b).
Assert-True (-not ($src -match 'C:\\DML-tray')) 'no C# tray install path'

# --------------------------------------------------------------------------
Say "`n== The yq download is pinned AND verified ==" 'White'
Assert-True ($src -match "YqVersion\s*=\s*'v\d+\.\d+\.\d+'") 'yq version is pinned to an exact release'
Assert-True ($src -match "YqSha256\s*=\s*'[0-9A-Fa-f]{64}'") 'yq has a full SHA256 pin'
Assert-True (-not ($src -match 'releases/latest')) 'the download never resolves "latest"'

$yqFn = Get-FunctionAst $ast 'Install-YqPinned'
Assert-True ($null -ne $yqFn) 'Install-YqPinned exists'
if ($yqFn) {
    $body = $yqFn.Body.Extent.Text
    # ORDERING: the hash must be checked on the temp file and the file removed
    # on mismatch. Verifying after the move leaves a bad binary sitting at the
    # path everything else reads.
    # THE INVARIANT IS `throw` BEFORE `Move-Item`, not "a Get-FileHash appears
    # somewhere earlier". The first version of this check compared the index of
    # the first Get-FileHash against Move-Item and was VACUOUS: the function
    # opens with an already-present hash check, so an inserted
    # `Move-Item`-then-verify still had a Get-FileHash before it and the test
    # stayed green under exactly the mutation it existed to catch.
    #
    # Anchoring on the refusal is what actually matters: the file may only
    # reach its final path after the mismatch branch has had its chance to
    # abort.
    $throwIdx = $body.IndexOf('throw')
    $moveIdx  = $body.IndexOf('Move-Item')
    Assert-True ($throwIdx -ge 0) 'a mismatched download aborts rather than continuing'
    Assert-True ($moveIdx -ge 0) 'the verified file is moved into place'
    Assert-True ($throwIdx -ge 0 -and $moveIdx -ge 0 -and $throwIdx -lt $moveIdx) `
        'nothing is moved into place until the hash mismatch has had its chance to throw'
    Assert-True ($body -match 'Remove-Item') 'a mismatched download is deleted, not left behind'
    # And the verification must read the TEMP file, never the installed path --
    # hashing the target after the move proves nothing about what was
    # downloaded.
    Assert-True ($body -match 'Get-FileHash -LiteralPath \$tmp') 'the download is hashed at its temp path'
}

# --------------------------------------------------------------------------
Say "`n== Docker Desktop is instructed, not installed ==" 'White'
# It is a separate product with its own licence terms. Installing it silently
# makes the user's licensing decision for them.
Assert-True ($src -match '\$InstallDocker') 'installing Docker Desktop is behind an explicit switch'
$wingetCalls = ([regex]::Matches($src, 'winget install')).Count
Assert-True ($wingetCalls -le 1) 'at most one winget install call'
if ($wingetCalls -eq 1) {
    # The single call must sit inside the opt-in branch. Asked of the AST
    # rather than by regex over text: the token-joined $src normalises
    # whitespace, so a source-shaped pattern like `elseif ($InstallDocker)`
    # fails on formatting rather than on meaning -- a test that goes red when
    # someone reindents is a test people learn to ignore.
    $wingetAst = $ast.FindAll({
        param($n)
        $n -is [System.Management.Automation.Language.CommandAst] -and
        $n.GetCommandName() -eq 'winget'
    }, $true) | Select-Object -First 1
    Assert-True ($null -ne $wingetAst) 'the winget call is a real command, not a string'
    if ($wingetAst) {
        # MEMBERSHIP, not ancestry. The first version walked ancestor
        # if-statements and asked whether ANY of their conditions mentioned
        # InstallDocker -- which stays true when the call is moved into the
        # unguarded `else` of that very statement, the one mutation this check
        # exists to catch. Reproduced by a reviewer.
        #
        # The call must fall inside the BODY of a clause whose condition names
        # InstallDocker.
        $guarded = $false
        $node = $wingetAst.Parent
        while ($null -ne $node) {
            if ($node -is [System.Management.Automation.Language.IfStatementAst]) {
                foreach ($clause in $node.Clauses) {
                    if ($clause.Item1.Extent.Text -match 'InstallDocker') {
                        $b = $clause.Item2.Extent
                        $w = $wingetAst.Extent
                        if ($w.StartOffset -ge $b.StartOffset -and $w.EndOffset -le $b.EndOffset) {
                            $guarded = $true
                        }
                    }
                }
            }
            $node = $node.Parent
        }
        Assert-True $guarded 'the winget call is inside the BODY of the -InstallDocker branch'
    }
}
Assert-True ($src -match 'personal use') 'the licence position is stated to the user'

# --------------------------------------------------------------------------
Say "`n== Defender exclusions are DIRECTORY-scoped ==" 'White'
# Excluding a compiler BINARY leaves it unscanned machine-wide -- a much larger
# hole than skipping one build tree.
Assert-True ($src -match 'ExclusionPath') 'uses -ExclusionPath'
Assert-True (-not ($src -match 'ExclusionProcess')) 'never uses -ExclusionProcess'

# --------------------------------------------------------------------------
Say "`n== -DryRun performs no side effects ==" 'White'
$chg = Get-FunctionAst $ast 'Invoke-Change'
Assert-True ($null -ne $chg) 'every side effect funnels through Invoke-Change'
if ($chg) {
    Assert-True ($chg.Body.Extent.Text -match 'if \(\$DryRun\)') 'Invoke-Change short-circuits on -DryRun'
}
# The real proof: run the whole installer with -DryRun against a scratch games
# dir and assert nothing appeared. A guard that exists but is bypassed by one
# stray New-Item is exactly what this catches.
$scratch = Join-Path ([System.IO.Path]::GetTempPath()) ("dml-native-dry-" + [System.Guid]::NewGuid().ToString('N'))
try {
    $out = & powershell -NoProfile -ExecutionPolicy Bypass -File $installer -GamesDir $scratch -DryRun 2>&1
    $joined = ($out -join "`n")
    Assert-True (-not (Test-Path -LiteralPath $scratch)) '-DryRun created no games directory'
    Assert-True ($joined -match 'DRY') '-DryRun announced what it would have done'
    # AND IT REACHED THE END. "no directory + the word DRY" is satisfied by an
    # installer that dies right after the banner, which is exactly the failure
    # mode the Defender fix exists for -- so on its own it proved nothing.
    #
    # Anchored on the LAST step's own heading rather than on exit 0: exit 1 is
    # legitimate here (this machine may have no Docker Desktop), so requiring
    # success would make the check fail for an honest reason.
    Assert-True ($joined -match 'Writing launcher settings') '-DryRun ran through to the final step'
} catch {
    Assert-True $false "the -DryRun run failed: $($_.Exception.Message)"
} finally {
    if (Test-Path -LiteralPath $scratch) { Remove-Item -Recurse -Force $scratch -ErrorAction SilentlyContinue }
}

# --------------------------------------------------------------------------
Say "`n== launcher.json is MERGED, not clobbered ==" 'White'
# close_to_tray and start_with_windows are set from inside the app. Rewriting
# the whole document to change two keys resets them on every re-run.
$cfgFn = Get-FunctionAst $ast 'Write-LauncherConfig'
Assert-True ($null -ne $cfgFn) 'Write-LauncherConfig exists'
if ($cfgFn) {
    $body = $cfgFn.Body.Extent.Text
    Assert-True ($body -match 'ConvertFrom-Json') 'it reads the existing file first'
    Assert-True ($body -match 'PSObject\.Properties') 'it carries every existing key forward'
    # A BOM makes serde_json reject the file, so the launcher would silently
    # fall back to defaults and ignore everything this installer configured.
    Assert-True ($body -match 'UTF8Encoding\(\$false\)') 'it writes UTF-8 WITHOUT a BOM'
}

# Behavioural: keys the installer does not own must survive.
if ($cfgFn) {
    $tmpCfg = Join-Path ([System.IO.Path]::GetTempPath()) ("dml-cfg-" + [System.Guid]::NewGuid().ToString('N') + ".json")
    '{"close_to_tray":false,"start_with_windows":true,"backend":"wsl"}' |
        Set-Content -LiteralPath $tmpCfg -Encoding utf8
    $DryRun = $false
    function Invoke-Change([string]$What, [scriptblock]$Action) { & $Action; return $true }
    function Info([string]$m) { }
    function Warn([string]$m) { }
    . ([scriptblock]::Create($cfgFn.Extent.Text))
    Write-LauncherConfig $tmpCfg 'C:\games' 'C:\games\tools\yq.exe'
    $after = Get-Content -LiteralPath $tmpCfg -Raw | ConvertFrom-Json
    $names0 = @($after.PSObject.Properties.Name)
    Assert-Eq 'native' $after.backend 'backend was set to native'

    # THE KEYS ARE CHECKED AGAINST THE RUST STRUCT, not against the installer's
    # own spelling. This assertion used to read `$after.games_dir` -- the exact
    # string the writer emitted -- so it validated the writer against itself and
    # stayed green while `games_dir` and `yq_bin` were being SILENTLY DROPPED by
    # serde. launcher_config.rs is #[serde(rename_all = "camelCase")] with no
    # alias, so a snake_case key is not an error, it is ignored, and only
    # `backend` (one word, identical in both cases) survived.
    #
    # Reading the contract from the Rust source is what makes this test able to
    # fail: rename a field there and this goes red rather than agreeing with
    # whatever the PowerShell happens to say.
    $cfgRs = Join-Path (Split-Path -Parent (Split-Path -Parent $guides)) 'crates\dml-core\src\launcher_config.rs'
    Assert-True (Test-Path -LiteralPath $cfgRs) 'launcher_config.rs found (the contract this must match)'
    if (Test-Path -LiteralPath $cfgRs) {
        $rs = Get-Content -LiteralPath $cfgRs -Raw
        Assert-True ($rs -match 'rename_all\s*=\s*"camelCase"') 'launcher_config.rs still uses camelCase'
        # snake_case field -> the camelCase key serde actually reads.
        function ConvertTo-CamelKey([string]$snake) {
            $parts = $snake -split '_'
            $out = $parts[0]
            foreach ($x in $parts[1..($parts.Count - 1)]) {
                if ($x.Length -gt 0) { $out += $x.Substring(0,1).ToUpper() + $x.Substring(1) }
            }
            return $out
        }
        foreach ($field in @('games_dir', 'yq_bin')) {
            Assert-True ($rs -match "pub\s+$field\s*:") "launcher_config.rs still declares $field"
            $key = ConvertTo-CamelKey $field
            Assert-True ($names0 -contains $key) "the installer writes '$key' (the key serde reads for $field)"
        }
        Assert-Eq 'C:\games' $after.gamesDir 'the games dir landed under the key serde reads'
    }
    # Asked via PSObject.Properties: under StrictMode a MISSING property throws
    # rather than comparing false, which turns the clobber mutation into a crash
    # instead of a clean red. A test that explodes still fails, but it fails
    # without saying what it found.
    $names = @($after.PSObject.Properties.Name)
    Assert-True ($names -contains 'close_to_tray') 'close_to_tray (user preference) still present'
    Assert-True ($names -contains 'start_with_windows') 'start_with_windows (user preference) still present'
    if ($names -contains 'close_to_tray') {
        Assert-Eq $false $after.close_to_tray 'close_to_tray kept its value'
    }
    if ($names -contains 'start_with_windows') {
        Assert-Eq $true $after.start_with_windows 'start_with_windows kept its value'
    }
    $raw = [System.IO.File]::ReadAllBytes($tmpCfg)
    Assert-True (-not ($raw.Length -ge 3 -and $raw[0] -eq 0xEF -and $raw[1] -eq 0xBB -and $raw[2] -eq 0xBF)) `
        'the written file has no UTF-8 BOM'
    Remove-Item -LiteralPath $tmpCfg -Force -ErrorAction SilentlyContinue
}

# --------------------------------------------------------------------------
Say "`n== Summary ==" 'White'
Say "  $script:Checks checks, $script:Failures failure(s)" $(if ($script:Failures) { 'Red' } else { 'Green' })
exit $(if ($script:Failures) { 1 } else { 0 })
