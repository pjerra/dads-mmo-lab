#!/usr/bin/env bash
# 7.10 re-run, install half: the honest-cancel copy from a REAL cancelled install, on the
# merged engine, with the two findings the 2026-09-05 run filed asserted by name.
#
# The live 7.2 WotLK install publishes 3724/8085/3306, which preflight refuses an install
# against. So its containers are STOPPED THROUGH THE APP (`Controller.stop()`, the Server tab's
# own call) before the driver and STARTED again after. Nothing is removed: `docker ps -a` is
# recorded either side.
#
# The clone must never reach the build stage - a build compiles for hours. The driver clicks
# Stop 20 s into clone-core and the spine raises between stages; `docker ps` is read after the
# dialog and `docker buildx du`/`docker ps` are checked for a builder or a build container.
set -u
OUT=/home/pk/p7/out710
PY=/home/pk/p7/venv/bin/python
DRV=/home/pk/p7/drivers
CHOSEN=/home/pk/p7-cancel-install
export QT_QPA_PLATFORM=offscreen

say() { ~/bin/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }

log() { echo "$(date -Is) $*" >> "$OUT/cancel-run.log"; }

echo "=== 7.10 re-run, install half, started $(date -Is) ===" > "$OUT/cancel-run.log"
{
  echo "code under test : /home/pk/p7/checkout at $(git -C /home/pk/p7/checkout rev-parse HEAD)"
  echo "interpreter     : $PY  ($($PY --version 2>&1))"
  echo "throwaway folder: $CHOSEN"
  echo "df before       : $(df -h /home/pk | tail -1)"
} >> "$OUT/cancel-run.log"

say "lane 7.10 re-run: stopping the live WotLK containers THROUGH THE APP so the cancel run's install can get past preflight. They are stopped, never removed, and started again straight after."
"$PY" /home/pk/p7/stopstart.py stop > "$OUT/containers-stop.txt" 2>&1
log "stopstart.py stop exit $?"

rm -rf "$CHOSEN"
mkdir -p "$CHOSEN"

say "lane 7.10 re-run: running widget_cancel_driver.py - clicks the WotLK tile's Install, waits for the engine's clone-core line, clicks Stop 20s in, and reads the modal. The clone is stopped during clone-core so no build ever starts."
{
  echo "=== widget_cancel_driver.py ==="
  echo "started (local): $(date -Is)"
  echo "command: $PY $DRV/widget_cancel_driver.py"
  echo "---"
} > "$OUT/widget-cancel.log"
t0=$(date +%s)
"$PY" "$DRV/widget_cancel_driver.py" >> "$OUT/widget-cancel.log" 2>&1
rc=$?
t1=$(date +%s)
echo "--- exit code: $rc   elapsed: $((t1-t0))s   finished: $(date -Is)" >> "$OUT/widget-cancel.log"
log "widget_cancel_driver.py exit $rc  $((t1-t0))s"

say "lane 7.10 re-run: the cancel driver finished; recording what the cancelled folder holds and checking no build container or builder was left running"
{
  echo "=== the cancelled folder after the 7.10 re-run cancel, $(date -Is)"
  echo "--- state file"
  cat "$CHOSEN/.yulon-install.json" 2>&1
  echo "--- is docker-compose.yml upstream's (git-tracked, unmodified) or the engine's?"
  git -C "$CHOSEN" ls-files docker-compose.yml 2>&1
  echo "(empty status = tracked and unmodified)"
  git -C "$CHOSEN" status --porcelain docker-compose.yml 2>&1
  echo "--- first 3 lines of docker-compose.yml"
  head -3 "$CHOSEN/docker-compose.yml" 2>&1
  echo "--- compose files present"
  ls -l "$CHOSEN"/compose*.yml "$CHOSEN"/docker-compose*.yml 2>&1
  echo "--- HEAD"
  git -C "$CHOSEN" log --oneline -1 2>&1
  echo "--- size"
  du -sh "$CHOSEN" 2>&1
  echo "--- untracked files (what the engine wrote, if anything)"
  git -C "$CHOSEN" status --porcelain 2>&1
  echo "--- docker ps (anything still running?)"
  docker ps --format '{{.Names}} {{.Image}} {{.Status}}' 2>&1
  echo "--- docker ps -a (anything left behind?)"
  docker ps -a --format '{{.Names}} {{.Image}} {{.Status}}' 2>&1
  echo "--- buildx builders and their state"
  docker buildx ls 2>&1
  echo "--- any buildkit container?"
  docker ps -a --filter 'name=buildx_buildkit' --format '{{.Names}} {{.Status}}' 2>&1
  echo "(no lines above = none)"
  echo "--- systemd-inhibit list (keep_awake released?)"
  systemd-inhibit --list 2>&1 | tail -5
  echo "--- df"
  df -h /home/pk | tail -1
} > "$OUT/cancel-folder-after.txt" 2>&1

say "lane 7.10 re-run: removing the throwaway folder and starting the live WotLK containers again through the app"
rm -rf "$CHOSEN"
"$PY" /home/pk/p7/stopstart.py start > "$OUT/containers-start.txt" 2>&1
log "stopstart.py start exit $?"
sleep 120
{
  echo "=== after the containers were started again, $(date -Is)"
  docker ps --format '{{.Names}}\t{{.Status}}\t{{.Ports}}' 2>&1
  ss -ltn 2>/dev/null | grep -E ':(3724|8085|3306)\b' || echo "(none listening yet)"
  df -h /home/pk | tail -1
} >> "$OUT/containers-start.txt"
echo "=== 7.10 re-run, install half, finished $(date -Is) ===" >> "$OUT/cancel-run.log"
say "lane 7.10 re-run: install half finished, containers back up. See /home/pk/p7/out710/cancel-run.log"
