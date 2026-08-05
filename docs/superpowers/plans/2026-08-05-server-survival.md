# Server survival implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** hold the WSL distro open on every distro backend, and make closing the launcher stop the server cleanly instead of letting WSL power-cut it.

**Architecture:** one flag change arms the existing keep-alive for `Backend::Wsl`. The exit path moves from `RunEvent::Exit` (which cannot be prevented) to `RunEvent::ExitRequested` (which can), gains a pure decision function, and routes a confirmed exit through the ordinary `games_stop` path before releasing the holder.

**Tech Stack:** Rust, Tauri 2 (2.11.5), Svelte 5 runes, vitest.

**Spec:** [`docs/superpowers/specs/2026-08-05-server-survival-design.md`](../specs/2026-08-05-server-survival-design.md)

**Branch:** `feat/arch-wsl-backend` (continue on it; HEAD `e9ab100`).

## Global Constraints

- **`Backend::Native` behaviour must not change.** Docker Desktop keeps its own containers alive; it never holds a distro and never prompts on exit.
- **The clean stop runs BEFORE the holder is released.** Releasing first starts the distro's 15-second clock while compose is still shutting containers down. `games_stop` already declares intent after the work for exactly this reason — do not reorder it.
- **Tri-state discipline.** A probe that could not answer is evidence of NOTHING. An unknown server status prompts, it does not silently exit.
- **The launcher must always be closable.** Every prompt has a close-anyway escape; a bounded stop that overruns offers it.
- **Anti-vacuity is mandatory.** This branch has produced six tests that could not fail. After writing each test, mutate the production code, watch it go red for the right reason, revert, confirm `git status --short` is clean. **Commit before you mutate** — four implementers on this branch lost work to `git checkout` over uncommitted edits.
- Cargo root is the REPO ROOT. If `cargo` is not on PATH use `C:\Users\perzi\.cargo\bin\cargo.exe`.
- `cargo test --workspace` takes ~110s and passes at 1908/0 today. Targeted `-p` runs are fine.
- Do NOT start or stop the user's real server except where a task says so explicitly.

---

## File Structure

| File | Responsibility | Task |
|---|---|---|
| `launcher/src-tauri/src/wsl_keepalive.rs` | arm for `Wsl`; the exit decision as a pure function | 1, 2 |
| `launcher/src-tauri/src/lib.rs` | the `ExitRequested` hook, the two exit commands, the re-entrancy guard | 3 |
| `launcher/src/lib/exit-guard.svelte.ts` (new) | frontend exit state + the pure copy selection | 4 |
| `launcher/src/routes/+page.svelte` | mount the dialog | 4 |
| `launcher/src/lib/pages/Config.svelte` | the backend-asymmetry copy | 5 |
| `crates/dml-core/tests/live_wsl_keepalive.rs` (new) | the live proof on `Backend::Wsl` | 6 |

---

### Task 1: Arm the keep-alive for `Backend::Wsl`

**Files:**
- Modify: `launcher/src-tauri/src/wsl_keepalive.rs:112-134` (the doc comment and `applies_to`)
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: `dml_core::backend::Backend`.
- Produces: `applies_to(Backend) -> bool` — unchanged signature, `Wsl` now `true`.

- [ ] **Step 1: Write the failing test**

Add to `mod tests` in `wsl_keepalive.rs`:

```rust
    /// THE POINT OF THIS CHANGE. `Backend::Wsl` drives the SAME `dml-arch`
    /// distro as `Arch` (both go through `runner::DISTRO`), so it has the
    /// identical 15-second termination — measured n=8, 14.7-14.9s, spread 0.2s.
    /// It is also the DEFAULT and the only backend the Settings dropdown
    /// offers, so before this change the fix protected a backend nobody could
    /// select, while the one the user's real server runs on stayed exposed.
    #[test]
    fn every_distro_backend_holds_its_distro_open() {
        assert!(applies_to(Backend::Arch), "Arch drives dml-arch");
        assert!(applies_to(Backend::Wsl), "Wsl drives the SAME dml-arch");
    }

    /// Docker Desktop keeps its own utility VM alive for its containers, so
    /// there is no distro to hold and a holder would be a stray `wsl.exe` for
    /// no reason.
    #[test]
    fn docker_desktop_never_holds_a_distro() {
        assert!(!applies_to(Backend::Native));
    }
```

Delete the existing `applies_to` tests that assert `Wsl` is false (around lines 873-882) — they assert the contract this task reverses.

- [ ] **Step 2: Run the test to verify it fails**

```
cargo test -p launcher --lib wsl_keepalive::tests::every_distro_backend_holds_its_distro_open
```

Expected: FAIL on `Wsl drives the SAME dml-arch`.

- [ ] **Step 3: Implement**

Replace `applies_to`'s body (`wsl_keepalive.rs:128-134`):

```rust
pub fn applies_to(backend: Backend) -> bool {
    match backend {
        Backend::Arch | Backend::Wsl => true,
        Backend::Native => false,
    }
}
```

Then rewrite the doc comment above it. The current one explains why `Wsl` was **deliberately left alone**; that reasoning is now spent and a stale justification is how the next reader reverts a fix. State instead: both distro backends drive `runner::DISTRO`, so both need the holder; `Native` does not because `com.docker.backend` already holds `docker-desktop` for the same reason, which is why that distro never showed the behaviour in the same sitting.

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p launcher --lib wsl_keepalive::
```

Expected: PASS.

- [ ] **Step 5: Prove it is not vacuous**

Commit first, then set `Backend::Wsl => false`, re-run, confirm RED on the new test, `git checkout -- launcher/src-tauri/src/wsl_keepalive.rs`, confirm `git status --short` clean. Paste both outputs into your report.

- [ ] **Step 6: Commit**

```bash
git add launcher/src-tauri/src/wsl_keepalive.rs
git commit -m "fix(launcher): hold the distro open on Wsl too, not just Arch

