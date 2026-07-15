#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_curl_stub
  export HOME="$FIXTURE"   # so ~/.dml/soap.lock lands in the fixture
}
teardown() { teardown_fixture; }

@test "soap_envelope escapes the command and targets urn:AC" {
  source "$BATS_TEST_DIRNAME/../src/20-soap.sh"
  run soap_envelope 'server info & <x>'
  [[ "$output" == *"urn:AC"* ]]
  [[ "$output" == *"executeCommand"* ]]
  [[ "$output" == *"server info &amp; &lt;x&gt;"* ]]
}

@test "soap_parse_result extracts result text" {
  source "$BATS_TEST_DIRNAME/../src/20-soap.sh"
  run soap_parse_result "$(cat "$BATS_TEST_DIRNAME/fixtures/soap-ok.xml")"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Console command executed."* ]]
}

@test "soap_parse_result returns 2 and faultstring on fault" {
  source "$BATS_TEST_DIRNAME/../src/20-soap.sh"
  run soap_parse_result "$(cat "$BATS_TEST_DIRNAME/fixtures/soap-fault.xml")"
  [ "$status" -eq 2 ]
  [[ "$output" == *"There is no such command"* ]]
}

@test "wow soap-exec returns result envelope on success" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow soap-exec "server info" --json
  [ "$status" -eq 0 ]
  # Assert the actual parsed <result> text from soap-ok.xml (not just
  # "not null") so a real parse regression (e.g. truncation, wrong tag,
  # whitespace mangling) is caught instead of masked.
  [ "$(echo "$output" | jq -r '.data.result')" = "Console command executed." ]
}

@test "wow soap-exec maps fault to SOAP_FAULT" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow soap-exec "bogus" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "wow soap-exec maps curl connection failure to SOAP_UNREACHABLE" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7   # curl: couldn't connect
  run bash "$DML" wow soap-exec "server info" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}

@test "wow soap-exec maps HTTP 401 to SOAP_AUTH" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow soap-exec "server info" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
}

@test "soap-exec with no command returns BAD_ARG envelope" {
  run bash "$DML" wow soap-exec --json
  [ "$status" -eq 1 ]
  # Must still be a valid JSON envelope, not a bare stderr message from an
  # unguarded ${1:?...} abort under set -euo pipefail.
  echo "$output" | jq -e . >/dev/null
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
