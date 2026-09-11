#!/usr/bin/env bash
# Tortoise tw_world reimport, REHEARSED on a copy of the live volume on m910q (owner answer 1).
#
# WHAT THIS PROVES: that the fork's head binary (the 24.04 image built 2026-09-08 04:21Z,
# still on the box untagged as 656b8e4949f8) starts cleanly on a world database reimported
# from `sql/base/*.sql` at their head, with OUR characters and accounts beside it -- the
# updater applies every world migration to the fresh base (hash-keyed, all 173 of them) and
# the two character migrations to the copied tw_char, and the world reports up.
#
# WHAT IT TOUCHES: a NEW volume (a byte copy of the live one), a NEW compose project in
# ~/tortoise-rehearsal with its own container names, ports on loopback only, and the live
# ./data folder mounted READ-ONLY. The live volume, the live project, its image tag and its
# etc/ are not written. The live db container is started ALONE for a fresh dump first (the
# 05:40 dumps predate the 8.4d/8.5d gates), then stopped again.
#
# WAITS FIRST: m910q is one-server-at-a-time (owner rule) and a workflow lane may hold it.
# This waits for the re-press workflow's 8.4b lane to return a RESULT before touching the box.
set -u

JOURNAL="/c/Users/perzi/.claude/projects/C--Users-perzi-dads-mmo-lab/e6db1900-f1cd-4099-995e-e1ef6bec1004/subagents/workflows/wf_c667c9db-593/journal.jsonl"
LOG="/c/Users/perzi/AppData/Local/Temp/claude/C--Users-perzi-dads-mmo-lab/e6db1900-f1cd-4099-995e-e1ef6bec1004/scratchpad/tortoise-rehearsal.log"
HEAD_IMAGE_ID="656b8e4949f8"
HEAD_TAG="yulon.local/cmangos-tortoise-server:head-2404-2026-09-08"
say() { echo "[$(date -u +%H:%M:%SZ)] $*" | tee -a "$LOG"; }
onbox() { ssh m910q "$1" 2>&1 | tee -a "$LOG"; }

say "=========== TORTOISE REIMPORT REHEARSAL (copied volume, m910q) ==========="

# --- 0. wait for m910q to be free of the workflow's lane -------------------------------
if [ "${SKIP_WAIT:-0}" != "1" ]; then
  say "waiting for the 8.4b lane to return (m910q lock)"
  for i in $(seq 1 180); do
    if grep '"type":"result"' "$JOURNAL" 2>/dev/null | grep -q '8\.4b'; then say "8.4b returned"; break; fi
    if ! grep -q '8\.4b' "$JOURNAL" 2>/dev/null && [ "$i" -gt 5 ]; then :; fi
    sleep 60
  done
fi
running=$(ssh m910q 'sudo docker ps --format "{{.Names}}" | grep -E "worldserver|mangosd|realmd|authserver" | tr "\n" " "')
if [ -n "$running" ]; then
  say "REFUSING: a server is running on m910q ($running); one server at a time. Nothing done."
  exit 2
fi
onbox 'claude-say "Yulon: Tortoise reimport REHEARSAL on a COPY of the db volume - live install untouched" >/dev/null 2>&1; echo announced'

# --- 1. the head image gets a name ------------------------------------------------------
say "--- 1. tag the surviving 24.04 head image ---"
onbox "sudo docker image inspect $HEAD_IMAGE_ID --format '{{.Id}} {{.Created}} {{.Size}}' && sudo docker tag $HEAD_IMAGE_ID $HEAD_TAG && sudo docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | grep tortoise"
onbox "sudo docker run --rm --entrypoint sh $HEAD_TAG -c 'strings /opt/tortoise/bin/mangosd | grep -c -E \"ns1__executeCommand|soap_serve\"; grep -m1 PRETTY /etc/os-release'"

# --- 2. fresh dumps of the LIVE databases, db alone, then stop --------------------------
say "--- 2. fresh dumps (db container alone) ---"
onbox 'cd ~/tortoise-server && mkdir -p ~/tortoise-backup-2026-09-08b && sudo docker compose up -d tortoise-db >/dev/null 2>&1 && for i in $(seq 1 40); do s=$(sudo docker inspect tortoise-db --format "{{.State.Health.Status}}" 2>/dev/null); [ "$s" = healthy ] && break; sleep 3; done; echo "db: $s"; PW=$(cat .db_password); for d in tw_world tw_char tw_logon; do sudo docker exec -e MYSQL_PWD="$PW" tortoise-db mariadb-dump -u root --single-transaction "$d" > ~/tortoise-backup-2026-09-08b/$d.sql; done; for q in "SELECT COUNT(*) FROM tw_char.characters" "SELECT COUNT(*) FROM tw_logon.account" "SELECT COUNT(*) FROM tw_world.migrations" "SELECT COUNT(*) FROM tw_char.migrations"; do sudo docker exec -e MYSQL_PWD="$PW" tortoise-db mariadb -u root -N -e "$q"; done | tr "\n" " " | tee ~/tortoise-backup-2026-09-08b/counts-before.txt; echo; sudo docker compose stop tortoise-db >/dev/null 2>&1; ls -la ~/tortoise-backup-2026-09-08b'

