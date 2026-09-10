# T19 live — the adopt press and the updates press, on the owner's Tortoise install (m910q, 2026-09-10)

**Both presses went through, and the second reached the two route lines T14's
live half could not.** "Adopt as imported…" wrote one row into
`tw_world`.`yulon_install` on the owner's word; "Apply pending database
updates…" then applied the three `character_updates` files to the install the
whole T10 → T11 → T14 chain was built for and left the marker alone.

Pressed from `95b53a84` (the tip of `yulon-phase8b` at the time, carrying the
adopt press `a62dd763` and T14's updates button), on the owner's explicit yes of
2026-09-10 through the question tool. The code was read on the box from
`~/yulon-runs/t19-press`, a `git clone --shared` of the box's own clone checked
out at that commit and clean; the app ran under the venv at
`~/dads-mmo-lab/pylauncher/.venv` (Python 3.11.15, PySide6 6.11.2).

Every stamp in every file here was read from the box's own clock by the script
that wrote the line. Every action on the box was announced first in the Claude
activity terminal.

## The box, as found and as left

`01-containers-as-found.txt`, taken at 21:44:02 CEST **before anything was
started**, reads `tortoise-mangosd` and `tortoise-realmd` `Exited (0) 31 hours
ago` and `tortoise-db` `Exited (0) 3 hours ago` — the database's three hours are
today's index drop (`pyplan/gates/m910q-index-drop-2026-09-10/`; `states.txt:2`
has that container `started` and `finished` six seconds apart at 16:27 UTC), and
`states.txt:1-4` gives the same three containers from `docker inspect` with
their exit codes and timestamps. Every `vanilla-*`, `tbc-*` and `rehearsal-*`
container was already exited; `r6`, which is not this project's, was `Up 5 days`.
So the world was DOWN on arrival.

`16-containers-as-left.txt`, taken at 21:48:00 CEST after the final Stop, reads
`tortoise-mangosd`, `tortoise-realmd` and `tortoise-db` all `Exited (0)`; every
other container is on the same line it was on in `01-`, and `r6` is still `Up 5
days`. `states.txt:37-40` gives the same three from `docker inspect`: `exited`,
`exit=0`. Nothing on this box was started that was not stopped again.

Both Start and Stop of the world went through the app's own controller
(`ControllerServices.for_entry(...).controller`, `control.py`), never a raw
`docker stop`. The database was started on its own through the app's own
primitive (`docker.start_database()`, `startdb.py`), which is the function
`stage_start_db` calls. Nothing was compiled, rebuilt, restored or fetched. Apart
from the one marker row and the three `character_updates` files — the two writes
the owner consented to — every statement this capture issued of its own accord is
a `SELECT` or a `SHOW`.

The scratch tree `~/t19-live` and the checkout `~/yulon-runs/t19-press` were
removed after the captures were copied off.

## What was pressed, and how

`press.py` builds the real wiring (`ControllerServices.for_entry`), the real tab
(`ControllerView`), selects Modules, and calls the button's own handler —
`adopt_as_imported()` for the first press, `apply_database_updates()` for the
second. It was run twice, once per button, so the two presses have separate
frames, separate panels and separate SQL traces.

**How the adopt button was made to take its enabling reading.** The Modules tab
asks the databases once per time the database comes up, off `_apply_status()` →
`_ask_about_the_import()`, and with the database down that reading is
`unreadable` and the button is greyed. So the database was started alone first
(`02-startdb.txt`, `start_database had to start it: True`, `world_running after:
False`), exactly as T14's press did, and the driver then calls the app's own
`refresh_status()` slot — the one the five-second poll calls — and pumps the
event loop until the tab has the answer. **Nothing in the driver sets
`_adopt_state`**; the tab set it from its own probe, which is the first eight
statements of `sql-trace-adopt.txt:1-25`. `05-press-adopt.txt` records the
result: `view._adopt_state: ImportState(state='populated', …)`.

Two concessions to a headless box, both narrow, both T14's:

* **`QMessageBox.question` is replaced by a function that builds the SAME dialog
  from the same arguments** — same parent, title, text, buttons, default —
  shows it, photographs it and answers Yes. A real `exec()` blocks forever with
  nobody to click it. The text in the confirmation frames is the app's own,
  composed by `adopt_confirmation()` / `update_confirmation()` against the
  owner's folder. That the default button is No was asserted from the built
  dialog, not read off the picture (`05-press-adopt.txt`,
  `08-press-updates.txt`: `default button is No: True`).
