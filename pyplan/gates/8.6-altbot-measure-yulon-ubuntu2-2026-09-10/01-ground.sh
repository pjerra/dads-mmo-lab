#!/usr/bin/env bash
# Step 01 -- the box before anything is done to it. Read-only.
source "$HOME/t26-out/lib.sh"
say "step 01, reading the box before touching anything: containers, account/character/guild/group counts, the deployed bridge scripts, and the SOAP seam's liveness."
{
hdr "T26 measure, step 01: ground"

sect "containers (nothing here restarts any of them)"
docker ps --format '{{.Names}}\t{{.Status}}'
docker inspect ac-worldserver --format 'worldserver started={{.State.StartedAt}} restarts={{.RestartCount}}'
docker inspect ac-authserver --format 'authserver  started={{.State.StartedAt}} restarts={{.RestartCount}}'
docker inspect ac-database   --format 'database    started={{.State.StartedAt}} restarts={{.RestartCount}}'

sect "the four counts this lane must leave unchanged"
echo "accounts    = $(q 'SELECT COUNT(*) FROM acore_auth.account;')"
echo "characters  = $(q 'SELECT COUNT(*) FROM acore_characters.characters;')"
echo "guilds      = $(q 'SELECT COUNT(*) FROM acore_characters.guild;')"
echo "guild_member= $(q 'SELECT COUNT(*) FROM acore_characters.guild_member;')"
echo "group_member= $(q 'SELECT COUNT(*) FROM acore_characters.group_member;')"
echo "groups      = $(q 'SELECT COUNT(*) FROM acore_characters.groups;')"
echo "online      = $(q 'SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;')"

sect "the non-bot accounts (id, username, gmlevel) -- PERZI and Pakka are read only to prove they were left alone"
qt "SELECT a.id,a.username,a.last_login,IFNULL(aa.gmlevel,0) AS gmlevel FROM acore_auth.account a LEFT JOIN acore_auth.account_access aa ON aa.id=a.id WHERE a.username NOT LIKE 'RNDBOT%' ORDER BY a.id;"
qt "SELECT guid,name,account,level,online FROM acore_characters.characters WHERE name='Pakka';"

sect "the group table, raw (T13's own read)"
q "SELECT gm.guid, gm.memberGuid, c.name, gm.memberFlags, gm.subgroup, gm.roles FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid = gm.memberGuid ORDER BY gm.guid, gm.memberGuid;"
echo "(empty above means no group rows at all)"

sect "the characters T13's harness used"
qt "SELECT guid,name,account,race,class,level,online FROM acore_characters.characters WHERE name IN ('Jurnaar','Dalio','Nore','Zarraden','Grirmirn') ORDER BY guid;"

sect "the deployed bridge scripts (the app's five, from T13's run)"
ls -l "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"

sect "the module conf directory as shipped"
ls -l "$HOME/wowserver/env/dist/etc/modules/"

sect "the SOAP seam answers, and the bridge with it"
$PRESS who
$PRESS send dml_bridge_ping
$PRESS send "server info"

sect "done"
} 2>&1 | tee "$OUT/01-ground.log"
say "step 01 done."
