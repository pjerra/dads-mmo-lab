#!/bin/bash
# check-migrate-scripts.sh — guardrails for the two migration scripts.
#
# The migration scripts are proven live (README increment 4) but they live in
# poc/ with no test harness, so they drifted away from their own recorded
# lessons: the ac-* container-name lesson was written into a comment while the
# code kept polling dml-native-*, and the two scripts' default directories
# stopped pointing at each other. This file is the tripwire for exactly that
# class.
#
# Run it from anywhere:  bash poc/native-docker/migrate/check-migrate-scripts.sh
# It needs bash + coreutils only; Git Bash on Windows is enough. shellcheck is
# used when present and reported as an explicit SKIP when not (never as a pass —
# a silent skip is the vacuous-pass trap). Set DML_REQUIRE_SHELLCHECK=1 to turn
# that skip into a failure (CI seam).
#
# Two kinds of check:
#   * STATIC   — greps/parses the scripts for the invariants above.
#   * BEHAVIOUR — actually RUNS each script against a recorded fake `docker` on
#                 PATH and asserts the call sequence. This is what verifies the
#                 export's "read-only except starting the database alone" claim,
#                 including the failure paths, without touching a real server.
set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXPORT_SH="$HERE/export-from-wsl.sh"
IMPORT_SH="$HERE/import-to-desktop.sh"
# The CLEAN stock-image stack (no playerbots module, no host binds). Still
# checked for the ac-* names, but it is NOT what a migration copies.
TEMPLATE="$HERE/../wow-playerbots/docker-compose.yml"
# The MIGRATION stack — modelled on the working migrated server. This is the
# file the import's header points at and the file the behaviour fixtures use,
# so "the template the header names actually passes the import's validation"
# is asserted rather than assumed.
MIGRATED="$HERE/docker-compose.migrated.yml"

PASS=0
FAIL=0
SKIP=0
ok()      { printf '  [ok]   %s\n' "$1"; PASS=$((PASS + 1)); }
bad()     { printf '  [FAIL] %s\n' "$1"; FAIL=$((FAIL + 1)); }
skipped() { printf '  [SKIP] %s\n' "$1"; SKIP=$((SKIP + 1)); }
section() { printf '\n== %s ==\n' "$1"; }

# must DESC FILE ERE / must_not DESC FILE ERE / must_lit DESC FILE FIXED-STRING
must()     { if grep -Eq -- "$3" "$2"; then ok "$1"; else bad "$1"; fi; }
must_not() { if grep -Eq -- "$3" "$2"; then bad "$1"; else ok "$1"; fi; }
must_lit() { if grep -Fq -- "$3" "$2"; then ok "$1"; else bad "$1"; fi; }
# must_code: the match has to be CODE, not a comment. These scripts already
# failed once by writing a lesson into a comment while the code kept doing the
# old thing, so a check a comment can satisfy is no check at all.
must_code() { if grep -Eq -- "^[^#]*($3)" "$2"; then ok "$1"; else bad "$1"; fi; }
assert_eq() { # DESC EXPECTED ACTUAL
  if [ "$2" = "$3" ]; then ok "$1"; else bad "$1 (expected '$2', got '$3')"; fi
}
assert_true() { # DESC CONDITION-RESULT(0/1)
  if [ "$2" = 0 ]; then ok "$1"; else bad "$1"; fi
}

for f in "$EXPORT_SH" "$IMPORT_SH" "$TEMPLATE" "$MIGRATED"; do
  [ -f "$f" ] || { echo "missing file: $f"; exit 2; }
done

# ---------------------------------------------------------------- static -----

section "stale dml-native-* container names (the lesson the code ignored)"
must_not "export-from-wsl.sh references no dml-native-* container" "$EXPORT_SH" 'dml-native-'
must_not "import-to-desktop.sh references no dml-native-* container" "$IMPORT_SH" 'dml-native-'
must_not "poc wow-playerbots compose declares no dml-native-* container_name" "$TEMPLATE" 'container_name: *dml-native-'

section "the ac-* names the working server actually uses (mutation guard: deleting the lines above must not pass)"
must_code "import polls ac-database health" "$IMPORT_SH" 'Health\.Status.*ac-database'
must_code "import restores the dump into ac-database" "$IMPORT_SH" 'docker exec -i ac-database mysql'
must_code "import's verification queries ac-database" "$IMPORT_SH" 'docker exec ac-database mysql'
must "import's follow-the-boot hint names ac-worldserver" "$IMPORT_SH" 'docker logs -f ac-worldserver'
for c in ac-database ac-db-import ac-authserver ac-worldserver ac-client-data-init; do
  must "template declares container_name: $c" "$TEMPLATE" "container_name: $c"
