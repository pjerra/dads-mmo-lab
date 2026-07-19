import { describe, expect, it } from "vitest";
import { filterPbKeys, stagedPbChanges } from "./pb-keys";

const keys = [
  { key: "AiPlayerbot.MaxRandomBots", value: "500", default: "500", line: 3 },
  { key: "AiPlayerbot.RandomBotTalk", value: "1", default: "1", line: 8 },
  { key: "AIPlayerbot.GuildFeedback", value: "1", default: null, line: 12 },
];

describe("filterPbKeys", () => {
  it("returns everything for an empty or whitespace query", () => {
    expect(filterPbKeys(keys, "")).toEqual(keys);
    expect(filterPbKeys(keys, "   ")).toEqual(keys);
  });

  it("matches case-insensitively on the key substring", () => {
    expect(filterPbKeys(keys, "talk").map((k) => k.key)).toEqual(["AiPlayerbot.RandomBotTalk"]);
    expect(filterPbKeys(keys, "RANDOM").map((k) => k.key)).toEqual([
      "AiPlayerbot.MaxRandomBots",
      "AiPlayerbot.RandomBotTalk",
    ]);
    expect(filterPbKeys(keys, "nope")).toEqual([]);
  });
});

describe("stagedPbChanges", () => {
  it("returns only edits that differ from the current value", () => {
    expect(stagedPbChanges(keys, {})).toEqual([]);
    expect(stagedPbChanges(keys, { "AiPlayerbot.RandomBotTalk": "1" })).toEqual([]);
    expect(stagedPbChanges(keys, { "AiPlayerbot.RandomBotTalk": "0" })).toEqual([
      { key: "AiPlayerbot.RandomBotTalk", value: "0" },
    ]);
  });

  it("ignores edits for keys not present in the parsed list", () => {
    expect(stagedPbChanges(keys, { ghost: "1" })).toEqual([]);
  });

  it("keeps the user's edit order", () => {
    const edits: Record<string, string> = {};
    edits["AIPlayerbot.GuildFeedback"] = "0";
    edits["AiPlayerbot.MaxRandomBots"] = "900";
    expect(stagedPbChanges(keys, edits).map((c) => c.key)).toEqual([
      "AIPlayerbot.GuildFeedback",
      "AiPlayerbot.MaxRandomBots",
    ]);
  });
});
