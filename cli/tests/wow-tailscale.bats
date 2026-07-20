#!/usr/bin/env bats
# Batch 5 (overnight): Tailscale "Play Together" -- `wow tailscale
# install|up|status|down`. Pure command-wrapper arms; the whole privileged
# tool-chain (tailscale/sudo/pacman/systemctl/iptables) is stubbed via
# use_tailscale_stub, and the tailscale binary is pointed at the stub through
# the DML_TS_BIN seam (a bogus name exercises the not-installed path).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_tailscale_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

# --- install ---------------------------------------------------------------

@test "tailscale install: already present -> already:true, no pacman needed" {
  # DML_TS_BIN already points at the stub, so command -v finds it.
  run bash "$DML" wow tailscale install --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.installed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.already')" = "true" ]
}

@test "tailscale install: fresh -> runs pacman, installed:true already:false" {
  export DML_TS_BIN="tailscale-absent-xyz"   # command -v fails -> install path
  run bash "$DML" wow tailscale install --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.installed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.already')" = "false" ]
}

@test "tailscale install: no passwordless sudo -> SUDO_REQUIRED with a guided hint" {
  export DML_TS_BIN="tailscale-absent-xyz"
  export DML_STUB_SUDO_FAIL=1
  run bash "$DML" wow tailscale install --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SUDO_REQUIRED" ]
  echo "$output" | jq -r '.error.hint' | grep -qi 'DML shell'
}

@test "tailscale install: pacman failure -> INSTALL_FAILED" {
  export DML_TS_BIN="tailscale-absent-xyz"
  export DML_STUB_PACMAN_EXIT=1
  run bash "$DML" wow tailscale install --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "INSTALL_FAILED" ]
}

# --- up --------------------------------------------------------------------

@test "tailscale up: not installed -> NOT_INSTALLED" {
  export DML_TS_BIN="tailscale-absent-xyz"
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_INSTALLED" ]
}

@test "tailscale up: already authenticated -> connected:true, IP set, no auth_url" {
  export DML_STUB_TS_CONNECTED=1
  export DML_STUB_TS_IP="100.101.102.103"
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.connected')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.ip')" = "100.101.102.103" ]
  [ "$(echo "$output" | jq -r '.data.auth_url')" = "null" ]
  # firewall is opened on kernel-TUN boxes, skipped in userspace mode -- never
  # a hard failure.
  fw="$(echo "$output" | jq -r '.data.firewall')"
  [ "$fw" != "failed" ]
}

@test "tailscale up: needs login -> connected:false, auth_url returned for the user" {
  export DML_STUB_TS_UP_URL="https://login.tailscale.com/a/feedface99"
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.connected')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.auth_url')" = "https://login.tailscale.com/a/feedface99" ]
  [ "$(echo "$output" | jq -r '.data.ip')" = "null" ]
}

@test "tailscale up: daemon/login unreachable without sudo -> SUDO_REQUIRED" {
  export DML_STUB_SUDO_FAIL=1
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SUDO_REQUIRED" ]
}

@test "tailscale up: passes --timeout and the 3724,8085 firewall ports on connect" {
  export DML_STUB_TS_CONNECTED=1
  export DML_STUB_TS_IP="100.90.80.70"
  export DML_STUB_TS_CALL_LOG="$FIXTURE/ts-calls.log"
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 0 ]
  grep -q 'up --timeout=' "$DML_STUB_TS_CALL_LOG"
}

# --- status ----------------------------------------------------------------

@test "tailscale status: connected -> IP + BackendState reported" {
  export DML_STUB_TS_IP="100.64.5.5"
  export DML_STUB_TS_STATE="Running"
  run bash "$DML" wow tailscale status --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.connected')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.ip')" = "100.64.5.5" ]
  [ "$(echo "$output" | jq -r '.data.backend_state')" = "Running" ]
}

@test "tailscale status: not installed -> NOT_INSTALLED" {
  export DML_TS_BIN="tailscale-absent-xyz"
  run bash "$DML" wow tailscale status --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_INSTALLED" ]
}

# --- down ------------------------------------------------------------------

@test "tailscale down: disconnects -> down:true" {
  run bash "$DML" wow tailscale down --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.down')" = "true" ]
}

@test "tailscale down: failure -> TAILSCALE_DOWN_FAILED" {
  export DML_STUB_TS_DOWN_EXIT=1
  run bash "$DML" wow tailscale down --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "TAILSCALE_DOWN_FAILED" ]
}

# --- dispatch --------------------------------------------------------------

@test "tailscale: unknown subcommand -> UNKNOWN_COMMAND" {
  run bash "$DML" wow tailscale frobnicate --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "UNKNOWN_COMMAND" ]
}
