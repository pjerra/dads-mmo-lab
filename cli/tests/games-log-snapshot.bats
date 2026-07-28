#!/usr/bin/env bats
# Incident follow-up 3: snapshot the worldserver log to ~/.dml/logs BEFORE a
# stop or restart. `docker compose down` + `up` RECREATES the containers and a
# recreated container starts with an EMPTY log -- during the 2026-07-21 freeze
# incident that destroyed the evidence twice, because the restart reached for
# to fix the freeze was also what erased the reason for it.
#
# The Rust twin (native mode) is crates/dml-wow/src/logsnap.rs; WSL mode runs
# THIS path, so both must behave the same.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  export HOME="$FIXTURE"          # sandboxes ~/.dml/logs
  LOGDIR="$FIXTURE/.dml/logs"
  add_game wow compose
  # What the stub serves for `docker logs` -- the incident's own signature.
  cat > "$FIXTURE/world.log" <<'EOS'
AzerothCore rev. deadbeef
[MySQL] Can't connect to MySQL server on 'ac-database' (110)
EOS
  export DML_STUB_LOGS_FILE="$FIXTURE/world.log"
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
}
teardown() { teardown_fixture; }

# First line number in the docker call log matching $1 (0 when absent).
_call_line() { grep -n -- "$1" "$FIXTURE/calls.log" | head -1 | cut -d: -f1; }

@test "games stop snapshots the worldserver log before the compose down" {
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  f="$(ls -1 "$LOGDIR")"
  [[ "$f" =~ ^world-[0-9]{8}-[0-9]{6}-wow-stop\.log$ ]]
  # Load-bearing: the file holds the REAL log bytes. A touch-an-empty-file
  # implementation (or one that names a file it never writes) fails here.
  grep -q "Can't connect to MySQL" "$LOGDIR/$f"
  echo "$output" | grep -q "worldserver log snapshot saved: $f"
  # ORDER is the whole point: the read must happen before the down that
  # destroys the log, not after it.
  snap_at="$(_call_line 'logs --tail')"
  down_at="$(_call_line 'compose down')"
  [ -n "$snap_at" ]
  [ -n "$down_at" ]
  [ "$snap_at" -lt "$down_at" ]
}

@test "games restart snapshots the worldserver log before its compose down" {
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --json
  [ "$status" -eq 0 ]
  f="$(ls -1 "$LOGDIR")"
  [[ "$f" =~ ^world-[0-9]{8}-[0-9]{6}-wow-restart\.log$ ]]
  grep -q "Can't connect to MySQL" "$LOGDIR/$f"
  snap_at="$(_call_line 'logs --tail')"
  down_at="$(_call_line 'compose down')"
  [ "$snap_at" -lt "$down_at" ]
}

@test "games restart snapshots before the dml-start.sh hook recreates containers" {
  # The hook path is the one WSL installs actually take, and it recreates the
  # containers itself (compose up --no-deps) -- so the snapshot has to be
  # taken before the hook runs, not before some `docker compose down` that
  # never happens on this branch.
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] staged start: mode=$1"
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'staged start: mode=restart'
  f="$(ls -1 "$LOGDIR")"
  grep -q "Can't connect to MySQL" "$LOGDIR/$f"
}

@test "games start does not snapshot (a cold start destroys no prior evidence)" {
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  [ ! -d "$LOGDIR" ]
  [[ "$output" != *"log snapshot"* ]]
}

@test "a snapshot that cannot be written only warns -- the stop still succeeds" {
  # A FILE where ~/.dml/logs should be: mkdir -p fails. The stop is what the
  # user asked for; evidence capture must never be able to block it.
  mkdir -p "$FIXTURE/.dml"
  printf 'not a directory\n' > "$FIXTURE/.dml/logs"
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | tail -1 | jq -r '.event')" = "done" ]
  [ "$(echo "$output" | tail -1 | jq -r '.data.state')" = "stopped" ]
  echo "$output" | grep -q '"level":"warn".*could not snapshot the worldserver log'
}

@test "no worldserver container: silent skip, no file, no warn" {
  # Stopping a non-WoW title has no ac-worldserver to read -- there is no
  # evidence to lose, so a warning would be pure noise on every other game in
  # the library. (`docker logs` exits non-zero with "No such container"; the
  # exit status, not the output, is what decides.)
  export DML_STUB_LOGS_EXIT=1
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ ! -d "$LOGDIR" ]
  [[ "$output" != *"snapshot"* ]]
}

@test "an empty worldserver log is skipped rather than saved as an empty file" {
  : > "$FIXTURE/world.log"
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ -z "$(ls -A "$LOGDIR" 2>/dev/null)" ]
  [[ "$output" != *"log snapshot saved"* ]]
}

@test "retention keeps the newest DML_LOG_SNAPSHOT_KEEP snapshots" {
  mkdir -p "$LOGDIR"
  printf 'old\n' > "$LOGDIR/world-20200101-000000-wow-stop.log"
  printf 'old\n' > "$LOGDIR/world-20200102-000000-wow-stop.log"
  # A foreign file must NOT eat a keep slot (nor be deleted).
  printf 'x\n' > "$LOGDIR/notes.txt"
  export DML_LOG_SNAPSHOT_KEEP=2
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ ! -f "$LOGDIR/world-20200101-000000-wow-stop.log" ]
  [ -f "$LOGDIR/world-20200102-000000-wow-stop.log" ]
  [ -f "$LOGDIR/notes.txt" ]
  [ "$(ls -1 "$LOGDIR" | grep -c '^world-.*\.log$')" -eq 2 ]
}

