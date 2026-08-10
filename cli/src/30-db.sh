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

# ---------------------------------------------------------------------------
# Schema-name resolution (Task 6, feat/core-family) -- the bash mirror of
# crates/dml-wow/src/db.rs `database_names_for_title` + config.rs
# `database_info_value`. The names used to be hardcoded acore_* literals on
# this surface while the Rust side resolved them from the server's own
# config -- the recorded bash<->Rust divergence (cli/CLAUDE.md). Closed here.
#
# Tiers, first NON-EMPTY value wins, exactly the precedence the SERVER
# applies (AC's env bridge beats conf values):
#   1. docker-compose.override.yml  .services.ac-worldserver.environment
#   2. the BASE compose file (the same four recognised names _has_compose
#      checks) -- a DML native install writes AC_*_DATABASE_INFO into the
#      BASE compose and leaves the on-disk conf stock
#   3. env/dist/etc/worldserver.conf   (*DatabaseInfo keys)
#   4. env/dist/etc/worldserver.conf.dist (a conf that omits the key is
#      LEGAL -- AC merges .conf over .dist)
# The winning value is parsed like parse_database_info (db.rs): trim, strip
# surrounding double quotes, REQUIRE >=4 semicolons (a truncated hand-edit
# must never resolve the PASSWORD as a schema), take the LAST field (the
# password may itself contain semicolons), refuse empty, refuse anything
# outside [A-Za-z0-9_$] (schema names are spliced into SQL as IDENTIFIERS,
# which MySQL cannot ?-bind, and the compose env is user-writable text --
# this charset gate is the SQL-injection choke point), refuse all-digits
# (not a legal unquoted MySQL identifier; splicing it fails as a syntax
# error that would then mislabel as "is ac-database running?").
#
# The core trio is ALL-or-nothing; playerbots resolves SEPARATELY and is
# OPTIONAL (env AC_PLAYERBOTS_DATABASE_INFO -> modules/playerbots.conf
# PlayerbotsDatabaseInfo -> its .dist -> absent) -- a server without
# mod-playerbots is legal, and absence degrades exactly like the 2026-08-01
# bot-identity incident semantics: prefix-only bot SQL, never LIKE '%',
# never 0=1. An unresolvable core trio REFUSES (DB_NAMES_UNRESOLVED via
# _db_names_require in envelope arms) -- never a guessed acore_* default,
# which would silently restore the divergence this exists to close.
#
# NO yq here (yq is a per-arm dep behind MISSING_DEP and three registry arms
# are pinned to never fork it) and NO DB probes (stats' positional stubs are
# a contract): resolution reads FILES only, via pure awk + the no-fork conf
# readers.
# ---------------------------------------------------------------------------

# Memoized once per invocation (subshell callers inherit a parent's resolved
# names; an unresolved parent state re-resolves in the subshell, which only
# costs the file reads again).
DB_NAMES_STATE=""
DB_NAME_WORLD=""; DB_NAME_CHARS=""; DB_NAME_AUTH=""; DB_NAME_PLAYERBOTS=""

# The refusal text, byte-close to the Rust twin (db.rs `DbConfig::names` /
# `db_err_to_cmd`'s NamesUnresolved arm) so the two surfaces tell one story.
DB_NAMES_ERR_MSG="could not read the schema names from the server's compose env or worldserver.conf"
DB_NAMES_ERR_HINT="Could not read the schema names from the server's compose env or worldserver.conf. Is DML_GAMES_DIR set to the directory holding your server?"

