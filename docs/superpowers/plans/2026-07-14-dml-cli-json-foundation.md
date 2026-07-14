# DML CLI JSON Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Promote the `dml` CLI out of Install-DML.ps1 into versioned files under `cli/`, and add the machine-readable `--json` / NDJSON contract the DML Launcher GUI will consume.

**Architecture:** The CLI stays a single deployable bash file (`cli/dml`) built by concatenating `cli/src/*.sh` in glob order. Human text output remains the default (the existing C# tray parses it); `--json` opts into envelopes and NDJSON event streams. A new `games` namespace carries the JSON-first commands; legacy commands are untouched.

**Tech Stack:** bash 4+ (already required — associative arrays), bats-core for tests (runs inside the `dml-arch` WSL distro), jq for test assertions only (already a phase3 dependency; never a runtime dependency of `dml` itself).

## Global Constraints

- Repo on Windows at `C:\Users\perzi\dads-mmo-lab`; the same tree inside WSL is `/mnt/c/Users/perzi/dads-mmo-lab`.
- The CLI executes inside WSL2 distro `dml-arch` as user `dml`; games live in `GAMES_DIR="$HOME/games"` (= `/home/dml/games`).
- `set -euo pipefail` is preserved in the built CLI (spec §6; existing code depends on `${1:?...}` semantics).
- Do **not** modify `guides/DML-Windows/Install-DML.ps1` in this plan. Its embedded CLI (v2.6.0, lines 836–1633) remains the bootstrap; `cli/` becomes the canonical source that the launcher dev-installs over it. (Installer sync is a later plan. Note for that plan: bumping the bash `VERSION` requires updating `$ExpectedCliVersion = 'dml v2.6.0'` at Install-DML.ps1:813.)
- Default (non-`--json`) output of existing commands must not change, with one deliberate exception documented in Task 6: `start`/`stop`/new `restart` learn to use a game dir's `dml-start.sh` hook when present (this fixes restarts re-triggering `ac-db-import`).
- JSON success envelope: `{"ok":true,"data":<object>}`. Error envelope: `{"ok":false,"error":{"code":"<SCREAMING_SNAKE>","message":"...","hint":"..."}}`, always followed by `exit 1`.
- NDJSON progress events (one JSON object per line, only in `--json` mode on long commands): `{"event":"section_start","name":"..."}`, `{"event":"line","level":"info|warn|error","text":"..."}`, `{"event":"section_end","name":"...","status":"ok|error"}`, terminal `{"event":"done","data":{...}}` (exit 0) or `{"event":"error","error":{...}}` (exit 1). (`{"event":"pct","value":N}` is reserved for the installer plan; not emitted here.)
- Error codes used in this plan: `UNKNOWN_COMMAND`, `NOT_FOUND`, `NO_COMPOSE`, `DOCKER_DOWN`, `START_FAILED`, `STOP_FAILED`.
- Commit after every task; work on branch `feat/dml-launcher-windows`.
- All bats/bash commands below run **from Windows PowerShell** via the pattern:
  `wsl -d dml-arch -u dml -- bash -lc "<command>"` — shown per step.

---

### Task 0: One-time test tooling in dml-arch

**Files:**
- None (environment only).

**Interfaces:**
- Produces: `bats` and `jq` available on PATH inside `dml-arch` for every later task's test steps.

- [ ] **Step 1: Install bats-core (jq is already a phase3 dep, install is idempotent)**

Run:
```powershell
wsl -d dml-arch -u root -- bash -lc "pacman -S --noconfirm --needed bats jq && bats --version && jq --version"
```
Expected: prints `Bats 1.x.x` and `jq-1.x`.

*(No commit — nothing in the repo changed.)*

---

### Task 1: Extract the CLI verbatim into `cli/src/` with a build script

**Files:**
- Create: `cli/src/00-head.sh` (Install-DML.ps1 lines 837–841, verbatim)
- Create: `cli/src/90-main.sh` (Install-DML.ps1 lines 842–1632, verbatim)
- Create: `cli/build.sh`
- Create: `cli/dml` (built artifact, committed)
- Create: `cli/dev-install.ps1`

**Interfaces:**
- Produces: `cli/dml` — byte-identical (modulo CRLF→LF) to the CLI the installer deploys; `cli/build.sh` — rebuilds `cli/dml` from `cli/src/*.sh`; `cli/dev-install.ps1` — installs `cli/dml` to `/usr/local/bin/dml` in `dml-arch`.
- Consumes: nothing.

- [ ] **Step 1: Extract the two source files from Install-DML.ps1**

The embedded CLI is a single-quoted here-string — no unescaping needed, only CRLF→LF. PowerShell (from repo root; `Get-Content` uses 0-based indexing, so lines 837–841 are indices 836–840):

```powershell
$all = Get-Content guides\DML-Windows\Install-DML.ps1
New-Item -ItemType Directory -Force cli\src | Out-Null
[IO.File]::WriteAllText("$PWD\cli\src\00-head.sh", (($all[836..840] -join "`n") + "`n"))
[IO.File]::WriteAllText("$PWD\cli\src\90-main.sh", (($all[841..1631] -join "`n") + "`n"))
```

Verify the head looks right:
```powershell
Get-Content cli\src\00-head.sh
```
Expected exactly:
```bash
#!/usr/bin/env bash
set -euo pipefail

VERSION="2.6.0"
GAMES_DIR="$HOME/games"
```
And `Get-Content cli\src\90-main.sh -TotalCount 1` prints the first helper (a `_require_docker`-related line or blank line — it must NOT contain a shebang), while `Get-Content cli\src\90-main.sh -Tail 1` prints `esac`.

- [ ] **Step 2: Write the build script**

Create `cli/build.sh`:
```bash
#!/usr/bin/env bash
# Builds the single-file dml CLI from cli/src/*.sh (glob order = numeric prefixes).
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
cat src/*.sh > dml
chmod +x dml
bash -n dml   # parse check
echo "built cli/dml ($(wc -l < dml) lines)"
```

- [ ] **Step 3: Build and verify against the live installed CLI**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && diff /usr/local/bin/dml dml && echo IDENTICAL"
```
Expected: `built cli/dml (…)` then `IDENTICAL`. If diff reports differences, the extraction offsets are wrong — fix before continuing (the only acceptable difference is none).

- [ ] **Step 4: Write the dev-install script**

Create `cli/dev-install.ps1`:
```powershell
# Installs the built cli/dml into the dml-arch distro (dev loop).
param([string]$Distro = 'dml-arch')
$ErrorActionPreference = 'Stop'
$repoWsl = '/mnt/c/Users/perzi/dads-mmo-lab'
wsl -d $Distro -u root -- bash -lc "install -m 0755 $repoWsl/cli/dml /usr/local/bin/dml && dml version"
```

Run it:
```powershell
powershell -File cli\dev-install.ps1
```
Expected output: `dml v2.6.0`.

- [ ] **Step 5: Commit**

```powershell
git add cli
git commit -m "feat(cli): extract dml CLI v2.6.0 from Install-DML.ps1 into versioned cli/ with build + dev-install"
```

---

### Task 2: JSON emit helpers (`cli/src/10-json.sh`) — TDD

**Files:**
- Create: `cli/src/10-json.sh`
- Create: `cli/tests/json.bats`

**Interfaces:**
- Produces (bash functions, all print to stdout):
  - `json_escape <string>` → string with `\` `"` newline/CR/tab escaped, other control chars stripped
  - `json_ok <raw-json>` → `{"ok":true,"data":<raw-json>}` (caller passes well-formed JSON; default `null`)
  - `json_err <CODE> <message> [hint]` → error envelope (does **not** exit; caller exits)
  - `ndjson_event <raw-json-fields>` → `{<raw-json-fields>}` one line
  - `ndjson_line <level> <text>`, `ndjson_section_start <name>`, `ndjson_section_end <name> <status>`, `ndjson_done <raw-json>`, `ndjson_error <CODE> <message> [hint]`
  - Global `DML_JSON` (0/1): consumed by later tasks; helpers themselves are mode-agnostic.
- Consumes: nothing from other tasks (pure bash).

- [ ] **Step 1: Write the failing test**

Create `cli/tests/json.bats`:
```bash
#!/usr/bin/env bats
# Contract tests for the JSON emit helpers.

setup() {
  source "$BATS_TEST_DIRNAME/../src/10-json.sh"
}

@test "json_ok wraps data in success envelope" {
  run json_ok '{"version":"3.0.0"}'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.version')" = "3.0.0" ]
}

@test "json_ok defaults data to null" {
  run json_ok
  [ "$(echo "$output" | jq -c '.data')" = "null" ]
}

@test "json_err builds error envelope with code/message/hint" {
  run json_err DOCKER_DOWN 'Docker is not running' 'Try: sudo systemctl start docker'
  [ "$(echo "$output" | jq -r '.ok')" = "false" ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DOCKER_DOWN" ]
  [ "$(echo "$output" | jq -r '.error.message')" = "Docker is not running" ]
  [ "$(echo "$output" | jq -r '.error.hint')" = "Try: sudo systemctl start docker" ]
}

@test "json_escape handles quotes backslashes and newlines" {
  run json_escape $'he said "hi\\" and\nleft'
  [ "$output" = 'he said \"hi\\\" and\nleft' ]
}

@test "ndjson_line emits a single valid JSON line" {
  run ndjson_line info 'Starting wow...'
  [ "$(echo "$output" | jq -r '.event')" = "line" ]
  [ "$(echo "$output" | jq -r '.level')" = "info" ]
  [ "$(echo "$output" | jq -r '.text')" = "Starting wow..." ]
}

@test "ndjson section and done events are valid JSON" {
  run ndjson_section_start start
  [ "$(echo "$output" | jq -r '.event')" = "section_start" ]
  run ndjson_section_end start ok
  [ "$(echo "$output" | jq -r '.status')" = "ok" ]
  run ndjson_done '{"state":"running"}'
  [ "$(echo "$output" | jq -r '.event')" = "done" ]
  [ "$(echo "$output" | jq -r '.data.state')" = "running" ]
  run ndjson_error NOT_FOUND 'no such title' ''
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/json.bats"
```
Expected: FAIL — `No such file or directory` sourcing `src/10-json.sh`.

- [ ] **Step 3: Write the implementation**

Create `cli/src/10-json.sh`:
```bash
# ---------------------------------------------------------------------------
# JSON / NDJSON emit helpers (machine-readable mode for the DML Launcher).
# Pure bash — no jq at runtime. DML_JSON is set by the arg parser (see main).
# ---------------------------------------------------------------------------
DML_JSON="${DML_JSON:-0}"

json_escape() {
    local s="${1-}"
    s=${s//\\/\\\\}
    s=${s//\"/\\\"}
    s=${s//$'\n'/\\n}
    s=${s//$'\r'/\\r}
    s=${s//$'\t'/\\t}
    # Strip remaining ASCII control chars JSON forbids unescaped
    printf '%s' "$s" | tr -d '\000-\010\013\014\016-\037'
}

json_ok() {
    local data="${1:-null}"
    printf '{"ok":true,"data":%s}\n' "$data"
}

json_err() {
    local code="$1" msg="$2" hint="${3:-}"
    printf '{"ok":false,"error":{"code":"%s","message":"%s","hint":"%s"}}\n' \
        "$code" "$(json_escape "$msg")" "$(json_escape "$hint")"
}

ndjson_event() {
    printf '{%s}\n' "$1"
}

ndjson_line() {
    local level="$1" text="$2"
    ndjson_event "\"event\":\"line\",\"level\":\"$level\",\"text\":\"$(json_escape "$text")\""
}

ndjson_section_start() {
    ndjson_event "\"event\":\"section_start\",\"name\":\"$(json_escape "$1")\""
}

ndjson_section_end() {
    ndjson_event "\"event\":\"section_end\",\"name\":\"$(json_escape "$1")\",\"status\":\"$2\""
}

ndjson_done() {
    local data="${1:-null}"
    ndjson_event "\"event\":\"done\",\"data\":$data"
}

ndjson_error() {
    local code="$1" msg="$2" hint="${3:-}"
    ndjson_event "\"event\":\"error\",\"error\":{\"code\":\"$code\",\"message\":\"$(json_escape "$msg")\",\"hint\":\"$(json_escape "$hint")\"}"
}
```

- [ ] **Step 4: Run tests to verify they pass, and rebuild**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/json.bats && bash build.sh"
```
Expected: all tests PASS (`6 tests, 0 failures`), then `built cli/dml (…)`. Note: `cli/dml` now contains the helpers between head and main — that is expected from this task on (the Task 1 `diff /usr/local/bin/dml` check no longer applies).

- [ ] **Step 5: Commit**

```powershell
git add cli/src/10-json.sh cli/tests/json.bats cli/dml
git commit -m "feat(cli): add JSON envelope + NDJSON event helpers with bats contract tests"
```

---

### Task 3: `--json` flag parsing, version bump, JSON `version` + unknown-command envelope

**Files:**
- Modify: `cli/src/00-head.sh` (VERSION line)
- Modify: `cli/src/90-main.sh` (arg parsing before dispatch; `version` and `*` case arms)
- Create: `cli/tests/cli-core.bats`

**Interfaces:**
- Consumes: `json_ok`, `json_err` from Task 2.
- Produces: global `DML_JSON=1` when `--json` appears anywhere in argv (flag is stripped before dispatch — later tasks rely on this); `dml version --json` → `{"ok":true,"data":{"version":"3.0.0"}}`; unknown command in JSON mode → `UNKNOWN_COMMAND` envelope, exit 1.

- [ ] **Step 1: Write the failing test**

Create `cli/tests/cli-core.bats`:
```bash
#!/usr/bin/env bats
# End-to-end contract tests against the BUILT cli/dml artifact.

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
}

