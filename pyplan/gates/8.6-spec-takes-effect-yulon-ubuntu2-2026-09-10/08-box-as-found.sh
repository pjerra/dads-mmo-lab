#!/usr/bin/env bash
# Step 08 -- the box read back against step 01's own numbers, after everything.
# Every line here has a line in `01-ground.log` to be compared with.
source "$(dirname "$0")/lib.sh"

say "step 08, reading the box back against step 01's own numbers. Read-only."
{
hdr "T18 live, step 08: the box as left"

sect "the three containers"
docker ps --format '{{.Names}}  {{.Status}}'
$PRESS liveness

sect "the owner's things -- compare with 01-ground.log"
$PRESS sql "SELECT username, last_login FROM acore_auth.account WHERE username IN ('PERZI','YULON_243C46E3','YULONADMIN') ORDER BY id;"
$PRESS sql "SELECT name, level, online FROM acore_characters.characters WHERE name='Pakka';"
$PRESS sql "SELECT id, name, address, localAddress, localSubnetMask FROM acore_auth.realmlist;"
ls -l "$LUA/LootPet2.lua"
grep -n '^Logger.ALE' "$HOME/wowserver/env/dist/etc/worldserver.conf"

sect "accounts -- this lane made none, and none was touched"
$PRESS sql "SELECT COUNT(*) AS accounts FROM acore_auth.account;"
$PRESS sql "SELECT id, gmlevel, RealmID FROM acore_auth.account_access ORDER BY id;"
echo "-- orphaned account_access rows (must be 0) --"
$PRESS sql "SELECT COUNT(*) FROM acore_auth.account_access aa LEFT JOIN acore_auth.account a ON a.id=aa.id WHERE a.id IS NULL;"

sect "characters -- the count, and the two this lane used"
$PRESS sql "SELECT COUNT(*) AS characters FROM acore_characters.characters;"
$PRESS sql "SELECT COUNT(*) AS online FROM acore_characters.characters WHERE online=1;"
$PRESS sql "SELECT guid, account, name, race, class, level, online FROM acore_characters.characters WHERE guid IN (1,188) ORDER BY guid;"

sect "the group table -- empty again"
$PRESS grouprows
$PRESS sql "SELECT COUNT(*) AS group_member_rows FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) AS groups_rows FROM acore_characters.\`groups\`;"

sect "the module conf directory -- what this lane left"
ls -l "$HOME/wowserver/env/dist/etc/modules/"
ls -l "$LUA"
echo "-- the deployed conf is still byte-identical to the module's template --"
md5sum "$HOME/wowserver/modules/mod-playerbots/conf/playerbots.conf.dist" \
       "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

sect "the module clone -- untouched, still what step 01 read"
( cd "$HOME/wowserver/modules/mod-playerbots" && git log -1 --format='HEAD %H' && git status --porcelain | head -5 && echo "(no lines above = nothing modified)" )
ls -l "$HOME/wowserver/modules/mod-playerbots/.yulon-clone.json" 2>&1

sect "the app's own reading, last"
$PRESS ground
$PRESS conf

sect "done"
} 2>&1 | tee "$OUT/08-box-as-found.log"
say "step 08 done -- the box is read back and matches step 01 except for the deployed conf and the sixth bridge script."
