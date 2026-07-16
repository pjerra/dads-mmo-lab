# Summon Helper NPCs Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A "Summon an NPC" card on the GM Tools page (6 presets + custom entry id) that temp-spawns a service NPC next to an online character via a new `dml_summon_npc` Eluna bridge, with a read-only creature_template existence check in the CLI.

**Architecture:** Same relay as GM tools: page → typed Tauri command → `dml wow gm summon` → SOAP → Eluna hook 42 → `SpawnCreature(..., 3, 300000)` (5-minute self-despawn). The new Lua file lands in `cli/lua/gm/`, so round 2's `bridge-setup` deploys it with zero deploy-code changes.

**Tech Stack:** Eluna Lua, bash CLI (built artifact `cli/dml`), bats, Rust/Tauri 2, Svelte 5 runes.

**Spec:** `docs/superpowers/specs/2026-07-16-summon-npcs-design.md`

## Global Constraints

- Branch `feat/dml-launcher-windows`. Never merge; never push unless asked.
- NEVER hand-edit `cli/dml` — edit `cli/src/*.sh`, run `bash cli/build.sh`, commit both.
- CLI bash rules (`set -euo pipefail`): guard fallible command substitutions (`|| { …; exit 1; }`); no `local` in the top-level dispatch case (bare variables like the neighboring arms).
- Bridge command fired as BARE tokens: `dml_summon_npc <player> <entry>`. Trigger token pinned by tests on both sides — never rename one side alone.
- Names pass `_valid_charname` before entering any command string. Entry: `^[0-9]+$` AND 1–999999 → else `BAD_ARG`.
- Existence check order (spec-pinned): validate name → validate entry → `db_world_query` existence+name lookup (`NOT_FOUND` if empty, `DB_UNREACHABLE` on query error) → `_gm_require_online` → `_party_fire`.
- Success payload exactly: `{"summoned":true,"player":"X","entry":N,"npc":"<name>"}` (npc JSON-escaped).
- Spawn invariant: type **3** (TEMPSUMMON_TIMED_DESPAWN), timer **300000** ms — pinned by a Lua test.
- Presets exactly (verified live 2026-07-16): Auctioneer 8661, Banker 5060, Innkeeper 6272, Stable Master 9896, Repair Bot 14337, Casino 990000.
- `launcher/src-tauri/Cargo.toml` ghost modification: NEVER stage.
- Committed blobs LF (lua/sh/dml/ts/svelte); bats runs inside the dml-arch WSL distro (`wsl -d dml-arch -u dml -- bash -lc "cd /mnt/c/Users/perzi/dads-mmo-lab/cli && bash build.sh && bats tests/"`); known DrvFs flake ("cannot execute binary file") — re-run once before treating as real.
- Test baselines before this plan: bats 181 (gm-lua 8, wow-gm 14), vitest 19, cargo 17, check 0/0.
- User-facing copy verbatim where given in tasks.

---

### Task 1: The summon bridge — `cli/lua/gm/dml_summon_npc.lua`

**Files:**
- Create: `cli/lua/gm/dml_summon_npc.lua`
- Test: `cli/tests/gm-lua.bats` (append)

**Interfaces:**
- Consumes: nothing (standalone Eluna script; deployed by the existing `bridge-setup`).
- Produces: console/SOAP command `dml_summon_npc <playerName> <creatureEntry>` (Task 2 fires it).

- [ ] **Step 1: Append the failing tests**

