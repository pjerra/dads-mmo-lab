#!/usr/bin/env bash
# T4's rollback: `tortoise-rollback.sh` AND the half it does not do.
#
# Last night's rollback restored `tw_world` whole and `tw_char` only in part -- it undoes the two
# character-side changes by hand, and one of those hands is `DROP TABLE IF EXISTS
# tw_char.character_pvp_currency`, which is how that table went missing and had to be restored
# from a dump the next morning. So: their script first (image tag, tw_world, the index, the
# migration row), then `tw_char` restored WHOLE from the fresh T4 dump, then the four counts and
# the two tables that went missing, read back rather than assumed.
#
# Runs from the laptop like its neighbour; every action is on m910q.
set -u
BACKUP=~/tortoise-backup-2026-09-09-T4
LOG="/c/Users/perzi/AppData/Local/Temp/claude/C--Users-perzi-dads-mmo-lab/e6db1900-f1cd-4099-995e-e1ef6bec1004/scratchpad/t4-rollback-plus.log"
say() { echo "[$(date -u +%H:%M:%SZ)] $*" | tee -a "$LOG"; }
onbox() { ssh m910q "$1" 2>&1 | tee -a "$LOG"; }

say "=========== T4 ROLLBACK (m910q): their script, then tw_char whole ==========="
bash "$(dirname "$0")/../tortoise-reimport-rehearsal-m910q-2026-09-08/tortoise-rollback.sh"

say "--- tw_char restored WHOLE from the fresh dump, and the Dockerfile put back ---"
onbox 'claude-say "Yulon T4: restoring tw_char whole from the fresh dump after the rollback" >/dev/null 2>&1; cd ~/tortoise-server && cp ~/t4/Dockerfile.before Dockerfile && echo "Dockerfile restored to the one the install was built from"; sudo docker compose stop >/dev/null 2>&1; sudo docker compose up -d tortoise-db >/dev/null 2>&1; for i in $(seq 1 40); do s=$(sudo docker inspect tortoise-db --format "{{.State.Health.Status}}" 2>/dev/null); [ "$s" = healthy ] && break; sleep 3; done; echo "db: $s"; PW=$(cat .db_password); M="sudo docker exec -i -e MYSQL_PWD=$PW tortoise-db mariadb -u root"; $M < '"$BACKUP"'/tw_char.sql && echo "tw_char restored whole"; $M -e "GRANT ALL ON tw_char.* TO '"'"'mangos'"'"'@'"'"'%'"'"'; FLUSH PRIVILEGES;"'

say "--- the four counts and the two tables, read back ---"
onbox 'cd ~/tortoise-server && PW=$(cat .db_password); M="sudo docker exec -i -e MYSQL_PWD=$PW tortoise-db mariadb -u root"; $M -N -e "SELECT COUNT(*) FROM tw_char.characters; SELECT COUNT(*) FROM tw_logon.account; SELECT COUNT(*) FROM tw_world.migrations; SELECT COUNT(*) FROM tw_char.migrations" | tr "\n" " "; echo " (characters, accounts, world migrations, char migrations -- expect 903 109 158 1)"; $M -N -e "SELECT CONCAT(table_name, \" present\") FROM information_schema.tables WHERE table_schema=\"tw_char\" AND table_name IN (\"character_pvp_currency\",\"character_inventory_copy\")"; sudo docker compose stop >/dev/null 2>&1; echo "stopped: $(sudo docker ps --format "{{.Names}}" | tr "\n" " ")"'
say "DONE. Report ROLLED BACK and stop; do not press it twice."
