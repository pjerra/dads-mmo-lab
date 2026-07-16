# ---------------------------------------------------------------------------
# My Party (Plan 4): Eluna bridge deployment + group-join confirmation.
# Bridge scripts run playerbot commands in the online player's own session
# (SOAP -> Eluna hook 42 -> Player:RunCommand). See docs/.../my-party-design.
# ---------------------------------------------------------------------------

# Source dir of the committed bridge scripts (repo cli/lua/party). Resolved
# relative to this script's own location at build time is not possible in the
# concatenated artifact, so we resolve from the CLI's known repo layout via
# DML_LUA_DIR (test seam), falling back to the install-relative path.
_party_lua_src_dir() { echo "${DML_LUA_DIR:-/mnt/c/Users/perzi/dads-mmo-lab/cli/lua/party}"; }

# Host dir where mod-ale loads scripts (bind-mounted into the container at
# ALE.ScriptPath). <server dir>/env/dist/etc/modules/lua_scripts.
_party_lua_dest_dir() {
    local sdir="$1"
    echo "$sdir/env/dist/etc/modules/lua_scripts"
}

# Copy the 3 bridge scripts into dest; echo "changed" if any file's content
# differs from what's already there (idempotence), "" otherwise.
_party_deploy_scripts() {
    local src dest changed=""
    src="$(_party_lua_src_dir)"
    dest="$(_party_lua_dest_dir "$1")"
    mkdir -p "$dest"
    local f
    for f in dml_addclass.lua dml_uninvite.lua dml_login.lua; do
        if [[ ! -f "$dest/$f" ]] || ! cmp -s "$src/$f" "$dest/$f"; then
            cp "$src/$f" "$dest/$f"
            changed=1
        fi
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
      2) json_err SOAP_FAULT "The $2 command was rejected" "Run 'Enable My Party' (party-setup) and restart the server first." ;;
      *) json_err SOAP_UNREACHABLE "Could not reach the server" "Is it running?" ;;
    esac
    exit 1
}
