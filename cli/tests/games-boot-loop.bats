#!/usr/bin/env bats
# Boot-loop detection on the START/RESTART path users actually take
# (incident follow-up 2, round-2 findings G3/G9).
#
# Home's primary Start and Restart buttons call `dml games start|restart`, NOT
# `wow world-restart` (that button is feature-locked). `_games_start_impl`
# hands the whole boot off to the title's own `<compose_dir>/dml-start.sh`
# hook, whose readiness wait narrated the 2026-07-21 crash loop as "still
# waiting ... world is loading" for the full 30-minute budget.
#
# The hook is a DEPLOYED artifact -- a copy sits in every installed title dir
# and is never refreshed by a CLI update -- so the detection lives in the CLI
# (which IS updated) and watches the hook from the outside while it streams.
# That is what these tests drive: a fake hook plus a docker stub whose
# .RestartCount climbs (TOP-LEVEL -- docker rejects .State.RestartCount).
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  export HOME="$FIXTURE"
  add_game wow compose
  export DML_STUB_CALL_LOG="$FIXTURE/calls.log"
  export DML_STUB_RESTART_COUNT_SEQ_STATE="$FIXTURE/rc.seq"
  # Retention off: these tests are about the boot-loop note, not the log
  # snapshot that shares the same stop/restart path.
  export DML_LOG_SNAPSHOT_KEEP=0
  # Poll every second so a test finishes in seconds instead of the 15s
  # production cadence.
  export DML_BOOT_LOOP_POLL_SECS=1
  # The incident's own log signature, for the cause half of the note.
  cat > "$FIXTURE/world.log" <<'EOS'
[MySQL] Can't connect to MySQL server on 'ac-database' (110)
[MySQL] Can't connect to MySQL server on 'ac-database' (110)
EOS
  export DML_STUB_LOGS_FILE="$FIXTURE/world.log"
}
teardown() { teardown_fixture; }

# A hook that behaves like guides/wow-wotlk/dml-start.sh's _wait_ready: it
# keeps printing keepalive lines and finally gives up. $1 = seconds, $2 = exit.
_chatty_hook() {
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<EOS
#!/usr/bin/env bash
echo "[dml] Starting WoW server (staged)..."
for i in \$(seq 1 $1); do
  echo "[dml] still waiting (~\${i}s) - world is loading (cold starts with many bots can take 15+ minutes)..."
  sleep 1
done
echo "[dml] HOOK_FINISHED"
exit $2
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
}

# How many times the watch actually asked docker for the restart count.
_rc_probes() { grep -c 'inspect -f {{.RestartCount}}' "$FIXTURE/calls.log" || true; }

@test "games restart names a boot loop while the start hook is still waiting" {
  _chatty_hook 8 1
  # Up between crashes, so nothing but a climbing RestartCount can tell this
  # apart from a genuinely slow boot.
  export DML_STUB_RESTART_COUNT_SEQ="0 1 2 3 4 5 6 7 8 9"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --json
  # A DIAGNOSIS ONLY: same failure, same exit code as before.
  [ "$status" -eq 1 ]
  echo "$output" | tail -1 | grep -q '"code":"START_FAILED"'
  echo "$output" | grep -q '"level":"warn".*boot loop detected'
  echo "$output" | grep -q 'crash-retrying, not slow-booting'
  echo "$output" | grep -q 'repeated MySQL connection errors'
  # Latched: one line per boot, not one per poll.
  [ "$(echo "$output" | grep -c 'boot loop detected')" -eq 1 ]
  # The hook's own stream is untouched.
  echo "$output" | grep -q 'world is loading'
  echo "$output" | grep -q 'HOOK_FINISHED'
}

@test "games start names a boot loop even while the hook is silent" {
  # The load-bearing difference from a line-triggered implementation: the real
  # hook can go minutes without printing (its keepalive is 60s apart, and an
  # older deployed copy may not print at all). The watch must tick on its own
  # clock, not on the child's output.
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] Starting WoW server (staged)..."
sleep 8
echo "[dml] HOOK_FINISHED"
exit 1
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RESTART_COUNT_SEQ="0 1 2 3 4 5 6 7 8 9"
  run bash "$DML" games start wow --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q 'boot loop detected'
  echo "$output" | grep -q 'HOOK_FINISHED'
  # Proof the ticks came from the timer: the hook printed exactly two lines,
  # so a per-line watch could never have reached the 3-restart threshold.
  [ "$(_rc_probes)" -ge 4 ]
}

