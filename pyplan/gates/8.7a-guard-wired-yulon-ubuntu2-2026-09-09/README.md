# 8.7a's second clause, met by a button — WoW WotLK on `yulon-ubuntu2`, 2026-09-09

**Yes: by a button now, and not only by the engine.** *"A module whose SQL targets the world database
is refused while the server runs, with the step named and no rows written, and applies once the
server is stopped"* (`checklist.md:2501`) is true of the applier
`ControllerServices.for_entry()` hands the Modules tab, with **nothing attached to it by this gate**.
T2's press (`../8.7a-direct-sql-yulon-ubuntu2-2026-09-09/`) had to write `Applier._world_running`
itself and led with *"met by the engine and by no button"*; the line that did it is the one line
deleted here. And the refusal's own instruction — *"Press Stop, then install again"* — was followed
end to end and **succeeded**: the database came up alone, the SQL ran, the world stayed stopped, and
the report says so in the app's own words.

One thing this gate adds that T2's could not even attempt: **the second reading, caught firing.**
The Server tab's Start was pressed *inside* the database's health wait, and the install refused
1 second after the world container came up, with sixteen readings unmoved.

**Box:** `yulon-ubuntu2` (the owner's WotLK install at `/home/pk/wowserver`, native `243c46e3`,
`mod-ale`, 500 bots, his account `PERZI` and warrior `Pakka`).
**Tree pressed:** `7b05565f`, checked out on the box as a git worktree at `~/t7` and verified in
every capture (`yulon package under test: /home/pk/t7/pylauncher/yulon/__init__.py`). The live-half
commit also carries a comments-only correction (120 s → 180 s); **it is not in the pressed tree**,
and it cannot be — it changes no code.
**Subject:** `manifests/wow-wotlk/modules/mod-arac.json` — `data/sql/db-world/arac.sql`,
`applied_by: direct`, `db: world`, `build.rebuild: false`. The same subject as T2, so the two gates
are comparable reading by reading.
**Driver:** `t7_press.py`, one step per capture, run through the box's own `.venv` against `~/t7`.
**Every action was announced on that box's activity terminal (`~/claude-say`) before it was taken.**
**Two clocks in these logs:** the driver stamps UTC (`11:42:45Z`), the app's own logger prints the
box's local time (`13:42:45`, CEST). They are the same instant.

---

## Verdict, clause by clause

