#!/usr/bin/env bash
# T3, 2026-09-09, yulon-ubuntu2: the corrected 7.10 networking clause, proved
# falsifiable and then re-run inside the whole 33-clause widget half.
#
# Same shape as `../7.10-rerun-ubuntu2-2026-09-09/run-710-rerun.sh`, with only
# what T3 adds:
#
#   1. the code under test is moved to 528f219e (the merged tip this ticket's
#      worktree is based on); the 09-09 run measured 2c9bd211, three merges
#      earlier. `pylauncher/yulon` moved by exactly two files between them --
#      `apply.py`, which f8cd55df grew from 2105 lines to 2245 (it is not a new
#      file, and round 1 of this folder called it one), and `log_panel.py`
#      (+18/-2, the _StreamWorker guard) -- and the second of those is under the
#      Stop clause this driver presses;
#   2. `clause35_falsify.py` runs FIRST and presses Apply, so /etc/ufw is copied
#      aside and put back around it exactly as the 09-09 runner does around
#      sweep_driver4.py, and ac-authserver is restarted after it;
#   3. the widget driver's three LANE constants are set from the environment --
#      a new account name and its password (the "does not exist before the
#      click" clause is only a reading if the name is new) and a new shots
#      directory (so the 09-09 run's frames are not overwritten).
#
# FAIL-CLOSED (round 2). The round-1 version of this script logged each driver's
# exit status and then threw it away: `run_driver` ended on an `echo`, so its
# own return was that echo's 0, and with only `set -u` the script walked on
# through the cleanup and printed "run finished" whatever had happened. A
# falsification driver that died between B1 and B6 would have left the owner's
# box advertising 192.168.77.77 under a log that said the run finished.
#
# So: `run_driver` returns the driver's status, both drivers are invoked
# fail-fast through `die`, and everything that puts the box back -- the ufw
# rules, the realm row on BOTH address columns and its mask, the authserver
# restart, the account this run creates -- is in an EXIT trap. The trap runs on
# the way out of a failure exactly as it does on the way out of a success, and
# it re-raises the status it was called with, so the script's own exit code is
# the driver's. `restore.log` is written by the trap and is therefore present in
# both cases; `T3_ALLOW_FAILURE=1` says a non-zero exit is what this invocation
# was for, which is how the fail-closed path was exercised on the box.
set -u
set -o pipefail
LANE=/home/pk/lane710b
OUT=${T3_OUT:-$LANE/out-t3}
PY=/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python
DRV=${T3_DRIVERS:-$LANE/drivers-t3}
export QT_QPA_PLATFORM=offscreen
export PYTHONPATH=$LANE/checkout/pylauncher
export WIDGET_ACCOUNT=WIDGET0909T3
export WIDGET_ACCOUNT_PW=widget0909t3pw
export WIDGET_SHOTS=$OUT/shots
export T3_SHOTS=$OUT/shots

# What the box must hold when this script is done, whatever happened.
REALM_ADDRESS=100.99.204.5
REALM_MASK=255.255.255.0

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }

mysql_q() {  # mysql_q <statement>   -- password in the environment, never on a command line
  docker exec -e MYSQL_PWD=password ac-database mysql -uroot -N -B -e "$1" 2>/dev/null
}

probe() {  # probe <file-tag>
  local f="$OUT/state-$1.txt"
  {
    echo "=== state-$1 ==="
    echo "taken (local): $(date -Is)   (UTC: $(date -u -Is))"
    echo "--- docker ps ---"
    docker ps --format '{{.Names}}\t{{.Status}}\t{{.Ports}}'
    echo "--- worldserver pid ---"
    pgrep -a worldserver || echo "(no worldserver process)"
    echo "--- realmlist (address, localAddress, mask) ---"
    mysql_q "SELECT id, name, address, localAddress, localSubnetMask, port FROM acore_auth.realmlist;"
    echo "--- non-bot accounts ---"
    mysql_q "SELECT id, username FROM acore_auth.account WHERE username NOT LIKE 'RNDBOT%' ORDER BY id;"
    echo "--- account_access ---"
    mysql_q "SELECT id, gmlevel FROM acore_auth.account_access ORDER BY id;"
    echo "--- characters ---"
    mysql_q "SELECT COUNT(*) FROM acore_characters.characters;"
    echo "--- the owner's things ---"
    # Round 2: round 1 read /home/pk/wowserver/lua_scripts/LootPet.lua and
    # /home/pk/wowserver/etc/worldserver.conf. Both are "No such file" in its own
    # state-before AND state-after, so the pair agreed about nothing -- a probe
    # that cannot see the thing it is watching reports no change when the thing
    # is deleted. These are the paths the engine actually loads.
    mysql_q "SELECT c.name, c.level FROM acore_characters.characters c JOIN acore_auth.account a ON a.id=c.account WHERE a.username='PERZI';"
    ls -l /home/pk/wowserver/env/dist/etc/modules/lua_scripts/ 2>&1
    grep -n '^Logger.ALE' /home/pk/wowserver/env/dist/etc/worldserver.conf 2>&1 || echo "(no Logger.ALE line)"
    ls -l /home/pk/wowserver/env/dist/etc/worldserver.conf 2>&1
    echo "--- ufw ---"
    sudo ufw status 2>&1
    sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
    sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
    echo "--- disk ---"
    df -h /home/pk | tail -1
    echo "--- throwaway folders ---"
    for d in "$LANE/p710-doomed-install" "$LANE/p710-empty"; do
      if [ -e "$d" ]; then echo "$d: $(ls -A "$d" | wc -l) entries"; else echo "$d: absent"; fi
    done
  } > "$f" 2>&1
  echo "wrote $f"
}

