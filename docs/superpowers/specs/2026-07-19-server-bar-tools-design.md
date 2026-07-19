# Live Server Bar + Tools Page (Round Q) — Design Spec

**Date:** 2026-07-19 · Branch `feat/dml-launcher-windows` · Design review waived (standing).
User requests: (1) server status must update in REAL TIME during start/stop/restart, never
show blank while re-loading, be ALWAYS visible, and its dots must reflect transitional
states (restarting etc.); (2) Console's stopped message should say the server looks
offline, not question the install; (3) add the tray-DML features: LAN play
enable/disable/status, Wrath Unbound addon install/update + uninstall, dml doctor, open
DML shell.

## Ground truth

- Home has NO polling; `detail` is component-local (blank on remount until refetch).
  `restart-state.svelte.ts` has `restartState.restarting/needed` written by every
  lifecycle flow. Sidebar lives in `routes/+page.svelte` (grid `200px 1fr`, NAV from
  `nav.ts`).
- CLI (all existing, TEXT-mode stdout): `dml lan <title> on <lan-ip>|off|status|refresh
  <lan-ip>` (fast, non-interactive; needs the server RUNNING for on/off/status); `dml
  doctor` (~5s, curl internet check inside); `dml unbound` / `dml unbound-remove` —
  INTERACTIVE wizards (fetch-and-exec, prompt for confirmations, force-rebuild the
  worldserver) — must run through the Round D interactive-install machinery
  (spawn_interactive + the single global InstallSlot + InstallTerminal/installStore).
- Title id for LAN: `wow-server-playerbots`.

## Q-T1: Rust + api plumbing

New tauri commands in lib.rs (register all; follow existing patterns):
1. `wow_lan(action: String, ip: Option<String>) -> Result<String, CmdError>` — allowlist
   action ∈ {on, off, status, refresh}; when on/refresh, `ip` REQUIRED and must match
   `^[0-9]{1,3}(\.[0-9]{1,3}){3}$` (reject otherwise, CmdError BAD_ARG-style); runs
   `dml lan wow-server-playerbots <action> [ip]` in TEXT mode capturing stdout+stderr
   combined as the returned String (find the runner's non-JSON capture seam — command_raw
   exists for arg-building; add a small capture runner beside it if none exists, same
   wsl invocation path as everything else).
2. `dml_doctor() -> Result<String, CmdError>` — `dml doctor`, same capture, ~10s budget.
3. `tool_install(tool: String, on_event: Channel, ...) `— tool ∈ {unbound,
   unbound-remove} (allowlist); spawns `dml <tool>` interactively EXACTLY like
   games_install (same InstallSlot atomic reservation — one interactive session
   globally, BUSY otherwise; same chunk streaming; games_install_input/cancel work
   unchanged against it).
4. `open_shell() -> Result<(), String>` — detached `wt.exe wsl -d <distro> -u dml --cd
   /home/dml` with fallback `cmd /c start wsl ...` when wt.exe isn't found (spawn, don't
   wait; distro/user come from the same constants the runner uses).
5. `detect_lan_ip() -> Option<String>` — UdpSocket bind 0.0.0.0:0 → connect
   8.8.8.8:80 → local_addr ip (no packets sent); None on failure.
api.ts: `wowLan(action, ip?)`, `dmlDoctor()`, `toolInstall(tool, onEvent)` (mirror
gamesInstall's shape), `openShell()`, `detectLanIp()`.
Gates: cargo test (25+; add a unit test for the ip regex validator as a pure fn),
check 0/0, vitest green.

## Q-T2: Live server status (store + bar + console message)

1. New `launcher/src/lib/server-status.svelte.ts` (restart-state pattern):
   `export const serverStatus = $state({ detail: null as ServerDetail | null, refreshing:
   false, lastError: null as string | null })` + `export async function
   refreshServerStatus()` (single-flight guard) + `export function startStatusPolling()`
   — module-level setInterval(7s) started ONCE (idempotent, from the shell on mount),
   each tick refreshes unless one is in flight. Polling always runs (server-detail is
   cheap and local).
2. Home.svelte: `detail` local state → the store (instant last-known render on remount;
   its manual Refresh + post-action refreshes call refreshServerStatus()). The card
   additionally shows a "Restarting…" amber state while `restartState.restarting` is
   true regardless of the polled verdict (polling during restart otherwise flaps
   stopped/starting — the explicit flag wins the display). Dots/status colors must
   therefore track live: online green, starting/restarting amber, stopped red,
   soap_unreachable orange.
3. Sidebar (routes/+page.svelte): compact always-visible status chip at the top of the
   sidebar above the nav (dot + short label: "World is up" / "Starting…" /
   "Restarting…" / "Stopped" / "Unreachable"), reading the same store + restartState.
   Clicking it navigates to Home. Colors as above; dot pulses (CSS animation) in the
   amber states.
4. Console.svelte stopped message: read serverStatus — when `available === false`, show
   "The server looks stopped — start it from Home to see live logs." if the store's
   containers exist, else the old "No server logs — is the server installed?".
Tests: pure helpers where reasonable (e.g. a `statusLabel(verdict, restarting)` pure fn
with a truth table). Gates vitest/check.

## Q-T3: Tools page

New nav entry `Tools` (Server section, after Console) → `pages/Tools.svelte`:
- **LAN play** card [flag `lan-play`, LOCKED]: on mount + Refresh → `wowLan("status")`
  → output in a muted <pre>. IP input prefilled via detectLanIp() (editable) +
  buttons Enable (on <ip>), Disable (off), Re-apply (refresh <ip>) — each two-step
  confirm, output shown, disabled while running or when locked. Hint line: other PCs
  need the Windows firewall/portproxy from the DML install (the CLI prints specifics).
- **Wrath Unbound addon** card [flag `unbound-addon`, LOCKED]: explains it layers the
  multi-class addon onto the server and force-rebuilds the worldserver (30-90 min).
  Install/Update button → interactive session via toolInstall("unbound") rendered in
  the SAME InstallTerminal+installStore machinery (installStore.id = "tool:unbound");
  Uninstall button → toolInstall("unbound-remove") (typed-confirm "unbound" first —
  it's destructive: drops tables + rebuilds). Library.svelte's own panel gate must
  exclude `tool:`-prefixed ids (one-line change) so tool sessions render only on Tools.
- **Doctor** card (read-only, UNLOCKED): Run button → dmlDoctor() output in a <pre>
  (spinner while running).
- **DML shell** card (UNLOCKED): button → openShell(); note that it opens a Windows
  terminal inside the distro.
Flags registered `"untested"`; SMOKE-TESTS gains §14 Tools rows ([lan-play]: status
shows current address; enable with the PC's LAN IP → another device on the LAN can hit
the realm (or at minimum status reflects the LAN IP; full two-PC check optional);
disable restores localhost; [unbound-addon]: install runs the wizard interactively to
completion + rebuild, uninstall prompts + reverts — LONG, batch with the modules
sitting; doctor row: all checks report; shell row: terminal opens in the distro).
README launcher feature list += one line. Gates vitest/check.

## Also in this round (no task ceremony)
- SMOKE-TESTS: stopped-server console row ✅ (user-passed tonight) with the message
  change noted in the row.

## Execution
SDD: T1 → T2 → T3 sequential, task review each (T1 opus lens on the IPC surface: the
new commands take webview input — allowlists/regex validation are the boundary), final
gates (bats untouched; cargo/vitest/check + full-suite sanity), dev-install redeploy NOT
needed (no cli/ changes) but cargo rebuild means the user restarts tauri dev.
