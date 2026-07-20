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
  stop_line=$(grep -n 'compose stop -t 180 ac-worldserver ac-authserver' "$DML_STUB_CALL_LOG" | head -1 | cut -d: -f1)
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

@test "backup create without --include-world dumps exactly the three char DBs" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  grep -q 'acore_characters acore_playerbots acore_auth' "$FIXTURE/calls.log"
  ! grep -q 'acore_world' "$FIXTURE/calls.log"
}

@test "backup create --include-world adds acore_world to the same dump" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup create --include-world --json
  [ "$status" -eq 0 ]
  grep -q 'acore_characters acore_playerbots acore_auth acore_world' "$FIXTURE/calls.log"
  echo "$output" | grep -q '"world":true'
}

@test "backup create --include-world failure still cleans up and errors" {
  export DML_STUB_DUMP_EXIT=1
  run bash "$DML" wow backup create --include-world --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BACKUP_FAILED'
}

@test "backup create --include-world names the file -full and list marks world:true" {
  run bash "$DML" wow backup create --include-world --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -Eq '"file":"wow-[0-9]{8}-[0-9]{6}-full\.sql\.gz"'
  run bash "$DML" wow backup list --json
  [ "$(echo "$output" | jq -r '.data.backups[0].world')" = "true" ]
  run bash "$DML" wow backup create --json
  run bash "$DML" wow backup list --json
  [ "$(echo "$output" | jq -r '[.data.backups[] | select(.world==false)] | length')" = "1" ]
}

@test "_valid_backup_name accepts -full and -full-prerestore, rejects scrambled order" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/30-db.sh" 2>/dev/null || true; source "'"$BATS_TEST_DIRNAME"'/../src/60-backup.sh"; _valid_backup_name wow-20260717-120000-full.sql.gz && echo A; _valid_backup_name wow-20260717-120000-full-prerestore.sql.gz && echo B; _valid_backup_name wow-20260717-120000-prerestore-full.sql.gz || echo C'
  [ "${lines[0]}" = "A" ]
  [ "${lines[1]}" = "B" ]
  [ "${lines[2]}" = "C" ]
}

@test "restore of a -full backup takes a full safety dump" {
  bdir="$FIXTURE/.dml/backups"; mkdir -p "$bdir"
  printf 'x' | gzip > "$bdir/wow-20260101-000000-full.sql.gz"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup restore --file wow-20260101-000000-full.sql.gz --json
  [ "$status" -eq 0 ]
  grep 'mysqldump' "$FIXTURE/calls.log" | grep -q 'acore_world'
  echo "$output" | grep -q -- '-full-prerestore.sql.gz'
}

@test "restore of a plain backup keeps the safety dump world-free" {
  bdir="$FIXTURE/.dml/backups"; mkdir -p "$bdir"
  printf 'x' | gzip > "$bdir/wow-20260101-000000.sql.gz"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow backup restore --file wow-20260101-000000.sql.gz --json
  [ "$status" -eq 0 ]
  ! grep 'mysqldump' "$FIXTURE/calls.log" | grep -q 'acore_world'
}

# ---------- backup content summary (Batch 4: per-snapshot sidecar) ----------

