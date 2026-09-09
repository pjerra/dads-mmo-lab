#!/usr/bin/env bash
# Every image tag and untag the daemon performs, with the daemon's own clock.
#
# This is the ONLY instrument in this folder that can see the `-failed` tags.
# `_restore_rollback()` names the new build `<ref>-failed` before it moves a
# single tag -- so that a retag which fails part-way can be undone onto that
# name rather than leaving the tags mixed -- and `_let_go()` removes those names
# again as soon as the restore has settled, either way. Between the two there is
# no sleep and no container work: on this box the whole window measured under a
# second, which no `docker images` poll can be relied on to land inside.
#
# `docker events` is a push stream from the daemon, not a poll of it, so the
# window's width does not matter. It is also a SECOND process that the press
# knows nothing about, which is the property that makes it evidence: the press's
# own log says it named the new build; this says the daemon did it, and when.
#
# Started before the press and killed after it. `--since` is passed so the
# stream also carries anything from the seconds before this script got going.
set -u
OUT=${GATE_OUT:-$HOME/restore-gate-out}
LOG=$OUT/events.log
SINCE=${1:-30s}

mkdir -p "$OUT"
{
    echo "=== docker image events, from a second process ==="
    echo "started $(date -u '+%Y-%m-%dT%H:%M:%SZ'), --since $SINCE"
} >>"$LOG"
exec docker events --since "$SINCE" --filter type=image \
    --format '{{.TimeNano}} {{.Action}} {{.Actor.Attributes.name}}' >>"$LOG" 2>&1
