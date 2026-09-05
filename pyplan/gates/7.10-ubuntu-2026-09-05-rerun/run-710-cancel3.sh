#!/usr/bin/env bash
# 7.10 re-run, install half, third attempt.
#
# Attempt 1 (`widget-cancel-wotlk-refused.log`): `wow-wotlk`, ports free, every preflight check
# [pass], refused on the container-name guard because the live install's stopped `ac-database`
# still belongs to install 243c46e3.
# Attempt 2 (`widget-cancel-tbc-refused-client.log`): `wow-tbc`, refused at preflight because
# the injected picker handed the same folder back for the CLIENT prompt too, and it has no
# `Data/` - `wow-tbc` has `requires_client_dir: true`.
# This attempt: `wow-tbc` again, with the picker telling the two prompts apart and a throwaway
# stand-in client folder (`Data/expansion.MPQ`, the `required_file` catalog.json names). The
# cancel happens during the clone, stages before anything reads a client.
#
# The clone must never reach the build stage. Stop is clicked 20 s into the clone; `docker ps`,
# `docker buildx ls` and `docker images` are read afterwards and recorded.
set -u
OUT=/home/pk/p7/out710
PY=/home/pk/p7/venv/bin/python
DRV=/home/pk/p7/drivers
CHOSEN=/home/pk/p7-cancel-install-tbc
FAKE=/home/pk/p7-fake-tbc-client
export QT_QPA_PLATFORM=offscreen

say() { ~/bin/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }
log() { echo "$(date -Is) $*" >> "$OUT/cancel-run.log"; }

echo "=== 7.10 re-run, install half, attempt 3 (wow-tbc + stand-in client), started $(date -Is) ===" >> "$OUT/cancel-run.log"
{
  echo "df before    : $(df -h /home/pk | tail -1)"
  echo "images before: $(docker images --format '{{.Repository}}:{{.Tag}}' | tr '\n' ' ')"
} >> "$OUT/cancel-run.log"

say "lane 7.10 re-run: third try at a real cancelled install - wow-tbc with a stand-in client folder. Stopping the live WotLK containers through the app again; they are started again after."
"$PY" /home/pk/p7/stopstart.py stop > "$OUT/containers-stop-3.txt" 2>&1
log "stopstart.py stop exit $?"

rm -rf "$CHOSEN" "$FAKE"
mkdir -p "$CHOSEN"

say "lane 7.10 re-run: clicking the TBC tile's Install for real, waiting for the clone line, then clicking Stop 20 seconds in. The cancel lands during the clone so no build ever starts."
{
  echo "=== widget_cancel_driver_tbc.py (attempt 3) ==="
  echo "started (local): $(date -Is)"
  echo "command: $PY $DRV/widget_cancel_driver_tbc.py"
  echo "---"
} > "$OUT/widget-cancel-tbc.log"
t0=$(date +%s)
"$PY" "$DRV/widget_cancel_driver_tbc.py" >> "$OUT/widget-cancel-tbc.log" 2>&1
rc=$?
t1=$(date +%s)
echo "--- exit code: $rc   elapsed: $((t1-t0))s   finished: $(date -Is)" >> "$OUT/widget-cancel-tbc.log"
log "widget_cancel_driver_tbc.py (attempt 3) exit $rc  $((t1-t0))s"

say "lane 7.10 re-run: recording what the cancelled wow-tbc folder holds and checking nothing was left building"
{
  echo "=== the cancelled wow-tbc folder after the 7.10 re-run cancel, $(date -Is)"
  echo "--- state file"
  cat "$CHOSEN/.yulon-install.json" 2>&1
  echo "--- top level"
  ls -la "$CHOSEN" 2>&1
  echo "--- src/"
  ls -la "$CHOSEN/src" 2>&1
  echo "--- compose files at the root"
  ls -l "$CHOSEN"/compose*.yml "$CHOSEN"/docker-compose*.yml 2>&1
  echo "--- .git at the root?"
  ls -d "$CHOSEN/.git" 2>&1
  echo "--- the stand-in client folder, untouched?"
  find "$FAKE" 2>&1
  echo "--- size"
  du -sh "$CHOSEN" 2>&1
  echo "--- docker ps (anything still running?)"
  docker ps --format '{{.Names}} {{.Image}} {{.Status}}' 2>&1
  echo "--- docker ps -a (anything left behind?)"
  docker ps -a --format '{{.Names}} {{.Image}} {{.Status}}' 2>&1
  echo "--- buildx builders"
  docker buildx ls 2>&1
  echo "--- any buildkit container?"
  docker ps -a --filter 'name=buildx_buildkit' --format '{{.Names}} {{.Status}}' 2>&1
  echo "(no lines above = none)"
  echo "--- images now (nothing new should have been BUILT)"
  docker images --format '{{.Repository}}:{{.Tag}} {{.CreatedSince}}' 2>&1
  echo "--- systemd-inhibit list (keep_awake released?)"
  systemd-inhibit --list 2>&1 | tail -5
  echo "--- df"
  df -h /home/pk | tail -1
} > "$OUT/cancel-folder-after-tbc.txt" 2>&1

say "lane 7.10 re-run: re-rendering the cancel copy over the wow-wotlk cancelled-clone folder shape, now that the real cancelled wow-tbc folder is there to contrast with"
mv "$OUT/copy-shapes.log" "$OUT/copy-shapes-before-tbc-cancel.log" 2>/dev/null
{
  echo "=== copy_shapes_driver.py ==="
  echo "started (local): $(date -Is)"
  echo "---"
} > "$OUT/copy-shapes.log"
t0=$(date +%s)
"$PY" "$DRV/copy_shapes_driver.py" >> "$OUT/copy-shapes.log" 2>&1
rc=$?
t1=$(date +%s)
echo "--- exit code: $rc   elapsed: $((t1-t0))s   finished: $(date -Is)" >> "$OUT/copy-shapes.log"
log "copy_shapes_driver.py exit $rc  $((t1-t0))s"

say "lane 7.10 re-run: removing the throwaway folders and starting the live WotLK containers again through the app"
rm -rf "$CHOSEN" "$FAKE" /home/pk/p7-wotlk-cancel-shape /home/pk/p7-cancel-install
"$PY" /home/pk/p7/stopstart.py start > "$OUT/containers-start.txt" 2>&1
log "stopstart.py start exit $?"
sleep 150
{
  echo "=== after the containers were started again, $(date -Is)"
  docker ps --format '{{.Names}}\t{{.Status}}\t{{.Ports}}' 2>&1
  ss -ltn 2>/dev/null | grep -E ':(3724|8085|3306)\b' || echo "(none listening yet)"
  df -h /home/pk | tail -1
} >> "$OUT/containers-start.txt"
echo "=== 7.10 re-run, install half, attempt 3 finished $(date -Is) ===" >> "$OUT/cancel-run.log"
say "lane 7.10 re-run: install half finished, containers back up. See /home/pk/p7/out710/cancel-run.log"
