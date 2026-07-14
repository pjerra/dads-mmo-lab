# DML WoW Deep Features Implementation Plan (Plan 3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Prerequisite:** Plan 1 (`docs/superpowers/plans/2026-07-14-dml-cli-json-foundation.md`) complete — this plan adds `dml wow …` subcommands to the same CLI, using its `--json` envelope + NDJSON helpers (`cli/src/10-json.sh`) and its build/test harness. Plan 2 (launcher) is independent and not required here.
>
> **Grounding:** every mechanism below is from the 2026-07-15 research facts sheet (SOAP/MySQL verified against AzerothCore + mod-playerbots source). The single unverified feature — **My Party** — is deliberately NOT built here; Task 8 is a spike that resolves it and produces the input for a future Plan 4. See memory `my-party-soap-limitation`.

**Goal:** Add read/command features for a running AzerothCore 3.3.5a + mod-playerbots server to the `dml` CLI — SOAP enablement, a serialized SOAP client, item search, in-game item mail, teleports, and a character dashboard — each a `dml wow … --json` command the GUI consumes.

**Architecture:** Two backend channels (spec §5.1): **AzerothCore SOAP** (`urn:AC` `executeCommand` over HTTP Basic to `127.0.0.1:7878`) for mutating GM actions (mail, teleport, account), and **read-only MySQL** against `acore_world` / `acore_characters` for search and dashboards. All logic that can be pure (XML envelope building, SQL building, response parsing, row→JSON shaping) is a testable bash function; the two thin exec wrappers (`curl`, `mysql`) are stubbed in tests exactly like Plan 1 stubbed `docker`.

**Tech Stack:** bash (same CLI), `curl` + `mysql` client inside `dml-arch` (both present: curl is a phase3 dep, mysql via the `mariadb`/`mysql` client — install in Task 0 if absent), bats-core for tests, a tiny stub `curl`/`mysql` on PATH for unit tests.

## Global Constraints

- Repo `C:\Users\perzi\dads-mmo-lab`, branch `feat/dml-launcher-windows`. Commit after every task. Do not run two plans/sessions concurrently on this checkout.
- All new CLI code lives in `cli/src/*.sh` (built into `cli/dml` by `cli/build.sh` — never hand-edit `cli/dml`), tests in `cli/tests/`. Reuse Plan 1's `json_ok`/`json_err`/`json_escape`/`ndjson_*` (`cli/src/10-json.sh`) and the `DML_JSON` flag. Follow the established envelope contract exactly (`cli/README.md`).
- New commands live under a `wow)` case arm with subcommands: `soap-setup`, `soap-exec`, `items search`, `mail-item`, `teleport`, `teleport-list`, `characters`, `paperdoll`. Error codes introduced: `SOAP_DISABLED`, `SOAP_AUTH`, `SOAP_FAULT`, `SOAP_UNREACHABLE`, `DB_UNREACHABLE`, `BAD_ARG`, `NOT_FOUND`.
- **SOAP security (hard):** bind SOAP to `127.0.0.1:7878` ONLY — never `0.0.0.0`, never a public port. HTTP Basic auth over plaintext = full admin console. The GM account is the existing `admin`/`admin` (security 3, RealmID -1, created by the WoW installer). Port table references saying `8086` are wrong (AC default is 7878) and are reconciled in Task 1.
- **SOAP is synchronous on the single world thread with no server-side rate limit** — the CLI MUST serialize SOAP calls via an flock lock file (`~/.dml/soap.lock`); never issue concurrent SOAP commands.
- **Mutations go through SOAP GM commands, not direct DB writes** (the worldserver owns item-GUID generation and caches character state; direct writes to a live DB desync/corrupt). MySQL is used **read-only** here (search, dashboard, `game_tele` list). The one exception (offline coordinate teleport by DB write) is explicitly deferred, not implemented.
- **Icons are not in MySQL.** Item search returns `displayid`; turning that into an icon needs client DBC extraction (`ItemDisplayInfo.dbc`) — that enrichment is out of scope for Plan 3 and noted as a follow-up. Do not fake it.
- Character/item args passed into SOAP command strings MUST be validated/escaped (names: `[A-Za-z0-9_]{1,12}`; item specs: `^[0-9]+:[0-9]+$`) before building the command — SOAP executes arbitrary console commands, so argument injection = RCE-equivalent on the server.
- Live-server steps use the machine's existing `wow-server-playerbots` install. **Read-only steps (search, dashboard, teleport-list) are safe to smoke live. Mutating steps (mail, teleport) mail to / move only a disposable test character you create — never a real character.** The server must be running for SOAP/live steps; start it via the other session's `dml games start wow-server-playerbots` if needed, or skip live smoke and note it.
- Dev-loop test command (PowerShell): `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/<file>"`.

---

### Task 0: Ensure mysql client + fixtures dir in dml-arch

**Files:** none (environment only).

**Interfaces:** Produces `mysql` (mariadb client) + `curl` on PATH inside `dml-arch` for later tasks.

- [ ] **Step 1: Ensure clients present (idempotent)**

