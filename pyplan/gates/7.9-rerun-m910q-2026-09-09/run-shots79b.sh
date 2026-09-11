#!/usr/bin/env bash
# The photograph half of the 7.9 re-run, 2026-09-09: the Server and Console tabs of each
# CMaNGOS game, through the real ControllerView, one game at a time.
#
# WHY IT IS A SECOND PASS. `gate-79-controller-surface.py` drives `ControllerServices` on
# purpose -- it times the callables the buttons bind -- so nothing it does travels through a
# QWidget and there is no surface for it to photograph. This pass presses two of the same
# clauses through the widget and grabs a frame either side of each press.
#
# WHY IT IS RUN AGAIN EIGHT HOURS AFTER THE 09-08 PASS. Two reasons, and the second is the
# larger one:
#   * `starter.py` changed in `576a3f93` and can no longer be copied to the lane root -- see
#     `run79b.sh`'s header and `import-probe.txt`. It is invoked here from its committed path
#     inside the checkout.
#   * `pylauncher/yulon/ui/controller_view.py` is +549/-.. lines between `96a129dc` and
#     `c2fcf0ea`. Every attribute `shots79.py` reaches for -- `_tabs`, `status_label`,
#     `refresh_button`, `console_log`, `command_edit`, `send_button`, `shutdown()`, and the
#     `status_poll_ms=0` keyword that makes the "before" frames honest -- is on the surface
#     this pass photographs, and that surface is what changed most.
#
# It starts each game's stack itself and stops it again, because the gate pass leaves every
# stack stopped and m910q's rule is one server at a time.
set -u
LANE=$HOME/lane79b
OUT=$LANE/out
SHOTS=$LANE/shots
PY=$HOME/gate81b-venv/bin/python
GATES=$LANE/checkout/pyplan/gates
DRIVERS=$GATES/7.9-rerun-m910q-2026-09-08
export PYTHONPATH=$LANE/checkout/pylauncher
export QT_QPA_PLATFORM=offscreen

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> "$HOME/claude-activity.log"; }

exits() {  # exits <tag> <container>...
  local tag="$1"; shift
  {
    echo "=== exits-$tag  $(date -Is) ==="
    for c in "$@"; do
      docker inspect "$c" --format \
        '{{.Name}} exit={{.State.ExitCode}} running={{.State.Running}} started={{.State.StartedAt}} finished={{.State.FinishedAt}} restarts={{.RestartCount}} oom={{.State.OOMKilled}}' \
        2>&1
    done
  } >> "$OUT/exit-codes.txt"
}

shots_for() {  # shots_for <catalog-id> <server-dir> <logname> <container>...
  local game="$1" dir="$2" log="$OUT/$3"; shift 3
  say "lane 7.9 re-run 09-09, shots: starting $game and photographing its Server and Console tabs"
  {
    echo "=== shots79.py $game $dir ==="
    echo "drivers from   : $DRIVERS (committed paths, NOT copies -- starter.py imports ready_wait)"
    echo "started (local): $(date -Is)"
    echo "--- docker ps BEFORE the start (the ground this pass begins from) ---"
    docker ps --format '{{.Names}}\t{{.Status}}'
    echo "---"
  } > "$log"
  "$PY" "$DRIVERS/starter.py" "$game" "$dir" >> "$log" 2>&1
  "$PY" "$DRIVERS/shots79.py" "$game" "$dir" "$SHOTS" >> "$log" 2>&1
  local rc=$?
  echo "--- shots79 exit code: $rc   finished: $(date -Is)" >> "$log"
  echo "$game shots exit $rc -> $3" >> "$OUT/run.log"
  say "lane 7.9 re-run 09-09, shots: stopping $game again"
  "$PY" "$DRIVERS/stopper.py" "$game" "$dir" >> "$OUT/stops.txt" 2>&1
  exits "$game-after-shots-stop" "$@"
}

mkdir -p "$SHOTS"
say "lane 7.9 re-run 09-09: photograph pass starting; one server at a time, each stopped after its shots"
shots_for wow-tbc      "$HOME/tbc-7.4c"        shots-tbc.log      tbc-db tbc-realmd tbc-mangosd
shots_for wow-vanilla  "$HOME/vanilla-75b"     shots-vanilla.log  vanilla-db vanilla-realmd vanilla-mangosd
shots_for wow-tortoise "$HOME/tortoise-server" shots-tortoise.log tortoise-db tortoise-realmd tortoise-mangosd
say "lane 7.9 re-run 09-09: photograph pass done; every stack stopped"
