# Feature Locks for Untested Features — Design Spec (Round K0)

**Date:** 2026-07-18 · **Branch:** `feat/dml-launcher-windows` · User instruction: "disable the untested features until we do the smoke tests for each" + a living smoke list.

## Design

- `docs/SMOKE-TESTS.md` (already committed) is the living checklist; bracketed keys map to flags.
- **`launcher/src/lib/features.svelte.ts`** (shared reactive module):
  - `export const FEATURES: Record<string, "tested" | "untested">` — one entry per bracketed key in SMOKE-TESTS.md (all `"untested"` today): `restart, console-send, title-install, title-remove, teleport-named, teleport-coords, gm-actions, gm-summon, gm-atlogin, mail-item, party-ops, party-botcmd, bot-level, party-presets, preset-io, settings-save, config-edit, ale-reload, modules-cpp, modules-rebuild, modules-conf, modules-lua, modules-sql, client-path, module-repair, backup-create, backup-restore, accounts, bridge-setup`.
  - Reactive testing-mode state persisted to `localStorage["dml.testingMode"]` (guarded try/catch; default off): `testingModeOn(): boolean`, `setTestingMode(on: boolean)`.
  - `featureLocked(key: string): boolean` → `FEATURES[key] === "untested" && !testingModeOn()`; unknown key → `false` (fail-open for reads; every mutating key must be registered).
  - Pure decision helper `lockedFor(status, testingOn)` exported for vitest.
  - `export const LOCKED_HINT = "Untested — enable untested features in Settings to try it (see docs/SMOKE-TESTS.md)"`.
- **Settings toggle** (Config page, settings tab, bottom card): checkbox `Enable untested features (for smoke testing)` + muted line `Untested features stay disabled until their smoke test passes. The checklist lives in docs/SMOKE-TESTS.md.` The toggle itself is NEVER locked.
- **Page sweep**: every mutating control from the key list gets `disabled={… || featureLocked("<key>")}` and, when locked, `title={LOCKED_HINT}`. Read-only surfaces (status, lists, tooltips, logs view, tracking diagnosis) are never locked. Server start/stop stays unlocked (core lifecycle, long in use). Where a whole card is one feature (e.g. Accounts create), locking the primary buttons suffices — inputs may stay enabled.
- **Process (recorded in memory + SMOKE-TESTS.md header):** each new feature round appends smoke rows + registers its flag as `"untested"`; when the user reports a pass, the flag flips to `"tested"` and the row goes ✅.

## Testing

vitest: `features.test.ts` — `lockedFor` truth table + unknown-key fail-open (+3). Existing suites stay green; check 0/0. Live: toggle off → buttons disabled with hint; toggle on → usable; flag flipped to tested → always usable.
