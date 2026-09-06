#!/usr/bin/env bash
# bug-checklist §41's gate, run whole on yulon-ubuntu against the finished
# AzerothCore WotLK install at /home/pk/wowserver (install_id 243c46e3).
#
# The gate, in §41's words: "choose loopback through the app, press Install
# again on the finished install, and the row is still 127.0.0.1 with a log line
# saying why it was left alone -- while a server whose loopback was never chosen
# still ends up advertising a reachable address."
#
# Order: before-state, the tab's loopback choice, press 1 (left alone), remove
# the record, press 2 (advertises again), the undo through the tab, put back.
#
# Nothing here compiles: every recorded stage of this install is already
# completed, so the engine skips them. The press is watched for a build line and
# killed if one appears (see the watchdog below).
#
# Takes one argument, `all` (the default) or `part2`. `part2` exists because
# the first run of this script died after press 1: `wc -l` on a yulon.log that
# did not exist yet printed its error to the same file the line count was read
# from, and `$(( wc: ... + 1 ))` under `set -u` exits the shell. The counter
# below was made robust and the rest of the gate was run as `part2`; press 1 was
# not repeated, because it had already produced its evidence.
set -u

PHASE="${1:-all}"
OUT=/home/pk/p7/out-b41
CO=/home/pk/p7/checkout
PY=/home/pk/p7/venv/bin/python
SERVER=/home/pk/wowserver
GATE="$CO/pyplan/gates/bug41-loopback-2026-09-05"
YLOG="$HOME/.local/share/yulon/yulon.log"

mkdir -p "$OUT"
export QT_QPA_PLATFORM=offscreen

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> "$OUT/run.log"; }
row() {
  docker exec -e MYSQL_PWD=password ac-database mysql -uroot -N -B \
    -e "SELECT address, localAddress FROM acore_auth.realmlist WHERE id=1;" 2>&1
}

echo "=== lane b41 §41 gate on yulon-ubuntu, started $(date -Is) ===" >> "$OUT/run.log"

# ---------------------------------------------------------------- before ----
if [ "$PHASE" = all ]; then
say "lane b41: recording the WotLK realm row, ufw and the install state before anything is changed"
{
  echo "=== before, $(date -Is)"
  echo "--- checkout SHA"; git -C "$CO" rev-parse HEAD
  echo "--- realm row (address, localAddress)"; row
  echo "--- full realmlist"
  docker exec -e MYSQL_PWD=password ac-database mysql -uroot -N -B \
    -e "SELECT id,name,address,localAddress,port FROM acore_auth.realmlist;" 2>&1
  echo "--- ufw status"; sudo -n ufw status 2>&1
  echo "--- ufw show added"; sudo -n ufw show added 2>&1
  echo "--- server dir, this app's own files"; ls -la "$SERVER"/.yulon-* 2>&1
  echo "--- install state"; cat "$SERVER/.yulon-install.json" 2>&1
  echo "--- docker ps"; docker ps --format '{{.Names}}\t{{.Status}}' 2>&1
  echo "--- df"; df -h / | tail -1
  echo "--- yulon.log size"; wc -l "$YLOG" 2>&1
} > "$OUT/before.txt" 2>&1

# --------------------------------------------- half one: the tab's choice ----
say "lane b41: clicking the Networking tab's new 'Only this computer (127.0.0.1)' radio, Show plan and Apply for real, offscreen. This rewrites the realm row to 127.0.0.1 on purpose; it is put back at the end of this run."
{ echo "=== widget_driver_b41_wotlk.py, started $(date -Is)"; echo "---"; } > "$OUT/widget-loopback.log"
"$PY" "$GATE/widget_driver_b41_wotlk.py" >> "$OUT/widget-loopback.log" 2>&1
echo "--- exit code: $?   finished $(date -Is)" >> "$OUT/widget-loopback.log"
{ echo "=== ufw after the loopback Apply, $(date -Is)"; sudo -n ufw status 2>&1
  echo "--- show added"; sudo -n ufw show added 2>&1; } > "$OUT/ufw-after-apply.txt" 2>&1
fi