done

section "directory defaults (the two scripts must point at each other)"
export_default() { sed -n 's/^OUT="\${1:-\(.*\)}"$/\1/p' "$EXPORT_SH" | head -1; }
import_default() { sed -n 's/^DIR="\${1:-\(.*\)}"$/\1/p' "$IMPORT_SH" | head -1; }
E_DEF="$(export_default)"
I_DEF="$(import_default)"
if [ -z "$E_DEF" ] || [ -z "$I_DEF" ]; then
  bad "could not parse both default dirs (export='$E_DEF' import='$I_DEF')"
else
  assert_eq "export default folder name is the title id wow-server-playerbots" \
    "wow-server-playerbots" "$(basename "$E_DEF")"
  assert_eq "import default folder name is the title id wow-server-playerbots" \
    "wow-server-playerbots" "$(basename "$I_DEF")"
  assert_eq "both defaults resolve to the same folder name" \
    "$(basename "$E_DEF")" "$(basename "$I_DEF")"
fi
must_not "export header advertises no stale dml-native/wow-playerbots path" "$EXPORT_SH" 'dml-native.wow-playerbots'
must_not "import header advertises no stale dml-native/wow-playerbots path" "$IMPORT_SH" 'dml-native.wow-playerbots'

section "layout the WORKING server's compose actually binds"
must_code "import prepares env/dist/etc (the compose bind)" "$IMPORT_SH" 'env/dist/etc'
must_code "import prepares env/dist/logs (the compose bind)" "$IMPORT_SH" 'env/dist/logs'
must_not "import no longer creates the stale bare ./logs dir" "$IMPORT_SH" '^ *mkdir -p logs$'
must_code "import stages the exported etc/ tree into env/dist/etc" "$IMPORT_SH" 'cp -r etc'

section "the import validates what actually broke live, not just container_name"
# container_name alone was never the failure that cost anyone a day. These
# three are, and all three are silent: the server boots, looks healthy, and is
# not the user's server.
must_code "import scopes its compose checks to one service" "$IMPORT_SH" 'cfg_service_block'
must_code "import requires the env/dist/etc bind (else Settings saves go nowhere)" \
  "$IMPORT_SH" '/azerothcore/env/dist/etc'
must_code "import checks the env/dist/logs bind" "$IMPORT_SH" '/azerothcore/env/dist/logs'
must_code "import requires the ./modules mount when the export carries mod-playerbots" \
  "$IMPORT_SH" 'block_mounts .* /azerothcore/modules'
must_code "import requires AC_PLAYERBOTS_DATABASE_INFO for a playerbots export" \
  "$IMPORT_SH" 'AC_PLAYERBOTS_DATABASE_INFO'
must_lit "import's header points at the migration template it validates" \
  "$IMPORT_SH" "migrate/docker-compose.migrated.yml"

section "the migration template the header names must PASS that validation"
# F4's class: the header told users to copy a template that failed the import's
# own checks, and the harness's happy path used it and reported success.
for c in ac-database ac-db-import ac-authserver ac-worldserver ac-client-data-init; do
  must "migration template declares container_name: $c" "$MIGRATED" "container_name: $c"
done
must_not "migration template declares no dml-native-* container_name" "$MIGRATED" 'container_name: *dml-native-'
must_lit "migration template binds the host config tree" "$MIGRATED" "./env/dist/etc:/azerothcore/env/dist/etc"
must_lit "migration template binds the host log dir" "$MIGRATED" "./env/dist/logs:/azerothcore/env/dist/logs"
must "migration template wires the playerbots database" "$MIGRATED" 'AC_PLAYERBOTS_DATABASE_INFO'
must_lit "migration template keeps the client-data volume read-only on the world" \
  "$MIGRATED" "client-data:/azerothcore/env/dist/data/:ro"
# WAS: 'keeps the image tag a seam', asserting acore/…:${IMAGE_TAG:-master}.
# That seam was the BUG (incident 2026-08-02). A migrated stack carries the
# user's OWN images -- playerbots compiled in -- and parking them on upstream's
# moving `:master` meant an ordinary `docker compose up` pulled a fresher
# upstream build and overwrote all four. Stock AzerothCore came up wearing the
# user's server's name: bots gone from the binary, mmaps demanding newer client
# data, client-data-init failing outright. Nothing in the output named a cause.
must "migration template runs the images from a namespace no registry serves" \
  "$MIGRATED" 'dml\.local/ac-wotlk-worldserver:\$\{DML_LOCAL_TAG:-migrated\}'
