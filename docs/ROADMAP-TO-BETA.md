# ROADMAP TO BETA

**The single ordered path from where the project is today to v0.1.0 in a
stranger's hands.** Written 2026-08-01 from a full read of the tracking docs, not
from memory — every "done" below is verified by commit or by a file on disk.

Two documents already exist and neither is this one:

* [`SHIP-LIST.md`](SHIP-LIST.md) is the *release-discipline* doc — the rule that
  no new features ship before the release gate, plus the incident record.
* [`superpowers/plans/2026-07-20-post-smoke-roadmap.md`](superpowers/plans/2026-07-20-post-smoke-roadmap.md)
  is the *everything-ever-asked-for* backlog, deliberately exhaustive.

This file is the short one: **what is actually in the way of the beta, in
order.** If an item is not here, it is not blocking the beta.

---

## THE DECISION THIS ROADMAP RESTS ON

**The beta waits for native** (user, 2026-08-01, reversing the WSL-only scope of
2026-07-30). The reversal is justified by evidence, not optimism: the first
end-to-end native install ran on real hardware on 2026-07-31 — **8/8 stages, exit
0, 21m18s**, with `ac-worldserver` and `ac-database` up and healthy.

What is left is **integration, not architecture**. The engine works and is
proven. Nothing in the launcher can reach it.

The user also wants `dml-arch` **retired**. Docker Desktop's own distro cannot
host the title installers (Alpine, no sudo, no systemd, rebuilt on upgrade), so
native IS the retirement path — there is no shortcut through the existing distro.

---

## 🔴 PHASE A — the three sharp edges (small, and they are ours)

Deliberately first. Each is small, each is already understood, and each one
undermines a gate we are about to lean on. Doing them first means Phase B is
built on instruments we trust.

### A1 — Resume after a generator fix silently reuses the stale file

Found live on 2026-07-31 and it cost real time. `generate-compose` had recorded
itself done, so re-running the install after fixing the generator **reused the
old broken output** and skipped the fix. The state file had to be deleted by
hand.

A stranger hitting this gets a build that fails for a reason we already fixed,
with no way to know that. Resume is a headline feature of the install engine;
resume that silently serves stale generated output is worse than no resume,
because it lies about what it did.

### A2 — Nothing caps build parallelism — ✅ DONE 2026-08-01

Preflight warned *"8 CPUs but only 15.6 GB — room for 7 jobs, not 8. Nothing caps
build parallelism for you here."* Honest, and inert.

**The investigation changed the answer.** Reading the PINNED upstream Dockerfile
(`apps/docker/Dockerfile`, the whole point of pinning) settled the native plan's
open Task 1 research question:

```
&& cmake --build . --config "$CTYPE" -j $(($(nproc) + 1))
```

* The job count is **hardcoded inside the `RUN`**, not an `ARG`, so no
  `--build-arg` can change it. `ARG CMAKE_EXTRA_OPTIONS` is declared and then
  never referenced — a dead option, so not an injection point either.
  `cmake --build` honours `CMAKE_BUILD_PARALLEL_LEVEL` only when `-j` is absent,
  and here it never is. **There is no knob. The Docker VM's CPU count is the
  only lever that exists.**
* It is `nproc` **+ 1**. Every piece of sizing advice in this project was
  computed against `nproc`, understating the peak by one whole compiler
  (~2 GB) — and understating it exactly on the machines already tight enough to
  care.

So the deliverable was not a cap (impossible) but **arithmetic the user can act
on**, which is the entire mitigation available. The off-by-one had left the worst
case silent: a VM with exactly as many CPUs as its RAM can feed (4 CPUs / 8 GB)
got **no warning at all** while the build ran 5 concurrent compilers against room
for 4. Preflight now counts what upstream really starts, names what those jobs
need, and advises a CPU number that is one *below* the job count — advising the
job count itself re-created the very overcommit the warning exists to prevent.
Both halves mutation-proven.

**Known, recorded, NOT fixed:** `Install-DML.ps1`'s WSL sizing carries the same
wrong assumption in its comment ("one C++ compiler per core") and its
`memBoundCores` formula. Its impact is heavily buffered — `min(4, hostCores,
memBoundCores)` dominates on any normal machine and the 8 GB swap floor catches
the rest — and that file is explicitly flagged as dangerous to edit casually
(it embeds an old CLI as a here-string; installer↔CLI sync is its own planned
job). Fix it there, with its 128-check harness, not in passing.

