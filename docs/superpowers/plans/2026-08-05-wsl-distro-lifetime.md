# WSL distro lifetime — why `dml-arch` powers off under a live server

**Date:** 2026-08-05. **Branch:** `feat/arch-wsl-backend`. **Base HEAD:** `e226497`.
**Status:** mechanism established by measurement; mitigations tested; one
launcher change specified but **NOT implemented** (deliberately out of scope
here — see "The launcher change" at the end).

Follow-up to the finding recorded in
[`docs/backend-comparison-2026-08.md`](../../backend-comparison-2026-08.md)
("The finding that was not on the list"), which recorded the *observation* —
WSL shut the Arch distro down with 1,948 bots online — but not the *cause*.

---

## The one-sentence answer

**WSL 2.7.10 terminates a distro instance ~15 s after the last `wsl.exe`
client session on that distro exits, regardless of what is still running inside
it; the timer is reset only by another session *into that distro*, and there is
no configuration key anywhere that changes or disables it.**

Everything else below is evidence for that sentence, or a consequence of it.

---

## Environment

```
WSL version      2.7.10.0
Kernel           6.18.33.2-microsoft-standard-WSL2
Windows          10.0.26200.8875
Distros          dml-arch (systemd=true, default user dml), docker-desktop
```

`C:\Users\perzi\.wslconfig` as found (unchanged by this work, restore verified —
SHA256 `FAA1386C…3EB273` before and after):

```ini
[wsl2]
memory=16GB
processors=6
swap=10GB
localhostForwarding=true
vmIdleTimeout=60000

[experimental]
autoMemoryReclaim=gradual
```

`vmIdleTimeout=60000` is 60 s and is the documented **default**; the file states
it explicitly rather than changing it. `/etc/wsl.conf` inside `dml-arch` carries
only `[boot] systemd=true` and `[user] default=dml` — no lifetime keys, and none
exist to put there.

---

## Method — and why the numbers are trustworthy

The obvious way to watch a distro die is to poll `wsl --list --verbose`. That is
**invalid here**, because the thing being measured is a timer driven by
`wsl.exe` activity — the observer is the independent variable. Any harness that
polls with `wsl.exe` risks measuring itself.

So the oracle never calls `wsl.exe` during the observation window:

* A heartbeat loop inside the distro appends a timestamp + `/proc/uptime` to a
  file on `/mnt/c` every 0.5 s.
* Windows watches that file's **length** grow. When it stops growing, the distro
  is dead. Death time is the last observed growth, resolution ~0.5 s.
* `vmmemWSL` presence is read with `Get-Process` — Windows-side, no `wsl.exe`.

Harness: `Measure-DistroLifetime.ps1` (scratchpad, not committed — it is a
throwaway measuring instrument, and every number it produced is reproduced in
the tables below).

### Two traps hit while building it, both worth recording

**1. The workload has to live at the right layer.** The first heartbeat was
`setsid nohup … &` from the `wsl.exe` session. It produced *zero* lines and the
run aborted. That was not the distro dying — a follow-up session found the
distro alive with `uptime=5.93` and `NO_HEARTBEAT_PROC`. **WSL tears down the
processes of a session when its client detaches**, so a background process
started from a `wsl.exe` call is not a proxy for the game server at all. The
server's containers are children of `dockerd`, a *systemd system service*, which
survives session teardown. The heartbeat was moved to a transient systemd system
unit (`systemd-run --unit=dml-hb --collect`) to match that layer. Only then did
it measure the right thing.

**2. A probe whose failure mode looks like its negative answer.** To ask whether
an instance-idle-timeout key exists, the first attempt wrote candidate keys into
`.wslconfig` and ran `wsl --list --verbose` looking for WSL's `Unknown key`
warning. No warning appeared — which reads like "the key is valid". It is not:
`wsl --list` does not parse `.wslconfig` at all. The probe was redone with a
**real distro start and a positive control** (`zzzDefinitelyNotAKey`); see
below.

---

## The mechanism, in two stages

Stage 1 kills the server. Stage 2 is the one that is configurable, and it is not
the one that matters.

| Stage | What ends | Trigger | Configurable? |
|---|---|---|---|
| 1 | the **distro instance** (`dml-arch` → `Stopped`) | ~15 s after the last `wsl.exe` session into that distro exits | **No** |
| 2 | the **utility VM** (`vmmemWSL` process) | `vmIdleTimeout` after the last distro stops | Yes, `vmIdleTimeout` |

Measured, `vmIdleTimeout=60000`: distro dies at t0+14.8 s, `vmmemWSL` vanishes at
t0+76.6 s. 76.6 − 14.8 = **61.8 s ≈ the 60 s `vmIdleTimeout`**. The two stages
are independent and additive.

