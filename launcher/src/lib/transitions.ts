// Pure verdict-transition logic (Batch 2 F6): given the previous and new
// polled verdicts plus the relevant permission flags, decide which side
// effects the status store should run. Extracted from server-status.svelte.ts
// so vitest can cover the whole truth table without touching tauri or
// localStorage.

import type { ServerVerdict } from "./api";

export type VerdictOrNull = ServerVerdict | null;

export interface TransitionFlags {
  // lan-auto-refresh feature flag unlocked AND the Tools toggle on. The
  // keep-awake permission is NOT here: releasing the sleep block ignores it
  // on purpose (turning off is always safe), and engaging is decided by
  // shouldEngageKeepAwake, so a keepAwakeAllowed field would be a flag this
  // function silently ignores.
  lanAutoAllowed: boolean;
}

export interface TransitionActions {
  // "off": release the sleep block; null: leave alone. There is deliberately
  // no "on" here -- ENGAGING is not a transition decision, it's a per-poll
  // assertion (see shouldEngageKeepAwake).
  keepAwake: "off" | null;
  // Run the LAN address auto-refresh flow (only ever on starting→online).
  lanRefresh: boolean;
}

// Pure: should this poll assert the sleep block ON? Deliberately independent
// of the previous verdict AND of whether we believe the block is already
// engaged, because two release paths can drop it with no transition to
// recover on:
//   1. the 3-failed-poll release in server-status.svelte.ts -- polls then
//      recover as online→online, which is not a transition;
//   2. the Rust watchdog (lib.rs), which releases after 120s with no status
//      push and never tells the frontend, so keepAwakeActive stays stale-true.
// Engaging is idempotent at the OS level (power.rs re-sends the current state
// as a no-op), so asserting it on every online poll costs one channel send
// per 7s and makes both releases self-healing.
export function shouldEngageKeepAwake(next: VerdictOrNull, keepAwakeAllowed: boolean): boolean {
  return next === "online" && keepAwakeAllowed;
}

export function verdictTransitionActions(
  prev: VerdictOrNull,
  next: VerdictOrNull,
  flags: TransitionFlags,
): TransitionActions {
  const actions: TransitionActions = { keepAwake: null, lanRefresh: false };
  if (prev !== next) {
    if (next === "stopped" || next === "crashed") {
      // Release even when no longer allowed (flag re-locked / toggle turned
      // off while active): turning OFF is always safe, staying on is not.
      // prev===null is the very first poll after app start -- nothing was
      // ever engaged by us, so skip the pointless call.
      if (prev !== null) actions.keepAwake = "off";
    }
  }
  if (prev === "starting" && next === "online" && flags.lanAutoAllowed) {
    actions.lanRefresh = true;
  }
  return actions;
}

// Parse of `dml lan <title> status` text output (see the lan arm in
// cli/src/90-main.sh):
//   "LAN play: OFF (realm address 127.0.0.1 -- this PC only)"
//   "LAN play: ON  (realm address 192.168.1.50)"
// Unrecognized/error output parses as off with no ip -- the auto-refresh
// flow then does nothing, which is the safe answer.
export function parseLanStatus(text: string): { on: boolean; ip: string | null } {
  const on = /LAN play: ON\b/.test(text);
  const ipMatch = text.match(/realm address ([0-9]{1,3}(?:\.[0-9]{1,3}){3})/);
  return { on, ip: ipMatch ? ipMatch[1] : null };
}
