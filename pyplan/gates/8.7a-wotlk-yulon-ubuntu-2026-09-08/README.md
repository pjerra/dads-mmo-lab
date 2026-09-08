# 8.7a live — Modules, WoW WotLK, on `yulon-ubuntu`

**Box:** `yulon-ubuntu`, AzerothCore WotLK at `/home/pk/wowserver`, the 7.2 install six later
boxes are gated against. **Date:** 2026-09-08, 10:48–11:11 VM local.
**Tree:** this lane's worktree at `9516725c`, exported with `git archive` to `~/gate87a-lane3`.
**Python:** `~/laneA/.venv/bin/python` (PySide6, offscreen).
**Checkpoint taken before anything was written:** `8.7a-before-2026-09-08` on the Hyper-V host.
The two 8.9a checkpoints were not touched.
**Left running:** the whole stack, up, 500/500 bots logged in. See *What this run changed* below.

Every capture is the app's **Modules tab**, taken through `ControllerView` built by
`ControllerServices.for_entry()` against the live install — no fakes below the view — and every
one records whether `ac-worldserver` was alive at that instant, because a screenshot of a server
that has died photographs exactly like a refusal.

---

## Verdict, clause by clause

| Definition-of-done clause | Verdict | Evidence |
| --- | --- | --- |
| the commits-behind figure equals the same query run by hand | **PASS** | `1b-commits-behind-nonzero.png`, `14-behind-nonzero.log` |
| a module whose SQL targets the world database is refused while the server runs, with the step named and **no rows written** | **PASS** | `3-refused-world-up.png`, `11-cycle2-up.log` |
| …and applies once the server is stopped | **PASS** | `5-applied-world-down.png`, `12-cycle2-apply.log` |
| a module whose SQL is marked for the import one-shot is either applied or reported as not applied — never logged as done while nothing ran | **PASS** | `2-pending-not-done.png`, `02-install.log`, `11-cycle2-up.log` |
| a configuration change the module needs is shown by the running server, not only read back from the file | **NOT MET** | `8-config-and-the-running-server.png`, `13-cycle2-running.log` — measured reason below |

Four of five. **The box is not tickable on this evidence**, and the fifth clause is not a matter
of running the gate again: it is blocked on the rebuild the owner runs, and on a defect in the
manifest layer. Both are written out below with the exact command that would settle them.

---

## The ground each step started from

Read before anything was done (`11-cycle2-up.log`, and `01-behind.log` for the first cycle):

| Fact | At the start |
| --- | --- |
| containers | `ac-authserver`, `ac-client-data-init`, `ac-database`, `ac-db-import`, `ac-worldserver` |
| import service | `ac-db-import` |
| `allowed_modules()` | `'mod-playerbots'` — the folder on disk, not the image |
| `importer_sees_modules()` | `True` — this install's compose file binds `./modules` into the importer |
| `updates` rows | world **2974**, characters **31**, auth **23** |
| `mod-npc-beastmaster` on disk | **False** |
| creature 601026 in `creature_template` | **0** |
| `Creatures.CustomIDs` in `worldserver.conf` | `"190010,55005,999991,25462,98888,601014,34567,34568"` — does **not** contain 601026 |

Every assertion this gate makes was false before its action ran. That is the whole reason the
ground is a table rather than a sentence.

---

## 1 — how far behind each installed module is

`1-commits-behind.png` is the first press: `mod-npc-beastmaster: 0 commits behind`,
`mod-playerbots: 0 commits behind`. **Both zero, and zero is what a clone made ten minutes ago
always is** — a step whose assertion is already true proves nothing, so the figure was made
non-trivial and re-read:

```
HEAD before the rewind: 5ef5708  (mod-1v1-arena)
is-shallow-repository: 'true'    the app clones modules shallow, so HEAD~3 does not exist
fetch --deepen 10 -> rc=0
reset --hard HEAD~3 -> HEAD is now at 2b85db9
```

`1b-commits-behind-nonzero.png` — the app, pressed:

```
mod-1v1-arena: 4 commits behind
mod-npc-beastmaster: 0 commits behind
mod-playerbots: 0 commits behind
```

and the same question run by hand, in the same run:

