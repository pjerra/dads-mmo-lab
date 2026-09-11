#!/usr/bin/env bash
# T26 live half, step 05 -- CLAIM 2: a guild mate on ANOTHER account added as a
# bot.
#
# The guild is made with the CORE's own two commands over the app's own channel:
# `.guild create <master> <name>` (`cs_guild.cpp:35`, `Console::Yes`, and the
# target must be CONNECTED -- `HandleGuildCreateCommand` refuses an offline one)
# and `.guild invite <character> <name>` (`:37`, which takes a `PlayerIdentifier`
# and calls `Guild::AddMember(guid)` with no connected check, so an OFFLINE
# character can be put in a guild). Neither is part of Yu'lon: they are this
# lane's staging, exactly as T16 staged a party it could not press.
#
# The picker is read BEFORE and AFTER the guild exists, so the row that changes
# is the claim: `Tsixmate` moves from "not on this character's account, not in
# its guild, not an addclass bot, and not on a linked account" to "in this
# character's guild".
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
MATE=${2:?the guild mate}
GUILD=${3:?the guild name}

say "step 05 -- making the guild $GUILD with $MASTER as its leader, putting $MATE (another account, offline) in it, and then adding $MATE as a bot through the app's own Add this character."

hdr "T26 live step 05 -- the guild mate"

sect "the guilds before -- and that neither character is in one"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member;"
dbq1 "SELECT guildid FROM acore_characters.guild_member WHERE guid IN (SELECT guid FROM acore_characters.characters WHERE name IN ('$MASTER','$MATE'));"
dbq1 "SELECT guildid, name FROM acore_characters.guild WHERE name = '$GUILD';"

sect "the guild made, with the master as its leader"
SINCE=$(utc)
$PRESS send guild create "$MASTER" "$GUILD"
$PRESS send guild invite "$MATE" "$GUILD"
sleep 2
dbq1 "SELECT g.guildid, g.name, g.leaderguid FROM acore_characters.guild g WHERE g.name = '$GUILD';"
dbq1 "SELECT gm.guildid, gm.guid, c.name, c.account, c.online FROM acore_characters.guild_member gm JOIN acore_characters.characters c ON c.guid = gm.guid WHERE gm.guildid = (SELECT guildid FROM acore_characters.guild WHERE name = '$GUILD') ORDER BY gm.guid;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member;"

sect "THE PICKER now -- the row that changed is the guild mate's"
$PRESS candidates "$MASTER" Tsix

sect "CLAIM 2 -- InstallParty.add_named($MASTER, $MATE)"
SINCE2=$(utc)
echo "window opens at $SINCE2"
echo "dml_botadd lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE2" 2>&1 | grep -c 'dml_botadd')"
echo "group_member rows before: $(dbq 'SELECT COUNT(*) FROM acore_characters.group_member;')"
echo "$MATE online before: $(dbq "SELECT online FROM acore_characters.characters WHERE name = '$MATE';")"
$PRESS addnamed "$MASTER" "$MATE"
echo ">>> the world's own log for that window:"
docker logs ac-worldserver --since "$SINCE2" 2>&1 | grep -E "dml_botadd" | head -10

sect "the group table RAW -- group_member.guid is the GROUP, memberGuid is the member"
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, c.name, c.account, c.online FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
$PRESS members "$MASTER"

sect "done"
say "step 05 finished."
echo "step 05 finished $(stamp)"
