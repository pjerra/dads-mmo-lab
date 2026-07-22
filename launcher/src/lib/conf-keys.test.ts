import { describe, expect, it } from "vitest";
import {
  confKeyHint,
  filterConfKeys,
  installedConfModules,
  stagedConfChanges,
} from "./conf-keys";

const keys = [
  { key: "Transmogrification.Enable", value: "1", default: "1", line: 87, help: "Enables transmog." },
  { key: "Transmogrification.SetCostModifier", value: "3.0", default: "3.0", line: 90, help: "" },
  { key: "BeastMaster.MinLevel", value: "10", default: null, line: 4 },
];

describe("filterConfKeys", () => {
  it("returns everything for an empty or whitespace query", () => {
    expect(filterConfKeys(keys, "")).toEqual(keys);
    expect(filterConfKeys(keys, "  ")).toEqual(keys);
  });

  it("matches case-insensitively on the key substring", () => {
    expect(filterConfKeys(keys, "cost").map((k) => k.key)).toEqual([
      "Transmogrification.SetCostModifier",
    ]);
    expect(filterConfKeys(keys, "TRANSMOG").length).toBe(2);
    expect(filterConfKeys(keys, "nope")).toEqual([]);
  });
});

describe("stagedConfChanges", () => {
  it("returns only edits that differ from the current value", () => {
    expect(stagedConfChanges(keys, {})).toEqual([]);
    expect(stagedConfChanges(keys, { "BeastMaster.MinLevel": "10" })).toEqual([]);
    expect(stagedConfChanges(keys, { "BeastMaster.MinLevel": "25" })).toEqual([
      { key: "BeastMaster.MinLevel", value: "25" },
    ]);
  });

  it("ignores edits for keys not present in the parsed list", () => {
    expect(stagedConfChanges(keys, { ghost: "1" })).toEqual([]);
  });

  it("keeps the user's edit order", () => {
    const edits: Record<string, string> = {};
    edits["BeastMaster.MinLevel"] = "25";
    edits["Transmogrification.Enable"] = "0";
    expect(stagedConfChanges(keys, edits).map((c) => c.key)).toEqual([
      "BeastMaster.MinLevel",
      "Transmogrification.Enable",
    ]);
  });
});

describe("installedConfModules", () => {
  const cpp = [
    { key: "mod-ah-bot", name: "Auction House Bot", desc: "AH bot.", installed: true, conf_name: "mod_ahbot.conf" },
    { key: "mod-transmog", name: "Transmogrification", desc: "Reskin gear.", installed: true, conf_name: "transmog.conf" },
    // installed but its conf never reached env/dist/etc/modules -> no card
    { key: "mod-solocraft", name: "Solocraft", desc: "Solo scaling.", installed: true, conf_name: "Solocraft.conf" },
    // not installed -> no card even though the conf name would be present
    { key: "mod-ale", name: "ALE", desc: "Lua engine.", installed: false, conf_name: "mod_ale.conf" },
    // no conf file at all (mod-aoe-loot style) -> no card
    { key: "mod-junk-to-gold", name: "Junk to Gold", desc: "Auto-sell.", installed: true, conf_name: "" },
  ];
  const files = [
    { name: "mod_ahbot.conf" },
    { name: "mod_ale.conf" },
    { name: "playerbots.conf" },
    { name: "transmog.conf" },
  ];

  it("keeps only installed modules whose conf passes the editable-files list, in order", () => {
    expect(installedConfModules(cpp, files)).toEqual([
      { key: "mod-ah-bot", name: "Auction House Bot", desc: "AH bot.", conf: "mod_ahbot.conf" },
      { key: "mod-transmog", name: "Transmogrification", desc: "Reskin gear.", conf: "transmog.conf" },
    ]);
  });

  it("is empty when nothing qualifies", () => {
    expect(installedConfModules([], files)).toEqual([]);
    expect(installedConfModules(cpp, [])).toEqual([]);
  });
});

describe("confKeyHint", () => {
  it("joins help and default when both exist", () => {
    expect(confKeyHint({ help: "Enables transmog.", default: "1" })).toBe(
      "Enables transmog. — Default: 1",
    );
  });
  it("degrades to whichever part exists", () => {
    expect(confKeyHint({ help: "", default: "1" })).toBe("Default: 1");
    expect(confKeyHint({ help: "Doc only.", default: null })).toBe("Doc only.");
    expect(confKeyHint({ default: null })).toBe("");
  });
});
