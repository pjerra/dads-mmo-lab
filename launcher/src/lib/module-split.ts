// Pure helpers for the Modules page's installed/available split (Modules-page
// round, Task 3). No Svelte imports -- keeps this vitest-testable without a
// component harness.

export type LuaStatus = "installed" | "cloned" | "absent";

// deployed implies installed regardless of cloned (a lua module can be
// deployed from a state where its clone was later removed/replaced).
export function luaStatus(m: { cloned: boolean; deployed: boolean }): LuaStatus {
  if (m.deployed) return "installed";
  if (m.cloned) return "cloned";
  return "absent";
}

export function luaStatusLabel(s: LuaStatus): string {
  switch (s) {
    case "installed": return "Installed";
    case "cloned": return "Cloned, not deployed";
    case "absent": return "Not installed";
  }
}

// Splits rows into installed/available while preserving each half's original
// relative order -- a single pass, no sort.
export function splitInstalled<T>(
  rows: T[],
  isInstalled: (m: T) => boolean,
): { installed: T[]; available: T[] } {
  const installed: T[] = [];
  const available: T[] = [];
  for (const row of rows) {
    (isInstalled(row) ? installed : available).push(row);
  }
  return { installed, available };
}

// The "Available" catalog starts expanded only on a fresh server that has
// nothing installed yet -- otherwise it starts collapsed so the page reads
// installed-first.
export function defaultExpanded(installedCount: number): boolean {
  return installedCount === 0;
}
