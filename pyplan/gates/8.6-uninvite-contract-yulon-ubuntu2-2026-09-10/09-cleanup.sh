#!/usr/bin/env bash
# Step 09 -- take this lane's own three directories off the box.
#
# Self-contained (it does not source lib.sh) because it deletes the tree lib.sh
# lives in: it was copied to /tmp on the box and run from there, and its output
# was captured to 09-cleanup.log on the laptop.
set -u
stamp() { date +"%H:%M:%S %Z"; }
"$HOME/claude-say" "T13 live: step 09, removing this lane's own directories (~/t13-live, ~/t13-venv, ~/t13-out). Nothing under ~/wowserver is touched." >/dev/null

echo "=============================================================="
echo "T13 live, step 09: the lane's own files off the box -- $(stamp) -- $(hostname)"
echo "=============================================================="

echo
echo "---- what is there before -- $(stamp) ----"
du -sh "$HOME/t13-live" "$HOME/t13-venv" "$HOME/t13-out" 2>&1

echo
echo "---- removing them -- $(stamp) ----"
rm -rf "$HOME/t13-live" "$HOME/t13-venv" "$HOME/t13-out" /tmp/secretcheck.py /tmp/secretcheck2.py
ls -d "$HOME/t13-live" "$HOME/t13-venv" "$HOME/t13-out" 2>&1
echo "(three 'No such file or directory' lines above is the point)"

echo
echo "---- the box's own clone was never touched -- $(stamp) ----"
git -C "$HOME/dads-mmo-lab" log --oneline -1
git -C "$HOME/dads-mmo-lab" status --short

echo
echo "---- the server, last look -- $(stamp) ----"
docker ps --format '{{.Names}}\t{{.Status}}'
ls -l "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"
docker logs ac-worldserver --since "$(date -u -d '30 minutes ago' +%Y-%m-%dT%H:%M:%SZ)" 2>&1 | grep -c "t13_stage" || true
echo "(the number above is how many t13_stage lines the last half hour of the world's log holds)"

echo
echo "---- done -- $(stamp) ----"
"$HOME/claude-say" "T13 live: step 09 done. The lane is finished; the world is up with the ticket's five bridge scripts deployed." >/dev/null
