# 8.1a's crash-loop clause, re-pressed under the three-strike rule — 2026-09-08

The clause changed today. `dashboard.LOOP_RESTART_STRIKES` went from 1 to 3 (commit *One restart is a
hiccup; three are a loop*), and 8.1a's definition of done now reads "a crash-looping world reads as a
restart loop **within two polls of its third new restart**". The checklist line says in as many words
that the crash-loop stage of this gate is owed a re-press under the new rule. This is that re-press.

**Code under test:** `78305907` (`pylauncher/yulon/dashboard.py:39`, `LOOP_RESTART_STRIKES = 3`),
checked out on the box at `~/gate81a3s`. Runner: `gate81a3s.py`, committed beside this file.
Interpreter `~/gate81b-venv/bin/python` (3.11.15, PySide6 6.11.2).

## Where this ran, and where it did not

**Not on 8.1a's own box.** `yulon-ubuntu`'s virtual disk is
`D:\VMs\yulon-ubuntu-moved\…\yulon-ubuntu_FDA3EB6B-….avhdx`, and the Hyper-V host's **D: drive left
the SATA bus at 12:40:41 on 2026-09-08**. What the two machines said, read on 2026-09-08 ~16:45–16:50:

| where | what it says |
|---|---|
| host, System log, first event | `9/8/2026 12:40:41 PM  disk  51`, then `Microsoft-Windows-Ntfs 140` |
| host, still firing at 16:48 | `The system failed to flush data to the transaction log … VolumeId: D: … Failure status: A device which does not exist was specified` — 75 `iaStorE 4155` and 74 `Ntfs 140` in five minutes |
| host, a read of the VHDX | `Get-Content D:\VMs\…\yulon-ubuntu.vhdx` → **`A device which does not exist was specified`** |
| host, `Get-Partition` for D: | returns **nothing** — the volume is no longer enumerable |
| guest kernel, 12:40:53 | `EXT4-fs error (device sda2): … Journal has aborted` … `EXT4-fs (sda2): Remounting filesystem read-only` |
| guest, `mount` | `/dev/sda2 on / type ext4 (rw,relatime,emergency_ro)`; `touch` → `Read-only file system` |
| guest, `docker inspect ac-worldserver` | `exited`, `ExitCode 139`, `Error: open /var/lib/docker/containers/…/hosts: read-only file system` — while `docker ps` still prints `Up 6 hours` |

