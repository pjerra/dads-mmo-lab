# 7.9's controller-surface gate, RE-RUN on the merged Phase 8b tip — m910q, 2026-09-08

Answering half of Phase 8's exit line (`pyplan/checklist.md:2508`): *"Phase 7's
controller-surface gate and cross-server regression pass re-run green on the merged tip,
because the base controller's stop path and the service assembly both change."* This folder is
the controller-surface half — 7.9's harness against all three CMaNGOS games. The cross-server
half (7.10) is `pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/`.

**Code under test.** `/home/pk/lane79/checkout` on `m910q`, a fresh
`git clone --branch yulon-phase8b` made on the box, at
**`96a129dcd03866e3ac997d1be1e690553f92c535`** — `origin/yulon-phase8b`'s tip, verified with
`git ls-remote` before the clone and `git rev-parse HEAD` after it (`run.log` carries the sha
as the runner read it). Interpreter `/home/pk/gate81b-venv/bin/python`, **Python 3.11.15**,
**PySide6 6.11.2**. Docker 29.7.2. Box `PK-M910q`, Linux 6.8.0-138-generic.

**Harness.** `pyplan/gates/gate-79-controller-surface.py` **at that commit**, run unmodified.
The runner around it (`run79-rerun.sh`), the between-games stop (`stopper.py`) and the
photograph pass (`starter.py`, `shots79.py`, `run-shots79.sh`) are this lane's and are
committed here exactly as run.

## Result: 33 gate checks, 33 OK, 0 FAIL — and 18 more from the photograph pass

| game | server dir | checks | exit | elapsed | log |
|---|---|---|---|---|---|
| `wow-tbc` | `~/tbc-7.4c` | **11 passed / 0 failed** | 0 | 281 s | `gate79-tbc.log` |
| `wow-vanilla` | `~/vanilla-75b` | **11 passed / 0 failed** | 0 | 208 s | `gate79-vanilla.log` |
| `wow-tortoise` | `~/tortoise-server` | **11 passed / 0 failed** | 0 | 306 s | `gate79-tortoise.log` |
| `wow-tbc` shots | | 6 OK / 0 FAIL | 0 | — | `shots-tbc.log` |
| `wow-vanilla` shots | | 6 OK / 0 FAIL | 0 | — | `shots-vanilla.log` |
| `wow-tortoise` shots | | 6 OK / 0 FAIL | 0 | — | `shots-tortoise.log` |

`run.log` carries the exit codes and elapsed times as the runner wrote them.

## The ground, read before anything was started

`ground-start.txt`, taken at `2026-09-08T23:40:09+02:00`, **before the first game was
touched**:

* `docker ps -a` — **every** game container `Exited`; nothing of the four games running. The
  only thing up on the box was `r6` (`refute5img:latest`, up 3 days, not a game server);
* listening ports: **`(none of 3724/3443/8085/8888/8129)`**;
* `~/vanilla-75b/sql_scripts/backups` and `~/tortoise-server/sql_scripts/backups`:
  **absent**. `~/tbc-7.4c/sql_scripts/backups`: 13 entries, from earlier lanes.

That reading is what makes the rest mean anything. Every game's baseline section then reads
`status: db=False auth=False world=False` in its own log (`gate79-tbc.log:10`,
and the same two lines in the other two), so `start_staged()` really did the starting — none
of the three "all three up" lines is a restatement of the state the run began in. `ground-end.txt`
(`23:53:57`) closes the same loop: no game port listening, every game container `Exited`,
and the two absent backup directories now hold 10 and 8 entries — created by this run,
which is the independent confirmation that `backup()` wrote where it said it did.

`ground-after-tbc.txt`, `ground-after-vanilla.txt` and `ground-after-tortoise.txt` are the
same probe after each game, and each shows exactly one game's three containers up.

## What each of the 11 checks answered, per game

