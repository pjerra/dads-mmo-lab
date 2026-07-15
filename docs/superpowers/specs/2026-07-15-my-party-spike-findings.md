# SPIKE findings — resolving the "My Party" bot-join mechanism

**Date:** 2026-07-15
**Task:** Plan 3, Task 8 (SPIKE). Deliverable is this decision document; it builds no feature.
**Purpose:** Settle *exactly* how a bot joins a specific player's party without a human typing,
so Plan 4 (My Party) is written against a verified mechanism rather than a guess.

## Confidence legend

- **VERIFIED** — read directly from primary source (mod-playerbots source, the extracted Lab
  Eluna scripts, or the mod's config/SQL). Citations given.
- **INFERRED** — a reasonable conclusion drawn from verified pieces, but not literally observed.
- **PENDING-LIVE** — needs one live test on the running server (blocked today on the `dmlsoap`
  GM account not existing yet).

---

## Bottom line (TL;DR)

**My Party cannot be built over SOAP alone. The verified mechanism is: a bundled server-side
Eluna (mod-ale) helper script executes the playerbot chat command *in the online player's own
session* on the CLI's behalf.** The Lab does exactly this today — it ships five `dml_*.lua`
Eluna bridges and drives them by name through the worldserver console / SOAP `executeCommand`.
The player's character **must be logged in** while the party is built. Confidence: **VERIFIED**
(from mod-playerbots source *and* the Lab's own extracted scripts). The only PENDING-LIVE item
is a cosmetic confirmation of the exact failure string a naked SOAP `bot add` returns.

---

## What was inspected (and what was not)

| Source | Status | Notes |
|---|---|---|
| Live SOAP bot-add test (Step 1) | Attempted, **blocked on auth** | `admin/admin` → HTTP 401 → `SOAP_AUTH`; `dmlsoap` GM account not created yet. |
| The Lab AppImage / squashfs (Step 2) | **Fully inspected** | `Ubuntu` WSL distro, user `labtest`, `~/squashfs-root` present. Binary `usr/bin/the-lab` (Tauri app) + **six extractable Eluna scripts** under `usr/lib/TheLab/eluna-scripts/`. |
| mod-playerbots source (Step 3) | **Fully inspected** | `github.com/mod-playerbots/mod-playerbots@master` — command handler, command server, config, SQL all read. |

Nothing was unavailable. The Lab's `dml_*.lua` bridge scripts turned out to be shipped as plain
text on disk (not compiled into the binary), which made The Lab's exact mechanism directly
readable rather than merely inferred.

---

## Step 1 — the SOAP test (recorded, live-confirm deferred)

Command run (both singular/plural spellings):

```
dml wow soap-exec 'playerbots bot add Somebotname' --json
dml wow soap-exec 'playerbot  bot add Somebotname' --json
```

Result (both):

```json
{"ok":false,"error":{"code":"SOAP_AUTH","message":"SOAP authentication failed","hint":"Check admin account / gmlevel 3."}}
```

The CLI's SOAP client (`cli/src/20-soap.sh`) defaults to `admin/admin`; the server returns HTTP
401, which the client maps to `SOAP_AUTH`. The `dmlsoap` GM account the CLI will actually use
does not exist yet (user must create it at the worldserver console).

**PENDING-LIVE:** once `dmlsoap` exists, re-run the above to capture the *exact* rejection.
Source research (below) predicts it will fail even with valid credentials — either with
`"You may only add bots from an active session"` **or** an AzerothCore "command not available in
console" / usage rejection (because the command is registered `Console::No`). Either outcome
confirms SOAP-only My Party is dead; the test is now a formality, not a gate.

---

## The four questions

### Q1 — Mechanism: how a specific bot joins a specific player's party with no human typing

**VERIFIED. The path is: SOAP → Eluna helper command → playerbot command run in the player's session.**

The Lab ships these Eluna (mod-ale / AzerothCore-Lua-Engine) scripts, deployed to the server's
`AC_ALE_SCRIPT_PATH/lua_scripts` directory. Each registers `PLAYER_EVENT_ON_COMMAND` (Eluna hook
id **42**) and only fires for **console/SOAP origin** (`if player ~= nil then return end`):

| Script | SOAP-callable command | What it does (verified from script body) |
|---|---|---|
| `dml_addclass.lua` | `dml_addclass <player> <class> [gender]` | `p:RunCommand("playerbots bot addclass <class> [gender]")` — spawns a fresh class bot from the AddClass pool **as the player**, which auto-joins the player's group. |
| `dml_login.lua` | `dml_login <player> <botName>` | `p:RunCommand("playerbots bot login <botName>")` — logs an already-owned bot back in; mod-playerbots auto-rejoins it to the master's group. |
| `dml_whisper.lua` | `dml_whisper <player> <botName> <message…>` | `p:Whisper(msg, 0, bot)` — sends a whisper from the player to the bot, hitting mod-playerbots' chat-command hook (used for `talents spec …`, `autogear`, `maintenance`, etc.). |
| `dml_uninvite.lua` | `dml_uninvite <botName>` | `bot:RemoveFromGroup()` — detaches the bot from its group. |
| `dml_summon_npc.lua` | `dml_summon_npc <player> <creatureEntry>` | `p:SpawnCreature(...)` — temp service NPC at the player (transmog etc.). |
| `dml_gm.lua` | `dml_gm_power/health/money/revive …` | GM helpers routed through the player. |

Extraction path: `~/squashfs-root/usr/lib/TheLab/eluna-scripts/*.lua` (Ubuntu WSL, user `labtest`).

**Why the indirection is mandatory (VERIFIED from mod-playerbots source):**

1. The `.playerbots bot` command — which owns the `add` / `addclass` / `login` subcommands — is
   registered `SEC_PLAYER, **Console::No**`:
   `{"bot", HandlePlayerbotCommand, SEC_PLAYER, Console::No}`
   — `src/Script/PlayerbotCommandScript.cpp:45` (table nested under `{"playerbots", …}` at :44/:54).
   `Console::No` means AzerothCore's chat framework refuses to run it from a console/SOAP/RA
   session at all.
2. Even inside the handler there is an explicit session guard:
   ```cpp
   WorldSession* m_session = handler->GetSession();
   if (!m_session) { handler->PSendSysMessage("You may only add bots from an active session"); return false; }
   ```
   — `src/Bot/PlayerbotMgr.cpp:878-883`.

Eluna's `Player:RunCommand` runs the command through **that online player's own `ChatHandler`
/ session**, so both checks pass. This is precisely The Lab's workaround. It is mechanism **#4**
from the brief's candidate list ("SOAP staging + in-game trigger"), realized as *SOAP →
Eluna player-context relay*. Note it is **not** the client-addon-SavedVariables path, **not**
`BotAutologin`, **not** the TCP `:8888` command server (see Q-notes), and **not** a direct
`group_member` DB insert — the only DB touch is a *read* to confirm the join (below).

**Join confirmation (INFERRED, from Lab binary strings — co-occurrence, not traced control
flow):** after firing the command, The Lab appears to poll `acore_characters.group_member` for
the new member for ~6 s:
> "No new member joined the group within 6 s. The bot may have spawned but not yet attached"

Party membership is read with (Lab binary string, verbatim):
```sql
SELECT leaderGuid FROM acore_characters.groups g2
 WHERE g2.guid IN (SELECT guid FROM acore_characters.group_member WHERE memberGuid = ?)
```

**Fallbacks / verb map for Plan 4:**
- Spawn a *new* curated party member → `dml_addclass`.
- Re-attach an *existing owned* bot (e.g. after the player relogged) → `dml_login`.
- Configure a member (spec/talents/gear) → `dml_whisper … <playerbot command>`.
- Remove a member → `dml_uninvite`.
- Confirm the join → poll `group_member` (6 s budget).

### Q2 — Login requirement: must the player's character be logged in?

**VERIFIED: YES.** Every bridge begins with `GetPlayerByName(<player>)` and aborts (printing
"player not found/online") if that returns nil. The underlying reason is the `Console::No`
registration + the `GetSession()` guard above: the command only exists inside a live player
session. There is no offline path — you cannot pre-stage a party for a character that is not
online.

**Trigger of the actual join:** the Eluna `PLAYER_EVENT_ON_COMMAND` handler calling
`Player:RunCommand` (for `addclass`/`login`) or `Player:Whisper` (for config). mod-playerbots
then attaches the bot to the master's group via its standard group-invite path
(the `dml_login.lua` header notes "auto-rejoins them to the master's group via its standard
GroupInviteOperation queue"). No client addon and no `SavedVariables` file are involved on the
party-building path.

### Q3 — Account / ownership setup the pre-generated bots need

**VERIFIED.** `PlayerbotMgr::AddPlayerBot` (`src/Bot/PlayerbotMgr.cpp:99-138`) allows a master to
control a bot if **any** of these hold:

```cpp
bool isRndbot      = !masterAccountId;                                             // random bot, no master
bool sameAccount   = allowAccountBots      && accountId == masterAccountId;        // own-account alt
bool sameGuild     = allowGuildBots        && guild && guild->GetMember(playerGuid);
bool addClassBot   = sRandomPlayerbotMgr.IsAddclassBot(playerGuid.GetCounter());
bool linkedAccount = allowTrustedAccountBots && IsAccountLinked(accountId, masterAccountId);
// else → "Failure: You are not allowed to control bot <name>"
```

Plus a per-master cap: `count >= maxAddedBots` → `"Failure: You have added too many bots (more than <n>)"` (:132-136).

Relevant config (VERIFIED, `conf/playerbots.conf.dist@master`):

| Key | Default | Meaning |
|---|---|---|
| `AiPlayerbot.MaxAddedBots` | 40 | Per-master cap on controlled bots. |
| `AiPlayerbot.AllowAccountBots` | 1 | Invite alts from the player's own account. |
| `AiPlayerbot.AllowGuildBots` | 1 | Invite alts in the player's guild. |
| `AiPlayerbot.AllowTrustedAccountBots` | 1 | Invite from linked/trusted accounts. |
| `AiPlayerbot.BotAutologin` | 0 | Auto-login all of a player's alts as bots on login. |
| `AiPlayerbot.AddClassCommand` | 1 | Enable the `addclass` command (0 = GM only). |
| `AiPlayerbot.AddClassAccountPoolSize` | 50 | Accounts the mod auto-creates & reserves for addclass. |

**The Lab's My Party uses the AddClass pool** — the `addClassBot`/`IsAddclassBot` branch — which
needs **no per-player account provisioning**. mod-playerbots auto-creates the addclass account
pool at startup; the CLI only needs `AddClassCommand = 1` and a large enough
`AddClassAccountPoolSize`. Bot accounts are tagged in the `acore_playerbots.playerbots_account_type`
table, whose `account_type` column is (VERIFIED, `data/sql/playerbots/base/playerbots_account_type.sql`):

> `0 = unassigned, 1 = RNDbot, 2 = AddClass`

The Lab's roster query for the party picker reads exactly these (binary string, verbatim):
```sql
SELECT c.guid, c.name, c.class, c.level
  FROM acore_characters.characters c
  JOIN acore_playerbots.playerbots_account_type t ON t.account_id = c.account
 WHERE t.account_type IN (1, 2);
```

**How to satisfy from the CLI:** the shipped defaults already satisfy this —
`playerbots.conf.dist` has `AiPlayerbot.AddClassCommand=1` and `AddClassAccountPoolSize=50`, so
in the common case nothing needs writing; just use the addclass relay. (No CLI path writes
`AiPlayerbot.*` keys today — if Plan 4 wants to expose these knobs, that's a new verb to build,
following the `dml wow soap-setup` env-var/override pattern in `cli/src/90-main.sh`.) No manual
`.account create` needed. (Own-account / guild / linked paths remain
available as alternatives but require the bot characters to already exist under the right
account — more provisioning, so not the recommended default.)

### Q4 — Ambient random-bot interaction (`RANDOM_BOTS` counts / `flush_random_bots`)

**VERIFIED that they are independent systems; changing random-bot counts is NOT required for a
curated party.** Random bots are `account_type = 1`; addclass party bots are `account_type = 2`
— disjoint account pools. Curated addclass bots count toward the per-master `MaxAddedBots` cap
(40) but **not** toward `MinRandomBots`/`MaxRandomBots`, and random bots are not pulled into a
player's group.

The Lab still exposes `flush_random_bots` as an **optional** "quiet the world" action, not a
prerequisite. Its implementation (VERIFIED from Lab binary strings) sets the random-bot config
and restarts:
- toggles `AiPlayerbot.MinRandomBots` / `AiPlayerbot.MaxRandomBots` / `AiPlayerbot.RandomBotAutologin`,
- references `AiPlayerbot.DeleteRandomBotAccounts`,
- shows "Restarting the server so it can remove all random bots" / "wiping".

Config defaults (VERIFIED, conf@master): `MinRandomBots = 500`, `MaxRandomBots = 500`,
`RandomBotAutologin = 1`. The running server is configured for ~2000 random bots, so a "reduce
ambient bots while I play with my party" toggle is a reasonable Plan 4 nicety — but it must be
**decoupled** from party creation, and it costs a worldserver restart.

**Note on the TCP command server (`AiPlayerbot.CommandServerPort = 8888`):** VERIFIED it is
**not** a party mechanism. Its dispatch (`RandomPlayerbotMgr::HandleRemoteCommand`,
`src/Bot/RandomPlayerbotMgr.cpp:2967-2988`) parses `"<command>,<guid>"`, requires an
**already-spawned** bot by GUID, and forwards to `PlayerbotAI::HandleRemoteCommand`
(`src/Bot/PlayerbotAI.cpp:5203+`) which only answers read-only introspection —
`state` / `position` / `values` / `travel`. It cannot add a bot or form a group. Ignore it for
My Party.

---

## Prerequisites the mechanism implies (for the installer / Plan 4 preflight)

All VERIFIED from Lab strings (`hasModAle`, `luaScriptsMissing`, `hasElunaMount`, `hasSoapEnv`,
`AC_ALE_SCRIPT_PATH/lua_scripts`, `AC_SOAP_ENABLED`) + the scripts themselves:

1. **mod-ale (Eluna / AC Lua Engine) installed** in the playerbots server image. Without it the
   `dml_*.lua` bridges never load and there is no way to run a player-session command from SOAP.
2. **The `dml_*.lua` bridge scripts deployed** to the server's Eluna scripts dir
   (`env/dist/bin/lua_scripts`, exposed via `AC_ALE_SCRIPT_PATH`). Our repo will need its own
   copies (or equivalents) — they are ~50–115 lines each and trivially reproducible from the
   verbs above. (This repo is AGPL; re-implement rather than copy Lab bytes.)
3. **SOAP enabled** (`AC_SOAP_ENABLED=1`) and a GM account at gmlevel 3 (the `dmlsoap` account).
4. **`AiPlayerbot.AddClassCommand = 1`** (+ adequate `AddClassAccountPoolSize`).
5. **The target character online** during every party operation.

---

## Recommended Plan 4 task breakdown (My Party), built on the verified mechanism

1. **Ship the Eluna bridge scripts.** Add our own AGPL `dml_addclass.lua`, `dml_login.lua`,
   `dml_whisper.lua`, `dml_uninvite.lua` (summon/gm optional) to the playerbots server image and
   wire deployment into `install wow-server-playerbots` (copy into `AC_ALE_SCRIPT_PATH`). Preflight
   check: mod-ale present + scripts loaded + SOAP up + `AddClassCommand=1`.
2. **`dml wow party` CLI verbs** over the SOAP `executeCommand` transport (reusing `cli/src/20-soap.sh`):
   - `party add --player <name> --class <c> [--gender m|f]` → `executeCommand("dml_addclass <name> <c> [gender]")`.
   - `party config --player <name> --bot <name> --cmd "<playerbot chat cmd>"` → `dml_whisper …`
     (spec/talents/gear from the role→class→spec→level / Wowhead-talent-code UI).
   - `party relogin --player <name> --bot <name>` → `dml_login …`.
   - `party kick --bot <name>` → `dml_uninvite …`.
3. **Join confirmation + readback.** After add, poll `acore_characters.group_member` /
   `groups` (MySQL reader, `cli/src/30-db.sh`) for the new member with a ~6 s timeout; surface
   "spawned but not yet attached" as a soft warning, not a hard failure. Implement
   `party list --player <name>` (= `get_user_party`) from the same tables.
4. **Precondition guard = character online.** Before any party op, verify the character is online
   (`characters.online`); if not, return a clear "log the character in first" error. This is the
   #1 predictable failure and must be first-class in the UX.
5. **Bot roster source.** Populate the party picker from
   `playerbots_account_type.account_type IN (1,2)` joined to `characters` (+ talent query), as
   The Lab does — read-only MySQL.
6. **Presets.** `save/list/export/import_party_preset` as TOML under
   `~/.config/dads-mmo-lab/party-presets/*.toml` (already in the design).
7. **(Optional, decoupled) Ambient-bot quieting.** A separate `dml wow randombots --count N`
   action that writes `MinRandomBots`/`MaxRandomBots` and restarts — explicitly **not** part of
   party creation, and clearly flagged as "restarts the server".
8. **Live gate (user-supervised).** With `dmlsoap` created and a character online, end-to-end:
   `party add` → bot appears in the player's group in-game within 6 s. This is the real
   acceptance test that the whole relay works on this box. Check item: if the join is refused,
   inspect `AiPlayerbot.GroupInvitationPermission` (default 1) — it gates invite acceptance by
   level/guild in `PlayerbotSecurity::LevelFor` and could bite if the curated bot's level and
   the master's level diverge sharply (likely only affects non-owner invites to random bots,
   but cheap to rule out at the gate).

---

## Open items / what a live test would still settle

- **~~PENDING-LIVE~~ → LIVE-CONFIRMED (2026-07-15, as `dmlsoap`):** naked SOAP
  `playerbots bot add Somebotname` (and the `playerbot` spelling) fails with the
  console-availability/usage rejection — the SOAP fault is the `.playerbots` USAGE text
  listing only `debug / gtask / pmon / rndbot`. The `bot` subcommand (Console::No) is not
  even *visible* to a session-less invoker, so the command never reaches the
  "active session" guard. SOAP-only bot add: confirmed impossible, live, exactly as
  concluded above.
- **PENDING-LIVE (acceptance):** the full relay (SOAP → `dml_addclass` → in-game join) has not
  been exercised on this server because the bridge scripts are not yet deployed here and no GM
  SOAP account exists. Task 8 of the Plan 4 breakdown is that test.
- **INFERRED (low risk):** that our reimplemented bridge scripts behave identically to The Lab's
  — they will, because they are thin wrappers over documented Eluna bindings (`Player:RunCommand`,
  `Player:Whisper`, `Player:RemoveFromGroup`) and the verified mod-playerbots command surface.

---

## Sources

- The Lab, extracted: `~/squashfs-root/usr/lib/TheLab/eluna-scripts/{dml_addclass,dml_login,dml_whisper,dml_uninvite,dml_summon_npc,dml_gm}.lua` (Ubuntu WSL, user `labtest`); binary strings from `usr/bin/the-lab`.
- mod-playerbots @ `github.com/mod-playerbots/mod-playerbots@master`:
  - `src/Script/PlayerbotCommandScript.cpp:44-54` — `.playerbots bot` registered `SEC_PLAYER, Console::No`.
  - `src/Bot/PlayerbotMgr.cpp:878-883` — `"You may only add bots from an active session"` session guard.
  - `src/Bot/PlayerbotMgr.cpp:99-138` — `AddPlayerBot` ownership rules + `MaxAddedBots` cap.
  - `src/Bot/RandomPlayerbotMgr.cpp:2967-2988` — command server dispatch (`<cmd>,<guid>`, existing bot only).
  - `src/Bot/PlayerbotAI.cpp:5203+` — per-bot remote commands: `state`/`position`/`values`/`travel` (read-only).
  - `conf/playerbots.conf.dist@master` — `MaxAddedBots`, `AllowAccountBots`, `AllowGuildBots`, `AllowTrustedAccountBots`, `BotAutologin`, `AddClassCommand`, `AddClassAccountPoolSize`, `MinRandomBots`, `MaxRandomBots`, `RandomBotAutologin`, `CommandServerPort`.
  - `data/sql/playerbots/base/playerbots_account_type.sql` — `0=unassigned, 1=RNDbot, 2=AddClass`.
- This repo: `cli/src/20-soap.sh` (SOAP client / `admin/admin` default → `SOAP_AUTH` on 401); Step 1 output above.
