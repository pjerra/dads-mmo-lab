#!/usr/bin/env bash
# T16 live half, step 03 -- THE MEASUREMENT, seven trials.
#
# Five send `character level <bot> 42` BY HAND over the seam's own channel at
# +0, +5, +10, +20 and +30 seconds after the group row appears; two send the
# same level through the app's own seam (`play.InstallPlay.set_level`, the one
# `party.add_bot` is handed) at the ends of that range, so "the app's press" and
# "the same command by hand" are compared rather than assumed equal.
#
# A FRESH join and a DIFFERENT bot each time. The join is a real core group
# invite through this lane's harness, because `dml_addclass` cannot run on a box
# with no non-bot session -- step 02 captures that refusal.
#
# Every trial reads the LIVE level (`.pinfo`) and `characters.level` on the same
# clock, forces the server's own `saveall` ten seconds after the send, and keeps
# reading until the row agrees or the trial's tail runs out.
#
# Each bot that actually joined is handed back to the module's own randomiser
# with `playerbots rndbot init <bot>` (RandomPlayerbotMgr::RandomizeFirst) as
# soon as its trial ends -- which re-rolls its level and its build and takes it
# out of the group, both of which are the module's to decide.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
LEVEL=${2:?level}
shift 2
LISTS=("$@")

say "step 03 -- the measurement: seven timed level presses on freshly joined bots in ${MASTER}'s party."

hdr "T16 step 03 -- the timing table"
echo "master = $MASTER   level asked for = $LEVEL"
echo "candidate lists, one per trial: ${LISTS[*]}"

sect "the master's own ground -- the module refuses an invite from a master more than five levels under the invitee"
$PRESS pinfo "$MASTER"
$PRESS dblevel "$MASTER"
grep -n '^AiPlayerbot.GroupInvitationPermission\|^AiPlayerbot.GearScoreCheck' \
    "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

SINCE=$(utc)
echo "log window opens at $SINCE"
echo "t16_stage lines in that window BEFORE the trials: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 't16_stage')"

sect "the chat capture on, for the master"
$PRESS send "dml_t16_stage watch $MASTER"

i=0
for row in "0 hand" "5 hand" "10 hand" "20 hand" "30 hand" "0 seam" "30 seam"; do
    set -- $row
    delay=$1; how=$2
    list=${LISTS[$i]}
    i=$((i + 1))
    sect "TRIAL $i -- delay=+${delay}s how=$how candidates=$list"
    $PRESS trial "$MASTER" "$list" "$delay" "$LEVEL" "$how" 90 | tee "$OUT/trial-$i.txt"
    bot=$(grep -o '^RESULT.*' "$OUT/trial-$i.txt" | grep -o 'bot=[A-Za-z]*' | head -1 | cut -d= -f2)
    if [ -n "${bot:-}" ]; then
        sect "handing $bot back to the module's own randomiser"
        $PRESS send "playerbots rndbot init $bot"
        sleep 6
        $PRESS dblevel "$bot"
    else
        echo "no bot joined in this trial; nothing to hand back"
    fi
    $PRESS send "dml_t16_stage show $MASTER"
    $PRESS members "$MASTER"
    sleep 3
done

sect "the party disbanded, if anything is left in it"
$PRESS send "dml_t16_stage show $MASTER"
$PRESS send "dml_t16_stage disband $MASTER"

sect "the master, handed back to the module's own randomiser too"
$PRESS send "playerbots rndbot init $MASTER"
sleep 6
$PRESS dblevel "$MASTER"

sect "the chat capture off"
$PRESS send "dml_t16_stage unwatch"

sect "the world's own log for the whole of step 03"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E 't16_stage|t16_chat|dml_' | tail -120

sect "the group table after the seven trials"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.groups;"

sect "done"
say "step 03 finished -- the timing table is measured."
echo "step 03 finished $(stamp)"