```powershell
wsl -d dml-arch -u root -- bash -lc "pacman -S --noconfirm --needed mariadb-clients curl go-yq && mysql --version && curl --version | head -1 && yq --version"
```
Expected: prints a mariadb/mysql client version, a curl version, and a yq version (Arch's `go-yq` is mikefarah/yq v4 — the `yq` used in Task 1 for a correct YAML merge). *(No commit.)*

---

### Task 1: `dml wow soap-setup` — enable SOAP on the install (TDD)

**Files:**
- Modify: `cli/src/90-main.sh` (new `wow)` arm + `soap-setup` sub, `_wow_server_dir` helper)
- Create: `cli/tests/wow-soap-setup.bats`

**Interfaces:**
- Consumes: `json_ok`/`json_err` (Plan 1), `_scan_games`/`_resolve_compose_dir` (Plan 1).
- Produces:
  - `_wow_server_dir` → echoes the compose dir of the `wow-server-playerbots` title (via `_resolve_compose_dir`), or empty.
  - `dml wow soap-setup --json` → idempotently ensures the worldserver service in `docker-compose.override.yml` has env `AC_SOAP_ENABLED=1`, `AC_SOAP_IP=0.0.0.0`, `AC_SOAP_PORT=7878` and a `ports:` entry `127.0.0.1:7878:7878`, then reports `{"ok":true,"data":{"changed":<bool>,"restart_required":<bool>}}`. (AC binds inside the container; the host mapping is what pins it to localhost. `AC_SOAP_IP=0.0.0.0` = all *container* interfaces, still reachable only via the localhost-bound host port.)
  - Writes nothing if already configured (`changed:false`).
- **Test seam:** the override path is `$(_wow_server_dir)/docker-compose.override.yml`; tests set `DML_GAMES_DIR` to a fixture containing `wow-server-playerbots/docker-compose.yml` + a minimal `docker-compose.override.yml`, and assert the patched YAML.

- [ ] **Step 1: Write the failing test**

Create `cli/tests/wow-soap-setup.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  OVR="$DML_GAMES_DIR/wow-server-playerbots/docker-compose.override.yml"
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    restart: on-failure
    environment:
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "250"
EOF
}
teardown() { teardown_fixture; }

@test "soap-setup adds SOAP env and localhost port mapping" {
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  grep -q 'AC_SOAP_ENABLED' "$OVR"
  grep -q 'AC_SOAP_PORT' "$OVR"
  grep -q '127.0.0.1:7878:7878' "$OVR"
}

@test "soap-setup is idempotent and preserves existing worldserver env as valid YAML" {
  bash "$DML" wow soap-setup --json >/dev/null
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(grep -c 'AC_SOAP_ENABLED' "$OVR")" = "1" ]
  # YAML must remain valid AND still contain the pre-existing playerbot env
  # (guards against the duplicate-top-level-services-key bug).
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MAX_RANDOM_BOTS == "250"' "$OVR"
  yq -e '.services.ac-worldserver.environment.AC_SOAP_PORT == "7878"' "$OVR"
  # exactly one localhost SOAP port mapping after two runs
  [ "$(yq '.services.ac-worldserver.ports | length' "$OVR")" = "1" ]
}

@test "soap-setup errors NOT_FOUND when wow server absent" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  run bash "$DML" wow soap-setup --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
```

- [ ] **Step 2: Run — expect FAIL** (`UNKNOWN_COMMAND`: no `wow` arm).

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/wow-soap-setup.bats"
```

- [ ] **Step 3: Implement**

In `cli/src/90-main.sh`, add helper below `_resolve_compose_dir`:
```bash
# Compose dir of the WoW Playerbots title, or empty.
_wow_server_dir() {
    local dir="$GAMES_DIR/wow-server-playerbots"
    [[ -d "$dir" ]] || return 0
    _resolve_compose_dir "$dir/"
}
```
Add a `wow)` case arm above the `version)` arm. Start with just `soap-setup` (later tasks extend the inner case):
```bash
  wow)
    wsub="${1:-}"
    shift || true
    case "$wsub" in
      soap-setup)
        sdir="$(_wow_server_dir)"
        if [[ -z "$sdir" ]]; then
            json_err NOT_FOUND "WoW Playerbots server not installed" "Install it first, then re-run."
            exit 1
        fi
        ovr="$sdir/docker-compose.override.yml"
        [[ -f "$ovr" ]] || printf 'services:\n  ac-worldserver:\n    environment:\n' > "$ovr"
        changed=false
        if ! grep -q 'AC_SOAP_ENABLED' "$ovr"; then
            # Merge the SOAP env + a localhost-bound port into the EXISTING
            # ac-worldserver service with yq (mikefarah v4) — never a second
            # top-level `services:` block (that would be a duplicate YAML key
            # and silently drop the existing playerbot env). ports uses unique
            # so re-runs never duplicate the mapping.
            yq -i '
              .services.ac-worldserver.environment.AC_SOAP_ENABLED = "1" |
              .services.ac-worldserver.environment.AC_SOAP_IP = "0.0.0.0" |
              .services.ac-worldserver.environment.AC_SOAP_PORT = "7878" |
              .services.ac-worldserver.ports = ((.services.ac-worldserver.ports // []) + ["127.0.0.1:7878:7878"] | unique)
            ' "$ovr"
            changed=true
        fi
        json_ok "{\"changed\":$changed,\"restart_required\":$changed}"
        ;;
      *)
        json_err UNKNOWN_COMMAND "Unknown wow subcommand: $wsub" "Try: dml wow soap-setup --json"
        exit 1
        ;;
    esac
    ;;
```

- [ ] **Step 4: Run — expect PASS**, then rebuild + full suite.

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/wow-soap-setup.bats && bash build.sh && bats tests/"
```
Expected: all pass, output pristine.

- [ ] **Step 5: Reconcile the wrong SOAP port references (8086 → 7878)**

The research flagged `8086` in the port-conflict table and CLI help as the SOAP port (AC default is 7878). Fix only the SOAP-labeled occurrences in `cli/src/90-main.sh` (the `_check_port_conflicts` `_ports` array entry `"8086:WoW SOAP API …"` → `"7878:WoW SOAP API …"`, and any scan-table SOAP line). Do NOT touch 3724/8085 (auth/world are correct). Grep first:
```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && grep -n 8086 src/90-main.sh"
```
Edit each SOAP-labeled line to 7878, rebuild, re-run `bats tests/` (must stay green).

- [ ] **Step 6: Commit**

```powershell
git add cli/src/90-main.sh cli/tests/wow-soap-setup.bats cli/dml
git commit -m "feat(cli): dml wow soap-setup enables AC SOAP on 127.0.0.1:7878; reconcile SOAP port to 7878"
```

---

### Task 2: SOAP client core — `dml wow soap-exec` (TDD)

**Files:**
- Create: `cli/src/20-soap.sh`
- Modify: `cli/src/90-main.sh` (`soap-exec` sub in the `wow` arm)
- Create: `cli/tests/soap.bats`
- Modify: `cli/tests/helpers/env.bash` (add `use_curl_stub`)

**Interfaces:**
- Consumes: `json_ok`/`json_err`/`json_escape` (Plan 1).
- Produces (pure, in `20-soap.sh`):
  - `soap_envelope <command>` → prints the exact `urn:AC` SOAP XML for `executeCommand` with the command XML-escaped.
  - `soap_parse_result <xml>` → prints the text inside `<result>…</result>`; returns 2 and prints the faultstring if the body is a SOAP fault.
  - `soap_url` → `http://127.0.0.1:7878/` (override `DML_SOAP_URL`), `soap_user`/`soap_pass` (override `DML_SOAP_USER`/`DML_SOAP_PASS`, default `admin`/`admin`).
  - `soap_exec <command>` → serializes via `flock ~/.dml/soap.lock`, POSTs the envelope with `curl` Basic auth, classifies outcome: success → prints `<result>` text (exit 0); SOAP fault → exit 2; HTTP 401 → exit 3; connection refused / curl error → exit 4.
- Produces (CLI): `dml wow soap-exec "<command>" --json` → `{"ok":true,"data":{"result":"<text>"}}` or error envelopes `SOAP_FAULT`/`SOAP_AUTH`/`SOAP_UNREACHABLE`.
- **Test seam:** `use_curl_stub` puts a fake `curl` on PATH that echoes a canned response file selected by env `DML_STUB_SOAP_RESPONSE` (path) and exits with `DML_STUB_CURL_EXIT` (default 0). Tests feed canned success/fault/401 XML.

- [ ] **Step 1: Add the curl stub to the harness**

Append to `cli/tests/helpers/env.bash`:
```bash
use_curl_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/curl" <<'EOS'
#!/usr/bin/env bash
# Canned SOAP responder. Ignores all args; emits the file in DML_STUB_SOAP_RESPONSE
# to stdout and exits with DML_STUB_CURL_EXIT (default 0). For HTTP-code mode,
# if DML_STUB_HTTP is set, append it as the trailing line (callers use -w).
[[ -n "${DML_STUB_SOAP_RESPONSE:-}" ]] && cat "$DML_STUB_SOAP_RESPONSE"
[[ -n "${DML_STUB_HTTP:-}" ]] && printf '%s' "$DML_STUB_HTTP"
exit "${DML_STUB_CURL_EXIT:-0}"
EOS
  chmod +x "$STUB_BIN/curl"
  export PATH="$STUB_BIN:$PATH"
}
```
Create fixture responses `cli/tests/fixtures/soap-ok.xml`:
```xml
<?xml version="1.0" encoding="UTF-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"><SOAP-ENV:Body><ns1:executeCommandResponse xmlns:ns1="urn:AC"><result>Console command executed.
</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>
```
and `cli/tests/fixtures/soap-fault.xml`:
```xml
<?xml version="1.0" encoding="UTF-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/"><SOAP-ENV:Body><SOAP-ENV:Fault><faultcode>SOAP-ENV:Server</faultcode><faultstring>There is no such command</faultstring></SOAP-ENV:Fault></SOAP-ENV:Body></SOAP-ENV:Envelope>
```

- [ ] **Step 2: Write the failing tests**

Create `cli/tests/soap.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_curl_stub
  export HOME="$FIXTURE"   # so ~/.dml/soap.lock lands in the fixture
}
teardown() { teardown_fixture; }

@test "soap_envelope escapes the command and targets urn:AC" {
  source "$BATS_TEST_DIRNAME/../src/20-soap.sh"
  run soap_envelope 'server info & <x>'
  [[ "$output" == *"urn:AC"* ]]
  [[ "$output" == *"executeCommand"* ]]
  [[ "$output" == *"server info &amp; &lt;x&gt;"* ]]
}

@test "soap_parse_result extracts result text" {
  source "$BATS_TEST_DIRNAME/../src/20-soap.sh"
  run soap_parse_result "$(cat "$BATS_TEST_DIRNAME/fixtures/soap-ok.xml")"
  [ "$status" -eq 0 ]
  [[ "$output" == *"Console command executed."* ]]
}

@test "soap_parse_result returns 2 and faultstring on fault" {
  source "$BATS_TEST_DIRNAME/../src/20-soap.sh"
  run soap_parse_result "$(cat "$BATS_TEST_DIRNAME/fixtures/soap-fault.xml")"
  [ "$status" -eq 2 ]
  [[ "$output" == *"There is no such command"* ]]
}

@test "wow soap-exec returns result envelope on success" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow soap-exec "server info" --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.result')" != "null" ]
}

@test "wow soap-exec maps fault to SOAP_FAULT" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow soap-exec "bogus" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
}

@test "wow soap-exec maps curl connection failure to SOAP_UNREACHABLE" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7   # curl: couldn't connect
  run bash "$DML" wow soap-exec "server info" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_UNREACHABLE" ]
}
```

- [ ] **Step 3: Run — expect FAIL.**

- [ ] **Step 4: Implement `cli/src/20-soap.sh`**

```bash
# ---------------------------------------------------------------------------
# AzerothCore SOAP client. Mutating GM commands go through here.
# SOAP is synchronous on the single world thread — every call is serialized
# under an flock so the CLI never issues concurrent commands.
# ---------------------------------------------------------------------------
soap_url()  { echo "${DML_SOAP_URL:-http://127.0.0.1:7878/}"; }
soap_user() { echo "${DML_SOAP_USER:-admin}"; }
soap_pass() { echo "${DML_SOAP_PASS:-admin}"; }

# XML-escape stdin argument.
_xml_escape() {
    local s="${1-}"
    s=${s//&/&amp;}
    s=${s//</&lt;}
    s=${s//>/&gt;}
    printf '%s' "$s"
}

soap_envelope() {
    local cmd; cmd="$(_xml_escape "$1")"
    cat <<EOF
<?xml version="1.0" encoding="utf-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://schemas.xmlsoap.org/soap/envelope/" xmlns:ns1="urn:AC">
  <SOAP-ENV:Body>
    <ns1:executeCommand><command>$cmd</command></ns1:executeCommand>
  </SOAP-ENV:Body>
</SOAP-ENV:Envelope>
EOF
}

# Prints <result> text (exit 0), or faultstring (exit 2) if a fault body.
soap_parse_result() {
    local xml="$1"
    if [[ "$xml" == *"<faultstring>"* ]]; then
        local f="${xml#*<faultstring>}"; f="${f%%</faultstring>*}"
        printf '%s' "$f"
        return 2
    fi
    if [[ "$xml" == *"<result>"* ]]; then
        local r="${xml#*<result>}"; r="${r%%</result>*}"
        printf '%s' "$r"
        return 0
    fi
    printf '%s' "$xml"
    return 2
}

# soap_exec <command> -> prints result text; exit 0 ok / 2 fault / 3 auth / 4 unreachable
soap_exec() {
    local cmd="$1" body resp code lockdir="$HOME/.dml"
    mkdir -p "$lockdir"
    body="$(soap_envelope "$cmd")"
    exec {lockfd}>>"$lockdir/soap.lock"
    flock "$lockfd"
    resp="$(printf '%s' "$body" | curl -s -w '\n%{http_code}' \
        --max-time 30 \
        -u "$(soap_user):$(soap_pass)" \
        -H 'Content-Type: application/xml' \
        --data-binary @- "$(soap_url)" 2>/dev/null)"
    code=$?
    flock -u "$lockfd"
    exec {lockfd}>&-
    if [[ $code -ne 0 ]]; then
        return 4
    fi
    local http="${resp##*$'\n'}" xml="${resp%$'\n'*}"
    if [[ "$http" == "401" ]]; then return 3; fi
    local out rc
    out="$(soap_parse_result "$xml")"; rc=$?
    printf '%s' "$out"
    return "$rc"
}
```
Add the `soap-exec` sub inside the `wow` arm's inner case (above `*)`):
```bash
      soap-exec)
        cmd="${1:?Usage: dml wow soap-exec \"<command>\"}"
        out="$(soap_exec "$cmd")"; rc=$?
        case "$rc" in
          0) json_ok "{\"result\":\"$(json_escape "$out")\"}" ;;
          2) json_err SOAP_FAULT "$out" "The worldserver rejected the command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check admin account / gmlevel 3." ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach SOAP at $(soap_url)" "Is the worldserver running with SOAP enabled? Run: dml wow soap-setup" ; exit 1 ;;
        esac
        ;;
```
Ensure `20-soap.sh` sorts between json and main in the build (it does: `10-json.sh` < `20-soap.sh` < `90-main.sh`).

- [ ] **Step 5: Run — expect PASS**, rebuild, full suite green.

- [ ] **Step 6: Live smoke (read-only, safe)** — only if the server is running with SOAP enabled:

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && ./dml wow soap-setup --json && ./dml wow soap-exec 'server info' --json"
```
Expected: soap-setup reports changed (first time; then restart the server via the other session or `docker compose up -d ac-worldserver` in the server dir), and `soap-exec 'server info'` returns a `result` string with the AC build/uptime. If the server isn't running, expect `SOAP_UNREACHABLE` — note it and rely on the stub tests.

- [ ] **Step 7: Commit**

```powershell
git add cli/src/20-soap.sh cli/src/90-main.sh cli/tests/soap.bats cli/tests/helpers/env.bash cli/tests/fixtures/soap-ok.xml cli/tests/fixtures/soap-fault.xml cli/dml
git commit -m "feat(cli): serialized AzerothCore SOAP client + dml wow soap-exec"
```

---

### Task 3: MySQL read helper + `dml wow items search` (TDD)

**Files:**
- Create: `cli/src/30-db.sh`
- Modify: `cli/src/90-main.sh` (`items` sub)
- Create: `cli/tests/wow-items.bats`
- Modify: `cli/tests/helpers/env.bash` (add `use_mysql_stub`)

**Interfaces:**
- Produces (pure, `30-db.sh`):
  - `db_world_query <sql>` / `db_chars_query <sql>` → run SQL via `mysql` against the `ac-database` container (`docker exec ac-database mysql -N -B -uroot -p<pw> <schema>`), tab-separated rows on stdout; exit 4 if unreachable. Password from `DML_DB_ROOT_PASSWORD` (default `password`, matching dml-start.sh).
  - `sql_escape <s>` → escapes `'` and `\` for safe single-quoted literals.
  - `build_item_search_sql <name> <quality|-> <minlvl|-> <maxlvl|-> <limit>` → pure; returns the exact SELECT (columns `entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid`).
- Produces (CLI): `dml wow items search --name <s> [--quality N] [--min-level N] [--max-level N] [--limit N] --json` → `{"ok":true,"data":{"items":[{entry,name,quality,item_level,required_level,class,subclass,inventory_type,displayid}]}}`. Icons intentionally absent (see constraints).
- **Test seam:** `use_mysql_stub` puts a fake `docker` on PATH whose `exec ac-database mysql …` branch echoes the file in `DML_STUB_DB_ROWS` (TSV) and exits `DML_STUB_DB_EXIT`. (Reuses the game stub's `docker`; extend it, don't duplicate — see step.)

- [ ] **Step 1: Extend the harness with a mysql-capable docker stub**

In `cli/tests/helpers/env.bash`, add:
```bash
use_mysql_stub() {
  STUB_BIN="${STUB_BIN:-$FIXTURE/bin}"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/docker" <<'EOS'
#!/usr/bin/env bash
# Minimal docker stub for `docker exec ac-database mysql …`.
if [[ "${1:-}" == "exec" ]]; then
  [[ -n "${DML_STUB_DB_ROWS:-}" ]] && cat "$DML_STUB_DB_ROWS"
  exit "${DML_STUB_DB_EXIT:-0}"
fi
if [[ "${1:-}" == "info" ]]; then exit 0; fi
exit 0
EOS
  chmod +x "$STUB_BIN/docker"
  export PATH="$STUB_BIN:$PATH"
}
```
Create `cli/tests/fixtures/items.tsv` (tab-separated; use real tabs):
```
6948	Hearthstone	1	1	1	15	1	0	6418
19019	Thunderfury	5	80	60	2	7	13	30606
```

- [ ] **Step 2: Write failing tests**

Create `cli/tests/wow-items.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
}
teardown() { teardown_fixture; }

@test "build_item_search_sql filters by name, quality and level" {
  source "$BATS_TEST_DIRNAME/../src/30-db.sh"
  run build_item_search_sql "thunder" 5 60 80 25
  [[ "$output" == *"item_template"* ]]
  [[ "$output" == *"name LIKE '%thunder%'"* ]]
  [[ "$output" == *"Quality = 5"* ]]
  [[ "$output" == *"RequiredLevel >= 60"* ]]
  [[ "$output" == *"RequiredLevel <= 80"* ]]
  [[ "$output" == *"LIMIT 25"* ]]
}

@test "build_item_search_sql omits absent filters" {
  source "$BATS_TEST_DIRNAME/../src/30-db.sh"
  run build_item_search_sql "sword" - - - 50
  [[ "$output" != *"Quality ="* ]]
  [[ "$output" != *"RequiredLevel >="* ]]
}

@test "sql_escape neutralizes quotes" {
  source "$BATS_TEST_DIRNAME/../src/30-db.sh"
  run sql_escape "O'Brien"
  [ "$output" = "O\\'Brien" ]
}

@test "items search returns JSON rows from the db" {
  export DML_STUB_DB_ROWS="$BATS_TEST_DIRNAME/fixtures/items.tsv"
  run bash "$DML" wow items search --name thunder --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.items | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.items[1].name')" = "Thunderfury" ]
  [ "$(echo "$output" | jq -r '.data.items[1].quality')" = "5" ]
  [ "$(echo "$output" | jq -r '.data.items[1].displayid')" = "30606" ]
}

@test "items search maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_ROWS="$BATS_TEST_DIRNAME/fixtures/items.tsv"
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow items search --name x --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}
```

- [ ] **Step 3: Run — expect FAIL.**

- [ ] **Step 4: Implement `cli/src/30-db.sh`**

```bash
# ---------------------------------------------------------------------------
# Read-only MySQL access to the AzerothCore DBs via the ac-database container.
# Search/dashboard only. Mutations go through SOAP, never direct writes.
# ---------------------------------------------------------------------------
_db_pw() { echo "${DML_DB_ROOT_PASSWORD:-password}"; }

_db_query() {  # _db_query <schema> <sql>
    docker exec -i ac-database mysql -N -B -uroot -p"$(_db_pw)" "$1" -e "$2" 2>/dev/null
}
db_world_query() { _db_query acore_world "$1"; }
db_chars_query() { _db_query acore_characters "$1"; }

sql_escape() {
    local s="${1-}"
    s=${s//\\/\\\\}
    s=${s//\'/\\\'}
    printf '%s' "$s"
}

# All args required; "-" means "omit this filter".
build_item_search_sql() {
    local name="$1" quality="$2" minl="$3" maxl="$4" limit="$5"
    local where="1=1"
    [[ -n "$name" ]] && where+=" AND name LIKE '%$(sql_escape "$name")%'"
    [[ "$quality" != "-" ]] && where+=" AND Quality = $quality"
    [[ "$minl" != "-" ]] && where+=" AND RequiredLevel >= $minl"
    [[ "$maxl" != "-" ]] && where+=" AND RequiredLevel <= $maxl"
    printf 'SELECT entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid FROM item_template WHERE %s ORDER BY RequiredLevel,name LIMIT %s;' "$where" "$limit"
}

# Reads TSV rows on stdin, emits a JSON array of item objects.
_items_rows_to_json() {
    local first=1 out='['
    local entry name q il rl cls sub inv disp
    while IFS=$'\t' read -r entry name q il rl cls sub inv disp; do
        [[ -z "$entry" ]] && continue
        [[ $first -eq 0 ]] && out+=','
        out+="{\"entry\":$entry,\"name\":\"$(json_escape "$name")\",\"quality\":$q,\"item_level\":$il,\"required_level\":$rl,\"class\":$cls,\"subclass\":$sub,\"inventory_type\":$inv,\"displayid\":$disp}"
        first=0
    done
    out+=']'
    printf '%s' "$out"
}
```
Add the `items` sub inside the `wow` arm:
```bash
      items)
        isub="${1:-}"; shift || true
        case "$isub" in
          search)
            name=""; quality="-"; minl="-"; maxl="-"; limit=50
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --name) name="$2"; shift 2 ;;
                --quality) quality="$2"; shift 2 ;;
                --min-level) minl="$2"; shift 2 ;;
                --max-level) maxl="$2"; shift 2 ;;
                --limit) limit="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" "See: dml wow items search --name <text>"; exit 1 ;;
              esac
            done
            sql="$(build_item_search_sql "$name" "$quality" "$minl" "$maxl" "$limit")"
            rows="$(db_world_query "$sql")" || {
              json_err DB_UNREACHABLE "Could not query the item database" "Is ac-database running? Try: dml games status wow-server-playerbots"; exit 1; }
            json_ok "{\"items\":$(printf '%s' "$rows" | _items_rows_to_json)}"
            ;;
          *) json_err BAD_ARG "Unknown items subcommand: $isub" "Try: dml wow items search --name <text>"; exit 1 ;;
        esac
        ;;
```
Note: numeric flags (`--quality` etc.) are compared/inlined as integers into SQL; validate they are integers to avoid injection — add near the top of `search`:
```bash
            for v in "$quality" "$minl" "$maxl" "$limit"; do
              [[ "$v" == "-" || "$v" =~ ^[0-9]+$ ]] || { json_err BAD_ARG "Numeric flag expected, got: $v" ""; exit 1; }
            done
```
(Place this right before building `sql`, after arg parsing.)

- [ ] **Step 5: Run — expect PASS**, rebuild, full suite.

- [ ] **Step 6: Live smoke (read-only, safe)** if server running:
```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && ./dml wow items search --name 'Thunderfury' --json | head -c 400"
```
Expected: JSON with Thunderfury (entry 19019). If DB down, `DB_UNREACHABLE` — note it.

- [ ] **Step 7: Commit**

```powershell
git add cli/src/30-db.sh cli/src/90-main.sh cli/tests/wow-items.bats cli/tests/helpers/env.bash cli/tests/fixtures/items.tsv cli/dml
git commit -m "feat(cli): read-only MySQL helper + dml wow items search (item_template)"
```

---

### Task 4: `dml wow mail-item` via SOAP `.send items` (TDD)

**Files:**
- Modify: `cli/src/90-main.sh` (`mail-item` sub + `_valid_charname` / `_valid_item_spec` validators)
- Create: `cli/tests/wow-mail.bats`

**Interfaces:**
- Consumes: `soap_exec` (Task 2), validators, `json_ok`/`json_err`.
- Produces: `dml wow mail-item --to <char> --items <id:count>[,<id:count>…] [--subject S] [--body B] --json`. Validates the char name (`^[A-Za-z0-9_]{1,12}$`) and each item spec (`^[0-9]+:[0-9]+$`), rejects >12 attachments (`BAD_ARG`), builds `send items "<char>" "<subject>" "<body>" id:count …`, runs it over SOAP, returns `{"ok":true,"data":{"sent":true,"to":"<char>","attachments":N}}` or the SOAP error envelope.
- **Test seam:** reuse `use_curl_stub` — assert the fixture success path returns sent:true, and that validation rejects bad names/specs BEFORE any curl call (so no server needed).

- [ ] **Step 1: Write failing tests**

Create `cli/tests/wow-mail.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_curl_stub
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "mail-item sends via SOAP and reports attachment count" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow mail-item --to Testchar --items 6948:1,19019:1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.sent')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.attachments')" = "2" ]
}

@test "mail-item rejects an invalid character name before calling SOAP" {
  run bash "$DML" wow mail-item --to 'bad name!' --items 6948:1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "mail-item rejects a malformed item spec" {
  run bash "$DML" wow mail-item --to Testchar --items 6948x1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "mail-item rejects more than 12 attachments" {
  spec="1:1,2:1,3:1,4:1,5:1,6:1,7:1,8:1,9:1,10:1,11:1,12:1,13:1"
  run bash "$DML" wow mail-item --to Testchar --items "$spec" --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement** — add validators near the other `wow` helpers in `90-main.sh`:
```bash
_valid_charname() { [[ "$1" =~ ^[A-Za-z0-9_]{1,12}$ ]]; }
_valid_item_spec() { [[ "$1" =~ ^[0-9]+:[0-9]+$ ]]; }
```
Add the `mail-item` sub in the `wow` arm:
```bash
      mail-item)
        to=""; items=""; subject="Dad's MMO Lab"; body="Enjoy!"
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --to) to="$2"; shift 2 ;;
            --items) items="$2"; shift 2 ;;
            --subject) subject="$2"; shift 2 ;;
            --body) body="$2"; shift 2 ;;
            *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        _valid_charname "$to" || { json_err BAD_ARG "Invalid character name: $to" "1-12 letters/digits/underscore."; exit 1; }
        IFS=',' read -ra specs <<< "$items"
        [[ "${#specs[@]}" -ge 1 && "${#specs[@]}" -le 12 ]] || { json_err BAD_ARG "Provide 1-12 items as id:count[,id:count…]" ""; exit 1; }
        attach=""
        for s in "${specs[@]}"; do
          _valid_item_spec "$s" || { json_err BAD_ARG "Malformed item spec: $s" "Use itemid:count"; exit 1; }
          attach+=" $s"
        done
        # subject/body are placed inside double quotes in the console command;
        # strip any double quotes to keep the command well-formed.
        subject="${subject//\"/}"; body="${body//\"/}"
        cmd="send items \"$to\" \"$subject\" \"$body\"$attach"
        out="$(soap_exec "$cmd")"; rc=$?
        case "$rc" in
          0) json_ok "{\"sent\":true,\"to\":\"$(json_escape "$to")\",\"attachments\":${#specs[@]}}" ;;
          2) json_err SOAP_FAULT "$out" "The server rejected the mail command." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach the server" "Run: dml wow soap-setup, then start the server." ; exit 1 ;;
        esac
        ;;
