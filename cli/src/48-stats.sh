# ---------------------------------------------------------------------------
# `wow stats` (Statistics page): ONE read-only envelope with every number the
# page needs, so a page load is a single dml invocation. All queries go
# through the read-only db helpers in 30-db.sh -- this file adds ZERO write
# paths. Bot detection reuses the authoritative playerbots_account_type
# idiom from _bots_counts (40-config.sh); "family" is every non-bot account
# EXCLUDING the AHBOT and DMLSOAP system accounts.
#
# The query order is FIXED -- tests stub the mysql calls positionally via
# DML_STUB_DB_ROWS_SEQ, so reordering or inserting a query is a test change.
# Segment-sensitive stats (classes/factions/top-levels/richest) carry
# per-segment family/bots data for the page's All|Family|Bots filter
# ("all" is a client-side merge):
#   1  playerbots-schema probe (chars; the ONLY tolerated failure -- a
#      missing acore_playerbots schema degrades every bot stat to zero
#      instead of failing the whole envelope)
#   2  population overview (chars)   3  level buckets (chars)
#   4  class breakdown f/b (chars)   5  faction split f/b (chars)
#   6  top-5 levels FAMILY (chars)   7  top-5 levels BOTS (chars)
#   8  guild count+members (chars)   9  copper totals (chars)
#   10 top-5 richest FAMILY (chars)  11 top-5 richest BOTS (chars)
#   12 auction house (chars)         13 mail counts (chars)
#   14 family journey rows (chars)   15 uptime aggregates (auth)
#   16 realm name (auth)             17 last-15 boots (auth)
#   18 online-bot zones (chars)      19 online-bot continents (chars)
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

    # -- 1: playerbots schema probe (review finding 8c). A box without the
    # acore_playerbots schema (playerbots module absent / not yet migrated)
    # used to fail EVERY $bot-referencing query and surface as one opaque
    # DB_UNREACHABLE. Probe it once; when missing, substitute a constant
    # FALSE so all bot counts degrade to zeros and the rest of the page
    # keeps working. A fully-down DB fails this probe too -- and then also
    # fails query 2, so the envelope still errors exactly as before.
    if ! db_chars_query "SELECT 1 FROM acore_playerbots.playerbots_account_type LIMIT 1;" >/dev/null 2>&1; then
        bot="0=1"
    fi
    fam="NOT ($bot) AND NOT ($sys)"

    # -- 2: population overview: one full-table pass, five numbers ----------
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

    # -- 3: level spread bucketed by 10 (bucket 0 = levels 1-10 ... 7 = 71-80),
    # family/bot split per bucket so the chart can stack the two series.
    # GREATEST(c.level,1): level is UNSIGNED, so a level-0 row (possible via
    # hand-edited DBs) would make `level-1` an out-of-range error in strict
    # mode and fail the whole envelope (review finding 8a); clamping folds
    # level 0 into bucket 0 instead.
    local levels='[' lfirst=1 b fcnt bcnt
    rows="$(db_chars_query "SELECT FLOOR((GREATEST(c.level,1)-1)/10),
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

    # -- 4: class breakdown, family/bots split per class (segment filter).
    # Zero-count entries are dropped per segment -- "all" is a client-side
    # sum of the two arrays, so nothing is lost.
    local cls_fam='[' cls_bot='[' cffirst=1 cbfirst=1 cls fcls_n bcls_n fv bv
    rows="$(db_chars_query "SELECT c.class,
        COALESCE(SUM(CASE WHEN $fam THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN $bot THEN 1 ELSE 0 END),0)
      FROM characters c WHERE NOT ($sys) GROUP BY c.class ORDER BY c.class;")" || return 1
    while IFS=$'\t' read -r cls fcls_n bcls_n || [[ -n "$cls" ]]; do
        [[ -z "$cls" ]] && continue
        [[ "$cls" =~ ^[0-9]+$ ]] || continue
        fv="$(_stats_num "$fcls_n")"; bv="$(_stats_num "$bcls_n")"
        if (( fv > 0 )); then
            [[ $cffirst -eq 0 ]] && cls_fam+=','
            cls_fam+="{\"class\":$((10#$cls)),\"count\":$fv}"; cffirst=0
        fi
        if (( bv > 0 )); then
            [[ $cbfirst -eq 0 ]] && cls_bot+=','
            cls_bot+="{\"class\":$((10#$cls)),\"count\":$bv}"; cbfirst=0
        fi
    done <<< "$rows"
    cls_fam+=']'; cls_bot+=']'

    # -- 5: faction split per segment (race -> faction mapping) -------------
    local fam_alliance fam_horde bot_alliance bot_horde
    rows="$(db_chars_query "SELECT
        COALESCE(SUM(CASE WHEN ($fam) AND c.race IN (1,3,4,7,11) THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN ($fam) AND c.race IN (2,5,6,8,10) THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN ($bot) AND c.race IN (1,3,4,7,11) THEN 1 ELSE 0 END),0),
        COALESCE(SUM(CASE WHEN ($bot) AND c.race IN (2,5,6,8,10) THEN 1 ELSE 0 END),0)
      FROM characters c;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r fam_alliance fam_horde bot_alliance bot_horde <<< "$rows" || true
    fam_alliance="$(_stats_num "$fam_alliance")"; fam_horde="$(_stats_num "$fam_horde")"
    bot_alliance="$(_stats_num "$bot_alliance")"; bot_horde="$(_stats_num "$bot_horde")"

    # -- 6+7: top-5 highest-level characters per segment. The per-row
    # `family` flag is kept (true/false by construction) so the client's
    # merged "all" view renders the family badge exactly as before.
    local tops_fam='[' tops_bot='[' tfirst=1 name lvl
    rows="$(db_chars_query "SELECT c.name, c.level
      FROM characters c WHERE $fam
      ORDER BY c.level DESC, c.totaltime DESC, c.name LIMIT 5;")" || return 1
    while IFS=$'\t' read -r name lvl || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $tfirst -eq 0 ]] && tops_fam+=','
        tops_fam+="{\"name\":\"$(json_escape "$name")\",\"level\":$(_stats_num "$lvl"),\"family\":true}"
        tfirst=0
    done <<< "$rows"
    tops_fam+=']'
    tfirst=1
    rows="$(db_chars_query "SELECT c.name, c.level
      FROM characters c WHERE $bot
      ORDER BY c.level DESC, c.totaltime DESC, c.name LIMIT 5;")" || return 1
    while IFS=$'\t' read -r name lvl || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $tfirst -eq 0 ]] && tops_bot+=','
        tops_bot+="{\"name\":\"$(json_escape "$name")\",\"level\":$(_stats_num "$lvl"),\"family\":false}"
        tfirst=0
    done <<< "$rows"
    tops_bot+=']'

    # -- 8: guild count + member total (avg size is client-side math) ------
    local guilds members
    rows="$(db_chars_query "SELECT (SELECT COUNT(*) FROM guild), (SELECT COUNT(*) FROM guild_member);")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r guilds members <<< "$rows" || true
    guilds="$(_stats_num "$guilds")"; members="$(_stats_num "$members")"

    # -- 9: copper totals (money is COPPER -- the client divides by 10000).
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

    # -- 10+11: top-5 richest characters per segment (same family-flag
    # convention as the top-levels pair above).
    local rich_fam='[' rich_bot='[' rfirst=1 money
    rows="$(db_chars_query "SELECT c.name, c.money
      FROM characters c WHERE $fam
      ORDER BY c.money DESC, c.name LIMIT 5;")" || return 1
    while IFS=$'\t' read -r name money || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $rfirst -eq 0 ]] && rich_fam+=','
        rich_fam+="{\"name\":\"$(json_escape "$name")\",\"copper\":$(_stats_num "$money"),\"family\":true}"
        rfirst=0
    done <<< "$rows"
    rich_fam+=']'
    rfirst=1
    rows="$(db_chars_query "SELECT c.name, c.money
      FROM characters c WHERE $bot
      ORDER BY c.money DESC, c.name LIMIT 5;")" || return 1
    while IFS=$'\t' read -r name money || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $rfirst -eq 0 ]] && rich_bot+=','
        rich_bot+="{\"name\":\"$(json_escape "$name")\",\"copper\":$(_stats_num "$money"),\"family\":false}"
        rfirst=0
    done <<< "$rows"
    rich_bot+=']'

    # -- 12: auction house stock. On this server the AH is 100% the ahbot's
    # shop -- the page labels it "auction house shop stock", never implying
    # player listings.
    local ah_count ah_buyout
    rows="$(db_chars_query "SELECT COUNT(*), COALESCE(SUM(buyoutprice),0) FROM auctionhouse;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r ah_count ah_buyout <<< "$rows" || true
    ah_count="$(_stats_num "$ah_count")"; ah_buyout="$(_stats_num "$ah_buyout")"

    # -- 13: pending mail (+ how much of it is addressed to the family) -----
    local mail_total mail_fam
    rows="$(db_chars_query "SELECT COUNT(*),
        COALESCE(SUM(CASE WHEN m.receiver IN (SELECT c.guid FROM characters c WHERE $fam) THEN 1 ELSE 0 END),0)
      FROM mail m;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r mail_total mail_fam <<< "$rows" || true
    mail_total="$(_stats_num "$mail_total")"; mail_fam="$(_stats_num "$mail_fam")"

    # -- 14: the family's journey -- one row per family character. The two
    # correlated COUNTs are per-guid index lookups (guid is the PK prefix on
    # both side tables); with a handful of family characters this stays
    # sub-second (validated live).
    local journey='[' jfirst=1 jlvl jcls jtime jseen jonl jkills jach jquests
    rows="$(db_chars_query "SELECT c.name, c.level, c.class, c.totaltime, c.logout_time, c.online, c.totalKills,
        (SELECT COUNT(*) FROM character_achievement a WHERE a.guid = c.guid),
        (SELECT COUNT(*) FROM character_queststatus_rewarded q WHERE q.guid = c.guid)
      FROM characters c WHERE $fam
      ORDER BY c.totaltime DESC, c.name;")" || return 1
    while IFS=$'\t' read -r name jlvl jcls jtime jseen jonl jkills jach jquests || [[ -n "$name" ]]; do
        [[ -z "$name" ]] && continue
        [[ $jfirst -eq 0 ]] && journey+=','
        # online (review finding 8b): logout_time is stale by definition for
        # a character who is logged in RIGHT NOW -- the client shows "Online
        # now" instead of the last-seen timestamp.
        journey+="{\"name\":\"$(json_escape "$name")\",\"level\":$(_stats_num "$jlvl"),\"class\":$(_stats_num "$jcls"),\"playtime\":$(_stats_num "$jtime"),\"last_seen\":$(_stats_num "$jseen"),\"online\":$(_stats_bool "$jonl"),\"kills\":$(_stats_num "$jkills"),\"achievements\":$(_stats_num "$jach"),\"quests\":$(_stats_num "$jquests")}"
        jfirst=0
    done <<< "$rows"
    journey+=']'

    # -- 15: server history aggregates (acore_auth.uptime) ------------------
    local boots up_total up_longest up_peak
    rows="$(db_auth_query "SELECT COUNT(*), COALESCE(SUM(uptime),0), COALESCE(MAX(uptime),0), COALESCE(MAX(maxplayers),0) FROM uptime;")" || return 1
    rows="${rows%%$'\n'*}"
    IFS=$'\t' read -r boots up_total up_longest up_peak <<< "$rows" || true
    boots="$(_stats_num "$boots")"; up_total="$(_stats_num "$up_total")"
    up_longest="$(_stats_num "$up_longest")"; up_peak="$(_stats_num "$up_peak")"

    # -- 16: realm name -----------------------------------------------------
    local realm
    realm="$(db_auth_query "SELECT name FROM realmlist ORDER BY id LIMIT 1;")" || return 1
    realm="${realm%%$'\n'*}"

    # -- 17: the last 15 boots, oldest first (per-boot chart data) ----------
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

    # -- 18: top-8 busiest zones for ONLINE bots (ids -> names client-side) -
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

    # -- 19: online bots per continent, bounded to the four playable maps +
    # one "other worlds" bucket (map -1; review finding 8d) -- bots parked in
    # instances/battlegrounds used to spray one chip per raw map id.
    local conts='[' cofirst=1 cmap ccnt
    rows="$(db_chars_query "SELECT CASE WHEN c.map IN (0,1,530,571) THEN c.map ELSE -1 END, COUNT(*) FROM characters c WHERE c.online = 1 AND ($bot) GROUP BY 1 ORDER BY 2 DESC, 1;")" || return 1
    while IFS=$'\t' read -r cmap ccnt || [[ -n "$cmap" ]]; do
        [[ -z "$cmap" ]] && continue
        [[ "$cmap" =~ ^-?[0-9]+$ ]] || continue
        # 10# refuses a sign, so normalize the only legal negative by hand.
        if [[ "$cmap" == -* ]]; then cmap="-1"; else cmap=$((10#$cmap)); fi
        [[ $cofirst -eq 0 ]] && conts+=','
        conts+="{\"map\":$cmap,\"count\":$(_stats_num "$ccnt")}"
        cofirst=0
    done <<< "$rows"
    conts+=']'

    printf '%s' "{\"population\":{\"family\":{\"total\":$fam_total,\"online\":$fam_online},\"bots\":{\"total\":$bot_total,\"online\":$bot_online},\"levels\":$levels,\"classes\":{\"family\":$cls_fam,\"bots\":$cls_bot},\"factions\":{\"family\":{\"alliance\":$fam_alliance,\"horde\":$fam_horde},\"bots\":{\"alliance\":$bot_alliance,\"horde\":$bot_horde}},\"top_levels\":{\"family\":$tops_fam,\"bots\":$tops_bot},\"guilds\":{\"count\":$guilds,\"members\":$members}},\"economy\":{\"copper\":{\"total\":$cop_total,\"family\":$cop_fam,\"bots\":$cop_bot},\"richest\":{\"family\":$rich_fam,\"bots\":$rich_bot},\"auction\":{\"count\":$ah_count,\"buyout\":$ah_buyout},\"mail\":{\"total\":$mail_total,\"to_family\":$mail_fam}},\"journey\":$journey,\"history\":{\"boots\":$boots,\"total_uptime\":$up_total,\"longest\":$up_longest,\"peak\":$up_peak,\"realm\":\"$(json_escape "$realm")\",\"recent\":$recent},\"botwatch\":{\"zones\":$zones,\"continents\":$conts,\"playtime\":$bot_playtime}}"
    return 0
}
