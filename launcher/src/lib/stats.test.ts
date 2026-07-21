import { describe, expect, it } from "vitest";
import {
  avgGuildSize,
  bucketValue,
  continentName,
  fillLevelBuckets,
  formatGold,
  formatLastSeen,
  formatPlaytime,
  formatYears,
  levelBucketLabel,
  pct,
  pickClasses,
  pickFactions,
  pickRichest,
  pickTopLevels,
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
  it("continentName renders the CLI's -1 bucket as Other worlds (8d)", () => {
    expect(continentName(-1)).toBe("Other worlds");
  });
});

describe("avgGuildSize", () => {
  it("is members/guilds to one decimal, 0 with no guilds", () => {
    expect(avgGuildSize(300, 20)).toBe("15.0");
    expect(avgGuildSize(0, 0)).toBe("0");
  });
});

describe("segment filter (All|Family|Bots)", () => {
  const classes = {
    family: [
      { class: 1, count: 3 },
      { class: 8, count: 1 },
    ],
    bots: [
      { class: 1, count: 37 },
      { class: 5, count: 20 },
    ],
  };

  it("pickClasses returns a single segment untouched", () => {
    expect(pickClasses("family", classes)).toEqual(classes.family);
    expect(pickClasses("bots", classes)).toEqual(classes.bots);
  });

  it("pickClasses merges 'all' by summing per class id, sorted by id", () => {
    expect(pickClasses("all", classes)).toEqual([
      { class: 1, count: 40 },
      { class: 5, count: 20 },
      { class: 8, count: 1 },
    ]);
  });

  it("pickFactions adds the two splits for 'all'", () => {
    const f = {
      family: { alliance: 3, horde: 1 },
      bots: { alliance: 117, horde: 133 },
    };
    expect(pickFactions("family", f)).toEqual({ alliance: 3, horde: 1 });
    expect(pickFactions("all", f)).toEqual({ alliance: 120, horde: 134 });
  });

  it("pickTopLevels re-takes the top 5 by level from the merged lists", () => {
    const tops = {
      family: [
        { name: "Milla", level: 80, family: true },
        { name: "Venn", level: 12, family: true },
      ],
      bots: [
        { name: "Bota", level: 80, family: false },
        { name: "Botb", level: 79, family: false },
        { name: "Botc", level: 78, family: false },
        { name: "Botd", level: 77, family: false },
      ],
    };
    const all = pickTopLevels("all", tops);
    expect(all).toHaveLength(5);
    expect(all.map((t) => t.name)).toEqual(["Bota", "Milla", "Botb", "Botc", "Botd"]);
    expect(pickTopLevels("family", tops)).toEqual(tops.family);
  });

  it("pickRichest re-takes the top 5 by copper from the merged lists", () => {
    const rich = {
      family: [{ name: "Milla", copper: 90_000, family: true }],
      bots: [
        { name: "Goldy", copper: 1_211_290_000, family: false },
        { name: "Poor", copper: 10, family: false },
      ],
    };
    expect(pickRichest("all", rich).map((r) => r.name)).toEqual(["Goldy", "Milla", "Poor"]);
    expect(pickRichest("bots", rich)).toEqual(rich.bots);
  });

  it("bucketValue picks the segment's series ('all' stacks)", () => {
    const l = { bucket: 3, family: 2, bots: 30 };
    expect(bucketValue("all", l)).toBe(32);
    expect(bucketValue("family", l)).toBe(2);
    expect(bucketValue("bots", l)).toBe(30);
  });
});
