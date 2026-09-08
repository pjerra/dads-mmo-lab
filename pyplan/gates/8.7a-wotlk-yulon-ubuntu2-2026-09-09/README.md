# 8.7a's fifth clause, pressed on `yulon-ubuntu2` — WoW WotLK, 2026-09-09

> *a configuration change the module needs is shown by the running server, not only read back
> from the file*

**Box:** `yulon-ubuntu2` (16 GB, 12 vCPU), the AzerothCore WotLK install at `/home/pk/wowserver`
built through the app's own engine on 2026-09-08 and **rebuilt with `mod-ale` compiled in** by the
rebuild lane the same night (`pyplan/gates/rebuild-live-yulon-ubuntu2-2026-09-09/`).
**Time:** 22:56–23:02 UTC (00:56–01:02 VM local). **Tree:** `96a129dc`, copied onto the box as
`~/lane87a/pylauncher` with this lane's `apply.py` fix on top of it (which changes nothing for
`mod-ale` — all of its keys have values). **The manifest fix in §4 was written after this press and
is not in it**: `restart_recommended=False` was measured against the manifest as shipped, which is
the only way that reading means anything.
**Python:** `~/dads-mmo-lab/pylauncher/.venv/bin/python` (3.12.3, PySide6 6.11.2, offscreen).
**Runner:** `gate87a5.py`, committed beside this file. **Left running:** the whole stack, up, on the
same build and with `mod_ale.conf` byte-identical to the file this press found.

Every capture is the app's **Modules tab**, rendered through `ControllerView` built by
`ControllerServices.for_entry()` against the live install, and each one logs whether
`ac-worldserver` was alive at that instant — a server that has died photographs exactly like a
refusal.

**No checkpoint was taken for this press, and that was a measurement, not an oversight.** See
*The checkpoint that was not taken*.

---

## Verdict

| Clause | Verdict | Evidence |
| --- | --- | --- |
| a configuration change the module needs is shown by the running server, not only read back from the file | **PASS**, for a module that has been through a rebuild, with the change made by the app's own seam | `4-the-running-server-shows-the-value-the-app-wrote.png`, `3-press.log` |
| …and the reading is not trivially true: with the value wrong, the running server says so | **PASS** (the control) | `2-wrong-value-the-running-server-follows-it.png`, `2-control.log` |

8.7a's other four clauses were proved on `yulon-ubuntu` on 2026-09-08 and are **not** re-pressed
here. This folder is only the fifth.

---

## Why this is not a re-photograph of what was already on the record

Two readings of this clause were already taken on this box, both by other lanes on 2026-09-08–09:

* the **rebuild lane** asked the restarted world through the app's own channel and got
  `|- mod-ale` on the enabled-modules list and `> mod_ale.conf` under *Using modules
  configuration* (`rebuild-live-…/8.7a-fifth-clause.md`);
* an **8.6 part-2 lane** switched `ALE.Enabled` off by hand, restarted, read the bridge as absent,
  and put it back (its own words are in `~/claude-activity.log`, 00:23:56 → 00:31:32 VM local).

Both were taken with the file's values **already** the ones the app wants. Nothing in either can
tell "the running server follows this file" apart from "the running server was going to do that
anyway", and a step whose assertion is already true before its action runs proves nothing. So the
ground here was read first and then deliberately made **false**:

```
[22:58:34Z] mod_ale.conf on disk: True
  ALE.Enabled = 1
  ALE.ScriptPath = "/azerothcore/env/dist/etc/modules/lua_scripts"
  Logger.ALE=4,Console ALELog            ← another lane's edit, 00:25 VM local, left alone
WHICH ASSERTIONS ARE ALREADY TRUE HERE (so that pressing them would prove nothing):
  * ALE.Enabled is already 1 and the engine is already running
  * the five bridge scripts are already loaded and dml_bridge_ping already answers
  * mod_ale.conf is already the file the running server names
```

and the same reading, from the running server itself, this boot only:

```
Loading Modules Configuration...
> Config::LoadFile: Failed open file '/azerothcore/env/dist/etc/modules/playerbots.conf'
Using modules configuration:
> mod_ale.conf
[dml_addclass] loaded -- addclass relay ready        … five in all
dml_bridge_ping: outcome=yes  ['DML-BRIDGE-READY dml_bridge_ping']
.server debug: 'List of enabled modules:', '|- mod-ale', '|- mod-playerbots'
```

**The log is bounded by the container's own `StartedAt`**, not by `--since 20m`. That is the
difference between reading this boot and reading the boot before it: a `Using modules
configuration` line from twenty minutes ago is exactly how a stale line reads as a fresh fact.

## 1 — the control: the file is pointed somewhere else, and the running server follows

