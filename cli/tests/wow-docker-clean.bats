#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  use_backup_stub
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
}
teardown() { teardown_fixture; }

_done_data() { echo "$1" | grep '"event":"done"' | tail -1; }

@test "docker-usage returns raw 'docker system df' lines" {
  printf 'TYPE  TOTAL  ACTIVE  SIZE  RECLAIMABLE\nImages  3  1  500MB  200MB\n' > "$FIXTURE/df.txt"
  export DML_STUB_DOCKER_OUT="$FIXTURE/df.txt"
  run bash "$DML" wow docker-usage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.lines | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.lines[0]')" = "TYPE  TOTAL  ACTIVE  SIZE  RECLAIMABLE" ]
  grep -q '^system df$' "$DML_STUB_CALL_LOG"
}

@test "docker-usage maps docker-down to DOCKER_DOWN" {
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow docker-usage --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DOCKER_DOWN" ]
}

@test "docker-clean rejects an invalid or missing level" {
  for lvl in 0 4 abc; do
    run bash "$DML" wow docker-clean --level "$lvl" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
  run bash "$DML" wow docker-clean --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "docker-clean level 1: db-protect < stop-world < builder prune, no volume/image calls, done payload" {
  run bash "$DML" wow docker-clean --level 1 --json
  [ "$status" -eq 0 ]
  db_line=$(grep -n 'compose up -d ac-database' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
  stop_line=$(grep -n 'compose stop -t 180 ac-worldserver' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
  builder_line=$(grep -n '^builder prune -af$' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
  [ -n "$db_line" ]
  [ -n "$stop_line" ]
  [ -n "$builder_line" ]
  [ "$db_line" -lt "$stop_line" ]
  [ "$stop_line" -lt "$builder_line" ]
  [ "$(grep -c '^volume ' "$DML_STUB_CALL_LOG")" = "0" ]
  [ "$(grep -c '^image ' "$DML_STUB_CALL_LOG")" = "0" ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.level')" = "1" ]
  [ "$(echo "$d" | jq -r '.data.cleaned')" = "true" ]
  echo "$output" | grep -q 'Next rebuild will be a full recompile'
}

@test "docker-clean level 2: removes the project-derived build volume" {
  export DML_STUB_VOLUME_NAMES=$'unrelated-volume\nwow-server-playerbots_ac-build'
  run bash "$DML" wow docker-clean --level 2 --json
  [ "$status" -eq 0 ]
  grep -q '^volume rm wow-server-playerbots_ac-build$' "$DML_STUB_CALL_LOG"
  echo "$output" | grep -q 'removing build volume: wow-server-playerbots_ac-build'
}

@test "docker-clean level 2: volume in use -> warn, cleanup still ok" {
  export DML_STUB_VOLUME_NAMES="wow-server-playerbots_ac-build"
  export DML_STUB_DOCKER_FAIL_ARM=volume
  run bash "$DML" wow docker-clean --level 2 --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"level":"warn"'
  echo "$output" | grep -q 'may still be in use'
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.cleaned')" = "true" ]
}

@test "docker-clean level 3: prunes DANGLING images only -- never -a" {
  # `image prune -af` deletes every image no RUNNING container uses, and a
  # STOPPED server's images are exactly that: stage 3 with the stack down
  # deleted all four tagged dml.local images on a real box (2026-08-20). The
  # -a must never come back.
  run bash "$DML" wow docker-clean --level 3 --json
  [ "$status" -eq 0 ]
  grep -q '^image prune -f$' "$DML_STUB_CALL_LOG"
  [ "$(grep -c 'image prune -af' "$DML_STUB_CALL_LOG")" = 0 ]
  [ "$(grep -c 'image prune.*-a\b' "$DML_STUB_CALL_LOG")" = 0 ]
}
