#!/usr/bin/env bash
# T16 live half, step 01b -- the SERVER's own source for what `.character level`
# does, and for the one thing that writes an online character's row.
#
# The module was step 01. This step is AzerothCore itself, whose source tree is
# on the box at ~/wowserver/src (the same tree the image is built from), because
# the command `play.InstallPlay.set_level` sends is the core's, not the module's.
set -u
. "$(dirname "$0")/lib.sh"

AC="$HOME/wowserver"

say "step 01b -- reading AzerothCore's own source for .character level and .saveall. Nothing is changed."

hdr "T16 step 01b -- the core's own source"

sect "which AzerothCore this is"
$PRESS send server info
git -C "$AC" rev-parse HEAD 2>/dev/null || echo "(the server folder is not a git checkout: $(ls -d "$AC"/.git 2>/dev/null || echo 'no .git'))"
grep -rn 'AC_VERSION\|rev\.' "$AC/src/common/GitRevision.cpp" 2>/dev/null | head -3

sect "SOURCE A -- .character level: ONLINE gives the level in memory and writes NO row"
grep -n 'static void HandleCharacterLevel' "$AC/src/server/scripts/Commands/cs_character.cpp"
sed -n '252,282p' "$AC/src/server/scripts/Commands/cs_character.cpp"

sect "SOURCE B -- the command that calls it, and the sentence the app reads back"
grep -n 'static bool HandleCharacterLevelCommand' "$AC/src/server/scripts/Commands/cs_character.cpp"
sed -n '439,455p' "$AC/src/server/scripts/Commands/cs_character.cpp"
grep -rn 'LANG_YOU_CHANGE_LVL' "$AC/src/server/game/Miscellaneous/Language.h" | head -2
grep -rn 'You changed level of' "$AC"/sql/base/db_world/*.sql "$AC"/data 2>/dev/null | head -2

sect "SOURCE C -- .saveall: every Player in the world, bots included"
grep -n 'static bool HandleSaveAllCommand' "$AC/src/server/scripts/Commands/cs_misc.cpp"
sed -n '1401,1408p' "$AC/src/server/scripts/Commands/cs_misc.cpp"
grep -n 'void ObjectAccessor::SaveAllPlayers' "$AC/src/server/game/Globals/ObjectAccessor.cpp"
sed -n '286,294p' "$AC/src/server/game/Globals/ObjectAccessor.cpp"

sect "SOURCE D -- how often a character saves itself, with nobody asking"
grep -n '^PlayerSaveInterval' "$AC/env/dist/etc/worldserver.conf"
grep -rn 'CONFIG_INTERVAL_SAVE' "$AC/src/server/game/World/World.cpp" | head -3

sect "SOURCE E -- the module's own save, the one that DID move a row in this run"
grep -n 'bot->SaveToDB' "$AC/modules/mod-playerbots/src/Bot/Factory/PlayerbotFactory.cpp" | grep -v '//' | head -3
sed -n '866,876p' "$AC/modules/mod-playerbots/src/Bot/Factory/PlayerbotFactory.cpp"

sect "done"
say "step 01b finished -- the core's own level and save paths are cited."
echo "step 01b finished $(stamp)"
