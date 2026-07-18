<script lang="ts">
  import { onMount } from "svelte";
  import { wowCommands, type ModCommands } from "$lib/api";

  let mods: ModCommands[] = $state([]);
  let error: string | null = $state(null);
  let busy = $state(false);

  async function refresh() {
    busy = true;
    error = null;
    try {
      mods = await wowCommands();
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      busy = false;
    }
  }
  onMount(refresh);
</script>

<section class="content">
  <header class="bar">
    <h2>In-Game Commands</h2>
    <button onclick={refresh} disabled={busy}>Refresh</button>
  </header>

  {#if error}
    <div class="error-card"><strong>Couldn't load commands.</strong><p>{error}</p></div>
  {:else if mods.length === 0}
    <p class="muted">No installed mods with commands yet — install mods on the Modules page.</p>
  {:else}
    {#each mods as m (m.key)}
      <div class="card">
        <h3>{m.name}</h3>
        <pre class="cmds">{m.text}</pre>
      </div>
    {/each}
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; box-sizing: border-box; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; }
  .card h3 { margin: 0 0 8px; font-size: 15px; }
  .cmds { margin: 0; color: #c9d1d9; font-family: Consolas, monospace; font-size: 12.5px; line-height: 1.45; white-space: pre-wrap; word-break: break-word; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
