# Sidebar Reorg + Home Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure the launcher's flat sidebar into Lab-style grouped sections, add a Home landing page (server status + start/stop), and split the Config page into two sidebar entries (Settings, Modules) sharing one component mount.

**Architecture:** Launcher-only change. A new pure data module `nav.ts` defines the grouped sidebar; the SvelteKit shell (`+page.svelte`) renders it and keeps its `{#if}` page chain. A new `Home.svelte` composes two existing patterns (Dashboard's status card, Library's start/stop streaming). `Config.svelte` swaps its internal tab state for a `tab` prop and gains honest read-only handling for the two security-locked files.

**Tech Stack:** Svelte 5 (runes) + SvelteKit + TypeScript; vitest for the pure module; svelte-check as the compile gate. No Rust, CLI, or Lua changes.

**Spec:** `docs/superpowers/specs/2026-07-16-sidebar-reorg-design.md`

## Global Constraints

- Branch `feat/dml-launcher-windows`. Never merge; never push unless asked.
- **Launcher-only:** do not modify anything under `cli/`, `launcher/src-tauri/`, or `cli/lua/`. The CLI stays the enforcement point for the raw-write lock.
- `launcher/src-tauri/Cargo.toml` may show as modified (pre-existing EOL-only ghost change) — NEVER stage, commit, or revert it.
- All npm commands run from `launcher/` (`cd launcher` first). PowerShell 5.1 has no `&&` — run commands separately or use git-bash.
- Sidebar groups and entries, exactly (spec table): **Server**: Home, Library · **Characters**: Dashboard, Teleport · **Items & Bots**: Item Database, Playerbots · **Config**: Settings, Modules. Reserved future entries (GM Tools, Summon, Backups) are NOT rendered — no greyed-out or "[soon]" items.
- **Home is the default page** (replaces `library`).
- The WoW game id literal is `"wow-server-playerbots"` (same as Config.svelte's `WOW_ID`).
- Read-only files in the Modules view, exactly: `.env`, `docker-compose.override.yml`. Writable: `playerbots.conf`, `mod_ahbot.conf`, `mod_ale.conf`.
- Settings and Modules are ONE `Config.svelte` mount driven by a `tab` prop — hopping between them must preserve unsaved edits (spec: "identical to today's in-page tab behavior").
- Copy stays plain-language; user-facing strings are given verbatim in the tasks — use them exactly.
- Gates for every task: `npm run check` → 0 errors, 0 warnings; `npm test` → all pass.
- No new npm dependencies. No component-test infrastructure (none exists; do not introduce it).

---

### Task 1: `nav.ts` — grouped sidebar data module

**Files:**
- Create: `launcher/src/lib/nav.ts`
- Test: `launcher/src/lib/nav.test.ts`

**Interfaces:**
- Consumes: nothing.
- Produces (Task 4 imports these from `$lib/nav`):
  - `NAV`: readonly array of `{ section: string; pages: readonly { id, label }[] }`
  - `type PageId` — literal union of all page ids (`"home" | "library" | "dashboard" | "teleport" | "items" | "playerbots" | "settings" | "modules"`)
  - `DEFAULT_PAGE: PageId` — `"home"`

- [ ] **Step 1: Write the failing test**

Create `launcher/src/lib/nav.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { NAV, DEFAULT_PAGE } from "./nav";

describe("NAV", () => {
  const ids = NAV.flatMap((s) => s.pages.map((p) => p.id));

  it("has exactly the spec's pages, in order", () => {
    expect(ids).toEqual([
      "home",
      "library",
      "dashboard",
      "teleport",
      "items",
      "playerbots",
      "settings",
      "modules",
    ]);
  });

  it("page ids are unique", () => {
    expect(new Set(ids).size).toBe(ids.length);
  });

  it("has exactly the spec's sections, in order", () => {
    expect(NAV.map((s) => s.section)).toEqual([
      "Server",
      "Characters",
      "Items & Bots",
      "Config",
    ]);
  });

  it("default page is home and exists in NAV", () => {
    expect(DEFAULT_PAGE).toBe("home");
    expect(ids).toContain(DEFAULT_PAGE);
  });

  it("every entry has a non-empty label", () => {
    for (const s of NAV) {
      for (const p of s.pages) {
        expect(p.label.length).toBeGreaterThan(0);
      }
    }
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

From `launcher/`:

Run: `npx vitest run src/lib/nav.test.ts`
Expected: FAIL — cannot resolve `./nav` (module does not exist yet).

- [ ] **Step 3: Write the implementation**

Create `launcher/src/lib/nav.ts`:

```ts
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
```

- [ ] **Step 4: Run tests to verify they pass**

From `launcher/`:

Run: `npx vitest run src/lib/nav.test.ts`
Expected: PASS (5 tests).

Run: `npm test` and `npm run check`
Expected: all vitest suites pass; svelte-check 0 errors, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/nav.ts launcher/src/lib/nav.test.ts
git commit -m "feat(launcher): grouped sidebar nav data module"
```

---

### Task 2: `Home.svelte` — landing page (status card + start/stop)

**Files:**
- Create: `launcher/src/lib/pages/Home.svelte`

**Interfaces:**
- Consumes (all existing, from `$lib/api`): `wowServerInfo(): Promise<ServerInfo>`; `gamesStatus(id: string): Promise<{ id: string; state: "running" | "stopped" }>`; `gamesStart(id: string, onEvent: (e: TermEvent) => void): Promise<void>`; `gamesStop` (same signature); types `ServerInfo`, `TermEvent`. From `$lib/terminal-state`: `applyEvent`, `initialTermState`, `TermState`. Component `$lib/Terminal.svelte` (prop `state: TermState`).
- Produces: default component export, **no props** (Task 4 mounts `<Home />`).

Design rules (from spec): container state (`gamesStatus`) drives the Start/Stop buttons; world state (`wowServerInfo`) drives the status card; they may briefly disagree during boot — no reconciliation logic. A `wowServerInfo` failure must NOT hide the Start button; a `gamesStatus` failure shows its error in place of the buttons. This page is not yet reachable from the sidebar — that lands in Task 4; the gate here is compile + tests.

- [ ] **Step 1: Write the component**

Create `launcher/src/lib/pages/Home.svelte`:

```svelte
<script lang="ts">
  import { onMount } from "svelte";
  import { wowServerInfo, gamesStatus, gamesStart, gamesStop, type ServerInfo } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";

  const WOW_ID = "wow-server-playerbots";

  let info: ServerInfo | null = $state(null);
  let infoError: string | null = $state(null);
  let containerState: "running" | "stopped" | null = $state(null);
  let statusError: string | null = $state(null);
  let refreshing = $state(false);

  let busy = $state(false);
  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  async function refresh() {
    refreshing = true;
    try {
      containerState = (await gamesStatus(WOW_ID)).state;
      statusError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      statusError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      containerState = null;
    }
    try {
      info = await wowServerInfo();
      infoError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      infoError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      info = null;
    } finally {
      refreshing = false;
    }
  }
  onMount(refresh);

  async function act(action: "start" | "stop") {
    busy = true;
    showTerm = true;
    term = initialTermState();
    try {
      const run = action === "start" ? gamesStart : gamesStop;
      await run(WOW_ID, (e) => {
        term = applyEvent(term, e);
      });
    } catch (e) {
      const err = e as { code?: string; message?: string; hint?: string };
      term = applyEvent(term, {
        event: "error",
        error: {
          code: err.code ?? "IPC",
          message: err.message ?? String(e),
          hint: err.hint ?? "",
        },
      });
    } finally {
      busy = false;
      await refresh();
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Home</h2>
    <button onclick={refresh} disabled={refreshing || busy}>Refresh</button>
  </header>

  {#if infoError}
    <div class="error-card"><strong>Couldn't read world status.</strong><p>{infoError}</p></div>
  {:else if info}
    <div class="card">
      <div class="card-title">
        <span class="dot {info.online ? 'on' : 'off'}"></span>
        <strong>{info.online ? "World is up" : "World is down"}</strong>
      </div>
      {#if info.online}
        <div class="stats">
          <span>Players online: <strong>{info.players ?? "?"}</strong></span>
          <span>Uptime: <strong>{info.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{info.mean_ms ?? "?"} ms avg</strong></span>
        </div>
      {:else}
        <p class="muted">Start the server below.</p>
      {/if}
    </div>
  {/if}

  <header class="bar"><h2>WoW server</h2></header>
  {#if statusError}
    <div class="error-card"><strong>Couldn't reach the DML backend.</strong><p>{statusError}</p></div>
  {:else if containerState}
    <div class="card">
      <div class="card-title">
        <span class="dot {containerState === 'running' ? 'on' : 'off'}"></span>
        {WOW_ID}
      </div>
      <div>
        {#if containerState === "running"}
          <button disabled={busy} onclick={() => act("stop")}>Stop</button>
        {:else}
          <button class="primary" disabled={busy} onclick={() => act("start")}>Start</button>
        {/if}
      </div>
    </div>
  {/if}

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .stats { display: flex; gap: 18px; flex-wrap: wrap; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
```

- [ ] **Step 2: Run the gates**

From `launcher/`:

Run: `npm run check`
Expected: 0 errors, 0 warnings. (A new unreferenced component is still type-checked by svelte-check.)

Run: `npm test`
Expected: all suites pass (unchanged).

- [ ] **Step 3: Commit**

```bash
git add launcher/src/lib/pages/Home.svelte
git commit -m "feat(launcher): Home landing page (world status + start/stop)"
```

---

### Task 3: Config.svelte — `tab` prop, per-page headers, read-only locked files

**Files:**
- Modify: `launcher/src/lib/pages/Config.svelte`

**Interfaces:**
- Produces (Task 4 relies on this): Config accepts an optional prop `tab?: "settings" | "files"`, default `"settings"`. All other behavior (save flows, restart banner via `restartState`, `onFileSelect` stale-write guard, CharPicker) is unchanged.
- Consumes: nothing new.

Current state: the component owns `let tab: "settings" | "files" = $state("settings")`, a `setTab()` helper, and renders an in-page tab bar (`.tabs` buttons "Settings" / "Files (Advanced)") next to an `<h2>Config</h2>` header.

- [ ] **Step 1: Replace tab state with a prop**

In the `<script>` block, replace:

```ts
let tab: "settings" | "files" = $state("settings");
```

with:

```ts
let { tab = "settings" }: { tab?: "settings" | "files" } = $props();
```

Delete the whole `setTab` function:

```ts
function setTab(t: "settings" | "files") {
  tab = t;
  confirmingRestart = false;
}
```

and replace its job (reset the armed restart confirmation when the visible tab changes) with an effect. Add this AFTER the `let confirmingRestart = $state(false);` declaration:

```ts
// Switching between the Settings and Modules sidebar entries changes `tab`
// without remounting -- an armed "sure?" confirmation must not survive that.
$effect(() => {
  void tab;
  confirmingRestart = false;
});
```

- [ ] **Step 2: Add the read-only file lock (UI side)**

After the existing `FILES` constant, add:

```ts
// UI mirror of the CLI's raw-write lock (cli rejects these two names).
const READONLY_FILES: RawFileName[] = [".env", "docker-compose.override.yml"];
```

After the `const dirty = $derived(...)` line, add:

```ts
const fileReadonly = $derived(READONLY_FILES.includes(file));
```

- [ ] **Step 3: Replace the header + tab bar in the template**

Replace:

```svelte
<header class="bar">
  <h2>Config</h2>
  <div class="tabs">
    <button class:active={tab === "settings"} onclick={() => setTab("settings")}>Settings</button>
    <button class:active={tab === "files"} onclick={() => setTab("files")}>Files (Advanced)</button>
  </div>
</header>
```

with:

```svelte
<header class="bar">
  <h2>{tab === "settings" ? "Settings" : "Modules"}</h2>
</header>
```

Also delete the now-unused styles from the `<style>` block:

```css
.tabs button { background: none; border: 1px solid #30363d; color: #8b949e; border-radius: 6px 6px 0 0; padding: 6px 14px; cursor: pointer; }
.tabs button.active { color: #f0f6fc; background: #161b22; }
```

- [ ] **Step 4: Make locked files honest in the files view**

In the `{:else}` (files) branch of the template, replace the `{#if fileLoaded}` block:

```svelte
{#if fileLoaded}
  <textarea
    rows="18"
    spellcheck="false"
    bind:value={fileContent}
    oninput={() => (confirmingRestart = false)}
    disabled={saving || restartState.restarting}
  ></textarea>
  {#if lastBackup}<p class="muted">Previous version kept as {lastBackup}</p>{/if}
  <div class="row">
    <button class="primary" onclick={saveFile} disabled={saving || restartState.restarting}>Save</button>
    <button onclick={() => saveAndRestart(saveFile)} disabled={saving || restartState.restarting}>
      {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
    </button>
  </div>
{/if}
```

with:

```svelte
{#if fileLoaded}
  <textarea
    rows="18"
    spellcheck="false"
    bind:value={fileContent}
    oninput={() => (confirmingRestart = false)}
    readonly={fileReadonly}
    disabled={saving || restartState.restarting}
  ></textarea>
  {#if fileReadonly}
    <p class="muted">Read-only — locked so a bad edit can't run commands on your PC. Change these via the Settings page.</p>
  {:else}
    {#if lastBackup}<p class="muted">Previous version kept as {lastBackup}</p>{/if}
    <div class="row">
      <button class="primary" onclick={saveFile} disabled={saving || restartState.restarting}>Save</button>
      <button onclick={() => saveAndRestart(saveFile)} disabled={saving || restartState.restarting}>
        {confirmingRestart ? "This disconnects players — sure?" : "Save & Restart"}
      </button>
    </div>
  {/if}
{/if}
```

- [ ] **Step 5: Run the gates**

From `launcher/`:

Run: `npm run check`
Expected: 0 errors, 0 warnings. The old shell still renders `<Config />` with no prop — the default keeps it compiling, and until Task 4 lands only the Settings view is reachable from the UI. That transient state is expected mid-branch.

Run: `npm test`
Expected: all suites pass.

- [ ] **Step 6: Commit**

```bash
git add launcher/src/lib/pages/Config.svelte
git commit -m "feat(launcher): Config takes a tab prop; locked files open read-only"
```

---

### Task 4: Shell reorg — grouped sidebar, Home default, Settings/Modules entries

**Files:**
- Modify: `launcher/src/routes/+page.svelte` (full rewrite below)

**Interfaces:**
- Consumes: `NAV`, `DEFAULT_PAGE`, `PageId` from `$lib/nav` (Task 1); `<Home />` (Task 2); `<Config tab={...} />` (Task 3).
- Produces: nothing downstream.

- [ ] **Step 1: Rewrite the shell**

Replace the entire content of `launcher/src/routes/+page.svelte` with:

```svelte
<script lang="ts">
  import { NAV, DEFAULT_PAGE, type PageId } from "$lib/nav";
  import Home from "$lib/pages/Home.svelte";
  import Library from "$lib/pages/Library.svelte";
  import Dashboard from "$lib/pages/Dashboard.svelte";
  import Items from "$lib/pages/Items.svelte";
  import Teleport from "$lib/pages/Teleport.svelte";
  import Config from "$lib/pages/Config.svelte";
  import Playerbots from "$lib/pages/Playerbots.svelte";

  let page: PageId = $state(DEFAULT_PAGE);
</script>

<main class="shell">
  <nav class="sidebar">
    <h1>DML<span>Launcher</span></h1>
    {#each NAV as s (s.section)}
      <span class="section">{s.section}</span>
      {#each s.pages as p (p.id)}
        <button class:active={page === p.id} onclick={() => (page = p.id)}>{p.label}</button>
      {/each}
    {/each}
  </nav>

  {#if page === "home"}<Home />{/if}
  {#if page === "library"}<Library />{/if}
  {#if page === "dashboard"}<Dashboard />{/if}
  {#if page === "teleport"}<Teleport />{/if}
  {#if page === "items"}<Items />{/if}
  {#if page === "playerbots"}<Playerbots />{/if}
  {#if page === "settings" || page === "modules"}
    <Config tab={page === "settings" ? "settings" : "files"} />
  {/if}
</main>

<style>
  :global(body) { margin: 0; background: #010409; color: #c9d1d9; font-family: "Segoe UI", system-ui, sans-serif; }
  .shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }
  .sidebar { background: #0d1117; border-right: 1px solid #30363d; padding: 16px 0; display: flex; flex-direction: column; gap: 2px; overflow-y: auto; }
  .sidebar h1 { font-size: 16px; margin: 0 16px 8px; color: #58a6ff; }
  .sidebar h1 span { color: #c9d1d9; font-weight: 300; margin-left: 4px; }
  .section { padding: 12px 16px 4px; font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: #6e7681; user-select: none; }
  .sidebar button { padding: 8px 16px; color: #8b949e; font-size: 14px; background: none; border: none; text-align: left; cursor: pointer; border-left: 2px solid transparent; }
  .sidebar button.active { color: #f0f6fc; background: #161b22; border-left-color: #58a6ff; }
</style>
```

Notes on intent, for the implementer:
- The single `{#if page === "settings" || page === "modules"}` block keeps ONE Config instance alive while hopping between the two entries, so unsaved edits survive the hop (spec requirement). Do not split it into two blocks.
- Section headers are plain `<span>`s — labels, not buttons.
- The keyed `{#each NAV ...}` loops and the `{#if}` chain preserve the existing shell pattern; do not introduce a router.

- [ ] **Step 2: Run the gates**

From `launcher/`:

Run: `npm run check`
Expected: 0 errors, 0 warnings.

Run: `npm test`
Expected: all suites pass (including `nav.test.ts` from Task 1).

Run: `npm run build`
Expected: vite production build succeeds.

- [ ] **Step 3: Commit**

```bash
git add launcher/src/routes/+page.svelte
git commit -m "feat(launcher): grouped Lab-style sidebar, Home default, Settings/Modules entries"
```

---

### Task 5: Copy + docs sweep, release build gate

**Files:**
- Modify: `launcher/src/lib/pages/Playerbots.svelte:99` (one string)
- Modify: `launcher/README.md` (the "## Pages" section)
- Modify: `CLAUDE.md` (repo root — the two launcher bullets that describe the sidebar and the files-tab security lock)

**Interfaces:** none (docs/copy only, plus the final build gate).

- [ ] **Step 1: Fix the Playerbots restart hint**

The old copy points at pages that changed meaning. In `launcher/src/lib/pages/Playerbots.svelte` line 99, replace:

```
— one-time setup; afterward restart the server (Library or Config) to load the scripts.
```

with:

```
— one-time setup; afterward stop and start the server (Home or Library) to load the scripts.
```

- [ ] **Step 2: Update `launcher/README.md`**

Replace the entire `## Pages` section (currently a flat 6-bullet list from "- **Library**" through the Playerbots bullet) with:

```markdown
## Pages

The sidebar is grouped into sections; entries for upcoming features appear
as they ship.

**Server**
- **Home** — landing page: world up/down card (players, uptime, update-time
  stats) plus Start/Stop for the WoW server with live terminal output.
- **Library** — install status per game, Start/Stop with live terminal output.

**Characters**
- **Dashboard** — world up/down, uptime, players online, update-time stats;
  character viewer (level, gold, equipped gear as of the last save).
- **Teleport** — pick a character and one of the ~2000 named locations
  (two-step confirm).

**Items & Bots**
- **Item Database** — search `item_template` by name/quality/level; send any
  item to a character by in-game mail.
- **Playerbots (My Party)** — auto-detects your online character and builds a
  party of playerbots: click a class to add a bot, see your group, kick or
  re-summon bots. First use shows **Enable My Party** (one-time: deploys the
  Eluna bridge scripts — then stop and start the server from Home or Library
  to load them). Requires the character online.

**Config** (Settings and Modules are one editor split across two entries; a
save on either shows the restart-needed banner on both)
- **Settings** — curated server settings (XP/gold rates, bot population, bot
  autologin, AHBot, message of the day) with safe ranges. Every setting
  except the message of the day writes an `AC_*` env var into the wow
  title's compose override (restart-to-apply); the message of the day has no
  env/conf key in this AC build, so it is instead sent over SOAP and applies
  **instantly** while the server keeps running — no restart.
- **Modules** — direct editor for the module confs (`playerbots.conf`,
  `mod_ahbot.conf`, `mod_ale.conf`, YAML/`.bak` semantics unchanged: every
  save keeps a `.bak`). `.env` and the compose override open **read-only** —
  a bad edit there could run commands on the host, so they are locked; change
  them via Settings. Save shows a restart-needed banner; Save & Restart
  streams the restart into the terminal panel.
```

- [ ] **Step 3: Update repo `CLAUDE.md`**

Two bullets in the `## launcher/` section describe the old structure. Replace this bullet:

```
- Sidebar pages Library/Dashboard/Items/Teleport/Config/Playerbots are all live (components under `launcher/src/lib/pages/`, shell in `+page.svelte`; Playerbots is `launcher/src/lib/pages/Playerbots.svelte`). The config editor writes `AC_*` env vars via `dml wow config` (registry in `cli/src/40-config.sh`) — every setting is restart-to-apply EXCEPT the message of the day, which has no env/conf key and is instead set live over SOAP while the server keeps running.
```

with:

```
- Sidebar is grouped (Server: Home/Library · Characters: Dashboard/Teleport · Items & Bots: Item Database/Playerbots · Config: Settings/Modules) — data module `launcher/src/lib/nav.ts` (vitest-pinned), shell in `+page.svelte`; **Home is the default page** (status card + start/stop). Settings and Modules are ONE `Config.svelte` mount driven by a `tab` prop (single `{#if}` block in the shell — do not split it, hopping between them must keep unsaved edits). Future Lab-parity entries (GM Tools, Summon, Backups) get added to nav.ts only when their page ships. The config editor writes `AC_*` env vars via `dml wow config` (registry in `cli/src/40-config.sh`) — every setting is restart-to-apply EXCEPT the message of the day, which has no env/conf key and is instead set live over SOAP while the server keeps running.
```

And in the `**SECURITY**` bullet, replace the phrase:

```
the Config "Advanced Files" tab can raw-**read** all 5 files but raw-**write** ONLY the 3 module confs
```

with:

```
the Modules page can raw-**read** all 5 files but raw-**write** ONLY the 3 module confs (the UI also opens the two protected names read-only — `READONLY_FILES` in Config.svelte mirrors the CLI lock)
```

Leave the rest of that bullet (the cli/src/90-main.sh enforcement description) untouched.

- [ ] **Step 4: Run the full gates + release build**

From `launcher/`:

Run: `npm run check` — Expected: 0 errors, 0 warnings.
Run: `npm test` — Expected: all suites pass.
Run: `npm run tauri build` (release-build gate from the spec; takes several minutes)
Expected: completes; NSIS/MSI bundle written under `launcher/src-tauri/target/release/bundle/`. (Unsigned — SmartScreen warning at install time is expected and fine.)

- [ ] **Step 5: Commit**

```bash
git add launcher/src/lib/pages/Playerbots.svelte launcher/README.md CLAUDE.md
git commit -m "docs(launcher): grouped sidebar + Home + Settings/Modules copy sweep"
```

Do NOT stage `launcher/src-tauri/Cargo.toml` even if it shows as modified (pre-existing EOL ghost).

---

## Post-plan user gate (not a task — controller hands this to the user)

Click-through on the built app (or `npm run tauri dev`): Home is the landing page and its status card is truthful; Start/Stop streams into the terminal and settles to the right button; every sidebar entry navigates; Settings and Modules each show their half of old Config; edit a setting on Settings, hop to Modules and back — the edit survives; a save on Settings shows the restart banner on Modules too; `.env` opens read-only with no Save button.
