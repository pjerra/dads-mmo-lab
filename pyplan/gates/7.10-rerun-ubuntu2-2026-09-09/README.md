# 7.10's cross-server regression pass, RE-RUN on the merged tip — `yulon-ubuntu2`, 2026-09-09

**52 of 53.** The same 53 clauses as the 2026-09-08 run, none added, none removed, none
softened — and the score is the same number with a **different clause failing**. Both of that
night's findings are fixed and both fixes were re-measured against their own ground. The one
FAIL tonight is in the harness, not in the product, and it is reported as a FAIL rather than
adjusted, because adjusting it is what would make this run worthless.

The second half of Phase 8's exit line (`pyplan/checklist.md:2508`). The controller-surface
half is `pyplan/gates/7.9-rerun-m910q-2026-09-08/` (33 of 33). This is the cross-server
regression pass. Its predecessor is `pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/`, which this
folder is a re-run of, clause for clause.

---

## Provenance

**Code under test.** `/home/pk/lane710b/checkout` on `yulon-ubuntu2`, a fresh
`git clone --branch yulon-phase8b` made on the box at
**`2c9bd2119d5b8e042b21dc99cfa40a70909fdffc`** — `origin/yulon-phase8b`'s tip, confirmed with
`git ls-remote` before the clone. Every driver prints the package it imported and `run.log`
records it: `yulon imported : /home/pk/lane710b/checkout/pylauncher/yulon/__init__.py`.

**The two fixes are on that tip, verified by commit and not by assumption:**

| fix | commit | what it changed |
|---|---|---|
| the log-panel / runner stop path | **`d56f91b7`** | `runner.end_streams_started_on()`, `_Child.started_on`, `_StreamWorker.request_stop()` |
| the ready wait | **`576a3f93`** | `pyplan/gates/ready_wait.py`, imported by `gate-79-controller-surface.py` and by this lane's `wait_ready.py` |

Interpreter `/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python`, **Python 3.12.3**,
**PySide6 6.11.2**, **pydantic 2.13.5**, Docker 29.1.3 — the same four the 2026-09-08 run
used. `QT_QPA_PLATFORM=offscreen` throughout.

**The same trap as last time, repeated because it will mislead somebody again.** `run.log`
says `box : yulon-ubuntu`. That is the VM's *hostname*, which the rebuild kept; the machine is
**`yulon-ubuntu2`**. The original `yulon-ubuntu` is OffCritical in Hyper-V and was not touched.

**Target.** The WotLK install at `/home/pk/wowserver`, `install_id 243c46e3`, all six stages
completed, `acore_world` at **316** tables, **500** characters in world. Its
`.yulon-install.json` carries a `last_error` written by the rollback-restore lane's sabotage
press earlier tonight; it is another lane's evidence and was left byte-for-byte alone.

**A Hyper-V checkpoint was taken from the host before the first press:**
`7.10-rerun-before-2026-09-09b` (`Checkpoint-VM -Name yulon-ubuntu2`, 2026-09-09 04:52:37).
Host `U:` went 30.28 GB → 23.56 GB taking it. It is still there, alongside the rollback lane's
`before-rollback-restore-test-2026-09-09`.

**Who else was on the box.** `who` before anything: `pk seat0 (login screen)` and
`pk tty2 (gnome-session)`, both since 2026-09-08 20:57 and both **idle 7 h 52 m** — the
desktop session that is always up, not somebody working. Nothing owned by that session was
stopped. The one `sleep 3600` running on the box belongs to the rollback lane's
`restore-gate-show.sh` (PID 290856, parent `bash /home/pk/restore-gate-show.sh`) and was left
running; the probes' own children are named and are all gone.

---

## Result: 53 clauses, 52 OK, 1 FAIL

