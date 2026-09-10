#!/usr/bin/env bash
# T16 live half, step 05 -- the harness off, and the world restarted without it.
#
# The world is restarted through the app's own Stop and Start, and the count of
# `t16_stage` lines in the window that restart opens is the proof the file is
# not loaded any more -- a file deleted from a directory the engine already read
# is still in the engine.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}

say "step 05 -- taking this lane's harness off the server and restarting the world through the app without it."

hdr "T16 step 05 -- teardown"

sect "nothing of this lane's is still in a party"
$PRESS send "dml_t16_stage show $MASTER"
$PRESS send "dml_t16_stage disband $MASTER"
$PRESS send "dml_t16_stage unwatch"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.groups;"

sect "the harness off the disk"
rm -v "$LUA/t16_stage.lua"
ls -1 "$LUA"

sect "the world stopped and started through the app's own two buttons"
SINCE=$(utc)
echo "window opens at $SINCE"
echo "t16_stage lines in the window BEFORE the restart: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 't16_stage')"
$PRESS stop
$PRESS start
wait_bridge
wait_bots 500
echo "t16_stage lines in the window AFTER the restart: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 't16_stage')"
echo ">>> what the engine loaded this time:"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E 'loaded --|\[dml_|\[t16_' | head -20

sect "the harness command is gone: the world does not answer it"
$PRESS send "dml_t16_stage show $MASTER"

sect "the panel's own preconditions, with the box back to itself"
$PRESS ground "$MASTER" 2>&1 | head -20

sect "done"
say "step 05 finished -- the harness is off and the world is up without it."
echo "step 05 finished $(stamp)"
