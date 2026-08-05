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

## SCOPE, FIXED BY THE USER 2026-08-01

**v0.1.0 ships WoW Playerbots on the native backend, WITH the Wrath Unbound
add-on. Vanilla and TBC move to v0.2.**

> **AMENDED 2026-08-03 (user): Wrath Unbound is IN the v0.1.0 cut.** The
> original deferral was justified by Unbound needing the `.sh`-in-a-distro
> runner (B4b); it was ported to a native staged engine on 2026-08-02 and needs
> neither. Its live gate is therefore part of the release standard now — Phase C
> gains an Unbound install/uninstall run. Vanilla and TBC are unaffected and
> still wait for B4b, which is what the rest of this section is about.

Vanilla and TBC move to **v0.2**. They need the
`.sh`-in-a-detected-distro runner (B4b), which is roughly a week: distro
detection, streaming a Linux installer through the terminal, and an apt path for
the two pacman-first scripts. Playerbots native is built and click-verified
today, so holding the release for three more titles trades a working thing in a
tester's hands for a longer wait.

Two further decisions taken the same day:

* **Port guard: REFUSE on 3724 / 8085 / 7878, keep 3306 advisory.** Those three
  have no remedy — the `ac-*` container names are global to the docker engine, so
  a second stack simply cannot work, and letting it crash-loop only spends the
  user's time arriving at the same answer. 3306 keeps warning because it HAS an
  automatic fix (the `.env` `DOCKER_DB_EXTERNAL_PORT` override). A probe that
  cannot answer never blocks — tri-state discipline holds.
* **`native-test` is kept.** The launcher wiring is verified but nothing has yet
  streamed a real build through the UI, and its warm Docker cache makes that
  re-test minutes rather than hours.

**What remains between here and the beta:** Task 11 (account + SOAP bootstrap —
without it every SOAP feature is dead with no cause shown) and Task 8
(`Install-DML-Native.ps1`), plus the live gates only a human can run.

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

> **AMENDED 2026-08-04 (user): the distro is the FOUNDATION again, not the thing
> being retired.** Branch `feat/arch-wsl-backend` moves the launcher off Docker
> Desktop and onto `dockerd` running inside a `dml-arch` WSL distro the launcher
> creates itself, with the Rust `dml-wow` binary running INSIDE that distro. That
> reverses the retirement above: `dml-arch` stops being legacy and becomes the
> supported backend, and Docker Desktop becomes the kept-working fallback for a
> user who already has a server there.
>
> **Status, stated honestly.** Nine tasks landed and each passed its own review;
> `crates/dml-core`'s `distro`, `setup` (Arch chain), `backend::Arch` and
> `DmlRunner::arch()` all exist and are tested, and Task 10's live gate
> provisioned a genuinely fresh `dml-arch-test` from nothing in ≈3m43s
> (`docs/superpowers/plans/2026-08-04-arch-wsl-backend-gate.md`).
> **AMENDED 2026-08-05.** The vocabulary half of that sentence is now WRONG and
> is corrected here rather than deleted: the port shipped (`dml_core::vocab`,
> 107 call sites — 74 translated, 33 falling back to the bash `dml` in the same
> distro), `--json` is appended only for the bash target, and `dml-wow` gained
> `games-list`/`games-status`. **The launcher wiring is still NOT done** for the
> other half: `probe_arch_with`, `derive_arch` and `dml_core::distro` have zero
> production callers, so an Arch user has no first-run path, and the Settings
> dropdown offers no `arch` option. Four blockers found and closed the same day
> (the vocabulary, a games-dir split that made reads silently wrong, WSL powering
> the distro off 15s after the last session, and a SOAP credential split) are
> recorded in `docs/backend-comparison-2026-08.md`,
> `docs/superpowers/plans/2026-08-05-wsl-distro-lifetime.md` and `CLAUDE.md`.
>
> **So the default deliberately still points at the old backend.** `DML_BACKEND`
> unset, empty or unrecognised resolves to `Wsl` (the bash CLI in the distro),
> auto-detection can never choose `Arch`, and `arch` is an explicit opt-in only.
> Flipping it before the call sites are ported would replace a working user's
> status card with "unrecognized subcommand 'games'". The flip happens in the
> next plan, in the same change that teaches the launcher that vocabulary.
>
> This does not move any Phase B item: native stays the v0.1.0 backend for the
> beta. The Arch backend is the road after it.

---

## 🟢 PHASE A — the three sharp edges — ✅ ALL THREE RESOLVED

Deliberately first. Each is small, each is already understood, and each one
undermines a gate we are about to lean on. Doing them first means Phase B is
built on instruments we trust.

