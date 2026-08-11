import { describe, expect, it } from "vitest";
import { SETUP_CATALOG, setupDoneKey, setupFor, type SetupAction } from "./setup-catalog";

// Needs-setup notices (Modules-page round, Task 5): a small hand-maintained
// catalog naming installed modules that need a manual step or two before
// they do anything, plus the guided actions (open-tuner/open-files/
// place-npc/fixit/copy-command) that wrap machinery already on this page.
const VALID_ACTION_TYPES: ReadonlySet<SetupAction["type"]> = new Set([
  "open-tuner",
  "open-files",
  "place-npc",
  "fixit",
  "copy-command",
]);

describe("setup-catalog", () => {
  it("every catalog entry has at least one step", () => {
    for (const [key, setup] of Object.entries(SETUP_CATALOG)) {
      expect(setup.steps.length, `${key} has no steps`).toBeGreaterThan(0);
    }
  });

  it("every action across the catalog is one of the five known machinery types", () => {
    for (const [key, setup] of Object.entries(SETUP_CATALOG)) {
      for (const action of setup.actions) {
        expect(VALID_ACTION_TYPES.has(action.type), `${key} has unknown action type ${action.type}`).toBe(true);
      }
    }
  });

  it("setupFor returns null for an unknown key", () => {
    expect(setupFor("mod-does-not-exist")).toBeNull();
  });

  it("setupFor returns the catalog entry for a known key", () => {
    expect(setupFor("mod-ahbot")).toBe(SETUP_CATALOG["mod-ahbot"]);
  });

  it("setupDoneKey incorporates both server dir and module key -- two servers don't share dismissals", () => {
    const a = setupDoneKey("C:/servers/one", "mod-ahbot");
    const b = setupDoneKey("C:/servers/two", "mod-ahbot");
    expect(a).not.toBe(b);
    const c = setupDoneKey("C:/servers/one", "bmah");
    expect(a).not.toBe(c);
  });
});
