// WHICH restart actually lands a saved setting.
//
// A bare `restart_required: true` was not enough information, and the gap cost a
// real bug. `dml wow config set` writes a bind-mounted conf file, and it also
// REMOVES any legacy `AC_*` env key that was shadowing that row (env beats conf
// in AzerothCore, so leaving it would make the save a silent no-op). But a
// container's environment is fixed when it is CREATED: `docker restart` hands
// the same container back with the same env. So a save that removed a shadowing
// env key needs the container RECREATED -- the down->up that `games restart`
// performs -- and the user who reached for the fast world-only restart instead
// watched their change revert (found live on the Bot World page, 2026-07-29).
//
// The CLI now answers the question directly (`apply_needed` in the `config set`
// envelope, on both the bash and the native surface). This module is the pure
// half of acting on it: rank the answers, escalate across a multi-row save, and
// own the user-facing copy so the two banners and Home cannot drift.

export type ApplyNeeded = "none" | "world-restart" | "recreate";

/// Strength order. Escalation is by rank, never by recency: a save that touches
/// three rows needs the STRONGEST apply any one of them asked for.
const RANK: Record<ApplyNeeded, number> = {
  none: 0,
  "world-restart": 1,
  recreate: 2,
};

export function escalate(current: ApplyNeeded, incoming: ApplyNeeded): ApplyNeeded {
  return RANK[incoming] > RANK[current] ? incoming : current;
}

/// Read `apply_needed` off a `config set` result.
///
/// FAILS SAFE, not open: an older `dml` in the distro (or any envelope that
/// reports `restart_required` without saying which restart) resolves to
/// `recreate`, the stronger action. Guessing the WEAKER one is exactly the bug
/// this field exists to fix -- a recreate always applies the change, while a
/// world-restart may silently not. The cost of over-doing it is a slower
/// restart, which is visible and honest; the cost of under-doing it is a
/// setting that appears to revert.
export function normalizeApplyNeeded(r: {
  restart_required?: boolean;
  apply_needed?: string;
  applied?: string;
}): ApplyNeeded {
  if (r.apply_needed === "none") return "none";
  if (r.apply_needed === "world-restart") return "world-restart";
  if (r.apply_needed === "recreate") return "recreate";
  // Unknown or absent: trust `restart_required` for WHETHER, assume the
  // stronger answer for WHICH.
  return r.restart_required ? "recreate" : "none";
}

/// Banner copy. Names the button the user must press, because "restart the
/// server" was ambiguous precisely where it mattered -- Home offers two
/// restarts and only one of them can apply a recreate.
export function bannerText(apply: ApplyNeeded): string {
  switch (apply) {
    case "recreate":
      return "Saved — use Restart on the Home page to apply this. The fast world-only restart cannot: it reuses the same container, and this change needs a fresh one.";
    case "world-restart":
      return "Saved — restart the server to apply the changes. The fast world-only restart on Home is enough for this one.";
    default:
      return "";
  }
}

/// Why the fast world-only restart is refused while a recreate is pending.
/// Empty when it is safe to offer.
export function fastRestartBlockedReason(apply: ApplyNeeded): string {
  return apply === "recreate"
    ? "A saved setting needs a full Restart — the fast world-only restart reuses the running container, which keeps its old settings."
    : "";
}
