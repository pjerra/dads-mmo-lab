#!/usr/bin/env bash
# The three Tortoise counts, read before this lane touches anything and again after it is done.
#
# WHY THIS EXISTS AND THE 09-08 RUN HAD NO EQUIVALENT. m910q's Tortoise install was reimported
# and rolled back at 01:06Z on 2026-09-09 and is at its pre-attempt counts -- 903 characters,
# 109 accounts, 158 world migrations -- and this lane's brief is to leave it that way. 7.9's
# section 11 takes a dump of all four Tortoise databases and RESTORES every one of them, so
# this lane writes to that install by design. A round trip that lands where it started is only
# a claim until both ends are counted.
#
# It starts ONLY the db container, asks three SELECTs, and stops it again -- no realmd, no
# mangosd, so it is not a server and does not collide with m910q's one-server-at-a-time rule.
# The password is read from the install's own .env into MYSQL_PWD and is never printed, never
# in an argv: `gate-logs-carry-generated-passwords`.
#
# RUN IT:  bash tortoise-counts.sh <tag>          # writes ~/lane79b/out/tortoise-counts.txt
set -u
TAG="${1:?usage: tortoise-counts.sh <tag>}"
OUT=$HOME/lane79b/out
DIR=$HOME/tortoise-server
mkdir -p "$OUT"

was_up=$(docker inspect tortoise-db --format '{{.State.Running}}' 2>/dev/null || echo unknown)
if [ "$was_up" != "true" ]; then docker start tortoise-db >/dev/null 2>&1; fi
PW=$(sed -n 's/^DB_ROOT_PASSWORD=//p' "$DIR/.env")
# Wait for MARIADB, not for the container. `docker exec sh -c 'exit 0'` succeeds the instant
# the container runs, which on the first attempt at 05:46 was ~20 s before the server was
# listening, and all four SELECTs came back `ERROR 2002 ... mysqld.sock (2)`. An error string
# where a count belongs is not a count, and a probe that reports one is worse than none.
waited=0
until docker exec -e MYSQL_PWD="$PW" tortoise-db mariadb -u root -N -e "SELECT 1" >/dev/null 2>&1; do
  waited=$((waited + 2)); sleep 2
  if [ "$waited" -ge 180 ]; then
    echo "=== tortoise-counts $TAG: mariadb never answered SELECT 1 in ${waited}s; NO COUNTS TAKEN ===" \
      >> "$OUT/tortoise-counts.txt"
    [ "$was_up" != "true" ] && docker stop tortoise-db >/dev/null 2>&1
    tail -3 "$OUT/tortoise-counts.txt"; exit 1
  fi
done
{
  echo "=== tortoise-counts $TAG  $(date -Is)  (UTC $(date -u -Is)) ==="
  echo "tortoise-db was already running before this probe: $was_up   (waited ${waited}s for mariadb)"
  for q in \
    "SELECT COUNT(*) FROM tw_char.characters" \
    "SELECT COUNT(*) FROM tw_logon.account" \
    "SELECT COUNT(*) FROM tw_world.migrations" \
    "SELECT COUNT(*) FROM tw_char.migrations"
  do
    printf '%-48s %s\n' "$q" "$(docker exec -e MYSQL_PWD="$PW" tortoise-db mariadb -u root -N -e "$q" 2>&1 | tr -d '\r')"
  done
} >> "$OUT/tortoise-counts.txt"
if [ "$was_up" != "true" ]; then
  docker stop tortoise-db >/dev/null 2>&1
  echo "tortoise-db stopped again (it was not running when this probe started)" >> "$OUT/tortoise-counts.txt"
fi
tail -8 "$OUT/tortoise-counts.txt"
