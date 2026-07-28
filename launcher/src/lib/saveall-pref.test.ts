// @vitest-environment jsdom
import { describe, expect, it } from "vitest";

// The 2026-07-21 incident in one line: the "save characters before restart"
// box was left unticked, that choice was persisted under this key, and it
// silently survived an app restart -- so a much later restart skipped the save
// with nobody remembering they had armed it. Seed exactly that residue BEFORE
// the module loads (a static import would be hoisted above this line and never
// see it), then assert the module comes up SAFE anyway.
const LEGACY_KEY = "dml.saveBeforeRestart";
localStorage.setItem(LEGACY_KEY, "0");

const { saveallPref, setSaveBeforeRestart, wouldSkipSaveall, saveallBoxChecked } = await import(
  "./saveall-pref.svelte"
);

// Captured at load so the mutating tests below cannot make these vacuous.
const initialSave = saveallPref.saveBeforeRestart;
const legacyAfterLoad = localStorage.getItem(LEGACY_KEY);

describe("saveall preference is session-only", () => {
  it("starts every app session saving, even with the incident's persisted 0 in storage", () => {
    expect(initialSave).toBe(true);
  });

  it("sweeps the legacy persisted key so an old 0 cannot be resurrected", () => {
    expect(legacyAfterLoad).toBe(null);
  });

  it("still lets the user turn saving off for the current session", () => {
    setSaveBeforeRestart(false);
    expect(saveallPref.saveBeforeRestart).toBe(false);
    setSaveBeforeRestart(true);
    expect(saveallPref.saveBeforeRestart).toBe(true);
  });

  it("writes nothing to storage when the box is unticked", () => {
    setSaveBeforeRestart(false);
    expect(localStorage.getItem(LEGACY_KEY)).toBe(null);
    // Nothing else in this file touches storage, so an empty store is proof
    // the preference is not persisted under some other name either.
    expect(localStorage.length).toBe(0);
    setSaveBeforeRestart(true);
  });
});

// Pure truth tables -- no storage/DOM -- same shape as features.test.ts.
describe("wouldSkipSaveall", () => {
  it("skips only when the user turned saving off and the feature is unlocked", () => {
    expect(wouldSkipSaveall(false, false)).toBe(true);
  });

  it("never skips while [skip-saveall] is locked, whatever the box says", () => {
    expect(wouldSkipSaveall(false, true)).toBe(false);
    expect(wouldSkipSaveall(true, true)).toBe(false);
  });

  it("never skips while saving is on", () => {
    expect(wouldSkipSaveall(true, false)).toBe(false);
  });
});

describe("saveallBoxChecked", () => {
  it("shows ticked while locked even though the user unticked it", () => {
    expect(saveallBoxChecked(false, true)).toBe(true);
  });

  it("tracks the user's choice while unlocked", () => {
    expect(saveallBoxChecked(true, false)).toBe(true);
    expect(saveallBoxChecked(false, false)).toBe(false);
  });

  it("is the exact complement of wouldSkipSaveall in every combination", () => {
    // The display invariant: an unticked box means a skip WILL happen (so the
    // faster-mode warning is honest), and a ticked box means it will not.
    for (const save of [true, false]) {
      for (const locked of [true, false]) {
        expect(saveallBoxChecked(save, locked)).toBe(!wouldSkipSaveall(save, locked));
      }
    }
  });
});
