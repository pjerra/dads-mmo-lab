import { describe, expect, it } from "vitest";
import { lockedFor } from "./features.svelte";

// Pure truth table -- lockedFor takes no dependency on localStorage/DOM, so
// this runs in vitest's default node environment (no @vitest-environment
// pragma needed, unlike tooltip.test.ts).
describe("lockedFor", () => {
  it("locks an untested feature when testing mode is off", () => {
    expect(lockedFor("untested", false)).toBe(true);
  });

  it("unlocks an untested feature when testing mode is on", () => {
    expect(lockedFor("untested", true)).toBe(false);
  });

  it("never locks a tested feature, testing mode off or on", () => {
    expect(lockedFor("tested", false)).toBe(false);
    expect(lockedFor("tested", true)).toBe(false);
  });

  it("fails open for an unregistered key, testing mode off or on", () => {
    expect(lockedFor(undefined, false)).toBe(false);
    expect(lockedFor(undefined, true)).toBe(false);
  });
});