`2-wrong-value-the-running-server-follows-it.png`, `2-control.log`. `ALE.ScriptPath` was pointed —
**by hand, and it is labelled as mine everywhere** — at a decoy directory holding one marker script
and none of the five bridge scripts, and the world was restarted through the app's own Stop and
Start (21.7 s stop, ready 40 s after the start):

```
AFTER (mine, by hand): ALE.ScriptPath = "/azerothcore/env/dist/etc/modules/lua_gate_decoy"

the RUNNING server, with the WRONG value in force:
  Using modules configuration:
  > mod_ale.conf
  [gate87a5] GATE87A5-MARKER-FROM-THE-DECOY-PATH
the marker from the decoy path is in this boot's log: True
any of the five bridge scripts loaded this boot: []
dml_bridge_ping now: outcome=no  ["Command 'dml_bridge_ping' does not exist"]
CONTROL: server info outcome=yes  'AzerothCore rev. 413bea61a85e+ …'
```

Three things at once: the server loaded a script from the path the file named, it loaded **none**
of the five it had been loading a minute earlier, and a command that existed before **stopped
existing**. The `server info` control is there because a channel that has gone silent answers a
missing command and a working command identically — with it, `no` means the server refused, not
that nobody was listening.

## 2 — the press: the app's own `configure()`, and the file read back

`3-the-app-wrote-it-back-file-only.png`, `3-press.log`. The seam is the Modules surface's own —
`view.services.applier.configure(manifest)`, the manifest being the shipped `mod-ale.json`:

```
ground for this step (the wrong value is in force): ALE.ScriptPath = "…/lua_gate_decoy"
ground: dml_bridge_ping outcome=no ["Command 'dml_bridge_ping' does not exist"]

configure mod-ale: 2 step(s), 0 skipped, 0 left unapplied, rebuild=False
--- ApplyReport.done ---
  patch env/dist/etc/modules/mod_ale.conf (changed)
  set 2 key(s) in env/dist/etc/modules/mod_ale.conf
rebuild_required=False restart_recommended=False          ← the defect in §4
the file, READ BACK (which the clause says is NOT enough):
  ALE.Enabled = 1
  ALE.ScriptPath = "/azerothcore/env/dist/etc/modules/lua_scripts"
```

The clause's own wording is why that capture exists and why it is not the answer: at this moment
the file is right and **the running server is still wrong** — the same `dml_bridge_ping` still
answered *does not exist*. "Read back from the file" is a picture of work not yet in force.

## 3 — the proof: the same question, asked of the running server

`4-the-running-server-shows-the-value-the-app-wrote.png`. Stopped and started through the app
again — the same instrument as the control, so the only thing that differed between the two
readings is the file:

```
bridge scripts this boot loaded from the path the app wrote: 5
the decoy marker in THIS boot's log (must be False): False
dml_bridge_ping: outcome=yes ['DML-BRIDGE-READY dml_bridge_ping']
.server debug: 'List of enabled modules:', '|- mod-ale', '|- mod-playerbots'
CLAUSE 5 on this box, for a module that has been through a rebuild: PASS
```

