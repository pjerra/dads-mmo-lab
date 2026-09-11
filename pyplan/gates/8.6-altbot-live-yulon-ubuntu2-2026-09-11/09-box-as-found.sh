#!/usr/bin/env bash
# T26 live half, step 09 -- the three throwaway accounts and their four
# characters removed with the server's own `.account delete`
# (`cs_account.cpp:93`, `Console::Yes`), and then EVERY number step 01 read,
# read back.
#
# The client is closed before this runs: `.account delete` on an account whose
# character is in the world would kick it, and a box left tidy should not need
# to.
set -u
. "$(dirname "$0")/lib.sh"

say "step 09 -- deleting the three throwaway accounts and their four characters with the server's own .account delete, then reading every number step 01 read back."

hdr "T26 live step 09 -- the box as found"

sect "the client is out of the world first"
$PRESS send server info
dbq1 "SELECT guid, name, account, online FROM acore_characters.characters WHERE name LIKE 'Tsix%' ORDER BY guid;"

sect "the three throwaway accounts deleted, with their characters"
for a in T26MASTER T26MATE T26FRIEND; do
    $PRESS send account delete "$a"
done
dbq1 "SELECT id, username FROM acore_auth.account WHERE username LIKE 'T26%';"
dbq1 "SELECT guid, name FROM acore_characters.characters WHERE name LIKE 'Tsix%';"

sect "no orphaned rows anywhere those characters could have left one"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member WHERE memberGuid BETWEEN 1002 AND 1005;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member WHERE guid BETWEEN 1002 AND 1005;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_keys;"
dbq1 "SELECT COUNT(*) FROM acore_auth.account_access WHERE id BETWEEN 116 AND 118;"
dbq1 "SELECT COUNT(*) FROM acore_auth.realmcharacters WHERE acctid BETWEEN 116 AND 118;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters WHERE account BETWEEN 116 AND 118;"

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
say "step 09 finished -- the throwaway accounts and characters are gone and every number step 01 read has been read back."
echo "step 09 finished $(stamp)"
