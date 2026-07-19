// Pure helpers for the Dashboard's in-game-style Achievements browser
// (Task P2). Kept dependency-free (no big-JSON import here) so vitest can
// exercise them with hand-made fixtures instead of pulling in the full
// 217KB achievements-wotlk.json.

export interface AchievementCategory {
  id: number;
  // null = root category; otherwise the parent root's id.
  parent: number | null;
  name: string;
  order: number;
}

export interface AchievementDef {
  id: number;
  cat: number;
  name: string;
  desc: string;
  points: number;
  order: number;
  // -1 = both factions, 0 = horde, 1 = alliance. The launcher doesn't know
  // the viewed character's faction, so callers show every row regardless
  // of this field -- see the Achievements-tab comment in Dashboard.svelte.
  faction: number;
}

export interface CategoryNode {
  root: AchievementCategory;
  children: AchievementCategory[];
}

// Root categories (parent === null) in `order`, each with its child
// categories (also in `order`) nested beneath it -- the left-rail shape.
export function categoryTree(categories: AchievementCategory[]): CategoryNode[] {
  const roots = categories.filter((c) => c.parent === null).sort((a, b) => a.order - b.order);
  return roots.map((root) => ({
    root,
    children: categories.filter((c) => c.parent === root.id).sort((a, b) => a.order - b.order),
  }));
}

// Achievements visible for a selected category: just that category's own
// achievements, unless `catId` names a root category, in which case its
// children's achievements are included too (in-game behavior -- selecting
// a root shows everything beneath it). Sorted by in-game `order`, then id.
export function scopeAchievements(
  all: AchievementDef[],
  catId: number,
  categories: AchievementCategory[],
): AchievementDef[] {
  const cat = categories.find((c) => c.id === catId);
  const catIds = new Set<number>([catId]);
  if (cat && cat.parent === null) {
    for (const c of categories) if (c.parent === catId) catIds.add(c.id);
  }
  return all
    .filter((a) => catIds.has(a.cat))
    .sort((a, b) => a.order - b.order || a.id - b.id);
}

// Sum of `points` across achievements whose id is in `earnedSet`.
export function earnedPoints(all: AchievementDef[], earnedSet: Set<number>): number {
  let total = 0;
  for (const a of all) if (earnedSet.has(a.id)) total += a.points;
  return total;
}