### Guest-side proof of who does it

The previous boot's persistent journal (`journalctl -b -1`) names the caller:

```
systemd-logind[158]: poweroff requested from client PID 577 ('systemctl') (unit init.scope)...
systemd-logind[158]: The system will power off now!
dockerd[197]: msg="Daemon shutdown complete"
```

`init.scope` is **WSL's own init**, i.e. PID 1's scope — not a user, not
systemd's own idea, not an OOM killer. WSL decided, and drove it through
`systemctl poweroff`. That is the same code path the original incident reported
from the guest kernel log:

```
WSL (2 - init-systemd(dml-arch)) ERROR: InitTerminateInstanceInternal:2763:
systemctl poweroff did not terminate the instance in 10000 ms, calling
reboot(RB_POWER_OFF)
```

### Why the original incident read ~25 s and the idle repro reads ~15 s

They are the same event with a different tail. WSL fires the idle timer at ~15 s
and asks systemd to power off. On an idle distro systemd completes that in
milliseconds. Under 1,948 bots + a MySQL container, systemd did **not** finish
within WSL's 10,000 ms grace, so WSL force-cut with `reboot(RB_POWER_OFF)` —
15 s + 10 s ≈ the "roughly 25 seconds" originally observed. The 10 s in the
error message is WSL's grace period, not the idle timeout.

This also explains, without any new hypothesis, why the containers looked
healthy in the post-mortem: `OOMKilled=false`, `ExitCode=0`, `dockerd` logging
`Daemon shutdown complete`. They were **stopped gracefully by systemd**, exactly
as if someone had shut the machine down. Nothing crashed. That is why every
crash-shaped explanation failed to fit.

---

## Measured survival — the distribution

Time from the last `wsl.exe` client exiting to the last heartbeat, distro idle
apart from `systemd`, `dockerd` and the heartbeat unit. Eight independent runs,
each preceded by `wsl --shutdown`:

| Run | Distro survival (s) | `vmmemWSL` gone (s) |
|---|---|---|
| baseline-1 | 14.9 | 76.0 |
| baseline-2 | 14.8 | 76.9 |
| baseline-3 | 14.7 | 76.6 |
| baseline-4 (kernel-log capture) | 14.8 | 76.7 |
| baseline-5 (kernel-log capture) | 14.9 | 76.7 |
| `vmIdleTimeout=-1` | 14.8 | **never** (watched 120 s) |
| poll with `wsl --list --verbose` @7 s | 14.8 | 76.8 |
| poll with a distro session @20 s | 14.7 | — |

**n=8, min 14.7, max 14.9, median 14.8, spread 0.2 s.** With 0.5 s heartbeat
resolution the underlying timer is **15.0 s**.

This is **not** a "sometimes" bug. It fired on every single unattended run, with
a spread of 200 ms. There is no configuration under which it did not fire.

---

## Mitigations tested

### 1. `.wslconfig` `vmIdleTimeout=-1` — **DOES NOT WORK**

Measured directly. The distro still died at **14.8 s**, identical to baseline.
The VM then never exited (watched 120 s, `vmmemWSL` still present).

So `vmIdleTimeout` governs stage 2 only. Setting `-1` is strictly the worst of
both worlds for this product: **the server still dies on schedule, and the now
immortal ~1.4 GB VM destroys the 1.0–1.6 GB idle saving that is the Arch
backend's entire reason to exist.** It converts the one thing Arch wins into a
permanent cost while fixing nothing.

Cost/ownership, moot given the above but worth stating: `.wslconfig` is
machine-wide and affects *every* WSL distro on the box including
`docker-desktop`. DML's installer does author this file (it says so in its own
header comment), so writing it is not unprecedented — but it is a user-owned
file the user has hand-tuned with load-bearing arithmetic in the comments, and
this change would not have bought anything.

### 2. No instance-level key exists — **CONFIRMED, with a positive control**

Candidate keys written into `.wslconfig`, then a **real distro start** (which is
what parses the file), watching for WSL's `Unknown key` warning:

| Key | `[wsl2]` | `[experimental]` |
|---|---|---|
| `instanceIdleTimeout` | Unknown key | Unknown key |
| `distroIdleTimeout` | Unknown key | Unknown key |
| `keepAlive` | — | Unknown key |
| `zzzDefinitelyNotAKey` / `zzzControlKey` (**control**) | Unknown key | Unknown key |
| `vmIdleTimeout` (**control, known-good**) | accepted, no warning | — |

Both controls behaved correctly, so the negatives are real negatives.
`wsl --manage <distro>` likewise offers only `--move`, `--set-sparse`,
`--set-default-user`, `--resize` — nothing about lifetime. `/etc/wsl.conf` has no
such key either.

