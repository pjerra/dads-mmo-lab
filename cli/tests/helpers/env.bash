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
if [[ "${1:-}" == "info" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1 || exit 0
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
if [[ "${1:-}" == "ps" ]]; then exit 0; fi
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
# Canned SOAP responder. Ignores all args; emits the file in DML_STUB_SOAP_RESPONSE
# to stdout and exits with DML_STUB_CURL_EXIT (default 0). Real `curl -w
# '\n%{http_code}'` (what soap_exec actually passes) ALWAYS appends a trailing
# "\n<code>" line -- so this stub always appends one too, defaulting to 200
# when DML_STUB_HTTP is unset, and honoring an explicit DML_STUB_HTTP value
# when the caller wants to simulate a non-200 (e.g. 401).
#
# soap_exec pipes the request body into curl via `--data-binary @-` (stdin).
# When DML_STUB_CAPTURE is set, save that stdin verbatim so tests can assert
# on the exact XML/command text the server would have received.
if [[ -n "${DML_STUB_CAPTURE_APPEND:-}" ]]; then
  cat >> "$DML_STUB_CAPTURE_APPEND"
elif [[ -n "${DML_STUB_CAPTURE:-}" ]]; then
  cat > "$DML_STUB_CAPTURE"
else
  cat >/dev/null
fi
[[ -n "${DML_STUB_SOAP_RESPONSE:-}" ]] && cat "$DML_STUB_SOAP_RESPONSE"
printf '\n%s' "${DML_STUB_HTTP:-200}"
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
    log "mysqldump ${args#*mysqldump }"
    if [[ "${DML_STUB_DUMP_EXIT:-0}" != 0 ]]; then echo "dump boom" >&2; exit "${DML_STUB_DUMP_EXIT}"; fi
    printf 'SQL DUMP CONTENT\n'
    exit 0
  fi
  # NB: checked AFTER mysqldump (which exits above), so this only matches the import.
  if [[ "$args" == *" mysql"* ]]; then
    log "mysql-import"
    cat > /dev/null
    exit "${DML_STUB_IMPORT_EXIT:-0}"
  fi
  exit 0
fi
if [[ "${1:-}" == "compose" ]]; then
  shift
  log "compose $*"
  exit "${DML_STUB_COMPOSE_EXIT:-0}"
fi
exit 0
EOS
  chmod +x "$STUB_BIN/docker"
  export PATH="$STUB_BIN:$PATH"
}