Append to `cli/tests/gm-lua.bats` (note: the file's existing `LUA=` line sits at the top; add `LUA2=` directly under it, then append the tests at the end of the file):

Add below the existing `LUA=` assignment:

```bats
LUA2="$BATS_TEST_DIRNAME/../lua/gm/dml_summon_npc.lua"
```

Append at the end of the file:

```bats
@test "summon bridge exists with an AGPL/Dad's MMO Lab header" {
  [ -f "$LUA2" ]
  grep -qi 'AGPL' "$LUA2"
  grep -qiE "Dad's MMO Lab" "$LUA2"
}

@test "summon bridge registers hook 42 and gates to console/SOAP origin" {
  grep -q 'RegisterPlayerEvent(42,' "$LUA2"
  grep -qE 'if +player +~= +nil +then +return' "$LUA2"
}

@test "summon bridge pins the dml_summon_npc token with a digits-only entry" {
  grep -q 'dml_summon_npc%s' "$LUA2"
  grep -q '(%d+)' "$LUA2"
}

@test "summon bridge uses timed self-despawn (type 3, 300000 ms)" {
  grep -q ', 3, 300000)' "$LUA2"
}

@test "summon bridge handler returns false to suppress the not-found" {
  grep -q 'return false' "$LUA2"
}
```

- [ ] **Step 2: Run to verify failure**

Run (dml-arch WSL): `bats tests/gm-lua.bats` — Expected: the 5 new tests FAIL (file missing), the 8 old PASS.

- [ ] **Step 3: Write the bridge**

Create `cli/lua/gm/dml_summon_npc.lua`:

```lua
--[[
    dml_summon_npc.lua -- Dad's MMO Lab launcher summon bridge.
    License: AGPL-3.0-only (same as the repo).
    Reimplemented for DML; behavioral reference: The Lab's summon relay.
    See docs/superpowers/specs/2026-07-16-summon-npcs-design.md.

    One console/SOAP-only command:

        dml_summon_npc <playerName> <creatureEntry>

    Temp-spawns <creatureEntry> just in front of the ONLINE player.
    Spawn type 3 = TEMPSUMMON_TIMED_DESPAWN with a 300000 ms timer --
    the creature vanishes after 5 minutes no matter what, so repeated
    summons can't litter the world. No DB writes.

    Why a bridge: `.npc add` needs an in-world GM session with a
    position, which SOAP doesn't have -- Eluna routes through the
    player's own position instead (same pattern as the other bridges).
]]--

local function OnSummonCommand(event, player, command)
    -- Console/SOAP origin only: a real player typing this must never match.
    if player ~= nil then return end

    local pname, entry = command:match("^dml_summon_npc%s+(%S+)%s+(%d+)$")
    if not pname then return end

    local p = GetPlayerByName(pname)
    if not p then
        print(string.format("[dml_summon_npc] player not online: %s", pname))
        return false
    end

    local e = tonumber(entry)
    local x, y, z, o = p:GetX(), p:GetY(), p:GetZ(), p:GetO()
    -- Drop it just in front of the player so it isn't standing inside them.
    local fx = x + math.cos(o) * 2.0
    local fy = y + math.sin(o) * 2.0

    -- WorldObject:SpawnCreature(entry, x, y, z, o, spawnType, despawnTimer)
    p:SpawnCreature(e, fx, fy, z, o, 3, 300000)
    print(string.format("[dml_summon_npc] %s -> npc %d", pname, e))
    return false
end

RegisterPlayerEvent(42, OnSummonCommand)
print("[dml_summon_npc] loaded")
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `bats tests/gm-lua.bats` — Expected: 13/13.
Run: `bats tests/` — Expected: 186/186 (181 + 5).

- [ ] **Step 5: Verify LF, commit**

`git add cli/lua/gm/dml_summon_npc.lua cli/tests/gm-lua.bats`; commit `feat(cli): AGPL summon-NPC Eluna bridge (5-min temp spawn)`. Verify `git show HEAD:cli/lua/gm/dml_summon_npc.lua | file -` reports no CRLF.

---

### Task 2: `dml wow gm summon`

**Files:**
- Modify: `cli/src/90-main.sh` (new `summon)` arm inside the existing `gm)` dispatch, between `revive)`'s closing `;;` and the gm `*)`; also extend the gm `*)` hint)
- Test: `cli/tests/wow-gm.bats` (append)

**Interfaces:**
- Consumes: `_valid_charname`, `_need_flag_val`, `db_world_query` (exists in `cli/src/30-db.sh`), `_gm_require_online` (55-gm.sh), `_party_fire` (50-party.sh), `json_ok`/`json_err`/`json_escape`.
- Produces: `dml wow gm summon --player X --entry N --json` → `{"summoned":true,"player":"X","entry":N,"npc":"<name>"}`.

- [ ] **Step 1: Append the failing tests to `cli/tests/wow-gm.bats`**

Note the stub nuance: the world-DB existence lookup and the chars-DB online guard both go through the same docker-exec mysql stub — tests needing both use `DML_STUB_DB_ROWS_SEQ` (successive row files; first call = npc name lookup, second = online guid), exactly like the party add tests.

```bats
# ---------- gm summon (bridge-backed, existence-checked, online-guarded) ----------

