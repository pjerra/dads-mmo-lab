# Modules page — round 3 design (2026-08-17)

Launcher-only round, from click-through feedback on the live VM after round 2
shipped. Eleven items: chrome that stays visible while scrolling, NPC
information surfaced where the user is already looking, confirmations on the
destructive buttons, and honesty about which modules can actually be toggled.

**No new backend.** Every call this round needs already exists. Nothing here
triggers a worldserver rebuild — deliberately, see §7.

## Decisions taken (user, 2026-08-17)

1. **Toggles: real switches only.** A module without a genuine runtime on/off
   gets a DISABLED control naming why, not a switch that lies. (Rejected:
   writing `Enable=0` into any conf — most modules never read such a key, so
   the toggle would silently do nothing; and rebuild-to-disable, which makes
   every toggle a 30–90 minute operation.)
2. **Confirmations on all four**: restart/stop world server, rebuild, remove
   module, place-NPC/fixit.
3. **NPC data: honest gaps.** Modules we have no data for show nothing rather
   than a guess.
4. **"Restart world server" means the FULL container restart**, not the SOAP
   world-restart — it is the one that applies config changes, which is what a
   user wants right after a setup step.

### Deviation from what was approved in chat, and why it is better

The chat design said §3 would add a hand-maintained `npcEntry` to each of the
seven `SETUP_CATALOG` entries. Reading the code first showed that is
duplication: `commands::cmd_block_for(key)` already carries lines of the exact
shape

```
npc add 999991 0 -8828.3 630.2 94.1 3.7   — Stormwind Arena Battlemaster (Alliance)
```

`moduletail::npc_coord_specs` parses those same lines to decide where to spawn,
and the whole block is already exposed to the frontend through
`wow_commands_read`. So the id, the map, the coordinates AND the human label
are all present with no new data at all. Hand-maintaining a second copy would
create exactly the drift the launcher has been bitten by before (a catalog and
a resolver disagreeing about the same module).

**Consequence worth stating:** this covers any module whose command block
carries `npc add` lines, which is a superset of the four in `PLACE_NPC_KEYS`.
It still does not cover a URL-installed module with no command block — that
gap is real and stays (§8).

## 1. Sticky toolbar

A toolbar pinned above the tab strip so it survives scrolling the tab body:

| Side | Controls |
|---|---|
| Left | Refresh · Check for updates |
| Right | Restart world server |

These move from wherever they sit today; they are not duplicated. The bar uses
`position: sticky` inside the page's existing scroll container.

## 2. Restart inside the setup panel

A third button beside *Place NPC in capitals* / *Mark as done*. Several
catalog entries end with "Restart the world server for it to appear" — this
makes that step actionable without leaving the panel.

One restart function, three call sites (toolbar, setup panel, and whatever
exists today). It is the FULL container restart per decision 4.

## 3. NPC id and spawn command

New pure module `launcher/src/lib/npc-commands.ts`:

```ts
export interface NpcLine { entry: number; map: number; x: number; y: number; z: number; o: number; label: string | null; }
export function parseNpcLines(block: string): NpcLine[];
export function spawnCommandFor(l: NpcLine): string;   // ".npc add <entry>"
export function npcEntriesFor(block: string): number[]; // deduped, source order
```

`parseNpcLines` mirrors `moduletail::npc_coord_specs`'s token discipline
(`npc add` + 6 tokens, uint entry/map, signed-decimal coords) so the two agree
on what counts as a spawn line. It additionally keeps the trailing `— label`
that the Rust parser discards, because the label is the whole point on screen.

Surfaced in three places:

* **Needs-setup panel** — "NPC 999991" plus a copyable `.npc add 999991`, and
  the label when present.
* **Modules tab** row description — `NPC 999991`.
* **Tuning tab** row description — same.

Where a module has several distinct entries (Alliance/Horde battlemasters),
all are listed. Where it has none, nothing is shown.

## 4. Place-NPC reports where it spawned

