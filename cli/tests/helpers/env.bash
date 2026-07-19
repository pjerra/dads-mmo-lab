# Shared test harness: fixture games dir + docker stub.
make_fixture() {
  FIXTURE="$(mktemp -d)"
  export DML_GAMES_DIR="$FIXTURE/games"
  mkdir -p "$DML_GAMES_DIR"
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
  if [[ -n "${DML_STUB_PORTS:-}" ]]; then
    while read -r c i h; do
      [[ "$c" == "${2:-}" && "$i" == "${3:-}" ]] && echo "0.0.0.0:$h"
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
    st="${DML_STUB_DB_SEQ_STATE:-/tmp/dml_seq_state.$$}"
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
  st="${DML_STUB_CURL_SEQ_STATE:-/tmp/dml_curl_seq.$$}"
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
    st="${DML_STUB_DB_SEQ_STATE:-/tmp/dml_seq_state.$$}"
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
    st="${DML_STUB_GIT_HEAD_SEQ_STATE:-/tmp/dml_git_head_seq.$$}"
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
