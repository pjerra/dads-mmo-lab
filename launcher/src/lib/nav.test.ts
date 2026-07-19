import { describe, expect, it } from "vitest";
import { NAV, DEFAULT_PAGE } from "./nav";

describe("NAV", () => {
  const ids = NAV.flatMap((s) => s.pages.map((p) => p.id));

  it("has exactly the spec's pages, in order", () => {
    expect(ids).toEqual([
      "home",
      "library",
      "console",
      "tools",
      "accounts",
      "modmanager",
      "dashboard",
      "teleport",
      "gmtools",
      "items",
      "playerbots",
      "commands",
      "settings",
      "botworld",
      "ahbot",
      "modules",
      "backups",
      "help",
    ]);
  });

  it("page ids are unique", () => {
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("has exactly the spec's sections, in order", () => {
    expect(NAV.map((s) => s.section)).toEqual([
      "Server",
      "Characters",
      "Items & Bots",
      "Config",
      "Help",
    ]);
  });

  it("default page is home and exists in NAV", () => {
    expect(DEFAULT_PAGE).toBe("home");
    expect(ids).toContain(DEFAULT_PAGE);
  });

  it("every entry has a non-empty label", () => {
    for (const s of NAV) {
      for (const p of s.pages) {
        expect(p.label.length).toBeGreaterThan(0);
      }
    }
  });

  it("config section's modules entry is labeled Module Configs", () => {
    const cfg = NAV.find((s) => s.section === "Config")!;
    expect(cfg.pages.find((p) => p.id === "modules")!.label).toBe("Module Configs");
  });
});
