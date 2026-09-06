# Hypeer reader — Rust launcher (`origin/rust-main`) feature-by-feature mechanism report

**Branch SHA:** `21cfdf144b0969b19f4b887e3e97b767dab99f86` (`git -C /c/Users/perzi/dml-phase8 rev-parse origin/rust-main`).
All `path:line` citations are against that SHA. Nothing was checked out; every file was read via `git show origin/rust-main:<path>`.

**Line ranges actually read** (rule 2 — where a file was not read whole, say so):

| file | lines read |
|---|---|
| `docs/FEATURES.md` | 1–119 (whole) |
| `crates/dml-wow/src/soap.rs` | 1–513 (whole) |
| `crates/dml-wow/src/soap_cmds.rs` | 1–1228 (whole) |
| `crates/dml-wow/src/account_write.rs` | 1–343 (whole) |
| `crates/dml-wow/src/botid.rs` | 1–339 (whole) |
| `crates/dml-wow/src/bridge.rs` | 1–534 (whole) |
| `crates/dml-wow/src/registry.rs` | 1–109 (whole) |
| `crates/dml-wow/src/status.rs` | 1–768 (all non-test; tests 769–1127 skipped) |
| `crates/dml-wow/src/logsnap.rs` | 1–400 (non-test 1–381; 400–815 are tests) |
| `crates/dml-wow/src/tuning.rs` | 1–659 (all non-test) |
| `crates/dml-wow/src/restore.rs` | 1–372 (all non-test) |
| `crates/dml-wow/src/backup.rs` | 1–760 fully; 760–1155 code-only (comment lines filtered out by the reading command) |
| `crates/dml-wow/src/modmgr.rs` | 1–120, 536–660, 2295–2340, 2775–2845 read in full; the rest (120–536, 660–2295, 2340–2775, 2845–2900) read only through a grep of `pub fn` / `cmd.args` / `const` lines. **Not read in full.** |
| `crates/dml-wow/src/moduletail.rs` | 1–698 (all non-test) |
| `crates/dml-wow/src/party.rs` | 1–789 (all non-test) |
| `crates/dml-wow/src/party_specs.rs` | 1–196 (all non-test) |
| `crates/dml-wow/src/pages.rs` | 1–801 (all non-test) |
| `crates/dml-wow/src/paperdoll.rs` | 1–219 (all non-test) |
| `crates/dml-wow/src/stats.rs` | 1–420, 470–574 (non-test; **420–470 not read** — the history/zones/continents assembly tail) |
| `crates/dml-wow/src/iteminfo.rs` | 1–200 (**200–518 not read**) |
| `crates/dml-wow/src/soap_autosetup.rs` | 1–180 (**180–336 not read**) |
| `crates/dml-wow/src/accountwide.rs` | 1–150 (**150–970 not read**) |
| `crates/dml-wow/src/lan.rs` | 1–140 (**140–788 not read**) |
| `crates/dml-wow/src/ahbot.rs` | 1–80 (**80–414 not read**) |
| `crates/dml-wow/src/lifecycle.rs` | 918–1090 in full; the rest via a `pub fn`/`const`/`saveall` grep. **Not read in full.** |
| `crates/dml-wow/src/config.rs` | 686–700, 855–946 only. **Not read in full.** |
| `crates/dml-wow/src/destructive.rs` | 1–60 in full; the rest via a `pub fn`/`args(` grep. **Not read in full.** |
| `crates/dml-wow/src/maint.rs` | 560–600, 740–800 in full; the rest via a `pub fn`/`args(` grep. **Not read in full.** |
| `crates/dml-wow/src/commands.rs` | 1–45 (**45–624 not read**) |
| `launcher/src-tauri/src/watch.rs` | 1–227 (whole) |
| `launcher/src-tauri/src/power.rs` | 1–66 (whole) |
| `launcher/src-tauri/src/single_instance.rs` | 1–109 (whole) |
| `launcher/src-tauri/src/autostart.rs` | 1–73 (whole) |
| `launcher/src-tauri/src/realmlist.rs` | 1–320 (non-test) |
| `launcher/src-tauri/src/lib.rs` (9456 lines) | 3495–3530, 3796–3805, 4036–4060, 4543–4680, 5445–5470 read in full; the full `#[tauri::command]` name list enumerated by grep. **The overwhelming majority of this file was NOT read.** |

**One global fact that governs every "over SOAP" claim below.** There is exactly ONE SOAP transport
(`crates/dml-wow/src/soap.rs:243` `pub fn exec`) — a `reqwest::blocking` POST of an
`urn:AC` `executeCommand` envelope (`soap.rs:127-132`) to `http://127.0.0.1:7878/` by default
(`soap.rs:101`), Basic auth, 30 s response / 5 s connect timeout (`soap.rs:246-248`).
Every feature that says "over SOAP" below has the call path
**`<feature>_cmd(...)` (builds the verbatim command string, in `soap_cmds.rs`/`party.rs`) →
`soap::exec(&SoapConfig::load(), &cmd)` (fires it) → `<arm>_result(outcome)` (maps the four
outcomes)**, and the `exec` call is taken under `state.soap_lock` (`Arc<Mutex<()>>`) in the
Tauri command. Where I verified that whole path I say so and cite both ends.

---

# Part 1 — FEATURES.md, feature by feature

## Server control

### Live server dashboard
**What it does.** `status::read_server_detail` (`status.rs:675-699`) composes one verdict out of
four independent probes: `docker ps -a` container rows → world state; `docker inspect
{{.State.StartedAt}}` + `docker logs --since` → `world_ready`; a SOAP `server info` (only when the
world container is `running`); `docker inspect {{.State.ExitCode}}` (only when it is not running and
not absent). `compute_verdict` (`status.rs:496-526`) collapses those into
`online|starting|soap_unreachable|stopped|crashed`. Ports and bot counts are added, then
`assemble_server_detail` (`status.rs:634-669`) emits the envelope.

**Server-side mechanism.**
- SOAP, verbatim command text `server info` — built and fired at `status.rs:683`
  (`soap::exec(soap_cfg, "server info")`); parsed by `parse_server_info_fields`
  (`status.rs:79-120`), which line-matches `AzerothCore rev. `, `Connected players: `,
  `Server uptime: `, `|- Mean:`, `|- Median:`.
- `docker` CLI, argv `["ps","-a","--format","{{.Names}}|{{.State}}|{{.Status}}"]`
  (`status.rs:263`).
- `docker` CLI, argv `["inspect","-f","{{.State.StartedAt}}","ac-worldserver"]` (`status.rs:300`)
  then `["logs","--since",<started>,"ac-worldserver"]` (`status.rs:313`).
- `docker` CLI, argv `["inspect","-f","{{.State.ExitCode}}","ac-worldserver"]` (`status.rs:345`),
  `["inspect","-f","{{.State.Running}}",<name>]` (`status.rs:374`, `status.rs:394`),
  `["inspect","-f","{{.RestartCount}}",<name>]` (`status.rs:438`).
- `docker` CLI, argv `["port",<container>,<internal>]` (`status.rs:547`); the four probes are
  `ac-worldserver:8085`, `ac-authserver:3724`, `ac-worldserver:7878`, `ac-database:3306`
  (`status.rs:566-569`).
- MySQL read, `<characters>.characters`: `SELECT COUNT(*) FROM characters WHERE online = 1 AND
  <bot_clause>` (`status.rs:577-582`, run at `status.rs:590`).
- Client's files: none.
- **Bots max is a FILE read, not a server read**: `AiPlayerbot.MaxRandomBots` out of the live
  `playerbots.conf` via `ConfigReader::compute_value` (`status.rs:602-605`).

**Start / stop / restart.** `lifecycle::games_lifecycle_stream_with` (`lifecycle.rs:1090+`)
drives `docker compose` (`compose_up_argv`/`compose_down_argv`/`compose_sequence_for_mode`,
re-exported from `dml_core::compose` at `lifecycle.rs:40-42`; the argv themselves live in
`dml-core` and I did **not** read them). World-only restart:
`world_restart_stream` (`lifecycle.rs:929-1043`) fires SOAP `saveall` (`lifecycle.rs:981`) then
`docker restart -t 300 ac-worldserver` (`lifecycle.rs:986`).

**Threading / serialisation facts.** `soap::exec` is blocking and "must not run on the async
runtime thread, callers wrap `exec` in `spawn_blocking`" (`soap.rs:240-242`). The `saveall` in
`world_restart_stream` is taken under `soap_lock` (`lifecycle.rs:976`) — the doc comment says it
"serializes the `saveall` SOAP call against any other native SOAP command in flight"
(`lifecycle.rs:926-928`). Every docker probe is bounded: `DOCKER_PROBE_TIMEOUT = 5s`
(`status.rs:43`), `CONSOLE_TAIL_TIMEOUT = 20s` (`status.rs:46`), `COMPOSE_DOWN_TIMEOUT = 240s`
(`lifecycle.rs:344`), `COMPOSE_UP_TIMEOUT = 600s` (`lifecycle.rs:349`),
`WORLD_RESTART_STOP_TIMEOUT` (used `lifecycle.rs:989`). All docker reads go through
`output_bounded_draining`, never the non-draining helper — `status.rs:9-12` records why.

**Incident note behind the design.** Verbatim, `status.rs:406-410`:
> `// Boot-loop evidence (incident follow-up 2). On the night of the 2026-07-21`
> `// incident the world crash-retried on "Can't connect to MySQL (110)" for ten`
> `// minutes while the readiness wait kept printing "still waiting ... bots`
> `// respawning". These are the two primitives that tell the two situations`
> `// apart; the decision itself lives in `lifecycle::wr_wait_for_world`.`

Also `status.rs:384-391`:
> `/// The "can't tell -> not running" collapse above is the right default for a`
> `/// PRECONDITION (refusing to act on an unreadable stack is safe). It is the`
> `/// wrong default for the world-restart liveness strike counter, where it turns`
> `/// a few seconds of Docker unavailability into a fabricated "the world server`
> `/// exited" abort of a perfectly healthy restart.`

**Emulator family.** AzerothCore WotLK only, hard-coded in several places: container names
`ac-worldserver` / `ac-authserver` / `ac-database` (`status.rs:239`), the version line prefix
`"AzerothCore rev. "` (`status.rs:88`), the readiness marker `"world initialized in"`
(`status.rs:291`), port 7878 for SOAP (`status.rs:568`, `soap.rs:101`), 8085/3724/3306
(`status.rs:566-569`), and the mod-playerbots conf key `AiPlayerbot.MaxRandomBots`
(`status.rs:603`).

---

### Server console
**What it does.** Two halves. The log tail is `status::read_console_tail` (`status.rs:755-767`):
`docker logs --tail <lines> ac-worldserver`, stdout+stderr merged, ANSI-stripped
(`strip_ansi`, `status.rs:711-730`), `\r` dropped, split to lines. The GM command line is
`wow_console_send_native` (`lib.rs:3856`), which fires the user's raw text over SOAP and decodes
the reply. The `.commands` cheat-sheet is a **static, embedded text table** — `commands::cmd_block_for`
(`commands.rs:29+`), one block per installed module, no server call at all
(`commands.rs:8-12`).

**Server-side mechanism.**
- `docker` CLI, argv `["logs","--tail",<lines>,"ac-worldserver"]` (`status.rs:757`).
- SOAP: whatever the user typed, verbatim, un-validated. Result mapper
  `console_send_result` (`soap_cmds.rs:439-454`) — the only mapper that entity-decodes the OK
  result as well as the fault text (`soap_cmds.rs:441`).
- MySQL: none. `.playerbots` family: none.

**Threading.** `read_console_tail` is bounded at 20 s (`status.rs:46`, used `status.rs:759`);
console send is taken under `soap_lock` (per the pattern at `lib.rs:3856+`; I read the sibling
`wow_gm_gold_native` at `lib.rs:4049` for the exact idiom).

**Incident note.** None recorded on the console path itself. `status.rs:9-12` records the
pipe-buffer deadlock that forced `output_bounded_draining`:
> `// wedged/absent `docker` can never hang the poll` … `` `docker logs` output can ``
> `` // exceed the OS pipe buffer, which deadlocks the non-draining helper ``

Note: history-recall (F2) and autocomplete (F3) are **frontend only** —
`launcher/src/lib/pages/Console.svelte:52` (`// --- Input helpers: history recall (F2) + autocomplete (F3)`).

**Emulator family.** AzerothCore only (`ac-worldserver` hard-coded, `status.rs:757`).

---

### Game installer
**What it does.** Not read. The catalog + install streams live in
`crates/dml-wow/src/install_native.rs` (4198 lines) and `launcher/src-tauri/src/provision.rs`
(1492 lines); the Tauri surface is `games_catalog`, `games_install`, `games_install_native`,
`games_install_input`, `games_install_cancel` (grep of `#[tauri::command]` in `lib.rs`).