**Which key this press varied, exactly.** `ALE.ScriptPath` — the app writes both of its keys on
every `configure`, and both were in the report, but only the path was made wrong first, so the path
is the one whose effect this folder demonstrates. The other key has its own reason to be believed
and it is not mine: the module's **compiled** default for `ALE.Enabled` is `false`
(`src/LuaEngine/ALEConfig.cpp:20`, recorded in the manifest's note on 2026-09-08), so an engine that
runs at all is that file value in force rather than a default — and the 8.6 part-2 lane switched it
off and on again by hand on this same box an hour before this press. Two keys, two different
arguments, kept apart.

## 4 — two defects this press found, both fixed here, both driven by a failing test first

**1. `applier.configure('mod-ale')` reported that nothing further was needed.**
`rebuild_required=False restart_recommended=False`, over a world that went on loading scripts from
the previous `ALE.ScriptPath` until this gate restarted it by hand. `configure` never sets
`rebuild_required` (`apply.py:_report`, `action != "configure"`), and the derivation behind
`restart_recommended` reads NPCs, direct SQL and server DBCs — none of which a conf write touches.
The field that exists for exactly this, `build.restart`, was declared by TBC's five conf mods and
**not** by this WotLK module whose whole `configure` is a conf write. Its own schema description
names the shape in advance: *"An item whose whole content is `conf[].keys` reported 'nothing further
needed' while the value it had just written sat in a file the emulator reads once, at startup."*

RED first, with the message kept:

```
tests/test_apply.py::test_configuring_ale_asks_for_the_restart_the_engine_needs
E  AssertionError: configuring ALE writes ALE.Enabled/ALE.ScriptPath into a file the engine
   reads at startup; a report that recommends nothing tells the user the value is in force
E  assert False is True
E   +  where False = Build(rebuild=True, restart=False).restart
```

Fixed by declaring the fact in `manifests/wow-wotlk/modules/mod-ale.json` (`build.restart: true`),
with the measurement written into the manifest's notes. Not by widening the derivation: what a
given item needs after a conf write is the item's own fact, and the field was added for it.

**2. A conf key the catalog names with no value was dropped in silence** — bug-checklist 44a,
opened by 8.7a's own run and unfixed until now. `mod-npc-beastmaster.json` names
`Creatures.CustomIDs` on `env/dist/etc/worldserver.conf` with a note and no `default`;
`Applier._conf` built `writes` from the keys that have one and `continue`d, so that conf entry
appeared in neither `done` nor `skipped` and the file was byte-identical afterwards. RED first:

```
tests/test_apply.py::test_a_conf_key_the_catalog_names_with_no_value_is_reported_not_dropped
E  AssertionError: assert 'conf env/dist/etc/worldserver.conf: no value in the catalog for
   Creatures.CustomIDs — not written' in ()
```

Now reported in `skipped`, naming the file and every valueless key. **Reported rather than filled
in**, deliberately: which value belongs in a user's core configuration is the catalog's sentence to
write, and `Creatures.CustomIDs` in particular is an **append** to a comma-separated list that this
applier has no syntax for. That leaves 44a's second half open, and it is named as open.

`origin/rust-main` settled the shape of both: `moduletail.rs:31-47`'s `ConfActivateOutcome` gives
every not-done outcome a name of its own (*"Only `Activated` wrote anything; the other three are
QUIET outcomes"*), and `tuning.rs:596-616` discriminates a failed key write by re-reading the file
and answering `NOT_FOUND` rather than shrugging.

## 5 — what this press does NOT settle

* **It is `mod-ale`, on `wow-wotlk`, on this box.** 8.7a's own run was against
  `mod-npc-beastmaster` and `mod-1v1-arena`, and neither is compiled into any binary anywhere, so
  their conf files are still invisible to their servers for the reason 8.7a measured. The command
  that would settle that half, which **this lane did not run** and which the owner runs:

  ```
  cd /home/pk/wowserver && docker compose -f docker-compose.yml -f docker-compose.build.yml \
      build ac-worldserver && docker compose up -d --no-deps ac-worldserver
  ```

  (or the app's own Rebuild button, whose live press is in `rebuild-live-yulon-ubuntu2-2026-09-09/`)
  after installing that module here.
* **The `Creatures.CustomIDs` half of the clause is unreachable on 601026 whatever we write.** The
  key's only effect is to silence a load-time gossip complaint, and 8.7a measured that this
  module's own creature row carries `flags_extra = 2` (`CREATURE_FLAG_EXTRA_MODULE`), which
  suppresses that complaint by itself (`ObjectMgr.cpp:1219-1229`) — the boot logged none. So a
  running server cannot show that key's value for that creature, and the manifest's note (*"add
  601026 to silence a harmless gossip warning"*) describes a warning that does not occur. Read
  again here, unchanged: `Creatures.CustomIDs = "190010,55005,999991,25462,98888,601014,34567,34568"`.
* **`playerbots.conf` is still missing from the same log line**, exactly as 8.6 and the rebuild lane
  recorded: that module's conf ships as a `.dist` this install never activates, so mod-playerbots
  runs on its compiled defaults. Untouched here; it is the evidence that the *reading* works, not a
  second failure.
* **Ticking 8.7a is the owner's session's job.** `pyplan/checklist.md` was not edited by this lane.

## Where the plan is silent

Read before deciding either fix, and neither page answers:

* **What a conf-only `configure` owes the user.** `phase8-decisions.md` and
  `phase8-parity-decisions.md` say what a rebuild owes (owner answer 2: a rollback, restored
  automatically) and what the channel press owes; neither says what a press that writes a value the
  running server has not read yet must tell them. The `build.restart` field's own schema description
  is the closest thing to a rule, and it describes the failure rather than prescribing the remedy.
  Answered here one way — the item declares it — and named as a choice made in the silence.
* **What to do with a key the catalog names and gives no value.** Nothing anywhere says whether that
  is a manifest bug, a prompt, or a step to report. Answered here as *report it*, which is the
  smallest true thing; filling it in would have needed a list-append syntax the manifest schema does
  not have and a decision about a user's core configuration that is not mine.

`phase8-parity-decisions.md:457` does sanction edits to this exact file — *"The Lua engine's
manifest — the conf key corrected to the one the module has, and a revision pinned"* — which is the
line `build.restart` was added under.

## The checkpoint that was not taken

The run sheet says to checkpoint `yulon-ubuntu2` before anything destructive. Read on the host
before doing so (`Get-VMSnapshot`, `Get-Volume`, `Get-ChildItem U:\VMs\yulon-ubuntu2`):

| Fact | Reading |
| --- | --- |
| checkpoints already on the VM | `rebuild-before-ale-2026-09-09` (22:21), `8.6-before-my-party-2026-09-09` (23:52), `7.10-before-regression-rerun-2026-09-09` (00:34) |
| free space on `U:` | **10.9 GB** |
| the chain already on that disk | `yulon-ubuntu2.vhdx` 64.25 GB + three differencing disks (13.96 / 1.19 / 1.05 GB) |
| memory assigned to the running VM | 16 GB — the rebuild lane's Standard checkpoint of this VM wrote **7.85 GB** of `.VMRS` |

A fourth memory-saving checkpoint would have left roughly 3 GB for a differencing disk that grows
while a crash-loop press writes MySQL and worldserver logs into it. Filling the volume under a live
VM is how a box is lost, so **no checkpoint was taken**, no existing one was deleted (each is
another lane's rollback point), and the rollback point for this press is the newest of the three,
`7.10-before-regression-rerun-2026-09-09`, taken 22 minutes before this press began:

```
ssh vmhost 'Restore-VMSnapshot -VMName yulon-ubuntu2 -Name "7.10-before-regression-rerun-2026-09-09" -Confirm:$false'
```

Restoring it would undo lane 7.10's sweep as well as this press. Nothing here needs it: everything
this press wrote is listed below and was put back by the gate's own `restore` stage.

## Two other lanes were on this box tonight

Named because their fingerprints are in this evidence and a reader would otherwise attribute them
to this press:

* `mod_ale.conf` carries `Logger.ALE=4,Console ALELog` and a sibling file
  `mod_ale.conf.before-logger-fix` (00:25 VM local) — **lane 7.10's** edit. Left exactly as found;
  the app's own `configure()` rewrote only its own two keys and reproduced the rest byte-for-byte.
* `env/dist/etc/modules/lua_scripts/LootPet.lua.bak-20260909-002427` — the **8.6 part-2** lane's
  backup of ALE's own shipped example. Left in place. It is not a `.lua` file any more, so the
  engine does not load it, and this boot's log names five scripts, not six.

The box was quiet before this press began (`~/claude-activity.log`: lane 7.10's cleanup finished
00:51:37, no python of its own left running), and the press was announced on the activity terminal
before every action.

## What this press changed on the box, and how it was put back

* `mod_ale.conf` was edited twice — once by this gate's hand (the decoy path) and once by the app's
  `configure()` — and is now **byte-identical to the file as found**, verified rather than assumed:
  `the file now == the file as found: True` (`5-restore.log`), against a copy taken before the first
  write (`mod_ale.conf.as-found`, committed here).
* The decoy directory `env/dist/etc/modules/lua_gate_decoy/` and its one marker script were removed
  (`decoy directory removed: True`).
* The world was stopped and started twice through the app, so `ac-worldserver` and `ac-authserver`
  are on their **third** container start of the night; the image is unchanged (`cad2566406d2`), and
  `RestartCount` is 0 on all three containers. Two pre-stop log snapshots were written by `logsnap`
  to `~/.local/share/yulon/logs/` — that is the app doing its job, not a leftover.
* No database write, no account, no character, no module installed or removed, no realm row touched:
  the app's Start does not advertise a realm (`controller.py:201-220`), so lane 7.10's realm address
  is as it left it.
* `~/lane87a/` holds this lane's tree and `out/`.

## Files

| file | what it is |
| --- | --- |
| `gate87a5.py` | the runner: `ground`, `perturb`, `configure`, `verify`, `restore` |
| `1-ground.log` | the ground, and which assertions were already true |
| `2-control.log` | the decoy path in force, and the bridge command gone |
| `3-press.log` | `configure()`, the file read back, the restart, and the proof |
| `5-restore.log` | the file put back byte-for-byte, the decoy removed |
| `mod_ale.conf.as-found` | the file before anything, for the byte comparison above |
| `1-ground-the-values-are-already-right.png` | the tab at the ground |
| `2-wrong-value-the-running-server-follows-it.png` | the control |
| `3-the-app-wrote-it-back-file-only.png` | the app's report and the file — the insufficient half |
| `4-the-running-server-shows-the-value-the-app-wrote.png` | the clause met |

## Reproducing

```
rsync -a --exclude .venv ~/dads-mmo-lab/pylauncher/ ~/lane87a/pylauncher/   # then this lane's apply.py
scp gate87a5.py <box>:~/lane87a/
ssh <box> 'cd ~/lane87a && QT_QPA_PLATFORM=offscreen ~/dads-mmo-lab/pylauncher/.venv/bin/python \
    gate87a5.py ground'      # then perturb, configure, restore — one per invocation
```

One stage per invocation on purpose: `perturb` and `configure` each stop and start the whole stack,
and a gate that times out half-way through a stop leaves a server down.
