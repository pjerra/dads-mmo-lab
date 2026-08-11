# ---------------------------------------------------------------------------
# Module management (Round C): registries + shared helpers.
# Three families ported from the DML server manager (guides/wow-wotlk/
# wow-manage.sh): cpp (C++ source modules -- SQL is NEVER hand-applied;
# AzerothCore's ac-db-import auto-applies and tracks it in the `updates`
# table, and hand-applying desyncs that tracking), lua (ALE scripts -- SQL
# IS hand-applied, that's normal for this family), sql (typed SQL mods).
# Registry row formats are the manager's own, verbatim.
# ---------------------------------------------------------------------------

# key|display name|git url|sql dirs (informational -- db-import applies them)
#
# MIRRORED SNAPSHOT: crates/dml-wow/data/module-catalog.json embeds these
# three registries (cpp here, plus lua/sql below) for the native launcher +
# dml-wow-cli. Edited any of them? Regenerate:
#   bash cli/dml wow module catalog --json | jq .data > crates/dml-wow/data/module-catalog.json
# Skip it and the native path ships stale data -- crates/dml-wow/tests/module_parity.rs
# would catch it, but SKIPS (silently passes) on any machine without the
# native runtime at C:/Users/perzi/dml-native (i.e. CI and most dev boxes).
_module_registry_cpp() {
cat <<'EOF'
mod-1v1-arena|1v1 Arena|https://github.com/azerothcore/mod-1v1-arena.git|characters
mod-aoe-loot|AoE Loot|https://github.com/azerothcore/mod-aoe-loot.git|world
mod-ah-bot|Auction House Bot|https://github.com/azerothcore/mod-ah-bot.git|world
mod-ah-bot-plus|Auction House Bot Plus (blizzlike pricing)|https://github.com/NathanHandley/mod-ah-bot-plus.git|world
mod-autobalance|Auto Balance (dynamic difficulty)|https://github.com/azerothcore/mod-autobalance.git|world
mod-ale|AzerothCore Lua Engine (ALE)|https://github.com/azerothcore/mod-ale.git|
mod-player-bot-level-brackets|Bot Level Brackets (Playerbot distribution)|https://github.com/DustinHendrickson/mod-player-bot-level-brackets.git|characters
mod-challenge-modes|Challenge Modes (Hardcore, Iron Man, etc.)|https://github.com/nl-saw/mod-challenge-modes.git|world,characters
mod-custom-login|Custom Login (starter gear + rep on first login)|https://github.com/azerothcore/mod-custom-login.git|characters
mod-individual-progression|Individual Progression (Vanilla -> TBC -> WotLK)|https://github.com/ZhengPeiRu21/mod-individual-progression.git|world,characters
mod-junk-to-gold|Junk to Gold (auto-sell gray items)|https://github.com/noisiver/mod-junk-to-gold.git|world
mod-learn-spells|Learn Spells on Levelup|https://github.com/azerothcore/mod-learn-spells.git|world
mod-npc-beastmaster|NPC Beastmaster (pets for all classes)|https://github.com/azerothcore/mod-npc-beastmaster.git|world,characters
mod-quest-loot-party|Quest Loot Party (quest items drop for all eligible party members)|https://github.com/pangolp/mod-quest-loot-party.git|world
mod-arac|All Races All Classes (ARAC - data mod: SQL + DBC + MPQ)|https://github.com/heyitsbench/mod-arac.git|world
mod-dungeon-master|Dungeon Master (roguelike dungeon challenge system)|https://github.com/InstanceForge/mod-dungeon-master.git|world,characters
mod-solocraft|Solocraft (solo dungeon/raid scaling)|https://github.com/azerothcore/mod-solocraft.git|world
mod-talentbutton|Talent Button (dual-spec at 10 + anywhere talent reset)|https://github.com/brian8544/mod-talentbutton.git|
mod-transmog|Transmogrification|https://github.com/azerothcore/mod-transmog.git|world,characters
EOF
}

# key|display name|git url
_module_registry_lua() {
cat <<'EOF'
accountwide|Accountwide Systems (achievements, currency, mounts, pets)|https://github.com/Aldori15/azerothcore-eluna-accountwide.git
activechat|Azeroth Chatter (lore-grounded ambient world RP chat)|https://github.com/svey-xyz/ActiveChat.git
battlepass|Battle Pass System (XP progression + rewards)|https://github.com/Shonik/lua-battlepass.git
bmah|Black Market Auction House (MoP-style BMAH + client addon)|https://github.com/DadsMmoLab/dads-mmo-lab.git
lootpet|Loot Pet (vanity pet auto-loots nearby corpses)|https://github.com/Brytenwally/Lootpet.git
paragon|Paragon Anniversary (endless post-80 stat progression + client files)|https://github.com/Grim-Batol/Paragon-Anniversary.git
sitmeanrest|Sit Means Rest (regen buff on /sit; strips on movement)|https://github.com/Brytenwally/SitMeansRest.git
sod|Season of Discovery Buffs (phased leveling XP rate bonus)|https://github.com/DadsMmoLab/dads-mmo-lab.git
unlimitedammo|Unlimited Ammo (auto-refills Hunter arrows/bullets)|https://github.com/Day36512/Acore_Lua_Unlimited_Ammo.git
EOF
}

# key|display name|git url|install type
_module_registry_sql() {
cat <<'EOF'
all-stackables|All Stackables to 200|https://github.com/AsgavinYT/azerothcore-all-stackables-200.git|clone_sql
baby-mobs|Baby Mobs (HPx0.25 / DMGx0.25 / ARMx0.25)||tweak_world
buff-mobs|Buff Mobs (HPx2 / DMGx1.5 / ARMx1.5)||tweak_world
xbuff-mobs|Extreme Buff Mobs (HPx4 / DMGx2 / ARMx2)||tweak_world
hearthstone-cd|Hearthstone Cooldown Tweaks|https://github.com/AsgavinYT/hearthstone-cooldowns.git|clone_sql_pick
lvl1-mounts|Level One Mounts (ride at level 1)|https://github.com/tomcoffingiii/mod-level-one-mounts.git|clone_sql
nerf-mobs|Nerf Mobs (HPx0.5 / DMGx0.75 / ARMx0.75)||tweak_world
npc-teleporter|NPC Teleporter (capital + starting zones)|https://github.com/Zoidwaffle/sql-npc-teleporter.git|clone_dist
portals-capitals|Portals in All Capitals|https://github.com/azerothcore/portals-in-all-capitals.git|clone_sql
rare-drops|Rare Drops (450 Classic rares loot)|https://github.com/StraysFromPath/mod-rare-drops.git|clone_sql_norevert
EOF
}

