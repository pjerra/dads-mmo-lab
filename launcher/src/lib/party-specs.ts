// Party wizard light (Batch 5 F5): role -> class -> spec map for the
// Playerbots page picker, plus per-class pve spec lists for the per-bot
// "Change spec" control.
//
// SPEC SOURCE OF TRUTH (Batch 5 F5 follow-up): the ACTUAL spec options and the
// CLI validation both come from the deployed playerbots.conf at runtime --
// `wow party specs` parses it, buildSpecIndex() below turns that into the
// picker's option lists, and the same conf drives the CLI's _valid_bot_spec.
// So the picker can no longer offer a spec the validator would reject --
// membership is shared by construction, and buildSpecIndex applies the same
// charset guard (isValidSpecShape) the CLI applies. The
// static ROLE_MAP / PVE_SPECS_BY_CLASS_ID / SPEC_ALLOWLIST below are the
// ROLE grouping (which class fills which role -- a UI concept absent from the
// conf) and the OFFLINE FALLBACK used only when the live conf isn't readable
// (server not installed / dev). SPEC_ALLOWLIST still mirrors the shipped
// defaults (verified 2026-07-19); a vitest pins its self-consistency.
//
// No DK anywhere -- class 6 is excluded from the party system entirely
// (_valid_bot_class). "bear pvp" / "frostfire pvp" do not exist in the conf.

// The CLI's spec-name charset guard, mirrored (bash `_valid_bot_spec` in
// cli/src/50-party.sh, Rust `valid_bot_spec_shape` in crates/dml-wow/src/
// party.rs -- all three must agree). Wide enough for anything a hand-written
// conf realistically carries (mixed case, digits, . _ -), narrow enough that
// the name is safe in the `dml_whisper <p> <b> talents spec <name>` tail: no
// quotes, no backslash, no CR/LF, no shell/SQL metacharacters.
const SPEC_NAME_RE = /^[A-Za-z0-9][A-Za-z0-9 ._-]*$/;
export function isValidSpecShape(name: string): boolean {
  return SPEC_NAME_RE.test(name);
}

// A live premade spec parsed from the deployed playerbots.conf. Defined here
// (the dependency-free data module) so api.ts and the picker share one shape.
export interface LiveSpec {
  class_id: number;
  class: string;
  specno: number;
  name: string;
  link: string | null;
  tree: string | null;
}

// The picker's live index: spec lists keyed by class NAME (the add-picker
// works in class names) and by class ID (the per-bot Change-spec works in
// characters.class ids). Each value keeps the full LiveSpec so the preview can
// read tree/link. Names within a class are unique in the conf.
export interface SpecIndex {
  byName: Record<string, LiveSpec[]>;
  byId: Record<number, LiveSpec[]>;
}
export function buildSpecIndex(specs: LiveSpec[]): SpecIndex {
  const byName: Record<string, LiveSpec[]> = {};
  const byId: Record<number, LiveSpec[]> = {};
  for (const s of specs) {
    // A conf name the CLI's charset guard would refuse can only ever be a dead
    // option ("Unknown spec" on click), so it is dropped here rather than
    // offered. This is the ONLY name-based filter -- everything the validator
    // accepts reaches the picker verbatim.
    if (!isValidSpecShape(s.name)) continue;
    (byName[s.class] ??= []).push(s);
    (byId[s.class_id] ??= []).push(s);
  }
  return { byName, byId };
}

export type Role = "Tank" | "Healer" | "Melee" | "Ranged";

export interface RolePick {
  class: string; // CLI class name (party add --class)
  classId: number; // characters.class id (for display via className())
  spec: string; // premade pve spec name (party add --spec)
}

export const ROLES: Role[] = ["Tank", "Healer", "Melee", "Ranged"];

export const ROLE_MAP: Record<Role, RolePick[]> = {
  Tank: [
    { class: "warrior", classId: 1, spec: "prot pve" },
    { class: "paladin", classId: 2, spec: "prot pve" },
    { class: "druid", classId: 11, spec: "bear pve" },
  ],
  Healer: [
    { class: "priest", classId: 5, spec: "holy pve" },
    { class: "paladin", classId: 2, spec: "holy pve" },
    { class: "shaman", classId: 7, spec: "resto pve" },
    { class: "druid", classId: 11, spec: "resto pve" },
  ],
  Melee: [
    { class: "rogue", classId: 4, spec: "combat pve" },
    { class: "warrior", classId: 1, spec: "fury pve" },
    { class: "paladin", classId: 2, spec: "ret pve" },
    { class: "shaman", classId: 7, spec: "enh pve" },
    { class: "druid", classId: 11, spec: "cat pve" },
  ],
  Ranged: [
    { class: "hunter", classId: 3, spec: "bm pve" },
    { class: "mage", classId: 8, spec: "frost pve" },
    { class: "warlock", classId: 9, spec: "affli pve" },
    { class: "priest", classId: 5, spec: "shadow pve" },
    { class: "shaman", classId: 7, spec: "ele pve" },
    { class: "druid", classId: 11, spec: "balance pve" },
  ],
};

// Per-class pve specs (MVP: pve only) for the per-bot "Change spec" select,
// keyed by characters.class id.
export const PVE_SPECS_BY_CLASS_ID: Record<number, string[]> = {
  1: ["arms pve", "fury pve", "prot pve"],
  2: ["holy pve", "prot pve", "ret pve"],
  3: ["bm pve", "mm pve", "surv pve"],
  4: ["as pve", "combat pve", "subtlety pve"],
  5: ["disc pve", "holy pve", "shadow pve"],
  7: ["ele pve", "enh pve", "resto pve"],
  8: ["arcane pve", "fire pve", "frost pve", "frostfire pve"],
  9: ["affli pve", "demo pve", "destro pve"],
  11: ["balance pve", "bear pve", "resto pve", "cat pve"],
};

// OFFLINE FALLBACK allowlist -- mirrors the shipped playerbots.conf defaults
// (the same static fallback the CLI's _valid_bot_spec uses when no conf is
// deployed). At runtime the live conf (`wow party specs`) is the source of
// truth for BOTH sides, so this list is only a self-consistency net for the
// static ROLE_MAP / PVE maps above (a vitest pins it).
export const SPEC_ALLOWLIST: string[] = [
  "arms pve", "arms pvp", "fury pve", "fury pvp", "prot pve", "prot pvp",
  "holy pve", "holy pvp", "ret pve", "ret pvp",
  "bm pve", "bm pvp", "mm pve", "mm pvp", "surv pve", "surv pvp",
  "as pve", "as pvp", "combat pve", "combat pvp", "subtlety pve", "subtlety pvp",
  "disc pve", "disc pvp", "shadow pve", "shadow pvp",
  "ele pve", "ele pvp", "enh pve", "enh pvp", "resto pve", "resto pvp",
  "arcane pve", "arcane pvp", "fire pve", "fire pvp", "frost pve", "frost pvp", "frostfire pve",
  "affli pve", "affli pvp", "demo pve", "demo pvp", "destro pve", "destro pvp",
  "balance pve", "balance pvp", "bear pve", "cat pve", "cat pvp",
];

// The CLI class-name set (must match _valid_bot_class; no deathknight).
export const VALID_BOT_CLASSES = [
  "warrior", "paladin", "hunter", "rogue", "priest", "shaman", "mage", "warlock", "druid",
] as const;
