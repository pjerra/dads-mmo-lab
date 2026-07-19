#!/usr/bin/env bats
# Batch 1 Feature 1: conf-file registry rows (expanded live server rates).
# The `conf:` env-column routing writes worldserver.conf in place instead of
# the compose override (env is frozen at container creation AND beats conf,
# so env rows can never live-apply -- see 40-config.sh registry block).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  OVR="$GDIR/docker-compose.override.yml"
  ETC="$GDIR/env/dist/etc"
  mkdir -p "$ETC/modules"
  export HOME="$FIXTURE"   # sandboxes ~/.dml (soap.lock) away from the real home
}
teardown() { teardown_fixture; }

_seed_worldconf() {
  cat > "$ETC/worldserver.conf" <<'EOF'
# Comment that must survive edits
#    Rate.Honor
#        Description: honor rate (commented mention must NOT match)
Rate.Honor = 1
Rate.XP.Kill      = 1
GameType = 0
EOF
}

@test "conf-row set replaces the line in place, keeps comments, applies live over SOAP" {
  _seed_worldconf
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/captured.xml"
  run bash "$DML" wow config set --key rates.honor --value 3 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "live" ]
  grep -q '^Rate.Honor = 3$' "$ETC/worldserver.conf"
  # comment block untouched, commented mention NOT edited, neighbors intact
  grep -q '^# Comment that must survive edits$' "$ETC/worldserver.conf"
  grep -q '^#    Rate.Honor$' "$ETC/worldserver.conf"
  grep -q '^GameType = 0$' "$ETC/worldserver.conf"
  # the live apply went over SOAP as `reload config`
  captured="$(cat "$DML_STUB_CAPTURE")"
  cmd="${captured#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "reload config" ]
}

@test "conf-row set creates worldserver.conf from its .dist when only the dist exists" {
  cat > "$ETC/worldserver.conf.dist" <<'EOF'
# dist header comment
Rate.Honor = 1
EOF
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow config set --key rates.honor --value 2 --json
  [ "$status" -eq 0 ]
  [ -f "$ETC/worldserver.conf" ]
  grep -q '^# dist header comment$' "$ETC/worldserver.conf"
  grep -q '^Rate.Honor = 2$' "$ETC/worldserver.conf"
}

@test "conf-row set errors NOT_FOUND when neither conf nor dist exists" {
  use_curl_stub
  run bash "$DML" wow config set --key rates.honor --value 2 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "conf-row set appends a missing key and a second identical set is a no-op" {
  printf '# only a comment\n' > "$ETC/worldserver.conf"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow config set --key rates.reputation --value 2.5 --json
  [ "$status" -eq 0 ]
  grep -q '^Rate.Reputation.Gain = 2.5$' "$ETC/worldserver.conf"
  run bash "$DML" wow config set --key rates.reputation --value 2.5 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "none" ]
}

@test "conf-row migration: legacy env override is removed and forces restart even with SOAP up" {
  _seed_worldconf
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_RATE_XP_KILL: "3"
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "2000"
EOF
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow config set --key rates.xp_kill --value 4 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  # the frozen env is still inside the RUNNING container -> restart, not live
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  grep -q '^Rate.XP.Kill = 4$' "$ETC/worldserver.conf"
  run yq -e '.services.ac-worldserver.environment | has("AC_RATE_XP_KILL")' "$OVR"
  [ "$status" -ne 0 ]
  # unrelated env keys survive the removal
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MAX_RANDOM_BOTS == "2000"' "$OVR"
}

@test "conf-row set reports restart when SOAP is unreachable" {
  _seed_worldconf
  use_curl_stub
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow config set --key rates.honor --value 5 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  grep -q '^Rate.Honor = 5$' "$ETC/worldserver.conf"
}

@test "cross-faction bool rows write AllowTwoSide keys and validate 0/1" {
  _seed_worldconf
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow config set --key crossfaction.group --value 1 --json
  [ "$status" -eq 0 ]
  grep -q '^AllowTwoSide.Interaction.Group = 1$' "$ETC/worldserver.conf"
  run bash "$DML" wow config set --key crossfaction.group --value 2 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "config list reads conf rows conf-first, then .dist, then the registry default" {
  use_mysql_stub
  # no conf, no dist -> default
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.honor") | .value')" = "1" ]
  # dist only -> dist value
  printf 'Rate.Honor = 2\n' > "$ETC/worldserver.conf.dist"
  run bash "$DML" wow config list --json
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.honor") | .value')" = "2" ]
  # conf wins over dist; LAST duplicate wins; exact-key match (no prefix bleed)
  cat > "$ETC/worldserver.conf" <<'EOF'
Rate.Honor = 7
Rate.Honor = 3
Rate.Honor.Extra = 9
EOF
  run bash "$DML" wow config list --json
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.honor") | .value')" = "3" ]
  # conf rows are conservative restart_required=true in list (set's `applied`
  # field is the authoritative answer)
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.honor") | .restart_required')" = "true" ]
}
