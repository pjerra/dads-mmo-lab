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
# FAIL-CLOSED, in two layers, because each layer's fix exposed the next.
#
#   Round 1 threw the drivers' exit status away: `run_driver` ended on an
#   `echo`, so what it returned was that echo's 0, and with only `set -u` the
#   script walked on through its cleanup and printed "run finished" whatever had
#   happened. Round 2: `run_driver` returns the status, both drivers are invoked
#   fail-fast through `die`, and everything that puts the box back moved into an
#   EXIT trap so a failure could not skip it.
#
#   Round 2's trap then had the same defect one level down. It saved the status
#   it was called with and checked NOTHING of its own: `cp`, `chown`, the realm
#   UPDATE, `docker restart` and two DELETEs all ran with their results
#   discarded, so a restoration that silently did not happen still ended the run
#   "finished", exit 0. A trap that cannot report its own failure is worth as
#   little as a clause that cannot fail, and it is the piece protecting the
#   owner's live box.
#
#   Round 3: every restoration step below runs, then VERIFIES its own
#   postcondition against a fresh reading taken afterwards -- the realm row read
#   back on all three columns, the authserver's own "Added realm" line, the ufw
#   files by sha256 against the copies plus their ownership and mode, the
#   account and any orphaned account_access row gone, the world still on the pid
#   it started on. A step that fails does not stop the ones after it (a run
#   whose ufw restore failed still needs its realm row back); every failure is
#   written as a distinct `[RESTORATION FAILED]` line, and the script then exits
#   90 -- not the drivers' status, which stays in the log beside it.
#
# Two knobs exist only so those paths can be EXERCISED rather than asserted, and
# both are named in this folder's README:
#   T3_DRIVERS       point at a stand-in driver that fails on purpose
#   T3_REALM_ROW_ID  the row the realm UPDATE targets (default 1). The
#                    verification always reads id 1, the row this box actually
#                    advertises, so pointing the UPDATE at a row that does not
#                    exist is a restoration that cannot succeed, and the check
#                    has to notice.
#   T3_ALLOW_FAILURE says a non-zero exit is what this invocation was for.
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
REALM_PORT=8085
REALM_ROW_ID=${T3_REALM_ROW_ID:-1}
RESTORE_FAILED_STATUS=90

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }

mysql_q() {  # mysql_q <statement>
  # The password reaches the client in MYSQL_PWD, and `docker exec -e MYSQL_PWD`
  # with no `=value` takes it from THIS shell's environment, so the value is on
  # no argv -- not the mysql client's inside the container, not `docker exec`'s
  # out here. (Round 2 wrote `-e MYSQL_PWD=password` and a comment claiming the
  # opposite; the comment is the part that was true, so the code moved to meet
  # it.) The value itself is `password`, the AzerothCore compose fixture's
  # default root password, which is not generated, is the same on every install
  # this project makes, and reaches nothing off this box: `state-before.txt`
  # shows ac-database published as `127.0.0.1:3306->3306/tcp`, loopback only.
  MYSQL_PWD=password docker exec -e MYSQL_PWD ac-database mysql -uroot -N -B -e "$1" 2>/dev/null
}

sha_of() { sudo sha256sum "$1" 2>/dev/null | cut -d' ' -f1; }

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

# --- the EXIT trap: the box goes back whatever happened, and says whether it did
RESTORED=0
PROBLEMS=0

note() { echo "$*" >> "$OUT/restore.log"; }

verified() {  # verified <ok|fail> <what was checked, and what it read>
  if [ "$1" = ok ]; then
    note "  [RESTORED] $2"
  else
    PROBLEMS=$((PROBLEMS + 1))
    note "  [RESTORATION FAILED] $2"
    say "T3: RESTORATION FAILED -- $2"
  fi
}

