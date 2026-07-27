// Pure geometry + view-resolution for the shared item hover tooltip.
//
// The paperdoll (CharacterSheet) and the Item Database render the same
// tooltip; only the DOM reads (getBoundingClientRect, post-render height)
// belong to the components. Everything decidable from plain numbers and an
// ItemInfo lives here so it can be tested without a DOM.

import { QUALITY_COLORS } from "./wow";
import type { ItemInfo, WowheadTooltip } from "./api";

/** Tooltip box width used for the does-it-fit test. Keep in sync with CSS. */
export const TOOLTIP_WIDTH = 340;
const GAP = 8;
const MARGIN = 8;

export interface TooltipAnchor {
  top: number;
  /** px from the viewport's left edge, or null when anchored from the right. */
  left: number | null;
  /** px from the viewport's right edge, or null when anchored from the left. */
  right: number | null;
}

/**
 * Place the tooltip beside the hovered target, flipping to the target's left
 * when a right-hand box would overflow the viewport.
 */
export function anchorTooltip(
  rect: { top: number; left: number; right: number },
  viewport: { width: number; height: number },
  width = TOOLTIP_WIDTH,
  gap = GAP,
): TooltipAnchor {
  const flip = rect.right + width > viewport.width;
  return {
    top: rect.top,
    left: flip ? null : rect.right + gap,
    right: flip ? viewport.width - rect.left + gap : null,
  };
}

/**
 * Pull the tooltip back on-screen once its real height is known. The initial
 * `top` is the hovered element's top edge, which runs off the bottom for
 * targets low in the viewport (Feet, Off Hand, the last row of a search).
 */
export function clampTooltipTop(
  top: number,
  tooltipHeight: number,
  viewportHeight: number,
  margin = MARGIN,
): number {
  const maxTop = Math.max(margin, viewportHeight - tooltipHeight - margin);
  return Math.min(Math.max(top, margin), maxTop);
}

export interface HoverItemBasics {
  name: string;
  quality: number;
  item_level?: number;
}

export interface ResolvedItemHover {
  wowhead: WowheadTooltip | null;
  localHtml: string | null;
  label: string;
  color: string;
  sub: string | null;
}

/**
 * What the tooltip should show for one item, given whatever the item-info
 * batch has produced so far. `info` is undefined while the batch is in
 * flight, so every caller gets a usable plain-text tooltip immediately and
 * silently upgrades when the data lands.
 *
 * Only source "wowhead" yields a wowhead tooltip and only "local" yields
 * locally-rendered HTML: "unavailable" (offline, or wowhead doesn't know a
 * server-custom entry) must fall through to the plain fallback rather than
 * rendering an empty box.
 */
export function resolveItemHover(
  info: ItemInfo | undefined,
  item: HoverItemBasics,
): ResolvedItemHover {
  return {
    wowhead: info?.source === "wowhead" ? (info.wowhead ?? null) : null,
    localHtml: info?.source === "local" ? (info.tooltip_html ?? null) : null,
    label: item.name,
    color: QUALITY_COLORS[item.quality] ?? "#c9d1d9",
    sub: item.item_level === undefined ? null : `ilvl ${item.item_level}`,
  };
}
