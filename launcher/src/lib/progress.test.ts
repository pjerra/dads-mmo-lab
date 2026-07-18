import { describe, expect, it } from "vitest";
import { chunkIds, formatEpochDate } from "./progress";

describe("chunkIds", () => {
  it("splits into chunks of the given size, remainder in the last chunk", () => {
    expect(chunkIds([1, 2, 3, 4, 5], 2)).toEqual([[1, 2], [3, 4], [5]]);
  });
  it("defaults to chunks of 25", () => {
    const ids = Array.from({ length: 30 }, (_, i) => i + 1);
    const chunks = chunkIds(ids);
    expect(chunks.length).toBe(2);
    expect(chunks[0].length).toBe(25);
    expect(chunks[1].length).toBe(5);
  });
  it("returns an empty array for an empty input", () => {
    expect(chunkIds([])).toEqual([]);
  });
});

describe("formatEpochDate", () => {
  it("formats a positive epoch as a UTC YYYY-MM-DD string", () => {
    expect(formatEpochDate(1700000000)).toBe("2023-11-14");
    expect(formatEpochDate(1690000000)).toBe("2023-07-22");
  });
  it("returns '' for epoch 0 and other non-positive/invalid values", () => {
    expect(formatEpochDate(0)).toBe("");
    expect(formatEpochDate(-1)).toBe("");
    expect(formatEpochDate(NaN)).toBe("");
  });
});
