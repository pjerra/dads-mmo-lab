#!/usr/bin/env bash
# Step 05 -- THE HONEST CASE. Dalio is in Jurnaar's own party. Jurnaar asks for
# it. It must go, and the world's log must say it went. Round 1's reviewer:
# "the live half's honest case is load-bearing, the negative capture alone is
# not enough" -- a check that refused everything would look exactly like step 04.
source "$(dirname "$0")/lib.sh"

MASTER1=Jurnaar; BOT1=Dalio

say "step 05, the honest case: whispering dml_uninvite $MASTER1 $BOT1 while $BOT1 IS in $MASTER1's party. The bot must go."
{
hdr "T13 live, step 05: the honest case -- a bot in the asking master's own party"

sect "the group table BEFORE"
$PRESS grouprows | tail -n +2 | tee "$OUT/05-grouprows-before.txt"
$PRESS group "$MASTER1"
$PRESS send "dml_t13_stage show $MASTER1 $BOT1"

sect "the log window opens here, and is proved empty BEFORE the whisper"
SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE=$SINCE  (UTC, with the Z docker needs)"
echo "-- every [dml_uninvite] line in the window, before the whisper: --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep "\[dml_uninvite\]"
echo "-- exit $? from that grep: 1 means the window holds none --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "\[dml_uninvite\]" || true

sect "the whisper, over the same SOAP channel party.py's dismiss() uses"
$PRESS uninvite "$MASTER1" "$BOT1"
echo "(an empty <result> is the SUCCESS shape here: the removing branch answers"
echo " only in the world's log -- the reply party.py's dismiss() already parses)"

sect "the world's own log for that window, verbatim"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep "\[dml_uninvite\]"

sect "the group table AFTER"
$PRESS grouprows | tail -n +2 | tee "$OUT/05-grouprows-after.txt"
echo "-- diff before/after: the two rows of $MASTER1's group are gone --"
diff "$OUT/05-grouprows-before.txt" "$OUT/05-grouprows-after.txt"
echo "-- (a two-player group whose second member leaves is disbanded by the core,"
echo "    so the leader's own row goes with it) --"

sect "and the seam's own reads agree"
$PRESS group "$MASTER1"
$PRESS send "dml_t13_stage show $MASTER1 $BOT1"
$PRESS sql "SELECT COUNT(*) AS group_member_rows FROM acore_characters.group_member;"

sect "the SECOND master's party is untouched by this"
$PRESS group Grirmirn

sect "done"
} 2>&1 | tee "$OUT/05-honest.log"
say "step 05 done."
