# 8.7d — Modules, WoW Tortoise — **RUN 2026-09-08 11:25Z–11:30Z on m910q**

> **This file is a record, not a plan.** Every line below was produced against the
> **live** Tortoise stack on m910q — `tortoise-db` / `tortoise-realmd` /
> `tortoise-mangosd`, on the **pre-rebuild** image
> `yulon.local/cmangos-tortoise-server:native-58c6fd1c`. `transcript.txt` is the
> stages' output with a UTC clock on every line and the worldserver's own
> `status=` beside every capture; `1-*.png … 6-*.png` are the screenshots those
> stages wrote, pulled off the box unedited; `gate87d.py` is what ran.
>
> **All five clauses passed.** Nothing was rebuilt, the source checkout was not
> moved, and the box was left with the Tortoise stack up and
> `etc/mangosd.conf` byte-identical to what it was at 11:25Z.

**The box.** `pyplan/checklist.md:2504` — *"Modules, WoW Tortoise — as 8.7b, and
nothing may run this fork's own database auto-update path while the world is up.
Definition of done: as 8.7b [at least one manifest installs and its key is
reported by the running server after the restart it asks for]. Gate: `m910q`;
evidence `pyplan/gates/8.7d-tortoise-m910q-<date>/`. Visible effect: the module's
own in-game effect after the restart it asks for."*

| clause | verdict | where |
|---|---|---|
| (1) a manifest set and a binding for this game | **PASSED** | `transcript.txt` 11:25:48Z; `1-modules-listed.png` |
| (2) at least one manifest installs | **PASSED** | 11:25:57Z; `2-installed.png` |
| (3) its key is reported by the running server **after the restart it asks for** | **PASSED** | 11:26:01Z (still 600, no restart) → 11:27:23Z (120, after); `3-reported-after-restart.png` |
| (4) **nothing may run this fork's auto-update path while the world is up** | **PASSED** | 11:29:07Z–11:29:11Z; `4-refused-while-armed.png`, `5-permitted-again.png` |
| (5) the item is reversible | **PASSED** | 11:29:12Z, byte-identical conf; `6-removed.png` |
| visible effect | **PASSED, and narrower than the manifest first claimed** | 11:12:47Z, `.perf cpu` — see *What the visible effect really is* |

**Which tree ran.** The gate was run three times and only the third is recorded
here. Run 1 (11:07Z–11:17Z) passed every clause and then produced the finding
below — that `Perf.ReportInterval` fires an empty function — so the manifest's
name, description and notes were corrected. Run 2 (11:13Z–11:17Z) re-took the
screenshots against the corrected manifest. Run 3, **this one**, is against the
final committed tip, after `black`, `ruff --fix` and one mypy annotation touched
`autoupdate.py`; nothing behavioural changed between runs 2 and 3, and re-running
rather than asserting that is the point. The tree was exported with `git archive`
and unpacked to `~/gate87d/pylauncher`:

```
bea06c21aa99cbdb47c02405eda0d0c2  yulon/controller_wow_tortoise/autoupdate.py
f877e68d1c441d82744ad84d46b02b73  yulon/controller_wow_tortoise/modules.py
4219085e7e2c9d6d3446051747449a8d  yulon/ui/controller_view.py
677a6c807694f09cd21f065b18396d5c  manifests/wow-tortoise/mods/perf-report.json
```

Three readings in `transcript.txt` are still run 1's, because no later run could
improve them and each is labelled where it appears: the by-hand migration
subtraction (11:08:03Z), the worldserver's own `[DB Auto-Updater]` lines from the
11:08:34Z start, and the `.perf cpu` capture (11:12:47Z).

**A warning for the next lane, paid for here.** The first `--checks` run of this
lane reported `ALL GREEN` over a tree that **did not contain any of the new
files** — it was launched before the commit, and the helper syncs tracked files
only, so it linted 183 files instead of 186. The same command after the commit
found five ruff errors, one mypy error and two files black would reformat.
Commit first, then gate, and read the file count.

---

## Why this box could not inherit TBC's answer

8.7b proves its clause with `motd`, because `.server motd` makes the CMaNGOS
console state the value back. **That command does not exist on this fork.** Its
`serverCommandTable` is `corpses / exit / idlerestart / idleshutdown / info /
resetallraids / restart / shutdown` (`src/game/Chat/Chat.cpp:700-711`, read on
m910q at `~/tortoise-server/src/tortoise-wow`, `3a8472e`), and `motd` appears in
no command table at all. Nor does anything else obvious: a scan of
`src/game/Commands/Commands.cpp` for a `PSendSysMessage` within four lines of a
`getConfig` turns up **eight hits, of which exactly two report a configured
value** — `HandlePerfEnable` and `HandlePerfIntervalReport`.

