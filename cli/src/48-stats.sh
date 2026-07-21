# ---------------------------------------------------------------------------
# `wow stats` (Statistics page): ONE read-only envelope with every number the
# page needs, so a page load is a single dml invocation. All queries go
# through the read-only db helpers in 30-db.sh -- this file adds ZERO write
# paths. Bot detection reuses the authoritative playerbots_account_type
# idiom from _bots_counts (40-config.sh); "family" is every non-bot account
# EXCLUDING the AHBOT and DMLSOAP system accounts.
#
# The query order is FIXED -- tests stub the mysql calls positionally via
# DML_STUB_DB_ROWS_SEQ, so reordering or inserting a query is a test change:
#   1  population overview (chars)   2  level buckets (chars)
#   3  class breakdown (chars)       4  faction split (chars)
#   5  top-5 levels (chars)          6  guild count+members (chars)
#   7  copper totals (chars)         8  top-5 richest (chars)
#   9  auction house (chars)         10 mail counts (chars)
#   11 family journey rows (chars)   12 uptime aggregates (auth)
#   13 realm name (auth)             14 last-15 boots (auth)
#   15 online-bot zones (chars)      16 online-bot continents (chars)
# ---------------------------------------------------------------------------

# Echo a JSON-safe non-negative integer: the 10#-normalized value when the
# input is purely digits, else 0. EVERY interpolated number in the stats
# envelope funnels through this -- a NULL/empty/garbage field from mysql
# must degrade to 0, never emit invalid JSON, and 10# strips leading zeros
# (e.g. "007" -> 7, since "count":007 is also invalid JSON). Same idiom as
# the players-online fix (90-main.sh `players`) and _bots_counts.
_stats_num() {
    local v="${1-}"
    if [[ "$v" =~ ^[0-9]+$ ]]; then printf '%s' "$((10#$v))"; else printf '0'; fi
    return 0
}

# Echo a JSON boolean from a 0/1 SQL CASE field; anything but "1" is false.
_stats_bool() {
    if [[ "${1-}" == "1" ]]; then printf 'true'; else printf 'false'; fi
    return 0
}

