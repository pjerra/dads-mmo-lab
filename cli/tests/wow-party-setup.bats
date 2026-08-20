#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  export HOME="$FIXTURE"   # for ~/.dml/soap.lock
  export DML_LUA_DIR="$BATS_TEST_DIRNAME/../lua"   # deploy source ROOT (party/ + gm/)
}
teardown() { teardown_fixture; }

# bridge-setup streams NDJSON; the terminal `done` carries the data.
_done_data() { echo "$1" | grep '"event":"done"' | tail -1; }

@test "bridge-setup deploys ALL bridge scripts (party + gm) and reports restart_required" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow bridge-setup --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "true" ]
  # Eluna loads the deployed lua when the WORLD process starts -- a world-only
  # restart is enough, and saying so keeps the launcher from demanding a full
  # recreate for a script deploy.
  [ "$(echo "$d" | jq -r '.data.apply_needed')" = "world-restart" ]
  for f in dml_addclass.lua dml_uninvite.lua dml_login.lua dml_gm.lua; do
    [ -f "$GDIR/env/dist/etc/modules/lua_scripts/$f" ]
  done
}

@test "party-setup is an alias: identical deploy, same done payload" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party-setup --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.changed')" = "true" ]
  [ -f "$GDIR/env/dist/etc/modules/lua_scripts/dml_gm.lua" ]
}

@test "bridge-setup is idempotent: second run reports changed:false" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  bash "$DML" wow bridge-setup --json >/dev/null
  run bash "$DML" wow bridge-setup --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "false" ]
  [ "$(echo "$d" | jq -r '.data.apply_needed')" = "none" ]
}

@test "bridge-setup errors NOT_FOUND when the wow server is absent" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  use_curl_stub
  run bash "$DML" wow bridge-setup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"error"'
  echo "$output" | grep -q '"code":"NOT_FOUND"'
}

@test "bridge-setup errors SOAP_UNREACHABLE when the server can't be reached (preflight)" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow bridge-setup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"SOAP_UNREACHABLE"'
}

# --- ALE conf enforcement (the silent-bridge bug, found live 2026-08-20) ----
# mod_ale.conf ships `ALE.ScriptPath = "lua_scripts"`, a RELATIVE path the
# worldserver resolves against its cwd (/azerothcore), where nothing is -- so
# deployed bridges answer "Command does not exist" while the deploy reports
# success. bridge-setup now enforces the same two keys the Unbound installer
# enforces. Mirrors crates/dml-wow/src/bridge.rs::ensure_ale_conf tests.

@test "bridge-setup repairs a relative ALE.ScriptPath to the absolute container path" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  # Scripts deployed FIRST, so this run's restart_required can come ONLY from
  # the conf repair -- without this, the script deploy would set it too and a
  # deleted fold would stay green.
  bash "$DML" wow bridge-setup --json >/dev/null
  mkdir -p "$GDIR/env/dist/etc/modules"
  printf '# keep me\nALE.Enabled = true\nALE.ScriptPath = "lua_scripts"\nALE.AutoReload = false\n' \
    > "$GDIR/env/dist/etc/modules/mod_ale.conf"
  run bash "$DML" wow bridge-setup --json
  [ "$status" -eq 0 ]
  conf="$GDIR/env/dist/etc/modules/mod_ale.conf"
  grep -q '/azerothcore/env/dist/etc/modules/lua_scripts' "$conf"
  # The relative path must be GONE (LAST-line ! would assert; mid-test it
  # would not -- use a count instead, per the repo's bats rule).
  [ "$(grep -c '= "lua_scripts"' "$conf")" = 0 ]
  # Only the two required keys are touched: comments + user settings survive.
  grep -q '# keep me' "$conf"
  grep -q 'ALE.AutoReload = false' "$conf"
  # A conf repair demands the same world restart a script change does.
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.apply_needed')" = "world-restart" ]
}

@test "bridge-setup with a conf already in shape does not demand a restart for it" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  mkdir -p "$GDIR/env/dist/etc/modules"
  printf 'ALE.Enabled = 1\nALE.ScriptPath = "/azerothcore/env/dist/etc/modules/lua_scripts"\n' \
    > "$GDIR/env/dist/etc/modules/mod_ale.conf"
  bash "$DML" wow bridge-setup --json >/dev/null   # first run deploys scripts
  run bash "$DML" wow bridge-setup --json          # second run: nothing left to change
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "false" ]
}

@test "bridge-setup names a MISSING mod_ale.conf instead of inventing one" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow bridge-setup --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'mod_ale.conf not found'
  # The conf must NOT be created: inventing it would claim mod-ale is set up
  # on a server that does not have the module.
  [ "$(find "$GDIR/env/dist/etc/modules" -name mod_ale.conf 2>/dev/null | wc -l)" = 0 ]
}
