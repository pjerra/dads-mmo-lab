#!/usr/bin/env bats
# Batch 5 F2: `dml wow module client-patch --key mod-arac` -- ARAC's server
# DBC copies (throwaway container into the data volume) + client Patch-A.MPQ.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  use_docker_stub
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  export DML_STUB_CALL_LOG="$FIXTURE/docker.log"
}
teardown() { teardown_fixture; }

install_arac() {
  mkdir -p "$SDIR/modules/mod-arac/.git" "$SDIR/modules/mod-arac/patch-contents/DBFilesContent"
  printf 'dbc1' > "$SDIR/modules/mod-arac/patch-contents/DBFilesContent/CharBaseInfo.dbc"
  printf 'dbc2' > "$SDIR/modules/mod-arac/patch-contents/DBFilesContent/CharStartOutfit.dbc"
  printf 'dbc3' > "$SDIR/modules/mod-arac/patch-contents/DBFilesContent/SkillRaceClassInfo.dbc"
  printf 'mpq' > "$SDIR/modules/mod-arac/Patch-A.MPQ"
}

set_client() {
  mkdir -p "$FIXTURE/wowclient/Data" "$FIXTURE/.dml"
  touch "$FIXTURE/wowclient/Wow.exe"
  printf '%s\n' "$FIXTURE/wowclient" > "$FIXTURE/.dml/client-path"
}

@test "client-patch refuses any key but mod-arac" {
  run bash "$DML" wow module client-patch --key mod-transmog --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"BAD_ARG"'
}

@test "client-patch on an uninstalled mod-arac is NOT_INSTALLED" {
  run bash "$DML" wow module client-patch --key mod-arac --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_INSTALLED'
}

@test "client-patch resolves the volume via docker inspect Mounts and copies all 3 DBCs" {
  install_arac
  export DML_STUB_MOUNT_VOLUME="stub-vol"
  run bash "$DML" wow module client-patch --key mod-arac --json
  [ "$status" -eq 0 ]
  grep -q 'inspect ac-worldserver' "$DML_STUB_CALL_LOG"
  # one docker run per DBC file, volume from inspect, dest under /data/dbc/
  for f in CharBaseInfo.dbc CharStartOutfit.dbc SkillRaceClassInfo.dbc; do
    grep -q -- "run --rm -v stub-vol:/data .*$f:ro alpine cp /src/$f /data/dbc/$f" "$DML_STUB_CALL_LOG"
  done
  [ "$(grep -c '^run ' "$DML_STUB_CALL_LOG")" = "3" ]
  echo "$output" | grep -q '"dbc_files":3'
  echo "$output" | grep -q '"restart_required":true'
}

@test "client-patch falls back to the deploy's real volume name when inspect yields nothing" {
  install_arac
  # DML_STUB_MOUNT_VOLUME unset -> stub prints empty -> fallback path
  run bash "$DML" wow module client-patch --key mod-arac --json
  [ "$status" -eq 0 ]
  grep -q -- 'run --rm -v wow-server-playerbots_ac-client-data:/data' "$DML_STUB_CALL_LOG"
  # exact-match the full fallback string, not the manager's bare ac-client-data
  [ "$(grep -cE -- '-v ac-client-data:/data' "$DML_STUB_CALL_LOG")" = "0" ]
  echo "$output" | grep -q 'using the default name wow-server-playerbots_ac-client-data'
}

@test "client-patch without a client path warns + skips the MPQ but the server step still lands" {
  install_arac
  run bash "$DML" wow module client-patch --key mod-arac --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'no client folder set'
  echo "$output" | grep -q '"client_patched":false'
  echo "$output" | grep -q '"dbc_files":3'
}

@test "client-patch with a client path installs Patch-A.MPQ into Data/ root (never a locale subfolder)" {
  install_arac
  set_client
  run bash "$DML" wow module client-patch --key mod-arac --json
  [ "$status" -eq 0 ]
  [ -f "$FIXTURE/wowclient/Data/Patch-A.MPQ" ]
  [ ! -e "$FIXTURE/wowclient/Data/enUS/Patch-A.MPQ" ]
  echo "$output" | grep -q '"client_patched":true'
}

@test "client-patch with a missing DBC dir errors before any docker run" {
  mkdir -p "$SDIR/modules/mod-arac/.git"
  run bash "$DML" wow module client-patch --key mod-arac --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
  ! grep -q '^run ' "$DML_STUB_CALL_LOG" 2>/dev/null
}

@test "installing mod-arac never marks a rebuild (data-only), other cpp modules still do" {
  use_git_stub
  run bash "$DML" wow module install --family cpp --key mod-arac --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"rebuild_required":false'
  run grep -qxF 'mod-arac' "$SDIR/.dml-rebuild-pending"
  [ "$status" -ne 0 ]   # absent file or absent entry, either way: no pending mark
  run bash "$DML" wow module install --family cpp --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"rebuild_required":true'
  grep -qxF 'mod-aoe-loot' "$SDIR/.dml-rebuild-pending"
}
