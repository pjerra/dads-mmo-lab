// Adapter for embedding Wowhead's ZamModelViewer (wrath content tree) in a
// Tauri webview, driven by native WotLK display ids from the CLI's paperdoll
// output. The invocation shape (options object), the equipment-slot table,
// the character model-id formula, and the character-customization mechanism
// below are ported from `wow-model-viewer` (github.com/Miorey/wow-model-viewer,
// npm `wow-model-viewer`, version 1.5.3, ISC license, Copyright (c) Miorey) --
// specifically `index.js`/`character_modeling.js`/`wow_model_viewer.js` from
// that package, as captured verbatim in `.superpowers/sdd/recon-modelviewer.md`
// (see that file for full source extracts, CORS probe results, and the
// original porting verdict). EXCEPTION: the equipment-slot semantics were
// re-derived 2026-07-22 from a decompile of the live-tree viewer.min.js
// itself, which proved the reference README's slot table wrong -- see
// `.superpowers/sdd/model-browser-report.md` (the authority for the slot
// mapping, meta-path routing, and skip-count behavior in this file).
// `viewer.min.js` itself is Wowhead/ZAM's own proprietary build, loaded at
// runtime through the app's `zam` asset proxy -- not vendored here.
//
// ISC License (for the ported invocation/mapping logic in this file):
// Copyright (c) Miorey
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

import { invoke } from "@tauri-apps/api/core";
import type { PaperdollData } from "./api";

// jquery (v3.x) ships no bundled TypeScript types and this repo doesn't carry
// @types/jquery. viewer.min.js only touches a jQuery-wrapped container's own
// internals (recon §3) -- our own code never inspects one, it just builds one
// and hands it to the untyped ZamModelViewer constructor, so a minimal
// callable shim (typing only the seam we actually use, see ./jquery.d.ts) is
// enough here.
type JQueryStatic = (selector: string) => unknown;

// The viewer's own global surface (recon §3: a single IIFE-assigned global,
// `window.ZamModelViewer`, no other public export) is likewise untyped
// upstream -- the constructor's option shape and instance are both `unknown`
// here on purpose rather than modeled with a speculative interface.
interface ZamModelViewerConstructor {
  new (options: Record<string, unknown>): unknown;
}

declare global {
  interface Window {
    $: JQueryStatic;
    jQuery: JQueryStatic;
    // Recon §1.2/1.3: truthy by default upstream (a retail display-id
    // translation endpoint). We always target the native wrath content
    // tree, so per the recon's own verdict this must be left `undefined`
    // -- must be set BEFORE viewer.min.js executes (its customization
    // table branches on this at load time, not per-call).
    WOTLK_TO_RETAIL_DISPLAY_ID_API: string | undefined;
    // Wowhead's site-global environment that viewer.min.js assumes exists
    // (it does on wowhead.com). Without it the engine dies with a
    // ReferenceError ("WH is not defined") before its first asset fetch --
    // found live 2026-07-19; the stub below is the reference package's
    // setup.js ported verbatim.
    WH?: Record<string, unknown>;
    ZamModelViewer?: ZamModelViewerConstructor;
  }
}

// CORRECTED SLOT SEMANTICS (2026-07-22, decompile of the live-tree
// viewer.min.js -- see .superpowers/sdd/model-browser-report.md, which
// supersedes the recon's README-derived slot table): the items-array slot
// vocabulary is the WoW client **InventoryType** enum, NOT the equip-slot
// enum + 1. The reference README's table ("15=Back, 16=Main Hand,
// 17=Off Hand, 18=Ranged") is wrong for the real engine -- its own demo
// cloak `[15, 17238]` silently 404s. Wowhead's own Paperdoll.js (recon
// `updateItemViewer`) feeds the engine WH.Wow.Item.INVENTORY_TYPE_*
// constants, and the engine's internal display-slot table (`Er`) and
// attachment/geoset branches all key off InventoryType.
//
// acSlot+1 *coincidentally* equals InventoryType for slots 0-9 and 18,
// which is why body armor always rendered while back/weapons never did.
// Fixed (unambiguous) rows only -- chest (AC 4: chest 5 vs robe 20),
// off hand (AC 16: weapon/held 22 vs shield 14) and ranged (AC 17: bow 15 /
// thrown 25 / wand-gun 26) depend on the item and are resolved per-item in
// resolveViewerItems below. Neck (AC 1), rings (10/11), trinkets (12/13)
// and bags are never displayed by the engine and have no row.
export const AC_TO_INVENTORY_TYPE: Record<number, number> = {
  0: 1, // Head
  2: 3, // Shoulders
  3: 4, // Body (shirt)
  5: 6, // Waist
  6: 7, // Legs
  7: 8, // Feet
  8: 9, // Wrists
  9: 10, // Hands
  14: 16, // Back -> INVENTORY_TYPE_BACK (16, NOT 15 -- 15 is "bow")
  15: 21, // Main Hand -> INVENTORY_TYPE_MAIN_HAND (right hand, all weapon types)
  18: 19, // Tabard
};

