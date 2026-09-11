# 7.10's cross-server regression pass, RE-RUN on the merged Phase 8b tip — `yulon-ubuntu2`, 2026-09-09

The second half of Phase 8's exit line (`pyplan/checklist.md:2508`): *"Phase 7's
controller-surface gate and cross-server regression pass re-run green on the merged tip,
because the base controller's stop path and the service assembly both change."* The
controller-surface half is `pyplan/gates/7.9-rerun-m910q-2026-09-08/`. This is the
cross-server regression pass — WotLK's 6.5 coverage gate, re-run against the tip.

**It did not come back all green, and that is the report.** 52 of 53 checks pass. The one
failure is a real defect on the merged tip, reproduced on demand, and the two probes beside it
found a second one that has been latent in a gate harness since 7.9.

---

## Provenance

**Code under test.** `/home/pk/lane710/checkout` on `yulon-ubuntu2`, a fresh
`git clone --branch yulon-phase8b` made on the box, at
**`96a129dcd03866e3ac997d1be1e690553f92c535`** — `origin/yulon-phase8b`'s tip. Every driver
prints the package it imported and the runner records it:
`yulon imported : /home/pk/lane710/checkout/pylauncher/yulon/__init__.py`, so the checkout is
proved rather than assumed (the box's own `~/dads-mmo-lab` is at `e40f4590` and belongs to
other lanes).

Interpreter `/home/pk/dads-mmo-lab/pylauncher/.venv/bin/python`, **Python 3.12.3**,
**PySide6 6.11.2**, **pydantic 2.13.5** — the same three the 2026-09-05 run used. Docker
29.1.3, Compose 2.40.3. `QT_QPA_PLATFORM=offscreen` throughout.

**A trap in the provenance header, named because it will mislead somebody.** `run.log` says
`box : yulon-ubuntu`. That is the VM's *hostname*, which the rebuild kept; the machine is
**`yulon-ubuntu2`**, the replacement built on 2026-09-08 after the original box's host disk
left the bus. The original `yulon-ubuntu` is OffCritical in Hyper-V and was not touched.

