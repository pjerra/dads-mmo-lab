// Adapter for embedding Wowhead's ZamModelViewer (wrath content tree) in a
// Tauri webview, driven by native WotLK display ids from the CLI's paperdoll
// output. The invocation shape (options object), the equipment-slot table,
// the character model-id formula, and the character-customization mechanism
// below are ported from `wow-model-viewer` (github.com/Miorey/wow-model-viewer,
// npm `wow-model-viewer`, version 1.5.3, ISC license, Copyright (c) Miorey) --
// specifically `index.js`/`character_modeling.js`/`wow_model_viewer.js` from
// that package, as captured verbatim in `.superpowers/sdd/recon-modelviewer.md`
// (THE authority for every fact below -- see that file for full source
// extracts, CORS probe results, and the porting verdict this adapter follows).
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

// Recon §1.2, README equipment-slot table: the viewer's own 1-22 slot
// numbering is the standard WoW client inventory-slot enum (1-19) plus 3
// viewer-internal extras (20/21/22, the "new" chest-robe/mainhand/offhand
// slots used only as an internal 404-fallback -- see recon §1.2/1.3,
// `getDisplaySlot()`). AC's EquipmentSlots enum is that same 1-19 numbering,
// 0-indexed -- so every AC slot the viewer marks "Is displayed: Yes" maps to
// `acSlot + 1`. Only those rendered slots are listed: neck (AC 1 -> viewer
// 2), both rings (AC 10/11 -> viewer 11/12) and both trinkets (AC 12/13 ->
// viewer 13/14) are the viewer's own `NOT_DISPLAYED_SLOTS = [2,11,12,13,14]`
// and are deliberately absent here.
export const AC_TO_VIEWER_SLOT: Record<number, number> = {
  0: 1, // Head
  2: 3, // Shoulders
  3: 4, // Body (shirt)
  4: 5, // Chest
  5: 6, // Waist
  6: 7, // Legs
  7: 8, // Feet
  8: 9, // Wrists
  9: 10, // Hands
  14: 15, // Back
  15: 16, // Main Hand
  16: 17, // Off Hand
  17: 18, // Ranged
  18: 19, // Tabard
};

// Recon §1.1 (`optionsFromModel`): `characterItems.filter(e =>
// !NOT_DISPLAYED_SLOTS.includes(e[0]))` -- we filter by only ever emitting
// mapped (i.e. displayed) slots in the first place, plus drop displayid 0
// (an empty slot; the viewer has nothing to render there).
export function buildViewerItems(
  equipped: { slot: number; displayid: number }[],
): [number, number][] {
  const items: [number, number][] = [];
  for (const it of equipped) {
    if (it.displayid === 0) continue;
    const viewerSlot = AC_TO_VIEWER_SLOT[it.slot];
    if (viewerSlot === undefined) continue;
    items.push([viewerSlot, it.displayid]);
  }
  return items;
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

// Recon §1.2 (`getDisplaySlot`, captured source verbatim): the viewer's own
// internal 404-fallback remaps three slots to "new"-style meta locations --
// `{ 5: 20, 16: 21, 18: 22 }` (chest -> robe-chest, mainhand -> mainhand(new),
// offhand -> offhand(new)). Mirrored exactly so the pre-flight probe below
// checks the same alternate location the viewer itself would try.
export function viewerFallbackSlot(slot: number): number | null {
  if (slot === 5) return 20;
  if (slot === 16) return 21;
  if (slot === 18) return 22;
  return null;
}

// Drop items whose display meta doesn't exist on the CDN: custom/GM
// displayids (server-side items wowhead never had) 404, and ONE missing
// meta rejects the viewer's entire construction -- the F1 failure mode.
// probe returns true (meta exists), false (confirmed missing), or null
// (probe itself failed -- network); unknowns are KEPT so a transient
// hiccup can't silently strip gear.
export async function probeRenderableItems(
  items: [number, number][],
  probe: (url: string) => Promise<boolean | null>,
): Promise<[number, number][]> {
  const kept: [number, number][] = [];
  for (const [slot, id] of items) {
    const primary = await probe(`${CONTENT_PATH}meta/armor/${slot}/${id}.json`);
    if (primary !== false) {
      kept.push([slot, id]);
      continue;
    }
    const fb = viewerFallbackSlot(slot);
    if (fb !== null && (await probe(`${CONTENT_PATH}meta/armor/${fb}/${id}.json`)) !== false) {
      // Keep the item under the FALLBACK slot -- the renderer refetches the
      // meta itself, so handing it the original slot replays the 404 the
      // probe just saw and the item is silently dropped. Live case: robes
      // (chest 5 -> robe 20, e.g. Gamemaster's Robe 22033) and weapons
      // (16 -> 21 / 18 -> 22, e.g. Warglaive off-hands) only exist on the
      // CDN under their fallback slot.
      kept.push([fb, id]);
    }
  }
  return kept;
}

async function fetchProbe(url: string): Promise<boolean | null> {
  try {
    const res = await fetch(url);
    return res.ok;
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
export function skippedItemsNote(total: number, shown: number): string | null {
  const skipped = total - shown;
  if (total <= 0 || skipped <= 0) return null;
  return `${skipped} of ${total} equipped item${total === 1 ? "" : "s"} can't be shown in 3D (custom/GM items).`;
}

// What createCharacterViewer resolves with: the (untyped) viewer instance
// plus how many of the doll's viewer-renderable items actually made it into
// the construction -- the caller derives the skipped-items note from these.
export interface CharacterViewerResult {
  viewer: unknown;
  totalItems: number;
  shownItems: number;
}

// Recon §1.1: the final options object ZamModelViewer receives for a
// playable character (env='live'-shaped, which is what the wrath tree
// wants too) -- `type: 2, contentPath, container: jQuery(selector), aspect,
// hd: true, models: {id, type: 16}, items, charCustomization` (the last
// omitted entirely when unavailable, matching `model.noCharCustomization`).
// The viewer instance stays `unknown` (recon has no documented
// destroy/teardown API -- see CharacterModel.svelte's guarded call site).
//
// Construction runs up to three attempts: full gear first; if that rejects,
// probe out CDN-missing items (custom/GM gear) and retry; finally retry
// naked -- a base model beats no model. The container is cleared between
// attempts so a half-constructed canvas can't stack under the retry.
export async function createCharacterViewer(
  containerId: string,
  doll: PaperdollData,
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

  const allItems = buildViewerItems(doll.equipped);
  try {
    const viewer = await construct(allItems);
    return { viewer, totalItems: allItems.length, shownItems: allItems.length };
  } catch (e) {
    document.getElementById(containerId)?.replaceChildren();
    const kept = await probeRenderableItems(allItems, fetchProbe);
    if (kept.length !== allItems.length) {
      try {
        const viewer = await construct(kept);
        return { viewer, totalItems: allItems.length, shownItems: kept.length };
      } catch {
        document.getElementById(containerId)?.replaceChildren();
      }
    }
    if (allItems.length === 0) throw e;
    const viewer = await construct([]);
    return { viewer, totalItems: allItems.length, shownItems: 0 };
  }
}
