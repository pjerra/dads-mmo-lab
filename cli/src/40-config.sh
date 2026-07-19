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
#
# CONF-FILE ROWS (Batch 1): an env column of `conf:<Key>` (worldserver.conf)
# or `conf:<file>.conf:<Key>` (a conf under env/dist/etc/modules/) routes
# `config list`/`config set` to a comment-preserving in-place edit of the
# HOST conf file (bind-mounted into the container) instead of the compose
# override. Rationale: a running container's env is frozen at creation AND
# AC's env bridge beats conf values, so env-set keys can never live-apply --
# conf rows CAN (worldserver.conf ones via SOAP `reload config`). On save,
# any matching legacy AC_* env key (derived mechanically from the conf key,
# see _cfg_env_name_for) is removed from the override so the conf value is
# authoritative after the next recreate; while the frozen env is still in
# the running container the save reports `"applied":"restart"`. The three
# rates rows below were MIGRATED from env rows to conf rows (their legacy
# env keys are cleaned up on save by the same derivation).
_cfg_rows() {
cat <<'EOF'
rates.xp_kill|Rates|XP from kills|float|0.5|20|conf:Rate.XP.Kill|1|Multiplies XP earned from kills. 3 = level three times as fast.
rates.xp_quest|Rates|XP from quests|float|0.5|20|conf:Rate.XP.Quest|1|Multiplies XP from quest turn-ins.
rates.xp_explore|Rates|XP from exploring|float|0.5|20|conf:Rate.XP.Explore|1|Multiplies XP for discovering new areas.
rates.gold|Rates|Gold drops|float|0.5|20|conf:Rate.Drop.Money|1|Multiplies money dropped by creatures.
rates.honor|Rates|Honor gains|float|0.5|20|conf:Rate.Honor|1|Multiplies honor points from PvP kills and battlegrounds.
rates.reputation|Rates|Reputation gains|float|0.5|20|conf:Rate.Reputation.Gain|1|Multiplies reputation earned with factions.
rates.rested|Rates|Rested XP build-up|float|0.5|20|conf:Rate.Rest.InGame|1|How fast rested XP builds while logged in at an inn or city.
rates.loot|Rates|Common item drops|float|0.5|20|conf:Rate.Drop.Item.Normal|1|Multiplies how often creatures drop common (white) items.
rates.creature_damage|Rates|Monster damage|float|0.1|10|conf:Rate.Creature.Normal.Damage|1|Multiplies damage normal monsters deal. Below 1 makes fights easier.
rates.creature_hp|Rates|Monster health|float|0.1|10|conf:Rate.Creature.Normal.HP|1|Multiplies normal monsters' health. Below 1 makes fights faster.
rates.movespeed|Rates|Player movement speed|float|0.5|10|conf:Rate.MoveSpeed.Player|1|Multiplies how fast characters run. Applies on login.
crossfaction.accounts|Cross-faction|Both factions on one account|bool|||conf:AllowTwoSide.Accounts|1|When on, one account can have both Alliance and Horde characters.
crossfaction.group|Cross-faction|Group across factions|bool|||conf:AllowTwoSide.Interaction.Group|0|When on, Alliance and Horde players can group up together.
crossfaction.guild|Cross-faction|Guilds across factions|bool|||conf:AllowTwoSide.Interaction.Guild|0|When on, guilds can have members from both factions.
crossfaction.chat|Cross-faction|Chat across factions|bool|||conf:AllowTwoSide.Interaction.Chat|0|When on, both factions understand each other in chat.
crossfaction.auction|Cross-faction|Shared auction house|bool|||conf:AllowTwoSide.Interaction.Auction|0|When on, both factions use one shared auction house.
crossfaction.calendar|Cross-faction|Calendar across factions|bool|||conf:AllowTwoSide.Interaction.Calendar|0|When on, calendar invites work across factions.
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

# _cfg_env_remove <ENV>: deletes the key from the override's environment map
# (no-op when the override or key is absent). Callers use _cfg_env_read first
# to learn whether it WAS present (frozen-env restart signal).
_cfg_env_remove() {
    [[ -f "$cfg_ovr" ]] || return 0
    E="$1" "$DML_YQ_BIN" -i \
        'del(.services.ac-worldserver.environment[strenv(E)])' "$cfg_ovr" 2>/dev/null || true
    return 0
}

# --- conf-file rows (Batch 1) ----------------------------------------------
# See the registry comment block: `conf:` env-column values route config
# list/set to the bind-mounted conf files instead of the compose override.

# _cfg_conf_route <envcol>: parses a `conf:[<file>.conf:]<Key>` spec. Sets
# conf_file (defaults to worldserver.conf when the file part is omitted) and
# conf_key for the caller (top-level dispatch -- deliberately not local).
# Returns 1 when the value is not a conf: spec at all.
_cfg_conf_route() {
    local spec
    [[ "$1" == conf:* ]] || return 1
    spec="${1#conf:}"
    conf_file="worldserver.conf"
    if [[ "$spec" == *.conf:* ]]; then
        conf_file="${spec%%:*}"
        spec="${spec#*:}"
    fi
    conf_key="$spec"
    return 0
}

# _cfg_conf_path <file>: host path of a conf. worldserver/authserver live in
# env/dist/etc/, every other conf under env/dist/etc/modules/. The callers'
# specs come from the registry (or a validated direct route), never a raw
# user path.
_cfg_conf_path() {
    case "$1" in
        worldserver.conf|authserver.conf) printf '%s' "$cfg_sdir/env/dist/etc/$1" ;;
        *) printf '%s' "$cfg_sdir/env/dist/etc/modules/$1" ;;
    esac
    return 0
}

