// Module-level runes state for live server status (Round Q, Q-T2). Home.svelte
// used to hold `detail` as component-local $state, which meant it went blank
// on every remount until the page's onMount refetch landed -- and the bar/
// sidebar had no way to see it at all. Moving it here (same pattern as
// restart-state.svelte.ts / term-store.svelte.ts) gives every consumer
// (Home, the sidebar chip, Console) an instant last-known render plus a
// single shared poll loop.
//
// The store shape is a flat object (no lazily-created nested objects), so
// plain property writes on `serverStatus` are reactive through the module
// export directly -- no termBuf-style "must return through the proxy" trap
// applies here.

import {
  detectLanIp,
  setKeepAwake,
  traySetStatus,
  wowLan,
  wowServerDetail,
  wowSoapAutosetup,
  type ServerDetail,
} from "./api";
import { featureLocked } from "./features.svelte";
import { installStatusText, type InstallProgress } from "./install-progress.svelte";
import { applyAutosetupOutcome } from "./soap-setup-state.svelte";
import { toolPrefs } from "./tool-prefs.svelte";
import { parseLanStatus, shouldEngageKeepAwake, verdictTransitionActions } from "./transitions";

export const serverStatus = $state({
  detail: null as ServerDetail | null,
  refreshing: false,
  lastError: null as string | null,
  // Batch 2 F6: true while the Rust-side sleep block is engaged (drives the
  // "keeping PC awake" hint next to the sidebar chip).
  keepAwakeActive: false,
  // Transient note after a LAN auto-refresh actually changed the address.
  lanNotice: null as string | null,
  // Batch 3 F10: true while the "AZEROTH IS READY!" toast is showing.
  readyToast: false,
});

// Batch 2 F6 follow-up: keep-awake is engaged only while we BELIEVE the server
// is online, and it is released on an observed online→stopped/crashed
// transition. But a failed poll keeps the stale detail and skips every
// transition, so if polls START failing while the last verdict was online the
// release never fires and the PC stays pinned awake forever. Count consecutive
// failed polls and, past this limit, release the block even though we never
// saw a clean stop.
export const KEEP_AWAKE_FAILURE_LIMIT = 3;
let consecutivePollFailures = 0;

// Pure: past the failure limit, with the sleep block actually engaged, we can
// no longer trust the stale "online" verdict that engaged it -- release it.
export function shouldReleaseKeepAwakeOnFailure(
  consecutiveFailures: number,
  keepAwakeActive: boolean,
): boolean {
  return keepAwakeActive && consecutiveFailures >= KEEP_AWAKE_FAILURE_LIMIT;
}

// Automatic SOAP account setup rides the poll that already runs, rather than
// adding a second one.
//
// `soap.reachable && soap.auth_ok === false` is exactly "the server answered
// and refused us" -- the one state in which an account needs creating. Rust
// re-derives that verdict authoritatively with `soap_bootstrap::classify`
// before it writes anything, so this is a cheap trigger and NOT the decision:
// a false positive costs one `server info` call and nothing else.
//
// `auth_ok === null` means the poll did not determine it. Treating null as
// false would create a GM3 account on evidence we do not have.
export function shouldTryAutosetup(
  detail: ServerDetail | null,
  settled: boolean,
  inFlight: boolean,
): boolean {
  if (settled || inFlight || !detail) return false;
  return detail.soap.reachable === true && detail.soap.auth_ok === false;
}

let autosetupSettled = false;
let autosetupInFlight = false;

// Best-effort by design: a failed autosetup call must never break the status
// poll it rides on. The manual card is still reachable either way.
async function maybeAutosetup(): Promise<void> {
  if (!shouldTryAutosetup(serverStatus.detail, autosetupSettled, autosetupInFlight)) return;
  autosetupInFlight = true;
  try {
    autosetupSettled = applyAutosetupOutcome(await wowSoapAutosetup());
  } catch {
    /* leave it unsettled; the next poll tries again */
  } finally {
    autosetupInFlight = false;
  }
}

