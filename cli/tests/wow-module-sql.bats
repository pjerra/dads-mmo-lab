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
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
}
teardown() { teardown_fixture; }

@test "sql install: unknown key BAD_ARG; backup choice required" {
  run bash "$DML" wow module install --family sql --key nope --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  run bash "$DML" wow module install --family sql --key buff-mobs --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  echo "$output" | grep -qi 'backup'
}

@test "sql install tweak: applies multipliers, writes marker with APPLIED values" {
  run bash "$DML" wow module install --family sql --key buff-mobs --no-backup --json
  [ "$status" -eq 0 ]
  grep -q 'mysql' "$FIXTURE/calls.log"
  grep -q 'HealthModifier \* 2' "$FIXTURE/calls.log"
  grep -q 'APPLIED_HP_MULT=2' "$SDIR/sql_scripts/installed/buff-mobs.installed"
}

@test "sql install tweak: sibling auto-removed first (mutual exclusion)" {
  run bash "$DML" wow module install --family sql --key buff-mobs --no-backup --json
  [ "$status" -eq 0 ]
  run bash "$DML" wow module install --family sql --key nerf-mobs --no-backup --json
  [ "$status" -eq 0 ]
  [ ! -f "$SDIR/sql_scripts/installed/buff-mobs.installed" ]
  [ -f "$SDIR/sql_scripts/installed/nerf-mobs.installed" ]
  grep -q '0.500000' "$FIXTURE/calls.log"
}

@test "sql install tweak twice: EXISTS" {
  run bash "$DML" wow module install --family sql --key buff-mobs --no-backup --json
  [ "$status" -eq 0 ]
  run bash "$DML" wow module install --family sql --key buff-mobs --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'EXISTS'
}

@test "sql install --backup: mysqldump precedes tweak SQL" {
  run bash "$DML" wow module install --family sql --key buff-mobs --backup --json
  [ "$status" -eq 0 ]
  head -1 "$FIXTURE/calls.log" | grep -q 'mysqldump'
  grep -q 'acore_world' "$FIXTURE/calls.log"
}

@test "sql install clone_sql: applies up files sorted, skips Down/Example" {
  mkdir -p "$SDIR/sql_scripts/clones/portals-capitals/.git"
  printf 'INSERT INTO x VALUES (1);' > "$SDIR/sql_scripts/clones/portals-capitals/a_Up.sql"
  printf 'DELETE FROM x;' > "$SDIR/sql_scripts/clones/portals-capitals/b_Down.sql"
  printf 'INSERT INTO x VALUES (2);' > "$SDIR/sql_scripts/clones/portals-capitals/zz_Example.sql"
  run bash "$DML" wow module install --family sql --key portals-capitals --no-backup --json
  [ "$status" -eq 0 ]
  [ "$(wc -l < "$FIXTURE/calls.log")" -eq 1 ]
  grep -q 'mysql-import' "$FIXTURE/calls.log"
  [ -f "$SDIR/sql_scripts/installed/portals-capitals.installed" ]
}

@test "sql install hearthstone: variant required + recorded" {
  run bash "$DML" wow module install --family sql --key hearthstone-cd --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  mkdir -p "$SDIR/sql_scripts/clones/hearthstone-cd/.git"
  printf 'UPDATE spell_dbc SET RecoveryTime = 300000, CategoryRecoveryTime = 300000 WHERE Id = 8690;' > "$SDIR/sql_scripts/clones/hearthstone-cd/HS_5min.sql"
  run bash "$DML" wow module install --family sql --key hearthstone-cd --variant 5min --no-backup --json
  [ "$status" -eq 0 ]
  grep -q 'HEARTHSTONE_COOLDOWN=5min' "$SDIR/sql_scripts/installed/hearthstone-cd.installed"
}

@test "sql remove tweak: inverse applied + marker cleared" {
  run bash "$DML" wow module install --family sql --key buff-mobs --no-backup --json
  [ "$status" -eq 0 ]
  run bash "$DML" wow module remove --family sql --key buff-mobs --no-backup --json
  [ "$status" -eq 0 ]
  [ ! -f "$SDIR/sql_scripts/installed/buff-mobs.installed" ]
  grep -q '0.500000' "$FIXTURE/calls.log"
}