@test "games start: a slow but healthy boot is never accused" {
  _chatty_hook 7 0
  export DML_STUB_RESTART_COUNT=0        # never climbs
  t0=$SECONDS
  run bash "$DML" games start wow --json
  elapsed=$(( SECONDS - t0 ))
  [ "$status" -eq 0 ]
  echo "$output" | tail -1 | grep -q '"event":"done"'
  [[ "$output" != *"boot loop"* ]]
  # THREE assertions an early-returning / do-nothing watch cannot satisfy:
  # the boot really did run to the end...
  echo "$output" | grep -q 'HOOK_FINISHED'
  # ...it really took long enough for the threshold to be reachable...
  [ "$elapsed" -ge 7 ]
  # ...and the watch really did poll docker across that whole boot. A watch
  # that never runs, or that returns before the first tick, scores 0 here.
  [ "$(_rc_probes)" -ge 4 ]
}

@test "games start: an inspect hiccup between readings does not hide a boot loop" {
  # 0, docker fails to answer, then 3 -- a real +3 climb interrupted by one bad
  # reading. Skipping the bad reading keeps the baseline (0) and the loop is
  # still named; RESETTING the baseline on it re-bases at 3 and the loop is
  # never diagnosed, which on a wedged daemon is every few polls.
  _chatty_hook 8 1
  export DML_STUB_RESTART_COUNT_SEQ="0 - 3 4 5"
  run bash "$DML" games start wow --json
  [ "$status" -eq 1 ]
  # The delta is measured from the PRE-hiccup baseline: a re-based
  # implementation could at best report 1.
  echo "$output" | grep -q 'restarted 3 times since this wait began'
}

@test "games start: an unreadable first reading does not become a fake baseline" {
  # docker misses the first poll, then answers 7 -- a long-lived server
  # carrying historical restarts. Skipping the bad reading makes 7 the
  # baseline (delta 0, silence); collapsing it to 0 makes the next reading a
  # +7 "boot loop" on a perfectly healthy server.
  _chatty_hook 7 0
  export DML_STUB_RESTART_COUNT_SEQ="- 7 7 7 7 7 7"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  [[ "$output" != *"boot loop"* ]]
  echo "$output" | grep -q 'HOOK_FINISHED'
  [ "$(_rc_probes)" -ge 4 ]
}

@test "games start: a title whose compose project owns no world container is never accused" {
  # Same scoping rule the log snapshot learned: `docker inspect ac-worldserver`
  # answers for whichever title owns that container, so a MapleStory start
  # while the WoW stack crash-loops must not be told IT is looping.
  _chatty_hook 6 0
  export DML_STUB_COMPOSE_PS_ID=""       # this project owns no ac-worldserver
  export DML_STUB_RESTART_COUNT_SEQ="0 1 2 3 4 5 6 7 8 9"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  [[ "$output" != *"boot loop"* ]]
  echo "$output" | grep -q 'HOOK_FINISHED'
  # It never even asked: an unscoped watch would have probed the shared name.
  [ "$(_rc_probes)" -eq 0 ]
  # ...but it DID run and DID ask this title's own compose project -- so the
  # silence above is a scoping decision, not "the watch never started".
  [ "$(grep -c 'ps -a -q ac-worldserver' "$FIXTURE/calls.log" || true)" -ge 3 ]
}

@test "games start: text mode output is unchanged (the legacy tray parses it)" {
  _chatty_hook 6 0
  export DML_STUB_RESTART_COUNT_SEQ="0 1 2 3 4 5 6 7 8 9"
  run bash "$DML" games start wow
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'HOOK_FINISHED'
  echo "$output" | tail -1 | grep -q '^\[dml\] wow started$'
  [[ "$output" != *"boot loop"* ]]
}
