#!/usr/bin/env bash
# Dad's MMO Lab — Linux prerequisites installer.
#
# The Linux counterpart to guides/DML-Windows/Install-DML.ps1, and it stops in
# the same place that one does: it makes DOCKER work, and leaves installing the
# game server to the launcher. Same division of labour, same reason — the
# launcher can install a server, but it cannot make a machine able to run
# containers, because that needs root.
#
#   curl -fsSL <raw-url>/scripts/install-dml.sh | bash
#   ...or, having cloned:  ./scripts/install-dml.sh
#
# WHAT IT DOES
#   1. Refuses politely on anything that is not apt-based.
#   2. Installs Docker Engine (skipped when already present).
#   3. Adds you to the `docker` group.
#   4. Proves the engine actually answers.
#   5. Tells you to log out and back in IF the group is not live yet.
#
# WHY STEP 5 IS NOT A FOOTNOTE: group membership only applies to sessions
# started AFTER it is granted. A user who installs Docker and goes straight
# back to the launcher is in an OLD session where `docker` is still denied —
# and the launcher then reports "Docker isn't set up", which reads as the
# install having failed. That is exactly what happened on the first Ubuntu box
# this was tested on (2026-08-19).
set -uo pipefail

say()  { printf '\n\033[1m==> %s\033[0m\n' "$1"; }
ok()   { printf '    \033[32m✓\033[0m %s\n' "$1"; }
warn() { printf '    \033[33m!\033[0m %s\n' "$1"; }
die()  { printf '\n\033[31m✗ %s\033[0m\n' "$1" >&2; exit 1; }

# --- 1. is this a distro we can honestly claim to support? -------------------
# apt only. The alternative was dnf/pacman arms that nobody here can test, and
# an untested install path is worse than an honest refusal: it fails halfway
# and leaves the machine in a state the user did not choose.
say "Checking this is a supported distro"
if ! command -v apt-get >/dev/null 2>&1; then
    printf '\n'
    printf 'This script only supports Debian/Ubuntu (apt), and this machine has no apt-get.\n\n'
    printf 'Everything it does is small enough to do by hand:\n'
    printf '  1. Install Docker Engine     https://docs.docker.com/engine/install/\n'
    printf '  2. sudo usermod -aG docker "$USER"\n'
    printf '  3. Log out and back in, then check:  docker run --rm hello-world\n\n'
    printf 'Then start the launcher — it installs the game server itself.\n'
    exit 1
fi
. /etc/os-release 2>/dev/null || true
ok "${PRETTY_NAME:-apt-based system}"

# --- 2. docker engine --------------------------------------------------------
say "Docker Engine"
if command -v docker >/dev/null 2>&1; then
    ok "already installed ($(docker --version 2>/dev/null | head -1))"
else
    command -v curl >/dev/null 2>&1 || sudo apt-get install -y curl \
        || die "could not install curl"
    # Docker's own convenience script: the one install path Docker themselves
    # support across Debian/Ubuntu releases, so it does not rot when a new
    # release ships before this script is updated.
    curl -fsSL https://get.docker.com | sudo sh \
        || die "Docker install failed — see https://docs.docker.com/engine/install/"
    command -v docker >/dev/null 2>&1 || die "Docker install reported success but no docker binary is on PATH"
    ok "installed ($(docker --version 2>/dev/null | head -1))"
fi

# --- 3. the docker group -----------------------------------------------------
say "Group membership"
if id -nG "$USER" | tr ' ' '\n' | grep -qx docker; then
    ok "$USER is already in the docker group"
else
    sudo usermod -aG docker "$USER" || die "could not add $USER to the docker group"
    ok "added $USER to the docker group"
fi

# --- 4. does the engine actually answer? -------------------------------------
# Judged by EFFECT, not by whether the install command exited 0 — the whole
# point is whether the launcher will be able to talk to it.
say "Checking the engine responds"
GROUP_LIVE=no
if docker info >/dev/null 2>&1; then
    GROUP_LIVE=yes
    ok "docker info answered — the engine is up and reachable as $USER"
elif sudo docker info >/dev/null 2>&1; then
    warn "the engine is up, but not reachable as $USER yet (group not applied to this session)"
else
    sudo systemctl enable --now docker >/dev/null 2>&1 || true
    if sudo docker info >/dev/null 2>&1; then
        ok "started the docker service"
    else
        die "Docker is installed but the engine is not responding. Try: sudo systemctl status docker"
    fi
fi

# --- 5. what happens next ----------------------------------------------------
say "Done"
if [ "$GROUP_LIVE" = yes ]; then
    printf '    Docker is ready. Start the launcher and it will install the server.\n\n'
else
    printf '    \033[1mOne more step: log out and back in (or reboot).\033[0m\n'
    printf '    Group membership only applies to sessions started after it is granted,\n'
    printf '    so THIS session still cannot reach Docker. Until you do, the launcher\n'
    printf '    will keep saying Docker is not set up.\n\n'
    printf '    After logging back in, check with:  docker run --rm hello-world\n\n'
fi
