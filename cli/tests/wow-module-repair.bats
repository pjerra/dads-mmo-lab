#!/usr/bin/env bats
load helpers/env.bash

# Round J Task 1: `wow module tracking` (read-only diagnosis) + `wow module
# repair` (mark/clear on the AC `updates` tracking tables) + the generalized
# _db_write_stmt. See docs/superpowers/specs/2026-07-18-module-repair-design.md
# and docs/superpowers/plans/2026-07-18-module-repair.md.

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  use_mysql_stub
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
}
teardown() { teardown_fixture; }

install_module() {  # install_module <key> -- satisfies _cpp_installed
  mkdir -p "$SDIR/modules/$1/.git"
}

# ---------- tracking ----------

@test "tracking returns per-db tracked_rows + files with tracked flags" {
  install_module mod-transmog
  mkdir -p "$SDIR/modules/mod-transmog/data/sql/db-world"
  printf 'CREATE TABLE x;\n' > "$SDIR/modules/mod-transmog/data/sql/db-world/trasmorg.sql"
  printf 'trasmorg.sql\n' > "$FIXTURE/world.tsv"
  printf '' > "$FIXTURE/empty.tsv"
  printf '1\n' > "$FIXTURE/count.tsv"
  # Query order: world LIKE -> world COUNT (exact-name, for the one
  # discovered file) -> characters LIKE -> auth LIKE.
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/world.tsv $FIXTURE/count.tsv $FIXTURE/empty.tsv $FIXTURE/empty.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow module tracking --key mod-transmog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.key')" = "mod-transmog" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.tracked_rows | join(",")')" = "trasmorg.sql" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files[0].name')" = "trasmorg.sql" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files[0].tracked')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.dbs.characters.tracked_rows | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.dbs.auth.tracked_rows | length')" = "0" ]
}

@test "tracking discovery falls back to modules/K/sql/<short> and flags undiscovered rows untracked" {
  install_module mod-transmog
  mkdir -p "$SDIR/modules/mod-transmog/sql/world"
  printf 'CREATE TABLE x;\n' > "$SDIR/modules/mod-transmog/sql/world/legacy.sql"
  printf '' > "$FIXTURE/empty.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/empty.tsv $FIXTURE/empty.tsv $FIXTURE/empty.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow module tracking --key mod-transmog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files[0].name')" = "legacy.sql" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files[0].tracked')" = "false" ]
}

@test "tracking: per-file tracked is an exact-name lookup, independent of the LIKE tracked_rows list" {
  # Real-world regression: mod-ah-bot ships mod_auctionhousebot.sql, which is
  # genuinely tracked in the DB but whose name contains neither LIKE term
  # ("ah-bot" / "ah_bot"), so tracked_rows stays empty while the file's own
  # `tracked` flag must still come back true (exact-name COUNT, not a LIKE
  # substring match).
  install_module mod-ah-bot
  mkdir -p "$SDIR/modules/mod-ah-bot/data/sql/db-world"
  printf 'CREATE TABLE ah;\n' > "$SDIR/modules/mod-ah-bot/data/sql/db-world/mod_auctionhousebot.sql"
  printf '' > "$FIXTURE/empty.tsv"
  printf '1\n' > "$FIXTURE/count.tsv"
  # Query order for `world characters auth`: world LIKE (empty) -> world
  # COUNT for the one discovered file (1, i.e. tracked) -> characters LIKE
  # (empty, no discovered files so no COUNT call) -> auth LIKE (empty, same).
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/empty.tsv $FIXTURE/count.tsv $FIXTURE/empty.tsv $FIXTURE/empty.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow module tracking --key mod-ah-bot --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.tracked_rows | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files[0].name')" = "mod_auctionhousebot.sql" ]
  [ "$(echo "$output" | jq -r '.data.dbs.world.files[0].tracked')" = "true" ]
}

@test "tracking on a module that isn't installed -> NOT_FOUND" {
  run bash "$DML" wow module tracking --key mod-transmog --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

# ---------- repair: validation ----------

@test "repair rejects invalid key, db, and mode -- no SQL runs" {
  install_module mod-transmog
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow module repair --key not-a-module --db world --mode mark --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow module repair --key mod-transmog --db nope --mode mark --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow module repair --key mod-transmog --db world --mode delete --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -s "$FIXTURE/query.log" ]
}

@test "repair rejects path-injection filenames (../evil.sql, x.sql;DROP) -- no SQL runs" {
  install_module mod-transmog
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow module repair --key mod-transmog --db world --mode mark --files '../evil.sql' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow module repair --key mod-transmog --db world --mode mark --files 'x.sql;DROP' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -s "$FIXTURE/query.log" ]
}

