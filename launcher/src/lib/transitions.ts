// Pure verdict-transition logic (Batch 2 F6): given the previous and new
// polled verdicts plus the relevant permission flags, decide which side
// effects the status store should run. Extracted from server-status.svelte.ts
// so vitest can cover the whole truth table without touching tauri or
// localStorage.

import type { ServerVerdict } from "./api";

export type VerdictOrNull = ServerVerdict | null;

export interface TransitionFlags {
  // keep-awake feature flag unlocked AND the Tools toggle on.
  keepAwakeAllowed: boolean;
  // lan-auto-refresh feature flag unlocked AND the Tools toggle on.
  lanAutoAllowed: boolean;
}

export interface TransitionActions {
  // "on": engage the sleep block; "off": release it; null: leave alone.
  keepAwake: "on" | "off" | null;
  // Run the LAN address auto-refresh flow (only ever on starting→online).
  lanRefresh: boolean;
}

export function verdictTransitionActions(
  prev: VerdictOrNull,
  next: VerdictOrNull,
  flags: TransitionFlags,
): TransitionActions {
  const actions: TransitionActions = { keepAwake: null, lanRefresh: false };
  if (prev !== next) {
    if (next === "online") {
      // Engage only when allowed -- a locked flag or disabled toggle must
      // never turn the sleep block on.
      if (flags.keepAwakeAllowed) actions.keepAwake = "on";
    } else if (next === "stopped" || next === "crashed") {
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