| Clause | Verdict | Evidence |
| --- | --- | --- |
| the shipped applier CARRIES the seam (T2's one open finding) | **PASS** — `_world_running` and `_start_database` are both `_for_wotlk.<locals>.<lambda>`, read and never assigned | `0-seams.log`, and the header of every other log |
| a direct world-SQL step is refused while the world runs, **through that applier** | **PASS, first press** | `2-refuse-world-up.log`, `refusal-sentence.txt` |
| the refusal **names the step** | **PASS** — `sql data/sql/db-world/arac.sql → world` | `refusal-sentence.txt`, `2-modules-tab-refuses-world-up.png` |
| **no rows written** by the refused press | **PASS** — all 16 readings identical, counts *and* checksums | `counts-ground.json` vs `counts-after-refusal.json` |
| *"Press Stop, then install again"* can be followed and **succeeds** | **PASS** — the dead end T2 measured is closed | `5-stop-again.log`, `6-press-applies.log`, `success-report.txt` |
| the database is started **alone**, and the report says so | **PASS** — `✓ started the database alone; the world server was left stopped` | `success-report.txt`, `6-modules-tab-applies-world-stopped.png` |
| the world is **not** started by that path | **PASS** — `ac-worldserver exited pid=0` at the capture, only `ac-database` up | `6-press-applies.log` |
| the counts move when it does apply | **PASS** — four tables by count, all six by checksum, both UPDATE predicates to zero | `counts-after-success.json`, `counts-table.txt` |
| **the second reading fires** when a world comes up during the health wait | **PASS** — refused 1 s after the world container started, nothing written | `4-race.log`, `race-refusal-sentence.txt` |
| `None` is reachable and is a refusal | **PASS as a reading** (not as a press) — a missing container and an unreachable daemon both answer `None`, where `.settled` answers `False` | `0-seams.log` |
| the module removes, and its rows stay | **PASS, as designed** (`remove()`: *"DB rows are kept"*) | `7-remove.log`, `remove-report.txt` |
| the box goes back to the ground | **PASS** — restored from a fresh pre-press dump, every count and checksum equal | `8-restore.log`, `counts-table.txt` |

---

## The seams, as the app built them (`0-seams.log`, 11:42Z)

```
applier._world_running   = <function _for_wotlk.<locals>.<lambda> at 0x...>
applier._start_database  = <function _for_wotlk.<locals>.<lambda> at 0x...>
applier._world_running() answers True right now
```

The driver **reads** those two private fields to prove they arrived and never writes them; `install()`
is then called on the object `for_entry()` returned. If either were `None` the guard would return at
its first line and this gate would have photographed a live world being written to.

**The `None` branch, seen for the first time.** T2 recorded that only `True` and `False` had ever
been returned live. All three answers are now measured on this box:

| what was asked | `container_state().status` | `.settled` | `docker.world_running()` |
| --- | --- | --- | --- |
| `ac-worldserver`, up | `running` | `True` | `True` |
| `no-such-container-8f2a` | `''` | `False` | **`None`** |
| `ac-worldserver`, `DOCKER_HOST=tcp://127.0.0.1:1` | `''` | `False` | **`None`** |

The middle column is why `docker.world_running()` exists: through this guard `False` is fail-**open**,
so a wiring that reused `container_state(...).settled` — which My Party's group next door does, and
correctly for its own question — would tell a host whose Docker will not answer that its world was
down and let the SQL through. `DOCKER_HOST` was set for exactly the one read and restored in a
`finally`; the log prints it back at `None`.

**Where that leaves the user, said plainly:** in both of those states the refusal's advice is *not*
followable either. The sentence for `None` is *"Stop the server, then install again"*, and a daemon
that cannot be reached cannot stop a server or start a database. The guard is right to refuse — it is
the only safe answer to *could not ask* — but this is a state the app has no repair for, and it is
not T7's bug to fix.

## The ground, read before anything was written (`1-ground.log`, 11:42:26Z)

`ac-worldserver running pid=409560 restarts=0`, `ac-database` healthy, `ac-authserver` up;
`controller.status() = InstallStatus(db=True, auth=True, world=True)`; `modules/mod-arac` absent, no
`mod_arac` conf, no `.arac_sql_applied` marker; `modules/` holding `mod-ale` and `mod-playerbots`.

**Every one of the sixteen ground readings equals T2's**, which is how this gate knows the box really
was handed back: 62 / 283 / 77 / 0 / 20 / 9464 rows, the same six checksums, and the three
"work still to do" predicates at 5, 635 and 0. Two of the six tables are UPDATEd rather than inserted
into, so their row count cannot move at all — `skills_still_to_change` (5) and
`quests_still_to_change` (635) are what make a successful press distinguishable from one that wrote
nothing. `1b`'s fresh `mysqldump` of exactly those six tables was taken next: 7 038 460 bytes,
6 `CREATE TABLE` lines (`1-dump.log`), kept on the box at `~/t7-gate-out/` and **not committed**.

## The counts, at every stage

`counts-table.txt`, built from the saved JSON rather than retyped.

| reading | ground | after refusal | after the race | after success | after remove | after restore |
| --- | --- | --- | --- | --- | --- | --- |
| rows playercreateinfo | 62 | 62 | 62 | 100 | 100 | 62 |
| rows playercreateinfo_action | 283 | 283 | 283 | 488 | 488 | 283 |
| rows playercreateinfo_skills | 77 | 77 | 77 | 77 | 77 | 77 |
| rows playercreateinfo_spell_custom | 0 | 0 | 0 | 7219 | 7219 | 0 |
| rows player_totem_model | 20 | 20 | 20 | 40 | 40 | 20 |
| rows quest_template | 9464 | 9464 | 9464 | 9464 | 9464 | 9464 |
| checksum playercreateinfo | 1375550933 | 1375550933 | 1375550933 | 773387713 | 773387713 | 1375550933 |
| checksum playercreateinfo_action | 1810990106 | 1810990106 | 1810990106 | 3925301461 | 3925301461 | 1810990106 |
| checksum playercreateinfo_skills | 1381324222 | 1381324222 | 1381324222 | 4287417897 | 4287417897 | 1381324222 |
| checksum playercreateinfo_spell_custom | 0 | 0 | 0 | 840767378 | 840767378 | 0 |
| checksum player_totem_model | 58188735 | 58188735 | 58188735 | 3963586888 | 3963586888 | 58188735 |
| checksum quest_template | 4127409649 | 4127409649 | 4127409649 | 1875883529 | 1875883529 | 4127409649 |
| skills_still_to_change | 5 | 5 | 5 | 0 | 0 | 5 |
| quests_still_to_change | 635 | 635 | 635 | 0 | 0 | 635 |
| quests_at_1791 | 12 | 12 | 12 | 647 | 647 | 12 |
| totem_rows_for_arac_races | 0 | 0 | 0 | 20 | 20 | 0 |

**How the "nothing was written" columns are chained, because two of them could not be read at all.**
The reading *before* the raced press and the reading *before* the successful press are
`UNREADABLE (... container 3c922e8e... is not running)` for all sixteen — the app's Stop had taken the
database down, which is `bug-checklist §46` reproducing itself inside this gate. So the chain is
read *after* each press instead: ground = after refusal = **after the race** (all sixteen at the
ground values, with the database back up to be read) → after success (fourteen moved). Nothing was
written by either refusal, and the checksums are what say so — a refusal that had already sent three
of `arac.sql`'s statements would photograph like one that sent none if only row counts were kept.

---

## 1 — the refusal, world up, through the app's own applier (11:42:45Z)

`2-refuse-world-up.log`. `applier._world_running()` answered `True` from a real `docker inspect`
before the press; `ac-worldserver running pid=409560 restarts=0` at the capture, its own last log
lines the bot-engine report (`Non-combat: 436, Combat: 57, Dead: 7`). The press raised:

```
mod-arac: the world server is running, and it holds world in memory and writes back over whatever
it finds there. No SQL was run and no rows were written: sql data/sql/db-world/arac.sql → world.
Press Stop, then install again — the steps this run already took repeat, and the SQL follows them.
```

`2-modules-tab-refuses-world-up.png` is that sentence in the app's own Modules tab, rendered
offscreen through the tab's own failure slot (`_module_failed`), which prefixes it
`install mod-arac FAILED:`.

**What the refusal did not stop, correctly:** the clone. `modules/mod-arac` went from absent to a
checkout during the refused press, because the guard is a pre-pass over the SQL step and `install()`
clones, deploys and patches first. The sentence says so itself.

## 2 — Stop, through the app's own control (11:42:58Z – 11:43:19Z)

`3-stop.log`. `svc.controller.stop()` returned `True` in **20.9 s**; afterwards `containers up = `
(nothing), `ac-worldserver exited pid=0`, `ac-database exited pid=0` — `compose stop`, so the
containers are **kept** and merely `exited`, not removed. The applier's own seam then answered
`False`.

## 3 — the second reading, caught firing (11:43:41Z – 11:43:48Z)

`4-race.log`. This is the window Codex named in review, measured. The install began with the world
and the database both down, so `start_database()` really had work to do and really waited:

```
11:43:41Z  install() begins; the seam answers False, so the guard permits
13:43:41   start_database(): starting ac-database alone
13:43:41   docker compose up -d --no-deps ac-database        (cwd=/home/pk/wowserver)
11:43:43Z  >>> RACING PRESS: svc.controller.start()          <- the Server tab's Start
13:43:43   start_staged(): compose up -d --no-deps ac-database ac-authserver ac-worldserver
11:43:47.437  ac-worldserver STARTS (pid 532359)
11:43:48Z  REFUSED
```

The refusal's sentence is **byte-identical** to press 1's (`refusal-sentence.txt` and
`race-refusal-sentence.txt` `diff` clean), which is the design decision showing itself: one rule, one
vocabulary, so a user does not meet two. The offscreen rendering would have been identical too, so
only one such PNG is kept.

At the capture: `ac-worldserver running pid=532359 restarts=0 started=11:43:47.437`, its own log
lines mid-startup (`> Version DB world: ACDB 335.17-dev`, `Initialize ALE Lua Engine...`). **The
world came up one second before the reading that refused.** Sixteen readings at the ground values
afterwards: nothing was written.

**What this proves about the code before round 2:** at 11:43:48 the world had been `running` for one
second, and the pre-round-2 code would have sent `arac.sql` right there — the first reading (`False`,
taken at 11:43:41) was the only one it had.

**The residual window is not zero, and this gate does not claim it is.** What remains is the time
between the second `docker inspect` returning and the first statement reaching the database: one
`docker inspect` costs about 0.3 s of CLI start-up on this platform (`docker.py`'s own measurement,
2026-08-22, not this gate's) plus `docker exec` latency for the first statement. A world started
inside *that* is still unprotected. Closing it needs the lifecycle lock Codex named — Start and module
actions serialised on one per-install mutex — which crosses into the Server tab and is deliberately
not built here.

## 4 — Stop again, then install again: the sentence followed (11:44:19Z – 11:44:42Z)

`5-stop-again.log` — `stop()` returned `True` in **3.8 s** (the world had been up 35 s and had not
finished loading, which is why this stop is so much cheaper than the first).

`6-press-applies.log` — the whole sequence the ticket asks for, in one capture:

```
ground: the seam answers False
ground: the database is exited pid=0 restarts=0
13:44:34   start_database(): starting ac-database alone
13:44:34   docker compose up -d --no-deps ac-database     (cwd=/home/pk/wowserver)
13:44:35   wait_db_healthy() called: db_container=ac-database timeout=180.0
11:44:42Z  install() returned after 8.8s
```

and the report the app produced (`success-report.txt`, drawn in
`6-modules-tab-applies-world-stopped.png`):

```
install mod-arac:
  ✓ clone https://github.com/heyitsbench/mod-arac.git → modules/mod-arac
  ✓ started the database alone; the world server was left stopped
  ✓ sql data/sql/db-world/arac.sql → world
  – skipped: client Patch-A.MPQ: no client dir configured
  – skipped: server_dbc patch-contents/DBFilesContent: no DBC copier configured
  ⚠ Press Stop and then Start on the Server tab to apply this.
```

At the capture: `containers up: ac-database Up 7 seconds (healthy)` and **`ac-worldserver exited
pid=0`** — the database alone, the world left where Stop put it. `ac-authserver` was `exited` too:
`start_database()` starts one service with `--no-deps` and nothing else. The world's own last log
lines are its shutdown (`All connections on DatabasePool 'acore_playerbots' closed.`), which is a
stopped server's log and not a running one's.

Fourteen of the sixteen readings moved; the two constant-by-construction row counts did not, and
their two predicates went 5 → 0 and 635 → 0.

## 5 — the box, put back (11:44:50Z – 11:48Z)

* `7-remove.log` — `applier.remove(manifest)` reported `✓ rm -r modules/mod-arac` and **kept every
  row**, which is `remove()`'s documented behaviour and this manifest has no remove-time SQL. So
  *"back to the ground"* was **not** true after the remove, and could not be.
* `8-restore.log` — restored from the fresh `mysqldump` taken before press 1, loaded with the world
  still `exited` (checked by the applier's own seam before the write, because writing these tables
  under a live world is the very thing the guard forbids). `rc=0`, and **every count and every
  checksum equal to the ground reading.**
* `9-start.log` — `svc.controller.start()` returned in 1.2 s; `10-ready.log` — readiness `True` in
  0.2 s.

**On that 0.2 s, honestly.** `azerothcore_ready('100.99.204.5', 8085)` matches the realm's
address:port in the **auth** log, and it answered `True` while the world was 36 s into loading 500
bots. It is an auth-side marker, not proof the world had finished. What does say the world finished
is its own log at the last capture: `498/500`, `499/500`, `500/500 Bot ... logged in`
(`11-final-state.log`).

---

## How the box was left (`11-final-state.log`, 11:48Z)

| Thing | State |
| --- | --- |
| `ac-worldserver` | **running**, `pid=538052 restarts=0`, started 11:45:44Z, `500/500` bots logged in |
| `ac-authserver` | **running**, `pid=538055`, started 11:45:44Z — **not restarted by this gate** |
| `ac-database` | **running**, healthy, started 11:44:34Z |
| `controller.status()` | `InstallStatus(db=True, auth=True, world=True)` |
| realm row | `1  Yulon ubuntu2  100.99.204.5  100.99.204.5  255.255.255.0  8085` — read, `needs a write: False`, **not written** |
| the six world tables | identical to the ground, count and checksum |
| `modules/` | `mod-ale`, `mod-playerbots` — `mod-arac` cloned and removed, nothing else changed |
| `worldserver.conf` | **untouched** — `mtime 2026-09-08 23:11:29`, with `Logger.ALE=4,Console Server` still at line 706 |
| accounts | 104 rows, ids 1–109; the four non-bot ones are `101 YULON_243C46E3`, `102 PERZI`, `103 YULONADMIN`, `109 YULONPANEL` — the same four the previous lane left |
| orphaned `account_access` rows | **0** |

**The world's pid changed** (409560 → 538052) and its `StartedAt` with it: this gate stopped and
started the world three times, which the clause it proves cannot be tested without.

**The owner's things, listed and read only:**

* `/home/pk/LootPet.lua` — 31 216 bytes, mtime **2026-09-08 22:17:42**, unchanged.
* `/home/pk/LootPet2.lua` — **does not exist on this box** (`stat: cannot statx`). The handover named
  it; the file is not there, and this gate created and deleted nothing in `/home/pk`.
* account `PERZI` — id 102, `last_login 2026-09-08 23:24:16`, `joindate 2026-09-08 20:28:14`,
  salt and verifier both 32 bytes. **No password was set, reset or read**; no account was created,
  so there is nothing to have orphaned.
* character `Pakka` — guid 1001, level 6, race 2, class 1, offline. `acore_characters` was read once
  (that row) and never written; `arac.sql` touches `acore_world` only.

---

## What this press still did not show

1. **A hand on the keyboard.** The three tab renderings are the real tab with the real strings
   through the tab's own slots, rendered offscreen — not a person clicking Install.
2. **The renderings were made after the presses.** They were produced at 13:49 local from the exact
   strings the presses saved (`refusal-sentence.txt`, `success-report.txt`, `remove-report.txt`), so
   the `ac-worldserver at capture:` line in the shot log is the state at *rendering* time (running),
   not at the press. The container states at each press are in the press logs, where they were taken.
3. **`characters` and `playerbots`.** `WORLD_HELD_DBS` has three members and only `world` was
   pressed, on this game. The two shipped manifests with a direct `characters` step
   (`wow-wotlk/ale/accountwide.json`, `wow-wotlk/ale/battlepass.json`) were not pressed; no shipped
   manifest writes `playerbots` directly at all.
4. **The other three games.** TBC, Vanilla and Tortoise have the same two seams wired and unit tests
   that prove arrival, and none of the three has been pressed live. On Tortoise, `GuardedApplier`'s
   own 2504 check runs *before* 8.7a's, so an armed updater would show `AutoUpdateRefused` first —
   worth one line in whatever gate presses it.
5. **`start_database` on a non-AzerothCore spec.** It starts `spec.compose_services()[0]`, and that
   has only ever been live-gated on AzerothCore. Whether it names the right service on the CMaNGOS
   trees is a per-spec fact nobody has measured.
6. **`remove` and `configure` under a live world.** `mod-arac` has no remove-time or configure-time
   SQL, so the guard was never exercised on the two actions whose refusals `remove()`'s ordering
   comment is about.
7. **The database that will not start.** The refusal for it is unit-tested and carries the daemon's
   own sentence; no live press produced one, because making it happen means breaking the owner's
   database on purpose.
8. **The module's in-game effect.** No client was driven, the DBC and MPQ halves were skipped by the
   app's own wiring, and the rows were removed again within minutes.

---

## Deviations

* **Three stop/start cycles, not one.** T2 kept it to one by ordering its steps around a single
  stop; the race press has to *end* with the world up (it is the racing Start that puts it there), so
  a second Stop was needed before the successful press. Cost: two extra `compose stop`/`up` cycles of
  a 500-bot world, 20.9 s and 3.8 s. Every write to the six tables still happened with the world
  down, which is the rule the whole gate is about.
* **The race press ran over the clone press 1 had already made**, rather than from a clean
  `modules/`. That is the ordinary state a user retrying an install is in, and the guard's own
  sentence promises it (*"the steps this run already took repeat"*).
* **The duplicate rendering was deleted.** The race refusal's sentence is byte-identical to press
  1's, so its offscreen PNG was too (`md5 78dd1d97…` for both); one copy is kept and
  `race-refusal-sentence.txt` carries the text.
* **The driver was fixed twice on the box, in the last step only.** `azerothcore_ready` lives in
  `yulon.docker`, not `yulon.catalog.catalog`, and `account_access` keys on `id` rather than
  `AccountID` on this schema; a `LIKE 'RNDBOT%'` was replaced with `id > 100` after the shell ate its
  quotes. All three are in `step_ready`/`step_final` — the "put the box back" reads — and none of
  them touches a press. The committed `t7_press.py` is the fixed copy.
* **No `vmshot` photograph from the Hyper-V host.** T2 put one reading on the VM's desktop and
  photographed it. This gate's equivalent evidence is the app's own offscreen renderings plus the
  container states and the world's own log lines printed at every capture; a desktop photograph would
  have shown the same text one layer further from the source.
* **The 7 MB table dump is not committed.** It lives on the box at
  `~/t7-gate-out/world-six-tables-before.sql`; what is here is the counts and checksums that make it
  checkable.
* **`DOCKER_HOST` was pointed at a dead port for exactly one read**, to produce the
  unreachable-daemon answer without touching the real daemon, and restored in a `finally` (the log
  prints it back at `None`).

## Files

| File | What it is |
| --- | --- |
| `t7_press.py` | the driver; one step per capture, run on the box against the worktree at `~/t7` |
| `0-seams.log` | the two seams on the applier the app built, and all three answers of `world_running` |
| `1-tables.log`, `1-ground.log`, `1-dump.log` | the subject, the ground, and the fresh restorable copy |
| `2-refuse-world-up.log`, `refusal-sentence.txt`, `2-modules-tab-refuses-world-up.png` | the refusal, world up, through the shipped applier |
| `3-stop.log` | the app's own Stop, and what it leaves behind |
| `4-race.log`, `race-refusal-sentence.txt` | Start pressed during the health wait, and the second reading refusing |
| `5-stop-again.log`, `6-press-applies.log`, `success-report.txt`, `6-modules-tab-applies-world-stopped.png` | Stop, then install again — and it succeeds |
| `7-remove.log`, `remove-report.txt`, `7-modules-tab-after-remove.png` | the remove, and the rows it keeps |
| `8-restore.log` | back to the ground, from the pre-press dump |
| `9-start.log`, `10-ready.log`, `11-final-state.log` | the box handed back |
| `12-counts-table.log`, `counts-table.txt`, `counts-*.json` | every reading, as saved by the driver |