# One-line "what does this actually do" info per module key, shown on the
# Modules page. A separate lookup (NOT a registry column): every registry
# consumer does a positional `IFS='|' read` whose LAST variable swallows any
# trailing remainder, so appending a column would silently corrupt the last
# field at ten call sites. Unknown key -> empty (callers emit "" then).
# _module_desc_var <key>: NO-FORK form -- sets the description into the global
# REPLY (bash parameter expansion only). _module_desc is a thin echo wrapper so
# existing `$(_module_desc ...)` callers are unchanged; the hot module-list rows
# call _module_desc_var directly to skip the subshell.
_module_desc_var() {
    case "$1" in
      mod-1v1-arena) REPLY="Adds a 1v1 arena bracket to the arena system -- queue solo duels through the arena NPC." ;;
      mod-aoe-loot) REPLY="Loot every nearby corpse with one click, retail-style." ;;
      mod-ah-bot) REPLY="Keeps the Auction House stocked and buying: a bot lists and purchases items so the economy works on a solo server." ;;
      mod-ah-bot-plus) REPLY="Enhanced Auction House Bot fork: fully customizable, blizzlike pricing by category/quality/item level, conf-only (no SQL). Replaces mod-ah-bot -- install one or the other, not both." ;;
      mod-autobalance) REPLY="Scales dungeon/raid mob health and damage to your party size, so any group size can run any instance." ;;
      mod-ale) REPLY="The Lua scripting engine (Eluna fork) that powers every script in the Lua/ALE family below -- install this first." ;;
      mod-player-bot-level-brackets) REPLY="Spreads the random playerbot population across level ranges you configure, instead of everyone clustering at cap." ;;
      mod-challenge-modes) REPLY="Opt-in challenge rulesets per character: Hardcore (death is final), Iron Man, slow-XP and friends, with rewards." ;;
      mod-custom-login) REPLY="Gives freshly created characters configurable starter gear, items, and reputation on first login." ;;
      mod-individual-progression) REPLY="Each character progresses through Vanilla -> TBC -> WotLK content gates individually, like a personal timeline." ;;
      mod-junk-to-gold) REPLY="Automatically sells gray junk for gold the moment you loot it." ;;
      mod-learn-spells) REPLY="Class spells are learned automatically on level-up -- no class-trainer visits." ;;
      mod-npc-beastmaster) REPLY="A Beastmaster NPC that lets ANY class tame, stable and use hunter pets." ;;
      mod-quest-loot-party) REPLY="Quest items drop for every eligible party member at once instead of one at a time." ;;
      mod-arac) REPLY="All Races, All Classes: unlocks every race/class combo. After install, click 'Apply client patch' on this row (server DBCs + client Patch-A.MPQ), then restart -- the server part alone is not enough." ;;
      mod-dungeon-master) REPLY="Roguelike dungeon challenge system: scaling runs with modifiers and escalating rewards." ;;
      mod-solocraft) REPLY="Scales YOU up when entering dungeons/raids under-manned, so soloing instances is viable." ;;
      mod-talentbutton) REPLY="Dual spec from level 10 and a talent-reset button usable anywhere." ;;
      mod-transmog) REPLY="Transmogrification NPC: reskin your gear's appearance while keeping its stats." ;;
      accountwide) REPLY="Shares achievements, currency, mounts and pets across every character on the same account." ;;
      activechat) REPLY="Ambient lore-flavored NPC chatter around the world, so cities feel alive." ;;
      battlepass) REPLY="A battle-pass style XP track with tiered rewards for playing." ;;
      bmah) REPLY="MoP-style Black Market Auction House with rare/unobtainable items (server script + client addon)." ;;
      lootpet) REPLY="Your vanity pet trots around auto-looting nearby corpses for you." ;;
      paragon) REPLY="Endless post-80 'paragon' stat progression with client-side UI files." ;;
      sitmeanrest) REPLY="Sitting down grants a rest/regen buff; standing or moving strips it." ;;
      sod) REPLY="Season-of-Discovery-style phased XP rate bonuses while leveling." ;;
      unlimitedammo) REPLY="Hunters never run out: arrows and bullets auto-refill." ;;
      all-stackables) REPLY="Raises stack sizes to 200 for everything stackable." ;;
      baby-mobs) REPLY="World tweak: all mobs at quarter health/damage/armor -- easy mode." ;;
      buff-mobs) REPLY="World tweak: mobs at 2x health, 1.5x damage/armor -- harder world." ;;
      xbuff-mobs) REPLY="World tweak: mobs at 4x health, 2x damage/armor -- much harder world." ;;
      hearthstone-cd) REPLY="Pick a shorter Hearthstone cooldown (several variants to choose from)." ;;
      lvl1-mounts) REPLY="Mount training and riding available from level 1." ;;
      nerf-mobs) REPLY="World tweak: mobs at half health, 0.75x damage/armor -- gentler world." ;;
      npc-teleporter) REPLY="Adds teleporter NPCs in capitals and starting zones." ;;
      portals-capitals) REPLY="Two-way portals connecting all capital cities." ;;
      rare-drops) REPLY="About 450 Classic rare-spawn loot tables restored/enriched." ;;
      *) REPLY="" ;;
    esac
    return 0
}
_module_desc() { _module_desc_var "$1"; printf '%s\n' "$REPLY"; return 0; }

