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
_module_registry_cpp() {
cat <<'EOF'
mod-1v1-arena|1v1 Arena|https://github.com/azerothcore/mod-1v1-arena.git|characters
mod-aoe-loot|AoE Loot|https://github.com/azerothcore/mod-aoe-loot.git|world
mod-ah-bot|Auction House Bot|https://github.com/azerothcore/mod-ah-bot.git|world
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
_module_desc() {
    case "$1" in
      mod-1v1-arena) echo "Adds a 1v1 arena bracket to the arena system -- queue solo duels through the arena NPC." ;;
      mod-aoe-loot) echo "Loot every nearby corpse with one click, retail-style." ;;
      mod-ah-bot) echo "Keeps the Auction House stocked and buying: a bot lists and purchases items so the economy works on a solo server." ;;
      mod-autobalance) echo "Scales dungeon/raid mob health and damage to your party size, so any group size can run any instance." ;;
      mod-ale) echo "The Lua scripting engine (Eluna fork) that powers every script in the Lua/ALE family below -- install this first." ;;
      mod-player-bot-level-brackets) echo "Spreads the random playerbot population across level ranges you configure, instead of everyone clustering at cap." ;;
      mod-challenge-modes) echo "Opt-in challenge rulesets per character: Hardcore (death is final), Iron Man, slow-XP and friends, with rewards." ;;
      mod-custom-login) echo "Gives freshly created characters configurable starter gear, items, and reputation on first login." ;;
      mod-individual-progression) echo "Each character progresses through Vanilla -> TBC -> WotLK content gates individually, like a personal timeline." ;;
      mod-junk-to-gold) echo "Automatically sells gray junk for gold the moment you loot it." ;;
      mod-learn-spells) echo "Class spells are learned automatically on level-up -- no class-trainer visits." ;;
      mod-npc-beastmaster) echo "A Beastmaster NPC that lets ANY class tame, stable and use hunter pets." ;;
      mod-quest-loot-party) echo "Quest items drop for every eligible party member at once instead of one at a time." ;;
      mod-arac) echo "All Races, All Classes: unlocks every race/class combo. After install, click 'Apply client patch' on this row (server DBCs + client Patch-A.MPQ), then restart -- the server part alone is not enough." ;;
      mod-dungeon-master) echo "Roguelike dungeon challenge system: scaling runs with modifiers and escalating rewards." ;;
      mod-solocraft) echo "Scales YOU up when entering dungeons/raids under-manned, so soloing instances is viable." ;;
      mod-talentbutton) echo "Dual spec from level 10 and a talent-reset button usable anywhere." ;;
      mod-transmog) echo "Transmogrification NPC: reskin your gear's appearance while keeping its stats." ;;
      accountwide) echo "Shares achievements, currency, mounts and pets across every character on the same account." ;;
      activechat) echo "Ambient lore-flavored NPC chatter around the world, so cities feel alive." ;;
      battlepass) echo "A battle-pass style XP track with tiered rewards for playing." ;;
      bmah) echo "MoP-style Black Market Auction House with rare/unobtainable items (server script + client addon)." ;;
      lootpet) echo "Your vanity pet trots around auto-looting nearby corpses for you." ;;
      paragon) echo "Endless post-80 'paragon' stat progression with client-side UI files." ;;
      sitmeanrest) echo "Sitting down grants a rest/regen buff; standing or moving strips it." ;;
      sod) echo "Season-of-Discovery-style phased XP rate bonuses while leveling." ;;
      unlimitedammo) echo "Hunters never run out: arrows and bullets auto-refill." ;;
      all-stackables) echo "Raises stack sizes to 200 for everything stackable." ;;
      baby-mobs) echo "World tweak: all mobs at quarter health/damage/armor -- easy mode." ;;
      buff-mobs) echo "World tweak: mobs at 2x health, 1.5x damage/armor -- harder world." ;;
      xbuff-mobs) echo "World tweak: mobs at 4x health, 2x damage/armor -- much harder world." ;;
      hearthstone-cd) echo "Pick a shorter Hearthstone cooldown (several variants to choose from)." ;;
      lvl1-mounts) echo "Mount training and riding available from level 1." ;;
      nerf-mobs) echo "World tweak: mobs at half health, 0.75x damage/armor -- gentler world." ;;
      npc-teleporter) echo "Adds teleporter NPCs in capitals and starting zones." ;;
      portals-capitals) echo "Two-way portals connecting all capital cities." ;;
      rare-drops) echo "About 450 Classic rare-spawn loot tables restored/enriched." ;;
      *) echo "" ;;
    esac
    return 0
}

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