# ------------------------------------------------------------- press one ----
press() {
  local tag="$1"
  local log="$OUT/press-$tag.log"
  local before_lines=0
  [ -f "$YLOG" ] && before_lines=$(wc -l < "$YLOG")
  echo "$before_lines" > "$OUT/yulon-log-lines-before-$tag.txt"
  {
    echo "=== press $tag: $PY -m yulon.install_wiring wow-wotlk --server-dir $SERVER"
    echo "started (box local): $(date -Is)"
    echo "---"
  } > "$log"
  local t0 t1 rc
  t0=$(date +%s)
  ( cd "$CO/pylauncher" && "$PY" -m yulon.install_wiring wow-wotlk --server-dir "$SERVER" ) \
    < /dev/null >> "$log" 2>&1 &
  local pid=$!
  # Watchdog: this install is finished, so nothing may compile. If a line that
  # only a real build prints shows up, the run is killed rather than allowed to
  # spend hours. `[stage] build` alone is not it -- the build stage prints a
  # skip line too -- so the watch is on the compiler's own output.
  while kill -0 "$pid" 2>/dev/null; do
    if grep -qE 'Building CXX|make\[[0-9]+\]|^\[ *[0-9]+%\]|#[0-9]+ \[[a-z-]+ +[0-9]+/[0-9]+\]' "$log"; then
      echo "!!! WATCHDOG: a compile line appeared; killing the press !!!" >> "$log"
      kill -TERM "$pid" 2>/dev/null
      sleep 5
      kill -KILL "$pid" 2>/dev/null
      break
    fi
    sleep 5
  done
  wait "$pid"; rc=$?
  t1=$(date +%s)
  echo "--- exit code: $rc   elapsed: $((t1-t0))s   finished $(date -Is)" >> "$log"
  {
    echo "=== after press $tag, $(date -Is)"
    echo "--- realm row"; row
    echo "--- record file"; cat "$SERVER/.yulon-network.json" 2>&1
    echo "--- install state"; cat "$SERVER/.yulon-install.json" 2>&1
    echo "--- docker ps"; docker ps --format '{{.Names}}\t{{.Status}}' 2>&1
    echo "--- new yulon.log lines since this press started (it held $before_lines before)"
    tail -n +"$(( before_lines + 1 ))" "$YLOG" 2>&1
  } > "$OUT/after-press-$tag.txt" 2>&1
}

if [ "$PHASE" = all ]; then
say "lane b41: pressing Install a second time on the finished WotLK install. Every recorded stage is already completed so the engine skips them; a watchdog kills the run if any compile line appears."
press chosen
say "lane b41: first press finished. See /home/pk/p7/out-b41/press-chosen.log"
fi

# --------------------------------------------------- the other half ----
say "lane b41: removing the recorded choice so the folder looks like a server whose loopback was never chosen, then pressing Install again - it should put a reachable address back"
{
  echo "=== removing the record, $(date -Is)"
  echo "--- the file, before it is removed"; cat "$SERVER/.yulon-network.json" 2>&1
  rm -v "$SERVER/.yulon-network.json" 2>&1
  echo "--- present now?"; ls -la "$SERVER/.yulon-network.json" 2>&1
  echo "--- realm row still says"; row
} > "$OUT/record-removed.txt" 2>&1

press never-chosen
say "lane b41: second press finished. See /home/pk/p7/out-b41/press-never-chosen.log"

# ------------------------------------------------------------- the undo ----
say "lane b41: showing the way back a user actually has - picking 'LAN (same Wi-Fi)' in the same tab and pressing Apply, which rewrites the record and the row"
{ echo "=== widget_driver_b41_wotlk_undo.py, started $(date -Is)"; echo "---"; } > "$OUT/widget-undo.log"
"$PY" "$GATE/widget_driver_b41_wotlk_undo.py" >> "$OUT/widget-undo.log" 2>&1
echo "--- exit code: $?   finished $(date -Is)" >> "$OUT/widget-undo.log"

# --------------------------------------------------------------- put back ----
say "lane b41: putting the box back - removing the record file this lane wrote, deleting any ufw rule the Applies added, and reading the realm row back"
{
  echo "=== putting the box back, $(date -Is)"
  echo "--- removing the record file (the folder had none when this lane started)"
  rm -v "$SERVER/.yulon-network.json" 2>&1
  echo "--- this app's own files in the server folder now"; ls -la "$SERVER"/.yulon-* 2>&1
  echo "--- ufw rules added by the Applies, before deleting"; sudo -n ufw show added 2>&1
  for p in 3724 8085 3306; do sudo -n ufw delete allow "$p"/tcp 2>&1; done
  echo "--- ufw show added, after"; sudo -n ufw show added 2>&1
  echo "--- ufw status, after"; sudo -n ufw status 2>&1
  echo "--- realm row, read back"; row
  echo "--- full realmlist, read back"
  docker exec -e MYSQL_PWD=password ac-database mysql -uroot -N -B \
    -e "SELECT id,name,address,localAddress,port FROM acore_auth.realmlist;" 2>&1
  echo "--- install state, unchanged?"; cat "$SERVER/.yulon-install.json" 2>&1
  echo "--- docker ps"; docker ps --format '{{.Names}}\t{{.Status}}' 2>&1
  echo "--- df"; df -h / | tail -1
} > "$OUT/put-back.txt" 2>&1

say "lane b41: gate run finished, box put back. Outputs in /home/pk/p7/out-b41/"
echo "=== lane b41 §41 gate finished $(date -Is) ===" >> "$OUT/run.log"
