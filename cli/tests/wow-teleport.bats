#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "teleport-list returns rows from game_tele" {
  use_mysql_stub
  printf 'Stormwind\t-8960.0\t516.0\t96.3\t0\nOrgrimmar\t1633.0\t-4373.0\t31.3\t1\n' > "$FIXTURE/tele.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/tele.tsv"
  run bash "$DML" wow teleport-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.locations | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.locations[0].name')" = "Stormwind" ]
  [ "$(echo "$output" | jq -r '.data.locations[0].map')" = "0" ]
}

@test "teleport sends named SOAP command" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow teleport --char Testchar --to Stormwind --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.teleported')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.to')" = "Stormwind" ]
}

@test "teleport rejects a bad char name" {
  use_curl_stub
  run bash "$DML" wow teleport --char 'x y' --to Stormwind --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "teleport --coords is rejected as deferred" {
  use_curl_stub
  run bash "$DML" wow teleport --char Testchar --coords 1,2,3,0 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

# --- Extra coverage beyond the brief -----------------------------------
# teleport_list's DB failure path, and teleport's guarded soap_exec
# substitution + SOAP error mapping. soap_exec's success/fault/auth/
# unreachable classification only works because the surrounding assignment
# is GUARDED (`if out="$(soap_exec ...)"; then rc=0; else rc=$?; fi`) --
# 00-head.sh runs the whole built script under `set -euo pipefail`, so an
# unguarded `out="$(soap_exec "$cmd")"; rc=$?` aborts the script the instant
# soap_exec returns non-zero, before rc=$? or any case statement below it
# ever runs. These three tests pin teleport's SOAP-error mapping to the
# same guarded pattern already used by soap-exec and mail-item.

@test "teleport-list maps db failure to DB_UNREACHABLE" {
  use_mysql_stub
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow teleport-list --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "teleport maps SOAP fault to SOAP_FAULT" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow teleport --char Testchar --to Nowhere --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "teleport maps HTTP 401 to SOAP_AUTH" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow teleport --char Testchar --to Stormwind --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
}

@test "teleport maps curl connection failure to SOAP_UNREACHABLE" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow teleport --char Testchar --to Stormwind --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}

# --- Payload sanitization: assert on the actual SOAP command text ------
# The curl stub (helpers/env.bash) captures the request body it receives on
# stdin (the XML posted via --data-binary @-) to DML_STUB_CAPTURE when set --
# i.e. exactly what would go to the worldserver. These tests assert on that
# captured text rather than trusting the shell-level string ops in isolation.
# --to is a free-form location name that is used in BOTH a SQL query
# (teleport-list's --search, sql_escape'd) and a SOAP console command
# (teleport's --to) -- each context needs its own escaping. This proves the
# SOAP context strips the same injection surface mail-item already guards
# (a literal double quote, and an embedded newline that could otherwise be
# read by the worldserver as a second console command -- the AC #2695
# `.send items` crash class).

@test "teleport strips a double quote from --to out of the SOAP command" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/captured.xml"
  run bash "$DML" wow teleport --char Testchar --to 'Storm"wind' --json
  [ "$status" -eq 0 ]
  [ -s "$DML_STUB_CAPTURE" ]
  captured="$(cat "$DML_STUB_CAPTURE")"
  cmd="${captured#*<command>}"; cmd="${cmd%%</command>*}"
  # Quote stripped: the raw injected quote never reaches the wire...
  [[ "$cmd" != *'Storm"wind'* ]]
  # ...it arrives stripped down to Stormwind, still correctly quoted by the CLI.
  [[ "$cmd" == *'"Stormwind"'* ]]
}

@test "teleport neutralizes an embedded newline in --to instead of letting it inject a second console command" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/captured.xml"
  run bash "$DML" wow teleport --char Testchar \
    --to $'Stormwind\n.server shutdown 1' --json
  [ "$status" -eq 0 ]
  [ -s "$DML_STUB_CAPTURE" ]
  captured="$(cat "$DML_STUB_CAPTURE")"
  cmd="${captured#*<command>}"; cmd="${cmd%%</command>*}"
  # No raw newline reached the wire: the whole <command>...</command> is one line.
  [ "$(printf '%s' "$cmd" | wc -l)" -eq 0 ]
  # The injected ".server shutdown 1" stayed part of the --to argument, on
  # the same line/text as "Stormwind" -- it was neutralized to a space, not
  # treated as a separate command.
  [[ "$cmd" == *'Stormwind .server shutdown 1'* ]]
}