# Assembles the full stats payload (the `data` object) on stdout. Returns 1
# without partial output when any query fails -- the caller maps that to one
# DB_UNREACHABLE envelope. Empty result sets are NOT failures: they emit
# zeros / empty arrays (a freshly-installed empty DB is a valid state).
_stats_payload() {
    local bot sys fam rows
    # Bot accounts: the authoritative cross-schema idiom (account_type 1 =
    # random bot, 2 = ahbot-style system bot pool) used by _bots_counts,
    # `players online` and the backup summary.
    bot="c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))"
    # System accounts that are neither family nor ambient bots: the auction
    # house seeder and the launcher's own SOAP account.
    sys="c.account IN (SELECT id FROM acore_auth.account WHERE username IN ('AHBOT','DMLSOAP'))"
    fam="NOT ($bot) AND NOT ($sys)"

    # -- 1: population overview: one full-table pass, five numbers ----------
    local fam_total fam_online bot_total bot_online bot_playtime
    rows="$(db_chars_query "SELECT
        COALESCE(SUM(CASE WHEN $fam THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN ($fam) AND c.online=1 THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN $bot THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN ($bot) AND c.online=1 THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN $bot THEN c.totaltime ELSE 0 END),0)
      FROM characters c;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r fam_total fam_online bot_total bot_online bot_playtime <<< "$rows" || true
    fam_total="$(_stats_num "$fam_total")"; fam_online="$(_stats_num "$fam_online")"
    bot_total="$(_stats_num "$bot_total")"; bot_online="$(_stats_num "$bot_online")"
    bot_playtime="$(_stats_num "$bot_playtime")"

    # -- 2: level spread bucketed by 10 (bucket 0 = levels 1-10 ... 7 = 71-80),
    # family/bot split per bucket so the chart can stack the two series.
    local levels='[' lfirst=1 b fcnt bcnt
    rows="$(db_chars_query "SELECT FLOOR((c.level-1)/10),
        COALESCE(SUM(CASE WHEN $fam THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN $bot THEN 1 ELSE 0 END),0)
      FROM characters c GROUP BY 1 ORDER BY 1;")" || return 1
    while IFS=$'\t' read -r b fcnt bcnt || [[ -n "$b" ]]; do
        [[ -z "$b" ]] && continue
        # The bucket id keys the row -- a non-numeric one has no meaning, so
        # the row is dropped rather than remapped to bucket 0.
        [[ "$b" =~ ^[0-9]+$ ]] || continue
        [[ $lfirst -eq 0 ]] && levels+=','
        levels+="{\"bucket\":$((10#$b)),\"family\":$(_stats_num "$fcnt"),\"bots\":$(_stats_num "$bcnt")}"
        lfirst=0
    done <<< "$rows"
    levels+=']'

    # -- 3: class breakdown (all non-system characters) ---------------------
    local classes='[' cfirst=1 cls cnt
    rows="$(db_chars_query "SELECT c.class, COUNT(*) FROM characters c WHERE NOT ($sys) GROUP BY c.class ORDER BY c.class;")" || return 1
    while IFS=$'\t' read -r cls cnt || [[ -n "$cls" ]]; do
        [[ -z "$cls" ]] && continue
        [[ "$cls" =~ ^[0-9]+$ ]] || continue
        [[ $cfirst -eq 0 ]] && classes+=','
        classes+="{\"class\":$((10#$cls)),\"count\":$(_stats_num "$cnt")}"
        cfirst=0
    done <<< "$rows"
    classes+=']'

    # -- 4: faction split via the race -> faction mapping -------------------
    local alliance horde
    rows="$(db_chars_query "SELECT
        COALESCE(SUM(CASE WHEN c.race IN (1,3,4,7,11) THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN c.race IN (2,5,6,8,10) THEN 1 ELSE 0 END),0)
      FROM characters c WHERE NOT ($sys);")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r alliance horde <<< "$rows" || true
    alliance="$(_stats_num "$alliance")"; horde="$(_stats_num "$horde")"

    # -- 5: top-5 highest-level characters (family flagged) -----------------
    local tops='[' tfirst=1 name lvl isfam
    rows="$(db_chars_query "SELECT c.name, c.level, CASE WHEN $fam THEN 1 ELSE 0 END
      FROM characters c WHERE NOT ($sys)
      ORDER BY c.level DESC, c.totaltime DESC, c.name LIMIT 5;")" || return 1
    while IFS=$'\t' read -r name lvl isfam || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $tfirst -eq 0 ]] && tops+=','
        tops+="{\"name\":\"$(json_escape "$name")\",\"level\":$(_stats_num "$lvl"),\"family\":$(_stats_bool "$isfam")}"
        tfirst=0
    done <<< "$rows"
    tops+=']'

    # -- 6: guild count + member total (avg size is client-side math) ------
    local guilds members
    rows="$(db_chars_query "SELECT (SELECT COUNT(*) FROM guild), (SELECT COUNT(*) FROM guild_member);")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r guilds members <<< "$rows" || true
    guilds="$(_stats_num "$guilds")"; members="$(_stats_num "$members")"

    # -- 7: copper totals (money is COPPER -- the client divides by 10000).
    # "total" spans family+bots only (system accounts excluded), so the
    # three numbers always add up on screen.
    local cop_total cop_fam cop_bot
    rows="$(db_chars_query "SELECT
        COALESCE(SUM(CASE WHEN NOT ($sys) THEN c.money ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN $fam THEN c.money ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN $bot THEN c.money ELSE 0 END),0)
      FROM characters c;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r cop_total cop_fam cop_bot <<< "$rows" || true
    cop_total="$(_stats_num "$cop_total")"; cop_fam="$(_stats_num "$cop_fam")"; cop_bot="$(_stats_num "$cop_bot")"

    # -- 8: top-5 richest characters (family flagged) -----------------------
    local rich='[' rfirst=1 money
    rows="$(db_chars_query "SELECT c.name, c.money, CASE WHEN $fam THEN 1 ELSE 0 END
      FROM characters c WHERE NOT ($sys)
      ORDER BY c.money DESC, c.name LIMIT 5;")" || return 1
    while IFS=$'\t' read -r name money isfam || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $rfirst -eq 0 ]] && rich+=','
        rich+="{\"name\":\"$(json_escape "$name")\",\"copper\":$(_stats_num "$money"),\"family\":$(_stats_bool "$isfam")}"
        rfirst=0
    done <<< "$rows"
    rich+=']'

    # -- 9: auction house stock. On this server the AH is 100% the ahbot's
    # shop -- the page labels it "auction house shop stock", never implying
    # player listings.
    local ah_count ah_buyout
    rows="$(db_chars_query "SELECT COUNT(*), COALESCE(SUM(buyoutprice),0) FROM auctionhouse;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r ah_count ah_buyout <<< "$rows" || true
    ah_count="$(_stats_num "$ah_count")"; ah_buyout="$(_stats_num "$ah_buyout")"

    # -- 10: pending mail (+ how much of it is addressed to the family) -----
    local mail_total mail_fam
    rows="$(db_chars_query "SELECT COUNT(*),
        COALESCE(SUM(CASE WHEN m.receiver IN (SELECT c.guid FROM characters c WHERE $fam) THEN 1 ELSE 0 END),0)
      FROM mail m;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r mail_total mail_fam <<< "$rows" || true
    mail_total="$(_stats_num "$mail_total")"; mail_fam="$(_stats_num "$mail_fam")"

    # -- 11: the family's journey -- one row per family character. The two
    # correlated COUNTs are per-guid index lookups (guid is the PK prefix on
    # both side tables); with a handful of family characters this stays
    # sub-second (validated live).
    local journey='[' jfirst=1 jlvl jcls jtime jseen jkills jach jquests
    rows="$(db_chars_query "SELECT c.name, c.level, c.class, c.totaltime, c.logout_time, c.totalKills,
        (SELECT COUNT(*) FROM character_achievement a WHERE a.guid = c.guid),
        (SELECT COUNT(*) FROM character_queststatus_rewarded q WHERE q.guid = c.guid)
      FROM characters c WHERE $fam
      ORDER BY c.totaltime DESC, c.name;")" || return 1
    while IFS=$'\t' read -r name jlvl jcls jtime jseen jkills jach jquests || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $jfirst -eq 0 ]] && journey+=','
        journey+="{\"name\":\"$(json_escape "$name")\",\"level\":$(_stats_num "$jlvl"),\"class\":$(_stats_num "$jcls"),\"playtime\":$(_stats_num "$jtime"),\"last_seen\":$(_stats_num "$jseen"),\"kills\":$(_stats_num "$jkills"),\"achievements\":$(_stats_num "$jach"),\"quests\":$(_stats_num "$jquests")}"
        jfirst=0
    done <<< "$rows"
    journey+=']'

    # -- 12: server history aggregates (acore_auth.uptime) ------------------
    local boots up_total up_longest up_peak
    rows="$(db_auth_query "SELECT COUNT(*), COALESCE(SUM(uptime),0), COALESCE(MAX(uptime),0), COALESCE(MAX(maxplayers),0) FROM uptime;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r boots up_total up_longest up_peak <<< "$rows" || true
    boots="$(_stats_num "$boots")"; up_total="$(_stats_num "$up_total")"
    up_longest="$(_stats_num "$up_longest")"; up_peak="$(_stats_num "$up_peak")"

    # -- 13: realm name -----------------------------------------------------
    local realm
    realm="$(db_auth_query "SELECT name FROM realmlist ORDER BY id LIMIT 1;")" || return 1
    realm="${realm%%$'\n'*}"

    # -- 14: the last 15 boots, oldest first (per-boot chart data) ----------
    local recent='[' refirst=1 rstart rup
    rows="$(db_auth_query "SELECT starttime, uptime FROM (SELECT starttime, uptime FROM uptime ORDER BY starttime DESC LIMIT 15) t ORDER BY starttime;")" || return 1
    while IFS=$'\t' read -r rstart rup || [[ -n "$rstart" ]]; do
        [[ -z "$rstart" ]] && continue
        [[ "$rstart" =~ ^[0-9]+$ ]] || continue
        [[ $refirst -eq 0 ]] && recent+=','
        recent+="{\"start\":$((10#$rstart)),\"uptime\":$(_stats_num "$rup")}"
        refirst=0
    done <<< "$rows"
    recent+=']'

    # -- 15: top-8 busiest zones for ONLINE bots (ids -> names client-side) -
    local zones='[' zfirst=1 zone zcnt
    rows="$(db_chars_query "SELECT c.zone, COUNT(*) FROM characters c WHERE c.online = 1 AND ($bot) GROUP BY c.zone ORDER BY COUNT(*) DESC, c.zone LIMIT 8;")" || return 1
    while IFS=$'\t' read -r zone zcnt || [[ -n "$zone" ]]; do
        [[ -z "$zone" ]] && continue
        [[ "$zone" =~ ^[0-9]+$ ]] || continue
        [[ $zfirst -eq 0 ]] && zones+=','
        zones+="{\"zone\":$((10#$zone)),\"count\":$(_stats_num "$zcnt")}"
        zfirst=0
    done <<< "$rows"
    zones+=']'

    # -- 16: online bots per continent (map ids -> names client-side) -------
    local conts='[' cofirst=1 cmap ccnt
    rows="$(db_chars_query "SELECT c.map, COUNT(*) FROM characters c WHERE c.online = 1 AND ($bot) GROUP BY c.map ORDER BY COUNT(*) DESC, c.map;")" || return 1
    while IFS=$'\t' read -r cmap ccnt || [[ -n "$cmap" ]]; do
        [[ -z "$cmap" ]] && continue
        [[ "$cmap" =~ ^[0-9]+$ ]] || continue
        [[ $cofirst -eq 0 ]] && conts+=','
        conts+="{\"map\":$((10#$cmap)),\"count\":$(_stats_num "$ccnt")}"
        cofirst=0
    done <<< "$rows"
    conts+=']'

    printf '%s' "{\"population\":{\"family\":{\"total\":$fam_total,\"online\":$fam_online},\"bots\":{\"total\":$bot_total,\"online\":$bot_online},\"levels\":$levels,\"classes\":$classes,\"factions\":{\"alliance\":$alliance,\"horde\":$horde},\"top_levels\":$tops,\"guilds\":{\"count\":$guilds,\"members\":$members}},\"economy\":{\"copper\":{\"total\":$cop_total,\"family\":$cop_fam,\"bots\":$cop_bot},\"richest\":$rich,\"auction\":{\"count\":$ah_count,\"buyout\":$ah_buyout},\"mail\":{\"total\":$mail_total,\"to_family\":$mail_fam}},\"journey\":$journey,\"history\":{\"boots\":$boots,\"total_uptime\":$up_total,\"longest\":$up_longest,\"peak\":$up_peak,\"realm\":\"$(json_escape "$realm")\",\"recent\":$recent},\"botwatch\":{\"zones\":$zones,\"continents\":$conts,\"playtime\":$bot_playtime}}"
    return 0
}