// Engine meta-path router (live viewer.min.js `ga()`): the texture-
// composited body-armor InventoryTypes fetch `meta/armor/{slot}/{id}.json`;
// EVERY other slot -- all weapons, shields, held, ranged -- fetches
// `meta/item/{id}.json` with no slot in the path at all.
const ARMOR_META_SLOTS: ReadonlySet<number> = new Set([1, 3, 4, 5, 6, 7, 8, 9, 10, 16, 19, 20]);

export function viewerMetaUrl(slot: number, displayId: number): string {
  return ARMOR_META_SLOTS.has(slot)
    ? `${CONTENT_PATH}meta/armor/${slot}/${displayId}.json`
    : `${CONTENT_PATH}meta/item/${displayId}.json`;
}

// Recon §1.1 (`optionsFromModel`): `models: { id: race*2-1+gender, type:
// modelingType.CHARACTER }` -- verbatim formula. `gender` here uses the
// SAME convention as AzerothCore (0 = male / 1 = female): verified
// empirically 2026-07-19 against the wrath meta itself (character/3.json =
// Race 2 Gender 0, character/4.json = Race 2 Gender 1 -- Blizzard's
// standard 0=male pairing). The recon's §1.2 claim of an inverted
// 0=female/1=male viewer convention was WRONG for this tree; the earlier
// acGenderToViewer flip built on it rendered every character as the
// opposite sex and was removed.
export function buildCharacterModelId(race: number, gender: number): number {
  return race * 2 - 1 + gender;
}

const CONTENT_PATH = "http://zam.localhost/modelviewer/wrath/";
// Engine from the LIVE tree, data from the WRATH tree (live smoke
// 2026-07-19): Wowhead migrated model storage from monolithic `mo3/` to
// native `m2/`+`skin/`+`anim/` files in EVERY content tree -- wrath
// included (wrath/m2/<id>.m2 serves 200) -- but the wrath tree's own
// viewer/viewer.min.js is still the legacy engine that requests the
// removed mo3 files (verified byte-identical across fetches). The live
// tree's viewer.min.js is the new-format engine; it is contentPath-driven
// and its only site-global needs (WH.debug, WH.WebP.getImageExtension)
// are already stubbed above, so it renders wrath data natively.
const VIEWER_SCRIPT_URL = "http://zam.localhost/modelviewer/live/viewer/viewer.min.js";

// Recon §4 ("Container sizing"): `aspect` is required (the constructor
// throws "Bad aspect ratio given" if falsy) and combines with the
// container's own CSS width (read via `container.width(...)`) to size the
// internal canvas -- the recon's source never pins a specific numeric
// default, so this is derived from CharacterModel.svelte's own fixed
// container size (300x380).
const VIEWER_ASPECT = 300 / 380;

let viewerScriptsPromise: Promise<void> | null = null;

// Idempotent: the module-level promise is created once and reused for every
// caller/every character switch, so the script tag is only ever injected
// once per app session.
export function loadViewerScripts(): Promise<void> {
  if (!viewerScriptsPromise) {
    viewerScriptsPromise = doLoadViewerScripts();
    // A rejected load must not stay memoized forever -- clear it so the next
    // character switch's call retries from scratch instead of replaying the
    // same stale failure. A successful load stays memoized (no `.then()`
    // clears it), keeping the "inject the script tag once per session"
    // behavior intact.
    viewerScriptsPromise.catch(() => {
      viewerScriptsPromise = null;
    });
  }
  return viewerScriptsPromise;
}

