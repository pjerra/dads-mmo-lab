#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_curl_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "_parse_server_info extracts fields from live capture" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/40-config.sh"; _parse_server_info < "'"$BATS_TEST_DIRNAME"'/fixtures/server-info-live.txt"'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.online')" = "true" ]
  [ "$(echo "$output" | jq -r '.players')" = "1" ]
  [ "$(echo "$output" | jq -r '.uptime')" = "19 minute(s) 29 second(s)" ]
  [ "$(echo "$output" | jq -r '.mean_ms')" = "44" ]
  [ "$(echo "$output" | jq -r '.median_ms')" = "18" ]
  [[ "$(echo "$output" | jq -r '.version')" == 52f58186a533+* ]]
}

@test "server-info wraps the parsed object in an envelope" {
  # Build a SOAP <result> body around the live text so soap_exec extracts it.
  {
    printf '<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>'
    cat "$BATS_TEST_DIRNAME/fixtures/server-info-live.txt"
    printf '</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>'
  } > "$FIXTURE/si.xml"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/si.xml"
  run bash "$DML" wow server-info --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.online')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.players')" = "1" ]
}

@test "server-info reports online:false when SOAP is unreachable (not an error)" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-info --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.players')" = "null" ]
}

@test "server-info keeps SOAP_AUTH as an error" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow server-info --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
}

@test "server-info folds a SOAP fault into online:false (not an error)" {
  # Only rc=4 (unreachable, above) and rc=3 (auth, above) were pinned --
  # a fault (rc=2, e.g. "There is no such command") falls through the same
  # `*)` arm in 90-main.sh's server-info case and must also read as "down",
  # not as a CLI error.
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow server-info --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online')" = "false" ]
}
