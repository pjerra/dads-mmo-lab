import { describe, expect, it, vi } from "vitest";
import {
  AC_TO_INVENTORY_TYPE,
  applyViewerSheath,
  buildCharacterModelId,
  deriveSheathValues,
  displayIdCandidates,
  readSheathedPref,
  releaseGlContexts,
  resolveViewerItems,
  sheathTypeForItem,
  skippedItemsNote,
  viewerMetaUrl,
  writeSheathedPref,
  type MetaProbeResult,
} from "./model-viewer";

const BASE = "http://zam.localhost/modelviewer/wrath/";

// Ground truth for every assertion below: the decompiled live-tree
// viewer.min.js (engine path router `ga()`, display-slot table `Er`,
// per-item 404 swallowing) + curl-verified CDN responses, documented in
// .superpowers/sdd/model-browser-report.md. The items array speaks the WoW
// client InventoryType enum -- NOT equip-slot+1 as the reference package's
// README claims (its own demo cloak `[15, 17238]` silently 404s).

describe("viewerMetaUrl", () => {
  it("routes the texture-composited armor InventoryTypes to meta/armor/{slot}/", () => {
    for (const slot of [1, 3, 4, 5, 6, 7, 8, 9, 10, 16, 19, 20]) {
      expect(viewerMetaUrl(slot, 123)).toBe(`${BASE}meta/armor/${slot}/123.json`);
    }
  });

  it("routes every other slot (weapons/shields/held/ranged) to meta/item/{id} -- no slot in the path", () => {
    for (const slot of [13, 14, 15, 17, 21, 22, 23, 25, 26]) {
      expect(viewerMetaUrl(slot, 456)).toBe(`${BASE}meta/item/456.json`);
    }
  });
});

describe("AC_TO_INVENTORY_TYPE", () => {
  it("maps fixed AC equip slots to engine InventoryType", () => {
    expect(AC_TO_INVENTORY_TYPE[0]).toBe(1); // Head
    expect(AC_TO_INVENTORY_TYPE[2]).toBe(3); // Shoulders
    expect(AC_TO_INVENTORY_TYPE[3]).toBe(4); // Body (shirt)
    expect(AC_TO_INVENTORY_TYPE[5]).toBe(6); // Waist
    expect(AC_TO_INVENTORY_TYPE[6]).toBe(7); // Legs
    expect(AC_TO_INVENTORY_TYPE[7]).toBe(8); // Feet
    expect(AC_TO_INVENTORY_TYPE[8]).toBe(9); // Wrists
    expect(AC_TO_INVENTORY_TYPE[9]).toBe(10); // Hands
    expect(AC_TO_INVENTORY_TYPE[18]).toBe(19); // Tabard
  });

  it("maps Back to INVENTORY_TYPE_BACK (16) -- 15 would route cloaks to meta/item/ and 404", () => {
    expect(AC_TO_INVENTORY_TYPE[14]).toBe(16);
  });

  it("maps Main Hand to INVENTORY_TYPE_MAIN_HAND (21) -- 16 is the BACK armor directory", () => {
    expect(AC_TO_INVENTORY_TYPE[15]).toBe(21);
  });

  it("has no fixed row for the per-item-resolved slots (chest/off hand/ranged) or never-displayed ones", () => {
    for (const slot of [1, 4, 10, 11, 12, 13, 16, 17, 19]) {
      expect(AC_TO_INVENTORY_TYPE[slot]).toBeUndefined();
    }
  });
});

