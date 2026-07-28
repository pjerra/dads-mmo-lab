#!/usr/bin/env bats
# `wow world-restart` (Batch 3 F11f): fast world-only restart -- saveall
# best-effort, docker restart -t 300 of ONLY ac-worldserver, readiness wait
# on the boot-complete marker. The settings-don't-apply caveat must be
# surfaced in the stream AND the done payload.
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  use_curl_stub
  export HOME="$FIXTURE"
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  add_game wow-server-playerbots compose
  # Boot-complete marker for the current run (the stub serves this for
  # `docker logs --since ...`, i.e. _world_ready's current-run view).
  printf 'World Initialized In 0 minutes 42 seconds\n' > "$FIXTURE/ready.log"
  export DML_STUB_LOGS_SINCE_FILE="$FIXTURE/ready.log"
}
teardown() { teardown_fixture; }

@test "world-restart: event sequence -- caveat warn, restart -t 300 world only, ok + done note" {
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 0 ]
  # Ordered event sequence.
  echo "$output" | head -1 | grep -q '"event":"section_start","name":"world-restart"'
  echo "$output" | grep -q '"level":"warn".*does NOT apply settings changes'
  echo "$output" | grep -q '"event":"section_end","name":"world-restart","status":"ok"'
  echo "$output" | tail -1 | grep -q '"event":"done"'
  [ "$(echo "$output" | tail -1 | jq -r '.data.restarted')" = "world-only" ]
  echo "$output" | tail -1 | jq -r '.data.note' | grep -q 'full Restart'
  # Exactly the world container was docker-restarted, gracefully...
  grep -q '^restart -t 300 ac-worldserver$' "$FIXTURE/calls.log"
  # ...and never a full compose cycle (that's the full Restart's job).
  run grep -E 'compose (down|up)' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "world-restart: saveall is attempted over SOAP before the restart" {
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 0 ]
  grep -q '7878' "$FIXTURE/curl.log"
}

@test "world-restart --no-saveall: skips the pre-stop saveall, still restarts" {
  export DML_STUB_CURL_LOG="$FIXTURE/curl.log"
  run bash "$DML" wow world-restart --no-saveall --json
  [ "$status" -eq 0 ]
  # Assert on the stream FIRST -- a later `run` would clobber $output.
  echo "$output" | grep -q 'skipping pre-stop saveall'
  [ "$(echo "$output" | tail -1 | jq -r '.data.restarted')" = "world-only" ]
  # No SOAP saveall attempted (curl never hit :7878 -- log absent or no match).
  run grep -q '7878' "$FIXTURE/curl.log"
  [ "$status" -ne 0 ]
}

@test "world-restart: a failed SOAP saveall does not block the restart (best effort)" {
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | tail -1 | jq -r '.data.restarted')" = "world-only" ]
}

@test "world-restart: docker restart failure -> RESTART_FAILED envelope" {
  export DML_STUB_RESTART_EXIT=1
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"section_end","name":"world-restart","status":"error"'
  echo "$output" | tail -1 | grep -q '"code":"RESTART_FAILED"'
}

@test "world-restart: readiness never seen -> READY_TIMEOUT" {
  printf 'still booting...\n' > "$FIXTURE/ready.log"
  export DML_READY_TIMEOUT_SECS=0
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"READY_TIMEOUT"'
}

@test "world-restart: server not installed -> NOT_FOUND" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"NOT_FOUND"'
}

@test "world-restart: docker down -> DOCKER_DOWN" {
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"DOCKER_DOWN"'
}

