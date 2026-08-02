// Module-level runes state so the pending-restart signal survives Config.svelte
// being unmounted (e.g. the user switches sidebar pages) and remounted later.
// `apply` records WHICH restart can land what was saved -- the pages that save
// live nowhere near Home, where the restart buttons are, so the strength has to
// travel with the signal. See lib/apply-needed.ts for why the distinction
// exists.
import { escalate, type ApplyNeeded } from "$lib/apply-needed";

export const restartState = $state({
  needed: false,
  restarting: false,
  /**
   * A STOP is in flight.
   *
   * Sibling of `restarting` and added for the same reason: while containers
   * are coming down the polled verdict flaps to `soap_unreachable`, and Home
   * rendered that as "World is running, but the launcher can't reach it" --
   * an alarming sentence about a server the user just asked to stop. Reported
   * live, 2026-08-03.
   */
  stopping: false,
  apply: "none" as ApplyNeeded,
});

/// Record what a save asked for. Escalates, never downgrades: across a
/// multi-row save the strongest answer wins regardless of row order.
export function noteApplyNeeded(apply: ApplyNeeded): void {
  if (apply === "none") return;
  restartState.apply = escalate(restartState.apply, apply);
  restartState.needed = true;
}

/// Everything pending has been applied. Clearing `needed` without clearing
/// `apply` would leave a stale strength to escalate against on the next save.
export function clearApplyNeeded(): void {
  restartState.needed = false;
  restartState.apply = "none";
}
