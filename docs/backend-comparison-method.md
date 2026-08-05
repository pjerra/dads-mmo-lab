# Measuring Docker Desktop against Arch WSL — method

**Status:** harness built and validated 2026-08-05; **the run was taken the same
day** and the results are in [`docs/backend-comparison-2026-08.md`](backend-comparison-2026-08.md).
Numbers in *this* document are properties of the machine (read-only probes) or
illustrations of what a reading would mean — the measured answers are in the
results document, and where the two disagree the results document wins. Three
corrections it makes to this one:

* the "~740 MB of Windows-side processes" figure below measured **548.6–586.5 MB
  across 8 processes** on the day of the run (nine Desktop samples,
  `samples.csv` lines 3–11; an earlier version of this line said
  "551.8–594.2 MB", which was wrong at both ends — 594.2 appears in no record
  and 551.8 is not the minimum);
* `docker desktop stop` **exits the whole application** on Docker Desktop engine
  29.6.2, so the "running, engine stopped" row of the three-state table costs
  **0 MB**, not ~740 MB, by that route;
* `dml-arch` does **not stay running unattended**. WSL terminated it ~25 s after
  the last `wsl.exe` client exited — once with 1,948 bots live — so every Arch
  sample needs a holder process, and the run procedure in Part C below is
  incomplete without one.
* the run's **boot timing was never sampled**: all 20 records carry
  `"ready": null` because `-TimeReady` was not run. It was re-measured
  separately on 2026-08-05 (Desktop **33.1 s**, Arch **23.5 s**, T0 to the log
  marker); see the results document. Note for anyone using `-TimeReady` next
  time that its `MarkerSeconds` is **poll-derived on a 5 s loop**, not taken
  from the log line's own timestamp.

**What is being compared.** The same AzerothCore + playerbots stack, two ways:

* **Docker Desktop** on Windows — server at `C:\Users\perzi\dml-native`.
  Desktop runs Windows-side processes *and* its own `docker-desktop` WSL distro.
* **Arch WSL** — the `dml-arch` distro running its own `dockerd` under systemd,
  server inside the distro at `~/games/wow-server-playerbots`. No Docker Desktop.

The hypothesis under test is the user's: that the second saves substantial
resources. The metric list comes from
`docs/superpowers/specs/2026-08-04-arch-wsl-backend-design.md` — idle RAM, RAM
with the server up and bots online, launcher-open to world-ready, disk, and RAM
returned after stop.

---

## The tools

| File | Job |
|---|---|
| `scripts/Measure-BackendFootprint.ps1` | Takes ONE sample of ONE backend at ONE phase. Refuses when the machine state does not match what the sample claims. Writes JSON + appends CSV. |
| `scripts/Compare-BackendFootprint.ps1` | Reads the samples, subtracts baselines, prints the comparison, and flags every reading whose provenance is bad. |

Both are read-only with respect to the machine. **Neither starts or stops a
server, an engine, a distro or the WSL VM.** Changing state is the operator's
job; the harness's job is to refuse when the state is wrong, so a mixed reading
is never filed as a clean one.

Self-check, any time, touches nothing:

```powershell
powershell -File scripts\Measure-BackendFootprint.ps1 -SelfTest
```

That runs 12 precondition cases plus 2 process-name checks against the **real**
`Test-Precondition` function using synthetic machine states. It exists because
the states that matter most cannot be produced on demand — proving that a stray
distro triggers a refusal would otherwise mean booting one, and proving the
"engine phase but the server auto-started" refusal would mean starting the
user's server.

The suite was validated by mutation, per the repo's anti-vacuity rule: deleting
the stray-distro refusal turned exactly one case red; making preconditions never
refuse turned 9 of 12 red while the 3 clean-machine cases stayed green (so those
genuinely test the no-false-refusal direction); and restoring a `vmmem*` wildcard
turned both process-name checks red.

---

## The measurement problems, and what was done about each

These are the hard part. Getting them wrong produces confident nonsense.

### 1. WSL2 memory is not what Task Manager suggests

**The problem.** All WSL distros share **one utility VM with one Linux kernel**.
The Windows-visible process is `vmmemWSL`, and it reports the whole VM. It is
not per-distro, and it does not shrink promptly when a process inside frees
memory.

