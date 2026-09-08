#!/usr/bin/env bash
# The LIVE Tortoise tw_world reimport on m910q (owner answer 1), after the rehearsal on a copy
# came up: head image (24.04, SOAP, the lockout fix), tw_world from their base, the one migration
# whose own INSERTs collide (20260903063722_world) applied with INSERT IGNORE and recorded under
# the hash the updater computes, the character index their character migration assumes, then the
# updater applies the other 172 world and 2 character migrations at first start.
#
# ROLLBACK, all in hand before anything moves: image `rollback-2026-09-08` (= the running tag),
# dumps in ~/tortoise-backup-2026-09-08b (taken today with the server stopped, counts-before.txt),
# and tortoise-rollback.sh next to this file.
set -u
LOG="/c/Users/perzi/AppData/Local/Temp/claude/C--Users-perzi-dads-mmo-lab/e6db1900-f1cd-4099-995e-e1ef6bec1004/scratchpad/tortoise-reimport-live.log"
HEAD_TAG="yulon.local/cmangos-tortoise-server:head-2404-2026-09-08"
LIVE_TAG="yulon.local/cmangos-tortoise-server:native-58c6fd1c"
say() { echo "[$(date -u +%H:%M:%SZ)] $*" | tee -a "$LOG"; }
onbox() { ssh m910q "$1" 2>&1 | tee -a "$LOG"; }

say "=========== TORTOISE LIVE REIMPORT (m910q) ==========="
running=$(ssh m910q 'sudo docker ps --format "{{.Names}}" | grep -E "worldserver|mangosd|realmd|authserver" | tr "\n" " "')
[ -n "$running" ] && { say "REFUSING: a server is running ($running)"; exit 2; }
onbox 'ls ~/tortoise-backup-2026-09-08b/tw_world.sql ~/tortoise-backup-2026-09-08b/tw_char.sql >/dev/null && cat ~/tortoise-backup-2026-09-08b/counts-before.txt && sudo docker image inspect yulon.local/cmangos-tortoise-server:rollback-2026-09-08 --format "rollback image: {{.Id}}" && sudo docker image inspect '"$HEAD_TAG"' --format "head image: {{.Id}}"' || { say "REFUSING: rollback assets missing"; exit 2; }
onbox 'claude-say "Yulon: LIVE Tortoise reimport - head image onto the running tag, tw_world from their base; rollback tag and dumps in hand" >/dev/null 2>&1; echo announced'

say "--- 1. the head image takes the tag compose expects; the Dockerfile says 24.04 like the image ---"
onbox "sudo docker tag $HEAD_TAG $LIVE_TAG && sudo docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | grep tortoise; cd ~/tortoise-server && sed -i 's|^FROM ubuntu:22.04|FROM ubuntu:24.04|' Dockerfile && grep -n '^FROM' Dockerfile"

