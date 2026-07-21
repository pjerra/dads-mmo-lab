#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "paperdoll: appearance from the migrated discrete columns (new schema)" {
  # 17 columns: name level class money race gender skin face hairStyle
  # hairColor facialStyle slot entry item-name Quality ItemLevel displayid
  printf 'Testchar\t80\t1\t123450000\t2\t1\t3\t5\t7\t9\t11\t0\t40001\tHelm\t4\t200\t5001\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow paperdoll --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.race')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.gender')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.skin')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.face')" = "5" ]
  [ "$(echo "$output" | jq -r '.data.hair_style')" = "7" ]
  [ "$(echo "$output" | jq -r '.data.hair_color')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.facial_style')" = "11" ]
  [ "$(echo "$output" | jq -r '.data.name')" = "Testchar" ]
  [ "$(echo "$output" | jq -r '.data.gold')" = "12345" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].displayid')" = "5001" ]
}

@test "paperdoll: falls back to packed playerBytes on a pre-migration DB" {
  # First query (new-schema columns) fails like mysql's ERROR 1054; the
  # fallback query then serves the old 14-column shape and the packed
  # playerBytes decode must produce identical JSON.
  pb=$(( 3 | (5 << 8) | (7 << 16) | (9 << 24) ))
  : > "$FIXTURE/none"
  # Query 1 is now the online-freshness lookup (smoke item 5): offline -> no
  # saveall. Queries 2/3 are the new-schema attempt + packed fallback.
  printf '0\n' > "$FIXTURE/off"
  printf 'Testchar\t80\t1\t123450000\t2\t1\t%s\t11\t0\t40001\tHelm\t4\t200\t5001\n' "$pb" > "$FIXTURE/oldrows"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/off $FIXTURE/none $FIXTURE/oldrows"
  export DML_STUB_DB_EXIT_SEQ="0 1 0"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq"
  run bash "$DML" wow paperdoll --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.skin')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.face')" = "5" ]
  [ "$(echo "$output" | jq -r '.data.hair_style')" = "7" ]
  [ "$(echo "$output" | jq -r '.data.hair_color')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.facial_style')" = "11" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].displayid')" = "5001" ]
}

@test "paperdoll: both schema queries failing is DB_UNREACHABLE" {
  : > "$FIXTURE/none"
  # Slot 1 = the online lookup (fails too -- ignored by design), 2/3 = the
  # two schema queries.
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/none $FIXTURE/none $FIXTURE/none"
  export DML_STUB_DB_EXIT_SEQ="1 1 1"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq"
  run bash "$DML" wow paperdoll --char Testchar --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | tail -1 | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}