```
BY HAND mod-1v1-arena:       rev-list --count HEAD..FETCH_HEAD = '4' (app said 4)
                             HEAD=2b85db9 FETCH_HEAD=5ef5708
BY HAND mod-npc-beastmaster: '0' (app said 0)   HEAD=99dbd9b FETCH_HEAD=99dbd9b
BY HAND mod-playerbots:      '0' (app said 0)   HEAD=b949b50 FETCH_HEAD=b949b50
```

Four is four, on a checkout that really is behind, against three modules of which two are not.
The clone was put back to `5ef5708` immediately afterwards (last lines of `14-behind-nonzero.log`).

**The shallow clone is worth recording**, because it is the reason a rewind of this kind needs a
`--deepen` first and the first attempt silently did nothing: `git reset --hard HEAD~3` in a
depth-1 clone fails, and a gate that does not check the return code reports "HEAD before" and
"HEAD after" as the same commit and carries on. That failure is in `03-refused.log` from the
first cycle, kept rather than deleted.

## 2 — a step nothing ran is never reported as done

`2-pending-not-done.png`, and `02-install.log` for the module with real files:

```
install mod-npc-beastmaster: 3 step(s), 0 skipped, 2 left unapplied, rebuild=True
--- ApplyReport.done ---
  clone https://github.com/azerothcore/mod-npc-beastmaster.git → modules/mod-npc-beastmaster
  touch include.sh
  activate env/dist/etc/modules/mod_npc_beastmaster.conf from conf/mod_npc_beastmaster.conf.dist
--- ApplyReport.pending_sql ---
  db=world      path=data/sql/db-world/*.sql      files=['data/sql/db-world/beastmaster_tames.sql',
                'data/sql/db-world/beastmaster_tames_inserts.sql', 'data/sql/db-world/npc_beastmaster.sql']
  db=characters path=data/sql/db-characters/*.sql files=['data/sql/db-characters/track_tamed_pets.sql']
steps in `done` mentioning sql (must be none): []
```

The glob was resolved against the clone — four real filenames, which the importer then applied by
those same names in step 4 — and nothing about SQL is in `done`. This is the half the box calls
*"which is what happens today"*, and it does not happen any more.

**Three manifests were pressed, and two of them found nothing** (`11-cycle2-up.log`):
`mod-junk-to-gold` and `mod-learn-spells` both report `files=[]`, because those repositories ship
no `data/sql/db-world/` at all. That is the honest answer and it is the right one — an empty match
is reported as empty rather than guessed — but see *Defects* for what it says about the manifests.

## 3 — refused while the world is running, with the rows counted

`3-refused-world-up.png`. Ground: `ac-worldserver alive = True`, `ac-authserver alive = True`.

```
ledger BEFORE: {'world': 2977, 'characters': 32, 'auth': 23}
creature 601026 BEFORE: 1
FAILED: ac-worldserver, ac-authserver are running. The importer writes to the databases
underneath them, and a running worldserver holds characters in memory and saves them back over
whatever it finds. Press Stop first, then try again.
ledger AFTER:  {'world': 2977, 'characters': 32, 'auth': 23}
rows written by the refused press: {'world': 0, 'characters': 0, 'auth': 0}
containers created by the press: []
button enabled again: True  took 1.1s
```

The step is named (`ac-worldserver, ac-authserver are running`), the remedy is named
(`Press Stop first`), **no rows were written in any of the three databases**, and no container was
created — the refusal happens before `start_database()`, so nothing was even brought up to be
refused at. The same press was made in both cycles, an hour apart, with the same result
(`03-refused.log`, `11-cycle2-up.log`).

## 4 — and it applies once the server is stopped

`4-stopped.png` → `5-applied-world-down.png`, `12-cycle2-apply.log`. Stopped through the app's own
Stop (45.4 s in the first cycle, with the pre-stop log snapshot written by `logsnap`), then the
same button pressed again:

```
ledger with the database still UP: {'world': 2977, 'characters': 32, 'auth': 23}
apply_module_sql(): running ac-db-import for modules: mod-1v1-arena,mod-npc-beastmaster,mod-playerbots
  Updating Auth database...       >> Auth database is up-to-date!
  Updating Character database...  >> Character database is up-to-date!
  Updating World database...      >> Applying update "1v1_Battlemaster.sql" '8CD4090'...
                                  >> Applied 1 query.
ledger AFTER: {'world': 2978, 'characters': 32, 'auth': 23}
rows written: {'world': 1, 'characters': 0, 'auth': 0}
```

and in the first cycle, four files in one run (`04-apply.log`):

