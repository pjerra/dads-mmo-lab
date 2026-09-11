# The direct-SQL guard, pressed against a live world — WoW WotLK on `yulon-ubuntu2`, 2026-09-09

**8.7a's second clause is met by the engine and by no button.** *"A module whose SQL targets the
world database is refused while the server runs, with the step named and no rows written, and
applies once the server is stopped"* (`checklist.md:2501`) is now true of
`apply.py::_refuse_direct_sql_into_a_running_world` against a real running AzerothCore worldserver,
on its first press, with the row counts as the proof rather than the sentence. It is **not** true of
anything a user can press: no shipped caller passes the `world_running` seam (`apply.py:1920` says
so, and this gate wired it itself), and after the app's own Stop there is no control that puts the
database back, so the refusal's own instruction — *"Press Stop, then install again"* — was measured
ending in a raw Docker container id. Both halves are below, with what they cost.

**Box:** `yulon-ubuntu2` (the owner's WotLK install at `/home/pk/wowserver`, native `243c46e3`,
rebuilt with `mod-ale`, 500 bots, his account `PERZI` and warrior `Pakka` on it).
**Tree:** `84734ea5`, the same commit checked out on the box and in the lane's worktree.
**Subject:** `manifests/wow-wotlk/modules/mod-arac.json` — `data/sql/db-world/arac.sql`,
`applied_by: direct`, `db: world`, `build.rebuild: false`.
**Driver:** `press.py`, one step per capture, run through the box's own `.venv`.
**Every action was announced on that box's activity terminal before it was taken.**
**Left running:** the whole stack, up, realm row set, authserver restarted, the six world tables
byte-for-byte back to what the owner left. See *How the box was left*.

---

## Verdict, clause by clause

| Clause | Verdict | Evidence |
| --- | --- | --- |
| the guard refuses a direct world-SQL step while the world runs | **PASS, first press** | `2-refuse-world-up.log`, `refusal-sentence.txt` |
| the refusal **names the step** | **PASS** — `sql data/sql/db-world/arac.sql → world` | `refusal-sentence.txt`, `2-modules-tab-refuses-world-up.png` |
| **no rows written** by the refused press | **PASS** — all 16 readings identical, counts *and* checksums | `counts-ground.json` vs `counts-after-refusal.json` |
| it applies once the server is stopped | **PASS for the engine, by no route a user has** | `6-press-applies.log`, and *The dead end* below |
| the counts move when it does apply | **PASS** — four tables by count, all six by checksum, both UPDATE predicates to zero | `counts-after-success.json`, `9-vm-desktop-counts-ground-to-restore.png` |
| a second press changes nothing | **PASS** (idempotence, not a checklist clause) | `7-press-again-idempotent.log` |
| the app's Stop and Start drive the world | **PASS** — down in 19.4 s, up and `ready = True` | `3-stop.log`, `10-start.log`, `12-ready.log` |
| the module removes, and its rows stay | **PASS, as designed** (`remove()`: *"DB rows are kept"*) | `8-remove.log` |
| the box goes back to the ground | **PASS** — restored from a pre-press dump, every count and checksum equal | `9-restore.log` |

---

## The ground, read before anything was written

`1-ground.log`, 08:02:33Z. `ac-worldserver running pid=308725 restarts=0`, `ac-database` healthy,
`ac-authserver` up; `modules/mod-arac` absent, no `mod_arac` conf, no `.arac_sql_applied` marker;
the applier is the Modules tab's own (`DockerSql`, `client_dir=None`, `dbc=None`).

**The ground is the whole reason this press proves anything.** Two of `arac.sql`'s six tables are
UPDATEd rather than inserted into, so their row *count* cannot move at all — and if the ground had
already satisfied those two predicates, a successful press would have photographed exactly like one
that wrote nothing. It had not:

```
skills_still_to_change   5     rows still matching  skill IN (45,46,160,173,226) AND classMask<>0 AND raceMask<>0
quests_still_to_change   635   rows still matching  a.allowableclasses<>0 AND t.AllowableRaces<>0 AND t.AllowableRaces<>1791
totem_rows_for_arac_races  0   rows in player_totem_model for RaceID IN (1,4,5,7,10)
```

