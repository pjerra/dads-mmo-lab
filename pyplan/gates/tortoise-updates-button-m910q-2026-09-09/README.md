# T14 live half — the updates button pressed on the owner's Tortoise install (m910q, 2026-09-09)

**The press REFUSED, and the refusal is the finding.** The button is on the tab,
it is enabled for this install, its confirmation names the three real files in
the owner's clone, and pressing Yes started the database alone, asked the
databases one question, and stopped: *"WoW Tortoise's databases do not read as a
finished import (populated: 903 rows in tw_char.characters, 110 rows in
tw_logon.account), so these files do not belong to them yet. Nothing was applied,
nothing was imported and nothing was cleared. Finish the install of this folder
first -- these files go in as part of it."*

Every database reading is byte-identical before and after (`01-` vs `05-`). The
only things that moved are the app's own `last_error` record and container
uptimes.

**So the button cannot yet help the install it was built for**, and that is not
a T14 defect — the precondition mirrors `_import`'s own branch exactly, on
purpose. It is `MarkerGate`'s: see finding 1.

Pressed from `f31a1d2d` (T14 merged as `ed035e41` plus the lead's sentence edits
`f2c22b4e`/`f31a1d2d`). Fresh-install half: not re-proved here — cite
`pyplan/gates/tortoise-soap-yulon-arch-2026-09-09/`.

## The box, as found and as left

