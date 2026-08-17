// Per-module enable/disable toggle (Modules-page round 2, Task 4): pure
// eligibility + current-state reading for the master-switch toggle in the
// Modules-page action column. A module only gets a toggle when the tuning
// registry (the SAME rows the Tuning tab loads via moduleTuningCache)
// declares a master switch for it -- a row whose key ends in `.enable` or
// `.enabled` AND whose module matches. The write itself goes through the
// EXISTING tuner path (api.ts wowConfigTuningSet) -- this module never
// touches the network, so it stays vitest-testable without a harness (same
// rule as module-row.ts).

/** Structural subset of api.ts's ModuleTuning -- exactly what eligibility
 *  and state-reading need, so tests build rows without the full shape. */
export interface TuningRowLike {
  key: string;
  backend: "conf" | "lua";
  module: string;
  value?: string;
  default?: string | null;
}

export interface ToggleSpec {
  settingKey: string;
  backend: "conf" | "lua";
}

// Registry rows carry display names ("NPC Beastmaster", "Learn Spells on
// Level-up") while the Modules page holds keys ("mod-npc-beastmaster",
// "mod-learn-spells") and catalog titles with parenthetical blurbs ("NPC
// Beastmaster (pets for all classes)"). Fold all of them onto one comparable
// form: drop any "(...)" blurb, lowercase, strip every non-alphanumeric.
function norm(s: string): string {
  return s
    .toLowerCase()
    .replace(/\([^)]*\)/g, "")
    .replace(/[^a-z0-9]/g, "");
}

/** The input with a leading "mod" dropped after normalization --
 *  "mod-npc-beastmaster" -> "npcbeastmaster", "mod-learn-spells" ->
 *  "learnspells" -- so cpp keys compare against registry module names and
 *  key prefixes. */
function stripModPrefix(n: string): string {
  return n.startsWith("mod") ? n.slice(3) : n;
}

/**
 * Resolve a Modules-page row (by module key OR display title) to its
 * registry-declared master switch, or null when the row gets no toggle.
 *
 * Rules (pinned by module-toggle.test.ts):
 * - only keys ending in `.enable`/`.enabled` qualify as master switches;
 * - the row's module must match (normalized module name, or the setting-key
 *   prefix for keys like learnspells.enable vs mod-learn-spells);
 * - `mod-playerbots` -> ALWAYS null: the bot engine is the server's core
 *   feature, and its Enable flag must never be one misclick away;
 * - no match -> null (the row simply renders no toggle).
 */
export function moduleToggle(moduleKeyOrTitle: string, registryRows: TuningRowLike[]): ToggleSpec | null {
  const n = norm(moduleKeyOrTitle);
  if (n === "") return null;
  const stripped = stripModPrefix(n);
  if (stripped === "playerbots") return null;
  for (const r of registryRows) {
    const dot = r.key.lastIndexOf(".");
    if (dot < 0) continue;
    const suffix = r.key.slice(dot + 1);
    if (suffix !== "enable" && suffix !== "enabled") continue;
    const mod = norm(r.module);
    const keyPrefix = norm(r.key.slice(0, dot));
    if (mod === n || mod === stripped || keyPrefix === n || keyPrefix === stripped) {
      return { settingKey: r.key, backend: r.backend };
    }
  }
  return null;
}

/**
 * Current on/off state, read off the same tuner rows the Tuning tab shows.
 * A live value wins; an empty one (conf not created yet -- the registry read
 * reports "" then) falls back to the row's shipped default; a missing row
 * reads as off.
 */
export function toggleIsOn(spec: ToggleSpec, registryRows: TuningRowLike[]): boolean {
  const row = registryRows.find((r) => r.key === spec.settingKey);
  if (!row) return false;
  const v = row.value !== undefined && row.value !== "" ? row.value : (row.default ?? "");
  return v === "1";
}
