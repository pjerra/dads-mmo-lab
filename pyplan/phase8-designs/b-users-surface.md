# Phase 8 design — the user's surface

> One of three independent designs for `pyplan/phase8-parity-decisions.md`, written 2026-09-06
> from the surface angle: which buttons exist, what each shows, and what the app does when the
> server cannot answer. Every `path:line` is at `7bc5ebd3` under `pylauncher/` unless a
> `pyplan/` page is named; emulator citations are the `phase8-reads/*.md` reports at the pinned
> revisions listed in `phase8-delta.md` "Provenance". The owner's eight answers on the parity
> page are taken as given and are not reopened here. This document writes no code and names the
> seam layer abstractly — "what the surface needs from it" — so the seam design can name modules.

---

## 1. The decision

**Every one of the nine steps is reached from the existing Controller, on the tab whose job it
already is, with two new tabs where nothing fits: 8.1 (the command channel) and 8.2 (dashboard,
log snapshot) and 8.8 (Steam) and 8.9 (uninstall) land on the Server tab; 8.3 on the Accounts
tab; 8.7 on the Modules tab; 8.4 (teleport, mail, revive, level, rename, gear sets) gets a new
"Characters" tab; 8.5 and 8.6 (Browse Bots, My Party) get a new "Bots" tab.** The user never
reads the word "SOAP": they see "commands: ready" on the Server tab, and every button that needs
the channel is enabled by that one line. Every question a button asks is rendered in all three
outcomes — the server said yes, the server said no, the app could not ask — and while the server
is down, starting, or shutting down the button is disabled and the sentence beside it says which.
**The one rule for a thing a tree cannot do:** the control is not built. In its place the tab
holds one sentence naming the server and what it lacks (the Console tab's Send idiom,
`ui/controller_view.py:1292-1304`), and the sentence and the absence come from the same predicate
on the entry's Phase 8 capability data — never from a game id, never from two places that can
disagree. A thing the *host* cannot do right now (no pty, no Steam) is the second shape: drawn,
disabled, with its reason, as Send is today. A thing the *server state* forbids (down, starting,
shutting down) is the third: drawn, disabled, and the status line says why.

---

## 2. Architecture and module layout

### 2.1 Files, in the style-guide §3 shape

| Layer | Module | Owns | Must never |
|---|---|---|---|
| `ui/controller_view.py` (changed, add-after only) | `ControllerServices` gains the seams in §2.2; `_build_server_tab()` gains the commands line, the dashboard block, "Save the worldserver log", "Uninstall…" and (Linux only) the Steam group **after** `box.addWidget(self.repair_label)` at `:794`; `__init__` gains `self.refresh_status()` after `self._timer.start(...)` at `:732` (closes bug-checklist line 552); `_set_busy()` at `:982` also locks every Phase 8 action button; two new tab builders `_build_characters_tab()` and `_build_bots_tab()` are one-liners that compose the sub-views below and connect their `action_failed` up | The status poll's cadence, `_busy`, the arm/confirm idiom, the console single-flight flag (`:1341`), the `LineRelay` pattern (`:719-720`) | Rewrite any existing builder; branch on a game id (it reads `entry` capability data only); shell out |
| `ui/widgets/outcome.py` (new) | `OutcomeLabel(QLabel)`: renders one `Outcome` (§2.3) as `✓ <text>` / `✗ <text>` / `? could not ask: <text>` plus the `effect` sentence; `ChannelBanner(QLabel)`: renders one `ChannelState` as the "commands: …" sentence. One widget, every tab, so the three outcomes cannot be collapsed by a tab that forgot one | Know which tab uses it or what the command was |
| `ui/characters_view.py` (new) | The Characters tab (8.4): the human-character picker, the teleport / mail item / mail gold / revive / set level / rename / gear-set groups; each button → one `self._run(...)`; each result → one `OutcomeLabel`; a `not_drawn(reason)` helper that puts the reason label where a group would have gone | Business logic, SQL text, command text, a subprocess; reach up into `ControllerView` (it emits `action_failed`) |
| `ui/bots_view.py` (new) | The Bots tab (8.5 + 8.6): the Browse table with filter and paging; the My Party group (drawn only when `capabilities.party` is true — WotLK — and replaced by its reason label otherwise); the bridge-missing card with "Set up the party bridge" | Decide what a bot is (the seam's clause does); poll the database itself |
| `ui/uninstall_dialog.py` (new) | The dialog `pyplan/phase8-decisions.md:162-176` already designed: size from `plan()`, the "Keep my characters" box, the typed confirmation, Cancel/Uninstall; returns the user's choice and nothing else | Run the uninstall (the view's job runner does, off the GUI thread); know a container name |
| `ui/tab_titles.py` | unchanged | — |
| `main.py` (changed, add-after) | `add_controller()` connects `view.uninstalled` (signal up) to a new `on_uninstalled(game, server_dir)` that calls `state.forget()`, `_warn_unless_remembered()`, `drop_controller(key)` and `catalog_view`'s tile reset — after the existing `view.action_failed.connect(...)` at `:275-277` | Tear a tab down from inside the view (`drop_controller` stays the window's, `:189-214`) |
| `ui/catalog_view.py` (changed, add-after) | `installed_games` seeding gains a `forget_installed(game_id)` so the tile reads "Install" again after an uninstall (`_show_installed` at `:412` is the existing half) | — |

`controller_view.py` is 1937 lines today; the two new tabs are sub-views so the file grows by
the wiring only. Every sub-view is handed the slice of `ControllerServices` it needs (call down)
and emits `action_failed(str)` (signal up), the way `LogPanel` emits `run_finished`
(`ui/widgets/log_panel.py:113-114`).

### 2.2 What `ControllerServices` gains, and what each seam must return

Named abstractly; the seam design names the modules. Every callable below is called **only**
through `ControllerView._run()` (`:839-847`) or the sub-view's equivalent, i.e. on a
`ThreadedJobRunner` thread (`ui/widgets/job.py:148-186`), never on the GUI thread. The view's
`_console_pending` flag (`:1341-1351`) is the model for every one of them: one press in flight
per tab, the button re-armed only in the done/error slot.

