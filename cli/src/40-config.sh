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
    [[ -n "$version" ]] && { json_escape_var "$version"; vjson="\"$REPLY\""; }
    [[ -n "$uptime" ]] && { json_escape_var "$uptime"; ujson="\"$REPLY\""; }
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
# bots.population is special-cased (one number written to BOTH
# AiPlayerbot.MinRandomBots and .MaxRandomBots in playerbots.conf, and both
# legacy MIN/MAX env overrides removed).
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
# conf rows CAN (worldserver.conf ones via SOAP `reload config`; ditto
# mod_ahbot.conf ones, Batch 4 F14 -- AzerothCore's `reload config`
# re-parses module confs too and mod-ah-bot's own
# AHBot_WorldScript::OnBeforeConfigLoad(reload=true) re-reads Account/GUID
# and restarts its bots with the new values, verified against the deployed
# module source AuctionHouseBotWorldScript.cpp 2026-07-19). On save,
# any matching legacy AC_* env key (derived mechanically from the conf key,
# see _cfg_env_name_for) is removed from the override so the conf value is
# authoritative after the next recreate; while the frozen env is still in
# the running container the save reports `"applied":"restart"`. The three
# rates rows below were MIGRATED from env rows to conf rows (their legacy
# env keys are cleaned up on save by the same derivation), and so were
# bots.population / bots.autologin (playerbots.conf) and the three AHBot
# rows (mod_ahbot.conf, Batch 4 -- ahbot.character stays special-cased in
# `config set`: it resolves a name to GUID + Account and writes BOTH conf
# keys, cleaning both legacy env keys). playerbots.conf rows
# NEVER live-apply (the module reads its conf at startup) -- their set
# always answers `"applied":"restart"`. `config set` additionally accepts a
# DIRECT route key `conf:<file>.conf:<Key>` for the all-keys browsers (Bot
# World's playerbots.conf + the Module tuning tab's per-module browsers):
# any conf passing _cfg_file_path's dynamic module-conf allowlist is
# writable there -- worldserver.conf/authserver.conf keys stay
# curated-rows-only on purpose -- validated as ^[A-Za-z0-9_.]+$ with a
# newline-free value. A direct save live-applies ONLY when the module has a
# VERIFIED reload console command (_conf_reload_cmd; currently mod-transmog)
# and no frozen legacy env; everything else stays restart-to-apply.
#
# MIRRORED SNAPSHOT: crates/dml-wow/data/config-registry.json embeds this
# registry for the native launcher + dml-wow-cli. Edited a row? Regenerate:
#   bash cli/dml wow config registry --json | jq .data.settings > crates/dml-wow/data/config-registry.json
# Skip it and the native path ships stale data -- crates/dml-wow/tests/config_parity.rs
# would catch it, but SKIPS (silently passes) on any machine without the
# native runtime at C:/Users/perzi/dml-native (i.e. CI and most dev boxes).
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
bots.population|Bot Population|World bot population|int|0|3000|conf:playerbots.conf:AiPlayerbot.MaxRandomBots|500|How many ambient bots populate the world. Saving writes min and max to this one number.
bots.autologin|Bot Population|Bots log in at server start|bool|||conf:playerbots.conf:AiPlayerbot.RandomBotAutologin|1|When on, ambient bots log in automatically after the server starts.
bots.per_interval|Bot Population|Bot logins per cycle|int|1|500|conf:playerbots.conf:AiPlayerbot.RandomBotsPerInterval|60|How many bots are processed per update cycle. Higher fills the world faster after a restart.
bots.periodic_online|Bot Population|Bots rotate online and offline|bool|||conf:playerbots.conf:AiPlayerbot.EnablePeriodicOnlineOffline|0|When on, bots log in and out over time like real players instead of staying a fixed set.
bots.alliance_ratio|Bot Balance|Alliance share|int|0|100|conf:playerbots.conf:AiPlayerbot.RandomBotAllianceRatio|50|Weighting for how many bots are Alliance. 50 and 50 with Horde share means an even split.
bots.horde_ratio|Bot Balance|Horde share|int|0|100|conf:playerbots.conf:AiPlayerbot.RandomBotHordeRatio|50|Weighting for how many bots are Horde.
bots.min_level|Bot Balance|Lowest bot level|int|1|80|conf:playerbots.conf:AiPlayerbot.RandomBotMinLevel|1|New bots are created no lower than this level.
bots.max_level|Bot Balance|Highest bot level|int|1|80|conf:playerbots.conf:AiPlayerbot.RandomBotMaxLevel|80|New bots are created no higher than this level.
bots.sync_level|Bot Balance|Match player levels|bool|||conf:playerbots.conf:AiPlayerbot.SyncLevelWithPlayers|0|When on, the highest bot level follows the highest real player's level.
bots.disable_random_levels|Bot Balance|Same starting level for all bots|bool|||conf:playerbots.conf:AiPlayerbot.DisableRandomLevels|0|When on, every new bot starts at the fixed starting level below instead of a random level.
bots.starting_level|Bot Balance|Bot starting level|int|1|80|conf:playerbots.conf:AiPlayerbot.RandombotStartingLevel|1|The level new bots start at when random levels are off.
bots.min_level_chance|Bot Balance|Chance of a lowest-level bot|float|0|1|conf:playerbots.conf:AiPlayerbot.RandomBotMinLevelChance|0.1|Chance a new bot is created at the lowest level. 0.1 means 10 percent.
bots.max_level_chance|Bot Balance|Chance of a highest-level bot|float|0|1|conf:playerbots.conf:AiPlayerbot.RandomBotMaxLevelChance|0.1|Chance a new bot is created at the highest level. 0.1 means 10 percent.
bots.fixed_level|Bot Balance|Freeze bot levels|bool|||conf:playerbots.conf:AiPlayerbot.RandomBotFixedLevel|0|When on, bots keep their level forever and never level up.
bots.xp_rate|Bot Balance|Bot leveling speed|float|0.5|20|conf:playerbots.conf:AiPlayerbot.RandomBotXPRate|1.0|Multiplies how fast bots level, on top of the server XP rate.
bots.join_lfg|Bot Behavior|Bots queue for dungeons|bool|||conf:playerbots.conf:AiPlayerbot.RandomBotJoinLfg|1|When on, bots use the dungeon finder and fill group slots.
bots.join_bg|Bot Behavior|Bots join battlegrounds|bool|||conf:playerbots.conf:AiPlayerbot.RandomBotJoinBG|1|When on, bots queue for battlegrounds and arenas so PvP fights happen.
bots.auto_quests|Bot Behavior|Bots do quests|bool|||conf:playerbots.conf:AiPlayerbot.AutoDoQuests|1|When on, bots pick up and complete quests on their own.
bots.auto_equip_loot|Bot Behavior|Bots equip looted upgrades|bool|||conf:playerbots.conf:AiPlayerbot.AutoEquipUpgradeLoot|1|When on, bots put on items they loot when those are upgrades.
bots.auto_gear|Bot Behavior|Bots get gear on level-up|bool|||conf:playerbots.conf:AiPlayerbot.AutoUpgradeEquip|1|When on, bots automatically receive level-appropriate equipment upgrades.
bots.auto_talents|Bot Behavior|Bots pick talents|bool|||conf:playerbots.conf:AiPlayerbot.AutoPickTalents|1|When on, bots spend their talent points automatically on level-up.
bots.auto_trainer_spells|Bot Behavior|Bots learn trainer spells|bool|||conf:playerbots.conf:AiPlayerbot.AutoLearnTrainerSpells|1|When on, bots learn their class spells automatically on level-up.
bots.auto_quest_spells|Bot Behavior|Bots learn quest spells|bool|||conf:playerbots.conf:AiPlayerbot.AutoLearnQuestSpells|1|When on, bots learn class quest reward spells automatically.
bots.trading|Bot Behavior|Bot trading|int|0|3|conf:playerbots.conf:AiPlayerbot.EnableRandomBotTrading|1|0 off, 1 buy and sell, 2 only buy, 3 only sell.
bots.mail|Bot Behavior|Bots can send mail|bool|||conf:playerbots.conf:AiPlayerbot.BotSendMailEnabled|1|When on, bots can mail items or money when asked in chat.
bots.talk|Bot Chat|Bots chat|bool|||conf:playerbots.conf:AiPlayerbot.RandomBotTalk|1|When on, bots talk in say, yell and general chat.
bots.emote|Bot Chat|Bots use emotes|bool|||conf:playerbots.conf:AiPlayerbot.RandomBotEmote|0|When on, bots wave, dance and cheer now and then.
bots.suggest_dungeons|Bot Chat|Bots suggest dungeons|bool|||conf:playerbots.conf:AiPlayerbot.RandomBotSuggestDungeons|1|When on, bots suggest dungeon runs in chat.
bots.greet|Bot Chat|Bots greet on invite|bool|||conf:playerbots.conf:AiPlayerbot.EnableGreet|0|When on, bots say hello when invited to a group.
bots.broadcasts|Bot Chat|Bot world chatter|bool|||conf:playerbots.conf:AiPlayerbot.EnableBroadcasts|1|When on, bots announce loot, levels and events in the world channels.
bots.guild_chat|Bot Chat|Bots chat in guilds|bool|||conf:playerbots.conf:AIPlayerbot.GuildFeedback|1|When on, guild member bots comment on guild events. Note the key really is spelled AIPlayerbot in playerbots.conf.
bots.active_alone|Bot Performance|Active bots with no players nearby|int|0|100|conf:playerbots.conf:AiPlayerbot.BotActiveAlone|10|Roughly what percent of bots stay active when no real player is around. Higher feels livelier but uses more CPU.
bots.active_rotation|Bot Performance|Active bot rotation seconds|int|5|3600|conf:playerbots.conf:AiPlayerbot.BotActiveAloneDurationSeconds|30|How often a different set of bots takes its active turn.
bots.smart_scale|Bot Performance|Auto-reduce bots under load|bool|||conf:playerbots.conf:AiPlayerbot.botActiveAloneSmartScale|1|When on, the server quietly idles bots when it is struggling to keep up.
bots.force_radius|Bot Performance|Always-active radius in yards|int|0|1000|conf:playerbots.conf:AiPlayerbot.BotActiveAloneForceWhenInRadius|150|Bots within this many yards of a real player are always active. 0 disables.
bots.force_zone|Bot Performance|Always active in your zone|bool|||conf:playerbots.conf:AiPlayerbot.BotActiveAloneForceWhenInZone|1|When on, bots in the same zone as a real player are always active.
bots.force_guild|Bot Performance|Always active in your guild|bool|||conf:playerbots.conf:AiPlayerbot.BotActiveAloneForceWhenInGuild|1|When on, bots that share a guild with a real player are always active.
ahbot.seller|Auction House|Auction seller bot|bool|||conf:mod_ahbot.conf:AuctionHouseBot.EnableSeller|0|When on, the auction house is stocked with items for sale.
ahbot.buyer|Auction House|Auction buyer bot|bool|||conf:mod_ahbot.conf:AuctionHouseBot.EnableBuyer|0|When on, the bot bids on auctions that players list.
ahbot.character|Auction House|Seller character|char|||conf:mod_ahbot.conf:AuctionHouseBot.GUID|0|Which character appears as the auction seller. Saving also writes the matching account id. Shown as the stored character id.
ahbot.items_per_cycle|Auction House|Listings added per cycle|int|1|1000|conf:mod_ahbot.conf:AuctionHouseBot.ItemsPerCycle|200|How many auctions the bot adds or removes per pass. Higher fills the auction house faster.
ahbot.duplicates|Auction House|Max duplicate stacks|int|0|100|conf:mod_ahbot.conf:AuctionHouseBot.DuplicatesCount|0|Most stacks of the same item the bot may sell at once. 0 means no limit.
ahbot.duration|Auction House|Auction duration class|int|0|2|conf:mod_ahbot.conf:AuctionHouseBot.ElapsingTimeClass|1|How long bot auctions last. 0 = 1 to 3 days, 1 = 1 to 24 hours, 2 = 10 to 60 minutes.
ahbot.vendor_items|Auction House|Sell vendor items|bool|||conf:mod_ahbot.conf:AuctionHouseBot.VendorItems|0|When on, the bot also lists items you could simply buy from vendors.
ahbot.loot_items|Auction House|Sell loot items|bool|||conf:mod_ahbot.conf:AuctionHouseBot.LootItems|1|When on, the bot lists items that drop as loot or from fishing.
ahbot.loot_trade_goods|Auction House|Sell loot trade goods|bool|||conf:mod_ahbot.conf:AuctionHouseBot.LootTradeGoods|1|When on, the bot lists trade goods like ore, herbs, leather and cloth.
ahbot.boe_items|Auction House|Sell bind-on-equip gear|bool|||conf:mod_ahbot.conf:AuctionHouseBot.Bind_When_Equipped|1|When on, the bot lists gear that binds when equipped - most green and blue items.
ahbot.max_item_level|Auction House|Highest item level sold|int|0|300|conf:mod_ahbot.conf:AuctionHouseBot.DisableItemsAboveLevel|0|The bot skips items above this item level. 0 means no cap.
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

