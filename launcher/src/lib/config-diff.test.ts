import { describe, expect, it } from "vitest";
import { dirtyKeys } from "./config-diff";

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