describe("resolveViewerItems", () => {
  const probeFrom = (answers: Record<string, MetaProbeResult | null>) => {
    const asked: string[] = [];
    const probe = async (url: string) => {
      asked.push(url);
      // `in`-check, not `??`: a stored null (probe-failed) must survive.
      return url in answers ? answers[url] : { ok: false };
    };
    return { probe, asked };
  };

  it("renders a plain chest at armor slot 5 without touching the robe location", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/armor/5/30763.json`]: { ok: true },
    });
    const r = await resolveViewerItems([{ slot: 4, displayid: 30763 }], probe);
    expect(r).toEqual({ items: [[5, 30763]], total: 1 });
    expect(asked).toEqual([`${BASE}meta/armor/5/30763.json`]);
  });

  it("resolves a robe to slot 20 when the chest meta 404s (Gamemaster's Robe 22033)", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/armor/5/22033.json`]: { ok: false },
      [`${BASE}meta/armor/20/22033.json`]: { ok: true },
    });
    const r = await resolveViewerItems([{ slot: 4, displayid: 22033 }], probe);
    expect(r).toEqual({ items: [[20, 22033]], total: 1 });
    expect(asked).toEqual([`${BASE}meta/armor/5/22033.json`, `${BASE}meta/armor/20/22033.json`]);
  });

  it("resolves a main-hand weapon to slot 21 via meta/item/ (Trashbringer 23875)", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/item/23875.json`]: { ok: true, inventoryType: 17 },
    });
    const r = await resolveViewerItems([{ slot: 15, displayid: 23875 }], probe);
    expect(r).toEqual({ items: [[21, 23875]], total: 1 });
    expect(asked).toEqual([`${BASE}meta/item/23875.json`]);
  });

  it("skips both Warglaives (45479/45481: meta/item/ 404s) but still counts them", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/item/45479.json`]: { ok: false },
      [`${BASE}meta/item/45481.json`]: { ok: false },
    });
    const r = await resolveViewerItems(
      [
        { slot: 15, displayid: 45479 },
        { slot: 16, displayid: 45481 },
      ],
      probe,
    );
    expect(r).toEqual({ items: [], total: 2 });
    expect(skippedItemsNote(r.total, r.items.length)).toBe(
      "2 of 2 equipped items can't be shown in 3D (no Wowhead model data).",
    );
  });

  it("resolves an off-hand weapon/held to slot 22", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/item/700.json`]: { ok: true, inventoryType: 22 },
    });
    const r = await resolveViewerItems([{ slot: 16, displayid: 700 }], probe);
    expect(r.items).toEqual([[22, 700]]);
  });

  it("resolves a shield to slot 14 so the engine uses the shield mount, not the palm", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/item/1680.json`]: { ok: true, inventoryType: 14 },
    });
    const r = await resolveViewerItems([{ slot: 16, displayid: 1680 }], probe);
    expect(r.items).toEqual([[14, 1680]]);
  });

  it("resolves ranged items to the meta's own InventoryType (bow 15 / thrown 25 / wand-gun 26), never 18", async () => {
    for (const [invType, expected] of [
      [15, 15],
      [25, 25],
      [26, 26],
      [undefined, 26], // unknown -> right-hand default; 18 has no attachment bone
    ] as const) {
      const { probe } = probeFrom({
        [`${BASE}meta/item/20723.json`]: { ok: true, inventoryType: invType },
      });
      const r = await resolveViewerItems([{ slot: 17, displayid: 20723 }], probe);
      expect(r.items).toEqual([[expected, 20723]]);
    }
  });

  it("resolves a cloak to armor slot 16 (back cloaks live ONLY at meta/armor/16/)", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/armor/16/15120.json`]: { ok: true },
    });
    const r = await resolveViewerItems([{ slot: 14, displayid: 15120 }], probe);
    expect(r.items).toEqual([[16, 15120]]);
    expect(asked).toEqual([`${BASE}meta/armor/16/15120.json`]);
  });

  it("drops fixed-slot items whose armor meta 404s (custom/GM displayids) and counts them", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/armor/1/1170.json`]: { ok: true },
      [`${BASE}meta/armor/7/9999.json`]: { ok: false },
    });
    const r = await resolveViewerItems(
      [
        { slot: 0, displayid: 1170 },
        { slot: 6, displayid: 9999 },
      ],
      probe,
    );
    expect(r).toEqual({ items: [[1, 1170]], total: 2 });
  });

  it("keeps items on the best-guess slot when the probe itself fails (network hiccup)", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/armor/5/300.json`]: null,
      [`${BASE}meta/item/400.json`]: null,
    });
    const r = await resolveViewerItems(
      [
        { slot: 4, displayid: 300 },
        { slot: 15, displayid: 400 },
      ],
      probe,
    );
    expect(r).toEqual({
      items: [
        [5, 300],
        [21, 400],
      ],
      total: 2,
    });
  });

  it("excludes never-displayed slots and empty displayids from BOTH items and the total", async () => {
    const { probe, asked } = probeFrom({});
    const r = await resolveViewerItems(
      [
        { slot: 1, displayid: 5555 }, // Neck: engine never displays
        { slot: 10, displayid: 6666 }, // Ring
        { slot: 12, displayid: 7777 }, // Trinket
        { slot: 4, displayid: 0 }, // Chest, empty
      ],
      probe,
    );
    expect(r).toEqual({ items: [], total: 0 });
    expect(asked).toEqual([]);
  });

  it("preserves equipped order in the resolved items array", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/armor/1/11.json`]: { ok: true },
      [`${BASE}meta/armor/5/22.json`]: { ok: true },
      [`${BASE}meta/item/33.json`]: { ok: true, inventoryType: 17 },
    });
    const r = await resolveViewerItems(
      [
        { slot: 0, displayid: 11 },
        { slot: 4, displayid: 22 },
        { slot: 15, displayid: 33 },
      ],
      probe,
    );
    expect(r.items).toEqual([
      [1, 11],
      [5, 22],
      [21, 33],
    ]);
  });
});

