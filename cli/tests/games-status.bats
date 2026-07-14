#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
}

teardown() { teardown_fixture; }

@test "games status reports stopped" {
  add_game wow-server-playerbots compose
  run bash "$DML" games status wow-server-playerbots --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.state')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.id')" = "wow-server-playerbots" ]
}

@test "games status reports running" {
  add_game wow-server-playerbots compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow-server-playerbots/docker-compose.yml"
  run bash "$DML" games status wow-server-playerbots --json
  [ "$(echo "$output" | jq -r '.data.state')" = "running" ]
}

@test "games status for unknown title returns NOT_FOUND exit 1" {
  run bash "$DML" games status nope --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
