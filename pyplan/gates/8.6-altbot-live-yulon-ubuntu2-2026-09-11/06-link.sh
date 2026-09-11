#!/usr/bin/env bash
# T26 live half, step 06 -- CLAIM 3: a character on a LINKED account added as a
# bot, with the link written by the panel's own "Link an account".
#
# Both presses, in the order the panel makes them: `link_plan` first, which
# writes nothing and names both accounts and the consequence, then
# `link_account`, which runs `link_transaction_sql` -- one session of
# START TRANSACTION / the two-direction INSERT IGNORE / SELECT 'wrote',
# ROW_COUNT() / SELECT both directional rows / COMMIT -- through the
# `SqlWriter` the factory wires at `controller_view.py:1167`.
#
# The table is read before and after by hand, so the two rows this app claims to
# have written are read back by something that is not this app.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
FRIEND=${2:?the friend character}
ACCOUNT=${3:?the friend account}

say "step 06 -- linking $MASTER's account to $ACCOUNT through the panel's own Link an account, then adding $FRIEND as a bot."

hdr "T26 live step 06 -- the linked account"

sect "the link table before -- empty, as step 01 read it"
dbq1 "SELECT * FROM acore_playerbots.playerbots_account_links;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"
dbq1 "SELECT id, username FROM acore_auth.account WHERE username LIKE 'T26%' ORDER BY id;"

sect "the FIRST press -- InstallParty.link_plan, which writes nothing"
$PRESS linkplan "$MASTER" "$ACCOUNT"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"

sect "the refusals this control makes, pressed rather than described"
$PRESS linkaccount "$MASTER" T26MASTER
$PRESS linkaccount "$MASTER" NOSUCHACCOUNT

sect "the SECOND press -- InstallParty.link_account, which writes"
$PRESS linkaccount "$MASTER" "$ACCOUNT"
dbq1 "SELECT account_id, linked_account_id FROM acore_playerbots.playerbots_account_links ORDER BY account_id;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"

sect "and pressing it again, now that both rows are there"
$PRESS linkaccount "$MASTER" "$ACCOUNT"

sect "THE PICKER now -- the row that changed is the friend's"
$PRESS candidates "$MASTER" Tsix

sect "CLAIM 3 -- InstallParty.add_named($MASTER, $FRIEND)"
SINCE=$(utc)
echo "window opens at $SINCE"
echo "dml_botadd lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 'dml_botadd')"
echo "group_member rows before: $(dbq 'SELECT COUNT(*) FROM acore_characters.group_member;')"
echo "$FRIEND online before: $(dbq "SELECT online FROM acore_characters.characters WHERE name = '$FRIEND';")"
$PRESS addnamed "$MASTER" "$FRIEND"
echo ">>> the world's own log for that window:"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "dml_botadd" | head -10

sect "the group table RAW, and what every one of this lane's characters is now"
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, c.name, c.account, c.online FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
$PRESS members "$MASTER"

sect "done"
say "step 06 finished."
echo "step 06 finished $(stamp)"
