import { invoke, Channel } from "@tauri-apps/api/core";
import type { LiveSpec } from "./party-specs";
// Task A3: native-vs-WSL write routing. `resolveBackendMode` is memoized
// once per process in page-cache.svelte.ts (a cheap Rust read, constant for
// the process lifetime) -- reused here so every routed write below pays at
// most one IPC call for the mode, not one per write. NB this makes api.ts
// and page-cache.svelte.ts mutually-importing; that's fine under ESM (both
// sides only reference function bindings, never evaluated at module-init
// time), but keep it in mind if page-cache.svelte.ts ever grows a
// module-level side effect that runs before its exports are initialized.
import { resolveBackendMode } from "./page-cache.svelte";
export type { LiveSpec };

export interface DmlErr {
  code: string;
  message: string;
  hint: string;
}

export interface Game {
  id: string;
  path: string;
  running: boolean;
}

export type TermEvent =
  | { event: "section_start"; name: string }
  | { event: "line"; level: "info" | "warn" | "error"; text: string }
  | { event: "section_end"; name: string; status: "ok" | "error" }
  | { event: "done"; data: unknown }
  | { event: "error"; error: DmlErr }
  | { event: string; [key: string]: unknown }; // forward-compat: pct etc.

// Native save dialog + write, both on the rust side -- returns false when
// the user cancels. The webview never chooses the path.
export async function saveTextFile(defaultName: string, content: string): Promise<boolean> {
  return invoke("save_text_file", { defaultName, content });
}

export async function gamesList(): Promise<Game[]> {
  const data = await invoke<{ games: Game[] }>("games_list");
  return data.games;
}

export async function gamesStatus(id: string): Promise<{ id: string; state: "running" | "stopped" }> {
  return await invoke("games_status", { id });
}

function streamAction(cmd: "games_start") {
  return (id: string, onEvent: (e: TermEvent) => void): Promise<void> => {
    const ch = new Channel<TermEvent>();
    ch.onmessage = onEvent;
    return invoke(cmd, { id, onEvent: ch });
  };
}

export const gamesStart = streamAction("games_start");
// manageDocker (native mode only, review finding #6): whether this stop should
// also stop the Docker Desktop engine, passed through as `manage_docker`.
// Undefined lets the Rust side default ON (dml::native::stop_engine_enabled) --
// callers that care pass the persisted toolPrefs.manageDocker preference.
export const gamesStop = (
  id: string,
  onEvent: (e: TermEvent) => void,
  manageDocker?: boolean,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("games_stop", { id, manageDocker, onEvent: ch });
};

export interface CharacterSummary {
  guid: number;
  name: string;
  level: number;
}
export interface Account {
  id: number;
  username: string;
  gm_level: number;
  characters: CharacterSummary[];
}
export interface ServerInfo {
  online: boolean;
  version: string | null;
  players: number | null;
  uptime: string | null;
  mean_ms: number | null;
  median_ms: number | null;
}
// "crashed" (Batch 2 F8): containers exist, world not running, and the world
// container's exit code is neither 0 nor 143 (SIGTERM) -- i.e. it died rather
// than being stopped.
export type ServerVerdict = "stopped" | "starting" | "online" | "soap_unreachable" | "crashed";
export interface ContainerRow {
  name: string;
  role: "world" | "auth" | "database";
  // Docker's state string ("running", "exited", "restarting", ...) or
  // "absent" when the container doesn't exist (e.g. after compose down).
  state: string;
  status: string;
}
export interface SoapState {
  reachable: boolean;
  auth_ok: boolean | null;
  version: string | null;
  players: number | null;
  uptime: string | null;
  mean_ms: number | null;
  median_ms: number | null;
}
export interface ServerDetail {
  verdict: ServerVerdict;
  // Last exit code of the world container when it exists but isn't running
  // (drives crashed-vs-stopped); null while running / when absent.
  exit_code: number | null;
  containers: ContainerRow[];
  world_ready: boolean;
  soap: SoapState;
  ports: {
    world: string | null;
    auth: string | null;
    soap: string | null;
    db: string | null;
  };
  bots: { online: number | null; max: number | null };
}
export interface ItemRow {
  entry: number;
  name: string;
  quality: number;
  item_level: number;
  required_level: number;
  class: number;
  subclass: number;
  inventory_type: number;
  displayid: number;
}
export interface TeleLocation {
  name: string;
  x: number;
  y: number;
  z: number;
  map: number;
}
export interface PaperdollItem {
  slot: number;
  entry: number;
  name: string;
  quality: number;
  item_level: number;
  displayid: number;
}
export interface PaperdollData {
  name: string;
  level: number;
  class: number;
  race: number;
  gender: number;
  skin: number;
  face: number;
  hair_style: number;
  hair_color: number;
  facial_style: number;
  gold: number;
  note: string;
  equipped: PaperdollItem[];
}
export interface ConfigSetting {
  key: string;
  group: string;
  label: string;
  explain: string;
  type: "float" | "int" | "bool" | "text" | "char";
  min: number | null;
  max: number | null;
  value: string;
  default: string;
  restart_required: boolean;
  env: string;
}
// Batch 1 F3: the editable-file list is dynamic (wow config files), so
// names are plain strings validated CLI-side (basename-shape allowlist).
export type RawFileName = string;
export interface ConfFile {
  name: string;
  exists: boolean;
  dist: boolean;
  readonly: boolean;
}

