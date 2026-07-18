# Installs the built cli/dml (+ party bridge Lua) into the dml-arch distro (dev loop).
param([string]$Distro = "dml-arch")
$ErrorActionPreference = 'Stop'
$repoWsl = "/mnt/c/Users/perzi/dads-mmo-lab"
wsl -d $Distro -u root -- bash -lc "install -m 0755 $repoWsl/cli/dml /usr/local/bin/dml && mkdir -p /usr/local/share/dml/lua/party /usr/local/share/dml/lua/gm && install -m 0644 $repoWsl/cli/lua/party/*.lua /usr/local/share/dml/lua/party/ && install -m 0644 $repoWsl/cli/lua/gm/*.lua /usr/local/share/dml/lua/gm/ && mkdir -p /usr/local/share/dml/installers && install -m 0755 $repoWsl/guides/wow-wotlk/install-wow-wotlk.sh $repoWsl/guides/wow-vanilla/install-wow-vanilla.sh $repoWsl/guides/wow-tbc/install-wow-tbc.sh $repoWsl/guides/Maplestory/install-maplestory.sh $repoWsl/guides/runescape/install-runescape.sh $repoWsl/guides/Mu-online/install-muonline.sh /usr/local/share/dml/installers/ && dml version"
