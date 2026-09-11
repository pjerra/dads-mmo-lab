#!/usr/bin/env bash
# T26 live half, step 04 -- the picker as the box stands, then CLAIM 1: an alt on
# the master's own account added as a bot and joining. Then two more sentences
# from the same character, the second of which is CLAIM 4.
#
# The order is chosen so that the ONLY thing that differs between the press that
# works and the press that is refused is the state being tested:
#
#   1. the picker, with nothing linked and no guild anywhere
#   2. add_named(master, alt)                 -> joins                  CLAIM 1
#   3. add_named(master, alt) again           -> "already in the party"
#   4. dml_uninvite master alt  (STAGING, not a claim: the bridge's own
#      uninvite takes the bot out of the group and LEAVES IT LOGGED IN, which
#      is why `InstallParty.remove` sends a logout whisper after it -- T13)
#   5. add_named(master, alt) again           -> "is logged in"         CLAIM 4
#
# So claim 4's character is the same character rule 1 admitted a minute earlier,
# on the same account, with the same guid. The refusal cannot be a permission
# rule wearing another sentence: it is `PlayerbotMgr.cpp:685-686`'s
# short-circuit, which `dml_botadd.lua` checks for itself with
# `GetPlayerByName(cname)` because the module's own words go to the master's
# game window and never to this app.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
ALT=${2:?alt}

say "step 04 -- the picker as the box stands, then adding $MASTER's own alt $ALT as a bot, then pressing the same name again twice: once while it is in the party and once while it is only in the world."

hdr "T26 live step 04 -- the picker, the alt, and the online refusal"

sect "who is in the world -- the master is a REAL session, which is what this route needs"
$PRESS send server info
dbq1 "SELECT guid, name, account, level, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
$PRESS ground "$MASTER"

sect "THE PICKER, before anything is linked and before any guild exists"
$PRESS candidates "$MASTER" Tsix

sect "CLAIM 1 -- InstallParty.add_named($MASTER, $ALT)"
SINCE=$(utc)
echo "window opens at $SINCE"
echo "dml_botadd lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 'dml_botadd')"
echo "group_member rows before: $(dbq 'SELECT COUNT(*) FROM acore_characters.group_member;')"
echo "$ALT online before: $(dbq "SELECT online FROM acore_characters.characters WHERE name = '$ALT';")"
$PRESS addnamed "$MASTER" "$ALT"
echo ">>> the world's own log for that window:"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "dml_botadd|playerbot" | head -20

sect "what the party holds now, and what the ALT is now"
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, c.name, c.account, c.online FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
dbq1 "SELECT guid, name, account, level, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
$PRESS members "$MASTER"

sect "the same press again while it is IN the party -- the app stops before sending"
$PRESS addnamed "$MASTER" "$ALT"

sect "STAGING (not a claim): the bridge's own uninvite, which leaves the bot logged in"
SINCE3=$(utc)
$PRESS send dml_uninvite "$MASTER" "$ALT"
sleep 3
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"
dbq1 "SELECT guid, name, online FROM acore_characters.characters WHERE name = '$ALT';"
docker logs ac-worldserver --since "$SINCE3" 2>&1 | grep -E "dml_uninvite" | head -5

sect "CLAIM 4 -- the SAME name pressed again, now that it is in the world and out of the party"
SINCE4=$(utc)
echo "window opens at $SINCE4"
echo "dml_botadd lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE4" 2>&1 | grep -c 'dml_botadd')"
$PRESS addnamed "$MASTER" "$ALT"
echo ">>> the world's own log for that window:"
docker logs ac-worldserver --since "$SINCE4" 2>&1 | grep -E "dml_botadd" | head -10
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"

sect "and the picker greys it for the same reason"
$PRESS candidates "$MASTER" Tsix

sect "done"
say "step 04 finished -- the alt joined, and the same name pressed again while it is in the world was refused in the bridge's own words."
echo "step 04 finished $(stamp)"
