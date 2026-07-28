import { describe, it, expect } from "vitest";
import { ALE_KEY, aleInstallOffer } from "./ale-gate";
import type { CppModule } from "./api";

// The Lua scripts tab used to hide its whole list when ALE wasn't installed,
// which also hid the reason to install ALE at all. It now shows the rows
// disabled with a one-click Install ALE -- and that button must only appear
// when it would actually install something.

const cpp = (key: string, installed: boolean): CppModule =>
  ({ key, name: key === ALE_KEY ? "Eluna (ALE)" : key, installed }) as CppModule;

describe("aleInstallOffer", () => {
  it("offers the ALE module when it isn't installed yet", () => {
    const offer = aleInstallOffer([cpp("mod-transmog", true), cpp(ALE_KEY, false)], false);
    expect(offer).toEqual({ key: ALE_KEY, name: "Eluna (ALE)" });
  });

  it("offers nothing once ALE is ready", () => {
    expect(aleInstallOffer([cpp(ALE_KEY, true)], true)).toBeNull();
  });

  it("offers nothing when the catalog has no ALE row", () => {
    // Never render a button that would install nothing.
    expect(aleInstallOffer([cpp("mod-transmog", true)], false)).toBeNull();
  });

  it("offers nothing when the row claims installed but ALE isn't ready", () => {
    // Inconsistent state (clone present, ale_ready false): re-installing is
    // not the fix, so don't pretend one click solves it.
    expect(aleInstallOffer([cpp(ALE_KEY, true)], false)).toBeNull();
  });

  it("offers nothing for an empty catalog", () => {
    expect(aleInstallOffer([], false)).toBeNull();
  });
});
