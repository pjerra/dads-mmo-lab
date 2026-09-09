# 7.10's widget half, re-run with the self-comparing clause corrected — `yulon-ubuntu2`, 2026-09-09

## 33 of 33.

`widget-run.log:127` — `RESULT: 33 OK, 0 FAIL`, exit 0. The same 33 clauses as
`../7.10-rerun-ubuntu2-2026-09-09/widget-run.log`, none added, none removed, none softened;
one of them rewritten, and the FAIL that run reported is gone because the clause that produced
it now measures something.

The clause is `drivers/widget_driver.py:296-306` of that folder — the file this ticket owns and
the only product-side file changed. **It compared the plan to itself.** It read
`acore_auth.realmlist.address` out of the database and asserted that string appeared in the plan
text the Networking tab shows. But `network_plan('lan')` **proposes** this box's LAN address
(`172.30.48.189`) and the row **holds** whatever was last applied. On 2026-09-04, 09-05 and
09-08 the row happened to hold the LAN address, so for three runs the clause could not have
failed; on 09-09 the 8.7a lane had put `100.99.204.5` there so the owner's client reaches this
VM through the Hyper-V host, and it failed while saying nothing whatever about the widget.

A corrected clause is worth nothing until it has been watched failing, so this folder contains
four runs and not one: `clause35-falsify.log`, in which both corrected halves are driven until
they FAIL and then until they PASS; `widget-run.log`, the full 33; `failclosed/`, in which a
driver is made to fail; and `failrestore/`, in which the harness's own restoration is made to
fail — because a harness that cannot report a failure is the same defect one level up, and a
restoration that cannot report one is that defect in the piece that protects the owner's live
box.

---

## Provenance

**Code under test.** `/home/pk/lane710b/checkout` on `yulon-ubuntu2`, moved for this run from
`2c9bd211` (what the 09-09 baseline measured) to **`528f219e8c883bebdf2ee6c225f3d5468989442a`**,
the merged `yulon-phase8b` tip this ticket's worktree is based on. `run.log:2` records it and
every driver prints the package it imported (`run.log:6`).

That is a deliberate move, not a drift: `pylauncher/yulon` differs between those two shas by
exactly two files. `apply.py` grew by 140 lines, from 2105 to 2245, in `f8cd55df` — round 1 of
this folder called it a new file and it is not one; the lines are new, the file is not, and
nothing this driver reaches is in them. `ui/widgets/log_panel.py` is +18/−2, the
`_StreamWorker.stop()` guard that stops a directly-driven worker quitting the GUI thread's event
loop, and that one **is** under the **LogPanel Stop** clauses this driver presses; they pass on
the newer code (`widget-run.log:106-110`).

Interpreter `/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python`, **Python 3.12.3**, **PySide6
6.11.2**, **pydantic 2.13.5**, Docker 29.1.3, `QT_QPA_PLATFORM=offscreen` — the same five the
09-09 run used.

**The same trap, restated because it will mislead somebody again.** `run.log:7` says
`box : yulon-ubuntu`. That is the VM's *hostname*, which the rebuild kept; the machine is
**`yulon-ubuntu2`**. The original `yulon-ubuntu` is on the failed D: SSD and was not touched.

**Target.** The WotLK install at `/home/pk/wowserver`, world up on **pid 409560** throughout —
the process T2 left, still the same pid in `state-final.txt`. 1002 characters. The server was
never stopped, started or restarted by this lane; `ac-authserver` was restarted once, on
purpose, and only after the realm row had been put back (below).

**Three lane constants, not assertions**, are read from the environment by the corrected
driver, each defaulting to the value the 09-09 run used so an unset environment reproduces that
run exactly (`run-t3.sh:47-49`):

