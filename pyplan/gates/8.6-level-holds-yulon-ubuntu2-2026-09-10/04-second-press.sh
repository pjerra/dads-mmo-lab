#!/usr/bin/env bash
# T16 live half, step 04 -- THE SECOND PRESS, with the code half in the tree.
#
# `party._level_step` -- the function the panel's Add now calls -- against this
# server, with the app's own `play.InstallPlay.set_level`, the app's own channel
# and the app's own group read. `add_bot` itself cannot be pressed on this box
# (step 02: `dml_addclass` needs a non-bot master and there is none), so the
# press is the step under it, the way T18 pressed `party.spec_command` where the
# panel's Add was out of reach.
#
# Run TWICE, so the claim is not one press: a bot at some level the module chose
# and, second, one that is already at the level being asked for -- the "already"
# arm, which must send nothing and say so.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
LEVEL=${2:?level}
LIST_A=${3:?candidates a}
LIST_B=${4:?candidates b}

say "step 04 -- the second press: party._level_step against the live server, with the code half in the tree."

hdr "T16 step 04 -- the second press"

sect "the tree this is pressed from"
echo "PRESS = $PRESS"
grep -n '^SAVE_COMMAND\|^LEVEL_TRIES\|^LEVEL_SLEEP' "$TREE/pylauncher/yulon/party.py"
grep -n 'def _level_step' "$TREE/pylauncher/yulon/party.py"
md5sum "$TREE/pylauncher/yulon/party.py"

SINCE=$(utc)
echo "log window opens at $SINCE"
echo "t16_stage lines in that window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 't16_stage')"

sect "the master's ground"
$PRESS pinfo "$MASTER"
$PRESS members "$MASTER"

sect "PRESS A -- a bot at whatever level the module gave it, asked for $LEVEL"
$PRESS levelpress "$MASTER" "$LIST_A" "$LEVEL" | tee "$OUT/press-a.txt"
BOT_A=$(grep -o '^PRESS.*' "$OUT/press-a.txt" | grep -o 'bot=[A-Za-z]*' | head -1 | cut -d= -f2)
if [ -n "${BOT_A:-}" ]; then
    sect "PRESS A2 -- the SAME bot, asked for the level it is now at: the 'already' arm"
    $PRESS levelpress "$MASTER" "$BOT_A" "$LEVEL" | tee "$OUT/press-a2.txt"
    sect "handing $BOT_A back to the module's own randomiser"
    $PRESS send "playerbots rndbot init $BOT_A"
    sleep 6
    $PRESS dblevel "$BOT_A"
fi
$PRESS send "dml_t16_stage disband $MASTER"
sleep 3

sect "PRESS B -- a second bot, a second level, so the claim is not one press"
$PRESS levelpress "$MASTER" "$LIST_B" 55 | tee "$OUT/press-b.txt"
BOT_B=$(grep -o '^PRESS.*' "$OUT/press-b.txt" | grep -o 'bot=[A-Za-z]*' | head -1 | cut -d= -f2)
if [ -n "${BOT_B:-}" ]; then
    sect "handing $BOT_B back to the module's own randomiser"
    $PRESS send "playerbots rndbot init $BOT_B"
    sleep 6
    $PRESS dblevel "$BOT_B"
fi

sect "the party disbanded"
$PRESS send "dml_t16_stage show $MASTER"
$PRESS send "dml_t16_stage disband $MASTER"
$PRESS members "$MASTER"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;"

sect "the world's own log for this step"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E 't16_stage|t16_chat|dml_' | tail -40

sect "done"
say "step 04 finished -- the second press is captured."
echo "step 04 finished $(stamp)"