| Driver | what it drives | checks | exit | elapsed | log |
|---|---|---|---|---|---|
| `sweep_driver.py` | base `Controller` status / import / ports | 3 OK | 0 | 2 s | `sweep1.log` |
| `sweep_driver2.py` | modules, plans, repair refusal, update, console, log stream | 7 OK | 0 | 5 s | `sweep2.log` |
| `sweep_driver3.py` | backup → verify → restore → start | 7 OK | 0 | 46 s | `sweep3.log` |
| `sweep_driver4.py` | `network_apply('lan')`, realmlist file, interrupted-restore | 3 OK | 0 | 1 s | `sweep4.log` |
| `widget_driver.py` | the real buttons on `ControllerView` + `CatalogView` | **32 OK, 1 FAIL** | **1** | 9 s | `widget-run.log` |

**The drivers were not edited.** `drivers.diff` is the complete diff against the committed
2026-09-08 originals and it is **102 lines, all of them one of two things**: `lane710` →
`lane710b` (the lane's own directory), and `WIDGET0908B` → `WIDGET0909D` (a new account name,
so that *"the account does not exist before the click"* is a reading taken tonight and not one
inherited). Not one assertion, threshold, timeout or comparison was touched.

`widget_driver.py`'s exit is **1**, not the 134 of 2026-09-08. The
`QThread: Destroyed while thread '' is still running` abort is gone, which is the second thing
`d56f91b7` claimed and the cheapest of the two to check.

---

## The per-clause table, all 53

`OK`/`FAIL` are the driver's own verdicts, copied from the logs named above.

### `sweep_driver.py` — base `Controller` (3)

| # | clause | verdict | what it read |
|---|---|---|---|
| 1 | `status()` | OK | `db=True auth=True world=True` |
| 2 | `import_state()` — and it FAILS on `unreadable` | OK | `populated`, *103 rows in `acore_auth.account`, 1001 rows in `acore_characters.characters`* |
| 3 | `port_conflicts()` | OK | `[]` |

### `sweep_driver2.py` — modules, plans, refusal, update, console, stream (7)

| # | clause | verdict | what it read |
|---|---|---|---|
| 4 | `ManifestStore.load_index()` for module/ale/mod/keg | OK | all four kinds loaded |
| 5 | `network_plan('lan')` | OK | `ready=True` |
| 6 | `network_plan('internet')` | OK | `ready=True` |
| 7 | `repair_import()` refuses on a populated DB | OK | refused, naming `ac-worldserver, ac-authserver` |
| 8 | `check_for_update()` | OK | `current=0.6.59 latest=v0.6.59Public available=False error=None` |
| 9 | `send_console('server info')` — prompted, and at least one non-blank line | OK | **9 lines**, `prompted=True`, core rev `413bea61a85e+ 2026-09-04` |
| 10 | `logs_source()` yields live lines | OK | 5 live lines |

### `sweep_driver3.py` — backup → verify → restore → start (7)

| # | clause | verdict | what it read |
|---|---|---|---|
| 11 | `backup()` of the live server | OK | 4 dumps, `server_was_running=True` |
| 12 | `verify_dump()` on every dump | OK | 4 of 4 |
| 13 | `plan_restore()` refuses while running | OK | refused, naming both containers |
| 14 | `docker stop ac-authserver ac-worldserver` | OK | stopped |
| 15 | `plan_restore()` allows it once they are down | OK | no refusals |
| 16 | `restore()` | OK | `('acore_auth',)` from `20260909_050601_acore_auth.sql`, with a pre-restore safety dump |
| 17 | `controller.start()` | OK | auth + world back |

### `sweep_driver4.py` — networking (3)

| # | clause | verdict | what it read |
|---|---|---|---|
| 18 | `network_apply('lan')` | OK | `done=('ufw allow 3724/tcp', 'ufw allow 8085/tcp', 'realmlist → 172.30.48.189')`, 1 skipped (the §39 warning) |
| 19 | `write_client_realmlist()` round-trips a client file | OK | `set realmlist 192.168.1.50` in `Data/enUS/realmlist.wtf` |
| 20 | `interrupted_restore()` | OK | `None` |

