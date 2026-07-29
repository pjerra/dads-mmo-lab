#!/bin/bash
# Export a DML WSL (dml-arch) WoW-playerbots server for native Docker Desktop.
# Run INSIDE the distro:  wsl -d dml-arch -u dml -- bash /mnt/c/.../export-from-wsl.sh [OUT_DIR]
#
# OUT_DIR defaults to C:\Users\<you>\dml-native\wow-server-playerbots — the same
# default import-to-desktop.sh reads, and the folder name IS the title id the
# launcher/CLI look up, so it must stay wow-server-playerbots.
#
# Produces in OUT_DIR:
#   db-dump.sql.gz                          consistent dump of every acore_* DB
#   client-data.tar                         the maps/vmaps/mmaps/dbc volume
#   etc/                                    the live env/dist/etc config tree
#   modules/                                module sources WITH their .git dirs
#   conf/env.ac                             the compose env file
#   conf/soap.env                           launcher SOAP credentials (mode 600)
#   conf/docker-compose.override.yml.orig   the server's REAL runtime settings
#   img-*.tar.gz                            the EXACT images it runs (docker save)
#
# READ-ONLY towards the server except ONE thing: when the database container is
# not already running it is started ALONE for a consistent dump, then returned
# to the state it was found in — stopped if it existed and was stopped, removed
# if this script created it. An EXIT trap performs that restore, so an early
# failure (never-healthy database, suspiciously small dump) cannot leave a
# database running that the script found stopped. No other container is ever
# touched: an earlier version ran a project-wide teardown here, which would
# have killed a LIVE worldserver any time the database alone happened to be
# down.
#
# Env seams: DML_SERVER_DIR, DB_ROOT_PASSWORD, DML_IMAGE_TAG,
# DML_CLIENT_DATA_VOLUME, DML_HEALTH_TRIES, DML_HEALTH_SLEEP.
set -uo pipefail

SDIR="${DML_SERVER_DIR:-$HOME/games/wow-server-playerbots}"
# NB the `|| echo` fallback used to sit INSIDE the pipeline, where `tr` always
# succeeded and the fallback was therefore dead code — an unreachable cmd.exe
# produced an empty user name and a /mnt/c/Users//... path. Kept inside the
# ${1:-...} default so an explicit OUT_DIR never pays for the cmd.exe hop.
win_user() {
  local u
  u="$(cmd.exe /c echo %USERNAME% 2>/dev/null | tr -d '\r\n')"
  [ -n "$u" ] || u="${USER:-$(id -un)}"
  printf '%s' "$u"
}
OUT="${1:-/mnt/c/Users/$(win_user)/dml-native/wow-server-playerbots}"
DB_PASS="${DB_ROOT_PASSWORD:-password}"
IMAGE_TAG="${DML_IMAGE_TAG:-master}"
HEALTH_TRIES="${DML_HEALTH_TRIES:-60}"
HEALTH_SLEEP="${DML_HEALTH_SLEEP:-5}"

mkdir -p "$OUT" "$OUT/conf"
cd "$SDIR" || { echo "server dir not found: $SDIR"; exit 1; }

# Resolve ac-database through THIS server's own compose project rather than a
# bare `docker ps` name match: a bare container name answers for whichever
# project happens to own it (the same lesson the worldserver log snapshot had
# to learn). $CID is then the ONLY way this script addresses the container —
# resolving it and then going back to the bare name would buy nothing: with a
# second AzerothCore stack on the engine, the health poll would report the
# OTHER project's database as healthy and mysqldump would silently export the
# WRONG world. Only `docker compose <cmd> ac-database` may use the name, and
# there it is a compose SERVICE, already scoped to this project.
resolve_db_cid() { docker compose ps -aq ac-database 2>/dev/null | head -n 1; }
CID="$(resolve_db_cid)"
EXISTED_BEFORE=0
[ -n "$CID" ] && EXISTED_BEFORE=1
WAS_RUNNING=0
if [ -n "$CID" ] && [ "$(docker inspect -f '{{.State.Running}}' "$CID" 2>/dev/null)" = "true" ]; then
  WAS_RUNNING=1
fi

STARTED_DB=0
restore_db_state() {
  [ "$STARTED_DB" = "1" ] || return 0
  STARTED_DB=0
  if [ "$EXISTED_BEFORE" = "1" ]; then
    echo "[export] returning ac-database to the stopped state it was found in..."
    docker compose stop ac-database >/dev/null 2>&1
  else
    echo "[export] removing the ac-database container this export created..."
    docker compose rm -sf ac-database >/dev/null 2>&1
  fi
}
trap 'restore_db_state' EXIT
trap 'restore_db_state; exit 130' INT TERM

if [ "$WAS_RUNNING" = "0" ]; then
  echo "[export] starting ac-database alone for a consistent dump..."
  STARTED_DB=1
  docker compose up -d ac-database
  # The container may only exist NOW, so the id resolved before the `up` is
  # stale (empty, when this script created it). Ask again.
  CID="$(resolve_db_cid)"
