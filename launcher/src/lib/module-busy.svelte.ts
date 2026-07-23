// Module-level runes state: ONE mutation gate shared by the co-mounted
// Modules-page tabs (module-update round review fix). ModuleManager's `busy`
// and ModuleTuning's per-module update used to gate on component-local flags
// only, but all three tabs stay MOUNTED (display:none switching), so a
// stream started on one tab left the other tab's mutating buttons live --
// e.g. a Tuning-tab stash+pull under a Modules-tab remove/rebuild, or the
// same clone's Update fired from both tabs, races two CLI mutations on one
// checkout. Every mutating flow on either tab sets this flag and every
// mutating button disables on it (same flat-$state-at-module-scope pattern
// as restart-state.svelte.ts).
export const moduleBusy = $state({ busy: false });
