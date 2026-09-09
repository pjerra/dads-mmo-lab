#!/usr/bin/env bash
# READ-ONLY. Why `tortoise-mangosd` crash-looped four times on the first start of this lane's
# Tortoise section, and whether that predates the lane.
#
# The worldserver's own last words name the statement and the table:
#     Making copy of character_inventory table.
#     SQL: TRUNCATE `character_inventory_copy`
#     [1146] Table 'tw_char.character_inventory_copy' doesn't exist
#     Your database structure is not up to date.
#     /src/src/shared/Database/DatabaseMysql.cpp:190: Error: Assertion in HandleMySQLError failed
#     ObjectMgr::BackupCharacterInventory()  <-  HonorMaintenancer::DoMaintenance()
#           <- World::SetInitialWorldSettings()  <- Master::Run()
#
# `HonorMaintenancer::DoMaintenance()` is reached from world start-up, so a missing table makes
# the world unstartable rather than making one feature fail. Whether it fires is a DATE
# question -- `saved_variables` holds the last and next maintenance day -- which is why this is
# worth asking rather than assuming.
#
# Every statement here is a SELECT or a SHOW. Nothing is created, dropped or written. The
# password goes in via MYSQL_PWD and is never printed.
set -u
OUT=$HOME/lane79b/out
DIR=$HOME/tortoise-server
mkdir -p "$OUT"

was_up=$(docker inspect tortoise-db --format '{{.State.Running}}' 2>/dev/null || echo unknown)
if [ "$was_up" != "true" ]; then docker start tortoise-db >/dev/null 2>&1; fi
PW=$(sed -n 's/^DB_ROOT_PASSWORD=//p' "$DIR/.env")
waited=0
until docker exec -e MYSQL_PWD="$PW" tortoise-db mariadb -u root -N -e "SELECT 1" >/dev/null 2>&1; do
  waited=$((waited + 2)); sleep 2; [ "$waited" -ge 180 ] && { echo "mariadb never answered"; exit 1; }
done

q() { docker exec -e MYSQL_PWD="$PW" tortoise-db mariadb -u root -e "$1" 2>&1 | tr -d '\r'; }
{
  echo "=== tortoise-crash-probe  $(date -Is)  (UTC $(date -u -Is)) ==="
  echo "tortoise-db was already running before this probe: $was_up"
  echo
  echo "--- does the table the worldserver truncates exist? ---"
  q "SHOW TABLES FROM tw_char LIKE 'character_inventory%'"
  echo
  echo "--- the row HonorMaintenancer reads to decide whether to run at all ---"
  q "SELECT lastHonorMaintenanceDay, nextHonorMaintenanceDay, honorMaintenanceMarker FROM tw_char.saved_variables"
  echo "today (server clock, days since epoch): $(( $(date -u +%s) / 86400 ))"
  echo
  echo "--- every tw_char table, so 'one table is missing' is a count and not an impression ---"
  q "SELECT COUNT(*) AS tw_char_tables FROM information_schema.tables WHERE table_schema='tw_char'"
  echo
  echo "--- was character_inventory_copy in the dump THIS LANE took at 05:56? ---"
  for f in "$DIR"/sql_scripts/backups/20260909_0556*tw_char*; do
    [ -e "$f" ] || { echo "(no 20260909_0556 tw_char dump on disk)"; break; }
    echo "$f: $(grep -c 'character_inventory_copy' "$f" 2>/dev/null) mentions of character_inventory_copy"
  done
  echo
  echo "--- the worldserver's crash, as the container recorded it ---"
  docker logs tortoise-mangosd 2>&1 | grep -nE "character_inventory_copy|not up to date|Assertion in HandleMySQLError|HonorMaintenancer|BackupCharacterInventory" | tail -12
  echo
  echo "--- how many times it died this way (whole container log) ---"
  echo "Assertion in HandleMySQLError: $(docker logs tortoise-mangosd 2>&1 | grep -c 'Assertion in HandleMySQLError')"
  echo "character_inventory_copy doesn't exist: $(docker logs tortoise-mangosd 2>&1 | grep -c "character_inventory_copy' doesn't exist")"
} >> "$OUT/tortoise-crash-probe.txt" 2>&1

if [ "$was_up" != "true" ]; then
  docker stop tortoise-db >/dev/null 2>&1
  echo "tortoise-db stopped again (it was not running when this probe started)" >> "$OUT/tortoise-crash-probe.txt"
fi
cat "$OUT/tortoise-crash-probe.txt"
