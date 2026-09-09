#!/usr/bin/env bash
# 7.10 cross-server regression sweep, RE-RUN on the merged Phase 8b tip (96a129dc),
# yulon-ubuntu2, 2026-09-08.
#
# Same shape as pyplan/gates/7.10-ubuntu-2026-09-05-rerun/run-710-rerun.sh, with four
# constants moved and three deviations that are named in this lane's README:
#
#   1. the box is yulon-ubuntu2, which REPLACES yulon-ubuntu (its host's D: SSD left the bus
#      on 2026-09-08); the checkout is ~/lane710b/checkout and the venv is the one the box's
#      own ~/dads-mmo-lab carries;
#   2. `sleep 180` after the restore driver is replaced by wait_ready.py, the install's own
#      `wait_server_ready()` -- the 2026-09-05 folder's README records that the blind sleep is
#      what let a console check pass on an empty reply;
#   3. `network_apply('lan')` rewrites `acore_auth.realmlist`, and on THIS box that row was
#      written by the 8.7a rebuild lane. The row is read out of the database before the
#      driver and put back after it if it moved, and both readings are in sweep4.log.
#
# The install-half cancel drivers of the 2026-09-05 run are NOT here: this box has 31.7 GiB
# free against preflight's 48 GiB refusal floor, so no install can be started on it at all.
# The README says so and cites rather than claims.
set -u
LANE=/home/pk/lane710b
OUT=$LANE/out
PY=/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python
DRV=$LANE/drivers
export QT_QPA_PLATFORM=offscreen
export PYTHONPATH=$LANE/checkout/pylauncher

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }

probe() {  # probe <file-tag>
  local f="$OUT/state-$1.txt"
  {
    echo "=== state-$1 ==="
    echo "taken (local): $(date -Is)   (UTC: $(date -u -Is))"
    echo "--- docker ps ---"
    docker ps --format '{{.Names}}\t{{.Status}}\t{{.Ports}}'
    echo "--- listening 3724/8085/3306 ---"
    ss -ltn 2>/dev/null | grep -E ':(3724|8085|3306)\b' || echo "(none)"
    echo "--- schema table counts (auth/characters/world/playerbots) ---"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT table_schema, COUNT(*) FROM information_schema.tables WHERE table_schema LIKE 'acore%' GROUP BY table_schema ORDER BY table_schema;" 2>/dev/null || echo "(query failed)"
    echo "--- account rows (count, then the non-RNDBOT ones) ---"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT COUNT(*) FROM acore_auth.account;" 2>/dev/null || echo "(query failed)"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT id, username FROM acore_auth.account WHERE username NOT LIKE 'RNDBOT%' ORDER BY id;" 2>/dev/null || echo "(query failed)"
    echo "--- account_access rows ---"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT id, gmlevel FROM acore_auth.account_access ORDER BY id;" 2>/dev/null || echo "(query failed)"
    echo "--- characters online ---"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT COUNT(*) FROM acore_characters.characters;" 2>/dev/null || echo "(query failed)"
    echo "--- realmlist ---"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT id, name, address, localAddress, port FROM acore_auth.realmlist;" 2>/dev/null || echo "(query failed)"
    echo "--- ufw ---"
    sudo ufw status numbered 2>&1
    echo "--- ufw rules sha256 ---"
    sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
    echo "--- disk ---"
    df -h /home/pk | tail -1
    echo "--- install state ---"
    cat /home/pk/wowserver/.yulon-install.json 2>/dev/null || echo "(no state file)"
    echo "--- backups this lane may create ---"
    if [ -d /home/pk/wowserver/sql_scripts/backups ]; then
      ls -1 /home/pk/wowserver/sql_scripts/backups | wc -l
    else
      echo "(no backups directory)"
    fi
    echo "--- throwaway folders this lane uses ---"
    for d in "$LANE/p710-doomed-install" "$LANE/p710-empty" "$LANE/sweep_fake_client"; do
      if [ -e "$d" ]; then echo "$d: $(ls -A "$d" | wc -l) entries"; else echo "$d: absent"; fi
    done
  } > "$f" 2>&1
  echo "wrote $f"
}

realm_address() {
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT address FROM acore_auth.realmlist WHERE id=1;" 2>/dev/null | tr -d '\r\n'
}

run_driver() {  # run_driver <path> <logname>
  local d="$1" log="$OUT/$2"
  say "lane 7.10 re-run: running $(basename "$d") against the live WotLK server"
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
}

mkdir -p "$OUT"
echo "=== 7.10 re-run sweep started $(date -Is) ===" > "$OUT/run.log"
{
  echo "code under test : $LANE/checkout at $(git -C "$LANE/checkout" rev-parse HEAD)"
  echo "interpreter     : $PY  ($($PY --version 2>&1))"
  echo "PySide6         : $($PY -c 'import PySide6; print(PySide6.__version__)' 2>&1)"
  echo "pydantic        : $($PY -c 'import pydantic; print(pydantic.VERSION)' 2>&1)"
  echo "yulon imported  : $($PY -c 'import pathlib, yulon; print(pathlib.Path(yulon.__file__).resolve())' 2>&1)"
  echo "box             : $(hostname)  $(uname -sr)"
  echo "docker          : $(docker --version)"
  echo "server dir      : /home/pk/wowserver"
  echo "drivers         : $DRV (the 2026-09-05 drivers; see drivers.diff for every change)"
  echo "QT_QPA_PLATFORM : $QT_QPA_PLATFORM"
} >> "$OUT/run.log"

