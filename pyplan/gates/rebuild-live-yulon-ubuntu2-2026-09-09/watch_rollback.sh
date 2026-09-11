#!/usr/bin/env bash
# Watch the `-rollback` tags while the compile runs, from OUTSIDE the press.
#
# `_keep_rollback()` makes those tags in the first seconds of `rebuild()` and
# `_let_go()` removes them at the end, so they exist only DURING the compile --
# the one window in which "the build you have now was kept" is a thing anybody
# can photograph. The press's own log says it happened; this says the daemon
# agrees, read by a second process that the press knows nothing about.
#
# Writes one stamped block per poll to $OUT/rollback-watch.log and stops when
# the press's log says it returned or raised.
set -u
OUT=${GATE_OUT:-$HOME/rebuild-gate-out}
PRESS=$OUT/rebuild.log
LOG=$OUT/rollback-watch.log
INTERVAL=${1:-20}

mkdir -p "$OUT"
{
    echo "=== rollback watch started $(date -u '+%Y-%m-%dT%H:%M:%SZ') ==="
    echo "GROUND, before the press has done anything:"
    docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | sort | sed 's/^/  /'
    echo "  -rollback tags now: $(docker images --format '{{.Repository}}:{{.Tag}}' | grep -c -- '-rollback')"
} >>"$LOG"

while true; do
    {
        echo "--- $(date -u '+%Y-%m-%dT%H:%M:%SZ') ---"
        docker images --format '{{.Repository}}:{{.Tag}} {{.ID}} {{.CreatedSince}}' \
            | grep -- '-rollback' | sort | sed 's/^/  ROLLBACK /'
        echo "  count: $(docker images --format '{{.Repository}}:{{.Tag}}' | grep -c -- '-rollback')"
        echo "  world: $(docker inspect -f '{{.State.Status}} pid={{.State.Pid}} restarts={{.RestartCount}}' ac-worldserver 2>&1)"
    } >>"$LOG"
    if [ -f "$PRESS" ] && grep -q 'REBUILD RETURNED CLEANLY\|REBUILD RAISED' "$PRESS"; then
        {
            echo "=== the press has finished; one last reading ==="
            echo "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
            docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | sort | sed 's/^/  /'
            echo "  -rollback tags now: $(docker images --format '{{.Repository}}:{{.Tag}}' | grep -c -- '-rollback')"
        } >>"$LOG"
        break
    fi
    sleep "$INTERVAL"
done