# --- validators (exit status IS the signal, like _valid_charname) ----------
_valid_module_key() { [[ "$1" =~ ^[a-z0-9-]{1,64}$ ]]; }
_valid_cpp_key()    { [[ "$1" =~ ^mod-[a-z0-9-]{1,64}$ ]]; }
_valid_module_url() { [[ "$1" =~ ^https://[A-Za-z0-9._~/-]+(\.git)?$ ]]; }

# Derives a custom-module key from a git URL basename (lowercased, .git
# stripped). Prints nothing if the result is not a valid mod-* key.
_module_key_from_url() {
    local base="${1##*/}"
    base="${base%.git}"
    base="${base,,}"
    if _valid_cpp_key "$base"; then printf '%s' "$base"; fi
    return 0
}

# --- state ------------------------------------------------------------------
_cpp_installed() { [[ -d "$1/modules/$2/.git" ]]; }

_rebuild_pending_file() { echo "$1/.dml-rebuild-pending"; }

_rebuild_pending_add() {
    local f; f="$(_rebuild_pending_file "$1")"
    grep -qxF "$2" "$f" 2>/dev/null || printf '%s\n' "$2" >> "$f"
    return 0
}

_rebuild_pending_clear() { rm -f "$(_rebuild_pending_file "$1")"; return 0; }

_rebuild_pending_json() {
    local f line out="" first=1
    f="$(_rebuild_pending_file "$1")"
    if [[ -f "$f" ]]; then
        while IFS= read -r line || [[ -n "$line" ]]; do
            [[ -z "$line" ]] && continue
            [[ $first -eq 0 ]] && out+=','
            out+="\"$(json_escape "$line")\""; first=0
        done < "$f"
    fi
    printf '[%s]' "$out"
    return 0
}

# Is <key> listed in the rebuild-pending file? (exit status)
_rebuild_pending_has() { grep -qxF "$2" "$(_rebuild_pending_file "$1")" 2>/dev/null; }

# NO-FORK batched variant for the module-list emitter: _rebuild_pending_load
# scans the pending file ONCE (pure bash, zero forks) into a set, then
# _rebuild_pending_has_mem answers per-row lookups in-process. A per-row
# `_rebuild_pending_has` grep otherwise forks once per module row even though
# the file is usually absent. Full-line exact match mirrors grep -qxF.
declare -gA _MOD_PENDING_SET=()
_rebuild_pending_load() {
    _MOD_PENDING_SET=()
    local f line
    f="$1/.dml-rebuild-pending"
    [[ -f "$f" ]] || return 0
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ -z "$line" ]] && continue
        _MOD_PENDING_SET["$line"]=1
    done < "$f"
    return 0
}
_rebuild_pending_has_mem() { [[ -n "${_MOD_PENDING_SET[$1]:-}" ]]; }

# Build-capability probe (spec 2026-08-09). Asks `docker compose config`
# (client-side: works with the engine down) whether ac-worldserver can build.
# Mirrors Rust's buildcap::build_files: a composegen-shaped server keeps
# `build:` in docker-compose.build.yml, which compose NEVER auto-loads, so
# the probe must pass the SAME explicit -f set (base, override, build --
# each only if present, that order) the rebuild's own build step uses, or
# the guard and the build step could disagree. A WSL/bash-era server has no
# build.yml at all -- build: lives in the base compose there and needs no
# flags, same as before this fix. Service-scoped awk over the CANONICALIZED
# yaml -- never a raw grep (the _stack_is_ac lesson). Prints yes|no|unknown;
# unknown must never refuse.
_module_can_build() {
    local sdir="$1" f cfg
    local -a fset=()
    if [[ -f "$sdir/docker-compose.build.yml" ]]; then
        for f in docker-compose.yml docker-compose.override.yml docker-compose.build.yml; do
            [[ -f "$sdir/$f" ]] && fset+=(-f "$f")
        done
    fi
    cfg="$(cd "$sdir" 2>/dev/null && docker compose ${fset[@]+"${fset[@]}"} config 2>/dev/null)" || { echo unknown; return 0; }
    [[ -z "$cfg" ]] && { echo unknown; return 0; }
    if printf '%s\n' "$cfg" | awk '
        /^services:/ { insvc=1; next }
        insvc && /^[^ ]/ { insvc=0 }
        insvc && /^  ac-worldserver:/ { inws=1; next }
        inws && /^  [^ ]/ { inws=0 }
        inws && /^    build:/ { found=1; exit }
        END { exit found?0:1 }
    '; then echo yes; else echo no; fi
}

# --- conf names (verbatim table from the manager; custom-login added) ------
# _module_conf_name_var <key>: NO-FORK form -- sets REPLY. _module_conf_name is
# a thin echo wrapper kept for existing `$(...)` callers.
_module_conf_name_var() {
    case "$1" in
        mod-1v1-arena)                  REPLY="1v1arena.conf" ;;
        mod-aoe-loot)                   REPLY="mod_aoe_loot.conf" ;;
        mod-ah-bot)                     REPLY="mod_ahbot.conf" ;;
        mod-ah-bot-plus)                REPLY="mod_ahbot.conf" ;;
        mod-autobalance)                REPLY="AutoBalance.conf" ;;
        mod-dungeon-master)             REPLY="mod_dungeon_master.conf" ;;
        mod-talentbutton)               REPLY="mod_talentbutton.conf" ;;
        mod-ale)                        REPLY="mod_ale.conf" ;;
        mod-player-bot-level-brackets)  REPLY="mod_player_bot_level_brackets.conf" ;;
        mod-challenge-modes)            REPLY="challenge_modes.conf" ;;
        mod-custom-login)               REPLY="mod_customlogin.conf" ;;
        mod-individual-progression)     REPLY="individualProgression.conf" ;;
        mod-learn-spells)               REPLY="mod_learnspells.conf" ;;
        mod-npc-beastmaster)            REPLY="mod_npc_beastmaster.conf" ;;
        mod-quest-loot-party)           REPLY="mod-quest-loot-party.conf" ;;
        mod-solocraft)                  REPLY="Solocraft.conf" ;;
        mod-transmog)                   REPLY="transmog.conf" ;;
        *)                              REPLY="" ;;
    esac
    return 0
}
_module_conf_name() { _module_conf_name_var "$1"; printf '%s\n' "$REPLY"; return 0; }

# Conf state for a cpp module: none | needs-rebuild | ready | active.
# .conf.dist appears under modules/<key>/ only after a rebuild for most
# modules (mod-custom-login ships it in-repo -- same logic covers both).
# _module_conf_state_var is the NO-FORK form (sets REPLY); _module_conf_state
# is the echo wrapper. Note the `ready`/`needs-rebuild` branch still forks a
# `find` via _module_conf_dist, but only for the handful of modules whose conf
# is neither absent nor active.
_module_conf_state_var() {
    local sdir="$1" key="$2" name dist active
    _module_conf_name_var "$key"; name="$REPLY"
    if [[ -z "$name" ]]; then REPLY="none"; return 0; fi
    active="$sdir/env/dist/etc/modules/$name"
    if [[ -f "$active" ]]; then REPLY="active"; return 0; fi
    dist="$(_module_conf_dist "$sdir" "$key")"
    if [[ -n "$dist" ]]; then REPLY="ready"; else REPLY="needs-rebuild"; fi
    return 0
}
_module_conf_state() { _module_conf_state_var "$1" "$2"; printf '%s\n' "$REPLY"; return 0; }

# Prints the .conf.dist path for a key, or nothing. Expected location
# first, then a bounded find (manager behavior).
_module_conf_dist() {
    local sdir="$1" key="$2" name p
    _module_conf_name_var "$key"; name="$REPLY"
    [[ -z "$name" ]] && return 0
    p="$sdir/modules/$key/conf/$name.dist"
    if [[ -f "$p" ]]; then printf '%s' "$p"; return 0; fi
    p="$(find "$sdir/modules/$key" -maxdepth 4 -type f -name "$name.dist" 2>/dev/null | head -n1)" || p=""
    [[ -n "$p" ]] && printf '%s' "$p"
    return 0
}

# Auto-activate a cpp module's conf after install (Modules-page round, Task
# 1; mirrors Rust `moduletail::conf_activate` with force=false): copy the
# clone's .conf.dist into the active dir IFF no active conf exists, so the
# server actually reads the module's settings instead of silently running
# stock defaults. STRICTLY ADVISORY -- it never overwrites a user's edited
# conf and never fails the install (exit 0 always). Echoes the conf name (no
# trailing newline) when it activated, nothing otherwise. Modules whose
# .conf.dist only appears after a rebuild activate on the NEXT install/the
# launcher's catch-up pass instead.
_module_conf_auto_activate() {
    local sdir="$1" mkey="$2" cname dist active
    _module_conf_name_var "$mkey"; cname="$REPLY"
    [[ -z "$cname" ]] && return 0
    active="$sdir/env/dist/etc/modules/$cname"
    [[ -f "$active" ]] && return 0
    dist="$(_module_conf_dist "$sdir" "$mkey")"
    [[ -z "$dist" ]] && return 0
    mkdir -p "$(dirname "$active")" && cp "$dist" "$active" && printf '%s' "$cname"
    return 0
}

# --- lua/sql state (list-only in C1; installers land in plan C2) -----------
# Deployed check per lua key (mirrors the manager's ale_lua_is_deployed).
_lua_deployed() {
    local lua="$1/env/dist/etc/modules/lua_scripts"
    case "$2" in
        accountwide)  [[ -d "$lua/accountwide" ]] ;;
        activechat)   [[ -d "$lua/AzerothChatter" ]] ;;
        battlepass)   [[ -d "$lua/battlepass" ]] ;;
        bmah)         [[ -f "$lua/BMAH.lua" ]] ;;
        lootpet)      [[ -f "$lua/LootPet.lua" ]] ;;
        paragon)      [[ -d "$lua/paragon" ]] ;;
        sitmeanrest)  [[ -f "$lua/SitMeansRest.lua" ]] ;;
        sod)          [[ -f "$lua/SOD.lua" ]] ;;
        unlimitedammo) [[ -f "$lua/UnlimitedAmmo.lua" ]] ;;
        *) return 1 ;;
    esac
}

_lua_cloned() { [[ -d "$1/ale_scripts/$2/.git" ]]; }

