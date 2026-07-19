#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  use_curl_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

all_running_rows() {
  cat > "$FIXTURE/ps.rows" <<'EOF'
ac-database|running|Up 41 seconds (healthy)
ac-worldserver|running|Up 33 seconds
ac-authserver|running|Up 33 seconds
EOF
  export DML_STUB_PS_ROWS="$FIXTURE/ps.rows"
}

booting_log() {
  cat > "$FIXTURE/boot.log" <<'EOF'
Loading Creature templates...
778/1887 Bot Coischawhu logged in
EOF
  export DML_STUB_LOGS_FILE="$FIXTURE/boot.log"
}

ready_log() {
  cat > "$FIXTURE/ready.log" <<'EOF'
Playerbots World Thread Processor initialized
WORLD: World Initialized In 0 Minutes 14 Seconds
AC>
EOF
  export DML_STUB_LOGS_FILE="$FIXTURE/ready.log"
}

soap_live_response() {
  {
    printf '<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>'
    cat "$BATS_TEST_DIRNAME/fixtures/server-info-live.txt"
    printf '</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>'
  } > "$FIXTURE/si.xml"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/si.xml"
}

@test "server-detail: no containers at all -> stopped, all absent, ports null, exit 0" {
  export DML_STUB_CAPTURE="$FIXTURE/soap-probe.xml"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.containers | length')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].name')" = "ac-worldserver" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].role')" = "world" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "absent" ]
  [ "$(echo "$output" | jq -r '.data.containers[1].role')" = "auth" ]
  [ "$(echo "$output" | jq -r '.data.containers[2].role')" = "database" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.ports.world')" = "null" ]
  # World not running -> SOAP must not even be probed.
  [ ! -f "$FIXTURE/soap-probe.xml" ]
}

@test "server-detail: docker daemon down -> stopped with absent containers, exit 0" {
  all_running_rows
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "absent" ]
}

@test "server-detail: all running + SOAP answers -> online with stats" {
  all_running_rows
  ready_log
  soap_live_response
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.players')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.soap.uptime')" = "19 minute(s) 29 second(s)" ]
  [ "$(echo "$output" | jq -r '.data.soap.mean_ms')" = "44" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "running" ]
  [ "$(echo "$output" | jq -r '.data.containers[2].status')" = "Up 41 seconds (healthy)" ]
}

@test "server-detail: running, SOAP dead, no marker yet -> starting" {
  all_running_rows
  booting_log
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "starting" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "null" ]
}

@test "server-detail: running, SOAP dead, marker present -> soap_unreachable" {
  all_running_rows
  ready_log
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "soap_unreachable" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "true" ]
}

@test "server-detail: stale marker from previous run is ignored (--since guard)" {
  all_running_rows
  # Full log HAS the marker (previous run), the since-StartedAt view does NOT.
  # If the CLI forgot --since, the stub serves the full log and this fails.
  ready_log
  cat > "$FIXTURE/since.log" <<'EOF'
Loading Creature templates...
12/1887 Bot Somebot logged in
EOF
  export DML_STUB_LOGS_SINCE_FILE="$FIXTURE/since.log"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "starting" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "false" ]
}

@test "server-detail: 401 means the world answered -> online with auth_ok false" {
  all_running_rows
  ready_log
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.players')" = "null" ]
}

@test "server-detail: a SOAP fault is still an answer -> online, stats null" {
  all_running_rows
  ready_log
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.players')" = "null" ]
}

@test "server-detail: ports come from docker port as strings" {
  all_running_rows
  ready_log
  soap_live_response
  export DML_STUB_PORTS="ac-worldserver 8085 8085
ac-worldserver 7878 7878
ac-authserver 3724 3724
ac-database 3306 3306"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.ports.world')" = "8085" ]
  [ "$(echo "$output" | jq -r '.data.ports.auth')" = "3724" ]
  [ "$(echo "$output" | jq -r '.data.ports.soap')" = "7878" ]
  [ "$(echo "$output" | jq -r '.data.ports.db')" = "3306" ]
}

