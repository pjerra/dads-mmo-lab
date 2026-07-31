# Shared test harness: fixture games dir + docker stub.
make_fixture() {
  FIXTURE="$(mktemp -d)"
  export DML_GAMES_DIR="$FIXTURE/games"
  mkdir -p "$DML_GAMES_DIR"
  # Where the sequencing stubs keep their "which reply is next" counters.
  #
  # This is EXPORTED because the stubs are separate processes that cannot see
  # $FIXTURE, and it exists because the previous default was
  # `/tmp/dml_<kind>_seq.$$` -- shared /tmp, keyed by the STUB's own pid. Two
  # ways that bites, both producing the intermittent-and-unreproducible failure
  # that is the worst kind to own:
  #
  #   * $$ is a NEW pid on every stub invocation, so a counter keyed by it never
  #     advances -- the sequence silently replays its first entry forever, and
  #     the test proves nothing while passing.
  #   * pids are RECYCLED. A leftover /tmp/dml_curl_seq.4242 from any earlier run
  #     is read as this run's progress the moment a stub happens to be pid 4242.
  #
  # Every test today sets its own *_SEQ_STATE into the fixture, so the trap is
  # currently unreached -- which is exactly why it needed closing now rather
  # than after someone forgot. Correctness should not depend on each test author
  # remembering an opt-in.
  export DML_STUB_STATE_DIR="$FIXTURE/stub-state"
  mkdir -p "$DML_STUB_STATE_DIR"
}

add_game() {  # add_game <id> compose|install|empty|nested
  local id="$1" kind="$2" dir="$DML_GAMES_DIR/$1"
  mkdir -p "$dir"
  case "$kind" in
    compose) touch "$dir/docker-compose.yml" ;;
    install) touch "$dir/install.sh" ;;
    nested)  mkdir -p "$dir/sub" && touch "$dir/sub/compose.yml" ;;
    empty)   : ;;
  esac
}