# Read-only advisory string for a lua script, or empty. Surfaced on the
# Modules card as a warning. Only fires when the script is actually DEPLOYED
# (live on the server), so it never nags about a module the user hasn't run.
# Paragon's upstream ships a `.test` chat command with NO GM permission gate
# -- any player can invoke it (a debug leftover), a real footgun once the
# server is opened to others. Uses a single-quoted string (never printf'd
# through a double-quoted context) so the literal ".test" carries no shell
# meaning.
# NO-FORK form sets REPLY; _lua_warn is the echo wrapper for other callers.
_lua_warn_var() {
    REPLY=""
    case "$2" in
        paragon)
            if _lua_deployed "$1" paragon; then
                REPLY='Heads-up: the Paragon script adds a ".test" chat command with no GM permission check — any player can run it (an upstream debug leftover). Remove or guard it before opening your server to other people.'
            fi
            ;;
    esac
    return 0
}
_lua_warn() { _lua_warn_var "$1" "$2"; printf '%s' "$REPLY"; return 0; }

_sql_installed() { [[ -f "$1/sql_scripts/installed/$2.installed" ]]; }

# --- backup gate ------------------------------------------------------------
# World-inclusive safety backup used by --backup gates on DB-mutating module
# operations. Streams ndjson progress; returns 1 on dump failure (caller
# aborts the whole operation).
_module_backup_now() {
    local bdir bfile
    # Task 6: the dump runs over the RESOLVED schema names -- and REFUSES
    # when they never resolved: every caller treats rc 1 as "safety backup
    # failed, abort", which is exactly right for a dump that would otherwise
    # capture guessed schemas while reporting success. Same warn-then-fail
    # shape as modmgr::module_backup_now.
    if ! _db_names_resolve; then
        ndjson_line warn "backup refused: $DB_NAMES_ERR_MSG"
        return 1
    fi
    bdir="$(_backup_dir)"; mkdir -p "$bdir"
    bfile="wow-$(date -u +%Y%m%d-%H%M%S)-full.sql.gz"
    # Narration rule (Task 6): never promise bots the dump will not carry --
    # keep the strings byte-identical to modmgr::module_backup_now.
    if [[ -n "$DB_NAME_PLAYERBOTS" ]]; then
        ndjson_line info "backing up characters, bots, accounts and world..."
    else
        ndjson_line info "backing up characters, accounts and world..."
        ndjson_line warn "no playerbots database is configured on this server -- the backup will not include bot data"
    fi
    if ! _backup_dump_to "$bdir/$bfile" 1; then
        local errtail
        errtail="$(tail -c 160 "$bdir/$bfile.err" 2>/dev/null | tr -d '\r\n"\\')" || errtail=""
        rm -f "$bdir/$bfile.err"
        [[ -n "$errtail" ]] && ndjson_line warn "backup failed: $errtail"
        return 1
    fi
    ndjson_line info "backup created: $bfile"
    while IFS= read -r _p || [[ -n "$_p" ]]; do
        [[ -z "$_p" ]] && continue
        ndjson_line info "pruned old backup: $_p"
    done < <(_backup_prune)
    return 0
}

# --- client path (for module client-side files) -----------------------------
_client_path_file() { echo "$HOME/.dml/client-path"; }

# Saved client path, printed only while it still exists on disk.
_client_path() {
    local f p
    f="$(_client_path_file)"
    [[ -r "$f" ]] || return 0
    p="$(cat "$f" 2>/dev/null)" || p=""
    [[ -d "$p" ]] && printf '%s' "$p"
    return 0
}

# Exit-status helper: does the dir look like a WoW client? (manager heuristic)
_client_valid() {
    [[ -f "$1/Wow.exe" || -f "$1/wow.exe" || -f "$1/WowT.exe" || -d "$1/Interface" ]]
}

# C:\Games\WoW -> /mnt/c/Games/WoW (drive lowercased, backslashes flipped).
# Non-Windows paths pass through untouched.
_client_win_to_wsl() {
    local p="$1" drive rest
    if [[ "$p" =~ ^([A-Za-z]):[\\/](.*)$ ]]; then
        drive="${BASH_REMATCH[1],,}"
        rest="${BASH_REMATCH[2]//\\//}"
        printf '/mnt/%s/%s' "$drive" "$rest"
    else
        printf '%s' "$p"
    fi
    return 0
}

