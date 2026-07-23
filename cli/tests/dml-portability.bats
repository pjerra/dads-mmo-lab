#!/usr/bin/env bats
# Native-Windows portability (Git Bash + Docker Desktop, spike/docker-desktop-native):
# flock shim in soap_exec, doctor's native-host systemd branch, the
# DML_GAMES_DIR games-root override, and environment-neutral scan output.
# Everything is exercised inside the Linux test host via the existing seams:
# PATH stubs (helpers/env.bash), exported functions, and the DML_SYSTEMD_DIR
# override (mirrors DML_GAMES_DIR / DML_YQ_BIN).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"   # so ~/.dml/soap.lock lands in the fixture
}

teardown() { teardown_fixture; }

# --- flock shim (20-soap.sh) -------------------------------------------------

@test "soap-exec succeeds with no 'command not found' noise when flock is absent" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  # Mask flock the way a Git Bash host lacks it: a symlink farm of /usr/bin
  # minus flock, with the stub bin (curl) still first on PATH.
  FARM="$FIXTURE/farm"
  mkdir -p "$FARM"
  cp -rs /usr/bin/. "$FARM/" 2>/dev/null || true
  rm -f "$FARM/flock"
  # PATH is swapped only inside the inner shell so bats/teardown keep a
  # working PATH after the fixture (and the farm inside it) is deleted.
  export DML FIXTURE STUB_BIN FARM
  run bash -c 'export PATH="$STUB_BIN:$FARM"
    bash "$DML" wow soap-exec "server info" --json 2>"$FIXTURE/soap-stderr.txt"'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.result')" = "Console command executed." ]
  run grep "command not found" "$FIXTURE/soap-stderr.txt"
  [ "$status" -ne 0 ]
}

@test "soap-exec still acquires and releases the flock when flock exists" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  # Exported function shadows the real flock (command -v finds it, so the
  # have_flock guard takes the locking path) and logs its argv per call.
  export FLOCK_LOG="$FIXTURE/flock.log"
  flock() { printf '%s\n' "$*" >> "$FLOCK_LOG"; }
  export -f flock
  run bash "$DML" wow soap-exec "server info" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.result')" = "Console command executed." ]
  # exactly one acquire (bare fd) then one release (-u fd) -- unchanged shape
  [ "$(wc -l < "$FLOCK_LOG")" -eq 2 ]
  head -1 "$FLOCK_LOG" | grep -qE '^[0-9]+$'
  tail -1 "$FLOCK_LOG" | grep -qE '^-u [0-9]+$'
}

# --- doctor systemd branch (90-main.sh) --------------------------------------

_stub_systemctl_down() {
  cat > "$STUB_BIN/systemctl" <<'EOS'
#!/usr/bin/env bash
[[ "${1:-}" == "is-system-running" ]] && { echo offline; exit 1; }
exit 0
EOS
  chmod +x "$STUB_BIN/systemctl"
}

_stub_uname_msys() {
  cat > "$STUB_BIN/uname" <<'EOS'
#!/usr/bin/env bash
if [[ "${1:-}" == "-o" ]]; then echo Msys; exit 0; fi
exec /usr/bin/uname "$@"
EOS
  chmod +x "$STUB_BIN/uname"
}

@test "doctor on a native Windows host reports ok instead of the systemd WARN" {
  use_docker_stub
  use_curl_stub
  _stub_uname_msys
  export DML_SYSTEMD_DIR="$FIXTURE/no-systemd-here"   # absent, like Git Bash
  run bash "$DML" doctor
  [ "$status" -eq 0 ]
  [[ "$output" == *"[ok]  native Windows mode (no systemd needed)"* ]]
  [[ "$output" != *"[WARN] systemd is not running"* ]]
}

@test "doctor inside the distro still WARNs when systemd is down" {
  use_docker_stub
  use_curl_stub
  _stub_systemctl_down
  export DML_SYSTEMD_DIR="$FIXTURE"   # exists, like /run/systemd/system in WSL
  run bash "$DML" doctor
  [[ "$output" == *"[WARN] systemd is not running -- from Windows run: wsl --shutdown, then reopen"* ]]
  [[ "$output" != *"native Windows mode"* ]]
}

@test "doctor: missing systemd dir alone (non-Windows host) is NOT native mode" {
  use_docker_stub
  use_curl_stub
  _stub_systemctl_down
  # No uname stub: the real uname says Linux, so BOTH-signals gating keeps
  # a systemd-less Linux box on the WARN path.
  export DML_SYSTEMD_DIR="$FIXTURE/no-systemd-here"
  run bash "$DML" doctor
  [[ "$output" == *"[WARN] systemd is not running"* ]]
  [[ "$output" != *"native Windows mode"* ]]
}

# --- DML_GAMES_DIR override (00-head.sh) -------------------------------------

@test "DML_GAMES_DIR points the games root at an arbitrary folder" {
  use_docker_stub
  add_game portable-title compose
  run bash "$DML" list
  [ "$status" -eq 0 ]
  [ "$output" = "portable-title" ]
  # Repointing the override repoints every games-dir read.
  ALT="$FIXTURE/alt-games"
  mkdir -p "$ALT/other-title"
  touch "$ALT/other-title/docker-compose.yml"
  export DML_GAMES_DIR="$ALT"
  run bash "$DML" list
  [ "$status" -eq 0 ]
  [ "$output" = "other-title" ]
}

# --- environment-neutral output (90-main.sh scan) ----------------------------

@test "scan banner is environment-neutral (no dml-arch)" {
  use_docker_stub
  run bash "$DML" scan
  [ "$status" -eq 0 ]
  [[ "$output" == *"Scanning for all running containers in Docker..."* ]]
  [[ "$output" != *"dml-arch"* ]]
}
