// A start/stop request arriving from the tray menu.
//
// Module-level runes store (same pattern as restart-state.svelte.ts) so the
// value survives navigation without prop-drilling: the shell receives the
// tray event and navigates to Home, and Home consumes the request on its next
// effect run.
//
// Home takes the request and runs its OWN act() -- the identical path its
// Start/Stop buttons use -- rather than the tray calling the lifecycle API
// itself. One implementation, one place to change.
export const trayAction = $state({ pending: null as "start" | "stop" | null });
