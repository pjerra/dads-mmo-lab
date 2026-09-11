#!/usr/bin/env bash
# Step 04 -- restart the world through the app so `sConfigMgr` reads the conf
# the app just deployed, prove from the world's OWN log that it did, re-stage
# the party the restart dissolved, and take the AFTER reading of
# `talents spec list`.
source "$(dirname "$0")/lib.sh"

MASTER=Jurnaar
BOT=Lacosh

say "step 04, restarting the world through the app's own Stop and Start so the deployed playerbots.conf is read, then asking the module what specs it has now."
{
hdr "T18 live, step 04: restart through the app, and the AFTER reading"

sect "the restart window opens here"
SINCE=$(utc)
echo "SINCE=$SINCE"
echo "-- the window BEFORE the restart, which must hold no 'Loaded playerbots config' line --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "Loaded playerbots config" || true
echo "(the number above is how many such lines the window holds before the restart)"

sect "STOP, through the app's own Stop"
$PRESS stop

sect "START, through the app's own Start"
$PRESS start

sect "waiting for the world to answer the bridge again"
wait_bridge

sect "THE WORLD'S OWN LOG: what it says about the module conf this time"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -iE "playerbots.conf|Loaded playerbots config|Not found modules config|Loading TalentSpecs" || true
echo "-- and the count of the line step 01 caught, which must now be zero --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "Failed open file '/azerothcore/env/dist/etc/modules/playerbots.conf'" || true

sect "the scripts that loaded with it"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "^\[(dml_|t18_)" || true

sect "the app's own reading now"
$PRESS ground "$MASTER"
$PRESS conf

sect "waiting for the random bots to be back in the world"
wait_bots 500

sect "re-staging: chat capture, then the invite the restart dissolved"
$PRESS grouprows
$PRESS send "dml_t18_stage watch $MASTER $BOT"
$PRESS sql "SELECT guid, name, level, online FROM acore_characters.characters WHERE name IN ('$MASTER','$BOT');"
$PRESS send "dml_t18_stage invite $MASTER $BOT"
sleep 5
$PRESS send "dml_t18_stage show $MASTER $BOT"
$PRESS grouprows

sect "the debug strategy again (it does not survive a relog)"
WIN=$(utc)
echo "WIN=$WIN"
$PRESS whisper "$MASTER" "$BOT" nc +debug
sleep 8
docker logs ac-worldserver --since "$WIN" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "THE AFTER READING -- talents spec list, with the app's conf deployed"
WIN2=$(utc)
echo "WIN2=$WIN2"
echo "-- the window before the whisper, which must hold no reply --"
docker logs ac-worldserver --since "$WIN2" 2>&1 | grep -c "specs found" || true
$PRESS whisper "$MASTER" "$BOT" talents spec list
sleep 12
docker logs ac-worldserver --since "$WIN2" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "done"
} 2>&1 | tee "$OUT/04-restart-and-list.log"
say "step 04 done -- the world has read the deployed conf and the module has been asked what specs it has."