describe("displayIdCandidates", () => {
  it("is server-id only when wowhead knows nothing extra", () => {
    expect(displayIdCandidates(30606)).toEqual([30606]);
    expect(displayIdCandidates(30606, undefined)).toEqual([30606]);
    expect(displayIdCandidates(30606, null)).toEqual([30606]);
    expect(displayIdCandidates(30606, 0)).toEqual([30606]);
  });

  it("appends a differing wowhead id AFTER the server id (server meta wins when it exists)", () => {
    expect(displayIdCandidates(45479, 45150)).toEqual([45479, 45150]);
  });

  it("dedupes an agreeing wowhead id", () => {
    expect(displayIdCandidates(30606, 30606)).toEqual([30606]);
  });

  it("rescues a zero server displayid when wowhead knows the item", () => {
    expect(displayIdCandidates(0, 45150)).toEqual([45150]);
  });

  it("drops unprobeable ids entirely", () => {
    expect(displayIdCandidates(0)).toEqual([]);
    expect(displayIdCandidates(0, 0)).toEqual([]);
    expect(displayIdCandidates(-5, Number.NaN)).toEqual([]);
  });
});

describe("resolveViewerItems with wowhead display-id overrides", () => {
  const probeFrom = (answers: Record<string, MetaProbeResult | null>) => {
    const asked: string[] = [];
    const probe = async (url: string) => {
      asked.push(url);
      return url in answers ? answers[url] : { ok: false };
    };
    return { probe, asked };
  };

  it("renders both Warglaives via wowhead ids when the server displayids miss everywhere", async () => {
    // Real ids: server item_template 45479 (MH) / 45481 (OH) 404 on every
    // tree; wowhead's own displayIds 45150/45146 exist (tbc/cata -- served
    // through the proxy's cross-tree fallback). OH meta InventoryType 22.
    const { probe, asked } = probeFrom({
      [`${BASE}meta/item/45479.json`]: { ok: false },
      [`${BASE}meta/item/45150.json`]: { ok: true, inventoryType: 21 },
      [`${BASE}meta/item/45481.json`]: { ok: false },
      [`${BASE}meta/item/45146.json`]: { ok: true, inventoryType: 22 },
    });
    const r = await resolveViewerItems(
      [
        { slot: 15, entry: 32837, displayid: 45479 },
        { slot: 16, entry: 32838, displayid: 45481 },
      ],
      probe,
      new Map([
        [32837, 45150],
        [32838, 45146],
      ]),
    );
    expect(r).toEqual({
      items: [
        [21, 45150],
        [22, 45146],
      ],
      total: 2,
    });
    // Server id tried FIRST for each item, wowhead id only after its miss.
    // (Items probe concurrently, so only the PER-ITEM relative order is
    // guaranteed -- not the global interleaving.)
    expect(asked.indexOf(`${BASE}meta/item/45479.json`)).toBeLessThan(
      asked.indexOf(`${BASE}meta/item/45150.json`),
    );
    expect(asked.indexOf(`${BASE}meta/item/45481.json`)).toBeLessThan(
      asked.indexOf(`${BASE}meta/item/45146.json`),
    );
    expect(asked).toHaveLength(4);
  });

  it("never probes the wowhead id when the server id's meta exists", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/armor/1/1170.json`]: { ok: true },
    });
    const r = await resolveViewerItems(
      [{ slot: 0, entry: 999, displayid: 1170 }],
      probe,
      new Map([[999, 4242]]),
    );
    expect(r.items).toEqual([[1, 1170]]);
    expect(asked).toEqual([`${BASE}meta/armor/1/1170.json`]);
  });

  it("skips honestly when BOTH the server and wowhead ids miss", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/item/111.json`]: { ok: false },
      [`${BASE}meta/item/222.json`]: { ok: false },
    });
    const r = await resolveViewerItems(
      [{ slot: 15, entry: 5, displayid: 111 }],
      probe,
      new Map([[5, 222]]),
    );
    expect(r).toEqual({ items: [], total: 1 });
    expect(skippedItemsNote(r.total, r.items.length)).toBe(
      "1 of 1 equipped item can't be shown in 3D (no Wowhead model data).",
    );
  });

  it("keeps the best-guess SERVER id on a probe failure instead of guessing on the override", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/item/111.json`]: null,
    });
    const r = await resolveViewerItems(
      [{ slot: 15, entry: 5, displayid: 111 }],
      probe,
      new Map([[5, 222]]),
    );
    expect(r.items).toEqual([[21, 111]]);
    expect(asked).toEqual([`${BASE}meta/item/111.json`]);
  });

  it("resolves the override through the per-slot ladders too (robe at 20 via wowhead id)", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/armor/5/300.json`]: { ok: false },
      [`${BASE}meta/armor/20/300.json`]: { ok: false },
      [`${BASE}meta/armor/5/400.json`]: { ok: false },
      [`${BASE}meta/armor/20/400.json`]: { ok: true },
    });
    const r = await resolveViewerItems(
      [{ slot: 4, entry: 7, displayid: 300 }],
      probe,
      new Map([[7, 400]]),
    );
    expect(r.items).toEqual([[20, 400]]);
    expect(asked).toEqual([
      `${BASE}meta/armor/5/300.json`,
      `${BASE}meta/armor/20/300.json`,
      `${BASE}meta/armor/5/400.json`,
      `${BASE}meta/armor/20/400.json`,
    ]);
  });

  it("rescues an item whose server displayid is 0 when wowhead knows it", async () => {
    const { probe } = probeFrom({
      [`${BASE}meta/armor/5/777.json`]: { ok: true },
    });
    const r = await resolveViewerItems(
      [{ slot: 4, entry: 8, displayid: 0 }],
      probe,
      new Map([[8, 777]]),
    );
    expect(r).toEqual({ items: [[5, 777]], total: 1 });
  });

  it("ignores overrides for entries the doll doesn't wear and items without an entry key", async () => {
    const { probe, asked } = probeFrom({
      [`${BASE}meta/armor/1/11.json`]: { ok: true },
    });
    const r = await resolveViewerItems(
      [{ slot: 0, displayid: 11 }],
      probe,
      new Map([[12345, 999]]),
    );
    expect(r.items).toEqual([[1, 11]]);
    expect(asked).toEqual([`${BASE}meta/armor/1/11.json`]);
  });
});