### `widget_driver.py` — the real buttons (33)

| # | clause | verdict | what it read |
|---|---|---|---|
| 21 | the 5 s status poll is OFF, so only a click can fill `status_label` | OK | `'status: unknown'` before any click |
| 22 | Refresh is a real, enabled `QPushButton` reading "Refresh" | OK | |
| 23 | the click filled the status label from the live daemon | OK | `status: db up, auth up, world up` |
| 24 | the widget agrees with `docker ps` read separately | OK | all three containers |
| 25 | Send is enabled (a pty exists here) | OK | |
| 26 | the typed command reached the `QLineEdit` | OK | `'server info'`, one key event per character |
| 27 | the console round-trip came back through the widget | OK | core rev in the panel |
| 28 | the `QLineEdit` was cleared after Send | OK | |
| 29 | the account does not exist before the click | OK | `count=0` for `WIDGET0909D` |
| 30 | the GM spin box was driven to 2 by two Up keys | OK | `value=2` |
| 31 | the click wrote a real SRP6 row (salt 32, verifier 32) | OK | `108 WIDGET0909D 32 32 2`, read by `docker exec mysql` |
| 32 | the GM level the spin box showed is the one the DB got | OK | spin box 2, DB 2 |
| 33 | Apply is DISABLED before any plan exists | OK | never clicked, either |
| 34 | LAN is the default mode | OK | |
| **35** | **the plan the widget shows names the realm address the server advertises** | **FAIL** | plan names `172.30.48.189`; the row advertises `100.99.204.5`. **See below — the clause is wrong, not the widget** |
| 36 | the plan names both ports | OK | 3724 and 8085 |
| 37 | Apply became enabled once a plan existed | OK | still not clicked |
| 38 | the backup Refresh is a real button | OK | |
| 39 | the backup list came back without an error paragraph | OK | 5 rows, `problem_label=''` |
| 40 | the WotLK tile's Install is a real, enabled button | OK | |
| 41 | the Install click reached the folder picker | OK | `p710-doomed-install` |
| 42 | preflight REFUSED rather than warned | OK | `ok=False` |
| 43 | the refusal names the server's ports (this box is under that floor) | OK | ground read before the click: ports held = True |
| 44 | the refusal names free space (this box is under that floor) | OK | ground read before the click: **28.1 GiB** against preflight's 48 GiB |
| 45 | the refusal reached the user as a modal, not only a log line | OK | `Install failed`, clicked away by a real `OK` press |
| 46 | nothing was written into the chosen folder | OK | `contents=[]` |
| 47 | no compose file, so nothing could be remembered as installed | OK | the tile still reads `Install` |
| 48 | the panel's Stop exists and is dead while idle | OK | |
| 49 | Stop went live once a job was running | OK | |
| **50** | **the follow stopped after the click** | **OK** | *2026-09-08's one FAIL.* 11 lines had arrived first. **Not discriminating on this box tonight — see below** |
| 51 | the panel reports it as CANCELLED, not as finished | OK | `panel.cancelled=True panel.running=False` |
| 52 | the cancel copy for an empty folder names that folder | OK | |
| 53 | the cancel copy for the complete install names that folder | OK | and offers *"Use existing…"* |

---

## The one FAIL, clause 35 — and why it is the clause

**The widget is right and the check is wrong**, and every number needed to say so was read by
a route that is not the widget.

The clause is `widget_driver.py`'s:

```python
advertised = db_read("SELECT address FROM acore_auth.realmlist WHERE id=1;")
check("the plan the widget shows names the realm address the server advertises",
      bool(advertised) and advertised in plan_text, f"{advertised} present")
```

It compares **the address the plan PROPOSES to set** against **the address the realm row
CURRENTLY holds**. Those are two different quantities, and they are equal only when the row
already holds the box's LAN address.

