# 7.9's controller-surface gate, RE-RUN on the merged tip — m910q, 2026-09-09: **29 of 33**

**29 of 33 gate checks, and the four that failed are one install's database rather than the code
under test.** Two of the three games are 11 of 11 and byte-identical to the night before; the
third could not start a worldserver at all, for a reason that was already true 2 h 38 min before
this lane touched the box and was **already written down in this repository three commits before
the tip**. "Re-run green on the merged tip" is therefore not true of the controller-surface half,
and this folder says precisely how it is false.

---

## Why this run exists

The 2026-09-08 audit, quoted in this lane's brief:

> graded MET on `96a129dc` (33 of 33), an ancestor of the tip — but gate-79's own ready-wait
> table has since changed (`576a3f93`, and `7.9-rerun-m910q-2026-09-08/starter.py` now IMPORTS
> `pyplan/gates/ready_wait.py` instead of carrying a third copy). That half has not been re-run
> since the script changed. It exercised the three CMaNGOS games, which never touched the wrong
> WotLK row, so nothing is known to be broken — but re-run green on the merged tip is not
> literally true of it either.

It is the controller-surface half of Phase 8's exit line (`pyplan/checklist.md:2508`). The
cross-server half is `pyplan/gates/7.10-rerun-ubuntu2-2026-09-09/` (52 of 53). Its predecessor
is `pyplan/gates/7.9-rerun-m910q-2026-09-08/`, which this folder re-runs clause for clause.

**46 commits separate the two runs**, `96a129dc..c2fcf0ea` — **+2943/-124** across
`pylauncher/yulon/`, of which **+486/-63** are in `pylauncher/yulon/ui/controller_view.py`, the
file that assembles the very services this gate times — plus the ready-wait rewrite the audit
named.

---

## Provenance

**Code under test.** `/home/pk/lane79b/checkout` on `m910q`, a fresh
`git clone --branch yulon-phase8b` made on the box, at
**`c2fcf0eaeff012a80078d15c784645b4b974c1a1`** — `origin/yulon-phase8b`'s tip, confirmed with
`git ls-remote` before the clone and `git rev-parse HEAD` after it, with a clean
`git status --porcelain`. `run.log` and every per-game log header carry the sha as the runner
read it. `~/lane79`, the 09-08 lane, was not written to.

Interpreter `/home/pk/gate81b-venv/bin/python`, **Python 3.11.15**, **PySide6 6.11.2**,
Docker **29.7.2**, box `PK-M910q`, Linux 6.8.0-138-generic — the same four the 09-08 run used, so
the environment is not a variable between the two.

**Harness.** `pyplan/gates/gate-79-controller-surface.py` at that commit, **run unmodified**, with
its ready-wait dispatch now coming from `pyplan/gates/ready_wait.py` as `576a3f93` left it. The
runner (`run79b.sh`), the photograph runner (`run-shots79b.sh`), the Tortoise counts probe and the
two diagnostics are this lane's and are committed here exactly as they ran.

---

## The ground, read before anything was started

`ground-start.txt`, taken at **`2026-09-09T03:47:23Z`**, before the first game was touched:

