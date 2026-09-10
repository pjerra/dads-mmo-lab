#!/usr/bin/env bash
# Step 02 -- redeploy the five bridge scripts through the app's OWN deploy seam
# (`party.deploy`, the function "Enable My Party" calls), then restart the world
# so the Lua engine reads them, and prove from the world's own log that it did.
source "$(dirname "$0")/lib.sh"
say "step 02, redeploying the five bridge scripts through party.deploy and RESTARTING ac-worldserver so the new dml_uninvite.lua is loaded."
{
hdr "T13 live, step 02: deploy through party.deploy, restart the world, prove the scripts loaded"

sect "the deploy, through the app's own seam"
$PRESS deploy

sect "the deployed bytes are now the ticket's bytes"
md5sum "$HOME/wowserver/env/dist/etc/modules/lua_scripts/dml_"*.lua
md5sum "$TREE/pylauncher/lua/party/"*.lua

sect "the restart window opens here"
SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE=$SINCE   (UTC with the Z docker needs: a zone-less stamp is read in"
echo " the CLIENT's local zone, which on this +02:00 box opens the window two"
echo " hours early -- the T3 trap of 2026-09-09)"
echo "-- the window BEFORE the restart, which must be empty of a load line --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "dml_uninvite" || true
echo "(the number above is how many dml_uninvite lines the window holds before the restart)"

sect "restarting ac-worldserver"
docker restart ac-worldserver

sect "waiting for the world to answer the bridge again"
for i in $(seq 1 60); do
    sleep 10
    ans=$($PRESS send dml_bridge_ping 2>&1 | tail -1)
    echo "$(stamp) try $i: $ans"
    case "$ans" in *DML-BRIDGE-READY*) break;; esac
done

sect "the world's own log for this restart window"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "^\[dml_" || true

sect "the line this ticket needs, verbatim"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep "dml_uninvite" || true

sect "liveness + preconditions after the restart"
$PRESS ground

sect "done"
} 2>&1 | tee "$OUT/02-deploy-restart.log"
say "step 02 done -- the five scripts are deployed and the world has restarted."
