# ---------------------------------------------------------------------------
# Read-only MySQL access to the AzerothCore DBs via the ac-database container.
# Search/dashboard only. Mutations go through SOAP, never direct writes.
# ---------------------------------------------------------------------------
_db_pw() { echo "${DML_DB_ROOT_PASSWORD:-password}"; }

_db_query() {  # _db_query <schema> <sql>
    docker exec -i ac-database mysql -N -B -uroot -p"$(_db_pw)" "$1" -e "$2" 2>/dev/null
}
db_world_query() { _db_query acore_world "$1"; }
db_chars_query() { _db_query acore_characters "$1"; }
db_auth_query() { _db_query acore_auth "$1"; }

sql_escape() {
    local s="${1-}"
    s=${s//\\/\\\\}
    s=${s//\'/\\\'}
    printf '%s' "$s"
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