Each is a count of *rows this file has not changed yet*. All three had work to do, and the file's
own DELETE + INSERT for the totems had nothing of its own to overwrite.

## The counts, at every stage

`counts-table.txt`, built from the saved JSON rather than retyped;
`9-vm-desktop-counts-ground-to-restore.png` is the same table on the VM's own desktop, photographed
from the Hyper-V host, with `ac-worldserver: running pid=409560 restarts=0` printed underneath it.

| reading | ground | after refusal | after success | after remove | after restore |
| --- | --- | --- | --- | --- | --- |
| rows playercreateinfo | 62 | 62 | 100 | 100 | 62 |
| rows playercreateinfo_action | 283 | 283 | 488 | 488 | 283 |
| rows playercreateinfo_skills | 77 | 77 | 77 | 77 | 77 |
| rows playercreateinfo_spell_custom | 0 | 0 | 7219 | 7219 | 0 |
| rows player_totem_model | 20 | 20 | 40 | 40 | 20 |
| rows quest_template | 9464 | 9464 | 9464 | 9464 | 9464 |
| checksum playercreateinfo | 1375550933 | 1375550933 | 773387713 | 773387713 | 1375550933 |
| checksum playercreateinfo_action | 1810990106 | 1810990106 | 3925301461 | 3925301461 | 1810990106 |
| checksum playercreateinfo_skills | 1381324222 | 1381324222 | 4287417897 | 4287417897 | 1381324222 |
| checksum playercreateinfo_spell_custom | 0 | 0 | 840767378 | 840767378 | 0 |
| checksum player_totem_model | 58188735 | 58188735 | 3963586888 | 3963586888 | 58188735 |
| checksum quest_template | 4127409649 | 4127409649 | 1875883529 | 1875883529 | 4127409649 |
| skills_still_to_change | 5 | 5 | 0 | 0 | 5 |
| quests_still_to_change | 635 | 635 | 0 | 0 | 635 |
| quests_at_1791 | 12 | 12 | 647 | 647 | 12 |
| totem_rows_for_arac_races | 0 | 0 | 20 | 20 | 0 |

`playercreateinfo_skills` and `quest_template` are the two whose row counts are constant by
construction. Their checksums moved and their predicates went 5 → 0 and 635 → 0, which is the only
shape in which those two tables can prove anything.

The six tables were derived from the SQL file itself (`0-tables.log`), not from prose:
`playercreateinfo`, `playercreateinfo_action`, `playercreateinfo_skills`,
`playercreateinfo_spell_custom`, `player_totem_model`, `quest_template`.
`quest_template_addon` is joined by one predicate and never written.

---

## 1 — the refusal, world up (08:02:57Z)

`2-refuse-world-up.log`. `world_running()` answered `True` from a real `docker inspect` before the
press, and the press raised:

```
mod-arac: the world server is running, and it holds world in memory and writes back over whatever
it finds there. No SQL was run and no rows were written: sql data/sql/db-world/arac.sql → world.
Press Stop, then install again — the steps this run already took repeat, and the SQL follows them.
```

`2-modules-tab-refuses-world-up.png` is that sentence in the app's own Modules tab, rendered
offscreen through the tab's own failure slot (`_module_failed`), which prefixes it
`install mod-arac FAILED:`. `ac-worldserver` was `running pid=308725 restarts=0` at the capture.

**The counts are the clause, and they did not move** — not one of the sixteen, counts and checksums
alike. A refusal that had already sent three of `arac.sql`'s statements would photograph exactly
like this one; the checksums are what tell them apart.

**What the refusal did not stop, correctly:** the clone. `modules/mod-arac` went from absent to a
full checkout with `.yulon-clone.json` and `include.sh` during the refused press, because the guard
is a pre-pass over the SQL step and `install()` clones, deploys and patches first. The sentence says
so itself — *"the steps this run already took repeat, and the SQL follows them"* — and this is the
guard being scoped rather than a blanket "nothing while the world runs".

## 2 — the dead end the refusal walks into (08:03:40Z – 08:05:21Z)