# CFG_ENV_MAP: an in-process snapshot of the override's ac-worldserver
# environment (KEY -> value), populated by _cfg_env_load_map. While
# CFG_ENV_MAP_LOADED is 1, _cfg_env_read resolves here instead of forking yq
# per call -- `config list` does one env read per registry row (~65), and a
# yq fork each made every page load laggy.
declare -gA CFG_ENV_MAP=()
CFG_ENV_MAP_LOADED=0

# _cfg_env_load_map: dump the override's ac-worldserver environment ONCE (a
# single yq fork) into CFG_ENV_MAP, then flip CFG_ENV_MAP_LOADED so subsequent
# _cfg_env_read calls resolve in-process. Safe when the file/section is absent
# (an empty map is a valid answer). A caller that later MUTATES the override
# must _cfg_env_unload_map so reads see fresh data.
_cfg_env_load_map() {
    CFG_ENV_MAP=()
    CFG_ENV_MAP_LOADED=1
    [[ -f "$cfg_ovr" ]] || return 0
    local line k v
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ -z "$line" ]] && continue
        k="${line%%=*}"; v="${line#*=}"
        [[ -n "$k" ]] && CFG_ENV_MAP["$k"]="$v"
    done < <("$DML_YQ_BIN" -r '.services.ac-worldserver.environment // {} | to_entries[] | .key + "=" + (.value | tostring)' "$cfg_ovr" 2>/dev/null || true)
    return 0
}

