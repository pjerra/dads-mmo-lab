// Bot World all-keys browser logic (Batch 1 F2), kept pure for unit tests.
// The Config page's "Bot World" tab feeds `wow config pb-keys` rows through
// these: a case-insensitive key search and a staged-edits diff that becomes
// one `config set conf:playerbots.conf:<Key>` call per changed key.

export interface PbKeyRow {
  key: string;
  value: string;
  default: string | null;
  line: number;
}

// Case-insensitive substring match on the KEY. An empty/whitespace query
// returns the full list unchanged.
export function filterPbKeys<T extends { key: string }>(keys: T[], query: string): T[] {
  const q = query.trim().toLowerCase();
  if (!q) return keys;
  return keys.filter((k) => k.key.toLowerCase().includes(q));
}

// Staged edits -> the writes Save will perform. Only keys that exist in the
// parsed list AND whose edit differs from the current value count; edit
// order is preserved so saves run in the order the user typed them.
export function stagedPbChanges(
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