**Why the obvious fix fails.** The natural instinct is to ask each distro what
it is using — `/proc/meminfo` or `free -m` inside `dml-arch`. That does not
work: distros are PID and mount namespaces inside one kernel, and `/proc/meminfo`
is not namespaced. Every distro reports the **whole VM's** memory. Two distros
running at once cannot be told apart from Windows *or* from inside.

**What can be attributed and what cannot:**

| Quantity | Attributable to a backend? |
|---|---|
| `vmmemWSL` working set / private bytes | **Only if that backend's distro is the only one running.** Otherwise it is a sum with no split. |
| Per-process RSS inside a distro | Partially — a distro's PID namespace shows only its own processes, but this misses page cache, kernel slab and tmpfs, which are VM-global. An undercount of unknown size. |
| Machine-level memory in use | Yes, against an `off` baseline on a quiet box. Includes everything else running, which is why the box must be quiet. |
| Kernel's share within the VM | **No.** Real memory the backend causes to be used, belonging to a shared kernel. |

**Resolution.** The harness does not invent a split. It **refuses to sample when
more than one distro is running**, which makes the single VM figure honestly
attributable, and it records the fact in the sample's caveats. The headline
number is deliberately the *machine-level* one — how much of the box's 31.9 GB is
in use — because that sidesteps the per-process attribution argument entirely.
Summing Docker Desktop's Windows processes and `vmmemWSL` would double-count some
pages and miss others.

### 2. `vmmem*` is not one process family

This machine also runs **`vmmemCmZygote`** (806 MB private, no readable start
time, no executable path) — a Windows container-manager VM with nothing to do
with WSL. A `vmmem*` wildcard silently adds it to whichever backend is under
test. The harness matches `vmmemWSL` and `vmmem` **by exact name**, and the
self-test fails if a wildcard or `vmmemCmZygote` ever appears in that list.

### 3. Docker Desktop's footprint is split, and lands in the same VM

Desktop's cost is in two places: Windows-side processes (measured on this box at
**~740 MB working set across 8 processes** while its engine was *stopped*), and
its own `docker-desktop` distro — **which lives in the same shared utility VM as
`dml-arch`**. So `vmmemWSL` can contain both backends at once.

**Resolution.** A clean reading requires the other side genuinely off, defined
below, and enforced by the precondition check.

### 4. "Off" has to mean off, and Desktop has three states

`docker desktop status` reported `Status: running` on this box at a moment when
the `docker-desktop` distro was `Stopped` and no engine could serve a request.
**It reports the app, not the engine.** Treating it as an engine probe certifies
a dead engine as live.

The three states, and how the harness tells them apart:

| State | Windows processes | `docker-desktop` distro |
|---|---|---|
| Fully exited | absent | Stopped |
| Running, engine stopped | **present (~740 MB)** | Stopped |
| Running, engine up | present | Running |

So:

* **Desktop is off** = no Docker Desktop Windows processes at all *and* the
  `docker-desktop` distro is `Stopped`. Quitting from the tray, not just
  stopping the engine. For an Arch reading the middle state is not good enough —
  it is 740 MB of the very thing being measured.
* **Arch is off** = `dml-arch` is `Stopped` in `wsl --list --verbose`.

### 5. Only one backend can host a server at a time

Both stacks publish 3724 / 8085 / 7878 and both use the engine-global `ac-*`
container names. WSL2 localhost forwarding puts the distro's published ports on
the Windows `127.0.0.1`, so they collide across backends too. The harness
encodes this as a **refusal**, not a warning: the other backend's distro running
is a violation, and so is a phase whose port state contradicts its label.

### 6. `restart: unless-stopped` can start the server behind your back

Read from the live compose at `C:\Users\perzi\dml-native\wow-server-playerbots\docker-compose.yml`:
`ac-database`, `ac-authserver` and `ac-worldserver` all carry
`restart: unless-stopped`.

Consequences:

* Starting the engine can bring the **whole server** up by itself. An `engine`
  sample taken then is a `server` sample wearing the wrong label — and it would
  understate the difference between the backends by exactly the amount the
  comparison is trying to find.
