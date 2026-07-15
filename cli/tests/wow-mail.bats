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

@test "mail-item sends via SOAP and reports attachment count" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow mail-item --to Testchar --items 6948:1,19019:1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.sent')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.attachments')" = "2" ]
}

@test "mail-item rejects an invalid character name before calling SOAP" {
  run bash "$DML" wow mail-item --to 'bad name!' --items 6948:1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "mail-item rejects a malformed item spec" {
  run bash "$DML" wow mail-item --to Testchar --items 6948x1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "mail-item rejects more than 12 attachments" {
  spec="1:1,2:1,3:1,4:1,5:1,6:1,7:1,8:1,9:1,10:1,11:1,12:1,13:1"
  run bash "$DML" wow mail-item --to Testchar --items "$spec" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

# --- Extra coverage beyond the brief -----------------------------------
# soap_exec's success arm (Task 2) uses a *guarded* assignment
# (`if out="$(soap_exec "$cmd")"; then ... else ... fi`) specifically because
# 00-head.sh runs under `set -euo pipefail`: an unguarded `out="$(...)"; rc=$?`
# aborts the whole script the instant soap_exec returns non-zero (fault/auth/
# unreachable), before rc=$? or any case statement ever runs. These three
# tests pin mail-item's SOAP-error mapping to the same guarded pattern.

@test "mail-item maps SOAP fault to SOAP_FAULT" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow mail-item --to Testchar --items 6948:1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "mail-item maps HTTP 401 to SOAP_AUTH" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow mail-item --to Testchar --items 6948:1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
}

@test "mail-item maps curl connection failure to SOAP_UNREACHABLE" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow mail-item --to Testchar --items 6948:1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}