**The folder is named for the day before the run, and its timestamps sit on more than one clock
(added 2026-09-09).** `run.log:1` opens `=== 7.10 re-run sweep started 2026-09-09T00:34:39+02:00 ===`
and `run.log:20` closes it at `00:45:19+02:00`: the run started thirty-four minutes after local
midnight on the **9th**, inside a folder named `…-2026-09-08`. In UTC the whole run — and the staging
commit that created the folder (`d8c08cb5`, 2026-09-09 00:13:31 +0200, which is 22:13:31Z on the 8th)
— falls on **2026-09-08**. Whether that is why the name reads `09-08` is recorded nowhere, so this
paragraph states the dates and does not give a reason. **Which clock each timestamp is on:**
everything this lane wrote is VM-local `+0200` and says so — `run.log`'s two banner lines,
`shots/shots.txt` (`2026-09-09T00:44:09+0200  server-tab-before-refresh.png …`), and the four state
probes, which print both (`state-before.txt:2` — `taken (local): 2026-09-09T00:34:40+02:00   (UTC:
2026-09-08T22:34:40+00:00)`; likewise `state-after-restore.txt:2` `00:44:06+02:00`,
`state-after.txt:2` `00:45:18+02:00`, `final-state.txt:2` `00:51:35+02:00`). The bare clock times in
this README's own prose are that same VM-local `+0200` — among them `00:03:26` (the 8.6 lane's
realmlist write, in *"One thing `network_apply('lan')` did NOT prove here"*), `~00:26` and `00:31:32`
(that lane's restart and its handover, in the **Target** bullets below), `00:34:40` (`state-before.txt`)
and `00:44:06` (the ready wait killed). The one `Z` reading in this README is a container's:
*"world and auth started `22:35:46Z`"* is Docker's own UTC timestamp, `final-state.txt:55-56`
(`started=2026-09-08T22:35:46.516853149Z`) — the same instant the VM shell was calling `00:35:46` on
the 9th. The VM's shell runs `Europe/Oslo (+0200)` and the `ac-worldserver` container — the one
container asked — answered UTC, measured on the box on 2026-09-09 and recorded in
`pyplan/gates/8.6-panel-live-yulon-ubuntu2-2026-09-09/README.md:74-94`, the bullet beginning
*"`part-2-party.md:3` is wrong about the VM's clock"*. The checkpoint time `00:34:30` given later in
this section was read on the Hyper-V **host**, a third clock this paragraph did not measure. **The folder is not renamed**: `pyplan/checklist.md` cites it by name.

**Target.** The WotLK install at `/home/pk/wowserver`, `install_id 243c46e3`, all six stages
completed — but **not the same install the 2026-09-05 run measured**, and the differences
matter:

* it was **installed on this box tonight** through the engine, then **rebuilt by the 8.7a
  lane** with mod-ale compiled in;
* its command channel is enabled (8.2a) and its Server tab carries that group;
* the 8.6 lane held the box until `00:31:32` and **restarted the worldserver at ~00:26** as
  the last thing it did, so this run's world was 8 minutes old at `state-before.txt`;
* `acore_world` has **316** tables, not the 315 every earlier install gate reported. The other
  three counts are the familiar 22 / 111 / 30. The extra table arrived with the mod-ale
  rebuild; it is recorded here as an observation, not diagnosed.

**A Hyper-V checkpoint was taken first**, from the host, before anything was pressed:
`7.10-before-regression-rerun-2026-09-09` (`Checkpoint-VM -Name yulon-ubuntu2`, 2026-09-09
00:34:30). It is still there.

---

## Result: 53 checks, 52 OK, 1 FAIL

| Driver | what it drives | checks | exit | elapsed | log |
|---|---|---|---|---|---|
| `sweep_driver.py` | base `Controller` status / import / ports | 3 OK | 0 | 1 s | `sweep1.log` |
| `sweep_driver2.py` | modules, plans, repair refusal, update, console, log stream | 7 OK | 0 | 6 s | `sweep2.log` |
| `sweep_driver3.py` | backup → verify → restore → start | 7 OK | 0 | 57 s | `sweep3.log` |
| `sweep_driver4.py` | `network_apply('lan')`, realmlist file, interrupted-restore | 3 OK | 0 | 2 s | `sweep4.log` |
| `widget_driver.py` | the real buttons on `ControllerView` + `CatalogView` | **32 OK, 1 FAIL** | **134** | 70 s | `widget-run.log` |

`run.log` carries the exit codes and elapsed times as the runner wrote them.

**Not one of the 53 is vacuous, and two of them used to be.** The 2026-09-05 folder's own
README names two checks that passed on answers meaning "the question was not asked":
`send_console('server info')` returning `lines=()`, and `import_state()` returning
`state='unreadable'` under a permission error. Both now fail on those answers
(`drivers.diff`), and both passed here on real content — a 9-line reply with `prompted=True`,
and `populated`, *"103 rows in acore_auth.account, 1001 rows in acore_characters.characters"*.

---

## The ground, read before anything was pressed

`state-before.txt`, `00:34:40`:

* all three `ac-*` containers up; 3724, 8085 and 127.0.0.1:3306 listening;
* `acore_auth` **103** accounts (`101 YULON_243C46E3`, `102 PERZI`, `103 YULONADMIN`),
  `account_access` 3 rows — so `WIDGET0908B` did not exist before the click that made it, and
  the driver asserts that separately (`the account does not exist before the click -- count=0`);
* `acore_characters.characters` **1001**;
* realm row `1 Yulon ubuntu2 172.30.48.189 172.30.48.189 8085`;
* `ufw` **inactive**, with the sha256 of both rule files;
* `/home/pk/wowserver/sql_scripts/backups` **absent** — so the five dumps in `state-after.txt`
  were written by this run;
* all three throwaway folders **absent**.

`state-after-restore.txt` and `state-after.txt` are the same probe twice more, and
`diff state-before.txt state-after.txt` is four things and nothing else: the account the
widget created, the dumps, the throwaway folders, and a ufw ownership warning (below). Schema
counts, realm row, install record, ufw rule hashes and published ports are **identical**.

### The firewall plan was read before any driver could apply it

`network-plan-probe.txt`, run twice over (once as the probe, once inside `sweep4.log`). `ufw`
on this box is **inactive**, and the 7.1 lockout came from a plan that carried
`('ufw', '--force', 'enable')` with no warning. The plan here is:

```
firewall_commands:
  [0] ('ufw', 'allow', '3724/tcp')
  [1] ('ufw', 'allow', '8085/tcp')
enable at index  : None
VERDICT: no enable in the plan -- nothing to lock this box out
```

plus the §39 warning in the user's own words. `sweep_driver4.py` then applied it for real:
`done=('ufw allow 3724/tcp', 'ufw allow 8085/tcp', 'realmlist → 172.30.48.189')`,
`skipped=(the §39 warning,)`, and `ufw status` afterwards is still **inactive**.

---

## What the re-run FOUND

### 1. The LogPanel's Stop button cannot stop a follow that is between lines — and the process then aborts

`widget-run.log`'s one FAIL, and `log-panel-stop-probe.txt`, which is the same question with a
stopwatch on it.

The driver clicks the panel's real **Stop** with `QTest.mouseClick`. `panel.cancelled` goes
**True** — so `stop()` really ran — and `panel.running` is **still True 60 s later**. The
process then ends with `QThread: Destroyed while thread '' is still running` and **exit 134**
(SIGABRT). `shots/log-panel-after-stop.png` is that state photographed: header still reading
*worldserver log*, Stop still enabled, elapsed clock at **0:00:59**.

**The probe turns it from a symptom into a mechanism, by asking the same panel the same
question with two sources, seconds apart:**

| source | result |
|---|---|
| **B** — a synthetic generator, one line every 50 ms | `RUNNING WENT FALSE` **0.02 s** after the click; `panel.wait(10000) -> True`; status `'cancelled'` |
| **A** — the real `docker logs -f ac-worldserver` | **never stopped in 120 s**; `panel.wait(10000) -> False`; `worker._stop=True` the whole time; `lines=210` frozen from the 1-second mark onward |

`worker._stop=True` says `request_stop()` reached the worker. `lines` frozen says no line
arrived. And `_StreamWorker.run()` (`pylauncher/yulon/ui/widgets/log_panel.py:81-84`) can only
notice the flag *between* lines:

```python
for text in self._source():
    if self._stop:
        message = "stopped"
        break
```

So a follow that is blocked waiting for the server's next line stays blocked, and the thread
outlives the click by however long the worldserver stays quiet. `LogPanel.run()` has the
parameter that fixes this and says so in its own docstring — *"`cancel`, when given, is set by
`stop()` so a source that supports it can be interrupted even while blocked between lines
(review finding, 2026-08-21)"* — and **the one call site that follows a log passes no cancel
event**: `pylauncher/yulon/ui/controller_view.py:2857`,
`self.console_log.run(self.services.logs_source, title="worldserver log")`.

**This is NOT a Phase 8 regression, and that is measured rather than assumed.**
`git diff cfb4c04f..96a129dc -- pylauncher/yulon/ui/widgets/log_panel.py` is **empty**: no
commit touched that file between the 2026-09-05 run's tree and this one. `follow_logs()` is
byte-identical across the two (same md5 of the function body). The call site is the same call,
without a `cancel=`, at both commits. **The 2026-09-05 run passed this check because its
worldserver printed within the 60-second window; tonight's did not.** A check whose verdict is
set by how chatty the server happens to be is the shape this project has a lesson about, and
it took eleven weeks and a quiet log to answer differently.

`docker logs -f --tail 200` also explains the frozen figure: 200 replayed lines plus 10 new is
the `lines=210` that never moved.

**What it would cost a user:** press Stop on the Console tab's log follow while the world is
quiet and the button appears dead — the header keeps counting — until the server prints again.
Close the app in that window and it aborts on the way out, which is the exit 134 this run
recorded.

### 2. `gate-79-controller-surface.py`'s ready wait can never answer for an AzerothCore install

`ready-after-restore.txt` and `ready-marker-probe.txt`.

This run replaced the 2026-09-05 runner's blind `sleep 180` with the install's own
`wait_server_ready()`, spelled the way `pyplan/gates/gate-79-controller-surface.py:152` spells
it for this game: `wait_server_ready("127.0.0.1", auth_port)`. **It never returned.** It was
killed at `00:44:06` after **8 m 19 s**, and the world had been up and loaded the whole time —
`ready...` appears 4 times in the worldserver log and both 3724 and 8085 were listening.

The cause is in the marker, and the probe measures both halves on the same server at the same
moment:

* `docker.azerothcore_ready()` (`pylauncher/yulon/docker.py:2736-2741`) builds the AUTH marker
  as `f"{realm_host}:{realm_port}"`;
* the authserver prints that pair in exactly one line: `Added realm "<name>" at <address>:<port>.`,
  and both halves come from `acore_auth.realmlist` — the **address** column and the **port**
  column, which is the **world** port;
* this box's row is `172.30.48.189 … 8085`. Occurrences of `127.0.0.1:3724` in the auth log:
  **0**. Occurrences of `172.30.48.189:8085`: **1**.

**A against B, one function, two argument pairs:**

| call | result |
|---|---|
| `wait_server_ready("127.0.0.1", 3724)` — the harness's spelling | still waiting after **8 m 19 s**, killed |
| `wait_server_ready("172.30.48.189", 8085)` — the realm row's own address and port | **`ready=True` after 0.3 s** |

So `_READY_CALLS["wow-wotlk"]` in 7.9's harness would burn the full
`MANAGEMENT_FLOOR_SECONDS` on every one of its three ready waits and then report the server as
never ready. It has never been noticed because **7.9's harness has only ever been pointed at
the three CMaNGOS games**, whose `wait_server_ready()` builds a different marker and takes no
realm arguments at all — the entry for `wow-wotlk` is there because the table refuses ids it
has no spelling for, not because anything ran it. The 7.9 re-run beside this one drove TBC,
Vanilla and Tortoise and reached ready in 31–77 s on all three, which is the same table's
other three rows working.

**The fix is to read the pair rather than to type it**: the realm row's `address` and `port`
are one query away, and `ready_marker_probe.py` in this folder is that query plus the call. It
is deliberately not applied to the shared harness here, for the reason 7.9's own line gives
about changing an instrument whose counts are already published.

### 3. Two defects in this lane's own scripts, kept in the record rather than quietly fixed

* **The ufw rules came back owned by the wrong user.** The runner copies
  `/etc/ufw/user.rules` aside, `chown pk:pk`s the copy so the driver can read it, and restores
  with `cp -a` — which preserves the *copy's* ownership. The sha256 matched either side, so
  every hash-based check said "restored", and `ufw` itself was the only thing that noticed:
  `WARN: uid is 0 but '/etc/ufw/user.rules' is owned by 1000` (visible in the
  `state-after.txt` / `state-before.txt` diff). Put back by hand to `root:root 640`, with the
  before/after `ls -l` and the unchanged sha256 appended to `sweep4.log`. **The one-line fix
  is `sudo chown root:root` after the `cp -a`**; the runner is committed as it ran, so the
  defect is visible rather than papered over.
* **The cleanup's account delete did not delete.** `cleanup-710.sh`'s multi-table
  `DELETE aa FROM acore_auth.account_access aa JOIN …` failed with
  `ERROR 1046 (3D000): No database selected`, and the script's own next line — *"and AFTER:
  this must print nothing at all"* — printed `104 WIDGET0908B`. That check is why this is a
  paragraph and not a leftover account. Re-run with the database named on the command line
  (`mysql … acore_auth -e "DELETE FROM account_access WHERE id IN (SELECT id FROM account
  WHERE username='WIDGET0908B'); DELETE FROM account …"`), after which the same read prints
  nothing and the counts are back to **103 accounts, 3 `account_access` rows**.

---

## What each shared layer answered

* **base `Controller`** — `status()` `db=True auth=True world=True`; `import_state()`
  `populated`, *"103 rows in acore_auth.account, 1001 rows in acore_characters.characters"*
  (and it now FAILS on `unreadable`); `port_conflicts()` `[]`; `repair_import()` **refused** on
  a populated DB naming the two running containers; `start()` brought auth+world back after
  `sweep_driver3.py` stopped them.
* **`docker.py`** — `follow_logs()` yielded 5 live worldserver lines in `sweep2.log` and 210 in
  the panel; the console round-trip came back through the widget with the core revision
  (`AzerothCore rev. 413bea61a85e+ 2026-09-04`), `Connected players: 0. Characters in world:
  500.`, uptime and `Update time diff: 3ms … 95/99/max 119/125/142ms`.
* **`runner.py`** — every subprocess above, plus the log stream. **The 2026-09-05 abort has not
  come back**: `sweep_driver2.py` takes five lines off `logs_source()` and `break`s without
  closing the generator, and exits **0**. The exit 134 in this run is `widget_driver.py` and it
  is finding 1 — a QThread still running at teardown, not the `_enter_buffered_busy` shape
  `atexit.register(_close_abandoned_streams)` fixed.
* **`platform.py`** — `check_for_update()` `current=0.6.59 latest=v0.6.59Public
  available=False error=None`.
* **`networking.py`** — `network_plan('lan')` and `('internet')` both `ready=True`;
  `network_apply('lan')` 3 done, 1 skipped; `write_client_realmlist()` round-tripped
  `192.168.1.50` into `Data/enUS/realmlist.wtf`.
* **maintenance / backup-restore** — `backup()` of the live server produced 4 dumps
  (`acore_auth` 81,410 B, `acore_characters` 10,093,102 B, `acore_playerbots` 62,406,531 B,
  `acore_world` 306,399,612 B) while it was running; `verify_dump()` passed on all four;
  `plan_restore()` refused while auth+world were up and allowed it once they were down;
  `restore()` completed with a pre-restore safety backup; `interrupted_restore()` `None`.

### One thing `network_apply('lan')` did NOT prove here, said plainly

Its realmlist half wrote `UPDATE … SET address='172.30.48.189'` over a row that **already read
`172.30.48.189`** — the 8.6 lane set it there at `00:03:26`, half an hour before this run
started, and `run.log` records the address it read before the sweep. `sweep4.log` carries both
readings and the conclusion: *"before: 172.30.48.189 / after: 172.30.48.189 / (unchanged —
nothing to put back)"*. **A step whose result is its start state proves nothing**, so the
`realmlist → 172.30.48.189` entry in `NetworkReport.done` is reported as what the app said it
did and not as evidence that the row was written. The firewall half of the same call IS
evidenced — the two `allow` rules appear in the `user.rules` diff in `sweep4.log`, and were
absent before it.