| | TBC (`~/tbc-7.4c`) | Vanilla (`~/vanilla-75b`) | Tortoise (`~/tortoise-server`) |
|---|---|---|---|
| baseline: this run started it | yes | yes | yes |
| baseline ready wait | 64.7 s | 35.3 s | 76.5 s |
| `console.send_command()` | 3.60 s, **6 lines** | 3.60 s, **5 lines** | 3.61 s, **22 lines** |
| world pid / RestartCount / StartedAt across the attach | unchanged | unchanged | unchanged |
| `backup()` | 5.3 s, **4 dumps** | 3.6 s, **5 dumps** | 6.4 s, **4 dumps** |
| `verify_dump()` | 4/4 | 5/5 | 4/4 |
| `plan_restore()` over a running server | **refused**, naming `tbc-mangosd, tbc-realmd` | **refused**, naming `vanilla-mangosd, vanilla-realmd` | **refused**, naming `tortoise-mangosd, tortoise-realmd` |
| `stop_staged()` (the Stop button) | **2.2 s** | **22.3 s** | **5.2 s** |
| `start_staged()` (the Start button) | 6.1 s | 6.1 s | 6.1 s |
| to ready after that start | 62.6 s | 31.2 s | 69.2 s |
| `restore()`, EVERY dump | **49.6 s**, 4 databases | **36.6 s**, 5 databases | **44.5 s**, 4 databases |
| ready again after the restore | 64.9 s | 31.2 s | 69.9 s |

Databases actually round-tripped: TBC `characters, logs, mangos, realmd` (its `mangos` is
158,550,486 B); Vanilla `characters, classiclogs, logs, mangos, realmd` (`mangos`
113,674,581 B); Tortoise `tw_char, tw_logon, tw_logs, tw_world` (`tw_world` 171,361,088 B).
`missing_core` was `()` on all three — the `acore_*` warning the 2026-09-04 run found on every
CMaNGOS game is still gone at this tip.

**`stop_staged()` did NOT reproduce the 301 s tail.** The 2026-09-04 run measured 3.1 s and
301.2 s from the same TBC install an hour apart and concluded a Stop button must tolerate five
minutes. Tonight's three presses are 2.2 s, 22.3 s and 5.2 s. That neither refutes the 301 s
reading nor reproduces it: `stop_grace_period: 5m` is still in the compose templates, and a
server with characters in memory is what makes a shutdown long. **These three servers had
`Online players: 0`** (the console replies say so), so the fast end of the range is what an
empty server buys, and the 2026-09-04 range stands as the number to size a spinner for.

## What the re-run FOUND: TBC's worldserver aborts on a clean Stop, and the gate cannot see it

`79-tbc-stop-abort.log`. **`stop_staged()` returned `True`, all three containers went down,
the gate passed — and `tbc-mangosd` exited `139`.**

```
/tbc-mangosd exit=139 started=…T21:54:13.414909796Z finished=…T21:55:27.669534862Z restarts=0 oom=false
/tbc-realmd  exit=0   started=…T21:54:13.416323438Z finished=…T21:55:26.538471473Z restarts=0 oom=false
/tbc-db      exit=0   started=…T21:54:07.727635087Z finished=…T21:55:27.943916809Z restarts=0 oom=false
```

Its last words are not the ones the checklist already records:

```
Object::~Object (GUID: 181646 TypeId: 5) deleted but still in world!!
Critical Error: A condition which must never be false was found to be false. Server was shut down to protect data integrity.
~Object(): false
```

**This is a different mechanism from the one 7.9 booked on 2026-09-04**, and the difference is
in the timestamps above. That one was `SQL ERROR: Lost connection to MySQL server during query`
on `UPDATE characters SET online = 0` — the database taken out from under the worldserver by a
hand-typed, un-ordered `docker stop` of all three. Here `stop_staged()` walked the dependency
graph exactly as its docstring promises: **`tbc-db` finished 0.3 s AFTER `tbc-mangosd`**, so
the database was still there, and the three `Lost connection to MySQL` lines in this
container's whole log belong to earlier days rather than to this shutdown. The abort is
CMaNGOS-TBC's own object-teardown assertion firing during its normal shutdown.

