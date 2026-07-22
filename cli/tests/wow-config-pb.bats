#!/usr/bin/env bats
# Batch 1 Feature 2: guided playerbots world-settings editor.
# Curated `conf:playerbots.conf:` registry rows + the pb-keys all-keys
# browser + the direct conf-route in `config set`.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  OVR="$GDIR/docker-compose.override.yml"
  MODS="$GDIR/env/dist/etc/modules"
  PB="$MODS/playerbots.conf"
  mkdir -p "$MODS"
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

_seed_pb() {
  cat > "$PB" <<'EOF'
# Playerbots test conf
AiPlayerbot.MinRandomBots = 500
AiPlayerbot.MaxRandomBots = 500

# a comment mentioning AiPlayerbot.RandomBotTalk = 9 must not parse
AiPlayerbot.RandomBotTalk = 1
AiPlayerbot.CommandPrefix = ""
AiPlayerbot.RandomBotTalk = 0
EOF
}

@test "pb-keys parses active keys, skips comments/blanks, duplicate key last-wins" {
  _seed_pb
  run bash "$DML" wow config pb-keys --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "playerbots.conf" ]
  [ "$(echo "$output" | jq -r '.data.keys | length')" = "4" ]
  # duplicate: last value wins, line points at the WINNING line
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="AiPlayerbot.RandomBotTalk") | .value')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="AiPlayerbot.RandomBotTalk") | .line')" = "8" ]
  # raw right-hand side preserved (quotes included) for verbatim round-trips
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="AiPlayerbot.CommandPrefix") | .value')" = '""' ]
  # no dist present -> default is null
  [ "$(echo "$output" | jq -r '.data.keys[0].default')" = "null" ]
}

@test "pb-keys reports .dist defaults when both files exist" {
  _seed_pb
  printf 'AiPlayerbot.MaxRandomBots = 123\n' > "$PB.dist"
  run bash "$DML" wow config pb-keys --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="AiPlayerbot.MaxRandomBots") | .default')" = "123" ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="AiPlayerbot.MaxRandomBots") | .value')" = "500" ]
}

@test "pb-keys falls back to the .dist when the conf does not exist yet" {
  printf 'AiPlayerbot.MaxRandomBots = 500\n' > "$PB.dist"
  run bash "$DML" wow config pb-keys --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "playerbots.conf.dist" ]
  [ "$(echo "$output" | jq -r '.data.keys[0].value')" = "500" ]
  [ "$(echo "$output" | jq -r '.data.keys[0].default')" = "500" ]
}

@test "pb-keys errors NOT_FOUND when neither conf nor dist exists" {
  run bash "$DML" wow config pb-keys --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "direct conf route writes playerbots.conf and is restart-to-apply" {
  _seed_pb
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.EnableGreet --value 1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  grep -q '^AiPlayerbot.EnableGreet = 1$' "$PB"
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.EnableGreet --value 1 --json
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "none" ]
}

# NB Module-tuning rework: the direct route now covers every module conf that
# passes _cfg_file_path's allowlist (see wow-config-conf-keys.bats); the core
# worldserver.conf/authserver.conf remain curated-rows-only, asserted here.
@test "direct conf route rejects core confs and validates key/value shape" {
  _seed_pb
  run bash "$DML" wow config set --key conf:GameType --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key conf:worldserver.conf:GameType --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key 'conf:playerbots.conf:Bad Key;' --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.EnableGreet --value $'1\nEvil = 1' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  # nothing leaked into the conf
  ! grep -q 'Evil' "$PB"
}

# The all-keys browser makes every playerbots key a first-class editable
# field, including the boot-time bot-wipe latch that `wow bots flush` wraps
# in a typed ack, a safety backup and a restore that survives signals. Set
# by hand it stays armed forever: EVERY later boot wipes all random bots'
# characters, auctions and mail. The direct route refuses it.
@test "direct conf route refuses the flush-managed bot-wipe key" {
  _seed_pb
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.DeleteRandomBotAccounts --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  echo "$output" | grep -q 'bots flush'
  # not written, not even appended
  ! grep -q 'DeleteRandomBotAccounts' "$PB"
}

@test "direct conf route refuses the bot-wipe key even when setting it back to 0" {
  # No special-case for the "safe" value: the flush verb owns this key, and a
  # partial exception would just be a second way to reason about it.
  _seed_pb
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.DeleteRandomBotAccounts --value 0 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "direct conf route still accepts ordinary playerbots keys" {
  _seed_pb
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.EnableGreet --value 1 --json
  [ "$status" -eq 0 ]
  grep -q '^AiPlayerbot.EnableGreet = 1$' "$PB"
}

# CommandPrefix is seeded quoted ("") -- the quote-handling round-trip cases.
@test "direct conf route: re-setting a quoted value to the same value is a no-op" {
  _seed_pb
  # effective value unchanged ("" -> "") must NOT report a change or flip restart
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.CommandPrefix --value '""' --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "none" ]
}

@test "direct conf route: editing a quoted value preserves the quotes" {
  _seed_pb
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.CommandPrefix --value '.' --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  # a value that may need quotes keeps them across a legitimate edit
  grep -q '^AiPlayerbot.CommandPrefix = "."$' "$PB"
}

@test "direct conf route: a user-quoted value is written with its quotes" {
  _seed_pb
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.EnableGreet --value '"1"' --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  grep -q '^AiPlayerbot.EnableGreet = "1"$' "$PB"
}

@test "bots.population conf row writes BOTH Min and Max and removes both legacy envs" {
  _seed_pb
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AI_PLAYERBOT_MIN_RANDOM_BOTS: "1600"
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "2000"
      AC_RATE_XP_KILL: "3"
EOF
  run bash "$DML" wow config set --key bots.population --value 750 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  grep -q '^AiPlayerbot.MinRandomBots = 750$' "$PB"
  grep -q '^AiPlayerbot.MaxRandomBots = 750$' "$PB"
  run yq -e '.services.ac-worldserver.environment | has("AC_AI_PLAYERBOT_MIN_RANDOM_BOTS")' "$OVR"
  [ "$status" -ne 0 ]
  run yq -e '.services.ac-worldserver.environment | has("AC_AI_PLAYERBOT_MAX_RANDOM_BOTS")' "$OVR"
  [ "$status" -ne 0 ]
  # unrelated env keys survive
  yq -e '.services.ac-worldserver.environment.AC_RATE_XP_KILL == "3"' "$OVR"
}

@test "config list shows a still-present legacy env value for a conf row (truthful pre-migration read)" {
  use_mysql_stub
  _seed_pb
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "2000"
EOF
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  # env (what the server actually runs) beats the conf's 500 until a save
  # cleans the env off
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .value')" = "2000" ]
  # without the env, the conf value shows
  rm -f "$OVR"
  run bash "$DML" wow config list --json
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .value')" = "500" ]
}

@test "curated playerbots rows read defaults from the dist and write the modules conf" {
  printf 'AiPlayerbot.RandomBotAllianceRatio = 65\n' > "$PB.dist"
  use_mysql_stub
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.alliance_ratio") | .value')" = "65" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.alliance_ratio") | .group')" = "Bot Balance" ]
  # a set creates the conf from the dist, then edits it
  run bash "$DML" wow config set --key bots.alliance_ratio --value 30 --json
  [ "$status" -eq 0 ]
  [ -f "$PB" ]
  grep -q '^AiPlayerbot.RandomBotAllianceRatio = 30$' "$PB"
}
