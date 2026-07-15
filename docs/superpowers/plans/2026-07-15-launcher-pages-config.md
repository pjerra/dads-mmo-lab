# Launcher Pages + Config Editor Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the DML Launcher's disabled sidebar pages (Dashboard, Item Database, Teleport, Config) into working pages on top of the live-verified `dml wow` CLI, adding six new CLI verbs (`accounts`, `server-info`, `config list/set/raw-read/raw-write`) and the user-requested multi-function config editor.

**Architecture:** The brains stay in the bash `dml` CLI (new verbs follow Plan 3's JSON-envelope contract, stub-tested with bats); the Tauri Rust layer grows one thin typed command per verb (webview can never run anything else); the Svelte UI becomes a 5-page shell where each page is its own component and the existing Terminal/reducer are untouched. Spec: `docs/superpowers/specs/2026-07-15-launcher-pages-config-design.md` (user-approved, facts pinned against the live stack 2026-07-15).

**Tech Stack:** bash + bats-core (stub harness in `cli/tests/helpers/env.bash`), mikefarah yq v4, Tauri 2 + Rust (cargo tests over cmd.exe fixture scripts), Svelte 5 runes + SvelteKit single route, vitest, svelte-check.

## Global Constraints

- NEVER hand-edit `cli/dml` — it is a build artifact; edit `cli/src/*.sh` then `bash build.sh`. Concatenation order is glob order: `00-head.sh, 10-json.sh, 20-soap.sh, 30-db.sh, 40-config.sh (new), 90-main.sh`.
- The whole built CLI runs under `set -euo pipefail`. EVERY command substitution whose command can fail must be guarded (`if out="$(cmd)"; then rc=0; else rc=$?; fi` or `|| true` / `|| echo ""`), and every helper that ends in a conditional needs a final `return 0`. This bit Plan 3 five times.
- JSON contract: every `--json` invocation emits exactly ONE envelope on stdout — `{"ok":true,"data":{...}}` or `{"ok":false,"error":{"code","message","hint"}}` + exit 1. Error codes only from: UNKNOWN_COMMAND, NOT_FOUND, NO_COMPOSE, DOCKER_DOWN, START_FAILED, STOP_FAILED, SOAP_AUTH, SOAP_FAULT, SOAP_UNREACHABLE, DB_UNREACHABLE, BAD_ARG, MISSING_DEP.
- Never quote-wrap name arguments in AC console commands (live-confirmed: AC's parser keeps quotes literal; only `#subject`/`#text` QuotedStrings take quotes).
- Mutations via SOAP or registry-validated env writes only; MySQL access read-only; every value-taking flag calls `_need_flag_val "$1" $#` before reading `$2`.
- All new shell files LF (`.gitattributes` enforces). jq is test-only, never a runtime dependency.
- CLI test command (PowerShell tool, NOT git-bash): `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/<file>.bats"`. Full suite: `bats tests/` (108 tests green at plan end; 96 at plan start).
- Launcher commands run from `launcher/`: `npm test` (vitest), `npm run check` (svelte-check), and from `launcher/src-tauri`: `cargo test` with `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"` prepended.
- `launcher/src-tauri/Cargo.toml` has a harmless pre-existing EOL-only ghost modification — NEVER stage or revert it; commit files explicitly by path.
- Git commit messages containing double quotes: write to a temp file and use `git commit -F <file>` (PowerShell 5.1 mangles embedded quotes).
- Registry env names are PINNED from the live stack: `AC_AI_PLAYERBOT_MIN_RANDOM_BOTS` / `AC_AI_PLAYERBOT_MAX_RANDOM_BOTS` / `AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN` (present in the real override.yml), `AC_RATE_XP_KILL` / `AC_RATE_XP_QUEST` / `AC_RATE_DROP_MONEY` / `AC_MOTD` / `AC_AUCTION_HOUSE_BOT_ENABLE_SELLER` / `AC_AUCTION_HOUSE_BOT_ENABLE_BUYER` / `AC_AUCTION_HOUSE_BOT_GUID` / `AC_AUCTION_HOUSE_BOT_ACCOUNT` (mangling rule proven by the worldserver log line `Updates.EnableDatabases` → `AC_UPDATES_ENABLE_DATABASES`).
- The real server dir bind-mounts `./env/dist/etc` into the container (verified), so module confs are host files under `<server dir>/env/dist/etc/modules/` — raw config IO is plain file IO.

---

### Task 1: `dml wow accounts` (read-only account+character list)

**Files:**
- Modify: `cli/src/30-db.sh` (add `_accounts_rows_to_json` after `_items_rows_to_json`)
- Modify: `cli/src/90-main.sh` (new `accounts)` arm in the `wow` case, before `characters)`)
- Test: `cli/tests/wow-accounts.bats` (new)

**Interfaces:**
- Consumes: `db_chars_query` (30-db.sh), `json_ok`/`json_err`/`json_escape` (10-json.sh), mysql stub (`use_mysql_stub`, `DML_STUB_DB_ROWS`, `DML_STUB_DB_EXIT` in helpers/env.bash).
- Produces: `dml wow accounts --json` → `{"ok":true,"data":{"accounts":[{"id":<int>,"username":"<str>","characters":[{"guid":<int>,"name":"<str>","level":<int>}]}]}}`. Accounts with no characters have `"characters":[]`. Task 6's `wow_accounts` Rust command and Task 8's CharPicker rely on exactly this shape.

- [ ] **Step 1: Write the failing tests**

Create `cli/tests/wow-accounts.bats`:

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

# Rows are: account_id, username, guid, char_name, level (TSV; LEFT JOIN
# misses coalesced to empty strings by the SQL).
@test "accounts groups characters under their account" {
  printf '251\tHYPEER\t2502\tHypeer\t100\n253\tTEST1\t2503\tTesten\t1\n253\tTEST1\t2504\tAltchar\t5\n' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.accounts | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].username')" = "HYPEER" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters | length')" = "2" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters[1].name')" = "Altchar" ]
  [ "$(echo "$output" | jq -r '.data.accounts[1].characters[0].level')" = "1" ]
}

@test "accounts keeps a character-less account with empty characters array" {
  printf '254\tDMLSOAP\t\t\t\n' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].username')" = "DMLSOAP" ]
  [ "$(echo "$output" | jq -r '.data.accounts[0].characters | length')" = "0" ]
}

@test "accounts survives the trailing-newline-stripped last row" {
  # printf without trailing \n = what command substitution feeds the parser
  printf '251\tHYPEER\t2502\tHypeer\t100' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.accounts | length')" = "1" ]
}

@test "accounts maps db failure to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow accounts --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "accounts SQL filters bot accounts" {
  # The stub answers any query, so assert on the QUERY text the arm builds:
  # the mysql stub records its -e argument to DML_STUB_DB_QUERY_LOG when set.
  printf '' > "$FIXTURE/rows.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/rows.tsv"
  export DML_STUB_DB_QUERY_LOG="$FIXTURE/query.log"
  run bash "$DML" wow accounts --json
  [ "$status" -eq 0 ]
  grep -q "NOT LIKE 'RNDBOT%'" "$FIXTURE/query.log"
  grep -q "<> 'AHBOT'" "$FIXTURE/query.log"
}
```

- [ ] **Step 2: Add query logging to the mysql stub (verified absent)**

In `cli/tests/helpers/env.bash`, inside `use_mysql_stub`'s embedded docker stub script, the `exec` branch currently reads:

```bash
if [[ "${1:-}" == "exec" ]]; then
  [[ -n "${DML_STUB_DB_ROWS:-}" ]] && cat "$DML_STUB_DB_ROWS"
  exit "${DML_STUB_DB_EXIT:-0}"
fi
```

Change it to log the full argv (which contains the `-e <sql>` text) when the seam is set:

```bash
if [[ "${1:-}" == "exec" ]]; then
  [[ -n "${DML_STUB_DB_QUERY_LOG:-}" ]] && printf '%s\n' "$*" >> "$DML_STUB_DB_QUERY_LOG"
  [[ -n "${DML_STUB_DB_ROWS:-}" ]] && cat "$DML_STUB_DB_ROWS"
  exit "${DML_STUB_DB_EXIT:-0}"
fi
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-accounts.bats"`
Expected: FAIL — `accounts` is an unknown wow subcommand (UNKNOWN_COMMAND envelope), so the jq asserts mismatch.

- [ ] **Step 4: Implement `_accounts_rows_to_json` in `cli/src/30-db.sh`**

Append after `_items_rows_to_json`:

```bash
# Reads TSV rows (account_id, username, guid, char_name, level) sorted by
# account_id, emits a JSON array of account objects with nested characters.
# LEFT JOIN misses arrive as empty guid/name/level fields. Same last-row
# guard as _items_rows_to_json (see the long comment there).
_accounts_rows_to_json() {
    local out='[' first=1 cur_id="" cur_name="" chars="" cfirst=1
    local aid uname guid cname clvl
    while IFS=$'\t' read -r aid uname guid cname clvl || [[ -n "$aid" ]]; do
        [[ -z "$aid" ]] && continue
        if [[ "$aid" != "$cur_id" ]]; then
            if [[ -n "$cur_id" ]]; then
                [[ $first -eq 0 ]] && out+=','
                out+="{\"id\":$cur_id,\"username\":\"$(json_escape "$cur_name")\",\"characters\":[$chars]}"
                first=0
            fi
            cur_id="$aid"; cur_name="$uname"; chars=""; cfirst=1
        fi
        if [[ -n "$guid" ]]; then
            [[ $cfirst -eq 0 ]] && chars+=','
            chars+="{\"guid\":$guid,\"name\":\"$(json_escape "$cname")\",\"level\":$clvl}"
            cfirst=0
        fi
    done
    if [[ -n "$cur_id" ]]; then
        [[ $first -eq 0 ]] && out+=','
        out+="{\"id\":$cur_id,\"username\":\"$(json_escape "$cur_name")\",\"characters\":[$chars]}"
    fi
    out+=']'
    printf '%s' "$out"
}
```

- [ ] **Step 5: Add the `accounts)` arm in `cli/src/90-main.sh`**

Insert inside `case "$wsub" in`, directly BEFORE the `characters)` arm:

```bash
      accounts)
        # Read-only list of real player accounts and their characters.
        # The 250 RNDBOT* ambient-bot accounts and AHBOT are noise for the
        # GUI's character picker; SOAP-only accounts (e.g. DMLSOAP) simply
        # have no characters and are harmless to include.
        sql="SELECT a.id, a.username, COALESCE(c.guid,''), COALESCE(c.name,''), COALESCE(c.level,'')
             FROM acore_auth.account a
             LEFT JOIN characters c ON c.account = a.id
             WHERE a.username NOT LIKE 'RNDBOT%' AND a.username <> 'AHBOT'
             ORDER BY a.id, c.level DESC;"
        rows="$(db_chars_query "$sql")" \
          || { json_err DB_UNREACHABLE "Could not reach the characters/auth database" "Is ac-database running?"; exit 1; }
        json_ok "{\"accounts\":$(printf '%s' "$rows" | _accounts_rows_to_json)}"
        ;;
```

- [ ] **Step 6: Rebuild and run the tests**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-accounts.bats"`
Expected: 5 tests PASS.

- [ ] **Step 7: Full suite**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"`
Expected: 101/101 PASS (96 existing + 5 new).

