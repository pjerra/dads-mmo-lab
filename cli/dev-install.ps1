# Installs the built cli/dml (+ party bridge Lua) into the dml-arch distro (dev loop).
param([string]$Distro = "dml-arch")
$ErrorActionPreference = 'Stop'
$repoWsl = "/mnt/c/Users/perzi/dads-mmo-lab"
wsl -d $Distro -u root -- bash -lc "install -m 0755 $repoWsl/cli/dml /usr/local/bin/dml && mkdir -p /usr/local/share/dml/lua/party /usr/local/share/dml/lua/gm && install -m 0644 $repoWsl/cli/lua/party/*.lua /usr/local/share/dml/lua/party/ && install -m 0644 $repoWsl/cli/lua/gm/*.lua /usr/local/share/dml/lua/gm/ && dml version"
