# ---------------------------------------------------------------------------
# GM tools (Lab-parity round 2): shared guard for the bridge-backed gm ops.
# The gm dispatch arms live in 90-main.sh next to the other wow arms; only
# reusable helpers live here. See docs/.../2026-07-16-gm-tools-design.md.
# ---------------------------------------------------------------------------

# Online-guard shared by gm gold/heal/revive (the Eluna bridge can only
# mutate a live Player object). Emits NOT_FOUND and exits if offline.
# Task 6: refuses unresolved schema names FIRST -- the guard's query cannot
# run without them, and letting it fail silently would report a config
# problem as "Character not online".
_gm_require_online() {
    local g
    _db_names_require
    g="$(_party_online_guid "$1")"
    [[ "$g" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $1" "This action needs the character logged in. (Set level works offline.)"; exit 1; }
    return 0
}
