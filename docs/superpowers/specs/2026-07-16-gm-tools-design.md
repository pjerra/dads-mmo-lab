# GM Character Tools — Design

**Date:** 2026-07-16
**Branch:** `feat/dml-launcher-windows` (stays here; no merge until asked)
**Round:** 2 of the Lab-parity roadmap (sidebar reorg ✓ → **GM tools** → summon NPCs → My Party phase 2 → backup/restore)

## Goal

A "GM Tools" page (CHARACTERS sidebar section) with four actions on any character: **Revive**, **Full heal**, **Set level**, **Set gold** — Lab parity for `gm_revive` / `gm_set_health_pct` / level / `gm_set_money`. Approach: Eluna bridge (like My Party) for the live ops, stock SOAP for level. MySQL stays strictly read-only; all mutations go over SOAP.

## The Eluna bridge: `cli/lua/gm/dml_gm.lua`

AGPL reimplementation of The Lab's `dml_gm.lua` (reference read live from the extracted AppImage at Ubuntu-distro `labtest:~/squashfs-root/usr/lib/TheLab/eluna-scripts/dml_gm.lua`), using the exact skeleton our party bridges already use:

- `RegisterPlayerEvent(42, handler)` (PLAYER_EVENT_ON_COMMAND), gated to console/SOAP origin: `if player ~= nil then return end`.
- Resolve the target with `GetPlayerByName(name)`; if not online, log and return (the CLI's online guard makes this unreachable in practice, but the script stays safe standalone).
- Three commands (The Lab's power command is deliberately skipped — out of scope):
  - `dml_gm_health <name> <pct>` — `SetHealth(floor(GetMaxHealth() * pct / 100))`, floor 1 HP.
  - `dml_gm_money <name> <copper>` — `SetCoinage(copper)`, absolute.
  - `dml_gm_revive <name>` — `ResurrectPlayer(1.0, false)` (100% HP, **no resurrection sickness**), then `SetHealth(GetMaxHealth())`. Harmless on an already-alive character (mirrors The Lab).
- Every mutation ends with `SaveToDB()` so it survives a worldserver crash.
- Rationale (from the Lab's own header): direct DB UPDATEs only apply on next login because the worldserver caches Player state and overwrites the row on logout — the bridge mutates the live object instead.

## Bridge deployment generalizes: `dml wow bridge-setup`

Rounds 3–4 add more bridge scripts (summon, whisper), so deployment stops being party-specific now:

- New verb `dml wow bridge-setup [--json]` (NDJSON streaming, like party-setup today): deploys **every** bridge dir under the CLI's lua share (`lua/party/`, `lua/gm/`; future dirs ride along automatically) to the server's Eluna script folder, reports `restart_required` in `done`.
- `dml wow party-setup` becomes an **alias** of bridge-setup — existing callers (Playerbots page → Rust `wow_party_setup`) keep working unchanged and now deploy the GM bridge too.
- `cli/dev-install.ps1` stages the whole `cli/lua/` tree (currently only `party/`) to `/usr/local/share/dml/lua/`.
- One server restart loads new scripts (no live Eluna reload on this AC build) — same as My Party.

## CLI verbs: new `cli/src/55-gm.sh`, namespace `dml wow gm ...`

All return the standard JSON envelope; character names pass `_valid_charname` before any command string is built (same injection posture the security reviews passed); bridge ops reuse the My Party helpers **as-is, keeping their current names** — the online guard (`_party_online_guid`) and the SOAP fire + rc mapping (`_party_fire`: rc3→SOAP_AUTH, rc2→SOAP_FAULT, else→SOAP_UNREACHABLE). No rename churn: the CLI is one concatenated file, the helpers are global, and the party tests already prove them. One edit only: `_party_fire`'s SOAP_FAULT hint text changes from `party-setup` to `bridge-setup` (the honest verb; party-setup remains an alias so old docs stay true).

| Verb | Args | Path | Notes |
|---|---|---|---|
| `gm level` | `--player X --level N` | stock SOAP `.character level X N` | N range-checked 1–80 (`BAD_ARG` outside); works for **offline** characters; absolute (can de-level) |
| `gm gold` | `--player X --gold N` | bridge `dml_gm_money X <N*10000>` | gold→copper ×10,000; cap 214,748 gold (2^31−1 copper) → `BAD_ARG` above; online-guarded |
| `gm heal` | `--player X` | bridge `dml_gm_health X 100` | online-guarded |
| `gm revive` | `--player X` | bridge `dml_gm_revive X` | online-guarded |

Offline character on a bridge op → `NOT_FOUND` with hint "character must be online for this action". Non-numeric/out-of-range inputs → `BAD_ARG`. Success payloads (exact JSON pinned in the plan): level → `{leveled:true, player, level}`; gold → `{gold_set:true, player, gold}`; heal → `{healed:true, player}`; revive → `{revived:true, player}`.

Bash constraints carried from prior rounds: `set -euo pipefail` — guard every fallible command substitution; helpers ending in a conditional need `return 0`; no `local` in the top-level dispatch case.

## Launcher

**Sidebar:** `nav.ts` CHARACTERS gains `{ id: "gmtools", label: "GM Tools" }` (order: Dashboard, Teleport, GM Tools); `nav.test.ts` pinned arrays update to match. This is round 1's "entry ships with its page" rule.

**New page `launcher/src/lib/pages/GMTools.svelte`:**
- Character row: existing `CharPicker` (any character) + Online/Offline badge from `wowPartyOnline()` (already excludes bots; no new CLI) + Refresh.
- **Revive** / **Full heal**: single-click buttons; disabled with a "needs the character online" hint when offline.
- **Set level**: number input 1–80 + Apply with the standard two-step confirm ("This can lower the level — sure?"); enabled regardless of online state.
- **Set gold**: number input 0–214748 + Apply with two-step confirm ("This replaces their current money — sure?").
- Feedback: short success note per action ("Revived Testen"); failures in the standard error card with the CLI hint.
- One-time **"Deploy server bridges"** two-step button (same pattern/copy family as Playerbots' Enable): streams `wow_bridge_setup` into the shared Terminal, sets `restartState.needed` on `done.restart_required`, notes that a restart loads the scripts. Needed for users who ran party-setup before this round (their server lacks the GM script until a redeploy + restart).

**Rust (`src-tauri/src/lib.rs`):** four request-response commands `wow_gm_level(player, level)`, `wow_gm_gold(player, gold)`, `wow_gm_heal(player)`, `wow_gm_revive(player)` via `run_json_cmd`; one streaming `wow_bridge_setup` via the existing `stream_args` helper. Fixed argv, no shell, user values only as trailing flag values — same security invariant as the party commands. All registered in `generate_handler!`.

**api.ts:** typed wrappers + result interfaces matching the CLI payloads above; `wowBridgeSetup` mirrors `wowPartySetup`'s streaming signature.

## Error handling

Nothing novel: CLI envelopes → Rust CmdError → error card with hint (established chain). The page never guesses — online state comes from `wowPartyOnline`, and a stale badge just means the action itself returns the online-guard `NOT_FOUND`, which the card shows.

## Testing & gates

- **bats** (stub harness, like the party suites): per verb — capture-assert the exact fired bridge tokens (`dml_gm_money X 50000000` etc.), online-guard → NOT_FOUND, range caps → BAD_ARG (level 0/81, gold −1/214749), gold→copper conversion, SOAP rc2/rc3/curl-7 mapping, level's offline path (no online guard). bridge-setup: deploys both dirs, party-setup alias equivalence.
- **lua pin tests** (like party-lua.bats): AGPL header on `dml_gm.lua`; grep-pin each trigger token (`dml_gm_health%s`, `dml_gm_money%s`, `dml_gm_revive%s`) so a CLI↔Lua rename can't silently break the relay; origin-gate line present.
- **vitest**: nav pins updated (gmtools in CHARACTERS).
- Gates: full bats suite, `svelte-check` 0/0, vitest, `cargo test`, tauri release build.
- **User live gate (later, with round 1's click-through):** bridge-setup + restart; revive/heal/gold on an online char; level on an offline char; `.character level` output shape confirmed live (server was down during design — stock AC command, syntax verified against AC source conventions, parse pinned at the gate).

## Out of scope

Race/faction-change flags, power restore, summon NPCs (round 3), My Party phase 2 presets, any DB write path.
