#!/usr/bin/env bats
# Incident follow-up 1 (2026-07-21): `dml wow docker-restart` -- restart the
# Docker DAEMON inside dml-arch. The night of the incident the whole fix was
# one `systemctl restart docker` that nothing in the launcher could click.
#
# The arm runs as the unprivileged `dml` user, so every privileged call goes
# through `sudo -n`; the whole tool-chain (sudo/systemctl/docker) is stubbed by
# use_docker_restart_stub, and DML_SYSTEMCTL_BIN points at a bogus name to
# exercise the no-systemd path deterministically (same seam convention as
# DML_TS_BIN / DML_YQ_BIN).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_restart_stub
  export HOME="$FIXTURE"
  # Keep the bounded readiness wait short so failure paths cost ~2s instead of
  # the 30s production default. The tests that care about the wait assert on
  # measured elapsed time, never on this value being honoured silently.
  export DML_DOCKER_RESTART_TIMEOUT=2
}
teardown() { teardown_fixture; }

# --- systemd availability ---------------------------------------------------

@test "docker-restart: no systemctl in the distro -> NOT_SUPPORTED" {
  export DML_SYSTEMCTL_BIN="systemctl-absent-xyz"
  run bash "$DML" wow docker-restart --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_SUPPORTED" ]
}

@test "docker-restart: systemd not running -> NOT_SUPPORTED and nothing is restarted" {
  export DML_STUB_SYSTEMD_STATE=offline
  export DML_STUB_SYSTEMCTL_LOG="$FIXTURE/systemctl.log"
  run bash "$DML" wow docker-restart --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_SUPPORTED" ]
  # The probe ran, the restart did NOT -- a refusal that still kicked the
  # daemon would be the worst of both worlds.
  [ -f "$DML_STUB_SYSTEMCTL_LOG" ]
  grep -q 'is-system-running' "$DML_STUB_SYSTEMCTL_LOG"
  ! grep -q 'restart docker' "$DML_STUB_SYSTEMCTL_LOG"
}

@test "docker-restart: degraded systemd is still systemd -> proceeds, restarted:true" {
  # "degraded" is the NORMAL state inside a WSL distro (some unrelated unit
  # failed at boot), and systemd exits 1 for it. Refusing here would refuse on
  # most real boxes.
  export DML_STUB_SYSTEMD_STATE=degraded
  run bash "$DML" wow docker-restart --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.restarted')" = "true" ]
}

# --- sudo -------------------------------------------------------------------

@test "docker-restart: sudo refuses (no NOPASSWD rule) -> NO_SUDO with a guided hint" {
  export DML_STUB_SUDO_FAIL=1
  run bash "$DML" wow docker-restart --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NO_SUDO" ]
  echo "$output" | jq -r '.error.hint' | grep -qi 'DML shell'
}

@test "docker-restart: always passes sudo -n, so it can never hang on a password prompt" {
  export DML_STUB_SUDO_LOG="$FIXTURE/sudo.log"
  run bash "$DML" wow docker-restart --json
  [ "$status" -eq 0 ]
  [ -f "$DML_STUB_SUDO_LOG" ]
  grep -qE '^-n .*systemctl restart docker$' "$DML_STUB_SUDO_LOG"
}

# --- the restart itself -----------------------------------------------------

@test "docker-restart: systemctl restart fails -> RESTART_FAILED carrying the cause" {
  export DML_STUB_SYSTEMCTL_RESTART_EXIT=5
  run bash "$DML" wow docker-restart --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "RESTART_FAILED" ]
  # The hint must carry systemctl's own stderr -- without it there is nothing
  # actionable on screen.
  echo "$output" | jq -r '.error.hint' | grep -q 'Failed to restart docker.service'
}

# --- the bounded wait for the daemon ---------------------------------------

@test "docker-restart: waits for dockerd to answer again and reports how long" {
  # docker info refuses twice, then answers. "systemctl returned 0" is not
  # evidence the daemon is back, so the arm must POLL past both refusals --
  # which costs real seconds an early-returning implementation cannot fake.
  export DML_STUB_DOCKER_INFO_SEQ="1 1 0"
  export DML_STUB_DOCKER_INFO_SEQ_STATE="$FIXTURE/info-seq"
  export DML_DOCKER_RESTART_TIMEOUT=10
  run bash "$DML" wow docker-restart --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.restarted')" = "true" ]
  waited="$(echo "$output" | jq -r '.data.waited_seconds')"
  [ "$waited" -ge 2 ]
}

@test "docker-restart: daemon never comes back -> DOCKER_STILL_DOWN after the full wait" {
  export DML_STUB_DOCKER_DOWN=1
  export DML_DOCKER_RESTART_TIMEOUT=3
  start=$(date +%s)
  run bash "$DML" wow docker-restart --json
  elapsed=$(( $(date +%s) - start ))
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DOCKER_STILL_DOWN" ]
  # An implementation that trusted systemctl's exit code would answer
  # instantly; only a real bounded poll can take the whole timeout.
  [ "$elapsed" -ge 3 ]
}

# --- nothing may block forever ----------------------------------------------
# dml-arch's docker.service is Type=notify with TimeoutStartSec=0, so systemd
# waits INDEFINITELY for dockerd's READY=1 -- precisely the wedged-daemon case
# this arm exists for. Every blocking call is bounded; a bound that is hit is a
# RESTART_TIMEOUT envelope, never a spinner that never settles.

@test "docker-restart: a systemctl restart that never returns -> RESTART_TIMEOUT" {
  export DML_STUB_SYSTEMCTL_RESTART_HANG=25
  export DML_DOCKER_RESTART_CMD_TIMEOUT=2
  t0=$SECONDS
  run bash "$DML" wow docker-restart --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "RESTART_TIMEOUT" ]
  # An unbounded call sits there for the stub's whole 25s (and then reports
  # SUCCESS); the bound cuts it at ~2s and says what happened.
  [ "$elapsed" -lt 15 ]
  echo "$output" | jq -r '.error.message' | grep -qi 'timed out'
}

@test "docker-restart: a docker info poll that never answers still ends at the cap" {
  # docker.service Requires=docker.socket, so the socket ACCEPTS and the
  # request then blocks while dockerd starts -- a poll that hangs rather than
  # one that refuses. Each probe is bounded, and the budget is measured in
  # elapsed time so a blocking probe cannot stretch it.
  export DML_STUB_DOCKER_INFO_HANG=25
  export DML_DOCKER_RESTART_TIMEOUT=3
  t0=$SECONDS
  run bash "$DML" wow docker-restart --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DOCKER_STILL_DOWN" ]
  [ "$elapsed" -lt 20 ]
}

# --- dispatch ---------------------------------------------------------------

@test "docker-restart: unknown flag -> BAD_ARG" {
  run bash "$DML" wow docker-restart --force --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
