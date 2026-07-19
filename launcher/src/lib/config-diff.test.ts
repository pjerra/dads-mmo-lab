import { describe, expect, it } from "vitest";
import { dirtyKeys, requiredSaveFlags } from "./config-diff";

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
