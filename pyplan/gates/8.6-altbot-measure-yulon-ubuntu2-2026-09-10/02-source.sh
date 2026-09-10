#!/usr/bin/env bash
# Step 02 -- the module's own source, on this box, at this commit. Read-only.
source "$HOME/t26-out/lib.sh"
SRC="$HOME/wowserver/modules/mod-playerbots/src"
say "step 02, reading mod-playerbots' own source on the box: which command adds a NAMED character, and the permission rules that gate it. Read-only, nothing is run."
{
hdr "T26 measure, step 02: the module's source"

sect "which module, which commit"
cd "$HOME/wowserver/modules/mod-playerbots" && git log -1 --format="commit %H%nauthored %ad%nsubject %s" --date=iso
echo "path: $HOME/wowserver/modules/mod-playerbots"

sect "the brief's own grep"
cd "$HOME/wowserver" && find . -path '*playerbots*' -name '*.cpp' | xargs grep -ln "bot add\|AddBot\|IsAllowedToJoin\|AllowGuildBots\|AllowAccountBots\|RandomBotAccount" 2>/dev/null

sect "A. the command table -- src/Script/PlayerbotCommandScript.cpp"
grep -n "" "$SRC/Script/PlayerbotCommandScript.cpp" | sed -n '25,75p'

sect "B. the entry point -- src/Bot/PlayerbotMgr.cpp:860-895 (HandlePlayerbotMgrCommand)"
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '860,896p'

sect "C. what 'add' does -- src/Bot/PlayerbotMgr.cpp:673-706 (ProcessBotCommand)"
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '673,707p'

sect "D. THE THREE RULES -- src/Bot/PlayerbotMgr.cpp:84-118 (AddPlayerBot)"
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '84,120p'

sect "E. the cap and how the refusal is delivered -- src/Bot/PlayerbotMgr.cpp:120-145"
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '120,146p'

sect "F. the fourth rule: linked accounts -- src/Bot/PlayerbotMgr.cpp:191-196 (IsAccountLinked)"
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '191,197p'

sect "G. how the NAME is resolved for 'add' -- src/Bot/PlayerbotMgr.cpp:1281-1325"
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '1281,1325p'

sect "H. who gets a PlayerbotMgr at all -- src/Script/Playerbots.cpp:82-90 and src/Bot/PlayerbotMgr.cpp:1803-1817"
grep -n "" "$SRC/Script/Playerbots.cpp" | sed -n '82,90p'
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '1803,1818p'
grep -n "" "$SRC/Bot/PlayerbotMgr.cpp" | sed -n '1731,1762p'

sect "I. the config keys -- src/PlayerbotAIConfig.cpp"
grep -n "AllowAccountBots\|AllowGuildBots\|AllowTrustedAccountBots\|RandomBotAccountPrefix\|RandomBotAccountCount\|MaxAddedBots\|SelfBotLevel" "$SRC/PlayerbotAIConfig.cpp"
echo "-- and the fields they land in --"
grep -n "allowAccountBots, allowGuildBots\|randomBotAccountPrefix\|maxAddedBots" "$SRC/PlayerbotAIConfig.h"
echo "-- is there any 'friends' or 'AllowPlayerBots' key at all? --"
grep -rn "AiPlayerbot.Allow" "$SRC" | sort -u
echo "-- keys with 'friend' in the name --"
grep -rno "AiPlayerbot\.[A-Za-z.]*[Ff]riend[A-Za-z.]*" "$SRC" | sort -u
echo "-- keys with 'PlayerBots' in the name --"
grep -rno "AiPlayerbot\.[A-Za-z.]*PlayerBots[A-Za-z.]*" "$SRC" | sort -u

sect "J. what makes a character an 'addclass bot' -- src/Bot/RandomPlayerbotMgr.cpp:1719-1755, :2154-2175"
grep -n "" "$SRC/Bot/RandomPlayerbotMgr.cpp" | sed -n '1719,1755p'
grep -n "" "$SRC/Bot/RandomPlayerbotMgr.cpp" | sed -n '2154,2176p'

sect "K. which accounts count as random-bot accounts -- src/Bot/RandomPlayerbotMgr.cpp:488-520, :610-625"
grep -n "" "$SRC/Bot/RandomPlayerbotMgr.cpp" | sed -n '488,520p'
grep -n "" "$SRC/Bot/RandomPlayerbotMgr.cpp" | sed -n '610,626p'

sect "done"
} 2>&1 | tee "$OUT/02-source.log"
say "step 02 done -- the module's source is read."
