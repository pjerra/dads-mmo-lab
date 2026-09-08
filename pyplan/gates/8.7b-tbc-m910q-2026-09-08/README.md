# 8.7b live: TBC modules, and the value the server itself reads back

**Box:** `m910q`, the native CMaNGOS TBC install at `~/tbc-7.4c` (`tbc-db`, `tbc-realmd`,
`tbc-mangosd`; MariaDB, world schema `mangos`). **Date:** 2026-09-08, 08:45Z – 09:02Z.
**Tree:** this lane's worktree on `yulon-phase8b` at `74c71d97`, tarred to `~/gate87b/pylauncher`
and run under `~/gate81b-venv/bin/python` (PySide6, `QT_QPA_PLATFORM=offscreen`) — **not** from
`~/dads-mmo-lab`, which sits detached at `a13a5def`, has no `manifests/wow-tbc/` at all and
carries another lane's uncommitted edits to `yulon/networking.py`.

**Plan run:** [`pyplan/gates/8.7b-tbc-m910q-plan.md`](../8.7b-tbc-m910q-plan.md), written by the
lane that built the manifest set. Every step of it ran. Two were made stronger and one turned up
a defect; all three are named below.

## The definition of done

> *at least one manifest installs and its key is reported by the running server after the restart
> it asks for.*

**Met.** The manifest is `motd`; the server's own answer is `.server motd`, which
`ChatHandler::HandleServerMotdCommand` answers from `sWorld.GetMotd()` — read on this box at
`src/mangos-tbc/src/game/Chat/Level0.cpp:283`, with `allowConsole` true in its command-table row
at `Chat/Chat.cpp:816`, so it answers with no client and no logged-in character.

| Moment | What the RUNNING server said, asked with `.server motd` | Where |
| --- | --- | --- |
| Ground, before anything | `Welcome to the Continued Massive Network Game Object Server.` | `03-before-motd.txt` |
| Conf changed, **no restart** | `Welcome to the Continued Massive Network Game Object Server.` | `06-before-restart-motd.txt`, `11-console-before-restart-still-the-old-motd.png` |
| After Stop→Start on the Server tab | **`Welcome!`** | `07-after-restart-motd.txt` |
| After a second install + restart | **`Yulon 8.7b gate`** | `07b-distinctive-value.txt`, `12-console-after-restart-the-servers-own-answer.png` |
| After Remove + restart | `Welcome to the Continued Massive Network Game Object Server.` | `08-conf-sha.txt`, `13-console-after-remove-back-to-the-shipped-default.png` |

The middle row is the one that makes the restart load-bearing rather than decorative: at
08:49:48Z the file on disk read `Motd = "Welcome!"` and the process that had been up since
08:46:28Z still answered with the old line. `build.restart` is not over-claiming.

## Ground, read before the first action

`etc/mangosd.conf` was `af8e3e25843004a5ad98f5dd815cfca65d3d3736de87a69fcae4e0128db5db57`, and
line 924 was CMaNGOS's shipped default. That matters twice: the step-7 assertion was **not**
already true before step 4 ran, and step 8's checksum comparison is meaningful rather than
vacuous. `00-preconditions.txt`.

The same discipline caught a trap in the SQL half. `item_template` had **121 rows already at
`stackable = 200`** before `all-stackables` was installed — so "some rows are at 200" proves
nothing here. The assertion used is the whole population: `3471` rows with `stackable > 1`, all
`3471` at 200 after the install, back to `121` at 200 after the remove. `09-sql-mod.txt`.

## Every clause, and where it was pressed

