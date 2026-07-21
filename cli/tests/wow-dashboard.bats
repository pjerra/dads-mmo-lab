#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  # Sandbox ~/.dml: the saveall cooldown stamp (and soap.lock) must live in
  # the per-test fixture, never the real HOME -- a leftover real-HOME stamp
  # would make the saveall tests below flake across suite runs.
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "characters lists an account's chars with gold in gold-units" {
  export DML_STUB_ACCOUNT_ID=1
  printf '4\tPriesttest\t80\t5\t1\t0\t123456\n' > "$FIXTURE/chars.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/chars.tsv"
  run bash "$DML" wow characters --account admin --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.characters[0].name')" = "Priesttest" ]
  [ "$(echo "$output" | jq -r '.data.characters[0].level')" = "80" ]
  [ "$(echo "$output" | jq -r '.data.characters[0].gold')" = "12" ]
}

# Fixture is 17 tab-separated fields matching the NEW-SCHEMA SELECT (and its
# matching `read -r nm lvl cls money crace cgender skin face hstyle hcolor
# facial slot entry iname q ilvl disp`): name,level,class,money,race,gender,
# skin,face,hairStyle,hairColor,facialStyle,slot,entry,item_name,quality,
# item_level,displayid. Appearance fields are zeroed here since this test
# only asserts note/equipped/gold (appearance + the packed-playerBytes
# fallback are covered by wow-paperdoll-model.bats).
@test "paperdoll returns equipped items with note last_saved" {
  printf 'Priesttest\t80\t5\t123456\t0\t0\t0\t0\t0\t0\t0\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/pd.tsv"
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.note')" = "last_saved" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].name')" = "Hearthstone" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].item_level')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].displayid')" = "6418" ]
  [ "$(echo "$output" | jq -r '.data.gold')" = "12" ]
}

@test "paperdoll rejects a bad char name" {
  run bash "$DML" wow paperdoll --char 'no good' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

# --- Extra coverage beyond the brief -----------------------------------
# Error-path coverage mirroring the convention established by
# wow-items.bats / wow-teleport.bats: DB_UNREACHABLE on a failing query, and
# NOT_FOUND for an unresolvable account / character.

@test "characters requires --account" {
  run bash "$DML" wow characters --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "characters maps db failure to DB_UNREACHABLE" {
  export DML_STUB_ACCOUNT_ID=1
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow characters --account admin --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "characters maps an unknown account to NOT_FOUND" {
  : > "$FIXTURE/empty.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/empty.tsv"
  run bash "$DML" wow characters --account ghost --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "characters with non-numeric stubbed account id is rejected" {
  export DML_STUB_ACCOUNT_ID="1 OR 1=1"
  printf '4\tPriesttest\t80\t5\t1\t0\t123456\n' > "$FIXTURE/chars.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/chars.tsv"
  run bash "$DML" wow characters --account foo --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
  [ "$(echo "$output" | jq -r 'has("data")')" = "false" ]
}

@test "paperdoll maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

# ---------- online-gear freshness (smoke item 5) ----------
# Query order: 1 = the online lookup, 2 = the equipment SELECT.

@test "paperdoll fires a best-effort saveall BEFORE reading an ONLINE character" {
  printf '1\n' > "$FIXTURE/online.tsv"
  printf 'Priesttest\t80\t5\t123456\t0\t0\t0\t0\t0\t0\t0\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/online.tsv $FIXTURE/pd.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].name')" = "Hearthstone" ]
  grep -q 'saveall' "$FIXTURE/cap.txt"
}

@test "paperdoll for an ONLINE character still works when SOAP is down (saveall is best-effort)" {
  printf '1\n' > "$FIXTURE/online.tsv"
  printf 'Priesttest\t80\t5\t123456\t0\t0\t0\t0\t0\t0\t0\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/online.tsv $FIXTURE/pd.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].name')" = "Hearthstone" ]
  [ "$(echo "$output" | jq -r '.data.note')" = "last_saved" ]
}

@test "paperdoll saveall is rate-limited: a second read inside the cooldown skips the fire" {
  # Review follow-up: saveall saves EVERY online player, so browsing several
  # online characters must not fire one global save per view. Two full reads
  # in the same HOME -> exactly ONE saveall (default 30s window).
  printf '1\n' > "$FIXTURE/online.tsv"
  printf 'Priesttest\t80\t5\t123456\t0\t0\t0\t0\t0\t0\t0\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/online.tsv $FIXTURE/pd.tsv $FIXTURE/online.tsv $FIXTURE/pd.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/cap.txt"
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].name')" = "Hearthstone" ]
  [ "$(grep -c '<command>saveall</command>' "$FIXTURE/cap.txt")" -eq 1 ]
}

@test "paperdoll saveall cooldown 0 disables the throttle (fires on every read)" {
  printf '1\n' > "$FIXTURE/online.tsv"
  printf 'Priesttest\t80\t5\t123456\t0\t0\t0\t0\t0\t0\t0\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/online.tsv $FIXTURE/pd.tsv $FIXTURE/online.tsv $FIXTURE/pd.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/cap.txt"
  export DML_SAVEALL_COOLDOWN=0
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(grep -c '<command>saveall</command>' "$FIXTURE/cap.txt")" -eq 2 ]
}

@test "paperdoll saveall failure does not stamp the cooldown (next read retries)" {
  # Fire 1 fails (SOAP down) -> no stamp -> fire 2 retries once SOAP is back.
  printf '1\n' > "$FIXTURE/online.tsv"
  printf 'Priesttest\t80\t5\t123456\t0\t0\t0\t0\t0\t0\t0\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/online.tsv $FIXTURE/pd.tsv $FIXTURE/online.tsv $FIXTURE/pd.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ ! -f "$HOME/.dml/saveall.stamp" ]
  unset DML_STUB_CURL_EXIT
  export DML_STUB_CAPTURE="$FIXTURE/cap2.txt"
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  grep -q '<command>saveall</command>' "$FIXTURE/cap2.txt"
  [ -f "$HOME/.dml/saveall.stamp" ]
}

@test "paperdoll for an OFFLINE character never fires SOAP" {
  printf '0\n' > "$FIXTURE/online.tsv"
  printf 'Priesttest\t80\t5\t123456\t0\t0\t0\t0\t0\t0\t0\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/online.tsv $FIXTURE/pd.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].name')" = "Hearthstone" ]
  [ ! -f "$FIXTURE/curl.log" ]
}

@test "paperdoll maps an unknown character to NOT_FOUND" {
  : > "$FIXTURE/empty.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/empty.tsv"
  run bash "$DML" wow paperdoll --char Ghosttest --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "characters with valueless --account emits a BAD_ARG envelope, not an unbound-variable abort" {
  # Regression (final whole-plan review): see the same test in wow-items.bats.
  run bash "$DML" wow characters --account --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "paperdoll with valueless --char emits a BAD_ARG envelope, not an unbound-variable abort" {
  run bash "$DML" wow paperdoll --char --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
