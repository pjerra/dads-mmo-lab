#!/usr/bin/env bats
# `wow world-restart` (Batch 3 F11f): fast world-only restart -- saveall
# best-effort, docker restart -t 300 of ONLY ac-worldserver, readiness wait
# on the boot-complete marker. The settings-don't-apply caveat must be
# surfaced in the stream AND the done payload.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  use_curl_stub
  export HOME="$FIXTURE"
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  add_game wow-server-playerbots compose
  # Boot-complete marker for the current run (the stub serves this for
  # `docker logs --since ...`, i.e. _world_ready's current-run view).
  printf 'World Initialized In 0 minutes 42 seconds\n' > "$FIXTURE/ready.log"
  export DML_STUB_LOGS_SINCE_FILE="$FIXTURE/ready.log"
}
teardown() { teardown_fixture; }

@test "world-restart: event sequence -- caveat warn, restart -t 300 world only, ok + done note" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 0 ]
  # Ordered event sequence.
  echo "$output" | head -1 | grep -q '"event":"section_start","name":"world-restart"'
  echo "$output" | grep -q '"level":"warn".*does NOT apply settings changes'
  echo "$output" | grep -q '"event":"section_end","name":"world-restart","status":"ok"'
  echo "$output" | tail -1 | grep -q '"event":"done"'
  [ "$(echo "$output" | tail -1 | jq -r '.data.restarted')" = "world-only" ]
  echo "$output" | tail -1 | jq -r '.data.note' | grep -q 'full Restart'
  # Exactly the world container was docker-restarted, gracefully...
  grep -q '^restart -t 300 ac-worldserver$' "$FIXTURE/calls.log"
  # ...and never a full compose cycle (that's the full Restart's job).
  run grep -E 'compose (down|up)' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "world-restart: saveall is attempted over SOAP before the restart" {
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 0 ]
  grep -q '7878' "$FIXTURE/curl.log"
}

@test "world-restart: a failed SOAP saveall does not block the restart (best effort)" {
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | tail -1 | jq -r '.data.restarted')" = "world-only" ]
}

@test "world-restart: docker restart failure -> RESTART_FAILED envelope" {
  export DML_STUB_RESTART_EXIT=1
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"section_end","name":"world-restart","status":"error"'
  echo "$output" | tail -1 | grep -q '"code":"RESTART_FAILED"'
}

@test "world-restart: readiness never seen -> READY_TIMEOUT" {
  printf 'still booting...\n' > "$FIXTURE/ready.log"
  export DML_READY_TIMEOUT_SECS=0
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"READY_TIMEOUT"'
}

@test "world-restart: server not installed -> NOT_FOUND" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"NOT_FOUND"'
}

@test "world-restart: docker down -> DOCKER_DOWN" {
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"DOCKER_DOWN"'
}