# _cfg_env_unload_map: drop the snapshot so _cfg_env_read forks yq again.
_cfg_env_unload_map() {
    CFG_ENV_MAP=()
    CFG_ENV_MAP_LOADED=0
    return 0
}

# _cfg_env_read <ENV>: echoes the override's value for that env key, or "".
# Resolves against the CFG_ENV_MAP snapshot when one is loaded (identical
# answer, no fork); otherwise forks yq as before.
_cfg_env_read() {
    if [[ "${CFG_ENV_MAP_LOADED:-0}" == 1 ]]; then
        printf '%s' "${CFG_ENV_MAP[$1]:-}"
        return 0
    fi
    [[ -f "$cfg_ovr" ]] || { printf ''; return 0; }
    E="$1" "$DML_YQ_BIN" -r '.services.ac-worldserver.environment[strenv(E)] // ""' "$cfg_ovr" 2>/dev/null || printf ''
    return 0
}

# _cfg_env_read_var <ENV>: NO-FORK sibling of _cfg_env_read -- returns via the
# global REPLY instead of stdout, so hot per-row emitters can call it without a
# `$()` subshell. When the CFG_ENV_MAP snapshot is loaded (the config-list case)
# this is pure parameter expansion, zero forks. Falls back to the forking
# _cfg_env_read only when no snapshot is loaded (identical answer either way).
_cfg_env_read_var() {
    if [[ "${CFG_ENV_MAP_LOADED:-0}" == 1 ]]; then
        REPLY="${CFG_ENV_MAP[$1]:-}"
        return 0
    fi
    REPLY="$(_cfg_env_read "$1")"
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

# _cfg_env_frozen <ENV>: 0 when the RUNNING ac-worldserver container carries
# this legacy AC_* variable in its creation-time environment.
#
# The override.yml read above answers "is the override still on disk", which
# is NOT the same question as "will a `reload config` actually take effect".
# A container keeps the environment it was CREATED with: cleaning the key out
# of override.yml does nothing to the running process, and AC's env bridge
# beats conf values, so the world keeps serving the frozen number until a
# compose recreate. Save #1 removes the key and correctly says "restart";
# save #2 would otherwise see a clean file, report applied:"live" and lie --
# the effective rate never moved. Asking docker closes that gap.
#
# Degrades to 1 (not frozen) when docker is down or the container is absent:
# with no container there is no frozen env to beat the conf, and the SOAP
# reload that gates the live claim cannot succeed then either.
_cfg_env_frozen() {
    local envs
    envs="$(docker inspect -f '{{range .Config.Env}}{{println .}}{{end}}' ac-worldserver 2>/dev/null)" || return 1
    printf '%s\n' "$envs" | grep -q "^$1=" || return 1
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

# _cfg_conf_path_var <file>: NO-FORK sibling of _cfg_conf_path -- returns via
# the global REPLY so hot emitters skip the `$()` subshell.
_cfg_conf_path_var() {
    case "$1" in
        worldserver.conf|authserver.conf) REPLY="$cfg_sdir/env/dist/etc/$1" ;;
        *) REPLY="$cfg_sdir/env/dist/etc/modules/$1" ;;
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

# _cfg_unquote <s>: strips ONE matching pair of surrounding double quotes,
# leaving a bare or unbalanced value untouched. Symmetric on purpose -- used
# to normalize/compare conf values regardless of quoting style.
_cfg_unquote() {
    local s="${1-}"
    if [[ ${#s} -ge 2 && "${s:0:1}" == '"' && "${s: -1}" == '"' ]]; then
        s="${s:1:${#s}-2}"
    fi
    printf '%s' "$s"
    return 0
}

# _cfg_conf_read_raw <path> <Key>: the conf's value for Key, quoting PRESERVED
# ("" when the file or key is absent). Exact-key match (prefix + '='), comments
# skipped, LAST occurrence wins (AC semantics). The key travels via the
# environment (K=), never awk -v, so dots/backslashes are never escape-processed.
_cfg_conf_read_raw() {
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
    printf '%s' "$val"
    return 0
}

# _cfg_conf_read <path> <Key>: like _cfg_conf_read_raw but with surrounding
# quotes stripped -- the common read used by config list/env fallbacks.
_cfg_conf_read() {
    local val
    val="$(_cfg_conf_read_raw "$1" "$2")"
    val="${val%\"}"; val="${val#\"}"
    printf '%s' "$val"
    return 0
}

# --- batched, NO-FORK conf reads (hot config-list path) --------------------
# A per-row `$(_cfg_conf_read file key)` forks ~3 subshells (outer subst ->
# _cfg_conf_read_raw subst -> awk), so a ~65-row config list forks hundreds of
# times -- ~165ms each on native Git Bash. _cfg_conf_load_file scans a conf ONCE
# with a pure-bash `while read` loop (ZERO extra forks) into CFG_CONF_RAW, keyed
# by "<path>\x1f<Key>" with the RAW (quote-preserved) value, LAST occurrence
# wins. _cfg_conf_get_var then resolves per-row lookups in-process.
#
# The loop reproduces _cfg_conf_read_raw's value semantics EXACTLY for the keys
# the registry looks up (which are all [A-Za-z0-9_.]+): a line matches when,
# after stripping a trailing CR and leading blanks, it is `<Key>[blank]*= ...`;
# the value is everything after the first `=`, trimmed of leading/trailing
# blanks (spaces and tabs), quotes preserved. That is identical to matching a
# specific key with _cfg_conf_read_raw, because the awk there also requires the
# key to sit at column 0 (post leading-trim) immediately followed by optional
# blanks then `=`, and the pre-`=` token trims to exactly that key. A bats test
# pins a conf-backed config-list row against the old path.
declare -gA CFG_CONF_RAW=()
declare -gA CFG_CONF_FILE_DONE=()
_cfg_conf_load_file() {
    [[ -n "${CFG_CONF_FILE_DONE[$1]:-}" ]] && return 0
    CFG_CONF_FILE_DONE[$1]=1
    [[ -f "$1" ]] || return 0
    local line s pre v re_line='^[[:blank:]]*[A-Za-z0-9_.]+[[:blank:]]*='
    while IFS= read -r line || [[ -n "$line" ]]; do
        s="${line%$'\r'}"
        [[ "$s" =~ $re_line ]] || continue
        [[ "$s" =~ ^[[:blank:]]+ ]] && s="${s#"${BASH_REMATCH[0]}"}"
        pre="${s%%=*}"
        v="${s#*=}"
        [[ "$pre" =~ [[:blank:]]+$ ]] && pre="${pre%"${BASH_REMATCH[0]}"}"
        [[ "$v" =~ ^[[:blank:]]+ ]] && v="${v#"${BASH_REMATCH[0]}"}"
        [[ "$v" =~ [[:blank:]]+$ ]] && v="${v%"${BASH_REMATCH[0]}"}"
        CFG_CONF_RAW["$1"$'\x1f'"$pre"]="$v"
    done < "$1"
    return 0
}

# _cfg_conf_get_var <path> <Key>: NO-FORK equivalent of `$(_cfg_conf_read ...)`.
# Loads the file into CFG_CONF_RAW on first touch, then resolves in-process and
# returns the quote-stripped value via REPLY ("" when file or key is absent --
# same as _cfg_conf_read, so an empty answer still triggers the caller's .dist
# fallback). Read-only path ONLY: the cache is never invalidated, so mutating
# code must keep using _cfg_conf_read/_cfg_conf_write.
_cfg_conf_get_var() {
    _cfg_conf_load_file "$1"
    local v="${CFG_CONF_RAW["$1"$'\x1f'"$2"]:-}"
    v="${v%\"}"; v="${v#\"}"
    REPLY="$v"
    return 0
}

# _cfg_conf_write <path> <Key> <value>: comment-preserving in-place edit --
# replaces every active `Key = ...` line (duplicates collapse to the same
# value; AC reads the last anyway) or appends `Key = value` when absent.
# tmp-file + mv like raw-write, so a failure never truncates the conf.
# Sets CFG_CHANGED=true when the EFFECTIVE value actually changed.
#
# Quotes are normalized SYMMETRICALLY before the compare, so a pure quote
# toggle (foo <-> "foo") is a no-op that never flips restart_required. The
# written value preserves quoting style -- it stays quoted when the user
# quoted the new value OR the stored line was quoted -- so a legitimate edit
# of a value that needs quotes (spaces etc.) never silently drops them.
_cfg_conf_write() {
    local curq cur newq new out_val tmp
    curq="$(_cfg_conf_read_raw "$1" "$2")"
    cur="$(_cfg_unquote "$curq")"
    newq="$3"
    new="$(_cfg_unquote "$newq")"
    [[ "$cur" == "$new" ]] && return 0
    if [[ "$newq" != "$new" || "$curq" != "$cur" ]]; then
        out_val="\"$new\""
    else
        out_val="$new"
    fi
    tmp="$1.tmp.$$"
    K="$2" V="$out_val" awk '
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

# _pb_kv_lines <file>: every active `Key = value` line as
# key<US>value<US>lineno rows (US = 0x1f, never appears in conf values).
# Comments and non-assignment lines are skipped; the caller dedupes.
_pb_kv_lines() {
    awk '
        {
            s = $0; sub(/\r$/, "", s)
            if (s !~ /^[ \t]*[A-Za-z0-9_.]+[ \t]*=/) next
            sub(/^[ \t]+/, "", s)
            eq = index(s, "=")
            k = substr(s, 1, eq - 1); sub(/[ \t]+$/, "", k)
            v = substr(s, eq + 1); sub(/^[ \t]+/, "", v); sub(/[ \t]+$/, "", v)
            printf "%s\x1f%s\x1f%d\n", k, v, NR
        }
    ' "$1" 2>/dev/null || true
    return 0
}

# _conf_help_lines <file>: per-key comment-block help for the conf-keys
# browser, as key<US>help lines (US = 0x1f; keys without help are omitted).
# Module authors document keys in the # comment blocks of their .conf.dist --
# two documentation styles exist in the wild and both are handled:
#
#   adjacent block (mod-learn-spells style): the contiguous # block directly
#     above the key, blank lines between block and key allowed (a NEW block
#     after a blank replaces the old one, so section headers don't bleed in).
#   shared doc block (mod-transmog / AC core style): ONE big block documents
#     many keys, each entry introduced by a `#    Key.Name` header line; the
#     lines after a header (until the next header / end of block) become that
#     key's help. Wins over the adjacent block when both match.
#
# Decoration-only lines (####, lone #) are dropped, whitespace is squeezed,
# lines join with single spaces, and each help is capped at 400 chars.
_conf_help_lines() {
    [[ -f "$1" ]] || return 0
    awk '
        { n++; line[n] = $0 }
        END {
            US = sprintf("%c", 31)
            # pass 1: the keys this conf actually assigns (header detection
            # matches ONLY real keys, so prose is never mistaken for one)
            for (i = 1; i <= n; i++) {
                s = line[i]; sub(/\r$/, "", s); sub(/^[ \t]+/, "", s)
                if (s ~ /^[A-Za-z0-9_.]+[ \t]*=/) {
                    eq = index(s, "=")
                    k = substr(s, 1, eq - 1); sub(/[ \t]+$/, "", k)
                    if (!(k in keyseen)) { keyseen[k] = 1; korder[++nk] = k }
                }
            }
            # pass 2: adjacent blocks (buf) + per-key header slices (hdr)
            prev = "start"; buf = ""; cap = ""
            for (i = 1; i <= n; i++) {
                s = line[i]; sub(/\r$/, "", s)
                t = s; sub(/^[ \t]+/, "", t)
                if (t ~ /^#/) {
                    txt = t; sub(/^#+[ \t]*/, "", txt); sub(/[ \t]+$/, "", txt)
                    gsub(/[ \t]+/, " ", txt)
                    if (prev != "comment") buf = ""
                    if (txt in keyseen) {
                        cap = txt
                    } else if (txt != "" && txt ~ /[A-Za-z0-9]/) {
                        if (cap != "") hdr[cap] = (hdr[cap] == "" ? txt : hdr[cap] " " txt)
                        buf = (buf == "" ? txt : buf " " txt)
                    }
                    prev = "comment"
                } else if (t == "") {
                    # blank keeps the block for the key below, ends a slice
                    cap = ""; prev = "blank"
                } else if (t ~ /^[A-Za-z0-9_.]+[ \t]*=/) {
                    eq = index(t, "=")
                    k = substr(t, 1, eq - 1); sub(/[ \t]+$/, "", k)
                    if (buf != "") adj[k] = buf
                    buf = ""; cap = ""; prev = "key"
                } else {
                    buf = ""; cap = ""; prev = "other"
                }
            }
            for (i = 1; i <= nk; i++) {
                k = korder[i]
                h = (k in hdr && hdr[k] != "") ? hdr[k] : ((k in adj) ? adj[k] : "")
                if (length(h) > 400) h = substr(h, 1, 400)
                if (h != "") printf "%s%s%s\n", k, US, h
            }
        }
    ' "$1" 2>/dev/null || true
    return 0
}

# _conf_reload_cmd <conf-basename>: the owning module's known live-reload
# console command, or "" when none is known. Deliberately tiny and honest:
# only commands VERIFIED against the module's own docs belong here
# (mod-transmog ships `.transmog reload` -- see its cheat-sheet block in
# 47-commands.sh). Everything else stays restart-to-apply; do NOT invent
# reload commands for other modules.
_conf_reload_cmd() {
    case "$1" in
        transmog.conf) printf 'transmog reload' ;;
        *) : ;;
    esac
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

# _cfg_env_name_for_var <ConfKey>: NO-FORK sibling of _cfg_env_name_for --
# identical derivation, returns via the global REPLY so hot emitters skip the
# `$()` subshell.
_cfg_env_name_for_var() {
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
    REPLY="AC_${out^^}"
    return 0
}

# _float_in_range <val> <min> <max>: 0 iff val is a decimal in [min,max].
_float_in_range() {
    [[ "$1" =~ ^[0-9]+([.][0-9]+)?$ ]] || return 1
    awk -v v="$1" -v lo="$2" -v hi="$3" 'BEGIN { exit !(v >= lo && v <= hi) }'
}

# _cfg_file_path <name>: maps an editable file name to its host path under
# $cfg_sdir (the base compose bind-mounts ./env/dist/etc into the container,
# so module confs are ordinary host files). Unknown name -> rc 1.
# Batch 1 F3 replaced the hardcoded 3-conf allowlist with a dynamic one:
# ANY <name>.conf whose basename-shape regex passes (the regex IS the
# traversal guard -- no slashes can match, so the name can never leave the
# modules dir) AND whose conf or .dist already exists under
# env/dist/etc/modules/, plus worldserver.conf/authserver.conf one dir up
# (fixed names, same literal-match safety as before). .env and the compose
# override stay read-only in raw-write (guarded there, not here).
_cfg_file_path() {
    local p
    case "$1" in
        .env) printf '%s' "$cfg_sdir/.env" ;;
        docker-compose.override.yml) printf '%s' "$cfg_sdir/docker-compose.override.yml" ;;
        worldserver.conf|authserver.conf) printf '%s' "$cfg_sdir/env/dist/etc/$1" ;;
        *)
            [[ "$1" =~ ^[A-Za-z0-9_.-]+\.conf$ ]] || return 1
            p="$cfg_sdir/env/dist/etc/modules/$1"
            [[ -f "$p" || -f "$p.dist" ]] || return 1
            printf '%s' "$p"
            ;;
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

# _boot_loop_note <new-restarts> [container]: the diagnosis line a readiness
# wait emits once it has established (from a climbing .RestartCount) that
# the world is CRASH-RETRYING rather than slow-booting -- incident follow-up 2.
# On the night of the 2026-07-21 incident the world retried on "Can't connect to
# MySQL (110)" for ten minutes while the wait printed "still waiting ... bots
# respawning takes a while...".
#
# Rust twin: `lifecycle::boot_loop_note`. The two texts must stay
# byte-identical (CLAUDE.md: a new NDJSON line lands on BOTH surfaces or the
# parity suites diverge).
#
# The log scan only ever NAMES a cause; it never establishes the loop. Two hits
# minimum, because a single connect failure is normal on a cold start (the
# world races the database container's own boot and retries). `--tail 200` is a
# few crash cycles' worth, and `grep -c` reads to EOF so it cannot SIGPIPE
# `docker logs` the way an early-exiting `grep -q`/`-m1` would under pipefail.
_boot_loop_note() {
    local n="$1" c="${2:-ac-worldserver}" hits head
    hits="$(timeout -k 5 10 docker logs --tail 200 "$c" 2>&1 | grep -icE "could not connect to mysql|can't connect to mysql" || true)"
    head="boot loop detected: the world server has restarted $n times since this wait began -- it is crash-retrying, not slow-booting."
    if [[ "${hits:-0}" -ge 2 ]]; then
        printf '%s Its log shows repeated MySQL connection errors, so the world cannot reach the database. Try Restart Docker (Tools), then start the server again.' "$head"
    else
        printf '%s Check the Console log for the boot error; if it shows database connection errors, try Restart Docker (Tools), then start the server again.' "$head"
    fi
}

# --- the shared boot-loop tracker ------------------------------------------
# ONE implementation of the decision, used by EVERY readiness wait on the
# start/restart path:
#   * `wow world-restart`'s `until _world_ready` loop (90-main.sh), and
#   * `games start|restart`'s watch over the `dml-start.sh` hook
#     (`_stream_cmd_bootwatch`, 90-main.sh) -- the path Home's primary Start
#     and Restart buttons take, which is where the 2026-07-21 incident
#     actually happened.
# A second copy with its own threshold or wording is the failure mode this
# exists to prevent (round-2 findings G3/G9). Rust twin:
# `lifecycle::BootLoopWatch`.

# How many restarts NEW SINCE THE WAIT BEGAN make it a boot loop rather than a
# slow boot. WHY 3: docker's restart policy increments .RestartCount only
# when it revives a container that DIED, so a healthy boot -- however slow,
# however many bots it is creating -- never increments it at all and even ONE
# is already abnormal. Three is about tolerating a ONE-OFF (a single OOM-kill
# the next boot survives is a hiccup, not a loop); crying wolf would train
# users to ignore the line. Rust twin: `BOOT_LOOP_RESTART_STRIKES`.
DML_BOOT_LOOP_STRIKES=3

# Seconds between polls for the watch that runs ALONGSIDE a streaming child
# (the world-restart arm has its own 2s readiness cadence and does not use
# this). 15s is ~1/100th of the 1800s budget it watches -- fine-grained enough
# to name a loop minutes before the timeout, cheap enough that a 30-minute cold
# start costs ~120 `docker inspect` calls. Test-only override seam, same shape
# as DML_READY_TIMEOUT_SECS; a non-numeric value falls back to the default
# rather than making `read -t` fail.
_boot_loop_poll_secs() {
    local p="${DML_BOOT_LOOP_POLL_SECS:-15}"
    [[ "$p" =~ ^[1-9][0-9]*$ ]] || p=15
    echo "$p"
}

# Tracker state. Module-level (not local) because the two callers drive it
# across many polls; _boot_loop_reset is mandatory before each wait.
_BOOT_LOOP_BASE=""
_BOOT_LOOP_NOTED=0
BOOT_LOOP_NOTE=""
_boot_loop_reset() { _BOOT_LOOP_BASE=""; _BOOT_LOOP_NOTED=0; BOOT_LOOP_NOTE=""; return 0; }

# _boot_loop_check <container>: ONE poll. Returns 0 exactly ONCE per wait -- on
# the poll that proves the loop, with the diagnosis line in the global
# BOOT_LOOP_NOTE -- and 1 on every other poll. ADVISORY ONLY: no caller changes
# its outcome or exit code because of it.
#
# CALL IT WITHOUT `$(...)`, the same no-fork convention `json_escape_var` uses
# and for a harder reason: a command substitution runs this in a SUBSHELL, so
# the baseline and the latch would be discarded after every poll -- the tracker
# would re-baseline forever and could never reach the threshold.
#
# TRI-STATE, the lesson every probe in this codebase has now learned the hard
# way: an unreadable/non-numeric reading is DOCKER FAILING TO ANSWER, not zero
# restarts. It is skipped entirely -- it neither sets nor resets the baseline
# -- because collapsing it to 0 fabricates a loop on a long-lived server and
# re-baselining on it hides a real one on the wedged daemon this feature
# exists for. An empty container argument is the same kind of non-answer.
#
# The baseline is the FIRST READABLE reading rather than a fixed zero, so a
# server carrying hundreds of historical restarts can never trip it. A reading
# BELOW the baseline can only mean the container was RECREATED (a fresh
# container starts at 0), which re-baselines: `games restart` recreates
# containers mid-wait via compose, and measuring a new container against the
# old one's count would blind the watch for the rest of the boot.
#
# The inspect is time-bounded: this runs inline in a stream reader, so a
# dockerd that accepts the socket and then never answers would stall the
# child's output instead of just missing a poll.
_boot_loop_check() {
    local c="${1:-}" rc delta
    BOOT_LOOP_NOTE=""
    [[ -n "$c" ]] || return 1
    (( _BOOT_LOOP_NOTED == 0 )) || return 1
    rc="$(timeout -k 5 10 docker inspect -f '{{.RestartCount}}' "$c" 2>/dev/null || true)"
    rc="${rc%%$'\n'*}"; rc="${rc//$'\r'/}"
    [[ "$rc" =~ ^[0-9]+$ ]] || return 1
    if [[ -z "$_BOOT_LOOP_BASE" ]]; then _BOOT_LOOP_BASE="$rc"; return 1; fi
    if (( rc < _BOOT_LOOP_BASE )); then _BOOT_LOOP_BASE="$rc"; return 1; fi
    delta=$(( rc - _BOOT_LOOP_BASE ))
    (( delta >= DML_BOOT_LOOP_STRIKES )) || return 1
    _BOOT_LOOP_NOTED=1
    # This substitution is safe: _boot_loop_note is pure output, no state.
    BOOT_LOOP_NOTE="$(_boot_loop_note "$delta" "$c")"
    return 0
}

# --- `wow bots flush` helpers (Batch 1 F4) ---------------------------------

# _flush_restart_authworld <sdir> <label>: one staged auth+world restart,
# reusing the exact `games restart` internals (guides/wow-wotlk/dml-start.sh
# semantics): saveall best-effort -> compose stop -t 180 (graceful, chars
# saved) -> compose up -d --no-deps (recreate so conf/env changes apply,
# WITHOUT re-running the one-shot db-import/client-data init) -> readiness
# wait on the boot-complete marker (_world_ready). Streams progress lines in
# --json mode. Returns 0 ready / 1 compose failed / 2 readiness timeout
# (budget DML_READY_TIMEOUT_SECS, default 1800s -- bot deletion/creation
# happens DURING boot and can take many minutes).
_flush_restart_authworld() {
    local sdir="$1" label="$2" timeout="${DML_READY_TIMEOUT_SECS:-1800}" t0=$SECONDS last_note=0 elapsed
    [[ "$DML_JSON" == 1 ]] && ndjson_line info "saving all characters (best effort)..."
    soap_exec 'saveall' >/dev/null 2>&1 || true
    [[ "$DML_JSON" == 1 ]] && ndjson_line info "stopping auth + world ($label)..."
    (cd "$sdir" && docker compose stop -t 180 ac-worldserver ac-authserver >/dev/null 2>&1) || return 1
    [[ "$DML_JSON" == 1 ]] && ndjson_line info "starting auth + world (compose, no deps)..."
    (cd "$sdir" && docker compose up -d --no-deps ac-authserver ac-worldserver >/dev/null 2>&1) || return 1
    [[ "$DML_JSON" == 1 ]] && ndjson_line info "waiting for the world ($label)..."
    while :; do
        _world_ready && return 0
        elapsed=$(( SECONDS - t0 ))
        (( elapsed >= timeout )) && return 2
        if (( elapsed - last_note >= 60 )); then
            last_note=$elapsed
            [[ "$DML_JSON" == 1 ]] && ndjson_line info "still waiting (~$(( elapsed / 60 ))m) - deleting/creating thousands of bots takes a while..."
        fi
        sleep 2
    done
}

# Trap hook for `wow bots flush`: however the flow dies after the delete
# flag was armed, the flag goes back to 0 -- otherwise EVERY later boot
# would silently wipe the bots again. The arm clears FLUSH_RESTORE_CONF
# once it restores the flag itself.
#
# Wired to EXIT *and* HUP/INT/TERM/PIPE (see the flush arm): bash runs the
# EXIT trap on a normal/`exit`/set -e death but NOT when an untrapped fatal
# signal kills the shell -- and that is exactly how this dies in the field
# (the launcher closing kills the wsl.exe child tree, a stream consumer
# going away raises SIGPIPE on stdout). The armed window spans a whole
# bot-deletion boot, up to DML_READY_TIMEOUT_SECS.
_flush_restore_flag() {
    [[ -n "${FLUSH_RESTORE_CONF:-}" ]] || return 0
    _cfg_conf_write "$FLUSH_RESTORE_CONF" "AiPlayerbot.DeleteRandomBotAccounts" "0" || true
    rm -f "$(_flush_marker_for "$FLUSH_RESTORE_CONF")" 2>/dev/null || true
    return 0
}

# Signal variant: restore, then re-raise with the default handler so the
# exit status still reflects the signal (128+n) for whoever is watching.
_flush_restore_flag_signal() {
    local sig="$1"
    _flush_restore_flag
    trap - "$sig"
    kill -s "$sig" $$ 2>/dev/null || true
    return 0
}

# On-disk breadcrumb next to the server dir, written BEFORE the flag is
# armed and removed only once it is back to 0. SIGKILL and power loss are
# not trappable at all, so the trap above cannot be the last line of
# defence: the marker lets the next start/restart/flush notice a flag that
# survived and heal it before the server boots and wipes the bots again.
# <conf> is the playerbots.conf path; the marker lives at the server-dir
# root (conf is always <sdir>/env/dist/etc/modules/playerbots.conf).
_flush_marker_for() {
    local conf="$1" sdir
    sdir="${conf%/env/dist/etc/modules/playerbots.conf}"
    [[ "$sdir" != "$conf" ]] || return 0
    printf '%s\n' "$sdir/.dml-bot-flush-armed"
    return 0
}

# Self-heal: if <sdir> carries an arm marker, a previous flush died before
# it could restore the flag. Force it back to 0 and drop the marker. Echoes
# a one-line note when it healed something, nothing otherwise; never fails.
_flush_heal_flag() {
    local sdir="$1" conf marker
    conf="$sdir/env/dist/etc/modules/playerbots.conf"
    marker="$sdir/.dml-bot-flush-armed"
    [[ -f "$marker" ]] || return 0
    if [[ -f "$conf" ]]; then
        _cfg_conf_write "$conf" "AiPlayerbot.DeleteRandomBotAccounts" "0" || true
    fi
    rm -f "$marker" 2>/dev/null || true
    printf '%s\n' "an interrupted bot flush had left the bot-delete flag armed - reset to 0 so this boot keeps your bots"
    return 0
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
        rows="$(db_chars_query "SELECT COUNT(*) FROM characters WHERE online = 1 AND $(_bot_account_where account);")" || true
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
                    # Anchor the key with a following `=` so it matches ONLY
                    # AiPlayerbot.MaxRandomBots and NOT longer keys that share
                    # the prefix, e.g. AiPlayerbot.MaxRandomBotsPriceChangeInterval
                    # (= 172800), which `tail -n1` would otherwise pick as "max".
                    val="$(grep -E '^[[:space:]]*AiPlayerbot\.MaxRandomBots[[:space:]]*=' "$conf" 2>/dev/null | tail -n1)" || true
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

# --- guided module tuning (overnight Batch 3) ------------------------------
# Plain-language, curated activator controls for a handful of optional modules
# whose knobs are otherwise buried in a .conf or a deployed .lua script. Two
# backends share one surface (`config tuning-list` / `config tuning-set`):
#
#   conf : reuses the existing conf-row mechanism (_cfg_conf_path/_read/_write
#          + _cfg_conf_ensure) against the bind-mounted module .conf under
#          env/dist/etc/modules/. Read at server startup -> restart to apply.
#   lua  : a safe, comment/format-preserving line-replace of the DEPLOYED ALE
#          script under env/dist/etc/modules/lua_scripts/ (never the repo --
#          this family is cloned + deployed at install time). Applies live via
#          `.reload ale` (or a restart).
#
# Every write ships LOCKED behind the frontend `guided-config` flag (see
# launcher/src/lib/features.svelte.ts + docs/SMOKE-TESTS.md section 21).
#
# One row per knob:  key|backend|file|confkey|module|label|type|min|max|default|explain
# `key`     = stable composite id the GUI/CLI address the row by (module.knob).
# `backend` = conf | lua.
# `file`    = the .conf basename (conf) or the .lua basename (lua) as deployed.
# `confkey` = the exact key token in that file (BeastMaster.Enable,
#             UnlimitedAmmoNamespace.ENABLED, the bare table key DURATION, ...).
# `module`  = the plain module name (also the GUI card heading / group).
# `type`    = bool (1/0) | int (min..max) | list (comma-separated ids).
# `default` = the DISPLAY/JSON form of the value (bool -> 1/0). For a lua bool
#             this is translated to true/false on write and back on read.
# Keys/defaults are the upstream .conf.dist / .lua HEADs, verified 2026-07-20
# (mod-npc-beastmaster, mod-learn-spells, Day36512/Acore_Lua_Unlimited_Ammo,
# Brytenwally/SitMeansRest).
#
# MIRRORED SNAPSHOT: crates/dml-wow/data/tuning-registry.json embeds this
# registry for the native launcher + dml-wow-cli. Edited a row? Regenerate:
#   bash cli/dml wow config tuning-registry --json | jq .data.settings > crates/dml-wow/data/tuning-registry.json
# Skip it and the native path ships stale data -- crates/dml-wow/tests/tuning_parity.rs
# would catch it, but SKIPS (silently passes) on any machine without the
# native runtime at C:/Users/perzi/dml-native (i.e. CI and most dev boxes).
_mtune_rows() {
cat <<'EOF'
beastmaster.enable|conf|mod_npc_beastmaster.conf|BeastMaster.Enable|NPC Beastmaster|Enable the Beastmaster NPC|bool|||1|Master switch for the Beastmaster NPC that lets classes tame, stable and use hunter pets.
beastmaster.hunter_only|conf|mod_npc_beastmaster.conf|BeastMaster.HunterOnly|NPC Beastmaster|Hunters only|bool|||1|When on, only Hunters may use the Beastmaster. Turn it off to let every class tame pets.
beastmaster.allowed_classes|conf|mod_npc_beastmaster.conf|BeastMaster.AllowedClasses|NPC Beastmaster|Allowed classes|list|||0|Comma-separated class ids allowed to adopt pets (0 = all classes). Only used when Hunters-only is off.
beastmaster.min_level|conf|mod_npc_beastmaster.conf|BeastMaster.MinLevel|NPC Beastmaster|Minimum level|int|0|80|10|Level a character must reach before adopting a pet (0 = no requirement).
learnspells.enable|conf|mod_learnspells.conf|LearnSpells.Enable|Learn Spells on Level-up|Enable auto-learn|bool|||1|Master switch: characters learn their class spells automatically on level-up, no trainer visits.
learnspells.announce|conf|mod_learnspells.conf|LearnSpells.Announce|Learn Spells on Level-up|Announce at login|bool|||1|Show a short message at login telling the player auto-learn is active.
learnspells.on_first_login|conf|mod_learnspells.conf|LearnSpells.OnFirstLogin|Learn Spells on Level-up|Grant all spells on first login|bool|||0|Give a brand-new character every spell up to its level at once. Handy for instant-level servers.
learnspells.max_level|conf|mod_learnspells.conf|LearnSpells.MaxLevel|Learn Spells on Level-up|Learn up to level|int|1|80|80|Stop auto-learning spells past this level.
unlimitedammo.enabled|lua|UnlimitedAmmo.lua|UnlimitedAmmoNamespace.ENABLED|Unlimited Ammo|Enable unlimited ammo|bool|||0|Ships off. When on, Hunters' arrows and bullets refill automatically so they never run out.
unlimitedammo.max_ammo|lua|UnlimitedAmmo.lua|UnlimitedAmmoNamespace.MAX_AMMO|Unlimited Ammo|Ammo to keep stocked|int|1|100000|1000|How many arrows or bullets to top the quiver up to on each refill.
unlimitedammo.min_threshold|lua|UnlimitedAmmo.lua|UnlimitedAmmoNamespace.MIN_AMMO_THRESHOLD|Unlimited Ammo|Refill when below|int|1|100000|52|Top the ammo back up once it drops under this many.
sitmeansrest.duration|lua|SitMeansRest.lua|DURATION|Sit Means Rest|Rest duration (seconds)|int|1|86400|20|How long the sit-to-rest regen buff lasts.
sitmeansrest.regen_aura|lua|SitMeansRest.lua|REGEN_AURA|Sit Means Rest|Regen spell id|int|1|999999|25990|The spell applied while resting. 25990 restores health and mana at any level.
EOF
}

# Deployed ALE script path for a lua-backend tuning file.
_lua_cfg_path() { printf '%s' "$1/env/dist/etc/modules/lua_scripts/$2"; }

# _mtune_to_json <type> <fileval>: lua file value -> display/JSON form.
_mtune_to_json() {
    if [[ "$1" == bool ]]; then
        case "$2" in true) printf '1' ;; false) printf '0' ;; *) printf '%s' "$2" ;; esac
    else
        printf '%s' "$2"
    fi
    return 0
}

