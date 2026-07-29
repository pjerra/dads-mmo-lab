#!/bin/bash
# Import a DML WoW-playerbots export into Docker Desktop (native, no distro).
# Run from Git Bash on Windows:  bash import-to-desktop.sh [EXPORT_DIR]
#
# EXPORT_DIR defaults to C:\Users\<you>\dml-native\wow-server-playerbots — the
# same default export-from-wsl.sh writes to. It must hold that script's output
# PLUS the two compose files for the migrated stack:
#
#   docker-compose.yml           the stack. COPY migrate/docker-compose.migrated.yml
#                                — it is the working migrated server's own
#                                compose and is what the validation below
#                                accepts: container_name ac-database /
#                                ac-db-import / ac-authserver / ac-worldserver /
#                                ac-client-data-init, the ./env/dist/etc and
#                                ./env/dist/logs binds, the client-data volume,
#                                and AC_PLAYERBOTS_DATABASE_INFO.
#                                NOT ../wow-playerbots/docker-compose.yml: that
#                                one is the CLEAN stock-image stack (no host
#                                binds, no playerbots wiring, by design) and
#                                this script refuses it.
#   docker-compose.override.yml  the RUNTIME settings — SOAP on, the real bot
#                                min/max, the rates, and the ./modules mount.
#                                This script REFUSES without it: an import that
#                                skips it boots a 500-bot 1x-rate SOAP-off
#                                server that looks fine and is not the user's
#                                server (found live, 2026-07-24). The distro's
#                                own copy travels as
#                                conf/docker-compose.override.yml.orig.
#
# Idempotent-ish: re-running re-extracts and re-imports.
#
# NB: the folder name IS the title id the launcher/CLI look up — it must be
# wow-server-playerbots (same as in the distro), and the compose must keep the
# standard ac-* container_names, or `dml games list` / every `wow` feature
# misses the server (found live, 2026-07-24).
#
# Native `wow config` needs mikefarah yq on Windows: download
# yq_windows_amd64.exe and set DML_YQ_BIN to it (the launcher's native .bat
# does). Keep the runtime settings (SOAP/bots/rates env) in
# docker-compose.override.yml next to the compose — the config system reads
# and writes .services.ac-worldserver.environment in THAT file. Module
# sources must keep their .git dirs (installed-check is modules/<key>/.git).
#
# Env seams: DML_DOCKER, DB_ROOT_PASSWORD, COMPOSE_PROJECT_NAME,
# DML_HEALTH_TRIES, DML_HEALTH_SLEEP.
set -uo pipefail

# Docker Desktop's docker.exe (per-user installs are NOT on PATH). DML_DOCKER
# wins, matching dml-core's own discovery order.
if [ -n "${DML_DOCKER:-}" ]; then
  DOCKER_BIN_DIR="$(dirname "$DML_DOCKER")"
  export PATH="$DOCKER_BIN_DIR:$PATH"
else
  for CAND in \
    "${LOCALAPPDATA:-}/Programs/DockerDesktop/resources/bin" \
    "/c/Program Files/Docker/Docker/resources/bin"; do
    [ -e "$CAND/docker.exe" ] && export PATH="$CAND:$PATH" && break
  done
fi
command -v docker >/dev/null || { echo "[import] docker.exe not found - is Docker Desktop installed?"; exit 1; }
# The compose validation reads the merged config per SERVICE, which needs awk.
# Git Bash ships it; say so plainly rather than silently skipping the checks.
command -v awk >/dev/null || { echo "[import] awk not found - run this from Git Bash"; exit 1; }
docker info >/dev/null 2>&1 || { echo "[import] Docker Desktop engine is not running - start it first"; exit 1; }

DIR="${1:-$HOME/dml-native/wow-server-playerbots}"
cd "$DIR" || { echo "[import] export dir not found: $DIR"; exit 1; }
P="${COMPOSE_PROJECT_NAME:-dml-wow-native}"
DB_PASS="${DB_ROOT_PASSWORD:-password}"
HEALTH_TRIES="${DML_HEALTH_TRIES:-60}"
HEALTH_SLEEP="${DML_HEALTH_SLEEP:-5}"

