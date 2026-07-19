import { describe, expect, it } from "vitest";
import {
  parseGearSets,
  gearSetFromDoll,
  buildSpecs,
  chunkSpecs,
  planMails,
  runSequential,
  type GearSet,
  type GearSetItem,
} from "./gearsets.svelte";
import type { PaperdollData } from "./api";

function items(n: number): GearSetItem[] {
  return Array.from({ length: n }, (_, i) => ({
    slot: i,
    entry: 1000 + i,
    name: `Item ${i}`,
    quality: 3,
  }));
}

function set(n: number, name = "Raid"): GearSet {
  return { name, sourceChar: "Testen", class: 8, level: 80, capturedAt: 1, items: items(n) };
}

describe("parseGearSets", () => {
  it("degrades garbage to empty", () => {
    expect(parseGearSets(null)).toEqual([]);
    expect(parseGearSets("nope")).toEqual([]);
    expect(parseGearSets('{"a":1}')).toEqual([]);
  });
  it("keeps valid sets and drops malformed items", () => {
    const raw = JSON.stringify([
      { name: "Ok", sourceChar: "T", class: 8, level: 80, capturedAt: 5, items: [{ slot: 0, entry: 42, name: "X", quality: 4 }, { entry: "bad" }, { entry: -1 }] },
      { items: [] },
      { name: "NoItems" },
    ]);
    const sets = parseGearSets(raw);
    expect(sets).toHaveLength(1);
    expect(sets[0].name).toBe("Ok");
    expect(sets[0].items).toEqual([{ slot: 0, entry: 42, name: "X", quality: 4 }]);
  });
});

describe("gearSetFromDoll", () => {
  it("captures every equipped slot incl. shirt/tabard and trims the name", () => {
    const doll = {
      name: "Testen",
      level: 80,
      class: 8,
      race: 10,
      gender: 0,
      skin: 0,
      face: 0,
      hair_style: 0,
      hair_color: 0,
      facial_style: 0,
      gold: 1,
      note: "last_saved",
      equipped: [
        { slot: 3, entry: 4330, name: "Shirt", quality: 1, item_level: 1, displayid: 1 },
        { slot: 18, entry: 45574, name: "Tabard", quality: 1, item_level: 1, displayid: 2 },
      ],
    } as PaperdollData;
    const s = gearSetFromDoll(doll, "  My Set  ");
    expect(s.name).toBe("My Set");
    expect(s.sourceChar).toBe("Testen");
    expect(s.items.map((i) => i.entry)).toEqual([4330, 45574]);
  });
});

describe("buildSpecs", () => {
  it("always uses count 1, never merges duplicate entries", () => {
    const dup: GearSetItem[] = [
      { slot: 10, entry: 999, name: "Ring", quality: 4 },
      { slot: 11, entry: 999, name: "Ring", quality: 4 },
    ];
    expect(buildSpecs(dup)).toEqual(["999:1", "999:1"]);
  });
});

describe("chunkSpecs", () => {
  it.each([
    [0, 0],
    [12, 1],
    [13, 2],
    [19, 2],
  ])("%i items -> %i chunks", (n, chunks) => {
    const out = chunkSpecs(buildSpecs(items(n)));
    expect(out).toHaveLength(chunks);
    for (const c of out) expect(c.length).toBeLessThanOrEqual(12);
    expect(out.flat()).toHaveLength(n);
  });
});

describe("planMails", () => {
  it("numbers subjects i/n and joins specs with commas", () => {
    const plan = planMails(set(13), "Alt");
    expect(plan).toHaveLength(2);
    expect(plan[0].subject).toBe("Gear set: Raid (1/2)");
    expect(plan[1].subject).toBe("Gear set: Raid (2/2)");
    expect(plan[0].items.split(",")).toHaveLength(12);
    expect(plan[1].items.split(",")).toHaveLength(1);
  });
});

describe("runSequential", () => {
  it("sends every chunk in order on success", async () => {
    const sent: string[] = [];
    const progress: number[] = [];
    const out = await runSequential(
      planMails(set(19), "Alt"),
      async (e) => {
        sent.push(e.subject);
      },
      (n) => progress.push(n),
    );
    expect(out).toEqual({ sent: 2, total: 2, error: null });
    expect(sent).toEqual(["Gear set: Raid (1/2)", "Gear set: Raid (2/2)"]);
    expect(progress).toEqual([1, 2]);
  });
  it("stops at the first failure and reports delivered-so-far honestly", async () => {
    let calls = 0;
    const out = await runSequential(planMails(set(19), "Alt"), async () => {
      calls++;
      if (calls === 2) throw { message: "SOAP fault", hint: "bad entry" };
    });
    expect(calls).toBe(2);
    expect(out.sent).toBe(1);
    expect(out.error).toContain("mail 2 of 2 failed");
    expect(out.error).toContain("SOAP fault");
    expect(out.error).toContain("already delivered");
  });
  it("a first-mail failure does not claim earlier deliveries", async () => {
    const out = await runSequential(planMails(set(5), "Alt"), async () => {
      throw { message: "boom" };
    });
    expect(out.sent).toBe(0);
    expect(out.error).toContain("mail 1 of 1 failed");
    expect(out.error).not.toContain("already delivered");
  });
});