**There is no configuration fix. The only lever is holding a session open.**

### 3. `wsl --list --verbose` polling every 7 s — **DOES NOT WORK**

Died at **14.8 s** despite 8 polls landing during the window. A `wsl.exe`
invocation that only queries the service does **not** reset the timer. Only a
session *into the distro* counts. This matters: it rules out the cheapest
possible keep-alive, and it means any future "cheap status check" that avoids
entering the distro will silently stop holding it.

### 4. A distro session every 7 s (what the launcher already does) — **WORKS**

`wsl.exe -d dml-arch -u dml --exec /bin/true` every 7 s for 60 s:

* distro **alive** throughout (8 polls),
* polling stopped at t0+60.3 s,
* distro died at t0+74.3 s — **14.0 s after the last poll.**

Causation, not correlation: the death moved with the observer. Every session
resets the 15 s timer.

### 5. A distro session every 20 s — **DOES NOT WORK** (the boundary is real)

Died at **14.7 s**, before the first poll ever fired. Confirms the semantics are
*reset*, not *disable*: any interval ≥ 15 s fails.

An ugly detail from this run: the polls that landed **after** death silently
**restarted the distro** (a `wsl -d …` call boots a stopped distro). With
`restart: unless-stopped` on the stack, `dockerd` then brings the containers
back. See "What `restart: unless-stopped` does" below.

### 6. A long-lived holder process — **WORKS**

`wsl.exe -d dml-arch -u dml --exec /bin/sleep 600` started from Windows and left
attached: distro alive for the full 60 s, then killed the holder — distro died
**15.0 s later**. Same timer, same reset semantics, one process instead of a
tick. This is what the comparison run had to do by hand to take any Arch sample
at all, and it is what `com.docker.backend` does for `docker-desktop`, which is
why Docker Desktop's distro never showed this behaviour in the same sitting.

Cost: one `wsl.exe` on Windows (a few MB) plus one `sleep` in the guest. No
user-owned state touched. **This is the mitigation that fits the product.**

### 7. `/etc/wsl.conf`, systemd units inside the distro — **CANNOT WORK**

The decision is made on the Windows side and executed by WSL's init. Nothing
inside the guest is consulted. A guest-side unit cannot refuse it: we watched
systemd stop `dockerd` cleanly on the way down, and the one guest-side thing
that *can* delay it — being slow to stop — merely converts a graceful poweroff
into `reboot(RB_POWER_OFF)` after 10 s, which is worse.

---

## What `restart: unless-stopped` does, and does not do

`~/games/wow-server-playerbots/docker-compose.yml` sets `restart: unless-stopped`
on 3 services. It does **not** keep the distro alive and does **not** prevent the
outage — it is a `dockerd` policy, and `dockerd` is stopped along with everything
else.

What it does do is make the failure **self-healing on next touch**, which is
arguably worse for diagnosis than a clean failure:

1. Launcher closes → 15 s later the distro powers off, containers stop cleanly.
2. The server is **down and unreachable** — for minutes, hours, or overnight.
3. The next time *anything* runs `wsl -d dml-arch …` (the user reopening the
   launcher), the distro boots, systemd starts `dockerd`, `unless-stopped`
   restarts the stack, and ~25 s later the world is up again.

So the user's experience is: *"my server is up whenever I look at it, and my
friends say it keeps going down."* Every check made through the launcher is a
check that repairs the thing it is checking. That is a nasty diagnostic shape and
it deserves to be named in the UI, not just fixed.

---

## What this means for the `Backend::Arch` decision

**The Arch backend cannot host a server unattended.** Not "may not on some
machines" — it did not, on eight consecutive measured runs, with a 200 ms
spread, and there is no setting that changes it. The moment the last `wsl.exe`
session into `dml-arch` exits, the server has 15 seconds to live.

That is a different class of problem from the resource question the comparison
was built to answer. A backend that saves 1.0–1.6 GB at idle but requires a
Windows process to be babysitting it is not competing with Docker Desktop on
RAM — Docker Desktop spends part of its ~590 MB of Windows-side processes doing
exactly this babysitting, on purpose. The Arch saving is, in part, **the cost of
the thing that keeps the server alive, not yet paid.**

It is fixable, cheaply, and the fix is measured to work (§4 and §6). But it is a
**required** piece of work, not an optimisation — and until it ships, "run your
server on the Arch backend" is not a claim the product can make.

### The honest scope of the fix