**Server-side mechanism.** **Could not establish** — see "Facts I could not establish".

---

### LAN play
**What it does.** `lan::lan_action` (declared in the module header, `lan.rs:11-13`) does a
container check, a retry loop waiting for the realm DB to answer, then an `UPDATE` of the realm
address plus a read-back.

**Server-side mechanism.**
- MySQL **write** — the realmlist row. `account_write.rs:6-8` names it as one of the standing
  exceptions: *"Two exceptions already existed — `wow backup restore` and the LAN toggle's
  realmlist `UPDATE`."* The exact statement text is in the unread half of `lan.rs` (140–788).
- `docker` CLI: container-running checks (named at `lan.rs:12-13`, argv not read).
- Client's files: **`realmlist.wtf` is written by a different feature** (`realmlist.rs`, below) —
  the LAN toggle only prints instructions (`lan.rs:113`:
  `"Other PCs on your network: set realmlist {ip}"`).

**Threading.** Retry budget `tries_and_gap` (`lan.rs:72-78`): `refresh` gets `(60 tries, 10 s
gap)`, every interactive action gets `(18, 5)`.

**Incident note.** None recorded in the half I read. There is a security note, `lan.rs:7-9`:
> `//! live server: the private-address classifier (security-relevant -- it is`
> `//! the only thing standing between `dml lan on <ip>` and silently exposing`
> `//! the realm to a public address)`

**Emulator family.** AzerothCore only, **stated explicitly and deliberately**, `lan.rs:20-23`:
> `//! AC-ONLY BY DECISION (chunk2-decisions.md item 3): `dml::db` has no`
> `//! MaNGOS/Tortoise support, so native mode never reaches the oracle's`
> `//! `tw_logon` branch (`90-main.sh:942-960`) -- WSL keeps handling every`
> `//! other title family; native only ever drives the single fixed AC title.`

That is the ONE place in everything I read that mentions the CMaNGOS family, and it says the
Rust native path never reaches it.

---

### Play together over the internet (router forwarding / Tailscale)
**What it does.** `wow_tailscale` (`lib.rs:5451-5459`) validates the action against a closed
allowlist (`TAILSCALE_ACTIONS`) then, in native mode, drives the Windows `tailscale.exe`
directly.

**Server-side mechanism.** None of the four (no SOAP, no MySQL, no docker, no client files) on
the native path — an external `tailscale.exe` subprocess. Verbatim, `lib.rs:5445-5449`:
> `/// hangs waiting on the browser login. Native mode (spike/docker-desktop-`
> `/// native) has no distro to shell into -- `dml wow tailscale` sudo's into`
> `/// pacman/systemd/iptables, none of which exist on Windows -- so it drives`
> `/// the Windows Tailscale app's `tailscale.exe` directly instead, mapping its`
> `/// output into the same JSON shapes.`

**Emulator family.** N/A.

---

### Auto-stop
**What it does.** `watch.rs` is a pure state machine: DISARMED until `Wow.exe` is first seen;
once ARMED, two consecutive polls without it fire the stop; firing auto-disarms
(`watch.rs:36-73`). The impure half (5 s cadence, `tasklist` probe, the actual stop) is in
`lib.rs` (`watch.rs:3-5`), and `set_auto_shutdown` is the Tauri toggle.

**Server-side mechanism.** Client-side process probe only:
`tasklist /FI "IMAGENAME eq Wow.exe" /FO CSV /NH` (named at `watch.rs:75`; parsed by
`tasklist_shows_wow`, `watch.rs:79-83`). The stop it fires is the ordinary lifecycle
`docker compose` path.

**Threading.** `MISS_THRESHOLD = 2` (`watch.rs:28`); the loop owns a 5 s cadence (`watch.rs:3-4`).

**Incident note.** Verbatim, `watch.rs:88-95`:
> `/// The None case matters: a spawn failure, a nonzero exit ("ERROR: The RPC`
> `/// server is unavailable" under session/load trouble), or empty output are`
> `/// correlated failure modes -- treating them as "WoW is gone" would let two`
> `/// such failures 5s apart clear the 2-poll debounce and gracefully stop the`
> `/// server out from under a player who is still in-game.`

**Emulator family.** N/A (Windows-only, `tasklist`).

---

### Doctor & shell
**What it does.** `dml_doctor`, `open_shell` Tauri commands (grep of `lib.rs`). Environment
health checks live in `crates/dml-wow/src/preflight.rs` (2306 lines) — **not read**.
`maint::read_port_check` (`maint.rs:343`) is the LAN/DB diagnostic: `docker port <container>
<internal>` (`maint.rs:264`) plus a `.env` parse for `parse_db_external_port` (`maint.rs:299`).
`maint::read_docker_usage` runs `docker system df` (`maint.rs:204`);
`maint::read_container_stats` runs a `docker` argv at `maint.rs:174` (**exact argv not read**).

**Server-side mechanism.** `docker` CLI (argv above); no SOAP, no MySQL on the paths I read.

**Incident note.** none recorded (in the parts read).

---

## Your characters

### Character sheet (gear, 3D model, talent trees, achievement browser)
**What it does.** Four independent reads.
1. **Gear**: `paperdoll::read_paperdoll` (`paperdoll.rs:200-218`) — ONE cross-schema join,
   equipment slots 0–18, ordered by slot.
2. **Talent trees**: `pages::read_char_progress` (`pages.rs:714-769`) reads
   `activeTalentGroup`/`talentGroupsCount` and the active spec's talent spell ids; the tree
   LAYOUT is a static frontend JSON (`launcher/src/lib/talent-trees-wotlk.json`).
3. **Achievements**: `pages::read_achievements` (`pages.rs:776-799`) returns the earned set only;
   the 1320-entry browser (categories, points) is a static frontend JSON
   (`launcher/src/lib/achievements-wotlk.json`, referenced at
   `launcher/src/lib/CharacterSheet.svelte:222`).
4. **Tooltips / icons / 3D display id**: `iteminfo.rs` fetches Wowhead JSON + zamimg icons over
   HTTPS and disk-caches them (`iteminfo.rs:6-13`).

**Server-side mechanism.**
- MySQL read, `<characters>.characters ⋈ character_inventory ⋈ item_instance ⋈
  <world>.item_template`, slots 0–18: `paperdoll.rs:60-68` (`join_tail`), two column sets at
  `paperdoll.rs:72-79` (new appearance schema) and `paperdoll.rs:83-90` (old packed
  `playerBytes`/`playerBytes2`), fired at `paperdoll.rs:206` / `paperdoll.rs:213`.
- MySQL read, `<characters>.characters`: `SELECT guid FROM characters WHERE name = ? LIMIT 1;`
  (`pages.rs:614`).
- MySQL read, `<characters>.characters`:
  `SELECT activeTalentGroup, talentGroupsCount FROM characters WHERE guid = ?;` (`pages.rs:615-616`).
- MySQL read, `<characters>.character_achievement`:
  `SELECT COUNT(*) FROM character_achievement WHERE guid = ?;` (`pages.rs:617-618`),
  `SELECT achievement, date FROM character_achievement WHERE guid = ? ORDER BY date DESC LIMIT 10;`
  (`pages.rs:619-620`),
  `SELECT achievement, date FROM character_achievement WHERE guid = ? ORDER BY achievement;`
  (`pages.rs:621-622`).
- MySQL read, `<characters>.character_talent`:
  `SELECT spell FROM character_talent WHERE guid = ? AND (specMask & (1 << ?)) ORDER BY spell;`
  (`pages.rs:623-624`).
- HTTPS (not one of the listed mechanisms): `https://nether.wowhead.com` (`iteminfo.rs:53`),
  `https://wow.zamimg.com` (`iteminfo.rs:57`), `https://www.wowhead.com` (`iteminfo.rs:61`),
  cached under `~/.dml/wowhead-cache` (`iteminfo.rs:7-13`).
- SOAP: **NONE**, deliberately — see the incident note.

**Threading.** `paperdoll` tries the new appearance schema and, only on `DbError::Query`, falls
back to the old packed columns (`paperdoll.rs:206-217`) — a `DbError::Unreachable` is propagated,
never retried. `read_char_progress` runs five queries in a fixed order; the last three degrade to
an empty result set on failure (`pages.rs:740-766`), only the guid and talent-meta lookups are
hard errors (`pages.rs:736-739`).

**Incident note.** Verbatim, `paperdoll.rs:25-32`:
> `//! DOCUMENTED DIVERGENCE (side-effect only, not a JSON field). The CLI arm, for`
> `//! a character who is currently ONLINE, first fires a rate-limited SOAP`
> `//! `saveall` to flush live gear to the DB before reading. This native reader`
> `//! does NOT issue SOAP — it is a pure MySQL read by design (no engine console`
> `//! dependency). The JSON SHAPE is identical; only a currently-online character's`
> `//! gear could be marginally staler than a CLI read that happened to win the`
> `//! saveall race.`

And, on character-name lookup, `paperdoll.rs:202-204`:
> `// utf8mb4_bin lookup -- canonical form or nothing (db::canon_char_name):`
> `` // `paperdoll testa` on a server whose character is `Testa` was the live ``
> `// find that exposed this whole class.`

**Emulator family.** AzerothCore WotLK only. Hard-coded AC table/column names throughout
(`character_inventory`, `item_instance`, `item_template.Quality`/`ItemLevel`/`displayid`,
`character_achievement`, `character_talent.specMask`, `characters.playerBytes`). The
appearance-schema fallback (`paperdoll.rs:14-20`) is explicitly *"AC's migration split the packed
`playerBytes`/`playerBytes2` appearance columns"*.

---

### Teleport
**What it does.** `pages::read_teleport_list` (`pages.rs:116-120`) lists named locations out of
`game_tele`; `wow_teleport_native` (`lib.rs:4570-4587`) fires a SOAP teleport;
`wow_teleport_coords_native` (`lib.rs:3506+`) writes a position directly into `characters` and
**refuses an online character**.

**Server-side mechanism.**
- MySQL read, `<world>.game_tele`:
  `SELECT name,CAST(position_x AS CHAR),CAST(position_y AS CHAR),CAST(position_z AS CHAR),map FROM
  game_tele WHERE {where_clause} ORDER BY name LIMIT 500;` (`pages.rs:84-87`), fired at
  `pages.rs:118`. `where_clause` is `1=1` or `name LIKE ?` bound to `%<search>%` (`pages.rs:80-83`).
- SOAP, verbatim command text `teleport name {char_name} {to}` — built by
  `soap_cmds::teleport_name_cmd` (`soap_cmds.rs:280`), fired at `lib.rs:4581`
  (`dml_wow::soap::exec(&cfg, &cmd)`), mapped by `soap_cmds::teleport_result`
  (`soap_cmds.rs:530-549`). **Complete call path verified.**
- MySQL **write**, `<characters>.characters` (coords path):
  `UPDATE characters SET position_x=?, position_y=?, position_z=?, map=?, orientation=0 WHERE
  guid=?` — this is `RETURN_HOME_UPDATE_SQL` (`lib.rs:3800-3801`), shared by the coords arm
  (`lib.rs:3502-3504` calls it *"THIRD sanctioned direct `characters` write"*).

**Threading.** The SOAP fire is taken under `state.soap_lock` inside `spawn_blocking`
(`lib.rs:4577-4579`).

**Incident note.** Verbatim, `lib.rs:3496-3502`:
> `/// NATIVE-MODE `wow teleport-coords` (`90-main.sh:1895-1933`, Part 5a).`
> `/// UNLIKE `teleport`/`gm return-home`, this arm NEVER calls SOAP -- an`
> `/// ONLINE character is REJECTED (`CHAR_ONLINE`), not teleported live: a`
> `/// running worldserver holds its own in-memory position and would clobber`
> `/// this direct write on the character's next auto-save/logout.`

**Emulator family.** AzerothCore only: `game_tele` table, the `teleport name` GM command,
`characters.position_x/y/z/map/orientation`. Validation caps map id at `<= 999`
(`soap_cmds.rs:69-71`) and coords at `|v| <= 20000` (`soap_cmds.rs:80-82`).

---

### GM tools (revive, heal, level, gold, return home, summon, rename/customize)
**What it does.** Six separate arms. FOUR of them do **not** use a stock AzerothCore GM command
at all — they call CUSTOM Eluna/mod-ale bridge commands the launcher itself deploys.

**Server-side mechanism — the verbatim command text of each:**

