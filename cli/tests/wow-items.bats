#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
}
teardown() { teardown_fixture; }

@test "build_item_search_sql filters by name, quality and level" {
  source "$BATS_TEST_DIRNAME/../src/30-db.sh"
  run build_item_search_sql "thunder" 5 60 80 25
  [[ "$output" == *"item_template"* ]]
  [[ "$output" == *"name LIKE '%thunder%'"* ]]
  [[ "$output" == *"Quality = 5"* ]]
  [[ "$output" == *"RequiredLevel >= 60"* ]]
  [[ "$output" == *"RequiredLevel <= 80"* ]]
  [[ "$output" == *"LIMIT 25"* ]]
}

@test "build_item_search_sql omits absent filters" {
  source "$BATS_TEST_DIRNAME/../src/30-db.sh"
  run build_item_search_sql "sword" - - - 50
  [[ "$output" != *"Quality ="* ]]
  [[ "$output" != *"RequiredLevel >="* ]]
}

@test "sql_escape neutralizes quotes" {
  source "$BATS_TEST_DIRNAME/../src/30-db.sh"
  run sql_escape "O'Brien"
  [ "$output" = "O\\'Brien" ]
}

@test "items search returns JSON rows from the db" {
  export DML_STUB_DB_ROWS="$BATS_TEST_DIRNAME/fixtures/items.tsv"
  run bash "$DML" wow items search --name thunder --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.items[1].name')" = "Thunderfury" ]
  [ "$(echo "$output" | jq -r '.data.items[1].quality')" = "5" ]
  [ "$(echo "$output" | jq -r '.data.items[1].displayid')" = "30606" ]
}

@test "items search maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_ROWS="$BATS_TEST_DIRNAME/fixtures/items.tsv"
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "items search with empty --name returns BAD_ARG" {
  export DML_STUB_DB_ROWS="$BATS_TEST_DIRNAME/fixtures/items.tsv"
  run bash "$DML" wow items search --name "" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]

  run bash "$DML" wow items search --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