* On Arch, **merely booting the distro** starts `dockerd` via systemd, which
  restarts those containers. `wsl -d dml-arch -- anything` is therefore not a
  neutral act; it can start the user's server.

The harness refuses `engine` and `off` phases when any stack port is open, and
refuses `server` when none is.

### 7. Querying docker changes what you are measuring

Measured on this box: a read-only `docker context ls` **woke the stopped
`docker-desktop` distro and created a fresh WSL VM** (`vmmemWSL` PID changed
411484 → 436912). Docker queries are therefore **opt-in** (`-QueryDocker`), off
by default, and the sample records a caveat when used.

### 8. `.wslconfig` changes what every RAM number means

This machine's `%USERPROFILE%\.wslconfig`:

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

Three consequences, all load-bearing:

* **`memory=16GB` is a cap.** The VM can never report more, on either backend.
  A loaded reading at or near 16 GB is **the cap, not the demand** — the
  workload may want more and be unable to take it. It also means a genuine
  difference in appetite above 16 GB is invisible to this comparison.
* **`autoMemoryReclaim=gradual`** returns freed guest memory to Windows
  gradually, on an undocumented schedule. Any single post-stop sample measures
  the reclaim schedule as much as the workload.
* **`vmIdleTimeout=60000`** tears the VM down 60 s after the last distro stops.
  Observed live: `vmmemWSL` vanished entirely between two probes. This is the
  *only* state in which "all memory returned" is literally true, and it is what
  makes `wsl --shutdown` between backends an effective reset.

The harness reads `.wslconfig` into every sample and emits the matching caveats
automatically, so a reading can never be quoted without them.

### 9. Timing: the cheap readiness probe lies

TCP 8085 accepting a connection is **necessary but not sufficient**. Docker's
userland proxy binds and accepts the moment the container starts, long before
the worldserver has loaded — taken alone it would report a 30-minute boot as a
5-second one.

The authoritative probe is the product's own definition, mirrored from
`crates/dml-wow/src/status.rs::world_ready`: `docker inspect -f
'{{.State.StartedAt}}' ac-worldserver`, then `docker logs --since <started>
ac-worldserver`, scanning case-insensitively for `world initialized in`. The
harness records **both** times so the gap is visible, and marks a TCP-only
result as not authoritative.

**Both of the harness's own figures are poll-derived**, on a 5 s sleep loop, and
`-TimeReady` does not start the server — it prints "Trigger the start NOW" and
waits, so T0 and the start are separate acts. A figure that must be
*timestamp*-derived has to come from `docker logs -t` and the marker line's own
timestamp, which is how the 2026-08-05 re-measurement was taken.

### 10. Disk: the two sides are not like for like

Measured now (logical `.vhdx` sizes, read-only):

| Store | Size |
|---|---|
| `dml-arch` — `C:\DML\wsl\ext4.vhdx` | **87.21 GB** |
| `docker-desktop` — `...\Docker\wsl\main\ext4.vhdx` | 0.09 GB |
| Docker Desktop image store — `...\Docker\wsl\docker_data.vhdx` | **27.49 GB** |
| `dml-arch-gate2` (throwaway) | 1.30 GB |

**Reporting 87 GB against 27 GB would be a lie.** `dml-arch`'s single vhdx holds
the OS, the docker image store, the build cache *and* the server directory.
Docker Desktop's image store is the separate `docker_data.vhdx`, and its server
directory (`C:\Users\perzi\dml-native`, which includes a 3 GB `client-data.tar`
and ~700 MB of image tarballs) sits on the Windows filesystem, outside every
figure in that table.

Also: **a `.vhdx` never shrinks.** It is a high-water mark — an honest answer to
"what has this cost me" and a dishonest one to "what is it using now".

