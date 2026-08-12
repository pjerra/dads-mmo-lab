// Pure row-model builder for the Modules page (Modules-page round 2, Task 1).
// Every section (installed cpp/lua/sql, catalogs) renders through ONE Svelte
// row snippet fed by buildModuleRow -- Tasks 2-4 extend chips/actions through
// THIS builder, never by ad-hoc markup. No Svelte imports: vitest-testable
// without a component harness (same rule as module-split.ts).

export type ModuleFamily = "cpp" | "lua" | "sql";

export type RowChipKind = "setup" | "rebuild" | "update" | "conf-failed";
export interface RowChip {
  id: string;
  kind: RowChipKind;
  label: string;
  clickable: boolean;
}

export type RowActionId = "tune" | "repair" | "remove" | "install" | "toggle";
export interface RowAction {
  id: RowActionId;
  label: string;
  disabled?: boolean;
}

export interface ModuleRow {
  key: string;
  title: string;
  family: ModuleFamily;
  installed: boolean;
  chips: RowChip[];
  actions: RowAction[];
}

// Structural subset of api.ts's CppModule/LuaModule/SqlModule -- exactly the
// fields the row model needs, so the builder stays pure and family-agnostic.
// cpp/sql carry `installed`; lua carries `cloned`/`deployed` (deployed implies
// installed, matching module-split.ts's luaStatus).
export interface RowModule {
  key: string;
  name: string;
  installed?: boolean;
  cloned?: boolean;
  deployed?: boolean;
}

export interface RowCtx {
  family: ModuleFamily;
  // rare-drops: no automated reversal, the Remove action renders disabled.
  removeDisabled?: boolean;
  // Round 2, Task 2: this module has a setup-catalog entry that hasn't been
  // dismissed yet. ModuleManager.svelte computes this (setupFor() +
  // localStorage) and passes the bool through -- module-row.ts stays free of
  // that dependency, it just slots the resulting chip in.
  needsSetup?: boolean;
  // Round 2, Task 2: this module's auto-activation catch-up failed.
  // ModuleManager.svelte derives the chip via conf-activation-chip.ts's
  // confActivationChip() and passes it straight through.
  confFailChip?: RowChip | null;
  // Round 2, Task 3: whether a real Tuning-tab destination exists for this
  // module (module-nav.ts's hasTuningTarget()) -- false suppresses the tune
  // action so an uncurated module (the round-1 gap) doesn't dead-end into an
  // empty tab. Omitted defaults to true (existing tune behaviour unchanged)
  // -- ModuleManager.svelte opts a row OUT only when it actually knows there
  // is nothing to tune.
  hasTuningTarget?: boolean;
  // Round 2, Task 4: the module has a registry-declared master switch
  // (module-toggle.ts's moduleToggle() found one) -- `on` is its current
  // state (toggleIsOn() over the same tuner rows the Tuning tab reads).
  // Null/omitted means no toggle renders; installed rows only.
  toggle?: { on: boolean } | null;
}

function isInstalled(m: RowModule, family: ModuleFamily): boolean {
  if (family === "lua") return !!(m.cloned || m.deployed);
  return !!m.installed;
}

export function buildModuleRow(m: RowModule, ctx: RowCtx): ModuleRow {
  const installed = isInstalled(m, ctx.family);
  const actions: RowAction[] = [];
  if (!installed) {
    // Catalog rows keep their own action (Install) in the same column
    // geometry as the installed trio.
    actions.push({ id: "install", label: "Install" });
  } else {
    // A cloned-but-not-deployed lua module still needs Install to deploy it.
    if (ctx.family === "lua" && !m.deployed) {
      actions.push({ id: "install", label: "Install" });
    }
    // Fixed order: tune - repair - remove. sql has no conf, so no tune;
    // the repair surface (update-tracking) exists for cpp modules only.
    // hasTuningTarget (Round 2, Task 3) additionally suppresses tune when
    // ModuleManager.svelte knows there is no real Tuning-tab destination --
    // omitted defaults to true, so every other caller is unaffected.
    if (ctx.family !== "sql" && ctx.hasTuningTarget !== false) {
      actions.push({ id: "tune", label: "Config tuning" });
    }
    if (ctx.family === "cpp") actions.push({ id: "repair", label: "Repair…" });
    actions.push({
      id: "remove",
      label: "Remove",
      ...(ctx.removeDisabled ? { disabled: true } : {}),
    });
    // Round 2, Task 4: the master-switch toggle renders in a fixed slot at
    // the end of the action column (empty on rows without one, so buttons
    // never shift) -- installed rows only, and only when the registry
    // declared a switch for this module.
    if (ctx.toggle) {
      actions.push({ id: "toggle", label: ctx.toggle.on ? "Enabled" : "Disabled" });
    }
  }
  // Round 2, Task 2: the setup chip IS the click target (no more separate
  // badge + text-link pair) and a failed conf-activation chip rides in
  // ready-made from the ctx -- both are real buttons in the Svelte side
  // (RowChip.clickable), never ad-hoc markup.
  const chips: RowChip[] = [];
  if (ctx.needsSetup) {
    chips.push({ id: "setup", kind: "setup", label: "Needs setup", clickable: true });
  }
  if (ctx.confFailChip) {
    chips.push(ctx.confFailChip);
  }
  return {
    key: m.key,
    title: m.name,
    family: ctx.family,
    installed,
    chips,
    actions,
  };
}