---

## The widget half, and the frames

`widget_driver.py` builds the real `ControllerView` and the real `CatalogView` over a real
`LogPanel` with the real `installer_for_app` factory, on a real `QApplication`, and presses
their real `QPushButton`s with `QTest.mouseClick`. `status_poll_ms=0`, so only a click can fill
the status label; Apply on the Networking tab is asserted **disabled** before a plan exists and
is never clicked.

Thirteen frames, each with a sidecar line in `shots/shots.txt` naming the containers `docker ps`
reported up at the instant of the grab — all thirteen were taken with all three alive.

| frame | subject |
|---|---|
| `server-tab-before-refresh.png` / `-after-refresh.png` | `status: unknown` with the poll off, then `db up, auth up, world up` after one click. The "before" is asserted empty **one line before it is written**, so it cannot hold the after state — 8.2d shipped one that did |
| `console-tab-before-send.png` / `-after-send.png` | the empty panel, then the live reply |
| `accounts-tab-after-create.png` | the account the click created, GM level driven by two Up keys |
| `networking-tab-before-plan.png` / `-after-plan.png` | Apply dead, then the plan and Apply live (never clicked) |
| `maintenance-tab-after-refresh.png` | the backup list, no error paragraph |
| `catalog-tiles-before-install-click.png` / `-after-refusal.png` | the tile before the press, and still reading `Install` after |
| `modal-01-install-failed.png` | the refusal, as a real modal, clicked away by a real button press |
| `log-panel-while-following.png` / `log-panel-after-stop.png` | finding 1, photographed |

