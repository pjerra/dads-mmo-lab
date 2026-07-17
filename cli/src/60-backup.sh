# ---------------------------------------------------------------------------
# Character backups (Lab-parity round 5): whole-server snapshots.
# mysqldump of acore_characters+acore_playerbots+acore_auth -> ~/.dml/backups.
# RESTORE IS THE PROJECT'S ONE SANCTIONED WRITE PATH INTO CHARACTER DATA
# (the pre-existing LAN toggle's realmlist UPDATE is the only other MySQL
# write): only inside `backup restore`, only with world+auth stopped, always
# behind an automatic pre-restore safety backup. See
# docs/.../2026-07-17-backups-design.md.
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
            rm -f "$bdir/$f"
            echo "$f"
        fi
    done < <(ls -1 "$bdir" 2>/dev/null | grep -E '\.sql\.gz$' | sort -r)
    return 0
}

# Dump the character DBs (plus acore_world when $2 = 1 -- used before
# module installs, which mutate world data), gzip to a tmp file, mv into
# place ($1) -- no partial files on failure. On failure: tmp removed,
# "$1.err" left with stderr (caller reads + removes it), returns 1.
_backup_dump_to() {
    local out="$1" incw="${2:-0}" tmp
    local dbs=(acore_characters acore_playerbots acore_auth)
    [[ "$incw" == 1 ]] && dbs+=(acore_world)
    tmp="$out.tmp"
    if docker exec ac-database mysqldump -uroot -p"$(_db_pw)" --databases "${dbs[@]}" --single-transaction --quick 2>"$out.err" | gzip > "$tmp"; then
        mv "$tmp" "$out"
        rm -f "$out.err"
        return 0
    fi
    rm -f "$tmp"
    return 1
}