# Candidate scan roots (manager's list); DML_CLIENT_SCAN_ROOTS (colon-sep)
# overrides for tests. Roots are scanned one level deep with the client
# heuristic; capped at 10 candidates.
_client_detect() {
    local roots=() r d n=0
    if [[ -n "${DML_CLIENT_SCAN_ROOTS:-}" ]]; then
        IFS=':' read -r -a roots <<< "$DML_CLIENT_SCAN_ROOTS"
    else
        roots=("$HOME/Games" "$HOME/wow wotlk" "$HOME")
        for d in /mnt/[a-z]; do
            [[ -d "$d" ]] || continue
            roots+=("$d/Games" "$d/Program Files (x86)" "$d/wow wotlk" "$d")
        done
    fi
    for r in "${roots[@]}"; do
        [[ -d "$r" ]] || continue
        for d in "$r"/*/; do
            [[ -d "$d" ]] || continue
            d="${d%/}"
            if _client_valid "$d"; then
                printf '%s\n' "$d"
                n=$(( n + 1 ))
                (( n >= 10 )) && return 0
            fi
        done
    done
    return 0
}

# --- lua (ALE) family -------------------------------------------------------
# SQL for THIS family is hand-applied -- that is normal for ALE scripts
# (the never-hand-apply rule is about cpp modules and db-import tracking).

_lua_has_sql() { [[ "$1" == accountwide || "$1" == battlepass || "$1" == bmah || "$1" == paragon ]]; }

# Task 6 scope ruling -- RECORDED EXCEPTION to resolved-name splicing: this
# helper's call sites pass hardcoded acore_* (and the module-owned acore_ale)
# literals on purpose. The module .sql payloads they apply hardcode the
# standard schema names INTERNALLY, so resolving only our argv while the
# payload hardcodes would be a half-rename that breaks worse than either
# consistent state; on a renamed server the module subsystem is broken by
# the modules themselves before our tooling enters. Same ruling as
# moduletail.rs's BATTLEPASS consts and unbound.rs on the Rust side.
_ale_sql_file() {
    docker exec -i ac-database mysql -uroot -p"$(_db_pw)" "$1" < "$2"
}

# Clone or update an ALE script. sod/bmah live as subfolders of the
# DadsMmoLab repo, so they use a sparse checkout instead of a full clone.
_lua_clone() {
    local sdir="$1" key="$2" url="$3" cdir sparse=""
    cdir="$sdir/ale_scripts/$key"
    case "$key" in
        sod)  sparse="guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery" ;;
        bmah) sparse="guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse" ;;
    esac
    mkdir -p "$sdir/ale_scripts"
    if [[ -d "$cdir/.git" ]]; then
        ndjson_line info "updating $key..."
        if [[ -n "$sparse" ]]; then
            (cd "$cdir" && _stream_cmd git pull --depth=1 origin HEAD) || return 1
        else
            (cd "$cdir" && _stream_cmd git pull --depth 1) || return 1
        fi
        return 0
    fi
    [[ -d "$cdir" ]] && rm -rf "$cdir"
    ndjson_line info "cloning $key..."
    if [[ -n "$sparse" ]]; then
        mkdir -p "$cdir"
        (cd "$cdir" \
          && git init -q \
          && git remote add origin "$url" \
          && git config core.sparseCheckout true \
          && printf '%s/\n' "$sparse" > .git/info/sparse-checkout \
          && _stream_cmd git pull --depth=1 origin HEAD) || return 1
    else
        _stream_cmd git clone --depth 1 "$url" "$cdir" || return 1
    fi
    return 0
}

# Copy the script's .lua payload from the clone into lua_scripts/ (per-key
# rules ported from ale_deploy_lua_files, incl. the activechat basename-
# collision renames + require patches and the battlepass CSMH require strip).
_lua_deploy() {
    local sdir="$1" key="$2" cdir lua f
    cdir="$sdir/ale_scripts/$key"
    lua="$sdir/env/dist/etc/modules/lua_scripts"
    mkdir -p "$lua"
    ndjson_line info "deploying $key lua files..."
    case "$key" in
        accountwide)
            [[ -d "$cdir/lua_scripts/AccountWide" ]] || return 1
            mkdir -p "$lua/accountwide"
            cp "$cdir/lua_scripts/AccountWide"/*.lua "$lua/accountwide/" || return 1
            ;;
        activechat)
            [[ -d "$cdir/AzerothChatter" ]] || return 1
            mkdir -p "$lua/AzerothChatter"
            cp -r "$cdir/AzerothChatter/." "$lua/AzerothChatter/" || return 1
            # ALE's loader dedupes by basename across subdirs: data/chatter.lua
            # collides with logic/chatter.lua (same for context) -- rename the
            # data pair and patch every require() call site to match.
            [[ -f "$lua/AzerothChatter/data/chatter.lua" ]] && mv "$lua/AzerothChatter/data/chatter.lua" "$lua/AzerothChatter/data/chatter_data.lua"
            [[ -f "$lua/AzerothChatter/data/context.lua" ]] && mv "$lua/AzerothChatter/data/context.lua" "$lua/AzerothChatter/data/context_data.lua"
            while IFS= read -r f; do
                sed -i 's/require("data\.chatter")/require("data.chatter_data")/g; s/require("data\.context")/require("data.context_data")/g' "$f"
            done < <(find "$lua/AzerothChatter" -name '*.lua' 2>/dev/null)
            ;;
        battlepass)
            [[ -d "$cdir/lua_scripts" ]] || return 1
            cp -r "$cdir/lua_scripts/." "$lua/" || return 1
            # ALE auto-loads .ext files before any .lua; the explicit require
            # re-executes CSMH and corrupts its state -- strip the line.
            [[ -f "$lua/05_BP_Communication.lua" ]] && sed -i '/require("lib\.CSMH\.CSMH_SMH")/d' "$lua/05_BP_Communication.lua"
            ;;
        bmah)
            [[ -f "$cdir/guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse/BMAH.lua" ]] || return 1
            cp "$cdir/guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse/BMAH.lua" "$lua/" || return 1
            ;;
        lootpet)
            [[ -f "$cdir/LootPet.lua" ]] || return 1
            cp "$cdir/LootPet.lua" "$lua/" || return 1
            ;;
        paragon)
            [[ -d "$cdir/serverside/paragon" ]] || return 1
            cp -r "$cdir/serverside/paragon" "$lua/" || return 1
            ;;
        sitmeanrest)
            [[ -f "$cdir/SitMeansRest.lua" ]] || return 1
            cp "$cdir/SitMeansRest.lua" "$lua/" || return 1
            ;;
        sod)
            [[ -f "$cdir/guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery/SOD.lua" ]] || return 1
            cp "$cdir/guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery/SOD.lua" "$lua/" || return 1
            rm -rf "$lua/sod"   # stale duplicate-load guard (manager parity)
            ;;
        unlimitedammo)
            [[ -f "$cdir/UnlimitedAmmo.lua" ]] || return 1
            cp "$cdir/UnlimitedAmmo.lua" "$lua/" || return 1
            ;;
        *) return 1 ;;
    esac
    return 0
}

# Per-key SQL application (this family's SQL is hand-applied by design).
_lua_apply_sql() {
    local sdir="$1" key="$2" cdir f bn
    cdir="$sdir/ale_scripts/$key"
    case "$key" in
        accountwide)
            ndjson_line info "applying accountwide SQL (acore_characters)..."
            _ale_sql_file acore_characters "$cdir/sql/create_accountwide_tables.sql" || return 1
            ;;
        battlepass)
            ndjson_line info "applying battlepass SQL (acore_world + acore_characters)..."
            _ale_sql_file acore_world "$cdir/sql/battlepass_world.sql" || return 1
            _ale_sql_file acore_characters "$cdir/sql/battlepass_characters.sql" || return 1
            ;;
        bmah)
            ndjson_line info "applying BMAH SQL (acore_world)..."
            _ale_sql_file acore_world "$cdir/guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse/sql/BMAH_Up.sql" || return 1
            ;;
        paragon)
            ndjson_line info "applying paragon migrations..."
            # 01_create_database.sql creates acore_ale (run via acore_world);
            # remaining numbered migrations run against acore_ale, in order.
            # Date-prefixed / Example data files are skipped (deliberate).
            [[ -f "$cdir/sql/01_create_database.sql" ]] && { _ale_sql_file acore_world "$cdir/sql/01_create_database.sql" || return 1; }
            while IFS= read -r f; do
                bn="$(basename "$f")"
                [[ "$bn" == "01_create_database.sql" ]] && continue
                [[ "$bn" =~ ^[0-9]{2}-[0-9]{2} || "$bn" =~ [Ee]xample ]] && continue
                ndjson_line info "  $bn"
                _ale_sql_file acore_ale "$f" || return 1
            done < <(find "$cdir/sql" -name '*.sql' 2>/dev/null | sort)
            ;;
    esac
    return 0
}

# Client-side copies for the keys that have them; soft-skips when no client
# path is set. sod's SERVER-side DBC step is not ported (needs a temp
# container into the data volume) -- warned instead.
_lua_client_copy() {
    local sdir="$1" key="$2" cdir cpath
    cdir="$sdir/ale_scripts/$key"
    cpath="$(_client_path)"
    case "$key" in
        bmah|paragon|sod) ;;
        *) return 0 ;;
    esac
    if [[ -z "$cpath" ]]; then
        ndjson_line warn "no client folder set — skipped client files (set it on the Modules page)"
        [[ "$key" == sod ]] && ndjson_line warn "SOD server DBC files must be copied manually — see guides/wow-wotlk/wow-manage.sh copy_server_dbc"
        return 0
    fi
    case "$key" in
        bmah)
            if [[ -d "$cdir/guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse/Client Files/AddOns/BlackMarketUI" ]]; then
                ndjson_line info "installing BlackMarketUI client addon..."
                mkdir -p "$cpath/Interface/AddOns/BlackMarketUI" || { ndjson_line warn "client copy failed — copy it manually"; return 0; }
                cp -r "$cdir/guides/wow-wotlk/ALE-Kegs/BlackMarketAuctionHouse/Client Files/AddOns/BlackMarketUI/." "$cpath/Interface/AddOns/BlackMarketUI/" || ndjson_line warn "client addon copy failed — copy it manually"
            fi
            ;;
        paragon)
            if [[ -d "$cdir/clientside/Interface" ]]; then
                ndjson_line info "installing paragon client Interface files..."
                mkdir -p "$cpath/Interface" || { ndjson_line warn "client copy failed — copy it manually"; return 0; }
                cp -r "$cdir/clientside/Interface/." "$cpath/Interface/" || ndjson_line warn "client Interface copy failed — copy it manually"
            fi
            ;;
        sod)
            if [[ -d "$cdir/guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery/Data" ]]; then
                ndjson_line info "installing SOD client Data patches..."
                mkdir -p "$cpath/Data" || { ndjson_line warn "client copy failed — copy it manually"; return 0; }
                cp -r "$cdir/guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery/Data/." "$cpath/Data/" || ndjson_line warn "client Data copy failed — copy it manually"
            fi
            if [[ -d "$cdir/guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery/Interface" ]]; then
                mkdir -p "$cpath/Interface" || { ndjson_line warn "client copy failed — copy it manually"; return 0; }
                cp -r "$cdir/guides/wow-wotlk/ALE-Kegs/SeasonOfDiscovery/Interface/." "$cpath/Interface/" || ndjson_line warn "client Interface copy failed — copy it manually"
            fi
            ndjson_line warn "SOD server DBC files must be copied manually — see guides/wow-wotlk/wow-manage.sh copy_server_dbc"
            ;;
    esac
    return 0
}

# Delete deployed lua payload (mirror of _lua_deploy; DB tables kept).
_lua_remove_deployed() {
    local sdir="$1" key="$2" lua
    lua="$sdir/env/dist/etc/modules/lua_scripts"
    case "$key" in
        accountwide)   rm -rf "$lua/accountwide" ;;
        activechat)    rm -rf "$lua/AzerothChatter" ;;
        battlepass)    rm -rf "$lua/battlepass" "$lua/lib/CSMH"; rm -f "$lua"/05_BP_*.lua ;;
        bmah)          rm -f "$lua/BMAH.lua" ;;
        lootpet)       rm -f "$lua/LootPet.lua" ;;
        paragon)       rm -rf "$lua/paragon" ;;
        sitmeanrest)   rm -f "$lua/SitMeansRest.lua" ;;
        sod)           rm -f "$lua/SOD.lua" ;;
        unlimitedammo) rm -f "$lua/UnlimitedAmmo.lua" ;;
    esac
    return 0
}

# --- accountwide configurator (overnight Batch 1) --------------------------
# The accountwide ALE family ships EVERY sharing subsystem DISABLED, one
# `local ENABLE_* = false` line (column 0) per per-subsystem lua file, and no
# in-game UI. This helper set reads/writes those flags in the DEPLOYED copy
# under env/dist/etc/modules/lua_scripts/accountwide/ -- NEVER the repo: this
# family clones the upstream repo fresh at install time and copies the lua in
# (see _lua_deploy's accountwide case). Ported from the manager's _aw_enable +
# configure_ale_accountwide (guides/wow-wotlk/wow-manage.sh), generalized to
# also turn flags back OFF and to READ current state for the launcher GUI.
# Apply is `.reload ale` in-game (or a worldserver restart) -- the dispatch
# surfaces that hint; the create_accountwide_tables.sql prerequisite is
# applied by the install path, not here.

# Deployed accountwide script dir for a server dir.
_aw_dir() { printf '%s' "$1/env/dist/etc/modules/lua_scripts/accountwide"; }

# Is the accountwide payload deployed? (exit status IS the signal)
_aw_installed() { [[ -d "$(_aw_dir "$1")" ]]; }

# Subsystem registry (reputation is pick-one -- handled separately below).
# One row per togglable flag:  flag|file|group|parent|label|explain
# `group` nests the money/achievement sub-toggles under their parent in the
# GUI; `parent` is the flag that must be ON for a sub-toggle to have any
# effect ("" = a top-level system). Descriptions are plain-language for a
# parent running the server. Flag names + files are the upstream HEAD's,
# verbatim (Aldori15/azerothcore-eluna-accountwide).
_aw_registry() {
cat <<'EOF'
ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS|AccountAchievements.lua|achievements||Achievements|Earn an achievement on one character and every character on the account gets it too.
ENABLE_ACCOUNTWIDE_CRITERIA_PROGRESS|AccountAchievements.lua|achievements|ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS|Achievement progress|Also share partial progress toward achievements, not just the finished ones.
ENABLE_ACCOUNTWIDE_REALM_FIRST_ACHIEVEMENTS|AccountAchievements.lua|achievements|ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS|Realm-first achievements|Include the special "realm first" achievements in the sharing.
ENABLE_ACCOUNTWIDE_CURRENCY|AccountCurrency.lua|currency||Currency|Share badges and tokens (like dungeon emblems) across all your characters.
ENABLE_ACCOUNTWIDE_MONEY|AccountMoney.lua|money||Gold|Keep one shared gold pool for every character on the account.
ENABLE_REALTIME_TICK|AccountMoney.lua|money|ENABLE_ACCOUNTWIDE_MONEY|Live gold sync|Update the shared gold every few seconds while you play, not only on logout.
ENABLE_ALTBOT_REALTIME_TICK|AccountMoney.lua|money|ENABLE_REALTIME_TICK|Live gold sync for alt-bots|Also live-sync the shared gold for your alt playerbots.
ENABLE_ACCOUNTWIDE_MOUNTS|AccountMounts.lua|mounts||Mounts|Learn a mount on one character and all your characters can ride it.
ENABLE_ACCOUNTWIDE_PETS|AccountPets.lua|pets||Battle pets|Share companion and vanity pets across all your characters.
ENABLE_ACCOUNTWIDE_PLAYTIME|AccountPlaytime.lua|playtime||Playtime|Adds a .playtime command that totals time played across the whole account.
ENABLE_ACCOUNTWIDE_PROFESSIONS|AccountProfessions.lua|professions||Professions|Share learned professions and their skill level across all your characters.
ENABLE_ACCOUNTWIDE_PVP_RANK|AccountPvPRank.lua|pvp||PvP rank|Share honor kills and honor/arena points across the account.
ENABLE_ACCOUNTWIDE_TAXI_PATHS|AccountTaxiPaths.lua|taxi||Flight paths|Share discovered flight points per faction. Needs a special mod-ale fork - may not work on this server.
ENABLE_ACCOUNTWIDE_TITLES|AccountTitles.lua|titles||Titles|Earn a title on one character and every character on the account can use it.
EOF
}

# Flag-name allowlist (identifier-safe -> never sed/awk regex-injectable).
_aw_valid_flag() { [[ "$1" =~ ^ENABLE_[A-Z0-9_]{1,60}$ ]]; }

# _aw_flag_read <file> <flag>: echoes "on" | "off" | "" (flag absent / no
# file). Matches an UNCOMMENTED `local <flag> = true|false` line (leading
# whitespace tolerated; a `-- local ...` comment is skipped), LAST occurrence
# wins. The flag travels via the environment (F=), never awk -v, so its
# underscores are never escape-processed; a trailing `-- comment` after the
# boolean is tolerated. The `rest ~ /^[ \t]*=/` guard makes prefix flag names
# safe (a longer flag's line has more identifier chars, not `=`, after ours).
_aw_flag_read() {
    local file="$1"
    [[ -f "$file" ]] || { printf ''; return 0; }
    F="$2" awk '
        {
            s = $0; sub(/\r$/, "", s); sub(/^[ \t]+/, "", s)
            head = "local " ENVIRON["F"]
            if (index(s, head) == 1) {
                rest = substr(s, length(head) + 1)
                if (rest ~ /^[ \t]*=/) {
                    sub(/^[ \t]*=[ \t]*/, "", rest)
                    tok = rest
                    sub(/[ \t].*$/, "", tok); sub(/--.*$/, "", tok); sub(/;.*$/, "", tok)
                    if (tok == "true")  { val = "on";  found = 1 }
                    else if (tok == "false") { val = "off"; found = 1 }
                }
            }
        }
        END { if (found) print val }
    ' "$file" 2>/dev/null
    return 0
}