### The install refusal was decided by a reading taken before the click

The 2026-09-05 driver asserted the port floor and cited the space one, because that box had
50.8 GiB. The 2026-09-04 one asserted space, because that box had 39 GiB. **This box is under
both**, and the driver says so before it clicks:

```
free space on /home/pk before the click: 35.2 GB = 32.8 GiB
preflight's floor is 48 GiB, so a space refusal is EXPECTED on this box
the install's ports are held by a running server: True
```

and preflight then refused with both, in one modal:

> free space on Docker's disk and the server folder: 33 GB free, and the install needs 48 GB …
> the server's ports: ac-worldserver, ac-authserver already publish the ports this server needs …

Each of the two assertions is guarded by the reading that says it applies, so the driver
answers correctly on a box under only one of them.

---

## What is NOT re-run here, and why

| 2026-09-05 item | checks | why not |
|---|---|---|
| `widget_cancel_driver_tbc.py` — a real cancelled install, clicked and stopped | 20 | **Unreachable on this box.** preflight refuses every install at 32.8 GiB against a 48 GiB floor, so no install can start here to be cancelled. That refusal is itself asserted above, through the widget |
| `copy_shapes_driver.py` — the two cancel-copy findings on the `wow-wotlk` clone shape | 16 | same folder-shape work, and it needs a clone into free space this box does not have |
| the every-dump restore | — | `sweep_driver3.py` restores `report.dumps[0]` only (`acore_auth`), which is the 2026-08-28 driver's shape and is kept for comparability. **The every-dump path was driven tonight on three servers** by the 7.9 half beside this one: 4, 5 and 4 databases restored by name |
| `keep_awake()` released | — | nothing here takes the inhibitor; no install was started |

