#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"   # sandboxes ~/.dml/party-presets
  PDIR="$FIXTURE/.dml/party-presets"
}
teardown() { teardown_fixture; }

@test "preset-save snapshots bot classes to a file and reports them" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '8\n5\n' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name dungeon-crew --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.saved')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.overwrote')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.bots | join(",")')" = "mage,priest" ]
  [ "$(cat "$PDIR/dungeon-crew")" = "mage
priest" ]
}

@test "preset-save skips unsupported class ids (deathknight 6)" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '8\n6\n5\n' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name crew --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.bots | length')" = "2" ]
  [ "$(grep -c . "$PDIR/crew")" = "2" ]
}

@test "preset-save over an existing name reports overwrote:true" {
  mkdir -p "$PDIR"; printf 'warrior\n' > "$PDIR/crew"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '8\n' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name crew --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.overwrote')" = "true" ]
  [ "$(cat "$PDIR/crew")" = "mage" ]
}

@test "preset-save with no bots in the party maps to NOT_FOUND" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name crew --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "preset-save rejects bad preset names" {
  for bad in 'a b' 'x;y' '../etc' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; do
    run bash "$DML" wow party preset-save --player Testen --name "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "preset-save offline player maps to NOT_FOUND" {
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  run bash "$DML" wow party preset-save --player Ghost --name crew --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "preset-list is empty when nothing is saved, then lists name+count" {
  run bash "$DML" wow party preset-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.presets | length')" = "0" ]
  mkdir -p "$PDIR"; printf 'mage\npriest\nwarrior\n' > "$PDIR/trio"
  run bash "$DML" wow party preset-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.presets[0].name')" = "trio" ]
  [ "$(echo "$output" | jq -r '.data.presets[0].bots')" = "3" ]
}

@test "preset-delete removes the file; deleting a missing preset maps to NOT_FOUND" {
  mkdir -p "$PDIR"; printf 'mage\n' > "$PDIR/tmp1"
  run bash "$DML" wow party preset-delete --name tmp1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.deleted')" = "true" ]
  [ ! -f "$PDIR/tmp1" ]
  run bash "$DML" wow party preset-delete --name tmp1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
