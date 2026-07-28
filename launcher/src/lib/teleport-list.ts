// What the Teleport location list should say when it has no rows to show.
//
// The list is an independent direct-MySQL read, so it can fail while the
// server verdict is "online" and the page is therefore past the ServerRequired
// gate. A failed read leaves the previous locations in place -- on the first
// load that is the initial empty array, which is why the naive
// "length === 0 -> no matches" branch blamed the filter for a DB error.
//
// Same shape as Statistics.svelte's error / stopped / stale-with-last-values
// split, kept pure so the wording is vitest-pinned.

export type LocListNotice =
  | { kind: "loading"; text: string }
  | { kind: "failed"; text: string }
  | { kind: "empty"; text: string }
  | { kind: "stale"; text: string }
  | null;

/**
 * The notice for the current list state, or null when the list speaks for
 * itself. `stale` is the one kind that renders *alongside* the rows; the
 * others replace them.
 *
 * `loadFailed` tracks the last load() outcome specifically -- not the page's
 * general `error`, which a failed teleport action also sets and which would
 * otherwise mislabel a genuinely empty filter result as a load failure.
 */
export function locListNotice(loading: boolean, loadFailed: boolean, count: number): LocListNotice {
  if (loading) return { kind: "loading", text: "Loading locations…" };
  if (loadFailed) {
    // The detail lives in the error card above; these only say which of the
    // two failure shapes it is.
    return count === 0
      ? { kind: "failed", text: "Couldn't load locations." }
      : { kind: "stale", text: "Refresh failed — showing the locations from the last successful load." };
  }
  return count === 0 ? { kind: "empty", text: "No locations match the filter." } : null;
}