* **The plan proposes `172.30.48.189`.** That is this box's only LAN IPv4:
  `ip -4 -o addr show` → `lo 127.0.0.1/8`, `eth0 172.30.48.189/20`, `docker0 172.17.0.1/16`
  and two compose bridges. `networking.py` picked the right one, and
  `shots/networking-tab-after-plan.png` is the tab rendering it.
* **The row holds `100.99.204.5`.** That address is **not on this box at all**
  (`ip -4 -o addr show | grep -c 100.99.204.5` → `0`); it is the Hyper-V host's Tailscale
  address, written into the realm row so a WoW client running on the host could reach this VM
  — the 8.6 lane's doing, recorded in
  `pyplan/gates/rollback-restore-yulon-ubuntu2-2026-09-09/README.md:68`.

So the clause is measuring a property of *the box's realm row*, not of the widget. It passed
on 2026-09-04, 09-05 and 09-08 because on each of those nights the row happened to hold the
LAN address — which means for three runs it compared the plan's own proposal to itself. **That
is the shape this project has a rule about**: a step whose assertion is already true before its
action runs proves nothing, and this one was true for a reason that had nothing to do with the
code under test. Tonight the row moved for an unrelated and legitimate reason and the clause
answered differently. It is left exactly as it is, and scored as a FAIL.

What it would take to make the clause say what it means: compare the plan text against the LAN
address `network_plan()` computed (`plan.lan_ip`) and, separately, assert that
`network_apply()` writes that value into the row. **The second half is evidenced tonight for
the first time** (see below), so the material for a correct clause is now on disk. Changing the
clause is not this lane's to do, and doing it inside a run whose job is to re-measure the same
53 would have destroyed the comparison.

**No product defect is claimed here.** `network_plan('lan')` proposing the LAN address in LAN
mode is its contract; the tab renders it; `network_apply` writes it.

---

## The 2026-09-08 findings, re-measured against their own ground

### Finding 1 — the LogPanel's Stop button. FIXED, and clause 50 is not what proves it.

**Read the ground first.** Before touching the fixed code, the pre-fix tree
(`~/lane710b/oldpylauncher`, `runner.py` and `log_panel.py` restored to `96a129dc`,
everything else the tip) was driven through the same probe. `ground-stop-probe-OLDCODE.txt`:

| source | pre-fix verdict |
|---|---|
| **B** synthetic generator, one line per 50 ms, no child process | STOPPED **0.03 s** after the click |
| **C** a real child: two lines, then `sleep 3600` | **HUNG** — `running=True cancelled=True worker._stop=True lines=2` for the full **90 s**, then killed |
| **A** the real `docker logs -f ac-worldserver` | STOPPED — but only after **26.93 s**, when the world finally printed |

**A is why clause 50 alone is not evidence tonight.** The widget driver waits 60 s for the
follow to stop. On the pre-fix code, on this box, at this hour, the world broke the blocked
read at 26.93 s — so **clause 50 would have passed tonight with the bug still in**. The world
printed **0 lines in 20 s** when measured directly (`ground-stop-probe-OLDCODE.txt`, first
block), which is quiet enough to hang a read and not quiet enough to hang it for a minute. A
verdict set by how chatty the server happens to be is exactly what made this clause pass for
eleven weeks and fail on 2026-09-08, and it would have flipped back tonight for the same
non-reason. **Source C exists so that it cannot.** No measurement of this box can make a
`sleep 3600` chatty.

**The press.** `press-stop-probe-NEWCODE.txt`, the same file, the same three sources, the same
minute, `PYTHONPATH` the only difference:

| source | pre-fix | merged tip |
|---|---|---|
| B synthetic | STOPPED 0.03 s | STOPPED 0.03 s |
| **C quiet child** | **HUNG 90 s** | **STOPPED 0.02 s** |
| A live worldserver | STOPPED 26.93 s | STOPPED **0.01 s** |

