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
