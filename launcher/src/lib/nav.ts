// Grouped sidebar structure. The sidebar renders each multi-item section as a
// collapsible dropdown (accordion) in routes/+page.svelte; single-item
// sections render as a plain top-level link. Pages that used to be in-page
// tabs (the Config views, the two Bots views) are promoted to sidebar
// children here -- their page id doubles as the `view` prop passed to the
// shared Config/Bots component, so the sidebar drives the view (no in-page
// tab bar). The character view (Dashboard) keeps its OWN internal tabs.
export const NAV = [
  {
    section: "Server",
    pages: [
      { id: "home", label: "Home" },
      { id: "library", label: "Library" },
      { id: "console", label: "Console" },
      { id: "tools", label: "Tools" },
      { id: "accounts", label: "Accounts" },
      { id: "statistics", label: "Statistics" },
      { id: "modmanager", label: "Modules" },
    ],
  },
  {
    section: "Characters",
    pages: [
      { id: "dashboard", label: "Character" },
      { id: "teleport", label: "Teleport" },
      { id: "gmtools", label: "GM Tools" },
    ],
  },
  {
    section: "Items & Bots",
    pages: [
      { id: "items", label: "Item Database" },
      // Promoted from the old Bots page tabs -> Bots view="party" / "browse".
      { id: "party", label: "My Party" },
      { id: "browse", label: "Browse Bots" },
      { id: "commands", label: "Commands" },
    ],
  },
  {
    section: "Config",
    // Promoted from the old Config page tabs -> Config view=<id>. Backups is
    // its own page (Backups component). The old moduletuning/files entries
    // moved into the Modules page as in-page tabs (module-update round).
    pages: [
      { id: "settings", label: "Settings" },
      { id: "botworld", label: "Bot World" },
      { id: "ahbot", label: "Auction House" },
      { id: "accountwide", label: "Account-wide" },
      { id: "backups", label: "Backups" },
    ],
  },
  {
    section: "Help",
    pages: [{ id: "help", label: "Help & FAQ" }],
  },
] as const;

export type PageId = (typeof NAV)[number]["pages"][number]["id"];

// Page ids that route to the shared Config component (their id == the Config
// view). Kept here so the router and any nav logic share one source.
export const CONFIG_VIEWS = ["settings", "botworld", "ahbot", "accountwide"] as const;
export const BOTS_VIEWS = ["party", "browse"] as const;

export const DEFAULT_PAGE: PageId = "home";