must_not "migration template never runs an image off upstream's moving tag" \
  "$MIGRATED" 'image: *acore/ac-wotlk-[a-z-]*:\$\{IMAGE_TAG:-master\}'
must "import retags every loaded image into dml.local" "$IMPORT_SH" 'docker tag .*dml\.local/ac-wotlk-'

section "soap.env carry-over (README: required, script never did it)"
must_code "export carries ~/.dml/soap.env into the export dir" "$EXPORT_SH" 'soap\.env'
must_code "import installs soap.env into the Windows home" "$IMPORT_SH" 'soap\.env'
must_lit "import strips CRs from soap.env" "$IMPORT_SH" "tr -d '\\r'"

section "export is read-only towards the server (except the database, restored after)"
must_not "export never runs a project-wide compose down" "$EXPORT_SH" 'compose down'
must "export installs an EXIT trap so early failures still restore" "$EXPORT_SH" '^trap .*EXIT'
must_code "export restores by stopping ONLY ac-database" "$EXPORT_SH" 'compose stop ac-database'
must_code "export removes a container it created itself" "$EXPORT_SH" 'compose rm .*ac-database'
must_code "export resolves ac-database through its own compose project" "$EXPORT_SH" 'compose ps .*ac-database'

section "the resolved container is USED, not just resolved (bare names answer for another project's container)"
# The lesson was applied to one line and then dropped: CID was resolved
# project-scoped and never used again, while the health poll, SHOW DATABASES
# and mysqldump all still said `ac-database`. On a box running two AzerothCore
# stacks that dumps the WRONG server's world and reports success.
must_code "export polls health through the resolved container id" "$EXPORT_SH" 'Health\.Status.*\$CID'
must_code "export lists databases through the resolved container id" "$EXPORT_SH" 'docker exec "?\$CID"? mysql '
must_code "export dumps through the resolved container id" "$EXPORT_SH" 'docker exec "?\$CID"? mysqldump'
assert_eq "export re-resolves the container id after creating it" \
  2 "$(grep -c 'CID="\$(resolve_db_cid)"' "$EXPORT_SH")"
must_not "export never inspects/execs ac-database by bare name (compose service args are fine)" \
  "$EXPORT_SH" '^[^#]*docker (inspect|exec) .*ac-database'
must_code "export refuses rather than falling back to a bare name" "$EXPORT_SH" 'could not resolve .*ac-database'

section "silent-wrong-output guards"
must_not "export does not hardcode the distro's client-data volume name" "$EXPORT_SH" 'wow-server-playerbots_ac-client-data'
must_code "export verifies the client-data volume exists before tarring it" "$EXPORT_SH" 'volume inspect'
must_code "export verifies each image exists before docker save" "$EXPORT_SH" 'image inspect'
must_code "import aborts on a missing exported image tarball" "$IMPORT_SH" 'image tarball'
must_not "import no longer skips a missing image tarball silently" "$IMPORT_SH" '\[ -f "\$f\.tar\.gz" \] &&'
must_code "import refuses without docker-compose.override.yml (the biggest lesson)" "$IMPORT_SH" 'docker-compose\.override\.yml'

section "hygiene"
if grep -q $'\r' "$MIGRATED"; then bad "docker-compose.migrated.yml is LF-only (no CR bytes)"; else ok "docker-compose.migrated.yml is LF-only (no CR bytes)"; fi
for f in "$EXPORT_SH" "$IMPORT_SH" "${BASH_SOURCE[0]}"; do
  n="$(basename "$f")"
  if grep -q $'\r' "$f"; then bad "$n is LF-only (no CR bytes)"; else ok "$n is LF-only (no CR bytes)"; fi
  if bash -n "$f" 2>/dev/null; then ok "$n parses (bash -n)"; else bad "$n parses (bash -n)"; fi
done

# ------------------------------------------------------------- behaviour -----

TMPROOT="$(mktemp -d)"
trap 'rm -rf "$TMPROOT"' EXIT