53 checks against the 2026-09-05 run's 88 is that difference, and the missing 35 are the two
install-half drivers, which this box cannot host.

---

## Deviations from the original recipe, named

1. **The box.** `yulon-ubuntu2` replaces `yulon-ubuntu`, whose host disk failed on 2026-09-08.
   Its hostname still reads `yulon-ubuntu`; see the trap above.
2. **A fresh clone at the tip**, not the box's shared `~/dads-mmo-lab`, and the imported
   package is printed rather than assumed.
3. **`sleep 180` became `wait_ready.py`.** That substitution is what found finding 2. The step
   it replaced would have "passed" silently.
4. **Two checks were made stricter** — `send_console` and `import_state` — on the two answers
   the 2026-09-05 folder itself names as false passes. Both passed on real content.
5. **The realm row is read out of the database either side of `network_apply`**, and the runner
   is prepared to put it back, because on this box that row belongs to another lane's work. It
   did not have to.
6. **Thirteen screenshots**, where the 2026-09-05 run took none.
7. **A Hyper-V checkpoint** was taken before the first press.
8. **The install-half cancel drivers are absent**, for the disk reason above.

---

## The box, as it was left

`final-state.txt`, plus the two corrections appended to it and to `sweep4.log`:

* all three containers **up**, `RestartCount=0` on each, `exit=0`, world and auth started
  `22:35:46Z` by `sweep_driver3.py`'s `controller.start()`;
