# Server Console — Design Spec (Round B)

**Date:** 2026-07-17
**Branch:** `feat/dml-launcher-windows`
**Status:** User waived design review ("I trust you… keep going"); pattern recon'd from The Lab.

## Problem

The user wants to "attach to server console" like the DML server manager. The manager's
`server_attach` uses `docker attach` — dangerous (Ctrl+C kills the worldserver, Ctrl+P/Q
detach is arcane). The Lab's own console (recovered from the binary) is the safe pattern:
a **read-only log stream** + a **SOAP executeCommand send box**. We port that.

## Design

A new **Console** page (Server section, after Library): a worldserver log viewer
(polled tail — the launcher's request-response machinery has no stream-cancel path,
and a 3-second poll of `docker logs --tail` is indistinguishable from follow for a
local tool) + a command box that sends GM console commands over SOAP and shows each
command's reply in a session history.

### CLI: two new verbs (both under `wow`)

**`dml wow console-tail [--lines N] --json`** — read-only. Runs
`docker logs --tail N ac-worldserver 2>&1`, strips ANSI escape sequences (the AC log
is full of color codes; also `ESC[?2004h` bracketed-paste noise) and carriage returns,
returns `{"available":true,"lines":["...", ...]}`. `--lines` defaults to 200, validated
`^[0-9]+$` + base-10 normalized (`10#` — octal-bypass rule) + range 1–1000, else
`BAD_ARG`. Docker down / container absent → `{"available":false,"lines":[]}`, exit 0
(down is an answer). No other error paths.

**`dml wow console-send --command "<text>" --json`** — sends ONE console command via
the existing `soap_exec` (XML-escaped, flock-serialized). Mirrors the `soap-exec` arm's
rc handling (0 → ok, 2 → `SOAP_FAULT` with fault text, 3 → `SOAP_AUTH`, 4 →
`SOAP_UNREACHABLE`), but additionally **decodes the XML entities** in the result text
(`&#xD;` → removed, `&lt;` `&gt;` `&amp;` `&quot;` → literals) so the UI shows real
text instead of entity soup — this is why the existing `soap-exec` verb (kept
untouched, public surface) isn't reused directly. Empty/whitespace command → `BAD_ARG`.
Returns `{"result":"<decoded text>"}`.

**Free-text is deliberate.** This is the explicit manual GM console — the same
capability the public CLI already exposes as `wow soap-exec` and The Lab ships as its
console. The closed-allowlist rule binds *automated/canned* UI actions (GM buttons, bot
commands), not the console, whose entire purpose is operator-typed commands. The
command string reaches SOAP only through `soap_exec`'s XML escaping; nothing is
interpolated into shell.

### Launcher

- **nav**: `{ id: "console", label: "Console" }` appended to the Server section
  (Home, Library, Console). `nav.test.ts` pins updated to match.
- **Console.svelte**:
  - Log viewer: monospace scrollback of the last 200 lines; Refresh button;
    **Auto-refresh** checkbox (default ON, 3 s interval, skips ticks while a fetch or
    send is in flight, interval torn down on unmount via `$effect` cleanup). Sticky
    auto-scroll: after an update, scroll to bottom only if the user was already within
    ~40 px of the bottom (reading scrollback is never yanked away).
  - `available:false` → muted "No server logs — is the server installed?" instead of
    the log box.
  - Command row: text input + Send button (disabled while sending or input blank/
    whitespace); Enter submits. Each send appends `{command, result | error}` to a
    session-local history list rendered newest-last under the input (command echoed
    with a `>` prefix, reply in a `<pre>`); a send failure shows the error inline in
    history (SOAP_FAULT shows the fault text) — never a blocking error card. After a
    send completes (either way), the log tail is refreshed once.
  - No confirmation dialogs — typing a command is the deliberate act (canned GM
    buttons elsewhere keep their two-step confirms).
- **Plumbing**: `wow_console_tail(lines: Option<u32>)` + `wow_console_send(command:
  String)` Rust commands (thin `run_json_cmd` wrappers), `ConsoleTail` type +
  `wowConsoleTail(lines?)` / `wowConsoleSend(command)` in api.ts.

## Testing

- **bats** (`wow-console.bats`): tail — default passes `--tail 200` to docker (args
  captured via a new `DML_STUB_LOGS_ARGS_LOG` seam on the existing logs stub arm),
  `--lines 50` passes `--tail 50`, leading-zero `--lines 050` normalizes to 50 (octal
  pin), `--lines 0`/`1001`/`abc` → `BAD_ARG`, ANSI + CR stripped (fixture with real
  escape bytes), docker down → `available:false` exit 0. send — command text reaches
  the SOAP body (curl capture), entities decoded in result, empty → `BAD_ARG`, fault →
  `SOAP_FAULT` envelope, unreachable → `SOAP_UNREACHABLE`.
- **vitest**: nav pins updated (console in Server section).
- Gates: full bats, vitest, cargo, svelte-check all green.
- **Live gate (batched)**: open Console while the server runs — logs appear and follow;
  send `server info` and see the reply; send a bogus command and see the fault inline;
  stop the server and see "No server logs" state.

Out of scope: true streaming/follow (poll is enough), attach-style TTY (deliberately
rejected), multi-container log selection (worldserver only — auth/db logs are noise
for this audience), command history recall (↑), log download.
