#!/usr/bin/env bash
# T16 live half -- the four things every step script here needs. T18's `lib.sh`,
# which is T13's.
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
set -u

TREE="$HOME/t16-live"
GATE="$TREE/pyplan/gates/8.6-level-holds-yulon-ubuntu2-2026-09-10"
PY="$HOME/dads-mmo-lab/pylauncher/.venv/bin/python"
PRESS="$PY $GATE/t16press.py"
OUT="$HOME/t16-out"
LUA="$HOME/wowserver/env/dist/etc/modules/lua_scripts"
mkdir -p "$OUT"

stamp() { date +"%H:%M:%S %Z"; }
utc()   { date -u +%Y-%m-%dT%H:%M:%SZ; }

say() { "$HOME/claude-say" "T16 live: $*" >/dev/null; }

hdr() {
    echo "=============================================================="
    echo "$* -- $(stamp) -- $(hostname)"
    echo "=============================================================="
}

sect() { echo; echo "---- $* -- $(stamp) ----"; }

# Wait for the bridge to answer again after a restart, printing every try.
wait_bridge() {
    for i in $(seq 1 90); do
        sleep 10
        ans=$($PRESS send dml_bridge_ping 2>&1 | tail -1)
        echo "$(stamp) try $i: $ans"
        case "$ans" in *DML-BRIDGE-READY*) return 0;; esac
    done
    return 1
}

# Wait for the random bots to be back in the world.
wait_bots() {
    want=${1:-500}
    for i in $(seq 1 60); do
        n=$($PRESS sql "SELECT COUNT(*) FROM acore_characters.characters WHERE online=1;" 2>/dev/null | tail -1)
        echo "$(stamp) online=$n"
        [ "$n" = "$want" ] && return 0
        sleep 10
    done
    return 1
}
