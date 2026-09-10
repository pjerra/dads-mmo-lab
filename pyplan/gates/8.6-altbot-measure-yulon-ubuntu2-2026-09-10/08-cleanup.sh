#!/usr/bin/env bash
# Step 08 -- this lane's own directory off the box. Nothing under ~/wowserver
# was ever written, so there is nothing else to undo.
set -u
"$HOME/claude-say" "T26 measure: step 08, taking this lane's own working directory (~/t26-out) off the box. Nothing under ~/wowserver was written at any point." >/dev/null
{
echo "=============================================================="
echo "T26 measure, step 08: cleanup -- $(date +'%H:%M:%S %Z') -- $(hostname)"
echo "=============================================================="
echo "-- what this lane put on the box (scripts + captures, nothing else) --"
ls -l "$HOME/t26-out"
echo "-- the captures are already copied off; removing --"
rm -rf "$HOME/t26-out"
ls -d "$HOME/t26-out" 2>&1
echo "-- ~/wowserver is untouched: the five bridge scripts and the two LootPet files are still there --"
ls -1 "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"
echo "-- all three containers still up --"
docker ps --format '{{.Names}}\t{{.Status}}'
echo "-- the box's own clone was never touched --"
git -C "$HOME/dads-mmo-lab" log -1 --format='%h %s' 2>&1
git -C "$HOME/dads-mmo-lab" status --porcelain 2>&1 | head -5
} 2>&1
