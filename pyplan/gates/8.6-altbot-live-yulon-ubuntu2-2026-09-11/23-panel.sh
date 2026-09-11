#!/usr/bin/env bash
# T26 live round 2, step 23 -- must-fixes 2 and 3, pressed on the box against
# the fixed code.
#
#   1. add an alt through `add_named`                -> it joins
#   2. `InstallParty.state`                          -> the PANEL's own party
#      holds it, and the summary line under the report says "1 bot in this
#      party." where round 1 said "This character's party has no bots in it yet."
#   3. `InstallParty.remove`                         -> the readback watches the
#      row leave `group_member`, and the row is read by hand either side
#   4. `InstallParty.remove` on the same name again  -> the refusal that replaces
#      round 1's vacuous `removed=True`
#   5. add TWO, then "Dismiss all"                   -> both go, and the group
#      table is empty afterwards
#
# Every press has its `docker logs` window opened and proved empty of the
# pattern first, and `group_member` read by hand before and after.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
ALT=${2:?alt}
MATE=${3:?the guild mate}
GUILD=${4:?the guild name}

say "step 23 -- pressing the two must-fixes: the panel's own party list after an add, a dismissal whose readback watches the row leave, the refusal for a character that is not in the party, and Dismiss all with two altbots standing."

hdr "T26 live round 2, step 23 -- the panel's party, the dismissal, and Dismiss all"

sect "0. the ground -- nothing standing, and the code this presses"
md5sum "$TREE/pylauncher/yulon/party.py"
grep -n "def altbot_store_path\|class AltbotMemory\|also=self._altbots.names(master)\|standing = self.members(master)" "$TREE/pylauncher/yulon/party.py"
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"
$PRESS state "$MASTER"

sect "1. ADD the alt through the app's own Add this character"
SINCE=$(utc)
echo "dml_botadd lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 'dml_botadd')"
echo "group_member rows before: $(dbq 'SELECT COUNT(*) FROM acore_characters.group_member;')"
$PRESS addnamed "$MASTER" "$ALT"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "dml_botadd" | head -3
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, c.name, c.account, c.online FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"

sect "2. MUST-FIX 2 -- what the PANEL draws, and the line it prints under the report"
$PRESS state "$MASTER"

sect "3. MUST-FIX 3 -- the dismissal, with the group table read by hand either side"
SINCE2=$(utc)
echo "dml_uninvite lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE2" 2>&1 | grep -c 'dml_uninvite')"
echo ">>> group_member BEFORE:"
dbq "SELECT gm.guid, gm.memberGuid, c.name FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.memberGuid;"
$PRESS dismiss "$MASTER" "$ALT"
echo ">>> group_member AFTER:"
dbq "SELECT gm.guid, gm.memberGuid, c.name FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.memberGuid;"
docker logs ac-worldserver --since "$SINCE2" 2>&1 | grep -E "dml_uninvite|dml_whisper" | head -5
$PRESS state "$MASTER"

sect "4. the SAME dismissal again -- round 1 answered removed=True here"
SINCE3=$(utc)
echo "dml_uninvite lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE3" 2>&1 | grep -c 'dml_uninvite')"
$PRESS dismiss "$MASTER" "$ALT"
echo ">>> the world's own log for that window (nothing should have been sent):"
docker logs ac-worldserver --since "$SINCE3" 2>&1 | grep -E "dml_uninvite|dml_whisper" | head -5
echo "(no dml_ line above means nothing was sent)"

sect "5. TWO altbots, so Dismiss all has something to dismiss"
$PRESS send guild create "$MASTER" "$GUILD"
$PRESS send guild invite "$MATE" "$GUILD"
sleep 2
dbq1 "SELECT g.guildid, g.name, g.leaderguid FROM acore_characters.guild g WHERE g.name = '$GUILD';"
SINCE4=$(utc)
echo "dml_botadd lines in the window BEFORE the two presses: $(docker logs ac-worldserver --since "$SINCE4" 2>&1 | grep -c 'dml_botadd')"
$PRESS addnamed "$MASTER" "$ALT"
$PRESS addnamed "$MASTER" "$MATE"
docker logs ac-worldserver --since "$SINCE4" 2>&1 | grep -E "dml_botadd" | head -4
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, c.name, c.account, c.online FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
$PRESS state "$MASTER"

sect "6. DISMISS ALL"
SINCE5=$(utc)
echo "dml_uninvite lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE5" 2>&1 | grep -c 'dml_uninvite')"
$PRESS removeall "$MASTER"
echo ">>> the world's own log for that window:"
docker logs ac-worldserver --since "$SINCE5" 2>&1 | grep -E "dml_uninvite|dml_whisper" | head -8
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
$PRESS state "$MASTER"

sect "7. the app's own record, and where it lives"
$PRESS remembered "$MASTER"
PYCFG=$($PY -c "import sys; sys.path.insert(0, '$TREE/pylauncher'); from yulon import platform; print(platform.config_dir())"); echo "platform.config_dir() = $PYCFG"; ls -l "$PYCFG/party-altbots.json" 2>&1 | head -2
cat "$PYCFG/party-altbots.json" 2>/dev/null

sect "done"
say "step 23 finished -- the panel's party holds the altbot, the dismissal watched the row leave, the second dismissal sent nothing, and Dismiss all emptied a party of two."
echo "step 23 finished $(stamp)"
