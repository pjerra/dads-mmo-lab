<script lang="ts">
  import { onMount } from "svelte";
  import { wowTeleportList, wowTeleport, wowTeleportCoords, type TeleLocation } from "$lib/api";
  import CharPicker from "$lib/CharPicker.svelte";
  import { featureLocked, LOCKED_HINT } from "$lib/features.svelte";

  let search = $state("");
  let locations: TeleLocation[] = $state([]);
  let loading = $state(false);
  let error: string | null = $state(null);
  let charName = $state("");
  let picked: string | null = $state(null);
  let confirming = $state(false);
  let teleporting = $state(false);
  let doneMsg: string | null = $state(null);
  let confirmingCoords = $state(false);

  // Client mirrors of the CLI's teleport-coords validators (cli/src/90-main.sh
  // _valid_coord / the inline map check) so a bad value is caught before the
  // round trip instead of surfacing as a raw BAD_ARG.
  const MAP_RE = /^[0-9]{1,3}$/;
  const COORD_RE = /^-?[0-9]{1,5}(\.[0-9]+)?$/;
  function validCoord(v: string): boolean {
    return COORD_RE.test(v) && Math.abs(Number(v)) <= 20000;
  }

  let showCoords = $state(false);
  let coordMap = $state("");
  let coordX = $state("");
  let coordY = $state("");
  let coordZ = $state("");

  // Location favorites (Batch 3 F11d): starred location names, persisted in
  // localStorage, pinned at the top of the list. Same guarded-storage idiom
  // as the Console favorites / features.svelte.ts.
  const FAVS_KEY = "dml.teleFavs";
  function readFavs(): string[] {
    try {
      if (typeof localStorage === "undefined") return [];
      const raw = localStorage.getItem(FAVS_KEY);
      if (!raw) return [];
      const arr = JSON.parse(raw);
      return Array.isArray(arr) ? arr.filter((x) => typeof x === "string") : [];
    } catch {
      return [];
    }
  }
  function writeFavs(favs: string[]): void {
    try {
      if (typeof localStorage !== "undefined") localStorage.setItem(FAVS_KEY, JSON.stringify(favs));
    } catch {
      // In-memory list still applies this session.
    }
  }
  let favs: string[] = $state(readFavs());
  function toggleFav(name: string) {
    favs = favs.includes(name) ? favs.filter((f) => f !== name) : [...favs, name];
    writeFavs(favs);
  }
  // Favorites float to the top of whatever the current filter returned;
  // within each group the CLI's own ordering is kept.
  const sortedLocations = $derived(
    [...locations].sort((a, b) => Number(favs.includes(b.name)) - Number(favs.includes(a.name))),
  );
  const mapValid = $derived(MAP_RE.test(coordMap));
  const coordsValid = $derived(mapValid && validCoord(coordX) && validCoord(coordY) && validCoord(coordZ));

  function toggleCoords() {
    showCoords = !showCoords;
    confirmingCoords = false;
    doneMsg = null;
    error = null;
  }

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
    confirmingCoords = false;
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

  async function goCoords() {
    const who = charName;
    if (!who || !coordsValid) return;
    if (!confirmingCoords) {
      confirmingCoords = true;
      return;
    }
    confirmingCoords = false;
    teleporting = true;
    error = null;
    try {
      const r = await wowTeleportCoords(who, Number(coordMap), Number(coordX), Number(coordY), Number(coordZ));
      doneMsg = `${r.char} sent to map ${r.map} at (${r.x}, ${r.y}, ${r.z}).`;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      error = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    } finally {
      teleporting = false;
      confirmingCoords = false;
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
    <CharPicker bind:selected={charName} onpick={() => { confirming = false; doneMsg = null; }} />
    {#if picked}
      <span class="muted">→ {picked}</span>
      <button
        class="primary"
        onclick={go}
        disabled={!charName || teleporting || featureLocked("teleport-named")}
        title={featureLocked("teleport-named") ? LOCKED_HINT : undefined}
      >
        {teleporting ? "Teleporting…" : confirming ? `Really send ${charName} to ${picked}?` : "Teleport"}
      </button>
    {/if}
    <button onclick={toggleCoords} disabled={teleporting}>Coordinates…</button>
  </div>

  {#if showCoords}
    <div class="card coords-card">
      <div class="row">
        <label class="field">Map<input class="coord map" bind:value={coordMap} oninput={() => (confirmingCoords = false)} disabled={teleporting} /></label>
        <label class="field">X<input class="coord" bind:value={coordX} oninput={() => (confirmingCoords = false)} disabled={teleporting} /></label>
        <label class="field">Y<input class="coord" bind:value={coordY} oninput={() => (confirmingCoords = false)} disabled={teleporting} /></label>
        <label class="field">Z<input class="coord" bind:value={coordZ} oninput={() => (confirmingCoords = false)} disabled={teleporting} /></label>
        <button
          class="primary"
          onclick={goCoords}
          disabled={!charName || !coordsValid || teleporting || featureLocked("teleport-coords")}
          title={featureLocked("teleport-coords") ? LOCKED_HINT : undefined}
        >
          {teleporting ? "Teleporting…" : confirmingCoords ? `Overwrite ${charName}'s saved position?` : "Teleport"}
        </button>
      </div>
      <p class="muted">Character must be logged out.</p>
    </div>
  {/if}

  {#if error}<div class="error-card"><p>{error}</p></div>{/if}
  {#if doneMsg}<div class="ok-card"><p>{doneMsg}</p></div>{/if}
  {#if locations.length === 500}
    <p class="muted">Showing the first 500 — narrow the filter to see the rest.</p>
  {/if}

  <div class="loclist">
    {#if loading}
      <p class="muted">Loading locations…</p>
    {:else if sortedLocations.length === 0}
      <p class="muted">No locations match the filter.</p>
    {:else}
      {#each sortedLocations as l (l.name)}
      <span class="locrow" class:sel={picked === l.name}>
        <button class="loc" onclick={() => pick(l.name)} disabled={teleporting}>
          {l.name} <span class="muted">map {l.map}</span>
        </button>
        <button
          class="star"
          class:faved={favs.includes(l.name)}
          onclick={() => toggleFav(l.name)}
          title={favs.includes(l.name) ? "Remove from favorites" : "Pin to the top as a favorite"}
        >
          {favs.includes(l.name) ? "★" : "☆"}
        </button>
      </span>
      {/each}
    {/if}
  </div>
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 14px; }
  .bar h2 { margin: 0; font-size: 18px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  input { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 8px; min-width: 240px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 12px 16px; }
  .coords-card .row { align-items: flex-end; }
  .field { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: #8b949e; }
  .field input.coord { min-width: 90px; width: 90px; }
  .field input.coord.map { width: 60px; }
  .loclist { display: flex; flex-wrap: wrap; gap: 6px; }
  .locrow { display: inline-flex; align-items: stretch; border: 1px solid #30363d; border-radius: 6px; background: #0d1117; overflow: hidden; }
  .locrow.sel { border-color: #58a6ff; }
  .loc { background: none; border: none; padding: 5px 4px 5px 10px; color: #c9d1d9; cursor: pointer; font-size: 13px; }
  .locrow.sel .loc { color: #f0f6fc; }
  .star { background: none; border: none; padding: 0 8px 0 2px; color: #6e7681; cursor: pointer; font-size: 13px; }
  .star.faved { color: #d29922; }
  .star:hover { color: #d29922; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; font-size: 13px; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
  .ok-card { background: #161b22; border: 1px solid #2ea043; border-radius: 8px; padding: 12px 16px; }
</style>