So the manifest written for this game is `perf-report`, and the proof is

```
.perf intervalreport            ->  Performance report interval is <n>
```

* the handler: `Commands.cpp:19146-19154`, printing
  `sWorld.getConfig(CONFIG_UINT32_PERFORMANCE_REPORT_INTERVAL)`;
* the command row: `Chat.cpp:837`, `{ "intervalreport", SEC_ADMINISTRATOR,
  true, ... }` — that `true` is `allowConsole`, so the mangosd console answers
  with **no client and no logged-in character**;
* the conf key: `Perf.ReportInterval`, `World.cpp:1575`.

`motd` still ships in the Tortoise set, because it is a thing users want; its
manifest records in its own notes that on this tree only a logging-in client
reports it (`src/game/Handlers/CharacterHandler.cpp:786-809`), so it is not what
this gate leaned on.

---

## Clause (3), the definition of done, with its clocks

Everything below is `transcript.txt`, UTC. m910q's local clock is CEST, so the
same instants read 13:xx in `claude-say`'s output.

| when | what | the server's own words |
|---|---|---|
| 11:25:56Z | ground, nothing installed | `Performance report interval is 600` |
| 11:25:57Z | Install pressed **on the Modules tab** | `✓ set 2 key(s) in etc/mangosd.conf` |
| 11:25:57Z | the file | `2196c2196 < Perf.ReportInterval = 600 --- > Perf.ReportInterval = 120` |
| 11:26:01Z | asked again, **without restarting** | `Performance report interval is 600` |
| 11:26:05–11:27:19Z | `controller.stop()` then `controller.start()`, ready in 73 s | |
| **11:27:23Z** | **asked again** | **`Performance report interval is 120`** |

The 11:26:01Z line is the one that makes the restart load-bearing rather than
decorative: the file had already changed and the running server had not moved.
`build.restart` on this manifest is therefore a fact and not a guess.

**Ground, read before the action and recorded:** `Perf.ReportInterval = 600` was
already in `etc/mangosd.conf` at 11:25:48Z and `.perf intervalreport` already
said 600, so the step's assertion (`120`) was not true before it ran.

Two honest details about that console reply:

* it is followed by `Incorrect syntax.` — `HandlePerfIntervalReport` prints the
  value and then `return false`, which is this fork's own behaviour and not an
  error about the value.
* the reply window carries the worldserver's whole stdout, which on a 500-bot
  install is hundreds of `SQL: SELECT ...` lines. `3-reported-after-restart.png`
  shows the answer at the bottom of that traffic, which is what the Console tab
  really looks like here.

---

## Clause (4) — the clause that belongs to this box alone

### What the auto-update path is, measured

Read on m910q from `~/tortoise-server/src/tortoise-wow` (`3a8472e`), 2026-09-08:

* `World::SetInitialWorldSettings()` calls `sAutoUpdater.ProcessUpdates()` and on
  a false answer logs `DB AutoUpdater FAILED, cancelling server.` and calls
  `exit(1)` — **`src/game/World.cpp:1987-1994`**. On this fork the updater **is**
  the worldserver, at startup, and one bad migration cancels the whole world.
  AzerothCore's `applied_by="db-import"` defers to `ac-db-import`, a *separate*
  one-shot container that can fail without taking a world down. The same word
  means something much more dangerous here.
* the switch is `Database.AutoUpdate.Enabled`, read as
  `sConfig.GetBoolDefault("Database.AutoUpdate.Enabled", true)` —
  **`src/shared/Database/AutoUpdater.cpp:493`**. **An absent key means ENABLED.**
* the folders come from `Database.AutoUpdate.Path` plus `AuthUpdateName` /
  `CharUpdateName` / `WorldUpdateName`. This install's are `auth` / `character` /
  `world`; the core's compiled fallbacks are `Logon` / `Char` / `World`
  (`AutoUpdater.cpp:499-501`), so a guard that inherited the fallbacks would look
  in three directories that do not exist and report a confident zero.