**It happened on both of tonight's TBC stops**: `ground-end.txt` (`23:53:57`) reports
`tbc-mangosd Exited (139) 9 minutes ago`, which is the gate pass's stop at `23:44:5x`, and the
inspect above is the photograph pass's stop at `23:55:27`.

**It is a per-tree fact and was measured per tree.** On the same call, the same night, the same
box: `vanilla-mangosd exit=0`, `tortoise-mangosd exit=0`. TBC alone.

**What this re-run does NOT assert, said plainly.** The gate's stop check is
`gate-79-controller-surface.py:377-391`: it asks `running([world, auth, db])` (`:383`) and passes when
nothing is up. *A worldserver that abort-traps on every Stop is indistinguishable, through that
check, from one that shuts down cleanly* — the same shape as the empty console reply that
passed on 2026-09-05. The one line that would catch it is an `inspect(world)["exit_code"]`
read after the stop, next to the `running()` read that is already there; `inspect()` exists in
the harness and would need one more field in its format string. **It is deliberately NOT added
here**: the counts in the table above are this instrument as it stands at `96a129dc`, and
7.9's own line records what it cost the last time a check was added and the old numbers were
left reading as if they came from the new harness. The change belongs to whoever ticks the box,
with the exit-code finding above as its RED.

## A second per-tree fact: what "the console reply" is on Tortoise

TBC answers `server info` in 6 lines and Vanilla in 5 — core revision, world DB, EventAI,
players online, uptime. **Tortoise answers in 22**, and `shots/wow-tortoise-console-tab-after-send.png`
shows why: its worldserver interleaves playerbot SQL logging into the same tty the console
attaches to, so seventeen of the twenty-two lines a user reads are
`[0 ms] SQL: SELECT action_line FROM ai_playerbot_custom_strategy …`. The five that answer the
question are at the bottom. Nothing is broken — `prompted=True`, the reply is delimited on the
core's own prompt, and the gate passes — but anyone sizing or filtering that panel should size
it for Tortoise, not for TBC.

## The photograph pass, and why it is a second pass

`gate-79-controller-surface.py` drives `ControllerServices` on purpose: its docstring says
timing `docker.stop_staged()` would time a function the Server tab does not call. The
consequence is that **nothing the gate does travels through a QWidget**, so there is no surface
for it to photograph. `shots79.py` presses two of the same clauses through the real
`ControllerView` with `QTest.mouseClick`, on `QT_QPA_PLATFORM=offscreen`, and grabs a frame
either side of each press. It starts each game's stack itself (`starter.py`, through
`Controller.start()` and that game's own `wait_server_ready()`) and stops it again
(`stopper.py`), because the gate pass leaves every stack stopped and m910q's rule is one server
at a time.

Two things keep the frames honest:

* **`status_poll_ms=0`.** `ControllerView` re-reads status on a 5 s timer; with the timer on, a
  "before" frame can already hold the answer. With it off the label reads `status: unknown`
  until a click, and **that is asserted one line before the before-frame is written** — so a
  `…-before-refresh.png` showing the post-press state cannot ship here, which is what 8.2d did.
* **Every capture logs whether the server was alive at that instant**, read through
  `docker ps` and not through the widget. `shots/shots.txt` is that record: all twelve frames
  were taken with all three of that game's containers up.

| frame | subject |
|---|---|
| `shots/wow-<game>-server-tab-before-refresh.png` | the Server tab reading `status: unknown`, poll off, nothing clicked |
| `shots/wow-<game>-server-tab-after-refresh.png` | the same tab after one `QTest.mouseClick` on Refresh: `status: db up, auth up, world up` |
| `shots/wow-<game>-console-tab-before-send.png` | the Console tab, panel empty (asserted 0 chars) |
| `shots/wow-<game>-console-tab-after-send.png` | the reply that came back through the widget from that game's live worldserver |