@test "version --json returns success envelope with semver" {
  run bash "$DML" version --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.version')" = "3.0.0" ]
}

@test "version without --json keeps legacy text output" {
  run bash "$DML" version
  [ "$status" -eq 0 ]
  [ "$output" = "dml v3.0.0" ]
}

@test "unknown command in json mode returns UNKNOWN_COMMAND envelope and exit 1" {
  run bash "$DML" frobnicate --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "UNKNOWN_COMMAND" ]
}

@test "--json may appear before the command too" {
  run bash "$DML" --json version
  [ "$(echo "$output" | jq -r '.data.version')" = "3.0.0" ]
}
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/cli-core.bats"
```
Expected: FAIL — version prints `dml v2.6.0`, `--json` lands in `$1` and is treated as a command.

- [ ] **Step 3: Implement**

In `cli/src/00-head.sh`, change:
```bash
VERSION="2.6.0"
```
to:
```bash
VERSION="3.0.0"
```

In `cli/src/90-main.sh`, find the dispatch block:
```bash
cmd="${1:-help}"
shift || true

case "$cmd" in
```
and insert the flag parser **above** it, so the block becomes:
```bash
# --- machine-readable mode: strip --json from argv anywhere -----------------
DML_JSON=0
_args=()
for _a in "$@"; do
    if [[ "$_a" == "--json" ]]; then DML_JSON=1; else _args+=("$_a"); fi
