// Pure helpers for the Dashboard's in-game-style Talents card (Task O2).
// Kept dependency-free (no JSON import here) so vitest can exercise them
// with hand-made mini trees instead of pulling in the full 49KB
// talent-trees-wotlk.json.

export interface Talent {
  id: number;
  row: number;
  col: number;
  // spellId per rank, index 0 = rank 1, 1-5 entries long.
  ranks: number[];
  // Carried through for a later iteration (prereq arrow rendering) -- not
  // consumed by any helper below.
  prereq?: { id: number; rank: number }[];
}

export interface Tree {
  id: number;
  name: string;
  talents: Talent[];
}

// Highest learned rank for a talent, given the set of learned spell ids
// (progress.talents.spells, active spec only). In practice the learned set
// contains only the single spell id for the highest rank actually taken --
// WoW doesn't grant separate "learned" flags per rank -- but this scans
// every rank and takes the max index+1 found rather than trusting a
// particular position, so it degrades safely if that assumption ever
// doesn't hold (e.g. stale/partial data).
export function learnedRank(talent: Talent, learnedSet: Set<number>): number {
  let rank = 0;
  for (let i = 0; i < talent.ranks.length; i++) {
    if (learnedSet.has(talent.ranks[i])) rank = Math.max(rank, i + 1);
  }
  return rank;
}

// Total points spent in a tree: sum of learned ranks across its talents.
export function treePoints(tree: Tree, learnedSet: Set<number>): number {
  let total = 0;
  for (const talent of tree.talents) total += learnedRank(talent, learnedSet);
  return total;
}

// Number of grid rows needed to render a tree: one more than the highest
// `row` value present among its talents (0 for an empty tree).
export function treeRows(tree: Tree): number {
  let maxRow = -1;
  for (const talent of tree.talents) if (talent.row > maxRow) maxRow = talent.row;
  return maxRow + 1;
}
