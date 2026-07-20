import { describe, expect, it } from "vitest";
import { NAV, DEFAULT_PAGE, CONFIG_VIEWS, BOTS_VIEWS } from "./nav";

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
      "party",
      "browse",
      "commands",
      "settings",
      "botworld",
      "ahbot",
      "accountwide",
      "moduletuning",
      "files",
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

  it("config section promotes the config views + Backups to sidebar children", () => {
    const cfg = NAV.find((s) => s.section === "Config")!;
    expect(cfg.pages.map((p) => p.id)).toEqual([
      "settings",
      "botworld",
      "ahbot",
      "accountwide",
      "moduletuning",
      "files",
      "backups",
    ]);
  });

  it("items & bots section has the two bot views as children", () => {
    const ib = NAV.find((s) => s.section === "Items & Bots")!;
    expect(ib.pages.map((p) => p.id)).toEqual(["items", "party", "browse", "commands"]);
  });

  it("CONFIG_VIEWS + BOTS_VIEWS are all real nav page ids", () => {
    for (const v of [...CONFIG_VIEWS, ...BOTS_VIEWS]) {
      expect(ids).toContain(v);
    }
    // Config views map 1:1 onto the Config section's non-Backups children.
    const cfg = NAV.find((s) => s.section === "Config")!;
    expect(cfg.pages.filter((p) => p.id !== "backups").map((p) => p.id)).toEqual([
      ...CONFIG_VIEWS,
    ]);
  });
});
