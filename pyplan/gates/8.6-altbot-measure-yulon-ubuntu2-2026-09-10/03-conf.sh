#!/usr/bin/env bash
# Step 03 -- the conf as SHIPPED on this box, and the env that can override it.
# Nothing here writes: every conf file is read, and only AiPlayerbot.* / AC_AI_*
# key lines are printed (memory note `gate-logs-carry-generated-passwords`:
# never cat a conf into a log).
source "$HOME/t26-out/lib.sh"
MODS="$HOME/wowserver/env/dist/etc/modules"
say "step 03, reading the shipped playerbots conf keys and the compose env that can override them. Read-only; no conf is edited."
{
hdr "T26 measure, step 03: the conf as shipped"

sect "which files exist (a .conf would win over the .conf.dist; there is none)"
ls -l "$MODS"/*.conf "$MODS"/*.conf.dist 2>&1
echo "-- and inside the container, which the world actually reads --"
docker exec ac-worldserver ls -l /azerothcore/env/dist/etc/modules/ 2>&1 | sed -n '1,20p'

sect "the four gate keys, as shipped -- $MODS/playerbots.conf.dist"
grep -n "AiPlayerbot.AllowAccountBots\|AiPlayerbot.AllowGuildBots\|AiPlayerbot.AllowTrustedAccountBots\|AiPlayerbot.RandomBotAccountPrefix\|AiPlayerbot.RandomBotAccountCount\|AiPlayerbot.MaxAddedBots\|AiPlayerbot.SelfBotLevel\|AiPlayerbot.Enabled " "$MODS/playerbots.conf.dist"

sect "with their comment blocks (the module's own words)"
for k in AllowAccountBots AllowGuildBots AllowTrustedAccountBots RandomBotAccountPrefix MaxAddedBots; do
    echo "### AiPlayerbot.$k"
    n=$(grep -n "^AiPlayerbot.$k" "$MODS/playerbots.conf.dist" | head -1 | cut -d: -f1)
    [ -n "$n" ] && sed -n "$((n>14 ? n-14 : 1)),${n}p" "$MODS/playerbots.conf.dist"
    echo
done

sect "is any AiPlayerbot key set anywhere ELSE (a .conf, worldserver.conf, an include)?"
grep -rn "AllowGuildBots\|AllowAccountBots\|AllowTrustedAccountBots\|RandomBotAccountPrefix" "$HOME/wowserver/env/dist/etc/" 2>/dev/null | grep -v "playerbots.conf.dist"
echo "(nothing above = the .dist values are the live ones)"

sect "the compose env that CAN override a conf key (only AC_AI_* names printed)"
grep -n "AC_AI_" "$HOME/wowserver/docker-compose.override.yml" | sed 's/=.*$/=<value below>/' >/dev/null
grep -n "AC_AI_" "$HOME/wowserver/docker-compose.override.yml"

sect "and what the RUNNING worldserver container actually carries (AC_AI_* only)"
docker inspect ac-worldserver --format '{{range .Config.Env}}{{println .}}{{end}}' | grep "^AC_AI_" | sort

sect "the playerbots database and the three tables the rules read"
q "SHOW DATABASES LIKE 'acore_playerbots';"
qt "SELECT TABLE_NAME, TABLE_ROWS FROM information_schema.tables WHERE TABLE_SCHEMA='acore_playerbots' AND TABLE_NAME IN ('playerbots_account_links','playerbots_account_keys','playerbots_account_type');"
echo "-- how many random-bot accounts, and how they are typed --"
qt "SELECT account_type, COUNT(*) FROM acore_playerbots.playerbots_account_type GROUP BY account_type;"
echo "-- any account links at all? (this is the friends/family route) --"
q "SELECT COUNT(*) FROM acore_playerbots.playerbots_account_links;"
echo "-- accounts whose username starts with the shipped prefix --"
q "SELECT COUNT(*) FROM acore_auth.account WHERE username LIKE 'RNDBOT%';"

sect "done"
} 2>&1 | tee "$OUT/03-conf.log"
say "step 03 done -- the shipped conf is recorded."
