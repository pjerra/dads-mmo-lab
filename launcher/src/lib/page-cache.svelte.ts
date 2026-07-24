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
// heavy pages load through in-process Rust readers (microseconds, no bash/yq,
// Docker may be closed); in WSL mode they stay on the CLI — WSL behavior is
// byte-identical to today. All three now have a native fast-read:
//   config settings -> wowConfigRead  (else wowConfigList)
//   module list     -> wowModuleRead  (else wowModuleList)
//   module tuning   -> wowTuningRead   (else wowConfigTuningList)
// The Rust readers share the same registry/catalog caches the launcher warms
// at startup, so the first native open of each page is fast too — not just
// the cached re-open.

import {
  backendMode,
  wowAccounts,
  wowAccountsRead,
  wowBotsList,
  wowBotsRead,
  wowConfigFiles,
  wowConfigList,
  wowConfigRead,
  wowConfigTuningList,
  wowModuleList,
  wowModuleRead,
  wowPaperdoll,
  wowPaperdollRead,
  wowStats,
  wowStatsRead,
  wowTeleportList,
  wowTeleportListRead,
  wowTuningRead,
  type Account,
  type BackendMode,
  type BotFilters,
  type BotsPage,
  type ConfFile,
  type ConfigSetting,
  type ModuleList,
  type ModuleTuning,
  type PaperdollData,
  type TeleLocation,
  type WowStats,
} from "./api";

// Pure: the error string every page builds from a thrown DmlErr-ish value
// (message plus an optional " — hint"). Centralised so the cache and its
// consumers format load failures identically.
export function formatLoadError(e: unknown): string {
  const err = e as { message?: string; hint?: string };
  return `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
}

// Pure: which reader a backend uses. Native gets the fast in-process Rust
// read; WSL keeps shelling the CLI. Shared by all three heavy pages (config,
// module list, module tuning) — the native-vs-wsl decision is identical for
// each, so one tested pure fn drives them all and can't silently drift.
// "read" = the native Rust sibling (wow*Read); "list" = the CLI command.
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

// Same native-vs-wsl router as loadConfigSettings, for the Module list page.
// Native reads the whole ModuleList off the runtime files via Rust; WSL shells
// `dml wow module list`. Identical .data shape either way.
async function loadModuleList(): Promise<ModuleList> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowModuleRead() : wowModuleList();
}

// Same native-vs-wsl router for the Module tuning tab's settings rows. Native
// fills each row's value + installed via Rust; WSL shells `dml wow config
// tuning-list`. Identical ModuleTuning[] shape either way.
async function loadModuleTuning(): Promise<ModuleTuning[]> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowTuningRead() : wowConfigTuningList();
}

// --- Task 3: routed loaders for the simpler DB-backed pages -----------------
// Same native-vs-wsl router as loadConfigSettings, for the Teleport, Bot Browser
// and Accounts pages. Native reads each surface over a direct MySQL connection in
// Rust (no docker exec / fork storm); WSL keeps shelling the CLI. Identical .data
// shape either way. These pages don't use a createCachedStore (they take live
// args / mutate often), so the loaders are exported and called directly.
export async function loadTeleportList(search?: string): Promise<TeleLocation[]> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowTeleportListRead(search) : wowTeleportList(search);
}

export async function loadBotsPage(f: BotFilters): Promise<BotsPage> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowBotsRead(f) : wowBotsList(f);
}

export async function loadAccounts(): Promise<Account[]> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowAccountsRead() : wowAccounts();
}

// --- Task 4: routed loaders for the COMPLEX DB-backed pages -----------------
// Same native-vs-wsl router, for the Statistics page (whole stats envelope) and
// the character paperdoll. Native reads over direct MySQL in Rust (the 18
// independent stats queries run concurrently); WSL keeps shelling the CLI.
// Identical .data shape either way.
export async function loadStats(): Promise<WowStats> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowStatsRead() : wowStats();
}

export async function loadPaperdoll(charName: string): Promise<PaperdollData> {
  const mode = await resolveBackendMode();
  return pickConfigReader(mode) === "read" ? wowPaperdollRead(charName) : wowPaperdoll(charName);
}

// The shared page caches. Consumed by:
//   configSettingsCache -> Config.svelte (Settings/Bot World/Auction tabs)
//   moduleListCache      -> ModuleManager.svelte (`list`) + ModuleTuning.svelte (server modules)
//   moduleTuningCache    -> ModuleTuning.svelte (`mtSettings`)
//   configFilesCache     -> ModuleTuning.svelte (which module confs exist)
export const configSettingsCache = createCachedStore<ConfigSetting[]>(loadConfigSettings);
export const moduleListCache = createCachedStore<ModuleList>(loadModuleList);
export const moduleTuningCache = createCachedStore<ModuleTuning[]>(loadModuleTuning);
export const configFilesCache = createCachedStore<ConfFile[]>(wowConfigFiles);
