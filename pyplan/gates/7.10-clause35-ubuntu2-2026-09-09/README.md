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
two runs and not one: `clause35-falsify.log`, in which both corrected halves are driven until
they FAIL and then until they PASS, and `widget-run.log`, the full 33.

---

## Provenance

**Code under test.** `/home/pk/lane710b/checkout` on `yulon-ubuntu2`, moved for this run from
`2c9bd211` (what the 09-09 baseline measured) to **`528f219e8c883bebdf2ee6c225f3d5468989442a`**,
the merged `yulon-phase8b` tip this ticket's worktree is based on. `run.log:2` records it and
every driver prints the package it imported (`run.log:6`).

That is a deliberate move, not a drift: `pylauncher/yulon` differs between those two shas by
exactly two files — `apply.py` (new, 140 lines, not reached by this driver) and
`ui/widgets/log_panel.py` (+18/−2, the `_StreamWorker.stop()` guard that stops a directly-driven
worker quitting the GUI thread's event loop). The second of those is under the **LogPanel Stop**
clauses this driver presses, and they pass on the newer code (`widget-run.log:106-110`).

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

**Two lane constants, not assertions**, are read from the environment by the corrected driver,
each defaulting to the value the 09-09 run used so an unset environment reproduces that run
exactly (`run-t3.sh:27-30`):

| constant | 09-09 | tonight | why |
|---|---|---|---|
| `WIDGET_ACCOUNT` | `WIDGET0909D` | `WIDGET0909T3` | *"the account does not exist before the click"* is only a reading if the name is new |
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
before the driver and puts them back after, ownership and mode restated rather than left to
`cp -a` (the 2026-09-08 finding 3a), and the log carries the diff the apply made and the
sha256s proving the restore: `320f53e1…` and `f6696cd7…` on both the live files and the copies
(`clause35-falsify.log:103-108`). `ufw` was `inactive` before and after.

---

## The 33 clauses, against `../7.10-rerun-ubuntu2-2026-09-09/widget-run.log`

**32 of the 33 are identical in wording and verdict.** One clause changed — number 15 — and it
went FAIL → OK. Nothing else moved.

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
| 24 | the refusal names free space, which this box is under | OK | OK |
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
* **realm row `100.99.204.5` / `100.99.204.5`, mask `255.255.255.0`** — restored twice over: by
  the driver's own B6 clause, then unconditionally by `run-t3.sh`, because a driver that died
  between B1 and B6 would leave the owner's box advertising `192.168.77.77` and no assertion in
  a process that is gone can put it back.
* **`ac-authserver` restarted** after the restore, and its own log is the reading:
  `Added realm "Yulon ubuntu2" at 100.99.204.5:8085.`
* **`ufw` inactive, rules byte-identical** to the copies taken before the Apply, root:root 640.
* **The owner's things untouched.** `PERZI`'s `Pakka` (guid 1001, level 6) is there; 1002
  characters; `Logger.ALE=4,Console Server` is still line **706** of
  `/home/pk/wowserver/env/dist/etc/worldserver.conf`, whose mtime is 2026-09-08 23:11 — before
  this lane existed; `LootPet.lua` is 2026-09-08 22:17. *(Its path is `/home/pk/LootPet.lua`;
  there is no `lua_scripts` directory anywhere under `/home/pk`, which the ticket's description
  of it will not lead you to.)*
* **The account this run created is gone.** `WIDGET0909T3` (id 110) was created by clause 11 and
  deleted afterwards — `account-cleanup.txt`. Its `account_access` row survived the first delete
  and is the reason `state-after.txt` still lists `110 2`: the joined multi-table `DELETE` in
  `run-t3.sh` did not take, and the row was removed by hand a minute later. `state-final.txt`
  shows `account_access` back to `101 102 103 109`, exactly `state-before.txt`. The password
  `run-t3.sh:28` gives that account is a throwaway that authenticates nothing: the account it
  belonged to no longer exists, and it is written down for the same reason the 09-09 driver
  writes its own default down — so a re-run reproduces the constants rather than inventing them.
  No other password appears in this folder; every database reading passes its own in `MYSQL_PWD`
  rather than on a command line.
* `/home/pk/lane710b/checkout` is left at `528f219e`, not the `2c9bd211` it was found on.

---

## Files

| file | what it is |
|---|---|
| `widget-run.log` | the full 33-clause widget half, 33 OK 0 FAIL, exit 0 |
| `clause35-falsify.log` | both corrected halves watched failing and then passing, plus the ufw copy-aside/restore and the authserver restart |
| `clause35_falsify.py` | the driver that produced it |
| `run-t3.sh` | the runner: state probes, the ufw handling, both drivers, the account cleanup |
| `widget_driver.diff` | the complete change to `../7.10-rerun-ubuntu2-2026-09-09/drivers/widget_driver.py` — 46 insertions, 6 deletions, all of it the one clause and the two lane constants |
| `run.log` | shas, versions, exits, elapsed |
| `state-before.txt`, `state-after.txt`, `state-final.txt` | the box either side, and after the orphan row was removed |
| `account-cleanup.txt` | the account created and removed |
| `ufw-user.rules.before`, `ufw-user6.rules.before` | the copies the restore was checked against |
| `shots/` | 13 frames from the widget run + 4 from the falsify run, each with the containers `docker ps` reported up at the instant of the grab |

**One thing this folder cannot claim.** `../7.10-rerun-ubuntu2-2026-09-09/drivers/widget_driver.py`
now carries the correction, and that folder's own `widget-run.log` was produced by the file as it
stood *before* it. The ticket puts the fix in that file; this note is here so the folder does not
read as though its log came from the driver now sitting beside it.
