#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  export HOME="$FIXTURE"   # for ~/.dml/soap.lock
  export DML_LUA_DIR="$BATS_TEST_DIRNAME/../lua/party"   # deploy source = repo scripts
}
teardown() { teardown_fixture; }

# party-setup streams NDJSON; the terminal `done` carries the data.
_done_data() { echo "$1" | grep '"event":"done"' | tail -1; }

@test "party-setup deploys the 3 bridge scripts and reports restart_required" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party-setup --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "true" ]
  for f in dml_addclass.lua dml_uninvite.lua dml_login.lua; do
    [ -f "$GDIR/env/dist/etc/modules/lua_scripts/$f" ]
  done
}

@test "party-setup is idempotent: second run reports changed:false" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  bash "$DML" wow party-setup --json >/dev/null
  run bash "$DML" wow party-setup --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "false" ]
}

@test "party-setup errors NOT_FOUND when the wow server is absent" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  use_curl_stub
  run bash "$DML" wow party-setup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"error"'
  echo "$output" | grep -q '"code":"NOT_FOUND"'
}

@test "party-setup errors SOAP_UNREACHABLE when the server can't be reached (preflight)" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow party-setup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"SOAP_UNREACHABLE"'
}
