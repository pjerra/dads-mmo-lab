// Setup catalog (Modules-page round, Task 5): a small, hand-maintained data
// module naming the installed modules that need a manual step or two before
// they actually do anything for a player (AHBot needs an auction-house
// character, several NPC mods need their NPC placed, ARAC needs its client
// patch applied). Kept as launcher-side data rather than piped through
// module-catalog.json + both bash/Rust list arms (recorded deviation #1 in
// the plan header) -- this is pure UI metadata for a launcher-only feature,
// not something the CLI needs to know about.
//
// Each action wraps machinery that ALREADY exists on the Modules page --
// open-tuner/open-files reuse Task 4's cross-tab nav store, place-npc/fixit
// reuse the page's existing wowModulePlaceNpc/wowModuleFixit calls, and
// copy-command is a clipboard write. No new mutation surfaces.

export type SetupAction =
  | { type: "open-tuner"; key: string; label: string }
  | { type: "open-files"; file: string; label: string }
  | { type: "place-npc"; key: string; label: string }
  | { type: "fixit"; fix: "battlepass-npc"; label: string }
  | { type: "copy-command"; command: string; label: string };

export interface ModuleSetup {
  summary: string;
  steps: string[];
  actions: SetupAction[];
}

export const SETUP_CATALOG: Record<string, ModuleSetup> = {
  "mod-ahbot": {
    summary: "AHBot only auctions once it has an auction-house character.",
    steps: [
      "Create a dedicated account and character for it (or pick an existing one).",
      "Set AuctionHouseBot.Account / AuctionHouseBot.GUID in mod_ahbot.conf.",
      "Restart the world server.",
    ],
    actions: [
      { type: "copy-command", command: "account create ahbot <password>", label: "Copy account create command" },
      { type: "open-tuner", key: "mod-ahbot", label: "Open tuning" },
      { type: "open-files", file: "mod_ahbot.conf", label: "Open mod_ahbot.conf" },
    ],
  },
  battlepass: {
    summary: "Battle Pass's SQL doesn't ship its vendor NPC -- place it once.",
    steps: [
      "Place the Battle Pass vendor NPC in Stormwind and Orgrimmar.",
      "Restart the world server for it to appear.",
    ],
    actions: [{ type: "fixit", fix: "battlepass-npc", label: "Place NPC" }],
  },
  bmah: {
    summary: "BMAH needs its Auctioneer NPC placed before players can use it.",
    steps: [
      "Place the BMAH Auctioneer in Stormwind and Orgrimmar.",
      "Restart the world server for it to appear.",
    ],
    actions: [{ type: "place-npc", key: "bmah", label: "Place NPC in capitals" }],
  },
  "mod-1v1-arena": {
    summary: "The 1v1 arena queue NPC needs to be placed before players can queue.",
    steps: [
      "Place the 1v1 arena queue NPC in Stormwind and Orgrimmar.",
      "Restart the world server for it to appear.",
    ],
    actions: [{ type: "place-npc", key: "mod-1v1-arena", label: "Place NPC in capitals" }],
  },
  "mod-npc-beastmaster": {
    summary: "The Beastmaster NPC needs to be placed before players can use it.",
    steps: [
      "Place the Beastmaster in Stormwind and Orgrimmar.",
      "Restart the world server for it to appear.",
    ],
    actions: [{ type: "place-npc", key: "mod-npc-beastmaster", label: "Place NPC in capitals" }],
  },
  "mod-transmog": {
    summary: "The Transmogrifier NPC needs to be placed before players can use it.",
    steps: [
      "Place the Transmogrifier in Stormwind and Orgrimmar.",
      "Restart the world server for it to appear.",
    ],
    actions: [{ type: "place-npc", key: "mod-transmog", label: "Place NPC in capitals" }],
  },
  "mod-arac": {
    summary: "ARAC needs its client patch applied before the extra races/classes show up.",
    steps: [
      "Apply the client patch using this row's own \"Apply client patch\" button.",
      "Restart the world server for it to take effect.",
    ],
    // Deliberately no open-files action here (per plan Task 5): ARAC's
    // client patch is applied via the row's own button, not a conf file --
    // wiring open-files at it would be wrong.
    actions: [],
  },
};

export function setupFor(key: string): ModuleSetup | null {
  return SETUP_CATALOG[key] ?? null;
}

// v1 dismissal key: no page currently exposes a real server identity (see
// ModuleManager.svelte's SETUP_SERVER constant), so callers pass a literal
// backend string today. The function still takes serverDir as a real
// parameter so a later round can pass the actual identity without changing
// this signature or invalidating existing dismissals' shape.
export function setupDoneKey(serverDir: string, moduleKey: string): string {
  return `dml.setupDone.${serverDir}.${moduleKey}`;
}
