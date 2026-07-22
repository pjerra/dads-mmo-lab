// Shared conf-key browser logic (Module-tuning rework), kept pure for unit
// tests. Generalizes the Bot World pb-keys helpers to ANY module conf: the
// Config page's "Module tuning" tab feeds `wow config conf-keys --file <name>`
// rows through these (one browser per installed C++ module), and the Bot
// World tab keeps using the same functions via its pb-keys re-exports.

export interface ConfKeyRow {
  key: string;
  value: string;
  default: string | null;
  line: number;
  // Comment-block help parsed from the conf's .dist by the CLI ("" when the
  // module author documented nothing near the key).
  help?: string;
}

// Case-insensitive substring match on the KEY. An empty/whitespace query
// returns the full list unchanged.
export function filterConfKeys<T extends { key: string }>(keys: T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return keys;
  return keys.filter((k) => k.key.toLowerCase().includes(q));
}

// Staged edits -> the writes Save will perform. Only keys that exist in the
// parsed list AND whose edit differs from the current value count; edit
// order is preserved so saves run in the order the user typed them.
export function stagedConfChanges(
  keys: { key: string; value: string }[],
  edits: Record<string, string>,
): { key: string; value: string }[] {
  const byKey = new Map(keys.map((k) => [k.key, k.value]));
  const out: { key: string; value: string }[] = [];
  for (const [key, value] of Object.entries(edits)) {
    const cur = byKey.get(key);
    if (cur !== undefined && value !== cur) out.push({ key, value });
  }
  return out;
}

// One "Server modules" card per INSTALLED C++ module whose conf actually
// passes the CLI's editable-conf allowlist. `files` is the `wow config files`
// answer (every *.conf under env/dist/etc/modules, present as the live conf
// or its .dist) -- a module whose conf only exists inside its source checkout
// (state "ready"/"needs-rebuild") has nothing conf-keys could parse yet, so
// it gets no card. Registry order is preserved.
export interface ConfModuleCard {
  key: string; // module key (mod-transmog)
  name: string; // display name
  desc: string;
  conf: string; // conf basename (transmog.conf)
}
export function installedConfModules(
  cpp: { key: string; name: string; desc: string; installed: boolean; conf_name?: string | null }[],
  files: { name: string }[],
): ConfModuleCard[] {
  const present = new Set(files.map((f) => f.name));
  const out: ConfModuleCard[] = [];
  for (const m of cpp) {
    if (!m.installed || !m.conf_name) continue;
    if (!present.has(m.conf_name)) continue;
    out.push({ key: m.key, name: m.name, desc: m.desc, conf: m.conf_name });
  }
  return out;
}

// Tooltip/hint text for one browser row: the .dist help first, then the
// default so the tooltip is never empty when either exists.
export function confKeyHint(row: { help?: string; default: string | null }): string {
  const parts: string[] = [];
  if (row.help) parts.push(row.help);
  if (row.default !== null) parts.push(`Default: ${row.default}`);
  return parts.join(" — ");
}