```

- [ ] **Step 4: Run — expect PASS**, rebuild, full suite.

- [ ] **Step 5: Live smoke (MUTATING — disposable char only).** Only if the server is up and you have created a throwaway character (e.g. `Mailtest`). Mail it a Hearthstone and confirm in-game / via a follow-up `.pinfo`. This also exercises the historical `.send items` crash (AC issue #2695) against the bundled build:
```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && ./dml wow mail-item --to Mailtest --items 6948:1 --json"
```
Expected: `sent:true`. If no disposable char exists, SKIP and note it — do NOT mail a real character.

- [ ] **Step 6: Commit**

```powershell
git add cli/src/90-main.sh cli/tests/wow-mail.bats cli/dml
git commit -m "feat(cli): dml wow mail-item (validated SOAP .send items, <=12 attachments)"
```

---

### Task 5: Teleport — `dml wow teleport-list` (MySQL) + `dml wow teleport` (SOAP) (TDD)

**Files:**
- Modify: `cli/src/90-main.sh` (`teleport-list`, `teleport` subs)
- Create: `cli/tests/wow-teleport.bats`

**Interfaces:**
- Consumes: `db_world_query` (Task 3), `soap_exec` + validators (Tasks 2/4).
- Produces:
  - `dml wow teleport-list [--search <s>] --json` → reads `acore_world.game_tele` (`name,position_x,position_y,position_z,map`), returns `{"ok":true,"data":{"locations":[{name,x,y,z,map}]}}`.
  - `dml wow teleport --char <name> --to <location> --json` → validates the char name, runs SOAP `teleport name "<char>" "<location>"` (offline-capable named teleport), returns `{"ok":true,"data":{"teleported":true,"char":"<name>","to":"<location>"}}` or SOAP error. **Arbitrary coordinates are NOT implemented** (needs an offline DB write; deferred — see constraints); attempting `--coords` returns `BAD_ARG` pointing at the deferral.
- **Test seam:** `teleport-list` via `use_mysql_stub` with a `game_tele` TSV fixture; `teleport` via `use_curl_stub`.

- [ ] **Step 1: Write failing tests**

Create `cli/tests/wow-teleport.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  export HOME="$FIXTURE"
}
teardown() { teardown_fixture; }