export async function wowAccounts(): Promise<Account[]> {
  const data = await invoke<{ accounts: Account[] }>("wow_accounts");
  return data.accounts;
}
// NATIVE-MODE fast sibling of wowAccounts: identical Account[] shape, read over a
// direct MySQL connection in the launcher's Rust core (no bash, no docker exec).
// Call ONLY when backendMode() === "native"; in wsl mode call wowAccounts.
export async function wowAccountsRead(): Promise<Account[]> {
  const data = await invoke<{ accounts: Account[] }>("wow_accounts_read");
  return data.accounts;
}
// Task A3: native mode routes to the SOAP-backed `_native` siblings (Task
// A2b); WSL mode keeps shelling `dml` byte-identically. Universal routing
// (native->Rust for WSL too, over SOAP-over-TCP) is a safe future flip once
// live-smoked -- see the plan's A4 decision note.
export async function wowAccountCreate(user: string, pass: string): Promise<{ created: boolean; user: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_account_create_native", { user, pass })
    : invoke("wow_account_create", { user, pass });
}
export async function wowAccountSetPassword(user: string, pass: string): Promise<{ password_set: boolean; user: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_account_set_password_native", { user, pass })
    : invoke("wow_account_set_password", { user, pass });
}
export async function wowAccountSetGm(user: string, level: number): Promise<{ gm_set: boolean; user: string; level: number }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_account_set_gm_native", { user, level })
    : invoke("wow_account_set_gm", { user, level });
}
export async function wowAccountDelete(user: string): Promise<{ deleted: boolean; user: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_account_delete_native", { user })
    : invoke("wow_account_delete", { user });
}
// Task B2: native mode routes to the direct SOAP/docker/DB `_read` siblings
// (`dml::status`); WSL mode keeps shelling `dml` byte-identically.
export async function wowServerInfo(): Promise<ServerInfo> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_server_info_read") : invoke("wow_server_info");
}
export async function wowServerDetail(): Promise<ServerDetail> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_server_detail_read") : invoke("wow_server_detail");
}
// --- Statistics page (`dml wow stats`): one read-only envelope ------------
export interface StatsLevelBucket {
  bucket: number; // 0 = levels 1-10 ... 7 = levels 71-80
  family: number;
  bots: number;
}
export interface StatsTopLevel {
  name: string;
  level: number;
  family: boolean;
}
export interface StatsRich {
  name: string;
  copper: number;
  family: boolean;
}
// Segment-sensitive stats arrive split per segment for the page's
// All|Family|Bots filter -- "all" is a client-side merge (stats.ts pick*).
export interface StatsClassCount {
  class: number;
  count: number;
}
export interface StatsFactionSplit {
  alliance: number;
  horde: number;
}
export interface StatsSegmented<T> {
  family: T;
  bots: T;
}
export interface StatsJourneyRow {
  name: string;
  level: number;
  class: number;
  playtime: number; // seconds
  last_seen: number; // unix seconds (0 = never saved a logout)
  online: boolean; // logged in right now -> the page shows "Online now"
  kills: number;
  achievements: number;
  quests: number;
}
export interface StatsBoot {
  start: number; // unix seconds
  uptime: number; // seconds
}
export interface WowStats {
  population: {
    family: { total: number; online: number };
    bots: { total: number; online: number };
    levels: StatsLevelBucket[];
    classes: StatsSegmented<StatsClassCount[]>;
    factions: StatsSegmented<StatsFactionSplit>;
    top_levels: StatsSegmented<StatsTopLevel[]>;
    guilds: { count: number; members: number };
  };
  economy: {
    // Money is COPPER everywhere -- divide by 10000 for gold on screen.
    copper: { total: number; family: number; bots: number };
    richest: StatsSegmented<StatsRich[]>;
    auction: { count: number; buyout: number };
    mail: { total: number; to_family: number };
  };
  journey: StatsJourneyRow[];
  history: {
    boots: number;
    total_uptime: number;
    longest: number;
    peak: number;
    realm: string;
    recent: StatsBoot[];
  };
  botwatch: {
    zones: { zone: number; count: number }[];
    continents: { map: number; count: number }[];
    playtime: number; // seconds, all bots combined
  };
}
export async function wowStats(): Promise<WowStats> {
  return await invoke("wow_stats");
}
// NATIVE-MODE fast sibling of wowStats: identical WowStats shape, assembled from
// direct-MySQL queries in the launcher's Rust core (no bash, no docker exec).
// Call ONLY when backendMode() === "native"; in wsl mode call wowStats.
export async function wowStatsRead(): Promise<WowStats> {
  return await invoke("wow_stats_read");
}
// Task C starter: native mode routes to the direct docker/git `_read`
// siblings (`dml::maint`); WSL mode keeps shelling `dml` byte-identically.
export async function wowDockerUsage(): Promise<{ lines: string[] }> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_docker_usage_read") : invoke("wow_docker_usage");
}
// Native-mode routing (Chunk 4a): `wow_docker_clean_native` drives `docker
// compose`/`docker builder|image prune`/`docker volume` directly (no `dml`
// shell-out) -- same level/Channel contract either way.
export const wowDockerClean = async (level: number, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_docker_clean_native", { level, onEvent: ch })
    : invoke("wow_docker_clean", { level, onEvent: ch });
};
export interface UpdateRepo {
  label: "AzerothCore" | "mod-playerbots";
  url: string;
  branch: string;
  head: string;
  dirty: number;
  behind: number | null;
}
export interface UpdateCheck {
  repos: UpdateRepo[];
  note?: string;
}
export async function wowUpdateCheck(): Promise<UpdateCheck> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_update_check_read") : invoke("wow_update_check");
}
// Native-mode routing (Chunk 3b): the native command emits its ndjson
// events directly (no `dml` subprocess, faithful port of the `wow update`
// arm's fail-closed gates) -- same `backup` boolean either way. WSL mode is
// byte-for-byte unchanged.
export const wowServerUpdate = async (backup: boolean, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_update_native", { backup, onEvent: ch })
    : invoke("wow_server_update", { backup, onEvent: ch });
};
export interface ConsoleTail {
  available: boolean;
  lines: string[];
}
export async function wowConsoleTail(lines?: number): Promise<ConsoleTail> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_console_tail_read", { lines })
    : invoke("wow_console_tail", { lines });
}
export async function wowConsoleSend(command: string): Promise<{ result: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_console_send_native", { command })
    : invoke("wow_console_send", { command });
}

export interface ModCommands {
  key: string;
  name: string;
  text: string;
}

export interface CppModule {
  key: string;
  name: string;
  desc: string;
  url: string | null;
  installed: boolean;
  pending_rebuild: boolean;
  conf: "none" | "needs-rebuild" | "ready" | "active";
  // The module's conf basename (transmog.conf), null when the module has no
  // conf file. Additive (Module-tuning rework) -- optional so an older
  // deployed CLI without the field can't crash the Modules/Config pages.
  conf_name?: string | null;
  // The installed clone's last commit: short sha + YYYY-MM-DD commit date
  // (local git reads only -- update-check owns fetching). Null when the
  // module isn't installed or its dir has no .git. Additive (module-update
  // round) -- optional for the same older-CLI reason as conf_name.
  head?: string | null;
  head_date?: string | null;
  custom: boolean;
}
export interface LuaModule {
  key: string;
  name: string;
  desc: string;
  url: string | null;
  cloned: boolean;
  deployed: boolean;
  has_sql: boolean;
  // Batch 6 A: read-only advisory shown on the card (e.g. Paragon's
  // unguarded `.test` chat command). Null when there's nothing to flag.
  warn: string | null;
}
export interface SqlModule {
  key: string;
  name: string;
  desc: string;
  url: string | null;
  type: string;
  installed: boolean;
}
export interface ModuleList {
  families: { cpp: CppModule[]; lua: LuaModule[]; sql: SqlModule[] };
  rebuild_pending: string[];
  ale_ready: boolean;
}
export async function wowModuleList(): Promise<ModuleList> {
  return await invoke("wow_module_list");
}
// NATIVE-MODE fast sibling of wowModuleList: identical ModuleList shape, but the
// launcher's Rust core fills every dynamic field (installed/conf/head/date,
// cloned/deployed/warn, sql installed, rebuild_pending, ale_ready) straight off
// the runtime files (no bash/fork; only local git reads for installed clones).
// Call ONLY when backendMode() === "native"; in wsl mode call wowModuleList. The
// static catalog is fetched once per session and cached in Rust (warmed at
// startup).
export async function wowModuleRead(): Promise<ModuleList> {
  return await invoke("wow_module_read");
}

export async function wowCommands(): Promise<ModCommands[]> {
  const mode = await resolveBackendMode();
  const data =
    mode === "native"
      ? await invoke<{ mods: ModCommands[] }>("wow_commands_read")
      : await invoke<{ mods: ModCommands[] }>("wow_commands");
  return data.mods;
}

