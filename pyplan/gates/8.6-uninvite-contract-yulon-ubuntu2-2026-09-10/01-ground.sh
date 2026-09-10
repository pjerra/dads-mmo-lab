#!/usr/bin/env bash
# Step 01 -- what the box looked like BEFORE this ticket touched it.
source "$(dirname "$0")/lib.sh"
say "step 01, reading the box's state before anything is changed (containers, realm row, deployed scripts, group table)."
{
hdr "T13 live, step 01: ground state, before the redeploy"

sect "containers"
docker ps --format '{{.Names}}\t{{.Status}}\t{{.Image}}'

sect "the world's own liveness, through the app's docker reader"
$PRESS ground 2>&1 | head -1

sect "the scripts DEPLOYED right now (md5 + size), and the five this ticket ships"
md5sum "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"*.lua
echo "-- the ticket's own copies --"
md5sum "$TREE/pylauncher/lua/party/"*.lua
echo "-- the deployed dml_uninvite.lua, whole (this is the OLD one-argument script) --"
cat -n "$HOME/wowserver/env/dist/etc/modules/lua_scripts/dml_uninvite.lua"

sect "the realm row, before"
$PRESS sql "SELECT id,name,address,localAddress,localSubnetMask,port,flag FROM acore_auth.realmlist;"

sect "authserver, before"
docker inspect -f '{{.Name}} {{.State.Status}} started={{.State.StartedAt}} restarts={{.RestartCount}}' ac-authserver

sect "the owner's things, before (READ ONLY -- never used by this lane)"
$PRESS sql "SELECT id,username,last_login FROM acore_auth.account WHERE username IN ('PERZI','YULON_243C46E3','YULONADMIN') ORDER BY id;"
$PRESS sql "SELECT guid,name,level,online FROM acore_characters.characters WHERE name='Pakka';"
ls -l "$HOME/wowserver/env/dist/etc/modules/lua_scripts/LootPet2.lua"
grep -n '^Logger.ALE' "$HOME/wowserver/env/dist/etc/worldserver.conf"

sect "the group table, whole server, before"
$PRESS grouprows
echo "(no output above means the group_member table is empty)"
$PRESS sql "SELECT COUNT(*) AS group_member_rows FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) AS online_characters FROM acore_characters.characters WHERE online=1;"

sect "done"
} 2>&1 | tee "$OUT/01-ground.log"
say "step 01 done."
