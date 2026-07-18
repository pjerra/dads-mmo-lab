#!/usr/bin/env bats
load helpers/env.bash

# QoL batch Round I, Task 1: teleport-coords (offline DB write), gm at-login
# (SOAP flag commands), party preset-show/preset-import.

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"   # sandboxes ~/.dml/party-presets
  PDIR="$FIXTURE/.dml/party-presets"
}
teardown() { teardown_fixture; }

# ---------- teleport-coords ----------

@test "teleport-coords rejects a bad character name and bad map ids without touching the DB" {
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow teleport-coords --char 'x y' --map 0 --x 1 --y 2 --z 3 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  for bad_map in 1234 abc; do
    run bash "$DML" wow teleport-coords --char Testchar --map "$bad_map" --x 1 --y 2 --z 3 --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
  [ ! -s "$FIXTURE/query.log" ]
}

@test "teleport-coords rejects non-numeric and out-of-range coordinates without touching the DB" {
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  for bad_coord in 1e9 abc 25000; do
    run bash "$DML" wow teleport-coords --char Testchar --map 0 --x "$bad_coord" --y 2 --z 3 --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
  [ ! -s "$FIXTURE/query.log" ]
}

@test "teleport-coords maps an unknown character to NOT_FOUND" {
  printf '' > "$FIXTURE/none.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  run bash "$DML" wow teleport-coords --char Ghost --map 0 --x 1 --y 2 --z 3 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "teleport-coords rejects an online character (CHAR_ONLINE) and never runs the UPDATE" {
  printf '5\t1\n' > "$FIXTURE/row.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow teleport-coords --char Testchar --map 0 --x 1 --y 2 --z 3 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "CHAR_ONLINE" ]
  [ -s "$FIXTURE/query.log" ]
  ! grep -q UPDATE "$FIXTURE/query.log"
}

@test "teleport-coords happy path writes the UPDATE and returns the envelope" {
  printf '5\t0\n' > "$FIXTURE/row.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/row.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow teleport-coords --char Testchar --map 0 --x -100.5 --y 200 --z 30 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.teleported')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.char')" = "Testchar" ]
  [ "$(echo "$output" | jq -r '.data.map')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.x')" = "-100.5" ]
  [ "$(echo "$output" | jq -r '.data.y')" = "200" ]
  [ "$(echo "$output" | jq -r '.data.z')" = "30" ]
  update_line="$(grep UPDATE "$FIXTURE/query.log")"
  [[ "$update_line" == *"position_x="* ]]
  [[ "$update_line" == *"map="* ]]
  [[ "$update_line" == *"WHERE guid=5"* ]]
  [[ "$update_line" == *"orientation=0"* ]]
}

# ---------- gm at-login ----------

@test "gm at-login rejects an invalid flag without contacting SOAP" {
  use_curl_stub
  export DML_STUB_CAPTURE="$FIXTURE/cap.xml"
  run bash "$DML" wow gm at-login --player Testchar --flag rebirth --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -s "$FIXTURE/cap.xml" ]
}

@test "gm at-login sends the exact character command for each of the four flags" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.xml"
  for flag in rename customize changerace changefaction; do
    run bash "$DML" wow gm at-login --player Testchar --flag "$flag" --json
    [ "$status" -eq 0 ]
    [ "$(echo "$output" | jq -r '.data.applied')" = "true" ]
    [ "$(echo "$output" | jq -r '.data.flag')" = "$flag" ]
    captured="$(cat "$FIXTURE/cap.xml")"
    cmd="${captured#*<command>}"; cmd="${cmd%%</command>*}"
    [ "$cmd" = "character $flag Testchar" ]
  done
}

@test "gm at-login maps a SOAP fault to SOAP_FAULT" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow gm at-login --player Ghost --flag rename --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

# ---------- party preset-show ----------

@test "preset-show returns name and classes for an existing preset" {
  mkdir -p "$PDIR"; printf 'mage\npriest\n' > "$PDIR/dungeon-crew"
  run bash "$DML" wow party preset-show --name dungeon-crew --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.name')" = "dungeon-crew" ]
  [ "$(echo "$output" | jq -r '.data.classes | join(",")')" = "mage,priest" ]
}

@test "preset-show missing preset maps to NOT_FOUND" {
  run bash "$DML" wow party preset-show --name nosuch --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

# ---------- party preset-import ----------

@test "preset-import rejects a bad class token (nothing written), then writes LF one-per-line content on success" {
  run bash "$DML" wow party preset-import --name crew --classes 'mage,necromancer' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$PDIR/crew" ]
  run bash "$DML" wow party preset-import --name crew --classes 'mage,priest' --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.imported')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.classes | join(",")')" = "mage,priest" ]
  [ "$(cat "$PDIR/crew")" = "mage
priest" ]
}

@test "preset-import on an existing name requires --force, then overwrites with --force" {
  mkdir -p "$PDIR"; printf 'warrior\n' > "$PDIR/crew"
  run bash "$DML" wow party preset-import --name crew --classes 'mage,priest' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "EXISTS" ]
  [ "$(cat "$PDIR/crew")" = "warrior" ]
  run bash "$DML" wow party preset-import --name crew --classes 'mage,priest' --force --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.imported')" = "true" ]
  [ "$(cat "$PDIR/crew")" = "mage
priest" ]
}
