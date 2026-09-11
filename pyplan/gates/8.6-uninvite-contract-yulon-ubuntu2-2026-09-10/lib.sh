#!/usr/bin/env bash
# T13 live half -- the two things every step script here needs.
#
# `stamp` reads the CLOCK, in the same script that writes the line: no time in
# this folder was ever typed by hand (memory note `stamps-come-from-the-clock`,
# 2026-09-09, when an afternoon of ticket stamps ran hours fast).
#
# `say` puts the line in the box's own Claude activity terminal BEFORE the
# action, so the owner watching that window sees what is being done to his
# server (memory note `always-use-activity-terminal`).
set -u

TREE="$HOME/t13-live"
GATE="$TREE/pyplan/gates/8.6-uninvite-contract-yulon-ubuntu2-2026-09-10"
PY="$HOME/t13-venv/bin/python"
PRESS="$PY $GATE/t13press.py"
OUT="$HOME/t13-out"
mkdir -p "$OUT"

stamp() { date +"%H:%M:%S %Z"; }

say() { "$HOME/claude-say" "T13 live: $*" >/dev/null; }

hdr() {   # hdr <title>
    echo "=============================================================="
    echo "$* -- $(stamp) -- $(hostname)"
    echo "=============================================================="
}

sect() { echo; echo "---- $* -- $(stamp) ----"; }