| New field on `ControllerServices` | Signature the surface needs | Three-outcome shape | Step |
|---|---|---|---|
| `capabilities: Capabilities` | plain data read off the catalog entry at `for_entry()` time; fields the surface reads: `channel: "soap" \| "console" \| "none"`, `set_level: bool`, `mail_items_per_message: int`, `gm_levels: tuple[tuple[int, str], ...]`, `max_level: int`, `party: bool`, `gear_read: bool`, `bot_marker: str` (a sentence for the Bots tab's column header) | none — data | all |
| `channel_probe` | `() -> ChannelState` | `state ∈ {ready, not-set-up, starting, shutting-down, down, no-channel, could-not-ask}`, `reason: str` | 8.1 |
| `channel_setup` | `(progress: Callable[[str], None]) -> Outcome` | yes = verified by a real round-trip; no = the server/database refused (text); could-not-ask = nothing could be reached | 8.1 |
| `dashboard` | `() -> Dashboard` | `players: int \| None`, `bots: int \| None`, `container_since: str`, `restart_count: int`, `settled: bool`, each `None` field carrying its own `reason` | 8.2 |
| `save_world_log` | `(mode: str) -> Outcome` (`text` names the file) | saved / docker refused / container absent | 8.2 |
| `accounts_list` | `() -> tuple[AccountRow, ...]` (`name, level, level_name, characters`) | rows / empty (with the human-filter warning) / raises → could-not-ask | 8.3 |
| `account_set_password` | `(name, password) -> Outcome` | as `Outcome` | 8.3 |
| `account_set_level` | `(name, level) -> Outcome` with `effect ∈ {"now", "after a restart"}` | as `Outcome` | 8.3 |
| `characters_list` | `() -> tuple[CharacterRow, ...]` (`name, level, class_name, online`) humans only | rows / empty / could-not-ask | 8.4, 8.6 |
| `teleports` | `(search: str) -> tuple[str, ...]` (≤ 500 names) | as above | 8.4 |
| `items` | `(search: str) -> tuple[ItemRow, ...]` (`entry, name, quality, item_level, required_level`) | as above | 8.4 |
| `gm_teleport`, `gm_mail_item`, `gm_mail_gold`, `gm_revive`, `gm_set_level`, `gm_rename` | `(character, …) -> Outcome`; `gm_set_level` is `None` where `capabilities.set_level` is false | as `Outcome` | 8.4 |
| `gear_read`, `gear_mail` | `(character) -> GearSet`; `(character, set) -> tuple[Outcome, ...]` one per mail sent | per mail | 8.4 |
| `bots_page` | `(filter: BotFilter) -> BotPage` (`total, rows, marker_note`) | rows / `total == 0 and characters > 0` (the fail-open warning) / could-not-ask | 8.5 |
| `party: PartyActions \| None` | `bridge_probe() -> Outcome`, `bridge_setup(progress) -> Outcome`, `add(character, class_id, spec_name, level, progress) -> Outcome`, `members(character) -> tuple[str, ...]`, `kick(character, bot) -> Outcome`, `specs(class_id) -> tuple[str, ...]` | as `Outcome`; `None` on every non-WotLK entry | 8.6 |
| `modules_check_updates`, `modules_update` | `() -> tuple[UpdateRow, ...]`; `(manifest) -> ApplyReport` | rows / none / could-not-ask | 8.7 |
| `steam: SteamActions \| None` | `probe() -> Outcome` (found, with the path / not found / could not read), `add_shortcuts(client_exe) -> Outcome` | as `Outcome`; `None` off Linux | 8.8 |
| `uninstall_plan`, `uninstall_run` | `() -> UninstallPlan` (`size_bytes, containers, volumes, images, unknown: tuple[str, ...]`); `(plan, keep_characters, progress) -> Outcome` | as `Outcome` | 8.9 |

### 2.3 The three-outcome type, and the server-state type

The surface needs exactly one result shape for every question, and one state shape for the
channel. It never infers a third outcome from the absence of the first two (kickoff §7 rule 2):

```
Outcome(kind: "yes" | "no" | "could-not-ask", text: str, effect: str = "")
ChannelState(state: "ready" | "not-set-up" | "starting" | "shutting-down" | "down"
                    | "no-channel" | "could-not-ask", reason: str)
```

`text` is what the server printed (AC returns the print buffer on success and the same text as a
fault on failure, `ACSoap.cpp:133-140`; TBC/Vanilla the same, `MaNGOSsoap.cpp:135,:140`; the
attach console returns `ConsoleReply.lines` with `prompted`, `console.py:84-107`) or, for
could-not-ask, why (connection refused; no prompt in the window; "Server is shutting down",
`ACSoap.cpp:128`). `effect` is the one sentence a person can act on: "takes effect now", "at the
character's next login" (rename, `cs_character.cpp:332`; TBC `Level2.cpp:3645-3654`), "after a
restart" (a Tortoise `rank` written to the database, `AccountMgr.cpp:250-256`).

### 2.4 Who owns each relationship on the window (kickoff §7 rule 7)

| Relationship | Owner | The rule |
|---|---|---|
| status poll ↔ channel probe | the poll (`refresh_status`, `:849-861`) owns the cadence; the probe owns the meaning | The probe is asked only when `status.world` is true, at most once per 3 polls (15 s), and not again once `ready` until `status.world` goes false — the `_ask_about_the_import` shape (`:904-920`). A probe on AC/TBC/Vanilla is a `server info` on the world thread (fact 4); this bounds it to one every 15 s while starting, zero once ready |
| Server-tab `_busy` ↔ every Phase 8 button | `_set_busy()` (`:982-1010`) | Start/Stop/Remove/Repair/Uninstall lock every command button on every tab; a command button never locks the Server tab (a teleport must not disable Stop) |
| one command in flight ↔ many tabs | each tab's own pending flag (the `_console_pending` shape, `:1341`) for the button; the seam's lock for the wire | Two tabs may each have one press pending; the seam serialises them on the channel (one SOAP request at a time on TBC/Vanilla is the server's own rule, `MaNGOSsoap.cpp:51-63`; the world thread serialises everything, fact 4). The Console tab's Send stays on `docker attach` and keeps its own flag; attach and SOAP are different transports on the same world queue, so they never share a tty and cannot corrupt each other's reply |
| `_console_available()` (`:1353-1363`) ↔ Tortoise's command buttons | the one predicate | Tortoise's `channel` is `"console"`; every command button on a Tortoise tab is enabled by the same `can_send()` that enables Send, and disabled with the same `NO_TTY_HELP`/`NO_SCRIPT_HELP` text |
| log snapshot ↔ Stop / Remove / Uninstall | the seam's `stop`-shaped call orders it: snapshot first, then stop | A snapshot that fails never blocks a stop; its outcome is one line in `problem_label` after the stop's own |
| My Party ↔ Browse Bots | the Bots sub-view | A finished party job calls the browse refresh so the new bot rows appear without a press |
| Modules Install ↔ server state | the Modules tab, from the same `InstallStatus` the poll delivers (`status_changed`, `:679`) | A manifest with a `sql` step into `characters` or `world` is not installable while `status.world` is true (owner Q7: no direct writes to those while running; today `apply.py` has no guard, delta fact 9). The button is disabled and the report says "Stop the server first: this module changes the world database" |
| uninstall ↔ `main.py` registries | the window (`drop_controller`, `main.py:189-214`) | The view emits `uninstalled`; it never removes its own tab. `state.forget()` (`state.py:82-86`) runs last, as the decisions page requires (`phase8-decisions.md:154-158`) |
| helper-account credentials ↔ the Accounts list | the seam hides the app's own account by name from `accounts_list` | The Server tab's commands line is where that account is seen; the Accounts list never offers to change or delete it |

---

## 3. The surface, per step

### 3.1 — The command channel (8.1)

**Tab:** Server. **Widgets, added after `self.repair_label` at `:794`:**

```
  WoW WotLK — /home/pk/wow-server-playerbots
  status: db up, auth up, world up
  commands: ready                                   ← ChannelBanner
  players 1 · bots 498 · container up 1h 12m · restarts 0     ← 8.2
  [Start] [Stop] [Refresh] [Stop and remove containers…] [Uninstall…]
  [Set up server commands]                          ← only while commands: not set up
  (problem_label — paragraphs, as today)
```

The user sees "commands", never "SOAP" or "helper account" in a label; the tooltip on the banner
spells it out once ("Yu'lon keeps a small administrator account on this server so its buttons can
send commands"), logs stay acronym-only (style-guide §6).

| ChannelState | Banner text | Set-up button | Every command button on other tabs |
|---|---|---|---|
| `ready` | "commands: ready" | hidden | enabled |
| `not-set-up` | "commands: not set up — press Set up server commands" + `reason` ("no helper account yet" / "the helper account's password file is missing" / "the server refused the helper account's password") | shown, enabled | disabled; each tab's banner repeats the line |
| `starting` | "commands: waiting for the world to finish loading" | hidden | disabled |
| `shutting-down` | "commands: the server is shutting down (up to 5 minutes)" | hidden | disabled |
| `down` | "commands: the server is stopped" | hidden | disabled |
| `no-channel` (Tortoise) | "commands: through the console (each one takes about 4 s)" when `can_send()`; else `NO_TTY_HELP` verbatim (`console.py:118-124`) | **never drawn** on this entry | enabled iff `_console_available()` |
| `could-not-ask` | "commands: could not ask — " + `reason` | hidden | disabled |

**One press or two.** With the world **down**, "Set up server commands" is **one press**: the seam
writes the config (AC: `AC_SOAP_ENABLED=1` into the override env, the same mechanism as
`AC_AI_PLAYERBOT_MAX_RANDOM_BOTS`, `catalog.json:57-59`, `Config.cpp:435-438`; TBC/Vanilla:
`SOAP.Enabled = 1` through `families/conf.py`, dist `mangosd.conf.dist.in:1858-1860`), writes the
helper account's SRP6 row through the existing `accounts.create_account` path
(`controller_wow_wotlk/accounts.py:312`, idempotent by name, `:327-341`), and the banner reads
"commands: will be checked when the server starts". With the world **up** it is **two-press**,
armed with the Remove/Repair idiom (`REMOVE_IDLE`/`REMOVE_ARMED`, `:628-646`): the armed paragraph
says "This restarts the world server; anyone playing is disconnected and their characters are
saved first (up to 5 minutes). Press again." — a restart is not data-destroying, but it is the
one action here that interrupts a person at a client, and the tab has one arm/disarm shape
(`:639-646`) rather than two kinds of confirmation.

**What runs where.** The press → `self._run(lambda: services.channel_setup(relay.emit_line), self._setup_done, self._setup_failed)`; progress lines arrive through a `LineRelay` (`job.py:194-215`) into `problem_label` exactly as the import's do (`:1229-1246`): "writing the server config…", "creating the helper account…", "restarting the world server…", "asking the server to answer…". The verify is a real round-trip (`server info`, PLAYER(0) `Console::Yes`, `cs_server.cpp:89`; TBC `Chat.cpp:814`) **before** the credentials file is written — the Rust launcher's own record is why (`soap_autosetup.rs:116-120`: a create-then-verify failure that persisted the create made one row per tick, forever). The credentials live in `<server_dir>/.yulon-commands.json`, 0600, beside `.db_password` — the seam names the file; the surface needs only "is it there, and what to say when it is not".

**The three outcomes after the press.** Yes: banner "commands: ready", `problem_label` "The server answered. Every command button is now live." No: banner "commands: not set up — the server refused: <fault text>", the button stays, `problem_label` carries the text and one remedy ("The helper account is named `yulon`. If you made an account with that name yourself, give it GM level 3 on the Accounts tab and press Set up again."). Could-not-ask: banner "commands: could not ask — <reason>" and the button stays. There is **no contentless "already done"** outcome (`soap_autosetup.rs:154-160`): the banner is re-derived from the machine on every poll, so a reopened window shows the same state a fresh one would.

**What a dad does when the account cannot be made.** The failure text is on the tab and in the log (`action_failed`, `main.py:275-277`). The Accounts tab's Create still works — it writes the same kind of row (`:1447-1474`). The remedy sentence above tells them to make `yulon` by hand with GM level 3; the seam's next press finds it by name (never a second account) and resets its password by the SRP6 row — an auth-database write, which Q7 allows and `accounts.py` already does (`:111`).

**Down / starting / shutting down.** The table above; the "starting" row is the one the Rust launcher got wrong in the other direction (`status.rs:406-410`: a boot loop read as "still waiting"). Here the probe returns `starting` only while `container_state().settled` is true and the channel connects but does not answer within its reply timeout; a container that is not `settled` or whose `restart_count` grew reads as `could-not-ask` with "the world server keeps restarting (N restarts)" — the 8.2 dashboard shows the same number (closes bug line 499's shape for every family).

**Family cannot.** Tortoise has no SOAP and no RA (`TW src/mangosd/CMakeLists.txt:19-30`, `dep/src/CMakeLists.txt:19-25`); `channel` is `"console"`, the set-up button is not built, and its stock commands go through `send_command()` (`console.py:228-336`, ≥ 3.6 s each, fact 1) with `ConsoleReply.prompted=False` mapped to `could-not-ask` ("no console prompt in the reply window — the world server may still be starting", the Console tab's own wording at `:1381-1384`). Its `pending_commands` table (60 s, no output, `TW World.cpp:2724,:3946-3948`) is **not** a surface: a button whose answer arrives a minute later with no text is a question with one outcome.

**Visible in-game effect.** None directly — the proof is on the tab: "commands: ready" after a press, and the Accounts list (8.3) showing `yulon` is *not* listed while `account onlinelist` typed on the Console tab still answers. **Box and capture:** `yulon-ubuntu` (WotLK, Off at baseline) → `pyplan/gates/8.1a-wotlk-yulon-ubuntu/`: a screenshot of the banner before and after, the transcript of `problem_label`, and — asked of the machine, not the app — `docker exec ac-database mysql … "SELECT username FROM acore_auth.account"` and `"SELECT gmlevel FROM account_access"` read from the box shell, plus one `curl` SOAP `server info` from the box with the file's credentials. TBC and Vanilla on `m910q` (one at a time; Tortoise stopped and restored, announced) → `8.1b-tbc-m910q/`, `8.1b-vanilla-m910q/`, the same captures against `realmd.account.gmlevel`. Tortoise → `8.1c-tortoise-m910q/`: a screenshot showing the console banner and no set-up button, and the test in §7 that asserts the absence.

### 3.2 — Dashboard and the log snapshot (8.2)

**Tab:** Server. **Widgets:** the dashboard line under the banner (mock-up above); "Save the worldserver log" (one press) on the **Console** tab, after `self.follow_button` at `:1306`, because that is where the log already is.

| Field | Source (per tree) | When `None` |
|---|---|---|
| players | a database read: `characters.online = 1 AND NOT <bot clause>` — AC `acore_characters.characters` (`characters.sql:49`) with the two-signal clause (`botid.rs:112-113,:133-134,:164`); TBC `characters.sql:811`, Vanilla `:659`, Tortoise `tw_char` `create_databases.sql:982`, each with the account-prefix clause from the installed conf's `AiPlayerbot.RandomBotAccountPrefix` (TBC/Vanilla dist `RNDBOT`, code default `rndbot`, `PlayerbotAIConfig.cpp:500`; Tortoise `:545`) | "players ?" with the reason in the tooltip |
| bots | the same read, inverted | "bots ?" |
| container up | `docker.container_state(spec.world).started_at` (`docker.py:1883-1910`) rendered as an age | "container up ?" |
| restarts | `container_state().restart_count`; when `settled` is false the whole line is replaced by **"world: restarting (N restarts) — not up"** and the status word for world reads "restarting", not "up" | — |

Bots and players are reads through `DockerSql.query` (`apply.py:504-528`), off the world thread (fact 4, "only direct MySQL reads bypass it"); the line refreshes every 3rd poll while `status.db` is true. No `server info` is fired for the dashboard on any tree — the channel probe (8.1) is the only periodic command, and it stops once `ready`.

**Bug boxes this step closes.** Line 552 (first interval says unknown with Start enabled): `refresh_status()` called once after the timer starts. Line 499 (a crash-looping mangosd reads as up): `settled`/`restart_count` are what the status words are rendered from, on every family — the same two primitives `wait_ready()` already uses (`docker.py:1861-1880`). Line 289 (window geometry) is **inherited**, not closed: Phase 9's settings page owns it (Q8iii).

**The snapshot.** Before `Controller.stop()` (`controller.py:272-292`), `remove()` (`:294-305`) and the uninstall, the seam runs `docker logs --tail 2000 <world container id resolved through this install's own compose project>` (`logsnap.rs:277-327` records why a service, not a name, `:77-84`) into `config_dir()/logs/world-<stamp>-<game>-<mode>.log`. The done label gains one line: "Saved the last 2000 lines of the worldserver log to <path>." / "Could not save the worldserver log: <reason> — the stop went ahead." / "No worldserver container to read a log from." The Console tab's button gives the same three outcomes on demand.

**Visible effect.** A file a person can open, named on the tab; the dashboard numbers agreeing with `docker exec … mysql "SELECT COUNT(*) FROM characters WHERE online=1"` and `docker inspect --format '{{.RestartCount}}'` typed on the box. **Boxes:** all four, one each — `8.2-wotlk-yulon-ubuntu/`, `8.2-tbc-m910q/`, `8.2-vanilla-m910q/`, `8.2-tortoise-m910q/`: screenshot of the line + the two shell readbacks + the saved log file's first and last line. The restart-loop rendering is proved once, on `m910q` TBC, by the `7.9-m910q-tbc-restarts.journal` shape: stop the database container under a running mangosd, screenshot "restarting (N)".

### 3.3 — Accounts: list, set password, GM level (8.3)

**Tab:** Accounts. Closes bug-checklist line 239. **Widgets, added after the "Create account" group at `:1442`:**

```
  ┌ Create account ────────────────────────┐   (unchanged, :1412-1424; the GM spin box's
  │ Username [        ]  Password [      ] │    range now comes from capabilities.gm_levels)
  │ GM level [0 ▾]              [Create]   │
  └────────────────────────────────────────┘
  ┌ Accounts on this server ── [Refresh] ──┐
  │ name        level          characters  │
  │ dad         3 Administrator  Thrall(80)│
  │ kiddo       0 Player         Bob(12)   │
  └────────────────────────────────────────┘
  ┌ Selected: dad ─────────────────────────┐
  │ New password [      ] [      ] [Set password]
  │ GM level [3 Administrator ▾]  [Set GM level]
  │ ✓ dad: password changed — takes effect now       ← OutcomeLabel
  └────────────────────────────────────────┘
```

| Control | Press | Mechanism the surface needs | Down / starting / shutting down |
|---|---|---|---|
| Refresh (list) | one | `accounts_list()`: a database read — AC `acore_auth.account ⋈ account_access ⋈ acore_characters.characters` (`pages.rs:296-306` is the shape), TBC/Vanilla `realmd.account.gmlevel` (`realmd.sql:46`), Tortoise `tw_logon.account.rank` (`create_databases.sql:2093`) — humans only, the app's own account hidden | works whenever `status.db` is true; else the list says "Start the server (its database) to list accounts" |
| Set password | one (the two fields must match; the button is disabled until they do) | `account_set_password`: AC `account set password <a> <pw> <pw>` — **three arguments** (`cs_account.cpp:59,:1044-1055`) over the channel, or the SRP6 row `UPDATE` (the math is `accounts.py:85-94`); TBC/Vanilla `.account set password` is `SEC_CONSOLE` and SOAP runs at `SEC_CONSOLE` (`MaNGOSsoap.cpp:113`), or the hex `v`/`s` row; Tortoise: the console when `can_send()`, else the `sha_pass_hash` row (`AccountMgr.cpp:59`; the scheme `accounts.py` already measured, `catalog.py:931-943`) | the row route needs only the database; the command route needs `ready`. The surface does not choose — the seam returns `effect` — but it disables the button when neither is possible and says which |
| Set GM level | one | `account_set_level`: AC `account set gmlevel <a> <n> -1` (all three arguments from console, `cs_account.cpp:939-944`) or the `account_access` upsert that exists (`accounts.py:507-509`, read live by `LoginDatabase.cpp:101`); TBC/Vanilla `.account set gmlevel` (`SEC_CONSOLE`, range 0–3, `Level3.cpp:1098`, caller must outrank → SOAP at `SEC_CONSOLE` can grant 3) or `UPDATE account SET gmlevel` (read live, `AccountMgr.cpp:229`); Tortoise `.account set gmlevel` (`SEC_ADMINISTRATOR`(4), range 0–4, `Commands.cpp:293`, writes `rank`, `:321`) — the console route updates the in-memory cache, the row route does **not** (`AccountMgr.cpp:250-256`) so its `effect` is "after a restart" | as above |

**Per tree, the spin box.** From `capabilities.gm_levels`: AC/TBC/Vanilla `(0 Player, 1 Moderator, 2 Gamemaster, 3 Administrator)`; Tortoise `(0 Player, 1 Observer, 2 Moderator, 3 Developer, 4 Administrator)` (`TW Common.h:184-193`). The names are shown because the numbers mean different things per tree; the level the channel needs is 3 on AC/TBC/Vanilla (`ACSoap.cpp:103-107`, `MaNGOSsoap.h:49`) and the helper account is hidden anyway.

**Three outcomes, always.** `OutcomeLabel` under the group. A "no" on AC carries the server's own text ("Account not found"); a could-not-ask names the channel state. Neither field is ever read back into a label; the password fields are cleared on press as `create_account` does (`:1474`).

**Family cannot.** Nothing in 8.3 is undrawn on any tree: every tree has list, password and level by at least one route. Tortoise on a host with no pty and no WSL gets the row routes only, and the two `effect` sentences say so ("after a restart" for the level).

**Visible in-game effect.** Log out, log in with the new password; the character list appears. With GM level ≥ 1 on AC the in-game `.gm on` is accepted (not gated here; the proof is the login). **Boxes:** `8.3a-wotlk-yulon-ubuntu/` (vmhost 3.3.5a client logs in with the changed password; readback `SELECT gmlevel FROM account_access` on the box), `8.3b-tbc-m910q/`, `8.3b-vanilla-m910q/` (m910q's own clients), `8.3c-tortoise-m910q/` — screenshots of the list and the outcome line, the client login screen after (the `bug39-lan-press-2026-09-05/client-account-typed.jpg` shape), and the row readbacks.

### 3.4 — Characters: teleport, item mail, gold, revive, set level, rename, gear sets (8.4)

**Tab:** new, "Characters", between Accounts and Maintenance. Nothing existing fits: every action here is *on one character*, and the Console tab's free-text line is what the Lab reader called "a text window, not a searchable database" (delta row "Item database search").

```
  commands: ready                                          ← ChannelBanner (same object as Server)
  ┌ Your characters ── [Refresh] ───────────────────────┐
  │ Thrall   80  Warrior   online     ◄ selected        │
  │ Bob      12  Mage      offline                      │
  └─────────────────────────────────────────────────────┘
  ┌ Teleport ─────────────────┐ ┌ Mail an item ─────────────────────────┐
  │ Where [stormwind    ▾]    │ │ Find [thunderfury ] → results (≤ 50)  │
  │ [Teleport Thrall]         │ │ 19019 Thunderfury, Blessed…  epic 80  │
  │ ✓ Thrall teleported to…   │ │ count [1] [Mail 1 item to Thrall]     │
  └───────────────────────────┘ └───────────────────────────────────────┘
  ┌ Mail gold ────────────────┐ ┌ Revive ───────┐ ┌ Set level ──────────┐
  │ gold [100 ] [Mail gold]   │ │ [Revive Thrall]│ │ [80] [Set level]    │
  └───────────────────────────┘ └───────────────┘ └─────────────────────┘
  ┌ Rename ───────────────────┐ ┌ Gear sets ────────────────────────────┐
  │ [Ask for a new name at    │ │ [Save Thrall's equipped gear as…]      │
  │  next login]              │ │ sets: [Raid set ▾] [Mail set (2 mails)]│
  └───────────────────────────┘ └───────────────────────────────────────┘
```

Every action is **one press**: none destroys or overwrites data (teleport and level are
reversible by the same buttons; mail is additive; rename only flags `at_login`). The two-press
idiom stays reserved for Remove, Repair, Uninstall and the 8.1 restart. Each button names the
selected character in its label so a press on the wrong row is visible before it lands.

| Group | Mechanism the surface needs | Offline character? | Tree cannot → what is drawn instead |
|---|---|---|---|
| Teleport | `teleports(search)` = a read of `game_tele.name` (AC `game_tele.sql:23-32`; TBC `mangos.sql:1983`; VAN `:1875`; TW `tw_world_game_tele.sql:26`), ≤ 500 rows (`pages.rs:86`), filtered as typed; `gm_teleport(char, name)` = `teleport name <char> <tele>` (AC GAMEMASTER(2) `Console::Yes`, `cs_tele.cpp:46`; TBC `.tele name` `SEC_MODERATOR` console true, `Chat.cpp:829`; VAN `:810`; TW `SEC_DEVELOPER` console true, `Chat.cpp:716`) | yes on all four — the position is written to the database (`cs_tele.cpp:146-156`; TBC `Level1.cpp:1544-1552`; VAN `:1478-1487`; TW `Commands.cpp:8033-8042`) | drawn on all four. Map coordinates are **not** offered (owner Q7; every `.go` is console-blocked on every tree) — no field, no note; the group is named "Teleport" and offers places |
| Mail an item | `items(search)` = a read of `item_template` (AC columns `item_template.sql:24-40`; TBC `mangos.sql:2962`; VAN `:2658`; TW `tw_world_item_template.sql:26-32`, `display_id` not `displayid`); `gm_mail_item(char, entry, count)` = `send items <char> "Yu'lon" "Sent from the launcher" <id>:<count>` (AC `cs_send.cpp:40,:54`; TBC `Level3.cpp:6557`; VAN `:6416`; TW `Commands.cpp:5521`), quotes and CR/LF stripped by the seam (AC #2695, `soap_cmds.rs:249-252`) | yes on all four | **one item per press on every tree**, so the 12-vs-1 cap (AC `Mail.h:33`, TBC `Mail.h:49` = 12; VAN `Mail.h:49` = 1, TW `Mail/Mail.h:51` = 1) never reaches this button. The `count` spin is a stack count, capped by the seam |
| Mail gold | `gm_mail_gold(char, gold)` = `send money <char> "Yu'lon" "…" <copper>` (AC `cs_send.cpp:43`; TBC `Chat.cpp:737`; VAN `:727`; TW `:666`) | yes | drawn on all four; the cap is `capabilities`-data (WotLK 214748 gold per `soap_cmds.rs:186`; the other three are measured at their gate, not inherited) |
| Revive | `gm_revive(char)` = `revive <char>` (AC `cs_misc.cpp:115`, offline `Player::OfflineResurrect` `:1217-1221`; TBC `Level3.cpp:3407,:3419-3421`; VAN `:3281,:3293-3295`; TW `Commands.cpp:3024,:3037-3043`) | yes | drawn on all four |
| Set level | `gm_set_level(char, n)` = `character level <char> <n>` (AC `cs_character.cpp:74,:447`, clamped `:452-453`; TBC `Level3.cpp:3934,:3961`; VAN `:3795`); spin max = `capabilities.max_level` | yes | **Tortoise: not drawn.** In its place: "Tortoise's server has no set-level command that can be sent from outside the game (`.levelup` only works in-game)." (`TW Chat.cpp:151-165`, `:923`) |
| Rename | `gm_rename(char)` = `character rename <char>` (AC `cs_character.cpp:75,:332`; TBC `Level2.cpp:3627`; VAN `:3609`); TW top-level `rename <char>` (`Chat.cpp:850`); `effect` = "at the next login" | yes | drawn on all four (the seam spells the command per tree) |
| Gear sets | `gear_read(char)` = the `character_inventory ⋈ item_instance ⋈ item_template`, bag 0, slot < 19 join (`azerothcore.md` D; `Player.h:681`); sets are app state under `config_dir()/gearsets/<game>.json`; `gear_mail(char, set)` = N `send items` calls, ≤ `mail_items_per_message` ids each; the button label carries the count: "Mail set (2 mails)" on AC/TBC, "Mail set (19 mails)" on Vanilla/Tortoise | yes | **"Save … as" is drawn only where `capabilities.gear_read` is true — WotLK today.** CMaNGOS and Tortoise inventory schemas are UNVERIFIED (delta row "Gear-set presets"); the group shows "Saving a set from this server's inventory is not built yet; a set saved from a WoW WotLK character cannot be mailed here (different item ids)." Mailing is per game id |

**Threads.** Each press → `self._run(...)` with the result into that group's `OutcomeLabel`; the gear-set mail is one job with a `LineRelay` line per mail ("mail 3 of 19: sent"), the import's shape (`:1221-1227`). The character list and both searches are database reads on the runner; typing in a search box starts a job only on Enter or after a 400 ms `QTimer` pause, never per keystroke, so a slow `docker exec` cannot stack twelve reads.

**Down / starting / shutting down.** The picker and both searches work whenever `status.db` is true (reads). Every action button follows the `ChannelState` table in §3.1; on a Tortoise tab, `_console_available()`. While the world is **starting**, a press is impossible — the button is disabled — which is the whole reason for the state: on the attach transport a command sent to a loading worldserver returns `prompted=False` (`console.py:98-102`) and on SOAP it connects and blocks (`ACSoap.cpp:125-130`). While **shutting down**, AC's fault "Command aborted: the server is shutting down" (`:128`) is rendered as could-not-ask if a press slips in before the poll notices.

**Visible in-game effect, per group.** Teleport: the character loads into the place (online) or stands there at next login (offline). Mail: the mailbox envelope; the mail is from the character's own name (from a console the sender is the recipient, `cs_send.cpp:117-118`) with subject "Yu'lon". Gold: the same, with money attached. Revive: the corpse-run ends. Level: the level in the character frame. Rename: the name prompt at login. Gear set: N mails. **Boxes:** `8.4a-wotlk-yulon-ubuntu/` with the vmhost 3.3.5a client (LAN step as in `bug39-lan-press-2026-09-05/`): one screenshot per group *in the client* (mailbox open, the loading screen's destination, the level) beside the tab's outcome line and the row readback (`SELECT position_x, map FROM characters WHERE name=…`, `SELECT COUNT(*) FROM mail WHERE receiver=…`) typed on the box. `8.4b-tbc-m910q/`, `8.4b-vanilla-m910q/` with m910q's 2.4.3/1.12.1 clients; `8.4c-tortoise-m910q/` with the Tortoise client, and a screenshot showing the set-level reason label where the group is not.

### 3.5 — Browse Bots (8.5)

**Tab:** new, "Bots", after Characters. Two groups: Browse (all four) and My Party (8.6).

```
  ┌ Browse bots ── name [        ] class [any ▾] online [any ▾] [Refresh] ─────────┐
  │ 498 bots (of 500 characters marked as bots) · page 1 of 10 · marker: rndbot… │
  │ name        class    race    level  online  zone                             │
  │ Aelthar     Mage     Gnome   61     yes     Hellfire Peninsula               │
  │ …                                                                            │
  │ [◀ prev] [next ▶]                                                            │
  └──────────────────────────────────────────────────────────────────────────────┘
```

| Tree | Columns | The marker (the seam's clause; the header names it) |
|---|---|---|
| WotLK | name, class, race, level, online, zone | `acore_playerbots.playerbots_account_type.account_type IN (1,2,3)` **or** `acore_auth.account.username LIKE '<prefix>%'` — both signals, because the registry can be empty with 1000 bots playing (`botid.rs:4-17`) and type 3 is a city-bot citizen (`:96-99`); prefix from `AiPlayerbot.RandomBotAccountPrefix` in the installed conf (`playerbots.conf.dist:106`) |
| TBC, Vanilla | name, class, race, level, online (no zone column until the `characters.zone` column is read at the gate) | `realmd.account.username LIKE '<prefix>%'` — the only durable marker (`cmangos.md` TBC E; `PlayerbotAIConfig.cpp:939`); prefix read from the installed `aiplayerbot.conf` (`RNDBOT` in the dist, `rndbot` in code) — **never assumed** |
| Tortoise | name, class, race, level, online | `tw_logon.account.username LIKE '<prefix>%'` (`aiplayerbot.conf.dist.in:63`; `PlayerbotAIConfig.cpp:545`) |

**Three outcomes.** Rows; **zero bots while `characters` has rows** → the header line turns into a warning: "0 bots but N characters — the bot marker may not match this server; the app looked for accounts named `<prefix>…`" (the fail-open lesson, `botid.rs:4-17`, rendered instead of an empty table); could-not-ask → "Start the server (its database) to browse bots". Paging is 50 rows, clamped by the seam (`pages.rs:143-145`). Down/starting/shutting-down: a pure read — the table works whenever `status.db` is true and says so otherwise; nothing here touches the world thread. `.rndbot list` is **not** used: its output never reaches the attach console (fact 5) and on AC never reaches anyone (fact 6).

**Visible in-game effect.** A row's name typed into `/who <name>` in the client finds the bot; the online column agrees with `/who`. **Boxes:** `8.5a-wotlk-yulon-ubuntu/`, `8.5b-tbc-m910q/`, `8.5b-vanilla-m910q/`, `8.5c-tortoise-m910q/` (Tortoise is already running there with 500 bots) — screenshot of the table, a client screenshot of `/who` for one row, and `SELECT COUNT(*) …` with the same clause typed on the box as the independent count.

### 3.6 — My Party (8.6, WotLK only)

**Tab:** Bots, second group. Drawn only when `capabilities.party` is true; on the other three the group is replaced by: "Building a bot party from the launcher works on WoW WotLK only (owner decision, 2026-09-06)." — and nothing else, because the CMaNGOS route (`.rndbot spoof`/`add`/`p`) is "plausible from source and unproven end to end" (delta row) and the owner set the scope (Q4).

```
  ┌ My Party (WoW WotLK) ─────────────────────────────────────────────────────┐
  │ bridge: ready            commands: ready         Thrall: online          │
  │ For [Thrall ▾]  add a [Mage ▾]  spec [frost pve ▾]  level [80]  [Add bot]│
  │ ▸ adding a mage for Thrall…                                              │
  │ ▸ Xyzzy joined the party (4.1 s)                                         │
  │ ▸ Xyzzy set to level 80                                                  │
  │ ▸ Xyzzy: talents frost pve                                               │
  │ ▸ Xyzzy: gearing                                                         │
  │ ✓ Xyzzy is in Thrall's party                                             │
  │ in the party now: Xyzzy (Mage)  [Kick Xyzzy]                             │
  └───────────────────────────────────────────────────────────────────────────┘
```

**What the user picks.** The character (from `characters_list`, humans, must be **online** — the button is disabled with "Thrall is not logged in" otherwise, because the bridge's `dml_addclass` resolves a live player, `party.rs:264`); the class (the WotLK nine without Death Knight, `party_specs.rs:18-31`); the spec, from the server's own premade names `AiPlayerbot.PremadeSpecName.<class>.<n>` read out of the installed `playerbots.conf` (`playerbots.conf.dist:1764-1780`; `party_specs.rs:39-52`); the level (1–80), applied after the bot joins by the stock `character level <bot> <n>` (`cs_character.cpp:74`, works on a connected player `:447`) — a characters-database change through the server's own command, which Q7 allows.

**What they must already be doing.** Logged in on that character, standing anywhere; the bot logs in on its own and joins. Nothing is written to the client (Q5).

**The route, and the three prerequisites the group shows as three banners.** (1) `bridge: ready` — a probe over the channel of one bridge command whose only job is to answer (the seam names it; "Command does not exist" is the missing-bridge answer the Rust launcher recorded live, `bridge.rs:189-195`); (2) `commands: ready` (8.1); (3) the character online. The route is the mod-ale Lua bridge over the channel — `dml_addclass <player> <class>`, then a poll of `group_member` for the new guid (12 × 500 ms, `party.rs:321-333`), then `character level`, then `dml_whisper <player> <bot> talents spec <name>`, then `dml_whisper … autogear` (`party.rs:175-250`); the six scripts are this repository's own code from `rust-main:cli/lua/party/*.lua`, ported into the app's resources and deployed into `<server_dir>/env/dist/etc/modules/lua_scripts/` (a server-folder write, which Yu'lon owns). The gearing and spec commands are bot chat actions reachable only from a live `Player&` (`PlayerbotAI.cpp:600`), which is exactly what `dml_whisper` supplies from inside the world.

**When the bridge is missing.** The group's first banner reads one of: "bridge: the Lua engine module is not installed — install *AzerothCore Lua Engine (ALE)* on the Modules tab, then Start the server" (mod-ale is a manifest, `manifests/wow-wotlk/modules/mod-ale.json`, `build.rebuild: true`); "bridge: not set up — press Set up the party bridge" with that **one-press** button, which deploys the scripts and patches `mod_ale.conf` (`ALE.Enabled = true`, an absolute `ALE.ScriptPath` — the manifest already forces the path, `mod-ale.json:17-19,:29-35`; the compiled default of `ALE.Enabled` is `false` while the dist comment says `true`, `ALEConfig.cpp:20`, so the setup writes the key rather than trusting the file) and then says "restart the world server for the bridge to load (Server tab)"; or "bridge: could not ask — <reason>". There is no "Enable My Party" that can report success for a no-op (`bridge.rs:60-65`): the outcome is the probe's answer after the restart, never the deploy's.

**Three outcomes of Add bot.** Yes: the ✓ line and the member list refreshed from `group_member` (`party.rs:267-268` is the query shape). No: the bridge's own text ("You are not allowed to control bot …", `PlayerbotMgr.cpp:112-116`; the `MaxAddedBots` cap 40, `:131-136`) or "no new party member appeared within 6 s — the bot may still be logging in; press Refresh" (a timeout is reported as *no*, with its cause, not as success). Could-not-ask: the channel state. Kick: `dml_uninvite <bot>` then `dml_whisper … logout` (`party.rs:186,:195`), one press.

**Threads.** One job per Add, with a `LineRelay` for the ▸ lines; `_party_pending` blocks a second Add until done; a finished job triggers the Browse refresh (§2.4).

**Down / starting / shutting down.** All three banners go grey and the Add button is disabled; the state sentence is the channel's.

**Visible in-game effect.** The bot in the party frame, at the chosen level, wearing gear. **Box:** `yulon-ubuntu` WotLK with the vmhost 3.3.5a client (LAN step) → `pyplan/gates/8.6-wotlk-yulon-ubuntu/`: a client screenshot of the party frame with the named bot, the tab's ▸ transcript, `SELECT memberGuid FROM group_member …` and `SELECT level FROM characters WHERE name=<bot>` typed on the box, and the worldserver log line showing the bridge command was executed. The mod-ale install + rebuild it needs is a rebuild — **asked for, never started** (memory: ask before any rebuild); the gate line says so.

### 3.7 — Module update checks; manifests for TBC, Vanilla, Tortoise (8.7)

**Tab:** Modules. **Added after the button row at `:1740`:** a "Check for updates" button (one press) and an "Update selected" button; list items gain a suffix "— update available (N commits behind)" when `modules_check_updates()` says so (`modules.refresh()` + the `manifest_store` ETag machinery exist and have no UI caller, `controller_wow_wotlk/modules.py:65`; commits-behind = `git fetch` in `ContainerGit`, the Rust shape `maint.rs:438`).

| Change | What it closes |
|---|---|
| Install/Remove/Update enabled only when a manifest is selected (`currentItemChanged` → `_module_selection_changed`) | bug-checklist line 394 (both buttons enabled with nothing selected; `_module_action()` returns silently at `:1780-1781`) — the silent return stays as belt and braces, the buttons no longer invite it |
| Install/Update disabled while `status.world` is true for a manifest whose `sql` steps target `characters` or `world` (`manifest.py:119`, `apply.py:60`), with the report line "Stop the server first: this module changes the world database" | owner Q7 on the one existing path that writes those databases while running (delta fact 9) |
| The store per game: `manifests/wow-tbc/`, `wow-vanilla/`, `wow-tortoise/` with their own indexes and a `controller_wow_<game>/modules.py` binder; `has_manifests` set on the entries (`catalog.py:1013`); the `_no_manifest_store()` warning (`:274-288`) becomes unreachable for them | the CMaNGOS Modules tab, which today says "(this game has no manifests yet)" (`:1755`) |

Three outcomes of Check for updates: a list of what is behind; "every installed module is current"; could-not-ask ("GitHub could not be reached — <reason>", or "this module's clone is not one the app made", `apply.py:965` `_require_own_clone`). A CMaNGOS manifest is flat `.conf` keys under `etc/` (`lab.md` F5; `cmangos.md` Step 1) — the applier's `conf` primitive already writes those (`apply.py:1388-1410`); what a first CMaNGOS manifest set contains is the seam designer's and the owner's (§11).

**Visible effect.** After Update + rebuild + Start, the module's own in-game effect (the manifest's `npcs` note, e.g. transmog NPC 190010, `lab.md` F5). **Boxes:** `8.7a-wotlk-yulon-ubuntu/` (a module deliberately pinned one commit behind, the badge, the update, the rebuild — **asked for**); `8.7b-<game>-m910q/` for each CMaNGOS entry with at least one conf-only manifest applied and its key read back from the installed `.conf` on the box.

### 3.8 — Steam (8.8, Linux / Steam Deck only)

**Tab:** Server, a "Steam" group after the Uninstall row — **built only on Linux** (`platform.is_linux()`-shaped predicate at build time; `is_steamos()` at `platform.py:132` is the existing sibling). On Windows and macOS **nothing is drawn**: the owner scoped the feature to Linux/Steam Deck (Q2d), so it is not a host limitation to explain but a thing the product does not offer there; the capability table in `pylauncher/README.md` says "no — Linux/Steam Deck only" and §7's test asserts the absence. The Console pattern (drawn, disabled, reason) is for a thing the platform *should* do and cannot yet; this is not that.

```
  ┌ Steam ──────────────────────────────────────────────────────────────┐
  │ Steam: found at ~/.local/share/Steam (close Steam before adding)   │
  │ [Add this server and the WoW client to Steam]                      │
  │ ✓ added "WoW WotLK server" and "World of Warcraft" to the library; │
  │   open Steam to see them                                            │
  └─────────────────────────────────────────────────────────────────────┘
```

| Press | Mechanism the surface needs (UNVERIFIED — delta row "Steam integration") | Outcomes |
|---|---|---|
| one | `steam.probe()`: the userdata folder and whether Steam is running; `steam.add_shortcuts(client_exe)`: two Non-Steam entries — the server (the surviving `setup-gaming-mode.sh` with its three arguments, copied **out of the AppImage mount first** as its header demands, `setup-gaming-mode.sh:56-62`) and the client `Wow.exe` from `KnownInstall.client_dir` (`state.py:31`) with the compat tool the guide names (`WoW-WotLK-HOWTO.md:225`, GE-Proton) | yes: both entries named; no: "Steam is running — close it and press again" or "the client folder is not recorded for this install" (the button is disabled with that sentence when `client_dir` is `None`); could-not-ask: "Steam was not found (looked in ~/.steam/steam and ~/.local/share/Steam)" — button disabled, sentence shown |

**No spike, no build.** `shortcuts.vdf`'s format and the compat-tool assignment are recorded nowhere in this repository (delta "Could not ask" 6) and no test box has Steam. The surface above is what the button shows; the mechanism is owed a read of a real Steam userdata folder before the box is written. §11 asks the owner which machine.

**Visible effect.** The two entries in the Steam library; launching the first starts the stack and prints the client instructions; the second launches the client under Proton. **Box:** the owner's Steam Deck or a Linux box with Steam (none this side) → `pyplan/gates/8.8-steam-<box>/`: a photo/screenshot of the library, `cat shortcuts.vdf | strings` from the box, and the server tab reading "world up" after the shortcut is launched from Gaming Mode.

### 3.9 — Uninstall (8.9)

**Tab:** Server, "Uninstall…" after Repair in the button row (`:780-788`). It opens the dialog `phase8-decisions.md:162-176` designed — the one place on this tab a modal is right, because the decisions page chose the typed confirmation and a checkbox, and both need a dialog. The size on the dialog comes from `uninstall_plan()` run **first** on the job runner (the dialog opens reading "measuring…" and fills in; a plan that cannot prove ownership disables Uninstall in the dialog with the refusal text — `phase8-decisions.md:127-133`).

| Press | What happens | Outcomes |
|---|---|---|
| "Uninstall…" (one press, opens the dialog) | plan on the runner; dialog on the GUI thread when the plan arrives | the dialog, or "Could not measure this install: <reason>" in `problem_label` |
| dialog "Uninstall" (the typed server name is the confirmation) | `_set_busy(True)`; the seam: log snapshot (8.2) → stop with the 300 s grace (`docker.py:938`) → remove containers/images/volumes per the checkbox → folder (Windows read-only bit; WSL-resident path via `wsl.exe`) → `progress` lines through a `LineRelay` into `problem_label`; on yes the view emits `uninstalled(game, server_dir)` and the window forgets the record last and drops the tab (`main.py:189-214`) | yes: "Uninstalled. Backups under <backups dir> were kept." then the tab closes; no: the refusal, the record kept, the tab stays ("the recoverable failure", `phase8-decisions.md:154-158`); could-not-ask: Docker unreachable — nothing touched, said so |

While the server is **up** the dialog says "This stops the server first (up to 5 minutes)". `busy_reason()` (`:798-818`) gains the uninstall: the window will not close mid-uninstall. Cancel: refused once past the stop, as the decisions page's open item 4 suggests; the dialog says so before the press.

**Visible effect.** The tab gone; the Catalog tile reads "Install" again; on the box, `docker ps -a`, `docker volume ls` and `ls <server_dir>` answer nothing of it — or the volume alone when the box was ticked, and a reinstall to the same folder finds the characters (`phase8-decisions.md:205-207`). **Boxes:** `8.9a-wotlk-yulon-ubuntu/` and `8.9b-vanilla-m910q/` (one CMaNGOS game, per the decisions page): both runs (ticked → reinstall → the character on the client's list; unticked → nothing of the project on the box), the shell readbacks, and the tile screenshot.

---

## 4. Delivery order and gates

Every gate is a press through the real widget on a real box, announced on `claude-say`, with the capture under `pyplan/gates/8.x…/` named in the row; never a CLI. Linux first (Q3's "WotLK first" + the standing Linux-first rule); the Windows press is one extra box on the WotLK row where the mechanism differs on Windows (the attach console).

| Step | What lands | Gate (box → capture) |
|---|---|---|
| **8.1a** | `Outcome`/`ChannelState`, `OutcomeLabel`/`ChannelBanner`, the Server-tab banner + set-up button (two-press when up, one when down), `_set_busy` widened, the poll's 15 s probe; `.yulon-commands.json`; WotLK wiring | `yulon-ubuntu` WotLK from Off: press with the world down → Start → banner `ready`; Stop → `down`; Start → `starting` seen on screen before `ready`; Stop pressed and `shutting-down` seen. Shell readbacks of the account rows and one `curl` round-trip → `8.1a-wotlk-yulon-ubuntu/`. Then `yulon-win11` WotLK: the same presses → `8.1a-wotlk-yulon-win11/` |
| **8.1b** | TBC and Vanilla wiring (conf patch + restart) | `m910q`, one game at a time, Tortoise stopped and restored (announced) → `8.1b-tbc-m910q/`, `8.1b-vanilla-m910q/` |
| **8.1c** | Tortoise: `channel = "console"`, no set-up button, `prompted=False` → could-not-ask | `m910q` Tortoise running → `8.1c-tortoise-m910q/`: the console banner, a Characters-tab press answering within ~4 s |
| **8.2** | dashboard line, `refresh_status()` at construction, restart-loop rendering, log snapshot on Stop/Remove, "Save the worldserver log" | all four boxes as §3.2; the crash-loop press on `m910q` TBC → `8.2-*` |
| **8.3a / 8.3b / 8.3c** | Accounts list, set password, set GM level; the per-tree level names | WotLK on `yulon-ubuntu` + vmhost client login with the new password; TBC/Vanilla/Tortoise on `m910q` with its clients → `8.3*` |
| **8.4a / 8.4b / 8.4c** | the Characters tab; gear sets on WotLK; Tortoise without set-level | one client screenshot per group per game, the row readbacks → `8.4*` |
| **8.5a / 8.5b / 8.5c** | the Bots tab's Browse group, per-tree marker | four boxes, `/who` in the client, the independent count → `8.5*` |
| **8.6** | My Party: the three banners, bridge setup, Add/Kick, the ported Lua scripts as resources | `yulon-ubuntu` WotLK: mod-ale installed via the Modules tab, the rebuild **asked for**, bridge set up, Start, a bot in the party frame on the vmhost client → `8.6-wotlk-yulon-ubuntu/` |
| **8.7a / 8.7b** | update checks + selection-gated buttons + the Q7 gate; CMaNGOS manifest sets | WotLK on `yulon-ubuntu` (rebuild asked for); one conf-only manifest per CMaNGOS game on `m910q` → `8.7*` |
| **8.8** | the Steam group on Linux; absence elsewhere | a box with Steam (owner's — §11) → `8.8-steam-<box>/` |
| **8.9a / 8.9b** | Uninstall…, the dialog, `uninstalled` signal, `state.forget()` last, the tile reset | `yulon-ubuntu` WotLK both runs; `m910q` Vanilla both runs → `8.9*` |

8.1a → 8.2 → 8.3a → 8.4a → 8.5a → 8.6 → 8.9a is the WotLK spine and is sequential (8.2 before
8.3 because every later tab's disabled-state sentence is the banner 8.1a/8.2 make true); the
`b`/`c` rows each follow their `a`; 8.7 and 8.8 are independent of everything but 8.1a. The
Phase 7.9/7.10 re-runs are in §6.

---

## 5. Proposed `8.x` checklist lines

Top-level, never nested; the ids match `test_docs_pins.py`'s `^- \[x\] (\d+\.\d+[a-z]?) ` shape (`tests/test_docs_pins.py:55`).

- [ ] 8.1a **Server commands, WoW WotLK** — the Server tab shows a "commands:" line in all seven states, "Set up server commands" (one press with the world down, two presses when up), SOAP on `127.0.0.1` enabled in the installed config, the app's helper account created by the SRP6 row and verified by a real round-trip before its credentials are written to `<server_dir>/.yulon-commands.json`. Linux and native Windows. **DoD (a person at the window):** after the press the banner reads "commands: ready"; a Stop turns it to "shutting down" and then "stopped"; a Start shows "waiting for the world to finish loading" before "ready"; the Accounts list does not show the helper account. **Gate:** `yulon-ubuntu` then `yulon-win11`, `pyplan/gates/8.1a-wotlk-yulon-ubuntu/` and `8.1a-wotlk-yulon-win11/`: banner screenshots per state, the `problem_label` transcript, `SELECT username FROM acore_auth.account` / `SELECT gmlevel FROM account_access` typed on the box, one `curl` `server info` with the file's credentials. **Visible effect:** the banner and the Characters tab's buttons going live.
- [ ] 8.1b **Server commands, WoW TBC and WoW Vanilla** — as 8.1a with `SOAP.Enabled = 1` patched into `mangosd.conf` and the level in `realmd.account.gmlevel`. Linux. **DoD:** as 8.1a per game. **Gate:** `m910q`, one game at a time, `8.1b-tbc-m910q/`, `8.1b-vanilla-m910q/`.
- [ ] 8.1c **Server commands, WoW Tortoise (console route)** — no set-up button is built; the banner reads "commands: through the console" where `can_send()` and `NO_TTY_HELP` where not; a stock command from the Characters tab answers through `send_command()` and an unprompted window is shown as could-not-ask. Linux. **DoD:** the banner text; no set-up button anywhere on the tab; a Revive from the Characters tab answers within 5 s. **Gate:** `m910q`, `8.1c-tortoise-m910q/`. **Visible effect:** the revived character.
- [ ] 8.2 **Dashboard and the worldserver log snapshot** — the Server tab's status line gains players, bots, container-up age and restart count from database reads and `docker inspect`; a crash-looping world reads "restarting (N restarts)", never "up"; the first status is asked at construction (bug-checklist line 552 closed; line 499 closed); Stop, Remove and Uninstall save the last 2000 lines of the worldserver log to `config_dir()/logs/` and say where; the Console tab's "Save the worldserver log" does the same on demand. All four, Linux. **DoD:** the numbers on the tab equal `SELECT COUNT(*) FROM characters WHERE online=1` split by the bot clause and `docker inspect --format '{{.RestartCount}}'` typed on the box; the named log file exists and ends with the line the Console tab was showing. **Gate:** `8.2-wotlk-yulon-ubuntu/`, `8.2-tbc-m910q/` (including the forced restart loop), `8.2-vanilla-m910q/`, `8.2-tortoise-m910q/`.
- [ ] 8.3a **Accounts: list, set password, set GM level — WoW WotLK** — the Accounts tab lists human accounts with level name and characters; Set password (matching fields) and Set GM level (0–3 by name) on the selected account render yes / no / could-not-ask with an effect sentence (bug-checklist line 239 closed). Linux and native Windows. **DoD:** the list shows the accounts `SELECT username FROM account` shows minus the helper; after Set password the vmhost client logs in with the new password; after Set GM level the row readback matches. **Gate:** `8.3a-wotlk-yulon-ubuntu/`, `8.3a-wotlk-yulon-win11/`. **Visible effect:** the client's character list after login.
- [ ] 8.3b **Accounts — WoW TBC and WoW Vanilla** — as 8.3a against `realmd.account`. **Gate:** `8.3b-tbc-m910q/`, `8.3b-vanilla-m910q/`, the m910q clients logging in.
- [ ] 8.3c **Accounts — WoW Tortoise** — levels 0–4 by Tortoise's names; the level set through the console takes effect now, through the row "after a restart", and the tab says which. **Gate:** `8.3c-tortoise-m910q/`.
- [ ] 8.4a **Characters tab — WoW WotLK** — teleport (named places), mail one item, mail gold, revive, set level, rename, gear sets (save + "Mail set (N mails)"); every button one press, named for the selected character, all three outcomes on screen; every action works on an offline character. Linux and native Windows. **DoD:** each group's in-game effect seen on the vmhost client (loading screen / mailbox / level / rename prompt), the rows read back on the box. **Gate:** `8.4a-wotlk-yulon-ubuntu/`, `8.4a-wotlk-yulon-win11/`.
- [ ] 8.4b **Characters tab — WoW TBC and WoW Vanilla** — as 8.4a; Vanilla's gear-set mailing not drawn (no inventory read yet) with its reason. **Gate:** `8.4b-tbc-m910q/`, `8.4b-vanilla-m910q/`.
- [ ] 8.4c **Characters tab — WoW Tortoise** — as 8.4a through the console; set level not drawn, its reason in its place; `rename` as the top-level command. **Gate:** `8.4c-tortoise-m910q/` including a screenshot of the reason label.
- [ ] 8.5a **Browse bots — WoW WotLK** — the Bots tab lists bots (name, class, race, level, online, zone) by the two-signal marker, filtered and paged; zero bots with characters present is a warning, not an empty table. **DoD:** the table's count equals the same clause typed on the box; `/who <name>` in the client finds a listed bot. **Gate:** `8.5a-wotlk-yulon-ubuntu/`.
- [ ] 8.5b **Browse bots — WoW TBC and WoW Vanilla** — by the account-prefix marker read from the installed conf. **Gate:** `8.5b-tbc-m910q/`, `8.5b-vanilla-m910q/`.
- [ ] 8.5c **Browse bots — WoW Tortoise** — as 8.5b against `tw_logon`/`tw_char`. **Gate:** `8.5c-tortoise-m910q/`.
- [ ] 8.6 **My Party — WoW WotLK only, server-side route** — the Bots tab's party group with three banners (bridge, commands, character online), "Set up the party bridge" (deploys the ported Lua scripts and patches `mod_ale.conf`, then asks for a restart), Add bot (class, premade spec, level) with a ▸ progress transcript, Kick; the group is replaced by a one-line reason on the other three games. Linux. **DoD:** with mod-ale installed from the Modules tab (rebuild asked for) and the bridge set up, Add bot puts a named bot of the chosen class, spec and level into the online character's party frame on the vmhost client within 30 s; a missing bridge reads "bridge: not set up" and never "ready". **Gate:** `8.6-wotlk-yulon-ubuntu/`: party-frame screenshot, the transcript, `group_member` and `characters.level` read on the box, the worldserver log line of the bridge command.
- [ ] 8.7a **Module update checks — WoW WotLK** — "Check for updates" marks installed modules behind their remote; "Update selected"; Install/Remove/Update enabled only with a selection (bug-checklist line 394 closed) and disabled while the world runs for a manifest that writes `characters`/`world` (owner Q7). **DoD:** a module pinned one commit behind shows the badge, updates, and the report asks for the rebuild. **Gate:** `8.7a-wotlk-yulon-ubuntu/`.
- [ ] 8.7b **Module manifests for WoW TBC, WoW Vanilla, WoW Tortoise** — a manifest set and `modules.py` binder per game; the Modules tab lists and applies at least one conf-only manifest per game. **DoD:** the key read back from the installed `.conf` on the box after Install. **Gate:** `8.7b-tbc-m910q/`, `8.7b-vanilla-m910q/`, `8.7b-tortoise-m910q/`.
- [ ] 8.8 **Steam — Linux / Steam Deck only** — the Server tab's Steam group (absent on Windows/macOS) adds the server launcher script (copied out of the AppImage mount) and the client as two Non-Steam entries with the compat tool; disabled with the reason when Steam is not found, the client folder is not recorded, or Steam is running. **DoD:** both entries in the Steam library on a real Steam install; launching the first brings "world up" on the tab. **Gate:** the owner's box (§11), `8.8-steam-<box>/`. **Prerequisite:** a read of a real `shortcuts.vdf` before the seam is written.
- [ ] 8.9a **Uninstall — WoW WotLK** — "Uninstall…" opens the decided dialog (size, "Keep my characters", typed name); the run snapshots the log, stops, removes the project and folder, forgets the record last, closes the tab and resets the tile; ownership that cannot be proved is refused in the dialog. Linux and native Windows (the read-only bit). **DoD:** ticked → reinstall to the same folder → the character on the client's list; unticked → `docker ps -a`, `docker volume ls`, `ls` on the box show nothing of it. **Gate:** `8.9a-wotlk-yulon-ubuntu/`, `8.9a-wotlk-yulon-win11/`.
- [ ] 8.9b **Uninstall — one CMaNGOS game (WoW Vanilla)** — as 8.9a. **Gate:** `8.9b-vanilla-m910q/`.
- [ ] **Phase 8 exit criteria met** — every `8.x` box above ticked by a press on its box with its capture; the Phase 7.9 and 7.10 re-runs in §6 green; `pylauncher/README.md`'s capability table carries a "run live" per new row and platform; nothing reachable only from a CLI.

---

## 6. Blast radius on the proven controller paths

| Existing surface | Changes | Does not change |
|---|---|---|
| Server tab (`:736-796`) | new widgets added after `repair_label`; `_set_busy()` locks more buttons; the status words read `settled`/`restart_count`; `refresh_status()` once at construction; Stop/Remove gain the snapshot line | Start, Stop, Refresh, Remove, Repair, Stop-the-other: their slots, labels and two-press idiom (`:1012-1270`) |
| Console tab (`:1274-1310`) | one button added after Follow | Send, `_console_pending`, `NO_TTY_HELP`, `_parse_reply` — the attach transport is untouched and remains the Console tab's |
| Accounts tab (`:1401-1489`) | groups added after Create; the spin range from data | `create_account()` and its slots |
| Maintenance tab | none | all |
| Modules tab (`:1726-1797`) | selection gating; two buttons; the Q7 gate; stores for three more games | `reload_modules()`, `_module_action()` |
| Networking tab | none | all |
| `ControllerServices` (`:96-120`) and the four factories (`:334-565`) | new fields, one line each per factory; `_assemble()` gains the family-neutral ones | every existing field and lambda, including the distro plumbing the tests pin (`test_for_wotlk_wires_the_distro_into_every_seam_that_talks_to_docker`, `tests/test_controller_view.py:1240`) |
| `main.py` (`:216-290`) | one `connect` after `:277`; `on_uninstalled` | `drop_controller`, `add_controller`'s distro logic, `_stop_background_threads` |
| `controller.py`, `state.py`, `console.py` | none | all — the base controller learns nothing game-shaped (style-guide §3) |

**Phase 7 gates to re-run before the exit box:** 7.9 (start/stop/logs/accounts/backup on each installed server — the Server, Console, Accounts and Maintenance tabs each changed by addition; `pyplan/gates/gate-79-controller-surface.py`'s three criteria through `ControllerServices.for_entry()` on `m910q` TBC, Vanilla, Tortoise and `yulon-ubuntu` WotLK), and 7.10 (the WotLK coverage pass, `7.10-ubuntu-2026-09-05-rerun/` shape) after 8.9a. Bug §39's LAN press (`bug39-lan-press-2026-09-05/`) is not re-run: the Networking tab is untouched, and every 8.x WotLK gate on `yulon-ubuntu` that logs a client in exercises it anyway.

---

## 7. Tests

Offscreen, in `tests/test_controller_view.py`'s shape (`_Ps` fakes `docker ps`, `_inline_jobs` runs every job inline, `:41-49`; a `_FakeCommands`/`_FakeReads` in the `_FakeMaintenance` mould, `:100-142`, records what each press asked). New names; existing ones cited only where they exist:

| Test (new unless marked) | Asserts |
|---|---|
| `test_the_commands_banner_renders_all_seven_states` | one `ChannelState` per state → the exact sentence; no state falls through to another's text |
| `test_every_command_button_is_disabled_unless_commands_are_ready` | with `starting`, `down`, `shutting-down`, `could-not-ask`: every Characters/Bots/Accounts action button `isEnabled()` is False and its tab banner shows the reason |
| `test_setup_is_one_press_with_the_world_down_and_two_when_up` | `_Ps.names` empty → one press calls `channel_setup`; names up → first press arms with the restart paragraph, second calls |
| `test_setup_verifies_before_it_records` | the fake's verify raises → `Outcome("could-not-ask")` rendered, the credentials file absent, a second press calls setup again with the same account name (never a second account) |
| `test_the_probe_is_asked_at_most_once_per_three_polls_and_not_once_ready` | count of `channel_probe` calls across 9 `refresh_status()` ticks, before and after `ready` |
| `test_the_first_status_is_asked_at_construction` (bug 552) | `_Ps.calls` holds a `docker ps` before any timer tick; Start is disabled when the fake says all up |
| `test_a_restarting_world_reads_as_restarting_not_up` (bug 499) | `settled=False, restart_count=3` → status word "restarting (3 restarts)", Start disabled, banner `could-not-ask` |
| `test_stop_saves_the_log_first_and_a_failed_snapshot_never_blocks_the_stop` | call order in the fake; the snapshot's `no` line appears after the stop's line |
| `test_the_accounts_list_hides_the_helper_and_names_levels_per_tree` | the WotLK fake returns `yulon` among rows → not listed; Tortoise entry → spin box 0–4 with Tortoise's names, WotLK 0–3 |
| `test_set_password_needs_matching_fields_and_clears_them` | button disabled until equal; after press both fields empty and the fake got the value once |
| `test_a_gm_level_written_to_the_row_says_after_a_restart` | `Outcome(effect="after a restart")` → the sentence on the label |
| `test_tortoise_draws_no_set_level_and_says_why` | on the Tortoise entry no `set_level_button` attribute exists; the group's place holds the reason label; on WotLK the button exists |
| `test_every_characters_button_names_the_selected_character` | selecting a row rewrites six button labels |
| `test_mail_set_says_how_many_mails_per_tree` | 19 pieces: "(2 mails)" with `mail_items_per_message=12`, "(19 mails)" with 1 |
| `test_gear_save_is_absent_where_the_inventory_cannot_be_read` | `gear_read=False` → no save button, the reason label |
| `test_the_bots_table_warns_on_zero_bots_with_characters_present` | `BotPage(total=0, characters=500)` → the warning sentence, not an empty table |
| `test_my_party_is_absent_off_wotlk_and_present_on_it` | `party=None` → the reason label and no Add button; WotLK → three banners |
| `test_add_bot_is_disabled_while_the_character_is_offline_or_the_bridge_is_missing` | each banner alone disables it with its own sentence |
| `test_add_bot_streams_its_progress_and_refreshes_the_browse_table` | the `LineRelay` lines land on the label in order; the browse fake is called again after done |
| `test_a_party_timeout_is_reported_as_no_with_its_cause` | `Outcome("no", "no new party member within 6 s…")` rendered, never a ✓ |
| `test_modules_buttons_follow_the_selection` (bug 394) | nothing selected → all three disabled; select → enabled |
| `test_a_world_writing_module_cannot_be_installed_while_the_world_runs` | a manifest with an `sql` step into `world` + `status.world=True` → Install disabled, the report says stop first; a conf-only manifest stays enabled |
| `test_the_steam_group_is_absent_off_linux_and_disabled_without_steam` | `steam=None` → no group; `probe()` could-not-ask → button disabled with the sentence |
| `test_uninstall_signals_up_and_never_removes_its_own_tab` | after a yes the view emitted `uninstalled` once and is still in the tab widget (the window removes it) |
| `test_uninstall_is_refused_in_the_dialog_when_ownership_is_unproved` | plan with `unknown` non-empty → the dialog's Uninstall disabled with the refusal |
| `test_busy_reason_covers_the_uninstall` | `busy_reason()` non-None while the uninstall job runs |
| `test_every_action_button_is_locked_while_an_action_runs` (**exists**, `:1060`) | extended: the Phase 8 buttons are in the locked set |
| `test_removing_containers_takes_two_presses` (**exists**, `:664`), `test_refresh_cancels_an_armed_remove` (**exists**, `:696`) | unchanged, and the armed set-up button obeys the same `_disarm_actions()` |
| `test_the_console_says_why_it_is_disabled_where_there_is_no_pty` (**exists**, `:378`) | extended: on a Tortoise entry the Characters buttons are disabled by the same predicate with the same text |
| `test_every_game_in_the_catalog_can_be_opened` (**exists**, `:1873`) | still green with the new services fields |
| `tests/test_docs_pins.py` | every `path:symbol` in `phase8-parity-decisions.md` and the `8.x` lines resolves at the pinned SHA; the widening onto `phase8-plans/` when a box ticks (kickoff §6) |

**Live press gates:** §4's table. Each press is driven the way `pyplan/gates/press-driver.py` and the bug §39 gate were — through the widget, screenshot per state — and every DoD names a client-visible thing or a shell readback typed on the box, never the app's own log line alone.

---

## 8. Risks worth re-reading

- **The channel costs the world thread.** SOAP and the attach console both queue on the single world thread (delta fact 4). Prices measured so far: the attach transport ≥ 3.6 s per command (`console.py:45,:76`); SOAP's is *not measured on any Yu'lon box* — 8.1a's gate records `server info`'s wall time with 500 bots. The probe's 15 s cadence while starting is bounded; if a `server info` under load measures over 2 s, the cadence moves to 30 s and the number goes on the 8.1a line.
- **"Starting" is inferred, not announced.** SOAP connects and then waits (`ACSoap.cpp:125-130`); the seam's reply timeout decides `starting`. Too short and a slow-but-alive world reads `could-not-ask`; too long and a press waits. The 2026-09-04 incident recorded 46 minutes to the first `Avg Diff:` on 9p (`controller.py:359-371`), so the timeout is per press (a few seconds) and the *state* is re-asked, never a fixed total.
- **Two installs of one game share container names** (bug-checklist line 227; `controller.py:180-184`): the dashboard's `docker inspect ac-worldserver` and the bot counts would read the other install's server. Enumerated, not fixed here: the dashboard line carries the container name in its tooltip so the mismatch is visible.
- **The bot marker fails open** (`botid.rs:4-17`): a wrong prefix makes every bot a human on the dashboard *and* on the Accounts list. The Bots tab's zero-bots warning is the tell; the prefix is read from the installed conf, never assumed, and the gate's independent count is the proof per tree.
- **mod-ale is unpinned** (`mod-ale.json:9-11`; `azerothcore.md` E read HEAD of the same day) and its `ALE.Enabled` compiled default is `false` (`ALEConfig.cpp:20`). The bridge probe after every restart is what stands between "set up" and "silently nothing" (`bridge.rs:189-195,:60-65`); the group never reports ready from the deploy.
- **My Party's join wait is a poll** (12 × 500 ms, `party.rs:321-333`); a bot that logs in slower is reported *no* with a cause and a Refresh. Five bots are five presses; `MaxAddedBots` 40 (`playerbots.conf.dist:136`) is the server's answer, rendered as the server's text.
- **Tortoise on native Windows without WSL has no channel at all** (`can_send()`, `console.py:156-164`): the Characters and Accounts command buttons there are disabled with `NO_TTY_HELP`, and the row routes carry 8.3. Named on the 8.1c line; §11 asks whether that is v1.
- **Module SQL while the world runs** was allowed by nothing and refused by nothing (delta fact 9); 8.7's gate is the first refusal on that path and it is a *view* gate — the seam should refuse too, so a CLI cannot do what the button will not (rule: guards prove the value arrives).
- **`shortcuts.vdf` is unread** (delta "Could not ask" 6). Nothing in 8.8 is buildable until a real folder is read; the surface is fixed now so the seam has a shape to fit.
- **The log snapshot adds a `docker logs` before every stop**; `docker logs` output can exceed the pipe buffer and deadlock a non-draining reader (`status.rs:9-12`). The seam must drain; the surface shows the snapshot's own outcome so a slow one is visible rather than a hang.
- **Uninstall cannot be cancelled past the stop** — said on the dialog before the press, and `busy_reason()` refuses the window close for its length (the import's precedent, `:798-818`).
- **The Accounts list hides one account by name.** A user who names their own account `yulon` on the Accounts tab will not see it listed; the create path's "already existed" line (`:1481-1483`) is where they learn it. Enumerated; the name is on the 8.1a line and in the remedy sentence.

---

## 9. What the implementer should NOT build yet

- No character sheet, no paperdoll, no talent view, no achievements (Phase 9, Q2c) — the Characters tab's picker shows name, level, class, online and nothing more.
- No doctor page, no shell, no console history or autocomplete (Q2f; Phase 9).
- No tuning knobs, no config editor, no settings page, no window-geometry persistence (Q8iii; bug line 289 inherited) — the Modules tab gains update checks only.
- No single-instance guard, no autostart, no automatic backups, no server self-update, no adoption of a foreign folder (Q8iv).
- No auto-stop when the client exits, no keep-awake while the server runs (Q2e).
- No coordinate teleport, no `.additem`, no heal, no summon-NPC (refused; the bridge 8.6 adopts does not cover them until an owner answer says so).
- No client-folder write of any kind (Q5): no addon, no `realmlist.wtf`.
- No direct write to `characters` or `world` while the world runs (Q7): every Characters-tab action is the server's own command; gear sets are read-only on the server side.
- No Steam group on Windows or macOS, and no `shortcuts.vdf` writer before a real one has been read.
- No My Party on TBC, Vanilla or Tortoise, and no `.rndbot spoof`/`add`/`p` route (Q4) — the reason label is the whole surface there.
- No `pending_commands` writer for Tortoise: one outcome is not a question.
- No change to `controller.py`, `state.py`, `console.py`'s transport, or the Networking tab.

---

## 10. Doc changes

- `pylauncher/README.md`, the capability table (`:11-18`): one row per new surface, in its three values — "Server commands (set up / status line)", "Dashboard line", "Save the worldserver log", "Accounts: list / set password / GM level", "Characters: teleport / mail / revive / level / rename / gear sets", "Browse bots", "My Party (WotLK)", "Module update checks", "Steam (Linux)", "Uninstall" — each cell "run live" only after the box in §5 ticks, "built" before, "no — Linux/Steam Deck only" for Steam on Windows/macOS, "no — needs a terminal (WSL)" for Tortoise's command buttons on native Windows; the "GM console" row's Linux cell stays "built" until a Phase 8 gate presses Send.
- `pyplan/README.md` §9: the three struck-through lines the parity page already prescribes, dated, in the Linux-native line's shape (`:398-401`).
- `pyplan/style-guide.md` §3: rows for `ui/widgets/outcome.py`, `ui/characters_view.py`, `ui/bots_view.py`, `ui/uninstall_dialog.py` — **applied when the modules exist**, as Phase 7 did (`phase7-decisions.md:979-985`).
- `pyplan/bug-checklist.md`: lines 239, 394, 499, 552 annotated with the `8.x` that closes each, ticked only when that box ticks; line 289 annotated "inherited — Phase 9".
- `pyplan/checklist.md` Phase 8: §5's lines, the ticked 2026-08-21 identification box kept.
- `pyplan/roadmap.md`: not edited; the parity page's Appendix A carries §8.

---

## 11. Open questions for the owner

1. **Tortoise on native Windows without WSL has no command channel** (no SOAP, no pty). Is "the Accounts row routes work; every Characters-tab button is disabled with the terminal sentence" acceptable for v1, or is Tortoise's Phase 8 Linux-only until a channel exists?
2. **Which machine has Steam for the 8.8 gate?** No test box this side has a Steam install; the format read and the press both need one (a Steam Deck, or a Linux box with Steam).
3. **May a fresh install run the 8.1 set-up itself at the end of `ready`**, so a new user never presses "Set up server commands"? The parity page recorded that the install engine's stage list does not change; this would be a stage-body addition to `ready`, not a new stage, and it restarts nothing (the config is written before the first `up`). Until answered, 8.1 is a press on the Server tab.
4. **Does an `8.x` box tick on Linux alone**, with the native-Windows press its own later line (as 7.7 did), or must the Windows press be in the same box? §5 above writes Windows into the WotLK boxes where the mechanism differs (8.1a, 8.3a, 8.4a, 8.9a); if the answer is "own line", those split.
