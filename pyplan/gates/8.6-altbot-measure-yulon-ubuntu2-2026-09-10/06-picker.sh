#!/usr/bin/env bash
# Step 06 -- what a picker can read, and which characters the module would
# accept for a given master under the conf THIS box ships. Read-only SQL.
#
# The predicate is transcribed from src/Bot/PlayerbotMgr.cpp:100-116 with the
# shipped values (AllowAccountBots=1, AllowGuildBots=1, AllowTrustedAccountBots=1):
#     offline                                     (:683 short-circuits an online one)
#   AND ( account = master.account                (sameAccount)
#       OR guild   = master.guild                 (sameGuild)
#       OR in the addclass pool                   (addClassBot)
#       OR linked to master.account )             (linkedAccount)
#   AND guid <> master.guid                       (:1315)
source "$HOME/t26-out/lib.sh"

say "step 06, reading the columns a picker would use and computing, from the shipped conf, which characters the module would accept for each master. Read-only SQL."
{
hdr "T26 measure, step 06: the picker's columns and its filter"

sect "1. the columns the picker needs, and where each one lives"
echo "-- acore_characters.characters: name, account, level, class, race, gender, online --"
qt "SELECT COLUMN_NAME, COLUMN_TYPE FROM information_schema.columns WHERE TABLE_SCHEMA='acore_characters' AND TABLE_NAME='characters' AND COLUMN_NAME IN ('guid','account','name','race','class','gender','level','online') ORDER BY ORDINAL_POSITION;"
echo "-- there is NO guild column on characters; guild is its own table --"
qt "SELECT COLUMN_NAME FROM information_schema.columns WHERE TABLE_SCHEMA='acore_characters' AND TABLE_NAME='characters' AND COLUMN_NAME LIKE '%guild%';"
qt "SELECT COLUMN_NAME, COLUMN_TYPE FROM information_schema.columns WHERE TABLE_SCHEMA='acore_characters' AND TABLE_NAME='guild_member' ORDER BY ORDINAL_POSITION;"
echo "-- the account NAME is in the other database --"
qt "SELECT COLUMN_NAME, COLUMN_TYPE FROM information_schema.columns WHERE TABLE_SCHEMA='acore_auth' AND TABLE_NAME='account' AND COLUMN_NAME IN ('id','username') ORDER BY ORDINAL_POSITION;"
echo "-- and the two rows only the playerbots database can answer --"
qt "SELECT COLUMN_NAME, COLUMN_TYPE FROM information_schema.columns WHERE TABLE_SCHEMA='acore_playerbots' AND TABLE_NAME IN ('playerbots_account_links','playerbots_account_type') ORDER BY TABLE_NAME, ORDINAL_POSITION;"

sect "2. one row of what the picker would show, for the five names T13 used"
qt "SELECT c.guid, c.name, c.account, a.username AS account_name, c.level, c.class, c.race, c.online, IFNULL(gm.guildid,0) AS guildid, IFNULL(g.name,'') AS guild_name FROM acore_characters.characters c JOIN acore_auth.account a ON a.id=c.account LEFT JOIN acore_characters.guild_member gm ON gm.guid=c.guid LEFT JOIN acore_characters.guild g ON g.guildid=gm.guildid WHERE c.name IN ('Jurnaar','Nore','Dalio','Grirmirn','Zarraden','Rinmu') ORDER BY c.guid;"

sect "3. the population this box actually holds"
qt "SELECT CASE WHEN t.account_type=1 THEN 'rndbot pool (type 1)' WHEN t.account_type=2 THEN 'addclass pool (type 2)' ELSE 'not a bot account' END AS pool, COUNT(*) AS characters, SUM(c.online) AS online FROM acore_characters.characters c LEFT JOIN acore_playerbots.playerbots_account_type t ON t.account_id=c.account GROUP BY pool;"

sect "4. WHAT THE MODULE WOULD ACCEPT, per master, under the shipped conf"
for m in Jurnaar Dalio; do
  echo "### master $m"
  qt "SELECT
        (SELECT COUNT(*) FROM acore_characters.characters c
          WHERE c.online=0 AND c.guid<>m.guid
            AND c.account=m.account) AS by_same_account,
        (SELECT COUNT(*) FROM acore_characters.characters c
          JOIN acore_characters.guild_member gmc ON gmc.guid=c.guid
          WHERE c.online=0 AND c.guid<>m.guid
            AND gmc.guildid=IFNULL((SELECT guildid FROM acore_characters.guild_member WHERE guid=m.guid),0)) AS by_guild,
        (SELECT COUNT(*) FROM acore_characters.characters c
          JOIN acore_playerbots.playerbots_account_type t ON t.account_id=c.account
          WHERE c.online=0 AND c.guid<>m.guid AND t.account_type=2) AS by_addclass_pool,
        (SELECT COUNT(*) FROM acore_characters.characters c
          JOIN acore_playerbots.playerbots_account_links l ON l.account_id=c.account
          WHERE c.online=0 AND c.guid<>m.guid AND l.linked_account_id=m.account) AS by_link,
        (SELECT COUNT(*) FROM acore_characters.characters c WHERE c.online=1) AS refused_already_logged_in
      FROM acore_characters.characters m WHERE m.name='$m';"
done

sect "5. and the only character on this box that all four rules would REFUSE"
echo "(read-only SQL: this row is counted, never used -- no command was sent as it or to it)"
qt "SELECT c.guid, c.name, c.account, c.level, c.online FROM acore_characters.characters c
    LEFT JOIN acore_playerbots.playerbots_account_type t ON t.account_id=c.account
    WHERE c.online=0 AND t.account_id IS NULL;"

sect "6. the cap the picker has to respect"
grep -n "AiPlayerbot.MaxAddedBots" "$HOME/wowserver/env/dist/etc/modules/playerbots.conf.dist"
echo "-- and the two conf keys that change the picker's job entirely --"
grep -n "AiPlayerbot.BotAutologin\|AiPlayerbot.KeepAltsInGroup\|AiPlayerbot.GroupInvitationPermission\|AiPlayerbot.SelfBotLevel" "$HOME/wowserver/env/dist/etc/modules/playerbots.conf.dist"

sect "done"
} 2>&1 | tee "$OUT/06-picker.log"
say "step 06 done."
