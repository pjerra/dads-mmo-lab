# Modules-Page Round 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Execution harness (user directive):** the SAME worktree and workflow as
> round 1 — `C:/Users/perzi/dml-desks/modules-round`, branch
> `feat/modules-page-round`, one Workflow, **5 agents max, strictly
> sequential**. Script: `.claude/workflows/modules-page-round2.js`.

**Goal:** Apply the user's five click-through feedback items: chip-as-action, silent-success conf activation, uniform action column + Config-tuning/Open-config buttons, one row style across sections, and an honest per-module disable toggle.

**Architecture:** Launcher-only (Svelte 5 + pure TS helpers). No CLI, Rust, or contract changes; only existing api.ts commands. Spec: `docs/superpowers/specs/2026-08-12-modules-page-round2-design.md`.

**Tech Stack:** Svelte 5 (runes + snippets), vitest, svelte-check.

## Global Constraints

- **Launcher-only.** `git diff` for every task touches nothing outside `launcher/src/`. No new Tauri commands; no api.ts signature changes.
- **Raw-write allowlist untouched:** only `playerbots.conf`, `mod_ahbot.conf`, `mod_ale.conf` are editable in file surfaces; everything else opens read-only.
- Pure logic lives in `.ts` helpers pinned by vitest tests written FIRST (fail → implement → pass). UI wiring is proven by `npx vitest run` (full suite, expect ≥ 800 passing, 0 failed) + `npm run check` (svelte-check 0 errors) per task.
- Baselines at branch tip (remeasure before quoting): vitest 800/800 (67 files), svelte-check clean.
- Run vitest from `launcher/`: `npx vitest run` (never judge by a piped tail; read the summary line).
- Commit per task with the EXACT marker prefix given in the task (workflow idempotency depends on it). End commit messages with the standard co-author footer.
- Svelte 5 idiom: shared markup via `{#snippet}`/`{@render}` (round 1 already does this in `ModuleManager.svelte`), state via runes in `.svelte.ts` stores.

---

### Task 1: One row skeleton for all sections — marker `feat(launcher): modules rows share one skeleton with aligned action column`

**Files:**
- Modify: `launcher/src/lib/pages/ModuleManager.svelte` (row snippets → one shared row snippet with slots for chips/actions)
- Create: `launcher/src/lib/module-row.ts` (pure row-model builder)
- Test: `launcher/src/lib/module-row.test.ts`

**Interfaces:**
- Produces `buildModuleRow(m: InstalledModule | CatalogEntry, ctx: RowCtx): ModuleRow` in `module-row.ts` where `ModuleRow = { key: string; title: string; family: 'cpp'|'lua'|'sql'; installed: boolean; chips: RowChip[]; actions: RowAction[] }`, `RowChip = { id: string; kind: 'setup'|'rebuild'|'update'|'conf-failed'; label: string; clickable: boolean }`, `RowAction = { id: 'tune'|'repair'|'remove'|'install'|'toggle'; label: string; disabled?: boolean }`. Tasks 2–4 extend `chips`/`actions` through THIS builder — never by ad-hoc markup.
- The Svelte side renders every section (installed cpp/lua/sql, catalogs) through ONE `{#snippet moduleRow(row)}` — name+status left, chips after the name, actions in a fixed-width right-aligned column (CSS grid column, same order `tune · repair · remove` / `install`).

- [ ] **Step 1:** Write `module-row.test.ts` FIRST: installed cpp module → actions `[tune, repair, remove]` in that order; catalog entry → `[install]`; sql family installed → no `tune` (no conf); chips empty by default. Run: fails (module missing).
- [ ] **Step 2:** Implement `module-row.ts`; tests pass.
- [ ] **Step 3:** Refactor `ModuleManager.svelte`: delete the per-section row markup variants; render all sections through the shared snippet fed by `buildModuleRow`. Keep round 1's installed-first/collapsible-catalog structure (from `module-split.ts`) unchanged. Remove the raw `.conf` filename text from rows (Task 3 re-homes config access; do not add its buttons here — the builder's `tune` action id is enough).
- [ ] **Step 4:** Uniform visual constants: one row height, one chip style block (size/padding/radius shared, kind → color token), one separator style, applied across all sections in this file's styles.
- [ ] **Step 5:** `npx vitest run` full + `npm run check` — both clean. Visually compare sections in the diff (no leftover second row-markup path: `grep -c "moduleRow(" ModuleManager.svelte` matches the section count).
- [ ] **Step 6:** Commit with the task marker.

