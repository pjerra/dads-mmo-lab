#!/usr/bin/env bats
# `wow config tuning-registry`: the cheap, values-free sibling of `config
# tuning-list`. Emits the SAME .data.settings[] array with every STATIC
# registry field but reads NO values and does NO installed-state check (no
# conf/lua file reads, no yq, no docker, no git) so the launcher Rust core can
# fetch the registry once and fill value + installed itself, in-process.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "config tuning-registry: row count matches config tuning-list (13)" {
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "13" ]
  run bash "$DML" wow config tuning-registry --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "13" ]
}

@test "config tuning-registry: static fields are byte-identical to tuning-list" {
  # Both arms must produce byte-identical STATIC fields for every row, in the
  # same order. Strip the two dynamic fields (.value, .installed) and compare
  # the whole arrays.
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  list_static="$(echo "$output" | jq -cS '[.data.settings[] | del(.value,.installed)]')"
  run bash "$DML" wow config tuning-registry --json
  [ "$status" -eq 0 ]
  reg_static="$(echo "$output" | jq -cS '[.data.settings[] | del(.value,.installed)]')"
  [ "$list_static" = "$reg_static" ]
}

@test "config tuning-registry: value is empty and installed false for every row" {
  run bash "$DML" wow config tuning-registry --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '[.data.settings[] | select(.value != "")] | length')" = "0" ]
  [ "$(echo "$output" | jq -r '[.data.settings[] | select(.installed != false)] | length')" = "0" ]
  # spot-check a concrete static row survives verbatim (min/max/default/file)
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .min')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .max')" = "80" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .default')" = "10" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .file')" = "mod_npc_beastmaster.conf" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="sitmeansrest.duration") | .backend')" = "lua" ]
}

@test "config tuning-registry: succeeds with NO yq, docker or git (never forks any)" {
  # A PATH shim makes yq, docker AND git fail hard; the registry arm must still
  # answer 13 rows without emitting any shim's stderr complaint -- proving it
  # reads no values and probes no install state.
  shim="$FIXTURE/failshim"
  mkdir -p "$shim"
  printf '#!/usr/bin/env bash\necho "yq must not run" >&2\nexit 3\n' > "$shim/yq"
  printf '#!/usr/bin/env bash\necho "docker must not run" >&2\nexit 3\n' > "$shim/docker"
  printf '#!/usr/bin/env bash\necho "git must not run" >&2\nexit 3\n' > "$shim/git"
  chmod +x "$shim/yq" "$shim/docker" "$shim/git"
  run env DML_YQ_BIN="$shim/yq" PATH="$shim:$PATH" bash "$DML" wow config tuning-registry --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "13" ]
  [[ "$output" != *"yq must not run"* ]]
  [[ "$output" != *"docker must not run"* ]]
  [[ "$output" != *"git must not run"* ]]
}
