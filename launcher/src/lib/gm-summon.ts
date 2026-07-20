// Pure classification for the GM Tools "Summon an NPC" errors. Kept out of the
// Svelte component so the tricky NOT_FOUND disambiguation is unit-testable.
//
// Summon targets that only exist once a specific module is installed. When a
// summon of one of these fails because its creature_template row is absent,
// name the module + point at the Modules page instead of the raw
// "No creature with entry N" CLI error. Entries: Casino (mod-gasino-casino),
// Transmogrifier 190010 (mod-transmog), Beastmaster 601026
// (mod-npc-beastmaster), Black Market 2069430 (bmah).
export const MODULE_NPCS: Record<number, { npc: string; module: string }> = {
  990000: { npc: "Casino", module: "the Casino module (mod-gasino-casino)" },
  190010: { npc: "Transmogrifier", module: "the Transmogrification module (mod-transmog)" },
  601026: { npc: "Beastmaster", module: "the NPC Beastmaster module (mod-npc-beastmaster)" },
  2069430: { npc: "Black Market Auctioneer", module: "the Black Market Auction House module (bmah)" },
};

export interface SummonError {
  code?: string;
  message?: string;
}

// Returns the "module isn't installed" guidance ONLY when the summon failed
// because a module NPC's creature_template row is genuinely missing. Returns
// null for every other failure so the caller shows the raw error.
//
// The CLI returns code NOT_FOUND for TWO distinct cases: a missing creature
// ("No creature with entry N") and an offline character ("Character not
// online", from the online guard that runs AFTER the creature check). Keying
// only on the code conflated them, so a summon that failed just because the
// character logged out (a reachable state -- the page's `online` list goes
// stale) wrongly told the user to reinstall a module they already have. Match
// the creature-existence message text to tell the two apart.
export function summonModuleHint(entry: number, err: SummonError): string | null {
  const mod = MODULE_NPCS[entry];
  if (!mod) return null;
  if (err.code !== "NOT_FOUND") return null;
  if (!(err.message ?? "").includes("No creature with entry")) return null;
  return `No ${mod.npc} NPC exists yet (entry ${entry}). It comes from ${mod.module} — install it on the Modules page, restart the server, then try again.`;
}
