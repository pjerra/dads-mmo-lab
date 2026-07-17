# My Party Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Per-bot control buttons (Gear up / Fix talents / Maintain) and party presets (save/list/delete/load with replace semantics) on the Playerbots page, powered by a new whisper-as-player Eluna bridge and a closed-allowlist CLI surface.

**Architecture:** Same relay as phase 1: page → typed Tauri command → `dml wow party …` → SOAP → Eluna hook 42. New bridge `dml_whisper` spoofs a player→bot whisper (the only door mod-playerbots opens). `party botcmd` maps a 3-action allowlist to fixed whisper strings; presets are per-file class lists under `~/.dml/party-presets/`, with `preset-load` streaming NDJSON (kick → add → wait-join → auto-talents+gear). The join-wait poll is extracted from the `party add` arm into a shared helper.

**Tech Stack:** Eluna Lua, bash CLI (built artifact `cli/dml`), bats, Rust/Tauri 2, Svelte 5 runes.

**Spec:** `docs/superpowers/specs/2026-07-17-my-party-phase2-design.md`

## Global Constraints

- Branch `feat/dml-launcher-windows`. Never merge; never push unless asked.
- NEVER hand-edit `cli/dml` — edit `cli/src/*.sh`, run `bash cli/build.sh`, commit both.
- CLI bash rules (`set -euo pipefail`): guard fallible command substitutions; helpers ending in a conditional need `return 0` (except validators whose exit status IS the signal, like `_valid_charname`); no `local` in the top-level dispatch case (helpers MAY use `local`).
- **No free-text whisper surface**: `botcmd` actions are a closed case allowlist — `gear`→`autogear`, `talents`→`talents autopick`, `maintain`→`maintenance`. These exact whisper strings are verified against this build's mod-playerbots chat-trigger registry.
- Character names via `_valid_charname` before any command string. Preset names via `^[A-Za-z0-9_-]{1,32}$` (no path characters possible).
- Bridge tokens fired bare: `dml_whisper <player> <bot> <msg…>`, `dml_addclass`, `dml_uninvite`. Token pins on both sides — never rename one side alone.
- Preset storage: `~/.dml/party-presets/<name>`, one class name per line (the names `party add --class` accepts). Classes re-validated on load (files can be hand-edited); unknown lines warn+skip.
- Payloads exactly: botcmd `{"sent":true,"player","bot","action"}`; preset-save `{"saved":true,"name","bots":[…],"overwrote":bool}`; preset-list `{"presets":[{"name","bots":N}]}`; preset-delete `{"deleted":true,"name"}`; preset-load done `{"loaded":true,"requested":N,"joined":M}`.
- Maintenance is NOT auto-run on preset load (it can walk bots to trainers mid-load).
- The join-wait extraction is a pure refactor: ALL existing wow-party.bats tests must pass unchanged — they are the regression net.
- `launcher/src-tauri/Cargo.toml` ghost (if it reappears): NEVER stage. Committed blobs LF. bats runs inside dml-arch WSL (`wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"`); known DrvFs flake — re-run once.
- Test baselines before this plan: bats 195 (party-lua 8, wow-party 19, wow-party-setup 5, wow-gm 23, gm-lua 13), vitest 19, cargo 17, check 0/0.
- User-facing copy verbatim where given.

---

### Task 1: The whisper bridge — `cli/lua/party/dml_whisper.lua`

