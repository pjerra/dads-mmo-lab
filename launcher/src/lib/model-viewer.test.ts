import { describe, expect, it } from "vitest";
import {
  AC_TO_INVENTORY_TYPE,
  buildCharacterModelId,
  displayIdCandidates,
  resolveViewerItems,
  skippedItemsNote,
  viewerMetaUrl,
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

describe("buildCharacterModelId", () => {
  it("matches race*2-1+gender with AC's own 0=male/1=female convention (verified vs wrath meta)", () => {
    expect(buildCharacterModelId(2, 0)).toBe(3); // orc male -- meta character/3.json Race=2 Gender=0
    expect(buildCharacterModelId(2, 1)).toBe(4); // orc female -- meta character/4.json Race=2 Gender=1
    expect(buildCharacterModelId(8, 1)).toBe(16); // troll female -- meta character/16.json
    expect(buildCharacterModelId(1, 0)).toBe(1); // human male
  });
});
