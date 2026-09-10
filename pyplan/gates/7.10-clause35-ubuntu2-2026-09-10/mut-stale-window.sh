#!/usr/bin/env bash
# MUTATION 1 -- the restart window.
#
# What has to be shown: a matching `Added realm` line planted in the gap between
# the precheck and `docker restart`, with the restarted authserver announcing
# nothing of its own, makes the run RED. Under round 4's rule the same line made
# it GREEN, and this script measures that too rather than asserting it.
#
# Two things are staged, and both are put back here:
#
#   the planted line   -- by the runner itself, under T23_PLANT_IN_GAP=1, at the
#                         one place in the script that is inside round 4's
#                         window and outside the container's StartedAt. It is
#                         marked `[T23-PLANTED]` in the container's own log so
#                         that a later reader of that log is not deceived by it,
#                         and it still matches what the check greps for
#                         (`at 100.99.204.5:8085`), which is the whole point.
#
#   the silence        -- `authserver.conf`'s `Logger.root` lowered from 4 (Info)
#                         to 3 (Warning) for the length of the run. "Added realm"
#                         is an Info line, so the restarted process writes none;
#                         the server itself is untouched and goes on serving the
#                         realm at the address the row holds. That is exactly
#                         Codex's case -- "if the restarted process announces
#                         nothing" -- staged without lying to the server about
#                         anything. The file is copied aside first, restored
#                         afterwards, and compared byte for byte with `cmp`,
#                         which is the same rule this ticket writes for ufw.
#
# Nothing here touches the realm row, the owner's account, or /etc/ufw.
set -u
CONF=/home/pk/wowserver/env/dist/etc/authserver.conf
OUT=/home/pk/lane710b/out-t23-mutwindow
BAK=/home/pk/lane710b/authserver.conf.t23-before
LOG=$OUT/mutation.log

mkdir -p "$OUT"
: > "$LOG"
note() { echo "$*" | tee -a "$LOG"; }

~/claude-say "T23 mutation 1: silencing ac-authserver's Info log for one run (authserver.conf copied aside first), planting one marked Added realm line in the restart gap, and putting the conf back byte for byte"

note "=== T23 mutation 1: the restart window ==="
note "taken (local): $(date -Is)   (UTC: $(date -u -Is))"
note ""
note "--- the conf, before ---"
cp -a "$CONF" "$BAK" || { note "could not copy the conf aside; refusing to touch it"; exit 70; }
cmp -s "$CONF" "$BAK" || { note "the copy is not the conf; refusing to touch it"; exit 70; }
sha256sum "$CONF" "$BAK" | tee -a "$LOG"
grep -n '^Logger.root' "$CONF" | tee -a "$LOG"

note ""
note "--- silencing: Logger.root 4 (Info) -> 3 (Warning) ---"
if ! grep -q '^Logger.root=4,Console Auth$' "$CONF"; then
  note "the line this mutation edits is not in the conf as expected; nothing changed"
  exit 70
fi
sed -i 's/^Logger.root=4,Console Auth$/Logger.root=3,Console Auth/' "$CONF"
grep -n '^Logger.root' "$CONF" | tee -a "$LOG"

note ""
note "--- the run, with T23_PLANT_IN_GAP=1 ---"
T3_OUT=$OUT \
T3_DRIVERS=/home/pk/lane710b/drivers-failclosed \
T3_ALLOW_FAILURE=1 T23_PLANT_IN_GAP=1 \
  bash /home/pk/lane710b/invoke-t3.sh >> "$LOG" 2>&1
note "the runner exited: $?"

note ""
note "--- putting the conf back ---"
cp -a "$BAK" "$CONF"
sha256sum "$CONF" "$BAK" | tee -a "$LOG"
if cmp -s "$CONF" "$BAK"; then note "authserver.conf is byte for byte what it was (cmp)"; else note "AUTHSERVER.CONF DOES NOT MATCH -- say so loudly"; fi
grep -n '^Logger.root' "$CONF" | tee -a "$LOG"


# --- the contrast, measured -------------------------------------------------
# Taken BEFORE the authserver is restarted with its voice back, because that
# restart writes a real announcement and would then be what round 4's window
# read. At this moment the container's log holds the planted line and nothing
# else since the run began, which is the state the two rules have to be compared
# in. The two stamps are the ones the run itself wrote into restore.log; neither
# was typed here.
CLOCK=$(grep -o -- "--since '[0-9TZ:-]*'" "$OUT/restore.log" | head -1 | sed "s/.*'\(.*\)'/\1/")
STARTED=$(grep -o -- "verbatim: --since '[^']*'" "$OUT/restore.log" | sed "s/.*'\(.*\)'/\1/")
{
  echo "=== the same log, read through the two windows ==="
  echo "taken (local): $(date -Is)"
  echo
  echo "round 4's window  (a stamp off the clock, taken before the restart): --since '$CLOCK'"
  echo "round 5's window  (the restarted container's own StartedAt)        : --since '$STARTED'"
  echo
  echo "--- what round 4's rule would have read: the LAST 'Added realm' line since $CLOCK ---"
  docker logs --since "$CLOCK" ac-authserver 2>&1 | grep -a "Added realm" | tail -1
  echo "--- what round 5's rule actually read: the LAST 'Added realm' line since $STARTED ---"
  docker logs --since "$STARTED" ac-authserver 2>&1 | grep -a "Added realm" | tail -1 || true
  echo "(a blank line above means there was none, which is the verdict restore.log recorded)"
  echo
  echo "--- every line the planted one sits among, for the record ---"
  docker logs --since "$CLOCK" ac-authserver 2>&1 | grep -a "Added realm"
} > "$OUT/contrast.txt" 2>&1
cat "$OUT/contrast.txt" >> "$LOG"

note ""
note "--- and the authserver, restarted with its voice back, bounded by its own StartedAt ---"
SB=$(docker inspect -f '{{.State.StartedAt}}' ac-authserver)
note "StartedAt before: $SB"
docker restart ac-authserver >> "$LOG" 2>&1
SA=$(docker inspect -f '{{.State.StartedAt}}' ac-authserver)
note "StartedAt after : $SA"
if [ "$SA" = "$SB" ]; then note "StartedAt DID NOT MOVE -- this reading is refused"; fi
sleep 12
note "the last 'Added realm' line since $SA:"
docker logs --since "$SA" ac-authserver 2>&1 | grep -a "Added realm" | tail -1 | tee -a "$LOG"
note "realm row id 1: $(MYSQL_PWD=password docker exec -e MYSQL_PWD ac-database mysql -uroot -N -B -e "SELECT CONCAT_WS('|', address, localAddress, localSubnetMask) FROM acore_auth.realmlist WHERE id=1;" 2>/dev/null)"
note "docker inspect: $(docker inspect -f '{{.Name}} Running={{.State.Running}} Pid={{.State.Pid}}' ac-worldserver) $(docker inspect -f '{{.Name}} Running={{.State.Running}} Pid={{.State.Pid}}' ac-authserver)"

~/claude-say "T23 mutation 1 done: authserver.conf restored byte for byte, ac-authserver announcing the realm again"
