# Backend::Arch — Plan 1: foundations and provisioning

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** make `dml-wow` run *inside* the `dml-arch` WSL distro against that
distro's own `dockerd`, driven from Windows, and make a fresh PC able to reach
that state from nothing.

**Architecture:** the launcher spawns `wsl.exe -d dml-arch -u dml --exec dml-wow
<cmd> --json`. Everything below that spawn is unmodified Rust running on Linux
paths against a local docker socket. The only new routing is a third `Backend`
arm and a runner constructor; the only new subsystem is distro creation.

**Tech Stack:** Rust (workspace at repo root), `wsl.exe` 2.7.10, Arch Linux WSL
from the official catalog, docker 29.6.1 + docker-compose 5.3.1 +
docker-buildx 0.35.0 under systemd.

**Spec:** [`docs/superpowers/specs/2026-08-04-arch-wsl-backend-design.md`](../specs/2026-08-04-arch-wsl-backend-design.md)

**Branch:** `feat/arch-wsl-backend` (already created, off `rust-main`).

## Global Constraints

Every task's requirements implicitly include this section.

- **`--exec`, never `--`.** `wsl -- ` runs a shell that splits on `;`, expands
  `$HOME` and globs `*` against the cwd. Verified 2026-07-28. Every new wsl.exe
  invocation in this plan uses `--exec` with real argv.
- **Tri-state discipline.** A probe that could not answer is evidence of
  NOTHING. `Tri::Unknown` is never read as `Tri::No`.
- **Pinned known-good package versions:** docker `1:29.6.1-1`,
  docker-compose `5.3.1-1`, docker-buildx `0.35.0-1`. `docker-buildx` is
  required, not optional — `install_native.rs`'s `pct` parser reads BuildKit
  vertex headers and resume rests on BuildKit's cache.
- **Distro and user names:** `dml_core::runner::DISTRO` (`"dml-arch"`) and
  `dml_core::runner::USER` (`"dml"`). Never re-hardcode the strings.
- **Test portability:** no hardcoded `cmd.exe`, no drive-letter `Path`
  literals. Use the existing `#[cfg]` fixture helpers in `proc.rs`
  (`shell_program()`, `shell_args()`, `fixture()`).
- **Anti-vacuity:** after writing a test that overrides a shared fixture or
  asserts a bound, mutate the production code and watch it go red. An override
  never proved to take effect probably didn't.
- **Never run bats and the cargo parity suites at the same time.** Every bats
  `setup()` rewrites `cli/dml` in place while the parity suites spawn it.
- **Cargo lives at** `%USERPROFILE%\.cargo\bin` and may be missing from a fresh
  shell's PATH. Use the full path or prepend it per call.
- **Do not run `cargo test --workspace` and `npm run tauri dev` at once** —
  same `target/` lock, double the peak RAM on a 31 GB box.
- **Commands run from the repo root**, not `launcher/src-tauri`.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `crates/dml-core/src/proc.rs` | make the bounded runner's deadline real | 1 |
| `crates/dml-core/src/backend.rs` | the third `Backend` arm and its detection | 2 |
| `crates/dml-core/src/runner.rs` | `DmlRunner::arch()` — the wsl.exe invocation prefix | 3 |
| `crates/dml-core/src/engine.rs` | start/stop the engine per backend (Desktop vs systemd) | 4 |
| `crates/dml-core/src/compose.rs` | the Linux default games dir | 5 |
| `crates/dml-core/src/distro.rs` (new) | pure argv + step list for creating and preparing the distro | 6 |
| `crates/dml-core/src/setup.rs` | the Arch probe chain — what is the first thing missing | 7 |
| `.github/workflows/rust.yml` | build and publish the Linux `dml-wow` artifact | 8 |
| `launcher/src-tauri/src/payload.rs` | the bundled-resource manifest gains the binary | 8 |
| `launcher/src-tauri/src/provision.rs` | deploy the binary into the distro + version handshake | 9 |

---

### Task 1: Make the bounded runner's deadline real

The foundation task. Every `wsl.exe` call this plan adds is bounded, and today
that bound is not enforced when the child spawns a grandchild — `wsl.exe` does.
The diagnosis is already written in `engine.rs:455-472`: `child.kill()`'s result
is discarded and the following `child.wait()` then blocks until the shell ends
by itself. Measured: a 600 ms bound returning after 605 s.

**Files:**
- Modify: `crates/dml-core/src/proc.rs:100-159` (`run_bounded_outcome`)
- Modify: `crates/dml-core/src/proc.rs:422-447` (`output_bounded`, same pattern)
- Test: `crates/dml-core/src/engine.rs:473-492` (un-ignore the existing pin)

**Interfaces:**
- Consumes: nothing.
- Produces: no signature change. `run_bounded_outcome(Command, Duration) ->
  BoundedOutcome` and `output_bounded(Command, Duration) -> Option<Output>`
  keep their contracts; only the timeout path's behaviour changes.

- [ ] **Step 1: Un-ignore the pinning test and watch it hang or fail**

In `crates/dml-core/src/engine.rs`, delete the `#[ignore = "flaky: the bound is
still not fully enforced — see the doc comment"]` attribute from
`a_deadline_bounds_the_call_even_when_a_grandchild_holds_the_pipes` (line 474),
and replace the last three paragraphs of its doc comment (the ones beginning
"IGNORED, and the reason is a result rather than a convenience") with:

```rust
    /// Enabled 2026-08-04. The original fix returned early instead of joining
    /// the reader threads, which was A cause; the remaining one was the
    /// `child.kill()` / `child.wait()` pair inside the poll loop — a kill whose
    /// result is discarded followed by a wait that blocks until `cmd.exe` ends
    /// on its own. The reap now happens on a detached thread, so the caller's
    /// deadline is the caller's deadline.
```

- [ ] **Step 2: Run it and confirm it fails**

```
cargo test -p dml-core --lib a_deadline_bounds_the_call_even_when_a_grandchild_holds_the_pipes
```

Expected: FAIL on `took 600s: the call outlived its own deadline, which is the
bug`. It may take up to 10 minutes to fail — that duration IS the bug. If it
passes in under a second, stop and investigate before changing anything: the
test is not reproducing and a fix cannot be verified.

- [ ] **Step 3: Fix `run_bounded_outcome`**

Replace `crates/dml-core/src/proc.rs:100-144` (from `let deadline` through the
`if timed_out { return BoundedOutcome::TimedOut; }` block) with:

```rust
    let deadline = std::time::Instant::now() + timeout;
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    timed_out = true;
                    break None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => {
                timed_out = true;
                break None;
            }
        }
    };
    // THE TIMEOUT PATH TOUCHES NOTHING BLOCKING. Two separate traps live here,
    // and each one on its own is enough to make a bound fictional.
    //
    // 1. Joining the reader threads. `child.kill()` kills the CHILD; a
    //    GRANDCHILD that inherited the pipe handles keeps them open, so the
    //    readers never see EOF. Nothing is lost by skipping the join —
    //    `TimedOut` carries no output and never did.
    //
    // 2. `child.wait()` after the kill. `kill()` can fail, and its result was
    //    discarded; `wait()` then blocks INFINITE for a process we have
    //    already decided to stop waiting for. Measured 2026-08-03: a
    //    600ms-bounded call against `cmd /C ping -n 600` returned after 605
    //    SECONDS, and the deadline had fired correctly — the time was spent
    //    here. The reap still happens, on a thread nobody waits for.
    if timed_out {
        let _ = child.kill();
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        return BoundedOutcome::TimedOut;
    }
```

- [ ] **Step 4: Fix the same pattern in `output_bounded`**

`output_bounded` (`crates/dml-core/src/proc.rs:422-447`) has the identical
kill-then-blocking-wait shape in two arms. Replace both arms' bodies:

```rust
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    reap_detached(child);
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => {
                reap_detached(child);
                return None;
            }
```

and add this helper immediately above `run_bounded_outcome` in the same file:

```rust
/// Kill a child we have stopped waiting for and reap it on a thread nobody
/// joins. Blocking on `wait()` here is how a bounded call loses its bound: the
/// kill may fail, and the wait is then INFINITE against a process that outlives
/// the deadline by design. See `run_bounded_outcome`'s timeout path.
fn reap_detached(mut child: std::process::Child) {
    let _ = child.kill();
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}
```

- [ ] **Step 5: Run the pinning test and the whole proc/engine suite**

```
cargo test -p dml-core --lib proc::
cargo test -p dml-core --lib engine::
```

Expected: PASS, and the grandchild test completes in well under 20 seconds.

- [ ] **Step 6: Prove the test is not vacuous**

Temporarily put `let _ = child.wait();` back inline (before the
`std::thread::spawn`) in `run_bounded_outcome`, re-run the grandchild test, and
confirm it goes red again. Then revert the mutation.

- [ ] **Step 7: Run the workspace suite — the failure mode was parallel load**

```
cargo test --workspace
```

Expected: the grandchild test passes under parallel load too. That is the exact
condition under which it previously took 605 s.

- [ ] **Step 8: Commit**

```bash
git add crates/dml-core/src/proc.rs crates/dml-core/src/engine.rs
git commit -m "fix(core): a bounded call now actually returns at its deadline

The kill-then-wait pair inside the poll loop was the remaining cause: kill's
result was discarded and the following wait blocked INFINITE for a process we
had already decided to stop waiting for. Measured 605s on a 600ms bound. The
reap moves to a detached thread and the grandchild pinning test comes off
#[ignore]."
```

---

### Task 2: `Backend::Arch`

**Files:**
- Modify: `crates/dml-core/src/backend.rs:24-124` (enum, `from_override`, `detect`, `resolve`)
- Test: `crates/dml-core/src/backend.rs:126-246` (the existing `mod tests`)

