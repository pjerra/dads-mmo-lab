// Party wizard light (Batch 5 F5): static role -> class -> spec map for the
// Playerbots page picker, plus per-class pve spec lists for the per-bot
// "Change spec" control. Every spec string here MUST be a member of
// SPEC_ALLOWLIST below, which mirrors the CLI's _valid_bot_spec closed
// allowlist (50-party.sh) -- itself verified against the deployed
// playerbots.conf's AiPlayerbot.PremadeSpecName.* values (2026-07-19).
// A vitest pins the mirror (party-specs.test.ts).
//
// CAVEAT (also in the CLI comment): spec names are conf-driven. If the user
// edits PremadeSpecName.* the allowlist drifts and a mismatch fails
// SILENTLY in-game (whisper reply only, invisible to SOAP).
//
// No DK anywhere -- class 6 is excluded from the party system entirely
// (_valid_bot_class). "bear pvp" / "frostfire pvp" do not exist in the conf.

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

// Mirror of the CLI's _valid_bot_spec allowlist (50-party.sh) -- keep the
// two in sync BY HAND; the vitest only proves this file is self-consistent
// (everything offered by the maps above is in this list).
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
