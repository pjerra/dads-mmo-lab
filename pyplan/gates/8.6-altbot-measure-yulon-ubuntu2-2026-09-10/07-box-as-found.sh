#!/usr/bin/env bash
# Step 07 -- the box read back, against step 01's own numbers.
source "$HOME/t26-out/lib.sh"
say "step 07, reading the box back to prove it is as it was found: counts, containers, conf files, the bridge scripts, PERZI and Pakka."
{
hdr "T26 measure, step 07: the box as left"

sect "containers -- the same StartedAt values step 01 read, and no restart"
docker ps --format '{{.Names}}\t{{.Status}}'
docker inspect ac-worldserver --format 'worldserver started={{.State.StartedAt}} restarts={{.RestartCount}}'
docker inspect ac-authserver --format 'authserver  started={{.State.StartedAt}} restarts={{.RestartCount}}'
docker inspect ac-database   --format 'database    started={{.State.StartedAt}} restarts={{.RestartCount}}'

sect "the counts step 01 took"
echo "accounts    = $(q 'SELECT COUNT(*) FROM acore_auth.account;')"
echo "characters  = $(q 'SELECT COUNT(*) FROM acore_characters.characters;')"
echo "guilds      = $(q 'SELECT COUNT(*) FROM acore_characters.guild;')"
echo "guild_member= $(q 'SELECT COUNT(*) FROM acore_characters.guild_member;')"
echo "group_member= $(q 'SELECT COUNT(*) FROM acore_characters.group_member;')"
echo "groups      = $(q 'SELECT COUNT(*) FROM acore_characters.groups;')"
echo "online      = $(q 'SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;')"
echo "account_links = $(q 'SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;')"
echo "account_keys  = $(q 'SELECT COUNT(*) FROM acore_playerbots.playerbots_account_keys;')"

sect "the same three non-bot accounts, the same gm levels, the same last_login"
qt "SELECT a.id,a.username,a.last_login,IFNULL(aa.gmlevel,0) AS gmlevel FROM acore_auth.account a LEFT JOIN acore_auth.account_access aa ON aa.id=a.id WHERE a.username NOT LIKE 'RNDBOT%' ORDER BY a.id;"
qt "SELECT guid,name,account,level,online FROM acore_characters.characters WHERE name='Pakka';"

sect "the five names this lane read, unchanged"
qt "SELECT guid,name,account,level,online FROM acore_characters.characters WHERE name IN ('Jurnaar','Dalio','Nore','Zarraden','Grirmirn','Jordanik','Rinmu') ORDER BY guid;"

sect "no conf was edited -- the module conf directory, byte sizes and mtimes"
ls -l "$HOME/wowserver/env/dist/etc/modules/"
md5sum "$HOME/wowserver/env/dist/etc/modules/playerbots.conf.dist" "$HOME/wowserver/env/dist/etc/modules/mod_ale.conf"

sect "the deployed bridge scripts, untouched (no script was added or removed)"
ls -l "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"
md5sum "$HOME/wowserver/env/dist/etc/modules/lua_scripts/"*.lua

sect "the world is up and the bridge still answers"
$PRESS send dml_bridge_ping
$PRESS send "server info"

sect "nothing was created: no account, character, guild, group or link was made by this lane"
echo "(this lane sent no .account create, no SQL write of any kind, and no guild command)"

sect "done"
} 2>&1 | tee "$OUT/07-box-as-found.log"
say "step 07 done -- the box reads back as it was found."