### Task 2: Chip-as-action + silent success / failure chip — marker `feat(launcher): setup chip is the action; conf activation silent on success, chip on failure`

**Files:**
- Modify: `launcher/src/lib/pages/ModuleManager.svelte` (chip click wiring; remove the separate setup link row; remove the success notice)
- Modify: `launcher/src/lib/setup-catalog.ts` (only if the chip model needs a field)
- Create: `launcher/src/lib/conf-activation-chip.ts` (pure: catch-up outcome → chip|null)
- Test: `launcher/src/lib/conf-activation-chip.test.ts` + extend `module-row.test.ts`

**Interfaces:**
- Consumes round 1's catch-up call in `ModuleManager.svelte` (the `wowModuleConfActivate` loop) and its outcome values `activated | already-active | no-dist | no-conf | error`.
- Produces `confActivationChip(outcome: string, err?: string): RowChip | null` — `activated`/`already-active` → `null` (SILENT); `no-dist`/`no-conf`/`error` → `{ kind: 'conf-failed', clickable: true, label: 'conf not activated' }` whose click opens a small panel with the reason + the manual hint. Setup chips: `clickable: true`, click calls the SAME open-setup-panel function the removed link used.

- [ ] **Step 1:** Tests first: outcome mapping (5 cases) red → implement → green. Extend row tests: setup chip present ⇒ clickable.
- [ ] **Step 2:** Wire chips as `<button>`s in the shared row snippet (keyboard focusable); remove the separate "setup" text/link block; remove the success notice emission — search for the round-1 notice string in `ModuleManager.svelte` and delete its success branch, keeping the failure path feeding `confActivationChip`.
- [ ] **Step 3:** Full vitest + svelte-check clean. Grep proof: the success-notice string appears 0 times in `launcher/src/` (`grep -rc` = 0).
- [ ] **Step 4:** Commit with the task marker.

### Task 3: Config tuning action + "Open config file" on the Tuning tab — marker `feat(launcher): config-tuning action on rows; open-config buttons on Tuning tab`

