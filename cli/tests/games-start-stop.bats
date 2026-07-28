#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  # stop/restart now write a pre-down worldserver log snapshot -- sandbox
  # ~/.dml/logs so this suite can never litter the real home dir.
  export HOME="$FIXTURE"
}

teardown() { teardown_fixture; }

@test "games start streams NDJSON and ends with done running" {
  add_game wow compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"  # post-start state
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  first="$(echo "$output" | head -1)"
  last="$(echo "$output" | tail -1)"
  [ "$(echo "$first" | jq -r '.event')" = "section_start" ]
  [ "$(echo "$last" | jq -r '.event')" = "done" ]
  [ "$(echo "$last" | jq -r '.data.state')" = "running" ]
  # every line is valid JSON
  echo "$output" | while IFS= read -r l; do echo "$l" | jq -e . >/dev/null; done
}

@test "games start uses dml-start.sh hook when present and streams its output" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] staged start: mode=$1"
exit 0
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'staged start: mode=start'
}

@test "games start fails with START_FAILED when hook exits nonzero" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] ERROR: db not healthy" >&2
exit 1
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  run bash "$DML" games start wow --json
  [ "$status" -eq 1 ]
  last="$(echo "$output" | tail -1)"
  [ "$(echo "$last" | jq -r '.event')" = "error" ]
  [ "$(echo "$last" | jq -r '.error.code')" = "START_FAILED" ]
}

@test "games start with docker down returns DOCKER_DOWN" {
  add_game wow compose
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" games start wow --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | tail -1 | jq -r '.error.code')" = "DOCKER_DOWN" ]
}

@test "games stop ends with done stopped" {
  add_game wow compose
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | tail -1 | jq -r '.data.state')" = "stopped" ]
}

@test "games restart passes restart mode to hook" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] staged start: mode=$1"
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'staged start: mode=restart'
}

@test "games restart --no-saveall exports DML_SKIP_SAVEALL=1 to the hook, title still resolved" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] staged start: mode=$1 skip=${DML_SKIP_SAVEALL:-0}"
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --no-saveall --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'staged start: mode=restart skip=1'
}

@test "games restart without --no-saveall leaves DML_SKIP_SAVEALL unset (0)" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] staged start: mode=$1 skip=${DML_SKIP_SAVEALL:-0}"
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'staged start: mode=restart skip=0'
}

@test "games start unknown title returns NOT_FOUND" {
  run bash "$DML" games start nope --json
  [ "$status" -eq 1 ]
  last="$(echo "$output" | tail -1)"
  [ "$(echo "$last" | jq -r '.event')" = "error" ]
  [ "$(echo "$last" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "games start install-only title returns NO_COMPOSE" {
  add_game runescape install
  run bash "$DML" games start runescape --json
  [ "$status" -eq 1 ]
  last="$(echo "$output" | tail -1)"
  [ "$(echo "$last" | jq -r '.event')" = "error" ]
  [ "$(echo "$last" | jq -r '.error.code')" = "NO_COMPOSE" ]
}

@test "games start ignores non-executable hook and falls back to compose" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] HOOK_MARKER_SHOULD_NOT_RUN"
EOS
  # deliberately not chmod +x -- hook must be ignored and compose used instead
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  [[ "$output" != *"HOOK_MARKER_SHOULD_NOT_RUN"* ]]
  [[ "$output" == *"up -d"* ]]
}

@test "games start in text mode prints legacy-style message" {
  add_game wow compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"[dml] wow started"* ]]
  [[ "$output" != *'"event"'* ]]
}

@test "games stop in text mode prints legacy-style message" {
  add_game wow compose
  run bash "$DML" games stop wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"[dml] wow stopped"* ]]
  [[ "$output" != *'"event"'* ]]
}

@test "games start with missing title emits terminal NOT_FOUND error event" {
  # --json is stripped by the arg parser before reaching the games dispatcher,
  # so $1 is empty here -- this pins the ${1:?}-replacement guard (fix 1a).
  run bash "$DML" games start --json
  [ "$status" -eq 1 ]
  last="$(echo "$output" | tail -1)"
  [ "$(echo "$last" | jq -r '.event')" = "error" ]
  [ "$(echo "$last" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "games restart skips port-conflict warnings (own server holds the ports)" {
  add_game wow compose
  cat > "$STUB_BIN/ss" <<'EOS'
#!/usr/bin/env bash
printf 'LISTEN 0 4096 0.0.0.0:8085 0.0.0.0:*\n'
EOS
  chmod +x "$STUB_BIN/ss"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --json
  [ "$status" -eq 0 ]
  [[ "$output" != *"already in use"* ]]
}

@test "games start still warns on a real port conflict" {
  add_game wow compose
  cat > "$STUB_BIN/ss" <<'EOS'
#!/usr/bin/env bash
printf 'LISTEN 0 4096 0.0.0.0:8085 0.0.0.0:*\n'
EOS
  chmod +x "$STUB_BIN/ss"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  [[ "$output" == *"already in use"* ]]
}

@test "games restart in text mode prints restarted" {
  add_game wow compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"[dml] wow restarted"* ]]
}
