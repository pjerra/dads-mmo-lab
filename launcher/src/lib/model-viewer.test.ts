import { describe, expect, it } from "vitest";
import {
  AC_TO_VIEWER_SLOT,
  acGenderToViewer,
  buildCharacterModelId,
  buildViewerItems,
  probeRenderableItems,
  viewerFallbackSlot,
} from "./model-viewer";

describe("viewerFallbackSlot", () => {
  it("maps only the recon's three 404-fallback slots", () => {
    expect(viewerFallbackSlot(5)).toBe(20); // chest -> robe
    expect(viewerFallbackSlot(16)).toBe(21); // mainhand
    expect(viewerFallbackSlot(17)).toBe(22); // offhand
    for (const slot of [1, 3, 6, 15, 19]) {
      expect(viewerFallbackSlot(slot)).toBeNull();
    }
  });
});

describe("probeRenderableItems", () => {
  const probeFrom = (answers: Record<string, boolean | null>) => {
    const asked: string[] = [];
    const probe = async (url: string) => {
      asked.push(url);
      // `in`-check, not `??`: a stored null (probe-failed) must survive.
      return url in answers ? answers[url] : false;
    };
    return { probe, asked };
  };

  it("keeps items whose primary meta exists and drops confirmed-missing ones", async () => {
    const { probe } = probeFrom({
      "http://zam.localhost/modelviewer/wrath/meta/armor/1/100.json": true,
      "http://zam.localhost/modelviewer/wrath/meta/armor/7/9999.json": false,
    });
    const kept = await probeRenderableItems(
      [
        [1, 100],
        [7, 9999],
      ],
      probe,
    );
    expect(kept).toEqual([[1, 100]]);
  });

  it("retries the fallback slot before dropping chest/mainhand/offhand items", async () => {
    const { probe, asked } = probeFrom({
      "http://zam.localhost/modelviewer/wrath/meta/armor/16/200.json": false,
      "http://zam.localhost/modelviewer/wrath/meta/armor/21/200.json": true,
    });
    const kept = await probeRenderableItems([[16, 200]], probe);
    expect(kept).toEqual([[16, 200]]);
    expect(asked).toHaveLength(2);
  });

  it("keeps items when the probe itself fails (null) -- never strips gear on a network hiccup", async () => {
    const { probe } = probeFrom({
      "http://zam.localhost/modelviewer/wrath/meta/armor/1/300.json": null,
    });
    const kept = await probeRenderableItems([[1, 300]], probe);
    expect(kept).toEqual([[1, 300]]);
  });
});

describe("AC_TO_VIEWER_SLOT", () => {
  it("maps every rendered AC slot to the recon's viewer slot (acSlot + 1)", () => {
    expect(AC_TO_VIEWER_SLOT[0]).toBe(1); // Head
    expect(AC_TO_VIEWER_SLOT[2]).toBe(3); // Shoulders
    expect(AC_TO_VIEWER_SLOT[3]).toBe(4); // Body (shirt)
    expect(AC_TO_VIEWER_SLOT[4]).toBe(5); // Chest
    expect(AC_TO_VIEWER_SLOT[5]).toBe(6); // Waist
    expect(AC_TO_VIEWER_SLOT[6]).toBe(7); // Legs
    expect(AC_TO_VIEWER_SLOT[7]).toBe(8); // Feet
    expect(AC_TO_VIEWER_SLOT[8]).toBe(9); // Wrists
    expect(AC_TO_VIEWER_SLOT[9]).toBe(10); // Hands
    expect(AC_TO_VIEWER_SLOT[14]).toBe(15); // Back
    expect(AC_TO_VIEWER_SLOT[15]).toBe(16); // Main Hand
    expect(AC_TO_VIEWER_SLOT[16]).toBe(17); // Off Hand
    expect(AC_TO_VIEWER_SLOT[17]).toBe(18); // Ranged
    expect(AC_TO_VIEWER_SLOT[18]).toBe(19); // Tabard
  });

  it("has no entry for neck/ring/trinket slots (the viewer's own NOT_DISPLAYED_SLOTS)", () => {
    for (const slot of [1, 10, 11, 12, 13]) {
      expect(AC_TO_VIEWER_SLOT[slot]).toBeUndefined();
    }
  });
});

describe("buildViewerItems", () => {
  it("maps rendered slots and filters unmapped slots + displayid 0", () => {
    const items = buildViewerItems([
      { slot: 0, displayid: 1170 }, // Head -> mapped
      { slot: 1, displayid: 5555 }, // Neck -> unmapped, dropped
      { slot: 4, displayid: 0 }, // Chest, empty -> dropped
      { slot: 15, displayid: 2222 }, // Main Hand -> mapped
    ]);
    expect(items).toEqual([
      [1, 1170],
      [16, 2222],
    ]);
  });

  it("returns an empty array when every item is unmapped or empty", () => {
    expect(buildViewerItems([{ slot: 1, displayid: 999 }, { slot: 12, displayid: 1 }])).toEqual([]);
  });
});

describe("buildCharacterModelId", () => {
  it("matches the recon's race*2-1+gender formula", () => {
    expect(buildCharacterModelId(7, 1)).toBe(14); // gnome male -- recon's own worked example (id=14)
    expect(buildCharacterModelId(1, 0)).toBe(1); // human female
    expect(buildCharacterModelId(2, 1)).toBe(4); // orc male
  });
});

describe("acGenderToViewer", () => {
  it("pins the AC (0=male/1=female) <-> viewer (0=female/1=male) convention boundary", () => {
    expect(acGenderToViewer(0)).toBe(1); // AC male -> viewer male
    expect(acGenderToViewer(1)).toBe(0); // AC female -> viewer female
  });
});
