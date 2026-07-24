#!/usr/bin/env bats
# `wow module catalog`: the cheap, state-free sibling of `module list`. Emits
# the SAME .data.families.{cpp,lua,sql}[] arrays with every STATIC registry
# field only -- no filesystem probes, no git, no docker, no yq. Every dynamic
# per-row field is placeholdered, as are the top-level rebuild_pending ([]) and
# ale_ready (false), so the launcher Rust core can fetch the catalog once and
# fill live state itself, in-process. Custom-clone scanning is deliberately
# omitted (registry rows only).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  use_docker_stub
  use_git_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "module catalog: family row counts match module list registry rows (19/9/10)" {
  # No custom clones in the fixture, so list's cpp equals the registry count.
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.cpp | length')" = "19" ]
  [ "$(echo "$output" | jq -r '.data.families.lua | length')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.families.sql | length')" = "10" ]
  run bash "$DML" wow module catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.cpp | length')" = "19" ]
  [ "$(echo "$output" | jq -r '.data.families.lua | length')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.families.sql | length')" = "10" ]
}

@test "module catalog: static fields are byte-identical to module list per family" {
  # Strip each family's dynamic fields and compare the arrays. cpp additionally
  # filters to registry (custom==false) rows -- catalog never scans custom clones.
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  list_json="$output"
  run bash "$DML" wow module catalog --json
  [ "$status" -eq 0 ]
  cat_json="$output"

  lcpp="$(echo "$list_json" | jq -cS '[.data.families.cpp[] | select(.custom==false) | del(.installed,.pending_rebuild,.conf,.head,.head_date)]')"
  ccpp="$(echo "$cat_json"  | jq -cS '[.data.families.cpp[] | del(.installed,.pending_rebuild,.conf,.head,.head_date)]')"
  [ "$lcpp" = "$ccpp" ]

  llua="$(echo "$list_json" | jq -cS '[.data.families.lua[] | del(.cloned,.deployed,.warn)]')"
  clua="$(echo "$cat_json"  | jq -cS '[.data.families.lua[] | del(.cloned,.deployed,.warn)]')"
  [ "$llua" = "$clua" ]

  lsql="$(echo "$list_json" | jq -cS '[.data.families.sql[] | del(.installed)]')"
  csql="$(echo "$cat_json"  | jq -cS '[.data.families.sql[] | del(.installed)]')"
  [ "$lsql" = "$csql" ]
}

@test "module catalog: every dynamic field is placeholdered" {
  run bash "$DML" wow module catalog --json
  [ "$status" -eq 0 ]
  # cpp placeholders
  [ "$(echo "$output" | jq -r '[.data.families.cpp[] | select(.installed!=false or .pending_rebuild!=false or .conf!="" or .custom!=false or .head!=null or .head_date!=null)] | length')" = "0" ]
  # lua placeholders
  [ "$(echo "$output" | jq -r '[.data.families.lua[] | select(.cloned!=false or .deployed!=false or .warn!=null)] | length')" = "0" ]
  # sql placeholders
  [ "$(echo "$output" | jq -r '[.data.families.sql[] | select(.installed!=false)] | length')" = "0" ]
  # top-level placeholders
  [ "$(echo "$output" | jq -c '.data.rebuild_pending')" = "[]" ]
  [ "$(echo "$output" | jq -r '.data.ale_ready')" = "false" ]
  # spot-check concrete static values survive verbatim
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-transmog") | .conf_name')" = "transmog.conf" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-aoe-loot") | .url')" = "https://github.com/azerothcore/mod-aoe-loot" ]
  [ "$(echo "$output" | jq -r '.data.families.lua[] | select(.key=="accountwide") | .has_sql')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.families.sql[] | select(.key=="baby-mobs") | .type')" = "tweak_world" ]
  [ "$(echo "$output" | jq -r '.data.families.sql[] | select(.key=="baby-mobs") | .url')" = "null" ]
}

@test "module catalog: succeeds with NO yq, docker or git (never forks any)" {
  # A PATH shim makes yq, docker AND git fail hard; the catalog arm must still
  # answer the full registry without emitting any shim's stderr complaint.
  shim="$FIXTURE/failshim"
  mkdir -p "$shim"
  printf '#!/usr/bin/env bash\necho "yq must not run" >&2\nexit 3\n' > "$shim/yq"
  printf '#!/usr/bin/env bash\necho "docker must not run" >&2\nexit 3\n' > "$shim/docker"
  printf '#!/usr/bin/env bash\necho "git must not run" >&2\nexit 3\n' > "$shim/git"
  chmod +x "$shim/yq" "$shim/docker" "$shim/git"
  run env DML_YQ_BIN="$shim/yq" PATH="$shim:$PATH" bash "$DML" wow module catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp | length')" = "19" ]
  [ "$(echo "$output" | jq -r '.data.families.lua | length')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.families.sql | length')" = "10" ]
  [[ "$output" != *"yq must not run"* ]]
  [[ "$output" != *"docker must not run"* ]]
  [[ "$output" != *"git must not run"* ]]
}