done
set -- ${_args[@]+"${_args[@]}"}
unset _args _a
# ---------------------------------------------------------------------------

cmd="${1:-help}"
shift || true

case "$cmd" in
```
(The `${_args[@]+…}` guard keeps `set -u` happy when argv becomes empty.)

Find the `version)` arm:
```bash
  version)
    echo "dml v$VERSION"
    ;;
```
and replace with:
```bash
  version)
    if [[ "$DML_JSON" == 1 ]]; then
        json_ok "{\"version\":\"$VERSION\"}"
    else
        echo "dml v$VERSION"
    fi
    ;;
```

Find the fallback arm:
```bash
  *)
    echo "[dml] Unknown command: $cmd" >&2
    echo "Run 'dml help' for usage." >&2
    exit 1
    ;;
```
and replace with:
```bash
  *)
    if [[ "$DML_JSON" == 1 ]]; then
        json_err UNKNOWN_COMMAND "Unknown command: $cmd" "Run 'dml help' for usage."
    else
        echo "[dml] Unknown command: $cmd" >&2
        echo "Run 'dml help' for usage." >&2
    fi
    exit 1
    ;;
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/cli-core.bats && bats tests/json.bats"
```
Expected: all PASS.

- [ ] **Step 5: Commit**

```powershell
git add cli/src/00-head.sh cli/src/90-main.sh cli/tests/cli-core.bats cli/dml
git commit -m "feat(cli): v3.0.0 with --json flag, JSON version envelope, UNKNOWN_COMMAND error"
```

---

### Task 4: Test fixtures + `dml games list --json`

**Files:**
- Modify: `cli/src/00-head.sh` (GAMES_DIR override for tests)
- Modify: `cli/src/90-main.sh` (new `_scan_games` helper + `games` case arm)
- Create: `cli/tests/helpers/env.bash` (fixture + docker stub harness)
- Create: `cli/tests/games-list.bats`

**Interfaces:**
- Consumes: `json_ok` (Task 2), `DML_JSON` (Task 3), `_has_compose` / `_compose_running` (existing, extracted in Task 1).
- Produces:
  - `GAMES_DIR="${DML_GAMES_DIR:-$HOME/games}"` — tests point `DML_GAMES_DIR` at a fixture.
  - `_scan_games` — prints one `id<TAB>compose_dir` line per installed title (compose_dir empty when only `install.sh` marks it). Task 5/6 reuse it.
  - `dml games list --json` → `{"ok":true,"data":{"games":[{"id":"<dir>","path":"<abs>","running":<bool>}]}}` (sorted by glob order, deduped like legacy `list`).
- **Docker stub contract** (used by every later test): a fake `docker` on PATH that reads env var `DML_STUB_RUNNING` (newline-separated compose-file paths considered running) and `DML_STUB_DOCKER_DOWN=1` to fail `docker info`.

- [ ] **Step 1: Write the test harness**

Create `cli/tests/helpers/env.bash`:
```bash
# Shared test harness: fixture games dir + docker stub.
make_fixture() {
  FIXTURE="$(mktemp -d)"
  export DML_GAMES_DIR="$FIXTURE/games"
  mkdir -p "$DML_GAMES_DIR"
}

