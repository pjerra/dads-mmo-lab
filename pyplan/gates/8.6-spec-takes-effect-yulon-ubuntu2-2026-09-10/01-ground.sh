#!/usr/bin/env bash
# Step 01 -- the box before anything is touched. Read-only: no conf, no script,
# no account, no character, no SQL write, no restart.
#
# Nothing here cats a conf file into the log (memory note
# `gate-logs-carry-generated-passwords`): only `AiPlayerbot.*` key lines and
# file listings are printed, and the container environment is never dumped
# because it carries the database password.
source "$(dirname "$0")/lib.sh"
say "step 01, reading the box before anything is touched -- containers, the deployed bridge scripts, the module conf files, and what the app's picker reads today. READ-ONLY."
{
hdr "T18 live, step 01: the box as found"

sect "the three containers"
docker ps --format '{{.Names}}  {{.Status}}'

sect "the worldserver's own view of the module conf files it can read"
docker exec ac-worldserver ls -l /azerothcore/env/dist/etc/modules/
echo "-- and the same directory on the host, which is the bind mount --"
ls -l "$HOME/wowserver/env/dist/etc/modules/"

sect "the module clone the activation would read its template out of"
ls -l "$HOME/wowserver/modules/mod-playerbots/conf/"
echo "-- is it a git checkout, and does this app claim it? --"
( cd "$HOME/wowserver/modules/mod-playerbots" && git log -1 --format='HEAD %H %ad' --date=iso && git status --porcelain | head -5 && echo "(no lines above = nothing modified)" )
ls -l "$HOME/wowserver/modules/mod-playerbots/.yulon-clone.json" 2>&1
echo "-- and does this app claim the SERVER directory? (adoption fact 2) --"
ls -l "$HOME/wowserver/.yulon-install.json"

sect "the spec keys in the module's own template -- file and line on this box"
grep -c '^AiPlayerbot.PremadeSpecName' "$HOME/wowserver/modules/mod-playerbots/conf/playerbots.conf.dist"
echo "(the number above is how many PremadeSpecName keys the template defines)"
grep -n '^AiPlayerbot.PremadeSpecName.9.[0-9] ' "$HOME/wowserver/modules/mod-playerbots/conf/playerbots.conf.dist"
echo "-- and the module's own comment above them --"
sed -n '1760,1780p' "$HOME/wowserver/modules/mod-playerbots/conf/playerbots.conf.dist"

sect "what the module DOES with those keys -- the two lines party.py cites"
grep -n 'AiPlayerbot.PremadeSpecName' "$HOME/wowserver/modules/mod-playerbots/src/PlayerbotAIConfig.cpp"
grep -n 'specs found' "$HOME/wowserver/modules/mod-playerbots/src/Ai/Base/Actions/ChangeTalentsAction.cpp"
grep -n 'not found' "$HOME/wowserver/modules/mod-playerbots/src/Ai/Base/Actions/ChangeTalentsAction.cpp"

sect "what the WORLD said about that conf when it last started"
docker logs ac-worldserver 2>&1 | grep -iE "playerbots.conf|Not found modules config|modules config files" | head -10
echo "(a 'Failed open file .../playerbots.conf' line is what NO deployed conf looks like)"

sect "the bridge scripts deployed on this box, against the six the app ships"
ls -l "$LUA"
$PY - <<'PYEOF'
import sys
from pathlib import Path
sys.path.insert(0, str(Path.home() / "t18-live" / "pylauncher"))
from yulon import party
have = {p.name for p in (Path.home() / "wowserver/env/dist/etc/modules/lua_scripts").glob("*.lua")}
print("party.BRIDGE_SCRIPTS =", party.BRIDGE_SCRIPTS)
print("deployed  :", sorted(n for n in have if n.startswith("dml_")))
print("missing   :", sorted(set(party.BRIDGE_SCRIPTS) - have))
PYEOF

sect "the app's own reading of this install -- liveness, facts, preconditions"
$PRESS ground

sect "what the app's picker offers TODAY, class by class"
$PRESS specs

sect "and where it looks for the conf, and what it finds"
$PRESS conf

sect "the owner's things, untouched, read for the record"
$PRESS sql "SELECT username, last_login FROM acore_auth.account WHERE username IN ('PERZI','YULON_243C46E3','YULONADMIN') ORDER BY id;"
$PRESS sql "SELECT name, level, online FROM acore_characters.characters WHERE name='Pakka';"
$PRESS sql "SELECT id, name, address, localAddress, localSubnetMask FROM acore_auth.realmlist;"
ls -l "$LUA/LootPet2.lua"
grep -n '^Logger.ALE' "$HOME/wowserver/env/dist/etc/worldserver.conf"

sect "characters this lane will use, and the group table before anything"
$PRESS sql "SELECT guid, account, name, race, class, level, online FROM acore_characters.characters WHERE guid IN (1,4,11,12) ORDER BY guid;"
$PRESS sql "SELECT COUNT(*) AS group_member_rows FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) AS online_characters FROM acore_characters.characters WHERE online=1;"

sect "done"
} 2>&1 | tee "$OUT/01-ground.log"
say "step 01 done -- the box is recorded and nothing was written."
