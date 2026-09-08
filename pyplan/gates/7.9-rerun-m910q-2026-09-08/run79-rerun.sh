#!/usr/bin/env bash
# 7.9 controller-surface gate, RE-RUN on the merged Phase 8b tip, m910q, 2026-09-08.
#
# Same harness as pyplan/gates/gate-79-controller-surface.py at 96a129dc, run from a fresh
# clone of that commit (~/lane79/checkout). One game at a time; the previous game's stack is
# stopped through the app's own Controller.stop() before the next is touched, because m910q's
# rule is ONE server at a time.
#
# The ground is read BEFORE anything is started, and again after every game, so that no
# section below can be read as proving a state it started in.
set -u
LANE=$HOME/lane79
OUT=$LANE/out
PY=$HOME/gate81b-venv/bin/python
GATE=$LANE/checkout/pyplan/gates/gate-79-controller-surface.py
export PYTHONPATH=$LANE/checkout/pylauncher

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> "$HOME/claude-activity.log"; }

ground() {  # ground <tag>
  local f="$OUT/ground-$1.txt"
  {
    echo "=== ground-$1 ==="
    echo "taken (local): $(date -Is)   (UTC: $(date -u -Is))"
    echo "--- docker ps -a (all containers) ---"
    docker ps -a --format '{{.Names}}\t{{.Status}}\t{{.Image}}'
    echo "--- listening game ports ---"
    ss -ltn 2>/dev/null | grep -E ':(3724|3443|8085|8888|8129)\b' || echo "(none of 3724/3443/8085/8888/8129)"
    echo "--- disk ---"
    df -h /home/pk | tail -1
    echo "--- memory ---"
    free -g | head -2
    echo "--- backup dirs that exist right now ---"
    for d in "$HOME"/tbc-7.4c "$HOME"/vanilla-75b "$HOME"/tortoise-server; do
      if [ -d "$d/sql_scripts/backups" ]; then
        echo "$d/sql_scripts/backups: $(ls -1 "$d/sql_scripts/backups" | wc -l) entries"
      else
        echo "$d/sql_scripts/backups: absent"
      fi
    done
  } > "$f" 2>&1
  echo "wrote $f"
}

run_game() {  # run_game <catalog-id> <server-dir> <logname>
  local game="$1" dir="$2" log="$OUT/$3"
  say "lane 7.9 re-run: $game against $dir at 96a129dc -- console, backup, verify, restore round trip, timed stop/start"
  {
    echo "=== gate-79-controller-surface.py $game $dir ==="
    echo "code under test : $LANE/checkout at $(git -C "$LANE/checkout" rev-parse HEAD)"
    echo "interpreter     : $PY ($($PY --version 2>&1))"
    echo "PySide6         : $($PY -c 'import PySide6; print(PySide6.__version__)' 2>&1)"
    echo "started (local) : $(date -Is)"
    echo "---"
  } > "$log"
  local t0 t1 rc
  t0=$(date +%s)
  "$PY" "$GATE" "$game" "$dir" >> "$log" 2>&1
  rc=$?
  t1=$(date +%s)
  echo "--- exit code: $rc   elapsed: $((t1-t0))s   finished: $(date -Is)" >> "$log"
  echo "$game exit $rc  $((t1-t0))s  -> $3" >> "$OUT/run.log"
  return $rc
}

stop_stack() {  # stop_stack <catalog-id> <server-dir>
  say "lane 7.9 re-run: stopping the $1 stack through Controller.stop() before the next game"
  "$PY" "$LANE/stopper.py" "$1" "$2" >> "$OUT/stops.txt" 2>&1
}

mkdir -p "$OUT"
echo "=== 7.9 re-run started $(date -Is) ===" > "$OUT/run.log"
{
  echo "code under test : $LANE/checkout at $(git -C "$LANE/checkout" rev-parse HEAD)"
  echo "interpreter     : $PY ($($PY --version 2>&1))"
  echo "box             : $(hostname)  $(uname -sr)"
  echo "docker          : $(docker --version)"
} >> "$OUT/run.log"

say "lane 7.9 re-run: reading the GROUND on m910q before any server is started"
ground start

run_game wow-tbc      "$HOME/tbc-7.4c"        gate79-tbc.log
ground after-tbc
stop_stack wow-tbc "$HOME/tbc-7.4c"

run_game wow-vanilla  "$HOME/vanilla-75b"     gate79-vanilla.log
ground after-vanilla
stop_stack wow-vanilla "$HOME/vanilla-75b"

run_game wow-tortoise "$HOME/tortoise-server" gate79-tortoise.log
ground after-tortoise
stop_stack wow-tortoise "$HOME/tortoise-server"

ground end
echo "=== 7.9 re-run finished $(date -Is) ===" >> "$OUT/run.log"
say "lane 7.9 re-run: all three CMaNGOS games done; see ~/lane79/out/run.log"