| constant | 09-09 | tonight | why |
|---|---|---|---|
| `WIDGET_ACCOUNT` | `WIDGET0909D` | `WIDGET0909T3` | *"the account does not exist before the click"* is only a reading if the name is new |
| `WIDGET_ACCOUNT_PW` | `widget0909dpw` | `widget0909t3pw` | it travels with the name; the account is deleted on the way out |
| `WIDGET_SHOTS` | `…/out/shots` | `…/out-t3/shots` | so the 09-09 run's frames are not overwritten |

---

## The corrected clause

The two halves the old clause ran together are now separated.

**The proposal half** is the clause in the driver. It asserts the text a user reads names the
address the widget's **own** plan object computed — `plan.lan_ip in plan_text`, where
`plan = view._plan`, the `NetworkPlan` the widget itself is showing. It is falsifiable:
`_format_plan()` renders `plan.lan_ip or '?'`, so a plan that resolved no LAN address, or a tab
left showing an older plan, makes it false. Both are watched happening in `clause35-falsify.log`.

**The apply half** — that the row the server advertises *becomes* the plan's address, and only
after Apply — is **not** asserted in `widget_driver.py`, because that driver never presses
Apply: *"the 7.1 lane's ufw lockout came from that button"* is its own reason, and its module
docstring is the contract. It is cited in the corrected clause's comment from
`../7.10-rerun-ubuntu2-2026-09-09/sweep4.log:79-83`, which reads the row either side of
`network_apply('lan')` — `before: 100.99.204.5`, `after : 172.30.48.189`,
`put back to: 100.99.204.5`.

**And it is re-measured tonight rather than only cited**, through the widget's own `Apply`
QPushButton, in `clause35_falsify.py`. That driver is where the button gets pressed, so the
regression driver keeps its never-press design and the apply half still gets a live reading of
its own, with its ground read first.

The clause's new detail line prints the row *beside* the plan and says whether they agree, so a
reader can see the dependency is gone rather than take it on trust — `widget-run.log:60-62`:

```
       plan.lan_ip, the address the widget computed: '172.30.48.189'
       realmlist.address, read by docker exec mysql: '100.99.204.5'   (the row is NOT what this
       clause compares against; it is printed so a reader can see whether the two agree tonight)
[OK]   the plan the widget shows names the LAN address the widget itself computed --
       plan.lan_ip='172.30.48.189', row='100.99.204.5', they DISAGREE -- which this clause no
       longer depends on
```

That single line is the whole repair: **the same box, the same row, the same disagreement that
made the old clause FAIL, and the corrected clause passes on the merits.**

---

## The deliberate disagreement: watched failing, then watched passing

`clause35_falsify.py`, log in `clause35-falsify.log`. Both predicates are written **once**, at
the top of the file (`proposal_clause()` and `apply_clause()`), and every evaluation calls one
of them — a driver that retyped the comparison at each site could quietly evaluate a different
expression at the one it wanted to fail. Each evaluation also declares in advance whether it is
supposed to pass, and an evaluation that goes the other way is listed under `UNEXPECTED` and
exits non-zero: otherwise *"I proved it can fail"* is a claim rather than a reading.

**Part A — the proposal half.** The mutation is of the INPUT (a `dataclasses.replace` of a
frozen `NetworkPlan`), never of the clause.

| | evaluation | expected | got | log |
|---|---|---|---|---|
| A1 | the real plan against the text the widget rendered for it | OK | **OK** | `:23` |
| A2 | the same clause against a plan whose `lan_ip` is not what the tab shows | FAIL | **FAIL** | `:24` |
| A3 | that mutant handed to the widget's own `_plan_ready`, so it re-renders | OK | **OK** | `:30` |

```
[FAIL] A2  the same clause, against a plan whose lan_ip is NOT what the tab shows --
       mutant.lan_ip='192.168.77.77' does not appear in the text the widget rendered for
       '172.30.48.189' -- the clause SAYS SO, which is what the row-comparison could not do
[OK]   A3  and passes again once the widget renders the plan it was given -- the clause follows
       the WIDGET ('192.168.77.77' now on screen), not the row ('100.99.204.5', unchanged
       throughout A)
```

