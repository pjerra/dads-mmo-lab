# Server Status & Health Panel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the SOAP-only "World is up/down" lie with a container-first four-state verdict (`stopped` / `starting` / `online` / `soap_unreachable`) via a new `dml wow server-detail` command, and give Home an expandable health panel.

**Architecture:** New read-only CLI verb aggregates `docker ps -a` (three long-running containers), a `--since`-guarded boot-marker grep of the worldserver log, one SOAP `server info` probe, and `docker port` lookups into a single `json_ok` envelope with a CLI-derived `verdict`. Home and Dashboard render from `verdict`; Home's server card expands into an inline health panel fed by the same fetch.

**Tech Stack:** bash (cli/src concatenated by `cli/build.sh`), bats + stubs, Tauri 2 Rust command layer, Svelte 5 runes, TypeScript.

## Global Constraints

- Everything stays on `feat/dml-launcher-windows`; NO merge.
- `cli/dml` is a committed build artifact: NEVER hand-edit; run `bash cli/build.sh` and commit the regenerated file with the source.
- `set -euo pipefail` is active in the built CLI: guard every fallible command substitution (`if v="$(...)"; then` or `|| true`); helpers use `local` and end with `return 0` — EXCEPT exit-status helpers (like `_valid_charname`), documented as such; NO `local` in the top-level dispatch case in `90-main.sh`.
- `server-detail` is strictly read-only: docker inspection + one SOAP `server info`. NO MySQL, NO writes, NO NDJSON — single `json_ok` envelope like `server-info`.
- Down is an answer, not an error: docker daemon down / containers absent → verdict `stopped`, exit 0. The verb has NO error paths.
- Verdict derivation (exact, in this order): world container not `running` → `stopped`; else SOAP reachable (rc 0, 2, or 3) → `online`; else world_ready → `soap_unreachable`; else `starting`.
- Boot marker: case-insensitive grep for `World Initialized In` on `docker logs --since <StartedAt of ac-worldserver>` (stale-marker guard: `compose stop`/`start` preserves logs).
- SOAP classification for this verb: rc 0 → `reachable:true, auth_ok:true` + stats; rc 2 (fault — the server answered) → `reachable:true, auth_ok:true`, stats null; rc 3 (401) → `reachable:true, auth_ok:false`, stats null; rc 4 → `reachable:false, auth_ok:null`, stats null. (`server-info` itself is UNTOUCHED, its bats pins stay green.)
- Containers array: exactly `ac-worldserver` (role `world`), `ac-authserver` (role `auth`), `ac-database` (role `database`), in that fixed order; `state` is Docker's state string or `absent`; one-shot containers (ac-db-import, ac-client-data-init) excluded.
- Ports JSON: host-port strings or null — keys `world` (8085 on ac-worldserver), `auth` (3724 on ac-authserver), `soap` (7878 on ac-worldserver), `db` (3306 on ac-database).
- UI copy (exact): stopped → "Server is stopped" / "Start the server below." (Home) / "Start it from the Library page." (Dashboard); starting → "Starting up…" / "The world is still loading — this takes a couple of minutes while bots spawn."; online → "World is up" + stats; soap_unreachable → "World is running, but the launcher can't reach it" / "If this persists for more than a minute, Docker's networking in the distro is likely stuck — restarting Docker inside dml-arch usually fixes it."
- Gates after every task: full bats suite green (`wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"` — DrvFs flake "cannot execute binary file" → re-run once), and for launcher tasks `npm test` (vitest), `npm run check` (svelte-check), `cd src-tauri; cargo test` — baselines 234 bats / 19 vitest / 17 cargo / 0 errors 0 warnings.
- Commits end with `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`.

---

## File Map

- `cli/src/40-config.sh` — split `_parse_server_info` into `_parse_server_info_fields` + wrapper; add `_detail_container_rows`, `_world_ready`, `_host_port_json` (Task 1)
- `cli/src/90-main.sh` — `server-detail` arm next to `server-info` (Task 1)
- `cli/tests/helpers/env.bash` — `use_docker_stub` grows `ps -a` / `logs` / `inspect` / `port` arms (Task 1)
- `cli/tests/wow-server-detail.bats` — NEW verdict-matrix suite (Task 1)
- `launcher/src-tauri/src/lib.rs` — `wow_server_detail` command + registration (Task 2)
- `launcher/src/lib/api.ts` — `ServerDetail` types + `wowServerDetail()` (Task 2)
- `launcher/src/lib/pages/Home.svelte` — four-state card + expandable health panel (Task 3)
- `launcher/src/lib/pages/Dashboard.svelte` — four-state card (Task 4)

