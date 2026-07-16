param([string]$Distro = "dml-arch")
$repoWsl = "/mnt/c/Users/perzi/dads-mmo-lab"
wsl -d $Distro -u root -- bash -lc "install -m 0755 $repoWsl/cli/dml /usr/local/bin/dml && mkdir -p /usr/local/share/dml/lua/party && install -m 0644 $repoWsl/cli/lua/party/*.lua /usr/local/share/dml/lua/party/ && dml version"