A3 is the point of A2: the clause tracks the **widget**, and the row it used to depend on did
not move at any point in Part A.

**Part B — the apply half, with its ground.** The row is deliberately set to a third address,
`192.168.77.77` — neither the plan's nor the one this box advertises, and on no interface here,
so nothing can reach it by accident during the seconds it holds it.

| | step | expected | got | log |
|---|---|---|---|---|
| B1 | the GROUND read, then the row set to `192.168.77.77` and read back | — | `100.99.204.5` → `192.168.77.77` | `:36-38` |
| B2 | the ground **disagrees** with the plan, so the clause has something to do | OK | **OK** | `:39` |
| B3 | `row == plan.lan_ip`, evaluated **before** Apply | FAIL | **FAIL** | `:40` |
| B4 | the real `Apply` QPushButton pressed with `QTest.mouseClick` | — | 3 done, 1 refused | `:42-52` |
| B5 | **the same expression**, on the row read again | OK | **OK** | `:55` |
| B6 | row put back to `100.99.204.5`/`100.99.204.5`/`255.255.255.0`, read back | OK | **OK** | `:56-58` |

```
[FAIL] B3  the row the server advertises holds the plan's LAN address -- row='192.168.77.77'
       plan.lan_ip='172.30.48.189' -- BEFORE Apply, and the clause fails, which is the whole
       point of correcting it
[OK]   B5  the row the server advertises holds the plan's LAN address -- row '192.168.77.77' ->
       '172.30.48.189', plan.lan_ip '172.30.48.189' -- the SAME clause, the same expression, and
       the only thing that changed is that the button was pressed
```

B2 is the guard the ticket asks for and it is not decoration: had the ground already equalled
the plan, the driver refuses and returns 2 without pressing anything, because **a clause true
before its action is the defect this whole round exists to repair.**

Every reading of the row is taken by `docker exec ac-database mysql`, a route that is not the
widget. The password is passed in `MYSQL_PWD`, so it is on no command line these logs record.

**Apply really applied, and was really put back.** It added two `ufw allow` rules and ran the
realmlist UPDATE; it did **not** run `ufw enable` — that is the plan's own refusal, rendered in
full in the tab (`clause35-falsify.log:51`). `run-t3.sh` copies `/etc/ufw/user{,6}.rules` aside
before the driver; the diff the apply made is appended to the driver's own log
(`clause35-falsify.log:65-102`), and the restore is done by the EXIT trap, which restates
ownership and mode rather than leaving them to `cp -a` (the 2026-09-08 finding 3a) and prints
the sha256s that prove it: `320f53e1…` and `f6696cd7…` on both the live files and the copies
(`restore.log:5-8`). `ufw` was `inactive` before and after.

---

## The 33 clauses, against `../7.10-rerun-ubuntu2-2026-09-09/widget-run.log`

**31 of the 33 are identical in wording and verdict.** One clause changed — number 15 — and it
went FAIL → OK. One more, number 24, has the same verdict and a different label, because its
label interpolates the free-space reading taken before the click (`free 28.1 GiB` → `free 26.9
GiB`); that is the clause working as designed, not a clause moving, and it is called out here
rather than counted as identical.

