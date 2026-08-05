# Keeping the server alive, and stopping it honestly

**Status:** design approved by the user 2026-08-05. Branch `feat/arch-wsl-backend`.

**One sentence:** the WSL distro that hosts the server dies 15 seconds after the
last session into it exits, so the launcher must hold it open on *every* distro
backend — and closing the launcher must stop the server cleanly instead of
letting WSL power-cut it.

---

## Why

Measured on this machine on 2026-08-05, n=8, 14.7–14.9 s, spread 0.2 s: **WSL
2.7.10 powers a distro off ~15 seconds after the last `wsl.exe` client session
into that distro exits, regardless of what is running inside it.** The guest
journal names WSL's own init issuing `poweroff`, after which `dockerd` logs
`Daemon shutdown complete`.

This is not theoretical. During the backend comparison it terminated `dml-arch`
with the user's real server and **1,948 playerbots live**. Because systemd could
not stop that many containers inside WSL's 10-second grace, the sequence ended in
`reboot(RB_POWER_OFF)` — a hard cut, not a shutdown.

Two facts make it worse than a single outage:

* **`restart: unless-stopped`** is set on three services, so the stack is
  *self-healing on next touch*. The user sees a server that is up every time they
  look and down whenever their friends try to connect.
* Nothing in the product says any of this is happening.

`wsl_keepalive.rs` was built to answer it and works — proven both directions: it
held the distro for 62.1 s (4.1× the deadline) and the distro died 16.5 s after
release. **But it arms for `Backend::Arch` only**, and `Backend::Arch` is not
selectable from the UI. The default, `Backend::Wsl`, drives the *same*
`dml-arch` distro through the same `runner::DISTRO` constant and has the
identical exposure. So today the fix protects a backend nobody runs.

---

## Decisions taken (user, 2026-08-05)

| # | Question | Decision |
|---|---|---|
| 1 | Order of work | **Safety before features.** This spec ships before multi-title switching and before porting the remaining bash verbs. |
| 2 | Which backends hold the distro open | **Every distro backend** — `Arch` and `Wsl`. `Native` never. |
| 3 | What closing the launcher does | **Warn, then stop cleanly.** Not a silent stop, not a silent cut. |
| 4 | Multi-server scope (context only) | One title running at a time; switching, not simultaneous. Not this spec. |

---

## Part 1 — hold the distro open on every distro backend

`wsl_keepalive::applies_to` currently answers `Arch => true`, `Wsl => false`,
`Native => false`. It becomes `Arch | Wsl => true`, `Native => false`.

**Why `Wsl` is safe to change.** The holder is purely additive. It spawns one
`wsl.exe -d <distro> -u <user> --exec /bin/sleep infinity`, alters no command the
launcher issues, and exists only while the server is *meant* to be running. It
cannot change what any verb does, because it is not in any verb's path.

**Why `Native` stays false.** Docker Desktop keeps its own utility VM alive for
its containers. There is no distro to hold and nothing to fix.

**The honest cost.** Holding the distro open keeps the WSL VM resident, which is
part of the memory the comparison measured. That is the correct trade: the server
needs the VM to exist. It is a cost paid only while a server should be running,
not at idle.

---

## Part 2 — the exit contract

### The hook

`RunEvent::ExitRequested`, **not** `RunEvent::Exit`. This is the technical crux:
`ExitRequested` can be prevented (`api.prevent_exit()`); `Exit` cannot. The
current code hooks `Exit` (`lib.rs:7764`), which is too late to ask anything.

The tray's Quit already routes through `app.exit(0)` (`tray.rs:90`), so it reaches
the same hook — no second path to maintain.

### The flow

1. On `ExitRequested`, ask: is a server running, and is this a distro backend?
2. **Native backend:** never prompt. Exit immediately, exactly as today.
3. **No server running:** never prompt. Exit immediately. This is the common case
   and must stay frictionless.
4. **Server running on a distro backend:** prevent the exit and show a dialog:
   *"Your server is running. Closing DML Launcher will stop it."* with
   **Stop server and close** / **Cancel**.
5. On confirm: run the ordinary `games stop` path — the one the Stop button uses,
   which already takes the bounded worldserver log snapshot and the pre-stop
   backup — **then** release the holder, **then** exit. That converts a power cut
   into the same clean stop the user could have performed by hand.