add_game() {  # add_game <id> compose|install|empty|nested
  local id="$1" kind="$2" dir="$DML_GAMES_DIR/$1"
  mkdir -p "$dir"
  case "$kind" in
    compose) touch "$dir/docker-compose.yml" ;;
    install) touch "$dir/install.sh" ;;
    nested)  mkdir -p "$dir/sub" && touch "$dir/sub/compose.yml" ;;
    empty)   : ;;
  esac
}

use_docker_stub() {
  STUB_BIN="$FIXTURE/bin"
  mkdir -p "$STUB_BIN"
  cat > "$STUB_BIN/docker" <<'EOS'
#!/usr/bin/env bash
if [[ "${1:-}" == "info" ]]; then
  [[ "${DML_STUB_DOCKER_DOWN:-0}" == 1 ]] && exit 1 || exit 0
fi
if [[ "${1:-}" == "compose" ]]; then
  # find -f <file>
  file=""
  args=("$@")
  for i in "${!args[@]}"; do
    [[ "${args[$i]}" == "-f" ]] && file="${args[$((i+1))]}"
  done
  rest="${args[*]}"
  if [[ "$rest" == *"ps --status running -q"* ]]; then
    if [[ -n "$file" ]] && grep -qxF "$file" <<< "${DML_STUB_RUNNING:-}"; then
      echo "stub-container-id"
    fi
    exit 0
  fi
  if [[ "$rest" == *"up -d"* || "$rest" == *"down"* ]]; then
    echo "stub compose: $rest"
    exit "${DML_STUB_COMPOSE_EXIT:-0}"
  fi
  exit 0
fi
if [[ "${1:-}" == "ps" ]]; then exit 0; fi
exit 0
EOS
  chmod +x "$STUB_BIN/docker"
  export PATH="$STUB_BIN:$PATH"
}

