# My Party Phase 2 — Per-Bot Control + Party Presets — Design

**Date:** 2026-07-17
**Branch:** `feat/dml-launcher-windows` (stays here; no merge until asked)
**Round:** 4 of the Lab-parity roadmap (sidebar ✓ → GM tools ✓ → summon ✓ → **My Party phase 2** → backup/restore)

## Goal

Two additions to the Playerbots (My Party) page: **per-bot buttons** (Gear up / Fix talents / Maintain) powered by a new whisper bridge, and **party presets** (save your current bot lineup under a name; load it back later — replace semantics). CLI-orchestrated with a closed command set; no free-text whisper path anywhere.

## Why a whisper bridge

mod-playerbots accepts its bot commands only as whispers FROM a player session TO the bot — SOAP has no way to spoof player chat. Eluna's `Player:Whisper` fills the gap (routes through core `Player::Whisper`, which fires the module's chat hook). Verified in THIS build's chat-trigger registry (`modules/mod-playerbots/src/Ai/Base/ChatTriggerContext.h`): `talents` (subcommands `switch 1/2`, `autopick`, `spec list`, `spec <name>`, `apply <link>` per `ChangeTalentsAction.cpp`), `autogear`, `autogear bis`, `maintenance` all exist.

## The Eluna bridge: `cli/lua/party/dml_whisper.lua`

AGPL reimplementation of The Lab's `dml_whisper.lua` (reference read live from the extracted AppImage), our bridge style:

- One command: `dml_whisper <playerName> <botName> <message...>` — third capture is greedy (`(.+)$`) so messages contain spaces (`talents autopick`).
- Console/SOAP origin gate FIRST: `if player ~= nil then return end` (chat parses always carry a non-nil player, so in-game chat can never trigger it).
- Resolve both names via `GetPlayerByName`; log + `return false` if either is offline.
- `p:Whisper(msg, 0, b)` (language 0 = universal; core forces whispers universal anyway).
- `return false` to consume the command (AC's parser would otherwise error on it).
- Lives in the `party/` family dir → round 2's `bridge-setup` deploys it with zero deploy-code changes (existing deployments need one redeploy + restart; the existing button covers it).

## CLI verbs (party namespace)

All standard envelopes; names via `_valid_charname` before any command string; bridge fires via `_party_fire` (rc mapping + bridge-setup hint). Bash rules as always: `set -euo pipefail` guards, no `local` in the top-level dispatch.

### `party botcmd --player X --bot B --action gear|talents|maintain`

- Closed allowlist (case arm), mapping to fixed whisper strings:
  - `gear` → `autogear`
  - `talents` → `talents autopick`
  - `maintain` → `maintenance`
- Unknown action → `BAD_ARG` (hint lists the three).
- Both X and B online-guarded via `_party_online_guid` directly (NOT `_gm_require_online` — its hint text is gm-specific); offline → `NOT_FOUND` with a party-appropriate message naming which character ("Character not online: B" / hint "The bot must be in the world — is it still in your party?"). Bots in a party are online characters, so the same guard works for them.
- Fire: `_party_fire "dml_whisper $X $B <mapped string>" "botcmd"`.
- Payload: `{"sent":true,"player":"X","bot":"B","action":"gear"}`.
- **No free-text variant exists** — this is the whole surface.

### Presets — storage

One file per preset: `~/.dml/party-presets/<name>` — one class NAME per line (the same names `party add --class` takes: warrior|paladin|hunter|rogue|priest|shaman|mage|warlock|druid). Preset name validated `^[A-Za-z0-9_-]{1,32}$` → else `BAD_ARG`. A small `_class_name_from_id` helper (50-party.sh, case map for ids 1–9/11, matching wow.ts's `className`) converts the DB's class ids on save; unknown/unsupported ids (e.g. DK 6) are skipped with a note.

### `party preset-save --player X --name N`

Player online-guarded. Snapshots the LIVE party's `is_bot` rows (the exact SQL `party list` uses), maps class ids → names, writes the file (mkdir -p the dir). Zero bots → `NOT_FOUND` "party has no bots to save". If the name existed, the payload says so: `{"saved":true,"name":"N","bots":["mage","priest"],"overwrote":true|false}`.

### `party preset-list`

`{"presets":[{"name":"dungeon-crew","bots":3}]}` (line counts). Empty/missing dir → `{"presets":[]}`.

### `party preset-delete --name N`

`{"deleted":true,"name":"N"}`; missing file → `NOT_FOUND`.

### `party preset-load --player X --name N` (NDJSON streaming)

Replace semantics (user-chosen): section `preset-load`, then
1. Validate name + player online + file exists (missing → `NOT_FOUND` error event).
2. **Kick phase**: query current bot names (the party-list SQL), fire `dml_uninvite <bot>` for each (`line info "kicked <bot>"`).
3. **Add+prep phase**, per saved class: fire `dml_addclass X <class>`; poll for the join with the group-diff logic **extracted into a shared helper** (used by both the existing `party add` arm and this loop — the existing add tests are the regression net for the extraction; poll seams `DML_PARTY_POLL_TRIES`/`DML_PARTY_POLL_SLEEP` kept); on join, whisper the newcomer `talents autopick` then `autogear` (`line info "<bot> joined — talents + gear applied"`). A class that doesn't attach in time → `line warn` and the load continues.
4. `done` data: `{"loaded":true,"requested":N,"joined":M}`.

Maintenance is deliberately NOT auto-run on load (it can walk bots to trainers mid-load); the per-bot Maintain button covers it.

## Launcher

**Playerbots.svelte** (existing page) — two additions:

- **Per-bot buttons** on each `is_bot` row (next to Kick/Re-summon): **Gear up**, **Fix talents**, **Maintain** → `wowPartyBotcmd(player, bot, action)`; success note `Told <bot> to gear up — give it a moment.` (matching phrasing per action: "…to fix its talents…", "…to do maintenance…"); disabled while busy/setting; snapshot-before-await as the page already does.
- **Presets card** below the party list:
  - Name input (maxlength 32) + **Save current party** → `wowPartyPresetSave`; note `Saved preset "<name>" (3 bots).` or `…(replaced the old one).` when `overwrote`.
  - Saved presets listed (fetched on mount + after save/load/delete via `wowPartyPresetList`): each row shows name + bot count with **Load** and **Delete**, both two-step confirmed (`Replaces your current bots — sure?` / `Delete "<name>" — sure?`).
  - Load streams `wowPartyPresetLoad` into the shared Terminal (busy-gates the page like Enable My Party does), then refreshes the party list and note `Loaded "<name>" — M of N bots joined.` from the done data.

**Rust**: `wow_party_botcmd(player, bot, action)`, `wow_party_preset_save(player, name)`, `wow_party_preset_list()`, `wow_party_preset_delete(name)` via `run_json_cmd`; `wow_party_preset_load(player, name, on_event)` via `stream_args`. Fixed argv, values only as trailing flag values.

**api.ts**: `BotcmdResult {sent, player, bot, action}`, `PresetInfo {name, bots}`, `PresetSaveResult {saved, name, bots: string[], overwrote}`, wrappers `wowPartyBotcmd`, `wowPartyPresetSave`, `wowPartyPresetList`, `wowPartyPresetDelete`, `wowPartyPresetLoad` (streaming signature like `wowPartySetup` but with args).

## Error handling

Established chains only. New codes per verb documented in cli/README (botcmd: BAD_ARG/NOT_FOUND/SOAP_*; preset verbs add NOT_FOUND for missing preset/no bots; preset-load streams its errors as NDJSON error events). The gm README errors-footer convention from round 3 applies: document every emitting code.

## Testing & gates

- **party-lua.bats** (append): `dml_whisper.lua` exists + AGPL header; hook 42; gate-order assertion (gate line before match line, as round 3 hardened); token pin `dml_whisper%s`; greedy capture pin `(.+)$`; both-lookup pin (two `GetPlayerByName`); `Whisper(` call pin; `return false`.
- **wow-party.bats** gets the botcmd tests (it's a party bot action); **a NEW `cli/tests/wow-party-presets.bats`** gets all four preset verbs (keeps files focused): botcmd — capture-assert all three exact whisper strings, unknown action → BAD_ARG, offline player/bot → NOT_FOUND; preset-save — file content equals expected class names (HOME=$FIXTURE so `~/.dml` is sandboxed), id→name mapping incl. skipping an unsupported id, name validation, overwrote flag, no-bots NOT_FOUND; preset-list (empty dir → `[]`); preset-delete (+NOT_FOUND); preset-load — full NDJSON walk with capture+SEQ stubs and `DML_PARTY_POLL_TRIES=1 DML_PARTY_POLL_SLEEP=0`: kicks fired, adds fired, whispers fired per join, warn path on no-join, done counts requested/joined.
- **Regression net**: the join-wait extraction must keep ALL existing wow-party.bats add tests green unchanged.
- Gates: full bats, `svelte-check` 0/0, vitest, `cargo test`, tauri release build.
- **User live gate (batched with rounds 1–3):** save a 2-bot party as a preset, kick one, load the preset (party restored, bots geared/talented), one Maintain click on a bot.

## Out of scope

TOML preset import/export (Lab had it; our per-line format can grow an exporter later), spec-name picker (`talents spec <name>` free text), per-bot level (GM Tools Set level already works on bots — they're characters), `autogear bis` (plain autogear is the beginner-safe default), round 5 backup/restore.
