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

@test "cpp registry has 18 rows incl. mod-custom-login; sql registry has 10 without xp-rates" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/70-modules.sh"; _module_registry_cpp | wc -l; _module_registry_sql | wc -l; _module_registry_lua | wc -l'
  [ "${lines[0]}" = "18" ]
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
