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

@test "party online lists human online chars (rows come pre-filtered by SQL)" {
  # cols: guid, name, class, level
  printf '2503\tTesten\t8\t1\n' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.online[0].name')" = "Testen" ]
  [ "$(echo "$output" | jq -r '.data.online[0].class')" = "8" ]
}

@test "party online SQL excludes bot accounts" {
  printf '' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  grep -q 'playerbots_account_type' "$FIXTURE/q.log"
  grep -q 'online' "$FIXTURE/q.log"
}

@test "party online maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow party online --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}
