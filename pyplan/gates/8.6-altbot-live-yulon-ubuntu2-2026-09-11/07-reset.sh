#!/usr/bin/env bash
# T26 live half, step 07 -- the box put back to the state step 02 left it in, so
# the four claims can be pressed ONCE MORE against the fixed code and the second
# round's captures are a single clean pass rather than a patch on the first.
#
# Round 1 (02:04-02:09) is kept: `04-alt-round1.log`, `05-guild-round1.log` and
# `06-link-round1.log` are where the defect was found, and a folder that showed
# only the run that worked would hide that it took two.
#
# What this undoes, and only this:
#   * the three bots this lane added are dismissed through the APP's own route
#     (`InstallParty.remove` = `dml_uninvite` then the logout whisper, T13's
#     contract) where they are in the party, and by the logout whisper alone
#     where the bridge's own uninvite already took them out of it;
#   * the guild this lane made is deleted with the core's own `.guild delete`;
#   * the two rows this lane's link write put in `playerbots_account_links` are
#     deleted. There is no app route that unlinks -- the panel writes the link
#     and the module reads it; removing it is this lane's own cleanup and it is
#     a DELETE of exactly the two pairs the write's readback named.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}

say "step 07 -- putting the box back to where step 02 left it: the three bots dismissed through the app, the guild deleted, the two link rows deleted. The four claims will then be pressed once more against the fixed code."

hdr "T26 live step 07 -- back to the ground, between the two rounds"

sect "what is standing before the reset"
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, c.name, c.online FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"

sect "the bots in the party, dismissed through InstallParty.remove"
for bot in Tsixmate Tsixfriend; do
    $PRESS dismiss "$MASTER" "$bot"
done

sect "the one the bridge's own uninvite already took out of the party, logged out by the app's own whisper"
$PRESS send dml_whisper "$MASTER" Tsixalt logout
sleep 5

sect "the guild deleted"
$PRESS send guild delete T26Kinfolk
sleep 2
dbq1 "SELECT COUNT(*) FROM acore_characters.guild;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member;"

sect "the two link rows deleted"
dbq1 "SELECT account_id, linked_account_id FROM acore_playerbots.playerbots_account_links ORDER BY account_id;"
dbq "DELETE FROM acore_playerbots.playerbots_account_links WHERE account_id IN (116,118) AND linked_account_id IN (116,118);"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"

sect "the ground again -- this should read exactly as step 02 left it"
sleep 10
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters WHERE online = 1;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member WHERE guid IN (SELECT guid FROM acore_characters.characters WHERE name LIKE 'Tsix%');"
$PRESS send server info

sect "and the code the second round will press"
md5sum "$TREE/pylauncher/yulon/party.py"
grep -n "def party_rows_sql\|def party_members\|members=lambda: self.party_members" "$TREE/pylauncher/yulon/party.py"

sect "done"
say "step 07 finished -- the box is back where step 02 left it and the second round can press the same four claims."
echo "step 07 finished $(stamp)"
