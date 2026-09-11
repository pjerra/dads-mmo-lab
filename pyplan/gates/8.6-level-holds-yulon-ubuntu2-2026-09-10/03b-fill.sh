#!/usr/bin/env bash
# T16 live half, step 03b -- the rows step 03 could not stage.
#
# A core group invite is answered by the bot on its own terms and roughly one
# candidate in three came: `PlayerbotSecurity::LevelFor` and the bot's own next
# AI check both have a say, and a trial whose three candidates all declined
# measured nothing rather than measuring something wrong. This step re-runs
# exactly those rows, with a longer candidate list and nothing else changed.
#
# Same script, same driver, same `trial` verb. Each row names its delay so the
# README's table can be read off the two logs together.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
LEVEL=${2:?level}
shift 2

say "step 03b -- re-running the trial rows whose candidates all declined the invite."

hdr "T16 step 03b -- the rows step 03 could not stage"
echo "master = $MASTER   level = $LEVEL"
$PRESS pinfo "$MASTER"

SINCE=$(utc)
echo "log window opens at $SINCE"

$PRESS send "dml_t16_stage watch $MASTER"

i=0
for row in "$@"; do
    delay=${row%%:*}
    how=${row#*:}; how=${how%%:*}
    list=${row##*:}
    i=$((i + 1))
    sect "FILL $i -- delay=+${delay}s how=$how candidates=$list"
    $PRESS trial "$MASTER" "$list" "$delay" "$LEVEL" "$how" 90 | tee "$OUT/fill-$i.txt"
    bot=$(grep -o '^RESULT.*' "$OUT/fill-$i.txt" | grep -o 'bot=[A-Za-z]*' | head -1 | cut -d= -f2)
    if [ -n "${bot:-}" ]; then
        sect "handing $bot back to the module's own randomiser"
        $PRESS send "playerbots rndbot init $bot"
        sleep 6
        $PRESS dblevel "$bot"
    else
        echo "no bot joined in this row either"
    fi
    $PRESS send "dml_t16_stage disband $MASTER"
    $PRESS members "$MASTER"
    sleep 3
done

sect "the chat capture off, the party disbanded"
$PRESS send "dml_t16_stage unwatch"
$PRESS send "dml_t16_stage disband $MASTER"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;"

sect "the world's own log for this step"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E 't16_stage|t16_chat|dml_' | tail -60

sect "done"
say "step 03b finished."
echo "step 03b finished $(stamp)"
