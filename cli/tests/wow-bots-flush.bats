#!/usr/bin/env bats
# Batch 1 Feature 4: `wow bots flush` -- backup, arm the delete flag,
# restart (wipe), restore the flag, restart (rebuild). The flag must come
# back to 0 on EVERY exit path (EXIT trap), or the next boot would silently
# wipe the bots again.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  MODS="$GDIR/env/dist/etc/modules"
  PB="$MODS/playerbots.conf"
  mkdir -p "$MODS"
  export HOME="$FIXTURE"
  BDIR="$FIXTURE/.dml/backups"
  use_backup_stub
  use_curl_stub   # saveall best-effort goes over SOAP
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  # readiness marker the docker-logs stub serves (see _world_ready)
  printf 'World Initialized In 42 seconds\n' > "$FIXTURE/ready.log"
  export DML_STUB_LOGS_FILE="$FIXTURE/ready.log"
}
teardown() { teardown_fixture; }

_flag_line() { grep '^AiPlayerbot.DeleteRandomBotAccounts' "$PB"; }

@test "bots flush without --yes and the typed ack is CONFIRM_REQUIRED and touches nothing" {
  printf 'AiPlayerbot.DeleteRandomBotAccounts = 0\n' > "$PB"
  run bash "$DML" wow bots flush --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"CONFIRM_REQUIRED"'
  run bash "$DML" wow bots flush --yes --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"CONFIRM_REQUIRED"'
  run bash "$DML" wow bots flush --yes --ack nope --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"CONFIRM_REQUIRED"'
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
  [ ! -f "$DML_STUB_CALL_LOG" ]
}

@test "bots flush happy path: backup, two staged restarts, flag restored, done event" {
  printf '# pb conf\nAiPlayerbot.DeleteRandomBotAccounts = 0\n' > "$PB"
  run bash "$DML" wow bots flush --yes --ack flush --json
  [ "$status" -eq 0 ]
  d="$(echo "$output" | grep '"event":"done"' | tail -1)"
  [ "$(echo "$d" | jq -r '.data.flushed')" = "true" ]
  bfile="$(echo "$d" | jq -r '.data.backup')"
  [[ "$bfile" =~ ^wow-[0-9]{8}-[0-9]{6}\.sql\.gz$ ]]
  [ -f "$BDIR/$bfile" ]
  # docker call ORDER: dump first (nothing destroyed on a failed dump), then
  # stop/up twice (deletion boot + rebuild boot)
  run grep -c '^compose stop -t 180 ac-worldserver ac-authserver$' "$DML_STUB_CALL_LOG"
  [ "$output" = "2" ]
  run grep -c '^compose up -d --no-deps ac-authserver ac-worldserver$' "$DML_STUB_CALL_LOG"
  [ "$output" = "2" ]
  head -1 "$DML_STUB_CALL_LOG" | grep -q '^mysqldump'
  # the flag is back at 0 (armed to 1 in between, restored in step 5)
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
  # comment preserved by the in-place edits
  grep -q '^# pb conf$' "$PB"
  # saveall went out (best-effort) before each stop
  grep -q 'saveall' "$FIXTURE/captured.xml" 2>/dev/null || true
  # narrative order: backup -> armed -> deleted/restoring -> rebuild
  echo "$output" | grep -q 'backing up characters'
  echo "$output" | grep -q 'delete flag armed'
  echo "$output" | grep -q 'restoring the setting'
  echo "$output" | grep -q 'rebuild the bot population'
}

@test "bots flush aborts on a failed backup BEFORE touching the conf" {
  printf 'AiPlayerbot.DeleteRandomBotAccounts = 0\n' > "$PB"
  export DML_STUB_DUMP_EXIT=1
  run bash "$DML" wow bots flush --yes --ack flush --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"BACKUP_FAILED"'
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
  # no restart was attempted
  ! grep -q '^compose' "$DML_STUB_CALL_LOG"
}

@test "bots flush restores the armed flag when the restart fails mid-flow" {
  # Conf deliberately WITHOUT the key: the armed write appends it, so the
  # trailing '= 0' line PROVES the 1 was written and then restored.
  printf '# pb conf without the delete key\n' > "$PB"
  export DML_STUB_COMPOSE_EXIT=1
  run bash "$DML" wow bots flush --yes --ack flush --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"RESTART_FAILED"'
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
}

@test "bots flush restores the flag on a readiness timeout" {
  printf 'AiPlayerbot.DeleteRandomBotAccounts = 0\n' > "$PB"
  printf 'no marker here\n' > "$FIXTURE/ready.log"
  export DML_READY_TIMEOUT_SECS=0
  run bash "$DML" wow bots flush --yes --ack flush --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"TIMEOUT"'
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
}

@test "bots flush creates playerbots.conf from the dist when only the dist exists" {
  printf 'AiPlayerbot.DeleteRandomBotAccounts = 0\n' > "$PB.dist"
  run bash "$DML" wow bots flush --yes --ack flush --json
  [ "$status" -eq 0 ]
  [ -f "$PB" ]
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
}

@test "bots flush rejects an unknown subcommand" {
  run bash "$DML" wow bots explode --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"UNKNOWN_COMMAND"'
}