make_fake_docker() { # BIN_DIR
  mkdir -p "$1"
  cat > "$1/docker" <<'FAKE'
#!/bin/bash
# Recorded fake `docker`. Every invocation is appended to $FAKE_DOCKER_LOG so a
# test can read the call sequence back IN ORDER (same oracle style as the
# repo's LifecycleEnv fakes). Behaviour is steered by FAKE_* env vars.
printf '%s\n' "$*" >> "$FAKE_DOCKER_LOG"
# Scenario state that OUTLIVES one invocation, so the fake can model the one
# thing that matters here: a container that did not exist before `compose up`
# and does after it. A stateless fake made "resolve the id again after
# creating the container" untestable.
FAKE_STATE_DIR="$(dirname "$FAKE_DOCKER_LOG")/fake-state"
mkdir -p "$FAKE_STATE_DIR"
db_exists() {
  [ "${FAKE_DB_EXISTS:-false}" = true ] && return 0
  [ -f "$FAKE_STATE_DIR/db-created" ]
}
case "${1:-}" in
  info) exit 0 ;;
  ps)
    # bare `docker ps --format ...` — the legacy, project-blind detection
    [ "${FAKE_DB_RUNNING:-false}" = true ] && echo ac-database
    exit 0 ;;
  compose)
    shift
    # Find the subcommand: the first argument that is neither a flag nor the
    # value of one. Matching positionally would miss `compose -p NAME config`.
    all_args="$*"
    sub=""
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -p|--project-name|-f|--file|--project-directory) shift; shift || break ;;
        -*) shift ;;
        *)  sub="$1"; break ;;
      esac
    done
    case "$sub" in
      ps)
        want_all=false
        for a in $all_args; do case "$a" in -aq|-a|--all) want_all=true ;; esac; done
        if db_exists; then
          if [ "$want_all" = true ] || [ "${FAKE_DB_RUNNING:-false}" = true ]; then
            echo cid-ac-database
          fi
        fi
        exit 0 ;;
      up)
        # `compose up -d ac-database` CREATES the container when it is absent —
        # the state the very next `compose ps -aq` must report. FAKE_UP_CREATES_
        # NOTHING models the honest could-not-tell: the id never resolves.
        if [ -z "${FAKE_UP_CREATES_NOTHING:-}" ]; then
          case "$all_args" in *ac-database*) : > "$FAKE_STATE_DIR/db-created" ;; esac
        fi
        exit 0 ;;
      rm) rm -f "$FAKE_STATE_DIR/db-created"; exit 0 ;;
      config)
        # The merged config the import validates. Derived from the REAL files in
        # the working directory rather than a canned blob: a fixed happy answer
        # made every compose assertion vacuous — the harness green-lit a
        # template that the check would have rejected. Concatenation is not
        # compose's merge, but for grep-shaped validation it carries the same
        # lines the merge would.
        for f in docker-compose.yml docker-compose.override.yml; do
          [ -f "$f" ] && cat "$f"
        done
        exit 0 ;;
      *) exit 0 ;;
    esac ;;
  inspect)
    case "$*" in
      *Health.Status*) echo "${FAKE_DB_HEALTH:-healthy}" ;;
      *State.Running*) echo "${FAKE_DB_RUNNING:-false}" ;;
    esac
    exit 0 ;;
  exec)
    stdin_flag=false
    for a in "$@"; do [ "$a" = "-i" ] && stdin_flag=true; done
    [ "$stdin_flag" = true ] && cat > /dev/null
    case "$*" in
      *mysqldump*)        head -c "${FAKE_DUMP_BYTES:-4000000}" /dev/urandom ;;
      *"SHOW DATABASES"*) printf 'acore_auth\nacore_characters\nacore_playerbots\nacore_world\nmysql\n' ;;
      *SELECT*)           printf '  characters: 2505\n  accounts:   255\n' ;;
    esac
    exit 0 ;;
  run)    printf 'FAKE-TAR-STREAM'; exit 0 ;;
  save)   head -c 4096 /dev/urandom; exit 0 ;;
  load)   cat > /dev/null; exit 0 ;;
  image)  exit "${FAKE_IMAGE_INSPECT_EXIT:-0}" ;;
  volume) exit "${FAKE_VOLUME_INSPECT_EXIT:-0}" ;;
esac
exit 0
FAKE
  chmod +x "$1/docker"
}

make_fake_server_dir() { # SERVER_DIR
  local s="$1"
  mkdir -p "$s/env/dist/etc/modules" "$s/conf/dist" "$s/modules/mod-playerbots/.git"
  printf 'x\n' > "$s/env/dist/etc/worldserver.conf"
  printf 'x\n' > "$s/env/dist/etc/modules/playerbots.conf"
  printf 'DOCKER_DB_EXTERNAL_PORT=3306\n' > "$s/conf/dist/env.ac"
  printf 'services:\n  ac-worldserver:\n    environment:\n      AC_SOAP_ENABLED: "1"\n' \
    > "$s/docker-compose.override.yml"
  printf 'name: dml-wow-native\n' > "$s/docker-compose.yml"
}