6. Show progress while it runs. Stopping ~2,000 bots is not instant, and a window
   that appears frozen is how a user reaches for Task Manager, which reproduces
   the exact failure this spec exists to prevent.

### The tri-state case

"Is a server running?" is a probe, and this project's standing rule is that a
probe which cannot answer is evidence of nothing. If the status is **unknown**,
prompt anyway, with honest wording: *"Couldn't confirm whether your server is
running. Closing DML Launcher may stop it."*

The asymmetry justifies it: a needless dialog costs one click, and a missed one
costs a database killed mid-write.

### Escape hatches

The dialog must never trap the user in an unclosable application.

* The clean stop is **bounded**. If it exceeds its budget, say so and offer to
  close anyway — an honest "this is taking longer than expected" beats a window
  that will not close.
* If the stop **fails**, report the failure and offer to close anyway. The user
  is entitled to close their launcher even when the server misbehaves.
* A second `ExitRequested` while the stop is already running must not start a
  second stop. Re-entrancy is guarded.
* Force-kill remains covered by the existing `KILL_ON_JOB_CLOSE` job object, so
  no holder is leaked whatever happens.

### What the user is told, outside the modal

The backend picker in Settings gains one line of copy, because this is a real
difference between the two backends someone is choosing between:

> On Docker Desktop your server keeps running when you close the launcher. On
> WSL it cannot — Windows shuts the distro down shortly after, so the launcher
> stops the server for you.

That belongs next to the choice, not only in a dialog that appears once the
decision has already been made.

---

## What this does NOT deliver

Stated plainly, because the gap is the reason to keep Docker Desktop:

**This buys "the server runs while the launcher runs" — not unattended hosting.**
True unattended hosting on WSL needs something that outlives the launcher
process: a Windows service, or a scheduled task holding the session. That is a
separate piece of work and a genuine point in Docker Desktop's favour that no
amount of launcher work erases.

Also out of scope: multi-title switching; porting the remaining bash verbs;
`vmIdleTimeout` (measured — it does not prevent this, and it destroys the idle
memory saving that is the Arch backend's whole point).

---

## Testing

**Unit, deterministic, behind the existing seams.**
* `applies_to` for all three backends.
* The exit decision as a pure function of (backend, server status tri-state):
  prompt / no-prompt / prompt-with-uncertain-wording. Every combination.
* Re-entrancy: a second exit request during an in-flight stop starts no second stop.
* The bound: a stop that overruns yields the close-anyway path, not a hang.

**Production wiring must be pinned, not just the logic.** The keep-alive's own
wiring was previously untested — deleting all five call sites left the suite green
at 234/0, because with `install()` gone no holder was ever spawned, silently.
That guard now exists; this spec's wiring gets the equivalent. The ordering is
part of the contract and is asserted: the clean stop runs **before** the holder is
released, because releasing first re-creates the 15-second cut the stop exists to
avoid.

**Anti-vacuity is mandatory.** This branch has produced six tests that could not
fail, most caught only by mutating production code and watching for red. Every
new test here gets that treatment, and the mutation is recorded.

**Live.** One `#[ignore]`d test proving the distro survives past 15 s on
`Backend::Wsl` specifically — the backend this spec exists to protect — and dies
after release. Half one alone would also pass for `wsl --list` polling, which is
measured *not* to work; the death-after-release is what identifies the holder as
the cause.

**Not automated.** The dialog itself needs a human: `npm run tauri dev`, a running
server, and a real click of both Quit and Cancel. Tray Quit and window-close are
currently code-inspected only, and this spec is exactly the change that makes
that insufficient.

---

## Risks

1. **The clean stop is slow.** ~2,000 bots take real time. Mitigated by progress
   and a bound, but a user in a hurry will feel it. Accepted: the alternative is
   the corruption this spec exists to prevent.
2. **A dialog on exit is friction.** Mitigated by never showing it when no server
   is running, which is the common case.
3. **Holding the distro open costs memory.** Real, and disclosed in the
   comparison document. Paid only while a server should be running.
4. **`Backend::Wsl` is the backend the user's live server runs on.** Any change
   there deserves care. This one adds a process and touches no command path,
   which is the least invasive shape available.