teardown_fixture() {
  [[ -n "${FIXTURE:-}" ]] && rm -rf "$FIXTURE"
}
```

- [ ] **Step 2: Write the failing test**

Create `cli/tests/games-list.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
}

teardown() { teardown_fixture; }

@test "games list --json lists compose, install-only and nested titles" {
  add_game wow-server-playerbots compose
  add_game runescape install
  add_game tortoise nested
  add_game junk empty
  run bash "$DML" games list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.games | length')" = "3" ]
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="wow-server-playerbots") | .running')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.games[] | select(.id=="junk") | .id' )" = "" ]
}

@test "games list --json marks running titles via compose ps" {
  add_game wow-server-playerbots compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow-server-playerbots/docker-compose.yml"
  run bash "$DML" games list --json
  [ "$(echo "$output" | jq -r '.data.games[0].running')" = "true" ]
}

@test "games list --json with no games dir returns empty array" {
  rm -rf "$DML_GAMES_DIR"
  run bash "$DML" games list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -c '.data.games')" = "[]" ]
}

@test "legacy list output is unchanged" {
  add_game wow-server-playerbots compose
  run bash "$DML" list
  [ "$status" -eq 0 ]
  [ "$output" = "wow-server-playerbots" ]
}
```

- [ ] **Step 3: Run test to verify it fails**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/games-list.bats"
```
Expected: FAIL — `games` is an unknown command (and legacy `list` ignores `DML_GAMES_DIR`).

- [ ] **Step 4: Implement**

In `cli/src/00-head.sh`, change:
```bash
GAMES_DIR="$HOME/games"
```
to:
```bash
GAMES_DIR="${DML_GAMES_DIR:-$HOME/games}"
```

In `cli/src/90-main.sh`, add below the `_compose_running` helper (keep existing helpers untouched):
```bash
# Prints one "id<TAB>compose_dir" line per installed title (compose_dir may be
# empty for install.sh-only titles). Mirrors the legacy list/status scan rules.
_scan_games() {
    [[ -d "$GAMES_DIR" ]] || return 0
    local dir subdir title
    declare -A _scan_seen=()
    for dir in "$GAMES_DIR"/*/; do
        [[ -d "$dir" ]] || continue
        title=$(basename "$dir")
        [[ -n "${_scan_seen[$title]:-}" ]] && continue
        if _has_compose "$dir"; then
            printf '%s\t%s\n' "$title" "${dir%/}"
            _scan_seen["$title"]=1
        elif [[ -f "$dir/install.sh" ]]; then
            printf '%s\t%s\n' "$title" ""
            _scan_seen["$title"]=1
        else
            for subdir in "$dir"*/; do
                [[ -d "$subdir" ]] || continue
                if _has_compose "$subdir"; then
                    printf '%s\t%s\n' "$title" "${subdir%/}"
                    _scan_seen["$title"]=1
                    break
                elif [[ -f "$subdir/install.sh" ]]; then
                    printf '%s\t%s\n' "$title" ""
                    _scan_seen["$title"]=1
                    break
                fi
            done
        fi
    done
}
```

Add a new case arm directly above the `version)` arm:
```bash
  games)
    sub="${1:-list}"
    shift || true
    case "$sub" in
      list)
        first=1
        out='{"games":['
        while IFS=$'\t' read -r gid gdir; do
            [[ -z "$gid" ]] && continue
            running=false
            if [[ -n "$gdir" ]] && [[ "$(_compose_running "$gdir")" -gt 0 ]]; then
                running=true
            fi
            [[ $first -eq 0 ]] && out+=','
            out+="{\"id\":\"$(json_escape "$gid")\",\"path\":\"$(json_escape "${gdir:-$GAMES_DIR/$gid}")\",\"running\":$running}"
            first=0
        done < <(_scan_games)
        out+=']}'
        json_ok "$out"
        ;;
      *)
        json_err UNKNOWN_COMMAND "Unknown games subcommand: $sub" "Try: dml games list --json"
        exit 1
        ;;
    esac
    ;;
```

- [ ] **Step 5: Run tests to verify they pass**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/games-list.bats && bats tests/cli-core.bats"
```
Expected: all PASS. (`legacy list output is unchanged` passes because legacy `list` reads the same `GAMES_DIR` variable, which the fixture overrides — its human formatting is untouched.)

- [ ] **Step 6: Commit**

```powershell
git add cli/src cli/tests cli/dml
git commit -m "feat(cli): dml games list --json with fixture + docker-stub test harness"
```

---

### Task 5: `dml games status <id> --json`

**Files:**
- Modify: `cli/src/90-main.sh` (add `status` under the `games` arm)
- Create: `cli/tests/games-status.bats`

**Interfaces:**
- Consumes: `_scan_games`, docker stub harness (Task 4); `json_ok`/`json_err` (Task 2).
- Produces: `dml games status <id> --json` → `{"ok":true,"data":{"id":"...","state":"running"|"stopped"}}`; missing title → `NOT_FOUND` envelope + exit 1. Task 6 reuses the same resolution helper `_resolve_compose_dir <title-dir>`.

- [ ] **Step 1: Write the failing test**

Create `cli/tests/games-status.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
}