### A3 — `cli/tests/soap.bats` test 6 flakes — ⚠️ NOT REPRODUCED, hazards closed

**The flake did not reproduce**: 15 isolated runs and 3 full-suite runs, all
clean. It stays open, and nothing may be claimed as its fix.

**The recorded hypothesis is disproved, which is worth more than a guess.** It
read "likely cross-test contamination of `~/.dml/soap.env`". It cannot be:
`setup()` exports `HOME="$FIXTURE"`, a fresh `mktemp -d` per **test**, so the
directories tests 2 and 3 write into no longer exist when test 6 runs. Left
standing, that note would have sent the next investigation somewhere there is
nothing to find.

Two REAL hazards of the same class were found and closed:

* **`build.sh` published a torn file.** `cat src/*.sh > dml` truncates instantly
  and fills over milliseconds, so a concurrent reader sees a half-file — the
  documented cause of ~450 fake failures when bats overlapped the cargo parity
  suites. Now temp-file + atomic `mv`. Same change fixed a second bug in the same
  four lines: the parse check ran *after* the redirect, so a syntax error in
  `src/` was **published first and reported second**.
* **Stub counters defaulted to `/tmp/dml_<kind>_seq.$$`** — shared `/tmp`, keyed
  by the stub's own pid. `$$` is a new pid per invocation (counter never
  advances → sequence silently replays entry one → a test that proves nothing
  while passing), and pids are recycled (a leftover file from any earlier run is
  read as this run's progress). Unreached today because every test sets its own
  state path — which is exactly why it was worth closing before someone forgot.

New `cli/tests/build-script.bats` (5 tests) pins the build script. Mutation-proven:
restoring the old `build.sh` reddens 3 of 5.

**Still true and still the reason this mattered:** an intermittently red suite is
indistinguishable from a real regression. If test 6 flakes again, capture the
full output — the harness can no longer be the cause of the two mechanisms above.

---

## 🔴 PHASE B — make native reachable (the actual beta blocker)

Plan: [`2026-07-29-native-first-install.md`](superpowers/plans/2026-07-29-native-first-install.md),
14 tasks. Six are done.

| # | Task | State |
|---|---|---|
| 1 | Pin the upstream build contract | ✅ `0dbfd3f` — core `190184a0`, module `ba46fcde`, **verified** after checkout |
| 2 | Compose/override generation | ✅ `e354cb5` |
| 3 | Native install engine (staged, resumable, NDJSON) | ✅ proven live |
| 4 | Honest hardware preflight | ✅ `e354cb5` |
| 5 | `dml-wow install-native` CLI surface | ✅ |
| 9 | Migration scripts fixed to match their own lessons | ✅ `e354cb5` |
| **6** | **Launcher wiring** | ❌ **THE ONE THAT MATTERS** |
| **8** | **`Install-DML-Native.ps1`** | ❌ not started |
| **11** | **Account + SOAP bootstrap** | ❌ not started |
| 7 | Port guard armed on native start | ⚠️ **armed, but as a WARNING where the plan specified a REFUSAL** — see below |
| 10 | `migrate-import` | ❌ not started |
| 12, 13 | LIVE gates — real build, kill-mid-build resume, first login | 🙋 user |
| 14 | Docs + doctrine reconciliation | partial |

### B1 — Task 6: launcher wiring — *the single highest-value item in the repo*

Today `install-native` is reachable **only by running the binary from a
terminal**. Three specific things are missing:

1. `games_install_native` Tauri command streaming engine events over the existing
   `Channel<Value>` terminal plumbing, sharing the `InstallSlot` busy-guard.
2. **Backend auto-detect can never select native on a fresh machine.**
   `backend.rs:51-56` requires a server directory that only the install creates —
   so a new user can never reach the installer that would create it.
3. Native first-run is suppressed: `first-run.ts:300` returns `null` for native.

Plus a Resume button when a title dir carries `.dml-install.json`, and cancel
copy that tells the truth (cancel is `taskkill /F /T`; resumability is the state
file + the BuildKit cache, not process suspension).

### B2 — Task 8: `Install-DML-Native.ps1`

The WSL-free machine installer. `Install-DML.ps1` stays untouched as the WSL
route. This one: detect-or-install Docker Desktop, install Git for Windows
(native mode hard-requires it), pinned `yq.exe` + SHA256, write
`~/.dml/launcher.json` with `backend=native`, apply the existing directory-scoped
Defender exclusions **before** any build runs, `-DryRun`.

Explicitly **no** C# tray, **no** WSL features, **no** Arch import — which
resolves SHIP-LIST 4.0b ("two launchers and a stranger cannot tell them apart")
by construction rather than by patch.

### B3 — Task 11: account + SOAP bootstrap

After readiness, a guided worldserver-console step that creates the `dmlsoap` GM3
account and then **verifies a real SOAP round-trip before the flow may declare
the install done**. A skipped step leaves every SOAP feature dead with no cause
shown, so "done" must be earned. Deliberately not automated — that would need an
SRP6 `INSERT` into `acore_auth`, a new sanctioned-write class only the user can
approve.

### B4 — Task 7: the guard exists, but not as specified — DECIDE

**Verified by reading, 2026-08-01.** `check_port_conflicts` IS armed on native
start ([`lifecycle.rs`](../crates/dml-wow/src/lifecycle.rs) in
`games_lifecycle_stream_with`), with the real `port_listening` prober, and
deliberately on cold starts only — on a restart the ports are held by this
server's own containers, so checking would cry wolf. That part is right.

**But it WARNS. The plan specified a REFUSAL**, and explicitly considered and
rejected the advisory-only variant: *"a start that will lose the port race and
crash-loop is exactly what the boot-loop watch would then diagnose — refusing
earlier is the honest surface."*

This is a genuine plan-vs-implementation divergence and it is a user-visible
behaviour change either way, so it is a decision rather than a bug to quietly
fix. The case for keeping the warning: 3306 already has an automatic remedy (the
`.env` `DOCKER_DB_EXTERNAL_PORT` write), and a refusal that fires on a prober
that merely failed to answer would block a start for no reason — the tri-state
rule the plan itself insists on. The case for the refusal: with `ac-*` container
names global to the docker engine, a second stack cannot work, and letting it
crash-loop spends the user's time to reach the same conclusion.

Leaning: **refuse for the three game ports, keep 3306 advisory** (it is the one
with a real remedy), and never refuse on an unanswerable probe.

### B4b — Install the OTHER WoW titles by running their `.sh` in a WSL distro (user decision, 2026-08-01)

**Scope narrowed by the user: the launcher installs WoW servers only.** MapleStory,
RuneScape and Mu Online are dropped from the install surface.

**Question asked: can Docker Desktop's own WSL distro run the installers?**
**Answer, probed live rather than recalled: no — and the alternative is better.**

| distro | bash | sudo | systemd | docker | verdict |
|---|---|---|---|---|---|
| `docker-desktop` | ❌ absent | ❌ | ❌ | — | cannot run a bash script at all |
| `Ubuntu` 26.04 | ✅ | ✅ | ✅ pid 1 | ✅ **`docker ps` works** | can run them |

Three verified facts make this work, and each one was checked, not assumed:

1. **Docker Desktop's WSL integration means a script in Ubuntu drives the REAL
   engine.** `docker ps` succeeds and `docker compose version` reports v5.3.1
   there, so containers an installer creates are the same ones native mode
   manages afterwards. That is the whole trick — no docker-in-docker, no socket
   forwarding.
2. **No sudo is needed.** `install_docker()` in
   `guides/wow-wotlk/install-wow-wotlk-ubuntu.sh` returns 0 immediately when
   docker and the compose plugin are both present, and EVERY `sudo` in that
   script sits inside the path that check skips. The sudo-password problem that
   would otherwise sink this simply does not arise.
3. **Vanilla and TBC are CMaNGOS, not AzerothCore** (`cmangos/mangos-classic`,
   `cmangos/mangos-tbc`, `cmangos/playerbots`). Different build, different DB,
   different containers — so `install_native.rs`, which is AzerothCore-shaped
   throughout, does NOT generalise to them. Running their scripts is days;
   re-deriving CMaNGOS knowledge in Rust is weeks. **Running the scripts is the
   correct answer here, not the lazy one.**

The resulting shape is a hybrid, and it is the honest one:

| title | how |
|---|---|
| WotLK Playerbots | native Rust engine — already proven |
| Wrath Unbound | `.sh` in a distro (uses NO package manager at all — cleanest case) |
| Vanilla (CMaNGOS) | `.sh` in a distro — needs an apt path, currently pacman-first |
| TBC (CMaNGOS) | `.sh` in a distro — same |

**THIS RETIRES `dml-arch`**, which was the point: nothing would run inside it.

**Distro choice (user decision): DETECT.** Probe every installed distro for
bash + a reachable docker, use the first that passes, and import a DML-owned
Ubuntu only when none does. Rejected alternatives and why: writing into the
user's personal Ubuntu is not reproducible for a stranger and is rude to their
machine; always creating a DML distro pays a first-run cost even when a perfectly
good one exists, and Docker Desktop's WSL integration is OFF by default for a new
distro, so that path also needs a detect-and-instruct step for the toggle.
Detection subsumes both. Roughly one extra day over the simplest option.

Two things this must not repeat from the WSL era: the probe is TRI-STATE (a
distro that fails to answer is evidence of nothing, not evidence of absence), and
anything passing a host path through `wsl.exe` uses `--exec` with real argv, per
the recorded metacharacter bug.

### B5 — Task 10 (`migrate-import`)

The "bring your existing server across" path. Genuinely valuable and genuinely
optional for a beta — a new user installs fresh.

---

## 🙋 PHASE C — the gates only the user can run

No amount of engineering closes these. They are the release standard.

- **Task 12 — LIVE native gate.** Fresh build in a scratch `DML_GAMES_DIR`,
  **kill the launcher mid-build**, reopen, Resume must continue cache-warm rather
  than from zero. Then a real WoW client login. Then the same on a VM that has
  never seen DML.
- **Task 13 — LIVE migration gate**, if Task 10 ships: real export/import, the
  identity check (2505 characters, 255 accounts), then the negative test — with
  one server running the other must refuse on ports.
- **The five outstanding live gates** already listed in SHIP-LIST Phase 1.
- **The Desktop-icon check** — run the launcher installer: is there an icon, does
  the Start menu open the new app (not the retired tray)?
- **Three tray-design answers** (these block the multi-server work, not the
  beta): where does a server's display name live; does the tray list every server
  or follow one active server; does Home follow the active server or stay
  WoW-only?

---

## 🟢 PHASE D — release

From SHIP-LIST Phase 5, unchanged and still correct:

- Get **one other human** to run it before any public post.
- Tag `v0.1.0` on `rust-main`, `npm run tauri build`, attach the installers to a
  GitHub Release with honest notes.
- Unsigned → SmartScreen warning. Say so in the notes rather than hoping.
- Call it beta and mean it: ask for bug reports, not stars.

---

## ⛔ EXPLICITLY NOT BEFORE THE BETA

Recorded here so they stop competing for attention. All are real, all are filed,
none of them block a release.

| Item | Where it lives |
|---|---|
| **Keira3 integration** | roadmap Round 5.5 — **not scoped**; it is a bulk world-DB *writer* and the standing posture is read-only MySQL. That decision is task one, not an implementation detail. |
| **Perf Advisor** | spec `2026-07-27-perf-advisor-design.md` — plan validated, but **2 Criticals first**: the box now runs `MapUpdate.Threads=3` and can no longer reproduce its own acceptance diagnosis, and the stats JSON shape is pinned in three bash places, not one |
| **Console always-on stream** | last open item of `2026-07-25-common-writes-to-rust.md` (that plan is otherwise ~90% done — see its corrected header) |
| **13-item smoke-test feature batch** | roadmap Round 2.5 — backup toggles, realmlist picker, zone-grouped teleports, pinned backups, bots log-out/in, module sorting… |
| **Addon-from-URL** | roadmap Round 2.5 — both halves exist, needs joining + untrusted-archive defenses |
| **Per-install container names** | roadmap — the prerequisite for the multi-server tray; `ac-*` names are global to the docker engine, so one stack per PC today |
| **Promote the dml CLI out of the installer here-doc** | a fresh `Install-DML.ps1` still ships CLI v2.6.0 against the repo's v3.0.0 |
| **Kill the bash CLI** | SHIP-LIST Phase 6 — next month's job, explicitly after 1–5 |

---

## HOW TO KEEP THIS FILE HONEST

Three failure modes this project has already hit, all of them cheap to avoid:

1. **A decision that lives only in a conversation is a decision that gets lost.**
   File it the same day. `.superpowers/` is gitignored and does not count.
2. **A recovered plan is not a finished plan — and a finished plan is not an
   outstanding one.** On 2026-07-30 a completed subsystem was committed with a
   "do NOT start this" header, five days after it shipped. Before deferring a
   recovered plan, check `git log` for its own commits.
3. **A test that cannot fail is not a test.** After writing one that overrides a
   shared fixture, mutate the production code and watch it go red.