`panel.wait(10000) -> True` on all three, and the process exits **0**.

**Photographed, with the mechanism in the sidecar.** `stop_shot.py` takes the same click under
both trees and writes, beside each frame, how many of *its own named child processes* were
alive at the grab:

| frame | header | clock | Stop | its child |
|---|---|---|---|---|
| `shots/stop-quiet-child-OLDCODE-before-click.png` | the job title | running | enabled | **1 alive** |
| `shots/stop-quiet-child-OLDCODE-5s-after-click.png` | still the job title | **0:00:06** | still enabled | **1 still alive** |
| `shots/stop-quiet-child-NEWCODE-before-click.png` | the job title | running | enabled | **1 alive** |
| `shots/stop-quiet-child-NEWCODE-5s-after-click.png` | **`cancelled`** | stopped at 0:00:02 | greyed out | **0 — gone** |

The child count is the fix's own claimed mechanism observed directly: `d56f91b7` says Stop now
reaches the *source* and ends its child so the blocked read returns EOF. Pre-fix, five seconds
after the click, the child is still there and the panel is still running. Post-fix the child is
gone and the panel is not.

`shots/log-panel-after-stop.png` is clause 50's own frame, and it shows a stopped panel where
2026-09-08's showed a header still counting at 0:00:59.

### Finding 2 — the ready wait. FIXED, with its ground bounded rather than re-spent.

`ready-marker-probe2.txt`, one process, one server, three calls:

| call | result |
|---|---|
| **A (the ground)** `wait_server_ready("127.0.0.1", 3724)` — the spelling the row carried until `576a3f93`, budget bounded at 60 s | **`ready=False` after 121.8 s** |
| **B** `wait_server_ready("100.99.204.5", 8085)` — the realm row's own pair | `ready=True` after **0.2 s** |
| **C (the fix)** `ready_wait.wait_ready_for_game(entry, server_dir)` — no pair typed in the probe at all | `ready=True` after **0.4 s** |

`realm_pair()` asked the install and got `('100.99.204.5', 8085)`. Occurrences in the auth
log of `127.0.0.1:3724`: **0**. Of `100.99.204.5:8085`: **1**. The world's `ready...`: present.

A is bounded and it is *stated* that it is bounded: 2026-09-08 gave the same call 8 m 19 s and
killed it, and a ground reading does not need to re-spend that. It is still a real answer —
`False`, on a server that answers `True` in 0.2 s to the right question.

**And it was exercised for real, not only in a probe.** The runner's own step — the one that
replaced 2026-09-05's blind `sleep 180` — now runs the committed
`pyplan/gates/7.10-rerun-ubuntu2-2026-09-08/wait_ready.py`, which imports
`pyplan/gates/ready_wait.py`. It ran immediately after `sweep_driver3.py` restarted the world,
so the server really was coming up and the answer was not its start state
(`ready-after-restore.txt`):

```
wow-wotlk advertises 100.99.204.5:8085 in its realm row; waiting for its own ready marker (05:06:46)
ready=True after 36.4s (05:07:23)
```

36.4 s, against 2026-09-08's 8 m 19 s and a kill. The pair is printed **before** the wait, which
is the other half of that fix: last time the only clue in eight minutes of silence was a line
naming the wrong pair.

### Finding 3a — the ufw rules came back owned by `pk`. FIXED.

The runner now `chown root:root` / `chmod 640` after the `cp -a`, and prints the ownership
(`sweep4.log`): `-rw-r----- 1 root root` on both files, sha256 identical to
`ufw-user.rules.before` / `ufw-user6.rules.before`. **The `WARN: uid is 0 but … is owned by
1000` line that appeared in 2026-09-08's `state-before`/`state-after` diff is absent from
tonight's.**

### Finding 3b — the cleanup's account delete did not delete. FIXED.