`wow_module_place_npc` already returns `maps: [{ map, placed }]`. Add a pure
map-id → capital name mapping and render the outcome:

* newly placed in both → "Placed in Stormwind and Orgrimmar."
* already there → "Already present in Stormwind and Orgrimmar."
* mixed → names each side honestly.

Map names come from the coordinates' own capital, not from the map id alone
(map 0 is Eastern Kingdoms, not "Stormwind"), so the mapping lives beside the
parsed `NpcLine` and falls back to "map N" for anything unrecognised.

## 5. Mark-as-done hides the place action

Once a module is marked done (localStorage, keyed by server —
`setupDoneKey`), its place-NPC/fixit action is not rendered. The NPC id and
spawn command REMAIN visible: they are reference information, not a step.

## 6. Confirmations

Reuses the page's existing two-click confirm (the `confirmingUpdate` pattern:
first click arms and shows a sentence, second click acts, Cancel disarms).
Applied to restart/stop, rebuild, remove module, and place-NPC/fixit.

New pure module `launcher/src/lib/confirm-actions.ts` holding the registry of
which actions confirm and the sentence each shows, so the set is data and can
be asserted by a test rather than being scattered through markup.

## 7. Toggles: real switches, honest refusals

`module-toggle.ts` already decides eligibility from the tuning registry
(a row whose key ends `.enable`/`.enabled` and whose module matches). This
round adds the NEGATIVE case: a module with no such row renders a disabled
control with the reason — "No on/off setting — remove it to disable."

`toggleReason(rows, key)` returns `{ kind: "switch", spec } | { kind: "none" }`
so the component never has to re-derive it.

**Explicitly NOT doing:** rebuild-to-disable. That is the only fully general
answer and it costs 30–90 minutes per flip. Filed as a follow-up, not built.

## 8. What still is not automatic

A module installed from a URL gets a setup notice only if its key happens to
be one of `SETUP_CATALOG`'s seven, and NPC information only if it ships a
command block. Nothing scans an unknown module's SQL to discover it needs an
NPC placed.

This is a known, named gap rather than an oversight. Closing it means parsing
the installed module's own `.sql` for `creature_template` inserts and guessing
which entry matters — a module can ship several. Filed as a follow-up.

## 9. Remove the "conf active" text

`ModuleManager.svelte:1101`'s `<span class="muted">conf active</span>` is
deleted: the presence of the config-tuning button already says it.

**KEEPING** `confActivationChip`'s "conf not activated" chip — that one
appears only on real failure (`no-dist`/`no-conf`/`error`) and is the only
signal a module's conf never landed.

## Testing

Pure logic in `.ts` modules under vitest, Svelte kept thin:

| Module | Asserted |
|---|---|
| `npc-commands.ts` | parses real command blocks; ignores malformed lines the Rust parser also ignores; keeps labels; dedupes entries; empty for blocks with none |
| `capital-names.ts` | map+coords → capital; unknown → "map N" |
| `confirm-actions.ts` | every destructive action is in the registry; each has a sentence |
| `module-toggle.ts` | existing eligibility unchanged; new `toggleReason` returns `none` with a reason for a module with no master switch |

**Parity assertion:** one test feeds the SAME command blocks to
`parseNpcLines` and compares the `(entry, map, x, y, z, o)` tuples against the
values `moduletail`'s own Rust tests pin, so the TS and Rust parsers cannot
drift on what counts as a spawn line.

## Files

New: `launcher/src/lib/npc-commands.ts` (+test), `capital-names.ts` (+test),
`confirm-actions.ts` (+test).
Modified: `ModuleManager.svelte` (toolbar, panel button, descriptions, confirm
wiring, conf-active removal), `module-toggle.ts` (+`toggleReason`),
`setup-catalog.ts` (mark-done hides the action).

No Rust, no bash, no new Tauri command.

## Out of scope, filed as follow-ups

1. Rebuild-to-disable for modules with no runtime switch.
2. SQL scanning to detect setup needs for URL-installed modules.
