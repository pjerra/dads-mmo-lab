#!/usr/bin/env bats
# End-to-end contract tests against the BUILT cli/dml artifact.

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
}

@test "version --json returns success envelope with semver" {
  run bash "$DML" version --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.version')" = "3.0.0" ]
}

@test "version without --json keeps legacy text output" {
  run bash "$DML" version
  [ "$status" -eq 0 ]
  [ "$output" = "dml v3.0.0" ]
}

@test "unknown command in json mode returns UNKNOWN_COMMAND envelope and exit 1" {
  run bash "$DML" frobnicate --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "UNKNOWN_COMMAND" ]
}

@test "--json may appear before the command too" {
  run bash "$DML" --json version
  [ "$(echo "$output" | jq -r '.data.version')" = "3.0.0" ]
}

@test "no arguments prints help and exits 0" {
  run bash "$DML"
  [ "$status" -eq 0 ]
  [[ "$output" == *"dml -- Dad's MMO Lab CLI"* ]]
}

@test "duplicate --json flags are tolerated" {
  run bash "$DML" version --json --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.version')" = "3.0.0" ]
}
