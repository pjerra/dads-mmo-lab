// Gear sets light (Batch 5 F4): save a character's equipped set (from the
// paperdoll) as a named local set; mail any saved set to any character via
// the existing mail-item arm. Entirely frontend -- localStorage + the
// already-shipped `wow mail-item` CLI/SOAP path; no new backend surface.
//
// Server-side facts this leans on (verified against cs_send.cpp):
// `.send items` creates FRESH item copies with no soulbind/bonding check
// (BoP/heirloom/quest items all sendable); enchants/gems are NOT carried
// (the paperdoll only stores entry ids anyway); offline recipients work;
// hard cap 12 item stacks per mail -- hence the chunking below.
//
// Reactive module-level store (Svelte 5 runes in a .svelte.ts module, same
// pattern as restart-state/features) so Dashboard's Save button and Items'
// Gear sets card stay in sync without prop-drilling. Pure helpers are
// exported separately for vitest (node env, no localStorage).

import type { PaperdollData } from "$lib/api";
import { wowMailItem } from "$lib/api";

export interface GearSetItem {
  slot: number;
  entry: number;
  name: string;
  quality: number;
}

export interface GearSet {
  name: string;
  sourceChar: string;
  class: number;
  level: number;
  capturedAt: number;
  items: GearSetItem[];
}

export const GEARSETS_KEY = "dml.gearsets.v1";

// `.send items` hard cap (MAX_MAIL_ITEMS): 12 stacks per mail.
export const MAIL_CHUNK = 12;

function hasStorage(): boolean {
  try {
    return typeof localStorage !== "undefined";
  } catch {
    return false;
  }
}

// --- pure helpers (vitest targets) -----------------------------------------

// Parse storage content into a clean GearSet[]. Garbage (bad JSON, not an
// array, malformed entries) degrades to [] / gets dropped -- a corrupted
// entry must never break the Items page.
export function parseGearSets(raw: string | null): GearSet[] {
  if (!raw) return [];
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return [];
  }
  if (!Array.isArray(parsed)) return [];
  const out: GearSet[] = [];
  for (const v of parsed) {
    const s = v as Partial<GearSet>;
    if (typeof s?.name !== "string" || !s.name) continue;
    if (!Array.isArray(s.items)) continue;
    const items: GearSetItem[] = [];
    for (const it of s.items) {
      const i = it as Partial<GearSetItem>;
      if (typeof i?.entry !== "number" || !Number.isInteger(i.entry) || i.entry <= 0) continue;
      items.push({
        slot: typeof i.slot === "number" ? i.slot : 0,
        entry: i.entry,
        name: typeof i.name === "string" ? i.name : `item ${i.entry}`,
        quality: typeof i.quality === "number" ? i.quality : 1,
      });
    }
    out.push({
      name: s.name.slice(0, 32),
      sourceChar: typeof s.sourceChar === "string" ? s.sourceChar : "?",
      class: typeof s.class === "number" ? s.class : 0,
      level: typeof s.level === "number" ? s.level : 0,
      capturedAt: typeof s.capturedAt === "number" ? s.capturedAt : 0,
      items,
    });
  }
  return out;
}

// Capture a paperdoll as a GearSet. Empty slots are never present in the
// paperdoll (INNER JOIN); shirt/tabard ARE captured (harmless to send).
export function gearSetFromDoll(doll: PaperdollData, name: string): GearSet {
  return {
    name: name.trim().slice(0, 32),
    sourceChar: doll.name,
    class: doll.class,
    level: doll.level,
    capturedAt: Date.now(),
    items: doll.equipped.map((it) => ({
      slot: it.slot,
      entry: it.entry,
      name: it.name,
      quality: it.quality,
    })),
  };
}

// Mail specs: ALWAYS count 1 per equipped slot -- unique-equipped items have
// MaxCount 1, so duplicate ring/trinket entries must go as two "entry:1"
// specs, never merged into "entry:2".
export function buildSpecs(items: GearSetItem[]): string[] {
  return items.map((it) => `${it.entry}:1`);
}

// ≤12 specs per mail (19 slots max -> at most 2 mails).
export function chunkSpecs(specs: string[]): string[][] {
  const chunks: string[][] = [];
  for (let i = 0; i < specs.length; i += MAIL_CHUNK) {
    chunks.push(specs.slice(i, i + MAIL_CHUNK));
  }
  return chunks;
}

export interface MailPlanEntry {
  items: string; // "entry:1,entry:1,..."
  subject: string;
}

export function planMails(set: GearSet, _to: string): MailPlanEntry[] {
  const chunks = chunkSpecs(buildSpecs(set.items));
  const n = chunks.length;
  return chunks.map((c, i) => ({
    items: c.join(","),
    subject: `Gear set: ${set.name} (${i + 1}/${n})`,
  }));
}

export interface MailOutcome {
  sent: number; // mails delivered
  total: number;
  error: string | null; // null = full success
}

// Sequential sender: one mail at a time (single SOAP console, clean error
// attribution -- NEVER parallel). On a failure it stops and reports honestly
// that earlier mails were already delivered -- there is no rollback.
export async function runSequential(
  plan: MailPlanEntry[],
  send: (entry: MailPlanEntry) => Promise<void>,
  onProgress?: (sent: number, total: number) => void,
): Promise<MailOutcome> {
  let sent = 0;
  for (const entry of plan) {
    try {
      await send(entry);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      const msg = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      return {
        sent,
        total: plan.length,
        error: `mail ${sent + 1} of ${plan.length} failed: ${msg}${
          sent > 0 ? "; the earlier mails were already delivered (no rollback exists)" : ""
        }`,
      };
    }
    sent++;
    onProgress?.(sent, plan.length);
  }
  return { sent, total: plan.length, error: null };
}

// --- reactive store + storage-backed API ------------------------------------

function readStored(): GearSet[] {
  try {
    return hasStorage() ? parseGearSets(localStorage.getItem(GEARSETS_KEY)) : [];
  } catch {
    return [];
  }
}

function writeStored(sets: GearSet[]): void {
  try {
    if (hasStorage()) localStorage.setItem(GEARSETS_KEY, JSON.stringify(sets));
  } catch {
    // Storage unavailable -- sets just don't persist this session.
  }
}

const store = $state({ sets: readStored() });

export function listGearSets(): GearSet[] {
  return store.sets;
}

// Save (replace-by-name) the doll's equipped set. Returns the saved set.
export function saveGearSet(doll: PaperdollData, name: string): GearSet {
  const set = gearSetFromDoll(doll, name);
  store.sets = [...store.sets.filter((s) => s.name !== set.name), set];
  writeStored(store.sets);
  return set;
}

export function deleteGearSet(name: string): void {
  store.sets = store.sets.filter((s) => s.name !== name);
  writeStored(store.sets);
}

// Batch 4 D: add/replace an already-built GearSet (e.g. one parsed from an
// imported TOML block). Same replace-by-name + persist as saveGearSet, but
// from an existing set rather than a live paperdoll.
export function addGearSet(set: GearSet): GearSet {
  store.sets = [...store.sets.filter((s) => s.name !== set.name), set];
  writeStored(store.sets);
  return set;
}

// Mail a saved set to a character, sequentially, ≤12 items per mail.
export async function mailGearSet(
  to: string,
  set: GearSet,
  onProgress?: (sent: number, total: number) => void,
): Promise<MailOutcome> {
  const plan = planMails(set, to);
  return runSequential(
    plan,
    async (entry) => {
      await wowMailItem({ to, items: entry.items, subject: entry.subject });
    },
    onProgress,
  );
}