# run_export SCENARIO-DIR -- runs export-from-wsl.sh with the fake docker.
run_export() { # NAME  [VAR=VAL ...]
  local name="$1"; shift
  local root="$TMPROOT/$name"
  local bin="$root/bin" srv="$root/server" out="$root/out" home="$root/home"
  mkdir -p "$root" "$out" "$home/.dml"
  printf 'DML_SOAP_URL=http://127.0.0.1:7878/\nDML_SOAP_USER=dmlsoap\nDML_SOAP_PASS=secret\n' \
    > "$home/.dml/soap.env"
  make_fake_docker "$bin"
  make_fake_server_dir "$srv"
  EXPORT_LOG="$root/docker.log"
  : > "$EXPORT_LOG"
  EXPORT_OUT="$out"
  # Scenario knobs arrive as NAME=VALUE words; hand them to `env` rather than
  # `export "$@"`, which shellcheck reads as exporting a variable NAME.
  ( export PATH="$bin:$PATH" FAKE_DOCKER_LOG="$EXPORT_LOG" HOME="$home" DML_SERVER_DIR="$srv"
    timeout 25 env "$@" bash "$EXPORT_SH" "$out" > "$root/stdout.txt" 2>&1 )
  EXPORT_RC=$?
}

log_has()  { grep -Fq -- "$2" "$1"; }

section "BEHAVIOUR: export leaves the server exactly as it found it"

run_export stopped FAKE_DB_EXISTS=false FAKE_DB_RUNNING=false
assert_eq "[stack fully down] export succeeds" 0 "$EXPORT_RC"
if log_has "$EXPORT_LOG" "compose up -d ac-database"; then ok "[stack fully down] started the database alone"; else bad "[stack fully down] started the database alone"; fi
# "alone" is the load-bearing word: a bare `compose up` would boot the whole
# world on a machine whose owner only asked for an export.
if grep -E '^compose .*\bup\b' "$EXPORT_LOG" | grep -vq 'ac-database'; then bad "[stack fully down] started ONLY ac-database"; else ok "[stack fully down] started ONLY ac-database"; fi
if grep -q 'compose down' "$EXPORT_LOG"; then bad "[stack fully down] no project-wide teardown"; else ok "[stack fully down] no project-wide teardown"; fi
if grep -Eq 'compose rm .*ac-database' "$EXPORT_LOG"; then ok "[stack fully down] removed the container it created"; else bad "[stack fully down] removed the container it created"; fi

run_export running FAKE_DB_EXISTS=true FAKE_DB_RUNNING=true
assert_eq "[database already running] export succeeds" 0 "$EXPORT_RC"
if grep -Eq 'compose up -d ac-database' "$EXPORT_LOG"; then bad "[database already running] did not touch the running database"; else ok "[database already running] did not touch the running database"; fi
if grep -Eq 'compose (down|stop|rm)' "$EXPORT_LOG"; then bad "[database already running] stopped nothing"; else ok "[database already running] stopped nothing"; fi

run_export created_stopped FAKE_DB_EXISTS=true FAKE_DB_RUNNING=false
assert_eq "[container exists but stopped] export succeeds" 0 "$EXPORT_RC"
if grep -q 'compose down' "$EXPORT_LOG"; then bad "[container exists but stopped] never tears the project down"; else ok "[container exists but stopped] never tears the project down"; fi
if grep -q 'compose stop ac-database' "$EXPORT_LOG"; then ok "[container exists but stopped] stopped it again, kept the container"; else bad "[container exists but stopped] stopped it again, kept the container"; fi
if grep -Eq 'compose rm' "$EXPORT_LOG"; then bad "[container exists but stopped] did not delete a pre-existing container"; else ok "[container exists but stopped] did not delete a pre-existing container"; fi

run_export tiny_dump FAKE_DB_EXISTS=false FAKE_DB_RUNNING=false FAKE_DUMP_BYTES=100
if [ "$EXPORT_RC" != 0 ]; then ok "[dump suspiciously small] export fails"; else bad "[dump suspiciously small] export fails"; fi
if grep -Eq 'compose (stop|rm) .*ac-database' "$EXPORT_LOG"; then ok "[dump suspiciously small] STILL restored the database state"; else bad "[dump suspiciously small] STILL restored the database state"; fi