echo "[import] checking the exported payload..."
[ -f docker-compose.yml ] || { echo "[import] no docker-compose.yml in $DIR - see this script's header"; exit 1; }
[ -f docker-compose.override.yml ] || {
  echo "[import] no docker-compose.override.yml in $DIR."
  echo "[import] That file IS the server's real config: SOAP on, the real bot"
  echo "[import] min/max, the rates, and the ./modules mount. Importing without"
  echo "[import] it boots a defaults server that looks fine and is not the"
  echo "[import] user's server. The distro's copy travels in the export as"
  echo "[import] conf/docker-compose.override.yml.orig - merge its environment"
  echo "[import] block into the ac-worldserver service and re-run."
  exit 1
}
for f in db-dump.sql.gz client-data.tar; do
  [ -f "$f" ] || { echo "[import] missing export file: $f"; exit 1; }
done
for i in worldserver authserver db-import client-data; do
  [ -f "img-$i.tar.gz" ] || { echo "[import] missing image tarball: img-$i.tar.gz - re-run the export"; exit 1; }
done
[ -d etc ] || { echo "[import] missing exported config tree: etc/ - re-run the export"; exit 1; }

# ----------------------------------------------------------------------------
# Compose validation.
#
# container_name was never the expensive failure — it stops the import dead at
# "database never became healthy", which is loud. The checks below are for the
# ones that are SILENT: every shape they reject produces a server that starts,
# looks healthy, and is not the user's server (README "the biggest lesson", and
# the 2026-07-24 boot failure). They all read the MERGED config, so it makes no
# difference whether a property lives in the base compose or the override.
#
# Bind mounts are matched on the CONTAINER side: `docker compose config`
# rewrites binds into long syntax with the host path resolved to an absolute
# one, so `./env/dist/etc` is not in the output but its target is.
# ----------------------------------------------------------------------------
cfg_service_block() { # SERVICE -> the merged block for that service
  printf '%s\n' "$CFG" | awk -v svc="$1" '
    /^[^ \t]/                    { in_svc = 0 }
    /^  [A-Za-z0-9._-]+:[ \t]*$/ { in_svc = ($0 ~ "^  " svc ":[ \t]*$") ? 1 : 0; next }
    in_svc                       { print }
  '
}
block_mounts() { # BLOCK CONTAINER-PATH
  # Long syntax puts the container path on its own `target:` line; the `/` in
  # the pattern keeps PORT entries (which also carry `target:`, with a number)
  # out of it. Short syntax `- ./host:/container` needs no rewriting.
  printf '%s\n' "$1" | sed 's|target: */|:/|' | grep -Eq ":$2/?(:[a-zA-Z,]+)?[ \t]*$"
}
block_has_env() { # BLOCK KEY
  printf '%s\n' "$1" | grep -Eq "(^|[ \t-])$2[:=]"
}

echo "[import] validating the compose stack..."
CFG="$(docker compose -p "$P" config 2>&1)" || { printf '%s\n' "$CFG"; exit 1; }
for c in ac-database ac-authserver ac-worldserver; do
  printf '%s\n' "$CFG" | grep -q "container_name: *$c" || {
    echo "[import] compose does not declare container_name: $c"
    echo "[import] every 'dml wow' arm addresses containers by those exact names"
    echo "[import] (docker exec ac-database ...), so they are not cosmetic."
    exit 1
  }
done

WORLD_CFG="$(cfg_service_block ac-worldserver)"
[ -n "$WORLD_CFG" ] || { echo "[import] compose declares no ac-worldserver service"; exit 1; }

block_mounts "$WORLD_CFG" /azerothcore/env/dist/etc || {
  echo "[import] ac-worldserver does not bind the host env/dist/etc tree."
  echo "[import] Add  ./env/dist/etc:/azerothcore/env/dist/etc  to its volumes."
  echo "[import] Without it the worldserver reads the IMAGE's built-in"
  echo "[import] .conf.dist defaults, the exported config staged into"
  echo "[import] env/dist/etc is never used, and every setting the launcher"
  echo "[import] saves there appears to save and does nothing."
  exit 1
}

if ! block_mounts "$WORLD_CFG" /azerothcore/env/dist/logs; then
  echo "[import] NOTE: ac-worldserver does not bind ./env/dist/logs -"
  echo "[import] AC_LOGS_DIR then writes inside the container only. Not fatal;"
  echo "[import] the working migrated server binds it."
fi

