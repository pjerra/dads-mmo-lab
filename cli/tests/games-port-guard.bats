#!/usr/bin/env bats
#
# The stack-port REFUSAL (Task 7, user decision 2026-08-01), mirroring
# crates/dml-wow/src/lifecycle.rs `stack_port_refusal`. A fix on one surface only
# half-ships, so both are pinned.
#
# The three ports here are the ones THIS stack binds. A collision on them is
# fatal because the `ac-*` container names are global to the docker engine, so a
# second stack cannot work however it is started.

load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  export HOME="$FIXTURE"
  add_game wow compose
  export DML_STUB_RUNNING=""
}

teardown() { teardown_fixture; }

# `ss` is stubbed rather than the port bound for real: a test that binds 3724
# fails on a developer machine that happens to be running a server, and cannot
# model "ss is missing" at all.
stub_ss() {  # stub_ss <lines of `ss -tlnp` output>
  cat > "$STUB_BIN/ss" <<EOS
#!/usr/bin/env bash
printf '%s\n' "\$SS_OUTPUT"
exit \${SS_EXIT:-0}
EOS
  chmod +x "$STUB_BIN/ss"
  export SS_OUTPUT="$1"
  export SS_EXIT="${2:-0}"
}

@test "a free box starts normally" {
  stub_ss "LISTEN 0 128 0.0.0.0:22 0.0.0.0:*"
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "a taken login port REFUSES the start" {
  stub_ss "LISTEN 0 128 0.0.0.0:3724 0.0.0.0:*"
  run bash "$DML" start wow
  [ "$status" -ne 0 ]
  [[ "$output" == *"3724"* ]]
  [[ "$output" == *"login server"* ]]
  # The refusal must land BEFORE the compose call, not after it.
  [[ "$output" != *"Starting wow"* ]]
}

@test "every colliding stack port is named, not just the first" {
  # Reporting one at a time sends the user round the loop again.
  stub_ss "LISTEN 0 128 0.0.0.0:3724 0.0.0.0:*
LISTEN 0 128 0.0.0.0:8085 0.0.0.0:*"
  run bash "$DML" start wow
  [ "$status" -ne 0 ]
  [[ "$output" == *"3724"* ]]
  [[ "$output" == *"8085"* ]]
}

@test "the SOAP port is part of the refusal" {
  stub_ss "LISTEN 0 128 0.0.0.0:7878 0.0.0.0:*"
  run bash "$DML" start wow
  [ "$status" -ne 0 ]
  [[ "$output" == *"7878"* ]]
}

@test "the DB port stays ADVISORY -- it has an automatic remedy" {
  # 3306 gets the .env remap; promoting it would turn a fixable collision into
  # a hard stop, and clients never connect to it directly.
  stub_ss "LISTEN 0 128 0.0.0.0:3306 0.0.0.0:*"
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "another game's port does not refuse a WoW start" {
  # The advisory sweep covers 17 ports as a courtesy; EverQuest holding 4000
  # says nothing about whether this stack can come up.
  stub_ss "LISTEN 0 128 0.0.0.0:4000 0.0.0.0:*"
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
  # ...but it is still reported.
  [[ "$output" == *"4000"* ]]
}

@test "a probe that cannot answer never blocks a start" {
  # TRI-STATE. `ss` missing or failing is evidence of NOTHING. The cost of a
  # wrong refusal is a user who cannot start a server that would have worked,
  # with no override anywhere.
  stub_ss "" 1
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "an empty ss answer never blocks a start either" {
  stub_ss "" 0
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}
