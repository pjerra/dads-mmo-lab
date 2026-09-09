#!/usr/bin/env bash
# The LIVE Tortoise tw_world reimport on m910q (owner answer 1): tw_world from their base, the
# character index their character migration assumes, then the updater applies ALL 173 world and
# 2 character migrations at first start, IN ORDER.
#
# 2026-09-09, T4: this script's step 2 lost its migration half, and step 1 stopped moving a tag.
# It pre-applied `20260903063722_world.sql` before the updater ran, and the live press of
# 2026-09-09 01:06Z failed on exactly that (LIVE-ATTEMPT-2026-09-09.md). The audit's sentence:
#
#   "the live script pre-applies `20260903063722_world.sql` ahead of the other 172 migrations,
#    and twelve of that file's statements `UPDATE spell_template SET script_name …`, a column
#    the fork's base does not define — so it can never run first."
#
# The only reason for pre-applying it was the duplicate-key collision inside that file, and the
# image template fixes that at build time (`INSERT INTO` -> `INSERT IGNORE INTO`, 3a1ed6ee). So
# the image is rebuilt FIRST, through the app's own Rebuild, and step 1 now only proves that the
# image on the tag compose uses carries the rewrite -- it no longer tags a pre-built image over
# it, which would put the image WITHOUT the rewrite back.
#
# ROLLBACK, all in hand before anything moves: the image `rollback-2026-09-08` (= the pre-rebuild
# image, `ff4de2e90500`); fresh dumps of all four databases in ~/tortoise-backup-2026-09-09-T4
# (taken with the server stopped); and tortoise-rollback.sh next to this file -- which restores
# tw_char only in part, so a rollback also restores tw_char whole from those dumps (T4).
#
# The app's OWN `<ref>-rollback` tag is deliberately not on that list. `_keep_rollback` makes it
# before the compile and `_let_go` removes it after a rebuild that came up, so between presses it
# does not exist -- and the pre-flight below asked for it once and got
# "No such image: …native-58c6fd1c-rollback" on the very run this script's log records.
set -u
LOG="/c/Users/perzi/AppData/Local/Temp/claude/C--Users-perzi-dads-mmo-lab/e6db1900-f1cd-4099-995e-e1ef6bec1004/scratchpad/tortoise-reimport-live.log"
LIVE_TAG="yulon.local/cmangos-tortoise-server:native-58c6fd1c"
say() { echo "[$(date -u +%H:%M:%SZ)] $*" | tee -a "$LOG"; }
onbox() { ssh m910q "$1" 2>&1 | tee -a "$LOG"; }

say "=========== TORTOISE LIVE REIMPORT (m910q) ==========="
running=$(ssh m910q 'sudo docker ps --format "{{.Names}}" | grep -E "worldserver|mangosd|realmd|authserver" | tr "\n" " "')
[ -n "$running" ] && { say "REFUSING: a server is running ($running)"; exit 2; }
# The pre-rebuild image is asked for by its own name (see the header), and the dumps are this
# script's net: T4's four databases, not the three of 2026-09-08b.
onbox 'ls ~/tortoise-backup-2026-09-09-T4/tw_world.sql ~/tortoise-backup-2026-09-09-T4/tw_char.sql ~/tortoise-backup-2026-09-09-T4/tw_logon.sql ~/tortoise-backup-2026-09-09-T4/tw_logs.sql >/dev/null && sudo docker image inspect yulon.local/cmangos-tortoise-server:rollback-2026-09-08 --format "pre-rebuild rollback image: {{.Id}}"' || { say "REFUSING: rollback assets missing"; exit 2; }
onbox 'claude-say "Yulon: LIVE Tortoise reimport - tw_world from their base on the rebuilt image; rollback tags and fresh dumps in hand" >/dev/null 2>&1; echo announced'

say "--- 1. the image on the tag compose uses is the REBUILT one, and it carries the rewrite ---"
onbox "sudo docker images --format '{{.Repository}}:{{.Tag}} {{.ID}}' | grep tortoise; cd ~/tortoise-server && grep -n '^FROM' Dockerfile"
# Read through a bare `ssh`, not through `onbox`: `onbox` pipes into `tee`, so its exit status is
# tee's and nothing it runs can stop this script. This is the one check that MUST stop it -- an
# image without the rewrite is the 01:06Z failure again -- so the counts are read into a variable
# and compared here. `.` stands for the spaces on purpose: quoted patterns do not survive the trip
# through ssh, a remote shell and `sh -c`, which is exactly how the first version of this line
# printed `…20260903063722_world.sql:0` for both counts instead of 14 and 0.
counts=$(ssh m910q "sudo docker run --rm --entrypoint sh $LIVE_TAG -c 'F=/opt/tortoise/sql/database_updates/world/20260903063722_world.sql; echo \"\$(grep -c INSERT.IGNORE.INTO \$F) \$(grep -c ^INSERT.INTO \$F)\"'" | tr -d '\r')
say "in the image on $LIVE_TAG: INSERT IGNORE INTO / bare INSERT INTO = $counts (expect '14 0')"
[ "$counts" = "14 0" ] || { say "REFUSING: that image does not carry the INSERT IGNORE rewrite; rebuild it before reimporting"; exit 3; }