`DELETE` with the database named on the command line. `final-state.txt`'s own check —
*"and AFTER: this must print nothing at all"* — printed nothing, and the counts are back to
**103 accounts / 3 `account_access` rows**. No hand correction was needed this time.

### The thing 2026-09-08 said `network_apply` did NOT prove. It is proved tonight.

That README's own carve-out was: the realmlist half wrote `address='172.30.48.189'` over a row
that already read `172.30.48.189`, so `NetworkReport.done`'s `realmlist → …` entry was reported
as what the app *said* it did. Tonight the row started somewhere else, and `sweep4.log` has
both readings:

```
before: 100.99.204.5
after : 172.30.48.189
put back to: 100.99.204.5
```

The row genuinely moved, by the app, and was put back by the runner. **The carve-out is
closed.** (Its cause — the row holding a non-LAN address — is also what made clause 35 fail.
The same fact does both.)

---

## Two defects in this lane's own instruments, found and kept in the record

* **A dead liveness probe, caught before it was believed.** `stop_shot.py` first ran its quiet
  child as `bash -lc "echo MARKER…; sleep 3600"` and counted survivors with `pgrep -f MARKER`.
  It printed **`child processes alive: 0`** beside every frame — including frames taken while
  the child was demonstrably streaming into the panel. The cause is bash's last-command
  optimisation: it **execs** into `sleep 3600`, and the marker is gone from the command line
  that remains (`pgrep -af` on a stand-in shows `sleep 60` and nothing else). A "0" after the
  click would have read as proof that the child was ended. Fixed with
  `exec -a <marker> sleep 3600`, and `children_alive()` now **refuses and exits 2** if it
  cannot see its own child *while the child is still streaming* — so the next dead probe fails
  loudly instead of printing a plausible number. The frames in `shots/` are from the fixed
  version; both runs of it are in the shot log.
* **A race in the stop probe's status reading, settled rather than reported.** The 05:00 press
  read `panel.status_text()` immediately after `panel.wait()` and got the job title on source A
  and `'cancelled'` on B and C. `finished` is a **queued** signal — joining the worker thread
  does not deliver it. The probe now pumps once and reads again, and
  `press-stop-probe-NEWCODE-live-status.txt` shows both readings on source A:
  `'A -- the real worldserver follow (docker logs -f)'` the instant the thread joined, then
  `'cancelled'`. **A probe artefact, not a product difference**, and no clause depends on it
  (clause 51 reads `panel.cancelled`, the flag, not the header).

---

## The ground, read before anything was pressed

`state-before.txt`, `05:05:52`:

* all three `ac-*` containers up (48 min); 3724, 8085 and 127.0.0.1:3306 listening;
* schema **22 / 111 / 316 / 30**;
* `acore_auth` **103** accounts (`101 YULON_243C46E3`, `102 PERZI`, `103 YULONADMIN`),
  `account_access` **3** rows — `WIDGET0909D` did not exist, and clause 29 asserts that
  separately by its own read;
* `acore_characters.characters` **1001**;
* realm row `1 Yulon ubuntu2 100.99.204.5 100.99.204.5 8085`;
* `ufw` **inactive**, sha256 of both rule files recorded;
* `/home/pk/wowserver/sql_scripts/backups` present and **empty** — so the five dumps in
  `state-after.txt` were written by this run;
* all three throwaway folders **absent**;
* 29 GB free on `/`, i.e. **28.1 GiB** — under preflight's 48 GiB floor, which is what makes
  clause 44 assertable and the two install-half drivers unrunnable.

`diff state-before.txt state-after.txt` is **four things and nothing else**: the containers'
uptimes (the sweep restarted them), the account the widget created, the five dumps, and the
three throwaway folders. Schema counts, realm row, ufw hashes, install record and published
ports are byte-identical — **and, unlike 2026-09-08, there is no ufw ownership warning in it.**

### The firewall plan was read before any driver could apply it

`network-plan-probe.txt`, and again inside `sweep4.log`:

```
firewall_commands:
  [0] ('ufw', 'allow', '3724/tcp')
  [1] ('ufw', 'allow', '8085/tcp')
enable at index  : None
VERDICT: no enable in the plan -- nothing to lock this box out
```

The runner refuses to run `sweep_driver4.py` at all if that probe exits non-zero. `ufw` was
inactive before and is inactive after; the two `allow` rules appear in the `user.rules` diff in
`sweep4.log` and were absent before it, so the firewall half is evidenced, not asserted.

---

## What is NOT re-run here, and why

Unchanged from 2026-09-08, and for the same reason, now with tonight's number:

| item | checks | why not |
|---|---|---|
| `widget_cancel_driver_tbc.py` — a real cancelled install, clicked and stopped | 20 | **Unreachable on this box.** preflight refuses every install at **28.1 GiB** against a 48 GiB floor, so no install can start here to be cancelled. That refusal is itself asserted through the widget (clauses 42–45) |
| `copy_shapes_driver.py` — the two cancel-copy findings on the `wow-wotlk` clone shape | 16 | same folder-shape work; needs a clone into free space this box does not have |
| the every-dump restore | — | `sweep_driver3.py` restores `report.dumps[0]` only, which is the 2026-08-28 driver's shape and is kept for comparability. The every-dump path was driven by 7.9's re-run on three servers |
| `keep_awake()` released | — | nothing here takes the inhibitor; no install was started |

53 against the 2026-09-05 run's 88 is that difference. **Every one of the 53 that could run,
ran.** Nothing was skipped for time, and no clause was scored without being executed.

---

## The diff against the 2026-09-08 run, clause by clause

| clause | 2026-09-08 | 2026-09-09 | why it moved |
|---|---|---|---|
| 50 — the follow stopped after the click | **FAIL** (still running 60 s later; exit 134) | **OK** (11 lines had arrived first) | `d56f91b7`. **But** the ground press shows the pre-fix code also passes this clause tonight (26.93 s < the 60 s wait), so the fix is evidenced by source C and by the child-liveness frames, not by this clause |
| 35 — the plan names the realm address the server advertises | OK | **FAIL** | the realm row now holds `100.99.204.5` (the 8.6 lane's, for a client on the host) instead of the LAN address. The clause compares the plan's proposal against the row; on three previous runs those were the same value. **A clause defect, not a product one** |
| 1–34, 36–49, 51–53 | OK | OK | unchanged |
| — the runner's ready wait | killed after 8 m 19 s | `ready=True` after **36.4 s** | `576a3f93` |
| — the ufw rules' ownership after restore | `pk:pk`, ufw warned | `root:root 640`, no warning | finding 3a fixed in the runner |
| — the cleanup's account delete | failed silently, deleted by hand | deleted, and the "must print nothing" check printed nothing | finding 3b fixed |
| — `network_apply`'s realmlist half | reported, not evidenced (wrote the value already there) | **evidenced**: `100.99.204.5` → `172.30.48.189` → put back | the row started somewhere else |
| — `widget_driver.py`'s exit code | **134** (SIGABRT, QThread destroyed while running) | **1** (one failed check) | `d56f91b7` |

**Score: 52 of 53 on 2026-09-08; 52 of 53 tonight.** The same total by a different route: one
real product defect closed, one harness defect exposed.

---

## An observation, recorded and not diagnosed

`NetworkReport.restart_required` came back **`True`** and `network_apply()` does not itself
restart anything. This run restarted `ac-authserver` by hand after putting the row back, and
the authserver then announced `Added realm "Yulon ubuntu2" at 100.99.204.5:8085.`
(`sweep4.log`). Whether the app surfaces that flag to the user anywhere is not something this
gate looked at, and nothing here says it does or does not.

---

## The box, as it was left

`final-state.txt`, taken after the cleanup:

* all three containers **up**, `RestartCount=0`, `exit=0` on each; world started `03:06:45Z` by
  `sweep_driver3.py`'s `controller.start()`, auth `03:07:26Z` by this run's restart;
* **`Connected players: 0. Characters in world: 500.`** — read back at 05:12 through the app's
  own console seam, with `Update time diff: 2ms` and a 500-diff mean of 34 ms;
* schema **22 / 111 / 316 / 30**, unchanged from `state-before.txt`;
* `acore_auth.account` **103** rows, `account_access` **3** — `WIDGET0909D` and its access row
  gone, `101 YULON_243C46E3`, `102 PERZI`, `103 YULONADMIN` untouched;
* **realm row `1 Yulon ubuntu2 100.99.204.5 100.99.204.5 8085`** — exactly as this run found
  it, and the **authserver was restarted** so it re-reads and announces that row;
* `ufw` **inactive**, both rule files at `state-before.txt`'s sha256 and back to
  `root:root 640`;
* the five dumps this run wrote removed; the now-empty `sql_scripts/backups` left in place,
  because the app is what creates it;
* all three throwaway folders removed; this lane's own probe children all gone (the one
  `sleep 3600` still on the box belongs to the rollback lane);
