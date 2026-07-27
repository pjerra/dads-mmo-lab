# Launcher self-configuration + system tray — design

**Status:** approved design, not yet planned or built.
**Date:** 2026-07-27.
**Roadmap entries this replaces:** "The release build is not self-configuring"
and "System-tray presence for the launcher" in
`docs/superpowers/plans/2026-07-20-post-smoke-roadmap.md` (Round 2).

## Problem

Double-clicking the built `launcher.exe` produces a launcher that reports a
running server as **offline** and whose bash-backed features fail. Neither is a
bug in the code paths themselves — the app simply has no configuration and no
way to acquire any.

Two independent causes:

1. **Backend.** `DML_BACKEND` unset means `Backend::Wsl`
   (`crates/dml-core/src/backend.rs:34-44`), so the launcher queries the bash
   CLI inside the `dml-arch` distro. On a machine running the native Docker
   Desktop server that is a *different install*, and it really is stopped — the
   launcher reports the truth about the wrong server. The UI can read the mode
   (`backend_mode`) but cannot change it, and nothing auto-detects.
2. **Paths.** `DML_GAMES_DIR`, `DML_SCRIPT` and `DML_YQ_BIN` are equally unset
   for an installed app, and their unset-defaults disagree with each other
   (below). The working dev-mode script sets all four; nothing sets them for the
   exe.

Separately, the launcher has **no system tray at all** (`tauri` declared with
`features = []`, no `trayIcon` in `tauri.conf.json`), and closing the window
exits the app — so there is nothing to return to.

## Decisions taken

| Decision | Choice | Rationale |
|---|---|---|
| Who the installed build serves | **Both, staged** | Ship self-configuration now; bundling `cli/` into the installer is a separate later piece. |
| Backend selection | **Auto-detect + visible, changeable setting** | Zero-config on a fresh install, but never a mystery — the user can see and override what was picked. |
| Config storage | **`~/.dml/launcher.json`, read by Rust** | The backend must be known at startup, before any window: the tray shows status while minimised, and every Rust command needs the mode. |
| Window close | **Hides to tray; app keeps running** | This is what makes "return to the launcher from the tray" true. Quit stays explicit in the tray menu. |
| Tray scope | Status in icon/tooltip, Start/Stop, start-with-Windows, state-change notifications | All four requested. |
| Tray status source | **Pushed from the frontend** | See "Why push, not poll". |

## Why push, not poll

Status polling today is **entirely frontend-driven**: `startStatusPolling()` runs
a 7-second `setInterval`, single-flighted, started from the shell's `onMount`
(`launcher/src/lib/server-status.svelte.ts:54-56,185-199`). Rust knows nothing
about server state.

Hide-to-tray keeps the webview alive, so that poll keeps running. The tray
therefore takes a **push**: the frontend calls a `tray_set_status(verdict)`
command on transition and Rust renders icon + tooltip.

A Rust-side poller was rejected for two concrete reasons, not economy:

- It would open a **second, unserialized SOAP client** alongside the frontend's.
- It would **flap during restarts**. The `restarting` suppression flag lives
  only in the webview (`launcher/src/lib/restart-state.svelte.ts`), so a Rust
  poller would see the verdict oscillate `stopped`/`starting` as containers
  cycle, and would double-fire the "ready" notification that
  `azerothReadyTransition` already handles correctly.

Pushing inherits both behaviours for free.

**The cost of pushing** is that a hidden webview may have its timers throttled by
WebView2, which would stall the pushes. That is covered by the keep-awake
watchdog below, and the tray tooltip carries an "as of" timestamp so a stalled
status is visible rather than silently wrong.

## Architecture

### Config resolution (phase 1)

One resolver answers four questions — which backend, which games dir, where the
bash script is, where yq is — with a single documented precedence:

```
DML_* env var  →  ~/.dml/launcher.json  →  auto-detect  →  honest "not found"
```

**Env must stay highest.** All 18 parity suites, the bats suite and the CLI
integration tests inject these variables as override seams; if `launcher.json`
outranked them, tests would start reading a developer's persisted config.

