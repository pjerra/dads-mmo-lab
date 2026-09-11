#!/usr/bin/env bash
# T26 live half, step 01 -- the box before anything, and the two things this
# lane must prove about it before it presses: the sixth bridge script, and what
# the module's own conf says about the four rules.
#
# Nothing here writes. Step 09 reads every one of these numbers back.
set -u
. "$(dirname "$0")/lib.sh"

say "step 01 -- reading the box before anything is touched: containers, counts, the owner's things, the six bridge scripts and the module conf."

hdr "T26 live step 01 -- the box as found"

sect "the box"
hostname; date; uptime

sect "the three containers"
docker ps --format "{{.Names}}\t{{.Status}}\t{{.Ports}}"
for c in ac-worldserver ac-authserver ac-database; do
    echo "$c: $(docker inspect $c --format 'started={{.State.StartedAt}} restarts={{.RestartCount}} status={{.State.Status}}')"
done

sect "the module clone, pinned"
git -C "$HOME/wowserver/modules/mod-playerbots" rev-parse HEAD
git -C "$HOME/wowserver/modules/mod-playerbots" status --porcelain | head

sect "THE SIXTH BRIDGE SCRIPT -- on disk, and loaded in the RUNNING world"
ls -l --time-style=long-iso "$LUA"
md5sum "$LUA"/dml_*.lua
echo
echo "the same six in this ticket's tree:"
md5sum "$TREE"/pylauncher/lua/party/dml_*.lua
echo
S=$(docker inspect ac-worldserver --format "{{.State.StartedAt}}")
echo "the running world started at $S; every 'loaded' line it printed since:"
docker logs ac-worldserver --since "$S" 2>&1 | grep -E "^\[dml_[a-z_]+\] loaded"

sect "the module conf as deployed -- the cap and the three allow flags"
ls -l "$HOME/wowserver/env/dist/etc/modules/playerbots.conf" "$HOME/wowserver/env/dist/etc/modules/playerbots.conf.dist"
md5sum "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"
grep -nE "^AiPlayerbot\.(MaxAddedBots|AllowAccountBots|AllowGuildBots|AllowTrustedAccountBots|BotAutologin|KeepAltsInGroup|Enabled) " "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

sect "the owner's things -- untouchable, read here so step 09 can read them back"
ls -l --time-style=long-iso "$LUA/LootPet2.lua"
md5sum "$LUA/LootPet2.lua"
grep -n "^Logger.ALE" "$HOME/wowserver/env/dist/etc/worldserver.conf"
dbq1 "SELECT address, localAddress, localSubnetMask FROM acore_auth.realmlist;"
dbq1 "SELECT id, username, last_login FROM acore_auth.account WHERE username IN ('PERZI','YULONADMIN','YULON_243C46E3') ORDER BY id;"
dbq1 "SELECT guid, account, name, level, online FROM acore_characters.characters WHERE name = 'Pakka';"

sect "the counts this lane will be read back against"
dbq1 "SELECT COUNT(*) FROM acore_auth.account;"
dbq1 "SELECT COUNT(*) FROM acore_auth.account_access;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters WHERE online = 1;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild;"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member;"
dbq1 "SELECT COUNT(*) FROM acore_characters.groups;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_keys;"
dbq1 "SELECT account_type, COUNT(*) FROM acore_playerbots.playerbots_account_type GROUP BY account_type;"

sect "this server's own MaxPlayerLevel, and the pool this lane will copy a character out of"
grep -nE "^MaxPlayerLevel" "$HOME/wowserver/env/dist/etc/worldserver.conf"
dbq1 "SELECT c.guid, c.name, c.account, c.race, c.class, c.level, c.online FROM acore_characters.characters c JOIN acore_playerbots.playerbots_account_type t ON t.account_id = c.account WHERE t.account_type = 2 AND c.online = 0 ORDER BY c.guid LIMIT 5;"

sect "the world answers, and what is in it"
$PRESS send server info
$PRESS send dml_bridge_ping

sect "the panel's own preconditions"
$PRESS ground

sect "done"
say "step 01 finished -- the box is read and nothing was written."
echo "step 01 finished $(stamp)"
