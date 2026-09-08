#!/usr/bin/env bash
# Continue the rehearsal on the COPY: each time the head updater cancels on a migration whose
# rows are already in their base, record the migration as applied (name + the hash the log
# printed) and start again. Counts how many of the 173 collide with the base they ship, and
# whether the world comes up once they are skipped. Live install untouched; rehearsal only.
set -u
LOG="/c/Users/perzi/AppData/Local/Temp/claude/C--Users-perzi-dads-mmo-lab/e6db1900-f1cd-4099-995e-e1ef6bec1004/scratchpad/tortoise-rehearsal-count.log"
say() { echo "[$(date -u +%H:%M:%SZ)] $*" | tee -a "$LOG"; }
say "=========== rehearsal, part 2: which head migrations collide with the head base ==========="
ssh m910q 'claude-say "Yulon: Tortoise rehearsal part 2 on the COPY - marking base-duplicate migrations applied one at a time" >/dev/null 2>&1'
for round in $(seq 1 12); do
  out=$(ssh m910q 'cd ~/tortoise-rehearsal && PW=$(sed -n "s/^DB_ROOT_PASSWORD=//p" .env); L=$(sudo docker logs rehearsal-mangosd 2>&1); fail=$(echo "$L" | grep -o "Migration [0-9_a-z]* with hash [0-9A-F]* failed to apply" | tail -1); if [ -z "$fail" ]; then echo "NOFAIL"; exit 0; fi; name=$(echo "$fail" | awk "{print \$2}"); hash=$(echo "$fail" | awk "{print \$5}"); err=$(echo "$L" | grep -E "^\[[0-9]+\] " | tail -1); echo "SKIP $name $hash :: $err"; sudo docker exec -e MYSQL_PWD=$PW rehearsal-db mariadb -u root -e "INSERT INTO tw_world.migrations (Name, Module, Hash, AppliedAt) VALUES (\"$name\", \"\", \"$hash\", NOW())"; sudo docker compose up -d --force-recreate rehearsal-mangosd >/dev/null 2>&1; verdict=""; for i in $(seq 1 50); do sleep 10; L=$(sudo docker logs rehearsal-mangosd 2>&1); if echo "$L" | grep -q "DB AutoUpdater FAILED"; then verdict=FAILED; break; fi; if echo "$L" | grep -q -E "World server is up and running|World initialized|started up successfully|Ready to login"; then verdict=UP; break; fi; st=$(sudo docker inspect rehearsal-mangosd --format "{{.State.Status}}"); [ "$st" = exited ] && { verdict=EXITED; break; }; done; echo "VERDICT ${verdict:-TIMEOUT}"; echo "$L" | grep -c "Attempting to execute update"')
  say "round $round: $(echo "$out" | tr '\n' ' ' | cut -c1-400)"
  case "$out" in *NOFAIL*|*"VERDICT UP"*|*"VERDICT TIMEOUT"*|*"VERDICT EXITED"*) break;; esac
done
say "--- after ---"
ssh m910q 'cd ~/tortoise-rehearsal && PW=$(sed -n "s/^DB_ROOT_PASSWORD=//p" .env); for q in "SELECT COUNT(*) FROM tw_world.migrations" "SELECT COUNT(*) FROM tw_char.characters" "SELECT COUNT(*) FROM tw_char.migrations"; do sudo docker exec -e MYSQL_PWD=$PW rehearsal-db mariadb -u root -N -e "$q"; done | tr "\n" " "; echo; sudo docker logs rehearsal-mangosd 2>&1 | grep -E "Auto-Updater\] (Found|Attempting to execute update 2026090[3-6])|failed to apply|up and running|World initialized|Ready to login" | sed "s/, hash [0-9A-F]*//" | tail -25 | cut -c1-160; ss -ltn 2>/dev/null | grep -E ":8090|:3724" ; sudo docker logs rehearsal-mangosd 2>&1 | tail -8 | cut -c1-160; sudo docker compose stop >/dev/null 2>&1; echo stopped' 2>&1 | tee -a "$LOG"
ssh m910q 'claude-say "Yulon: Tortoise rehearsal part 2 done; rehearsal stopped; live install untouched" >/dev/null 2>&1'
say DONE