// Chunk 3a: native-mode module install/update/remove are fully native
// STREAMED commands (no `dml` subprocess) -- same Channel signature as the
// WSL sibling, routed via resolveBackendMode() like wowWorldRestart/
// wowBridgeSetup above.
export const wowModuleInstall = async (
  family: string,
  key: string | null,
  url: string | null,
  onEvent: (e: TermEvent) => void,
  backup?: boolean,
  variant?: string,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_module_install_native", { family, key, url, backup, variant, onEvent: ch })
    : invoke("wow_module_install", { family, key, url, backup, variant, onEvent: ch });
};
export const wowModuleRemove = async (
  family: string,
  key: string,
  onEvent: (e: TermEvent) => void,
  backup?: boolean,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_module_remove_native", { family, key, backup, onEvent: ch })
    : invoke("wow_module_remove", { family, key, backup, onEvent: ch });
};
// Native-mode routing (Chunk 4a): `wow_module_rebuild_native` streams
// `docker compose up -d --build` LIVE (no `dml` shell-out) -- same
// backup/Channel contract either way.
export const wowModuleRebuild = async (
  backup: boolean,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_module_rebuild_native", { backup, onEvent: ch })
    : invoke("wow_module_rebuild", { backup, onEvent: ch });
};
// Module-update round: one repo object per installed cpp clone with a .git
// dir (registry + custom), same field shape as the server-level UpdateRepo.
// An installed mod-playerbots clone IS listed here even though
// wowModuleUpdate refuses it (it updates with the server core).
export interface ModuleCheckRepo {
  label: string;
  url: string;
  branch: string;
  head: string;
  dirty: number;
  behind: number | null;
}
export interface ModuleUpdateCheck {
  repos: ModuleCheckRepo[];
}
export async function wowModuleUpdateCheck(): Promise<ModuleUpdateCheck> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_module_update_check_native") : invoke("wow_module_update_check");
}
// Per-module source pull (patch backup + stash + ff-only, no auto rebuild).
// Done event data: { key, changed, before, after, pending_rebuild }. Native
// mode runs this fully in Rust (no `dml` subprocess) via `dml::modmgr::
// update_module` -- same Channel signature either way.
export const wowModuleUpdate = async (
  key: string,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_module_update_native", { key, onEvent: ch })
    : invoke("wow_module_update", { key, onEvent: ch });
};
// Batch 5 F2: ARAC server-DBC + client-MPQ patch stream (key is allowlisted
// CLI-side to mod-arac).
export const wowModuleClientPatch = async (
  key: string,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_module_client_patch_native", { key, onEvent: ch })
    : invoke("wow_module_client_patch", { key, onEvent: ch });
};
export async function wowModuleConfActivate(
  key: string,
  force?: boolean,
): Promise<{ key: string; activated: boolean; conf_name: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_module_conf_activate_native", { key, force })
    : invoke("wow_module_conf_activate", { key, force });
}

export interface TrackingFile {
  name: string;
  tracked: boolean;
}

export interface TrackingDb {
  tracked_rows: string[];
  files: TrackingFile[];
}

export interface ModuleTracking {
  key: string;
  dbs: {
    world: TrackingDb;
    characters: TrackingDb;
    auth: TrackingDb;
  };
}

export async function wowModuleTracking(key: string): Promise<ModuleTracking> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_module_tracking_native", { key }) : invoke("wow_module_tracking", { key });
}

export interface RepairResult {
  file: string;
  result: "marked" | "file_missing" | "cleared" | "not_tracked";
}

export interface ModuleRepair {
  key: string;
  db: string;
  mode: string;
  results: RepairResult[];
}

export async function wowModuleRepair(
  key: string,
  db: "world" | "characters" | "auth",
  repairMode: "mark" | "clear",
  files?: string,
): Promise<ModuleRepair> {
  const backendMode = await resolveBackendMode();
  return backendMode === "native"
    ? invoke("wow_module_repair_native", { key, db, mode: repairMode, files })
    : invoke("wow_module_repair", { key, db, mode: repairMode, files });
}

// Batch 3 F13b: canned one-shot module fixes (currently only the missing
// Battle Pass vendor NPC). Idempotent CLI-side.
export interface ModuleFixit {
  key: string;
  already_placed: boolean;
  template: "created" | "exists";
  spawns_placed: number;
  restart_required: boolean;
  note: string;
}

export async function wowModuleFixit(key: "battlepass-npc"): Promise<ModuleFixit> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_module_fixit_native", { key }) : invoke("wow_module_fixit", { key });
}

// Batch 2 (overnight): spawn an installed NPC-mod's creature in both capitals
// (Stormwind + Orgrimmar) from its ready-made coord block. Idempotent per map
// CLI-side; the key is allowlisted on both sides. The NPC only appears after a
// world restart (restart_required tells the UI to nudge one).
export type PlaceNpcKey = "mod-1v1-arena" | "mod-transmog" | "mod-npc-beastmaster" | "bmah";
export interface ModulePlaceNpc {
  key: string;
  entry: number;
  maps: { map: number; placed: boolean }[];
  spawns_placed: number;
  already_placed: boolean;
  restart_required: boolean;
  note: string;
}
export async function wowModulePlaceNpc(key: PlaceNpcKey): Promise<ModulePlaceNpc> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_module_place_npc_native", { key })
    : invoke("wow_module_place_npc", { key });
}

export interface ClientPath {
  path: string | null;
  valid: boolean;
}
export async function wowClientPathGet(): Promise<ClientPath> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_client_path_read")
    : await invoke("wow_client_path_get");
}
export async function wowClientPathSet(path: string): Promise<ClientPath> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_client_path_set_native", { path })
    : await invoke("wow_client_path_set", { path });
}
export async function wowClientPathDetect(): Promise<{ candidates: string[] }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_client_path_detect_read")
    : await invoke("wow_client_path_detect");
}

