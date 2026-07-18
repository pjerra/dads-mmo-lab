// Pure helpers for the Dashboard's Talents/Achievements cards (Round G:
// achievements-talents). Kept dependency-free so they're trivially
// vitest-able without mounting Svelte/Tauri machinery.

// Splits an id list into chunks of at most `size` -- entity-info caps each
// call at 25 ids server-side (cli/src/90-main.sh `entity-info)`), so a
// character with more than 25 distinct talent spells needs multiple
// sequential calls.
export function chunkIds(ids: number[], size = 25): number[][] {
  if (size <= 0) return ids.length === 0 ? [] : [ids.slice()];
  const out: number[][] = [];
  for (let i = 0; i < ids.length; i += size) {
    out.push(ids.slice(i, i + size));
  }
  return out;
}

// Formats a Unix epoch-seconds timestamp as a UTC "YYYY-MM-DD" string.
// character_achievement.date is 0 for "never" and the DB/CLI never
// validates it beyond "looks numeric", so this must degrade to "" for
// anything that isn't a genuine positive timestamp rather than rendering
// "1970-01-01" or "Invalid Date".
export function formatEpochDate(epoch: number): string {
  if (!Number.isFinite(epoch) || epoch <= 0) return "";
  const d = new Date(epoch * 1000);
  if (Number.isNaN(d.getTime())) return "";
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, "0");
  const day = String(d.getUTCDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}