| # | clause | 09-09 | tonight |
|---|---|---|---|
| 1 | the 5 s status poll is OFF, so only a click can fill status_label | OK | OK |
| 2 | Refresh is a real QPushButton with the text a user reads | OK | OK |
| 3 | the click filled the Server tab's status label from the live daemon | OK | OK |
| 4 | what the widget says agrees with `docker ps` read separately | OK | OK |
| 5 | Send is enabled on Linux (a pty exists here) | OK | OK |
| 6 | the typed command reached the QLineEdit | OK | OK |
| 7 | the console round-trip came back through the widget | OK | OK |
| 8 | the QLineEdit was cleared after Send | OK | OK |
| 9 | the account does not exist before the click | OK | OK |
| 10 | the GM spin box was driven to 2 by two Up keys | OK | OK |
| 11 | the click wrote a real SRP6 account row (salt 32, verifier 32) | OK | OK |
| 12 | the GM level the spin box showed is the GM level the DB got | OK | OK |
| 13 | Apply is DISABLED before any plan is shown | OK | OK |
| 14 | LAN is the default mode | OK | OK |
| **15** | ~~the plan … names the realm address the server advertises~~ → **the plan the widget shows names the LAN address the widget itself computed** | **FAIL** | **OK** |
| 16 | the plan the widget shows names both ports | OK | OK |
| 17 | Apply became enabled once a plan existed (and is still not clicked) | OK | OK |
| 18 | the backup Refresh is a real button | OK | OK |
| 19 | the backup list came back without an error paragraph | OK | OK |
| 20 | the WotLK tile's Install is a real, enabled QPushButton | OK | OK |
| 21 | the Install click reached the folder picker | OK | OK |
| 22 | the preflight REFUSED rather than warned, and nothing was installed | OK | OK |
| 23 | the refusal names the server's ports, which this box is under | OK | OK |
| 24 | the refusal names free space, which this box is under *(label carries the reading: `28.1 GiB` → `26.9 GiB`)* | OK | OK |
| 25 | the refusal reached the user as a modal dialog, not only a log line | OK | OK |
| 26 | nothing was written into the chosen folder | OK | OK |
| 27 | no compose file, so nothing could be remembered as installed | OK | OK |
| 28 | the panel's Stop button exists and is dead while idle | OK | OK |
| 29 | Stop went live once a job was running | OK | OK |
| 30 | the follow stopped after the click | OK | OK |
| 31 | the panel reports it as CANCELLED, not as finished | OK | OK |
| 32 | the copy for a folder with nothing in it names that folder | OK | OK |
| 33 | the copy for the folder holding the real, complete install names that folder | OK | OK |

**Three readings the two runs disagree on, none of them a clause, all recorded rather than
smoothed:**

* **Free space, 28.1 GiB → 26.9 GiB.** Still under preflight's 48 GiB floor, so clause 24 asserts
  the space refusal for the same reason it did last night, decided by a reading taken before the
  click and not inherited.
* **The backup list, 5 rows → 0.** Clause 19 asserts the list came back *without an error
  paragraph*, which it did, so the count is not what it measures. `/home/pk/wowserver/sql_scripts/backups`
  is empty with an mtime of **2026-09-09 05:11**, four minutes after the 09-09 sweep's own
  `widget_driver.py` finished at 05:07:45 and six hours before this run started. Nothing in this
  lane reads or writes that directory except the Refresh press.
* **Server uptime, 52 s → 1 h 57 s.** The 09-09 run pressed Send seconds after its restore
  driver restarted the world; tonight the world is the one T2 left up, pid 409560.

---

## The box, left as T2 left it

`state-final.txt`, taken after everything:

* **world up, still pid 409560** — never stopped, started or restarted by this lane.
* **realm row `100.99.204.5` / `100.99.204.5`, mask `255.255.255.0`** — restored twice over and
  **verified** once: by the driver's own B6 clause, then unconditionally by the EXIT trap, which
  re-reads `id 1` on all three columns and would have said `[RESTORATION FAILED]` if it did not
  match (`restore.log:10-14`). A driver that died between B1 and B6 would leave the owner's box
  advertising `192.168.77.77`, and no assertion in a process that is gone can put it back.
* **`ac-authserver` restarted** after the restore, and the check is its own newest announcement
  *since that restart began*, not any line in a tail: `Added realm "Yulon ubuntu2" at
  100.99.204.5:8085.` (`restore.log:15-19`).