The root README does not document individual launcher pages — no README change in this round.

---

### Task 1: CLI `wow server-detail` + docker stub arms + bats matrix

**Files:**
- Modify: `cli/src/40-config.sh` (replace `_parse_server_info` lines 11-33; append helpers at end of file)
- Modify: `cli/src/90-main.sh` (insert `server-detail)` arm immediately after the `server-info)` arm's closing `;;` at line ~1094)
- Modify: `cli/tests/helpers/env.bash` (extend `use_docker_stub`)
- Create: `cli/tests/wow-server-detail.bats`
- Commit also: regenerated `cli/dml`

**Interfaces:**
- Consumes: `soap_exec` (20-soap.sh; rc 0 ok / 2 fault / 3 auth / 4 unreachable), `json_ok`/`json_escape` (10-json.sh), curl stub env seams (`DML_STUB_SOAP_RESPONSE`, `DML_STUB_HTTP`, `DML_STUB_CURL_EXIT`, `DML_STUB_CAPTURE`), fixtures `cli/tests/fixtures/server-info-live.txt` and `soap-fault.xml`.
- Produces: `dml wow server-detail --json` → `{"ok":true,"data":{"verdict":...,"containers":[...],"world_ready":bool,"soap":{...},"ports":{...}}}` exactly as specified below; stub seams `DML_STUB_PS_ROWS`, `DML_STUB_LOGS_FILE`, `DML_STUB_LOGS_SINCE_FILE`, `DML_STUB_STARTED_AT` (Tasks 2-4 consume the JSON shape only).

- [ ] **Step 1: Extend `use_docker_stub` in `cli/tests/helpers/env.bash`**

Replace the line `if [[ "${1:-}" == "ps" ]]; then exit 0; fi` inside the `use_docker_stub` heredoc with:

```bash
if [[ "${1:-}" == "ps" ]]; then
  # server-detail: `docker ps -a --format '{{.Names}}|{{.State}}|{{.Status}}'`
  # -> canned rows from DML_STUB_PS_ROWS (a file). Daemon-down => exit 1
  # with no output, like real docker.
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  [[ -n "${DML_STUB_PS_ROWS:-}" && -f "${DML_STUB_PS_ROWS}" ]] && cat "$DML_STUB_PS_ROWS"
  exit 0
fi
if [[ "${1:-}" == "inspect" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  printf '%s\n' "${DML_STUB_STARTED_AT:-2026-07-17T10:00:00.000000000Z}"
  exit 0
fi
if [[ "${1:-}" == "logs" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  # The REAL --since filtering is docker's job, so the stub emulates it:
  # when the caller passed --since and DML_STUB_LOGS_SINCE_FILE is set,
  # serve that file (the "current run only" view); otherwise serve the
  # full log. The stale-marker test relies on the two views differing.
  if [[ "$*" == *"--since"* && -n "${DML_STUB_LOGS_SINCE_FILE:-}" ]]; then
    cat "$DML_STUB_LOGS_SINCE_FILE"
  elif [[ -n "${DML_STUB_LOGS_FILE:-}" && -f "${DML_STUB_LOGS_FILE}" ]]; then
    cat "$DML_STUB_LOGS_FILE"
  fi
  exit 0
fi
if [[ "${1:-}" == "port" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1
  # `docker port <name> <internal>` -> DML_STUB_PORTS is a newline table of
  # "<container> <internal> <hostport>"; matching row prints "0.0.0.0:<hostport>".
  if [[ -n "${DML_STUB_PORTS:-}" ]]; then
    while read -r c i h; do
      [[ "$c" == "${2:-}" && "$i" == "${3:-}" ]] && echo "0.0.0.0:$h"
    done <<< "$DML_STUB_PORTS"
  fi
  exit 0
fi
```

- [ ] **Step 2: Write the failing bats suite `cli/tests/wow-server-detail.bats`**

```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
  use_curl_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

all_running_rows() {
  cat > "$FIXTURE/ps.rows" <<'EOF'
ac-database|running|Up 41 seconds (healthy)
ac-worldserver|running|Up 33 seconds
ac-authserver|running|Up 33 seconds
EOF
  export DML_STUB_PS_ROWS="$FIXTURE/ps.rows"
}

booting_log() {
  cat > "$FIXTURE/boot.log" <<'EOF'
Loading Creature templates...
778/1887 Bot Coischawhu logged in
EOF
  export DML_STUB_LOGS_FILE="$FIXTURE/boot.log"
}

ready_log() {
  cat > "$FIXTURE/ready.log" <<'EOF'
Playerbots World Thread Processor initialized
WORLD: World Initialized In 0 Minutes 14 Seconds
AC>
EOF
  export DML_STUB_LOGS_FILE="$FIXTURE/ready.log"
}

soap_live_response() {
  {
    printf '<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>'
    cat "$BATS_TEST_DIRNAME/fixtures/server-info-live.txt"
    printf '</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>'
  } > "$FIXTURE/si.xml"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/si.xml"
}

@test "server-detail: no containers at all -> stopped, all absent, ports null, exit 0" {
  export DML_STUB_CAPTURE="$FIXTURE/soap-probe.xml"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.containers | length')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].name')" = "ac-worldserver" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].role')" = "world" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "absent" ]
  [ "$(echo "$output" | jq -r '.data.containers[1].role')" = "auth" ]
  [ "$(echo "$output" | jq -r '.data.containers[2].role')" = "database" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.ports.world')" = "null" ]
  # World not running -> SOAP must not even be probed.
  [ ! -f "$FIXTURE/soap-probe.xml" ]
}

@test "server-detail: docker daemon down -> stopped with absent containers, exit 0" {
  all_running_rows
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "absent" ]
}

@test "server-detail: all running + SOAP answers -> online with stats" {
  all_running_rows
  ready_log
  soap_live_response
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.players')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.soap.uptime')" = "19 minute(s) 29 second(s)" ]
  [ "$(echo "$output" | jq -r '.data.soap.mean_ms')" = "44" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "running" ]
  [ "$(echo "$output" | jq -r '.data.containers[2].status')" = "Up 41 seconds (healthy)" ]
}

@test "server-detail: running, SOAP dead, no marker yet -> starting" {
  all_running_rows
  booting_log
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "starting" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "null" ]
}

@test "server-detail: running, SOAP dead, marker present -> soap_unreachable" {
  all_running_rows
  ready_log
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "soap_unreachable" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "true" ]
}

@test "server-detail: stale marker from previous run is ignored (--since guard)" {
  all_running_rows
  # Full log HAS the marker (previous run), the since-StartedAt view does NOT.
  # If the CLI forgot --since, the stub serves the full log and this fails.
  ready_log
  cat > "$FIXTURE/since.log" <<'EOF'
Loading Creature templates...
12/1887 Bot Somebot logged in
EOF
  export DML_STUB_LOGS_SINCE_FILE="$FIXTURE/since.log"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "starting" ]
  [ "$(echo "$output" | jq -r '.data.world_ready')" = "false" ]
}

@test "server-detail: 401 means the world answered -> online with auth_ok false" {
  all_running_rows
  ready_log
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.soap.reachable')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.soap.players')" = "null" ]
}

@test "server-detail: a SOAP fault is still an answer -> online, stats null" {
  all_running_rows
  ready_log
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "online" ]
  [ "$(echo "$output" | jq -r '.data.soap.auth_ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.soap.players')" = "null" ]
}

@test "server-detail: ports come from docker port as strings" {
  all_running_rows
  ready_log
  soap_live_response
  export DML_STUB_PORTS="ac-worldserver 8085 8085
ac-worldserver 7878 7878
ac-authserver 3724 3724
ac-database 3306 3306"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.ports.world')" = "8085" ]
  [ "$(echo "$output" | jq -r '.data.ports.auth')" = "3724" ]
  [ "$(echo "$output" | jq -r '.data.ports.soap')" = "7878" ]
  [ "$(echo "$output" | jq -r '.data.ports.db')" = "3306" ]
}

@test "server-detail: world exited -> stopped, docker status text passes through" {
  cat > "$FIXTURE/ps.rows" <<'EOF'
ac-database|running|Up 2 hours (healthy)
ac-worldserver|exited|Exited (137) 5 minutes ago
ac-authserver|exited|Exited (0) 5 minutes ago
EOF
  export DML_STUB_PS_ROWS="$FIXTURE/ps.rows"
  run bash "$DML" wow server-detail --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.verdict')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].state')" = "exited" ]
  [ "$(echo "$output" | jq -r '.data.containers[0].status')" = "Exited (137) 5 minutes ago" ]
  [ "$(echo "$output" | jq -r '.data.containers[2].state')" = "running" ]
}

@test "server-info still behaves exactly as before (regression canary)" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow server-info --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online')" = "false" ]
}
```

- [ ] **Step 3: Run the new suite to verify it fails**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-server-detail.bats"`
Expected: FAIL — the `server-detail` arm doesn't exist yet, so the CLI prints the unknown-subcommand error and every `jq` assertion fails.

- [ ] **Step 4: Split `_parse_server_info` in `cli/src/40-config.sh`**

Replace the whole `_parse_server_info()` function (lines 11-33) with:

```bash
# Parses the raw text of the SOAP `server info` result (stdin) into the JSON
# field fragment (stdout, no braces/online key) shared by server-info and
# server-detail. The raw text carries literal `&#xD;` entities because
# soap_parse_result extracts the <result> text without XML-decoding it.
# Unparseable fields become null rather than an error -- the UI renders
# "unknown" for those instead of failing the whole card.
_parse_server_info_fields() {
    local raw line version="" players="" uptime="" mean="" median=""
    raw="$(cat)"
    raw="${raw//&#xD;/}"
    while IFS= read -r line; do
        case "$line" in
            AzerothCore\ rev.*) version="${line#AzerothCore rev. }" ;;
            Connected\ players:*) players="${line#Connected players: }"; players="${players%%.*}" ;;
            Server\ uptime:*) uptime="${line#Server uptime: }" ;;
            *'|- Mean:'*) mean="${line#*Mean: }"; mean="${mean%%ms*}" ;;
            *'|- Median:'*) median="${line#*Median: }"; median="${median%%ms*}" ;;
        esac
    done <<< "$raw"
    [[ "$players" =~ ^[0-9]+$ ]] || players=null
    [[ "$mean" =~ ^[0-9]+$ ]] || mean=null
    [[ "$median" =~ ^[0-9]+$ ]] || median=null
    local vjson=null ujson=null
    [[ -n "$version" ]] && vjson="\"$(json_escape "$version")\""
    [[ -n "$uptime" ]] && ujson="\"$(json_escape "$uptime")\""
    printf '"version":%s,"players":%s,"uptime":%s,"mean_ms":%s,"median_ms":%s' \
        "$vjson" "$players" "$ujson" "$mean" "$median"
    return 0
}

