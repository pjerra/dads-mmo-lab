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

@test "games catalog reports each title's family" {
  run bash "$DML" games catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-server-playerbots") | .family')" = "azerothcore" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="wow-vanilla-server") | .family')" = "cmangos" ]
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="maplestory-server") | .family')" = "other" ]
}

# --- _title_family (internal helper, no CLI verb) ---------------------------
# Deliberately NOT wired into games catalog: that arm reads the family straight
# out of the registry row's 6th field (90-main.sh), because it already does a
# positional `read` over the same line and calling a helper would re-split it.
# So this helper has no production caller and is not going to get one -- it is
# kept as the single readable place that answers "what family is <id>?" for
# future bash callers, and tested directly by sourcing the file, same pattern as
# wow-module-cpp.bats's _module_key_from_url/_rebuild_pending_add tests.
# (The earlier comment here promised a later task would wire it in. Task 3
# decided otherwise; the promise was left behind. Final review, 2026-08-06.)

@test "_title_family: known id prints its family" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/80-titles.sh"; _title_family wow-vanilla-server'
  [ "$status" -eq 0 ]
  [ "$output" = "cmangos" ]
}

@test "_title_family: unknown id fails with no output" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/80-titles.sh"; _title_family not-a-title'
  [ "$status" -eq 1 ]
  [ -z "$output" ]
}

# --- install_supported (host verdict) --------------------------------------
# script_available answers "is the installer FILE there?"; install_supported
# answers "could a script run on this host AT ALL?". They are separate fields
# because they need different words on screen: the launcher used to collapse
# both into one hint that blamed cli/dev-install.ps1, so a Windows-native user
# whose dev-install was perfectly fine was told to re-run it forever.
#
# bats runs inside a Linux distro, so DML_OSTYPE is the seam for the Windows
# branch (see _installers_supported, cli/src/80-titles.sh).

@test "games catalog: install_supported true on a Linux host, independent of script_available" {
  # No installer files shipped at all -- the HOST verdict must still be true,
  # or the UI can't tell "not shipped yet" from "can never work here".
  run bash "$DML" games catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.install_supported')" = "true" ]
  [ "$(echo "$output" | jq -r '[.data.titles[].script_available] | unique | .[]')" = "false" ]
}

@test "games catalog: install_supported false under Git Bash on Windows, script_available unchanged" {
  fake_installer install-maplestory.sh "$FIXTURE/maplestory-server"
  run env DML_OSTYPE=msys bash "$DML" games catalog --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.install_supported')" = "false" ]
  # script_available stays a pure file-existence answer: the host verdict must
  # not overwrite it, otherwise the two causes are indistinguishable again.
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="maplestory-server") | .script_available')" = "true" ]
}

@test "games catalog: cygwin/MSYS OSTYPE flavours are also unsupported hosts" {
  run env DML_OSTYPE=cygwin bash "$DML" games catalog --json
  [ "$(echo "$output" | jq -r '.data.install_supported')" = "false" ]
  run env DML_OSTYPE=MINGW64_NT-10.0 bash "$DML" games catalog --json
  [ "$(echo "$output" | jq -r '.data.install_supported')" = "false" ]
}

# CHARACTERIZATION, not evidence for install_supported: this pins pre-existing
# behaviour and passes with the host guard reverted (verified by revert during
# review). It is here so a future change cannot quietly re-hardcode the
# installers path -- the original half of this bug.
@test "games catalog: script_available follows DML_INSTALLERS_DIR, never a fixed path" {
  # The live blocker was a hardcoded Linux default that no Windows host has.
  # Pin the override seam so a future edit can't reintroduce a fixed location.
  other="$FIXTURE/elsewhere"
  mkdir -p "$other"
  run env DML_INSTALLERS_DIR="$other" bash "$DML" games catalog --json
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .script_available')" = "false" ]
  touch "$other/install-runescape.sh"
  run env DML_INSTALLERS_DIR="$other" bash "$DML" games catalog --json
  [ "$(echo "$output" | jq -r '.data.titles[] | select(.id=="runescape-server") | .script_available')" = "true" ]
}

@test "games install: unsupported host refuses BEFORE the file check and never blames dev-install" {
  # Script present + host unsupported: without the guard the installer runs
  # and exits 0, creating the server dir.
  fake_installer install-maplestory.sh "$FIXTURE/maplestory-server"
  run env DML_OSTYPE=msys bash "$DML" games install maplestory-server
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'WSL backend'
  ! echo "$output" | grep -q 'dev-install.ps1'
  [ ! -e "$FIXTURE/maplestory-server" ]
}

@test "games install: unsupported host wins over a MISSING installer file too" {
  # No script AND no supported host -- the honest reason is the host, not the
  # shipping step, so dev-install.ps1 must not be named here either.
  run env DML_OSTYPE=msys bash "$DML" games install maplestory-server
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'WSL backend'
  ! echo "$output" | grep -q 'dev-install.ps1'
}

