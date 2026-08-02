// What the Home card and the Library row say about the Wrath Unbound add-on.
//
// It lived only on the Tools page, which is the one page you go to in order to
// CHANGE it — not the two you look at to find out what your server currently
// is. That gap is not cosmetic: a half-finished install is a real state a user
// can be left in (a closed launcher mid-rebuild does it), and until now the
// only way to discover it was to open the page you would only open if you
// already suspected.
//
// Pure on purpose. The pages fetch; this decides.

import type { UnboundStatus } from "./api";

export type BadgeTone = "ok" | "warn";

export interface UnboundBadge {
  text: string;
  tone: BadgeTone;
  /** Longer explanation for a `title=` tooltip. */
  detail: string;
}

/**
 * The badge for a server, or `null` when there is nothing honest to say.
 *
 * `null` covers two DIFFERENT situations deliberately, and neither deserves a
 * badge: the add-on is genuinely absent, and we could not find out (WSL mode,
 * where the command refuses; a failed probe; not fetched yet). Rendering
 * "not installed" for the second would be a claim nothing checked.
 */
export function unboundBadge(status: UnboundStatus | null | undefined): UnboundBadge | null {
  if (!status) return null;

  // A half-applied core patch OUTRANKS the phase. It is the condition that
  // makes install refuse and uninstall decline to reverse, so a server showing
  // "Installed" while carrying it would be telling the user the opposite of
  // what they need to act on.
  if (status.patch === "MIXED") {
    return {
      text: "Unbound: patch half-applied",
      tone: "warn",
      detail:
        "Some but not all six patched core files carry the change. Install and uninstall both refuse until the six are restored (git checkout -- <file>).",
    };
  }

  switch (status.phase) {
    case "installed":
      return {
        text: status.addon_version ? `Unbound ${status.addon_version}` : "Unbound installed",
        tone: "ok",
        detail: "The Wrath Unbound add-on is installed on this server.",
      };
    case "installing":
      return {
        text: "Unbound: install unfinished",
        tone: "warn",
        detail: status.next_stage
          ? `A previous install stopped at "${status.next_stage}". Tools → Wrath Unbound → Resume install continues from there.`
          : "A previous install did not finish. Tools → Wrath Unbound → Resume install continues from where it stopped.",
      };
    case "uninstalling":
      return {
        text: "Unbound: uninstall unfinished",
        tone: "warn",
        detail:
          "A previous uninstall did not finish. Tools → Wrath Unbound → Uninstall resumes it.",
      };
    default:
      // "absent", or a phase a newer engine invented. Say nothing rather than
      // guess -- the forward-compat rule this project applies to every union.
      return null;
  }
}
