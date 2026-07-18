// Page components (Console.svelte, the per-title terminal on Library/Home/
// ModuleManager-style pages, ...) are destroyed whenever the user navigates
// the sidebar, but the Tauri Channel streams they kicked off (games_start,
// module install/rebuild, backup create, ...) keep firing into whatever
// local $state they were bound to -- once the page unmounts, those events
// land on orphaned state and are lost. This module is the surviving home for
// terminal transcripts: module-level runes state (same pattern as
// restart-state.svelte.ts) so a stream's output is still there when the user
// navigates back to the page that started it.

import { initialTermState, type TermState } from "./terminal-state";

export interface TermBuf {
  term: TermState;
  show: boolean;
}

export interface ConsoleHistEntry {
  command: string;
  result: string | null;
  error: string | null;
}

const store = $state({ bufs: {} as Record<string, TermBuf> });

// Lazily create the buffer for `key`; repeat calls with the same key return
// the same object (so callers can hold onto it across renders).
// MUST return by re-reading store.bufs[key]: Svelte's $state set trap stores
// a NEW proxy around the assigned object, so returning the local pre-proxy
// variable on the creation path would hand the first caller a non-reactive
// plain object -- its template reads would never subscribe, and the terminal
// would silently never appear on that key's first use.
export function termBuf(key: string): TermBuf {
  if (!store.bufs[key]) {
    store.bufs[key] = { term: initialTermState(), show: false };
  }
  return store.bufs[key];
}

export function beginRun(key: string): TermBuf {
  const buf = termBuf(key);
  buf.term = initialTermState();
  buf.show = true;
  return buf;
}

export function clearBuf(key: string): void {
  const buf = termBuf(key);
  buf.term = initialTermState();
  buf.show = false;
}

// Pure flattener for downloads -- no store access, so it's node-testable in
// isolation from the runes state above.
export function termText(t: TermState): string {
  return t.sections
    .map((sec) => [`== ${sec.name} ==`, ...sec.lines.map((l) => l.text)].join("\n"))
    .join("\n\n");
}

export const consoleStore = $state({ hist: [] as ConsoleHistEntry[] });
export const installStore = $state({ text: "" });
