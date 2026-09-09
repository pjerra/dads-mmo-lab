#!/usr/bin/env bash
# The invocation itself, committed.
#
# Round 3 wrote the `RUNNER EXIT STATUS` line by hand at an ssh prompt, so the
# 90 the README cites was traceable to no file anybody could read -- a number in
# a log with nothing behind it. It is produced here instead, by the same script
# for all three runs, so the line in each `run.log` has a source.
#
# The three invocations this folder's evidence comes from, in the order they
# were run (each `T3_OUT` is the folder the evidence was fetched from):
#
#   failrestore -- a driver that fails AND a restoration that cannot succeed
#     T3_OUT=/home/pk/lane710b/out-t3-failrestore \
#     T3_DRIVERS=/home/pk/lane710b/drivers-failclosed \
#     T3_ALLOW_FAILURE=1 T3_REALM_ROW_ID=9999 bash invoke-t3.sh
#
#   failclosed -- a driver that fails, every restoration verifying
#     T3_OUT=/home/pk/lane710b/out-t3-failclosed \
#     T3_DRIVERS=/home/pk/lane710b/drivers-failclosed \
#     T3_ALLOW_FAILURE=1 bash invoke-t3.sh
#
#   green -- the real drivers
#     bash invoke-t3.sh
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
