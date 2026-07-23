#!/bin/bash
# Import a DML WoW-playerbots export into Docker Desktop (native, no distro).
# Run from Git Bash on Windows:  bash import-to-desktop.sh [EXPORT_DIR]
#
# EXPORT_DIR (default C:\Users\<you>\dml-native\wow-playerbots) must contain the
# export-from-wsl.sh output AND a docker-compose.yml for the migrated stack
# (copy poc/native-docker/wow-playerbots/docker-compose.yml and add the ./etc +
# ./logs binds and AC_PLAYERBOTS_DATABASE_INFO — or use the one this migration
# already generated). Idempotent-ish: re-running re-extracts and re-imports.
set -uo pipefail

# Docker Desktop's docker.exe (per-user installs are NOT on PATH)
for CAND in \
  "$LOCALAPPDATA/Programs/DockerDesktop/resources/bin" \
  "/c/Program Files/Docker/Docker/resources/bin"; do
  [ -e "$CAND/docker.exe" ] && export PATH="$CAND:$PATH" && break
done
command -v docker >/dev/null || { echo "[import] docker.exe not found - is Docker Desktop installed?"; exit 1; }
docker info >/dev/null 2>&1 || { echo "[import] Docker Desktop engine is not running - start it first"; exit 1; }

DIR="${1:-$HOME/dml-native/wow-playerbots}"
cd "$DIR" || { echo "[import] export dir not found: $DIR"; exit 1; }
mkdir -p logs
P="dml-wow-native"

echo "[import] loading server images..."
for f in img-worldserver img-authserver img-db-import img-client-data; do
  [ -f "$f.tar.gz" ] && gunzip -c "$f.tar.gz" | docker load
done

echo "[import] creating stack shell (volumes/network, nothing started)..."
docker compose -p "$P" config -q || exit 1
docker compose -p "$P" up --no-start

echo "[import] restoring client-data volume..."
WINDIR_SRC="$(cygpath -w "$DIR" 2>/dev/null || echo "$DIR")"
MSYS_NO_PATHCONV=1 docker run --rm \
  -v "${P}_client-data:/to" \
  -v "$WINDIR_SRC:/src:ro" \
  --entrypoint tar nginx:alpine -C /to -xf /src/client-data.tar

echo "[import] starting database..."
docker compose -p "$P" up -d ac-database
H=""
for i in $(seq 1 60); do
  H=$(docker inspect --format '{{.State.Health.Status}}' dml-native-database 2>/dev/null || true)
  [ "$H" = "healthy" ] && break
  sleep 5
done
[ "$H" = "healthy" ] || { echo "[import] database never became healthy"; exit 1; }

echo "[import] restoring databases..."
gunzip -c db-dump.sql.gz | docker exec -i dml-native-database mysql -uroot -ppassword

echo "[import] verification:"
docker exec dml-native-database mysql -uroot -ppassword -N -e "
  SELECT CONCAT('  characters: ', COUNT(*)) FROM acore_characters.characters;
  SELECT CONCAT('  accounts:   ', COUNT(*)) FROM acore_auth.account;
" 2>/dev/null

echo "[import] booting the world..."
docker compose -p "$P" up -d
echo "[import] COMPLETE - watch: docker logs -f dml-native-worldserver (ready = 'World initialized')"
