#!/usr/bin/env bats
# `wow config registry`: the cheap, values-free sibling of `config list`.
# Emits the SAME .data.settings[] array with every STATIC field but reads NO
# values (no yq fork, no conf reads, no motd DB lookup, no docker) so the
# launcher Rust core can fetch the registry once and read values itself.
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

@test "config registry: row count matches config list (66)" {
  run bash "$DML" wow config registry --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "66" ]
}

@test "config registry: settings order and static fields are identical to config list" {
  # Both arms must produce byte-identical STATIC fields for every row, in the
  # same order. Strip .value (registry has none) and compare the whole arrays.
  use_mysql_stub
  printf 'Live motd from db\n' > "$FIXTURE/motd.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/motd.tsv"
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  list_static="$(echo "$output" | jq -cS '[.data.settings[] | del(.value)]')"
  run bash "$DML" wow config registry --json
  [ "$status" -eq 0 ]
  reg_static="$(echo "$output" | jq -cS '[.data.settings[] | del(.value)]')"
  [ "$list_static" = "$reg_static" ]
}

@test "config registry: a spot-checked row matches config list's static fields exactly" {
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  lrow="$(echo "$output" | jq -cS '.data.settings[] | select(.key=="bots.population") | del(.value)')"
  run bash "$DML" wow config registry --json
  [ "$status" -eq 0 ]
  rrow="$(echo "$output" | jq -cS '.data.settings[] | select(.key=="bots.population") | del(.value)')"
  [ "$lrow" = "$rrow" ]
  # spot-check the concrete static values on the registry row
  run bash "$DML" wow config registry --json
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .group')" = "Bot Population" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .type')" = "int" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .min')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .max')" = "3000" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .default')" = "500" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .env')" = "conf:playerbots.conf:AiPlayerbot.MaxRandomBots" ]
}

@test "config registry: value is empty for every row and restart_required mirrors list" {
  run bash "$DML" wow config registry --json
  [ "$status" -eq 0 ]
  # every value is the empty string (the launcher reads live values itself)
  [ "$(echo "$output" | jq -r '[.data.settings[] | select(.value != "")] | length')" = "0" ]
  # server.motd is the one live-applied row (restart_required false); all
  # others are restart-to-apply -- same as config list.
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="server.motd") | .restart_required')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .restart_required')" = "true" ]
  [ "$(echo "$output" | jq -r '[.data.settings[] | select(.restart_required==false)] | length')" = "1" ]
}

@test "config registry: succeeds with NO yq and NO docker (never forks either)" {
  # A PATH shim makes yq AND docker fail hard; the registry arm must still
  # answer 66 rows. If it forked yq/docker (as config list does) this would
  # break -- proving the arm does no value reads. DML_YQ_BIN points at the
  # failing shim too (that is the name `config list`'s preamble would fork).
  shim="$FIXTURE/failshim"
  mkdir -p "$shim"
  printf '#!/usr/bin/env bash\necho "yq must not run" >&2\nexit 3\n' > "$shim/yq"
  printf '#!/usr/bin/env bash\necho "docker must not run" >&2\nexit 3\n' > "$shim/docker"
  chmod +x "$shim/yq" "$shim/docker"
  run env DML_YQ_BIN="$shim/yq" PATH="$shim:$PATH" bash "$DML" wow config registry --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "66" ]
  # and it did not emit either shim's stderr complaint
  [[ "$output" != *"yq must not run"* ]]
  [[ "$output" != *"docker must not run"* ]]
}