| Clause | Result | Evidence |
| --- | --- | --- |
| "a manifest set … for this game" | five `mod` manifests, all `wow-tbc`, all `rebuild=False restart=True` | `01-manifest-set.txt` |
| "and binding" | `ControllerServices.for_entry()` on the live install: store `wow-tbc`, applier root `~/tbc-7.4c`, `world → mangos` (not `acore_world`), client `mariadb`, container `tbc-db` | `02-binding.txt` |
| "a module is a **configuration activation**" | the Modules tab's own **Install selected** press wrote one key into `etc/mangosd.conf` | `04-install-tab-press.txt`, `4-…-before-install.png`, `5-…-after-install.png` |
| the install is a real edit, not a report about one | `diff` against the ground copy: exactly one changed line | `05-conf-diff.txt` |
| "the restart it asks for" | `restart_recommended=True`, drawn for the user as *"⚠ Press Stop and then Start on the Server tab to apply this."* | `5-…-after-install.png` |
| **"its key is reported by the running server after the restart"** | `Welcome!`, then `Yulon 8.7b gate` | `07-after-restart-motd.txt`, `12-…-the-servers-own-answer.png` |
| "**or a SQL mod**" | `all-stackables` installed and removed through the same button; 3471 → 3471 at 200 → back to 121, backup table created and dropped | `09-sql-mod.txt`, `8-…-sql-installed.png`, `9-…-sql-removed.png` |
| reversible | after Remove the conf is **byte-identical** to the ground copy — same sha256 | `08-conf-sha.txt`, `10-…-after-remove.png` |
| the server still loads with all of it undone | `Avg Diff:` reached in 66 s on the final restart | `11-console-tab.txt` |

## Where this run went beyond the plan, and why

**The plan drove `ControllerServices.applier`; this run pressed the button.** The plan says in so
many words that it does not prove "the Modules tab as a widget". It costs nothing to press the
real one, so steps 4, 8 and 9 ran through `ControllerView`'s **Install selected** / **Remove
selected** buttons on a view built over the live install, and step 7's restart is the Server tab's
own **Stop** then **Start** — the two buttons the report names, pressed in that order. That is what
turned up the defect below.

**The restart was proved twice, with two different values.** A tab press can only ever write the
prompt's default (see the defect), so the first pass proved the clause with `Welcome!`. The
second pass installed `Yulon 8.7b gate` through the applier — a string no default could produce —
and restarted again. It also proved something the plan did not test: the second install replaced
the line the first one wrote, leaving **exactly one** `Motd` line rather than appending beside it.

**The ready marker was read per run, not per log.** `docker logs` on this container accumulates
across restarts; by the end it held **50** `Avg Diff:` lines, of which the run under test
contributed **1**. Every wait used `docker logs --since "$(docker inspect -f '{{.State.StartedAt}}')"`.
The first `compose up -d` reached ready in 62 s, and the five restarts after it in 53, 59, 65, 63
and 66 s — nowhere near the 1800 s the plan budgeted, and the 5-minute `stop_grace_period` never
bit: `stop_staged()` keeps the containers, and Stop returned in 5 s.

## The defect that matters: a SQL mod cannot be installed compliantly at all

Owner answer 7 says no Phase 8 feature writes the world database while the world server is up, and
`write-ledger.md`'s row for `apply.py::_run_sql::run_statement` says 8.7 is where the applier's
guard lands. `all-stackables` writes the world schema, so the compliant sequence is Stop →
Install → Start.

**Step 9 above did not do that** — it installed with the world running, which is what the plan
asked for and what the clause needed, and the data was put back to ground afterwards. That is
recorded here rather than glossed, because on the way to writing it down the obvious question was
asked of the machine: what happens if you *do* press Stop first?

```
--- pressing Stop on the Server tab, as owner answer 7 requires ---
status label: status: db down, auth down, world down
census after Stop: tbc-db exited | tbc-mangosd exited | tbc-realmd exited

--- now pressing Install selected on all-stackables, world down ---
install all-stackables FAILED: SQL failed (inline → mangos): Error response from daemon:
container f94521f9a435da653cf216d8308c02fa1e9535e0a122efbdc542144a35b32605 is not running
```