teardown() { teardown_fixture; }

@test "games status reports stopped" {
  add_game wow-server-playerbots compose
  run bash "$DML" games status wow-server-playerbots --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.state')" = "stopped" ]
  [ "$(echo "$output" | jq -r '.data.id')" = "wow-server-playerbots" ]
}

@test "games status reports running" {
  add_game wow-server-playerbots compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow-server-playerbots/docker-compose.yml"
  run bash "$DML" games status wow-server-playerbots --json
  [ "$(echo "$output" | jq -r '.data.state')" = "running" ]
}

@test "games status for unknown title returns NOT_FOUND exit 1" {
  run bash "$DML" games status nope --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/games-status.bats"
```
Expected: FAIL — `UNKNOWN_COMMAND` (no `status` sub-arm yet).

- [ ] **Step 3: Implement**

In `cli/src/90-main.sh`, add below `_scan_games`:
```bash
# Echoes the compose dir for a title dir (itself, or first subdir with a
# compose file). Echoes nothing if none found. Mirrors legacy start/stop.
_resolve_compose_dir() {
    local dir="$1" subdir
    if _has_compose "$dir"; then echo "$dir"; return 0; fi
    for subdir in "$dir"*/; do
        if [[ -d "$subdir" ]] && _has_compose "$subdir"; then
            echo "${subdir%/}"
            return 0
        fi
    done
    return 0
}
```

Inside the `games` arm's inner case, add above the `*)` sub-arm:
```bash
      status)
        gid="${1:?Usage: dml games status <title> --json}"
        dir="$GAMES_DIR/$gid"
        if [[ ! -d "$dir" ]]; then
            json_err NOT_FOUND "Title not found: $gid" "Run: dml games list --json"
            exit 1
        fi
        compose_dir="$(_resolve_compose_dir "$dir/")"
        state=stopped
        if [[ -n "$compose_dir" ]] && [[ "$(_compose_running "$compose_dir")" -gt 0 ]]; then
            state=running
        fi
        json_ok "{\"id\":\"$(json_escape "$gid")\",\"state\":\"$state\"}"
        ;;
```

- [ ] **Step 4: Run tests to verify they pass**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/games-status.bats && bats tests/games-list.bats"
```
Expected: all PASS.

- [ ] **Step 5: Commit**

```powershell
git add cli/src/90-main.sh cli/tests/games-status.bats cli/dml
git commit -m "feat(cli): dml games status --json with NOT_FOUND envelope"
```

---

### Task 6: `dml games start|stop|restart <id> --json` with NDJSON streaming + dml-start.sh hook

**Files:**
- Modify: `cli/src/90-main.sh` (add `start`/`stop`/`restart` sub-arms + `_stream_cmd` helper)
- Create: `cli/tests/games-start-stop.bats`

**Interfaces:**
- Consumes: `_resolve_compose_dir` (Task 5), `ndjson_*` (Task 2), docker stub (Task 4), `_require_docker`/`_check_port_conflicts` (existing).
- Produces:
  - `dml games start <id> --json` → NDJSON stream ending in `{"event":"done","data":{"id":"...","state":"running"}}` exit 0, or `{"event":"error",...}` exit 1. Codes: `NOT_FOUND`, `NO_COMPOSE`, `DOCKER_DOWN`, `START_FAILED`.
  - Same for `stop` (`STOP_FAILED`, final state `"stopped"`) and `restart`.
  - **Hook contract:** if `<compose_dir>/dml-start.sh` exists and is executable, `start`/`restart` invoke it (`bash dml-start.sh start|restart`) instead of `docker compose up -d`, streaming its stdout+stderr as `line` events. This fixes restarts re-running `ac-db-import` (the hook uses `docker start` for existing containers) and applies in text mode too.

- [ ] **Step 1: Write the failing test**

