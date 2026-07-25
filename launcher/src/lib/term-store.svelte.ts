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

// `clearAnchor`: the last few log lines visible when the user hit Clear.
// The Console's log view is a server-side tail snapshot that refills every
// poll, so "clear" is implemented as "only show lines newer than this
// anchor" (see tailAfterAnchor) -- kept in the store so a cleared console
// stays cleared across page switches.
export const consoleStore = $state({
  hist: [] as ConsoleHistEntry[],
  clearAnchor: null as string[] | null,
});

// The reply-history sidecar is module-level (survives navigation) and only
// ever appended -- a long console session would grow it without bound (the
// visible log pane is a bounded server tail, but this list is not). Cap it to
// the most recent MAX_CONSOLE_HIST entries on push.
export const MAX_CONSOLE_HIST = 300;
export function pushConsoleHist(entry: ConsoleHistEntry): void {
  const next = [...consoleStore.hist, entry];
  consoleStore.hist =
    next.length > MAX_CONSOLE_HIST ? next.slice(-MAX_CONSOLE_HIST) : next;
}

// Pure: the portion of a freshly-fetched tail that comes AFTER the last
// occurrence of the anchor line-sequence. [] means "anchor found, nothing
// new yet"; null means "anchor no longer in the window" (enough new output
// scrolled it away -- the caller should show everything and drop the
// anchor). Multi-line anchors exist because single log lines repeat (AHBot
// spams identical cycle lines).
export function tailAfterAnchor(fetched: string[], anchor: string[]): string[] | null {
  if (anchor.length === 0) return null;
  for (let start = fetched.length - anchor.length; start >= 0; start--) {
    let match = true;
    for (let j = 0; j < anchor.length; j++) {
      if (fetched[start + j] !== anchor[j]) {
        match = false;
        break;
      }
    }
    if (match) return fetched.slice(start + anchor.length);
  }
  return null;
}

// Home for an in-flight/finished games_install session -- see Library.svelte
// and InstallTerminal.svelte. `id`/`running`/`nonce` gate whether the panel
// renders at all (Library.svelte gated on local $state before, which nav-away
// destroyed even though the backend's install session was still alive).
// `exitCode` isn't part of the panel's *gating* but is needed so a remounted
// InstallTerminal (nav-away-and-back after the session already finished) can
// still render the correct ok/err exit styling instead of guessing.
export interface InstallStoreState {
  text: string;
  id: string | null;
  running: boolean;
  nonce: number;
  exitCode: number | null;
}

export const installStore = $state<InstallStoreState>({
  text: "",
  id: null,
  running: false,
  nonce: 0,
  exitCode: null,
});

// The backend (src-tauri/src/lib.rs games_install) tracks exactly ONE global
// install slot and rejects a second concurrent invoke with a BUSY error.
// installStore.nonce is bumped once per fresh install (Library's
// startInstall()) and used to key InstallTerminal's {#key} block, so
// nav-away-and-back remounts a fresh component instance against the SAME
// nonce -- if that remounted instance invoked games_install again it would
// hit BUSY and (pre-fix) falsely flip a still-running session to "exited".
// claimInstallInvoke lets only the FIRST instance to see a given nonce
// actually call games_install; later remounts against that nonce just
// observe installStore reactively. Plain module state (not $state -- nothing
// renders it directly), so it survives InstallTerminal's destroy/recreate
// the same way installStore does.
let invokedNonce: number | null = null;
export function claimInstallInvoke(nonce: number): boolean {
  if (invokedNonce === nonce) return false;
  invokedNonce = nonce;
  return true;
}
