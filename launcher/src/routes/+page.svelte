<script lang="ts">
  import Library from "$lib/pages/Library.svelte";
  import Dashboard from "$lib/pages/Dashboard.svelte";
  import Items from "$lib/pages/Items.svelte";
  import Teleport from "$lib/pages/Teleport.svelte";
  import Config from "$lib/pages/Config.svelte";

  const PAGES = [
    { id: "library", label: "Library" },
    { id: "dashboard", label: "Dashboard" },
    { id: "items", label: "Item Database" },
    { id: "teleport", label: "Teleport" },
    { id: "config", label: "Config" },
  ] as const;
  type PageId = (typeof PAGES)[number]["id"];
  let page: PageId = $state("library");
</script>

<main class="shell">
  <nav class="sidebar">
    <h1>DML<span>Launcher</span></h1>
    {#each PAGES as p (p.id)}
      <button class:active={page === p.id} onclick={() => (page = p.id)}>{p.label}</button>
    {/each}
    <button class="disabled" disabled title="Coming with My Party">Playerbots</button>
  </nav>

  {#if page === "library"}<Library />{/if}
  {#if page === "dashboard"}<Dashboard />{/if}
  {#if page === "items"}<Items />{/if}
  {#if page === "teleport"}<Teleport />{/if}
  {#if page === "config"}<Config />{/if}
</main>

<style>
  :global(body) { margin: 0; background: #010409; color: #c9d1d9; font-family: "Segoe UI", system-ui, sans-serif; }
  .shell { display: grid; grid-template-columns: 200px 1fr; height: 100vh; }
  .sidebar { background: #0d1117; border-right: 1px solid #30363d; padding: 16px 0; display: flex; flex-direction: column; gap: 2px; }
  .sidebar h1 { font-size: 16px; margin: 0 16px 14px; color: #58a6ff; }
  .sidebar h1 span { color: #c9d1d9; font-weight: 300; margin-left: 4px; }
  .sidebar button { padding: 8px 16px; color: #8b949e; font-size: 14px; background: none; border: none; text-align: left; cursor: pointer; border-left: 2px solid transparent; }
  .sidebar button.active { color: #f0f6fc; background: #161b22; border-left-color: #58a6ff; }
  .sidebar button.disabled { opacity: 0.35; cursor: default; }
</style>
