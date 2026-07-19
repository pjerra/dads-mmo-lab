export function dirtyKeys(
  settings: { key: string; value: string }[],
  edits: Record<string, string>,
): string[] {
  return settings
    .filter((s) => edits[s.key] !== undefined && edits[s.key] !== s.value)
    .map((s) => s.key);
}

// Which feature-lock keys must be unlocked to save this dirty set (Batch 1).
// Conf-file rows (env column "conf:...") are a NEW save mechanism, gated
// separately from the long-tested env rows:
//   conf: rows targeting playerbots.conf  -> "bots-world"
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
    if (env.startsWith("conf:")) {
      flags.add(env.startsWith("conf:playerbots.conf:") ? "bots-world" : "rates-live");
    } else {
      flags.add("settings-save");
    }
  }
  return [...flags];
}
