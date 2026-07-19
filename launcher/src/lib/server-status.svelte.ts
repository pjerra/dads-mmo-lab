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

import { wowServerDetail, type ServerDetail } from "./api";

export const serverStatus = $state({
  detail: null as ServerDetail | null,
  refreshing: false,
  lastError: null as string | null,
});

// Single-flight: refreshServerStatus can be called concurrently from the
// poll loop, a manual Refresh click, and post-action refreshes -- only one
// underlying request runs at a time, everyone else is a no-op.
export async function refreshServerStatus(): Promise<void> {
  if (serverStatus.refreshing) return;
  serverStatus.refreshing = true;
  try {
    serverStatus.detail = await wowServerDetail();
    serverStatus.lastError = null;
  } catch (e) {
    // A failed poll must NOT clobber the last-known detail -- the bar/chip
    // would otherwise blank out on every transient error during a restart.
    // Keep the stale detail, just surface the error for callers that want it
    // (e.g. Home's error card on first load, before anything has ever
    // succeeded).
    const err = e as { message?: string; hint?: string };
    serverStatus.lastError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  } finally {
    serverStatus.refreshing = false;
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
  dot: "on" | "mid" | "bad" | "off";
};

export function statusLabel(
  verdict: ServerDetail["verdict"] | null,
  restarting: boolean,
): StatusLabel {
  if (restarting) return { label: "Restarting…", dot: "mid" };
  switch (verdict) {
    case "online":
      return { label: "World is up", dot: "on" };
    case "starting":
      return { label: "Starting…", dot: "mid" };
    case "soap_unreachable":
      return { label: "Unreachable", dot: "bad" };
    case "stopped":
      return { label: "Stopped", dot: "off" };
    default:
      return { label: "Stopped", dot: "off" };
  }
}

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
