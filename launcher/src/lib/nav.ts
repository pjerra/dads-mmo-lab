// Grouped sidebar structure (Lab-parity round 1). Future rounds append
// their entries here when the page ships -- never before (no "[soon]" rows).
export const NAV = [
  {
    section: "Server",
    pages: [
      { id: "home", label: "Home" },
      { id: "library", label: "Library" },
    ],
  },
  {
    section: "Characters",
    pages: [
      { id: "dashboard", label: "Dashboard" },
      { id: "teleport", label: "Teleport" },
    ],
  },
  {
    section: "Items & Bots",
    pages: [
      { id: "items", label: "Item Database" },
      { id: "playerbots", label: "Playerbots" },
    ],
  },
  {
    section: "Config",
    pages: [
      { id: "settings", label: "Settings" },
      { id: "modules", label: "Modules" },
    ],
  },
] as const;

export type PageId = (typeof NAV)[number]["pages"][number]["id"];

export const DEFAULT_PAGE: PageId = "home";
