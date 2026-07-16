import { describe, expect, it } from "vitest";
import { qualityName, QUALITY_COLORS } from "./wow";

describe("qualityName", () => {
  it("maps the WoW quality tiers", () => {
    expect(qualityName(0)).toBe("Poor");
    expect(qualityName(1)).toBe("Common");
    expect(qualityName(2)).toBe("Uncommon");
    expect(qualityName(3)).toBe("Rare");
    expect(qualityName(4)).toBe("Epic");
    expect(qualityName(5)).toBe("Legendary");
  });
  it("falls back for unknown tiers and has a color per tier", () => {
    expect(qualityName(9)).toBe("Unknown");
    for (let q = 0; q <= 5; q++) expect(QUALITY_COLORS[q]).toMatch(/^#/);
  });
  it("includes quality tiers 6 and 7 (Artifact, Heirloom)", () => {
    expect(qualityName(6)).toBe("Artifact");
    expect(qualityName(7)).toBe("Heirloom");
    expect(QUALITY_COLORS[7]).toMatch(/^#/);
  });
});
