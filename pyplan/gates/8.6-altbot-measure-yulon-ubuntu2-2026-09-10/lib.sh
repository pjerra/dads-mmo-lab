#!/usr/bin/env bash
# T26 measure -- the two things every step script needs, T13's `lib.sh` pattern.
# `stamp` reads the CLOCK inside the script that writes the line (memory note
# `stamps-come-from-the-clock`); `say` puts the line in the box's own Claude
# activity terminal BEFORE the action (`always-use-activity-terminal`).
set -u

OUT="$HOME/t26-out"
PRESS="python3 $OUT/t26press.py"
SQL="docker exec ac-database mysql -uroot -p\$MYSQL_ROOT_PASSWORD -N -B -e"
mkdir -p "$OUT"

stamp() { date +"%H:%M:%S %Z"; }
utc()   { date -u +%Y-%m-%dT%H:%M:%SZ; }
say()   { "$HOME/claude-say" "T26 measure: $*" >/dev/null; }
hdr() {
    echo "=============================================================="
    echo "$* -- $(stamp) -- $(hostname)"
    echo "=============================================================="
}
sect() { echo; echo "---- $* -- $(stamp) ----"; }

# One read against the character/auth databases, through the container's own
# root credential (never printed: the password stays inside the container's
# environment, referenced as a shell variable evaluated in the container).
q() { docker exec ac-database sh -c 'exec mysql -uroot -p"$MYSQL_ROOT_PASSWORD" -N -B -e "$1"' _ "$1" 2>/dev/null; }
qt() { docker exec ac-database sh -c 'exec mysql -uroot -p"$MYSQL_ROOT_PASSWORD" -t -e "$1"' _ "$1" 2>/dev/null; }