* **Frames are `QWidget.grab()`** of the real widgets under
  `QT_QPA_PLATFORM=offscreen`, not desktop screenshots.

Nothing else is faked: no seam is attached, no private field is written, and the
engine is the one `install_wiring.installer_for_app()` builds.

**The SQL traces cover the whole driver process, not only the press.** Each
records every subprocess with its argv and — through a wrapper on
`docker._pump`, which is where `exec_stdin()` writes — the statements
themselves. So each trace opens with the tab's own enabling reading and only
then reaches the press; the `systemd-inhibit` line is where the stage spine
starts, and everything after it belongs to the press. The install's password is
redacted by the driver and never appears in any argv: `docker exec -e MYSQL_PWD`
forwards the NAME. `readings.py` and `probe.py` do the same — unlike T14's,
which spelled `-p<password>` into an argv.

## Step 1 — the readings before either press (`03-readings-before.txt`, `04-probe-before.txt`)

At 21:44:23 CEST, database up, world down (`states.txt:9-12`):

| reading | value |
| --- | --- |
| `yulon_install` anywhere on this server | **nothing** (`03-:39-41`) |
| `SELECT * FROM tw_world.yulon_install` | `ERROR 1146 … Table 'tw_world.yulon_install' doesn't exist` |
| `tw_char` tables | 109 |
| `character_inventory_copy` present | yes (1) |
| `guild_bank_money.money` type | `int(11)` |
| `ai_playerbot_random_bots` indexes | `bot`, `event`, `owner`, `PRIMARY`, `uq_owner_bot_event` |
| `idx_owner_bot_event` | **absent** (no rows) |
| characters | 903 |
| accounts | 110 |
| `guild_bank_money` rows / `MIN(money)` | 0 / NULL |
| `.yulon-install.json` `completed` | 9 stages including `import` |
| `.yulon-install.json` `last_error` | T14's refusal, still there from 2026-09-09 |

`04-probe-before.txt` is the gate's own answer, read-only, through the same
`MarkerGate` both presses use: `marker table anywhere on this server: ''`;
`probe answer: populated | complete: False | detail: 903 rows in
tw_char.characters, 110 rows in tw_logon.account`; `import_reads_as_finished:
False` — the refusal T14 recorded, still true — and `adoption_gaps: ()`, so the
adopt press's presence check would pass. `marker_row:` names the row it would
write: schema `tw_world`, table `yulon_install`, plan hash `8b60e764371f2293`.

`idx_owner_bot_event` is absent because the lead dropped it earlier today on the
owner's word (`pyplan/gates/m910q-index-drop-2026-09-10/`). **The updates press
below re-creates it from the fork's own file. That is expected and the owner
knows.**

## Step 2 — the Modules tab, both buttons (`frame-adopt-1-modules-tab.png`)

`frame-adopt-1-modules-tab.png` is the tab as the reading left it: **"Adopt as
imported…" and "Apply pending database updates…" side by side, both enabled**,
beside "Rebuild the server…", with "Install from link…", "Install from folder…",
"Apply module SQL" and "Check for updates" greyed. `05-press-adopt.txt` says the
same in words — `adopt button enabled: True`, `updates button enabled: True` —
and `frame-adopt-1b-adopt-button.png` / `frame-adopt-1c-updates-button.png` are
the two buttons alone.

## Step 3 — the adopt press

**The confirmation** (`confirmation-adopt.txt`, `frame-adopt-2-confirmation.png`)
names the folder `/home/pk/tortoise-server`, the three databases `tw_world,
tw_logon, tw_char`, and the one row:

> This writes ONE row and nothing else. Into `` `tw_world`.`yulon_install` `` — the
> table this app's own import creates and writes at the end of a successful one,
> created here if it is not already there — goes a row recording that this
> install plan (8b60e764371f2293) finished. Nothing is imported, nothing is
> dropped, no user is created, no SQL file is streamed, and your characters,
> accounts and world are read only to learn which state they are in.

and the consequence paragraph:

> Yu'lon will treat these databases as a finished import from now on. It cannot
> check that the import finished; you are saying so. If it did not, the next
> press of Apply pending database updates will run the flagged files on an
> unfinished database.

No is the default (`05-press-adopt.txt`: `default button is No: True`); the
driver answered Yes.

**The report panel** (`panel-adopt.txt`, `frame-adopt-3b-report-panel.png`),
header `finished: done`, the route's own lines verbatim:

