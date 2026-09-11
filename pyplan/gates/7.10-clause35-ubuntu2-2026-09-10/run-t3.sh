#!/usr/bin/env bash
# T3, 2026-09-09 / T23, 2026-09-10, yulon-ubuntu2: the corrected 7.10 networking
# clause, proved falsifiable and then re-run inside the whole 33-clause widget
# half.
#
# T23 (2026-09-10) is this file's fifth round, and it closes the two fail-open
# paths the owed Codex pass on `a4454ec0` found in the 09-09 runner. Neither is
# in shipped code; both are in the harness that is supposed to protect the
# owner's live box, which is where every previous round's defect has been too.
# The two rules T23 adds are stated where they are enforced, and they are:
#
#   THE BOUNDARY RULE  -- restore_the_box(), section 3
#   THE BACKUP RULE    -- backup_the_firewall_or_stop(), and the trap's section 1
#
# The 09-09 folder next door is left exactly as it was merged; it is the record
# of what round 4 shipped, defects and all, and this folder is what replaces it.
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
#   Round 4 (2026-09-09) put a `Z` on the `--since` stamp and added a precheck
#   that the window was empty before the restart. The owed Codex pass found that
#   this still fails open, because the stamp is taken from the CLOCK and the
#   restart happens afterwards: everything between `date -u` and the container
#   coming back is a gap, and an `Added realm` line written into that gap is
#   inside the window and reads as the restarted server's own announcement. The
#   precheck cannot see it -- the precheck runs BEFORE the gap opens. Round 5
#   moves the boundary off the clock and onto the restarted container itself.
#
#   The same pass found that the two privileged `cp`s that copy /etc/ufw aside,
#   and their `chown`, were unchecked, and with no `set -e` the driver that
#   presses Apply ran regardless. Worse, the trap read a missing copy as proof
#   that nothing had been changed and wrote `[RESTORED]`. Round 5 refuses to
#   start the driver without both backups verified, and once that phase has been
#   entered treats a missing backup as a restoration FAILURE.
#
# Knobs exist only so those paths can be EXERCISED rather than asserted, and all
# of them are named in this folder's README:
#   T3_DRIVERS       point at a stand-in driver that fails on purpose
#   T3_REALM_ROW_ID  the row the realm UPDATE targets (default 1). The
#                    verification always reads id 1, the row this box actually
#                    advertises, so pointing the UPDATE at a row that does not
#                    exist is a restoration that cannot succeed, and the check
#                    has to notice.
#   T3_ALLOW_FAILURE says a non-zero exit is what this invocation was for.
#   T23_PLANT_IN_GAP writes one matching `Added realm` line into ac-authserver's
#                    own log stream at the one place round 4 left open: after
#                    the precheck, before `docker restart`. Marked
#                    `[T23-PLANTED]` so a reader of the container's log can see
#                    what it is, and still matching what the check greps for.
#   T23_DROP_BACKUP_AFTER_APPLY_PHASE
#                    deletes both firewall copies after the Apply-capable phase
#                    has been entered, so the trap meets the case the Codex
#                    finding describes: a backup that is not there when it is
#                    needed.
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
BACKUP_REFUSED_STATUS=70

# Set to 1 at the single point past which something in this run could have
# pressed Apply and changed the owner's firewall. Everything before that point
# may end with "no copy was taken, so nothing was changed"; nothing after it may.
APPLY_PHASE_ENTERED=0
# The sizes the two copies had when they were verified against the live files.
# Empty until the backup step succeeds; the trap compares against them so that a
# copy that is still there but has been truncated is caught as well as one that
# has gone.
UFW4_BACKUP_BYTES=
UFW6_BACKUP_BYTES=

# `$REALM_ROW_ID` is interpolated unquoted into an UPDATE. It is a knob this
# folder's own exercise sets, so it is checked here rather than trusted: a
# non-numeric value would be SQL, and the check runs before the EXIT trap is
# installed, so a bad one stops the script before anything on the box is touched.
if ! [[ $REALM_ROW_ID =~ ^[0-9]+$ ]]; then
  echo "T3_REALM_ROW_ID must be a whole number; got '$REALM_ROW_ID'" >&2
  exit 64
