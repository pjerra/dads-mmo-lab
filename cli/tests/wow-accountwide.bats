#!/usr/bin/env bats
# Accountwide Systems configurator (overnight Batch 1): `wow accountwide get`
# reports installed-state + each present subsystem's on/off + the reputation
# pick-one block; `wow accountwide set` flips a single ENABLE_* flag in the
# DEPLOYED accountwide lua files. All edits go to a fixture copy under the
# stubbed games dir -- never a real server.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  export HOME="$FIXTURE"
  SDIR="$DML_GAMES_DIR/wow-server-playerbots"
  AWDIR="$SDIR/env/dist/etc/modules/lua_scripts/accountwide"
}
teardown() { teardown_fixture; }

# Seed a representative deployed accountwide payload (every flag ships false,
# column 0, the upstream shape). Reputation seeds BOTH variant files.
seed_accountwide() {
  mkdir -p "$AWDIR"
  cat > "$AWDIR/AccountAchievements.lua" <<'EOF'
-- Account-wide achievements config
local ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS = false
local ENABLE_ACCOUNTWIDE_CRITERIA_PROGRESS = false
local ENABLE_ACCOUNTWIDE_REALM_FIRST_ACHIEVEMENTS = false
-- END CONFIG
EOF
  cat > "$AWDIR/AccountCurrency.lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_CURRENCY = false
EOF
  cat > "$AWDIR/AccountMoney.lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_MONEY = false
local ENABLE_REALTIME_TICK = false
local ENABLE_ALTBOT_REALTIME_TICK = false
EOF
  cat > "$AWDIR/AccountMounts.lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_MOUNTS = false -- share learned mounts across alts
EOF
  cat > "$AWDIR/AccountPets.lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_PETS = false
EOF
  cat > "$AWDIR/AccountPlaytime.lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_PLAYTIME = false
EOF
  cat > "$AWDIR/AccountProfessions.lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_PROFESSIONS = false
EOF
  cat > "$AWDIR/AccountPvPRank.lua" <<'EOF'
local RUN_INIT_SEED_ON_STARTUP = true
local ENABLE_ACCOUNTWIDE_PVP_RANK = false
EOF
  cat > "$AWDIR/AccountTitles.lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_TITLES = false
EOF
  cat > "$AWDIR/AccountReputation (default AC-Wotlk).lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_REPUTATION = false
EOF
  cat > "$AWDIR/AccountReputation (modified for Ashen Order).lua" <<'EOF'
local ENABLE_ACCOUNTWIDE_REPUTATION = false
EOF
}

@test "accountwide get: not installed -> installed:false, empty lists (not an error)" {
  run bash "$DML" wow accountwide get --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.installed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.subsystems | length')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.reputation.present')" = "false" ]
}

@test "accountwide get: server not installed at all -> NOT_FOUND" {
  rm -rf "$SDIR"
  run bash "$DML" wow accountwide get --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "accountwide get: lists present subsystems, all off by default" {
  seed_accountwide
  run bash "$DML" wow accountwide get --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.installed')" = "true" ]
  # every seeded flag is off
  [ "$(echo "$output" | jq -r '[.data.subsystems[] | select(.value != "off")] | length')" = "0" ]
  # mounts is present with its label/explain
  [ "$(echo "$output" | jq -r '.data.subsystems[] | select(.key=="ENABLE_ACCOUNTWIDE_MOUNTS") | .value')" = "off" ]
  [ "$(echo "$output" | jq -r '.data.subsystems[] | select(.key=="ENABLE_ACCOUNTWIDE_MOUNTS") | .file')" = "AccountMounts.lua" ]
  # nested criteria toggle carries its parent
  [ "$(echo "$output" | jq -r '.data.subsystems[] | select(.key=="ENABLE_ACCOUNTWIDE_CRITERIA_PROGRESS") | .parent')" = "ENABLE_ACCOUNTWIDE_COMPLETED_ACHIEVEMENTS" ]
  # reputation: both variants present, currently off
  [ "$(echo "$output" | jq -r '.data.reputation.present')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.reputation.value')" = "off" ]
  [ "$(echo "$output" | jq -r '.data.reputation.variants | sort | join(",")')" = "custom,default" ]
  [ "$(echo "$output" | jq -r '.data.reputation.active')" = "null" ]
}

@test "accountwide get: a subsystem whose file is absent is skipped" {
  seed_accountwide
  rm -f "$AWDIR/AccountMounts.lua"
  run bash "$DML" wow accountwide get --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '[.data.subsystems[] | select(.key=="ENABLE_ACCOUNTWIDE_MOUNTS")] | length')" = "0" ]
}

