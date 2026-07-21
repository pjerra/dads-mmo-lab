// Persistent selected-character store (Batch 3 F12), following the
// server-status.svelte.ts pattern: module-level runes state + pure helpers
// exported for vitest. The selection survives app restarts via localStorage
// (guarded access, features.svelte.ts idiom) and is shared by every
// CharPicker plus the sidebar "playing as" chip.

import type { Account, CharacterSummary } from "./api";

export interface SelectedChar {
  guid: number;
  name: string;
  account: string;
}

const KEY = "dml.selectedChar";

// Pure: validate a raw localStorage payload into a SelectedChar (or null).
// Anything malformed -- old formats, hand-edited storage, wrong types --
// degrades to null instead of a crash at module-init time.
export function parseStoredChar(raw: string | null): SelectedChar | null {
  if (!raw) return null;
  try {
    const v = JSON.parse(raw) as Partial<SelectedChar> | null;
    if (
      v &&
      typeof v.guid === "number" &&
      Number.isFinite(v.guid) &&
      typeof v.name === "string" &&
      v.name.length > 0 &&
      typeof v.account === "string" &&
      v.account.length > 0
    ) {
      return { guid: v.guid, name: v.name, account: v.account };
    }
    return null;
  } catch {
    return null;
  }
}

// Pure: does the freshly-fetched account list still contain the stored
// character? Matched by guid (names can be freed and re-taken; guids are
// stable) and confirmed against the stored account. A deleted character or
// account returns null -- callers then fall back to their own default.
export function findStoredChar(
  accounts: Account[],
  stored: SelectedChar | null,
): { account: string; char: CharacterSummary } | null {
  if (!stored) return null;
  const acc = accounts.find((a) => a.username === stored.account);
  if (!acc) return null;
  const char = acc.characters.find((c) => c.guid === stored.guid);
  if (!char) return null;
  return { account: acc.username, char };
}

function readStored(): SelectedChar | null {
  try {
    if (typeof localStorage === "undefined") return null;
    return parseStoredChar(localStorage.getItem(KEY));
  } catch {
    return null;
  }
}

export const charStore = $state({
  selected: readStored() as SelectedChar | null,
});

// Cross-page "open the full Character view for <name>" request (smoke
// item 4b), following the chipStart pattern in server-status.svelte.ts:
// Bot Browser sets it, the shell route navigates to the Character page,
// Dashboard adopts the name into charName (its auto-load effect then
// fetches) and clears the request. NOT persisted -- purely an in-flight
// navigation signal.
export const charView = $state({ requestedName: null as string | null });

export function requestCharView(name: string): void {
  charView.requestedName = name;
}

export function setSelectedChar(sel: SelectedChar | null): void {
  charStore.selected = sel;
  try {
    if (typeof localStorage !== "undefined") {
      if (sel) localStorage.setItem(KEY, JSON.stringify(sel));
      else localStorage.removeItem(KEY);
    }
  } catch {
    // In-memory selection still applies this session.
  }
}
