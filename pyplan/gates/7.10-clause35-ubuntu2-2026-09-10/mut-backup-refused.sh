#!/usr/bin/env bash
# MUTATION 2 -- the firewall backup.
#
# What has to be shown: with both backup copies forced to fail, the driver that
# presses Apply never starts.
#
# The force is `chattr +i` on both copy targets. An immutable file cannot be
# written by root either, so the runner's own `sudo cp -a` returns non-zero --
# a read-only target, the first of the two forms the ticket names, and the one
# that puts nothing between the script and the `cp` it really runs. No wrapper,
# no PATH games, no edit to the runner for the sake of the mutation.
#
# Nothing here touches /etc/ufw itself, and the live rules are read before and
# after so that the run can be seen not to have moved them.
set -u
OUT=/home/pk/lane710b/out-t23-mutbackup
LOG=$OUT/mutation.log

mkdir -p "$OUT"
: > "$LOG"
note() { echo "$*" | tee -a "$LOG"; }

~/claude-say "T23 mutation 2: making both firewall backup targets immutable so the runner's cp must fail, then running it -- the driver that presses Apply must never start. /etc/ufw itself is not touched."

note "=== T23 mutation 2: the firewall backup ==="
note "taken (local): $(date -Is)   (UTC: $(date -u -Is))"
note ""
note "--- /etc/ufw before, so the run can be seen not to have moved it ---"
sudo ufw status 2>&1 | tee -a "$LOG"
sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1 | tee -a "$LOG"
sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1 | tee -a "$LOG"

note ""
note "--- the two copy targets made immutable ---"
for f in "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before"; do
  sudo chattr -i "$f" 2>/dev/null || true
  rm -f "$f"; : > "$f"
  sudo chattr +i "$f" && note "chattr +i $f"
  lsattr "$f" 2>&1 | tee -a "$LOG"
done

note ""
note "--- the run ---"
T3_OUT=$OUT T3_ALLOW_FAILURE=1 bash /home/pk/lane710b/invoke-t3.sh >> "$LOG" 2>&1
RC=$?
note "the runner exited: $RC"

note ""
note "--- did any driver run? there is a log only if run_driver was called ---"
for f in clause35-falsify.log widget-run.log; do
  if [ -e "$OUT/$f" ]; then note "$OUT/$f EXISTS -- a driver ran"; else note "$OUT/$f does not exist -- that driver was never invoked"; fi
done
note "shots the falsify driver would have written: $(ls -A "$OUT/shots" 2>/dev/null | wc -l)"

note ""
note "--- the immutable flag off again, and the live rules read back ---"
for f in "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before"; do
  sudo chattr -i "$f" && note "chattr -i $f"
done
sudo ufw status 2>&1 | tee -a "$LOG"
sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1 | tee -a "$LOG"
sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1 | tee -a "$LOG"
note "docker inspect: $(docker inspect -f '{{.Name}} Running={{.State.Running}} Pid={{.State.Pid}}' ac-worldserver) $(docker inspect -f '{{.Name}} Running={{.State.Running}} Pid={{.State.Pid}}' ac-authserver)"

~/claude-say "T23 mutation 2 done: the immutable flags are off again and /etc/ufw was never written"
