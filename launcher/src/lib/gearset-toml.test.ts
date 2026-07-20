import { describe, expect, it } from "vitest";
import { gearSetToToml, gearSetFromToml } from "./gearset-toml";
import type { GearSet } from "./gearsets.svelte";

function set(overrides: Partial<GearSet> = {}): GearSet {
  return {
    name: "Raid",
    sourceChar: "Testen",
    class: 8,
    level: 80,
    capturedAt: 1699999999999,
    items: [
      { slot: 0, entry: 40000, name: "Helm of the Fjord", quality: 4 },
      { slot: 15, entry: 41000, name: "Cloak", quality: 3 },
    ],
    ...overrides,
  };
}

describe("gearSetToToml + gearSetFromToml round-trip", () => {
  it("round-trips a normal set exactly", () => {
    const s = set();
    const back = gearSetFromToml(gearSetToToml(s));
    expect(back).toEqual(s);
  });

  it("preserves quotes/backslashes in item names", () => {
    const s = set({
      items: [{ slot: 1, entry: 999, name: 'The "Real" Blade \\ Edge', quality: 5 }],
    });
    const toml = gearSetToToml(s);
    expect(gearSetFromToml(toml)).toEqual(s);
  });

  it("round-trips duplicate ring entries (no merge)", () => {
    const s = set({
      items: [
        { slot: 10, entry: 555, name: "Signet", quality: 4 },
        { slot: 11, entry: 555, name: "Signet", quality: 4 },
      ],
    });
    const back = gearSetFromToml(gearSetToToml(s));
    expect(back.items).toHaveLength(2);
    expect(back.items.map((i) => i.entry)).toEqual([555, 555]);
  });

  it("emits a TOML shape with a comment, top-level keys and [[items]] tables", () => {
    const toml = gearSetToToml(set());
    expect(toml.startsWith("#")).toBe(true);
    expect(toml).toContain('name = "Raid"');
    expect(toml).toContain("[[items]]");
    expect(toml).toContain("entry = 40000");
    expect((toml.match(/\[\[items\]\]/g) ?? []).length).toBe(2);
  });
});

describe("gearSetFromToml hardening", () => {
  it("throws on empty/blank input", () => {
    expect(() => gearSetFromToml("")).toThrow();
    expect(() => gearSetFromToml("   \n  ")).toThrow();
  });

  it("throws when there is no name", () => {
    expect(() => gearSetFromToml('[[items]]\nentry = 1\nname = "x"\n')).toThrow();
  });

  it("throws when there are no valid items", () => {
    expect(() => gearSetFromToml('name = "Empty"\n')).toThrow();
    // an item with a non-positive/invalid entry is dropped -> no valid items
    expect(() => gearSetFromToml('name = "Bad"\n[[items]]\nentry = -3\nname = "x"\n')).toThrow();
  });

  it("drops a malformed item but keeps the good ones", () => {
    const toml = [
      'name = "Mix"',
      "[[items]]",
      "entry = 100",
      'name = "Good"',
      "quality = 2",
      "[[items]]",
      "entry = 0", // invalid -> dropped by parseGearSets
      'name = "Zero"',
    ].join("\n");
    const s = gearSetFromToml(toml);
    expect(s.name).toBe("Mix");
    expect(s.items).toHaveLength(1);
    expect(s.items[0].entry).toBe(100);
  });

  it("tolerates comments, blank lines and surrounding whitespace", () => {
    const toml = [
      "# a header comment",
      "",
      '   name = "Trimmed"   ',
      "  [[items]]  ",
      "  entry = 42  ",
      '  name = "Spaced"  ',
      "  quality = 3  ",
    ].join("\n");
    const s = gearSetFromToml(toml);
    expect(s.name).toBe("Trimmed");
    expect(s.items[0].entry).toBe(42);
    expect(s.items[0].name).toBe("Spaced");
  });

  it("defaults missing scalar fields the same way the storage path does", () => {
    // Only name + one entry supplied -> parseGearSets fills the rest.
    const s = gearSetFromToml('name = "Sparse"\n[[items]]\nentry = 7\n');
    expect(s.sourceChar).toBe("?");
    expect(s.class).toBe(0);
    expect(s.items[0].quality).toBe(1);
    expect(s.items[0].name).toBe("item 7");
  });
});
