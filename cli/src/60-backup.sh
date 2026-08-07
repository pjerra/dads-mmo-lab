# ---------------------------------------------------------------------------
# Character backups (Lab-parity round 5): whole-server snapshots.
# mysqldump of the RESOLVED characters+playerbots+auth schemas (30-db.sh
# _db_names_resolve, Task 6) -> ~/.dml/backups.
# SIX DIRECT MYSQL WRITES ARE SANCTIONED PROJECT-WIDE: the pre-existing LAN
# toggle's realmlist UPDATE (90-main.sh `lan`), teleport-coords'
# characters.position_x/y/z/map/orientation UPDATE (offline characters only
# -- 90-main.sh `teleport-coords` arm, via `_chars_write_stmt` in
# 30-db.sh), module repair's INSERT/DELETE on the `updates` tracking tables
# ONLY -- never game tables (90-main.sh `module repair` arm, via the
# generalized `_db_write_stmt` in 30-db.sh; see
# docs/superpowers/specs/2026-07-18-module-repair-design.md), `module fixit
# battlepass-npc`'s fixed-literal creature_template/creature INSERTs for
# entry 90100 (Batch 3 F13b -- idempotence-checked, zero user input in the
# statements), `module place-npc`'s fixed-literal `creature` spawn INSERTs
# for allowlisted NPC mods (Batch 2 overnight -- coords from the cheat-sheet,
# per-map idempotence-checked, entry from a closed allowlist), and RESTORE
# below -- the project's one sanctioned write path for whole CHARACTER-DB
# snapshots: only inside `backup restore`, only with world+auth stopped,
# always behind an automatic pre-restore safety backup.
# See docs/.../2026-07-17-backups-design.md.
# ---------------------------------------------------------------------------

_backup_dir() { echo "$HOME/.dml/backups"; }

# Exit status IS the signal (same pattern as _valid_preset_name).
_valid_backup_name() { [[ "$1" =~ ^wow-[0-9]{8}-[0-9]{6}(-full)?(-prerestore)?\.sql\.gz$ ]]; }

# Keep the newest ${DML_BACKUP_KEEP:-10} backups (ALL files incl.
# -prerestore); delete the rest, echoing each pruned name.
_backup_prune() {
    local bdir keep n f
    bdir="$(_backup_dir)"
    keep="${DML_BACKUP_KEEP:-10}"
    [[ -d "$bdir" ]] || return 0
    n=0
    while IFS= read -r f || [[ -n "$f" ]]; do
        [[ -z "$f" ]] && continue
        n=$(( n + 1 ))
        if (( n > keep )); then
            rm -f "$bdir/$f" "$bdir/$f.meta"
            echo "$f"
        fi
    done < <(ls -1 "$bdir" 2>/dev/null | grep -E '\.sql\.gz$' | sort -r)
    return 0
}

# Dump the character DBs (plus the world schema when $2 = 1 -- used before
# module installs, which mutate world data), gzip to a tmp file, mv into
# place ($1) -- no partial files on failure. On failure: tmp removed,
# "$1.err" left with stderr (caller reads + removes it), returns 1.
#
# Task 6: the dump set is the RESOLVED schema names -- a renamed server used
# to get a dump of the WRONG (absent) acore_* schemas, the worst failure
# class this repo records, because the backup reports success and holds
# nothing. Playerbots joins the set only WHEN RESOLVED: its absence on a
# schema-less server turns what used to be a hard mysqldump failure
# ("unknown database acore_playerbots") into a correct omission -- which
# every dump surface must NARRATE (see _dump_narration below), since an
# omission a user only discovers at restore time is data loss in the one
# artifact they restore from. Unresolved core names refuse (rc 1) before
# any docker exec; streaming arms already refused DB_NAMES_UNRESOLVED via
# _db_names_require_stream first. Mirrors backup.rs mysqldump_args_for.
_backup_dump_to() {
    local out="$1" incw="${2:-0}" tmp
    _db_names_resolve || return 1
    local dbs=("$DB_NAME_CHARS")
    [[ -n "$DB_NAME_PLAYERBOTS" ]] && dbs+=("$DB_NAME_PLAYERBOTS")
    dbs+=("$DB_NAME_AUTH")
    [[ "$incw" == 1 ]] && dbs+=("$DB_NAME_WORLD")
    tmp="$out.tmp"
    if docker exec ac-database mysqldump -uroot -p"$(_db_pw)" --databases "${dbs[@]}" --single-transaction --quick 2>"$out.err" | gzip > "$tmp"; then
        mv "$tmp" "$out"
        rm -f "$out.err"
        return 0
    fi
    rm -f "$tmp"
    return 1
}