@test "accountwide set: enable a flag round-trips through get" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_MOUNTS --value on --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  grep -q '^local ENABLE_ACCOUNTWIDE_MOUNTS = true' "$AWDIR/AccountMounts.lua"
  # trailing comment preserved on the flipped line
  grep -q 'share learned mounts across alts' "$AWDIR/AccountMounts.lua"
  run bash "$DML" wow accountwide get --json
  [ "$(echo "$output" | jq -r '.data.subsystems[] | select(.key=="ENABLE_ACCOUNTWIDE_MOUNTS") | .value')" = "on" ]
}

@test "accountwide set: disable round-trip" {
  seed_accountwide
  bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_TITLES --value on --json
  grep -q '^local ENABLE_ACCOUNTWIDE_TITLES = true' "$AWDIR/AccountTitles.lua"
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_TITLES --value off --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.value')" = "off" ]
  grep -q '^local ENABLE_ACCOUNTWIDE_TITLES = false' "$AWDIR/AccountTitles.lua"
}

@test "accountwide set: already at target value -> changed:false, still ok" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_PETS --value off --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
}

@test "accountwide set: nested money sub-toggle writes only its own flag" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key ENABLE_REALTIME_TICK --value on --json
  [ "$status" -eq 0 ]
  grep -q '^local ENABLE_REALTIME_TICK = true' "$AWDIR/AccountMoney.lua"
  # the parent + sibling in the same file are untouched
  grep -q '^local ENABLE_ACCOUNTWIDE_MONEY = false' "$AWDIR/AccountMoney.lua"
  grep -q '^local ENABLE_ALTBOT_REALTIME_TICK = false' "$AWDIR/AccountMoney.lua"
}

@test "accountwide set: unknown (well-formed) flag -> BAD_ARG" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_BOGUS --value on --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}

@test "accountwide set: malformed flag name -> BAD_ARG" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key "mounts; rm -rf /" --value on --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}

@test "accountwide set: bad --value -> BAD_ARG" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_MOUNTS --value maybe --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}

@test "accountwide set: known flag whose line is absent from the file -> NOT_FOUND" {
  seed_accountwide
  # professions registry row exists, but strip its flag line from the file
  printf -- '-- professions (no flag here)\n' > "$AWDIR/AccountProfessions.lua"
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_PROFESSIONS --value on --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
}

@test "accountwide set: not installed -> NOT_INSTALLED" {
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_MOUNTS --value on --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_INSTALLED'
}

@test "accountwide read: a commented-out flag line is NOT treated as present" {
  mkdir -p "$AWDIR"
  # only Mounts, and it is commented out -> should be skipped by get
  printf -- '-- local ENABLE_ACCOUNTWIDE_MOUNTS = false\n' > "$AWDIR/AccountMounts.lua"
  run bash "$DML" wow accountwide get --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '[.data.subsystems[] | select(.key=="ENABLE_ACCOUNTWIDE_MOUNTS")] | length')" = "0" ]
}

@test "accountwide reputation: enable with --variant default deletes the custom file + enables flag" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_REPUTATION --value on --variant default --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.value')" = "on" ]
  [ -f "$AWDIR/AccountReputation (default AC-Wotlk).lua" ]
  [ ! -f "$AWDIR/AccountReputation (modified for Ashen Order).lua" ]
  grep -q '^local ENABLE_ACCOUNTWIDE_REPUTATION = true' "$AWDIR/AccountReputation (default AC-Wotlk).lua"
  # get now reports the default variant active
  run bash "$DML" wow accountwide get --json
  [ "$(echo "$output" | jq -r '.data.reputation.value')" = "on" ]
  [ "$(echo "$output" | jq -r '.data.reputation.active')" = "default" ]
}

@test "accountwide reputation: enable with two variants and no --variant -> BAD_ARG" {
  seed_accountwide
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_REPUTATION --value on --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  # nothing deleted or flipped
  [ -f "$AWDIR/AccountReputation (default AC-Wotlk).lua" ]
  [ -f "$AWDIR/AccountReputation (modified for Ashen Order).lua" ]
}

@test "accountwide reputation: single deployed variant enables without --variant" {
  seed_accountwide
  rm -f "$AWDIR/AccountReputation (modified for Ashen Order).lua"
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_REPUTATION --value on --json
  [ "$status" -eq 0 ]
  grep -q '^local ENABLE_ACCOUNTWIDE_REPUTATION = true' "$AWDIR/AccountReputation (default AC-Wotlk).lua"
}

@test "accountwide reputation: disable clears the flag in every present variant" {
  seed_accountwide
  # turn the default variant on first
  bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_REPUTATION --value on --variant custom --json
  run bash "$DML" wow accountwide set --key ENABLE_ACCOUNTWIDE_REPUTATION --value off --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.value')" = "off" ]
  grep -q '^local ENABLE_ACCOUNTWIDE_REPUTATION = false' "$AWDIR/AccountReputation (modified for Ashen Order).lua"
}