Pure decision logic lives in `dml-core` as functions over probe *results*
(`detect(native_dir_exists, docker_present) -> Backend`), unit-testable with no
filesystem; thin IO wrappers do the probing. This extends the seam
`backend.rs` already has (`from_override` pure, `selected` reads env) rather
than inventing a pattern. Values parsed from `launcher.json` reuse
`from_override`, inheriting its typo-safety (an unrecognised value falls back
rather than stranding the user on an unfinished backend).

**Resolved values are exported into child processes.** Native children inherit
the launcher's environment (`crates/dml-core/src/runner.rs:154-160`), so
exporting the resolved `DML_GAMES_DIR`/`DML_YQ_BIN` fixes the bash side for
free. This collapses a real three-way divergence in today's unset-defaults:

| Consumer | Default when unset | Adequate for an installed app? |
|---|---|---|
| Rust title readers | `"."` (process CWD) | No — a Start-menu launch has no meaningful CWD |
| Launcher yq helper | `~\dml-native\tools\yq.exe` | Yes |
| bash CLI | `~/games` | No |

Note this export does **not** cross into WSL mode — Windows environment
variables do not reach the distro and there is no `WSLENV` wiring in the repo.
That is acceptable: WSL mode is the path that already works.

### `~/.dml/launcher.json`

```json
{
  "backend": "auto",
  "gamesDir": null,
  "dmlScript": null,
  "yqBin": null,
  "closeToTray": true,
  "startWithWindows": false
}
```

Every field is optional; absent means "work it out". `"backend"` takes
`auto` | `native` | `wsl`, default `auto`, so the file records *intent* rather
than a frozen answer and a machine that changes (Docker installed later, distro
removed) re-resolves correctly.

The three path fields are deliberate overrides, normally null, written when
detection cannot find something and the user points at it via a picker.

Behaviour follows its `~/.dml/` neighbours (`soap.env`, `client-path`): a
missing file is the normal first-run state meaning all-defaults; a corrupt or
partially-invalid file degrades **per field** to defaults rather than erroring;
writes go through a temp file and rename so a crash cannot leave a broken
config.

`localStorage` is left alone. `closeToTray` and `startWithWindows` live here
because Rust owns both behaviours (the close handler and the registry write).
Existing preferences (`dml.autoShutdown`, `dml.nativeManageDocker`, …) are NOT
migrated — that would be unrelated churn, and the resulting split is documented
rather than pretended away: **frontend-only preferences stay in `localStorage`;
anything Rust must know before a window exists goes in `launcher.json`.**

### Tray (phase 2)

Built in the existing `.setup()` hook, which currently binds `_app` unused and
spawns the interval-backup watcher — building a tray there requires only using
the app handle.

Only one build change is needed: `tauri = { version = "2", features = ["tray-icon"] }`.

- **Permissions need no change.** `core:default` already includes
  `core:tray:default` and `core:menu:default`
  (`launcher/src-tauri/capabilities/default.json`), and a tray built in Rust is
  not subject to the capability system at all.
- **The icon needs no new feature.** `icons/icon.ico` is embedded at compile
  time, so `app.default_window_icon().cloned()` supplies it. Loading an icon at
  runtime instead would require the non-default `image-ico`/`image-png`
  features.
- **Trap for the implementer:** `tray-icon 0.24.1` already appears in
  `Cargo.lock` as an *optional* dependency. That is not evidence the feature is
  enabled — `TrayIconBuilder` will not compile without the Cargo change above.

Tray contents: **Open** (also on left-click), a status line, **Start** /
**Stop**, **Exit**. Start/Stop reuse the existing lifecycle commands and the
same confirmation the Home card uses — a destructive-ish action must not be one
unguarded click away with no window open.

The window's label is `"main"` and is load-bearing in three places
(`tauri.conf.json` default, `capabilities/default.json`, and the existing
`get_webview_window("main")` fallback in `set_taskbar_progress`); the show/hide
handler reuses it.

## Error handling and edge cases

