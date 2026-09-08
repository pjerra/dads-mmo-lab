#!/usr/bin/env bash
# The photograph half of the 7.9 re-run: the Server and Console tabs of each CMaNGOS game,
# through the real ControllerView, one game at a time.
#
# WHY IT IS A SECOND PASS. `gate-79-controller-surface.py` drives `ControllerServices` on
# purpose -- it times the callables the buttons bind -- so nothing it does travels through a
# QWidget and there is no surface for it to photograph. This pass presses two of the same
# clauses through the widget and grabs a frame either side of each press.
#
# It starts each game's stack itself and stops it again, because the gate pass leaves every
# stack stopped and m910q's rule is one server at a time.
set -u
LANE=$HOME/lane79
OUT=$LANE/out
SHOTS=$LANE/shots
PY=$HOME/gate81b-venv/bin/python
export PYTHONPATH=$LANE/checkout/pylauncher
export QT_QPA_PLATFORM=offscreen

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> "$HOME/claude-activity.log"; }

shots_for() {  # shots_for <catalog-id> <server-dir> <logname>
  local game="$1" dir="$2" log="$OUT/$3"
  say "lane 7.9 re-run, shots: starting $game and photographing its Server and Console tabs"
  {
    echo "=== shots79.py $game $dir ==="
    echo "started (local): $(date -Is)"
    echo "--- docker ps BEFORE the start (the ground this pass begins from) ---"
    docker ps --format '{{.Names}}\t{{.Status}}'
    echo "---"
  } > "$log"
  "$PY" "$LANE/starter.py" "$game" "$dir" >> "$log" 2>&1
  "$PY" "$LANE/shots79.py" "$game" "$dir" "$SHOTS" >> "$log" 2>&1
  local rc=$?
  echo "--- shots79 exit code: $rc   finished: $(date -Is)" >> "$log"
  echo "$game shots exit $rc -> $3" >> "$OUT/run.log"
  say "lane 7.9 re-run, shots: stopping $game again"
  "$PY" "$LANE/stopper.py" "$game" "$dir" >> "$OUT/stops.txt" 2>&1
}

mkdir -p "$SHOTS"
say "lane 7.9 re-run: photograph pass starting; one server at a time, each stopped after its shots"
shots_for wow-tbc      "$HOME/tbc-7.4c"        shots-tbc.log
shots_for wow-vanilla  "$HOME/vanilla-75b"     shots-vanilla.log
shots_for wow-tortoise "$HOME/tortoise-server" shots-tortoise.log
say "lane 7.9 re-run: photograph pass done; every stack stopped"