# Back-compat wrapper: the `server-info` verb's envelope shape is public API.
_parse_server_info() {
    printf '{"online":true,%s}' "$(_parse_server_info_fields)"
    return 0
}
```

- [ ] **Step 5: Append the server-detail helpers to the end of `cli/src/40-config.sh`**

```bash
# --- server-detail helpers -------------------------------------------------
# All read-only. Down/absent is data, never an error.

# One "name|state|status" line per long-running service (fixed order:
# world, auth, database), from a single `docker ps -a`. Absent containers
# (including docker daemon down) get state "absent" and empty status.
_detail_container_rows() {
    local ps_out="" name line found
    ps_out="$(docker ps -a --format '{{.Names}}|{{.State}}|{{.Status}}' 2>/dev/null || true)"
    for name in ac-worldserver ac-authserver ac-database; do
        found=""
        while IFS= read -r line; do
            [[ "${line%%|*}" == "$name" ]] && { found="$line"; break; }
        done <<< "$ps_out"
        if [[ -n "$found" ]]; then
            printf '%s\n' "$found"
        else
            printf '%s|absent|\n' "$name"
        fi
    done
    return 0
}

# Exit-status helper (like _valid_charname): 0 when the CURRENT worldserver
# run has logged AzerothCore's boot-complete marker. `compose stop`/`start`
# preserves container logs, so a marker from the previous run would lie
# during a re-boot -- hence --since the container's StartedAt.
_world_ready() {
    local started hits
    if started="$(docker inspect -f '{{.State.StartedAt}}' ac-worldserver 2>/dev/null)"; then :; else return 1; fi
    [[ -z "$started" ]] && return 1
    hits="$(docker logs --since "$started" ac-worldserver 2>&1 | grep -ic 'World Initialized In' || true)"
    [[ "${hits:-0}" -gt 0 ]]
}

