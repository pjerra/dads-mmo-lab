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
        lootpet)      [[ -f "$lua/Lootpet.lua" ]] ;;
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
        rm -f "$bdir/$bfile.err"
        return 1
    fi
    ndjson_line info "backup created: $bfile"
    while IFS= read -r _p || [[ -n "$_p" ]]; do
        [[ -z "$_p" ]] && continue
        ndjson_line info "pruned old backup: $_p"
    done < <(_backup_prune)
    return 0
}
