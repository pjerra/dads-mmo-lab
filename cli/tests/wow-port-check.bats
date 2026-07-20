#!/usr/bin/env bats
# Batch 5 (overnight): LAN-readiness diagnostic -- `wow port-check`. Reads how
# Docker publishes the game/DB ports (docker port <container> <internal>) and
# reports whether each is reachable from other PCs (0.0.0.0 / a LAN IP) vs
# loopback-only. Read-only; the docker stub's `port` arm serves the mappings
# from DML_STUB_PORTS (a "<container> <internal> <hostport|ip:port>" table).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  use_docker_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "port-check: all game ports on 0.0.0.0 -> running + game_lan_ready" {
  export DML_STUB_PORTS="ac-authserver 3724 3724
ac-worldserver 8085 8085
ac-database 3306 13306"
  run bash "$DML" wow port-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.running')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.game_lan_ready')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.db_host_port')" = "13306" ]
  # per-port detail
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="login") | .host_port')" = "3724" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="login") | .lan_ready')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="world") | .host_port')" = "8085" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="database") | .host_port')" = "13306" ]
}

@test "port-check: db bound to loopback -> db_lan_exposed false, game still ready" {
  export DML_STUB_PORTS="ac-authserver 3724 3724
ac-worldserver 8085 8085
ac-database 3306 127.0.0.1:13306"
  run bash "$DML" wow port-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.game_lan_ready')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.db_lan_exposed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="database") | .host_ip')" = "127.0.0.1" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="database") | .lan_ready')" = "false" ]
}

@test "port-check: a game port stuck on loopback -> game_lan_ready false" {
  export DML_STUB_PORTS="ac-authserver 3724 127.0.0.1:3724
ac-worldserver 8085 8085
ac-database 3306 13306"
  run bash "$DML" wow port-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.game_lan_ready')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="login") | .lan_ready')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="world") | .lan_ready')" = "true" ]
}

@test "port-check: nothing published (server stopped) -> running false" {
  # No DML_STUB_PORTS -> docker port prints nothing for every container.
  run bash "$DML" wow port-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.running')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.game_lan_ready')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="world") | .published')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.ports[] | select(.service=="world") | .host_port')" = "null" ]
}

@test "port-check: db host port read from .env fallback when not published" {
  # Only game ports published; db unpublished -> fall back to the .env value.
  printf 'DOCKER_DB_EXTERNAL_PORT=23306\n' > "$GDIR/.env"
  export DML_STUB_PORTS="ac-authserver 3724 3724
ac-worldserver 8085 8085"
  run bash "$DML" wow port-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.db_host_port')" = "23306" ]
}

@test "port-check: server not installed -> NOT_FOUND" {
  rm -rf "$GDIR"
  run bash "$DML" wow port-check --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "port-check: docker down -> DOCKER_DOWN" {
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow port-check --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DOCKER_DOWN" ]
}