run_export never_healthy FAKE_DB_EXISTS=false FAKE_DB_RUNNING=false FAKE_DB_HEALTH=starting DML_HEALTH_TRIES=2 DML_HEALTH_SLEEP=0
assert_eq "[database never healthy] fails fast (not a 25s timeout kill)" 1 "$EXPORT_RC"
assert_eq "[database never healthy] honoured DML_HEALTH_TRIES=2" 2 "$(grep -c 'Health.Status' "$EXPORT_LOG")"
if grep -Eq 'compose (stop|rm) .*ac-database' "$EXPORT_LOG"; then ok "[database never healthy] STILL restored the database state"; else bad "[database never healthy] STILL restored the database state"; fi

section "BEHAVIOUR: export talks to the container it resolved, never to a bare name"
# The failure this catches is silent and total: with a second AzerothCore stack
# on the same engine, a bare `ac-database` answers for whichever project owns
# the name, so the export dumps the WRONG world and reports success. The fake
# hands out the id `cid-ac-database`, so a bare-name call is visible in the log.
bare_name_calls() { grep -Ec '^(exec|inspect) .* ac-database( |$)' "$1"; }

run_export ids_after_create FAKE_DB_EXISTS=false FAKE_DB_RUNNING=false
assert_eq "[created by us] export succeeds" 0 "$EXPORT_RC"
assert_eq "[created by us] no exec/inspect used the bare name" 0 "$(bare_name_calls "$EXPORT_LOG")"
if grep -Eq '^inspect .*cid-ac-database' "$EXPORT_LOG"; then ok "[created by us] health poll used the resolved id"; else bad "[created by us] health poll used the resolved id"; fi
if grep -Eq '^exec cid-ac-database mysqldump' "$EXPORT_LOG"; then ok "[created by us] mysqldump ran in the resolved container"; else bad "[created by us] mysqldump ran in the resolved container"; fi
if grep -Eq '^exec cid-ac-database mysql ' "$EXPORT_LOG"; then ok "[created by us] SHOW DATABASES ran in the resolved container"; else bad "[created by us] SHOW DATABASES ran in the resolved container"; fi

run_export ids_already_running FAKE_DB_EXISTS=true FAKE_DB_RUNNING=true
assert_eq "[already running] export succeeds" 0 "$EXPORT_RC"
assert_eq "[already running] no exec/inspect used the bare name" 0 "$(bare_name_calls "$EXPORT_LOG")"

# ...and when the id cannot be resolved at all, the export must SAY so rather
# than fall back to a name that may belong to somebody else's stack.
run_export unresolvable FAKE_DB_EXISTS=false FAKE_DB_RUNNING=false FAKE_UP_CREATES_NOTHING=1
if [ "$EXPORT_RC" != 0 ]; then ok "[id unresolvable] export fails instead of guessing"; else bad "[id unresolvable] export fails instead of guessing"; fi
assert_eq "[id unresolvable] never fell back to the bare name" 0 "$(bare_name_calls "$EXPORT_LOG")"

section "BEHAVIOUR: export output shape"
run_export shape FAKE_DB_EXISTS=false FAKE_DB_RUNNING=false
for want in db-dump.sql.gz client-data.tar etc conf/soap.env conf/docker-compose.override.yml.orig modules; do
  if [ -e "$EXPORT_OUT/$want" ]; then ok "[export output] produced $want"; else bad "[export output] produced $want"; fi
done

# ---- import -----------------------------------------------------------------

make_import_dir() { # DIR (populated with a plausible export)
  local d="$1"
  mkdir -p "$d/etc/modules" "$d/modules/mod-playerbots/.git" "$d/conf"
  printf 'x\n' > "$d/etc/worldserver.conf"
  printf 'x\n' > "$d/etc/modules/playerbots.conf"
  printf 'FAKE-TAR' > "$d/client-data.tar"
  head -c 200000 /dev/urandom | gzip > "$d/db-dump.sql.gz"
  for i in worldserver authserver db-import client-data; do
    head -c 2048 /dev/urandom | gzip > "$d/img-$i.tar.gz"
  done
  printf 'DML_SOAP_URL=http://127.0.0.1:7878/\r\nDML_SOAP_USER=dmlsoap\r\n' > "$d/conf/soap.env"
  # The fixture is the MIGRATION template — the file the import's header tells
  # a human to copy. If that file cannot pass the import's own validation, the
  # instruction is wrong, and this harness is the thing that says so.
  cp "$MIGRATED" "$d/docker-compose.yml"
  # ...and a real override: the ./modules mount plus the runtime env, exactly
  # like the working server's (poc/native-docker/README.md "the biggest
  # lesson"). An override without the mount is the recorded boot failure.
  cat > "$d/docker-compose.override.yml" <<'OVR'
services:
  ac-worldserver:
    volumes:
      - ./modules:/azerothcore/modules
    environment:
      AC_PLAYERBOTS_UPDATES_ENABLE_DATABASES: "1"
      AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN: "1"
      AC_SOAP_ENABLED: "1"
      AC_SOAP_IP: 0.0.0.0
      AC_SOAP_PORT: "7878"
OVR
}

