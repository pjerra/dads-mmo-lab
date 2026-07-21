import { describe, expect, it } from "vitest";
import { charView, findStoredChar, parseStoredChar, requestCharView } from "./char-store.svelte";
import type { Account } from "./api";

const ACCOUNTS: Account[] = [
  {
    id: 1,
    username: "PLAYER",
    gm_level: 3,
    characters: [
      { guid: 10, name: "Testen", level: 42 },
      { guid: 11, name: "Altfive", level: 5 },
    ],
  },
  { id: 2, username: "KIDDO", gm_level: 0, characters: [{ guid: 20, name: "Venn", level: 12 }] },
];

describe("parseStoredChar", () => {
  it("round-trips a valid payload", () => {
    const raw = JSON.stringify({ guid: 10, name: "Testen", account: "PLAYER" });
    expect(parseStoredChar(raw)).toEqual({ guid: 10, name: "Testen", account: "PLAYER" });
  });

  it("returns null for null/empty raw", () => {
    expect(parseStoredChar(null)).toBeNull();
    expect(parseStoredChar("")).toBeNull();
  });

  it("returns null for malformed JSON and wrong shapes", () => {
    expect(parseStoredChar("not json")).toBeNull();
    expect(parseStoredChar("{}")).toBeNull();
    expect(parseStoredChar(JSON.stringify({ guid: "10", name: "x", account: "y" }))).toBeNull();
    expect(parseStoredChar(JSON.stringify({ guid: 10, name: "", account: "y" }))).toBeNull();
    expect(parseStoredChar(JSON.stringify({ guid: 10, name: "x" }))).toBeNull();
    expect(parseStoredChar("null")).toBeNull();
  });
});

describe("findStoredChar", () => {
  it("finds the stored char by guid within its account", () => {
    const hit = findStoredChar(ACCOUNTS, { guid: 20, name: "Venn", account: "KIDDO" });
    expect(hit).toEqual({ account: "KIDDO", char: { guid: 20, name: "Venn", level: 12 } });
  });

  it("matches by guid even if the char was renamed since storing", () => {
    const hit = findStoredChar(ACCOUNTS, { guid: 11, name: "OldName", account: "PLAYER" });
    expect(hit?.char.name).toBe("Altfive");
  });

  it("returns null when the account is gone", () => {
    expect(findStoredChar(ACCOUNTS, { guid: 10, name: "Testen", account: "DELETED" })).toBeNull();
  });

  it("returns null when the char is gone from the account", () => {
    expect(findStoredChar(ACCOUNTS, { guid: 999, name: "Ghost", account: "PLAYER" })).toBeNull();
  });

  it("returns null for a null stored selection", () => {
    expect(findStoredChar(ACCOUNTS, null)).toBeNull();
  });
});

describe("charView request (Bot Browser -> Character page handoff)", () => {
  it("starts empty, carries a requested name, and can be cleared by the consumer", () => {
    expect(charView.requestedName).toBeNull();
    requestCharView("Botmage");
    expect(charView.requestedName).toBe("Botmage");
    charView.requestedName = null; // what Dashboard's adopt-effect does
    expect(charView.requestedName).toBeNull();
  });
});
