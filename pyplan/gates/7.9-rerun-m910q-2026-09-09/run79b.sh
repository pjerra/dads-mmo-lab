#!/usr/bin/env bash
# 7.9 controller-surface gate, RE-RUN on the merged Phase 8b tip, m910q, 2026-09-09.
#
# WHY A SECOND RE-RUN, EIGHT HOURS AFTER THE FIRST. The 2026-09-08 re-run graded 33/33 on
# `96a129dc`. `576a3f93` then MOVED the ready-wait table out of
# `gate-79-controller-surface.py` into `pyplan/gates/ready_wait.py` and rewrote the call
# signature (`wait_ready_for_game(entry, server_dir)`, rows taking `ReadyArgs` rather than an
# `auth_port` int). That is the harness's own dispatch, so the 33 counts were taken with an
# instrument that no longer exists in that spelling.
#
# THREE DIFFERENCES FROM `run79-rerun.sh`, AND EVERY ONE OF THEM IS THE POINT.
#
# 1. The scripts are invoked FROM THE CHECKOUT, at their committed paths. The 09-08 recipe
#    copied `starter.py` to the lane root; `starter.py` now does
#    `sys.path.insert(0, Path(__file__).resolve().parent.parent)` to reach `ready_wait`, so at
#    the lane root that resolves to `$HOME` and the import fails. Measured on this box before
#    this run: `import-probe.txt`.
# 2. `exits()` reads `docker inspect` after every stop, for all three games. The 09-08 run
#    found `tbc-mangosd` exiting 139 on a clean `stop_staged()` and booked it as a finding the
#    gate cannot see; that finding is only a finding if it is looked for again, and looked for
#    on the other two as well, so that "TBC alone" stays a measurement rather than a memory.
# 3. `$LANE` is `~/lane79b`. `~/lane79` is the 09-08 run's evidence and is not written to.
#
# The harness itself is UNMODIFIED, so the eleven-check instrument is the tip's own and the
# numbers below are comparable to the 09-08 table clause by clause.
#
# One game at a time; the previous game's stack is stopped through the app's own
# `Controller.stop()` before the next is touched, because m910q's rule is ONE server at a time.
# The ground is read BEFORE anything is started, and again after every game, so that no section
# can be read as proving a state it began in.
set -u
LANE=$HOME/lane79b
OUT=$LANE/out
PY=$HOME/gate81b-venv/bin/python
GATES=$LANE/checkout/pyplan/gates
GATE=$GATES/gate-79-controller-surface.py
STOPPER=$GATES/7.9-rerun-m910q-2026-09-08/stopper.py
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

run_game() {  # run_game <catalog-id> <server-dir> <logname>
  local game="$1" dir="$2" log="$OUT/$3"
  say "lane 7.9 re-run 09-09: $game against $dir at the merged tip -- console, backup, verify, restore round trip, timed stop/start"
  {
    echo "=== gate-79-controller-surface.py $game $dir ==="
    echo "code under test : $LANE/checkout at $(git -C "$LANE/checkout" rev-parse HEAD)"
    echo "harness         : $GATE"
    echo "ready-wait table: $GATES/ready_wait.py"
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

stop_stack() {  # stop_stack <catalog-id> <server-dir> <container>...
  local game="$1" dir="$2"; shift 2
  say "lane 7.9 re-run 09-09: stopping the $game stack through Controller.stop() before the next game"
  "$PY" "$STOPPER" "$game" "$dir" >> "$OUT/stops.txt" 2>&1
  exits "$game-after-stop" "$@"
}

mkdir -p "$OUT"
echo "=== 7.9 re-run (09-09) started $(date -Is) ===" > "$OUT/run.log"
{
  echo "code under test : $LANE/checkout at $(git -C "$LANE/checkout" rev-parse HEAD)"
  echo "interpreter     : $PY ($($PY --version 2>&1))"
  echo "box             : $(hostname)  $(uname -sr)"
  echo "docker          : $(docker --version)"
} >> "$OUT/run.log"

say "lane 7.9 re-run 09-09: reading the GROUND on m910q before any server is started"
ground start

run_game wow-tbc      "$HOME/tbc-7.4c"        gate79-tbc.log
ground after-tbc
stop_stack wow-tbc "$HOME/tbc-7.4c" tbc-db tbc-realmd tbc-mangosd

run_game wow-vanilla  "$HOME/vanilla-75b"     gate79-vanilla.log
ground after-vanilla
stop_stack wow-vanilla "$HOME/vanilla-75b" vanilla-db vanilla-realmd vanilla-mangosd

run_game wow-tortoise "$HOME/tortoise-server" gate79-tortoise.log
ground after-tortoise
stop_stack wow-tortoise "$HOME/tortoise-server" tortoise-db tortoise-realmd tortoise-mangosd

ground end
echo "=== 7.9 re-run (09-09) finished $(date -Is) ===" >> "$OUT/run.log"
say "lane 7.9 re-run 09-09: all three CMaNGOS games done; see ~/lane79b/out/run.log"