@test "gm summon fires dml_summon_npc and returns the npc name" {
  printf 'Auctioneer Beardo\n' > "$FIXTURE/npc.tsv"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/npc.tsv $FIXTURE/guid.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm summon --player Testen --entry 8661 --json
  [ "$status" -eq 0 ]
  [ "$(echo "$output" | jq -r '.data.summoned')" = "true" ]
  [ "$(echo "$output" | jq -r '.data.entry')" = "8661" ]
  [ "$(echo "$output" | jq -r '.data.npc')" = "Auctioneer Beardo" ]
  grep -q 'dml_summon_npc Testen 8661' "$FIXTURE/cap.txt"
}

@test "gm summon rejects entry 0, 1000000 and non-numeric" {
  for bad in 0 1000000 abc; do
    run bash "$DML" wow gm summon --player Testen --entry "$bad" --json
    [ "$status" -eq 1 ]
    [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
  done
}

@test "gm summon rejects an invalid character name" {
  run bash "$DML" wow gm summon --player 'x; drop' --entry 8661 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "BAD_ARG" ]
}

@test "gm summon unknown entry maps to NOT_FOUND before any SOAP fire" {
  printf '' > "$FIXTURE/nonpc.tsv"
  export DML_STUB_DB_ROWS="$FIXTURE/nonpc.tsv"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  export DML_STUB_CAPTURE="$FIXTURE/cap.txt"
  run bash "$DML" wow gm summon --player Testen --entry 424242 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -q '424242'
  [ ! -s "$FIXTURE/cap.txt" ]
}

@test "gm summon maps a DB error to DB_UNREACHABLE" {
  export DML_STUB_DB_EXIT=1
  run bash "$DML" wow gm summon --player Testen --entry 8661 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "DB_UNREACHABLE" ]
}

@test "gm summon offline player maps to NOT_FOUND (after the entry check)" {
  printf 'World Banker\n' > "$FIXTURE/npc.tsv"
  printf '' > "$FIXTURE/noguid.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/npc.tsv $FIXTURE/noguid.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-ok.xml"
  run bash "$DML" wow gm summon --player Ghost --entry 5060 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "NOT_FOUND" ]
  echo "$output" | grep -qi 'not online'
}

@test "gm summon maps a SOAP fault to SOAP_FAULT with the bridge-setup hint" {
  printf 'World Banker\n' > "$FIXTURE/npc.tsv"
  printf '2503\n' > "$FIXTURE/guid.tsv"
  export DML_STUB_DB_ROWS_SEQ="$FIXTURE/npc.tsv $FIXTURE/guid.tsv"
  export DML_STUB_DB_SEQ_STATE="$FIXTURE/seq.state"
  use_curl_stub
  export DML_STUB_SOAP_RESPONSE="$BATS_TEST_DIRNAME/fixtures/soap-fault.xml"
  run bash "$DML" wow gm summon --player Testen --entry 5060 --json
  [ "$status" -eq 1 ]
  [ "$(echo "$output" | jq -r '.error.code')" = "SOAP_FAULT" ]
  echo "$output" | grep -q 'bridge-setup'
}
```

- [ ] **Step 2: Run to verify failure**

Run: `bats tests/wow-gm.bats` — Expected: the 7 new FAIL (UNKNOWN_COMMAND), the 14 old PASS.

- [ ] **Step 3: Add the `summon)` arm in `cli/src/90-main.sh`**

Insert between `revive)`'s closing `;;` and the gm dispatch's `*)` (bare variables, no `local`):

```bash
          summon)
            player=""; entry=""
            while [[ $# -gt 0 ]]; do
              case "$1" in
                --player) _need_flag_val "$1" $#; player="$2"; shift 2 ;;
                --entry) _need_flag_val "$1" $#; entry="$2"; shift 2 ;;
                *) json_err BAD_ARG "Unknown flag: $1" ""; exit 1 ;;
              esac
            done
            _valid_charname "$player" || { json_err BAD_ARG "Invalid player name: $player" ""; exit 1; }
            if ! [[ "$entry" =~ ^[0-9]+$ ]] || (( entry < 1 || entry > 999999 )); then
              json_err BAD_ARG "Invalid creature entry: $entry" "Creature entry id, 1-999999."; exit 1
            fi
            # Existence + name lookup (read-only) BEFORE any SOAP fire, so a
            # bad custom entry fails with a clean message instead of an
            # in-game silent no-op.
            npcname="$(db_world_query "SELECT name FROM creature_template WHERE entry=$entry LIMIT 1;")" \
              || { json_err DB_UNREACHABLE "Could not check the creature entry" "Is ac-database running?"; exit 1; }
            [[ -n "$npcname" ]] || { json_err NOT_FOUND "No creature with entry $entry" "Check the id (creature_template.entry)."; exit 1; }
            _gm_require_online "$player"
            _party_fire "dml_summon_npc $player $entry" "summon"
            json_ok "{\"summoned\":true,\"player\":\"$(json_escape "$player")\",\"entry\":$entry,\"npc\":\"$(json_escape "$npcname")\"}"
            ;;