Create `cli/tests/games-start-stop.bats`:
```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_docker_stub
}

teardown() { teardown_fixture; }

@test "games start streams NDJSON and ends with done running" {
  add_game wow compose
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"  # post-start state
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  first="$(echo "$output" | head -1)"
  last="$(echo "$output" | tail -1)"
  [ "$(echo "$first" | jq -r '.event')" = "section_start" ]
  [ "$(echo "$last" | jq -r '.event')" = "done" ]
  [ "$(echo "$last" | jq -r '.data.state')" = "running" ]
  # every line is valid JSON
  echo "$output" | while IFS= read -r l; do echo "$l" | jq -e . >/dev/null; done
}

@test "games start uses dml-start.sh hook when present and streams its output" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] staged start: mode=$1"
exit 0
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games start wow --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'staged start: mode=start'
}

@test "games start fails with START_FAILED when hook exits nonzero" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] ERROR: db not healthy" >&2
exit 1
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  run bash "$DML" games start wow --json
  [ "$status" -eq 1 ]
  last="$(echo "$output" | tail -1)"
  [ "$(echo "$last" | jq -r '.event')" = "error" ]
  [ "$(echo "$last" | jq -r '.error.code')" = "START_FAILED" ]
}

@test "games start with docker down returns DOCKER_DOWN" {
  add_game wow compose
  export DML_STUB_DOCKER_DOWN=1
  run bash "$DML" games start wow --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | tail -1 | jq -r '.error.code')" = "DOCKER_DOWN" ]
}

@test "games stop ends with done stopped" {
  add_game wow compose
  run bash "$DML" games stop wow --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | tail -1 | jq -r '.data.state')" = "stopped" ]
}

@test "games restart passes restart mode to hook" {
  add_game wow compose
  cat > "$DML_GAMES_DIR/wow/dml-start.sh" <<'EOS'
#!/usr/bin/env bash
echo "[dml] staged start: mode=$1"
EOS
  chmod +x "$DML_GAMES_DIR/wow/dml-start.sh"
  export DML_STUB_RUNNING="$DML_GAMES_DIR/wow/docker-compose.yml"
  run bash "$DML" games restart wow --json
  [ "$status" -eq 0 ]
  echo "$output" | grep -q 'staged start: mode=restart'
}
```

- [ ] **Step 2: Run test to verify it fails**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/games-start-stop.bats"
```
Expected: FAIL — `UNKNOWN_COMMAND` sub-arm errors.

- [ ] **Step 3: Implement**

In `cli/src/90-main.sh`, add below `_resolve_compose_dir`:
```bash
# Runs a command, streaming its combined output. In JSON mode each line
# becomes an NDJSON "line" event; in text mode lines pass through unchanged.
# Returns the command's exit code (set -o pipefail is active globally).
_stream_cmd() {
    if [[ "$DML_JSON" == 1 ]]; then
        "$@" 2>&1 | while IFS= read -r _l; do ndjson_line info "$_l"; done
    else
        "$@" 2>&1
    fi
}

# Shared guard for games start/stop/restart. Sets gid, dir, compose_dir or
# emits the right error (respecting DML_JSON) and exits 1.
_games_resolve_or_fail() {
    gid="${1:?Usage: dml games <start|stop|restart> <title>}"
    dir="$GAMES_DIR/$gid"
    if [[ ! -d "$dir" ]]; then
        if [[ "$DML_JSON" == 1 ]]; then ndjson_error NOT_FOUND "Title not found: $gid" "Run: dml games list --json"
        else echo "[dml] ERROR: Title not found: $gid" >&2; fi
        exit 1
    fi
    compose_dir="$(_resolve_compose_dir "$dir/")"
    if [[ -z "$compose_dir" ]]; then
        if [[ "$DML_JSON" == 1 ]]; then ndjson_error NO_COMPOSE "No compose file found in $gid or its subdirectories." "Reinstall the title or check $dir"
        else echo "[dml] ERROR: No compose file found in $gid or its subdirectories." >&2; fi
        exit 1
    fi
    if ! docker info &>/dev/null; then
        if [[ "$DML_JSON" == 1 ]]; then ndjson_error DOCKER_DOWN "Docker is not running." "Try: sudo systemctl start docker (or dml doctor)"
        else echo "[dml] Docker is not running. Try: sudo systemctl start docker" >&2; fi
        exit 1
    fi
}

# Start or restart with hook support. $1 = title, $2 = start|restart
_games_start_impl() {
    local mode="$2"
    _games_resolve_or_fail "$1"
    [[ "$DML_JSON" == 1 ]] && ndjson_section_start "$mode"
    cd "$compose_dir"
    _check_port_conflicts > >(if [[ "$DML_JSON" == 1 ]]; then while IFS= read -r _l; do ndjson_line warn "$_l"; done; else cat; fi)
    local rc=0
    if [[ -x "./dml-start.sh" ]]; then
        _stream_cmd bash ./dml-start.sh "$mode" || rc=$?
    else
        if [[ "$mode" == "restart" ]]; then
            _stream_cmd docker compose down || rc=$?
        fi
        [[ $rc -eq 0 ]] && { _stream_cmd docker compose up -d || rc=$?; }
    fi
    if [[ $rc -ne 0 ]]; then
        if [[ "$DML_JSON" == 1 ]]; then
            ndjson_section_end "$mode" error
            ndjson_error START_FAILED "$gid failed to $mode (exit $rc)" "Check logs: docker compose logs, or dml doctor"
        else
            echo "[dml] ERROR: $gid failed to $mode (exit $rc)" >&2
        fi
        exit 1
    fi
    if [[ "$DML_JSON" == 1 ]]; then
        ndjson_section_end "$mode" ok
        ndjson_done "{\"id\":\"$(json_escape "$gid")\",\"state\":\"running\"}"
    else
        echo "[dml] $gid started"
    fi
}
```

Inside the `games` arm's inner case, add above the `*)` sub-arm:
```bash
      start)
        _games_start_impl "${1:-}" start
        ;;
      restart)
        _games_start_impl "${1:-}" restart
        ;;
      stop)
        _games_resolve_or_fail "${1:-}"
        [[ "$DML_JSON" == 1 ]] && ndjson_section_start stop
        cd "$compose_dir"
        rc=0
        _stream_cmd docker compose down || rc=$?
        if [[ $rc -ne 0 ]]; then
            if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end stop error
                ndjson_error STOP_FAILED "$gid failed to stop (exit $rc)" "Try: dml kill $gid"
            else
                echo "[dml] ERROR: $gid failed to stop (exit $rc)" >&2
            fi
            exit 1
        fi
        if [[ "$DML_JSON" == 1 ]]; then
            ndjson_section_end stop ok
            ndjson_done "{\"id\":\"$(json_escape "$gid")\",\"state\":\"stopped\"}"
        else
            echo "[dml] $gid stopped"
        fi
        ;;