# run_import NAME [PREP-COMMAND...] -- PREP runs against the staged export dir
# (path in $IMPORT_DIR) after it is built and before the script runs, so a
# scenario can delete a file to test a refusal.
run_import() { # NAME [PREP...]
  local name="$1"; shift
  local root="$TMPROOT/imp-$name"
  local bin="$root/bin" dir="$root/wow-server-playerbots" home="$root/home"
  mkdir -p "$root" "$home"
  make_fake_docker "$bin"
  make_import_dir "$dir"
  IMPORT_DIR="$dir"
  IMPORT_HOME="$home"
  IMPORT_LOG="$root/docker.log"
  : > "$IMPORT_LOG"
  [ "$#" -gt 0 ] && "$@"
  ( export PATH="$bin:$PATH" FAKE_DOCKER_LOG="$IMPORT_LOG" HOME="$home" \
      LOCALAPPDATA="$root/no-such-localappdata" DML_DOCKER="$bin/docker"
    timeout 25 bash "$IMPORT_SH" "$dir" > "$root/stdout.txt" 2>&1 )
  IMPORT_RC=$?
  IMPORT_STDOUT="$root/stdout.txt"
}
drop_override() { rm -f "$IMPORT_DIR/docker-compose.override.yml"; }
drop_world_image() { rm -f "$IMPORT_DIR/img-worldserver.tar.gz"; }
drop_etc_bind()   { sed -i '/env\/dist\/etc:/d' "$IMPORT_DIR/docker-compose.yml"; }
drop_logs_bind()  { sed -i '/env\/dist\/logs:\//d' "$IMPORT_DIR/docker-compose.yml"; }
drop_bot_db()     { sed -i '/AC_PLAYERBOTS_DATABASE_INFO/d' "$IMPORT_DIR/docker-compose.yml"; }
drop_mod_mount()  { sed -i '/azerothcore\/modules/d' "$IMPORT_DIR/docker-compose.override.yml"; }
use_poc_template() { cp "$TEMPLATE" "$IMPORT_DIR/docker-compose.yml"; }
# Delete the LAST ./env/dist/etc bind — the worldserver's — leaving the
# db-import and authserver ones in place. A whole-file grep still finds the
# path, so only a SERVICE-SCOPED check can tell this apart from a valid stack.
etc_bind_on_other_services_only() {
  local f="$IMPORT_DIR/docker-compose.yml"
  tac "$f" | sed '0,/env\/dist\/etc:/{/env\/dist\/etc:/d}' | tac > "$f.new" && mv "$f.new" "$f"
}

section "BEHAVIOUR: import drives the ac-* containers the compose really creates"
run_import happy
assert_eq "[import] succeeds against a complete export" 0 "$IMPORT_RC"
if grep -q 'dml-native-' "$IMPORT_LOG"; then bad "[import] never addressed a dml-native-* container"; else ok "[import] never addressed a dml-native-* container"; fi
if grep -q 'exec -i ac-database mysql' "$IMPORT_LOG"; then ok "[import] restored the dump into ac-database"; else bad "[import] restored the dump into ac-database"; fi
if [ -f "$IMPORT_DIR/env/dist/etc/worldserver.conf" ]; then ok "[import] staged etc/ into env/dist/etc"; else bad "[import] staged etc/ into env/dist/etc"; fi
if [ -d "$IMPORT_DIR/env/dist/logs" ]; then ok "[import] created the env/dist/logs bind dir"; else bad "[import] created the env/dist/logs bind dir"; fi
if [ -f "$IMPORT_HOME/.dml/soap.env" ]; then
  ok "[import] installed soap.env into the Windows home"
  if grep -q $'\r' "$IMPORT_HOME/.dml/soap.env"; then bad "[import] stripped CRs from soap.env"; else ok "[import] stripped CRs from soap.env"; fi
else
  bad "[import] installed soap.env into the Windows home"
  bad "[import] stripped CRs from soap.env (no file)"
fi

