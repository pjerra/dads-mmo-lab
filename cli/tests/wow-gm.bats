#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

# ---------- gm level (stock SOAP, no bridge, no online-guard) ----------

@test "gm level sets a level over plain SOAP and reports the new level" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm level --player Testen --level 42 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.leveled')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.player')" = "Testen" ]
  [ "$(echo "$output" | jq -r '.data.level')" = "42" ]
  grep -q '.character level Testen 42' "$FIXTURE/cap.txt"
}

@test "gm level does NOT need the DB (works for offline chars)" {
  export DML_STUB_DB_EXIT=1
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow gm level --player Testen --level 10 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.leveled')" = "true" ]
}

@test "gm level rejects an invalid character name" {
  run bash "$DML" wow gm level --player 'x; drop' --level 10 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "gm level rejects level 0, 256 and non-numeric" {
  for bad in 0 256 abc; do
    run bash "$DML" wow gm level --player Testen --level "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "gm level maps a SOAP fault (unknown char) to SOAP_FAULT" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow gm level --player Ghost --level 10 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "gm level maps 401 to SOAP_AUTH and curl exit 7 to SOAP_UNREACHABLE" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_HTTP=401
  run bash "$DML" wow gm level --player Testen --level 10 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
  unset DML_STUB_HTTP
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow gm level --player Testen --level 10 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}

@test "gm rejects an unknown subcommand" {
  run bash "$DML" wow gm smite --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "UNKNOWN_COMMAND" ]
}
