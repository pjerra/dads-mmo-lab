# 8.1a's crash-loop clause under the three-strike rule, on its own tree — 2026-09-09

The clause 8.1a's line now carries: *a crash-looping world reads as a restart loop **within two
polls of its third new restart** rather than reading as up*. `dashboard.LOOP_RESTART_STRIKES` went
1 → 3 on 2026-09-08, and the line says the crash-loop stage of this gate
(`gate81a.py stage_d2`, six polls at ten seconds against a world with no database) is owed a
re-press under the new rule.

That re-press ran on 2026-09-08 — **on `m910q`'s TBC tree**, because `yulon-ubuntu`'s virtual disk
left the Hyper-V host's SATA bus that lunchtime. Its README says so in as many words and says this
WotLK press is still owed: *"It does not discharge 8.1a's own WotLK/Linux line, and nothing here may
be copied onto it: a per-tree fact is measured per tree."* **This is that press.**

**Box:** `yulon-ubuntu2` (16 GB, 12 vCPU) — `yulon-ubuntu`'s replacement, whose registration was not
touched. AzerothCore WotLK at `/home/pk/wowserver`, installed through the app's own engine on
2026-09-08 and rebuilt with `mod-ale` compiled in the same night. **Time:** 23:04–23:12 UTC
(01:04–01:12 VM local). **Code under test:** `96a129dc`, `pylauncher/yulon/dashboard.py:39`
(`LOOP_RESTART_STRIKES = 3`), copied to `~/lane87a/pylauncher` and read back by the gate itself:
`LOOP_RESTART_STRIKES as shipped in this checkout: 3`. **Interpreter:**
`~/dads-mmo-lab/pylauncher/.venv/bin/python` (3.12.3, PySide6 6.11.2, offscreen).
**Runner:** `gate81a3s_wotlk.py`, committed beside this file — the TBC re-press's `gate81a3s.py`
with its four constants changed for this tree (server dir, catalog entry, output dir, checkout),
`stage_single` dropped (the false-alarm direction and its old-threshold replay were proved there and
are shared code, not per-tree), a `restore` stage added, and one assertion added: the world's restart
policy is read before the first kill.

**No checkpoint was taken, and that was a measurement.** The reasoning, with the numbers, is in the
sibling folder `pyplan/gates/8.7a-wotlk-yulon-ubuntu2-2026-09-09/README.md` (*The checkpoint that
was not taken*): `U:` had **10.9 GB** free, this VM's last Standard checkpoint wrote **7.85 GB** of
`.VMRS`, and three other lanes' checkpoints were already on the disk. The rollback point is the
newest of those, `7.10-before-regression-rerun-2026-09-09`, taken 30 minutes before this press.
Nothing here needed it: the stack was put back with the app's own Start and read back afterwards.

Every capture is the real `ControllerView` Server tab with `status_poll_ms=0` — the view's own "do
not poll" setting, so the timer never starts and every tick in the transcript is one the script
asked for, which makes the watcher's strike count exactly the number of ticks above it. Every
capture logs `.State.Running` for the world container at that instant: **all six say `true`**.

---

## What was proved

| Direction | Stage | Result |
| --- | --- | --- |
| the threshold crossed **one strike at a time**, on a world docker calls `running` throughout | `strikes` | 1 strike → `up`; 2 strikes → `up`; **3 strikes → `restart loop — 3 restarts`**, on the first probe after the third kill — **zero polls later**, inside the "within two polls" bound |
| stage_d2's own recipe (the database taken away, six polls at ten seconds) | `loop` | third new restart at **poll 1**, `restart loop` verdict at **poll 1** — and what that cadence cannot see, below |
| the command-channel interlock the later boxes key off | both | `enabled=True` at one strike, `enabled=False` at three, on the same running world |

### The two frames that are the whole point

`1-tab-strike-one-of-three.png` and `2-tab-strike-three-of-three.png` are the same tab, the same
running world, forty-seven seconds apart:

```
1: up — 0 players, 0 bots, up 22s             status: db up, auth up, world up   [Turn on the command channel]  LIVE
2: restart loop — 3 restarts, this run up 22s  status: db up, auth up, world up   [Turn on the command channel]  GREYED
```

