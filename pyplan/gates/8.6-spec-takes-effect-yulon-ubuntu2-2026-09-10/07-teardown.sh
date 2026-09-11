#!/usr/bin/env bash
# Step 07 -- put the box back. The debug strategy off, the staged party
# disbanded, this lane's harness off the server, and a restart through the app
# so the world is no longer running a script this lane wrote.
#
# What is deliberately LEFT: `env/dist/etc/modules/playerbots.conf`, and the
# six bridge scripts. Both are the app's own artefacts -- the conf is what the
# Modules tab's activation puts there and is the thing 8.6 needs, and the six
# scripts are `party.deploy`'s own output, five of which were already here.
source "$(dirname "$0")/lib.sh"

MASTER=Jurnaar
BOT=Lacosh

say "step 07, putting the box back: the debug strategy off, the staged party disbanded, this lane's harness removed, and a restart through the app. The deployed playerbots.conf stays -- it is the app's own artefact."
{
hdr "T18 live, step 07: teardown"

sect "the bot's talents handed back to the module's own chooser"
echo '(talents autopick runs PlayerbotFactory::InitTalentsTree(true) -- the same'
echo " randomiser that gave Lacosh the arcane 63/8/0 build step 06 read before the"
echo " press. It does NOT restore that exact build, and nothing could: a random"
echo " bot's talents are the module's to choose and it re-rolls them itself. This"
echo " puts the choice back where it was, and says so rather than claiming a"
echo " restore.)"
$PRESS whisper "$MASTER" "$BOT" talents autopick
sleep 8
$PRESS send ".saveall"
sleep 5
$PRESS sql "SELECT COUNT(*) AS talent_rows FROM acore_characters.character_talent WHERE guid=188;"
$PRESS sql "SELECT GROUP_CONCAT(spell ORDER BY spell) FROM acore_characters.character_talent WHERE guid=188;" > "$OUT/07-talents-after-autopick.txt"
head -c 400 "$OUT/07-talents-after-autopick.txt"; echo

sect "the debug strategy off, and the party disbanded"
WIN=$(utc)
$PRESS whisper "$MASTER" "$BOT" nc -debug
sleep 5
$PRESS grouprows
$PRESS send "dml_t18_stage disband $MASTER"
sleep 3
$PRESS send "dml_t18_stage show $MASTER"
$PRESS send "dml_t18_stage unwatch"
$PRESS grouprows
docker logs ac-worldserver --since "$WIN" 2>&1 | grep -E "t18_chat|t18_stage" || echo "(no lines)"

sect "this lane's harness off the server"
rm -f "$LUA/t18_stage.lua"
ls -l "$LUA"

sect "the restart window opens here"
SINCE=$(utc)
echo "SINCE=$SINCE"

sect "STOP and START, through the app"
$PRESS stop
$PRESS start
wait_bridge

sect "the world's own log: the six bridge scripts loaded and the harness did NOT"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "^\[(dml_|t18_)" || true
echo "-- count of t18_stage lines in this window, which must be 0 --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "t18_stage" || true

sect "and the conf is still in force after the teardown restart"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -iE "playerbots.conf|Loaded playerbots config" || true
$PRESS specs mage warlock

sect "waiting for the random bots to be back in the world"
wait_bots 500

sect "done"
} 2>&1 | tee "$OUT/07-teardown.log"
say "step 07 done -- the harness is gone, the party is disbanded, and the world is up on the deployed conf."
