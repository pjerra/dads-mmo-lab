<script lang="ts">
  import { onMount } from "svelte";
  import { NAV, DEFAULT_PAGE, CONFIG_VIEWS, BOTS_VIEWS, type PageId } from "$lib/nav";
  import {
    chipStart,
    chipStartVisible,
    serverStatus,
    startStatusPolling,
    statusLabel,
  } from "$lib/server-status.svelte";
  import { restartState } from "$lib/restart-state.svelte";
  import { installProgress } from "$lib/install-progress.svelte";
  import { initAutoShutdown } from "$lib/auto-shutdown.svelte";
  import { trayAction } from "$lib/tray-action.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { charStore, charView, setSelectedChar } from "$lib/char-store.svelte";
  import { backendStatus, wowAccounts, type Account, type CharacterSummary } from "$lib/api";
  import Home from "$lib/pages/Home.svelte";
  import Library from "$lib/pages/Library.svelte";
  import Console from "$lib/pages/Console.svelte";
  import Tools from "$lib/pages/Tools.svelte";
  import Accounts from "$lib/pages/Accounts.svelte";
  import Statistics from "$lib/pages/Statistics.svelte";
  import ModuleManager from "$lib/pages/ModuleManager.svelte";
  import Dashboard from "$lib/pages/Dashboard.svelte";
  import Items from "$lib/pages/Items.svelte";
  import Teleport from "$lib/pages/Teleport.svelte";
  import GMTools from "$lib/pages/GMTools.svelte";
  import Config from "$lib/pages/Config.svelte";
  import Bots from "$lib/pages/Bots.svelte";
  import Commands from "$lib/pages/Commands.svelte";
  import Backups from "$lib/pages/Backups.svelte";
  import Help from "$lib/pages/Help.svelte";
  import ServerRequired from "$lib/ServerRequired.svelte";
  import { requiresServer, serverGate } from "$lib/server-gate";
  import FirstRun from "$lib/FirstRun.svelte";
  import { firstRunState, firstRunNeedsProbe, type BackendStatusReport } from "$lib/first-run";
  import SoapBootstrap from "$lib/SoapBootstrap.svelte";
  import {
    soapSetupState,
    clearSoapSetup,
    dismissSoapSetup,
  } from "$lib/soap-setup-state.svelte";

  let page: PageId = $state(DEFAULT_PAGE);

  // Collapsible (accordion) sidebar: each multi-item section expands/collapses
  // independently; the section owning the current page starts open. Navigating
  // to a page always opens its section. Single-item sections (Help) render as
  // a plain link, no header.
  function sectionOf(pid: PageId): string {
    return NAV.find((s) => s.pages.some((p) => p.id === pid))?.section ?? "";
  }
  let expanded = $state<Record<string, boolean>>({ [sectionOf(DEFAULT_PAGE)]: true });
  function toggleSection(sec: string) {
    expanded[sec] = !expanded[sec];
  }
  function go(pid: PageId) {
    page = pid;
    expanded[sectionOf(pid)] = true;
    // Coming back to Home is the moment the first-run screen has to be right:
    // the user may have just installed a title in the Library. firstRunNeedsProbe
    // stops this once the machine is set up, so an established user pays one
    // probe per session, not one per Home visit.
    if (pid === "home" && firstRunNeedsProbe(backend, backendEverReady)) void probeBackend();
  }
  // The Config/Bots components take the nav page id as their `view` prop.
  const isConfigView = $derived((CONFIG_VIEWS as readonly string[]).includes(page));
  const isBotsView = $derived((BOTS_VIEWS as readonly string[]).includes(page));

  // First-run backend probe (SHIP-LIST 4.4). Owned by the shell, not by Home,
  // for two reasons: Home must not mount (and fire its own status fetches) on
  // a machine that has no server yet, and the sidebar/Settings/Help have to
  // stay reachable throughout -- a user may need to switch backend or read Help
  // BEFORE they can set anything up, so this never blocks more than the Home
  // content area.
  let backend: BackendStatusReport | null = $state(null);
  let backendErr: string | null = $state(null);
  // Session latch: once this machine has answered "ready", the first-run screen
  // is done with it. Without it a single timed-out probe would evict an
  // established user's working Home and replace it with "couldn't check".
  let backendEverReady = $state(false);
  // Re-entry guard AND the first-run screen's busy state: the retry button is
  // rendered off this, so it must be reactive. Dropping a re-click silently
  // (which is what the guard does) is only acceptable while the button visibly
  // says it is already working -- on a wedged distro the probe can run for the
  // full 120 s cold-start budget.
  let probing = $state(false);

  async function probeBackend() {
    if (probing) return;
    probing = true;
    try {
      const r = await backendStatus();
      backend = r;
      backendErr = null;
      if (r.state === "ready") backendEverReady = true;
    } catch (e) {
      // backend_status is documented never to fail for a merely unset-up
      // machine, so a rejection is the IPC itself -- which tells us nothing
      // about what is installed. firstRunState renders that as could-not-tell.
      const err = e as { message?: string };
      backendErr = err.message ?? String(e);
    } finally {
      probing = false;
    }
  }

  let firstRun = $derived(
    firstRunState({ report: backend, error: backendErr, everReady: backendEverReady }),
  );

  // Polling is idempotent (module-level flag) and lives here so it starts
  // once for the whole app regardless of which page the user lands on --
  // the status chip below must be live even when Home is never visited.
  onMount(() => {
    void probeBackend();
    startStatusPolling();
    // Re-asserts the persisted auto-shutdown toggle to the Rust watcher and
    // hooks its event channel -- idempotent, like startStatusPolling.
    initAutoShutdown();
    // Tray Start/Stop. Rust has already surfaced the window; land on Home and
    // hand the request over, so Home runs the SAME act() its own buttons do.
    void listen<string>("tray-action", (e) => {
      go("home");
      // Home is what consumes this, and on an unset-up machine Home is not
      // mounted -- the first-run screen is. Dropping the request here keeps it
      // from sitting in the store and firing an unasked-for start minutes
      // later, once setup finishes and Home finally mounts.
      if (firstRun) return;
      trayAction.pending = e.payload === "stop" ? "stop" : "start";
    });
  });

  let status = $derived(
    statusLabel(serverStatus.detail?.verdict ?? null, restartState.restarting, installProgress),
  );

  // Server-required greeting (DECIDED 2026-07-21): pages that can't function
  // with the server down are replaced by ServerRequired rather than mounted
  // and left to error. Gating here (not per page) also means the page's own
  // onMount fetches never fire while the server is down. Null = render the
  // page normally.
  let pageGate = $derived(
    requiresServer(page)
      ? serverGate(serverStatus.detail?.verdict ?? null, restartState.restarting)
      : null,
  );
  let showChipStart = $derived(
    chipStartVisible(serverStatus.detail?.verdict ?? null, restartState.restarting),
  );

  // Chip quick-start (Batch 2 F8): navigate to Home and hand it the start
  // request -- Home runs its own start flow so streaming lands in its
  // terminal exactly like a normal Start click. Deliberately NO stop
  // counterpart on the chip (accidental-click risk).
  function requestChipStart() {
    go("home");
    // Same reason as the tray handler above: Home is the consumer, and it is
    // not mounted while the first-run screen is up. Land the user on the
    // screen that tells them what is actually missing instead of queueing a
    // start for later.
    if (firstRun) return;
    chipStart.requested = true;
  }

  // Cross-page full-character-view request (smoke item 4b): Bot Browser's
  // "Open full view" sets charView.requestedName; navigating here mounts
  // Dashboard, which adopts the name and clears the request (mirrors the
  // chipStart handoff above, in the opposite direction).
  $effect(() => {
    if (charView.requestedName) go("dashboard");
  });

  // Sidebar "playing as" switcher (Batch 3 F12): minimal footer chip; the
  // dropdown fetches the same accounts/chars data CharPicker uses, on open.
  // Selecting writes the shared char store -- every mounted CharPicker then
  // follows (see CharPicker's store-adoption $effect).
  const ACTIONABLE_NAME = /^[A-Za-z0-9_]{1,12}$/;
  let charMenuOpen = $state(false);
  let charAccounts: Account[] = $state([]);
  let charMenuErr: string | null = $state(null);
  function actionableChars(chars: CharacterSummary[]): CharacterSummary[] {
    return chars.filter((c) => ACTIONABLE_NAME.test(c.name));
  }
  async function toggleCharMenu() {
    charMenuOpen = !charMenuOpen;
    if (!charMenuOpen) return;
    try {
      charAccounts = await wowAccounts();
      charMenuErr = null;
    } catch (e) {
      const err = e as { message?: string };
      charMenuErr = err.message ?? String(e);
    }
  }
  function pickChar(account: string, c: CharacterSummary) {
    setSelectedChar({ guid: c.guid, name: c.name, account });
    charMenuOpen = false;
  }