use_docker_stub() {
  STUB_BIN="$FIXTURE/bin"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/docker" <<'EOS'
#!/usr/bin/env bash
# Optional argv capture (backup-stub convention): one line per docker call,
# so tests can assert exactly which commands ran (world-restart/keep-data).
[[ -n "${DML_STUB_CALL_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_CALL_LOG"
if [[ "${1:-}" == "info" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1 || exit 0
fi
if [[ "${1:-}" == "restart" ]]; then
  # world-restart (Batch 3 F11f): `docker restart -t 300 ac-worldserver`.
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  exit "${DML_STUB_RESTART_EXIT:-0}"
fi
if [[ "${1:-}" == "volume" ]]; then
  # games remove keep-data (Batch 3 F13c): `docker volume rm <name>`.
  if [[ "${2:-}" == "rm" && "${DML_STUB_VOLUME_RM_EXIT:-0}" != 0 ]]; then
    echo "stub volume rm failure (in use)" >&2
    exit "${DML_STUB_VOLUME_RM_EXIT}"
  fi
  exit 0
fi
if [[ "${1:-}" == "compose" ]]; then
  # find -f <file>
  file=""
  args=("$@")
  for i in "${!args[@]}"; do
    [[ "${args[$i]}" == "-f" ]] && file="${args[$((i+1))]}"
  done
  rest="${args[*]}"
  if [[ "$rest" == *"ps --status running -q"* ]]; then
    if [[ -n "$file" ]] && grep -qxF "$file" <<< "${DML_STUB_RUNNING:-}"; then
      echo "stub-container-id"
    fi
    exit 0
  fi
  if [[ "$rest" == *"ps -a -q"* ]]; then
    # Log-snapshot container resolution: `docker compose -f <file> ps -a -q
    # ac-worldserver` asks THIS title's compose project whether it owns a
    # worldserver container. DML_STUB_COMPOSE_PS_ID is the id it answers with
    # (default: present); set it EMPTY to model a project that has no such
    # service/container -- the case that must take no snapshot at all even
    # while some other project's ac-worldserver is alive.
    # DML_STUB_COMPOSE_PS_EXIT models compose failing outright ("no such
    # service"), which real compose reports as a nonzero exit.
    ps_id="${DML_STUB_COMPOSE_PS_ID-stub-world-cid}"
    [[ -n "$ps_id" ]] && printf '%s\n' "$ps_id"
    exit "${DML_STUB_COMPOSE_PS_EXIT:-0}"
  fi
  if [[ "$rest" == *"up -d"* || "$rest" == *"down"* ]]; then
    echo "stub compose: $rest"
    exit "${DML_STUB_COMPOSE_EXIT:-0}"
  fi
  exit 0
fi
if [[ "${1:-}" == "ps" ]]; then
  # server-detail: `docker ps -a --format '{{.Names}}|{{.State}}|{{.Status}}'`
  # -> canned rows from DML_STUB_PS_ROWS (a file). Daemon-down => exit 1
  # with no output, like real docker.
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  [[ -n "${DML_STUB_PS_ROWS:-}" && -f "${DML_STUB_PS_ROWS}" ]] && cat "$DML_STUB_PS_ROWS"
  exit 0
fi
if [[ "${1:-}" == "inspect" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  # world-restart running precondition (Batch 1): `docker inspect -f
  # '{{.State.Running}}' <container>`. DML_STUB_RUNNING_STATE drives it
  # (default true so the happy path sees an up stack); set it to false to
  # model a stopped stack -> NOT_RUNNING with no long readiness wait.
  # DML_STUB_RUNNING_STATE_WORLD / _DB override per container: the precondition
  # probes ac-worldserver and ac-database separately and treats them
  # differently (a down world with a healthy DB is a legitimate recovery
  # restart), which one shared variable cannot model.
  if [[ "$*" == *State.Running* ]]; then
    run_state="${DML_STUB_RUNNING_STATE:-true}"
    case "$*" in
      *ac-worldserver*) run_state="${DML_STUB_RUNNING_STATE_WORLD:-$run_state}" ;;
      *ac-database*)    run_state="${DML_STUB_RUNNING_STATE_DB:-$run_state}" ;;
    esac
    printf '%s\n' "$run_state"
    exit 0
  fi
  # Boot-loop detection (incident follow-up 2): `docker inspect -f
  # '{{.State.RestartCount}}'`. DML_STUB_RESTART_COUNT_SEQ is a space-separated
  # list consumed one per call (sticky on the last; state file in
  # DML_STUB_RESTART_COUNT_SEQ_STATE -- same convention as DML_STUB_DB_ROWS_SEQ)
  # so a test can make the count CLIMB across readiness polls;
  # DML_STUB_RESTART_COUNT is the constant fallback. Set either to an empty or
  # non-numeric value to model docker failing to answer, which must never be
  # read as evidence of anything.
  if [[ "$*" == *RestartCount* ]]; then
    if [[ -n "${DML_STUB_RESTART_COUNT_SEQ:-}" ]]; then
      st="${DML_STUB_RESTART_COUNT_SEQ_STATE:-${DML_STUB_STATE_DIR:?stub state dir unset -- call make_fixture}/rc_seq}"
      i=0; [[ -f "$st" ]] && i="$(cat "$st")"
      vals=($DML_STUB_RESTART_COUNT_SEQ)
      idx=$i; (( idx >= ${#vals[@]} )) && idx=$(( ${#vals[@]} - 1 ))
      printf '%s\n' "${vals[$idx]}"
      echo $(( i + 1 )) > "$st"
    else
      printf '%s\n' "${DML_STUB_RESTART_COUNT-0}"
    fi
    exit 0
  fi
  # server-detail crashed-vs-stopped (Batch 2 F8): the ExitCode format string
  # is served from DML_STUB_EXIT_CODE (default 0 = clean exit); the StartedAt
  # form keeps its canned timestamp for the world-ready checks.
  if [[ "$*" == *ExitCode* ]]; then
    printf '%s\n' "${DML_STUB_EXIT_CODE:-0}"
    exit 0
  fi
  # config set's frozen-env check (_cfg_env_frozen): the RUNNING container's
  # creation-time environment, one NAME=value per line. DML_STUB_CONTAINER_ENV
  # is a newline list; unset = no frozen env, which is the common case.
  if [[ "$*" == *Config.Env* ]]; then
    [[ -n "${DML_STUB_CONTAINER_ENV:-}" ]] && printf '%s\n' "${DML_STUB_CONTAINER_ENV}"
    exit 0
  fi
  # module client-patch (Batch 5 F2): the data-volume resolution inspects
  # the worldserver's Mounts -- serve DML_STUB_MOUNT_VOLUME (empty when
  # unset, which exercises the caller's fallback-name path).
  if [[ "$*" == *Mounts* ]]; then
    printf '%s\n' "${DML_STUB_MOUNT_VOLUME:-}"
    exit "${DML_STUB_INSPECT_EXIT:-0}"
  fi
  printf '%s\n' "${DML_STUB_STARTED_AT:-2026-07-17T10:00:00.000000000Z}"
  exit 0
fi
if [[ "${1:-}" == "logs" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  [[ -n "${DML_STUB_LOGS_ARGS_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_LOGS_ARGS_LOG"
  # DML_STUB_LOGS_HANG=<secs>: a read that never comes back (dockerd wedged on
  # a socket-activated start). `exec` so the sleep REPLACES this shell -- a
  # `sleep` left as a child would outlive the TERM `timeout(1)` sends and keep
  # the caller's pipe open, which is precisely the hang being modelled.
  [[ -n "${DML_STUB_LOGS_HANG:-}" ]] && exec sleep "${DML_STUB_LOGS_HANG}"
  # DML_STUB_LOGS_EXIT models `Error: No such container` (real docker exits 1
  # and prints to stderr). Distinct from "answered with an empty log": the log
  # snapshot treats the former as nothing-to-preserve and the latter the same,
  # but only the exit status can tell a caller which happened.
  if [[ "${DML_STUB_LOGS_EXIT:-0}" != 0 ]]; then
    echo "Error: No such container: ${*: -1}" >&2
    exit "${DML_STUB_LOGS_EXIT}"
  fi
  # The REAL --since filtering is docker's job, so the stub emulates it:
  # when the caller passed --since and DML_STUB_LOGS_SINCE_FILE is set,
  # serve that file (the "current run only" view); otherwise serve the
  # full log. The stale-marker test relies on the two views differing.
  if [[ "$*" == *"--since"* && -n "${DML_STUB_LOGS_SINCE_FILE:-}" ]]; then
    cat "$DML_STUB_LOGS_SINCE_FILE"
  elif [[ -n "${DML_STUB_LOGS_FILE:-}" && -f "${DML_STUB_LOGS_FILE}" ]]; then
    cat "$DML_STUB_LOGS_FILE"
  fi
  exit 0
fi
if [[ "${1:-}" == "port" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  # `docker port <name> <internal>` -> DML_STUB_PORTS is a newline table of
  # "<container> <internal> <hostport>"; matching row prints "0.0.0.0:<hostport>".
  # The <hostport> field may also be a full "ip:port" (e.g. 127.0.0.1:7878) so
  # the port-check diagnostic can exercise loopback-vs-0.0.0.0 detection --
  # bare ports keep the historical 0.0.0.0 default (server-detail relies on it).
  if [[ -n "${DML_STUB_PORTS:-}" ]]; then
    while read -r c i h; do
      [[ "$c" == "${2:-}" && "$i" == "${3:-}" ]] || continue
      case "$h" in *:*) echo "$h" ;; *) echo "0.0.0.0:$h" ;; esac
    done <<< "$DML_STUB_PORTS"
  fi
  exit 0
fi
if [[ "${1:-}" == "exec" ]]; then
  # server-detail's bots block: `docker exec -i ac-database mysql ...`. Same
  # env-var-driven canned-output convention as use_mysql_stub's exec arm
  # below (DML_STUB_DB_ROWS[_SEQ]/DML_STUB_DB_EXIT/DML_STUB_DB_QUERY_LOG),
  # kept here too so server-detail tests can stub mysql without losing the
  # ps/inspect/logs/port arms above (use_mysql_stub replaces this whole file).
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  [[ -n "${DML_STUB_DB_QUERY_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_DB_QUERY_LOG"
  if [[ -n "${DML_STUB_DB_ROWS_SEQ:-}" ]]; then
    st="${DML_STUB_DB_SEQ_STATE:-${DML_STUB_STATE_DIR:?stub state dir unset -- call make_fixture}/db_seq}"
    i=0; [[ -f "$st" ]] && i="$(cat "$st")"
    files=($DML_STUB_DB_ROWS_SEQ)
    idx=$i; (( idx >= ${#files[@]} )) && idx=$(( ${#files[@]} - 1 ))
    [[ -f "${files[$idx]}" ]] && cat "${files[$idx]}"
    echo $(( i + 1 )) > "$st"
    # DML_STUB_DB_EXIT_SEQ: optional space-separated exit codes parallel to
    # ROWS_SEQ (clamped to last) -- lets one query fail and the next succeed
    # (the paperdoll schema-fallback tests need exactly that).
    if [[ -n "${DML_STUB_DB_EXIT_SEQ:-}" ]]; then
      exits=($DML_STUB_DB_EXIT_SEQ)
      eidx=$i; (( eidx >= ${#exits[@]} )) && eidx=$(( ${#exits[@]} - 1 ))
      exit "${exits[$eidx]}"
    fi
    exit "${DML_STUB_DB_EXIT:-0}"
  fi
  [[ -n "${DML_STUB_DB_ROWS:-}" ]] && cat "$DML_STUB_DB_ROWS"
  exit "${DML_STUB_DB_EXIT:-0}"
fi
exit 0
EOS
  chmod +x "$STUB_BIN/docker"

  # _check_port_conflicts (90-main.sh) shells out to `ss` to scan the real
  # host's listening ports. Stub it so port-conflict warnings are always
  # empty/deterministic in tests instead of depending on whatever is actually
  # listening on the machine running the suite.
  cat > "$STUB_BIN/ss" <<'EOS'
#!/usr/bin/env bash
exit 0
EOS
  chmod +x "$STUB_BIN/ss"

  export PATH="$STUB_BIN:$PATH"
}

teardown_fixture() {
  [[ -n "${FIXTURE:-}" ]] && rm -rf "$FIXTURE"
}

use_curl_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/curl" <<'EOS'
#!/usr/bin/env bash
# Canned responder. Legacy mode (SOAP): emit DML_STUB_SOAP_RESPONSE then
# "\n<code>". -o mode (item-info): write the body to the -o target, print a
# bare code. DML_STUB_CURL_SEQ = space-sep response files consumed one per
# call (sticky last; state file in DML_STUB_CURL_SEQ_STATE);
# DML_STUB_HTTP_SEQ = matching space-sep http codes (sticky last).
# DML_STUB_CURL_LOG captures argv per call.
outfile=""
args=("$@")
for i in "${!args[@]}"; do
  [[ "${args[$i]}" == "-o" ]] && outfile="${args[$((i+1))]}"
done
resp="${DML_STUB_SOAP_RESPONSE:-}"
code="${DML_STUB_HTTP:-200}"
if [[ -n "${DML_STUB_CURL_SEQ:-}" ]]; then
  st="${DML_STUB_CURL_SEQ_STATE:-${DML_STUB_STATE_DIR:?stub state dir unset -- call make_fixture}/curl_seq}"
  i=0; [[ -f "$st" ]] && i="$(cat "$st")"
  files=($DML_STUB_CURL_SEQ)
  idx=$i; (( idx >= ${#files[@]} )) && idx=$(( ${#files[@]} - 1 ))
  resp="${files[$idx]}"
  if [[ -n "${DML_STUB_HTTP_SEQ:-}" ]]; then
    codes=($DML_STUB_HTTP_SEQ)
    cidx=$i; (( cidx >= ${#codes[@]} )) && cidx=$(( ${#codes[@]} - 1 ))
    code="${codes[$cidx]}"
  fi
  echo $(( i + 1 )) > "$st"
fi
[[ -n "${DML_STUB_CURL_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_CURL_LOG"
if [[ -n "${DML_STUB_CAPTURE_APPEND:-}" ]]; then
  cat >> "$DML_STUB_CAPTURE_APPEND"
elif [[ -n "${DML_STUB_CAPTURE:-}" ]]; then
  cat > "$DML_STUB_CAPTURE"
else
  cat >/dev/null
fi
if [[ -n "$outfile" ]]; then
  if [[ -n "$resp" && -f "$resp" ]]; then cat "$resp" > "$outfile"; else : > "$outfile"; fi
  printf '%s' "$code"
else
  [[ -n "$resp" && -f "$resp" ]] && cat "$resp"
  printf '\n%s' "$code"
fi
exit "${DML_STUB_CURL_EXIT:-0}"
EOS
  chmod +x "$STUB_BIN/curl"
  export PATH="$STUB_BIN:$PATH"
}

use_mysql_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/docker" <<'EOS'
#!/usr/bin/env bash
# Minimal docker stub for `docker exec ac-database mysql …`.
if [[ "${1:-}" == "exec" ]]; then
  [[ -n "${DML_STUB_DB_QUERY_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_DB_QUERY_LOG"
  if [[ -n "${DML_STUB_DB_ROWS_SEQ:-}" ]]; then
    # DML_STUB_DB_ROWS_SEQ = space-separated list of row-files; return the
    # next one per call, then stick on the last. State in $DML_STUB_DB_SEQ_STATE.
    st="${DML_STUB_DB_SEQ_STATE:-${DML_STUB_STATE_DIR:?stub state dir unset -- call make_fixture}/db_seq}"
    i=0; [[ -f "$st" ]] && i="$(cat "$st")"
    files=($DML_STUB_DB_ROWS_SEQ)
    idx=$i; (( idx >= ${#files[@]} )) && idx=$(( ${#files[@]} - 1 ))
    [[ -f "${files[$idx]}" ]] && cat "${files[$idx]}"
    echo $(( i + 1 )) > "$st"
    # Per-call exit codes (parallel to ROWS_SEQ, clamped) -- same seam as the
    # combined docker stub's exec arm above.
    if [[ -n "${DML_STUB_DB_EXIT_SEQ:-}" ]]; then
      exits=($DML_STUB_DB_EXIT_SEQ)
      eidx=$i; (( eidx >= ${#exits[@]} )) && eidx=$(( ${#exits[@]} - 1 ))
      exit "${exits[$eidx]}"
    fi
    exit "${DML_STUB_DB_EXIT:-0}"
  fi
  [[ -n "${DML_STUB_DB_ROWS:-}" ]] && cat "$DML_STUB_DB_ROWS"
  exit "${DML_STUB_DB_EXIT:-0}"
fi
if [[ "${1:-}" == "info" ]]; then exit 0; fi
exit 0
EOS
  chmod +x "$STUB_BIN/docker"
  export PATH="$STUB_BIN:$PATH"
}

use_backup_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/docker" <<'EOS'
#!/usr/bin/env bash
# Backup-suite docker stub: mysqldump / mysql import / compose stop+start.
# Appends one line per call to DML_STUB_CALL_LOG so tests can assert ORDER.
log() { [[ -n "${DML_STUB_CALL_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_CALL_LOG"; return 0; }
if [[ "${1:-}" == "info" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1 || exit 0
fi
if [[ "${1:-}" == "exec" ]]; then
  args="$*"
  if [[ "$args" == *mysqldump* ]]; then
    if [[ "$args" != *-uroot* || "$args" != *-p* ]]; then
      echo "Access denied for user 'root'@'localhost' (using password: NO)" >&2
      exit 1
    fi
    log "mysqldump ${args#*mysqldump }"
    if [[ "${DML_STUB_DUMP_EXIT:-0}" != 0 ]]; then echo "dump boom" >&2; exit "${DML_STUB_DUMP_EXIT}"; fi
    printf 'SQL DUMP CONTENT\n'
    exit 0
  fi
  # sql-mod family: `docker exec ac-database mysql -uroot -p... <db> -e <stmt>`
  # (tweak_world multipliers, hearthstone/teleporter reversal statements).
  # Checked BEFORE the import branch below -- order is load-bearing, since
  # the import branch's `*" mysql"*` glob would otherwise swallow -e calls too.
  if [[ "$args" == *" -e "* ]]; then
    log "mysql-stmt ${args#*-e }"
    exit "${DML_STUB_SQL_EXIT:-0}"
  fi
  # NB: checked AFTER mysqldump (which exits above), so this only matches the import.
  if [[ "$args" == *" mysql"* ]]; then
    if [[ "$args" != *-uroot* || "$args" != *-p* ]]; then
      echo "Access denied for user 'root'@'localhost' (using password: NO)" >&2
      exit 1
    fi
    log "mysql-import"
    if [[ -n "${DML_STUB_IMPORT_CAPTURE:-}" ]]; then cat > "$DML_STUB_IMPORT_CAPTURE"; else cat > /dev/null; fi
    exit "${DML_STUB_SQL_EXIT:-${DML_STUB_IMPORT_EXIT:-0}}"
  fi
  exit 0
fi
if [[ "${1:-}" == "compose" ]]; then
  shift
  log "compose $*"
  exit "${DML_STUB_COMPOSE_EXIT:-0}"
fi
# bots-flush readiness (_world_ready): inspect serves a canned StartedAt,
# logs serves DML_STUB_LOGS_FILE (same conventions as use_docker_stub above;
# --since filtering is docker's job, the canned file already IS the
# current-run view).
if [[ "${1:-}" == "inspect" ]]; then
  printf '%s\n' "${DML_STUB_STARTED_AT:-2026-07-17T10:00:00.000000000Z}"
  exit 0
fi
if [[ "${1:-}" == "logs" ]]; then
  [[ -n "${DML_STUB_LOGS_FILE:-}" && -f "${DML_STUB_LOGS_FILE}" ]] && cat "${DML_STUB_LOGS_FILE}"
  exit 0
fi
# docker-clean seams (`wow docker-usage`/`wow docker-clean`): builder/image/
# system prune-style commands share one shape -- log argv, emit canned
# output from DML_STUB_DOCKER_OUT (a file, same convention as
# DML_STUB_PS_ROWS/DML_STUB_LOGS_FILE elsewhere), fail when
# DML_STUB_DOCKER_FAIL_ARM matches the arm name.
if [[ "${1:-}" == "builder" || "${1:-}" == "image" || "${1:-}" == "system" ]]; then
  arm="$1"
  log "$*"
  if [[ "${DML_STUB_DOCKER_FAIL_ARM:-}" == "$arm" ]]; then
    echo "stub $arm failure" >&2
    exit "${DML_STUB_DOCKER_FAIL_EXIT:-1}"
  fi
  if [[ -n "${DML_STUB_DOCKER_OUT:-}" && -f "${DML_STUB_DOCKER_OUT}" ]]; then
    cat "${DML_STUB_DOCKER_OUT}"
  fi
  exit 0
fi
# `volume ls --format ...` lists DML_STUB_VOLUME_NAMES (newline-separated,
# may include non-matching noise so tests can prove the caller's grep
# filters correctly); `volume rm <name>` fails when DML_STUB_DOCKER_FAIL_ARM
# is "volume" (models a volume still in use -- the in-use warn path).
if [[ "${1:-}" == "volume" ]]; then
  sub="${2:-}"
  log "$*"
  if [[ "$sub" == "ls" ]]; then
    [[ -n "${DML_STUB_VOLUME_NAMES:-}" ]] && printf '%s\n' "${DML_STUB_VOLUME_NAMES}"
    exit 0
  fi
  if [[ "$sub" == "rm" ]]; then
    if [[ "${DML_STUB_DOCKER_FAIL_ARM:-}" == "volume" ]]; then
      echo "stub volume rm failure (in use)" >&2
      exit "${DML_STUB_DOCKER_FAIL_EXIT:-1}"
    fi
    exit 0
  fi
  exit 0
fi
exit 0
EOS
  chmod +x "$STUB_BIN/docker"
  export PATH="$STUB_BIN:$PATH"
}

use_git_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/git" <<'EOS'
#!/usr/bin/env bash
# git stub: logs argv; `clone` creates <dest>/.git so installed-checks pass.
# Extended (Round L, server-update) with seams for `wow update-check`/
# `wow update`: DML_STUB_GIT_URL (remote get-url origin), DML_STUB_GIT_BRANCH
# (rev-parse --abbrev-ref HEAD), DML_STUB_GIT_HEAD_SEQ (space-sep rev-parse
# --short HEAD outputs, consumed in order via a state file in
# DML_STUB_GIT_HEAD_SEQ_STATE, sticky on the last entry once exhausted --
# same convention as DML_STUB_CURL_SEQ/DML_STUB_DB_ROWS_SEQ above),
# DML_STUB_GIT_DIRTY (status --porcelain output; `diff` echoes it too, into
# whatever the caller redirected stdout to, so a patch file ends up
# non-empty), DML_STUB_GIT_PULL_EXIT / DML_STUB_GIT_STASH_POP_EXIT (fail
# those two ops specifically, independent of the blanket DML_STUB_GIT_EXIT
# below), DML_STUB_GIT_BEHIND (rev-list --count). `checkout`/`reset`/
# `stash push`/`fetch`/`remote add` just log+exit 0 via the fallthrough at
# the bottom -- callers only care that they ran (see DML_STUB_GIT_LOG).
[[ -n "${DML_STUB_GIT_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_GIT_LOG"
if [[ "${DML_STUB_GIT_EXIT:-0}" != 0 ]]; then
  echo "fatal: stub git failure" >&2
  exit "${DML_STUB_GIT_EXIT}"
fi
if [[ "${1:-}" == "init" ]]; then
  mkdir -p .git/info
  exit 0
fi
if [[ "${1:-}" == "clone" ]]; then
  dest="${!#}"
  mkdir -p "$dest/.git"
  exit 0
fi
if [[ "${1:-}" == "remote" && "${2:-}" == "get-url" ]]; then
  printf '%s\n' "${DML_STUB_GIT_URL:-https://github.com/mod-playerbots/azerothcore-wotlk.git}"
  exit 0
fi
if [[ "${1:-}" == "rev-parse" && "${2:-}" == "--abbrev-ref" ]]; then
  printf '%s\n' "${DML_STUB_GIT_BRANCH:-Playerbot}"
  exit 0
fi
if [[ "${1:-}" == "rev-parse" && "${2:-}" == "--short" ]]; then
  if [[ -n "${DML_STUB_GIT_HEAD_SEQ:-}" ]]; then
    st="${DML_STUB_GIT_HEAD_SEQ_STATE:-${DML_STUB_STATE_DIR:?stub state dir unset -- call make_fixture}/git_head_seq}"
    i=0; [[ -f "$st" ]] && i="$(cat "$st")"
    shas=($DML_STUB_GIT_HEAD_SEQ)
    idx=$i; (( idx >= ${#shas[@]} )) && idx=$(( ${#shas[@]} - 1 ))
    printf '%s\n' "${shas[$idx]}"
    echo $(( i + 1 )) > "$st"
  else
    printf '%s\n' "${DML_STUB_GIT_HEAD:-abc1234}"
  fi
  exit 0
fi
if [[ "${1:-}" == "status" ]]; then
  [[ -n "${DML_STUB_GIT_DIRTY:-}" ]] && printf '%s\n' "${DML_STUB_GIT_DIRTY}"
  exit 0
fi
if [[ "${1:-}" == "diff" ]]; then
  if [[ -n "${DML_STUB_GIT_DIRTY:-}" ]]; then
    printf 'diff --git a/stub b/stub\n%s\n' "${DML_STUB_GIT_DIRTY}"
  fi
  exit 0
fi
if [[ "${1:-}" == "stash" && "${2:-}" == "pop" ]]; then
  if [[ "${DML_STUB_GIT_STASH_POP_EXIT:-0}" != 0 ]]; then
    echo "fatal: stub stash pop conflict" >&2
    exit "${DML_STUB_GIT_STASH_POP_EXIT}"
  fi
  exit 0
fi
if [[ "${1:-}" == "pull" ]]; then
  if [[ "${DML_STUB_GIT_PULL_EXIT:-0}" != 0 ]]; then
    echo "fatal: stub pull failure" >&2
    exit "${DML_STUB_GIT_PULL_EXIT}"
  fi
  echo "Already up to date."
  exit 0
fi
if [[ "${1:-}" == "rev-list" && "${2:-}" == "--count" ]]; then
  printf '%s\n' "${DML_STUB_GIT_BEHIND:-0}"
  exit 0
fi
exit 0
EOS
  chmod +x "$STUB_BIN/git"
  export PATH="$STUB_BIN:$PATH"
}

# Batch 5 (overnight): Tailscale Play Together. Stubs the whole privileged
# tool-chain the `wow tailscale` arm shells out to -- tailscale (client),
# sudo (transparent pass-through minus its -n), pacman, systemctl, iptables.
# The arm reads the tailscale binary via the DML_TS_BIN seam, so a test points
# it at $STUB_BIN/tailscale for present/connected cases and at a bogus name to
# exercise the not-installed path. Behaviour knobs:
#   DML_STUB_TS_CONNECTED=1   `tailscale up` succeeds silently (already authed)
#   DML_STUB_TS_IP=100.a.b.c  `tailscale ip -4` prints this (a tailnet IP)
#   DML_STUB_TS_UP_URL        auth URL `tailscale up` prints when NOT connected
#   DML_STUB_TS_STATE=Running BackendState in `tailscale status --json`
#   DML_STUB_TS_DOWN_EXIT     exit code of `tailscale down` (default 0)
#   DML_STUB_SUDO_FAIL=1      sudo -n fails like a missing NOPASSWD rule
#   DML_STUB_PACMAN_EXIT      exit code of `pacman -S ...` (default 0)
#   DML_STUB_IPTABLES_C_EXIT  exit of `iptables -C` (default 1 = rule absent)
#   DML_STUB_TS_CALL_LOG      append one line per tailscale call for assertions
use_tailscale_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"

  cat > "$STUB_BIN/sudo" <<'EOS'
#!/usr/bin/env bash
# Transparent sudo: drop leading option flags (e.g. -n), then exec the rest.
# DML_STUB_SUDO_FAIL models a box without passwordless sudo.
if [[ "${DML_STUB_SUDO_FAIL:-0}" == 1 ]]; then
  echo "sudo: a password is required" >&2
  exit 1
fi
while [[ "${1:-}" == -* ]]; do shift; done
exec "$@"
EOS
  chmod +x "$STUB_BIN/sudo"

  cat > "$STUB_BIN/tailscale" <<'EOS'
#!/usr/bin/env bash
[[ -n "${DML_STUB_TS_CALL_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_TS_CALL_LOG"
case "${1:-}" in
  up)
    if [[ "${DML_STUB_TS_CONNECTED:-0}" == 1 ]]; then
      exit 0
    fi
    # `-`, NOT `:-`: an explicitly EMPTY DML_STUB_TS_UP_URL must mean "up printed
    # no URL at all" (the live case where the control plane had not answered
    # yet). With `:-` an empty value silently fell back to the default URL, so a
    # test for the no-URL path proved nothing.
    printf 'To authenticate, visit:\n\n\t%s\n\n' "${DML_STUB_TS_UP_URL-https://login.tailscale.com/a/0123456789abcdef}"
    exit 1
    ;;
  ip)
    [[ -n "${DML_STUB_TS_IP:-}" ]] && printf '%s\n' "${DML_STUB_TS_IP}"
    exit 0
    ;;
  status)
    # DML_STUB_TS_STATUS_EXIT models a daemon that is not answering at all --
    # which is what the `up` arm's daemon precondition checks before it spends
    # the whole login timeout discovering the same thing.
    if [[ "${DML_STUB_TS_STATUS_EXIT:-0}" != 0 ]]; then
      echo "failed to connect to local tailscaled" >&2
      exit "${DML_STUB_TS_STATUS_EXIT}"
    fi
    if [[ "${2:-}" == "--json" ]]; then
      # DML_STUB_TS_AUTH_URL models tailscaled HOLDING a pending login URL --
      # the state that exists after `tailscale up` has already given up waiting
      # for it (measured live: the control plane answered 30s in, `up` waited 8).
      printf '{"BackendState":"%s","AuthURL":"%s","Self":{"TailscaleIPs":["%s"]}}\n' \
        "${DML_STUB_TS_STATE:-Running}" "${DML_STUB_TS_AUTH_URL:-}" "${DML_STUB_TS_IP:-100.64.0.1}"
      exit 0
    fi
    printf '%s   dml-host   linux   -\n' "${DML_STUB_TS_IP:-100.64.0.1}"
    exit 0
    ;;
  down)
    exit "${DML_STUB_TS_DOWN_EXIT:-0}"
    ;;
  *)
    exit 0
    ;;
esac
EOS
  chmod +x "$STUB_BIN/tailscale"

  cat > "$STUB_BIN/pacman" <<'EOS'
#!/usr/bin/env bash
exit "${DML_STUB_PACMAN_EXIT:-0}"
EOS
  chmod +x "$STUB_BIN/pacman"

  cat > "$STUB_BIN/systemctl" <<'EOS'
#!/usr/bin/env bash
# is-system-running -> "running"; enable/anything else -> success.
# DML_STUB_SYSTEMCTL_ENABLE_EXIT models `enable --now tailscaled` FAILING (no
# such unit, masked, or the daemon dying on start). The up arm must report that
# cause rather than swallowing it and blaming a login timeout.
[[ "${1:-}" == "is-system-running" ]] && { echo running; exit 0; }
if [[ "${1:-}" == "enable" && "${DML_STUB_SYSTEMCTL_ENABLE_EXIT:-0}" != 0 ]]; then
  echo "Failed to enable unit: Unit tailscaled.service not found." >&2
  exit "${DML_STUB_SYSTEMCTL_ENABLE_EXIT}"
fi
exit 0
EOS
  chmod +x "$STUB_BIN/systemctl"

  cat > "$STUB_BIN/iptables" <<'EOS'
#!/usr/bin/env bash
# -C (check) -> DML_STUB_IPTABLES_C_EXIT (default 1 = rule absent so -I runs);
# -I (insert) and everything else -> success.
[[ "${1:-}" == "-C" ]] && exit "${DML_STUB_IPTABLES_C_EXIT:-1}"
exit 0
EOS
  chmod +x "$STUB_BIN/iptables"

  export PATH="$STUB_BIN:$PATH"
  export DML_TS_BIN="$STUB_BIN/tailscale"
}

# Incident follow-up 1 (2026-07-21): `wow docker-restart`. Stubs the three
# tools that arm shells out to -- sudo (transparent pass-through minus its
# flags), systemctl (the systemd probe AND the restart itself), and docker
# (only `info`, which is the readiness poll). Deliberately a dedicated stub
# rather than use_docker_stub + use_tailscale_stub: this arm is the ONLY one
# whose docker stub must be able to answer differently on successive `info`
# calls, and it needs none of the tailscale tool-chain. Behaviour knobs:
#   DML_STUB_SUDO_FAIL=1             sudo fails like a missing NOPASSWD rule
#   DML_STUB_SUDO_LOG                append one line of sudo's argv per call
#                                    (how the suite proves -n is always passed)
#   DML_STUB_SYSTEMD_STATE           what `systemctl is-system-running` prints
#                                    (default running; e.g. offline = no systemd)
#   DML_STUB_SYSTEMCTL_LOG           append one line of systemctl's argv per call
#   DML_STUB_SYSTEMCTL_RESTART_EXIT  exit code of `systemctl restart docker`
#   DML_STUB_SYSTEMCTL_RESTART_HANG  seconds `systemctl restart docker` blocks
#                                    without ever returning (Type=notify +
#                                    TimeoutStartSec=0 = the wedged daemon)
#   DML_STUB_DOCKER_INFO_HANG        seconds each `docker info` blocks without
#                                    answering (socket accepted, daemon mute)
#   DML_STUB_DOCKER_DOWN=1           `docker info` always fails (never comes back)
#   DML_STUB_DOCKER_INFO_SEQ         space-separated exit codes consumed one per
#                                    `docker info` call, sticky on the last
#                                    (state file in ..._SEQ_STATE) -- the same
#                                    seq convention as DML_STUB_RESTART_COUNT_SEQ
#                                    above. Lets a test make the daemon come back
#                                    only after N polls, which is the only way to
#                                    prove the arm actually WAITED.
use_docker_restart_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"

  cat > "$STUB_BIN/sudo" <<'EOS'
#!/usr/bin/env bash
[[ -n "${DML_STUB_SUDO_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_SUDO_LOG"
if [[ "${DML_STUB_SUDO_FAIL:-0}" == 1 ]]; then
  echo "sudo: a password is required" >&2
  exit 1
fi
while [[ "${1:-}" == -* ]]; do shift; done
exec "$@"
EOS
  chmod +x "$STUB_BIN/sudo"

  cat > "$STUB_BIN/systemctl" <<'EOS'
#!/usr/bin/env bash
[[ -n "${DML_STUB_SYSTEMCTL_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_SYSTEMCTL_LOG"
if [[ "${1:-}" == "is-system-running" ]]; then
  state="${DML_STUB_SYSTEMD_STATE:-running}"
  printf '%s\n' "$state"
  # Real systemd exits NONZERO for every state except "running" -- "degraded"
  # (some unrelated unit failed) is the normal state inside a WSL distro, so a
  # caller that reads the exit code instead of the printed state would refuse
  # on most real boxes. The stub reproduces that exactly.
  [[ "$state" == "running" ]] && exit 0
  exit 1
fi
if [[ "${1:-}" == "restart" ]]; then
  # DML_STUB_SYSTEMCTL_RESTART_HANG=<secs>: docker.service is Type=notify with
  # TimeoutStartSec=0, so a dockerd wedged during startup leaves `systemctl
  # restart docker` waiting for READY=1 forever -- the exact wedged-daemon case
  # this arm exists for. `exec` so the sleep REPLACES this shell: a `sleep`
  # left as a child would survive timeout(1)'s TERM and keep the caller's
  # command-substitution pipe open, hiding the bound under test.
  [[ -n "${DML_STUB_SYSTEMCTL_RESTART_HANG:-}" ]] && exec sleep "${DML_STUB_SYSTEMCTL_RESTART_HANG}"
  rc="${DML_STUB_SYSTEMCTL_RESTART_EXIT:-0}"
  if [[ "$rc" != 0 ]]; then
    echo "Failed to restart docker.service: Unit docker.service not found." >&2
    exit "$rc"
  fi
  exit 0
fi
exit 0
EOS
  chmod +x "$STUB_BIN/systemctl"

  cat > "$STUB_BIN/docker" <<'EOS'
#!/usr/bin/env bash
if [[ "${1:-}" == "info" ]]; then
  # DML_STUB_DOCKER_INFO_HANG=<secs>: docker.service Requires=docker.socket, so
  # a connect to the socket-activated socket SUCCEEDS and the request then
  # blocks while dockerd starts -- a poll that never answers rather than one
  # that refuses. `exec` for the same reason as the systemctl arm above.
  [[ -n "${DML_STUB_DOCKER_INFO_HANG:-}" ]] && exec sleep "${DML_STUB_DOCKER_INFO_HANG}"
  if [[ -n "${DML_STUB_DOCKER_INFO_SEQ:-}" ]]; then
    st="${DML_STUB_DOCKER_INFO_SEQ_STATE:-${DML_STUB_STATE_DIR:?stub state dir unset -- call make_fixture}/docker_info_seq}"
    i=0; [[ -f "$st" ]] && i="$(cat "$st")"
    codes=($DML_STUB_DOCKER_INFO_SEQ)
    idx=$i; (( idx >= ${#codes[@]} )) && idx=$(( ${#codes[@]} - 1 ))
    echo $(( i + 1 )) > "$st"
    exit "${codes[$idx]}"
  fi
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  exit 0
fi
exit 0
EOS
  chmod +x "$STUB_BIN/docker"

  export PATH="$STUB_BIN:$PATH"
}
