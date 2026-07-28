import { describe, expect, it } from "vitest";
import {
  ROLES,
  ROLE_MAP,
  PVE_SPECS_BY_CLASS_ID,
  SPEC_ALLOWLIST,
  VALID_BOT_CLASSES,
  buildSpecIndex,
  isValidSpecShape,
  type LiveSpec,
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
    // The shipped names are plain lowercase-and-spaces; that is a strict
    // subset of the CLI's (wider) injection-safe charset.
    for (const s of SPEC_ALLOWLIST) {
      expect(s).toMatch(/^[a-z ]+$/);
      expect(isValidSpecShape(s)).toBe(true);
    }
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

// The charset guard mirrored from the CLI (_valid_bot_spec / valid_bot_spec_shape).
describe("isValidSpecShape", () => {
  it("accepts what a hand-written playerbots.conf realistically carries", () => {
    for (const s of ["frost pve", "Frost PvE", "frost-pve", "frost_pve", "Arctic-PvE 2.0", "spec1"]) {
      expect(isValidSpecShape(s)).toBe(true);
    }
  });

  it("rejects anything unsafe in the whisper tail", () => {
    for (const s of [
      "",
      " frost",
      "-frost",
      "frost'pve",
      'frost"pve',
      "frost\\pve",
      "frost\npve",
      "frost\rpve",
      "frost pve; .server shutdown",
      "$(id)",
      "frost<pve>",
      "frost&pve",
    ]) {
      expect(isValidSpecShape(s)).toBe(false);
    }
  });
});

// Batch 5 F5 follow-up: the live picker index built from `wow party specs`.
describe("buildSpecIndex (live spec picker)", () => {
  const live: LiveSpec[] = [
    { class_id: 1, class: "warrior", specno: 0, name: "arms pve", link: "3022-305-2033", tree: "55/8/8" },
    { class_id: 1, class: "warrior", specno: 1, name: "fury pve", link: "-305-2033", tree: "0/8/8" },
    { class_id: 8, class: "mage", specno: 2, name: "frost pve", link: "23000503110003", tree: "18/0/53" },
    { class_id: 8, class: "mage", specno: 9, name: "custom test pve", link: null, tree: null },
  ];

  it("groups specs by class name AND by class id", () => {
    const idx = buildSpecIndex(live);
    expect(idx.byName["warrior"].map((s) => s.name)).toEqual(["arms pve", "fury pve"]);
    expect(idx.byName["mage"].map((s) => s.name)).toEqual(["frost pve", "custom test pve"]);
    expect(idx.byId[1].map((s) => s.name)).toEqual(["arms pve", "fury pve"]);
    expect(idx.byId[8].map((s) => s.name)).toEqual(["frost pve", "custom test pve"]);
  });

  it("keeps the full spec so the preview can read tree/link", () => {
    const idx = buildSpecIndex(live);
    const arms = idx.byName["warrior"].find((s) => s.name === "arms pve")!;
    expect(arms.tree).toBe("55/8/8");
    expect(arms.link).toBe("3022-305-2033");
    // a name the conf defined without a link stays null (plain build summary)
    const custom = idx.byId[8].find((s) => s.name === "custom test pve")!;
    expect(custom.tree).toBeNull();
    expect(custom.link).toBeNull();
  });

  it("surfaces specs the static mirror never had (drift-proof)", () => {
    const idx = buildSpecIndex(live);
    // 'custom test pve' is not in SPEC_ALLOWLIST, yet the live index offers it.
    expect(SPEC_ALLOWLIST).not.toContain("custom test pve");
    expect(idx.byId[8].some((s) => s.name === "custom test pve")).toBe(true);
  });

  it("never offers a spec name the CLI validator would reject", () => {
    // playerbots.conf is raw-writable from the Modules editor, so a conf value
    // can carry anything. Mixed case / punctuation is legal and must survive
    // (the picker offering it and the CLI refusing it was the drift); a name
    // that is whisper-unsafe is dropped instead of becoming a dead option.
    const idx = buildSpecIndex([
      { class_id: 1, class: "warrior", specno: 5, name: "Arctic-PvE 2.0", link: null, tree: null },
      { class_id: 1, class: "warrior", specno: 6, name: "frost pve; .server shutdown", link: null, tree: null },
      { class_id: 1, class: "warrior", specno: 7, name: 'frost"pve', link: null, tree: null },
    ]);
    expect(idx.byName["warrior"].map((s) => s.name)).toEqual(["Arctic-PvE 2.0"]);
    expect(idx.byId[1].map((s) => s.name)).toEqual(["Arctic-PvE 2.0"]);
  });

  it("empty live data yields empty indexes (UI then uses the static fallback)", () => {
    const idx = buildSpecIndex([]);
    expect(idx.byName).toEqual({});
    expect(idx.byId).toEqual({});
  });
});