// Single-flight: refreshServerStatus can be called concurrently from the
// poll loop, a manual Refresh click, and post-action refreshes -- only one
// underlying request runs at a time, everyone else is a no-op.
export async function refreshServerStatus(): Promise<void> {
  if (serverStatus.refreshing) return;
  serverStatus.refreshing = true;
  try {
    const prev = serverStatus.detail?.verdict ?? null;
    serverStatus.detail = await wowServerDetail();
    serverStatus.lastError = null;
    consecutivePollFailures = 0;
    runTransitionActions(prev, serverStatus.detail.verdict);
    void maybeAutosetup();
  } catch (e) {
    // A failed poll must NOT clobber the last-known detail -- the bar/chip
    // would otherwise blank out on every transient error during a restart.
    // Keep the stale detail, just surface the error for callers that want it
    // (e.g. Home's error card on first load, before anything has ever
    // succeeded).
    const err = e as { message?: string; hint?: string };
    serverStatus.lastError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    // Release keep-awake if polls keep failing while the block is held (see
    // KEEP_AWAKE_FAILURE_LIMIT). The guard on keepAwakeActive makes this a
    // no-op once released, so it fires at most once per failure streak.
    consecutivePollFailures += 1;
    if (shouldReleaseKeepAwakeOnFailure(consecutivePollFailures, serverStatus.keepAwakeActive)) {
      setKeepAwake(false)
        .then(() => (serverStatus.keepAwakeActive = false))
        .catch(() => {});
    }
  } finally {
    serverStatus.refreshing = false;
  }
}

// Impure executor for the pure transition decisions (Batch 2 F6): keep-awake
// engage/release + LAN address auto-refresh. All best-effort -- a failed side
// effect must never break the status poll itself.
function runTransitionActions(prev: ServerDetail["verdict"] | null, next: ServerDetail["verdict"]): void {
  const actions = verdictTransitionActions(prev, next, {
    lanAutoAllowed: !featureLocked("lan-auto-refresh") && toolPrefs.lanAutoRefresh,
  });
  if (actions.keepAwake === "off") {
    setKeepAwake(false)
      .then(() => (serverStatus.keepAwakeActive = false))
      .catch(() => {});
  } else if (shouldEngageKeepAwake(next, !featureLocked("keep-awake") && toolPrefs.keepAwake)) {
    // Asserted on EVERY online poll, not just the transition into online --
    // see shouldEngageKeepAwake for the two releases that would otherwise be
    // unrecoverable. Same heartbeat doctrine as the tray push below.
    setKeepAwake(true)
      .then(() => (serverStatus.keepAwakeActive = true))
      .catch(() => {});
  }
  if (actions.lanRefresh) void lanAutoRefresh();
  if (azerothReadyTransition(prev, next)) fireReadyNotification();
  if (serverWentDownTransition(prev, next)) fireServerDownNotification(next);
  // Tray tooltip — pushed on EVERY successful poll, deliberately NOT only on
  // transitions. It doubles as the heartbeat the Rust keep-awake watchdog
  // waits on: gating this on `prev !== next` meant a steady online server
  // pushed nothing, the watchdog saw >2min of silence and released the sleep
  // block, and back then engagement was transition-only so it could never
  // re-engage. The PC then slept mid-session while the sidebar still claimed
  // it was being kept awake. (Engagement is now a per-poll assertion too, so
  // a watchdog release self-heals on the next poll — but the heartbeat is
  // still what stops the watchdog firing in the first place.)
  // `apply_status` is an idempotent set_tooltip, so a 7s cadence is free.
  void traySetStatus(next).catch(() => {});
}

