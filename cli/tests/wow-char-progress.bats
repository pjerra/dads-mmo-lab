#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/dbseq"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/queries.log"
}
teardown() { teardown_fixture; }

@test "char-progress: full shape (guid, groups, achievements, active-spec talents)" {
  printf '7\n' > "$FIXTURE/r1"                       # guid
  printf '1\t2\n' > "$FIXTURE/r2"                    # activeTalentGroup=1, groups=2
  printf '42\n' > "$FIXTURE/r3"                      # total achievements
  printf '1234\t1700000000\n4567\t1690000000\n' > "$FIXTURE/r4"
  printf '11111\n22222\n' > "$FIXTURE/r5"            # talent spells
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1 $FIXTURE/r2 $FIXTURE/r3 $FIXTURE/r4 $FIXTURE/r5"
  run bash "$DML" wow char-progress --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.achievements.total')" = "42" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent[0].id')" = "1234" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent[0].date')" = "1700000000" ]
  [ "$(echo "$output" | jq -r '.data.talents.groups_count')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.talents.active_group')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.talents.spells | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.talents.spells[0]')" = "11111" ]
  grep -q 'specMask & (1 << 1)' "$FIXTURE/queries.log"
}

@test "char-progress: unknown character -> NOT_FOUND" {
  printf '' > "$FIXTURE/r1"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1"
  run bash "$DML" wow char-progress --char Nobody --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "char-progress: invalid name -> BAD_ARG before any SQL" {
  run bash "$DML" wow char-progress --char 'x;drop' --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  [ ! -f "$FIXTURE/queries.log" ]
}

@test "char-progress: DB down -> DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow char-progress --char Testchar --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'DB_UNREACHABLE'
}

@test "char-progress: empty achievements/talents -> zeros and empty arrays" {
  printf '7\n' > "$FIXTURE/r1"
  printf '0\t1\n' > "$FIXTURE/r2"
  printf '0\n' > "$FIXTURE/r3"
  printf '' > "$FIXTURE/r4"
  printf '' > "$FIXTURE/r5"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1 $FIXTURE/r2 $FIXTURE/r3 $FIXTURE/r4 $FIXTURE/r5"
  run bash "$DML" wow char-progress --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.achievements.total')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.achievements.recent | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.talents.spells | length')" = "0" ]
}

@test "achievements: full earned list with dates" {
  printf '7\n' > "$FIXTURE/r1"                       # guid
  printf '6\t1690000000\n1234\t1700000000\n' > "$FIXTURE/r2"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1 $FIXTURE/r2"
  run bash "$DML" wow achievements --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.earned | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.earned[0].id')" = "6" ]
  [ "$(echo "$output" | jq -r '.data.earned[1].date')" = "1700000000" ]
}

@test "achievements: character with none earned -> empty array" {
  printf '7\n' > "$FIXTURE/r1"
  printf '' > "$FIXTURE/r2"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1 $FIXTURE/r2"
  run bash "$DML" wow achievements --char Testchar --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.earned | length')" = "0" ]
}

@test "achievements: unknown character -> NOT_FOUND" {
  printf '' > "$FIXTURE/r1"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/r1"
  run bash "$DML" wow achievements --char Nobody --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "achievements: invalid name -> BAD_ARG before any SQL" {
  run bash "$DML" wow achievements --char 'x;drop' --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}
