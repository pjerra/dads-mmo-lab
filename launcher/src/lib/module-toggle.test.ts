import { describe, expect, it } from "vitest";
import { moduleToggle, toggleIsOn, type TuningRowLike } from "./module-toggle";

// Per-module enable/disable toggle (Modules-page round 2, Task 4): a row on
// the Modules page only gets a toggle when the tuning registry declares a
// master switch for that module -- a row whose key ends in `.enable`/`.enabled`
// AND whose module matches. mod-playerbots is ALWAYS refused (disabling the
// server's own bot engine from a row toggle would be a footgun), and no match
// means no toggle -- the row simply doesn't render one. The write itself goes
// through the EXISTING tuner path (wowConfigTuningSet), so this module stays
// pure: eligibility + current-state reading only.

// Real registry shapes (crates/dml-wow/data/tuning-registry.json): conf rows
// carry the module's display name ("NPC Beastmaster") while the Modules page
// knows the cpp key ("mod-npc-beastmaster") and catalog title ("NPC
// Beastmaster (pets for all classes)").
const rows: TuningRowLike[] = [
  { key: "beastmaster.enable", backend: "conf", module: "NPC Beastmaster", value: "1", default: "1" },
  { key: "beastmaster.hunter_only", backend: "conf", module: "NPC Beastmaster", value: "1", default: "1" },
  { key: "learnspells.enable", backend: "conf", module: "Learn Spells on Level-up", value: "0", default: "1" },
  { key: "unlimitedammo.enabled", backend: "lua", module: "Unlimited Ammo", value: "0", default: "0" },
  { key: "sitmeansrest.duration", backend: "lua", module: "Sit Means Rest", value: "20", default: "20" },
];

describe("moduleToggle eligibility", () => {
  it("matches a cpp module key against the row's module name (.enable suffix)", () => {
    expect(moduleToggle("mod-npc-beastmaster", rows)).toEqual({
      settingKey: "beastmaster.enable",
      backend: "conf",
    });
  });

  it("matches a cpp module key against the row's key prefix (mod-learn-spells -> learnspells.enable)", () => {
    expect(moduleToggle("mod-learn-spells", rows)).toEqual({
      settingKey: "learnspells.enable",
      backend: "conf",
    });
  });

  it("matches a catalog title with a parenthetical suffix (.enabled suffix variant, lua backend)", () => {
    expect(moduleToggle("Unlimited Ammo (auto-refills Hunter arrows/bullets)", rows)).toEqual({
      settingKey: "unlimitedammo.enabled",
      backend: "lua",
    });
  });

  it("matches the plain lua module key too", () => {
    expect(moduleToggle("unlimitedammo", rows)).toEqual({
      settingKey: "unlimitedammo.enabled",
      backend: "lua",
    });
  });

  it("no toggle when the module's rows have no master switch (Sit Means Rest has none)", () => {
    expect(moduleToggle("Sit Means Rest", rows)).toBeNull();
    expect(moduleToggle("sitmeanrest", rows)).toBeNull();
  });

  it("no toggle for a module the registry doesn't know", () => {
    expect(moduleToggle("mod-transmog", rows)).toBeNull();
  });

  it("a non-master `.enable`-less key never matches, even for a known module", () => {
    // Only the hunter_only row present: module matches but no master switch.
    const only = rows.filter((r) => r.key === "beastmaster.hunter_only");
    expect(moduleToggle("mod-npc-beastmaster", only)).toBeNull();
  });

  it("mod-playerbots is ALWAYS refused, even when a matching master row exists", () => {
    const withPb: TuningRowLike[] = [
      ...rows,
      { key: "playerbots.enable", backend: "conf", module: "Playerbots", value: "1", default: "1" },
    ];
    expect(moduleToggle("mod-playerbots", withPb)).toBeNull();
    // The refusal holds for the display-name form too.
    expect(moduleToggle("Playerbots", withPb)).toBeNull();
  });
});

describe("toggleIsOn", () => {
  it("reads the current value off the matched row (1 = on, 0 = off)", () => {
    expect(toggleIsOn({ settingKey: "beastmaster.enable", backend: "conf" }, rows)).toBe(true);
    expect(toggleIsOn({ settingKey: "learnspells.enable", backend: "conf" }, rows)).toBe(false);
    expect(toggleIsOn({ settingKey: "unlimitedammo.enabled", backend: "lua" }, rows)).toBe(false);
  });

  it("an empty live value falls back to the row's default", () => {
    const blank: TuningRowLike[] = [
      { key: "learnspells.enable", backend: "conf", module: "Learn Spells on Level-up", value: "", default: "1" },
    ];
    expect(toggleIsOn({ settingKey: "learnspells.enable", backend: "conf" }, blank)).toBe(true);
  });

  it("a missing row reads as off", () => {
    expect(toggleIsOn({ settingKey: "nosuch.enable", backend: "conf" }, rows)).toBe(false);
  });
});