@test "server-detail: world exited -> stopped, docker status text passes through" {
  cat > "$FIXTURE/ps.rows" <<'EOF'
ac-database|running|Up 2 hours (healthy)
ac-worldserver|exited|Exited (137) 5 minutes ago
ac-authserver|exited|Exited (0) 5 minutes ago
EOF
  export DML_STUB_PS_ROWS="$FIXTURE/ps.rows"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "exited" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].status')" = "Exited (137) 5 minutes ago" ]
  [ "$(echo "$output" | jq -r '.data.containers[2].state')" = "running" ]
}

# ---------- bots block (N1) ----------

@test "server-detail: bots block reports online count + env-override max when running" {
  all_running_rows
  ready_log
  soap_live_response
  add_game wow-server-playerbots compose
  cat > "$DML_GAMES_DIR/wow-server-playerbots/docker-compose.override.yml" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "600"
EOF
  printf '42\n' > "$FIXTURE/bots.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/bots.tsv"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.bots.online')" = "42" ]
  [ "$(echo "$output" | jq -r '.data.bots.max')" = "600" ]
}

@test "server-detail: bots.online is null when the mysql lookup fails, rest of the envelope unaffected" {
  all_running_rows
  ready_log
  soap_live_response
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.bots.online')" = "null" ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.soap.players')" = "1" ]
}

@test "server-detail: bots block stays null and mysql is never queried when the world isn't running" {
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/dbq.log"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.bots.online')" = "null" ]
  [ "$(echo "$output" | jq -r '.data.bots.max')" = "null" ]
  [ ! -f "$FIXTURE/dbq.log" ]
}

@test "server-detail: bots.max falls back to playerbots.conf when no env override is set" {
  all_running_rows
  ready_log
  soap_live_response
  add_game wow-server-playerbots compose
  mkdir -p "$DML_GAMES_DIR/wow-server-playerbots/env/dist/etc/modules"
  cat > "$DML_GAMES_DIR/wow-server-playerbots/env/dist/etc/modules/playerbots.conf" <<'EOF'
AiPlayerbot.MaxRandomBots = 777
EOF
  printf '5\n' > "$FIXTURE/bots.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/bots.tsv"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.bots.max')" = "777" ]
}

# ---------- crashed vs stopped (Batch 2 F8) ----------

world_exited_rows() {  # $1 = docker status text for the world row
  cat > "$FIXTURE/ps.rows" <<EOF
ac-database|running|Up 2 hours (healthy)
ac-worldserver|exited|$1
ac-authserver|exited|Exited (0) 5 minutes ago
EOF
  export DML_STUB_PS_ROWS="$FIXTURE/ps.rows"
}

@test "server-detail: world exit code 0 -> stopped (clean exit), exit_code in envelope" {
  world_exited_rows "Exited (0) 5 minutes ago"
  export DML_STUB_EXIT_CODE=0
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.exit_code')" = "0" ]
}

@test "server-detail: world exit code 143 (SIGTERM = normal stop) -> stopped" {
  world_exited_rows "Exited (143) 5 minutes ago"
  export DML_STUB_EXIT_CODE=143
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.exit_code')" = "143" ]
}

@test "server-detail: world exit code 137 (SIGKILL) -> crashed" {
  world_exited_rows "Exited (137) 5 minutes ago"
  export DML_STUB_EXIT_CODE=137
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "crashed" ]
  [ "$(echo "$output" | jq -r '.data.exit_code')" = "137" ]
}

@test "server-detail: world exit code 1 -> crashed" {
  world_exited_rows "Exited (1) 5 minutes ago"
  export DML_STUB_EXIT_CODE=1
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "crashed" ]
  [ "$(echo "$output" | jq -r '.data.exit_code')" = "1" ]
}

@test "server-detail: absent world -> stopped with exit_code null (nothing to inspect)" {
  # No PS rows at all: every container is absent. Even a poisoned stub exit
  # code must not leak through -- the absent guard skips the inspect.
  export DML_STUB_EXIT_CODE=137
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.exit_code')" = "null" ]
}

@test "server-detail: running world keeps exit_code null and its verdict" {
  all_running_rows
  ready_log
  soap_live_response
  export DML_STUB_EXIT_CODE=137
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.exit_code')" = "null" ]
}

@test "server-info still behaves exactly as before (regression canary)" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow server-info --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online')" = "false" ]
}
