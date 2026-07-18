import { describe, expect, it } from "vitest";
import { learnedRank, treePoints, treeRows, type Talent, type Tree } from "./talent-trees";

// Hand-made mini tree -- deliberately NOT the real 49KB
// talent-trees-wotlk.json, per the task brief (keep unit tests fast and
// independent of the data file's exact contents).
const threeRankTalent: Talent = { id: 1, row: 0, col: 0, ranks: [100, 101, 102] };
const oneRankTalent: Talent = { id: 2, row: 0, col: 1, ranks: [200] };
const fiveRankTalent: Talent = { id: 3, row: 2, col: 3, ranks: [300, 301, 302, 303, 304] };

const miniTree: Tree = {
  id: 1,
  name: "Test Tree",
  talents: [threeRankTalent, oneRankTalent, fiveRankTalent],
};

describe("learnedRank", () => {
  it("returns 0 when none of the talent's ranks are learned", () => {
    expect(learnedRank(threeRankTalent, new Set())).toBe(0);
    expect(learnedRank(threeRankTalent, new Set([999]))).toBe(0);
  });

  it("returns the 1-based rank matching the learned spell id", () => {
    expect(learnedRank(threeRankTalent, new Set([100]))).toBe(1);
    expect(learnedRank(threeRankTalent, new Set([101]))).toBe(2);
    expect(learnedRank(threeRankTalent, new Set([102]))).toBe(3);
  });

  it("handles a single-rank talent", () => {
    expect(learnedRank(oneRankTalent, new Set([200]))).toBe(1);
    expect(learnedRank(oneRankTalent, new Set())).toBe(0);
  });

  it("handles a 5-rank talent at max rank", () => {
    expect(learnedRank(fiveRankTalent, new Set([304]))).toBe(5);
  });

  it("defensively takes the max index found if multiple ranks are present in the learned set", () => {
    // Real data only ever contains the single highest-rank spell id, but
    // the derivation must not silently pick a lower rank if it ever sees
    // more than one -- it should take the max, not the first/last match.
    expect(learnedRank(threeRankTalent, new Set([100, 102]))).toBe(3);
    expect(learnedRank(fiveRankTalent, new Set([301, 300, 303]))).toBe(4);
  });

  it("ignores spell ids belonging to other talents", () => {
    expect(learnedRank(oneRankTalent, new Set([100, 101, 102, 300]))).toBe(0);
  });
});

describe("treePoints", () => {
  it("returns 0 for a tree with nothing learned", () => {
    expect(treePoints(miniTree, new Set())).toBe(0);
  });

  it("sums learned ranks across all talents in the tree", () => {
    // rank 2 (of 3) + rank 1 (of 1) + rank 4 (of 5) = 7
    const learned = new Set([101, 200, 303]);
    expect(treePoints(miniTree, learned)).toBe(7);
  });

  it("sums to the max when every talent is maxed", () => {
    const learned = new Set([102, 200, 304]);
    expect(treePoints(miniTree, learned)).toBe(3 + 1 + 5);
  });

  it("returns 0 for a tree with no talents", () => {
    expect(treePoints({ id: 2, name: "Empty", talents: [] }, new Set([100]))).toBe(0);
  });

  it("ignores spell ids not present in the tree", () => {
    expect(treePoints(miniTree, new Set([9999]))).toBe(0);
  });
});

describe("treeRows", () => {
  it("returns max row + 1 for a populated tree", () => {
    // talents in miniTree span rows 0 and 2 -> 3 rows needed
    expect(treeRows(miniTree)).toBe(3);
  });

  it("returns 1 for a tree whose talents are all on row 0", () => {
    const tree: Tree = { id: 3, name: "Flat", talents: [oneRankTalent] };
    expect(treeRows(tree)).toBe(1);
  });

  it("returns 0 for a tree with no talents", () => {
    expect(treeRows({ id: 4, name: "Empty", talents: [] })).toBe(0);
  });

  it("uses the highest row even if talents are out of order", () => {
    const tree: Tree = {
      id: 5,
      name: "Reordered",
      talents: [
        { id: 10, row: 4, col: 0, ranks: [1] },
        { id: 11, row: 1, col: 0, ranks: [2] },
        { id: 12, row: 7, col: 0, ranks: [3] },
      ],
    };
    expect(treeRows(tree)).toBe(8);
  });
});
