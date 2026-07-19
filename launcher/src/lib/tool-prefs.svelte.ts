// Persisted Tools-page preference toggles (Batch 2 F6): keep-awake and LAN
// auto-refresh. Both default ON -- each is additionally gated by its (locked)
// feature flag, so nothing engages until the flag is unlocked; the toggle
// exists so a user can opt out once the feature is live. Module-level runes
// state (restart-state pattern) so the transition logic in
// server-status.svelte.ts reads the same values the Tools page edits.

const KEEP_AWAKE_KEY = "dml.keepAwake";
const LAN_AUTO_KEY = "dml.lanAutoRefresh";

// Guarded storage access, same idiom as features.svelte.ts: missing storage
// (node-environment tests) or a throwing storage (privacy mode) falls back
// to the default.
function readStored(key: string, fallback: boolean): boolean {
  try {
    if (typeof localStorage === "undefined") return fallback;
    const v = localStorage.getItem(key);
    return v === null ? fallback : v === "1";
  } catch {
    return fallback;
  }
}

function writeStored(key: string, on: boolean): void {
  try {
    if (typeof localStorage !== "undefined") localStorage.setItem(key, on ? "1" : "0");
  } catch {
    // In-memory state still applies for this session.
  }
}

export const toolPrefs = $state({
  keepAwake: readStored(KEEP_AWAKE_KEY, true),
  lanAutoRefresh: readStored(LAN_AUTO_KEY, true),
});

export function setKeepAwakePref(on: boolean): void {
  toolPrefs.keepAwake = on;
  writeStored(KEEP_AWAKE_KEY, on);
}

export function setLanAutoRefreshPref(on: boolean): void {
  toolPrefs.lanAutoRefresh = on;
  writeStored(LAN_AUTO_KEY, on);
}