| arm | verbatim command sent over SOAP | builder | fire site | mapper |
|---|---|---|---|---|
| set level | `.character level {player} {level}` (note the **leading dot**) | `soap_cmds.rs:167` | `lib.rs:3991+` | `gm_level_result` `soap_cmds.rs:475-486` |
| rename / customize / changerace / changefaction | `character {flag} {player}` (**no** leading dot) | `soap_cmds.rs:178` | `lib.rs:4013+` | `gm_at_login_result` `soap_cmds.rs:491-502` |
| set gold | `dml_gm_money {player} {copper}` (copper = gold × 10000) | `soap_cmds.rs:190` | `lib.rs:4051` | `party_fire_result(.., "gold")` `soap_cmds.rs:392-412` |
| heal | `dml_gm_health {player} 100` | `soap_cmds.rs:195` | `lib.rs:4062+` | `party_fire_result(.., "heal")` |
| revive | `dml_gm_revive {player}` | `soap_cmds.rs:200` | `lib.rs:4092`, exec at `lib.rs:4102` region | `party_fire_result(.., "revive")` `lib.rs:4102` |
| summon NPC | `dml_summon_npc {player} {entry}` | `soap_cmds.rs:211` | `soap_cmds.rs:662` (`gm_summon`) | `party_fire_result(.., "summon")` `soap_cmds.rs:663` |
| return home (ONLINE) | `teleport name {player} {capital.name}` | inline `lib.rs:4641` | `lib.rs:4642` | `return_home_online_result` `lib.rs:3744` |

- **The `dml_*` commands are NOT AzerothCore commands.** They are Lua scripts the launcher
  deploys into mod-ale: `cli/lua/gm/dml_gm.lua`, `cli/lua/gm/dml_summon_npc.lua` (see the
  **Bridge setup** entry below).
- MySQL read (summon precondition), `<world>.creature_template`:
  `SELECT name FROM creature_template WHERE entry=? LIMIT 1` (`soap_cmds.rs:633`).
- MySQL read (online precondition, shared by gold/heal/revive/summon), `<characters>.characters`:
  `SELECT guid FROM characters WHERE name=? AND online=1 LIMIT 1` (`soap_cmds.rs:605`, via
  `char_is_online` `soap_cmds.rs:597-610`; call site for gold at `lib.rs:4046`).
- MySQL read (return home), `<characters>.characters`:
  `SELECT guid, race, online FROM characters WHERE name = ? LIMIT 1` (`lib.rs:3796`, fired
  `lib.rs:4611-4616`).
- MySQL **write** (return home, OFFLINE arm), `<characters>.characters`:
  `UPDATE characters SET position_x=?, position_y=?, position_z=?, map=?, orientation=0 WHERE guid=?`
  (`lib.rs:3800-3801`, fired `lib.rs:4659-4664`). Called *"the FIRST db-write path in the native
  core"* at `lib.rs:4592-4593`.
- `.playerbots` family: **none**.

**Threading.** Every fire is `spawn_blocking` + `soap_lock` (`lib.rs:4043-4049` for gold;
`soap_cmds.rs:660` for summon: `let _guard = lock.lock()...` immediately before `soap::exec`).
Order is load-bearing and documented at `soap_cmds.rs:614-619`:
> `// Order matches the oracle exactly: creature_template`
> `// existence+name lookup (World DB) -> online check (Characters DB) -> SOAP`
> `// fire -> success with the looked-up NPC name.`

**Incident note.** Verbatim, `soap_cmds.rs:598-600`:
> `// Canonical stored form -- characters.name is utf8mb4_bin, see`
> `` // db::canon_char_name. Without this, `gm gold testa` answered NOT_ONLINE ``
> `` // for an online `Testa` before SOAP was ever consulted. ``

And on the fault mapper, `soap_cmds.rs:390-391`:
> `` // fault message. Unlike the generic mappers, the SOAP_FAULT text here is NEVER the server's own ``
> `` // fault string -- bash's `_party_fire` discards `$out` entirely on rc=2. ``

**Emulator family.** AzerothCore WotLK only. The gold cap `0..=214748` is named
*"(the WotLK money cap)"* (`soap_cmds.rs:186`); level range `1..=255` (`soap_cmds.rs:161`);
faction capitals are Stormwind/Orgrimmar (`lib.rs:3785` `faction_capital`, tests at
`lib.rs:8877`/`8889`). The bridge commands require mod-ale.

---

### Item mail
**What it does.** `pages::read_items_search` (`pages.rs:601-605`) searches `item_template`;
`wow_mail_item_native` (`lib.rs:4543-4566`) sends the mail over SOAP.

**Server-side mechanism.**
- MySQL read, `<world>.item_template`:
  `SELECT entry,name,Quality,ItemLevel,RequiredLevel,class,subclass,InventoryType,displayid FROM
  item_template WHERE {where} ORDER BY RequiredLevel, name LIMIT ?;` (`pages.rs:561-565`),
  default limit 50 (`pages.rs:533`), every filter bound (`pages.rs:545-560`).
- SOAP, verbatim command text `send items {to} "{subject}" "{body}"{attach}` where `attach` is
  ` id:count` repeated (`soap_cmds.rs:246`, assembled `soap_cmds.rs:233-243`). Fired at
  `lib.rs:4560`; mapped by `soap_cmds::mail_result` (`soap_cmds.rs:507-526`).
  **Complete call path verified.**
- Cap: 1–12 item specs (`soap_cmds.rs:230-232`).

**Threading.** `spawn_blocking` + `soap_lock` (per the `lib.rs:4543-4566` block; exec at
`lib.rs:4560`).

**Incident note.** Verbatim, `soap_cmds.rs:249-252`:
> `` /// Strip `"` and replace CR/LF each with a single space — matching bash's ``
> `` /// `${var//\"/}` / `${var//$'\n'/ }` / `${var//$'\r'/ }` chain EXACTLY ``
> `/// (replace, not delete, so words don't glue together). Closes the AC #2695`
> `` /// newline-injection crash surface for `.send items`. ``