```

Also update the gm `*)` hint string to: `Try: dml wow gm level|gold|heal|revive|summon --json`.

- [ ] **Step 4: Rebuild + test**

Run: `bash build.sh && bats tests/wow-gm.bats` — Expected: 21/21.
Run: `bats tests/` — Expected: 193/193.

- [ ] **Step 5: Commit**

`git add cli/src/90-main.sh cli/dml cli/tests/wow-gm.bats`; commit `feat(cli): gm summon (existence-checked temp NPC summon)`.

---

### Task 3: Rust command + api.ts wrapper

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs` (one command after `wow_gm_revive`, plus `generate_handler!` registration)
- Modify: `launcher/src/lib/api.ts` (append after `wowBridgeSetup`)

**Interfaces:**
- Consumes: `run_json_cmd`; Task 2's verb.
- Produces (Task 4 uses): Rust `wow_gm_summon(player: String, entry: u32)`; TS `GmSummonResult { summoned: boolean; player: string; entry: number; npc: string }`, `wowGmSummon(player: string, entry: number): Promise<GmSummonResult>`.

- [ ] **Step 1: Rust**

```rust
#[tauri::command]
async fn wow_gm_summon(player: String, entry: u32, state: State<'_, AppState>) -> Result<serde_json::Value, CmdError> {
    run_json_cmd(
        state,
        vec!["wow".into(), "gm".into(), "summon".into(), "--player".into(), player, "--entry".into(), entry.to_string()],
    )
    .await
}
```

Register `wow_gm_summon` in `generate_handler![...]`.

- [ ] **Step 2: api.ts**

```ts
export interface GmSummonResult { summoned: boolean; player: string; entry: number; npc: string; }
export async function wowGmSummon(player: string, entry: number): Promise<GmSummonResult> {
  return await invoke("wow_gm_summon", { player, entry });
}
```

- [ ] **Step 3: Gates**

`cargo test` (from launcher/src-tauri/): 17/17 zero warnings. From launcher/: `npm run check` 0/0, `npm test` 19/19.

- [ ] **Step 4: Commit**

`git add launcher/src-tauri/src/lib.rs launcher/src/lib/api.ts` (NOT Cargo.toml); commit `feat(launcher): gm summon command + wrapper`. Verify both blobs LF.

---

### Task 4: Summon card on GM Tools + docs + full gates

**Files:**
- Modify: `launcher/src/lib/pages/GMTools.svelte`
- Modify: `cli/README.md` (gm section), `launcher/README.md` (GM Tools bullet), `CLAUDE.md` (gm verb list; drop "Summon" from the future-entries note)

**Interfaces:**
- Consumes: `wowGmSummon`/`GmSummonResult` (Task 3); the page's existing `charName`/`isOnline`/`busy`/`showErr` state.
- Produces: the user-facing card. No new sidebar entry (spec: card on GM Tools).

