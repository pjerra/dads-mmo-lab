// Which pages need a running server, and what to say when it isn't.
//
// DECIDED (user, 2026-07-21): pages that are useless while the server is
// stopped greet you with a message and a Start button rather than being
// greyed out in the sidebar -- grey-out shows no reason, looks broken, and
// blocks exploring what a page does.
//
// Kept pure (no Svelte, no Tauri) so the gate list and the messages are
// vitest-pinned: adding a page here changes user-visible behaviour.

import type { ServerVerdict } from "./api";

/**
 * Pages that cannot function with the server down (they need the world DB or
 * SOAP). Everything else stays usable stopped -- config edits and module
 * installs apply on next start, disk tools and Help never needed a server.
 */
const SERVER_REQUIRED_PAGES = new Set([
  "console",
  "dashboard",
  "teleport",
  "gmtools",
  "items",
  "party",
  "browse",
  "accounts",
]);

export function requiresServer(page: string): boolean {
  return SERVER_REQUIRED_PAGES.has(page);
}

export type GateKind = "stopped" | "crashed" | "starting" | "unreachable";

export interface GateState {
  kind: GateKind;
  title: string;
  body: string;
  /** Whether offering "Start server" is the right fix for this state. */
  canStart: boolean;
}

/**
 * The greeting to show over a server-requiring page, or null when the page
 * should render normally.
 *
 * A null verdict (no poll has landed yet) deliberately does NOT gate: it
 * would flash "start the server" on every cold start before the first poll.
 */
export function serverGate(verdict: ServerVerdict | null, restarting: boolean): GateState | null {
  if (verdict === "online") return null;
  if (verdict === null) return null;

  // A restart in flight outranks the polled verdict, which lags behind it --
  // otherwise the mid-restart "stopped" poll offers a Start button that would
  // collide with the restart already running.
  if (restarting || verdict === "starting") {
    return {
      kind: "starting",
      title: "The server is starting…",
      body: "This page needs the world server. It will work as soon as the server finishes coming up.",
      canStart: false,
    };
  }

  if (verdict === "soap_unreachable") {
    return {
      kind: "unreachable",
      title: "The server isn't responding",
      body: "The containers are running but the world server isn't answering. Try a restart from Home, or check the Console for what it's doing.",
      canStart: false,
    };
  }

  if (verdict === "crashed") {
    return {
      kind: "crashed",
      title: "The server crashed",
      body: "The world server stopped unexpectedly. Start it again below — the Console has the log if you want to see why.",
      canStart: true,
    };
  }

  return {
    kind: "stopped",
    title: "The server isn't running",
    body: "This page needs the world server to be up. Start it below, or from Home.",
    canStart: true,
  };
}
