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

src() {
  source "$BATS_TEST_DIRNAME/../src/10-json.sh"
  source "$BATS_TEST_DIRNAME/../src/70-modules.sh"
}

@test "cpp registry has 19 rows incl. mod-custom-login; sql registry has 10 without xp-rates" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; _module_registry_cpp | wc -l; _module_registry_sql | wc -l; _module_registry_lua | wc -l'
  [ "${lines[0]}" = "19" ]
  [ "${lines[1]}" = "10" ]
  [ "${lines[2]}" = "9" ]
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; _module_registry_cpp | grep -c custom-login; _module_registry_sql | grep -c xp-rates || true'
  [ "${lines[0]}" = "1" ]
}

@test "_module_key_from_url derives and validates custom keys" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; _module_key_from_url "https://github.com/x/Mod-Cool-Thing.git"; echo; _module_key_from_url "https://github.com/x/not-a-module.git"; echo END'
  [ "${lines[0]}" = "mod-cool-thing" ]
  [ "${lines[1]}" = "END" ]
}

@test "rebuild-pending add is deduplicating and json renders it" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; s="'"$SDIR"'"; _rebuild_pending_add "$s" mod-aoe-loot; _rebuild_pending_add "$s" mod-aoe-loot; _rebuild_pending_add "$s" mod-transmog; _rebuild_pending_json "$s"'
  [ "$output" = '["mod-aoe-loot","mod-transmog"]' ]
}

@test "_module_conf_state walks none/needs-rebuild/ready/active" {
  mkdir -p "$SDIR/modules/mod-transmog/conf" "$SDIR/env/dist/etc/modules"
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; s="'"$SDIR"'"; _module_conf_state "$s" mod-junk-to-gold; _module_conf_state "$s" mod-transmog; touch "$s/modules/mod-transmog/conf/transmog.conf.dist"; _module_conf_state "$s" mod-transmog; touch "$s/env/dist/etc/modules/transmog.conf"; _module_conf_state "$s" mod-transmog'
  [ "${lines[0]}" = "none" ]
  [ "${lines[1]}" = "needs-rebuild" ]
  [ "${lines[2]}" = "ready" ]
  [ "${lines[3]}" = "active" ]
}

@test "module list: families + rebuild_pending + ale_ready" {
  mkdir -p "$SDIR/modules/mod-aoe-loot/.git" "$SDIR/modules/mod-ale/.git"
  bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; _rebuild_pending_add "'"$SDIR"'" mod-aoe-loot'
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp | length')" = "19" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-aoe-loot") | .installed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-aoe-loot") | .pending_rebuild')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-transmog") | .installed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.families.lua | length')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.families.sql | length')" = "10" ]
  [ "$(echo "$output" | jq -r '.data.ale_ready')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.rebuild_pending | length')" = "1" ]
  # Round R: every registry module carries a non-empty desc + a web url
  # (clone url minus .git) across all three families.
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-transmog") | .url')" = "https://github.com/azerothcore/mod-transmog" ]
  [ -n "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-transmog") | .desc')" ]
  [ "$(echo "$output" | jq -r '[.data.families.cpp[] | select(.custom==false) | .desc] | map(length > 0) | all')" = "true" ]
  [ "$(echo "$output" | jq -r '[.data.families.lua[] | .desc] | map(length > 0) | all')" = "true" ]
  [ "$(echo "$output" | jq -r '[.data.families.sql[] | .desc] | map(length > 0) | all')" = "true" ]
  # tweak_world sql mods have no repo -> url null
  [ "$(echo "$output" | jq -r '.data.families.sql[] | select(.key=="buff-mobs") | .url')" = "null" ]
}

@test "module list: custom clone appears with custom:true" {
  mkdir -p "$SDIR/modules/mod-my-thing/.git"
  run bash "$DML" wow module list --json
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-my-thing") | .custom')" = "true" ]
}

@test "module install cpp: clones shallow, marks rebuild pending, emits done" {
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow module install --family cpp --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  grep -q -- 'clone --depth 1 https://github.com/azerothcore/mod-aoe-loot.git' "$FIXTURE/git.log"
  [ -d "$SDIR/modules/mod-aoe-loot/.git" ]
  grep -qxF 'mod-aoe-loot' "$SDIR/.dml-rebuild-pending"
  echo "$output" | grep -q '"event":"done"'
  echo "$output" | grep -q '"rebuild_required":true'
}