@test "teleport-list returns rows from game_tele" {
  use_mysql_stub
  printf 'Stormwind\t-8960.0\t516.0\t96.3\t0\nOrgrimmar\t1633.0\t-4373.0\t31.3\t1\n' > "$FIXTURE/tele.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/tele.tsv"
  run bash "$DML" wow teleport-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.locations | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.locations[0].name')" = "Stormwind" ]
  [ "$(echo "$output" | jq -r '.data.locations[0].map')" = "0" ]
}

@test "teleport sends named SOAP command" {
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow teleport --char Testchar --to Stormwind --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.teleported')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.to')" = "Stormwind" ]
}

@test "teleport rejects a bad char name" {
  use_curl_stub
  run bash "$DML" wow teleport --char 'x y' --to Stormwind --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "teleport --coords is rejected as deferred" {
  use_curl_stub
  run bash "$DML" wow teleport --char Testchar --coords 1,2,3,0 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
```

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement** the two subs in the `wow` arm:
```bash
      teleport-list)
        search=""
        [[ "${1:-}" == "--search" ]] && { search="$2"; shift 2; }
        where="1=1"
        [[ -n "$search" ]] && where="name LIKE '%$(sql_escape "$search")%'"
        sql="SELECT name,position_x,position_y,position_z,map FROM game_tele WHERE $where ORDER BY name LIMIT 500;"
        rows="$(db_world_query "$sql")" || { json_err DB_UNREACHABLE "Could not query teleport locations" ""; exit 1; }
        first=1; out='['
        while IFS=$'\t' read -r nm x y z mp; do
          [[ -z "$nm" ]] && continue
          [[ $first -eq 0 ]] && out+=','
          out+="{\"name\":\"$(json_escape "$nm")\",\"x\":$x,\"y\":$y,\"z\":$z,\"map\":$mp}"
          first=0
        done <<< "$rows"
        out+=']'
        json_ok "{\"locations\":$out}"
        ;;
      teleport)
        char=""; to=""
        while [[ $# -gt 0 ]]; do
          case "$1" in
            --char) char="$2"; shift 2 ;;
            --to) to="$2"; shift 2 ;;
            --coords) json_err BAD_ARG "Coordinate teleport is not available yet" "Use --to <named location>; coords need an offline DB path (planned)."; exit 1 ;;
            *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
          esac
        done
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        [[ -n "$to" ]] || { json_err BAD_ARG "Missing --to <location>" "List with: dml wow teleport-list --json"; exit 1; }
        to_clean="${to//\"/}"
        out="$(soap_exec "teleport name \"$char\" \"$to_clean\"")"; rc=$?
        case "$rc" in
          0) json_ok "{\"teleported\":true,\"char\":\"$(json_escape "$char")\",\"to\":\"$(json_escape "$to")\"}" ;;
          2) json_err SOAP_FAULT "$out" "Unknown location? See dml wow teleport-list." ; exit 1 ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "" ; exit 1 ;;
          *) json_err SOAP_UNREACHABLE "Could not reach the server" "" ; exit 1 ;;
        esac
        ;;
