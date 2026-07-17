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
export type RawFileName =
  | ".env"
  | "docker-compose.override.yml"
  | "playerbots.conf"
  | "mod_ahbot.conf"
  | "mod_ale.conf";

export async function wowAccounts(): Promise<Account[]> {
  const data = await invoke<{ accounts: Account[] }>("wow_accounts");
  return data.accounts;
}
export async function wowServerInfo(): Promise<ServerInfo> {
  return await invoke("wow_server_info");
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
export async function wowPaperdoll(charName: string): Promise<PaperdollData> {
  return await invoke("wow_paperdoll", { charName });
}
export async function wowConfigList(): Promise<ConfigSetting[]> {
  const data = await invoke<{ settings: ConfigSetting[] }>("wow_config_list");
  return data.settings;
}
export async function wowConfigSet(
  key: string,
  value: string,
): Promise<{ changed: boolean; restart_required: boolean }> {
  return await invoke("wow_config_set", { key, value });
}
export async function wowConfigRawRead(
  file: RawFileName,
): Promise<{ file: string; content: string }> {
  return await invoke("wow_config_raw_read", { file });
}
export async function wowConfigRawWrite(
  file: RawFileName,
  content: string,
): Promise<{ written: boolean; backup: string | null }> {
  return await invoke("wow_config_raw_write", { file, content });
}
export const gamesRestart = (id: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("games_restart", { id, onEvent: ch });
};

export interface OnlineChar { guid: number; name: string; class: number; level: number; }
export interface PartyMember { guid: number; name: string; class: number; level: number; is_bot: boolean; }
export interface PartyAddResult { added: boolean; joined: boolean; bot: string | null; note: string | null; }

export async function wowPartyOnline(): Promise<OnlineChar[]> {
  const d = await invoke<{ online: OnlineChar[] }>("wow_party_online");
  return d.online;
}
export async function wowPartyAdd(player: string, className: string, gender?: string): Promise<PartyAddResult> {
  return await invoke("wow_party_add", { player, class: className, gender });
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

export interface BotcmdResult { sent: boolean; player: string; bot: string; action: string; }
export interface PresetInfo { name: string; bots: number; }
export interface PresetSaveResult { saved: boolean; name: string; bots: string[]; overwrote: boolean; }

export async function wowPartyBotcmd(player: string, bot: string, action: "gear" | "talents" | "maintain"): Promise<BotcmdResult> {
  return await invoke("wow_party_botcmd", { player, bot, action });
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

export const wowBridgeSetup = (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_bridge_setup", { onEvent: ch });
};

export interface BackupInfo { file: string; size: number; created: string; }

export async function wowBackupList(): Promise<BackupInfo[]> {
  const d = await invoke<{ backups: BackupInfo[] }>("wow_backup_list");
  return d.backups;
}
export async function wowBackupDelete(file: string): Promise<{ deleted: boolean; file: string }> {
  return await invoke("wow_backup_delete", { file });
}
export const wowBackupCreate = (onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_backup_create", { onEvent: ch });
};
export const wowBackupRestore = (file: string, onEvent: (e: TermEvent) => void): Promise<void> => {
  const ch = new Channel<TermEvent>();
  ch.onmessage = onEvent;
  return invoke("wow_backup_restore", { file, onEvent: ch });
};
