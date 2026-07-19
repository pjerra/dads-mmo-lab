#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  export HOME="$FIXTURE"
  export DML_INSTALLERS_DIR="$FIXTURE/installers"
  mkdir -p "$DML_INSTALLERS_DIR"
}
teardown() { teardown_fixture; }

fake_installer() {  # fake_installer <script-name> <server-dir-to-create>
  cat > "$DML_INSTALLERS_DIR/$1" <<EOF
#!/usr/bin/env bash
read -r answer
echo "you said: \$answer"
echo "\$answer" > "$FIXTURE/got-stdin"
mkdir -p "$2"
exit 0
EOF
}

@test "games catalog: registry rows with installed/script_available" {
  mkdir -p "$FIXTURE/maplestory-server"
  fake_installer install-maplestory.sh "$FIXTURE/maplestory-server"
  run bash "$DML" games catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.titles | length')" = "6" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="maplestory-server") | .installed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="maplestory-server") | .script_available')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .installed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .script_available')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .running')" = "null" ]
}

@test "games install: --json rejected, unknown id rejected, EXISTS when installed" {
  run bash "$DML" games install maplestory-server --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
  run bash "$DML" games install not-a-title
  [ "$status" -eq 1 ]
  mkdir -p "$FIXTURE/maplestory-server"
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 1 ]
  echo "$output" | grep -qi 'already installed'
}

@test "games install: missing script -> NO_SCRIPT-style error" {
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 1 ]
  echo "$output" | grep -qi 'installer script'
}

@test "games install: runs the script with stdin passthrough, symlinks home-kind" {
  fake_installer install-maplestory.sh "$FIXTURE/maplestory-server"
  run bash -c 'echo hello | bash "'"$DML"'" games install maplestory-server'
  [ "$status" -eq 0 ]
  [ "$(cat "$FIXTURE/got-stdin")" = "hello" ]
  echo "$output" | grep -q 'you said: hello'
  [ -L "$FIXTURE/games/maplestory-server" ]
  [ "$(readlink "$FIXTURE/games/maplestory-server")" = "$FIXTURE/maplestory-server" ]
}

@test "games install: installer failure -> exit code passes through, no symlink" {
  cat > "$DML_INSTALLERS_DIR/install-maplestory.sh" <<'EOF'
#!/usr/bin/env bash
echo boom
exit 7
EOF
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 7 ]
  [ ! -e "$FIXTURE/games/maplestory-server" ]
}

@test "games install: declined install (exit 0, no dir) -> no phantom symlink" {
  cat > "$DML_INSTALLERS_DIR/install-maplestory.sh" <<'EOF'
#!/usr/bin/env bash
echo "aborted by user"
exit 0
EOF
  run bash "$DML" games install maplestory-server
  [ "$status" -eq 0 ]
  [ ! -e "$FIXTURE/games/maplestory-server" ]
}

@test "games remove: unknown id / not installed / no --yes" {
  run bash "$DML" games remove not-a-title --json
  [ "$status" -eq 1 ]
  run bash "$DML" games remove maplestory-server --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'NOT_FOUND'
  mkdir -p "$FIXTURE/maplestory-server"
  run bash "$DML" games remove maplestory-server --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'CONFIRM_REQUIRED'
  echo "$output" | grep -q 'maplestory-server'
  [ -d "$FIXTURE/maplestory-server" ]
}

@test "games remove --yes: deletes dir + symlink + launcher file, keeps ~/.dml" {
  mkdir -p "$FIXTURE/maplestory-server" "$FIXTURE/games" "$FIXTURE/.dml/backups"
  touch "$FIXTURE/maplestory-server/docker-compose.yml"
  ln -s "$FIXTURE/maplestory-server" "$FIXTURE/games/maplestory-server"
  touch "$FIXTURE/maplestory-launcher.sh" "$FIXTURE/.dml/backups/keepme"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"event":"done"'
  [ ! -e "$FIXTURE/maplestory-server" ]
  [ ! -e "$FIXTURE/games/maplestory-server" ]
  [ ! -e "$FIXTURE/maplestory-launcher.sh" ]
  [ -f "$FIXTURE/.dml/backups/keepme" ]
}