@test "a title whose own compose project has no worldserver takes no snapshot" {
  # The WoW stack is up, so a global `docker logs ac-worldserver` SUCCEEDS --
  # but this stop is for a different title, whose compose project owns no such
  # container. Reading the global name here files the WoW world's log under
  # the wrong title's name AND burns a slot in the shared newest-N window,
  # evicting the very evidence the feature exists to keep.
  export DML_STUB_COMPOSE_PS_ID=""      # this project resolves no container...
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/logs-args.log"
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ ! -d "$LOGDIR" ]
  [[ "$output" != *"snapshot"* ]]
  # ...and no log was read at all: the decision comes from the title's own
  # compose project, never from whether SOME ac-worldserver happens to exist.
  [ ! -f "$FIXTURE/logs-args.log" ]
}

@test "the snapshot reads the container the title's compose project resolves" {
  export DML_STUB_COMPOSE_PS_ID="c0ffee1234ab"
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/logs-args.log"
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  grep -q -- '--tail 2000 c0ffee1234ab' "$FIXTURE/logs-args.log"
  # A hardcoded container NAME is exactly the bug: it is not scoped to a
  # project, so it answers for whichever title happens to own it.
  ! grep -q 'ac-worldserver' "$FIXTURE/logs-args.log"
}

@test "compose failing to resolve the container is a silent skip, not a warn" {
  # `no such service: ac-worldserver` -- the shape every non-WoW compose file
  # produces. Nothing to preserve, so no file, no line, no read.
  export DML_STUB_COMPOSE_PS_EXIT=1
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/logs-args.log"
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ ! -d "$LOGDIR" ]
  [[ "$output" != *"snapshot"* ]]
  [ ! -f "$FIXTURE/logs-args.log" ]
}

@test "a docker logs read that never returns is bounded -- the stop still runs" {
  # dockerd wedged on a socket-activated start: the read connects and then
  # blocks forever. Evidence capture must never be able to block a stop the
  # user asked for (the function's own contract), and the Rust twin has capped
  # this read at 20s from day one.
  export DML_STUB_LOGS_HANG=25
  export DML_LOG_SNAPSHOT_TIMEOUT=2
  t0=$SECONDS
  run bash "$DML" games stop wow --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 0 ]
  # An unbounded read waits the stub's whole 25s; the bound cuts it at ~2s.
  [ "$elapsed" -lt 15 ]
  # The stop itself still happened, and the timed-out read left no file.
  [ "$(echo "$output" | tail -1 | jq -r '.data.state')" = "stopped" ]
  [ -z "$(ls -A "$LOGDIR" 2>/dev/null)" ]
  # ...and it SAID SO. The container was already resolved, so a read that hits
  # its deadline means evidence was lost to a wedged daemon -- exactly the
  # incident this feature exists for. Skipping silently would leave the
  # operator believing a snapshot was taken.
  echo "$output" | grep -q '"level":"warn"'
  echo "$output" | grep -q 'could not snapshot the worldserver log'
}

@test "the snapshot just written is never pruned away by its own retention" {
  # A backwards clock jump (WSL2 lagging the host after sleep/resume) leaves
  # NEWER-named snapshots on disk, so the fresh file sorts last and a prune
  # over the whole pool deletes the very file the caller reports as saved.
  mkdir -p "$LOGDIR"
  printf 'from the future\n' > "$LOGDIR/world-29991231-235959-wow-stop.log"
  export DML_LOG_SNAPSHOT_KEEP=1
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  f="$(echo "$output" | sed -n 's/.*worldserver log snapshot saved: \([^"]*\)".*/\1/p')"
  [ -n "$f" ]
  # The load-bearing assertion: what was REPORTED is what is on disk.
  [ -f "$LOGDIR/$f" ]
  grep -q "Can't connect to MySQL" "$LOGDIR/$f"
  # ...and the window still holds at exactly keep.
  [ "$(ls -1 "$LOGDIR" | grep -c '^world-.*\.log$')" -eq 1 ]
}

@test "DML_LOG_SNAPSHOT_KEEP=0 disables snapshots instead of saving then deleting" {
  export DML_LOG_SNAPSHOT_KEEP=0
  export DML_STUB_LOGS_ARGS_LOG="$FIXTURE/logs-args.log"
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ -z "$(ls -A "$LOGDIR" 2>/dev/null)" ]
  # Reporting "saved: X" for a file the prune deleted on the way out is the
  # one thing a retention window must never do.
  [[ "$output" != *"log snapshot saved"* ]]
  # Retention 0 means the feature is off, so nothing is even read.
  [ ! -f "$FIXTURE/logs-args.log" ]
}

@test "text mode reports the snapshot as a plain [dml] line, not JSON" {
  run bash "$DML" games stop wow
  [ "$status" -eq 0 ]
  [[ "$output" == *"[dml] worldserver log snapshot saved: world-"* ]]
  [[ "$output" != *'"event"'* ]]
}