**Emulator family.** AzerothCore only (`item_template` column names, the `send items` GM command,
AC issue #2695).

---

### Gear sets
**What it does.** **Entirely frontend.** Saved sets are `localStorage` under key
`dml.gearsets.v1` (`launcher/src/lib/gearsets.svelte.ts:36`); mailing a set reuses the existing
mail-item arm, chunked at 12 stacks per mail (`gearsets.svelte.ts:39` `MAIL_CHUNK = 12`).
Export/import as text is `launcher/src/lib/gearset-toml.ts`.

**Server-side mechanism.** SOAP `send items …` only, through `wowMailItem`
(`gearsets.svelte.ts:18`) → `wow_mail_item_native` → `soap_cmds::mail_items_cmd`. **No Rust
backend surface exists for gear sets** — verbatim, `gearsets.svelte.ts:1-4`:
> `// Gear sets light (Batch 5 F4): save a character's equipped set (from the`
> `// paperdoll) as a named local set; mail any saved set to any character via`
> `` // the existing mail-item arm. Entirely frontend -- localStorage + the ``
> `` // already-shipped `wow mail-item` CLI/SOAP path; no new backend surface. ``

**Incident note.** Verbatim, `gearsets.svelte.ts:6-10`:
> `// Server-side facts this leans on (verified against cs_send.cpp):`
> `` // `.send items` creates FRESH item copies with no soulbind/bonding check ``
> `// (BoP/heirloom/quest items all sendable); enchants/gems are NOT carried`
> `// (the paperdoll only stores entry ids anyway); offline recipients work;`
> `// hard cap 12 item stacks per mail -- hence the chunking below.`

**Emulator family.** AzerothCore (`cs_send.cpp` is an AC source file).

---

## Playerbots

### ~2000 AI bots populating the world
**What it does.** Nothing in the launcher spawns the bots — mod-playerbots does. The launcher
only counts them and sets `AiPlayerbot.MaxRandomBots`.

**Server-side mechanism.**
- MySQL read (count online), `<characters>.characters` with the two-signal bot clause
  (`status.rs:577-582`).
- Conf-file read/write for the population number:
  `conf:playerbots.conf:AiPlayerbot.MaxRandomBots` (`crates/dml-wow/data/config-registry.json:234`,
  key `bots.population` at `:224`; read at `status.rs:603`).
- `.playerbots` command family: **NOT USED ANYWHERE.** A `git grep` for `.playerbots` /
  `"playerbots ` across `crates/` and `launcher/src-tauri/` returns only schema names
  (`acore_playerbots`, `playerbots_account_type`, `playerbots.conf`) — never a worldserver
  command.

**Bot identity** is the single answer in `botid.rs`: registry OR account-name prefix.
- Registry clause: `{col} IN (SELECT account_id FROM {playerbots}.playerbots_account_type WHERE
  account_type IN (1,2,3))` (`botid.rs:112-113`).
- Prefix clause: `{col} IN (SELECT id FROM {auth}.account WHERE UPPER(username) LIKE '<PREFIX>%')`
  (`botid.rs:133-134` + `botid.rs:148-151`).
- Combined: `({registry} OR {prefix})` (`botid.rs:164`); human is `NOT {bot}` (`botid.rs:174`).

**Incident note.** The longest one in the tree, verbatim `botid.rs:4-17`:
> `//! WHY THIS MODULE EXISTS (incident 2026-08-01). Every bot check in the repo`
> `//! used to ask a single question: is the account in`
> `` //! `acore_playerbots.playerbots_account_type` with `account_type IN (1,2)`? ``
> `//! That registry is populated by mod-playerbots itself — and on a freshly`
> `//! built install it can be **completely empty** while 1000 bot characters`
> `` //! exist and play. Live proof: the `native-test` install had 1000 bot ``
> `` //! characters across 100 `RNDBOT*` accounts with `playerbots_account_type`, ``
> `` //! `playerbots_random_bots` and `playerbots_account_links` ALL at zero rows. ``
> `` //! `account NOT IN (<empty set>)` is TRUE for every row, so the human filter ``
> `//! fails **OPEN**: every bot was reported as a real player on the launcher's`
> `` //! Home page, while `bots online` (the same predicate, inverted) reported 0. ``
> `//! A detector whose failure mode is "everything is a human" is worse than no`
> `//! detector, because nothing about the output looks broken.`

Second incident, verbatim `botid.rs:96-99`:
> `` /// Signal 1: the playerbots registry (`1` = random bot, `2` = addclass bot, ``
> `` /// `3` = mod-city-bots citizen — `CITIZEN_ACCOUNT_TYPE` in that module's ``
> `` /// `CitizenInfo.h`; verified live 2026-08-10: 400 type-3 rows for the ``
> `` /// `citybot*` stage cast, which the launcher was counting as FAMILY). ``

Third, verbatim `botid.rs:33-38`:
> `/// FAIL-SAFE DIRECTION. The prefix arm carries the mirror-image hazard: an`
> `` /// EMPTY prefix would compile to `LIKE '%'`, which matches every account and ``
> `` /// would classify the whole family as bots. ``

**Emulator family.** AzerothCore + mod-playerbots. Hard-coded: schema table
`playerbots_account_type`, conf key `AiPlayerbot.RandomBotAccountPrefix` (`botid.rs:55`), default
prefix `"rndbot"` (`botid.rs:46`). Also aware of the third-party `mod-city-bots`
(`botid.rs:97-99`).

---

### My Party
**What it does.** Add a bot by class/gender, kick, dismiss-all, relogin, `botcmd`
(gear/talents/maintain/spec), and preset save/list/delete/load/show/import.
`party::party_add` (`party.rs:673-714`) is a four-outcome state machine: online-guid lookup →
pre-fire member snapshot → SOAP fire → new-member poll → name resolution → spec + autogear
whispers.

**Server-side mechanism — every command is a CUSTOM `dml_*` bridge command, verbatim:**

| action | command text | builder |
|---|---|---|
| add a bot | `dml_addclass {player} {class}` (+ ` {gender}` when given) | `party.rs:175-179` |
| kick | `dml_uninvite {bot}` | `party.rs:186` |
| post-kick logout | `dml_whisper {player} {bot} logout` | `party.rs:195` |
| relogin | `dml_login {player} {bot}` | `party.rs:202` |
| botcmd gear | `dml_whisper {player} {bot} autogear` | `party.rs:211` + `party.rs:226` |
| botcmd talents | `dml_whisper {player} {bot} talents autopick` | `party.rs:212` + `party.rs:226` |
| botcmd maintain | `dml_whisper {player} {bot} maintenance` | `party.rs:213` + `party.rs:226` |
| botcmd spec | `dml_whisper {player} {bot} talents spec {spec}` | `party.rs:232-234` |
| preset-load per class | `dml_addclass {player} {class}` | `party.rs:241`, fired `party.rs:604` |
| preset-load finish | `dml_whisper {player} {bot} talents autopick` then `… autogear` | `party.rs:247`/`party.rs:250`, fired `party.rs:618-619` |

- MySQL reads, `<characters>`:
  - `SELECT guid FROM characters WHERE name=? AND online=1 LIMIT 1` (`party.rs:264`).
  - `SELECT memberGuid FROM group_member WHERE guid=(SELECT guid FROM group_member WHERE
    memberGuid=? LIMIT 1)` (`party.rs:267-268`).
  - `SELECT c.name FROM group_member gm JOIN characters c ON c.guid = gm.memberGuid WHERE gm.guid
    = (SELECT guid FROM group_member WHERE memberGuid=? LIMIT 1) AND <bot_clause> ORDER BY c.name`
    (`party.rs:279-286`).
  - `SELECT c.class FROM group_member gm JOIN characters c …` (`party.rs:294-301`).
  - `SELECT name FROM characters WHERE guid=? LIMIT 1` (`party.rs:305`).
- Presets are FILES, not DB rows: `~/.dml/party-presets/<name>`, one class name per line,
  LF-terminated (`party.rs:342-344`, `party.rs:354-356`).
- Spec names come from a FILE read of the deployed `playerbots.conf`
  (`party_specs::find_conf` `party_specs.rs:39-52`, keys
  `AiPlayerbot.PremadeSpecName.<cls>.<spc>` / `AiPlayerbot.PremadeSpecLink.<cls>.<spc>.<lvl>`,
  parsed `party_specs.rs:121-173`).
- `.playerbots` family: **none**. `docker exec`: **none**.

**Threading.** Every SOAP fire is under a `lock.lock()` guard, re-taken per fire
(`party.rs:583`, `party.rs:587`, `party.rs:602`, `party.rs:617`, `party.rs:684`, `party.rs:704`).
New-member poll: `DML_PARTY_POLL_TRIES` default 12 (`party.rs:321-323`),
`DML_PARTY_POLL_SLEEP` default 500 ms (`party.rs:327-333`); the poll loop is
`wait_new_member` (`party.rs:511-525`).

**Incident note.** Verbatim, `party.rs:271-276`:
> `/// The bot-members-of-a-party query shared BYTE-IDENTICALLY by `dismiss-all``
> `` /// (`90-main.sh:3189-3194`) and `preset-load`'s kick phase … ``
> `` /// Bot identity is [`crate::botid::bot_clause`] (registry OR reserved account ``
> `` /// prefix): with registry-only detection, a party full of bots read back as ``
> `` /// zero bots, so `dismiss-all` dismissed nobody and `preset-save` saved an ``
> `` /// empty preset. ``

And on the spec allowlist, `party.rs:62-66`:
> `` /// Deliberately wider than the shipped names' plain lowercase-and-spaces: ``
> `` /// playerbots.conf is hand-editable and the picker offers every conf name ``
> `` /// verbatim, so a narrower rule here only produced specs the UI offered and ``
> `` /// this refused. ``

And, on the DB-error swallow asymmetry, `party.rs:473-479`:
> `` /// Unlike `party_online_guid`/`group_member_guids`/`char_name_by_guid` ``
> `` /// (which swallow query failure exactly like the oracle's own `2>/dev/null` ``
> `` /// helpers do), a failure here MUST surface: `dismiss-all`'s bash caller ``
> `` /// explicitly checks this query and exits `DB_UNREACHABLE "Could not read ``
> `` /// the party"` on failure … rather than treating an unreachable DB as "zero bots" ``

**Emulator family.** AzerothCore + mod-playerbots + mod-ale. Class ids are the WotLK 9-class set
with **deathknight (6) deliberately excluded** (`party_specs.rs:18-31`, `party.rs:52-59`).

---

### Bridge setup (the prerequisite My Party and GM tools both need)
**What it does.** `bridge::bridge_setup_stream` (`bridge.rs:141-226`): SOAP `server info`
preflight → copy every `*.lua` under `<parent of DML_SCRIPT>/lua/<family>/` into
`<server dir>/env/dist/etc/modules/lua_scripts/` → repair `mod_ale.conf`.

**Server-side mechanism.**
- SOAP, verbatim command text `server info` (`bridge.rs:157`) — preflight only.
- Filesystem: source `<parent of DML_SCRIPT>/lua` (`bridge.rs:29-41`), destination
  `<server dir>/env/dist/etc/modules/lua_scripts` (`bridge.rs:46-48`). The six deployed scripts
  are `cli/lua/gm/dml_gm.lua`, `cli/lua/gm/dml_summon_npc.lua`, `cli/lua/party/dml_addclass.lua`,
  `cli/lua/party/dml_login.lua`, `cli/lua/party/dml_uninvite.lua`, `cli/lua/party/dml_whisper.lua`.
- Conf write: `mod_ale.conf` keys `ALE.Enabled` and `ALE.ScriptPath` (`bridge.rs:250-264`).

**Threading.** The preflight `server info` is taken under `soap_lock` (`bridge.rs:155`).

**Incident note.** Two, verbatim. `bridge.rs:189-195`:
> `// Deployed scripts ALE never reads are the silent-bridge bug (found live`
> `` // on Ubuntu 2026-08-20): mod_ale.conf ships `ALE.ScriptPath = ``
> `` // "lua_scripts"`, a RELATIVE path the worldserver resolves against its ``
> `` // cwd (/azerothcore), where nothing is -- so every bridge command answers ``
> `` // "Command does not exist" while the deploy reports success. ``

`bridge.rs:60-65`:
> `` /// This used to mirror the bash's `for d in "$root"/*/` glob silently matching ``
> `` /// nothing on an absent dir. That faithfulness cost more than it bought: with ``
> `` /// `DML_SCRIPT` unset, `lua_root_from_env` resolves to a CWD-relative `"lua"` ``
> `` /// that does not exist, so `bridge_setup_stream` emitted ``
> `` /// `done{changed:false}` -- a SUCCESS envelope for a no-op. "Enable My Party" ``
> `` /// then appeared to work and My Party simply did not function, with nothing ``
> `` /// pointing at the cause. ``

And `bridge.rs:365-367` (test docstring):
> `/// A disabled engine is the OTHER half of the silent-bridge bug: ALE's`
> `` /// compiled-in default for `ALE.Enabled` is FALSE (the conf's own comment ``
> `/// claims true — the code wins), so a conf missing the key entirely runs`
> `/// no scripts at all.`

**Emulator family.** AzerothCore + mod-ale (an AzerothCore Lua engine). Not portable to
CMaNGOS/Eluna as written.

---

### Browse Bots
**What it does.** `pages::read_bots` (`pages.rs:249-263`) — one `COUNT(*)` plus one page of rows.
Favourites/starring is not in the Rust surface (frontend).

**Server-side mechanism.**
- MySQL read, `<characters>.characters`:
  `SELECT COUNT(*) FROM characters c WHERE {bots_where};` (`pages.rs:202`) and
  `SELECT c.guid, c.name, c.class, c.race, c.gender, c.level, c.online, c.zone FROM characters c
  WHERE {bots_where} ORDER BY c.name LIMIT {limit} OFFSET {offset};` (`pages.rs:209-213`).
- `bots_where` = the `botid::bot_clause` plus optional filters (`pages.rs:173-196`). Name filter
  binds a canonicalised prefix with `%`/`_` escaped and `ESCAPE '!'` declared (`pages.rs:179-181`).
- Limit is clamped to `1..=200`, default 50 (`pages.rs:143-145`).

**Incident note.** Verbatim, `pages.rs:167-171` (on the `ESCAPE '!'` choice):
> `` /// the bash does — that escaping is LIKE-pattern semantics (stopping a literal ``
> `` /// `%`/`_` in the name from acting as a wildcard), not SQL-string escaping, so ``
> `` /// it still applies even though the value itself is bound rather than spliced ``
> `` /// as a literal. ``

**Emulator family.** AzerothCore. Valid class ids `[1,2,3,4,5,6,7,8,9,11]` with a note that
*"10 has never shipped a class"* (`pages.rs:147-153`).

---

## Server customization

### 38 server modules
**What it does.** Install / update / remove per family (cpp, lua, sql), a "check for updates"
commits-behind read, conf activation, place-NPC, client-patch.

**The 38 count checks out**: 19 cpp + 9 lua + 10 sql, pinned by
`registry.rs:105-107` (`families["cpp"] == 19`, `lua == 9`, `sql == 10`). The cpp registry is
listed verbatim at `modmgr.rs:100-120`.

**Server-side mechanism.**
- `git` CLI: `["clone","--depth","1",<url>]` + dest (`modmgr.rs:530`); local reads via
  `git -C <dir> <args>` (`modmgr.rs:350-420`); `git fetch --quiet origin` (`maint.rs:438`);
  advance is either `git pull --ff-only` or `git fetch [--depth 1] origin <branch>` +
  `git checkout --detach FETCH_HEAD` (`modmgr.rs:1542-1551`).
- `docker exec` (statement form): `["exec","ac-database","mysql","-uroot","-p<pw>",<db>,"-e",<stmt>]`
  (`modmgr.rs:560`).
- `docker exec` (file form): `["exec","-i","ac-database","mysql","-uroot","-p<pw>",<db>]` with the
  `.sql` file's bytes on stdin (`modmgr.rs:634`).
- `docker compose` build guard: `["compose", <build -f set>, "config","--format","json"]`
  (`modmgr.rs:2311-2313`).
- Rebuild: `["compose", <build -f set>, "build","ac-worldserver","ac-db-import"]`
  (`modmgr.rs:2794-2798`), preceded by `["compose","stop","-t","180","ac-worldserver"]`
  (`modmgr.rs:2783`) and followed by `["compose","up","-d"]` (`modmgr.rs:2823`).
- `docker run` (mod-arac client patch): `["run","--rm","-v",<vol>:/data,"-v",<file>:/src/<bn>:ro,
  "alpine","cp","/src/<bn>","/data/dbc/<bn>"]` (`moduletail.rs:476`), preceded by
  `["inspect","ac-worldserver","--format","{{range .Mounts}}{{if eq .Destination
  \"/azerothcore/env/dist/data\"}}{{.Name}}{{end}}{{end}}"]` (`moduletail.rs:451-456`).
- MySQL **write** (module tracking repair), `<world|characters|auth>.updates`:
  `INSERT INTO updates (name, hash, state, timestamp, speed) VALUES (?, ?, 'RELEASED', NOW(), 0)
  ON DUPLICATE KEY UPDATE hash=?, state='RELEASED'` (`moduletail.rs:240-242`) and
  `DELETE FROM updates WHERE name=?` (`moduletail.rs:244`), fired `moduletail.rs:674` /
  `moduletail.rs:688`. Called *"the FOURTH sanctioned direct MySQL write"* (`moduletail.rs:621`).
- MySQL **write** (place-npc / fixit), `<world>.creature`:
  `INSERT INTO creature (id, map, position_x, position_y, position_z, orientation, spawntimesecs)
  VALUES (?, ?, ?, ?, ?, ?, 300)` (`moduletail.rs:313-315`); guarded by
  `SELECT COUNT(*) FROM creature_template WHERE entry=?` (`moduletail.rs:311`) and
  `SELECT COUNT(*) FROM creature WHERE id=? AND map=?` (`moduletail.rs:312`).
- MySQL **write** (Battle Pass fixit), multi-statement, run via `mysql_run_stmt`: the
  `creature_template` INSERT for entry 90100 (`moduletail.rs:323-330`) and four schema-adaptive
  `SET/PREPARE/EXECUTE` blocks (`moduletail.rs:333-357`). Capitals hard-coded at
  `moduletail.rs:361-362`: `(0, -8819.3, 636.2, 94.1, 3.7)` and `(1, 1609.2, -4407.7, 17.5, 4.5)`.
- SOAP: **none** on the module paths.

**Threading.** `GIT_PROBE_TIMEOUT = 5s` (`modmgr.rs:69`), `GIT_NET_TIMEOUT = 1200s`
(`modmgr.rs:74`), `SQL_APPLY_TIMEOUT = 600s` (`modmgr.rs:78`), compose-config guard 30 s
(`modmgr.rs:2317`), `docker run … alpine cp` 60 s (`moduletail.rs:479`), volume resolve 10 s
(`moduletail.rs:458`). The rebuild build step is **deliberately unbounded**
(`destructive.rs:33-43`, `run_streamed_unbounded`). SQL-file application uses a bespoke
stdin-feeding runner with three concurrent drain threads (`modmgr.rs:576-623`).

**Incident note.** Several, verbatim.
`modmgr.rs:22-36`:
> `` //! SUBPROCESS DISCIPLINE. Every `git`/`docker` call here goes through ``
> `` //! [`super::status::output_bounded_draining`] (never `crate::output_bounded` ``
> `//! — see that function's own doc comment for the pipe-deadlock hazard a`
> `` //! `git clone`/`docker exec … mysqldump` can trigger). SQL file application ``
> `` //! (the lua/sql families) pipes the `.sql` file's bytes to `docker exec -i ``
> `` //! ac-database mysql …` over real stdin — EXACTLY like the bash — rather ``
> `` //! than the `mysql` crate: a module's `.sql` file can carry multiple ``
> `` //! statements (and, upstream, `DELIMITER` blocks), which the crate's ``
> `` //! single-statement `exec_drop` cannot run safely. ``

`modmgr.rs:38-44`:
> `` //! TWO DIFFERENT EXECUTABLES. `git` … and the resolved Docker Desktop `docker` ``
> `` //! binary … are NEVER the same handle … named ``
> `` //! `git_program`/`docker_program` (never a shared `program`) so a caller ``
> `` //! can't accidentally hand a `docker` path to a git-only function or vice versa. ``

`modmgr.rs:2796-2797`:
> `// db-import must rebuild WITH the worldserver, or the updater keeps serving`
> `// install-time module SQL (spec 2026-08-10; the VM ledger evidence).`

`modmgr.rs:377-379` (on `git diff --binary HEAD`):
> `` /// `HEAD` (not a bare `git diff`) is load-bearing: a bare `git diff` shows the ``
> `` /// worktree against the INDEX, so an edit the user had already `git add`ed was … ``

`destructive.rs:26-37`:
> `` //! `module rebuild` is different on purpose: since 2026-08-09 it ``
> `` //! builds through the explicit overlay set (`docker compose -f ``
> `` //! docker-compose.yml -f docker-compose.override.yml -f ``
> `` //! docker-compose.build.yml build ac-worldserver`, streamed here with `pct` ``
> `` //! progress from ninja's step counter) and only then runs a plain `docker ``
> `` //! compose up -d` -- never a bare `up -d --build`, which on a composegen ``
> `//! server carries no `-f` for the build overlay and would silently compile nothing.`

**Emulator family.** AzerothCore only, and the module subsystem **deliberately refuses to honour
renamed schemas**. Verbatim, `modmgr.rs:549-557`:
> `/// TASK 6 SCOPE RULING — the module subsystem is a recorded EXCEPTION to`
> `` /// resolved-name splicing, not a target. Every `db` argument at this ``
> `` /// executor's call sites stays a standard `acore_*` literal on purpose: the ``
> `` /// SQL payloads this runs are third-party module `.sql` files … whose OWN CONTENT ``
> `/// hardcodes the standard schema names internally … The module subsystem`
> `/// requires standard names end-to-end.`

Same ruling for the Battle Pass literals (`moduletail.rs:302-308`, which spell
`TABLE_SCHEMA='acore_world'` and `acore_world.creature_template` verbatim in the SQL).
Also hard-coded: the fallback client-data volume name
`"wow-server-playerbots_ac-client-data"` (`moduletail.rs:442`) and the in-container path
`/azerothcore/env/dist/data` (`moduletail.rs:455`).

---

### Settings with guardrails
**What it does.** A static 66-row config registry (`registry.rs:83-85` pins the count) whose
values are read live off the runtime conf files and written back through two routes:
`config_set_direct` (Route A, `config.rs:865-946`) for `conf:` keys, and `config_set_curated`
(Route B, `config.rs:949+`, **not read**) for the registry rows.

**Server-side mechanism.**
- Client's files: no. Server's conf files: `env/dist/etc/*.conf` and
  `env/dist/etc/modules/*.conf` (`config::conf_path_in`, cited from `tuning.rs:622-623`).
- SOAP, verbatim command text `server set motd 1 enUS {text}` for the MOTD key
  (`soap_cmds.rs:293`), mapper `motd_result` (`soap_cmds.rs:362-383`).
- SOAP, verbatim command text `transmog reload` — the ONLY live-reload command known
  (`config.rs:689-694`), fired at `config.rs:938` under `soap_lock` (`config.rs:936`).
- `docker`: only indirectly (a changed conf sets `restart_required`).

**Threading.** `config_lock` (`Arc<Mutex<()>>`) serialises every native conf/override write
(`tuning.rs:513-515`: *"Serializes against every other native conf/override write"*);
`soap_lock` guards the reload fire (`config.rs:936`).

**Incident note.** `config.rs:687-688`, verbatim:
> `` /// Deliberately tiny: only `transmog.conf` has a ``
> `/// verified reload command; everything else stays restart-to-apply.`

**Emulator family.** AzerothCore (`server set motd`, `transmog reload`, `AC_*` legacy env keys
at `config.rs:921-923`).

---

### Module tuning
**What it does.** 13 curated rows (`registry.rs:88-90` pins the count), each with a `conf` or
`lua` backend. `TuningReader::compute` (`tuning.rs:421-449`) reads the live value; `tuning_set`
(`tuning.rs:507-657`) writes it.

**Server-side mechanism.**
- Server's conf files (`conf` backend): `env/dist/etc/…/<file>` via `config::conf_path_in`
  (`tuning.rs:623`), written with `config::conf_write` (`tuning.rs:633`).
- Server's Lua script files (`lua` backend): the DEPLOYED ALE script at
  `<title>/env/dist/etc/modules/lua_scripts/<file>` (`tuning.rs:303-311`), line-replaced by
  `lua_cfg_write` (`tuning.rs:279-298`).
- SOAP: **none on this path.** The `lua` backend returns
  `"apply_needed": "none"` and the advisory string
  `".reload ale (Console page) or restart the server to apply"` (`tuning.rs:315`,
  emitted `tuning.rs:590`) — i.e. the user is TOLD to reload, the launcher does not do it.
- The `conf` backend returns `"restart_required": true, "applied": "restart",
  "apply_needed": "world-restart"` (`tuning.rs:636-649`) — **no live reload at all**.

**Threading.** `config_lock` held for the whole `tuning_set` (`tuning.rs:515`).

**Incident note.** Verbatim, `tuning.rs:270-273` (the re-verify step):
> `/// 3. patch, then RE-VERIFY by reading the patched text back — a patch that`
> `/// doesn't read back as the requested value is discarded`
> `` /// ([`LuaWrite::Failed`]) rather than applied, so a bad edit can never ``
> `/// truncate or corrupt the live script;`

And, on why the lua write became native, `tuning.rs:135-142`:
> `// Before this, the lua branch of [`tuning_set`] shelled the bash CLI through a`
> `` // `DmlRunner`, which contradicted the workspace plan's central promise … ``
> `// and it was FIVE of the 13 embedded tuning rows, not the 2 the inherited TODO claimed`

**Emulator family.** AzerothCore modules; the 13 confkeys are named at `tuning.rs:39-51`
(BeastMaster.*, LearnSpells.*, UnlimitedAmmoNamespace.*, DURATION, REGEN_AURA).

---

### Account-wide sharing
**What it does.** `accountwide.rs` reads/writes 14 `ENABLE_*` boolean flags in the DEPLOYED
accountwide Lua files.

**Server-side mechanism.**
- Server's Lua script files only:
  `<server dir>/env/dist/etc/modules/lua_scripts/accountwide/<file>` (`accountwide.rs:66-68`),
  files `AccountAchievements.lua`, `AccountCurrency.lua`, `AccountMoney.lua`, `AccountMounts.lua`,
  `AccountPets.lua`, `AccountPlaytime.lua`, `AccountProfessions.lua`, `AccountPvPRank.lua`,
  `AccountTaxiPaths.lua`, `AccountTitles.lua` (`accountwide.rs:40-53`).
- SOAP / MySQL / docker: **none** (`accountwide.rs:8`: *"Both verbs are plain JSON, no
  streaming."*). The reply carries the hint `".reload ale (Console page) or restart the server to
  apply"` (`accountwide.rs:62`) — again, told, not done.

**Incident note.** none recorded (in lines 1–150). There is a design note,
`accountwide.rs:4-8`:
> `` //! reads/writes the `ENABLE_*` flags in the DEPLOYED accountwide lua files … ``
> `//! never the repo (that family clones the upstream repo fresh at install time and`
> `//! copies the lua in).`

**Emulator family.** AzerothCore + mod-ale, and specifically the upstream
`Aldori15/azerothcore-eluna-accountwide` HEAD (`accountwide.rs:26-27`). One row carries a
compatibility warning verbatim (`accountwide.rs:52`):
> `"Share discovered flight points per faction. Needs a special mod-ale fork - may not work on this server."`

---

### Config editor
**What it does.** `wow_config_files_native`, `wow_config_raw_read_native`,
`wow_config_raw_write_native`, `wow_config_raw_reset_native` (Tauri names from the `lib.rs`
grep). Implementation is in `config.rs` — **not read** beyond lines 686–700 and 855–946.
**Could not establish** what the automatic backups are (files? `.bak` siblings?).

---

### Accounts
**What it does.** List, create, set password, set GM level, delete.

**Server-side mechanism.**
- MySQL read (list), fired on the Characters connection with fully-qualified auth names:
  `SELECT a.id, a.username, COALESCE(g.gmlevel,0), COALESCE(c.guid,''), COALESCE(c.name,''),
  COALESCE(c.level,'') FROM {auth}.account a LEFT JOIN (SELECT id, MAX(gmlevel) AS gmlevel FROM
  {auth}.account_access GROUP BY id) g ON g.id = a.id LEFT JOIN characters c ON c.account = a.id
  WHERE NOT {username_is_bot} {registry_arm} AND a.username <> 'AHBOT' AND a.username <> 'DMLSOAP'
  AND a.username NOT LIKE 'DMLSOAP\_%' ORDER BY a.id, c.level DESC;` (`pages.rs:296-306`,
  fired `pages.rs:372-380`).
- SOAP, verbatim command texts:
  - `account create {user} {pass}` (`soap_cmds.rs:120`)
  - `account set password {user} {pass} {pass}` (`soap_cmds.rs:126`)
  - `account set gmlevel {user} {level} -1` (`soap_cmds.rs:134`)
  - `account delete {user}` (`soap_cmds.rs:145`)
  All four map through `account_result` (`soap_cmds.rs:460-469`); fire site
  `lib.rs:3901+` (`wow_account_create_native` and siblings).
- Deleting `admin` is refused outright (`soap_cmds.rs:139-144`).

**Incident note.** Verbatim, `pages.rs:281-287`:
> `` /// `Some(pb)` ADDS the registry exclusion (2026-08-10): mod-city-bots' ``
> `` /// 400 `citybot*` citizens carry registry type 3 but no rndbot prefix, and ``
> `/// they buried the picker (403 rows, 3 of them real on the live VM).`

**Emulator family.** AzerothCore (`account_access.RealmID`/`gmlevel`, the `account …` GM commands,
realm `-1`).

---

### Backups
**What it does.** Create (streamed), list, validate, delete, restore (streamed). Plus two
automatic dumps: a pre-stop safety dump and a 6-hourly interval dump.

**Server-side mechanism.**
- `docker exec` (create): `["exec", <container>, "mysqldump", "-uroot", "-p<pw>", "--databases",
  <characters>, [<playerbots>], <auth>, [<world> when include_world], "--single-transaction",
  "--quick"]` (`backup.rs:711-729`), stdout gzip-encoded straight to a `.tmp` sibling
  (`backup.rs:844-857`), then renamed (`backup.rs:916`).
- `docker exec` (restore): `["exec","-i","ac-database","mysql","-uroot","-p<pw>"]` — no trailing
  database name (`restore.rs:90-92`), with a `GzDecoder` streamed into its stdin in 64 KiB chunks
  (`restore.rs:199-217`).
- SOAP, verbatim command text `saveall`, fired before the restore's stop (`restore.rs:316`) under
  `soap_lock` (`restore.rs:314`).
- `docker compose` (restore): `["compose","stop","-t","180","ac-worldserver","ac-authserver"]`
  (`restore.rs:319`) and `["compose","start","ac-worldserver","ac-authserver"]`
  (`restore.rs:334`, `restore.rs:360`).
- MySQL reads (the `.meta` summary sidecar): `SELECT COUNT(*) FROM characters;`
  (`backup.rs:375`), `SELECT COUNT(*) FROM account;` (`backup.rs:376`),
  `SELECT COUNT(*) FROM characters WHERE <bot_clause>;` (`backup.rs:383-390`).
- Files: `~/.dml/backups/wow-<YYYYMMDD-HHMMSS>[-full][-prerestore].sql.gz` + a `.meta` sidecar
  (`backup.rs:73-75`, `backup.rs:106-129`).
- Validation is entirely local: gzip decompress + byte-scan for
  `CREATE TABLE \`characters\`` and `CREATE TABLE \`account\`` (`backup.rs:946-947`,
  `backup.rs:981-1003`). No server contact at all.

**Threading.** `DUMP_TIMEOUT = 1800s` (`backup.rs:657`); the dump drains stderr on its own thread
into a 64 KiB tail buffer (`backup.rs:755`, `backup.rs:828-838`) while the main thread polls
`try_wait()` and kills on overrun (`backup.rs:866-887`). The restore is **deliberately
unbounded** — verbatim `restore.rs:36-44`:
> `//! DELIBERATELY UNBOUNDED, LIKE THE MODULE-REBUILD BUILD STREAM … the bash oracle puts NO`
> `` //! wall-clock timeout on this pipeline … Killing it on an arbitrary clock would ``
> `//! abort a restore that was still making real, unrecoverable progress against`
> `//! a database the caller has already been told is "LEFT STOPPED" pending this`
> `//! exact import finishing … no `kill()` anywhere in this module.`

Retention: `DML_BACKUP_KEEP` default 10 (`backup.rs:80-82`); interval backup every 6 h
(`backup.rs:559`) with a 30-minute check tick (`backup.rs:563`).

**Incident note.** Verbatim, `backup.rs:14-22`:
> `` //! `dump_to` STREAMS, IT NEVER BUFFERS THE WHOLE DUMP. A `--include-world` ``
> `` //! dump can be many GB of stdout — [`dump_to`] is also the pre-restore SAFETY ``
> `` //! dump inside `wow_backup_restore_native` (`lib.rs`), so an earlier ``
> `` //! fully-buffered `Vec<u8>` capture … meant an OOM here could kill the whole Tauri process while ``
> `//! the game server was stopped, with no recovery.`

`backup.rs:41-57` (the key-order trap):
> `` //! ON-DISK FORMAT IS SHARED WITH WSL … `serde_json`'s `Map` is ``
> `` //! NOT built with the `preserve_order` feature in this workspace … so ``
> `` //! serializing a `json!({"characters":...})` value would emit keys in **alphabetical** order … ``
> `` //! bytes [`read_summary`]'s own regex-equivalent parser … would then reject, silently degrading ``
> `//! every native-written sidecar to `null`.`

`backup.rs:660-663` (the worst failure class):
> `` /// over the RESOLVED schema names (Task 6): a renamed server used to get a dump of the WRONG ``
> `` /// (absent) `acore_*` schemas — the worst failure class this repo records, ``
> `/// because the backup reports success and holds nothing.`

`restore.rs:25-34` (the bidirectional deadlock):
> `//! THE CLASSIC PIPE DEADLOCK THIS AVOIDS: if the parent only writes stdin and`
> `//! only reads stdout/stderr AFTER the child exits … a chatty child (mysql echoing warnings …`
> `//! or docker itself buffering exec-attach frames) can fill its stdout/stderr`
> `//! pipe and block, while the parent is simultaneously blocked writing more`
> `//! stdin the child will never come back to drain — classic bidirectional deadlock.`

**Emulator family.** AzerothCore. Hard-coded container name `ac-database` (`backup.rs:665`,
`restore.rs:91`), MySQL root user, and the two validation markers are AC table names.

---

### Self-updating
**What it does.** `maint::update_stream` (`maint.rs:757+`) pulls the core and the playerbots
module, with a rollback path.

**Server-side mechanism.**
- `git` CLI: `git -C <dir> <args>` (`maint.rs:575`), including
  `git -C <dir> fetch --quiet origin` (`maint.rs:438`) for the commits-behind read, and
  `git checkout --detach <sha>` for rollback (named in the operator hint at `maint.rs:746`).
- **Remote allowlist**: the core origin must contain `mod-playerbots/azerothcore-wotlk`
  (`maint.rs:781`) and the module origin `mod-playerbots/mod-playerbots` (`maint.rs:798`) — a
  hard error otherwise, no override.
- Docker disk cleanup: `destructive::docker_clean_stream` (`destructive.rs:624`), which runs
  `["compose","up","-d","ac-database"]` (`destructive.rs:655`),
  `["compose","stop","-t","180","ac-worldserver"]` (`destructive.rs:663`),
  `["volume","ls","--format","{{.Name}}"]` (`destructive.rs:684`), `["volume","rm",<vol>]`
  (`destructive.rs:696`), `["image","rm",<img>]` (`destructive.rs:580`).
- `unbound` core patch: `git apply --check --verbose <patch>` (`maint.rs:1408`) and
  a `curl -sfL <url> -o <path>` download (`maint.rs:1400`).

**Incident note.** Verbatim, `maint.rs:777-779`:
> `// AzerothCore must be the custom mod-playerbots fork on the Playerbot`
> `// branch -- pulling upstream azerothcore/azerothcore-wotlk here would`
> `// break the playerbots integration. No override: hard error.`

And `maint.rs:751-753`:
> `` "Wrath Unbound's core patch is NOT applied. Do NOT rebuild -- a rebuild now would produce a `` 
> `worldserver without Unbound while its database still has it."`

**Emulator family.** AzerothCore + the `mod-playerbots` fork specifically — the remote check is a
hard gate, not a preference.

---

## Under the hood

### One frontend, not the only one / `dml-wow` CLI with 74 subcommands
`crates/dml-wow-cli/src/cli.rs` and `run.rs` exist on this branch (seen in greps at
`crates/dml-wow-cli/src/cli.rs:241`, `run.rs:647`). I did **not** count the subcommands or read
`docs/cli-contract.md`. **Could not establish** the "74" figure.

---

# Part 2 — `pyplan/rust-prior-art.md` §7 items not already covered above

### `backup.rs` streamed create + list/validate/delete, `restore.rs`
Covered under **Backups** above.

### `registry.rs` — embedded static registries
**What it does.** Three JSON snapshots compiled into the binary with `include_str!`
(`registry.rs:29-31`): `data/config-registry.json` (66 rows, pinned `registry.rs:83-85`),
`data/tuning-registry.json` (13 rows, `registry.rs:88-90`), `data/module-catalog.json`
(19/9/10 families, `registry.rs:102-108`).

**Server-side mechanism.** None — pure `include_str!`, no I/O of any kind.

**Incident note.** Verbatim, `registry.rs:5-9`:
> `//! Native mode used to shell the bash `dml` CLI once per session to fetch`
> `//! three STATIC registries … None of these three ever changes at runtime: they are hand-authored`
> `//! catalogs of what settings/modules exist, not values. Baking them into the`
> `//! binary removes the last bash dependency from the native read path.`

And the count-pin rationale, `registry.rs:78-81`:
> `` // Pins the exact row count so an unnoticed add/remove in the bash ``
> `` // oracle's `_cfg_rows` heredoc … fails HERE, on ``
> `` // every machine, rather than only in `config_parity.rs`, which SKIPS on ``
> `` // any box without the native runtime at C:/Users/perzi/dml-native. ``

### `status.rs` composite — covered under **Live server dashboard**.

### `logsnap.rs` — pre-stop worldserver log snapshot
**What it does.** Before every stop/restart, resolve THIS title's world container through its own
compose project, read a bounded tail of its log, write it to `~/.dml/logs`, prune.

**Server-side mechanism.**
- `docker compose` argv `["compose","ps","-a","-q","ac-worldserver"]`, run in the stopping
  title's own compose dir (`logsnap.rs:277-279`, spawned `logsnap.rs:298-306`). The returned
  id is charset-validated before it reaches the next argv (`logsnap.rs:285-292`).
- `docker` argv `["logs","--tail","2000",<container id>]` (`logsnap.rs:327`), stdout+stderr
  merged (`logsnap.rs:331-332`).
- Files: `~/.dml/logs/world-<YYYYMMDD-HHMMSS>-<title>-<mode>.log` (`logsnap.rs:150-153`).

**Threading.** `SNAPSHOT_TAIL_LINES = 2000` (`logsnap.rs:52`), `SNAPSHOT_MAX_BYTES = 2 MiB`
(`logsnap.rs:59`) applied AFTER the line cap, `SNAPSHOT_TIMEOUT_SECS = 20` overridable by
`DML_LOG_SNAPSHOT_TIMEOUT` (`logsnap.rs:67`, `logsnap.rs:125-132`), resolve timeout 10 s
(`logsnap.rs:72`), retention `DML_LOG_SNAPSHOT_KEEP` default 10 (`logsnap.rs:115-117`).

**Incident note.** Verbatim, `logsnap.rs:3-9`:
> `` //! WHY THIS EXISTS. `docker compose down` + `up -d` RECREATES the containers, ``
> `//! and a recreated container starts with an EMPTY log — everything the old`
> `//! worldserver printed is gone the moment the restart runs. During the`
> `//! 2026-07-21 freeze incident that destroyed the evidence TWICE: the restart`
> `//! the operator reached for to fix the freeze was also the thing that erased`
> `//! the reason for it.`

Second, `logsnap.rs:77-84`:
> `` /// WHY A SERVICE AND NOT A CONTAINER NAME. `docker logs ac-worldserver` ``
> `/// answers for whichever title happens to own that container, so stopping a`
> `/// non-WoW title while the WoW stack is up used to save the WoW world's log`
> `/// under the other title's name — and, because retention is one shared`
> `/// newest-N pool, evict the genuine WoW evidence this module exists to keep.`

Third, on the prune-exclusion, `logsnap.rs:183-190`:
> `` /// WHY `fresh` IS EXCLUDED FROM THE POOL. Retention is a plain descending NAME ``
> `/// sort, so a backwards clock jump (WSL2's clock lagging the host after a long`
> `/// sleep/resume) leaves newer-NAMED snapshots on disk and sorts the file just`
> `/// written to the BOTTOM — pruning it on the spot while the caller reports`
> `/// "snapshot saved: <name>" for a file that is already gone.`

Fourth, on the silent-prune-lines change, `logsnap.rs:254-261`:
> `` /// PRUNES ARE SILENT, on both surfaces. The native path used to emit one ``
> `` /// `pruned old log snapshot: <name>` info line per deleted file, which bash ``
> `/// never did — the same stop narrated differently depending on the backend …`

Fifth, on the "not read where it is decided" trap, `logsnap.rs:375-380`:
> `` // There is deliberately NO `snapshot_world_log(program, compose_dir, …)` ``
> `` // wrapper that resolves `~/.dml/logs` + `DML_LOG_SNAPSHOT_KEEP` internally: ``
> `` // the live caller (`lifecycle::games_lifecycle_stream_with`) resolves both into ``
> `` // its `LifecycleEnv` and passes them here, which is what lets the pre-stop ``
> `// ORDER be asserted against the real call site with temp directories instead of`
> `// the operator's actual snapshot pool (round-2 finding G17).`

**Emulator family.** AzerothCore — `WORLD_SERVICE = "ac-worldserver"` (`logsnap.rs:85`).

### `modmgr.rs` / `moduletail.rs` — covered under **38 server modules**.

### `watch.rs` / `power.rs` / `single_instance.rs` / `realmlist.rs` / `autostart.rs`

**`power.rs` — keep-awake.** `SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED)`
(`power.rs:35-41`), bound with a hand-written `extern "system"` declaration against kernel32
(`power.rs:23-26`). No-op on non-Windows (`power.rs:59-62`). Mechanism: none of the four
(OS API).
**Threading.** Verbatim, `power.rs:7-13`:
> `//! ES_CONTINUOUS state is PER-THREAD: it lives and dies with the thread that`
> `//! set it. Tauri commands run on pool threads that can be recycled, which`
> `//! would silently drop the request -- so all calls are funneled through one`
> `//! dedicated, long-lived manager thread that owns the state.`

**`single_instance.rs` — dependency-free guard.** Binds `127.0.0.1:51789` (`single_instance.rs:19`);
a second launch connects, exchanges the handshake `b"dml-launcher-1\n"`
(`single_instance.rs:28`), and exits. Three outcomes, never two (`single_instance.rs:33-40`).
Mechanism: none of the four (loopback TCP).
**Threading.** The poke retries for 4 s with a 250 ms gap (`single_instance.rs:64-69`); read
timeouts 400 ms (client, `single_instance.rs:79`) / 500 ms (server, `single_instance.rs:100`);
one short-lived thread per accepted connection (`single_instance.rs:99`).
**Incident note.** Verbatim, `single_instance.rs:53-62`:
> `/// The retry is load-bearing, not politeness. The listener only starts`
> `` /// accepting inside Tauri's `.setup()`, hundreds of milliseconds after the ``
> `/// first instance binds the port. A second launch during that window`
> `/// completes its TCP connect against the kernel backlog, gets no reply`
> `/// because nobody is accepting yet, and would conclude "stranger on the`
> `/// port" — starting a second full app, which is exactly what this guard`
> `/// exists to prevent.`
And `single_instance.rs:22-27`:
> `/// The port sits in Windows' dynamic range (49152-65535), so an unrelated`
> `/// process can hold it. Without this check that stranger would make the`
> `/// launcher exit silently on every double-click — indistinguishable from a`
> `/// broken install, and permanent until reboot.`

**`realmlist.rs` — realmlist check + fix.** Reads BOTH `Data/<locale>/realmlist.wtf`
(`realmlist.rs:194-216`) and `WTF/Config.wtf` (`realmlist.rs:230` = `L31` of the 200+ slice);
"matches" is computed on the effective value, realmlist.wtf first (`realmlist.rs:245`).
The fix writes ONLY `realmlist.wtf`, as `set realmlist {target}\r\n` (`realmlist.rs:299`).
A read-only file is unlocked, written, re-locked (`realmlist.rs:296-302`).
**Server-side mechanism.** The client's files: `<client>/Data/<locale>/realmlist.wtf` (write) and
`<client>/WTF/Config.wtf` (read only). The client path is obtained by shelling the bash CLI:
`runner.run_json(&["wow","client-path","get"])` (`realmlist.rs:183`) and translated from
`/mnt/c/...` WSL form (`realmlist.rs:39-54`).
**Incident note.** Verbatim, `realmlist.rs:3-11`:
> `//! The WoW 3.3.5 client picks its login server from`
> `` //! `Data/<locale>/realmlist.wtf` (`set realmlist <host>`); when that file is ``
> `` //! empty the client falls back to the `SET realmList "<host>"` CVar it last ``
> `` //! persisted into `WTF/Config.wtf` (verified on the user's real client: an ``
> `//! empty realmlist.wtf + the effective pointer living in Config.wtf). Status`
> `//! therefore reports BOTH …`
And `realmlist.rs:25-27`:
> `/// Locale folders a 3.3.5 client can ship with. Matched case-insensitively:`
> `` /// real installs vary (the user's client has `Data/enus`). ``
**Emulator family.** WotLK 3.3.5 CLIENT (not server) — 12 locale folders listed at
`realmlist.rs:27-30`. Fix targets are restricted to loopback/RFC1918/hostname
(`realmlist.rs:114-122`).

**`autostart.rs` — start with Windows.** `reg.exe add|delete|query` against
`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`, value `"DML Launcher"`
(`autostart.rs:13-14`, argv at `autostart.rs:36`/`56`/`66`).
**Incident note.** Two, verbatim. `autostart.rs:28-35`:
> `` /// True when a Run entry exists AND still points at a file that exists. … ``
> `` /// dev and installed builds live at different paths … so an entry left behind by a deleted build is a ``
> `/// realistic state. Reporting that as "enabled" would show a toggle that is`
> `/// on while nothing actually starts.`
`autostart.rs:60-65`:
> `` /// QUOTED. The installers put this in `C:\Program Files\DML Launcher\`, ``
> `` /// and an unquoted path sends CreateProcess's space heuristic hunting ``
> `` /// `C:\Program.exe` and `C:\Program Files\DML.exe` first — if either ``
> `/// exists, autostart launches the wrong binary.`
And `autostart.rs:9-11`:
> `//! This is the repo's first registry WRITE; existing access is a read-only`
> `` //! `reg query` for WSL detection. ``

### `party.rs`/`party_specs.rs` — covered under **My Party**.
### `pages.rs`/`paperdoll.rs`/`stats.rs` — covered under **Character sheet**, **Browse Bots**,
**Accounts**, **Teleport**, **Item mail**; the Statistics page below.

**`stats.rs` — Statistics page.** 19 queries: one probe plus 18 run CONCURRENTLY, one thread and
one connection each (`stats.rs:527-548`, `run_concurrent` `stats.rs:554-572`). The full SQL of
all 18 is at `stats.rs:156-320`; databases touched are `<characters>` (16 of them — including
`guild`, `guild_member`, `auctionhouse`, `mail`, `character_achievement`,
`character_queststatus_rewarded`) and `<auth>` (3: `uptime` twice, `realmlist` once,
`stats.rs:284-298`). The bot-schema probe is
`SELECT 1 FROM {playerbots}.playerbots_account_type LIMIT 1;` (`stats.rs:46`).
**Incident note.** Verbatim, `stats.rs:59-63`:
> `` /// `probe_ok == false` used to mean `0=1` — "this box has no bots" — which is ``
> `/// a lie on any box that has them and silently moved every bot into the`
> `/// family totals.`
And `stats.rs:65-67`:
> `` /// (The registry-only `BOT_SUBQUERY` const that used to live here is GONE: ``
> `` /// it duplicated [`crate::botid::registry_clause`]'s text, and the one place ``
> `/// that decides what a bot is must stay one place.)`

### `iteminfo.rs` — item lookup + tooltip cache
**What it does.** Fetch a Wowhead tooltip JSON, its icon, and (items only) the 3D `displayId`
from Wowhead XML; fall back to a locally-built tooltip from `item_template` when Wowhead has
nothing.
**Server-side mechanism.** MySQL read, `<world>.item_template`, columns
`name,Quality,ItemLevel,armor,RequiredLevel,dmg_min1,dmg_max1,delay,stat_type1..5,stat_value1..5`
(`iteminfo.rs:196-197`). HTTPS to the three bases at `iteminfo.rs:53/57/61`. Disk cache at
`~/.dml/wowhead-cache/{tooltips,icons,xml}` (`iteminfo.rs:7-13`). **The exact query builder and
the cache-write path are in lines 200–518, which I did not read.**
**Threading.** "blocking, bounded ~10s — the CLI's `curl --max-time 10`" (`iteminfo.rs:6-7`).
**Incident note.** Verbatim, `iteminfo.rs:66-71`:
> `` /// `_iteminfo_one`/`_entity_one`'s poisoned-cache gate (`46-iteminfo.sh:134`, ``
> `` /// `:175`): a tooltip body is only trusted when it's brace-wrapped AND ``
> `` /// carries both a `"name":"` and a `"tooltip":"` key — an error body (or any ``
> `/// other non-tooltip JSON) fails this and gets dropped rather than embedded verbatim.`
**Emulator family.** AzerothCore `item_template`; Wowhead's WotLK ("nether") subdomain.

### `botid.rs` — covered under **~2000 AI bots**.
### `soap.rs` / `soap_autosetup.rs` / `account_write.rs` — the SOAP transport and its setup

**`soap.rs`.** Covered in the global note at the top. Additional facts:
- Credential resolution order: env `DML_SOAP_URL/USER/PASS` (non-empty) → `~/.dml/soap.env` →
  compiled-in `http://127.0.0.1:7878/` / `admin` / `admin` (`soap.rs:101-120`).
- Result text is NEVER entity-decoded on the way out (`soap.rs:11-16`).
- The bash file-lock is deliberately NOT ported: verbatim `soap.rs:236-242`:
  > `` /// … a port of `soap_exec` … minus the bash file-lock (that's a ``
  > `` /// same-process concurrency guard the Rust side doesn't need the same way; ``
  > `` /// later tasks may add one at the call site). Because reqwest's blocking ``
  > `` /// client must not run on the async runtime thread, callers wrap `exec` in ``
  > `` /// `spawn_blocking` — this function itself stays plain sync. ``
  The call-site lock is `AppState::soap_lock`, taken by every native SOAP command I read.
- **Incident note**, verbatim `soap.rs:64-69`:
  > `/// It has to be answered HERE because nowhere downstream can. The credential`
  > `` /// panel used to decide by comparing the resolved strings against `"admin"`, ``
  > `/// which cannot tell an install with no account at all from one whose SOAP`
  > `` /// account is genuinely named `admin` — so it either invented an account for ``
  > `/// a fresh install or denied a real one.`
- Second, verbatim `soap.rs:506-513`:
  > `` // NB there is deliberately no "load and load_with_provenance agree" test. … ``
  > `` // Measured: it failed on the first run, comparing a config ``
  > `` // resolved before that test's `set_var` against one resolved after. ``

**`account_write.rs` — the third sanctioned MySQL write.**
- MySQL read: `SELECT 1 FROM account WHERE username = ? LIMIT 1` (`account_write.rs:51`);
  `SELECT 1 FROM account WHERE username LIKE ? ESCAPE '!' LIMIT 1` (`account_write.rs:130`);
  `SELECT id FROM account WHERE username = ?` (`account_write.rs:179`).
- MySQL **write**, `<auth>.account`:
  `INSERT INTO account (username, salt, verifier) VALUES (?, ?, ?)` (`account_write.rs:160`).
- MySQL **write**, `<auth>.account_access`:
  `INSERT INTO account_access (id, gmlevel, RealmID) VALUES (?, ?, ?)` with `(id, 3, -1)`
  (`account_write.rs:199-200`).
- **Incident / rationale note**, verbatim `account_write.rs:4-16`:
  > `# THIS IS THE THIRD SANCTIONED WRITE INTO A DML-MANAGED MySQL DATABASE`
  > `//! The standing rule is that MySQL is read-only and mutations go over SOAP GM`
  > `` //! commands. Two exceptions already existed — `wow backup restore` and the LAN ``
  > `//! toggle's realmlist `UPDATE`. The user sanctioned this third one on`
  > `//! 2026-08-01 …`
  > `//! **SOAP cannot create the account that SOAP needs.** Every GM command travels`
  > `//! over SOAP, SOAP requires a GM-level-3 account, and a fresh AzerothCore has`
  > `` //! none. The only other route is the worldserver console, and `docker attach` ``
  > `//! REFUSES piped stdin against a TTY container ("stdin is not a terminal",`
  > `//! measured against a live Docker Desktop); dropping the tty makes attach accept`
  > `//! the pipe and never return. So the choice was a manual step forever, or this.`
- Second, verbatim `account_write.rs:64-73` (why `ESCAPE '!'` and not `'\\'`):
  > `` /// **`!` rather than `\`, because the `ESCAPE` argument is a string LITERAL and ``
  > `` /// MySQL parses it per `sql_mode`.** `ESCAPE '\\'` is one backslash under the ``
  > `` /// default mode and TWO characters under `NO_BACKSLASH_ESCAPES` … so the statement errors ``
  > `` /// out on a server whose only unusual property is a stricter `sql_mode`, taking ``
  > `/// the family guard down with it.`
- Third, verbatim `account_write.rs:173-175`:
  > `` // Read the id back rather than trusting LAST_INSERT_ID across a pooled ``
  > `` // connection: `execute` does not promise the same session for a follow-up ``
  > `// query, and granting GM on the WRONG id would be a real security bug.`
- The only proof that the SRP6 verifier is right is a live test, verbatim
  `account_write.rs:286-287`:
  > `/// This is the only test that can prove the SRP6 is right: every offline`
  > `/// check passes just as happily on a verifier the server will reject.`

**`soap_autosetup.rs`.** Password: 16 chars from a 70-symbol alphabet exactly matching
`valid_account_pass` (`soap_autosetup.rs:20-25`), rejection-sampled rather than `% 70`
(`soap_autosetup.rs:29-33`). Fallback name `dmlsoap_<6 hex>` (`soap_autosetup.rs:84-86`).
Max 3 verify attempts (`soap_autosetup.rs:105`). The verification is a real SOAP round-trip.
**Incident note**, verbatim `soap_autosetup.rs:116-120`:
> `` /// **`Pending` is why this is a state machine and not a function.** A create ``
> `/// that succeeded followed by a verify that failed must not leave the latch`
> `/// open: the next poll would create a SECOND account, and the one after that a`
> `/// third -- one row per tick into the user's auth database, forever.`
Second, verbatim `soap_autosetup.rs:154-160`:
> `/// **There is no contentless "already done" outcome, and that is deliberate.**`
> `/// One used to exist. It cost the manual fallback card: the frontend's record`
> `/// of how setup went lives in a module-level store, a webview reload wipes it,`
> `/// and a reloaded UI that asks again and hears only "already concluded" learns`
> `/// nothing — so the card never rendered again for the rest of the process, on`
> `/// the exact path where the launcher had FAILED to make the account and the`
> `/// card was the only thing left that worked.`
**Emulator family.** AzerothCore — the 4–16 password ceiling is called *"AzerothCore's ceiling,
not a preference"* (`soap_autosetup.rs:16`).

---

# Things FEATURES.md claims that the code does not do, or does differently

Each item: **CLAIM** (FEATURES.md, a claim) vs **CODE** (the fact).

1. **"Teleport — send any character to ~2000 named locations"** (`docs/FEATURES.md:40`).
   CODE: the query is `… FROM game_tele WHERE {where} ORDER BY name LIMIT 500;`
   (`pages.rs:86`). At most 500 rows ever come back, filtered by an optional
   `name LIKE '%<search>%'`. The Teleport page passes `search` straight through
   (`launcher/src/lib/pages/Teleport.svelte:87`) and does not paginate. Whether the server's
   `game_tele` holds ~2000 rows is a separate question the code does not answer; what the
   launcher *shows* is capped at 500.

2. **"Restarts … save all characters first (there's an optional toggle for a faster restart)"**
   (`FEATURES.md:12-15`). CODE: only the WORLD-ONLY restart
   (`lifecycle::world_restart_stream`) fires SOAP `saveall` (`lifecycle.rs:981`). The full
   start/stop/restart path has **no pre-stop saveall at all** on native. Verbatim,
   `lifecycle.rs:351-358`:
   > `` /// The GUI's "faster restart" (`--no-saveall`) info line on the native ``
   > `` /// raw-compose path. WSL threads `DML_SKIP_SAVEALL` into `dml-start.sh`, ``
   > `/// which then skips its own pre-stop SOAP `saveall` call; the native path`
   > `/// … none) so there is no separate pre-stop saveall step to skip in the first`
   > `/// place.`
   > `` pub const SKIP_SAVEALL_NOTE: &str = "faster-restart requested -- the native compose path has no separate pre-stop saveall to skip; the graceful `docker compose down` already saves characters on shutdown."; ``
   The claim is true for WSL mode and for the world-only restart; it is not true of the native
   full restart, which relies on `compose down`'s graceful shutdown.

3. **"Module tuning … Transmog changes even apply live, no restart"** (`FEATURES.md:79-81`,
   inside the Modules→**Tuning tab** paragraph). CODE: `tuning::tuning_set`'s `conf` backend
   never calls a reload command — it returns `"restart_required": true, "applied": "restart",
   "apply_needed": "world-restart"` unconditionally (`tuning.rs:636-649`). The live
   `transmog reload` SOAP fire lives in a DIFFERENT function, `config::config_set_direct`
   (`config.rs:931-943`), reached by `wow_config_set_native` (`launcher/src/lib/api.ts:952`),
   not by `wow_config_tuning_set_native` (`api.ts:1053`). So: live-apply exists, but on the raw
   `conf:` key route, not on the curated Tuning switches the sentence is attached to.
   `config.rs:687-688` states the scope: *"only `transmog.conf` has a verified reload command;
   everything else stays restart-to-apply."*

4. **"Lua script mods keep their simple curated switches … no raw script editing"**
   (`FEATURES.md:82-83`) — and, in the same paragraph, the implication that tuning applies.
   CODE: a lua-backend tuning write reports `"apply_needed": "none"` and returns the hint string
   `".reload ale (Console page) or restart the server to apply"` (`tuning.rs:315`, emitted
   `tuning.rs:590`). The launcher does **not** issue `.reload ale` itself. Same for account-wide
   sharing (`accountwide.rs:62`). The user is told to do it.

5. **"GM tools — revive, heal, set level or gold … summon a banker/auctioneer/repair bot"**
   (`FEATURES.md:43-46`). CODE: four of these are NOT AzerothCore GM commands. They are custom
   Eluna/mod-ale bridge commands the launcher deploys itself: `dml_gm_money`
   (`soap_cmds.rs:190`), `dml_gm_health` (`soap_cmds.rs:195`), `dml_gm_revive`
   (`soap_cmds.rs:200`), `dml_summon_npc` (`soap_cmds.rs:211`), backed by
   `cli/lua/gm/dml_gm.lua` and `cli/lua/gm/dml_summon_npc.lua`. Without mod-ale installed AND
   `mod_ale.conf` repaired AND the scripts deployed (`bridge.rs:141-226`), every one of them
   answers "Command does not exist" (`bridge.rs:189-194`). FEATURES.md never mentions this
   prerequisite. The fault hint the code emits says so: *"Deploy the server bridges
   (bridge-setup) and restart the server first."* (`soap_cmds.rs:399`).
   The same is true of the whole **My Party** feature (`dml_addclass`, `dml_uninvite`,
   `dml_whisper`, `dml_login`).

6. **"~2000 AI bots populate the world … (via Dad's MMO Lab + mod-playerbots)"** and
   **"all ~2500 world bots"** (`FEATURES.md:55-57`, `:64`). CODE: the number is not a constant
   anywhere in the Rust tree. It is whatever `AiPlayerbot.MaxRandomBots` says in the live
   `playerbots.conf` (`status.rs:603`, registry key `bots.population` at
   `crates/dml-wow/data/config-registry.json:224`). The two different figures in FEATURES.md
   (2000 and 2500) have no code counterpart.

7. **"the full achievement browser (1320 achievements with categories, points and earned
   dates)"** (`FEATURES.md:32-34`). CODE: the SERVER read returns only earned
   `{id, date}` pairs (`pages.rs:790-798`). Categories, points and the 1320 catalogue are a
   static frontend JSON, `launcher/src/lib/achievements-wotlk.json`; the page itself records a
   simplification, `launcher/src/lib/CharacterSheet.svelte:222`:
   > `` // full 1320. Faction filtering is a known simplification: the launcher ``

8. **"Gear sets — save a character's full outfit as a reusable preset"** (`FEATURES.md:51-54`).
   CODE: `localStorage` in the browser, key `dml.gearsets.v1`
   (`launcher/src/lib/gearsets.svelte.ts:36`). There is **no backend surface at all** —
   the file says so in its own header (`gearsets.svelte.ts:4`: *"no new backend surface"*), and
   no `#[tauri::command]` in `lib.rs` mentions gear sets. Consequence not stated in
   FEATURES.md: sets do not survive a cleared webview profile and are not shared between
   machines. Also `gearsets.svelte.ts:8`: *"enchants/gems are NOT carried"*.

9. **A leading-dot inconsistency inside GM tools.** `gm_level_cmd` emits
   `.character level {player} {level}` **with** a leading dot (`soap_cmds.rs:167`, pinned by
   the test at `soap_cmds.rs:833-834`), while `gm_at_login_cmd` emits
   `character {flag} {player}` **without** one (`soap_cmds.rs:178`, test `soap_cmds.rs:855-856`),
   as do `account …`, `teleport name …`, `send items …`, `server set motd …` and `server info`.
   Both are pinned by tests, so this is intentional-as-shipped, not a typo I found; but it is
   an asymmetry no comment explains, and it is invisible from FEATURES.md.

10. **"Self-updating — pull the latest AzerothCore + playerbots source"** (`FEATURES.md:106-107`).
    CODE: it will refuse unless the core remote contains `mod-playerbots/azerothcore-wotlk`
    (`maint.rs:781`) and the module remote contains `mod-playerbots/mod-playerbots`
    (`maint.rs:798`) — *"No override: hard error."* (`maint.rs:779`). It does not pull
    "the latest AzerothCore"; it pulls the latest of one specific fork.
    Related, from the prior-art page itself (`pyplan/rust-prior-art.md`, §6):
    *"Note the Rust clones plain `azerothcore/azerothcore-wotlk`; Yu'lon's catalog names the
    `mod-playerbots` fork — reconcile before pinning."* — that §6 line and `maint.rs:781` say
    opposite things about which remote the Rust side expects. I did not read
    `install_native.rs`, so I cannot say which of the two is right about the INSTALL path;
    `maint.rs:781` is unambiguous about the UPDATE path.

11. **"a rotatable 3D model wearing your equipment"** (`FEATURES.md:33`). CODE: the display id
    that drives it comes from a Wowhead XML fetch over the public internet
    (`iteminfo.rs:60-62`, `extract_positive_display_id` `iteminfo.rs:133-139`), not from the
    server. A `displayId="0"` (gems) never qualifies (`iteminfo.rs:130-132`). Offline, the
    feature degrades.

12. **"Game installer — install complete servers (WotLK Playerbots, Vanilla, TBC, MapleStory,
    RuneScape, Mu Online)"** (`FEATURES.md:19-21`). I did not read `install_native.rs` /
    `provision.rs`, so I cannot confirm or refute this. What I CAN say from the files I did
    read: every mechanism in `dml-wow` is AzerothCore-shaped and the one module that mentions
    another family says native mode never reaches it (`lan.rs:20-23`, quoted above). So if
    non-AC titles install, they do so through a path none of the §7 modules serves.
    **This is a could-not-ask, not a refutation.**

