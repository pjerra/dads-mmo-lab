#!/usr/bin/env bats
# Structural validation of the AGPL Eluna bridge scripts. We can't run mod-ale
# here, so we assert the load-bearing invariants: the ON_COMMAND hook id, the
# console/SOAP origin gate, the EXACT playerbot command strings (the
# `playerbots bot addclass` path nuance is easy to get wrong), and that each
# returns false to suppress the not-found.
LUA_DIR="$BATS_TEST_DIRNAME/../lua/party"

@test "all three bridge scripts exist" {
  [ -f "$LUA_DIR/dml_addclass.lua" ]
  [ -f "$LUA_DIR/dml_uninvite.lua" ]
  [ -f "$LUA_DIR/dml_login.lua" ]
}

@test "each script registers PLAYER_EVENT_ON_COMMAND (hook 42)" {
  grep -q 'RegisterPlayerEvent(42,' "$LUA_DIR/dml_addclass.lua"
  grep -q 'RegisterPlayerEvent(42,' "$LUA_DIR/dml_uninvite.lua"
  grep -q 'RegisterPlayerEvent(42,' "$LUA_DIR/dml_login.lua"
}

@test "each script gates to console/SOAP origin (player == nil)" {
  grep -qE 'if +player +~= +nil +then +return' "$LUA_DIR/dml_addclass.lua"
  grep -qE 'if +player +~= +nil +then +return' "$LUA_DIR/dml_uninvite.lua"
  grep -qE 'if +player +~= +nil +then +return' "$LUA_DIR/dml_login.lua"
}

@test "addclass runs the correct 'playerbots bot addclass' command path" {
  grep -q 'playerbots bot addclass' "$LUA_DIR/dml_addclass.lua"
  # Must NOT use the intuitive-but-wrong 'playerbots addclass' (no 'bot').
  # Anchored to a Lua STRING LITERAL: the header comment legitimately names
  # the wrong form in prose (backtick-quoted), so a raw whole-file grep reads
  # the explanation as a violation (the strip-COMMENTS-first gotcha) -- the
  # old bare-! form was masking exactly that false match.
  [ "$(grep -cE "[\"']playerbots addclass" "$LUA_DIR/dml_addclass.lua")" = "0" ]
  grep -q 'RunCommand' "$LUA_DIR/dml_addclass.lua"
}

@test "login runs 'playerbots bot login'; uninvite uses RemoveFromGroup" {
  grep -q 'playerbots bot login' "$LUA_DIR/dml_login.lua"
  grep -q 'RemoveFromGroup' "$LUA_DIR/dml_uninvite.lua"
}

@test "each handler returns false to suppress the not-found" {
  grep -q 'return false' "$LUA_DIR/dml_addclass.lua"
  grep -q 'return false' "$LUA_DIR/dml_uninvite.lua"
  grep -q 'return false' "$LUA_DIR/dml_login.lua"
}

@test "scripts carry an AGPL/Dad's MMO Lab header, not Lab bytes" {
  grep -qiE "Dad's MMO Lab" "$LUA_DIR/dml_addclass.lua"
  grep -qiE "Dad's MMO Lab" "$LUA_DIR/dml_uninvite.lua"
  grep -qiE "Dad's MMO Lab" "$LUA_DIR/dml_login.lua"
}

@test "each bridge matches the exact dml_<verb> token the CLI fires" {
  # Pins the literal match token in each script's command:match() pattern to
  # the dml_<verb> prefix the CLI actually fires (50-party.sh / 90-main.sh).
  # A rename on either side (CLI or Lua) that drifts the token would break
  # the relay silently in-game while both suites stayed green -- this test
  # fails that drift here instead.
  grep -q 'dml_addclass%s' "$LUA_DIR/dml_addclass.lua"
  grep -q 'dml_uninvite%s' "$LUA_DIR/dml_uninvite.lua"
  grep -q 'dml_login%s' "$LUA_DIR/dml_login.lua"
}

# ---------- dml_whisper (My Party phase 2) ----------

@test "whisper bridge exists with an AGPL/Dad's MMO Lab header" {
  [ -f "$LUA_DIR/dml_whisper.lua" ]
  grep -qi 'AGPL' "$LUA_DIR/dml_whisper.lua"
  grep -qiE "Dad's MMO Lab" "$LUA_DIR/dml_whisper.lua"
}

@test "whisper bridge registers hook 42 and gates BEFORE matching" {
  grep -q 'RegisterPlayerEvent(42,' "$LUA_DIR/dml_whisper.lua"
  gate_line=$(grep -n 'if player ~= nil then return end' "$LUA_DIR/dml_whisper.lua" | head -1 | cut -d: -f1)
  match_line=$(grep -n 'command:match' "$LUA_DIR/dml_whisper.lua" | head -1 | cut -d: -f1)
  [ "$gate_line" -lt "$match_line" ]
}

@test "whisper bridge pins the dml_whisper token with a greedy message capture" {
  grep -q 'dml_whisper%s' "$LUA_DIR/dml_whisper.lua"
  grep -qF '(.+)$' "$LUA_DIR/dml_whisper.lua"
}

@test "whisper bridge resolves BOTH the player and the bot" {
  [ "$(grep -c 'GetPlayerByName' "$LUA_DIR/dml_whisper.lua")" -ge 2 ]
}

@test "whisper bridge sends via Player:Whisper (universal language)" {
  grep -q 'Whisper(msg, 0, b)' "$LUA_DIR/dml_whisper.lua"
}

@test "whisper bridge handler returns false to consume the command" {
  grep -q 'return false' "$LUA_DIR/dml_whisper.lua"
}