export async function wowItemsSearch(p: {
  name: string;
  quality?: number;
  minLevel?: number;
  maxLevel?: number;
}): Promise<ItemRow[]> {
  const mode = await resolveBackendMode();
  const data =
    mode === "native"
      ? await invoke<{ items: ItemRow[] }>("wow_items_search_read", p)
      : await invoke<{ items: ItemRow[] }>("wow_items_search", p);
  return data.items;
}
export async function wowMailItem(p: {
  to: string;
  items: string;
  subject?: string;
  body?: string;
}): Promise<{ sent: boolean; to: string; attachments: number }> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_mail_item_native", p) : invoke("wow_mail_item", p);
}
export async function wowTeleportList(search?: string): Promise<TeleLocation[]> {
  const data = await invoke<{ locations: TeleLocation[] }>("wow_teleport_list", { search });
  return data.locations;
}
// NATIVE-MODE fast sibling of wowTeleportList: identical TeleLocation[] shape,
// read over a direct MySQL connection in the launcher's Rust core (the float
// coordinates are CAST-rendered server-side so they byte-match the CLI). Call
// ONLY when backendMode() === "native"; in wsl mode call wowTeleportList.
export async function wowTeleportListRead(search?: string): Promise<TeleLocation[]> {
  const data = await invoke<{ locations: TeleLocation[] }>("wow_teleport_list_read", { search });
  return data.locations;
}
export async function wowTeleport(
  charName: string,
  to: string,
): Promise<{ teleported: boolean; char: string; to: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_teleport_native", { charName, to })
    : invoke("wow_teleport", { charName, to });
}
// Part 5a: native mode routes to the DB-only `_native` sibling (never SOAP --
// see the Rust doc comment on `wow_teleport_coords_native`); WSL mode keeps
// shelling `dml` byte-identically.
export async function wowTeleportCoords(
  charName: string,
  map: number,
  x: number,
  y: number,
  z: number,
): Promise<{ teleported: boolean; char: string; map: number; x: number; y: number; z: number }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_teleport_coords_native", { charName, map, x, y, z })
    : await invoke("wow_teleport_coords", { charName, map, x, y, z });
}
export async function wowPaperdoll(charName: string): Promise<PaperdollData> {
  return await invoke("wow_paperdoll", { charName });
}
// NATIVE-MODE fast sibling of wowPaperdoll: identical PaperdollData shape, read
// over a direct MySQL connection in the launcher's Rust core (no bash, no docker
// exec, no SOAP saveall). Call ONLY when backendMode() === "native"; in wsl mode
// call wowPaperdoll.
export async function wowPaperdollRead(charName: string): Promise<PaperdollData> {
  return await invoke("wow_paperdoll_read", { charName });
}
export interface WowheadTooltip {
  name: string;
  quality: number;
  icon: string;
  tooltip: string;
}
export interface ItemInfo {
  entry: number;
  source: "wowhead" | "local" | "unavailable";
  icon?: string | null;
  icon_b64?: string | null;
  wowhead?: WowheadTooltip;
  name?: string;
  quality?: number;
  tooltip_html?: string;
  // Wowhead's own 3D display id for this item (from the item XML's
  // displayId attribute), emitted by the CLI only when strictly positive.
  // The model viewer uses it as a fallback probe candidate when the
  // server's displayid has no Wowhead model data (e.g. the Warglaives).
  display_id?: number;
}
// NATIVE-MODE fast sibling of wow_item_info: same {items:[...]} shape, read
// via direct reqwest + the shared ~/.dml/wowhead-cache disk cache in the
// launcher's Rust core (dml::iteminfo) instead of shelling `dml`.
export async function wowItemInfo(entries: number[]): Promise<ItemInfo[]> {
  const mode = await resolveBackendMode();
  const d =
    mode === "native"
      ? await invoke<{ items: ItemInfo[] }>("wow_item_info_read", { entries })
      : await invoke<{ items: ItemInfo[] }>("wow_item_info", { entries });
  return d.items;
}
export interface AchievementEntry {
  id: number;
  date: number;
}
export interface CharProgress {
  achievements: { total: number; recent: AchievementEntry[] };
  talents: { groups_count: number; active_group: number; spells: number[] };
}
export async function wowCharProgress(charName: string): Promise<CharProgress> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_char_progress_read", { charName })
    : invoke("wow_char_progress", { charName });
}
export interface EarnedAchievements {
  earned: AchievementEntry[];
}
export async function wowAchievements(charName: string): Promise<EarnedAchievements> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_achievements_read", { charName })
    : invoke("wow_achievements", { charName });
}
export interface EntityInfo {
  id: number;
  source: "wowhead" | "unavailable";
  icon?: string | null;
  icon_b64?: string | null;
  wowhead?: WowheadTooltip;
}
// NATIVE-MODE fast sibling of wow_entity_info: same {entities:[...]} shape,
// same dml::iteminfo machinery as wowItemInfo above (no local/DB fallback).
export async function wowEntityInfo(kind: "spell" | "achievement", ids: number[]): Promise<EntityInfo[]> {
  const mode = await resolveBackendMode();
  const d =
    mode === "native"
      ? await invoke<{ entities: EntityInfo[] }>("wow_entity_info_read", { kind, ids })
      : await invoke<{ entities: EntityInfo[] }>("wow_entity_info", { kind, ids });
  return d.entities;
}
export async function wowConfigList(): Promise<ConfigSetting[]> {
  const data = await invoke<{ settings: ConfigSetting[] }>("wow_config_list");
  return data.settings;
}
// Which backend the launcher process selected. The frontend router uses this
// to decide whether to fast-path config reads through Rust (native) or keep
// shelling the CLI (wsl). Cheap and pure — safe to call on mount and cache.
export type BackendMode = "native" | "wsl";

// The launcher's OWN settings, persisted at ~/.dml/launcher.json. Distinct
// from the AC config registry: Rust reads these at startup, before any window
// exists, which is why they cannot live in localStorage like the rest of the
// launcher's preferences.
export interface LauncherConfig {
  backend: string | null;
  gamesDir: string | null;
  dmlScript: string | null;
  yqBin: string | null;
  closeToTray: boolean;
  startWithWindows: boolean;
}
export interface LauncherSettings {
  config: LauncherConfig;
  // Which source currently WINS for the backend. "env" means the dropdown is
  // read-only -- an environment variable overrides the persisted setting.
  backendSource: "env" | "file" | "auto";
  effectiveBackend: BackendMode;
  envBackend: string | null;
}

export async function launcherConfigRead(): Promise<LauncherSettings> {
  return await invoke<LauncherSettings>("launcher_config_read");
}

// The Rust parameter is named `cfg`, so the invoke key is `cfg`. Deriving the
// key from the Rust PARAMETER name matters: a wrongly-cased key fails
// silently for Option params rather than erroring.
export async function launcherConfigWrite(cfg: LauncherConfig): Promise<void> {
  return await invoke("launcher_config_write", { cfg });
}

export async function backendMode(): Promise<BackendMode> {
  return await invoke<BackendMode>("backend_mode");
}
// NATIVE-MODE fast sibling of wowConfigList: identical ConfigSetting[] shape,
// but the launcher's Rust core reads the live values straight off the runtime
// files (no bash/yq/fork; Docker may be closed). Call this ONLY when
// backendMode() === "native"; in wsl mode call wowConfigList instead. The
// static registry is fetched from the CLI once per session and cached in Rust.
export async function wowConfigRead(): Promise<ConfigSetting[]> {
  const data = await invoke<{ settings: ConfigSetting[] }>("wow_config_read");
  return data.settings;
}
// Task B3: native mode routes to the SOAP/file-backed `_native` sibling
// (Task B2a); WSL mode keeps shelling `dml` byte-identically. Same pattern
// as the A3 write routing.
export async function wowConfigSet(
  key: string,
  value: string,
): Promise<{ changed: boolean; restart_required: boolean; applied?: "live" | "restart" | "none" }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_config_set_native", { key, value })
    : await invoke("wow_config_set", { key, value });
}
export interface PbKey {
  key: string;
  value: string;
  default: string | null;
  line: number;
}
export async function wowConfigPbKeys(): Promise<{ source: string; keys: PbKey[] }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_config_pb_keys_native")
    : await invoke("wow_config_pb_keys");
}
// Module-tuning rework: pb-keys generalized to any editable module conf.
// `help` is the key's comment-block doc parsed from the conf's .dist ("" when
// the module author documented nothing near the key).
export interface ConfKey {
  key: string;
  value: string;
  default: string | null;
  line: number;
  help: string;
}
export async function wowConfigConfKeys(
  file: string,
): Promise<{ file: string; source: "conf" | "dist"; keys: ConfKey[] }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_config_conf_keys_native", { file })
    : await invoke("wow_config_conf_keys", { file });
}
export async function wowConfigFiles(): Promise<ConfFile[]> {
  const data = await invoke<{ files: ConfFile[] }>("wow_config_files");
  return data.files;
}

// --- Guided module tuning (overnight Batch 3) ------------------------------
// Curated, plain-language activator knobs for a few optional modules. Two
// backends share one surface: "conf" rows edit the module's bind-mounted
// .conf (restart to apply); "lua" rows line-replace the deployed ALE script
// (apply live with `.reload ale`). `installed` = the owning module's file is
// deployed on the box; when false the GUI points the user at the Modules page.
export interface ModuleTuning {
  key: string;
  backend: "conf" | "lua";
  module: string; // plain module name; also the card heading
  label: string;
  explain: string;
  type: "bool" | "int" | "list";
  min: number | null;
  max: number | null;
  value: string;
  default: string;
  installed: boolean;
  // The row's backing file basename (mod_learnspells.conf / UnlimitedAmmo.lua)
  // -- lets the Module tuning tab render curated conf rows inside the owning
  // module's card. Additive; optional so an older deployed CLI can't crash it.
  file?: string;
}
export async function wowConfigTuningList(): Promise<ModuleTuning[]> {
  const data = await invoke<{ settings: ModuleTuning[] }>("wow_config_tuning_list");
  return data.settings;
}
// NATIVE-MODE fast sibling of wowConfigTuningList: identical ModuleTuning[]
// shape, but the launcher's Rust core reads each row's live value + installed
// state straight off the runtime files (no bash/fork; Docker may be closed).
// Call ONLY when backendMode() === "native"; in wsl mode call
// wowConfigTuningList. The static tuning registry is fetched once per session
// and cached in Rust (and warmed at startup).
export async function wowTuningRead(): Promise<ModuleTuning[]> {
  const data = await invoke<{ settings: ModuleTuning[] }>("wow_tuning_read");
  return data.settings;
}
// Task B3: same native/WSL routing as wowConfigSet above.
export async function wowConfigTuningSet(
  key: string,
  value: string,
): Promise<{
  key: string;
  backend: "conf" | "lua";
  changed: boolean;
  restart_required: boolean;
  applied: "restart" | "reload-ale" | "none";
  reload?: string;
}> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_config_tuning_set_native", { key, value })
    : await invoke("wow_config_tuning_set", { key, value });
}

