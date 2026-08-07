#!/usr/bin/env bats
# Batch 4 Feature 14: Guided Auction House page.
#   * `wow ahbot repair` -- faithful port of wow-manage.sh configure_ahbot
#     (account/GUID lookup + conf writes; the account/character creation
#     stays a MANUAL step surfaced in the envelope).
#   * conf:mod_ahbot.conf: registry rows (curated Auction House tab) --
#     round-trip through `config list`/`config set`, with the same legacy
#     env migration + live-apply semantics as the Batch 1 rates rows
#     (mod-ah-bot re-reads its conf on SOAP `reload config`, verified
#     against the deployed module source).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  MODS="$GDIR/env/dist/etc/modules"
  AHCONF="$MODS/mod_ahbot.conf"
  OVR="$GDIR/docker-compose.override.yml"
  mkdir -p "$MODS" "$GDIR/modules/mod-ah-bot"
  export HOME="$FIXTURE"
  # Keys below all EXIST in the deployed mod_ahbot.conf.dist (checked
  # 2026-07-19) -- the fixture dist mirrors that shape.
  cat > "$AHCONF.dist" <<'EOF'
# AH bot dist header comment
AuctionHouseBot.EnableSeller = 0
AuctionHouseBot.EnableBuyer = 0
AuctionHouseBot.Account = 0
AuctionHouseBot.GUID = 0
AuctionHouseBot.ItemsPerCycle = 200
AuctionHouseBot.DuplicatesCount = 0
AuctionHouseBot.ElapsingTimeClass = 1
AuctionHouseBot.VendorItems = 0
AuctionHouseBot.LootItems = 1
AuctionHouseBot.LootTradeGoods = 1
AuctionHouseBot.Bind_When_Equipped = 1
AuctionHouseBot.DisableItemsAboveLevel = 0
EOF
  use_docker_stub
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  # character lookup row: guid 7, account 42
  printf '7\t42\n' > "$FIXTURE/charrow.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/charrow.tsv"
}
teardown() { teardown_fixture; }

_done_line() { echo "$1" | grep '"event":"done"' | tail -1; }

@test "ahbot repair happy path: conf written, done event, applied live over reload config" {
  export DML_STUB_CAPTURE="$FIXTURE/captured.xml"
  run bash "$DML" wow ahbot repair --char Gasino --json
  [ "$status" -eq 0 ]
  repout="$output"
  grep -q '^AuctionHouseBot.Account = 42$' "$AHCONF"
  grep -q '^AuctionHouseBot.GUID = 7$' "$AHCONF"
  grep -q '^AuctionHouseBot.EnableSeller = 1$' "$AHCONF"
  grep -q '^AuctionHouseBot.EnableBuyer = 1$' "$AHCONF"
  # dist comment survives (created-from-dist then edited in place)
  grep -q '^# AH bot dist header comment$' "$AHCONF"
  d="$(_done_line "$repout")"
  [ "$(echo "$d" | jq -r '.data.repaired')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.already')" = "false" ]
  [ "$(echo "$d" | jq -r '.data.guid')" = "7" ]
  [ "$(echo "$d" | jq -r '.data.account')" = "42" ]
  [ "$(echo "$d" | jq -r '.data.module')" = "mod-ah-bot" ]
  [ "$(echo "$d" | jq -r '.data.applied')" = "live" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "false" ]
  # the manual account/character step is surfaced, never automated
  echo "$d" | jq -r '.data.manual_steps' | grep -qi 'create one character'
  # live apply went over SOAP as `reload config`
  captured="$(cat "$DML_STUB_CAPTURE")"
  cmd="${captured#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "reload config" ]
}

@test "ahbot repair second run is the already-configured path (no change, no reload)" {
  run bash "$DML" wow ahbot repair --char Gasino --json
  [ "$status" -eq 0 ]
  export DML_STUB_CURL_LOG="$FIXTURE/curl2.log"
  run bash "$DML" wow ahbot repair --char Gasino --json
  [ "$status" -eq 0 ]
  d="$(_done_line "$output")"
  [ "$(echo "$d" | jq -r '.data.already')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.applied')" = "none" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "false" ]
  echo "$output" | grep -q 'already configured'
  # no second reload was attempted
  [ ! -f "$DML_STUB_CURL_LOG" ]
}

@test "ahbot repair without mod-ah-bot installed -> NOT_INSTALLED" {
  rm -rf "$GDIR/modules/mod-ah-bot"
  run bash "$DML" wow ahbot repair --char Gasino --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"NOT_INSTALLED"'
}

