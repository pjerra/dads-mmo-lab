# Summon Helper NPCs — Design

**Date:** 2026-07-16
**Branch:** `feat/dml-launcher-windows` (stays here; no merge until asked)
**Round:** 3 of the Lab-parity roadmap (sidebar reorg ✓ → GM tools ✓ → **summon NPCs** → My Party phase 2 → backup/restore)

## Goal

Summon a temporary service NPC (auctioneer, banker, innkeeper, stable master, repair bot — or any creature entry) next to an online character, from a new card on the existing **GM Tools** page. Same three-layer relay as My Party/GM tools; the NPC self-despawns after 5 minutes.

## The Eluna bridge: `cli/lua/gm/dml_summon_npc.lua`

AGPL reimplementation of The Lab's `dml_summon_npc.lua` (reference read live from `labtest:~/squashfs-root/usr/lib/TheLab/eluna-scripts/dml_summon_npc.lua`), in our gm-bridge style:

- One command: `dml_summon_npc <playerName> <creatureEntry>` — entry matched with `(%d+)` (digits only at the Lua layer too).
- Console/SOAP origin gate first: `if player ~= nil then return end`.
- Resolve via `GetPlayerByName`; if offline, log + `return false` (CLI's online guard makes this unreachable in practice).
- Spawn 2 yards in front of the player so it doesn't stand inside them: `fx = x + cos(o)*2.0`, `fy = y + sin(o)*2.0`, then `p:SpawnCreature(entry, fx, fy, z, o, 3, 300000)`.
- **Spawn type 3 = TEMPSUMMON_TIMED_DESPAWN, timer 300000 ms** — the creature vanishes after 5 minutes no matter what; repeated summons can't litter the world. No DB writes. (NB: The Lab's own reference uses 8 with a comment claiming timed despawn — on our AC source 8 is TEMPSUMMON_MANUAL_DESPAWN, i.e. never despawns; verified in src/server/game/Entities/Object/Object.h + TemporarySummon.cpp. Deliberate deviation from the reference.)
- Why a bridge at all (from the Lab's header): `.npc add` needs an in-world GM session with a position, which SOAP doesn't have — Eluna routes through the player's own position.

Because the script lives in the `gm/` family dir, round 2's `dml wow bridge-setup` deploys it with **zero deploy-code changes** (the deploy loop copies every `*.lua` under each family dir). Users who deployed before this round need one redeploy + restart — the existing "Deploy server bridges" button covers it.

## CLI verb: `dml wow gm summon` (in the existing gm namespace)

`dml wow gm summon --player X --entry N --json`

Order of checks (each failing with a clean envelope):
1. `_valid_charname "$player"` → else `BAD_ARG`.
2. Entry `^[0-9]+$` and 1–999999 → else `BAD_ARG` (hint: "Creature entry id, 1-999999.").
3. **Existence + name lookup** (read-only, the allowed MySQL posture): `db_world_query "SELECT name FROM creature_template WHERE entry=N LIMIT 1;"` — empty → `NOT_FOUND` "No creature with entry N" (this replaces the in-game silent failure for bad custom entries); query error → `DB_UNREACHABLE`. Guard the substitution (`|| npcname=""` style) — `set -euo pipefail` is live.
4. `_gm_require_online "$player"` → offline → `NOT_FOUND` (same copy as the other gm verbs).
5. `_party_fire "dml_summon_npc $player $entry" "summon"` (bare tokens; rc mapping + bridge-setup hint come free).

Success payload: `{"summoned":true,"player":"X","entry":N,"npc":"<name from the lookup>"}` (name JSON-escaped).

Dispatch arm goes inside the existing `gm)` case in `cli/src/90-main.sh` (bare variables, no `local`); no new helpers needed (`db_world_query` already exists in `cli/src/30-db.sh`).

## Plumbing

- **Rust** (`launcher/src-tauri/src/lib.rs`): `wow_gm_summon(player: String, entry: u32)` via `run_json_cmd`, fixed argv `["wow","gm","summon","--player",player,"--entry",entry.to_string()]`; registered in `generate_handler!`.
- **api.ts**: `GmSummonResult { summoned: boolean; player: string; entry: number; npc: string }`; `wowGmSummon(player: string, entry: number): Promise<GmSummonResult>`.

## UI: a card on GM Tools (no new sidebar entry)

Appended to `launcher/src/lib/pages/GMTools.svelte`, reusing the page's `charName`/`isOnline`/`busy`/`act()` machinery:

- **"Summon an NPC"** card: five preset buttons + a custom row.
- Presets (entries verified live on this server 2026-07-16; one constant `NPCS` array so extending later — e.g. Transmogrifier if mod-transmog is ever installed — is a one-line change):
  - Auctioneer — 8661 (Auctioneer Beardo)
  - Banker — 5060 (World Banker)
  - Innkeeper — 6272 (Innkeeper Janene)
  - Stable Master — 9896 (World Stable Master)
  - Repair Bot — 14337 (Field Repair Bot 74A)
  - Casino — 990000 (Gasino; the user's own Eluna casino mod, verified installed on this server: creature_template row + `lua_scripts/gasino/` script both present. On a server without the mod the button fails cleanly via the existence check — `NOT_FOUND "No creature with entry 990000"`.)
- Custom row: number input (1–999999, `Number.isInteger` guard like level/gold) + a "Summon" button.
- All summon controls disabled when `!charName || !isOnline || busy` (the page's existing offline hint covers why). **No two-step confirm** — the action is harmless and self-cleaning.
- Success note via the existing `act()` pattern: `Summoned <npc> — despawns in 5 minutes.` (uses the payload's `npc` name). Errors land in the standard error card with the CLI hint.

## Error handling

Nothing novel: envelope → CmdError → error card chain. New cases are all CLI-side (`NOT_FOUND` unknown entry, `DB_UNREACHABLE`, plus the shared online-guard/rc mapping).

## Testing & gates

- **gm-lua.bats** (append, against a second file var pointing at `dml_summon_npc.lua`): file exists; AGPL/Dad's MMO Lab header; hook 42; origin gate; token pin `dml_summon_npc%s`; spawn-args pin `, 3, 300000` (the self-despawn invariant); `grep -q 'return false'` on this file (single-handler script, so presence suffices — the count-≥3 pin on `dml_gm.lua` stays as-is).
- **wow-gm.bats** (append): capture-assert `dml_summon_npc Testen 8661` fired; unknown entry (empty DB rows) → NOT_FOUND; DB error → DB_UNREACHABLE; entry 0 / 1000000 / abc → BAD_ARG; offline → NOT_FOUND; SOAP fault → SOAP_FAULT with bridge-setup hint; success payload carries the npc name from the stubbed row.
- NB the test-stub nuance: the existence lookup (world DB) and the online guard (chars DB) both go through the same docker-exec mysql stub — use `DML_STUB_DB_ROWS_SEQ` (successive row files) like the party add tests where a test needs both lookups.
- Gates: full bats suite, `svelte-check` 0/0, vitest, `cargo test`, tauri release build.
- **User live gate (batched with rounds 1–2):** redeploy bridges + restart, summon a Banker next to an online char, watch it despawn (or just trust the timer).

## Out of scope

Transmogrifier preset (mod-transmog not installed — "No creatures found!" verified live), gameobject spawns (mailboxes use a different API), permanent spawns, and any DB write path.