# ---------------------------------------------------------------------------
# Dump narration (Task 6) -- the copy must track the dump set: a line that
# promises bots while _backup_dump_to omits the playerbots schema is the
# silent-omission hazard named above. One helper pair so the rule is decided
# once; the bots-flush and module-backup surfaces follow the same rule with
# their own copy (mirrors backup.rs dump_narration + destructive.rs /
# modmgr.rs -- keep the strings byte-identical to the Rust twins).
# ---------------------------------------------------------------------------

# _dump_narration <include_world>: echoes `backup create`'s "backing up ..."
# copy for the current resolved names.
_dump_narration() {
    _db_names_resolve || return 1
    if [[ -n "$DB_NAME_PLAYERBOTS" ]]; then
        if [[ "${1:-0}" == 1 ]]; then printf 'backing up characters, bots, accounts and world...'
        else printf 'backing up characters, bots and accounts...'; fi
    else
        if [[ "${1:-0}" == 1 ]]; then printf 'backing up characters, accounts and world...'
        else printf 'backing up characters and accounts...'; fi
    fi
    return 0
}

# _dump_omission_warn: the shared ndjson warn line for a playerbots omission;
# a no-op when the schema resolved (or nothing resolved at all -- the caller
# already refused then).
_dump_omission_warn() {
    _db_names_resolve || return 0
    [[ -z "$DB_NAME_PLAYERBOTS" ]] || return 0
    ndjson_line warn "no playerbots database is configured on this server -- the backup will not include bot data"
    return 0
}

# ---------------------------------------------------------------------------
# Batch 4 (progress & empty states): per-snapshot content summary. At create
# time we drop a tiny sidecar `<backup>.meta` next to the .sql.gz holding a
# compact JSON count of what the snapshot contains, so the Backups page can
# tell snapshots apart BEFORE a restore. Purely additive and best-effort:
# a failed/empty count writes no sidecar, `backup list` renders old backups
# (no sidecar) with a null summary, and this NEVER fails the backup.
# ---------------------------------------------------------------------------

# Echoes {"characters":N,"accounts":N,"bots":N|null} on success (return 0),
# nothing on failure (return 1). characters+accounts are the raw dumped-table
# row counts; bots is the playerbots subset (null when that schema/read is
# unavailable -- an optional "if cheap" field, exactly the account_type IN
# (1,2) idiom `wow config`'s _bots_counts already uses). Reuses the existing
# db_*_query wrappers (30-db.sh), so it runs only while ac-database is up --
# which it always is at backup time (mysqldump needs it too).
_backup_summary_json() {
    local chars accts bots
    chars="$(db_chars_query "SELECT COUNT(*) FROM characters;")" || chars=""
    chars="${chars%%$'\n'*}"
    accts="$(db_auth_query "SELECT COUNT(*) FROM account;")" || accts=""
    accts="${accts%%$'\n'*}"
    [[ "$chars" =~ ^[0-9]+$ && "$accts" =~ ^[0-9]+$ ]] || return 1
    bots="$(db_chars_query "SELECT COUNT(*) FROM characters WHERE $(_bot_account_where account);")" || bots=""
    bots="${bots%%$'\n'*}"
    if [[ "$bots" =~ ^[0-9]+$ ]]; then bots="$((10#$bots))"; else bots=null; fi
    printf '{"characters":%s,"accounts":%s,"bots":%s}' "$((10#$chars))" "$((10#$accts))" "$bots"
    return 0
}

# Writes the summary sidecar for a just-created backup ($1 = the .sql.gz
# path). Swallows every failure -- a missing summary is never an error.
_backup_write_meta() {
    local sj
    sj="$(_backup_summary_json)" || return 0
    [[ -n "$sj" ]] && printf '%s\n' "$sj" > "$1.meta"
    return 0
}

# Reads + validates a backup's summary sidecar; echoes the compact JSON
# object when present and well-formed, or the literal `null` otherwise (so
# `backup list` can embed the result directly). A malformed/garbage sidecar
# degrades to null rather than corrupting the list envelope. $1 = the .sql.gz
# path (NOT the .meta path).
_backup_summary_read() {
    local meta="$1.meta" raw re
    [[ -f "$meta" ]] || { echo null; return 0; }
    raw="$(<"$meta")"
    raw="${raw%%$'\n'*}"
    re='^\{"characters":[0-9]+,"accounts":[0-9]+,"bots":([0-9]+|null)\}$'
    if [[ "$raw" =~ $re ]]; then echo "$raw"; else echo null; fi
    return 0
}
