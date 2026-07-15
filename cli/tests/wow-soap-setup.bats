#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  # Realistic base compose: already publishes the SOAP port via
  # DOCKER_SOAP_EXTERNAL_PORT (defaults to 7878, unbound). This is vendored
  # and must not be edited by soap-setup -- only .env may pin its value.
  cat > "$GDIR/docker-compose.yml" <<'EOF'
services:
  ac-worldserver:
    ports:
      - "${DOCKER_WORLD_EXTERNAL_PORT:-8085}:8085"
      - "${DOCKER_SOAP_EXTERNAL_PORT:-7878}:7878"
EOF
  OVR="$GDIR/docker-compose.override.yml"
  ENVF="$GDIR/.env"
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    restart: on-failure
    environment:
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "250"
EOF
}
teardown() { teardown_fixture; }

@test "soap-setup pins SOAP via .env and merges env, touching no ports:" {
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  grep -qxF 'DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878' "$ENVF"
  grep -q 'AC_SOAP_ENABLED' "$OVR"
  grep -q 'AC_SOAP_IP' "$OVR"
  grep -q 'AC_SOAP_PORT' "$OVR"
  # The override must NEVER carry a ports: key -- the base compose file
  # already publishes 7878, and compose concatenates ports: lists across
  # base+override, so a second entry here would double-bind the port.
  yq -e '.services.ac-worldserver | has("ports") | not' "$OVR"
}

@test "soap-setup is idempotent: second run is a no-op, playerbot env preserved" {
  bash "$DML" wow soap-setup --json >/dev/null
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  # exactly one SOAP port line in .env after two runs
  [ "$(grep -c '^DOCKER_SOAP_EXTERNAL_PORT=' "$ENVF")" = "1" ]
  # YAML must remain valid AND still contain the pre-existing playerbot env
  # (guards against the duplicate-top-level-services-key bug).
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MAX_RANDOM_BOTS == "250"' "$OVR"
  yq -e '.services.ac-worldserver.environment.AC_SOAP_PORT == "7878"' "$OVR"
}

@test "soap-setup errors NOT_FOUND when wow server absent" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "soap-setup preserves pre-existing unrelated .env content" {
  printf 'FOO=bar\n' > "$ENVF"
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  grep -qxF 'FOO=bar' "$ENVF"
  grep -qxF 'DOCKER_SOAP_EXTERNAL_PORT=127.0.0.1:7878' "$ENVF"
}

@test "soap-setup errors MISSING_DEP when yq is unavailable" {
  # DML_YQ_BIN is a test-only override seam (mirrors Plan 1's DML_GAMES_DIR)
  # letting us simulate "yq not installed" without touching real PATH/yq.
  run env DML_YQ_BIN=definitely-missing-yq-bin bash "$DML" wow soap-setup --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "MISSING_DEP" ]
}
