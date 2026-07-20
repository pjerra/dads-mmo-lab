// Pure logic for the Console page (improvements Batch 3): shell-style command
// history recall, log-line severity classification, and command autocomplete.
// No Svelte/DOM/store access here so it's node-testable in isolation.

import type { CoreCommand } from "./gm-commands";

// --- History recall (F2) ---------------------------------------------------
// `history` is the list of prior commands, oldest first. `cursor` is the
// currently recalled index, or null when the input holds the live draft.
// `draft` is the text to restore when the user walks back down past the newest
// entry. Up walks toward older commands; Down walks toward newer, then returns
// to the draft. Mirrors bash/PowerShell arrow-key recall.
export interface RecallResult {
  value: string;
  cursor: number | null;
}

export function recallHistory(
  history: string[],
  cursor: number | null,
  dir: "up" | "down",
  draft: string,
): RecallResult {
  if (history.length === 0) return { value: draft, cursor: null };
  if (dir === "up") {
    // From the draft, Up jumps to the newest command; otherwise step older,
    // clamping at the oldest (index 0).
    const next = cursor === null ? history.length - 1 : Math.max(0, cursor - 1);
    return { value: history[next], cursor: next };
  }
  // dir === "down"
  if (cursor === null) return { value: draft, cursor: null };
  const next = cursor + 1;
  if (next >= history.length) return { value: draft, cursor: null };
  return { value: history[next], cursor: next };
}

// --- Log severity coloring (F2) --------------------------------------------
// Classify a raw worldserver log line by the level marker it prints. Matched
// case-sensitively against the upper-case tokens the server emits, so ordinary
// chat/log text containing the word "error" in prose isn't miscoloured.
export type LogSeverity = "error" | "warn" | "normal";

export function logSeverity(line: string): LogSeverity {
  if (/\b(?:ERROR|FATAL)\b/.test(line)) return "error";
  if (/\b(?:WARN|WARNING)\b/.test(line)) return "warn";
  return "normal";
}

// --- Command autocomplete (F3) ---------------------------------------------
// Derive dot-less, placeholder-free command stems from the GM cheat-sheet
// catalog. The Console sends commands without the leading dot, so ".tele
// <place>" -> "tele" and ".gm on / .gm off" -> ["gm on", "gm off"].
export function consoleCommands(catalog: CoreCommand[]): string[] {
  const out = new Set<string>();
  for (const c of catalog) {
    for (const part of c.cmd.split("/")) {
      let stem = part.trim();
      const lt = stem.indexOf("<");
      if (lt >= 0) stem = stem.slice(0, lt).trim();
      stem = stem.replace(/^\./, "").trim();
      if (stem) out.add(stem);
    }
  }
  return [...out];
}

// Prefix-match the input against a pool of candidate commands (catalog stems
// plus the user's favorites). Case-insensitive, de-duplicated, exact matches
// dropped (nothing left to complete), capped so the dropdown stays small.
export function commandSuggestions(pool: string[], input: string, cap = 8): string[] {
  const q = input.trim().toLowerCase();
  if (!q) return [];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const c of pool) {
    const cl = c.trim().toLowerCase();
    if (!cl || seen.has(cl)) continue;
    seen.add(cl);
    if (cl.startsWith(q) && cl !== q) {
      out.push(c.trim());
      if (out.length >= cap) break;
    }
  }
  return out;
}