fi

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
    # T23: `docker ps` says a container is up; it does not say WHICH run of it is
    # up. `StartedAt` does, and it is the same field the restore's boundary rule
    # is built on, so every capture in this folder carries the process the run
    # was talking to, at that moment, in the field the rule uses.
    echo "--- docker inspect: is the process alive, and which start of it ---"
    for c in ac-worldserver ac-authserver ac-database; do
      docker inspect -f '{{.Name}} Running={{.State.Running}} Pid={{.State.Pid}} StartedAt={{.State.StartedAt}} Restarting={{.State.Restarting}}' "$c" 2>&1
    done
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
    say "T23: RESTORATION FAILED -- $2"
  fi
}

restore_the_box() {
  local driver_rc=$?
  [ "$RESTORED" = 1 ] && exit "$driver_rc"
  RESTORED=1
  say "T23: restoring the box (the drivers left status $driver_rc) -- ufw, realm row, authserver, account"
  note "=== restore, run by the EXIT trap ==="
  note "the status the drivers left: $driver_rc"
  note "taken (local): $(date -Is)"
  note "realm UPDATE targets row id: $REALM_ROW_ID   (verification always reads id 1)"

  # --- 1. ufw ---------------------------------------------------------------
  #
  # THE BACKUP RULE, second half. Round 4 asked one question here -- does the
  # copy exist? -- and read "no" as "then nothing was changed, [RESTORED]". That
  # is a question with three answers collapsed into two. The copy can be absent
  # because nothing that could touch the firewall ever ran, and it can be absent
  # because it was never made, or was lost, while something that presses Apply
  # was running. In the second case the live firewall may already be altered and
  # the one thing that could have told us is gone, so the absence of a copy is
  # not evidence that nothing changed -- it is the loss of the evidence. Once
  # the Apply-capable phase has been entered, a backup that is missing, empty or
  # no longer the size it was verified at is a RESTORATION FAILURE, never
  # `[RESTORED]`.
  note "--- ufw rules ---"
  local have4=0 have6=0 size4 size6
  size4=$(stat -c %s "$OUT/ufw-user.rules.before"  2>/dev/null || echo "")
  size6=$(stat -c %s "$OUT/ufw-user6.rules.before" 2>/dev/null || echo "")
  [ -s "$OUT/ufw-user.rules.before" ]  && [ -n "$UFW4_BACKUP_BYTES" ] && [ "$size4" = "$UFW4_BACKUP_BYTES" ] && have4=1
  [ -s "$OUT/ufw-user6.rules.before" ] && [ -n "$UFW6_BACKUP_BYTES" ] && [ "$size6" = "$UFW6_BACKUP_BYTES" ] && have6=1
  note "  the Apply-capable phase was entered: $APPLY_PHASE_ENTERED"
  note "  ufw-user.rules.before : ${size4:-(absent)} bytes, verified at ${UFW4_BACKUP_BYTES:-(never)} -> usable=$have4"
  note "  ufw-user6.rules.before: ${size6:-(absent)} bytes, verified at ${UFW6_BACKUP_BYTES:-(never)} -> usable=$have6"
  if [ "$have4" != 1 ] || [ "$have6" != 1 ]; then
    if [ "$APPLY_PHASE_ENTERED" = 1 ]; then
      verified fail "the Apply-capable phase WAS entered and a firewall backup is missing, empty or short (user.rules usable=$have4, user6.rules usable=$have6): the live rules can be compared with nothing, and that is NOT evidence that nothing changed"
      {
        sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
        sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
      } | sed 's/^/  the live rules, unrestorable, as they stand now: /' >> "$OUT/restore.log"
    else
      note "  (the run stopped before the Apply-capable phase, so no copy was owed)"
      verified ok "nothing to put back: nothing in this run could have pressed Apply"
    fi
  else
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
  #
  # Round 4, both reviewers, the same defect again. `date -u +%Y-%m-%dT%H:%M:%S`
  # has no `Z` and no offset, and Docker's documented rule for `--since` is that
  # a zone-less stamp is read in the CLIENT'S LOCAL timezone. This box is +02:00,
  # so the window opened TWO HOURS before the restart while the note line printed
  # "UTC" -- the stale marker was still wide open, and an authserver that came
  # back without announcing anything would have had the previous run's line
  # picked up by `tail -1` and read as verification. One character (`Z`), and
  # then a structural capture that proves the window is closed rather than
  # asserting it: the same `--since` query, run BEFORE the restart, must count
  # ZERO `Added realm` lines. That reading also covers the no-announcement case
  # without staging a silent authserver -- an empty window reads `(none)` below
  # and fails.
  #
  # Round 5 (T23), the owed Codex pass on round 4: it is STILL open, and the
  # precheck is structurally unable to close it. The stamp comes off the clock
  # and `docker restart` runs afterwards, so there is a gap -- the precheck, the
  # stamp, whatever else the script does, the stop, the start, the container
  # coming back -- and every one of those moments lies INSIDE the window. A line
  # written into that gap is read by the query below as the restarted server's
  # own announcement. The precheck cannot see it, because the precheck happens
  # before the gap opens; it only ever proved that the window was empty at the
  # instant it was opened, which was never the question.
  #
  # THE BOUNDARY RULE. The only window this check may read is the restarted
  # container's own `StartedAt`, taken from `docker inspect` AFTER `docker
  # restart` has returned, and used only if it DIFFERS from the value read
  # before the restart. If it is unreadable, or unchanged, this check reads no
  # window at all and records a RESTORATION FAILURE: a line found in some other
  # window is not evidence that THIS restart announced anything. `StartedAt` is
  # already an RFC3339 stamp in UTC with a `Z` -- it is the container's own
  # account of when the process behind these log lines began, so nothing written
  # before the restart can be inside it, whoever wrote it and whenever they did.
  # The stamp handed to Docker is printed verbatim, because a window nobody can
  # read is a window nobody can check.
  #
  # The precheck below stays. It is no longer load-bearing and says so: it is a
  # cheap reading of how the box looked going in, and the run does not depend on
  # it.
  note "--- ac-authserver ---"
  local since announced stale precheck_since started_before started_after planted_rc
  precheck_since=$(date -u +%Y-%m-%dT%H:%M:%SZ)
  stale=$(docker logs --since "$precheck_since" ac-authserver 2>&1 | grep -ac "Added realm")
  note "  precheck (NOT load-bearing since T23): --since '$precheck_since', a stamp off the clock"
  note "  'Added realm' lines already inside that clock window, before the restart: $stale (expected 0)"
  if [ "$stale" != "0" ]; then
    note "  (a non-zero count here is a reading about the box, not the verdict; the verdict below"
    note "   is bounded by the container's own StartedAt and cannot see anything written before it)"
  fi
  started_before=$(docker inspect -f '{{.State.StartedAt}}' ac-authserver 2>/dev/null)
  note "  ac-authserver StartedAt BEFORE the restart: ${started_before:-(unreadable)}"

  # --- THE GAP -- this is the exact place round 4 left open, and the knob below
  # is here so that it can be filled on purpose and the run watched going RED.
  if [ "${T23_PLANT_IN_GAP:-0}" = "1" ]; then
    note "  T23_PLANT_IN_GAP=1: writing one matching 'Added realm' line into ac-authserver's own"
    note "  log stream HERE, after the precheck and before 'docker restart' -- the gap round 4's"
    note "  clock stamp left inside its own window."
    docker exec ac-authserver sh -c \
      "printf '%s\n' '[T23-PLANTED] Added realm \"Yulon ubuntu2\" at ${REALM_ADDRESS}:${REALM_PORT}.' > /proc/1/fd/1" \
      >/dev/null 2>&1
    planted_rc=$?
    note "  planted (docker exec exit $planted_rc); it is inside '$precheck_since' and it will be"
    note "  OUTSIDE the StartedAt window the verdict below is read through."
  fi

  if docker restart ac-authserver >> "$OUT/restore.log" 2>&1; then
    started_after=$(docker inspect -f '{{.State.StartedAt}}' ac-authserver 2>/dev/null)
    note "  ac-authserver StartedAt AFTER  the restart: ${started_after:-(unreadable)}"
    if [ -z "$started_after" ] || [ "$started_after" = "$started_before" ]; then
      verified fail "ac-authserver's StartedAt is '${started_after:-(unreadable)}' after 'docker restart' returned and was '${started_before:-(unreadable)}' before it: the container did not restart, so there is no window this check may read and no line anywhere is evidence of a restart that did not happen"
    else
      since="$started_after"
      note "  the stamp handed to Docker, verbatim: --since '$since'"
      note "  (the restarted container's own StartedAt: RFC3339, UTC, with its Z -- not the clock,"
      note "   not this box's +02:00, and not a moment before the process writing these lines began)"
      sleep 10
      announced=$(docker logs --since "$since" ac-authserver 2>&1 | grep -a "Added realm" | tail -1)
      note "  the LAST 'Added realm' line written since $since:"
      note "    ${announced:-(none)}"
      if [ -n "$announced" ] && printf '%s' "$announced" | grep -qF "at ${REALM_ADDRESS}:${REALM_PORT}"; then
        verified ok "ac-authserver restarted (StartedAt moved to $since) and now announces the realm at ${REALM_ADDRESS}:${REALM_PORT}"
      else
        verified fail "ac-authserver's newest announcement since its own StartedAt $since is '${announced:-(none)}', not 'at ${REALM_ADDRESS}:${REALM_PORT}'"
      fi
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
  # Not `-s` alone: a probe that died after its first line is non-empty and says
  # nothing. The three headings below are the ones a reader needs the file for.
  if [ -s "$OUT/state-final.txt" ] \
     && grep -q '^=== state-final ===' "$OUT/state-final.txt" \
     && grep -q '^--- realmlist' "$OUT/state-final.txt" \
     && grep -q "^--- the owner's things ---" "$OUT/state-final.txt"; then
    verified ok "state-final.txt written, with its realmlist and owner's-things sections in it"
  else
    verified fail "state-final.txt is missing or truncated: no final reading of this box worth having"
  fi

  # --- the verdict ----------------------------------------------------------
  if [ "$PROBLEMS" -gt 0 ]; then
    note "=== RESTORATION FAILED: $PROBLEMS step(s) did not verify. The drivers' own status was $driver_rc. ==="
    say "T23: RESTORATION FAILED -- $PROBLEMS step(s); exiting $RESTORE_FAILED_STATUS"
    {
      echo "=== T3 run: RESTORATION FAILED -- $PROBLEMS step(s) did not verify ==="
      echo "the drivers' own status was $driver_rc; this script exits $RESTORE_FAILED_STATUS"
      echo "$(date -Is)"
    } >> "$OUT/run.log"
    exit "$RESTORE_FAILED_STATUS"
  fi
  note "=== every restoration verified; this script exits $driver_rc ==="
  if [ "$driver_rc" -ne 0 ]; then
    say "T23: this run FAILED (status $driver_rc); the box was put back and every step verified"
    echo "=== T3 run FAILED, status $driver_rc, box restored and verified $(date -Is) ===" >> "$OUT/run.log"
  else
    echo "=== T3 run finished $(date -Is) ===" >> "$OUT/run.log"
  fi
  exit "$driver_rc"
}

