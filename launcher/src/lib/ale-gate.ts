// The Lua-scripts tab's ALE prerequisite.
//
// Lua scripts need mod-ale (the Eluna engine) installed before any of them can
// run. Hiding the whole list until then also hid the reason to install ALE, so
// the rows stay visible but disabled, above a note carrying the fixing action.

import type { CppModule } from "./api";

/** Registry key of the Eluna module (`cpp_installed("mod-ale")` server-side). */
export const ALE_KEY = "mod-ale";

/**
 * The module the "Install ALE" button should install, or null when offering
 * one would be wrong: ALE already ready, no ALE row in the catalog, or a row
 * that claims installed while the server still reports ALE not ready (a
 * re-install wouldn't fix that, so don't imply it would).
 */
export function aleInstallOffer(
  cpp: CppModule[],
  aleReady: boolean,
): { key: string; name: string } | null {
  if (aleReady) return null;
  const row = cpp.find((m) => m.key === ALE_KEY);
  if (!row || row.installed) return null;
  return { key: row.key, name: row.name };
}