# --- 3. copy the live volume ------------------------------------------------------------
say "--- 3. byte-copy the live volume to the rehearsal volume ---"
onbox 'sudo docker volume rm -f tortoise-rehearsal_db-data >/dev/null 2>&1; sudo docker volume create tortoise-rehearsal_db-data >/dev/null && sudo docker run --rm -v yulon-wow-tortoise-58c6fd1c_db-data:/from:ro -v tortoise-rehearsal_db-data:/to alpine sh -c "cp -a /from/. /to/ && du -sh /to"'

# --- 4. the throwaway project -----------------------------------------------------------
say "--- 4. write ~/tortoise-rehearsal (own names, loopback ports, data read-only) ---"
onbox 'rm -rf ~/tortoise-rehearsal && mkdir -p ~/tortoise-rehearsal && cp -r ~/tortoise-server/etc ~/tortoise-rehearsal/etc && cp ~/tortoise-server/.env ~/tortoise-rehearsal/.env && cd ~/tortoise-rehearsal && sed -i "s/tortoise-db;3306/rehearsal-db;3306/g" etc/mangosd.conf etc/realmd.conf && grep -n "Database.Info\|DatabaseInfo" etc/mangosd.conf etc/realmd.conf | sed "s/;[^;]*;\([^;]*\);/;***;\1;/" | head -5 && cat > docker-compose.yml <<EOF
services:
  rehearsal-db:
    container_name: rehearsal-db
    image: mariadb:10.6
    networks: [rehearsal-net]
    environment:
      MARIADB_ROOT_PASSWORD: \${DB_ROOT_PASSWORD}
    volumes:
      - tortoise-rehearsal_db-data:/var/lib/mysql
    healthcheck:
      test: ["CMD", "healthcheck.sh", "--connect", "--innodb_initialized"]
      interval: 5s
      timeout: 10s
      retries: 40
      start_period: 60s
  rehearsal-mangosd:
    container_name: rehearsal-mangosd
    image: '"$HEAD_TAG"'
    networks: [rehearsal-net]
    working_dir: /opt/tortoise/bin
    command: ["./mangosd"]
    stdin_open: true
    tty: true
    volumes:
      - ./etc:/opt/tortoise/etc
      - /home/pk/tortoise-server/data:/opt/tortoise/data:ro
    depends_on:
      rehearsal-db:
        condition: service_healthy
networks:
  rehearsal-net: {}
volumes:
  tortoise-rehearsal_db-data:
    external: true
EOF
sudo docker compose config --quiet && echo "compose ok"'

# --- 5. reimport tw_world from their base, the way the installer does ---------------------
say "--- 5. db up; drop tw_world; import sql/base/*.sql (sorted by name); playerbots world SQL (warn) ---"
onbox 'cd ~/tortoise-rehearsal && sudo docker compose up -d rehearsal-db >/dev/null 2>&1 && for i in $(seq 1 40); do s=$(sudo docker inspect rehearsal-db --format "{{.State.Health.Status}}" 2>/dev/null); [ "$s" = healthy ] && break; sleep 3; done; echo "rehearsal-db: $s"; PW=$(cat .env | sed -n "s/^DB_ROOT_PASSWORD=//p"); M="sudo docker exec -i -e MYSQL_PWD=$PW rehearsal-db mariadb -u root"; $M -e "DROP DATABASE IF EXISTS tw_world; CREATE DATABASE tw_world DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci; GRANT ALL ON tw_world.* TO '"'"'mangos'"'"'@'"'"'%'"'"'; FLUSH PRIVILEGES;" && echo "tw_world recreated"; cd ~/tortoise-server/src/tortoise-wow/sql/base; n=0; fail=0; for f in $(ls *.sql | sort); do if $M tw_world < "$f" 2>/tmp/imp.err; then n=$((n+1)); else fail=$((fail+1)); echo "FAILED base/$f: $(head -c 300 /tmp/imp.err)"; fi; done; echo "base: $n imported, $fail failed"; cd ~/tortoise-server/src/tortoise-wow/modules/mod-playerbots/sql/world 2>/dev/null && for f in *.sql classic/*.sql; do [ -f "$f" ] || continue; $M tw_world < "$f" 2>/tmp/imp.err && echo "playerbots world: $f ok" || echo "playerbots world: $f WARN $(head -c 200 /tmp/imp.err)"; done; $M -N -e "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='"'"'tw_world'"'"'; SELECT COUNT(*) FROM tw_world.migrations;" | tr "\n" " "; echo " (tables, migrations rows -- expect >=150 and 0)"'

