<script lang="ts">
  import { onMount } from "svelte";
  import { gamesList, gamesStart, gamesStop, type Game } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";

  let games: Game[] = $state([]);
  let loadError: string | null = $state(null);
  let busyId: string | null = $state(null);
  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  async function refresh() {
    try {
      games = await gamesList();
      loadError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      loadError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
    }
  }
  onMount(refresh);

  async function act(id: string, action: "start" | "stop") {
    busyId = id;
    showTerm = true;
    term = initialTermState();
    try {
      const run = action === "start" ? gamesStart : gamesStop;
      await run(id, (e) => {
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
      busyId = null;
      await refresh();
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Game Library</h2>
    <button onclick={refresh}>Refresh</button>
  </header>

  {#if loadError}
    <div class="error-card">
      <strong>Couldn't reach the DML backend.</strong>
      <p>{loadError}</p>
    </div>
  {:else if games.length === 0}
    <p class="muted">No games installed yet. (Install flows arrive in a later release.)</p>
  {/if}

  <div class="cards">
    {#each games as g (g.id)}
      <div class="card">
        <div class="card-title">
          <span class="dot {g.running ? 'on' : 'off'}"></span>
          {g.id}
        </div>
        <div class="card-actions">
          {#if g.running}
            <button disabled={busyId !== null} onclick={() => act(g.id, "stop")}>Stop</button>
          {:else}
            <button class="primary" disabled={busyId !== null} onclick={() => act(g.id, "start")}>
              Start
            </button>
          {/if}
        </div>
      </div>
    {/each}
  </div>

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .cards { display: flex; flex-wrap: wrap; gap: 12px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; min-width: 260px; display: flex; justify-content: space-between; align-items: center; gap: 16px; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