* `docker ps -a` — **every** game container `Exited`. `tortoise-mangosd Exited (139) 3 hours ago`,
  `tbc-mangosd Exited (139) 6 hours ago`, everything else `Exited (0)`. The only thing up was `r6`
  (`refute5img:latest`, up 4 days, not a game server and not this lane's);
* listening game ports: **`(none of 3724/3443/8085/8888/8129)`**;
* backup directories: `~/tbc-7.4c` **13 entries** (the 2026-09-04 lanes'), `~/vanilla-75b` **0**,
  `~/tortoise-server` **0**;
* disk **29 G free**.

**And the Tortoise counts, because this lane's brief is to leave that install exactly as it found
it** (`tortoise-counts.txt`, `03:47:16Z`, with only `tortoise-db` started for the read and stopped
again after it):

| | before the run | after the gate pass | at the end of the lane |
|---|---|---|---|
| `tw_char.characters` | **903** | **903** | **903** |
| `tw_logon.account` | **109** | **109** | **109** |
| `tw_world.migrations` | **158** | **158** | **158** |
| `tw_char.migrations` | 1 | 1 | 1 |

Those are the pre-attempt counts the brief names. Section 11 of the gate dumps and restores every
one of Tortoise's four databases, so this lane writes to that install by design; the round trip
landed where it started, and both ends are counted rather than asserted.

Every game's baseline section then reads `status: db=False auth=False world=False` in its own log
(`gate79-tbc.log:13`, `gate79-vanilla.log:13`, `gate79-tortoise.log:14`), so `start_staged()`
really did the starting in all three cases — **none of the eleven clauses is a restatement of the
state its run began in**, which is the rule this whole set of presses exists to keep.

---

## Result

| pass | game | server dir | checks | exit | elapsed | log |
|---|---|---|---|---|---|---|
| gate | `wow-tbc` | `~/tbc-7.4c` | **11 passed / 0 failed** | 0 | 280 s | `gate79-tbc.log` |
| gate | `wow-vanilla` | `~/vanilla-75b` | **11 passed / 0 failed** | 0 | 209 s | `gate79-vanilla.log` |
| gate | `wow-tortoise` | `~/tortoise-server` | **7 passed / 4 failed** | **1** | 170 s | `gate79-tortoise.log` |
| shots | `wow-tbc` | | 6 OK / 0 FAIL | 0 | — | `shots-tbc.log` |
| shots | `wow-vanilla` | | 6 OK / 0 FAIL | 0 | — | `shots-vanilla.log` |
| shots | `wow-tortoise` | | 6 OK / 0 FAIL | 0 | — | `shots-tortoise.log` |

**The Tortoise line of the photograph pass is 6 OK against a worldserver that could not start, and
those six are worth nothing.** That is finding 3 below, and it is named here rather than in a
footnote so the 18 is not read as 18.

---

## The eleven clauses, per game

| clause | TBC | Vanilla | Tortoise |
|---|---|---|---|
| baseline: this run did the starting | yes | yes | yes |
| baseline ready wait | **63.0 s** OK | **35.5 s** OK | **28.0 s FAIL** — never ready |
| `console.send_command()` answers | 3.60 s, **6 lines** | 3.60 s, **5 lines** | 3.61 s, **not delimited FAIL** |
| world pid / RestartCount / StartedAt across the attach | unchanged (pid 2590686, restarts 0) | unchanged (pid 2598982, restarts 0) | unchanged (pid 2607931, **restarts 4**) |
| `backup()` | 5.1 s, **4 dumps** | 3.8 s, **5 dumps** | 4.7 s, **4 dumps** |
| `verify_dump()` | 4/4 | 5/5 | 4/4 |
| `plan_restore()` over a running server | **refused**, naming `tbc-mangosd, tbc-realmd` | **refused**, naming `vanilla-mangosd, vanilla-realmd` | **refused**, naming `tortoise-mangosd, tortoise-realmd` |
| `stop_staged()` (the Stop button) | **2.1 s** | **22.0 s** | **4.1 s** |
| `start_staged()` (the Start button) | 6.1 s | 6.1 s | 6.1 s |
| to ready after that start | **63.1 s** OK | **31.3 s** OK | **28.2 s FAIL** — gave up |
| `restore()`, EVERY dump | **49.9 s**, 4 databases | **36.7 s**, 5 databases | **43.9 s**, 4 databases |
| ready again after the restore | **65.4 s** OK | **31.4 s** OK | **26.7 s FAIL** — never ready |

Databases actually round-tripped: TBC `characters, logs, mangos, realmd` (its `mangos` dump is
158,550,486 B); Vanilla `characters, classiclogs, logs, mangos, realmd` (`mangos` 113,674,581 B);
Tortoise `tw_char, tw_logon, tw_logs, tw_world` (`tw_world` 171,361,088 B). Exact byte counts are
in `dump-cleanup.txt`, which names every file it removed.

**Four of Tortoise's seven passes are real and are the interesting part of that column.** Its
`backup()`, `verify_dump()`, `plan_restore()` refusal and `stop_staged()` all did their jobs
against a worldserver in a crash loop — the backup took four correct dumps, all four verified, the
restore refused while the containers were up and named them, and the Stop brought the whole stack
down in 4.1 s. The controller surface behaved; the server underneath it did not exist.

---

## The diff against the 2026-09-08 run, clause by clause

| clause | 09-08 (`96a129dc`) | 09-09 (`c2fcf0ea`) | verdict |
|---|---|---|---|
| **TBC** baseline ready | 64.7 s | 63.0 s | same |
| **TBC** console | 3.60 s, 6 lines | 3.60 s, 6 lines | **identical** |
| **TBC** backup | 5.3 s, 4 dumps | 5.1 s, 4 dumps | same |
| **TBC** verify | 4/4 | 4/4 | identical |
| **TBC** plan_restore refusal | refused, both names | refused, both names | identical |
| **TBC** stop_staged | 2.2 s | **2.1 s** | same |
| **TBC** start_staged | 6.1 s | 6.1 s | identical |
| **TBC** ready after start | 62.6 s | 63.1 s | same |
| **TBC** restore | 49.6 s, 4 DBs | 49.9 s, 4 DBs | same |
| **TBC** ready after restore | 64.9 s | 65.4 s | same |
| **Vanilla** baseline ready | 35.3 s | 35.5 s | same |
| **Vanilla** console | 3.60 s, 5 lines | 3.60 s, 5 lines | **identical** |
| **Vanilla** backup | 3.6 s, 5 dumps | 3.8 s, 5 dumps | same |
| **Vanilla** verify | 5/5 | 5/5 | identical |
| **Vanilla** plan_restore refusal | refused, both names | refused, both names | identical |
| **Vanilla** stop_staged | 22.3 s | **22.0 s** | same |
| **Vanilla** start_staged | 6.1 s | 6.1 s | identical |
| **Vanilla** ready after start | 31.2 s | 31.3 s | same |
| **Vanilla** restore | 36.6 s, 5 DBs | 36.7 s, 5 DBs | same |
| **Vanilla** ready after restore | 31.2 s | 31.4 s | same |
| **Tortoise** baseline ready | 76.5 s **OK** | **FAIL, 28.0 s** | **CHANGED — finding 1** |
| **Tortoise** console | 3.61 s, 22 lines, `prompted=True` | 3.61 s, **`prompted=False`, FAIL** | **CHANGED — finding 1** |
| **Tortoise** backup | 6.4 s, 4 dumps | 4.7 s, 4 dumps | same |
| **Tortoise** verify | 4/4 | 4/4 | identical |
| **Tortoise** plan_restore refusal | refused, both names | refused, both names | identical |
| **Tortoise** stop_staged | 5.2 s | **4.1 s** | same |
| **Tortoise** start_staged | 6.1 s | 6.1 s | identical |
| **Tortoise** ready after start | 69.2 s **OK** | **FAIL, 28.2 s** | **CHANGED — finding 1** |
| **Tortoise** restore | 44.5 s, 4 DBs | 43.9 s, 4 DBs | same |
| **Tortoise** ready after restore | 69.9 s **OK** | **FAIL, 26.7 s** | **CHANGED — finding 1** |
| **`missing_core`** on all three | `()` | `()` | identical |
| **TBC's Stop exits 139** | yes, twice | **yes, twice, same GUID** | **reproduced — finding 4** |
| **Vanilla's Stop exits** | 0 | 0 | identical |

**Twenty-six of the thirty clauses are unchanged, and every timing that moved moved by less than a
second or by a rounding of one.** The four that changed are all Tortoise's ready-marker clauses,
and they are one cause.

**The frames were compared byte for byte, not by eye.** Six of the nine frames that do not depend
on server output are **md5-identical** across the two runs — every TBC frame, every Vanilla frame,
and the Tortoise console-before frame. The +486 lines in `controller_view.py` changed nothing a
camera can see on the Server tab of those two games. The two that differ are Tortoise's, and
finding 2 says why.

---

## Finding 1 — Tortoise's worldserver cannot start, and it was already so before this lane

`tortoise-crash-probe.txt` (every statement in it a `SELECT` or a `SHOW`; nothing created, dropped
or written).

The worldserver's own last words name the statement, the table and the call stack:

```
Making copy of character_inventory table.
SQL: TRUNCATE `character_inventory_copy`
[1146] Table 'tw_char.character_inventory_copy' doesn't exist
Your database structure is not up to date. Please make sure you have executed all the queries in the sql/updates folders.
/src/src/shared/Database/DatabaseMysql.cpp:190: Error: Assertion in HandleMySQLError failed: false
./mangosd(_ZN9ObjectMgr24BackupCharacterInventoryEv+0x33)
./mangosd(_ZN17HonorMaintenancer13DoMaintenanceEv+0x34c)
./mangosd(_ZN5World23SetInitialWorldSettingsEv+0x1e9f)
./mangosd(_ZN6Master3RunEv+0x9b)
```

`HonorMaintenancer::DoMaintenance()` is reached from `World::SetInitialWorldSettings()`, so a
missing table does not break a feature — it makes the world unstartable. The gate's own
`docker.py` saw it for what it was: *`tortoise-mangosd restarted 4 times while being waited on;
that is a crash loop, not a slow start`*.

**Five readings, and none of them is this lane's doing:**

1. `SHOW TABLES FROM tw_char LIKE 'character_inventory%'` returns **`character_inventory` and
   nothing else**. The table the worldserver truncates has never been there. `tw_char` holds 107
   tables.
2. `saved_variables` reads `lastHonorMaintenanceDay 20698, nextHonorMaintenanceDay **20705**`, and
   the box's own clock puts today at **day 20705**. **The maintenance day is today.** Day 20698 is
   2026-09-02 and 20705 is 2026-09-09, which is why the same install passed this same gate 11 of
   11 at 23:44 on 2026-09-08 and cannot start seven hours later.
3. `docker logs -t tortoise-mangosd` puts the **first** occurrence of this failure at
   **`2026-09-09T01:09:37Z`** — 2 h 38 min before this lane's first command, and three minutes
   after the 01:06Z Tortoise rollback restarted it. The other sixteen are all this lane's, between
   03:56:11Z and 03:58:55Z.
4. The table is absent from `~/tortoise-backup-2026-09-08/tw_char.sql` (taken 2026-09-08 **05:40**)
   and from `~/tortoise-backup-2026-09-08b/tw_char.sql` (2026-09-08 **16:37**) — zero mentions in
   either. It has been missing for at least a day and a half, through backups this lane did not
   take.
5. The dump this lane's own `backup()` wrote at 05:56 also contains zero mentions of it, so the
   restore in section 11 put back exactly what it found.

**And it was written down here three commits before the tip.** `9d9b41e8`, the evidence commit
behind Tortoise's SOAP switch, pressed on `yulon-arch` on a **fresh** install:

> Also named, not fixed: a fresh install of this fork crash-loops until the fork's own
> `character_updates` SQL is applied by hand, and the catalog has no phase for that directory.

That directory is `~/tortoise-server/src/tortoise-wow/sql/character_updates/`, it ships **three**
files, and the one that creates the missing table is
**`20260812142512_character_inventory_copy.sql`**. `tw_char.migrations` holds **one** row, so at
most one of the three was ever applied.

**What this lane adds to that sentence, and it is the larger half.** The `yulon-arch` press found
a *fresh* install crash-looping *immediately*. This is an **established** install, 903 characters,
serving gates for weeks, and it crash-loops **on a date**. The two together say the defect is not
about fresh installs: *every* install of this fork that has never had `character_updates` applied
is one honor-maintenance day away from being unstartable, and the day is weekly. That is a
different and worse claim than "a fresh install needs SQL applied by hand", and it was reachable
only by running the gate again on a different day.

**Not fixed here, deliberately.** Applying that SQL is a write to an install this lane's brief
says to leave exactly as it found it, and the counts table above is the promise it kept. The
statement that would settle it, for whoever owns the box, is the fork's own file:

```
docker exec -i -e MYSQL_PWD="$(sed -n 's/^DB_ROOT_PASSWORD=//p' ~/tortoise-server/.env)" \
  tortoise-db mariadb -u root tw_char \
  < ~/tortoise-server/src/tortoise-wow/sql/character_updates/20260812142512_character_inventory_copy.sql
```

with the other two files in that directory owed the same treatment and the catalog owed a phase
that does it, which is the half `9d9b41e8` already booked.

---

## Finding 2 — the Server tab reads "world up" for a worldserver dying every seven seconds

`shots/wow-tortoise-server-tab-after-refresh.png`. One `QTest.mouseClick` on Refresh, and the
label reads:

> **status: db up, auth up, world up**

At that instant `tortoise-mangosd` had `RestartCount 4` and had never once completed start-up.
`ground-after-tortoise.txt`, taken independently through the docker CLI eleven seconds earlier,
calls the same container **`Restarting (139) Less than a second ago`**.

This is not the gate lying — the gate reported `FAIL` three times. **It is the product's own
surface, photographed.** `recheck()` asks whether the containers are running, and a container in a
restart loop is running, repeatedly. A dad reading that tab is told his server is up while it is
aborting on a database assertion.

The value that separates the two is already in the same daemon reply the app fetches:
`docker.py`'s wait path reads `RestartCount` and says *"that is a crash loop, not a slow start"* in
so many words. The Server tab's status read does not ask for it. **Not changed here** — this is a
gate, the instrument has to stay as it is for the 29 to mean anything against the 33, and a
surface change belongs to whoever ticks the box, with this frame as its RED.

The frame diff also explains itself, and the explanation is **not** a regression: Tortoise's two
Server-tab frames differ from the 09-08 pair because `c7e577c7` changed that entry's
`operations.channel` from `"attach"` to `"soap"`. On 09-08 the tab said *"This core has no remote
command listener to turn on — it is built with neither SOAP nor the telnet console"* with a **Test
the console** button; tonight it says *"Command channel: not set up yet."* with a **Turn on the
command channel** button. That is `c7e577c7`'s intent, measured on `yulon-arch` and recorded in
`9d9b41e8`, arriving on the surface 7.9 gates. It is named here because a frame diff with no
explanation is how a deliberate change gets filed as a regression.

---

## Finding 3 — the photograph pass scored 6 of 6 on that same dead server

`shots-tortoise.log`. Three separate guards were supposed to make this impossible and all three
let it through:

1. **`starter.py` got it right and nobody listened.** It printed `ready=False after 26.9s` and
   returned non-zero. `run-shots79b.sh` — and `run-shots79.sh` before it, unchanged in this
   respect — does not check that exit code, and ran `shots79.py` anyway.
2. **The liveness sidecar cannot tell restarting from running.** `shots79.py`'s `containers_up()`
   greps `docker ps --format {{.Names}}`, and a crash-looping container appears there most of the
   time. `shots/shots.txt` therefore records *"containers alive at capture: `['tortoise-db',
   'tortoise-realmd', 'tortoise-mangosd']`"* for all four Tortoise frames — true, and useless. The
   guard exists because *"a dead server photographs like a refusal"*; a **dying** one photographs
   like a healthy one, which is worse, because it produces a green frame instead of an obvious gap.
3. **"the console round-trip came back and is not empty" passed on 265,227 characters of
   start-up spam.** The eight lines the log quotes are `reference_loot_template` warnings and
   `[Eluna]: Executed 3 Lua scripts`. Not one word of them answers `server info`. The gate's own
   console clause, asking a stricter question — was the reply *delimited on this core's prompt* —
   said `prompted=False` and failed. The photograph pass asks only whether bytes arrived.

The two one-line changes that would catch it are `[ $? -eq 0 ] || return` after `starter.py` in the
runner, and reading `.State.Restarting`/`.RestartCount` in `containers_up()` instead of `docker
ps`. **Neither is made here**, for the reason in finding 2: this folder's numbers are this
instrument as it stands, and the 09-08 folder's numbers were taken with the same one. This is the
RED for whoever changes it. It is the same shape as the blind spot the 09-08 README named for the
stop check, found again one pass over — which is the argument for making both changes at once
rather than either alone.

---

## Finding 4 — TBC's Stop still abort-traps, same GUID, and it is still TBC alone

`stop-abort.txt`, `exit-codes.txt`. The 09-08 run found `tbc-mangosd` exiting **139** on a clean
`stop_staged()` and booked it as a finding the gate cannot see. **It reproduced**, on both of
tonight's TBC stops:

```
/tbc-mangosd exit=139 started=…T03:50:57.446Z finished=…T03:52:05.329Z restarts=0 oom=false
/tbc-realmd  exit=0   started=…T03:50:57.447Z finished=…T03:52:04.248Z restarts=0 oom=false
/tbc-db      exit=0   started=…T03:48:46.450Z finished=…T03:52:05.675Z restarts=0 oom=false
```

Its last words, and the GUID is the one the 09-08 run recorded:

```
Object::~Object (GUID: 181646 TypeId: 5) deleted but still in world!!
Critical Error: A condition which must never be false was found to be false. Server was shut down to protect data integrity.
~Object(): false
```

**The same GUID on two different nights makes it deterministic**, not an incidental teardown race,
and that is new information: 09-08 could only say it happened twice in one evening.

`stop_staged()` ordered the shutdown correctly again — **`tbc-db` finished 0.35 s AFTER
`tbc-mangosd`**, so the database was not pulled out from under the worldserver, which is what
distinguishes this from the `Lost connection to MySQL` mechanism 7.9 booked on 2026-09-04 from a
hand-typed `docker stop`.

**It is a per-tree fact and it was measured per tree again**, on the same night, the same box, the
same call: `vanilla-mangosd exit=0`, last words `SOAP shutting down` / `Halting process...`. TBC
alone. `tortoise-mangosd` also shows `exit=139`, and that one is **not** this mechanism — it is the
crash loop of finding 1 exiting, with `restarts=4` beside it, and conflating the two would invent a
second victim for a bug that has one.

---

## Finding 5 — the 09-08 recipe can no longer be run, and it fails before it starts a server

`import-probe.txt`, taken **before** anything was started, because a run that dies on an import
proves nothing about a server.

`576a3f93` gave `starter.py` `sys.path.insert(0, Path(__file__).resolve().parent.parent)` to reach
`ready_wait`. The 09-08 recipe **copied** `starter.py` to the lane root, where that expression
resolves to `$HOME`:

| where `starter.py` was run from | result |
|---|---|
| the lane root — the 2026-09-08 recipe | `ModuleNotFoundError: No module named 'ready_wait'` |
| its committed path inside the checkout — **what this run used** | `IMPORTED OK; READY_CALLS: ['wow-tbc', 'wow-tortoise', 'wow-vanilla', 'wow-wotlk']` |
| `gate-79-controller-surface.py` (inserts its **own** directory) | `IMPORTED OK` — unaffected |
| `ready_wait.py` itself | `IMPORTED OK` |

So `run79b.sh` and `run-shots79b.sh` invoke every driver from
`$LANE/checkout/pyplan/gates/7.9-rerun-m910q-2026-09-08/`, not from copies. The change that made
the two passes share one ready-wait table also made one of them un-relocatable, which is a fair
price and worth knowing before the next lane copies a script out of a gate folder.

---

## What the ready-wait rewrite actually did to these three games — said plainly

`576a3f93` is the reason this re-run was asked for, and on the three CMaNGOS games it changed
**the plumbing and not the call**. `READY_CALLS`'s rows now take a `ReadyArgs` rather than an int,
and `wait_ready_for_game(entry, server_dir)` replaces `wait_ready_for_game(game, auth_port)` — but
TBC and Vanilla still resolve to `wait_server_ready()` with no arguments, and Tortoise still to
`wait_server_ready("127.0.0.1", entry.container_spec().ports[0])`. The `realm_pair` thunk that
sends a `SELECT` is reached **only** by the WotLK row, so **not one of tonight's three games sent a
statement to read a realm row**, exactly as `ready_wait.py`'s docstring promises. The measured
consequence: TBC 64.7 s → 63.0 s and Vanilla 35.3 s → 35.5 s at the baseline, which is noise.

**What this re-run therefore does NOT assert:** the WotLK row — the one row `576a3f93` actually
fixed and the only one whose arguments have to be right — **is not exercised here**, because 7.9
runs the three CMaNGOS games. Its evidence is `pyplan/gates/7.10-rerun-ubuntu2-2026-09-09/`, on a
box that has a WotLK install. This folder proves the rewrite did not break the three rows it did
not need to change; it proves nothing about the fourth.

---

## The box, as it was left

`ground-end.txt` (`04:05:41Z`) against `ground-start.txt` (`03:47:23Z`):

| | start | end |
|---|---|---|
| game containers running | none | **none** |
| listening game ports | none of the five | **none of the five** |
| `~/tbc-7.4c/sql_scripts/backups` | 13 entries | **13 entries** |
| `~/vanilla-75b/sql_scripts/backups` | 0 entries | **0 entries** |
| `~/tortoise-server/sql_scripts/backups` | 0 entries | **0 entries** |
| disk free | 29 G | **29 G** |
| not ours, untouched | `r6` up 4 days | `r6` up 4 days |

**The 26 dumps this run wrote were removed, and the removal is itself a record**
(`dump-cleanup.txt`, which names and sizes every file before deleting it and re-counts against
`ground-start.txt` afterwards): 8 from TBC, 10 from Vanilla, 8 from Tortoise, ~1.25 GB. **TBC's
thirteen from the 2026-09-04 lanes survived**, which is the control showing the glob removed this
run's files and not the directory.

Tortoise is at 903 characters, 109 accounts, 158 world migrations — its pre-attempt counts,
unchanged, read three times. **Nothing was installed, rebuilt, reimported or removed. No account
was created. No configuration file was written. No SQL was applied.** The `character_updates` file
finding 1 names is still unapplied, which is the state the brief asked for.

Kept on the box on purpose: `~/lane79b/` — the checkout, the runners and `out/`, which is what this
folder is a copy of. `~/lane79`, the 09-08 lane, was not touched.

---

## Files

| File | What it is |
|---|---|
| `run79b.sh` | the gate runner, exactly as it executed; its header names the three ways it differs from `run79-rerun.sh` and why |
| `run-shots79b.sh` | the photograph runner, exactly as it executed |
| `tortoise-counts.sh` | the three Tortoise counts, read-only, `tortoise-db` only |
| `tortoise-crash-probe.sh` | finding 1's diagnosis; every statement a `SELECT` or a `SHOW` |
| `cleanup-79b.sh` | the dump removal, which names every file before it goes |
| `gate79-tbc.log`, `gate79-vanilla.log`, `gate79-tortoise.log` | the three gate transcripts. The Tortoise one is 254 KB because the console clause captured 265,227 characters of a worldserver's start-up before it died; that volume **is** the evidence for `prompted=False` and is not trimmed |
| `shots-tbc.log`, `shots-vanilla.log`, `shots-tortoise.log` | the three photograph transcripts |
| `ground-start.txt`, `ground-after-*.txt`, `ground-end.txt` | the same probe, five times |
| `tortoise-counts.txt` | the counts, before / after the gate pass / at the end |
| `tortoise-counts-FIRST-ATTEMPT-2002.txt` | the probe's own first attempt, which waited for the container instead of for mariadb and returned four `ERROR 2002` strings where counts belong. Kept because a probe that reports an error string as a reading is exactly the artefact this repository keeps being bitten by |
| `tortoise-crash-probe.txt` | finding 1: the missing table, the maintenance-day row, the timestamps, the older dumps, and the SQL file that creates it |
| `stop-abort.txt` | finding 4: all nine containers' exit codes, TBC's last words and its whole-log counts, Vanilla's last words on the same call |
| `exit-codes.txt` | `docker inspect` after every one of the six stops, all three games |
| `import-probe.txt` | finding 5: the four import results, taken before anything was started |
| `stops.txt` | all six `Controller.stop()` calls, status either side |
| `dump-cleanup.txt` | every dump this run wrote, named and sized, then removed, then re-counted |
| `run.log` | provenance header, per-game exit code and elapsed time |
| `shots/` | twelve frames and `shots.txt`, the liveness record for each — read finding 3 before trusting the four Tortoise ones |