- [ ] **Step 8: Commit**

```bash
git add cli/dml cli/src/30-db.sh cli/src/90-main.sh cli/tests/wow-accounts.bats cli/tests/helpers/env.bash
git commit -m "feat(cli): dml wow accounts (read-only account+character list, bot accounts filtered)"
```

---

### Task 2: `dml wow server-info` (parsed SOAP server status)

**Files:**
- Create: `cli/src/40-config.sh` (starts with `_parse_server_info`; Task 3 adds the registry)
- Modify: `cli/src/90-main.sh` (new `server-info)` arm after `soap-exec)`)
- Create: `cli/tests/fixtures/server-info-live.txt`
- Test: `cli/tests/wow-server-info.bats` (new)

**Interfaces:**
- Consumes: `soap_exec` (20-soap.sh; exit 0 ok / 2 fault / 3 auth / 4 unreachable), `json_ok`/`json_err`/`json_escape`, curl stub (`use_curl_stub`, `DML_STUB_SOAP_RESPONSE`, `DML_STUB_HTTP`, `DML_STUB_CURL_EXIT`).
- Produces: `dml wow server-info --json` → `{"ok":true,"data":{"online":bool,"version":str|null,"players":int|null,"uptime":str|null,"mean_ms":int|null,"median_ms":int|null}}`. Server down (SOAP unreachable OR fault) is `online:false` with ok:true — down is an answer, not an error. Only `SOAP_AUTH` stays an error envelope. Task 8's Dashboard renders exactly these fields.

- [ ] **Step 1: Create the live fixture**

Create `cli/tests/fixtures/server-info-live.txt` with EXACTLY this content (captured from the real worldserver 2026-07-15; the `&#xD;` sequences are literal — `soap_parse_result` does not decode XML entities):

```
AzerothCore rev. 52f58186a533+ 2026-07-10 08:24:30 -0700 (Playerbot branch) (Unix, RelWithDebInfo, Static)&#xD;
Connected players: 1. Characters in world: 1799.&#xD;
Connection peak: 1.&#xD;
Server uptime: 19 minute(s) 29 second(s)&#xD;
Update time diff: 15ms. Last 500 diffs summary:&#xD;
|- Mean: 44ms&#xD;
|- Median: 18ms&#xD;
|- Percentiles (95, 99, max): 131ms, 143ms, 161ms&#xD;
```

- [ ] **Step 2: Write the failing tests**

Create `cli/tests/wow-server-info.bats`:

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

@test "_parse_server_info extracts fields from live capture" {
  run bash -c 'source "'"$BATS_TEST_DIRNAME"'/../src/10-json.sh"; source "'"$BATS_TEST_DIRNAME"'/../src/40-config.sh"; _parse_server_info < "'"$BATS_TEST_DIRNAME"'/fixtures/server-info-live.txt"'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.online')" = "true" ]
  [ "$(echo "$output" | jq -r '.players')" = "1" ]
  [ "$(echo "$output" | jq -r '.uptime')" = "19 minute(s) 29 second(s)" ]
  [ "$(echo "$output" | jq -r '.mean_ms')" = "44" ]
  [ "$(echo "$output" | jq -r '.median_ms')" = "18" ]
  [[ "$(echo "$output" | jq -r '.version')" == 52f58186a533+* ]]
}

@test "server-info wraps the parsed object in an envelope" {
  # Build a SOAP <result> body around the live text so soap_exec extracts it.
  {
    printf '<?xml version="1.0"?><SOAP-ENV:Envelope><SOAP-ENV:Body><ns1:executeCommandResponse><result>'
    cat "$BATS_TEST_DIRNAME/fixtures/server-info-live.txt"
    printf '</result></ns1:executeCommandResponse></SOAP-ENV:Body></SOAP-ENV:Envelope>'
  } > "$FIXTURE/si.xml"
  export DML_STUB_SOAP_RESPONSE="$FIXTURE/si.xml"
  run bash "$DML" wow server-info --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.ok')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.online')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.players')" = "1" ]
}

@test "server-info reports online:false when SOAP is unreachable (not an error)" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CURL_EXIT=7
  run bash "$DML" wow server-info --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.online')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.players')" = "null" ]
}

@test "server-info keeps SOAP_AUTH as an error" {
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-401-unauthorized.txt"
  export DML_STUB_HTTP=401
  run bash "$DML" wow server-info --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_AUTH" ]
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-server-info.bats"`
Expected: FAIL — `40-config.sh` doesn't exist / unknown subcommand.

- [ ] **Step 4: Create `cli/src/40-config.sh` with the parser**

```bash
# ---------------------------------------------------------------------------
# WoW config registry + server-info parsing for the DML Launcher.
# (Registry + config verbs land in this file too — see `wow config` in main.)
# ---------------------------------------------------------------------------

# Parses the raw text of the SOAP `server info` result (stdin) into a JSON
# object (stdout). The raw text carries literal `&#xD;` entities because
# soap_parse_result extracts the <result> text without XML-decoding it.
# Unparseable fields become null rather than an error -- the Dashboard
# renders "unknown" for those instead of failing the whole card.
_parse_server_info() {
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
    printf '{"online":true,"version":%s,"players":%s,"uptime":%s,"mean_ms":%s,"median_ms":%s}' \
        "$vjson" "$players" "$ujson" "$mean" "$median"
    return 0
}
```

- [ ] **Step 5: Add the `server-info)` arm in `cli/src/90-main.sh`**

Insert directly AFTER the whole `soap-exec)` arm (after its `;;`):

```bash
      server-info)
        # Down is an answer, not an error: unreachable/fault -> online:false.
        # Only auth failure stays an error (creds are wrong, not the server).
        if out="$(soap_exec 'server info')"; then rc=0; else rc=$?; fi
        case "$rc" in
          0) json_ok "$(printf '%s' "$out" | _parse_server_info)" ;;
          3) json_err SOAP_AUTH "SOAP authentication failed" "Check ~/.dml/soap.env" ; exit 1 ;;
          *) json_ok '{"online":false,"version":null,"players":null,"uptime":null,"mean_ms":null,"median_ms":null}' ;;
        esac
        ;;
```

- [ ] **Step 6: Rebuild and run the tests**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-server-info.bats"`
Expected: 4 tests PASS.

- [ ] **Step 7: Full suite, then commit**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"` — expected 105/105.

```bash
git add cli/dml cli/src/40-config.sh cli/src/90-main.sh cli/tests/wow-server-info.bats cli/tests/fixtures/server-info-live.txt
git commit -m "feat(cli): dml wow server-info (parsed status; down is online:false, not an error)"
```

---

### Task 3: config registry + `dml wow config list` / `config set`

**Files:**
- Modify: `cli/src/40-config.sh` (registry data + read/write helpers)
- Modify: `cli/src/90-main.sh` (new `config)` arm with `list`/`set` subcases, before the wow `*)` arm)
- Test: `cli/tests/wow-config.bats` (new)