```
  >> Applying update "track_tamed_pets.sql"          (characters)
  >> Applying update "beastmaster_tames.sql"         (world)
  >> Applying update "beastmaster_tames_inserts.sql" (world)
  >> Applying update "npc_beastmaster.sql"           (world)
world 2974 → 2977, characters 31 → 32, auth 23 → 23
```

The ledger before the stop is read **while the database is still up**, and that is not tidiness:
the app's Stop takes `ac-database` down with the servers, so a count taken after it answers `-1`
for every schema. The first cycle recorded exactly that and then printed a difference against it,
which read as a plausible number (`2978`) rather than as a failed read. `04-apply.log` still
carries the bad line; the fix and its reason are in `gate87a.py::step_stop`.

`allowed_modules()` is what makes this work at all: it moved from `'mod-playerbots'` to
`'mod-1v1-arena,mod-npc-beastmaster,mod-playerbots'` as the modules arrived, and the importer's own
header repeats the list it was given. Without it the run logs `Loading modules: all` and applies
nothing (measured on this box 2026-09-07).

## 5 — the running server, and the one clause this box cannot meet

`6-started.png` (the Server tab after the restart) → `7-running-server-states-it.png` →
`8-config-and-the-running-server.png`, `13-cycle2-running.log`.

**What the running server does show.** Asked through the app's own console seam, the restarted
world knows the creature the app's module SQL added:

```
> .lookup creature White Fang
  601026 - White Fang
```

and it states worldserver.conf values back:

| `worldserver.conf`, on disk | `.server debug`, from the running server |
| --- | --- |
| `vmap.enableLOS = 1`, `vmap.enableHeight = 1`, `vmap.enableIndoorCheck = 1` | `VMAPs status: Enabled. LineOfSight: true, getHeight: true, indoorCheck: true` |
| `MoveMaps.Enable = 1` | `MMAPs status: Enabled` |
| `DBC.Locale = 255` | `Default DBC locale: enUS.` |

So "shown by the running server" is a question with an answer on this tree. It is the *module's*
configuration that cannot reach it, for a reason measured here rather than guessed:

```
List of enabled modules:
|- mod-playerbots

Loading Modules Configuration...
> Config::LoadFile: Failed open file '/azerothcore/env/dist/etc/modules/playerbots.conf'
> Not found modules config files
```

`env/dist/etc/modules/mod_npc_beastmaster.conf` and `1v1arena.conf` are **on disk**, activated by
the app's own install, sitting in that very directory — and the server does not open them. It asks
for `playerbots.conf` **by name**, because AzerothCore looks module conf files up by the names of
the modules **compiled into the binary** (`Acore::Module::GetEnableModulesList()`, the same list
`.server debug` prints). A module that has not been compiled in has no name on that list, so its
conf file is invisible however correct it is.

**The consequence, stated plainly:** on WotLK, a newly installed module's configuration cannot be
shown by the running server until the worldserver has been rebuilt with that module. This clause
is gated on the rebuild, and the owner runs rebuilds. The command that would settle it, run in
`/home/pk/wowserver` on `yulon-ubuntu`:

```
docker compose -f docker-compose.yml -f docker-compose.build.yml build ac-worldserver \
  && docker compose up -d --no-deps ac-worldserver
```

After it, `.server debug` should list `mod-npc-beastmaster` and `mod-1v1-arena` among the enabled
modules, `Loading Modules Configuration...` should open their two conf files by name instead of
failing, and the module's own in-game effect (the White Fang NPC, `.beastmaster`) becomes
reachable — which is also this box's *Visible effect*. **This lane did not run it**, per the
standing rule.

---

## Defects this run found

1. **A conf key a manifest declares with no value is never written.**
   `mod-npc-beastmaster.json` names `Creatures.CustomIDs` on `env/dist/etc/worldserver.conf` with a
   note — *"add 601026 to silence a harmless gossip warning"* — and no `default`. The install
   reports `activate … mod_npc_beastmaster.conf` and nothing about `worldserver.conf`, and the file
   is byte-identical before and after: still
   `"190010,55005,999991,25462,98888,601014,34567,34568"`, still without `601026`
   (`11-cycle2-up.log`, both readings). So the one *core-side* configuration change this module
   needs — the only one a running server could show without a rebuild — is documented in the
   catalog and not made by the app. That is the same shape as the defect 8.7a's other half was
   opened for: a manifest that names a step nobody takes.
   The harm here is currently nil, incidentally, and only by luck: this module's creature row is
   `flags_extra = 2` (`CREATURE_FLAG_EXTRA_MODULE`), which suppresses the warning on its own
   (`ObjectMgr.cpp:1219-1229`, read on the box), and `gossip complaints in this boot: 0`.

