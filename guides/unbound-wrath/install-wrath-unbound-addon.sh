#!/bin/bash
# ============================================================
#  Dad's MMO Lab — Wrath Unbound Add-On Installer
#  Layers the multi-class Wrath Unbound mod onto an EXISTING
#  Dad's MMO Lab WotLK Playerbots server (AzerothCore + Docker)
#
#  https://github.com/DadsMmoLab/dads-mmo-lab
#
#  Version: 1.2.2 (2026-06-14 — reworks ALE/Lua staging to match
#  wow-manage.sh's convention: unbound_mentor.lua now deploys to
#  $SERVER_DIR/env/dist/etc/modules/lua_scripts/ (the shared ALE-Kegs
#  directory, covered by AzerothCore's stock env/dist/etc bind mount) and
#  mod_ale.conf is configured in place with an absolute ALE.ScriptPath —
#  docker-compose.override.yml is no longer touched and no custom
#  AC_ALE_SCRIPT_PATH env var is added. Fixes a path-mismatch bug where
#  running wow-manage.sh before this installer left unbound_mentor.lua in
#  a directory ALE never scanned (Mentor said "Greetings" only, Mentor
#  Stone just ate). Uninstaller updated to match: removes
#  unbound_mentor.lua from the new and legacy locations, and no longer
#  deletes mod_ale.conf, which other ALE-Kegs Lua mods may share.
#
#  v1.2.1 (2026-06-14) fixed a dbimport duplicate-key bug in v1.2.0's
#  catalog-fix migrations 12-14 that could surface on reinstall/update
#  ("Duplicate entry '2-34769' for key 'unbound_class_catalog.PRIMARY'");
#  same feature set as 1.2.0:
#
#  v1.2.0 (2026-06-13) adds full cross-class access: Mentor-unlocked
#  classes can now train abilities directly from class trainers, equip
#  cross-class gear, and accept that class's quests, via a small AzerothCore
#  core-engine patch (Player::m_unboundClassMask). Mentor UI overhaul: spells
#  now buy instantly with one click, plus a "Buy ALL available abilities"
#  button. Catalog audit: corrected req_levels against real trainers, added
#  missing Mage teleports/portals + Paladin Summon Warhorse, and fixed
#  Paladin Judgement / Paladin+Warlock mount / Druid Flight Form purchases
#  that previously took gold and granted nothing.)
#
#  UPDATING AN EXISTING INSTALL: just re-run this installer. It re-stages
#  every file, re-applies all SQL migrations, applies the new core-engine
#  patch, and rebuilds — safe and idempotent.
#
#  NOT to be confused with install-wow-unbound.sh (The Unbound Era —
#  a separate, from-scratch Vanilla CMaNGOS project).
#
#  What this does:
#    1. Verifies this is a compatible Dad's MMO Lab WotLK install
#    2. Backs up your world/characters databases before touching anything
#    3. Drops in the mod-unbound C++ module + Lua script + SQL migrations
#    4. Stages the Eluna/ALE Lua engine module if your server doesn't have it
#    5. Applies a small core-engine patch enabling cross-class trainer/quest/item access
#    6. Patches worldserver.conf (ValidateSkillLearnedBySpells = 0)
#    7. Rebuilds the worldserver (forced — new compiled module + core patch)
#    8. Walks you through the one manual step (.npc add 900001)
#
#  Prerequisite: a running Dad's MMO Lab WotLK Playerbots server
#  (built via install-wow-wotlk.sh / install-wow.sh). This installer
#  does NOT build a server from scratch.
# ============================================================

WIZARD_VERSION="1.4.0"

# Default server location (install-wow-wotlk.sh / install-wow.sh standard).
# detect_server_dir() in MAIN will auto-detect if this path doesn't exist.
SERVER_DIR="$HOME/wow-server-playerbots"

set -o pipefail

# ─────────────────────────────────────────
# COLORS  (matching install-wow-wotlk.sh conventions)
# ─────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
WHITE='\033[1;37m'
NC='\033[0m'
BOLD='\033[1m'

IS_WSL2=false
grep -qi "microsoft\|wsl" /proc/version 2>/dev/null && IS_WSL2=true

print_header() {
    echo -e "${BOLD}${MAGENTA:-$CYAN}=============================================================${NC}"
    echo -e "${BOLD}  Dad's MMO Lab — Wrath Unbound Add-On Installer (v${WIZARD_VERSION})${NC}"
    echo -e "${BOLD}${MAGENTA:-$CYAN}=============================================================${NC}"
    echo ""
}

print_step()    { echo -e "${BLUE}▶ $1${NC}"; }
print_success() { echo -e "${GREEN}✅ $1${NC}"; }
print_warning() { echo -e "${YELLOW}⚠️  $1${NC}"; }

ask_yes_no() {
    local prompt="$1"
    local answer
    while true; do
        read -r -p "$(echo -e "${WHITE}${prompt} [y/n]: ${NC}")" answer
        case "$answer" in
            [Yy]*) return 0 ;;
            [Nn]*) return 1 ;;
            *) echo "Please answer yes or no." ;;
        esac
    done
}

# ============================================================
# Returns 0 (true) if the given path is on the Windows filesystem under WSL2.
# Covers standard WSL2 mounts (/mnt/c/, /mnt/d/, …) and custom root=/ mounts
# where drive letters sit directly under / (/c/, /d/, …).
# Standard Linux root dirs (/home/, /usr/, /bin/, …) are multi-character and
# never match the single-letter /X/ pattern used by Windows drives.
is_windows_fs_path() {
    $IS_WSL2 || return 1
    [[ "$1" =~ ^/mnt/[a-zA-Z]/ ]] && return 0   # /mnt/c/…, /mnt/d/…
    [[ "$1" =~ ^/[a-zA-Z]/ ]]     && return 0   # /c/…, /d/… (custom root=/)
    return 1
}