* a migration is keyed `<module>:<sha1 of the file bytes>` against a `migrations`
  table **in each target database** (`AutoUpdater.cpp:83-86`, `:114-127`, `:443`).
  Files are the `*.sql` directly inside a folder, not recursively
  (`directory_iterator`, `:102-106`). Rows with no file are old migrations and
  are harmless; files with no row are what would run.

That last point is what makes a guard possible at all: the subtraction is
computable from outside without starting anything.

### The reading, and the same reading by hand

At 11:25:48Z the app answered

```
updater settings: enabled=True declared=True path='/opt/tortoise/sql/database_updates/'
                  folders={'auth': 'auth', 'characters': 'character', 'world': 'world'}
updater arming:   enabled=True, outstanding by database: auth 0, characters 0, world 0  armed=False
```

and at 11:08:03Z the same subtraction typed at a shell — `sha1sum` inside the
container, `SELECT LOWER(Hash) ... WHERE Module = ''` through `mariadb`, `comm -23`
between them, **no `yulon` code on either side** — answered

```
world/*.sql     = 125 file(s); tw_world.migrations rows = 158; file-hashes with no row = 0
character/*.sql =   1 file(s); tw_char.migrations  rows =   1; file-hashes with no row = 0
auth/*.sql      =   0 file(s); tw_logon.migrations rows =   0; file-hashes with no row = 0
```

and **the server itself said the same thing** on its next start (11:08:34Z, in
`transcript.txt`):

```
Initiating auto-updater...
[DB Auto-Updater] Found 0 possible migrations for auth.
[DB Auto-Updater] Found 1 possible migration for character.
[DB Auto-Updater] Found 125 possible migrations for world.
[DB Auto-Updater] Migration 20260902074750_world with hash AC4F8000... exists in DB but not as file, old migration?
```

— 125 files, no `[SUCCESS] Executed update`, only "in DB but not as file" notes.
Three independent readings, one answer: **0 outstanding, which is exactly why
this stack starts** where the rebuilt one crash-looped on 173.

### Permitted → refused → permitted

The refusal is only worth something if the same press succeeded a moment before
and succeeds a moment after. All three were the **same item, the same applier,
the same running world**, and one file was the only difference:

| when | the updater | the press on the Modules tab |
|---|---|---|
| 11:29:07Z | `world 0`, `armed=False` | **permitted** — `install perf-report:` |
| 11:29:08Z | one `*.sql` written into `/opt/tortoise/sql/database_updates/world/` inside the running container | `world 1`, `armed=True` |
| 11:29:09Z | armed | **REFUSED** (`4-refused-while-armed.png`) |
| 11:29:10Z | probe file deleted | `world 0`, `armed=False` |
| 11:29:11Z | disarmed | **permitted again** (`5-permitted-again.png`) |

What the tab showed, verbatim:

> `install perf-report FAILED: perf-report: refused while the world is up. This
> item asks for a restart, and that restart would hand 1 migration(s) to this
> fork's own auto-updater, which runs inside the worldserver at startup and
> cancels the whole world on one failure: zzzz_yulon_87d_probe.sql. Stop the
> world first, or set Database.AutoUpdate.Enabled = 0 in etc/mangosd.conf and
> start it by hand once.`

and `etc/mangosd.conf`'s sha256 was **unchanged** across the refusal
(`4e469bbd…` before and after), so a refused press writes nothing.

`tortoise-mangosd: status=running restarts=0` is printed beside every one of
those captures. **The world was up the whole time and nothing restarted it** —
which is the point: the probe file existed for one second between two lines of
one stage, and was deleted before anything could start into it. Its content was
`SELECT 1;`, so even the worst case was harmless.

### What the guard is, in the package

`yulon/controller_wow_tortoise/autoupdate.py`, and it has two teeth:

* **static** — `check_manifest()` refuses any item with an
  `applied_by="db-import"` SQL step. A test over the four manifests this
  repository ships would not cover this: `ManifestFetcher` mirrors them from
  GitHub into a cache the store then reads, so the file the applier is handed at
  runtime is not necessarily the file here.
* **live** — `check_restart_is_survivable()` refuses an item that asks for a
  restart while the updater is armed **and the world is up**.

Three answers, never two. `Arming.armed` is `True` / `False` / **`None`**, and
`None` — "could not read" — **refuses**. `migrations` genuinely may not exist
yet (the updater `CREATE TABLE IF NOT EXISTS`es it on its first run,
`AutoUpdater.cpp:135`), so "no such table" is an answer this reader really gets,
and answering it with "nothing outstanding" would be the exact lie the guard
exists to prevent.