**Interfaces:**
- Consumes: `dml_core::setup::Tri`.
- Produces: `Backend::Arch`; `from_override(Option<&str>) -> Backend` now
  returns `Arch` for everything that is not `native`/`docker`;
  `detect(native_dir_exists: bool, docker_present: bool, distro_usable: Tri) ->
  Backend`; `resolve(env_value, file_value, native_dir_exists, docker_present,
  distro_usable) -> Backend` — same signature, new third-arm behaviour.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/dml-core/src/backend.rs`:

```rust
    #[test]
    fn arch_is_the_default_and_wsl_resolves_to_it() {
        // Decision 2 of the spec: the bash CLI is retired as a runtime path,
        // but `wsl` names the same distro and the same daemon, so an existing
        // launcher.json or a hand-written DML_BACKEND=wsl must land on Arch.
        // Refusing would strand every current user; mapping it to Native would
        // point them at a server directory that is not theirs.
        assert_eq!(from_override(None), Backend::Arch);
        assert_eq!(from_override(Some("")), Backend::Arch);
        assert_eq!(from_override(Some("arch")), Backend::Arch);
        assert_eq!(from_override(Some("  WSL ")), Backend::Arch);
        assert_eq!(from_override(Some("natve")), Backend::Arch);
    }

    #[test]
    fn native_still_needs_saying_so_explicitly() {
        assert_eq!(from_override(Some("native")), Backend::Native);
        assert_eq!(from_override(Some("  DOCKER ")), Backend::Native);
    }

    #[test]
    fn a_usable_distro_is_always_arch() {
        for dir in [true, false] {
            for docker in [true, false] {
                assert_eq!(detect(dir, docker, Tri::Yes), Backend::Arch, "dir={dir} docker={docker}");
            }
        }
    }

    #[test]
    fn a_working_native_user_with_no_distro_is_left_on_native() {
        // They have a server installed under Docker Desktop and no distro to
        // move it to. Moving them would point the app at a directory that does
        // not exist yet.
        assert_eq!(detect(true, true, Tri::No), Backend::Native);
        assert_eq!(detect(true, true, Tri::Unknown), Backend::Native);
    }

    #[test]
    fn a_fresh_machine_gets_arch_because_arch_is_the_one_we_can_provision() {
        assert_eq!(detect(false, false, Tri::No), Backend::Arch);
        assert_eq!(detect(false, true, Tri::No), Backend::Arch);
        assert_eq!(detect(false, false, Tri::Unknown), Backend::Arch);
    }

    #[test]
    fn resolve_env_still_outranks_everything() {
        // Load-bearing: the parity, bats and CLI-integration suites all inject
        // these vars as override seams.
        assert_eq!(resolve(Some("native"), Some("arch"), false, false, Tri::Yes), Backend::Native);
        assert_eq!(resolve(Some("arch"), Some("native"), true, true, Tri::No), Backend::Arch);
    }

    #[test]
    fn resolve_auto_means_detect_in_both_places() {
        assert_eq!(resolve(Some("auto"), None, true, true, Tri::No), Backend::Native);
        assert_eq!(resolve(None, Some("  AUTO "), false, false, Tri::Yes), Backend::Arch);
    }
```

Delete the now-contradicted `defaults_to_wsl`, `native_aliases`,
`unknown_falls_back_to_wsl`, `detect_never_picks_native_without_docker`,
`detect_picks_native_when_a_native_server_is_already_there`,
`a_machine_with_docker_and_no_distro_gets_native`,
`a_working_wsl_user_is_left_where_they_are`,
`a_probe_that_could_not_answer_is_not_evidence_of_no_distro`,
`resolve_env_outranks_everything`, `resolve_auto_in_file_means_detect_not_wsl`,
`resolve_absent_file_value_means_detect`,
`resolve_auto_in_the_ENV_also_means_detect` and
`resolve_typo_in_file_is_wsl_not_detect` — every one of them asserts the old
Wsl-default contract. `resolve_ignores_empty_env_and_falls_through_to_file`
stays, with its `Some("native")` file value unchanged.

- [ ] **Step 2: Run the tests to confirm they fail**

```
cargo test -p dml-core --lib backend::
```

Expected: FAIL — `Backend::Arch` does not exist yet (compile error `no variant
or associated item named 'Arch' found`).

- [ ] **Step 3: Add the variant and rewrite the three functions**

Replace `crates/dml-core/src/backend.rs:24-92` with:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// THE supported backend: the `dml-arch` WSL distro hosting its own
    /// `dockerd`, with the Rust `dml-wow` binary running INSIDE it.
    Arch,
    /// Retired as a runtime path. Kept so an existing `launcher.json` still
    /// parses; `from_override` maps it to [`Backend::Arch`], which names the
    /// same distro and the same daemon.
    Wsl,
    /// Docker Desktop on the Windows host. Kept working as a fallback, never
    /// extended.
    Native,
}

/// Pure parse of the backend choice from an override value.
///
/// Everything that is not explicitly Docker Desktop resolves to
/// [`Backend::Arch`] — including `wsl`, an empty string, and a typo. That is a
/// reversal of the old rule (unknown → Wsl) and it is deliberate: Arch is now
/// the backend the launcher can provision from nothing, so it is the safe
/// place for an unrecognised value to land. Sending a typo to Native would
/// point the user at a Docker Desktop they may not have installed.
pub fn from_override(value: Option<&str>) -> Backend {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("native") | Some("docker") => Backend::Native,
        _ => Backend::Arch,
    }
}

/// The backend selected for this process, read from `DML_BACKEND`.
pub fn selected() -> Backend {
    from_override(std::env::var(BACKEND_ENV).ok().as_deref())
}

/// Which backend a machine looks like it wants.
///
/// `distro_usable` answers "is `dml-arch` registered?" and is a [`Tri`]
/// because a probe that could not answer is evidence of nothing.
///
/// | distro usable | native dir | docker | → |
/// |---|---|---|---|
/// | `Yes` | — | — | **Arch** (a distro we can talk to IS the supported backend) |
/// | `No`/`Unknown` | yes | yes | Native (they have a working server; do not move them) |
/// | `No`/`Unknown` | otherwise | | **Arch** (the one backend we can build from nothing) |
///
/// The middle row is the one worth defending. A user with a server already
/// installed under Docker Desktop and no distro to move it to must not be
/// routed at a directory that does not exist. Everyone else — including the
/// fresh machine with neither — gets Arch, because Arch is provisionable and
/// Docker Desktop is a separate download with its own licence terms.
///
/// A user with BOTH a usable distro and a native server gets Arch, and can say
/// `DML_BACKEND=native` to say otherwise. That is the cost of having a default
/// at all, and it is one setting rather than a lost server.
pub fn detect(native_dir_exists: bool, docker_present: bool, distro_usable: Tri) -> Backend {
    if distro_usable == Tri::Yes {
        return Backend::Arch;
    }
    if native_dir_exists && docker_present {
        return Backend::Native;
    }
    Backend::Arch
}
```

In `resolve` (line 99 onward), change only the doc comment's parameter name
from `wsl_usable` to `distro_usable` and the signature accordingly; the body is
unchanged.

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p dml-core --lib backend::
```

Expected: PASS.

- [ ] **Step 5: Fix every non-exhaustive match the new variant broke**

```
cargo build --workspace 2>&1 | grep -n "non-exhaustive\|E0004" | head -20
```

Every site that matches on `Backend` now needs an `Arch` arm. Until Task 3
lands, route `Backend::Arch` exactly where `Backend::Wsl` goes at each site —
they target the same distro, and Task 3 is what makes the binary differ.

- [ ] **Step 6: Commit**

```bash
git add crates/dml-core/src/backend.rs
git commit -m "feat(core): Backend::Arch, and it is the default

Arch is the backend the launcher can provision from nothing, so it is where an
unrecognised value now lands. DML_BACKEND=wsl resolves to Arch (same distro,
same daemon); a user with a Docker Desktop server and no distro stays on Native."
```

---

### Task 3: `DmlRunner::arch()`

**Files:**
- Modify: `crates/dml-core/src/runner.rs:31-32` (add `ARCH_BINARY`)
- Modify: `crates/dml-core/src/runner.rs:119-142` (add `arch()`, route `for_backend`)
- Test: `crates/dml-core/src/runner.rs` `mod tests`

**Interfaces:**
- Consumes: `Backend::Arch` from Task 2.
- Produces: `runner::ARCH_BINARY: &str` (`"dml-wow"`); `DmlRunner::arch() ->
  DmlRunner`; `DmlRunner::for_backend(Backend::Arch)` returns it. Field shapes
  are unchanged (`program`, `prefix_args`, `path_prepend`, `host_label`,
  `host_hint`).

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/dml-core/src/runner.rs`:

```rust
    #[test]
    fn the_arch_runner_spawns_the_rust_binary_through_exec() {
        let r = DmlRunner::arch();
        assert_eq!(r.program, OsString::from("wsl.exe"));
        assert_eq!(
            r.prefix_args,
            vec![
                "-d".to_string(),
                DISTRO.to_string(),
                "-u".to_string(),
                USER.to_string(),
                "--exec".to_string(),
                ARCH_BINARY.to_string(),
            ]
        );
    }

    /// `--exec` is not a style preference. Verified 2026-07-28: `wsl -- `
    /// runs a shell, which splits on `;`, expands `$HOME` and globs `*`
    /// against the cwd. Title ids and paths cross this boundary.
    #[test]
    fn the_arch_runner_never_uses_the_shell_form() {
        let r = DmlRunner::arch();
        assert!(
            !r.prefix_args.iter().any(|a| a == "--"),
            "the bare -- form runs a shell; use --exec: {:?}",
            r.prefix_args
        );
        assert!(r.prefix_args.iter().any(|a| a == "--exec"));
    }

    #[test]
    fn the_arch_runner_inherits_path_and_blames_the_distro() {
        let r = DmlRunner::arch();
        // The distro has its own PATH; prepending a Windows dir would be
        // meaningless inside it.
        assert!(r.path_prepend.is_none());
        assert_eq!(r.host_label, "arch");
        assert!(
            r.host_hint.contains("dml-arch"),
            "a diagnostic must name the distro it is about: {}",
            r.host_hint
        );
    }

    #[test]
    fn for_backend_routes_arch_and_wsl_to_the_rust_binary() {
        // Backend::Wsl is retired as a runtime path — nothing may route to the
        // bash CLI any more.
        for b in [Backend::Arch, Backend::Wsl] {
            let r = DmlRunner::for_backend(b);
            assert_eq!(r.host_label, "arch", "{b:?} must use the Arch runner");
            assert!(r.prefix_args.iter().any(|a| a == ARCH_BINARY));
        }
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

```
cargo test -p dml-core --lib runner::
```

Expected: FAIL — `no function or associated item named 'arch'`.

- [ ] **Step 3: Implement**

Add after `crates/dml-core/src/runner.rs:32`:

```rust
/// The Rust CLI's program name inside the distro. Deployed to
/// `/usr/local/bin/dml-wow` by `provision.rs`; invoked as a bare name so the
/// distro's own PATH resolves it.
pub const ARCH_BINARY: &str = "dml-wow";
```

Add to `impl DmlRunner`, immediately before `native()`:

```rust
    /// THE supported backend: the Rust CLI running INSIDE `dml-arch`, against
    /// that distro's own `dockerd`. No Docker Desktop, no bash middleman.
    ///
    /// `--exec` rather than `--` is load-bearing rather than stylistic: the
    /// bare form runs a shell inside the distro, which splits on `;`, expands
    /// `$HOME` and globs `*` against the cwd (verified 2026-07-28). Title ids
    /// and paths cross this boundary.
    ///
    /// `path_prepend` stays `None`: the distro has its own PATH, and a Windows
    /// directory prepended to it would be meaningless.
    pub fn arch() -> Self {
        DmlRunner {
            program: "wsl.exe".into(),
            prefix_args: ["-d", DISTRO, "-u", USER, "--exec", ARCH_BINARY]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            path_prepend: None,
            host_label: "arch",
            host_hint: "Check the distro: wsl -d dml-arch -u dml --exec dml-wow version",
        }
    }