2. **Two shipped manifests declare a SQL step for a module that ships no SQL.**
   `mod-junk-to-gold` and `mod-learn-spells` both declare `data/sql/db-world/*.sql` with
   `applied_by: db-import`; both clones contain no `.sql` file at all. The report says so honestly
   (`files=[]`) rather than guessing, which is the behaviour 8.7a asked for — but a user reading
   the tab sees a pending SQL step for a module that has none.

3. **`mod-1v1-arena`'s manifest names the wrong database and the wrong depth.**
   It declares `db=characters path=data/sql/db-characters/*.sql`; the repository ships
   `data/sql/db-world/base/1v1_Battlemaster.sql` and `data/sql/delete/1v1_delete.sql`. So
   `pending_sql` is empty while the importer — which walks the module's `data/sql` tree itself —
   applied `1v1_Battlemaster.sql` to the **world** database. The apply is correct; the report of
   what was owed was not. A flat `*.sql` glob cannot see a file one directory deeper.

4. **Removing a module leaves its activated conf behind.**
   `remove mod-learn-spells` reported one step, `rm -r modules/mod-learn-spells`, and
   `env/dist/etc/modules/mod_learnspells.conf` is still there. Harmless on this tree — the server
   does not open a conf for a module it was not compiled with, which is finding 5's mechanism —
   but it is a file the app wrote and does not take back.

None of these are in `pyplan/bug-checklist.md` yet.

---

## What this run changed on the box, and how to put it back

* **`mod-npc-beastmaster` and `mod-1v1-arena` are installed** in `/home/pk/wowserver/modules/`,
  with `mod_npc_beastmaster.conf` and `1v1arena.conf` activated. They are **left in place**: their
  SQL is already in the databases and cannot be un-applied, so removing the clones would leave
  orphan rows — worse than either end state. **A rebuild of this install will now compile them in.**
* `acore_world.updates` 2974 → **2978** (`beastmaster_tames.sql`, `beastmaster_tames_inserts.sql`,
  `npc_beastmaster.sql`, `1v1_Battlemaster.sql`); `acore_characters.updates` 31 → **32**
  (`track_tamed_pets.sql`); `acore_auth.updates` unchanged at 23.
* `creature_template` gained entry 601026 (White Fang) and the 1v1 Battlemaster.
* `env/dist/etc/modules/mod_learnspells.conf` is left over from a module that was installed and
  removed again (defect 4).
* Nothing else. `worldserver.conf` is untouched; no account, character or bot was written.

To put all of it back:

```
ssh vmhost 'Restore-VMSnapshot -VMName yulon-ubuntu -Name "8.7a-before-2026-09-08" -Confirm:$false'
```

To drop just the two modules without restoring (the SQL rows stay):

```
ssh yulon-ubuntu 'rm -rf /home/pk/wowserver/modules/mod-npc-beastmaster /home/pk/wowserver/modules/mod-1v1-arena'
```

## The limit of this route, said before anyone assumes otherwise

`importer_sees_modules()` answered **True** here: this install's compose file binds `./modules`
into `ac-db-import`. On a server built by the **DML bash installer** it answers False — that shape
uses the stock upstream `acore/ac-wotlk-db-import:master` image with **no `./modules` mount at
all**, so this route cannot help it, and `docker.apply_module_sql()` refuses before running
anything rather than exiting 0 having applied nothing. Both compose captures are in
`pylauncher/tests/data/` (`wotlk-compose-config.json`, `wotlk-compose-config-script.json`) and the
comparison is in the 2026-09-08 UI gate's README.

## Reproducing

```
scp gate87a.py <box>:~/gate87a-lane3/          # beside an exported pylauncher/
ssh <box> 'cd ~/gate87a-lane3 && GATE_MODULE=<module-id> ~/laneA/.venv/bin/python gate87a.py \
    ground behind install refused stop apply start running config'
```

Steps are selected by argv on purpose: `stop`/`apply`/`start` are the long ones, and a gate that
times out half-way through them leaves a server down.