**Resolution.** The comparable number is `docker system df` from each engine
(image store + build cache, the engine's own logical view). The vhdx figures are
reported separately, labelled as high-water marks, with the caveat printed
alongside them by `Compare-BackendFootprint.ps1`.

The registry (`HKCU:\Software\...\Lxss`) is the source for distro paths, never a
path convention: `docker-desktop`'s `BasePath` carries a `\\?\` long-path prefix
and `dml-arch` lives at `C:\DML\wsl`, neither of which a convention would find.

### 11. `wsl.exe` output is UTF-16LE

Every naive parse of `wsl --list --verbose` fails — PowerShell renders it with a
NUL between every character. The harness sets `WSL_UTF8=1` (WSL 0.64+; this box
runs 2.7.10), which makes it emit UTF-8. It also parses state and version as the
**last two columns** so a distro name containing spaces survives.

A failed `wsl` probe is treated as **"could not answer"**, never as "nothing is
running" — the tri-state discipline the repo uses everywhere. A caller that
reads a failed probe as an empty list will happily certify a contaminated
machine as clean; self-test case 11 pins this.

---

## Which metrics are honestly measurable

| Metric | Verdict |
|---|---|
| **Idle RAM (engine up, no server)** | **Measurable.** Machine-level delta over an `off` baseline, plus the VM figure attributable because everything else is off, plus Desktop's Windows-side processes. |
| **RAM with server up + bots online** | **Measurable but capped.** `memory=16GB` truncates any demand above it, identically on both backends. Bot count is operator-reported, not measured. |
| **Launcher-open to world-ready** | **Measurable** via the log marker. The `-TimeReady` phase times from T0 to the marker; "launcher open" is the operator's T0. |
| **Disk** | **Measurable only as `docker system df`.** The vhdx totals are high-water marks over different contents and must not be compared directly. |
| **RAM returned after stop** | **NOT honestly measurable as a difference between the backends.** See below. |

### Why "RAM returned after stop" is not a real comparison

The WSL utility VM does not hand memory back to Windows promptly. With
`autoMemoryReclaim=gradual` it does so on an undocumented schedule; without it,
it does not do so at all while the VM lives. Either way:

* a low figure is a property of **WSL**, shared by both backends, not evidence
  that one leaks and the other does not;
* the number you get depends mostly on how long you waited;
* the only state in which all memory is provably returned is the one where the
  VM process is **gone** — which `vmIdleTimeout=60000` reaches on both backends
  60 s after the last distro stops.

The harness therefore samples this phase as a **series** (`-Repeat 6`, 30 s
apart by default) and `Compare-BackendFootprint.ps1` prints the curve with that
caveat attached. **Report the curve, or report that it is not a differentiator.
Do not report a single number.** Fabricating one would be worse than none.

One thing here *is* a genuine, reportable difference: Docker Desktop's ~740 MB of
Windows-side processes persist after its engine is stopped and only go away when
the user quits the app. Arch has no equivalent — its Windows-side plumbing
measured **23 MB**.

---

## Run procedure

### Preconditions for the whole sitting

1. **The machine must be quiet.** No builds, no agents, no other work. During
   harness development, `Committed Bytes` sat at 51.7 GB with 5 GB available
   because sibling agents were compiling; measurements taken then are garbage.
2. **Know which distros exist.** They change: over a two-hour window this box
   went from four registered distros to three, while another process created a
   fifth. Always run `wsl --list --verbose` and confirm before trusting a number.
3. Close the launcher, the WoW client, and any terminal sitting inside a distro.

### Ordering

**Do all Desktop measurements, then all Arch measurements.** Do not alternate.
Each switch costs a full `wsl --shutdown`, a settle, and an engine cold start;
alternating multiplies that by the number of phases and adds a contamination
risk at every crossing.

### Part A — Docker Desktop

```powershell
cd C:\Users\perzi\dads-mmo-lab

# A0. Everything off. Quit Docker Desktop from the tray (not just "stop engine").
wsl --shutdown
#    wait 60 s, then confirm the VM is gone:
Get-Process vmmemWSL -ErrorAction SilentlyContinue   # expect: nothing
wsl --list --verbose                                  # expect: all Stopped

powershell -File scripts\Measure-BackendFootprint.ps1 -Backend desktop -Phase off -Label run1

# A1. Start Docker Desktop. Wait for the engine (the docker-desktop distro must
#     read Running). If the server auto-starts (restart: unless-stopped), stop
#     it before sampling - the harness will refuse otherwise, which is the point.
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend desktop -Phase engine -Label run1

# A2. Time a cold start. Run this FIRST, then trigger the start from the launcher.
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend desktop -TimeReady -QueryDocker -Label run1

# A3. Server up, bots online. Check the bot count in the launcher and pass it.
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend desktop -Phase server -QueryDocker -BotsOnline 500 -Label run1

# A4. Disk, while the engine is still up.
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend desktop -Phase disk -QueryDocker -Label run1

# A5. Stop the server (launcher, or `docker compose down` in the server dir).
#     Leave the ENGINE running - this phase asks what the server gave back.
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend desktop -Phase stopped -Repeat 6 -IntervalSeconds 30 -Label run1
```

### Part B — the crossing

```powershell
# Quit Docker Desktop from the tray. Then:
wsl --shutdown
#    wait 60 s and CONFIRM - this is the step that keeps the two halves independent:
Get-Process vmmemWSL -ErrorAction SilentlyContinue   # expect: nothing
Get-Process -Name 'com.docker.backend','Docker Desktop' -ErrorAction SilentlyContinue  # expect: nothing
```

Skipping this puts both backends in one VM generation. `Compare-BackendFootprint.ps1`
detects it and flags `SHARED VM GENERATION`, but the run is then wasted.

> **NEVER run `wsl --shutdown` or `wsl --terminate` while other work is using a
> distro** — it kills everything inside every distro. Only during a quiet sitting.

### Part C — Arch WSL

```powershell
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend arch -Phase off -Label run1

# C1. Start the distro. NB this starts dockerd via systemd, which may restart
#     `unless-stopped` containers - i.e. it can start the server. Check, and
#     stop the stack if so, before sampling 'engine'.
wsl -d dml-arch -u dml --exec true
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend arch -Phase engine -Label run1

powershell -File scripts\Measure-BackendFootprint.ps1 -Backend arch -TimeReady -QueryDocker -Label run1
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend arch -Phase server -QueryDocker -BotsOnline 500 -Label run1
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend arch -Phase disk -QueryDocker -Label run1
powershell -File scripts\Measure-BackendFootprint.ps1 -Backend arch -Phase stopped -Repeat 6 -IntervalSeconds 30 -Label run1
```

### Part D — report

```powershell
powershell -File scripts\Compare-BackendFootprint.ps1 -Label run1 -Markdown
```

Read the **Provenance** section first. If it reports forced samples, a shared VM
generation, or a missing baseline, fix and re-run rather than publishing.

### Settling delays

Defaults, overridable with `-SettleSeconds`:

| Phase | Settle | Why |
|---|---|---|
| `off` | 60 s | matches `vmIdleTimeout`, so the VM has actually gone |
| `engine` | 60 s | engine start churns for ~30 s |
| `server` | 120 s | bot logins and world loading continue after "ready" |
| `stopped` | 30 s, then 6 samples 30 s apart | the reclaim curve is the answer here |
| `disk` | 5 s | nothing to settle |

### Output

`results/backend-comparison/` (gitignored): one JSON per run plus an appended
`samples.csv`, so two runs diff without retyping. Each JSON carries the sample,
the `.wslconfig` in force, the precondition verdict, the machine-generated
caveats, and the explicit `notMeasured` list.

---

## The single biggest threat to validity

**A busy machine.** Everything else here is detected and refused; this one is
not, and it is silent.

The harness cannot tell a 2 GB compile from a 2 GB backend. Its headline metric
is machine-level memory, which includes every other process on the box, and its
defence is the `off` baseline — which only works if the noise is *constant*
across the sitting. It is not: a background build that starts between the
Desktop half and the Arch half lands entirely in one column and looks exactly
like a real difference.

Mitigations, in order of value: run on a genuinely idle box; take both halves in
one sitting; keep `-Label` per sitting so `Compare` never mixes them; and treat
any Desktop-vs-Arch gap smaller than a couple of hundred MB as noise rather than
signal. If the two halves cannot be taken close together, say so in the results
document.

Runner-up: **the 16 GB cap**. If the loaded server genuinely wants more than
16 GB on either backend, both columns read ~16 GB and the comparison silently
reports a tie where a real difference exists.
