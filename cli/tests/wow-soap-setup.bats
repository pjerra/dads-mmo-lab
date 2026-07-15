#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  OVR="$DML_GAMES_DIR/wow-server-playerbots/docker-compose.override.yml"
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    restart: on-failure
    environment:
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "250"
EOF
}
teardown() { teardown_fixture; }

@test "soap-setup adds SOAP env and localhost port mapping" {
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  grep -q 'AC_SOAP_ENABLED' "$OVR"
  grep -q 'AC_SOAP_PORT' "$OVR"
  grep -q '127.0.0.1:7878:7878' "$OVR"
}

@test "soap-setup is idempotent and preserves existing worldserver env as valid YAML" {
  bash "$DML" wow soap-setup --json >/dev/null
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(grep -c 'AC_SOAP_ENABLED' "$OVR")" = "1" ]
  # YAML must remain valid AND still contain the pre-existing playerbot env
  # (guards against the duplicate-top-level-services-key bug).
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MAX_RANDOM_BOTS == "250"' "$OVR"
  yq -e '.services.ac-worldserver.environment.AC_SOAP_PORT == "7878"' "$OVR"
  # exactly one localhost SOAP port mapping after two runs
  [ "$(yq '.services.ac-worldserver.ports | length' "$OVR")" = "1" ]
}

@test "soap-setup errors NOT_FOUND when wow server absent" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
