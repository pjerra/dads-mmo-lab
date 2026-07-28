// Whether a backend-setup run is in flight, as module-level runes state.
//
// Same pattern (and same reason) as restart-state.svelte.ts: the first-run
// screen is destroyed the moment the user touches the sidebar, but the
// `backend_setup` stream it kicked off keeps going. Component-local state
// would come back false on remount, re-arming a button whose job is already
// running -- the mistake claimInstallInvoke exists to prevent on the install
// path.
export const backendSetupRun = $state({ running: false });
