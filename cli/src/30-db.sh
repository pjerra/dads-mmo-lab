# ---------------------------------------------------------------------------
# MySQL access to the AzerothCore DBs via the ac-database container.
# Search/dashboard queries here are read-only; most mutations go through
# SOAP, never a direct write. SIX direct MySQL writes are sanctioned
# project-wide (see 60-backup.sh header for the full list): the pre-existing
# LAN toggle's realmlist UPDATE (90-main.sh `lan`), backup restore,
# the characters.position_x/y/z/map/orientation UPDATE via
# _chars_write_stmt below -- OFFLINE characters only, two callers with
# identical mechanics: the `teleport-coords` arm and `gm return-home`'s
# offline faction-capital path in 90-main.sh -- module repair's INSERT/DELETE on
# the `updates` tracking tables ONLY (never game tables), (Batch 3
# F13b) `module fixit battlepass-npc`'s fixed-literal INSERTs of
# creature_template/creature entry 90100 (idempotence-checked, no user
# input in the statements), and (Batch 2 overnight) `module place-npc`'s
# fixed-literal `creature` spawn INSERTs for allowlisted NPC mods -- coords
# parsed from the 47-commands.sh cheat-sheet + re-validated numeric, entry
# from a closed key allowlist, per-map idempotence-checked -- all via the
# generalized _db_write_stmt below, see the `module repair` / `module
# fixit` / `module place-npc` arms in 90-main.sh.
# ---------------------------------------------------------------------------
_db_pw() { echo "${DML_DB_ROOT_PASSWORD:-password}"; }

_db_query() {  # _db_query <schema> <sql>
    docker exec -i ac-database mysql -N -B -uroot -p"$(_db_pw)" "$1" -e "$2" 2>/dev/null
}
db_world_query() { _db_query acore_world "$1"; }
db_chars_query() { _db_query acore_characters "$1"; }
db_auth_query() { _db_query acore_auth "$1"; }

# Direct MySQL write helper, generalized from the original characters-only
# version -- mirrors the pre-existing LAN toggle's _lan_sql docker-exec
# invocation (90-main.sh `lan`), using the fixed ac-database container name
# like the read helpers above (not a docker-compose-resolved id, since
# callers of this helper don't have a compose context). <acore_db> is
# checked against the three acore schema names as defense-in-depth --
# callers (90-main.sh `teleport-coords` and `module repair`) already
# validate against a closed set before reaching here, but the helper never
# trusts that alone.
_db_write_stmt() {  # _db_write_stmt <acore_db> <stmt>
    case "$1" in
        acore_world|acore_characters|acore_auth) ;;
        *) return 1 ;;
    esac
    docker exec ac-database mysql -uroot -p"$(_db_pw)" "$1" -e "$2" 2>/dev/null
}

# Thin wrapper -- behavior identical to the original characters-only helper
# (no callers changed). Used by `teleport-coords` (offline characters only)
# -- see this file's header comment for the full sanctioned-write list.
_chars_write_stmt() {
    _db_write_stmt acore_characters "$1"
}