# Host port for a container's internal port as a JSON string, or `null`.
# `docker port` prints one "0.0.0.0:8085"-style line per bind; take the first.
_host_port_json() {
    local out=""
    out="$(docker port "$1" "$2" 2>/dev/null | head -n1 || true)"
    out="${out##*:}"
    if [[ "$out" =~ ^[0-9]+$ ]]; then printf '"%s"' "$out"; else printf 'null'; fi
    return 0
}
```

- [ ] **Step 6: Add the `server-detail)` arm in `cli/src/90-main.sh`**

Insert directly after the `server-info)` arm's closing `;;` (after current line ~1094). Top-level dispatch: NO `local`.

```bash
      server-detail)
        # Container state first, SOAP second -- the four-state verdict.
        # Read-only; down/booting are answers, so this verb never errors.
        detail_rows="$(_detail_container_rows)"
        detail_world_state=""; detail_containers=""
        while IFS='|' read -r dc_name dc_state dc_status; do
          case "$dc_name" in
            ac-worldserver) dc_role=world ;;
            ac-authserver) dc_role=auth ;;
            *) dc_role=database ;;
          esac
          [[ "$dc_name" == ac-worldserver ]] && detail_world_state="$dc_state"
          dc_entry="$(printf '{"name":"%s","role":"%s","state":"%s","status":"%s"}' \
            "$(json_escape "$dc_name")" "$dc_role" "$(json_escape "$dc_state")" "$(json_escape "$dc_status")")"
          if [[ -z "$detail_containers" ]]; then detail_containers="$dc_entry"
          else detail_containers="$detail_containers,$dc_entry"; fi
        done <<< "$detail_rows"
        detail_ready=false
        if [[ "$detail_world_state" == running ]] && _world_ready; then detail_ready=true; fi
        detail_reach=false; detail_auth=null
        detail_stats='"version":null,"players":null,"uptime":null,"mean_ms":null,"median_ms":null'
        if [[ "$detail_world_state" == running ]]; then
          if out="$(soap_exec 'server info')"; then rc=0; else rc=$?; fi
          case "$rc" in
            0) detail_reach=true; detail_auth=true
               detail_stats="$(printf '%s' "$out" | _parse_server_info_fields)" ;;
            2) detail_reach=true; detail_auth=true ;;
            3) detail_reach=true; detail_auth=false ;;
            *) detail_reach=false ;;
          esac
        fi
        if [[ "$detail_world_state" != running ]]; then detail_verdict=stopped
        elif [[ "$detail_reach" == true ]]; then detail_verdict=online
        elif [[ "$detail_ready" == true ]]; then detail_verdict=soap_unreachable
        else detail_verdict=starting; fi
        detail_pw="$(_host_port_json ac-worldserver 8085)"
        detail_pa="$(_host_port_json ac-authserver 3724)"
        detail_psp="$(_host_port_json ac-worldserver 7878)"
        detail_pd="$(_host_port_json ac-database 3306)"
        json_ok "{\"verdict\":\"$detail_verdict\",\"containers\":[$detail_containers],\"world_ready\":$detail_ready,\"soap\":{\"reachable\":$detail_reach,\"auth_ok\":$detail_auth,$detail_stats},\"ports\":{\"world\":$detail_pw,\"auth\":$detail_pa,\"soap\":$detail_psp,\"db\":$detail_pd}}"
        ;;
