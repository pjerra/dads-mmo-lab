<script lang="ts">
  import { onMount } from "svelte";
  import { wowTeleportList, wowTeleport, type TeleLocation } from "$lib/api";
  import CharPicker from "$lib/CharPicker.svelte";

  let search = $state("");
  let locations: TeleLocation[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let charName = $state("");
  let picked: string | null = $state(null);
  let confirming = $state(false);
  let teleporting = $state(false);
  let doneMsg: string | null = $state(null);

  async function load() {
    loading = true;
    error = null;
    try {
      locations = await wowTeleportList(search.trim() || undefined);
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      loading = false;
    }
  }
  onMount(load);

  function pick(name: string) {
    picked = name;
    confirming = false;
    doneMsg = null;
  }

  async function go() {
    const who = charName;
    const dest = picked;
    if (!dest || !who) return;
    if (!confirming) {
      confirming = true;
      return;
    }
    teleporting = true;
    error = null;
    try {
      const r = await wowTeleport(who, dest);
      doneMsg = `${r.char} sent to ${r.to}.`;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      teleporting = false;
      confirming = false;
    }
  }
</script>

<section class="content">
  <header class="bar"><h2>Teleport</h2></header>

  <form class="row" onsubmit={(e) => { e.preventDefault(); load(); }}>
    <input placeholder="Filter locations…" bind:value={search} disabled={loading || teleporting} />
    <button type="submit" disabled={loading || teleporting}>Filter</button>
  </form>

  <div class="row">
    <span class="muted">Who:</span>
    <CharPicker bind:selected={charName} />
    {#if picked}
      <span class="muted">→ {picked}</span>
      <button class="primary" onclick={go} disabled={!charName || teleporting}>
        {teleporting ? "Teleporting…" : confirming ? `Really send ${charName} to ${picked}?` : "Teleport"}
      </button>
    {/if}
  </div>

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if doneMsg}<div class="ok-card"><p>{doneMsg}</p></div>{/if}
  {#if locations.length === 500}
    <p class="muted">Showing the first 500 — narrow the filter to see the rest.</p>
  {/if}

  <div class="loclist">
    {#each locations as l (l.name)}
      <button class="loc" class:sel={picked === l.name} onclick={() => pick(l.name)} disabled={teleporting}>
        {l.name} <span class="muted">map {l.map}</span>
      </button>
    {/each}
  </div>
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; }
  .bar h2 { margin: 0; font-size: 18px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; min-width: 240px; }
  .loclist { display: flex; flex-wrap: wrap; gap: 6px; }
  .loc { background: #0d1117; border: 1px solid #30363d; border-radius: 6px; padding: 5px 10px; color: #c9d1d9; cursor: pointer; font-size: 13px; }
  .loc.sel { border-color: #58a6ff; color: #f0f6fc; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .ok-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
</style>
