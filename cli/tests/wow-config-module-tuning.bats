#!/usr/bin/env bats
# Guided module tuning (overnight Batch 3): `wow config tuning-list` reports
# the curated activator knobs for a few optional modules + whether each is
# deployed; `wow config tuning-set` writes the value with the right backend --
# the conf-row mechanism for .conf-based modules (NPC Beastmaster, Learn
# Spells) and a comment/format-preserving line-replace of the deployed ALE
# script for the .lua ones (Unlimited Ammo, Sit Means Rest). Every edit lands
# in a fixture copy under the stubbed games dir -- never a real server.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  MOD="$SDIR/env/dist/etc/modules"
  LUA="$MOD/lua_scripts"
  mkdir -p "$LUA"
}
teardown() { teardown_fixture; }

# Deployed .conf for both cpp module tuning targets (upstream .dist shape).
seed_confs() {
  cat > "$MOD/mod_npc_beastmaster.conf" <<'EOF'
# Beastmaster config -- comment must survive edits
BeastMaster.Enable = 1
BeastMaster.HunterOnly = 1
BeastMaster.AllowedClasses = 0
BeastMaster.MinLevel = 10
EOF
  cat > "$MOD/mod_learnspells.conf" <<'EOF'
LearnSpells.Enable = 1
LearnSpells.Announce = 1
LearnSpells.OnFirstLogin = 0
LearnSpells.MaxLevel = 80
EOF
}

# Deployed ALE scripts, verbatim upstream format: UnlimitedAmmo is column-0
# namespaced with an inline comment; SitMeansRest keys live indented inside a
# `local CONFIG = { ... }` table with a trailing comma + inline comment.
seed_luas() {
  cat > "$LUA/UnlimitedAmmo.lua" <<'EOF'
UnlimitedAmmoNamespace = {}
UnlimitedAmmoNamespace.ENABLED = false -- Set this to false to disable the script
UnlimitedAmmoNamespace.MAX_AMMO = 1000 -- Maximum ammunition allowed
UnlimitedAmmoNamespace.MIN_AMMO_THRESHOLD = 52 -- Ammo count threshold to add max ammo
EOF
  cat > "$LUA/SitMeansRest.lua" <<'EOF'
local CONFIG = {
    DURATION = 20,          -- Seconds to rest
    CHECK_INTERVAL = 500,   -- Check for movement every 500ms
    REGEN_AURA = 25990,     -- Graccus Mistletoe (Fruitcake effect)
    EVENT_ID = 99100,
    SIT_EMOTE_ID = 86,      -- TEXT_EMOTE_SIT
}
EOF
}

# --- listing ---------------------------------------------------------------

@test "tuning-list: server not installed -> NOT_FOUND" {
  rm -rf "$SDIR"
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "tuning-list: nothing deployed -> every row present at its default, installed:false" {
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "13" ]
  [ "$(echo "$output" | jq -r '[.data.settings[] | select(.installed != false)] | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .value')" = "10" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="unlimitedammo.enabled") | .value')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="sitmeansrest.regen_aura") | .value')" = "25990" ]
}

@test "tuning-list: reads deployed conf values and marks conf rows installed" {
  seed_confs
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.enable") | .installed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="learnspells.max_level") | .value')" = "80" ]
  # lua rows are still not deployed
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="unlimitedammo.enabled") | .installed')" = "false" ]
}

@test "tuning-list: reads a conf value from its .dist when only the dist exists" {
  cat > "$MOD/mod_npc_beastmaster.conf.dist" <<'EOF'
BeastMaster.MinLevel = 15
EOF
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .value')" = "15" ]
  # still not "installed" -- only the dist is present, not the live conf
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .installed')" = "false" ]
}

@test "tuning-list: translates lua booleans + reads table ints, marks lua rows installed" {
  seed_luas
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="unlimitedammo.enabled") | .value')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="unlimitedammo.enabled") | .installed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="unlimitedammo.max_ammo") | .value')" = "1000" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="sitmeansrest.duration") | .value')" = "20" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="sitmeansrest.regen_aura") | .value')" = "25990" ]
}

# --- conf backend round-trips ---------------------------------------------

@test "tuning-set (conf): int round-trips, comment preserved, list reflects it" {
  seed_confs
  run bash "$DML" wow config tuning-set --key beastmaster.min_level --value 25 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.backend')" = "conf" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  grep -q '^BeastMaster.MinLevel = 25$' "$MOD/mod_npc_beastmaster.conf"
  grep -q '^# Beastmaster config -- comment must survive edits$' "$MOD/mod_npc_beastmaster.conf"
  run bash "$DML" wow config tuning-list --json
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.min_level") | .value')" = "25" ]
}

@test "tuning-set (conf): bool round-trips" {
  seed_confs
  run bash "$DML" wow config tuning-set --key learnspells.on_first_login --value 1 --json
  [ "$status" -eq 0 ]
  grep -q '^LearnSpells.OnFirstLogin = 1$' "$MOD/mod_learnspells.conf"
}

@test "tuning-set (conf): list value (comma-separated class ids) round-trips" {
  seed_confs
  run bash "$DML" wow config tuning-set --key beastmaster.allowed_classes --value 3,8 --json
  [ "$status" -eq 0 ]
  grep -q '^BeastMaster.AllowedClasses = 3,8$' "$MOD/mod_npc_beastmaster.conf"
}

