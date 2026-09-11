#!/usr/bin/env bash
# T26 live half -- the four things every step script here needs. T16's `lib.sh`,
# which is T18's, which is T13's.
#
# `stamp` reads the CLOCK, in the same script that writes the line: no time in
# this folder was typed by hand (memory note `stamps-come-from-the-clock`).
#
# `utc` is the only shape a `docker logs --since` window may be opened with --
# UTC with a trailing `Z` -- because a zone-less stamp is read in the CLIENT's
# local zone and on this +02:00 box opens the window two hours early (memory
# note `docker-logs-since-is-client-local`). Every window here is queried
# BEFORE the action to prove it empty of the pattern.
#
# `say` puts the line in the box's own Claude activity terminal BEFORE the
# action (memory note `always-use-activity-terminal`).
#
# `dbq` and `dbq1` run a read INSIDE the container, so `$MYSQL_ROOT_PASSWORD` is
# dereferenced there and its value never reaches this shell or any capture
# (memory note `gate-logs-carry-generated-passwords`).
set -u

TREE="$HOME/t26-live"
GATE="$TREE/pyplan/gates/8.6-altbot-live-yulon-ubuntu2-2026-09-11"
PY="$HOME/dads-mmo-lab/pylauncher/.venv/bin/python"
PRESS="$PY $GATE/t26press.py"
OUT="$HOME/t26-out"
LUA="$HOME/wowserver/env/dist/etc/modules/lua_scripts"
mkdir -p "$OUT"

stamp() { date +"%H:%M:%S %Z"; }
utc()   { date -u +%Y-%m-%dT%H:%M:%SZ; }

say() { "$HOME/claude-say" "T26 live: $*" >/dev/null; }

hdr() {
    echo "=============================================================="
    echo "$* -- $(stamp) -- $(hostname)"
    echo "=============================================================="
}

sect() { echo; echo "---- $* -- $(stamp) ----"; }

# One read, inside the container. `$1` is the SQL.
dbq() {
    docker exec ac-database sh -c 'MYSQL_PWD="$MYSQL_ROOT_PASSWORD" mysql -N -B -u root -e "$1"' _ "$1"
}

# The same, printing the statement first so a capture says what it asked.
dbq1() {
    echo "SQL: $1"
    dbq "$1"
}