```
[21:44:39] Step 2 of 2 (100%): adopt
[21:44:39] --- adopt
[21:44:39] The databases read as populated: 903 rows in tw_char.characters, 110 rows in tw_logon.account
[21:44:39] Writing one row into `tw_world`.`yulon_install`: this install plan (8b60e764371f2293) is recorded as finished. Nothing else is run.
[21:44:40] These databases now read as imported: tw_world.yulon_install records a finished import
[21:44:40] "Apply pending database updates…" can now apply the files this install plan has gained since this server was made. No import ran and nothing was cleared.
```

`05-press-adopt.txt` records `run_finished: [(True, 'done')]` and
`action_failed: []`. **There is no "stopped the database again" line, and there
cannot be one from this button** — see finding 2.

**The SQL trace** (`sql-trace-adopt.txt`) — after the tab's own reading
(`:1-25`) and the `systemd-inhibit` at `:31` where the spine starts, the press
issues, in order: `docker ps` (`:32`, `start_database` finding the container
already up, so no `compose up`), `docker inspect tortoise-mangosd` (`:34`, the
second world reading, in the by-name wrapper), the gate's own probe and
`adoption_gaps()` (`:36-67`, eleven `SELECT`/`SHOW` statements), `docker inspect
tortoise-mangosd` again (`:69`), and then **exactly one script into `tw_world`**
(`:71-74`):

```
CREATE TABLE IF NOT EXISTS `tw_world`.`yulon_install` (plan_hash CHAR(16) NOT NULL, finished_unix BIGINT NOT NULL);
INSERT INTO `tw_world`.`yulon_install` (plan_hash, finished_unix) VALUES ('8b60e764371f2293', 1789069479);
```

then the re-probe (`:75-88`). **Three `docker inspect` reads of the world in the
press, and the third is immediately before the write** (`:69-70`, one line above
the script) — `d0c5ab01`'s fix, visible in the trace. The reading of
`tortoise-db` at `:29-30`, before the spine starts, is `ask_db_running()`
deciding whether this press would have to put the container back down.

What is **not** in this trace: no `DROP`, no `CREATE DATABASE`, no `CREATE
USER`, no `verify` rule, no SQL file streamed, and no `INSERT` other than the
one above. The docker verbs in the whole file are `ps`, `inspect` and `exec`
and nothing else; the one non-docker process is the `systemd-inhibit` sleep the
stage spine holds. **The world server was never started by this press.**

**The readings after** (`06-readings-after-adopt.txt`, `07-probe-after-adopt.txt`).
Diffed against `03-` line by line, the only differences are the marker and the
container's uptime string:

| reading | before (`03-`) | after (`06-`) |
| --- | --- | --- |
| `yulon_install` anywhere | nothing | `tw_world  yulon_install` |
| `SELECT * FROM tw_world.yulon_install` | table doesn't exist | `8b60e764371f2293  1789069479` |
| its columns | — | `plan_hash char(16)`, `finished_unix bigint(20)` |
| `tw_char` tables / `character_inventory_copy` / `money` type / indexes / characters / accounts / `guild_bank_money` | 109 / yes / `int(11)` / no `idx_owner_bot_event` / 903 / 110 / 0, NULL | identical |
| `.yulon-install.json` | 9 completed stages, T14's `last_error` | identical |

`07-probe-after-adopt.txt`: `probe answer: imported | complete: True | detail:
tw_world.yulon_install records a finished import`, `import_reads_as_finished:
True`. The refusal T14 recorded is gone.

`frame-adopt-4-after.png` is the whole window after the press; `05-press-adopt.txt`
records `adopt button enabled after the press: False` — the button greyed itself
the moment its own press had written the row.

## Step 4 — the updates press

`08-press-updates.txt`, a second run of the driver at 21:45:07 CEST: the tab's
reading is now `ImportState(state='imported', …)`, so `adopt button enabled:
False` (`frame-updates-1b-adopt-button.png`, greyed) and `updates button
enabled: True` (`frame-updates-1c-updates-button.png`).

**The confirmation** (`confirmation-updates.txt`,
`frame-updates-2-confirmation.png`) names the folder and the three real files in
the owner's clone:

```
This applies 3 SQL step(s), the whole of character updates, into this install's databases:

    src/tortoise-wow/sql/character_updates/20260708055500_ai_playerbot_random_bots_index.sql
    src/tortoise-wow/sql/character_updates/20260731160000_guild_bank_money_unsigned.sql
    src/tortoise-wow/sql/character_updates/20260812142512_character_inventory_copy.sql
```

