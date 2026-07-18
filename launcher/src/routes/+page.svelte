<script lang="ts">
  import { NAV, DEFAULT_PAGE, type PageId } from "$lib/nav";
  import Home from "$lib/pages/Home.svelte";
  import Library from "$lib/pages/Library.svelte";
  import Console from "$lib/pages/Console.svelte";
  import Accounts from "$lib/pages/Accounts.svelte";
  import ModuleManager from "$lib/pages/ModuleManager.svelte";
  import Dashboard from "$lib/pages/Dashboard.svelte";
  import Items from "$lib/pages/Items.svelte";
  import Teleport from "$lib/pages/Teleport.svelte";
  import GMTools from "$lib/pages/GMTools.svelte";
  import Config from "$lib/pages/Config.svelte";
  import Playerbots from "$lib/pages/Playerbots.svelte";
  import Backups from "$lib/pages/Backups.svelte";

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
  {#if page === "console"}<Console />{/if}
  {#if page === "accounts"}<Accounts />{/if}
  {#if page === "modmanager"}<ModuleManager />{/if}
  {#if page === "dashboard"}<Dashboard />{/if}
  {#if page === "teleport"}<Teleport />{/if}
  {#if page === "gmtools"}<GMTools />{/if}
  {#if page === "items"}<Items />{/if}
  {#if page === "playerbots"}<Playerbots />{/if}
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
  .section { padding: 12px 16px 4px; font-size: 11px; letter-spacing: 0.08em; text-transform: uppercase; color: #6e7681; user-select: none; }
  .sidebar button { padding: 8px 16px; color: #8b949e; font-size: 14px; background: none; border: none; text-align: left; cursor: pointer; border-left: 2px solid transparent; }
  .sidebar button.active { color: #f0f6fc; background: #161b22; border-left-color: #58a6ff; }
</style>
