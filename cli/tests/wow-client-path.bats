#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

mk_client() { mkdir -p "$1"; touch "$1/Wow.exe"; }

@test "client-path get: unset -> path null" {
  run bash "$DML" wow client-path get --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.path')" = "null" ]
}

@test "client-path set: valid dir saved and get returns it" {
  mk_client "$FIXTURE/wowclient"
  run bash "$DML" wow client-path set --path "$FIXTURE/wowclient" --json
  [ "$status" -eq 0 ]
  run bash "$DML" wow client-path get --json
  [ "$(echo "$output" | jq -r '.data.path')" = "$FIXTURE/wowclient" ]
  [ "$(echo "$output" | jq -r '.data.valid')" = "true" ]
}

@test "client-path set: dir without client markers -> NOT_CLIENT" {
  mkdir -p "$FIXTURE/notwow"
  run bash "$DML" wow client-path set --path "$FIXTURE/notwow" --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_CLIENT'
}

@test "client-path set: missing dir -> BAD_PATH; Interface dir counts as a marker" {
  run bash "$DML" wow client-path set --path "$FIXTURE/nope" --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_PATH'
  mkdir -p "$FIXTURE/wow2/Interface"
  run bash "$DML" wow client-path set --path "$FIXTURE/wow2" --json
  [ "$status" -eq 0 ]
}

@test "client-path set: windows path converts to /mnt form" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; _client_win_to_wsl "C:\\Games\\WoW 3.3.5a"'
  [ "$output" = "/mnt/c/Games/WoW 3.3.5a" ]
}

@test "client-path get: saved path that vanished -> valid:false" {
  mk_client "$FIXTURE/gone"
  bash "$DML" wow client-path set --path "$FIXTURE/gone" --json >/dev/null
  rm -rf "$FIXTURE/gone"
  run bash "$DML" wow client-path get --json
  [ "$(echo "$output" | jq -r '.data.valid')" = "false" ]
}

@test "client-path detect: finds candidates under the scan roots" {
  mk_client "$FIXTURE/Games/World of Warcraft"
  export DML_CLIENT_SCAN_ROOTS="$FIXTURE/Games"
  run bash "$DML" wow client-path detect --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.candidates | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.candidates[0]')" = "$FIXTURE/Games/World of Warcraft" ]
}