```

- [ ] **Step 4: Run — expect PASS**, rebuild, full suite.

- [ ] **Step 5: Live smoke** — `teleport-list` is read-only/safe; `teleport` MUTATES (disposable char only, and the char should be offline for a clean named teleport). Note results or skip.

- [ ] **Step 6: Commit**

```powershell
git add cli/src/90-main.sh cli/tests/wow-teleport.bats cli/dml
git commit -m "feat(cli): dml wow teleport-list (game_tele) + named SOAP teleport (coords deferred)"
```

---

### Task 6: Character dashboard — `dml wow characters` + `dml wow paperdoll` (TDD)

**Files:**
- Modify: `cli/src/90-main.sh` (`characters`, `paperdoll` subs)
- Create: `cli/tests/wow-dashboard.bats`

**Interfaces:**
- Consumes: `db_chars_query`/`db_world_query` (Task 3).
- Produces:
  - `dml wow characters --account <name> --json` → joins `acore_characters.characters` filtered by the account id resolved from `acore_auth.account` (via a `db_auth_query` added here) → `{"ok":true,"data":{"characters":[{guid,name,level,class,race,gender,gold}]}}` (`gold = FLOOR(money/10000)`).
  - `dml wow paperdoll --char <name> --json` → the character's equipped items: `characters` (level/class/money) + `character_inventory ⋈ item_instance ⋈ item_template` for `bag=0 AND slot BETWEEN 0 AND 18` → `{"ok":true,"data":{"name","level","class","gold","equipped":[{slot,entry,name,quality,item_level,displayid}]}}`.
- Note the live-state caveat (constraints): DB rows are last-saved for online chars. Include a `"note"` field `"last_saved"` in the paperdoll data so the GUI can warn. (Live-accurate `.pinfo` via SOAP is a future refinement, not built here.)
- **Test seam:** `use_mysql_stub` — but two different queries (auth lookup then characters) both hit the same stub `docker exec`. Make the stub return the file in `DML_STUB_DB_ROWS`; for the two-query `characters` path, set `DML_STUB_DB_ROWS` to a fixture whose FIRST line is the account id and rely on the command issuing the auth query first. To keep tests deterministic, implement `characters` to resolve the account id with a SEPARATE stub var `DML_STUB_ACCOUNT_ID` when set (test-only shortcut documented in code).

- [ ] **Step 1: Write failing tests**

Create `cli/tests/wow-dashboard.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
}
teardown() { teardown_fixture; }

