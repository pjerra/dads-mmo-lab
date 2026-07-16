# Plan 4 — My Party (design)

Date: 2026-07-16 · Status: **approved by user** (brainstorm w/ per-section sign-off; part 3 amended after live preflight — see §7)
Builds on the verified mechanism in `docs/superpowers/specs/2026-07-15-my-party-spike-findings.md` and the launcher-pages plan (shell, CharPicker, terminal, `dml wow` CLI + typed Rust command pattern).

## 1. Goal

Turn the disabled **Playerbots** sidebar page on: let a logged-in player build a curated party of
playerbots from the DML Launcher — add a bot by class, see the current party, kick a bot, re-summon
after a relog. Ships the whole relay end-to-end (Eluna bridge scripts + a setup step + the live gate).

**Scope this round = the lean core loop.** Explicitly deferred to a later plan: party presets,
per-bot spec/talent/gear editing, ambient random-bot quieting, summon-NPC / GM bridges.

## 2. The verified mechanism (recap, one paragraph)

SOAP alone cannot add a bot (the `.playerbots bot` subcommand is `Console::No` — invisible to a
session-less invoker; live-confirmed). The working path: a small server-side **Eluna (mod-ale) Lua
bridge** exposes a hook that, triggered by name over SOAP `executeCommand`, runs
`.playerbots bot addclass <class>` **inside the target online player's own session** (via
`Player:RunCommand`), so the add happens as if the player typed it. The bot joins the player's group.
The player's character must be **online** for every party op.

## 3. Decisions made during brainstorm (user-confirmed)