**Closed out 2026-08-03**: A1 fixed and pinned by three tests, A2 done
2026-08-01, A3 not reproduced with its hazards closed. Nothing in this phase
blocks anything.

### A1 — Resume after a generator fix silently reuses the stale file — ✅ DONE

Found live on 2026-07-31 and it cost real time. `generate-compose` had recorded
itself done, so re-running the install after fixing the generator **reused the
old broken output** and skipped the fix. The state file had to be deleted by
hand.

A stranger hitting this gets a build that fails for a reason we already fixed,
with no way to know that. Resume is a headline feature of the install engine;
resume that silently serves stale generated output is worse than no resume,
because it lies about what it did.

**Fixed, and verified 2026-08-03 rather than assumed.** The architecture no
longer has a "recorded, therefore skipped" path at all: `run_stage` runs every
stage on every run and each one decides from ON-DISK EVIDENCE what work is left
(`a_resume_skips_the_clones_and_the_build_when_the_disk_agrees`). So
`do_generate` always calls `composegen::write_all_with`, whose `write_file`
calls for the base and build overlays are unconditional — a template fix
reaches an existing install by construction.

The one file that is NOT unconditionally rewritten is
`docker-compose.override.yml`, and that asymmetry is deliberate rather than a
remnant: before `up` it is purely our output and gets refreshed, after `up` it
is where `crate::config` keeps the user's bot counts, rates and SOAP settings,
so regenerating it would eat them. Both halves are pinned —
`a_resume_before_up_refreshes_every_generated_file` (asserting a planted
`STALE-GENERATED-OUTPUT` marker is gone) and
`a_resume_after_up_never_touches_the_users_settings`. All three green.

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
14 tasks. TEN are done — only the two LIVE gates and the docs pass remain.

| # | Task | State |
|---|---|---|
| 1 | Pin the upstream build contract | ✅ `0dbfd3f` — core `190184a0`, module `ba46fcde`, **verified** after checkout |
| 2 | Compose/override generation | ✅ `e354cb5` |
| 3 | Native install engine (staged, resumable, NDJSON) | ✅ proven live |
| 4 | Honest hardware preflight | ✅ `e354cb5` |
| 5 | `dml-wow install-native` CLI surface | ✅ |
| 9 | Migration scripts fixed to match their own lessons | ✅ `e354cb5` |
| 6 | Launcher wiring | ✅ built + **click-verified 2026-08-01** (both scenarios; see B1) |
| 8 | `Install-DML-Native.ps1` | ✅ **2026-08-01** + a 40-check harness, 3 mutations caught |
| 11 | Account + SOAP bootstrap | ✅ **2026-08-01** — verified before it saves |
| 7 | Port guard on native start | ✅ **2026-08-01** — refuses on the three stack ports, mirrored bash↔Rust |
| 10 | `migrate-import` | ❌ not started |
| 12, 13 | LIVE gates — real build, kill-mid-build resume, first login | 🙋 user |
| 14 | Docs + doctrine reconciliation | partial |

### B1 — Task 6: launcher wiring — ✅ BUILT + CLICK-VERIFIED 2026-08-01

**Live gate PASSED on the user's machine**, both scenarios, against a scratch
games dir so the real server was never touched:

* **Fresh-PC scenario** (games dir absent) → "No game server installed yet" +
  Open Library → Library offers an armed **Install** on WoW Playerbots →
  clicking it raises the consent panel (hours / tens of GB / cannot be
  cancelled) → "Not now" starts nothing.
* **Half-finished-install scenario** (a real `.dml-install.json` recorded
  through `generate-compose`, path-bound to the scratch dir so `load_state`
  accepts it) → the **"unfinished install"** badge, the explanation, and
  **"Resume install"** — with **no Start button**.

**WHAT THIS DID NOT PROVE, so none of it may be assumed:**

1. **A native install has never been run from the UI.** Both scenarios stop at
   the button. The engine has an 8/8 live run behind it from the CLI, and the
   translation layer is unit-tested, but no build has ever been streamed through
   `InstallTerminal`. That is why `native-install` still reads `"untested"` in
   the feature registry and why the consent panel says so out loud.
2. **Auto-detect was not exercised.** This machine has `dml-arch`, so `detect`
   correctly answers Wsl and the backend had to be pinned with `DML_BACKEND`.
   The fresh-machine path (Docker present, no distro → Native) is unit-tested
   only; it needs a machine with no distro, which is the Task 12 VM leg.
3. **Nothing about the build itself** — no cold cache, no resume of a REAL
   interrupted build, no first login.

### B1 (original scope) — what the wiring had to fix

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

### ✅ B2, B3, B4 — all three shipped 2026-08-01