say "--- 2. db alone; tw_world from their base; the colliding migration with INSERT IGNORE; the char index ---"
onbox 'cd ~/tortoise-server && sudo docker compose up -d tortoise-db >/dev/null 2>&1 && for i in $(seq 1 40); do s=$(sudo docker inspect tortoise-db --format "{{.State.Health.Status}}" 2>/dev/null); [ "$s" = healthy ] && break; sleep 3; done; echo "db: $s"; PW=$(cat .db_password); M="sudo docker exec -i -e MYSQL_PWD=$PW tortoise-db mariadb -u root"; $M -e "DROP DATABASE IF EXISTS tw_world; CREATE DATABASE tw_world DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci; GRANT ALL ON tw_world.* TO '"'"'mangos'"'"'@'"'"'%'"'"'; FLUSH PRIVILEGES;" && echo "tw_world recreated"; cd src/tortoise-wow/sql/base; n=0; fail=0; for f in $(ls *.sql | sort); do if $M tw_world < "$f" 2>/tmp/imp.err; then n=$((n+1)); else fail=$((fail+1)); echo "FAILED base/$f: $(head -c 300 /tmp/imp.err)"; fi; done; echo "base: $n imported, $fail failed"; cd ../../modules/mod-playerbots/sql/world 2>/dev/null && for f in *.sql classic/*.sql; do [ -f "$f" ] || continue; $M tw_world < "$f" 2>/tmp/imp.err && echo "playerbots world: $f ok" || echo "playerbots world: $f WARN $(head -c 160 /tmp/imp.err)"; done; cd ~/tortoise-server; F=src/tortoise-wow/sql/database_updates/world/20260903063722_world.sql; H=$(sha1sum $F | cut -c1-40 | tr a-f A-F); sed "s/^\(\s*\)INSERT INTO/\1INSERT IGNORE INTO/" $F | $M tw_world && $M -e "INSERT INTO tw_world.migrations (Name, Module, Hash, AppliedAt) VALUES ('"'"'20260903063722_world'"'"', '"'"''"'"', '"'"'$H'"'"', NOW())" && echo "20260903063722_world applied with INSERT IGNORE and recorded as $H"; $M tw_char < src/tortoise-wow/sql/character_updates/20260708055500_ai_playerbot_random_bots_index.sql && echo "char index in place"; $M -N -e "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='"'"'tw_world'"'"'; SELECT COUNT(*) FROM tw_world.migrations; SELECT COUNT(*) FROM tw_char.characters; SELECT COUNT(*) FROM tw_logon.account" | tr "\n" " "; echo " (world tables, world migration rows, characters, accounts)"'

say "--- 3. the whole stack up on the head image; watching the updater ---"
onbox 'cd ~/tortoise-server && sudo docker compose up -d >/dev/null 2>&1; verdict=""; for i in $(seq 1 45); do sleep 10; L=$(sudo docker logs tortoise-mangosd 2>&1); if echo "$L" | grep -q "DB AutoUpdater FAILED\|cancelling server"; then verdict=FAILED; break; fi; if echo "$L" | grep -q -E "World server is up and running|World initialized|Ready to login"; then verdict=UP; break; fi; st=$(sudo docker inspect tortoise-mangosd --format "{{.State.Status}} {{.RestartCount}}"); case "$st" in exited*|dead*) verdict="EXITED ($st)"; break;; esac; done; echo "VERDICT: ${verdict:-TIMEOUT}"; sudo docker logs tortoise-mangosd 2>&1 | grep -E "Auto-Updater\] Found|failed to apply|AutoUpdater FAILED|up and running|Loading time" | sed "s/, hash [0-9A-F]*//" | tail -8 | cut -c1-160; sudo docker logs tortoise-mangosd 2>&1 | grep -c "Attempting to execute update"'

say "--- 4. what is running, and the proof it is the head ---"
onbox 'cd ~/tortoise-server && PW=$(cat .db_password); for q in "SELECT COUNT(*) FROM tw_world.migrations" "SELECT COUNT(*) FROM tw_char.migrations" "SELECT COUNT(*) FROM tw_char.characters" "SELECT COUNT(*) FROM tw_logon.account"; do sudo docker exec -e MYSQL_PWD=$PW tortoise-db mariadb -u root -N -e "$q"; done | tr "\n" " "; echo " (world migrations, char migrations, characters, accounts)"; ss -ltn | grep -E ":8090 |:3724 " | awk "{print \$4}" | tr "\n" " "; echo; sudo docker exec tortoise-mangosd sh -c "strings /opt/tortoise/bin/mangosd | grep -c -E \"ns1__executeCommand|soap_serve\"; grep -m1 PRETTY /etc/os-release"; sudo docker ps --format "{{.Names}} {{.Status}} {{.Image}}" | grep tortoise'
say "DONE. If VERDICT was not UP, run tortoise-rollback.sh. The stack is LEFT UP for the report shots; stop it after."