# _mtune_to_lua <type> <jsonval>: display/JSON form -> lua file value.
_mtune_to_lua() {
    if [[ "$1" == bool ]]; then
        case "$2" in 1) printf 'true' ;; 0) printf 'false' ;; *) printf '%s' "$2" ;; esac
    else
        printf '%s' "$2"
    fi
    return 0
}

# _lua_cfg_read <path> <key>: echoes the current file value of a `<key> = ...`
# assignment ("" when the file or an UNCOMMENTED key line is absent). Handles
# both column-0 namespaced keys (UnlimitedAmmoNamespace.ENABLED = false) and
# indented bare table keys with a trailing comma (    DURATION = 20,). The
# value token is everything after `=` up to the first whitespace/comma/
# semicolon/inline `--` comment; LAST occurrence wins (Lua load semantics).
# The key travels via the environment (K=), never awk -v, so its dots are
# never regex/escape-processed, and `index()` matches it literally.
_lua_cfg_read() {
    local val=""
    [[ -f "$1" ]] || { printf ''; return 0; }
    val="$(K="$2" awk '
        {
            s = $0; sub(/\r$/, "", s); sub(/^[ \t]+/, "", s)
            k = ENVIRON["K"]
            if (index(s, k) == 1) {
                rest = substr(s, length(k) + 1)
                if (rest ~ /^[ \t]*=/) {
                    sub(/^[ \t]*=[ \t]*/, "", rest)
                    tok = rest
                    sub(/[ \t].*$/, "", tok)
                    sub(/,.*$/, "", tok)
                    sub(/;.*$/, "", tok)
                    sub(/--.*$/, "", tok)
                    if (tok != "") { val = tok; found = 1 }
                }
            }
        }
        END { if (found) print val }
    ' "$1" 2>/dev/null)" || val=""
    printf '%s' "$val"
    return 0
}

