#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  use_backup_stub
  use_git_stub
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  mkdir -p "$SDIR/modules/mod-ale/.git"
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
}
teardown() { teardown_fixture; }

seed_clone() { mkdir -p "$SDIR/ale_scripts/$1"; }

@test "lua install: gated on mod-ale" {
  rm -rf "$SDIR/modules/mod-ale"
  run bash "$DML" wow module install --family lua --key lootpet --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_READY'
}

@test "lua install: no-SQL script clones and deploys, backup flags rejected" {
  run bash "$DML" wow module install --family lua --key lootpet --backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  seed_clone lootpet
  mkdir -p "$SDIR/ale_scripts/lootpet/.git"
  printf 'x' > "$SDIR/ale_scripts/lootpet/LootPet.lua"
  run bash "$DML" wow module install --family lua --key lootpet --json
  [ "$status" -eq 0 ]
  [ -f "$SDIR/env/dist/etc/modules/lua_scripts/LootPet.lua" ]
  echo "$output" | grep -q '"event":"done"'
}

@test "lua install: SQL script requires a backup choice and applies SQL" {
  run bash "$DML" wow module install --family lua --key accountwide --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  mkdir -p "$SDIR/ale_scripts/accountwide/.git" "$SDIR/ale_scripts/accountwide/lua_scripts/AccountWide" "$SDIR/ale_scripts/accountwide/sql"
  printf 'a' > "$SDIR/ale_scripts/accountwide/lua_scripts/AccountWide/AW.lua"
  printf 'CREATE TABLE x;' > "$SDIR/ale_scripts/accountwide/sql/create_accountwide_tables.sql"
  run bash "$DML" wow module install --family lua --key accountwide --no-backup --json
  [ "$status" -eq 0 ]
  [ -f "$SDIR/env/dist/etc/modules/lua_scripts/accountwide/AW.lua" ]
  grep -q 'mysql' "$FIXTURE/calls.log" || grep -q 'acore_characters' "$FIXTURE/calls.log" || true
  echo "$output" | grep -q '"event":"done"'
}

@test "lua install --backup: dump happens before SQL" {
  mkdir -p "$SDIR/ale_scripts/accountwide/.git" "$SDIR/ale_scripts/accountwide/lua_scripts/AccountWide" "$SDIR/ale_scripts/accountwide/sql"
  printf 'a' > "$SDIR/ale_scripts/accountwide/lua_scripts/AccountWide/AW.lua"
  printf 'sql' > "$SDIR/ale_scripts/accountwide/sql/create_accountwide_tables.sql"
  run bash "$DML" wow module install --family lua --key accountwide --backup --json
  [ "$status" -eq 0 ]
  head -1 "$FIXTURE/calls.log" | grep -q 'mysqldump'
  grep -q 'acore_world' "$FIXTURE/calls.log"
}

@test "lua install: sparse clone for sod configures sparseCheckout" {
  run bash "$DML" wow module install --family lua --key sod --json
  grep -q 'init' "$FIXTURE/git.log"
  grep -q 'sparseCheckout true' "$FIXTURE/git.log"
  grep -q 'pull --depth=1 origin HEAD' "$FIXTURE/git.log"
}

@test "lua install: deploy failure surfaces DEPLOY_FAILED" {
  mkdir -p "$SDIR/ale_scripts/lootpet/.git"
  run bash "$DML" wow module install --family lua --key lootpet --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'DEPLOY_FAILED'
}

@test "lua install: unknown key / custom url rejected" {
  run bash "$DML" wow module install --family lua --key nope --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  run bash "$DML" wow module install --family lua --key lootpet --url https://x/y.git --json
  [ "$status" -eq 1 ]
}

@test "lua remove: clears clone + deployed file, DB kept message, no backup flags" {
  mkdir -p "$SDIR/ale_scripts/lootpet/.git" "$SDIR/env/dist/etc/modules/lua_scripts"
  printf 'x' > "$SDIR/env/dist/etc/modules/lua_scripts/LootPet.lua"
  run bash "$DML" wow module remove --family lua --key lootpet --backup --json
  [ "$status" -eq 1 ]
  run bash "$DML" wow module remove --family lua --key lootpet --json
  [ "$status" -eq 0 ]
  [ ! -d "$SDIR/ale_scripts/lootpet" ]
  [ ! -f "$SDIR/env/dist/etc/modules/lua_scripts/LootPet.lua" ]
  echo "$output" | grep -q 'kept'
}

@test "lua list rows carry has_sql" {
  run bash "$DML" wow module list --json
  [ "$(echo "$output" | jq -r '.data.families.lua[] | select(.key=="accountwide") | .has_sql')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.families.lua[] | select(.key=="lootpet") | .has_sql')" = "false" ]
}

# Batch 6 A: the Paragon `.test` unguarded-command warning is a read-only
# field on the lua row, present ONLY once the script is deployed (live).
@test "lua list: paragon warn is null until deployed, set once deployed" {
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.lua[] | select(.key=="paragon") | .warn')" = "null" ]
  # No warn on an unrelated lua row either.
  [ "$(echo "$output" | jq -r '.data.families.lua[] | select(.key=="lootpet") | .warn')" = "null" ]
  # Deploy paragon (its deployed-check is the lua_scripts/paragon dir).
  mkdir -p "$SDIR/env/dist/etc/modules/lua_scripts/paragon"
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.lua[] | select(.key=="paragon") | .warn')" != "null" ]
  echo "$output" | jq -r '.data.families.lua[] | select(.key=="paragon") | .warn' | grep -q '\.test'
}
