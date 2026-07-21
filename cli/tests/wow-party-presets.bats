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

# ---------- preset-load (streaming) ----------

_done_data() { echo "$1" | grep '"event":"done"' | tail -1; }

@test "preset-load kicks current bots, adds each class, preps each joiner" {
  mkdir -p "$PDIR"; printf 'mage\npriest\n' > "$PDIR/crew"
  # SEQ call order: online-guid, kick-list (one old bot), then per class:
  # before-snapshot, wait-poll, joiner-name.
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf 'Oldbot\n' > "$FIXTURE/kicklist.tsv"
  printf '2503\n' > "$FIXTURE/before1.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after1.tsv"
  printf 'Botmage\n' > "$FIXTURE/name1.tsv"
  printf '2503\n9001\n' > "$FIXTURE/before2.tsv"
  printf '2503\n9001\n9002\n' > "$FIXTURE/after2.tsv"
  printf 'Botpriest\n' > "$FIXTURE/name2.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/kicklist.tsv $FIXTURE/before1.tsv $FIXTURE/after1.tsv $FIXTURE/name1.tsv $FIXTURE/before2.tsv $FIXTURE/after2.tsv $FIXTURE/name2.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/allcaps.txt"
  run bash "$DML" wow party preset-load --player Testen --name crew --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.loaded')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.requested')" = "2" ]
  [ "$(echo "$d" | jq -r '.data.joined')" = "2" ]
  grep -q 'dml_uninvite Oldbot' "$FIXTURE/allcaps.txt"
  # Review fix: the kick phase pairs every uninvite with a master `logout`
  # whisper (uninvite alone leaves the bot in-world following the player --
  # the exact smoke finding `kick`/`dismiss-all` already fixed).
  grep -q 'dml_whisper Testen Oldbot logout' "$FIXTURE/allcaps.txt"
  grep -q 'dml_addclass Testen mage' "$FIXTURE/allcaps.txt"
  grep -q 'dml_addclass Testen priest' "$FIXTURE/allcaps.txt"
  grep -q 'dml_whisper Testen Botmage talents autopick' "$FIXTURE/allcaps.txt"
  grep -q 'dml_whisper Testen Botmage autogear' "$FIXTURE/allcaps.txt"
  grep -q 'dml_whisper Testen Botpriest talents autopick' "$FIXTURE/allcaps.txt"
  # Second joiner gets BOTH whispers too.
  grep -q 'dml_whisper Testen Botpriest autogear' "$FIXTURE/allcaps.txt"
  # Ordering: the kick precedes its logout whisper, which precedes the first
  # add, which precedes its whispers (the capture file appends in call order).
  kick_line=$(grep -n 'dml_uninvite Oldbot' "$FIXTURE/allcaps.txt" | head -1 | cut -d: -f1)
  logout_line=$(grep -n 'dml_whisper Testen Oldbot logout' "$FIXTURE/allcaps.txt" | head -1 | cut -d: -f1)
  add_line=$(grep -n 'dml_addclass Testen mage' "$FIXTURE/allcaps.txt" | head -1 | cut -d: -f1)
  whisper_line=$(grep -n 'dml_whisper Testen Botmage talents autopick' "$FIXTURE/allcaps.txt" | head -1 | cut -d: -f1)
  [ "$kick_line" -lt "$logout_line" ]
  [ "$logout_line" -lt "$add_line" ]
  [ "$add_line" -lt "$whisper_line" ]
}

@test "preset-load kick phase skips an invalid DB-sourced bot name (defense-in-depth)" {
  # A hostile/corrupt row in the kick-list must never reach a SOAP command
  # string -- the loop re-checks _valid_charname like kick/dismiss-all do.
  mkdir -p "$PDIR"; printf 'mage\n' > "$PDIR/solo"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf 'Bad;Name\n' > "$FIXTURE/kicklist.tsv"
  printf '2503\n' > "$FIXTURE/before1.tsv"
  printf '2503\n' > "$FIXTURE/after1.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/kicklist.tsv $FIXTURE/before1.tsv $FIXTURE/after1.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/allcaps.txt"
  run bash "$DML" wow party preset-load --player Testen --name solo --json
  [ "$status" -eq 0 ]
  # The add phase still fired; the bad name produced NO uninvite/logout fire.
  grep -q 'dml_addclass Testen mage' "$FIXTURE/allcaps.txt"
  ! grep -q 'dml_uninvite' "$FIXTURE/allcaps.txt"
  ! grep -q 'Bad;Name' "$FIXTURE/allcaps.txt"
}

@test "preset-load missing preset emits a NOT_FOUND error event" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Testen --name nosuch --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"error"'
  echo "$output" | grep -q '"code":"NOT_FOUND"'
}

@test "preset-load offline player emits a NOT_FOUND error event" {
  mkdir -p "$PDIR"; printf 'mage\n' > "$PDIR/crew"
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Ghost --name crew --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"NOT_FOUND"'
}

@test "preset-load counts a non-attaching class as requested but not joined (warn path)" {
  mkdir -p "$PDIR"; printf 'mage\n' > "$PDIR/solo"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/kicklist.tsv"
  printf '2503\n' > "$FIXTURE/before1.tsv"
  printf '2503\n' > "$FIXTURE/after1.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/kicklist.tsv $FIXTURE/before1.tsv $FIXTURE/after1.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Testen --name solo --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.requested')" = "1" ]
  [ "$(echo "$d" | jq -r '.data.joined')" = "0" ]
  echo "$output" | grep -q '"level":"warn"'
}

@test "preset-load warns and skips unknown class lines (hand-edited file)" {
  mkdir -p "$PDIR"; printf 'necromancer\nmage\n' > "$PDIR/weird"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/kicklist.tsv"
  printf '2503\n' > "$FIXTURE/before1.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after1.tsv"
  printf 'Botmage\n' > "$FIXTURE/name1.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/kicklist.tsv $FIXTURE/before1.tsv $FIXTURE/after1.tsv $FIXTURE/name1.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Testen --name weird --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.requested')" = "1" ]
  echo "$output" | grep -q 'necromancer'
}