# --- 6. the one manual character update the auto-migration assumes ------------------------
say "--- 6. tw_char: idx_owner_bot_event (their character_updates/20260708055500, ADD INDEX IF NOT EXISTS) ---"
onbox 'cd ~/tortoise-rehearsal && PW=$(sed -n "s/^DB_ROOT_PASSWORD=//p" .env); sudo docker exec -i -e MYSQL_PWD=$PW rehearsal-db mariadb -u root tw_char < ~/tortoise-server/src/tortoise-wow/sql/character_updates/20260708055500_ai_playerbot_random_bots_index.sql && sudo docker exec -e MYSQL_PWD=$PW rehearsal-db mariadb -u root -N -e "SHOW INDEX FROM tw_char.ai_playerbot_random_bots WHERE Key_name LIKE '"'"'%owner_bot_event%'"'"'" | awk "{print \$3}" | sort -u'

# --- 7. start the head binary and watch its updater ----------------------------------------
say "--- 7. mangosd (head, 24.04) up; watching the updater for up to 25 min ---"
onbox 'cd ~/tortoise-rehearsal && sudo docker compose up -d rehearsal-mangosd >/dev/null 2>&1; verdict=""; for i in $(seq 1 150); do sleep 10; L=$(sudo docker logs rehearsal-mangosd 2>&1); if echo "$L" | grep -q "DB AutoUpdater FAILED\|cancelling server"; then verdict=FAILED; break; fi; if echo "$L" | grep -q -E "World server is up and running|World initialized|started up successfully|Ready to login"; then verdict=UP; break; fi; st=$(sudo docker inspect rehearsal-mangosd --format "{{.State.Status}} {{.RestartCount}}"); case "$st" in exited*|dead*) verdict="EXITED ($st)"; break;; esac; done; echo "VERDICT: ${verdict:-TIMEOUT}"; echo "--- updater lines ---"; sudo docker logs rehearsal-mangosd 2>&1 | grep -E "Auto-Updater|AutoUpdater|Attempting to execute|SUCCESS|FAIL|Executed update" | sed "s/, hash [0-9A-F]*//" | tail -40; echo "--- counts ---"; sudo docker logs rehearsal-mangosd 2>&1 | grep -c "Executed update\|\[SUCCESS\]"; sudo docker logs rehearsal-mangosd 2>&1 | grep -c "Failed to execute\|\[FAIL\]"; echo "--- last 25 lines ---"; sudo docker logs rehearsal-mangosd 2>&1 | tail -25 | cut -c1-220'

# --- 8. what survived ---------------------------------------------------------------------
say "--- 8. rows after, in the rehearsal copy ---"
onbox 'cd ~/tortoise-rehearsal && PW=$(sed -n "s/^DB_ROOT_PASSWORD=//p" .env); for q in "SELECT COUNT(*) FROM tw_char.characters" "SELECT COUNT(*) FROM tw_logon.account" "SELECT COUNT(*) FROM tw_world.migrations" "SELECT COUNT(*) FROM tw_char.migrations" "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='"'"'tw_world'"'"'"; do sudo docker exec -e MYSQL_PWD=$PW rehearsal-db mariadb -u root -N -e "$q"; done | tr "\n" " "; echo " (characters, accounts, world migrations, char migrations, world tables)"; echo "before: $(cat ~/tortoise-backup-2026-09-08b/counts-before.txt)"'

# --- 9. stop the rehearsal; keep its volume for a look; nothing live was started -----------
say "--- 9. stop rehearsal containers (volume kept: tortoise-rehearsal_db-data) ---"
onbox 'cd ~/tortoise-rehearsal && sudo docker compose stop >/dev/null 2>&1; sudo docker ps --format "{{.Names}} {{.Status}}" | grep -E "tortoise|rehearsal" || echo "nothing of ours running"; df -h / | tail -1'
onbox 'claude-say "Yulon: Tortoise rehearsal finished; rehearsal containers stopped; live install untouched" >/dev/null 2>&1; echo announced'
say "DONE. Read $LOG for VERDICT."
