#!/usr/bin/env bats
# Server display names (`dml games name`).
#
# The name lives in the SERVER's own directory (<title dir>/.dml-name), not in
# launcher.json: it is a property of the server, so it survives a launcher
# reinstall, travels with the directory, and is readable from both backends.
# These tests pin that storage location as hard as they pin the envelopes --
# moving the file into launcher config would pass every envelope assertion and
# still break the feature's whole point.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

# --- set / clear ------------------------------------------------------------

@test "games name --set writes .dml-name in the server's own directory" {
  add_game wow-server-playerbots compose
  run bash "$DML" games name wow-server-playerbots --set "Dad's Server" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.id')" = "wow-server-playerbots" ]
  [ "$(echo "$output" | jq -r '.data.name')" = "Dad's Server" ]
  [ "$(cat "$DML_GAMES_DIR/wow-server-playerbots/.dml-name")" = "Dad's Server" ]
}

@test "games name --clear removes the file and reports name:null" {
  add_game wow-server-playerbots compose
  run bash "$DML" games name wow-server-playerbots --set "Renamed" --json
  [ "$status" -eq 0 ]
  run bash "$DML" games name wow-server-playerbots --clear --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.name')" = "null" ]
  [ ! -e "$DML_GAMES_DIR/wow-server-playerbots/.dml-name" ]
}

@test "games name --clear is idempotent when no name was ever set" {
  add_game wow-server-playerbots compose
  run bash "$DML" games name wow-server-playerbots --clear --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.name')" = "null" ]
}

@test "games name --set names a home-kind title installed outside the games dir" {
  # wow-vanilla-server etc. install to $HOME/<id> with a games/ symlink; the
  # name file must land next to the SERVER either way.
  mkdir -p "$FIXTURE/maplestory-server"
  run bash "$DML" games name maplestory-server --set "Maple" --json
  [ "$status" -eq 0 ]
  [ "$(cat "$FIXTURE/maplestory-server/.dml-name")" = "Maple" ]
}

# --- name rules -------------------------------------------------------------

@test "games name --set trims surrounding whitespace" {
  add_game wow-server-playerbots compose
  run bash "$DML" games name wow-server-playerbots --set "   Spaced Out   " --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.name')" = "Spaced Out" ]
  [ "$(cat "$DML_GAMES_DIR/wow-server-playerbots/.dml-name")" = "Spaced Out" ]
}

@test "games name --set rejects an empty or whitespace-only name" {
  add_game wow-server-playerbots compose
  run bash "$DML" games name wow-server-playerbots --set "" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG"  ]
  run bash "$DML" games name wow-server-playerbots --set "    " --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -e "$DML_GAMES_DIR/wow-server-playerbots/.dml-name" ]
}

@test "games name --set caps the name at 40 characters" {
  add_game wow-server-playerbots compose
  ok="$(printf 'a%.0s' $(seq 1 40))"
  run bash "$DML" games name wow-server-playerbots --set "$ok" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.name')" = "$ok" ]
  toolong="$(printf 'a%.0s' $(seq 1 41))"
  run bash "$DML" games name wow-server-playerbots --set "$toolong" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  # The rejected write must not have clobbered the accepted one.
  [ "$(cat "$DML_GAMES_DIR/wow-server-playerbots/.dml-name")" = "$ok" ]
}

@test "games name --set refuses control characters, CR and LF" {
  add_game wow-server-playerbots compose
  # A newline in the value would read back as a DIFFERENT name (the reader
  # takes the first line only), so silently accepting it is a lie.
  run bash "$DML" games name wow-server-playerbots --set "$(printf 'Line1\nLine2')" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" games name wow-server-playerbots --set "$(printf 'Carriage\rReturn')" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" games name wow-server-playerbots --set "$(printf 'Tab\there')" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ ! -e "$DML_GAMES_DIR/wow-server-playerbots/.dml-name" ]
}

@test "games name --set stores shell metacharacters literally, never executes them" {
  # The name is a plain file body, never spliced into a command string.
  add_game wow-server-playerbots compose
  run bash "$DML" games name wow-server-playerbots --set '$(touch pwned) `id` ; rm -rf /' --json
  [ "$status" -eq 0 ]
  [ "$(cat "$DML_GAMES_DIR/wow-server-playerbots/.dml-name")" = '$(touch pwned) `id` ; rm -rf /' ]
  [ ! -e "$FIXTURE/pwned" ]
  [ ! -e "$DML_GAMES_DIR/pwned" ]
  # ...and it survives the round-trip through games list's JSON intact.
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].display_name')" = '$(touch pwned) `id` ; rm -rf /' ]
}

