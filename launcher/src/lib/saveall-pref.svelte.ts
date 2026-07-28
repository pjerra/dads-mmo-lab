// "Save characters before restart" preference — DELIBERATELY NOT PERSISTED.
//
// The 2026-07-21 incident was this exact box left unticked: the choice was
// stored under LEGACY_KEY, silently survived an app restart, and a much later
// restart skipped the save with nobody remembering they had armed it. A
// dangerous setting must not outlive the session that armed it, so the safe
// value (saving ON) is re-asserted on every app start and the toggle only
// lasts this session. LEGACY_KEY is swept on load so an old "0" left in a
// user's storage cannot come back to life.
//
// Module-level runes state (restart-state.svelte.ts pattern) so the choice
// survives sidebar navigation — "this app session", not "this mount of Home".
const LEGACY_KEY = "dml.saveBeforeRestart";

// Guarded storage access, same idiom as features.svelte.ts: localStorage may be
// absent (node-environment tests) or throw (privacy mode). Nothing to sweep in
// either case, and the in-memory default below is already the safe one.
try {
  if (typeof localStorage !== "undefined") localStorage.removeItem(LEGACY_KEY);
} catch {
  /* unreachable storage cannot be holding a stale value we could act on */
}

export const saveallPref = $state({ saveBeforeRestart: true });

export function setSaveBeforeRestart(on: boolean): void {
  saveallPref.saveBeforeRestart = on;
}

// Pure decision helpers — no storage/DOM — same shape as lockedFor in
// features.svelte.ts.

/** Effective skip: only when [skip-saveall] is unlocked AND saving was turned off. */
export function wouldSkipSaveall(saveBeforeRestart: boolean, locked: boolean): boolean {
  return !locked && !saveBeforeRestart;
}

/**
 * The box shows ticked whenever a save WILL happen — including while the
 * feature is locked and the skip is therefore ignored — so the display never
 * claims a skip that wouldSkipSaveall will not perform, and an unticked box is
 * always an honest "characters are not being saved".
 */
export function saveallBoxChecked(saveBeforeRestart: boolean, locked: boolean): boolean {
  return saveBeforeRestart || locked;
}