**Task 7 — the start refuses on a CONTAINER-NAME conflict.** It shipped first as
a *port* guard and an adversarial audit measured that guard wrong in both
directions against a live Docker Desktop, which is worth recording because the
idea is so plausible:

| situation | the bind probe said | reality |
|---|---|---|
| Docker publishing `0.0.0.0:47893` — `netstat` LISTENING, serving HTTP 200 | **FREE** | taken |
| a plain `TcpListener` holding `0.0.0.0:47895` | **TAKEN** | `docker run -p 47895:80` came up anyway |

So it was inert for the only cause its own message named ("another DML server is
already running") and fired for cases where that message was wrong — with no
override. **A guard wrong in both directions is worse than none, because people
trust it.** Independently reproduced before acting on it.

The question was never about ports: the `ac-*` names are global to the docker
ENGINE, so a second stack cannot exist whatever the ports are doing. The guard
now asks exactly what the install-time guard asks, through the same pure helpers,
so the two cannot disagree. Mirrored into bash. Tri-state holds — a docker that
cannot answer never blocks a start.

**The tri-state test also found a pre-existing bug**: `_check_port_conflicts`
ran an unguarded `ss` substitution under `set -euo pipefail`, so on a machine
without `ss` the whole command died silently — `dml start <title>` exited 1 with
no output at all.

**Task 11 — the account + SOAP bootstrap.** Guided, because automation is not
available: `docker attach` REFUSES piped stdin against a TTY container
(verified live), and without the tty it accepts the pipe and never returns. The
only other route is an SRP6 write, which would be a third sanctioned MySQL write
and is the user's call. What the module guarantees instead is that **"done" is
earned** — `~/.dml/soap.env` is written only after a real round-trip succeeds.
Rejected and Unreachable are separate outcomes, because the usual cause of
unreachable is a world server still booting and blaming the password sends the
user to recreate a working account.

**Task 8 — `Install-DML-Native.ps1`.** No WSL, no Arch, no C# tray (which
resolves SHIP-LIST 4.0b by construction). Docker Desktop is instructed rather
than installed — its licence is the user's decision. The yq pin was *obtained*:
I first wrote a plausible hash from nothing, which would have failed every
install for a reason resembling tampering.

### B2 (original scope) — Task 8: `Install-DML-Native.ps1`

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

> **Wrath Unbound no longer waits for this (2026-08-03 note).** The scope
> section above says Unbound moves to v0.2 *behind this runner*; that premise
> expired on 2026-08-02, when the 3124-line bash add-on installer was ported to
> a native staged/resumable engine (`unbound.rs` + `unbound_payload.rs`, CLI
> `dml-wow unbound install|uninstall|status`, Tools-card wiring, client add-ons
> installed and exportable). It needs no distro and no `.sh` runner.
>
> This changes only what is POSSIBLE, not what ships: whether Unbound is in the
> v0.1.0 cut is the user's scope call, unchanged and unmade. B4b is still
> required for Vanilla and TBC, which have no native port.

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

#### REJECTED: run the installers in a Docker container instead of a distro