async function doLoadViewerScripts(): Promise<void> {
  // Dynamic import: jQuery 3.7.1 ships plain `main: "dist/jquery.js"` (no
  // `module`/`exports` field), so this resolves to the same UMD CJS build
  // under both Vite/vitest and the app's own bundler. That build's wrapper
  // (confirmed by reading node_modules/jquery/dist/jquery.js) only invokes
  // the real jQuery factory when `global.document` is already present at
  // eval time; otherwise it exports a lazy `function(w) {...}` stand-in that
  // itself throws only once actually *called* without a document. Either
  // way, a static top-level import here would still hand every consumer of
  // this module -- including the pure-helper vitest run, which has no real
  // `window.document` -- the wrong shape (that lazy stand-in instead of a
  // callable jQuery). Deferring the import into this function, only ever
  // invoked from a live webview with a real document, keeps jQuery out of
  // any code path that doesn't actually need one.
  const jq = (await import("jquery")).default;
  window.$ = jq;
  window.jQuery = jq;
  window.WOTLK_TO_RETAIL_DISPLAY_ID_API = undefined;

  // The reference package's setup.js, ported verbatim (minus its
  // CONTENT_PATH/retail-API defaults, which we override above/below):
  // viewer.min.js references window.WH at load/construct time and throws
  // "WH is not defined" without it. WH.WebP.getImageExtension() picks the
  // texture extension; WH.Wow.Item is the client inventory-type enum the
  // engine's slot handling reads. debug is a no-op here (the reference
  // console.logs) -- the engine calls it routinely.
  if (!window.WH) {
    window.WH = {
      debug: () => {},
      defaultAnimation: "Stand",
      WebP: { getImageExtension: () => ".webp" },
      Wow: {
        Item: {
          INVENTORY_TYPE_HEAD: 1,
          INVENTORY_TYPE_NECK: 2,
          INVENTORY_TYPE_SHOULDERS: 3,
          INVENTORY_TYPE_SHIRT: 4,
          INVENTORY_TYPE_CHEST: 5,
          INVENTORY_TYPE_WAIST: 6,
          INVENTORY_TYPE_LEGS: 7,
          INVENTORY_TYPE_FEET: 8,
          INVENTORY_TYPE_WRISTS: 9,
          INVENTORY_TYPE_HANDS: 10,
          INVENTORY_TYPE_FINGER: 11,
          INVENTORY_TYPE_TRINKET: 12,
          INVENTORY_TYPE_ONE_HAND: 13,
          INVENTORY_TYPE_SHIELD: 14,
          INVENTORY_TYPE_RANGED: 15,
          INVENTORY_TYPE_BACK: 16,
          INVENTORY_TYPE_TWO_HAND: 17,
          INVENTORY_TYPE_BAG: 18,
          INVENTORY_TYPE_TABARD: 19,
          INVENTORY_TYPE_ROBE: 20,
          INVENTORY_TYPE_MAIN_HAND: 21,
          INVENTORY_TYPE_OFF_HAND: 22,
          INVENTORY_TYPE_HELD_IN_OFF_HAND: 23,
          INVENTORY_TYPE_PROJECTILE: 24,
          INVENTORY_TYPE_THROWN: 25,
          INVENTORY_TYPE_RANGED_RIGHT: 26,
          INVENTORY_TYPE_QUIVER: 27,
          INVENTORY_TYPE_RELIC: 28,
          INVENTORY_TYPE_PROFESSION_TOOL: 29,
          INVENTORY_TYPE_PROFESSION_ACCESSORY: 30,
        },
      },
    };
  }

  if (window.ZamModelViewer) return;

  await new Promise<void>((resolve, reject) => {
    const script = document.createElement("script");
    script.src = VIEWER_SCRIPT_URL;
    script.onerror = () => reject(new Error("Failed to load ZamModelViewer script"));
    const startedAt = Date.now();
    const TIMEOUT_MS = 15000;
    const POLL_MS = 50;
    const pollForGlobal = () => {
      if (window.ZamModelViewer) {
        resolve();
      } else if (Date.now() - startedAt > TIMEOUT_MS) {
        reject(new Error("Timed out waiting for ZamModelViewer to load"));
      } else {
        setTimeout(pollForGlobal, POLL_MS);
      }
    };
    script.onload = pollForGlobal;
    document.head.appendChild(script);
  });
}

// Recon §1.2 (`characterPart()`, verbatim): maps Wowhead's named
// customization "Options" (from `meta/charactercustomization/{id}.json`) to
// the character-object keys `optionsFromModel`/`getCharacterOptions` build
// `charCustomization` from. The recon's source ternaries 8 of these keys on
// `window.WOTLK_TO_RETAIL_DISPLAY_ID_API`; loadViewerScripts() always sets
// that flag to `undefined` before the viewer script runs, so they're
// transcribed here already resolved to their enabled form.
const CHARACTER_PART: Record<string, string | undefined> = {
  Face: "face",
  "Skin Color": "skin",
  "Hair Style": "hairStyle",
  "Hair Color": "hairColor",
  "Facial Hair": "facialStyle",
  Mustache: "facialStyle",
  Beard: "facialStyle",
  Sideburns: "facialStyle",
  "Face Shape": "facialStyle",
  Eyebrow: "facialStyle",
  "Jaw Features": undefined,
  "Face Features": undefined,
  "Skin Type": undefined,
  Ears: "ears",
  "Fur Color": "furColor",
  Snout: "snout",
  Blindfold: undefined,
  Tattoo: undefined,
  "Eye Color": undefined,
  "Tattoo Color": undefined,
  Armbands: undefined,
  "Jewelry Color": undefined,
  Bracelets: undefined,
  Necklace: undefined,
  Earring: undefined,
  "Primary Color": "primaryColor",
  "Secondary Color Strength": "secondaryColorStrength",
  "Secondary Color": "secondaryColor",
  "Horn Color": "hornColor",
  Horns: "horns",
  "Body Size": "bodySize",
};

