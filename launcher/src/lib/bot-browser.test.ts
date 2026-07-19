import { describe, expect, it } from "vitest";
import {
  parseFavs,
  toggleFav,
  sortWithFavs,
  pageInfo,
  zoneName,
  levelFilter,
  levelValid,
} from "./bot-browser";
import type { BotRow } from "./api";

function row(name: string): BotRow {
  return { guid: 1, name, class: 8, race: 10, gender: 0, level: 80, online: false, zone: 0 };
}

describe("parseFavs", () => {
  it("parses a clean list", () => {
    expect(parseFavs('["Botmage","Botwar"]')).toEqual(["Botmage", "Botwar"]);
  });
  it("degrades garbage to empty", () => {
    expect(parseFavs(null)).toEqual([]);
    expect(parseFavs("")).toEqual([]);
    expect(parseFavs("not json")).toEqual([]);
    expect(parseFavs('{"a":1}')).toEqual([]);
    expect(parseFavs("42")).toEqual([]);
  });
  it("drops non-strings, invalid names and duplicates", () => {
    expect(parseFavs('["Ok",7,null,"bad name","waytoolongname13","Ok","Two_2"]')).toEqual([
      "Ok",
      "Two_2",
    ]);
  });
});

describe("toggleFav", () => {
  it("adds when absent, removes when present, never mutates input", () => {
    const start = ["A"];
    const added = toggleFav(start, "B");
    expect(added).toEqual(["A", "B"]);
    expect(start).toEqual(["A"]);
    expect(toggleFav(added, "A")).toEqual(["B"]);
  });
});

describe("sortWithFavs", () => {
  it("pins favorites first, preserving order within both groups", () => {
    const rows = [row("Alpha"), row("Bravo"), row("Charlie"), row("Delta")];
    const sorted = sortWithFavs(rows, ["Delta", "Bravo"]);
    expect(sorted.map((r) => r.name)).toEqual(["Bravo", "Delta", "Alpha", "Charlie"]);
  });
  it("no favorites -> unchanged order", () => {
    const rows = [row("A"), row("B")];
    expect(sortWithFavs(rows, []).map((r) => r.name)).toEqual(["A", "B"]);
  });
});

describe("pageInfo", () => {
  it("first page of many", () => {
    expect(pageInfo(2500, 50, 0)).toEqual({ page: 1, pages: 50, hasPrev: false, hasNext: true });
  });
  it("middle page", () => {
    expect(pageInfo(2500, 50, 100)).toEqual({ page: 3, pages: 50, hasPrev: true, hasNext: true });
  });
  it("last page (partial)", () => {
    expect(pageInfo(120, 50, 100)).toEqual({ page: 3, pages: 3, hasPrev: true, hasNext: false });
  });
  it("empty result set is one page with no nav", () => {
    expect(pageInfo(0, 50, 0)).toEqual({ page: 1, pages: 1, hasPrev: false, hasNext: false });
  });
  it("offset beyond the end clamps the page number", () => {
    expect(pageInfo(10, 50, 500)).toEqual({ page: 1, pages: 1, hasPrev: true, hasNext: false });
  });
});

describe("zoneName", () => {
  it("maps known ids and falls back for unknown", () => {
    expect(zoneName(1637)).toBe("Orgrimmar");
    expect(zoneName(999999)).toBe("zone 999999");
  });
});

describe("levelFilter", () => {
  it("treats every empty shape as no filter (undefined)", () => {
    // "" pristine, null = cleared numberlike input, undefined = never set.
    expect(levelFilter("")).toBeUndefined();
    expect(levelFilter(null)).toBeUndefined();
    expect(levelFilter(undefined)).toBeUndefined();
  });
  it("a cleared Max field does NOT become 0 (the zero-results bug)", () => {
    // Regression: `null === ""` is false, so null used to fall through to 0.
    expect(levelFilter(null)).not.toBe(0);
  });
  it("keeps a real numeric bound, floored and non-negative", () => {
    expect(levelFilter(60)).toBe(60);
    expect(levelFilter("42")).toBe(42);
    expect(levelFilter(12.9)).toBe(12);
  });
  it("garbage degrades to undefined, not NaN", () => {
    expect(levelFilter("abc")).toBeUndefined();
    expect(levelFilter(NaN)).toBeUndefined();
  });
});

describe("levelValid", () => {
  it("accepts whole numbers in 1..255 regardless of number/string form", () => {
    // The number case is the crash the string-only .trim() helper hit.
    expect(levelValid(80)).toBe(true);
    expect(levelValid("80")).toBe(true);
    expect(levelValid(1)).toBe(true);
    expect(levelValid(255)).toBe(true);
  });
  it("does not throw when handed the numberlike bind read-back", () => {
    expect(() => levelValid(80)).not.toThrow();
    expect(() => levelValid(null)).not.toThrow();
  });
  it("rejects empty, out-of-range and non-integer values", () => {
    expect(levelValid("")).toBe(false);
    expect(levelValid(null)).toBe(false);
    expect(levelValid(undefined)).toBe(false);
    expect(levelValid(0)).toBe(false);
    expect(levelValid(256)).toBe(false);
    expect(levelValid(12.5)).toBe(false);
    expect(levelValid("abc")).toBe(false);
  });
});
