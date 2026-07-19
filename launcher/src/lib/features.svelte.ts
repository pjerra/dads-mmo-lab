// Feature-lock registry for Round K0 ("disable the untested features until we
// do the smoke tests for each"). Every mutating control tied to a bracketed
// key in docs/SMOKE-TESTS.md is locked until that row is reported as passing
// -- flip "Enable untested features" in Settings to exercise a locked control
// while actually running its smoke test.
//
// Shared reactive state lives in this .svelte.ts module (Svelte 5 runes) so
// every page sees the same testing-mode flag without prop-drilling, matching
// the restart-state.svelte.ts pattern already used in this codebase.

export type FeatureStatus = "tested" | "untested";

// One entry per bracketed key in docs/SMOKE-TESTS.md. Flip a key to "tested"
// ONLY when the user reports that row's smoke test passed -- do not flip
// speculatively.
export const FEATURES: Record<string, FeatureStatus> = {
  restart: "tested",
  "console-send": "tested",
  "title-install": "untested",
  "title-remove": "untested",
  "teleport-named": "tested",
  "teleport-coords": "tested",
  "gm-actions": "tested",
  "gm-summon": "tested",
  "gm-atlogin": "tested",
  "mail-item": "tested",
  "party-ops": "tested",
  "party-botcmd": "tested",
  "bot-level": "tested",
  "party-presets": "tested",
  "preset-io": "tested",
  "settings-save": "tested",
  "config-edit": "untested",
  "ale-reload": "untested",
  "modules-cpp": "untested",
  "modules-rebuild": "untested",
  "modules-conf": "untested",
  "modules-lua": "untested",
  "modules-sql": "untested",
  "client-path": "untested",
  "module-repair": "untested",
  "backup-create": "untested",
  "backup-restore": "untested",
  accounts: "untested",
  "account-delete": "untested",
  "bridge-setup": "tested",
  "docker-clean": "untested",
  "server-update": "untested",
  "lan-play": "untested",
  "unbound-addon": "untested",
  "rates-live": "untested",
  "bots-world": "untested",
  "config-reset": "untested",
  "bots-flush": "untested",
  "auto-shutdown": "untested",
  "keep-awake": "untested",
  "lan-auto-refresh": "untested",
  "realmlist-fix": "untested",
  "chip-start": "untested",
  "world-restart": "untested",
  "module-fixit": "untested",
  "ahbot-page": "untested",
};

export const LOCKED_HINT =
  "Untested — enable untested features in Settings to try it (see docs/SMOKE-TESTS.md)";

const STORAGE_KEY = "dml.testingMode";

// Guarded storage access: localStorage may be absent entirely (SSR/vitest's
// node environment -- this app runs ssr=false, but the module is also
// imported by node-environment tests) or throw (privacy mode). Either way we
// fail back to in-memory-only behavior rather than crashing the app.
function hasStorage(): boolean {
  try {
    return typeof localStorage !== "undefined";
  } catch {
    return false;
  }
}

function readStored(): boolean {
  try {
    return hasStorage() && localStorage.getItem(STORAGE_KEY) === "1";
  } catch {
    return false;
  }
}

function writeStored(on: boolean): void {
  try {
    if (hasStorage()) localStorage.setItem(STORAGE_KEY, on ? "1" : "0");
  } catch {
    // Storage unavailable/blocked -- the in-memory flag below still applies
    // for the rest of this session.
  }
}

// Module-level runes state (same pattern as restart-state.svelte.ts) so the
// testing-mode flag survives page navigation without prop-drilling.
const testingMode = $state({ on: readStored() });

export function testingModeOn(): boolean {
  return testingMode.on;
}

export function setTestingMode(on: boolean): void {
  testingMode.on = on;
  writeStored(on);
}

// Pure decision helper -- no localStorage/DOM access -- so it's unit
// testable in plain node without touching storage. An unregistered key fails
// OPEN (never locked): every real mutating key must be registered in
// FEATURES above, so an unknown key here means a bug in the registry, not a
// feature that should silently lock.
export function lockedFor(status: FeatureStatus | undefined, testingOn: boolean): boolean {
  if (status === undefined) return false;
  return status === "untested" && !testingOn;
}

export function featureLocked(key: string): boolean {
  return lockedFor(FEATURES[key], testingModeOn());
}
