#!/usr/bin/env bats
load helpers/env.bash

# Round L: `wow update-check` / `wow update`. See docs/superpowers/specs/
# 2026-07-18-server-update-design.md. Git stub defaults (env.bash) already
# match the expected Playerbots fork/branch, so most tests below don't need
# to touch DML_STUB_GIT_URL/DML_STUB_GIT_BRANCH -- only the two gate tests
# deliberately break one of them.

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  use_git_stub
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  mkdir -p "$SDIR/.git"
}
teardown() { teardown_fixture; }

@test "update: AC remote mismatch fails closed, no pull attempted" {
  export DML_STUB_GIT_URL="https://github.com/someoneelse/azerothcore-wotlk.git"
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow update --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'REMOTE_MISMATCH'
  ! grep -q 'pull --ff-only' "$FIXTURE/git.log"
}

@test "update: AC branch mismatch fails closed, no pull attempted" {
  export DML_STUB_GIT_BRANCH="master"
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow update --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BRANCH_MISMATCH'
  ! grep -q 'pull --ff-only' "$FIXTURE/git.log"
}

@test "update: backup choice is required" {
  run bash "$DML" wow update --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  echo "$output" | grep -qi 'backup'
}

@test "update --no-backup: clean tree already up to date, no pending marker" {
  export DML_STUB_GIT_HEAD_SEQ="abc1111 abc1111"
  export DML_STUB_GIT_HEAD_SEQ_STATE="$FIXTURE/headseq"
  run bash "$DML" wow update --no-backup --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"event":"done"'
  echo "$output" | grep -q '"changed":false'
  echo "$output" | grep -q '"ac":"up to date"'
  [ ! -f "$SDIR/.dml-rebuild-pending" ]
}

@test "update --no-backup: new commits pulled -> core-update pending + done changed:true (PIN: not a cpp module row)" {
  export DML_STUB_GIT_HEAD_SEQ="abc1111 def2222"
  export DML_STUB_GIT_HEAD_SEQ_STATE="$FIXTURE/headseq"
  run bash "$DML" wow update --no-backup --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"changed":true'
  echo "$output" | grep -q '"ac":"abc1111 -> def2222"'
  grep -qxF 'core-update' "$SDIR/.dml-rebuild-pending"

  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.rebuild_pending | index("core-update") != null')" = "true" ]
  [ "$(echo "$output" | jq -r '[.data.families.cpp[] | select(.key=="core-update")] | length')" = "0" ]
}

@test "update --no-backup: dirty tree -> patch file + stash push/pop logged" {
  export DML_STUB_GIT_DIRTY=" M src/foo.cpp"
  export DML_STUB_GIT_HEAD_SEQ="abc1111 def2222"
  export DML_STUB_GIT_HEAD_SEQ_STATE="$FIXTURE/headseq"
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow update --no-backup --json
  [ "$status" -eq 0 ]
  patch="$(ls "$SDIR"/local-changes-*.patch 2>/dev/null | head -1)"
  [ -n "$patch" ]
  [ -s "$patch" ]
  grep -q 'stash push' "$FIXTURE/git.log"
  grep -q 'stash pop' "$FIXTURE/git.log"
}

@test "update --no-backup: pull failure restores stash + PULL_FAILED" {
  export DML_STUB_GIT_DIRTY=" M src/foo.cpp"
  export DML_STUB_GIT_PULL_EXIT=1
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow update --no-backup --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'PULL_FAILED'
  grep -q 'stash pop' "$FIXTURE/git.log"
}

@test "update --no-backup: stash pop conflict -> checkout -f + reset --hard + both recovery warnings" {
  export DML_STUB_GIT_DIRTY=" M src/foo.cpp"
  export DML_STUB_GIT_STASH_POP_EXIT=1
  export DML_STUB_GIT_HEAD_SEQ="abc1111 def2222"
  export DML_STUB_GIT_HEAD_SEQ_STATE="$FIXTURE/headseq"
  export DML_STUB_GIT_LOG="$FIXTURE/git.log"
  run bash "$DML" wow update --no-backup --json
  [ "$status" -eq 0 ]
  grep -q 'checkout -f -- .' "$FIXTURE/git.log"
  grep -q 'reset --hard HEAD' "$FIXTURE/git.log"
  echo "$output" | grep -q 'local-changes-'
  echo "$output" | grep -q 'git stash pop'
}

@test "update-check: repo shape (url/branch/head/dirty/behind), module omitted with a note" {
  export DML_STUB_GIT_BEHIND=3
  export DML_STUB_GIT_DIRTY=" M src/foo.cpp"
  export DML_STUB_GIT_HEAD_SEQ="abc1111"
  export DML_STUB_GIT_HEAD_SEQ_STATE="$FIXTURE/headseq"
  run bash "$DML" wow update-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.repos | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].label')" = "AzerothCore" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].branch')" = "Playerbot" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].head')" = "abc1111" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].dirty')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].behind')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.note')" != "null" ]
}