**Interfaces:**
- Consumes: `_wow_server_dir` (90-main.sh, returns the wow title's compose dir or empty), `DML_YQ_BIN` seam, `db_chars_query`, `_valid_charname`, `_need_flag_val`, json helpers, mysql stub.
- Produces:
  - `dml wow config list --json` → `{"settings":[{"key","group","label","explain","type","min","max","value","default","restart_required","env"}]}` — `type` ∈ `float|int|bool|text|char`; `min`/`max` are numbers or null; `value`/`default` are ALWAYS JSON strings (bools are `"1"`/`"0"`; `ahbot.character`'s value is the stored GUID string); `restart_required` is always `true`; `value` = current env value from the override, else `default`.
  - `dml wow config set --key <k> --value <v> --json` → `{"changed":bool,"restart_required":bool}` (restart_required mirrors changed, same as soap-setup). Unknown key → `NOT_FOUND`; bad value → `BAD_ARG`; missing yq → `MISSING_DEP`; no wow install → `NOT_FOUND`.
  - Registry helper contract for Task 4+11: `_cfg_rows` prints pipe-separated rows `key|group|label|type|min|max|env|default|explain`.

> **AMENDMENT (2026-07-15, user-approved after Step 1 found it):** `Motd` is NOT a
> conf key on this AC build (17.0.0-dev) — MOTD is DB-backed (`acore_auth.motd`,
> `MotdMgr`) and set live via the `.server set motd <realmId> <locale> <text>`
> console command; `AC_MOTD` would silently no-op. The `server.motd` registry row
> therefore: env column is the sentinel `-`; `config set server.motd` sanitizes
> (strip `"`, CR/LF→space) then sends `server set motd 1 enUS <text>` over
> `soap_exec` (rc 0 → `{"changed":true,"restart_required":false}`; rc 3 →
> SOAP_AUTH; rc 2 → SOAP_FAULT; rc 4 → SOAP_UNREACHABLE with hint "The server
> must be running to change the message of the day — start it first.");
> `config list` reads the motd row's value read-only from
> `db_auth_query "SELECT text FROM motd WHERE realmid=1 LIMIT 1;"` (guarded,
> default on empty/failure) and reports `restart_required:false` for this row
> only. Tests use the curl capture stub (`DML_STUB_CAPTURE`) to assert the exact
> posted command text, plus a mysql stub for the list read-back and a
> DB-down-falls-back-to-default case. The code blocks below predate the
> amendment where they mention `AC_MOTD`.

- [ ] **Step 1: Verify the pinned conf keys against the real stack (evidence step, no code)**

Run: `wsl -d dml-arch -u dml -- bash -lc "grep -hE '^(Rate\.XP\.Kill|Rate\.XP\.Quest|Rate\.Drop\.Money|Motd) ' ~/games/wow-server-playerbots/env/dist/etc/worldserver.conf.dist | head; grep -hE '^AuctionHouseBot\.(EnableSeller|EnableBuyer|Account|GUID) ' ~/games/wow-server-playerbots/env/dist/etc/modules/mod_ahbot.conf.dist"`
Expected: each key appears with a default (`Rate.XP.Kill = 1`, `Motd = "Welcome..."`, `AuctionHouseBot.EnableSeller = 0`, etc.). If a key is missing, STOP and re-derive the env name from the actual key before continuing (update the registry row AND the spec).

- [ ] **Step 2: Write the failing tests**

Create `cli/tests/wow-config.bats`:

```bash
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  add_game wow-server-playerbots compose
  GDIR="$DML_GAMES_DIR/wow-server-playerbots"
  OVR="$GDIR/docker-compose.override.yml"
  cat > "$OVR" <<'EOF'
services:
  ac-worldserver:
    environment:
      AC_AI_PLAYERBOT_MIN_RANDOM_BOTS: "1600"
      AC_AI_PLAYERBOT_MAX_RANDOM_BOTS: "2000"
EOF
}
teardown() { teardown_fixture; }

@test "config list returns the registry with live values and defaults" {
  run bash "$DML" wow config list --json
  [ "$status" -eq 0 ]
  # bots.population reads the MAX env (2000); rates fall back to default "1"
  [ "$(echo "$output" | jq -r '.data.settings | length')" = "9" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="bots.population") | .value')" = "2000" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .value')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="rates.xp_kill") | .default')" = "1" ]
  [ "$(echo "$output" | jq -r '.data.settings[] | select(.key=="server.motd") | .type')" = "text" ]
  [ "$(echo "$output" | jq -r '.data.settings[0].restart_required')" = "true" ]
}

@test "config set writes the env var and is idempotent" {
  run bash "$DML" wow config set --key rates.xp_kill --value 3 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.changed')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "true" ]
  yq -e '.services.ac-worldserver.environment.AC_RATE_XP_KILL == "3"' "$OVR"
  # pre-existing env preserved (the duplicate-services-key regression guard)
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MIN_RANDOM_BOTS == "1600"' "$OVR"
  run bash "$DML" wow config set --key rates.xp_kill --value 3 --json
  [ "$(echo "$output" | jq -r '.data.changed')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.restart_required')" = "false" ]
}

@test "config set bots.population writes BOTH min and max" {
  run bash "$DML" wow config set --key bots.population --value 500 --json
  [ "$status" -eq 0 ]
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MIN_RANDOM_BOTS == "500"' "$OVR"
  yq -e '.services.ac-worldserver.environment.AC_AI_PLAYERBOT_MAX_RANDOM_BOTS == "500"' "$OVR"
}

@test "config set rejects out-of-range and wrong-type values as BAD_ARG" {
  run bash "$DML" wow config set --key rates.xp_kill --value 21 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key rates.xp_kill --value abc --json
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key bots.autologin --value 2 --json
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow config set --key bots.population --value 3001 --json
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "config set rejects an unknown key as NOT_FOUND" {
  run bash "$DML" wow config set --key nope.nope --value 1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "config set server.motd sanitizes quotes and newlines" {
  run bash "$DML" wow config set --key server.motd --value $'Wel"come\nfriends' --json
  [ "$status" -eq 0 ]
  [ "$(yq -r '.services.ac-worldserver.environment.AC_MOTD' "$OVR")" = "Welcome friends" ]
}

@test "config set ahbot.character resolves the char and writes GUID+ACCOUNT" {
  use_mysql_stub
  printf '2503\t253\n' > "$FIXTURE/char.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/char.tsv"
  run bash "$DML" wow config set --key ahbot.character --value Testen --json
  [ "$status" -eq 0 ]
  yq -e '.services.ac-worldserver.environment.AC_AUCTION_HOUSE_BOT_GUID == "2503"' "$OVR"
  yq -e '.services.ac-worldserver.environment.AC_AUCTION_HOUSE_BOT_ACCOUNT == "253"' "$OVR"
}

@test "config set ahbot.character with unknown char is NOT_FOUND" {
  use_mysql_stub
  printf '' > "$FIXTURE/char.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/char.tsv"
  run bash "$DML" wow config set --key ahbot.character --value Nobody --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "config errors NOT_FOUND when wow server absent and MISSING_DEP without yq" {
  rm -rf "$DML_GAMES_DIR/wow-server-playerbots"
  run bash "$DML" wow config list --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  add_game wow-server-playerbots compose
  run env DML_YQ_BIN=definitely-missing-yq bash "$DML" wow config list --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "MISSING_DEP" ]
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-config.bats"`
Expected: FAIL (unknown subcommand `config`).

- [ ] **Step 4: Add registry + helpers to `cli/src/40-config.sh`**

Append:

```bash
# --- Config registry -------------------------------------------------------
# One row per curated setting: key|group|label|type|min|max|env|default|explain
# type: float | int | bool | text | char. bool values are "1"/"0" strings
# (that is what the AC env bridge expects). ahbot.character is special-cased
# in `config set` (resolves a character name to GUID+ACCOUNT, writes both).
# bots.population is special-cased (one number written to MIN and MAX).
_cfg_rows() {
cat <<'EOF'
rates.xp_kill|Rates|XP from kills|float|0.5|20|AC_RATE_XP_KILL|1|Multiplies XP earned from kills. 3 = level three times as fast.
rates.xp_quest|Rates|XP from quests|float|0.5|20|AC_RATE_XP_QUEST|1|Multiplies XP from quest turn-ins.
rates.gold|Rates|Gold drops|float|0.5|20|AC_RATE_DROP_MONEY|1|Multiplies money dropped by creatures.
bots.population|Playerbots|World bot population|int|0|3000|AC_AI_PLAYERBOT_MAX_RANDOM_BOTS|500|How many ambient bots populate the world. Saving writes min and max to this one number.
bots.autologin|Playerbots|Bots log in at server start|bool|||AC_AI_PLAYERBOT_RANDOM_BOT_AUTOLOGIN|1|When on, ambient bots log in automatically after the server starts.
ahbot.seller|AHBot|Auction seller bot|bool|||AC_AUCTION_HOUSE_BOT_ENABLE_SELLER|0|When on, the auction house is stocked with items for sale.
ahbot.buyer|AHBot|Auction buyer bot|bool|||AC_AUCTION_HOUSE_BOT_ENABLE_BUYER|0|When on, the bot occasionally buys player auctions.
ahbot.character|AHBot|Seller character|char|||AC_AUCTION_HOUSE_BOT_GUID|0|Which character appears as the auction seller. Saving also writes the matching account id. Shown as the stored character id.
server.motd|Server|Message of the day|text|||AC_MOTD|Welcome to Dad's MMO Lab!|Shown to every player at login. Quotes and line breaks are removed.
EOF
}

# Shared preamble for every `wow config` subcommand: needs yq + the wow dir.
# Sets: cfg_sdir, cfg_ovr. Emits the error envelope and exits on failure.
_cfg_preamble() {
    DML_YQ_BIN="${DML_YQ_BIN:-yq}"
    if ! command -v "$DML_YQ_BIN" >/dev/null 2>&1; then
        json_err MISSING_DEP "yq is required for wow config but not installed" "Run: pacman -S go-yq (inside dml-arch as root)"
        exit 1
    fi
    cfg_sdir="$(_wow_server_dir)"
    if [[ -z "$cfg_sdir" ]]; then
        json_err NOT_FOUND "WoW Playerbots server not installed" "Install it first, then re-run."
        exit 1
    fi
    cfg_ovr="$cfg_sdir/docker-compose.override.yml"
    return 0
}

# _cfg_env_read <ENV>: echoes the override's value for that env key, or "".
_cfg_env_read() {
    [[ -f "$cfg_ovr" ]] || { printf ''; return 0; }
    E="$1" "$DML_YQ_BIN" -r '.services.ac-worldserver.environment[strenv(E)] // ""' "$cfg_ovr" 2>/dev/null || printf ''
    return 0
}

# _cfg_env_write <ENV> <value>: merges the key into the EXISTING service
# (soap-setup's proven pattern -- never a second top-level services: block).
# strenv() keeps hostile values out of the yq program text entirely.
# Sets CFG_CHANGED=true when the stored value actually changed.
_cfg_env_write() {
    local cur
    cur="$(_cfg_env_read "$1")"
    [[ "$cur" == "$2" ]] && return 0
    [[ -f "$cfg_ovr" ]] || printf 'services:\n  ac-worldserver:\n    environment:\n' > "$cfg_ovr"
    E="$1" V="$2" "$DML_YQ_BIN" -i \
        '.services.ac-worldserver.environment[strenv(E)] = strenv(V)' "$cfg_ovr"
    CFG_CHANGED=true
    return 0
}

# _float_in_range <val> <min> <max>: 0 iff val is a decimal in [min,max].
_float_in_range() {
    [[ "$1" =~ ^[0-9]+([.][0-9]+)?$ ]] || return 1
    awk -v v="$1" -v lo="$2" -v hi="$3" 'BEGIN { exit !(v >= lo && v <= hi) }'
}
```

- [ ] **Step 5: Add the `config)` arm in `cli/src/90-main.sh`**

Insert inside `case "$wsub" in`, directly BEFORE the final `*)` arm of the wow case:

```bash
      config)
        csub="${1:-}"; shift || true
        case "$csub" in
          list)
            _cfg_preamble
            first=1; out='['
            while IFS='|' read -r key group label type minv maxv env def explain; do
              [[ -z "$key" ]] && continue
              val="$(_cfg_env_read "$env")"
              [[ -n "$val" ]] || val="$def"
              minj="${minv:-null}"; maxj="${maxv:-null}"
              [[ $first -eq 0 ]] && out+=','
              out+="{\"key\":\"$key\",\"group\":\"$group\",\"label\":\"$(json_escape "$label")\",\"explain\":\"$(json_escape "$explain")\",\"type\":\"$type\",\"min\":$minj,\"max\":$maxj,\"value\":\"$(json_escape "$val")\",\"default\":\"$(json_escape "$def")\",\"restart_required\":true,\"env\":\"$env\"}"
              first=0
            done < <(_cfg_rows)
            out+=']'
            json_ok "{\"settings\":$out}"
            ;;
          set)
            key=""; value=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --key) _need_flag_val "$1" $#; key="$2"; shift 2 ;;
                --value) _need_flag_val "$1" $#; value="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            [[ -n "$key" ]] || { json_err BAD_ARG "Missing --key" "See: dml wow config list --json"; exit 1; }
            row="$(_cfg_rows | grep -F "$key|" | head -1)" || true
            [[ "$row" == "$key|"* ]] || { json_err NOT_FOUND "Unknown setting: $key" "See: dml wow config list --json"; exit 1; }
            IFS='|' read -r _ group label type minv maxv env def explain <<< "$row"
            _cfg_preamble
            CFG_CHANGED=false
            case "$type" in
              float)
                _float_in_range "$value" "$minv" "$maxv" \
                  || { json_err BAD_ARG "$label must be a number between $minv and $maxv, got: $value" ""; exit 1; }
                ;;
              int)
                [[ "$value" =~ ^[0-9]+$ ]] && (( value >= minv && value <= maxv )) \
                  || { json_err BAD_ARG "$label must be a whole number between $minv and $maxv, got: $value" ""; exit 1; }
                ;;
              bool)
                [[ "$value" =~ ^[01]$ ]] \
                  || { json_err BAD_ARG "$label takes 1 (on) or 0 (off), got: $value" ""; exit 1; }
                ;;
              text)
                value="${value//\"/}"; value="${value//$'\n'/ }"; value="${value//$'\r'/ }"
                ;;
              char)
                _valid_charname "$value" \
                  || { json_err BAD_ARG "Invalid character name: $value" "1-12 letters/digits/underscore."; exit 1; }
                ;;
            esac
            if [[ "$key" == "ahbot.character" ]]; then
              crow="$(db_chars_query "SELECT guid, account FROM characters WHERE name='$(sql_escape "$value")' LIMIT 1;")" \
                || { json_err DB_UNREACHABLE "Could not look up the character" "Is ac-database running?"; exit 1; }
              [[ -n "$crow" ]] || { json_err NOT_FOUND "No such character: $value" ""; exit 1; }
              IFS=$'\t' read -r cguid cacct <<< "$crow"
              [[ "$cguid" =~ ^[0-9]+$ && "$cacct" =~ ^[0-9]+$ ]] \
                || { json_err DB_UNREACHABLE "Unexpected character lookup result" ""; exit 1; }
              _cfg_env_write AC_AUCTION_HOUSE_BOT_GUID "$cguid"
              _cfg_env_write AC_AUCTION_HOUSE_BOT_ACCOUNT "$cacct"
            elif [[ "$key" == "bots.population" ]]; then
              _cfg_env_write AC_AI_PLAYERBOT_MIN_RANDOM_BOTS "$value"
              _cfg_env_write AC_AI_PLAYERBOT_MAX_RANDOM_BOTS "$value"
            else
              _cfg_env_write "$env" "$value"
            fi
            json_ok "{\"changed\":$CFG_CHANGED,\"restart_required\":$CFG_CHANGED}"
            ;;
          *)
            json_err BAD_ARG "Unknown config subcommand: $csub" "Try: dml wow config list --json"
            exit 1
            ;;
        esac
        ;;
```

NOTE the grep: `grep -F "$key|"` matches the key anywhere; the `[[ "$row" == "$key|"* ]]` guard right after is what enforces an exact key prefix — keep both lines.

- [ ] **Step 6: Rebuild and run the tests**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-config.bats"`
Expected: 9 tests PASS.

- [ ] **Step 7: Full suite, then commit**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bats tests/"` — expected 114/114.

```bash
git add cli/dml cli/src/40-config.sh cli/src/90-main.sh cli/tests/wow-config.bats
git commit -m "feat(cli): dml wow config list/set (curated registry over the override env write path)"
```

---

### Task 4: `dml wow config raw-read` / `raw-write` (Files tab backend)

**Files:**
- Modify: `cli/src/40-config.sh` (add `_cfg_file_path`)
- Modify: `cli/src/90-main.sh` (add `raw-read` / `raw-write` subcases inside the `config)` arm from Task 3)
- Test: append to `cli/tests/wow-config.bats`

**Interfaces:**
- Consumes: `_cfg_preamble` (Task 3), json helpers.
- Produces:
  - `dml wow config raw-read --file <name> --json` → `{"file":"<name>","content":"<full file, JSON-escaped>"}`; unknown name or missing file → `NOT_FOUND`.
  - `dml wow config raw-write --file <name> --json` with the new content on **stdin** → `{"written":true,"backup":"<name>.bak"}` (backup `null` when the file didn't exist before). Only `docker-compose.override.yml` is YAML-validated; invalid YAML → `BAD_ARG`, target untouched.
  - Allowlist: `.env`, `docker-compose.override.yml`, `playerbots.conf`, `mod_ahbot.conf`, `mod_ale.conf`. Task 6's `wow_config_raw_write` feeds stdin via the runner.

- [ ] **Step 1: Write the failing tests (append to `cli/tests/wow-config.bats`)**

```bash
@test "config raw-read returns file content; unknown name is NOT_FOUND" {
  printf 'FOO=bar\nBAZ=1\n' > "$GDIR/.env"
  run bash "$DML" wow config raw-read --file .env --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.file')" = ".env" ]
  # NB: $(cat file) inside the arm strips the trailing newline before
  # json_escape, and $(...) here strips it again -- compare without it.
  [ "$(echo "$output" | jq -r '.data.content')" = $'FOO=bar\nBAZ=1' ]
  run bash "$DML" wow config raw-read --file worldserver.conf --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  run bash "$DML" wow config raw-read --file ../../../etc/passwd --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "config raw-write writes via stdin and keeps a .bak of the old content" {
  mkdir -p "$GDIR/env/dist/etc/modules"
  printf 'old=1\n' > "$GDIR/env/dist/etc/modules/playerbots.conf"
  run bash -c 'printf "new=2\n" | bash "'"$DML"'" wow config raw-write --file playerbots.conf --json'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.written')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.backup')" = "playerbots.conf.bak" ]
  [ "$(cat "$GDIR/env/dist/etc/modules/playerbots.conf")" = "new=2" ]
  [ "$(cat "$GDIR/env/dist/etc/modules/playerbots.conf.bak")" = "old=1" ]
}

@test "config raw-write of a brand-new file reports backup null" {
  mkdir -p "$GDIR/env/dist/etc/modules"
  rm -f "$GDIR/env/dist/etc/modules/mod_ale.conf"
  run bash -c 'printf "x=1\n" | bash "'"$DML"'" wow config raw-write --file mod_ale.conf --json'
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.backup')" = "null" ]
}

@test "config raw-write rejects invalid YAML for the override and leaves it untouched" {
  printf 'services: {}\n' > "$OVR"
  run bash -c 'printf "services: [unclosed\n" | bash "'"$DML"'" wow config raw-write --file docker-compose.override.yml --json'
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  [ "$(cat "$OVR")" = "services: {}" ]
  [ ! -f "$OVR.bak" ]
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-config.bats"`
Expected: the 4 new tests FAIL (BAD_ARG "Unknown config subcommand"), the 9 from Task 3 still PASS.

- [ ] **Step 3: Add `_cfg_file_path` to `cli/src/40-config.sh`**

```bash
# _cfg_file_path <name>: maps an allowlisted file name to its host path
# under $cfg_sdir (the base compose bind-mounts ./env/dist/etc into the
# container, so module confs are ordinary host files). Unknown name -> rc 1.
# The allowlist is the traversal guard: names are matched literally, never
# used as path fragments.
_cfg_file_path() {
    case "$1" in
        .env) printf '%s' "$cfg_sdir/.env" ;;
        docker-compose.override.yml) printf '%s' "$cfg_sdir/docker-compose.override.yml" ;;
        playerbots.conf|mod_ahbot.conf|mod_ale.conf) printf '%s' "$cfg_sdir/env/dist/etc/modules/$1" ;;
        *) return 1 ;;
    esac
    return 0
}
```

- [ ] **Step 4: Add the subcases inside the `config)` arm (between `set)` and `*)`)**

```bash
          raw-read)
            fname=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; fname="$2"; shift 2; }
            [[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }
            _cfg_preamble
            fpath="$(_cfg_file_path "$fname")" \
              || { json_err NOT_FOUND "Not an editable file: $fname" "Editable: .env, docker-compose.override.yml, playerbots.conf, mod_ahbot.conf, mod_ale.conf"; exit 1; }
            [[ -f "$fpath" ]] || { json_err NOT_FOUND "File does not exist yet: $fname" ""; exit 1; }
            json_ok "{\"file\":\"$(json_escape "$fname")\",\"content\":\"$(json_escape "$(cat "$fpath")")\"}"
            ;;
          raw-write)
            fname=""
            [[ "${1:-}" == "--file" ]] && { _need_flag_val "$1" $#; fname="$2"; shift 2; }
            [[ -n "$fname" ]] || { json_err BAD_ARG "Missing --file <name>" ""; exit 1; }
            _cfg_preamble
            fpath="$(_cfg_file_path "$fname")" \
              || { json_err NOT_FOUND "Not an editable file: $fname" "Editable: .env, docker-compose.override.yml, playerbots.conf, mod_ahbot.conf, mod_ale.conf"; exit 1; }
            mkdir -p "$(dirname "$fpath")"
            tmp="$fpath.tmp.$$"
            cat > "$tmp"
            if [[ "$fname" == "docker-compose.override.yml" ]]; then
              # A syntactically broken override stops the whole stack from
              # even starting -- validate BEFORE touching the real file.
              if ! "$DML_YQ_BIN" e '.' "$tmp" >/dev/null 2>&1; then
                rm -f "$tmp"
                json_err BAD_ARG "That is not valid YAML - not saved" "Fix the syntax and save again."
                exit 1
              fi
            fi
            bakjson=null
            if [[ -f "$fpath" ]]; then
              cp -p "$fpath" "$fpath.bak"
              bakjson="\"$(json_escape "$fname.bak")\""
            fi
            mv "$tmp" "$fpath"
            json_ok "{\"written\":true,\"backup\":$bakjson}"
            ;;
```

- [ ] **Step 5: Rebuild, run the file, then the full suite**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/wow-config.bats && bats tests/"`
Expected: 13/13 in wow-config.bats; full suite 118/118.

- [ ] **Step 6: Commit**

```bash
git add cli/dml cli/src/40-config.sh cli/src/90-main.sh cli/tests/wow-config.bats
git commit -m "feat(cli): dml wow config raw-read/raw-write (allowlisted files, .bak, YAML pre-validation)"
```

---

### Task 5: CLI docs + full-suite gate

**Files:**
- Modify: `cli/README.md` (extend the `## wow subcommands` section)

**Interfaces:** Consumes everything from Tasks 1-4. Produces the written contract Tasks 6-11 build against.

- [ ] **Step 1: Append to the wow subcommands documentation in `cli/README.md`**

Add after the existing `paperdoll` block, matching the established style:

```markdown
- `dml wow accounts --json` →
  `{"accounts":[{"id","username","characters":[{"guid","name","level"}]}]}`
  Read-only list of real player accounts and their characters (the GUI's
  character picker). Ambient-bot accounts (`RNDBOT*`) and `AHBOT` are
  filtered out; accounts with no characters (e.g. a SOAP-only account) come
  back with an empty `characters` array. Errors: `DB_UNREACHABLE`.

- `dml wow server-info --json` →
  `{"online","version","players","uptime","mean_ms","median_ms"}`
  Parsed `server info` over SOAP. A down/unreachable worldserver is
  `online:false` with `ok:true` — down is an answer, not an error; only bad
  credentials stay an error (`SOAP_AUTH`). Unparseable fields are `null`.

- `dml wow config list --json` →
  `{"settings":[{"key","group","label","explain","type","min","max","value",
  "default","restart_required","env"}]}`
  The curated settings registry with live values. Values are read from the
  wow title's `docker-compose.override.yml` environment (the write target is
  the source of truth); an unset key shows its default. `type` is one of
  `float|int|bool|text|char`; `value`/`default` are always JSON strings
  (bools are `"1"`/`"0"`; the AHBot seller character's value is the stored
  character GUID). Every setting is restart-to-apply. Errors: `NOT_FOUND`
  (wow title not installed), `MISSING_DEP` (yq).

- `dml wow config set --key <k> --value <v> --json` →
  `{"changed":bool,"restart_required":bool}` (mirrors `changed`, like
  soap-setup). The value is validated against the registry (type + range) —
  `BAD_ARG` otherwise; unknown key is `NOT_FOUND`. Writes the mapped
  `AC_*` env var into the override via yq (same proven merge path as
  soap-setup; never a second top-level `services:` block). Special cases:
  `bots.population` writes BOTH `AC_AI_PLAYERBOT_MIN_RANDOM_BOTS` and
  `..._MAX_RANDOM_BOTS` to the one number; `ahbot.character` resolves the
  character name read-only to its guid+account and writes
  `AC_AUCTION_HOUSE_BOT_GUID` + `AC_AUCTION_HOUSE_BOT_ACCOUNT`
  (`NOT_FOUND` if no such character); `server.motd` strips double quotes
  and CR/LF (replaced with a space).

- `dml wow config raw-read --file <name> --json` → `{"file","content"}` and
  `dml wow config raw-write --file <name> --json` (new content on stdin) →
  `{"written":true,"backup":"<name>.bak"|null}`
  The Advanced files editor. `<name>` must be one of `.env`,
  `docker-compose.override.yml`, `playerbots.conf`, `mod_ahbot.conf`,
  `mod_ale.conf` (`NOT_FOUND` otherwise — the literal-name allowlist is
  also the path-traversal guard; module confs are host files because the
  base compose bind-mounts `./env/dist/etc`). Every overwrite keeps a
  single-slot `.bak` of the previous content. The compose override is
  YAML-validated before writing — invalid YAML is `BAD_ARG` and the file
  is untouched.
```

- [ ] **Step 2: Full suite gate**

Run: `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"`
Expected: 118/118, output pristine.

- [ ] **Step 3: Commit**

```bash
git add cli/README.md
git commit -m "docs(cli): document wow accounts/server-info/config verbs"
```

---

### Task 6: Rust layer — stdin runner + typed commands + games_restart

**Files:**
- Modify: `launcher/src-tauri/src/dml/runner.rs` (factor output-parsing; add `run_json_with_stdin`)
- Modify: `launcher/src-tauri/src/lib.rs` (generic json helper + 11 wow commands + `games_restart`; register all)
- Create: `launcher/src-tauri/tests/fixtures/stdin_echo.cmd`

**Interfaces:**
- Consumes: CLI verbs from Tasks 1-4 (exact argv: `["wow","accounts"]`, `["wow","server-info"]`, `["wow","items","search","--name",name,...]`, `["wow","mail-item","--to",to,"--items",items,...]`, `["wow","teleport-list","--search",s]`, `["wow","teleport","--char",c,"--to",t]`, `["wow","paperdoll","--char",c]`, `["wow","config","list"]`, `["wow","config","set","--key",k,"--value",v]`, `["wow","config","raw-read","--file",f]`, `["wow","config","raw-write","--file",f]` + stdin, `["games","restart",id]` streaming).
- Produces (Task 7's api.ts invokes these names): `wow_accounts`, `wow_server_info`, `wow_items_search(name, quality?, minLevel?, maxLevel?)`, `wow_mail_item(to, items, subject?, body?)`, `wow_teleport_list(search?)`, `wow_teleport(charName, to)`, `wow_paperdoll(charName)`, `wow_config_list`, `wow_config_set(key, value)`, `wow_config_raw_read(file)`, `wow_config_raw_write(file, content)`, `games_restart(id, onEvent)`. All return `Result<serde_json::Value, CmdError>` (the `data` payload) except `games_restart` (streams TermEvents, returns `()`); Tauri exposes camelCase parameter names to JS.

- [ ] **Step 1: Write the failing runner test (append to `runner.rs` tests)**

```rust
    #[test]
    fn run_json_with_stdin_delivers_input_to_the_child() {
        let env = fixture_runner()
            .run_json_with_stdin(&[&fixture("stdin_echo.cmd")], "hello world")
            .unwrap();
        assert!(env.ok);
        assert_eq!(env.data["echo"], "hello world");
    }
```

Create `launcher/src-tauri/tests/fixtures/stdin_echo.cmd` (CRLF line endings like the sibling fixtures):

```
@echo off
set /p LINE=
echo {"ok":true,"data":{"echo":"%LINE%"}}
```

- [ ] **Step 2: Run to verify it fails**

Run (PowerShell): `$env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"; cd C:\Users\perzi\dads-mmo-lab\launcher\src-tauri; cargo test`
Expected: compile error — `run_json_with_stdin` not found.

- [ ] **Step 3: Implement in `runner.rs`**

Factor the shared tail of `run_json` and add the stdin variant:

```rust
    fn finish_json(&self, out: std::process::Output) -> Result<Envelope, RunnerError> {
        let stdout = decode_wsl_output(&out.stdout);
        parse_envelope(&stdout).map_err(|parse_err| {
            if stdout.trim().is_empty() && !out.status.success() {
                let stderr = decode_wsl_output(&out.stderr);
                let stderr = stderr.trim();
                if stderr.is_empty() {
                    RunnerError::Spawn(format!(
                        "wsl exited with code {} and no output",
                        out.status.code().unwrap_or(-1)
                    ))
                } else {
                    RunnerError::Spawn(stderr.to_string())
                }
            } else {
                RunnerError::BadOutput { raw: parse_err }
            }
        })
    }

    pub fn run_json_with_stdin(&self, args: &[&str], input: &str) -> Result<Envelope, RunnerError> {
        use std::io::Write;
        let mut child = self
            .command(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        {
            let mut stdin = child.stdin.take().expect("stdin piped above");
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        } // dropping stdin closes it so the child sees EOF
        let out = child
            .wait_with_output()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        self.finish_json(out)
    }
```

Rewrite `run_json`'s body to call the shared tail:

```rust
    pub fn run_json(&self, args: &[&str]) -> Result<Envelope, RunnerError> {
        let out = self
            .command(args)
            .output()
            .map_err(|e| RunnerError::Spawn(e.to_string()))?;
        self.finish_json(out)
    }
```

- [ ] **Step 4: Run cargo test — runner tests green**

Expected: all existing tests + `run_json_with_stdin_delivers_input_to_the_child` PASS.

- [ ] **Step 5: Add the commands to `lib.rs`**

Add a generic helper after `envelope_to_result`:

```rust
async fn run_json_cmd(
    state: State<'_, AppState>,
    args: Vec<String>,
) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        runner.run_json(&refs)
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map_err(CmdError::from)
    .and_then(envelope_to_result)
}
```

Then the commands (validation stays minimal — the CLI is the authority; the vector-argv spawn has no shell, so pass-through is injection-safe):

```rust
#[tauri::command]
async fn wow_accounts(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "accounts".into()]).await
}

#[tauri::command]
async fn wow_server_info(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "server-info".into()]).await
}

#[tauri::command]
async fn wow_items_search(
    name: String,
    quality: Option<u32>,
    min_level: Option<u32>,
    max_level: Option<u32>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> =
        vec!["wow".into(), "items".into(), "search".into(), "--name".into(), name];
    if let Some(q) = quality {
        args.extend(["--quality".into(), q.to_string()]);
    }
    if let Some(l) = min_level {
        args.extend(["--min-level".into(), l.to_string()]);
    }
    if let Some(l) = max_level {
        args.extend(["--max-level".into(), l.to_string()]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_mail_item(
    to: String,
    items: String,
    subject: Option<String>,
    body: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> =
        vec!["wow".into(), "mail-item".into(), "--to".into(), to, "--items".into(), items];
    if let Some(s) = subject {
        args.extend(["--subject".into(), s]);
    }
    if let Some(b) = body {
        args.extend(["--body".into(), b]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_teleport_list(
    search: Option<String>,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let mut args: Vec<String> = vec!["wow".into(), "teleport-list".into()];
    if let Some(s) = search {
        args.extend(["--search".into(), s]);
    }
    run_json_cmd(state, args).await
}

#[tauri::command]
async fn wow_teleport(
    char_name: String,
    to: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "teleport".into(), "--char".into(), char_name, "--to".into(), to],
    )
    .await
}

#[tauri::command]
async fn wow_paperdoll(
    char_name: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "paperdoll".into(), "--char".into(), char_name]).await
}

#[tauri::command]
async fn wow_config_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "list".into()]).await
}

#[tauri::command]
async fn wow_config_set(
    key: String,
    value: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "config".into(), "set".into(), "--key".into(), key, "--value".into(), value],
    )
    .await
}

#[tauri::command]
async fn wow_config_raw_read(
    file: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "config".into(), "raw-read".into(), "--file".into(), file]).await
}

#[tauri::command]
async fn wow_config_raw_write(
    file: String,
    content: String,
    state: State<'_, AppState>,
) -> Result<serde_json::Value, CmdError> {
    let runner = state.runner.clone();
    tauri::async_runtime::spawn_blocking(move || {
        runner.run_json_with_stdin(
            &["wow", "config", "raw-write", "--file", &file],
            &content,
        )
    })
    .await
    .map_err(|e| CmdError { code: "INTERNAL".into(), message: e.to_string(), hint: String::new() })?
    .map_err(CmdError::from)
    .and_then(envelope_to_result)
}

#[tauri::command]
async fn games_restart(
    id: String,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    stream_action("restart", id, on_event, state).await
}
```

Register every new command in `generate_handler![...]` alongside the existing five.

- [ ] **Step 6: cargo test + commit**

Run: `cargo test` — expected all green (17 tests: 16 existing + stdin).

```bash
git add launcher/src-tauri/src/dml/runner.rs launcher/src-tauri/src/lib.rs launcher/src-tauri/tests/fixtures/stdin_echo.cmd
git commit -m "feat(launcher): typed wow_* commands, stdin-capable runner, games_restart"
```

---

### Task 7: api.ts wrappers + page-shell refactor (sidebar switches pages)

**Files:**
- Modify: `launcher/src/lib/api.ts` (types + wrappers for every Task 6 command)
- Create: `launcher/src/lib/pages/Library.svelte` (extract today's content verbatim)
- Modify: `launcher/src/routes/+page.svelte` (becomes the shell)

**Interfaces:**
- Consumes: Task 6 command names/parameters (Tauri camelCases Rust snake_case params: `char_name` → `charName`, `min_level` → `minLevel`).
- Produces (Tasks 8-11 import these): types `Account`, `CharacterSummary`, `ServerInfo`, `ItemRow`, `TeleLocation`, `PaperdollData`, `PaperdollItem`, `ConfigSetting`, `RawFileName`; functions `wowAccounts(): Promise<Account[]>`, `wowServerInfo(): Promise<ServerInfo>`, `wowItemsSearch(p: {name: string; quality?: number; minLevel?: number; maxLevel?: number}): Promise<ItemRow[]>`, `wowMailItem(p: {to: string; items: string; subject?: string; body?: string}): Promise<{sent: boolean; to: string; attachments: number}>`, `wowTeleportList(search?: string): Promise<TeleLocation[]>`, `wowTeleport(charName: string, to: string): Promise<{teleported: boolean; char: string; to: string}>`, `wowPaperdoll(charName: string): Promise<PaperdollData>`, `wowConfigList(): Promise<ConfigSetting[]>`, `wowConfigSet(key: string, value: string): Promise<{changed: boolean; restart_required: boolean}>`, `wowConfigRawRead(file: RawFileName): Promise<{file: string; content: string}>`, `wowConfigRawWrite(file: RawFileName, content: string): Promise<{written: boolean; backup: string | null}>`, `gamesRestart(id: string, onEvent: (e: TermEvent) => void): Promise<void>`. Shell exports nothing; page components receive no props (self-contained), except Library which keeps its internal state.

- [ ] **Step 1: Extend `launcher/src/lib/api.ts`**

Append:

```typescript
export interface CharacterSummary {
  guid: number;
  name: string;
  level: number;
}
export interface Account {
  id: number;
  username: string;
  characters: CharacterSummary[];
}
export interface ServerInfo {
  online: boolean;
  version: string | null;
  players: number | null;
  uptime: string | null;
  mean_ms: number | null;
  median_ms: number | null;
}
export interface ItemRow {
  entry: number;
  name: string;
  quality: number;
  item_level: number;
  required_level: number;
  class: number;
  subclass: number;
  inventory_type: number;
  displayid: number;
}
export interface TeleLocation {
  name: string;
  x: number;
  y: number;
  z: number;
  map: number;
}
export interface PaperdollItem {
  slot: number;
  entry: number;
  name: string;
  quality: number;
  item_level: number;
  displayid: number;
}
export interface PaperdollData {
  name: string;
  level: number;
  class: number;
  gold: number;
  note: string;
  equipped: PaperdollItem[];
}
export interface ConfigSetting {
  key: string;
  group: string;
  label: string;
  explain: string;
  type: "float" | "int" | "bool" | "text" | "char";
  min: number | null;
  max: number | null;
  value: string;
  default: string;
  restart_required: boolean;
  env: string;
}
export type RawFileName =
  | ".env"
  | "docker-compose.override.yml"
  | "playerbots.conf"
  | "mod_ahbot.conf"
  | "mod_ale.conf";

export async function wowAccounts(): Promise<Account[]> {
  const data = await invoke<{ accounts: Account[] }>("wow_accounts");
  return data.accounts;
}
export async function wowServerInfo(): Promise<ServerInfo> {
  return await invoke("wow_server_info");
}
export async function wowItemsSearch(p: {
  name: string;
  quality?: number;
  minLevel?: number;
  maxLevel?: number;
}): Promise<ItemRow[]> {
  const data = await invoke<{ items: ItemRow[] }>("wow_items_search", p);
  return data.items;
}
export async function wowMailItem(p: {
  to: string;
  items: string;
  subject?: string;
  body?: string;
}): Promise<{ sent: boolean; to: string; attachments: number }> {
  return await invoke("wow_mail_item", p);
}
export async function wowTeleportList(search?: string): Promise<TeleLocation[]> {
  const data = await invoke<{ locations: TeleLocation[] }>("wow_teleport_list", { search });
  return data.locations;
}
export async function wowTeleport(
  charName: string,
  to: string,
): Promise<{ teleported: boolean; char: string; to: string }> {
  return await invoke("wow_teleport", { charName, to });
}
export async function wowPaperdoll(charName: string): Promise<PaperdollData> {
  return await invoke("wow_paperdoll", { charName });
}
export async function wowConfigList(): Promise<ConfigSetting[]> {
  const data = await invoke<{ settings: ConfigSetting[] }>("wow_config_list");
  return data.settings;
}
export async function wowConfigSet(
  key: string,
  value: string,
): Promise<{ changed: boolean; restart_required: boolean }> {
  return await invoke("wow_config_set", { key, value });
}
export async function wowConfigRawRead(
  file: RawFileName,
): Promise<{ file: string; content: string }> {
  return await invoke("wow_config_raw_read", { file });
}
export async function wowConfigRawWrite(
  file: RawFileName,
  content: string,
): Promise<{ written: boolean; backup: string | null }> {
  return await invoke("wow_config_raw_write", { file, content });
}
export const gamesRestart = (id: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("games_restart", { id, onEvent: ch });
};
```

- [ ] **Step 2: Extract `launcher/src/lib/pages/Library.svelte`**

Move today's `+page.svelte` content (script block state/functions `games`, `loadError`, `busyId`, `term`, `showTerm`, `refresh`, `act`; the `<section class="content">` markup; the content-related styles) into `Library.svelte` unchanged, importing from `$lib/api` and `$lib/terminal-state` exactly as before. The component has no props.

- [ ] **Step 3: Rewrite `launcher/src/routes/+page.svelte` as the shell**

```svelte
<script lang="ts">
  import Library from "$lib/pages/Library.svelte";
  import Dashboard from "$lib/pages/Dashboard.svelte";
  import Items from "$lib/pages/Items.svelte";
  import Teleport from "$lib/pages/Teleport.svelte";
  import Config from "$lib/pages/Config.svelte";

  const PAGES = [
    { id: "library", label: "Library" },
    { id: "dashboard", label: "Dashboard" },
    { id: "items", label: "Item Database" },
    { id: "teleport", label: "Teleport" },
    { id: "config", label: "Config" },
  ] as const;
  type PageId = (typeof PAGES)[number]["id"];
  let page: PageId = $state("library");
</script>

<main class="shell">
  <nav class="sidebar">
    <h1>DML<span>Launcher</span></h1>
    {#each PAGES as p (p.id)}
      <button class:active={page === p.id} onclick={() => (page = p.id)}>{p.label}</button>
    {/each}
    <button class="disabled" disabled title="Coming with My Party">Playerbots</button>
  </nav>

  {#if page === "library"}<Library />{/if}
  {#if page === "dashboard"}<Dashboard />{/if}
  {#if page === "items"}<Items />{/if}
  {#if page === "teleport"}<Teleport />{/if}
  {#if page === "config"}<Config />{/if}
</main>

<style>
  :global(body) { margin: 0; background: #010409; color: #c9d1d9; font-family: "Segoe UI", system-ui, sans-serif; }
  .shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }
  .sidebar { background: #0d1117; border-right: 1px solid #30363d; padding: 16px 0; display: flex; flex-direction: column; gap: 2px; }
  .sidebar h1 { font-size: 16px; margin: 0 16px 14px; color: #58a6ff; }
  .sidebar h1 span { color: #c9d1d9; font-weight: 300; margin-left: 4px; }
  .sidebar button { padding: 8px 16px; color: #8b949e; font-size: 14px; background: none; border: none; text-align: left; cursor: pointer; border-left: 2px solid transparent; }
  .sidebar button.active { color: #f0f6fc; background: #161b22; border-left-color: #58a6ff; }
  .sidebar button.disabled { opacity: 0.35; cursor: default; }
</style>
```

NOTE: for THIS step only, create minimal placeholder components so the app compiles (each later task replaces its own): `Dashboard.svelte`, `Items.svelte`, `Teleport.svelte`, `Config.svelte`, each containing exactly:

```svelte
<section class="content"><p>Coming up in this plan.</p></section>
<style>.content { padding: 20px 24px; }</style>
```

These placeholders are REPLACED by Tasks 8-11 within this same plan — they are scaffolding for compilation order, not deferred work. The `{#if}` chain (not `<svelte:component>`) keeps each page's lifecycle independent.

Also move the shared `.content`/`.bar`/`.cards`/`.card`/`.dot`/`button`/`.muted`/`.error-card` styles INTO `Library.svelte` (they were page styles, not shell styles). Each new page carries its own copy of the small `.content`/`.error-card` styles — Svelte styles are component-scoped; a shared stylesheet is not worth the churn for 5 pages.

- [ ] **Step 4: Gates**

Run (from `launcher/`): `npm run check` — expected 0 errors, 0 warnings. `npm test` — expected all vitest green (reducer untouched). From `launcher/src-tauri`: `cargo test` — green.

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/api.ts launcher/src/lib/pages/ launcher/src/routes/+page.svelte
git commit -m "feat(launcher): page shell with working sidebar; api wrappers for all wow verbs"
```

---

### Task 8: CharPicker + Dashboard page

**Files:**
- Create: `launcher/src/lib/CharPicker.svelte`
- Create: `launcher/src/lib/wow.ts` (quality names helper)
- Create: `launcher/src/lib/wow.test.ts`
- Replace: `launcher/src/lib/pages/Dashboard.svelte`

**Interfaces:**
- Consumes: `wowAccounts`, `wowServerInfo`, `wowPaperdoll`, types from api.ts.
- Produces: `CharPicker.svelte` props: `{ selected: string }` two-way bindable (`bind:selected`) — the selected character NAME, `""` when none. `wow.ts` exports `qualityName(q: number): string` and `QUALITY_COLORS: Record<number, string>`. Tasks 9-11 reuse both.

- [ ] **Step 1: Write the failing vitest for `wow.ts`**

Create `launcher/src/lib/wow.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { qualityName, QUALITY_COLORS } from "./wow";

describe("qualityName", () => {
  it("maps the WoW quality tiers", () => {
    expect(qualityName(0)).toBe("Poor");
    expect(qualityName(1)).toBe("Common");
    expect(qualityName(2)).toBe("Uncommon");
    expect(qualityName(3)).toBe("Rare");
    expect(qualityName(4)).toBe("Epic");
    expect(qualityName(5)).toBe("Legendary");
  });
  it("falls back for unknown tiers and has a color per tier", () => {
    expect(qualityName(9)).toBe("Unknown");
    for (let q = 0; q <= 5; q++) expect(QUALITY_COLORS[q]).toMatch(/^#/);
  });
});
```

- [ ] **Step 2: Run `npm test` — expect FAIL (module missing), then implement `launcher/src/lib/wow.ts`**

```typescript
const QUALITY_NAMES: Record<number, string> = {
  0: "Poor",
  1: "Common",
  2: "Uncommon",
  3: "Rare",
  4: "Epic",
  5: "Legendary",
};

export const QUALITY_COLORS: Record<number, string> = {
  0: "#9d9d9d",
  1: "#ffffff",
  2: "#1eff00",
  3: "#0070dd",
  4: "#a335ee",
  5: "#ff8000",
};

export function qualityName(q: number): string {
  return QUALITY_NAMES[q] ?? "Unknown";
}
```

Run `npm test` — expected PASS.

- [ ] **Step 3: Create `launcher/src/lib/CharPicker.svelte`**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { wowAccounts, type Account } from "$lib/api";

  let { selected = $bindable("") }: { selected?: string } = $props();
  let accounts: Account[] = $state([]);
  let accountName = $state("");
  let error: string | null = $state(null);

  const current = $derived(accounts.find((a) => a.username === accountName));

  onMount(async () => {
    try {
      accounts = await wowAccounts();
      const first = accounts.find((a) => a.characters.length > 0);
      if (first) {
        accountName = first.username;
        selected = first.characters[0].name;
      }
    } catch (e) {
      const err = e as { message?: string };
      error = err.message ?? String(e);
    }
  });

  function onAccountChange() {
    selected = current?.characters[0]?.name ?? "";
  }
</script>

{#if error}
  <span class="err">Couldn't load characters: {error}</span>
{:else}
  <select bind:value={accountName} onchange={onAccountChange}>
    {#each accounts as a (a.id)}
      <option value={a.username}>{a.username}</option>
    {/each}
  </select>
  <select bind:value={selected} disabled={!current || current.characters.length === 0}>
    {#each current?.characters ?? [] as c (c.guid)}
      <option value={c.name}>{c.name} (lvl {c.level})</option>
    {/each}
  </select>
{/if}

<style>
  select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 5px 8px; }
  .err { color: #f85149; font-size: 13px; }
</style>
```

- [ ] **Step 4: Replace `launcher/src/lib/pages/Dashboard.svelte`**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { wowServerInfo, wowPaperdoll, type ServerInfo, type PaperdollData } from "$lib/api";
  import { qualityName, QUALITY_COLORS } from "$lib/wow";
  import CharPicker from "$lib/CharPicker.svelte";

  let info: ServerInfo | null = $state(null);
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
      info = await wowServerInfo();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      infoError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loadingInfo = false;
    }
  }
  onMount(refreshInfo);

  async function loadDoll() {
    if (!charName) return;
    loadingDoll = true;
    dollError = null;
    doll = null;
    try {
      doll = await wowPaperdoll(charName);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      dollError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loadingDoll = false;
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Dashboard</h2>
    <button onclick={refreshInfo} disabled={loadingInfo}>Refresh</button>
  </header>

  {#if infoError}
    <div class="error-card"><strong>Couldn't read server status.</strong><p>{infoError}</p></div>
  {:else if info}
    <div class="card status">
      <div>
        <span class="dot {info.online ? 'on' : 'off'}"></span>
        <strong>{info.online ? "World is up" : "World is down"}</strong>
      </div>
      {#if info.online}
        <div class="stats">
          <span>Players online: <strong>{info.players ?? "?"}</strong></span>
          <span>Uptime: <strong>{info.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{info.mean_ms ?? "?"} ms avg</strong></span>
        </div>
      {:else}
        <p class="muted">Start it from the Library page.</p>
      {/if}
    </div>
  {/if}

  <header class="bar"><h2>Character viewer</h2></header>
  <div class="pickrow">
    <CharPicker bind:selected={charName} />
    <button onclick={loadDoll} disabled={!charName || loadingDoll}>Show gear</button>
  </div>
  {#if dollError}
    <div class="error-card"><p>{dollError}</p></div>
  {:else if doll}
    <div class="card doll">
      <strong>{doll.name}</strong> — level {doll.level}, {doll.gold} gold
      <table>
        <tbody>
          {#each doll.equipped as it (it.slot)}
            <tr>
              <td style="color: {QUALITY_COLORS[it.quality] ?? '#c9d1d9'}">{it.name}</td>
              <td>{qualityName(it.quality)}</td>
              <td>ilvl {it.item_level}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      <p class="muted">Shown as of the character's last save — an online character can lag a little.</p>
    </div>
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; }
  .status .stats { display: flex; gap: 24px; margin-top: 8px; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; margin-right: 6px; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .pickrow { display: flex; gap: 8px; align-items: center; }
  table { border-collapse: collapse; margin-top: 10px; }
  td { padding: 3px 12px 3px 0; font-size: 14px; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
```

- [ ] **Step 5: Gates + commit**

Run: `npm run check` (0/0), `npm test` (green).

```bash
git add launcher/src/lib/CharPicker.svelte launcher/src/lib/wow.ts launcher/src/lib/wow.test.ts launcher/src/lib/pages/Dashboard.svelte
git commit -m "feat(launcher): Dashboard page (server status + paperdoll viewer) and shared CharPicker"
```

---

### Task 9: Item Database page

**Files:**
- Replace: `launcher/src/lib/pages/Items.svelte`

**Interfaces:**
- Consumes: `wowItemsSearch`, `wowMailItem`, `ItemRow`, `qualityName`/`QUALITY_COLORS`, `CharPicker`.
- Produces: the Items page. Send dialog clamps count to 1–200 client-side (server-side the CLI validates the `id:count` spec format).

- [ ] **Step 1: Replace `launcher/src/lib/pages/Items.svelte`**

```svelte
<script lang="ts">
  import { wowItemsSearch, wowMailItem, type ItemRow } from "$lib/api";
  import { qualityName, QUALITY_COLORS } from "$lib/wow";
  import CharPicker from "$lib/CharPicker.svelte";

  let name = $state("");
  let quality = $state<string>("");
  let minLevel = $state<string>("");
  let maxLevel = $state<string>("");
  let rows: ItemRow[] = $state([]);
  let searched = $state(false);
  let searching = $state(false);
  let error: string | null = $state(null);

  let sendItem: ItemRow | null = $state(null);
  let sendTo = $state("");
  let sendCount = $state(1);
  let sendSubject = $state("");
  let sending = $state(false);
  let sentMsg: string | null = $state(null);

  async function search() {
    if (!name.trim()) return;
    searching = true;
    error = null;
    sentMsg = null;
    try {
      rows = await wowItemsSearch({
        name: name.trim(),
        quality: quality === "" ? undefined : Number(quality),
        minLevel: minLevel === "" ? undefined : Number(minLevel),
        maxLevel: maxLevel === "" ? undefined : Number(maxLevel),
      });
      searched = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      searching = false;
    }
  }

  async function send() {
    if (!sendItem || !sendTo) return;
    const count = Math.min(200, Math.max(1, Math.floor(sendCount) || 1));
    sending = true;
    error = null;
    try {
      await wowMailItem({
        to: sendTo,
        items: `${sendItem.entry}:${count}`,
        subject: sendSubject.trim() || undefined,
      });
      sentMsg = `Sent ${count}x ${sendItem.name} to ${sendTo} (check the mailbox).`;
      sendItem = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      sending = false;
    }
  }
</script>

<section class="content">
  <header class="bar"><h2>Item Database</h2></header>

  <form class="filters" onsubmit={(e) => { e.preventDefault(); search(); }}>
    <input placeholder="Item name (required)" bind:value={name} />
    <select bind:value={quality}>
      <option value="">Any quality</option>
      {#each [0, 1, 2, 3, 4, 5] as q}
        <option value={String(q)}>{qualityName(q)}</option>
      {/each}
    </select>
    <input placeholder="Min lvl" size="6" bind:value={minLevel} />
    <input placeholder="Max lvl" size="6" bind:value={maxLevel} />
    <button class="primary" type="submit" disabled={!name.trim() || searching}>Search</button>
  </form>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if sentMsg}<div class="ok-card"><p>{sentMsg}</p></div>{/if}

  {#if searched && rows.length === 0 && !error}
    <p class="muted">No items matched.</p>
  {/if}

  {#if rows.length > 0}
    <table>
      <thead><tr><th>Name</th><th>Quality</th><th>Item lvl</th><th>Req lvl</th><th></th></tr></thead>
      <tbody>
        {#each rows as it (it.entry)}
          <tr>
            <td style="color: {QUALITY_COLORS[it.quality] ?? '#c9d1d9'}">{it.name}</td>
            <td>{qualityName(it.quality)}</td>
            <td>{it.item_level}</td>
            <td>{it.required_level}</td>
            <td><button onclick={() => { sendItem = it; sentMsg = null; }}>Send</button></td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}

  {#if sendItem}
    <div class="card sendbox">
      <strong>Send {sendItem.name}</strong>
      <div class="row">
        <CharPicker bind:selected={sendTo} />
        <label>Count <input type="number" min="1" max="200" bind:value={sendCount} /></label>
      </div>
      <input placeholder="Mail subject (optional)" bind:value={sendSubject} />
      <div class="row">
        <button class="primary" onclick={send} disabled={!sendTo || sending}>Send mail</button>
        <button onclick={() => (sendItem = null)} disabled={sending}>Cancel</button>
      </div>
    </div>
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar h2 { margin: 0; font-size: 18px; }
  .filters { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input, select { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  table { border-collapse: collapse; }
  th { text-align: left; color: #8b949e; font-size: 13px; padding: 4px 14px 4px 0; }
  td { padding: 4px 14px 4px 0; font-size: 14px; border-top: 1px solid #21262d; }
  .card, .sendbox { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; flex-direction: column; gap: 10px; }
  .row { display: flex; gap: 10px; align-items: center; }
  label { font-size: 14px; color: #8b949e; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .ok-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
</style>
```

- [ ] **Step 2: Gates + commit**

Run: `npm run check` (0/0), `npm test` (green).

```bash
git add launcher/src/lib/pages/Items.svelte
git commit -m "feat(launcher): Item Database page (search/filters + send-to-character mail)"
```

---

### Task 10: Teleport page

**Files:**
- Replace: `launcher/src/lib/pages/Teleport.svelte`

**Interfaces:**
- Consumes: `wowTeleportList`, `wowTeleport`, `TeleLocation`, `CharPicker`.
- Produces: the Teleport page. Two-step confirm (button becomes "Confirm?" on first click — deterministic, no browser dialogs). 500-row cap notice when the result length is exactly 500.

- [ ] **Step 1: Replace `launcher/src/lib/pages/Teleport.svelte`**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { wowTeleportList, wowTeleport, type TeleLocation } from "$lib/api";
  import CharPicker from "$lib/CharPicker.svelte";

  let search = $state("");
  let locations: TeleLocation[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let charName = $state("");
  let picked: string | null = $state(null);
  let confirming = $state(false);
  let teleporting = $state(false);
  let doneMsg: string | null = $state(null);

  async function load() {
    loading = true;
    error = null;
    try {
      locations = await wowTeleportList(search.trim() || undefined);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loading = false;
    }
  }
  onMount(load);

  function pick(name: string) {
    picked = name;
    confirming = false;
    doneMsg = null;
  }

  async function go() {
    if (!picked || !charName) return;
    if (!confirming) {
      confirming = true;
      return;
    }
    teleporting = true;
    error = null;
    try {
      const r = await wowTeleport(charName, picked);
      doneMsg = `${r.char} sent to ${r.to}.`;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      teleporting = false;
      confirming = false;
    }
  }
</script>

<section class="content">
  <header class="bar"><h2>Teleport</h2></header>

  <form class="row" onsubmit={(e) => { e.preventDefault(); load(); }}>
    <input placeholder="Filter locations…" bind:value={search} />
    <button type="submit" disabled={loading}>Filter</button>
  </form>

  <div class="row">
    <span class="muted">Who:</span>
    <CharPicker bind:selected={charName} />
    {#if picked}
      <span class="muted">→ {picked}</span>
      <button class="primary" onclick={go} disabled={!charName || teleporting}>
        {confirming ? `Really send ${charName} to ${picked}?` : "Teleport"}
      </button>
    {/if}
  </div>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if doneMsg}<div class="ok-card"><p>{doneMsg}</p></div>{/if}
  {#if locations.length === 500}
    <p class="muted">Showing the first 500 — narrow the filter to see the rest.</p>
  {/if}

  <div class="loclist">
    {#each locations as l (l.name)}
      <button class="loc" class:sel={picked === l.name} onclick={() => pick(l.name)}>
        {l.name} <span class="muted">map {l.map}</span>
      </button>
    {/each}
  </div>
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; }
  .bar h2 { margin: 0; font-size: 18px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; min-width: 240px; }
  .loclist { display: flex; flex-wrap: wrap; gap: 6px; }
  .loc { background: #0d1117; border: 1px solid #30363d; border-radius: 6px; padding: 5px 10px; color: #c9d1d9; cursor: pointer; font-size: 13px; }
  .loc.sel { border-color: #58a6ff; color: #f0f6fc; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .ok-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
</style>
```

- [ ] **Step 2: Gates + commit**

Run: `npm run check` (0/0), `npm test` (green).

```bash
git add launcher/src/lib/pages/Teleport.svelte
git commit -m "feat(launcher): Teleport page (filtered locations, two-step confirm)"
```

---

### Task 11: Config page (Settings + Files tabs, Save & Restart)

**Files:**
- Create: `launcher/src/lib/config-diff.ts`
- Create: `launcher/src/lib/config-diff.test.ts`
- Replace: `launcher/src/lib/pages/Config.svelte`

**Interfaces:**
- Consumes: `wowConfigList`, `wowConfigSet`, `wowConfigRawRead`, `wowConfigRawWrite`, `gamesRestart`, `ConfigSetting`, `RawFileName`, `CharPicker`, `Terminal` + `applyEvent`/`initialTermState` (existing, from `$lib/Terminal.svelte` / `$lib/terminal-state`).
- Produces: the Config page; `config-diff.ts` exports `dirtyKeys(settings: {key: string; value: string}[], edits: Record<string, string>): string[]`. The restart target id is the literal `"wow-server-playerbots"`.

- [ ] **Step 1: Write the failing vitest**

Create `launcher/src/lib/config-diff.test.ts`:

```typescript
import { describe, expect, it } from "vitest";
import { dirtyKeys } from "./config-diff";

const settings = [
  { key: "rates.xp_kill", value: "1" },
  { key: "server.motd", value: "Hi" },
];

describe("dirtyKeys", () => {
  it("returns only keys whose edit differs from the live value", () => {
    expect(dirtyKeys(settings, {})).toEqual([]);
    expect(dirtyKeys(settings, { "rates.xp_kill": "1" })).toEqual([]);
    expect(dirtyKeys(settings, { "rates.xp_kill": "3" })).toEqual(["rates.xp_kill"]);
    expect(dirtyKeys(settings, { "rates.xp_kill": "3", "server.motd": "Yo" })).toEqual([
      "rates.xp_kill",
      "server.motd",
    ]);
  });
  it("ignores edits for keys that do not exist", () => {
    expect(dirtyKeys(settings, { ghost: "1" })).toEqual([]);
  });
});
```

- [ ] **Step 2: Run `npm test` — expect FAIL, then implement `launcher/src/lib/config-diff.ts`**

```typescript
export function dirtyKeys(
  settings: { key: string; value: string }[],
  edits: Record<string, string>,
): string[] {
  return settings
    .filter((s) => edits[s.key] !== undefined && edits[s.key] !== s.value)
    .map((s) => s.key);
}
```

Run `npm test` — expected PASS.

- [ ] **Step 3: Replace `launcher/src/lib/pages/Config.svelte`**

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import {
    wowConfigList,
    wowConfigSet,
    wowConfigRawRead,
    wowConfigRawWrite,
    gamesRestart,
    type ConfigSetting,
    type RawFileName,
  } from "$lib/api";
  import { dirtyKeys } from "$lib/config-diff";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";
  import CharPicker from "$lib/CharPicker.svelte";

  const WOW_ID = "wow-server-playerbots";
  const FILES: RawFileName[] = [
    ".env",
    "docker-compose.override.yml",
    "playerbots.conf",
    "mod_ahbot.conf",
    "mod_ale.conf",
  ];

  let tab: "settings" | "files" = $state("settings");
  let settings: ConfigSetting[] = $state([]);
  let edits: Record<string, string> = $state({});
  let error: string | null = $state(null);
  let saving = $state(false);
  let restartNeeded = $state(false);

  let file: RawFileName = $state(".env");
  let fileContent = $state("");
  let fileLoaded = $state(false);
  let lastBackup: string | null = $state(null);

  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);
  let restarting = $state(false);

  const groups = $derived([...new Set(settings.map((s) => s.group))]);
  const dirty = $derived(dirtyKeys(settings, edits));

  async function load() {
    error = null;
    try {
      settings = await wowConfigList();
      edits = {};
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  onMount(load);

  async function saveSettings(): Promise<boolean> {
    saving = true;
    error = null;
    try {
      for (const key of dirty) {
        const r = await wowConfigSet(key, edits[key]);
        if (r.restart_required) restartNeeded = true;
      }
      await load();
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
    }
  }

  async function loadFile() {
    error = null;
    fileLoaded = false;
    lastBackup = null;
    try {
      const r = await wowConfigRawRead(file);
      fileContent = r.content;
      fileLoaded = true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }

  async function saveFile(): Promise<boolean> {
    saving = true;
    error = null;
    try {
      const r = await wowConfigRawWrite(file, fileContent);
      lastBackup = r.backup;
      restartNeeded = true;
      return true;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return false;
    } finally {
      saving = false;
    }
  }

  let confirmingRestart = $state(false);
  async function saveAndRestart(saveFn: () => Promise<boolean>) {
    if (!confirmingRestart) {
      confirmingRestart = true;
      return;
    }
    confirmingRestart = false;
    if (!(await saveFn())) return;
    restarting = true;
    showTerm = true;
    term = initialTermState();
    try {
      await gamesRestart(WOW_ID, (e) => {
        term = applyEvent(term, e);
      });
      restartNeeded = false;
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      term = applyEvent(term, {
        event: "error",
        error: { code: err.code ?? "IPC", message: err.message ?? String(e), hint: err.hint ?? "" },
      });
    } finally {
      restarting = false;
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Config</h2>
    <div class="tabs">
      <button class:active={tab === "settings"} onclick={() => (tab = "settings")}>Settings</button>
      <button class:active={tab === "files"} onclick={() => (tab = "files")}>Files (Advanced)</button>
    </div>
  </header>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if restartNeeded}
    <div class="warn-card"><p>Saved — restart the server to apply the changes.</p></div>
  {/if}

  {#if tab === "settings"}
    {#each groups as g (g)}
      <h3>{g}</h3>
      {#each settings.filter((s) => s.group === g) as s (s.key)}
        <div class="setting" class:dirty={dirty.includes(s.key)}>
          <div class="meta">
            <strong>{s.label}</strong>
            <span class="muted">{s.explain}</span>
          </div>
          {#if s.type === "bool"}
            <input
              type="checkbox"
              checked={(edits[s.key] ?? s.value) === "1"}
              onchange={(e) => (edits[s.key] = e.currentTarget.checked ? "1" : "0")}
            />
          {:else if s.type === "float" || s.type === "int"}
            <input
              type="number"
              min={s.min}
              max={s.max}
              step={s.type === "float" ? "0.5" : "1"}
              value={edits[s.key] ?? s.value}
              oninput={(e) => (edits[s.key] = e.currentTarget.value)}
            />
          {:else if s.type === "char"}
            <div class="charwrap">
              <span class="muted">current id: {s.value}</span>
              <CharPicker
                selected={edits[s.key] ?? ""}
                bind:selected={
                  () => edits[s.key] ?? "",
                  (v) => (edits[s.key] = v)
                }
              />
            </div>
          {:else}
            <input
              value={edits[s.key] ?? s.value}
              oninput={(e) => (edits[s.key] = e.currentTarget.value)}
            />
          {/if}
        </div>
      {/each}
    {/each}
    <div class="row">
      <button class="primary" onclick={saveSettings} disabled={dirty.length === 0 || saving || restarting}>
        Save {dirty.length > 0 ? `(${dirty.length})` : ""}
      </button>
      <button onclick={() => saveAndRestart(saveSettings)} disabled={dirty.length === 0 || saving || restarting}>
        {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
      </button>
    </div>
  {:else}
    <div class="row">
      <select bind:value={file}>
        {#each FILES as f (f)}<option value={f}>{f}</option>{/each}
      </select>
      <button onclick={loadFile} disabled={saving || restarting}>Open</button>
    </div>
    {#if fileLoaded}
      <textarea rows="18" spellcheck="false" bind:value={fileContent}></textarea>
      {#if lastBackup}<p class="muted">Previous version kept as {lastBackup}</p>{/if}
      <div class="row">
        <button class="primary" onclick={saveFile} disabled={saving || restarting}>Save</button>
        <button onclick={() => saveAndRestart(saveFile)} disabled={saving || restarting}>
          {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
        </button>
      </div>
    {/if}
  {/if}

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .tabs button { background: none; border: 1px solid #30363d; color: #8b949e; border-radius: 6px 6px 0 0; padding: 6px 14px; cursor: pointer; }
  .tabs button.active { color: #f0f6fc; background: #161b22; }
  h3 { margin: 10px 0 0; font-size: 15px; color: #58a6ff; }
  .setting { display: flex; justify-content: space-between; align-items: center; gap: 16px; background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 10px 14px; }
  .setting.dirty { border-color: #d29922; }
  .meta { display: flex; flex-direction: column; gap: 2px; }
  .charwrap { display: flex; gap: 8px; align-items: center; }
  input, select, textarea { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  textarea { font-family: Consolas, monospace; font-size: 13px; width: 100%; box-sizing: border-box; }
  .row { display: flex; gap: 10px; align-items: center; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .warn-card { background: #161b22; border: 1px solid #d29922; border-radius: 8px; padding: 12px 16px; }
</style>
```

NOTE on the `char` input: Svelte 5 function-binding (`bind:selected={() => ..., (v) => ...}`) requires Svelte ≥5.9; if `npm run check` rejects it, fall back to a plain non-bound `CharPicker` wrapper: give CharPicker an optional `onpick?: (name: string) => void` callback prop invoked whenever `selected` changes (`$effect(() => onpick?.(selected))` inside CharPicker), and use `<CharPicker onpick={(v) => (edits[s.key] = v)} />` here. Either way the observable behavior is: picking a character stages `edits["ahbot.character"] = <name>`.

- [ ] **Step 4: Gates + commit**

Run: `npm run check` (0 errors / 0 warnings), `npm test` (green, includes config-diff).

```bash
git add launcher/src/lib/config-diff.ts launcher/src/lib/config-diff.test.ts launcher/src/lib/pages/Config.svelte
git commit -m "feat(launcher): Config page (curated settings + advanced files editor, Save & Restart)"
```

---

### Task 12: Docs, full gates, and the live click-through checklist

**Files:**
- Modify: `launcher/README.md` (pages section)
- Modify: `CLAUDE.md` (launcher section: pages now real; config editor shipped)

**Interfaces:** Consumes everything. Produces the user-facing docs + the go-live checklist.

- [ ] **Step 1: Update `launcher/README.md`**

Replace its feature description with the five-page reality. Add:

```markdown
## Pages

- **Library** — install status per game, Start/Stop with live terminal output.
- **Dashboard** — world up/down, uptime, players online, update-time stats;
  character viewer (level, gold, equipped gear as of the last save).
- **Item Database** — search `item_template` by name/quality/level; send any
  item to a character by in-game mail.
- **Teleport** — pick a character and one of the ~2000 named locations
  (two-step confirm).
- **Config** — Settings tab: curated server settings (XP/gold rates, bot
  population, bot autologin, AHBot, message of the day) with safe ranges,
  written as `AC_*` env vars into the wow title's compose override; Files tab
  (Advanced): direct editor for `.env`, the compose override (YAML-validated
  before save), and the module confs — every save keeps a `.bak`. Both tabs
  offer **Save** (shows a restart-needed banner) and **Save & Restart**
  (confirm → streams the restart into the terminal panel).
- **Playerbots** — disabled until the My Party feature (Plan 4).
```

- [ ] **Step 2: Update `CLAUDE.md`** — in the launcher section, replace the "only Library is real" wording with one line: sidebar pages Library/Dashboard/Items/Teleport/Config are live (components under `launcher/src/lib/pages/`, shell in `+page.svelte`); Playerbots stays disabled pending Plan 4; config editor writes `AC_*` env via `dml wow config` (registry in `cli/src/40-config.sh`).

- [ ] **Step 3: Full gate run**

- `wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"` → 118/118
- `cd launcher; npm test` → green; `npm run check` → 0/0
- `cd launcher/src-tauri; cargo test` → green (17)
- `cd launcher; npm run tauri build` → NSIS+MSI+exe produced (release build proves the full bundle still compiles)

- [ ] **Step 4: Commit**

```bash
git add launcher/README.md CLAUDE.md
git commit -m "docs: launcher pages + config editor documented; full gates green"
```

- [ ] **Step 5: USER-SUPERVISED LIVE GATE (do not mark the plan complete without it)**

With the real server running and the user present, in `launcher/` run `npm run tauri dev` and walk through:
1. Dashboard shows "World is up" with a plausible player count; character viewer renders Testen's gear.
2. Item Database: search "hearthstone", send 1 to Testen, confirm the mail arrives in-game.
3. Teleport: send Testen somewhere (e.g. Stormwind) with the two-step confirm; confirm in-game.
4. Config Settings: change "XP from kills" to 2, **Save & Restart**, watch the restart stream; after boot, confirm a kill grants doubled XP in-game; check `docker-compose.override.yml` gained `AC_RATE_XP_KILL: "2"`.
5. Config Files: open `playerbots.conf`, add a comment line, Save; verify the `.bak` exists next to it on the host.
6. Sidebar: Playerbots entry visibly disabled with the "coming with My Party" hint.

Record pass/fail per item in the SDD ledger. Any failure = fix task before merge consideration.
