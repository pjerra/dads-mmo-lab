// Bot Browser (Batch 5 F1) pure helpers: favorites persistence + list
// ordering + pagination math. Pure functions are exported separately from the
// storage-touching ones so vitest (node env, no localStorage) covers the
// logic without a DOM -- same guarded-storage pattern as features.svelte.ts.

import type { BotRow } from "$lib/api";

export const FAVS_KEY = "dml.botfavs.v1";

function hasStorage(): boolean {
  try {
    return typeof localStorage !== "undefined";
  } catch {
    return false;
  }
}

// --- pure helpers (vitest targets) -----------------------------------------

// Parse whatever was in storage into a clean favorites list: array of
// plausible character names only, deduped, order preserved. Garbage (not an
// array, non-string entries, absurd names) degrades to [] / gets dropped --
// a corrupted favorites entry must never break the page.
export function parseFavs(raw: string | null): string[] {
  if (!raw) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: string[] = [];
  for (const v of parsed) {
    if (typeof v !== "string") continue;
    if (!/^[A-Za-z0-9_]{1,12}$/.test(v)) continue;
    if (out.includes(v)) continue;
    out.push(v);
  }
  return out;
}

export function toggleFav(favs: string[], name: string): string[] {
  return favs.includes(name) ? favs.filter((f) => f !== name) : [...favs, name];
}

// Favorites float to the top of the page, both groups keeping their
// server-side name order (stable partition, not a resort).
export function sortWithFavs(rows: BotRow[], favs: string[]): BotRow[] {
  const favSet = new Set(favs);
  return [...rows.filter((r) => favSet.has(r.name)), ...rows.filter((r) => !favSet.has(r.name))];
}

export interface PageInfo {
  page: number; // 1-based
  pages: number; // total pages (>= 1)
  hasPrev: boolean;
  hasNext: boolean;
}

export function pageInfo(total: number, limit: number, offset: number): PageInfo {
  const safeLimit = Math.max(1, limit);
  const pages = Math.max(1, Math.ceil(total / safeLimit));
  const page = Math.min(pages, Math.floor(offset / safeLimit) + 1);
  return { page, pages, hasPrev: offset > 0, hasNext: offset + safeLimit < total };
}

// A `type="number"` input bound with `bind:value` hands back "" (pristine),
// null (Svelte's numberlike read-back once the field is CLEARED), a number,
// or a numeric string. Only a real finite value becomes a search bound;
// every "no value" shape returns undefined so the CLI gets no filter at all.
// This is the fix for a cleared Max field turning into `<= 0` (zero results):
// `null === ""` is false, so the old pristine-only guard let null fall
// through to Math.max(0, ...) = 0.
export function levelFilter(v: unknown): number | undefined {
  if (v === "" || v === null || v === undefined) return undefined;
  const n = Number(v);
  if (!Number.isFinite(n)) return undefined;
  return Math.max(0, Math.floor(n));
}

// Set-level validity: a whole number in the server's 1..255 band. Tolerates
// the number | string | null the numberlike input hands back -- the old
// string-only helper called v.trim() and threw `v.trim is not a function`
// the moment a digit was typed, because bind:value had already coerced the
// value to a JS number.
export function levelValid(v: unknown): boolean {
  if (v === "" || v === null || v === undefined) return false;
  const n = Number(v);
  return Number.isInteger(n) && n >= 1 && n <= 255;
}

// --- storage-touching wrappers ---------------------------------------------

export function loadFavs(): string[] {
  try {
    return hasStorage() ? parseFavs(localStorage.getItem(FAVS_KEY)) : [];
  } catch {
    return [];
  }
}

export function saveFavs(favs: string[]): void {
  try {
    if (hasStorage()) localStorage.setItem(FAVS_KEY, JSON.stringify(favs));
  } catch {
    // Storage unavailable -- favorites just don't persist this session.
  }
}

// Small static id→name map for common zones (there is no zone-name table in
// the DB -- deliberately client-side, unknown ids fall back to "zone <id>").
const ZONE_NAMES: Record<number, string> = {
  1: "Dun Morogh",
  3: "Badlands",
  4: "Blasted Lands",
  8: "Swamp of Sorrows",
  10: "Duskwood",
  11: "Wetlands",
  12: "Elwynn Forest",
  14: "Durotar",
  15: "Dustwallow Marsh",
  17: "The Barrens",
  28: "Western Plaguelands",
  33: "Stranglethorn Vale",
  38: "Loch Modan",
  40: "Westfall",
  41: "Deadwind Pass",
  44: "Redridge Mountains",
  45: "Arathi Highlands",
  46: "Burning Steppes",
  47: "The Hinterlands",
  51: "Searing Gorge",
  85: "Tirisfal Glades",
  130: "Silverpine Forest",
  139: "Eastern Plaguelands",
  141: "Teldrassil",
  148: "Darkshore",
  215: "Mulgore",
  267: "Hillsbrad Foothills",
  331: "Ashenvale",
  357: "Feralas",
  361: "Felwood",
  400: "Thousand Needles",
  405: "Desolace",
  406: "Stonetalon Mountains",
  440: "Tanaris",
  490: "Un'Goro Crater",
  493: "Moonglade",
  618: "Winterspring",
  1377: "Silithus",
  1497: "Undercity",
  1519: "Stormwind City",
  1537: "Ironforge",
  1637: "Orgrimmar",
  1638: "Thunder Bluff",
  1657: "Darnassus",
  3430: "Eversong Woods",
  3433: "Ghostlands",
  3483: "Hellfire Peninsula",
  3487: "Silvermoon City",
  3518: "Nagrand",
  3519: "Terokkar Forest",
  3520: "Shadowmoon Valley",
  3521: "Zangarmarsh",
  3522: "Blade's Edge Mountains",
  3524: "Azuremyst Isle",
  3525: "Bloodmyst Isle",
  3557: "The Exodar",
  3703: "Shattrath City",
  3711: "Sholazar Basin",
  4197: "Wintergrasp",
  65: "Dragonblight",
  66: "Zul'Drak",
  67: "The Storm Peaks",
  210: "Icecrown",
  394: "Grizzly Hills",
  495: "Howling Fjord",
  3537: "Borean Tundra",
  2817: "Crystalsong Forest",
  4395: "Dalaran",
};

export function zoneName(id: number): string {
  return ZONE_NAMES[id] ?? `zone ${id}`;
}