describe("skippedItemsNote", () => {
  it("is null when nothing was skipped (including a naked doll)", () => {
    expect(skippedItemsNote(0, 0)).toBeNull();
    expect(skippedItemsNote(12, 12)).toBeNull();
  });

  it("counts the dropped items against the renderable total", () => {
    expect(skippedItemsNote(12, 10)).toBe(
      "2 of 12 equipped items can't be shown in 3D (no Wowhead model data).",
    );
    expect(skippedItemsNote(5, 0)).toBe(
      "5 of 5 equipped items can't be shown in 3D (no Wowhead model data).",
    );
  });

  it("uses the singular form for a single-item doll", () => {
    expect(skippedItemsNote(1, 0)).toBe(
      "1 of 1 equipped item can't be shown in 3D (no Wowhead model data).",
    );
  });

  it("never emits a note on nonsense inputs (shown > total)", () => {
    expect(skippedItemsNote(3, 5)).toBeNull();
  });
});

// Ground truth for the sheathing tests: the live-tree viewer.min.js
// decompile traced in .superpowers/sdd/sheathe-report.md -- the character
// actor's setSheath(main, off) speaks the client SheatheType vocabulary
// (1 = 2H back, 2 = staff back, 3 = 1H hips, 4 = shield, 7 = both crossed
// on the back), applied via the engine's Lr/Ir attachment tables.
describe("sheathTypeForItem", () => {
  it("sheathes both Warglaives of Azzinoth crossed on the back (type 7) regardless of meta", () => {
    expect(sheathTypeForItem(21, null, 32837)).toBe(7);
    expect(sheathTypeForItem(22, null, 32838)).toBe(7);
    // The entry special-case must beat the generic 1H-sword rule (subclass
    // 7 -> hip) -- the hip look is exactly what this feature is NOT for.
    expect(sheathTypeForItem(21, { itemClass: 2, itemSubClass: 7 }, 32837)).toBe(7);
  });

  it("puts two-handers on the back (type 1): 2H axe/mace/polearm/2H sword/fishing pole", () => {
    for (const sub of [1, 5, 6, 8, 20]) {
      expect(sheathTypeForItem(21, { itemClass: 2, itemSubClass: sub })).toBe(1);
    }
  });

  it("gives staves their own back angle (type 2)", () => {
    expect(sheathTypeForItem(21, { itemClass: 2, itemSubClass: 10 })).toBe(2);
  });

  it("puts one-handers on the hips (type 3): 1H axe/mace/sword/fist/dagger", () => {
    for (const sub of [0, 4, 7, 13, 15]) {
      expect(sheathTypeForItem(21, { itemClass: 2, itemSubClass: sub })).toBe(3);
      expect(sheathTypeForItem(22, { itemClass: 2, itemSubClass: sub })).toBe(3);
    }
  });

  it("mounts shields on the back (type 4) from the resolved slot alone", () => {
    expect(sheathTypeForItem(14, null)).toBe(4);
  });

  it("gives held frills (resolved slot 22, meta InventoryType 23) no sheathed pose (type 0)", () => {
    // Production never yields a slot-23 row: resolveViewerItemForId lands
    // every non-shield off-hand at 22, so a frill is identified by its
    // meta InventoryType, not by the resolved slot.
    expect(sheathTypeForItem(22, { itemClass: 4, inventoryType: 23 })).toBe(0);
  });

  it("falls back to the generic back position (type 1) when the meta is unavailable", () => {
    expect(sheathTypeForItem(21, null)).toBe(1);
    expect(sheathTypeForItem(22, { itemClass: 4 })).toBe(1);
  });
});

