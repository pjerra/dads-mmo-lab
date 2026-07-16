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

@test "party online lists human online chars (rows come pre-filtered by SQL)" {
  # cols: guid, name, class, level
  printf '2503\tTesten\t8\t1\n' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.online[0].name')" = "Testen" ]
  [ "$(echo "$output" | jq -r '.data.online[0].class')" = "8" ]
}

@test "party online SQL excludes bot accounts" {
  printf '' > "$FIXTURE/on.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/on.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  run bash "$DML" wow party online --json
  [ "$status" -eq 0 ]
  grep -q 'playerbots_account_type' "$FIXTURE/q.log"
  grep -q 'online' "$FIXTURE/q.log"
}

@test "party online maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow party online --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "party add rejects a class outside the allowlist" {
  use_curl_stub
  run bash "$DML" wow party add --player Testen --class necromancer --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "party add rejects an offline/unknown player" {
  use_mysql_stub
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party add --player Ghost --class mage --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "party add confirms a join when a new group member appears" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  # Call 1 = online-guard lookup (player guid 2503).
  # Call 2 = pre-fire group snapshot (solo: just the player).
  # Call 3+ = post-fire poll (player + new bot guid 9001).
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/before.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after.tsv"
  # Bot name lookup for guid 9001.
  printf 'Botmage\n' > "$FIXTURE/botname.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/before.tsv $FIXTURE/after.tsv $FIXTURE/botname.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=3 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.added')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.bot')" = "Botmage" ]
}

@test "party add returns joined:false with a note when no member appears in time" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/solo.tsv"   # never grows
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/solo.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=2 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.note')" != "null" ]
}

@test "party add still succeeds (bot:null) when the bot-name lookup fails" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  use_mysql_stub
  # Same as the join case, but the name-lookup (call 4) returns empty rows,
  # simulating a transient ac-database blip: the bot has already joined, so
  # the add must still emit ONE success envelope (joined:true) with bot:null.
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '2503\n' > "$FIXTURE/before.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after.tsv"
  printf '' > "$FIXTURE/noname.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/before.tsv $FIXTURE/after.tsv $FIXTURE/noname.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=3 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class mage --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.joined')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.bot')" = "null" ]
}

@test "party add fires the correct bridge command over SOAP" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.xml"
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"; printf '2503\n' > "$FIXTURE/solo.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/solo.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  run bash "$DML" wow party add --player Testen --class druid --gender female --json
  [ "$status" -eq 0 ]
  cap="$(cat "$FIXTURE/cap.xml")"; cmd="${cap#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "dml_addclass Testen druid female" ]
}

@test "party list returns group members with bot flags" {
  use_mysql_stub
  printf '2503\n' > "$FIXTURE/guid.tsv"
  # members rows: guid, name, class, level, is_bot
  printf '2503\tTesten\t8\t1\t0\n9001\tBotmage\t8\t80\t1\n' > "$FIXTURE/mem.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/mem.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party list --player Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.members | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.members[1].name')" = "Botmage" ]
  [ "$(echo "$output" | jq -r '.data.members[1].is_bot')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.members[0].is_bot')" = "false" ]
  # jq -r prints bare true/false for BOTH a JSON boolean and the string
  # "true"/"false", so assert the TYPE too -- a regression to a quoted is_bot
  # would slip past the value asserts above.
  [ "$(echo "$output" | jq -r '.data.members[1].is_bot | type')" = "boolean" ]
  [ "$(echo "$output" | jq -r '.data.members[0].is_bot | type')" = "boolean" ]
}

@test "party list of an offline player is NOT_FOUND" {
  use_mysql_stub
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  run bash "$DML" wow party list --player Ghost --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