Backend::Wsl drives the same dml-arch distro through the same runner::DISTRO
constant, so it has the identical 15-second termination -- and it is the default
and the only backend the Settings dropdown offers. The keep-alive was protecting
a backend nobody could select while the one the user's real server runs on stayed
exposed."
```

---

### Task 2: The exit decision, as a pure function

**Files:**
- Modify: `launcher/src-tauri/src/wsl_keepalive.rs` (append near `applies_to`)
- Test: same file, `mod tests`

**Interfaces:**
- Consumes: `Backend`, `applies_to`.
- Produces:
  - `pub enum ServerPresence { Running, Stopped, Unknown }`
  - `pub enum ExitAction { ExitNow, PromptRunning, PromptUnknown }`
  - `pub fn exit_decision(backend: Backend, presence: ServerPresence) -> ExitAction`
  - `pub fn presence_from(holding: bool, last_verdict: Option<&str>) -> ServerPresence`
  - `Keepalive::last_verdict(&self) -> Option<&str>` and a new `last_verdict: Option<String>` field on `KeepaliveReport`

**Two things about `KeepaliveReport` you must fix in this task** (verified against the current source, do not skip):

1. **It has no `last_verdict` field.** `Keepalive::observed_status(&mut self, verdict: &str)` already receives the verdict and currently throws it away. Store it (`self.last_verdict = Some(verdict.to_string())`), expose it via a `last_verdict()` accessor, and add it to `KeepaliveReport`, so Task 3 can derive presence without reaching into `STATE` from `lib.rs` or running a fresh probe.
2. **`KeepaliveReport::applies`'s doc comment says "False on Native/Wsl"**, which Task 1 has just made false. Correct it to name `Native` only. A stale comment on a field the UI reads is how the next reader re-derives the wrong rule — this branch has already paid for that twice.

- [ ] **Step 1: Write the failing tests**

```rust
    /// Nine combinations, all of them. The matrix IS the contract.
    #[test]
    fn docker_desktop_never_prompts_on_exit() {
        for p in [ServerPresence::Running, ServerPresence::Stopped, ServerPresence::Unknown] {
            assert_eq!(
                exit_decision(Backend::Native, p),
                ExitAction::ExitNow,
                "Desktop keeps its containers running; closing the launcher is harmless: {p:?}"
            );
        }
    }

    #[test]
    fn a_running_server_on_a_distro_backend_prompts() {
        for b in [Backend::Arch, Backend::Wsl] {
            assert_eq!(exit_decision(b, ServerPresence::Running), ExitAction::PromptRunning, "{b:?}");
        }
    }

    #[test]
    fn a_stopped_server_exits_without_friction() {
        for b in [Backend::Arch, Backend::Wsl] {
            assert_eq!(exit_decision(b, ServerPresence::Stopped), ExitAction::ExitNow, "{b:?}");
        }
    }

    /// Tri-state discipline. A status we could not read is NOT a stopped
    /// server. A needless dialog costs one click; a missed one costs a database
    /// killed mid-write, because WSL's 10s grace expires before systemd can
    /// stop ~2000 bots and the sequence ends in reboot(RB_POWER_OFF).
    #[test]
    fn an_unknown_status_prompts_rather_than_risking_the_cut() {
        for b in [Backend::Arch, Backend::Wsl] {
            assert_eq!(exit_decision(b, ServerPresence::Unknown), ExitAction::PromptUnknown, "{b:?}");
        }
    }

    /// The holder is the strongest signal we have: it is set by an ACT (the
    /// user pressed Start, or a poll adopted a stack that was already up), not
    /// by a probe that might be stale.
    #[test]
    fn presence_trusts_the_holder_over_a_stale_verdict() {
        assert_eq!(presence_from(true, Some("stopped")), ServerPresence::Running);
        assert_eq!(presence_from(true, None), ServerPresence::Running);
    }

    #[test]
    fn presence_falls_back_to_the_last_verdict_when_not_holding() {
        assert_eq!(presence_from(false, Some("online")), ServerPresence::Running);
        assert_eq!(presence_from(false, Some("stopped")), ServerPresence::Stopped);
    }

    /// Never polled = we do not know. Not "stopped".
    #[test]
    fn presence_with_no_verdict_at_all_is_unknown() {
        assert_eq!(presence_from(false, None), ServerPresence::Unknown);
        assert_eq!(presence_from(false, Some("")), ServerPresence::Unknown);
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```
cargo test -p launcher --lib wsl_keepalive::
```

Expected: FAIL to compile — `cannot find type 'ServerPresence'`.

- [ ] **Step 3: Implement**

Add to `wsl_keepalive.rs` immediately after `applies_to`:

```rust
/// What we believe the server is doing, at the moment the user asked to close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerPresence {
    Running,
    Stopped,
    /// We could not tell. NOT a synonym for `Stopped` — see [`exit_decision`].
    Unknown,
}

/// What closing the launcher should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitAction {
    /// Close immediately. No dialog, no delay.
    ExitNow,
    /// Ask first: a server is running and closing will stop it.
    PromptRunning,
    /// Ask first, and say honestly that we could not confirm.
    PromptUnknown,
}

/// Should closing the launcher ask the user first?
///
/// On `Native` never: Docker Desktop keeps its own containers running, so
/// closing the launcher is harmless there. On a distro backend it is not —
/// WSL powers the distro off ~15s after the last session exits, and on a loaded
/// server the 10s grace expires before systemd can stop the containers, so the
/// sequence ends in `reboot(RB_POWER_OFF)`: a hard cut of MySQL mid-write.
///
/// `Unknown` prompts. A probe that could not answer is evidence of nothing, and
/// the asymmetry decides it: a needless dialog costs one click, a missed one
/// costs the database.
pub fn exit_decision(backend: Backend, presence: ServerPresence) -> ExitAction {
    if !applies_to(backend) {
        return ExitAction::ExitNow;
    }
    match presence {
        ServerPresence::Running => ExitAction::PromptRunning,
        ServerPresence::Unknown => ExitAction::PromptUnknown,
        ServerPresence::Stopped => ExitAction::ExitNow,
    }
}

/// Derive presence from what the launcher already knows, so exiting never has
/// to run a fresh probe — a probe at exit time can hang, and a launcher that
/// will not close is worse than the bug this guards.
///
/// `holding` wins: it is set by an ACT (Start pressed, or a poll adopting a
/// stack that was already up) rather than by a reading that may be stale.
pub fn presence_from(holding: bool, last_verdict: Option<&str>) -> ServerPresence {
    if holding {
        return ServerPresence::Running;
    }
    match last_verdict.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if verdict_means_running(v) => ServerPresence::Running,
        Some(_) => ServerPresence::Stopped,
        None => ServerPresence::Unknown,
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p launcher --lib wsl_keepalive::
```

Expected: PASS.

- [ ] **Step 5: Prove it is not vacuous**

Commit, then change `ServerPresence::Unknown => ExitAction::PromptUnknown` to `=> ExitAction::ExitNow`, confirm `an_unknown_status_prompts_rather_than_risking_the_cut` goes RED, revert, confirm clean. Paste both outputs.

- [ ] **Step 6: Commit**

```bash
git add launcher/src-tauri/src/wsl_keepalive.rs
git commit -m "feat(launcher): the exit decision, as a pure nine-case function

Presence is derived from what the launcher already knows rather than a fresh
probe -- a probe at exit time can hang, and a launcher that will not close is
worse than the bug being guarded. An unknown status prompts: a needless dialog
costs one click, a missed one costs a database killed mid-write."
```

---

### Task 3: Hook `ExitRequested`, and stop cleanly before releasing

**Files:**
- Modify: `launcher/src-tauri/src/lib.rs:7760-7774` (the `RunEvent` handler)
- Modify: `launcher/src-tauri/src/lib.rs` (register two new commands in the `generate_handler!` list)
- Test: `launcher/src-tauri/src/lib.rs` `mod tests`

**Interfaces:**
- Consumes: `wsl_keepalive::{exit_decision, presence_from, ExitAction, keepalive_report, shutdown}`, `backend::selected()`.
- Produces:
  - `#[tauri::command] fn exit_intent() -> String` — `"exit_now" | "prompt_running" | "prompt_unknown"`, so the frontend can render without duplicating the rule.
  - `#[tauri::command] async fn exit_stop_and_close(app, id, on_event, state) -> Result<(), CmdError>`
  - `#[tauri::command] fn exit_anyway(app)`
  - `static EXIT_CONFIRMED: AtomicBool` — the re-entrancy guard.

- [ ] **Step 1: Write the failing tests**

```rust
    /// Re-entrancy. Confirming the dialog calls `app.exit(0)`, which fires
    /// `ExitRequested` a SECOND time. Without the latch that second pass would
    /// prompt again and the launcher could never close.
    #[test]
    fn a_confirmed_exit_is_not_prompted_a_second_time() {
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
        assert!(should_prompt_on_exit(ExitAction::PromptRunning), "first pass asks");
        EXIT_CONFIRMED.store(true, Ordering::SeqCst);
        assert!(
            !should_prompt_on_exit(ExitAction::PromptRunning),
            "once confirmed, every later ExitRequested must pass straight through"
        );
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
    }

    #[test]
    fn exit_now_never_prompts_whatever_the_latch_says() {
        for latched in [false, true] {
            EXIT_CONFIRMED.store(latched, Ordering::SeqCst);
            assert!(!should_prompt_on_exit(ExitAction::ExitNow));
        }
        EXIT_CONFIRMED.store(false, Ordering::SeqCst);
    }

    /// The wire strings the frontend switches on. A rename here is a silent
    /// UI break, so they are pinned.
    #[test]
    fn the_exit_intent_wire_values_are_stable() {
        assert_eq!(exit_action_wire(ExitAction::ExitNow), "exit_now");
        assert_eq!(exit_action_wire(ExitAction::PromptRunning), "prompt_running");
        assert_eq!(exit_action_wire(ExitAction::PromptUnknown), "prompt_unknown");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```
cargo test -p launcher --lib
```

Expected: FAIL to compile — `cannot find value 'EXIT_CONFIRMED'`.

- [ ] **Step 3: Implement**

Add near the other statics in `lib.rs`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once the user has confirmed the exit (or chosen to close anyway), so the
/// `app.exit(0)` that follows is not intercepted and prompted a second time.
static EXIT_CONFIRMED: AtomicBool = AtomicBool::new(false);

/// Pure: does this exit need a dialog, given the latch?
fn should_prompt_on_exit(action: wsl_keepalive::ExitAction) -> bool {
    if EXIT_CONFIRMED.load(Ordering::SeqCst) {
        return false;
    }
    !matches!(action, wsl_keepalive::ExitAction::ExitNow)
}

/// The wire vocabulary the frontend switches on.
fn exit_action_wire(action: wsl_keepalive::ExitAction) -> &'static str {
    match action {
        wsl_keepalive::ExitAction::ExitNow => "exit_now",
        wsl_keepalive::ExitAction::PromptRunning => "prompt_running",
        wsl_keepalive::ExitAction::PromptUnknown => "prompt_unknown",
    }
}

fn current_exit_action() -> wsl_keepalive::ExitAction {
    let report = wsl_keepalive::keepalive_report();
    let presence = wsl_keepalive::presence_from(report.holding, report.last_verdict.as_deref());
    wsl_keepalive::exit_decision(dml_core::backend::selected(), presence)
}

#[tauri::command]
fn exit_intent() -> String {
    exit_action_wire(current_exit_action()).to_string()
}

/// Close anyway — the escape hatch. The user is entitled to close their
/// launcher even when the server misbehaves or the stop overruns.
#[tauri::command]
fn exit_anyway(app: tauri::AppHandle) {
    EXIT_CONFIRMED.store(true, Ordering::SeqCst);
    app.exit(0);
}
```

Then `exit_stop_and_close`, which runs the ORDINARY stop path and only then exits. Do not hand-roll a stop: `games_stop` already takes the bounded worldserver log snapshot and the pre-stop backup, and already declares `server_should_stop()` after the work rather than before.

```rust
/// Stop the server the ordinary way, then close.
///
/// ORDER IS THE CONTRACT: the stop runs BEFORE the holder is released.
/// Releasing first starts the distro's 15-second clock while compose is still
/// shutting containers down, which is the ungraceful stop this whole command
/// exists to avoid. `games_stop` already gets that ordering right internally.
#[tauri::command]
async fn exit_stop_and_close(
    app: tauri::AppHandle,
    id: String,
    manage_docker: Option<bool>,
    on_event: Channel<serde_json::Value>,
    state: State<'_, AppState>,
) -> Result<(), CmdError> {
    let result = games_stop(id, manage_docker, on_event, state).await;
    EXIT_CONFIRMED.store(true, Ordering::SeqCst);
    app.exit(0);
    result
}
```

Register all three in `generate_handler!`.

Finally replace the `RunEvent` handler (`lib.rs:7760-7774`). `ExitRequested` is the hook that can be prevented; `Exit` cannot, which is why the current code has no way to ask anything:

```rust
        .run(|app, event| match event {
            // THE HOOK THAT CAN SAY NO. `RunEvent::Exit` fires too late to ask
            // anything — by then the decision is made. Tray Quit routes through
            // `app.exit(0)` (tray.rs:90), so it reaches this same arm and there
            // is no second path to maintain.
            tauri::RunEvent::ExitRequested { api, .. } => {
                let action = current_exit_action();
                if should_prompt_on_exit(action) {
                    api.prevent_exit();
                    let _ = app.emit("exit-requested", exit_action_wire(action));
                }
            }
            tauri::RunEvent::Exit => {
                power::keep_awake(false);
                // Release the held WSL session. THE POLITE PATH ONLY: an abrupt
                // kill never reaches here, which is why the child is also in a
                // KILL_ON_JOB_CLOSE job object.
                wsl_keepalive::shutdown();
            }
            _ => {}
        });
```

`app.emit` needs `use tauri::Emitter;` in scope.

- [ ] **Step 4: Run the tests to verify they pass**

```
cargo test -p launcher --lib
cargo build --workspace
```

Expected: PASS, clean build.

- [ ] **Step 5: Prove it is not vacuous**

Commit, then make `should_prompt_on_exit` ignore the latch (delete its first two lines), confirm `a_confirmed_exit_is_not_prompted_a_second_time` goes RED, revert, confirm clean. Paste both outputs.

- [ ] **Step 6: Commit**

```bash
git add launcher/src-tauri/src/lib.rs
git commit -m "feat(launcher): ask before closing stops the server

RunEvent::ExitRequested rather than RunEvent::Exit: only the former can be
prevented, and the old code hooked the latter, which fires too late to ask
anything. A confirmed exit runs the ordinary games_stop -- log snapshot and
pre-stop backup included -- and only then releases the holder, because releasing
first starts the distro's 15s clock while compose is still shutting down."
```

---

### Task 4: The dialog

**Files:**
- Create: `launcher/src/lib/exit-guard.svelte.ts`
- Create: `launcher/src/lib/exit-guard.test.ts`
- Modify: `launcher/src/routes/+page.svelte` (mount the dialog)
- Modify: `launcher/src/lib/api.ts` (invoke wrappers)

**Interfaces:**
- Consumes: the `exit-requested` event carrying `"prompt_running" | "prompt_unknown"`; commands `exit_stop_and_close`, `exit_anyway`.
- Produces: `exitGuard` (module-level runes store), `exitCopy(kind)` (pure).

- [ ] **Step 1: Write the failing tests**

`launcher/src/lib/exit-guard.test.ts`:

```ts
import { describe, it, expect } from 'vitest';
import { exitCopy } from './exit-guard.svelte';

describe('exitCopy', () => {
  it('says plainly that closing stops the server', () => {
    const c = exitCopy('prompt_running');
    expect(c.title).toMatch(/running/i);
    expect(c.body).toMatch(/stop/i);
    expect(c.confirm).toBe('Stop server and close');
    expect(c.cancel).toBe('Cancel');
  });

  it('admits uncertainty rather than claiming a running server', () => {
    const c = exitCopy('prompt_unknown');
    // The honest wording matters: asserting a running server we could not
    // confirm is the same overclaiming the tri-state exists to prevent.
    expect(c.body).toMatch(/could ?n[o']t confirm/i);
    expect(c.body).toMatch(/may stop/i);
  });

  it('never shouts an error word — this is a routine choice, not a failure', () => {
    for (const k of ['prompt_running', 'prompt_unknown'] as const) {
      const all = Object.values(exitCopy(k)).join(' ');
      expect(all).not.toMatch(/error|failed|fatal|warning/i);
    }
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

```
cd launcher && npm test -- exit-guard
```

Expected: FAIL — cannot resolve `./exit-guard.svelte`.

- [ ] **Step 3: Implement**

`launcher/src/lib/exit-guard.svelte.ts`:

```ts
export type ExitPrompt = 'prompt_running' | 'prompt_unknown';

export interface ExitCopy {
  title: string;
  body: string;
  confirm: string;
  cancel: string;
}

/** Pure, so the wording is testable without mounting anything. */
export function exitCopy(kind: ExitPrompt): ExitCopy {
  const confirm = 'Stop server and close';
  const cancel = 'Cancel';
  if (kind === 'prompt_running') {
    return {
      title: 'Your server is running',
      body: 'Closing DML Launcher will stop it. Windows shuts the WSL distro down shortly after the launcher exits, so the server cannot keep running without it.',
      confirm,
      cancel
    };
  }
  return {
    title: 'Your server may be running',
    body: "Couldn't confirm whether your server is running. Closing DML Launcher may stop it, so it will be stopped cleanly first.",
    confirm,
    cancel
  };
}

/** Module-level so it survives navigation, mirroring restart-state.svelte.ts. */
export const exitGuard = $state<{ open: boolean; kind: ExitPrompt; busy: boolean; note: string }>({
  open: false,
  kind: 'prompt_running',
  busy: false,
  note: ''
});
```

In `+page.svelte`, listen for `exit-requested`, set `exitGuard.open` and `exitGuard.kind`, and render a modal with the two buttons. **Confirm** calls `exit_stop_and_close` with the current title id and sets `exitGuard.busy = true`, streaming the stop's events into the existing terminal so a long stop shows progress rather than a frozen window. **Cancel** just closes the modal.

While busy, show a third affordance — *Close anyway* — wired to `exit_anyway`. Stopping ~2,000 bots is not instant, and a window that appears frozen is how a user reaches for Task Manager, which reproduces the exact failure this feature prevents.

- [ ] **Step 4: Run the tests to verify they pass**

```
cd launcher && npm test -- exit-guard && npm run check
```

Expected: PASS, svelte-check 0 errors.

- [ ] **Step 5: Prove it is not vacuous**

Commit, then change `exitCopy('prompt_unknown')`'s body to the `prompt_running` text, confirm the uncertainty test goes RED, revert, confirm clean.

- [ ] **Step 6: Commit**

```bash
git add launcher/src/lib/exit-guard.svelte.ts launcher/src/lib/exit-guard.test.ts launcher/src/routes/+page.svelte launcher/src/lib/api.ts
git commit -m "feat(launcher): the exit dialog, with an honest uncertain wording

Two prompts, not one: a confirmed running server and a status we could not read
say different things, because asserting a running server we could not confirm is
the same overclaiming the tri-state exists to prevent. A 'Close anyway' escape
appears while the stop runs -- a window that looks frozen is how a user reaches
for Task Manager, which reproduces the exact cut this prevents."
```

---

### Task 5: Tell the user before they choose, not after

**Files:**
- Modify: `launcher/src/lib/pages/Config.svelte` (the backend picker, around 589-600)
- Test: `launcher/src/lib/backend-copy.test.ts` (new)

**Interfaces:**
- Consumes: nothing.
- Produces: `backendSurvivalNote(backend)` exported from `Config.svelte`'s sibling module, or inline copy pinned by a test that reads the component source.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';

describe('the backend picker states the survival difference', () => {
  it('says the server does not survive closing the launcher on WSL', () => {
    const src = readFileSync('src/lib/pages/Config.svelte', 'utf8');
    // Strip comments first: this repo has been bitten TWICE by source scans
    // that read a comment as the thing they were looking for.
    const code = src.replace(/<!--[\s\S]*?-->/g, '').replace(/\/\/.*$/gm, '');
    expect(code).toMatch(/keeps? running when you close/i);
    expect(code).toMatch(/WSL/);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```
cd launcher && npm test -- backend-copy
```

Expected: FAIL — no such copy in the component.

- [ ] **Step 3: Implement**

Add beneath the backend `<select>` in `Config.svelte`:

> On Docker Desktop your server keeps running when you close the launcher. On WSL it cannot — Windows shuts the distro down shortly after, so the launcher stops the server for you.

This belongs next to the choice. A user picking a backend on the strength of ~0.4–1.0 GB of saved memory is entitled to know that the cheaper one cannot outlive the launcher.

- [ ] **Step 4: Run it to verify it passes**

```
cd launcher && npm test -- backend-copy && npm run check
```

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/pages/Config.svelte launcher/src/lib/backend-copy.test.ts
git commit -m "docs(ui): the backend picker states which backend outlives the launcher

A user choosing a backend on the strength of ~0.4-1.0 GB of saved memory is
entitled to know that the cheaper one cannot keep a server running once the
launcher closes."
```

---

### Task 6: Prove it live on `Backend::Wsl`

**Files:**
- Create: `crates/dml-core/tests/live_wsl_keepalive.rs`

**Interfaces:**
- Consumes: the same machinery `crates/dml-core/tests/live_arch_smoke.rs` uses.

- [ ] **Step 1: Write the live test**

Model it on the existing live Arch keep-alive test. It must assert BOTH halves:

```rust
/// `#[ignore]`d: touches the real `dml-arch` distro.
///
/// BOTH halves are required. Surviving past 15s alone would also pass for
/// `wsl --list` polling, which is MEASURED not to work (only a session INTO the
/// distro counts). It is the death AFTER release that identifies the holder as
/// the cause.
#[ignore = "touches the real dml-arch distro; run with -- --ignored"]
#[test]
fn the_wsl_backend_holds_its_distro_open_and_lets_go() {
    // 1. terminate the distro so the run starts from a known state
    // 2. establish the holder as Backend::Wsl resolves it
    // 3. assert the distro is still Running at 40s — well past the ~15s deadline
    // 4. release
    // 5. assert it stops within ~25s (the 15s timer plus poweroff grace)
}
```

Fill in the body using the existing live test's helpers; do not invent a second way to ask `wsl --list --verbose` for a distro's state. Note the existing live probe had a bug worth not repeating: `tokens.next()?` returned `None` from the whole scan on a blank line, so a Running distro read as dead.

- [ ] **Step 2: Run it**

```
cargo test -p dml-core --test live_wsl_keepalive -- --ignored --nocapture
```

Expected: PASS. Record the real elapsed times. **This starts no server** — it holds an otherwise idle distro, so it is safe to run unattended.

- [ ] **Step 3: Run the whole suite**

```
cargo test --workspace
cd launcher && npm test && npm run check
```

Expected: 0 failures.

- [ ] **Step 4: Commit**

```bash
git add crates/dml-core/tests/live_wsl_keepalive.rs
git commit -m "test(core): prove the holder on Wsl, the backend that was exposed"
```

- [ ] **Step 5: Write the human gate**

Append to `docs/superpowers/plans/2026-08-05-server-survival.md` a short **Human gate** section listing what only a person can check: `npm run tauri dev`, start a server, click tray Quit and confirm the dialog appears; click Cancel and confirm the launcher stays open with the server up; click Quit again, confirm, and watch the stop stream to completion before the window closes; then confirm `wsl --list --verbose` shows the distro stopping rather than being cut. Note that tray Quit and window-close were previously code-inspected only, and this change is exactly what makes that insufficient.

---

## Self-Review

**Spec coverage.** Part 1 → Task 1. The exit hook and `ExitRequested`-not-`Exit` → Task 3. The dialog and its two wordings → Task 4. The tri-state unknown case → Tasks 2 and 4. Escape hatches (close anyway, re-entrancy) → Tasks 3 and 4. The bounded stop → Task 4's *Close anyway* while busy, plus `games_stop`'s own existing behaviour. Settings copy → Task 5. Testing discipline → every task's Step 5. The live proof → Task 6. The human gate → Task 6 Step 5.

**Not covered, deliberately:** unattended hosting (a Windows service) is named in the spec as out of scope; `vmIdleTimeout` is measured not to help.

**Type consistency.** `ServerPresence` and `ExitAction` (Task 2) are consumed by `current_exit_action` and `should_prompt_on_exit` (Task 3). The wire strings `exit_now`/`prompt_running`/`prompt_unknown` (Task 3) are the same literals `ExitPrompt` accepts (Task 4). `keepalive_report()` supplies `holding` (exists today) and `last_verdict` (does NOT exist today — Task 2 adds it, along with storing the verdict in `Keepalive::observed_status`, which currently discards it). Verified against the current source rather than assumed.

---

## Known residual findings after fix round 3 (2026-08-05)

Recorded HERE, in git, rather than only in `.superpowers/` — which is gitignored,
and this repo has already lost a user-approved spec and a 13-item feature batch
that way. All four are below the line the final review drew as "blocks the gate";
none of them is a reason to delay the click-through.

- **M9 — the exit guarantee is per-60-second-burst, not per-sequence.** Proven
  RED by a probe: twenty exit requests spaced one `EXIT_REQUEST_WINDOW` apart
  were all prevented. A webview that spoke once and then died therefore traps a
  *patient* user forever — click Exit, window surfaces, no dialog paints, go
  away, come back, click again, vetoed, indefinitely. Nothing tells them to
  click again. The signal that collapses this is already in the file and unused:
  the keep-awake watchdog treats the same `AppState::last_status_push` as stale
  after 2 minutes, while `exit_prevention_allowed` reads it as a set-once latch.
  Reuse the threshold.
- **L11 — a fourth `ExitAction` variant compiles, silently prompts, and is
  untested.** `should_prompt_on_exit` is `!matches!(…, ExitNow)` rather than an
  exhaustive match, and every test iterates a hand-written array parallel to the
  real enum.
- **L12 — `HideToTray` hides a launcher that is silently staying up.** Created
  by C1: with `closeToTray` ON (the default) and a stop in flight, X hides the
  terminal, the note and the Close-anyway button. If the stop then fails the
  launcher does not exit, so there is no visible surface at all — a `wsl.exe`
  holder pinning ~1.4 GB, server still up, indistinguishable from a clean close.
- **L13's remainder — copy and doc drift.** `Config.svelte`'s "so the launcher
  stops the server for you" is now conditional three ways (Close anyway, the
  bound, the never-spoke path) and the spec sentence moves with it;
  `backend-copy.test.ts` still carries its own pre-refactor `code()`/`find()`
  without `normalizeEol`, the one stripper the F2 consolidation missed.

## Human gate

`live_wsl_keepalive.rs` (Task 6) proves the WSL-side timing claim against the real distro — measured 2026-08-05: established the holder, survived 40.1s (well past the ~15s deadline), released, died 16.9s later (inside the ~25s expectation). It cannot prove the other half of this plan: real OS window state (hidden vs. destroyed vs. surfaced) and a real tray-icon click do not exist inside a `cargo test`/`vitest` process. Tasks 3 and 4's reviews caught two genuine regressions in this exact area — a window-destroy loop and a dialog opening into a hidden webview nobody could see — by reading source (vendored Tauri runtime code, `RunEvent` wiring), not by running anything, precisely because nothing runnable could see them either. **Tray Quit and window-close were previously code-inspected only; that is exactly what makes a scripted check insufficient here.** The items below are what only a person, running the real app, can verify — including two cases the plan's original script would never have reproduced.

**Setup, once:** `cd launcher && npm run tauri dev`. Have a title installed so a server can actually be started (Home → Start). Keep a second terminal free for `wsl --list --verbose` / `Get-Process wsl,wslrelay,vmmemWSL`.

**PACING RULE, and it is not optional — read it before item 1.** Fix round 2 (finding F1) bounded the exit veto: at most `MAX_UNANSWERED_EXIT_PREVENTIONS` (= 2) close attempts in a row may be prevented, and the run of attempts only resets after `EXIT_REQUEST_WINDOW` (= 60 seconds) of quiet. Cancel is invisible to Rust — it touches no command — so a cancelled dialog and a dialog nobody could see are the same event at that boundary. **Leave at least 60 seconds between deliberate close attempts (or restart the launcher between items).** Items 1, 2, 4 and 5 all trigger the dialog; run three of them inside one minute and the third close will legitimately take the exit with your server still running, which reads as "item 5's dialog never appeared" and is a false regression against correct code. Item 8 exercises that bound on purpose.

For each item: what to do, what to expect, and what its ABSENCE would mean — a gate that only says "check it works" is not a gate.

1. **Tray Quit from an ALREADY-HIDDEN window** — the default path, since `closeToTray` is ON by default and this is what closing the launcher normally looks like.
   - Do: start a server, close the window with its own X (hides to tray, no prompt — that part is already correct and unrelated to this check), THEN right-click the tray icon and click **Exit**.
   - Expect: the window comes back on screen — unminimized and focused — with the exit dialog ("Your server is running" / "Your server may be running") already open in it.
   - If it did not happen: the window stays hidden and the dialog renders into a webview nobody can see, so clicking Exit produces no visible change at all. That is the Task 4 review finding verbatim (`show_main_window` missing from the `tray_quit → RunEvent::ExitRequested` path) — the natural next move for "I clicked Exit and nothing happened" is Task Manager, which skips the clean stop and hands the server the exact hard WSL cut this whole plan exists to prevent. Opening a fresh, already-visible `tauri dev` window and clicking Quit does NOT reproduce this; you must close to tray first.

2. **`closeToTray` OFF plus the window's own X, with a server running.**
   - Do: in Settings, turn off "Closing the window keeps DML Launcher running in the system tray". Start a server. Click the window's native X.
   - Expect: the window stays visible the whole time — it must not vanish, even for an instant — and the exit dialog opens in place. Click Cancel, then click "Open DML Launcher" from the tray menu: the same window should simply refocus, with no relaunch and no error.
   - If it did not happen: two distinct historical bugs live here, and either reappearing is a real regression. (a) The window disappears and nothing else happens — the `HideAndPrompt` bug (Task 4) in this arm instead of the tray arm: hide-then-prompt into a webview nobody can see. (b) The window disappears and never comes back — tray Open does nothing, a second X click repeats the same nothing — the Task 3 regression: `WindowEvent::Destroyed` fires its own `ExitRequested` with no window left for `show_main_window` to find, recoverable only by Task Manager (the exact hard cut this plan exists to prevent, plus a hang no click can escape).

3. **`closeToTray` OFF, nothing running.**
   - **PRECONDITION — at least one status poll must have SUCCEEDED in this launcher run.** Confirm it on screen before clicking anything: Home's status card must be showing a real settled verdict ("Stopped"), not "Couldn't read world status", not a blank card, and not "Unknown". If no title is installed, `wowServerDetail()` errors on every poll and this precondition can never be met on that machine — install a title first, or skip this item and say so in the report rather than filing it as a failure.
   - **Why that is a real property of the design, not a fudge:** Rust owns no status poller (deliberately — a second one would duplicate the SOAP client and flap during restarts), so "is a server running" is *only ever* the last verdict the webview PUSHED (`traySetStatus`, and `server-status.svelte.ts:165` pushes it from the poll's success path only). With nothing ever pushed, the honest answer is Unknown, and this feature treats Unknown as "may be running" on purpose — so a machine that has never polled successfully is not a machine with "nothing running", it is a machine that does not know.
   - Do: with the setting still off, make sure no server is running, wait for a poll to land "Stopped", then click the window's X.
   - Expect: the launcher closes immediately — no dialog, no delay, no flicker.
   - If it did not happen: a dialog appearing with nothing at risk is a false alarm, and a feature that cries wolf trains its user to click through the real one; it also means a user who explicitly turned OFF the tray behaviour is being asked something anyway.
   - And the inverse trap: if you skipped the precondition and the launcher closed anyway, that is **not** a pass for this item. With no verdict ever pushed, the close was taken by F1's never-spoke fallback (item 9) and the verdict path this item exists to test was never reached.

4. **Cancel leaves the launcher open with the server still up.**
   - Do: trigger the dialog by any route above with a server running, click **Cancel**.
   - Expect: the dialog closes; the window is exactly as before (visible, usable); the server is still running (Home's status card, or `wsl --list --verbose` still shows `dml-arch` `Running`); the launcher process has not exited.
   - If it did not happen: Cancel stopping the server, or closing the app anyway, turns a routine "not now" into an accidental server stop or an app that will not stay open — the opposite of what the button promises.

5. **Confirming streams the stop to completion before the window closes.**
   - Do: trigger the dialog again, click **"Stop server and close."**
   - Expect: the button relabels to "Stopping…" and disables; a terminal pane appears in the dialog and shows the ordinary `games stop` sequence actually running (log snapshot, pre-stop backup, compose down); only once that stream reaches its terminal event (`done` or `error`) does the window disappear and the process exit.
   - If it did not happen: a window that vanishes before the stream reaches its terminal event means the server is being cut mid-stop instead of given the chance to shut down cleanly — the identical failure this feature exists to prevent, just moved one step later and dressed up as a "confirmed" exit.

6. **"Close anyway" works while a stop is running.**
   - Do: while item 5's confirmed stop is still streaming ("Stopping…" showing, terminal active), click **"Close anyway."**
   - Expect: the launcher closes promptly, without waiting for the stop to finish.
   - If it did not happen: stopping ~2,000 bots is not instant, and a dialog with no escape while it works is how a user reaches for Task Manager — reproducing the exact hard cut this feature exists to prevent. This is the one state where the launcher must remain closable mid-operation.

7. **`wsl --list --verbose` shows the distro stopping, not being cut.**
   - Do: from item 5's confirmed exit, watch the second terminal's `wsl --list --verbose` for the ~30s after the launcher window disappears.
   - Expect: `dml-arch` is still `Running` for a few seconds after the window closes (the holder is only released once `games_stop` has already finished, per the Global Constraints ordering), then transitions to `Stopped` within roughly 15–25s — the same idle-timer-plus-poweroff-grace window `live_wsl_keepalive.rs` measures automatically.
   - If it did not happen: an INSTANT transition to `Stopped` at the same moment the window closes would mean the holder was released before, or without, the stop actually running — reopening the exact hard-cut risk this plan exists to close. A distro that never reaches `Stopped` (stays `Running` with the launcher process gone) means the holder leaked: ~1.4GB of VM held open with no launcher left to manage or release it, discoverable only by chance — a stray `wslrelay.exe`/`vmmemWSL` in Task Manager, or RAM that never comes back.

### Added by fix round 2 (2026-08-05) — items 8–10

Findings F1 and F3 changed **when the launcher refuses to prevent an exit** and **what a failed confirmed stop does**. Both are user-visible, neither was reachable by items 1–7, and item 8 in particular describes an outcome a tester would otherwise report as a bug.

8. **Three UNANSWERED close attempts inside a minute: the launcher stops asking and goes.** (F1 — the bound.)
   - **REWRITTEN BY FIX ROUND 3 (2026-08-05, C2). The earlier wording said only "the third click closes", with no precondition — which pre-blessed the behaviour that turned out to be this branch's worst defect and would have instructed a tester not to file it.** The bound applies to asks *nobody answered*. It is suspended entirely while a confirmed stop is draining — that case is item 11, and there the third click closing is a FAILURE, not a pass. Read both items before running either.
   - **PRECONDITION: no confirmed stop may be in flight.** Every close attempt below ends in **Cancel**; if you click "Stop server and close" at any point, this item is void — restart the launcher and begin again. A stop that is running is an answer in progress, and this item is about the absence of one.
   - **Run this item LAST, or against a server you are willing to lose.** It ends with the launcher closing while the server is up, which means the distro powers off ~15s later and the server dies with it, ungracefully. That is the designed price of the bound; the alternative is a launcher that cannot be closed at all. Restart the server from Home afterwards.
   - Do: `closeToTray` OFF, server running. Click the window's X → dialog → **Cancel**. Within a few seconds click X again → dialog → **Cancel**. Click X a third time, still inside the same minute.
   - Expect: attempts one and two show the dialog; the **third closes the launcher outright** — no dialog, server still running. Then, per item 7's second terminal, `dml-arch` goes `Stopped` ~15–25s later. Now relaunch, start the server again, and let a full quiet minute pass with the window untouched: the next X must show the dialog again.
   - If it did not happen, three distinct failures: (a) the third click prompts again, and so does the fourth and the tenth — the veto is unbounded, which is F1 verbatim. A webview that cannot render the dialog can never answer it, so nothing closes the process and the user reaches for Task Manager, which skips `RunEvent::Exit` entirely and hands the server the hard cut this whole plan exists to prevent. (b) The dialog does **not** come back after the quiet minute — the run of attempts never resets, so three cancels spread across an ordinary working session silently disarm the next real Quit and hard-cut a live server: the plan's own harm, rebuilt out of its fix. (c) The **first** click closes without asking, with a healthy server and a live UI — the bound has collapsed to zero and the dialog is gone as a feature.
   - Rule on this, don't just tick it: two prompts is the entire allowance, and a user who cancels twice out of habit gets no third warning inside that minute. If two is the wrong number, the constant to move is `MAX_UNANSWERED_EXIT_PREVENTIONS` in `launcher/src-tauri/src/lib.rs`. (Ruled 2026-08-05: two stays, and the mid-stop case is handled by suspending the bound rather than by enlarging it — see item 11.)

9. **A launcher whose UI never came up still closes on the first click.** (F1 — the half no amount of ordinary clicking reaches.)
   - Do this one with the **server stopped**, so nothing is at risk. Make sure nothing is serving `http://localhost:1420`, then from the repo root run the debug binary directly: `cargo run -p launcher` — **not** `npm run tauri dev`, which starts vite first and defeats the whole point. A debug build loads `devUrl`, so the webview lands on a connection error and no frontend code ever runs: no `onMount`, no poll, no `tray_set_status`. Right-click the tray icon → **Exit**.
   - Expect: the process exits on that **first** click. (Do not test this with the window's X: `closeToTray` defaults ON, so X only hides — correctly.) Check Task Manager afterwards; nothing named `launcher.exe` should survive.
   - Also observe-if-it-happens, since this shape occurs in the wild: any time the window comes up blank or frozen — a WebView2 update mid-flight, a JS error before mount — Tray Exit must still close it on the first click.
   - If it did not happen: this is the self-reinforcing trap F1 was filed for. No status push → `last_verdict: None` → `Unknown` → `PromptUnknown` → prevent, and the dialog that would answer it is exactly the thing that cannot render. Tray Exit does nothing, X is `prevent_close()`d unconditionally, no UI path closes the process — and the only way out, Task Manager, is a hard kill that skips the polite release. It would also be a straight regression: before this plan the app always terminated.

10. **A confirmed stop that FAILS leaves the launcher OPEN and says so.** (F3.)
   - **Honestly: you cannot induce this safely against the real server.** Every route that makes `dml games stop` fail means breaking Docker or the title's compose files underneath a live server, which is precisely what the safety rules forbid. So this is **observe-if-it-happens**: whenever "Stop server and close" ends in an error instead of the window disappearing, stop and record everything below. Anyone with a throwaway title in a scratch games dir (`DML_GAMES_DIR`) can induce it deliberately by breaking that title's compose file so `compose down` exits non-zero — never against `dml-arch`'s real server.
   - Expect, if it does happen: the launcher **does not close**. The dialog stays open, the terminal pane inside it shows the `error` event, Cancel plus "Stop server and close" become clickable again, **"Close anyway" is still on screen**, and the note beneath the terminal reads *"The stop reported a problem, so the launcher is staying open and your server may still be running. Try again, or use Close anyway to leave regardless."* Then check the second terminal: `dml-arch` must still be `Running` a minute later — the failure arm re-takes the keep-alive hold `games_stop` released on its way out.
   - **The two as-built quirks the previous version of this item told you to record are FIXED (fix round 3, 2026-08-05 — M7/M8) and are now assertions above, not caveats.** If you see either of them, file it: (i) a note claiming *"The launcher is still closing"* is a sentence that is wrong in the one direction that matters — a user told the launcher is leaving, watching it stay, reaches for Task Manager, which is the hard cut this plan exists to prevent; (ii) "Close anyway" missing after the failure means the escape hatch vanished at the exact moment it became the only control that still does anything, leaving Cancel (which does nothing about a server that may still be up) and a Confirm that just failed.
   - Also worth knowing while you are here: the failure arm **resets the run of close attempts** (C2), so a failed stop does not eat into item 8's allowance. After this, the next X must show the dialog.
   - If it did not happen — i.e. the launcher closed anyway on a failed stop: that is F3 verbatim, and **fix round 3 found that F3's own fix never worked** — `run_stream` returns `Ok(code)` for every exit code, so the branch that was supposed to catch this could not be reached. The user clicked a button labelled "Stop server and close", the stop did not happen, and `app.exit(0)` was dispatched to the event loop before the IPC error reached the webview, so the one message that would have told them raced a process exit and lost. Worse, `games_stop` releases the keep-alive holder whatever the outcome, so the distro's 15-second clock is already running underneath containers a failed `compose down` left alive — the ungraceful cut, arrived at through the button that promised the graceful one.

### Added by fix round 3 (2026-08-05) — item 11

Finding C2 changed **what the exit bound does while the launcher is already busy answering**. It is the one item on this list whose expected outcome is the exact opposite of item 8's, which is why it is written separately rather than folded in.

11. **Impatient clicks DURING a confirmed stop must never take the exit.** (C2.)
    - **Do this with a server you are willing to lose, and read the failure branch before you start** — if the fix is not in, this item kills the server mid-`compose down`, which is the harm itself.
    - Do: `closeToTray` OFF, server running, ideally with a real bot population so the stop takes tens of seconds. Click the window's X → dialog → **"Stop server and close."** While the terminal is still streaming (button reads "Stopping…"), click the window's X again. Wait two seconds, click it a third time. Keep clicking every couple of seconds until the stop finishes on its own.
    - Expect: **nothing closes.** The window stays put, the terminal keeps streaming, and the stop runs to its terminal event, after which the launcher exits normally. The clicks are absorbed silently — the dialog is already open and already answering, so there is genuinely nothing new to show; that is the correct behaviour, not a frozen UI. "Close anyway" remains available the whole time and is the way out if you want one.
    - If it did not happen: the launcher exiting on the third click is C2 verbatim, and it is the plan's own harm produced by the plan's own fix. `exit_stop_and_close` is sitting inside the await, holding ground truth that the request IS being answered; a bound that spends its budget there kills the process mid-`compose down`, releases the WSL holder, and the distro powers off ~15s later on top of half-stopped containers. The launcher had every piece of information needed to know better and never asked itself.
    - The other direction is also a failure, and it is why the fix is a suspension rather than a bigger number: once the stop **settles**, the ordinary bound must be back. Let a failed or finished stop leave the launcher open (item 10), then run item 8 — if close attempts can now never take the exit, the veto is unbounded again and F1 is rebuilt.
