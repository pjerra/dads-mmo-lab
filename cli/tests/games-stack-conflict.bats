#!/usr/bin/env bats
#
# The stack-conflict REFUSAL, mirroring
# crates/dml-wow/src/lifecycle.rs `stack_conflict_refusal`.
#
# WHY THIS FILE NO LONGER TESTS PORTS. The first version of this guard probed
# 3724/8085/7878 and refused if it could not bind them. Measured against a live
# Docker Desktop on 2026-08-01, that question has no bearing on whether
# `docker compose up` can work: a port Docker was publishing -- LISTENING, and
# serving HTTP 200 -- probed as FREE, and a port an ordinary listener held
# probed as TAKEN yet `docker run -p` came up over it anyway. Wrong in both
# directions is worse than absent, because people trust a guard.
#
# The real constraint is the `ac-*` container names, which are global to the
# docker engine on every platform. That is what this now asserts.

load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"
  add_game wow compose
  # A REAL compose body, not add_game's empty touch: the guard now gates on
  # the compose file actually claiming the ac-* names (audit #8), so an empty
  # file would skip the guard and pass every REFUSES test below vacuously.
  printf 'services:\n  ac-worldserver:\n    container_name: ac-worldserver\n' \
    > "$DML_GAMES_DIR/wow/docker-compose.yml"
  STUB_BIN="$FIXTURE/bin"
  mkdir -p "$STUB_BIN"
  export PATH="$STUB_BIN:$PATH"
}

teardown() { teardown_fixture; }

# A docker stub whose `ps -a --format` answer the test controls. Everything else
# succeeds, so the start reaches the guard and then the compose call.
stub_docker() {  # stub_docker <ps output> [ps exit code]
  cat > "$STUB_BIN/docker" <<'EOS'
#!/usr/bin/env bash
if [[ "${1:-}" == "info" ]]; then exit 0; fi
if [[ "${1:-}" == "ps" ]]; then
  printf '%s\n' "$DOCKER_PS_OUT"
  exit "${DOCKER_PS_EXIT:-0}"
fi
echo "stub docker $*"
exit 0
EOS
  chmod +x "$STUB_BIN/docker"
  export DOCKER_PS_OUT="$1"
  export DOCKER_PS_EXIT="${2:-0}"
}