@test "world-restart: a stopped stack -> NOT_RUNNING with no long readiness wait" {
  # The world/database containers report not-running: `docker restart` would
  # otherwise START the world alone against a down DB and hang ~30 min.
  export DML_STUB_RUNNING_STATE=false
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"section_end","name":"world-restart","status":"error"'
  echo "$output" | tail -1 | grep -q '"code":"NOT_RUNNING"'
  # It bailed BEFORE issuing any docker restart (that is what would hang).
  run grep -q '^restart -t 300 ac-worldserver$' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "world-restart: a down database with the world up -> NOT_RUNNING (the hang guard)" {
  # Only the DB half guards the ~30min READY_TIMEOUT hang, so it stays strict.
  export DML_STUB_RUNNING_STATE_DB=false
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"NOT_RUNNING"'
  echo "$output" | tail -1 | grep -q 'database'
  run grep -q '^restart -t 300 ac-worldserver$' "$FIXTURE/calls.log"
  [ "$status" -ne 0 ]
}

@test "world-restart: a world that never comes back up fails fast, not at READY_TIMEOUT" {
  # Round 2 F1: the DB-only precondition deliberately ADMITS a crashed world
  # (that is the recovery path below). If the world then exits again instead of
  # booting -- bad conf value, missing map/DBC data, OOM -- the readiness wait
  # must notice the container is not running instead of burning the whole
  # DML_READY_TIMEOUT_SECS budget (30 min by default) while the launcher holds
  # the UI on "Restarting...".
  printf 'still booting...\n' > "$FIXTURE/ready.log"   # marker never appears...
  export DML_STUB_RUNNING_STATE_WORLD=false           # ...and the world is down
  export DML_READY_TIMEOUT_SECS=60
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  t0=$SECONDS
  run bash "$DML" wow world-restart --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"section_end","name":"world-restart","status":"error"'
  echo "$output" | tail -1 | grep -q '"code":"RESTART_FAILED"'
  echo "$output" | tail -1 | grep -q 'The world server exited instead of coming back up'
  # The restart WAS attempted -- this is the crashed-world recovery path, not
  # the precondition bail.
  grep -q '^restart -t 300 ac-worldserver$' "$FIXTURE/calls.log"
  # The load-bearing assertion: the stream ended WELL INSIDE the readiness
  # budget. A wait that only watches for the boot marker cannot satisfy this --
  # it returns at 60s. (Fast-fail costs ~8s: 5 consecutive down probes, 2s apart.)
  [ "$elapsed" -lt 30 ]
}

# --- boot-loop detection (incident follow-up 2) ------------------------------
# On the night of the 2026-07-21 incident the world crash-retried on "Can't
# connect to MySQL (110)" for ten minutes while this wait printed "still
# waiting (~Nm) - bots respawning takes a while...". Rust twin:
# crates/dml-wow/src/lifecycle.rs (wr_wait_for_world + boot_loop_note).

@test "world-restart: a boot loop is named, and the outcome is unchanged" {
  printf 'still booting...\n' > "$FIXTURE/ready.log"   # readiness never appears
  # The container is UP between crashes (State.Running stays true), so the
  # liveness guard never trips -- a climbing RestartCount is the ONLY thing
  # that distinguishes this from a genuinely slow boot.
  export DML_STUB_RESTART_COUNT_SEQ="0 1 2 3 4 5 6 7 8 9"
  export DML_STUB_RESTART_COUNT_SEQ_STATE="$FIXTURE/rc.seq"
  cat > "$FIXTURE/full.log" <<'EOS'
[MySQL] Can't connect to MySQL server on 'ac-database' (110)
[MySQL] Can't connect to MySQL server on 'ac-database' (110)
EOS
  export DML_STUB_LOGS_FILE="$FIXTURE/full.log"
  export DML_READY_TIMEOUT_SECS=10
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  # A DIAGNOSIS, not a new failure mode: same terminal event, same exit code.
  echo "$output" | tail -1 | grep -q '"code":"READY_TIMEOUT"'
  echo "$output" | grep -q '"level":"warn".*boot loop detected'
  echo "$output" | grep -q 'crash-retrying, not slow-booting'
  echo "$output" | grep -q 'repeated MySQL connection errors'
  echo "$output" | grep -q 'Restart Docker'
  # Latched: one line per wait, not one per two-second poll.
  [ "$(echo "$output" | grep -c 'boot loop detected')" -eq 1 ]
}

@test "world-restart: a slow but healthy boot is never called a boot loop" {
  printf 'still booting...\n' > "$FIXTURE/ready.log"
  export DML_STUB_RESTART_COUNT=0        # never climbs
  export DML_READY_TIMEOUT_SECS=6
  t0=$SECONDS
  run bash "$DML" wow world-restart --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"READY_TIMEOUT"'
  [[ "$output" != *"boot loop"* ]]
  # Load-bearing: the wait really did run to the budget (>=3 polls at 2s), so
  # the absence above is a decision and not "it never got that far".
  [ "$elapsed" -ge 6 ]
}

@test "world-restart: an unreadable restart count is never read as a boot loop" {
  # Same lesson the liveness guard already learned: "docker could not answer"
  # is evidence of nothing. An empty inspect reading must not manufacture a
  # diagnosis (nor reset the baseline into one).
  printf 'still booting...\n' > "$FIXTURE/ready.log"
  export DML_STUB_RESTART_COUNT=""
  export DML_READY_TIMEOUT_SECS=6
  t0=$SECONDS
  run bash "$DML" wow world-restart --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"READY_TIMEOUT"'
  [[ "$output" != *"boot loop"* ]]
  [ "$elapsed" -ge 6 ]
}

# The two tests below exist because the constant-unreadable one above CANNOT
# discriminate: with every reading unreadable, "skip the reading", "collapse it
# to 0" and "reset the baseline" all produce the same empty output. Each of
# these feeds INTERLEAVED readable/unreadable readings, where the three
# implementations disagree. (Rust twin:
# wr_wait_for_world_survives_a_hiccup_between_restart_count_readings.)

@test "world-restart: an inspect hiccup between readings does not hide a boot loop" {
  # 0, then docker fails to answer, then 3 -- a real +3 climb interrupted by
  # one bad reading. Skipping the bad reading keeps the baseline (0) and the
  # loop is still named; RESETTING the baseline on it (wr_rc_base="") re-bases
  # at 3 and the loop is never diagnosed, which on a box with a wedged daemon
  # is every few polls.
  printf 'still booting...\n' > "$FIXTURE/ready.log"
  export DML_STUB_RESTART_COUNT_SEQ="0 - 3 4"
  export DML_STUB_RESTART_COUNT_SEQ_STATE="$FIXTURE/rc.seq"
  export DML_READY_TIMEOUT_SECS=10
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"READY_TIMEOUT"'
  # The DELTA is measured from the pre-hiccup baseline, not from the reading
  # after it -- a re-based implementation could at best report 1.
  echo "$output" | grep -q 'restarted 3 times since this wait began'
}

@test "world-restart: an unreadable FIRST reading does not become a fake baseline" {
  # docker misses the very first poll, then answers 7 -- a long-lived server
  # carrying historical restarts. Skipping the bad reading makes 7 the
  # baseline (delta 0, no note); collapsing it to 0 makes 0 the baseline and
  # the next reading is a +7 "boot loop" on a perfectly healthy server.
  printf 'still booting...\n' > "$FIXTURE/ready.log"
  export DML_STUB_RESTART_COUNT_SEQ="- 7 7 7"
  export DML_STUB_RESTART_COUNT_SEQ_STATE="$FIXTURE/rc.seq"
  export DML_READY_TIMEOUT_SECS=8
  t0=$SECONDS
  run bash "$DML" wow world-restart --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"READY_TIMEOUT"'
  [[ "$output" != *"boot loop"* ]]
  # Load-bearing: the wait really did poll past the readable readings, so the
  # absence above is a decision and not "it never got that far".
  [ "$elapsed" -ge 8 ]
}

@test "world-restart: with no MySQL evidence the note points at the log, not the database" {
  printf 'still booting...\n' > "$FIXTURE/ready.log"
  export DML_STUB_RESTART_COUNT_SEQ="0 1 2 3 4 5 6 7 8 9"
  export DML_STUB_RESTART_COUNT_SEQ_STATE="$FIXTURE/rc.seq"
  printf 'terminate called after throwing an instance of std::bad_alloc\n' > "$FIXTURE/full.log"
  export DML_STUB_LOGS_FILE="$FIXTURE/full.log"
  export DML_READY_TIMEOUT_SECS=10
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'boot loop detected'
  # It must not assert a cause it did not observe.
  [[ "$output" != *"MySQL connection errors"* ]]
  echo "$output" | grep -q 'Check the Console log for the boot error'
}

@test "world-restart: a down world with a healthy database is a legitimate recovery restart" {
  # `docker restart` on a stopped container STARTS it, and with the DB up
  # there is nothing to hang on -- this is the Home card's crashed-verdict
  # recovery path, so it must proceed instead of claiming nothing is running.
  export DML_STUB_RUNNING_STATE_WORLD=false
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  run bash "$DML" wow world-restart --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q '"level":"info".*world server is not running'
  [ "$(echo "$output" | tail -1 | jq -r '.data.restarted')" = "world-only" ]
  grep -q '^restart -t 300 ac-worldserver$' "$FIXTURE/calls.log"
}
