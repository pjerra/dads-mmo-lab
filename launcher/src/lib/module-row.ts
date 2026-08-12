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
    if (ctx.family !== "sql") actions.push({ id: "tune", label: "Config tuning" });
    if (ctx.family === "cpp") actions.push({ id: "repair", label: "Repair…" });
    actions.push({
      id: "remove",
      label: "Remove",
      ...(ctx.removeDisabled ? { disabled: true } : {}),
    });
  }
  return {
    key: m.key,
    title: m.name,
    family: ctx.family,
    installed,
    // Empty by default -- Task 2 (setup / conf-failed) and later rounds add
    // chips through this builder.
    chips: [],
    actions,
  };
}
