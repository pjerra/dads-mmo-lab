# ---------------------------------------------------------------------------
# WoW config registry + server-info parsing for the DML Launcher.
# (Registry + config verbs land in this file too — see `wow config` in main.)
# ---------------------------------------------------------------------------

# Parses the raw text of the SOAP `server info` result (stdin) into the JSON
# field fragment (stdout, no braces/online key) shared by server-info and
# server-detail. The raw text carries literal `&#xD;` entities because
# soap_parse_result extracts the <result> text without XML-decoding it.
# Unparseable fields become null rather than an error -- the UI renders
# "unknown" for those instead of failing the whole card.
_parse_server_info_fields() {
    local raw line version="" players="" uptime="" mean="" median=""
    raw="$(cat)"
    raw="${raw//&#xD;/}"
    while IFS= read -r line; do
        case "$line" in
            AzerothCore\ rev.*) version="${line#AzerothCore rev. }" ;;
            Connected\ players:*) players="${line#Connected players: }"; players="${players%%.*}" ;;
            Server\ uptime:*) uptime="${line#Server uptime: }" ;;
            *'|- Mean:'*) mean="${line#*Mean: }"; mean="${mean%%ms*}" ;;
            *'|- Median:'*) median="${line#*Median: }"; median="${median%%ms*}" ;;
        esac
    done <<< "$raw"
    [[ "$players" =~ ^[0-9]+$ ]] || players=null
    [[ "$mean" =~ ^[0-9]+$ ]] || mean=null
    [[ "$median" =~ ^[0-9]+$ ]] || median=null
    local vjson=null ujson=null
    [[ -n "$version" ]] && vjson="\"$(json_escape "$version")\""
    [[ -n "$uptime" ]] && ujson="\"$(json_escape "$uptime")\""
    printf '"version":%s,"players":%s,"uptime":%s,"mean_ms":%s,"median_ms":%s' \
        "$vjson" "$players" "$ujson" "$mean" "$median"
    return 0
}

# Back-compat wrapper: the `server-info` verb's envelope shape is public API.
_parse_server_info() {
    printf '{"online":true,%s}' "$(_parse_server_info_fields)"
    return 0
}

# --- Config registry -------------------------------------------------------
# One row per curated setting: key|group|label|type|min|max|env|default|explain
# type: float | int | bool | text | char. bool values are "1"/"0" strings
# (that is what the AC env bridge expects). ahbot.character is special-cased
# in `config set` (resolves a character name to GUID+ACCOUNT, writes both).
# bots.population is special-cased (one number written to MIN and MAX).
# server.motd is special-cased end to end: this AC build has NO Motd conf key
# (verified against worldserver.conf.dist 2026-07-15) -- motd lives in
# acore_auth.motd, is loaded by MotdMgr, and is applied LIVE by the console
# command `.server set motd`. So its env column is a "-" sentinel, `list`
# reads it from the DB, and `set` goes over SOAP instead of the override.
_cfg_rows() {
cat <<'EOF'
rates.xp_kill|Rates|XP from kills|float|0.5|20|AC_RATE_XP_KILL|1|Multiplies XP earned from kills. 3 = level three times as fast.
rates.xp_quest|Rates|XP from quests|float|0.5|20|AC_RATE_XP_QUEST|1|Multiplies XP from quest turn-ins.
rates.gold|Rates|Gold drops|float|0.5|20|AC_RATE_DROP_MONEY|1|Multiplies money dropped by creatures.
bots.population|Playerbots|World bot population|int|0|3000|AC_AI_PLAYERBOT_MAX_RANDOM_BOTS|500|How many ambient bots populate the world. Saving writes min and max to this one number.
bots.autologin|Playerbots|Bots log in at server start|bool|||AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN|1|When on, ambient bots log in automatically after the server starts.
ahbot.seller|AHBot|Auction seller bot|bool|||AC_AUCTION_HOUSE_BOT_ENABLE_SELLER|0|When on, the auction house is stocked with items for sale.
ahbot.buyer|AHBot|Auction buyer bot|bool|||AC_AUCTION_HOUSE_BOT_ENABLE_BUYER|0|When on, the bot occasionally buys player auctions.
ahbot.character|AHBot|Seller character|char|||AC_AUCTION_HOUSE_BOT_GUID|0|Which character appears as the auction seller. Saving also writes the matching account id. Shown as the stored character id.
server.motd|Server|Message of the day|text|||-|Welcome to Dad's MMO Lab!|Shown to every player at login. Applies instantly while the server runs - no restart needed. Quotes and line breaks are removed.
EOF
}

