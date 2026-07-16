// Module-level runes state so the pending-restart signal survives Config.svelte
// being unmounted (e.g. the user switches sidebar pages) and remounted later.
export const restartState = $state({ needed: false, restarting: false });
