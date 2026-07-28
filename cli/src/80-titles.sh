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

# ---------------------------------------------------------------------------
# Custom display names (`dml games name`).
#
# WHERE THE NAME LIVES, and why: <title dir>/.dml-name, i.e. WITH THE SERVER.
# It is a property of the server, not of one frontend -- so it survives a
# launcher reinstall, travels with the directory if it is moved or copied, and
# is readable by this CLI under either backend without a shared config file.
# Putting it in ~/.dml/launcher.json would tie a server's identity to one
# launcher install on one machine.
# ---------------------------------------------------------------------------

DML_NAME_FILE=".dml-name"
DML_NAME_MAX=40

# Title ids become a path component and (for `games name`) the directory a
# file is WRITTEN into, so the write path validates them rather than trusting
# the caller. Same character class as the launcher's validate_game_id, plus an
# explicit `..` refusal: every character of ".." is individually allowed, so
# the class alone would let a name file land in the games dir's parent.
_valid_title_id() {
    [[ "$1" =~ ^[A-Za-z0-9._-]+$ ]] && [[ "$1" != *..* ]]
}

# Echoes the directory a title is installed in ($GAMES_DIR first, then the
# legacy $HOME/<id> layout -- the same resolution `games catalog` uses), or
# nothing when it is not installed anywhere.
_title_dir() {
    if [[ -d "$GAMES_DIR/$1" ]]; then printf '%s' "$GAMES_DIR/$1"
    elif [[ -d "$HOME/$1" ]]; then printf '%s' "$HOME/$1"
    fi
    return 0
}

# Echoes a title's CUSTOM name, or nothing when it has none.
#
# The reader never trusts the file: it is plain text a user may hand-edit or
# copy over from Windows. First line only, control characters (incl. a CRLF's
# \r) dropped, trimmed, and capped at the same length the writer enforces --
# a hand-written oddity must degrade to a sane label or to no label at all,
# never to a broken one.
_title_name_read() {
    local f name
    f="$(_title_dir "$1")"
    [[ -n "$f" && -f "$f/$DML_NAME_FILE" ]] || return 0
    # `read` returns nonzero at EOF-without-newline but HAS already assigned,
    # so the failure is swallowed rather than treated as "no name".
    IFS= read -r name < "$f/$DML_NAME_FILE" 2>/dev/null || true
    name="${name//[[:cntrl:]]/}"
    name="${name#"${name%%[![:space:]]*}"}"
    name="${name%"${name##*[![:space:]]}"}"
    printf '%s' "${name:0:$DML_NAME_MAX}"
}

# The label to show for a title: custom name, else the registry name, else the
# id. A server must NEVER render as a blank label anywhere, which is why the
# chain ends at the id (always non-empty) rather than at the registry, which
# only knows the six shipped titles.
_title_display_name() {
    local name row
    name="$(_title_name_read "$1")"
    if [[ -n "$name" ]]; then printf '%s' "$name"; return 0; fi
    row="$(_title_row "$1")"
    if [[ -n "$row" ]]; then
        # Field 2 by parameter expansion, not `cut`: this runs once per row of
        # `games list`, and on native Git Bash every fork costs ~165ms.
        name="${row#*|}"; name="${name%%|*}"
        [[ -n "$name" ]] && { printf '%s' "$name"; return 0; }
    fi
    printf '%s' "$1"
}

# Validates + trims a user-supplied name. Returns 0 with the normalized value
# in _NAME_OUT, or 1 with the reason in _NAME_ERR.
#
# Two globals rather than stdout because the caller needs BOTH the value and
# the failure reason, and a `$( )` capture would run this in a subshell where
# any second channel is lost. Call it directly, never in a command substitution.
_title_name_normalize() {
    local n="${1-}"
    _NAME_OUT=""; _NAME_ERR=""
    # Control characters are REFUSED, not stripped: the name is stored as a
    # whole file body and read back first-line-only, so a value containing a
    # newline would come back as a different name with no explanation.
    if [[ "$n" == *[[:cntrl:]]* ]]; then
        _NAME_ERR="Name cannot contain line breaks, tabs or control characters"
        return 1
    fi
    n="${n#"${n%%[![:space:]]*}"}"
    n="${n%"${n##*[![:space:]]}"}"
    if [[ -z "$n" ]]; then
        _NAME_ERR="Name cannot be empty"
        return 1
    fi
    if (( ${#n} > DML_NAME_MAX )); then
        _NAME_ERR="Name is too long (${#n} characters, max $DML_NAME_MAX)"
        return 1
    fi
    _NAME_OUT="$n"
    return 0
}