The refusal says *Press Stop, then install again*. Pressed exactly that way, through the app's own
controls:

* `3-stop.log` — `svc.controller.stop()` returned `True` in **19.4 s**; `compose stop` took the
  whole project down: `containers up = ` (nothing), `ac-database exited`.
* `4-press-world-stopped.log` — the same install pressed again, world down, and it **failed**:

```
SQL failed (arac.sql → acore_world): Error response from daemon:
container 3c922e8e7c78d62c2ab93177595b2cd99d168cd738e392ca1b1e80d6e83e1b5f is not running
```

This is [bug-checklist §46](../../bug-checklist.md) — *"On CMaNGOS there is no compliant way to
install a SQL mod at all"* — **happening on AzerothCore**, in the same words and with the same
64-character container id. Two facts this press adds to that entry:

* **It is not a CMaNGOS property.** The entry's title scopes it to that family; the mechanism is
  `stop_staged()` + `DockerSql`'s `docker exec`, which are shared, so every family has it. WotLK is
  now measured.
* **On this family the missing capability already exists as a function.**
  `docker.start_database()` — `compose up -d --no-deps ac-database`, waited healthy — put the
  database back **alone, with the world still down, in 6.6 s** (`5-database-alone.log`), which is
  exactly the "world down, database up" state §46 says the app has no way to reach. Its own comment
  predicted this use: *"Started rather than demanded, because Stop takes the database down with
  everything else — a user who followed the repair refusals would otherwise have no way back to a
  state that action accepts."* What is missing on the direct-SQL route is a **caller**, not a
  primitive: `apply_module_sql()` (the db-import route) calls it and the applier route does not.

Nothing was written by the failed press — the counts read after it were unreadable for the same
reason the write was, and the next press's own ground reading (`6-press-applies.log`, top) is the
ground values 62 / 283 / 0 / 20 / 9464, so nothing had moved.

## 3 — the success, world stopped (08:06:00Z)

`6-press-applies.log`, with `ac-worldserver exited` and `ac-database running` at the capture. The
report the app produced (`success-report.txt`, drawn in
`6-modules-tab-applies-world-stopped.png`):

```
install mod-arac:
  ✓ clone https://github.com/heyitsbench/mod-arac.git → modules/mod-arac
  ✓ sql data/sql/db-world/arac.sql → world
  – skipped: client Patch-A.MPQ: no client dir configured
  – skipped: server_dbc patch-contents/DBFilesContent: no DBC copier configured
  ⚠ Press Stop and then Start on the Server tab to apply this.
```

Fourteen of the sixteen readings moved (the two constant-by-construction row counts did not), and
the two UPDATE predicates went to zero. A second press at 08:06:58Z
(`7-press-again-idempotent.log`) changed **nothing** — `INSERT IGNORE` and idempotent UPDATEs — so
the route is safe to repeat, which is worth knowing for a manifest whose upstream marker
(`.arac_sql_applied`) this app does not write.

**Two of the module's four halves were skipped, by the app's own wiring, not by this gate.**
`controller_view.py` builds the WotLK applier as `applier(server_dir, sql=sql, client_dir=…)`, and
`wotlk_modules.applier()`'s `dbc=` parameter is filled by **no caller anywhere in the tree**
(grepped, 2026-09-09), so a `server_dbc` step can never run in the shipped app; and this press
passed no client dir. ARAC without its DBC and MPQ is not a working module — this gate is about the SQL route
and says nothing about the rest of it.

## 4 — the box, put back (08:07Z – 08:38Z)

* `8-remove.log` — `applier.remove(manifest)` deleted `modules/mod-arac` and **kept every row**,
  which is `remove()`'s documented behaviour (*"Run remove-time patches/SQL, delete deployed files
  and the clone. DB rows are kept"*) and this manifest has no remove-time SQL. So *"back to the
  ground"* was **not** true after the remove, and could not be: the applier has no undo for this.
* `9-restore.log` — restored from `mysqldump` of exactly those six tables, taken before press 1
  (`1b-dump.log`, 7 038 460 bytes, kept on the box at `~/arac-gate-out/`, not committed here) and
  loaded with the world still down, because writing those tables under a live world is the very
  thing the guard forbids. Result: **every count and every checksum equal to the ground reading.**
