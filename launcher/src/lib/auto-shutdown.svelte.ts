// Auto-shutdown store (Batch 2 F5): shared runes state for the Tools card,
// plus the app-start re-assert and the "auto-shutdown" tauri event listener.
// Module-level (same pattern as restart-state / server-status) so the card's
// status survives page hops and the listener exists exactly once regardless
// of which page is mounted.

import { listen } from "@tauri-apps/api/event";
import { setAutoShutdown } from "./api";
import { featureLocked } from "./features.svelte";
import { refreshServerStatus } from "./server-status.svelte";

const STORAGE_KEY = "dml.autoShutdown";

export type AutoShutdownState = "off" | "waiting" | "armed";

export const autoShutdown = $state({
  enabled: false,
  state: "off" as AutoShutdownState,
  notice: null as string | null,
  error: null as string | null,
});

// Same guarded-storage idiom as features.svelte.ts: storage may be missing
// (node-environment tests) or throw (privacy mode) -- fall back to in-memory.
function readStored(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function writeStored(on: boolean): void {
  try {
    if (typeof localStorage !== "undefined") localStorage.setItem(STORAGE_KEY, on ? "1" : "0");
  } catch {
    // In-memory state still applies for this session.
  }
}

export type AutoShutdownOutcome = "stopped" | "stop_failed" | "not_running" | "unknown";

type AutoShutdownEvent =
  | { kind: "state"; state: "waiting" | "armed" }
  | { kind: "fired"; outcome: AutoShutdownOutcome };

// Pure copy helper (unit-testable): the notice shown after the watcher fires.
// Each outcome tells the honest truth -- crucially "stop_failed" no longer
// masquerades as "nothing to stop", so the user isn't told the world is off
// while it keeps running.
export function firedNotice(outcome: AutoShutdownOutcome): string {
  switch (outcome) {
    case "stopped":
      return "Server stopped automatically because WoW was closed.";
    case "stop_failed":
      return "WoW was closed, but the server could not be stopped automatically — please stop it from Home.";
    case "unknown":
      return "WoW was closed, but the server's state couldn't be checked — please check it from Home.";
    case "not_running":
    default:
      return "WoW was closed, but the server wasn't running — nothing to stop.";
  }
}

let initStarted = false;

// Idempotent app-start hook (called from the shell's onMount): re-asserts the
// persisted toggle to the backend -- the watcher lives in the Rust process
// and knows nothing about localStorage -- and subscribes to watcher events.
// A LOCKED flag means the watcher is never (re-)engaged, even if a previous
// session left the toggle on.
export function initAutoShutdown(): void {
  if (initStarted) return;
  initStarted = true;
  listen<AutoShutdownEvent>("auto-shutdown", (e) => {
    const p = e.payload;
    if (p.kind === "state") {
      autoShutdown.state = p.state;
    } else if (p.kind === "fired") {
      autoShutdown.notice = firedNotice(p.outcome);
      // The chip/Home card should reflect the stop right away, not on the
      // next 7s poll tick.
      refreshServerStatus();
    }
  }).catch(() => {
    // Not running under tauri (vitest/browser dev) -- the card still renders.
  });
  autoShutdown.enabled = readStored();
  if (autoShutdown.enabled && !featureLocked("auto-shutdown")) {
    setAutoShutdown(true)
      .then(() => (autoShutdown.state = "waiting"))
      .catch((err) => (autoShutdown.error = String((err as { message?: string }).message ?? err)));
  }
}

export async function setAutoShutdownEnabled(on: boolean): Promise<void> {
  if (on && featureLocked("auto-shutdown")) return; // locked: never engage
  autoShutdown.enabled = on;
  autoShutdown.notice = null;
  autoShutdown.error = null;
  writeStored(on);
  try {
    await setAutoShutdown(on);
    autoShutdown.state = on ? "waiting" : "off";
  } catch (err) {
    autoShutdown.error = String((err as { message?: string }).message ?? err);
    autoShutdown.state = "off";
  }
}

// Pure copy helper (unit-testable without DOM): the status line under the
// toggle.
export function autoShutdownLabel(enabled: boolean, state: AutoShutdownState): string {
  if (!enabled || state === "off") return "Off.";
  if (state === "armed") return "Armed — the server will stop shortly after WoW closes.";
  return "Waiting for WoW to start…";
}
