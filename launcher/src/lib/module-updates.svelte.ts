// Module-level runes state for per-module update checks (module-update
// round). Same pattern as server-status.svelte.ts: a flat $state object at
// module scope so ModuleManager keeps its last-known check results across
// remounts, plus pure helpers (versionLabel/updateChip) exported for tests
// and shared by every card that renders version/update info.
//
// The check itself fetches origin per installed clone CLI-side (30s+ with
// many modules), so results are cached here rather than re-fetched on every
// page visit -- the user re-runs it explicitly via the Check button.

import { wowModuleUpdateCheck, type ModuleCheckRepo } from "./api";

export const moduleUpdates = $state({
  // Last update-check result, keyed by module label (= module key). Replaced
  // wholesale on every successful check -- a module removed since the last
  // check must not keep a stale entry.
  repos: {} as Record<string, ModuleCheckRepo>,
  // True while a check is in flight (single-flight, drives the button spinner).
  checking: false,
  // True once any check has completed -- distinguishes "never checked" (show
  // nothing) from "checked, zero git modules" (empty repos is a valid answer).
  checked: false,
  lastError: null as string | null,
});

// Single-flight: concurrent callers (button double-click, page remount) are
// no-ops while a check is already running.
export async function runUpdateCheck(): Promise<void> {
  if (moduleUpdates.checking) return;
  moduleUpdates.checking = true;
  try {
    const res = await wowModuleUpdateCheck();
    const byLabel: Record<string, ModuleCheckRepo> = {};
    for (const r of res.repos) byLabel[r.label] = r;
    moduleUpdates.repos = byLabel;
    moduleUpdates.checked = true;
    moduleUpdates.lastError = null;
  } catch (e) {
    // A failed check keeps the last-known results -- only the error surfaces.
    const err = e as { message?: string; hint?: string };
    moduleUpdates.lastError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
  } finally {
    moduleUpdates.checking = false;
  }
}

// Pure: the "abc1234 · 2026-05-01" version line for a module card, from the
// list arm's additive head/head_date fields. Null-safe on both sides -- an
// older deployed CLI omits the fields entirely, a not-installed/no-.git
// module reports them null -- so absence degrades to "" (card shows nothing)
// and a missing date degrades to the bare sha.
export function versionLabel(
  head: string | null | undefined,
  headDate: string | null | undefined,
): string {
  if (!head) return "";
  if (!headDate) return head;
  return `${head} · ${headDate}`;
}

// Pure: the update chip text for a checked module. Null (no chip) when
// behind is unknown (null: fetch failed / never checked) or 0 (up to date);
// otherwise "Update available — N commit(s) behind" with correct plural.
export function updateChip(behind: number | null | undefined): string | null {
  if (behind === null || behind === undefined || behind === 0) return null;
  return `Update available — ${behind} ${behind === 1 ? "commit" : "commits"} behind`;
}
