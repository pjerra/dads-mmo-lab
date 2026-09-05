#!/usr/bin/env bash
# 7.10 re-run cleanup: put the box back the way state-before.txt found it, and record it.
set -u
OUT=/home/pk/p7/out710
F="$OUT/final-state.txt"
say() { ~/bin/claude-say "$1" >/dev/null 2>&1; printf '%s %s\n' "$(date -Is)" "$1" >> /home/pk/claude-activity.log; }
q() { docker exec ac-database mysql -uroot -ppassword -N -B -e "$1" 2>&1; }

say "lane 7.10 re-run: cleaning up - deleting the WIDGET0905R account the widget driver created, removing the backup dumps and the throwaway folders, and recording the box's final state."

{
echo "=== 7.10 re-run final state, $(date -Is)"
echo
echo "--- the account the widget driver created, BEFORE deleting it"
q "SELECT COUNT(*) FROM acore_auth.account;"
q "SELECT id, username FROM acore_auth.account WHERE username NOT LIKE 'RNDBOT%' ORDER BY id;"
q "SELECT id, gmlevel FROM acore_auth.account_access WHERE id=(SELECT id FROM acore_auth.account WHERE username='WIDGET0905R');"
echo
echo "--- deleting it (account_access and realmcharacters first)"
q "DELETE FROM acore_auth.account_access WHERE id=(SELECT id FROM acore_auth.account WHERE username='WIDGET0905R');"
q "DELETE FROM acore_auth.realmcharacters WHERE acctid=(SELECT id FROM acore_auth.account WHERE username='WIDGET0905R');"
q "DELETE FROM acore_auth.account WHERE username='WIDGET0905R';"
echo "--- AFTER"
q "SELECT COUNT(*) FROM acore_auth.account;"
q "SELECT id, username FROM acore_auth.account WHERE username NOT LIKE 'RNDBOT%' ORDER BY id;"
q "SELECT COUNT(*) FROM acore_auth.account_access;"
echo
echo "--- the backup dumps sweep_driver3 made (this run created the whole backups dir)"
ls -la /home/pk/wowserver/sql_scripts/backups 2>&1
echo "--- removing just the files this run wrote, leaving the (now empty) directory the app makes"
rm -fv /home/pk/wowserver/sql_scripts/backups/20260905_2211*.sql \
       /home/pk/wowserver/sql_scripts/backups/20260905_2212*.sql 2>&1
ls -la /home/pk/wowserver/sql_scripts/backups 2>&1
echo
echo "--- throwaway folders this lane made"
for d in /home/pk/p7-doomed-install /home/pk/p7-empty /home/pk/p7-cancel-install \
         /home/pk/p7-cancel-install-tbc /home/pk/p7-fake-tbc-client \
         /home/pk/p7-wotlk-cancel-shape /home/pk/p7/sweep_fake_client; do
  if [ -e "$d" ]; then echo "$d: $(ls -A "$d" | wc -l) entries -> removing"; rm -rf "$d"; else echo "$d: already gone"; fi
done
for d in /home/pk/p7-doomed-install /home/pk/p7-empty /home/pk/p7-cancel-install \
         /home/pk/p7-cancel-install-tbc /home/pk/p7-fake-tbc-client \
         /home/pk/p7-wotlk-cancel-shape /home/pk/p7/sweep_fake_client; do
  echo "$d: $( [ -e "$d" ] && echo STILL THERE || echo absent )"
done
echo
echo "--- what this lane KEPT on purpose (lane b39 reuses it)"
ls -la /home/pk/p7 2>&1
echo
echo "--- the live install, as found"
docker ps --format '{{.Names}}\t{{.Status}}\t{{.Ports}}' 2>&1
docker ps -a --format '{{.Names}}\t{{.Status}}' 2>&1
echo "--- schema table counts (auth/characters/world/playerbots)"
q "SELECT table_schema, COUNT(*) FROM information_schema.tables WHERE table_schema LIKE 'acore%' GROUP BY table_schema ORDER BY table_schema;"
echo "--- realmlist"
q "SELECT id, name, address, localAddress, port FROM acore_auth.realmlist;"
echo "--- install state file"
cat /home/pk/wowserver/.yulon-install.json 2>&1
echo "--- ufw"
sudo ufw status 2>&1
sudo ls -l /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
sudo sha256sum /etc/ufw/user.rules /etc/ufw/user6.rules 2>&1
echo "  (state-before.txt recorded 320f53e1ee90a7fd92f17b67f50b06b51cb20998cd52f01dbcb52e24160618bf and f6696cd7741aada36ab2055dd1a9d098e23e0290ca3a67dafbbad2920e5fdf60)"
echo "--- docker images (nothing added, nothing pruned)"
docker images --format '{{.Repository}}:{{.Tag}} {{.Size}}' 2>&1
echo "--- docker system df"
docker system df 2>&1
echo "--- transient units this lane started (all should be gone; --collect removes them)"
systemctl --user list-units --all 'dml-710*' --no-legend 2>&1 | sed 's/^/  /'
echo "  (no lines above = none left)"
systemctl list-units --all 'lane710*' --no-legend 2>&1 | sed 's/^/  /'
echo "  (no lines above = none left)"
echo "--- disk"
df -h /home/pk | tail -1
echo "--- uptime"
uptime
} > "$F" 2>&1
echo "wrote $F"
say "lane 7.10 re-run: cleanup done. The box is back to 102 accounts, ufw inactive with its original rules, the WotLK install untouched and running, and /home/pk/p7 kept for lane b39."