@test "ahbot repair works with the plus fork installed instead (mod-ah-bot-plus)" {
  # Batch 2 (overnight): only mod-ah-bot-plus present -- repair must detect it
  # and write the PLUS fork's OWN key names, which differ from the original
  # mod-ah-bot's: GUIDs (plural list) and Buyer.Enabled, with NO Account key
  # (verified against each fork's conf/mod_ahbot.conf.dist, 2026-07-20).
  rm -rf "$GDIR/modules/mod-ah-bot"
  mkdir -p "$GDIR/modules/mod-ah-bot-plus"
  # Seed the plus fork's dist shape so the keys exist to edit in place.
  cat > "$AHCONF.dist" <<'EOF'
# AH bot plus dist header comment
AuctionHouseBot.EnableSeller = 0
AuctionHouseBot.Buyer.Enabled = 0
AuctionHouseBot.GUIDs =
EOF
  run bash "$DML" wow ahbot repair --char Gasino --json
  [ "$status" -eq 0 ]
  d="$(_done_line "$output")"
  [ "$(echo "$d" | jq -r '.data.repaired')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.module')" = "mod-ah-bot-plus" ]
  # plus-fork key names, singular Account/GUID/EnableBuyer NOT introduced
  grep -q '^AuctionHouseBot.GUIDs = 7$' "$AHCONF"
  grep -q '^AuctionHouseBot.EnableSeller = 1$' "$AHCONF"
  grep -q '^AuctionHouseBot.Buyer.Enabled = 1$' "$AHCONF"
  [ "$(grep -cE '^AuctionHouseBot\.Account' "$AHCONF")" = "0" ]
  [ "$(grep -cE '^AuctionHouseBot\.GUID ' "$AHCONF")" = "0" ]
  ! grep -qE '^AuctionHouseBot\.EnableBuyer' "$AHCONF"
}

@test "ahbot repair prefers the plus fork when both are somehow present" {
  mkdir -p "$GDIR/modules/mod-ah-bot-plus"
  run bash "$DML" wow ahbot repair --char Gasino --json
  [ "$status" -eq 0 ]
  d="$(_done_line "$output")"
  [ "$(echo "$d" | jq -r '.data.module')" = "mod-ah-bot-plus" ]
}

@test "ahbot repair with an unknown character -> NOT_FOUND carrying the manual steps" {
  : > "$FIXTURE/charrow.tsv"
  run bash "$DML" wow ahbot repair --char Nobody --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"NOT_FOUND"'
  echo "$output" | grep -qi 'log into the game with it once'
  # nothing was configured (the conf may exist -- created from its dist --
  # but every value is still the dist default)
  grep -q '^AuctionHouseBot.Account = 0$' "$AHCONF"
  grep -q '^AuctionHouseBot.GUID = 0$' "$AHCONF"
  grep -q '^AuctionHouseBot.EnableSeller = 0$' "$AHCONF"
}

@test "ahbot repair cleans legacy AC_ env overrides and reports restart (frozen env)" {
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AUCTION_HOUSE_BOT_GUID: "3"
      AC_AUCTION_HOUSE_BOT_ACCOUNT: "9"
      AC_RATE_XP_KILL: "3"
EOF
  run bash "$DML" wow ahbot repair --char Gasino --json
  [ "$status" -eq 0 ]
  d="$(_done_line "$output")"
  [ "$(echo "$d" | jq -r '.data.applied')" = "restart" ]
  [ "$(echo "$d" | jq -r '.data.restart_required')" = "true" ]
  run yq -e '.services.ac-worldserver.environment | has("AC_AUCTION_HOUSE_BOT_GUID")' "$OVR"
  [ "$status" -ne 0 ]
  run yq -e '.services.ac-worldserver.environment | has("AC_AUCTION_HOUSE_BOT_ACCOUNT")' "$OVR"
  [ "$status" -ne 0 ]
  # unrelated keys survive
  yq -e '.services.ac-worldserver.environment.AC_RATE_XP_KILL == "3"' "$OVR"
}

@test "config list serves the Auction House conf rows from the dist defaults" {
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="ahbot.items_per_cycle") | .group')" = "Auction House" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="ahbot.items_per_cycle") | .value')" = "200" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="ahbot.seller") | .env')" = "conf:mod_ahbot.conf:AuctionHouseBot.EnableSeller" ]
}

@test "Auction House conf row round-trips: set writes the conf and applies live" {
  run bash "$DML" wow config set --key ahbot.items_per_cycle --value 400 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "live" ]
  grep -q '^AuctionHouseBot.ItemsPerCycle = 400$' "$AHCONF"
  run bash "$DML" wow config list --json
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="ahbot.items_per_cycle") | .value')" = "400" ]
}

@test "ahbot.seller set migrates its legacy env key and reports restart" {
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AUCTION_HOUSE_BOT_ENABLE_SELLER: "1"
EOF
  run bash "$DML" wow config set --key ahbot.seller --value 1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  grep -q '^AuctionHouseBot.EnableSeller = 1$' "$AHCONF"
  run yq -e '.services.ac-worldserver.environment | has("AC_AUCTION_HOUSE_BOT_ENABLE_SELLER")' "$OVR"
  [ "$status" -ne 0 ]
}

@test "ahbot.character set resolves the name and writes BOTH conf keys" {
  run bash "$DML" wow config set --key ahbot.character --value Gasino --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  grep -q '^AuctionHouseBot.GUID = 7$' "$AHCONF"
  grep -q '^AuctionHouseBot.Account = 42$' "$AHCONF"
}
