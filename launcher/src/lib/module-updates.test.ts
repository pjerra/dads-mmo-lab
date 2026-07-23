import { describe, expect, it } from "vitest";
import { checkBadge, updateChip, versionLabel } from "./module-updates.svelte";
import type { ModuleCheckRepo } from "./api";

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

// Check badge: null until a check has run (or for a module the check didn't
// cover), then the Server update card's per-repo language -- so an
// all-up-to-date check is visibly different from "never checked", and a
// failed per-repo fetch (behind null) from an up-to-date module.
describe("checkBadge", () => {
  const repo = (behind: number | null): ModuleCheckRepo => ({
    label: "mod-aoe-loot",
    url: "https://github.com/azerothcore/mod-aoe-loot",
    branch: "master",
    head: "abc1234",
    dirty: 0,
    behind,
  });

  it("is null before any check has run, whatever the cached repo says", () => {
    expect(checkBadge(false, undefined)).toBe(null);
    expect(checkBadge(false, repo(3))).toBe(null);
  });

  it("is null for a module the check did not cover (not installed / no .git)", () => {
    expect(checkBadge(true, undefined)).toBe(null);
  });

  it("shows a green 'up to date' at behind 0 -- checked is no longer silent", () => {
    expect(checkBadge(true, repo(0))).toEqual({ text: "up to date", cls: "on" });
  });

  it("shows a muted '? behind' when the per-repo fetch failed (behind null)", () => {
    expect(checkBadge(true, repo(null))).toEqual({ text: "? behind", cls: "off" });
  });

  it("carries the amber update chip text when behind > 0", () => {
    expect(checkBadge(true, repo(1))).toEqual({
      text: "Update available — 1 commit behind",
      cls: "warn",
    });
    expect(checkBadge(true, repo(2))).toEqual({
      text: "Update available — 2 commits behind",
      cls: "warn",
    });
  });
});
