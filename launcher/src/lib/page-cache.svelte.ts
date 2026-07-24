// Module-level cached stores for the heavy pages (Config settings, Modules
// list, Module tuning). Same survival trick as server-status.svelte.ts /
// module-updates.svelte.ts: every page mounts/unmounts on each nav
// (routes/+page.svelte gates each with `{#if page === ...}`), so a page's
// component-local state is thrown away when the user leaves it and rebuilt
// from scratch on return. Holding the last-loaded payload at module scope
// lets a re-open render INSTANTLY from cache while a background refresh()
// quietly updates it — first open still loads fresh, re-opens feel instant.
//
// Native fast-read routing (spike/docker-desktop-native): in native mode the
// config settings load through the in-process Rust reader (wowConfigRead,
// microseconds, no bash/yq, Docker may be closed); in WSL mode it stays on
// the CLI (wowConfigList) — WSL behavior is byte-identical to today. The
// module list and tuning list have no Rust fast-read yet (Task 2 covered
// only `config list`), so those load through the CLI in both modes — the
// cache still makes their re-open instant.

import {
  backendMode,
  wowConfigFiles,
  wowConfigList,
  wowConfigRead,
  wowConfigTuningList,
  wowModuleList,
  type BackendMode,
  type ConfFile,
  type ConfigSetting,
  type ModuleList,
  type ModuleTuning,
} from "./api";

// Pure: the error string every page builds from a thrown DmlErr-ish value
// (message plus an optional " — hint"). Centralised so the cache and its
// consumers format load failures identically.
export function formatLoadError(e: unknown): string {
  const err = e as { message?: string; hint?: string };
  return `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
}

// Pure: which config reader a backend uses. Native gets the fast in-process
// Rust read; WSL keeps shelling the CLI. Kept separate + tested so the
// native-vs-wsl decision can't silently drift.
export function pickConfigReader(mode: BackendMode): "read" | "list" {
  return mode === "native" ? "read" : "list";
}

export interface CachedStore<T> {
  // Last successfully-loaded payload, or null before the first success.
  data: T | null;
  // True while a load is in flight (drives optional spinners; also the
  // single-flight guard).
  loading: boolean;
  // Flips true on the first successful load and stays true — lets a consumer
  // tell "have cached data, refreshing" from "never loaded yet".
  loaded: boolean;
  // Last load error, or null. A failed refresh keeps the previous `data`.
  error: string | null;
}

// Factory: a single-flight cached store. refresh() never clobbers a good
// cache on failure (it keeps the last-known `data` and just surfaces the
// error, exactly like the status/update stores do), and concurrent callers
// (mount + a manual refresh + a post-mutation reload) collapse to one
// underlying request. invalidate() drops the cache for a forced reload.
export function createCachedStore<T>(loader: () => Promise<T>) {
  const store = $state<CachedStore<T>>({
    data: null,
    loading: false,
    loaded: false,
    error: null,
  });

  async function refresh(): Promise<void> {
    if (store.loading) return;
    store.loading = true;
    try {
      store.data = await loader();
      store.loaded = true;
      store.error = null;
    } catch (e) {
      // Keep the stale-but-good cache; only the error is new.
      store.error = formatLoadError(e);
    } finally {
      store.loading = false;
    }
  }

  function invalidate(): void {
    store.data = null;
    store.loaded = false;
    store.error = null;
  }

  return { store, refresh, invalidate };
}

// --- Backend mode memo -----------------------------------------------------
// backend_mode is a cheap, pure Rust read, but it's constant for the whole
// process lifetime — resolve it once and reuse the promise so the config
// cache doesn't pay an IPC on every refresh.
let modePromise: Promise<BackendMode> | null = null;
export function resolveBackendMode(): Promise<BackendMode> {
  if (!modePromise) modePromise = backendMode();
  return modePromise;
}

async function loadConfigSettings(): Promise<ConfigSetting[]> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowConfigRead() : wowConfigList();
}

// The shared page caches. Consumed by:
//   configSettingsCache -> Config.svelte (Settings/Bot World/Auction tabs)
//   moduleListCache      -> ModuleManager.svelte (`list`) + ModuleTuning.svelte (server modules)
//   moduleTuningCache    -> ModuleTuning.svelte (`mtSettings`)
//   configFilesCache     -> ModuleTuning.svelte (which module confs exist)
export const configSettingsCache = createCachedStore<ConfigSetting[]>(loadConfigSettings);
export const moduleListCache = createCachedStore<ModuleList>(wowModuleList);
export const moduleTuningCache = createCachedStore<ModuleTuning[]>(wowConfigTuningList);
export const configFilesCache = createCachedStore<ConfFile[]>(wowConfigFiles);