@test "tuning-set (conf): creates the conf from its .dist on first write" {
  cat > "$MOD/mod_npc_beastmaster.conf.dist" <<'EOF'
# dist header
BeastMaster.MinLevel = 10
EOF
  run bash "$DML" wow config tuning-set --key beastmaster.min_level --value 30 --json
  [ "$status" -eq 0 ]
  [ -f "$MOD/mod_npc_beastmaster.conf" ]
  grep -q '^# dist header$' "$MOD/mod_npc_beastmaster.conf"
  grep -q '^BeastMaster.MinLevel = 30$' "$MOD/mod_npc_beastmaster.conf"
}

@test "tuning-set (conf): same value again is a no-op (changed:false, applied:none)" {
  seed_confs
  run bash "$DML" wow config tuning-set --key beastmaster.hunter_only --value 1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "none" ]
}

@test "tuning-set (conf): module not installed (no conf, no dist) -> NOT_INSTALLED" {
  run bash "$DML" wow config tuning-set --key beastmaster.enable --value 0 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_INSTALLED" ]
}

# --- lua backend round-trips ----------------------------------------------

@test "tuning-set (lua): flips ENABLED false->true, preserves inline comment" {
  seed_luas
  run bash "$DML" wow config tuning-set --key unlimitedammo.enabled --value 1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.backend')" = "lua" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "reload-ale" ]
  grep -q '^UnlimitedAmmoNamespace.ENABLED = true -- Set this to false to disable the script$' "$LUA/UnlimitedAmmo.lua"
}

@test "tuning-set (lua): int round-trips on a namespaced key" {
  seed_luas
  run bash "$DML" wow config tuning-set --key unlimitedammo.max_ammo --value 500 --json
  [ "$status" -eq 0 ]
  grep -q '^UnlimitedAmmoNamespace.MAX_AMMO = 500 -- Maximum ammunition allowed$' "$LUA/UnlimitedAmmo.lua"
  # the other keys in the file are untouched
  grep -q '^UnlimitedAmmoNamespace.ENABLED = false' "$LUA/UnlimitedAmmo.lua"
  grep -q '^UnlimitedAmmoNamespace.MIN_AMMO_THRESHOLD = 52' "$LUA/UnlimitedAmmo.lua"
}

@test "tuning-set (lua): indented table key keeps its indent, comma and comment" {
  seed_luas
  run bash "$DML" wow config tuning-set --key sitmeansrest.duration --value 45 --json
  [ "$status" -eq 0 ]
  grep -q '^    DURATION = 45,          -- Seconds to rest$' "$LUA/SitMeansRest.lua"
  # a sibling table key sharing the file is untouched
  grep -q '^    REGEN_AURA = 25990,     -- Graccus Mistletoe (Fruitcake effect)$' "$LUA/SitMeansRest.lua"
}

@test "tuning-set (lua): second table key round-trips through the list" {
  seed_luas
  run bash "$DML" wow config tuning-set --key sitmeansrest.regen_aura --value 21562 --json
  [ "$status" -eq 0 ]
  grep -q '^    REGEN_AURA = 21562,' "$LUA/SitMeansRest.lua"
  run bash "$DML" wow config tuning-list --json
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="sitmeansrest.regen_aura") | .value')" = "21562" ]
}

@test "tuning-set (lua): same value again is a no-op (changed:false)" {
  seed_luas
  run bash "$DML" wow config tuning-set --key sitmeansrest.duration --value 20 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "none" ]
}

@test "tuning-set (lua): script not deployed -> NOT_INSTALLED" {
  run bash "$DML" wow config tuning-set --key unlimitedammo.enabled --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_INSTALLED" ]
}

@test "tuning-set (lua): known key whose line is absent from the deployed file -> NOT_FOUND" {
  mkdir -p "$LUA"
  printf -- '-- UnlimitedAmmo (no config here)\n' > "$LUA/UnlimitedAmmo.lua"
  run bash "$DML" wow config tuning-set --key unlimitedammo.max_ammo --value 500 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "tuning read: a commented-out lua key is NOT treated as present" {
  mkdir -p "$LUA"
  printf -- '-- UnlimitedAmmoNamespace.ENABLED = true\n' > "$LUA/UnlimitedAmmo.lua"
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  # value falls back to the registry default (0), not the commented true
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="unlimitedammo.enabled") | .value')" = "0" ]
}

# --- validation ------------------------------------------------------------

@test "tuning-set: unknown key -> NOT_FOUND" {
  seed_confs
  run bash "$DML" wow config tuning-set --key beastmaster.bogus --value 1 --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "tuning-set: bool rejects a non 0/1 value -> BAD_ARG" {
  seed_confs
  run bash "$DML" wow config tuning-set --key beastmaster.enable --value 2 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "tuning-set: int out of range -> BAD_ARG" {
  seed_confs
  run bash "$DML" wow config tuning-set --key beastmaster.min_level --value 999 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "tuning-set: list rejects injection-shaped value -> BAD_ARG (nothing written)" {
  seed_confs
  run bash "$DML" wow config tuning-set --key beastmaster.allowed_classes --value '3; rm -rf /' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  grep -q '^BeastMaster.AllowedClasses = 0$' "$MOD/mod_npc_beastmaster.conf"
}

@test "tuning-set: missing --key -> BAD_ARG" {
  run bash "$DML" wow config tuning-set --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
