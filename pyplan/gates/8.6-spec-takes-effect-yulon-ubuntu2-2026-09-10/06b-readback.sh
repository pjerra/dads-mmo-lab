#!/usr/bin/env bash
# Step 06b -- the readback, and why step 06 could not hear the press itself.
#
# `ChangeTalentsAction::Execute` calls `botAI->ResetStrategies()` BETWEEN
# `SpecPick(param)` and `botAI->TellMaster(out)`
# (`src/Ai/Base/Actions/ChangeTalentsAction.cpp:64-67` and `:93`), and
# ResetStrategies puts the non-combat strategies back to the conf's own list --
# which does not include `debug` (`AiPlayerbot.RandomBotNonCombatStrategies = ""`,
# line 1371 of the deployed conf). So the one branch of `TellMasterNoFacing`
# that a bot master can hear is switched off by the action's own reply path, and
# `Picking frost pve` is dropped exactly as `Spec <x> not found` was for T5.
# `talents spec list` is the one talents command that does NOT reset, which is
# why the list in step 04 came through.
#
# The readback therefore comes from `talents` with no argument -- the module
# reporting its own current tab -- with the debug strategy re-armed first.
source "$(dirname "$0")/lib.sh"

MASTER=Jurnaar
BOT=Lacosh
GUID=188

say "step 06b, re-arming the debug strategy the module's own spec action resets, and reading the bot's spec back from the module."
{
hdr "T18 live, step 06b: the readback, and the reply the module resets away"

sect "the module's own source, on this box, for why step 06 heard nothing"
sed -n '60,70p' "$HOME/wowserver/modules/mod-playerbots/src/Ai/Base/Actions/ChangeTalentsAction.cpp"
echo "..."
sed -n '92,95p' "$HOME/wowserver/modules/mod-playerbots/src/Ai/Base/Actions/ChangeTalentsAction.cpp"
echo "-- and the strategy list ResetStrategies puts back --"
grep -n '^AiPlayerbot.RandomBotNonCombatStrategies' "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

sect "the control from step 06 changed nothing -- the talents are still the pressed ones"
$PRESS send ".saveall"
sleep 5
$PRESS sql "SELECT GROUP_CONCAT(spell ORDER BY spell) FROM acore_characters.character_talent WHERE guid=$GUID;" > "$OUT/06-talents-after-control.txt"
if diff -q "$OUT/06-talents-after.txt" "$OUT/06-talents-after-control.txt" >/dev/null; then
    echo "IDENTICAL to 06-talents-after.txt -- 'warglaive pvp' moved nothing"
else
    echo "DIFFERENT -- something changed after the control, which was not expected"
fi
md5sum "$OUT/06-talents-after.txt" "$OUT/06-talents-after-control.txt"

sect "re-arming the debug strategy"
WIN=$(utc)
$PRESS whisper "$MASTER" "$BOT" nc +debug
sleep 8
docker logs ac-worldserver --since "$WIN" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "THE READBACK -- what the module says the bot's spec is, after the press"
WIN2=$(utc)
echo "WIN2=$WIN2"
$PRESS whisper "$MASTER" "$BOT" talents
sleep 10
docker logs ac-worldserver --since "$WIN2" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"
echo "(step 06 read 'arcane (63/8/0)' before the press; step 04's list gives"
echo " 'frost pve' as (18-0-53))"

sect "a SECOND chosen name, so one press is not the whole claim"
$PRESS sql "SELECT GROUP_CONCAT(spell ORDER BY spell) FROM acore_characters.character_talent WHERE guid=$GUID;" > "$OUT/06b-talents-before-fire.txt"
WIN3=$(utc)
$PRESS spec "$MASTER" "$BOT" "fire pve"
sleep 12
$PRESS send ".saveall"
sleep 5
$PRESS sql "SELECT GROUP_CONCAT(spell ORDER BY spell) FROM acore_characters.character_talent WHERE guid=$GUID;" > "$OUT/06b-talents-after-fire.txt"
if diff -q "$OUT/06b-talents-before-fire.txt" "$OUT/06b-talents-after-fire.txt" >/dev/null; then
    echo "IDENTICAL -- 'fire pve' moved nothing"
else
    echo "DIFFERENT -- 'fire pve' moved the build again"
fi
head -c 400 "$OUT/06b-talents-after-fire.txt"; echo

sect "and the readback for THAT one"
$PRESS whisper "$MASTER" "$BOT" nc +debug
sleep 6
WIN4=$(utc)
$PRESS whisper "$MASTER" "$BOT" talents
sleep 10
docker logs ac-worldserver --since "$WIN4" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "done"
} 2>&1 | tee "$OUT/06b-readback.log"
say "step 06b done -- the bot's spec has been read back from the module."