@test "sql remove rare-drops: NO_REVERT before any flags/DB touch" {
  mkdir -p "$SDIR/sql_scripts/installed"
  touch "$SDIR/sql_scripts/installed/rare-drops.installed"
  run bash "$DML" wow module remove --family sql --key rare-drops --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NO_REVERT'
  [ -f "$SDIR/sql_scripts/installed/rare-drops.installed" ]
}

@test "sql remove hearthstone: reset statement runs" {
  mkdir -p "$SDIR/sql_scripts/installed"
  touch "$SDIR/sql_scripts/installed/hearthstone-cd.installed"
  run bash "$DML" wow module remove --family sql --key hearthstone-cd --no-backup --json
  [ "$status" -eq 0 ]
  grep -q 'RecoveryTime = 1800000' "$FIXTURE/calls.log"
}

@test "sql install SQL failure: no marker written" {
  export DML_STUB_SQL_EXIT=1
  run bash "$DML" wow module install --family sql --key buff-mobs --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'SQL_FAILED'
  [ ! -f "$SDIR/sql_scripts/installed/buff-mobs.installed" ]
}

@test "sql install hearthstone: 5min does not match the 15min file" {
  mkdir -p "$SDIR/sql_scripts/clones/hearthstone-cd/.git"
  printf 'UPDATE spell_dbc SET RecoveryTime = 300000, CategoryRecoveryTime = 300000 WHERE Id = 8690;' > "$SDIR/sql_scripts/clones/hearthstone-cd/HS_5min.sql"
  printf 'UPDATE spell_dbc SET RecoveryTime = 900000, CategoryRecoveryTime = 900000 WHERE Id = 8690;' > "$SDIR/sql_scripts/clones/hearthstone-cd/HS_15min.sql"
  run bash "$DML" wow module install --family sql --key hearthstone-cd --variant 5min --no-backup --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'applying HS_5min.sql'
  [ "$(echo "$output" | grep -c 'applying HS_15min.sql')" = "0" ]
  grep -q 'HEARTHSTONE_COOLDOWN=5min' "$SDIR/sql_scripts/installed/hearthstone-cd.installed"
}

@test "sql install+remove clone_dist: sed level + teleporter reversal" {
  mkdir -p "$SDIR/sql_scripts/clones/npc-teleporter/.git" "$SDIR/sql_scripts/clones/npc-teleporter/data/sql/db-world"
  printf 'SET @ONY_LEVEL := 60;\n' > "$SDIR/sql_scripts/clones/npc-teleporter/data/sql/db-world/npc.dist"
  run bash "$DML" wow module install --family sql --key npc-teleporter --variant 75 --no-backup --json
  [ "$status" -eq 0 ]
  [ -f "$SDIR/sql_scripts/clones/npc-teleporter_gen_1.sql" ]
  grep -q '@ONY_LEVEL := 75' "$SDIR/sql_scripts/clones/npc-teleporter_gen_1.sql"
  run bash "$DML" wow module remove --family sql --key npc-teleporter --no-backup --json
  [ "$status" -eq 0 ]
  grep -q '190000' "$FIXTURE/calls.log"
  grep -q 'mysql-stmt' "$FIXTURE/calls.log"
}

@test "sql install clone_sql with no up files fails, no marker" {
  mkdir -p "$SDIR/sql_scripts/clones/portals-capitals/.git"
  printf 'DELETE FROM x;' > "$SDIR/sql_scripts/clones/portals-capitals/b_Down.sql"
  run bash "$DML" wow module install --family sql --key portals-capitals --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'SQL_FAILED'
  [ ! -f "$SDIR/sql_scripts/installed/portals-capitals.installed" ]
}

@test "sql remove clone_sql: down file applied, marker cleared" {
  mkdir -p "$SDIR/sql_scripts/clones/portals-capitals/.git"
  printf 'INSERT INTO x VALUES (1);' > "$SDIR/sql_scripts/clones/portals-capitals/a_Up.sql"
  printf 'DELETE FROM x;' > "$SDIR/sql_scripts/clones/portals-capitals/b_Down.sql"
  run bash "$DML" wow module install --family sql --key portals-capitals --no-backup --json
  [ "$status" -eq 0 ]
  run bash "$DML" wow module remove --family sql --key portals-capitals --no-backup --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'applying b_Down.sql'
  [ ! -f "$SDIR/sql_scripts/installed/portals-capitals.installed" ]
}