@test "characters lists an account's chars with gold in gold-units" {
  export DML_STUB_ACCOUNT_ID=1
  printf '4\tPriesttest\t80\t5\t1\t0\t123456\n' > "$FIXTURE/chars.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/chars.tsv"
  run bash "$DML" wow characters --account admin --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.characters[0].name')" = "Priesttest" ]
  [ "$(echo "$output" | jq -r '.data.characters[0].level')" = "80" ]
  [ "$(echo "$output" | jq -r '.data.characters[0].gold')" = "12" ]
}

@test "paperdoll returns equipped items with note last_saved" {
  printf 'Priesttest\t80\t5\t123456\t0\t6948\tHearthstone\t1\t1\t6418\n' > "$FIXTURE/pd.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/pd.tsv"
  run bash "$DML" wow paperdoll --char Priesttest --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.note')" = "last_saved" ]
  [ "$(echo "$output" | jq -r '.data.equipped[0].name')" = "Hearthstone" ]
}

@test "paperdoll rejects a bad char name" {
  run bash "$DML" wow paperdoll --char 'no good' --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}
```
(The paperdoll fixture row packs: name, level, class, money, then per-item slot, entry, name, quality, displayid — the implementation's join returns character columns repeated per row; for the single-item fixture the reducer reads character fields from the first row.)

- [ ] **Step 2: Run — expect FAIL.**

- [ ] **Step 3: Implement.** Add an auth-schema query helper next to the others in `30-db.sh`:
```bash
db_auth_query() { _db_query acore_auth "$1"; }
```
Add the two subs in the `wow` arm. For `characters`, resolve the account id (test shortcut via `DML_STUB_ACCOUNT_ID`), then query characters:
```bash
      characters)
        acct=""
        [[ "${1:-}" == "--account" ]] && { acct="$2"; shift 2; }
        [[ -n "$acct" ]] || { json_err BAD_ARG "Missing --account <name>" ""; exit 1; }
        if [[ -n "${DML_STUB_ACCOUNT_ID:-}" ]]; then
          aid="$DML_STUB_ACCOUNT_ID"
        else
          aid="$(db_auth_query "SELECT id FROM account WHERE username='$(sql_escape "$acct")' LIMIT 1;")" \
            || { json_err DB_UNREACHABLE "Could not reach the auth database" ""; exit 1; }
        fi
        [[ -n "$aid" ]] || { json_err NOT_FOUND "No such account: $acct" ""; exit 1; }
        rows="$(db_chars_query "SELECT guid,name,level,class,race,gender,money FROM characters WHERE account=$aid ORDER BY level DESC;")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        first=1; out='['
        while IFS=$'\t' read -r guid nm lvl cls race gen money; do
          [[ -z "$guid" ]] && continue
          [[ $first -eq 0 ]] && out+=','
          out+="{\"guid\":$guid,\"name\":\"$(json_escape "$nm")\",\"level\":$lvl,\"class\":$cls,\"race\":$race,\"gender\":$gen,\"gold\":$((money/10000))}"
          first=0
        done <<< "$rows"
        out+=']'
        json_ok "{\"characters\":$out}"
        ;;
      paperdoll)
        char=""
        [[ "${1:-}" == "--char" ]] && { char="$2"; shift 2; }
        _valid_charname "$char" || { json_err BAD_ARG "Invalid character name: $char" ""; exit 1; }
        sql="SELECT c.name,c.level,c.class,c.money,ci.slot,it.entry,it.name,it.Quality,it.displayid
             FROM characters c
             JOIN character_inventory ci ON ci.guid=c.guid AND ci.bag=0 AND ci.slot BETWEEN 0 AND 18
             JOIN item_instance ii ON ii.guid=ci.item
             JOIN acore_world.item_template it ON it.entry=ii.itemEntry
             WHERE c.name='$(sql_escape "$char")' ORDER BY ci.slot;"
        rows="$(db_chars_query "$sql")" || { json_err DB_UNREACHABLE "Could not reach the characters database" ""; exit 1; }
        [[ -n "$rows" ]] || { json_err NOT_FOUND "No such character or no equipped items: $char" ""; exit 1; }
        cname=""; clevel=0; cclass=0; cmoney=0
        first=1; eq='['
        while IFS=$'\t' read -r nm lvl cls money slot entry iname q disp; do
          [[ -z "$nm" ]] && continue
          cname="$nm"; clevel="$lvl"; cclass="$cls"; cmoney="$money"
          [[ $first -eq 0 ]] && eq+=','
          eq+="{\"slot\":$slot,\"entry\":$entry,\"name\":\"$(json_escape "$iname")\",\"quality\":$q,\"displayid\":$disp}"
          first=0
        done <<< "$rows"
        eq+=']'
        json_ok "{\"name\":\"$(json_escape "$cname")\",\"level\":$clevel,\"class\":$cclass,\"gold\":$((cmoney/10000)),\"note\":\"last_saved\",\"equipped\":$eq}"
        ;;
