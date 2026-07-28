// Rules for a typed mail recipient.
//
// The Item Database lets you type a character name instead of only picking one
// from the account dropdowns, which moves the common failure from "impossible"
// to "typo". Two things follow: the name must satisfy the same shape every
// action verb enforces (an unfiltered name fails later as an opaque BAD_ARG),
// and a well-formed name the launcher doesn't recognise must still be
// sendable -- bots and freshly-created characters are legitimately missing
// from the account list -- but flagged, because mail to a nonexistent
// character is silently lost in-game.

/** The shape every `dml wow` action verb enforces on a character name. */
const ACTIONABLE_NAME = /^[A-Za-z0-9_]{1,12}$/;

export function isValidCharName(name: string): boolean {
  return ACTIONABLE_NAME.test(name);
}

export type RecipientStatus = "empty" | "invalid" | "unknown" | "known";

/**
 * How the page should treat what's currently typed. `known` names match a
 * character the launcher has listed (case-insensitively -- the DB stores one
 * canonical casing and the server matches case-insensitively anyway).
 */
export function recipientStatus(typed: string, knownNames: string[]): RecipientStatus {
  const name = typed.trim();
  if (name === "") return "empty";
  if (!isValidCharName(name)) return "invalid";
  const lower = name.toLowerCase();
  return knownNames.some((n) => n.toLowerCase() === lower) ? "known" : "unknown";
}

/** Whether a send may be attempted at all. Unknown-but-well-formed is allowed. */
export function canSendTo(typed: string, knownNames: string[]): boolean {
  const status = recipientStatus(typed, knownNames);
  return status === "known" || status === "unknown";
}