# Shared preamble for every `wow config` subcommand: needs yq + the wow dir.
# Sets: cfg_sdir, cfg_ovr. Emits the error envelope and exits on failure.
_cfg_preamble() {
    DML_YQ_BIN="${DML_YQ_BIN:-yq}"
    if ! command -v "$DML_YQ_BIN" >/dev/null 2>&1; then
        json_err MISSING_DEP "yq is required for wow config but not installed" "Run: pacman -S go-yq (inside dml-arch as root)"
        exit 1
    fi
    cfg_sdir="$(_wow_server_dir)"
    if [[ -z "$cfg_sdir" ]]; then
        json_err NOT_FOUND "WoW Playerbots server not installed" "Install it first, then re-run."
        exit 1
    fi
    cfg_ovr="$cfg_sdir/docker-compose.override.yml"
    return 0
}

# _cfg_env_read <ENV>: echoes the override's value for that env key, or "".
_cfg_env_read() {
    [[ -f "$cfg_ovr" ]] || { printf ''; return 0; }
    E="$1" "$DML_YQ_BIN" -r '.services.ac-worldserver.environment[strenv(E)] // ""' "$cfg_ovr" 2>/dev/null || printf ''
    return 0
}

# _cfg_env_write <ENV> <value>: merges the key into the EXISTING service
# (soap-setup's proven pattern -- never a second top-level services: block).
# strenv() keeps hostile values out of the yq program text entirely.
# Sets CFG_CHANGED=true when the stored value actually changed.
_cfg_env_write() {
    local cur
    cur="$(_cfg_env_read "$1")"
    [[ "$cur" == "$2" ]] && return 0
    [[ -f "$cfg_ovr" ]] || printf 'services:\n  ac-worldserver:\n    environment:\n' > "$cfg_ovr"
    E="$1" V="$2" "$DML_YQ_BIN" -i \
        '.services.ac-worldserver.environment[strenv(E)] = strenv(V)' "$cfg_ovr"
    CFG_CHANGED=true
    return 0
}

# _float_in_range <val> <min> <max>: 0 iff val is a decimal in [min,max].
_float_in_range() {
    [[ "$1" =~ ^[0-9]+([.][0-9]+)?$ ]] || return 1
    awk -v v="$1" -v lo="$2" -v hi="$3" 'BEGIN { exit !(v >= lo && v <= hi) }'
}

# _cfg_file_path <name>: maps an allowlisted file name to its host path
# under $cfg_sdir (the base compose bind-mounts ./env/dist/etc into the
# container, so module confs are ordinary host files). Unknown name -> rc 1.
# The allowlist is the traversal guard: names are matched literally, never
# used as path fragments.
_cfg_file_path() {
    case "$1" in
        .env) printf '%s' "$cfg_sdir/.env" ;;
        docker-compose.override.yml) printf '%s' "$cfg_sdir/docker-compose.override.yml" ;;
        playerbots.conf|mod_ahbot.conf|mod_ale.conf) printf '%s' "$cfg_sdir/env/dist/etc/modules/$1" ;;
        *) return 1 ;;
    esac
    return 0
}

# --- server-detail helpers -------------------------------------------------
# All read-only. Down/absent is data, never an error.

# One "name|state|status" line per long-running service (fixed order:
# world, auth, database), from a single `docker ps -a`. Absent containers
# (including docker daemon down) get state "absent" and empty status.
_detail_container_rows() {
    local ps_out="" name line found
    ps_out="$(docker ps -a --format '{{.Names}}|{{.State}}|{{.Status}}' 2>/dev/null || true)"
    for name in ac-worldserver ac-authserver ac-database; do
        found=""
        while IFS= read -r line; do
            [[ "${line%%|*}" == "$name" ]] && { found="$line"; break; }
        done <<< "$ps_out"
        if [[ -n "$found" ]]; then
            printf '%s\n' "$found"
        else
            printf '%s|absent|\n' "$name"
        fi
    done
    return 0
}

# Exit-status helper (like _valid_charname): 0 when the CURRENT worldserver
# run has logged AzerothCore's boot-complete marker. `compose stop`/`start`
# preserves container logs, so a marker from the previous run would lie
# during a re-boot -- hence --since the container's StartedAt.
_world_ready() {
    local started hits
    if started="$(docker inspect -f '{{.State.StartedAt}}' ac-worldserver 2>/dev/null)"; then :; else return 1; fi
    [[ -z "$started" ]] && return 1
    hits="$(docker logs --since "$started" ac-worldserver 2>&1 | grep -icm1 'World Initialized In' || true)"
    [[ "${hits:-0}" -gt 0 ]]
}

# Host port for a container's internal port as a JSON string, or `null`.
# `docker port` prints one "0.0.0.0:8085"-style line per bind; take the first.
_host_port_json() {
    local out=""
    out="$(docker port "$1" "$2" 2>/dev/null | head -n1 || true)"
    out="${out##*:}"
    if [[ "$out" =~ ^[0-9]+$ ]]; then printf '"%s"' "$out"; else printf 'null'; fi
    return 0
}
