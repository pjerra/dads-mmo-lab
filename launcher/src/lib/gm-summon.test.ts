import { describe, expect, it } from "vitest";
import { MODULE_NPCS, summonModuleHint } from "./gm-summon";

describe("summonModuleHint", () => {
  const CASINO = 990000; // a module NPC that DOES exist on the live server

  it("returns the install-the-module hint when the creature row is missing", () => {
    const hint = summonModuleHint(190010, {
      code: "NOT_FOUND",
      message: "No creature with entry 190010",
    });
    expect(hint).toContain("Transmogrifier");
    expect(hint).toContain("mod-transmog");
    expect(hint).toContain("entry 190010");
  });

  it("does NOT claim the module is missing for an offline-character NOT_FOUND", () => {
    // Regression: the CLI's online guard also returns NOT_FOUND (different
    // message). A summon that fails only because the char logged out must
    // fall through (null) to the plain "log in first" error, not tell the
    // user to reinstall a module they already have.
    expect(
      summonModuleHint(CASINO, {
        code: "NOT_FOUND",
        message: "Character not online: Bob",
      }),
    ).toBeNull();
  });

  it("falls through for a NOT_FOUND with no recognizable message", () => {
    expect(summonModuleHint(CASINO, { code: "NOT_FOUND" })).toBeNull();
    expect(summonModuleHint(CASINO, { code: "NOT_FOUND", message: "" })).toBeNull();
  });

  it("falls through for a non-module entry even when the creature is missing", () => {
    // e.g. a stock Auctioneer or a custom entry -- no module to point at.
    expect(
      summonModuleHint(8661, { code: "NOT_FOUND", message: "No creature with entry 8661" }),
    ).toBeNull();
  });

  it("falls through for non-NOT_FOUND errors (DB, SOAP, ...)", () => {
    expect(
      summonModuleHint(CASINO, { code: "DB_UNREACHABLE", message: "No creature with entry 990000" }),
    ).toBeNull();
    expect(summonModuleHint(CASINO, {})).toBeNull();
  });

  it("covers every catalog module NPC", () => {
    for (const key of Object.keys(MODULE_NPCS)) {
      const entry = Number(key);
      const hint = summonModuleHint(entry, {
        code: "NOT_FOUND",
        message: `No creature with entry ${entry}`,
      });
      expect(hint).toContain(MODULE_NPCS[entry].npc);
      expect(hint).toContain(MODULE_NPCS[entry].module);
    }
  });
});
