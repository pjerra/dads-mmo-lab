import { describe, expect, it } from "vitest";
import {
  categoryTree,
  earnedPoints,
  scopeAchievements,
  type AchievementCategory,
  type AchievementDef,
} from "./achievements";

// Hand-made mini dataset -- deliberately NOT the real 217KB
// achievements-wotlk.json, per the task brief (keep unit tests fast and
// independent of the data file's exact contents).
const categories: AchievementCategory[] = [
  { id: 1, parent: null, name: "General", order: 1 },
  { id: 2, parent: null, name: "Quests", order: 2 },
  { id: 10, parent: 1, name: "Leveling", order: 1 },
  { id: 11, parent: 1, name: "Exploration", order: 2 },
];

const achievements: AchievementDef[] = [
  { id: 100, cat: 1, name: "Root A2", desc: "", points: 5, order: 2, faction: -1 },
  { id: 101, cat: 1, name: "Root A1", desc: "", points: 10, order: 1, faction: -1 },
  { id: 200, cat: 10, name: "Child A", desc: "", points: 15, order: 1, faction: 0 },
  { id: 201, cat: 11, name: "Child B", desc: "", points: 20, order: 1, faction: 1 },
  { id: 300, cat: 2, name: "Other root", desc: "", points: 25, order: 1, faction: -1 },
];

describe("categoryTree", () => {
  it("returns only root categories at the top level, in order", () => {
    const tree = categoryTree(categories);
    expect(tree.map((n) => n.root.id)).toEqual([1, 2]);
  });

  it("nests each root's children beneath it, in order", () => {
    const tree = categoryTree(categories);
    const general = tree.find((n) => n.root.id === 1)!;
    expect(general.children.map((c) => c.id)).toEqual([10, 11]);
  });

  it("gives a root with no children an empty children array", () => {
    const tree = categoryTree(categories);
    const quests = tree.find((n) => n.root.id === 2)!;
    expect(quests.children).toEqual([]);
  });

  it("orders roots by their `order` field, not id or array position", () => {
    const reordered: AchievementCategory[] = [
      { id: 2, parent: null, name: "Second", order: 2 },
      { id: 1, parent: null, name: "First", order: 1 },
    ];
    expect(categoryTree(reordered).map((n) => n.root.id)).toEqual([1, 2]);
  });
});

describe("scopeAchievements", () => {
  it("returns only a leaf category's own achievements", () => {
    const scope = scopeAchievements(achievements, 10, categories);
    expect(scope.map((a) => a.id)).toEqual([200]);
  });

  it("returns a root category's own achievements plus its children's, sorted by order then id", () => {
    const scope = scopeAchievements(achievements, 1, categories);
    // Sort is across the WHOLE scope (root + children combined), not
    // grouped by category first: order-1 achievements 101/200/201 (tied,
    // broken by id) come before order-2 achievement 100.
    expect(scope.map((a) => a.id)).toEqual([101, 200, 201, 100]);
  });

  it("does not leak achievements from an unrelated category", () => {
    const scope = scopeAchievements(achievements, 1, categories);
    expect(scope.map((a) => a.id)).not.toContain(300);
  });

  it("returns an empty array for a category with no achievements", () => {
    expect(scopeAchievements(achievements, 999, categories)).toEqual([]);
  });

  it("treats an unknown catId as a leaf (no children folded in)", () => {
    expect(scopeAchievements(achievements, 999, categories)).toEqual([]);
  });
});

describe("earnedPoints", () => {
  it("returns 0 when nothing is earned", () => {
    expect(earnedPoints(achievements, new Set())).toBe(0);
  });

  it("sums points only for achievements present in the earned set", () => {
    expect(earnedPoints(achievements, new Set([100, 200]))).toBe(5 + 15);
  });

  it("ignores earned ids that aren't in the achievement list", () => {
    expect(earnedPoints(achievements, new Set([9999]))).toBe(0);
  });

  it("sums all points when everything is earned", () => {
    const all = new Set(achievements.map((a) => a.id));
    expect(earnedPoints(achievements, all)).toBe(5 + 10 + 15 + 20 + 25);
  });
});
