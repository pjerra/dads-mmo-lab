#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  use_curl_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "console-tail: default asks docker for --tail 200" {
  printf 'line one\nline two\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/args.log"
  run bash "$DML" wow console-tail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.available')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.lines | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.lines[1]')" = "line two" ]
  grep -q -- '--tail 200' "$FIXTURE/args.log"
}

@test "console-tail: --lines 50 passes --tail 50" {
  printf 'x\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/args.log"
  run bash "$DML" wow console-tail --lines 50 --json
  [ "$status" -eq 0 ]
  grep -q -- '--tail 50' "$FIXTURE/args.log"
}

@test "console-tail: leading-zero --lines normalizes to base-10" {
  printf 'x\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/args.log"
  run bash "$DML" wow console-tail --lines 050 --json
  [ "$status" -eq 0 ]
  grep -q -- '--tail 50' "$FIXTURE/args.log"
}

@test "console-tail: bad --lines values are BAD_ARG" {
  for v in 0 1001 abc; do
    run bash "$DML" wow console-tail --lines "$v" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "console-tail: ANSI escapes and CRs are stripped" {
  printf '\033[0m\033[36mWORLD: World Initialized\033[0m\r\n\033[?2004hAC> hello\r\n' > "$FIXTURE/log.txt"
  export DML_STUB_LOGS_FILE="$FIXTURE/log.txt"
  run bash "$DML" wow console-tail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.lines[0]')" = "WORLD: World Initialized" ]
  [ "$(echo "$output" | jq -r '.data.lines[1]')" = "AC> hello" ]
}

@test "console-tail: docker down -> available:false, exit 0" {
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow console-tail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.available')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.lines | length')" = "0" ]
}

@test "console-send: command text reaches the SOAP body" {
  cat > "$FIXTURE/resp.xml" <<'EOF'
<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>ok</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
EOF
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CAPTURE="$FIXTURE/sent.xml"
  run bash "$DML" wow console-send --command "server info" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.result')" = "ok" ]
  grep -q 'server info' "$FIXTURE/sent.xml"
}

@test "console-send: XML entities in the result are decoded" {
  cat > "$FIXTURE/resp.xml" <<'EOF'
<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>a &lt;b&gt; &quot;c&quot; &amp;d&#xD;
next</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
EOF
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  run bash "$DML" wow console-send --command "x" --json
  [ "$status" -eq 0 ]
  result="$(echo "$output" | jq -r '.data.result')"
  [[ "$result" == *'a <b> "c" &d'* ]]
  [[ "$result" == *'next'* ]]
}

@test "console-send: empty command is BAD_ARG" {
  run bash "$DML" wow console-send --command "   " --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "console-send: fault -> SOAP_FAULT" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow console-send --command "bogus" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "console-send: unreachable -> SOAP_UNREACHABLE" {
  printf 'x' > "$FIXTURE/resp.xml"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow console-send --command "server info" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}