@test "no other stack: the start proceeds" {
  stub_docker "nginx some-web-project"
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "another stack owning ac-worldserver REFUSES the start" {
  stub_docker $'ac-worldserver	some-other-stack' 
  run bash "$DML" start wow
  [ "$status" -ne 0 ]
  [[ "$output" == *"ac-worldserver"* ]]
  # Naming the other stack is the point -- a conflict the user cannot locate
  # is not actionable.
  [[ "$output" == *"some-other-stack"* ]]
  # The refusal must land BEFORE compose runs.
  [[ "$output" != *"Starting wow"* ]]
}

@test "every container the stack claims is guarded" {
  # A name missing from the list is a collision the guard cannot see.
  for name in ac-database ac-db-import ac-client-data-init ac-authserver ac-worldserver; do
    stub_docker "$name"$'	'"other-stack"
    run bash "$DML" start wow
    [ "$status" -ne 0 ]
    [[ "$output" == *"$name"* ]]
  done
}

@test "a hand-run container with no compose project still conflicts" {
  # `docker run --name ac-database ...` owns the name just as firmly and reports
  # an empty project label. Skipping the unlabelled case is how it sails past.
  stub_docker "ac-database"
  run bash "$DML" start wow
  [ "$status" -ne 0 ]
  [[ "$output" == *"not managed by Docker Compose"* ]]
}

@test "a name that merely CONTAINS ours is not a conflict" {
  # Substring matching would refuse a start because of somebody's unrelated
  # backup container.
  stub_docker $'my-ac-database-backup	other-stack' 
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "a docker that cannot answer never blocks a start" {
  # TRI-STATE. A wedged or restarting docker is evidence of NOTHING, and must
  # not block a start it knows nothing about.
  stub_docker "" 1
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "an empty docker answer never blocks a start either" {
  stub_docker "" 0
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "the JSON games path refuses too -- the route the launcher takes" {
  # Audit #6: the guard lived only in the legacy start arm, so
  # `dml games start --json` (Home's Start button) sailed past it and
  # compose clobbered whichever stack owned the names.
  stub_docker $'ac-worldserver	some-other-stack'
  run bash "$DML" games start wow --json
  [ "$status" -ne 0 ]
  [[ "$output" == *'"code":"STACK_CONFLICT"'* ]]
  [[ "$output" == *"some-other-stack"* ]]
  [[ "$output" != *'"event":"done"'* ]]
}

@test "a non-AC title is never refused over WoW's containers" {
  # Audit #8: starting maplestory while a WoW server ran used to refuse,
  # telling the user to stop a server that was not in the way at all.
  add_game maple compose
  printf 'services:
  maple:
    container_name: maple-server
'     > "$DML_GAMES_DIR/maple/docker-compose.yml"
  stub_docker $'ac-worldserver	wow-stack'
  run bash "$DML" start maple
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting maple"* ]]
}

@test "our own stopped stack is not a conflict with starting ourselves" {
  # `docker compose stop` leaves the containers in ps -a, owned by this very
  # stack. Refusing a start over our own stopped containers told the user to
  # go stop the server they were trying to start.
  stub_docker "ac-worldserver"$'	'"wow"$'	'"$DML_GAMES_DIR/wow"
  run bash "$DML" start wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"Starting wow"* ]]
}

@test "the JSON path lets our own stopped stack start too" {
  stub_docker "ac-worldserver"$'	'"wow"$'	'"$DML_GAMES_DIR/wow"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  [[ "$output" == *'"event":"done"'* ]]
}

@test "own-stack exclusion holds across every path spelling a shell produces" {
  # THE LIVE INCIDENT (2026-08-02): the migrated server's label said
  # C:\Users\...\wow-server-playerbots while Git Bash's PWD said
  # /c/Users/..., and the byte comparison refused the user's OWN server.
  # The project name is dml-wow-native -- underivable -- so only the
  # working-dir comparison can rescue it.
  for spelling in \
    "$DML_GAMES_DIR/wow" \
    "$DML_GAMES_DIR/wow/" ; do
    stub_docker "ac-worldserver"$'\t'"dml-wow-native"$'\t'"$spelling"
    run bash "$DML" start wow
    [ "$status" -eq 0 ]
    [[ "$output" == *"Starting wow"* ]]
  done
}

@test "a foreign stack WITH a working-dir label still refuses" {
  # The exclusion must not decay into excluding everything that carries a
  # label -- a different directory's stack is what the guard exists to catch.
  stub_docker "ac-worldserver"$'\t'"dml-wow-native"$'\t'"/somewhere/else/wow"
  run bash "$DML" start wow
  [ "$status" -ne 0 ]
  [[ "$output" == *"dml-wow-native"* ]]
}

@test "_canon_path folds windows, msys and wsl spellings to one form" {
  # Extract the function from the built CLI and drive it directly -- the
  # spelling table is the contract.
  eval "$(sed -n '/^_canon_path()/,/^}/p' "$DML")"
  [ "$(_canon_path 'C:\Users\First Last\dml-native')" = "c:/users/first last/dml-native" ]
  [ "$(_canon_path 'C:/Users/First Last/dml-native/')" = "c:/users/first last/dml-native" ]
  [ "$(_canon_path '/c/Users/First Last/dml-native')" = "c:/users/first last/dml-native" ]
  [ "$(_canon_path '/mnt/c/Users/First Last/dml-native')" = "c:/users/first last/dml-native" ]
  # POSIX paths keep their case: /srv/A and /srv/a really differ on Linux.
  [ "$(_canon_path '/srv/Games/wow/')" = "/srv/Games/wow" ]
}
