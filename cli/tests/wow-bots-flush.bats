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
  # NB: keep a copy -- any later `run` would clobber $output
  flushout="$output"
  d="$(echo "$flushout" | grep '"event":"done"' | tail -1)"
  [ "$(echo "$d" | jq -r '.data.flushed')" = "true" ]
  bfile="$(echo "$d" | jq -r '.data.backup')"
  [[ "$bfile" =~ ^wow-[0-9]{8}-[0-9]{6}\.sql\.gz$ ]]
  [ -f "$BDIR/$bfile" ]
  # docker call ORDER: dump first (nothing destroyed on a failed dump), then
  # stop/up twice (deletion boot + rebuild boot)
  [ "$(grep -c '^compose stop -t 180 ac-worldserver ac-authserver$' "$DML_STUB_CALL_LOG")" = "2" ]
  [ "$(grep -c -- '^compose up -d --no-deps ac-authserver ac-worldserver$' "$DML_STUB_CALL_LOG")" = "2" ]
  head -1 "$DML_STUB_CALL_LOG" | grep -q '^mysqldump'
  # the flag is back at 0 (armed to 1 in between, restored in step 5)
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
  # comment preserved by the in-place edits
  grep -q '^# pb conf$' "$PB"
  # narrative order: backup -> armed -> deleted/restoring -> rebuild
  echo "$flushout" | grep -q 'backing up characters'
  echo "$flushout" | grep -q 'delete flag armed'
  echo "$flushout" | grep -q 'restoring the setting'
  echo "$flushout" | grep -q 'rebuild the bot population'
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

# --- surviving an untrappable death (review fix) ----------------------------
# The EXIT trap covers normal/`exit`/set -e deaths and the signal traps cover
# HUP/INT/TERM/PIPE, but SIGKILL and power loss are not trappable at all. The
# on-disk marker is what makes the flag recoverable in those cases: the next
# start/restart heals it BEFORE the server boots and wipes the bots.

@test "bots flush leaves no arm marker behind on the happy path" {
  printf 'AiPlayerbot.DeleteRandomBotAccounts = 0\n' > "$PB"
  run bash "$DML" wow bots flush --yes --ack flush --json
  [ "$status" -eq 0 ]
  [ ! -e "$GDIR/.dml-bot-flush-armed" ]
}

@test "bots flush clears the arm marker when the restart fails mid-flow" {
  printf '# pb conf without the delete key\n' > "$PB"
  export DML_STUB_COMPOSE_EXIT=1
  run bash "$DML" wow bots flush --yes --ack flush --json
  [ "$status" -eq 1 ]
  [ ! -e "$GDIR/.dml-bot-flush-armed" ]
}

@test "games start heals a delete flag left armed by a killed flush" {
  # Simulate the SIGKILL aftermath: flag still 1, marker still present.
  printf '# pb conf\nAiPlayerbot.DeleteRandomBotAccounts = 1\n' > "$PB"
  : > "$GDIR/.dml-bot-flush-armed"
  run bash "$DML" games start wow-server-playerbots --json
  [ "$status" -eq 0 ]
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
  [ ! -e "$GDIR/.dml-bot-flush-armed" ]
  echo "$output" | grep -q 'interrupted bot flush'
  grep -q '^# pb conf$' "$PB"
}

@test "games restart heals a delete flag left armed by a killed flush" {
  printf 'AiPlayerbot.DeleteRandomBotAccounts = 1\n' > "$PB"
  : > "$GDIR/.dml-bot-flush-armed"
  run bash "$DML" games restart wow-server-playerbots --json
  [ "$status" -eq 0 ]
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 0" ]
  [ ! -e "$GDIR/.dml-bot-flush-armed" ]
}

@test "games start without a marker never touches playerbots.conf or warns" {
  printf 'AiPlayerbot.DeleteRandomBotAccounts = 1\n' > "$PB"
  run bash "$DML" games start wow-server-playerbots --json
  [ "$status" -eq 0 ]
  # No marker -> no heal: the value stands exactly as the user left it.
  [ "$(_flag_line)" = "AiPlayerbot.DeleteRandomBotAccounts = 1" ]
  run grep -c 'interrupted bot flush' <<< "$output"
  [ "$status" -ne 0 ]
}
