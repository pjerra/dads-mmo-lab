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

@test "paperdoll: appearance fields decoded from playerBytes" {
  # playerBytes = skin 3 | face 5<<8 | hairStyle 7<<16 | hairColor 9<<24
  pb=$(( 3 | (5 << 8) | (7 << 16) | (9 << 24) ))
  # columns: name level class money race gender playerBytes playerBytes2 slot entry item-name Quality ItemLevel displayid
  printf 'Testchar\t80\t1\t123450000\t2\t1\t%s\t11\t0\t40001\tHelm\t4\t200\t5001\n' "$pb" > "$FIXTURE/rows"
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