```

Replace `for_backend` (line 137-142) with:

```rust
    /// Construct the runner for the selected backend.
    ///
    /// `Wsl` routes here too: it is retired as a runtime path, and it named the
    /// same distro and the same daemon this runner talks to. Nothing routes to
    /// the bash CLI any more — `cli/dml` survives only as the oracle the parity
    /// suites diff against.
    pub fn for_backend(b: Backend) -> Self {
        match b {
            Backend::Arch | Backend::Wsl => Self::arch(),
            Backend::Native => Self::native(),
        }
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p dml-core --lib runner::
cargo build --workspace
```

Expected: PASS, clean build.

- [ ] **Step 5: Prove the `--exec` test is not vacuous**

Change `"--exec"` to `"--"` in `arch()`, re-run
`the_arch_runner_never_uses_the_shell_form`, confirm red, revert.

- [ ] **Step 6: Commit**

```bash
git add crates/dml-core/src/runner.rs
git commit -m "feat(core): DmlRunner::arch — the Rust CLI inside the distro

wsl.exe -d dml-arch -u dml --exec dml-wow. --exec, not --: the bare form runs a
shell that splits on ; and globs, and title ids cross this boundary. Backend::Wsl
routes here too — nothing spawns the bash CLI any more."
```

---

### Task 4: Engine control — Desktop vs systemd

`docker_program()` already resolves correctly inside the distro: its Windows
candidates come from `LOCALAPPDATA`/`ProgramFiles`, which do not exist on Linux,
so it already falls through to a bare `docker`. `engine_running()` runs `docker
info`, which also already works against a local socket. **Only starting and
stopping the engine is Desktop-specific**, so that is all this task changes.

**Files:**
- Modify: `crates/dml-core/src/engine.rs` (add `EngineKind`, the systemd argv
  builders, `start_engine_systemd`, `engine_running_tri`)
- Test: `crates/dml-core/src/engine.rs` `mod tests`

**Interfaces:**
- Consumes: `Backend` (Task 2), `proc::run_bounded_outcome`, `setup::Tri`.
- Produces:
  - `engine::EngineKind { Desktop, Systemd }`
  - `engine::EngineKind::for_backend(Backend) -> EngineKind`
  - `engine::SYSTEMCTL_PROGRAM: &str` (`"systemctl"`), `engine::SUDO_PROGRAM: &str` (`"sudo"`)
  - `engine::systemd_is_active_argv() -> [&'static str; 3]`
  - `engine::systemd_start_argv() -> [&'static str; 4]`
  - `engine::start_engine_systemd() -> std::io::Result<std::process::Output>`
  - `engine::engine_running_tri(program: &OsStr) -> Tri`

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/dml-core/src/engine.rs`:

```rust
    #[test]
    fn engine_kind_follows_the_backend() {
        use crate::backend::Backend;
        assert_eq!(EngineKind::for_backend(Backend::Arch), EngineKind::Systemd);
        assert_eq!(EngineKind::for_backend(Backend::Wsl), EngineKind::Systemd);
        assert_eq!(EngineKind::for_backend(Backend::Native), EngineKind::Desktop);
    }

    #[test]
    fn systemd_probe_argv_is_quiet_and_names_the_unit() {
        assert_eq!(systemd_is_active_argv(), ["is-active", "--quiet", "docker"]);
    }

    /// `sudo -n` is the whole point. Without it a distro whose NOPASSWD rule is
    /// missing does not fail — it BLOCKS on a password prompt that no button
    /// can answer, and the caller sees a timeout whose cause is invisible.
    #[test]
    fn the_systemd_start_refuses_rather_than_prompting_for_a_password() {
        let argv = systemd_start_argv();
        assert_eq!(argv, ["-n", "systemctl", "start", "docker"]);
        assert_eq!(SUDO_PROGRAM, "sudo");
    }

    /// A missing docker binary and a stopped engine are different repairs.
    /// `engine_running -> bool` collapses them; this one must not.
    #[test]
    fn engine_running_tri_reports_a_missing_program_as_no_not_unknown() {
        use std::ffi::OsString;
        let got = engine_running_tri(&OsString::from("definitely-not-docker-9f2.exe"));
        assert_eq!(got, crate::setup::Tri::No, "a program that is not installed is a definitive no");
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

```
cargo test -p dml-core --lib engine::
```

Expected: FAIL — `cannot find type 'EngineKind' in this scope`.

- [ ] **Step 3: Implement**

Add to `crates/dml-core/src/engine.rs`, after `docker_desktop_program()`
(line 115-124):

```rust
/// How the container engine is started and stopped. This is the ONLY part of
/// engine control that differs by backend: `docker_program()` already falls
/// through to a bare `docker` inside the distro (its Windows candidates come
/// from `LOCALAPPDATA`/`ProgramFiles`, which do not exist on Linux), and
/// `docker info` answers against a local socket exactly as it does against
/// Docker Desktop's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    /// `docker desktop start|stop` on the Windows host.
    Desktop,
    /// `dockerd` as a systemd unit inside the distro.
    Systemd,
}

impl EngineKind {
    pub fn for_backend(b: crate::backend::Backend) -> Self {
        match b {
            crate::backend::Backend::Arch | crate::backend::Backend::Wsl => EngineKind::Systemd,
            crate::backend::Backend::Native => EngineKind::Desktop,
        }
    }
}

pub const SYSTEMCTL_PROGRAM: &str = "systemctl";
pub const SUDO_PROGRAM: &str = "sudo";

/// `systemctl is-active --quiet docker` — exit 0 means the unit is running.
/// Pure, for tests.
pub fn systemd_is_active_argv() -> [&'static str; 3] {
    ["is-active", "--quiet", "docker"]
}

/// `sudo -n systemctl start docker`. Pure, for tests.
///
/// `-n` is load-bearing. Without it, a distro whose NOPASSWD sudoers rule is
/// missing does not fail — `sudo` blocks on a password prompt that no button in
/// the launcher can answer, and the caller sees a bare timeout with no cause.
/// With `-n` the same machine fails immediately and says so.
pub fn systemd_start_argv() -> [&'static str; 4] {
    ["-n", "systemctl", "start", "docker"]
}

/// Start the in-distro daemon. Bounded by the same [`ENGINE_START_ASK_TIMEOUT`]
/// as the Desktop ask, and returns the same shape so
/// [`start_engine_succeeded`] classifies both identically.
pub fn start_engine_systemd() -> std::io::Result<std::process::Output> {
    let mut cmd = std::process::Command::new(SUDO_PROGRAM);
    cmd.args(systemd_start_argv());
    windows_no_window(&mut cmd);
    match crate::proc::run_bounded_outcome(cmd, ENGINE_START_ASK_TIMEOUT) {
        crate::proc::BoundedOutcome::Ran(out) => Ok(out),
        crate::proc::BoundedOutcome::SpawnFailed(e) => Err(e),
        crate::proc::BoundedOutcome::TimedOut => Err(std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "systemctl start docker did not answer",
        )),
    }
}

/// Is the engine up, as a [`Tri`]?
///
/// [`engine_running`] returns `bool` and so cannot distinguish "docker is not
/// installed" (a definitive no, with a repair) from "the probe fell over"
/// (evidence of nothing, with only a retry). The setup chain needs both apart.
pub fn engine_running_tri(program: &OsStr) -> crate::setup::Tri {
    let mut cmd = std::process::Command::new(program);
    cmd.args(docker_info_args());
    windows_no_window(&mut cmd);
    match crate::setup::ProbeOutcome::from_bounded(crate::proc::run_bounded_outcome(
        cmd,
        crate::setup::DEFAULT_PROBE_TIMEOUT,
    )) {
        crate::setup::ProbeOutcome::ProgramMissing => crate::setup::Tri::No,
        crate::setup::ProbeOutcome::CouldNotTell => crate::setup::Tri::Unknown,
        crate::setup::ProbeOutcome::Ran { code, .. } => {
            if code == Some(0) {
                crate::setup::Tri::Yes
            } else {
                crate::setup::Tri::No
            }
        }
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p dml-core --lib engine::
```

Expected: PASS.

- [ ] **Step 5: Verify the systemd start works against the real distro**

```
wsl -d dml-arch -u dml --exec sudo -n systemctl start docker
wsl -d dml-arch -u dml --exec systemctl is-active --quiet docker
echo $?
```

Expected: exit 0 from both. If `sudo -n` fails, the NOPASSWD rule is missing —
that is Task 6's job, note it and continue.

- [ ] **Step 6: Commit**

```bash
git add crates/dml-core/src/engine.rs
git commit -m "feat(core): EngineKind — systemd dockerd alongside Docker Desktop

Only start/stop differ by backend: docker_program() already falls through to a
bare docker on Linux and docker info already answers a local socket. sudo -n so
a missing NOPASSWD rule fails loudly instead of blocking on a prompt no button
can answer. engine_running_tri keeps 'not installed' apart from 'could not ask'."
```

---

### Task 5: The Linux default games directory

`games_dir_from_env()` falls back to `"."`, which was survivable when the
launcher always exported `DML_GAMES_DIR`. Inside the distro the binary is
spawned directly and the fallback becomes the real answer, so `.` would put a
server wherever the process happened to start.

**Files:**
- Modify: `crates/dml-core/src/compose.rs:16-21`
- Test: `crates/dml-core/src/compose.rs` `mod tests`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `compose::games_dir_from(env_value: Option<OsString>, home:
  Option<OsString>) -> PathBuf` (new, pure); `games_dir_from_env() -> PathBuf`
  keeps its signature and becomes a thin reader that calls it. `DML_GAMES_DIR`
  remains the override seam every test suite uses.

**Why a pure core:** the decision is worth testing on both platforms, and the
only alternative is `std::env::set_var` inside a test — which mutates
process-global state that every other test in the same binary shares. Cargo
runs tests in parallel by default, so that shape is a flake generator, and it
would flake in the one direction that looks like a real failure.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/dml-core/src/compose.rs`:

```rust
    use std::ffi::OsString;

    fn os(s: &str) -> Option<OsString> {
        Some(OsString::from(s))
    }

    /// The override seam every parity/bats/integration suite injects.
    #[test]
    fn the_env_var_wins_over_everything() {
        // Forward slashes work on BOTH platforms; a backslash literal would be
        // Windows-only (test-portability rule).
        assert_eq!(
            games_dir_from(os("/tmp/dml-games-test"), os("/home/dml")),
            PathBuf::from("/tmp/dml-games-test")
        );
    }

    /// Empty is not a value. `:-`-style "empty means unset" has bitten this
    /// repo before (the tailscale stub, 2026-07-29), so pin the direction.
    #[test]
    fn an_empty_env_var_falls_through_rather_than_resolving_to_nothing() {
        assert_eq!(games_dir_from(os(""), os("/home/dml")), PathBuf::from("/home/dml/games"));
    }

    /// Inside the distro nothing exports DML_GAMES_DIR, so the fallback IS the
    /// answer. `.` would put a server wherever the process happened to start.
    #[test]
    fn the_fallback_is_the_home_games_dir_not_the_cwd() {
        let got = games_dir_from(None, os("/home/dml"));
        assert_eq!(got, PathBuf::from("/home/dml/games"));
        assert_ne!(got, PathBuf::from("."), "a cwd-relative default is the bug this fixes");
    }

    /// No env var and no home is the one case with nothing to go on. `.` is
    /// the honest answer there — inventing a path would be worse.
    #[test]
    fn no_home_and_no_override_is_still_the_cwd() {
        assert_eq!(games_dir_from(None, None), PathBuf::from("."));
        assert_eq!(games_dir_from(None, os("")), PathBuf::from("."));
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

```
cargo test -p dml-core --lib compose::
```

Expected: FAIL to compile — `cannot find function 'games_dir_from'`.

- [ ] **Step 3: Implement**

Replace `crates/dml-core/src/compose.rs:16-21` with:

```rust
/// The pure decision behind [`games_dir_from_env`].
///
/// Split out so both branches are testable on both platforms without
/// `std::env::set_var`, which mutates process-global state every other test in
/// the binary shares — and cargo runs them in parallel.
///
/// Empty is NOT a value: an empty `DML_GAMES_DIR` falls through to the home
/// default rather than resolving to nothing. Treating empty as set is the
/// `${VAR:-default}` trap this repo hit on 2026-07-29, where a test that set a
/// stub's value empty to mean "printed nothing" silently got the default back
/// and proved nothing.
pub fn games_dir_from(env_value: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> PathBuf {
    if let Some(dir) = env_value.filter(|s| !s.is_empty()) {
        return PathBuf::from(dir);
    }
    // Inside the distro nothing exports DML_GAMES_DIR — the binary is spawned
    // directly by the launcher — so this fallback IS the answer, and a
    // cwd-relative "." would put a server wherever the process happened to
    // start. The spec fixes the server directory at ~/games (Linux ext4, not
    // /mnt/c: the AzerothCore compile is thousands of small-file writes and
    // drvfs is far slower for that).
    if let Some(home) = home.filter(|s| !s.is_empty()) {
        return PathBuf::from(home).join("games");
    }
    PathBuf::from(".")
}

pub fn games_dir_from_env() -> PathBuf {
    // `HOME` is the distro's own variable. On Windows it is usually absent,
    // which is correct: there the launcher exports DML_GAMES_DIR and the home
    // branch must not fire.
    games_dir_from(std::env::var_os("DML_GAMES_DIR"), std::env::var_os("HOME"))
}
```

- [ ] **Step 4: Run the test to verify it passes**

```
cargo test -p dml-core --lib compose::
```

Expected: PASS on both platforms — all four tests are platform-independent,
which is the point of the pure split.

- [ ] **Step 5: Confirm no caller regressed**

```
cargo test --workspace
```

Expected: PASS. `games_dir_from_env` kept its signature, so this is checking
that no test anywhere depended on the old `"."` fallback.

- [ ] **Step 6: Commit**

```bash
git add crates/dml-core/src/compose.rs
git commit -m "feat(core): ~/games is the Linux default games dir

Inside the distro nothing exports DML_GAMES_DIR, so the fallback is the real
answer and '.' would put a server wherever the process started. The env var
remains the override seam every test suite injects."
```

---

### Task 6: Distro creation and first boot

Pure argv builders and an ordered step list. No spawning in this task — the
execution seam is Task 9's job, and keeping the decisions pure is what makes
the ordering testable without owning a machine in each state.

**Files:**
- Create: `crates/dml-core/src/distro.rs`
- Modify: `crates/dml-core/src/lib.rs` (add `pub mod distro;`)
- Test: inside `crates/dml-core/src/distro.rs`

**Interfaces:**
- Consumes: `runner::{DISTRO, USER}`.
- Produces:
  - `distro::CATALOG_NAME: &str` (`"archlinux"`)
  - `distro::PACKAGES: [&str; 4]`
  - `distro::install_distro_argv(name: &str) -> Vec<String>`
  - `distro::WSL_CONF: &str`
  - `distro::terminate_argv(name: &str) -> Vec<String>`
  - `distro::set_default_user_argv(name: &str, user: &str) -> Vec<String>`
  - `distro::FirstBootStep { id: &'static str, as_root: bool, argv: Vec<String> }`
  - `distro::first_boot_steps(user: &str) -> Vec<FirstBootStep>`

- [ ] **Step 1: Write the failing tests**

Create `crates/dml-core/src/distro.rs` containing ONLY this test module plus a
`use super::*;` — the implementation arrives in Step 3:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_install_comes_from_the_official_catalog_and_does_not_launch_a_shell() {
        let argv = install_distro_argv("dml-arch");
        assert_eq!(
            argv,
            vec![
                "--install".to_string(),
                "archlinux".to_string(),
                "--name".to_string(),
                "dml-arch".to_string(),
                "--no-launch".to_string(),
            ]
        );
    }

    /// `--no-launch` is not a nicety. Without it `wsl --install` starts the
    /// distro's interactive first-run account setup, which waits on a console
    /// nobody is attached to — the launcher would hang with nothing on screen.
    #[test]
    fn the_install_never_opens_an_interactive_first_run() {
        assert!(install_distro_argv("x").iter().any(|a| a == "--no-launch"));
    }

    #[test]
    fn systemd_is_switched_on_in_wsl_conf() {
        assert!(WSL_CONF.contains("[boot]"));
        assert!(WSL_CONF.contains("systemd=true"));
        // LF only: bash inside WSL chokes on CRLF (.gitattributes enforces the
        // same rule for every shell file in this repo).
        assert!(!WSL_CONF.contains('\r'), "wsl.conf must be LF-only");
    }

    #[test]
    fn buildx_is_installed_because_progress_and_resume_depend_on_it() {
        // install_native.rs's pct parser reads BuildKit vertex headers and
        // resume rests on BuildKit's cache. Without buildx the build silently
        // falls back to the legacy builder: the progress bar goes dead and the
        // failure reads as a hang.
        assert!(PACKAGES.contains(&"docker-buildx"));
        assert!(PACKAGES.contains(&"docker"));
        assert!(PACKAGES.contains(&"docker-compose"));
        assert!(PACKAGES.contains(&"git"));
    }

    #[test]
    fn first_boot_order_creates_the_user_before_it_needs_one() {
        let ids: Vec<&str> = first_boot_steps("dml").iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            vec!["wsl-conf", "useradd", "sudoers", "pacman-sync", "docker-group", "docker-enable"]
        );
    }

    /// First boot runs as root and CANNOT use sudo: the sudoers drop-in is
    /// itself one of these steps, so anything invoking `sudo` before it lands
    /// would prompt for a password on a console nobody is attached to. That is
    /// the invariant — not the tautology that a hardcoded `root(...)` helper
    /// returns `as_root: true`.
    #[test]
    fn no_first_boot_step_reaches_for_a_sudo_that_does_not_exist_yet() {
        for step in first_boot_steps("dml") {
            assert!(step.as_root, "{} must run as root", step.id);
            assert!(
                !step.argv.iter().any(|a| a == "sudo"),
                "{} invokes sudo, but the sudoers drop-in is step 3 of this very list: {:?}",
                step.id,
                step.argv
            );
        }
    }

    #[test]
    fn the_sudoers_rule_is_nopasswd_and_scoped_to_the_user() {
        let step = first_boot_steps("dml").into_iter().find(|s| s.id == "sudoers").unwrap();
        let joined = step.argv.join(" ");
        assert!(joined.contains("dml ALL=(ALL) NOPASSWD: ALL"), "got {joined}");
        assert!(
            joined.contains("/etc/sudoers.d/"),
            "must be a drop-in, never an edit of /etc/sudoers: {joined}"
        );
    }

    #[test]
    fn pacman_never_waits_for_a_confirmation_nobody_can_give() {
        let step = first_boot_steps("dml").into_iter().find(|s| s.id == "pacman-sync").unwrap();
        assert!(step.argv.iter().any(|a| a == "--noconfirm"), "got {:?}", step.argv);
    }

    #[test]
    fn set_default_user_uses_manage_not_a_config_edit() {
        assert_eq!(
            set_default_user_argv("dml-arch", "dml"),
            vec![
                "--manage".to_string(),
                "dml-arch".to_string(),
                "--set-default-user".to_string(),
                "dml".to_string(),
            ]
        );
    }

    #[test]
    fn terminate_argv_names_the_distro() {
        assert_eq!(terminate_argv("dml-arch"), vec!["--terminate".to_string(), "dml-arch".to_string()]);
    }
}
```

- [ ] **Step 2: Run the tests to confirm they fail**

```
cargo test -p dml-core --lib distro::
```

Expected: FAIL to compile — nothing in scope. (Add `pub mod distro;` to
`crates/dml-core/src/lib.rs` first, alphabetically after `dashboard`.)

- [ ] **Step 3: Implement**

Put this above the test module in `crates/dml-core/src/distro.rs`:

```rust
//! Creating and preparing the `dml-arch` distro.
//!
//! Pure argv builders and one ordered step list, deliberately with no spawning
//! of its own: the ORDER is the part worth testing, and it is worth testing
//! without owning a machine that happens to be in each state. The execution
//! seam lives in the launcher's `provision.rs`.
//!
//! Flags verified against WSL 2.7.10 on 2026-08-04: `--install <distro>
//! --name --no-launch --location --vhd-size --web-download`, and
//! `--manage <distro> --set-default-user`.

/// The official catalog name (`wsl --list --online`). Not a third-party rootfs:
/// the spec's decision 4 is a catalog install, so there is no artifact to host,
/// verify or keep patched.
pub const CATALOG_NAME: &str = "archlinux";

/// What the backend needs, pinned known-good on 2026-08-04:
/// docker `1:29.6.1-1`, docker-compose `5.3.1-1`, docker-buildx `0.35.0-1`.
///
/// `docker-buildx` is REQUIRED. `install_native.rs`'s `pct` progress parser
/// reads BuildKit vertex headers out of the streamed build output, and install
/// resume rests on BuildKit's cache. Without it the build falls back to the
/// legacy builder, the progress bar goes silent and resume degrades — a
/// failure that presents as a hang rather than as a missing package.
pub const PACKAGES: [&str; 4] = ["docker", "docker-compose", "docker-buildx", "git"];

/// `wsl --install archlinux --name <name> --no-launch`.
///
/// `--no-launch` is load-bearing: without it `wsl --install` starts the
/// distro's interactive first-run account setup, which waits on a console
/// nobody is attached to. The launcher would hang with nothing on screen.
pub fn install_distro_argv(name: &str) -> Vec<String> {
    vec![
        "--install".to_string(),
        CATALOG_NAME.to_string(),
        "--name".to_string(),
        name.to_string(),
        "--no-launch".to_string(),
    ]
}

/// `/etc/wsl.conf`. LF only — bash inside WSL chokes on CRLF, which is why
/// `.gitattributes` forces LF on every shell file in this repo.
pub const WSL_CONF: &str = "[boot]\nsystemd=true\n";

/// `wsl --terminate <name>` — required after writing `wsl.conf`, because
/// systemd only comes up on the next boot of the distro.
pub fn terminate_argv(name: &str) -> Vec<String> {
    vec!["--terminate".to_string(), name.to_string()]
}

/// `wsl --manage <name> --set-default-user <user>`. Preferred over editing the
/// `[user]` section of `wsl.conf` by hand: it is the documented API, and it
/// cannot corrupt a file the rest of this module also writes.
pub fn set_default_user_argv(name: &str, user: &str) -> Vec<String> {
    vec![
        "--manage".to_string(),
        name.to_string(),
        "--set-default-user".to_string(),
        user.to_string(),
    ]
}

/// One first-boot step, run inside the distro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FirstBootStep {
    /// Stable id — the wire name in the streamed NDJSON and the key the
    /// ordering test asserts on.
    pub id: &'static str,
    /// Whether this step needs `-u root`. Every step here does: they all run
    /// before the unprivileged user has sudo rights, or they configure sudo
    /// itself.
    pub as_root: bool,
    /// argv AFTER `wsl.exe -d <name> -u <who> --exec`.
    pub argv: Vec<String>,
}

/// The ordered first-boot sequence, root-only by construction.
///
/// Order is the contract: the sudoers drop-in cannot be written for a user that
/// does not exist, and `usermod -aG docker` cannot add a group member before
/// the `docker` package has created the group.
pub fn first_boot_steps(user: &str) -> Vec<FirstBootStep> {
    let root = |id: &'static str, argv: Vec<String>| FirstBootStep { id, as_root: true, argv };
    let s = |v: &str| v.to_string();
    vec![
        // `printf %s` rather than a heredoc: this argv crosses `--exec`, so
        // there is no shell to interpret one, and `printf` writes the exact
        // bytes with no trailing surprise.
        root(
            "wsl-conf",
            vec![
                s("sh"),
                s("-c"),
                format!("printf %s '{WSL_CONF}' > /etc/wsl.conf"),
            ],
        ),
        root("useradd", vec![s("useradd"), s("-m"), s("-G"), s("wheel"), user.to_string()]),
        root(
            "sudoers",
            vec![
                s("sh"),
                s("-c"),
                format!(
                    "printf %s '{user} ALL=(ALL) NOPASSWD: ALL\n' > /etc/sudoers.d/99-dml && chmod 0440 /etc/sudoers.d/99-dml"
                ),
            ],
        ),
        root(
            "pacman-sync",
            {
                let mut v = vec![s("pacman"), s("-Syu"), s("--noconfirm"), s("--needed")];
                v.extend(PACKAGES.iter().map(|p| p.to_string()));
                v
            },
        ),
        root("docker-group", vec![s("usermod"), s("-aG"), s("docker"), user.to_string()]),
        root("docker-enable", vec![s("systemctl"), s("enable"), s("--now"), s("docker")]),
    ]
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p dml-core --lib distro::
```

Expected: PASS (10 tests).

- [ ] **Step 5: Prove the ordering test is not vacuous**

Swap `useradd` and `sudoers` in the returned vector, re-run
`first_boot_order_creates_the_user_before_it_needs_one`, confirm red, revert.

- [ ] **Step 6: Commit**

```bash
git add crates/dml-core/src/distro.rs crates/dml-core/src/lib.rs
git commit -m "feat(core): distro creation and first-boot steps, pure

wsl --install archlinux --name <n> --no-launch (without --no-launch the
interactive first-run setup waits on a console nobody is attached to), systemd
via wsl.conf, a NOPASSWD sudoers drop-in, and pacman -Syu of the four packages.
docker-buildx is in the list because install_native's pct parser reads BuildKit
vertex headers and resume rests on BuildKit's cache."
```

---

### Task 7: The Arch setup chain

`derive` (WSL) and `derive_native` (Docker Desktop) already exist and share the
"first missing link wins" contract. This adds the third.

**Files:**
- Modify: `crates/dml-core/src/setup.rs` (add `ArchFacts`, `derive_arch`, `probe_arch_with`)
- Test: `crates/dml-core/src/setup.rs` `mod tests`

**Interfaces:**
- Consumes: `Tri`, `SetupState`, `SetupStep`, `Probes`, `BackendStatus`,
  `ProbeOutcome`, `ProbeBudget`, `classify_wsl_list`, `classify_cli_version`,
  `classify_titles`, `cli_version_matches`, `EXPECTED_CLI_VERSION`.
- Produces:
  - `setup::ArchFacts { wsl: Tri, distro: Tri, dockerd: Tri, cli: Tri, cli_version: Option<String>, titles: Option<usize>, detail: Option<String> }`
  - `setup::derive_arch(distro: &str, f: ArchFacts) -> BackendStatus`
  - `setup::probe_arch_with(distro: &str, user: &str, run: impl FnMut(&[&str], ProbeBudget) -> ProbeOutcome) -> BackendStatus`
  - Reuses the existing `SetupState` variants; `SetupStep::Engine` names the
    dockerd link.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `crates/dml-core/src/setup.rs`:

```rust
    fn arch_ok() -> ArchFacts {
        ArchFacts {
            wsl: Tri::Yes,
            distro: Tri::Yes,
            dockerd: Tri::Yes,
            cli: Tri::Yes,
            cli_version: Some(EXPECTED_CLI_VERSION.to_string()),
            titles: Some(1),
            detail: None,
        }
    }

    #[test]
    fn arch_all_green_is_ready() {
        assert_eq!(derive_arch(DISTRO, arch_ok()).state, SetupState::Ready);
    }

    #[test]
    fn arch_chain_stops_at_the_first_missing_link() {
        let no_wsl = ArchFacts { wsl: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_wsl).state, SetupState::NoWsl);

        let no_distro = ArchFacts { distro: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_distro).state, SetupState::NoDistro);

        let no_dockerd = ArchFacts { dockerd: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_dockerd).state, SetupState::DockerStopped);

        let no_cli = ArchFacts { cli: Tri::No, ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_cli).state, SetupState::NoCli);

        let old_cli = ArchFacts { cli_version: Some("2.6.0".to_string()), ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, old_cli).state, SetupState::CliOutdated);

        let no_titles = ArchFacts { titles: Some(0), ..arch_ok() };
        assert_eq!(derive_arch(DISTRO, no_titles).state, SetupState::NoTitles);
    }

    /// Tri-state discipline: each unanswered link must name ITSELF, so the
    /// screen can say which question went dark instead of "something failed".
    #[test]
    fn an_unanswered_link_blocks_at_that_link() {
        for (facts, step) in [
            (ArchFacts { wsl: Tri::Unknown, ..arch_ok() }, SetupStep::Wsl),
            (ArchFacts { distro: Tri::Unknown, ..arch_ok() }, SetupStep::Distro),
            (ArchFacts { dockerd: Tri::Unknown, ..arch_ok() }, SetupStep::Engine),
            (ArchFacts { cli: Tri::Unknown, ..arch_ok() }, SetupStep::Cli),
            (ArchFacts { titles: None, ..arch_ok() }, SetupStep::Titles),
        ] {
            let got = derive_arch(DISTRO, facts);
            assert_eq!(got.state, SetupState::Unknown, "{step:?}");
            assert_eq!(got.blocked_at, Some(step));
        }
    }

    /// A dockerd that is merely stopped is REPAIRABLE (start the unit); a
    /// question that went unanswered is not. Collapsing them would put a
    /// "start docker" button in front of a machine that never answered.
    #[test]
    fn a_dockerd_that_did_not_answer_is_not_a_stopped_dockerd() {
        let quiet = ArchFacts { dockerd: Tri::Unknown, ..arch_ok() };
        assert_ne!(derive_arch(DISTRO, quiet).state, SetupState::DockerStopped);
    }

    #[test]
    fn arch_probe_short_circuits_and_never_asks_a_question_it_cannot_answer() {
        // No distro: nothing may be asked INSIDE it.
        let mut asked: Vec<String> = Vec::new();
        let got = probe_arch_with(DISTRO, USER, |args, _| {
            asked.push(args.join(" "));
            ran(0, "Ubuntu\n")
        });
        assert_eq!(got.state, SetupState::NoDistro);
        assert_eq!(asked.len(), 1, "asked {asked:?}");
    }

    /// The in-distro probes must cross the boundary with --exec. The bare --
    /// form runs a shell there (verified 2026-07-28).
    #[test]
    fn every_in_distro_probe_uses_exec() {
        let mut asked: Vec<Vec<String>> = Vec::new();
        let _ = probe_arch_with(DISTRO, USER, |args, _| {
            asked.push(args.iter().map(|s| s.to_string()).collect());
            match args {
                a if a.contains(&"--list") => ran(0, "dml-arch\n"),
                a if a.contains(&"is-active") => ran(0, ""),
                a if a.contains(&"version") => ran(0, &format!("{{\"ok\":true,\"data\":{{\"version\":\"{EXPECTED_CLI_VERSION}\"}}}}")),
                _ => ran(0, "{\"ok\":true,\"data\":{\"games\":[]}}"),
            }
        });
        for argv in asked.iter().filter(|a| a.contains(&"-d".to_string())) {
            assert!(argv.iter().any(|a| a == "--exec"), "shell form in {argv:?}");
            assert!(!argv.iter().any(|a| a == "--"), "shell form in {argv:?}");
        }
    }
