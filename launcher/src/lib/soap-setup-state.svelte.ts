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

export const soapSetupState = $state({
  /** Show the guided account step. */
  needed: false,
});

/// A native install finished successfully. Deliberately NOT called for the WSL
/// route: those installers walk the user through their own account step, and
/// raising this there would ask them to redo it.
export function noteNativeInstallFinished(): void {
  soapSetupState.needed = true;
}

/// The credentials were verified and saved, or the user dismissed the step.
export function clearSoapSetup(): void {
  soapSetupState.needed = false;
}