@test "module install cpp: activates the conf the clone carried" {
  export DML_STUB_GIT_CLONE_CONF=mod_aoe_loot.conf
  run bash "$DML" wow module install --family cpp --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  [ -f "$SDIR/env/dist/etc/modules/mod_aoe_loot.conf" ]
  [ "$(cat "$SDIR/env/dist/etc/modules/mod_aoe_loot.conf")" = "DIST DEFAULTS" ]
  echo "$output" | grep -q 'Activated mod_aoe_loot.conf with defaults'
}

@test "module install cpp: an existing conf is left byte-identical" {
  mkdir -p "$SDIR/env/dist/etc/modules"
  printf 'USER EDIT\n' > "$SDIR/env/dist/etc/modules/mod_aoe_loot.conf"
  export DML_STUB_GIT_CLONE_CONF=mod_aoe_loot.conf
  run bash "$DML" wow module install --family cpp --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  [ "$(cat "$SDIR/env/dist/etc/modules/mod_aoe_loot.conf")" = "USER EDIT" ]
  [ "$(echo "$output" | grep -c 'Activated mod_aoe_loot.conf')" = 0 ]
}

@test "module install cpp: no .conf.dist in the clone -> no note, install still ok" {
  run bash "$DML" wow module install --family cpp --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  [ ! -f "$SDIR/env/dist/etc/modules/mod_aoe_loot.conf" ]
  [ "$(echo "$output" | grep -c 'Activated')" = 0 ]
  echo "$output" | grep -q '"event":"done"'
}

@test "module install cpp: already installed -> git pull (update)" {
  mkdir -p "$SDIR/modules/mod-aoe-loot/.git"
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow module install --family cpp --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  grep -q 'pull --depth 1' "$FIXTURE/git.log"
  echo "$output" | grep -q '"action":"updated"'
}

@test "module install cpp custom URL: key derived, bad names rejected" {
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow module install --family cpp --url https://github.com/x/mod-cool.git --json
  [ "$status" -eq 0 ]
  [ -d "$SDIR/modules/mod-cool/.git" ]
  run bash "$DML" wow module install --family cpp --url https://github.com/x/notamod.git --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}

@test "module install cpp: git failure -> GIT_FAILED, no pending mark" {
  export DML_STUB_GIT_EXIT=128
  run bash "$DML" wow module install --family cpp --key mod-transmog --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'GIT_FAILED'
  ! grep -qxF 'mod-transmog' "$SDIR/.dml-rebuild-pending" 2>/dev/null
}

@test "module install: unknown cpp key without url is BAD_ARG; backup flags rejected on cpp" {
  run bash "$DML" wow module install --family cpp --key mod-nope --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  run bash "$DML" wow module install --family cpp --key mod-aoe-loot --backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}

@test "module remove cpp: deletes clone, marks pending, arac warns" {
  mkdir -p "$SDIR/modules/mod-arac/.git"
  run bash "$DML" wow module remove --family cpp --key mod-arac --json
  [ "$status" -eq 0 ]
  [ ! -d "$SDIR/modules/mod-arac" ]
  grep -qxF 'mod-arac' "$SDIR/.dml-rebuild-pending"
  echo "$output" | grep -q 'data-only'
}

@test "module remove cpp: not installed -> NOT_FOUND" {
  run bash "$DML" wow module remove --family cpp --key mod-transmog --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "module install sql: family is implemented (see wow-module-sql.bats); missing backup flag is BAD_ARG" {
  run bash "$DML" wow module install --family sql --key lvl1-mounts --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}

@test "cpp install refuses an image-only server before cloning" {
  export DML_STUB_COMPOSE_CONFIG=nobuild
  run bash "$DML" wow module install --family cpp --key mod-transmog --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"MODULE_NO_BUILD_CONFIG"'
  [ ! -d "$SDIR/modules/mod-transmog" ]
}

@test "module list emits can_build" {
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.data.can_build == true'
}

@test "module list emits can_build:false on an image-only server" {
  export DML_STUB_COMPOSE_CONFIG=nobuild
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  echo "$output" | jq -e '.data.can_build == false'
}
