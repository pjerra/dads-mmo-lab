import { describe, expect, it } from "vitest";
import { buildModuleRow, type RowCtx } from "./module-row";

// Row-model builder (Modules-page round 2, Task 1): every section of the
// Modules page renders through ONE row skeleton fed by buildModuleRow.
// Tasks 2-4 extend chips/actions through THIS builder -- never by ad-hoc
// markup -- so these tests pin the base shape they build on.

const cpp = (over: Partial<{ key: string; name: string; installed: boolean }> = {}) => ({
  key: "mod-transmog",
  name: "Transmog",
  installed: true,
  ...over,
});

const ctx = (over: Partial<RowCtx> = {}): RowCtx => ({ family: "cpp", ...over });

describe("buildModuleRow", () => {
  it("installed cpp module gets tune, repair, remove in that order", () => {
    const row = buildModuleRow(cpp(), ctx());
    expect(row.actions.map((a) => a.id)).toEqual(["tune", "repair", "remove"]);
    expect(row.installed).toBe(true);
    expect(row.family).toBe("cpp");
    expect(row.key).toBe("mod-transmog");
    expect(row.title).toBe("Transmog");
  });

  it("catalog (not-installed) entry gets install only", () => {
    const row = buildModuleRow(cpp({ installed: false }), ctx());
    expect(row.actions.map((a) => a.id)).toEqual(["install"]);
    expect(row.installed).toBe(false);
  });

  it("installed sql module has no tune (no conf) and no repair", () => {
    const row = buildModuleRow(
      { key: "rare-drops", name: "Rare drops", installed: true },
      ctx({ family: "sql" }),
    );
    expect(row.actions.map((a) => a.id)).toEqual(["remove"]);
  });

  it("chips are empty by default", () => {
    expect(buildModuleRow(cpp(), ctx()).chips).toEqual([]);
    expect(buildModuleRow(cpp({ installed: false }), ctx()).chips).toEqual([]);
  });

  it("installed (deployed) lua module gets tune + remove -- repair is cpp-only", () => {
    const row = buildModuleRow(
      { key: "bmah", name: "BMAH", cloned: true, deployed: true },
      ctx({ family: "lua" }),
    );
    expect(row.actions.map((a) => a.id)).toEqual(["tune", "remove"]);
    expect(row.installed).toBe(true);
  });

  it("cloned-not-deployed lua module keeps install (deploy) ahead of tune + remove", () => {
    const row = buildModuleRow(
      { key: "bmah", name: "BMAH", cloned: true, deployed: false },
      ctx({ family: "lua" }),
    );
    expect(row.actions.map((a) => a.id)).toEqual(["install", "tune", "remove"]);
    expect(row.installed).toBe(true);
  });

  it("catalog lua entry (neither cloned nor deployed) gets install only", () => {
    const row = buildModuleRow(
      { key: "paragon", name: "Paragon", cloned: false, deployed: false },
      ctx({ family: "lua" }),
    );
    expect(row.actions.map((a) => a.id)).toEqual(["install"]);
    expect(row.installed).toBe(false);
  });

  it("removeDisabled ctx marks the remove action disabled (rare-drops has no reversal)", () => {
    const row = buildModuleRow(
      { key: "rare-drops", name: "Rare drops", installed: true },
      ctx({ family: "sql", removeDisabled: true }),
    );
    const remove = row.actions.find((a) => a.id === "remove");
    expect(remove?.disabled).toBe(true);
    // Other rows' remove stays enabled.
    const plain = buildModuleRow(cpp(), ctx());
    expect(plain.actions.find((a) => a.id === "remove")?.disabled).toBeUndefined();
  });

  it("action labels are the visible button text", () => {
    const row = buildModuleRow(cpp(), ctx());
    expect(row.actions.map((a) => a.label)).toEqual(["Config tuning", "Repair…", "Remove"]);
    const cat = buildModuleRow(cpp({ installed: false }), ctx());
    expect(cat.actions[0].label).toBe("Install");
  });
});
