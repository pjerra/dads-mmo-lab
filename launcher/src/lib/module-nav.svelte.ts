// One-shot cross-tab navigation targets (Modules-page round, Task 4): a click
// on an installed module's name/conf filename on the Modules tab sets a
// pending target here, then switches the co-mounted `tab` state to
// "tuning"/"files". Those tabs stay MOUNTED (display:none) the whole time
// (module-update round), so this can't be component props -- it has to be
// module-scope runes state, same shape as char-store.svelte.ts's charView
// request/consume pattern. take*() returns the value AND clears it, so a
// later re-activation of the same tab (e.g. the user just clicking the tab
// strip afterwards) doesn't re-trigger the expand+scroll or file-select.
export const moduleNav = $state({
  tuningKey: null as string | null,
  confFile: null as string | null,
});

export function requestTuning(key: string): void {
  moduleNav.tuningKey = key;
}

export function requestConfFile(name: string): void {
  moduleNav.confFile = name;
}

export function takeTuning(): string | null {
  const k = moduleNav.tuningKey;
  moduleNav.tuningKey = null;
  return k;
}

export function takeConfFile(): string | null {
  const f = moduleNav.confFile;
  moduleNav.confFile = null;
  return f;
}