# Playerbots wiring. Detected from the export itself (the module sources
# travel with it) or from the updater switch in the merged config, so a
# genuinely non-playerbots migration is not held to it.
PLAYERBOTS=0
[ -d modules/mod-playerbots ] && PLAYERBOTS=1
printf '%s\n' "$CFG" | grep -q 'AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES' && PLAYERBOTS=1
if [ "$PLAYERBOTS" = 1 ]; then
  block_mounts "$WORLD_CFG" /azerothcore/modules || {
    echo "[import] ac-worldserver does not mount the module sources."
    echo "[import] Add  ./modules:/azerothcore/modules  (the override is where"
    echo "[import] the distro keeps it). With Playerbots.Updates.EnableDatabases=1"
    echo "[import] the worldserver scans /azerothcore/modules/mod-playerbots at"
    echo "[import] boot and SHUTS DOWN if it is missing - the only boot failure"
    echo "[import] of the 2026-07-24 migration."
    exit 1
  }
  block_has_env "$WORLD_CFG" AC_PLAYERBOTS_DATABASE_INFO || {
    echo "[import] ac-worldserver has no AC_PLAYERBOTS_DATABASE_INFO."
    echo "[import] The export carries a fourth database (acore_playerbots); a"
    echo "[import] worldserver with the other three wired and this one missing"
    echo "[import] boots and is wrong. Copy the line from"
    echo "[import] migrate/docker-compose.migrated.yml."
    exit 1
  }
fi

if block_mounts "$WORLD_CFG" /azerothcore/modules && [ ! -d modules ]; then
  echo "[import] the compose mounts ./modules but this export has no modules/ dir."
  echo "[import] The worldserver scans /azerothcore/modules/mod-playerbots at"
  echo "[import] boot and SHUTS DOWN if it is missing - re-run the export."
  exit 1
fi

echo "[import] staging the config tree + log dir the compose binds..."
mkdir -p env/dist/etc env/dist/logs
cp -r etc/. env/dist/etc/ || { echo "[import] could not stage etc/ into env/dist/etc"; exit 1; }

echo "[import] loading server images..."
for i in worldserver authserver db-import client-data; do
  gunzip -c "img-$i.tar.gz" | docker load || { echo "[import] failed to load img-$i.tar.gz"; exit 1; }
done

echo "[import] creating stack shell (volumes/network, nothing started)..."
docker compose -p "$P" up --no-start || exit 1

echo "[import] restoring client-data volume..."
VOL="${P}_client-data"
docker volume inspect "$VOL" >/dev/null 2>&1 || { echo "[import] volume missing after 'up --no-start': $VOL"; exit 1; }
WINDIR_SRC="$(cygpath -w "$DIR" 2>/dev/null || echo "$DIR")"
MSYS_NO_PATHCONV=1 docker run --rm \
  -v "$VOL:/to" \
  -v "$WINDIR_SRC:/src:ro" \
  --entrypoint tar nginx:alpine -C /to -xf /src/client-data.tar || exit 1

echo "[import] starting database..."
docker compose -p "$P" up -d ac-database || exit 1
H=""
for _ in $(seq 1 "$HEALTH_TRIES"); do
  H="$(docker inspect --format '{{.State.Health.Status}}' ac-database 2>/dev/null || true)"
  [ "$H" = "healthy" ] && break
  sleep "$HEALTH_SLEEP"
done
[ "$H" = "healthy" ] || { echo "[import] database never became healthy"; exit 1; }

echo "[import] restoring databases..."
gunzip -c db-dump.sql.gz | docker exec -i ac-database mysql -uroot -p"$DB_PASS" || { echo "[import] database restore failed"; exit 1; }

echo "[import] verification:"
docker exec ac-database mysql -uroot -p"$DB_PASS" -N -e "
  SELECT CONCAT('  characters: ', COUNT(*)) FROM acore_characters.characters;
  SELECT CONCAT('  accounts:   ', COUNT(*)) FROM acore_auth.account;
" 2>/dev/null

if [ -f conf/soap.env ]; then
  mkdir -p "$HOME/.dml"
  # Native `dml` sources this file from the WINDOWS home; strip the CRs that
  # wsl.exe piping adds so the values do not carry a trailing \r.
  tr -d '\r' < conf/soap.env > "$HOME/.dml/soap.env"
  chmod 600 "$HOME/.dml/soap.env" 2>/dev/null || true
  echo "[import] installed SOAP credentials -> $HOME/.dml/soap.env"
else
  echo "[import] NOTE: no conf/soap.env in the export - run 'dml wow soap-setup' after boot"
fi

echo "[import] booting the world..."
docker compose -p "$P" up -d || exit 1
echo "[import] COMPLETE - watch: docker logs -f ac-worldserver (ready = 'World Initialized In')"
