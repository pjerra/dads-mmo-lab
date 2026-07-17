#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  export HOME="$FIXTURE"   # sandboxes ~/.dml/backups
  BDIR="$FIXTURE/.dml/backups"
  use_backup_stub
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
}
teardown() { teardown_fixture; }

_done_data() { echo "$1" | grep '"event":"done"' | tail -1; }

@test "backup create dumps the three DBs consistently and writes a gz file" {
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  f="$(echo "$d" | jq -r '.data.file')"
  [[ "$f" =~ ^wow-[0-9]{8}-[0-9]{6}\.sql\.gz$ ]]
  [ -f "$BDIR/$f" ]
  [ "$(echo "$d" | jq -r '.data.size')" -gt 0 ]
  grep -q 'mysqldump -uroot' "$DML_STUB_CALL_LOG"
  grep -q -- '--databases acore_characters acore_playerbots acore_auth --single-transaction --quick' "$DML_STUB_CALL_LOG"
  gunzip -c "$BDIR/$f" | grep -q 'SQL DUMP CONTENT'
}

@test "backup create maps a dump failure to BACKUP_FAILED and leaves no partial file" {
  export DML_STUB_DUMP_EXIT=1
  run bash "$DML" wow backup create --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"BACKUP_FAILED"'
  [ -z "$(ls -A "$BDIR" 2>/dev/null)" ]
}

@test "backup create maps docker-down to DOCKER_DOWN" {
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow backup create --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"DOCKER_DOWN"'
}

@test "backup create prunes to DML_BACKUP_KEEP and reports the pruned names" {
  mkdir -p "$BDIR"
  printf 'x' > "$BDIR/wow-20200101-000000.sql.gz"
  printf 'x' > "$BDIR/wow-20200102-000000.sql.gz"
  export DML_BACKUP_KEEP=2
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.pruned | length')" = "1" ]
  [ "$(echo "$d" | jq -r '.data.pruned[0]')" = "wow-20200101-000000.sql.gz" ]
  [ ! -f "$BDIR/wow-20200101-000000.sql.gz" ]
  [ -f "$BDIR/wow-20200102-000000.sql.gz" ]
}

@test "backup rejects an unknown subcommand" {
  run bash "$DML" wow backup smite --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "UNKNOWN_COMMAND" ]
}

@test "backup list is empty then newest-first with parsed created stamps" {
  run bash "$DML" wow backup list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.backups | length')" = "0" ]
  mkdir -p "$BDIR"
  printf 'x' > "$BDIR/wow-20250101-120000.sql.gz"
  printf 'xx' > "$BDIR/wow-20250201-130000-prerestore.sql.gz"
  run bash "$DML" wow backup list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.backups | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.backups[0].file')" = "wow-20250201-130000-prerestore.sql.gz" ]
  [ "$(echo "$output" | jq -r '.data.backups[0].created')" = "2025-02-01 13:00:00" ]
  [ "$(echo "$output" | jq -r '.data.backups[1].created')" = "2025-01-01 12:00:00" ]
}

@test "backup delete removes the file; missing -> NOT_FOUND" {
  mkdir -p "$BDIR"; printf 'x' > "$BDIR/wow-20250101-120000.sql.gz"
  run bash "$DML" wow backup delete --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.deleted')" = "true" ]
  [ ! -f "$BDIR/wow-20250101-120000.sql.gz" ]
  run bash "$DML" wow backup delete --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "backup delete rejects invalid names (traversal-proof)" {
  for bad in '../etc' 'wow-x.sql.gz' 'wow-20250101-120000.sql' 'wow-20250101-120000.sql.gz;rm'; do
    run bash "$DML" wow backup delete --file "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

_seed_backup() {
  mkdir -p "$BDIR"
  printf 'RESTORE SQL\n' | gzip > "$BDIR/wow-20250101-120000.sql.gz"
}

@test "backup restore orders stop < safety dump < import < start and reports the safety file" {
  _seed_backup
  export DML_STUB_IMPORT_CAPTURE="$FIXTURE/imported.sql"
  run bash "$DML" wow backup restore --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.restored')" = "true" ]
  safety="$(echo "$d" | jq -r '.data.safety_backup')"
  [[ "$safety" =~ -prerestore\.sql\.gz$ ]]
  [ -f "$BDIR/$safety" ]
  stop_line=$(grep -n 'compose stop ac-worldserver ac-authserver' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
  dump_line=$(grep -n 'mysqldump' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
  import_line=$(grep -n 'mysql-import' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
  start_line=$(grep -n 'compose start ac-worldserver ac-authserver' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
  [ "$stop_line" -lt "$dump_line" ]
  [ "$dump_line" -lt "$import_line" ]
  [ "$import_line" -lt "$start_line" ]
  grep -q 'RESTORE SQL' "$FIXTURE/imported.sql"
}

@test "backup restore import failure leaves the server STOPPED and names the safety file" {
  _seed_backup
  export DML_STUB_IMPORT_EXIT=1
  run bash "$DML" wow backup restore --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"BACKUP_FAILED"'
  echo "$output" | grep -q 'LEFT STOPPED'
  echo "$output" | grep -q 'prerestore'
  ! grep -q 'compose start' "$DML_STUB_CALL_LOG"
}

@test "backup restore safety-dump failure restarts the server and imports nothing" {
  _seed_backup
  export DML_STUB_DUMP_EXIT=1
  run bash "$DML" wow backup restore --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"BACKUP_FAILED"'
  ! grep -q 'mysql-import' "$DML_STUB_CALL_LOG"
  grep -q 'compose start' "$DML_STUB_CALL_LOG"
}

@test "backup restore stop failure aborts before any dump or write" {
  _seed_backup
  export DML_STUB_COMPOSE_EXIT=1
  run bash "$DML" wow backup restore --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"BACKUP_FAILED"'
  ! grep -q 'mysqldump' "$DML_STUB_CALL_LOG"
  ! grep -q 'mysql-import' "$DML_STUB_CALL_LOG"
}

@test "backup restore missing file emits a NOT_FOUND error event" {
  run bash "$DML" wow backup restore --file wow-19990101-000000.sql.gz --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"error"'
  echo "$output" | grep -q '"code":"NOT_FOUND"'
}

@test "backup restore rejects invalid names (traversal-proof)" {
  for bad in '../etc' 'wow-x.sql.gz' 'wow-20250101-120000.sql.gz;rm'; do
    run bash "$DML" wow backup restore --file "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}
