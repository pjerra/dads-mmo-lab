// Grouped sidebar structure (Lab-parity round 1). Future rounds append
// their entries here when the page ships -- never before (no "[soon]" rows).
export const NAV = [
  {
    section: "Server",
    pages: [
      { id: "home", label: "Home" },
      { id: "library", label: "Library" },
      { id: "console", label: "Console" },
      { id: "tools", label: "Tools" },
      { id: "accounts", label: "Accounts" },
      { id: "modmanager", label: "Modules" },
    ],
  },
  {
    section: "Characters",
    pages: [
      { id: "dashboard", label: "Dashboard" },
      { id: "teleport", label: "Teleport" },
      { id: "gmtools", label: "GM Tools" },
    ],
  },
  {
    section: "Items & Bots",
    pages: [
      { id: "items", label: "Item Database" },
      { id: "playerbots", label: "Playerbots" },
      { id: "commands", label: "Commands" },
    ],
  },
  {
    section: "Config",
    pages: [
      { id: "settings", label: "Settings" },
      { id: "botworld", label: "Bot World" },
      { id: "modules", label: "Module Configs" },
      { id: "backups", label: "Backups" },
    ],
  },
] as const;

export type PageId = (typeof NAV)[number]["pages"][number]["id"];

export const DEFAULT_PAGE: PageId = "home";