with the marker clause, the stopped-world clause and the guild-bank clause;
`default button is No: True`. The driver answered Yes.

**The report panel** (`panel-updates.txt`,
`frame-updates-3b-report-panel.png`), header `finished: done`, **reaching the two
route lines T14's live half could not**:

```
[21:45:07] --- import
[21:45:07] A stop here leaves the statements that already ran in place and clears nothing; the flagged phase is applied whole again the next time this is pressed.
[21:45:08] These databases read as imported; nothing else in the install plan is re-run.
[21:45:08] These databases are imported already, but 3 SQL step(s) of character updates are applied to every install, however old. This is what puts a file added to the install plan since onto a server that already exists.
[21:45:08] character updates: src/tortoise-wow/sql/character_updates/20260708055500_ai_playerbot_random_bots_index.sql -> tw_char
[21:45:08] character updates: src/tortoise-wow/sql/character_updates/20260731160000_guild_bank_money_unsigned.sql -> tw_char
[21:45:08] character updates: src/tortoise-wow/sql/character_updates/20260812142512_character_inventory_copy.sql -> tw_char
[21:45:08] character updates: applied. The import marker is unchanged.
```

The cancel note under `--- import` is `RERUN_CANCEL_NOTE`, this route's own
truthful sentence. **T14's finding 2 is closed on the live box**: the install's
`IMPORT_CANCEL_NOTE` ("Databases left half-written are detected and cleared…")
is in no line of `panel-updates.txt`.

**The SQL trace** (`sql-trace-updates.txt`) — the tab's own reading first
(`:1-19`), `systemd-inhibit` at `:20`, then `docker ps`, one `docker inspect
tortoise-mangosd`, the press's single probe (`:25-38`, five statements — the
`imported` branch short-circuits), and then the three files streamed into
`tw_char`, one `docker exec` each (`:40`, `:53`, `:73`). Every statement in the
file that is not a comment:

```
SHOW DATABASES                                                    (x3, twice over: the tab's reading and the press's probe)
SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='tw_world' AND table_name='yulon_install'   (x2)
SELECT plan_hash FROM `tw_world`.`yulon_install` ORDER BY finished_unix DESC LIMIT 1                          (x2)
ALTER TABLE `ai_playerbot_random_bots` ADD INDEX IF NOT EXISTS `idx_owner_bot_event` (`owner`, `bot`, `event`);
ALTER TABLE `guild_bank_money` MODIFY `money` INT(10) UNSIGNED NOT NULL DEFAULT 0;
CREATE TABLE IF NOT EXISTS `character_inventory_copy` LIKE `character_inventory`;
```

A grep of the file for `DROP`, `CREATE USER`, `INSERT INTO` and `verify` returns
**0 matches**: no marker rewrite, no drop, no user created, no verify query. The
docker verbs in the whole file are `exec` (13), `inspect` (4) and `ps` (4) and
nothing else; the one non-docker process is the `systemd-inhibit` sleep. **The
world server was never started by this press either.**

**The readings after** (`09-readings-after-updates.txt`,
`10-probe-after-updates.txt`). Diffed against `06-`, exactly three things moved,
all of them the three files' own work:

| reading | after adopt (`06-`) | after updates (`09-`) |
| --- | --- | --- |
| `guild_bank_money.money` type | `int(11)` | **`int(10) unsigned`** |
| `ai_playerbot_random_bots` indexes | `bot`, `event`, `owner`, `PRIMARY`, `uq_owner_bot_event` | the same **plus `idx_owner_bot_event`** |
| `idx_owner_bot_event` columns | absent | `owner`(1), `bot`(2), `event`(3), non-unique |
| `yulon_install` row | `8b60e764371f2293  1789069479` | **identical** |
| `tw_char` tables | 109 | 109 |
| `character_inventory_copy` | present | present |
| characters / accounts | 903 / 110 | 903 / 110 |
| `.yulon-install.json` | unchanged | unchanged |

The `MODIFY` took, and it took **on 0 rows**: `guild_bank_money` reads `0` rows
and `MIN(money)` NULL in `03-`, `06-` and `09-` alike, so the strict-mode refusal
the confirmation warns about could not fire and the conversion had no data to
convert. T14's finding 3 is now closed: the column the owner's install never had
narrowed is narrowed.

