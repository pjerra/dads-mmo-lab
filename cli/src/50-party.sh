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

# Poll group membership until a NEW member (one not in $2, a space-
# separated guid snapshot) appears for player guid $1; echo the new guid
# or "" on timeout. Seams: DML_PARTY_POLL_TRIES (12) / _SLEEP (0.5).
_party_wait_new_member() {
    local pguid="$1" before="$2" tries slp newguid i now g
    tries="${DML_PARTY_POLL_TRIES:-12}"; slp="${DML_PARTY_POLL_SLEEP:-0.5}"
    newguid=""; i=0
    while (( i < tries )); do
        now="$(_party_group_member_guids "$pguid" | tr '\n' ' ')"
        for g in $now; do
            [[ "$g" == "$pguid" ]] && continue
            case " $before " in *" $g "*) : ;; *) newguid="$g"; break ;; esac
        done
        [[ -n "$newguid" ]] && break
        i=$(( i + 1 ))
        [[ "$slp" != "0" ]] && sleep "$slp"
    done
    echo "$newguid"
    return 0
}

# characters.class id -> the class name `party add --class` accepts.
# Unsupported ids (6 = deathknight) echo "" -- callers skip those.
_class_name_from_id() {
    case "$1" in
      1) echo warrior ;; 2) echo paladin ;; 3) echo hunter ;; 4) echo rogue ;;
      5) echo priest ;; 7) echo shaman ;; 8) echo mage ;; 9) echo warlock ;;
      11) echo druid ;; *) echo "" ;;
    esac
    return 0
}

_preset_dir() { echo "$HOME/.dml/party-presets"; }

# Exit status IS the signal (same pattern as _valid_charname).
_valid_preset_name() { [[ "$1" =~ ^[A-Za-z0-9_-]{1,32}$ ]]; }

# The class set `party add --class` accepts. Shared by preset-load (skips
# unknown lines from a hand-edited preset file) and preset-import (BAD_ARG
# on any bad token). Exit status IS the signal (same pattern as
# _valid_charname/_valid_preset_name). Deathknight (class id 6) is
# deliberately excluded -- see _class_name_from_id above.
_valid_bot_class() {
    case "$1" in
      warrior|paladin|hunter|rogue|priest|shaman|mage|warlock|druid) return 0 ;;
      *) return 1 ;;
    esac
}

# Premade-spec names `party add --spec` / `party botcmd --action spec`
# accept (Batch 5 F5). CLOSED allowlist of the EXACT live spec names
# (AiPlayerbot.PremadeSpecName.* -- verified against the deployed
# playerbots.conf 2026-07-19): the playerbots `talents spec <name>` command
# exact-matches these, and a wrong name replies only IN-GAME ("Spec <x> not
# found" -- invisible to SOAP), so the CLI must reject anything else up
# front. Chars are only [a-z ] -- injection-safe in the whisper tail; the
# no-free-text-whisper rule holds because this is still a fixed allowlist.
# DK (class 6) specs are deliberately absent (no DK in the wizard, matching
# _valid_bot_class). NB per the live conf: "bear pvp" and "frostfire pvp"
# DO NOT EXIST -- do not "complete the symmetry" here. CAVEAT: these names
# are conf-driven; if the user edits PremadeSpecName.* this list drifts and
# failures become silent (in-game whisper reply only).
_valid_bot_spec() {
    case "$1" in
      "arms pve"|"arms pvp"|"fury pve"|"fury pvp"|"prot pve"|"prot pvp") return 0 ;;
      "holy pve"|"holy pvp"|"ret pve"|"ret pvp") return 0 ;;
      "bm pve"|"bm pvp"|"mm pve"|"mm pvp"|"surv pve"|"surv pvp") return 0 ;;
      "as pve"|"as pvp"|"combat pve"|"combat pvp"|"subtlety pve"|"subtlety pvp") return 0 ;;
      "disc pve"|"disc pvp"|"shadow pve"|"shadow pvp") return 0 ;;
      "ele pve"|"ele pvp"|"enh pve"|"enh pvp"|"resto pve"|"resto pvp") return 0 ;;
      "arcane pve"|"arcane pvp"|"fire pve"|"fire pvp"|"frost pve"|"frost pvp"|"frostfire pve") return 0 ;;
      "affli pve"|"affli pvp"|"demo pve"|"demo pvp"|"destro pve"|"destro pvp") return 0 ;;
      "balance pve"|"balance pvp"|"bear pve"|"cat pve"|"cat pvp") return 0 ;;
      *) return 1 ;;
    esac
}
