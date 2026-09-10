#!/usr/bin/env bash
# Step 06 -- one chosen spec, pressed on a bot, read back from the bot.
#
# The command string is built by `party.spec_command(master, bot, name)` -- the
# app's own builder, the one the panel's Add sends -- and goes over the seam's
# own channel. Nothing here types `dml_whisper ... talents spec` by hand.
#
# Three readbacks, because one of them alone proves less than it looks:
#   * the module's own reply, said out loud and caught in the world's log;
#   * `talents` afterwards, which is the module reporting its own current tab;
#   * `character_talent` in the database, flushed with the server's own
#     `.saveall` (T5 measured that an online character's row lags until a save).
source "$(dirname "$0")/lib.sh"

MASTER=Jurnaar
BOT=Lacosh
GUID=188
SPEC="frost pve"
BADSPEC="warglaive pvp"

say "step 06, pressing one chosen spec on a bot through the app's own command builder, and reading the spec back from the bot."
{
hdr "T18 live, step 06: the spec press and its readback"

sect "the picker offers this name for this class, and the app would refuse the other"
$PRESS specs mage
echo "chosen  : $SPEC"
echo "control : $BADSPEC  (not in the list above)"

sect "what the module says the bot's spec is BEFORE"
WIN0=$(utc)
echo "WIN0=$WIN0"
$PRESS whisper "$MASTER" "$BOT" talents
sleep 8
docker logs ac-worldserver --since "$WIN0" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "and what the database holds for $BOT BEFORE (flushed with the server's own save)"
$PRESS send ".saveall"
sleep 5
$PRESS sql "SELECT COUNT(*) AS talent_rows FROM acore_characters.character_talent WHERE guid=$GUID;"
$PRESS sql "SELECT GROUP_CONCAT(spell ORDER BY spell) FROM acore_characters.character_talent WHERE guid=$GUID;" > "$OUT/06-talents-before.txt"
wc -c "$OUT/06-talents-before.txt"
head -c 600 "$OUT/06-talents-before.txt"; echo

sect "THE PRESS -- party.spec_command, over the seam's own channel"
WIN=$(utc)
echo "WIN=$WIN"
echo "-- the window before the press, which must hold no 'Picking' line --"
docker logs ac-worldserver --since "$WIN" 2>&1 | grep -c "Picking" || true
$PRESS spec "$MASTER" "$BOT" "$SPEC"
sleep 12
echo "-- the world's own log for that press --"
docker logs ac-worldserver --since "$WIN" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "the readback -- what the module says the bot's spec is NOW"
WIN2=$(utc)
echo "WIN2=$WIN2"
$PRESS whisper "$MASTER" "$BOT" talents
sleep 10
docker logs ac-worldserver --since "$WIN2" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"

sect "and what the database holds now"
$PRESS send ".saveall"
sleep 6
$PRESS sql "SELECT COUNT(*) AS talent_rows FROM acore_characters.character_talent WHERE guid=$GUID;"
$PRESS sql "SELECT GROUP_CONCAT(spell ORDER BY spell) FROM acore_characters.character_talent WHERE guid=$GUID;" > "$OUT/06-talents-after.txt"
wc -c "$OUT/06-talents-after.txt"
head -c 600 "$OUT/06-talents-after.txt"; echo
echo "-- did the build change? --"
if diff -q "$OUT/06-talents-before.txt" "$OUT/06-talents-after.txt" >/dev/null; then
    echo "IDENTICAL -- the talent rows did not change"
else
    echo "DIFFERENT -- the talent rows changed"
fi
md5sum "$OUT/06-talents-before.txt" "$OUT/06-talents-after.txt"

sect "the control -- a name the picker does NOT offer, sent by hand"
WIN3=$(utc)
echo "WIN3=$WIN3"
$PRESS whisper "$MASTER" "$BOT" talents spec "$BADSPEC"
sleep 10
docker logs ac-worldserver --since "$WIN3" 2>&1 | grep -E "t18_chat" || echo "(no chat lines)"
echo "(the app would never send this: party._spec_refusal refuses a name that is"
echo " not in seam.specs(klass) before anything reaches the wire)"

sect "the app's own refusal for that name, from the app's own function"
$PY - <<PYEOF
import sys
from pathlib import Path
sys.path.insert(0, str(Path.home() / "t18-live" / "pylauncher"))
from yulon import party
sys.path.insert(0, str(Path("$GATE")))
import t18press
seam, _a, _s = t18press.build()
specs = seam.specs("mage")
print("seam.specs('mage') =", specs)
print("refusal for $BADSPEC:")
print("   ", party._spec_refusal("$BADSPEC", "mage", specs))
print("refusal for $SPEC:")
print("   ", repr(party._spec_refusal("$SPEC", "mage", specs)))
PYEOF

sect "done"
} 2>&1 | tee "$OUT/06-spec-press.log"
say "step 06 done -- a chosen spec was pressed on a bot and read back from it."