**Files:**
- Modify: `launcher/src/lib/pages/ModuleManager.svelte` (action wiring: `tune` → round 1's `requestTuning(key)` nav store)
- Modify: `launcher/src/lib/ModuleTuning.svelte` (per-module section header gains "Open config file")
- Modify: `launcher/src/lib/module-nav.svelte.ts` (only if the tune request needs the conf name carried along)
- Test: extend `launcher/src/lib/module-nav.test.ts`

**Interfaces:**
- Consumes: `requestTuning` from `module-nav.svelte.ts` (round 1), the existing file-open surface used by `ModuleFiles.svelte` and its read-only/editable split (raw-write allowlist — REUSE its allowlist check, do not restate the list).
- Produces: rows' `tune` action routes to the Tuning tab section for that module (same behavior the name-click has since round 1 — both entry points live). Each Tuning-tab module section header gets an "Open config file" button opening the module's ACTIVE conf via the existing ModuleFiles surface; files outside the allowlist open read-only (the surface already enforces this — verify, don't fork it).
- Known round-1 gap (recorded follow-up): uncurated Lua modules have no tuning card. For THIS round: rows only get the `tune` action when a tuning target exists (curated card or open-config fallback section) — `buildModuleRow` gains a `hasTuningTarget` ctx flag; no dead-end clicks.

- [ ] **Step 1:** Extend `module-nav.test.ts` first (tune request for a module without a curated card resolves to the open-config fallback / suppressed action) — red → implement → green.
- [ ] **Step 2:** Wire the row action + the Tuning-tab buttons. `ModuleTuning.svelte` renders the button from the module's conf name (the data ModuleFiles already has — no new backend call).
- [ ] **Step 3:** Full vitest + svelte-check clean. Manual-diff proof: no new write path — `git diff` contains no changes to any file-writing call.
- [ ] **Step 4:** Commit with the task marker.

### Task 4: Disable toggle from registry-declared switches — marker `feat(launcher): per-module enable/disable toggle from tuning-registry master switches`

**Files:**
- Create: `launcher/src/lib/module-toggle.ts` (pure eligibility + write-request builder)
- Modify: `launcher/src/lib/pages/ModuleManager.svelte` (toggle in the action column)
- Test: `launcher/src/lib/module-toggle.test.ts`

**Interfaces:**
- Consumes the tuning registry rows the launcher already loads for the Tuning tab (keys like `beastmaster.enable`, `learnspells.enable`, `unlimitedammo.enabled`; each row carries `backend: 'conf'|'lua'` and `module`), and the EXISTING tuner write call used when a user edits that row on the Tuning tab (reuse the exact same api.ts function — no new command).
- Produces `moduleToggle(moduleKeyOrTitle: string, registryRows: TuningRow[]): ToggleSpec | null` where `ToggleSpec = { settingKey: string; backend: 'conf'|'lua' }`. Rules pinned by tests: match a row whose key ends in `.enable`/`.enabled` AND whose module matches; `mod-playerbots` → ALWAYS null; no match → null (row shows no toggle). Toggle write goes through the tuner path; conf-backend success raises the existing restart banner, lua-backend shows the existing redeploy note. Current on/off state comes from the same tuner read the Tuning tab uses.
- UI: toggle sits in the action column (fixed slot so rows without it keep alignment — empty slot, not shifted buttons).

- [ ] **Step 1:** `module-toggle.test.ts` FIRST: eligibility (match / no-match / playerbots-refusal / `.enabled` suffix variant), red → implement → green.
- [ ] **Step 2:** Wire the toggle; disabled (greyed, tooltip "restart pending") while the module's write is in flight — reuse `module-busy.svelte.ts`.
- [ ] **Step 3:** Full vitest + svelte-check clean. Mutation proof (the two-halves rule): break the playerbots refusal in `module-toggle.ts` (return a spec) — the named test goes RED — restore with Edit.
- [ ] **Step 4:** Commit with the task marker.

### Task 5: Review vs spec + fix wave — marker `fix(launcher): round-2 review fixes`

- [ ] **Step 1:** Review `git diff <round1-tip>..HEAD` (the round-2 commits only; round-1 tip = the commit before Task 1's marker) against the spec, lenses: (a) every user item 1–5 demonstrably present; (b) launcher-only (`git diff --stat` shows only `launcher/src/`); (c) no new write surface, allowlist untouched; (d) alignment actually unified — ONE row snippet, ONE chip style block, no orphaned styles; (e) a11y: chips/toggles are real buttons with labels.
- [ ] **Step 2:** Fix Critical/Important in place; list Minors in `docs/superpowers/plans/2026-08-12-modules-page-round2-review.md`. Re-run full vitest + svelte-check after fixes.
- [ ] **Step 3:** Commit with the task marker (include findings count).

## Self-review notes (plan time)

- Spec item→task: 1→T2, 2→T2, 3→T1(remove filename)+T3, 4→T1, 5→T4; review→T5. Non-goals respected: no api.ts/Rust/bash edits anywhere.
- Type consistency: `RowChip.kind 'conf-failed'` (T1) is what T2's `confActivationChip` returns; `RowAction.id 'toggle'` (T1) is what T4 renders; `hasTuningTarget` ctx (T3) extends T1's builder.
- Open risk, accepted: exact api.ts function names for the tuner read/write are discovered by the T4 agent from the Tuning tab's existing wiring (they exist and are in use; the plan deliberately does not guess their names).
