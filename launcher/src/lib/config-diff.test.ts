import { describe, expect, it } from "vitest";
import { dirtyKeys, requiredSaveFlags, settingsInGroups, clearSavedEdits } from "./config-diff";

const settings = [
  { key: "rates.xp_kill", value: "1" },
  { key: "server.motd", value: "Hi" },
];

describe("dirtyKeys", () => {
  it("returns only keys whose edit differs from the live value", () => {
    expect(dirtyKeys(settings, {})).toEqual([]);
    expect(dirtyKeys(settings, { "rates.xp_kill": "1" })).toEqual([]);
    expect(dirtyKeys(settings, { "rates.xp_kill": "3" })).toEqual(["rates.xp_kill"]);
    expect(dirtyKeys(settings, { "rates.xp_kill": "3", "server.motd": "Yo" })).toEqual([
      "rates.xp_kill",
      "server.motd",
    ]);
  });
  it("ignores edits for keys that do not exist", () => {
    expect(dirtyKeys(settings, { ghost: "1" })).toEqual([]);
  });
});

describe("requiredSaveFlags", () => {
  const rows = [
    { key: "legacy.env_row", env: "AC_SOME_FUTURE_ENV_ROW" },
    { key: "server.motd", env: "-" },
    { key: "rates.honor", env: "conf:Rate.Honor" },
    { key: "bots.talk", env: "conf:playerbots.conf:AiPlayerbot.RandomBotTalk" },
    { key: "ahbot.seller", env: "conf:mod_ahbot.conf:AuctionHouseBot.EnableSeller" },
  ];

  it("maps env rows and motd to settings-save", () => {
    expect(requiredSaveFlags(rows, ["legacy.env_row", "server.motd"])).toEqual(["settings-save"]);
  });

  it("maps worldserver conf rows to rates-live", () => {
    expect(requiredSaveFlags(rows, ["rates.honor"])).toEqual(["rates-live"]);
  });

  it("maps playerbots conf rows to bots-world", () => {
    expect(requiredSaveFlags(rows, ["bots.talk"])).toEqual(["bots-world"]);
  });

  it("maps mod_ahbot conf rows to ahbot-page (Batch 4 F14)", () => {
    expect(requiredSaveFlags(rows, ["ahbot.seller"])).toEqual(["ahbot-page"]);
  });

  it("a mixed dirty set needs every mechanism's flag; unknown keys ignored", () => {
    expect(
      requiredSaveFlags(rows, ["legacy.env_row", "rates.honor", "bots.talk", "ahbot.seller", "ghost"]).sort(),
    ).toEqual(["ahbot-page", "bots-world", "rates-live", "settings-save"]);
    expect(requiredSaveFlags(rows, [])).toEqual([]);
  });
});

describe("settingsInGroups (per-tab Save scoping)", () => {
  const rows = [
    { key: "rates.xp", group: "Rates", value: "1" },
    { key: "server.motd", group: "Server", value: "Hi" },
    { key: "bots.count", group: "Bot World", value: "2000" },
    { key: "ahbot.seller", group: "Auction House", value: "1" },
  ];

  it("keeps only rows whose group is currently visible", () => {
    expect(settingsInGroups(rows, ["Rates", "Server"]).map((s) => s.key)).toEqual([
      "rates.xp",
      "server.motd",
    ]);
    expect(settingsInGroups(rows, ["Bot World"]).map((s) => s.key)).toEqual(["bots.count"]);
    expect(settingsInGroups(rows, ["Auction House"]).map((s) => s.key)).toEqual(["ahbot.seller"]);
  });

  it("scopes dirtyKeys so one tab's edits don't leak into another tab's Save", () => {
    // A Bot World edit is dirty, but on the Settings tab (Rates/Server groups)
    // it must NOT appear as dirty -- that was the pre-fix cross-tab bleed.
    const edits = { "bots.count": "5000" };
    expect(dirtyKeys(settingsInGroups(rows, ["Rates", "Server"]), edits)).toEqual([]);
    expect(dirtyKeys(settingsInGroups(rows, ["Bot World"]), edits)).toEqual(["bots.count"]);
  });

  it("returns nothing when no group matches", () => {
    expect(settingsInGroups(rows, [])).toEqual([]);
    expect(settingsInGroups(rows, ["Nope"])).toEqual([]);
  });
});

describe("clearSavedEdits (per-tab Save keeps other tabs' pending edits)", () => {
  it("drops only the just-saved keys and keeps the rest", () => {
    // User edited a Bot World row AND a Settings row, then saved only the
    // Settings row. The Bot World edit must survive the post-save reload.
    const edits = { "bots.count": "5000", "server.motd": "Yo" };
    expect(clearSavedEdits(edits, ["server.motd"])).toEqual({ "bots.count": "5000" });
  });

  it("does not mutate the input map", () => {
    const edits = { a: "1", b: "2" };
    const out = clearSavedEdits(edits, ["a"]);
    expect(edits).toEqual({ a: "1", b: "2" });
    expect(out).toEqual({ b: "2" });
  });

  it("empty saved list keeps every edit; saving all clears the map", () => {
    const edits = { a: "1", b: "2" };
    expect(clearSavedEdits(edits, [])).toEqual({ a: "1", b: "2" });
    expect(clearSavedEdits(edits, ["a", "b"])).toEqual({});
  });

  it("ignores saved keys that aren't present in edits", () => {
    expect(clearSavedEdits({ a: "1" }, ["ghost"])).toEqual({ a: "1" });
  });
});