* **`ufw` inactive, rules byte-identical** to the copies taken before the Apply, root:root 640 —
  compared by sha256 rather than assumed (`restore.log:5-8`).
* **The world is on the pid it started on**, checked rather than stated: the trap records
  `pgrep worldserver` before the script does anything and compares it at the end
  (`restore.log:25-27`).
* **The owner's things untouched, and now actually watched.** `PERZI`'s `Pakka` (level 6) is
  there and 1002 characters; `Logger.ALE=4,Console Server` is still line **706** of
  `/home/pk/wowserver/env/dist/etc/worldserver.conf`, mtime 2026-09-08 23:11 — before this lane
  existed; his `LootPet2.lua` (mtime 2026-09-09 02:06) and the five `dml_*.lua` bridge scripts
  are all present in `/home/pk/wowserver/env/dist/etc/modules/lua_scripts/`, byte counts and
  mtimes identical in `state-before.txt` and `state-final.txt`.

  Round 1's probe read `/home/pk/wowserver/lua_scripts/LootPet.lua` and
  `/home/pk/wowserver/etc/worldserver.conf`. **Neither exists**, and both its `state-before` and
  its `state-after` said `No such file` — so the pair agreed about nothing, and would have gone
  on agreeing if the real files had been deleted. That is a dead probe reporting "no change",
  and it is the same defect as a clause that cannot fail. The paths above are the ones the
  engine loads.
* **The account this run created is gone.** `WIDGET0909T3` (id 111) was created by clause 11,
  which is why `state-after.txt` still lists it; the EXIT trap deletes it and its
  `account_access` row on the way out, and `state-final.txt` shows `account_access` back to
  `101 102 103 109`, exactly `state-before.txt` (`restore.log:20-24`). Round 1 used a joined
  multi-table `DELETE` that did not take and left an orphan access row to be removed by hand;
  it is two plain statements now, access first. The password
  `run-t3.sh:48` gives that account is a throwaway that authenticates nothing: the account it
  belonged to no longer exists, and it is written down for the same reason the 09-09 driver
  writes its own default down — so a re-run reproduces the constants rather than inventing them.
  No other password appears in this folder; every database reading passes its own in `MYSQL_PWD`
  rather than on a command line.
* `/home/pk/lane710b/checkout` is left at `528f219e`, not the `2c9bd211` it was found on.

---

## The runner fails closed, and that was exercised rather than asserted

Round 1's `run-t3.sh` captured each driver's exit status, wrote it into `run.log`, and then threw
it away: `run_driver` ended on an `echo`, so what it returned was that echo's `0`, and with only
`set -u` the script walked on through its cleanup and printed *"run finished"* whatever had
happened. A falsification driver that died between B1 and B6 would have left the owner's box
advertising `192.168.77.77` under a log that said the run was fine — the same family of defect as
the clause this folder exists to repair: a reading that cannot come out wrong.

`run_driver` returns the driver's status now (`run-t3.sh:189`), both drivers are invoked
fail-fast through `die`, and **everything that puts the box back is in an EXIT trap** — the ufw
rules, the realm row on both address columns and its mask, the `ac-authserver` restart, and the
account the run creates. The trap runs on the way out of a failure exactly as on the way out of a
success and re-raises the status it was called with, so the script's exit code is the driver's
-- and, since round 3, its own if a restoration did not verify (next section).

**And it was run that way.** `failclosed/` is the same `run-t3.sh`, same box, same trap, with
`T3_DRIVERS` pointed at a stand-in for `clause35_falsify.py` that does its B1 — reads the realm
row, sets it to `192.168.77.77`, reads it back — and then exits 1 where B6 would have put it
back. The stub asserts nothing; one that could fail for a second reason would make the reading
ambiguous.