* schema counts **22 / 111 / 316 / 30**, unchanged from `state-before.txt`;
* `acore_auth.account` **103** rows, `account_access` **3** — `WIDGET0908B` and its access row
  gone, `101 YULON_243C46E3`, `102 PERZI`, `103 YULONADMIN` untouched;
* realm row `1 Yulon ubuntu2 172.30.48.189 172.30.48.189 8085` — exactly as the 8.6 lane left
  it;
* `ufw` **inactive**, both rule files at the sha256 `state-before.txt` recorded, **and back to
  `root:root 640`**;
* the five dumps this run wrote removed; the now-empty `sql_scripts/backups` directory left,
  because the app is what creates it;
* all three throwaway folders removed;
* `/home/pk/wowserver/.yulon-install.json` byte-for-byte unchanged;
* disk back to **34 GB free**;
* checkpoint `7.10-before-regression-rerun-2026-09-09` **kept** on the host;
* `~/lane710/` kept — checkout, drivers, probes and `out/`, which is what this folder is a copy
  of.

Nothing was installed, reinstalled, rebuilt or removed. No module was applied. No
configuration file was written.

## Files

| File | What it is |
|---|---|
| `run-710-rerun.sh` | the runner, exactly as it executed (including the `cp -a` that caused finding 3) |
| `cleanup-710.sh` | the cleanup, exactly as it executed (including the DELETE that failed, and the check that caught it) |
| `drivers/` | every driver exactly as run |
| `drivers.diff` | their complete diff against the committed 2026-09-05 originals |
| `network_plan_probe.py`, `network-plan-probe.txt` | the LAN plan read before anything could apply it |
| `wait_ready.py`, `ready-after-restore.txt` | the ready wait that replaced the blind sleep, and the record of why it was killed |
| `ready_marker_probe.py`, `ready-marker-probe.txt` | finding 2, A against B on the same server |
| `log_panel_stop_probe.py`, `log-panel-stop-probe.txt` | finding 1, A against B on the same panel |
| `sweep1.log` … `sweep4.log`, `widget-run.log` | the five driver transcripts |
| `state-before.txt`, `state-after-restore.txt`, `state-after.txt`, `final-state.txt` | the same probe, four times |
| `ufw-user.rules.before`, `ufw-user6.rules.before` | the copies taken before the networking driver |
| `shots/` | thirteen frames and `shots.txt`, the liveness record for each |