# _cfg_conf_ensure <path>: makes sure the conf exists, creating it from its
# .dist ONCE when only the dist is present (the AC docker layout ships dists;
# the conf appears on first edit). Returns 1 when neither exists.
_cfg_conf_ensure() {
    [[ -f "$1" ]] && return 0
    [[ -f "$1.dist" ]] || return 1
    cp "$1.dist" "$1"
    return 0
}

# _cfg_conf_read <path> <Key>: echoes the conf's value for Key ("" when the
# file or key is absent). Exact-key match (prefix + '='), comments skipped,
# LAST occurrence wins (AC semantics), surrounding quotes stripped. The key
# travels via the environment (K=), never awk -v, so dots/backslashes are
# never escape-processed.
_cfg_conf_read() {
    local val=""
    [[ -f "$1" ]] || { printf ''; return 0; }
    val="$(K="$2" awk '
        {
            s = $0; sub(/\r$/, "", s); sub(/^[ \t]+/, "", s)
            k = ENVIRON["K"]
            if (index(s, k) == 1) {
                rest = substr(s, length(k) + 1)
                sub(/^[ \t]*/, "", rest)
                if (substr(rest, 1, 1) == "=") {
                    v = substr(rest, 2)
                    sub(/^[ \t]+/, "", v); sub(/[ \t]+$/, "", v)
                    val = v; found = 1
                }
            }
        }
        END { if (found) print val }
    ' "$1" 2>/dev/null)" || val=""
    val="${val%\"}"; val="${val#\"}"
    printf '%s' "$val"
    return 0
}

# _cfg_conf_write <path> <Key> <value>: comment-preserving in-place edit --
# replaces every active `Key = ...` line (duplicates collapse to the same
# value; AC reads the last anyway) or appends `Key = value` when absent.
# tmp-file + mv like raw-write, so a failure never truncates the conf.
# Sets CFG_CHANGED=true when the effective value actually changed.
_cfg_conf_write() {
    local cur tmp
    cur="$(_cfg_conf_read "$1" "$2")"
    [[ "$cur" == "$3" ]] && return 0
    tmp="$1.tmp.$$"
    K="$2" V="$3" awk '
        BEGIN { done = 0 }
        {
            s = $0; sub(/\r$/, "", s); sub(/^[ \t]+/, "", s)
            k = ENVIRON["K"]
            if (index(s, k) == 1) {
                rest = substr(s, length(k) + 1)
                sub(/^[ \t]*/, "", rest)
                if (substr(rest, 1, 1) == "=") {
                    print k " = " ENVIRON["V"]
                    done = 1
                    next
                }
            }
            print
        }
        END { if (!done) print ENVIRON["K"] " = " ENVIRON["V"] }
    ' "$1" > "$tmp" || { rm -f "$tmp"; return 1; }
    mv "$tmp" "$1"
    CFG_CHANGED=true
    return 0
}