// --- Account-wide sharing configurator (overnight Batch 1) -----------------
// Reads/writes the ENABLE_* flags in the deployed accountwide lua files.
export interface AwSubsystem {
  key: string;
  file: string;
  group: string;
  parent: string | null; // flag that must be ON for this sub-toggle to matter
  label: string;
  explain: string;
  value: "on" | "off";
}
export interface AwReputation {
  present: boolean;
  value: "on" | "off";
  variants: ("default" | "custom")[]; // which variant files are deployed
  active: "default" | "custom" | null; // the variant currently enabled
}
export interface AccountwideState {
  installed: boolean;
  subsystems: AwSubsystem[];
  reputation: AwReputation;
}
export async function wowAccountwideGet(): Promise<AccountwideState> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_accountwide_get_native")
    : await invoke("wow_accountwide_get");
}
export async function wowAccountwideSet(
  key: string,
  value: "on" | "off",
  variant?: "default" | "custom",
): Promise<{ key: string; value: "on" | "off"; changed: boolean; reload: string; variant?: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_accountwide_set_native", { key, value, variant })
    : await invoke("wow_accountwide_set", { key, value, variant });
}
export async function wowConfigRawRead(
  file: RawFileName,
): Promise<{ file: string; source?: "conf" | "dist"; content: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_config_raw_read_native", { file })
    : await invoke("wow_config_raw_read", { file });
}
export async function wowConfigRawReset(
  file: RawFileName,
): Promise<{ reset: boolean; file: string; backup: string | null }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_config_raw_reset_native", { file })
    : await invoke("wow_config_raw_reset", { file });
}
export async function wowConfigRawWrite(
  file: RawFileName,
  content: string,
): Promise<{ written: boolean; backup: string | null }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_config_raw_write_native", { file, content })
    : await invoke("wow_config_raw_write", { file, content });
}
// skipSaveall = the "faster restart" option: skip the redundant pre-stop
// saveall (the graceful stop still saves characters on shutdown).
export const gamesRestart = (
  id: string,
  skipSaveall: boolean,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("games_restart", { id, skipSaveall, onEvent: ch });
};
// Batch 3 F11f: world-only restart -- faster, but does NOT apply settings
// changes (docker restart keeps creation-time env; full Restart owns that).
// Native-mode routing (world-restart-native task): the native command emits
// its ndjson events DIRECTLY (no `dml` subprocess), so it takes `noSaveall`
// instead of the WSL sibling's `skipSaveall` -- same boolean, renamed to
// match the Rust command's param name (mirrors the CLI's `--no-saveall`
// flag). WSL mode is byte-for-byte unchanged.
export const wowWorldRestart = async (
  skipSaveall: boolean,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_world_restart_native", { noSaveall: skipSaveall, onEvent: ch })
    : invoke("wow_world_restart", { skipSaveall, onEvent: ch });
};
// Flush & rebuild the ambient bot population (Batch 1 F4). The CLI enforces
// --yes plus the typed ack itself; the GUI's typed-confirm gates calling this.
// Native-mode routing (Chunk 4b): `wow_bots_flush_native` drives the same
// arm/restart/disarm/rebuild sequence directly (no `dml` shell-out) -- same
// no-parameter/Channel contract either way; the typed-"flush" UI is the gate
// on both backends.
export const wowBotsFlush = async (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_bots_flush_native", { onEvent: ch }) : invoke("wow_bots_flush", { onEvent: ch });
};

export interface OnlineChar { guid: number; name: string; class: number; level: number; }
// Batch 3 F11a: Home "players online" card row (bots excluded CLI-side).
export interface PlayerOnline { name: string; level: number; class: number; zone: number; }

export async function wowPlayersOnline(): Promise<PlayerOnline[]> {
  const mode = await resolveBackendMode();
  const d =
    mode === "native"
      ? await invoke<{ players: PlayerOnline[] }>("wow_players_online_read")
      : await invoke<{ players: PlayerOnline[] }>("wow_players_online");
  return d.players;
}
// Batch 5 F1 (Bot Browser): one page of the random-bot population.
export interface BotRow {
  guid: number;
  name: string;
  class: number;
  race: number;
  gender: number;
  level: number;
  online: boolean;
  zone: number;
}
export interface BotsPage {
  total: number;
  limit: number;
  offset: number;
  bots: BotRow[];
}
// A type alias (not an interface) on purpose: aliases get TS's implicit
// index signature, so this stays assignable to invoke()'s InvokeArgs.
export type BotFilters = {
  name?: string;
  class?: number;
  minLevel?: number;
  maxLevel?: number;
  online?: boolean;
  limit?: number;
  offset?: number;
};
export async function wowBotsList(f: BotFilters): Promise<BotsPage> {
  return await invoke("wow_bots_list", f);
}
// NATIVE-MODE fast sibling of wowBotsList: identical BotsPage shape (same clamp
// of limit to 1..=200), read over a direct MySQL connection in the launcher's
// Rust core. Call ONLY when backendMode() === "native"; in wsl mode call
// wowBotsList.
export async function wowBotsRead(f: BotFilters): Promise<BotsPage> {
  return await invoke("wow_bots_read", f);
}
export interface PartyMember { guid: number; name: string; class: number; level: number; is_bot: boolean; online: boolean; }
// spec/spec_applied only present when the add carried a --spec (Batch 5 F5).
export interface PartyAddResult {
  added: boolean;
  joined: boolean;
  bot: string | null;
  note: string | null;
  spec?: string;
  spec_applied?: boolean;
}

