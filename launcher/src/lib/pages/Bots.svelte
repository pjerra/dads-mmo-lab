<script lang="ts">
  // Wraps the two bot pages as tabs of one "Bots" sidebar entry:
  //  - My Party (Playerbots): build/gear/spec your own party bots
  //  - Browse all: the ~2500-bot browser
  // Lazy {#if} rendering means the Browse tab's DB query only runs when the
  // user actually opens it; My Party is the default (the more common action).
  import Playerbots from "./Playerbots.svelte";
  import BotBrowser from "./BotBrowser.svelte";

  type BotsTab = "party" | "browse";
  let tab = $state<BotsTab>("party");
  const TABS: { id: BotsTab; label: string }[] = [
    { id: "party", label: "My Party" },
    { id: "browse", label: "Browse all bots" },
  ];
</script>

<div class="bots-wrap">
  <header class="subtabs-bar">
    <div class="subtabs" role="tablist">
      {#each TABS as t (t.id)}
        <button
          role="tab"
          aria-selected={tab === t.id}
          class:active={tab === t.id}
          onclick={() => (tab = t.id)}
        >{t.label}</button>
      {/each}
    </div>
  </header>

  <div class="bots-body">
    {#if tab === "party"}<Playerbots />{/if}
    {#if tab === "browse"}<BotBrowser />{/if}
  </div>
</div>

<style>
  .bots-wrap { display: flex; flex-direction: column; height: 100vh; min-height: 0; }
  .subtabs-bar { padding: 16px 24px 0; }
  .subtabs { display: flex; gap: 4px; flex-wrap: wrap; }
  .subtabs button {
    padding: 6px 14px;
    background: #161b22;
    border: 1px solid #30363d;
    border-radius: 6px;
    color: #8b949e;
    font-size: 13.5px;
    cursor: pointer;
  }
  .subtabs button:hover { border-color: #58a6ff; color: #c9d1d9; }
  .subtabs button.active { background: #1f2937; border-color: #58a6ff; color: #f0f6fc; }
  /* The child page owns its own scrolling; let it fill the remaining height. */
  .bots-body { flex: 1; min-height: 0; display: flex; flex-direction: column; }
  .bots-body :global(> *) { flex: 1; min-height: 0; }
</style>
