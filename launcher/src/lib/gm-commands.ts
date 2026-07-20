// Core AzerothCore GM commands -- always available, no module required.
// Extracted from Commands.svelte (Batch 3 F11b) so the Console page's
// autocomplete (improvements Batch 3 F3) can draw from the same catalog the
// cheat-sheet renders. `cmd` is the in-game form (leading dot, <placeholder>
// args); the Console page derives dot-less completable stems from it.
export interface CoreCommand {
  cmd: string;
  what: string;
}

export const CORE_COMMANDS: CoreCommand[] = [
  { cmd: ".tele <place>", what: "Teleport yourself to a named place (e.g. .tele stormwind)." },
  { cmd: ".levelup <n>", what: "Raise your (or your target's) level by n." },
  { cmd: ".additem <id>", what: "Put an item straight into your bags (find ids on the Item Database page)." },
  { cmd: ".modify money <copper>", what: "Give money — 10000 copper = 1 gold." },
  { cmd: ".modify speed <1-10>", what: "Run faster (1 = normal; wears off on relog)." },
  { cmd: ".revive", what: "Bring yourself (or your target) back to life." },
  { cmd: ".summon <name>", what: "Pull a player to you." },
  { cmd: ".appear <name>", what: "Jump to a player." },
  { cmd: ".gm on / .gm off", what: "Turn GM mode on/off (invulnerable + invisible to mobs while on)." },
  { cmd: ".server info", what: "Show server version, uptime and who's online." },
  { cmd: ".saveall", what: "Save every online character right now." },
];