@test "repair on a module that isn't installed -> NOT_FOUND" {
  run bash "$DML" wow module repair --key mod-transmog --db world --mode mark --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

# ---------- repair: mark ----------

@test "repair mark: INSERT with the real uppercase sha1, RELEASED, ON DUPLICATE KEY UPDATE" {
  install_module mod-transmog
  mkdir -p "$SDIR/modules/mod-transmog/data/sql/db-characters"
  sqlfile="$SDIR/modules/mod-transmog/data/sql/db-characters/trasmorg.sql"
  printf 'CREATE TABLE trasmorg (id INT);\n' > "$sqlfile"
  expected_hash="$(sha1sum "$sqlfile" | awk '{print toupper($1)}')"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow module repair --key mod-transmog --db characters --mode mark --files trasmorg.sql --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.key')" = "mod-transmog" ]
  [ "$(echo "$output" | jq -r '.data.db')" = "characters" ]
  [ "$(echo "$output" | jq -r '.data.mode')" = "mark" ]
  [ "$(echo "$output" | jq -r '.data.results[0].file')" = "trasmorg.sql" ]
  [ "$(echo "$output" | jq -r '.data.results[0].result')" = "marked" ]
  insert_line="$(grep 'INSERT INTO updates' "$FIXTURE/query.log")"
  [[ "$insert_line" == *"acore_characters"* ]]
  [[ "$insert_line" == *"'trasmorg.sql'"* ]]
  [[ "$insert_line" == *"'$expected_hash'"* ]]
  [[ "$insert_line" == *"'RELEASED'"* ]]
  [[ "$insert_line" == *"NOW()"* ]]
  [[ "$insert_line" == *", 0)"* ]]
  [[ "$insert_line" == *"ON DUPLICATE KEY UPDATE"* ]]
}

@test "repair mark: file not found on disk -> file_missing, no INSERT" {
  install_module mod-transmog
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow module repair --key mod-transmog --db characters --mode mark --files ghost.sql --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.results[0].result')" = "file_missing" ]
  ! grep -q 'INSERT' "$FIXTURE/query.log" 2>/dev/null
}

@test "repair mark with no --files uses the discovered SQL files for that db" {
  install_module mod-transmog
  mkdir -p "$SDIR/modules/mod-transmog/data/sql/db-world"
  printf 'a\n' > "$SDIR/modules/mod-transmog/data/sql/db-world/one.sql"
  printf 'b\n' > "$SDIR/modules/mod-transmog/data/sql/db-world/two.sql"
  run bash "$DML" wow module repair --key mod-transmog --db world --mode mark --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.results | length')" = "2" ]
  names="$(echo "$output" | jq -r '.data.results[].file' | sort | tr '\n' ',')"
  [ "$names" = "one.sql,two.sql," ]
}

@test "repair --files overrides discovery -- only the named file is processed" {
  install_module mod-transmog
  mkdir -p "$SDIR/modules/mod-transmog/data/sql/db-world"
  printf 'a\n' > "$SDIR/modules/mod-transmog/data/sql/db-world/one.sql"
  printf 'b\n' > "$SDIR/modules/mod-transmog/data/sql/db-world/two.sql"
  run bash "$DML" wow module repair --key mod-transmog --db world --mode mark --files 'two.sql' --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.results | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.results[0].file')" = "two.sql" ]
}

# ---------- repair: clear ----------

@test "repair clear: COUNT then DELETE for a tracked row" {
  install_module mod-transmog
  printf '1\n' > "$FIXTURE/count.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/count.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow module repair --key mod-transmog --db world --mode clear --files trasmorg.sql --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.results[0].result')" = "cleared" ]
  grep -q 'SELECT COUNT' "$FIXTURE/query.log"
  grep -q 'DELETE FROM updates' "$FIXTURE/query.log"
  delete_line="$(grep 'DELETE FROM updates' "$FIXTURE/query.log")"
  [[ "$delete_line" == *"acore_world"* ]]
  [[ "$delete_line" == *"'trasmorg.sql'"* ]]
  first_line="$(head -1 "$FIXTURE/query.log")"
  [[ "$first_line" == *"SELECT COUNT"* ]]
}

@test "repair clear: untracked row -> not_tracked, no DELETE" {
  install_module mod-transmog
  printf '0\n' > "$FIXTURE/count.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/count.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow module repair --key mod-transmog --db world --mode clear --files ghost.sql --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.results[0].result')" = "not_tracked" ]
  ! grep -q 'DELETE' "$FIXTURE/query.log"
}

# ---------- _db_write_stmt / _chars_write_stmt parity ----------

@test "_db_write_stmt rejects a non-acore db name; _chars_write_stmt still behaves identically" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/30-db.sh"; _db_write_stmt evil_db "DROP TABLE x;"; echo "rc=$?"'
  [[ "$output" == *"rc=1"* ]]
}