# --- conf names (verbatim table from the manager; custom-login added) ------
_module_conf_name() {
    case "$1" in
        mod-1v1-arena)                  echo "1v1arena.conf" ;;
        mod-aoe-loot)                   echo "mod_aoe_loot.conf" ;;
        mod-ah-bot)                     echo "mod_ahbot.conf" ;;
        mod-autobalance)                echo "AutoBalance.conf" ;;
        mod-dungeon-master)             echo "mod_dungeon_master.conf" ;;
        mod-talentbutton)               echo "mod_talentbutton.conf" ;;
        mod-ale)                        echo "mod_ale.conf" ;;
        mod-player-bot-level-brackets)  echo "mod_player_bot_level_brackets.conf" ;;
        mod-challenge-modes)            echo "challenge_modes.conf" ;;
        mod-custom-login)               echo "mod_customlogin.conf" ;;
        mod-individual-progression)     echo "individualProgression.conf" ;;
        mod-learn-spells)               echo "mod_learnspells.conf" ;;
        mod-npc-beastmaster)            echo "mod_npc_beastmaster.conf" ;;
        mod-quest-loot-party)           echo "mod-quest-loot-party.conf" ;;
        mod-solocraft)                  echo "Solocraft.conf" ;;
        mod-transmog)                   echo "transmog.conf" ;;
        *)                              echo "" ;;
    esac
}

# Conf state for a cpp module: none | needs-rebuild | ready | active.
# .conf.dist appears under modules/<key>/ only after a rebuild for most
# modules (mod-custom-login ships it in-repo -- same logic covers both).
_module_conf_state() {
    local sdir="$1" key="$2" name dist active
    name="$(_module_conf_name "$key")"
    if [[ -z "$name" ]]; then echo "none"; return 0; fi
    active="$sdir/env/dist/etc/modules/$name"
    if [[ -f "$active" ]]; then echo "active"; return 0; fi
    dist="$(_module_conf_dist "$sdir" "$key")"
    if [[ -n "$dist" ]]; then echo "ready"; else echo "needs-rebuild"; fi
    return 0
}

# Prints the .conf.dist path for a key, or nothing. Expected location
# first, then a bounded find (manager behavior).
_module_conf_dist() {
    local sdir="$1" key="$2" name p
    name="$(_module_conf_name "$key")"
    [[ -z "$name" ]] && return 0
    p="$sdir/modules/$key/conf/$name.dist"
    if [[ -f "$p" ]]; then printf '%s' "$p"; return 0; fi
    p="$(find "$sdir/modules/$key" -maxdepth 4 -type f -name "$name.dist" 2>/dev/null | head -n1)" || p=""
    [[ -n "$p" ]] && printf '%s' "$p"
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

_sql_installed() { [[ -f "$1/sql_scripts/installed/$2.installed" ]]; }

# --- backup gate ------------------------------------------------------------
# World-inclusive safety backup used by --backup gates on DB-mutating module
# operations. Streams ndjson progress; returns 1 on dump failure (caller
# aborts the whole operation).
_module_backup_now() {
    local bdir bfile
    bdir="$(_backup_dir)"; mkdir -p "$bdir"
    bfile="wow-$(date -u +%Y%m%d-%H%M%S)-full.sql.gz"
    ndjson_line info "backing up characters, bots, accounts and world..."
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

# --- sql-mod family ---------------------------------------------------------
_sqlmod_dirs() { mkdir -p "$1/sql_scripts/installed" "$1/sql_scripts/clones"; return 0; }
_sqlmod_marker() { echo "$1/sql_scripts/installed/$2.installed"; }

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
