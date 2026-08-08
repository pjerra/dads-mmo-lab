#!/usr/bin/env bash
# DML Pack Installer -- one file, menu driven, no typing beyond single digits.
#
# Self-contained: dmlpack.py rides inside this script as a base64 payload after
# the __DMLPACK_PAYLOAD__ marker, so this single file is everything a fresh Steam
# Deck needs. Regenerate with build-installer.sh after changing dmlpack.py.
#
# Designed for a Deck with NO KEYBOARD: every prompt takes a single digit or y/n,
# packs are auto-discovered rather than typed, and the only free-text entry
# (reclaim's typed game name) is a deliberate safety gate.
set -uo pipefail

VERSION="1.0.0"
TOOLDIR=""
PACK=""

# ---------------------------------------------------------------- presentation
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  C_HDR=$'\033[1;36m'; C_STEP=$'\033[1;34m'; C_OK=$'\033[1;32m'
  C_WARN=$'\033[1;33m'; C_ERR=$'\033[1;31m'; C_DIM=$'\033[0;36m'; C_OFF=$'\033[0m'
else
  C_HDR=""; C_STEP=""; C_OK=""; C_WARN=""; C_ERR=""; C_DIM=""; C_OFF=""
fi
print_header()  { echo; echo "${C_HDR}============================================================${C_OFF}";
                  echo "${C_HDR}  $*${C_OFF}";
                  echo "${C_HDR}============================================================${C_OFF}"; }
print_step()    { echo "${C_STEP}==> ${C_OFF}$*"; }
print_success() { echo "${C_OK}  OK  ${C_OFF}$*"; }
print_warning() { echo "${C_WARN}  !!  ${C_OFF}$*"; }
print_error()   { echo "${C_ERR} FAIL ${C_OFF}$*" >&2; }
print_info()    { echo "${C_DIM}  ..  ${C_OFF}$*"; }

press_enter() { echo; read -r -p "Press Enter to continue... " _ || true; }
ask_yes_no() {  # $1 prompt, $2 default (y/n)
  local ans def="${2:-n}" hint="[y/N]"
  [ "$def" = y ] && hint="[Y/n]"
  read -r -p "$1 $hint " ans || true
  ans="${ans:-$def}"
  [[ "$ans" =~ ^[Yy] ]]
}

cleanup() { [ -n "$TOOLDIR" ] && [ -d "$TOOLDIR" ] && rm -rf "$TOOLDIR"; }
trap cleanup EXIT

# ------------------------------------------------------------------ unpack tool
extract_tool() {
  TOOLDIR=$(mktemp -d /tmp/dmlpack-tool-XXXXXX)
  # Prefer a sibling dmlpack.py when present (development), else use the payload.
  local here; here="$(cd "$(dirname "$(readlink -f "$0")")" && pwd)"
  if [ -f "$here/dmlpack.py" ]; then
    cp "$here/dmlpack.py" "$here/tools.Dockerfile" "$TOOLDIR/" 2>/dev/null || true
    print_info "using dmlpack.py from $here"
  else
    sed -e '1,/^__DMLPACK_PAYLOAD__$/d' "$(readlink -f "$0")" | base64 -d | tar -xJf - -C "$TOOLDIR"
  fi
  [ -f "$TOOLDIR/dmlpack.py" ] || { print_error "could not unpack dmlpack.py"; exit 1; }
  chmod +x "$TOOLDIR/dmlpack.py"
}
dmlpack() { python3 "$TOOLDIR/dmlpack.py" "$@"; }

# ------------------------------------------------------------- pack discovery
declare -a PACKS=()
find_packs() {
  PACKS=()
  local p
  while IFS= read -r p; do [ -n "$p" ] && PACKS+=("$p"); done < <(
    { find /run/media/deck -maxdepth 4 -name '*.dmlpack' -type f 2>/dev/null
      find "$HOME" -maxdepth 3 -name '*.dmlpack' -type f 2>/dev/null
      find "$(cd "$(dirname "$(readlink -f "$0")")" && pwd)" -maxdepth 2 -name '*.dmlpack' -type f 2>/dev/null
    } | sort -u )
}

pack_summary() {  # $1 path -> "Display Name|size|date"
  local info
  # NB: this python lives inside a single-quoted bash string, so double quotes
  # need no escaping -- escaping them produced a syntax error and every pack
  # showed up as "unreadable".
  info=$(tar -xOf "$1" manifest.json 2>/dev/null | python3 -c '
import json, sys
try:
    m = json.load(sys.stdin)
    print(m.get("display_name", "?"), m.get("packed_at", "?")[:10],
          len(m.get("members", [])), sep="|")
except Exception:
    print("unreadable|?|0")' 2>/dev/null) || info="unreadable|?|0"
  [ -n "$info" ] || info="unreadable|?|0"
  echo "$info"
}

choose_pack() {
  find_packs
  if [ ${#PACKS[@]} -eq 0 ]; then
    print_warning "No .dmlpack files found on the SD card or in your home folder."
    print_info "Plug in the card with the archives, or copy a .dmlpack to ~/"
    if ask_yes_no "Type a path manually instead?" n; then
      read -r -p "Full path to the .dmlpack: " PACK || true
      [ -f "$PACK" ] && return 0
      print_error "not a file: $PACK"
    fi
    return 1
  fi

  print_header "Choose a pack"
  local i=1 p name date members size
  for p in "${PACKS[@]}"; do
    IFS='|' read -r name date members <<< "$(pack_summary "$p")"
    size=$(du -h "$p" 2>/dev/null | cut -f1)
    printf "   %d) %-16s %6s   packed %s   %s members\n" "$i" "$name" "$size" "$date" "$members"
    printf "      %s%s%s\n" "$C_DIM" "$p" "$C_OFF"
    i=$((i+1))
  done
  echo "   0) back"
  echo
  local choice
  read -r -p "Pick a number: " choice || true
  [ "$choice" = 0 ] && return 1
  if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le ${#PACKS[@]} ]; then
    PACK="${PACKS[$((choice-1))]}"
    print_success "selected: $(basename "$PACK")"
    return 0
  fi
  print_error "not a valid choice"
  return 1
}

need_pack() { [ -n "$PACK" ] && [ -f "$PACK" ] || choose_pack; }

# --------------------------------------------------------- environment fixups
ensure_docker() {
  if ! command -v docker >/dev/null 2>&1; then
    print_error "Docker is not installed on this Deck."
    print_info "The server runs in Docker, so a restore cannot work without it."
    print_info "Install it from a terminal, then re-run this installer:"
    print_info "    sudo steamos-readonly disable && sudo pacman -S docker docker-compose"
    print_info "    sudo systemctl enable --now docker && sudo usermod -aG docker \$USER"
    return 1
  fi
  if ! docker info >/dev/null 2>&1; then
    print_warning "Docker is installed but not running."
    if ask_yes_no "Start it now (asks for your sudo password)?" y; then
      sudo systemctl start docker && sleep 3
    fi
    docker info >/dev/null 2>&1 || { print_error "Docker still not responding."; return 1; }
  fi
  print_success "Docker is running"
  return 0
}

steam_running() { pgrep -x steam >/dev/null 2>&1; }

ensure_steam_closed() {
  steam_running || { print_success "Steam is closed"; return 0; }
  print_warning "Steam is running."
  print_info "Steam rewrites its shortcuts file when it exits, which would silently"
  print_info "throw away the shortcuts this installer adds."
  if ask_yes_no "Close Steam now?" y; then
    steam -shutdown >/dev/null 2>&1 &
    local n=0
    while steam_running && [ $n -lt 30 ]; do sleep 2; n=$((n+1)); done
    if steam_running; then
      print_error "Steam is still running -- close it from the taskbar, then retry."
      return 1
    fi
    print_success "Steam closed"
    return 0
  fi
  print_warning "Continuing with Steam open -- shortcuts will be skipped."
  print_info "You can add them later from this menu (option 4)."
  return 0
}

# ------------------------------------------------------------------- actions
do_verify() {
  need_pack || return 0
  print_header "Checking the archive"
  print_info "Re-reads every byte and compares checksums. A few minutes for a big pack."
  dmlpack verify "$PACK"
  press_enter
}

do_preview() {
  need_pack || return 0
  print_header "Preview -- nothing will be written"
  local out; out=$(dmlpack restore "$PACK" --dry-run 2>&1)
  echo "$out"
  if grep -q "Steam is RUNNING" <<< "$out"; then
    echo
    print_info "The Steam warning above is fine for a preview -- it only has to be"
    print_info "closed for a real install, and option 1 offers to close it for you."
  fi
  press_enter
}

do_install() {
  need_pack || return 0
  local name; IFS='|' read -r name _ _ <<< "$(pack_summary "$PACK")"

  print_header "Install $name"
  print_info "pack : $(basename "$PACK") ($(du -h "$PACK" | cut -f1))"
  echo
  print_info "This will:"
  print_info "  1. check there is room, ports are free, and Docker is up"
  print_info "  2. unpack the server and the game client"
  print_info "  3. rebuild the Docker image"
  print_info "  4. add the Steam shortcuts"
  print_info "Nothing on this Deck is deleted."
  echo
  ask_yes_no "Continue?" y || return 0

  ensure_docker || { press_enter; return 0; }

  if ask_yes_no "Check the archive's checksums first (slower, recommended once)?" n; then
    dmlpack verify "$PACK" || { print_error "Archive failed its checks -- not installing."; press_enter; return 0; }
  fi

  print_step "checking this Deck is ready"
  if ! dmlpack restore "$PACK" --dry-run >/tmp/dmlpack-preflight.$$ 2>&1; then
    grep -E "FAIL|!!" /tmp/dmlpack-preflight.$$ || cat /tmp/dmlpack-preflight.$$
    echo
    if grep -q "Steam is RUNNING" /tmp/dmlpack-preflight.$$; then
      ensure_steam_closed || { rm -f /tmp/dmlpack-preflight.$$; press_enter; return 0; }
    fi
    if grep -q "missing runtime dep" /tmp/dmlpack-preflight.$$; then
      local need; need=$(grep -o "Proton[^ ]*[^-]*" /tmp/dmlpack-preflight.$$ | head -1)
      print_error "A required Proton version is not installed."
      print_info "In Steam: Library > any game > Properties > Compatibility, pick it once,"
      print_info "or install it from the Steam Play settings. Needed: $need"
      print_info "Then come back and run this again."
      rm -f /tmp/dmlpack-preflight.$$; press_enter; return 0
    fi
    # re-check after the fixes above
    if ! dmlpack restore "$PACK" --dry-run >/tmp/dmlpack-preflight.$$ 2>&1; then
      print_error "This Deck is not ready yet:"
      grep -E "FAIL" /tmp/dmlpack-preflight.$$ | head -8
      rm -f /tmp/dmlpack-preflight.$$; press_enter; return 0
    fi
  fi
  rm -f /tmp/dmlpack-preflight.$$
  print_success "ready to install"
  echo
  ask_yes_no "Unpack now? This takes several minutes." y || return 0

  print_header "Installing -- do not close this window"
  if dmlpack restore "$PACK"; then
    print_header "Done"
    print_success "$name is installed."
    print_info "Start Steam, then look for the new entries in your library."
    print_info "Launch the SERVER shortcut first and wait for it to say it is ready,"
    print_info "then launch the game."
  else
    print_warning "The payload restored, but something at the end needs attention"
    print_info "(most often: Steam was reopened, so the shortcuts were skipped)."
    print_info "Use option 4 from the main menu to add the shortcuts."
  fi
  press_enter
}

do_shortcuts() {
  need_pack || return 0
  print_header "Add the Steam shortcuts"
  ensure_steam_closed || { press_enter; return 0; }
  dmlpack shortcuts "$PACK"
  press_enter
}

do_list() {
  print_header "Packs found on this Deck"
  find_packs
  if [ ${#PACKS[@]} -eq 0 ]; then print_info "none found"; else
    local p name date members
    for p in "${PACKS[@]}"; do
      IFS='|' read -r name date members <<< "$(pack_summary "$p")"
      printf "   %-16s %6s  packed %s\n      %s%s%s\n" \
        "$name" "$(du -h "$p" | cut -f1)" "$date" "$C_DIM" "$p" "$C_OFF"
    done
  fi
  press_enter
}

do_advanced() {
  print_header "Advanced"
  print_warning "Reclaim DELETES a game from this Deck to free space."
  print_info "It refuses unless the pack has been proven to restore on a second Deck,"
  print_info "it re-checks every checksum before deleting anything, it refuses while a"
  print_info "running container still has one of those folders open, and it asks you to"
  print_info "type the game's name. It also removes that game's Docker volumes."
  echo "   1) Reclaim disk space (delete a game that is safely archived)"
  echo "   2) Record that a restore was proven on THIS machine"
  echo "   0) back"
  local c; read -r -p "Pick a number: " c || true
  case "$c" in
    1) need_pack || return 0; dmlpack reclaim "$PACK"; press_enter ;;
    2) need_pack || return 0
       # Pass the PACK, not the game name: the flag is per (game, version), and
       # handing over the archive is what keeps those two from drifting apart.
       dmlpack deck2-ok "$PACK"; press_enter ;;
    *) : ;;
  esac
}

do_repair() {
  need_pack || return 0
  print_header "Repair"
  print_info "For an install that unpacked fine but will not start -- most often a"
  print_info "Docker image that never got built. Re-checks and rebuilds only the"
  print_info "small stuff; your game files are NOT re-extracted."
  echo
  ask_yes_no "Run the repair now?" y || return 0
  ensure_docker || { press_enter; return 0; }
  dmlpack repair "$PACK"
  press_enter
}

# --------------------------------------------------- per-game setup guides
# These live in the installer rather than inside the .dmlpack on purpose: the
# text is for people (kids included) and gets reworded often, and updating a
# 40 KB installer beats rebuilding a 12 GB archive to fix a sentence.

game_id() {
  tar -xOf "$1" manifest.json 2>/dev/null \
    | python3 -c 'import json,sys
try: print(json.load(sys.stdin).get("game",""))
except Exception: print("")' 2>/dev/null
}

has_guide() {
  case "$(game_id "$1")" in
    tortoise-wow) return 0 ;;
    *) return 1 ;;
  esac
}

guide_title() {
  case "$(game_id "$1")" in
    tortoise-wow) echo "Turtle WoW: radio & music" ;;
    *) echo "Game setup" ;;
  esac
}