fi
[ -n "$CID" ] || {
  echo "[export] could not resolve this project's ac-database container."
  echo "[export] Refusing to fall back to the bare name: on a machine running a"
  echo "[export] second AzerothCore stack that would dump the WRONG server."
  echo "[export] Check 'docker compose ps -a' in $SDIR."
  exit 1
}

H=""
for _ in $(seq 1 "$HEALTH_TRIES"); do
  H="$(docker inspect --format '{{.State.Health.Status}}' "$CID" 2>/dev/null || true)"
  [ "$H" = "healthy" ] && break
  sleep "$HEALTH_SLEEP"
done
[ "$H" = "healthy" ] || { echo "[export] database never became healthy"; exit 1; }

echo "[export] dumping acore databases..."
DBS="$(docker exec "$CID" mysql -uroot -p"$DB_PASS" -N -e "SHOW DATABASES" 2>/dev/null | grep '^acore_' | tr '\n' ' ')"
[ -n "$DBS" ] || { echo "[export] no acore_* databases visible - wrong DB_ROOT_PASSWORD?"; exit 1; }
# shellcheck disable=SC2086
docker exec "$CID" mysqldump -uroot -p"$DB_PASS" --databases $DBS \
  --single-transaction --quick --routines --events --triggers 2>/dev/null \
  | gzip > "$OUT/db-dump.sql.gz"
[ "$(stat -c %s "$OUT/db-dump.sql.gz")" -gt 1000000 ] || { echo "[export] dump suspiciously small - aborting"; exit 1; }

restore_db_state

# The volume is named <compose project>_<volume key>, and compose derives the
# project from the directory name — so follow DML_SERVER_DIR instead of naming
# the distro's default path here. Check it exists first: `docker run -v
# <unknown>:/from` silently CREATES an empty volume and would hand us an empty
# tar that only surfaces as a broken world after the import.
PROJECT="${COMPOSE_PROJECT_NAME:-$(basename "$SDIR")}"
CLIENT_DATA_VOLUME="${DML_CLIENT_DATA_VOLUME:-${PROJECT}_ac-client-data}"
docker volume inspect "$CLIENT_DATA_VOLUME" >/dev/null 2>&1 || {
  echo "[export] client-data volume not found: $CLIENT_DATA_VOLUME"
  echo "[export] pick the right one from 'docker volume ls' and set DML_CLIENT_DATA_VOLUME"
  exit 1
}
echo "[export] exporting client-data volume ($CLIENT_DATA_VOLUME)..."
docker run --rm --entrypoint tar -v "$CLIENT_DATA_VOLUME:/from:ro" mysql:8.4 \
  -C /from -cf - . > "$OUT/client-data.tar"

echo "[export] copying config tree..."
rm -rf "${OUT:?}/etc"
cp -r "$SDIR/env/dist/etc" "$OUT/etc"
cp "$SDIR/conf/dist/env.ac" "$OUT/conf/env.ac" 2>/dev/null || true
# CRITICAL: the override carries the REAL runtime settings (SOAP on, bot
# min/max, rates, the ./modules mount). The confs alone run module DEFAULTS —
# a migration that skips this file boots a 500-bot 1x-rate server with SOAP
# off (found live, 2026-07-24). Merge its environment block into the native
# compose's ac-worldserver.
cp "$SDIR/docker-compose.override.yml" "$OUT/conf/docker-compose.override.yml.orig" 2>/dev/null || true
# Launcher-side SOAP reads $HOME/.dml/soap.env, and native `dml` reads the
# WINDOWS home — so carry the credentials across; import-to-desktop.sh installs
# this copy there.
if [ -f "$HOME/.dml/soap.env" ]; then
  cp "$HOME/.dml/soap.env" "$OUT/conf/soap.env"
  chmod 600 "$OUT/conf/soap.env" 2>/dev/null || true
else
  echo "[export] NOTE: no ~/.dml/soap.env here - SOAP will need 'dml wow soap-setup' after the import"
fi

echo "[export] copying module sources (WITH .git - the CLI's installed-check"
echo "         is [[ -d modules/<key>/.git ]], and the playerbots DB updater"
echo "         needs the sources mounted at /azerothcore/modules)..."
rm -rf "${OUT:?}/modules"
mkdir -p "$OUT/modules"
(cd "$SDIR/modules" && tar -cf - .) | (cd "$OUT/modules" && tar -xf -)

echo "[export] saving the exact server images (tag: $IMAGE_TAG)..."
for img in worldserver authserver db-import client-data; do
  ref="acore/ac-wotlk-$img:$IMAGE_TAG"
  # Unchecked, `docker save` of an absent image writes an error to stderr and
  # gzip still produces a plausible-looking file the import would fail to load.
  docker image inspect "$ref" >/dev/null 2>&1 || {
    echo "[export] image not present locally: $ref"
    echo "[export] if this server runs another tag, set DML_IMAGE_TAG (see 'docker compose images')"
    exit 1
  }
  docker save "$ref" | gzip > "$OUT/img-$img.tar.gz"
done

ls -lh "$OUT"
echo "[export] COMPLETE -> $OUT"
