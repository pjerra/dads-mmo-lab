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

# --- the 2026-07-29 live failure (found on a clean Windows 11 VM) -----------
#
# Reported as: "Could not start Tailscale login — timeout waiting for Tailscale
# service to enter a Running state". The daemon was healthy the whole time; the
# tailscaled journal showed RegisterReq at 22:37:52 and "AuthURL is ..." at
# 22:38:22 -- THIRTY SECONDS -- while `up` waited 8 and then reported a bare
# timeout, throwing away a login that was succeeding.

@test "tailscale up: the login wait is long enough for a slow control plane" {
  # The default is the fix. 8s (the old value) could not survive the measured
  # 30s delay, so this fails if anyone lowers it back under half a minute.
  export DML_STUB_TS_CONNECTED=1
  export DML_STUB_TS_IP="100.90.80.70"
  export DML_STUB_TS_CALL_LOG="$FIXTURE/ts-calls.log"
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 0 ]
  secs="$(grep -oE 'up --timeout=[0-9]+' "$DML_STUB_TS_CALL_LOG" | head -1 | grep -oE '[0-9]+$')"
  [ -n "$secs" ]
  [ "$secs" -ge 30 ]
}

@test "tailscale up: a URL the daemon holds is recovered when up printed none" {
  # `up` times out WITHOUT printing a URL (empty DML_STUB_TS_UP_URL), but
  # tailscaled has since received one. Before the fix this was a dead end: the
  # user got a timeout and never saw the link that would have let them finish
  # the login on any device.
  export DML_STUB_TS_UP_URL=""
  export DML_STUB_TS_AUTH_URL="https://login.tailscale.com/a/e73516d017e7e"
  export DML_STUB_TS_STATE="NeedsLogin"
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.connected')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.auth_url')" = "https://login.tailscale.com/a/e73516d017e7e" ]
}

@test "tailscale up: a daemon that cannot start names ITS cause, not a login timeout" {
  # systemctl enable fails AND the daemon does not answer -> the honest error is
  # the daemon, reported immediately. Before the fix this was swallowed
  # (best-effort, result discarded) and the user waited out the whole login
  # timeout to be told only that it had timed out.
  export DML_STUB_SYSTEMCTL_ENABLE_EXIT=1
  export DML_STUB_TS_STATUS_EXIT=1
  # The call log MUST be exported for the "never reached up" assertion below to
  # mean anything. Unset, it defaulted to /dev/null and `[ ! -s /dev/null ]` was
  # always TRUE, so that whole property was unprotected -- caught by adversarial
  # review, 2026-07-29, and exactly the dead-assertion class this repo keeps
  # relearning.
  export DML_STUB_TS_CALL_LOG="$FIXTURE/ts-calls-daemonfail.log"
  run bash "$DML" wow tailscale up --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "TAILSCALE_DAEMON_FAILED" ]
  # The hint must carry systemctl's OWN words -- that is the whole point.
  echo "$output" | jq -r '.error.hint' | grep -qi 'tailscaled.service not found'
  # And it must NOT have spent the login timeout first: the log exists (the
  # precondition's own `status` probe is in it) but no `up` was ever attempted.
  [ -f "$DML_STUB_TS_CALL_LOG" ]
  ! grep -q '^up ' "$DML_STUB_TS_CALL_LOG"
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
