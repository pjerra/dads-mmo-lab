#!/usr/bin/env bash
# Step 02 -- the six bridge scripts through the app's own deploy seam, this
# lane's staging harness beside them, a restart through the APP's own Stop and
# Start, and then the BEFORE reading: what `talents spec list` answers on a
# server with no deployed `playerbots.conf`.
#
# Why a harness at all, and why an invite rather than T13's GroupCreate, is in
# `t18_stage.lua`'s own header: `talents spec` reaches the module only at
# ALLOW_ALL, which for a random bot means `GetMaster() == from`, and the one
# line in the module that sets a master without a real player sitting at a
# client is `AcceptInvitationAction.cpp:51`, reached only by a real invite.
source "$(dirname "$0")/lib.sh"

MASTER=Jurnaar          # guid 1, account 1, Draenei warrior 80 -- T13's master
BOT=Lacosh              # guid 188, account 19, Gnome MAGE 80, same faction
CLASS=mage

say "step 02, deploying the six bridge scripts through party.deploy, installing this lane's staging harness, and RESTARTING the world through the app's own Stop and Start."
{
hdr "T18 live, step 02: deploy, restart through the app, and the BEFORE reading"

sect "the deploy, through the app's own seam (party.deploy -- 'Enable My Party')"
$PRESS deploy

sect "the deployed bytes are now this ticket's bytes"
md5sum "$LUA"/dml_*.lua
md5sum "$TREE/pylauncher/lua/party/"*.lua

sect "this lane's staging harness, beside the bridge and not part of it"
cp "$GATE/t18_stage.lua" "$LUA/t18_stage.lua"
md5sum "$GATE/t18_stage.lua" "$LUA/t18_stage.lua"
ls -l "$LUA"

sect "the restart window opens here"
SINCE=$(utc)
echo "SINCE=$SINCE   (UTC with the trailing Z docker needs: a zone-less stamp"
echo " is read in the CLIENT's local zone, which on this +02:00 box opens the"
echo " window two hours early -- the T3 trap of 2026-09-09)"
echo "-- the window BEFORE the restart, which must hold no load line of ours --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "t18_stage\|dml_botadd" || true
echo "(the number above is how many t18_stage/dml_botadd lines the window holds before the restart)"

sect "STOP, through the app's own Stop (docker.stop_staged)"
$PRESS stop

sect "START, through the app's own Start (docker.start_staged)"
$PRESS start

sect "waiting for the world to answer the bridge again"
wait_bridge

sect "what loaded this time -- the world's own log for this restart window"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "^\[(dml_|t18_)" || true

sect "and the conf line, which must STILL say the file is not there"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -iE "playerbots.conf|Loaded playerbots config" || true

sect "the app's own preconditions after the deploy -- all six scripts now"
$PRESS ground "$MASTER"

sect "waiting for the random bots to be back in the world"
wait_bots 500

sect "the two characters this lane will use"
$PRESS sql "SELECT guid, account, name, race, class, level, online FROM acore_characters.characters WHERE name IN ('$MASTER','$BOT') ORDER BY guid;"
echo "-- and $BOT's talents BEFORE anything is pressed --"
$PRESS sql "SELECT COUNT(*) AS talent_rows FROM acore_characters.character_talent WHERE guid=188;"
$PRESS sql "SELECT spell, specMask FROM acore_characters.character_talent WHERE guid=188 ORDER BY spell;" | head -60

sect "staging: the chat capture on, then a real group INVITE from $MASTER to $BOT"
$PRESS send "dml_t18_stage watch $MASTER $BOT"
$PRESS grouprows
$PRESS send "dml_t18_stage invite $MASTER $BOT"
sleep 5
$PRESS send "dml_t18_stage show $MASTER $BOT"
$PRESS grouprows
$PRESS ground "$MASTER" "$BOT"

sect "the debug strategy, so the module's replies are SAID rather than dropped"
WIN=$(utc)
echo "WIN=$WIN"
$PRESS whisper "$MASTER" "$BOT" nc +debug
sleep 8
docker logs ac-worldserver --since "$WIN" 2>&1 | grep -E "t18_chat|t18_stage" || echo "(no chat lines)"

sect "THE BEFORE READING -- talents spec list, with no deployed playerbots.conf"
WIN2=$(utc)
echo "WIN2=$WIN2"
echo "-- the window before the whisper, which must hold no reply --"
docker logs ac-worldserver --since "$WIN2" 2>&1 | grep -c "specs found" || true
$PRESS whisper "$MASTER" "$BOT" talents spec list
sleep 10
docker logs ac-worldserver --since "$WIN2" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "and what the module says its current spec is, before anything"
WIN3=$(utc)
$PRESS whisper "$MASTER" "$BOT" talents
sleep 8
docker logs ac-worldserver --since "$WIN3" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "done"
} 2>&1 | tee "$OUT/02-harness.log"
say "step 02 done -- the bridge and the harness are loaded, the party is staged, and the BEFORE reading is taken."