A **write** to D: from the host still reports success (`"probe" | Out-File D:\ro-probe.txt` returned
no error) while a read of a file already on it fails. That is not evidence the drive is alive: the
write is buffered, and the host's own `Ntfs 50` events say what happens to it — `{Delayed Write
Failed} Windows was unable to save all the data for the file D:\$BitMap. The data has been lost.`
A probe that has not been flushed proves nothing about the disk it was aimed at.

Two consequences beyond this box. `Get-VMSnapshot -VMName yulon-ubuntu` now returns **nothing** — the
host logs `Cannot load a checkpoint configuration: The data is invalid` for eight checkpoint IDs — so
the two checkpoints the run sheet says must not be deleted (`8.9a-before-purge-2026-09-08`,
`8.9a-purge-gate-2026-09-08b`) are **not readable**. And `D:\VMs` also holds `yulon-win11-gate`,
`yulon-fedora-gate` and `DML-server`, none of which appear in `Get-VM` any more; `ssh
yulon-win11-gate` times out. `yulon-win11` (E:), `yulon-fedora` (X:) and `yulon-arch` (Y:) are
unaffected.

**Nothing was done about it.** No storage rescan, no VM power cycle, no checkpoint touched — a failing
disk under a live VM is the owner's call, and the host must not be rebooted. The command that would
settle whether the drive is recoverable rather than gone, run on `vmhost`, is:

```
Update-HostStorageCache; Get-Partition | Where-Object DriveLetter -eq D
```

and the command that would then try to bring the guest back, accepting an fsck and whatever the lost
writes did to `/home/pk/wowserver`, is `Stop-VM -Name yulon-ubuntu -TurnOff -Confirm:$false` followed
by `Start-VM -Name yulon-ubuntu`.

**So this press ran on `m910q`, against `~/tbc-7.4c`** — the TBC tree, which is 8.1b's. It proves the
shared verdict code and both directions of the new rule. **It does not discharge 8.1a's own
WotLK/Linux line**, and nothing here may be copied onto it: a per-tree fact is measured per tree.

## What was proved

| Direction | Stage | Result |
|---|---|---|
| The threshold crossed **one strike at a time**, on a world docker calls `running` throughout | `strikes` | 1 strike → `up`; 2 strikes → `up`; **3 strikes → `restart loop — 3 restarts`**, on the first tick after the third new restart — **zero polls later**, inside the "within two polls" bound |
| ONE restart followed by a healthy run reads `up`, not a loop | `single` | RestartCount `1 → 2`, watcher holds **exactly one strike**, tab says `up` at 4 s and still `up` — same watcher, same one strike — 1 m 20 s later with the bots back to 120 |
| The same live readings under the OLD threshold | `single` (replay) | threshold 3 → `['up','up','up','up','up','up']`; **threshold 1 → `['up','restart_loop','restart_loop','restart_loop','restart_loop','restart_loop']`** — the 8.2b false alarm, reproduced and then refuted |
| stage_d2's own recipe (database taken away, six polls at ten seconds) | `loop` | third new restart at **poll 1**, `restart loop` verdict at **poll 1** |

### The two frames that are the whole point

`5-tab-strike-one-of-three.png` and `6-tab-strike-three-of-three.png` are the same tab, the same
running world, four seconds of layout apart:

```
5: up — 0 players, 0 bots, up 22s                  status: db up, auth up, world up    [Turn on the command channel]  ENABLED
6: restart loop — 3 restarts, this run up 22s      status: db up, auth up, world up    [Turn on the command channel]  GREYED
```

Frame 5 is the reading 8.2b photographed as `restart loop — 1 restarts` with that button greyed on a
healthy server. Under the shipped rule it says `up` and the button is live. Frame 6 is what a loop
now has to earn.

### The strike stage, verbatim

```
[15:15:09Z] GROUND, world   : {"status": "running", "restarts": 0, "started_at": "…T15:14:02Z", …}
[15:15:10Z] GROUND, watcher : _last_restarts=0 _strikes=0 _looping=False
[15:15:10Z] GROUND, the tab says: 'up — 0 players, 127 bots, up 1m'
--- kill 1 ---
[15:15:33Z] kill 1 probe  5: docker 'running'/1 | strikes=1 looping=False | tab 'up — 0 players, 0 bots, up 22s'
--- kill 2 ---
[15:15:57Z] kill 2 probe  5: docker 'running'/2 | strikes=2 looping=False | tab 'up — 0 players, 0 bots, up 22s'
--- kill 3 ---
[15:16:02Z] kill 3 probe  1: docker 'running'/3 | strikes=3 looping=True  | tab 'restart loop — 3 restarts, this run up 3s'
[15:16:20Z] the first `restart loop` on a RUNNING world came at: kill 3 probe 1, with the world RUNNING and 3 strikes
```

`docker 'running'` at every one of those readings. The `status == "restarting"` short-circuit — the
daemon's own word, which decides the verdict on its own — is **not** what produced frame 6; the strike
count is.

## Three things the machine said that the plan does not

**1. stage_d2's cadence cannot observe the third strike arriving on this tree.** The recipe is six
polls at ten seconds. Measured: with the database stopped, mangosd is dead in under a second, and at
the **first** ten-second poll `RestartCount` had already gone **2 → 9**. Seven new restarts inside one
poll interval, so the third strike was never on screen alone, and the container was in `restarting`
at every poll — which decides the verdict by itself, with no arithmetic. The recipe therefore answers
the definition of done ("within two polls of its third new restart": zero polls later) **without
exercising the rule the re-press exists to check**. That is why `stage_strikes` was written: the
database is left up and the world is killed three times, each kill after the previous run has lasted
long enough for docker to reset its own backoff, so the count climbs 1, 2, 3 with the world `running`
at every reading.

**2. `docker start` on a stopped container resets `RestartCount` to zero on this daemon.** Read
directly: the world was left `exited` with `RestartCount 12` at 15:13:01, `docker start tbc-mangosd`
was issued, and at 15:14:02 the same container read `running restarts=0`. This is the behaviour
`Dashboard._restarted()`'s docstring records from 2026-09-07 (`8 → 0` on a compose `Started`, not a
`Recreated`), measured again here on a plain `docker start`. It is also why the delta baseline, and
not the absolute count, is the only safe thing to threshold.

**3. `docker ps` and `docker inspect` can disagree about the same container.** On the broken
`yulon-ubuntu`, `docker ps` printed `ac-worldserver  Up 6 hours` while `docker inspect` on the same
container returned `exited`, `ExitCode 139`. The dashboard reads `docker inspect` (`docker.py`,
`container_state()`), which is the half that was telling the truth — but any check that trusts
`docker ps` alone would have called that host healthy.

## Instruments, and why each

* The tab is the **real `ControllerView`** built through `ControllerServices.for_entry`, with
  `status_poll_ms=0` — the view's own "do not poll" setting, so the timer never starts and every tick
  in the transcript is one the script asked for and the watcher's strike count is exactly the number
  of ticks above it. `view.grab()` is the same capture 8.7b used on this box.
* The strike count printed each line is `services.dashboard.__self__._strikes` — **the tab's own
  watcher**, not a second one kept alongside. A fresh watcher would baseline at the post-restart
  count and hold no strike at all, which is a different claim.
* The kill is `sudo kill -9 <.State.Pid>` **from the host**. Not `docker kill`, which sets Docker's
  explicitly-stopped flag so `restart: unless-stopped` never fires (measured on `yulon-ubuntu`,
  2026-09-06, and the reason stage_d2 exists); and not a signal from inside the container, which the
  kernel drops for PID 1 unless that process installed a handler.
* Every capture logs `.State.Running` for the world container at the moment of the shot — all eight
  say `true`. A dead container photographs like a refusal.
* Every stage prints the state it found **before** it acted, including `_strikes`, because a step
  whose assertion is already true at the start proves nothing. `up` was already the answer before the
  single restart; what had to become true is `up` **while the watcher holds one strike**.

## What was left behind

`m910q` is as it was found: **no game server up**, only the unrelated `r6` container that was already
running. The TBC stack was started at 14:54:42Z for this press and stopped at 15:17:00Z (`docker
compose stop` in `~/tbc-7.4c`); `tortoise-*` and `rehearsal-*` were exited when this began and were not
touched. `tbc-mangosd` exits `139` on a compose stop — it did so before this press too
(`Exited (139) 32 minutes ago` in the ground reading) and that is not something this press caused.
The worktree `~/gate81a3s` and the output directory `~/gate81a3s-out` were left in place, as the
box's other gate checkouts are.

`yulon-ubuntu` was **not** touched: read only, and left running on its dead disk.

## Files

| file | what it is |
|---|---|
| `gate81a3s.py` | the runner: `ground`, `single`, `loop`, `strikes` |
| `strikes.txt`, `strikes-readings.json` | the threshold crossed one strike at a time |
| `single.txt`, `single-readings.json` | one restart, a healthy run, and the old-threshold replay |
| `loop.txt`, `loop-readings.json` | stage_d2's own recipe, six polls at ten seconds |
| `0-tab-ground-zero-strikes.png` | the tab as found, before anything |
| `1-tab-before-the-single-restart.png` | up, 500 bots, zero strikes |
| `2-tab-one-strike-reads-up.png` | one strike, 4 s into the new run |
| `2b-tab-one-strike-after-a-real-run.png` | the same watcher, still one strike, bots back |
| `3-tab-before-the-crash-loop.png` | up, 500 bots, before the database is taken away |
| `4-tab-three-strikes-reads-restart-loop.png` | `restart loop — 9 restarts`, `db down` |
| `5-tab-strike-one-of-three.png` | one strike on a running world: `up`, button live |
| `6-tab-strike-three-of-three.png` | three strikes on a running world: loop, button greyed |