// Pure: did a server we KNEW was up go down? The mirror of
// azerothReadyTransition, added with the tray: a server dying while the
// window is hidden is exactly what a tray user wants to be told about.
// Deliberately narrow -- only FROM online (so a blip recovering into a stop
// is not reported twice), only TO a settled down-state (soap_unreachable is
// frequently transient, and starting is a deliberate restart), and never on
// prev===null, which would notify on every launch that finds a stopped
// server.
export function serverWentDownTransition(
  prev: ServerDetail["verdict"] | null,
  next: ServerDetail["verdict"] | null,
): boolean {
  if (prev !== "online") return false;
  return next === "stopped" || next === "crashed";
}

// --- "Azeroth is ready" notification (Batch 3 F10) --------------------------

// Pure: did this poll observe the world BECOMING ready? Fires on
// starting→online (the normal boot tail, which restarts also pass through)
// and on stopped/crashed→online (a start that completed between two polls).
// Deliberately NOT soap_unreachable→online: that's a SOAP blip recovering,
// not a boot finishing. prev===null is the very first poll after app launch
// -- the world was (probably) already up before the app started, so no
// notification.
export function azerothReadyTransition(
  prev: ServerDetail["verdict"] | null,
  next: ServerDetail["verdict"] | null,
): boolean {
  if (next !== "online") return false;
  return prev === "starting" || prev === "stopped" || prev === "crashed";
}

let readyToastTimer: ReturnType<typeof setTimeout> | undefined;

// In-app toast + Windows notification, both best-effort. The Windows side
// goes through tauri-plugin-notification's permission dance; any failure
// (permission denied, plugin missing) must never disturb the status poll.
function fireReadyNotification(): void {
  serverStatus.readyToast = true;
  clearTimeout(readyToastTimer);
  readyToastTimer = setTimeout(() => (serverStatus.readyToast = false), 12000);
  void (async () => {
    try {
      const { isPermissionGranted, requestPermission, sendNotification } = await import(
        "@tauri-apps/plugin-notification"
      );
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      if (granted) {
        sendNotification({
          title: "AZEROTH IS READY!",
          body: "The world server is up — time to play.",
        });
      }
    } catch {
      // Best-effort: the in-app toast above already happened.
    }
  })();
}

// The down-notification's executor. Mirrors fireReadyNotification's exact
// permission handling. No in-app toast counterpart on purpose: the whole
// point is to reach the user when NO window is showing.
function fireServerDownNotification(next: ServerDetail["verdict"]): void {
  const body = next === "crashed" ? "The world server crashed." : "The server has stopped.";
  void (async () => {
    try {
      const { isPermissionGranted, requestPermission, sendNotification } = await import(
        "@tauri-apps/plugin-notification"
      );
      let granted = await isPermissionGranted();
      if (!granted) granted = (await requestPermission()) === "granted";
      if (granted) sendNotification({ title: "DML Launcher", body });
    } catch {
      // Notifications are a courtesy; never let one disturb the poll loop.
    }
  })();
}

// After a start finishes (starting→online): if LAN play is on, re-point the
// realm address at this PC's current IP (DHCP may have moved it between
// sessions). The CLI's refresh arm is a deliberate no-op for public/current
// addresses (it refuses to rewrite an internet host's realm) and can also
// fail if the realm DB is unreachable -- and `wow lan` prints its own status
// text rather than erroring the IPC. So the toast must be driven by the
// CLI's own "[ok] LAN address refreshed" success line, NOT by an
// address-changed guess: awaiting the call is not evidence it did anything.
// Pure: did the CLI actually rewrite the realm address? `wow lan refresh`
// prints "[ok] LAN address refreshed: <old> -> <new>" ONLY on a real change;
// it prints "Realm address X is not a LAN address -- leaving it alone" for an
// internet host (exit 0, no change) and "[dml] ERROR: ..." on a DB failure.
// Since wow_lan surfaces all of these as plain text (never an IPC error), the
// success line is the only trustworthy signal that a toast is warranted.
export function lanRefreshApplied(output: string): boolean {
  return /^\[ok\] LAN address refreshed/m.test(output);
}

