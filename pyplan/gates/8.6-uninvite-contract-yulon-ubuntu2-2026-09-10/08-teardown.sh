#!/usr/bin/env bash
# Step 08 -- put the box back: disband the staged parties, take the staging
# harness off the server, restart the world without it, and read back that the
# owner's things are where they were.
source "$(dirname "$0")/lib.sh"

MASTER1=Jurnaar; BOT1=Dalio; MASTER2=Grirmirn; BOT2=Zarraden; BOT3=Nore

say "step 08, teardown: disbanding the staged parties, removing the staging harness, restarting the world without it."
{
hdr "T13 live, step 08: teardown"

sect "disbanding the two staged parties"
$PRESS grouprows | tail -n +2
$PRESS send "dml_t13_stage disband $MASTER1"
sleep 2
$PRESS send "dml_t13_stage disband $MASTER2"
sleep 2
$PRESS grouprows
echo "(no rows above = the group table is empty again)"
$PRESS sql "SELECT COUNT(*) AS group_member_rows FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) AS groups_rows FROM acore_characters.\`groups\`;"

sect "removing the staging harness from the server"
rm -f "$HOME/wowserver/env/dist/etc/modules/lua_scripts/t13_stage.lua"
ls -l "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"
echo "-- the five the app ships are still deployed, and are this ticket's bytes --"
md5sum "$HOME/wowserver/env/dist/etc/modules/lua_scripts/dml_"*.lua
md5sum "$TREE/pylauncher/lua/party/"*.lua

sect "restarting the world without the harness (the third and last restart of this run)"
SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE=$SINCE"
docker restart ac-worldserver
for i in $(seq 1 60); do
    sleep 10
    ans=$($PRESS send dml_bridge_ping 2>&1 | tail -1)
    echo "$(stamp) try $i: $ans"
    case "$ans" in *DML-BRIDGE-READY*) break;; esac
done
echo "-- what loads now: the five, and no t13_stage --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "^\[(dml_|t13_)" || true
echo "-- and the harness command is gone from the world --"
$PRESS send "dml_t13_stage show $MASTER1 $MASTER1"

sect "the bots this run touched are back in the world"
for i in $(seq 1 40); do
    n=$($PRESS sql "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;" | tail -1)
    echo "$(stamp) online=$n"
    [ "$n" = "500" ] && break
    sleep 10
done
$PRESS sql "SELECT name,guid,level,online FROM acore_characters.characters WHERE name IN ('$MASTER1','$BOT1','$MASTER2','$BOT2','$BOT3') ORDER BY guid;"
$PRESS sql "SELECT COUNT(*) AS group_member_rows FROM acore_characters.group_member;"

sect "containers, after"
docker ps --format '{{.Names}}\t{{.Status}}\t{{.Image}}'
docker inspect -f '{{.Name}} {{.State.Status}} started={{.State.StartedAt}} restarts={{.RestartCount}}' ac-authserver ac-worldserver ac-database

sect "the realm row, after (never edited by this lane, so no authserver restart was owed)"
$PRESS sql "SELECT id,name,address,localAddress,localSubnetMask,port,flag FROM acore_auth.realmlist;"

sect "the accounts, after -- no account was created, deleted or changed by this lane"
$PRESS sql "SELECT id,username,last_login FROM acore_auth.account ORDER BY id DESC LIMIT 5;"
$PRESS sql "SELECT COUNT(*) AS accounts FROM acore_auth.account;"
$PRESS sql "SELECT id,gmlevel,RealmID FROM acore_auth.account_access ORDER BY id;"

sect "the owner's things, after"
$PRESS sql "SELECT id,username,last_login FROM acore_auth.account WHERE username='PERZI';"
$PRESS sql "SELECT guid,name,level,online FROM acore_characters.characters WHERE name='Pakka';"
ls -l "$HOME/wowserver/env/dist/etc/modules/lua_scripts/LootPet2.lua"
grep -n '^Logger.ALE' "$HOME/wowserver/env/dist/etc/worldserver.conf"
$PRESS sql "SELECT COUNT(*) AS characters FROM acore_characters.characters;"

sect "done"
} 2>&1 | tee "$OUT/08-teardown.log"
say "step 08 done -- the box is back as it was, with the ticket's five scripts deployed."