| what had to happen | where it is |
|---|---|
| the driver exits 1 and the runner **stops there** | `failclosed/run.log:17-18` — `clause35_falsify.py exit 1`, `STOPPED: … (status 1)` |
| the run is recorded as FAILED, not finished | `failclosed/run.log:20` — `T3 run FAILED, status 1, box restored and verified` |
| the runner's own exit status is the driver's | `failclosed/run.log:21` — `RUNNER EXIT STATUS (as seen by the invoking shell): 1`, captured into the log rather than quoted from a terminal |
| the trap sees the damage | `failclosed/restore.log:11` — `as found now (id 1): 192.168.77.77\|192.168.77.77\|255.255.255.0` |
| and undoes it anyway, **and checks that it did** | `failclosed/restore.log:12-14` — read back on all three columns, then `[RESTORED]` |
| the authserver re-reads it | `failclosed/restore.log:17-19` — the last `Added realm` line written *since the restart began* |
| the world was never touched | `failclosed/restore.log:26-27` — pid `409560` at the start of the script and at the end |
| and every step said so | `failclosed/restore.log:29` — `every restoration verified; this script exits 1` |

`failclosed/clause35-falsify.log` carries the stub's own output under that name because the
runner names each log for the driver it ran and the stand-in occupies that driver's place; its
first lines say plainly which file it is.

---

## The trap had the same defect one level down, and that was exercised too

Round 2's trap ran unconditionally, which is what was asked, and **checked nothing**. It saved
the status it was called with and then ran `cp`, `chown`, an `UPDATE`, `docker restart` and two
`DELETE`s with every result discarded — `set -u` and `pipefail`, no `errexit`. A realm UPDATE
that matched no row, an authserver that came back announcing the wrong address, a `cp` that
failed, a delete that did not take: any of them and the run still ended *"run finished"*, exit 0.
That is the clause-that-cannot-fail defect for the third time, now in the piece whose whole job
is protecting the owner's live box.

**Round 3: every restoration step verifies its own postcondition against a fresh reading taken
afterwards**, and a step that fails does not stop the ones after it — a run whose ufw restore
failed still needs its realm row back.

| step | what is read back afterwards |
|---|---|
| ufw | sha256 of `/etc/ufw/user{,6}.rules` against the `.before` copies, **and** `root:root 640` on both |
| the realm row | `id 1` re-read and compared on **all three** columns against `100.99.204.5\|100.99.204.5\|255.255.255.0` |
| ac-authserver | the **last** `Added realm` line written **since the restart began**, and it must name `100.99.204.5:8085` |
| the account | zero rows for `WIDGET0909T3`, **and** zero orphaned `account_access` rows at any id |
| the world | the pid now equals the pid recorded before the script did anything |
| the final reading | `state-final.txt` exists and is non-empty |

Any failure writes a distinct `[RESTORATION FAILED]` line, and the script then exits **90** —
not the drivers' status, which is kept in the log beside it.

**Exercised**, in `failrestore/`: the same runner and the same failing stub driver, with
`T3_REALM_ROW_ID=9999` so the realm restore's `UPDATE` targets a row that does not exist while
the verification reads `id 1`, the row this box actually advertises. That is a restoration that
cannot succeed, and the check has to notice.