export async function wowPartyOnline(): Promise<OnlineChar[]> {
  const mode = await resolveBackendMode();
  const d =
    mode === "native"
      ? await invoke<{ online: OnlineChar[] }>("wow_party_online_read")
      : await invoke<{ online: OnlineChar[] }>("wow_party_online");
  return d.online;
}
// Read-only: the live premade specs from the deployed playerbots.conf. Empty
// when the server isn't installed (the UI then falls back to its static maps).
export async function wowPartySpecs(): Promise<LiveSpec[]> {
  const mode = await resolveBackendMode();
  const d =
    mode === "native"
      ? await invoke<{ source: string; specs: LiveSpec[] }>("wow_party_specs_read")
      : await invoke<{ source: string; specs: LiveSpec[] }>("wow_party_specs");
  return d.specs;
}
export async function wowPartyAdd(
  player: string,
  className: string,
  gender?: string,
  spec?: string,
): Promise<PartyAddResult> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_add_native", { player, class: className, gender, spec })
    : invoke("wow_party_add", { player, class: className, gender, spec });
}
export async function wowPartyList(player: string): Promise<PartyMember[]> {
  const d = await invoke<{ members: PartyMember[] }>("wow_party_list", { player });
  return d.members;
}
// Kick = uninvite + a master `logout` whisper (the bot despawns instead of
// following its ex-party around) -- hence the master `player` argument.
export async function wowPartyKick(
  player: string,
  bot: string,
): Promise<{ kicked: boolean; dismissed: boolean }> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_party_kick_native", { player, bot }) : invoke("wow_party_kick", { player, bot });
}
// `dismissed` counts bots whose uninvite fire actually succeeded (the CLI
// errors out instead when EVERY fire failed); `attempted` is the party's
// bot count, so attempted > dismissed means a partial failure.
export async function wowPartyDismissAll(
  player: string,
): Promise<{ dismissed: number; attempted: number; bots: string[] }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_dismiss_all_native", { player })
    : invoke("wow_party_dismiss_all", { player });
}
export async function wowPartyRelogin(player: string, bot: string): Promise<{ relogged: boolean }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_relogin_native", { player, bot })
    : invoke("wow_party_relogin", { player, bot });
}
// Native-mode routing (Chunk 2, task C2c item 4): `wow_bridge_setup_native`
// backs BOTH this and `wowBridgeSetup` below -- they are aliases for the
// identical bash arm (`bridge-setup|party-setup|setup)`), so one native
// command covers both call sites.
export const wowPartySetup = async (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_bridge_setup_native", { onEvent: ch })
    : invoke("wow_party_setup", { onEvent: ch });
};

export interface GmLevelResult { leveled: boolean; player: string; level: number; }
export interface GmGoldResult { gold_set: boolean; player: string; gold: number; }
export interface GmHealResult { healed: boolean; player: string; }
export interface GmReviveResult { revived: boolean; player: string; }
export interface GmSummonResult { summoned: boolean; player: string; entry: number; npc: string; }

export async function wowGmLevel(player: string, level: number): Promise<GmLevelResult> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_gm_level_native", { player, level })
    : invoke("wow_gm_level", { player, level });
}
export async function wowGmGold(player: string, gold: number): Promise<GmGoldResult> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_gm_gold_native", { player, gold })
    : invoke("wow_gm_gold", { player, gold });
}
export async function wowGmHeal(player: string): Promise<GmHealResult> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_gm_heal_native", { player }) : invoke("wow_gm_heal", { player });
}
export async function wowGmRevive(player: string): Promise<GmReviveResult> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_gm_revive_native", { player }) : invoke("wow_gm_revive", { player });
}
export async function wowGmSummon(player: string, entry: number): Promise<GmSummonResult> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_gm_summon_native", { player, entry })
    : invoke("wow_gm_summon", { player, entry });
}
export async function wowGmAtLogin(
  player: string,
  flag: "rename" | "customize" | "changerace" | "changefaction",
): Promise<{ applied: boolean; player: string; flag: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_gm_at_login_native", { player, flag })
    : invoke("wow_gm_at_login", { player, flag });
}
// Batch 4 C: send a stuck character to their hearth/home (`.unstuck … inn`).
// Works for offline characters too. NB the native command's arg is named
// `player` (not `charName` like the WSL command / A2c report) -- both sides
// take the same character name string, just a different invoke key.
export async function wowGmReturnHome(charName: string): Promise<{ sent_home: boolean; player: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_gm_return_home_native", { player: charName })
    : invoke("wow_gm_return_home", { charName });
}

export interface BotcmdResult { sent: boolean; player: string; bot: string; action: string; }
export interface PresetInfo { name: string; bots: number; }
export interface PresetSaveResult { saved: boolean; name: string; bots: string[]; overwrote: boolean; }

export async function wowPartyBotcmd(
  player: string,
  bot: string,
  action: "gear" | "talents" | "maintain" | "spec",
  spec?: string,
): Promise<BotcmdResult> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_botcmd_native", { player, bot, action, spec })
    : invoke("wow_party_botcmd", { player, bot, action, spec });
}
export async function wowPartyPresetSave(player: string, name: string): Promise<PresetSaveResult> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_preset_save_native", { player, name })
    : invoke("wow_party_preset_save", { player, name });
}
export async function wowPartyPresetList(): Promise<PresetInfo[]> {
  const mode = await resolveBackendMode();
  const d = await (mode === "native"
    ? invoke<{ presets: PresetInfo[] }>("wow_party_preset_list_native")
    : invoke<{ presets: PresetInfo[] }>("wow_party_preset_list"));
  return d.presets;
}
export async function wowPartyPresetDelete(name: string): Promise<{ deleted: boolean; name: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_preset_delete_native", { name })
    : invoke("wow_party_preset_delete", { name });
}
export const wowPartyPresetLoad = async (
  player: string,
  name: string,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_preset_load_native", { player, name, onEvent: ch })
    : invoke("wow_party_preset_load", { player, name, onEvent: ch });
};
export async function wowPartyPresetShow(name: string): Promise<{ name: string; classes: string[] }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_preset_show_native", { name })
    : invoke("wow_party_preset_show", { name });
}
export async function wowPartyPresetImport(
  name: string,
  classes: string,
  force?: boolean,
): Promise<{ imported: boolean; name: string; classes: string[] }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_party_preset_import_native", { name, classes, force })
    : invoke("wow_party_preset_import", { name, classes, force });
}

export const wowBridgeSetup = async (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_bridge_setup_native", { onEvent: ch })
    : invoke("wow_bridge_setup", { onEvent: ch });
};

// Batch 4: a lightweight per-snapshot content summary recorded at backup
// time (a `.meta` sidecar). Older backups predate it, so `summary` is null.
export interface BackupSummary { characters: number; accounts: number; bots: number | null; }
// Backup display names: the sidecar's optional user-typed (or
// auto-generated "Backup #N" / "Auto (stop)" / "Auto (6h)") label.
// `name` is `undefined` under WSL (the CLI's own `backup list` row has no
// such key at all) and `null` on a legacy native sidecar written before this
// field existed -- callers should treat both the same way (no name).
export interface BackupInfo { file: string; size: number; created: string; world: boolean; summary: BackupSummary | null; name?: string | null; }

// Native-mode routing (Chunk 2, task C2a): `create`/`list`/`validate`/
// `delete` all have native siblings (direct `docker exec … mysqldump` +
// `flate2` gzip / plain `std::fs`, no `dml` shell-out); `restore` stays
// WSL-only (out of scope for this task -- see `dml::backup`'s module doc
// comment on why restore is the one sanctioned whole-DB-overwrite path).
export async function wowBackupList(): Promise<BackupInfo[]> {
  const mode = await resolveBackendMode();
  const d =
    mode === "native"
      ? await invoke<{ backups: BackupInfo[] }>("wow_backup_list_native")
      : await invoke<{ backups: BackupInfo[] }>("wow_backup_list");
  return d.backups;
}
export async function wowBackupDelete(file: string): Promise<{ deleted: boolean; file: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_backup_delete_native", { file })
    : invoke("wow_backup_delete", { file });
}
export interface BackupValidation {
  valid: boolean;
  file: string;
  size: number;
  gzip_ok: boolean;
  sql_ok: boolean;
  markers: string[];
  detail: string;
}
// Batch 4 A: verify a backup before restoring it (gzip integrity + a light
// SQL-sanity scan for the character/account tables). Read-only, no server.
export async function wowBackupValidate(file: string): Promise<BackupValidation> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_backup_validate_native", { file })
    : invoke("wow_backup_validate", { file });
}
// `name` (backup display names) is native-only: the CLI has no `--name`
// flag, so the WSL invoke below never receives one -- Backups.svelte hides/
// disables the name input in WSL mode accordingly.
export const wowBackupCreate = async (onEvent: (e: TermEvent) => void, includeWorld?: boolean, name?: string): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_backup_create_native", { includeWorld, name, onEvent: ch })
    : invoke("wow_backup_create", { includeWorld, onEvent: ch });
};
// Native-mode routing (Chunk 4b): `wow_backup_restore_native` drives the
// stop -> pre-restore safety dump -> streamed gunzip-import -> start sequence
// directly (no `dml` shell-out) -- same file/Channel contract either way; no
// `--yes` on either backend, the launcher's own two-click confirm is the gate.
export const wowBackupRestore = async (file: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_backup_restore_native", { file, onEvent: ch })
    : invoke("wow_backup_restore", { file, onEvent: ch });
};