# --- argument handling ------------------------------------------------------

@test "games name: unknown or uninstalled title -> NOT_FOUND, nothing written" {
  run bash "$DML" games name not-a-title --set "Nope" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  [ ! -e "$DML_GAMES_DIR/not-a-title" ]
  # A registry title that is not installed has no directory to name either.
  run bash "$DML" games name runescape-server --set "Nope" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "games name: path-shaped ids are refused before any file write" {
  add_game wow-server-playerbots compose
  for bad in ".." "../escape" "a/b" 'x;touch y'; do
    run bash "$DML" games name "$bad" --set "Nope" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
  [ ! -e "$DML_GAMES_DIR/.dml-name" ]
  [ ! -e "$FIXTURE/.dml-name" ]
}

@test "games name: missing title / missing flag / missing value -> BAD_ARG envelope" {
  add_game wow-server-playerbots compose
  run bash "$DML" games name --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" games name wow-server-playerbots --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  # Value flag as the last token: under `set -u` an unguarded $2 read would
  # abort with no envelope at all.
  run bash "$DML" games name wow-server-playerbots --set
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" games name wow-server-playerbots --rename "x" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

# --- reading (defensive) ----------------------------------------------------
#
# .dml-name is plain text a user may hand-edit (or copy in from Windows), so
# the READER never trusts it. A server must never render as a blank label.

@test "games list: an empty or whitespace-only name file falls back to the registry name" {
  add_game wow-server-playerbots compose
  : > "$DML_GAMES_DIR/wow-server-playerbots/.dml-name"
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].display_name')" = "WoW WotLK (Playerbots)" ]
  printf '   \n' > "$DML_GAMES_DIR/wow-server-playerbots/.dml-name"
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].display_name')" = "WoW WotLK (Playerbots)" ]
}

@test "games list: a hand-edited name file is read first-line-only, trimmed and capped" {
  add_game wow-server-playerbots compose
  printf '  Line one  \nline two\n' > "$DML_GAMES_DIR/wow-server-playerbots/.dml-name"
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].display_name')" = "Line one" ]
  # CRLF from a Windows editor must not leak a \r into the label.
  printf 'Windows Edited\r\n' > "$DML_GAMES_DIR/wow-server-playerbots/.dml-name"
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].display_name')" = "Windows Edited" ]
  # An over-long hand-written name is capped to the same 40 the writer enforces
  # (also the no-trailing-newline case: `read` returns nonzero at EOF but has
  # already assigned, so a naive `read || return` would drop the name entirely).
  printf 'b%.0s' $(seq 1 60) > "$DML_GAMES_DIR/wow-server-playerbots/.dml-name"
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].display_name')" = "$(printf 'b%.0s' $(seq 1 40))" ]
}

# --- games list / catalog carry the name -----------------------------------

@test "games list --json gains display_name without disturbing the existing keys" {
  add_game wow-server-playerbots compose
  add_game tortoise nested
  run bash "$DML" games name wow-server-playerbots --set "Dad's Server" --json
  [ "$status" -eq 0 ]
  run bash "$DML" games list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="wow-server-playerbots") | .display_name')" = "Dad's Server" ]
  # No custom name, not in the registry either -> the id, never blank.
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="tortoise") | .display_name')" = "tortoise" ]
  # Existing keys unchanged.
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="wow-server-playerbots") | .running')" = "false" ]
  [[ "$(echo "$output" | jq -r '.data.games[] | select(.id=="tortoise") | .path')" == */tortoise/sub ]]
}

@test "games catalog --json gains display_name and custom_name" {
  add_game wow-server-playerbots compose
  run bash "$DML" games catalog --json
  [ "$status" -eq 0 ]
  # Not renamed: display_name is the registry name, custom_name is null.
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .display_name')" = "WoW WotLK (Playerbots)" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .custom_name')" = "null" ]
  # Not installed at all: still a name, still no custom name.
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .display_name')" = "RuneScape" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .custom_name')" = "null" ]

  run bash "$DML" games name wow-server-playerbots --set "Dad's Server" --json
  [ "$status" -eq 0 ]
  run bash "$DML" games catalog --json
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .display_name')" = "Dad's Server" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .custom_name')" = "Dad's Server" ]
  # Existing keys unchanged.
  [ "$(echo "$output" | jq -r '.data.titles | length')" = "6" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .name')" = "WoW WotLK (Playerbots)" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .installed')" = "true" ]
}