# _aw_flag_write <file> <flag> <on|off>: flips the flag's `= true|false`
# value in place. Tolerant sed anchored to line-start (so `-- local` comments
# are skipped -- the manager's proven pattern); verifies the new value landed
# before committing via tmp-file + mv, so a failed edit never truncates the
# file. Returns 1 when the flag line is absent OR the patch cannot be
# verified; sets AW_CHANGED=true only when the value actually moved. Callers
# validate <flag> with _aw_valid_flag first.
_aw_flag_write() {
    local file="$1" flag="$2" want="$3" cur from to tmp
    [[ -f "$file" ]] || return 1
    cur="$(_aw_flag_read "$file" "$flag")"
    [[ -z "$cur" ]] && return 1
    [[ "$cur" == "$want" ]] && return 0
    if [[ "$want" == on ]]; then from=false; to=true; else from=true; to=false; fi
    tmp="$file.tmp.$$"
    sed "s/^\([[:space:]]*local ${flag}\)[[:space:]]*=[[:space:]]*${from}/\1 = ${to}/" "$file" > "$tmp" || { rm -f "$tmp"; return 1; }
    if grep -q "^[[:space:]]*local ${flag} = ${to}" "$tmp"; then
        mv "$tmp" "$file"
        AW_CHANGED=true
        return 0
    fi
    rm -f "$tmp"
    return 1
}

