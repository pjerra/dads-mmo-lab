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

# Path of the DEPLOYED playerbots.conf (falling back to the shipped .dist),
# or empty when the WoW server is not installed. Single source of truth for
# both the live spec picker (`party specs`) and _valid_bot_spec below, so the
# two can never disagree (Batch 5 F5 follow-up: kills the old hand-kept
# allowlist mirror's silent drift).
_party_pb_conf() {
    local sdir conf
    sdir="$(_wow_server_dir)" || sdir=""
    [[ -n "$sdir" ]] || return 0
    conf="$sdir/env/dist/etc/modules/playerbots.conf"
    [[ -f "$conf" ]] || conf="$conf.dist"
    [[ -f "$conf" ]] || return 0
    printf '%s' "$conf"
    return 0
}

# Every live premade spec NAME (AiPlayerbot.PremadeSpecName.<class>.<specno>),
# one per line, deduped, EXCLUDING class 6 (deathknight -- no DK in the party
# system, matching _valid_bot_class). Empty when no conf is deployed. POSIX awk
# (index/substr/split -- no gawk 3-arg match), same style as _pb_kv_lines.
_party_spec_names() {
    local conf; conf="$(_party_pb_conf)"; [[ -n "$conf" ]] || return 0
    awk '
        { s=$0; sub(/\r$/,"",s); sub(/^[ \t]+/,"",s)
          if (s !~ /^AiPlayerbot\.PremadeSpecName\./) next
          eq=index(s,"="); if (eq==0) next
          key=substr(s,1,eq-1); sub(/[ \t]+$/,"",key)
          val=substr(s,eq+1); sub(/^[ \t]+/,"",val); sub(/[ \t]+$/,"",val)
          n=split(key,p,"."); cls=p[3]+0
          if (cls==6) next
          if (val != "") print val
        }
    ' "$conf" 2>/dev/null | sort -u
    return 0
}

# Rows for `party specs`: one TAB-separated row per (class,specno) --
# class_id<TAB>specno<TAB>name<TAB>highest-level-link<TAB>tree ("a/b/c" digit
# sums of the three dash-separated talent trees; empty when no link). Class 6
# excluded. Sorted by class then specno. $1 = conf path.
_party_spec_rows() {
    awk '
        function digsum(x,   i,c,t){ t=0; for(i=1;i<=length(x);i++){c=substr(x,i,1); if(c>="0"&&c<="9") t+=c+0} return t }
        { s=$0; sub(/\r$/,"",s); sub(/^[ \t]+/,"",s)
          if (s !~ /^AiPlayerbot\.PremadeSpec(Name|Link)\./) next
          eq=index(s,"="); if (eq==0) next
          key=substr(s,1,eq-1); sub(/[ \t]+$/,"",key)
          val=substr(s,eq+1); sub(/^[ \t]+/,"",val); sub(/[ \t]+$/,"",val)
          n=split(key,p,"."); cls=p[3]+0; spc=p[4]+0
          if (cls==6) next
          k=cls SUBSEP spc
          if (p[2]=="PremadeSpecName") { if (val!=""){ name[k]=val; seen[k]=1 } }
          else if (p[2]=="PremadeSpecLink") { lvl=p[5]+0; if (lvl>=blvl[k]){ blvl[k]=lvl; link[k]=val } }
        }
        END {
          for (k in seen) {
            split(k,kk,SUBSEP); cls=kk[1]+0; spc=kk[2]+0
            lk=link[k]; tree=""
            if (lk!="") { ng=split(lk,g,"-"); tree=digsum(g[1]) "/" (ng>=2?digsum(g[2]):0) "/" (ng>=3?digsum(g[3]):0) }
            printf "%d\t%d\t%s\t%s\t%s\n", cls, spc, name[k], lk, tree
          }
        }
    ' "$1" 2>/dev/null | sort -t"$(printf '\t')" -k1,1n -k2,2n
    return 0
}

# Premade-spec names `party add --spec` / `party botcmd --action spec` accept
# (Batch 5 F5). Driven by the DEPLOYED playerbots.conf's
# AiPlayerbot.PremadeSpecName.* values (via _party_spec_names) so it can never
# drift from what the playerbots `talents spec <name>` command actually
# accepts -- a wrong name replies only IN-GAME ("Spec <x> not found",
# invisible to SOAP), so the CLI must reject anything the conf does not define.
# The charset guard is enforced regardless (injection-safe whisper tail; the
# no-free-text-whisper rule holds because the value must still be a conf-listed
# name). When no conf is deployed (server not installed / tests) it FALLS BACK
# to a static mirror of the shipped defaults so validation keeps working. DK
# (class 6) specs are deliberately absent (no DK in the wizard).
_valid_bot_spec() {
    local want="$1" names
    # Injection guard first. The charset is WIDER than the shipped names' plain
    # lowercase-and-spaces: playerbots.conf is hand-editable (and raw-writable
    # from the Modules editor), the picker offers every conf name verbatim, and
    # refusing "Arms PvE" here just made the picker offer specs this function
    # rejected. What it must never admit is anything unsafe in the
    # `dml_whisper <p> <b> talents spec <name>` tail -- no quotes, no
    # backslash, no CR/LF, no shell/SQL metacharacters -- hence alphanumerics
    # plus space . _ - only, and an alphanumeric first character.
    # Keep this in sync with valid_bot_spec_shape (crates/dml-wow/src/party.rs)
    # and isValidSpecShape (launcher/src/lib/party-specs.ts).
    [[ "$want" =~ ^[A-Za-z0-9][A-Za-z0-9\ ._-]*$ ]] || return 1
    names="$(_party_spec_names)"
    if [[ -n "$names" ]]; then
        if grep -qxF -- "$want" <<< "$names"; then return 0; else return 1; fi
    fi
    # Fallback (no deployed conf): static mirror of the shipped playerbots.conf
    # defaults (verified 2026-07-19). NB "bear pvp" / "frostfire pvp" do NOT
    # exist -- do not "complete the symmetry".
    case "$want" in
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
