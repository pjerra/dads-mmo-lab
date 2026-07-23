import { describe, expect, it } from "vitest";
import { updateChip, versionLabel } from "./module-updates.svelte";

// Version line from the list arm's additive head/head_date fields: absent
// (older CLI) and null (not installed / no .git) must both degrade quietly.
describe("versionLabel", () => {
  it("joins sha and date with a middot", () => {
    expect(versionLabel("abc1234", "2026-05-01")).toBe("abc1234 · 2026-05-01");
  });

  it("degrades to the bare sha when the date is missing", () => {
    expect(versionLabel("abc1234", null)).toBe("abc1234");
    expect(versionLabel("abc1234", undefined)).toBe("abc1234");
  });

  it("is empty without a sha, whatever the date says (null, undefined, older-CLI absent)", () => {
    expect(versionLabel(null, null)).toBe("");
    expect(versionLabel(undefined, undefined)).toBe("");
    expect(versionLabel(null, "2026-05-01")).toBe("");
    expect(versionLabel("", "2026-05-01")).toBe("");
  });
});

// Update chip: only a KNOWN behind-count > 0 shows a chip -- null is "fetch
// failed / unknown", 0 is "up to date", both render nothing.
describe("updateChip", () => {
  it("is null when behind is unknown (null / older-CLI absent)", () => {
    expect(updateChip(null)).toBe(null);
    expect(updateChip(undefined)).toBe(null);
  });

  it("is null when up to date (behind 0)", () => {
    expect(updateChip(0)).toBe(null);
  });

  it("uses the singular for exactly one commit behind", () => {
    expect(updateChip(1)).toBe("Update available — 1 commit behind");
  });

  it("uses the plural for several commits behind", () => {
    expect(updateChip(2)).toBe("Update available — 2 commits behind");
    expect(updateChip(374)).toBe("Update available — 374 commits behind");
  });
});