# _aw_rep_files <dir>: reputation is pick-one -- two variant files may ship
# (default AC-WotLK vs a custom/Ashen-Order build), both carrying
# ENABLE_ACCOUNTWIDE_REPUTATION. Echoes "default|custom<TAB><path>" rows for
# whichever AccountReputation*.lua files are present (no nullglob set, so a
# no-match glob stays literal and is filtered by the -f test).
_aw_rep_files() {
    local dir="$1" f bn
    for f in "$dir"/AccountReputation*.lua; do
        [[ -f "$f" ]] || continue
        bn="$(basename "$f")"
        case "$bn" in
            *[Dd]efault*) printf 'default\t%s\n' "$f" ;;
            *)            printf 'custom\t%s\n'  "$f" ;;
        esac
    done
    return 0
}

# --- sql-mod family ---------------------------------------------------------
_sqlmod_dirs() { mkdir -p "$1/sql_scripts/installed" "$1/sql_scripts/clones"; return 0; }
_sqlmod_marker() { echo "$1/sql_scripts/installed/$2.installed"; }

# Task 6 scope ruling -- RECORDED EXCEPTION, same as _ale_sql_file above:
# call sites pass hardcoded acore_* literals because the SQL payloads they
# run (third-party module .sql files) hardcode the standard names anyway.
_sqlmod_run_stmt() { docker exec ac-database mysql -uroot -p"$(_db_pw)" "$1" -e "$2"; }
_sqlmod_run_file() { docker exec -i ac-database mysql -uroot -p"$(_db_pw)" "$1" < "$2"; }

# Apply list for clone_sql types: every .sql in the clone except reversal /
# example files, sorted for determinism. Classified by BASENAME, not full
# path -- a server path containing "down"/"example" would otherwise
# misclassify every file underneath it.
_sqlmod_up_files() {
    local f bn
    while IFS= read -r f; do
        [[ -z "$f" ]] && continue
        bn="$(basename "$f")"
        printf '%s' "$bn" | grep -qiE 'down|example' || printf '%s\n' "$f"
    done < <(find "$1" -name '*.sql' 2>/dev/null | sort)
    return 0
}
_sqlmod_down_files() {
    local f bn
    while IFS= read -r f; do
        [[ -z "$f" ]] && continue
        bn="$(basename "$f")"
        printf '%s' "$bn" | grep -qiE 'down' && printf '%s\n' "$f"
    done < <(find "$1" -name '*.sql' 2>/dev/null | sort)
    return 0
}

# H D A multipliers per tweak key.
_tweak_mults() {
    case "$1" in
        baby-mobs)  echo "0.25 0.25 0.25" ;;
        buff-mobs)  echo "2 1.5 1.5" ;;
        xbuff-mobs) echo "4 2 2" ;;
        nerf-mobs)  echo "0.5 0.75 0.75" ;;
        *) echo "" ;;
    esac
}

_tweak_apply() {
    _sqlmod_run_stmt acore_world "UPDATE creature_template SET HealthModifier = HealthModifier * $1, DamageModifier = DamageModifier * $2, ArmorModifier = ArmorModifier * $3;"
}

# Which sibling tweak (if any) is currently installed? (mutual exclusion)
_tweak_installed_sibling() {
    local sdir="$1" me="$2" k
    for k in baby-mobs buff-mobs xbuff-mobs nerf-mobs; do
        [[ "$k" == "$me" ]] && continue
        [[ -f "$(_sqlmod_marker "$sdir" "$k")" ]] && { printf '%s' "$k"; return 0; }
    done
    return 0
}

# Inverse-apply a tweak from its marker's APPLIED_* values (fallback to the
# registry defaults). Float-lossy by design -- manager parity.
_tweak_reverse() {
    local sdir="$1" key="$2" mfile h d a ih id ia
    mfile="$(_sqlmod_marker "$sdir" "$key")"
    h=""; d=""; a=""
    if [[ -f "$mfile" ]]; then
        h="$(grep -m1 '^APPLIED_HP_MULT=' "$mfile" | cut -d= -f2 || true)"
        d="$(grep -m1 '^APPLIED_DMG_MULT=' "$mfile" | cut -d= -f2 || true)"
        a="$(grep -m1 '^APPLIED_ARM_MULT=' "$mfile" | cut -d= -f2 || true)"
    fi
    if [[ -z "$h" || -z "$d" || -z "$a" ]]; then
        read -r h d a <<< "$(_tweak_mults "$key")"
    fi
    [[ -z "$h" ]] && return 1
    ih="$(awk "BEGIN{printf \"%.6f\", 1/$h}")"
    id="$(awk "BEGIN{printf \"%.6f\", 1/$d}")"
    ia="$(awk "BEGIN{printf \"%.6f\", 1/$a}")"
    _tweak_apply "$ih" "$id" "$ia"
}

# --- module repair (Round J): tracking diagnosis + updates-table mark/clear -
# AzerothCore's db-import tracks every applied module SQL file per DB in
# that DB's `updates` table (name + SHA1 + state). When tracking desyncs
# from actual schema state, the next start fails ("Table X already exists")
# or silently re-applies SQL. Ported from the manager's
# discover_module_sql_files / show_module_tracking / compute_sql_hash /
# mark_sql_applied (guides/wow-wotlk/wow-manage.sh 1633-1892) -- the SQL
# semantics are mirrored exactly; the manager's interactive prompts/echo
# output are replaced with JSON in the `module tracking`/`module repair`
# arms (90-main.sh). This helper set never touches actual game tables.

# Discover a module's top-level *.sql files for one DB (space-separated,
# the manager's own format -- also matches --files' space-separated CLI
# syntax). Checks data/sql/db-<short>/ first, falls back to sql/<short>/;
# prints nothing if neither exists. NOT recursive -- subdirs are usually
# versioned variants (manager comment, ported verbatim).
_module_discover_sql_files() {
    local sdir="$1" key="$2" db_short="$3" sql_dir
    sql_dir="$sdir/modules/$key/data/sql/db-${db_short}"
    [[ -d "$sql_dir" ]] || sql_dir="$sdir/modules/$key/sql/${db_short}"
    [[ -d "$sql_dir" ]] || return 0
    (cd "$sql_dir" && ls *.sql 2>/dev/null | tr '\n' ' ')
    return 0
}

