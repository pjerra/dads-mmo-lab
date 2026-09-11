#!/usr/bin/env bash
# Put yulon-ubuntu2 back the way the 7.10 re-run found it, and write down what was actually
# removed rather than what was intended to be.
#
# What this run creates on the box: one account (the widget driver's), the dumps
# sweep_driver3.py takes plus the pre-restore safety dumps restore() takes, and three
# throwaway folders under ~/lane710. The ufw rules and the realm row are put back by the
# runner itself, at the moment they are touched, and this only re-reads them.
#
# ~/lane710 itself is KEPT: the checkout, the drivers and out/ are the evidence this folder
# is a copy of.
set -u
LANE=/home/pk/lane710
OUT=$LANE/out
F=$OUT/final-state.txt
ACCOUNT=WIDGET0908B

say() { ~/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }

say "lane 7.10 re-run: cleanup -- removing the account the widget driver created and this run's dumps"

{
  echo "=== final-state ==="
  echo "taken (local): $(date -Is)   (UTC: $(date -u -Is))"

  echo "--- the account the widget driver created, BEFORE the delete ---"
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT a.id, a.username, IFNULL(aa.gmlevel,-99) FROM acore_auth.account a
     LEFT JOIN acore_auth.account_access aa ON aa.id=a.id
     WHERE a.username='$ACCOUNT';"
  echo "--- deleting it (access row first, then the account) ---"
  docker exec ac-database mysql -uroot -ppassword -e \
    "DELETE aa FROM acore_auth.account_access aa
       JOIN acore_auth.account a ON a.id = aa.id
      WHERE a.username='$ACCOUNT';
     DELETE FROM acore_auth.account WHERE username='$ACCOUNT';"
  echo "--- and AFTER: this must print nothing at all ---"
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT id, username FROM acore_auth.account WHERE username='$ACCOUNT';"
  echo "--- account rows now (count, then the non-RNDBOT ones) ---"
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT COUNT(*) FROM acore_auth.account;"
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT id, username FROM acore_auth.account WHERE username NOT LIKE 'RNDBOT%' ORDER BY id;"
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT id, gmlevel FROM acore_auth.account_access ORDER BY id;"

  echo "--- the dumps this run wrote, listed before they go ---"
  if [ -d /home/pk/wowserver/sql_scripts/backups ]; then
    ls -l /home/pk/wowserver/sql_scripts/backups
    rm -f /home/pk/wowserver/sql_scripts/backups/*.sql
    echo "--- after the delete ---"
    ls -A /home/pk/wowserver/sql_scripts/backups | wc -l
    echo "(the now-empty directory is left in place: the app is what creates it)"
  else
    echo "(no backups directory -- nothing to remove)"
  fi

  echo "--- throwaway folders, listed then removed ---"
  for d in "$LANE/p710-doomed-install" "$LANE/p710-empty" "$LANE/sweep_fake_client"; do
    if [ -e "$d" ]; then
      echo "$d:"; ls -A "$d" | sed 's/^/    /'
      rm -rf "$d"
      echo "  removed: $([ -e "$d" ] && echo NO || echo yes)"
    else
      echo "$d: absent"
    fi
  done

  echo "--- realm row, as it stands after everything ---"
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT id, name, address, localAddress, port FROM acore_auth.realmlist;"
  echo "--- ufw ---"
  sudo ufw status 2>&1
  sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
  echo "--- containers ---"
  docker ps --format '{{.Names}}\t{{.Status}}'
  docker inspect --format '{{.Name}} exit={{.State.ExitCode}} restarts={{.RestartCount}} started={{.State.StartedAt}}' \
    ac-database ac-authserver ac-worldserver 2>&1
  echo "--- schema table counts ---"
  docker exec ac-database mysql -uroot -ppassword -N -B -e \
    "SELECT table_schema, COUNT(*) FROM information_schema.tables WHERE table_schema LIKE 'acore%' GROUP BY table_schema ORDER BY table_schema;"
  echo "--- install state ---"
  cat /home/pk/wowserver/.yulon-install.json
  echo "--- disk ---"
  df -h /home/pk | tail -1
  echo "--- kept on purpose ---"
  echo "$LANE (checkout, drivers, out/) -- the evidence"
} > "$F" 2>&1

echo "wrote $F"
say "lane 7.10 re-run: cleanup done; see ~/lane710/out/final-state.txt"
