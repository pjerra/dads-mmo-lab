import { describe, it, expect } from "vitest";
import { anchorTooltip, clampTooltipTop, resolveItemHover } from "./item-tooltip";
import type { ItemInfo } from "./api";

// The paperdoll and the Item Database share one hover tooltip. These are the
// pure pieces of it: where the box goes, and what it should say. The Svelte
// side owns only the DOM reads (getBoundingClientRect) and the render.

describe("anchorTooltip", () => {
  const viewport = { width: 1000, height: 800 };

  it("anchors to the right of the target when there is room", () => {
    const a = anchorTooltip({ top: 100, left: 50, right: 90 }, viewport);
    expect(a.left).toBe(98); // right edge + 8px gap
    expect(a.right).toBeNull();
    expect(a.top).toBe(100);
  });

  it("flips to the left when the tooltip would overflow the right edge", () => {
    // right(700) + width(340) = 1040 > viewport 1000 -> must flip
    const a = anchorTooltip({ top: 100, left: 660, right: 700 }, viewport);
    expect(a.left).toBeNull();
    expect(a.right).toBe(348); // viewport - target.left + gap
  });

  it("does not flip when the tooltip fits exactly", () => {
    // right(660) + width(340) = 1000, exactly the viewport edge
    const a = anchorTooltip({ top: 0, left: 620, right: 660 }, viewport);
    expect(a.left).toBe(668);
    expect(a.right).toBeNull();
  });
});

describe("clampTooltipTop", () => {
  it("keeps a comfortably-placed tooltip where it is", () => {
    expect(clampTooltipTop(100, 200, 800)).toBe(100);
  });

  it("pulls a bottom-anchored tooltip up so it stays on screen", () => {
    // 700 + 200 would run 100px off an 800px viewport
    expect(clampTooltipTop(700, 200, 800)).toBe(592); // 800 - 200 - 8
  });

  it("never pushes the tooltip above the top margin", () => {
    expect(clampTooltipTop(-50, 200, 800)).toBe(8);
  });

  it("prefers the top margin when the tooltip is taller than the viewport", () => {
    expect(clampTooltipTop(300, 900, 800)).toBe(8);
  });
});

describe("resolveItemHover", () => {
  const item = { name: "Thunderfury", quality: 5, item_level: 80 };

  it("uses the wowhead tooltip when wowhead knows the item", () => {
    const info: ItemInfo = {
      entry: 19019,
      source: "wowhead",
      wowhead: { name: "Thunderfury", quality: 5, icon: "inv_sword", tooltip: "<b>Thunderfury</b>" },
    };
    const r = resolveItemHover(info, item);
    expect(r.wowhead?.tooltip).toBe("<b>Thunderfury</b>");
    expect(r.localHtml).toBeNull();
  });

  it("uses the locally-rendered tooltip for server-custom items", () => {
    // Custom entries (the Casino vendor's items, module-added gear) are not on
    // wowhead; the CLI falls back to item_template and marks source "local".
    const info: ItemInfo = {
      entry: 990000,
      source: "local",
      tooltip_html: "<b>Casino Chip</b>",
    };
    const r = resolveItemHover(info, { name: "Casino Chip", quality: 1, item_level: 1 });
    expect(r.localHtml).toBe("<b>Casino Chip</b>");
    expect(r.wowhead).toBeNull();
  });

  it("falls back to the row's own data while the batch is still in flight", () => {
    const r = resolveItemHover(undefined, item);
    expect(r.wowhead).toBeNull();
    expect(r.localHtml).toBeNull();
    expect(r.label).toBe("Thunderfury");
    expect(r.sub).toBe("ilvl 80");
  });

  it("colours the label by quality", () => {
    const r = resolveItemHover(undefined, item);
    expect(r.color).toBe("#ff8000"); // legendary
  });

  it("falls back to a readable colour for an unknown quality", () => {
    const r = resolveItemHover(undefined, { name: "Odd", quality: 99, item_level: 1 });
    expect(r.color).toBe("#c9d1d9");
  });

  it("omits the item-level line when the item has no level", () => {
    const r = resolveItemHover(undefined, { name: "Quest Token", quality: 1 });
    expect(r.sub).toBeNull();
  });

  it("does not treat an unavailable lookup as tooltip content", () => {
    // source "unavailable" means offline / wowhead unreachable: the row must
    // still hover with its plain fallback rather than rendering nothing.
    const info: ItemInfo = { entry: 12345, source: "unavailable" };
    const r = resolveItemHover(info, item);
    expect(r.wowhead).toBeNull();
    expect(r.localHtml).toBeNull();
    expect(r.label).toBe("Thunderfury");
  });
});
