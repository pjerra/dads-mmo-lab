import { invoke, Channel } from "@tauri-apps/api/core";

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

function streamAction(cmd: "games_start" | "games_stop") {
  return (id: string, onEvent: (e: TermEvent) => void): Promise<void> => {
    const ch = new Channel<TermEvent>();
    ch.onmessage = onEvent;
    return invoke(cmd, { id, onEvent: ch });
  };
}

export const gamesStart = streamAction("games_start");
export const gamesStop = streamAction("games_stop");

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
export async function wowAccountCreate(user: string, pass: string): Promise<{ created: boolean; user: string }> {
  return await invoke("wow_account_create", { user, pass });
}
export async function wowAccountSetPassword(user: string, pass: string): Promise<{ password_set: boolean; user: string }> {
  return await invoke("wow_account_set_password", { user, pass });
}
export async function wowAccountSetGm(user: string, level: number): Promise<{ gm_set: boolean; user: string; level: number }> {
  return await invoke("wow_account_set_gm", { user, level });
}
export async function wowAccountDelete(user: string): Promise<{ deleted: boolean; user: string }> {
  return await invoke("wow_account_delete", { user });
}
export async function wowServerInfo(): Promise<ServerInfo> {
  return await invoke("wow_server_info");
}
export async function wowServerDetail(): Promise<ServerDetail> {
  return await invoke("wow_server_detail");
}
export async function wowDockerUsage(): Promise<{ lines: string[] }> {
  return await invoke("wow_docker_usage");
}
export const wowDockerClean = (level: number, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_docker_clean", { level, onEvent: ch });
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
  return await invoke("wow_update_check");
}
export const wowServerUpdate = (backup: boolean, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_server_update", { backup, onEvent: ch });
};
export interface ConsoleTail {
  available: boolean;
  lines: string[];
}
export async function wowConsoleTail(lines?: number): Promise<ConsoleTail> {
  return await invoke("wow_console_tail", { lines });
}
export async function wowConsoleSend(command: string): Promise<{ result: string }> {
  return await invoke("wow_console_send", { command });
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

export async function wowCommands(): Promise<ModCommands[]> {
  const data = await invoke<{ mods: ModCommands[] }>("wow_commands");
  return data.mods;
}

export const wowModuleInstall = (
  family: string,
  key: string | null,
  url: string | null,
  onEvent: (e: TermEvent) => void,
  backup?: boolean,
  variant?: string,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_module_install", { family, key, url, backup, variant, onEvent: ch });
};
export const wowModuleRemove = (
  family: string,
  key: string,
  onEvent: (e: TermEvent) => void,
  backup?: boolean,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_module_remove", { family, key, backup, onEvent: ch });
};
export const wowModuleRebuild = (
  backup: boolean,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_module_rebuild", { backup, onEvent: ch });
};
// Batch 5 F2: ARAC server-DBC + client-MPQ patch stream (key is allowlisted
// CLI-side to mod-arac).
export const wowModuleClientPatch = (
  key: string,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_module_client_patch", { key, onEvent: ch });
};
export async function wowModuleConfActivate(
  key: string,
  force?: boolean,
): Promise<{ key: string; activated: boolean; conf_name: string }> {
  return await invoke("wow_module_conf_activate", { key, force });
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
  return await invoke("wow_module_tracking", { key });
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
  mode: "mark" | "clear",
  files?: string,
): Promise<ModuleRepair> {
  return await invoke("wow_module_repair", { key, db, mode, files });
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
  return await invoke("wow_module_fixit", { key });
}

export interface ClientPath {
  path: string | null;
  valid: boolean;
}
export async function wowClientPathGet(): Promise<ClientPath> {
  return await invoke("wow_client_path_get");
}
export async function wowClientPathSet(path: string): Promise<ClientPath> {
  return await invoke("wow_client_path_set", { path });
}
export async function wowClientPathDetect(): Promise<{ candidates: string[] }> {
  return await invoke("wow_client_path_detect");
}

export async function wowItemsSearch(p: {
  name: string;
  quality?: number;
  minLevel?: number;
  maxLevel?: number;
}): Promise<ItemRow[]> {
  const data = await invoke<{ items: ItemRow[] }>("wow_items_search", p);
  return data.items;
}
export async function wowMailItem(p: {
  to: string;
  items: string;
  subject?: string;
  body?: string;
}): Promise<{ sent: boolean; to: string; attachments: number }> {
  return await invoke("wow_mail_item", p);
}
export async function wowTeleportList(search?: string): Promise<TeleLocation[]> {
  const data = await invoke<{ locations: TeleLocation[] }>("wow_teleport_list", { search });
  return data.locations;
}
export async function wowTeleport(
  charName: string,
  to: string,
): Promise<{ teleported: boolean; char: string; to: string }> {
  return await invoke("wow_teleport", { charName, to });
}
export async function wowTeleportCoords(
  charName: string,
  map: number,
  x: number,
  y: number,
  z: number,
): Promise<{ teleported: boolean; char: string; map: number; x: number; y: number; z: number }> {
  return await invoke("wow_teleport_coords", { charName, map, x, y, z });
}
export async function wowPaperdoll(charName: string): Promise<PaperdollData> {
  return await invoke("wow_paperdoll", { charName });
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
}
export async function wowItemInfo(entries: number[]): Promise<ItemInfo[]> {
  const d = await invoke<{ items: ItemInfo[] }>("wow_item_info", { entries });
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
  return await invoke("wow_char_progress", { charName });
}
export interface EarnedAchievements {
  earned: AchievementEntry[];
}
export async function wowAchievements(charName: string): Promise<EarnedAchievements> {
  return await invoke("wow_achievements", { charName });
}
export interface EntityInfo {
  id: number;
  source: "wowhead" | "unavailable";
  icon?: string | null;
  icon_b64?: string | null;
  wowhead?: WowheadTooltip;
}
export async function wowEntityInfo(kind: "spell" | "achievement", ids: number[]): Promise<EntityInfo[]> {
  const d = await invoke<{ entities: EntityInfo[] }>("wow_entity_info", { kind, ids });
  return d.entities;
}
export async function wowConfigList(): Promise<ConfigSetting[]> {
  const data = await invoke<{ settings: ConfigSetting[] }>("wow_config_list");
  return data.settings;
}
export async function wowConfigSet(
  key: string,
  value: string,
): Promise<{ changed: boolean; restart_required: boolean; applied?: "live" | "restart" | "none" }> {
  return await invoke("wow_config_set", { key, value });
}
export interface PbKey {
  key: string;
  value: string;
  default: string | null;
  line: number;
}
export async function wowConfigPbKeys(): Promise<{ source: string; keys: PbKey[] }> {
  return await invoke("wow_config_pb_keys");
}
export async function wowConfigFiles(): Promise<ConfFile[]> {
  const data = await invoke<{ files: ConfFile[] }>("wow_config_files");
  return data.files;
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
  return await invoke("wow_accountwide_get");
}
export async function wowAccountwideSet(
  key: string,
  value: "on" | "off",
  variant?: "default" | "custom",
): Promise<{ key: string; value: "on" | "off"; changed: boolean; reload: string; variant?: string }> {
  return await invoke("wow_accountwide_set", { key, value, variant });
}
export async function wowConfigRawRead(
  file: RawFileName,
): Promise<{ file: string; source?: "conf" | "dist"; content: string }> {
  return await invoke("wow_config_raw_read", { file });
}
export async function wowConfigRawReset(
  file: RawFileName,
): Promise<{ reset: boolean; file: string; backup: string | null }> {
  return await invoke("wow_config_raw_reset", { file });
}
export async function wowConfigRawWrite(
  file: RawFileName,
  content: string,
): Promise<{ written: boolean; backup: string | null }> {
  return await invoke("wow_config_raw_write", { file, content });
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
export const wowWorldRestart = (
  skipSaveall: boolean,
  onEvent: (e: TermEvent) => void,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_world_restart", { skipSaveall, onEvent: ch });
};
// Flush & rebuild the ambient bot population (Batch 1 F4). The CLI enforces
// --yes plus the typed ack itself; the GUI's typed-confirm gates calling this.
export const wowBotsFlush = (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_bots_flush", { onEvent: ch });
};

export interface OnlineChar { guid: number; name: string; class: number; level: number; }
// Batch 3 F11a: Home "players online" card row (bots excluded CLI-side).
export interface PlayerOnline { name: string; level: number; class: number; zone: number; }

export async function wowPlayersOnline(): Promise<PlayerOnline[]> {
  const d = await invoke<{ players: PlayerOnline[] }>("wow_players_online");
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
export interface PartyMember { guid: number; name: string; class: number; level: number; is_bot: boolean; }
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
  const d = await invoke<{ online: OnlineChar[] }>("wow_party_online");
  return d.online;
}
export async function wowPartyAdd(
  player: string,
  className: string,
  gender?: string,
  spec?: string,
): Promise<PartyAddResult> {
  return await invoke("wow_party_add", { player, class: className, gender, spec });
}
export async function wowPartyList(player: string): Promise<PartyMember[]> {
  const d = await invoke<{ members: PartyMember[] }>("wow_party_list", { player });
  return d.members;
}
export async function wowPartyKick(bot: string): Promise<{ kicked: boolean }> {
  return await invoke("wow_party_kick", { bot });
}
export async function wowPartyRelogin(player: string, bot: string): Promise<{ relogged: boolean }> {
  return await invoke("wow_party_relogin", { player, bot });
}
export const wowPartySetup = (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_party_setup", { onEvent: ch });
};

export interface GmLevelResult { leveled: boolean; player: string; level: number; }
export interface GmGoldResult { gold_set: boolean; player: string; gold: number; }
export interface GmHealResult { healed: boolean; player: string; }
export interface GmReviveResult { revived: boolean; player: string; }
export interface GmSummonResult { summoned: boolean; player: string; entry: number; npc: string; }

export async function wowGmLevel(player: string, level: number): Promise<GmLevelResult> {
  return await invoke("wow_gm_level", { player, level });
}
export async function wowGmGold(player: string, gold: number): Promise<GmGoldResult> {
  return await invoke("wow_gm_gold", { player, gold });
}
export async function wowGmHeal(player: string): Promise<GmHealResult> {
  return await invoke("wow_gm_heal", { player });
}
export async function wowGmRevive(player: string): Promise<GmReviveResult> {
  return await invoke("wow_gm_revive", { player });
}
export async function wowGmSummon(player: string, entry: number): Promise<GmSummonResult> {
  return await invoke("wow_gm_summon", { player, entry });
}
export async function wowGmAtLogin(
  player: string,
  flag: "rename" | "customize" | "changerace" | "changefaction",
): Promise<{ applied: boolean; player: string; flag: string }> {
  return await invoke("wow_gm_at_login", { player, flag });
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
  return await invoke("wow_party_botcmd", { player, bot, action, spec });
}
export async function wowPartyPresetSave(player: string, name: string): Promise<PresetSaveResult> {
  return await invoke("wow_party_preset_save", { player, name });
}
export async function wowPartyPresetList(): Promise<PresetInfo[]> {
  const d = await invoke<{ presets: PresetInfo[] }>("wow_party_preset_list");
  return d.presets;
}
export async function wowPartyPresetDelete(name: string): Promise<{ deleted: boolean; name: string }> {
  return await invoke("wow_party_preset_delete", { name });
}
export const wowPartyPresetLoad = (player: string, name: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_party_preset_load", { player, name, onEvent: ch });
};
export async function wowPartyPresetShow(name: string): Promise<{ name: string; classes: string[] }> {
  return await invoke("wow_party_preset_show", { name });
}
export async function wowPartyPresetImport(
  name: string,
  classes: string,
  force?: boolean,
): Promise<{ imported: boolean; name: string; classes: string[] }> {
  return await invoke("wow_party_preset_import", { name, classes, force });
}

export const wowBridgeSetup = (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_bridge_setup", { onEvent: ch });
};

export interface BackupInfo { file: string; size: number; created: string; world: boolean; }

export async function wowBackupList(): Promise<BackupInfo[]> {
  const d = await invoke<{ backups: BackupInfo[] }>("wow_backup_list");
  return d.backups;
}
export async function wowBackupDelete(file: string): Promise<{ deleted: boolean; file: string }> {
  return await invoke("wow_backup_delete", { file });
}
export const wowBackupCreate = (onEvent: (e: TermEvent) => void, includeWorld?: boolean): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_backup_create", { includeWorld, onEvent: ch });
};
export const wowBackupRestore = (file: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_backup_restore", { file, onEvent: ch });
};

// --- Auction House repair (Batch 4 F14) ------------------------------------
// Streams the `wow ahbot repair` NDJSON flow (character lookup + conf writes;
// creating the bot's account/character stays a manual user step, surfaced in
// the stream's done payload).
export const wowAhbotRepair = (charName: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_ahbot_repair", { charName, onEvent: ch });
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
export const gamesRemove = (
  id: string,
  onEvent: (e: TermEvent) => void,
  keepData?: boolean,
): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("games_remove", { id, keepData, onEvent: ch });
};

// --- LAN / doctor / tool-install / shell (Round Q Tools page) --------------

export type LanAction = "on" | "off" | "status" | "refresh";

// Text-mode CLI output (dml lan / dml doctor print plain status lines, not
// a JSON envelope) -- both return the raw combined stdout+stderr as a string
// for display in a <pre>, same shape either way.
export async function wowLan(action: LanAction, ip?: string, internet?: boolean): Promise<string> {
  return await invoke("wow_lan", { action, ip, internet });
}

// Best-effort public IPv4 (Batch 4 F15) -- null means "couldn't tell",
// never an error.
export async function wowLanPublicIp(): Promise<string | null> {
  const d = await invoke<{ public_ip: string | null }>("wow_lan_public_ip");
  return d.public_ip;
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

export async function defenderHint(): Promise<{ vhdx_dir: string | null; command: string | null }> {
  return await invoke("defender_hint");
}

// --- Keep-awake sleep block (Batch 2 F6) -----------------------------------

export async function setKeepAwake(on: boolean): Promise<void> {
  return await invoke("set_keep_awake", { on });
}

// --- Auto-shutdown watcher (Batch 2 F5) ------------------------------------

// Progress arrives separately via the "auto-shutdown" tauri event channel
// (see auto-shutdown.svelte.ts); this just flips the backend watcher.
export async function setAutoShutdown(enabled: boolean): Promise<void> {
  return await invoke("set_auto_shutdown", { enabled });
}
