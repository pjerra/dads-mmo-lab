#!/usr/bin/env bats
# Batch 5 F5 follow-up: `wow party specs` reads the LIVE premade specs straight
# from the deployed playerbots.conf, and that same conf drives _valid_bot_spec
# (via `party add --spec`) so the picker and the validator can never drift.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  MODS="$SDIR/env/dist/etc/modules"
  mkdir -p "$MODS"
  touch "$SDIR/docker-compose.yml"   # so _wow_server_dir resolves this dir
}
teardown() { teardown_fixture; }

# A small but representative conf: warrior (with a level-60 + level-80 link so
# the highest wins), a mage incl a CUSTOM name absent from the static mirror,
# and a death-knight block (class 6) that MUST be excluded. No warlock at all,
# so a static-allowlist warlock spec must be rejected once the conf drives it.
seed_conf() {
  cat > "$MODS/playerbots.conf" <<'EOF'
####################################################################################################
# WARRIOR
#
AiPlayerbot.PremadeSpecName.1.0 = arms pve
AiPlayerbot.PremadeSpecGlyph.1.0 = 43418,43395,43423,43399,43397,43421
AiPlayerbot.PremadeSpecLink.1.0.60 = 3022032023335100002012211231241
AiPlayerbot.PremadeSpecLink.1.0.80 = 3022032023335100102012213231251-305-2033
####################################################################################################
# MAGE
#
AiPlayerbot.PremadeSpecName.8.2 = frost pve
AiPlayerbot.PremadeSpecLink.8.2.80 = 23000503110003
AiPlayerbot.PremadeSpecName.8.9 = custom test pve
####################################################################################################
# DEATH KNIGHT (class 6 -- must be EXCLUDED everywhere)
#
AiPlayerbot.PremadeSpecName.6.0 = blood pve
AiPlayerbot.PremadeSpecLink.6.0.80 = 2105440023-3050050002005-005
EOF
}

@test "party specs parses live names + highest-level link + tree distribution" {
  seed_conf
  run bash "$DML" wow party specs --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "playerbots.conf" ]
  # 3 non-DK rows (arms pve, frost pve, custom test pve); DK excluded.
  [ "$(echo "$output" | jq -r '.data.specs | length')" = "3" ]
  # arms pve carries class_id 1, class name, and its LEVEL-80 link's tree
  # distribution (55/8/8), not the level-60 link.
  arms="$(echo "$output" | jq -c '.data.specs[] | select(.name=="arms pve")')"
  [ "$(echo "$arms" | jq -r '.class_id')" = "1" ]
  [ "$(echo "$arms" | jq -r '.class')" = "warrior" ]
  [ "$(echo "$arms" | jq -r '.link')" = "3022032023335100102012213231251-305-2033" ]
  [ "$(echo "$arms" | jq -r '.tree')" = "55/8/8" ]
  # the custom (non-mirror) name is surfaced too
  [ "$(echo "$output" | jq -r '.data.specs[] | select(.name=="custom test pve") | .class_id')" = "8" ]
}

@test "party specs excludes death-knight (class 6) specs" {
  seed_conf
  run bash "$DML" wow party specs --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '[.data.specs[] | select(.class_id==6)] | length')" = "0" ]
  [ "$(echo "$output" | jq -r '[.data.specs[] | select(.name=="blood pve")] | length')" = "0" ]
}

@test "party specs falls back to the shipped .dist when the live conf is absent" {
  seed_conf
  mv "$MODS/playerbots.conf" "$MODS/playerbots.conf.dist"
  run bash "$DML" wow party specs --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "playerbots.conf.dist" ]
  [ "$(echo "$output" | jq -r '.data.specs | length')" = "3" ]
}

@test "party specs is NOT_FOUND when no conf exists at all" {
  run bash "$DML" wow party specs --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "a spec present in the live conf but absent from the static mirror is accepted" {
  # "custom test pve" is not in _valid_bot_spec's static fallback -- accepting
  # it proves validation reads the conf. It passes spec-validation, then fails
  # only at the online-guard (empty online result => NOT_FOUND), never BAD_ARG.
  seed_conf
  use_mysql_stub
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party add --player Testen --class mage --spec "custom test pve" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "a static-mirror spec that the live conf does NOT define is rejected" {
  # The seeded conf has no warlock block, so "affli pve" (a static-mirror
  # member) must be rejected once the conf drives validation -- proving it is
  # conf-driven, not the static fallback. Rejected up front (BAD_ARG, no SOAP).
  seed_conf
  use_curl_stub
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow party add --player Testen --class warlock --spec "affli pve" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/curl.log" ]
}

@test "spec validation rejects injection metacharacters before touching the conf" {
  seed_conf
  use_curl_stub
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow party add --player Testen --class mage --spec "frost pve; .server shutdown" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -f "$FIXTURE/curl.log" ]
}
