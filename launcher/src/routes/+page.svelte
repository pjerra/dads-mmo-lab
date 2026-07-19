<script lang="ts">
  import { onMount } from "svelte";
  import { NAV, DEFAULT_PAGE, type PageId } from "$lib/nav";
  import {
    chipStart,
    chipStartVisible,
    serverStatus,
    startStatusPolling,
    statusLabel,
  } from "$lib/server-status.svelte";
  import { restartState } from "$lib/restart-state.svelte";
  import { initAutoShutdown } from "$lib/auto-shutdown.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";
  import { charStore, setSelectedChar } from "$lib/char-store.svelte";
  import { wowAccounts, type Account, type CharacterSummary } from "$lib/api";
  import Home from "$lib/pages/Home.svelte";
  import Library from "$lib/pages/Library.svelte";
  import Console from "$lib/pages/Console.svelte";
  import Tools from "$lib/pages/Tools.svelte";
  import Accounts from "$lib/pages/Accounts.svelte";
  import ModuleManager from "$lib/pages/ModuleManager.svelte";
  import Dashboard from "$lib/pages/Dashboard.svelte";
  import Items from "$lib/pages/Items.svelte";
  import Teleport from "$lib/pages/Teleport.svelte";
  import GMTools from "$lib/pages/GMTools.svelte";
  import Config from "$lib/pages/Config.svelte";
  import Playerbots from "$lib/pages/Playerbots.svelte";
  import Commands from "$lib/pages/Commands.svelte";
  import Backups from "$lib/pages/Backups.svelte";
  import Help from "$lib/pages/Help.svelte";

  let page: PageId = $state(DEFAULT_PAGE);

  // Polling is idempotent (module-level flag) and lives here so it starts
  // once for the whole app regardless of which page the user lands on --
  // the status chip below must be live even when Home is never visited.
  onMount(() => {
    startStatusPolling();
    // Re-asserts the persisted auto-shutdown toggle to the Rust watcher and
    // hooks its event channel -- idempotent, like startStatusPolling.
    initAutoShutdown();
  });

  let status = $derived(statusLabel(serverStatus.detail?.verdict ?? null, restartState.restarting));
  let showChipStart = $derived(
    chipStartVisible(serverStatus.detail?.verdict ?? null, restartState.restarting),
  );

  // Chip quick-start (Batch 2 F8): navigate to Home and hand it the start
  // request -- Home runs its own start flow so streaming lands in its
  // terminal exactly like a normal Start click. Deliberately NO stop
  // counterpart on the chip (accidental-click risk).
  function requestChipStart() {
    chipStart.requested = true;
    page = "home";
  }

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
        onclick={() => (page = "home")}
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
      <span class="section">{s.section}</span>
      {#each s.pages as p (p.id)}
        <button class:active={page === p.id} onclick={() => (page = p.id)}>{p.label}</button>
      {/each}
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

  {#if page === "home"}<Home />{/if}
  {#if page === "library"}<Library />{/if}
  {#if page === "console"}<Console />{/if}
  {#if page === "tools"}<Tools />{/if}
  {#if page === "accounts"}<Accounts />{/if}
  {#if page === "modmanager"}<ModuleManager />{/if}
  {#if page === "dashboard"}<Dashboard />{/if}
  {#if page === "teleport"}<Teleport />{/if}
  {#if page === "gmtools"}<GMTools />{/if}
  {#if page === "items"}<Items />{/if}
  {#if page === "playerbots"}<Playerbots />{/if}
  {#if page === "commands"}<Commands />{/if}
  {#if page === "settings" || page === "modules" || page === "botworld" || page === "ahbot"}
    <Config
      tab={page === "settings"
        ? "settings"
        : page === "botworld"
          ? "botworld"
          : page === "ahbot"
            ? "ahbot"
            : "files"}
    />
  {/if}
  {#if page === "backups"}<Backups />{/if}
  {#if page === "help"}<Help onnav={(p) => (page = p)} />{/if}

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
  .shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }
  .sidebar { background: #0d1117; border-right: 1px solid #30363d; padding: 16px 0; display: flex; flex-direction: column; gap: 2px; overflow-y: auto; }
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
  .section { padding: 12px 16px 4px; font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: #6e7681; user-select: none; }
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