guide_tortoise_wow() {
  local RADIO="$HOME/tortoise-radio"
  while true; do
    print_header "Turtle WoW  --  Radio & Music"
    cat <<'TXT'
  YOUR OWN RADIO STATION IN THE GAME

  Turtle WoW can play music while you explore. While you are
  playing, type this into the chat box:

      .radio 1    a pirate radio station from the internet
      .radio 2    YOUR OWN songs

  There is no command to turn it off again. If you want quiet,
  turn the Music slider down in the game's Sound options.

  HOW TO ADD YOUR OWN SONGS

      1. Pick  1  below. Your music folder will open.
      2. Drag your songs into it.
         mp3, m4a, ogg, flac, wav and opus all work.
      3. Pick  2  below to load them.
      4. Go into the game and type   .radio 2

  To remove a song, delete it from the folder and pick 2 again.
  If you add a LOT of songs at once, loading them takes a
  minute or two the first time. That is normal.
TXT
    if [ -d "$RADIO/music" ]; then
      printf "
  You have %s songs in your music folder" "$(ls -1 "$RADIO/music" 2>/dev/null | wc -l)"
      [ -d "$RADIO/wow-music" ] && printf ", and %s songs from
  the game itself that you can copy over (pick 3)" \
        "$(ls -1 "$RADIO/wow-music" 2>/dev/null | wc -l)"
      printf ".
"
    else
      print_warning "Turtle WoW is not installed on this Deck yet -- install it first (option 1)."
    fi
    cat <<'TXT'

  ----------------------------------------------------------
   1) Open my music folder
   2) Load my new songs
   3) Open the game's own music folder
   4) Turn the radio on for the first time  (needs a grown-up)
   0) Back
TXT
    local c; read -r -p "Pick a number: " c || true
    case "$c" in
      1) if [ -d "$RADIO/music" ]; then
           xdg-open "$RADIO/music" >/dev/null 2>&1 &
           print_success "Opened $RADIO/music -- drag your songs in, then pick 2."
         else print_error "No music folder yet. Install Turtle WoW first."; fi
         press_enter ;;
      2) if docker ps -a --format '{{.Names}}' 2>/dev/null | grep -qx tortoise-radio; then
           print_step "loading your songs (this can take a minute)"
           if docker restart tortoise-radio >/dev/null 2>&1; then
             print_success "Done. In the game, type  .radio 2"
             print_info "New songs get converted on first load, so give it a moment."
           else print_error "Could not restart the radio."; fi
         else
           print_warning "The radio is not running yet."
           if [ -f "$RADIO/docker-compose.yml" ] && ask_yes_no "Start it now?" y; then
             (cd "$RADIO" && docker compose up -d) && print_success "Radio started."
           fi
         fi
         press_enter ;;
      3) if [ -d "$RADIO/wow-music" ]; then
           xdg-open "$RADIO/wow-music" >/dev/null 2>&1 &
           print_success "Opened the game's own music."
           print_info "Copy any you like into your music folder, then pick 2."
         else print_error "Not found. Install Turtle WoW first."; fi
         press_enter ;;
      4) print_header "Turning the radio on for the first time"
         print_info "This only has to be done ONCE on this Deck."
         print_info "It points the game's radio addresses at this machine, and sets it"
         print_info "up to survive SteamOS updates (they wipe the setting)."
         print_warning "It asks for the Deck's password, so grab a grown-up."
         if [ ! -f "$RADIO/install-hosts-fix.sh" ]; then
           print_error "Turtle WoW is not installed yet -- install it first."
         elif ask_yes_no "Run it now?" n; then
           sudo bash "$RADIO/install-hosts-fix.sh" && print_success "Radio is switched on."
         fi
         press_enter ;;
      0) return 0 ;;
      *) print_error "pick a number from the list" ;;
    esac
  done
}

do_gameguide() {
  # No pack needed when the game is already installed -- this menu is for
  # players, not for whoever does the restoring.
  if [ -n "$PACK" ]; then
    case "$(game_id "$PACK")" in
      tortoise-wow) guide_tortoise_wow; return 0 ;;
    esac
  fi
  if [ -d "$HOME/tortoise-radio" ]; then guide_tortoise_wow; return 0; fi
  print_info "This game has no extra setup guide."
  press_enter
}

do_selfinstall() {
  # A Deck with no keyboard cannot type "./install-dmlpack.sh". Drop a launcher
  # on the desktop so this is double-clickable from then on.
  print_header "Desktop shortcut"
  local self desk
  self="$(readlink -f "$0")"
  desk="${XDG_DESKTOP_DIR:-$HOME/Desktop}"
  mkdir -p "$desk"
  cat > "$desk/DML Pack Installer.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=DML Pack Installer
Comment=Restore an archived game server + client
Exec=konsole --hold -e bash "$self"
Icon=applications-games
Terminal=false
Categories=Game;
EOF
  chmod +x "$desk/DML Pack Installer.desktop"
  print_success "added: $desk/DML Pack Installer.desktop"
  print_info "Double-click it from Desktop Mode -- no typing needed."
  print_info "Keep this script where it is; the shortcut points at $self"
  press_enter
}

# ---------------------------------------------------------------------- menu
main_menu() {
  while true; do
    print_header "DML Pack Installer  v$VERSION"
    if [ -n "$PACK" ]; then
      print_info "selected pack: $(basename "$PACK")"
    else
      print_info "no pack selected yet"
    fi
    echo
    echo "   1) Install a game from a pack"
    echo "   2) Choose a different pack"
    echo "   3) Check a pack is not damaged"
    echo "   4) Add the Steam shortcuts only"
    echo "   5) Preview an install (changes nothing)"
    echo "   6) List the packs I can see"
    echo "   7) Advanced (free up disk space)"
    echo "   8) Put a shortcut to this installer on my desktop"
    echo "   9) Repair an install that won't start"
    if { [ -n "$PACK" ] && has_guide "$PACK"; } || [ -d "$HOME/tortoise-radio" ]; then
      if [ -n "$PACK" ] && has_guide "$PACK"; then
        echo "  10) $(guide_title "$PACK")"
      else
        echo "  10) Turtle WoW: radio & music"
      fi
    fi
    echo "   0) Quit"
    echo
    local c; read -r -p "Pick a number: " c || true
    case "$c" in
      1) do_install ;;
      2) PACK=""; choose_pack || true ;;
      3) do_verify ;;
      4) do_shortcuts ;;
      5) do_preview ;;
      6) do_list ;;
      7) do_advanced ;;
      8) do_selfinstall ;;
      9) do_repair ;;
      10) do_gameguide ;;
      0) echo; print_info "bye"; exit 0 ;;
      *) print_error "pick a number from the list" ;;
    esac
  done
}

