#!/bin/bash
# Export a DML WSL (dml-arch) WoW-playerbots server for native Docker Desktop.
# Run INSIDE the distro:  wsl -d dml-arch -u dml -- bash /mnt/c/.../export-from-wsl.sh [OUT_DIR]
#
# Produces in OUT_DIR (default C:\Users\<you>\dml-native\wow-playerbots):
#   db-dump.sql.gz     consistent dump of every acore_* database
#   client-data.tar    the maps/vmaps/mmaps/dbc volume
#   etc/               the live env/dist/etc config tree (module confs included)
#   conf/env.ac        the compose env file
#   img-*.tar.gz       the EXACT images the server runs (docker save)
#
# Read-only towards the server except one thing: if the stack is stopped, the
# database container alone is started for the dump and stopped again after.
set -uo pipefail
SDIR="${DML_SERVER_DIR:-$HOME/games/wow-server-playerbots}"
OUT="${1:-/mnt/c/Users/$(cmd.exe /c echo %USERNAME% 2>/dev/null | tr -d '\r' || echo "$USER")/dml-native/wow-playerbots}"
mkdir -p "$OUT"
cd "$SDIR" || { echo "server dir not found: $SDIR"; exit 1; }

WAS_RUNNING=0
docker ps --format '{{.Names}}' | grep -q '^ac-database$' && WAS_RUNNING=1

if [ "$WAS_RUNNING" = "0" ]; then
  echo "[export] starting ac-database alone for a consistent dump..."
  docker compose up -d ac-database
fi
H=""
for i in $(seq 1 60); do
  H=$(docker inspect --format '{{.State.Health.Status}}' ac-database 2>/dev/null || true)
  [ "$H" = "healthy" ] && break
  sleep 5
done
[ "$H" = "healthy" ] || { echo "[export] database never became healthy"; exit 1; }

echo "[export] dumping acore databases..."
DBS=$(docker exec ac-database mysql -uroot -ppassword -N -e "SHOW DATABASES" 2>/dev/null | grep '^acore_' | tr '\n' ' ')
# shellcheck disable=SC2086
docker exec ac-database mysqldump -uroot -ppassword --databases $DBS \
  --single-transaction --quick --routines --events --triggers 2>/dev/null \
  | gzip > "$OUT/db-dump.sql.gz"
[ "$(stat -c %s "$OUT/db-dump.sql.gz")" -gt 1000000 ] || { echo "[export] dump suspiciously small - aborting"; exit 1; }

if [ "$WAS_RUNNING" = "0" ]; then
  echo "[export] returning stack to stopped state..."
  docker compose down
fi

echo "[export] exporting client-data volume..."
docker run --rm --entrypoint tar -v wow-server-playerbots_ac-client-data:/from:ro mysql:8.4 \
  -C /from -cf - . > "$OUT/client-data.tar"

echo "[export] copying config tree..."
rm -rf "$OUT/etc"
cp -r "$SDIR/env/dist/etc" "$OUT/etc"
mkdir -p "$OUT/conf"
cp "$SDIR/conf/dist/env.ac" "$OUT/conf/env.ac" 2>/dev/null || true

echo "[export] saving the exact server images..."
for img in worldserver authserver db-import client-data; do
  docker save "acore/ac-wotlk-$img:master" | gzip > "$OUT/img-$img.tar.gz"
done

ls -lh "$OUT"
echo "[export] COMPLETE -> $OUT"
