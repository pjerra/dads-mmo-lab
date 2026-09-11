#!/bin/bash
echo "# Half 1, the box as left"
echo "stamp: $(date -Is)"
echo
echo "## containers"
docker ps -a --format "{{.Names}}|{{.Status}}|{{.Label \"com.docker.compose.project\"}}"
echo
echo "## docker inspect, the three Tortoise containers"
for c in tortoise-db tortoise-realmd tortoise-mangosd; do
  docker inspect -f "  > {{.Name}} status={{.State.Status}} running={{.State.Running}} exit={{.State.ExitCode}} finished={{.State.FinishedAt}}" "$c"
done
echo
echo "## ports 3724 / 8090"
ss -ltn | grep -E "3724|8090" || echo "  > nothing listening on either port"
echo
echo "## processes"
pgrep -c -f "[m]ain.py" >/dev/null && echo "  > a Yu'lon main.py runs" || echo "  > no Yu'lon process"
pgrep -x steam >/dev/null && echo "  > steam runs" || echo "  > no steam process"
pgrep -f "[W]oW.exe" >/dev/null && echo "  > the client runs" || echo "  > no client process"
echo
echo "## state.json"
cat ~/.local/share/yulon/state.json
echo
echo "## the client's realmlist, as left"
cat -A ~/clients/TurtleWoW/realmlist.wtf
echo
echo "## the server log Yu'lon saved on Stop"
ls -la ~/.local/share/yulon/logs/ 2>/dev/null | tail -3
echo
echo "## memory"
free -m | head -2
