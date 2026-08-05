// WSL keep-alive: the ONE place the UI learns that the thing holding the
// server's distro open has stopped holding it.
//
// The whole failure mode this guards is SILENCE. On the Arch backend WSL powers
// the distro off ~15 s after the last session into it exits, and when that
// happens the server goes down cleanly, with `restart: unless-stopped` bringing
// it back the next time the user opens the launcher -- so from the user's chair
// the server is up whenever they look and down whenever their friends do. If
// Rust cannot establish the holder, that has to be visible.
//
// Rust owns the holding. This file owns nothing but the telling: it renders
// what `wsl_keepalive_status` reports and what the `wsl-keepalive` event
// announces. Deliberately NOT a poller -- see the module comment in
// `wsl_keepalive.rs` for why a webview timer must never be load-bearing here.

import { listen } from "@tauri-apps/api/event";
import { wslKeepaliveStatus, type KeepaliveReport } from "$lib/api";

export type KeepaliveEvent = { kind: string; message: string };

/// Module-level (not component-local) so it survives sidebar navigation, the
/// same reason `restart-state.svelte.ts` and `install-progress.svelte.ts` are.
export const keepaliveState = $state<{ warning: string | null; dismissed: boolean }>({
  warning: null,
  dismissed: false,
});

/// PURE. What, if anything, the user needs to be told about a report.
///
/// `applies:false` is not "everything is fine" -- it is "this backend has no
/// keep-alive", and rendering reassurance from it would be a claim about a
/// mechanism that is not running. Both answer `null`, but for different
/// reasons, and the distinction is why this takes the whole report rather than
/// a boolean.
///
/// THE WAY OUT IS NAMED. A give-up latches until the user asks again, and
/// `want_running` -- which `games_start`/`games_restart` call -- is what reopens
/// the budget. A banner that reports the failure and its consequence but not the
/// one action that fixes it leaves the user staring at a dead server with no
/// visible move; that was the whole reason the recovery path went unnoticed as
/// broken in the first place.
const RECOVERY = "Press Start (or Restart) on the Home page to try again.";

export function keepaliveWarning(r: KeepaliveReport | null): string | null {
  if (!r || !r.applies) return null;
  if (!r.gave_up) return null;
  const why = r.last_error?.trim();
  const cause = why
    ? `The launcher could not keep the WSL distro open: ${why}`
    : "The launcher could not keep the WSL distro open.";
  return `${cause} Your server will stop about 15 seconds after the last connection to it closes. ${RECOVERY}`;
}

/// Read the live report AND subscribe to the announcement. Both, on purpose.
///
/// The event alone is not enough: this store is module-level, a webview reload
/// wipes it, and a reloaded UI would then show nothing on exactly the path
/// where the keep-alive had already failed and the banner was the only thing
/// left saying so. That is the recorded soap-autosetup lesson -- there is no
/// contentless "already told you". The mount read re-derives the real verdict
/// from Rust, which never forgot it.
export function initKeepaliveWatch(): void {
  void (async () => {
    try {
      keepaliveState.warning = keepaliveWarning(await wslKeepaliveStatus());
    } catch {
      // A launcher build without the command is not a reason to break the shell.
    }
  })();
  void listen<KeepaliveEvent>("wsl-keepalive", () => {
    // Re-read rather than trusting the payload: the report is the state, the
    // event is only the nudge, and re-deriving keeps one source of truth.
    void (async () => {
      try {
        const next = keepaliveWarning(await wslKeepaliveStatus());
        if (next && next !== keepaliveState.warning) keepaliveState.dismissed = false;
        keepaliveState.warning = next;
      } catch {
        /* ignore */
      }
    })();
  });
}

export function dismissKeepaliveWarning(): void {
  keepaliveState.dismissed = true;
}
