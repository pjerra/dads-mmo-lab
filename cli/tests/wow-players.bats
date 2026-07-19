#!/usr/bin/env bats
# `wow players online` (Batch 3 F11a): read-only who's-playing list for the
# Home card. Mirrors the party-online suite's stub conventions.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "players online lists name/level/class/zone rows" {
  # cols: name, level, class, zone
  printf 'Testen\t42\t8\t1519\nVenn\t12\t1\t12\n' > "$FIXTURE/pl.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/pl.tsv"
  run bash "$DML" wow players online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.players | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.players[0].name')" = "Testen" ]
  [ "$(echo "$output" | jq -r '.data.players[0].level')" = "42" ]
  [ "$(echo "$output" | jq -r '.data.players[0].class')" = "8" ]
  [ "$(echo "$output" | jq -r '.data.players[0].zone')" = "1519" ]
  [ "$(echo "$output" | jq -r '.data.players[1].name')" = "Venn" ]
}

@test "players online: empty result is an empty array, not an error" {
  printf '' > "$FIXTURE/pl.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/pl.tsv"
  run bash "$DML" wow players online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.players | length')" = "0" ]
}

@test "players online SQL excludes bot accounts and filters online=1" {
  printf '' > "$FIXTURE/pl.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/pl.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow players online --json
  [ "$status" -eq 0 ]
  grep -q 'playerbots_account_type' "$FIXTURE/q.log"
  grep -q 'online = 1' "$FIXTURE/q.log"
}

@test "players online: a NULL zone degrades to 0 instead of breaking the JSON" {
  printf 'Testen\t42\t8\tNULL\n' > "$FIXTURE/pl.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/pl.tsv"
  run bash "$DML" wow players online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.players[0].zone')" = "0" ]
}

@test "players online maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow players online --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "players: unknown subcommand -> UNKNOWN_COMMAND" {
  run bash "$DML" wow players bogus --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "UNKNOWN_COMMAND" ]
}
