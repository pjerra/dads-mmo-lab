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