**Files:**
- Create: `cli/lua/party/dml_whisper.lua`
- Test: `cli/tests/party-lua.bats` (append; the file's `LUA_DIR` var already points at `../lua/party`)

**Interfaces:**
- Consumes: nothing (standalone; `bridge-setup` deploys the whole `party/` dir automatically).
- Produces: console/SOAP command `dml_whisper <playerName> <botName> <message...>` (Tasks 3 and 5 fire it).

- [ ] **Step 1: Append the failing tests**

Append to `cli/tests/party-lua.bats`:

```bats
# ---------- dml_whisper (My Party phase 2) ----------

@test "whisper bridge exists with an AGPL/Dad's MMO Lab header" {
  [ -f "$LUA_DIR/dml_whisper.lua" ]
  grep -qi 'AGPL' "$LUA_DIR/dml_whisper.lua"
  grep -qiE "Dad's MMO Lab" "$LUA_DIR/dml_whisper.lua"
}

@test "whisper bridge registers hook 42 and gates BEFORE matching" {
  grep -q 'RegisterPlayerEvent(42,' "$LUA_DIR/dml_whisper.lua"
  gate_line=$(grep -n 'if player ~= nil then return end' "$LUA_DIR/dml_whisper.lua" | head -1 | cut -d: -f1)
  match_line=$(grep -n 'command:match' "$LUA_DIR/dml_whisper.lua" | head -1 | cut -d: -f1)
  [ "$gate_line" -lt "$match_line" ]
}

@test "whisper bridge pins the dml_whisper token with a greedy message capture" {
  grep -q 'dml_whisper%s' "$LUA_DIR/dml_whisper.lua"
  grep -qF '(.+)$' "$LUA_DIR/dml_whisper.lua"
}

@test "whisper bridge resolves BOTH the player and the bot" {
  [ "$(grep -c 'GetPlayerByName' "$LUA_DIR/dml_whisper.lua")" -ge 2 ]
}

@test "whisper bridge sends via Player:Whisper (universal language)" {
  grep -q 'Whisper(msg, 0, b)' "$LUA_DIR/dml_whisper.lua"
}

@test "whisper bridge handler returns false to consume the command" {
  grep -q 'return false' "$LUA_DIR/dml_whisper.lua"
}
```

- [ ] **Step 2: Run to verify failure**

Run (dml-arch WSL): `bats tests/party-lua.bats` — Expected: the 6 new FAIL (file missing), the 8 old PASS.

- [ ] **Step 3: Write the bridge**

Create `cli/lua/party/dml_whisper.lua`:

```lua
--[[
    dml_whisper.lua -- Dad's MMO Lab launcher whisper-as-player bridge.
    License: AGPL-3.0-only (same as the repo).
    Reimplemented for DML; behavioral reference: The Lab's whisper relay.
    See docs/superpowers/specs/2026-07-17-my-party-phase2-design.md.

    One console/SOAP-only command:

        dml_whisper <playerName> <botName> <message...>

    Sends <message> as a /whisper FROM the player TO the bot, exactly as
    if the player had typed it. mod-playerbots accepts its bot commands
    (autogear, talents autopick, maintenance, ...) only as whispers from
    a player session -- SOAP has no way to spoof player chat; Eluna's
    Player:Whisper does (it routes through core Player::Whisper, which
    fires the module's chat hook).
]]--

local function OnWhisperCommand(event, player, command)
    -- Console/SOAP origin only: chat parses always carry a non-nil
    -- player, so in-game chat can never trigger this.
    if player ~= nil then return end

    -- Greedy third capture: the message may contain spaces.
    local pname, bname, msg = command:match("^dml_whisper%s+(%S+)%s+(%S+)%s+(.+)$")
    if not pname then return end

    local p = GetPlayerByName(pname)
    if not p then
        print(string.format("[dml_whisper] player not online: %s", pname))
        return false
    end
    local b = GetPlayerByName(bname)
    if not b then
        print(string.format("[dml_whisper] bot not online: %s", bname))
        return false
    end

    -- Language 0 = universal (core forces whispers universal anyway).
    p:Whisper(msg, 0, b)
    print(string.format("[dml_whisper] %s -> %s: %s", pname, bname, msg))
    return false
end

RegisterPlayerEvent(42, OnWhisperCommand)
print("[dml_whisper] loaded")
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bats tests/party-lua.bats` — Expected: 14/14.
Run: `bats tests/` — Expected: 201/201 (195 + 6).

- [ ] **Step 5: Verify LF, commit**

`git add cli/lua/party/dml_whisper.lua cli/tests/party-lua.bats`; commit `feat(cli): AGPL whisper-as-player Eluna bridge`. Verify no CRLF in the committed blob.

---

### Task 2: Shared helpers — join-wait extraction + class-id map (pure refactor + additions)

**Files:**
- Modify: `cli/src/50-party.sh` (two new helpers), `cli/src/90-main.sh` (the `add)` arm's inline poll block replaced by a helper call)
- Test: existing `cli/tests/wow-party.bats` unchanged (the regression net)

**Interfaces:**
- Consumes: `_party_group_member_guids` (existing).
- Produces (Tasks 4–5 use): `_party_wait_new_member <pguid> <before-guids-space-separated>` → echoes the new member guid or "" (poll seams `DML_PARTY_POLL_TRIES`/`DML_PARTY_POLL_SLEEP` preserved); `_class_name_from_id <id>` → echoes warrior|paladin|hunter|rogue|priest|shaman|mage|warlock|druid or "" for unsupported ids (6 = DK); `_preset_dir` → `$HOME/.dml/party-presets`; `_valid_preset_name <name>` → exit status (regex `^[A-Za-z0-9_-]{1,32}$`).

- [ ] **Step 1: Add the helpers to `cli/src/50-party.sh`**

Append:

```bash
# Poll group membership until a NEW member (one not in $2, a space-
# separated guid snapshot) appears for player guid $1; echo the new guid
# or "" on timeout. Seams: DML_PARTY_POLL_TRIES (12) / _SLEEP (0.5).
_party_wait_new_member() {
    local pguid="$1" before="$2" tries slp newguid i now g
    tries="${DML_PARTY_POLL_TRIES:-12}"; slp="${DML_PARTY_POLL_SLEEP:-0.5}"
    newguid=""; i=0
    while (( i < tries )); do
        now="$(_party_group_member_guids "$pguid" | tr '\n' ' ')"
        for g in $now; do
            [[ "$g" == "$pguid" ]] && continue
            case " $before " in *" $g "*) : ;; *) newguid="$g"; break ;; esac
        done
        [[ -n "$newguid" ]] && break
        i=$(( i + 1 ))
        [[ "$slp" != "0" ]] && sleep "$slp"
    done
    echo "$newguid"
    return 0
}

# characters.class id -> the class name `party add --class` accepts.
# Unsupported ids (6 = deathknight) echo "" -- callers skip those.
_class_name_from_id() {
    case "$1" in
      1) echo warrior ;; 2) echo paladin ;; 3) echo hunter ;; 4) echo rogue ;;
      5) echo priest ;; 7) echo shaman ;; 8) echo mage ;; 9) echo warlock ;;
      11) echo druid ;; *) echo "" ;;
    esac
    return 0
}

_preset_dir() { echo "$HOME/.dml/party-presets"; }

# Exit status IS the signal (same pattern as _valid_charname).
_valid_preset_name() { [[ "$1" =~ ^[A-Za-z0-9_-]{1,32}$ ]]; }
```

- [ ] **Step 2: Replace the inline poll in the `add)` arm**

In `cli/src/90-main.sh`'s `add)` arm, replace exactly this block:

```bash
            tries="${DML_PARTY_POLL_TRIES:-12}"; slp="${DML_PARTY_POLL_SLEEP:-0.5}"
            newguid=""; i=0
            while (( i < tries )); do
              now="$(_party_group_member_guids "$pguid" | tr '\n' ' ')"
              for g in $now; do
                [[ "$g" == "$pguid" ]] && continue
                case " $before " in *" $g "*) : ;; *) newguid="$g"; break ;; esac
              done
              [[ -n "$newguid" ]] && break
              i=$(( i + 1 ))
              [[ "$slp" != "0" ]] && sleep "$slp"
            done
```

with:

```bash
            newguid="$(_party_wait_new_member "$pguid" "$before")"
```

Nothing else in the arm changes.

- [ ] **Step 3: Rebuild + regression net**

Run: `bash build.sh && bats tests/wow-party.bats` — Expected: 19/19 UNCHANGED (this is the point of the task).
Run: `bats tests/` — Expected: 201/201.

- [ ] **Step 4: Commit**

`git add cli/src/50-party.sh cli/src/90-main.sh cli/dml`; commit `refactor(cli): extract party join-wait into a shared helper; preset/class helpers`.

---

### Task 3: `party botcmd`

**Files:**
- Modify: `cli/src/90-main.sh` (new `botcmd)` arm inside the `party)` psub dispatch, after `relogin)`, before the party `*)`; also extend the party `*)` hint to mention botcmd)
- Test: `cli/tests/wow-party.bats` (append)

**Interfaces:**
- Consumes: `_valid_charname`, `_need_flag_val`, `_party_online_guid`, `_party_fire`, `json_ok`/`json_err`/`json_escape`; Task 1's bridge token.
- Produces: `dml wow party botcmd --player X --bot B --action gear|talents|maintain --json` → `{"sent":true,"player":"X","bot":"B","action":"gear"}` (Task 6 wraps it).

- [ ] **Step 1: Append the failing tests to `cli/tests/wow-party.bats`**

```bats
# ---------- botcmd (My Party phase 2) ----------

@test "party botcmd fires the exact whisper string for each action" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action gear --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.sent')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.action')" = "gear" ]
  grep -q 'dml_whisper Testen Botmage autogear' "$FIXTURE/cap.txt"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action talents --json
  [ "$status" -eq 0 ]
  grep -q 'dml_whisper Testen Botmage talents autopick' "$FIXTURE/cap.txt"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action maintain --json
  [ "$status" -eq 0 ]
  grep -q 'dml_whisper Testen Botmage maintenance' "$FIXTURE/cap.txt"
}

@test "party botcmd rejects an unknown action with the allowlist hint" {
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action dance --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  echo "$output" | grep -q 'gear talents maintain'
}

@test "party botcmd rejects invalid player and bot names" {
  run bash "$DML" wow party botcmd --player 'x; drop' --bot Botmage --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  run bash "$DML" wow party botcmd --player Testen --bot 'x; drop' --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "party botcmd offline player maps to NOT_FOUND naming the player" {
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party botcmd --player Ghost --bot Botmage --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -q 'Ghost'
}

@test "party botcmd offline bot maps to NOT_FOUND with the party hint" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/none.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/none.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party botcmd --player Testen --bot Goneb --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -q 'Goneb'
  echo "$output" | grep -qi 'party'
}

@test "party botcmd maps a SOAP fault to SOAP_FAULT with the bridge-setup hint" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow party botcmd --player Testen --bot Botmage --action gear --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
  echo "$output" | grep -q 'bridge-setup'
}
```

- [ ] **Step 2: Run to verify failure**

Run: `bats tests/wow-party.bats` — Expected: the 6 new FAIL (UNKNOWN_COMMAND), the 19 old PASS.

- [ ] **Step 3: Add the `botcmd)` arm**

Inside the `party)` psub dispatch (after `relogin)`'s `;;`, before the `*)`; bare variables, no `local`):

```bash
          botcmd)
            player=""; bot=""; action=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --bot) _need_flag_val "$1" $#; bot="$2"; shift 2 ;;
                --action) _need_flag_val "$1" $#; action="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_charname "$bot" || { json_err BAD_ARG "Invalid bot name: $bot" ""; exit 1; }
            # Closed allowlist -> fixed whisper strings. This is the whole
            # whisper surface: no free-text path exists.
            case "$action" in
              gear) wmsg="autogear" ;;
              talents) wmsg="talents autopick" ;;
              maintain) wmsg="maintenance" ;;
              *) json_err BAD_ARG "Invalid action: $action" "One of: gear talents maintain"; exit 1 ;;
            esac
            pguid="$(_party_online_guid "$player")"
            [[ "$pguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $player" "Log the character into the game first."; exit 1; }
            bguid="$(_party_online_guid "$bot")"
            [[ "$bguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $bot" "The bot must be in the world -- is it still in your party?"; exit 1; }
            _party_fire "dml_whisper $player $bot $wmsg" "botcmd"
            json_ok "{\"sent\":true,\"player\":\"$(json_escape "$player")\",\"bot\":\"$(json_escape "$bot")\",\"action\":\"$action\"}"
            ;;
```

Update the party `*)` hint to: `Try: dml wow party online|add|list|kick|relogin|botcmd|preset-save|preset-list|preset-delete|preset-load --json`.

- [ ] **Step 4: Rebuild + test**

Run: `bash build.sh && bats tests/wow-party.bats` — Expected: 25/25.
Run: `bats tests/` — Expected: 207/207.

- [ ] **Step 5: Commit**

`git add cli/src/90-main.sh cli/dml cli/tests/wow-party.bats`; commit `feat(cli): party botcmd (closed gear/talents/maintain whisper allowlist)`.

---

### Task 4: `preset-save`, `preset-list`, `preset-delete`

**Files:**
- Modify: `cli/src/90-main.sh` (three arms inside the `party)` psub dispatch, after `botcmd)`)
- Test: Create `cli/tests/wow-party-presets.bats`

**Interfaces:**
- Consumes: `_class_name_from_id`, `_preset_dir`, `_valid_preset_name` (Task 2), `_party_online_guid`, `db_chars_query`, json helpers.
- Produces: the three verbs with the payloads pinned in Global Constraints. Storage format (Task 5 reads it): one class name per line at `$(_preset_dir)/<name>`.

- [ ] **Step 1: Create `cli/tests/wow-party-presets.bats` with the failing tests**

```bats
#!/usr/bin/env bats
load helpers/env.bash

setup() {
  DML="$BATS_TEST_DIRNAME/../dml"
  bash "$BATS_TEST_DIRNAME/../build.sh" >/dev/null
  make_fixture
  use_mysql_stub
  export HOME="$FIXTURE"   # sandboxes ~/.dml/party-presets
  PDIR="$FIXTURE/.dml/party-presets"
}
teardown() { teardown_fixture; }

@test "preset-save snapshots bot classes to a file and reports them" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '8\n5\n' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name dungeon-crew --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.saved')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.overwrote')" = "false" ]
  [ "$(echo "$output" | jq -r '.data.bots | join(",")')" = "mage,priest" ]
  [ "$(cat "$PDIR/dungeon-crew")" = "mage
priest" ]
}

@test "preset-save skips unsupported class ids (deathknight 6)" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '8\n6\n5\n' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name crew --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.bots | length')" = "2" ]
  [ "$(grep -c . "$PDIR/crew")" = "2" ]
}

@test "preset-save over an existing name reports overwrote:true" {
  mkdir -p "$PDIR"; printf 'warrior\n' > "$PDIR/crew"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '8\n' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name crew --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.overwrote')" = "true" ]
  [ "$(cat "$PDIR/crew")" = "mage" ]
}

@test "preset-save with no bots in the party maps to NOT_FOUND" {
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/classes.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/classes.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  run bash "$DML" wow party preset-save --player Testen --name crew --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "preset-save rejects bad preset names" {
  for bad in 'a b' 'x;y' '../etc' 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa'; do
    run bash "$DML" wow party preset-save --player Testen --name "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "preset-save offline player maps to NOT_FOUND" {
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  run bash "$DML" wow party preset-save --player Ghost --name crew --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}

@test "preset-list is empty when nothing is saved, then lists name+count" {
  run bash "$DML" wow party preset-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.presets | length')" = "0" ]
  mkdir -p "$PDIR"; printf 'mage\npriest\nwarrior\n' > "$PDIR/trio"
  run bash "$DML" wow party preset-list --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.presets[0].name')" = "trio" ]
  [ "$(echo "$output" | jq -r '.data.presets[0].bots')" = "3" ]
}

@test "preset-delete removes the file; deleting a missing preset maps to NOT_FOUND" {
  mkdir -p "$PDIR"; printf 'mage\n' > "$PDIR/tmp1"
  run bash "$DML" wow party preset-delete --name tmp1 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.deleted')" = "true" ]
  [ ! -f "$PDIR/tmp1" ]
  run bash "$DML" wow party preset-delete --name tmp1 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
}
```

- [ ] **Step 2: Run to verify failure**

Run: `bats tests/wow-party-presets.bats` — Expected: 8 FAIL (UNKNOWN_COMMAND).

- [ ] **Step 3: Add the three arms** (after `botcmd)`, bare variables)

```bash
          preset-save)
            player=""; name=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --name) _need_flag_val "$1" $#; name="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" "Letters, digits, - and _ (max 32)."; exit 1; }
            pguid="$(_party_online_guid "$player")"
            [[ "$pguid" =~ ^[0-9]+$ ]] || { json_err NOT_FOUND "Character not online: $player" "Log the character into the game first."; exit 1; }
            sql="SELECT c.class
                 FROM group_member gm
                 JOIN characters c ON c.guid = gm.memberGuid
                 WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=$pguid LIMIT 1)
                   AND c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))
                 ORDER BY c.name;"
            rows="$(db_chars_query "$sql")" \
              || { json_err DB_UNREACHABLE "Could not read the party" "Is ac-database running?"; exit 1; }
            names=""
            while IFS=$'\t' read -r cls || [[ -n "$cls" ]]; do
              [[ -z "$cls" ]] && continue
              cname="$(_class_name_from_id "$cls")"
              [[ -n "$cname" ]] && names+="$cname"$'\n'
            done <<< "$rows"
            [[ -n "$names" ]] || { json_err NOT_FOUND "Party has no bots to save" "Add some bots first."; exit 1; }
            pdir="$(_preset_dir)"; mkdir -p "$pdir"
            overwrote=false; [[ -f "$pdir/$name" ]] && overwrote=true
            printf '%s' "$names" > "$pdir/$name"
            jarr=""; first=1
            while IFS= read -r n || [[ -n "$n" ]]; do
              [[ -z "$n" ]] && continue
              [[ $first -eq 0 ]] && jarr+=','
              jarr+="\"$n\""; first=0
            done <<< "$names"
            json_ok "{\"saved\":true,\"name\":\"$name\",\"bots\":[$jarr],\"overwrote\":$overwrote}"
            ;;
          preset-list)
            pdir="$(_preset_dir)"
            first=1; out='['
            if [[ -d "$pdir" ]]; then
              for f in "$pdir"/*; do
                [[ -f "$f" ]] || continue
                pname="$(basename "$f")"
                _valid_preset_name "$pname" || continue
                cnt="$(grep -c . "$f" 2>/dev/null)" || cnt=0
                [[ $first -eq 0 ]] && out+=','
                out+="{\"name\":\"$pname\",\"bots\":$cnt}"
                first=0
              done
            fi
            out+=']'
            json_ok "{\"presets\":$out}"
            ;;
          preset-delete)
            name=""
            [[ "${1:-}" == "--name" ]] && { _need_flag_val "$1" $#; name="$2"; shift 2; }
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" ""; exit 1; }
            pdir="$(_preset_dir)"
            [[ -f "$pdir/$name" ]] || { json_err NOT_FOUND "No preset named $name" ""; exit 1; }
            rm -f "$pdir/$name"
            json_ok "{\"deleted\":true,\"name\":\"$name\"}"
            ;;
```

- [ ] **Step 4: Rebuild + test**

Run: `bash build.sh && bats tests/wow-party-presets.bats` — Expected: 8/8.
Run: `bats tests/` — Expected: 215/215 (207 + 8).

- [ ] **Step 5: Commit**

`git add cli/src/90-main.sh cli/dml cli/tests/wow-party-presets.bats`; commit `feat(cli): party preset save/list/delete (per-file class lists)`.

---

### Task 5: `preset-load` (NDJSON streaming) + capture-append test seam

**Files:**
- Modify: `cli/src/90-main.sh` (the `preset-load)` arm, after `preset-delete)`), `cli/tests/helpers/env.bash` (additive capture-append seam)
- Test: `cli/tests/wow-party-presets.bats` (append)

**Interfaces:**
- Consumes: Tasks 1–4's pieces (`dml_uninvite`/`dml_addclass`/`dml_whisper` tokens, `_party_wait_new_member`, `_preset_dir`, `_valid_preset_name`, poll seams, `ndjson_*` emitters used by bridge-setup).
- Produces: `dml wow party preset-load --player X --name N --json` NDJSON stream; done `{"loaded":true,"requested":N,"joined":M}` (Task 6 streams it).

- [ ] **Step 1: Add the capture-append seam to the curl stub**

In `cli/tests/helpers/env.bash`'s `use_curl_stub` heredoc, replace:

```bash
if [[ -n "${DML_STUB_CAPTURE:-}" ]]; then
  cat > "$DML_STUB_CAPTURE"
else
  cat >/dev/null
fi
```

with:

```bash
if [[ -n "${DML_STUB_CAPTURE_APPEND:-}" ]]; then
  cat >> "$DML_STUB_CAPTURE_APPEND"
elif [[ -n "${DML_STUB_CAPTURE:-}" ]]; then
  cat > "$DML_STUB_CAPTURE"
else
  cat >/dev/null
fi
```

(Additive: no existing test sets the new variable.)

- [ ] **Step 2: Append the failing tests to `cli/tests/wow-party-presets.bats`**

```bats
# ---------- preset-load (streaming) ----------

_done_data() { echo "$1" | grep '"event":"done"' | tail -1; }

@test "preset-load kicks current bots, adds each class, preps each joiner" {
  mkdir -p "$PDIR"; printf 'mage\npriest\n' > "$PDIR/crew"
  # SEQ call order: online-guid, kick-list (one old bot), then per class:
  # before-snapshot, wait-poll, joiner-name.
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf 'Oldbot\n' > "$FIXTURE/kicklist.tsv"
  printf '2503\n' > "$FIXTURE/before1.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after1.tsv"
  printf 'Botmage\n' > "$FIXTURE/name1.tsv"
  printf '2503\n9001\n' > "$FIXTURE/before2.tsv"
  printf '2503\n9001\n9002\n' > "$FIXTURE/after2.tsv"
  printf 'Botpriest\n' > "$FIXTURE/name2.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/kicklist.tsv $FIXTURE/before1.tsv $FIXTURE/after1.tsv $FIXTURE/name1.tsv $FIXTURE/before2.tsv $FIXTURE/after2.tsv $FIXTURE/name2.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE_APPEND="$FIXTURE/allcaps.txt"
  run bash "$DML" wow party preset-load --player Testen --name crew --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.loaded')" = "true" ]
  [ "$(echo "$d" | jq -r '.data.requested')" = "2" ]
  [ "$(echo "$d" | jq -r '.data.joined')" = "2" ]
  grep -q 'dml_uninvite Oldbot' "$FIXTURE/allcaps.txt"
  grep -q 'dml_addclass Testen mage' "$FIXTURE/allcaps.txt"
  grep -q 'dml_addclass Testen priest' "$FIXTURE/allcaps.txt"
  grep -q 'dml_whisper Testen Botmage talents autopick' "$FIXTURE/allcaps.txt"
  grep -q 'dml_whisper Testen Botmage autogear' "$FIXTURE/allcaps.txt"
  grep -q 'dml_whisper Testen Botpriest talents autopick' "$FIXTURE/allcaps.txt"
}

@test "preset-load missing preset emits a NOT_FOUND error event" {
  printf '2503\n' > "$FIXTURE/guid.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/guid.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Testen --name nosuch --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"event":"error"'
  echo "$output" | grep -q '"code":"NOT_FOUND"'
}

@test "preset-load offline player emits a NOT_FOUND error event" {
  mkdir -p "$PDIR"; printf 'mage\n' > "$PDIR/crew"
  printf '' > "$FIXTURE/none.tsv"; export DML_STUB_DB_ROWS="$FIXTURE/none.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Ghost --name crew --json
  [ "$status" -eq 1 ]
  echo "$output" | grep -q '"code":"NOT_FOUND"'
}

@test "preset-load counts a non-attaching class as requested but not joined (warn path)" {
  mkdir -p "$PDIR"; printf 'mage\n' > "$PDIR/solo"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/kicklist.tsv"
  printf '2503\n' > "$FIXTURE/before1.tsv"
  printf '2503\n' > "$FIXTURE/after1.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/kicklist.tsv $FIXTURE/before1.tsv $FIXTURE/after1.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Testen --name solo --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.requested')" = "1" ]
  [ "$(echo "$d" | jq -r '.data.joined')" = "0" ]
  echo "$output" | grep -q '"level":"warn"'
}

@test "preset-load warns and skips unknown class lines (hand-edited file)" {
  mkdir -p "$PDIR"; printf 'necromancer\nmage\n' > "$PDIR/weird"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  printf '' > "$FIXTURE/kicklist.tsv"
  printf '2503\n' > "$FIXTURE/before1.tsv"
  printf '2503\n9001\n' > "$FIXTURE/after1.tsv"
  printf 'Botmage\n' > "$FIXTURE/name1.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/guid.tsv $FIXTURE/kicklist.tsv $FIXTURE/before1.tsv $FIXTURE/after1.tsv $FIXTURE/name1.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  export DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow party preset-load --player Testen --name weird --json
  [ "$status" -eq 0 ]
  d="$(_done_data "$output")"
  [ "$(echo "$d" | jq -r '.data.requested')" = "1" ]
  echo "$output" | grep -q 'necromancer'
}
```

- [ ] **Step 3: Run to verify failure**

Run: `bats tests/wow-party-presets.bats` — Expected: the 5 new FAIL, the 8 old PASS.

- [ ] **Step 4: Add the `preset-load)` arm** (after `preset-delete)`; bare variables; flag/name/player validation BEFORE any NDJSON output)

```bash
          preset-load)
            player=""; name=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --name) _need_flag_val "$1" $#; name="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            _valid_preset_name "$name" || { json_err BAD_ARG "Invalid preset name: $name" ""; exit 1; }
            [[ "$DML_JSON" == 1 ]] && ndjson_section_start preset-load
            pdir="$(_preset_dir)"
            if [[ ! -f "$pdir/$name" ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end preset-load error
                ndjson_error NOT_FOUND "No preset named $name" ""
              else echo "[dml] ERROR: no preset $name" >&2; fi
              exit 1
            fi
            pguid="$(_party_online_guid "$player")"
            if ! [[ "$pguid" =~ ^[0-9]+$ ]]; then
              if [[ "$DML_JSON" == 1 ]]; then
                ndjson_section_end preset-load error
                ndjson_error NOT_FOUND "Character not online: $player" "Log the character into the game first."
              else echo "[dml] ERROR: $player not online" >&2; fi
              exit 1
            fi
            # Kick phase (replace semantics): every current bot goes.
            sql="SELECT c.name
                 FROM group_member gm
                 JOIN characters c ON c.guid = gm.memberGuid
                 WHERE gm.guid = (SELECT guid FROM group_member WHERE memberGuid=$pguid LIMIT 1)
                   AND c.account IN (SELECT account_id FROM acore_playerbots.playerbots_account_type WHERE account_type IN (1,2))
                 ORDER BY c.name;"
            kicklist="$(db_chars_query "$sql")" || kicklist=""
            while IFS= read -r b || [[ -n "$b" ]]; do
              [[ -z "$b" ]] && continue
              if out="$(soap_exec "dml_uninvite $b")"; then
                [[ "$DML_JSON" == 1 ]] && ndjson_line info "kicked $b"
              else
                [[ "$DML_JSON" == 1 ]] && ndjson_line warn "could not kick $b"
              fi
            done <<< "$kicklist"
            requested=0; joined=0
            while IFS= read -r cls || [[ -n "$cls" ]]; do
              [[ -z "$cls" ]] && continue
              case "$cls" in
                warrior|paladin|hunter|rogue|priest|shaman|mage|warlock|druid) : ;;
                *) [[ "$DML_JSON" == 1 ]] && ndjson_line warn "skipping unknown class: $cls"; continue ;;
              esac
              requested=$(( requested + 1 ))
              before="$(_party_group_member_guids "$pguid" | tr '\n' ' ')"
              if out="$(soap_exec "dml_addclass $player $cls")"; then :; else
                [[ "$DML_JSON" == 1 ]] && ndjson_line warn "add $cls was rejected"
                continue
              fi
              newguid="$(_party_wait_new_member "$pguid" "$before")"
              if [[ "$newguid" =~ ^[0-9]+$ ]]; then
                joined=$(( joined + 1 ))
                bname="$(db_chars_query "SELECT name FROM characters WHERE guid=$newguid LIMIT 1;" 2>/dev/null)" || bname=""
                if [[ -n "$bname" ]]; then
                  out="$(soap_exec "dml_whisper $player $bname talents autopick")" || true
                  out="$(soap_exec "dml_whisper $player $bname autogear")" || true
                  [[ "$DML_JSON" == 1 ]] && ndjson_line info "$bname joined -- talents + gear applied"
                else
                  [[ "$DML_JSON" == 1 ]] && ndjson_line info "a $cls joined"
                fi
              else
                [[ "$DML_JSON" == 1 ]] && ndjson_line warn "$cls did not attach in time"
              fi
            done < "$pdir/$name"
            if [[ "$DML_JSON" == 1 ]]; then
              ndjson_section_end preset-load ok
              ndjson_done "{\"loaded\":true,\"requested\":$requested,\"joined\":$joined}"
            else
              echo "[dml] preset-load done ($joined/$requested joined)"
            fi
            ;;
```

- [ ] **Step 5: Rebuild + test**

Run: `bash build.sh && bats tests/wow-party-presets.bats` — Expected: 13/13.
Run: `bats tests/` — Expected: 220/220 (215 + 5).

- [ ] **Step 6: Commit**

`git add cli/src/90-main.sh cli/dml cli/tests/wow-party-presets.bats cli/tests/helpers/env.bash`; commit `feat(cli): party preset-load (streamed kick/add/prep, replace semantics)`.

---

### Task 6: Rust commands + api.ts wrappers

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (five commands after `wow_gm_summon`, registered in `generate_handler!`), `launcher/src/lib/api.ts` (append after `wowGmSummon`)

**Interfaces:**
- Consumes: `run_json_cmd`, `stream_args`; Tasks 3–5's verbs.
- Produces (Task 7 imports): TS `BotcmdResult {sent, player, bot, action}`, `PresetInfo {name, bots}`, `PresetSaveResult {saved, name, bots: string[], overwrote}`; `wowPartyBotcmd(player, bot, action)`, `wowPartyPresetSave(player, name)`, `wowPartyPresetList(): Promise<PresetInfo[]>`, `wowPartyPresetDelete(name)`, `wowPartyPresetLoad(player, name, onEvent)`.

- [ ] **Step 1: Rust**

```rust
#[tauri::command]
async fn wow_party_botcmd(player: String, bot: String, action: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "party".into(), "botcmd".into(), "--player".into(), player, "--bot".into(), bot, "--action".into(), action],
    )
    .await
}

#[tauri::command]
async fn wow_party_preset_save(player: String, name: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "party".into(), "preset-save".into(), "--player".into(), player, "--name".into(), name],
    )
    .await
}

#[tauri::command]
async fn wow_party_preset_list(state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "preset-list".into()]).await
}

#[tauri::command]
async fn wow_party_preset_delete(name: String, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(state, vec!["wow".into(), "party".into(), "preset-delete".into(), "--name".into(), name]).await
}

#[tauri::command]
async fn wow_party_preset_load(player: String, name: String, on_event: Channel<serde_json::Value>, state: State<'_, AppState>) -> Result<(), CmdError> {
    stream_args(
        vec!["wow".into(), "party".into(), "preset-load".into(), "--player".into(), player, "--name".into(), name],
        on_event,
        state,
    )
    .await
}
```

Register all five in `generate_handler![...]`.

- [ ] **Step 2: api.ts**

```ts
export interface BotcmdResult { sent: boolean; player: string; bot: string; action: string; }
export interface PresetInfo { name: string; bots: number; }
export interface PresetSaveResult { saved: boolean; name: string; bots: string[]; overwrote: boolean; }

export async function wowPartyBotcmd(player: string, bot: string, action: "gear" | "talents" | "maintain"): Promise<BotcmdResult> {
  return await invoke("wow_party_botcmd", { player, bot, action });
}
export async function wowPartyPresetSave(player: string, name: string): Promise<PresetSaveResult> {
  return await invoke("wow_party_preset_save", { player, name });
}
export async function wowPartyPresetList(): Promise<PresetInfo[]> {
  const d = await invoke<{ presets: PresetInfo[] }>("wow_party_preset_list");
  return d.presets;
}
export async function wowPartyPresetDelete(name: string): Promise<{ deleted: boolean; name: string }> {
  return await invoke("wow_party_preset_delete", { name });
}
export const wowPartyPresetLoad = (player: string, name: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_party_preset_load", { player, name, onEvent: ch });
};
```

- [ ] **Step 3: Gates**

`cargo test` 17/17 zero warnings; `npm run check` 0/0; `npm test` 19/19.

- [ ] **Step 4: Commit**

`git add launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts`; commit `feat(launcher): party botcmd + preset commands and wrappers`. Verify LF.

---

### Task 7: Playerbots page — per-bot buttons + presets card

**Files:**
- Modify: `launcher/src/lib/pages/Playerbots.svelte`

**Interfaces:**
- Consumes: Task 6's wrappers/types; the page's existing `player`/`members`/`busy`/`setting`/`showErr`/`refresh`/Terminal machinery.
- Produces: the user-facing UI. No nav change (existing page).

- [ ] **Step 1: Script additions**

Extend the `$lib/api` import with `wowPartyBotcmd, wowPartyPresetSave, wowPartyPresetList, wowPartyPresetDelete, wowPartyPresetLoad, type PresetInfo`.

Add state after `let confirmSetup = $state(false);`:

```ts
  let presets: PresetInfo[] = $state([]);
  let presetName = $state("");
  let loadingPreset = $state(false);
  let confirmingPreset: { kind: "load" | "delete"; name: string } | null = $state(null);
```

Add after `refresh()` (and call `refreshPresets()` from `onMount` alongside `refresh`):

```ts
  async function refreshPresets() {
    try { presets = await wowPartyPresetList(); } catch (e) { showErr(e); }
  }
```

Change the existing `onMount(refresh);` line to:

```ts
  onMount(() => { refresh(); refreshPresets(); });
```

Add after `resummon(...)`:

```ts
  const BOTCMD_PHRASE = { gear: "gear up", talents: "fix its talents", maintain: "do maintenance" } as const;
  async function botcmd(bot: string, action: "gear" | "talents" | "maintain") {
    const p = player;
    busy = true; error = null; note = null;
    try {
      await wowPartyBotcmd(p, bot, action);
      note = `Told ${bot} to ${BOTCMD_PHRASE[action]} — give it a moment.`;
    } catch (e) { showErr(e); } finally { busy = false; }
  }

  async function savePreset() {
    const p = player; const n = presetName.trim();
    if (!n) return;
    busy = true; error = null; note = null;
    try {
      const r = await wowPartyPresetSave(p, n);
      note = `Saved preset "${r.name}" (${r.bots.length} bots${r.overwrote ? ", replaced the old one" : ""}).`;
      await refreshPresets();
    } catch (e) { showErr(e); } finally { busy = false; }
  }

  async function loadPreset(name: string) {
    if (confirmingPreset?.kind !== "load" || confirmingPreset?.name !== name) {
      confirmingPreset = { kind: "load", name };
      return;
    }
    confirmingPreset = null;
    const p = player;
    loadingPreset = true; error = null; note = null; showTerm = true; term = initialTermState();
    let requested = 0, joined = 0;
    try {
      await wowPartyPresetLoad(p, name, (e) => {
        term = applyEvent(term, e);
        if (e.event === "done") {
          const d = e.data as { requested?: number; joined?: number } | undefined;
          requested = d?.requested ?? 0; joined = d?.joined ?? 0;
        }
      });
      note = `Loaded "${name}" — ${joined} of ${requested} bots joined.`;
    } catch (e) { showErr(e); }
    finally {
      loadingPreset = false;
      await refresh();
      await refreshPresets();
    }
  }

  async function deletePreset(name: string) {
    if (confirmingPreset?.kind !== "delete" || confirmingPreset?.name !== name) {
      confirmingPreset = { kind: "delete", name };
      return;
    }
    confirmingPreset = null;
    busy = true; error = null;
    try { await wowPartyPresetDelete(name); await refreshPresets(); }
    catch (e) { showErr(e); } finally { busy = false; }
  }
```

Also add `confirmingPreset = null;` inside the existing `refresh()`'s reset block (next to `confirmSetup = false;`).

- [ ] **Step 2: Template — per-bot buttons**

In the members table row, extend the `is_bot` cell (keep Kick/Re-summon first):

```svelte
              <td>{#if m.is_bot}<button onclick={() => kick(m.name)} disabled={busy || loadingPreset}>Kick</button>
                  <button onclick={() => resummon(m.name)} disabled={busy || loadingPreset}>Re-summon</button>
                  <button onclick={() => botcmd(m.name, "gear")} disabled={busy || loadingPreset}>Gear up</button>
                  <button onclick={() => botcmd(m.name, "talents")} disabled={busy || loadingPreset}>Fix talents</button>
                  <button onclick={() => botcmd(m.name, "maintain")} disabled={busy || loadingPreset}>Maintain</button>{:else}<span class="muted">you</span>{/if}</td>
```

(The existing Kick/Re-summon `disabled={busy}` gains `|| loadingPreset` as shown.)

- [ ] **Step 3: Template — presets card**

Insert after the Current party table's closing `{/if}` (still inside the online `{:else}` block, before its final `{/if}`):

```svelte
    <header class="bar"><h3>Party presets</h3></header>
    <div class="card">
      <div class="prow">
        <input placeholder="preset name" maxlength="32" bind:value={presetName}
          disabled={busy || setting || loadingPreset} />
        <button onclick={savePreset}
          disabled={!presetName.trim() || busy || setting || loadingPreset || members.filter((m) => m.is_bot).length === 0}>
          Save current party
        </button>
      </div>
      {#if presets.length === 0}
        <p class="muted">No presets saved yet — build a party and save it.</p>
      {:else}
        {#each presets as pr (pr.name)}
          <div class="prow">
            <span>{pr.name} <span class="muted">({pr.bots} bots)</span></span>
            <button onclick={() => loadPreset(pr.name)} disabled={busy || setting || loadingPreset}>
              {confirmingPreset?.kind === "load" && confirmingPreset?.name === pr.name ? "Replaces your current bots — sure?" : "Load"}
            </button>
            <button onclick={() => deletePreset(pr.name)} disabled={busy || setting || loadingPreset}>
              {confirmingPreset?.kind === "delete" && confirmingPreset?.name === pr.name ? `Delete "${pr.name}" — sure?` : "Delete"}
            </button>
          </div>
        {/each}
      {/if}
    </div>
```

Add to the `<style>` block:

```css
  .prow { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; padding: 4px 0; }
```

- [ ] **Step 4: Gates**

From `launcher/`: `npm run check` 0/0; `npm test` 19/19; `npm run build` (vite OK).

- [ ] **Step 5: Commit**

`git add launcher/src/lib/pages/Playerbots.svelte`; commit `feat(launcher): per-bot gear/talents/maintain buttons + party presets card`. Verify LF.

---

### Task 8: Docs + full gates + release build

**Files:**
- Modify: `cli/README.md` (party section), `launcher/README.md` (Playerbots bullet), `CLAUDE.md` (verb list, My Party bullet, round-status bullet)

**Interfaces:** none (docs + final gates).

- [ ] **Step 1: `cli/README.md`** — in the party subcommands section, document the five new verbs:

```markdown
    dml wow party botcmd --player <name> --bot <name> --action gear|talents|maintain --json
    dml wow party preset-save   --player <name> --name <preset> --json
    dml wow party preset-list   --json
    dml wow party preset-delete --name <preset> --json
    dml wow party preset-load   --player <name> --name <preset> --json

`botcmd` whispers a fixed command to the bot as if the player typed it
(`gear` → autogear, `talents` → talents autopick, `maintain` →
maintenance) — a closed allowlist; there is no free-text whisper.
Presets live under `~/.dml/party-presets/<name>` (one class name per
line). `preset-save` snapshots the LIVE party's bots (`overwrote:true`
when replacing). `preset-load` streams NDJSON and REPLACES the party:
kicks every current bot, then per saved class adds a bot, waits for the
join, and whispers `talents autopick` + `autogear` to the newcomer
(maintenance is deliberately not auto-run — it can walk bots to
trainers mid-load); `done` reports `{requested, joined}`.
Errors: BAD_ARG (names/action/preset name), NOT_FOUND (offline
player/bot, unknown preset, party has no bots to save),
DB_UNREACHABLE (party reads), SOAP_AUTH, SOAP_FAULT (bridge-setup
hint), SOAP_UNREACHABLE.
```

- [ ] **Step 2: `launcher/README.md`** — extend the Playerbots (My Party) bullet with:

```markdown
  Each bot row also has **Gear up** / **Fix talents** / **Maintain**
  (whispered to the bot as if you typed it), and a **Party presets** card
  saves your current lineup under a name and loads it back later —
  loading replaces your current bots and re-gears/re-talents the new
  ones automatically.
```

- [ ] **Step 3: `CLAUDE.md`** — (a) cli verb-list bullet: extend the party verbs mention with `botcmd (closed gear/talents/maintain whisper allowlist)` and `preset-save/list/delete/load (~/.dml/party-presets, one class per line, load = replace + streamed)`; (b) the My Party launcher bullet: append `Phase 2: per-bot Gear up/Fix talents/Maintain buttons + party presets (save/load/delete, load streams into the terminal).`; (c) add the round-status bullet after the Summon-NPCs one:

```markdown
- My-Party-phase-2 round (Lab-parity round 4, `docs/superpowers/plans/2026-07-17-my-party-phase2.md`) — **built via SDD** (dml_whisper bridge; party botcmd closed allowlist; presets save/list/delete/load with streamed replace-load); pending final whole-branch review + USER live gate. Later rounds: char backup/restore.
```

and remove the `Later rounds: My Party phase 2 → char backup/restore.` trailer from the Summon-NPCs bullet.

- [ ] **Step 4: Full gate battery**

- dml-arch WSL: `bash build.sh && bats tests/` — 220/220.
- `launcher/`: `npm run check` 0/0; `npm test` 19/19; `npm run tauri build` — bundles under `launcher/src-tauri/target/release/bundle/`.
- `launcher/src-tauri/`: `cargo test` 17/17.

- [ ] **Step 5: Commit**

`git add cli/README.md launcher/README.md CLAUDE.md`; commit `docs: my party phase 2 sweep`. Do NOT stage Cargo.toml if the ghost reappears.

---

## Post-plan user gate (batched with rounds 1–3)

Dev-install, Deploy server bridges, restart. With a character online and 2 bots added: click Gear up / Fix talents / Maintain on a bot (watch it respond in-game); save the party as a preset; kick a bot; Load the preset (party restored, newcomers geared+talented); Delete the preset.
