#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose   # Task 6: DB verbs resolve schema names from the title dir
  use_mysql_stub
  use_curl_stub
  export HOME="$FIXTURE"
  export DML_STUB_CURL_SEQ_STATE="$FIXTURE/curlseq"
  printf '{"name":"Thunderfury","quality":5,"icon":"inv_sword_39","tooltip":"<table><tr><td><b class=\\"q5\\">Thunderfury</b></td></tr></table>"}' > "$FIXTURE/wh.json"
  printf 'JPGDATA' > "$FIXTURE/icon.jpg"
  # Wowhead item XML fixtures (the display_id source): a real-shaped one, one
  # without the attribute, one with displayId="0" (gems etc.), and the
  # HTTP-200 "Item not found!" error body wowhead serves for unknown items.
  printf '<?xml version="1.0" encoding="UTF-8"?><wowhead><item id="32837"><name><![CDATA[Warglaive of Azzinoth]]></name><icon displayId="45150">inv_weapon_glave_01</icon></item></wowhead>' > "$FIXTURE/wg.xml"
  printf '<?xml version="1.0" encoding="UTF-8"?><wowhead><item id="19019"><name><![CDATA[Thunderfury]]></name><icon>inv_sword_39</icon></item></wowhead>' > "$FIXTURE/noattr.xml"
  printf '<?xml version="1.0" encoding="UTF-8"?><wowhead><item id="30606"><icon displayId="0">inv_jewelcrafting_talasite_03</icon></item></wowhead>' > "$FIXTURE/zerodid.xml"
  printf '<?xml version="1.0" encoding="UTF-8"?><wowhead><error>Item not found!</error></wowhead>' > "$FIXTURE/notfound.xml"
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
  # Three fetches now feed one uncached item: tooltip, icon, display-id XML
  # -- all three must land in the disk cache so the second call is curl-free.
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg $FIXTURE/wg.xml"
  bash "$DML" wow item-info --entries 19019 --json >/dev/null
  export DML_STUB_CURL_LOG="$FIXTURE/curl2.log"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ "$(echo "$output" | jq -r '.data.items[0].display_id')" = "45150" ]
  [ ! -f "$FIXTURE/curl2.log" ]
}

@test "item-info: display_id extracted from the wowhead item XML" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg $FIXTURE/wg.xml"
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow item-info --entries 32837 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ "$(echo "$output" | jq -r '.data.items[0].display_id')" = "45150" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/xml/32837.xml" ]
  # The XML comes from the main-site host with the &xml switch.
  grep -q 'wotlk/item=32837&xml' "$FIXTURE/curl.log"
}

@test "item-info: XML without a displayId attribute -> no display_id field" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg $FIXTURE/noattr.xml"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "wowhead" ]
  [ "$(echo "$output" | jq -r '.data.items[0].display_id')" = "null" ]
  # Valid wowhead XML is still cached even without the attribute.
  [ -f "$FIXTURE/.dml/wowhead-cache/xml/19019.xml" ]
}

@test "item-info: displayId=\"0\" (gems etc.) is treated as absent" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg $FIXTURE/zerodid.xml"
  run bash "$DML" wow item-info --entries 30606 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].display_id')" = "null" ]
}

@test "item-info: wowhead 200-with-<error> XML is cached as a definitive no" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg $FIXTURE/notfound.xml"
  run bash "$DML" wow item-info --entries 990001 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].display_id')" = "null" ]
  [ -f "$FIXTURE/.dml/wowhead-cache/xml/990001.xml" ]
}

@test "item-info: local fallback still carries display_id when only the tooltip host fails" {
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/wg.xml"
  export DML_STUB_HTTP_SEQ="404 200"
  printf 'Warglaive of Azzinoth\t5\t156\t0\t70\t214\t398\t2800\t0\t0\t0\t0\t0\t0\t0\t0\t0\t0\n' > "$FIXTURE/rows"
  export DML_STUB_DB_ROWS="$FIXTURE/rows"
  run bash "$DML" wow item-info --entries 32837 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].source')" = "local" ]
  [ "$(echo "$output" | jq -r '.data.items[0].display_id')" = "45150" ]
}

@test "item-info: non-wowhead XML body (CDN junk) is not cached and yields no display_id" {
  printf '<html>rate limited</html>' > "$FIXTURE/junk.html"
  export DML_STUB_CURL_SEQ="$FIXTURE/wh.json $FIXTURE/icon.jpg $FIXTURE/junk.html"
  run bash "$DML" wow item-info --entries 19019 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items[0].display_id')" = "null" ]
  [ ! -f "$FIXTURE/.dml/wowhead-cache/xml/19019.xml" ]
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
