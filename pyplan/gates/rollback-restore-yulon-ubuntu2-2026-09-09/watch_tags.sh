#!/usr/bin/env bash
# Watch the tags AND the image id under the world, from OUTSIDE the press.
#
# 2026-09-08's watcher (`rebuild-live-.../watch_rollback.sh`) polled the
# `-rollback` tags. This one polls three things instead, because the restore arm
# is a claim about all three:
#
#   * the `-rollback` tags -- absent, then present for the whole compile;
#   * the four live tags' image ids -- the ground's ids, then the NEW build's
#     ids once the compile finishes, then the ground's ids again once the
#     restore has moved them back. A gate that only reads the end state cannot
#     tell a restore from a press that never moved anything;
#   * the container under `ac-worldserver` -- its id, its restart count and the
#     image it is running, because "the world is up on the ground's image" is
#     two facts and the interesting failure has the first without the second.
#
# The `-failed` tags are NOT expected to show up here: their whole life is the
# milliseconds between `_restore_rollback()` naming the new build and
# `_let_go()` letting that name go. `docker events` is the instrument for those
# (see watch_events.sh); this line is here so that a reader who finds no
# `-failed` in this log does not read it as an absence.
set -u
OUT=${GATE_OUT:-$HOME/restore-gate-out}
PRESS=$OUT/press.log
LOG=$OUT/tag-watch.log
INTERVAL=${1:-3}

mkdir -p "$OUT"
ids() { docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | sort; }
world() {
    docker inspect -f \
        '{{.State.Status}} id={{slice .Id 0 12}} pid={{.State.Pid}} restarts={{.RestartCount}} image={{slice .Image 7 19}}' \
        ac-worldserver 2>&1
}

{
    echo "=== tag watch started $(date -u '+%Y-%m-%dT%H:%M:%SZ') ==="
    echo "GROUND, before the press has done anything:"
    ids | sed 's/^/  /'
    echo "  world: $(world)"
} >>"$LOG"

while true; do
    {
        echo "--- $(date -u '+%Y-%m-%dT%H:%M:%SZ') ---"
        ids | grep -E -- '-rollback|-failed' | sed 's/^/  SUFFIXED /'
        echo "  suffixed count: $(ids | grep -c -E -- '-rollback|-failed')"
        ids | grep -E 'yulon.local/ac-wotlk-[a-z-]+:native-[0-9a-f]+$' | sed 's/^/  LIVE /'
        echo "  world: $(world)"
    } >>"$LOG"
    if [ -f "$PRESS" ] && grep -q 'PRESS OUTCOME' "$PRESS"; then
        {
            echo "=== the press has finished; one last reading ==="
            echo "$(date -u '+%Y-%m-%dT%H:%M:%SZ')"
            ids | sed 's/^/  /'
            echo "  world: $(world)"
        } >>"$LOG"
        break
    fi
    sleep "$INTERVAL"
done