```

- [ ] **Step 2: Run the tests to confirm they fail**

```
cargo test -p dml-core --lib setup::
```

Expected: FAIL — `cannot find struct 'ArchFacts'`.

- [ ] **Step 3: Implement**

Add to `crates/dml-core/src/setup.rs`, after `derive_native`:

```rust
/// What the ARCH chain needs to know, in chain order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArchFacts {
    pub wsl: Tri,
    pub distro: Tri,
    /// Is the in-distro `dockerd` unit active? `Unknown` when the question
    /// itself went unanswered.
    pub dockerd: Tri,
    /// Is the Rust `dml-wow` binary present and runnable inside the distro?
    pub cli: Tri,
    pub cli_version: Option<String>,
    pub titles: Option<usize>,
    /// The verbatim words of whichever probe went dark.
    pub detail: Option<String>,
}

/// The Arch chain: WSL → distro → dockerd → `dml-wow` → a title.
///
/// Same discipline as [`derive`] and [`derive_native`]: the first unanswered or
/// missing link wins, so the consumer always has exactly one next step, and
/// `Unknown` is never read as absence. A dockerd that is merely stopped has a
/// repair (start the unit); a dockerd that did not answer has only a retry, and
/// putting the repair button in front of the second case is the exact mistake
/// the tri-state exists to prevent.
///
/// Reuses [`SetupState::DockerStopped`] and [`SetupStep::Engine`] rather than
/// minting Arch-specific twins: the user-facing sentence ("the container engine
/// is not running") and the repair ("start it") are the same on both backends,
/// and a second pair of states would mean every consumer grows a branch that
/// says the same thing twice.
pub fn derive_arch(distro: &str, f: ArchFacts) -> BackendStatus {
    let probes = Probes {
        wsl: f.wsl,
        distro: f.distro,
        cli: f.cli,
        cli_version: f.cli_version.clone(),
        titles: f.titles,
        detail: f.detail.clone(),
    };
    let unknown_at = |step: SetupStep| BackendStatus {
        state: SetupState::Unknown,
        blocked_at: Some(step),
        detail: f.detail.clone(),
        distro: distro.to_string(),
        expected_cli_version: EXPECTED_CLI_VERSION.to_string(),
        probes: probes.clone(),
    };
    let settled = |state: SetupState| BackendStatus {
        state,
        blocked_at: None,
        detail: None,
        distro: distro.to_string(),
        expected_cli_version: EXPECTED_CLI_VERSION.to_string(),
        probes: probes.clone(),
    };

    match f.wsl {
        Tri::Unknown => return unknown_at(SetupStep::Wsl),
        Tri::No => return settled(SetupState::NoWsl),
        Tri::Yes => {}
    }
    match f.distro {
        Tri::Unknown => return unknown_at(SetupStep::Distro),
        Tri::No => return settled(SetupState::NoDistro),
        Tri::Yes => {}
    }
    match f.dockerd {
        Tri::Unknown => return unknown_at(SetupStep::Engine),
        Tri::No => return settled(SetupState::DockerStopped),
        Tri::Yes => {}
    }
    match f.cli {
        Tri::Unknown => return unknown_at(SetupStep::Cli),
        Tri::No => return settled(SetupState::NoCli),
        Tri::Yes => {}
    }
    match f.cli_version.as_deref() {
        // A `Yes` with no version means a classifier broke its own contract.
        None => return unknown_at(SetupStep::Cli),
        Some(v) if !cli_version_matches(v) => return settled(SetupState::CliOutdated),
        Some(_) => {}
    }
    match f.titles {
        None => unknown_at(SetupStep::Titles),
        Some(0) => settled(SetupState::NoTitles),
        Some(_) => settled(SetupState::Ready),
    }
}

