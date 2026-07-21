// Pure helpers for the Statistics page. Kept dependency-free (no big-JSON
// import here) so vitest can exercise them with hand-made fixtures -- the
// page passes the zone-names lookup in (same split as achievements.ts /
// talent-trees.ts).

import type {
  StatsClassCount,
  StatsFactionSplit,
  StatsLevelBucket,
  StatsRich,
  StatsSegmented,
  StatsTopLevel,
} from "./api";

// --- All|Family|Bots segment filter (smoke item 7) -------------------------
// The CLI ships segment-sensitive stats pre-split; "all" merges client-side.

export type StatsSegment = "all" | "family" | "bots";

/** Class breakdown for a segment; "all" sums the two lists per class id. */
export function pickClasses(
  segment: StatsSegment,
  classes: StatsSegmented<StatsClassCount[]>,
): StatsClassCount[] {
  if (segment === "family") return classes.family;
  if (segment === "bots") return classes.bots;
  const byId = new Map<number, number>();
  for (const c of [...classes.family, ...classes.bots]) {
    byId.set(c.class, (byId.get(c.class) ?? 0) + c.count);
  }
  return [...byId.entries()]
    .map(([cls, count]) => ({ class: cls, count }))
    .sort((a, b) => a.class - b.class);
}

/** Faction split for a segment; "all" adds the two splits. */
export function pickFactions(
  segment: StatsSegment,
  factions: StatsSegmented<StatsFactionSplit>,
): StatsFactionSplit {
  if (segment === "family") return factions.family;
  if (segment === "bots") return factions.bots;
  return {
    alliance: factions.family.alliance + factions.bots.alliance,
    horde: factions.family.horde + factions.bots.horde,
  };
}

/** Top levels for a segment; "all" merges the two top-5s and re-takes 5. */
export function pickTopLevels(
  segment: StatsSegment,
  tops: StatsSegmented<StatsTopLevel[]>,
): StatsTopLevel[] {
  if (segment === "family") return tops.family;
  if (segment === "bots") return tops.bots;
  return [...tops.family, ...tops.bots]
    .sort((a, b) => b.level - a.level || a.name.localeCompare(b.name))
    .slice(0, 5);
}

/** Richest for a segment; "all" merges the two top-5s and re-takes 5. */
export function pickRichest(
  segment: StatsSegment,
  rich: StatsSegmented<StatsRich[]>,
): StatsRich[] {
  if (segment === "family") return rich.family;
  if (segment === "bots") return rich.bots;
  return [...rich.family, ...rich.bots]
    .sort((a, b) => b.copper - a.copper || a.name.localeCompare(b.name))
    .slice(0, 5);
}

/** Per-bucket chart value for the active segment ("all" stacks both). */
export function bucketValue(segment: StatsSegment, l: StatsLevelBucket): number {
  if (segment === "family") return l.family;
  if (segment === "bots") return l.bots;
  return l.family + l.bots;
}

/** Copper -> whole gold with thousands separators: 1211292125 -> "121,129g". */
export function formatGold(copper: number): string {
  const gold = Math.floor(Math.max(0, copper) / 10000);
  return `${gold.toLocaleString("en-US")}g`;
}

/**
 * Humanized playtime: minutes under an hour, one-decimal hours under two
 * days (the "17.7h" the family is used to from /played), one-decimal days
 * beyond that.
 */
export function formatPlaytime(seconds: number): string {
  const s = Math.max(0, seconds);
  if (s < 3600) return `${Math.round(s / 60)}m`;
  if (s < 48 * 3600) return `${(s / 3600).toFixed(1)}h`;
  return `${(s / 86400).toFixed(1)}d`;
}

/** Combined bot playtime reads best as years: 562221203 -> "17.8 years". */
export function formatYears(seconds: number): string {
  const years = Math.max(0, seconds) / 31_557_600; // Julian year
  return `${years.toFixed(1)} years`;
}

/**
 * "Last seen" from a unix logout_time. 0 means the character has never
 * saved a logout (brand new) -> "never". `now` is injectable for tests.
 */
export function formatLastSeen(unix: number, now: number = Date.now() / 1000): string {
  if (unix <= 0) return "never";
  const diff = Math.max(0, now - unix);
  if (diff < 120) return "just now";
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
  if (diff < 30 * 86400) return `${Math.floor(diff / 86400)}d ago`;
  return new Date(unix * 1000).toLocaleDateString("en-US", {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/** Boot start (unix seconds) -> a short chart label like "Jul 20". */
export function formatBootDate(unix: number): string {
  return new Date(unix * 1000).toLocaleDateString("en-US", { month: "short", day: "numeric" });
}

/** Bucket 0 -> "1-10", bucket 7 -> "71-80". */
export function levelBucketLabel(bucket: number): string {
  return `${bucket * 10 + 1}-${bucket * 10 + 10}`;
}

/**
 * The CLI only emits buckets that have characters; the chart wants a fixed
 * 1-80 axis. Fill 0..7 with zeros, keep any unexpected higher buckets
 * (defensive -- stock WotLK caps at 80) appended in order.
 */
export function fillLevelBuckets(rows: StatsLevelBucket[]): StatsLevelBucket[] {
  const byBucket = new Map(rows.map((r) => [r.bucket, r]));
  const out: StatsLevelBucket[] = [];
  for (let b = 0; b <= 7; b++) {
    out.push(byBucket.get(b) ?? { bucket: b, family: 0, bots: 0 });
  }
  const extras = rows.filter((r) => r.bucket > 7).sort((a, b) => a.bucket - b.bucket);
  return [...out, ...extras];
}

/** Bar-chart width: part as a 0-100 percentage of total (0 when total is 0). */
export function pct(part: number, total: number): number {
  if (total <= 0) return 0;
  return Math.min(100, Math.max(0, (part / total) * 100));
}

/** Zone id -> name via the generated lookup; unknown ids stay readable. */
export function zoneName(id: number, names: Record<string, string>): string {
  return names[String(id)] ?? `Zone ${id}`;
}

const CONTINENT_NAMES: Record<number, string> = {
  0: "Eastern Kingdoms",
  1: "Kalimdor",
  530: "Outland",
  571: "Northrend",
};

/** Map id -> continent name; instances/others stay readable. */
export function continentName(map: number): string {
  return CONTINENT_NAMES[map] ?? `Map ${map}`;
}

/** Guild average size, one decimal ("15.0" for 300/20); "0" when no guilds. */
export function avgGuildSize(members: number, guilds: number): string {
  if (guilds <= 0) return "0";
  return (members / guilds).toFixed(1);
}
