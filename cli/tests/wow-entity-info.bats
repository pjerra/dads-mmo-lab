#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_curl_stub
  export HOME="$FIXTURE"
  export DML_STUB_CURL_SEQ_STATE="$FIXTURE/curlseq"
  printf '{"name":"Icy Veins","quality":0,"icon":"spell_frost_coldhearted","tooltip":"<b>Icy Veins</b>"}' > "$FIXTURE/wh.json"
  printf 'JPGDATA' > "$FIXTURE/icon.jpg"
}
teardown() { teardown_fixture; }

@test "entity-info: kind validation + ids validation" {
  run bash "$DML" wow entity-info --kind item --ids 123 --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'

  run bash "$DML" wow entity-info --kind spell --ids abc --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'

  run bash "$DML" wow entity-info --kind spell --ids "$(seq 1 26 | tr '\n' ',' | sed 's/,$//')" --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'max 25'
}

@test "entity-info: spell happy path + kind-prefixed cache file" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  run bash "$DML" wow entity-info --kind spell --ids 12472 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.entities[0].source')" = "wowhead" ]
  [ "$(echo "$output" | jq -r '.data.entities[0].wowhead.name')" = "Icy Veins" ]
  [ "$(echo "$output" | jq -r '.data.entities[0].icon')" = "spell_frost_coldhearted" ]
  [ "$(echo "$output" | jq -r '.data.entities[0].icon_b64')" = "$(base64 -w0 < "$FIXTURE/icon.jpg")" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/tooltips/spell-12472.json" ]
}

@test "entity-info: achievement kind caches under achievement- prefix" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  run bash "$DML" wow entity-info --kind achievement --ids 2336 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.entities[0].source')" = "wowhead" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/tooltips/achievement-2336.json" ]
}

@test "entity-info: 404 -> unavailable (no local fallback)" {
  export DML_STUB_HTTP=404
  run bash "$DML" wow entity-info --kind spell --ids 99999 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.entities[0].source')" = "unavailable" ]
}

@test "entity-info: cache hit skips curl" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  bash "$DML" wow entity-info --kind spell --ids 12472 --json >/dev/null
  export DML_STUB_CURL_LOG="$FIXTURE/curl2.log"
  run bash "$DML" wow entity-info --kind spell --ids 12472 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.entities[0].source')" = "wowhead" ]
  [ ! -f "$FIXTURE/curl2.log" ]
}

@test "entity-info: item-info regression canary" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/tooltips/19019.json" ]
}