@test "backup create records a per-snapshot content summary sidecar" {
  # A docker stub that also answers the summary COUNT(*) queries so the
  # sidecar gets real numbers (use_backup_stub's -e arm returns nothing).
  # Match order matters: the bots query contains BOTH playerbots_account_type
  # and "FROM characters", so it must be checked first.
  cat > "$FIXTURE/bin/docker" <<'EOS'
#!/usr/bin/env bash
[[ -n "${DML_STUB_CALL_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_CALL_LOG"
[[ "${1:-}" == info ]] && exit 0
if [[ "${1:-}" == exec ]]; then
  args="$*"
  if [[ "$args" == *mysqldump* ]]; then printf 'SQL DUMP CONTENT\n'; exit 0; fi
  if [[ "$args" == *playerbots_account_type* ]]; then echo 42; exit 0; fi
  if [[ "$args" == *"FROM account"* ]]; then echo 7; exit 0; fi
  if [[ "$args" == *"FROM characters"* ]]; then echo 130; exit 0; fi
  exit 0
fi
exit 0
EOS
  chmod +x "$FIXTURE/bin/docker"
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  f="$(_done_data "$output" | jq -r '.data.file')"
  [ -f "$BDIR/$f.meta" ]
  run cat "$BDIR/$f.meta"
  [ "$(echo "$output" | jq -r '.characters')" = "130" ]
  [ "$(echo "$output" | jq -r '.accounts')" = "7" ]
  [ "$(echo "$output" | jq -r '.bots')" = "42" ]
}

@test "backup create writes no sidecar when the counts can't be read" {
  # The default use_backup_stub returns nothing for `mysql -e` queries, so
  # the summary is unreadable -- the backup still succeeds, just sidecar-free.
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  f="$(_done_data "$output" | jq -r '.data.file')"
  [ -f "$BDIR/$f" ]
  [ ! -f "$BDIR/$f.meta" ]
}

@test "backup list surfaces a snapshot summary and null for sidecar-less backups" {
  mkdir -p "$BDIR"
  printf 'x' > "$BDIR/wow-20250101-120000.sql.gz"
  printf '{"characters":130,"accounts":7,"bots":42}\n' > "$BDIR/wow-20250101-120000.sql.gz.meta"
  printf 'x' > "$BDIR/wow-20240101-120000.sql.gz"   # older, no sidecar
  run bash "$DML" wow backup list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.backups[0].summary.characters')" = "130" ]
  [ "$(echo "$output" | jq -r '.data.backups[0].summary.accounts')" = "7" ]
  [ "$(echo "$output" | jq -r '.data.backups[0].summary.bots')" = "42" ]
  [ "$(echo "$output" | jq -r '.data.backups[1].summary')" = "null" ]
}

@test "backup list degrades a malformed summary sidecar to null" {
  mkdir -p "$BDIR"
  printf 'x' > "$BDIR/wow-20250101-120000.sql.gz"
  printf 'garbage not json\n' > "$BDIR/wow-20250101-120000.sql.gz.meta"
  run bash "$DML" wow backup list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.backups[0].summary')" = "null" ]
}

@test "backup delete also removes the summary sidecar" {
  mkdir -p "$BDIR"
  printf 'x' > "$BDIR/wow-20250101-120000.sql.gz"
  printf '{"characters":1,"accounts":1,"bots":null}\n' > "$BDIR/wow-20250101-120000.sql.gz.meta"
  run bash "$DML" wow backup delete --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 0 ]
  [ ! -f "$BDIR/wow-20250101-120000.sql.gz" ]
  [ ! -f "$BDIR/wow-20250101-120000.sql.gz.meta" ]
}

@test "backup create prunes a pruned backup's summary sidecar too" {
  mkdir -p "$BDIR"
  printf 'x' > "$BDIR/wow-20200101-000000.sql.gz"
  printf '{"characters":1,"accounts":1,"bots":null}\n' > "$BDIR/wow-20200101-000000.sql.gz.meta"
  printf 'x' > "$BDIR/wow-20200102-000000.sql.gz"
  export DML_BACKUP_KEEP=2
  run bash "$DML" wow backup create --json
  [ "$status" -eq 0 ]
  [ ! -f "$BDIR/wow-20200101-000000.sql.gz" ]
  [ ! -f "$BDIR/wow-20200101-000000.sql.gz.meta" ]
}

# ---------- backup validate (Batch 4 A: gzip -t + light SQL-sanity) ----------

@test "backup validate passes a good dump (gzip ok + core table markers)" {
  mkdir -p "$BDIR"
  printf -- '-- MySQL dump\nCREATE TABLE `characters` (guid int);\nCREATE TABLE `account` (id int);\n' \
    | gzip > "$BDIR/wow-20250101-120000.sql.gz"
  run bash "$DML" wow backup validate --file wow-20250101-120000.sql.gz --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.valid')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.gzip_ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.sql_ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.markers | sort | join(",")')" = "account,characters" ]
  [ "$(echo "$output" | jq -r '.data.size')" -gt 0 ]
}

@test "backup validate fails a corrupt (non-gzip) file without touching docker" {
  mkdir -p "$BDIR"
  printf 'this is not a gzip archive at all' > "$BDIR/wow-20250102-120000.sql.gz"
  run bash "$DML" wow backup validate --file wow-20250102-120000.sql.gz --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.valid')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.gzip_ok')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.sql_ok')" = "false" ]
  echo "$output" | grep -qi 'corrupt'
}

@test "backup validate fails a truncated gzip archive" {
  mkdir -p "$BDIR"
  printf -- '-- MySQL dump\nCREATE TABLE `characters` (guid int);\nCREATE TABLE `account` (id int);\n' \
    | gzip > "$BDIR/wow-20250103-120000.sql.gz"
  # Lop off the trailing bytes so gzip -t reports a truncated/corrupt archive.
  full=$(stat -c %s "$BDIR/wow-20250103-120000.sql.gz")
  head -c $(( full - 6 )) "$BDIR/wow-20250103-120000.sql.gz" > "$BDIR/wow-20250103-120000.sql.gz.cut"
  mv "$BDIR/wow-20250103-120000.sql.gz.cut" "$BDIR/wow-20250103-120000.sql.gz"
  run bash "$DML" wow backup validate --file wow-20250103-120000.sql.gz --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.valid')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.gzip_ok')" = "false" ]
}

@test "backup validate flags a valid gzip that is missing the character tables" {
  mkdir -p "$BDIR"
  printf 'just some unrelated text, no CREATE TABLE here\n' | gzip > "$BDIR/wow-20250104-120000.sql.gz"
  run bash "$DML" wow backup validate --file wow-20250104-120000.sql.gz --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.valid')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.gzip_ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.sql_ok')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.markers | length')" = "0" ]
}

@test "backup validate rejects invalid names (traversal-proof) and missing files" {
  for bad in '../etc' 'wow-x.sql.gz' 'wow-20250101-120000.sql.gz;rm'; do
    run bash "$DML" wow backup validate --file "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
  run bash "$DML" wow backup validate --file wow-19990101-000000.sql.gz --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
