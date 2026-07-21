import { describe, expect, it } from "vitest";
import {
  avgGuildSize,
  continentName,
  fillLevelBuckets,
  formatGold,
  formatLastSeen,
  formatPlaytime,
  formatYears,
  levelBucketLabel,
  pct,
  zoneName,
} from "./stats";

describe("formatGold", () => {
  it("floors copper to whole gold with thousands separators", () => {
    expect(formatGold(1_211_292_125)).toBe("121,129g");
    expect(formatGold(10_000)).toBe("1g");
    expect(formatGold(9_999)).toBe("0g");
    expect(formatGold(0)).toBe("0g");
  });
  it("never goes negative", () => {
    expect(formatGold(-5)).toBe("0g");
  });
});

describe("formatPlaytime", () => {
  it("uses minutes under an hour", () => {
    expect(formatPlaytime(0)).toBe("0m");
    expect(formatPlaytime(611)).toBe("10m");
    expect(formatPlaytime(3599)).toBe("60m");
  });
  it("uses one-decimal hours under two days", () => {
    expect(formatPlaytime(63_720)).toBe("17.7h");
    expect(formatPlaytime(3600)).toBe("1.0h");
  });
  it("uses one-decimal days beyond two days", () => {
    expect(formatPlaytime(259_200)).toBe("3.0d");
  });
});

describe("formatYears", () => {
  it("converts the combined bot playtime to years", () => {
    expect(formatYears(562_221_203)).toBe("17.8 years");
    expect(formatYears(0)).toBe("0.0 years");
  });
});

describe("formatLastSeen", () => {
  const now = 1_750_000_000;
  it("0 means never (a brand-new character)", () => {
    expect(formatLastSeen(0, now)).toBe("never");
  });
  it("scales just now -> minutes -> hours -> days", () => {
    expect(formatLastSeen(now - 30, now)).toBe("just now");
    expect(formatLastSeen(now - 600, now)).toBe("10m ago");
    expect(formatLastSeen(now - 7200, now)).toBe("2h ago");
    expect(formatLastSeen(now - 3 * 86_400, now)).toBe("3d ago");
  });
  it("falls back to a date after a month", () => {
    // Just the shape -- exact text is locale-formatted en-US.
    expect(formatLastSeen(now - 90 * 86_400, now)).toMatch(/\d{4}/);
  });
});

describe("level buckets", () => {
  it("labels buckets as 1-10 ... 71-80", () => {
    expect(levelBucketLabel(0)).toBe("1-10");
    expect(levelBucketLabel(7)).toBe("71-80");
  });
  it("fills the missing buckets with zeros in order", () => {
    const filled = fillLevelBuckets([
      { bucket: 7, family: 2, bots: 30 },
      { bucket: 0, family: 1, bots: 50 },
    ]);
    expect(filled).toHaveLength(8);
    expect(filled[0]).toEqual({ bucket: 0, family: 1, bots: 50 });
    expect(filled[3]).toEqual({ bucket: 3, family: 0, bots: 0 });
    expect(filled[7]).toEqual({ bucket: 7, family: 2, bots: 30 });
  });
  it("keeps unexpected higher buckets appended (defensive)", () => {
    const filled = fillLevelBuckets([{ bucket: 9, family: 0, bots: 1 }]);
    expect(filled).toHaveLength(9);
    expect(filled[8].bucket).toBe(9);
  });
});

describe("pct", () => {
  it("is a clamped 0-100 percentage", () => {
    expect(pct(1, 4)).toBe(25);
    expect(pct(5, 4)).toBe(100);
    expect(pct(0, 4)).toBe(0);
  });
  it("is 0 when the total is 0 (empty DB must not divide by zero)", () => {
    expect(pct(3, 0)).toBe(0);
  });
});

describe("names", () => {
  it("zoneName uses the lookup and degrades to Zone <id>", () => {
    const names = { "1637": "Orgrimmar" };
    expect(zoneName(1637, names)).toBe("Orgrimmar");
    expect(zoneName(99999, names)).toBe("Zone 99999");
  });
  it("continentName maps the four playable maps and degrades to Map <id>", () => {
    expect(continentName(0)).toBe("Eastern Kingdoms");
    expect(continentName(1)).toBe("Kalimdor");
    expect(continentName(530)).toBe("Outland");
    expect(continentName(571)).toBe("Northrend");
    expect(continentName(37)).toBe("Map 37");
  });
});

describe("avgGuildSize", () => {
  it("is members/guilds to one decimal, 0 with no guilds", () => {
    expect(avgGuildSize(300, 20)).toBe("15.0");
    expect(avgGuildSize(0, 0)).toBe("0");
  });
});
