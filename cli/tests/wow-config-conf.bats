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
  # Task 6: add_game seeds a stock worldserver.conf.dist for schema-name
  # resolution -- this test owns the BOTH-absent scenario, so delete it.
  rm -f "$ETC/worldserver.conf.dist"
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
  # ...and specifically a RECREATE: this save REMOVED a shadowing AC_* env key,
  # and a container's environment is fixed when it is created, so restarting the
  # same container cannot apply it. Reporting "world-restart" here is what made
  # the user's setting silently revert (SHIP-LIST 4.0f). NB asserted BEFORE the
  # `run yq` calls below -- `run` replaces $output.
  [ "$(echo "$output" | jq -r '.data.apply_needed')" = "recreate" ]
  grep -q '^Rate.XP.Kill = 4$' "$ETC/worldserver.conf"
  run yq -e '.services.ac-worldserver.environment | has("AC_RATE_XP_KILL")' "$OVR"
  [ "$status" -ne 0 ]
  # unrelated env keys survive the removal
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MAX_RANDOM_BOTS == "2000"' "$OVR"
}

# The override.yml read answers "is the override still on disk", NOT "will a
# reload actually take effect". After save #1 migrated the key away, save #2
# used to find a clean file and report applied:"live" -- while the running
# container still carried the old value and AC's env bridge beat the conf.
@test "conf-row set reports restart when the legacy env is gone from the file but frozen in the container" {
  _seed_worldconf
  use_docker_stub
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  # No AC_RATE_XP_KILL in override.yml (an earlier save cleaned it) ...
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "2000"
EOF
  # ... but the container was created with it and still has it.
  export DML_STUB_CONTAINER_ENV='PATH=/usr/bin
AC_RATE_XP_KILL=3'
  run bash "$DML" wow config set --key rates.xp_kill --value 5 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  # The override file is already clean, but the RUNNING container still carries
  # the key -- so a recreate is the only thing that applies this, and the frozen
  # probe is what distinguishes it from a plain world-restart.
  [ "$(echo "$output" | jq -r '.data.apply_needed')" = "recreate" ]
  grep -q '^Rate.XP.Kill = 5$' "$ETC/worldserver.conf"
}

@test "conf-row set still applies live when the container carries no matching legacy env" {
  _seed_worldconf
  use_docker_stub
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  # A frozen env for an UNRELATED key must not block this row's live apply.
  export DML_STUB_CONTAINER_ENV='AC_RATE_HONOR=7'
  run bash "$DML" wow config set --key rates.xp_kill --value 6 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "live" ]
  # Applied live -> no restart of any kind is pending.
  [ "$(echo "$output" | jq -r '.data.apply_needed')" = "none" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
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

@test "config list conf value survives NO-FORK batching (matches _cfg_conf_read: quote strip, indentation, inline text)" {
  use_mysql_stub
  cat > "$ETC/worldserver.conf" <<'EOF'
    Rate.Honor = "3.5"
Rate.XP.Kill = 2 still counts
EOF
  # The batched getter (_cfg_conf_get_var, used by the emitter) must produce
  # the exact value the old per-row `$(_cfg_conf_read ...)` did.
  source "$BATS_TEST_DIRNAME/../src/10-json.sh"
  source "$BATS_TEST_DIRNAME/../src/40-config.sh"
  cfg_sdir="$GDIR"
  _cfg_conf_get_var "$ETC/worldserver.conf" "Rate.Honor"; a="$REPLY"
  [ "$a" = "$(_cfg_conf_read "$ETC/worldserver.conf" Rate.Honor)" ]
  [ "$a" = "3.5" ]
  _cfg_conf_get_var "$ETC/worldserver.conf" "Rate.XP.Kill"; b="$REPLY"
  [ "$b" = "$(_cfg_conf_read "$ETC/worldserver.conf" Rate.XP.Kill)" ]
  [ "$b" = "2 still counts" ]
  # ...and the emitted config-list row reflects the same batched value.
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.honor") | .value')" = "3.5" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .value')" = "2 still counts" ]
}