// --- Auction House repair (Batch 4 F14) ------------------------------------
// Streams the `wow ahbot repair` NDJSON flow (character lookup + conf writes;
// creating the bot's account/character stays a manual user step, surfaced in
// the stream's done payload).
// Native-mode routing (Chunk 2, task C2c item 8): `wow_ahbot_repair_native`
// ports the same flow directly (direct DB lookup + `dml::config::conf_write`
// + SOAP), no `dml` shell-out.
export const wowAhbotRepair = async (charName: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_ahbot_repair_native", { charName, onEvent: ch })
    : invoke("wow_ahbot_repair", { charName, onEvent: ch });
};

export interface TitleInfo {
  id: string;
  name: string;
  installed: boolean;
  running: "running" | "stopped" | null;
  script_available: boolean;
}
export async function gamesCatalog(): Promise<TitleInfo[]> {
  const d = await invoke<{ titles: TitleInfo[] }>("games_catalog");
  return d.titles;
}
export interface InstallEvent {
  event: "chunk" | "exit";
  text?: string;
  code?: number;
}
export const gamesInstall = (id: string, onEvent: (e: InstallEvent) => void): Promise<void> => {
  const ch = new Channel<InstallEvent>();
  ch.onmessage = onEvent;
  return invoke("games_install", { id, onEvent: ch });
};
// Batch 4 F16: install a community title from a pasted https git URL --
// streams the interactive `dml run <url>` through the same single global
// install session (reply input + Cancel work unchanged).
export const urlInstall = (url: string, onEvent: (e: InstallEvent) => void): Promise<void> => {
  const ch = new Channel<InstallEvent>();
  ch.onmessage = onEvent;
  return invoke("url_install", { url, onEvent: ch });
};
export async function gamesInstallInput(text: string): Promise<void> {
  return await invoke("games_install_input", { text });
}
export async function gamesInstallCancel(): Promise<void> {
  return await invoke("games_install_cancel");
}
// keepData (Batch 3 F13c): preserve the ~6 GB client-data docker volume so a
// later reinstall skips the big download.
// removeImages (Batch 6 B): ALSO delete the AzerothCore/MySQL docker images
// (~3-5 GB) the title used. Default off -- kept for a fast reinstall.
// Native-mode routing (Chunk 4a): `games_remove_native` hardcodes the same
// confirm=true semantics as this WSL sibling's hardcoded `--yes` (the
// typed-id UI is the user gate either way) -- same keepData/removeImages/
// Channel contract.
export const gamesRemove = async (
  id: string,
  onEvent: (e: TermEvent) => void,
  keepData?: boolean,
  removeImages?: boolean,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("games_remove_native", { id, keepData, removeImages, onEvent: ch })
    : invoke("games_remove", { id, keepData, removeImages, onEvent: ch });
};

// --- LAN / doctor / tool-install / shell (Round Q Tools page) --------------

export type LanAction = "on" | "off" | "status" | "refresh";

// Text-mode CLI output (dml lan / dml doctor print plain status lines, not
// a JSON envelope) -- both return the raw combined stdout+stderr as a string
// for display in a <pre>, same shape either way.
//
// Native-mode routing (Chunk 2, task C2c item 3): `wow_lan_native` is
// AC-only (direct MySQL against acore_auth.realmlist, no `docker exec`) --
// same text shape, so this needs no branching beyond which command to call.
// `local` (internet-play LAN fix): this host's LAN address, written to
// realmlist.localAddress so players INSIDE the house keep reaching the world
// server while `address` advertises a public IP/hostname. Omit it and the
// realm address is the only thing touched (previous behaviour); `off` always
// reverts localAddress to 127.0.0.1 regardless.
export async function wowLan(
  action: LanAction,
  ip?: string,
  internet?: boolean,
  local?: string,
): Promise<string> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? invoke("wow_lan_native", { action, ip, internet, local })
    : invoke("wow_lan", { action, ip, internet, local });
}

// Best-effort public IPv4 (Batch 4 F15) -- null means "couldn't tell",
// never an error.
export async function wowLanPublicIp(): Promise<string | null> {
  const mode = await resolveBackendMode();
  const d =
    mode === "native"
      ? await invoke<{ public_ip: string | null }>("wow_lan_public_ip_read")
      : await invoke<{ public_ip: string | null }>("wow_lan_public_ip");
  return d.public_ip;
}

// --- Tailscale "Play Together" (Batch 5 overnight) -------------------------

export type TailscaleAction = "install" | "up" | "status" | "down";

export interface TailscaleInstall {
  installed: boolean;
  already: boolean;
}
export interface TailscaleUp {
  connected: boolean;
  // The first-time login URL for the user to open in any browser; null once
  // authenticated. This is the ONE genuinely interactive step.
  auth_url: string | null;
  // The 100.x tailnet IP to share with friends (set once connected).
  ip: string | null;
  daemon: string;
  firewall: string;
}
export interface TailscaleStatus {
  connected: boolean;
  ip: string | null;
  backend_state: string | null;
  status_text: string;
}

// Overloads keep each action's payload typed at the call site.
export async function wowTailscale(action: "install"): Promise<TailscaleInstall>;
export async function wowTailscale(action: "up"): Promise<TailscaleUp>;
export async function wowTailscale(action: "status"): Promise<TailscaleStatus>;
export async function wowTailscale(action: "down"): Promise<{ down: boolean }>;
export async function wowTailscale(action: TailscaleAction): Promise<unknown> {
  return await invoke("wow_tailscale", { action });
}

// --- LAN port diagnostic + MySQL LAN exposure (Batch 5 overnight) ----------

export interface PortBinding {
  name: string;
  service: "login" | "world" | "database";
  internal: number;
  published: boolean;
  host_ip: string | null;
  host_port: number | null;
  lan_ready: boolean;
}
export interface PortCheck {
  running: boolean;
  game_lan_ready: boolean;
  db_host_port: number;
  db_lan_exposed: boolean;
  ports: PortBinding[];
}

// Read-only diagnostic: how Docker publishes the game/DB ports. Native mode
// routes to the direct `docker port` `_read` sibling (`dml::maint`); WSL
// mode keeps shelling `dml` byte-identically.
export async function wowPortCheck(): Promise<PortCheck> {
  const mode = await resolveBackendMode();
  return mode === "native" ? invoke("wow_port_check_read") : invoke("wow_port_check");
}

export async function dmlDoctor(): Promise<string> {
  return await invoke("dml_doctor");
}

export type ToolName = "unbound" | "unbound-remove";