// The CLI's paperdoll output only decodes the 5 WotLK playerBytes/playerBytes2
// appearance axes (Task 1 of this plan) -- everything else `CHARACTER_PART`
// knows how to name (ears, fur color, horns, ...) has no source data here and
// is simply never matched below.
type AppearanceField = "skin" | "face" | "hair_style" | "hair_color" | "facial_style";
const DOLL_FIELD_FOR_KEY: Partial<Record<string, AppearanceField>> = {
  face: "face",
  skin: "skin",
  hairStyle: "hair_style",
  hairColor: "hair_color",
  facialStyle: "facial_style",
};

interface CustomizationChoice {
  id: number;
}
interface CustomizationOptionGroup {
  id: number;
  name: string;
  choices?: CustomizationChoice[];
}

// The recon documents `characterPart()` (the named-option -> key mapping)
// verbatim, but the wrapper's own `getCharacterOptions()` body -- which
// actually walks `meta/charactercustomization/{id}.json` and turns a raw
// playerBytes index into an `{optionId, choiceId}` pair -- was never in the
// captured source. This is our own best-effort reconstruction (index into
// each named option's `choices` array using the doll's raw appearance
// value, matching how WoW's own customization tables are ordered) rather
// than a verbatim recon fact; any mismatch with the real JSON shape just
// throws, which the caller treats as "no customization available".
async function buildCharCustomization(
  doll: PaperdollData,
  modelId: number,
): Promise<{ options: { optionId: number; choiceId: number }[] } | undefined> {
  const res = await fetch(`${CONTENT_PATH}meta/charactercustomization/${modelId}.json`);
  if (!res.ok) {
    throw new Error(`charactercustomization/${modelId}.json: HTTP ${res.status}`);
  }
  const groups = (await res.json()) as CustomizationOptionGroup[];
  const options: { optionId: number; choiceId: number }[] = [];
  for (const group of groups) {
    const key = CHARACTER_PART[group.name];
    const dollField = key ? DOLL_FIELD_FOR_KEY[key] : undefined;
    if (!dollField) continue;
    const choice = group.choices?.[doll[dollField]];
    if (!choice) continue;
    options.push({ optionId: group.id, choiceId: choice.id });
  }
  return options.length > 0 ? { options } : undefined;
}

// One pre-flight probe result: `ok` mirrors HTTP 200 vs 404, and when the
// meta parsed, `inventoryType` carries its `Item.InventoryType` (needed to
// disambiguate shields vs held off-hands and bow/thrown/wand ranged items).
// A probe returning null means the probe ITSELF failed (network) -- callers
// keep the item on a best-guess slot so a transient hiccup never strips gear.
export interface MetaProbeResult {
  ok: boolean;
  inventoryType?: number;
}
export type MetaProbe = (url: string) => Promise<MetaProbeResult | null>;

// Pure: the ordered display-id probe candidates for one equipped item --
// the SERVER's displayid first, then wowhead's own display_id (from the
// item-info XML) when it differs. The server value wins whenever its meta
// exists; the wowhead value heals items whose server displayid has no
// Wowhead model data (both Warglaives: server 45479/45481 404 on every
// tree, wowhead 45150/45146 resolve via the proxy's cross-tree fallback).
// Zero/negative/non-finite ids are never probeable and are dropped -- an
// item whose server displayid is 0 but whose wowhead id is known is thereby
// RESCUED rather than skipped.
export function displayIdCandidates(serverDid: number, wowheadDid?: number | null): number[] {
  const out: number[] = [];
  if (Number.isFinite(serverDid) && serverDid > 0) out.push(serverDid);
  if (
    typeof wowheadDid === "number" &&
    Number.isFinite(wowheadDid) &&
    wowheadDid > 0 &&
    wowheadDid !== serverDid
  ) {
    out.push(wowheadDid);
  }
  return out;
}

// Resolve one equipped item across its ordered display-id candidates:
// first id whose meta exists wins; a candidate the CDN definitively lacks
// falls through to the next; all-miss => null (honest skip). A probe
// FAILURE (network) keeps the item on that candidate's best-guess slot
// immediately -- a transient hiccup must never strip gear, and guessing on
// the earlier (server) id beats guessing on a later one.
async function resolveViewerItem(
  acSlot: number,
  ids: number[],
  probe: MetaProbe,
): Promise<[number, number] | null> {
  for (const id of ids) {
    const r = await resolveViewerItemForId(acSlot, id, probe);
    if (r !== null) return r;
  }
  return null;
}