section "BEHAVIOUR: the import refuses the compose shapes that boot a wrong-but-plausible server"
# Every one of these produces a server that STARTS. That is the point: nothing
# downstream ever reports them, so the import is the only place they can be
# caught.
refuses_before_the_db_write() { # LABEL  EXPECTED-TEXT
  if [ "$IMPORT_RC" != 0 ]; then ok "$1 refuses"; else bad "$1 refuses"; fi
  if grep -q "$2" "$IMPORT_STDOUT"; then ok "$1 says what is missing"; else bad "$1 says what is missing"; fi
  if grep -Eq 'exec -i .*mysql' "$IMPORT_LOG"; then bad "$1 refuses BEFORE the database write"; else ok "$1 refuses BEFORE the database write"; fi
}

run_import no_etc_bind drop_etc_bind
refuses_before_the_db_write "[no env/dist/etc bind]" "env/dist/etc"

# A whole-file grep is not a check: the path is still in the file, just not on
# the service that needs it.
run_import etc_elsewhere etc_bind_on_other_services_only
refuses_before_the_db_write "[etc bind on other services only]" "env/dist/etc"

run_import no_bot_db drop_bot_db
refuses_before_the_db_write "[no AC_PLAYERBOTS_DATABASE_INFO]" "AC_PLAYERBOTS_DATABASE_INFO"

run_import no_mod_mount drop_mod_mount
refuses_before_the_db_write "[no ./modules mount]" "modules"

# The exact F4 case: the clean stock-image template a stale header told people
# to copy has none of the migration's binds.
run_import poc_template use_poc_template
refuses_before_the_db_write "[clean stock template copied by mistake]" "env/dist/etc"

# ...and a missing LOG bind is a real but survivable difference, so it must
# WARN rather than refuse — over-refusing a working migration is its own bug.
run_import no_logs_bind drop_logs_bind
assert_eq "[no env/dist/logs bind] still imports" 0 "$IMPORT_RC"
if grep -q 'env/dist/logs' "$IMPORT_STDOUT"; then ok "[no env/dist/logs bind] warns about it"; else bad "[no env/dist/logs bind] warns about it"; fi

run_import no_override drop_override
if [ "$IMPORT_RC" != 0 ]; then ok "[import] refuses without docker-compose.override.yml"; else bad "[import] refuses without docker-compose.override.yml"; fi
if grep -q 'docker-compose.override.yml' "$IMPORT_STDOUT"; then ok "[import] names the missing override in the refusal"; else bad "[import] names the missing override in the refusal"; fi
# NB: match ANY container name here, not just ac-database — asserting the ac-*
# spelling would pass vacuously while the script still wrote to dml-native-*.
if grep -Eq 'exec -i .*mysql' "$IMPORT_LOG"; then bad "[import] refused BEFORE writing to the database"; else ok "[import] refused BEFORE writing to the database"; fi

run_import no_image drop_world_image
if [ "$IMPORT_RC" != 0 ]; then ok "[import] refuses when an exported image tarball is missing"; else bad "[import] refuses when an exported image tarball is missing"; fi
if grep -Eq 'exec -i .*mysql' "$IMPORT_LOG"; then bad "[import] missing-image refusal happens BEFORE the database write"; else ok "[import] missing-image refusal happens BEFORE the database write"; fi

# ------------------------------------------------------------ shellcheck -----

section "shellcheck"
SHELLCHECK="${DML_SHELLCHECK:-$(command -v shellcheck || true)}"
if [ -n "$SHELLCHECK" ]; then
  for f in "$EXPORT_SH" "$IMPORT_SH" "${BASH_SOURCE[0]}"; do
    n="$(basename "$f")"
    if out="$("$SHELLCHECK" -S warning "$f" 2>&1)"; then
      ok "$n is shellcheck-clean"
    else
      bad "$n is shellcheck-clean"
      printf '%s\n' "$out" | sed 's/^/         /' | head -40
    fi
  done
elif [ "${DML_REQUIRE_SHELLCHECK:-0}" = 1 ]; then
  bad "shellcheck is required (DML_REQUIRE_SHELLCHECK=1) but not installed"
else
  skipped "shellcheck not installed - NOT a pass (set DML_SHELLCHECK=/path, or DML_REQUIRE_SHELLCHECK=1 to fail)"
fi

printf '\n%s\n' "-----------------------------------------------------------"
printf 'passed: %s   FAILED: %s   skipped: %s\n' "$PASS" "$FAIL" "$SKIP"
[ "$FAIL" = 0 ] || exit 1
exit 0
