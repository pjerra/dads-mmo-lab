#!/usr/bin/env bash
# T16 live half, step 01 -- the box before this lane touched anything, and the
# MODULE SOURCE the ticket asks to be cited: what runs after a bot joins, and
# every line in the module that writes a level.
#
# Nothing here changes anything. No conf file is `cat`ed and no container
# environment is printed (memory note `gate-logs-carry-generated-passwords`):
# only named keys and file listings.
set -u
. "$(dirname "$0")/lib.sh"

say "step 01 -- reading the box ground and the module's own level paths. Nothing is changed."

hdr "T16 step 01 -- ground"

MOD="$HOME/wowserver/modules/mod-playerbots"

sect "containers"
docker ps --format '{{.Names}}\t{{.Status}}'
$PRESS liveness

sect "the module clone these line numbers are from"
git -C "$MOD" rev-parse HEAD
echo "modified files: $(git -C "$MOD" status --porcelain | wc -l)"

sect "the bridge as found"
ls -1 "$LUA"
$PRESS ground 2>&1 | head -30

sect "the deployed conf T18 left, and the level keys the randomiser reads"
ls -l "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"
md5sum "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"
grep -n '^AiPlayerbot\.\(RandomBotMinLevel\|RandomBotMaxLevel\|RandomBotMaxLevelChance\|RandomBotMinLevelChance\|DisableRandomLevels\|RandombotStartingLevel\|RandomBotFixedLevel\|SyncLevelWithPlayers\|DowngradeMaxLevelBot\|AddClassCommand\|RandomBotAutoJoinBG\|RandomBotUpdateInterval\|RandomBotTeleportationDistance\|AutoInitOnly\|AutoInitEquipLevelLimitRatio\|EquipAndSpecPersistence\|EquipAndSpecPersistenceLevel\)' \
    "$HOME/wowserver/env/dist/etc/modules/playerbots.conf"

sect "this server's own MaxPlayerLevel, and PlayerSave.Interval (why an online row lags)"
grep -n '^MaxPlayerLevel\|^PlayerSave\.Interval\|^PlayerSaveInterval' "$HOME/wowserver/env/dist/etc/worldserver.conf"

sect "SOURCE 1 -- what addclass does: it logs an OFFLINE character in"
grep -n 'AddPlayerBot(guid, master->GetSession()->GetAccountId());' "$MOD/src/Bot/PlayerbotMgr.cpp"
sed -n '1155,1180p' "$MOD/src/Bot/PlayerbotMgr.cpp"

sect "SOURCE 2 -- the login path, queued onto the world thread"
grep -n 'sRandomPlayerbotMgr.OnPlayerLogin(bot);' "$MOD/src/Bot/PlayerbotMgr.cpp"
grep -n 'OnBotLoginOperation' "$MOD/src/Bot/PlayerbotMgr.cpp" | head -5
grep -n 'void PlayerbotHolder::OnBotLogin' "$MOD/src/Bot/PlayerbotMgr.cpp"

sect "SOURCE 3 -- the addclass bot is re-made at the MASTER's level, on login"
grep -n 'bool addClassBot = sRandomPlayerbotMgr.IsAccountType' "$MOD/src/Bot/PlayerbotMgr.cpp"
sed -n '570,600p' "$MOD/src/Bot/PlayerbotMgr.cpp"

sect "SOURCE 4 -- every line in the module that writes a player level"
grep -rn 'GiveLevel' "$MOD/src/" --include=*.cpp

sect "SOURCE 5 -- PlayerbotFactory::Prepare, the one GiveLevel every Randomize goes through"
grep -n 'void PlayerbotFactory::Prepare' "$MOD/src/Bot/Factory/PlayerbotFactory.cpp"
sed -n '583,596p' "$MOD/src/Bot/Factory/PlayerbotFactory.cpp"
grep -n 'void PlayerbotFactory::Randomize' "$MOD/src/Bot/Factory/PlayerbotFactory.cpp"

sect "SOURCE 6 -- the RandomBots refresh that could move a level later"
grep -n 'void RandomPlayerbotMgr::Randomize\|void RandomPlayerbotMgr::RandomizeFirst\|void RandomPlayerbotMgr::IncreaseLevel\|void RandomPlayerbotMgr::Refresh\|void RandomPlayerbotMgr::ProcessBot' "$MOD/src/Bot/RandomPlayerbotMgr.cpp"
grep -n 'Randomize(bot)\|RandomizeFirst(bot)\|IncreaseLevel(bot)\|Refresh(bot)' "$MOD/src/Bot/RandomPlayerbotMgr.cpp"

sect "SOURCE 7 -- autogear and AutoMaintenance, the other two the ticket names"
grep -rn 'class AutoMaintenanceOnLevelupAction\|AutoMaintenanceOnLevelupAction::Execute' "$MOD/src/Ai/Base/Actions/AutoMaintenanceOnLevelupAction.h" "$MOD/src/Ai/Base/Actions/AutoMaintenanceOnLevelupAction.cpp"
grep -rn 'autogear' "$MOD/src/" --include=*.cpp | head -10

sect "SOURCE 8 -- SaveToDB on the login path"
grep -n 'bot->SaveToDB' "$MOD/src/Bot/PlayerbotMgr.cpp" "$MOD/src/Bot/RandomPlayerbotMgr.cpp" | head -20

sect "the owner's things"
$PRESS sql "SELECT username, last_login FROM acore_auth.account WHERE username='PERZI';"
$PRESS sql "SELECT name, level, online FROM acore_characters.characters WHERE name='Pakka';"
$PRESS sql "SELECT id, address, localAddress, localSubnetMask FROM acore_auth.realmlist;"
ls -l "$LUA/LootPet2.lua"
grep -n '^Logger.ALE' "$HOME/wowserver/env/dist/etc/worldserver.conf"

sect "the population"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.characters;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.group_member;"
$PRESS sql "SELECT COUNT(*) FROM acore_characters.groups;"
$PRESS sql "SELECT COUNT(*) FROM acore_auth.account;"
$PRESS sql "SELECT id, username FROM acore_auth.account WHERE id IN (101,103);"

sect "done"
say "step 01 finished -- the ground and the module's level paths are read."
echo "step 01 finished $(stamp)"
