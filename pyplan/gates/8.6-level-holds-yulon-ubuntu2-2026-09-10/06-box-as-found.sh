#!/usr/bin/env bash
# T16 live half, step 06 -- the box read back against step 01's own numbers.
set -u
. "$(dirname "$0")/lib.sh"

say "step 06 -- reading the box back against step 01's numbers. Nothing is changed."

hdr "T16 step 06 -- the box as found"

sect "containers"
docker ps --format '{{.Names}}\t{{.Status}}'
$PRESS liveness

sect "the owner's things -- step 01 read PERZI 2026-09-08 23:24:16, Pakka level 6 offline, the realm row 100.99.204.5/100.99.204.5/255.255.255.0, LootPet2.lua 37294 bytes, Logger.ALE at line 706"
$PRESS sql "SELECT username, last_login FROM acore_auth.account WHERE username='PERZI';"
$PRESS sql "SELECT name, level, online FROM acore_characters.characters WHERE name='Pakka';"
$PRESS sql "SELECT id, address, localAddress, localSubnetMask FROM acore_auth.realmlist;"
ls -l "$LUA/LootPet2.lua"
grep -n '^Logger.ALE' "$HOME/wowserver/env/dist/etc/worldserver.conf"

sect "no account was made and none was touched -- step 01 read 103 accounts and 0 orphans"
$PRESS sql "SELECT COUNT(*) FROM acore_auth.account;"
$PRESS sql "SELECT id, username FROM acore_auth.account WHERE id IN (101,103);"
$PRESS sql "SELECT COUNT(*) FROM acore_auth.account_access;"
# The column is `id` on this core, not `AccountID`: the first run of this step
# named the wrong one and the read FAILED rather than answering 0, which is the
# whole reason it is spelled out here (memory note `incomplete-artifact-reads-as-fact`:
# a probe that cannot answer is not evidence of anything).
$PRESS sql "DESCRIBE acore_auth.account_access;"
$PRESS sql "SELECT COUNT(*) FROM acore_auth.account_access aa LEFT JOIN acore_auth.account a ON a.id = aa.id WHERE a.id IS NULL;"

sect "the population -- step 01 read 1001 characters, 500 online, 0 group rows"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.characters;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.groups;"

sect "the bridge and the conf -- six scripts, and T18's playerbots.conf md5 bd8d55ae591adfed443f174ed1311c61"
ls -1 "$LUA"
md5sum "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"
ls -l "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

sect "the module clone -- step 01 read b949b50b with nothing modified"
git -C "$HOME/wowserver/modules/mod-playerbots" rev-parse HEAD
echo "modified files: $(git -C "$HOME/wowserver/modules/mod-playerbots" status --porcelain | wc -l)"

sect "the characters this lane levelled, and who chose the level they carry now"
for n in "$@"; do
    $PRESS sql "SELECT name, level, online FROM acore_characters.characters WHERE name='$n';"
done

sect "done"
say "step 06 finished -- the box is read back."
echo "step 06 finished $(stamp)"
