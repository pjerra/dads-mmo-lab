// Module-level runes state so the post-install SOAP step survives Library being
// unmounted -- the same reason `restart-state.svelte.ts` exists for the
// pending-restart banner.
//
// THE BUG THIS FIXES, and it is worse than "the card disappears if you navigate
// away afterwards". The flag used to be component-local `$state` in
// Library.svelte, written only from `onInstallExit`. A native install runs for
// HOURS, and the moment the user clicks any sidebar item during it, Library is
// destroyed. The install keeps going (the terminal store is module-level for
// exactly this reason), but its `onExit` closure now belongs to an orphaned
// component: it sets a flag nobody reads, and the freshly mounted Library never
// re-fires it because `InstallTerminal` returns early at the nonce claim.
//
// Net result: a fully built server whose GM Tools, My Party, console send box
// and announcements all fail with a bare `SOAP_AUTH`, and the only surviving
// advice in the app is Tools' "add soap.env by hand" -- which is precisely the
// unverified-credentials state `crates/dml-wow/src/soap_bootstrap.rs` exists to
// prevent. The one screen that stops that outcome was the one a single click
// could destroy.
//
// A module-level store is written by the orphaned closure and READ by the fresh
// mount, so the step survives navigation exactly as the install itself does.

//
// AUTOSETUP (2026-08-01) changed what this store is FOR. The launcher now
// creates the account itself, so `needed` is no longer "an install finished" --
// it is "the launcher tried and could not". The event-driven setter that used
// to raise it is gone with its only caller.

import type { SoapAutosetupVerdict } from "./api";

export const soapSetupState = $state({
  /**
   * The account step is STILL OUTSTANDING.
   *
   * Since autosetup landed this is the FALLBACK, not the default: it is true
   * only when the launcher tried to create the account itself and could not.
   *
   * It is a fact about the SERVER, not about the window, which is why "Later"
   * no longer touches it -- see `dismissed`.
   */
  needed: false,
  /**
   * The user asked to be left alone; the problem is unchanged.
   *
   * Splitting this out fixes a one-way door that the first fix wave created.
   * "Later" used to call `clearSoapSetup()`, which drops `needed` -- and after
   * the latch landed, `needed` can only be raised again by
   * `applyAutosetupOutcome`'s `gave_up` arm, reached only through
   * `maybeAutosetup`, which the module-level `autosetupSettled` flag in
   * server-status.svelte.ts has already closed (that very gave_up set it). The
   * Library re-check that used to be the second way in was deleted with the old
   * event-driven surface, and Tools has no account card. So one click on
   * "Later" left the user with a server whose GM Tools, My Party and console
   * are all dead and no route back to the step that fixes it -- precisely the
   * unreachable-fallback state the latch change was made to close.
   *
   * The shell renders the card on `needed && !dismissed`; Home's SOAP health
   * row clears this flag, so hiding it is always undoable.
   */
  dismissed: false,
  /** Set when the launcher created the account on its own. Drives the banner. */
  autoResult: null as { user: string } | null,
  /** Why autosetup stopped trying. Shown above the manual card. */
  gaveUpReason: null as string | null,
});

/**
 * Fold one autosetup verdict into the UI state. Returns whether this run is
 * SETTLED — i.e. whether the poll should stop asking.
 *
 * An unknown status settles nothing and shows nothing. Same rule the TermEvent
 * union follows: a backend from a different build must not take the UI down.
 */
export function applyAutosetupOutcome(v: SoapAutosetupVerdict): boolean {
  switch (v.status) {
    case "created":
      soapSetupState.autoResult = { user: v.user ?? "" };
      soapSetupState.needed = false;
      soapSetupState.gaveUpReason = null;
      return true;
    case "gave_up":
      // The ONLY path that raises the manual card.
      //
      // `dismissed` is deliberately left alone. A re-derived verdict is the
      // same fact restated, not new information, so re-showing a card the user
      // just hid would make "Later" a button that undoes itself on the next
      // poll -- the mirror of the bug above.
      soapSetupState.needed = true;
      soapSetupState.autoResult = null;
      soapSetupState.gaveUpReason = v.reason;
      return true;
    // There is deliberately no "already ran" arm. The backend used to answer a
    // second call with a contentless `latched`, which told a webview that had
    // just RELOADED (and so lost this module-level store) nothing at all -- the
    // manual card stayed unreachable for the rest of the process, on the exact
    // path where the launcher had already failed to create the account. A
    // concluded run now re-derives "created"/"gave_up" instead, and the two
    // arms above rebuild the UI from it.
    default:
      return false;
  }
}

/// RESOLVED: the credentials were verified and saved (or the success banner was
/// acknowledged). Everything about the step goes away, including a stale
/// dismissal -- if the account ever falls over again the card must be free to
/// come back on its own.
export function clearSoapSetup(): void {
  soapSetupState.needed = false;
  soapSetupState.dismissed = false;
  soapSetupState.autoResult = null;
  soapSetupState.gaveUpReason = null;
}

/// HIDDEN, not resolved: "Later" on the manual card.
///
/// `needed` is untouched on purpose -- the account still does not work, and the
/// only thing that changed is that the user does not want to deal with it right
/// now. Home's SOAP health row reads `needed` to offer the way back.
export function dismissSoapSetup(): void {
  soapSetupState.dismissed = true;
}

/// The way back from "Later". Undoes the hiding only; it can never invent a
/// step, because the shell still gates on `needed`.
export function showSoapSetup(): void {
  soapSetupState.dismissed = false;
}