/// Run the Arch chain against an injected runner. SHORT-CIRCUITS: each link is
/// asked only when the one before it said `Yes`. "Is `dml-wow` installed inside
/// `dml-arch`" has no honest answer when there is no `dml-arch`, and every
/// skipped spawn is one fewer timeout a user with a sick machine sits through.
///
/// Every in-distro call uses `--exec`. The bare `--` form runs a shell inside
/// the distro, which splits on `;`, expands `$HOME` and globs (verified
/// 2026-07-28).
pub fn probe_arch_with(
    distro: &str,
    user: &str,
    mut run: impl FnMut(&[&str], ProbeBudget) -> ProbeOutcome,
) -> BackendStatus {
    let wsl = classify_wsl_list(&run(&["--list", "--quiet"], ProbeBudget::Warm), distro);
    let mut facts = ArchFacts {
        wsl: wsl.wsl,
        distro: wsl.distro,
        dockerd: Tri::Unknown,
        cli: Tri::Unknown,
        cli_version: None,
        titles: None,
        detail: wsl.detail,
    };

    if facts.distro == Tri::Yes {
        // THE cold-start call: the first thing to run inside the distro, so it
        // pays for booting the WSL2 VM and systemd.
        let out = run(
            &["-d", distro, "-u", user, "--exec", "systemctl", "is-active", "--quiet", "docker"],
            ProbeBudget::ColdStart,
        );
        facts.dockerd = match &out {
            ProbeOutcome::ProgramMissing => Tri::No,
            ProbeOutcome::CouldNotTell => Tri::Unknown,
            ProbeOutcome::Ran { code, .. } => {
                if *code == Some(0) {
                    Tri::Yes
                } else {
                    Tri::No
                }
            }
        };
        if facts.dockerd == Tri::Unknown {
            facts.detail = out.detail();
        }
    }

    if facts.dockerd == Tri::Yes {
        let cli = classify_cli_version(&run(
            &["-d", distro, "-u", user, "--exec", "dml-wow", "version", "--json"],
            ProbeBudget::Warm,
        ));
        facts.cli = cli.cli;
        facts.cli_version = cli.version;
        if cli.detail.is_some() {
            facts.detail = cli.detail;
        }

        let usable =
            facts.cli == Tri::Yes && facts.cli_version.as_deref().is_some_and(cli_version_matches);
        if usable {
            let out = run(
                &["-d", distro, "-u", user, "--exec", "dml-wow", "games", "list", "--json"],
                ProbeBudget::Warm,
            );
            facts.titles = classify_titles(&out);
            if facts.titles.is_none() {
                facts.detail = out.detail();
            }
        }
    }

    derive_arch(distro, facts)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p dml-core --lib setup::
```

Expected: PASS.

- [ ] **Step 5: Prove the short-circuit test is not vacuous**

Move the `dockerd` probe outside its `if facts.distro == Tri::Yes` guard,
re-run `arch_probe_short_circuits_and_never_asks_a_question_it_cannot_answer`,
confirm red (`asked.len()` becomes 2), revert.

- [ ] **Step 6: Run the whole workspace**

```
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/dml-core/src/setup.rs
git commit -m "feat(core): the Arch setup chain — wsl, distro, dockerd, binary, title

Third sibling of derive/derive_native, same 'first missing link wins' contract.
Reuses DockerStopped/Engine rather than minting Arch twins: the sentence and the
repair are the same on both backends. A dockerd that did not answer is not a
stopped dockerd — that distinction is the whole point of the Tri."
```

---

### Task 8: Build and bundle the Linux binary

**Files:**
- Modify: `.github/workflows/rust.yml` (linux job uploads the artifact)
- Modify: `launcher/src-tauri/src/payload.rs` (manifest gains the binary)
- Modify: `launcher/src-tauri/tauri.conf.json` (`bundle.resources`)
- Test: `launcher/src-tauri/src/payload.rs` `mod tests`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `payload::DML_WOW_BIN: &str` (`"backend/dml-wow"`);
  `payload::PayloadPaths` gains a `dml_wow_bin: PathBuf` field;
  `payload::PayloadStatus` gains `dml_wow_bin_present: bool`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `launcher/src-tauri/src/payload.rs`:

```rust
    /// The Arch backend cannot work without the Linux binary in the installer.
    /// This manifest is the only thing that carries it, and `resolve` reporting
    /// a present payload without it would be a lie the first run pays for.
    #[test]
    fn the_manifest_carries_the_linux_binary() {
        let dir = tmp_dir("payload-dml-wow");
        let p = paths(&dir);
        assert!(
            p.dml_wow_bin.ends_with("dml-wow"),
            "expected a dml-wow path, got {:?}",
            p.dml_wow_bin
        );
        assert_eq!(resolve(&dir).dml_wow_bin_present, false, "nothing written yet");

        std::fs::create_dir_all(p.dml_wow_bin.parent().unwrap()).unwrap();
        std::fs::write(&p.dml_wow_bin, b"#!/bin/true\n").unwrap();
        assert!(resolve(&dir).dml_wow_bin_present);
    }
```

If `tmp_dir` does not already exist in this test module, copy the helper from
`crates/dml-core/src/proc.rs:453-458` verbatim.

- [ ] **Step 2: Run the test to confirm it fails**

```
cargo test -p launcher --lib payload::
```

Expected: FAIL — `no field 'dml_wow_bin' on type 'PayloadPaths'`.

- [ ] **Step 3: Implement**

In `launcher/src-tauri/src/payload.rs`, add next to `CLI_SCRIPT` (line 49):

```rust
/// The Linux `dml-wow` binary, built by CI's ubuntu job. An ubuntu-built glibc
/// binary runs on Arch (older glibc build, newer host), so one artifact serves
/// both. This is the ONLY thing that carries the Arch backend onto a fresh PC.
pub const DML_WOW_BIN: &str = "backend/dml-wow";
```

Add `pub dml_wow_bin: PathBuf,` to `PayloadPaths` and
`pub dml_wow_bin_present: bool,` to `PayloadStatus`, then set them in `paths()`
(`dml_wow_bin: under(root, DML_WOW_BIN)`) and `resolve()`
(`dml_wow_bin_present: p.dml_wow_bin.is_file()`).

In `launcher/src-tauri/tauri.conf.json`, add `"backend/dml-wow"` to the
`bundle.resources` array.

In `.github/workflows/rust.yml`, append to the `linux` job's steps:

```yaml
      # The shipped artifact. The Windows installer bundles this binary and
      # provision.rs deploys it into the distro, so this job is no longer
      # advisory — a red linux job means there is nothing to ship.
      - run: cargo build -p dml-wow-cli --release --locked
      - uses: actions/upload-artifact@v4
        with:
          name: dml-wow-linux-x86_64
          path: target/release/dml-wow
          if-no-files-found: error
```

- [ ] **Step 4: Run the test to verify it passes**

```
cargo test -p launcher --lib payload::
```

Expected: PASS.

- [ ] **Step 5: Check the drift guard still holds**

`payload.rs` fails the test run when the manifest and the real layout drift.
Run the whole launcher suite so that guard sees the new entry:

```
cargo test -p launcher
```

Expected: PASS. If a manifest-drift test fails, it is telling you
`tauri.conf.json` and `payload.rs` disagree — fix the JSON, not the test.

- [ ] **Step 6: Commit**

```bash
git add .github/workflows/rust.yml launcher/src-tauri/src/payload.rs launcher/src-tauri/tauri.conf.json
git commit -m "feat(launcher): bundle the Linux dml-wow binary

CI's ubuntu job now builds and uploads the artifact the installer carries, so
that job stops being advisory: a red linux job means there is nothing to ship."
```

---

### Task 9: Deploy the binary and prove its version

**Files:**
- Modify: `launcher/src-tauri/src/provision.rs:194-296` (`plan`, `install_argv`) and `:643` (`destinations`)
- Test: `launcher/src-tauri/src/provision.rs` `mod tests`

**Interfaces:**
- Consumes: `payload::DML_WOW_BIN` (Task 8), `distro::first_boot_steps` (Task 6),
  `setup::probe_arch_with` (Task 7).
- Produces: `destinations() -> [(&'static str, &'static str); 5]` (the fifth
  entry is `("backend/dml-wow", "/usr/local/bin/dml-wow")`); `plan()` emits an
  `InstallStep` for it at mode `0755`.

- [ ] **Step 1: Write the failing tests**

Add to `mod tests` in `launcher/src-tauri/src/provision.rs`:

```rust
    #[test]
    fn the_binary_is_deployed_executable_to_a_path_on_the_distro_PATH() {
        let dests = destinations();
        let found = dests
            .iter()
            .find(|(src, _)| *src == crate::payload::DML_WOW_BIN)
            .expect("the Linux binary must be a provisioning destination");
        assert_eq!(found.1, "/usr/local/bin/dml-wow");
    }

    #[test]
    fn the_binary_step_is_mode_0755_because_a_0644_binary_cannot_run() {
        let dir = tmp_dir("provision-plan-bin");
        write_minimal_payload(&dir);
        let steps = plan(&dir).expect("plan");
        let step = steps
            .iter()
            .find(|s| s.dest == "/usr/local/bin/dml-wow")
            .expect("binary step");
        assert_eq!(step.mode, "0755");
    }

    /// Sources come from wherever the user installed the launcher, so this argv
    /// carries a HOST path. `--` would run a shell inside the distro and split
    /// it on `;`, expand `$` and glob (verified 2026-07-28).
    #[test]
    fn the_install_argv_crosses_the_boundary_with_exec() {
        let step = InstallStep {
            src: "backend/dml-wow".to_string(),
            dest: "/usr/local/bin/dml-wow".to_string(),
            mode: "0755".to_string(),
        };
        let argv = install_argv("dml-arch", &step, "/mnt/c/Users/a b/res");
        assert!(argv.iter().any(|a| a == "--exec"), "got {argv:?}");
        assert!(!argv.iter().any(|a| a == "--"), "shell form in {argv:?}");
    }
```

`write_minimal_payload` is whatever the existing `plan` tests already use to
build a payload tree; extend it with a `backend/dml-wow` file. If no such
helper exists, create the four existing destinations' files plus
`backend/dml-wow` inline in the test.

- [ ] **Step 2: Run the tests to confirm they fail**

```
cargo test -p launcher --lib provision::
```

Expected: FAIL — the binary is not among `destinations()`.

- [ ] **Step 3: Implement**

Change `destinations()`'s return type to `[(&'static str, &'static str); 5]` and
add the entry:

```rust
        // The Arch backend's whole runtime. /usr/local/bin is on the distro's
        // default PATH, which is why `DmlRunner::arch()` can invoke a bare
        // `dml-wow` rather than an absolute path.
        (crate::payload::DML_WOW_BIN, "/usr/local/bin/dml-wow"),
```

In `plan()`, emit the step with mode `"0755"` (a `0644` binary is not
executable, and the failure — `Permission denied` from `--exec` — reads like a
missing binary rather than a wrong mode).

Ensure `install_argv` uses `--exec`; if it still builds the `--` form, change it
and re-run the whole provision suite, since the existing four destinations go
through the same builder.

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p launcher --lib provision::
cargo test -p launcher
```

Expected: PASS.

- [ ] **Step 5: Verify the version handshake against the real distro**

Build a Linux binary and deploy it by hand once, to prove the contract before
the launcher automates it:

```
wsl -d dml-arch -u dml --exec sh -c "cd ~ && ~/.cargo/bin/cargo build --manifest-path /mnt/c/Users/perzi/dads-mmo-lab/Cargo.toml -p dml-wow-cli --release --target-dir ~/target"
wsl -d dml-arch -u dml --exec sudo -n install -m 0755 /home/dml/target/release/dml-wow /usr/local/bin/dml-wow
wsl -d dml-arch -u dml --exec dml-wow version --json
```

Expected: a JSON envelope whose version satisfies
`dml_core::setup::cli_version_matches` (currently `3.0.0`). If cargo is not
installed in the distro, run `wsl -d dml-arch -u dml --exec sudo -n pacman -S
--noconfirm rust` first.

If the reported version does NOT match `EXPECTED_CLI_VERSION`, stop and decide
deliberately: either the constant moves or the crate version does. Do not paper
over it — the handshake exists precisely to catch this.

- [ ] **Step 6: Commit**

```bash
git add launcher/src-tauri/src/provision.rs
git commit -m "feat(launcher): deploy dml-wow into the distro at 0755

/usr/local/bin is on the distro's default PATH, which is why the Arch runner can
invoke a bare dml-wow. 0755 because a 0644 binary fails --exec with Permission
denied, which reads like a missing binary rather than a wrong mode."
```

---

### Task 10: Live gate — provision a distro from nothing

The only task a human must run. Everything before it is testable without a
machine in a particular state; this one proves the machine can *get* to that
state. Uses a throwaway name so `dml-arch` is never at risk.

**Files:**
- Create: `docs/superpowers/plans/2026-08-04-arch-wsl-backend-gate.md` (the run log)

**Interfaces:**
- Consumes: everything from Tasks 1–9.
- Produces: a written record of the run, with the actual commands and the actual
  output.

- [ ] **Step 1: Confirm the throwaway name is free**

```
wsl --list --verbose
```

Expected: no `dml-arch-test`. If one exists from a previous attempt,
`wsl --unregister dml-arch-test` first — read the list before running that, and
never pass `dml-arch`.

- [ ] **Step 2: Create the distro**

```
wsl --install archlinux --name dml-arch-test --no-launch
```

Expected: exit 0, `dml-arch-test` appears in `wsl --list --verbose`. Record the
wall-clock time and the download size — Plan 3 needs both, and this is the
riskiest minute of a stranger's first run.

- [ ] **Step 3: Run the first-boot sequence**

Run each `distro::first_boot_steps("dml-arch-test", "dml")` entry in order, as
root.

**CORRECTED 2026-08-04 from the gate log** — the original listing here was wrong
in three ways and the operator had to fix all three live. Every correction below
is a real failure with a real message, not a tidy-up:

* `pacman-key --init` / `--populate archlinux` were MISSING. A fresh Arch WSL
  rootfs has no initialized keyring, so the first `pacman -Syu` of any kind
  fails: `warning: Public keyring not found; have you run 'pacman-key --init'?`
  / `error: required key missing from keyring`.
* `sudo` was missing from the package list, and the sudoers write came BEFORE
  the package install. `/etc/sudoers.d` does not exist on a fresh image until
  the `sudo` package creates it (`sudo` is `Required By: base-devel`, not
  `base`), so the write died with `sh: line 1: /etc/sudoers.d/99-dml: No such
  file or directory`.
* The restart after `/etc/wsl.conf` is a real step of the sequence, not an
  aside — systemd only becomes PID 1 on the NEXT boot, so `systemctl enable
  --now docker` cannot work in the boot that wrote the file.

```
wsl -d dml-arch-test -u root --exec sh -c "printf %s '[boot]
systemd=true
' > /etc/wsl.conf"
wsl --terminate dml-arch-test
wsl -d dml-arch-test -u root --exec useradd -m -G wheel dml
wsl -d dml-arch-test -u root --exec sh -c "pacman-key --init && pacman-key --populate archlinux"
wsl -d dml-arch-test -u root --exec pacman -Syu --noconfirm --needed docker docker-compose docker-buildx git sudo
wsl -d dml-arch-test -u root --exec sh -c "mkdir -p /etc/sudoers.d && printf %s 'dml ALL=(ALL) NOPASSWD: ALL
' > /etc/sudoers.d/99-dml && chmod 0440 /etc/sudoers.d/99-dml"
wsl -d dml-arch-test -u root --exec usermod -aG docker dml
wsl -d dml-arch-test -u root --exec systemctl enable --now docker
wsl --manage dml-arch-test --set-default-user dml
```

Expected: every command exits 0. Record any that does not, verbatim — a step
that fails here is a first-run failure for every user, and its message is what
they will see.

- [ ] **Step 4: Prove the daemon and the chain**

```
wsl -d dml-arch-test -u dml --exec systemctl is-active --quiet docker
echo $?
wsl -d dml-arch-test -u dml --exec docker info --format "{{.ServerVersion}} {{.Driver}}"
wsl -d dml-arch-test -u dml --exec docker buildx version
wsl -d dml-arch-test -u dml --exec sudo -n true
```

Expected: exit 0 from `is-active`; a server version and `overlayfs`; a buildx
version; exit 0 from the sudo check. Record the three versions and compare them
against the pinned known-good set (docker 29.6.1, compose 5.3.1, buildx 0.35.0).
A drift is not a failure — it is the rolling-release risk from the spec, and
recording it is how that risk stays visible.

- [ ] **Step 5: Deploy the binary and round-trip the chain**

**CORRECTED 2026-08-04 from the gate log** — two more things the original
listing got wrong:

* **`--json` does not exist on `dml-wow`.** Its clap parser rejects an argument
  it does not define (`error: unexpected argument '--json' found`), and the
  binary emits its envelope unconditionally anyway.
* **There is no `games list` subcommand.** `dml-wow` is a per-title CLI fixed to
  one already-installed title by design (ruled 2026-08-04), so the titles count
  comes from a shell probe of `$HOME/games` — the same one
  `setup::titles_count_script` builds.

A third correction is environmental, not a code bug: run these **from
PowerShell**, or set `MSYS_NO_PATHCONV=1` first. Git Bash rewrites the `/mnt/c/...`
argument into a Windows path before `wsl.exe` ever sees it, and the install
fails with `install: cannot stat 'C:/Program Files/Git/mnt/c/...'`.

```
wsl -d dml-arch-test -u root --exec install -m 0755 /mnt/c/Users/perzi/dads-mmo-lab/target/release/dml-wow /usr/local/bin/dml-wow
wsl -d dml-arch-test -u dml --exec dml-wow version
wsl -d dml-arch-test -u dml --exec sh -c 'n=0; for d in "$HOME"/games/*/; do if [ -f "${d}docker-compose.yml" ] || [ -f "${d}docker-compose.yaml" ] || [ -f "${d}compose.yml" ] || [ -f "${d}compose.yaml" ]; then n=$((n+1)); fi; done; echo "$n"'
```

Expected: a `dml-json-v3` envelope from `version`, and a bare `0` from the
titles probe. That combination is `SetupState::NoTitles`, the correct end state
for a provisioned distro with no server yet.

- [ ] **Step 6: Write the run log and commit it**

Create `docs/superpowers/plans/2026-08-04-arch-wsl-backend-gate.md` with: the
date, every command run, its exit code, the three package versions, the total
wall-clock time, and the download size. Numbers, not "worked fine" — Plan 3 uses
this as the Arch column's provisioning row.

```bash
git add docs/superpowers/plans/2026-08-04-arch-wsl-backend-gate.md
git commit -m "docs(gate): Plan 1 live gate — a distro provisioned from nothing"
```

- [ ] **Step 7: Clean up**

```
wsl --unregister dml-arch-test
```

Read `wsl --list --verbose` first and confirm the name before running this. It
deletes the root filesystem and is not reversible.

---

## Plan 2 (not yet written): launcher wiring

Written after Plan 1 lands, because it is built on interfaces Plan 1 will
adjust. Scope, recorded here so it exists in git rather than only in a
conversation:

- `startup.rs` resolution prefers `Backend::Arch`; the `DML_*` env → `launcher.json`
  → detect precedence is unchanged and stays load-bearing for the test suites.
- `backend_status` IPC routes to `probe_arch_with` on the Arch backend.
- `backend_setup` provisions the distro: `distro::install_distro_argv` +
  `first_boot_steps`, streamed as NDJSON over the existing `Channel<Value>`
  seam, idempotent, re-probing before and after.
- Distinct error codes per provisioning step (`ARCH_WSL_MISSING`,
  `ARCH_INSTALL_FAILED`, `ARCH_SYSTEMD_FAILED`, `ARCH_PACMAN_FAILED`,
  `ARCH_DOCKER_FAILED`, `ARCH_BINARY_DEPLOY_FAILED`) so a dead pacman mirror
  never surfaces as the same blank message as a missing WSL.
- Settings backend picker gains Arch; Docker Desktop is labelled a fallback.
- Auto-shutdown's stop-engine becomes `wsl --terminate dml-arch` on Arch, which
  returns the VM's RAM.
- One path helper for user-facing surfaces: `\\wsl$\dml-arch\...` for "open
  folder", `wslpath` for anything crossing into a command.

## Plan 3 (not yet written): the comparison

`docs/backend-comparison-2026-08.md`, per the spec's table: idle RAM, RAM with
500 bots, launcher-open-to-world-ready, full install time, disk, RAM returned
after stop. Measured on the same title with the same client data and modules.
The provisioning numbers from Task 10 are its first row.

---

## Self-Review

**Spec coverage.** Architecture §1 → Tasks 2, 3. The `CommandTarget` seam §2 →
Task 3 (the spec proposed a `CommandTarget` enum; `DmlRunner` already carries
exactly that shape in `program`/`prefix_args`, so a second abstraction over it
would have been a rename with no new capability — the constructor is the seam).
Provisioning §3 → Tasks 6, 10, and Plan 2 for the streamed UI. Binary delivery
§4 → Tasks 8, 9. Engine control §5 → Task 4. `install_native` default dir §5 →
Task 5. Launcher §6 → Plan 2. Comparison §7 → Plan 3. Error handling's bounded
calls → Task 1. `migrate.rs` is untouched by decision 7.

**Not covered by any task, deliberately:** `composegen`'s `canon_path` needs no
change (it already folds the four spellings); the stack-conflict and port guards
need no change (WSL2 localhost forwarding keeps 3724/8085/7878/3306 reachable
from Windows).

**Type consistency.** `Backend::Arch` (Task 2) is consumed by
`DmlRunner::for_backend` (Task 3) and `EngineKind::for_backend` (Task 4).
`ARCH_BINARY` (Task 3) is the same `"dml-wow"` string that `probe_arch_with`
(Task 7) and `destinations()` (Task 9) invoke. `ArchFacts.dockerd` (Task 7) is
the `Tri` that `engine_running_tri` (Task 4) produces. `payload::DML_WOW_BIN`
(Task 8) is the key `destinations()` (Task 9) looks up.
