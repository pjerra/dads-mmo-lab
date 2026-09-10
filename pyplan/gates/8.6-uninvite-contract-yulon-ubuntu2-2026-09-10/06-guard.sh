#!/usr/bin/env bash
# Step 06 -- THE FREE GUARD added in this commit. Round 2's reviewer, note 4:
# `dml_uninvite <master> <master>` passes IsMember (a player is a member of his
# own group) and would remove the master from his own party. Unreachable from
# the app, reachable from this channel, and one comparison to refuse.
source "$(dirname "$0")/lib.sh"

MASTER1=Jurnaar; BOT1=Dalio; BOT3=Nore

say "step 06, the free guard: re-staging $MASTER1's party, then whispering dml_uninvite $MASTER1 $MASTER1. The master must stay in his own group."
{
hdr "T13 live, step 06: dml_uninvite <master> <master>"

sect "re-staging $MASTER1's party, three members this time"
$PRESS send "dml_t13_stage group $MASTER1 $BOT1"
sleep 2
$PRESS send "dml_t13_stage add $MASTER1 $BOT3"
sleep 2
$PRESS send "dml_t13_stage show $MASTER1 $MASTER1"

sect "the group table BEFORE"
$PRESS grouprows | tail -n +2 | tee "$OUT/06-grouprows-before.txt"
$PRESS group "$MASTER1"

sect "the log window opens here, and is proved empty BEFORE the whisper"
SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE=$SINCE  (UTC, with the Z docker needs)"
echo "-- every [dml_uninvite] line in the window, before the whisper: --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep "\[dml_uninvite\]"
echo "-- exit $? from that grep: 1 means the window holds none --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "\[dml_uninvite\]" || true

sect "the whisper: the master named as his own bot"
$PRESS uninvite "$MASTER1" "$MASTER1"

sect "the world's own log for that window, verbatim"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep "\[dml_uninvite\]"

sect "the group table AFTER -- the master is still in his own party"
$PRESS grouprows | tail -n +2 | tee "$OUT/06-grouprows-after.txt"
md5sum "$OUT/06-grouprows-before.txt" "$OUT/06-grouprows-after.txt"
diff "$OUT/06-grouprows-before.txt" "$OUT/06-grouprows-after.txt" && echo "IDENTICAL"
$PRESS group "$MASTER1"
$PRESS send "dml_t13_stage show $MASTER1 $MASTER1"

sect "done"
} 2>&1 | tee "$OUT/06-guard.log"
say "step 06 done."
