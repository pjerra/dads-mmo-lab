#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  OVR="$GDIR/docker-compose.override.yml"
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AI_PLAYERBOT_MIN_RANDOM_BOTS: "1600"
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "2000"
EOF
}
teardown() { teardown_fixture; }

@test "config list returns the registry with live values and defaults" {
  # server.motd is DB-backed (acore_auth.motd; no conf/env var in this AC
  # build) -- its live value comes through the mysql stub. Everything else
  # reads the override env, falling back to the registry default.
  use_mysql_stub
  printf 'Live motd from db\n' > "$FIXTURE/motd.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/motd.tsv"
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  # bots.population reads the MAX env (2000); rates fall back to default "1"
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .value')" = "2000" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .value')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .default')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="server.motd") | .type')" = "text" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="server.motd") | .value')" = "Live motd from db" ]
  # motd applies live over SOAP -- the ONE row that never needs a restart
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="server.motd") | .restart_required')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .restart_required')" = "true" ]
}

@test "config list falls back to the motd default when the DB is down" {
  use_mysql_stub
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="server.motd") | .value')" = "Welcome to Dad's MMO Lab!" ]
}

@test "config set writes the env var and is idempotent" {
  run bash "$DML" wow config set --key rates.xp_kill --value 3 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  yq -e '.services.ac-worldserver.environment.AC_RATE_XP_KILL == "3"' "$OVR"
  # pre-existing env preserved (the duplicate-services-key regression guard)
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MIN_RANDOM_BOTS == "1600"' "$OVR"
  run bash "$DML" wow config set --key rates.xp_kill --value 3 --json
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
}

@test "config set bots.population writes BOTH min and max" {
  run bash "$DML" wow config set --key bots.population --value 500 --json
  [ "$status" -eq 0 ]
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MIN_RANDOM_BOTS == "500"' "$OVR"
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MAX_RANDOM_BOTS == "500"' "$OVR"
}

@test "config set rejects out-of-range and wrong-type values as BAD_ARG" {
  run bash "$DML" wow config set --key rates.xp_kill --value 21 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key rates.xp_kill --value abc --json
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key bots.autologin --value 2 --json
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key bots.population --value 3001 --json
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "config set rejects an unknown key as NOT_FOUND" {
  run bash "$DML" wow config set --key nope.nope --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "config set server.motd sanitizes the text and applies it live over SOAP" {
  # Motd has no env var: `set` sends `.server set motd` over SOAP instead.
  # Assert on the captured wire text (the curl stub records the XML body it
  # receives on stdin) -- quotes stripped, newline neutralized to a space,
  # text NOT quote-wrapped (AC's parser keeps quotes literal), and the
  # envelope reports changed:true / restart_required:false (applies live).
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/captured.xml"
  export HOME="$FIXTURE"
  run bash "$DML" wow config set --key server.motd --value $'Wel"come\nfriends' --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  [ -s "$DML_STUB_CAPTURE" ]
  captured="$(cat "$DML_STUB_CAPTURE")"
  cmd="${captured#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "server set motd 1 enUS Welcome friends" ]
}

@test "config set server.motd is SOAP_UNREACHABLE when the server is down" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  export HOME="$FIXTURE"
  run bash "$DML" wow config set --key server.motd --value Hello --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}

@test "config set ahbot.character resolves the char and writes GUID+ACCOUNT" {
  use_mysql_stub
  printf '2503\t253\n' > "$FIXTURE/char.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/char.tsv"
  run bash "$DML" wow config set --key ahbot.character --value Testen --json
  [ "$status" -eq 0 ]
  yq -e '.services.ac-worldserver.environment.AC_AUCTION_HOUSE_BOT_GUID == "2503"' "$OVR"
  yq -e '.services.ac-worldserver.environment.AC_AUCTION_HOUSE_BOT_ACCOUNT == "253"' "$OVR"
}

@test "config set ahbot.character with unknown char is NOT_FOUND" {
  use_mysql_stub
  printf '' > "$FIXTURE/char.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/char.tsv"
  run bash "$DML" wow config set --key ahbot.character --value Nobody --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "config errors NOT_FOUND when wow server absent and MISSING_DEP without yq" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  run bash "$DML" wow config list --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  add_game wow-server-playerbots compose
  run env DML_YQ_BIN=definitely-missing-yq bash "$DML" wow config list --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "MISSING_DEP" ]
}
