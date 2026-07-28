# ---------------------------------------------------------------------------
# Title (game) catalog + install/remove support (Round D).
# Installer scripts are the DML's own interactive bash installers, shipped
# unchanged to _installers_dir by cli/dev-install.ps1 and run with plain
# stdin/stdout passthrough (the launcher gives them a real terminal).
# ---------------------------------------------------------------------------

_installers_dir() { echo "${DML_INSTALLERS_DIR:-/usr/local/share/dml/installers}"; }

# Can THIS HOST run the title installers at all?
#
# All six are Linux-only by construction: they `sudo -v`, drive pacman/apt,
# `usermod -aG docker`, `systemctl enable/start docker`, write
# /etc/sudoers.d/docker-nopasswd and chmod /var/run/docker.sock. In NATIVE
# mode this same CLI runs under Git Bash on WINDOWS, where none of that
# exists -- a run dies at `sudo -v` ("Could not cache sudo credentials"),
# which explains nothing. Reporting the host verdict lets `games catalog`'s
# consumers say the true reason instead of blaming the shipping step.
#
# Deliberately a HOST check, NOT a backend check: a Linux user on the native
# backend runs these scripts on a real Linux box, where they work fine. The
# launcher's "native mode" and "cannot install" are only the same thing on
# Windows, so the answer has to come from where bash actually runs.
# _host_bash_is_windows (00-head.sh) carries the flavour test and its
# DML_OSTYPE seam.
_installers_supported() {
    ! _host_bash_is_windows
}

# One sentence for the text path, kept next to the check it explains.
_installers_unsupported_msg() {
    printf '%s' "installing titles needs the WSL backend: the DML installers are Linux scripts (sudo, pacman/apt, systemd) and cannot run on this host"
}

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
