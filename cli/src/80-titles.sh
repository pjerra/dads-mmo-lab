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

# Delete a title tree that may contain CONTAINER-OWNED files.
#
# A bind-mounted database directory belongs to the image's own user (mysql
# writes as uid 999), so a plain user-level `rm -rf` deletes what it can, hits
# EPERM on the rest, and returns non-zero. Under `set -eu` that killed the
# whole removal MID-DELETE, after the games/ symlink was gone but before the
# real directory, the launcher script, or any terminal event -- the launcher
# showed CLI_CRASH and the title could never be removed, because every retry
# failed identically. Found removing MapleStory on a clean VM, 2026-07-29:
# ~/maplestory-server/database/docker-db-data was owned by uid 999.
#
# So: try as the user, and if anything survives, delete as root INSIDE a
# container with the parent bind-mounted. That needs no sudo and no new
# dependency -- we are removing a docker-based title, so a docker daemon is
# definitionally available. Any locally-present image will do; `$2` lets the
# caller pass one it knows exists (the title's own), and we fall back to
# whatever is already pulled rather than reaching for the network.
#
# Returns 0 only when the path is really gone, so the caller can report an
# honest failure instead of dying.
_rm_title_tree() {
    local target="$1" preferred="${2:-}"
    [[ -n "$target" && -e "$target" ]] || return 0

    rm -rf "$target" 2>/dev/null || true
    [[ -e "$target" ]] || return 0

    local parent base img
    parent="$(cd "$(dirname "$target")" 2>/dev/null && pwd)" || return 1
    base="$(basename "$target")"
    [[ -n "$parent" && -n "$base" && "$base" != "/" && "$base" != "." ]] || return 1

    for img in "$preferred" $(docker image ls --format '{{.Repository}}:{{.Tag}}' 2>/dev/null | grep -v '<none>' | head -3); do
        [[ -n "$img" ]] || continue
        # --rm so the helper never lingers; the mount is the PARENT so the
        # container removes a child path and can never be handed "/".
        if docker run --rm -v "$parent:/dml-rm" "$img" rm -rf "/dml-rm/$base" >/dev/null 2>&1; then
            [[ -e "$target" ]] || return 0
        fi
    done
    [[ ! -e "$target" ]]
}