say "lane 7.10 re-run: capturing server state before the sweep"
probe before

# --- the firewall/realm plan, READ before anything applies it ----------------
say "lane 7.10 re-run: reading network_plan('lan') before any driver may apply it"
"$PY" "$LANE/network_plan_probe.py" > "$OUT/network-plan-probe.txt" 2>&1
PROBE_RC=$?
echo "network_plan_probe exit $PROBE_RC" >> "$OUT/run.log"
if [ "$PROBE_RC" -ne 0 ]; then
  say "lane 7.10 re-run: REFUSING to run the networking driver -- the LAN plan carries an enable"
fi

REALM_BEFORE=$(realm_address)
echo "realmlist.address before the sweep: $REALM_BEFORE" >> "$OUT/run.log"

run_driver "$DRV/sweep_driver.py" sweep1.log
run_driver "$DRV/sweep_driver2.py" sweep2.log
run_driver "$DRV/sweep_driver3.py" sweep3.log

say "lane 7.10 re-run: restore driver finished; waiting for the install's OWN ready marker"
{
  echo "=== wait_ready.py, after sweep_driver3's controller.start() ==="
  echo "this replaces the 2026-09-05 runner's blind 'sleep 180'"
} > "$OUT/ready-after-restore.txt"
"$PY" "$LANE/checkout/pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/wait_ready.py" >> "$OUT/ready-after-restore.txt" 2>&1
echo "wait_ready after restore exit $?" >> "$OUT/run.log"
probe after-restore

# --- ufw: copy the rules aside, run driver 4, put them back ------------------
if [ "$PROBE_RC" -eq 0 ]; then
  say "lane 7.10 re-run: copying /etc/ufw rules aside before the networking driver"
  sudo cp -a /etc/ufw/user.rules "$OUT/ufw-user.rules.before"
  sudo cp -a /etc/ufw/user6.rules "$OUT/ufw-user6.rules.before"
  sudo chown pk:pk "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before"
  run_driver "$DRV/sweep_driver4.py" sweep4.log
  {
    echo "--- ufw after network_apply('lan') ---"
    sudo ufw status 2>&1
    sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
    echo "--- diff of user.rules against the copy taken before the driver ---"
    sudo diff "$OUT/ufw-user.rules.before" /etc/ufw/user.rules 2>&1 || true
  } >> "$OUT/sweep4.log"
  say "lane 7.10 re-run: putting /etc/ufw rules back exactly as they were"
  sudo cp -a "$OUT/ufw-user.rules.before" /etc/ufw/user.rules
  sudo cp -a "$OUT/ufw-user6.rules.before" /etc/ufw/user6.rules
  # 2026-09-08 finding 3a, fixed here rather than re-measured: `cp -a` preserves the COPY's
  # ownership, and the copy was chowned to pk so the driver could read it. Every sha256
  # check said "restored" and ufw itself was the only thing that noticed --
  # "WARN: uid is 0 but '/etc/ufw/user.rules' is owned by 1000". The mode is restated too,
  # because -a carried the copy's mode as well.
  sudo chown root:root /etc/ufw/user.rules /etc/ufw/user6.rules
  sudo chmod 640 /etc/ufw/user.rules /etc/ufw/user6.rules
  {
    echo "--- ownership after the restore (2026-09-08 finding 3a) ---"
    sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules
  } >> "$OUT/sweep4.log" 2>&1
  {
    echo "--- ufw restored ---"
    sudo ufw status 2>&1
    sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before" 2>&1
  } >> "$OUT/sweep4.log"

  REALM_AFTER=$(realm_address)
  {
    echo "--- the realm row, either side of network_apply('lan') ---"
    echo "before: $REALM_BEFORE"
    echo "after : $REALM_AFTER"
  } >> "$OUT/sweep4.log"
  if [ "$REALM_BEFORE" != "$REALM_AFTER" ]; then
    say "lane 7.10 re-run: network_apply moved the realm address; putting it back to $REALM_BEFORE"
    docker exec ac-database mysql -uroot -ppassword -e \
      "UPDATE acore_auth.realmlist SET address='$REALM_BEFORE', localAddress='$REALM_BEFORE' WHERE id=1;" >> "$OUT/sweep4.log" 2>&1
    {
      echo "put back to: $(realm_address)"
      echo "--- restarting ac-authserver so it re-reads the row it announces ---"
      docker restart ac-authserver
      sleep 10
      docker logs --tail 60 ac-authserver 2>&1 | grep -i "Added realm" \
        || echo "(no 'Added realm' line in the last 60 -- look at the whole log)"
      echo "(the 8.7a rebuild lane wrote the address this box advertises; a regression sweep"
      echo " may measure network_apply but may not leave another lane's server unreachable)"
    } >> "$OUT/sweep4.log"
  else
    echo "(unchanged -- nothing to put back)" >> "$OUT/sweep4.log"
  fi
else
  say "lane 7.10 re-run: SKIPPING sweep_driver4 -- the plan probe refused"
  echo "sweep_driver4.py SKIPPED (network_plan_probe refused)" >> "$OUT/run.log"
fi

run_driver "$DRV/widget_driver.py" widget-run.log

say "lane 7.10 re-run: capturing server state after the sweep"
probe after
echo "=== 7.10 re-run sweep finished $(date -Is) ===" >> "$OUT/run.log"
say "lane 7.10 re-run: sweep finished; see ~/lane710b/out/run.log"
