#!/usr/bin/env bats
# Batch 4 Feature 15: internet play. The `lan` arm's address validation
# matrix (private-only by default, public IPv4/hostname only under
# --internet) plus the best-effort `wow lan public-ip` envelope.
#
# Validation runs BEFORE any docker/database work, so accepted addresses on
# a nonexistent title fall through to "Title not found" -- which is exactly
# how these tests tell "accepted" (reaches the title check) apart from
# "rejected" (address error, never gets there).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "lan on accepts a private IPv4 (falls through to the title check)" {
  for addr in 192.168.1.50 10.0.0.7 172.16.3.2 172.31.9.9 127.0.0.1; do
    run bash "$DML" lan ghost-title on "$addr"
    [ "$status" -eq 1 ]
    echo "$output" | grep -q 'Title not found'
  done
}

@test "lan on rejects a public IPv4 without --internet (private-only default)" {
  run bash "$DML" lan ghost-title on 8.8.8.8
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'not a private LAN address'
  # 172.32.x is OUTSIDE the 172.16-31 private block
  run bash "$DML" lan ghost-title on 172.32.0.1
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'not a private LAN address'
}

@test "lan on rejects a hostname without --internet" {
  run bash "$DML" lan ghost-title on myserver.duckdns.org
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'does not look like an IPv4 address'
}

@test "lan --internet on accepts a public IPv4 and a hostname" {
  run bash "$DML" lan ghost-title --internet on 84.210.13.37
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'Title not found'
  run bash "$DML" lan ghost-title --internet on myserver.duckdns.org
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'Title not found'
}

@test "lan --internet on rejects garbage and injection-shaped input" {
  for bad in 'foo bar' 'evil;DROP TABLE realmlist' 'a`id`' "x'y" '$(reboot)'; do
    run bash "$DML" lan ghost-title --internet on "$bad"
    [ "$status" -eq 1 ]
    echo "$output" | grep -q 'not a valid public address or hostname'
  done
}

@test "lan refresh stays private-only even with --internet elsewhere in use" {
  # refresh is the tray's automatic re-point -- it must never write a
  # public address (the arm's own non-LAN guard already protects a stored
  # public address from being clobbered).
  run bash "$DML" lan ghost-title refresh 8.8.8.8
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'not a private LAN address'
}

@test "wow lan public-ip returns the detected IPv4" {
  use_curl_stub
  printf '84.210.13.37' > "$FIXTURE/ip.txt"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/ip.txt"
  run bash "$DML" wow lan public-ip --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.public_ip')" = "84.210.13.37" ]
}

@test "wow lan public-ip degrades to null on curl failure or a non-IP answer" {
  use_curl_stub
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow lan public-ip --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.public_ip')" = "null" ]
  unset DML_STUB_CURL_EXIT
  printf '<html>captive portal</html>' > "$FIXTURE/portal.txt"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/portal.txt"
  run bash "$DML" wow lan public-ip --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.public_ip')" = "null" ]
}

# --- internet-play LAN fix: --local <lan-ip> --------------------------------
#
# `--local` carries the HOST's own LAN address for realmlist.localAddress, so
# unlike the realm `address` it is private/loopback-only even under
# --internet: a public value there would route players inside the house out
# to the internet, the exact breakage the flag exists to fix.

@test "lan --local accepts a private IPv4 alongside --internet" {
  run bash "$DML" lan ghost-title --internet --local 192.168.1.50 on wow.example.org
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'Title not found'
}

@test "lan --local rejects a public IPv4 even under --internet" {
  run bash "$DML" lan ghost-title --internet --local 84.210.13.37 on wow.example.org
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'not a private LAN address'
}

@test "lan --local rejects a hostname" {
  run bash "$DML" lan ghost-title --internet --local myserver.duckdns.org on wow.example.org
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'does not look like an IPv4 address'
}

@test "lan --local with no value emits BAD_ARG, not an unbound-variable abort" {
  # Every value-taking flag calls _need_flag_val before reading $2 -- without
  # it `set -u` aborts with no envelope at all.
  run bash "$DML" lan ghost-title --local
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'Missing value for --local'
}

@test "lan rejects an unknown leading flag with usage" {
  run bash "$DML" lan ghost-title --bogus on 192.168.1.50
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'Usage: dml lan'
}
