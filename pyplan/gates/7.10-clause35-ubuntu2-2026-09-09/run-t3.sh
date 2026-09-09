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
#      `apply.py` (new) and `log_panel.py` (+18/-2, the _StreamWorker guard) --
#      and the second of those is under the Stop clause this driver presses;
#   2. `clause35_falsify.py` runs FIRST and presses Apply, so /etc/ufw is copied
#      aside and put back around it exactly as the 09-09 runner does around
#      sweep_driver4.py, and ac-authserver is restarted after it;
#   3. the widget driver's two LANE constants are set from the environment --
#      a new account name (the "does not exist before the click" clause is only
#      a reading if the name is new) and a new shots directory (so the 09-09
#      run's frames are not overwritten).
set -u
LANE=/home/pk/lane710b
OUT=$LANE/out-t3
PY=/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python
DRV=$LANE/drivers-t3
export QT_QPA_PLATFORM=offscreen
export PYTHONPATH=$LANE/checkout/pylauncher
export WIDGET_ACCOUNT=WIDGET0909T3
export WIDGET_ACCOUNT_PW=widget0909t3pw
export WIDGET_SHOTS=$OUT/shots
export T3_SHOTS=$OUT/shots

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
    mysql_q "SELECT c.name, c.level FROM acore_characters.characters c JOIN acore_auth.account a ON a.id=c.account WHERE a.username='PERZI';"
    ls -l /home/pk/wowserver/lua_scripts/LootPet.lua 2>&1
    grep -n '^Logger.ALE' /home/pk/wowserver/etc/worldserver.conf 2>/dev/null || echo "(no Logger.ALE line)"
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

run_driver() {  # run_driver <path> <logname>
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

{
  echo "--- ufw after the Apply press ---"
  sudo ufw status 2>&1
  sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
  echo "--- diff of user.rules against the copy taken before the driver ---"
  sudo diff "$OUT/ufw-user.rules.before" /etc/ufw/user.rules 2>&1 || true
} >> "$OUT/clause35-falsify.log"
say "T3: putting /etc/ufw rules back exactly as they were"
sudo cp -a "$OUT/ufw-user.rules.before"  /etc/ufw/user.rules
sudo cp -a "$OUT/ufw-user6.rules.before" /etc/ufw/user6.rules
# 2026-09-08 finding 3a: `cp -a` carries the COPY's ownership, and the copy was
# chowned to pk so the driver could read it. Ownership and mode are restated.
sudo chown root:root /etc/ufw/user.rules /etc/ufw/user6.rules
sudo chmod 640 /etc/ufw/user.rules /etc/ufw/user6.rules
{
  echo "--- ufw restored ---"
  sudo ufw status 2>&1
  sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules \
    "$OUT/ufw-user.rules.before" "$OUT/ufw-user6.rules.before" 2>&1
  sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
  echo "--- the realm row after the driver put it back ---"
  mysql_q "SELECT address, localAddress, localSubnetMask FROM acore_auth.realmlist WHERE id=1;"
  # The driver restores the row itself (B6) and asserts it. This repeats the
  # UPDATE unconditionally anyway, because a driver that dies between B1 and B6
  # would leave the owner's box advertising 192.168.77.77 and no assertion in a
  # process that is gone can put it back.
  echo "--- the runner's own unconditional restore ---"
  mysql_q "UPDATE acore_auth.realmlist SET address='100.99.204.5', localAddress='100.99.204.5', localSubnetMask='255.255.255.0' WHERE id=1;"
  mysql_q "SELECT address, localAddress, localSubnetMask FROM acore_auth.realmlist WHERE id=1;"
  echo "--- restarting ac-authserver so it re-reads the row it announces ---"
  docker restart ac-authserver
  sleep 10
  docker logs --tail 60 ac-authserver 2>&1 | grep -i "Added realm" \
    || echo "(no 'Added realm' line in the last 60 -- look at the whole log)"
} >> "$OUT/clause35-falsify.log"

# --- the whole widget half, all 33 clauses -----------------------------------
run_driver "$DRV/widget_driver.py" widget-run.log

say "T3: removing the account this run created, and capturing state after"
{
  echo "=== the account this run created, and its removal ==="
  mysql_q "SELECT id, username FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';"
  mysql_q "DELETE aa FROM acore_auth.account_access aa JOIN acore_auth.account a ON a.id=aa.id WHERE a.username='$WIDGET_ACCOUNT';"
  mysql_q "DELETE FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';"
  echo "after the delete (expect no rows):"
  mysql_q "SELECT id, username FROM acore_auth.account WHERE username='$WIDGET_ACCOUNT';"
} > "$OUT/account-cleanup.txt" 2>&1

probe after
echo "=== T3 clause-35 run finished $(date -Is) ===" >> "$OUT/run.log"
say "T3: run finished; see ~/lane710b/out-t3/run.log"
