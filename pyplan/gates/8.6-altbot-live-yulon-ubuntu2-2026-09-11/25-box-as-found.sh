#!/usr/bin/env bash
# T26 live round 2, step 25 -- the box put back, and every number step 01 read,
# read back a second time.
#
# Round 2 re-made what round 1 had deleted, so the ids are not the ones step 09
# read: accounts 119-121 and characters 1006-1009. The guild, the three accounts
# and their four characters go the same way they went in round 1 -- the core's
# own `.guild delete` and `.account delete` -- and this lane's one new file, the
# altbot record under `platform.config_dir()`, goes with them: it did not exist
# on this box before 03:20 today.
set -u
. "$(dirname "$0")/lib.sh"

say "step 25 -- putting the box back: the guild deleted, the three throwaway accounts and their four characters deleted, this lane's altbot record removed, and every number step 01 read read back."

hdr "T26 live round 2, step 25 -- the box as found"

sect "the client is out of the world first"
$PRESS send server info
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"

sect "the guild this lane made, deleted"
dbq1 "SELECT guildid, name FROM acore_characters.guild WHERE name = 'T26Kinfolk';"
$PRESS send guild delete T26Kinfolk
sleep 2
dbq1 "SELECT COUNT(*) FROM acore_characters.guild;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member;"

sect "the three throwaway accounts deleted, with their characters"
for a in T26MASTER T26MATE T26FRIEND; do
    $PRESS send account delete "$a"
done
dbq1 "SELECT id, username FROM acore_auth.account WHERE username LIKE 'T26%';"
dbq1 "SELECT guid, name FROM acore_characters.characters WHERE name LIKE 'Tsix%';"

sect "no orphaned rows anywhere those characters could have left one"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member WHERE memberGuid BETWEEN 1006 AND 1009;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member WHERE guid BETWEEN 1006 AND 1009;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_keys;"
dbq1 "SELECT COUNT(*) FROM acore_auth.account_access WHERE id BETWEEN 119 AND 121;"
dbq1 "SELECT COUNT(*) FROM acore_auth.realmcharacters WHERE acctid BETWEEN 119 AND 121;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters WHERE account BETWEEN 119 AND 121;"

sect "this lane's one new file on the box, and it goes too"
PYCFG=$($PY -c "import sys; sys.path.insert(0, '$TREE/pylauncher'); from yulon import platform; print(platform.config_dir())")
echo "platform.config_dir() = $PYCFG"
ls -l "$PYCFG/party-altbots.json" 2>&1 | head -2
cat "$PYCFG/party-altbots.json" 2>/dev/null
rm -f "$PYCFG/party-altbots.json"
echo "after removing it:"
ls -l "$PYCFG/party-altbots.json" 2>&1 | head -2
echo "what is left in that directory:"
ls -l "$PYCFG" | head -20

sect "EVERY number step 01 read, read back"
docker ps --format "{{.Names}}\t{{.Status}}"
for c in ac-worldserver ac-authserver ac-database; do
    echo "$c: $(docker inspect $c --format 'started={{.State.StartedAt}} restarts={{.RestartCount}} status={{.State.Status}}')"
done
dbq1 "SELECT COUNT(*) FROM acore_auth.account;"
dbq1 "SELECT COUNT(*) FROM acore_auth.account_access;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters WHERE online = 1;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member;"
dbq1 "SELECT COUNT(*) FROM acore_characters.groups;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"
dbq1 "SELECT account_type, COUNT(*) FROM acore_playerbots.playerbots_account_type GROUP BY account_type;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_random_bots;"

sect "the owner's things"
ls -l --time-style=long-iso "$LUA/LootPet2.lua"
md5sum "$LUA/LootPet2.lua"
grep -n "^Logger.ALE" "$HOME/wowserver/env/dist/etc/worldserver.conf"
dbq1 "SELECT address, localAddress, localSubnetMask FROM acore_auth.realmlist;"
dbq1 "SELECT id, username, last_login FROM acore_auth.account WHERE username IN ('PERZI','YULONADMIN','YULON_243C46E3') ORDER BY id;"
dbq1 "SELECT guid, account, name, level, online FROM acore_characters.characters WHERE name = 'Pakka';"
md5sum "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

sect "the bridge -- six scripts, the same bytes, and the world still answers"
ls -l --time-style=long-iso "$LUA"
md5sum "$LUA"/dml_*.lua
$PRESS send dml_bridge_ping
$PRESS ground

sect "the module clone, still pinned and unmodified"
git -C "$HOME/wowserver/modules/mod-playerbots" rev-parse HEAD
git -C "$HOME/wowserver/modules/mod-playerbots" status --porcelain | head

sect "done"
say "step 25 finished -- the box is back to what step 01 read, and this lane's altbot record is off it."
echo "step 25 finished $(stamp)"
