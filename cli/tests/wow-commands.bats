#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  use_docker_stub
  use_git_stub
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
}
teardown() { teardown_fixture; }

@test "wow commands: no mods installed -> empty mods array" {
  run bash "$DML" wow commands --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.mods | length')" = "0" ]
}

@test "wow commands: installed cpp mod with a command block is included" {
  mkdir -p "$SDIR/modules/mod-transmog/.git"
  run bash "$DML" wow commands --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.mods | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.mods[0].key')" = "mod-transmog" ]
  echo "$output" | jq -r '.data.mods[0].text' | grep -q '\.transmog sync'
}

# Every registry key (cpp/lua/sql) turns out to have a case entry in
# _cmd_block_for -- confirmed by diffing the registries in 70-modules.sh
# against the case table (mod-aoe-loot, suggested as a blockless example
# while writing this test, actually HAS a block: ".aoeloot on/off"). So the
# "installed but blockless" scenario is exercised with a CUSTOM cpp clone
# instead (a dir under modules/ with .git that is not in the registry) --
# a real path through the same custom-clone scan `wow commands` shares with
# `module list`, and it naturally has no matching case arm.
@test "wow commands: installed custom cpp clone without a block is excluded" {
  mkdir -p "$SDIR/modules/mod-my-custom-thing/.git"
  run bash "$DML" wow commands --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.mods | length')" = "0" ]
}

@test "wow commands: cloned lua script with a block is included" {
  mkdir -p "$SDIR/ale_scripts/lootpet/.git"
  run bash "$DML" wow commands --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.mods | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.mods[0].key')" = "lootpet" ]
  echo "$output" | jq -r '.data.mods[0].text' | grep -q 'Loot Pet (ALE)'
}