describe("deriveSheathValues", () => {
  const fetcherFrom = (
    answers: Record<
      number,
      { itemClass?: number; itemSubClass?: number; inventoryType?: number } | null
    >,
  ) => {
    const asked: number[] = [];
    const fetchMeta = async (displayId: number) => {
      asked.push(displayId);
      return answers[displayId] ?? null;
    };
    return { fetchMeta, asked };
  };

  it("derives 7/7 for the Warglaives by entry WITHOUT any meta fetch", async () => {
    const { fetchMeta, asked } = fetcherFrom({});
    const r = await deriveSheathValues(
      [
        [21, 45150],
        [22, 45146],
      ],
      [
        { slot: 15, entry: 32837 },
        { slot: 16, entry: 32838 },
      ],
      fetchMeta,
    );
    expect(r).toEqual({ main: 7, off: 7 });
    expect(asked).toEqual([]);
  });

  it("derives a 2H main hand from the meta (Trashbringer: 2H sword -> back)", async () => {
    const { fetchMeta, asked } = fetcherFrom({ 23875: { itemClass: 2, itemSubClass: 8 } });
    const r = await deriveSheathValues([[21, 23875]], [{ slot: 15, entry: 1 }], fetchMeta);
    expect(r).toEqual({ main: 1, off: -1 });
    expect(asked).toEqual([23875]);
  });

  it("derives 1H + shield with a single meta fetch (shield needs none)", async () => {
    const { fetchMeta, asked } = fetcherFrom({ 100: { itemClass: 2, itemSubClass: 7 } });
    const r = await deriveSheathValues(
      [
        [21, 100],
        [14, 200],
      ],
      [
        { slot: 15, entry: 10 },
        { slot: 16, entry: 11 },
      ],
      fetchMeta,
    );
    expect(r).toEqual({ main: 3, off: 4 });
    expect(asked).toEqual([100]);
  });

  it("gives a lone ranged weapon a sheath value so the engine's Lr fallback engages", async () => {
    const { fetchMeta, asked } = fetcherFrom({});
    const r = await deriveSheathValues([[15, 400]], [{ slot: 17, entry: 12 }], fetchMeta);
    expect(r).toEqual({ main: 1, off: -1 });
    expect(asked).toEqual([]);
  });

  it("derives a held frill off-hand (resolved slot 22) from its meta InventoryType", async () => {
    const { fetchMeta, asked } = fetcherFrom({ 600: { itemClass: 4, inventoryType: 23 } });
    const r = await deriveSheathValues([[22, 600]], [{ slot: 16, entry: 13 }], fetchMeta);
    expect(r).toEqual({ main: -1, off: 0 });
    expect(asked).toEqual([600]);
  });

  it("is -1/-1 (nothing to sheathe) for an armor-only or empty doll", async () => {
    const { fetchMeta, asked } = fetcherFrom({});
    expect(
      await deriveSheathValues(
        [
          [1, 5],
          [5, 6],
        ],
        [],
        fetchMeta,
      ),
    ).toEqual({ main: -1, off: -1 });
    expect(await deriveSheathValues([], [], fetchMeta)).toEqual({ main: -1, off: -1 });
    expect(asked).toEqual([]);
  });

  it("degrades to the generic back position when the meta fetch fails", async () => {
    const { fetchMeta } = fetcherFrom({ 500: null });
    const r = await deriveSheathValues([[21, 500]], [{ slot: 15, entry: 14 }], fetchMeta);
    expect(r).toEqual({ main: 1, off: -1 });
  });
});

