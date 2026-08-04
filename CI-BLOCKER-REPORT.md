# Round 5.6 — the slow `install-native` test, diagnosed and fixed

**Branch:** `worktree-agent-acffab9c8b6e84b39`
**Base:** `76dc264` (`feat/arch-wsl-backend`)
**Commits:** `45f1f81` (fix), `e4e5a65` (test-order correction + contract doc)
**`proc.rs` touched:** **NO.**

---

## 0. The base had to be corrected first

The worktree was created at `a624a38` ("Update Install-DML.ps1 Mod Manager Unbound
Era Low Ram Fix") — a `main`-lineage commit with no `crates/` tree at all, and
therefore none of the `dml_core::proc` precedent this task depends on. I verified
`a624a38` is an **ancestor** of `76dc264` (so a reset loses nothing), reset my own
branch, and confirmed `abandon`, `Abandonable` and `collect_by` are all present.
All work below is on `76dc264`.

---

## 1. Diagnosis — it was never the probe

**The brief's hypothesis was that the docker-reachability probe does not honour
`PROBE_TIMEOUT`. It does honour it.** Both probe paths are already correct:

- `preflight::probe_docker` → `run_bounded_outcome` (the already-fixed one).
- `install_native.rs`'s `Call { timeout: Some(PROBE_TIMEOUT) }` →
  `output_bounded_draining`.

Against a program that cannot be spawned, both return in **milliseconds** — the
spawn fails with `ErrorKind::NotFound` before any deadline is relevant.

The time was spent in the **engine-start detour**, which is not a probe at all.
`InstallNative::do_preflight` (`install_native.rs:1476`) does:

```rust
if facts.docker.reachable != Tri::Yes {
    let attempted = self.io.ensure_engine(...);   // <-- here
```

`ProcIo::ensure_engine` → `native::ensure_engine_up_stream` →

```rust
poll_until_ready(
    ENGINE_POLL_INTERVAL_MS,   // 3_000
    ENGINE_POLL_TIMEOUT_MS,    // 180_000
    || engine_running(&program),   // <-- the SAME program that just failed to spawn
    ...)
```

The test sets `DML_DOCKER` to a path that does not exist, precisely so it is
hermetic. That makes the readiness predicate **false by construction on every
tick**: 61 checks, 3 seconds apart, asking a program that cannot be spawned
whether the engine is up. The loop burns its entire 180-second budget, the
preflight is re-gathered, and the run then emits the same
`INSTALL_DOCKER_UNREACHABLE` refusal that was available at t=0.

**Root cause, in this repo's own vocabulary.** `ProbeOutcome::ProgramMissing` is
documented as *"the one genuinely definitive negative"*, and
`classify_docker_info` collapses it into `Tri::No`, indistinguishable from a
merely-stopped engine. `ensure_decision` then took a plain `engine_up: bool`, so
"there is no docker CLI" (nothing can ever come up; waiting is dead time) and
"the engine is down" (start it and wait) produced the identical `Launch`. It is
the tri-state mistake, in a place it had not yet been applied.

**Two side effects that matter beyond the clock:**

1. It **launches the real Docker Desktop GUI** (`launch_detached`) from a unit
   test. On this box Docker Desktop was already running, so the spawn returned at
   once. On a box where it is *not* running, a `cargo test` run cold-starts
   Docker Desktop **and its `docker-desktop` WSL2 VM** mid-suite — on a 31 GB
   machine that already reserves 16 GB for WSL2, exactly the RAM-pressure
   scenario `CLAUDE.md` warns about. That is a plausible, non-deterministic route
   to the "12+ minutes" in the brief, and it is why it would not reproduce
   reliably.
2. **A second, separate unbounded call.** `engine_running` was a bare
   `cmd.status()` with **no wall-clock bound at all** — and it is the predicate
   the poll calls up to 61 times. A `docker info` against a dockerd wedged during
   startup (precisely the state the readiness wait exists for) connects and never
   answers, so the *first* call never returns and the 180s budget never advances.
   A deadline is only ever consulted after a call RETURNS. This is the
   "bound requested of the child but never enforced by the caller" shape from the
   task list, and it is now fixed too.

---

## 2. Measurements (same box, same binary, foreground)

| What | Before | After |
|---|---|---|
| `install_native_refuses_an_unreachable_docker_before_it_creates_anything` | **184.48 s** | **4.41 s** |
| `cargo test --workspace` (whole suite) | **261.91 s** — 1719 passed, 0 failed | **91.16 s** — 1726 passed, 0 failed, 7 ignored |

Confirming experiment before any code changed: re-running the offending test with
`DML_DOCKER_DESKTOP` pointed at a missing path (which makes `launch_detached`
fail and skips the poll) took **4.43 s**. That isolated the 180 s to
`poll_until_ready` and nothing else.

The +7 tests are exactly the 7 added here (3 in `native.rs`, 4 in `engine.rs`).

---

## 3. The fix

Applied at the **shared** path, so all three callers benefit: `install_native`,
`unbound` (delegates to the same `ProcIo::ensure_engine`), and the launcher's own
engine-up command — which had the identical 3-minute dead wait behind Home's
Start button.

`crates/dml-core/src/engine.rs`
- New `EnginePresence { Up, Down, CliMissing }` and `engine_presence` (bounded),
  with the pure classifier `engine_presence_of` split out so the mapping cannot
  drift from the spawn that feeds it.
- `EnsureDecision` gains `NoDockerCli`; `ensure_decision` now takes
  `EnginePresence`. **`CliMissing` outranks `desktop_found` deliberately** — a
  present GUI cannot rescue an absent CLI, because the readiness probe runs
  *through* the CLI. That pairing (Docker Desktop installed, `DML_DOCKER`
  unreachable) is the real machine's shape, not a hypothetical.
- `CouldNotTell` still reads as `Down`. A probe that blew its deadline is
  evidence of nothing; promoting it to the definitive negative would refuse to
  start an engine that was merely slow — the mirror-image failure, and a worse
  one than the wait it replaces.
- `engine_running` is now expressed in terms of `engine_presence` and inherits
  its bound (was an unbounded `cmd.status()`).

`crates/dml-wow/src/native.rs`
- `ensure_engine_up_stream_with(...)` — the injectable half (same precedent as
  the existing `stop_engine_stream_with`), taking presence / start / launch /
  sleep as parameters. `ensure_engine_up_stream` is now a thin production wrapper.
- New `NoDockerCli` arm: emits `DOCKER_CLI_MISSING` immediately, launching
  nothing and waiting for nothing.

`docs/cli-contract.md` — `DOCKER_CLI_MISSING` documented alongside
`DOCKER_DESKTOP_MISSING`, including that a `docker info` which merely times out
stays `DOCKER_ENGINE_TIMEOUT`.

---

## 4. The pin is deterministic, and it is on the WAIT

No wall clock. `sleep` is injected, so *"did we enter the readiness wait?"* is a
counter. The invariant asserted is `sleeps == 0`, plus `launches == 0`,
`starts == 0`, `probes == 1`.

The counters are asserted **before** the error code, deliberately. In the first
version the code assertion fired first, which pinned the *symptom*: a future
"fix" that merely mapped the readiness timeout onto the new code would have
satisfied it with all 180 seconds still in place. Commit `e4e5a65` corrects that.

Two companion tests stop the over-fix (deleting the poll would otherwise leave
the main test green): an engine that is merely `Down` must still be started and
still waited for, and a `CouldNotTell` must still be waited for.

### Vacuity check (mandatory)

Mutation applied to `ensure_decision` — the original bug reinstated, `CliMissing`
collapsed back into "merely down":

```
test native::engine_section_tests::a_missing_docker_cli_is_refused_without_entering_the_readiness_wait ... FAILED

assertion `left == right` failed: the readiness wait must never be ENTERED:
it cannot succeed behind a CLI that does not exist
  left: 60
 right: 0

test result: FAILED. 3 passed; 1 failed; 0 ignored; 988 filtered out; finished in 0.00s
```

`left: 60` is sixty sleeps of 3000 ms — the 180-second dead wait, caught exactly,
in 0.00 s.

Reverted with `git checkout -- crates/dml-core/src/engine.rs`; tree clean and
green:

```
$ git status --short
(empty)

test native::engine_section_tests::a_missing_docker_cli_is_refused_without_entering_the_readiness_wait ... ok
test native::engine_section_tests::an_engine_that_is_merely_down_is_still_started_and_waited_for ... ok
test native::engine_section_tests::a_docker_that_merely_did_not_answer_is_still_waited_for ... ok
test native::engine_section_tests::the_engine_stop_wraps_its_lines_in_a_section_it_closes_itself ... ok
test result: ok. 4 passed; 0 failed; 0 ignored; 988 filtered out; finished in 0.00s
```

---

## 5. Concerns — read this part

**The suite did not wedge, and I could not confirm this was the CI blocker.**
At the base commit `cargo test --workspace` **completed** in 261.91 s with 1719
passing and 0 failures. It is slow, not hung. The "12+ minutes / a green badge
means the job was cancelled" framing did not reproduce here.

More importantly, the 180 s path is gated on `docker_desktop_program()` returning
`Some`, and `ProcIo::ensure_engine` early-returns when it is `None`:

- **Linux CI: the slow path cannot trigger.** `candidate_docker_desktop_paths()`
  is built from `LOCALAPPDATA` / `ProgramFiles`, which do not exist on Linux, so
  the candidate list is empty and the result is `None`. (Code-level reasoning,
  not measured on a runner.)
- **Windows CI: depends on whether `Docker Desktop.exe` exists at one of the
  three candidate paths on `windows-latest`.** GitHub's Windows image ships
  Docker Engine rather than Docker Desktop, so it is probably `None` there too —
  but I could not verify a runner image from this machine. If it *is* present,
  this test was burning 180 s **and attempting to launch Docker Desktop on a CI
  runner**, which would be a genuine blocker.

So: the bug is real, the fix is real, and it removes a 3-minute dead wait plus a
GUI-launching side effect from the test suite on every platform. But **the claim
that this specific test is what blinds CI is unproven**, and the linux job's
artifact step failing to complete may well have a different cause that I have not
found. That should be checked against an actual CI run rather than assumed fixed.

**Other notes.**
- The 18 `dml-wow` parity suites skip without the live snapshot server
  (`DML_GAMES_DIR` + a running stack). That is the documented user gate, not
  something introduced here — the zero-SKIP parity run was not attempted.
- `ensure_decision`'s signature changed (`bool` → `EnginePresence`). Blast radius
  is contained: only `dml-core/src/engine.rs` and `dml-wow/src/native.rs`
  reference it; the launcher does not. Whole workspace compiles, 0 warnings.
- `ProcIo::ensure_engine` still returns `true` (meaning "re-gather your facts")
  after a `DOCKER_CLI_MISSING` refusal, costing one redundant preflight. Harmless
  and left alone deliberately to keep the rule expressed in exactly one place.
