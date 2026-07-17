#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
}
teardown() { teardown_fixture; }

@test "module conf reports state + conf name" {
  mkdir -p "$SDIR/modules/mod-transmog/conf"
  touch "$SDIR/modules/mod-transmog/conf/transmog.conf.dist"
  run bash "$DML" wow module conf --key mod-transmog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.state')" = "ready" ]
  [ "$(echo "$output" | jq -r '.data.conf_name')" = "transmog.conf" ]
}

@test "module conf: module without a conf -> state none, conf_name null" {
  run bash "$DML" wow module conf --key mod-junk-to-gold --json
  [ "$(echo "$output" | jq -r '.data.state')" = "none" ]
  [ "$(echo "$output" | jq -r '.data.conf_name')" = "null" ]
}

@test "conf-activate copies dist -> active" {
  mkdir -p "$SDIR/modules/mod-transmog/conf"
  printf 'Transmog.Enable = 1\n' > "$SDIR/modules/mod-transmog/conf/transmog.conf.dist"
  run bash "$DML" wow module conf-activate --key mod-transmog --json
  [ "$status" -eq 0 ]
  [ -f "$SDIR/env/dist/etc/modules/transmog.conf" ]
  grep -q 'Transmog.Enable' "$SDIR/env/dist/etc/modules/transmog.conf"
}

@test "conf-activate: existing active needs --force" {
  mkdir -p "$SDIR/modules/mod-transmog/conf" "$SDIR/env/dist/etc/modules"
  printf 'new\n' > "$SDIR/modules/mod-transmog/conf/transmog.conf.dist"
  printf 'old\n' > "$SDIR/env/dist/etc/modules/transmog.conf"
  run bash "$DML" wow module conf-activate --key mod-transmog --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'EXISTS'
  grep -q 'old' "$SDIR/env/dist/etc/modules/transmog.conf"
  run bash "$DML" wow module conf-activate --key mod-transmog --force --json
  [ "$status" -eq 0 ]
  grep -q 'new' "$SDIR/env/dist/etc/modules/transmog.conf"
}

@test "conf-activate: no dist yet -> NEEDS_REBUILD; no conf at all -> NO_CONF" {
  mkdir -p "$SDIR/modules/mod-transmog"
  run bash "$DML" wow module conf-activate --key mod-transmog --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NEEDS_REBUILD'
  run bash "$DML" wow module conf-activate --key mod-junk-to-gold --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NO_CONF'
}
