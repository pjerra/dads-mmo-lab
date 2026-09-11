#!/usr/bin/env bash
# T26 live half, step 02 -- the three throwaway accounts and the four throwaway
# characters this lane needs, and nothing else.
#
# WHY ANY OF THIS EXISTS. The four claims need a master with (1) another
# character on its own account, (2) an OFFLINE guild mate on a different
# account, and (3) an OFFLINE character on a third account that is not in the
# guild and not in the addclass pool. The measure of 2026-09-10 counted what
# this box has for a master: same-account 0, guild 0, linked 0, addclass pool
# 500, online-and-refused 500 -- so not one of the three exists here and every
# offline character except the owner's own `Pakka` is in the pool, which rule 3
# lets ANYBODY add. `Pakka` is out of bounds. So they are made.
#
# HOW, and why not with a client. Characters are made with the CORE's own
# `.pdump write` / `.pdump load` (`cs_character.cpp:57-58`, both `Console::Yes`,
# so the app's own SOAP channel can send them), copying one offline addclass-pool
# character into three throwaway accounts under new names and new guids. A new
# guid is in no pool, so rule 3 cannot answer for any of them and the rule that
# does is the one each claim is about. `PlayerDump.cpp:956` calls
# `sCharacterCache->AddCharacterCacheEntry(...)` on every load, which is what
# `.playerbots bot add` resolves a name through
# (`PlayerbotMgr.cpp:1281-1325`) -- so no world restart is owed for them.
#
# The account password comes from $T26_PW and is never in argv, never printed
# and never written to this box.
set -u
. "$(dirname "$0")/lib.sh"

SRC=${1:?the source character to copy}
DUMP=t26src.dump
# A BARE name and no path: this server ships `PlayerDump.DisallowPaths = 1`
# (`worldserver.conf:4688`), and `PlayerDumpWriter::WriteDumpToFile`
# (`PlayerDump.cpp:718-721`) refuses any name carrying a slash before it opens
# anything -- measured here at 01:51 with `/tmp/t26src.dump`, which came back
# "Failed to open file". The world's own working directory is `/azerothcore`
# and it is writable by the container's `acore` user.
DUMPPATH=/azerothcore/$DUMP

say "step 02 -- creating three throwaway accounts (T26MASTER, T26MATE, T26FRIEND) through the app's own Accounts seam, and four throwaway characters with the core's own .pdump load. No account or character that already existed is touched."

hdr "T26 live step 02 -- the throwaway accounts and characters"

sect "the source character, before it is copied"
dbq1 "SELECT guid, name, account, race, class, level, online FROM acore_characters.characters WHERE name = '$SRC';"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member WHERE guid = (SELECT guid FROM acore_characters.characters WHERE name = '$SRC');"

sect "the three accounts, through the Accounts tab's own create_account"
for a in T26MASTER T26MATE T26FRIEND; do
    $PRESS mkaccount "$a"
done
dbq1 "SELECT id, username, email, joindate FROM acore_auth.account WHERE username LIKE 'T26%' ORDER BY id;"
dbq1 "SELECT COUNT(*) FROM acore_auth.account_access;"

sect "the dump written out of the running world"
SINCE=$(utc)
echo "window opens at $SINCE"
$PRESS send pdump write "$DUMP" "$SRC"
docker exec ac-worldserver sh -c "ls -l $DUMPPATH; head -3 $DUMPPATH; wc -l $DUMPPATH"

sect "the four characters loaded back in, under new names on the three accounts"
for pair in "T26MASTER Tsixmaster" "T26MASTER Tsixalt" "T26MATE Tsixmate" "T26FRIEND Tsixfriend"; do
    set -- $pair
    echo ">>> .pdump load $DUMP $1 $2"
    $PRESS send pdump load "$DUMP" "$1" "$2"
done

sect "what they are now -- and that none of them is in a pool or a guild"
dbq1 "SELECT c.guid, c.name, c.account, a.username, c.race, c.class, c.level, c.online FROM acore_characters.characters c JOIN acore_auth.account a ON a.id = c.account WHERE c.name IN ('Tsixmaster','Tsixalt','Tsixmate','Tsixfriend') ORDER BY c.guid;"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_type WHERE account_id IN (SELECT account FROM acore_characters.characters WHERE name IN ('Tsixmaster','Tsixalt','Tsixmate','Tsixfriend'));"
dbq1 "SELECT COUNT(*) FROM acore_characters.guild_member WHERE guid IN (SELECT guid FROM acore_characters.characters WHERE name IN ('Tsixmaster','Tsixalt','Tsixmate','Tsixfriend'));"
dbq1 "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"

sect "the counts, moved by exactly what this step made"
dbq1 "SELECT COUNT(*) FROM acore_auth.account;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters;"
dbq1 "SELECT COUNT(*) FROM acore_characters.characters WHERE online = 1;"
dbq1 "SELECT COUNT(*) FROM acore_characters.group_member;"

sect "the world's own log for the window"
docker logs ac-worldserver --since "$SINCE" 2>&1 | grep -iE "pdump|dump" | head -20

sect "the dump file removed from the container"
docker exec ac-worldserver rm -f "$DUMPPATH"
docker exec ac-worldserver sh -c "ls -l $DUMPPATH 2>&1 | head -2"

sect "done"
say "step 02 finished -- three throwaway accounts and four throwaway characters exist, all offline, none in a pool and none in a guild."
echo "step 02 finished $(stamp)"
