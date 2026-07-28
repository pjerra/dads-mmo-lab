// Session-cache mechanics shared by every page that renders item info.
//
// Item-info payloads embed base64 icons, so an app-session cache that only
// ever grows is a real memory problem (a long Bot Browser or Item Database
// session touches thousands of distinct entries). These are the two rules
// that keeps that honest, kept pure so they're testable without Svelte or
// Tauri: bound the cache, and never re-fetch what is already held.

/** Approximate-LRU ceiling for the icon-carrying info caches. */
export const MAX_INFO_CACHE = 800;

/**
 * Insert with approximate-LRU eviction: a Map preserves insertion order, so
 * the first key is the oldest. Overwriting an existing key never evicts.
 */
export function cacheSet<K, V>(cache: Map<K, V>, key: K, value: V, max = MAX_INFO_CACHE): void {
  if (cache.size >= max && !cache.has(key)) {
    const oldest = cache.keys().next().value;
    if (oldest !== undefined) cache.delete(oldest);
  }
  cache.set(key, value);
}

/**
 * The entries a caller still needs to fetch: cached ones dropped, repeats
 * collapsed, first-seen order kept so a partial/streamed result lands in a
 * predictable order.
 */
export function missingEntries<V>(cache: Map<number, V>, entries: number[]): number[] {
  const seen = new Set<number>();
  const out: number[] = [];
  for (const entry of entries) {
    if (cache.has(entry) || seen.has(entry)) continue;
    seen.add(entry);
    out.push(entry);
  }
  return out;
}
