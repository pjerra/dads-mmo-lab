#!/usr/bin/env bash
# T4 step 1 -- ground, then FRESH dumps of every Tortoise database with the server stopped.
# Runs ON m910q. The generated database password is read from the install's own file into a
# variable and handed to the client through MYSQL_PWD; it is never printed, so this script's
# output is committable.
set -u
cd ~/tortoise-server || exit 2
OUT=~/tortoise-backup-2026-09-09-T4
mkdir -p "$OUT"

echo "=== T4 ground $(date -u +%FT%TZ) ==="
running=$(sudo docker ps --format '{{.Names}}' | grep -E 'mangosd|realmd|worldserver|authserver' | tr '\n' ' ')
[ -n "$running" ] && { echo "REFUSING: a server is running ($running)"; exit 2; }
echo "--- docker ps (whole box) ---"
sudo docker ps --format '{{.Names}}\t{{.Status}}\t{{.Image}}'
echo "--- the three image ids this ticket names ---"
for t in native-58c6fd1c rollback-2026-09-08 head-2404-2026-09-08; do
  sudo docker image inspect "yulon.local/cmangos-tortoise-server:$t" --format "$t {{.Id}} {{.Created}}"
done
echo "--- disk ---"; df -h / | tail -1
echo "--- the dumps already in hand (2026-09-08b) ---"; ls -l ~/tortoise-backup-2026-09-08b/

echo "--- db alone ---"
sudo docker compose up -d tortoise-db >/dev/null 2>&1
for i in $(seq 1 40); do s=$(sudo docker inspect tortoise-db --format '{{.State.Health.Status}}' 2>/dev/null); [ "$s" = healthy ] && break; sleep 3; done
echo "db: ${s:-unknown}"
PW=$(cat .db_password)
M="sudo docker exec -i -e MYSQL_PWD=$PW tortoise-db mariadb -u root"
D="sudo docker exec -e MYSQL_PWD=$PW tortoise-db mariadb-dump -u root"

echo "--- schemas ---"
$M -N -e "SELECT schema_name FROM information_schema.schemata WHERE schema_name NOT IN ('mysql','information_schema','performance_schema','sys')" | tr '\n' ' '; echo

echo "--- the four counts BEFORE ---"
for q in "SELECT COUNT(*) FROM tw_char.characters" "SELECT COUNT(*) FROM tw_logon.account" "SELECT COUNT(*) FROM tw_world.migrations" "SELECT COUNT(*) FROM tw_char.migrations"; do
  printf '%s = ' "$q"; $M -N -e "$q"
done

echo "--- the state the rebuild would start the world against ---"
$M -N -e "SELECT CONCAT('tw_char.migrations rows: ', IFNULL(GROUP_CONCAT(Name),'(none)')) FROM tw_char.migrations"
$M -N -e "SELECT CONCAT('idx_owner_bot_event: ', COUNT(*)) FROM information_schema.statistics WHERE table_schema='tw_char' AND table_name='ai_playerbot_random_bots' AND index_name='idx_owner_bot_event'"
$M -N -e "SELECT CONCAT('uq_owner_bot_event: ', COUNT(*)) FROM information_schema.statistics WHERE table_schema='tw_char' AND table_name='ai_playerbot_random_bots' AND index_name='uq_owner_bot_event'"
$M -N -e "SELECT CONCAT('tw_world.spell_template.script_name: ', COUNT(*)) FROM information_schema.columns WHERE table_schema='tw_world' AND table_name='spell_template' AND column_name='script_name'"
$M -N -e "SELECT CONCAT('tw_char.character_inventory_copy: ', COUNT(*)) FROM information_schema.tables WHERE table_schema='tw_char' AND table_name='character_inventory_copy'"
$M -N -e "SELECT CONCAT('tw_char.character_pvp_currency: ', COUNT(*)) FROM information_schema.tables WHERE table_schema='tw_char' AND table_name='character_pvp_currency'"
$M -N -e "SELECT CONCAT('tw_char tables: ', COUNT(*)) FROM information_schema.tables WHERE table_schema='tw_char'"

echo "--- FRESH dumps into $OUT ---"
for db in tw_char tw_logon tw_world; do
  $D --single-transaction --routines --events --add-drop-table --databases "$db" > "$OUT/$db.sql" 2>"$OUT/$db.err"
  rc=$?
  echo "$db: rc=$rc bytes=$(stat -c%s "$OUT/$db.sql") err=$(head -c 200 "$OUT/$db.err")"
done
echo "--- row-counted dumps of the four counted tables (one INSERT per row, so the rows can be counted IN the dump) ---"
$D --skip-extended-insert --no-create-info tw_char characters   > "$OUT/rows-tw_char.characters.sql"
$D --skip-extended-insert --no-create-info tw_logon account     > "$OUT/rows-tw_logon.account.sql"
$D --skip-extended-insert --no-create-info tw_world migrations  > "$OUT/rows-tw_world.migrations.sql"
$D --skip-extended-insert --no-create-info tw_char migrations   > "$OUT/rows-tw_char.migrations.sql"
for f in "$OUT"/rows-*.sql; do echo "$(basename "$f"): $(grep -c '^INSERT INTO' "$f") INSERT rows"; done
echo "--- what the whole-database dumps hold (including the two tables last night's rollback lost) ---"
for t in characters character_pvp_currency character_inventory_copy; do
  printf 'tw_char.sql CREATE %s: ' "$t"; grep -c "CREATE TABLE \`$t\`" "$OUT/tw_char.sql"
done
echo "--- sizes + sha256 ---"
ls -l "$OUT"; sha256sum "$OUT"/*.sql | sed 's|/home/pk|~|'
echo "--- db down again ---"
sudo docker compose stop >/dev/null 2>&1; echo "still running: $(sudo docker ps --format '{{.Names}}' | tr '\n' ' ')"
echo "=== T4 ground done $(date -u +%FT%TZ) ==="
