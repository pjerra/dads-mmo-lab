#!/usr/bin/env bats
# Static pins for the GM bridge script (no Eluna runtime in CI).
# Same posture as party-lua.bats: assert the load-bearing invariants.
LUA="$BATS_TEST_DIRNAME/../lua/gm/dml_gm.lua"

@test "gm bridge script exists" {
  [ -f "$LUA" ]
}

@test "gm bridge registers PLAYER_EVENT_ON_COMMAND (hook 42)" {
  grep -q 'RegisterPlayerEvent(42,' "$LUA"
}

@test "gm bridge gates to console/SOAP origin (player == nil)" {
  grep -qE 'if +player +~= +nil +then +return' "$LUA"
}

@test "gm bridge matches the exact dml_gm_* tokens the CLI fires" {
  # Pins the literal match tokens to what 90-main.sh fires. A rename on
  # either side breaks the relay silently in-game -- fail it here instead.
  grep -q 'dml_gm_health%s' "$LUA"
  grep -q 'dml_gm_money%s' "$LUA"
  grep -q 'dml_gm_revive%s' "$LUA"
}

@test "gm bridge saves to DB after each mutation (crash-safe)" {
  [ "$(grep -c 'SaveToDB' "$LUA")" -ge 3 ]
}

@test "gm bridge revive skips resurrection sickness" {
  grep -q 'ResurrectPlayer(1.0, false)' "$LUA"
}

@test "each handler returns false to suppress the not-found" {
  [ "$(grep -c 'return false' "$LUA")" -ge 3 ]
}

@test "gm bridge carries an AGPL/Dad's MMO Lab header, not Lab bytes" {
  grep -qiE "Dad's MMO Lab" "$LUA"
  grep -qi 'AGPL' "$LUA"
}
