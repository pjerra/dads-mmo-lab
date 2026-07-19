#!/usr/bin/env bats
# Batch 1 Feature 3: per-module conf editing -- dynamic editable-file list,
# dist-fallback reads, and reset-from-.dist.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  ETC="$GDIR/env/dist/etc"
  MODS="$ETC/modules"
  mkdir -p "$MODS"
}
teardown() { teardown_fixture; }

@test "config files lists the fixed four plus every discovered module conf with exists/dist flags" {
  printf 'w\n' > "$ETC/worldserver.conf"
  printf 'w\n' > "$ETC/worldserver.conf.dist"
  printf 'p\n' > "$MODS/playerbots.conf"
  printf 'p\n' > "$MODS/playerbots.conf.dist"
  printf 'a\n' > "$MODS/mod_ahbot.conf.dist"     # dist-only (not created yet)
  printf 't\n' > "$MODS/transmog.conf"           # conf-only (no dist, like live)
  run bash "$DML" wow config files --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.files | length')" = "7" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name==".env") | .readonly')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="docker-compose.override.yml") | .readonly')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="worldserver.conf") | .exists')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="worldserver.conf") | .dist')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="authserver.conf") | .exists')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="mod_ahbot.conf") | .exists')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="mod_ahbot.conf") | .dist')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="transmog.conf") | .exists')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="transmog.conf") | .dist')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.files[] | select(.name=="playerbots.conf") | .readonly')" = "false" ]
}

@test "traversal-shaped and unknown names are rejected everywhere" {
  printf 't\n' > "$MODS/transmog.conf"
  for bad in '../transmog.conf' 'a/b.conf' '../../etc/passwd' 'transmog.conf.dist' 'nope.conf'; do
    run bash "$DML" wow config raw-read --file "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
    run bash -c 'printf "x\n" | bash "'"$DML"'" wow config raw-write --file "'"$bad"'" --json'
    [ "$status" -eq 1 ]
    run bash "$DML" wow config raw-reset --file "$bad" --json
    [ "$status" -eq 1 ]
  done
}

@test "a discovered module conf (transmog-style, no dist) is read/write editable" {
  printf 'Transmog.Enable = 1\n' > "$MODS/transmog.conf"
  run bash "$DML" wow config raw-read --file transmog.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "conf" ]
  [ "$(echo "$output" | jq -r '.data.content')" = "Transmog.Enable = 1" ]
  run bash -c 'printf "Transmog.Enable = 0\n" | bash "'"$DML"'" wow config raw-write --file transmog.conf --json'
  [ "$status" -eq 0 ]
  [ "$(cat "$MODS/transmog.conf")" = "Transmog.Enable = 0" ]
  [ "$(cat "$MODS/transmog.conf.bak")" = "Transmog.Enable = 1" ]
}

@test "raw-read serves the .dist when only the dist exists" {
  printf 'Ahbot.Enable = 0\n' > "$MODS/mod_ahbot.conf.dist"
  run bash "$DML" wow config raw-read --file mod_ahbot.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "dist" ]
  [ "$(echo "$output" | jq -r '.data.content')" = "Ahbot.Enable = 0" ]
}

@test "raw-reset copies the dist over the conf and keeps a .bak" {
  printf 'default = 1\n' > "$MODS/playerbots.conf.dist"
  printf 'edited = 1\n' > "$MODS/playerbots.conf"
  run bash "$DML" wow config raw-reset --file playerbots.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.reset')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.backup')" = "playerbots.conf.bak" ]
  [ "$(cat "$MODS/playerbots.conf")" = "default = 1" ]
  [ "$(cat "$MODS/playerbots.conf.bak")" = "edited = 1" ]
}

@test "raw-reset refuses .env/override and errors NOT_FOUND without a dist" {
  run bash "$DML" wow config raw-reset --file .env --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config raw-reset --file docker-compose.override.yml --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  printf 't\n' > "$MODS/transmog.conf"
  run bash "$DML" wow config raw-reset --file transmog.conf --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  [ "$(cat "$MODS/transmog.conf")" = "t" ]
}