@test "games install: the refusal never advertises a route the user cannot take" {
  # A native Route A engine EXISTS, and an earlier version of this refusal
  # advertised it ("Library -> Install", "dml-wow install-native"). Both were
  # dead ends for every real user: the launcher bundles no dml-wow binary, and
  # the Library Install button is disabled on exactly this host. Three
  # independent reviewers caught it on 2026-07-29.
  #
  # So this test pins the HONEST property: the refusal offers only the route
  # that works. When the launcher wiring lands and the binary ships, this test
  # flips DELIBERATELY -- and `launcher/src/lib/title-install.ts` must be
  # updated in the same change, because that file holds the sentence a Windows
  # user actually sees.
  #
  # HOME is pinned into the fixture because `_title_installed` also looks at
  # `$HOME/<id>`: without this, a developer who happens to have that directory
  # would get "already installed" and never reach the host check at all.
  mkdir -p "$FIXTURE/home"
  run env DML_OSTYPE=msys HOME="$FIXTURE/home" bash "$DML" games install wow-server-playerbots
  [ "$status" -eq 1 ]
  ! echo "$output" | grep -q 'install-native'
  ! echo "$output" | grep -q 'Library ->'
  # The one thing that does work, and never the step that was already fine.
  echo "$output" | grep -q 'WSL backend'
  echo "$output" | grep -q 'Switch Settings'
  ! echo "$output" | grep -q 'dev-install.ps1'
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

# --- remove-images (Batch 6 B) ---------------------------------------------
# --remove-images ALSO deletes the AzerothCore/MySQL docker images the
# title's compose referenced (~3-5 GB), with ${DOCKER_IMAGE_TAG:-master}
# resolved. Default KEEP (fast reinstall).

_setup_wow_images() {  # installed title whose compose names two server images
  mkdir -p "$FIXTURE/maplestory-server"
  cat > "$FIXTURE/maplestory-server/docker-compose.yml" <<'EOF'
services:
  ac-database:
    image: mysql:8.4
  ac-worldserver:
    image: acore/ac-wotlk-worldserver:${DOCKER_IMAGE_TAG:-master}
volumes:
  ac-client-data:
EOF
}

@test "games remove --yes --remove-images: image rm runs per resolved image" {
  _setup_wow_images
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --remove-images --json
  [ "$status" -eq 0 ]
  grep -q '^image rm mysql:8.4$' "$FIXTURE/calls.log"
  grep -q '^image rm acore/ac-wotlk-worldserver:master$' "$FIXTURE/calls.log"
  echo "$output" | grep -q 'removed server image mysql:8.4'
  [ ! -e "$FIXTURE/maplestory-server" ]
}

@test "games remove --yes (no --remove-images): no image rm, kept note emitted" {
  _setup_wow_images
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  run grep 'image rm' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "games remove --yes (no --remove-images): kept-images note shown for compose titles" {
  _setup_wow_images
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'kept the downloaded server images'
}

@test "games remove --remove-images: DOCKER_IMAGE_TAG in .env resolves the tag" {
  _setup_wow_images
  printf 'DOCKER_IMAGE_TAG=v1.2.3\n' > "$FIXTURE/maplestory-server/.env"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" games remove maplestory-server --yes --remove-images --json
  [ "$status" -eq 0 ]
  grep -q '^image rm acore/ac-wotlk-worldserver:v1.2.3$' "$FIXTURE/calls.log"
}

# --- undeletable files (found on a clean VM, 2026-07-29) --------------------
# A bind-mounted database directory belongs to the image's user (mysql writes
# as uid 999), so a user-level `rm -rf` cannot delete it. That returned
# non-zero and `set -e` killed the removal MID-DELETE: the games/ symlink was
# gone, the real directory was half-emptied, and NO terminal event was emitted
# -- the launcher reported CLI_CRASH and every retry failed identically.
# Simulated here with a read-only parent directory, which produces the same
# EPERM without needing a container.

@test "games remove: an undeletable file reports REMOVE_FAILED instead of crashing" {
  mkdir -p "$FIXTURE/maplestory-server/database/docker-db-data"
  echo data > "$FIXTURE/maplestory-server/database/docker-db-data/ibdata1"
  # Read-only dir => its entries cannot be unlinked, exactly like root-owned
  # container files for an unprivileged user.
  chmod 500 "$FIXTURE/maplestory-server/database/docker-db-data"

  run bash "$DML" games remove maplestory-server --yes --json
  chmod 700 "$FIXTURE/maplestory-server/database/docker-db-data" || true

  # The load-bearing assertion: a TERMINAL event, not a silent death. Before
  # the fix the stream ended with no done/error at all and the runner had to
  # synthesize CLI_CRASH.
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"event":"error"'
  echo "$output" | tail -1 | grep -q 'REMOVE_FAILED'
  # And it must name what was left behind, so the user can finish the job.
  echo "$output" | tail -1 | grep -q 'maplestory-server'
}

@test "games remove: a normal title still removes cleanly and reports done" {
  mkdir -p "$FIXTURE/maplestory-server/database"
  echo x > "$FIXTURE/maplestory-server/database/f"
  run bash "$DML" games remove maplestory-server --yes --json
  [ "$status" -eq 0 ]
  echo "$output" | tail -1 | grep -q '"event":"done"'
  [ ! -e "$FIXTURE/maplestory-server" ]
}