die() {  # die <status> <message>
  local rc="$1"; shift
  say "T23: $* -- stopping here"
  echo "STOPPED: $* (status $rc)" >> "$OUT/run.log"
  if [ "${T3_ALLOW_FAILURE:-0}" = "1" ]; then
    echo "(T3_ALLOW_FAILURE=1: a non-zero exit is what this invocation was for)" >> "$OUT/run.log"
  fi
  exit "$rc"
}

run_driver() {  # run_driver <path> <logname>  -- RETURNS the driver's status
  local d="$1" log="$OUT/$2"
  say "T23: running $(basename "$d") against the live WotLK server"
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

backup_the_firewall_or_stop() {
  # THE BACKUP RULE. Nothing that can press Apply starts until BOTH firewall
  # files have been copied aside, chowned so the driver can read them, and read
  # back byte for byte against the live files with `cmp`. Any of those three
  # failing stops the run here, with the reason, and the driver is never
  # invoked. An unbacked firewall is not a thing to press Apply against.
  #
  # Round 4 ran these three unchecked, and with no `set -e` a `cp` that failed
  # -- a full disk, a target that could not be written, sudo refusing -- reached
  # the driver anyway and pressed the button that the 7.1 lane's ufw lockout
  # came from. `cp` succeeding is also not the same claim as the copy being the
  # file: `cp -a` can return 0 on a short write in some filesystems' hands, and
  # a copy that is not the original restores the box to something that was never
  # on it. `cmp` is the question actually being asked, so `cmp` is what is run.
  local live copy pair
  for pair in "/etc/ufw/user.rules:$OUT/ufw-user.rules.before" \
              "/etc/ufw/user6.rules:$OUT/ufw-user6.rules.before"; do
    live=${pair%%:*}; copy=${pair#*:}
    sudo cp -a "$live" "$copy" \
      || die "$BACKUP_REFUSED_STATUS" "the firewall backup of $live could not be made (cp exited non-zero), so the driver that presses Apply is NOT started"
    sudo chown pk:pk "$copy" \
      || die "$BACKUP_REFUSED_STATUS" "the firewall backup of $live could not be chowned to pk (chown exited non-zero), so the driver that presses Apply is NOT started"
    sudo cmp -s "$live" "$copy" \
      || die "$BACKUP_REFUSED_STATUS" "the firewall backup of $live is not byte for byte the live file (cmp said so), so the driver that presses Apply is NOT started"
    echo "backed up       : $live -> $copy, cmp byte for byte OK, $(stat -c %s "$copy") bytes" >> "$OUT/run.log"
  done
  UFW4_BACKUP_BYTES=$(stat -c %s "$OUT/ufw-user.rules.before")
  UFW6_BACKUP_BYTES=$(stat -c %s "$OUT/ufw-user6.rules.before")
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
  echo "T23_PLANT_IN_GAP: ${T23_PLANT_IN_GAP:-0} (1 plants one matching 'Added realm' line in the gap the boundary rule closes)"
  echo "T23_DROP_BACKUP_AFTER_APPLY_PHASE: ${T23_DROP_BACKUP_AFTER_APPLY_PHASE:-0} (1 deletes both firewall copies once Apply could have been pressed)"
  echo "ac-authserver   : $(docker inspect -f '{{.State.Running}} StartedAt={{.State.StartedAt}}' ac-authserver 2>&1)"
  echo "ac-worldserver  : $(docker inspect -f '{{.State.Running}} Pid={{.State.Pid}} StartedAt={{.State.StartedAt}}' ac-worldserver 2>&1)"
  echo "realm row id 1  : $(mysql_q "SELECT CONCAT_WS('|', address, localAddress, localSubnetMask) FROM acore_auth.realmlist WHERE id=1;")"
} >> "$OUT/run.log"
trap restore_the_box EXIT

say "T23: capturing server state before anything is pressed"
probe before

# the folder the install clause insists is empty is shared with the 09-09 run
rm -rf "$LANE/p710-doomed-install"; mkdir -p "$LANE/p710-doomed-install"

# --- the falsify driver PRESSES Apply, so /etc/ufw is copied aside -----------
say "T23: copying /etc/ufw rules aside and checking the copies before the driver that presses Apply"
backup_the_firewall_or_stop

# Past this line something in this run can have pressed Apply, and the trap may
# no longer read a missing copy as "then nothing changed". It is set AFTER the
# backup because a run that stopped in the backup step never reached a driver.
APPLY_PHASE_ENTERED=1
echo "apply-capable   : entered, both firewall backups verified" >> "$OUT/run.log"
say "T23: firewall backed up and verified; entering the Apply-capable phase"

run_driver "$DRV/clause35_falsify.py" clause35-falsify.log
RC=$?
if [ "${T23_DROP_BACKUP_AFTER_APPLY_PHASE:-0}" = "1" ]; then
  rm -f "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before"
  echo "T23_DROP_BACKUP_AFTER_APPLY_PHASE=1: both firewall copies deleted, here, after the Apply-capable phase was entered" >> "$OUT/run.log"
fi
capture_ufw_effect        # the reading belongs to the driver whether it passed or not
[ "$RC" -eq 0 ] || die "$RC" "clause35_falsify.py exited $RC, so the falsification did not hold"

# --- the whole widget half, all 33 clauses -----------------------------------
run_driver "$DRV/widget_driver.py" widget-run.log
RC=$?
[ "$RC" -eq 0 ] || die "$RC" "widget_driver.py exited $RC, so a clause of the 33 failed"

probe after
say "T23: both drivers green; the EXIT trap now puts the box back and checks that it did"
