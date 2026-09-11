#!/usr/bin/env bash
# T26 live round 2, step 22 -- THE MEASUREMENT the cold review asked for first:
# does this server itself record, anywhere this app can read, that a character
# was brought into the world by `.playerbots bot add` rather than by a person?
#
# The question matters because `members()` has to count a T26-added character
# for as long as it stands in the master's party and must never count a real
# person who happens to be there. A discriminator the WORLD holds would be
# better than a record this app keeps, so it is looked for first and the answer
# is written down either way.
#
# Everything below is read BEFORE the add and read again AFTER it, so the answer
# is a diff and not an argument.
set -u
. "$(dirname "$0")/lib.sh"

MASTER=${1:?master}
ALT=${2:?alt}

say "step 22 -- measuring whether the server records a bot-added character anywhere readable: every playerbots table, the auth account rows, the character row and the module's own bot list, before and after one add."

hdr "T26 live round 2, step 22 -- is there a discriminator on the box?"

sect "1. every table in the playerbots database, and how many rows each holds"
dbq1 "SELECT table_name, table_rows FROM information_schema.tables WHERE table_schema = 'acore_playerbots' ORDER BY table_name;"
for t in $(dbq "SELECT table_name FROM information_schema.tables WHERE table_schema='acore_playerbots' ORDER BY table_name;"); do
    echo "-- acore_playerbots.$t: $(dbq "SELECT COUNT(*) FROM acore_playerbots.$t;") rows"
done

sect "2. the columns of the two tables that could plausibly carry an owner"
dbq1 "DESCRIBE acore_playerbots.playerbots_random_bots;" 2>&1 | head -20
dbq1 "SHOW TABLES FROM acore_playerbots;"

sect "3. what the MODULE writes on the add path -- its own source, on the box"
MOD="$HOME/wowserver/modules/mod-playerbots"
echo ">>> every SQL write in PlayerbotMgr.cpp:"
grep -nE "PlayerbotsDatabase\.(Execute|PExecute|Query)|CharacterDatabase\.(Execute|PExecute)|LoginDatabase\.(Execute|PExecute)" "$MOD/src/Bot/PlayerbotMgr.cpp" | head -20
echo
echo ">>> AddPlayerBot, the whole of it, to see what it persists:"
sed -n '95,140p' "$MOD/src/Bot/PlayerbotMgr.cpp"
echo
echo ">>> and OnBotLogin's first lines:"
sed -n '460,475p' "$MOD/src/Bot/PlayerbotMgr.cpp"

sect "4. BEFORE the add -- the rows that could move"
dbq1 "SELECT * FROM acore_playerbots.playerbots_account_links;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_random_bots;"
dbq1 "SELECT id, username, online, last_ip, last_login FROM acore_auth.account WHERE username LIKE 'T26%' ORDER BY id;"
dbq1 "SELECT guid, account, name, online, logout_time, totaltime, at_login FROM acore_characters.characters WHERE name IN ('$MASTER','$ALT') ORDER BY guid;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"

sect "5. the module's own bot list, over the app's own channel"
$PRESS send playerbots bot list
$PRESS send playerbots bot
$PRESS send playerbots rndbot stats

sect "6. THE ADD"
SINCE=$(utc)
echo "window opens at $SINCE"
echo "dml_botadd lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -c 'dml_botadd')"
$PRESS addnamed "$MASTER" "$ALT"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "dml_botadd" | head -5
sleep 3

sect "7. AFTER the add -- the same rows, read again"
dbq1 "SELECT * FROM acore_playerbots.playerbots_account_links;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_random_bots;"
for t in $(dbq "SELECT table_name FROM information_schema.tables WHERE table_schema='acore_playerbots' ORDER BY table_name;"); do
    echo "-- acore_playerbots.$t: $(dbq "SELECT COUNT(*) FROM acore_playerbots.$t;") rows"
done
dbq1 "SELECT id, username, online, last_ip, last_login FROM acore_auth.account WHERE username LIKE 'T26%' ORDER BY id;"
dbq1 "SELECT guid, account, name, online, logout_time, totaltime, at_login FROM acore_characters.characters WHERE name IN ('$MASTER','$ALT') ORDER BY guid;"
dbq1 "SELECT gm.guid AS group_id, gm.memberGuid, gm.memberFlags, gm.subgroup, gm.roles, c.name, c.account FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
dbq1 "SELECT * FROM acore_characters.groups;"

sect "8. the module's own bot list again, now that there IS one"
$PRESS send playerbots bot list

sect "9. a random bot for comparison -- the marker's own case"
dbq1 "SELECT c.guid, c.name, c.account, a.username, c.online FROM acore_characters.characters c JOIN acore_auth.account a ON a.id = c.account WHERE c.online = 1 AND a.username LIKE 'RNDBOT%' LIMIT 3;"
dbq1 "SELECT id, username, online FROM acore_auth.account WHERE username LIKE 'RNDBOT%' ORDER BY id LIMIT 3;"

sect "done"
say "step 22 finished -- the before/after of every readable place the server could have recorded the add is captured."
echo "step 22 finished $(stamp)"