# --- the EXIT trap: the box goes back whatever happened -----------------------
RESTORED=0
restore_the_box() {
  local rc=$?
  [ "$RESTORED" = 1 ] && exit "$rc"
  RESTORED=1
  say "T3: restoring the box (this script is exiting $rc) -- ufw, realm row, authserver, account"
  {
    echo "=== restore, run by the EXIT trap ==="
    echo "the status this script is exiting with: $rc"
    echo "taken (local): $(date -Is)"
    if [ -f "$OUT/ufw-user.rules.before" ]; then
      echo "--- putting /etc/ufw rules back exactly as they were ---"
      sudo cp -a "$OUT/ufw-user.rules.before"  /etc/ufw/user.rules
      sudo cp -a "$OUT/ufw-user6.rules.before" /etc/ufw/user6.rules
      # 2026-09-08 finding 3a: `cp -a` carries the COPY's ownership, and the copy
      # was chowned to pk so the driver could read it. Ownership and mode restated.
      sudo chown root:root /etc/ufw/user.rules /etc/ufw/user6.rules
      sudo chmod 640 /etc/ufw/user.rules /etc/ufw/user6.rules
      sudo ufw status 2>&1
      sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules \
        "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before" 2>&1
      sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
    else
      echo "--- no ufw copy was taken yet, so nothing to put back ---"
    fi
    echo "--- the realm row as found now ---"
    mysql_q "SELECT address, localAddress, localSubnetMask FROM acore_auth.realmlist WHERE id=1;"
    echo "--- forcing it to $REALM_ADDRESS on BOTH address columns, mask $REALM_MASK ---"
    mysql_q "UPDATE acore_auth.realmlist SET address='$REALM_ADDRESS', localAddress='$REALM_ADDRESS', localSubnetMask='$REALM_MASK' WHERE id=1;"
    echo "--- read back ---"
    mysql_q "SELECT address, localAddress, localSubnetMask FROM acore_auth.realmlist WHERE id=1;"
    echo "--- restarting ac-authserver so it re-reads the row it announces ---"
    docker restart ac-authserver
    sleep 10
    docker logs --tail 60 ac-authserver 2>&1 | grep -i "Added realm" \
      || echo "(no 'Added realm' line in the last 60 -- look at the whole log)"
    echo "--- the account this run may have created, removed ---"
    mysql_q "SELECT id, username FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';"
    # Round 2: round 1's joined multi-table DELETE did not take and left an
    # orphan account_access row behind. Two plain statements, access first.
    mysql_q "DELETE FROM acore_auth.account_access WHERE id IN (SELECT id FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT');"
    mysql_q "DELETE FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';"
    echo "after the delete (expect no rows for it, and no orphan access row):"
    mysql_q "SELECT id, username FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';"
    mysql_q "SELECT id, gmlevel FROM acore_auth.account_access ORDER BY id;"
    echo "--- the world was never stopped or started by this lane ---"
    pgrep -a worldserver || echo "(no worldserver process -- THIS WOULD BE A PROBLEM)"
  } >> "$OUT/restore.log" 2>&1
  probe final >/dev/null 2>&1
  if [ "$rc" -ne 0 ]; then
    say "T3: this run FAILED (status $rc); the box was put back anyway -- see $OUT/restore.log"
    echo "=== T3 run FAILED, status $rc, box restored by the EXIT trap $(date -Is) ===" >> "$OUT/run.log"
  else
    echo "=== T3 run finished $(date -Is) ===" >> "$OUT/run.log"
  fi
  exit "$rc"
}
trap restore_the_box EXIT

