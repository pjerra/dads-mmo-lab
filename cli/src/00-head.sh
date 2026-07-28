#!/usr/bin/env bash
set -euo pipefail

VERSION="3.0.0"
# Games root. DML_GAMES_DIR overrides the default (like DML_SOAP_URL /
# DML_SOAP_USER / DML_SOAP_PASS override the SOAP endpoint): tests point it
# at a fixture dir, and native Windows mode (Git Bash + Docker Desktop)
# points it at a Windows-side folder instead of the MSYS2 $HOME.
GAMES_DIR="${DML_GAMES_DIR:-$HOME/games}"

# True when THIS bash is a Windows-native flavour (Git Bash / MSYS2 / Cygwin)
# rather than a Linux one.
#
# Two callers need the same answer for different reasons -- `doctor` reports
# a missing systemd as informational there (_host_is_native_windows), and
# `games catalog` / `games install` report that the Linux-only title
# installers can never run there (_installers_supported) -- so the flavour
# test lives in ONE place instead of drifting in two.
#
# DML_OSTYPE is the test seam (mirrors DML_GAMES_DIR / DML_SYSTEMD_DIR): bats
# runs inside the dml-arch distro, which is always linux-gnu, so the Windows
# branch is otherwise unreachable from the suite.
_host_bash_is_windows() {
    # uname FIRST, and it is authoritative for the positive answer: a genuine
    # Git Bash / Cygwin host says so here whatever the environment claims.
    # That ordering is load-bearing -- read the override first and an exported
    # DML_OSTYPE=linux-gnu would talk a real Windows host OUT of the guard,
    # re-arming `games install` and restoring the bare `sudo -v` failure this
    # whole change exists to explain. The override can only ever ADD a Windows
    # verdict (which is all the test seam needs), never remove one.
    local os; os="$(uname -o 2>/dev/null || true)"
    [[ "$os" == *Msys* || "$os" == *Cygwin* ]] && return 0
    case "${DML_OSTYPE:-$OSTYPE}" in
        msys*|cygwin*|win32*|MINGW*|MSYS*) return 0 ;;
    esac
    # A bare `case` with no match exits 0, so the verdict must be explicit.
    return 1
}