```

- [ ] **Step 7: Rebuild and run the new suite to verify it passes**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-server-detail.bats"`
Expected: 11/11 PASS.

- [ ] **Step 8: Run the FULL bats suite (regression gate)**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"`
Expected: 245 tests, 0 failures (234 baseline + 11 new). The `_parse_server_info` split must keep every existing `wow-server-info.bats` and `soap.bats` test green. DrvFs flake ("cannot execute binary file") → re-run once.

- [ ] **Step 9: Commit**

```bash
git add cli/src/40-config.sh cli/src/90-main.sh cli/tests/helpers/env.bash cli/tests/wow-server-detail.bats cli/dml
git commit -m "feat(cli): wow server-detail — container-first four-state verdict

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: Rust command + api.ts types/wrapper

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (new command next to `wow_server_info` at line ~119; register in the `generate_handler![...]` list at line ~444)
- Modify: `launcher/src/lib/api.ts` (types after `ServerInfo` at line ~60; wrapper after `wowServerInfo` at line ~121)

**Interfaces:**
- Consumes: `run_json_cmd(state, args)` (lib.rs), `dml wow server-detail --json` data shape (Task 1).
- Produces: Rust command `wow_server_detail` (no args); TS `ServerVerdict`, `ContainerRow`, `SoapState`, `ServerDetail`, `wowServerDetail(): Promise<ServerDetail>` — Tasks 3-4 import these exact names.

- [ ] **Step 1: Add the Rust command in `launcher/src-tauri/src/lib.rs`**

Directly after the `wow_server_info` function:

```rust
#[tauri::command]
async fn wow_server_detail(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "server-detail".into()]).await
}
```

Then add `wow_server_detail,` to the `tauri::generate_handler![...]` list, immediately after the existing `wow_server_info,` entry.

- [ ] **Step 2: Run cargo tests**

Run: `cd launcher/src-tauri && cargo test`
Expected: 17 passed (command is a thin `run_json_cmd` wrapper — covered by the existing runner tests' pattern; compilation is the gate).

- [ ] **Step 3: Add the TS types and wrapper in `launcher/src/lib/api.ts`**

Insert after the `ServerInfo` interface:

```ts
export type ServerVerdict = "stopped" | "starting" | "online" | "soap_unreachable";
export interface ContainerRow {
  name: string;
  role: "world" | "auth" | "database";
  // Docker's state string ("running", "exited", "restarting", ...) or
  // "absent" when the container doesn't exist (e.g. after compose down).
  state: string;
  status: string;
}
export interface SoapState {
  reachable: boolean;
  auth_ok: boolean | null;
  version: string | null;
  players: number | null;
  uptime: string | null;
  mean_ms: number | null;
  median_ms: number | null;
}
export interface ServerDetail {
  verdict: ServerVerdict;
  containers: ContainerRow[];
  world_ready: boolean;
  soap: SoapState;
  ports: {
    world: string | null;
    auth: string | null;
    soap: string | null;
    db: string | null;
  };
}
```

Insert after `wowServerInfo`:

```ts
export async function wowServerDetail(): Promise<ServerDetail> {
  return await invoke("wow_server_detail");
}
```

- [ ] **Step 4: Run the launcher gates**

Run (from `launcher/`): `npm test` then `npm run check`
Expected: 19 vitest passed; svelte-check 0 errors 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts
git commit -m "feat(launcher): wow_server_detail command + ServerDetail api types

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: Home — four-state status card + expandable health panel

**Files:**
- Modify: `launcher/src/lib/pages/Home.svelte` (full replacement below)

**Interfaces:**
- Consumes: `wowServerDetail`, `ServerDetail` (Task 2); `gamesStatus`/`gamesStart`/`gamesStop`, `Terminal`, `terminal-state` (existing, unchanged).
- Produces: nothing consumed by later tasks.

- [ ] **Step 1: Replace `launcher/src/lib/pages/Home.svelte` with**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { wowServerDetail, gamesStatus, gamesStart, gamesStop, type ServerDetail } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";

  const WOW_ID = "wow-server-playerbots";
  const ROLE_LABELS: Record<string, string> = {
    world: "World server",
    auth: "Auth server",
    database: "Database",
  };

  let detail: ServerDetail | null = $state(null);
  let detailError: string | null = $state(null);
  let containerState: "running" | "stopped" | null = $state(null);
  let statusError: string | null = $state(null);
  let refreshing = $state(false);
  let expanded = $state(false);

  let busy = $state(false);
  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  async function refresh() {
    refreshing = true;
    try {
      containerState = (await gamesStatus(WOW_ID)).state;
      statusError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      statusError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      containerState = null;
    }
    try {
      detail = await wowServerDetail();
      detailError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      detailError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      detail = null;
    } finally {
      refreshing = false;
    }
  }
  onMount(refresh);

  async function act(action: "start" | "stop") {
    busy = true;
    showTerm = true;
    term = initialTermState();
    try {
      const run = action === "start" ? gamesStart : gamesStop;
      await run(WOW_ID, (e) => {
        term = applyEvent(term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      term = applyEvent(term, {
        event: "error",
        error: {
          code: err.code ?? "IPC",
          message: err.message ?? String(e),
          hint: err.hint ?? "",
        },
      });
    } finally {
      busy = false;
      await refresh();
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Home</h2>
    <button onclick={refresh} disabled={refreshing || busy}>Refresh</button>
  </header>

  {#if detailError}
    <div class="error-card"><strong>Couldn't read world status.</strong><p>{detailError}</p></div>
  {:else if detail}
    <div class="card status-card" class:warn={detail.verdict === "soap_unreachable"}>
      <div class="card-title">
        <span
          class="dot"
          class:on={detail.verdict === "online"}
          class:mid={detail.verdict === "starting"}
          class:bad={detail.verdict === "soap_unreachable"}
          class:off={detail.verdict === "stopped"}
        ></span>
        <strong>
          {#if detail.verdict === "online"}World is up
          {:else if detail.verdict === "starting"}Starting up…
          {:else if detail.verdict === "soap_unreachable"}World is running, but the launcher can't reach it
          {:else}Server is stopped{/if}
        </strong>
      </div>
      {#if detail.verdict === "online"}
        <div class="stats">
          <span>Players online: <strong>{detail.soap.players ?? "?"}</strong></span>
          <span>Uptime: <strong>{detail.soap.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{detail.soap.mean_ms ?? "?"} ms avg</strong></span>
        </div>
      {:else if detail.verdict === "starting"}
        <p class="muted">The world is still loading — this takes a couple of minutes while bots spawn.</p>
      {:else if detail.verdict === "soap_unreachable"}
        <p class="muted">
          If this persists for more than a minute, Docker's networking in the distro is likely stuck —
          restarting Docker inside dml-arch usually fixes it.
        </p>
      {:else}
        <p class="muted">Start the server below.</p>
      {/if}
    </div>
  {/if}

  <header class="bar"><h2>WoW server</h2></header>
  {#if statusError}
    <div class="error-card"><strong>Couldn't reach the DML backend.</strong><p>{statusError}</p></div>
  {:else if containerState}
    <div class="card server-card">
      <div class="row">
        <button class="expander" onclick={() => (expanded = !expanded)} aria-expanded={expanded}>
          <span class="chev">{expanded ? "▾" : "▸"}</span>
          <span class="dot" class:on={containerState === "running"} class:off={containerState !== "running"}></span>
          {WOW_ID}
        </button>
        <div>
          {#if containerState === "running"}
            <button disabled={busy} onclick={() => act("stop")}>Stop</button>
          {:else}
            <button class="primary" disabled={busy} onclick={() => act("start")}>Start</button>
          {/if}
        </div>
      </div>
      {#if expanded}
        <div class="health">
          {#if detail}
            {#each detail.containers as c (c.name)}
              <div class="hrow">
                <span class="dot" class:on={c.state === "running"} class:off={c.state !== "running"}></span>
                <span class="hname">{ROLE_LABELS[c.role] ?? c.name}</span>
                <span class="hval">{c.state === "absent" ? "not created" : c.status || c.state}</span>
              </div>
            {/each}
            {#if detail.verdict === "online"}
              <div class="hrow"><span class="hname">Version</span><span class="hval">{detail.soap.version ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Uptime</span><span class="hval">{detail.soap.uptime ?? "?"}</span></div>
              <div class="hrow"><span class="hname">Players online</span><span class="hval">{detail.soap.players ?? "?"}</span></div>
              <div class="hrow">
                <span class="hname">World update time</span>
                <span class="hval">{detail.soap.mean_ms ?? "?"} ms mean · {detail.soap.median_ms ?? "?"} ms median</span>
              </div>
            {/if}
            <div class="hrow">
              <span class="hname">Ports</span>
              <span class="hval">
                game {detail.ports.world ?? "?"} · auth {detail.ports.auth ?? "?"} · SOAP {detail.ports.soap ?? "?"} · DB {detail.ports.db ?? "?"}
              </span>
            </div>
            <div class="hrow">
              <span class="hname">SOAP</span>
              <span class="hval">
                {detail.soap.reachable ? "reachable" : "unreachable"}{detail.soap.auth_ok === false
                  ? " — authentication failing, check ~/.dml/soap.env"
                  : ""}
              </span>
            </div>
          {:else}
            <p class="muted">No health data — hit Refresh.</p>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .card.warn { border-color: #f85149; }
  .server-card { flex-direction: column; align-items: stretch; }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 16px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .expander { background: none; border: none; padding: 0; display: flex; align-items: center; gap: 8px; font-weight: 600; font-size: inherit; color: inherit; cursor: pointer; }
  .chev { color: #8b949e; width: 12px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .dot.mid { background: #d29922; }
  .dot.bad { background: #f85149; }
  .stats { display: flex; gap: 18px; flex-wrap: wrap; }
  .health { margin-top: 12px; border-top: 1px solid #30363d; padding-top: 10px; display: flex; flex-direction: column; gap: 6px; }
  .hrow { display: flex; gap: 10px; align-items: center; font-size: 14px; }
  .hname { min-width: 150px; color: #8b949e; }
  .hval { color: #c9d1d9; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
```