say "--- 2. db alone; tw_world from their base; the char index. NO migration is pre-applied ---"
onbox 'cd ~/tortoise-server && sudo docker compose up -d tortoise-db >/dev/null 2>&1 && for i in $(seq 1 40); do s=$(sudo docker inspect tortoise-db --format "{{.State.Health.Status}}" 2>/dev/null); [ "$s" = healthy ] && break; sleep 3; done; echo "db: $s"; PW=$(cat .db_password); M="sudo docker exec -i -e MYSQL_PWD=$PW tortoise-db mariadb -u root"; $M -e "DROP DATABASE IF EXISTS tw_world; CREATE DATABASE tw_world DEFAULT CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci; GRANT ALL ON tw_world.* TO '"'"'mangos'"'"'@'"'"'%'"'"'; FLUSH PRIVILEGES;" && echo "tw_world recreated"; cd src/tortoise-wow/sql/base; n=0; fail=0; for f in $(ls *.sql | sort); do if $M tw_world < "$f" 2>/tmp/imp.err; then n=$((n+1)); else fail=$((fail+1)); echo "FAILED base/$f: $(head -c 300 /tmp/imp.err)"; fi; done; echo "base: $n imported, $fail failed"; cd ../../modules/mod-playerbots/sql/world 2>/dev/null && for f in *.sql classic/*.sql; do [ -f "$f" ] || continue; $M tw_world < "$f" 2>/tmp/imp.err && echo "playerbots world: $f ok" || echo "playerbots world: $f WARN $(head -c 160 /tmp/imp.err)"; done; cd ~/tortoise-server; echo "no migration is pre-applied here: the updater gets all 173 in order (T4)"; echo "the char index below is applied UNCONDITIONALLY, and that only works while head 20260903211500 has not run: it DROPs this index and adds uq_owner_bot_event. On 2026-09-09 it had already run (the rebuild started the world once), so this line put idx_owner_bot_event back beside the unique one -- see the T4 README"; $M tw_char < src/tortoise-wow/sql/character_updates/20260708055500_ai_playerbot_random_bots_index.sql && echo "char index in place"; $M -N -e "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='"'"'tw_world'"'"'; SELECT COUNT(*) FROM tw_world.migrations; SELECT COUNT(*) FROM tw_char.characters; SELECT COUNT(*) FROM tw_logon.account" | tr "\n" " "; echo " (world tables, world migration rows, characters, accounts)"'

say "--- 3. the whole stack up on the rebuilt image; watching the updater apply all 173 in order ---"
onbox 'cd ~/tortoise-server && sudo docker compose up -d >/dev/null 2>&1; verdict=""; for i in $(seq 1 120); do sleep 10; L=$(sudo docker logs tortoise-mangosd 2>&1); if echo "$L" | grep -q "DB AutoUpdater FAILED\|cancelling server"; then verdict=FAILED; break; fi; if echo "$L" | grep -q -E "World server is up and running|World initialized|Ready to login"; then verdict=UP; break; fi; st=$(sudo docker inspect tortoise-mangosd --format "{{.State.Status}} {{.RestartCount}}"); case "$st" in exited*|dead*) verdict="EXITED ($st)"; break;; esac; done; echo "VERDICT: ${verdict:-TIMEOUT}"; sudo docker logs tortoise-mangosd 2>&1 | grep -E "Auto-Updater\] Found|failed to apply|AutoUpdater FAILED|World server is up and running|Loading time" | sed "s/, hash [0-9A-F]*//" | tail -14 | cut -c1-180; echo "update statements attempted: $(sudo docker logs tortoise-mangosd 2>&1 | grep -c "Attempting to execute update")"'

say "--- 4. what is running, and the proof it is the head (the four counts among them) ---"
onbox 'cd ~/tortoise-server && PW=$(cat .db_password); for q in "SELECT COUNT(*) FROM tw_world.migrations" "SELECT COUNT(*) FROM tw_char.migrations" "SELECT COUNT(*) FROM tw_char.characters" "SELECT COUNT(*) FROM tw_logon.account"; do sudo docker exec -e MYSQL_PWD=$PW tortoise-db mariadb -u root -N -e "$q"; done | tr "\n" " "; echo " (world migrations, char migrations, characters, accounts)"; ss -ltn | grep -E ":8090 |:3724 " | awk "{print \$4}" | tr "\n" " "; echo; sudo docker exec tortoise-mangosd sh -c "grep -c -a -E \"ns1__executeCommand|soap_serve\" /opt/tortoise/bin/mangosd; grep -m1 PRETTY /etc/os-release"; sudo docker ps --format "{{.Names}} {{.Status}} {{.Image}}" | grep tortoise'
say "DONE. If VERDICT was not UP, run tortoise-rollback.sh. The stack is LEFT UP for the report shots; stop it after."
