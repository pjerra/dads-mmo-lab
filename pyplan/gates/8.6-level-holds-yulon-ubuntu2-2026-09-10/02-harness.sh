#!/usr/bin/env bash
# T16 live half, step 02 -- the wall the panel's own Add hits on this box,
# captured; then this lane's staging harness on, and the world restarted
# through the app's own Stop and Start.
#
# The wall first, because it is the reason everything after it is staged: two
# presses of `party.add_command` over the app's own channel, with the world's
# log window opened before each and proved empty of the pattern.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}

say "step 02 -- pressing the panel's own add command twice to capture the refusal, then loading the staging harness and restarting the world through the app."

hdr "T16 step 02 -- the addclass wall, and the harness"

sect "the bridge as deployed, through the app's own deploy seam"
$PRESS deploy

sect "who is in the world at all -- the answer that decides what can be pressed"
$PRESS send server info

sect "PRESS 1 -- party.add_command over the app's own channel, master $MASTER"
SINCE=$(utc)
echo "window opens at $SINCE"
echo "dml_addclass lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 'dml_addclass')"
echo "online characters before: $($PRESS sql "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;" | tail -1)"
echo "group_member rows before: $($PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;" | tail -1)"
$PRESS addclass "$MASTER" mage
sleep 30
echo "online characters after:  $($PRESS sql "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;" | tail -1)"
echo "group_member rows after:  $($PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;" | tail -1)"
echo ">>> the world's own log for that window:"
docker logs ac-worldserver --since "$SINCE" 2>&1 | tail -20

sect "PRESS 2 -- InstallParty.add, the panel's own Add, with a chosen level"
SINCE2=$(utc)
echo "window opens at $SINCE2"
echo "dml_addclass lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE2" 2>&1 | grep -c 'dml_addclass')"
$PRESS addpress "$MASTER" mage 42
echo ">>> the world's own log for that window:"
docker logs ac-worldserver --since "$SINCE2" 2>&1 | tail -20
echo "group_member rows after:  $($PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;" | tail -1)"

sect "the module's own guard, quoted from the box"
MOD="$HOME/wowserver/modules/mod-playerbots"
grep -n "GET_PLAYERBOT_MGR(player)" "$MOD/src/Bot/PlayerbotMgr.cpp" | head -3
sed -n '875,884p' "$MOD/src/Bot/PlayerbotMgr.cpp"
grep -rn "define GET_PLAYERBOT_MGR\|GET_PLAYERBOT_MGR(" "$MOD/src/Bot/PlayerbotMgr.h" "$MOD/src/Bot/PlayerbotAI.h" 2>/dev/null | head -5
grep -rn "GetPlayerbotMgr" "$MOD/src/Bot/PlayerbotsMgr.cpp" 2>/dev/null | head -10

sect "the harness on"
cp -v "$GATE/t16_stage.lua" "$LUA/t16_stage.lua"
ls -l "$LUA"
md5sum "$LUA/t16_stage.lua" "$GATE/t16_stage.lua"

sect "the world stopped and started through the app's own two buttons"
SINCE3=$(utc)
echo "window opens at $SINCE3"
echo "t16_stage lines in the window BEFORE the restart: $(docker logs ac-worldserver --since "$SINCE3" 2>&1 | grep -c 't16_stage')"
$PRESS stop
$PRESS start
wait_bridge
wait_bots 500
echo ">>> the world's own log says the harness loaded:"
docker logs ac-worldserver --since "$SINCE3" 2>&1 | grep -E 't16_stage|dml_' | head -20

sect "the panel's own preconditions, after the restart"
$PRESS ground "$MASTER" 2>&1 | head -25

sect "done"
say "step 02 finished -- the refusal is captured, the harness is loaded and the world is back up."
echo "step 02 finished $(stamp)"