- [ ] **Step 2: Run the launcher gates**

Run (from `launcher/`): `npm test` then `npm run check`
Expected: 19 vitest passed; svelte-check 0 errors 0 warnings (the expander is a real `<button>` with `aria-expanded`, so no a11y warnings).

- [ ] **Step 3: Commit**

```bash
git add launcher/src/lib/pages/Home.svelte
git commit -m "feat(launcher): Home four-state status card + expandable health panel

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: Dashboard — same four-state verdict

**Files:**
- Modify: `launcher/src/lib/pages/Dashboard.svelte` (script header lines 1-28 and status-card markup lines 52-70; character-viewer code and styles below stay untouched except the two new dot colors)

**Interfaces:**
- Consumes: `wowServerDetail`, `ServerDetail` (Task 2); `wowPaperdoll`/`PaperdollData` (existing).
- Produces: nothing.

- [ ] **Step 1: Update the script block**

Replace lines 1-28 (imports through `onMount(refreshInfo);`) with:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { wowServerDetail, wowPaperdoll, type ServerDetail, type PaperdollData } from "$lib/api";
  import { qualityName, QUALITY_COLORS } from "$lib/wow";
  import CharPicker from "$lib/CharPicker.svelte";

  let detail: ServerDetail | null = $state(null);
  let infoError: string | null = $state(null);
  let loadingInfo = $state(false);

  let charName = $state("");
  let doll: PaperdollData | null = $state(null);
  let dollError: string | null = $state(null);
  let loadingDoll = $state(false);

  async function refreshInfo() {
    loadingInfo = true;
    infoError = null;
    try {
      detail = await wowServerDetail();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      infoError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loadingInfo = false;
    }
  }
  onMount(refreshInfo);
```