print_header "DML Pack Installer"
print_info "restores a game server + client that was archived from another Steam Deck"
command -v python3 >/dev/null 2>&1 || { print_error "python3 is missing -- cannot continue"; exit 1; }
extract_tool
print_success "tool ready"
# Preselect when there is exactly one pack, so the common case is zero typing.
find_packs
if [ ${#PACKS[@]} -eq 1 ]; then
  PACK="${PACKS[0]}"
  print_success "found one pack: $(basename "$PACK")"
fi
main_menu
__DMLPACK_PAYLOAD__
/Td6WFoAAATm1rRGBMDnuAGA0AUhARYAAAAAAEZmY5ThZ/9cX10AMhtJzvDoCYtC4Ztj3lhiAtmN
/ZcYY76ITzSc5t8Oe9QJTARANKjLCIqkI2RAeNV1kdRlw0Ubvpr37dDjgUYwZciRBCSgKwihuaSB
qCKVz9SQLlk08d4G7yzZGFMf0QYvm8dQDU2YuMIal/oi6xtDyTnw08cmzzDgewGj2ISOl1ECfCyp
luub2pZtIgvOoGmvzjmc/OAaqOp5NMQgBycXQO0/kI4t6ZlZd8Gm/zOSN9v33xdQTWsgBOyKzxZs
BMzKXRJzLxI0nk8cdlfiEuke8trsLBfPYAm34uj7592xsksa10FJXwcg52XRzK6kX3eTRKdWzYWh
VHw0/l2PH4d6AT8b4P4S/Nr6MgCFRUDM4e/bxrlWPq7z8YLcl3hxgqWlnZIn7E1z+uezu4hZX/6a
DChgjAIgTBVRpEgpDvyut0BnS0RrE4X5OHYUDUq3CKABw3g6xgqPQeDl+Vom/u823yBZEkqHVQ0t
WgRajTogFdKUHQlB+7NQq3+ANgSQeILEtix/4Ox0Ci8RZ9epAzPjZb6lBIo4lxnCL9f0OwDHwxjs
n0sFrjnYhECSZBNeUGaV42ZCEkB1yy+0jJMq55/zbIYOSPMgp+euZcqzJCPRmF5jdjMKJ8+jKn9Z
iBIQQnPnqirxiROBgaCoOIHVaei+E/s4IIx7Ekm1pVLOE4V+IIlDpIrsndcJvTRQA22jnMqUprAA
letCSeL4MrybImQ7eXo/eiY4+YjEZ7ct38LmgXh+aG4nsENgaszggRURggSGifn0B94kcYAHcBuT
MX+qqlvR/eoIp1kjg0fqHe18hPq3TxSzOogpS6JaBO6ThiMOQ0kLyujcDixWz2k+Fc0tvf2uDruT
+rgxS+Q1yItkgO6+1KP8xi8uofZOm2YigBotjWD2gGQ1lmznyRslvtJ6v8D5OzpMi6uyAq/wgvKJ
kq4Vt6/zpqvkgoXT6YWUIbIGo4gX8Hr+IPGyB5ClkHjbxJgqGdiy+JWYQp0VoHQX+ttRBfMTg4yQ
Rcsp4miMelOLKTIYE3v/DJitTPCwHnOHGxqfhigNvGpJprlcRz8/eJkyjuqvQlMkyGxFxEMh41J2
hOYj5vCOQN47WHt3fzCdHWJzlSXYqfz7/PWuo4b4PsL/l04+83XN85y/u9TL7QTNjPjg50MHnwnL
+XGmE0Yns4BkCdyaIKqIc9iICKuDKSgoFG/8F5ZWAuNnTgpRSEBvN7RSpkGHVDIcQaeqnXi2cvDf
kyLmE8z4RmbUT80l2taEeAdxiXzAyKwhVPKEHrgF4G+ER/K9NcW+V6TQQvK934kU4KGCPSWX7STQ
WvlG+ZV3V29OHs3hSYSjnt/2Yrt1p/XThWF+sqB/sSUitmV6CwYeh2jDKDD19IMXSLqZvErFbPn7
W0Zn72h5sA/nWgGJT7MRpXv4F7X43w8HwgUTF5AVVp4aEnNpPIUGfqx+oajVpzD9cIIVlHEC2GcJ
4olARJ6JkyNsVlhONaUn02IkfO+i+Zj44hLrGcM77yVlR9gFbS2S385M0PqLeHGZ/j4kVH16mNzz
poNqRhfnSNRadmktyv0sku+HCk/O+JVbLHUWOHA2RjseYQ/5JjMhp6Ag4J+5i2WA2uN7DNQuJKwD
UlnDK5aLiNz1btHWgkzRseOkwBVfMam9GuDOKF5T3zPQVtfIJaHPOQEsKbULnWmcJQ6w53JUZUYn
4HNAIwith3jT/m2TQRyFlWBMa2m1xjLOENxK5zXjF49gchaz9DrXmAuN64nxtDTTq23jeOGo6bNL
yMVDRWq8hedpdxunqizej012rkRTsvk6C5wGoXrmCWRcsSwR4q8FvGoVVwKLv+54mbK/+wgPG6aa
lrnGb36B94ZP00izQGwFkR/m0t/5Ynpg1XCEX4bVTswz5+FYIbwHnlQsXSP8+36hUpdbl0jFXPSl
OcPJpSuPgf21UPEwO1qDNt7T1xGxnq3LtN4Vh69blsJc/S0xZHMPgfaB+Nnft4LABlb6uunsEDeD
BOPBsjmVBdzzcrNjN3jbhX6otMOrjC62UWDOtI2AqkPorw2HgEJ9bdQzEqg76MEo+pnB+LlL0re3
26iIGPoSzTC0VppdJ5DPewk4DsCBGnalOggu94eFMYLOh7M5C2MjLL64HKgndApJvp/1r1S9wUbz
vIi+ThExykh+sRbSm8Kg/SbswvR4Mg6qCr51Adjkvuo+9RsoaIE5cCfTHr1fYU3TnMscuWgTowT6
WfHn3QzWWsI6jFZThkuT4eSHvs3MD3vAR6J/e6MqfiNWliEdM6yoLaUclGj93bYFC6mD0xY+gfAd
PQOMnWe3TNEXkaX+Hm5/uQnGUmbh35mVtXOTOmkRbiH8KcS6eql/AwKM81fZc/gpMLptOhFBRvF8
iTR0wAL7dcmcYBvL04tgWsGywK2EalQjhr1sSvtdVUgzedpvVWfqJ48hk1mEcWcCJUCbuT6NgTkX
m/z4GFUs+eLM2kZEat1EmZu90Y6PUB/RdT8rd0FSkqdWLz4/zqSfelT3Hm6HTB6qKrDi1ICJVOzw
APoHLbPidq9bgNrkRXTVymTY5Hx3siY27nUsRDZevwx9CFugimXE7NsbuxajbsST5tCyt3bkCRGr
DBcxpMxeaMthyggU1zLFwS485v9VQn175uIuelLknD9Ag+5E8SlqxCKpcKhqfXcUL/6vqIdy0J/9
G6c6tN3Bw1KqcQPIZ5scB77NifbQEwhVT+/O7WrA03XfoMKFtKJOuP37dkklQnEl7LGsCB4C6XiV
vZU1ahURJgn3lBYTCFafGK9zk1vy1PfugUv0VfQ7dFlV6+giECHm6+8GKR5hq6bwu7Yul/0IcyNr
KYoqj9g4k2T2U8/3uQh8abXyXXOa2gmOBrLJO1O2ZIp8KzbJallHbJD7t5mk0X67ZWWcIriWLr3J
6l9vG9QHqJoWzD7ur4tDOqU23ohehoo8K+JqeCXzz+4Wfj3ddLAeV4xqlnsXNUHW2lrf9XRn7H+W
fEhZZ1dRx5Kpfr7nRs4cY+O+uowlMqaE0jjKteCRtdHE4vr3hXtp3Ttc6PxAWgb7fVmrdFrhoCm4
2gHPZtlp4WQseipff6Ra+ZGZzegLhAb7r/vBP1EQarSAgnvZkOBnNjFbMgaddyJ1x0gpSbnvJuHG
UzoiXAFwHM8d8MX7ElbvcA6aphMHes7TIBQBTLFMviT5lcxQ18qFgtGR3w3ThxID+AUM3yZqlBM6
mejBXGYwXLf+Bf1ajA9jPStkdDVYEWMFNi9MiKh4T7obtQb16pwK1V/IyeVhec8XsoKNtMh6Vq9k
sMCkk4cZQXniDoI94XHbF3LV+7zuduTAh8xHgcLmLmYxVouf1K66qaYXgzYrmBL36JfGT3u478J1
QgdcPVMx24BYfq6GMpAF6us8p4o8EyxW229p8kLj4RlJ03xcC8thpz6dCio1VGrJMgxJG4uRWcVe
RBZY5ZoZBExpHw4mFMisZMxnWn7V+gMB6xiEMa+1kqlMJ3BsWS9SZ1r3vrGHbOhUA4h48Q2EGPN3
8EjZ919S8Jgq3ZpWT2xmD6KlVv72U5HR3f7J5cZMTs6SHSz50w1Tn/4gjNElpqw5vhRPAvZunsXr
Wcg24DXw3dILUIVQRIJKmKgS7uPiR1csNDG29vsjAUc1NNskv1+Uf10KONWG/RZBZB19o28QKjse
kz0Y7PmaU6Ep/Krlry4ibU6eZYs0g6gdEnJiDiX1AYGCa/n8akBCO1Orv+qvOG/yDaTru3BmZC2v
4Q6vh+XOPb9OaAAT7nkk0EfdgmOPH2i6d44BQki07FNIWwSMGCAHRq5eI1CXQpTQ5oYbEWot5+iA
0/n91bRa3SHX6zgSjx+lKf2TkfCit/R04hwY5VpiDO7WbkVa2B3qqqM8pjF5BeXceTXKTjx+cL9Z
EmEYNjx3s34KvdsJ2kiTFHLdgSFK6AAi0NJWMLxviDzMep1mIE/Ml11F8rb1lKP/wNmhUIxXuX+R
bR2yyqBJCBSA4eMOoPaaQZx+IClccSO7xyWLNQZ3uyV3ZuBTtHf7PUnVe/gYr+3rryv9JBQxeq8s
LpK8k2Si1cumlhwuCCm7lBbacrzwuJcmNtNCuomN1fnubi/6TkJoLSO2tkVhLAVtonq1KalBM5MI
o2rR7ydn+gCdxXg3ov1fduvGFlQmoqZMStDMcjp4QEQ9G6uwWnBKMxTgsUgUAEmh+F1sxobpJPJI
kEmknXLNd7WY/ZioggdkBQxmDRGLU1tqC8J2mNGP1hCjNBg4vRp/VcXWonjmifh5q2Qr/z0EkxAX
N3b5sZaVZbSd8LbVQHg6uxABJF6EI3BCYngkdXMVLs0g/cspwfWcQk4XaA64fBGfHNZ0qx+L6K1A
13z5DdPPiHJqL+8haf3aBD8+5+vVVFVGPqkQobX6zbanZ25WKgZ4wVtJuXZhEcEa+CwTZes/LZ/p
6KVt1LwwLZzCjKYE1iXIGRna+3aBYkL2kpWxxiq36Q4OEIDyUdZf9f9zR+ucwmgU8e8D++5ZjMki
B4rvZ1+Vm/zQaxrkOhPsZ2RF8nH1a2JSja3QX0gcgSDdnvM4INiyrXDDNqk+M2sDECd/n11cZ2iu
T0lBR+eOZXSImxLtt0r4oVwazH54dDyDWR2eICB/lbd0UPdDQHLrx21Ahz3PKz2ElHP0D2K87EFD
RcoNyRu558ADw/g+stdX2tYDNgmtjm3ONc2S2GlQQspNcfh7WnAGB6kPsz1Yk+nZz+DsEhOokLa7
yle28o+wswKBx2evaBd2LuhdYNpL9KgFdQ4vlD7DNRDkGf3SKaasHr28ik4xM334qCGze8/07KVk
hpE3/6So+UNzQkMsQyCAX1O56dJzdtYNbWEW0vpuXmHuy9kW8ovGrob7v0dmN4ZjrH7ePM7CZgrl
cKHDq/LbfusRdhhGNGhLp0OfaZ9INfyPWx+VBAOsQySH6NBVbrVjuegq6HTqxUWxkCJZ+K9wPQ/i
A1+o6e8lIUlGCwv4fmQ+GXPw7dTYx5cV9ZX1HVjJDYhnTrBM3ngaGHpkNXORYBEawEYCybxkeF5k
eX1pweZlutdkccjA8mA7sqBBeIvRZ4i/SVrVj+KvqdX0rdnL767JKfkGaC/RFbfN47d+1V+6Cq0e
bLeTREYRgoGrZyKEqvpOLv/fnt58CVk87EiKadI0Qw0R3TfP74ukNUMRe/9uztBeCgFAv4vRjQ7t
WP7lbScORQjGI1FcrpCvKMIQXnzUyrLAbA/bqF3iQ/hpeAnkvc5O7zotw7XZocsAQr7sV89KgD7b
WG8yfxA2vQuXJ/Hkon4bPjVgTJce7GN/sqMTABxeChfHgSL2jnLk/6XKCSBFieWIOxSK6Ve366gS
VQQnP0BhL/qbMR+8bdEc2VeI7fYPAyePzWtZhDZph5b7Fwsg6VraeMbbeHY2KhNLtsu6IKJjld7q
ak92I8a4to1WwddQuZUemg51bdiokc2HFDjzQNMqzrMsUpdcnnzMZrrXu8tXhXtIDh3OCic4F3tU
1vImh/8PgQrEVCU47OpnaiBnYuh5E7W3OHpOsRdEkzgPnCp8BiNfs2HxzNGTmEoDko1yMykhuJXl
W1/MhMSE+FeSEfJbIJX1v4d8NszH5esdFFCvClu8f0tFzCm6zcSTWMkTf7C7Zfj2srG1gHpP4JkJ
FxiqLarSRCv5Jlv6reK+52h+tDcSSY1vx64VMUwwsFlJkHnP1EUhZEtLPZumOR7ATujmEx9ZX5Ab
n98BuWDildaxzf2aS3UQY3YgGDmpRhq+tUTzkl9FC/g/C3q+6+n4R3Qdsvhv+GLrnrwg4ADlRw9s
cgePBNux3mkNSj6uxMobmALsKULpKp03Xji78LYFuaCnMAHVDES+qLfjzkoszkgt+wF5j9sJCQgB
NsSvMOunes1iWzsTZBDtvuKw/aayDJjtDgqB1XrT6OkzE9zHYho2aD2xsTl+qHMPFzC/Zd8QrnEg
vG/n5lEQre2kIzAT5zwBO3Q1dhFqswfIM0c+G9CRuyzsnhzcHerbu7O2mYpdgU/Xc0hghKoilXiO
5lxdAW7aSvzpvPEaGB1Yhwx9XIhrz1qiTBFEuCc2nUHu+bepI1r3f67Q+0BsBqTQXvWyBOyjbH6O
/DQ1zWaFGORGZaPEUhiMJMt9ykBs0HDBVetHOyYWWH4OXBGvSEmLh6K5HtzqHv99gZVF4+e37l1L
6bzyFriiSwH9v3xWLh2UzNBHT0F4JiE5Vkhe50bCdY1hHoyS0YHleYcvTIFSgHl1ZpcFFZ4QKkCf
eTmRjXnGZpIENfSf8Qgq6gQIdpdjfIlB6LDdtifE1A1NXOCWTcL02qR/8RjuTCWWwTNxSaJtonSn
vH2LWLGGyAV0rsgum6STFtYrurNP+visUob/ps6mNVUbU0eRteUutE7FCljLmY5k1hhUpL6kvuLR
vi62b0FUIFMzuBTFcghln0pevQTckUA34BiWCRiqtfLXpoUmKqwbgIF43fVX2vHI5rBNhwKBLB59
/NDtl4RrKFSdq69n5JX1QieRdW1q2C2JVId0yagfBFgntEapOxmAlaOoYUosS1NdQXpgM5sJ4vtu
coNGcCbfN4wygWSdJTl9bbNUqk7LBpluG7mbiDc3aYidT7BVJhEDs4SmNnGtBl3qE6kEAZeD1134
FGdDrYJc8qj1Nd7YWuJn7QWxMc6HN91gq43IjztBbIDUKGoj308al1ZzNSHnLIzvYLl9hx42n8Tw
fz77TblqZo3miscfJVodMdSwHDorhShIZ4NMky7+6AXOdLIcjEfxpE6wIa4kQhlcTBMN86W+CCzM
FJhGVMOys3jIYUDx5KqKiFUx8mDWUJjKgVct6UUoNUTQbChWA25GO7LkVlzRqF1AQT1pRpV4Gjo0
tdVng27Y1O3kttZ83t9JzyWtFwYdHO5wxhd1KHV3U5MyFpxooF2ZIfzsloUQllJaWj7hsVLM6/2o
6ZCQl+07Xns0hNm3elL1uLmV6x+Rg3ZiyUPq+vcy2YVjTZUb3tZzeEXD6iV5BCS88Bqgg+S4iNKL
iiu3FeiQPS+gsiuqCkeWtVkwmHNwXkZnX8tMVXdNqEiWAVvdsN6vONnZCfzFpK2iF9HYMD8GabDs
UqXYR1++yPk0La8SMQ0E1nsp3b9Cl0TmV55Iq0qK4f/iBVfArPpDoR40ixrKeUOQ4edDlaKBAPEi
0Q/DSKLakA5O3Pwyh2/r+ozmEqHPPm48P4uI3W6J/LroQQbLgZvd7YZ27fuI7q+W+WHJbd/FxJkp
cXKPcJDgnIzMP656yihHyeFmTvN1Xhr/D7esYrvX0xB9yIkN6q8eo8fmF3Srs3galwpSSsumMmi2
XZAAz30gIPVqXAHr80gQQijTBw+YdrKxSFFcY5w99EmfVvNbPiDenZUVA/TvhoI2UBI9lue14oyP
nTcoo4ojOEKg1OW9Ml5sdWMXzk+n/m75sKmrTzq32/akDbE0AGmhFnOFaJHfpsZ+fFDlVMzXbCPz
CUk2OPYgnaLL08lIZlOpWYsW/2u30KM2k11jY1xJ5OOC27zmlMlmFNK2xoYfzxNjgShqPlR3dVL2
zufZlPrkomY1cidsVDM9CA5v5CXfrMPcURo3aARS2rRvUp7DoJr+Ia2Y0aG9NxtRJoAt6Xc31YJv
oQmKCb+e6AM6svO6IUBl4pemjiFv4qCzNe6IZBB72KpPsC0gO8Iv1nih3JGls0EWhu3p/Vnem/TF
1HRACbkQfMn2tfLbzxs2mU4XBsVEDw3z9AJHb/fofwkWblIUrtuUl9MLH/YaXfTrVJg92fC/eNQV
B1oE6TACWXl8SASL1T38uhc6UePCuVFrWHVo9npMZ6MD00i8+9+G2/CDva2N0ZQMw7PmVVj1KAqr
nYnTuP1qX0JXd4qN7mceVVTSfktS+tQtGGo74jD6C97QyXJsPWBvUXL4MSUKil7NMPbnEfSmfxFS
3Kk4XRrhtjyYfXvTJBSzk4gRROPsSyV/v2X9W4FpwzFyQ9bMn4tV2vuitXXItbBBd3FLjh55QnGB
8XUF7PwVVlmHEEAkVlxXkTl8hLkewRltP+XZg4Z1R0g/3OG7yPYu+rs7ON5Xg26DTyNFmIfm7B/A
cvc3tkkQ3QRf+88HCERqNzyCiil/NP2uYebi/VpDTjeApIHR5PaTVbhZ7JMTksjltvBEP5F+O4AD
uKIquKZ36xZMY4WgRrEOfKmQIMh5dto4zv8E8tCZUiWQAjhQ23LbagBNOTGYtI44B+WJGTdcR9QK
zrGUv4ZGWzsf7MkFFMzOVjoFvwgcbI5FWLdoomJqSQEivSMdglT7uf83EzPPZzFLQFLMhNbwm/+w
JIEXNTirFjA6V4xT96Eh3sASoyyIaI5xmSw9AyrIJmyjFvXZqyD5DWLr1FiRRSm8CohMKl0/u36J
G72ZiF8obH7HKfVS+4wJ3ejOM974yDM6zDfzPFILaCSE6ICMBRE5qakij55kzMPqmTNEi8cAKLyd
nXjySZnrBhOJa+v/x3/zjpGhlEQR6UARUqZ9b3dHM4m6VQruTF1hK4ga8JB0fjWEYLaiB8F8tmv2
HPx5DSNCdSqWWWX+w2HTeyLvMtmziqs5hlwRsth5Q58dTD7nkBl/w352+doQfnWq/9/4u+fzr71V
eOoXO4MuP2DmkgVCjxRIsEZ9+dVSpukpUVB6/B1Yfly59GM2wxrtQYQvxVsHXmj7EtudwMBJMUbB
CkqgSURm4y1SPJRxtvQgO05csncSPGHd1mcImpOc/y4Rfguo+MNH0CtMVV2MaHsWZWQVc+zw1ezA
WPVBV5yzo/fOsyERBC0cZLe85IvFH0jb/cuNc+tibvJHwHYKR7IBQBlA1aSRSPohl1of+gvAXy3O
bXw5edXFllhLfkr2tDgOSQqMgEefSrcgUNF3o5mJyUsByy5GvjrXqyyKgKHaBGzUbx/R/J5x7QMb
iB+NHss5Bhc5Odyr1aWg1orwrjQ7OKt8sFhxlyOiMYmZG/bmU7Ctbb6KLGdF539IdgLz4SiKePJa
twjBv34dN/ZDzkrFdg9r/Co2c3Xjmn0Zva3FEQg+spWldjBSejTXdT1TUi8oOnIlcLNU25xo+YlO
33bsyW/6aM84pjWRPyjUqc7uca5+RZu6uSZdoankE5xHJQx541oOVhWAfgJOZsFCtEJIglVg6hOF
SpsaN5oZIiZarswrcWwITC6iVM+J/Yb0aXymnHncVXURIwlPklrZZwlLyRlvBtsntnsh4nsZdOzj
AQmrmCCrW3behh29LpKyZ5me1zvdnFBXi3qHxeEhI3CNb0rNd57IIOtaVh6GJPchrlo8T7oqBvUx
KOYTIEP5XjiPIV6D3nOV+SQZOxwKQXI5klghB5lafZpou1zyfysBykGPUDKctJEzfyDImYkwgt2L
hdcJzF3vO964MsmAYMpPi4q3hJa8L6Lc5hQVih6b9FYFBxXHCS60pH2bJwU7bhICJ1TcWwsWBwVh
Wlosl4qEx4FzD7jsbOnLNw6H7705Nj2OFJS6WYKzszMpLTN8ZjGsDA2s2virAcY1LbFQzN4EKJuZ
mZ+L/dwPe71zroxH59sKkvD9dY4l6M8NXDh6dEfVmyYEzXf1V7xGdhUtmNGCy9rc2j0SpQjIme4Y
chnsNIJ8uKq/jsmmpgd9LJ8AF4pZeNNxu3Myg/NUHXwDPrqJCJPsuMsCfHXBtnGgn470zGI/TlwH
lTwteWiBymCKq1kI7A/o4JqZilnwf6JSyPN/jn2q37XcyhhnjZm7ZToSFfu3tC09RrY1UFQ6FQvp
vFgBSJp/+y+qtPrI2Mw4S3EXKP7xBhkemly4N0IBLJ+ZODUt396LmIi8VL5yXNDd9VHTKiGkqE4V
32XPVSJyawfbNULwDV0PuLlF2RJH/dGg67iGjYfOOksq1KkuB7IQpMnd4aRa/4q5I5gi/6wg6mWB
BWPlW6zd0NS4lOFgg+tWhmHpfpk0IY9OqqECG6gz8z046Sj7XnkTm4zatKAvxWjkIw2CrAPZ/0JJ
FFJQSQ4YfkzpLoqwc0D5jUCkPja0Ewdb/7Fm9GIxeqkxt+M2cn3ZNlNlJD8Rmt/nwyRhEN/DHDfl
69fnUgw2t7hHqgMBK0+MikifDRNJd3VqPPUrowFHz764PpLZFT8I0RjN/c74oDUMSGy4q5rdSf66
vEr6UCaqvlApH3Ok8smBWfWuMveg4+hjnFLjsDOJcKUCMg5H8ki3Y+Dg+0frCj/6w89mcpwPk0Dj
WDd7dDq3nO9yzgiMy+ip8KtiBZPRgtR/pq1EeeylMDDtdfWNJRxMDgHjKeKKoV1uyQFamoVAQEnO
h/jjoUfwd5C8XlqoDKk7vhxYlMzpV5aUWAueSQ7uBOzLpKTFhkky70f0pVx0/TfXqXZQ/330TEKs
liYsDDdihaRutvbk5Ph+y7Y1NLX+RhPAYWji0+7vck1ySMJRV+nDvZyRf7BFJCebhYFoiU3i+1kv
EH4m9LTDNIBuk0k/1diiyBn6P/ig6AiOiXMt4Oh68nHFXMfL4erv5LJhOeSx7j4XSbtQtDcSSUTF
IfX89oExDHPndv8eEFBaQzOeph3JuiukmPalEr6tgN7ZW/wl36cD/0cb705zd5J3Y3UkRDafba/2
/M+xYdY82Dn6qfDXCcmHYEArqBGQE288f7MxxRP5yPs18Nr5CgujfA12oLYCIEx6zkHhU0LcWrB5
CdcwEr6vZe09lEaqsfvUSyYiL6LplGaRKluK0ttS0D4qyU9B0qw3Tj05wy32YgQGBDglIB7oh7Gc
e6tFIbmSKfpgrO7NRbk8V0p9lsKQTxvq15N79ATISpAuYyDGyige618JiVNZJXeacIjCQkyr0GCV
lPRuUEsAsHjlM+w1EFaNtXClMQB7WOX7mhyDMUOuO/5T070LkG2uZf+HUQ7n8qy1bE9aJM6hzz7Q
xxyhuPl+ZDytlN1TAji2xau5PXQ4HOQ/AH3Fb9ia8itv5fWLGnOOjfSEjvpB3ilKr3FdGGVepbIu
7rSojue/K1u1U2A37bMGK3WZvrjduk5smuzCHle535LRGOyeNdl7iQwudyI0A82KQBXOHuri3hSU
dvn3ZcyGZrP2cxaDHuYJD7t/zHo16INQ+3XXkY/HyD/T0IfYtpFaBX6eGwahC5hj/imqSWxvB4iO
hsNxUhOVzs1URzBrsK7wn6WKpG4qkmJ8fI9JxnwFOqbOhi/h6+g363Hd69PTn3zyBKorllDZHDqg
0cPKDE7M6tLQ27ZQDThblLJGAYKHyV4Bs3V3WiyJSv/1ghpUwg69orvdjFeX5Rri2uOV6tJycLYb
+j+v+537VstXoThWTY2wY+uGfhDHeguiCkB5NCNg9JH5UObbGESS0f3I4X7JfrqGgidcEAevgPR6
z/WrcF9qgKW1f1rPs7pMJrxlZfHf2ZKwNKkWMZjvoTulbmfxuf4tM4Vd6GUT/7G/vPZkAvr7bZ8s
05hxryiE9ewdJedcYAD9xHWn3cO/a8sNfDIpehPwP7rb0s7iWmLRFOGti73hnDTHD3rpitktZoo3
KOTdjQt2sBa6+jGCS9pnkXzG40lgBLR3+65VRx5ugYCRhJeT2IU7V9RlIZ+8HnovagRh0WkroFjC
RhWLO4nZTs11MQAqcCsp1DevVOGc+16noNZc6byixcPu+DJSVYr59PxFVDlVlejvAF+cT1lWz86R
UIGrO28RQDQaKk9eJD2cdXt/4XDnOfoIM+2TO5UcqZpmWcdrYx0TZ/mxyK3pU+um8UwsHev6BGdW
M4fJ9U0MXVGNIvIXWcZe6xW3B/lzYRpmBw1XASX4kxmMhwaGE/UTcR8GTrHqTwzpmQSLLlLLYQ0O
/SzZbl8jmwUQCM7Bwo0ylzFhaRvv1DqxMwr9T36yl/HigF5OSvxS/r+aDOCOK5JNWMRiPIj7CQs7
mcA5+IjIW5/S3ElA31mD/EnzPTne5iJBH6hK2ZuKNnTaPWiRVZ7sMKwJS39LYjzNdbAh8EQVV/NY
0ruzlHlTH1gyj43KvWBHCLAxm//bZ5UN8BpehQqMgofu3NkiK7jvAsUsa8lBFOrc+TTf9N1oDyf3
sxsEL9A87J9UA0cIYWn8cQNh665qV0PiZZvJ8mTIgyLh8T3QAGIpGdEkM6CsKDhnWgJOkDt2blD5
SDhohhxYCdLjuDjG+AYeIWSC/GgRnbtkrJwvyJj6uQMABiOoGb8T4moKx3hHUTFsx0euGk7vBDEH
Ydnd8eOnOZk0G3fXYEEI12AHCBuPelywX9W15Eqpp2n7+DBb7T4PzKmZLvG8N11J+nyPBpb2JCQi
IPXVQHhdsgv2kA7RXVJVdT6zMXvidKSyhdJazDDXrTV5W2E2wCweM3z5iVA5D4Ztq1f7mFNYfop0
H3kaWcfjQS9UotoYPwWx2Xz1b1PxgX+sHqnifehKjkfR70eF/Ac86MJyuNrjD1NPha3q9ZdCobU/
9ecAfeHFXeqc+vmsUwH2Z+CrQjj57d/M1iGQmHCrim61T6NhnK5gev8BFy8awdffbJzi2DPmweh2
vGeqQq60fe44n1z5Sw1xIiC8XHxrkaZQE0GkYvICQ2xTR0+o/TGBg+Y4dBi8blhKIRsO9E7Af6Yc
1XNUds9gKnpo/HJ6YBMsHYf7a+KsYA5vmwCc7iEUcUs6KnB/unFLqsyQ6OiKofLZe1RFIFtQPtJu
cpC++3VeSJEp4cO7FASjzV4KV6LoyF2thVnerg33oaN+uZz4sZQJ0SAqcSXrFmUSfzloKleyAPKz
q5eoxdhSC8lt/v540t8SNRBN5X2IE8bPvbkAgUuxch5karGJurvQilsf7Od6jneQPQgIWgfLn4vW
54bulHtLjU8twjKSnGgLRGIMMO7NYfXRB+riyFP/MqBCsIhdWOy3sD7tDL31O0Zf+s+njigu58EA
yhEMXl6Y9F+PWi2kbey4ii7TV0p9T6Lh/Ri0S/nL0+34AVuSgwDGWi+hcxXtrkLr/9ol0axhrP6l
4j/naygoilBf0VSekdoXbwIhsKo+va05/lq4ymTMBmFwLTv1NfipVsXvQvIPtjFMvXmqlrr0BsEL
GIv/eP/6LRUe5Azp3gT2EGgKkkO8N6JdFYCx+onTqAG9ek0iKl3u6+PHMbOLOm72/Of83qfX/s6Z
1UC1cKWLcdNCryjd5+rhhCP2Ru4p9q0Y65IcHgl6z4Lf+rqoQsqB8D7s6NuV2EV0OSiP35uRUL+V
3iNvc2Kr/G/+aNW0R/hbJFXW+ry7N3SHe8bmjzilxPrH4Y2C5/yRon4DcREL9U2nYopFOdSTA2Xt
rOOcK9CVZ5iaCRRQpKUHswSzYksGZIq7tEq0cqbrvdBV/iyAs3Dg7PNkBzv3hI3QG+2xV/tdw8YW
uaIXCK8hlStHP1/XBeqY39bzj9gmeqA17Wl4OWvL6bciO0okTZP15ujwA0o4BwyrP165neTyNv9+
kTPv8TY2XVUq/eqNTOYVI2EkwrE/u9zohbjVeciXYHbg6Sxw+eh+RxCI/1NRL70jKg35zwilF6sV
jXurnM798eitAKShRfkUDfDFaITrO4oaanj3GeMynmzYlktvkVLt18J9DytZ4ms9Pzgt6Fu0UodS
ByTAiCMwEA5/YuX0yqyfKitZv6lqMfYs8ZmkXjoCM7XyyoOK11cd3PWZpgtEoza2HqrIge1rtcKL
1KmwV79iW8ST7OLq6WFga3BwPQBl1vKoiugDcPKRSXEu7a2yLiq6fbCybSWNGoauhC3clKFRY0DW
0zl1qozkfI7ca0zA9ZPhqRHmXpElGlCzmWerpsHGCHf9fGIppJouPp7HevQjGV+eBQm2g+Sbb0yG
9r4Uxhadp5sgmBtEaonot7L1F7/uxXO6kGjjCRp3ultB9vIqY7p6+1dUOm1uJS/TvgDiVyTs2ZWs
D5wCVbsLiD/q22WW7RWFmvi6NiSS1pW14S7jqmR7YJvbseVS6YoM/96Hnce7Lq3NK+DDD0Ge6rxM
u38wGW/COWgwZ1FGw6MFToaWli8uR6QU+FO0CznDuaHXlpQPRinyBkkzrqeGT/6BWzdQMDveQxRF
k/OJehI6zwuRyd1Cz4fFmeOfonpvVA/K/USzYI519K1PC+3rNfegEcqwfoeqVdGzk13Rnsrl8jwA
Ap9QQ+m7T8TW1GoZvo1oD3yCXR5UW7vpLp53oTEkdPp4upU/xLwX7NHLz9mBrljrbw5AYLnpKCl9
1ETSZ63wwriBDdyTflp0u40bGPkqreIagX+61Fj66V2PcUc8npXiQ2ypZGRxKWWYt73xejY7UYRV
4BYVNx7hONQqqUo92p6RICrSYM0/BQYrbxOBe5OP8rRk4n1lRCNXFqXYXJrkNZBeWUGzAoXLSM8W
H1tWuilYUUXRXj7wzqXMg12du84Y5l+LF074iAgBJbqxzRfkYkKsWU8xN5wNcJEEqketX4nMj5pg
kuprVkRhmiLgtw++4HcTGDy6HGJSePPqa9dao89V0aVKebTb6D09F9RU1SPDiQXR8dlThWu2Pf4A
hRj6qaoKiYHr27pVkufQkAi073G3Z8ZHGrT0g8/JhLf7MRO14b/YYVbaxNndeNYxu3YSeljl9RbD
QQ6I4OjCW/gcc2DWEGe7jAtxys3BLM5RXObbuZGbtVp82A8zeTbfH2IdLWoZ2vreX+cVZ439pxYl
TvVNEAeosHSPiAgpndXwQ64GxY3MYjRS/6xoU7Y94mzNyB1db1gcgEYKfSDu2LBGxxik46RDzInA
DnL8we8DDQrX84Pi7gNtAOT1C/S5SK0VB6QJcJ4a9qH5iGSNdDslTuk2MIsXRJnu0quZ8AkIigA7
yqyhwOdK7PWsF6trFT5TVxKMJQFxxFbcTyzQ0EdOg6aks7Tg517u19CQ6+jIJwONEliLtodJsuHD
wpYL9WkQHr3UXtmsB+h/cLaLUsEV0Lvxv8es+B2XWJrhxZVmADaRamI7LJptDi7Ssjw0jyOVTTZb
VE0yxh/4V8yBJI7q7bHpP1gkvkzlQyBcgjgIAsw+SOn/Bo1j4GBESBEjsxGKF0+ktoEoTfJkMRy7
DZjwcqJt6/5PX2WknjxHfFAmtrl8LTfETcTv2g8isprhaVpOOtnSf+sLTsmi5Y4SDj1bSlJNmEsF
RluBa3HhQ/0DrQpVvD+e9Usp2znu56zWWSfLODRdemL7FmLVm2RcXkJT0G6gujBrL3On7VobKPdT
HWa5Od0JIbZI5zFx4/72LKSE2v83LY/OE9QULMLbu38+d/8LVFI8sPL6FDZnzwXjcVShZsYRhPNA
2est3XiOr/BPbfTp9uATrTw+Gm/ZbK7MM+gdc3tCMm29fsI53Eo+ZUUwSBCFHLLn+cNOsfxSMyZe
5fss0Q/xQhZDF17rGwqe+scOaN117rKzW8elR/+cXK7ghTIUjncFYohij8SG8owrFBt0c26lM4Fv
3nIVYj1XgBiaZrmOhP/Eu+7gs/uU87EXHLcUqjS6J+4kEWWi3j6f97zmph+wGyReAPVtIdk7uzFy
un6/f3TOcWWyd4qL6rCH0cahwlwXCInFhNBn0/RLSok69R8bBrg94iQCyCDLw0/z9eAg0iCu+oct
hKuRn+TRH1Sm4Ryj8xKtSFaSFndPAl024IQ4lyt+pSYlY8vfJH5xHP7oGnnj6GC49yQbMOQ0hrBv
Vp0cXgBXWwIbpz3Lk5GPyi0y9eTsdBW7TAq/edAFq9PXEehfjeVDYlQKVnO22ZY2s8QEvue7Geuy
eB+jldcPJ4cafbXvJGzaydv/mBw4v5/x2TOH7HsHnqimmPF+kpBsjbz8Zobd4LacepfFclyi3sgV
5qexrtI3qlswY+8F7GMJ/hr6qxZazEF4opXx7C79Sg7gYN21ZwjprGQGB9CzXj2vn+ZBA3Cy6A8x
Q3pUCFuT7ETEm8rtGGzxwup1+/s6Ggu84NH8Z3baCJAsC/bwqjm9+riXkHW/fnHxfGEtSbf3Mlwq
V3n6ojzR2s6bEJeQXteinI9mQAR0M1LP+PkWUbWRLgSbzCUNz30x5MG6ZSLij2TAxOS/KOVqYUBb
+1E0pJAONG0+Gc8PmSxRH5u0YyNanVMpCIGLUNvVxIoY12qMvxI/C3YPrmZ2dC0R93qmYu7VPuDA
nq061JGrfRXr5/kbD5o318MpiCZYqVyxOxJw8ykwbmkiSgNsnQVQWnpxD73lI7m+SYvfYg0Kl7jK
BJgms33cydAy5SGd3D7OjH9svNbrG4o0MBfXS91oCIEoXOGIBI9NlDyNVAl2NIVuYEXfbTKFZ7X1
Qihd+bpIHr9MPLhSlsXWlyRUo+LIlsGzhBW2XSIFJizFX6cnrxxEcerH5rZjYAGhgCokH4St70lM
lUBEBGE1dsF3BWHtajDif5j9M2ax0BW1dw66Y6EkRLlmM3c4h2Q0me4mEY/zrQrRJnqe7zlrvv+y
OHhRwJaEotmLzXF65+iSRMSxh+xyN0shB24xrjZDh4nZuszsd0H9xjVluy4ipjMEDlChlr/dSa01
uzjmh0PxLE+AFOetGL/QvZoGkQgeP0kvJCHChhPMHQS3eCmCGWsFSZoMuX4WUOVc8pBMPE9LP4lu
p6NUb0nbX+/Yysqw/C7JLesBhtFNUn03Da+NAVz/tCzvTsm2gOJeuoINJSP0iQdI6m+84hrtkSZh
DL9wAUpgpLsPgs16FgESnxcaAAcRAQYmo59rAUCUYkBPfkRfrFvN+av6ZhEjfhokeqSIdZsbshBH
oLruE9bELGr6y0vgU9JqnUoYRCoyrz9pE7BfAuUDrqdisgWObevit1dCZ5JB90GMvb9BCzAc+hbU
Q59KBAa54PVZaR9VKMnKsGJLlL0bMNllfLGfSi57fLi+MgWVZ1M6rFzXNVqypD3C8mof/kGazEXn
24AfrmPrY++YhJV2O4houe+O8SBKL0geW7kMx3bjrssvmR+KQkaqZ84gw4g/WHGw8TOGcQc4qYxF
qULfhHa4jn5Y82/X7S8+xMVF1n2UIRB4fu0MVYyqHy0BElUM3gGM8H1rjipVmDryCFerbWVkhQlt
sWOjbB/oFPZ6zw9CoEQOwgCbl1ByuEJaOzwvzFnQRZVKv3Gz98l0PSaSpgD86Cx8a/Y/4pYsjgiT
64JCICLeBeK+3ILpP5nGyfa2R30ExCkruCLlwY/S943IDHVlfe+O8DdcvjTllIU07O3mxg1kV0VN
KD3RMAFSn4X8grhqvT2gYA/HtI0fBJrdAtxffxw3FdTabgMZCQeKV26As8nTVX8rwYmrBcswvHfF
K4vP/32hAnOqost1GGH6RuQ8Wq65u6IDl0QYc4CvEbu8H1egJWva80K2ogag4v1rLDwpNJDhE/XA
EWju6wtPIMRggm/MEbDxvym4hqrRLlEU1nnGgUWRxAYesP/z+eqdIhnBUDqHPLKThKiuvYNDDqhQ
1/IdRdOvc38rp301Eh8ByWyuqEJU8q5/KUu21EKIj2934xztQxb1SJklqvnCDs4an14GlEtDU8Dp
bHl+ZgWsJUT35ieoGMjLUEwoZ2lxLDkvTkEsoUAyQlZy+VL401ljDQ12EReVxjb09RisCPZ3eW2q
bEbeMYTpeYiU/HUC3IrxVaSHCDaGNUJ2ElQKEte5l0Z0uWN1Q++pE2OdOlV+lAWLeGaA1gPKqyIE
KgSAbnNItwf+ZgRflxrrp1YBskQVbZFNXyAHx1r72YXTjTeB7nZqvMM/rpAQum8iem0+RRyrQi6b
/4nkeMJ1/NZfGhJ8qVglm2t3yLO3MUzSMvU6F2vJ8ahnM5ioWWqoX6V94yepEjEL6uLkv+Rh07Cu
avsKx30bLuxTY06qnzQR6TguteqgbmQOXKGP+gKAOLyglH+WWvJAD+esqVT99tudC6OEwE274/Ij
2suonD3jKDTPoFlNKMJHfowo2VAh3WjFokXaQARl8W+uYcPBl9U3/ymqiakOk2B5tDI3NWnAV44O
XOV4UjkxYF94dBa9yxrN1e0RLG4Q8rsrX2iUDgkS4VIUYH4NN65EDCBjV0r43b8l9PCuWPmbMbm0
r1lNI9VcEWAqkY2qLjrglgkxZYpqZqoAbWPTgCg7e7pp/7LbUDkvm0DSgOYigcRCTeOVJlb6Dmsb
+I+JC+DYaVjCQhUvnWlcb8bhs0RRIKNfRinDtNnVdD8i4FJALKhMpFSe4O70m2stv0003zDI9XBo
ob9aOjecQQtTEhe8rMni8a8FGYr/JB9+TXMBVuKA979VLAzwP0Qy61l5XE6fDWB2z6SsXOI7DaKa
LzF0GYcV3C2eegMjO4Ww4xlKD1a71KeTqw7P/s9CNU1xPnhAt0pV1Qsnt/DtDzs+Tsm8gfRTC4J2
w7KGUJRj2lslQGZH2wn0y2msliOg00DArJDbTTSMgHquP0sFALpsWNOYWRP+9PlPLXOw0Lku/tU1
8gYSJKJ7aMLzVziSPUsYjtvA3DDxg0bkhBar27j6FERYCxmx4xIj7c9x/P53SYTipqFQEBHJAjWy
s/3LhCKFqe7FyPGJXKpCX/lc2qEUvztQ55E+gmPI2lQObc1loczmAlPE4tG8LqqdDZHcg65gIDIL
L69QOhU6CKW9069iZehrrXvbh4Fr31ySUZc36hFlZ6hbvnaXBTsFZyak+pHiM7GXbfhVy3AtJeqe
G4ULK0rtxYLT0uJVh8wyT4pkmQBz7XwPs8WxJh4oDcufWw3NOYel61vLvrl5rfM9oYtfoA3XD5YL
9tgVmrr44cUF+kJTzLycQxzVCahb3gYxiTizje2spqJnwJThyV/0TVVJ5sLq+VF9H9pkEQy00yaJ
TF6X4GXVmc6Vff1K51PBGY9y1wdwr3AALJBOx4Qwi2VMu2twFn4+iRqHUuqt9BML3c6apeFGUYzE
qetGkErcaoLh+q3ceEiMkie0pTg92kpvXlcKnITVgz6/qAFa37oe2di4dxPEs2wHDacgSy+mGEEs
DoJqjrVHdgbMDg+AJx2tdjSQeXIlmTq3OLpohnFZBlzaMeZVtlfJCi0o3734rDELtndVrSpm8yGD
lKCxFu/5rw1ML3XehZ9Xlul10qNSF8ZbamtGFD68ok9EuVL/DQY5CVI1C+rwhoqVs42EdJ6xFuZi
E6FJG964UnDzjeyWwoCJRYjCgH3gPVHqjnoqKlp8kAhlIU07rwAEfQIa3KeL4bLJp99jAokrkoay
1Y8GxyDei2+B8cvrFcGExN4fu7Z/OfIQ3FX486oPc3438nIuYWiIgxngyegIndUuR6hvFMchoeSG
3fJWLdjlfwpq8Ut6qIcVmiJoMqJp0pQCTnfFeQ2GPVcQC3BEt4174hp4w8ZnivYGOBb3oWZppTqi
8lkd/lS9Ew69Z49E1ylIiRHRUgRUELts0XBd/1qEJ++tQP0zX9HNd7v+OkhD+krWoi93F1vPQnzz
U4t7O5UaL1ZQq4PZQ6cTId9aHz7EJy9nzF38o7mZSnH5MQ+/5v+aJ+QuoLa68uKoeKnzyhUJOnCT
cp6CKfFtLexDcUEDyQMsc2WasdZG0bC6bIwZw2mpY5Wa52Ve1tPcA4QfqTYMMpxddnbf2qzUB9KJ
K1CLGtrCjZk1GG/Fep3GkjAITTF99gOnrBaKm0q6S3BEKUeTGcGjSe/sByD5RrQUCYWvdKQBq1ed
ROPD/UlOoBwdRWZUJ/1pgvTYYFSr3cp4b6TaT1FLudOZJQJh4DwR29pziCJWg8cEwJBOCSt90Mw2
v0nAW3G1vMps6Io6/oRUKcfxLLBybCoLIh+qH6mkwBtC26W73ZBvyAgl8WEFoJjBWr3HGJXZ9TNE
XAA+JpF72zfO1osCw27PnVrPUTweG1NqTGthGJpAP4ewRCW2Ig8pCmVgkYPlEe+N8NtH1LRf5neT
5wOcfnemh0xcjeWjtwXDbOQFrliCI+YmLChD6/j56UWBno2CkXu6HXACxJXfkqpz9xBBCyRiKmfS
VSH+POJBONbDLaJrPulhgh6SfRSOkhGRARGDt2MH/JKmPBVDYWOqISQXLy61vqjxN9PdJ9aw22G4
IvkLFWThPEjf5hWBMw4sZ3yIkXdeAJozFyiRKM6qJhm3xkyKy+JIJEugy/UP45a4huj5boxJ9GD8
lrjux6J8alDsXmYy+MPak4yltY2Q+CXql9u9zmZjFld2aAIXE7g7dC+vVXJQm0pNhAYyUsj8CGQt
BGIcVlNKqf3XAUAYm9PzipFwTPRXoX7+78QIe7GtR4eZhAC/hTg5wVoZdd/Qm1MavCOgHCK9vmMQ
1gNUyVQ3UfpCU1rP/PauVJXofWIsX6Up7pbY8SuW/askTtGyLcm57v58iqvq70t+EWEJflYLDc5W
D2T54z5lV4Q7wE98JHY4d0JSjzm7PYceWyLiCrDoo6M2G+DnHqUJWa2Tmpof6q5S3grQJvYL/+fD
OiulAHc5OwPSZfp/sgkST0SH0cNFDDZHKftXpWdSofc3UsETlF6ws0lbnbOg8s8Szww5plt6L9t1
Sx8PG8khRslTtHlqTq0F1HKfgYJVo9DYI/fwLypubGDM+I+uVl33oJblU6yZMG31E6oE0/YMZay3
lqCa+9SwngLW9x8B0d5rn/W0A9ToWQ+nzDfgTmAqhXCOZN/u4qjOmLW3evDa2Wt4CD5bMUrc9ZQg
de9nCBI+3/idFRyV5d9WFA9iNG5D6+WAXEBr9kxBz7JqXCgN0RYvUpFsqzhazDCNedg7LdwCzo0r
yI9kZ1Yj4fI4z5ZCET5DDhm8N8ICFIyKzJxAX4gnqhElbQUBo12C2j9//UiUwL7WfMgePJQ7Sevu
yrdFCemY5Z2iuBfanxafwpXGCN9tv+mk1LkTxL25dx0uG/Z0lSkrsnredfgpPKYmFd2QhhhwwrwO
KMxalJJPNeAAzpfwBN7G9KcBcHsTqtTQOTWgZheWIY4RoaFulICY5f8wHi1spFeoafQc0YmTeUqx
ylQ8lCSkfcyQ/z9PfDc8GxaBg1HZkXlvFbj0RNrgCzq7eo2YUwg4VT9o6xf6rVcDRSqFyfGHVoig
hjMdZe0TKAodznnGZ+gT74fzFAGXicZcpDXr/vNND20/NbIh1uNshENg0qsXrUJRimk4kJbnhJLq
ffRUdbZjEdHR3gbxNdfVIUR4qGBpwRurbFng0kHlibgriUqUG4sNesuTSA4bNxD81BveeJqh2Rhq
rSUyCxWNOFr5rcU9MWYM/zJ/pZZPyA1XBnwM065/JsGbzssFXGDx/NW7fRwgnwu63SriYcjLQlRZ
JpQkIEjLHxH4eHVopfwHT/DVDvxdWC+Adr5MARuf769v8rwdv92nqauh9jmvXDP2s7uIAoR3Glfp
nFSbUrOdWQ8sXX9LRLOT6QsTd+PFvzw71E7vw1t4vBcBht0wKHly+QL6QN5iRITKDEn9MzgberWx
DjafI1beQJnUH+L+uQvYD28IAlLofLKQXel0YtwzrKj87YT4xxU1p6bLnE9Ji61V+/6eN3qfVO++
3ObmuraRGvFrvlCDzGMYLmfuWW1n9JJue12zf/gEW0VhiMzzBsJ9mZOJWvNNk3k9OEmycqhNl6sN
SsfQeGP7tZ7UIYer4p2/HYEcRRUCRRQaoNhVIiCxSU+IxJabOtAmM+EsU1T9X3GTaCnZxVU9JF7d
7QqKAuLoUitG8yqycMYyjCoVc5nJuCOIHdJskKin39qHDgPT0A10kTadeLxvo3I5LqwuFttBflUx
tol4iKspfHzSqtmLD2JXE8eyeTIeCgInmFeyVtpAoS8abW7AHwNpUe/efTOj32mp/tLe7qZziVGr
w1IR3/wZw8RBIJKb8s3v0p64CSHowobGblQwz+vlK6IpRjNI1QCKwdlTzmIGFa7H07XoBkfk/Tu8
MPHVon15hM/yJTQcAYIrRFCbRTGTUQyuzjeH3yk70XEHvoZ0Egl0H9UGV0wydROB3BpsrCrbhfMP
kxIZ5XQ1PcVFyQckcqm0QydWMwSdI+0rA6JLRbqpD1gEnnO+CtoYn2DHgwMs+x/cT0CziR+i2Sjf
3MGJxWuA5E3SKiW0gMyzA6NT92NLQAMf9ztVjR3dDPkTfOWLETFp8irBxce+tcInPplG4JcfNlHd
WNV3UP68A52s3hwrFbybXz7opecBkpE7A/Iz873ZavjC2CFmVUcUh9GC/t011BilZNu8mJ5N2OJL
xJjK+L26Iw9A5lE+p5g1+gDdtjmJ9gdEfD6q4sO7tXLTeOUquhdAdNSrpJhs2Mq8WSFL8/gUDMdT
7Z/m6kM5qeGBp75YbIIfZo5bBqlb8n8exscIraedumCOCUFCZ8Diu2q3L0D3tCBU7f/oz/NEbxCw
rJ0OzWsERoB2LMsq9mObTffcjJUnnZT7R/lWkxl5jdNuqmxJpAkjxQoa4mhVso0wvFhqumr1ckhR
8bKS4jm+x/c/Otrc8gXVRzNi60Or7oZMGReG0+CU1WvF+VmB8qPrYI9F+MlPeAgNjCpKgAlMdLGO
WxURIC+mqYPh/69ORPnJxodePK42aGQubP5YSw4eXLuStWG/qqlepztgmZytImwnOgYcL3so+oOw
0VLsjYhFaGqbJHFTnHsOAYVhxdtEwJG7u9MawsVnbjgqcvHO2LTB5hEha36xoprj/lHxi18/1BIv
ek+XaVmoCT8ZyQrdtr6ull6CUdLgwFbMQf/yjbVpgI4jFk2vQFBPyHkB+vy80iN4n4O9eA5Lpn2y
A9R+oEr4XDdoFw8K3C9vJT73OQvPy5yD/jQB15+qxIQ0fu5mQynE2bliLdmFsWmtIqye7SfhC+2O
6abrDZ5uzu/8tUmIYhGRkHrpYvcS9cc33uYTSOIwNvBNxLoJT+BvHOcQWzHefleS/jIhRPloIIwA
JEmBo71OhDn8HxHJBwKxe/1CGJv7jqWOqAbHofrCVgGGJQAsQggfZybqxQ+4LK/sRyHwR6vbgiPr
INXdsvljh//DYkEhmKQejQrE4ONzrKDUU1tq2vm9cGg/v0wb6tcpbnSEGHtFcCaWrrUhTtjMyTaJ
mYmpjKmdl2X+hYkH3pTWN3RDRFt4FC+67eXtUrbxHFuL80YwYGQxLCP7Nda/nAOQ2+wuR6k7DMxN
STZDEh4LqOMqRg/+0n0f3Qb/eixL5IW44D4cVlDN1PxW21XT7KtyKpJkeRTArxd6rpH2xSGf9xp0
bFyY5+jKTkiDFwextCmtNMEFBbosRXa9WKCJZLSX3WSED2Igd66xopyEiiP9qvE10xlkmTbwBmES
hjAm6SfKYxtkXgrLyIhJ9jjW39SFlIGi9BwM2FRGgByNyZd/kCqbrQxECDurGTAwq4dH2kZSA7mo
l5E6GgV9/wKOYSTmNiaV1qHVdwHWLn/YmZ62p6ssktMPnNxVzDr7/8UYyhZG6Icravlre+mMcN7S
GyKo7z8wvTSAfzNWUMyVPKzJpa6CKfBHVBgGDGs7ZvYAByRDeP3Lm5+5pkLKp6t4dG2NsoOxU8RQ
Ajb2qESHqgbdITV032CCgkDMbN+S/fA4Aszj2CyK5xuCBBhy1dd5OVkJRQdzuS4ruAP1C0QO4V3h
8Zw1lA5v66e3ijEPAHkSA29hqHgINDi++G3JQKIzzV9GZx6/+P5mn9s9zuixQCOYu2tUm5qjh6tD
S+x4iKfMhMRPWWP/+KD3YP+nKtAOUchDU7WWzvU+qPZV7XPfzxoZl7rAI82xvw3VwgrEmD/O8As3
QKSPoFEgnSCPascS5R+jp0PaCDsjEhDxA+BxtOpS/P8CovSyG9Uwvptu8aMxtC5G5qlxxJbRxfO8
M1FKn6hv44GCRUZG7c/dXsj+NqyqESGJ0lOn2lX5esqyaE7JZ5z2fsv4LQbyOmwKEpIWZPi1YKMN
dbSUPlsZWu8u2aioiYi1fxvYdFe5QkYsvvpX8tvJk9WojMgOxFsiuQUy+I46Q+bEwLB/4WfkzSY0
3SQG0fF57TzHRT4zYvCA2Y+7npUtzD9wppapUaHTmD1KQtO//ts5RHjsYiYMspLjygE7vYn2bHve
WBar9lFL2lL1QcrZqOEZlYvybRZIrV7IHlPtqWA8Dm/+kzvWoZ1pkDe3huoSl3liv4fT21BStvjE
W8PaoWwlH29Fvp03Wy11rp8bjYP5MnkUyGwg8uLUTJJo7J7PpfsGc0PlOjtHARPSTU0VJT/GSqpH
JTFKgA1L47I7qt4/j5Nv7H39AmYk20Q5yx2FkLp5GXfHQM9qmZ2rNuzeQ2bxz/b30mc+QHBbqcIB
Nj4LOCNkijZfu8RfmiARDzPwvW0YAJ4AS7BT5UdVXHJ3wJRr9471wYjK0cN9G+jdimb69nM2RO/n
X0K55E7iG4pKfdm18DK23kf7yuIiR0T9UAWQmUktcLPescBemyIJc0YDWYwuXmVjgeCtccMDuUdn
8HXPyg56lOkCCVutNPWF1aHBded1VaOvHY2CX0XEI/NNLqUZTv2Rknlm/3UhFIeTGLBZyuYavAFd
kXSeVpJDMvv5OI68oPgeg+lEC5wkSqUJ4OEmZdYGWYEGQsBsU2jaMjp2tuWM0Bcmj1/0uJx9FU5q
f2pRwDwnzyaYchisFxZagEvcElZlstH7wEeoMZKKDF55la2qHYV0p0fz9Q6Th9oUeJgZCgeRHfhB
sNDRu1fQE1SnjwSnVSeNpcqveSw0HQyPwqjAMRz82rxa84b9SW0/vMhJ+W+NA02KS4I9duevOYC3
wd+F7Heuy3MxnxdiIk0arLUiQdeRFw1qDFC+/zJ3mBKTBvTZCxwWI/toB6drkorR0jyYfvgU5JhY
xB2xXzFPp96AEfqgYyTx5qhFuZyP1Qk23m1XL+F+a6Ku38Rrs3vevtzJib4lxjNBN57yCvPWsDsk
6j+zN5cWuXVwvd2aZpmVsPL0/nVbU5vbHgIZwkR3Kew9w1fg8dkIPlLWczUjGWrK7rW1hBFj0Khi
KWHOiR2nTLmQ7SKeZWr9x0YV2BayNBw48XGB7vtA/CL+H3bn93N/7U/kzzVPN6NQiwpHoJ2MvENe
i4V0rxQFUUy7x4fZeftumgD/2dgke7uz6B3j5bOJ3/buuDpHKeou6oAy96Td1Cn0DWNW1u8H5r7+
Q/+151OV67Lbvd3qxX1DyTIEjGCsMz9lTLawkY1j4F/kkshnQGJI44G47pxPLk3+IqEOiwQgzaMJ
1RhyILTgxZR8p7tnH31ydxx0i8OXGAZSzC0KELUHBde1HnSMXEp8b0GuGKC4OeUH4NGqI5T/jTIT
NMDqo1JEV554cB+8Jwq8A5Dct+WOPZeU/fZRQYw6D+7FQKjnTDx8/d3jz7SmNskFyApPb35coF/O
7ZQwwo40wvsgNrCGezGFva0dOj9BhmEYWYS9St5aFFDnuy5BQzndzIgXCtWz/VAMmW9YyrlnCaY/
Ljx0FoSgpp+6iv5tQRqke2EWonaqiRZHzQ7nkHtDSjUJ2imdn2qQ+K9grHH/0JklPRhWRvRIvYcB
fhtwco4Eq+8DNemHdagcge6B4aJIMXoS29bTyWwQOQLdkEG3C6WFBDJMOb8NMwTlps0nrmidw7f0
h4qKPy4gsmHohQe0VQD6GVkqJJCxREsDNbO8jy75WRdK1aT5fRxVtml/R3SI5GRn1wlCigmlBLCF
CelUHqFwdwfOR5OzRTiujczle1PWaqU2vtlmUpidFrNrY6WPYGmTVs2nbI7jhCffGjQ58s3/uCyG
GBFWO787/W5hVSoRkXmTkLhgbTW+I6a4YF/ppIQY9drFB+gW+Ihnm1xCeUF1erLIahjch93wBYvH
GG0luH7WOUvC0lekEyKqBCThFEFbklhSez1NsNFXwh+s2sNUGkrx+Jm8NW0EOQE9seThhwttTMTs
9LH0aPLwMuRlkX+CBQz48yk2spF0uAf392/yDfEy0nf+rwvt9jn97HYQFSean+FJlZ0uUKCQNmLq
hWbEqlSfH/cwROrySyOOeIuCWuNx/XyFxswf0CUJxSnpCQv+/7NQDZrnPCJnNBbScsi689GPNLu9
5/WO2/ethYASIYgZEZE4vleXpLOiadw7goNPdy2zeS9nLm/PxbeO2lOWHBW249Fp0ty7Bn7hRLn8
PyJ3NAycY6d7uIPYIkSYhl8Y3rjKxIeaUWZyCeMT7z4pVOWqK6SnYO0bf67K3r/6bk404fY6sSJq
GbhENyMv1HbTeb6/idQ5FmB4Sius2igEW279Cc0fi2K5xGJTrQeF5Vtx6LmDjyNyz4EHgWjtJ5NB
5gEO6KqQaz1UHN/pQEetnBROQLZflk/wVi7PPoVC1EAQ59eQyskE4JNSqU+yIcKSrMnkcUPgCCkk
22EpQDflooIkiwOxj89rEMyRlbGEF/9YIbnJc8ZtT/xhzr4u5oNrfVipua+UpRR45s8hDtBoPxC6
/KrAJkzcvedfuQmgkcwmB3jdrbZ+LoKuNH6hl5Ge5pckBCJGHVpMlVjyVRLzvXXX+S3H1ADVVxQZ
iIJxdyA6QtKML236zBDoIv/2/gbMDVOiITAi7GJVjFyZlwXZvIUoIXNly6gFxL4zMkZysJDPUfB1
8kvGjCAk2y90zQ9JCK6gqFPHNYiSLaXSnx/5bDCNHWXvIYy5M0PkWKtTgyWvfiXRNVMJDKCU7DQa
wEer15RiCIs7guQO6s/EmcnqWxm2YBseyqBkuoKDGreFLCXRtj+QTRO1oMwlX2xJYW0RKjAZCChg
ZZsjGpLhLL5Q7mm3UiD7lWKP/ATLV8jv/lCYiZS0x24zYXi8OwHNeHTvwhAxqj9BsxYkZSYPV64e
lZi4LqTymn+p+9uKWfmA8LEsoGCFblzv+clxEogoHmXD/52LJk03FeYfsp3bTZcUeb+winU9ICMx
LIk6AT5pGpLV8Tk941JMUlc6evOo+TM8dd7mIO45UMA7Gy7IOr5kRyjZHT9ePYNuWRnvtyytOYDg
IvMQdwezqwIJC9B3MqCUodxNvTJPjoh/W38/5QYZHKgG+FrEojz8SP1IJUOePqIAMDtZlNd5dNo6
2BbZELjUX9o4RJD7qj3Cs4brWeevMRVCbc6dNvRXmRBJpllIhigqPxFLcG2jjG1WO9Y5HcKnIgew
ZX6q3Jh3N4rl0WW2hE1zfkxRbrawGighkfsNEDs7X7KV9wC8qmgn3puQmEPZCRv+RYp3Psd6gmwK
+6vUYe9fbOapLtLx5u05GV8DyhOHLflLE7OwU8f9+cw0GbY4X1nEWuusUkPUYPXPDLr5Qv9A5v1Z
GO43JJFMquwaPsaqjcR7WZD0aUB3SfmIkyPKAVkofTz4y4sOWaUEOiWUEPLosNpAUW8Z0KWOdA8l
TvQcDZkzbbGIC2CKsTs0sb3XJuiuWzxa6jLVPcfnj5x16M/3s8XSPKnVTqXng9mf2dXR6bFU6iu/
NYYS4OagW4LRwnC7HG7ql22SExssmuTVJZ1gjyTZ0oCt5dxI0xbo5RK6/oW9kJS5drZ7p6o/5Fi4
AYNAqKMth8xwHOEJZIkyauQnXJ/noZZk7JiBgRKIHrWt5n0u1QV/kjBqO0QpOtKtCiSy1m2+U3LX
T+peJOOE+KaaVSj7Q8CMPuUA5Yra57Nf0dsWPAmdxFGSlUh8Wa4UvDsGNWmif5PSY2AavarqVJO7
nmskzSJJ3hQbJO0hFegfUU4lJKZmmmWtMLSSY3X2Pl7ZJ5j+/++F7LXrKuK4dU5D3oPwxbnLBwes
h0twoCBz7I8vj4UMYcUPoAMTiwjO72N+RfhBOW37fY9IvopIzrufK8wX9BVjlaiHyTDbAjL9uu7N
U+SZyn6TJiLcWMPISjobiFEoYUNhJcJAN7g4FQeHJdiZzC2kAGjHIKzecd5gTOZ+eg23yFoJjdMB
+r2qsd71UQ1L2YnZwY/OdZpzx+tkLPsycbZ/3UfbS7UGkDjd4Ce/Rpt00yTjThHxZ6AFK/bfOC73
9CKtt9PEpOoBfHdhKehjg18DG/84onVOifJwYg3acuesvCVoCfHJlW7QmkLY1fEV4+hGGQfpBFYc
GKd9Gg3DJghYBDRqGOXvtLVsEf+p0wa9tkTB4f+W+DFDtNCpNbjNY9cxCxj4DlNl1jLWDmu+t4bX
/XRoWOAoyPz2Ns6UfzULHHwwzhgttIeomsFn9+Y4mBv3htzV2GO1vJxT5YQqzsHMIjbhYURDwEdw
McBwb+hSVoUApsht7CASapm4HHOtkRg04b8qFffHY7hwXOx2h5e0mqmrJ/EoePG2OOVQmvrY8IPl
pwOGGmMBOC5xO6X33QfQSZmKVEgRTvi+jDYqJQzkmdN3jeNg5RJiEtnHZE9izkMbtnmYZAHgYprT
4QAyFN7pchV8adNdCSzQ5sRy2Au2fmHui00+yoTJAcU9gNL0Gl0/XAExSgE1eVcpuKcLXTdxjdGt
dZHAomIz1iQ1w9ZZwMGPZSMUy3X88K65sumWl88CfyStSVMhkPzZUsQdNblL7kZQ4yoGzYJPORtf
7JA748gN9JXf5tRy9DnLSElU/sPnVsWEm/lKH/JgSqgjYncctMyQiuraDQg9H/YKXt+gRdAkCAN7
/kEMamc0vWmmAcCpoN7Z3TqPF6RlPrgTcEveGNTV1FtDHYGYNKjkhGwI+mwJp+7Bx7cGNLAEEqmK
UHw0ADrGFuwZqS6v6N0FPhZMCMNP0JHZlBhqv2X2l/0NQZkx+A4HiyA4HT7NJcjfrOjLbEeGn3Tn
Z3HvDusOOcZcIJo52R6+5uGr2caTYP/vU3Jf/dlxHvn0usyrFmzPuky6qrJSQT0K+SxFaL/+zBPL
+z6KSC6bDTya2TQ8aP1NxJ5q1RWKtT247xxD/rCoUO4jkfL1lrjVKmHF8EtBeS9TxtEYwLRlTbUs
s3XDtSlz+rCvWTKeGS5XKtU3+MAd0mir7pCz0msY7XGL7SmQOV87QfATqexq3vIa3bhH7RBURNy+
uhfkj4W2SBqFzvOs8Fi3iiLRTYWw2XqKLSXAwZ9OP+dzVHyTaDCTU9ZX+kQniEBBwsBk2PMhh9AE
3cRgAjVdIi6GQLnKBArtYTJKYAs0unmhrkrysE8rOkMMmvvN+ub49DjNRVvqtOgTnMaSWKqZGbc/
M4Ka52zkDJicX3NX9GXr5EHgsMezSYWCfCzYtCApTPmtSnwSxnwghj1mr25bPgXpS9ZnJTYLLczR
sAB5VT0PbG9EJs4Lwmx/wl+srUExDmtymHRPZMyAM26n32RzUzwNjwxcz8bho9Wf/XteRU1M/bUG
9NLTT+9zVeoYfz0rAcJK1ea4sBBLiJaP7lKcl//eMjfbrcXEshwtCNG+JGn6OnReQnhBe39cGRoq
Qcn1fleEJe6UiJYCl3/ZGen2UlT12gQwJetghbPD/R4RWadCBoWBce7YaHCI9aVnnc1GSTypj7GX
I2l6nvOeaZORjgiVW1oPg11hRlRGF9y6I/cJsIbr7x8tq5BhEuScSQb1VuDNSOCf8JuGjYNx1xFE
I+VPg/m4CwaCNpxXAtRKhNNfFtfQZUdHOKWMVXNyX0toeQ3RgJiz8C+84UUYMPGnt+z2OInr6B5k
rxN62xc8Iwu9socyc/yErUVkuKEv34Cd9W0EAflRGmwDBKp8yjtpuCcJxT39lR3jMdO+AjFvPkCH
f1P9FPfOX0K36yVnt44XioXuEanZqJJecWi2yP95+/Xtk3ze7U9LEM8ENrWUmyzmRIfL/tGjsA2g
S8mpC9o0ql8jbzrRzYw0vTOVK+rdjlacXvuFwHgqy+993mRWW8wiciqCSCe8pTsW41btR5DE8Mgn
8aSHs1fd+GUsECrELnRxlGlrYFdAplT7c7bXUtuMqKnyBe7PNeAO4gUlj0pPwXBtbGrNMFHNmvA9
y1PhOFL1rXvYYjpX0cYZ3ejlYw5nKkmTQ2FSJLh8NDJovzMLvtlZQThZgPT7qTkyYDnU1yDziz9N
nVdK2aSM+7Nd2yZIpsdyOmzIVT9BCck47sqwpGnkwiyu5I+l0EcaqjoLXi66XZs+D3ML+KwltbIa
JF8DyIAATdVnuFxrZgjj8vblFCdsegpatZ3UoNk9MDEyXOo0/+M9VaxiDOyXzG2gJNYmChPbaceJ
wnFS5JxT50FPvj2Bt3vdXzf6JzMbx8InIdsPvB4vGjqTNvo/iW8RRZoORiXR2J3+DjBarWfOomi+
dXsGL9s0lffMG44HXm+xMbe4i+3i4qL8osrWdoYI+M5/pDzPdDdBJ3R5o3f5pY6H2MZPA58HRSpG
Wwed9dc1q3zwy87ASLvXrRMEnyy0h0CN1qX8vSoBqfBMXH4N1h+/dsSjAZDYULH+9cGDsoJ0YwQf
sf+Oa78uMeiiyJ6ptsAyeOrqWiL+l7tJ3+Bh0ChbdveSPSKmGfvE4KZHiIXX8aup1ptS/u0uSuaV
T+AB2MhpvYgXszxfo7ljZcqV2wmgSRsSYay9JRUxZ7C865ODz/zN0KEvau0ra1xaqC0vQ9pGwT3o
cUCOtzh2DyvvNkoNIPXqfeSixs8uMPoyNYIJ4li7K0FO8zXlQ2bNwEW9QhuDY2jblAiEfbjebB5A
MCvLUVxJtC+1V7FzSDdxQpln2YnMm67EtQtsM/mh3lwjXhwuQwyGwYE+Z0EbRljZFNw6+JwPGlmj
2LmvUGtEXhaPPTyvIoHf5eYvh/YzDmgPkCzATVGPT9XYnVf8Go2hhEEF9IXoKUg4musJwq3GZmA/
N8lU+SFjDFU2m3zoJE/ijtXvgTfKC8UvJXwEgEAH9LNM6EPnUM9c8lNNMbLNbGVp9rQVPj+QjJ8M
4rQ+jEkGpLBGkHFr5Bg/pIIf6tKPBJl9MGNUnROLSrh0Gm7C6kqrsjbv5j9qtJ/o0WpL3o4e8LkZ
h2XXB6AubINqrlueAImnjpF8Xgg6T5xIQIFrdxQXLMwb2yP+zUSqQdIWKJObRx6MY7n7sQk6ZBJs
IC+UKFkaQsHNVhoM72jS6asQyMP49qnebu8kcIjUdrdERlHcaKVDmmnOr3N/XOjdKZajNSl+hZGv
7SYGcSElU19Pz7MpRbljmEQp4jwwWpK3N6rmFq8SxBW6wwYVL1yDWCNgffkLfB9gdMSpUFGHG0DE
ZlyiBekhByui0kM543x00S9W30MTFdPhCqG6Wjlmak4X1dox54T5tdJGu1GZOQwgRMQNk5GgAfp5
51/BhRcpmXLacepyyMEAoYtMDrYvupntT9gAG2VkKrkfZDCr4VbN9iQmRcwYVuXw4rATf8NwMbQ8
WLbUUhCsitf8ksM3VVZ1tzF8DVCEiVzZfAk5zo7GS0q4Oj0OJD3qkaHHmZr2uZaips4QmZizK9wk
e6ExPaeOxmSvPghEnyH7QzRtMC+WOCZv0ClTi60F4JDjP3zlaIjjbCsVJmE7MIYy7sTOW3jciv3d
OhLHPdyvXaqzOfUw67Vj8+lph1fYhy9guNMgYBdpIuEFpxkS7IwFrtiS8UWQiHFodPkCo6UDTtl9
rk0XdtGLsq13hqlXriZ5DlxLpaLtIUAVc2YhSVK9rEKmB4SWHkEGJf3a4Fqvqv+sLfStjV/E1ZdO
jREFxUva1nozPR6W0HI3eiyxppa22pVya3GoeHDo6J0nDNLy6Pp/T4f0RsIbkMal4/CDPoKTbJNG
gSlsrLzOmMTJ5aEB2BacmO1xnVzzsGkx8q0q3Mn9PR15p1hPhxNq2GZSCxETFob9gJwC1Y7IuNrh
SKP8vvTbdsUR1nZ5PZ5ZLAALeHe5v1If6HXnFhs7AAAAULZb+G6aKUgAAYO5AYDQBbuVfe6xxGf7
AgAAAAAEWVo=