```
(The `paperdoll` fixture in the test omits the join columns for brevity — adjust the fixture TSV column order to match this SELECT: name, level, class, money, slot, entry, item name, quality, displayid. Update the test fixture in Step 1 if the column order differs, keeping the assertions.)

- [ ] **Step 4: Run — expect PASS**, rebuild, full suite. (If the paperdoll fixture column order needs adjusting to match the SELECT, fix the fixture and re-run — the assertion targets `.equipped[0].name` and `.note`.)

- [ ] **Step 5: Live smoke (read-only, safe):**
```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && ./dml wow characters --account admin --json"
```
Expected: the account's characters (empty array if none created). Read-only, safe on the live DB.

- [ ] **Step 6: Commit**

```powershell
git add cli/src/30-db.sh cli/src/90-main.sh cli/tests/wow-dashboard.bats cli/dml
git commit -m "feat(cli): dml wow characters + paperdoll (read-only dashboard from acore_characters)"
```

---

### Task 7: `dml wow` contract docs + full-suite gate

**Files:**
- Modify: `cli/README.md` (document the `wow` subcommands + error codes + SOAP/MySQL split)

**Interfaces:** Consumes everything above. Produces the written contract Plan 4 (My Party) and the launcher build against.

- [ ] **Step 1: Append a `## wow subcommands` section** to `cli/README.md` documenting: `soap-setup`, `soap-exec`, `items search`, `mail-item`, `teleport`/`teleport-list`, `characters`/`paperdoll` — each with its flags, JSON shape, and backend (SOAP vs MySQL). State the security posture (SOAP bound to 127.0.0.1, serialized), that mutations use SOAP not direct writes, and that icons need client DBC enrichment (not provided here). Note coordinate teleport is deferred.