1. Scope: **lean core loop** (add-by-class / list / kick / relogin), full relay + live gate.
2. Add model: **pick a class** (`.playerbots bot addclass <class>`), server spawns from the AddClass pool — no pre-made bot characters to provision.
3. Whose party: **auto-detect the online character** (from `characters.online`); if nobody online, the page disables and says "log a character in first."
4. Script deployment: a one-time **`party-setup`** step (mirrors `soap-setup`) — copies our AGPL bridge scripts into the Eluna script dir + preflight-checks prerequisites. (Rejected: installer-only deployment — the user's server is already installed.)

## 4. Live-verified prerequisites (preflight, 2026-07-16 on the real box)

- **mod-ale present + loaded** ✓ (`modules/mod-ale`, `mod_ale.conf` in the loaded module list).
- **AddClass on** ✓ — `AiPlayerbot.AddClassCommand = 1`, `AddClassAccountPoolSize = 50`.
- **Eluna script path PINNED** ✓ — `ALE.ScriptPath = /azerothcore/env/dist/etc/modules/lua_scripts`
  (inside the container). The base compose bind-mounts `./env/dist/etc` → `/azerothcore/env/dist/etc`,
  so on the **host** this is `<server dir>/env/dist/etc/modules/lua_scripts/` — plain host-file
  deployment, no `docker cp`. (`party-setup` `mkdir -p`s it if absent.)
- **No live Lua reload** on this build — `.reload` has no eluna/lua/ale subcommand, and
  `ALE.AutoReload = false`. Eluna scripts load **at worldserver startup**, so first-time
  script deployment needs **one restart** (see §7). Not per-party — a one-time setup cost.
- SOAP up + `dmlsoap` GM3 account ✓ (from Plan 3).

## 5. Backend — bridge scripts + CLI verbs

### Bridge scripts (our own AGPL Lua, in the repo under `cli/lua/party/`)

Thin wrappers over documented Eluna bindings (reproduced from the spike's verb list, NOT copied from
The Lab). Each registers a `PLAYER_EVENT_ON_COMMAND`-style hook keyed by a private prefix; the SOAP
side triggers it by having the bridge look up the named player and, if online, run the playerbot
command in that player's session. Files:
- `dml_addclass.lua` — find player by name (abort if offline) → `Player:RunCommand(".playerbots bot addclass <class>")`.
- `dml_uninvite.lua` — remove a named bot from the player's group.
- `dml_login.lua` — re-attach/re-summon a bot after the player relogged.
- `dml_party_lib.lua` — shared "resolve online player or fail" helper.

Trigger transport: SOAP `executeCommand` reaches the bridge, which resolves the named player and
runs the playerbot command **in that player's session** (`Player:RunCommand`, confirmed to exist in
the AC Eluna flavor by the spike's rigor review). **Behavioral reference for Task 1:** re-read the
already-extracted Lab bridges on this box —
`~/squashfs-root/usr/lib/TheLab/eluna-scripts/dml_{addclass,uninvite,login}.lua` (plain text) — and
**reimplement** against the mod-ale Eluna API (AGPL: reimplement, do not copy bytes). Task 1 pins the
exact hook/registration (e.g. a custom command intercept via `PLAYER_EVENT_ON_COMMAND` vs a
world-tick mailbox) from those reference scripts + the mod-ale source in
`modules/mod-ale/src/lualib`. If the Lab extraction is gone, re-extract per the spike's Step 2.

### CLI verbs (`dml wow party …`, SOAP transport in `cli/src/20-soap.sh`, standard JSON envelope)

- `party-setup --json` → `{"changed":bool,"restart_required":bool,"checks":{...}}`
  Deploys the `dml_*.lua` scripts into `<server dir>/env/dist/etc/modules/lua_scripts/` (fixed file
  set — NOT a raw-write path), and preflights: mod-ale present, SOAP reachable, `AddClassCommand=1`.
  Idempotent (re-deploy only if content differs). `restart_required:true` whenever scripts changed
  (Eluna loads at startup; see §7). Distinct clean errors: `MISSING_DEP` (mod-ale absent),
  `SOAP_UNREACHABLE`, `NOT_FOUND` (wow title not installed), a preflight `BAD_ARG`-class for
  AddClass disabled.
- `party online --json` → `{"online":[{"name","class","level","guid"}]}` — read-only from
  `acore_characters.characters WHERE online=1` (bot accounts excluded). Powers the auto-detect.
- `party add --player <name> --class <c> --json` → `{"added":bool,"joined":bool,"note":"..."}`
  Validates `<c>` against the 9-class allowlist (see §6) and `<name>` against the charname allowlist,
  then fires the addclass bridge over SOAP. Polls `acore_characters.group_member` for a new member
  for ~6 s: `joined:true` on success, or `joined:false` + a soft note ("spawned but not yet attached
  — try Refresh"). Guards **character online first** → clear "log the character in first" error.
- `party list --player <name> --json` → `{"members":[{"guid","name","class","level","is_bot"}]}`
  Read-only from `group_member`/`groups` joined to `characters` (+ `playerbots_account_type` to flag
  bots). Online-guard as above (a party only exists while the player is online).
- `party kick --bot <name> --json` → `{"kicked":bool}` — fires the uninvite bridge.
- `party relogin --player <name> --bot <name> --json` → `{"relogged":bool}` — fires the login bridge.

All mutating verbs: online-guard, name/class allowlists, SOAP-serialized via the existing flock.

## 6. Launcher — the Playerbots page

Turns the disabled sidebar entry on (`+page.svelte` + a new `launcher/src/lib/pages/Playerbots.svelte`;
typed `api.ts` wrappers + Rust commands per verb). Three zones:

1. **Status strip** — auto-detected online character ("Building a party for **Testen**") + Refresh.
   Nobody online → friendly "log a character in first," rest of page disabled. Not-yet-set-up
   (scripts absent / preflight fail) → the one-time **"Enable My Party"** button, which streams
   `party-setup` into the terminal panel (same pattern as Save & Restart) and, on
   `restart_required`, offers the one-time **Restart to load** (the only disconnect the feature
   ever causes — behind the usual "this disconnects players" confirm).
2. **Add-a-bot row** — 9 class buttons (Warrior, Paladin, Hunter, Rogue, Priest, Shaman, Mage,
   Warlock, Druid). Click → `party add`; buttons disable in-flight; success refreshes the party list;
   the soft "spawned but not attached" case shows a gentle warning. (Death Knight omitted from the
   default set — level/rules-gated; can be added later if the pool supports it.)
3. **Current party** — live list from `party list`: name + class/level (playerbots are real rows in
   `acore_characters`, so the class/level join always resolves), a **Kick** button per member, a
   **Re-summon** button for a bot that dropped after a relog, and a Refresh.

Errors render as the standard error card. Add/kick need no confirm (cheap, reversible). Only the
one-time setup restart confirms. Party cap = 4 bots (WoW 5-person group; player + 4).

Security posture (no new privilege class): bots are added **by the player's own in-game session**
via the verified relay, never an elevated path; class input is allowlisted to the 9 classes before
it reaches any console command; `party-setup` writes only the fixed `dml_*.lua` file set (not a
steerable raw-write); bot names from `party list` flow back through the `^[A-Za-z0-9_]{1,12}$`
allowlist for kick/relogin; MySQL stays read-only (online-check, roster, group readback).

## 7. Setup / apply flow (amended after preflight)

Because this build has **no live Lua reload**, first-time `party-setup` deploys the scripts and
reports `restart_required:true`; the scripts load on the next worldserver **startup**. The launcher
runs this as a one-time "Enable My Party" → optional "Restart to load now" (streamed, confirmed).
After that one restart, every `party add`/`kick`/`list`/`relogin` works with **no further restarts**
(they're runtime SOAP calls, not script changes). A future nicety (out of scope): flip
`ALE.AutoReload = true` or find mod-ale's own reload verb to avoid even the one setup restart.

## 8. Testing & gates

- bats (stubbed SOAP + MySQL) for every verb: online-guard rejects when offline; class allowlist
  rejects junk (and each of the 9 valid classes passes); `party add` polls-then-confirms + the
  soft-warning (empty-poll) path; `party-setup` idempotence + each preflight failure (mod-ale
  missing / SOAP down / AddClass off) → its own clean error; `party list` bot-flagging; name
  allowlist on kick/relogin.
- cargo tests for the new typed commands (fixture pattern); vitest for any pure page logic;
  svelte-check 0/0. Full existing suites stay green.
- **Live gate (user-supervised, the real acceptance test):** with a character online and
  `party-setup` applied (+ one restart), click a class → the bot appears in the player's group
  in-game within ~6 s. If a join is refused, check `AiPlayerbot.GroupInvitationPermission`
  (default 1; gates invite acceptance by level/guild) as the spike flagged.
