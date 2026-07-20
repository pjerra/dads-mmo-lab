export function dirtyKeys(
  settings: { key: string; value: string }[],
  edits: Record<string, string>,
): string[] {
  return settings
    .filter((s) => edits[s.key] !== undefined && edits[s.key] !== s.value)
    .map((s) => s.key);
}

// Per-tab Save scoping (improvements Batch 2): the curated-row tabs
// (Settings / Bot World / Auction House) share ONE settings+edits map, so a
// Save button must act only on the rows the CURRENT tab actually shows.
// Restrict the settings list to the currently-visible groups before computing
// dirty/toSave/saveLocked -- otherwise Save on one tab writes another tab's
// dirty rows, and saveLocked leaks a locked flag from a tab you can't see.
export function settingsInGroups<T extends { group: string }>(
  settings: T[],
  groups: string[],
): T[] {
  const set = new Set(groups);
  return settings.filter((s) => set.has(s.group));
}

// Which feature-lock keys must be unlocked to save this dirty set (Batch 1).
// Conf-file rows (env column "conf:...") are a NEW save mechanism, gated
// separately from the long-tested env rows:
//   conf: rows targeting playerbots.conf  -> "bots-world"
//   conf: rows targeting mod_ahbot.conf   -> "ahbot-page" (Batch 4 F14)
//   every other conf: row (worldserver)   -> "rates-live"
//   env rows / motd (original mechanism)  -> "settings-save"
export function requiredSaveFlags(
  settings: { key: string; env: string }[],
  dirty: string[],
): string[] {
  const byKey = new Map(settings.map((s) => [s.key, s.env]));
  const flags = new Set<string>();
  for (const k of dirty) {
    const env = byKey.get(k);
    if (env === undefined) continue;
    if (env.startsWith("conf:playerbots.conf:")) {
      flags.add("bots-world");
    } else if (env.startsWith("conf:mod_ahbot.conf:")) {
      flags.add("ahbot-page");
    } else if (env.startsWith("conf:")) {
      flags.add("rates-live");
    } else {
      flags.add("settings-save");
    }
  }
  return [...flags];
}
