import { describe, expect, it } from "vitest";
import { defaultExpanded, luaStatus, luaStatusLabel, splitInstalled } from "./module-split";

describe("splitInstalled", () => {
  it("preserves order within halves and loses no rows", () => {
    const rows = [
      { key: "a", installed: true },
      { key: "b", installed: false },
      { key: "c", installed: true },
      { key: "d", installed: false },
      { key: "e", installed: true },
    ];
    const { installed, available } = splitInstalled(rows, (m) => m.installed);
    expect(installed.map((m) => m.key)).toEqual(["a", "c", "e"]);
    expect(available.map((m) => m.key)).toEqual(["b", "d"]);
    // No row lost or duplicated.
    expect(installed.length + available.length).toBe(rows.length);
  });

  it("handles all-installed and all-available extremes", () => {
    const allIn = [{ k: 1 }, { k: 2 }];
    expect(splitInstalled(allIn, () => true).available).toEqual([]);
    expect(splitInstalled(allIn, () => false).installed).toEqual([]);
  });

  it("handles an empty list", () => {
    expect(splitInstalled([], () => true)).toEqual({ installed: [], available: [] });
  });
});

describe("luaStatus", () => {
  it("deployed implies installed regardless of cloned", () => {
    expect(luaStatus({ cloned: true, deployed: true })).toBe("installed");
    expect(luaStatus({ cloned: false, deployed: true })).toBe("installed");
  });
  it("cloned-only is cloned", () => {
    expect(luaStatus({ cloned: true, deployed: false })).toBe("cloned");
  });
  it("neither is absent", () => {
    expect(luaStatus({ cloned: false, deployed: false })).toBe("absent");
  });
});

describe("luaStatusLabel", () => {
  it("maps every status to its label", () => {
    expect(luaStatusLabel("installed")).toBe("Installed");
    expect(luaStatusLabel("cloned")).toBe("Cloned, not deployed");
    expect(luaStatusLabel("absent")).toBe("Not installed");
  });
});

describe("defaultExpanded", () => {
  it("is true iff installedCount is 0", () => {
    expect(defaultExpanded(0)).toBe(true);
    expect(defaultExpanded(1)).toBe(false);
    expect(defaultExpanded(5)).toBe(false);
  });
});
