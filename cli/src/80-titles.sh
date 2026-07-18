# ---------------------------------------------------------------------------
# Title (game) catalog + install/remove support (Round D).
# Installer scripts are the DML's own interactive bash installers, shipped
# unchanged to _installers_dir by cli/dev-install.ps1 and run with plain
# stdin/stdout passthrough (the launcher gives them a real terminal).
# ---------------------------------------------------------------------------

_installers_dir() { echo "${DML_INSTALLERS_DIR:-/usr/local/share/dml/installers}"; }

# id|display name|installer script|kind(games=installer manages ~/games itself,
# home=legacy $HOME/<id> layout needing a post-install symlink)|launcher file
_title_registry() {
cat <<'EOF'
wow-server-playerbots|WoW WotLK (Playerbots)|install-wow-wotlk.sh|games|wow-playerbots-launcher.sh
wow-vanilla-server|WoW Vanilla|install-wow-vanilla.sh|home|wow-vanilla-launcher.sh
wow-tbc-server|WoW TBC|install-wow-tbc.sh|home|wow-tbc-launcher.sh
maplestory-server|MapleStory v83|install-maplestory.sh|home|maplestory-launcher.sh
runescape-server|RuneScape|install-runescape.sh|home|runescape-launcher.sh
muonline-server|MU Online|install-muonline.sh|home|muonline-launcher.sh
EOF
}

# Prints the registry row for an id, or nothing (exact-key match).
_title_row() {
    local row
    row="$(_title_registry | grep -m1 -F "$1|" || true)"
    [[ "$row" == "$1|"* ]] && printf '%s' "$row"
    return 0
}

# Primary server dir for a title by kind.
_title_server_dir() {
    if [[ "$2" == games ]]; then echo "$GAMES_DIR/$1"; else echo "$HOME/$1"; fi
}

# Exit-status helper: is the title present at either location? (wotlk may
# live at the legacy $HOME path with a games/ symlink, or vice versa.)
_title_installed() {
    [[ -d "$GAMES_DIR/$1" || -d "$HOME/$1" ]]
}