```

- [ ] **Step 4: Run the full suite**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"
```
Expected: all files, all tests PASS.

- [ ] **Step 5: Commit**

```powershell
git add cli/src/90-main.sh cli/tests/games-start-stop.bats cli/dml
git commit -m "feat(cli): games start/stop/restart with NDJSON streaming and dml-start.sh hook support"
```

---

### Task 7: Windows-side smoke test + contract README

**Files:**
- Create: `cli/tests/windows-smoke.ps1`
- Create: `cli/README.md`

**Interfaces:**
- Consumes: everything above; `cli/dev-install.ps1` (Task 1).
- Produces: the exact call pattern the Tauri shell will use (`wsl.exe -d dml-arch -u dml -- dml … --json`), proven from PowerShell; the written contract later plans (launcher, WoW features) build against.

- [ ] **Step 1: Write the smoke test**

Create `cli/tests/windows-smoke.ps1`:
```powershell
# Smoke-tests the Windows->WSL->dml --json path the DML Launcher will use.
$ErrorActionPreference = 'Stop'
& "$PSScriptRoot\..\dev-install.ps1"

$raw = wsl -d dml-arch -u dml -- dml version --json
$v = ($raw | ConvertFrom-Json)
if (-not $v.ok) { throw "version --json not ok: $raw" }
if ($v.data.version -ne '3.0.0') { throw "unexpected version: $($v.data.version)" }

$raw = wsl -d dml-arch -u dml -- dml games list --json
$g = ($raw | ConvertFrom-Json)
if (-not $g.ok) { throw "games list --json not ok: $raw" }
Write-Host "SMOKE OK — $($g.data.games.Count) game(s):" ($g.data.games.id -join ', ')
```

- [ ] **Step 2: Run it**

```powershell
powershell -File cli\tests\windows-smoke.ps1
```
Expected: `SMOKE OK — N game(s): wow-server-playerbots` (N and titles depend on the machine's real installs; on this machine `wow-server-playerbots` exists).

- [ ] **Step 3: Write the contract README**

Create `cli/README.md`:
```markdown
# dml CLI

Canonical source for the `dml` CLI that runs inside the `dml-arch` WSL distro
(and, on Linux/Steam Deck, any bash host). Built as a single file:

    bash build.sh        # cat src/*.sh > dml
    ./dev-install.ps1    # (Windows) install into dml-arch + print version

Bootstrap installs still come from Install-DML.ps1 (embedded v2.6.0); the DML
Launcher dev-installs this newer CLI over it. Do not edit `dml` directly —
edit `src/*.sh` and rebuild.

## Machine-readable contract (--json)

Add `--json` anywhere in argv. Two shapes:

**Envelopes** (single JSON object, one line):
- ok:    `{"ok":true,"data":{...}}` — exit 0
- error: `{"ok":false,"error":{"code":"NOT_FOUND","message":"...","hint":"..."}}` — exit 1

**NDJSON streams** (long-running commands: `games start|stop|restart`):
one JSON object per line — `section_start`, `line` (level: info|warn|error),
`section_end`, then exactly one terminal `done` (exit 0) or `error` (exit 1).
`pct` is reserved for installers.

Error codes: UNKNOWN_COMMAND, NOT_FOUND, NO_COMPOSE, DOCKER_DOWN,
START_FAILED, STOP_FAILED.

Commands:
- `dml games list --json` → `{"games":[{"id","path","running"}]}`
- `dml games status <id> --json` → `{"id","state":"running"|"stopped"}`
- `dml games start|restart|stop <id> --json` → NDJSON stream
- `dml version --json` → `{"version":"3.0.0"}`

`games start|restart` run `<compose_dir>/dml-start.sh <mode>` when present
(staged AzerothCore start that avoids re-running ac-db-import); otherwise
`docker compose up -d` / `down`.

Tests: `bats tests/` inside the distro; `tests/windows-smoke.ps1` from Windows.
```

- [ ] **Step 4: Run the whole suite one last time**

```powershell
wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"
powershell -File cli\tests\windows-smoke.ps1
```
Expected: all bats PASS + `SMOKE OK`.

- [ ] **Step 5: Commit**

```powershell
git add cli/tests/windows-smoke.ps1 cli/README.md
git commit -m "feat(cli): Windows smoke test for the wsl.exe --json call path + contract README"
```