// Resolve ONE (slot, displayId) pair to its final [InventoryType, displayId]
// pair, or null when the CDN confirms the meta doesn't exist anywhere the
// engine would look (custom/GM displayids, and items absent from every
// content tree even after the proxy's cross-tree fallback).
//
// This MUST run before construction: the live engine swallows per-item meta
// 404s (`.catch(() => { t.H = true })` -- the item is just silently
// invisible), so construction NEVER rejects over a missing item and no
// after-the-fact retry can detect or fix a wrong slot.
async function resolveViewerItemForId(
  acSlot: number,
  id: number,
  probe: MetaProbe,
): Promise<[number, number] | null> {
  // Chest (AC 4): plain chests live at meta/armor/5/, robes ONLY at
  // meta/armor/20/ (e.g. Gamemaster's Robe 22033). The engine maps a passed
  // slot-20 item onto the chest display slot itself (Er[20] = 5), so the
  // robe geosets composite correctly.
  if (acSlot === 4) {
    const chest = await probe(viewerMetaUrl(5, id));
    if (chest === null || chest.ok) return [5, id];
    const robe = await probe(viewerMetaUrl(20, id));
    if (robe === null || robe.ok) return [20, id];
    return null;
  }
  // Off hand (AC 16): weapons and held frills render at slot 22 (left
  // palm); shields must pass 14 so the engine uses the shield mount bone
  // instead. Both route to meta/item/{id}.json -- the meta's own
  // Item.InventoryType tells them apart.
  if (acSlot === 16) {
    const meta = await probe(viewerMetaUrl(22, id));
    if (meta === null) return [22, id];
    if (!meta.ok) return null;
    return [meta.inventoryType === 14 ? 14 : 22, id];
  }
  // Ranged (AC 17): pass the meta's own InventoryType (15 bow / 25 thrown /
  // 26 wand-gun) -- NEVER 18: the engine's display-slot table has no
  // attachment for 18 (Er[18] = 0), so an 18 loads and then never shows.
  if (acSlot === 17) {
    const meta = await probe(viewerMetaUrl(26, id));
    if (meta === null) return [26, id];
    if (!meta.ok) return null;
    const it = meta.inventoryType;
    return [it === 15 || it === 25 || it === 26 ? it : 26, id];
  }
  const invType = AC_TO_INVENTORY_TYPE[acSlot];
  if (invType === undefined) return null; // never-displayed slot
  const meta = await probe(viewerMetaUrl(invType, id));
  if (meta === null || meta.ok) return [invType, id];
  return null;
}

// Everything createCharacterViewer needs to construct honestly: the final
// items array (engine InventoryType vocabulary) plus the count of
// viewer-eligible equipped items -- `total - items.length` is exactly the
// skipped-item count the "K of N can't be shown" note reports.
export interface ResolvedViewerItems {
  items: [number, number][];
  total: number;
}

export async function resolveViewerItems(
  equipped: { slot: number; displayid: number; entry?: number }[],
  probe: MetaProbe,
  overrides?: Map<number, number>,
): Promise<ResolvedViewerItems> {
  // Eligible = a slot the engine displays at all, with at least one
  // probeable display-id candidate (server displayid, or a wowhead
  // display_id override keyed by item entry). Neck/rings/trinkets and
  // empty slots are not candidates and never count against the
  // skipped-items note.
  const withIds = equipped.map(
    (it) =>
      [
        it,
        displayIdCandidates(
          it.displayid,
          it.entry !== undefined ? overrides?.get(it.entry) : undefined,
        ),
      ] as const,
  );
  const candidates = withIds.filter(
    ([it, ids]) =>
      ids.length > 0 &&
      (AC_TO_INVENTORY_TYPE[it.slot] !== undefined ||
        it.slot === 4 ||
        it.slot === 16 ||
        it.slot === 17),
  );
  // Per-item probes run concurrently (each item needs at most a handful of
  // sequential fetches); results keep the equipped order.
  const resolved = await Promise.all(
    candidates.map(([it, ids]) => resolveViewerItem(it.slot, ids, probe)),
  );
  return {
    items: resolved.filter((r): r is [number, number] => r !== null),
    total: candidates.length,
  };
}

