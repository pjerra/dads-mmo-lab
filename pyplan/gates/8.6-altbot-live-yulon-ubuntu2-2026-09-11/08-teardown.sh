#!/usr/bin/env bash
# T26 live half, step 08 -- the bots handed back and the guild and the link
# undone, while the master is still logged in (the app's own dismissal needs a
# live master, the same way the add does).
#
# The three bots go out through `InstallParty.remove`, which is T13's route:
# `dml_uninvite` and then the logout whisper, with the group table read back.
# The one that is only in the world gets the whisper alone.
#
# The guild is deleted with the core's own `.guild delete`. The two link rows
# are DELETEd: there is no unlink control in the app -- the panel writes the
# link, and taking it away again is this lane's cleanup and not a feature.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}

say "step 08 -- dismissing the three bots through the app's own route, deleting the guild this lane made and the two link rows it wrote. The throwaway accounts and characters go in step 09, after the client is closed."

hdr "T26 live step 08 -- the bots, the guild and the link undone"

sect "what is standing before the teardown"
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, c.name, c.account, c.online FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
$PRESS members "$MASTER"

sect "the two in the party, through InstallParty.remove"
for bot in Tsixmate Tsixfriend; do
    $PRESS dismiss "$MASTER" "$bot"
done

sect "the one that is only in the world, through the app's own logout whisper"
$PRESS send dml_whisper "$MASTER" Tsixalt logout
sleep 8

sect "the guild deleted"
$PRESS send guild delete T26Kinfolk
sleep 2

sect "the two link rows deleted"
dbq1 "SELECT account_id, linked_account_id FROM acore_playerbots.playerbots_account_links ORDER BY account_id;"
dbq "DELETE FROM acore_playerbots.playerbots_account_links WHERE account_id IN (116,118) AND linked_account_id IN (116,118);"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"

sect "what is left -- only the master, and only because the client is still in it"
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"
dbq1 "SELECT COUNT(*) FROM acore_characters.groups;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters WHERE online = 1;"
$PRESS send server info

sect "done"
say "step 08 finished -- the bots are out, the guild is gone and the link table is empty again."
echo "step 08 finished $(stamp)"
