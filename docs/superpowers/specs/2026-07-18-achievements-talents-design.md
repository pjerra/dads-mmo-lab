# Achievements & Talents on the Character View — Design Spec (Round G)

**Date:** 2026-07-18
**Branch:** `feat/dml-launcher-windows`
**Status:** Design review waived (standing user instruction).

## What the user asked for

The Dashboard character view should also show the character's **achievements** and
**talents**.

## Data reality → design

The characters DB has the raw facts (`character_achievement`: achievement id + date;
`character_talent`: talent spell ids per spec group; `characters.activeTalentGroup` /
`talentGroupsCount`) but names/icons/descriptions live in client DBC files the server
doesn't carry. Wowhead's tooltip endpoint covers both kinds with the exact shape Round E
already consumes (verified live):

```
GET <base>/wotlk/tooltip/spell/<id>?dataEnv=8&locale=0        → {name, icon, tooltip}
GET <base>/wotlk/tooltip/achievement/<id>?dataEnv=8&locale=0  → {name, icon, tooltip}
```

So Round G = one new read-only progress verb + a kind-generalized wowhead fetcher
reusing Round E's cache/icon/fallback machinery, + two Dashboard cards reusing the
Round E tooltip/hover pattern.

## CLI

**`dml wow char-progress --char <name> --json`** — read-only, request-response:
- Resolves the character guid (`NOT_FOUND` if absent, `_valid_charname` first,
  `DB_UNREACHABLE` on DB failure — same contract as `paperdoll`).
- `achievements`: `{ total, recent: [{id, date}] }` — total row count and the 10 most
  recent by `date` (unix epoch passed through as a number; UI formats).
- `talents`: `{ groups_count, active_group, spells: [spellId...] }` — spells of the
  ACTIVE spec only (`specMask & (1 << activeTalentGroup)`), ordered by spell id.

**`dml wow entity-info --kind spell|achievement --ids <csv> --json`** — the
kind-generalized sibling of `item-info`, sharing `_iteminfo_fetch` + the icon
cache/b64 logic: per id → cached-or-fetched wowhead JSON (same `{…}` +
`"name":"`/`"tooltip":"` gates, same poisoned-cache drop), icon fetched/cached/
base64-inlined; misses → `{"id":N,"source":"unavailable"}` (NO local fallback —
these kinds have no DB names). Cache layout: `tooltips/<kind>-<id>.json` (items keep
their existing un-prefixed files). Validation identical to item-info (`--ids`
csv regex + `10#` + dedup, max 25; `--kind` closed set). The verb never errors on
network trouble. `item-info` itself is UNTOUCHED.

## UI (Dashboard, below the model + paperdoll row)

- **Talents card**: header `Talents` + summary line `<n> talents (active spec)` with a
  `Dual spec` badge when `groups_count > 1`. Body: an icon grid (28px tiles, same slot
  styling family) of the active spec's talent spells — icon from entity-info, hover →
  the existing sanitized wowhead tooltip (spell tooltips include rank/description).
  Entities not yet loaded/unavailable render a dim placeholder tile with the spell id
  as title text.
- **Achievements card**: header `Achievements` + summary `<total> earned`. Body: the 10
  most recent as rows — icon (20px), name (from entity-info; id as fallback), date
  (`YYYY-MM-DD` from the epoch) — hover → sanitized wowhead tooltip.
- Loading is progressive and non-blocking (cards render immediately with placeholders;
  entity info fills in). The UI chunks ids into ≤25-per-call requests through the
  existing session cache (extended to key by `kind:id`). Failures degrade per-entity;
  the cards and the rest of the Dashboard never break.
- Both cards only render once a character is loaded (same gating as the paperdoll).

**Plumbing:** `wow_char_progress(char_name)` + `wow_entity_info(kind, ids)` Rust
commands, `CharProgress`/`EntityInfo` types + wrappers in api.ts.

## Testing

- bats (`wow-char-progress.bats`): guid lookup + shapes (mysql stub ROWS_SEQ for the
  two queries), specMask filtering math (crafted rows: only active-group talents
  returned), NOT_FOUND, bad char name, DB down; (`wow-entity-info.bats`): kind
  validation (closed set), spell + achievement happy paths (cache files at
  `tooltips/<kind>-<id>.json`), unavailable on 404 (no local fallback), cache hit, and
  the item-info regression canary (its cache path unchanged).
- vitest: date formatting + chunking helper if extracted (pure), else no new UI tests.
- Gates: full bats, vitest, cargo, check. Baselines entering G: bats 340, vitest 32,
  cargo 25, check 0/0.
- **Live gate (batched):** view a character with achievements + talents — cards fill
  in with real icons/names, hover shows spell/achievement tooltips, second view
  instant; a dual-spec character shows the badge and only active-spec talents.

## Out of scope

Talent-tree layout with point counts per tree (needs TalentTab DBC mapping — the flat
icon grid conveys the build via hover tooltips), inactive-spec talent display, glyph
display, achievement categories/points totals, statistics tab, links out to wowhead
pages.
