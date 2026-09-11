#!/usr/bin/env bash
# Step 04 -- THE NEGATIVE CASE. Zarraden is in Grirmirn's party. Jurnaar asks
# for it. The bot must stay where it is, the reply must carry the refusal, and
# the world's own log must say so.
source "$(dirname "$0")/lib.sh"

MASTER1=Jurnaar; MASTER2=Grirmirn; BOT2=Zarraden

say "step 04, the negative case: whispering dml_uninvite $MASTER1 $BOT2 while $BOT2 is in $MASTER2's party. The bot must stay."
{
hdr "T13 live, step 04: the negative case -- a bot in the SECOND master's party"

sect "the group table BEFORE, written to a file so 'unchanged' can be proved by bytes"
$PRESS grouprows | tail -n +2 | tee "$OUT/04-grouprows-before.txt"
md5sum "$OUT/04-grouprows-before.txt"
$PRESS group "$MASTER2"
$PRESS group "$MASTER1"

sect "the log window opens here, and is proved empty BEFORE the whisper"
SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE=$SINCE  (UTC, with the Z docker needs -- a zone-less stamp would be"
echo " read in this +02:00 box's local time and open the window two hours early)"
echo "-- every [dml_uninvite] line in the window, before the whisper: --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep "\[dml_uninvite\]"
echo "-- exit $? from that grep: 1 means the window holds none, which is the point --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c "\[dml_uninvite\]" || true

sect "the whisper, over the same SOAP channel party.py's dismiss() uses"
echo "(the command string is built by party.uninvite_command(), not typed here)"
$PRESS uninvite "$MASTER1" "$BOT2"

sect "the world's own log for that window, verbatim"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep "\[dml_uninvite\]"

sect "the group table AFTER"
$PRESS grouprows | tail -n +2 | tee "$OUT/04-grouprows-after.txt"
md5sum "$OUT/04-grouprows-before.txt" "$OUT/04-grouprows-after.txt"
echo "-- diff before/after (no output = byte-identical) --"
diff "$OUT/04-grouprows-before.txt" "$OUT/04-grouprows-after.txt" && echo "IDENTICAL"

sect "and the seam's own reads agree"
$PRESS group "$MASTER2"
$PRESS group "$MASTER1"
$PRESS send "dml_t13_stage show $MASTER2 $BOT2"

sect "done"
} 2>&1 | tee "$OUT/04-negative.log"
say "step 04 done."
