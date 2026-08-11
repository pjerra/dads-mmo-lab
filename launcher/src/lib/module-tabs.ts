// Pure logic for the tabbed Modules page (module-update round): the in-page
// tab strip's typing and the per-module update-button decisions, kept out of
// the components so vitest can cover them without mounting Svelte.

import type { ModuleCheckRepo } from "./api";

// Tab strip order + ids. "modules" hosts the old ModuleManager content,
// "tuning" and "files" host the views extracted from the Config page.
export const MODULE_TABS = ["modules", "tuning", "files"] as const;
export type ModuleTab = (typeof MODULE_TABS)[number];

export const MODULE_TAB_LABELS: Record<ModuleTab, string> = {
  modules: "Modules",
  tuning: "Tuning",
  files: "Config files",
};

// mod-playerbots tracks the custom AzerothCore fork -- pulling it alone would
// desync the core/module pair, so the CLI hard-refuses the key and the GUI
// never offers an Update button for it (a muted "updates with the server"
// note renders instead, pointing at the Modules tab's Server update card).
export function updatesWithServer(key: string): boolean {
  return key === "mod-playerbots";
}

// True when a module row/card should offer the per-module Update button: the
// last check reported a KNOWN behind-count > 0 (null = fetch failed/unknown,
// 0 = up to date) AND the module is not the server-coupled one above.
export function canOfferUpdate(key: string, behind: number | null | undefined): boolean {
  return !updatesWithServer(key) && typeof behind === "number" && behind > 0;
}

// The success note after a per-module update, from the done event's data
// ({ changed, pending_rebuild, rebuild_required }). mod-arac is the
// changed-without-rebuild case (data-only module -- the stream itself points
// at client-patch next).
//
// The third branch exists because `pending_rebuild` stopped being a synonym
// for "not mod-arac" (update-honesty round): it now reports whether the
// rebuild-pending marker REALLY got written, so a compiled module whose marker
// failed arrives here as changed + !pending_rebuild — and the old copy
// congratulated the user with "no rebuild needed" for a module that silently
// will not compile. `rebuild_required` (additive; absent on an older CLI, in
// which case this falls back to the previous wording) separates the two.
export function updateDoneNote(
  d: { changed?: boolean; pending_rebuild?: boolean; rebuild_required?: boolean } | undefined,
): string {
  if (!d?.changed) return "Already up to date.";
  if (d.pending_rebuild) return "Updated — rebuild required before it takes effect.";
  if (d.rebuild_required)
    return "Updated — but the rebuild could not be queued. Start Server rebuild yourself.";
  return "Updated — no rebuild needed.";
}

// After a successful update the clone sits at origin's head -- reflect that
// in the cached check row (the amber chip turns off in both tabs) without
// re-running the whole fetch-everything check.
export function repoAfterUpdate(repo: ModuleCheckRepo, after: string): ModuleCheckRepo {
  return { ...repo, head: after, behind: 0 };
}
