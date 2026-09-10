#!/usr/bin/env bash
# The invocation itself, committed.
#
# Round 3 wrote the `RUNNER EXIT STATUS` line by hand at an ssh prompt, so the
# 90 the README cited was traceable to no file anybody could read -- a number in
# a log with nothing behind it. It is produced here instead, by the same script
# for every run, so the line in each `run.log` has a source.
#
# The SIX invocations this folder's evidence comes from, in the order they were
# run (each `T3_OUT` is the folder the evidence was fetched from). The two
# mutations come first because a rule nobody has watched fail is a claim, and
# the green run comes last because it is the state the box is left in.
#
#   1. mut-backup-refused -- both firewall copies forced to fail, so the driver
#      that presses Apply must never start. `chattr +i` on both targets is what
#      forces it: root's own `cp` cannot write an immutable file, so this is the
#      "read-only target" the ticket names, with no wrapper between the script
#      and the `cp` it actually runs.
#        sudo chattr +i /home/pk/lane710b/out-t23-mutbackup/ufw-user{,6}.rules.before
#        T3_OUT=/home/pk/lane710b/out-t23-mutbackup \
#        T3_ALLOW_FAILURE=1 bash invoke-t3.sh
#
#   2. mut-backup-lost -- the copies made and verified, the Apply-capable phase
#      entered, and then both copies deleted: the trap must call that a
#      RESTORATION FAILURE and never `[RESTORED]`.
#        T3_OUT=/home/pk/lane710b/out-t23-mutbackuplost \
#        T3_DRIVERS=/home/pk/lane710b/drivers-failclosed \
#        T3_ALLOW_FAILURE=1 T23_DROP_BACKUP_AFTER_APPLY_PHASE=1 bash invoke-t3.sh
#
#   3. mut-stale-window -- a matching `Added realm` line planted in the gap
#      between the precheck and the restart, with the restarted authserver
#      silenced (`Logger.root` lowered to Warning in `authserver.conf` around
#      the run, by `mut-stale-window.sh`, restored byte for byte afterwards).
#      Round 4's clock window would have read the planted line and passed; the
#      boundary rule reads the container's own StartedAt, finds nothing, and the
#      run goes RED.
#        T3_OUT=/home/pk/lane710b/out-t23-mutwindow \
#        T3_DRIVERS=/home/pk/lane710b/drivers-failclosed \
#        T3_ALLOW_FAILURE=1 T23_PLANT_IN_GAP=1 bash invoke-t3.sh
#
#   4. failrestore -- a driver that fails AND a restoration that cannot succeed
#        T3_OUT=/home/pk/lane710b/out-t23-failrestore \
#        T3_DRIVERS=/home/pk/lane710b/drivers-failclosed \
#        T3_ALLOW_FAILURE=1 T3_REALM_ROW_ID=9999 bash invoke-t3.sh
#
#   5. failclosed -- a driver that fails, every restoration verifying
#        T3_OUT=/home/pk/lane710b/out-t23-failclosed \
#        T3_DRIVERS=/home/pk/lane710b/drivers-failclosed \
#        T3_ALLOW_FAILURE=1 bash invoke-t3.sh
#
#   6. green -- the real drivers
#        T3_OUT=/home/pk/lane710b/out-t23-green bash invoke-t3.sh
#
# It passes its environment through untouched and re-raises the runner's status,
# so putting it in front of `run-t3.sh` changes nothing except that the status
# is written down by a file rather than by a person.
set -u
OUT=${T3_OUT:-/home/pk/lane710b/out-t3}
bash /home/pk/lane710b/run-t3.sh
rc=$?
echo "RUNNER EXIT STATUS (as seen by the invoking shell): $rc" | tee -a "$OUT/run.log"
exit "$rc"
