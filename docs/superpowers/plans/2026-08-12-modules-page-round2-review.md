# Modules-page round 2 — Task 5 review

Date: 2026-08-12. Scope: `git diff 5d6cb0f..HEAD` (the four round-2 code
commits `d09a246`, `14ee10f`, `90f5685`, `ca1e10c`) against the spec
`docs/superpowers/specs/2026-08-12-modules-page-round2-design.md`.

## Lens results

- **(a) User items 1–5 present.** 1: setup/conf-failed chips render as
  `<button class="mchip">` and dispatch through `onChipClick` → the same
  setup panel the removed link opened. 2: `autoConfCatchup` sets no note on
  success (the round-1 string "Activated default configs" appears 0 times in
  `launcher/src/`); failures map through `confActivationChip()` to a
  clickable "conf not activated" chip whose panel shows message + hint.
  3: rows carry a `Config tuning` action routed via `requestTuning`; every
  Tuning-tab card header gained "Open config file" through the existing
  `requestConfFile` → ModuleFiles surface; the raw `.conf` filename left the
  rows. 4: ONE `{#snippet moduleRow}` feeds all six sections (installed +
  available × cpp/lua/sql) via `buildModuleRow`; one `.mchip` style block;
  one `.modrow` grid/height/separator. 5: `moduleToggle()` grants a toggle
  only on a registry-declared `.enable`/`.enabled` master switch, refuses
  `mod-playerbots` unconditionally (pinned by test), writes through the
  existing `wowConfigTuningSet`.
- **(b) Launcher-only.** `git diff --stat 5d6cb0f..HEAD` touches 10 files,
  all under `launcher/src/`.
- **(c) No new write surface.** The toggle reuses the tuner write; "Open
  config file" reuses the ModuleFiles surface (`ModuleFiles.svelte`
  untouched, allowlist untouched); no api.ts changes.
- **(d) Alignment unified.** `grep -c "moduleRow(" ModuleManager.svelte` = 4
  (1 snippet + 3 family wrappers); the old `.mrow` style survives only for
  the Server-update card's repo rows, with a comment saying so.
- **(e) A11y.** Chips are real `<button>`s with visible text labels; the
  toggle is `<button role="switch" aria-checked aria-label>`; keyboard
  focus order follows DOM order inside the fixed action column.

## Findings

### Important (fixed in place) — 1

1. **Modules tab showed no restart banner after a conf-backend toggle.**
   `toggleModule()` raises `noteApplyNeeded(...)`, but ModuleManager's
   modules section rendered no `restartState` banner — the only visible copy
   sat on the co-mounted, `display:none` Tuning tab. Every other raising
   surface (Tuning, Files, Config, GMTools, Home) renders the warn-card.
   The user's spec item 5 promises "raises the existing restart banner";
   with the banner off-screen the toggle produced zero visible feedback on
   the tab where the click happened. **Fix:** the same
   `{#if restartState.needed}` warn-card now renders in the Modules section,
   under the error card.

### Minors (recorded, not fixed) — 3

1. **Chip background differs by clickability.** Clickable `.mchip`s are
   `<button>`s and inherit the component-wide `button` background
   (`#21262d`); non-clickable chips are `<span>`s with a transparent
   background. Size/padding/radius/colors are shared (the one-block rule
   holds); only the background token diverges. One-line CSS if the user
   notices in the click-through.
2. **"Open config file" tooltip overpromises.** The Tuning-tab button's
   title says "Edit {conf} directly on the Module files tab", but files
   outside the raw-write allowlist open read-only there (the surface
   enforces this correctly — copy only).
3. **`stripModPrefix` strips any leading "mod".** A normalized name like
   "modernsomething" would compare as "ernsomething", so a pathological
   registry/module pairing could mis-match. The registry is curated and no
   current entry is affected — theoretical.

Findings: 1 Important (fixed), 3 Minor (listed). Full vitest + svelte-check
re-run clean after the fix (see Task 5 commit).