async function lanAutoRefresh(): Promise<void> {
  try {
    const before = parseLanStatus(await wowLan("status"));
    if (!before.on) return;
    const ip = await detectLanIp();
    if (!ip) return;
    const out = await wowLan("refresh", ip);
    if (lanRefreshApplied(out) && before.ip && before.ip !== ip) {
      serverStatus.lanNotice = `LAN address updated: ${before.ip} → ${ip}`;
      setTimeout(() => (serverStatus.lanNotice = null), 12000);
    }
  } catch {
    // Best-effort: LAN refresh failing must not disturb the status poll.
  }
}

let pollingStarted = false;

// Idempotent: safe to call from every shell mount (StrictMode-style double
// mount, hot reload, multiple call sites) -- only the first call actually
// starts the interval. Polling always runs regardless of which page is
// active; server-detail is cheap and local (no reason to gate it on Home
// being mounted, and the sidebar chip needs it live everywhere).
export function startStatusPolling(): void {
  if (pollingStarted) return;
  pollingStarted = true;
  refreshServerStatus();
  setInterval(() => {
    if (!serverStatus.refreshing) refreshServerStatus();
  }, 7000);
}

// Pure decision helper for the "Restarting..." override: the explicit
// restart-state flag wins the display regardless of the polled verdict --
// polling during a restart otherwise flaps between stopped/starting as the
// containers cycle. Shared by Home's card and the sidebar chip so both
// render identically.
export type StatusLabel = {
  label: string;
  // "crash" (Batch 2 F8) renders as a distinct pulsing red -- a crash must
  // not look like an ordinary stop.
  dot: "on" | "mid" | "bad" | "off" | "crash";
};

export function statusLabel(
  verdict: ServerDetail["verdict"] | null,
  restarting: boolean,
  install: InstallProgress | null,
): StatusLabel {
  // An install in flight wins outright, ahead of even the restart override.
  // During a first install the polled verdict is not just uninformative but
  // WRONG-BY-OMISSION: there is no stack yet, so it reports "stopped" for hours
  // while the machine compiles 1808 objects. `install` is a REQUIRED parameter
  // rather than an optional one precisely so a call site cannot quietly opt out
  // of that and go back to reporting a busy machine as an idle one.
  if (install?.active) return { label: installStatusText(install), dot: "mid" };
  if (restarting) return { label: "Restarting…", dot: "mid" };
  switch (verdict) {
    case "online":
      return { label: "World is up", dot: "on" };
    case "starting":
      return { label: "Starting…", dot: "mid" };
    case "soap_unreachable":
      return { label: "Unreachable", dot: "bad" };
    case "crashed":
      return { label: "Server crashed", dot: "crash" };
    case "stopped":
      return { label: "Stopped", dot: "off" };
    default:
      return { label: "Stopped", dot: "off" };
  }
}

// --- Sidebar chip quick-start (Batch 2 F8) ---------------------------------

// Pure visibility rule for the chip's inline Start button: only when the
// server could actually be started (stopped or crashed) and no restart is in
// flight. A null verdict (never polled / backend unreachable) shows nothing
// -- offering Start with zero knowledge would just produce error noise.
export function chipStartVisible(
  verdict: ServerDetail["verdict"] | null,
  restarting: boolean,
): boolean {
  return !restarting && (verdict === "stopped" || verdict === "crashed");
}

// Cross-page signal: the sidebar chip's Start button navigates to Home and
// asks it to run its own start flow (so streaming lands in Home's terminal
// exactly as if the user clicked Start there). Home consumes the request in
// an $effect.
export const chipStart = $state({ requested: false });

// Pure: distinguishes "installed but currently stopped" (containers were
// created at some point -- compose up ran, they may be exited now) from
// "never installed" (no containers at all, or every row is the CLI's
// "absent" sentinel for a container that was never created / was removed by
// a compose down). Console.svelte uses this to pick its stopped-state
// copy without guessing from `available` alone.
export function containersExist(detail: ServerDetail | null): boolean {
  if (!detail) return false;
  return detail.containers.some((c) => c.state !== "absent");
}
