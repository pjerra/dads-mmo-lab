#!/usr/bin/env bash
# 7.10 cross-server regression sweep, RE-RUN on the merged engine (cfb4c04f), yulon-ubuntu.
# Same shape as pyplan/gates/7.10-ubuntu-2026-09-04/run-710.sh: the four 2026-08-28 sweep
# drivers with their path constants adjusted, plus the 7.10-gaps widget driver, against the
# WotLK server the 7.2 install left running at ~/wowserver.
#
# One thing this runner does that the 09-04 one did not: ufw on this box is INACTIVE, and
# sweep_driver4 calls network_apply('lan'). On 2026-09-04 that plan carried
# ('ufw','--force','enable') with no warning, which is the 7.1 lockout. So the plan was READ
# on this box before this runner was allowed near it (out710/ufw-plan-probe-interactive.txt
# and .../ufw-plan-probe-systemd-run.txt, both taken 2026-09-05): the merged engine emits
# ('ufw','allow','3724/tcp') and ('ufw','allow','8085/tcp') and NO enable, plus a warning
# saying so. `ufw allow` still writes into /etc/ufw/user.rules on an inactive firewall, so the
# rules files are copied aside before the driver and put back after it, and the sha256 of both
# is recorded either side.
set -u
OUT=/home/pk/p7/out710
mkdir -p "$OUT"
PY=/home/pk/p7/venv/bin/python
DRV=/home/pk/p7/drivers
export QT_QPA_PLATFORM=offscreen

say() { ~/bin/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }

probe() {  # probe <file-tag>
  local f="$OUT/state-$1.txt"
  {
    echo "=== state-$1 ==="
    echo "taken (local): $(date -Is)"
    echo "--- docker ps ---"
    docker ps --format '{{.Names}}\t{{.Status}}\t{{.Ports}}'
    echo "--- listening 3724/8085/3306 ---"
    ss -ltnp 2>/dev/null | grep -E ':(3724|8085|3306)\b' || echo "(none)"
    echo "--- schema table counts (auth/characters/world/playerbots) ---"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT table_schema, COUNT(*) FROM information_schema.tables WHERE table_schema LIKE 'acore%' GROUP BY table_schema ORDER BY table_schema;" 2>/dev/null || echo "(query failed)"
    echo "--- account rows (count, then the non-RNDBOT ones) ---"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT COUNT(*) FROM acore_auth.account;" 2>/dev/null || echo "(query failed)"
    docker exec ac-database mysql -uroot -ppassword -N -B -e \
      "SELECT id, username FROM acore_auth.account WHERE username NOT LIKE 'RNDBOT%' ORDER BY id;" 2>/dev/null || echo "(query failed)"
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
    echo "--- throwaway folders this lane uses ---"
    for d in /home/pk/p7-doomed-install /home/pk/p7-empty /home/pk/p7-cancel-install; do
      if [ -e "$d" ]; then echo "$d: $(ls -A "$d" | wc -l) entries"; else echo "$d: absent"; fi
    done
  } > "$f" 2>&1
  echo "wrote $f"
}

echo "=== 7.10 re-run sweep started $(date -Is) ===" > "$OUT/run.log"
{
  echo "code under test : /home/pk/p7/checkout at $(git -C /home/pk/p7/checkout rev-parse HEAD)"
  echo "interpreter     : $PY  ($($PY --version 2>&1))"
  echo "PySide6         : $($PY -c 'import PySide6; print(PySide6.__version__)' 2>&1)"
  echo "server dir      : /home/pk/wowserver"
  echo "drivers         : $DRV (2026-08-28 originals + the 7.10-gaps widget driver; see drivers.diff)"
  echo "QT_QPA_PLATFORM : $QT_QPA_PLATFORM"
} >> "$OUT/run.log"

say "lane 7.10 re-run: capturing server state before the sweep"
probe before

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

run_driver "$DRV/sweep_driver.py" sweep1.log
run_driver "$DRV/sweep_driver2.py" sweep2.log
run_driver "$DRV/sweep_driver3.py" sweep3.log
say "lane 7.10 re-run: restore driver finished; waiting 180s for the stack to come back"
sleep 180
probe after-restore

# --- ufw: copy the rules aside, run driver 4, put them back ---
say "lane 7.10 re-run: copying /etc/ufw rules aside before the networking driver, which adds allow rules"
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
{
  echo "--- ufw restored ---"
  sudo ufw status 2>&1
  sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before" 2>&1
} >> "$OUT/sweep4.log"

run_driver "$DRV/widget_driver.py" widget-run.log

say "lane 7.10 re-run: capturing server state after the sweep"
probe after
echo "=== 7.10 re-run sweep finished $(date -Is) ===" >> "$OUT/run.log"
say "lane 7.10 re-run: sweep half finished; see /home/pk/p7/out710/run.log"