* `/home/pk/wowserver/.yulon-install.json` byte-for-byte unchanged, `last_error` and all;
* `~/wowserver/lua_scripts` untouched — the owner's `LootPet.lua` was not read or moved;
* disk **29 GB free**, the same figure as before the run;
* checkpoint `7.10-rerun-before-2026-09-09b` **kept** on the host, host `U:` 23.56 GB free;
* `~/lane710b/` kept — `checkout` (the tip), `oldpylauncher` (the pre-fix `runner.py` and
  `log_panel.py` used for the ground press), `drivers`, the probes and `out/`, which is what
  this folder is a copy of.

Nothing was installed, reinstalled, rebuilt or removed. No module was applied. No configuration
file was written. No server other than this one was touched, on this or any other box.

## Files

| File | What it is |
|---|---|
| `run-710-rerun.sh` | the runner, exactly as it executed (with findings 3a and the wait fix in it) |
| `cleanup-710.sh` | the cleanup, exactly as it executed (with finding 3b fixed) |
| `patch_lane.py` | the three edits applied to the 2026-09-08 copies of those two scripts, with the reason for each |
| `scripts.diff` | those edits as a diff against the committed 2026-09-08 scripts |
| `drivers/` | every driver exactly as run |
| `drivers.diff` | their complete diff against the 2026-09-08 originals — 102 lines, all path or account name |
| `ground-stop-probe-OLDCODE.txt` | **the ground for finding 1**: the pre-fix tree, three sources, and the world's measured line rate |
| `press-stop-probe-NEWCODE.txt` | the same probe on the merged tip |
| `press-stop-probe-NEWCODE-live-status.txt` | source A again, with the queued-signal race removed |
| `log_panel_stop_probe2.py` | that probe |
| `stop_shot.py`, `shots/stop-quiet-child-*.png` | the two trees photographed five seconds after the same click, with each frame's own child count |
| `ready_marker_probe2.py`, `ready-marker-probe2.txt` | **finding 2**: A (the old spelling, bounded), B (the row's pair), C (the committed fix) |
| `ready-after-restore.txt` | the committed wait, run for real after the restore restarted the world |
| `network_plan_probe.py`, `network-plan-probe.txt` | the LAN plan read before anything could apply it |
| `sweep1.log` … `sweep4.log`, `widget-run.log` | the five driver transcripts |
| `state-before.txt`, `state-after-restore.txt`, `state-after.txt`, `final-state.txt` | the same probe, four times |
| `ufw-user.rules.before`, `ufw-user6.rules.before` | the copies taken before the networking driver |
| `run.log` | exit codes and elapsed times as the runner wrote them |
| `shots/` | 17 frames and `shots.txt`, the liveness record for each |
