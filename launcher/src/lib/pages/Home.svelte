<script lang="ts">
  import { onMount } from "svelte";
  import { wowServerInfo, gamesStatus, gamesStart, gamesStop, type ServerInfo } from "$lib/api";
  import { applyEvent, initialTermState, type TermState } from "$lib/terminal-state";
  import Terminal from "$lib/Terminal.svelte";

  const WOW_ID = "wow-server-playerbots";

  let info: ServerInfo | null = $state(null);
  let infoError: string | null = $state(null);
  let containerState: "running" | "stopped" | null = $state(null);
  let statusError: string | null = $state(null);
  let refreshing = $state(false);

  let busy = $state(false);
  let term: TermState = $state(initialTermState());
  let showTerm = $state(false);

  async function refresh() {
    refreshing = true;
    try {
      containerState = (await gamesStatus(WOW_ID)).state;
      statusError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      statusError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      containerState = null;
    }
    try {
      info = await wowServerInfo();
      infoError = null;
    } catch (e) {
      const err = e as { message?: string; hint?: string };
      infoError = `${err.message ?? String(e)}${err.hint ? ` — ${err.hint}` : ""}`;
      info = null;
    } finally {
      refreshing = false;
    }
  }
  onMount(refresh);

  async function act(action: "start" | "stop") {
    busy = true;
    showTerm = true;
    term = initialTermState();
    try {
      const run = action === "start" ? gamesStart : gamesStop;
      await run(WOW_ID, (e) => {
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
      busy = false;
      await refresh();
    }
  }
</script>

<section class="content">
  <header class="bar">
    <h2>Home</h2>
    <button onclick={refresh} disabled={refreshing || busy}>Refresh</button>
  </header>

  {#if infoError}
    <div class="error-card"><strong>Couldn't read world status.</strong><p>{infoError}</p></div>
  {:else if info}
    <div class="card">
      <div class="card-title">
        <span class="dot {info.online ? 'on' : 'off'}"></span>
        <strong>{info.online ? "World is up" : "World is down"}</strong>
      </div>
      {#if info.online}
        <div class="stats">
          <span>Players online: <strong>{info.players ?? "?"}</strong></span>
          <span>Uptime: <strong>{info.uptime ?? "?"}</strong></span>
          <span>Update time: <strong>{info.mean_ms ?? "?"} ms avg</strong></span>
        </div>
      {:else}
        <p class="muted">Start the server below.</p>
      {/if}
    </div>
  {/if}

  <header class="bar"><h2>WoW server</h2></header>
  {#if statusError}
    <div class="error-card"><strong>Couldn't reach the DML backend.</strong><p>{statusError}</p></div>
  {:else if containerState}
    <div class="card">
      <div class="card-title">
        <span class="dot {containerState === 'running' ? 'on' : 'off'}"></span>
        {WOW_ID}
      </div>
      <div>
        {#if containerState === "running"}
          <button disabled={busy} onclick={() => act("stop")}>Stop</button>
        {:else}
          <button class="primary" disabled={busy} onclick={() => act("start")}>Start</button>
        {/if}
      </div>
    </div>
  {/if}

  {#if showTerm}
    <Terminal state={term} />
  {/if}
</section>

<style>
  .content { padding: 20px 24px; overflow-y: auto; display: flex; flex-direction: column; gap: 16px; }
  .bar { display: flex; justify-content: space-between; align-items: center; }
  .bar h2 { margin: 0; font-size: 18px; }
  .card { background: #0d1117; border: 1px solid #30363d; border-radius: 8px; padding: 14px 16px; display: flex; justify-content: space-between; align-items: center; gap: 16px; flex-wrap: wrap; }
  .card-title { display: flex; align-items: center; gap: 8px; font-weight: 600; }
  .dot { width: 9px; height: 9px; border-radius: 50%; display: inline-block; }
  .dot.on { background: #3fb950; }
  .dot.off { background: #6e7681; }
  .stats { display: flex; gap: 18px; flex-wrap: wrap; }
  button { background: #21262d; color: #c9d1d9; border: 1px solid #30363d; border-radius: 6px; padding: 6px 14px; cursor: pointer; }
  button.primary { background: #238636; border-color: #2ea043; color: white; }
  button:disabled { opacity: 0.5; cursor: default; }
  .muted { color: #8b949e; margin: 0; }
  .error-card { background: #161b22; border: 1px solid #f85149; border-radius: 8px; padding: 12px 16px; }
</style>
