#!/usr/bin/env bats
load helpers/env.bash

# Module-update round: `wow module list` head/head_date fields +
# `wow module update-check` + `wow module update --key`. Unlike
# wow-server-update.bats (git stub), this suite runs REAL git against LOCAL
# bare origins inside the mktemp fixture -- zero network -- because the
# behaviors under test ARE the clone/fetch/pull/stash mechanics.

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  export HOME="$FIXTURE"
  # Real commits need an identity; HOME is the fixture, so this global
  # config is fixture-local and dies with teardown.
  git config --global user.email dml-test@example.invalid
  git config --global user.name "dml test"
  git config --global init.defaultBranch master
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  mkdir -p "$SDIR/modules"
}
teardown() { teardown_fixture; }

# Builds a REAL local origin (bare) + an installed module clone of it:
# origin at $FIXTURE/origin-<key>.git, clone at $SDIR/modules/<key>.
# Two tracked files so the dirty-worktree test can edit one (b.txt) while
# the origin's new commit touches the other (a.txt) -- the stash pop then
# re-applies cleanly instead of conflicting.
make_module_repo() {
  local key="$1" work="$FIXTURE/seed-$1"
  git init -q "$work"
  ( cd "$work" && echo one > a.txt && echo keep > b.txt \
    && git add a.txt b.txt && git commit -qm seed )
  git clone -q --bare "$work" "$FIXTURE/origin-$key.git"
  rm -rf "$work"
  git clone -q "$FIXTURE/origin-$key.git" "$SDIR/modules/$key"
}

# Pushes one more commit (touching a.txt) to <key>'s bare origin, so the
# installed clone is exactly 1 behind.
origin_gains_commit() {
  local key="$1" work="$FIXTURE/adv-$1"
  git clone -q "$FIXTURE/origin-$key.git" "$work"
  ( cd "$work" && echo two >> a.txt && git add a.txt \
    && git commit -qm advance && git push -q origin HEAD )
  rm -rf "$work"
}

# --- module list: head / head_date ------------------------------------------

@test "module list: head + head_date populated for an installed registry clone" {
  make_module_repo mod-aoe-loot
  want="$(git -C "$SDIR/modules/mod-aoe-loot" log -1 --format=%h)"
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-aoe-loot") | .head')" = "$want" ]
  echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-aoe-loot") | .head_date' \
    | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'
}

@test "module list: head/head_date null when not installed or .git missing" {
  mkdir -p "$SDIR/modules/mod-transmog"   # dir present but no .git
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-transmog") | .head')" = "null" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-transmog") | .head_date')" = "null" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-aoe-loot") | .head')" = "null" ]
}

@test "module list: custom clone rows carry head/head_date too" {
  make_module_repo mod-my-custom
  want="$(git -C "$SDIR/modules/mod-my-custom" log -1 --format=%h)"
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-my-custom") | .custom')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-my-custom") | .head')" = "$want" ]
  echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-my-custom") | .head_date' \
    | grep -Eq '^[0-9]{4}-[0-9]{2}-[0-9]{2}$'
}

# --- module update-check ----------------------------------------------------

@test "module update-check: clone behind its local bare origin -> behind:1" {
  make_module_repo mod-aoe-loot
  origin_gains_commit mod-aoe-loot
  run bash "$DML" wow module update-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.repos | length')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].label')" = "mod-aoe-loot" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].branch')" = "master" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].dirty')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].behind')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].head')" = "$(git -C "$SDIR/modules/mod-aoe-loot" rev-parse --short HEAD)" ]
}

@test "module update-check: unreachable origin -> behind:null, still ok" {
  make_module_repo mod-aoe-loot
  rm -rf "$FIXTURE/origin-mod-aoe-loot.git"
  run bash "$DML" wow module update-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.repos[0].label')" = "mod-aoe-loot" ]
  [ "$(echo "$output" | jq -r '.data.repos[0].behind')" = "null" ]
}

@test "module update-check: registry + custom clones both probed" {
  make_module_repo mod-aoe-loot
  make_module_repo mod-my-custom
  origin_gains_commit mod-my-custom
  run bash "$DML" wow module update-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.repos | map(.label) | sort | join(",")')" = "mod-aoe-loot,mod-my-custom" ]
  [ "$(echo "$output" | jq -r '.data.repos[] | select(.label=="mod-aoe-loot") | .behind')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.repos[] | select(.label=="mod-my-custom") | .behind')" = "1" ]
}