**Ambiguous detection.** Native wins when both a native title dir and a working
WSL distro exist — it is the faster path and the one FOR-TESTERS already
recommends. The safeguard is not a cleverer probe: Settings shows what was
detected and why, and the dropdown overrides it permanently.

**The bridge stops lying.** `bridge_setup_stream` currently emits
`done{changed:false}` when `deploy_scripts` finds no families
(`crates/dml-wow/src/bridge.rs:56-66,152-166`) — it *reports success while
deploying zero lua files*, which is how "Enable My Party" can appear to work and
then not function, with nothing pointing at the cause. It becomes a real error
naming the cause. **Coupling constraint:** the lua root is
`<parent of DML_SCRIPT>/lua`, so any future bundled-script layout must keep
`lua/` a sibling of the script file or bridge deploys break even with
`DML_SCRIPT` resolved.

**Unresolvable `DML_SCRIPT`** surfaces where it bites — the affected features
show "this needs the bash CLI" with a picker writing `launcher.json.dmlScript` —
rather than failing generically at spawn time. The features that still need it
are: games list / catalog / install, url install, tool install, doctor, the
realmlist arms, and the auto-shutdown watcher. Module operations and self-update
do **not** — those are already ported.

**Env override active** makes the Settings dropdown read-only with a note
saying so; otherwise changing it appears to do nothing.

**Keep-awake watchdog.** `SetThreadExecutionState` is held by a Rust thread but
*driven by the webview poll* (`launcher/src/lib/server-status.svelte.ts:39-49`),
and today it cannot leak because process exit clears it
(`RunEvent::Exit → power::keep_awake(false)`). Hide-to-tray removes that
guarantee: if WebView2 throttles the hidden window's timers the poll stops and
the machine stays pinned awake indefinitely. Rust releases keep-awake if no
status push arrives for two minutes.

**Tray Exit** routes through `app.exit()` so the existing `RunEvent::Exit` arm
still clears keep-awake. A window destroy would bypass it.

**Second instance.** Launching the exe again while hidden must focus the
existing window, not start a second app fighting over the same server.

**Autostart** writes `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`
**directly from Rust — no new dependency**. `tauri-plugin-autostart` was
considered and rejected: it adds a crate, and driving it from the frontend also
needs a permission in `capabilities/default.json` or the toggle silently
no-ops — a failure mode with no compile-time signal. A direct write keeps the
toggle on the same Rust path as every other `launcher.json` setting, and this
is the repo's first registry *write* (existing access is a read-only
`reg query` for WSL detection), so it should be deliberate rather than
inherited.

The entry records the exe path from `std::env::current_exe()` and self-disables
if that path no longer exists — dev and installed builds differ
(`target\debug\launcher.exe` vs the installed location, and both NSIS and MSI
ship), so a stale entry pointing at a deleted build is a realistic state.

**Notifications** reuse the existing `azerothReadyTransition` logic rather than
adding a second source of truth, so they cannot double-fire.

## Testing

- **Pure, no filesystem:** `detect()`, precedence resolution, `launcher.json`
  parse/merge, and the `verdict → icon/tooltip` mapping. These carry the design's
  real logic and are all unit-testable in `dml-core` / a thin tray module.
- **Round-trip with a temp home:** config write/read, missing file, corrupt
  file, partially-invalid file.
- **Frontend (vitest):** the Settings row state machine, including the
  env-override read-only state.
- **Manual gates** (genuinely not automatable): hide-to-tray and reopen; Exit
  clears keep-awake; a second launch focuses the existing window; the autostart
  entry appears and disappears with the toggle.

## Non-goals

- Bundling `cli/` into the installer (staged; a separate piece).
- Migrating existing `localStorage` preferences.
- Porting the residual bash-backed features to Rust — that would retire
  `DML_SCRIPT` entirely, and is the natural follow-up once this lands.
- Non-Windows tray behaviour.
- A first-run setup wizard.

## Sequencing

Phase 1 (self-configuration) must land before phase 2 (tray). A tray app that
starts with Windows is exactly the case where "needs four env vars set by a
wrapper script" breaks down — it would auto-start into WSL mode and show the
server offline.