die() {  # die <status> <message>
  local rc="$1"; shift
  say "T3: $* -- stopping here"
  echo "STOPPED: $* (status $rc)" >> "$OUT/run.log"
  if [ "${T3_ALLOW_FAILURE:-0}" = "1" ]; then
    echo "(T3_ALLOW_FAILURE=1: a non-zero exit is what this invocation was for)" >> "$OUT/run.log"
  fi
  exit "$rc"
}

run_driver() {  # run_driver <path> <logname>  -- RETURNS the driver's status
  local d="$1" log="$OUT/$2"
  say "T3: running $(basename "$d") against the live WotLK server"
  {
    echo "=== $(basename "$d") ==="
    echo "started (local): $(date -Is)"
    echo "command: $PY $d"
    echo "---"
  } > "$log"
  local t0 t1 rc
  t0=$(date +%s)
  "$PY" "$d" >> "$log" 2>&1
  rc=$?
  t1=$(date +%s)
  echo "--- exit code: $rc   elapsed: $((t1-t0))s   finished: $(date -Is)" >> "$log"
  echo "$(basename "$d") exit $rc  $((t1-t0))s" >> "$OUT/run.log"
  return "$rc"          # round 2: round 1 ended on the echo above and returned 0
}

capture_ufw_effect() {  # what the Apply did, appended to the driver's own log
  {
    echo "--- ufw after the Apply press ---"
    sudo ufw status 2>&1
    sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
    echo "--- diff of user.rules against the copy taken before the driver ---"
    sudo diff "$OUT/ufw-user.rules.before" /etc/ufw/user.rules 2>&1 || true
  } >> "$OUT/clause35-falsify.log"
}

mkdir -p "$OUT" "$WIDGET_SHOTS"
echo "=== T3 clause-35 run started $(date -Is) ===" > "$OUT/run.log"
{
  echo "code under test : $LANE/checkout at $(git -C "$LANE/checkout" rev-parse HEAD)"
  echo "interpreter     : $PY  ($($PY --version 2>&1))"
  echo "PySide6         : $($PY -c 'import PySide6; print(PySide6.__version__)' 2>&1)"
  echo "pydantic        : $($PY -c 'import pydantic; print(pydantic.VERSION)' 2>&1)"
  echo "yulon imported  : $($PY -c 'import pathlib, yulon; print(pathlib.Path(yulon.__file__).resolve())' 2>&1)"
  echo "box             : $(hostname)  $(uname -sr)   (hostname is the rebuild's; the machine is yulon-ubuntu2)"
  echo "docker          : $(docker --version)"
  echo "server dir      : /home/pk/wowserver"
  echo "drivers         : $DRV"
  echo "account         : $WIDGET_ACCOUNT (new tonight)"
  echo "shots           : $WIDGET_SHOTS"
  echo "QT_QPA_PLATFORM : $QT_QPA_PLATFORM"
  echo "T3_ALLOW_FAILURE: ${T3_ALLOW_FAILURE:-0}"
} >> "$OUT/run.log"

say "T3: capturing server state before anything is pressed"
probe before

# the folder the install clause insists is empty is shared with the 09-09 run
rm -rf "$LANE/p710-doomed-install"; mkdir -p "$LANE/p710-doomed-install"

# --- the falsify driver PRESSES Apply, so /etc/ufw is copied aside -----------
say "T3: copying /etc/ufw rules aside before the driver that presses Apply"
sudo cp -a /etc/ufw/user.rules  "$OUT/ufw-user.rules.before"
sudo cp -a /etc/ufw/user6.rules "$OUT/ufw-user6.rules.before"
sudo chown pk:pk "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before"

run_driver "$DRV/clause35_falsify.py" clause35-falsify.log
RC=$?
capture_ufw_effect        # the reading belongs to the driver whether it passed or not
[ "$RC" -eq 0 ] || die "$RC" "clause35_falsify.py exited $RC, so the falsification did not hold"

# --- the whole widget half, all 33 clauses -----------------------------------
run_driver "$DRV/widget_driver.py" widget-run.log
RC=$?
[ "$RC" -eq 0 ] || die "$RC" "widget_driver.py exited $RC, so a clause of the 33 failed"

probe after
say "T3: both drivers green; the EXIT trap now puts the box back"
