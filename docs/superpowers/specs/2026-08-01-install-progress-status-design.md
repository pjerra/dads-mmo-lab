# Install progress in the launcher status — design

**Date:** 2026-08-01
**Status:** approved (user, 2026-08-01)
**Scope:** the NATIVE install path only (`dml-wow install-native`), matching the
v0.1.0 scope decision. The WSL install path is explicitly out.

## The problem

A native install takes hours, and almost all of it is one `docker compose build`.
For that whole time the launcher's status chip says **"Stopped"** and Home's
status card says **"Couldn't read world status."** Both are technically true and
completely useless: the machine is working hard and the UI shows a dead server.

## What the user sees when this is done

The sidebar chip and Home's card narrate the install:

```
● Checking your PC…
● Downloading AzerothCore…
● Downloading Playerbots…
● Building…            <- no number yet (apt, cmake configure)
● Building… 3%
● Building… 62%
● Building… 99%
● Starting containers…
● Waiting for the world…
● World is up
```

The percentage is real or absent. It is never estimated.

## Where the percentage comes from

AzerothCore's Dockerfile configures with **ninja** (`apps/docker/Dockerfile:67`
installs `ninja-build`, line 105 runs `cmake --build`). Ninja prints a step
counter with a known denominator, and BuildKit's plain progress passes it
through verbatim. From a real build log on this machine
(`dml-native/native-test/logs/build-20260731-210436.log`, 1739 such lines):

```
#26   3.703 [16/1808]   Building CXX object deps/fmt/CMakeFiles/fmt.dir/src/os.cc.o
#26 782.2   [1803/1808] Building CXX object modules/CMakeFiles/modules.dir/mod-playerbots/src/Util/PlaceholderHelper.cpp.o
```

Those lines already reach the launcher: `Engine::run_echo` streams combined
stdout+stderr line by line as `line` events (and tees them to
`<title>/logs/build-*.log`). No new subprocess, no polling, no log tailing.

### Three parsing rules, each from the real logs

1. **The bracket must contain digits and a slash, nothing else.** BuildKit's
   vertex headers look like `#7 [ac-client-data-init skeleton 2/4] RUN mkdir …`
   — a `2/4` that is a *Dockerfile stage step*, not compile progress. Matching it
   would make the bar leap to 50% during a 0.1s `mkdir`. The parser requires the
   bracket to be the token immediately after the elapsed-seconds field and to
   contain only `<digits>/<digits>`.

2. **Four images build in parallel**, so fractions from different `#N` vertices
   interleave. The tracker trusts the vertex with the **largest total** — the
   1808-step worldserver compile, not a 3-step sidecar — and ignores smaller
   ones rather than letting them fight for the display.

3. **Monotonic clamp.** A reported percentage never goes down, whatever arrives.
   A bar that goes backwards reads as a bug even when the underlying number is
   honest.

Plus a `total == 0` guard, so a malformed line can never divide by zero.

## Architecture

### Rust emits, the frontend renders

The parser lives in Rust and emits the **`pct` event**, which the `TermEvent`
union has had reserved for exactly this. Three reasons this is the right home
rather than parsing in TypeScript:

- `install-native` is native-only **by design** (bash's `_installers_supported`
  refuses on Windows), so there is no bash mirror and this does not half-ship.
- The CLI gains progress for free, which is the stated point of the Rust
  workspace: any frontend can drive the server without the launcher.
- A second frontend would otherwise re-implement the parser.

### Event shape

```json
{"event": "pct", "value": 62}
```

`value` is an integer 0–100. Emitted **only when the number changes and only
upward**, so a 1808-step build produces at most 101 events, not 1808. It carries
no stage name: consumers already know the stage from `section_start`.

`pct` is advisory. Every existing consumer ignores unknown events, and
`translateNativeEvent` must keep returning nothing for it — the install
terminal shows the raw build wall, not a percentage.

### Components

| Unit | File | Job |
| --- | --- | --- |
| `pct_event` | `crates/dml-core/src/events.rs` | the event constructor, beside its siblings |
| `parse_build_step` | `crates/dml-wow/src/install_native.rs` | pure: one line → `Option<BuildStep>` |
| `BuildProgress` | same | the tracker: largest-total wins, monotonic, emit-on-change |
| `Engine::run_echo_with` | same | `run_echo` plus a per-line hook; `do_build` passes the tracker |
| `installProgress` | `launcher/src/lib/install-progress.svelte.ts` | module-level runes store, survives navigation |
| `installProgressReduce` | same | pure: (state, TermEvent) → state |
| `statusLabel` | `launcher/src/lib/server-status.svelte.ts` | gains the install override |

The store is module-level for the same reason `restart-state.svelte.ts` is: a
component-local one dies when the user navigates away from Library, and the
whole point is that the chip keeps narrating from every page.

### Precedence

```
installing  >  restarting  >  polled verdict
```

An install in flight wins the display outright. During a first install the
polled verdict is meaningless anyway — there is no stack yet.

## The Home trap

Home's status card does **not** call `statusLabel`. It duplicates the verdict
chain inline in markup, and the entire card is wrapped in
`{#if serverStatus.detail}`. On a fresh install there is no compose file yet, so
`server-detail` errors, `serverStatus.detail` stays null, and Home renders the
`{:else if serverStatus.lastError}` error card — **"Couldn't read world status"**
— for the entire multi-hour build.

So the install branch must sit **above** that gate, not inside the verdict
chain. Home then reuses `statusLabel()` for the dot and headline and keeps its
own per-verdict body copy.

## Lifecycle

- `startInstall()` resets the store (new nonce, `active = true`).
- `section_start` sets the stage; `pct` sets the percentage.
- Leaving the `build` stage clears the percentage, so a later stage can never
  display a stale 99%.
- `done` and `error` both clear `active`. So does the runner's IPC-rejection
  path (`exit: -1`) — a store left `active` forever would pin the chip to
  "Building…" until the app restarts.

## Testing

Rust unit tests, fixtures taken verbatim from the real build log:

- a compile line parses to its fraction;
- the `[ac-client-data-init skeleton 2/4]` vertex header parses to `None`;
- `#26 DONE 12.3s`, `#7 CACHED` and the ` Image … Building ` header parse to `None`;
- a smaller-total vertex cannot overwrite a larger-total one;
- a fraction that would move the number down emits nothing;
- `total = 0` emits nothing;
- the same percentage twice emits once.

Vitest:

- the reducer's stage transitions, the build-stage exit clearing `pct`, and
  `done`/`error` clearing `active`;
- `statusLabel` precedence — installing beats restarting beats verdict;
- `translateNativeEvent` still returns nothing for `pct` (the existing pinned
  test must stay green).

No live gate. The fixtures are real build output, so the parser is verified
against the thing it parses rather than against an idea of it.

## Out of scope

- The WSL install path (raw pty chunks). Same ninja lines would appear; adding
  it later is a small change, but v0.1.0 is native-only.
- A progress bar widget. This round is text in the existing status surfaces.
- Percentages for the clone stages. `git clone` can report
  `Receiving objects: N%`, but only when `--progress` is forced on a non-TTY,
  which the engine does not currently pass. Recorded, not built.
