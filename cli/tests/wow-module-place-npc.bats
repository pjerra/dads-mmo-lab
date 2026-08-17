#!/usr/bin/env bats
# `wow module place-npc --key <mod>` (Batch 2 overnight): spawns an installed
# NPC-mod's creature in BOTH capitals (Stormwind map 0 / Orgrimmar map 1) from
# the ready-made coord blocks in 47-commands.sh (_cmd_block_for). Generalizes
# the battlepass-npc-fixit per-map pattern but is IDEMPOTENT PER MAP and never
# creates a creature_template (those ship with the module's own SQL). All
# statements are fixed literals with numeric-validated coords (sanctioned
# write #6).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  mkdir -p "$GDIR/modules/mod-transmog"
  use_mysql_stub
  export HOME="$FIXTURE"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/q.log"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  printf '0\n' > "$FIXTURE/zero.tsv"
  printf '1\n' > "$FIXTURE/one.tsv"
}
teardown() { teardown_fixture; }

@test "place-npc mod-transmog: full path -- both capital spawns + restart note" {
  # reads in order: template(exists) -> map0(empty) -> map1(empty).
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/one.tsv $FIXTURE/zero.tsv $FIXTURE/zero.tsv"
  run bash "$DML" wow module place-npc --key mod-transmog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.key')" = "mod-transmog" ]
  [ "$(echo "$output" | jq -r '.data.entry')" = "190010" ]
  [ "$(echo "$output" | jq -r '.data.spawns_placed')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.already_placed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  echo "$output" | jq -r '.data.note' | grep -qi 'restart'
  # both capital coordinates from the transmog cheat-sheet block landed.
  grep -q 'INSERT INTO creature (id, map' "$FIXTURE/q.log"
  grep -q -- '190010, 0, -8831.3, 628.2, 94.1, 3.7' "$FIXTURE/q.log"
  grep -q -- '190010, 1, 1595.0, -4401.5, 6.9, 4.5' "$FIXTURE/q.log"
}

@test "place-npc mod-transmog: per-map idempotence -- only the empty map is inserted" {
  # template exists, Stormwind (map 0) empty, Orgrimmar (map 1) already spawned.
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/one.tsv $FIXTURE/zero.tsv $FIXTURE/one.tsv"
  run bash "$DML" wow module place-npc --key mod-transmog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.spawns_placed')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.already_placed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.maps[0].placed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.maps[1].placed')" = "false" ]
  # only the Stormwind (map 0) coords were inserted; Orgrimmar's were not.
  grep -q -- '190010, 0, -8831.3, 628.2, 94.1, 3.7' "$FIXTURE/q.log"
  run grep -- '190010, 1, 1595.0, -4401.5, 6.9, 4.5' "$FIXTURE/q.log"
  [ "$status" -ne 0 ]
}

@test "place-npc mod-transmog: both maps already placed -> no writes" {
  # sticky-last single file => every count returns 1.
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/one.tsv"
  run bash "$DML" wow module place-npc --key mod-transmog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.spawns_placed')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.already_placed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  run grep 'INSERT INTO creature' "$FIXTURE/q.log"
  [ "$status" -ne 0 ]
}

@test "place-npc: missing creature_template -> NO_TEMPLATE, no writes" {
  # first read (template count) is 0.
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/zero.tsv"
  run bash "$DML" wow module place-npc --key mod-transmog --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NO_TEMPLATE" ]
  run grep 'INSERT INTO creature' "$FIXTURE/q.log"
  [ "$status" -ne 0 ]
}

@test "place-npc: module not installed -> NOT_INSTALLED" {
  rm -rf "$GDIR/modules/mod-transmog"
  run bash "$DML" wow module place-npc --key mod-transmog --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_INSTALLED" ]
}

@test "place-npc: db failure -> DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow module place-npc --key mod-transmog --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "place-npc: unknown/unsupported key -> BAD_ARG" {
  run bash "$DML" wow module place-npc --key mod-nope --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "place-npc: battlepass is redirected to fixit" {
  run bash "$DML" wow module place-npc --key battlepass --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  echo "$output" | grep -q 'fixit battlepass-npc'
}

@test "place-npc bmah (Lua family): deployed check + both spawns for entry 2069430" {
  rm -rf "$GDIR/modules/mod-transmog"
  mkdir -p "$GDIR/env/dist/etc/modules/lua_scripts"
  printf '%s\n' '-- BMAH' > "$GDIR/env/dist/etc/modules/lua_scripts/BMAH.lua"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/one.tsv $FIXTURE/zero.tsv $FIXTURE/zero.tsv"
  run bash "$DML" wow module place-npc --key bmah --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.entry')" = "2069430" ]
  [ "$(echo "$output" | jq -r '.data.spawns_placed')" = "2" ]
  grep -q -- '2069430, 0, -8816.3, 638.2, 94.1, 3.7' "$FIXTURE/q.log"
  grep -q -- '2069430, 1, 1597.5, -4404.5, 7.5, 4.5' "$FIXTURE/q.log"
}