- [ ] **Step 1: Script additions in GMTools.svelte**

Add `wowGmSummon` to the existing `$lib/api` import list. Add after the `let gold = $state(1000);` line:

```ts
  let customEntry = $state(990000);
```

Add after the `const isOnline = $derived(...)` line:

```ts
  const NPCS = [
    { entry: 8661, label: "Auctioneer" },
    { entry: 5060, label: "Banker" },
    { entry: 6272, label: "Innkeeper" },
    { entry: 9896, label: "Stable Master" },
    { entry: 14337, label: "Repair Bot" },
    { entry: 990000, label: "Casino" },
  ];
```

Add after the `applyGold()` function (it needs the result's `npc` name, so it doesn't reuse `act()`; same snapshot-before-await rule):

```ts
  async function summon(entry: number) {
    const p = charName;
    busy = true; error = null; note = null;
    try {
      const r = await wowGmSummon(p, entry);
      note = `Summoned ${r.npc} — despawns in 5 minutes.`;
    } catch (e) { showErr(e); } finally { busy = false; }
  }
```

- [ ] **Step 2: The card markup**

Insert between the Set gold card's closing `</div>` and the `<p class="muted">` deploy-bridges footer:

```svelte
  <div class="card">
    <div class="row">
      <strong>Summon an NPC</strong>
      {#each NPCS as n (n.entry)}
        <button onclick={() => summon(n.entry)} disabled={!charName || !isOnline || busy}>{n.label}</button>
      {/each}
    </div>
    <div class="row" style="margin-top: 8px;">
      <span class="muted">Custom entry id:</span>
      <input type="number" min="1" max="999999" bind:value={customEntry} disabled={busy} />
      <button onclick={() => summon(customEntry)}
        disabled={!charName || !isOnline || busy || !Number.isInteger(customEntry) || customEntry < 1 || customEntry > 999999}>
        Summon
      </button>
    </div>
    <p class="muted" style="margin-top: 8px;">Temporary — the NPC despawns after 5 minutes. Needs the character online.</p>
  </div>
```

(No two-step confirm — spec: harmless, self-cleaning action. The `Number.isInteger` guard mirrors level/gold: an emptied input becomes `null` and must disable the button.)

- [ ] **Step 3: Docs**

- `cli/README.md`, in `## gm subcommands (GM character tools)`: add to the command block `dml wow gm summon --player <name> --entry <1-999999> --json` and append to the prose: "`summon` temp-spawns the creature next to the ONLINE player (5-minute self-despawn) after checking the entry exists in `creature_template` (read-only) — unknown entry → `NOT_FOUND`; the payload carries the creature's name."
- `launcher/README.md`, GM Tools bullet: append the sentence "Also summons temporary service NPCs (auctioneer, banker, innkeeper, stable master, repair bot, casino — or any creature entry id); they despawn after 5 minutes."
- `CLAUDE.md`: in the cli bullet's verb list change `gm level/gold/heal/revive` → `gm level/gold/heal/revive/summon`; in the launcher sidebar bullet change `Future Lab-parity entries (Summon, Backups)` → `Future Lab-parity entries (Backups)`.

- [ ] **Step 4: Full gates**

- dml-arch WSL: `bash build.sh && bats tests/` — 193/193 (unchanged from Task 2; no CLI change here, just confirming).
- launcher/: `npm run check` 0/0; `npm test` 19/19; `npm run tauri build` — bundles under `launcher/src-tauri/target/release/bundle/`.
- launcher/src-tauri/: `cargo test` 17/17.

- [ ] **Step 5: Commit**

`git add launcher/src/lib/pages/GMTools.svelte cli/README.md launcher/README.md CLAUDE.md`; commit `feat(launcher): Summon an NPC card on GM Tools (+docs)`. Verify GMTools.svelte blob LF. Do NOT stage Cargo.toml.

---

## Post-plan user gate (batched with rounds 1–2)

Dev-install (`powershell -File cli\dev-install.ps1`), Deploy server bridges (GM Tools page), restart, log a character in, summon a Banker (appears in front of you, gone in 5 min), try the Casino button (Gasino gossip works since your mod is installed), try a bogus custom id (clean "No creature with entry N" error).