`stop_staged()` runs `compose stop` over the whole project — deliberately, so the servers close
their connections before the database goes away — and `DockerSql` reaches the database through
`docker exec tbc-db`. Stopping the world removes the applier's only route to the database. Both
facts are correct on their own; together there is no branch of the fork that works, and 8.7a's
guard as written closes the remaining one. Filed as **§46**, with what would have to change.

Nothing was written by the failed press: 3471 / 121 / no backup table, all at ground.
`14-sql-mod-world-stopped.txt`, `14-sql-mod-with-the-world-stopped.png`.

## The second defect this gate found

**A user cannot set their own message of the day from the Modules tab.**
`ControllerView._module_values()` opens the prompt dialog only when *some* required prompt has
`default is None`. `motd`'s single prompt has default `"Welcome!"`, so the press asks nothing and
the applier renders `{motd}` from the default. Measured live: the ground line
`Motd = "Welcome to the Continued Massive Network Game Object Server."` became `Motd = "Welcome!"`,
with no dialog shown (`04-install-tab-press.txt`). Every value in this manifest set is reachable
only as its default, and this is the one item in it whose whole point is that the operator picks
the text. Filed as **§45** in `bug-checklist.md`. It does not block this box — the clause asks
whether the key arrives at the server, and it does.

## What this gate does NOT prove, and who owes it

* **The applier's inline SQL is not covered by 8.7a's refusal.** 8.7a's guard, gated on
  `yulon-ubuntu` the same day, lives on the AzerothCore module importer
  (`docker.apply_module_sql()`). `Applier.install()`'s `sql[]` step is a different caller, and
  this run put three `UPDATE`/`CREATE TABLE` statements into the world schema with `tbc-mangosd`
  **running**, unrefused. That is not a failure of 8.7b — its checklist line asks for no guard —
  but 8.7a's line says "inside the step every caller passes through", and this is a caller. §46
  above is what that guard runs into on this family.
* **`xp-rates`, `all-flight-paths`, `cross-faction`.** Same mechanism against different keys; the
  unit tests cover each. Not pressed here.
* **That a player sees the effect.** No client was driven. `.server motd` is the server's own
  answer about its own state, which is what the clause asks for.
* **A caveat about the console seam, measured rather than assumed.** `ConsoleReply.lines` is
  everything inside the reply window, not "the answer": on a world that had just restarted, the
  window caught roughly twenty `Gameobject (GUID: …) not created` load lines between the prompt
  and the answer, visible in `12-…-the-servers-own-answer.png`. A caller comparing the whole tuple
  passes on a quiet server and fails on a busy one; `lines[-1]` is the durable read, and it is
  what the assertions here used. Documented behaviour of a shared tty, not a new defect.

## The box, before and after

Arrived with the **Tortoise** stack up — the 8.4d/8.5d lane stood down at 10:41 local and left it
as it found it (`~/claude-activity.log`). This gate stopped Tortoise and ran on TBC twice: the
main run 08:46Z–09:02Z, and the §46 addendum 09:05Z–09:09Z after the write-ledger read raised the
question. Between and after them the box was put back.

**Left running: `tortoise-db`, `tortoise-realmd`, `tortoise-mangosd`; the TBC stack down** — the
state the box was in at 08:45Z. Never more than one server up at a time. Nothing was rebuilt, no
scheduled task was created, and `etc/mangosd.conf` ends on
`af8e3e25843004a5ad98f5dd815cfca65d3d3736de87a69fcae4e0128db5db57`, the sha256 it carried before
the first step. `etc/mangosd.conf.before-8.7b` is left beside it as the reference copy. The world
database ends at its ground counts: 3471 rows with `stackable > 1`, 121 at 200, no
`yulon_stackable_backup`.

The scripts that produced all of this are in this folder — `gate87b_tab.py` (the Install press),
`gate87b_restart.py` (Stop/Start), `gate87b_sql.py` (the SQL mod), `gate87b_console.py` (the
server's own answer) and `gate87b_stopped.py` (§46) — so any claim here can be re-run rather than
re-argued.