# Read-only dispatch to the matching acore_<db> query helper (30-db.sh) for
# a "world|characters|auth" short name -- used by both `module tracking`
# and `module repair`'s clear-mode COUNT check.
_module_db_read() {
    case "$1" in
        world)      db_world_query "$2" ;;
        characters) db_chars_query "$2" ;;
        auth)       db_auth_query "$2" ;;
    esac
}

# Filename validator for repair targets -- no slashes, path-injection-proof
# (rejects ../evil.sql, x.sql;DROP, etc.). Exit status IS the signal (same
# pattern as _valid_charname).
_valid_module_sql_filename() { [[ "$1" =~ ^[A-Za-z0-9._-]+\.sql$ ]]; }
# ---------------------------------------------------------------------------
# Server self-update (Round L): update-check / update.
# Ports the manager's update_server_source / _check_remote / _pull_repo
# (guides/wow-wotlk/wow-manage.sh:7018-7193) FAIL-CLOSED: the manager's
# interactive "pull anyway?" overrides on a remote/branch mismatch become
# hard errors here -- there is no prompt to answer in the launcher, and
# silently pulling upstream AzerothCore over the Playerbots fork would break
# the playerbots integration outright. No chained auto-rebuild either -- the
# Modules page's existing rebuild banner + rebuild flow own that (see
# `_rebuild_pending_add ... core-update` below). See docs/superpowers/specs/
# 2026-07-18-server-update-design.md for the full contract.
# ---------------------------------------------------------------------------

# Guarded one-liner git reads: a bare `x="$(git ...)"` would abort the whole
# script under the global `set -e` the instant git exits non-zero (missing
# remote, detached HEAD, ...). Each of these always returns 0 itself (the
# `||` fallback absorbs the failure), matching the _client_path/soap_exec
# guarded-assignment convention used elsewhere in this codebase -- callers
# can assign straight from them with no extra guard.
_wow_git_url()    { (cd "$1" && git remote get-url origin) 2>/dev/null || printf ''; }
_wow_git_branch() { (cd "$1" && git rev-parse --abbrev-ref HEAD) 2>/dev/null || printf ''; }
_wow_git_head()   { (cd "$1" && git rev-parse --short HEAD) 2>/dev/null || printf ''; }
_wow_git_dirty()  { (cd "$1" && git status --porcelain --untracked-files=no) 2>/dev/null || printf ''; }

# Does a remote URL point at the expected fork? Old installs may still point
# at the pre-rename liyunfan1223 org -- GitHub redirects those, so they're
# accepted too (manager's own comment on _check_remote, ported verbatim).
_wow_remote_ok() { [[ "$1" == *"$2"* || "$1" == *liyunfan1223* ]]; }

# One `update-check` repo object. Never fails -- `behind` is null on a fetch
# failure, not an error (this is an informational endpoint, not a gate).
_wow_repo_check_json() {
    local dir="$1" label="$2" url branch head dirty dcount=0 behind=null bcount
    url="$(_wow_git_url "$dir")"
    branch="$(_wow_git_branch "$dir")"
    head="$(_wow_git_head "$dir")"
    dirty="$(_wow_git_dirty "$dir")"
    [[ -n "$dirty" ]] && dcount="$(printf '%s\n' "$dirty" | grep -c .)"
    if (cd "$dir" && git fetch --quiet origin) >/dev/null 2>&1; then
        if bcount="$(cd "$dir" && git rev-list --count "HEAD..origin/$branch" 2>/dev/null)"; then
            [[ "$bcount" =~ ^[0-9]+$ ]] && behind="$bcount"
        fi
    fi
    printf '{"label":"%s","url":"%s","branch":"%s","head":"%s","dirty":%s,"behind":%s}' \
        "$(json_escape "$label")" "$(json_escape "$url")" "$(json_escape "$branch")" \
        "$(json_escape "$head")" "$dcount" "$behind"
    return 0
}

# Pulls one repo, preserving local edits: a dirty tracked file gets backed up
# to a timestamped patch file AND stashed BEFORE the pull; the stash is
# popped back on top of the update afterward. Sets _WOW_PULL_CHANGED
# (true/false) and _WOW_PULL_SUMMARY ("<before> -> <after>" or "up to date")
# on success. On failure this emits the ndjson_error itself and returns 1 --
# the caller only needs to close its section and exit.
#
# DELIBERATE DEVIATION from the manager: no interactive "pull anyway?" --
# the caller (`wow update`) has already fail-closed on any remote/branch
# mismatch before this ever runs, so every path below is unconditional.
_wow_pull_repo() {
    local dir="$1" label="$2" before after dirty stamp patch=""
    before="$(_wow_git_head "$dir")"
    dirty="$(_wow_git_dirty "$dir")"
    if [[ -n "$dirty" ]]; then
        stamp="$(date -u +%Y%m%d-%H%M%S)"
        patch="$dir/local-changes-$stamp.patch"
        if ! (cd "$dir" && git diff) > "$patch" 2>/dev/null; then
            rm -f "$patch"
            ndjson_error EDIT_BACKUP_FAILED "$label: could not back up local changes -- aborting" "Nothing was pulled."
            return 1
        fi
        ndjson_line info "$label: local changes backed up to $patch"
        if ! (cd "$dir" && git stash push -m "dml update auto-stash $stamp") >/dev/null 2>&1; then
            ndjson_error EDIT_BACKUP_FAILED "$label: could not stash local changes -- aborting" "Nothing was pulled."
            return 1
        fi
    fi
    ndjson_line info "updating $label..."
    if ! (cd "$dir" && _stream_cmd git pull --ff-only); then
        if [[ -n "$dirty" ]]; then
            (cd "$dir" && git stash pop) >/dev/null 2>&1 || true
            ndjson_line info "$label: your local changes were restored unchanged."
        fi
        ndjson_error PULL_FAILED "git pull failed for $label (diverged branch?)" "Inspect manually: cd $dir && git status"
        return 1
    fi
    after="$(_wow_git_head "$dir")"
    _WOW_PULL_CHANGED=false
    [[ "$before" != "$after" ]] && _WOW_PULL_CHANGED=true
    if [[ -n "$dirty" ]]; then
        if (cd "$dir" && git stash pop) >/dev/null 2>&1; then
            ndjson_line info "$label: your local edits were re-applied on top of the update."
        else
            # Pop conflicted: git keeps the stash entry. Clear the
            # conflicted working tree back to the clean updated state so the
            # server still builds -- best-effort (|| true), this is already
            # the recovery path.
            (cd "$dir" && git checkout -f -- .) >/dev/null 2>&1 || true
            (cd "$dir" && git reset --hard HEAD) >/dev/null 2>&1 || true
            ndjson_line warn "$label: some of your edits CONFLICT with this update. The updated files were kept so the server builds cleanly."
            ndjson_line warn "Your edits are safe in two places -- patch file: $patch"
            ndjson_line warn "...and git stash: cd $dir && git stash pop   (resolve conflicts manually)"
        fi
    fi
    if [[ "$before" == "$after" ]]; then
        _WOW_PULL_SUMMARY="up to date"
    else
        _WOW_PULL_SUMMARY="$before -> $after"
    fi
    return 0
}