@test "module update-check: no git clones -> empty repos array (non-git dirs + lua clones excluded)" {
  mkdir -p "$SDIR/modules/mod-transmog"           # installed-looking dir, no .git
  mkdir -p "$SDIR/ale_scripts/accountwide/.git"   # lua clone lives outside modules/
  run bash "$DML" wow module update-check --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.repos | length')" = "0" ]
}

# --- module update ----------------------------------------------------------

@test "module update: pulls new origin commits -> changed:true + rebuild-pending marked" {
  make_module_repo mod-aoe-loot
  origin_gains_commit mod-aoe-loot
  before="$(git -C "$SDIR/modules/mod-aoe-loot" log -1 --format=%h)"
  run bash "$DML" wow module update --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  done_line="$(echo "$output" | grep '"event":"done"')"
  [ "$(echo "$done_line" | jq -r '.data.key')" = "mod-aoe-loot" ]
  [ "$(echo "$done_line" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$done_line" | jq -r '.data.pending_rebuild')" = "true" ]
  [ "$(echo "$done_line" | jq -r '.data.before')" = "$before" ]
  after="$(git -C "$SDIR/modules/mod-aoe-loot" log -1 --format=%h)"
  [ "$(echo "$done_line" | jq -r '.data.after')" = "$after" ]
  [ "$before" != "$after" ]
  grep -qxF 'mod-aoe-loot' "$SDIR/.dml-rebuild-pending"
}

@test "module update: already up to date -> changed:false, NOT marked pending" {
  make_module_repo mod-aoe-loot
  run bash "$DML" wow module update --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  done_line="$(echo "$output" | grep '"event":"done"')"
  [ "$(echo "$done_line" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$done_line" | jq -r '.data.pending_rebuild')" = "false" ]
  [ "$(echo "$done_line" | jq -r '.data.before')" = "$(echo "$done_line" | jq -r '.data.after')" ]
  [ ! -f "$SDIR/.dml-rebuild-pending" ]
}

@test "module update: dirty worktree -> patch backup + local edit re-applied on top" {
  make_module_repo mod-aoe-loot
  origin_gains_commit mod-aoe-loot
  echo local-edit >> "$SDIR/modules/mod-aoe-loot/b.txt"
  run bash "$DML" wow module update --key mod-aoe-loot --json
  [ "$status" -eq 0 ]
  patch="$(ls "$SDIR/modules/mod-aoe-loot"/local-changes-*.patch 2>/dev/null | head -1)"
  [ -n "$patch" ]
  [ -s "$patch" ]
  grep -q 'local-edit' "$patch"
  grep -q 'local-edit' "$SDIR/modules/mod-aoe-loot/b.txt"   # stash popped
  grep -q 'two' "$SDIR/modules/mod-aoe-loot/a.txt"          # update landed
  echo "$output" | grep -q '"changed":true'
}

@test "module update: mod-playerbots refused with the Server-update hint, nothing pulled" {
  make_module_repo mod-playerbots
  before="$(git -C "$SDIR/modules/mod-playerbots" log -1 --format=%h)"
  origin_gains_commit mod-playerbots
  run bash "$DML" wow module update --key mod-playerbots --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  echo "$output" | grep -q 'use Server update on the Modules page'
  [ "$(git -C "$SDIR/modules/mod-playerbots" log -1 --format=%h)" = "$before" ]
  [ ! -f "$SDIR/.dml-rebuild-pending" ]
}

@test "module update: unknown key -> NOT_FOUND; malformed key -> BAD_ARG" {
  run bash "$DML" wow module update --key mod-nope --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
  run bash "$DML" wow module update --key '../evil' --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}

@test "module update: dir without .git -> GIT_MISSING" {
  mkdir -p "$SDIR/modules/mod-transmog"
  run bash "$DML" wow module update --key mod-transmog --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'GIT_MISSING'
}

@test "module update: clone without an origin remote -> REMOTE_MISSING" {
  git init -q "$SDIR/modules/mod-aoe-loot"
  ( cd "$SDIR/modules/mod-aoe-loot" && echo x > f.txt \
    && git add f.txt && git commit -qm x )
  run bash "$DML" wow module update --key mod-aoe-loot --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'REMOTE_MISSING'
}

@test "module update-check / update: server not installed -> NOT_FOUND" {
  rm -rf "$SDIR"
  run bash "$DML" wow module update-check --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
  run bash "$DML" wow module update --key mod-aoe-loot --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}