// Mirrors gamesInstall's shape exactly: same InstallEvent stream, same
// single global interactive session (games_install_input/games_install_cancel
// work against it unchanged).
export const toolInstall = (tool: ToolName, onEvent: (e: InstallEvent) => void): Promise<void> => {
  const ch = new Channel<InstallEvent>();
  ch.onmessage = onEvent;
  return invoke("tool_install", { tool, onEvent: ch });
};

export async function openShell(): Promise<void> {
  return await invoke("open_shell");
}

export async function detectLanIp(): Promise<string | null> {
  return await invoke("detect_lan_ip");
}

// --- Realmlist check + one-click fix (Batch 2 F7) --------------------------

export interface RealmlistStatus {
  client_path: string | null;
  windows_path: string | null;
  path: string | null;
  exists: boolean;
  readonly: boolean;
  content: string;
  current: string | null;
  config_wtf: string | null;
  expected: string[];
  matches: boolean;
}

// lanIp is comparison data only (adds the LAN address to `expected`); the
// backend validates it and derives every path from the stored client folder.
export async function realmlistStatus(lanIp?: string): Promise<RealmlistStatus> {
  return await invoke("realmlist_status", { lanIp });
}

export async function realmlistFix(target: string, lanIp?: string): Promise<RealmlistStatus> {
  return await invoke("realmlist_fix", { target, lanIp });
}

export async function realmlistLock(locked: boolean, lanIp?: string): Promise<RealmlistStatus> {
  return await invoke("realmlist_lock", { locked, lanIp });
}

// --- Windows disk & performance tools (Batch 4 F17) -------------------------

export interface WslConfigState {
  path: string;
  exists: boolean;
  memory: string | null;
  processors: string | null;
}

export async function wslconfigRead(): Promise<WslConfigState> {
  return await invoke("wslconfig_read");
}

// Only provided fields are written; unrelated .wslconfig lines/sections are
// preserved. Takes effect after WSL restarts.
export async function wslconfigWrite(memory?: string, processors?: string): Promise<WslConfigState> {
  return await invoke("wslconfig_write", { memory, processors });
}

export async function restartWsl(): Promise<{
  shutdown: boolean;
  stopped_server: boolean;
  stop_attempted: boolean;
}> {
  return await invoke("restart_wsl");
}

// Writes the shrink script into Downloads and opens Explorer at it; returns
// the script path.
export async function generateCompactScript(): Promise<string> {
  return await invoke("generate_compact_script");
}

// Batch 5 (overnight): writes the "expose MySQL to LAN" admin PowerShell
// script into Downloads and opens Explorer at it; returns the script path.
// The port is the DB host port from the diagnostic (defaults to 3306).
export async function generateMysqlProxyScript(port?: number): Promise<string> {
  return await invoke("generate_mysql_proxy_script", { port });
}

export async function defenderHint(): Promise<{ vhdx_dir: string | null; command: string | null }> {
  return await invoke("defender_hint");
}

// --- Native-mode setup bootstrap (spike/docker-desktop-native) -------------
// Read-only status the "Native setup" Tools card loads on mount, plus the
// one-click fixes. `native` is true only when DML_BACKEND=native — the card
// renders only then, so WSL mode stays uncluttered. The three mutating fixes
// (yqInstall / soapCopy / defenderScript) are LOCKED frontend-side behind the
// native-setup feature lock; startDockerDesktop just launches the app.
export interface NativeSetupStatus {
  native: boolean;
  docker: { running: boolean; path: string | null };
  yq: { present: boolean; path: string };
  soap: { present: boolean; path: string; distro_available: boolean };
}

export async function nativeSetupStatus(): Promise<NativeSetupStatus> {
  return await invoke("native_setup_status");
}

export async function startDockerDesktop(): Promise<{ launched: boolean; path: string }> {
  return await invoke("start_docker_desktop");
}

// Incident follow-up 1 (2026-07-21): restart the Docker DAEMON inside dml-arch
// (`dml wow docker-restart`). The WSL-mode twin of startDockerDesktop above --
// same user problem (the engine is wedged), different machinery.
//
// DESTRUCTIVE: every running container goes down with the daemon, so the Tools
// card gates it behind a typed confirmation. Rejects with the CLI's own codes:
// NOT_SUPPORTED (no systemd) / NO_SUDO (no passwordless sudo) / RESTART_FAILED
// / RESTART_TIMEOUT (a blocking call hit its bound -- raised for BOTH a wedged
// systemd and a slow daemon, which need opposite advice, so its copy must quote
// the hint) / DOCKER_STILL_DOWN (the daemon did not answer again within the
// CLI's bounded wait) -- docker-restart.ts turns each into copy. Resolving means dockerd
// answered again, not merely that systemctl returned.
export async function wowDockerRestart(): Promise<{ restarted: boolean }> {
  return await invoke("wow_docker_restart");
}

export async function nativeYqInstall(): Promise<{ installed: boolean; path: string; bytes: number }> {
  return await invoke("native_yq_install");
}

export async function nativeSoapCopy(): Promise<{ copied: boolean; path: string }> {
  return await invoke("native_soap_copy");
}

// Writes the elevated Defender-exclusion script into Downloads and opens
// Explorer at it; returns the script path (run it as Administrator yourself).
export async function nativeDefenderScript(): Promise<string> {
  return await invoke("native_defender_script");
}

// --- Enrichment-cache maintenance (Batch 6 C) ------------------------------
// Two RUNTIME caches (safe to wipe -- they repopulate on demand): the
// Windows-side 3D-model/icon cache (zam) and the WSL-side item tooltip/icon
// cache (wowhead). Committed datasets (talent trees, achievements) are
// bundled into the binary and never appear here.
export interface CacheEntry {
  key: string;
  label: string;
  path: string;
  present: boolean;
  bytes: number;
  files: number;
}
export async function zamCacheStatus(): Promise<CacheEntry> {
  return await invoke("zam_cache_status");
}
export async function zamCacheClear(): Promise<{ cleared: boolean; freed_bytes: number }> {
  return await invoke("zam_cache_clear");
}
export async function wowCacheStatus(): Promise<{ caches: CacheEntry[] }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_cache_status_read")
    : await invoke("wow_cache_status");
}
export async function wowCacheClean(): Promise<{ wiped: boolean; freed_bytes: number; path: string }> {
  const mode = await resolveBackendMode();
  return mode === "native"
    ? await invoke("wow_cache_clean_native")
    : await invoke("wow_cache_clean");
}

// --- Keep-awake sleep block (Batch 2 F6) -----------------------------------

export async function setKeepAwake(on: boolean): Promise<void> {
  return await invoke("set_keep_awake", { on });
}

// Push the polled verdict to Rust so the tray can show it with the window
// hidden. Rust has no status poller of its own -- see the tray_set_status doc
// comment for why duplicating the poll there would be wrong.
export async function traySetStatus(verdict: string): Promise<void> {
  return await invoke("tray_set_status", { verdict });
}

// Start-with-Windows, backed by an HKCU\...\Run entry. `autostartGet` reports
// false for an entry whose recorded exe no longer exists, so a stale entry
// from a deleted build does not show as enabled.
export async function autostartGet(): Promise<boolean> {
  return await invoke<boolean>("autostart_get");
}
export async function autostartSet(on: boolean): Promise<void> {
  return await invoke("autostart_set", { on });
}

// --- Auto-shutdown watcher (Batch 2 F5) ------------------------------------

// Progress arrives separately via the "auto-shutdown" tauri event channel
// (see auto-shutdown.svelte.ts); this just flips the backend watcher.
export async function setAutoShutdown(enabled: boolean): Promise<void> {
  return await invoke("set_auto_shutdown", { enabled });
}