**One thing the frames show that the transcripts do not, and it is the reason the exit line
asks for this re-run.** The Server tab in every `…-before-refresh.png` carries a line Phase 7
never had — *"Command channel: verified as YULON_06CED116 at 2026-09-07 11:20 UTC"* on Vanilla,
and its equivalent on the other two, with a **Turn on the command channel** button under it.
That is 8.2's group sitting on the surface 7.9 gated, on the same tab, above the Start/Stop
buttons this gate times. The exit line's reason — *"the base controller's stop path and the
service assembly both change"* — is visible in one frame.

## Deviations from the original recipe, named

1. **The box.** 7.9's TBC half ran on `m910q`; its Vanilla and Tortoise halves ran on
   `yulon-ubuntu`, which is DEAD (its host's D: SSD left the bus on 2026-09-08). All three ran
   on `m910q` here, against that box's own installs — `~/vanilla-75b` and `~/tortoise-server`
   are m910q's, not copies of the dead box's.
2. **One server at a time.** m910q's rule. The runner stops each game through
   `Controller.stop()` before starting the next, and the stop is the app's own path rather than
   a hand-typed `docker stop` — the un-ordered `docker stop` is precisely what produced 7.9's
   `exit 139` mechanism 1. `stops.txt` is every one of those six stops with the status either
   side.
3. **A fresh clone rather than the box's `~/dads-mmo-lab`.** That checkout was at `a13a5def`
   with two modified files, and another lane owns it.
4. **Python 3.11.15**, not the 3.12.3 the 2026-09-05 sweep used — `~/gate81b-venv` is the
   interpreter every 8.x gate on this box has used.
5. **A photograph pass was added.** The 2026-09-04 run produced no screenshots at all.
6. **The harness itself is unmodified**, so the eleven-check instrument is the same one the
   ticked line describes and the numbers are comparable to it.

## The box, as it was left

`ground-end.txt` plus the inspect in `79-tbc-stop-abort.log`: **no game server up**, no game
port listening, every game container `Exited`, `r6` (not ours) still up, 28 GB free.

What this run added and left on the box, on purpose, as evidence:

* `~/tbc-7.4c/sql_scripts/backups` 13 → 21 entries, `~/vanilla-75b/…` absent → 10,
  `~/tortoise-server/…` absent → 8. These are the dumps `backup()` wrote plus the pre-restore
  safety dumps `restore()` took; they are what `verify_dump()` and `restore()` were measured
  on, so deleting them would delete the evidence. Roughly 1 GB in total (`ground-start`
  29 GB free → `ground-end` 28 GB).
* `~/lane79/` — the checkout, the runner, the drivers and `out/`, which is what this folder is
  a copy of.

Nothing was installed, reinstalled, rebuilt or removed. No account was created. No
configuration file was written.

## Files

| File | What it is |
|---|---|
| `run79-rerun.sh` | the gate runner, exactly as it executed |
| `run-shots79.sh` | the photograph runner, exactly as it executed |
| `stopper.py` | the between-games stop, through `Controller.stop()` |
| `starter.py` | the photograph pass's start + that game's own `wait_server_ready()` |
| `shots79.py` | the widget pass: two clauses, four frames, per game |
| `gate79-tbc.log`, `gate79-vanilla.log`, `gate79-tortoise.log` | the three gate transcripts |
| `shots-tbc.log`, `shots-vanilla.log`, `shots-tortoise.log` | the three photograph transcripts |
| `ground-start.txt`, `ground-after-*.txt`, `ground-end.txt` | the same probe, five times |
| `stops.txt` | all six `Controller.stop()` calls, status either side |
| `79-tbc-stop-abort.log` | the exit-139 finding: inspect, whole-log counts, and the shutdown itself |
| `run.log` | provenance header, per-game exit code and elapsed time |
| `shots/` | twelve frames and `shots.txt`, the liveness record for each |