Asked 2026-08-01 ("can we make Docker Desktop run a Linux container to install
the games?") and settled by experiment, not argument. Recorded here as a NEGATIVE
RESULT so nobody spends the afternoon again — it is an attractive idea and the
first two tests encourage it.

| test | result |
|---|---|
| container with `/var/run/docker.sock` mounted drives the HOST daemon | ✅ works (`docker ps` returned real containers) |
| nested `docker run -v /work:/x` where `/work` is the outer container's mount | ❌ **silently empty** |
| nested run given the WINDOWS host path (`C:/Users/...`) | ✅ works |
| mounting at the daemon's own host-mount path (`/run/desktop/mnt/host/c/...`) | ❌ empty |

**Why the third row does not save it.** `docker compose` computes absolute paths
from ITS OWN cwd before handing them to the daemon. Run inside a container, that
cwd is a Linux path, so `./modules:/azerothcore/modules` arrives at the host
daemon as a path that does not exist there — and a bind mount to a missing path
is not an error, it is an EMPTY DIRECTORY. The server then boots without its
modules and looks fine to every check we have. That is precisely the
"boots a silently wrong server" class the migration already hit once, which is
why an approach that merely *can* be made correct is not good enough here.

Making it work would mean patching every installer to emit Windows host paths —
i.e. modifying the scripts, when using the scripts unmodified was the entire
appeal. And a container that only clones and generates, leaving `compose up` to
the host, is just `install_native.rs` with extra steps.

**WSL has none of this.** Docker Desktop's WSL integration translates paths
natively, which is why the existing `dml-arch` server works today.

### B5 — Task 10 (`migrate-import`)

The "bring your existing server across" path. Genuinely valuable and genuinely
optional for a beta — a new user installs fresh.

---

## 🙋 PHASE C — the gates only the user can run

No amount of engineering closes these. They are the release standard.

- **Task 12 — LIVE native gate.** ✅ **LEG 1 PASSED 2026-08-04**: scratch
  `DML_GAMES_DIR`, killed from Task Manager at ~30% of the build, relaunched,
  Resume continued cache-warm rather than from zero. Measured **peak Docker VM
  16.4 GB, 1088.1s total** including the kill and resume — folded into
  SMOKE-TESTS §26, where the reason it does not contradict the 6/8 GB floors is
  written down (parallelism is memory-bound, so a bigger VM uses more rather
  than needing more). ⬜ **Leg 2 outstanding**: the same on a VM that has never
  seen DML (no Docker, no Git, no repo) — a build that only ever ran on the dev
  box does not count. ✅ **Real WoW client login PASSED 2026-08-04** — a new
  character created and entered the world on the RESUMED server. That is the
  part a status probe cannot stand in for: `verdict: online` says the stack
  answers, not that a player can play on it.
- **Task 13 — LIVE migration gate**, if Task 10 ships: real export/import, the
  identity check (2505 characters, 255 accounts), then the negative test — with
  one server running the other must refuse on ports.
- **The five outstanding live gates** already listed in SHIP-LIST Phase 1.
- **The Desktop-icon check** — run the launcher installer: is there an icon, does
  the Start menu open the new app (not the retired tray)?
- **Wrath Unbound live gate** (added 2026-08-03 with the scope amendment above).
  Install from the Tools card on a native server, confirm the `[UNBOUND] Prereq
  map built.` marker and the multi-class trainer in game, then uninstall and
  confirm the world comes back ready with the marker absent. The engine leaves
  the Mentor NPC spawn to the user by design (`.npc add 900001`), so that step
  is part of the gate rather than a defect.
- **Three tray-design answers** (these block the multi-server work, not the
  beta): where does a server's display name live; does the tray list every server
  or follow one active server; does Home follow the active server or stay
  WoW-only?

---

## ✅ THE DISTRIBUTION HOLE — found AND closed 2026-08-04

**Closed the same day.** `v0.1.0-rc1` is published as a GitHub pre-release with
both installers (NSIS 7.2 MB, MSI 10.3 MB), and `Install-DML-Native.ps1` now
**installs the launcher itself**, silently, resolving the newest release from the
GitHub API. A user runs one script and ends with a working product — no wizard,
no releases page, no manual step.

Two rules worth keeping. The launcher is installed BY DEFAULT (`-NoLauncher`
opts out), unlike Docker and Git which stay opt-in: those are third-party
products whose licences are the user's decision, while the launcher is this
project — and a setup script that refuses to install the thing it is setting up
was the dead end being fixed. And the asset is resolved from `/releases` rather
than `/releases/latest`, because the latter excludes pre-releases and the only
release today is one.

Leg 2 can now test the path a stranger actually takes: download the PS1, run it,
end with a launcher.

The original finding is kept below, because the reasoning is what stops it
reopening.

### The original finding

**`Install-DML-Native.ps1` does not install the launcher, and no built launcher
installer exists anywhere.** It sets up Docker, Git, WebView2, yq and
`launcher.json`, prints "Ready.", and then told the user to "open the DML
Launcher" — which is not on their machine, and which nothing they have access to
can put there.

Found by the user while preparing the Task 12 leg-2 VM, and the question that
exposed it is the one a stranger asks first: *where is the launcher?*

This is not a documentation bug. The launcher ships as an NSIS/MSI bundle from
`npm run tauri build`, which needs the repo, Rust and Node — everything the
native route exists to avoid. So today the ONLY way onto a clean machine is to
build it on a dirty one and copy the file across, which is not a distribution
story.

What closes it, in order:

1. **Build the bundle** (`npm run tauri build` → `target/release/bundle/`). The
   tree was empty when this was found on 2026-08-04.
2. **Attach it to a GitHub Release**, so the URL the installer now prints
   resolves to something. Phase D already calls for this; it is now a
   PREREQUISITE for leg 2 rather than a release-day step, because leg 2 cannot
   honestly pass without it.
3. The closing message now states plainly that the script prepares the PC but
   does not install the launcher, and names the releases page (fixed same day).

Until (2) lands, leg 2 can only be run by side-loading the installer onto the
VM — which tests the launcher but NOT the path a stranger takes. A gate that
skips the distribution step proves less than it appears to.

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
