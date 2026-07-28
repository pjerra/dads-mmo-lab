import { describe, it, expect } from "vitest";
import { FEATURES, lockedFor } from "./features.svelte";

// The feature lock fails OPEN for an unregistered key -- lockedFor(undefined,
// false) is false by design (features.test.ts pins that), because an unknown
// key means "registry bug", not "silently disable this control". The cost of
// that choice is that a typo or a one-sided rename does not throw, does not
// warn, and does not fail a single test: it just UNLOCKS the control for every
// user who never ticked "Enable untested features". For a key like
// "docker-restart" -- one click that kills every running container without the
// world's shutdown save -- that is the worst possible failure mode.
//
// So this suite closes the loop the registry cannot close itself: every key
// the UI actually passes to featureLocked() must exist in FEATURES. Sources
// come in via import.meta.glob(?raw) rather than node:fs (the app has no
// @types/node), same technique as taskbar-pairing.test.ts.
const SOURCES = import.meta.glob(
  ["./**/*.svelte", "./**/*.ts", "../routes/**/*.svelte", "../routes/**/*.ts"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

// Test files are excluded on purpose: a test may legitimately feed a made-up
// key to prove the fail-open behaviour, and that is not a shipped call site.
const CALL_SITES = Object.entries(SOURCES).filter(([file]) => !file.endsWith(".test.ts"));

/** Every `featureLocked("literal")` key in the app, with where it was found. */
const usedKeys = CALL_SITES.flatMap(([file, source]) =>
  [...source.matchAll(/featureLocked\(\s*"([^"]+)"\s*\)/g)].map((m) => ({ file, key: m[1] })),
);

// The one this suite exists for (findings G20): destructive, feature-locked,
// and reachable from a single button on Tools.
const DESTRUCTIVE_KEY = "docker-restart";

describe("feature keys used by the UI", () => {
  it("finds the call sites to check", () => {
    // Non-vacuity. If the glob, the raw query or the call shape ever changes,
    // this fires instead of every assertion below passing over an empty list.
    expect(usedKeys.length).toBeGreaterThanOrEqual(50);
    expect(new Set(usedKeys.map((u) => u.file)).size).toBeGreaterThanOrEqual(5);
  });

  it("every key the UI passes to featureLocked() is registered in FEATURES", () => {
    // An unregistered key is not a locked control -- it is an UNLOCKED one.
    const unregistered = usedKeys
      .filter((u) => FEATURES[u.key] === undefined)
      .map((u) => `${u.file}: featureLocked("${u.key}")`);
    expect(unregistered).toEqual([]);
  });

  it("still covers the destructive docker-restart button specifically", () => {
    // Per-key non-vacuity: the floor above is dominated by ~60 harmless keys,
    // so a rename of THIS one on the UI side (the exact drift that would ship
    // a one-click daemon restart unlocked) has to be visible on its own.
    expect(usedKeys.map((u) => u.key)).toContain(DESTRUCTIVE_KEY);
    expect(Object.keys(FEATURES)).toContain(DESTRUCTIVE_KEY);
  });

  it("keeps the destructive key on the locked side of the fail-open rule", () => {
    // Registered is not enough: the whole point of the registry entry is that
    // this control stays locked with testing mode OFF until its smoke row is
    // reported as passing. Flipping it to "tested" without that is exactly the
    // change this assertion is here to surface.
    expect(lockedFor(FEATURES[DESTRUCTIVE_KEY], false)).toBe(true);
    expect(lockedFor(FEATURES[DESTRUCTIVE_KEY], true)).toBe(false);
  });
});
