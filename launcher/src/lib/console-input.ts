// Pure logic for the Console page (improvements Batch 3): shell-style command
// history recall and log-line severity classification. No Svelte/DOM/store
// access here so it's node-testable in isolation.

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