13. **Bot-party terminology.** FEATURES.md says the bots come "via … mod-playerbots"
    (`FEATURES.md:56`) and never mentions mod-ale. Every party and GM-bridge action in the code
    is a mod-ale Lua command, not a mod-playerbots one, and there is **no use of a
    `.playerbots` worldserver subcommand anywhere in `crates/` or `launcher/src-tauri/`**
    (verified by `git grep`). If Phase 8 assumes a `.playerbots` command surface exists here,
    it does not.

---

# Facts I could not establish

1. **`install_native.rs` (4198 lines) and `provision.rs` (1492 lines)** — the Game installer.
   I looked for: the catalog rows, the per-title install argv, whether non-AzerothCore titles
   (Vanilla, TBC, MapleStory, RuneScape, Mu Online) have real installers or catalog stubs, and
   which remote the AzerothCore install clones (see divergence #10 above). **Not read at all.**

2. **`launcher/src-tauri/src/lib.rs` (9456 lines)** — I enumerated the full
   `#[tauri::command]` name list and read five short regions. I looked for and did **not**
   establish: the auto-shutdown watcher loop's real cadence and its "server actually up"
   guard (`set_auto_shutdown`); the interval-backup watcher's wiring; `auto_backup_before_stop`;
   `wow_bots_flush` / `wow_bots_flush_native`; the `Unbound` install/uninstall commands;
   `wow_migrate_import`; the six private `fn *_result` copies `soap_cmds.rs:428-432` says still
   live there.

3. **`config.rs` (2387 lines)** — I read only `conf_reload_cmd` (686–700) and
   `config_set_direct` (855–946). I looked for and did not establish: `config_set_curated`
   (Route B, the 66-row registry write path, `config.rs:949+`), the `ahbot`/`bots.population`
   companion writes it mentions (`tuning.rs:500-502`), the `AC_*` legacy-env migration, and
   what "automatic backups" means for the Config editor.

4. **`modmgr.rs` (4439 lines)** — read in full only at 1–120, 536–660, 2295–2340, 2775–2845.
   I looked for and did not establish: the LUA and SQL registry rows (only the 19 cpp rows at
   `modmgr.rs:100-120` were read), `install_lua`/`install_sql`/`remove_*` bodies, the
   hearthstone-cd / npc-teleporter variant statements, `module_backup_now`, and
   `wow_advance_repo`'s protect-file mechanism.

5. **`accountwide.rs` 150–970, `lan.rs` 140–788, `ahbot.rs` 80–414, `iteminfo.rs` 200–518,
   `commands.rs` 45–624, `soap_autosetup.rs` 180–336, `stats.rs` 420–470.**
   Specifically: the LAN realmlist `UPDATE` statement text (I know it exists — named at
   `account_write.rs:7` — but never read it); the ahbot `SELECT guid, account …` and the
   `reload config` SOAP fire (both named at `ahbot.rs:9-12` but not read); the iteminfo
   query builder and cache-write path.

6. **`destructive.rs` 60–920, `maint.rs` 1–560/600–740/800–1750, `lifecycle.rs` 1–918/1090–2536.**
   Specifically: `compose_up_argv`/`compose_down_argv`/`compose_sequence_for_mode` (they live in
   `dml_core::compose`, re-exported at `lifecycle.rs:40-42`, and I did **not** open `dml-core`
   at all — so the exact `docker compose` argv for a plain start/stop is a gap);
   `bots_flush_stream` (`destructive.rs:837`); `docker_clean_stream`'s three levels;
   `maint::read_container_stats`'s argv (`maint.rs:174`).

7. **The "74 subcommands" figure** for `dml-wow-cli` (`FEATURES.md:113`). I did not open
   `crates/dml-wow-cli/src/cli.rs` beyond two grep hits, and did not read
   `docs/cli-contract.md`.

8. **Whether `AppState::soap_lock` is genuinely the single serialisation point for every SOAP
   call.** I verified the guard at eight call sites (`soap_cmds.rs:660`, `bridge.rs:155`,
   `restore.rs:314`, `config.rs:936`, `lifecycle.rs:976`, `party.rs:583/587/602/617/684/704`,
   `lib.rs:4049`, `lib.rs:4579`, `lib.rs:4639`). I did **not** audit all ~40 native SOAP
   commands in `lib.rs` for a missing guard, and `soap.rs:239-241` explicitly says the bash
   file-lock was NOT ported and *"later tasks may add one at the call site"* — so an unguarded
   call site is a live possibility I did not rule out.

9. **Whether `docs/FEATURES.md` is current for this SHA.** It is dated by nothing and carries a
   status paragraph (`FEATURES.md:97-102`) saying much of it is *"built but still being tested
   by hand, and unlocks gradually as each piece is verified"*. I compared it to the code; I did
   not establish which of the two is more recent.
