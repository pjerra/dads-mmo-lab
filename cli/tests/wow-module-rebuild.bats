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
  printf 'mod-aoe-loot\n' > "$SDIR/.dml-rebuild-pending"
}
teardown() { teardown_fixture; }

@test "rebuild requires an explicit backup choice" {
  run bash "$DML" wow module rebuild --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  echo "$output" | grep -qi 'backup'
}

@test "rebuild --backup: dump BEFORE compose, pending cleared on success" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow module rebuild --backup --json
  [ "$status" -eq 0 ]
  grep -n 'mysqldump' "$FIXTURE/calls.log" | head -1 | grep -q '^1:'
  grep -q 'compose stop -t 180 ac-worldserver' "$FIXTURE/calls.log"
  grep -q 'compose up -d --build' "$FIXTURE/calls.log"
  [ ! -f "$SDIR/.dml-rebuild-pending" ]
  echo "$output" | grep -q '"event":"done"'
}

@test "rebuild --backup: world included in the safety dump" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow module rebuild --backup --json
  grep -q 'acore_world' "$FIXTURE/calls.log"
}

@test "rebuild --no-backup skips the dump" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow module rebuild --no-backup --json
  [ "$status" -eq 0 ]
  ! grep -q 'mysqldump' "$FIXTURE/calls.log"
}

@test "rebuild: backup failure aborts before compose" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  export DML_STUB_DUMP_EXIT=1
  run bash "$DML" wow module rebuild --backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BACKUP_FAILED'
  [ "$(grep -c 'compose up' "$FIXTURE/calls.log")" = "0" ]
  [ -f "$SDIR/.dml-rebuild-pending" ]
}

@test "rebuild: compose failure -> BUILD_FAILED, pending kept, log hint" {
  export DML_STUB_COMPOSE_EXIT=1
  run bash "$DML" wow module rebuild --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BUILD_FAILED'
  echo "$output" | grep -q 'rebuild.log'
  [ -f "$SDIR/.dml-rebuild-pending" ]
}

@test "module rebuild refuses an image-only server before the backup" {
  export DML_STUB_COMPOSE_CONFIG=nobuild
  run bash "$DML" wow module rebuild --backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"MODULE_NO_BUILD_CONFIG"'
  # Refusal precedes the backup: no dump narration may appear.
  run bash -c "echo '$output' | grep -c 'backing up'"
  [ "$output" = "0" ]
}

@test "module rebuild warns and proceeds when compose config cannot answer" {
  export DML_STUB_COMPOSE_CONFIG=fail
  run bash "$DML" wow module rebuild --no-backup --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'could not read the compose configuration'
  count=$(printf '%s' "$output" | grep -c '"code":"MODULE_NO_BUILD_CONFIG"' || true)
  [ "$count" = "0" ]
}
