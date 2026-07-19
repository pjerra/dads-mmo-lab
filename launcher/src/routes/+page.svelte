<script lang="ts">
  import { onMount } from "svelte";
  import { NAV, DEFAULT_PAGE, type PageId } from "$lib/nav";
  import { serverStatus, startStatusPolling, statusLabel } from "$lib/server-status.svelte";
  import { restartState } from "$lib/restart-state.svelte";
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

  let page: PageId = $state(DEFAULT_PAGE);

  // Polling is idempotent (module-level flag) and lives here so it starts
  // once for the whole app regardless of which page the user lands on --
  // the status chip below must be live even when Home is never visited.
  onMount(() => {
    startStatusPolling();
  });

  let status = $derived(statusLabel(serverStatus.detail?.verdict ?? null, restartState.restarting));
</script>

<main class="shell">
  <nav class="sidebar">
    <h1>DML<span>Launcher</span></h1>
    <button
      class="status-chip"
      onclick={() => (page = "home")}
      title="Go to Home"
    >
      <span
        class="dot"
        class:on={status.dot === "on"}
        class:mid={status.dot === "mid"}
        class:bad={status.dot === "bad"}
        class:off={status.dot === "off"}
      ></span>
      {status.label}
    </button>
    {#each NAV as s (s.section)}
      <span class="section">{s.section}</span>
      {#each s.pages as p (p.id)}
        <button class:active={page === p.id} onclick={() => (page = p.id)}>{p.label}</button>
      {/each}
    {/each}
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
  {#if page === "settings" || page === "modules"}
    <Config tab={page === "settings" ? "settings" : "files"} />
  {/if}
  {#if page === "backups"}<Backups />{/if}
</main>

<style>
  :global(body) { margin: 0; background: #010409; color: #c9d1d9; font-family: "Segoe UI", system-ui, sans-serif; }
  .shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }
  .sidebar { background: #0d1117; border-right: 1px solid #30363d; padding: 16px 0; display: flex; flex-direction: column; gap: 2px; overflow-y: auto; }
  .sidebar h1 { font-size: 16px; margin: 0 16px 8px; color: #58a6ff; }
  .sidebar h1 span { color: #c9d1d9; font-weight: 300; margin-left: 4px; }
  /* Always-visible live status chip (Round Q) -- sits above the nav so it's
     visible from every page, not just Home. */
  .sidebar button.status-chip {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 12px 12px;
    padding: 8px 10px;
    background: #161b22;
    border: 1px solid #30363d;
    border-left: 1px solid #30363d;
    border-radius: 6px;
    color: #c9d1d9;
    font-size: 13px;
  }
  .sidebar button.status-chip:hover { border-color: #58a6ff; }
  .status-chip .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; flex-shrink: 0; }
  .status-chip .dot.on { background: #3fb950; }
  .status-chip .dot.mid { background: #d29922; animation: chip-dot-pulse 1.4s ease-in-out infinite; }
  .status-chip .dot.off { background: #f85149; }
  .status-chip .dot.bad { background: #ffa657; }
  @keyframes chip-dot-pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
  .section { padding: 12px 16px 4px; font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: #6e7681; user-select: none; }
  .sidebar button { padding: 8px 16px; color: #8b949e; font-size: 14px; background: none; border: none; text-align: left; cursor: pointer; border-left: 2px solid transparent; }
  .sidebar button.active { color: #f0f6fc; background: #161b22; border-left-color: #58a6ff; }
</style>
