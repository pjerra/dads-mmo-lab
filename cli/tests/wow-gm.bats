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

# ---------- gm gold / heal / revive (bridge-backed, online-guarded) ----------

@test "gm gold converts gold to copper and fires dml_gm_money" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm gold --player Testen --gold 5000 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.gold_set')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.gold')" = "5000" ]
  grep -q 'dml_gm_money Testen 50000000' "$FIXTURE/cap.txt"
}

@test "gm gold accepts the exact cap and rejects one over it" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm gold --player Testen --gold 214748 --json
  [ "$status" -eq 0 ]
  grep -q 'dml_gm_money Testen 2147480000' "$FIXTURE/cap.txt"
  run bash "$DML" wow gm gold --player Testen --gold 214749 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "gm gold rejects negative and non-numeric amounts" {
  for bad in -5 12.5 abc; do
    run bash "$DML" wow gm gold --player Testen --gold "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "gm gold/heal/revive are online-guarded (offline -> NOT_FOUND)" {
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  for sub in "gold --player Ghost --gold 5" "heal --player Ghost" "revive --player Ghost"; do
    run bash "$DML" wow gm $sub --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  done
}

@test "gm heal fires dml_gm_health <name> 100" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm heal --player Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.healed')" = "true" ]
  grep -q 'dml_gm_health Testen 100' "$FIXTURE/cap.txt"
}

@test "gm revive fires dml_gm_revive <name>" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm revive --player Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.revived')" = "true" ]
  grep -q 'dml_gm_revive Testen' "$FIXTURE/cap.txt"
}

@test "gm revive maps a SOAP fault to SOAP_FAULT with the bridge-setup hint" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow gm revive --player Testen --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
  echo "$output" | grep -q 'bridge-setup'
}