# _cfg_env_name_for <ConfKey>: the AC docker env-bridge name for a conf key
# (AC_ + camelCase split on lower/digit->UPPER boundaries + dots as _, all
# uppercased): AiPlayerbot.MaxRandomBots -> AC_AI_PLAYERBOT_MAX_RANDOM_BOTS,
# Rate.XP.Kill -> AC_RATE_XP_KILL. Used to clean a legacy env override off
# override.yml when its conf row is saved (env would otherwise beat the conf
# forever). A derivation miss is harmless -- the removal is just a no-op.
_cfg_env_name_for() {
    local key="$1" out="" i c prev=""
    for (( i = 0; i < ${#key}; i++ )); do
        c="${key:$i:1}"
        if [[ "$c" == "." ]]; then
            out+="_"
        elif [[ "$c" == [A-Z] && "$prev" == [a-z0-9] ]]; then
            out+="_$c"
        else
            out+="$c"
        fi
        prev="$c"
    done
    printf 'AC_%s' "${out^^}"
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

# _bots_counts <world_state>: echoes the complete server-detail bots JSON
# fragment `"bots":{"online":<int|null>,"max":<int|null>}`. Computed ONLY
# when <world_state> is "running" -- otherwise both fields stay null and NO
# mysql call is made (a stopped/booting world has no live bot count and the
# max lookup isn't worth a docker exec either).
#
# online: exact COUNT(*) via db_chars_query, the same cross-schema idiom as
# `party online` (see that arm in 90-main.sh) but inverted -- INCLUDES bot
# accounts instead of excluding them. A query failure/timeout, empty result,
# or non-numeric garbage all degrade to null; this verb never errors on a
# read-only lookup.
#
# max: env override first (_cfg_env_read, the same helper `wow config` uses),
# falling back to a raw grep of playerbots.conf. Both steps need the same
# context _cfg_preamble would set up (cfg_ovr/DML_YQ_BIN), but unlike
# _cfg_preamble this never exits on a missing yq or missing server dir --
# server-detail must keep answering even when WoW isn't installed yet.
_bots_counts() {
    local state="$1" online=null max=null rows val conf
    if [[ "$state" == running ]]; then
        rows="$(db_chars_query "SELECT COUNT(*) FROM characters WHERE online = 1 AND account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2));")" || true
        rows="${rows%%$'\n'*}"
        # $((10#...)) after the regex gate: strips leading zeros, which would
        # otherwise emit invalid JSON (e.g. "max":0500) and break the whole
        # server-detail envelope.
        [[ "$rows" =~ ^[0-9]+$ ]] && online="$((10#$rows))"

        local cfg_sdir="" cfg_ovr=""
        cfg_sdir="$(_wow_server_dir)"
        DML_YQ_BIN="${DML_YQ_BIN:-yq}"
        if [[ -n "$cfg_sdir" ]]; then
            cfg_ovr="$cfg_sdir/docker-compose.override.yml"
            val="$(_cfg_env_read AC_AI_PLAYERBOT_MAX_RANDOM_BOTS)"
            if [[ "$val" =~ ^[0-9]+$ ]]; then
                max="$((10#$val))"
            else
                conf="$cfg_sdir/env/dist/etc/modules/playerbots.conf"
                if [[ -f "$conf" ]]; then
                    val="$(grep -E '^[[:space:]]*AiPlayerbot\.MaxRandomBots' "$conf" 2>/dev/null | tail -n1)" || true
                    val="${val#*=}"
                    val="${val//[[:space:]]/}"
                    val="${val//\"/}"
                    val="${val//\'/}"
                    [[ "$val" =~ ^[0-9]+$ ]] && max="$((10#$val))"
                fi
            fi
        fi
    fi
    printf '"bots":{"online":%s,"max":%s}' "$online" "$max"
    return 0
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
