# Character Viewer Upgrade — Design Spec (Round E)

**Date:** 2026-07-18
**Branch:** `feat/dml-launcher-windows`
**Status:** Design review waived (standing user instruction).

## What the user asked for

The Dashboard character viewer should look like the in-game character pane: item **icons**
in equipment slots, and **hovering an item shows its stats** in a WoW-style tooltip —
"there should be a way with wowhead".

## Design

**Wowhead is the data source for standard items; the server DB is the fallback for custom
ones.** Wowhead's tooltip endpoint (verified live) returns everything needed in one call:

```
GET https://nether.wowhead.com/wotlk/tooltip/item/<entry>?dataEnv=8&locale=0
→ {"name","quality","icon","tooltip"}   (tooltip = in-game-structured HTML, .q/.q0-.q7 classes)
GET https://wow.zamimg.com/images/wow/icons/large/<icon>.jpg   (the item icon)
```

Custom items (the user's casino/module items) don't exist on wowhead — those fall back to a
basic tooltip built from the server's own `item_template` (name, quality, item level, armor,
primary stats, damage/speed, required level). This hybrid beats embedding wowhead's remote
tooltip script: it works offline after first fetch, needs no remote JS in the webview, and
handles server-custom items honestly.

### CLI: `dml wow item-info --entries <csv> --json` (new `cli/src/46-iteminfo.sh` + arm)

- `--entries` = comma-separated item entry ids, `^[0-9]+(,[0-9]+)*$`, each `10#`-normalized,
  deduped, **max 25** (a paperdoll is ≤19) — violations `BAD_ARG`.
- **Disk cache** under `~/.dml/wowhead-cache/`: `tooltips/<entry>.json` (raw wowhead JSON)
  and `icons/<icon>.jpg`. Cache hits never touch the network.
- Per entry, in order: cached → use; else `curl -s -w '\n%{http_code}' --max-time 10` the
  tooltip URL — HTTP 200 → cache + use; anything else (404 = custom item, offline, timeout)
  → **local fallback**: `item_template` query building a minimal tooltip HTML
  (`<b class="qN">Name</b><br>Item Level X…` with armor/stats/damage lines when non-zero);
  DB also unreachable → `source:"unavailable"` entry (UI still shows the paperdoll name).
- Icons: wowhead-sourced items fetch/cache the jpg once and return it **base64-inlined**
  (`icon_b64`) so the webview needs no external image loads; local/unavailable items have
  `icon:null` (UI renders a quality-colored placeholder slot).
- Output: `{"items":[{entry, source:"wowhead"|"local"|"unavailable", name, quality,
  icon (name|null), icon_b64 (b64|null), tooltip_html}]}`. The verb NEVER fails on
  network/DB trouble — degradation is per-item data, not an error envelope.
- Test seams: `DML_WOWHEAD_BASE` / `DML_ZAMIMG_BASE` env overrides for the two URL bases
  (default the real hosts) so bats drives the curl stub; curl stub gains a sequenced
  response mechanism (`DML_STUB_CURL_SEQ`, mirroring `DML_STUB_DB_ROWS_SEQ`).

### Plumbing

`wow_item_info(entries: Vec<u32>)` Rust command (joins as csv, `run_json_cmd`);
`wowItemInfo(entries: number[]): Promise<ItemInfo[]>` in api.ts.

### Dashboard UI — in-game-style paperdoll

- Replace the flat gear table with the **character-pane slot layout**: left column head /
  neck / shoulders / back / chest / shirt / tabard / wrists (AC slots 0,1,2,14,4,3,18,8),
  right column hands / waist / legs / feet / ring / ring / trinket / trinket
  (9,5,6,7,10,11,12,13), bottom row main hand / off hand / ranged (15,16,17). Character
  name/level/class/gold beside the grid; the "last save" note stays.
- Each slot: 40px icon box (dark bg, quality-colored 1px border when filled, dim empty
  slot otherwise). Icon = `data:image/jpeg;base64,<icon_b64>`; no-icon items show a
  quality-colored square with the item's first letter.
- **Hover tooltip** (`WowTooltip` markup inside Dashboard or its own component): WoW-styled
  — near-black blue-tinted background, thin gold border, rounded corners; renders the
  (sanitized) `tooltip_html`; wowhead's classes styled to game colors: `.q` yellow
  `#ffd100`, `.q0`-`.q7` = the existing `QUALITY_COLORS`, `.q1` white body text, `.q2`
  green `#1eff00` for equip/use lines; positioned beside the hovered slot, kept inside the
  viewport; disappears on mouse-out. Also shown on the no-data case as name-only (from
  paperdoll data) so hover always answers.
- **Sanitizer** (`launcher/src/lib/tooltip.ts`, `sanitizeTooltipHtml`): wowhead HTML is
  remote content injected via `{@html}` — DOMParser walk with a tag allowlist
  (`table,tbody,tr,td,th,span,div,b,i,small,br`), `<a>` becomes `<span>`, ALL attributes
  stripped except `class` (validated `/^[\w -]+$/`), everything else (script, style,
  event handlers, images) dropped. Unit-tested in vitest.
- Fetch flow: after `wowPaperdoll` returns, one `wowItemInfo` call for all equipped
  entries; a module-level `Map` memoizes per-entry results for the session (the CLI disk
  cache covers cross-session). Tooltip data loading is non-blocking — slots render
  immediately with placeholders, icons fill in when the call lands.

### Testing

- **bats** (`wow-item-info.bats`, ~8): entries validation (bad csv / >25); cache hit skips
  curl (capture file absent); 200 → fields + both cache files written; 404 → local
  fallback built from mysql-stub rows (name/quality/ilvl line assertions); curl dead + DB
  dead → `source:"unavailable"`; dedup; icon_b64 round-trips (b64 of the stub jpg);
  second call is served from cache.
- **vitest**: `tooltip.test.ts` — script/onerror stripped, `a`→`span`, class kept, unknown
  tags dropped, text preserved (+4 tests).
- Gates: full bats, vitest, cargo, check. Baselines entering E: bats 329, vitest 20,
  cargo 18, check 0/0.
- **Live gate (batched)**: view a real character — icons render, hovering a standard item
  shows the wowhead tooltip, hovering a custom/casino item shows the local-fallback
  tooltip, second view is instant (cache).

### Out of scope

Enchant/gem display beyond what wowhead's HTML carries, socket-bonus computation,
item-set aggregation, talents/auras, live (`.pinfo`) inventory (paperdoll stays
last-saved), Items page icon-ification (same machinery could later reuse `item-info`).
