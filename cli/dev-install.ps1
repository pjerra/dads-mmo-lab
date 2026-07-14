# Installs the built cli/dml into the dml-arch distro (dev loop).
param([string]$Distro = 'dml-arch')
$ErrorActionPreference = 'Stop'
$repoWsl = '/mnt/c/Users/perzi/dads-mmo-lab'
wsl -d $Distro -u root -- bash -lc "install -m 0755 $repoWsl/cli/dml /usr/local/bin/dml && dml version"
