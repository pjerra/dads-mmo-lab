#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  use_curl_stub
  export HOME="$FIXTURE"
  export DML_STUB_CURL_SEQ_STATE="$FIXTURE/curlseq"
  printf '{"name":"Thunderfury","quality":5,"icon":"inv_sword_39","tooltip":"<table><tr><td><b class=\\"q5\\">Thunderfury</b></td></tr></table>"}' > "$FIXTURE/wh.json"
  printf 'JPGDATA' > "$FIXTURE/icon.jpg"
}
teardown() { teardown_fixture; }

@test "item-info: entries validation" {
  run bash "$DML" wow item-info --entries "abc" --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  run bash "$DML" wow item-info --entries "$(seq 1 26 | tr '\n' ',' | sed 's/,$//')" --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'max 25'
}

@test "item-info: wowhead 200 -> embedded json + icon b64 + cache files" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ "$(echo "$output" | jq -r '.data.items[0].wowhead.name')" = "Thunderfury" ]
  [ "$(echo "$output" | jq -r '.data.items[0].icon')" = "inv_sword_39" ]
  [ "$(echo "$output" | jq -r '.data.items[0].icon_b64')" = "$(base64 -w0 < "$FIXTURE/icon.jpg")" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/tooltips/19019.json" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/icons/inv_sword_39.jpg" ]
}

@test "item-info: second call is served from cache (no curl)" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  bash "$DML" wow item-info --entries 19019 --json >/dev/null
  export DML_STUB_CURL_LOG="$FIXTURE/curl2.log"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ ! -f "$FIXTURE/curl2.log" ]
}

@test "item-info: 404 -> local fallback from item_template" {
  export DML_STUB_HTTP=404
  printf 'Casino Chip\t3\t80\t0\t0\t0\t0\t0\t7\t10\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 990001 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "local" ]
  [ "$(echo "$output" | jq -r '.data.items[0].name')" = "Casino Chip" ]
  echo "$output" | jq -r '.data.items[0].tooltip_html' | grep -q '+10 Stamina'
  echo "$output" | jq -r '.data.items[0].tooltip_html' | grep -q 'Item Level 80'
}

@test "item-info: curl dead + DB empty -> unavailable, verb still ok" {
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow item-info --entries 424242 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "unavailable" ]
}

@test "item-info: dedup" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg"
  run bash "$DML" wow item-info --entries 19019,19019,019019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items | length')" = "1" ]
}

@test "item-info: poisoned tooltip cache is dropped and falls back local" {
  mkdir -p "$FIXTURE/.dml/wowhead-cache/tooltips"
  printf '<html>error page</html>' > "$FIXTURE/.dml/wowhead-cache/tooltips/5555.json"
  export DML_STUB_HTTP=404
  printf 'X\t1\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 5555 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "local" ]
  [ ! -f "$FIXTURE/.dml/wowhead-cache/tooltips/5555.json" ]
}

@test "item-info: weapon damage line renders in local fallback" {
  export DML_STUB_HTTP=404
  printf 'Blade\t2\t20\t0\t15\t10\t20\t2600\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 777 --json
  html="$(echo "$output" | jq -r '.data.items[0].tooltip_html')"
  echo "$html" | grep -q '10 - 20 Damage'
  echo "$html" | grep -q 'Speed 2.60'
  echo "$html" | grep -q 'Requires Level 15'
}

@test "item-info: fractional weapon damage renders (float columns)" {
  export DML_STUB_HTTP=404
  printf 'Blade\t2\t20\t0\t15\t10.5\t20.5\t2600\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 778 --json
  [ "$status" -eq 0 ]
  html="$(echo "$output" | jq -r '.data.items[0].tooltip_html')"
  echo "$html" | grep -q '10.5 - 20.5 Damage'
  echo "$html" | grep -q 'Speed 2.60'
}

@test "item-info: brace-wrapped junk without wowhead fields is treated as poisoned" {
  printf '{"error":"nope"}' > "$FIXTURE/wh_junk.json"
  export DML_STUB_CURL_SEQ="$FIXTURE/wh_junk.json"
  printf 'X\t1\t1\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 8888 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "local" ]
  [ ! -f "$FIXTURE/.dml/wowhead-cache/tooltips/8888.json" ]
}