// NATIVE probe (2026-07-22): the WebView's fetch() proved unusable for this
// -- WebView2 surfaces the zam scheme's clean 404s as network ERRORS, so
// every "meta missing" read as "network hiccup", items stayed on best-guess
// slots, and the engine dropped them invisibly (the robe-never-renders /
// no-skip-note-ever bug, proven via cache forensics: the second chest probe
// never fired). The Rust side answers three-valued (hit/miss/err) via
// reqwest against the real upstream and warms the shared cache on hit.
async function fetchMetaProbe(url: string): Promise<MetaProbeResult | null> {
  try {
    const rel = url.startsWith(CONTENT_PATH)
      ? `modelviewer/wrath/${url.slice(CONTENT_PATH.length)}`
      : url.replace(/^https?:\/\/[^/]+\//, "");
    const r = (await invoke("zam_probe", { path: rel })) as {
      status: "hit" | "miss" | "err";
      inventoryType?: number | null;
    };
    if (r.status === "hit") {
      return { ok: true, inventoryType: typeof r.inventoryType === "number" ? r.inventoryType : undefined };
    }
    if (r.status === "miss") return { ok: false };
    return null;
  } catch {
    return null;
  }
}

// Pre-flight: is the character's GEOMETRY actually downloadable? Wowhead's
// format migration (see VIEWER_SCRIPT_URL note) means geometry now lives at
// `m2/{Model}.m2`; a mid-migration or future upstream change would leave
// the engine constructing fine and then rendering an empty grey canvas, so
// probe the real file first and fail with an explanation instead. The
// probe's GET also pre-warms the proxy cache with the geometry itself.
export async function geometryAvailable(modelId: number): Promise<boolean> {
  try {
    const meta = await fetch(`${CONTENT_PATH}meta/character/${modelId}.json`);
    if (!meta.ok) return false;
    const j = (await meta.json()) as { Model?: number };
    if (!j.Model) return false;
    const geo = await fetch(`${CONTENT_PATH}m2/${j.Model}.m2`);
    return geo.ok;
  } catch {
    return false;
  }
}

// Pure: the "K of N equipped items can't be shown" caption for the model
// card (smoke item 6: silently dropped GM/custom items read as a bug).
// Null when nothing was dropped -- the card then shows no note at all.
// Wording covers both custom/GM displayids AND legit items Wowhead's data
// simply lacks (the Warglaives case) -- "no Wowhead model data" is the one
// honest common cause.
export function skippedItemsNote(total: number, shown: number): string | null {
  const skipped = total - shown;
  if (total <= 0 || skipped <= 0) return null;
  return `${skipped} of ${total} equipped item${total === 1 ? "" : "s"} can't be shown in 3D (no Wowhead model data).`;
}

// ---------------------------------------------------------------------------
// Weapon sheathing (2026-07-22, decompiled from the same live-tree
// viewer.min.js -- full trace in .superpowers/sdd/sheathe-report.md):
//
// * The outer viewer instance (`Si` class) exposes `method(name, args)`,
//   which forwards to the renderer's dispatcher: applied immediately when
//   the character actor is loaded, else QUEUED on the actor's load promise
//   (`actorPromises[0].then(...)`) -- so calling it right after
//   construction is safe, never dropped.
// * The character actor implements `setSheath(main, off)`: it stores the
//   two values and the per-frame item update re-derives every attachment
//   from them (`I(t,e)`), so the change repositions weapons LIVE -- no
//   rebuild needed.
// * The two values speak the client's SheatheType vocabulary (item.dbc
//   SheatheType / AC item_template.sheath). -1 = in hands (default). The
//   engine's own tables (`Lr` fallback + `Ir[sheathType][slot]`):
//     1 -> back, two-hander diagonal   (attachments 26/27)
//     2 -> back, staff angle           (attachments 30/31)
//     3 -> hips, one-handers           (attachments 32/33)
//     4 -> shield                      (shield always lands on attachment
//                                       28 via Lr -- Ir never refines
//                                       slot 14)
//     7 -> BOTH weapons crossed on the back (26/27) -- the Warglaives look
//   Passing any value >= 0 for either hand engages the Lr fallback for
//   every weapon-ish slot (ranged included: bow -> back). Fist weapons
//   (class 2 subclass 13) are HIDDEN by the engine while sheathed, exactly
//   like in-game. Values outside -1..9 would make the engine index
//   `Ir[value]` unguarded and throw -- sheathTypeForItem only ever
//   produces 0..7.
export interface SheathValues {
  main: number;
  off: number;
}

// Pure: the SheatheType for one resolved weapon-slot item. `finalSlot` is
// the item's resolved engine InventoryType (21 main hand, 22 off-hand,
// 14 shield); `meta` carries the item meta JSON's Item.ItemClass/
// ItemSubClass/InventoryType (null when the meta couldn't be fetched).
// NB: resolveViewerItemForId lands every non-shield off-hand at slot 22 --
// held frills included -- so a frill is recognized by its meta
// InventoryType (23), never by finalSlot.
export function sheathTypeForItem(
  finalSlot: number,
  meta: { itemClass?: number; itemSubClass?: number; inventoryType?: number } | null,
  entry?: number,
): number {
  // The Warglaives of Azzinoth (32837 MH / 32838 OH) are one-hand swords
  // (subclass 7 -> hip) by the generic rule, but the client sheathes them
  // CROSSED ON THE BACK -- the iconic look this feature exists for.
  if (entry === 32837 || entry === 32838) return 7;
  if (finalSlot === 14) return 4; // shield -> shield-back mount
  if (meta?.inventoryType === 23) return 0; // held frill: no real sheathed pose
  if (meta?.itemClass === 2) {
    const sub = meta.itemSubClass;
    if (sub === 10) return 2; // staff
    if (sub === 1 || sub === 5 || sub === 6 || sub === 8 || sub === 20) return 1; // 2H axe/mace/polearm/sword/fishing pole
    return 3; // 1H axe/mace/sword/fist/dagger -> hips
  }
  // Meta unavailable (offline/uncached) or non-weapon shape: the generic
  // back position -- correct for two-handers, acceptable for everything.
  return 1;
}

export type ItemMetaFetch = (
  displayId: number,
) => Promise<{ itemClass?: number; itemSubClass?: number; inventoryType?: number } | null>;

// Derive the doll's SheathValues pair from the already-resolved items
// array. Only the main-hand (21) and off-hand (22/14) rows matter -- the
// engine's Ir refinement only ever keys slots 21/22, and shields need no
// meta fetch at all. Slot-22 rows DO fetch: the meta's ItemClass/
// InventoryType is what tells an off-hand weapon (hips/back) from a held
// frill (InventoryType 23 -> type 0). The metas fetched here are the
// exact URLs the pre-flight probes warmed, so this is a local-cache read.
export async function deriveSheathValues(
  items: [number, number][],
  equipped: { slot: number; entry?: number }[],
  fetchMeta: ItemMetaFetch,
): Promise<SheathValues> {
  const entryAt = (acSlot: number) => equipped.find((e) => e.slot === acSlot)?.entry;
  const typeFor = async (
    found: [number, number] | undefined,
    acSlot: number,
  ): Promise<number> => {
    if (!found) return -1;
    const [slot, displayId] = found;
    const entry = entryAt(acSlot);
    const needsMeta = (slot === 21 || slot === 22) && entry !== 32837 && entry !== 32838;
    return sheathTypeForItem(slot, needsMeta ? await fetchMeta(displayId) : null, entry);
  };
  const [main, off] = await Promise.all([
    typeFor(
      items.find(([s]) => s === 21),
      15,
    ),
    typeFor(
      items.find(([s]) => s === 22 || s === 14),
      16,
    ),
  ]);
  // A lone ranged weapon (bow 15 / thrown 25 / wand-gun 26) sheathes via
  // the engine's Lr fallback, which only engages while ANY sheath value is
  // >= 0 -- give it one so the toggle still works for e.g. a hunter.
  if (main < 0 && off < 0 && items.some(([s]) => s === 15 || s === 25 || s === 26)) {
    return { main: 1, off: -1 };
  }
  return { main, off };
}

// Real ItemMetaFetch: meta/item/{id}.json through the zam proxy (the
// pre-flight probe already warmed the shared cache for every id this is
// asked about). Any failure degrades to null -> generic back position.
async function fetchItemSheathMeta(
  displayId: number,
): Promise<{ itemClass?: number; itemSubClass?: number; inventoryType?: number } | null> {
  try {
    const res = await fetch(`${CONTENT_PATH}meta/item/${displayId}.json`);
    if (!res.ok) return null;
    const j = (await res.json()) as {
      Item?: { ItemClass?: number; ItemSubClass?: number; InventoryType?: number };
    };
    return j.Item
      ? {
          itemClass: j.Item.ItemClass,
          itemSubClass: j.Item.ItemSubClass,
          inventoryType: j.Item.InventoryType,
        }
      : null;
  } catch {
    return null;
  }
}

// Runtime toggle on a live viewer instance: `on` moves weapons to their
// sheathed positions, `off` returns them to the hands (-1/-1, the engine
// default). Best-effort guarded like destroyViewer -- an engine build
// without `method`/`setSheath` just keeps weapons in hands.
export function applyViewerSheath(
  viewer: unknown,
  values: SheathValues | null,
  on: boolean,
): void {
  if (!viewer || typeof viewer !== "object") return;
  const method = (viewer as { method?: unknown }).method;
  if (typeof method !== "function") return;
  const v = on && values ? values : { main: -1, off: -1 };
  try {
    (method as (name: string, args: unknown[]) => unknown).call(viewer, "setSheath", [
      v.main,
      v.off,
    ]);
  } catch {
    // best-effort only
  }
}

// Sheathe-toggle preference: one global pref (not per character), default
// = weapons in hands. Guarded storage access, same idiom as
// features.svelte.ts readStored (vitest's node env has no localStorage).
const SHEATHED_PREF_KEY = "dml.modelSheathed";

export function readSheathedPref(): boolean {
  return typeof localStorage !== "undefined" && localStorage.getItem(SHEATHED_PREF_KEY) === "1";
}

export function writeSheathedPref(on: boolean): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(SHEATHED_PREF_KEY, on ? "1" : "0");
  }
}