Even with the holder, the contract becomes *"the server runs while the launcher
runs."* Truly unattended hosting — server up with no DML process on Windows at
all — is **not achievable on the Arch backend by any mechanism found here**,
because something on Windows must hold the session. Docker Desktop achieves it
by running a background service the user installed for that purpose. If
"server stays up after I close everything" is a requirement, that is a point in
Docker Desktop's favour that no amount of launcher work erases; matching it
would mean shipping a Windows service or scheduled task, which is a much larger
product decision than a keep-alive.

---

## The launcher change (specification only — NOT implemented here)

**Where it belongs:** a new `launcher/src-tauri/src/wsl_keepalive.rs`, alongside
`tray.rs` / `single_instance.rs` / `autostart.rs` — launcher-owned, Windows-side
lifetime concerns. Not `dml-core` (this is not part of the CLI contract, and no
CLI caller needs it) and **not the frontend**.

**Why not the existing frontend poll, even though it measurably works:**
`startStatusPolling()` in `launcher/src/lib/server-status.svelte.ts` is a
`setInterval(…, 7000)` in a WebView2 webview. Three independent reasons it is
the wrong owner:

1. **Timer throttling.** Chromium-family engines throttle timers in hidden or
   occluded windows to as little as once per minute. The launcher's default
   close action **hides to tray** (`closeToTray`, `tray.rs`). So the exact
   scenario the fix exists for — window closed, server should keep running — is
   the scenario in which the 7 s timer is most likely to be stretched past the
   15 s deadline. A 7 s nominal interval with a 15 s budget has only a 2.1×
   margin and no guarantee it is honoured.
2. **It is a side effect, not a contract.** Nothing in that file says a server's
   life depends on the interval. Someone tuning the poll to 20 s to reduce load
   would kill servers, and every test would still pass. (Compare the recorded
   lesson about `lifecycle_steps_for_mode`: an invariant that lives only in a
   value production never reads is not pinned.)
3. **`if (!serverStatus.refreshing)` skips ticks.** A slow poll widens the gap by
   design.

**Behaviour to implement:**

* Hold exactly one child: `wsl.exe -d <distro> -u <user> --exec /bin/sleep infinity`.
  Use `--exec`, per the repo rule — and note the args are our own literals, so
  this is not the injection case, it is just the cheaper spawn.
* **Only on `Backend::Arch`.** Native/Docker Desktop needs nothing; spawning it
  there is a stray `wsl.exe` for no reason.
* **Start** when DML starts the server (and on launcher start if a status poll
  reports the stack already up — a server started by a previous session must be
  adopted, not orphaned).
* **Stop** when DML stops the server, on backend change, and on launcher exit.
* **Watchdog + respawn.** The child dies whenever the user runs `wsl --shutdown`,
  or WSL updates, or the distro is terminated. If the server is supposed to be
  up and the holder has exited, respawn it (bounded, with a logged warning).
  Silently not holding is the failure this whole document is about.
* **Never let it be the only reason the distro is alive after the server is
  meant to be down** — an orphaned holder pins ~1.4 GB of VM forever and would
  quietly undo the backend's only advantage. Its lifetime must be tied to the
  server's, both directions.

**Product/UI, and this part is not optional:** if the user exits the launcher
while an Arch-backend server is running, the product must either stop the server
cleanly first or say plainly that it is about to stop. A launcher that exits
silently and takes the server down 15 seconds later — after the user watched it
report "online" — is the same class of silent divergence as the moving image tag
and the single-signal bot detector: the UI's last word is true when spoken and
false immediately after, with nothing in the output naming the cause.

**Testing it:** the assertion must be that the distro is still `Running` some
interval **well past 15 s** after the last ordinary command — 60 s is a fair
gate — with the holder as the only thing keeping it alive. A test that merely
asserts "we spawned a process" would have passed against `wsl --list` polling,
which is measured here **not** to work.

---

## Provenance

Every number above is from a run taken on 2026-08-05 between 10:29 and 10:52
UTC on this machine, `dml-arch` idle (`systemd` + `dockerd` + the heartbeat
unit, **no game containers**), each run preceded by `wsl --shutdown`. The user's
AzerothCore stack was deliberately **not** used as the workload; the original
1,948-bot incident is quoted from the comparison run, not re-created.

Nothing forbidden was done: no `wsl --unregister`, no database write, no server
config change, no `docker system prune` / `rmi` / volume removal. `.wslconfig`
was modified three times to test mitigations, each time restored from a byte
backup taken first, each restore hash-verified against
`FAA1386CA79C7D6BF46FD6026E0BB1736B1EE007E1C2BFE211A260C4EE3EB273`.

Final machine state, verified: `dml-arch` Stopped, `docker-desktop` Stopped,
`vmmemWSL` absent, 0 Docker Desktop processes, `.wslconfig` byte-identical to
the original.
