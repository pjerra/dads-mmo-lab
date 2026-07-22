#!/usr/bin/env bats
# Module-tuning rework: `wow config conf-keys --file <name>` (pb-keys
# generalized to any module conf passing _cfg_file_path's dynamic allowlist,
# plus per-key comment-block help from the .dist) and the generalized direct
# write route `config set --key conf:<file>.conf:<Key>` with mod-transmog's
# verified `transmog reload` live-apply. Every edit lands in a fixture copy
# under the stubbed games dir -- never a real server.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  OVR="$GDIR/docker-compose.override.yml"
  MODS="$GDIR/env/dist/etc/modules"
  mkdir -p "$MODS"
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

# mod-transmog style: ONE shared doc block documents many keys via
# `#    Key.Name` header lines; the keys follow in a run far below.
_seed_transmog_dist() {
  cat > "$MODS/transmog.conf.dist" <<'EOF'
[worldserver]

########################################
#    Transmogrification config
########################################
#
#    SETTINGS
#
#    Transmogrification.Enable
#        Description: Enables/Disables transmog.
#        Default:     1
#
#    Transmogrification.EnableTransmogInfo
#        Description: Enables/Disables the transmog info gossip page.
#        Default:     1
#

Transmogrification.Enable = 1
Transmogrification.EnableTransmogInfo = 1
Transmogrification.MembershipLevels = ""
EOF
}

# mod-learn-spells style: an adjacent # block with a BLANK line between the
# block and its key; plus a key with no comment anywhere near it.
_seed_learnspells_conf() {
  cat > "$MODS/mod_learnspells.conf" <<'EOF'
[worldserver]

########################################
#	Learn spells on level-up
########################################

# Enable the module? (1: true | 0: false)

LearnSpells.Enable = 1

# Max level Limit the player will learn spells
# 	Default:  = 80

LearnSpells.MaxLevel = 80
LearnSpells.MaxLevel = 75
Bare.NoComment = 5
EOF
}

# --- conf-keys: parsing ------------------------------------------------------

@test "conf-keys parses keys/values, quotes preserved, duplicate key last-wins" {
  _seed_learnspells_conf
  run bash "$DML" wow config conf-keys --file mod_learnspells.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.file')" = "mod_learnspells.conf" ]
  [ "$(echo "$output" | jq -r '.data.source')" = "conf" ]
  [ "$(echo "$output" | jq -r '.data.keys | length')" = "3" ]
  # duplicate: LAST value wins, line points at the winning line
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="LearnSpells.MaxLevel") | .value')" = "75" ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="LearnSpells.MaxLevel") | .line')" = "15" ]
  # no dist -> defaults are null
  [ "$(echo "$output" | jq -r '.data.keys[0].default')" = "null" ]
}

@test "conf-keys enriches defaults from the .dist when both files exist" {
  _seed_transmog_dist
  cp "$MODS/transmog.conf.dist" "$MODS/transmog.conf"
  # live conf diverges from the dist default
  sed -i 's/^Transmogrification.Enable = 1$/Transmogrification.Enable = 0/' "$MODS/transmog.conf"
  run bash "$DML" wow config conf-keys --file transmog.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "conf" ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.Enable") | .value')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.Enable") | .default')" = "1" ]
  # quoted value round-trips raw
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.MembershipLevels") | .value')" = '""' ]
}

@test "conf-keys falls back to the .dist (source:dist) when the conf does not exist" {
  _seed_transmog_dist
  run bash "$DML" wow config conf-keys --file transmog.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.source')" = "dist" ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.Enable") | .value')" = "1" ]
  # dist-as-source: its own value doubles as the default
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.Enable") | .default')" = "1" ]
}

# --- conf-keys: comment-block help -------------------------------------------

@test "conf-keys help: adjacent block with a blank line before the key (multi-line)" {
  _seed_learnspells_conf
  run bash "$DML" wow config conf-keys --file mod_learnspells.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="LearnSpells.Enable") | .help')" = "Enable the module? (1: true | 0: false)" ]
  # multi-line block collapses to one spaced line
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="LearnSpells.MaxLevel") | .help')" = "Max level Limit the player will learn spells Default: = 80" ]
}

@test "conf-keys help: no comment anywhere near the key -> empty help" {
  _seed_learnspells_conf
  run bash "$DML" wow config conf-keys --file mod_learnspells.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Bare.NoComment") | .help')" = "" ]
}

@test "conf-keys help: per-key slices out of a shared doc block (transmog style)" {
  _seed_transmog_dist
  run bash "$DML" wow config conf-keys --file transmog.conf --json
  [ "$status" -eq 0 ]
  h1="$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.Enable") | .help')"
  h2="$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.EnableTransmogInfo") | .help')"
  [ "$h1" = "Description: Enables/Disables transmog. Default: 1" ]
  [ "$h2" = "Description: Enables/Disables the transmog info gossip page. Default: 1" ]
  # keys the block does not document stay empty
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.MembershipLevels") | .help')" = "" ]
}

@test "conf-keys help: the .dist docs win even when the live conf lost its comments" {
  _seed_transmog_dist
  printf 'Transmogrification.Enable = 0\n' > "$MODS/transmog.conf"
  run bash "$DML" wow config conf-keys --file transmog.conf --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.Enable") | .value')" = "0" ]
  [ "$(echo "$output" | jq -r '.data.keys[] | select(.key=="Transmogrification.Enable") | .help')" = "Description: Enables/Disables transmog. Default: 1" ]
}