sql_escape() {
    local s="${1-}"
    s=${s//\\/\\\\}
    s=${s//\'/\\\'}
    printf '%s' "$s"
}

# ---------------------------------------------------------------------------
# Bot-vs-human account identity -- the ONE place that answers "is this
# characters.account a playerbot?". Mirrored in Rust by crates/dml-wow/src/
# botid.rs (see that file's header for the full incident write-up).
#
# INCIDENT 2026-08-01. Every bot check used to be the single question "is the
# account in acore_playerbots.playerbots_account_type with account_type IN
# (1,2)?". mod-playerbots populates that registry itself, and on a freshly
# built install it can be COMPLETELY EMPTY while 1000 bot characters play:
# `account NOT IN (<empty set>)` is TRUE for every row, so the human filter
# failed OPEN and the launcher's Home page listed every bot as a real player
# (while `bots online`, the same predicate inverted, reported 0).
#
# The second signal does not depend on the mod writing a row: mod-playerbots
# names every account it creates <AiPlayerbot.RandomBotAccountPrefix><n>, and
# its own conf reserves the prefix ("Prefix for created bot accounts (of any
# type). Do not change this prefix while there are existing bot accounts.").
# A bot is registry-tagged OR prefix-named; the two fail independently.
# ---------------------------------------------------------------------------

# _bot_prefix: the reserved bot account-name prefix. Env override (a test seam
# and an escape hatch), else the deployed playerbots.conf, else its .dist (a
# fresh native install ships ONLY the .dist), else the shipped default. An
# EMPTY answer is refused at the end: spliced into LIKE '%' it would match
# every account and report the whole family as bots -- the mirror image of the
# bug this exists to fix.
_bot_prefix() {
    local p="" dir conf
    if [[ -n "${DML_BOT_ACCOUNT_PREFIX-}" ]]; then
        p="$DML_BOT_ACCOUNT_PREFIX"
    else
        dir="$(_wow_server_dir 2>/dev/null)" || dir=""
        if [[ -n "$dir" ]]; then
            for conf in "$dir/env/dist/etc/modules/playerbots.conf" \
                        "$dir/env/dist/etc/modules/playerbots.conf.dist"; do
                [[ -f "$conf" ]] || continue
                # Anchor on a following `=` so only this key matches, same
                # doctrine as _bots_counts' MaxRandomBots read.
                p="$(grep -E '^[[:space:]]*AiPlayerbot\.RandomBotAccountPrefix[[:space:]]*=' "$conf" 2>/dev/null | tail -n1)" || p=""
                p="${p#*=}"
                p="${p//[[:space:]]/}"; p="${p//\"/}"; p="${p//\'/}"
                [[ -n "$p" ]] && break
            done
        fi
    fi
    p="${p//[[:space:]]/}"; p="${p//\"/}"; p="${p//\'/}"
    [[ -n "$p" ]] || p="rndbot"
    printf '%s' "$p"
    return 0
}

# _bot_prefix_like <value>: escape for a single-quoted MySQL LIKE pattern.
# Backslash FIRST (else the escapes added below get escaped in turn), then the
# two LIKE wildcards -- an unescaped `_` in a custom prefix matches any single
# character and would silently widen the bot set. The quote is DOUBLED rather
# than backslash-escaped (sql_escape's style) so the literal survives
# NO_BACKSLASH_ESCAPES, and so both surfaces emit the same bytes.
_bot_prefix_like() {
    local p="${1-}"
    p="${p//\\/\\\\}"
    p="${p//%/\\%}"
    p="${p//_/\\_}"
    p="${p//\'/\'\'}"
    printf '%s' "$p"
    return 0
}

# _bot_username_is_bot <username column>: the prefix test applied DIRECTLY to a
# username column, for queries already selecting from acore_auth.account (the
# `accounts` picker). UPPER() on both sides because the two disagree on case by
# nature: AzerothCore stores account names upper-cased (RNDBOT0) while the conf
# value naming them is lower-case ("rndbot") -- a bare LIKE 'rndbot%' works only
# under a case-insensitive collation. The single definition of the pattern;
# _bot_account_where's subselect form wraps THIS.
_bot_username_is_bot() {
    local col="${1:?column required}" p
    p="$(_bot_prefix)"; p="${p^^}"
    p="$(_bot_prefix_like "$p")"
    printf "UPPER(%s) LIKE '%s%%'" "$col" "$p"
    return 0
}

# _bot_account_where <column>: "<column> is a bot account", parenthesised so it
# can be AND-ed into a larger WHERE without re-associating.
_bot_account_where() {
    local col="${1:?column required}"
    printf "(%s IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2)) OR %s IN (SELECT id FROM acore_auth.account WHERE %s))" \
        "$col" "$col" "$(_bot_username_is_bot username)"
    return 0
}

# _bot_account_where_prefix_only <column>: the degraded form for callers that
# probed the playerbots schema and found it unusable. Was a constant `0=1`
# ("this box has no bots"), which is a lie on any box that has them.
_bot_account_where_prefix_only() {
    local col="${1:?column required}"
    printf "(%s IN (SELECT id FROM acore_auth.account WHERE %s))" \
        "$col" "$(_bot_username_is_bot username)"
    return 0
}

# _human_account_where <column>: the exact negation of _bot_account_where, so
# the two can never drift and double-count or double-drop a character.
_human_account_where() {
    printf 'NOT %s' "$(_bot_account_where "${1:?column required}")"
    return 0
}

# All args required; "-" means "omit this filter".
build_item_search_sql() {
    local name="$1" quality="$2" minl="$3" maxl="$4" limit="$5"
    local where="1=1"
    [[ -n "$name" ]] && where+=" AND name LIKE '%$(sql_escape "$name")%'"
    [[ "$quality" != "-" ]] && where+=" AND Quality = $quality"
    [[ "$minl" != "-" ]] && where+=" AND RequiredLevel >= $minl"
    [[ "$maxl" != "-" ]] && where+=" AND RequiredLevel <= $maxl"
    printf 'SELECT entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid FROM item_template WHERE %s ORDER BY RequiredLevel,name LIMIT %s;' "$where" "$limit"
}

# Reads TSV rows on stdin, emits a JSON array of item objects.
#
# NB: callers feed this via `rows="$(db_world_query "$sql")"` followed by
# `printf '%s' "$rows" | _items_rows_to_json` (see 90-main.sh). Command
# substitution unconditionally strips ALL trailing newlines from $rows, so
# the final TSV row always reaches this function's stdin WITHOUT a trailing
# newline. A plain `while read -r ...; do ...; done` silently drops that
# final line: when `read` hits EOF without a newline terminator it still
# populates the fields but returns non-zero, and the bare `while` treats
# that non-zero as "stop, don't run the body" -- discarding an already-read
# row. Verified empirically (bash 5.3): without the `|| [[ -n "$entry" ]]`
# guard below, a 2-row stream loses its last row every time. The guard is
# safe (confirmed no infinite loop / no duplicate row): once the stream is
# truly exhausted, the next `read` attempt fails immediately without
# touching $entry, so the immediately-following iteration also sees `read`
# fail but nothing further to emit and the loop terminates cleanly. Without
# this fix, `dml wow items search` would always silently truncate the last
# result row.
_items_rows_to_json() {
    local first=1 out='['
    local entry name q il rl cls sub inv disp
    while IFS=$'\t' read -r entry name q il rl cls sub inv disp || [[ -n "$entry" ]]; do
        [[ -z "$entry" ]] && continue
        [[ $first -eq 0 ]] && out+=','
        out+="{\"entry\":$entry,\"name\":\"$(json_escape "$name")\",\"quality\":$q,\"item_level\":$il,\"required_level\":$rl,\"class\":$cls,\"subclass\":$sub,\"inventory_type\":$inv,\"displayid\":$disp}"
        first=0
    done
    out+=']'
    printf '%s' "$out"
}

# Reads TSV rows (account_id, username, gm_level, guid, char_name, level)
# sorted by account_id, emits a JSON array of account objects with nested
# characters. LEFT JOIN misses arrive as empty guid/name/level fields;
# gm_level is COALESCE'd to 0 in the SQL already but is re-validated here
# (falls back to 0 if somehow non-numeric) before it's interpolated
# unquoted into JSON. Same last-row guard as _items_rows_to_json (see the
# long comment there).
_accounts_rows_to_json() {
    local out='[' first=1 cur_id="" cur_name="" cur_gm=0 chars="" cfirst=1
    local aid uname gmlvl guid cname clvl
    while IFS=$'\t' read -r aid uname gmlvl guid cname clvl || [[ -n "$aid" ]]; do
        [[ -z "$aid" ]] && continue
        if [[ "$aid" != "$cur_id" ]]; then
            if [[ -n "$cur_id" ]]; then
                [[ $first -eq 0 ]] && out+=','
                out+="{\"id\":$cur_id,\"username\":\"$(json_escape "$cur_name")\",\"gm_level\":$cur_gm,\"characters\":[$chars]}"
                first=0
            fi
            cur_id="$aid"; cur_name="$uname"
            if [[ "$gmlvl" =~ ^[0-9]+$ ]]; then cur_gm="$gmlvl"; else cur_gm=0; fi
            chars=""; cfirst=1
        fi
        if [[ -n "$guid" ]]; then
            [[ $cfirst -eq 0 ]] && chars+=','
            chars+="{\"guid\":$guid,\"name\":\"$(json_escape "$cname")\",\"level\":$clvl}"
            cfirst=0
        fi
    done
    if [[ -n "$cur_id" ]]; then
        [[ $first -eq 0 ]] && out+=','
        out+="{\"id\":$cur_id,\"username\":\"$(json_escape "$cur_name")\",\"gm_level\":$cur_gm,\"characters\":[$chars]}"
    fi
    out+=']'
    printf '%s' "$out"
}