// What createCharacterViewer resolves with: the (untyped) viewer instance
// plus how many of the doll's viewer-renderable items actually made it into
// the construction -- the caller derives the skipped-items note from these.
// `sheath` is the doll's derived per-hand SheatheType pair for
// applyViewerSheath (-1/-1 when there's nothing to sheathe).
export interface CharacterViewerResult {
  viewer: unknown;
  totalItems: number;
  shownItems: number;
  sheath: SheathValues;
}

// The item-info batch (CharacterSheet's fire-and-forget fetch) lands
// asynchronously after the paperdoll -- the viewer waits for it briefly
// because the wowhead display_id overrides it carries are what let
// wrong-server-displayid items (the Warglaives) render at all. The bound
// exists so a wowhead outage can never hang the model: past it the viewer
// constructs with server ids only, exactly the pre-override behavior.
const OVERRIDES_TIMEOUT_MS = 3000;

async function boundedOverrides(
  p: Promise<Map<number, number>>,
  ms: number,
): Promise<Map<number, number> | undefined> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    const winner = await Promise.race([
      p,
      new Promise<undefined>((resolve) => {
        timer = setTimeout(() => resolve(undefined), ms);
      }),
    ]);
    return winner ?? undefined;
  } catch {
    // The batch promise itself never intentionally rejects (CharacterSheet
    // resolves it with whatever the cache holds even on fetch failure) --
    // any rejection just degrades to server ids.
    return undefined;
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

// Recon §1.1: the final options object ZamModelViewer receives for a
// playable character (env='live'-shaped, which is what the wrath tree
// wants too) -- `type: 2, contentPath, container: jQuery(selector), aspect,
// hd: true, models: {id, type: 16}, items, charCustomization` (the last
// omitted entirely when unavailable, matching `model.noCharCustomization`).
// The viewer instance stays `unknown` (recon has no documented
// destroy/teardown API -- see CharacterModel.svelte's guarded call site).
//
// Items are fully resolved BEFORE the single construction: the live engine
// swallows per-item meta 404s (an unrenderable item is silently invisible,
// construction never rejects over it), so a catch-driven probe/retry ladder
// can neither fix wrong slots nor count skips -- the old one was dead code.
// The pre-flight probes hit the same URLs the engine re-requests, so they
// also pre-warm the zam proxy cache. One naked retry remains for genuine
// engine-level failures unrelated to items -- a base model beats no model.
export async function createCharacterViewer(
  containerId: string,
  doll: PaperdollData,
  displayIds?: Promise<Map<number, number>>,
): Promise<CharacterViewerResult> {
  const modelId = buildCharacterModelId(doll.race, doll.gender);
  if (!(await geometryAvailable(modelId))) {
    throw new Error(
      "Character model files aren't downloadable from Wowhead right now — showing gear without a model.",
    );
  }
  let charCustomization: { options: { optionId: number; choiceId: number }[] } | undefined;
  try {
    charCustomization = await buildCharCustomization(doll, modelId);
  } catch {
    // Best-effort only -- recon §1.1 shows the constructor happily accepts
    // a character with `charCustomization` simply omitted, rendering the
    // base model + equipped items without a customized look.
  }

  const Viewer = window.ZamModelViewer;
  if (!Viewer) throw new Error("ZamModelViewer script not loaded");

  const construct = async (items: [number, number][]) => {
    const options: Record<string, unknown> = {
      type: 2,
      contentPath: CONTENT_PATH,
      container: window.$(`#${containerId}`),
      aspect: VIEWER_ASPECT,
      hd: true,
      models: { type: 16, id: modelId },
      items,
    };
    if (charCustomization) options.charCustomization = charCustomization;
    return await new Viewer(options);
  };

  const overrides = displayIds
    ? await boundedOverrides(displayIds, OVERRIDES_TIMEOUT_MS)
    : undefined;
  const resolved = await resolveViewerItems(doll.equipped, fetchMetaProbe, overrides);
  try {
    const viewer = await construct(resolved.items);
    const sheath = await deriveSheathValues(resolved.items, doll.equipped, fetchItemSheathMeta);
    return { viewer, totalItems: resolved.total, shownItems: resolved.items.length, sheath };
  } catch (e) {
    document.getElementById(containerId)?.replaceChildren();
    if (resolved.items.length === 0) throw e;
    const viewer = await construct([]);
    // Naked fallback construction shows no items -- nothing to sheathe.
    return { viewer, totalItems: resolved.total, shownItems: 0, sheath: { main: -1, off: -1 } };
  }
}
