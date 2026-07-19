import { describe, expect, it } from "vitest";
import { parseFavs, toggleFav, sortWithFavs, pageInfo, zoneName } from "./bot-browser";
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