@test "games remove --yes: compose down attempted when compose exists" {
  mkdir -p "$FIXTURE/maplestory-server"
  touch "$FIXTURE/maplestory-server/docker-compose.yml"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -qi 'stopping'
}

# --- keep-data (Batch 3 F13c) ----------------------------------------------
# A compose file declaring the AzerothCore client-data volume gets that
# volume removed by default (compose down never removes named volumes) --
# unless --keep-data preserves it for a faster reinstall.

_setup_wow_removable() {  # creates an installed title whose compose declares ac-client-data
  mkdir -p "$FIXTURE/maplestory-server"
  cat > "$FIXTURE/maplestory-server/docker-compose.yml" <<'EOF'
services:
  ac-worldserver:
    image: x
volumes:
  ac-database:
  ac-client-data:
EOF
}

@test "games remove --yes: client-data volume rm runs, project-prefixed" {
  _setup_wow_removable
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  grep -q '^volume rm maplestory-server_ac-client-data$' "$FIXTURE/calls.log"
  echo "$output" | grep -q 'removed game data volume'
}

@test "games remove --yes --keep-data: volume rm absent, keep note emitted" {
  _setup_wow_removable
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --keep-data --json
  [ "$status" -eq 0 ]
  run grep 'volume rm' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "games remove --yes --keep-data: keep note names the volume, dir still deleted" {
  _setup_wow_removable
  run bash "$DML" games remove maplestory-server --yes --keep-data --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'keeping the downloaded game data volume (maplestory-server_ac-client-data'
  echo "$output" | grep -q '"event":"done"'
  [ ! -e "$FIXTURE/maplestory-server" ]
}

# DOCKER_VOL_DATA substitutes the service MOUNT source only; the top-level
# `volumes:` key stays `ac-client-data`, so that is the volume compose
# actually creates. Honoring the variable built a name docker never had --
# harmless-looking, but it meant the real ~6 GB volume silently leaked.
@test "games remove --yes: DOCKER_VOL_DATA in .env does NOT rename the removed volume" {
  _setup_wow_removable
  printf 'DOCKER_VOL_DATA=my-custom-data\n' > "$FIXTURE/maplestory-server/.env"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  grep -q '^volume rm maplestory-server_ac-client-data$' "$FIXTURE/calls.log"
  run grep 'my-custom-data' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

# A bare-name override that happens to match another DECLARED volume was the
# dangerous case: the old code would have removed the accounts/characters
# database volume right after compose down freed it.
@test "games remove --yes: DOCKER_VOL_DATA=ac-database never removes the database volume" {
  _setup_wow_removable
  printf 'DOCKER_VOL_DATA=ac-database\n' > "$FIXTURE/maplestory-server/.env"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  grep -q '^volume rm maplestory-server_ac-client-data$' "$FIXTURE/calls.log"
  run grep '^volume rm maplestory-server_ac-database$' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "games remove --yes: no client-data volume declared -> no volume rm at all" {
  mkdir -p "$FIXTURE/maplestory-server"
  printf 'services:\n  app:\n    image: x\n' > "$FIXTURE/maplestory-server/docker-compose.yml"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  run grep 'volume rm' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "games remove --yes: failed volume rm is a warn, removal still completes" {
  _setup_wow_removable
  export DML_STUB_VOLUME_RM_EXIT=1
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'could not remove game data volume'
  echo "$output" | grep -q '"event":"done"'
  [ ! -e "$FIXTURE/maplestory-server" ]
}

@test "games remove: unknown flag -> BAD_ARG" {
  mkdir -p "$FIXTURE/maplestory-server"
  run bash "$DML" games remove maplestory-server --yes --nuke-it --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'BAD_ARG'
}
