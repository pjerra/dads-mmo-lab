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
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "58" ]
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

@test "config set writes the env var and is idempotent (non-rate env rows unchanged)" {
  # The rates rows migrated to conf-file rows (see wow-config-conf.bats);
  # ahbot.seller proves the ORIGINAL env mechanism still works untouched.
  run bash "$DML" wow config set --key ahbot.seller --value 1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  yq -e '.services.ac-worldserver.environment.AC_AUCTION_HOUSE_BOT_ENABLE_SELLER == "1"' "$OVR"
  # pre-existing env preserved (the duplicate-services-key regression guard)
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MIN_RANDOM_BOTS == "1600"' "$OVR"
  run bash "$DML" wow config set --key ahbot.seller --value 1 --json
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
}

@test "config set bots.population writes BOTH min and max" {
  # Migrated to a playerbots.conf row (Batch 1 F2) -- the full env-removal
  # matrix lives in wow-config-pb.bats; this keeps the one-number-two-keys
  # contract pinned.
  mkdir -p "$GDIR/env/dist/etc/modules"
  printf 'AiPlayerbot.MinRandomBots = 500\nAiPlayerbot.MaxRandomBots = 500\n' \
    > "$GDIR/env/dist/etc/modules/playerbots.conf"
  run bash "$DML" wow config set --key bots.population --value 900 --json
  [ "$status" -eq 0 ]
  grep -q '^AiPlayerbot.MinRandomBots = 900$' "$GDIR/env/dist/etc/modules/playerbots.conf"
  grep -q '^AiPlayerbot.MaxRandomBots = 900$' "$GDIR/env/dist/etc/modules/playerbots.conf"
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

@test "config raw-read returns file content; unknown name is NOT_FOUND" {
  printf 'FOO=bar\nBAZ=1\n' > "$GDIR/.env"
  run bash "$DML" wow config raw-read --file .env --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.file')" = ".env" ]
  # NB: $(cat file) inside the arm strips the trailing newline before
  # json_escape, and $(...) here strips it again -- compare without it.
  [ "$(echo "$output" | jq -r '.data.content')" = $'FOO=bar\nBAZ=1' ]
  run bash "$DML" wow config raw-read --file worldserver.conf --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  run bash "$DML" wow config raw-read --file ../../../etc/passwd --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "config raw-write writes via stdin and keeps a .bak of the old content" {
  mkdir -p "$GDIR/env/dist/etc/modules"
  printf 'old=1\n' > "$GDIR/env/dist/etc/modules/playerbots.conf"
  run bash -c 'printf "new=2\n" | bash "'"$DML"'" wow config raw-write --file playerbots.conf --json'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.written')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.backup')" = "playerbots.conf.bak" ]
  [ "$(cat "$GDIR/env/dist/etc/modules/playerbots.conf")" = "new=2" ]
  [ "$(cat "$GDIR/env/dist/etc/modules/playerbots.conf.bak")" = "old=1" ]
}

@test "config raw-write of a brand-new file reports backup null" {
  # Since the dynamic allowlist (Batch 1 F3) a conf name is only editable
  # when the conf OR its .dist exists -- seed the dist, first write creates
  # the conf.
  mkdir -p "$GDIR/env/dist/etc/modules"
  rm -f "$GDIR/env/dist/etc/modules/mod_ale.conf"
  printf 'Ale.Enabled = 1\n' > "$GDIR/env/dist/etc/modules/mod_ale.conf.dist"
  run bash -c 'printf "x=1\n" | bash "'"$DML"'" wow config raw-write --file mod_ale.conf --json'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.backup')" = "null" ]
}

@test "config raw-write rejects invalid YAML for the override and leaves it untouched" {
  printf 'services: {}\n' > "$OVR"
  run bash -c 'printf "services: [unclosed\n" | bash "'"$DML"'" wow config raw-write --file docker-compose.override.yml --json'
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ "$(cat "$OVR")" = "services: {}" ]
  [ ! -f "$OVR.bak" ]
  [[ "$(echo "$output" | jq -r '.error.message')" == *"valid YAML"* ]]
}

# --- Advanced Files read-only guard (final-review security fix) -----------
# raw-write over .env or docker-compose.override.yml, combined with games
# restart, would let the Advanced Files editor drive host command execution
# (Docker Compose env/volume/entrypoint injection). Both stay READABLE via
# raw-read but must be REJECTED before any write, ahead of the tmp-file
# write and (for the override) the YAML validation step.

@test "config raw-write refuses to overwrite .env (read-only in the editor)" {
  printf 'FOO=bar\n' > "$GDIR/.env"
  run bash -c 'printf "EVIL=1\n" | bash "'"$DML"'" wow config raw-write --file .env --json'
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ "$(cat "$GDIR/.env")" = "FOO=bar" ]
}

@test "config raw-write refuses to overwrite docker-compose.override.yml (read-only in the editor)" {
  printf 'services: {}\n' > "$OVR"
  # Submit DIFFERENT, syntactically VALID YAML (the real attack shape: a
  # valid override injecting a privileged service) so the unchanged-check
  # below is load-bearing -- it proves the read-only guard blocks even
  # well-formed content, not just that stdin happened to equal the file.
  run bash -c 'printf "services:\n  ac-worldserver:\n    privileged: true\n" | bash "'"$DML"'" wow config raw-write --file docker-compose.override.yml --json'
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ "$(cat "$OVR")" = "services: {}" ]
}

@test "config raw-write of playerbots.conf still works after the .env/override read-only guard" {
  mkdir -p "$GDIR/env/dist/etc/modules"
  printf 'old=1\n' > "$GDIR/env/dist/etc/modules/playerbots.conf"
  run bash -c 'printf "new=2\n" | bash "'"$DML"'" wow config raw-write --file playerbots.conf --json'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.written')" = "true" ]
  [ "$(cat "$GDIR/env/dist/etc/modules/playerbots.conf")" = "new=2" ]
}

# --- server.motd SOAP error arms (final-review pin) ------------------------
# `config set server.motd` folds its SOAP result through the same soap_exec
# rc mapping as soap-exec/teleport (see 90-main.sh) -- only the UNREACHABLE
# arm had a test. These mirror wow-teleport.bats's fault/401 tests.

@test "config set server.motd maps SOAP fault to SOAP_FAULT" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  export HOME="$FIXTURE"
  run bash "$DML" wow config set --key server.motd --value Hi --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "config set server.motd maps HTTP 401 to SOAP_AUTH" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  export HOME="$FIXTURE"
  run bash "$DML" wow config set --key server.motd --value Hi --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
}