(`loadDoll` and the rest of the script stay exactly as they are.)

- [ ] **Step 2: Update the status card markup**

Replace the `{#if infoError} ... {/if}` status block (lines 52-70) with:

```svelte
  {#if infoError}
    <div class="error-card"><strong>Couldn't read server status.</strong><p>{infoError}</p></div>
  {:else if detail}
    <div class="card status" class:warn={detail.verdict === "soap_unreachable"}>
      <div>
        <span
          class="dot"
          class:on={detail.verdict === "online"}
          class:mid={detail.verdict === "starting"}
          class:bad={detail.verdict === "soap_unreachable"}
          class:off={detail.verdict === "stopped"}
        ></span>
        <strong>
          {#if detail.verdict === "online"}World is up
          {:else if detail.verdict === "starting"}Starting up…
          {:else if detail.verdict === "soap_unreachable"}World is running, but the launcher can't reach it
          {:else}Server is stopped{/if}
        </strong>
      </div>
      {#if detail.verdict === "online"}
        <div class="stats">
          <span>Players online: <strong>{detail.soap.players ?? "?"}</strong></span>
          <span>Uptime: <strong>{detail.soap.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{detail.soap.mean_ms ?? "?"} ms avg</strong></span>
        </div>
      {:else if detail.verdict === "starting"}
        <p class="muted">The world is still loading — this takes a couple of minutes while bots spawn.</p>
      {:else if detail.verdict === "soap_unreachable"}
        <p class="muted">
          If this persists for more than a minute, Docker's networking in the distro is likely stuck —
          restarting Docker inside dml-arch usually fixes it.
        </p>
      {:else}
        <p class="muted">Start it from the Library page.</p>
      {/if}
    </div>
  {/if}
```

- [ ] **Step 3: Add the two dot colors + warn border to the style block**

After `.dot.off { background: #6e7681; }` add:

```css
  .dot.mid { background: #d29922; }
  .dot.bad { background: #f85149; }
  .card.warn { border-color: #f85149; }
```

- [ ] **Step 4: Run the launcher gates**

Run (from `launcher/`): `npm test` then `npm run check`
Expected: 19 vitest passed; svelte-check 0 errors 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/pages/Dashboard.svelte
git commit -m "fix(launcher): Dashboard uses the four-state server verdict

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```