# ============================================================
#  detect_server_dir()
#
#  Resolves SERVER_DIR at runtime so users who renamed their server
#  folder (e.g. wow-unbound instead of the default wow-server-playerbots)
#  are not hard-blocked. Resolution order:
#
#  1. Default location  ~/wow-server-playerbots  (install-wow-wotlk.sh standard)
#  2. Common rename     ~/wow-unbound
#  3. Shallow scan      $HOME/* directories that look like AzerothCore installs
#     (have both docker-compose.yml and env/dist/ — the AC-specific layout)
#  4. Manual prompt     if nothing is found or the user wants to override
#
#  Windows-filesystem paths (/mnt/c/…, /c/…) are skipped in steps 1-3 and
#  hard-warned in step 4 — running the server from the Windows FS causes severe
#  Docker performance degradation and is not a supported configuration.
#
#  Sets SERVER_DIR on success; exits 1 on failure.
# ============================================================
detect_server_dir() {
    # ── Derive true Linux home ───────────────────────────────────────────────
    # On some WSL2 setups $HOME is set to the Windows user profile
    # (e.g. /c/Users/nolim or /mnt/c/Users/nolim) instead of /home/<user>.
    # When that happens, auto-detection and tilde expansion both produce wrong
    # paths.  Use getent passwd to get the real Linux home directory.
    local LINUX_HOME="$HOME"
    if $IS_WSL2 && [[ "$HOME" != /home/* ]]; then
        local pw_home
        pw_home="$(getent passwd "$(whoami)" 2>/dev/null | cut -d: -f6)"
        if [[ "$pw_home" == /home/* ]]; then
            LINUX_HOME="$pw_home"
        else
            LINUX_HOME="/home/$(whoami)"
        fi
        print_warning "\$HOME is set to '$HOME' (a Windows path on this WSL2 system)."
        echo "Using your Linux home directory instead: $LINUX_HOME"
        echo ""
    fi

    # ── 1 & 2: known locations ───────────────────────────────────────────────
    local windows_found=()   # Windows-FS paths found but skipped
    for candidate in "$LINUX_HOME/games/wow-server-playerbots" "$LINUX_HOME/wow-server-playerbots" "$LINUX_HOME/wow-unbound"; do
        if [ -d "$candidate" ] && [ -f "$candidate/docker-compose.yml" ]; then
            if is_windows_fs_path "$candidate"; then
                windows_found+=("$candidate")
            else
                SERVER_DIR="$candidate"
                return
            fi
        fi
    done

    # ── 3: shallow scan for AzerothCore installs ────────────────────────────
    # On WSL2, $HOME may be wrong (Windows path) — always include /home/ in the
    # scan so the server is found regardless of $HOME.  Also scan all of /home/
    # when running as root (sudo) since $HOME=/root in that case.
    local scan_roots=("$LINUX_HOME")
    if ($IS_WSL2 || [ "$EUID" -eq 0 ]) && [ -d /home ]; then
        while IFS= read -r -d '' udir; do
            [[ "$udir" == "$LINUX_HOME" ]] || scan_roots+=("$udir")
        done < <(find /home -mindepth 1 -maxdepth 1 -type d -print0 2>/dev/null)
    fi

    local candidates=()
    for scan_root in "${scan_roots[@]}"; do
        while IFS= read -r -d '' dir; do
            if [ -f "$dir/docker-compose.yml" ] && [ -d "$dir/env/dist" ]; then
                if is_windows_fs_path "$dir"; then
                    windows_found+=("$dir")
                else
                    candidates+=("$dir")
                fi
            fi
        done < <(find "$scan_root" -mindepth 1 -maxdepth 1 -type d -not -name ".*" -print0 2>/dev/null)
    done

    if [ "${#candidates[@]}" -eq 1 ]; then
        print_warning "Server not found at the default location (~/wow-server-playerbots)."
        echo -e "  Found a likely AzerothCore install at: ${CYAN}${candidates[0]}${NC}"
        echo ""
        if ask_yes_no "Use this as your server folder?"; then
            SERVER_DIR="${candidates[0]}"
            return
        fi
    elif [ "${#candidates[@]}" -gt 1 ]; then
        print_warning "Server not found at the default location. Multiple candidates found:"
        echo ""
        for i in "${!candidates[@]}"; do
            echo -e "  ${CYAN}$((i+1)).${NC} ${candidates[$i]}"
        done
        echo ""
        local choice
        while true; do
            read -r -p "$(echo -e "${WHITE}Enter the number of your WotLK Playerbots server: ${NC}")" choice
            if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "${#candidates[@]}" ]; then
                SERVER_DIR="${candidates[$((choice-1))]}"
                return
            fi
            echo "Please enter a number between 1 and ${#candidates[@]}."
        done
    fi

    # ── Windows-FS explanation (shown when only Windows paths were found) ────
    if [ "${#windows_found[@]}" -gt 0 ]; then
        echo ""
        print_warning "Found server folder(s) on your Windows filesystem — cannot use them:"
        for wf in "${windows_found[@]}"; do
            echo -e "  ${YELLOW}$wf${NC}"
        done
        echo ""
        echo "Running a WoW server from the Windows filesystem (/mnt/c/ or /c/) causes"
        echo "severe Docker I/O performance issues and is not a supported configuration."
        echo ""
        echo "Your server needs to be on the Linux filesystem. To fix this:"
        echo -e "  1. Copy the installer to your Linux home:"
        echo -e "     ${CYAN}cp /mnt/c/Users/\$(cmd.exe /c 'echo %USERNAME%' 2>/dev/null | tr -d '\r')/Downloads/install-wow-wotlk.sh ~/Downloads/${NC}"
        echo -e "     (or download it fresh from github.com/DadsMmoLab)"
        echo -e "  2. Run it from this terminal:"
        echo -e "     ${CYAN}bash ~/Downloads/install-wow-wotlk.sh${NC}"
        echo ""
    fi

    # ── 4: manual prompt ─────────────────────────────────────────────────────
    echo ""
    print_warning "Could not find a Dad's MMO Lab WotLK Playerbots install automatically."
    echo "Wrath Unbound adds onto an EXISTING server built with install-wow-wotlk.sh."
    echo ""
    # Running with sudo changes \$HOME to /root, which is why auto-detection
    # misses a server installed under the real user's home directory.
    if [ "$EUID" -eq 0 ]; then
        print_warning "You appear to be running as root (sudo)."
        echo "This changes \$HOME to /root, so auto-detection can't find your server."
        echo "Re-run without sudo: bash install-wrath-unbound-addon.sh"
        echo "Or enter the absolute path below (e.g. /home/username/wow-server-playerbots)."
        echo ""
    fi

    # WSL2: server must be on the Linux filesystem, not the Windows C: drive.
    if $IS_WSL2; then
        echo -e "${YELLOW}WSL2 detected.${NC} Your server must be on the Linux filesystem."
        echo "Do NOT use a /mnt/c/ path — that's your Windows drive."
        echo -e "Your server is most likely at: ${CYAN}/home/$(whoami)/wow-server-playerbots${NC}"
        echo ""
    fi

    local input expanded attempts=0
    while true; do
        attempts=$((attempts + 1))
        echo -e "${WHITE}Enter the full path to your server folder:${NC}"
        echo -e "${CYAN}Use an absolute path: /home/username/wow-server-playerbots${NC}"
        echo -e "${CYAN}  or tilde shorthand: ~/wow-server-playerbots${NC}"
        read -r input

        # Expand leading ~/ → LINUX_HOME/  (only when followed by / or end of
        # string).  Use LINUX_HOME (not $HOME) so tilde expansion is correct
        # even on WSL2 where $HOME may point to the Windows user profile.
        if [[ "$input" == "~/"* ]]; then
            expanded="${LINUX_HOME}/${input#~/}"
        elif [[ "$input" == "~" ]]; then
            expanded="$LINUX_HOME"
        else
            expanded="$input"
        fi

        # Hard-warn if the user types a Windows-filesystem path.
        if is_windows_fs_path "$expanded"; then
            echo ""
            print_warning "That path is on your Windows filesystem — not a supported location."
            echo "Running the server from /mnt/c/ or /c/ causes severe Docker performance"
            echo "degradation. Your server needs to be on the Linux filesystem:"
            echo -e "  ${CYAN}/home/$(whoami)/wow-server-playerbots${NC}"
            echo ""
            echo "Copy install-wow-wotlk.sh to your Linux home and re-run it from this"
            echo "terminal to install the base server in the right place:"
            echo -e "  ${CYAN}bash ~/Downloads/install-wow-wotlk.sh${NC}"
            echo ""
            if ask_yes_no "Proceed with this Windows-filesystem path anyway? (not recommended)"; then
                print_warning "Proceeding — expect performance issues and potential failures."
                echo ""
            else
                echo ""
                attempts=$((attempts - 1))   # don't count a Windows-path attempt
                continue
            fi
        fi

        if [ -d "$expanded" ] && [ -f "$expanded/docker-compose.yml" ]; then
            SERVER_DIR="$expanded"
            return
        fi

        echo ""
        print_warning "No server found at: $expanded"
        if [[ "$input" == "~"* ]] && [[ "$input" != "~/"* ]]; then
            echo "It looks like you typed '~home/...' or '~username/...' — that's not"
            echo "standard tilde syntax. Use a full absolute path instead:"
            echo -e "  ${CYAN}/home/$(whoami)/wow-server-playerbots${NC}"
        elif [ ! -d "$expanded" ]; then
            echo "That directory doesn't exist."
            local parent
            parent="$(dirname "$expanded")"
            if [ -d "$parent" ]; then
                echo -e "Folders found in ${CYAN}$parent${NC}:"
                ls -1 "$parent" 2>/dev/null | while read -r name; do
                    echo "  $name"
                done
            fi
        else
            echo "That directory exists but has no docker-compose.yml — it may not be"
            echo "a Dad's MMO Lab WotLK Playerbots install."
            echo -e "Folders found in ${CYAN}$(dirname "$expanded")${NC}:"
            ls -1 "$(dirname "$expanded")" 2>/dev/null | while read -r name; do
                echo "  $name"
            done
        fi
        echo ""

        if [ "$attempts" -ge 3 ]; then
            print_warning "Could not find a valid server folder after 3 attempts."
            echo "Make sure install-wow-wotlk.sh was run first, then try again."
            exit 1
        fi
    done
}

# ============================================================
#  check_compatibility()
#
#  Wrath Unbound's spell catalog (unbound_class_catalog) is built by
#  reading a specific set of SYNTHETIC npc_trainer rows (IDs 200002,
#  200004, 200006, 200008, 200010, 200012, 200014, 200016, 200018 —
#  one per class) that the mod-playerbots fork generates internally
#  for bot AI to query trainer spell lists. These rows have NO
#  associated creature_template entry — they are not real NPCs, just
#  incidental seed data from one specific build
#  (core_revision e98e7a97e3f2+, Playerbot branch, ACDB 335.16-dev).
#
#  This is NOT a documented, stable schema — a different Playerbots
#  build could produce different IDs or a different ID→class mapping,
#  silently breaking catalog population (empty catalog, or worse,
#  spells mapped to the wrong class). Wrath Unbound is therefore
#  scoped EXCLUSIVELY to Dad's MMO Lab WotLK Playerbots installs —
#  this function is the gate that enforces that.
#
#  Live-verified thresholds (2026-06-08, against the dev server):
#  actual = 9 distinct IDs / 1858 spell rows. Gate set well below
#  that (9 / 100) to tolerate normal build-to-build seed variance
#  while still rejecting servers with none of this data (0 / 0).
# ============================================================
check_compatibility() {
    print_step "Checking that this is a compatible Dad's MMO Lab Playerbots server..."

    local DB_RUNNING
    DB_RUNNING=$(docker compose ps -q ac-database 2>/dev/null)
    if [ -z "$DB_RUNNING" ]; then
        print_warning "Could not find a running ac-database container."
        echo "Wrath Unbound adds onto an EXISTING running Dad's MMO Lab WotLK server —"
        echo "start your server first (docker compose up -d), then run this installer."
        exit 1
    fi

    # ── Check 1: build fingerprint (informational — warn, don't block) ──
    local VERSION_INFO CORE_REV DB_VER
    VERSION_INFO=$(docker exec ac-database mysql -u root -ppassword acore_world -N \
        -e "SELECT core_revision, db_version FROM version LIMIT 1;" 2>/dev/null)
    CORE_REV=$(echo "$VERSION_INFO" | awk -F'\t' '{print $1}')
    DB_VER=$(echo "$VERSION_INFO"   | awk -F'\t' '{print $2}')

    echo -e "${CYAN}   Detected build: ${CORE_REV:-unknown} / ${DB_VER:-unknown}${NC}"

    if [[ "$CORE_REV" != e98e7a97e3f2* ]]; then
        print_warning "This server's core revision differs from the build Wrath Unbound"
        print_warning "was developed and tested against (e98e7a97e3f2+, Playerbot branch,"
        print_warning "ACDB 335.16-dev, 2026-05-29)."
        echo ""
        echo "It MAY still work if this is a Dad's MMO Lab WotLK install from a nearby"
        echo "build — the check below is what actually determines compatibility."
        echo ""
    else
        print_success "Core build matches the known-compatible baseline."
    fi

    # ── Check 2: the REAL dependency — synthetic Playerbots trainer seed data ──
    # This is a hard gate. If these rows are missing, wrong, or differently
    # mapped, the catalog migration will silently produce an empty or
    # incorrectly-mapped catalog — there is no safe way to proceed.
    local DISTINCT_IDS TOTAL_ROWS
    read -r DISTINCT_IDS TOTAL_ROWS <<< "$(docker exec ac-database mysql -u root -ppassword acore_world -N -e "
        SELECT COUNT(DISTINCT ID), COUNT(*)
        FROM npc_trainer
        WHERE ID IN (200002,200004,200006,200008,200010,200012,200014,200016,200018)
          AND SpellID > 0;" 2>/dev/null)"

    DISTINCT_IDS=${DISTINCT_IDS:-0}
    TOTAL_ROWS=${TOTAL_ROWS:-0}

    echo -e "${CYAN}   Found Playerbots trainer seed data for ${DISTINCT_IDS}/9 classes"
    echo -e "${CYAN}   (${TOTAL_ROWS} spell entries total).${NC}"

    if [ "$DISTINCT_IDS" -lt 9 ] || [ "$TOTAL_ROWS" -lt 100 ]; then
        echo ""
        print_warning "This server is missing the Playerbots trainer seed data that"
        print_warning "Wrath Unbound's spell catalog depends on."
        echo ""
        echo -e "${RED}Wrath Unbound is built specifically for Dad's MMO Lab WotLK Playerbots${NC}"
        echo -e "${RED}servers. This install doesn't match — installing here would produce${NC}"
        echo -e "${RED}an empty or broken spell catalog (or worse, map spells to the wrong${NC}"
        echo -e "${RED}classes). No changes have been made.${NC}"
        echo ""
        echo "If you believe this IS a Dad's MMO Lab install and are seeing this in"
        echo "error, please report it — this check may need adjusting for your build."
        exit 1
    fi

    print_success "Compatible Playerbots trainer data found — this looks like a"
    print_success "Dad's MMO Lab WotLK install. Proceeding."
    echo ""
}

# ============================================================
#  check_existing_install()
#
#  Two things to surface before touching the database, folded into one
#  gate + one confirmation:
#
#  1. Canary check — does this server already have Wrath Unbound on it?
#     (unbound_milestones is created by migration 01 and only exists on
#     servers that have already run this installer or had the mod
#     applied by hand.) Re-running is largely safe — the SQL audit
#     found most migrations idempotent.
#
#  2. Destructive-step warning (folded in from a separate
#     warn_destructive_steps() during scoping) — migrations 03 and 05
#     DELETE-then-INSERT rows in playercreateinfo_spell_custom for the
#     classes Wrath Unbound manages. That's the same table any
#     non-Wrath-Unbound custom creation-gift edits for those classes
#     would live in, so this ALWAYS needs surfacing — on a fresh
#     install just as much as a re-run — not only when an existing
#     Wrath Unbound install is detected.
# ============================================================
EXISTING_INSTALL=false

check_existing_install() {
    print_step "Checking for an existing Wrath Unbound install..."

    local CANARY
    CANARY=$(docker exec ac-database mysql -u root -ppassword acore_world -N \
        -e "SELECT 1 FROM unbound_milestones LIMIT 1;" 2>/dev/null)

    if [ "$CANARY" = "1" ]; then
        EXISTING_INSTALL=true
        print_warning "Wrath Unbound already appears to be installed on this server"
        print_warning "(found existing unbound_milestones data)."
        echo ""
        echo "Re-running this installer will re-apply its SQL migrations, restage"
        echo "the module files, and rebuild the worldserver. This only touches"
        echo "Wrath Unbound's own data and files — nothing else on your server."
    else
        print_success "No existing Wrath Unbound install detected — proceeding with a fresh install."
    fi
    echo ""

    print_warning "One thing worth knowing before you continue:"
    echo ""
    echo "Two of Wrath Unbound's SQL migrations (03 and 05) manage rows in"
    echo "'playercreateinfo_spell_custom' — the table that controls which spells"
    echo "characters receive at creation — for Warrior, Paladin, Hunter, Rogue,"
    echo "Priest, Shaman, Mage, Warlock, and Druid. They delete and re-insert"
    echo "those classes' rows every time they run."
    echo ""
    echo "If you've made your OWN custom creation-gift edits for those classes"
    echo "(outside of Wrath Unbound), this will overwrite them with Wrath"
    echo "Unbound's gift list. Nothing else on your server is touched, and your"
    echo "databases get backed up before any of this runs."
    echo ""

    if ! ask_yes_no "Continue?"; then
        echo "No changes made. Exiting."
        exit 0
    fi
    echo ""
}

# ============================================================
#  backup_database()
#
#  mysqldumps acore_world and acore_characters to a timestamped folder
#  before any migrations run. Cheap insurance — the SQL audit flagged
#  destructive DELETE-then-INSERT blocks in migrations 03/05, and a
#  rebuild + restart is involved, so a one-command restore path matters.
# ============================================================
BACKUP_DIR=""

backup_database() {
    print_step "Backing up your world and characters databases before making changes..."

    local TIMESTAMP
    TIMESTAMP=$(date +%Y%m%d-%H%M%S)
    BACKUP_DIR="$HOME/wrath-unbound-backups/$TIMESTAMP"
    mkdir -p "$BACKUP_DIR"

    local DB
    for DB in acore_world acore_characters; do
        echo -e "${CYAN}   Dumping ${DB}...${NC}"
        if ! docker exec ac-database mysqldump -u root -ppassword "$DB" > "$BACKUP_DIR/${DB}.sql" 2>/dev/null; then
            print_warning "Failed to back up ${DB} — aborting before making any changes."
            rm -rf "$BACKUP_DIR"
            exit 1
        fi
    done

    print_success "Backup saved to: $BACKUP_DIR"
    echo "If anything goes wrong, you can restore with:"
    echo -e "${CYAN}   docker exec -i ac-database mysql -u root -ppassword acore_world < $BACKUP_DIR/acore_world.sql${NC}"
    echo -e "${CYAN}   docker exec -i ac-database mysql -u root -ppassword acore_characters < $BACKUP_DIR/acore_characters.sql${NC}"
    echo ""
}

# ============================================================
#  stage_module_files()
#
#  Writes the Wrath Unbound payload (mod-unbound C++ module,
#  Lua script, SQL migrations, NPC setup) into the target server
#  tree. Payload is embedded inline below (heredocs) so this
#  installer is a single self-contained file — matches the
#  one-click feel of the rest of the Dad's MMO Lab suite.
#
#  NOTE FOR MAINTAINERS: the embedded blocks below are generated,
#  not hand-written. To refresh them after editing the live
#  source in ~/wow-server-playerbots/, regenerate this function
#  from the source tree rather than hand-editing the heredocs.
# ============================================================
stage_module_files() {
    print_step "Staging Wrath Unbound module files into your server..."

    local MODULE_DIR="$SERVER_DIR/modules/mod-unbound"
    mkdir -p "$MODULE_DIR/src"
    mkdir -p "$MODULE_DIR/data/sql/db-world"
    mkdir -p "$MODULE_DIR/data/sql/db-characters"
    mkdir -p "$SERVER_DIR/env/dist/etc/modules/lua_scripts"

    cat > "$MODULE_DIR/src/UnboundSystem.cpp" <<'WU_PAYLOAD_EOF_1'
#include "Player.h"
#include "ScriptMgr.h"
#include "ScriptDefines/PlayerScript.h"
#include "DatabaseEnv.h"
#include "Entities/Item/ItemTemplate.h"

// Unbound Wrath Edition — power chassis + weapon/armor proficiency hooks.
//
// OnPlayerHasActivePowerType:
//   AzerothCore gates ALL rage/energy generation through HasActivePowerType.
//   We intercept so any non-native power type the Lua system granted via
//   SetMaxPower > 0 actually generates in combat.
//
// OnPlayerLogin:
//   learnSkillRewardedSpells() (called when weapon skills are set) filters
//   proficiency spells by ClassMask.  A Paladin who unlocks Warrior will have
//   Swords/Axes/etc. proficiency (Paladin's ClassMask matches those entries)
//   but NOT Staves/Daggers/Wands/Bows (ClassMask excludes Paladin).
//   The client therefore shows those weapons as red/unequippable.
//   Fix: if the player is Unbound (has any entry in unbound_character_unlocks),
//   grant full weapon + armor proficiency and send SMSG_SET_PROFICIENCY so the
//   client updates immediately.  This fires after the player is in-world.
//
//   Also builds player->m_unboundClassMask (bitmask of EXTRA classes unlocked
//   via the Mentor, NOT including the native class; 0 = not Unbound) from
//   unbound_character_unlocks. CanUseItem, IsSpellFitByClassAndRace, and
//   SatisfyQuestClass (Player/PlayerStorage/PlayerQuest .cpp) consult this mask
//   so item, trainer-spell, and class-quest restrictions are relaxed only for this
//   character — item_template/SkillLineAbility/quest_template stay untouched, so
//   Playerbots' own class-appropriateness heuristics (which read those tables
//   directly) are unaffected for the random bot population.
//
// Everything else lives in env/dist/etc/modules/lua_scripts/unbound_mentor.lua.

class UnboundPlayerScript : public PlayerScript
{
public:
    UnboundPlayerScript() : PlayerScript("UnboundPlayerScript",
    {
        PLAYERHOOK_ON_PLAYER_HAS_ACTIVE_POWER_TYPE,
        PLAYERHOOK_ON_LOGIN,
        PLAYERHOOK_ON_AFTER_UPDATE_MAX_POWER
    }) {}

    // Prevent AzerothCore's UpdateMaxPower from wiping a Lua-set mana pool.
    // For non-caster classes (warriors, rogues, etc.) GetCreatePowers(POWER_MANA)
    // returns 0, so the recalculation always produces 0 — silently erasing whatever
    // SetMaxPower set.  We intercept here (before SetMaxPower is called) and restore
    // the previously stored value if it was non-zero.
    void OnPlayerAfterUpdateMaxPower(Player* player, Powers& power, float& value) override
    {
        if (power != POWER_MANA)
            return;
        if (player->getPowerType() == POWER_MANA)
            return;  // native caster — let normal calculation stand
        if (value > 0.0f)
            return;  // calculated a real value — don't interfere
        uint32 current = player->GetMaxPower(POWER_MANA);
        if (current > 0)
            value = static_cast<float>(current);
    }

    bool OnPlayerHasActivePowerType(Player const* player, Powers power) override
    {
        if (player->getPowerType() == power)
            return false;

        return player->GetMaxPower(power) > 0;
    }

    void OnPlayerLogin(Player* player) override
    {
        // Skip bots — they don't need cross-class weapon proficiency or
        // the Unbound class mask (Playerbots' own heuristics read
        // item_template/SkillLineAbility/quest_template directly and must
        // see the bot's native class only).
        if (player->GetSession()->IsBot())
            return;

        // Build the Unbound class mask: bitmask of EXTRA classes unlocked
        // via the Mentor, NOT including the native class (0 = not Unbound).
        // CanUseItem (PlayerStorage.cpp) checks GetUnboundClassMask() != 0
        // to bypass AllowableClass entirely; IsSpellFitByClassAndRace
        // (Player.cpp) and SatisfyQuestClass (PlayerQuest.cpp) instead OR
        // this onto getClassMask() to widen the effective class set.
        uint32 unboundClassMask = 0;

        QueryResult result = CharacterDatabase.Query(
            "SELECT class_id FROM unbound_character_unlocks WHERE char_guid = {}",
            player->GetGUID().GetCounter());

        if (result)
        {
            do
            {
                Field* fields = result->Fetch();
                uint8 classId = fields[0].Get<uint8>();
                unboundClassMask |= (1u << (classId - 1));
            } while (result->NextRow());
        }

        player->SetUnboundClassMask(unboundClassMask);

        // Not Unbound — nothing else to do.
        if (unboundClassMask == 0)
            return;

        // Grant full weapon and armor proficiency so the client shows all
        // weapon/armor types as equippable (not red).
        // The server-side equip check (GetSkillValue > 0) is handled by the
        // Lua layer which calls SetSkill for all weapon/armor skill IDs.
        uint32 allWeapons = (1u << MAX_ITEM_SUBCLASS_WEAPON) - 1u;
        uint32 allArmor   = (1u << MAX_ITEM_SUBCLASS_ARMOR)  - 1u;

        player->AddWeaponProficiency(allWeapons);
        player->AddArmorProficiency(allArmor);
        player->SendProficiency(ITEM_CLASS_WEAPON, player->GetWeaponProficiency());
        player->SendProficiency(ITEM_CLASS_ARMOR,  player->GetArmorProficiency());
    }
};

void AddUnboundScripts()
{
    new UnboundPlayerScript();
}
// cache-bust: 1781408710
WU_PAYLOAD_EOF_1

    cat > "$MODULE_DIR/src/UnboundSystem_loader.cpp" <<'WU_PAYLOAD_EOF_2'
// AzerothCore module loader — registers AddUnboundScripts() with the engine.
// The top-level modules/CMakeLists.txt calls Addmod_unboundScripts(),
// which this file defines by forwarding to our actual registration function.

void AddUnboundScripts();

void Addmod_unboundScripts()
{
    AddUnboundScripts();
}
WU_PAYLOAD_EOF_2

    cat > "$MODULE_DIR/npc_setup.sql" <<'WU_PAYLOAD_EOF_3'
-- Unbound Wrath Edition — Mentor NPC setup
-- Run once against acore_world AFTER the server has fully initialized.
-- Safe to re-run: INSERT IGNORE skips if entry already exists.
--
-- AzerothCore dropped `scale`, `mechanic_immune_mask`, and
-- `spell_school_immune_mask` from creature_template in migration
-- 2026_03_22_03.  This file uses the post-migration schema.
--
-- Apply:
--   docker exec -i <db-container> mysql -u root -p<pass> acore_world < npc_setup.sql
--
-- Then spawn in-game:
--   .npc add 900001

INSERT IGNORE INTO `creature_template`
    (`entry`, `name`, `subname`, `gossip_menu_id`,
     `minlevel`, `maxlevel`, `exp`, `faction`, `npcflag`,
     `speed_walk`, `speed_run`, `speed_swim`, `speed_flight`,
     `detection_range`, `rank`, `dmgschool`,
     `DamageModifier`, `BaseAttackTime`, `RangeAttackTime`,
     `BaseVariance`, `RangeVariance`,
     `unit_class`, `unit_flags`, `unit_flags2`, `dynamicflags`,
     `family`, `type`, `type_flags`,
     `lootid`, `pickpocketloot`, `skinloot`,
     `PetSpellDataId`, `VehicleId`, `mingold`, `maxgold`,
     `AIName`, `MovementType`, `HoverHeight`,
     `HealthModifier`, `ManaModifier`, `ArmorModifier`, `ExperienceModifier`,
     `RacialLeader`, `movementId`, `RegenHealth`,
     `flags_extra`, `ScriptName`, `VerifiedBuild`)
VALUES
    (900001, 'The Mentor', 'Unbound Class Trainer', 0,
     80, 80, 0, 35, 1,
     1.0, 1.14286, 1.0, 1.0,
     18, 0, 0,
     1.0, 1500, 2000,
     1.0, 1.0,
     1, 768, 2048, 0,
     0, 7, 0,
     0, 0, 0,
     0, 0, 0, 0,
     '', 0, 1.0,
     1.0, 1.0, 1.0, 1.0,
     0, 0, 1,
     2, '', 12340);

-- DisplayID 19097 = Ethereal Thief — final model, locked in by Joshua + Caitlin.
INSERT IGNORE INTO `creature_template_model`
    (`CreatureID`, `Idx`, `CreatureDisplayID`, `DisplayScale`, `Probability`, `VerifiedBuild`)
VALUES
    (900001, 0, 19097, 1.0, 1.0, 12340);
WU_PAYLOAD_EOF_3

    cat > "$MODULE_DIR/data/sql/db-world/01_unbound_world.sql" <<'WU_PAYLOAD_EOF_4'
-- Unbound Wrath Edition — world DB schema
-- Run against: acore_world
-- Safe to re-run: all tables use CREATE TABLE IF NOT EXISTS; INSERTs use IGNORE / ON DUPLICATE KEY.

-- ============================================================
-- Milestone ladder (how many gold each class unlock costs)
-- ============================================================
CREATE TABLE IF NOT EXISTS `unbound_milestones` (
    `milestone_index`    TINYINT UNSIGNED NOT NULL,
    `required_level`     TINYINT UNSIGNED NOT NULL,
    `unlock_cost_copper` INT UNSIGNED     NOT NULL,
    PRIMARY KEY (`milestone_index`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

INSERT IGNORE INTO `unbound_milestones` (`milestone_index`, `required_level`, `unlock_cost_copper`) VALUES
(1,  5,        0),       -- 1st class: free at level 5
(2,  25,   30000),       -- 2nd class: 3g at level 25
(3,  50,  800000),       -- 3rd class: 80g at level 50
(4,  70, 3000000),       -- 4th class: 300g at level 70
(5,  80,15000000);       -- 5th+ class: 1500g each at level 80 (index 5 is reused for all subsequent unlocks)

-- ============================================================
-- Purchasable spell catalog, populated from Playerbots trainer
-- data (npc_trainer IDs 200002–200018).
--
-- class_id follows WoW class constants:
--   1=Warrior  2=Paladin  3=Hunter   4=Rogue   5=Priest
--   7=Shaman   8=Mage     9=Warlock  11=Druid
-- ============================================================
CREATE TABLE IF NOT EXISTS `unbound_class_catalog` (
    `class_id`         TINYINT UNSIGNED NOT NULL,
    `spell_id`         INT UNSIGNED     NOT NULL,
    `gold_cost_copper` INT UNSIGNED     NOT NULL DEFAULT 0,
    `req_level`        TINYINT UNSIGNED NOT NULL DEFAULT 1,
    PRIMARY KEY (`class_id`, `spell_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- Populate from Playerbots synthetic trainer templates.
-- These are the same spells WotLK class trainers teach;
-- Playerbots loaded them into npc_trainer so bots can learn them.
-- Trainer template → class ID mapping (verified against creature subnames):
--   200002 = Warrior  200004 = Paladin  200006 = Druid
--   200008 = Mage     200010 = Warlock  200012 = Priest
--   200014 = Hunter   200016 = Rogue    200018 = Shaman
INSERT INTO `unbound_class_catalog` (`class_id`, `spell_id`, `gold_cost_copper`, `req_level`)
SELECT
    CASE `nt`.`ID`
        WHEN 200002 THEN 1
        WHEN 200004 THEN 2
        WHEN 200006 THEN 11
        WHEN 200008 THEN 8
        WHEN 200010 THEN 9
        WHEN 200012 THEN 5
        WHEN 200014 THEN 3
        WHEN 200016 THEN 4
        WHEN 200018 THEN 7
    END                         AS `class_id`,
    `nt`.`SpellID`              AS `spell_id`,
    `nt`.`MoneyCost`            AS `gold_cost_copper`,
    `nt`.`ReqLevel`             AS `req_level`
FROM `npc_trainer` `nt`
WHERE `nt`.`ID` IN (200002, 200004, 200006, 200008, 200010, 200012, 200014, 200016, 200018)
  AND `nt`.`SpellID` > 0
ON DUPLICATE KEY UPDATE
    `gold_cost_copper` = VALUES(`gold_cost_copper`),
    `req_level`        = VALUES(`req_level`);

-- ============================================================
-- Mentor NPC creature_template + model:
-- NOT applied here to avoid touching vanilla tables in the
-- auto-update path.  Run npc_setup.sql manually once, or use:
--   .npc add 900001   (after running npc_setup.sql)
-- ============================================================
WU_PAYLOAD_EOF_4

    cat > "$MODULE_DIR/data/sql/db-world/02_fix_catalog_req_level.sql" <<'WU_PAYLOAD_EOF_5'
-- Fix: lower tier-1 spell req_level from 8 → 1 so newly unlocked classes
-- have buyable abilities immediately (first milestone unlocks at level 5).
-- All classes had min req_level=8 from Playerbots trainer data, causing a
-- level 5-7 player to see "no abilities available" after unlocking a class.
UPDATE `unbound_class_catalog` SET `req_level` = 1 WHERE `req_level` <= 8;
WU_PAYLOAD_EOF_5

    cat > "$MODULE_DIR/data/sql/db-world/03_creation_gift_spells.sql" <<'WU_PAYLOAD_EOF_6'
-- Unbound Wrath Edition — per-class character-creation gift spells
-- Applied to: acore_world.playercreateinfo_spell_custom
--
-- These are granted for FREE when a player unlocks a class via the Mentor NPC,
-- matching exactly what a freshly-created level-1 character of that class receives.
-- "Aelric opens the door; trainers fill the rooms."
--
-- classmask = 2^(classId-1):
--   Warrior=1  Paladin=2  Hunter=4  Rogue=8  Priest=16
--   Shaman=64  Mage=128   Warlock=256  Druid=1024
-- racemask = 0 means all races.
--
-- Apply: docker exec ac-database mysql -u root -ppassword acore_world < this_file.sql

-- Clear any previous entries so this file is safe to re-run
DELETE FROM playercreateinfo_spell_custom WHERE racemask = 0 AND classmask IN (1,2,4,8,16,64,128,256,1024);

-- ── Warrior (classmask=1) ────────────────────────────────────────────────────
-- All 3 stances + starting combat abilities
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 1, 2457, 'Warrior - Battle Stance'),
(0, 1, 71,   'Warrior - Defensive Stance'),
(0, 1, 2458, 'Warrior - Berserker Stance'),
(0, 1, 78,   'Warrior - Heroic Strike r1'),
(0, 1, 6673, 'Warrior - Battle Shout r1'),
(0, 1, 100,  'Warrior - Charge r1');

-- ── Paladin (classmask=2) ────────────────────────────────────────────────────
-- Judgement is the core rotation ability — without it, an Unbound Paladin's
-- Seal is permanently inert. The trainer-taught ID (10321) has a
-- SPELL_EFFECT_LEARN_SPELL effect, which Mentor-driven grants silently fail
-- (see 14_judgement_fix.sql). This row is inserted as 10321 and corrected to
-- 20271 ("Judgement of Light" — the real castable Judgement button, confirmed
-- working via Testpal) by 14_judgement_fix.sql, which must run after this file.
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 2, 635,   'Paladin - Holy Light r1'),
(0, 2, 20154, 'Paladin - Seal of Righteousness r1'),
(0, 2, 465,   'Paladin - Devotion Aura r1'),
(0, 2, 10321, 'Paladin - Judgement');

-- ── Hunter (classmask=4) ────────────────────────────────────────────────────
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 4, 75,    'Hunter - Auto Shot'),
(0, 4, 2973,  'Hunter - Raptor Strike r1'),
(0, 4, 13165, 'Hunter - Aspect of the Hawk r1');

-- ── Rogue (classmask=8) ─────────────────────────────────────────────────────
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 8, 1784, 'Rogue - Stealth r1'),
(0, 8, 1752, 'Rogue - Sinister Strike r1'),
(0, 8, 2098, 'Rogue - Eviscerate r1');

-- ── Priest (classmask=16) ────────────────────────────────────────────────────
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 16, 585,  'Priest - Smite r1'),
(0, 16, 2050, 'Priest - Lesser Heal r1');

-- ── Shaman (classmask=64) ────────────────────────────────────────────────────
-- All 4 starter totems are gifted so Shaman spells that require totems work
-- immediately. 2484=Earthbind Totem; totem items (5175-5178) are given by
-- GrantClassGiftItems in the Lua (CLASS_GIFT_ITEMS[7]).
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 64, 403,  'Shaman - Lightning Bolt r1'),
(0, 64, 331,  'Shaman - Healing Wave r1'),
(0, 64, 8071, 'Shaman - Stoneskin Totem r1 (Earth)'),
(0, 64, 8042, 'Shaman - Searing Totem r1 (Fire)'),
(0, 64, 5394, 'Shaman - Healing Stream Totem r1 (Water)'),
(0, 64, 8512, 'Shaman - Windfury Totem r1 (Air)'),
(0, 64, 2484, 'Shaman - Earthbind Totem');

-- ── Mage (classmask=128) ────────────────────────────────────────────────────
-- Arcane Intellect (1459) is a key Mage utility spell taught by trainer at level 1
-- but not included in Playerbots creation data — must be explicitly gifted.
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 128, 133,  'Mage - Fireball r1'),
(0, 128, 168,  'Mage - Frost Armor r1'),
(0, 128, 1459, 'Mage - Arcane Intellect r1');

-- ── Warlock (classmask=256) ─────────────────────────────────────────────────
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 256, 686, 'Warlock - Shadow Bolt r1'),
(0, 256, 687, 'Warlock - Demon Skin'),
(0, 256, 688, 'Warlock - Summon Imp');

-- ── Druid (classmask=1024) ───────────────────────────────────────────────────
-- Bear Form and Aquatic Form are sold via the Mentor catalog, not gifted free.
-- Bear Form: 5 silver (500 copper) — see 04_catalog_druid_forms.sql
-- Aquatic Form: already in catalog at 900 copper from Playerbots trainer data.
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 1024, 5176, 'Druid - Wrath r1'),
(0, 1024, 5185, 'Druid - Healing Touch r1');
WU_PAYLOAD_EOF_6

    cat > "$MODULE_DIR/data/sql/db-world/04_catalog_druid_forms.sql" <<'WU_PAYLOAD_EOF_7'
-- Unbound Wrath Edition — add missing druid shapeshift forms to catalog
-- Bear Form was a class quest in vanilla, so the Playerbots trainer
-- template (200006) never included it.  Add it manually at a custom price.
-- All other forms are already present from the Playerbots trainer data.

INSERT INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level)
VALUES (11, 5487, 500, 10)
ON DUPLICATE KEY UPDATE gold_cost_copper = 500, req_level = 10;
WU_PAYLOAD_EOF_7

    cat > "$MODULE_DIR/data/sql/db-world/05_individual_purchase_prereqs.sql" <<'WU_PAYLOAD_EOF_8'
-- Unbound Wrath Edition — individual spell purchase with rank prerequisites
-- Applied to: acore_world
--
-- 1. Add prereq_spell column to unbound_class_catalog
--    Populated from npc_trainer.ReqSpell (the prerequisite rank).
-- 2. Update shaman creation gifts: add missing starter totems.

-- ── 1. prereq_spell column ────────────────────────────────────────────────
-- MySQL 8 on this server doesn't support ADD COLUMN IF NOT EXISTS; use stored proc pattern
SET @col_exists = (
    SELECT COUNT(*) FROM information_schema.COLUMNS
    WHERE TABLE_SCHEMA = 'acore_world'
      AND TABLE_NAME   = 'unbound_class_catalog'
      AND COLUMN_NAME  = 'prereq_spell');
SET @sql = IF(@col_exists = 0,
    'ALTER TABLE unbound_class_catalog ADD COLUMN prereq_spell INT UNSIGNED NOT NULL DEFAULT 0',
    'SELECT ''prereq_spell column already exists''');
PREPARE stmt FROM @sql; EXECUTE stmt; DEALLOCATE PREPARE stmt;

-- Populate prereq_spell from npc_trainer.ReqSpell for each catalog entry.
-- Trainer ID → class mapping:
--   200002=Warrior 200004=Paladin 200006=Druid  200008=Mage    200010=Warlock
--   200012=Priest  200014=Hunter  200016=Rogue   200018=Shaman
UPDATE unbound_class_catalog uc
INNER JOIN npc_trainer nt
    ON nt.SpellID = uc.spell_id
    AND nt.ID IN (200002,200004,200006,200008,200010,200012,200014,200016,200018)
SET uc.prereq_spell = nt.ReqSpell
WHERE nt.ReqSpell > 0 AND uc.prereq_spell = 0;

-- ── 2. Shaman starter totems (missing from Playerbots trainer template) ───
-- Each element's basic rank-1 totem, gifted free at class unlock.
-- Already in playercreateinfo_spell_custom for classmask=64.
DELETE FROM playercreateinfo_spell_custom WHERE classmask = 64 AND racemask = 0;
INSERT INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES
(0, 64, 403,  'Shaman - Lightning Bolt r1'),
(0, 64, 331,  'Shaman - Healing Wave r1'),
(0, 64, 8071, 'Shaman - Stoneskin Totem r1 (Earth)'),
(0, 64, 8042, 'Shaman - Searing Totem r1 (Fire)'),
(0, 64, 5394, 'Shaman - Healing Stream Totem r1 (Water)'),
(0, 64, 8512, 'Shaman - Windfury Totem r1 (Air)'),
(0, 64, 2484, 'Shaman - Earthbind Totem');
WU_PAYLOAD_EOF_8

    cat > "$MODULE_DIR/data/sql/db-world/06_universal_skill_access.sql" <<'WU_PAYLOAD_EOF_9'
-- Unbound Wrath Edition — universal skill access for Unbound characters
-- Applied to: acore_world.skillraceclassinfo_dbc
--
-- Problem: AzerothCore's _LoadSkills validates every skill against
-- GetSkillRaceClassInfo(skill, race, class). If no entry exists for that
-- skill+race+class combo, the skill is stripped from memory on every login.
-- This prevents Unbound characters from keeping cross-class skills (Staves
-- for a Paladin, Daggers for a Warrior, etc.).
--
-- Fix: insert rows with ClassMask=0, RaceMask=0 for every skill we need.
-- ClassMask=0 → all classes. RaceMask=0 → all races. This makes the check
-- always return a valid entry, allowing any character to keep any listed skill.
--
-- These rows are loaded at server start via storage.LoadFromDB("skillraceclassinfo_dbc")
-- called inside LoadDBC() in DBCStores.cpp.  Restart required after applying.
--
-- Safe to re-run: DELETE + re-INSERT on our ID range.

DELETE FROM skillraceclassinfo_dbc WHERE ID >= 10000;

-- Auto-generate one row per skill.
-- Fields: ID, SkillID, RaceMask, ClassMask, Flags, MinLevel, SkillTierID, SkillCostIndex
-- ClassMask=0 = all classes, RaceMask=0 = all races, SkillTierID=0 = level-scaled.

INSERT INTO skillraceclassinfo_dbc
  (ID, SkillID, RaceMask, ClassMask, Flags, MinLevel, SkillTierID, SkillCostIndex)
VALUES
-- ── Weapon skills ────────────────────────────────────────────────────────────
(10001,  43, 0, 0, 0, 0, 0, 0),   -- Swords
(10002,  44, 0, 0, 0, 0, 0, 0),   -- Axes
(10003,  45, 0, 0, 0, 0, 0, 0),   -- Bows
(10004,  46, 0, 0, 0, 0, 0, 0),   -- Guns
(10005,  54, 0, 0, 0, 0, 0, 0),   -- Maces
(10006,  55, 0, 0, 0, 0, 0, 0),   -- Two-Handed Swords
(10007, 118, 0, 0, 0, 0, 0, 0),   -- Dual Wield
(10008, 136, 0, 0, 0, 0, 0, 0),   -- Staves
(10009, 160, 0, 0, 0, 0, 0, 0),   -- Two-Handed Maces
(10010, 162, 0, 0, 0, 0, 0, 0),   -- Unarmed
(10011, 172, 0, 0, 0, 0, 0, 0),   -- Two-Handed Axes
(10012, 173, 0, 0, 0, 0, 0, 0),   -- Daggers
(10013, 176, 0, 0, 0, 0, 0, 0),   -- Thrown
(10014, 226, 0, 0, 0, 0, 0, 0),   -- Crossbows
(10015, 228, 0, 0, 0, 0, 0, 0),   -- Wands
(10016, 229, 0, 0, 0, 0, 0, 0),   -- Polearms
(10017, 433, 0, 0, 0, 0, 0, 0),   -- Shield
(10018, 473, 0, 0, 0, 0, 0, 0),   -- Fist Weapons
-- ── Armor skills ─────────────────────────────────────────────────────────────
(10019, 293, 0, 0, 0, 0, 0, 0),   -- Plate Mail
(10020, 413, 0, 0, 0, 0, 0, 0),   -- Mail
(10021, 414, 0, 0, 0, 0, 0, 0),   -- Leather
(10022, 415, 0, 0, 0, 0, 0, 0),   -- Cloth
-- ── Class spellbook tab skills (from playercreateinfo_skills classMask!=0) ───
-- These allow Unbound characters to keep spellbook tabs from unlocked classes.
(10030,   6, 0, 0, 0, 0, 0, 0),
(10031,   8, 0, 0, 0, 0, 0, 0),
(10032,  26, 0, 0, 0, 0, 0, 0),
(10033,  38, 0, 0, 0, 0, 0, 0),
(10034,  39, 0, 0, 0, 0, 0, 0),
(10035,  50, 0, 0, 0, 0, 0, 0),
(10036,  51, 0, 0, 0, 0, 0, 0),
(10037,  56, 0, 0, 0, 0, 0, 0),
(10038,  78, 0, 0, 0, 0, 0, 0),
(10039, 129, 0, 0, 0, 0, 0, 0),
(10040, 134, 0, 0, 0, 0, 0, 0),
(10041, 163, 0, 0, 0, 0, 0, 0),
(10042, 184, 0, 0, 0, 0, 0, 0),
(10043, 237, 0, 0, 0, 0, 0, 0),
(10044, 253, 0, 0, 0, 0, 0, 0),
(10045, 256, 0, 0, 0, 0, 0, 0),
(10046, 257, 0, 0, 0, 0, 0, 0),
(10047, 267, 0, 0, 0, 0, 0, 0),
(10048, 354, 0, 0, 0, 0, 0, 0),
(10049, 355, 0, 0, 0, 0, 0, 0),
(10050, 373, 0, 0, 0, 0, 0, 0),
(10051, 374, 0, 0, 0, 0, 0, 0),
(10052, 375, 0, 0, 0, 0, 0, 0),
(10053, 573, 0, 0, 0, 0, 0, 0),
(10054, 574, 0, 0, 0, 0, 0, 0),
(10055, 593, 0, 0, 0, 0, 0, 0),
(10056, 594, 0, 0, 0, 0, 0, 0),
(10057, 613, 0, 0, 0, 0, 0, 0),
(10058, 762, 0, 0, 0, 0, 0, 0),
(10059, 770, 0, 0, 0, 0, 0, 0),
(10060, 771, 0, 0, 0, 0, 0, 0),
(10061, 772, 0, 0, 0, 0, 0, 0);
WU_PAYLOAD_EOF_9

    cat > "$MODULE_DIR/data/sql/db-world/07_mentor_stone.sql" <<'WU_PAYLOAD_EOF_10'
-- Unbound Wrath Edition — Unbounding Mentor Stone
-- Applied to: acore_world
--
-- Creates a permanent use-item (entry 900100) given to every character at login.
-- Right-clicking summons the Mentor NPC (entry 900001) for 3 minutes.
--
-- Safe to re-run: uses INSERT IGNORE / ON DUPLICATE KEY UPDATE.
--
-- IMPORTANT — root cause writeup (see ~/wow-server-playerbots/CLAUDE.md "RESOLVED BUGS"):
-- The item's spellid_1 MUST point at a real, client-known spell ID (Blizzard IDs
-- top out around ~71000). A custom server-only ID like 900200 is invisible to the
-- client's binary Spell.dbc — the client silently refuses to recognize the item as
-- usable and never even sends CMSG_USE_ITEM. spellid_1 = 433 ("Food") was chosen as
-- a harmless defense-in-depth fallback: its only effect is a heal-over-time that
-- fizzles unless seated, so even if the Lua cancellation in unbound_mentor.lua were
-- ever bypassed, nothing disruptive happens (unlike the Hearthstone teleport that
-- was tried first during diagnosis). The Lua ITEM_EVENT_ON_USE handler unconditionally
-- returns true to cancel the real cast — the Lua-side STONE_LAST_USE 180s cooldown
-- fully replaces spellcooldown_1 as the gameplay cooldown.
--
-- displayid = 6418 (INV_Misc_Rune_01) — a Vanilla-era rune-stone icon guaranteed
-- present in any 3.3.5a client; newer WotLK icons (e.g. 58413) can render as "?"
-- on clients whose MPQ data is missing those textures.

-- ── Item 900100: Unbounding Mentor Stone ─────────────────────────────────────
-- class=15 (Miscellaneous), InventoryType=0 (non-equippable bag item).
-- maxcount=1 ensures only one copy can be held at a time.
-- spellid_1=433 (Food — real client-known spell, cancelled by Lua) +
-- spellcooldown_1=180000 ms (3 min, superseded by the Lua-side cooldown guard).
INSERT INTO item_template
    (entry, class, subclass, SoundOverrideSubclass, name,
     displayid, Quality, Flags, FlagsExtra,
     BuyCount, BuyPrice, SellPrice,
     InventoryType, AllowableClass, AllowableRace,
     ItemLevel, RequiredLevel,
     maxcount, stackable,
     spellid_1, spelltrigger_1, spellcharges_1, spellppmRate_1,
     spellcooldown_1, spellcategory_1, spellcategorycooldown_1,
     description, ScriptName)
VALUES
    (900100, 15, 0, -1, 'Unbounding Mentor Stone',
     6418, 3, 0, 0,
     1, 0, 0,
     0, -1, -1,
     1, 0,
     1, 1,
     433, 0, 0, 0,
     180000, 0, -1,
     'Summons your Unbounding Mentor for 3 minutes. (3 min cooldown)', '')
ON DUPLICATE KEY UPDATE
    name              = VALUES(name),
    displayid         = VALUES(displayid),
    Quality           = VALUES(Quality),
    spellid_1         = VALUES(spellid_1),
    spelltrigger_1    = VALUES(spelltrigger_1),
    spellcooldown_1   = VALUES(spellcooldown_1),
    description       = VALUES(description);

-- ── Give stone to all new characters at creation ─────────────────────────────
-- race=0 means any race; class entries cover all WotLK playable classes.
-- The Lua login hook in unbound_mentor.lua also gives it to existing characters.
INSERT IGNORE INTO playercreateinfo_item (race, class, itemid, amount) VALUES
(0,  1, 900100, 1),   -- Warrior
(0,  2, 900100, 1),   -- Paladin
(0,  3, 900100, 1),   -- Hunter
(0,  4, 900100, 1),   -- Rogue
(0,  5, 900100, 1),   -- Priest
(0,  6, 900100, 1),   -- Death Knight
(0,  7, 900100, 1),   -- Shaman
(0,  8, 900100, 1),   -- Mage
(0,  9, 900100, 1),   -- Warlock
(0, 11, 900100, 1);   -- Druid
WU_PAYLOAD_EOF_10

    cat > "$MODULE_DIR/data/sql/db-world/08_catalog_additions.sql" <<'WU_PAYLOAD_EOF_11'
-- Unbound Wrath Edition — catalog gap fill
-- Applied to: acore_world.unbound_class_catalog
--
-- These spells appear in WotLK class trainers (trainer_spell IDs 1-34) but were
-- absent from the catalog, which was originally populated from Playerbots
-- synthetic trainer data (npc_trainer IDs 200002–200018). The Playerbots
-- templates omit some low-level rank-1 spells, particularly those that native
-- characters receive at creation.
--
-- Spells already in creation gifts (playercreateinfo_spell_custom) are excluded
-- because Unbound players receive them for free at unlock.
-- Prices match WotLK trainer MoneyCost values.
--
-- Safe to re-run: uses INSERT IGNORE.
-- prereq_spell defaults to 0; PREREQ_MAP (built from catalog req_level order at
-- script load) will infer rank chains automatically.

-- prereq_spell is omitted; it defaults to 0 (added by 05_individual_purchase_prereqs.sql).
-- PREREQ_MAP in the Lua infers rank chains from req_level ordering at script load.
INSERT IGNORE INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level) VALUES

-- ── Warrior (class_id=1) ──────────────────────────────────────────────────────
-- Rend r1 (772), Parry (3127), Thunder Clap r1 (6343), Victory Rush (34428)
(1,   772,   100, 4),
(1,  3127,   100, 6),
(1,  6343,   100, 6),
(1, 34428,   100, 6),

-- ── Paladin (class_id=2) ──────────────────────────────────────────────────────
-- Judgement (10321), Blessing of Might r1 (19740),
-- Divine Protection r1 (498), Holy Light r2 (639),
-- Seal of Vengeance (31801 — high-level Retribution seal)
(2, 10321,   100, 4),
(2, 19740,   100, 4),
(2,   498,   100, 6),
(2,   639,   100, 6),
(2, 31801, 67000,64),

-- ── Hunter (class_id=3) ───────────────────────────────────────────────────────
-- Track Beasts (1494), Serpent Sting r1 (1978),
-- Hunter's Mark r1 (1130), Arcane Shot r1 (3044)
(3, 1494,    10, 2),
(3, 1978,   100, 4),
(3, 1130,   100, 6),
(3, 3044,   100, 6),

-- ── Rogue (class_id=4) ────────────────────────────────────────────────────────
-- Backstab r1 (53), Pickpocket (921),
-- Gouge r1 (1757), Ambush r1 (1776)
(4,   53,   100, 4),
(4,  921,   100, 4),
(4, 1757,   100, 6),
(4, 1776,   100, 6),

-- ── Priest (class_id=5) ───────────────────────────────────────────────────────
-- Power Word: Fortitude r1 (1243), Shadow Word: Pain r1 (589),
-- Lesser Heal r2 (2052 — rank 2 of creation gift 2050), Power Word: Shield r1 (17),
-- Smite r2 (591 — rank 2 of creation gift 585)
(5, 1243,    10, 1),
(5,  589,   100, 4),
(5, 2052,   100, 4),
(5,   17,   100, 6),
(5,  591,   100, 6),

-- ── Shaman (class_id=7) ───────────────────────────────────────────────────────
-- Rockbiter Weapon r1 (8017), Earth Shock r1 (8042 in gifts — skip),
-- Healing Wave r2 (332 — rank 2 of creation gift 331), Earthbind Totem (2484 in gifts — skip)
(7, 8017,    10, 1),
(7,  332,   100, 6),

-- ── Mage (class_id=8) ─────────────────────────────────────────────────────────
-- Arcane Intellect r1 (1459 — also in creation gifts; added here so higher ranks'
-- prereq chain resolves correctly and re-purchase is possible if lost)
-- Frostbolt r1 (116), Conjure Food r1 (587→5504),
-- Conjure Water r1 (143), Conjure Food r1 (587),
-- Fire Blast r1 (2136), Detect Magic (2855)
(8, 1459,    10, 1),
(8,  116,   100, 4),
(8, 5504,   100, 4),
(8,  143,   100, 6),
(8,  587,   100, 6),
(8, 2136,   100, 6),
(8, 2855,  2000,16),

-- ── Warlock (class_id=9) ──────────────────────────────────────────────────────
-- Immolate r1 (348), Corruption r1 (172), Curse of Weakness r1 (702),
-- Shadow Bolt r2 (695 — rank 2 of creation gift 686), Life Tap r1 (1454)
(9,  348,    10, 3),
(9,  172,   100, 4),
(9,  702,   100, 4),
(9,  695,   100, 6),
(9, 1454,   100, 6),

-- ── Druid (class_id=11) ───────────────────────────────────────────────────────
-- Mark of the Wild r1 (1126), Rejuvenation r1 (774), Moonfire r1 (8921),
-- Thorns r1 (467), Wrath r2 (5177 — rank 2 of creation gift 5176)
(11, 1126,    10, 1),
(11,  774,   100, 4),
(11, 8921,   100, 4),
(11,  467,   100, 6),
(11, 5177,   100,  6);
WU_PAYLOAD_EOF_11


    cat > "$MODULE_DIR/data/sql/db-world/10_catalog_audit_fixes.sql" <<'WU_PAYLOAD_EOF_14'
-- Unbound Wrath Edition — catalog req_level self-heal vs real WotLK trainers
-- Applied to: acore_world.unbound_class_catalog
--
-- 02_fix_catalog_req_level.sql blanket-lowered every entry with req_level <= 8
-- to req_level = 1 so a level-5 class unlock always had something buyable.
-- That also dragged down ~30 legitimate rank-2/utility spells (Heroic Strike
-- r2, Hammer of Justice, Aspect of the Hawk r2, etc.) that real trainers gate
-- at level 8, plus a separate batch of level 60/70 spells that were a tier
-- below their real 61/71 requirement.
--
-- Fix: pull req_level straight from the real class trainers (trainer +
-- trainer_spell, Type=0, Requirement=class_id) and apply it wherever the
-- catalog disagrees. Verified live (2026-06-13): this only ever RAISES
-- req_level — level-5 unlocks stay buyable because 08_catalog_additions.sql
-- already seeds req_level 1/2/4/6 entries per class.
--
-- Requires a worldserver restart afterward so PREREQ_MAP (built once at Eluna
-- load from catalog req_level order) re-sorts rank chains with the corrected
-- levels.
--
-- Safe to re-run: WHERE clause only touches rows that still disagree.

UPDATE unbound_class_catalog c
JOIN (
    SELECT t.Requirement AS class_id, ts.SpellID AS spell_id, MIN(ts.ReqLevel) AS req_level
    FROM trainer t
    JOIN trainer_spell ts ON ts.TrainerId = t.Id
    WHERE t.Type = 0 AND t.Requirement IN (1,2,3,4,5,7,8,9,11)
    GROUP BY t.Requirement, ts.SpellID
) rts ON rts.class_id = c.class_id AND rts.spell_id = c.spell_id
SET c.req_level = rts.req_level
WHERE c.req_level <> rts.req_level;

-- ============================================================
-- Hunter (class_id=3) gap fill: Aspect of the Monkey
-- ============================================================
-- 13163 = Aspect of the Monkey, a real Hunter trainer spell (req_level 4,
-- 100c) missing from the catalog. It was stuck in limbo because
-- 03_creation_gift_spells.sql's Hunter gift used to point at 13163 by mistake
-- (intending Aspect of the Hawk r1 = 13165, now corrected) — so 13163 was
-- neither gifted nor purchasable. prereq_spell defaults to 0: Aspect of the
-- Monkey has no rank chain.
INSERT IGNORE INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level) VALUES
(3, 13163, 100, 4);
WU_PAYLOAD_EOF_14

    cat > "$MODULE_DIR/data/sql/db-world/11_catalog_gap_additions.sql" <<'WU_PAYLOAD_EOF_15'
-- Unbound Wrath Edition — catalog gap fill: Mage teleports/portals + Paladin mount
-- Applied to: acore_world.unbound_class_catalog
--
-- Remaining real-trainer spells identified by the level 1-80 catalog audit
-- (2026-06-13) that were missing entirely. req_level/cost taken directly from
-- trainer_spell (Type=0, Requirement=class_id).
--
-- Deliberately NOT added (see audit notes):
--   - Paladin Summon Charger (34767, req40/3500c): trainer_spell gates it on
--     ReqAbility1=33391 ("Journeyman Riding", itself a 1000g Riding-trainer
--     spell at req60) and ReqAbility2=34769 (a second, untaught "Summon
--     Warhorse" companion spell). That prereq chain reaches into the Riding
--     skill system, which Unbound doesn't model — locked until a proper
--     prereq/talent system exists, per Joshua's call on Seal of Corruption.
--   - Paladin Seal of Corruption (53736): per Wowhead, this is the
--     Horde-faction name for the same "Holy Vengeance" seal as Seal of
--     Vengeance (31801, already in the catalog via 08_catalog_additions.sql)
--     — Alliance/Horde naming variants of one ability, not a talent rank or
--     an upgrade. Adding it would just duplicate 31801 under another name.
--
-- prereq_spell defaults to 0; PREREQ_MAP (built from catalog req_level order
-- at script load) infers same-named rank chains automatically.
-- Safe to re-run: uses INSERT IGNORE.

INSERT IGNORE INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level) VALUES

-- ── Paladin (class_id=2) ──────────────────────────────────────────────────────
-- Summon Warhorse (34768) — basic Paladin mount, no prereqs.
(2, 34768, 3500, 20),

-- ── Mage (class_id=8) — Teleport/Portal lines ─────────────────────────────────
-- req 20, 2000c: Teleport: Stormwind/Ironforge/Undercity/Orgrimmar/Exodar/Silvermoon/Stonard/Theramore
(8,  3561, 2000, 20),
(8,  3562, 2000, 20),
(8,  3563, 2000, 20),
(8,  3567, 2000, 20),
(8, 32271, 2000, 20),
(8, 32272, 2000, 20),
(8, 49358, 2000, 20),
(8, 49359, 2000, 20),

-- req 30, 8000c: Teleport: Darnassus/Thunder Bluff
(8,  3565, 8000, 30),
(8,  3566, 8000, 30),

-- req 35, 15000c: Portal: Theramore/Stonard
(8, 49360, 15000, 35),
(8, 49361, 15000, 35),

-- req 40, 15000c: Portal: Stormwind/Ironforge/Orgrimmar/Undercity/Exodar/Silvermoon
(8, 10059, 15000, 40),
(8, 11416, 15000, 40),
(8, 11417, 15000, 40),
(8, 11418, 15000, 40),
(8, 32266, 15000, 40),
(8, 32267, 15000, 40),

-- req 50, 32000c: Portal: Darnassus/Thunder Bluff
(8, 11419, 32000, 50),
(8, 11420, 32000, 50),

-- req 60, 20000c: Teleport: Shattrath (Aldor/Scryer faction-name variants)
(8, 33690, 20000, 60),
(8, 35715, 20000, 60),

-- req 65, 150000c: Portal: Shattrath (Aldor/Scryer faction-name variants)
(8, 33691, 150000, 65),
(8, 35717, 150000, 65),

-- req 71/74: Teleport/Portal: Dalaran
(8, 53140, 100000, 71),
(8, 53142, 100000, 74);
WU_PAYLOAD_EOF_15

    cat > "$MODULE_DIR/data/sql/db-world/12_mount_spell_fix.sql" <<'WU_PAYLOAD_EOF_16'
-- Unbound Wrath Edition — Paladin/Warlock mount purchase fix
-- Applied to: acore_world.unbound_class_catalog
--
-- Reported by Joshua: Summon Warhorse (Paladin) and Summon Felsteed (Warlock)
-- can be "bought" from the Mentor — gold is deducted and a success message is
-- shown — but the spell never appears in the spellbook, isn't selectable as a
-- mount, and the entry reappears in Browse as if never purchased.
--
-- Root cause (confirmed against AzerothCore source + Spell.dbc, 2026-06-13):
-- 34768 ("Summon Warhorse") and 1710 ("Summon Felsteed") are trainer TEACH
-- spells — their Effect array contains SPELL_EFFECT_LEARN_SPELL (36) twice,
-- meant to recursively grant the real mount spell + Apprentice Riding via the
-- temporary-learn trainer path. Player::_addSpell() (Player.cpp ~3192)
-- explicitly refuses any spell with SPELL_EFFECT_LEARN_SPELL when called via
-- the non-temporary player:LearnSpell() the Mentor uses — it adds the spell to
-- m_spells, immediately erases it, and returns false. The Lua never checks
-- that return value, so gold is taken and "Learned!" fires for a purchase that
-- silently did nothing.
--
-- Fix: point the catalog at the REAL castable mount spell each teach-spell was
-- meant to grant (same display name, same cost/req_level). Neither real mount
-- has a LEARN_SPELL effect, so player:LearnSpell() works normally — same code
-- path as Dreadsteed (23161), which already works correctly:
--   34768 "Summon Warhorse" (teach) -> 34769 "Summon Warhorse" (real mount)
--   1710  "Summon Felsteed" (teach) -> 5784  "Felsteed"        (real mount)
--
-- Note: both real mounts also require Apprentice Riding (skill 762 >= 75) to
-- be summonable once learned. Not modeled by the catalog, but
-- 06_universal_skill_access.sql already makes Riding (762) valid for every
-- class/race, and any character who trained a faction mount in the normal
-- 20-40 leveling range will already have it (confirmed live: Testmage has
-- Riding 150/150). Left out here to avoid scope creep into a riding-skill
-- purchase system — flag to Joshua if a player reports the mount is in their
-- spellbook but won't summon.
--
-- No worldserver restart required: the catalog is read live on every
-- Browse/Buy, and PREREQ_MAP doesn't reference these IDs (mounts have no rank
-- chain). Safe to re-run: each pair is a DELETE of the old spell_id followed
-- by INSERT IGNORE of the new one, so re-running never collides on the
-- (class_id, spell_id) primary key — even if an earlier INSERT IGNORE
-- migration re-creates the old row after this fix already ran once (e.g.
-- after an uninstall/reinstall where AzerothCore's update-tracking and the
-- catalog data fall out of sync). A plain UPDATE...SET spell_id=<new> would
-- collide with the primary key in that case since <new> already exists.

DELETE FROM unbound_class_catalog WHERE class_id = 2 AND spell_id = 34768;
INSERT IGNORE INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level) VALUES (2, 34769, 3500, 20);

DELETE FROM unbound_class_catalog WHERE class_id = 9 AND spell_id = 1710;
INSERT IGNORE INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level) VALUES (9, 5784, 10000, 20);
WU_PAYLOAD_EOF_16

    cat > "$MODULE_DIR/data/sql/db-world/13_flight_form_fix.sql" <<'WU_PAYLOAD_EOF_17'
-- Unbound Wrath Edition — Druid Flight Form purchase fix
-- Applied to: acore_world.unbound_class_catalog
--
-- Same bug class as 12_mount_spell_fix.sql, found by auditing every catalog
-- spell_id against Spell.dbc for SPELL_EFFECT_LEARN_SPELL (36).
--
-- 33950 "Flight Form" (Druid, req68/34000c) is a trainer TEACH spell —
-- Effects=[36,36,44], TriggerSpells=[33943 "Flight Form", 34090 "Expert
-- Riding"]. Player::_addSpell() erases any spell with SPELL_EFFECT_LEARN_SPELL
-- when learned via the non-temporary player:LearnSpell() the Mentor uses, so
-- buying 33950 took gold and granted nothing — identical symptom to the
-- mount bug (reappears in Browse, not in spellbook, not usable).
--
-- Fix: point the catalog at 33943, the real castable "Flight Form" shapeshift
-- spell (same name, same cost/req_level, Effects=[6,6,6] — no LEARN_SPELL,
-- learns normally).
--
-- Note: 34090 "Expert Riding" (skill 762 -> 225, needed to actually fly) is
-- not granted by this fix, same rationale as 12_mount_spell_fix.sql — Riding
-- skill is already universally accessible (06_universal_skill_access.sql) and
-- most level-68+ characters will already have at least Artisan Riding (300)
-- from normal flying-mount training, which exceeds the 225 Expert requirement.
--
-- No worldserver restart required. Safe to re-run: DELETE the old spell_id
-- then INSERT IGNORE the new one, so re-running never collides on the
-- (class_id, spell_id) primary key (see 12_mount_spell_fix.sql for why a
-- plain UPDATE isn't safe here).

DELETE FROM unbound_class_catalog WHERE class_id = 11 AND spell_id = 33950;
INSERT IGNORE INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level) VALUES (11, 33943, 34000, 68);
WU_PAYLOAD_EOF_17

    cat > "$MODULE_DIR/data/sql/db-world/14_judgement_fix.sql" <<'WU_PAYLOAD_EOF_18'
-- Unbound Wrath Edition — Paladin Judgement purchase/gift fix
-- Applied to: acore_world.unbound_class_catalog, acore_world.playercreateinfo_spell_custom
--
-- Same bug class as 12_mount_spell_fix.sql / 13_flight_form_fix.sql.
--
-- Confirmed live (2026-06-13) with Testpal (Rogue, second class Paladin via
-- Mentor at level 5): the Paladin-unlock creation gifts 635 (Holy Light r1),
-- 20154 (Seal of Righteousness r1) and 465 (Devotion Aura r1) were granted
-- correctly, but 10321 ("Judgement") was not — and buying "Judgement" from
-- the Mentor (req4/100c) takes gold, grants nothing, and the entry never
-- disappears from Browse ("keeps buying over and over").
--
-- 10321 "Judgement" is a trainer TEACH spell: Effects=[36,36,0],
-- TriggerSpells=[20271 "Judgement of Light", 21084 "Seal of Righteousness"].
-- Both player:LearnSpell() (Mentor purchase) and the Mentor's class-unlock
-- gift-granting code call learnSpell() non-temporary, which Player::_addSpell
-- erases-and-rejects for any SPELL_EFFECT_LEARN_SPELL spell. A *native*
-- character creation grants 10321 via AzerothCore's temporary=true path
-- (which DOES honor LEARN_SPELL), so freshly-rolled Paladins are unaffected —
-- only Mentor-driven unlocks and Mentor purchases hit the broken path.
--
-- Fix part 1 (catalog, live immediately, no restart): point the catalog entry
-- at 20271 "Judgement of Light" — the actual SCRIPT_EFFECT spell WotLK
-- Paladins use as their "Judgement" button (it judges using whichever Seal is
-- currently active, regardless of the "of Light" name). Same cost/req_level.
-- This is also the remediation path for Testpal and anyone else already
-- missing Judgement from a Mentor unlock.
--
-- Fix part 2 (creation-gift table, requires worldserver restart):
-- playercreateinfo_spell_custom is loaded into PlayerInfo at startup, so this
-- only affects FUTURE Mentor class-unlocks until restarted.
--
-- Not touched: 21084 "Seal of Righteousness" (10321's other trigger). Testpal
-- already has 20154 "Seal of Righteousness r1" as a creation gift and both
-- DBC entries share the same name with no rank text to distinguish them —
-- granting 21084 too risks an unverified duplicate/rank conflict. Flag for a
-- follow-up if Seal of Righteousness turns out not to rank up correctly.
--
-- Safe to re-run: DELETE the old spell_id/Spell row then INSERT IGNORE the
-- new one in each table, so re-running never collides on the primary key
-- (see 12_mount_spell_fix.sql for why a plain UPDATE isn't safe here).

DELETE FROM unbound_class_catalog WHERE class_id = 2 AND spell_id = 10321;
INSERT IGNORE INTO unbound_class_catalog (class_id, spell_id, gold_cost_copper, req_level) VALUES (2, 20271, 100, 4);

DELETE FROM playercreateinfo_spell_custom WHERE racemask = 0 AND classmask = 2 AND Spell = 10321;
INSERT IGNORE INTO playercreateinfo_spell_custom (racemask, classmask, Spell, Note) VALUES (0, 2, 20271, 'Paladin - Judgement of Light');
WU_PAYLOAD_EOF_18
    cat > "$MODULE_DIR/data/sql/db-characters/01_unbound_characters.sql" <<'WU_PAYLOAD_EOF_12'
-- Unbound Wrath Edition — characters DB schema
-- Run against: acore_characters
-- Safe to re-run: CREATE TABLE IF NOT EXISTS.

-- ============================================================
-- Per-character class unlock records.
-- One row per (player, class) pair. Never deleted — additive only.
-- ============================================================
CREATE TABLE IF NOT EXISTS `unbound_character_unlocks` (
    `char_guid`        INT UNSIGNED     NOT NULL,
    `class_id`         TINYINT UNSIGNED NOT NULL,
    `unlocked_at_level` TINYINT UNSIGNED NOT NULL,
    `unlocked_ts`      TIMESTAMP        NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (`char_guid`, `class_id`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
WU_PAYLOAD_EOF_12

    cat > "$SERVER_DIR/env/dist/etc/modules/lua_scripts/unbound_mentor.lua" <<'WU_PAYLOAD_EOF_13'
--[[ ============================================================
  unbound_mentor.lua — Unbound Wrath Edition
  --------------------------------------------------------------
  Handles:
    • Mentor NPC gossip (class unlocks + individual spell purchase)
    • Multi-resource pool initialization on unlock and login
    • Weapon/armor proficiency (all skills) for every Unbound character
    • Persistent unlock state in unbound_character_unlocks (acore_characters)

  C++ side (mod-unbound/UnboundSystem.cpp) hooks
  Player::HasActivePowerType so rage/energy generation fires
  for any power type whose GetMaxPower() > 0.

  NPC entry 900001 = The Mentor (model = Ethereal Thief, displayid 19097).
  Spawn in-game via: .npc add 900001
============================================================ --]]

local MENTOR_ENTRY      = 900001
local MENTOR_STONE_ENTRY = 900100
local PAGE_SIZE          = 10   -- spells shown per gossip page

-- ── Talent points for sale ────────────────────────────────────────────────
-- Flat price, no scaling with level or with how many you already own.
local TALENT_POINT_COST    = 750000            -- 75g in copper
local TALENT_POINT_BUNDLES = { 1, 5, 10 }      -- purchase sizes offered
-- A bought point becomes a permanent bonus talent point: AddBonusTalent feeds
-- Player::CalculateTalentsPoints(), and the running total is saved in
-- characters.extraBonusTalentCount, so it survives relog, level-up and respec.
-- AzerothCore reads that column back as a uint8, so the total must stay <= 255.
local MAX_BONUS_TALENT_POINTS = 255

-- Per-player timestamp (os.time seconds) of last Mentor Stone use.
-- Used to enforce the 3-minute cooldown on the Lua side as a guard.
local STONE_LAST_USE = {}
local STONE_COOLDOWN_SEC = 180

-- AzerothCore power type constants
local POWER_MANA   = 0
local POWER_RAGE   = 1
local POWER_ENERGY = 3

local RAGE_MAX    = 1000
local ENERGY_MAX  = 100

local RAGE_NATIVE   = { [1]=true, [11]=true }
local ENERGY_NATIVE = { [4]=true, [11]=true }
local MANA_NATIVE   = { [2]=true, [3]=true, [5]=true, [7]=true, [8]=true, [9]=true, [11]=true }

local CLASS_NAMES = {
    [1]="Warrior", [2]="Paladin", [3]="Hunter",  [4]="Rogue",
    [5]="Priest",  [7]="Shaman",  [8]="Mage",    [9]="Warlock", [11]="Druid"
}

-- ── Weapon and armor skill IDs (from SharedDefines.h) ─────────────────────
-- These skills govern item equip eligibility (CanEquipItem checks GetSkillValue).
-- Granting all of them makes every Unbound character weapon/armor-agnostic.
local WEAPON_SKILLS = {
    43,   -- Swords
    44,   -- Axes
    45,   -- Bows
    46,   -- Guns
    54,   -- Maces
    55,   -- Two-Handed Swords
    118,  -- Dual Wield
    136,  -- Staves
    160,  -- Two-Handed Maces
    162,  -- Unarmed
    172,  -- Two-Handed Axes
    173,  -- Daggers
    176,  -- Thrown
    226,  -- Crossbows
    228,  -- Wands
    229,  -- Polearms
    433,  -- Shield
    473,  -- Fist Weapons
}
local ARMOR_SKILLS = {
    293,  -- Plate Mail
    413,  -- Mail
    414,  -- Leather
    415,  -- Cloth
}

-- ── Rank prerequisite map ─────────────────────────────────────────────────
-- PREREQ_MAP[classId][spellId] = prereqSpellId (0 if none)
-- Built at script-load from GetSpellInfo data; avoids any DB dependency.
local PREREQ_MAP = {}

-- GetSpellInfo returns a SpellInfo object.  Its name method is :GetName(locale)
-- which returns a plain string.  Rank info isn't on SpellInfo, so we order
-- same-named spells by catalog req_level to determine the rank chain.
local function BuildPrereqMap()
    for classId = 1, 11 do
        if CLASS_NAMES[classId] then
            PREREQ_MAP[classId] = {}
            -- Include req_level so we can sort chains without rank text
            local Q = WorldDBQuery(string.format(
                "SELECT spell_id, req_level FROM unbound_class_catalog " ..
                "WHERE class_id = %d ORDER BY req_level, spell_id", classId))
            if Q then
                local byName = {}  -- spellName → [{reqLevel, id}]
                repeat
                    local spellId   = Q:GetUInt32(0)
                    local reqLevel  = Q:GetUInt32(1)
                    local info      = GetSpellInfo(spellId)
                    if info then
                        local ok, name = pcall(function() return info:GetName(0) end)
                        if ok and name and name ~= "" then
                            if not byName[name] then byName[name] = {} end
                            table.insert(byName[name], { lv=reqLevel, id=spellId })
                        end
                    end
                until not Q:NextRow()
                for _, group in pairs(byName) do
                    if #group > 1 then
                        table.sort(group, function(a, b)
                            return a.lv < b.lv or (a.lv == b.lv and a.id < b.id)
                        end)
                        for i = 2, #group do
                            PREREQ_MAP[classId][group[i].id] = group[i-1].id
                        end
                    end
                end
            end
        end
    end
    print("[UNBOUND] Prereq map built.")
end

pcall(BuildPrereqMap)

-- ============================================================
-- Helpers
-- ============================================================

local function FormatCopper(copper)
    if copper == 0 then return "Free" end
    local g = math.floor(copper / 10000)
    local s = math.floor((copper % 10000) / 100)
    local c = copper % 100
    if g > 0 then
        return s > 0 and string.format("%dg %ds", g, s) or string.format("%dg", g)
    end
    if s > 0 then return string.format("%ds", s) end
    return string.format("%dc", c)
end

local function GetSpellDisplayName(spellId)
    local info = GetSpellInfo(spellId)
    if not info then return "Spell #" .. spellId end
    local ok, name = pcall(function() return info:GetName(0) end)
    if ok and name and name ~= "" then return name end
    return "Spell #" .. spellId
end

-- A handful of spell IDs (e.g. Parry=3127, Plate Mail=750, Mail=8737) appear
-- in unbound_class_catalog under MULTIPLE class_ids. Gossip intid only carries
-- one number, so the browse/buy path encodes both class and spell into it —
-- otherwise a bare "WHERE spell_id = ... LIMIT 1" recovery would resolve to
-- whichever class_id happens to sort first, not the class the player actually
-- browsed from, and reject the purchase with "You have not unlocked that class."
-- Max spell_id in the catalog is well under 1,000,000 and class_id <= 11, so
-- this never collides and stays inside Lua's 32-bit intid range.
local SPELL_ID_MULT = 1000000
local function EncodeClassSpell(classId, spellId)
    return classId * SPELL_ID_MULT + spellId
end
local function DecodeClassSpell(intid)
    return math.floor(intid / SPELL_ID_MULT), intid % SPELL_ID_MULT
end

local function GetUnlockedClasses(player)
    local unlocked = {}
    local Q = CharDBQuery(string.format(
        "SELECT class_id FROM unbound_character_unlocks WHERE char_guid = %d",
        player:GetGUIDLow()))
    if Q then
        repeat unlocked[Q:GetUInt32(0)] = true until not Q:NextRow()
    end
    return unlocked
end

local function GetUnlockedCount(player)
    local Q = CharDBQuery(string.format(
        "SELECT COUNT(*) FROM unbound_character_unlocks WHERE char_guid = %d",
        player:GetGUIDLow()))
    return Q and Q:GetUInt32(0) or 0
end

local function GetMilestone(index)
    local capped = math.min(5, index)
    local Q = WorldDBQuery(string.format(
        "SELECT required_level, unlock_cost_copper FROM unbound_milestones WHERE milestone_index = %d",
        capped))
    if not Q then return nil end
    return { level = Q:GetUInt32(0), cost = Q:GetUInt32(1) }
end

-- ============================================================
-- Power pools
-- ============================================================

local function ApplyUnboundPools(player)
    local native  = player:GetClass()
    local level   = player:GetLevel()
    local unlocked = GetUnlockedClasses(player)

    local needRage, needEnergy, needMana = false, false, false
    for classId in pairs(unlocked) do
        if classId == 1  then needRage   = true end
        if classId == 4  then needEnergy = true end
        if classId ~= 1 and classId ~= 4 then needMana = true end
    end
    if RAGE_NATIVE[native]   then needRage   = false end
    if ENERGY_NATIVE[native] then needEnergy = false end
    if MANA_NATIVE[native]   then needMana   = false end

    if needRage and player:GetMaxPower(POWER_RAGE) == 0 then
        player:SetMaxPower(POWER_RAGE, RAGE_MAX)
    end
    if needEnergy and player:GetMaxPower(POWER_ENERGY) == 0 then
        player:SetMaxPower(POWER_ENERGY, ENERGY_MAX)
        player:SetPower(ENERGY_MAX, POWER_ENERGY)
    end
    if needMana then
        -- 80% of mage base mana at the player's level.
        -- Warriors (and other non-mana classes) have basemana=0 in player_class_stats,
        -- so we look up the mage value explicitly rather than trusting GetMaxPower.
        -- The == 0 guard is intentionally removed: server-side UpdateMaxPower for
        -- non-mana classes always recalculates to 0, which would silently undo the set.
        local mQ = WorldDBQuery(string.format(
            "SELECT basemana FROM player_class_stats WHERE class = 8 AND level = %d", level))
        local pool = mQ and math.floor(mQ:GetUInt32(0) * 0.8) or (level * 30)
        if pool < 100 then pool = 100 end
        player:SetMaxPower(POWER_MANA, pool)
        player:SetPower(pool, POWER_MANA)

        -- AzerothCore computes percentage-based spell costs (ManaCostPercentage —
        -- e.g. Arcane Intellect, Frost Armor) as a % of GetCreateMana(), which reads
        -- UNIT_FIELD_BASE_MANA — the player's NATIVE class's base mana (0 for
        -- Rogue/Warrior/Hunter/etc). That makes those spells cost 0 mana for
        -- cross-class casters, so mana never visibly depletes. SetCreateMana() isn't
        -- Lua-exposed, so set the raw field directly: UNIT_FIELD_BASE_MANA =
        -- OBJECT_END(6) + 0x72 = 120. SetCreateMana() is only called from the
        -- level-change paths GiveLevel/InitStatsForLevel, both of which complete
        -- before ApplyUnboundPools runs (1s after login, 200ms after level-up), so
        -- this sticks until the next level change re-applies it.
        player:SetUInt32Value(120, pool)
    end
end

-- ============================================================
-- Skill grants
-- ============================================================

-- Class-specific skill tabs (Arms/Fury/Protection for warrior, etc.)
-- so the WotLK spellbook renders the correct ability tabs.
local function ApplyUnboundSkills(player, classId)
    local mask = math.floor(2 ^ (classId - 1))
    local Q = WorldDBQuery(string.format(
        "SELECT skill FROM playercreateinfo_skills WHERE classMask = %d", mask))
    if not Q then return end
    local maxSkill = math.max(1, player:GetLevel() * 5)
    repeat
        local skillId = Q:GetUInt32(0)
        if player:GetSkillValue(skillId) == 0 then
            player:SetSkill(skillId, 1, 1, maxSkill)
        end
    until not Q:NextRow()
end

local function ApplyAllUnboundSkills(player)
    for classId in pairs(GetUnlockedClasses(player)) do
        ApplyUnboundSkills(player, classId)
    end
end

-- Universal weapon + armor proficiency for all Unbound characters.
-- SetSkill grants the skill entry so the server-side equip check passes
-- (CanEquipItem checks GetSkillValue(itemSkill) > 0). The client-side
-- "weapon shows as equippable" half is handled separately by the C++
-- OnPlayerLogin hook in UnboundSystem.cpp (AddWeaponProficiency + SendProficiency),
-- which bypasses learnSkillRewardedSpells()'s ClassMask filter directly —
-- no custom proficiency spells needed.
--
-- Pure SetSkill approach — no CharDBExecute.
-- With the skillraceclassinfo_dbc fix (06_universal_skill_access.sql), _LoadSkills
-- no longer strips cross-class skills (ClassMask=0 entries pass validation).
-- SetSkill marks skill SKILL_NEW → _SaveSkills INSERTs on logout → persists.
-- Using CharDBExecute alongside SetSkill caused duplicate-key aborts that
-- reverted the entire character save transaction.
local function ApplyUnboundWeaponArmorSkills(player)
    local maxSkill = math.max(1, player:GetLevel() * 5)
    for _, skillId in ipairs(WEAPON_SKILLS) do
        if player:GetSkillValue(skillId) == 0 then
            player:SetSkill(skillId, 1, 1, maxSkill)
        end
    end
    for _, skillId in ipairs(ARMOR_SKILLS) do
        if player:GetSkillValue(skillId) == 0 then
            player:SetSkill(skillId, 1, 1, maxSkill)
        end
    end
    -- Dual Wield off-hand spell
    if not player:HasSpell(674) then
        player:LearnSpell(674)
    end
end

-- ============================================================
-- Creation gift spells
-- ============================================================
-- Sourced from playercreateinfo_spell_custom (same table AzerothCore
-- reads for LearnCustomSpells at character creation).

local function GrantClassGiftSpells(player, classId)
    local mask = math.floor(2 ^ (classId - 1))
    local Q = WorldDBQuery(string.format(
        "SELECT Spell FROM playercreateinfo_spell_custom WHERE classmask = %d AND racemask = 0",
        mask))
    if not Q then return end
    repeat
        local spellId = Q:GetUInt32(0)
        if not player:HasSpell(spellId) then player:LearnSpell(spellId) end
    until not Q:NextRow()
end

local function GrantAllClassGiftSpells(player)
    for classId in pairs(GetUnlockedClasses(player)) do
        GrantClassGiftSpells(player, classId)
    end
end

-- ============================================================
-- Creation gift items
-- ============================================================
-- Physical reagent items required to cast certain abilities.
-- Shaman needs the four totem items in inventory to cast totem spells.
-- item 5175=Earth Totem  5176=Fire Totem  5177=Water Totem  5178=Air Totem
local CLASS_GIFT_ITEMS = {
    [7] = { 5175, 5176, 5177, 5178 },
}

local function GrantClassGiftItems(player, classId)
    local items = CLASS_GIFT_ITEMS[classId]
    if not items then return end
    for _, itemId in ipairs(items) do
        if player:GetItemCount(itemId) == 0 then
            player:AddItem(itemId, 1)
        end
    end
end

local function GrantAllClassGiftItems(player)
    for classId in pairs(GetUnlockedClasses(player)) do
        GrantClassGiftItems(player, classId)
    end
end

-- ============================================================
-- Individual spell purchase helpers
-- ============================================================

-- Returns an ordered list of {id, cost} for spells the player can buy
-- right now: level met, prereq met (or none), not already known.
local function GetBuyableSpells(player, classId)
    local level = player:GetLevel()
    local Q = WorldDBQuery(string.format(
        "SELECT spell_id, gold_cost_copper FROM unbound_class_catalog " ..
        "WHERE class_id = %d AND req_level <= %d ORDER BY req_level, spell_id",
        classId, level))
    if not Q then return {} end

    local classPrereqs = PREREQ_MAP[classId] or {}
    local list = {}
    repeat
        local spellId = Q:GetUInt32(0)
        local cost    = Q:GetUInt32(1)
        if not player:HasSpell(spellId) then
            local prereq = classPrereqs[spellId] or 0
            if prereq == 0 or player:HasSpell(prereq) then
                table.insert(list, { id=spellId, cost=cost })
            end
        end
    until not Q:NextRow()
    return list
end

-- Build (or rebuild) the browse gossip page for a class.
local function ShowBrowsePage(player, creature, classId, page)
    player:GossipClearMenu()

    local list = GetBuyableSpells(player, classId)
    local total = #list
    local startIdx = page * PAGE_SIZE + 1
    local endIdx   = math.min(startIdx + PAGE_SIZE - 1, total)

    if total == 0 then
        local nextQ = WorldDBQuery(string.format(
            "SELECT MIN(req_level) FROM unbound_class_catalog " ..
            "WHERE class_id = %d AND req_level > %d", classId, player:GetLevel()))
        local nextLvl = nextQ and nextQ:GetUInt32(0) or 0
        if nextLvl > 0 then
            player:GossipMenuAddItem(0, string.format(
                "No %s abilities available yet — come back at level %d.",
                CLASS_NAMES[classId], nextLvl), 0, 99, false)
        else
            player:GossipMenuAddItem(0,
                "You already know all available " .. CLASS_NAMES[classId] .. " abilities.",
                0, 99, false)
        end
        player:GossipMenuAddItem(0, "← Back", 99, 0, false)
        player:GossipSendMenu(100, creature)
        return
    end

    -- Page header (non-clickable)
    player:GossipMenuAddItem(0, string.format(
        "── %s abilities (page %d/%d) ──",
        CLASS_NAMES[classId], page+1, math.ceil(total/PAGE_SIZE)),
        0, 99, false)

    -- Buy-all shortcut: sender=27, intid=classId
    if page == 0 then
        local totalCost = 0
        for _, sp in ipairs(list) do totalCost = totalCost + sp.cost end
        player:GossipMenuAddItem(0, string.format(
            "|cffffd700* Buy ALL available abilities (%s) *|r", FormatCopper(totalCost)),
            27, classId, false)
    end

    -- Individual spell items: sender=25, intid=EncodeClassSpell(classId, spellId)
    for i = startIdx, endIdx do
        local sp = list[i]
        local label = string.format("%s  [%s]",
            GetSpellDisplayName(sp.id), FormatCopper(sp.cost))
        player:GossipMenuAddItem(0, label, 25, EncodeClassSpell(classId, sp.id), false)
    end

    -- Pagination: sender=24, intid encodes (classId*1000 + page)
    if page > 0 then
        player:GossipMenuAddItem(0, "← Prev page", 24, classId*1000+(page-1), false)
    end
    if endIdx < total then
        player:GossipMenuAddItem(0, "Next page →", 24, classId*1000+(page+1), false)
    end

    player:GossipMenuAddItem(0, "← Back to menu", 99, 0, false)
    player:GossipSendMenu(100, creature)
end

-- ============================================================
-- Talent point purchase
-- ============================================================

local function TalentPointMenuLabel(player)
    return string.format("Buy talent points  [%s each]  (%d unspent)",
        FormatCopper(TALENT_POINT_COST), player:GetFreeTalentPoints())
end

local function ShowTalentPointMenu(player, creature)
    player:GossipClearMenu()

    local purchased = player:GetBonusTalentCount()
    player:GossipMenuAddItem(0, string.format(
        "── %d unspent talent point(s), %d purchased ──",
        player:GetFreeTalentPoints(), purchased), 0, 99, false)

    local offered = 0
    for _, amount in ipairs(TALENT_POINT_BUNDLES) do
        if purchased + amount <= MAX_BONUS_TALENT_POINTS then
            player:GossipMenuAddItem(0, string.format(
                "Buy %d talent point%s  [%s]",
                amount, amount == 1 and "" or "s",
                FormatCopper(TALENT_POINT_COST * amount)), 31, amount, false)
            offered = offered + 1
        end
    end
    if offered == 0 then
        player:GossipMenuAddItem(0,
            "You have bought every talent point I can grant.", 0, 99, false)
    end

    player:GossipMenuAddItem(0, "← Back", 32, 0, false)
    player:GossipSendMenu(100, creature)
end

-- ============================================================
-- Gossip: Hello
-- ============================================================

local function OnGossipHello(event, player, creature)
    player:GossipClearMenu()

    local unlockedCnt = GetUnlockedCount(player)
    local unlocked    = GetUnlockedClasses(player)
    local native      = player:GetClass()

    if unlockedCnt == 0 then
        local ms = GetMilestone(1)
        if ms and player:GetLevel() >= ms.level then
            player:GossipMenuAddItem(0, "I wish to walk the Unbound path.", 1, 0, false)
        else
            local lvl = ms and ms.level or 5
            player:GossipMenuAddItem(0, string.format(
                "(Reach level %d to begin the Unbound path.)", lvl), 0, 99, false)
        end
        player:GossipMenuAddItem(0, TalentPointMenuLabel(player), 30, 0, false)
        player:GossipMenuAddItem(0, "Farewell.", 99, 0, false)
        player:GossipSendMenu(100, creature)
        return
    end

    local nextMs = GetMilestone(unlockedCnt + 1)
    if nextMs then
        if player:GetLevel() >= nextMs.level then
            player:GossipMenuAddItem(0, string.format(
                "Unlock another class  [%s | Requires level %d]",
                FormatCopper(nextMs.cost), nextMs.level), 1, 0, false)
        else
            player:GossipMenuAddItem(0, string.format(
                "(Next unlock: level %d, %s)", nextMs.level, FormatCopper(nextMs.cost)),
                0, 99, false)
        end
    end

    for classId in pairs(unlocked) do
        player:GossipMenuAddItem(0,
            "Browse " .. CLASS_NAMES[classId] .. " abilities", 2, classId, false)
    end

    player:GossipMenuAddItem(0, TalentPointMenuLabel(player), 30, 0, false)
    player:GossipMenuAddItem(0, "Farewell.", 99, 0, false)
    player:GossipSendMenu(100, creature)
end

-- ============================================================
-- Gossip: Select
-- sender=1   → show class picker for next unlock
-- sender=10  → execute class unlock (classId = intid)
-- sender=2   → browse spells for class (classId = intid, page 0)
-- sender=24  → paginate (intid = classId*1000 + page)
-- sender=25  → buy individual spell directly, then refresh Browse (intid = encoded classId+spellId)
-- sender=27  → buy every currently-available spell for the class (intid = classId)
-- sender=30  → open the talent point shop
-- sender=31  → buy talent points (intid = how many)
-- sender=32  → back to the main menu
-- sender=99  → close
-- ============================================================

local function OnGossipSelect(event, player, creature, sender, intid, code, menuId)
    player:GossipClearMenu()

    if sender == 99 or intid == 99 then
        player:GossipComplete()
        return
    end

    local native      = player:GetClass()
    local unlockedCnt = GetUnlockedCount(player)
    local unlocked    = GetUnlockedClasses(player)

    -- ---- sender=1: show class picker for unlock ----
    if sender == 1 then
        local nextMs = GetMilestone(unlockedCnt + 1)
        if not nextMs then
            player:SendBroadcastMessage("Error: milestone data missing.")
            player:GossipComplete()
            return
        end
        if player:GetLevel() < nextMs.level then
            player:SendBroadcastMessage(string.format(
                "You must reach level %d before unlocking another class.", nextMs.level))
            player:GossipComplete()
            return
        end
        local costStr = FormatCopper(nextMs.cost)
        for classId, name in pairs(CLASS_NAMES) do
            if classId ~= native and not unlocked[classId] then
                player:GossipMenuAddItem(0,
                    string.format("%s  [%s]", name, costStr), 10, classId, false)
            end
        end
        player:GossipMenuAddItem(0, "Never mind.", 99, 0, false)
        player:GossipSendMenu(100, creature)
        return
    end

    -- ---- sender=10: execute class unlock ----
    if sender == 10 then
        local classId = intid
        local nextMs  = GetMilestone(unlockedCnt + 1)
        if not nextMs then
            player:SendBroadcastMessage("Error: milestone data missing.")
            player:GossipComplete()
            return
        end
        if player:GetLevel() < nextMs.level then
            player:SendBroadcastMessage(string.format(
                "You must be level %d to unlock another class.", nextMs.level))
            player:GossipComplete()
            return
        end
        if unlocked[classId] then
            player:SendBroadcastMessage("You have already unlocked that class.")
            player:GossipComplete()
            return
        end
        if not CLASS_NAMES[classId] then
            player:SendBroadcastMessage("Unknown class.")
            player:GossipComplete()
            return
        end
        if nextMs.cost > 0 and player:GetCoinage() < nextMs.cost then
            player:SendBroadcastMessage(string.format(
                "You need %s to unlock this class.", FormatCopper(nextMs.cost)))
            player:GossipComplete()
            return
        end

        if nextMs.cost > 0 then player:ModifyMoney(-nextMs.cost) end

        CharDBExecute(string.format(
            "INSERT IGNORE INTO unbound_character_unlocks (char_guid, class_id, unlocked_at_level) " ..
            "VALUES (%d, %d, %d)",
            player:GetGUIDLow(), classId, player:GetLevel()))

        ApplyUnboundPools(player)
        ApplyUnboundSkills(player, classId)
        ApplyUnboundWeaponArmorSkills(player)
        GrantClassGiftSpells(player, classId)
        GrantClassGiftItems(player, classId)

        player:SendBroadcastMessage(string.format(
            "|cff00ff00The path of the %s is now open to you!|r " ..
            "Relog once to see the ability tabs in your spellbook.",
            CLASS_NAMES[classId]))
        player:GossipComplete()
        return
    end

    -- ---- sender=2: open browse for class (page 0) ----
    if sender == 2 then
        local classId = intid
        if not unlocked[classId] then
            player:GossipComplete()
            return
        end
        ShowBrowsePage(player, creature, classId, 0)
        return
    end

    -- ---- sender=24: paginate ----
    if sender == 24 then
        local classId = math.floor(intid / 1000)
        local page    = intid % 1000
        if not unlocked[classId] then
            player:GossipComplete()
            return
        end
        ShowBrowsePage(player, creature, classId, page)
        return
    end

    -- ---- sender=25: buy individual spell, then refresh the Browse page ----
    if sender == 25 then
        local classId, spellId = DecodeClassSpell(intid)
        local Q = WorldDBQuery(string.format(
            "SELECT gold_cost_copper FROM unbound_class_catalog WHERE class_id = %d AND spell_id = %d",
            classId, spellId))
        if not Q then
            player:SendBroadcastMessage("Spell not found in catalog.")
            player:GossipComplete()
            return
        end
        local cost = Q:GetUInt32(0)

        if not unlocked[classId] then
            player:SendBroadcastMessage("You have not unlocked that class.")
            player:GossipComplete()
            return
        end
        if player:HasSpell(spellId) then
            player:SendBroadcastMessage("You already know that ability.")
            ShowBrowsePage(player, creature, classId, 0)
            return
        end
        local prereq = PREREQ_MAP[classId] and PREREQ_MAP[classId][spellId] or 0
        if prereq > 0 and not player:HasSpell(prereq) then
            player:SendBroadcastMessage(string.format(
                "You must learn %s first.", GetSpellDisplayName(prereq)))
            ShowBrowsePage(player, creature, classId, 0)
            return
        end
        if player:GetCoinage() < cost then
            player:SendBroadcastMessage(string.format(
                "You need %s to buy that ability.", FormatCopper(cost)))
            ShowBrowsePage(player, creature, classId, 0)
            return
        end

        player:ModifyMoney(-cost)
        player:LearnSpell(spellId)
        player:SendBroadcastMessage(string.format(
            "|cff00ff00Learned %s!|r", GetSpellDisplayName(spellId)))
        ShowBrowsePage(player, creature, classId, 0)
        return
    end

    -- ---- sender=27: buy every available spell for the class ----
    if sender == 27 then
        local classId = intid
        if not unlocked[classId] then
            player:GossipComplete()
            return
        end

        if #GetBuyableSpells(player, classId) == 0 then
            player:SendBroadcastMessage("You already know everything currently available.")
            ShowBrowsePage(player, creature, classId, 0)
            return
        end

        -- Re-query each pass: buying a spell can satisfy another's prereq,
        -- which only shows up once GetBuyableSpells re-checks HasSpell().
        local learned = 0
        while true do
            local list = GetBuyableSpells(player, classId)
            if #list == 0 then break end
            local boughtAny = false
            for _, sp in ipairs(list) do
                if player:GetCoinage() >= sp.cost then
                    player:ModifyMoney(-sp.cost)
                    player:LearnSpell(sp.id)
                    learned = learned + 1
                    boughtAny = true
                end
            end
            if not boughtAny then break end
        end

        if learned == 0 then
            player:SendBroadcastMessage("You can't afford any available abilities right now.")
        else
            player:SendBroadcastMessage(string.format(
                "|cff00ff00Learned %d %s abilities!|r", learned, CLASS_NAMES[classId]))
        end
        ShowBrowsePage(player, creature, classId, 0)
        return
    end

    -- ---- sender=30: open the talent point shop ----
    if sender == 30 then
        ShowTalentPointMenu(player, creature)
        return
    end

    -- ---- sender=31: buy talent points ----
    if sender == 31 then
        local amount = 0
        for _, bundle in ipairs(TALENT_POINT_BUNDLES) do
            if intid == bundle then amount = bundle end
        end
        if amount == 0 then
            player:GossipComplete()
            return
        end

        if player:GetBonusTalentCount() + amount > MAX_BONUS_TALENT_POINTS then
            player:SendBroadcastMessage(
                "You have bought every talent point I can grant.")
            ShowTalentPointMenu(player, creature)
            return
        end

        local cost = TALENT_POINT_COST * amount
        if player:GetCoinage() < cost then
            player:SendBroadcastMessage(string.format(
                "You need %s for %d talent point%s.",
                FormatCopper(cost), amount, amount == 1 and "" or "s"))
            ShowTalentPointMenu(player, creature)
            return
        end

        player:ModifyMoney(-cost)
        -- AddBonusTalent makes the point permanent (saved in
        -- characters.extraBonusTalentCount, re-added by CalculateTalentsPoints
        -- on every login/level-up/respec); SetFreeTalentPoints makes it
        -- spendable right now, without waiting for that recalculation.
        player:AddBonusTalent(amount)
        player:SetFreeTalentPoints(player:GetFreeTalentPoints() + amount)
        player:SendBroadcastMessage(string.format(
            "|cff00ff00Gained %d talent point%s for %s.|r",
            amount, amount == 1 and "" or "s", FormatCopper(cost)))
        ShowTalentPointMenu(player, creature)
        return
    end

    -- ---- sender=32: back to the main menu ----
    if sender == 32 then
        OnGossipHello(event, player, creature)
        return
    end

    player:GossipComplete()
end

-- ============================================================
-- Mentor Stone item use handler
-- ============================================================
-- Fires when a player right-clicks the Unbounding Mentor Stone (entry 900100).
-- Summons the Mentor NPC 3 yards in front of the player for 3 minutes.
--
-- Uses ITEM_EVENT_ON_USE (event=2), which fires before the item spell is cast.
-- Returning false lets AzerothCore proceed with CastItemUseSpell, which applies
-- the 3-minute cooldown from item_template.spellcooldown_1.
-- Returning true (or nil) tells Eluna "handled it" — skips the spell cast.
-- The Lua-side STONE_LAST_USE table guards against the server-restart case
-- where spellcooldown_1 state is lost but the Lua table has been reset too.
RegisterItemEvent(MENTOR_STONE_ENTRY, 2, function(event, player, item, target)
    local guid = player:GetGUIDLow()
    local now  = os.time()
    local last = STONE_LAST_USE[guid] or 0

    if (now - last) < STONE_COOLDOWN_SEC then
        local remaining = STONE_COOLDOWN_SEC - (now - last)
        player:SendBroadcastMessage(string.format(
            "|cffff4444Unbounding Mentor Stone is on cooldown (%ds remaining).|r", remaining))
        return true  -- Eluna "handled" this use; skip the spell cast
    end

    STONE_LAST_USE[guid] = now

    -- Summon 3 yards ahead of the player, facing back toward the player.
    local angle = player:GetO()
    local x = player:GetX() + math.cos(angle) * 3
    local y = player:GetY() + math.sin(angle) * 3
    local z = player:GetZ()
    local face = angle + math.pi  -- face the player

    -- TEMPSUMMON_TIMED_DESPAWN (3): despawn after 180 000 ms regardless of state.
    local mentor = player:SpawnCreature(MENTOR_ENTRY, x, y, z, face, 3, 180000)
    if not mentor then
        player:SendBroadcastMessage(
            "|cffff4444Could not summon the Mentor here. Try again in the open world.|r")
        STONE_LAST_USE[guid] = 0  -- refund cooldown on failure
        return true
    end

    player:SendBroadcastMessage(
        "|cff00ff00Your Unbounding Mentor has arrived. (3 min)|r")
    -- Return true: cancel the item's spell cast. spellid_1 only exists so the
    -- 3.3.5a client recognizes this as a usable item and sends CMSG_USE_ITEM —
    -- a custom server-only spell ID (900200, absent from the client's Spell.dbc)
    -- left the client unable to resolve the item, so right-click did nothing.
    -- The Lua-side STONE_LAST_USE cooldown (180s, matching spellcooldown_1)
    -- fully replaces the need for the real spell cast to apply a cooldown.
    return true
end)

-- ============================================================
-- Event registration
-- ============================================================

RegisterCreatureGossipEvent(MENTOR_ENTRY, 1, OnGossipHello)
RegisterCreatureGossipEvent(MENTOR_ENTRY, 2, OnGossipSelect)

-- On level-up: re-apply pools so the mana pool grows with the player's level.
-- The C++ OnAfterUpdateMaxPower hook (UnboundSystem.cpp) preserves the pool across
-- stat recalculations, but it locks in the OLD value — so we must re-calculate
-- after each level change.  Short 200 ms delay lets GiveLevel() finish its own
-- UpdateAllStats() pass before we write the new value.
RegisterPlayerEvent(13, function(event, player, oldLevel)
    local Q = CharDBQuery(string.format(
        "SELECT 1 FROM unbound_character_unlocks WHERE char_guid = %d LIMIT 1",
        player:GetGUIDLow()))
    if Q then
        player:RegisterEvent(function()
            pcall(function() ApplyUnboundPools(player) end)
        end, 200, 1)
    end
end)

-- On login: restore pools, skills, weapon/armor proficiency, gift spells,
-- and ensure the player has their Mentor Stone.
-- Delayed 1s: calling SetMaxPower during PLAYER_EVENT_ON_LOGIN crashes
-- AzerothCore before the character is fully in-world.
RegisterPlayerEvent(3, function(event, player)
    -- Give the Mentor Stone to any character that doesn't have it.
    -- Runs unconditionally so existing characters and anyone who deleted
    -- theirs get it back automatically.
    if player:GetItemCount(MENTOR_STONE_ENTRY) == 0 then
        player:AddItem(MENTOR_STONE_ENTRY, 1)
    end

    local Q = CharDBQuery(string.format(
        "SELECT 1 FROM unbound_character_unlocks WHERE char_guid = %d LIMIT 1",
        player:GetGUIDLow()))
    if Q then
        -- The `player` userdata captured directly in this closure goes stale by the
        -- time the 1s timer fires (Eluna invalidates it during the login->world
        -- transition: "pointer to nonexisting (invalidated) object"). Capture the
        -- GUID instead and re-fetch a live Player reference inside the callback.
        local guid = player:GetGUID()
        local guidLow = player:GetGUIDLow()
        player:RegisterEvent(function()
            local livePlayer = GetPlayerByGUID(guid)
            if not livePlayer or not livePlayer:IsInWorld() then
                return
            end
            local ok, err = pcall(function()
                ApplyUnboundPools(livePlayer)
                ApplyAllUnboundSkills(livePlayer)
                ApplyUnboundWeaponArmorSkills(livePlayer)
                GrantAllClassGiftSpells(livePlayer)
                GrantAllClassGiftItems(livePlayer)
            end)
            if not ok then
                print(string.format("[UNBOUND] OnLogin post-login setup ERROR for guidLow=%d: %s",
                    guidLow, tostring(err)))
            end
        end, 1000, 1)
    end
end)
WU_PAYLOAD_EOF_13

    cat > "$MODULE_DIR/unbound-core-access.patch" <<'WU_PAYLOAD_EOF_19'
diff --git a/src/server/game/Conditions/ConditionMgr.cpp b/src/server/game/Conditions/ConditionMgr.cpp
index 90319545d..9a5ba20ed 100644
--- a/src/server/game/Conditions/ConditionMgr.cpp
+++ b/src/server/game/Conditions/ConditionMgr.cpp
@@ -132,7 +132,15 @@ bool Condition::Meets(ConditionSourceInfo& sourceInfo)
     case CONDITION_CLASS:
     {
         if (Unit* unit = object->ToUnit())
+        {
             condMeets = unit->getClassMask() & ConditionValue1;
+
+            // Unbound Wrath Edition — also meets the condition if any class
+            // this character has unlocked via the Mentor matches.
+            if (!condMeets)
+                if (Player* player = unit->ToPlayer())
+                    condMeets = player->GetUnboundClassMask() & ConditionValue1;
+        }
         break;
     }
     case CONDITION_RACE:
diff --git a/src/server/game/Entities/Creature/Trainer.cpp b/src/server/game/Entities/Creature/Trainer.cpp
index 58b61aabb..819a6e5a6 100644
--- a/src/server/game/Entities/Creature/Trainer.cpp
+++ b/src/server/game/Entities/Creature/Trainer.cpp
@@ -216,7 +216,12 @@ namespace Trainer
             case Type::Class:
             case Type::Pet:
                 // check class for class trainers
-                return player->getClass() == GetTrainerRequirement();
+                if (player->getClass() == GetTrainerRequirement())
+                    return true;
+
+                // Unbound Wrath Edition — also valid for any class this
+                // character has unlocked via the Mentor.
+                return (player->GetUnboundClassMask() & (1u << (GetTrainerRequirement() - 1))) != 0;
             case Type::Mount:
                 // check race for mount trainers
                 return player->getRace() == GetTrainerRequirement();
diff --git a/src/server/game/Entities/Player/Player.cpp b/src/server/game/Entities/Player/Player.cpp
index ee531531a..335b9b2cd 100644
--- a/src/server/game/Entities/Player/Player.cpp
+++ b/src/server/game/Entities/Player/Player.cpp
@@ -12367,7 +12367,9 @@ float Player::GetReputationPriceDiscount(FactionTemplateEntry const* factionTemp
 bool Player::IsSpellFitByClassAndRace(uint32 spell_id) const
 {
     uint32 racemask  = getRaceMask();
-    uint32 classmask = getClassMask();
+    // Unbound Wrath Edition — also fit spells belonging to any class this
+    // character has unlocked via the Mentor (trainer spell visibility).
+    uint32 classmask = getClassMask() | GetUnboundClassMask();
 
     SkillLineAbilityMapBounds bounds = sSpellMgr->GetSkillLineAbilityMapBounds(spell_id);
     if (bounds.first == bounds.second)
diff --git a/src/server/game/Entities/Player/Player.h b/src/server/game/Entities/Player/Player.h
index 4f38d8012..190374724 100644
--- a/src/server/game/Entities/Player/Player.h
+++ b/src/server/game/Entities/Player/Player.h
@@ -2134,6 +2134,12 @@ public:
     void SetFactionForRace(uint8 race);
     void setTeamId(TeamId teamid) { m_team = teamid; };
 
+    // Unbound Wrath Edition — bitmask of classes this character has access to
+    // via the Mentor (native class | every class unlocked in unbound_character_unlocks).
+    // 0 means this character is not Unbound. Populated on login by UnboundSystem.cpp.
+    [[nodiscard]] uint32 GetUnboundClassMask() const { return m_unboundClassMask; }
+    void SetUnboundClassMask(uint32 mask) { m_unboundClassMask = mask; }
+
     void InitDisplayIds();
 
     bool IsAtGroupRewardDistance(WorldObject const* pRewardSource) const;
@@ -2829,6 +2835,7 @@ protected:
     ObjectGuid m_lootGuid;
 
     TeamId m_team;
+    uint32 m_unboundClassMask = 0; // Unbound Wrath Edition — see GetUnboundClassMask()
     uint32 m_nextSave; // pussywizard
     uint16 m_additionalSaveTimer; // pussywizard
     uint8 m_additionalSaveMask; // pussywizard
diff --git a/src/server/game/Entities/Player/PlayerQuest.cpp b/src/server/game/Entities/Player/PlayerQuest.cpp
index 94ac419b4..b92ab0925 100644
--- a/src/server/game/Entities/Player/PlayerQuest.cpp
+++ b/src/server/game/Entities/Player/PlayerQuest.cpp
@@ -1091,7 +1091,9 @@ bool Player::SatisfyQuestClass(Quest const* qInfo, bool msg) const
     if (reqClass == 0)
         return true;
 
-    if ((reqClass & getClassMask()) == 0)
+    // Unbound Wrath Edition — also satisfy class quests for any class this
+    // character has unlocked via the Mentor.
+    if ((reqClass & (getClassMask() | GetUnboundClassMask())) == 0)
     {
         if (msg)
             SendCanTakeQuestResponse(INVALIDREASON_DONT_HAVE_REQ);
diff --git a/src/server/game/Entities/Player/PlayerStorage.cpp b/src/server/game/Entities/Player/PlayerStorage.cpp
index eb7f10aab..1e37b5b9b 100644
--- a/src/server/game/Entities/Player/PlayerStorage.cpp
+++ b/src/server/game/Entities/Player/PlayerStorage.cpp
@@ -2389,7 +2389,9 @@ InventoryResult Player::CanUseItem(ItemTemplate const* proto) const
         return EQUIP_ERR_YOU_CAN_NEVER_USE_THAT_ITEM;
     }
 
-    if ((proto->AllowableClass & getClassMask()) == 0 || (proto->AllowableRace & getRaceMask()) == 0)
+    // Unbound Wrath Edition — characters who've unlocked extra classes via the
+    // Mentor can ignore an item's AllowableClass restriction; AllowableRace is untouched.
+    if ((GetUnboundClassMask() == 0 && (proto->AllowableClass & getClassMask()) == 0) || (proto->AllowableRace & getRaceMask()) == 0)
     {
         return EQUIP_ERR_YOU_CAN_NEVER_USE_THAT_ITEM;
     }
WU_PAYLOAD_EOF_19

    print_success "Module files staged: mod-unbound/ (C++ + SQL) and env/dist/etc/modules/lua_scripts/unbound_mentor.lua"
    echo ""
}

# ============================================================
#  stage_mod_ale()
#
#  mod-unbound's entire player-facing system (Mentor, spell catalog,
#  Mentor Stone) is driven by env/dist/etc/modules/lua_scripts/unbound_mentor.lua,
#  which requires Eluna/ALE — AzerothCore's Lua scripting engine — to be
#  compiled into the worldserver. install-wow-wotlk.sh does NOT include
#  mod-ale by default. A server built purely from install-wow-wotlk.sh has
#  no Eluna engine at all — ALE.Enabled = 1 and unbound_mentor.lua just sit
#  there inert, the Mentor Stone casts its raw bound spell (Food) and does
#  nothing else, and "[UNBOUND] Prereq map built." never appears no matter
#  how long you wait.
#
#  Source: official azerothcore/mod-ale, pinned to the commit confirmed
#  working alongside mod-unbound on the dev server — not floating
#  "master", to avoid introducing a second moving part while debugging.
#
#  Idempotent: if modules/mod-ale already has a CMakeLists.txt (pre-existing
#  on this server, or staged by a prior run), this is a no-op.
# ============================================================
MOD_ALE_COMMIT="1cb86c9600260c3731c96dc3c98d25b4fc3f2153"

# ============================================================
#  stage_talent_bridge()   (added v1.3.0)
#
#  Cross-class TALENT system. Two Lua files into the shared ALE
#  lua_scripts dir:
#    unbound_addon_sync.lua  — MCUB addon-message bridge: class-unlock
#      sync + validated cross-class talent learn/respec. Enforces the
#      talent allowlist, tier gating, prereqs, rank order, and a shared
#      talent-point pool (Get/SetFreeTalentPoints). Self-creates the
#      unbound_character_talents tracking table.
#    unbound_talent_data.lua — per-class talent metadata the bridge
#      validates against (tier/col/maxRank/ranks/prereq).
#
#  Additive + idempotent: overwrites only these two files; the tracking
#  table is CREATE TABLE IF NOT EXISTS. No SQL migration, no schema risk.
#  Players need the CLIENT addons (WrathUnbound-Addons.zip) to use it.
# ============================================================
stage_talent_bridge() {
    print_step "Staging the cross-class talent bridge (Lua)..."

    local LUA_DIR="$SERVER_DIR/env/dist/etc/modules/lua_scripts"
    mkdir -p "$LUA_DIR"

    cat > "$LUA_DIR/unbound_addon_sync.lua" <<'WU_TALENT_BRIDGE_EOF'
-- Unbound <-> Multiclass Talents bridge (mod-ale / Eluna).
--   1. Class-unlock sync ("SYNC")  -> replies "CLASSES:<ids>".
--   2. Cross-class talent learn ("LEARN:<classId>:<spellId>") -> validated grant.
--
-- Cross-class talents are made to behave like REAL talents, enforced here:
--   * spell must be a real talent rank of that class (allowlist)
--   * you must own the class (unbound_character_unlocks) or it is your native
--   * rank order: you learn rank N only after rank N-1
--   * tier gating: (tier-1)*5 points already spent in THAT class tree
--   * prereq talent (if any) must be maxed
--   * budget: drawn from your REAL talent pool (GetFreeTalentPoints), shared
--     across your native tree and every cross-class tree
-- Spent cross-class ranks are tracked in unbound_character_talents so the tier
-- gate and rank order survive relog. The `.learn` GM command is never used.
--
-- Metadata comes from unbound_talent_data.lua (global UnboundTalentData).

local PREFIX = "MCUB"
local CHAT_MSG_WHISPER = 7
local ADDON_EVENT_ON_MESSAGE = 30
local PLAYER_EVENT_ON_LOGIN = 3
local PLAYER_EVENT_ON_LEVEL_CHANGE = 13
local POINTS_PER_TIER = 5

-- ---------------------------------------------------------------------------
-- Build lookup indices from UnboundTalentData (once, at load).
--   talentMeta[classId][talentId]      = { t,c,mr,r,p }
--   spellIndex[classId][spellId]       = { tid = talentId, ri = rankIndex }
--   tierColIndex[classId][tier][col]   = talentId       (for prereq lookup)
-- ---------------------------------------------------------------------------
local talentMeta, spellIndex, tierColIndex = {}, {}, {}
local indicesBuilt = false
local missingDataWarned = false

-- Built lazily on first use: ALE loads the lua_scripts dir alphabetically, so
-- unbound_talent_data.lua loads AFTER this file. By the time any addon message
-- arrives every script is loaded, so UnboundTalentData is guaranteed present.
local function EnsureIndices()
    if indicesBuilt then return true end
    if not UnboundTalentData then
        if not missingDataWarned then
            print("[UNBOUND] ERROR: unbound_talent_data.lua not loaded — " ..
                "cross-class talent learn will DENY. Check the data file exists " ..
                "in lua_scripts and `.reload ale`.")
            missingDataWarned = true
        end
        return false
    end
    for classId, talents in pairs(UnboundTalentData) do
        talentMeta[classId] = talents
        spellIndex[classId] = {}
        tierColIndex[classId] = {}
        for talentId, meta in pairs(talents) do
            tierColIndex[classId][meta.t] = tierColIndex[classId][meta.t] or {}
            tierColIndex[classId][meta.t][meta.c] = talentId
            for ri, rankSpell in ipairs(meta.r) do
                spellIndex[classId][rankSpell] = { tid = talentId, ri = ri }
            end
        end
    end
    indicesBuilt = true
    return true
end

-- ---------------------------------------------------------------------------
-- Persistent spent-rank tracking (self-creating table, acore_characters).
-- ---------------------------------------------------------------------------
local function EnsureTable()
    CharDBExecute(
        "CREATE TABLE IF NOT EXISTS `unbound_character_talents` (" ..
        "`char_guid` INT UNSIGNED NOT NULL, " ..
        "`class_id` TINYINT UNSIGNED NOT NULL, " ..
        "`talent_id` INT UNSIGNED NOT NULL, " ..
        "`rank` TINYINT UNSIGNED NOT NULL, " ..
        "PRIMARY KEY (`char_guid`, `talent_id`)) " ..
        "ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;")
end

local function GetStoredRank(guid, talentId)
    local Q = CharDBQuery(string.format(
        "SELECT `rank` FROM unbound_character_talents WHERE char_guid = %d AND talent_id = %d",
        guid, talentId))
    return Q and Q:GetUInt32(0) or 0
end

local function GetTreePoints(guid, classId)
    local Q = CharDBQuery(string.format(
        "SELECT COALESCE(SUM(`rank`),0) FROM unbound_character_talents " ..
        "WHERE char_guid = %d AND class_id = %d", guid, classId))
    return Q and Q:GetUInt32(0) or 0
end

-- Every cross-class rank this character has paid for, across all trees.
local function GetTotalSpent(guid)
    local Q = CharDBQuery(string.format(
        "SELECT COALESCE(SUM(`rank`),0) FROM unbound_character_talents " ..
        "WHERE char_guid = %d", guid))
    return Q and Q:GetUInt32(0) or 0
end

local function StoreRank(guid, classId, talentId, rank)
    CharDBExecute(string.format(
        "INSERT INTO unbound_character_talents (char_guid, class_id, talent_id, `rank`) " ..
        "VALUES (%d, %d, %d, %d) ON DUPLICATE KEY UPDATE `rank` = %d",
        guid, classId, talentId, rank, rank))
end

-- ---------------------------------------------------------------------------
-- Class-unlock helpers
-- ---------------------------------------------------------------------------
local function GetUnlockedSet(player)
    local set = {}
    local Q = CharDBQuery(string.format(
        "SELECT class_id FROM unbound_character_unlocks WHERE char_guid = %d",
        player:GetGUIDLow()))
    if Q then
        repeat set[Q:GetUInt32(0)] = true until not Q:NextRow()
    end
    return set
end

local function BuildClassList(player)
    local ids = {}
    for classId in pairs(GetUnlockedSet(player)) do ids[#ids + 1] = classId end
    table.sort(ids)
    for i = 1, #ids do ids[i] = tostring(ids[i]) end
    return table.concat(ids, ",")
end

local function Reply(player, text)
    player:SendAddonMessage(PREFIX, text, CHAT_MSG_WHISPER, player)
end

local function SendSync(player)
    if not player then return end
    Reply(player, "CLASSES:" .. BuildClassList(player))
end

-- ---------------------------------------------------------------------------
-- Cross-class talent learn
-- ---------------------------------------------------------------------------
local function HandleLearn(player, payload)
    local classId, spellId = payload:match("^(%d+):(%d+)$")
    classId = tonumber(classId)
    spellId = tonumber(spellId)
    if not classId or not spellId then return end

    local function deny(reason)
        Reply(player, string.format("DENY:%s:%d:%d", reason, classId, spellId))
    end

    -- 1. Class access: native class, or unlocked in Unbound.
    if classId ~= player:GetClass() then
        if not GetUnlockedSet(player)[classId] then
            return deny("LOCKED")
        end
    end

    -- 2. Must be a real talent rank of that class.
    if not EnsureIndices() then
        return deny("INVALID")
    end
    local idx = spellIndex[classId] and spellIndex[classId][spellId]
    if not idx then
        return deny("INVALID")
    end
    local meta = talentMeta[classId][idx.tid]
    local guid = player:GetGUIDLow()
    local stored = GetStoredRank(guid, idx.tid)

    -- 3. Idempotent: already have this rank (or higher) -> ack, no charge.
    if idx.ri <= stored then
        return Reply(player, string.format("LEARNED:%d:%d", classId, spellId))
    end

    -- 4. Rank order: only the next rank, never skip.
    if idx.ri ~= stored + 1 then
        return deny("RANK")
    end

    -- 5. Tier gating: (tier-1)*5 points already spent in this class tree.
    if GetTreePoints(guid, classId) < (meta.t - 1) * POINTS_PER_TIER then
        return deny("TIER")
    end

    -- 6. Prereq talent (if any) must be at max rank.
    if meta.p then
        local pTid = tierColIndex[classId][meta.p[1]] and
                     tierColIndex[classId][meta.p[1]][meta.p[2]]
        if pTid then
            local pMeta = talentMeta[classId][pTid]
            if GetStoredRank(guid, pTid) < pMeta.mr then
                return deny("PREREQ")
            end
        end
    end

    -- 7. Budget: shared real talent pool.
    local free = player:GetFreeTalentPoints()
    if free < 1 then
        return deny("NOPOINTS")
    end

    -- Grant: learn this rank, drop the superseded lower rank, debit a point,
    -- record the spend.
    player:LearnSpell(spellId)
    if idx.ri > 1 then
        player:RemoveSpell(meta.r[idx.ri - 1])
    end
    player:SetFreeTalentPoints(free - 1)
    StoreRank(guid, classId, idx.tid, idx.ri)
    Reply(player, string.format("LEARNED:%d:%d", classId, spellId))
end

-- ---------------------------------------------------------------------------
-- Respec: wipe EVERY talent this character has — the native class tree as well
-- as every cross-class tree — and hand back all of the points.
--
-- Two halves, and the order matters:
--   1. Cross-class ranks live only in unbound_character_talents / the spellbook,
--      so we unlearn them, credit the points back and drop the rows ourselves.
--   2. Native ranks are real talents, so Player::resetTalents() does the work.
--      It recomputes free points as CalculateTalentsPoints() — level total plus
--      any Mentor-purchased bonus points — which is exactly the right end state
--      once step 1 has zeroed the cross-class side. If the character had no
--      native talents spent, resetTalents() early-outs without touching the
--      counter, which is fine: step 1 already left it at the same value.
-- ---------------------------------------------------------------------------
local function HandleReset(player)
    if not EnsureIndices() then return end
    local guid = player:GetGUIDLow()
    local before = player:GetFreeTalentPoints()
    local crossRefund = 0

    local Q = CharDBQuery(string.format(
        "SELECT class_id, talent_id, `rank` FROM unbound_character_talents " ..
        "WHERE char_guid = %d", guid))
    if Q then
        repeat
            local classId = Q:GetUInt32(0)
            local talentId = Q:GetUInt32(1)
            local rank = Q:GetUInt32(2)
            local meta = talentMeta[classId] and talentMeta[classId][talentId]
            if meta then
                for i = 1, rank do
                    if meta.r[i] then player:RemoveSpell(meta.r[i]) end
                end
            end
            crossRefund = crossRefund + rank
        until not Q:NextRow()
    end

    if crossRefund > 0 then
        player:SetFreeTalentPoints(before + crossRefund)
    end
    CharDBExecute(string.format(
        "DELETE FROM unbound_character_talents WHERE char_guid = %d", guid))

    -- true = no reset cost; the cross-class half has always been free.
    player:ResetTalents(true)

    local after = player:GetFreeTalentPoints()
    Reply(player, "RESET:" .. (after > before and (after - before) or crossRefund))
end

-- ---------------------------------------------------------------------------
-- Free-point reconciliation.
--
-- Cross-class ranks are paid for out of the real talent pool, but the server
-- only knows about native talents: Player::InitTalentForLevel() recomputes free
-- points as (level total + purchased bonus - native spent) on login and after
-- every level-up. Without this pass that silently handed every cross-class
-- point back while the spells stayed learned — free respecs by relogging.
-- ---------------------------------------------------------------------------
local function ReconcileFreePoints(player)
    local spent = GetTotalSpent(player:GetGUIDLow())
    if spent <= 0 then return end
    local free = player:GetFreeTalentPoints()
    player:SetFreeTalentPoints(free > spent and (free - spent) or 0)
end

-- Runs on a short delay in both cases: the Player userdata captured here goes
-- stale across the login -> in-world transition, and on level-up GiveLevel()
-- must finish its own InitTalentForLevel() before we adjust the total.
local function ScheduleReconcile(player, delayMs)
    local guid = player:GetGUID()
    player:RegisterEvent(function()
        local live = GetPlayerByGUID(guid)
        if not live or not live:IsInWorld() then return end
        local ok, err = pcall(function() ReconcileFreePoints(live) end)
        if not ok then
            print("[UNBOUND] talent point reconcile ERROR: " .. tostring(err))
        end
    end, delayMs, 1)
end

-- ---------------------------------------------------------------------------
-- Router
-- ---------------------------------------------------------------------------
local function OnAddonMessage(event, sender, msgType, prefix, msg, target)
    if prefix ~= PREFIX or not sender then return end

    if msg == "SYNC" then
        SendSync(sender)
        return false
    end

    if msg == "RESET" then
        HandleReset(sender)
        return false
    end

    local learnPayload = msg:match("^LEARN:(.+)$")
    if learnPayload then
        HandleLearn(sender, learnPayload)
        return false
    end
end

RegisterServerEvent(ADDON_EVENT_ON_MESSAGE, OnAddonMessage)

RegisterPlayerEvent(PLAYER_EVENT_ON_LOGIN, function(event, player)
    ScheduleReconcile(player, 1000)
end)

RegisterPlayerEvent(PLAYER_EVENT_ON_LEVEL_CHANGE, function(event, player, oldLevel)
    ScheduleReconcile(player, 200)
end)

EnsureTable()
-- Indices bind lazily on the first cross-class pick (load-order safe).
print("[UNBOUND] Multiclass sync + cross-class talent bridge loaded " ..
    "(tier/prereq/budget enforced; talent metadata binds on first use).")
WU_TALENT_BRIDGE_EOF

    cat > "$LUA_DIR/unbound_talent_data.lua" <<'WU_TALENT_DATA_EOF'
-- AUTO-GENERATED cross-class talent metadata for the Unbound learn bridge.
-- Source: multiclass-talents-ui/Data/Talents_*.lua  (gen_talent_data.py).
-- Per class: talentId -> { t=tier, c=col, mr=maxRank, r={rank spellIds}, p={ptier,pcol}? }
-- Do not hand-edit.
UnboundTalentData = {
    [1] = { -- Warrior
        [12163]={t=4,c=2,mr=3,r={12163,12711,12712}},
        [12281]={t=5,c=4,mr=5,r={12281,12812,12813,12814,12815}},
        [12282]={t=1,c=1,mr=3,r={12282,12663,12664}},
        [12284]={t=5,c=3,mr=5,r={12284,12701,12702,12703,12704}},
        [12285]={t=2,c=1,mr=2,r={12285,12697}},
        [12286]={t=1,c=3,mr=2,r={12286,12658}},
        [12287]={t=1,c=3,mr=3,r={12287,12665,12666}},
        [12289]={t=6,c=3,mr=3,r={12289,12668,23695}},
        [12290]={t=3,c=1,mr=2,r={12290,12963}},
        [12292]={t=5,c=2,mr=1,r={12292}},
        [12294]={t=7,c=2,mr=1,r={12294}, p={5,2}},
        [12295]={t=2,c=3,mr=3,r={12295,12676,12677}},
        [12296]={t=3,c=2,mr=1,r={12296}},
        [12297]={t=2,c=3,mr=5,r={12297,12750,12751,12752,12753}},
        [12298]={t=1,c=2,mr=5,r={12298,12724,12725,12726,12727}},
        [12299]={t=3,c=4,mr=5,r={12299,12761,12762,12763,12764}},
        [12300]={t=2,c=2,mr=3,r={12300,12959,12960}},
        [12301]={t=1,c=1,mr=2,r={12301,12818}},
        [12308]={t=4,c=3,mr=3,r={12308,12810,12811}},
        [12311]={t=5,c=3,mr=2,r={12311,12958}},
        [12312]={t=5,c=1,mr=2,r={12312,12803}},
        [12313]={t=4,c=2,mr=2,r={12313,12804}},
        [12317]={t=4,c=3,mr=5,r={12317,13045,13046,13047,13048}},
        [12318]={t=3,c=4,mr=5,r={12318,12857,12858,12860,12861}},
        [12319]={t=6,c=3,mr=5,r={12319,12971,12972,12973,12974}},
        [12320]={t=1,c=3,mr=5,r={12320,12852,12853,12855,12856}},
        [12321]={t=1,c=2,mr=2,r={12321,12835}},
        [12322]={t=2,c=3,mr=5,r={12322,12999,13000,13001,13002}},
        [12323]={t=3,c=2,mr=1,r={12323}},
        [12324]={t=2,c=2,mr=5,r={12324,12876,12877,12878,12879}},
        [12328]={t=5,c=2,mr=1,r={12328}},
        [12329]={t=3,c=1,mr=3,r={12329,12950,20496}},
        [12700]={t=5,c=1,mr=5,r={12700,12781,12783,12784,12785}},
        [12797]={t=3,c=2,mr=2,r={12797,12799}},
        [12809]={t=5,c=2,mr=1,r={12809}},
        [12834]={t=3,c=4,mr=3,r={12834,12849,12867}, p={3,3}},
        [12862]={t=7,c=4,mr=2,r={12862,12330}},
        [12975]={t=3,c=1,mr=1,r={12975}},
        [16462]={t=1,c=2,mr=5,r={16462,16463,16464,16465,16466}},
        [16487]={t=3,c=3,mr=3,r={16487,16489,16492}},
        [16493]={t=3,c=3,mr=2,r={16493,16494}},
        [16538]={t=6,c=3,mr=5,r={16538,16539,16540,16541,16542}},
        [20243]={t=9,c=2,mr=1,r={20243}},
        [20500]={t=6,c=1,mr=2,r={20500,20501}},
        [20502]={t=4,c=2,mr=2,r={20502,20503}},
        [20504]={t=6,c=1,mr=2,r={20504,20505}},
        [23584]={t=4,c=1,mr=5,r={23584,23585,23586,23587,23588}},
        [23881]={t=7,c=2,mr=1,r={23881}, p={5,2}},
        [29140]={t=8,c=2,mr=3,r={29140,29143,29144}},
        [29590]={t=5,c=1,mr=3,r={29590,29591,29592}},
        [29593]={t=7,c=1,mr=2,r={29593,29594}},
        [29598]={t=3,c=3,mr=2,r={29598,29599}},
        [29623]={t=9,c=2,mr=1,r={29623}},
        [29721]={t=7,c=4,mr=2,r={29721,29776}},
        [29723]={t=9,c=1,mr=3,r={29723,29725,29724}},
        [29759]={t=8,c=4,mr=5,r={29759,29760,29761,29762,29763}},
        [29787]={t=7,c=3,mr=3,r={29787,29790,29792}},
        [29801]={t=9,c=2,mr=1,r={29801}, p={7,2}},
        [29834]={t=7,c=1,mr=2,r={29834,29838}},
        [29836]={t=9,c=3,mr=2,r={29836,29859}},
        [29888]={t=5,c=3,mr=2,r={29888,29889}},
        [35446]={t=8,c=2,mr=3,r={35446,35448,35449}, p={7,2}},
        [46854]={t=6,c=4,mr=2,r={46854,46855}},
        [46859]={t=8,c=3,mr=2,r={46859,46860}},
        [46865]={t=7,c=3,mr=2,r={46865,46866}},
        [46867]={t=10,c=2,mr=5,r={46867,56611,56612,56613,56614}},
        [46908]={t=7,c=1,mr=3,r={46908,46909,56924}},
        [46910]={t=8,c=1,mr=2,r={46910,46911}},
        [46913]={t=9,c=3,mr=3,r={46913,46914,46915}, p={7,2}},
        [46917]={t=11,c=2,mr=1,r={46917}},
        [46924]={t=11,c=2,mr=1,r={46924}},
        [46945]={t=8,c=3,mr=2,r={46945,46949}},
        [46951]={t=10,c=2,mr=3,r={46951,46952,46953}, p={9,2}},
        [46968]={t=11,c=2,mr=1,r={46968}},
        [47294]={t=9,c=3,mr=3,r={47294,47295,47296}},
        [50685]={t=2,c=2,mr=3,r={50685,50686,50687}},
        [50720]={t=7,c=2,mr=1,r={50720}, p={5,2}},
        [56636]={t=4,c=3,mr=3,r={56636,56637,56638}},
        [56927]={t=10,c=2,mr=5,r={56927,56929,56930,56931,56932}},
        [57499]={t=9,c=1,mr=1,r={57499}},
        [58872]={t=10,c=3,mr=2,r={58872,58874}},
        [59088]={t=4,c=1,mr=2,r={59088,59089}},
        [60970]={t=9,c=1,mr=1,r={60970}},
        [61216]={t=1,c=1,mr=3,r={61216,61221,61222}},
        [64976]={t=8,c=1,mr=1,r={64976}},
    },
    [2] = { -- Paladin
        [5923]={t=6,c=3,mr=5,r={5923,5924,5925,5926,25829}},
        [9452]={t=3,c=1,mr=2,r={9452,26016}},
        [9453]={t=2,c=3,mr=2,r={9453,25836}},
        [9799]={t=4,c=1,mr=2,r={9799,25988}},
        [20042]={t=2,c=3,mr=2,r={20042,20045}},
        [20049]={t=6,c=2,mr=3,r={20049,20056,20057}, p={3,2}},
        [20060]={t=1,c=2,mr=5,r={20060,20061,20062,20063,20064}},
        [20066]={t=7,c=2,mr=1,r={20066}},
        [20096]={t=2,c=3,mr=5,r={20096,20097,20098,20099,20100}},
        [20101]={t=1,c=3,mr=5,r={20101,20102,20103,20104,20105}},
        [20111]={t=5,c=1,mr=3,r={20111,20112,20113}},
        [20117]={t=3,c=2,mr=5,r={20117,20118,20119,20120,20121}},
        [20127]={t=8,c=1,mr=3,r={20127,20130,20135}},
        [20138]={t=4,c=3,mr=3,r={20138,20139,20140}},
        [20143]={t=3,c=3,mr=5,r={20143,20144,20145,20146,20147}},
        [20174]={t=2,c=2,mr=2,r={20174,20175}},
        [20177]={t=5,c=3,mr=5,r={20177,20179,20181,20180,20182}},
        [20196]={t=6,c=3,mr=3,r={20196,20197,20198}},
        [20205]={t=1,c=2,mr=5,r={20205,20206,20207,20209,20208}},
        [20210]={t=3,c=2,mr=5,r={20210,20212,20213,20214,20215}},
        [20216]={t=5,c=2,mr=1,r={20216}, p={3,2}},
        [20224]={t=1,c=3,mr=5,r={20224,20225,20330,20331,20332}},
        [20234]={t=3,c=3,mr=2,r={20234,20235}},
        [20237]={t=2,c=1,mr=3,r={20237,20238,20239}},
        [20244]={t=4,c=3,mr=2,r={20244,20245}},
        [20254]={t=4,c=1,mr=3,r={20254,20255,20256}},
        [20257]={t=2,c=2,mr=5,r={20257,20258,20259,20260,20261}},
        [20262]={t=1,c=3,mr=5,r={20262,20263,20264,20265,20266}},
        [20335]={t=2,c=2,mr=3,r={20335,20336,20337}},
        [20359]={t=5,c=3,mr=3,r={20359,20360,20361}},
        [20375]={t=3,c=3,mr=1,r={20375}},
        [20468]={t=3,c=2,mr=3,r={20468,20469,20470}},
        [20473]={t=7,c=2,mr=1,r={20473}, p={5,2}},
        [20487]={t=4,c=2,mr=2,r={20487,20488}},
        [20911]={t=5,c=2,mr=1,r={20911}},
        [20925]={t=7,c=2,mr=1,r={20925}, p={5,2}},
        [25956]={t=2,c=1,mr=2,r={25956,25957}},
        [26022]={t=3,c=4,mr=2,r={26022,26023}},
        [31785]={t=7,c=1,mr=2,r={31785,33776}},
        [31821]={t=3,c=1,mr=1,r={31821}},
        [31822]={t=5,c=1,mr=2,r={31822,31823}},
        [31825]={t=6,c=1,mr=2,r={31825,31826}},
        [31828]={t=7,c=3,mr=3,r={31828,31829,31830}},
        [31833]={t=7,c=1,mr=3,r={31833,31835,31836}},
        [31837]={t=8,c=3,mr=5,r={31837,31838,31839,31840,31841}},
        [31842]={t=9,c=1,mr=1,r={31842}},
        [31844]={t=2,c=1,mr=3,r={31844,31845,53519}},
        [31848]={t=6,c=1,mr=2,r={31848,31849}},
        [31850]={t=7,c=3,mr=3,r={31850,31851,31852}},
        [31858]={t=8,c=3,mr=3,r={31858,31859,31860}},
        [31866]={t=4,c=4,mr=3,r={31866,31867,31868}},
        [31869]={t=5,c=3,mr=1,r={31869}},
        [31871]={t=6,c=3,mr=2,r={31871,31872}},
        [31876]={t=7,c=3,mr=3,r={31876,31877,31878}},
        [31879]={t=8,c=2,mr=3,r={31879,31880,31881}, p={7,2}},
        [31935]={t=9,c=2,mr=1,r={31935}, p={7,2}},
        [32043]={t=4,c=3,mr=3,r={32043,35396,35397}},
        [35395]={t=9,c=2,mr=1,r={35395}},
        [53375]={t=8,c=3,mr=2,r={53375,53376}},
        [53379]={t=9,c=1,mr=3,r={53379,53484,53648}},
        [53380]={t=10,c=2,mr=3,r={53380,53381,53382}},
        [53385]={t=11,c=2,mr=1,r={53385}},
        [53486]={t=7,c=1,mr=2,r={53486,53488}},
        [53501]={t=9,c=3,mr=3,r={53501,53502,53503}},
        [53527]={t=4,c=1,mr=2,r={53527,53530}, p={3,1}},
        [53551]={t=8,c=1,mr=3,r={53551,53552,53553}},
        [53556]={t=10,c=3,mr=2,r={53556,53557}},
        [53563]={t=11,c=2,mr=1,r={53563}},
        [53569]={t=10,c=2,mr=2,r={53569,53576}, p={7,2}},
        [53583]={t=9,c=3,mr=2,r={53583,53585}},
        [53590]={t=9,c=1,mr=3,r={53590,53591,53592}},
        [53595]={t=11,c=2,mr=1,r={53595}},
        [53660]={t=4,c=4,mr=2,r={53660,53661}},
        [53671]={t=9,c=3,mr=5,r={53671,53673,54151,54154,54155}},
        [53695]={t=10,c=3,mr=2,r={53695,53696}},
        [53709]={t=10,c=2,mr=3,r={53709,53710,53711}, p={9,2}},
        [63646]={t=1,c=2,mr=5,r={63646,63647,63648,63649,63650}},
        [64205]={t=3,c=1,mr=1,r={64205}},
    },
    [3] = { -- Hunter
        [3674]={t=9,c=2,mr=1,r={3674}},
        [19159]={t=1,c=3,mr=2,r={19159,19160}},
        [19168]={t=6,c=1,mr=5,r={19168,19180,19181,24296,24297}},
        [19184]={t=2,c=2,mr=3,r={19184,19387,19388}},
        [19255]={t=3,c=1,mr=5,r={19255,19256,19257,19258,19259}},
        [19286]={t=3,c=4,mr=2,r={19286,19287}},
        [19290]={t=2,c=1,mr=3,r={19290,19294,24283}},
        [19295]={t=3,c=3,mr=3,r={19295,19297,19298}},
        [19306]={t=5,c=3,mr=1,r={19306}, p={3,3}},
        [19370]={t=5,c=2,mr=3,r={19370,19371,19373}},
        [19376]={t=2,c=3,mr=3,r={19376,63457,63458}},
        [19386]={t=7,c=2,mr=1,r={19386}, p={5,2}},
        [19407]={t=1,c=1,mr=2,r={19407,19412}},
        [19416]={t=4,c=3,mr=5,r={19416,19417,19418,19419,19420}},
        [19421]={t=2,c=2,mr=3,r={19421,19422,19423}},
        [19426]={t=1,c=3,mr=5,r={19426,19427,19429,19430,19431}},
        [19434]={t=3,c=3,mr=1,r={19434}, p={2,3}},
        [19454]={t=3,c=2,mr=3,r={19454,19455,19456}},
        [19461]={t=5,c=3,mr=3,r={19461,19462,24691}},
        [19464]={t=4,c=2,mr=3,r={19464,19465,19466}},
        [19485]={t=2,c=3,mr=5,r={19485,19487,19488,19489,19490}},
        [19498]={t=1,c=2,mr=3,r={19498,19499,19500}},
        [19503]={t=3,c=2,mr=1,r={19503}},
        [19506]={t=7,c=2,mr=1,r={19506}, p={5,2}},
        [19507]={t=6,c=4,mr=3,r={19507,19508,19509}},
        [19549]={t=2,c=2,mr=3,r={19549,19550,19551}},
        [19552]={t=1,c=2,mr=5,r={19552,19553,19554,19555,19556}},
        [19559]={t=3,c=1,mr=2,r={19559,19560}},
        [19572]={t=4,c=2,mr=2,r={19572,19573}},
        [19574]={t=7,c=2,mr=1,r={19574}, p={5,2}},
        [19577]={t=5,c=2,mr=1,r={19577}},
        [19578]={t=5,c=1,mr=2,r={19578,20895}},
        [19583]={t=1,c=3,mr=5,r={19583,19584,19585,19586,19587}},
        [19590]={t=5,c=4,mr=2,r={19590,19592}},
        [19598]={t=4,c=3,mr=5,r={19598,19599,19600,19601,19602}},
        [19609]={t=2,c=3,mr=3,r={19609,19610,19612}},
        [19616]={t=3,c=3,mr=5,r={19616,19617,19618,19619,19620}},
        [19621]={t=6,c=3,mr=5,r={19621,19622,19623,19624,19625}, p={4,3}},
        [23989]={t=5,c=2,mr=1,r={23989}},
        [24443]={t=2,c=4,mr=2,r={24443,19575}},
        [34453]={t=6,c=1,mr=2,r={34453,34454}},
        [34455]={t=7,c=1,mr=3,r={34455,34459,34460}},
        [34462]={t=7,c=3,mr=3,r={34462,34464,34465}},
        [34466]={t=8,c=3,mr=5,r={34466,34467,34468,34469,34470}},
        [34475]={t=6,c=1,mr=2,r={34475,34476}},
        [34482]={t=2,c=1,mr=3,r={34482,34483,34484}},
        [34485]={t=8,c=2,mr=5,r={34485,34486,34487,34488,34489}},
        [34490]={t=9,c=2,mr=1,r={34490}, p={8,2}},
        [34491]={t=6,c=3,mr=3,r={34491,34492,34493}},
        [34494]={t=2,c=4,mr=2,r={34494,34496}},
        [34497]={t=7,c=3,mr=3,r={34497,34498,34499}},
        [34500]={t=7,c=1,mr=3,r={34500,34502,34503}, p={6,1}},
        [34506]={t=8,c=1,mr=5,r={34506,34507,34508,34838,34839}},
        [34692]={t=9,c=2,mr=1,r={34692}, p={7,2}},
        [34948]={t=3,c=4,mr=2,r={34948,34949}},
        [34950]={t=3,c=1,mr=2,r={34950,34954}},
        [35029]={t=2,c=1,mr=2,r={35029,35030}},
        [35100]={t=5,c=1,mr=2,r={35100,35102}},
        [35104]={t=7,c=3,mr=3,r={35104,35110,35111}, p={5,3}},
        [52783]={t=1,c=1,mr=5,r={52783,52785,52786,52787,52788}},
        [53209]={t=11,c=2,mr=1,r={53209}},
        [53215]={t=9,c=1,mr=3,r={53215,53216,53217}},
        [53221]={t=9,c=3,mr=3,r={53221,53222,53224}},
        [53228]={t=8,c=3,mr=2,r={53228,53232}},
        [53234]={t=7,c=1,mr=3,r={53234,53237,53238}},
        [53241]={t=10,c=2,mr=5,r={53241,53243,53244,53245,53246}},
        [53252]={t=8,c=1,mr=2,r={53252,53253}, p={7,1}},
        [53256]={t=9,c=3,mr=3,r={53256,53259,53260}, p={8,3}},
        [53262]={t=9,c=1,mr=3,r={53262,53263,53264}},
        [53265]={t=3,c=2,mr=1,r={53265}},
        [53270]={t=11,c=2,mr=1,r={53270}},
        [53290]={t=10,c=3,mr=3,r={53290,53291,53292}, p={7,3}},
        [53295]={t=8,c=2,mr=3,r={53295,53296,53297}, p={7,2}},
        [53298]={t=9,c=1,mr=2,r={53298,53299}},
        [53301]={t=11,c=2,mr=1,r={53301}, p={9,2}},
        [53302]={t=9,c=4,mr=3,r={53302,53303,53304}},
        [53620]={t=1,c=2,mr=3,r={53620,53621,53622}},
        [56314]={t=10,c=2,mr=5,r={56314,56315,56316,56317,56318}},
        [56333]={t=4,c=2,mr=3,r={56333,56336,56337}},
        [56339]={t=5,c=1,mr=3,r={56339,56340,56341}, p={3,1}},
        [56342]={t=4,c=4,mr=3,r={56342,56343,56344}},
    },
    [4] = { -- Rogue
        [1329]={t=9,c=2,mr=1,r={1329}, p={7,2}},
        [5952]={t=8,c=1,mr=2,r={5952,51679}},
        [13705]={t=2,c=4,mr=5,r={13705,13832,13843,13844,13845}},
        [13706]={t=3,c=3,mr=5,r={13706,13804,13805,13806,13807}, p={1,3}},
        [13709]={t=5,c=1,mr=5,r={13709,13800,13801,13802,13803}},
        [13712]={t=4,c=3,mr=3,r={13712,13788,13789}},
        [13713]={t=2,c=2,mr=3,r={13713,13853,13854}},
        [13715]={t=1,c=3,mr=5,r={13715,13848,13849,13851,13852}},
        [13732]={t=1,c=2,mr=2,r={13732,13863}},
        [13733]={t=2,c=4,mr=3,r={13733,13865,13866}},
        [13741]={t=1,c=1,mr=3,r={13741,13793,13792}},
        [13742]={t=3,c=1,mr=2,r={13742,13872}},
        [13743]={t=4,c=2,mr=2,r={13743,13875}},
        [13750]={t=7,c=2,mr=1,r={13750}},
        [13754]={t=4,c=1,mr=2,r={13754,13867}},
        [13877]={t=5,c=2,mr=1,r={13877}},
        [13958]={t=1,c=2,mr=3,r={13958,13970,13971}},
        [13960]={t=5,c=3,mr=5,r={13960,13961,13962,13963,13964}},
        [13975]={t=2,c=3,mr=3,r={13975,14062,14063}},
        [13976]={t=4,c=2,mr=3,r={13976,13979,13980}},
        [13981]={t=3,c=1,mr=2,r={13981,14066}},
        [13983]={t=4,c=1,mr=3,r={13983,14070,14071}},
        [14057]={t=1,c=3,mr=2,r={14057,14072}},
        [14076]={t=2,c=2,mr=2,r={14076,14094}},
        [14079]={t=4,c=3,mr=2,r={14079,14080}},
        [14082]={t=5,c=3,mr=2,r={14082,14083}},
        [14113]={t=4,c=3,mr=5,r={14113,14114,14115,14116,14117}},
        [14128]={t=3,c=3,mr=5,r={14128,14132,14135,14136,14137}, p={1,3}},
        [14138]={t=1,c=3,mr=5,r={14138,14139,14140,14141,14142}},
        [14144]={t=1,c=2,mr=2,r={14144,14148}},
        [14156]={t=2,c=1,mr=3,r={14156,14160,14161}},
        [14158]={t=6,c=3,mr=2,r={14158,14159}},
        [14162]={t=1,c=1,mr=3,r={14162,14163,14164}},
        [14165]={t=2,c=1,mr=2,r={14165,14166}},
        [14168]={t=3,c=2,mr=2,r={14168,14169}},
        [14171]={t=3,c=3,mr=3,r={14171,14172,14173}},
        [14174]={t=5,c=3,mr=3,r={14174,14175,14176}},
        [14177]={t=5,c=2,mr=1,r={14177}},
        [14179]={t=1,c=1,mr=5,r={14179,58422,58423,58424,58425}},
        [14183]={t=7,c=2,mr=1,r={14183}, p={5,2}},
        [14185]={t=5,c=2,mr=1,r={14185}},
        [14186]={t=6,c=2,mr=5,r={14186,14190,14193,14194,14195}, p={5,2}},
        [14251]={t=3,c=2,mr=1,r={14251}, p={2,2}},
        [14278]={t=3,c=2,mr=1,r={14278}},
        [14983]={t=3,c=1,mr=1,r={14983}},
        [16511]={t=5,c=4,mr=1,r={16511}, p={3,3}},
        [16513]={t=4,c=2,mr=3,r={16513,16514,16515}},
        [18427]={t=4,c=4,mr=5,r={18427,18428,18429,61330,61331}},
        [30892]={t=2,c=1,mr=2,r={30892,30893}},
        [30894]={t=5,c=1,mr=2,r={30894,30895}},
        [30902]={t=6,c=3,mr=5,r={30902,30903,30904,30905,30906}},
        [30919]={t=6,c=2,mr=2,r={30919,30920}, p={5,2}},
        [31122]={t=7,c=1,mr=3,r={31122,31123,61329}},
        [31124]={t=6,c=3,mr=2,r={31124,31126}},
        [31130]={t=7,c=3,mr=2,r={31130,31131}},
        [31208]={t=5,c=1,mr=2,r={31208,31209}},
        [31211]={t=7,c=1,mr=3,r={31211,31212,31213}},
        [31216]={t=8,c=2,mr=5,r={31216,31217,31218,31219,31220}, p={7,2}},
        [31221]={t=6,c=1,mr=3,r={31221,31222,31223}},
        [31226]={t=9,c=1,mr=3,r={31226,31227,58410}},
        [31228]={t=7,c=3,mr=3,r={31228,31229,31230}},
        [31234]={t=8,c=3,mr=3,r={31234,31235,31236}},
        [31244]={t=5,c=4,mr=2,r={31244,31245}},
        [31380]={t=7,c=3,mr=3,r={31380,31382,31383}},
        [32601]={t=9,c=2,mr=1,r={32601}, p={7,2}},
        [35541]={t=8,c=3,mr=5,r={35541,35550,35551,35552,35553}},
        [36554]={t=9,c=2,mr=1,r={36554}},
        [51625]={t=7,c=1,mr=2,r={51625,51626}},
        [51627]={t=9,c=3,mr=3,r={51627,51628,51629}},
        [51632]={t=2,c=2,mr=2,r={51632,51633}},
        [51634]={t=8,c=1,mr=3,r={51634,51635,51636}},
        [51662]={t=11,c=2,mr=1,r={51662}},
        [51664]={t=10,c=2,mr=5,r={51664,51665,51667,51668,51669}},
        [51672]={t=9,c=1,mr=2,r={51672,51674}},
        [51682]={t=9,c=3,mr=2,r={51682,58413}},
        [51685]={t=10,c=2,mr=5,r={51685,51686,51687,51688,51689}},
        [51690]={t=11,c=2,mr=1,r={51690}},
        [51692]={t=8,c=3,mr=2,r={51692,51696}},
        [51698]={t=9,c=1,mr=3,r={51698,51700,51701}},
        [51708]={t=10,c=2,mr=5,r={51708,51709,51710,51711,51712}},
        [51713]={t=11,c=2,mr=1,r={51713}},
        [58414]={t=9,c=3,mr=2,r={58414,58415}},
        [58426]={t=7,c=2,mr=1,r={58426}},
    },
    [5] = { -- Priest
        [724]={t=7,c=2,mr=1,r={724}, p={5,2}},
        [10060]={t=7,c=2,mr=1,r={10060}, p={5,2}},
        [14520]={t=4,c=2,mr=3,r={14520,14780,14781}},
        [14521]={t=3,c=1,mr=3,r={14521,14776,14777}},
        [14522]={t=1,c=2,mr=5,r={14522,14788,14789,14790,14791}},
        [14523]={t=2,c=1,mr=3,r={14523,14784,14785}},
        [14531]={t=2,c=4,mr=2,r={14531,14774}},
        [14747]={t=2,c=2,mr=3,r={14747,14770,14771}},
        [14748]={t=3,c=3,mr=3,r={14748,14768,14769}},
        [14749]={t=2,c=3,mr=2,r={14749,14767}},
        [14750]={t=4,c=4,mr=2,r={14750,14772}},
        [14751]={t=3,c=2,mr=1,r={14751}},
        [14889]={t=1,c=3,mr=5,r={14889,15008,15009,15010,15011}},
        [14892]={t=3,c=4,mr=3,r={14892,15362,15363}},
        [14898]={t=6,c=3,mr=5,r={14898,15349,15354,15355,15356}},
        [14901]={t=5,c=3,mr=5,r={14901,15028,15029,15030,15031}},
        [14908]={t=1,c=2,mr=3,r={14908,15020,17191}},
        [14909]={t=4,c=3,mr=2,r={14909,15017}, p={2,3}},
        [14910]={t=6,c=1,mr=2,r={14910,33371}},
        [14911]={t=5,c=1,mr=2,r={14911,15018}},
        [14912]={t=4,c=2,mr=3,r={14912,15013,15014}},
        [14913]={t=1,c=1,mr=2,r={14913,15012}},
        [15257]={t=4,c=4,mr=3,r={15257,15331,15332}},
        [15259]={t=1,c=3,mr=5,r={15259,15307,15308,15309,15310}},
        [15260]={t=2,c=3,mr=3,r={15260,15327,15328}},
        [15270]={t=1,c=1,mr=3,r={15270,15335,15336}},
        [15273]={t=3,c=2,mr=5,r={15273,15312,15313,15314,15316}},
        [15274]={t=4,c=2,mr=2,r={15274,15311}},
        [15275]={t=2,c=2,mr=2,r={15275,15317}},
        [15286]={t=5,c=2,mr=1,r={15286}},
        [15318]={t=2,c=1,mr=3,r={15318,15272,15320}},
        [15337]={t=1,c=2,mr=2,r={15337,15338}, p={1,1}},
        [15392]={t=3,c=1,mr=2,r={15392,15448}},
        [15407]={t=3,c=3,mr=1,r={15407}},
        [15473]={t=7,c=2,mr=1,r={15473}, p={5,2}},
        [15487]={t=5,c=1,mr=1,r={15487}, p={3,1}},
        [17322]={t=4,c=3,mr=2,r={17322,17323}},
        [18530]={t=2,c=3,mr=5,r={18530,18531,18533,18534,18535}},
        [18551]={t=5,c=2,mr=5,r={18551,18552,18553,18554,18555}},
        [19236]={t=3,c=1,mr=1,r={19236}},
        [20711]={t=5,c=2,mr=1,r={20711}},
        [27789]={t=4,c=1,mr=2,r={27789,27790}},
        [27811]={t=3,c=2,mr=3,r={27811,27815,27816}},
        [27839]={t=5,c=3,mr=2,r={27839,27840}, p={5,2}},
        [27900]={t=2,c=2,mr=5,r={27900,27901,27902,27903,27904}},
        [33142]={t=7,c=3,mr=3,r={33142,33145,33146}},
        [33150]={t=6,c=1,mr=2,r={33150,33154}},
        [33158]={t=8,c=2,mr=5,r={33158,33159,33160,33161,33162}},
        [33167]={t=4,c=1,mr=3,r={33167,33171,33172}},
        [33186]={t=6,c=1,mr=2,r={33186,33190}},
        [33191]={t=8,c=3,mr=3,r={33191,33192,33193}},
        [33201]={t=5,c=1,mr=2,r={33201,33202}},
        [33206]={t=9,c=2,mr=1,r={33206}},
        [33213]={t=5,c=4,mr=3,r={33213,33214,33215}},
        [33221]={t=7,c=3,mr=5,r={33221,33222,33223,33224,33225}},
        [34753]={t=7,c=1,mr=3,r={34753,34859,34860}},
        [34861]={t=9,c=2,mr=1,r={34861}},
        [34908]={t=6,c=3,mr=3,r={34908,34909,34910}},
        [34914]={t=9,c=2,mr=1,r={34914}, p={7,2}},
        [45234]={t=7,c=1,mr=3,r={45234,45243,45244}},
        [47507]={t=8,c=3,mr=2,r={47507,47508}},
        [47509]={t=9,c=1,mr=3,r={47509,47511,47515}},
        [47516]={t=9,c=3,mr=2,r={47516,47517}},
        [47535]={t=8,c=2,mr=3,r={47535,47536,47537}},
        [47540]={t=11,c=2,mr=1,r={47540}},
        [47558]={t=9,c=3,mr=3,r={47558,47559,47560}},
        [47562]={t=10,c=2,mr=5,r={47562,47564,47565,47566,47567}},
        [47569]={t=8,c=1,mr=2,r={47569,47570}, p={7,2}},
        [47573]={t=10,c=3,mr=5,r={47573,47577,47578,51166,51167}},
        [47580]={t=9,c=3,mr=3,r={47580,47581,47582}},
        [47585]={t=11,c=2,mr=1,r={47585}, p={9,2}},
        [47586]={t=1,c=3,mr=5,r={47586,47587,47588,52802,52803}},
        [47788]={t=11,c=2,mr=1,r={47788}},
        [52795]={t=10,c=2,mr=5,r={52795,52797,52798,52799,52800}},
        [57470]={t=8,c=1,mr=2,r={57470,57472}},
        [63504]={t=7,c=3,mr=3,r={63504,63505,63506}},
        [63534]={t=9,c=1,mr=3,r={63534,63542,63543}},
        [63574]={t=5,c=3,mr=1,r={63574}, p={3,3}},
        [63625]={t=6,c=3,mr=3,r={63625,63626,63627}},
        [63730]={t=8,c=3,mr=3,r={63730,63733,63737}},
        [64044]={t=9,c=1,mr=1,r={64044}},
        [64127]={t=8,c=1,mr=2,r={64127,64129}},
    },
    [6] = { -- DeathKnight
        [48962]={t=1,c=2,mr=3,r={48962,49567,49568}},
        [48963]={t=2,c=2,mr=3,r={48963,49564,49565}},
        [48965]={t=2,c=4,mr=3,r={48965,49571,49572}},
        [48977]={t=5,c=1,mr=3,r={48977,49394,49395}},
        [48978]={t=2,c=1,mr=5,r={48978,49390,49391,49392,49393}},
        [48979]={t=1,c=1,mr=2,r={48979,49483}},
        [48982]={t=3,c=1,mr=1,r={48982}},
        [48985]={t=4,c=1,mr=3,r={48985,49488,49489}, p={3,1}},
        [48987]={t=3,c=2,mr=5,r={48987,49477,49478,49479,49480}},
        [48988]={t=6,c=2,mr=3,r={48988,49503,49504}, p={3,2}},
        [48997]={t=1,c=2,mr=3,r={48997,49490,49491}},
        [49004]={t=2,c=2,mr=3,r={49004,49508,49509}},
        [49005]={t=5,c=4,mr=1,r={49005}},
        [49006]={t=5,c=3,mr=3,r={49006,49526,50029}},
        [49013]={t=3,c=1,mr=3,r={49013,55236,55237}},
        [49015]={t=4,c=4,mr=3,r={49015,50154,55136}},
        [49016]={t=7,c=2,mr=1,r={49016}},
        [49018]={t=8,c=2,mr=3,r={49018,49529,49530}},
        [49023]={t=9,c=3,mr=3,r={49023,49533,49534}},
        [49024]={t=6,c=2,mr=2,r={49024,49538}},
        [49027]={t=7,c=1,mr=3,r={49027,49542,49543}},
        [49028]={t=11,c=2,mr=1,r={49028}},
        [49032]={t=8,c=2,mr=3,r={49032,49631,49632}},
        [49036]={t=2,c=1,mr=2,r={49036,49562}},
        [49039]={t=3,c=2,mr=1,r={49039}},
        [49042]={t=1,c=3,mr=5,r={49042,49786,49787,49788,49789}},
        [49137]={t=4,c=4,mr=2,r={49137,49657}},
        [49140]={t=2,c=3,mr=5,r={49140,49661,49662,49663,49664}},
        [49143]={t=9,c=2,mr=1,r={49143}},
        [49145]={t=4,c=3,mr=3,r={49145,49495,49497}},
        [49146]={t=4,c=2,mr=2,r={49146,51267}},
        [49149]={t=4,c=3,mr=2,r={49149,50115}},
        [49158]={t=3,c=3,mr=1,r={49158}},
        [49175]={t=1,c=1,mr=3,r={49175,50031,51456}},
        [49182]={t=1,c=3,mr=5,r={49182,49500,49501,55225,55226}},
        [49184]={t=11,c=2,mr=1,r={49184}},
        [49186]={t=5,c=2,mr=3,r={49186,51108,51109}},
        [49188]={t=6,c=3,mr=3,r={49188,56822,59057}},
        [49189]={t=9,c=1,mr=3,r={49189,50149,50150}},
        [49194]={t=5,c=1,mr=1,r={49194}},
        [49200]={t=9,c=1,mr=3,r={49200,50151,50152}},
        [49202]={t=10,c=2,mr=5,r={49202,50127,50128,50129,50130}},
        [49203]={t=7,c=2,mr=1,r={49203}},
        [49206]={t=11,c=2,mr=1,r={49206}},
        [49208]={t=6,c=3,mr=3,r={49208,56834,56835}},
        [49217]={t=9,c=1,mr=3,r={49217,49654,49655}},
        [49219]={t=4,c=3,mr=3,r={49219,49627,49628}},
        [49220]={t=5,c=2,mr=5,r={49220,49633,49635,49636,49638}},
        [49222]={t=8,c=3,mr=1,r={49222}},
        [49223]={t=5,c=3,mr=2,r={49223,49599}},
        [49224]={t=6,c=2,mr=3,r={49224,49610,49611}},
        [49226]={t=2,c=4,mr=3,r={49226,50137,50138}},
        [49455]={t=1,c=2,mr=2,r={49455,50147}},
        [49467]={t=3,c=3,mr=3,r={49467,50033,50034}},
        [49471]={t=5,c=3,mr=3,r={49471,49790,49791}},
        [49588]={t=2,c=3,mr=2,r={49588,49589}},
        [49796]={t=5,c=4,mr=1,r={49796}},
        [50040]={t=7,c=1,mr=3,r={50040,50041,50043}},
        [50117]={t=10,c=2,mr=5,r={50117,50118,50119,50120,50121}},
        [50187]={t=9,c=3,mr=3,r={50187,50190,50191}},
        [50365]={t=7,c=3,mr=2,r={50365,50371}},
        [50384]={t=7,c=3,mr=2,r={50384,50385}},
        [50391]={t=7,c=3,mr=2,r={50391,50392}},
        [50880]={t=3,c=1,mr=5,r={50880,50884,50885,50886,50887}, p={1,1}},
        [51052]={t=7,c=2,mr=1,r={51052}, p={6,2}},
        [51099]={t=9,c=2,mr=3,r={51099,51160,51161}, p={8,2}},
        [51123]={t=4,c=2,mr=5,r={51123,51127,51128,51129,51130}},
        [51271]={t=8,c=3,mr=1,r={51271}},
        [51459]={t=3,c=2,mr=5,r={51459,51462,51463,51464,51465}},
        [51468]={t=3,c=3,mr=3,r={51468,51472,51473}},
        [51745]={t=1,c=1,mr=2,r={51745,51746}},
        [52143]={t=6,c=4,mr=1,r={52143}, p={4,4}},
        [53137]={t=6,c=3,mr=2,r={53137,53138}},
        [54639]={t=8,c=2,mr=3,r={54639,54638,54637}},
        [55050]={t=9,c=2,mr=1,r={55050}},
        [55061]={t=2,c=2,mr=2,r={55061,55062}},
        [55090]={t=9,c=3,mr=1,r={55090}},
        [55107]={t=2,c=3,mr=2,r={55107,55108}},
        [55129]={t=1,c=3,mr=5,r={55129,55130,55131,55132,55133}},
        [55233]={t=8,c=3,mr=1,r={55233}},
        [55610]={t=6,c=1,mr=1,r={55610}, p={3,1}},
        [55620]={t=4,c=4,mr=2,r={55620,55623}},
        [55666]={t=6,c=1,mr=2,r={55666,55667}},
        [61154]={t=10,c=2,mr=5,r={61154,61155,61156,61157,61158}},
        [62905]={t=8,c=1,mr=2,r={62905,62908}},
        [63560]={t=7,c=4,mr=1,r={63560}, p={6,4}},
        [65661]={t=8,c=1,mr=3,r={65661,66191,66192}},
        [66799]={t=7,c=1,mr=5,r={66799,66814,66815,66816,66817}},
    },
    [7] = { -- Shaman
        [974]={t=9,c=2,mr=1,r={974}},
        [16035]={t=1,c=3,mr=5,r={16035,16105,16106,16107,16108}},
        [16038]={t=2,c=1,mr=3,r={16038,16160,16161}},
        [16039]={t=1,c=2,mr=5,r={16039,16109,16110,16111,16112}},
        [16040]={t=3,c=1,mr=5,r={16040,16113,16114,16115,16116}},
        [16041]={t=5,c=2,mr=1,r={16041}, p={3,2}},
        [16043]={t=1,c=2,mr=2,r={16043,16130}},
        [16086]={t=4,c=1,mr=2,r={16086,16544}},
        [16089]={t=3,c=3,mr=5,r={16089,60184,60185,60187,60188}},
        [16164]={t=3,c=2,mr=1,r={16164}},
        [16166]={t=7,c=2,mr=1,r={16166}, p={5,2}},
        [16173]={t=1,c=3,mr=5,r={16173,16222,16223,16224,16225}},
        [16176]={t=3,c=4,mr=3,r={16176,16235,16240}},
        [16178]={t=6,c=3,mr=5,r={16178,16210,16211,16212,16213}},
        [16179]={t=2,c=3,mr=5,r={16179,16214,16215,16216,16217}},
        [16180]={t=3,c=1,mr=3,r={16180,16196,16198}},
        [16181]={t=3,c=2,mr=3,r={16181,16230,16232}},
        [16182]={t=1,c=2,mr=5,r={16182,16226,16227,16228,16229}},
        [16184]={t=2,c=1,mr=2,r={16184,16209}},
        [16187]={t=4,c=2,mr=3,r={16187,16205,16206}},
        [16188]={t=5,c=3,mr=1,r={16188}},
        [16190]={t=7,c=2,mr=1,r={16190}, p={4,2}},
        [16194]={t=4,c=3,mr=5,r={16194,16218,16219,16220,16221}},
        [16252]={t=4,c=3,mr=5,r={16252,16306,16307,16308,16309}},
        [16254]={t=3,c=4,mr=3,r={16254,16271,16272}},
        [16255]={t=2,c=2,mr=5,r={16255,16302,16303,16304,16305}},
        [16256]={t=4,c=2,mr=5,r={16256,16281,16282,16283,16284}, p={2,2}},
        [16258]={t=2,c=1,mr=2,r={16258,16293}},
        [16259]={t=1,c=1,mr=3,r={16259,16295,52456}},
        [16261]={t=2,c=4,mr=3,r={16261,16290,51881}},
        [16262]={t=2,c=3,mr=2,r={16262,16287}},
        [16266]={t=3,c=1,mr=3,r={16266,29079,29080}},
        [16268]={t=5,c=2,mr=1,r={16268}},
        [16578]={t=6,c=3,mr=5,r={16578,16579,16580,16581,16582}, p={3,3}},
        [17364]={t=7,c=3,mr=1,r={17364}},
        [17485]={t=1,c=3,mr=5,r={17485,17486,17487,17488,17489}},
        [28996]={t=2,c=2,mr=3,r={28996,28997,28998}},
        [28999]={t=5,c=1,mr=2,r={28999,29000}},
        [29062]={t=4,c=4,mr=3,r={29062,29064,29065}},
        [29082]={t=6,c=3,mr=3,r={29082,29084,29086}},
        [29187]={t=2,c=2,mr=3,r={29187,29189,29191}},
        [29192]={t=5,c=1,mr=2,r={29192,29193}},
        [29206]={t=5,c=1,mr=3,r={29206,29205,29202}},
        [30160]={t=2,c=3,mr=3,r={30160,29179,29180}},
        [30664]={t=5,c=4,mr=3,r={30664,30665,30666}},
        [30672]={t=6,c=1,mr=3,r={30672,30673,30674}},
        [30675]={t=8,c=3,mr=3,r={30675,30678,30679}},
        [30706]={t=9,c=2,mr=1,r={30706}},
        [30798]={t=7,c=2,mr=1,r={30798}, p={5,2}},
        [30802]={t=6,c=1,mr=3,r={30802,30808,30809}},
        [30812]={t=9,c=1,mr=3,r={30812,30813,30814}},
        [30816]={t=7,c=1,mr=3,r={30816,30818,30819}, p={7,2}},
        [30823]={t=9,c=2,mr=1,r={30823}},
        [30864]={t=5,c=4,mr=3,r={30864,30865,30866}},
        [30867]={t=8,c=3,mr=3,r={30867,30868,30869}},
        [30872]={t=8,c=2,mr=2,r={30872,30873}},
        [30881]={t=7,c=1,mr=5,r={30881,30883,30884,30885,30886}},
        [43338]={t=3,c=3,mr=1,r={43338}},
        [51466]={t=8,c=2,mr=2,r={51466,51470}, p={7,2}},
        [51474]={t=9,c=1,mr=3,r={51474,51478,51479}},
        [51480]={t=9,c=3,mr=3,r={51480,51481,51482}},
        [51483]={t=7,c=3,mr=3,r={51483,51485,51486}},
        [51490]={t=11,c=2,mr=1,r={51490}},
        [51521]={t=8,c=3,mr=2,r={51521,51522}, p={7,3}},
        [51523]={t=9,c=3,mr=2,r={51523,51524}},
        [51525]={t=8,c=1,mr=3,r={51525,51526,51527}},
        [51528]={t=10,c=2,mr=5,r={51528,51529,51530,51531,51532}},
        [51533]={t=11,c=2,mr=1,r={51533}},
        [51554]={t=8,c=1,mr=2,r={51554,51555}},
        [51556]={t=9,c=1,mr=3,r={51556,51557,51558}},
        [51560]={t=9,c=3,mr=2,r={51560,51561}, p={9,2}},
        [51562]={t=10,c=2,mr=5,r={51562,51563,51564,51565,51566}},
        [51883]={t=5,c=3,mr=3,r={51883,51884,51885}},
        [51886]={t=7,c=3,mr=1,r={51886}, p={6,3}},
        [55198]={t=3,c=3,mr=1,r={55198}},
        [60103]={t=8,c=2,mr=1,r={60103}, p={7,2}},
        [61295]={t=11,c=2,mr=1,r={61295}},
        [62097]={t=10,c=2,mr=5,r={62097,62098,62099,62100,62101}},
        [63370]={t=8,c=1,mr=2,r={63370,63372}},
        [63373]={t=6,c=4,mr=2,r={63373,63374}},
    },
    [8] = { -- Mage
        [11069]={t=1,c=3,mr=5,r={11069,12338,12339,12340,12341}},
        [11070]={t=1,c=2,mr=5,r={11070,12473,16763,16765,16766}},
        [11071]={t=1,c=1,mr=3,r={11071,12496,12497}},
        [11078]={t=1,c=1,mr=2,r={11078,11080}},
        [11083]={t=3,c=4,mr=2,r={11083,12351}},
        [11094]={t=4,c=2,mr=2,r={11094,13043}},
        [11095]={t=4,c=1,mr=3,r={11095,12872,12873}},
        [11100]={t=3,c=1,mr=2,r={11100,12353}},
        [11103]={t=3,c=2,mr=3,r={11103,12357,12358}},
        [11108]={t=2,c=3,mr=3,r={11108,12349,12350}},
        [11113]={t=5,c=3,mr=1,r={11113}, p={3,3}},
        [11115]={t=5,c=2,mr=3,r={11115,11367,11368}},
        [11119]={t=2,c=1,mr=5,r={11119,11120,12846,12847,12848}},
        [11124]={t=6,c=3,mr=5,r={11124,12378,12398,12399,12400}},
        [11129]={t=7,c=2,mr=1,r={11129}, p={5,2}},
        [11151]={t=3,c=1,mr=3,r={11151,12952,12953}},
        [11160]={t=4,c=2,mr=3,r={11160,12518,12519}},
        [11170]={t=4,c=3,mr=3,r={11170,12982,12983}},
        [11175]={t=2,c=4,mr=3,r={11175,12569,12571}},
        [11180]={t=6,c=3,mr=3,r={11180,28592,28593}},
        [11185]={t=3,c=3,mr=3,r={11185,12487,12488}},
        [11189]={t=2,c=2,mr=2,r={11189,28332}},
        [11190]={t=5,c=3,mr=3,r={11190,12489,12490}},
        [11207]={t=2,c=1,mr=3,r={11207,12672,15047}},
        [11210]={t=1,c=1,mr=2,r={11210,12592}},
        [11213]={t=2,c=3,mr=5,r={11213,12574,12575,12576,12577}},
        [11222]={t=1,c=2,mr=3,r={11222,12839,12840}},
        [11232]={t=5,c=4,mr=5,r={11232,12500,12501,12502,12503}},
        [11237]={t=1,c=3,mr=5,r={11237,12463,12464,16769,16770}},
        [11242]={t=3,c=2,mr=3,r={11242,12467,12469}},
        [11247]={t=3,c=1,mr=2,r={11247,12606}},
        [11252]={t=4,c=1,mr=2,r={11252,12605}},
        [11255]={t=4,c=2,mr=2,r={11255,12598}},
        [11366]={t=3,c=3,mr=1,r={11366}},
        [11426]={t=7,c=2,mr=1,r={11426}, p={5,2}},
        [11958]={t=5,c=2,mr=1,r={11958}},
        [12042]={t=7,c=2,mr=1,r={12042}, p={6,2}},
        [12043]={t=5,c=2,mr=1,r={12043}},
        [12472]={t=3,c=2,mr=1,r={12472}},
        [15058]={t=6,c=2,mr=3,r={15058,15059,15060}, p={5,2}},
        [16757]={t=4,c=1,mr=2,r={16757,16758}},
        [18459]={t=1,c=2,mr=3,r={18459,18460,54734}},
        [18462]={t=4,c=3,mr=3,r={18462,18463,18464}},
        [28574]={t=2,c=1,mr=3,r={28574,54658,54659}},
        [29074]={t=4,c=4,mr=3,r={29074,29075,29076}},
        [29438]={t=2,c=3,mr=3,r={29438,29439,29440}},
        [29441]={t=2,c=2,mr=2,r={29441,29444}},
        [29447]={t=4,c=4,mr=3,r={29447,55339,55340}},
        [31569]={t=5,c=1,mr=2,r={31569,31570}},
        [31571]={t=6,c=3,mr=2,r={31571,31572}, p={5,2}},
        [31574]={t=6,c=1,mr=3,r={31574,31575,54354}},
        [31579]={t=7,c=1,mr=3,r={31579,31582,31583}},
        [31584]={t=8,c=3,mr=5,r={31584,31585,31586,31587,31588}},
        [31589]={t=9,c=2,mr=1,r={31589}},
        [31638]={t=5,c=1,mr=3,r={31638,31639,31640}},
        [31641]={t=6,c=1,mr=2,r={31641,31642}},
        [31656]={t=8,c=3,mr=3,r={31656,31657,31658}},
        [31661]={t=9,c=2,mr=1,r={31661}, p={7,2}},
        [31667]={t=5,c=4,mr=3,r={31667,31668,31669}},
        [31670]={t=1,c=3,mr=3,r={31670,31672,55094}},
        [31674]={t=7,c=3,mr=5,r={31674,31675,31676,31677,31678}},
        [31679]={t=7,c=3,mr=2,r={31679,31680}},
        [31682]={t=8,c=2,mr=2,r={31682,31683}},
        [31687]={t=9,c=2,mr=1,r={31687}},
        [34293]={t=7,c=1,mr=3,r={34293,34295,34296}},
        [35578]={t=10,c=3,mr=2,r={35578,35581}},
        [44378]={t=8,c=2,mr=2,r={44378,44379}, p={7,2}},
        [44394]={t=7,c=3,mr=3,r={44394,44395,44396}},
        [44397]={t=3,c=3,mr=3,r={44397,44398,44399}},
        [44400]={t=10,c=2,mr=3,r={44400,44402,44403}},
        [44404]={t=9,c=3,mr=5,r={44404,54486,54488,54489,54490}},
        [44425]={t=11,c=2,mr=1,r={44425}},
        [44442]={t=9,c=1,mr=2,r={44442,44443}, p={9,2}},
        [44445]={t=9,c=3,mr=3,r={44445,44446,44448}},
        [44449]={t=10,c=2,mr=5,r={44449,44469,44470,44471,44472}},
        [44457]={t=11,c=2,mr=1,r={44457}},
        [44543]={t=8,c=3,mr=2,r={44543,44545}},
        [44546]={t=9,c=1,mr=3,r={44546,44548,44549}},
        [44557]={t=9,c=3,mr=3,r={44557,44560,44561}, p={9,2}},
        [44566]={t=10,c=2,mr=5,r={44566,44567,44568,44570,44571}},
        [44572]={t=11,c=2,mr=1,r={44572}},
        [44745]={t=7,c=1,mr=2,r={44745,54787}, p={7,2}},
        [54646]={t=3,c=4,mr=1,r={54646}},
        [54747]={t=2,c=2,mr=2,r={54747,54749}},
        [55091]={t=6,c=1,mr=2,r={55091,55092}, p={5,2}},
        [64353]={t=8,c=1,mr=2,r={64353,64357}},
    },
    [9] = { -- Warlock
        [17778]={t=2,c=3,mr=3,r={17778,17779,17780}},
        [17783]={t=3,c=2,mr=3,r={17783,17784,17785}},
        [17788]={t=1,c=3,mr=5,r={17788,17789,17790,17791,17792}},
        [17793]={t=1,c=2,mr=5,r={17793,17796,17801,17802,17803}},
        [17804]={t=2,c=4,mr=2,r={17804,17805}},
        [17810]={t=1,c=3,mr=5,r={17810,17811,17812,17813,17814}},
        [17815]={t=5,c=2,mr=3,r={17815,17833,17834}},
        [17877]={t=3,c=2,mr=1,r={17877}},
        [17917]={t=4,c=2,mr=2,r={17917,17918}},
        [17927]={t=4,c=4,mr=3,r={17927,17929,17930}},
        [17954]={t=6,c=3,mr=5,r={17954,17955,17956,17957,17958}},
        [17959]={t=3,c=3,mr=5,r={17959,59738,59739,59740,59741}},
        [17962]={t=7,c=2,mr=1,r={17962}, p={5,2}},
        [18094]={t=4,c=2,mr=2,r={18094,18095}},
        [18096]={t=7,c=4,mr=3,r={18096,18073,63245}},
        [18119]={t=2,c=1,mr=2,r={18119,18120}},
        [18126]={t=3,c=1,mr=2,r={18126,18127}},
        [18130]={t=5,c=3,mr=1,r={18130}, p={3,3}},
        [18135]={t=4,c=1,mr=2,r={18135,18136}},
        [18174]={t=1,c=2,mr=3,r={18174,18175,18176}},
        [18179]={t=2,c=1,mr=2,r={18179,18180}},
        [18182]={t=2,c=3,mr=2,r={18182,18183}},
        [18213]={t=2,c=2,mr=2,r={18213,18372}},
        [18218]={t=4,c=1,mr=2,r={18218,18219}},
        [18220]={t=7,c=3,mr=1,r={18220}},
        [18223]={t=5,c=3,mr=1,r={18223}, p={3,3}},
        [18271]={t=6,c=2,mr=5,r={18271,18272,18273,18274,18275}, p={5,2}},
        [18288]={t=3,c=3,mr=1,r={18288}},
        [18692]={t=1,c=1,mr=2,r={18692,18693}},
        [18694]={t=1,c=2,mr=3,r={18694,18695,18696}},
        [18697]={t=1,c=3,mr=3,r={18697,18698,18699}},
        [18703]={t=2,c=1,mr=2,r={18703,18704}},
        [18705]={t=2,c=2,mr=3,r={18705,18706,18707}},
        [18708]={t=3,c=3,mr=1,r={18708}},
        [18709]={t=4,c=3,mr=2,r={18709,18710}, p={3,3}},
        [18731]={t=2,c=3,mr=3,r={18731,18743,18744}},
        [18754]={t=3,c=1,mr=3,r={18754,18755,18756}},
        [18767]={t=5,c=3,mr=2,r={18767,18768}},
        [18769]={t=4,c=2,mr=5,r={18769,18770,18771,18772,18773}, p={3,2}},
        [18827]={t=1,c=1,mr=2,r={18827,18829}},
        [19028]={t=3,c=2,mr=1,r={19028}},
        [23785]={t=6,c=2,mr=5,r={23785,23822,23823,23824,23825}, p={4,2}},
        [30054]={t=8,c=1,mr=2,r={30054,30057}},
        [30060]={t=7,c=2,mr=5,r={30060,30061,30062,30063,30064}},
        [30108]={t=9,c=2,mr=1,r={30108}, p={7,2}},
        [30143]={t=3,c=4,mr=3,r={30143,30144,30145}},
        [30146]={t=9,c=2,mr=1,r={30146}},
        [30242]={t=8,c=2,mr=5,r={30242,30245,30246,30247,30248}},
        [30283]={t=9,c=2,mr=1,r={30283}},
        [30288]={t=8,c=2,mr=5,r={30288,30289,30290,30291,30292}},
        [30293]={t=7,c=3,mr=3,r={30293,30295,30296}},
        [30299]={t=6,c=1,mr=3,r={30299,30301,30302}},
        [30319]={t=7,c=1,mr=3,r={30319,30320,30321}},
        [30326]={t=5,c=1,mr=1,r={30326}, p={4,2}},
        [32381]={t=4,c=4,mr=3,r={32381,32382,32383}},
        [32385]={t=5,c=1,mr=5,r={32385,32387,32392,32393,32394}},
        [32477]={t=8,c=3,mr=3,r={32477,32483,32484}},
        [34935]={t=5,c=1,mr=3,r={34935,34938,34939}, p={4,1}},
        [35691]={t=7,c=3,mr=3,r={35691,35692,35693}},
        [47193]={t=7,c=2,mr=1,r={47193}, p={6,2}},
        [47195]={t=7,c=1,mr=3,r={47195,47196,47197}},
        [47198]={t=9,c=1,mr=3,r={47198,47199,47200}},
        [47201]={t=10,c=2,mr=5,r={47201,47202,47203,47204,47205}},
        [47220]={t=9,c=3,mr=3,r={47220,47221,47223}},
        [47230]={t=1,c=4,mr=2,r={47230,47231}},
        [47236]={t=10,c=2,mr=5,r={47236,47237,47238,47239,47240}},
        [47245]={t=6,c=3,mr=3,r={47245,47246,47247}},
        [47258]={t=9,c=1,mr=3,r={47258,47259,47260}, p={7,2}},
        [47266]={t=10,c=2,mr=5,r={47266,47267,47268,47269,47270}},
        [48181]={t=11,c=2,mr=1,r={48181}},
        [50796]={t=11,c=2,mr=1,r={50796}},
        [53754]={t=3,c=1,mr=2,r={53754,53759}},
        [54037]={t=6,c=1,mr=2,r={54037,54038}},
        [54117]={t=8,c=3,mr=2,r={54117,54118}, p={7,3}},
        [54347]={t=9,c=1,mr=3,r={54347,54348,54349}, p={8,2}},
        [58435]={t=9,c=3,mr=1,r={58435}, p={9,2}},
        [59672]={t=11,c=2,mr=1,r={59672}},
        [63108]={t=5,c=2,mr=1,r={63108}},
        [63117]={t=9,c=3,mr=3,r={63117,63121,63123}},
        [63156]={t=8,c=3,mr=2,r={63156,63158}},
        [63349]={t=2,c=2,mr=3,r={63349,63350,63351}},
    },
    [11] = { -- Druid
        [5570]={t=5,c=2,mr=1,r={5570}},
        [16814]={t=1,c=2,mr=5,r={16814,16815,16816,16817,16818}},
        [16819]={t=3,c=4,mr=2,r={16819,16820}},
        [16821]={t=2,c=4,mr=2,r={16821,16822}},
        [16833]={t=2,c=3,mr=3,r={16833,16834,16835}},
        [16836]={t=3,c=1,mr=3,r={16836,16839,16840}},
        [16845]={t=2,c=1,mr=3,r={16845,16846,16847}},
        [16850]={t=4,c=3,mr=3,r={16850,16923,16924}},
        [16858]={t=1,c=3,mr=5,r={16858,16859,16860,16861,16862}},
        [16864]={t=3,c=2,mr=1,r={16864}},
        [16880]={t=3,c=2,mr=3,r={16880,61345,61346}, p={2,2}},
        [16896]={t=6,c=2,mr=3,r={16896,16897,16899}},
        [16909]={t=4,c=2,mr=5,r={16909,16910,16911,16912,16913}},
        [16929]={t=2,c=3,mr=3,r={16929,16930,16931}},
        [16934]={t=1,c=2,mr=5,r={16934,16935,16936,16937,16938}},
        [16940]={t=5,c=1,mr=2,r={16940,16941}},
        [16942]={t=3,c=3,mr=3,r={16942,16943,16944}},
        [16947]={t=2,c=1,mr=3,r={16947,16948,16949}},
        [16966]={t=4,c=1,mr=2,r={16966,16968}},
        [16972]={t=4,c=2,mr=3,r={16972,16974,16975}},
        [16998]={t=2,c=2,mr=2,r={16998,16999}},
        [17002]={t=3,c=1,mr=2,r={17002,24866}},
        [17003]={t=6,c=2,mr=5,r={17003,17004,17005,17006,24894}, p={4,2}},
        [17007]={t=7,c=2,mr=1,r={17007}},
        [17050]={t=1,c=1,mr=2,r={17050,17051}},
        [17056]={t=1,c=3,mr=5,r={17056,17058,17059,17060,17061}},
        [17063]={t=1,c=2,mr=3,r={17063,17065,17066}},
        [17069]={t=2,c=1,mr=5,r={17069,17070,17071,17072,17073}},
        [17074]={t=6,c=3,mr=5,r={17074,17075,17076,17077,17078}, p={4,3}},
        [17104]={t=5,c=2,mr=5,r={17104,24943,24944,24945,24946}},
        [17106]={t=3,c=1,mr=3,r={17106,17107,17108}},
        [17111]={t=4,c=3,mr=3,r={17111,17112,17113}},
        [17116]={t=5,c=1,mr=1,r={17116}, p={3,1}},
        [17118]={t=2,c=2,mr=3,r={17118,17119,17120}},
        [17123]={t=5,c=4,mr=2,r={17123,17124}},
        [18562]={t=7,c=2,mr=1,r={18562}, p={5,2}},
        [24858]={t=7,c=2,mr=1,r={24858}},
        [24968]={t=4,c=2,mr=5,r={24968,24969,24970,24971,24972}},
        [33589]={t=5,c=1,mr=3,r={33589,33590,33591}},
        [33592]={t=6,c=3,mr=2,r={33592,33596}},
        [33597]={t=6,c=1,mr=3,r={33597,33599,33956}},
        [33600]={t=7,c=4,mr=3,r={33600,33601,33602}},
        [33603]={t=8,c=3,mr=5,r={33603,33604,33605,33606,33607}},
        [33831]={t=9,c=3,mr=1,r={33831}},
        [33851]={t=7,c=4,mr=3,r={33851,33852,33957}},
        [33853]={t=6,c=3,mr=3,r={33853,33855,33856}},
        [33859]={t=8,c=3,mr=3,r={33859,33866,33867}},
        [33872]={t=5,c=4,mr=2,r={33872,33873}},
        [33879]={t=6,c=1,mr=2,r={33879,33880}},
        [33881]={t=7,c=3,mr=3,r={33881,33882,33883}},
        [33886]={t=8,c=2,mr=5,r={33886,33887,33888,33889,33890}},
        [33917]={t=9,c=2,mr=1,r={33917}, p={7,2}},
        [34151]={t=7,c=1,mr=3,r={34151,34152,34153}},
        [34297]={t=7,c=3,mr=2,r={34297,34300}, p={7,2}},
        [35363]={t=2,c=2,mr=2,r={35363,35364}},
        [37116]={t=4,c=3,mr=2,r={37116,37117}, p={3,3}},
        [48384]={t=7,c=3,mr=3,r={48384,48395,48396}, p={7,2}},
        [48389]={t=8,c=1,mr=3,r={48389,48392,48393}, p={7,2}},
        [48409]={t=4,c=4,mr=2,r={48409,48410}, p={3,3}},
        [48411]={t=3,c=3,mr=2,r={48411,48412}, p={2,3}},
        [48432]={t=10,c=2,mr=5,r={48432,48433,48434,51268,51269}},
        [48438]={t=11,c=2,mr=1,r={48438}, p={9,2}},
        [48483]={t=8,c=4,mr=3,r={48483,48484,48485}},
        [48488]={t=9,c=4,mr=2,r={48488,48514}},
        [48492]={t=9,c=1,mr=3,r={48492,48494,48495}},
        [48496]={t=8,c=3,mr=3,r={48496,48499,48500}},
        [48505]={t=11,c=2,mr=1,r={48505}},
        [48506]={t=10,c=2,mr=3,r={48506,48510,48511}},
        [48516]={t=9,c=1,mr=3,r={48516,48521,48525}},
        [48532]={t=9,c=3,mr=3,r={48532,48489,48491}, p={9,2}},
        [48535]={t=9,c=3,mr=3,r={48535,48536,48537}, p={9,2}},
        [48539]={t=9,c=1,mr=3,r={48539,48544,48545}},
        [49377]={t=5,c=3,mr=1,r={49377}},
        [50334]={t=11,c=2,mr=1,r={50334}},
        [50516]={t=9,c=2,mr=1,r={50516}, p={7,2}},
        [51179]={t=10,c=3,mr=5,r={51179,51180,51181,51182,51183}},
        [57810]={t=1,c=3,mr=5,r={57810,57811,57812,57813,57814}},
        [57849]={t=5,c=3,mr=3,r={57849,57850,57851}, p={5,2}},
        [57865]={t=3,c=3,mr=1,r={57865}, p={2,2}},
        [57873]={t=8,c=1,mr=3,r={57873,57876,57877}, p={7,2}},
        [57878]={t=6,c=1,mr=3,r={57878,57880,57881}},
        [61336]={t=3,c=2,mr=1,r={61336}},
        [63410]={t=10,c=1,mr=2,r={63410,63411}},
        [63503]={t=10,c=3,mr=1,r={63503}, p={10,2}},
        [65139]={t=9,c=2,mr=1,r={65139}, p={8,2}},
    },
}
WU_TALENT_DATA_EOF

    print_success "Cross-class talent bridge staged (server side)."
}

# ============================================================
#  stage_summons_module()   (added v1.3.0)
#
#  Multi-class SUMMONS C++ module (mod-multiclass-summons). Fixes
#  Warlock/Mage/Death-Knight pet + mount conflicts for multiclass
#  characters and lets eligible classes field multiple guardians at
#  once. Playerbots are excluded at runtime (no compile-time bot dep).
#
#  Compiles into the worldserver on the rebuild step (AzerothCore
#  auto-discovers modules/*/CMakeLists.txt). Spell-script rows are
#  applied here AND auto-applied from the module base path on startup.
#  Idempotent; overwrites only this module's own files.
# ============================================================
stage_summons_module() {
    print_step "Staging the multi-class summons module (C++)..."

    local MOD="$SERVER_DIR/modules/mod-multiclass-summons"
    mkdir -p "$MOD/src" "$MOD/data/sql/db-world/base"

    cat > "$MOD/CMakeLists.txt" <<'WU_SUMMONS_CMAKE_EOF'
AC_ADD_SCRIPT("${CMAKE_CURRENT_SOURCE_DIR}/src/multiclass_pet_fix.cpp")
AC_ADD_SCRIPT("${CMAKE_CURRENT_SOURCE_DIR}/src/mod_multiclass_summons_loader.cpp")
WU_SUMMONS_CMAKE_EOF

    cat > "$MOD/src/multiclass_pet_fix_loader.h" <<'WU_SUMMONS_LOADER_H_EOF'
#ifndef MULTICLASS_PET_FIX_LOADER_H
#define MULTICLASS_PET_FIX_LOADER_H

void AddMulticlassPetFixScripts();

#endif
WU_SUMMONS_LOADER_H_EOF

    cat > "$MOD/src/mod_multiclass_summons_loader.cpp" <<'WU_SUMMONS_LOADER_CPP_EOF'
#include "Log.h"

void AddMulticlassPetFixScripts();

void Addmod_multiclass_summonsScripts()
{
    LOG_ERROR("module.multiclass_pet_fix", "SUMMONS MODULE - Addmod_multiclass_summonsScripts loader called!");
    AddMulticlassPetFixScripts();
}
WU_SUMMONS_LOADER_CPP_EOF

    cat > "$MOD/src/multiclass_pet_fix.cpp" <<'WU_SUMMONS_FIX_CPP_EOF'
#include "ScriptMgr.h"
#include "Player.h"
#include "Pet.h"
#include "ObjectAccessor.h"
#include "ObjectMgr.h"
#include "SpellScript.h"
#include "SpellInfo.h"
#include "SpellMgr.h"
#include "CharmInfo.h"
#include "DBCStores.h"
#include "Map.h"
#include "TemporarySummon.h"
#include "WorldSession.h"
#include <algorithm>
#include <map>
#include <numbers>
#include <unordered_map>
#include <vector>

// mod-multiclass-summons — module-managed multi-summon system (pure guardian model).
//
// Every target summon (warlock demons, mage Water Elemental, DK ghoul) is created as a
// controllable GUARDIAN that the module fully owns — never a real Pet, so nothing is
// written to character_pet and the core's single-class pet checks / mount stash logic
// are bypassed entirely (this avoids the real-pet mount/dismount crashes).
//
//   Primary   (cast while the pet slot is free): Category PET guardian -> claims the pet
//             slot, so the client shows the pet action bar/frame and the player controls it.
//   Secondary (slot already held): Category ALLY guardian -> side minion, no bar, follows,
//             joins combat, auto-casts its attack.
//
// Guardians normally only know their creature-template spells, so we inject each summon's
// real pet ability set (correct spell IDs + level-appropriate ranks, sourced from the
// same PetLevelupSpell/PetDefaultSpells data the pet system uses) and default them to
// REACT_DEFENSIVE. Summons are session-only. Playerbots are skipped (stock single pet).

namespace
{
    // Summon spells this module intercepts. Keep in sync with
    // data/sql/db-world/base/multiclass_summons.sql.
    bool IsMulticlassSummonSpell(uint32 spellId)
    {
        switch (spellId)
        {
            case 688:   // Summon Imp
            case 697:   // Summon Voidwalker
            case 712:   // Summon Succubus
            case 691:   // Summon Felhunter
            case 30146: // Summon Felguard
            case 70907: // Summon Water Elemental (Temp)
            case 70908: // Summon Water Elemental (Perm)
            case 46584: // Raise Dead (Temp Ghoul)
            case 52150: // Raise Dead (Perm Ghoul)
                return true;
            default:
                return false;
        }
    }

    template <typename T>
    bool IsPlayerBotHelper(T const* player)
    {
        auto* session = player->GetSession();
        if (!session)
            return false;

        if constexpr (requires { session->IsBot(); })
        {
            return session->IsBot();
        }
        else
        {
            return false;
        }
    }

    bool IsPlayerBot(Player const* player)
    {
        return IsPlayerBotHelper(player);
    }

    SummonPropertiesEntry MakeProps(uint32 category, uint32 type)
    {
        SummonPropertiesEntry props{};
        props.Id = 67;
        props.Category = category;
        props.Faction = 0;
        props.Type = type;
        props.Slot = 0;
        props.Flags = 0;
        return props;
    }

    // A follow angle (relative to the owner's facing) that spreads summons evenly around
    // the owner instead of stacking them all on the default PET_FOLLOW_ANGLE.
    float FollowAngleForIndex(std::size_t index)
    {
        constexpr float step = 2.0f * std::numbers::pi_v<float> / 8.0f;
        return PET_FOLLOW_ANGLE + step * float(index % 8);
    }

    // Give a guardian the ability set the matching pet would have at the owner's level,
    // by writing the spell ids into the creature's spell slots (read by
    // CharmInfo::InitCharmCreateSpells, which runs later in Guardian::InitStats to build
    // the action bar + autocast). Active abilities go first so they take the bar's
    // castable slots; passives follow and are cast on init. Sourced from the same
    // PetLevelupSpell (per family) + PetDefaultSpells data the real pet system uses, so
    // ids and ranks are correct without hardcoding. No-op if no pet data exists for the
    // creature (leaves its template spells intact).
    void ApplyPetAbilities(Creature* guardian, uint8 level)
    {
        CreatureTemplate const* cinfo = guardian->GetCreatureTemplate();
        if (!cinfo)
            return;

        // first-rank-in-chain -> { highest required level <= owner level, that rank's id }
        std::map<uint32, std::pair<uint32, uint32>> best;

        auto consider = [&](uint32 spellId, uint32 reqLevel)
        {
            if (!spellId || reqLevel > level || !sSpellMgr->GetSpellInfo(spellId))
                return;

            uint32 const first = sSpellMgr->GetFirstSpellInChain(spellId);
            auto itr = best.find(first);
            if (itr == best.end() || reqLevel >= itr->second.first)
                best[first] = { reqLevel, spellId };
        };

        // 1) Keep the creature's own template spells (e.g. Water Elemental's Waterbolt +
        //    Freeze, Ghoul's Claw/Gnaw/Leap/Huddle). reqLevel 0 = always eligible; a
        //    proper pet-data rank below overrides them for shared spell chains.
        for (uint8 i = 0; i < MAX_CREATURE_SPELLS; ++i)
            consider(guardian->m_spells[i], 0);

        // 2) Add the matching pet's family level-up spells (correct rank for this level)
        //    — this is what fills in the warlock-demon kits the template doesn't carry.
        if (cinfo->family)
            if (PetLevelupSpellSet const* levelup = sSpellMgr->GetPetLevelupSpellList(cinfo->family))
                for (auto const& entry : *levelup)
                    consider(entry.second, entry.first);

        int32 const petSpellsId = cinfo->PetSpellDataId ? -(int32)cinfo->PetSpellDataId : (int32)guardian->GetEntry();
        if (PetDefaultSpellsEntry const* def = sSpellMgr->GetPetDefaultSpellsEntry(petSpellsId))
            for (uint32 spellId : def->spellid)
                if (SpellInfo const* info = sSpellMgr->GetSpellInfo(spellId))
                    consider(spellId, info->SpellLevel);

        if (best.empty())
            return;

        uint8 slot = 0;
        for (auto const& kv : best) // active abilities first
        {
            if (slot >= MAX_SPELL_CHARM)
                break;
            SpellInfo const* info = sSpellMgr->GetSpellInfo(kv.second.second);
            if (info && !info->IsPassive())
                guardian->m_spells[slot++] = kv.second.second;
        }
        for (auto const& kv : best) // then passives
        {
            if (slot >= MAX_SPELL_CHARM)
                break;
            SpellInfo const* info = sSpellMgr->GetSpellInfo(kv.second.second);
            if (info && info->IsPassive())
                guardian->m_spells[slot++] = kv.second.second;
        }
        for (; slot < MAX_SPELL_CHARM; ++slot)
            guardian->m_spells[slot] = 0;
    }

    struct ActiveSummon
    {
        ObjectGuid guid;
        uint32 entry;
        uint32 spellId;
        int32 duration;
        bool primary;
    };

    struct PlayerSummons
    {
        std::vector<ActiveSummon> list;
        uint32 reconcileTimer = 0;
    };

    // Owns every module summon for every (non-bot) player. World-thread only, so a plain
    // map needs no locking.
    class SummonManager
    {
    public:
        static SummonManager& Instance()
        {
            static SummonManager instance;
            return instance;
        }

        // A target summon spell was cast: create it as primary (pet slot free) or as a
        // side guardian, one active summon per creature entry.
        void HandleCast(Player* owner, uint32 spellId, uint32 entry, int32 duration)
        {
            // Re-casting the entry that already holds the pet slot: leave it in place
            // (no duplicate). Covers recasting your current primary.
            if (Creature* prim = ObjectAccessor::GetCreatureOrPetOrVehicle(*owner, owner->GetPetGUID()))
                if (prim->GetEntry() == entry)
                    return;

            PlayerSummons& ps = _players[owner->GetGUID()];
            RemoveEntry(owner, ps, entry);

            bool const primary = owner->GetPetGUID().IsEmpty();
            float const followAngle = FollowAngleForIndex(ps.list.size());

            if (TempSummon* summon = CreateGuardian(owner, entry, spellId, duration, primary, followAngle))
            {
                ps.list.push_back({ summon->GetGUID(), entry, spellId, duration, primary });
                LOG_INFO("module.multiclass_pet_fix", "Summon: {} entry {} (spell {}) as {} for {}",
                    summon->GetGUID().ToString(), entry, spellId, primary ? "PRIMARY" : "guardian", owner->GetName());
            }
            else if (ps.list.empty())
                _players.erase(owner->GetGUID());
        }

        // Throttled per-player reconcile: prune dead summons and promote a new primary if
        // the slot frees up.
        void Update(Player* owner, uint32 diff)
        {
            auto it = _players.find(owner->GetGUID());
            if (it == _players.end())
                return;

            PlayerSummons& ps = it->second;
            ps.reconcileTimer += diff;
            if (ps.reconcileTimer < RECONCILE_INTERVAL)
                return;
            ps.reconcileTimer = 0;

            Reconcile(owner, ps);

            if (ps.list.empty())
                _players.erase(it);
        }

        // Session-only: drop the registry (and despawn the summons) when the player leaves.
        void Clear(Player* owner)
        {
            auto it = _players.find(owner->GetGUID());
            if (it == _players.end())
                return;

            for (ActiveSummon const& summon : it->second.list)
                Unsummon(owner, summon.guid);

            _players.erase(it);
        }

    private:
        static constexpr uint32 RECONCILE_INTERVAL = 1000;

        std::unordered_map<ObjectGuid, PlayerSummons> _players;

        TempSummon* CreateGuardian(Player* owner, uint32 entry, uint32 spellId, int32 duration, bool primary,
            float followAngle)
        {
            static SummonPropertiesEntry const primaryProps = MakeProps(SUMMON_CATEGORY_PET, SUMMON_TYPE_PET);
            static SummonPropertiesEntry const secondaryProps = MakeProps(SUMMON_CATEGORY_ALLY, SUMMON_TYPE_GUARDIAN);

            // Spawn at the summon's follow position (out at pet range, at its own angle)
            // so they appear spread out rather than on top of each other.
            float x, y, z;
            owner->GetClosePoint(x, y, z, owner->GetObjectSize(), PET_FOLLOW_DIST, followAngle);

            TempSummon* summon = owner->GetMap()->SummonCreature(entry,
                Position(x, y, z, owner->GetOrientation()),
                primary ? &primaryProps : &secondaryProps,
                duration, owner, spellId);
            if (!summon)
                return nullptr;

            // Keep the summon following at its own angle around the owner.
            static_cast<Minion*>(summon)->SetFollowAngle(followAngle);

            if (std::string name = sObjectMgr->GeneratePetName(entry); !name.empty())
                summon->SetName(name);

            // Default to defensive: engage when the owner is attacked / attacks, rather
            // than pulling on sight (Guardian::InitStats forces aggressive). Guardian
            // also sent the pet bar (in InitSummon) with the old state, so for the
            // primary re-send it now that the react state is defensive.
            summon->SetReactState(REACT_DEFENSIVE);
            if (primary)
                owner->CharmSpellInitialize();

            return summon;
        }

        void Unsummon(Player* owner, ObjectGuid guid)
        {
            if (Creature* creature = ObjectAccessor::GetCreature(*owner, guid))
                if (TempSummon* summon = creature->ToTempSummon())
                    summon->UnSummon();
        }

        void RemoveEntry(Player* owner, PlayerSummons& ps, uint32 entry)
        {
            for (ActiveSummon const& summon : ps.list)
                if (summon.entry == entry)
                    Unsummon(owner, summon.guid);

            ps.list.erase(std::remove_if(ps.list.begin(), ps.list.end(),
                [entry](ActiveSummon const& summon) { return summon.entry == entry; }),
                ps.list.end());
        }

        void Reconcile(Player* owner, PlayerSummons& ps)
        {
            // Drop summons that have died or despawned.
            ps.list.erase(std::remove_if(ps.list.begin(), ps.list.end(),
                [owner](ActiveSummon const& summon)
                {
                    Creature* creature = ObjectAccessor::GetCreature(*owner, summon.guid);
                    return !creature || !creature->IsAlive();
                }),
                ps.list.end());

            if (ps.list.empty())
                return;

            // While mounted / in flight the pet slot can be transiently empty (the core
            // stashes a real pet, or a guardian primary persists separately). Never
            // promote in that window.
            if (owner->IsMounted() || owner->GetTemporaryUnsummonedPetNumber())
                return;

            // If the pet slot is occupied there is nothing to promote.
            if (!owner->GetPetGUID().IsEmpty())
                return;

            // Promote the oldest remaining summon to primary by re-spawning it as a
            // pet-slot guardian.
            ActiveSummon const promote = ps.list.front();
            Unsummon(owner, promote.guid);
            ps.list.erase(ps.list.begin());

            float const followAngle = FollowAngleForIndex(ps.list.size());

            if (TempSummon* summon = CreateGuardian(owner, promote.entry, promote.spellId, promote.duration,
                true, followAngle))
            {
                ps.list.push_back({ summon->GetGUID(), promote.entry, promote.spellId, promote.duration, true });
                LOG_INFO("module.multiclass_pet_fix", "Promoted entry {} (spell {}) to primary for {}",
                    promote.entry, promote.spellId, owner->GetName());
            }
        }
    };
}

class MulticlassPetFixPlayerScript : public PlayerScript
{
public:
    MulticlassPetFixPlayerScript() : PlayerScript("MulticlassPetFixPlayerScript",
    {
        PLAYERHOOK_ON_BEFORE_LOAD_PET_FROM_DB,
        PLAYERHOOK_ON_BEFORE_GUARDIAN_INIT_STATS_FOR_LEVEL,
        PLAYERHOOK_ON_BEFORE_TEMP_SUMMON_INIT_STATS,
        PLAYERHOOK_ON_PLAYER_IS_CLASS,
        PLAYERHOOK_ON_UPDATE,
        PLAYERHOOK_ON_LOGOUT
    }) { }

    // Real-pet support (hunter pets on multiclass characters): bypass the Death Knight
    // pet exception for non-undead pets loaded from character_pet. Module summons are
    // guardians and never travel this path.
    void OnPlayerBeforeLoadPetFromDB(Player* player, uint32& /*petentry*/, uint32& petnumber, bool& current, bool& forceLoadFromDB) override
    {
        PetStable* petStable = player->GetPetStable();
        if (!petStable)
            return;

        PetStable::PetInfo const* petInfo = nullptr;
        if (petnumber)
        {
            if (petStable->CurrentPet && petStable->CurrentPet->PetNumber == petnumber)
                petInfo = &petStable->CurrentPet.value();
            else
            {
                for (auto const& info : petStable->UnslottedPets)
                {
                    if (info.PetNumber == petnumber)
                    {
                        petInfo = &info;
                        break;
                    }
                }
            }
        }
        else if (current)
        {
            if (petStable->CurrentPet)
                petInfo = &petStable->CurrentPet.value();
        }

        if (petInfo)
        {
            CreatureTemplate const* creatureInfo = sObjectMgr->GetCreatureTemplate(petInfo->CreatureId);
            if (creatureInfo && creatureInfo->type != CREATURE_TYPE_UNDEAD)
            {
                // Force load from DB to bypass the DK pet exception check for all non-DK pets.
                forceLoadFromDB = true;
            }
        }
    }

    void OnPlayerBeforeGuardianInitStatsForLevel(Player* /*player*/, Guardian* guardian, CreatureTemplate const* /*cinfo*/, PetType& petType) override
    {
        if (guardian->IsPet())
        {
            if (petType == MAX_PET_TYPE)
            {
                petType = guardian->ToPet()->getPetType();
            }
        }
    }

    // Pet-context class identity for multiclass characters: if a character has learned
    // another class's pet-summon spell, treat them as that class for PET-ONLY checks.
    // Strictly gated on HasSpell + CLASS_CONTEXT_PET, so it never fires for a character
    // that lacks the spell and defers to the real class everywhere else.
    Optional<bool> OnPlayerIsClass(Player const* player, Classes playerClass, ClassContext context) override
    {
        if (context != CLASS_CONTEXT_PET)
            return std::nullopt;

        switch (playerClass)
        {
            case CLASS_WARLOCK:
                if (player->HasSpell(688) || player->HasSpell(697) || player->HasSpell(712) ||
                    player->HasSpell(691) || player->HasSpell(30146))
                    return true;
                break;
            case CLASS_MAGE:
                if (player->HasSpell(31687))
                    return true;
                break;
            case CLASS_DEATH_KNIGHT:
                if (player->HasSpell(46584))
                    return true;
                break;
            case CLASS_HUNTER:
                if (player->HasSpell(883))
                    return true;
                break;
            default:
                break;
        }

        return std::nullopt;
    }

    // Flag module summon guardians controllable and inject their pet ability set during
    // InitStats (before AddToWorld -> AIM_Initialize and before Guardian::InitStats builds
    // the action bar), so PetAI is selected and the abilities land on the bar + autocast.
    void OnPlayerBeforeTempSummonInitStats(Player* player, TempSummon* tempSummon, uint32& /*duration*/) override
    {
        if (IsPlayerBot(player))
            return;

        if (!tempSummon->IsGuardian())
            return;

        Guardian* guardian = static_cast<Guardian*>(tempSummon);
        if (!IsMulticlassSummonSpell(guardian->GetUInt32Value(UNIT_CREATED_BY_SPELL)))
            return;

        // NOTE: Do NOT call AIM_Initialize() here — AddToWorld() does it, and the mask
        // below ensures PetAI is picked at that point.
        guardian->AddUnitTypeMask(UNIT_MASK_CONTROLLABLE_GUARDIAN);
        guardian->InitCharmInfo();
        ApplyPetAbilities(guardian, player->GetLevel());
    }

    void OnPlayerUpdate(Player* player, uint32 diff) override
    {
        if (IsPlayerBot(player))
            return;

        SummonManager::Instance().Update(player, diff);
    }

    void OnPlayerLogout(Player* player) override
    {
        SummonManager::Instance().Clear(player);
    }
};

class SpellSummonPetOverrideScript : public SpellScript
{
    PrepareSpellScript(SpellSummonPetOverrideScript);

    void HandleSummon(SpellEffIndex effIndex)
    {
        Player* owner = GetCaster()->ToPlayer();
        if (!owner)
            return;

        // Leave playerbots on stock single-pet behaviour (their AI relies on GetPet()).
        if (IsPlayerBot(owner))
            return;

        uint32 const entry = GetSpellInfo()->Effects[effIndex].MiscValue;
        if (!entry)
            return;

        // The module owns every one of these summons — never run the default (real-pet)
        // effect, which would dismiss the active pet.
        PreventHitDefaultEffect(effIndex);

        int32 duration = GetSpellInfo()->GetDuration();
        if (Player* modOwner = owner->GetSpellModOwner())
            modOwner->ApplySpellMod(GetSpellInfo()->Id, SPELLMOD_DURATION, duration);

        SummonManager::Instance().HandleCast(owner, GetSpellInfo()->Id, entry, duration);
    }

    void Register() override
    {
        OnEffectHit += SpellEffectFn(SpellSummonPetOverrideScript::HandleSummon, EFFECT_0, SPELL_EFFECT_SUMMON_PET);
        OnEffectHit += SpellEffectFn(SpellSummonPetOverrideScript::HandleSummon, EFFECT_1, SPELL_EFFECT_SUMMON_PET);
        OnEffectHit += SpellEffectFn(SpellSummonPetOverrideScript::HandleSummon, EFFECT_2, SPELL_EFFECT_SUMMON_PET);

        OnEffectHit += SpellEffectFn(SpellSummonPetOverrideScript::HandleSummon, EFFECT_0, SPELL_EFFECT_SUMMON);
        OnEffectHit += SpellEffectFn(SpellSummonPetOverrideScript::HandleSummon, EFFECT_1, SPELL_EFFECT_SUMMON);
        OnEffectHit += SpellEffectFn(SpellSummonPetOverrideScript::HandleSummon, EFFECT_2, SPELL_EFFECT_SUMMON);
    }
};

class SpellSummonPetOverrideLoader : public SpellScriptLoader
{
public:
    SpellSummonPetOverrideLoader() : SpellScriptLoader("spell_summon_pet_override") { }

    SpellScript* GetSpellScript() const override
    {
        return new SpellSummonPetOverrideScript();
    }
};

class MulticlassSummonWorldScript : public WorldScript
{
public:
    MulticlassSummonWorldScript() : WorldScript("MulticlassSummonWorldScript") { }

    // Allow these summons to be cast while another pet is already active. Without
    // SPELL_ATTR1_DISMISS_PET_FIRST, Spell::CheckCast rejects a SUMMON_PET (or a
    // pet-category SUMMON, e.g. the temporary Water Elemental) with ALREADY_HAVE_SUMMON
    // when the caster has a pet, so the (often triggered) Water Elemental / permanent
    // ghoul summon silently fails. We intercept the effect and spawn a side guardian, so
    // the "dismiss first" semantics never actually run. Warlock demons already carry this
    // attribute; setting it again is a no-op. Runs after spells are loaded.
    void OnStartup() override
    {
        static constexpr uint32 spells[] = { 688, 697, 712, 691, 30146, 70907, 70908, 46584, 52150 };
        for (uint32 id : spells)
            if (SpellInfo const* info = sSpellMgr->GetSpellInfo(id))
                const_cast<SpellInfo*>(info)->AttributesEx |= SPELL_ATTR1_DISMISS_PET_FIRST;
    }
};

void AddMulticlassPetFixScripts()
{
    new MulticlassPetFixPlayerScript();
    new SpellSummonPetOverrideLoader();
    new MulticlassSummonWorldScript();

    // NOTE: spell_script_names registration is handled by
    // data/sql/db-world/base/multiclass_summons.sql, which the DBUpdater auto-applies
    // during database loading at startup, BEFORE LoadSpellScriptNames().
}
WU_SUMMONS_FIX_CPP_EOF

    cat > "$MOD/data/sql/db-world/base/multiclass_summons.sql" <<'WU_SUMMONS_SQL_EOF'
-- mod-multiclass-summons: Register the summon-override spell script.
--
-- This file lives under modules/<module>/data/sql/db-world/, which is the only
-- module SQL location AzerothCore's DBUpdater auto-applies (UpdateFetcher.cpp).
-- It runs during database loading at startup, BEFORE the world calls
-- LoadSpellScriptNames(), so these rows are present when the spell system caches
-- the table. Do NOT register these from C++ at runtime: that executes AFTER
-- LoadSpellScriptNames() and is only picked up on the next restart.

DELETE FROM `spell_script_names` WHERE `ScriptName` = 'spell_summon_pet_override';
INSERT INTO `spell_script_names` (`spell_id`, `ScriptName`) VALUES
(688, 'spell_summon_pet_override'),     -- Summon Imp
(697, 'spell_summon_pet_override'),     -- Summon Voidwalker
(712, 'spell_summon_pet_override'),     -- Summon Succubus
(691, 'spell_summon_pet_override'),     -- Summon Felhunter
(30146, 'spell_summon_pet_override'),   -- Summon Felguard
(70907, 'spell_summon_pet_override'),   -- Summon Water Elemental (Temp)
(70908, 'spell_summon_pet_override'),   -- Summon Water Elemental (Perm)
(46584, 'spell_summon_pet_override'),   -- Raise Dead (Temp Ghoul)
(52150, 'spell_summon_pet_override');   -- Raise Dead (Perm Ghoul)
WU_SUMMONS_SQL_EOF

    # Pre-apply the summon spell-script rows so the rebuilt worldserver
    # registers them on its next startup (module base/ SQL also auto-applies).
    # Non-fatal: auto-apply is the backstop.
    if docker exec -i ac-database mysql -u root -ppassword acore_world \
         < "$MOD/data/sql/db-world/base/multiclass_summons.sql" 2>/dev/null; then
        print_success "Multi-class summons module staged + spell-scripts registered."
    else
        print_warning "Summons module staged; spell-script SQL will auto-apply on startup."
    fi
}

stage_mod_ale() {
    print_step "Checking for the Eluna/ALE Lua engine module (mod-ale)..."

    local MOD_ALE_DIR="$SERVER_DIR/modules/mod-ale"

    # CMakeLists.txt at the module root is what makes this a real, buildable
    # AzerothCore module (every entry in modules/ has one — see mod-unbound,
    # mod-playerbots, etc.). Checking for it (not just "directory exists and
    # is non-empty") matters because git populates .git with objects BEFORE
    # checking out the working tree — an interrupted clone (dropped Wi-Fi,
    # etc.) can leave a non-empty modules/mod-ale/ with a .git folder but no
    # actual module files. A bare non-empty check would call that "already
    # staged" forever, print success, and leave Eluna uncompiled with no
    # further warning.
    if [ -f "$MOD_ALE_DIR/CMakeLists.txt" ]; then
        print_success "mod-ale already present in modules/ — skipping."
        echo ""
        return
    fi

    if [ -d "$MOD_ALE_DIR" ]; then
        print_warning "Found an incomplete modules/mod-ale/ (no CMakeLists.txt) —"
        print_warning "likely an interrupted clone from a previous run. Removing it"
        print_warning "and cloning fresh."
        rm -rf "$MOD_ALE_DIR"
    fi

    if ! command -v git >/dev/null 2>&1; then
        print_warning "git is not available — cannot stage mod-ale automatically."
        echo "Without it, Eluna never compiles into the worldserver and"
        echo "unbound_mentor.lua will never load. Install git, then run:"
        echo -e "${CYAN}   git clone https://github.com/azerothcore/mod-ale.git $MOD_ALE_DIR${NC}"
        echo -e "${CYAN}   cd $MOD_ALE_DIR && git checkout $MOD_ALE_COMMIT${NC}"
        echo "Then re-run this installer."
        exit 1
    fi

    echo "This server's worldserver doesn't have Eluna (the Lua engine mod-unbound"
    echo "depends on) compiled in yet. Staging the official azerothcore/mod-ale"
    echo "module now so it gets built in during the rebuild below."
    echo ""

    if ! git clone https://github.com/azerothcore/mod-ale.git "$MOD_ALE_DIR"; then
        print_warning "Failed to clone mod-ale (network issue, or modules/ isn't writable)."
        echo "Clone it manually and re-run this installer:"
        echo -e "${CYAN}   git clone https://github.com/azerothcore/mod-ale.git $MOD_ALE_DIR${NC}"
        echo -e "${CYAN}   cd $MOD_ALE_DIR && git checkout $MOD_ALE_COMMIT${NC}"
        exit 1
    fi

    if ! (cd "$MOD_ALE_DIR" && git checkout --quiet "$MOD_ALE_COMMIT"); then
        print_warning "Cloned mod-ale but couldn't check out the pinned commit ($MOD_ALE_COMMIT)."
        echo "Continuing with whatever was checked out by default — this may or"
        echo "may not match the version Wrath Unbound was tested against."
    fi

    # Belt and suspenders: confirm the working tree actually has the module,
    # not just that git exited 0 (e.g. disk-full mid-checkout).
    if [ ! -f "$MOD_ALE_DIR/CMakeLists.txt" ]; then
        print_warning "mod-ale was cloned but CMakeLists.txt is missing — the checkout"
        print_warning "looks incomplete (disk space?). Re-run this installer, or fix"
        print_warning "it manually:"
        echo -e "${CYAN}   rm -rf $MOD_ALE_DIR${NC}"
        echo -e "${CYAN}   git clone https://github.com/azerothcore/mod-ale.git $MOD_ALE_DIR${NC}"
        echo -e "${CYAN}   cd $MOD_ALE_DIR && git checkout $MOD_ALE_COMMIT${NC}"
        exit 1
    fi

    print_success "Staged mod-ale (Eluna/ALE) — will be compiled in during the rebuild below."
    echo ""
}

# ============================================================
#  apply_sql_migrations()
#
#  Runs the 14 SQL migrations against the right database, in order.
#  AzerothCore's built-in DB updater does NOT auto-apply files from
#  modules/<mod>/data/sql/** on this build (verified live — only
#  01_unbound_world.sql ended up tracked in the `updates` table, the
#  rest were applied by hand during dev), so we pipe each file into
#  mysql directly. All 14 are confirmed safe to re-run (idempotent —
#  INSERT IGNORE / ON DUPLICATE KEY UPDATE / CREATE TABLE IF NOT EXISTS
#  / information_schema guards), so this is safe on upgrade runs too.
# ============================================================
apply_sql_migrations() {
    print_step "Applying Wrath Unbound SQL migrations..."

    local MODULE_SQL="$SERVER_DIR/modules/mod-unbound/data/sql"
    local FAILED=0

    local DB_WORLD_FILES=(
        "01_unbound_world.sql"
        "02_fix_catalog_req_level.sql"
        "03_creation_gift_spells.sql"
        "04_catalog_druid_forms.sql"
        "05_individual_purchase_prereqs.sql"
        "06_universal_skill_access.sql"
        "07_mentor_stone.sql"
        "08_catalog_additions.sql"
        "10_catalog_audit_fixes.sql"
        "11_catalog_gap_additions.sql"
        "12_mount_spell_fix.sql"
        "13_flight_form_fix.sql"
        "14_judgement_fix.sql"
    )

    local FILE ERRMSG
    for FILE in "${DB_WORLD_FILES[@]}"; do
        echo -e "${CYAN}   Applying db-world/${FILE}...${NC}"
        ERRMSG=$(docker exec -i ac-database mysql -u root -ppassword acore_world \
                < "$MODULE_SQL/db-world/$FILE" 2>&1 >/dev/null)
        if [ $? -ne 0 ]; then
            print_warning "Failed to apply db-world/${FILE}"
            [ -n "$ERRMSG" ] && echo "   MySQL error: $ERRMSG"
            FAILED=1
        fi
    done

    echo -e "${CYAN}   Applying db-characters/01_unbound_characters.sql...${NC}"
    ERRMSG=$(docker exec -i ac-database mysql -u root -ppassword acore_characters \
            < "$MODULE_SQL/db-characters/01_unbound_characters.sql" 2>&1 >/dev/null)
    if [ $? -ne 0 ]; then
        print_warning "Failed to apply db-characters/01_unbound_characters.sql"
        [ -n "$ERRMSG" ] && echo "   MySQL error: $ERRMSG"
        FAILED=1
    fi

    # Apply npc_setup.sql HERE — before the worldserver rebuild — so the
    # creature_template entry for the Mentor (900001) exists when the server
    # starts.  RegisterCreatureGossipEvent(900001, ...) in the Lua crashes at
    # load time if the template is missing, preventing [UNBOUND] Prereq map
    # built. from ever appearing and causing wait_for_server() to time out.
    echo -e "${CYAN}   Applying npc_setup.sql (Mentor NPC template)...${NC}"
    local MODULE_DIR="$SERVER_DIR/modules/mod-unbound"
    ERRMSG=$(docker exec -i ac-database mysql -u root -ppassword acore_world \
            < "$MODULE_DIR/npc_setup.sql" 2>&1 >/dev/null) || true
    if [ -n "$ERRMSG" ] && echo "$ERRMSG" | grep -iv "warning\|insecure" | grep -q .; then
        print_warning "npc_setup.sql reported an error (Mentor template may be missing)"
        echo "   MySQL error: $ERRMSG"
        FAILED=1
    else
        print_success "Mentor NPC template (entry 900001) staged in world database."
    fi

    if [ "$FAILED" -ne 0 ]; then
        echo ""
        print_warning "One or more migrations failed to apply."
        echo "Your databases were backed up before any changes:"
        echo -e "${CYAN}   $BACKUP_DIR${NC}"
        echo "Restore from there if you need to roll back, then check the output"
        echo "above for the specific error before re-running this installer."
        exit 1
    fi

    print_success "All SQL migrations applied."
    echo ""
}

# ============================================================
#  apply_core_patches()
#
#  Wrath Unbound's cross-class access (Mentor-unlocked classes can train
#  abilities from class trainers, equip that class's gear, and accept that
#  class's quests) requires a small AzerothCore core-engine change: a new
#  Player::m_unboundClassMask field plus the five call sites that consult
#  it (Trainer.cpp, Player.cpp, PlayerQuest.cpp, PlayerStorage.cpp,
#  ConditionMgr.cpp). mod-unbound's OnPlayerLogin hook (UnboundSystem.cpp)
#  populates this field via SetUnboundClassMask() — the worldserver will
#  not COMPILE without this patch, since that method doesn't exist in
#  stock AzerothCore.
#
#  Applied with `git apply` against $SERVER_DIR (a git checkout — git is
#  already a hard dependency, used by stage_mod_ale() above).
#
#  Idempotent: if Player.h already declares GetUnboundClassMask(), the
#  patch was applied by a previous run and this is a no-op.
# ============================================================
apply_core_patches() {
    print_step "Applying Wrath Unbound core-engine patch (cross-class access)..."

    local PLAYER_H="$SERVER_DIR/src/server/game/Entities/Player/Player.h"
    local MODULE_DIR="$SERVER_DIR/modules/mod-unbound"
    local PATCH_FILE="$MODULE_DIR/unbound-core-access.patch"

    if [ ! -f "$PLAYER_H" ]; then
        print_warning "Could not find Player.h at:"
        print_warning "  $PLAYER_H"
        echo "Cross-class trainer/quest/item access requires this core-engine"
        echo "patch — without it the worldserver won't compile. Check that"
        echo "SERVER_DIR points at a real AzerothCore source checkout."
        exit 1
    fi

    if grep -q "GetUnboundClassMask" "$PLAYER_H"; then
        print_success "Core-engine cross-class patch already applied — nothing to do."
        echo ""
        return
    fi

    if [ ! -f "$PATCH_FILE" ]; then
        print_warning "Patch file missing at $PATCH_FILE — was stage_module_files run?"
        exit 1
    fi

    cd "$SERVER_DIR" || exit 1

    if ! git apply --check "$PATCH_FILE" 2>/dev/null; then
        print_warning "Core-engine patch did not apply cleanly to your AzerothCore source."
        echo "This usually means your src/ tree has diverged from the version"
        echo "Wrath Unbound was built against."
        echo ""
        echo "The patch is saved at:"
        echo -e "${CYAN}   $PATCH_FILE${NC}"
        echo "A maintainer can review and apply this 6-file diff by hand. Cross-class"
        echo "trainer/quest/item access won't work until this is resolved — everything"
        echo "else Wrath Unbound provides (Mentor, catalog, power pools, skills) is"
        echo "unaffected."
        exit 1
    fi

    git apply "$PATCH_FILE"
    print_success "Core-engine cross-class access patch applied (6 files)."
    echo "(Player::m_unboundClassMask + trainer/quest/item/condition checks)"
    echo ""
}

# ============================================================
#  configure_ale()
#
#  Wrath Unbound's Mentor (and Mentor Stone) is entirely driven by
#  env/dist/etc/modules/lua_scripts/unbound_mentor.lua via Eluna/ALE.
#  That directory is bind-mounted into the container for free as part
#  of AzerothCore's stock env/dist/etc mount — no custom volume mount
#  or AC_ALE_SCRIPT_PATH env var is needed, and docker-compose.override.yml
#  is never touched.
#
#  This matches the convention used by wow-manage.sh (the dads-mmo-lab
#  CLI) for all ALE-Kegs Lua mods, so unbound_mentor.lua sits alongside
#  any other ALE mods a player has installed via that tool, sharing the
#  same lua_scripts/ directory and mod_ale.conf.
#
#  This function only ensures mod_ale.conf (shared by all ALE mods) has
#  ALE.Enabled = 1 (integer — "true" is silently ignored) and ALE.ScriptPath
#  pointing at that shared directory. If wow-manage.sh (or a prior run)
#  already created mod_ale.conf, we correct it in place rather than
#  overwrite it — other ALE mods may depend on settings already in there.
#
#  Idempotent: re-running is a no-op if already correct.
# ============================================================
configure_ale() {
    print_step "Configuring mod_ale.conf for Lua script support..."

    local ALE_SCRIPT_PATH="/azerothcore/env/dist/etc/modules/lua_scripts"
    local MODULES_CONF_DIR="$SERVER_DIR/env/dist/etc/modules"
    local ALE_CONF="$MODULES_CONF_DIR/mod_ale.conf"

    if ! mkdir -p "$MODULES_CONF_DIR" 2>/dev/null; then
        print_warning "Could not create $MODULES_CONF_DIR"
        echo "The directory may be owned by root. Try:"
        echo -e "  ${CYAN}sudo mkdir -p $MODULES_CONF_DIR${NC}"
        echo -e "  ${CYAN}sudo chown deck:deck $MODULES_CONF_DIR${NC}"
        echo "Then re-run the installer."
        exit 1
    fi

    if [ ! -f "$ALE_CONF" ]; then
        if cat > "$ALE_CONF" << ALE_CONF_EOF
ALE.Enabled = 1
ALE.TraceBack = false
ALE.ScriptPath = "$ALE_SCRIPT_PATH"
ALE.PlayerAnnounceReload = false
ALE.RequirePaths = ""
ALE.RequireCPaths = ""
ALE.AutoReload = false
ALE.AutoReloadInterval = 1
ALE.BytecodeCache = true
ALE_CONF_EOF
        then
            print_success "Created mod_ale.conf (ALE.Enabled = 1, ALE.ScriptPath = \"$ALE_SCRIPT_PATH\")."
        else
            print_warning "Failed to write mod_ale.conf to $ALE_CONF"
            echo "The directory may be owned by root. Try:"
            echo -e "  ${CYAN}sudo chown deck:deck $ALE_CONF${NC}"
            echo "Then re-run the installer."
            exit 1
        fi
    else
        # Ensure existing conf has the correct integer format for ALE.Enabled
        if grep -q "ALE.Enabled = true" "$ALE_CONF" 2>/dev/null; then
            sed -i 's/ALE\.Enabled = true/ALE.Enabled = 1/' "$ALE_CONF"
            print_success "mod_ale.conf: corrected ALE.Enabled to integer format (1)."
        elif grep -q "^ALE.Enabled = 1" "$ALE_CONF" 2>/dev/null; then
            print_success "mod_ale.conf: ALE.Enabled = 1 already set."
        else
            echo "ALE.Enabled = 1" >> "$ALE_CONF"
            print_success "mod_ale.conf: added ALE.Enabled = 1."
        fi

        # Ensure ALE.ScriptPath points at the shared lua_scripts directory —
        # may already be set correctly by wow-manage.sh, or may be a stale
        # relative path / wrong directory from an older convention.
        if grep -qF "ALE.ScriptPath = \"$ALE_SCRIPT_PATH\"" "$ALE_CONF" 2>/dev/null; then
            print_success "mod_ale.conf: ALE.ScriptPath already correct."
        elif grep -q "^ALE.ScriptPath" "$ALE_CONF" 2>/dev/null; then
            sed -i "s|^ALE\.ScriptPath[[:space:]]*=.*\$|ALE.ScriptPath = \"$ALE_SCRIPT_PATH\"|" "$ALE_CONF"
            print_success "mod_ale.conf: corrected ALE.ScriptPath to \"$ALE_SCRIPT_PATH\"."
        else
            echo "ALE.ScriptPath = \"$ALE_SCRIPT_PATH\"" >> "$ALE_CONF"
            print_success "mod_ale.conf: added ALE.ScriptPath = \"$ALE_SCRIPT_PATH\"."
        fi
    fi

    echo ""
}

# ============================================================
#  verify_ale_config()
#  Confirms unbound_mentor.lua is staged where ALE actually scans
#  (env/dist/etc/modules/lua_scripts/), and that mod_ale.conf has
#  ALE enabled and pointed at that same directory. This is the
#  reality check that catches the "Mentor says Greetings only /
#  Mentor Stone just eats" failure mode before the user logs in.
# ============================================================
verify_ale_config() {
    print_step "Verifying ALE/Lua script configuration..."

    local ALE_SCRIPT_PATH="/azerothcore/env/dist/etc/modules/lua_scripts"
    local LUA_SCRIPT="$SERVER_DIR/env/dist/etc/modules/lua_scripts/unbound_mentor.lua"
    local ALE_CONF="$SERVER_DIR/env/dist/etc/modules/mod_ale.conf"
    local errors=0

    if [ -f "$LUA_SCRIPT" ]; then
        print_success "unbound_mentor.lua staged at env/dist/etc/modules/lua_scripts/"
    else
        print_warning "unbound_mentor.lua NOT found at $LUA_SCRIPT"
        errors=$((errors + 1))
    fi

    if [ -f "$ALE_CONF" ]; then
        if grep -q "^ALE.Enabled = 1" "$ALE_CONF"; then
            print_success "mod_ale.conf: ALE.Enabled = 1"
        else
            local found_val
            found_val=$(grep "ALE.Enabled" "$ALE_CONF" 2>/dev/null || echo "(ALE.Enabled line not found)")
            print_warning "mod_ale.conf: unexpected value — $found_val"
            errors=$((errors + 1))
        fi

        if grep -qF "ALE.ScriptPath = \"$ALE_SCRIPT_PATH\"" "$ALE_CONF"; then
            print_success "mod_ale.conf: ALE.ScriptPath = \"$ALE_SCRIPT_PATH\""
        else
            local found_path
            found_path=$(grep "ALE.ScriptPath" "$ALE_CONF" 2>/dev/null || echo "(ALE.ScriptPath line not found)")
            print_warning "mod_ale.conf: ALE.ScriptPath not pointing at lua_scripts/ — $found_path"
            errors=$((errors + 1))
        fi
    else
        print_warning "mod_ale.conf: not found at $ALE_CONF"
        errors=$((errors + 1))
    fi

    echo ""
    if [ "$errors" -gt 0 ]; then
        print_warning "$errors verification check(s) failed."
        echo "The Mentor's Lua script will NOT load until these are resolved."
        echo "Review the warnings above, correct the file manually if needed, then re-run."
        echo ""
        if ! ask_yes_no "Continue anyway?"; then
            exit 1
        fi
    else
        print_success "ALE config looks correct — the Mentor's Lua script should load on restart."
    fi

    echo ""
}

# ============================================================
#  patch_worldserver_conf()
#
#  Wrath Unbound REQUIRES ValidateSkillLearnedBySpells = 0 — if left at
#  the AzerothCore default of 1, the server strips every cross-class
#  spell from a character's record on each login, silently wiping
#  anything purchased through the Mentor. The setting is always present
#  and uncommented in worldserver.conf (`ValidateSkillLearnedBySpells = N`)
#  so the sed pattern is reliable.
# ============================================================
patch_worldserver_conf() {
    print_step "Checking worldserver.conf for the setting Wrath Unbound requires..."

    local CONF="$SERVER_DIR/env/dist/etc/worldserver.conf"

    if [ ! -f "$CONF" ]; then
        print_warning "Could not find worldserver.conf at:"
        print_warning "  $CONF"
        echo "You'll need to set this yourself before Wrath Unbound will work"
        echo "correctly:"
        echo -e "${CYAN}   ValidateSkillLearnedBySpells = 0${NC}"
        echo "Without it, AzerothCore strips cross-class spells from every"
        echo "character on login — wiping anything purchased through the Mentor."
        echo ""
        return
    fi

    local CURRENT
    CURRENT=$(grep -m1 "^ValidateSkillLearnedBySpells" "$CONF" | sed 's/.*=\s*//' | tr -d '[:space:]')

    if [ "$CURRENT" = "0" ]; then
        print_success "ValidateSkillLearnedBySpells is already set to 0 — nothing to change."
    else
        sed -i "s|^ValidateSkillLearnedBySpells.*|ValidateSkillLearnedBySpells = 0|" "$CONF"
        print_success "Set ValidateSkillLearnedBySpells = 0 in worldserver.conf"
        echo "(Required: without this, AzerothCore strips cross-class spells from"
        echo "every character on login — wiping anything bought through the Mentor.)"
    fi
    echo ""
}

# ============================================================
#  rebuild_server()
#
#  mod-unbound is a C++ module — it must be compiled into the
#  worldserver binary before any of its hooks (mana-pool preservation,
#  weapon/armor proficiency) take effect. This is an INCREMENTAL
#  rebuild — Docker reuses the existing compiled layers and only
#  compiles the new module in — NOT the original 2-4 hour from-scratch
#  Playerbots build, but still a real wait on Steam Deck hardware
#  (~30-90 min, per the dev rebuild history in CLAUDE.md).
#
#  Commands match the exact sequence used (and confirmed working)
#  during development: `docker compose build ac-worldserver` then
#  `docker compose up -d --force-recreate ac-worldserver`.
# ============================================================
rebuild_server() {
    print_step "Rebuilding the worldserver with the Wrath Unbound module..."
    echo ""
    echo "mod-unbound is a compiled C++ module — your worldserver needs an"
    echo "incremental rebuild to pick it up. Docker reuses your existing"
    echo "compiled layers and only builds the new module in, so this is much"
    echo "faster than the original multi-hour Playerbots compile, but it'll"
    echo "still take roughly 30-90 minutes on a Steam Deck."
    echo ""
    print_warning "Keep your Steam Deck plugged in and awake during this step."
    echo ""

    if ! ask_yes_no "Ready to rebuild the worldserver now?"; then
        echo ""
        echo "No problem — your module files and SQL migrations are already in"
        echo "place. When you're ready, rebuild manually with:"
        echo -e "${CYAN}   cd $SERVER_DIR${NC}"
        echo -e "${CYAN}   docker compose build ac-worldserver${NC}"
        echo -e "${CYAN}   docker compose up -d --force-recreate ac-worldserver${NC}"
        exit 0
    fi

    local LOGFILE="$HOME/wrath-unbound-rebuild.log"
    echo -e "${CYAN}   Progress saved to: $LOGFILE${NC}"
    echo -e "${CYAN}   Go grab a coffee — this will take a while.${NC}"
    echo ""

    cd "$SERVER_DIR" || exit 1

    docker compose build ac-worldserver 2>&1 | tee "$LOGFILE"
    if [ "${PIPESTATUS[0]}" -ne 0 ]; then
        print_warning "Build failed — check $LOGFILE for details."
        echo "Your databases were backed up before any changes were made:"
        echo -e "${CYAN}   $BACKUP_DIR${NC}"
        exit 1
    fi

    docker compose up -d --force-recreate ac-worldserver 2>&1 | tee -a "$LOGFILE"
    if [ "${PIPESTATUS[0]}" -ne 0 ]; then
        print_warning "Restart failed — check $LOGFILE for details."
        exit 1
    fi

    print_success "Worldserver rebuilt and restarted with Wrath Unbound."
    echo ""
}

# ============================================================
#  wait_for_server()
#
#  Confirms Wrath Unbound's Lua script loaded cleanly after the
#  rebuild + restart by polling for "[UNBOUND] Prereq map built." —
#  the exact line printed when the Lua catalog/PREREQ_MAP builds without
#  errors. --force-recreate in rebuild_server() creates a brand-new
#  container with an empty log buffer, so there is no stale line to
#  false-positive on; no --since timestamp needed.
#
#  Live-checked timing: warm restart ~12 seconds after container start.
#  Cold start after a fresh rebuild can take longer (new binary, DBC/map
#  loads), so the timeout is generous: poll every 5 seconds for up to
#  10 minutes.
# ============================================================
wait_for_server() {
    print_step "Waiting for the worldserver to come back up with Wrath Unbound loaded..."

    local MARKER="[UNBOUND] Prereq map built."
    local MAX_ATTEMPTS=120
    local ATTEMPT=0

    while [ "$ATTEMPT" -lt "$MAX_ATTEMPTS" ]; do
        if docker logs ac-worldserver 2>&1 | grep -F "$MARKER" > /dev/null; then
            print_success "Wrath Unbound loaded cleanly — saw \"$MARKER\" in the worldserver log."
            echo ""
            return
        fi
        ATTEMPT=$((ATTEMPT + 1))
        sleep 5
    done

    print_warning "Didn't see \"$MARKER\" in the worldserver log within 10 minutes."
    echo "The server may still be starting up, or Wrath Unbound's Lua script may"
    echo "have hit an error on load. Check the live log to see what's happening:"
    echo -e "${CYAN}   docker logs -f ac-worldserver${NC}"
    echo ""
    echo "If the log shows nothing Lua/Eluna-related at all (not even an error),"
    echo "confirm Eluna actually compiled into this binary:"
    echo -e "${CYAN}   docker exec ac-worldserver strings /azerothcore/env/dist/bin/worldserver | grep -i ALE.Enabled${NC}"
    echo "If that returns nothing, mod-ale wasn't built in — re-run this installer"
    echo "(it will stage mod-ale and rebuild again)."
    echo ""
}

# ============================================================
#  guide_manual_steps()
# ============================================================
guide_manual_steps() {
    echo ""
    if [ "$EXISTING_INSTALL" = true ]; then
        print_step "One last check — the Mentor (entry 900001):"
        echo ""
        echo -e "  If your Mentor from a previous install is still standing, you're"
        echo -e "  all set — no need to spawn another. If it's gone, log in and run:"
        echo -e "  ${GREEN}.npc add 900001${NC}"
    else
        print_step "One last step — spawn the Mentor in-game:"
        echo ""
        echo -e "  Log in, walk to where you want The Mentor to stand, and run:"
        echo -e "  ${GREEN}.npc add 900001${NC}"
        echo ""
        echo -e "  The spawn is permanent — you only need to do this once."
    fi
    echo ""
    echo -e "${WHITE}Press ENTER when you're done and ready to see the summary...${NC}"
    read -r
}

# ============================================================
#  show_completion()
# ============================================================
show_completion() {
    print_header
    if [ "$EXISTING_INSTALL" = true ]; then
        echo -e "${GREEN}${BOLD}Wrath Unbound has been updated!${NC}"
        echo ""
        echo -e "${WHITE}Your existing Wrath Unbound install was refreshed — module files,${NC}"
        echo -e "${WHITE}SQL migrations, the core-engine patch, and the worldserver binary${NC}"
        echo -e "${WHITE}are all up to date. Players will pick up the new abilities the next${NC}"
        echo -e "${WHITE}time they log in — no character action needed.${NC}"
        echo ""
        echo -e "${WHITE}${BOLD}New in this update:${NC}"
        echo -e "   ${CYAN}•${NC} Unlocked classes can now train abilities directly from class"
        echo -e "     trainers, equip that class's gear, and accept that class's quests"
        echo -e "   ${CYAN}•${NC} Mentor: individual spells buy instantly with one click, plus a"
        echo -e "     \"Buy ALL available abilities\" button"
        echo -e "   ${CYAN}•${NC} Catalog fixes: corrected req_levels against real trainers, added"
        echo -e "     missing Mage teleports/portals + Paladin Summon Warhorse, and fixed"
        echo -e "     Paladin Judgement / Paladin+Warlock mounts / Druid Flight Form"
        echo -e "     purchases that previously took gold and granted nothing"
    else
        echo -e "${GREEN}${BOLD}Wrath Unbound is installed!${NC}"
        echo ""
        echo -e "${WHITE}Your WotLK Playerbots server now has the multi-class Wrath Unbound${NC}"
        echo -e "${WHITE}mod layered on top. Here's what changed:${NC}"
    fi
    echo ""
    echo -e "   ${CYAN}•${NC} The Mentor (entry 900001) is spawned and ready for players"
    echo -e "   ${CYAN}•${NC} Players unlock additional classes through the Mentor starting at level 5"
    echo -e "   ${CYAN}•${NC} Cross-class spells are purchased individually (instant-buy, or"
    echo -e "     \"Buy ALL\"), with rank prerequisites enforced"
    echo -e "   ${CYAN}•${NC} Unlocked classes can train from, equip, and quest as that class too"
    echo ""
    echo -e "${WHITE}${BOLD}NEW in v1.3.0 — Cross-class talents:${NC}"
    echo -e "   ${CYAN}•${NC} Players can now spend talents in ANY unlocked class's trees,"
    echo -e "     with real tier/prereq gating and a shared talent-point pool"
    echo -e "     (respec supported) — all validated server-side."
    echo -e "   ${CYAN}•${NC} This needs the CLIENT addons. Hand your players the pack:"
    echo -e "     ${CYAN}WrathUnbound-Addons.zip${NC} — extract into"
    echo -e "     ${CYAN}World of Warcraft/Interface/AddOns/${NC}, then /reload in-game."
    echo ""
    echo -e "${WHITE}${BOLD}NEW in v1.3.0 - Multi-class summons (server-side):${NC}"
    echo -e "   ${CYAN}â¢${NC} Warlock/Mage/DK pets no longer conflict for multiclass"
    echo -e "     characters, and eligible classes can field multiple guardians at"
    echo -e "     once. Compiled into the worldserver during this rebuild."
    echo ""
    echo -e "${WHITE}${BOLD}NEW in v1.4.0 — Talent points:${NC}"
    echo -e "   ${CYAN}•${NC} Respec now unlearns EVERY talent — the character's own class"
    echo -e "     tree as well as every cross-class tree — and refunds all points."
    echo -e "   ${CYAN}•${NC} The Mentor sells talent points at a flat 75g each (1, 5 or 10"
    echo -e "     at a time). Bought points are permanent — they survive relog,"
    echo -e "     level-up and respec."
    echo -e "   ${CYAN}•${NC} Cross-class points spent are no longer handed back for free on"
    echo -e "     relog or level-up (they used to be silently refunded)."
    echo ""
    echo -e "${WHITE}${BOLD}Re-hand out the client pack with this update:${NC}"
    echo -e "   ${CYAN}•${NC} ${CYAN}WrathUnbound-Addons.zip${NC} was refreshed alongside this release"
    echo -e "     (Multiclass Talents UI 2.9.27, Unbound Spellbook 0.3)."
    echo -e "   ${CYAN}•${NC} The Unbound Spellbook was missing every ability that comes from a"
    echo -e "     class QUEST rather than a trainer — warlock demons, the hunter pet"
    echo -e "     commands, Death Gate, Runeforging, Taunt, Sunder Armor and more."
    echo -e "     Players need the new pack to see them."
    echo ""
    echo -e "${WHITE}${BOLD}A database backup was saved before any changes were made:${NC}"
    echo -e "   ${CYAN}$BACKUP_DIR${NC}"
    echo -e "${WHITE}Keep it somewhere safe in case you ever need to roll back.${NC}"
    echo ""
    echo -e "${WHITE}${BOLD}Worth knowing:${NC}"
    echo -e "   ${CYAN}•${NC} Cross-class spells currently land in the General tab of the"
    echo -e "     spellbook (and weapon/armor skills show oddly in the Skills panel)"
    echo -e "     until a future client-side fix ships — purely cosmetic, everything works"
    echo -e "   ${CYAN}•${NC} To update later, just re-run this installer — it detects the"
    echo -e "     existing install, refreshes everything, and won't duplicate anything"
    echo ""
    print_success "Have fun, and let your players loose on the Mentor!"
    echo ""
}

# ─────────────────────────────────────────
# MAIN
# ─────────────────────────────────────────
print_header
echo -e "${WHITE}This adds the Wrath Unbound multi-class mod to your existing${NC}"
echo -e "${WHITE}Dad's MMO Lab WotLK Playerbots server.${NC}"
echo ""

if ! ask_yes_no "Ready to check compatibility?"; then
    echo "No problem — run this script when you're ready!"
    exit 0
fi

# Resolve SERVER_DIR — auto-detects renamed folders (e.g. wow-unbound).
# `docker compose` resolves its project from CWD, so we must cd into
# SERVER_DIR before any compose call.
detect_server_dir
cd "$SERVER_DIR" || exit 1

check_compatibility
check_existing_install
backup_database
stage_module_files
stage_talent_bridge
stage_summons_module
stage_mod_ale
apply_sql_migrations
apply_core_patches
patch_worldserver_conf
configure_ale
verify_ale_config
rebuild_server
wait_for_server
guide_manual_steps
show_completion