- [ ] **Step 2: Full suite gate**
```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"
```
Expected: every suite green, output pristine.

- [ ] **Step 3: Commit**
```powershell
git add cli/README.md
git commit -m "docs(cli): document dml wow SOAP/MySQL subcommands and security posture"
```

---

### Task 8: SPIKE — resolve the My Party mechanism (investigation, produces Plan 4 input)

> This task builds NO feature. Its deliverable is a committed decision document that settles how a bot joins a specific player's party, so Plan 4 (My Party) can be written against a verified mechanism instead of a guess. See memory `my-party-soap-limitation`. Exit criterion: the document answers all four questions below with evidence.

**Files:**
- Create: `docs/superpowers/specs/2026-07-15-my-party-spike-findings.md`

- [ ] **Step 1: The decisive SOAP test.** With SOAP enabled and the server running, fire the bot-add command over SOAP and capture the exact response:
```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && ./dml wow soap-exec 'playerbots bot add Somebotname' --json; ./dml wow soap-exec 'playerbot bot add Somebotname' --json"
```
Record the exact `result`/`faultstring`. Expectation (from source research): a fault or a message like *"You may only add bots from an active session."* If so, **SOAP-only My Party is confirmed dead**.

- [ ] **Step 2: Inspect The Lab's mechanism.** Re-extract the AppImage if the earlier extraction is gone (`~/games`/scratch), then grep the binary + bundled assets:
```powershell
wsl -d Ubuntu -u labtest -- bash -lc "cd ~/squashfs-root 2>/dev/null && strings -n 6 usr/bin/the-lab | grep -iE 'playerbots? bot add|:8888|CommandServerPort|urn:AC|executeCommand|group_member|SavedVariables|\.lua' | sort -u | head -60"
```
Determine which of the five mechanisms The Lab uses (addon-relay via SavedVariables `.lua`, BotAutologin, TCP :8888, SOAP staging + in-game trigger, or direct `group_member` DB), and whether it requires the human logged in during party-building.

- [ ] **Step 3: Confirm the module surface.** Verify against the exact mod-playerbots fork/branch DML installs (`github.com/mod-playerbots/azerothcore-wotlk` Playerbot branch + `mod-playerbots/mod-playerbots`): the `bot add` session guard, the `MaxAddedBots`/`AllowAccountBots`/`BotAutologin` conf keys, the account-ownership rule (own/linked/guild/addclass), and whether the TCP command server accepts anything beyond `state`.

- [ ] **Step 4: Write the findings doc** answering, with evidence/citations:
  1. **Mechanism:** exactly how a specific bot joins a specific player's party without a human typing — the chosen path, and the fallbacks.
  2. **Login requirement:** must the player's character be logged in during/after party-building? If yes, what triggers the join (addon event, autologin)?
  3. **Account/ownership setup:** what account provisioning the pre-generated bots need (own/linked/guild/addclass), and how to satisfy it from the CLI.
  4. **Ambient random-bot interaction:** whether `AC_AI_PLAYERBOT_*RANDOM_BOTS` counts / `flush_random_bots` must change when a curated party is active.
  End with a **recommended Plan 4 task breakdown** for My Party built on the verified mechanism.

- [ ] **Step 5: Commit**
```powershell
git add docs/superpowers/specs/2026-07-15-my-party-spike-findings.md
git commit -m "docs(spike): resolve My Party bot-join mechanism (input for Plan 4)"
```