| what had to happen | where it is |
|---|---|
| the drivers' own status is kept, not overwritten | `failrestore/restore.log:2` — `the status the drivers left: 1` |
| the injection is on the record | `failrestore/restore.log:4` — `realm UPDATE targets row id: 9999   (verification always reads id 1)` |
| the realm restore is caught | `failrestore/restore.log:14` — `[RESTORATION FAILED] realmlist id 1 reads '192.168.77.77…', wanted '100.99.204.5…' — the row this box advertises is WRONG` |
| and so is what it caused | `failrestore/restore.log:19` — `[RESTORATION FAILED] ac-authserver's newest announcement since the restart is …192.168.77.77:8085…` |
| the other restorations still ran | `failrestore/restore.log:8`, `:24`, `:27`, `:28` — ufw, the account, the world's pid and the final reading all `[RESTORED]` |
| a distinct result, not the driver's | `failrestore/restore.log:29` and `failrestore/run.log:20-22` — `RESTORATION FAILED: 2 step(s) did not verify. The drivers' own status was 1.` |
| a non-zero exit | `failrestore/run.log:23` — `RUNNER EXIT STATUS (as seen by the invoking shell): 90` |
| the world stayed up throughout | `failrestore/restore.log:26-27` — pid `409560`, unchanged |
| the row put back by hand afterwards | `failrestore/hand-restore.txt` — `192.168.77.77…` read out, the UPDATE, `100.99.204.5\|100.99.204.5\|255.255.255.0` read back, `ac-authserver` restarted and its newest `Added realm` line naming `100.99.204.5:8085`, pid still `409560` |

**The exercise found a real defect in the check it was exercising.** The first version of the
authserver check read `docker logs --tail 80` and asked whether *any* line named the target
address. On the first failing-restoration run it **passed** — with the row deliberately left at
`192.168.77.77` and the container announcing `192.168.77.77` — because a *previous* run's
`Added realm … 100.99.204.5:8085` was still inside the last 80 lines. A stale marker read as a
fresh one, in the check written to prevent exactly that. It reads only lines written since the
restart began, and only the last announcement, and the run above is the one taken after the fix.
The first run's log is not kept; what is kept is the corrected check and the reading that proves
it bites.

## Files

| file | what it is |
|---|---|
| `widget-run.log` | the full 33-clause widget half, 33 OK 0 FAIL, exit 0 |
| `clause35-falsify.log` | both corrected halves watched failing and then passing, plus the ufw copy-aside/restore and the authserver restart |
| `clause35_falsify.py` | the driver that produced it |
| `run-t3.sh` | the runner: state probes, the ufw handling, both drivers fail-fast, and the EXIT trap that puts the box back whatever happened |
| `restore.log` | written by that trap, one `[RESTORED]` or `[RESTORATION FAILED]` line per step, each with the reading it was decided on |
| `failclosed/` | the same runner invoked so a **driver** deliberately fails — the trap still runs and every step verifies |
| `failrestore/` | the same runner invoked so a **restoration** deliberately fails — two `[RESTORATION FAILED]` lines, exit 90, the other steps still done |
| `failclosed_stub.py` | the stand-in driver both of those use: it moves the realm row and exits 1 |
| `failrestore/hand-restore.txt` | the realm row put back by hand after that deliberate failure, read back on all three columns, with the authserver's newest `Added realm` line |
| `widget_driver.diff` | the complete change to `../7.10-rerun-ubuntu2-2026-09-09/drivers/widget_driver.py` — 46 insertions, 6 deletions, all of it the one clause and the two lane constants |
| `run.log` | shas, versions, the world's pid, the two knobs' values, exits, elapsed |
| `gates.txt` | the two gate result lines for the commit this folder ships with, so the folder is self-contained |
| `state-before.txt`, `state-after.txt`, `state-final.txt` | the box before the drivers, after them, and after the EXIT trap has put it back — `after` is the one that still lists the account the run created |
| `ufw-user.rules.before`, `ufw-user6.rules.before` | the copies the restore was checked against |
| `shots/` | 13 frames from the widget run + 4 from the falsify run, each with the containers `docker ps` reported up at the instant of the grab |

`failclosed/` and `failrestore/` have **no `state-after.txt`**, and that is not an omission:
`probe after` runs only after both drivers are green, and in those two invocations the first
driver exits 1. Their `state-before.txt` and `state-final.txt` are the pair — before anything,
and after the trap.

**One thing this folder cannot claim.** `../7.10-rerun-ubuntu2-2026-09-09/drivers/widget_driver.py`
now carries the correction, and that folder's own `widget-run.log` was produced by the file as it
stood *before* it. The ticket puts the fix in that file; this note is here so the folder does not
read as though its log came from the driver now sitting beside it.