`idx_owner_bot_event` is back, three columns, non-unique, beside the unique
`uq_owner_bot_event` — re-created by
`20260708055500_ai_playerbot_random_bots_index.sql`, which is exactly what the
index-drop record said would happen on the next press of this route. **Expected;
the owner knows.**

The `tw_char` table count did not move because `character_inventory_copy` was
already there (`03-`) and the file is `CREATE TABLE IF NOT EXISTS`.
`10-probe-after-updates.txt` reads `imported | complete: True` with the same
detail as `07-`: the marker is untouched.

Neither press wrote to `.yulon-install.json`: `completed` is the same nine
stages in `03-`, `06-`, `09-` and `14-`, and `last_error` still carries T14's
refusal from the 9th. Neither `adopt` nor `import` is a `recorded` stage on
these tuples (`00-facts.txt`: `adopt stages: [('start-db', False), ('adopt',
False)]`, `update stages: [('start-db', False), ('import', False)]`), and
neither press failed, so nothing touched it.

## Step 5 — Start, the ready marker, and Stop

`11-worldlog-before-start.txt`, taken at 21:46:14 CEST **before** the Start with
the same `--since` window (epoch `1789069574`, given as epoch seconds because a
zone-less stamp is read in the client's local zone): `lines in the window: 0`,
`ready-marker matches in the window: 0`. That empty window is what makes the
line below this run's and not an earlier one's.

`12-start.txt`: Start through the app's own controller took the stack from
`InstallStatus(db=True, auth=False, world=False)` to `(db=True, auth=True,
world=True)`, `world_running after: True` (`states.txt:29-32`: all three
containers `running`).

`13-worldlog-ready.txt`, the worldserver's own log over that same window, matched
against the catalog entry's own `ready.world` pattern (`World server is up and
running|World initialized|MaNGOS.*started up successfully|Ready to login`) —
19487 lines, **one** match:

```
World server is up and running! Loading time: 0 minutes 59 seconds
```

`14-readings-world-up.txt` repeats every reading with the world up and differs
from `09-` in the container status lines and nothing else: **903 characters, 110
accounts, the marker row `8b60e764371f2293`, `int(10) unsigned`,
`idx_owner_bot_event` present** — read live off a running server. The tail of
`13-` shows the world issuing `DELETE FROM ai_playerbot_random_bots WHERE owner
= 0 AND bot = '56' AND event = 'login'`, which is the statement the re-created
composite index exists for.

`15-stop.txt`: Stop through the app, `controller.stop() -> True`, back to
`InstallStatus(db=False, auth=False, world=False)`, `world_running after: False`
at 21:48:00 CEST. The app saved the worldserver's log to its own log directory
on the way (`15-:1`), which is where a user's copy of it lives; the ready line
above is quoted from the container's log, not from that file.

## The `docker inspect` state at every step

`states.txt` carries a block per step, each stamped by the box's clock and each
holding one `docker inspect` line per container: `01` as found, `02` after the
database was started alone, `03-04` before either press, `05` after the adopt
press, `06-07` after its readings, `08` after the updates press, `09-10` after
its readings, `12` with the stack up, `14` with the world up, `15` stopped again.
`tortoise-mangosd` reads `status=exited running=false` in every block from `01`
to `09-10` — **the world was down across both presses** — `running=true` in `12`
and `14`, and `exited exit=0` again in `15`. The two steps without a block of
their own are `11` (the empty log window) and `13`/`16`, whose own files carry
their stamps.

## Deviations, stated

1. The driver was run **twice**, once per button, rather than pressing both in
   one session: each press then has its own trace, its own panel and its own
   frames, and the second run's frames show the adopt button greyed by the first.
2. Each SQL trace therefore covers a whole driver process — the tab's own
   enabling reading as well as the press. The `systemd-inhibit` line marks where
   the press begins; everything before it is the tab.
3. `readings.py` and `probe.py` give the client its password through `MYSQL_PWD`
   in the container's environment. T14's put `-p<password>` in an argv, which its
   reviewer noted.
4. The database was already up when each press ran (started by `startdb.py` for
   the before-readings, as T14 did), so `start-db` issued no `compose up` and the
   adopt press's conditional stop did not fire — see finding 2.
5. The ready line is quoted from `docker logs --since <epoch>` on the container,
   not from the copy the app saved on Stop; the empty pre-Start window
   (`11-`) is what bounds it.
6. The box's clone was given one `git fetch origin yulon-phase8b` so the commit
   could be checked out; nothing was pushed and no branch on the box was moved.

## Findings

### 1. The chain reaches its install. T19's ceiling is gone.

T14 recorded that the button could not help the install it was built for:
`MarkerGate` never answered `populated` + complete, so `import_reads_as_finished`
refused. `04-probe-before.txt` shows that refusal still standing at 21:44:24, and
`07-probe-after-adopt.txt` shows it gone twenty seconds later — not by teaching
the probe to infer, but because a person said so and one row was written. The
updates press then ran the route T11 built and put the file this server "died
without" (the catalog's own words for the index file) onto it. Both halves are
in this folder end to end, one press each.

### 2. The "stopped the database again" line cannot be reached from this button

`adopt_as_imported()` puts the database back down only if this press was what
started it (`was_up is False`). But the button is only enabled once the tab has a
`populated` reading, and that reading is only taken when the database is already
up — `_ask_about_the_import()` returns early on `not status.db` and
`_forget_the_adopt_reading()` drops the answer when it goes down. So through the
GUI `was_up` is always True, and the sentence never appears: it is absent from
`panel-adopt.txt`, and `sql-trace-adopt.txt` has no `compose stop`. The branch is
reachable from `adopt_as_imported()` called directly (the CLI wiring, and the
unit half's tests), not from the press. Not a defect — the guard is honest either
way — but the panel line the spec expected is not one a user will see, and the
`docker inspect tortoise-db` at `sql-trace-adopt.txt:29` is the reading that
decided it.

### 3. The `MODIFY` was a no-data change, as T14 predicted

`guild_bank_money` holds 0 rows with `MIN(money)` NULL in every reading here
(`03-`, `06-`, `09-`, `14-`), so `MODIFY money INT(10) UNSIGNED` could not meet
the negative balance the confirmation warns about. The type changed clean
(`09-:12`). What this press has NOT proved is the refusal path: nothing on this
box could make that file fail.

## Files

`00-facts.txt` (the entry, both stage tuples, the marker row, both confirmations
composed against the real folder) · `01-containers-as-found.txt` ·
`02-startdb.txt` · `03-readings-before.txt` · `04-probe-before.txt` (the gate's
own answer, read-only) · `05-press-adopt.txt` (the driver's own output) ·
`06-readings-after-adopt.txt` · `07-probe-after-adopt.txt` ·
`08-press-updates.txt` · `09-readings-after-updates.txt` ·
`10-probe-after-updates.txt` · `11-worldlog-before-start.txt` · `12-start.txt` ·
`13-worldlog-ready.txt` · `14-readings-world-up.txt` · `15-stop.txt` ·
`16-containers-as-left.txt` · `states.txt` · `sql-trace-adopt.txt` ·
`sql-trace-updates.txt` · `panel-adopt.txt` · `panel-updates.txt` ·
`confirmation-adopt.txt` · `confirmation-updates.txt` ·
`action-failed-adopt.txt` and `action-failed-updates.txt` (both empty) · the
fourteen frames · and the scripts that produced them (`facts.py`,
`containers.py`, `state.py`, `startdb.py`, `readings.py`, `probe.py`, `press.py`,
`control.py`, `worldlog.py`), committed so the captures can be read against what
was actually run.

| frame | what it shows |
| --- | --- |
| `frame-adopt-1-modules-tab.png` | the Modules tab with **both** buttons enabled, beside Rebuild |
| `frame-adopt-1b-adopt-button.png` / `-1c-updates-button.png` | each button alone, enabled |
| `frame-adopt-2-confirmation.png` | the adopt confirmation: the folder, the three databases, the one row with the plan hash, the consequence paragraph, the stopped-world clause, No focused as the default |
| `frame-adopt-3-report.png` / `-3b-report-panel.png` / `-4-after.png` | the window and the panel after the adopt press: `finished: done` and the route's four lines |
| `frame-updates-1-modules-tab.png` | the same tab after the adopt press |
| `frame-updates-1b-adopt-button.png` | the adopt button **greyed** — its own press wrote the row |
| `frame-updates-1c-updates-button.png` | the updates button, still enabled |
| `frame-updates-2-confirmation.png` | the updates confirmation: the three real files, the marker clause, the stopped-world clause, the guild-bank clause, No focused as the default |
| `frame-updates-3-report.png` / `-3b-report-panel.png` / `-4-after.png` | the window and the panel after the updates press, carrying **both** route lines and the truthful re-run cancel note |
