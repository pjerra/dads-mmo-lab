# The rebuild, pressed live — WoW WotLK on `yulon-ubuntu2`, 2026-09-09

**The first live press of the rollback feature that landed on 2026-09-08** (`native.rebuild()`,
`_keep_rollback`, `_restore_rollback`; 7808c0d6 + 235d8b08 + c0513d6d), and the same press that
answers questions 1–3 of `pyplan/gates/8.6-wotlk-yulon-ubuntu-2026-09-08/README.md`.

**Box:** `yulon-ubuntu2` (12 vCPU, 16 GB), the fresh AzerothCore WotLK install at `/home/pk/wowserver`
built through the app's own engine tonight (`~/wotlk-install.log`, `INSTALL RETURNED CLEANLY` at
20:18:43Z). **Tree:** this lane's worktree, `e40f4590` for everything up to the first press and
`5a7e8baa` (this lane's fix) for the ones after it. **Three presses, not two** — press 1 on the
unfixed tree, then 2a and 2b on the fixed one; the reason there are two of the latter is written
under *2 — the rollback*, and it was my mistake rather than the app's. **Checkpoint taken before anything was written:**
`rebuild-before-ale-2026-09-09` on the Hyper-V host, 22:21:39 host local. Every action was announced
on that box's activity terminal first. **Left running:** the whole stack, up, on the build press 2
made — see *Left on the box*.

Every capture is the VM's own desktop photographed from the Hyper-V host with `vmshot.ps1`
(`show.sh` puts one reading on that desktop; GNOME refuses an in-guest screenshot driven from ssh),
except `2-ale-installed-rebuild-pending.png`, which is the app's own Modules tab rendered offscreen.
`ac-worldserver`'s liveness is printed under every capture and quoted in each row below, because a
server that has died photographs exactly like a refusal.

---

## Verdict, clause by clause

| Clause | Verdict | Evidence |
| --- | --- | --- |
| 8.6 Q1 — the Lua engine is in the binary after the rebuild, read from the binary | **PASS** | `6-after-new-binary.log`, `4-engine-in-binary-bridge-answers.png` |
| 8.6 Q2 — `mod_ale.conf` arrives with `ALE.Enabled = 1` and the absolute script path | **PASS** | `4-install-ale.log`, `2b-ale-conf-written-engine-still-absent.png` |
| 8.6 Q3 — `dml_bridge_ping` answers `DML-BRIDGE-READY` | **PASS** | `7-ping.log`, `8-ping-after-press2.log` |
| the rollback is kept before the compile, and let go after | **PASS** | `press2-rebuild.log`, `press2-rollback-watch.log`, `3-…png`, `5-…png` |
| the rebuild replaces the running containers with what it built | **PASS** | `press1-rebuild.log`, `press2-rebuild.log` — the image id under `ac-worldserver` changed twice |
| **the rebuild reports the server ready when it is ready** | **FAILED, then fixed and re-pressed** | `press1-rebuild.log` — see *The defect this press found* |
| 8.7a clause 5 — a module's configuration is shown by the running server | **PASS**, for a module that has been through a rebuild | `9-clause5-server-debug.txt`, `6-…png`, and `8.7a-fifth-clause.md` for the two limits on that |

The rollback's **restore** path was not pressed: nothing this lane did produced a build that failed
to come up, and manufacturing one was out of scope tonight. `_keep_rollback` and `_let_go` were, live.

---

## The ground each step started from

Read before anything was installed or compiled (`1-ground.log`, 20:22:54Z; screenshot
`1-ground-no-engine-no-module-no-bridge.png`, `ac-worldserver running pid=26224 restarts=0`):

| Fact | At the start |
| --- | --- |
| `ac-worldserver` | `running pid=26224 restarts=0`, image `703ea6521431` |
| images on the daemon | four `yulon.local/ac-wotlk-*:native-243c46e3`, **no `-rollback` tag at all** |
| `allowed_modules()` | `'mod-playerbots'` |
| `modules/mod-ale` cloned | **False** |
| `mod_ale.conf` | **does not exist** |
| bridge scripts on disk | `[]` |
| `party.read_engine_in_binary()` | **`engine=False`** — and `False`, not `None`, so the control string was found and the reading worked |
| the command channel | **none** — no credential had ever been written for this install |

The ping's own ground needed the channel first, so the channel was set up before anything else was
touched (`2-channel.log`, `3-channel-prove.log`), and only then was the server asked:

```
asked 'dml_bridge_ping': outcome='no'   text: "Command 'dml_bridge_ping' does not exist\r\n"
asked 'server info':     outcome='yes'  text: 'AzerothCore rev. 413bea61a85e+ … Characters in world: 225.'
```

Word for word the answer 8.6 recorded on the box that died, on a different machine and a different
install — with the control beside it, because a refusal from a channel that answers nothing at all
is not evidence of a missing command.

**The channel setup is itself a small finding.** `settle()` caught the world six seconds after Start
and returned `Pending` — correctly. The account exists from then on, and `create_account` never
re-salts a row that exists, so the next process started from `Idle`, generated a new password and got
a 401. That is `Refused`, and `repair()` is the way out `ensure()`'s own comment describes. It walked
exactly that path and ended `Verified` (`3-channel-prove.log`). The comment is not theoretical.

---

## 1 — installing ALE, and what it does not do

`4-install-ale.log`, through `ControllerServices.for_entry(...).applier.install(...)` — the seam the
Modules tab presses. `2-ale-installed-rebuild-pending.png` is that tab, with its **Rebuild the
server…** button, showing the report.

```
install mod-ale: 3 step(s), 0 skipped, 0 left unapplied, rebuild=True
done:  clone https://github.com/azerothcore/mod-ale.git → modules/mod-ale
       activate env/dist/etc/modules/mod_ale.conf from conf/mod_ale.conf.dist
       set 2 key(s) in env/dist/etc/modules/mod_ale.conf
rebuild_required=True  restart_recommended=False
AFTER: mod-ale HEAD = '319f43edd58ffa6ed72873ddd68820ab86e4a99b'   (the manifest's pin, honoured)
AFTER: mod_ale.conf ALE.* = ['ALE.Enabled = 1', … 'ALE.ScriptPath = "/azerothcore/env/dist/etc/modules/lua_scripts"', …]
AFTER: the engine in the RUNNING binary is still: engine=False
```

That last line is 8.6's finding restated on a second machine: **the module on disk changes nothing
about the server that is running.** `ALE.Enabled = 1` is the corrected key — 8.6 measured that our
manifest had named `ALE.EnableLuaEngine`, which the module does not have, and that its compiled
default is `false`.

The five bridge scripts were deployed before the rebuild rather than after (`5-deploy.log`,
`changed=True`, five names), so the world the rebuild starts reads them at boot and the ping after it
is an arrival proof rather than a proof of `.reload`.

---

## 2 — the rollback, read by a second process

`3-rollback-tags-exist-during-the-compile.png`, and `press1-rollback-watch.log` /
`press2-rollback-watch.log` behind it. `watch_rollback.sh` polls `docker images` from **outside** the
press, so the claim in the press's own log and the state of the daemon are two separate readings:

```
[20:28:18Z] Kept the build you have now as a rollback (4 images tagged -rollback).
            If the new build does not come up it is put back automatically.
--- 2026-09-08T20:33:22Z ---     (the daemon, five minutes into the compile)
  ROLLBACK …ac-wotlk-authserver:native-243c46e3-rollback   1825d670a587
  ROLLBACK …ac-wotlk-client-data:native-243c46e3-rollback  92252d77483e
  ROLLBACK …ac-wotlk-db-import:native-243c46e3-rollback    dcaadcf6692d
  ROLLBACK …ac-wotlk-worldserver:native-243c46e3-rollback  703ea6521431
  count: 4     world: running pid=33073 restarts=0
```

`703ea6521431` is the image id the ground reading recorded under the running container. The rollback
names the build that was running, and the world is untouched while the compile runs.

The **other end** of that is the presses on the fixed tree. `press2-rollback-watch.log`, from outside:

```
21:13:20Z  count: 2      ← the two stale tags press 1's kill left; see below
21:13:35Z  count: 4      ROLLBACK …worldserver-rollback  8d99cf823bfc   ← press 1's build, the one running
   …       count: 4      (five samples, ~60 s)
21:14:57Z  count: 0      and the images now name worldserver 008648f92bf5 — a build that did not exist before
```

and `press2-rebuild.log`, from inside the next one (`5-press2-rollback-kept-then-let-go.png`):

```
ground: -rollback tags = []                                   ← before
[21:15:02Z] Kept the build you have now as a rollback (4 images tagged -rollback)
[21:16:05Z] The server is up.
  removed image …worldserver-rollback / …authserver-rollback / …db-import-rollback / …client-data-rollback
after: -rollback tags (must be none) = []                     ← after
```

Absent → present during → gone after, all three read rather than assumed, twice.

**Those are two different presses, and that was an accident of mine.** Two `ssh` launches I believed
had failed both eventually ran, so the fixed tree was pressed twice in a row: **press 2a**
(21:13:36Z → ~21:14:50Z, worldserver `008648f92bf5`) and **press 2b** (21:15:02Z → 21:16:05Z,
`cad2566406d2`). Both returned cleanly. 2b's start truncated `rebuild.log`, so **press 2a's own log is
gone** — what survives of it is the watcher's reading above, which is the half taken from outside
anyway. The two are named apart here rather than merged into one tidy press, because the rollback arc
in the watcher and the rollback arc in the log are not the same run and a reader would otherwise
match a timestamp to the wrong one. The fix is confirmed twice over rather than once, which is the
only good thing about it.

**A refusal `_let_go` can meet, measured by accident.** When this lane removed the stale tags press 1
left behind, two of the four `docker rmi` calls were refused:

```
Error response from daemon: conflict: unable to delete …db-import…-rollback (must be forced)
  - container edb70eb98b32 is using its referenced image dcaadcf6692d
```

The one-shot services (`ac-db-import`, `ac-client-data-init`) leave **exited** containers that still
reference the old image, so a `-rollback` tag on those two can survive `_let_go`. `_let_go`'s own
docstring says a refusal is logged and changes nothing, which is the right behaviour — but "the
rollback names are gone afterwards" is true of the two long-running services and **not guaranteed of
the one-shot ones**. It happened not to bite press 2 (the exited containers had been replaced by
then), which is exactly why it is written down here rather than left to be discovered.

---

## 3 — the answers to 8.6's questions

`6-after-new-binary.log`, `7-ping.log`, screenshot `4-engine-in-binary-bridge-answers.png`
(`ac-worldserver running pid=50099 restarts=0`, image `8d99cf823bfc` — a different id from the
ground's `703ea6521431`):

```
the Lua engine, read out of the NEW binary:
  engine=True  the Lua engine is built into this server's executable.

--- 'dml_bridge_ping' ---
outcome='yes'   text: 'DML-BRIDGE-READY dml_bridge_ping\r\n'
```

and the world server's own log, which nobody asked to print this:

```
[dml_addclass]    loaded -- addclass relay ready
[dml_bridge_ping] loaded -- DML-BRIDGE-READY on request
[dml_login]       loaded -- bring-online relay ready
[dml_uninvite]    loaded -- group-remove relay ready
[dml_whisper]     loaded
```

**My Party's server-side route works on this tree.** The 2026-08-20 attempt and 8.6's repeat of it
both failed for the reason 8.6 named — the engine was not in the image — and a rebuild is what
changed the answer. Repeated after press 2 on a second freshly built binary (`8-ping-after-press2.log`).

`.reload ale` answers `outcome='yes'` with empty text, and the ping still answers after it, so the
reload is not what makes the bridge work.

**One thing arrived that this lane did not deploy:** after press 2 the script directory also holds
`LootPet.lua`. That is `mod-ale`'s own shipped example, carried into the image by the module, not
something `party.deploy()` wrote — `5-deploy.log`'s five names are still the five.

---

## The defect this press found

**The rebuild's ready wait asserted an address the install's own last act had already replaced.**

`ready.auth` is `{{REALM_HOST}}:{{WORLD_PORT}}`, and `_ready_spec()` filled the token from
`INSTALL_REALM_HOST` — `127.0.0.1`, the row a **fresh** install advertises.
`StagedInstaller._advertise_realm()` is the install's LAST act and rewrites that row to the machine's
reachable address. So on **every install the Rebuild button can be pressed on**, the marker and the
log can never agree. Measured, not deduced (`press1-rebuild.log`, and the probe quoted below):

```
ready spec world = 'ready\\.\\.\\.'          WORLD MARKER MATCHES: True
ready spec auth  = '127\\.0\\.0\\.1:8085'    AUTH MARKER MATCHES THIS RUN: False
auth log line: 'Added realm "Yulon ubuntu2" at 100.99.204.5:8085.'
realmlist row: 1  Yulon ubuntu2  100.99.204.5  8085
_auth_ready() = False
```

What that looked like from outside, and it is the whole reason this is a defect and not a nuisance
(`4-engine-in-binary-bridge-answers.png`): the compile finished, the containers were replaced, the
new worldserver came up with `mod-ale` compiled in, loaded the five bridge scripts and answered
`DML-BRIDGE-READY` over its own channel — **while the press sat in "Waiting for the world server".**
`wait_for_ready()` grants another window every time the server prints, and this one prints bot
statistics every thirty seconds for ever, so it would have run to `READY_CEILING_SECONDS` (six hours)
and then `_restore_rollback` would have put the OLD build back. That is the report this button exists
to answer — *"i used the this thing to rebuild the server but nothing changes when i log back in"* —
with six hours added to it.

**The fix** (`5a7e8baa`): the address half of the marker is a wildcard and the port half stays exact.
Readiness needs the auth server to have advertised THIS install's realm on THIS install's world port;
**which** address it advertises is `_advertise_realm()`'s question and is asked there, against the row.

Two other repairs were written and thrown away, and why matters:

* *fill the token from the realmlist row* — correct, and it would have made the rebuild read the
  database. `REBUILD_OPENING_NOTE` promises "does not touch your database", the confirmation promises
  "your characters, accounts and databases are not touched", and `test_rebuild.py` asserts *nothing
  anywhere in the run went near a database*. Three things would have had to be weakened to fit one
  change, and a guard relaxed to admit the change it was meant to catch is not a guard.
* *drop the auth marker for a rebuild* — one line, and it stops the rebuild noticing an auth server
  that never came back. A press that says "rebuilt and running" over a server nobody can log into is
  this feature's own failure mode.

Driven by test first. RED, with the message recorded:

```
tests/test_rebuild.py::test_a_rebuild_waits_for_a_realm_line_whatever_address_it_advertises
E  AssertionError: ('127\\.0\\.0\\.1:8085', 'Added realm "Whatever" at 192.168.1.25:8085.')
```

Three tests hold it: the rebuild's marker matches a realm line at any address; it still **refuses**
one on another port (the control — a pattern that matched everything would pass the first for the
wrong reason); and the install and the rebuild use the same marker, so the install is not left on a
rule of its own that goes stale the moment `_advertise_realm()` runs. `test_spine.py`'s A3/A5 test was
rewritten rather than deleted: the world marker is still a literal, dots and all.

**The seam that hid it.** `support_native.Recorder` handed `wait_ready` the `ReadySpec` and threw it
away — `lambda spec, ready: self.ready`. A double that answers ready without ever looking at the
pattern cannot see a pattern that can never match a real log. It now keeps every spec
(`Recorder.ready_specs`), which is what the three tests above assert against.

## What the rebuild cost, on this box

Measured, and different from the 35–72 minutes the brief expected:

| | compile | recreate → ready | total |
| --- | --- | --- | --- |
| press 1 (ALE added: CMake reconfigure, 1899 objects) | 20:28:18Z → 20:38:42Z, **~10 min** | 20:38:42Z → the world was up by 20:42 | (never returned — the defect above) |
| press 2a (nothing changed: every layer cached) | ~21:13:40Z → ~21:14:10Z | recreated 21:14:13Z, done by 21:14:50Z | **~75 s** (watcher; its own log was truncated) |
| press 2b (same again, immediately after) | 21:15:02Z → 21:15:12Z | 21:15:35Z → ready 21:16:05Z, **30 s** | **63.4 s** |

The install's own `build` stage on the same tree ran **19:42:13Z → 20:07:14Z, 25 minutes** (the
compile itself 19:44:27Z → ~20:06Z, 1834 objects), and its `ready` stage 20:15:32Z → 20:18:43Z,
**~3 minutes**. So on this class of box a rebuild that adds one module is under half a fresh build,
not a whole one, and a rebuild that changes nothing is a minute. Warm ccache and a warm page cache
are doing that work and neither is a promise — but "35–72 minutes" is not what this machine did, and
per-box facts are measured per box.

## Where the plan was silent

`phase8-decisions.md`, `phase8-parity-decisions.md` and the 2026-09-08 owner answers all say what a
rebuild must do when the new build breaks (owner answer 2: keep a rollback, restore it automatically).
**None of them says what "the server came back up" means for a rebuild**, and that silence is exactly
where this defect lived: the install's definition was inherited whole, including the half of it that
is only true before the install's last act. Named here rather than decided quietly — the fix above
answers it one way (the realm line, on this install's world port), and that is a choice made in the
silence.

## Things left unexplained, rather than explained away

1. **`ac-worldserver` was started again at 21:11:45Z**, before press 2a and by nothing this lane
   knowingly ran. Same container id (`a00c43a0…`, created 20:38:49Z), `RestartCount` still 0, and a
   clean boot from `Starting worldserver...` on the same image `8d99cf82`. `docker events` for that
   window is empty, `systemctl show docker` says `NRestarts=0` with the daemon up since boot, there
   is no cron and no watchtower-style container, and `dmesg` shows no OOM kill — so it was neither a
   crash-restart nor a daemon restart. It sits in the same minutes as the duplicate launches above
   and is very probably mine, but "very probably" is not a measurement and I am not going to write it
   down as one. It changed no image and no conclusion here; every reading that matters was taken
   after it. **`ac-authserver` did the same at 21:24:28Z**, after this lane had finished pressing
   anything — same container, `RestartCount` 0, exit code 0, and it came back advertising the realm.
   The box is being left with that unexplained, which whoever takes it next should know.
2. **Press 1's `ready` stage was killed, not cancelled.** `rebuild()` was handed no cancel event, so
   the process was killed with `pkill`. What that left, read immediately after: the live tags naming
   the NEW build, four `-rollback` tags naming the old one, no `-failed` tags, and the containers up
   on the new build. Recorded because a killed press is not one of the outcomes `rebuild()`'s failure
   path is written for, and the state it leaves is the state a user would find.

## Left on the box

* The stack **UP**, on the build press 2 made (`ac-worldserver` image `cad2566406d2`, pid 98208,
  `restarts=0`), with `mod-ale` compiled in, the five bridge scripts loaded and the channel verified.
  Left up for lane 8.6.
* `modules/mod-ale` at `319f43ed`, `env/dist/etc/modules/mod_ale.conf`, and the bridge scripts at
  `env/dist/etc/modules/lua_scripts/`.
* The command channel: account `YULON_243C46E3`, credential at
  `~/.local/share/yulon/credentials/wow-wotlk-243c46e3.json`, SOAP on `127.0.0.1:7878`.
* No `-rollback` and no `-failed` tags.
* The checkpoint `rebuild-before-ale-2026-09-09` on the Hyper-V host. It predates everything above,
  so restoring it undoes the whole night's work on that box, including the install of ALE. The VM's
  `CheckpointType` was set to `Production` for the attempt and put back to `Standard`; Hyper-V fell
  back to a Standard checkpoint anyway (7.85 GB of `.VMRS`). `U:` had **40.5 GB** free afterwards.
* `~/lane-rebuild/` — this lane's tree at `5a7e8baa`, exported with `git archive`, which press 2 ran
  from. `~/dads-mmo-lab` was left untouched at `e40f4590`.
* Gate scripts as `~/rebuild_gate.py`, `~/watch_rollback.sh`, `~/show.sh`, output in
  `~/rebuild-gate-out/`.
