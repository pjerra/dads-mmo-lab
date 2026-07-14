#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
}

teardown() { teardown_fixture; }

@test "games list --json lists compose, install-only and nested titles" {
  add_game wow-server-playerbots compose
  add_game runescape install
  add_game tortoise nested
  add_game junk empty
  run bash "$DML" games list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.games | length')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="wow-server-playerbots") | .running')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="junk") | .id' )" = "" ]
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="runescape") | .running')" = "false" ]
  [[ "$(echo "$output" | jq -r '.data.games[] | select(.id=="runescape") | .path')" == */runescape ]]
  [[ "$(echo "$output" | jq -r '.data.games[] | select(.id=="tortoise") | .path')" == */tortoise/sub ]]
}

@test "games list --json with docker down still returns envelope" {
  add_game wow-server-playerbots compose
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" games list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.games[0].running')" = "false" ]
}

@test "games list --json marks running titles via compose ps" {
  add_game wow-server-playerbots compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow-server-playerbots/docker-compose.yml"
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].running')" = "true" ]
}

@test "games list --json with no games dir returns empty array" {
  rm -rf "$DML_GAMES_DIR"
  run bash "$DML" games list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -c '.data.games')" = "[]" ]
}

@test "legacy list output is unchanged" {
  add_game wow-server-playerbots compose
  run bash "$DML" list
  [ "$status" -eq 0 ]
  [ "$output" = "wow-server-playerbots" ]
}