# _db_parse_database_info <raw>: echo the schema out of a
# "host;port;user;pass;dbname" value, or rc 1. Byte-for-byte the semantics of
# parse_database_info (db.rs) -- see the block comment above for each rule.
_db_parse_database_info() {
    local v="${1-}" semis last re_ident='^[A-Za-z0-9_$]+$' re_digits='^[0-9]+$'
    v="${v#"${v%%[![:space:]]*}"}"
    v="${v%"${v##*[![:space:]]}"}"
    # trim_matches('"'): ALL leading and trailing double quotes, not one pair.
    while [[ "$v" == \"* ]]; do v="${v#\"}"; done
    while [[ "$v" == *\" ]]; do v="${v%\"}"; done
    semis="${v//[!;]/}"
    (( ${#semis} >= 4 )) || return 1
    last="${v##*;}"
    last="${last#"${last%%[![:space:]]*}"}"
    last="${last%"${last##*[![:space:]]}"}"
    [[ -n "$last" ]] || return 1
    [[ "$last" =~ $re_ident ]] || return 1
    [[ "$last" =~ $re_digits ]] && return 1
    printf '%s' "$last"
    return 0
}

# _db_compose_env_get <compose-file> <KEY>: the ac-worldserver environment
# value for KEY, or nothing. Pure awk, scoped by indentation to
# services: -> ac-worldserver: -> environment: -- ac-db-import and
# ac-authserver carry the SAME AC_*_DATABASE_INFO keys and must never be the
# source. Handles BOTH compose env forms: the mapping (`KEY: "value"`) and
# the list (`- KEY=value`, split at the FIRST `=`); values may hold
# semicolons inside quotes. The key travels via the environment (K=), never
# awk -v (no escape processing). Missing file / section / key: empty, which
# the caller reads as "this tier did not answer" (same as Rust's
# parse_override_env empty-map fallback; an empty-but-SET value also falls
# through, mirroring database_info_value's non-empty filter).
_db_compose_env_get() {
    [[ -f "$1" ]] || return 0
    K="$2" awk '
        function unq(s,  c) {
            if (length(s) >= 2) {
                c = substr(s, 1, 1)
                if ((c == "\"" || c == "\047") && substr(s, length(s), 1) == c)
                    return substr(s, 2, length(s) - 2)
            }
            return s
        }
        # A scalar the way YAML reads it: quoted -> up to the closing quote;
        # bare -> up to a whitespace-preceded # comment, trailing blanks cut.
        function yval(v,  q, j) {
            sub(/^[ \t]+/, "", v); sub(/[ \t]+$/, "", v)
            q = substr(v, 1, 1)
            if (q == "\"" || q == "\047") {
                for (j = 2; j <= length(v); j++)
                    if (substr(v, j, 1) == q) return substr(v, 2, j - 2)
                return substr(v, 2)
            }
            if (match(v, /[ \t]#/)) v = substr(v, 1, RSTART - 1)
            sub(/[ \t]+$/, "", v)
            return v
        }
        {
            raw = $0; sub(/\r$/, "", raw)
            t = raw; sub(/^[ \t]+/, "", t); sub(/[ \t]+$/, "", t)
            if (t == "" || substr(t, 1, 1) == "#") next
            i = 0
            while (substr(raw, i + 1, 1) == " ") i++
            if (in_env && i <= env_i) in_env = 0
            if (in_svc && i <= svc_i) in_svc = 0
            if (in_srv && i == 0) in_srv = 0
            if (!in_srv) { if (i == 0 && t ~ /^services:/) in_srv = 1; next }
            if (!in_svc) {
                if (t ~ /^["\047]?ac-worldserver["\047]?:/) { in_svc = 1; svc_i = i }
                next
            }
            if (!in_env) {
                if (t ~ /^environment:/) { in_env = 1; env_i = i }
                next
            }
            if (substr(t, 1, 1) == "-") {
                item = yval(substr(t, 2))
                eq = index(item, "=")
                if (eq == 0) next
                if (substr(item, 1, eq - 1) == ENVIRON["K"]) { print substr(item, eq + 1); exit }
                next
            }
            c = index(t, ":")
            if (c == 0) next
            k = substr(t, 1, c - 1); sub(/[ \t]+$/, "", k); k = unq(k)
            if (k != ENVIRON["K"]) next
            print yval(substr(t, c + 1)); exit
        }
    ' "$1" 2>/dev/null
    return 0
}

# _db_info_schema <sdir> <override> <base> <conf-file> <ConfKey>: one name
# through the full tier chain, echoed, or rc 1. The env name derives from the
# conf key via the same camelCase split the AC env bridge uses
# (_cfg_env_name_for_var: LoginDatabaseInfo -> AC_LOGIN_DATABASE_INFO). Conf
# tiers ride the no-fork cached reader (read-only path -- resolution never
# mutates the conf).
_db_info_schema() {
    local sdir="$1" ovr="$2" base="$3" cfile="$4" ckey="$5" raw="" ename cpath f
    _cfg_env_name_for_var "$ckey"; ename="$REPLY"
    for f in "$ovr" "$base"; do
        [[ -n "$f" ]] || continue
        raw="$(_db_compose_env_get "$f" "$ename")"
        [[ -n "$raw" ]] && break
    done
    if [[ -z "$raw" ]]; then
        case "$cfile" in
            worldserver.conf|authserver.conf) cpath="$sdir/env/dist/etc/$cfile" ;;
            *) cpath="$sdir/env/dist/etc/modules/$cfile" ;;
        esac
        _cfg_conf_get_var "$cpath" "$ckey"; raw="$REPLY"
        if [[ -z "$raw" ]]; then
            _cfg_conf_get_var "$cpath.dist" "$ckey"; raw="$REPLY"
        fi
    fi
    [[ -n "$raw" ]] || return 1
    _db_parse_database_info "$raw"
}

# _db_names_resolve: resolve (memoized) into DB_NAME_WORLD/_CHARS/_AUTH and
# the optional DB_NAME_PLAYERBOTS (empty = no playerbots schema configured).
# rc 0 = resolved, rc 1 = refused -- NEVER a partial answer for the core trio
# and never a default. A playerbots value that is present but unparseable
# resolves ABSENT (the server could not open a pool from it either).
_db_names_resolve() {
    case "${DB_NAMES_STATE:-}" in
        ok) return 0 ;;
        fail) return 1 ;;
    esac
    DB_NAMES_STATE=fail
    local sdir ovr base f a w c p
    sdir="$(_wow_server_dir 2>/dev/null)" || sdir=""
    [[ -n "$sdir" ]] || return 1
    ovr="$sdir/docker-compose.override.yml"
    base=""
    for f in docker-compose.yml docker-compose.yaml compose.yml compose.yaml; do
        [[ -f "$sdir/$f" ]] && { base="$sdir/$f"; break; }
    done
    a="$(_db_info_schema "$sdir" "$ovr" "$base" worldserver.conf LoginDatabaseInfo)" || return 1
    w="$(_db_info_schema "$sdir" "$ovr" "$base" worldserver.conf WorldDatabaseInfo)" || return 1
    c="$(_db_info_schema "$sdir" "$ovr" "$base" worldserver.conf CharacterDatabaseInfo)" || return 1
    p="$(_db_info_schema "$sdir" "$ovr" "$base" playerbots.conf PlayerbotsDatabaseInfo)" || p=""
    DB_NAME_AUTH="$a"; DB_NAME_WORLD="$w"; DB_NAME_CHARS="$c"; DB_NAME_PLAYERBOTS="$p"
    DB_NAMES_STATE=ok
    return 0
}

# _db_names_require: the envelope-arm preamble (house pattern: _cfg_preamble's
# yq gate). Called ONCE at the top of every DB-touching verb arm so a
# resolution failure surfaces as DB_NAMES_UNRESOLVED BEFORE any docker exec
# -- never as DB_UNREACHABLE about a server that is up and healthy. The
# emission cannot live inside db_*_query: those run in command substitutions,
# where an envelope on stdout would be captured, not shown.
_db_names_require() {
    _db_names_resolve && return 0
    json_err DB_NAMES_UNRESOLVED "$DB_NAMES_ERR_MSG" "$DB_NAMES_ERR_HINT"
    exit 1
}

# _db_names_require_stream <section>: the streaming-arm sibling
# (section_end + error event, matching every streamed refusal's shape; text
# mode gets the stderr line the streamed arms use).
_db_names_require_stream() {
    _db_names_resolve && return 0
    if [[ "$DML_JSON" == 1 ]]; then
        ndjson_section_end "$1" error
        ndjson_error DB_NAMES_UNRESOLVED "$DB_NAMES_ERR_MSG" "$DB_NAMES_ERR_HINT"
    else
        echo "[dml] ERROR: $DB_NAMES_ERR_MSG" >&2
    fi
    exit 1
}

_db_query() {  # _db_query <schema> <sql>
    docker exec -i ac-database mysql -N -B -uroot -p"$(_db_pw)" "$1" -e "$2" 2>/dev/null
}
# The read wrappers resolve rather than hardcode (Task 6). A best-effort
# caller (config list's motd read, server-detail's _bots_counts, the backup
# summary sidecar) that reaches these with unresolved names just gets rc 1
# and keeps its existing degrade; envelope arms called _db_names_require
# first, so this rc-1 path never mislabels a resolution failure.
db_world_query() { _db_names_resolve || return 1; _db_query "$DB_NAME_WORLD" "$1"; }
db_chars_query() { _db_names_resolve || return 1; _db_query "$DB_NAME_CHARS" "$1"; }
db_auth_query() { _db_names_resolve || return 1; _db_query "$DB_NAME_AUTH" "$1"; }

# Direct MySQL write helper, generalized from the original characters-only
# version -- mirrors the pre-existing LAN toggle's _lan_sql docker-exec
# invocation (90-main.sh `lan`), using the fixed ac-database container name
# like the read helpers above (not a docker-compose-resolved id, since
# callers of this helper don't have a compose context). <db> is checked
# against the RESOLVED core trio as defense-in-depth -- callers (90-main.sh
# `teleport-coords` and `module repair`) already validate against a closed
# set before reaching here, but the helper never trusts that alone.
#
# Task 6: the allowlist is RE-ANCHORED on the resolved names -- still a
# closed set of exactly three, still refusing everything else (the standing
# order: never widened), PLUS the identifier-charset re-check as a second
# belt on the splice. Callers passing hardcoded acore_* literals (the module
# subsystem's recorded exception) are refused here on a renamed server --
# the honest outcome, since their SQL payloads hardcode the standard names
# internally anyway.
_db_write_stmt() {  # _db_write_stmt <db> <stmt>
    local re_ident='^[A-Za-z0-9_$]+$'
    _db_names_resolve || return 1
    case "$1" in
        "$DB_NAME_WORLD"|"$DB_NAME_CHARS"|"$DB_NAME_AUTH") ;;
        *) return 1 ;;
    esac
    [[ "$1" =~ $re_ident ]] || return 1
    docker exec ac-database mysql -uroot -p"$(_db_pw)" "$1" -e "$2" 2>/dev/null
}

# Thin wrapper -- behavior identical to the original characters-only helper
# (no callers changed; the schema is the RESOLVED characters name). Used by
# `teleport-coords` (offline characters only) -- see this file's header
# comment for the full sanctioned-write list.
_chars_write_stmt() {
    _db_names_resolve || return 1
    _db_write_stmt "$DB_NAME_CHARS" "$1"
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
# can be AND-ed into a larger WHERE without re-associating. Schema names are
# RESOLVED (Task 6); a server whose config names no playerbots schema gets the
# prefix-only form -- the same either-signal-never-LIKE-'%' incident semantics,
# just decided from the config instead of a failed probe. rc 1 (nothing
# printed) when the core names never resolved -- envelope arms refuse via
# _db_names_require before ever building SQL, so that path only reaches
# best-effort callers, whose queries then fail closed.
_bot_account_where() {
    local col="${1:?column required}"
    _db_names_resolve || return 1
    if [[ -z "$DB_NAME_PLAYERBOTS" ]]; then
        _bot_account_where_prefix_only "$col"
        return 0
    fi
    # account_type 3 = mod-city-bots citizens (CITIZEN_ACCOUNT_TYPE) -- their
    # `citybot*` accounts never match the rndbot prefix arm, so the registry is
    # their only signal (verified live 2026-08-10: 400 type-3 rows counted as
    # family). Keep in sync with Rust botid.rs::registry_clause.
    printf "(%s IN (SELECT account_id FROM %s.playerbots_account_type WHERE account_type IN (1,2,3)) OR %s IN (SELECT id FROM %s.account WHERE %s))" \
        "$col" "$DB_NAME_PLAYERBOTS" "$col" "$DB_NAME_AUTH" "$(_bot_username_is_bot username)"
    return 0
}

# _bot_account_where_prefix_only <column>: the degraded form for callers that
# probed the playerbots schema and found it unusable (or resolved no
# playerbots schema at all). Was a constant `0=1` ("this box has no bots"),
# which is a lie on any box that has them.
_bot_account_where_prefix_only() {
    local col="${1:?column required}"
    _db_names_resolve || return 1
    printf "(%s IN (SELECT id FROM %s.account WHERE %s))" \
        "$col" "$DB_NAME_AUTH" "$(_bot_username_is_bot username)"
    return 0
}

# _human_account_where <column>: the exact negation of _bot_account_where, so
# the two can never drift and double-count or double-drop a character.
# Propagates _bot_account_where's resolution refusal instead of emitting a
# bare "NOT " fragment.
_human_account_where() {
    local w
    w="$(_bot_account_where "${1:?column required}")" || return 1
    printf 'NOT %s' "$w"
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
