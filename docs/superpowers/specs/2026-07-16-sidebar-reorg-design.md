# Sidebar Reorg + Home Page — Design

**Date:** 2026-07-16
**Branch:** `feat/dml-launcher-windows` (stays here; no merge until asked)
**Round:** 1 of the Lab-parity roadmap (sidebar reorg → GM tools → summon NPCs → My Party phase 2 → backup/restore)

## Goal

Restructure the launcher's flat sidebar into Lab-style grouped sections, add a **Home** landing page (server status + start/stop), and split the Config page into two sidebar entries: **Settings** (curated) and **Modules** (raw conf editor). This is the frame the later rounds' pages slot into. Launcher-only — no CLI, Lua, or Rust changes.

## Sidebar structure

Grouped sections with small uppercase headers, replacing the flat list in
`launcher/src/routes/+page.svelte`:

| Section | Entries (this round) | Reserved for later rounds |
|---|---|---|
| SERVER | Home, Library | — |
| CHARACTERS | Dashboard, Teleport | GM Tools (round 2), Summon (round 3) |
| ITEMS & BOTS | Item Database, Playerbots | — |
| CONFIG | Settings, Modules | Backups (round 5) |

Rules:

- Reserved entries are **not rendered** — no greyed-out "[soon]" items. Each later round adds its entry when its page ships.
- **Home is the default page** (`page = $state("home")` replaces `"library"`).
- The `PAGES` flat array becomes a `NAV` structure of `{ section, pages: [{ id, label }] }`. `PageId` derives from it; the `{#if}` page-render chain pattern stays.
- Section headers are non-interactive labels (uppercase, small, muted `#8b949e`-family color), matching the existing dark sidebar style. Active-entry styling (blue left border, `#161b22` background) is unchanged.
- Sidebar width stays 200px; existing shell grid unchanged.

## Home page

New file: `launcher/src/lib/pages/Home.svelte`. Composes two existing patterns — nothing new backend-side.

**Status card** (Dashboard's pattern): `onMount` → `wowServerInfo()` from `$lib/api`; green/grey dot + "World is up/down"; when up show players/uptime/update-time. Errors render in the standard `.error-card`. A Refresh button re-fetches. (Deliberate overlap: Dashboard keeps its own status card — cheap, and each page stands alone.)

**Start/Stop** (Library's `act()` pattern): one card for the WoW server (`const WOW_ID = "wow-server-playerbots"`, same literal Config uses). `gamesStatus(WOW_ID)` on mount decides which button shows: `state === "running"` → Stop, else Start (primary/green). Clicking streams `gamesStart`/`gamesStop(WOW_ID, onEvent)` into the shared `Terminal` component via `terminal-state`'s `applyEvent`, exactly like Library; buttons disabled while a stream is busy; on settle, re-fetch `gamesStatus` **and** `wowServerInfo` so the card and buttons agree.

Container state (`gamesStatus`) drives the buttons; world state (`wowServerInfo.online`) drives the card. They can briefly disagree during boot (container running, world still loading) — that is fine and honest; no reconciliation logic.

Copy stays plain-language ("World is up", "Start the server"). No install flows, no per-game list (that remains Library's job).

## Config split: Settings + Modules

`launcher/src/lib/pages/Config.svelte` currently has an in-page tab bar (`tab: "settings" | "files"`). The sidebar takes over that job:

- **Two sidebar entries, one mount.** The shell renders Config when `page === "settings" || page === "modules"` and passes `tab={page === "settings" ? "settings" : "files"}`. Because the component stays mounted while hopping between the two entries, unsaved settings edits and loaded file content survive the hop — identical to today's in-page tab behavior. Navigating to any *other* page unmounts it (existing behavior for all pages).
- **Inside Config:** local `tab` state becomes a prop (`let { tab }: { tab: "settings" | "files" } = $props()`). The in-page tab bar is removed. The `<h2>` header shows "Settings" or "Modules" to match the sidebar. `setTab()`'s job — resetting `confirmingRestart` on tab change — moves to an `$effect` watching the prop.
- **Restart banner is shared:** the "Saved — restart to apply" banner already reads the module store `restartState.needed`, so it shows on both entries regardless of where the save happened. Unchanged.
- **Save/Save & Restart logic, `onFileSelect` stale-write guard, CharPicker row:** all unchanged.

### Modules page honesty (read-only files)

The CLI's raw-write already rejects `.env` and `docker-compose.override.yml` (host-RCE lock from the launcher-pages round). The Modules UI currently lets you edit them and only fails at Save. Make the UI tell the truth:

- All five files stay openable (viewing `.env` is useful).
- For `.env` and `docker-compose.override.yml`: textarea `readonly`, Save / Save & Restart buttons hidden, and a muted note: "Read-only — locked so a bad edit can't run commands on your PC."
- The writable three remain `playerbots.conf`, `mod_ahbot.conf`, `mod_ale.conf` — which is why the page is honestly named **Modules**.
- Implemented as a UI-side constant (e.g. `READONLY_FILES`) mirroring the CLI lock; the CLI remains the enforcement point.

## Files

- **Create:** `launcher/src/lib/pages/Home.svelte`
- **Modify:** `launcher/src/routes/+page.svelte` (NAV groups, section headers, Home default, single Config mount with `tab` prop), `launcher/src/lib/pages/Config.svelte` (tab prop, drop tab bar, dynamic header, read-only file handling)
- **Unchanged:** all other pages, `$lib/api.ts`, Rust shell, CLI, Lua

## Error handling

Nothing new: Home reuses the established error shapes (`.error-card` for fetch failures, stream `error` events rendered by `Terminal`). A `wowServerInfo` failure (e.g. Docker down) must not hide the Start button — status card shows the error, start/stop card still renders from `gamesStatus` (and if that also fails, its error shows in place of the buttons).

## Testing & gates

- `svelte-check` 0 errors / 0 warnings; `vitest` suite stays green; release build (`cargo tauri build` path) compiles.
- Unit test for the NAV structure helper if extracted as a pure function (id uniqueness, Home first); no component-test infra exists — do not introduce it this round.
- **User click-through gate:** Home is the landing page and its status card is truthful; Start/Stop streams into the terminal and settles to the right button; every sidebar entry navigates; Settings and Modules each show their half of old Config; a save on Settings shows the restart banner on Modules too; `.env` opens read-only with no Save button.

## Out of scope

GM Tools, Summon, Backups pages and My Party phase 2 (rounds 2–5). Any CLI/Rust change. Restyling existing page content.
