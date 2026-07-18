#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_curl_stub
  use_mysql_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "account create: happy path sends exact console command" {
  cat > "$FIXTURE/resp.xml" <<'EOF'
<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>ok</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
EOF
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CAPTURE="$FIXTURE/sent.xml"
  run bash "$DML" wow account create --user Kiddo --pass secret1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.created')" = "true" ]
  grep -q 'account create Kiddo secret1' "$FIXTURE/sent.xml"
}

@test "account create: invalid user / pass rejected before SOAP" {
  export DML_STUB_CAPTURE="$FIXTURE/sent.xml"

  run bash "$DML" wow account create --user ab --pass secret1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/sent.xml" ]

  run bash "$DML" wow account create --user "has space" --pass secret1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/sent.xml" ]

  run bash "$DML" wow account create --user Kiddo --pass x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/sent.xml" ]

  run bash "$DML" wow account create --user Kiddo --pass "bad pass" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/sent.xml" ]
}

@test "account set-password: exact command with doubled pass" {
  cat > "$FIXTURE/resp.xml" <<'EOF'
<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>ok</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
EOF
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CAPTURE="$FIXTURE/sent.xml"
  run bash "$DML" wow account set-password --user Kiddo --pass newpass1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.password_set')" = "true" ]
  grep -q 'account set password Kiddo newpass1 newpass1' "$FIXTURE/sent.xml"
}

@test "account set-gm: exact command with -1 realm" {
  cat > "$FIXTURE/resp.xml" <<'EOF'
<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>ok</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
EOF
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CAPTURE="$FIXTURE/sent.xml"

  run bash "$DML" wow account set-gm --user Kiddo --level 2 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.gm_set')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.level')" = "2" ]
  grep -q 'account set gmlevel Kiddo 2 -1' "$FIXTURE/sent.xml"

  run bash "$DML" wow account set-gm --user Kiddo --level 5 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]

  run bash "$DML" wow account set-gm --user Kiddo --level abc --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "account create: SOAP fault surfaces as SOAP_FAULT" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow account create --user Kiddo --pass secret1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "account create: unreachable -> SOAP_UNREACHABLE" {
  printf 'x' > "$FIXTURE/resp.xml"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/resp.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow account create --user Kiddo --pass secret1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}

# Rows are: account_id, username, gm_level, guid, char_name, level (TSV;
# COALESCE(g.gmlevel,0) and the character LEFT JOIN misses coalesced to
# empty strings by the SQL -- see wow-accounts.bats for the pre-existing
# 5-column shape this SELECT superseded).
@test "accounts list: gm_level joined" {
  printf '1\tADMIN\t3\t\t\t\n2\tKid\t0\t7\tHypeer\t80\n' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].username')" = "ADMIN" ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].gm_level')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].gm_level')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters[0].name')" = "Hypeer" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters[0].level')" = "80" ]
}
