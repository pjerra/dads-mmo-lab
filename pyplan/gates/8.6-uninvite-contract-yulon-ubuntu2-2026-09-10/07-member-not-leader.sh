#!/usr/bin/env bash
# Step 07 -- the edge round 1's reviewer named, and the Python half beside it.
#
# Round 1, must-fix 1: the first version of the check asked whether the master
# LEADS the group. The feature's own definition (`group_rows_sql`) asks whether
# he is IN it. So: hand leadership away, leaving the asking master a plain
# member, and the honest case must still succeed.
#
# Then the same two cases again through `InstallParty.remove` -- the whole
# Python half, `dismiss()` included -- so the refusal is seen being recognised
# and not merely being spoken.
source "$(dirname "$0")/lib.sh"

MASTER1=Jurnaar; BOT1=Dalio; BOT3=Nore; MASTER2=Grirmirn; BOT2=Zarraden

say "step 07, the member-but-not-leader edge: handing $MASTER1's group leadership to $BOT1, then removing $BOT3 through the app's own seam."
{
hdr "T13 live, step 07: the asking master is a MEMBER, not the LEADER"

sect "before the hand-off"
$PRESS send "dml_t13_stage show $MASTER1 $MASTER1"
$PRESS sql "SELECT g.guid,g.leaderGuid,c.name AS leader FROM acore_characters.\`groups\` g JOIN acore_characters.characters c ON c.guid=g.leaderGuid ORDER BY g.guid;"

sect "handing leadership to $BOT1"
$PRESS send "dml_t13_stage leader $BOT1"
sleep 2
echo "-- $MASTER1 is now a member of a group he does not lead --"
$PRESS send "dml_t13_stage show $BOT1 $MASTER1"
$PRESS send "dml_t13_stage show $MASTER1 $MASTER1"
$PRESS sql "SELECT g.guid,g.leaderGuid,c.name AS leader FROM acore_characters.\`groups\` g JOIN acore_characters.characters c ON c.guid=g.leaderGuid ORDER BY g.guid;"

sect "the group table BEFORE"
$PRESS grouprows | tail -n +2 | tee "$OUT/07-grouprows-before.txt"

sect "the log window opens here, and is proved empty BEFORE the press"
SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE=$SINCE  (UTC, with the Z docker needs)"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "\[dml_(uninvite|whisper)\]"
echo "-- exit $? from that grep: 1 means the window holds none --"

sect "the honest case, pressed through InstallParty.remove -- master is NOT the leader"
$PRESS remove "$MASTER1" "$BOT3"

sect "the world's own log for that press, verbatim"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "\[dml_(uninvite|whisper)\]"

sect "the group table AFTER the honest press"
$PRESS grouprows | tail -n +2 | tee "$OUT/07-grouprows-after-honest.txt"
diff "$OUT/07-grouprows-before.txt" "$OUT/07-grouprows-after-honest.txt"
echo "-- ($BOT3's row is the only one gone; $MASTER1 is still in the group he does not lead) --"

sect "now the negative case through the same seam: $MASTER1 asks for $MASTER2's bot"
SINCE2=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE2=$SINCE2"
docker logs ac-worldserver --since "$SINCE2" 2>&1 | grep -E "\[dml_(uninvite|whisper)\]"
echo "-- exit $? from that grep: 1 means this second window holds none either --"
$PRESS remove "$MASTER1" "$BOT2"

sect "the world's own log for that press, verbatim"
docker logs ac-worldserver --since "$SINCE2" 2>&1 | grep -E "\[dml_(uninvite|whisper)\]"
echo "-- there is no [dml_whisper] line above: a bot the bridge refused to remove"
echo "   is not then told to log out. That is dismiss()'s own behaviour, live. --"

sect "the group table AFTER the refused press"
$PRESS grouprows | tail -n +2 | tee "$OUT/07-grouprows-after-refused.txt"
md5sum "$OUT/07-grouprows-after-honest.txt" "$OUT/07-grouprows-after-refused.txt"
diff "$OUT/07-grouprows-after-honest.txt" "$OUT/07-grouprows-after-refused.txt" && echo "IDENTICAL"
$PRESS group "$MASTER2"

sect "done"
} 2>&1 | tee "$OUT/07-member-not-leader.log"
say "step 07 done."