# _lua_cfg_write <path> <key> <fileval>: replaces the value of the LAST
# uncommented `<key> = ...` line in place, preserving leading whitespace, the
# original spacing around `=`, and any trailer (a table comma and/or inline
# `-- comment`). Editing the LAST occurrence matches _lua_cfg_read (and Lua's
# own last-assignment-wins load semantics), so a write round-trips through a
# read -- targeting the FIRST occurrence instead would leave the effective
# (last) value unchanged and fail verification. tmp-file + verify + mv, like
# _cfg_conf_write, so a bad edit never truncates the file. Returns 1 when the
# key line is absent (caller maps that to NOT_FOUND) OR the patch cannot be
# verified. Sets MTUNE_CHANGED=true only when the value actually moved.
# <fileval> is already file-form (true/false or a validated integer), so the
# reconstructed line is safe.
_lua_cfg_write() {
    local cur tmp
    cur="$(_lua_cfg_read "$1" "$2")"
    [[ -z "$cur" ]] && return 1
    [[ "$cur" == "$3" ]] && return 0
    tmp="$1.tmp.$$"
    K="$2" V="$3" awk '
        function is_key_line(line,    s, lead, body, k) {
            s = line
            sub(/\r$/, "", s)
            lead = ""
            if (match(s, /^[ \t]+/)) { lead = substr(s, 1, RLENGTH) }
            body = substr(s, length(lead) + 1)
            k = ENVIRON["K"]
            if (index(body, k) != 1) { return 0 }
            return (substr(body, length(k) + 1) ~ /^[ \t]*=/)
        }
        function rebuild(line,    cr, s, lead, body, k, after, eqlen, eqpart, rest, vlen, trailer) {
            cr = ""
            s = line
            if (s ~ /\r$/) { cr = "\r"; sub(/\r$/, "", s) }
            lead = ""
            if (match(s, /^[ \t]+/)) { lead = substr(s, 1, RLENGTH) }
            body = substr(s, length(lead) + 1)
            k = ENVIRON["K"]
            after = substr(body, length(k) + 1)
            eqlen = 0
            if (match(after, /^[ \t]*=[ \t]*/)) { eqlen = RLENGTH }
            eqpart = substr(after, 1, eqlen)
            rest = substr(after, eqlen + 1)
            vlen = length(rest)
            if (match(rest, /[ \t,;]/)) { vlen = RSTART - 1 }
            trailer = substr(rest, vlen + 1)
            return lead k eqpart ENVIRON["V"] trailer cr
        }
        { lines[NR] = $0; if (is_key_line($0)) last = NR }
        END {
            for (i = 1; i <= NR; i++) {
                if (i == last) print rebuild(lines[i])
                else print lines[i]
            }
        }
    ' "$1" > "$tmp" || { rm -f "$tmp"; return 1; }
    if [[ "$(_lua_cfg_read "$tmp" "$2")" != "$3" ]]; then
        rm -f "$tmp"
        return 1
    fi
    mv "$tmp" "$1"
    MTUNE_CHANGED=true
    return 0
}
