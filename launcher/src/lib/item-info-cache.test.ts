import { describe, it, expect } from "vitest";
import { cacheSet, missingEntries } from "./item-info-cache";
import type { ItemInfo } from "./api";

// Item-info payloads carry base64 icons, so the session cache needs a ceiling
// and callers must never re-fetch what they already hold. Both pages (paperdoll
// and Item Database) share these two rules.

const info = (entry: number): ItemInfo => ({ entry, source: "wowhead" });

describe("cacheSet", () => {
  it("stores the value under its key", () => {
    const c = new Map<number, ItemInfo>();
    cacheSet(c, 1, info(1), 3);
    expect(c.get(1)?.entry).toBe(1);
  });

  it("evicts the oldest entry when a new key arrives at the ceiling", () => {
    const c = new Map<number, ItemInfo>();
    cacheSet(c, 1, info(1), 2);
    cacheSet(c, 2, info(2), 2);
    cacheSet(c, 3, info(3), 2);
    expect(c.has(1)).toBe(false); // oldest evicted
    expect(c.has(2)).toBe(true);
    expect(c.has(3)).toBe(true);
    expect(c.size).toBe(2);
  });

  it("does not evict when overwriting a key already in the cache", () => {
    // A re-fetch of a cached entry must not cost an unrelated eviction.
    const c = new Map<number, ItemInfo>();
    cacheSet(c, 1, info(1), 2);
    cacheSet(c, 2, info(2), 2);
    cacheSet(c, 2, info(2), 2);
    expect(c.has(1)).toBe(true);
    expect(c.size).toBe(2);
  });
});

describe("missingEntries", () => {
  it("returns the entries the cache does not hold", () => {
    const c = new Map<number, ItemInfo>();
    cacheSet(c, 10, info(10));
    expect(missingEntries(c, [10, 11, 12])).toEqual([11, 12]);
  });

  it("returns nothing when everything is already cached", () => {
    const c = new Map<number, ItemInfo>();
    cacheSet(c, 10, info(10));
    expect(missingEntries(c, [10])).toEqual([]);
  });

  it("de-duplicates repeated entries so a batch never fetches one twice", () => {
    const c = new Map<number, ItemInfo>();
    expect(missingEntries(c, [7, 7, 8, 7])).toEqual([7, 8]);
  });

  it("preserves first-seen order", () => {
    const c = new Map<number, ItemInfo>();
    expect(missingEntries(c, [3, 1, 2])).toEqual([3, 1, 2]);
  });

  it("handles an empty request", () => {
    const c = new Map<number, ItemInfo>();
    expect(missingEntries(c, [])).toEqual([]);
  });
});