@test "conf-keys help is capped at 400 characters" {
  {
    printf '# start '
    for i in $(seq 1 60); do printf 'padpadpad%03d ' "$i"; done
    printf '\nBig.Key = 1\n'
  } > "$MODS/mod_big.conf"
  run bash "$DML" wow config conf-keys --file mod_big.conf --json
  [ "$status" -eq 0 ]
  hlen="$(echo "$output" | jq -r '.data.keys[] | select(.key=="Big.Key") | .help | length')"
  [ "$hlen" -eq 400 ]
}

# --- conf-keys: allowlist ----------------------------------------------------

@test "conf-keys rejects core/non-conf names and unknown confs with clean envelopes" {
  run bash "$DML" wow config conf-keys --file .env --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config conf-keys --file docker-compose.override.yml --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config conf-keys --file worldserver.conf --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config conf-keys --file mod_nope.conf --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  run bash "$DML" wow config conf-keys --file '../evil.conf' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  run bash "$DML" wow config conf-keys --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

# --- generalized direct write route -----------------------------------------

@test "direct conf route writes any allowlisted module conf and creates it from its .dist" {
  _seed_transmog_dist
  use_curl_stub
  export DML_STUB_CURL_EXIT=7   # SOAP down -> the transmog write may not claim live
  run bash "$DML" wow config set --key conf:transmog.conf:Transmogrification.EnableTransmogInfo --value 0 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ -f "$MODS/transmog.conf" ]
  grep -q '^Transmogrification.EnableTransmogInfo = 0$' "$MODS/transmog.conf"
  # comments came along from the dist
  grep -q '^#    Transmogrification config$' "$MODS/transmog.conf"
  # same value again is a no-op
  run bash "$DML" wow config set --key conf:transmog.conf:Transmogrification.EnableTransmogInfo --value 0 --json
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "none" ]
}

@test "direct conf route still rejects non-allowlisted and core conf names" {
  run bash "$DML" wow config set --key conf:mod_nope.conf:Some.Key --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  run bash "$DML" wow config set --key conf:authserver.conf:LoginDatabaseInfo --value x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "transmog live-reload: SOAP up + no frozen env -> applied live via 'transmog reload'" {
  _seed_transmog_dist
  use_docker_stub   # no DML_STUB_CONTAINER_ENV -> nothing frozen
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/captured.xml"
  run bash "$DML" wow config set --key conf:transmog.conf:Transmogrification.Enable --value 0 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "live" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
  captured="$(cat "$DML_STUB_CAPTURE")"
  cmd="${captured#*<command>}"; cmd="${cmd%%</command>*}"
  [ "$cmd" = "transmog reload" ]
}

@test "transmog live-reload falls back to restart when SOAP is unreachable" {
  _seed_transmog_dist
  use_docker_stub
  use_curl_stub
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow config set --key conf:transmog.conf:Transmogrification.Enable --value 0 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
}

@test "transmog live-reload reports restart while a legacy env is frozen in the container" {
  _seed_transmog_dist
  use_docker_stub
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CONTAINER_ENV='PATH=/usr/bin
AC_TRANSMOGRIFICATION_ENABLE=1'
  run bash "$DML" wow config set --key conf:transmog.conf:Transmogrification.Enable --value 0 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
}

@test "direct module-conf save cleans the matching legacy env override and reports restart" {
  _seed_transmog_dist
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_TRANSMOGRIFICATION_ENABLE: "1"
      AC_RATE_XP_KILL: "3"
EOF
  run bash "$DML" wow config set --key conf:transmog.conf:Transmogrification.Enable --value 0 --json
  [ "$status" -eq 0 ]
  # env was still in the override -> the running container is frozen with it
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  run yq -e '.services.ac-worldserver.environment | has("AC_TRANSMOGRIFICATION_ENABLE")' "$OVR"
  [ "$status" -ne 0 ]
  yq -e '.services.ac-worldserver.environment.AC_RATE_XP_KILL == "3"' "$OVR"
}

@test "playerbots direct writes stay restart-to-apply (no invented reload command)" {
  printf 'AiPlayerbot.MaxRandomBots = 500\n' > "$MODS/playerbots.conf"
  use_docker_stub
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow config set --key conf:playerbots.conf:AiPlayerbot.EnableGreet --value 1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.applied')" = "restart" ]
  # no SOAP call was even attempted
  [ ! -f "$DML_STUB_CURL_LOG" ]
}

@test "module list rows carry conf_name (null when the module has none)" {
  mkdir -p "$GDIR/modules/mod-transmog/.git"
  run bash "$DML" wow module list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-transmog") | .conf_name')" = "transmog.conf" ]
  [ "$(echo "$output" | jq -r '.data.families.cpp[] | select(.key=="mod-junk-to-gold") | .conf_name')" = "null" ]
}

@test "tuning-list rows carry their backing file name" {
  run bash "$DML" wow config tuning-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="beastmaster.enable") | .file')" = "mod_npc_beastmaster.conf" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="unlimitedammo.enabled") | .file')" = "UnlimitedAmmo.lua" ]
}
