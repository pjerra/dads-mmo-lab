#!/usr/bin/env bash
# Step 03 -- two parties to test against, and the record of why they had to be
# built by a harness rather than by the app or by the server's own commands.
#
#   MASTER1 = Jurnaar   (RNDBOT0, guid 1)    the master who ASKS
#   BOT1    = Dalio     (RNDBOT0, guid 4)    joins MASTER1's party (honest case)
#   MASTER2 = Grirmirn  (RNDBOT1, guid 11)   the SECOND master
#   BOT2    = Zarraden  (RNDBOT1, guid 12)   sits in MASTER2's party (negative case)
source "$(dirname "$0")/lib.sh"

MASTER1=Jurnaar; BOT1=Dalio; MASTER2=Grirmirn; BOT2=Zarraden

say "step 03, staging two parties. First recording that the server offers a SOAP caller no way to make one, then loading a staging harness and restarting the world."
{
hdr "T13 live, step 03: why a harness, then the two parties"

sect "route 1: the core's own group commands, as this SOAP caller sees them"
$PRESS send ".help group"
echo "-- and what happens when the filtered-out subcommand is called anyway --"
$PRESS send ".group join $MASTER1 $BOT1"
echo "-- the account this channel holds is gm level 3, so this is not a rights problem --"
$PRESS sql "SELECT id,gmlevel,RealmID FROM acore_auth.account_access ORDER BY id;"

sect "route 2: the app's own dml_addclass, with a bot as the master"
$PRESS sql "SELECT COUNT(*) AS characters_before FROM acore_characters.characters;"
$PRESS send "dml_addclass $MASTER1 mage"
sleep 10
$PRESS sql "SELECT COUNT(*) AS characters_after FROM acore_characters.characters;"
echo "-- the world log for that command --"
docker logs ac-worldserver --since "$(date -u -d '40 seconds ago' +%Y-%m-%dT%H:%M:%SZ)" 2>&1 | grep "dml_addclass" || true
echo "-- and the group table is still empty --"
$PRESS sql "SELECT COUNT(*) AS group_member_rows FROM acore_characters.group_member;"
echo "(addclass ran as $MASTER1 and made nothing: every online character on this"
echo " install is a random bot, and the module's addclass will not serve one.)"

sect "installing the staging harness beside the deployed bridge"
cp "$GATE/t13_stage.lua" "$HOME/wowserver/env/dist/etc/modules/lua_scripts/t13_stage.lua"
md5sum "$GATE/t13_stage.lua" "$HOME/wowserver/env/dist/etc/modules/lua_scripts/t13_stage.lua"
ls -l "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"

sect "restarting the world so the harness loads (the second restart of this run)"
SINCE=$(date -u +%Y-%m-%dT%H:%M:%SZ)
echo "SINCE=$SINCE"
docker restart ac-worldserver
for i in $(seq 1 60); do
    sleep 10
    ans=$($PRESS send dml_bridge_ping 2>&1 | tail -1)
    echo "$(stamp) try $i: $ans"
    case "$ans" in *DML-BRIDGE-READY*) break;; esac
done
echo "-- what loaded this time --"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -E "^\[(dml_|t13_)" || true

sect "waiting for the bots to be back in the world"
for i in $(seq 1 40); do
    n=$($PRESS sql "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;" | tail -1)
    echo "$(stamp) online=$n"
    [ "$n" = "500" ] && break
    sleep 10
done
$PRESS sql "SELECT name,guid,level,online FROM acore_characters.characters WHERE name IN ('$MASTER1','$BOT1','$MASTER2','$BOT2') ORDER BY guid;"

sect "party A: $MASTER2 (the SECOND master) with $BOT2"
$PRESS send "dml_t13_stage group $MASTER2 $BOT2"
sleep 2
$PRESS send "dml_t13_stage show $MASTER2 $BOT2"

sect "party B: $MASTER1 (the master who will ask) with $BOT1"
$PRESS send "dml_t13_stage group $MASTER1 $BOT1"
sleep 2
$PRESS send "dml_t13_stage show $MASTER1 $BOT1"

sect "the group table now holds two parties and nothing else"
$PRESS grouprows
$PRESS group "$MASTER1"
$PRESS group "$MASTER2"

sect "and neither master's group counts the other master's bot"
$PRESS send "dml_t13_stage show $MASTER1 $BOT2"
$PRESS send "dml_t13_stage show $MASTER2 $BOT1"

sect "done"
} 2>&1 | tee "$OUT/03-stage.log"
say "step 03 done -- two parties are staged."