Found with the world and auth exited: the earliest listing in this folder
(`01-readings-before.txt:75-80`) reads `tortoise-mangosd` and `tortoise-realmd`
`Exited (0) 3 hours ago` and every `vanilla-*` container exited 8 hours; by then
`startdb.py` had already started `tortoise-db` for the before-readings
(`Up 11 seconds (healthy)`), so the database's as-found state and the arrival
clock have no capture of their own. `r6` (not this project's) was up. So the
world was DOWN on arrival.

Left at 14:25 CEST (`stop_staged()` stamped 14:25:37): Stop through the app,
`controller.stop() -> True`, `InstallStatus(db=False, auth=False, world=False)`,
`world_running` False (`08-stop-restore.txt`). No container listing was taken
after that Stop, so the exit codes, "nothing else of this project running" and
`r6` untouched are proved through the press (`01-`, `07-`), not as-left.
`09-containers-day-after.txt`, a read-only `docker ps -a` taken by the lead the
next day, shows the Tortoise containers still exited since the 9th and `r6` up. The Start below was done to prove the world
still comes up and was undone immediately after. No compile, no rebuild, no
rollback restore, no account or character touched — every statement this capture
issued of its own accord is a `SELECT` or a `SHOW`.

Every world Start and Stop went through the app's own controller
(`ControllerServices.for_entry(...).controller`, `control.py`), never a raw
`docker stop`.

## What was pressed, and how

`press.py` builds the real wiring (`ControllerServices.for_entry`), the real tab
(`ControllerView`), selects Modules, and calls the button's own handler
`apply_database_updates()`. Two concessions to a headless box, both narrow:

* **`QMessageBox.question` is replaced by a function that builds the SAME dialog
  from the same arguments** — same parent, title, text, buttons, default —
  shows it, photographs it, and answers Yes. A real `exec()` blocks forever with
  nobody to click it. The text in `frame-2-confirmation.png` is the app's own,
  composed by `update_confirmation()` against the owner's folder.
* **Frames are `QWidget.grab()`** of the real widgets under
  `QT_QPA_PLATFORM=offscreen`, not desktop screenshots.

Nothing else is faked: no seam is attached, no private field is touched, and the
engine is the one `install_wiring.installer_for_app()` builds.

## Frames

| file | what it shows |
| --- | --- |
| `frame-1-modules-tab-button.png` | the Modules tab with **"Apply pending database updates…"** enabled, beside Rebuild |
| `frame-1b-button.png` | the button alone |
| `frame-2-confirmation.png` | the confirmation: the three files, the marker clause, the stopped-world clause, the guild-bank clause, **No as the default** |
| `frame-3-report.png` | the whole window after the press |
| `frame-3b-report-panel.png` | the panel: the header, the opening note, the two stage markers, the cancel note, and the FAILED header carrying the refusal. The ticket's two route lines ("These databases are imported already, but N SQL step(s) of …" and "…: applied. The import marker is unchanged.") are **not** in it: the press refused before either is reached |

## The two log lines, and one that should not be there

`panel.txt`, verbatim:

```
[14:22:29] Applying pending database updates for WoW Tortoise in /home/pk/tortoise-server
[14:22:29] You can stop this at any time. This does two things and nothing else: ...
[14:22:29] Step 1 of 2 (50%): start-db
[14:22:29] --- start-db
[14:22:29] Starting the database, which the import writes into.
[14:22:36] The database is up.
[14:22:36] Step 2 of 2 (100%): import
[14:22:36] --- import
[14:22:36] Databases left half-written are detected and cleared before the import is run again, so nothing here has to be undone by hand.
```

The last line is **finding 2**: it is the install's `IMPORT_CANCEL_NOTE`, and it
is false on this route.

## The SQL trace

`sql-trace.txt` records every subprocess the press started, with its argv and —
through a wrapper on `docker._pump`, which is where `exec_stdin()` writes — the
**statements themselves**. The install's password is redacted; it never appears
in the press's argv (`docker exec -e MYSQL_PWD` forwards the name); the capture's own
`readings.py` and `probe.py` pass it to their client at run time and are not in the trace.

Eight statements reached the database, all of them one `probe()` call:

```
SHOW DATABASES                                                         (x3)
SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='tw_world' AND table_name='yulon_install'
SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='tw_char' AND table_name='characters'
SELECT COUNT(*) FROM `tw_char`.`characters`
SELECT COUNT(*) FROM information_schema.tables WHERE table_schema='tw_logon' AND table_name='account'
SELECT COUNT(*) FROM `tw_logon`.`account` WHERE username NOT IN ('ADMINISTRATOR', 'GAMEMASTER', 'MODERATOR', 'PLAYER')
```

What is **not** in it: no `DROP DATABASE`, no `CREATE DATABASE`, no
`CREATE USER`, no `INSERT` into `yulon_install`, no `verify` rule, no SQL file
streamed, and no second probe. The only docker verbs besides `exec` are
`inspect` (the world's state, read twice — once before the database was started
and once after, which is the race T7 closed), `ps` (the names, once, before the
start) and `compose up -d --no-deps tortoise-db` — the database alone; the one
non-docker process is the `systemd-inhibit` sleep the stage spine holds
(`sql-trace.txt:1-6`). **The world server was never started by the
press.**

## Readings, before and after

`01-readings-before.txt` vs `05-readings-after.txt` differ only in the label,
`last_error`, and container uptimes. `07-readings-world-up.txt` repeats them
with the world running and agrees.

| reading | before | after |
| --- | --- | --- |
| `tw_char` tables | 109 | 109 |
| `character_inventory_copy` present | yes | yes |
| `guild_bank_money.money` type | `int(11)` | `int(11)` |
| `ai_playerbot_random_bots` indexes | `bot`, `event`, `idx_owner_bot_event`, `owner`, `PRIMARY`, `uq_owner_bot_event` | identical |
| `idx_owner_bot_event` | present (3 columns) | present (3 columns) |
| characters | 903 | 903 |
| accounts | 110 | 110 |
| `guild_bank_money` rows / min(money) | 0 / NULL | 0 / NULL |
| `tw_world.yulon_install` | **does not exist** | **does not exist** |
| `.yulon-install.json` `completed` | 9 stages incl. `import` | unchanged |
| `.yulon-install.json` `last_error` | `""` | the refusal, verbatim |

The `last_error` is the app's own record, written by `_staged()`'s failure path
(`_record_error`), which is what the reviewer asked to see; `completed` is
untouched, so nothing about this install's resume changed.

## The world afterwards

`06-start-after-press.txt`: Start through the app took the stack from
`InstallStatus(db=True, auth=False, world=False)` to
`(db=True, auth=True, world=True)`, `world_running` True. No worldserver log is
committed in this folder (the app saved one to its own log directory on Stop,
`08-stop-restore.txt:1`), so nothing here shows the ready marker or the world's
SQL; what is captured is the status change above and
`07-readings-world-up.txt`, which reads 903 characters with the world up. Then
Stop, back to the found state.

## Findings

### 1. `MarkerGate` never answers `populated` + complete, so the button cannot reach a marker-less install

`import_reads_as_finished()` accepts two answers — `imported` (a marker row of
any hash) and `populated` with every schema complete. The second is
**unreachable through `MarkerGate`**: its own docstring says *"`imported` is the
only branch that reports `complete`"*, and `probe()` returns
`ImportState("populated", populated)` with `complete` left at its default False
(`sqlplan.py`, the branch table).

This install has **no `yulon_install` table at all**, in any schema
(`02-probe-before.txt`), while `.yulon-install.json` records `import` among its
completed stages. So the probe answers `populated`, `complete=False`, and the
route refuses.

That matters beyond this box: T11's report says the flagged phase reaches *"an
install made by the shell scripts"*, which "has no marker row" and "reads
`populated`". On the CMaNGOS gate that install reads `populated` **incomplete**,
so T11's route never reached it either, and T14's button inherits the same
ceiling. The button today serves an install that carries a marker row; the
m910q install, which is the one the whole T10→T11→T14 chain was built for, is
not one.

**Not worked around.** Writing a marker row into the owner's database to make
the press proceed would be a write nobody consented to and would falsify the
very evidence this capture exists for. For the lead: the honest fixes are either
a `MarkerGate` that reports completeness on the `populated` branch (it already
asks per-schema table counts elsewhere), or an explicit "adopt this install as
imported" action — both bigger than T14 and both about T11's route, not about
the button.

Worth noting for the record: **before T14's precondition this press would also
have refused here**, in `stage_import()`'s install-shaped words (*"These
databases already hold data … Use an empty folder for a new install"*). T14 did
not take a working path away; it refuses in words that fit the button.

### 2. The panel prints the install's cancel note, which is false on this route

The spine yields `stage.cancel_note` right after `--- <name>`
(`native.py`, `_staged`), and `update_stages()` reuses the family's `import`
stage, which carries `IMPORT_CANCEL_NOTE`. So an updates press tells the user
*"Databases left half-written are detected and cleared before the import is run
again, so nothing here has to be undone by hand."* — a promise of exactly the
clearing this route exists to make unreachable, in a press that clears nothing.
Visible in `frame-3b-report-panel.png` and `panel.txt`.

One line where `recorded=False` already is: `replace(self.stage_named("import"),
recorded=False, cancel_note="")`, or a note of this route's own. Not made here —
the live half was captures.

### 3. The owner's install never had `guild_bank_money.money` narrowed

The ticket records that the three `character_updates/` files were applied by
hand on 2026-09-09. Two of them took: `character_inventory_copy` exists and
`idx_owner_bot_event` is on `ai_playerbot_random_bots`. The third did not —
`money` is `int(11)`, signed, not the `int(10) unsigned` that
`20260731160000_guild_bank_money_unsigned.sql` sets. The table has **0 rows**
and `MIN(money)` is NULL, so the strict-mode refusal the confirmation warns
about could not fire here and the `MODIFY` would be a clean no-data change.
Recorded because the owner's open question about that file, and the ticket's
expectation that the type would be "unchanged", both rest on a premise this box
does not support.

### 4. `idx_owner_bot_event`, for the owner's open question

Present before and after, three rows in `information_schema.statistics` (a
three-column index), alongside `uq_owner_bot_event`. The press did not touch it,
and — per finding 1 — no press through this button can currently re-add it on
this install.

## Files

`00-facts.txt` (the entry, the stages, the confirmation composed against the real
folder) · `01-readings-before.txt` · `02-probe-before.txt` (the gate's own
answer, read-only) · `03-stop-before-press.txt` · `04-press.txt` (the driver's
own output) · `05-readings-after.txt` · `06-start-after-press.txt` ·
`07-readings-world-up.txt` · `08-stop-restore.txt` · `sql-trace.txt` ·
`panel.txt` · `confirmation.txt` · `action-failed.txt` · the five frames · and
the scripts that produced them (`facts.py`, `readings.py`, `probe.py`,
`startdb.py`, `control.py`, `press.py`), committed so the captures can be read
against what was actually run.