Scoped to a running world, deliberately, and that scope is the clause's own
words. With the world already stopped there is nothing running to lose and the
operator is about to read the start's own log; refusing there would block every
module install done on a stopped server, which is when most of them are done.

On all three entry points — `install`, `configure` **and** `remove`. All three
end in "and now restart", and `remove()` is the one that gets forgotten: it puts
the shipped value back and asks for exactly the same restart.

And it is on the object **the tab actually holds**:
`1-modules-listed.png` is `ControllerServices.for_entry(...)`'s own view, and
the transcript records `applier = yulon.controller_wow_tortoise.autoupdate.GuardedApplier`
at 11:25:48Z. A guard built correctly in `modules.py` and a plain `Applier`
passed from `controller_view` would have passed every unit test and guarded
nothing anybody presses.

---

## What the visible effect really is, and a claim that was withdrawn

The first version of `perf-report.json` said it would *"make the worldserver
report its own map, session and query timings into the log"*. **That was wrong,
and the machine said so.** Following the interval timer to its handler:

```
PerformanceMonitor::Update()  -> if (IntervalReport.Passed()) ReportPerformanceToDB();
PerformanceMonitor::ReportPerformanceToDB()   // PerformanceMonitor.cpp:180-183
{
}
```

**The function is empty on this fork's HEAD.** So `Perf.ReportInterval` produces
no periodic output at all today; it changes the number the console reports, and
nothing else. The manifest now says that in its own notes, and its name and
description were changed to match before the screenshots were retaken.

What IS real, and what this item's visible effect is:

`Perf.Enable` — the item's other key — becomes `g_bEnableStatGather`
(`World.cpp:846`), which `XStatTimer::Begin`/`End` check before recording
anything (`Timer.h:189-190`). With it on, the running server produces this on its
own console with no client involved (11:12:47Z, `transcript.txt`):

```
CPU Performance report
QPC counter: 52
Tick: 192.34
->  WorldTick: [1] 192.34ms, 100.0%, min: 3.37ms, max: 1364.01ms
-> ->  MapManager: [1] 188.31ms, 97.9%, min: 2.94ms, max: 810.67ms
Map: 0, InstanceID: 0
->  Update: [1] 185.65ms, 98.6%, min: 0.61ms, max: 805.46ms
```

That is the module's own effect after the restart it asks for, stated by the
running server. It is not an *in-game* effect — no client was driven, and this
item has none to drive.

---

## The rest of the set, and what is NOT proved

`manifests/wow-tortoise/mods/` ships four items and the gate exercised one of
them end to end.

| item | what it is | how it is proved |
|---|---|---|
| `perf-report` | conf, `Perf.*` | **this gate**, on the console |
| `motd` | conf, `Motd` | file + unit tests; the running server reports it only to a logging-in client on this fork |
| `xp-rates` | conf, three `Rate.XP.*` | file + unit tests; no console command on this fork reports a rate |
| `all-stackables` | SQL, `tw_world.item_template.stackable` | unit tests; **not run against the live database** |

Also not proved here:

* **the SQL half against the live server.** `all-stackables` would `UPDATE`
  every stackable row of a database holding 901 real characters' items, and the
  gate that would make that safe is a backup-and-restore run this lane did not
  have the budget for. `test_tortoise_modules.py` proves the statements, the
  schema they are aimed at (`tw_world`, not `mangos` and not `acore_world`) and
  the undo; the live half is honestly open.
* **a client.** No Turtle 1.18.1 client was started. Everything above is the
  server's own word about its own state, which is what the definition of done
  asks for.
* **the guard's `unknown` branch on the live box.** `Arming(outstanding=None)`
  refusing is covered by
  `test_an_unreadable_updater_refuses_rather_than_assuming_the_best`; producing
  it live would have meant breaking the database connection under a running
  world, which is not worth doing to a server holding 901 characters.

## What was left on m910q

The **Tortoise** stack **up** (`tortoise-db` healthy, `tortoise-realmd`,
`tortoise-mangosd` — ready line at 11:30:24Z), on the pre-rebuild image.
`etc/mangosd.conf` sha256 `cf679be7…`, **the same value it had at 11:25:48Z**,
with the gate's own copy kept beside it as `etc/mangosd.conf.before-87d`. The
probe file is gone. TBC and Vanilla stay stopped — one server at a time. Nothing
was rebuilt and `src/tortoise-wow` is untouched at `3a8472e`.
