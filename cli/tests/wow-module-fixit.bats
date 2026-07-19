#!/usr/bin/env bats
# `wow module fixit battlepass-npc` (Batch 3 F13b): ensures creature_template
# 90100 exists and INSERTs the two capital spawns -- idempotent on an
# existing spawn. All statements are fixed literals (sanctioned write #5).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  printf '0\n' > "$FIXTURE/zero.tsv"
  printf '1\n' > "$FIXTURE/one.tsv"
  printf '2\n' > "$FIXTURE/two.tsv"
}
teardown() { teardown_fixture; }

@test "fixit battlepass-npc: full insert path -- template + both capital spawns + restart note" {
  # Call 1: spawn COUNT -> 0; call 2: template COUNT -> 0; later calls are
  # writes (output ignored, sticky-last).
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/zero.tsv $FIXTURE/zero.tsv"
  run bash "$DML" wow module fixit battlepass-npc --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.already_placed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.template')" = "created" ]
  [ "$(echo "$output" | jq -r '.data.spawns_placed')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  echo "$output" | jq -r '.data.note' | grep -qi 'restart'
  grep -q 'INSERT INTO creature_template' "$FIXTURE/q.log"
  grep -q 'INSERT INTO creature (id, map' "$FIXTURE/q.log"
  # Stormwind AND Orgrimmar coordinates from the cheat-sheet block.
  grep -q -- '-8819.3, 636.2, 94.1, 3.7' "$FIXTURE/q.log"
  grep -q -- '1609.2, -4407.7, 17.5, 4.5' "$FIXTURE/q.log"
}

@test "fixit battlepass-npc: template already present -> spawns only, template=exists" {
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/zero.tsv $FIXTURE/one.tsv"
  run bash "$DML" wow module fixit battlepass-npc --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.template')" = "exists" ]
  [ "$(echo "$output" | jq -r '.data.spawns_placed')" = "2" ]
  run grep 'INSERT INTO creature_template' "$FIXTURE/q.log"
  [ "$status" -ne 0 ]
  grep -q 'INSERT INTO creature (id, map' "$FIXTURE/q.log"
}

@test "fixit battlepass-npc: idempotent -- an existing spawn short-circuits with NO writes" {
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/two.tsv"
  run bash "$DML" wow module fixit battlepass-npc --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.already_placed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.spawns_placed')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  run grep 'INSERT' "$FIXTURE/q.log"
  [ "$status" -ne 0 ]
}

@test "fixit battlepass-npc: db failure -> DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow module fixit battlepass-npc --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "fixit: unknown key -> BAD_ARG listing the available fix" {
  run bash "$DML" wow module fixit bogus --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  echo "$output" | grep -q 'battlepass-npc'
}