* `10-start.log` — `svc.controller.start()`, then `11-realm-and-auth.log`, then `12-ready.log`.

---

## How the box was left

| Thing | State |
| --- | --- |
| `ac-worldserver` | **running**, `pid=409560 restarts=0`, started 08:08:24Z |
| `ac-authserver` | **running**, `pid=455691`, restarted 08:37:06Z after the realm row was written |
| `ac-database` | **running**, healthy |
| readiness | `wait_ready_for(spec, azerothcore_ready('100.99.204.5', 8085))` = **True** in 0.2 s |
| realm row | `1  Yulon ubuntu2  100.99.204.5  100.99.204.5  255.255.255.0  8085` — address and localAddress were already right; **`localSubnetMask` was `255.255.255.255` and is now `255.255.255.0`** |
| the six world tables | identical to the ground, count and checksum |
| `modules/` | `mod-ale`, `mod-playerbots` — `mod-arac` cloned and removed, nothing else changed |
| `worldserver.conf` | **untouched** — `mtime 2026-09-08 23:11:29`, hours before this lane, with `Logger.ALE=4,Console Server` still at line 706 (`14-final-state.log`) |

**The owner's things, listed and read only** (`13-owner-untouched.log`):

* `/home/pk/LootPet.lua` — 31 216 bytes, mtime **2026-09-08 22:17:42**, unchanged by this lane.
* account `PERZI` — id 102, `last_login 2026-09-08 23:24:16`, `joindate 2026-09-08 20:28:14`,
  verifier and salt both 32 bytes. **No password was set, reset or read**; the lane created no
  account and used none. (The lengths are printed; no hash, salt or password value is in any file
  here.)
* character `Pakka` — guid 1001, level 6, race 2, class 1, offline, `logout_time 1788910611`.
  `acore_characters` was never written by anything in this gate; `arac.sql` touches `acore_world`
  only.

---

## What this press still did not show

1. **That any user is protected.** The seam is not wired in the shipped app. `press.py` attaches
   `Applier._world_running` itself, because `apply.py:1920` records that no caller passes it —
   *"until then this is a capability, not a defence."* Pressed through the Modules tab as it ships
   today, this same install would have written those 7 219 rows into a live world without a word.
   **This is the one thing between 8.7a's second clause and a tick on the feature.**
2. **The `None` branch.** Only `True` and `False` were ever returned by the seam. The refusal
   written for *"could not tell whether the world server is running"* was never seen live, and
   neither was the `logger.warning` beside it.
3. **`characters` and `playerbots`.** `WORLD_HELD_DBS` has three members and only `world` was
   pressed. Two shipped manifests carry a direct `characters` step at install time —
   `wow-wotlk/ale/accountwide.json` and `wow-wotlk/ale/battlepass.json` — and neither was pressed;
   no shipped manifest writes `playerbots` directly at all, so that member of the set has never had
   a subject.
4. **The pre-pass claim.** The guard's docstring justifies being a pre-pass with `all-stackables`,
   *"three statements to `world` on install"*, and that manifest was not pressed — the half-applied
   case the design is defending against was reasoned about, not demonstrated.
5. **`remove` and `configure`.** `mod-arac` has no remove-time or configure-time SQL, so the guard
   was never exercised on the two actions whose refusals `remove()`'s ordering comment is about.
6. **A hand on the keyboard.** Both app-surface captures are the real tab, with the real strings,
   through the tab's own slots, rendered offscreen — not a person clicking Install.
7. **The module's in-game effect.** No client was driven; the DBC and MPQ halves were skipped by the
   app's wiring (above), and the rows were removed again within minutes.

**A note for whoever wires this seam.** The tree's one existing `world_running` wiring is My Party's
(`controller_view.py`: `docker.container_state(spec.world).settled`). `container_state()` returns an
empty `ContainerState` when Docker will not answer, and `.settled` turns that into `False` — which
through *this* guard is fail-**open**: "not running", the one answer that lets the SQL through. The
guard's own contract is three-valued and fails closed, so a wiring that reuses that expression
verbatim would quietly discard the `None` branch. `press.py::world_running()` maps a blank status to
`None` for that reason.