</script>

<main class="shell">
  <nav class="sidebar">
    <h1>DML<span>Launcher</span></h1>
    <div class="chip-row">
      <button
        class="status-chip"
        class:crashed={status.dot === "crash"}
        onclick={() => go("home")}
        title="Go to Home"
      >
        <span
          class="dot"
          class:on={status.dot === "on"}
          class:mid={status.dot === "mid"}
          class:bad={status.dot === "bad"}
          class:off={status.dot === "off"}
          class:crash={status.dot === "crash"}
        ></span>
        {status.label}
      </button>
      {#if showChipStart}
        <button
          class="chip-start"
          onclick={requestChipStart}
          disabled={featureLocked("chip-start")}
          title={featureLocked("chip-start") ? LOCKED_HINT : "Start the server"}
        >
          ▶
        </button>
      {/if}
    </div>
    {#if serverStatus.keepAwakeActive}
      <span class="chip-note" title="Windows sleep is blocked while the server is online (Tools → LAN play card to turn this off)">keeping PC awake</span>
    {/if}
    {#if serverStatus.lanNotice}
      <span class="chip-note lan">{serverStatus.lanNotice}</span>
    {/if}
    {#each NAV as s (s.section)}
      {#if s.pages.length === 1}
        <!-- Single-item section (Help): a plain top-level link, no dropdown. -->
        <button
          class="navitem"
          class:active={page === s.pages[0].id}
          onclick={() => go(s.pages[0].id)}>{s.pages[0].label}</button
        >
      {:else}
        <button
          class="section-head"
          class:open={expanded[s.section]}
          aria-expanded={!!expanded[s.section]}
          onclick={() => toggleSection(s.section)}
        >
          <span class="sec-caret">{expanded[s.section] ? "▾" : "▸"}</span>
          {s.section}
        </button>
        {#if expanded[s.section]}
          {#each s.pages as p (p.id)}
            <button
              class="navitem child"
              class:active={page === p.id}
              onclick={() => go(p.id)}>{p.label}</button
            >
          {/each}
        {/if}
      {/if}
    {/each}

    <!-- Persistent character switcher (Batch 3 F12): name + dropdown only. -->
    <div class="side-footer">
      {#if charMenuOpen}
        <div class="char-menu">
          {#if charMenuErr}
            <span class="char-err">Couldn't load characters: {charMenuErr}</span>
          {:else}
            {#each charAccounts as a (a.id)}
              {#each actionableChars(a.characters) as c (c.guid)}
                <button
                  class="char-row"
                  class:cursel={charStore.selected?.guid === c.guid}
                  onclick={() => pickChar(a.username, c)}
                >
                  {c.name} <span class="char-sub">lvl {c.level} · {a.username}</span>
                </button>
              {/each}
            {/each}
          {/if}
        </div>
      {/if}
      <button class="playing-chip" onclick={toggleCharMenu} title="Switch character">
        {#if charStore.selected}
          playing as <strong>{charStore.selected.name}</strong>
        {:else}
          pick a character
        {/if}
        <span class="caret">{charMenuOpen ? "▴" : "▾"}</span>
      </button>
    </div>
  </nav>

  {#if pageGate}
    <ServerRequired gate={pageGate} onstart={requestChipStart} />
  {:else if page === "home" && firstRun}
    <!-- Nothing to run yet: Home's status card would describe a server that
         does not exist. Replaces the Home CONTENT only -- the sidebar stays
         live, so Settings (backend switch) and Help remain reachable. -->
    <FirstRun
      state={firstRun}
      onnav={(p) => go(p)}
      onrecheck={() => void probeBackend()}
      rechecking={probing}
    />
  {:else}
    {#if page === "home"}<Home onnav={(p) => go(p)} />{/if}
    {#if page === "library"}<Library />{/if}
    {#if page === "console"}<Console />{/if}
    {#if page === "tools"}<Tools />{/if}
    {#if page === "accounts"}<Accounts />{/if}
    {#if page === "statistics"}<Statistics />{/if}
    {#if page === "modmanager"}<ModuleManager />{/if}
    {#if page === "dashboard"}<Dashboard />{/if}
    {#if page === "teleport"}<Teleport />{/if}
    {#if page === "gmtools"}<GMTools />{/if}
    {#if page === "items"}<Items />{/if}
    {#if isBotsView}<Bots view={page} />{/if}
    {#if page === "commands"}<Commands />{/if}
    {#if isConfigView}<Config view={page} />{/if}
    {#if page === "backups"}<Backups />{/if}
    {#if page === "help"}<Help onnav={(p) => go(p)} />{/if}
  {/if}

  {#if soapSetupState.autoResult}
    <!-- Silent by default, but not invisible: a GM3 account now exists on the
         user's server because the launcher put it there, and that is theirs to
         know. Shell-level because a native install runs for HOURS and the user
         will be on some other page when it lands -- the old card rendered only
         inside Library, which is the bug this replaces. -->
    <button
      class="soap-banner"
      onclick={clearSoapSetup}
      title="Dismiss"
    >
      Server access set up automatically as <strong>{soapSetupState.autoResult.user}</strong>.
      <span class="soap-sub">GM Tools, My Party and the console are live.</span>
    </button>
  {/if}

  {#if soapSetupState.needed && !soapSetupState.dismissed}
    <!-- The fallback, and it lives here for the same reason the banner does. A
         fallback reachable from one page only is the failure path inheriting
         the bug the success path just lost.

         DISMISSAL IS NOT RESOLUTION, and conflating them cost a regression
         once already: "Later" used to call clearSoapSetup, which drops
         `needed` -- and `needed` is now raised by exactly one thing, the
         gave_up arm, which the poll's autosetupSettled flag stops asking for.
         With Library's on-mount re-probe gone too, one click on Later put the
         step out of reach for the rest of the process. The condition stays
         INLINE rather than behind a helper because soap-surface.test.ts
         asserts the literal `soapSetupState.needed` appears in this shell. -->
    <div class="soap-fallback">
      {#if soapSetupState.gaveUpReason}
        <p class="soap-gaveup">{soapSetupState.gaveUpReason}</p>
      {/if}
      <SoapBootstrap onverified={clearSoapSetup} ondismiss={dismissSoapSetup} />
    </div>
  {/if}

  {#if serverStatus.readyToast}
    <!-- Batch 3 F10: in-app "world just came up" toast, visible from any
         page; mirrors the Windows notification the status store fires. -->
    <button class="ready-toast" onclick={() => (serverStatus.readyToast = false)} title="Dismiss">
      ⚔️ AZEROTH IS READY!
      <span class="ready-sub">The world server is up — time to play.</span>
    </button>
  {/if}
</main>

<style>
  :global(body) { margin: 0; background: #010409; color: #c9d1d9; font-family: "Segoe UI", system-ui, sans-serif; }
  /* Themed scrollbars app-wide (WebView2/Chromium): the default system bars
     are light and clash with the dark UI. A transparent track lets these sit
     cleanly on every background -- sidebar, page content, dropdowns,
     terminals, textareas and every scrollable list. */
  :global(::-webkit-scrollbar) { width: 10px; height: 10px; }
  :global(::-webkit-scrollbar-track) { background: transparent; }
  :global(::-webkit-scrollbar-thumb) { background: #30363d; border-radius: 8px; }
  :global(::-webkit-scrollbar-thumb:hover) { background: #484f58; }
  :global(::-webkit-scrollbar-corner) { background: transparent; }
  /* Text selection matches the app's accent instead of the browser default. */
  :global(::selection) { background: rgba(56, 139, 253, 0.35); color: #f0f6fc; }
  /* Dark native <select>/<option> app-wide (same spirit as the scrollbar
     theming above): WebView2's UA default renders white dropdowns, which is
     exactly what pages WITHOUT a local scoped `select` rule showed (e.g. My
     Party's Role/Class/Spec pickers). Element-level :global(select) is
     (0,0,1) specificity, so every page that already themes its selects with
     a scoped rule (`select.svelte-hash`, (0,1,1)) keeps its own look --
     this is purely the fallback for unstyled ones. <option> has no scoped
     styling anywhere, so the popup list goes dark app-wide. */
  :global(select) { background: #161b22; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; }
  :global(select:focus) { outline: none; border-color: #58a6ff; }
  :global(select:disabled) { opacity: 0.5; }
  :global(option) { background: #161b22; color: #c9d1d9; }
  /* Explicit ROWS, not columns alone. With only `grid-template-columns`, every
     shell-level block after the page (the SOAP banner, the fallback card) landed
     in a fresh IMPLICIT row that `.sidebar` did not span, and an `auto` row takes
     its height out of row 1: with the ~500px fallback card up, the sidebar grew an
     internal scrollbar, its `margin-top: auto` footer rode up onto the card's top
     edge, and column 1 beside the card painted bare body background. `1 / -1`
     spans that away only because the tracks are explicit -- with implicit rows,
     `-1` resolves to line 2 and the "span" covers row 1 alone.
     Row 1 is minmax(0, 1fr), not 1fr: `1fr` means minmax(AUTO, 1fr), and that auto
     floor is the page's full content height for any page root that is not a scroll
     container, which pushes the card off the bottom instead of shrinking the page.
     Nothing between the card and the viewport sets `overflow: hidden`, so a card
     taller than 100vh still scrolls into view on the document scrollbar.
     Two auto tracks for two surfaces even though applyAutosetupOutcome makes them
     mutually exclusive: that invariant lives in JS, an unused track costs zero
     height, and if it ever slips the second surface still lands in a spanned row
     rather than back in an implicit one. */
  .shell { display: grid; grid-template-columns: 200px 1fr; grid-template-rows: minmax(0, 1fr) auto auto; height: 100vh; }
  .sidebar { grid-column: 1; grid-row: 1 / -1; background: #0d1117; border-right: 1px solid #30363d; padding: 16px 0; display: flex; flex-direction: column; gap: 2px; overflow-y: auto; }
  .sidebar h1 { font-size: 16px; margin: 0 16px 8px; color: #58a6ff; }
  .sidebar h1 span { color: #c9d1d9; font-weight: 300; margin-left: 4px; }
  /* Always-visible live status chip (Round Q) -- sits above the nav so it's
     visible from every page, not just Home. */
  .chip-row { display: flex; gap: 6px; margin: 0 12px 12px; align-items: stretch; }
  .sidebar button.status-chip {
    display: flex;
    flex: 1;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    background: #161b22;
    border: 1px solid #30363d;
    border-left: 1px solid #30363d;
    border-radius: 6px;
    color: #c9d1d9;
    font-size: 13px;
  }
  .sidebar button.status-chip:hover { border-color: #58a6ff; }
  /* Distinct crash styling (Batch 2 F8): a crash must not read like a
     normal stop. */
  .sidebar button.status-chip.crashed { border-color: #f85149; color: #f85149; }
  /* Inline quick-start next to the chip -- start only, never stop (an
     accidental stop is costly; an accidental start is harmless). */
  .sidebar button.chip-start {
    padding: 8px 10px;
    background: #238636;
    border: 1px solid #2ea043;
    border-radius: 6px;
    color: white;
    font-size: 12px;
  }
  .sidebar button.chip-start:disabled { opacity: 0.5; cursor: default; }
  .status-chip .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .status-chip .dot.on { background: #3fb950; }
  .status-chip .dot.mid { background: #d29922; animation: chip-dot-pulse 1.4s ease-in-out infinite; }
  .status-chip .dot.off { background: #f85149; }
  .status-chip .dot.bad { background: #ffa657; }
  .status-chip .dot.crash { background: #f85149; animation: chip-dot-pulse 0.9s ease-in-out infinite; }
  @keyframes chip-dot-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  /* Small hints under the chip (keep-awake indicator, LAN refresh toast). */
  .chip-note { margin: -8px 12px 8px; padding: 0 10px; font-size: 11px; color: #6e7681; }
  .chip-note.lan { color: #3fb950; }
  /* Collapsible section header (accordion). Looks like the old section label
     but is a clickable toggle with a caret. */
  .sidebar button.section-head {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 11px 14px 5px;
    font-size: 11px;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: #6e7681;
    background: none;
    border: none;
    border-left: 2px solid transparent;
    cursor: pointer;
    user-select: none;
  }
  .sidebar button.section-head:hover { color: #b1bac4; }
  .sidebar button.section-head.open { color: #8b949e; }
  .sec-caret { font-size: 9px; width: 9px; display: inline-block; }
  /* Child nav items sit indented under an open section header. */
  .sidebar button.navitem.child { padding-left: 30px; }
  /* "Azeroth is ready" toast (Batch 3 F10): bottom-right, click to dismiss,
     auto-dismisses via the store's timer. */
  .ready-toast {
    position: fixed;
    right: 20px;
    bottom: 20px;
    z-index: 50;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 4px;
    background: #12261a;
    border: 1px solid #2ea043;
    border-radius: 10px;
    padding: 14px 18px;
    color: #3fb950;
    font-size: 16px;
    font-weight: 700;
    cursor: pointer;
    box-shadow: 0 6px 24px rgba(0, 0, 0, 0.5);
    animation: ready-pop 0.25s ease-out;
  }
  .ready-toast .ready-sub { color: #c9d1d9; font-size: 12.5px; font-weight: 400; }
  @keyframes ready-pop { from { transform: translateY(12px); opacity: 0; } to { transform: none; opacity: 1; } }
  /* grid-column on BOTH of these, because `.shell` is a grid and these are the
     first in-flow children it ever had besides the sidebar and the page. An
     auto-placed item goes into the next free cell, which is column 1 -- the
     200px sidebar track -- so the banner rendered as a 200px box under the nav
     and the fallback's two elements landed in two DIFFERENT cells (reason in
     column 1, card in column 2, side by side instead of stacked). `.ready-toast`
     escaped this only by being position: fixed. */
  .soap-banner {
    grid-column: 2;
    display: block;
    width: 100%;
    text-align: left;
    margin: 0.6rem 0 0;
    padding: 0.55rem 0.8rem;
    border: 1px solid var(--ok-fg, #8ec07c);
    border-radius: 0.4rem;
    background: rgba(142, 192, 124, 0.08);
    color: inherit;
    font: inherit;
    font-size: 0.88rem;
    cursor: pointer;
  }
  .soap-sub { display: block; opacity: 0.75; font-size: 0.8rem; }
  /* One grid item for the whole fallback, and the wrapper earns its keep twice:
     it keeps the reason directly ABOVE the card (block flow inside one cell,
     not two auto-placed cells), and it is the only handle this component has on
     the card's placement at all -- Svelte's scoped CSS stops at the component
     boundary, so `.soap-card` inside SoapBootstrap cannot be reached from
     here. */
  .soap-fallback { grid-column: 2; }
  .soap-gaveup {
    margin: 0.6rem 0 0;
    font-size: 0.88rem;
    color: var(--warn-fg, #f0c674);
  }
  .sidebar button { padding: 8px 16px; color: #8b949e; font-size: 14px; background: none; border: none; text-align: left; cursor: pointer; border-left: 2px solid transparent; }
  .sidebar button.active { color: #f0f6fc; background: #161b22; border-left-color: #58a6ff; }
  /* "playing as" footer (Batch 3 F12): pinned to the sidebar bottom. */
  .side-footer { margin-top: auto; padding: 8px 12px 0; display: flex; flex-direction: column; gap: 6px; }
  .sidebar .playing-chip {
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    padding: 7px 10px;
    font-size: 12.5px;
    color: #8b949e;
    display: flex;
    gap: 5px;
    align-items: baseline;
  }
  .sidebar .playing-chip strong { color: #f0f6fc; }
  .sidebar .playing-chip:hover { border-color: #58a6ff; }
  .caret { margin-left: auto; }
  .char-menu { display: flex; flex-direction: column; gap: 2px; max-height: 40vh; overflow-y: auto; background: #161b22; border: 1px solid #30363d; border-radius: 6px; padding: 4px; }
  .sidebar .char-menu .char-row { padding: 5px 8px; border-radius: 4px; font-size: 13px; color: #c9d1d9; display: flex; flex-direction: column; align-items: flex-start; gap: 1px; }
  .sidebar .char-menu .char-row:hover { background: #21262d; }
  .sidebar .char-menu .char-row.cursel { color: #58a6ff; }
  .char-sub { font-size: 11px; color: #6e7681; }
  .char-err { font-size: 11.5px; color: #f85149; padding: 4px 6px; }
</style>
