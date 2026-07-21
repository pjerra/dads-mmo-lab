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
  # -F: literal leading dot (same anti-pattern as the return-home grep) so a
  # dropped '.' can't slip through the regex wildcard.
  grep -qF '.character level Testen 42' "$FIXTURE/cap.txt"
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

# ---------- gm summon (bridge-backed, existence-checked, online-guarded) ----------

@test "gm summon fires dml_summon_npc and returns the npc name" {
  printf 'Auctioneer Beardo\n' > "$FIXTURE/npc.tsv"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/npc.tsv $FIXTURE/guid.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm summon --player Testen --entry 8661 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.summoned')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.entry')" = "8661" ]
  [ "$(echo "$output" | jq -r '.data.npc')" = "Auctioneer Beardo" ]
  grep -q 'dml_summon_npc Testen 8661' "$FIXTURE/cap.txt"
}

@test "gm summon rejects entry 0, 1000000 and non-numeric" {
  for bad in 0 1000000 abc; do
    run bash "$DML" wow gm summon --player Testen --entry "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "gm summon rejects an invalid character name" {
  run bash "$DML" wow gm summon --player 'x; drop' --entry 8661 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "gm summon unknown entry maps to NOT_FOUND before any SOAP fire" {
  printf '' > "$FIXTURE/nonpc.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/nonpc.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm summon --player Testen --entry 424242 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -q '424242'
  [ ! -s "$FIXTURE/cap.txt" ]
}

@test "gm summon maps a DB error to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow gm summon --player Testen --entry 8661 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "gm summon offline player maps to NOT_FOUND (after the entry check)" {
  printf 'World Banker\n' > "$FIXTURE/npc.tsv"
  printf '' > "$FIXTURE/noguid.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/npc.tsv $FIXTURE/noguid.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow gm summon --player Ghost --entry 5060 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -qi 'not online'
}

@test "gm summon maps a SOAP fault to SOAP_FAULT with the bridge-setup hint" {
  printf 'World Banker\n' > "$FIXTURE/npc.tsv"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/npc.tsv $FIXTURE/guid.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow gm summon --player Testen --entry 5060 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
  echo "$output" | grep -q 'bridge-setup'
}

@test "gm summon normalizes leading-zero entries (octal-safe)" {
  printf 'Auctioneer Beardo\n' > "$FIXTURE/npc.tsv"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/npc.tsv $FIXTURE/guid.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm summon --player Testen --entry 0008661 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.entry')" = "8661" ]
  grep -q 'dml_summon_npc Testen 8661' "$FIXTURE/cap.txt"
}

@test "gm validators reject octal-looking out-of-range values with BAD_ARG (no bash error leak)" {
  run bash "$DML" wow gm summon --player Testen --entry 081000000 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow gm level --player Testen --level 0999 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

# ---------- gm return-home (faction capital; online=SOAP tele, offline=DB) ----------
# Character lookup row: guid <TAB> race <TAB> online.

@test "gm return-home sends an ONLINE Horde char to Orgrimmar via the teleport console command" {
  printf '7\t2\t1\n' > "$FIXTURE/row.tsv"           # orc, online
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm return-home --char Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.sent_home')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.player')" = "Testen" ]
  [ "$(echo "$output" | jq -r '.data.capital')" = "Orgrimmar" ]
  [ "$(echo "$output" | jq -r '.data.via')" = "teleport" ]
  grep -qF 'teleport name Testen Orgrimmar' "$FIXTURE/cap.txt"
}

@test "gm return-home sends an ONLINE Alliance char to Stormwind via the teleport console command" {
  printf '5\t11\t1\n' > "$FIXTURE/row.tsv"          # draenei, online
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm return-home --char Milla --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.capital')" = "Stormwind" ]
  [ "$(echo "$output" | jq -r '.data.via')" = "teleport" ]
  grep -qF 'teleport name Milla Stormwind' "$FIXTURE/cap.txt"
}

@test "gm return-home writes an OFFLINE Horde char's position to the Orgrimmar coords (map 1)" {
  printf '7\t8\t0\n' > "$FIXTURE/row.tsv"           # troll, offline
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  use_curl_stub
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow gm return-home --char Testen --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.via')" = "db" ]
  [ "$(echo "$output" | jq -r '.data.capital')" = "Orgrimmar" ]
  update_line="$(grep UPDATE "$FIXTURE/query.log")"
  [[ "$update_line" == *"position_x=1609.2"* ]]
  [[ "$update_line" == *"position_y=-4407.7"* ]]
  [[ "$update_line" == *"position_z=17.5"* ]]
  [[ "$update_line" == *"map=1"* ]]
  [[ "$update_line" == *"orientation=0"* ]]
  [[ "$update_line" == *"WHERE guid=7"* ]]
  # offline path must never fire SOAP
  [ ! -f "$FIXTURE/curl.log" ]
}

@test "gm return-home writes an OFFLINE Alliance char's position to the Stormwind coords (map 0)" {
  printf '5\t1\t0\n' > "$FIXTURE/row.tsv"           # human, offline
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow gm return-home --char Milla --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.via')" = "db" ]
  [ "$(echo "$output" | jq -r '.data.capital')" = "Stormwind" ]
  update_line="$(grep UPDATE "$FIXTURE/query.log")"
  [[ "$update_line" == *"position_x=-8819.3"* ]]
  [[ "$update_line" == *"position_y=636.2"* ]]
  [[ "$update_line" == *"position_z=94.1"* ]]
  [[ "$update_line" == *"map=0"* ]]
  [[ "$update_line" == *"WHERE guid=5"* ]]
}

@test "gm return-home rejects an invalid character name" {
  run bash "$DML" wow gm return-home --char 'x; drop' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "gm return-home maps an unknown character to NOT_FOUND and an unknown race to an error" {
  printf '' > "$FIXTURE/none.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  run bash "$DML" wow gm return-home --char Ghost --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  printf '5\t9\t1\n' > "$FIXTURE/row.tsv"           # race 9: no WotLK faction
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  run bash "$DML" wow gm return-home --char Weird --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "gm return-home maps a SOAP fault (in combat / on flight) to SOAP_FAULT" {
  printf '7\t2\t1\n' > "$FIXTURE/row.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow gm return-home --char Testen --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "gm return-home maps 401 to SOAP_AUTH and curl exit 7 to SOAP_UNREACHABLE" {
  printf '7\t2\t1\n' > "$FIXTURE/row.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_HTTP=401
  run bash "$DML" wow gm return-home --char Testen --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
  unset DML_STUB_HTTP
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow gm return-home --char Testen --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}