restore_the_box() {
  local driver_rc=$?
  [ "$RESTORED" = 1 ] && exit "$driver_rc"
  RESTORED=1
  say "T3: restoring the box (the drivers left status $driver_rc) -- ufw, realm row, authserver, account"
  note "=== restore, run by the EXIT trap ==="
  note "the status the drivers left: $driver_rc"
  note "taken (local): $(date -Is)"
  note "realm UPDATE targets row id: $REALM_ROW_ID   (verification always reads id 1)"

  # --- 1. ufw ---------------------------------------------------------------
  note "--- ufw rules ---"
  if [ -f "$OUT/ufw-user.rules.before" ]; then
    sudo cp -a "$OUT/ufw-user.rules.before"  /etc/ufw/user.rules
    sudo cp -a "$OUT/ufw-user6.rules.before" /etc/ufw/user6.rules
    # 2026-09-08 finding 3a: `cp -a` carries the COPY's ownership, and the copy
    # was chowned to pk so the driver could read it. Ownership and mode restated.
    sudo chown root:root /etc/ufw/user.rules /etc/ufw/user6.rules
    sudo chmod 640 /etc/ufw/user.rules /etc/ufw/user6.rules
    local live4 want4 live6 want6 owner4 owner6
    live4=$(sha_of /etc/ufw/user.rules);  want4=$(sha_of "$OUT/ufw-user.rules.before")
    live6=$(sha_of /etc/ufw/user6.rules); want6=$(sha_of "$OUT/ufw-user6.rules.before")
    owner4=$(sudo stat -c '%U:%G %a' /etc/ufw/user.rules 2>/dev/null)
    owner6=$(sudo stat -c '%U:%G %a' /etc/ufw/user6.rules 2>/dev/null)
    note "  user.rules  live=$live4 before=$want4  $owner4"
    note "  user6.rules live=$live6 before=$want6  $owner6"
    if [ -n "$live4" ] && [ "$live4" = "$want4" ] && [ -n "$live6" ] && [ "$live6" = "$want6" ] \
       && [ "$owner4" = "root:root 640" ] && [ "$owner6" = "root:root 640" ]; then
      verified ok "/etc/ufw/user{,6}.rules match the copies taken before the Apply, root:root 640"
    else
      verified fail "/etc/ufw rules do NOT match the copies taken before the Apply, or their ownership/mode moved"
    fi
    sudo ufw status 2>&1 | sed 's/^/  ufw: /' >> "$OUT/restore.log"
  else
    note "  (no ufw copy was taken -- the run stopped before that step)"
    verified ok "nothing to put back: no ufw copy exists, so nothing was changed"
  fi

  # --- 2. the realm row -----------------------------------------------------
  note "--- the realm row ---"
  note "  as found now (id 1): $(mysql_q "SELECT CONCAT_WS('|', address, localAddress, localSubnetMask) FROM acore_auth.realmlist WHERE id=1;")"
  mysql_q "UPDATE acore_auth.realmlist SET address='$REALM_ADDRESS', localAddress='$REALM_ADDRESS', localSubnetMask='$REALM_MASK' WHERE id=$REALM_ROW_ID;"
  local row want
  # Always id 1: the row this box advertises is the thing that has to be right,
  # whatever row the UPDATE above was aimed at.
  row=$(mysql_q "SELECT CONCAT_WS('|', address, localAddress, localSubnetMask) FROM acore_auth.realmlist WHERE id=1;")
  want="$REALM_ADDRESS|$REALM_ADDRESS|$REALM_MASK"
  note "  read back (id 1): $row"
  note "  wanted          : $want"
  if [ "$row" = "$want" ]; then
    verified ok "realmlist id 1 is $want on all three columns"
  else
    verified fail "realmlist id 1 reads '$row', wanted '$want' -- the row this box advertises is WRONG"
  fi

  # --- 3. ac-authserver re-reads it -----------------------------------------
  # The first version of this check read `docker logs --tail 80` and asked
  # whether ANY line in it named the target address. The failing-restoration
  # exercise on 2026-09-09 caught it: with the row deliberately left at
  # 192.168.77.77 the container announced 192.168.77.77, and the check passed
  # anyway, because a PREVIOUS run's `Added realm ... 100.99.204.5:8085` was
  # still inside the last 80 lines. A stale marker read as a fresh one -- the
  # same defect this whole folder is about, found in the check written to
  # prevent it. Two changes: only lines written since this restart began count
  # (`--since`), and only the LAST announcement counts, because that is the one
  # the container is now serving.
  note "--- ac-authserver ---"
  local since announced
  since=$(date -u +%Y-%m-%dT%H:%M:%S)
  if docker restart ac-authserver >> "$OUT/restore.log" 2>&1; then
    sleep 10
    announced=$(docker logs --since "$since" ac-authserver 2>&1 | grep -a "Added realm" | tail -1)
    note "  the LAST 'Added realm' line written since $since UTC, when the restart began:"
    note "    ${announced:-(none)}"
    if [ -n "$announced" ] && printf '%s' "$announced" | grep -qF "at ${REALM_ADDRESS}:${REALM_PORT}"; then
      verified ok "ac-authserver restarted and now announces the realm at ${REALM_ADDRESS}:${REALM_PORT}"
    else
      verified fail "ac-authserver's newest announcement since the restart is '${announced:-(none)}', not 'at ${REALM_ADDRESS}:${REALM_PORT}'"
    fi
  else
    verified fail "docker restart ac-authserver did not succeed"
  fi

  # --- 4. the account this run creates --------------------------------------
  note "--- the account this run may have created ---"
  note "  before the delete: $(mysql_q "SELECT COUNT(*) FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';")"
  # Round 2: round 1's joined multi-table DELETE did not take and left an orphan
  # account_access row behind. Two plain statements, access first.
  mysql_q "DELETE FROM acore_auth.account_access WHERE id IN (SELECT id FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT');"
  mysql_q "DELETE FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';"
  local left orphans
  left=$(mysql_q "SELECT COUNT(*) FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';")
  orphans=$(mysql_q "SELECT COUNT(*) FROM acore_auth.account_access aa LEFT JOIN acore_auth.account a ON a.id=aa.id WHERE a.id IS NULL;")
  note "  after the delete: account rows=$left  orphaned account_access rows (any id)=$orphans"
  note "  account_access now: $(mysql_q "SELECT GROUP_CONCAT(CONCAT(id,':',gmlevel) ORDER BY id) FROM acore_auth.account_access;")"
  if [ "$left" = "0" ] && [ "$orphans" = "0" ]; then
    verified ok "$WIDGET_ACCOUNT is gone and no account_access row is orphaned"
  else
    verified fail "$WIDGET_ACCOUNT rows left=$left, orphaned account_access rows=$orphans"
  fi

  # --- 5. the world was never this lane's to touch --------------------------
  note "--- the world ---"
  local pid_now
  pid_now=$(pgrep worldserver | head -1)
  note "  pid at the start of this script: ${WORLD_PID_AT_START:-unknown}   now: ${pid_now:-none}"
  if [ -n "$pid_now" ] && [ "$pid_now" = "${WORLD_PID_AT_START:-}" ]; then
    verified ok "the world is up on the same pid it was on before this script ran ($pid_now)"
  else
    verified fail "the worldserver pid was ${WORLD_PID_AT_START:-unknown} and is now ${pid_now:-none}"
  fi

  # --- 6. the final reading -------------------------------------------------
  probe final >/dev/null 2>&1
  if [ -s "$OUT/state-final.txt" ]; then
    verified ok "state-final.txt written"
  else
    verified fail "state-final.txt was not written, so there is no final reading of this box"
  fi

  # --- the verdict ----------------------------------------------------------
  if [ "$PROBLEMS" -gt 0 ]; then
    note "=== RESTORATION FAILED: $PROBLEMS step(s) did not verify. The drivers' own status was $driver_rc. ==="
    say "T3: RESTORATION FAILED -- $PROBLEMS step(s); exiting $RESTORE_FAILED_STATUS"
    {
      echo "=== T3 run: RESTORATION FAILED -- $PROBLEMS step(s) did not verify ==="
      echo "the drivers' own status was $driver_rc; this script exits $RESTORE_FAILED_STATUS"
      echo "$(date -Is)"
    } >> "$OUT/run.log"
    exit "$RESTORE_FAILED_STATUS"
  fi
  note "=== every restoration verified; this script exits $driver_rc ==="
  if [ "$driver_rc" -ne 0 ]; then
    say "T3: this run FAILED (status $driver_rc); the box was put back and every step verified"
    echo "=== T3 run FAILED, status $driver_rc, box restored and verified $(date -Is) ===" >> "$OUT/run.log"
  else
    echo "=== T3 run finished $(date -Is) ===" >> "$OUT/run.log"
  fi
  exit "$driver_rc"
}

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
: > "$OUT/restore.log"
WORLD_PID_AT_START=$(pgrep worldserver | head -1)
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
  echo "world pid       : ${WORLD_PID_AT_START:-none} (must be the same one when this script exits)"
  echo "T3_ALLOW_FAILURE: ${T3_ALLOW_FAILURE:-0}"
  echo "T3_REALM_ROW_ID : $REALM_ROW_ID (the row the restore's UPDATE targets; the check reads id 1)"
} >> "$OUT/run.log"
trap restore_the_box EXIT

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
say "T3: both drivers green; the EXIT trap now puts the box back and checks that it did"
