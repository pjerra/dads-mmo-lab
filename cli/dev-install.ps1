# Installs the built cli/dml (+ the party/gm bridge Lua and the six title
# installers) into the dml-arch distro. DEV LOOP ONLY.
#
# THIS IS NO LONGER THE USER'S ROUTE (SHIP-LIST 4.2). A stranger who downloads
# the installer has no repo, so the launcher provisions the distro itself from
# the resources bundled inside the exe -- see
# launcher/src-tauri/src/provision.rs. This script survives because the dev loop
# needs to push a freshly-built cli/dml into the distro without rebuilding the
# launcher first.
#
# The two step lists cannot literally be one list (one is Rust, one is
# PowerShell), so provision.rs's
# `dev_install_ps1_installs_the_same_destinations_at_the_same_modes` reads this
# file and fails the test run if the destinations or modes drift apart.
param([string]$Distro = "dml-arch")
$ErrorActionPreference = 'Stop'

# Resolve the repo from THIS script's own location. It used to be one author's
# WSL checkout path, spelled out literally, which is why the script only ever
# worked on one machine.  (This file lives in <repo>/cli/.)
$repoWin = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$repoWsl = (& wsl.exe -d $Distro --exec wslpath -a -u $repoWin | Out-String).Trim()
if (-not $repoWsl) {
    throw "Could not translate '$repoWin' into a $Distro path. Is the distro installed (wsl -l -q)?"
}

# One bash -lc, with the repo root passed as $1 so a checkout path containing
# spaces survives. `--exec` (not the bare `--`) keeps wsl.exe from running this
# string through a shell of its own before bash ever sees it.
$steps = @(
    'set -e',
    'r="$1"',
    'install -D -m 0755 "$r/cli/dml" /usr/local/bin/dml',
    'install -D -m 0644 -t /usr/local/share/dml/lua/party "$r"/cli/lua/party/*.lua',
    'install -D -m 0644 -t /usr/local/share/dml/lua/gm "$r"/cli/lua/gm/*.lua',
    ('install -D -m 0755 -t /usr/local/share/dml/installers ' +
     '"$r/guides/wow-wotlk/install-wow-wotlk.sh" ' +
     '"$r/guides/wow-vanilla/install-wow-vanilla.sh" ' +
     '"$r/guides/wow-tbc/install-wow-tbc.sh" ' +
     '"$r/guides/Maplestory/install-maplestory.sh" ' +
     '"$r/guides/runescape/install-runescape.sh" ' +
     '"$r/guides/Mu-online/install-muonline.sh"'),
    'dml version'
)
& wsl.exe -d $Distro -u root --exec bash -lc ($steps -join '; ') _ $repoWsl
if ($LASTEXITCODE -ne 0) { throw "dev-install failed (exit $LASTEXITCODE)" }