describe("applyViewerSheath", () => {
  const recordingViewer = () => {
    const calls: [string, unknown[]][] = [];
    return {
      calls,
      viewer: {
        method(name: string, args: unknown[]) {
          calls.push([name, args]);
        },
      },
    };
  };

  it("sends the derived pair when sheathing and -1/-1 when drawing", () => {
    const { calls, viewer } = recordingViewer();
    applyViewerSheath(viewer, { main: 7, off: 7 }, true);
    applyViewerSheath(viewer, { main: 7, off: 7 }, false);
    expect(calls).toEqual([
      ["setSheath", [7, 7]],
      ["setSheath", [-1, -1]],
    ]);
  });

  it("treats missing values as unsheathed", () => {
    const { calls, viewer } = recordingViewer();
    applyViewerSheath(viewer, null, true);
    expect(calls).toEqual([["setSheath", [-1, -1]]]);
  });

  it("never throws on a null/method-less/throwing viewer (best-effort like destroyViewer)", () => {
    expect(() => applyViewerSheath(null, { main: 1, off: -1 }, true)).not.toThrow();
    expect(() => applyViewerSheath({}, { main: 1, off: -1 }, true)).not.toThrow();
    expect(() =>
      applyViewerSheath(
        {
          method() {
            throw new Error("boom");
          },
        },
        { main: 1, off: -1 },
        true,
      ),
    ).not.toThrow();
  });
});

describe("sheathed pref (guarded storage)", () => {
  it("defaults to false and survives a write without localStorage (node env)", () => {
    expect(readSheathedPref()).toBe(false);
    expect(() => writeSheathedPref(true)).not.toThrow();
    expect(readSheathedPref()).toBe(false);
  });
});

describe("buildCharacterModelId", () => {
  it("matches race*2-1+gender with AC's own 0=male/1=female convention (verified vs wrath meta)", () => {
    expect(buildCharacterModelId(2, 0)).toBe(3); // orc male -- meta character/3.json Race=2 Gender=0
    expect(buildCharacterModelId(2, 1)).toBe(4); // orc female -- meta character/4.json Race=2 Gender=1
    expect(buildCharacterModelId(8, 1)).toBe(16); // troll female -- meta character/16.json
    expect(buildCharacterModelId(1, 0)).toBe(1); // human male
  });
});

describe("releaseGlContexts", () => {
  it("loses the WebGL context of each canvas so the render loop stops spinning the GPU", () => {
    const loseContext = vi.fn();
    const gl = {
      getExtension: (name: string) => (name === "WEBGL_lose_context" ? { loseContext } : null),
    };
    // getContext returns the existing context only for the type it was created
    // with (here webgl2) and null for the others -- exactly the real behavior.
    const canvas = { getContext: (t: string) => (t === "webgl2" ? gl : null) };
    const container = { querySelectorAll: () => [canvas] } as unknown as HTMLElement;
    releaseGlContexts(container);
    expect(loseContext).toHaveBeenCalledTimes(1);
  });

  it("is a safe no-op for a null container", () => {
    expect(() => releaseGlContexts(null)).not.toThrow();
  });

  it("does not throw when a canvas exposes no WebGL context", () => {
    const canvas = { getContext: () => null };
    const container = { querySelectorAll: () => [canvas] } as unknown as HTMLElement;
    expect(() => releaseGlContexts(container)).not.toThrow();
  });
});