Frame 1 is the reading 8.2b photographed as `restart loop — 1 restarts` with that button greyed on a
healthy server. Under the shipped rule it says `up` and the interlock is open. Frame 2 is what a loop
now has to earn — and both were taken at the same 22 s of uptime, on the same container, so nothing
but the strike count differs between them.

**One thing in this gate's own transcript is wrong about those frames, and it is left here as a
correction rather than deleted.** Every capture line logs
`'Turn on the command channel': visible=False enabled=…`, and I wrote the record's first draft off
that: *the button is not on screen at all*. The pictures say otherwise — the button is there in both,
live in frame 1 and greyed in frame 2. `QWidget.isVisible()` is False for a widget whose top-level
window has never been shown, which is every widget in an offscreen `grab()`; it says nothing about
what was painted. `isEnabled()` is the reading that meant something, and the picture is the one that
settles it. A gate that only printed `visible` would have reported the interlock missing.

### The strike stage, verbatim

```
[23:04:18Z] GROUND, world   : {"status": "running", "restarts": 0, "pid": 162770, "policy": "unless-stopped"}
[23:04:18Z] GROUND, database: {"status": "running", "restarts": 0}
[23:04:19Z] GROUND, watcher : _last_restarts=0 _strikes=0 _looping=False
[23:04:19Z] GROUND, the tab says: 'up — 0 players, 500 bots, up 2m'
--- kill 1 ---
[23:04:42Z] kill 1 probe  5: docker 'running'/1 exit=0 | strikes=1 looping=False | tab 'up — 0 players, 0 bots, up 22s'
--- kill 2 ---
[23:05:06Z] kill 2 probe  5: docker 'running'/2 exit=0 | strikes=2 looping=False | tab 'up — 0 players, 0 bots, up 22s'
--- kill 3 ---
[23:05:11Z] kill 3 probe  1: docker 'running'/3 exit=0 | strikes=3 looping=True  | tab 'restart loop — 3 restarts, this run up 3s'
[23:05:30Z] the first `restart loop` on a RUNNING world came at: kill 3 probe 1, with the world RUNNING and 3 strikes
```

`docker 'running'` and `exit=0` at every one of those readings, and the world's restart policy was
read before the first kill (`unless-stopped`) — the stage asserts it, because a policy of `no` would
mean a kill produces no restart at all and the ladder could not exist. The `status == "restarting"`
short-circuit — the daemon's own word, which decides the verdict on its own — is **not** what
produced frame 2; the strike count is.

The instrument is the tab's own watcher (`services.dashboard.__self__._strikes`), not a second one
kept alongside: a fresh watcher would baseline at the post-restart count and hold no strike at all,
which is a different claim. The kill is `sudo kill -9 <.State.Pid>` from the host — not `docker
kill`, which sets Docker's explicitly-stopped flag so `restart: unless-stopped` never fires (the
recipe this line carried until 2026-09-06, and the reason `stage_d2` exists), and not a signal from
inside the container, which the kernel drops for PID 1 without a handler.

## stage_d2's own recipe on this tree, and the two things it cannot show

`3-tab-before-the-crash-loop.png` → `4-tab-three-strikes-reads-restart-loop.png`, `81a-3-loop.log`.
Ground first: a **fresh** watcher baselined at `_last_restarts=3` with `_strikes=0`, and the tab said
`up — 0 players, 270 bots, up 43s` — so the verdict this stage produces is not left over from the
stage before it.

```
[23:05:55Z] stopping the database (ac-database) so the world has nothing to connect to
[23:05:55Z] killing the world's main process so the crash cycle starts now: sudo kill -9 166108
[23:06:06Z] poll  1: docker 'restarting'/10 (new 7) exit=1 | strikes=7 looping=True | tab 'restart loop — 10 restarts, this run up 1s'
            ^ the THIRD new restart is on the board at poll 1
            ^ the tab first said `restart loop` at poll 1
[23:06:59Z] poll  6: docker 'restarting'/13 (new 10) exit=1 | strikes=10 looping=True
third NEW restart at poll: 1     first `restart loop` verdict at poll: 1
```

The clause is met by the recipe as written — zero polls between the third new restart and the
verdict, inside the "within two polls" bound. **And on this tree the recipe cannot exercise the rule
it re-presses**, which is the same measurement the TBC press made and is now a per-tree fact on
WotLK too:

1. **Seven new restarts arrived inside one ten-second poll** (`RestartCount` 3 → 10). The third
   strike was never on screen alone, so the arithmetic 1 → 2 → 3 is invisible at this cadence. That
   is why `stage_strikes` exists and why it is the stage the two frames above come from.
2. **The container read `restarting` at every poll**, and that word decides the verdict by itself
   with no strike counting at all. A rule change from 1 to 3 could not fail this stage.

**Two per-tree facts, recorded rather than inherited:**

* AzerothCore's worldserver **exits** when its database is gone — `exit=1` at every poll here.
  CMaNGOS' `mangosd` stays up retrying, which is what made 8.1b's `stable` say yes about a world
  whose database had gone (measured 2026-09-06 on `m910q`). The two trees fail differently, and the
  restart storm above is a consequence of this one: each exit is a new restart.
* On this daemon the `db down` half of the status line appeared **with the world container still
  reading `world up`** (`status: db down, auth up, world up`), because the container was mid-restart
  and its state is not the server's health. `4-tab-three-strikes-reads-restart-loop.png` was taken
  at seven strikes, not three — named here rather than captioned as a three-strike frame, which is
  what the strikes stage is for.

## What was left on the box

Put back through the app's own Start and then read, not assumed (`81a-4-restore.log`):

* `ac-database`, `ac-authserver`, `ac-worldserver` **up**, on the same image the press began with,
  and the world reading **`RestartCount` 0** — not the 13 this press caused. That is worth writing
  down rather than tidying away: the world was left `exited` with 13, and the app's Start
  (`compose up -d --no-deps`) **recreated** the container, so the count went 13 → 0 with a new
  container id, pid and `StartedAt` (23:07:57Z). It is the same behaviour `Dashboard._restarted()`'s
  docstring records from 2026-09-07 (`8 → 0` on a compose `Started`) and the TBC press measured again
  on a plain `docker start` — and it is why the dashboard thresholds the **delta** and not the
  absolute count. My README sentence before this one said the opposite until the restore stage was
  read: an assumption about a container's residue is not a reading of it.
* The bot population climbs back on its own: `up — 0 players, 0 bots, up 30s` thirty seconds after
  the start, and **500 again at 23:12:19Z**, read from the database rather than from the tab
  (`SELECT COUNT(*) FROM acore_characters.characters WHERE online=1` → `500`), which is the same
  count the ground reading had (`up — 0 players, 500 bots, up 2m`).
* Nothing else: no conf file, no database write, no account, no character, no module, no realm row.
  The 8.7a press in the sibling folder ran first, and everything it changed it put back.

## Files

| file | what it is |
| --- | --- |
| `gate81a3s_wotlk.py` | the runner: `ground`, `strikes`, `loop`, `restore` |
| `81a-1-ground.log` | the tab, the watcher and the three containers as found |
| `81a-2-strikes.log`, `strikes-readings.json` | the threshold crossed one strike at a time |
| `81a-3-loop.log`, `loop-readings.json` | stage_d2's own recipe, six polls at ten seconds |
| `81a-4-restore.log` | the stack back up through the app's Start, and the tab read again |
| `0-tab-ground-zero-strikes.png` | as found: `up — 0 players, 500 bots, up 2m`, zero strikes |
| `1-tab-strike-one-of-three.png` | one strike on a running world: `up`, interlock open |
| `2-tab-strike-three-of-three.png` | three strikes on a running world: a loop, interlock closed |
| `3-tab-before-the-crash-loop.png` | before the database was taken away |
| `4-tab-three-strikes-reads-restart-loop.png` | the recipe's own verdict, at seven strikes |
| `5-tab-left-as-found.png` | the box left as it was found |

## Reproducing

```
scp gate81a3s_wotlk.py <box>:~/lane87a/          # beside an exported pylauncher/
ssh <box> 'cd ~/lane87a && QT_QPA_PLATFORM=offscreen ~/dads-mmo-lab/pylauncher/.venv/bin/python \
    gate81a3s_wotlk.py ground'                   # then strikes, loop, restore
```

`strikes` needs the stack up and the database left alone; `loop` takes the database away and puts it
back; `restore` is the only stage that brings the world up again, and it is a separate invocation so
a timeout cannot leave a server down.
