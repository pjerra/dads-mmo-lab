#!/usr/bin/env bash
# Step 05 -- the OTHER route: the app's own Lua bridge, which runs the command
# inside a player's own session (ALE `Player:RunCommand`). Nothing new is
# deployed and the world is NOT restarted: `dml_login.lua` is already on the box
# from T13's run, and `playerbots bot login <name>` enters mod-playerbots through
# the SAME branch as `add` --
#   PlayerbotMgr.cpp:683  if (cmd == "add" || cmd == "addaccount" || cmd == "login")
# -- so it exercises exactly the permission path this ticket is about.
#
# Each press: an empty `docker logs --since` window, the group table and the
# online count before and after.
source "$HOME/t26-out/lib.sh"

snap() {  # snap <tag>
    echo "-- $1: group_member rows --"
    q "SELECT gm.guid, gm.memberGuid, c.name FROM acore_characters.group_member gm JOIN acore_characters.characters c ON c.guid=gm.memberGuid ORDER BY gm.guid, gm.memberGuid;" | tee "$OUT/05-grouprows-$1.txt"
    echo "   (rows: $(wc -l < "$OUT/05-grouprows-$1.txt"))"
    echo "-- $1: online characters = $(q 'SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;')"
}

case_press() {  # case_press <label> <master> <target>
    local label="$1" master="$2" target="$3"
    sect "CASE $label -- master $master, target $target"
    qt "SELECT c.guid,c.name,c.account,c.class,c.level,c.online,IFNULL(gm.guildid,0) AS guildid FROM acore_characters.characters c LEFT JOIN acore_characters.guild_member gm ON gm.guid=c.guid WHERE c.name IN ('$master','$target');"
    snap "$label-before"
    local since; since=$(utc)
    echo ">>> window opens at SINCE=$since"
    echo ">>> [dml_login] lines in the window BEFORE the press: $(docker logs ac-worldserver --since "$since" 2>&1 | grep -c 'dml_login' || true)"
    $PRESS send "dml_login $master $target"
    sleep 12
    echo ">>> the world's own log for that window:"
    docker logs ac-worldserver --since "$since" 2>&1 | grep -E "dml_login|playerbot|Playerbot" || echo "(no matching line)"
    snap "$label-after"
    echo "-- $target online now? --"
    q "SELECT name,online FROM acore_characters.characters WHERE name='$target';"
}

say "step 05, pressing the app's own Lua relay (Player:RunCommand) for the three cases. Nothing is deployed, the world is not restarted."
{
hdr "T26 measure, step 05: the relay route, three cases"

sect "the relay script that is already deployed, and what it runs"
md5sum "$HOME/wowserver/env/dist/etc/modules/lua_scripts/dml_login.lua"
grep -n "RunCommand\|playerbots bot login" "$HOME/wowserver/env/dist/etc/modules/lua_scripts/dml_login.lua"

sect "who is online at all -- the answer that decides what this step can prove"
$PRESS send "server info"
echo "(the 'Connected players' number is REAL sessions; 'Characters in world' counts the bots)"

case_press "a-same-account"   Jurnaar Nore
case_press "b-other-account"  Jurnaar Rinmu
case_press "c-guild-mate"     Dalio   Jordanik

sect "the box after the three presses"
echo "group_member rows = $(q 'SELECT COUNT(*) FROM acore_characters.group_member;')"
echo "groups rows       = $(q 'SELECT COUNT(*) FROM acore_characters.groups;')"
echo "online            = $(q 'SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;')"
echo "characters        = $(q 'SELECT COUNT(*) FROM acore_characters.characters;')"

sect "done"
} 2>&1 | tee "$OUT/05-relay.log"
say "step 05 done."
