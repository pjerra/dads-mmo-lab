import { describe, expect, it } from "vitest";
import {
  ROLES,
  ROLE_MAP,
  PVE_SPECS_BY_CLASS_ID,
  SPEC_ALLOWLIST,
  VALID_BOT_CLASSES,
} from "./party-specs";

describe("party-specs map integrity", () => {
  it("every role in ROLE_MAP is a listed role and vice versa", () => {
    expect(Object.keys(ROLE_MAP).sort()).toEqual([...ROLES].sort());
  });

  it("every role pick uses a valid CLI class name and an allowlisted spec", () => {
    for (const role of ROLES) {
      for (const pick of ROLE_MAP[role]) {
        expect(VALID_BOT_CLASSES).toContain(pick.class);
        expect(SPEC_ALLOWLIST).toContain(pick.spec);
        expect(pick.spec.endsWith(" pve")).toBe(true); // MVP: pve only
      }
    }
  });

  it("every Change-spec option is allowlisted and pve-only", () => {
    for (const specs of Object.values(PVE_SPECS_BY_CLASS_ID)) {
      for (const s of specs) {
        expect(SPEC_ALLOWLIST).toContain(s);
        expect(s.endsWith(" pve")).toBe(true);
      }
    }
  });

  it("no DK (class 6) anywhere", () => {
    expect(Object.keys(PVE_SPECS_BY_CLASS_ID)).not.toContain("6");
    for (const role of ROLES) {
      for (const pick of ROLE_MAP[role]) expect(pick.classId).not.toBe(6);
    }
  });

  it("allowlist has no duplicates and never invents bear pvp / frostfire pvp", () => {
    expect(new Set(SPEC_ALLOWLIST).size).toBe(SPEC_ALLOWLIST.length);
    expect(SPEC_ALLOWLIST).not.toContain("bear pvp");
    expect(SPEC_ALLOWLIST).not.toContain("frostfire pvp");
    // spec-name shape matches the CLI's injection-safe charset ([a-z ] only)
    for (const s of SPEC_ALLOWLIST) expect(s).toMatch(/^[a-z ]+$/);
  });

  it("classId matches the CLI class name in every pick", () => {
    const idByName: Record<string, number> = {
      warrior: 1, paladin: 2, hunter: 3, rogue: 4, priest: 5,
      shaman: 7, mage: 8, warlock: 9, druid: 11,
    };
    for (const role of ROLES) {
      for (const pick of ROLE_MAP[role]) {
        expect(pick.classId).toBe(idByName[pick.class]);
      }
    }
  });
});