---

## Corrections to the ticket

* **"the only shipped manifest with a direct world-SQL step" is wrong**, and the ticket asked for
  the glob rather than the sentence. `applied_by` **defaults to `"direct"`**
  (`yulon/manifest.py:136`), so every step that omits it is one. Counting every step whose effective
  `applied_by` is `direct` and whose `db` is in `WORLD_HELD_DBS`: **43 steps across 18 manifests, in
  all four games** — `all-stackables` in four games, eight more `wow-wotlk` mods, `battlepass`,
  `paragon`, `bmah`, `accountwide` (that one `characters`), and `mod-arac`. What is true of
  `mod-arac` is narrower: it is the only **`module`-type** manifest with such a step, and the only
  one whose SQL is a file inside a cloned module. The clause was pressed on the right subject; the
  blast radius of the guard is eighteen manifests, not one.

## Deviations

* **The six tables were restored from a dump.** The ticket asked me to re-read the counts after the
  remove and *"say so if they are not"* back to the ground. They were not, by design, so a
  `mysqldump` of exactly those tables was taken before press 1 and loaded back afterwards. Leaving
  ARAC's rows in the owner's live world — every race able to be every class, 635 quests' race masks
  rewritten — was not an acceptable way to hand the box back.
* **Start comes after the remove and the restore, not between the success and the remove.** The
  ticket's order would have needed three stop/start cycles of a 500-bot server to keep every write
  under a stopped world; this order needs one, and every write to those six tables happened with the
  world down, which is the rule the whole gate is about.
* **A fourth press (idempotence) that the ticket did not ask for**, taken because it costs one run
  and it is what produces the app's own `_format_report` text for the success screenshot.
* **`10-start.log` has no readiness line.** The first readiness wait was handed `127.0.0.1`, and
  `azerothcore_ready()` matches `<realm_host>:<realm_port>` in the **auth** log, which on this box
  prints `100.99.204.5:8085` — so it could never have matched, and its quiet budget never expired
  because a 500-bot world prints constantly. My mistake, not the app's. It was killed and the wait
  re-asked on its own against the realm's real address (`12-ready.log`, `ready = True` in 0.2 s)
  rather than by pressing Start again, which would have run `compose up -d` over a world that was
  already serving.
* **The 7 MB table dump is not committed.** It lives on the box at
  `~/arac-gate-out/world-six-tables-before.sql`; what is here is the counts and checksums that make
  it checkable.

## Files

| File | What it is |
| --- | --- |
| `press.py` | the driver; one step per capture, run on the box through its own `.venv` |
| `show.sh` | puts one reading on the VM's desktop for `vmshot.ps1` (GNOME blocks an in-guest shot from ssh) |
| `0-tables.log` | the six tables, derived from `arac.sql` itself |
| `1-ground.log`, `1b-dump.log` | the ground, and the restorable copy taken before anything was written |
| `2-refuse-world-up.log`, `refusal-sentence.txt`, `2-modules-tab-refuses-world-up.png` | the refusal |
| `3-stop.log`, `4-press-world-stopped.log`, `press2-failure-sentence.txt` | the app's Stop, and the dead end |
| `5-database-alone.log` | `start_database()` — the way out, 6.6 s |
| `6-press-applies.log`, `success-report.txt`, `6-modules-tab-applies-world-stopped.png` | the success |
| `7-press-again-idempotent.log` | the same press again, changing nothing |
| `8-remove.log`, `remove-report.txt`, `8-modules-tab-after-remove.png` | the remove, and the rows it keeps |
| `9-restore.log`, `9-vm-desktop-counts-ground-to-restore.png` | back to the ground, and that table on a real screen |
| `10-start.log`, `11-realm-and-auth.log`, `12-ready.log`, `13-owner-untouched.log`, `14-final-state.log` | the box handed back |
| `counts-*.json`, `counts-table.txt` | every reading, as saved by the driver |
