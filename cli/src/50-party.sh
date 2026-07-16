# ---------------------------------------------------------------------------
# My Party (Plan 4): Eluna bridge deployment + group-join confirmation.
# Bridge scripts run playerbot commands in the online player's own session
# (SOAP -> Eluna hook 42 -> Player:RunCommand). See docs/.../my-party-design.
#
# Deployment (bridge-setup, Plan/Task 2) is family-generic: it copies every
# *.lua file under every subdir of the lua ROOT (party/, gm/, ...) into the
# server's flat lua_scripts dir. party-setup/setup remain accepted aliases.
# ---------------------------------------------------------------------------

# Root of the committed bridge scripts (repo cli/lua -> installed
# /usr/local/share/dml/lua). Contains one subdir per bridge family
# (party/, gm/, ...). DML_LUA_DIR is the test seam and now points at
# this ROOT, not a single family dir.
_bridge_lua_root() { echo "${DML_LUA_DIR:-/usr/local/share/dml/lua}"; }

# Host dir where mod-ale loads scripts (bind-mounted into the container at
# ALE.ScriptPath). <server dir>/env/dist/etc/modules/lua_scripts.
_party_lua_dest_dir() {
    local sdir="$1"
    echo "$sdir/env/dist/etc/modules/lua_scripts"
}

# Copy every family's *.lua into dest (flat -- mod-ale loads a flat dir);
# echo "changed" if any file's content differs (idempotence), "" otherwise.
_bridge_deploy_scripts() {
    local root dest changed="" d f
    root="$(_bridge_lua_root)"
    dest="$(_party_lua_dest_dir "$1")"
    mkdir -p "$dest"
    for d in "$root"/*/; do
        [[ -d "$d" ]] || continue
        for f in "$d"*.lua; do
            [[ -f "$f" ]] || continue
            if [[ ! -f "$dest/$(basename "$f")" ]] || ! cmp -s "$f" "$dest/$(basename "$f")"; then
                cp "$f" "$dest/$(basename "$f")"
                changed=1
            fi
        done
    done
    [[ -n "$changed" ]] && echo changed
    return 0
}

# Online player's guid, or empty if not online (online-guard).
_party_online_guid() {
    db_chars_query "SELECT guid FROM characters WHERE name='$(sql_escape "$1")' AND online=1 LIMIT 1;" 2>/dev/null
    return 0
}

# The memberGuids of the group the player (guid $1) belongs to; empty if solo.
_party_group_member_guids() {
    db_chars_query "SELECT memberGuid FROM group_member WHERE guid=(SELECT guid FROM group_member WHERE memberGuid=$1 LIMIT 1);" 2>/dev/null
    return 0
}

# Fire a bridge command over SOAP; on failure emit the right envelope+exit.
# $1 = the full dml_* command; $2 = a short hint noun for the fault case.
#
# NB: deliberately uses if/else (not a bare `local rc=$?` after a bare `fi`).
# Bash's if/then/fi with NO else and a false condition exits the whole
# compound statement with status 0 ("or zero if no condition tested true"
# -- see the Bash manual's Conditional Constructs section), so a trailing
# `local rc=$?` right after such an `fi` would always read 0, never
# soap_exec's real rc. Confirmed empirically. The `else` branch keeps $?
# as soap_exec's exit code since `rc=$?` is the first thing that runs there.
_party_fire() {
    local rc
    if out="$(soap_exec "$1")"; then
        return 0
    else
        rc=$?
    fi
    case "$rc" in
      3) json_err SOAP_AUTH "SOAP auth failed" "Check ~/.dml/soap.env" ;;
      2) json_err SOAP_FAULT "The $2 command was rejected" "Deploy the server bridges (bridge-setup) and restart the server first." ;;
      *) json_err SOAP_UNREACHABLE "Could not reach the server" "Is it running?" ;;
    esac
    exit 1
}
